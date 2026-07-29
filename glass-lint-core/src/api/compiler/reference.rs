//! Test-only logical/physical equivalence oracle.
//!
//! Provides a small synthetic relation store and two evaluators
//! (`evaluate_logical` and `evaluate_physical`) that produce
//! deterministic witnesses.  The oracle compares the sorted witness
//! lists to verify that the physical planner has the same semantics
//! as the logical query over the same small domain.

#![cfg(test)]

use std::collections::BTreeMap;

use crate::api::{
    compiler::{
        normalize::{NormalizedEvent, NormalizedQuery, NormalizedRoot, NormalizedSubject},
        physical::{CompiledArgumentConstraints, PhysicalPlan, PhysicalRoot},
        rule::{EventPredicate, IdentityConstraint, lower_event, lower_identity},
    },
    rule::{
        ArgumentConstraint, ArgumentIndex, ArgumentMatcher, ArgumentMatcherKind,
        StaticStringPredicateKind, ValueMatcherKind,
        query::{EventSpec, IdentitySpec},
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

// ── Logical evaluator ───────────────────────────────────────────────────────

/// Evaluate a logical [`NormalizedQuery`] against a set of reference rows.
///
/// Returns sorted witnesses for comparison against physical evaluation.
pub(crate) fn evaluate_logical(
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
        NormalizedRoot::Lifecycle(_) => {
            // Lifecycle evaluation not implemented in Phase 12 oracle.
            Vec::new()
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
            NormalizedSubject::Direct => ev
                .identity()
                .expect("direct normalized events retain an identity"),
        };
        if !matches_identity_logical(identity, &row.identity) {
            continue;
        }
        if !matches_arguments_logical(ev.arguments(), &row.arguments) {
            continue;
        }
        match ev.subject() {
            NormalizedSubject::Direct => {}
            subject => {
                let kind = match subject {
                    NormalizedSubject::Returned { .. } => ReferenceSupportKind::Producer,
                    NormalizedSubject::Instance { .. } => ReferenceSupportKind::Constructor,
                    NormalizedSubject::Direct => unreachable!(),
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
            NormalizedSubject::Direct => Vec::new(),
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

fn matches_event_kind_logical(expected: &EventSpec, actual: &EventSpec) -> bool {
    expected == actual
}

fn matches_identity_logical(expected: &IdentitySpec, actual: &IdentitySpec) -> bool {
    expected == actual
}

fn matches_arguments_logical(
    constraints: &[ArgumentConstraint],
    args: &BTreeMap<ArgumentIndex, ReferenceValue>,
) -> bool {
    for constraint in constraints {
        let idx = constraint.arg_index();
        let Some(value) = args.get(&idx) else {
            return false;
        };
        if !matches_reference_value(constraint.predicate(), value) {
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
pub(crate) fn evaluate_physical(
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
        PhysicalRoot::Lifecycle { flow: _ } => {
            // Lifecycle evaluation not implemented in Phase 12 oracle.
            Vec::new()
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
    constraints: &CompiledArgumentConstraints,
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
    _member: &glass_lint_datastructures::SymbolPath,
    rows: &[ReferenceRow],
) -> Vec<ReferenceWitness> {
    let mut witnesses = Vec::new();
    for row in rows {
        if !has_correlated_support(row, ReferenceSupportKind::Constructor) {
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
    // Lower the identity spec and compare.
    let lowered = lower_identity(actual);
    // For Global constraints, compare name and strength.
    match (expected, &lowered) {
        (
            IdentityConstraint::Global {
                name: en,
                strength: es,
            }
            | IdentityConstraint::Any {
                name: en,
                strength: es,
            },
            IdentityConstraint::Global {
                name: an,
                strength: as_,
            }
            | IdentityConstraint::Any {
                name: an,
                strength: as_,
            },
        ) => en == an && es == as_,
        (
            IdentityConstraint::ModuleExport {
                module: em,
                export: ee,
            },
            IdentityConstraint::ModuleExport {
                module: am,
                export: ae,
            },
        ) => em == am && ee == ae,
        (IdentityConstraint::Rooted { path: ep }, IdentityConstraint::Rooted { path: ap }) => {
            ep == ap
        }
        _ => false,
    }
}

fn matches_arguments_physical(
    constraints: &CompiledArgumentConstraints,
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
        ArgumentMatcherKind::Value(vm) => match &vm.kind {
            ValueMatcherKind::Any => true,
            ValueMatcherKind::StaticString(sp) => match value {
                ReferenceValue::StaticString(s) => match &sp.kind {
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
                    matches!(sp.kind, StaticStringPredicateKind::Any)
                }
            },
        },
        ArgumentMatcherKind::ObjectKeys(_)
        | ArgumentMatcherKind::ObjectPropertyValue { .. }
        | ArgumentMatcherKind::RootedExpressions(_) => false,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::SymbolPath;
    use smol_str::SmolStr;

    use super::*;
    use crate::api::{
        compiler::{normalize::normalize_query_decl, physical::plan_normalized},
        rule::{
            ValueMatcher,
            query::{EventQuery, QueryDecl},
        },
    };

    // ── Helpers ──────────────────────────────────────────────────────

    fn row(event: u32, event_kind: EventSpec, identity: IdentitySpec, path: u32) -> ReferenceRow {
        ReferenceRow {
            event,
            event_kind,
            identity,
            arguments: BTreeMap::new(),
            object: None,
            support: None,
            path,
            completeness: ReferenceCompleteness::Complete,
        }
    }

    fn row_with_args(
        event: u32,
        event_kind: EventSpec,
        identity: IdentitySpec,
        path: u32,
        arguments: BTreeMap<ArgumentIndex, ReferenceValue>,
    ) -> ReferenceRow {
        ReferenceRow {
            event,
            event_kind,
            identity,
            arguments,
            object: None,
            support: None,
            path,
            completeness: ReferenceCompleteness::Complete,
        }
    }

    fn row_unknown(
        event: u32,
        event_kind: EventSpec,
        identity: IdentitySpec,
        path: u32,
    ) -> ReferenceRow {
        ReferenceRow {
            event,
            event_kind,
            identity,
            arguments: BTreeMap::new(),
            object: None,
            support: None,
            path,
            completeness: ReferenceCompleteness::Unknown,
        }
    }

    fn logical_witnesses(query: &NormalizedQuery, rows: &[ReferenceRow]) -> Vec<ReferenceWitness> {
        evaluate_logical(query, rows)
    }

    fn physical_witnesses(plan: &PhysicalPlan, rows: &[ReferenceRow]) -> Vec<ReferenceWitness> {
        evaluate_physical(plan, rows)
    }

    fn witnesses_equal(
        query: &NormalizedQuery,
        plan: &PhysicalPlan,
        rows: &[ReferenceRow],
    ) -> bool {
        let logical = logical_witnesses(query, rows);
        let physical = physical_witnesses(plan, rows);
        logical == physical
    }

    // ── Empty and non-empty relations ───────────────────────────────

    #[test]
    fn empty_rows_produce_no_witnesses() {
        let decl = QueryDecl::call_global("fetch").unwrap();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);
        assert!(witnesses_equal(&nq, &plan, &[]));
        assert!(logical_witnesses(&nq, &[]).is_empty());
        assert!(physical_witnesses(&plan, &[]).is_empty());
    }

    #[test]
    fn matching_rows_produce_witnesses() {
        let decl = QueryDecl::call_global("fetch").unwrap();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);

        let rows = vec![row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
        )];
        assert!(witnesses_equal(&nq, &plan, &rows));
        assert_eq!(logical_witnesses(&nq, &rows).len(), 1);
    }

    #[test]
    fn non_matching_rows_produce_no_witnesses() {
        let decl = QueryDecl::call_global("fetch").unwrap();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);

        let rows = vec![row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("navigate"),
            },
            0,
        )];
        assert!(witnesses_equal(&nq, &plan, &rows));
        assert!(logical_witnesses(&nq, &rows).is_empty());
    }

    // ── Duplicate rows ─────────────────────────────────────────────

    #[test]
    fn duplicate_rows_produce_deduplicated_witnesses() {
        let decl = QueryDecl::call_global("fetch").unwrap();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);

        let rows = vec![
            row(
                1,
                EventSpec::Call,
                IdentitySpec::Global {
                    name: SmolStr::new("fetch"),
                },
                0,
            ),
            row(
                1,
                EventSpec::Call,
                IdentitySpec::Global {
                    name: SmolStr::new("fetch"),
                },
                0,
            ),
        ];
        assert!(witnesses_equal(&nq, &plan, &rows));
        // Dedup should produce one witness.
        assert_eq!(logical_witnesses(&nq, &rows).len(), 1);
    }

    // ── Alternative order (Any) ────────────────────────────────────

    #[test]
    fn any_branch_order_produces_same_witnesses() {
        let decl_a = QueryDecl::any([
            Ok(QueryDecl::call_global("fetch").unwrap()),
            Ok(QueryDecl::call_global("navigate").unwrap()),
        ])
        .unwrap();
        let decl_b = QueryDecl::any([
            Ok(QueryDecl::call_global("navigate").unwrap()),
            Ok(QueryDecl::call_global("fetch").unwrap()),
        ])
        .unwrap();

        let nq_a = normalize_query_decl(&decl_a).unwrap();
        let nq_b = normalize_query_decl(&decl_b).unwrap();
        let plan_a = plan_normalized(&nq_a);
        let plan_b = plan_normalized(&nq_b);

        let rows = vec![
            row(
                1,
                EventSpec::Call,
                IdentitySpec::Global {
                    name: SmolStr::new("fetch"),
                },
                0,
            ),
            row(
                2,
                EventSpec::Call,
                IdentitySpec::Global {
                    name: SmolStr::new("navigate"),
                },
                1,
            ),
        ];

        let l_a = logical_witnesses(&nq_a, &rows);
        let l_b = logical_witnesses(&nq_b, &rows);
        let p_a = physical_witnesses(&plan_a, &rows);
        let p_b = physical_witnesses(&plan_b, &rows);

        assert_eq!(l_a, l_b, "logical witnesses should be order-independent");
        assert_eq!(p_a, p_b, "physical witnesses should be order-independent");
    }

    // ── Possible versus definite ───────────────────────────────────

    #[test]
    fn unknown_row_produces_possible_witness() {
        let decl = QueryDecl::call_global("fetch").unwrap();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);

        let rows = vec![row_unknown(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
        )];
        assert!(witnesses_equal(&nq, &plan, &rows));
        let witnesses = logical_witnesses(&nq, &rows);
        assert_eq!(witnesses.len(), 1);
        assert_eq!(witnesses[0].certainty, ReferenceCertainty::Possible);
    }

    #[test]
    fn complete_row_produces_definite_witness() {
        let decl = QueryDecl::call_global("fetch").unwrap();
        let nq = normalize_query_decl(&decl).unwrap();

        let rows = vec![row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
        )];
        let witnesses = logical_witnesses(&nq, &rows);
        assert_eq!(witnesses[0].certainty, ReferenceCertainty::Definite);
    }

    // ── Unknown alternatives ───────────────────────────────────────

    #[test]
    fn unknown_alternative_does_not_erase_complete_witness() {
        let decl = QueryDecl::any([
            Ok(QueryDecl::call_global("fetch").unwrap()),
            Ok(QueryDecl::call_global("navigate").unwrap()),
        ])
        .unwrap();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);

        // Only fetch row exists with Complete
        let rows = vec![row(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
        )];
        assert!(witnesses_equal(&nq, &plan, &rows));
        let witnesses = logical_witnesses(&nq, &rows);
        assert_eq!(witnesses.len(), 1);
        assert_eq!(witnesses[0].certainty, ReferenceCertainty::Definite);
    }

    // ── Evidence ordering ──────────────────────────────────────────

    #[test]
    fn witnesses_are_sorted_deterministically() {
        let decl = QueryDecl::any([
            Ok(QueryDecl::call_global("navigate").unwrap()),
            Ok(QueryDecl::call_global("fetch").unwrap()),
        ])
        .unwrap();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);

        let rows = vec![
            row(
                2,
                EventSpec::Call,
                IdentitySpec::Global {
                    name: SmolStr::new("navigate"),
                },
                1,
            ),
            row(
                1,
                EventSpec::Call,
                IdentitySpec::Global {
                    name: SmolStr::new("fetch"),
                },
                0,
            ),
        ];

        let l = logical_witnesses(&nq, &rows);
        let p = physical_witnesses(&plan, &rows);
        assert_eq!(l, p);

        // Witnesses should be sorted by primary_event then path_key.
        for pair in l.windows(2) {
            assert!(
                pair[0] <= pair[1],
                "witnesses must be sorted: {:?} >= {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    // ── Constrained scan ───────────────────────────────────────────

    #[test]
    fn constrained_scan_matches_arguments() {
        let eq = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().equals("/api"))
            .unwrap();
        let decl = eq.into_query();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);

        let mut args = BTreeMap::new();
        args.insert(
            ArgumentIndex::new_unchecked(0),
            ReferenceValue::StaticString("/api".into()),
        );

        let matching = vec![row_with_args(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
            args.clone(),
        )];
        assert!(witnesses_equal(&nq, &plan, &matching));
        assert_eq!(logical_witnesses(&nq, &matching).len(), 1);

        // Non-matching argument value.
        let mut wrong_args = BTreeMap::new();
        wrong_args.insert(
            ArgumentIndex::new_unchecked(0),
            ReferenceValue::StaticString("/other".into()),
        );
        let non_matching = vec![row_with_args(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
            wrong_args,
        )];
        assert!(witnesses_equal(&nq, &plan, &non_matching));
        assert!(logical_witnesses(&nq, &non_matching).is_empty());
    }

    // ── Filter order invariance ────────────────────────────────────

    #[test]
    fn argument_filter_order_produces_same_witnesses() {
        // Order of argument constraints should not matter.
        let eq_a = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().equals("/api"))
            .unwrap()
            .with_arg(1, ValueMatcher::static_string().equals("post"))
            .unwrap();
        let eq_b = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(1, ValueMatcher::static_string().equals("post"))
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().equals("/api"))
            .unwrap();

        let decl_a = eq_a.into_query();
        let decl_b = eq_b.into_query();

        let nq_a = normalize_query_decl(&decl_a).unwrap();
        let nq_b = normalize_query_decl(&decl_b).unwrap();
        let plan_a = plan_normalized(&nq_a);
        let plan_b = plan_normalized(&nq_b);

        let mut args = BTreeMap::new();
        args.insert(
            ArgumentIndex::new_unchecked(0),
            ReferenceValue::StaticString("/api".into()),
        );
        args.insert(
            ArgumentIndex::new_unchecked(1),
            ReferenceValue::StaticString("post".into()),
        );

        let rows = vec![row_with_args(
            1,
            EventSpec::Call,
            IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            0,
            args,
        )];

        let l_a = logical_witnesses(&nq_a, &rows);
        let l_b = logical_witnesses(&nq_b, &rows);
        let p_a = physical_witnesses(&plan_a, &rows);
        let p_b = physical_witnesses(&plan_b, &rows);

        assert_eq!(
            l_a, l_b,
            "logical witnesses should be filter-order independent"
        );
        assert_eq!(
            p_a, p_b,
            "physical witnesses should be filter-order independent"
        );
    }

    // ── Returned subject ───────────────────────────────────────────

    #[test]
    fn returned_subject_produces_support_evidence() {
        let decl = QueryDecl::member_call_returned("create", "send").unwrap();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);

        let rows = vec![ReferenceRow {
            event: 42,
            event_kind: EventSpec::MemberCall {
                member: SymbolPath::from("send"),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("create"),
            },
            arguments: BTreeMap::new(),
            object: Some(7),
            support: Some(ReferenceSupport {
                event: 7,
                path: 0,
                kind: ReferenceSupportKind::Producer,
            }),
            path: 0,
            completeness: ReferenceCompleteness::Complete,
        }];

        assert!(witnesses_equal(&nq, &plan, &rows));
        let w = logical_witnesses(&nq, &rows);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].support_events, vec![7]);

        let mut incomplete = rows[0].clone();
        incomplete.support = None;
        assert!(logical_witnesses(&nq, &[incomplete]).is_empty());
    }

    // ── Incompatible correlation keys ──────────────────────────────

    #[test]
    fn different_path_keys_produce_separate_witnesses() {
        let decl = QueryDecl::any([
            Ok(QueryDecl::call_global("fetch").unwrap()),
            Ok(QueryDecl::call_global("navigate").unwrap()),
        ])
        .unwrap();
        let nq = normalize_query_decl(&decl).unwrap();
        let plan = plan_normalized(&nq);

        let rows = vec![
            row(
                1,
                EventSpec::Call,
                IdentitySpec::Global {
                    name: SmolStr::new("fetch"),
                },
                10,
            ),
            row(
                2,
                EventSpec::Call,
                IdentitySpec::Global {
                    name: SmolStr::new("navigate"),
                },
                20,
            ),
        ];

        let l = logical_witnesses(&nq, &rows);
        let p = physical_witnesses(&plan, &rows);
        assert_eq!(l, p);
        assert_eq!(l.len(), 2);
        // Each witness should have its own path_key.
        assert!(l.iter().any(|w| w.path_key == 10));
        assert!(l.iter().any(|w| w.path_key == 20));
    }
}
