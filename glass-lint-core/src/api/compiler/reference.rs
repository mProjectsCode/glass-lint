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
            CanonicalArgumentConstraints, NormalizedEvent, NormalizedQuery, NormalizedRoot,
            NormalizedSubject,
        },
        physical::{PhysicalPlan, PhysicalRoot},
        rule::{EventPredicate, IdentityConstraint, lower_event, lower_identity},
    },
    rule::{
        ArgumentIndex, ArgumentMatcher, ArgumentMatcherKind, StaticStringPredicateKind,
        ValueMatcherKind,
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
        NormalizedRoot::Lifecycle(_) => {
            panic!("reference evaluator does not support lifecycle roots")
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
        PhysicalRoot::Lifecycle { .. } => {
            panic!("reference evaluator does not support lifecycle roots")
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
