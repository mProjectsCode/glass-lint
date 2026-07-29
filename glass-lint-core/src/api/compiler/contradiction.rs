use std::collections::BTreeMap;

use super::validate::{ContradictionKind, QueryCompileError};
use crate::api::{
    compiler::normalized::NormalizedSubject,
    rule::{
        ArgumentConstraint,
        query::{EventSpec, IdentitySpec, VarId},
    },
};

pub(crate) fn detect_event_contradictions(
    var: VarId,
    event: &EventSpec,
    identity: &IdentitySpec,
    subject: &NormalizedSubject,
    constraints: &[ArgumentConstraint],
) -> Result<(), QueryCompileError> {
    check_dimension_contradictions(var, event, identity, subject)?;
    check_argument_contradictions(var, constraints)
}

fn check_dimension_contradictions(
    var: VarId,
    event: &EventSpec,
    identity: &IdentitySpec,
    subject: &NormalizedSubject,
) -> Result<(), QueryCompileError> {
    if !matches!(subject, NormalizedSubject::Direct) {
        return Ok(());
    }
    let valid = match event {
        EventSpec::Call | EventSpec::Construct => matches!(
            identity,
            IdentitySpec::Global { .. }
                | IdentitySpec::Heuristic { .. }
                | IdentitySpec::ModuleExport { .. }
                | IdentitySpec::PackageModuleExport { .. }
        ),
        EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. } => matches!(
            identity,
            IdentitySpec::Rooted { .. }
                | IdentitySpec::Heuristic { .. }
                | IdentitySpec::ModuleNamespace { .. }
                | IdentitySpec::PackageModuleNamespace { .. }
        ),
        _ => true,
    };
    if !valid {
        return Err(QueryCompileError::ContradictoryPredicate {
            variable: var,
            detail: ContradictionKind::EventKind,
        });
    }
    Ok(())
}

fn check_argument_contradictions(
    var: VarId,
    constraints: &[ArgumentConstraint],
) -> Result<(), QueryCompileError> {
    let mut by_index: BTreeMap<usize, Vec<&crate::api::rule::ArgumentMatcher>> = BTreeMap::new();
    for c in constraints {
        by_index.entry(c.index()).or_default().push(c.predicate());
    }

    for matchers in by_index.values() {
        check_empty_accepted_sets(var, matchers)?;
        check_disjoint_exact_sets(var, matchers)?;
        check_exact_prefix_contradiction(var, matchers)?;
    }
    Ok(())
}

fn check_empty_accepted_sets(
    var: VarId,
    matchers: &[&crate::api::rule::ArgumentMatcher],
) -> Result<(), QueryCompileError> {
    use crate::api::rule::{ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcherKind};
    for m in matchers {
        if let ArgumentMatcherKind::Value(vm) = m.kind()
            && let ValueMatcherKind::StaticString(sp) = &vm.kind
        {
            let is_empty = matches!(
                &sp.kind,
                StaticStringPredicateKind::Exact(values)
                | StaticStringPredicateKind::ContainsAny(values)
                | StaticStringPredicateKind::ContainsAll(values)
                    if values.is_empty()
            );
            if is_empty {
                return Err(QueryCompileError::ContradictoryPredicate {
                    variable: var,
                    detail: ContradictionKind::StaticExactValues,
                });
            }
        }
    }
    Ok(())
}

fn check_disjoint_exact_sets(
    var: VarId,
    matchers: &[&crate::api::rule::ArgumentMatcher],
) -> Result<(), QueryCompileError> {
    use crate::api::rule::{ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcherKind};
    let exact_sets: Vec<std::collections::BTreeSet<String>> = matchers
        .iter()
        .filter_map(|m| match m.kind() {
            ArgumentMatcherKind::Value(vm) => match &vm.kind {
                ValueMatcherKind::StaticString(sp) => match &sp.kind {
                    StaticStringPredicateKind::Exact(values) if !values.is_empty() => {
                        Some(values.iter().cloned().collect())
                    }
                    _ => None,
                },
                ValueMatcherKind::Any => None,
            },
            _ => None,
        })
        .collect();

    if exact_sets.len() >= 2 {
        for i in 0..exact_sets.len() {
            for j in (i + 1)..exact_sets.len() {
                if exact_sets[i].is_disjoint(&exact_sets[j]) {
                    return Err(QueryCompileError::ContradictoryPredicate {
                        variable: var,
                        detail: ContradictionKind::StaticExactValues,
                    });
                }
            }
        }
    }
    Ok(())
}

fn check_exact_prefix_contradiction(
    var: VarId,
    matchers: &[&crate::api::rule::ArgumentMatcher],
) -> Result<(), QueryCompileError> {
    use crate::api::rule::{ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcherKind};
    let exact_values: Vec<&String> = matchers
        .iter()
        .filter_map(|m| match m.kind() {
            ArgumentMatcherKind::Value(vm) => match &vm.kind {
                ValueMatcherKind::StaticString(sp) => match &sp.kind {
                    StaticStringPredicateKind::Exact(values) => Some(values.iter()),
                    _ => None,
                },
                ValueMatcherKind::Any => None,
            },
            _ => None,
        })
        .flatten()
        .collect();

    let prefix_sets: Vec<&Vec<String>> = matchers
        .iter()
        .filter_map(|m| match m.kind() {
            ArgumentMatcherKind::Value(vm) => match &vm.kind {
                ValueMatcherKind::StaticString(sp) => match &sp.kind {
                    StaticStringPredicateKind::Prefix(values) => Some(values),
                    _ => None,
                },
                ValueMatcherKind::Any => None,
            },
            _ => None,
        })
        .collect();

    if exact_values.is_empty() || prefix_sets.is_empty() {
        return Ok(());
    }

    let any_compatible = exact_values.iter().any(|e| {
        prefix_sets
            .iter()
            .any(|prefixes| prefixes.iter().any(|p| e.starts_with(p.as_str())))
    });
    if !any_compatible {
        return Err(QueryCompileError::ContradictoryPredicate {
            variable: var,
            detail: ContradictionKind::StaticExactAndPrefix,
        });
    }
    Ok(())
}
