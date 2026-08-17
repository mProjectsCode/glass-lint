//! Small test-only logical/physical equivalence oracle.
//!
//! The oracle intentionally covers only the compiler vocabulary exercised by
//! this equivalence suite: direct events, constrained arguments, returned
//! objects, alternatives, and one representative lifecycle shape. Runtime
//! flow and the remaining physical roots have their own integration and
//! physical-plan tests; duplicating those interpreters here would make this
//! test seam another semantic implementation to maintain.

#![cfg(test)]

use std::collections::BTreeMap;

use crate::api::{
    compiler::{
        normalized::{
            CanonicalArgumentConstraints, NormalizedEvent, NormalizedLifecycle,
            NormalizedLifecycleCondition, NormalizedLifecycleEvent, NormalizedLifecycleSink,
            NormalizedQuery, NormalizedRoot, NormalizedSubject,
        },
        object_flow::CompiledObjectFlow,
        physical::{PhysicalPlan, PhysicalRoot},
        rule::IdentityConstraint,
    },
    rule::{
        ArgumentConstraint, ArgumentIndex, ArgumentMatcher, ArgumentMatcherKind,
        StaticStringPredicateKind, ValueMatcher, ValueMatcherKind,
        query::{EventSpec, IdentitySpec, lifecycle::LifecycleCallTarget},
    },
};

/// A single event row in the synthetic relation store.
#[derive(Debug, Clone)]
pub(crate) struct ReferenceRow {
    pub event: u32,
    pub event_kind: EventSpec,
    pub identity: IdentitySpec,
    pub arguments: BTreeMap<ArgumentIndex, ReferenceValue>,
    pub object: Option<u32>,
    pub support: Option<ReferenceSupport>,
    pub path: u32,
    pub completeness: ReferenceCompleteness,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReferenceSupport {
    pub event: u32,
    pub path: u32,
    pub kind: ReferenceSupportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReferenceSupportKind {
    Producer,
}

/// A value at a specific argument position.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReferenceValue {
    StaticString(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReferenceCompleteness {
    Complete,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReferenceWitness {
    pub primary_event: u32,
    pub support_events: Vec<u32>,
    pub path_key: u32,
    pub certainty: ReferenceCertainty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReferenceCertainty {
    Definite,
    Possible,
}

#[derive(Clone)]
struct LifecycleSourceMatcher {
    target: LifecycleCallTarget,
    arguments: Vec<ArgumentConstraint>,
}

struct LifecycleReferencePlan {
    source: LifecycleSourceMatcher,
    property: glass_lint_datastructures::SymbolPath,
    property_value: ValueMatcher,
    sink: LifecycleSinkMatcher,
}

struct LifecycleSinkMatcher {
    target: LifecycleCallTarget,
    argument: usize,
}

/// Convert the one lifecycle shape used by the equivalence test into its
/// deliberately smaller reference representation. Unsupported lifecycle
/// combinations are covered by the real flow tests, not silently added here.
fn lifecycle_plan_from_normalized(
    lifecycle: &NormalizedLifecycle,
) -> Option<LifecycleReferencePlan> {
    let source = lifecycle.sources().first()?;
    let source_target = lifecycle_target(source.event(), source.identity())?;
    let condition = lifecycle.condition()?;
    let events = match condition {
        NormalizedLifecycleCondition::AnyOf(events)
        | NormalizedLifecycleCondition::AllOf(events) => events,
    };
    let [NormalizedLifecycleEvent::PropertyWrite { property, value }] = events.as_ref() else {
        return None;
    };
    let completion = lifecycle.completion();
    let crate::api::compiler::normalized::NormalizedLifecycleCompletion::AnySink(sinks) =
        completion
    else {
        return None;
    };
    let [sink] = sinks.as_ref() else {
        return None;
    };
    let NormalizedLifecycleSink::ArgumentOf { target, index } = sink else {
        return None;
    };
    Some(LifecycleReferencePlan {
        source: LifecycleSourceMatcher {
            target: source_target,
            arguments: source.arguments().to_flat_vec(),
        },
        property: property.clone().into(),
        property_value: value.clone(),
        sink: LifecycleSinkMatcher {
            target: target.clone(),
            argument: *index,
        },
    })
}

fn lifecycle_plan_from_physical(flow: &CompiledObjectFlow) -> Option<LifecycleReferencePlan> {
    let source = flow.sources().next()?;
    let requirement = flow.requirements().next()?;
    let (property, property_value) = requirement.property_write()?;
    let sink = flow.sinks().next()?;
    let argument = sink.fixed_argument()?;
    if flow.sources().nth(1).is_some()
        || flow.requirements().nth(1).is_some()
        || flow.sinks().nth(1).is_some()
    {
        return None;
    }
    Some(LifecycleReferencePlan {
        source: LifecycleSourceMatcher {
            target: source.target().clone(),
            arguments: source.argument_constraints().to_flat_vec(),
        },
        property: property.clone().into(),
        property_value: property_value.clone(),
        sink: LifecycleSinkMatcher {
            target: sink.target().clone(),
            argument,
        },
    })
}

fn lifecycle_target(event: &EventSpec, identity: &IdentitySpec) -> Option<LifecycleCallTarget> {
    match (event, identity) {
        (EventSpec::Call, IdentitySpec::Global { name }) => {
            Some(LifecycleCallTarget::Global(name.clone()))
        }
        _ => None,
    }
}

/// Evaluate a normalized query over the synthetic rows.
pub(crate) fn evaluate_supported_logical(
    query: &NormalizedQuery,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut witnesses = evaluate_root_logical(query.root(), rows);
    witnesses.sort();
    witnesses.dedup();
    witnesses
}

fn evaluate_root_logical(root: &NormalizedRoot, rows: &[ReferenceRow]) -> Vec<ReferenceWitness> {
    match root {
        NormalizedRoot::Event(event) => evaluate_event_logical(event, rows),
        NormalizedRoot::Any(branches) => branches
            .iter()
            .flat_map(|branch| evaluate_root_logical(branch, rows))
            .collect(),
        NormalizedRoot::Lifecycle(lifecycle) => lifecycle_plan_from_normalized(lifecycle)
            .map_or_else(Vec::new, |plan| evaluate_lifecycle(&plan, rows)),
    }
}

fn evaluate_event_logical(event: &NormalizedEvent, rows: &[ReferenceRow]) -> Vec<ReferenceWitness> {
    let (identity, requires_producer) = match event.subject() {
        NormalizedSubject::Direct { identity } => (identity, false),
        NormalizedSubject::Returned { producer, .. } => (producer, true),
        NormalizedSubject::Instance { .. } => return Vec::new(),
    };
    rows.iter()
        .filter(|row| {
            matches_event_kind(event.event(), &row.event_kind)
                && matches_identity(identity, &row.identity)
                && matches_arguments(event.arguments(), &row.arguments)
                && (!requires_producer || has_producer_support(row))
        })
        .map(|row| ReferenceWitness {
            primary_event: row.event,
            support_events: row
                .support
                .as_ref()
                .filter(|_| requires_producer)
                .map(|support| vec![support.event])
                .unwrap_or_default(),
            path_key: row.path,
            certainty: certainty(row),
        })
        .collect()
}

fn evaluate_lifecycle(
    plan: &LifecycleReferencePlan,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut by_path: BTreeMap<u32, Vec<&ReferenceRow>> = BTreeMap::new();
    for row in rows {
        by_path.entry(row.path).or_default().push(row);
    }

    let mut witnesses = Vec::new();
    for (path, mut path_rows) in by_path {
        path_rows.sort_by_key(|row| row.event);
        for source in path_rows
            .iter()
            .copied()
            .filter(|row| matches_source(&plan.source, row))
        {
            let Some(condition) = path_rows.iter().copied().find(|row| {
                row.event > source.event
                    && matches!(&row.event_kind, EventSpec::PropertyWrite { property } if property == &plan.property)
                    && row
                        .arguments
                        .get(&ArgumentIndex::new_unchecked(0))
                        .is_some_and(|value| matches_value_matcher(&plan.property_value, value))
            }) else {
                continue;
            };
            let Some(sink) = path_rows
                .iter()
                .copied()
                .find(|row| row.event > condition.event && matches_sink(&plan.sink, row))
            else {
                continue;
            };
            witnesses.push(ReferenceWitness {
                primary_event: sink.event,
                support_events: vec![source.event],
                path_key: path,
                certainty: if [source, condition, sink]
                    .into_iter()
                    .any(|row| row.completeness == ReferenceCompleteness::Unknown)
                {
                    ReferenceCertainty::Possible
                } else {
                    ReferenceCertainty::Definite
                },
            });
        }
    }
    witnesses
}

fn matches_source(matcher: &LifecycleSourceMatcher, row: &ReferenceRow) -> bool {
    matches_target(&matcher.target, row)
        && matches_flat_arguments(&matcher.arguments, &row.arguments)
}

fn matches_sink(matcher: &LifecycleSinkMatcher, row: &ReferenceRow) -> bool {
    matches_target(&matcher.target, row)
        && u8::try_from(matcher.argument).is_ok_and(|index| {
            row.arguments
                .contains_key(&ArgumentIndex::new_unchecked(index))
        })
}

fn matches_target(target: &LifecycleCallTarget, row: &ReferenceRow) -> bool {
    matches!(target, LifecycleCallTarget::Global(name)
        if matches!(&row.event_kind, EventSpec::Call)
            && matches!(&row.identity, IdentitySpec::Global { name: actual } if actual == name))
}

fn matches_flat_arguments(
    constraints: &[ArgumentConstraint],
    args: &BTreeMap<ArgumentIndex, ReferenceValue>,
) -> bool {
    constraints.iter().all(|constraint| {
        args.get(&constraint.arg_index())
            .is_some_and(|value| matches_argument_matcher(constraint.predicate(), value))
    })
}

fn matches_value_matcher(matcher: &ValueMatcher, value: &ReferenceValue) -> bool {
    match matcher.kind() {
        ValueMatcherKind::Any => true,
        ValueMatcherKind::StaticString(predicate) => match value {
            ReferenceValue::StaticString(actual) => match predicate.kind() {
                StaticStringPredicateKind::Any => true,
                StaticStringPredicateKind::Exact(values) => values.iter().any(|v| v == actual),
                StaticStringPredicateKind::Prefix(values) => {
                    values.iter().any(|v| actual.starts_with(v))
                }
                StaticStringPredicateKind::ContainsAny(values) => {
                    values.iter().any(|v| actual.contains(v))
                }
                StaticStringPredicateKind::ContainsAll(values) => {
                    values.iter().all(|v| actual.contains(v))
                }
            },
        },
    }
}

fn matches_event_kind(expected: &EventSpec, actual: &EventSpec) -> bool {
    expected == actual
}

fn matches_identity(expected: &IdentitySpec, actual: &IdentitySpec) -> bool {
    expected == actual
}

fn matches_arguments(
    constraints: &CanonicalArgumentConstraints,
    args: &BTreeMap<ArgumentIndex, ReferenceValue>,
) -> bool {
    constraints.groups().iter().all(|group| {
        args.get(&group.index()).is_some_and(|value| {
            group
                .predicates()
                .iter()
                .all(|matcher| matches_argument_matcher(matcher, value))
        })
    })
}

fn certainty(row: &ReferenceRow) -> ReferenceCertainty {
    match row.completeness {
        ReferenceCompleteness::Complete => ReferenceCertainty::Definite,
        ReferenceCompleteness::Unknown => ReferenceCertainty::Possible,
    }
}

fn has_producer_support(row: &ReferenceRow) -> bool {
    row.object.is_some_and(|_| {
        row.support.as_ref().is_some_and(|support| {
            support.path == row.path
                && support.event != row.event
                && support.kind == ReferenceSupportKind::Producer
        })
    })
}

/// Evaluate the supported physical roots against the same synthetic rows.
pub(crate) fn evaluate_supported_physical(
    plan: &PhysicalPlan,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut witnesses = plan
        .roots()
        .iter()
        .flat_map(|root| evaluate_physical_root(root, rows))
        .collect::<Vec<_>>();
    witnesses.sort();
    witnesses.dedup();
    witnesses
}

fn evaluate_physical_root(root: &PhysicalRoot, rows: &[ReferenceRow]) -> Vec<ReferenceWitness> {
    match root {
        PhysicalRoot::IndexedScan {
            identity, event, ..
        } => evaluate_scan(identity, event, None, rows),
        PhysicalRoot::ConstrainedScan {
            identity,
            event,
            constraints,
            ..
        } => evaluate_scan(identity, event, Some(constraints), rows),
        PhysicalRoot::ReturnedSubject {
            producer, event, ..
        } => evaluate_scan(producer, event, None, rows)
            .into_iter()
            .filter_map(|mut witness| {
                let row = rows.iter().find(|row| {
                    row.event == witness.primary_event && row.path == witness.path_key
                })?;
                if !has_producer_support(row) {
                    return None;
                }
                witness.support_events = vec![row.support.as_ref()?.event];
                Some(witness)
            })
            .collect(),
        PhysicalRoot::InstanceSubject { .. } | PhysicalRoot::Lifecycle { .. } => match root {
            PhysicalRoot::Lifecycle { flow } => lifecycle_plan_from_physical(flow)
                .map_or_else(Vec::new, |plan| evaluate_lifecycle(&plan, rows)),
            PhysicalRoot::InstanceSubject { .. } => Vec::new(),
            _ => unreachable!(),
        },
    }
}

fn evaluate_scan(
    identity: &IdentityConstraint,
    event: &EventSpec,
    constraints: Option<&CanonicalArgumentConstraints>,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    rows.iter()
        .filter(|row| {
            row.event_kind == *event
                && IdentityConstraint::from(&row.identity) == *identity
                && constraints
                    .is_none_or(|constraints| matches_arguments(constraints, &row.arguments))
        })
        .map(|row| ReferenceWitness {
            primary_event: row.event,
            support_events: Vec::new(),
            path_key: row.path,
            certainty: certainty(row),
        })
        .collect()
}

fn matches_argument_matcher(matcher: &ArgumentMatcher, value: &ReferenceValue) -> bool {
    match matcher.kind() {
        ArgumentMatcherKind::Value(matcher) => matches_value_matcher(matcher, value),
        ArgumentMatcherKind::ObjectKeys(_)
        | ArgumentMatcherKind::ObjectPropertyValue { .. }
        | ArgumentMatcherKind::RootedExpressions(_) => false,
    }
}
