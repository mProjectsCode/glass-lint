//! Test-only logical/physical equivalence oracle.
//!
//! Provides a small synthetic relation store and two evaluators
//! (`evaluate_supported_logical` and `evaluate_supported_physical`) that
//! produce deterministic witnesses. The oracle compares the sorted witness
//! lists to verify that the physical planner has the same semantics as the
//! logical query over the supported event subset.

#![cfg(test)]

use std::collections::BTreeMap;

use crate::api::{
    compiler::{
        normalized::{
            CanonicalArgumentConstraints, NormalizedEvent, NormalizedLifecycle,
            NormalizedLifecycleCompletion, NormalizedLifecycleCondition, NormalizedLifecycleEvent,
            NormalizedLifecycleSink, NormalizedQuery, NormalizedRoot, NormalizedSubject,
        },
        object_flow::{CompiledObjectFlow, CompletionMode, RequirementMode},
        physical::{PhysicalPlan, PhysicalRoot},
        rule::{EventPredicate, IdentityConstraint, lower_event, lower_identity},
    },
    rule::{
        ArgumentConstraint, ArgumentIndex, ArgumentMatcher, ArgumentMatcherKind,
        StaticStringPredicateKind, ValueMatcher, ValueMatcherKind,
        query::{EventSpec, IdentitySpec, lifecycle::LifecycleCallTarget},
    },
};

// ── Reference row types ─────────────────────────────────────────────────────

/// A single event row in the synthetic relation store.
#[derive(Debug, Clone)]
pub(crate) struct ReferenceRow {
    /// Unique event identifier.
    pub event: u32,
    /// Kind of event (Call, Construct, MemberCall, etc.).
    pub event_kind: EventSpec,
    /// Identity (Global, Rooted, ModuleExport, etc.).
    pub identity: IdentitySpec,
    /// Argument values keyed by positional index.
    pub arguments: BTreeMap<ArgumentIndex, ReferenceValue>,
    /// Correlated object ID, if applicable.
    pub object: Option<u32>,
    /// The producer or constructor event that established the object
    /// correlation, together with its path key.
    pub support: Option<ReferenceSupport>,
    /// Correlation path key.
    pub path: u32,
    /// Completeness status.
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
    Constructor,
}

/// A value at a specific argument position.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReferenceValue {
    /// Known static string value.
    StaticString(String),
    /// Unknown or dynamic value.
    #[allow(dead_code)]
    Unknown,
}

/// Completeness of analysis for a reference row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReferenceCompleteness {
    Complete,
    Unknown,
}

// ── Witness types ───────────────────────────────────────────────────────────

/// A witness produced by evaluating a query or plan against reference rows.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReferenceWitness {
    /// Primary event key.
    pub primary_event: u32,
    /// Supporting event keys (producer, constructor, etc.).
    pub support_events: Vec<u32>,
    /// Correlation path key.
    pub path_key: u32,
    /// Witness certainty.
    pub certainty: ReferenceCertainty,
}

/// Certainty of a witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReferenceCertainty {
    Definite,
    Possible,
}

#[derive(Clone)]
enum LifecycleSourceMatcher {
    Target {
        target: LifecycleCallTarget,
        arguments: Vec<ArgumentConstraint>,
    },
}

#[derive(Clone)]
enum LifecycleRequirementMatcher {
    Property {
        property: String,
        value: ValueMatcher,
    },
    Member {
        member: glass_lint_datastructures::SymbolPath,
        arguments: Vec<ArgumentConstraint>,
    },
}

#[derive(Clone)]
struct LifecycleSinkMatcher {
    target: LifecycleCallTarget,
    argument: Option<usize>,
}

struct LifecycleReferencePlan {
    sources: Vec<LifecycleSourceMatcher>,
    requirements: Vec<LifecycleRequirementMatcher>,
    requirement_mode: RequirementMode,
    sinks: Vec<LifecycleSinkMatcher>,
    completion_mode: CompletionMode,
}

fn lifecycle_plan_from_normalized(lifecycle: &NormalizedLifecycle) -> LifecycleReferencePlan {
    let sources = lifecycle
        .sources()
        .iter()
        .filter_map(|source| {
            let target = lifecycle_target(source.event(), source.identity())?;
            Some(LifecycleSourceMatcher::Target {
                target,
                arguments: source.arguments().to_flat_vec(),
            })
        })
        .collect();
    let (requirements, requirement_mode) = lifecycle.condition().map_or_else(
        || (Vec::new(), RequirementMode::AnyRequired),
        |condition| match condition {
            NormalizedLifecycleCondition::AnyOf(events) => (
                events.iter().map(lifecycle_requirement).collect(),
                RequirementMode::AnyRequired,
            ),
            NormalizedLifecycleCondition::AllOf(events) => (
                events.iter().map(lifecycle_requirement).collect(),
                RequirementMode::AllRequired,
            ),
        },
    );
    let (sinks, completion_mode) = lifecycle.completion().map_or_else(
        || (Vec::new(), CompletionMode::AnySink),
        |completion| match completion {
            NormalizedLifecycleCompletion::Configuration => {
                (Vec::new(), CompletionMode::Configuration)
            }
            NormalizedLifecycleCompletion::AnySink(sinks) => (
                sinks.iter().map(lifecycle_sink).collect(),
                CompletionMode::AnySink,
            ),
            NormalizedLifecycleCompletion::AllSinks(sinks) => (
                sinks.iter().map(lifecycle_sink).collect(),
                CompletionMode::AllSinks,
            ),
        },
    );
    LifecycleReferencePlan {
        sources,
        requirements,
        requirement_mode,
        sinks,
        completion_mode,
    }
}

fn lifecycle_plan_from_physical(flow: &CompiledObjectFlow) -> LifecycleReferencePlan {
    LifecycleReferencePlan {
        sources: flow
            .sources()
            .map(|source| LifecycleSourceMatcher::Target {
                target: source.target().clone(),
                arguments: source.arguments().to_flat_vec(),
            })
            .collect(),
        requirements: flow
            .requirements()
            .map(|requirement| match requirement {
                requirement if let Some((property, value)) = requirement.property_write() => {
                    LifecycleRequirementMatcher::Property {
                        property: property.as_str().to_owned(),
                        value: value.clone(),
                    }
                }
                requirement => {
                    let (member, arguments) = requirement.member_call().unwrap();
                    LifecycleRequirementMatcher::Member {
                        member: member.clone(),
                        arguments: arguments.to_flat_vec(),
                    }
                }
            })
            .collect(),
        requirement_mode: flow.requirement_mode(),
        sinks: flow
            .sinks()
            .map(|sink| LifecycleSinkMatcher {
                target: sink.target().clone(),
                argument: sink.fixed_argument(),
            })
            .collect(),
        completion_mode: flow.completion_mode(),
    }
}

impl LifecycleReferencePlan {
    fn condition_sets<'a>(
        &self,
        path_rows: &[&'a ReferenceRow],
        source_event: u32,
    ) -> Option<Vec<Vec<&'a ReferenceRow>>> {
        let requirement_matches: Vec<Vec<&ReferenceRow>> = self
            .requirements
            .iter()
            .map(|matcher| {
                path_rows
                    .iter()
                    .copied()
                    .filter(|row| row.event > source_event && matches_requirement(matcher, row))
                    .collect()
            })
            .collect();

        self.requirement_mode.select_matches(requirement_matches)
    }

    fn completion_candidates<'a>(
        &self,
        path_rows: &[&'a ReferenceRow],
        condition_end: u32,
    ) -> Vec<Vec<&'a ReferenceRow>> {
        let sink_matches = self
            .sinks
            .iter()
            .map(|sink| {
                path_rows
                    .iter()
                    .copied()
                    .filter(|row| row.event > condition_end && matches_sink(sink, row))
                    .collect()
            })
            .collect();
        self.completion_mode.select_matches(sink_matches)
    }

    fn witness(
        path: u32,
        source: &ReferenceRow,
        condition_set: &[&ReferenceRow],
        completion_set: &[&ReferenceRow],
    ) -> ReferenceWitness {
        let primary_event = completion_set
            .iter()
            .map(|row| row.event)
            .max()
            .or_else(|| condition_set.iter().map(|row| row.event).max())
            .unwrap_or(source.event);
        let possible = source.completeness == ReferenceCompleteness::Unknown
            || condition_set
                .iter()
                .chain(completion_set.iter())
                .any(|row| row.completeness == ReferenceCompleteness::Unknown);

        ReferenceWitness {
            primary_event,
            support_events: vec![source.event],
            path_key: path,
            certainty: if possible {
                ReferenceCertainty::Possible
            } else {
                ReferenceCertainty::Definite
            },
        }
    }
}

fn lifecycle_target(event: &EventSpec, identity: &IdentitySpec) -> Option<LifecycleCallTarget> {
    match (event, identity) {
        (EventSpec::Call, IdentitySpec::Global { name }) => {
            Some(LifecycleCallTarget::Global(name.clone()))
        }
        (EventSpec::MemberCall { member }, IdentitySpec::Rooted { .. }) => {
            Some(LifecycleCallTarget::RootedMember(member.clone()))
        }
        _ => None,
    }
}

fn lifecycle_requirement(event: &NormalizedLifecycleEvent) -> LifecycleRequirementMatcher {
    match event {
        NormalizedLifecycleEvent::PropertyWrite { property, value } => {
            LifecycleRequirementMatcher::Property {
                property: property.as_str().to_owned(),
                value: value.clone(),
            }
        }
        NormalizedLifecycleEvent::MemberCall { member, arguments } => {
            LifecycleRequirementMatcher::Member {
                member: member.clone().into(),
                arguments: arguments.to_flat_vec(),
            }
        }
    }
}

fn lifecycle_sink(sink: &NormalizedLifecycleSink) -> LifecycleSinkMatcher {
    match sink {
        NormalizedLifecycleSink::ArgumentOf { target, index } => LifecycleSinkMatcher {
            target: target.clone(),
            argument: Some(*index),
        },
        NormalizedLifecycleSink::AnyArgumentOf { target } => LifecycleSinkMatcher {
            target: target.clone(),
            argument: None,
        },
    }
}

// ── Logical evaluator ───────────────────────────────────────────────────────

/// Evaluate a logical [`NormalizedQuery`] against a set of reference rows.
///
/// Returns sorted witnesses for comparison against physical evaluation.
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
        NormalizedRoot::Event(ev) => evaluate_event_logical(ev, rows),
        NormalizedRoot::Any(branches) => {
            let mut witnesses = Vec::new();
            for b in &**branches {
                witnesses.extend(evaluate_root_logical(b, rows));
            }
            witnesses
        }
        NormalizedRoot::Lifecycle(lifecycle) => {
            evaluate_lifecycle(&lifecycle_plan_from_normalized(lifecycle), rows)
        }
    }
}

fn evaluate_event_logical(ev: &NormalizedEvent, rows: &[ReferenceRow]) -> Vec<ReferenceWitness> {
    let mut witnesses = Vec::new();
    for row in rows {
        if !matches_event_kind_logical(ev.event(), &row.event_kind) {
            continue;
        }
        let identity = match ev.subject() {
            NormalizedSubject::Returned { producer, .. } => producer,
            NormalizedSubject::Instance { constructor, .. } => constructor,
            NormalizedSubject::Direct { identity } => identity,
        };
        if !matches_identity_logical(identity, &row.identity) {
            continue;
        }
        if !matches_arguments_logical(ev.arguments(), &row.arguments) {
            continue;
        }
        match ev.subject() {
            NormalizedSubject::Direct { .. } => {}
            subject => {
                let kind = match subject {
                    NormalizedSubject::Returned { .. } => ReferenceSupportKind::Producer,
                    NormalizedSubject::Instance { .. } => ReferenceSupportKind::Constructor,
                    NormalizedSubject::Direct { .. } => unreachable!(),
                };
                if !has_correlated_support(row, kind) {
                    continue;
                }
            }
        }

        let certainty = if row.completeness == ReferenceCompleteness::Unknown {
            ReferenceCertainty::Possible
        } else {
            ReferenceCertainty::Definite
        };

        let support_events = match ev.subject() {
            NormalizedSubject::Direct { .. } => Vec::new(),
            NormalizedSubject::Returned { .. } | NormalizedSubject::Instance { .. } => {
                vec![row.support.as_ref().expect("checked above").event]
            }
        };

        witnesses.push(ReferenceWitness {
            primary_event: row.event,
            support_events,
            path_key: row.path,
            certainty,
        });
    }
    witnesses
}

fn evaluate_lifecycle(
    plan: &LifecycleReferencePlan,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut by_path: BTreeMap<u32, Vec<&ReferenceRow>> = BTreeMap::new();
    for row in rows {
        by_path.entry(row.path).or_default().push(row);
    }
    for path_rows in by_path.values_mut() {
        path_rows.sort_by_key(|row| row.event);
    }

    let mut witnesses = Vec::new();
    for (path, path_rows) in by_path {
        for source in path_rows.iter().copied().filter(|row| {
            plan.sources
                .iter()
                .any(|matcher| matches_source(matcher, row))
        }) {
            let Some(condition_sets) = plan.condition_sets(&path_rows, source.event) else {
                continue;
            };
            for condition_set in condition_sets {
                let condition_end = condition_set
                    .iter()
                    .map(|row| row.event)
                    .max()
                    .unwrap_or(source.event);
                let completion_sets = plan.completion_candidates(&path_rows, condition_end);
                for completion_set in completion_sets {
                    witnesses.push(LifecycleReferencePlan::witness(
                        path,
                        source,
                        &condition_set,
                        &completion_set,
                    ));
                }
            }
        }
    }
    witnesses.sort();
    witnesses.dedup();
    witnesses
}

fn matches_source(matcher: &LifecycleSourceMatcher, row: &ReferenceRow) -> bool {
    match matcher {
        LifecycleSourceMatcher::Target { target, arguments } => {
            matches_target(target, row) && matches_flat_arguments(arguments, &row.arguments)
        }
    }
}

fn matches_requirement(matcher: &LifecycleRequirementMatcher, row: &ReferenceRow) -> bool {
    match matcher {
        LifecycleRequirementMatcher::Property { property, value } => {
            matches!(&row.event_kind, EventSpec::PropertyWrite { property: actual } if actual == &glass_lint_datastructures::SymbolPath::from(property.as_str()))
                && row
                    .arguments
                    .get(&ArgumentIndex::new_unchecked(0))
                    .is_some_and(|actual| matches_value_matcher(value, actual))
        }
        LifecycleRequirementMatcher::Member { member, arguments } => {
            matches!(&row.event_kind, EventSpec::MemberCall { member: actual } if actual == member)
                && matches_flat_arguments(arguments, &row.arguments)
        }
    }
}

fn matches_sink(matcher: &LifecycleSinkMatcher, row: &ReferenceRow) -> bool {
    matches_target(&matcher.target, row)
        && matcher.argument.is_none_or(|index| {
            u8::try_from(index).is_ok_and(|index| {
                row.arguments
                    .contains_key(&ArgumentIndex::new_unchecked(index))
            })
        })
}

fn matches_target(target: &LifecycleCallTarget, row: &ReferenceRow) -> bool {
    match target {
        LifecycleCallTarget::Global(name) => {
            matches!(&row.event_kind, EventSpec::Call)
                && matches!(&row.identity, IdentitySpec::Global { name: actual } if actual == name)
        }
        LifecycleCallTarget::RootedMember(member) => {
            matches!(&row.event_kind, EventSpec::MemberCall { member: actual } if actual == member)
        }
    }
}

fn matches_flat_arguments(
    constraints: &[ArgumentConstraint],
    args: &BTreeMap<ArgumentIndex, ReferenceValue>,
) -> bool {
    constraints.iter().all(|constraint| {
        args.get(&constraint.arg_index())
            .is_some_and(|value| matches_reference_value(constraint.predicate(), value))
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
            ReferenceValue::Unknown => matches!(predicate.kind(), StaticStringPredicateKind::Any),
        },
    }
}

fn matches_event_kind_logical(expected: &EventSpec, actual: &EventSpec) -> bool {
    expected == actual
}

fn matches_identity_logical(expected: &IdentitySpec, actual: &IdentitySpec) -> bool {
    expected == actual
}

fn matches_arguments_logical(
    constraints: &CanonicalArgumentConstraints,
    args: &BTreeMap<ArgumentIndex, ReferenceValue>,
) -> bool {
    for group in constraints.groups() {
        let idx = group.index();
        let Some(value) = args.get(&idx) else {
            return false;
        };
        if !group
            .predicates()
            .iter()
            .all(|m| matches_reference_value(m, value))
        {
            return false;
        }
    }
    true
}

// ── Physical evaluator ──────────────────────────────────────────────────────

/// Evaluate a [`PhysicalPlan`] against a set of reference rows.
///
/// Dispatches only on physical root fields. Returns sorted witnesses
/// for comparison against logical evaluation.
pub(crate) fn evaluate_supported_physical(
    plan: &PhysicalPlan,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut witnesses = Vec::new();
    for root in plan.roots() {
        witnesses.extend(evaluate_physical_root(root, rows));
    }
    witnesses.sort();
    witnesses.dedup();
    witnesses
}

fn evaluate_physical_root(root: &PhysicalRoot, rows: &[ReferenceRow]) -> Vec<ReferenceWitness> {
    match root {
        PhysicalRoot::IndexedScan {
            identity,
            event,
            evidence: _,
        } => evaluate_indexed_scan(identity, event, rows),
        PhysicalRoot::ConstrainedScan {
            identity,
            event,
            constraints,
            evidence: _,
        } => evaluate_constrained_scan(identity, event, constraints, rows),
        PhysicalRoot::ReturnedSubject {
            producer,
            member,
            event,
            evidence: _,
            object_slot: _,
        } => evaluate_returned_subject(producer, member, event, rows),
        PhysicalRoot::InstanceSubject {
            constructor,
            member,
            evidence: _,
            object_slot: _,
        } => evaluate_instance_subject(constructor, member, rows),
        PhysicalRoot::Lifecycle { flow } => {
            evaluate_lifecycle(&lifecycle_plan_from_physical(flow), rows)
        }
    }
}

fn evaluate_indexed_scan(
    identity: &IdentityConstraint,
    event: &EventPredicate,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut witnesses = Vec::new();
    for row in rows {
        if !matches_event_physical(event, &row.event_kind) {
            continue;
        }
        if !matches_identity_constraint(identity, &row.identity) {
            continue;
        }

        let certainty = if row.completeness == ReferenceCompleteness::Unknown {
            ReferenceCertainty::Possible
        } else {
            ReferenceCertainty::Definite
        };

        witnesses.push(ReferenceWitness {
            primary_event: row.event,
            support_events: Vec::new(),
            path_key: row.path,
            certainty,
        });
    }
    witnesses
}

fn evaluate_constrained_scan(
    identity: &IdentityConstraint,
    event: &EventPredicate,
    constraints: &CanonicalArgumentConstraints,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut witnesses = Vec::new();
    for row in rows {
        if !matches_event_physical(event, &row.event_kind) {
            continue;
        }
        if !matches_identity_constraint(identity, &row.identity) {
            continue;
        }
        if !matches_arguments_physical(constraints, &row.arguments) {
            continue;
        }

        let certainty = if row.completeness == ReferenceCompleteness::Unknown {
            ReferenceCertainty::Possible
        } else {
            ReferenceCertainty::Definite
        };

        witnesses.push(ReferenceWitness {
            primary_event: row.event,
            support_events: Vec::new(),
            path_key: row.path,
            certainty,
        });
    }
    witnesses
}

fn evaluate_returned_subject(
    identity: &IdentityConstraint,
    _member: &glass_lint_datastructures::SymbolPath,
    event: &EventPredicate,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut witnesses = Vec::new();
    for row in rows {
        if !has_correlated_support(row, ReferenceSupportKind::Producer) {
            continue;
        }
        if !matches_event_physical(event, &row.event_kind) {
            continue;
        }
        if !matches_identity_constraint(identity, &row.identity) {
            continue;
        }

        let certainty = if row.completeness == ReferenceCompleteness::Unknown {
            ReferenceCertainty::Possible
        } else {
            ReferenceCertainty::Definite
        };

        let support_events = vec![row.support.as_ref().expect("checked above").event];
        witnesses.push(ReferenceWitness {
            primary_event: row.event,
            support_events,
            path_key: row.path,
            certainty,
        });
    }
    witnesses
}

fn evaluate_instance_subject(
    constructor: &IdentityConstraint,
    member: &glass_lint_datastructures::SymbolPath,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut witnesses = Vec::new();
    for row in rows {
        if !has_correlated_support(row, ReferenceSupportKind::Constructor) {
            continue;
        }
        if !matches!(&row.event_kind, EventSpec::MemberCall { member: actual } if actual == member)
        {
            continue;
        }
        if !matches_identity_constraint(constructor, &row.identity) {
            continue;
        }

        let certainty = if row.completeness == ReferenceCompleteness::Unknown {
            ReferenceCertainty::Possible
        } else {
            ReferenceCertainty::Definite
        };

        let support_events = vec![row.support.as_ref().expect("checked above").event];
        witnesses.push(ReferenceWitness {
            primary_event: row.event,
            support_events,
            path_key: row.path,
            certainty,
        });
    }
    witnesses
}

fn has_correlated_support(row: &ReferenceRow, kind: ReferenceSupportKind) -> bool {
    row.object.is_some_and(|_| {
        row.support.as_ref().is_some_and(|support| {
            support.path == row.path && support.event != row.event && support.kind == kind
        })
    })
}

// ── Matching helpers ────────────────────────────────────────────────────────

fn matches_event_physical(expected: &EventPredicate, actual: &EventSpec) -> bool {
    &lower_event(actual) == expected
}

fn matches_identity_constraint(expected: &IdentityConstraint, actual: &IdentitySpec) -> bool {
    expected == &lower_identity(actual)
}

fn matches_arguments_physical(
    constraints: &CanonicalArgumentConstraints,
    args: &BTreeMap<ArgumentIndex, ReferenceValue>,
) -> bool {
    for group in constraints.groups() {
        let idx = group.index();
        let Some(value) = args.get(&idx) else {
            return false;
        };
        if !group
            .predicates()
            .iter()
            .all(|m| matches_reference_value(m, value))
        {
            return false;
        }
    }
    true
}

/// Check whether a matcher accepts a reference value.
fn matches_reference_value(matcher: &ArgumentMatcher, value: &ReferenceValue) -> bool {
    match matcher.kind() {
        ArgumentMatcherKind::Value(vm) => match vm.kind() {
            ValueMatcherKind::Any => true,
            ValueMatcherKind::StaticString(sp) => match value {
                ReferenceValue::StaticString(s) => match sp.kind() {
                    StaticStringPredicateKind::Any => true,
                    StaticStringPredicateKind::Exact(values) => values.iter().any(|v| v == s),
                    StaticStringPredicateKind::Prefix(prefixes) => {
                        prefixes.iter().any(|p| s.starts_with(p.as_str()))
                    }
                    StaticStringPredicateKind::ContainsAny(substrings) => {
                        substrings.iter().any(|sub| s.contains(sub.as_str()))
                    }
                    StaticStringPredicateKind::ContainsAll(substrings) => {
                        substrings.iter().all(|sub| s.contains(sub.as_str()))
                    }
                },
                ReferenceValue::Unknown => {
                    // An unknown value cannot satisfy a specific predicate.
                    matches!(sp.kind(), StaticStringPredicateKind::Any)
                }
            },
        },
        ArgumentMatcherKind::ObjectKeys(_)
        | ArgumentMatcherKind::ObjectPropertyValue { .. }
        | ArgumentMatcherKind::RootedExpressions(_) => false,
    }
}
