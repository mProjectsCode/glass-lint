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
    if !matches!(subject, NormalizedSubject::Direct { .. }) {
        return Ok(());
    }
    let valid = match event {
        EventSpec::Call => matches!(
            identity,
            IdentitySpec::Global { .. }
                | IdentitySpec::Heuristic { .. }
                | IdentitySpec::ModuleExport { .. }
                | IdentitySpec::PackageModuleExport { .. }
        ),
        EventSpec::Construct => matches!(
            identity,
            IdentitySpec::Global { .. }
                | IdentitySpec::Rooted { .. }
                | IdentitySpec::Heuristic { .. }
                | IdentitySpec::ModuleExport { .. }
                | IdentitySpec::PackageModuleExport { .. }
        ),
        EventSpec::MemberCall { .. }
        | EventSpec::MemberRead { .. }
        | EventSpec::PropertyWrite { .. } => matches!(
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
        check_static_intersection(var, matchers)?;
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
                | StaticStringPredicateKind::Prefix(values)
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

fn check_static_intersection(
    var: VarId,
    matchers: &[&crate::api::rule::ArgumentMatcher],
) -> Result<(), QueryCompileError> {
    use crate::api::rule::{ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcherKind};
    let predicates: Vec<&StaticStringPredicateKind> = matchers
        .iter()
        .filter_map(|matcher| match matcher.kind() {
            ArgumentMatcherKind::Value(vm) => match &vm.kind {
                ValueMatcherKind::StaticString(predicate) => Some(&predicate.kind),
                ValueMatcherKind::Any => None,
            },
            _ => None,
        })
        .collect();

    let mut exact_candidates: Option<std::collections::BTreeSet<String>> = None;
    let mut prefix_sets = Vec::new();
    for predicate in &predicates {
        match predicate {
            StaticStringPredicateKind::Exact(values) => {
                let values = values
                    .iter()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>();
                exact_candidates = Some(match exact_candidates {
                    Some(candidates) => candidates.intersection(&values).cloned().collect(),
                    None => values,
                });
            }
            StaticStringPredicateKind::Prefix(values) => prefix_sets.push(values),
            _ => {}
        }
    }

    if let Some(candidates) = exact_candidates {
        if candidates.is_empty() {
            return Err(QueryCompileError::ContradictoryPredicate {
                variable: var,
                detail: ContradictionKind::StaticExactValues,
            });
        }
        let compatible = candidates.iter().any(|candidate| {
            predicates
                .iter()
                .all(|predicate| accepts(predicate, candidate))
        });
        if !compatible {
            return Err(QueryCompileError::ContradictoryPredicate {
                variable: var,
                detail: ContradictionKind::StaticExactAndPrefix,
            });
        }
        return Ok(());
    }

    let mut possible_prefixes = vec![String::new()];
    for prefixes in prefix_sets {
        let mut next = Vec::new();
        for current in &possible_prefixes {
            for prefix in prefixes {
                if current.starts_with(prefix) {
                    next.push(current.clone());
                } else if prefix.starts_with(current) {
                    next.push(prefix.clone());
                }
            }
        }
        next.sort_unstable();
        next.dedup();
        possible_prefixes = next;
        if possible_prefixes.is_empty() {
            return Err(QueryCompileError::ContradictoryPredicate {
                variable: var,
                detail: ContradictionKind::StaticExactAndPrefix,
            });
        }
    }
    Ok(())
}

fn accepts(predicate: &crate::api::rule::StaticStringPredicateKind, candidate: &str) -> bool {
    use crate::api::rule::StaticStringPredicateKind;
    match predicate {
        StaticStringPredicateKind::Any => true,
        StaticStringPredicateKind::Exact(values) => values.iter().any(|value| value == candidate),
        StaticStringPredicateKind::Prefix(values) => {
            values.iter().any(|prefix| candidate.starts_with(prefix))
        }
        StaticStringPredicateKind::ContainsAny(values) => {
            values.iter().any(|value| candidate.contains(value))
        }
        StaticStringPredicateKind::ContainsAll(values) => {
            values.iter().all(|value| candidate.contains(value))
        }
    }
}
