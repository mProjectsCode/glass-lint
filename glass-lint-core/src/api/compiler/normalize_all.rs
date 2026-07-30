// ── All normalization ─────────────────────────────────────────────────────

use super::normalize::normalize_root;
use crate::api::{
    compiler::{
        contradiction::detect_event_contradictions,
        normalized::{
            CanonicalArgumentConstraints, NormalizedEvent, NormalizedRoot, NormalizedSubject,
        },
        validate::{ContradictionKind, QueryCompileError},
    },
    rule::{
        ArgumentConstraint,
        query::{
            AllExpr, EmissionDecl, EventQuery, EventSpec, IdentitySpec, QueryExpr, QueryExprKind,
            QueryPredicate, VarId,
        },
    },
};

/// Normalize an `All` expression.
///
/// Same-event conjunction: all branches reference the same VarId →
/// merge into one `NormalizedEvent`.
///
/// Uncorrelated multi-event: no shared variable → error.
///
/// Other multi-event: reject as unsupported.
pub(crate) fn normalize_all_root(
    all: &AllExpr,
    emission: &EmissionDecl,
) -> Result<NormalizedRoot, QueryCompileError> {
    // Collect the set of distinct binding variables across branches.
    let branch_vars: Vec<Vec<VarId>> = all.branches.iter().map(QueryExpr::vars).collect();

    // Single branch — normalize as-is (should be rare after construction).
    if all.branches.len() == 1 {
        return normalize_root(&all.branches[0], emission);
    }

    // Find the common event variable that all branches share.
    find_common_event_var(&all.branches).map_or_else(
        || {
            // No shared variable — check correlation scope.
            let all_share_some = branch_vars
                .first()
                .is_some_and(|first| {
                    branch_vars
                        .iter()
                        .skip(1)
                        .any(|vars| vars.iter().any(|v| first.contains(v)))
                });

            if all_share_some {
                Err(QueryCompileError::UnsupportedRelation {
                    relation: "all",
                    detail:
                        "multi-event All without same-variable correlation is unsupported through Phase 12"
                            .into(),
                })
            } else {
                Err(QueryCompileError::UncorrelatedConjunction)
            }
        },
        |var| merge_same_event(all, var, emission),
    )
}

/// Find a variable bound as an event by the first branch that also
/// appears in every other branch (directly referenced or correlated).
///
/// `ReturnedObject` and `ConstructedObject` predicates only bind new
/// object variables; they do not reference the event variable.  The
/// correlation is via a separate `MemberSubject` predicate.  These
/// binding-only predicates are accepted as not breaking the chain.
fn find_common_event_var(branches: &[QueryExpr]) -> Option<VarId> {
    if branches.is_empty() {
        return None;
    }
    // Collect binding vars from the first branch.
    let first_bindings = branches[0].binding_vars();
    for var in &first_bindings {
        if branches.iter().skip(1).all(|b| {
            b.contains_var(*var)
                || matches!(
                    &b.kind,
                    QueryExprKind::Require(
                        QueryPredicate::ReturnedObject { .. }
                            | QueryPredicate::ConstructedObject { .. }
                    )
                )
        }) {
            return Some(*var);
        }
    }
    None
}

/// Merge branches of a same-event `All` into one `NormalizedEvent`.
///
/// Collects event spec, identity, subject, and argument constraints
/// from all branches and merges them onto one event node.
fn merge_same_event(
    all: &AllExpr,
    event_var: VarId,
    _emission: &EmissionDecl,
) -> Result<NormalizedRoot, QueryCompileError> {
    let mut event_spec: Option<EventSpec> = None;
    let mut identity_spec: Option<IdentitySpec> = None;
    let mut subject = NormalizedSubject::Direct;
    let mut constraints: Vec<ArgumentConstraint> = Vec::new();

    for branch in &all.branches {
        match &branch.kind {
            QueryExprKind::Event(eq) => {
                merge_event_fields(&mut event_spec, &mut identity_spec, eq)?;
                constraints.extend(eq.constraints.iter().cloned());
            }
            QueryExprKind::SelectEvent(_) => {
                // Just a binding reference, no fields to merge.
            }
            QueryExprKind::Require(p) => match p {
                QueryPredicate::EventKind { expected, .. } => {
                    merge_event_kind(&mut event_spec, expected.clone())?;
                }
                QueryPredicate::EventIdentity { expected, .. } => {
                    merge_identity(&mut identity_spec, expected.clone())?;
                }
                QueryPredicate::Argument { index, matcher, .. } => {
                    constraints.push(ArgumentConstraint::new(*index, matcher.clone()));
                }
                QueryPredicate::ReturnedObject { bind, identity } => {
                    merge_subject_relation(
                        &mut subject,
                        NormalizedSubject::Returned {
                            producer: identity.clone(),
                            object_slot: var_to_slot(*bind),
                        },
                    )?;
                }
                QueryPredicate::ConstructedObject { bind, identity } => {
                    merge_subject_relation(
                        &mut subject,
                        NormalizedSubject::Instance {
                            constructor: identity.clone(),
                            object_slot: var_to_slot(*bind),
                        },
                    )?;
                }
                QueryPredicate::MemberSubject { event, object } => {
                    if *event != event_var {
                        return Err(QueryCompileError::UncorrelatedConjunction);
                    }
                    match &subject {
                        NormalizedSubject::Returned { object_slot, .. }
                        | NormalizedSubject::Instance { object_slot, .. }
                            if *object_slot == var_to_slot(*object) => {}
                        _ => {
                            return Err(QueryCompileError::UncorrelatedConjunction);
                        }
                    }
                }
            },
            _ => {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "unexpected branch kind in same-event All".into(),
                });
            }
        }
    }

    let event = event_spec.ok_or_else(|| QueryCompileError::InternalInvariant {
        detail: "same-event All missing event kind".into(),
    })?;

    let identity = identity_spec.ok_or_else(|| QueryCompileError::InternalInvariant {
        detail: "same-event All missing identity".into(),
    })?;

    // Canonicalize constraints: sort by index then by matcher payload.
    constraints.sort_by(|a, b| {
        a.index()
            .cmp(&b.index())
            .then_with(|| a.predicate().cmp(b.predicate()))
    });
    // Deduplicate.
    constraints.dedup();

    // Detect contradictions on the merged event.
    detect_event_contradictions(event_var, &event, &identity, &subject, &constraints)?;

    let slot = var_to_slot(event_var);
    let normalized_identity = matches!(subject, NormalizedSubject::Direct).then_some(identity);

    Ok(NormalizedRoot::Event(NormalizedEvent {
        slot,
        event,
        identity: normalized_identity,
        subject,
        arguments: CanonicalArgumentConstraints::from_canonicalized(&constraints),
    }))
}

fn var_to_slot(var: VarId) -> u32 {
    var.get()
}

fn merge_event_fields(
    event_spec: &mut Option<EventSpec>,
    identity_spec: &mut Option<IdentitySpec>,
    eq: &EventQuery,
) -> Result<(), QueryCompileError> {
    merge_event_kind(event_spec, eq.event.clone())?;
    merge_identity(identity_spec, eq.identity.clone())?;
    Ok(())
}

fn merge_event_kind(
    target: &mut Option<EventSpec>,
    candidate: EventSpec,
) -> Result<(), QueryCompileError> {
    if let Some(existing) = target {
        if *existing != candidate {
            // Event kinds must be compatible. For now, exact match required.
            return Err(QueryCompileError::ContradictoryPredicate {
                variable: VarId::new(0),
                detail: ContradictionKind::EventKind,
            });
        }
    } else {
        *target = Some(candidate);
    }
    Ok(())
}

fn merge_identity(
    target: &mut Option<IdentitySpec>,
    candidate: IdentitySpec,
) -> Result<(), QueryCompileError> {
    if let Some(existing) = target {
        if *existing != candidate {
            // Incompatible identities are a contradiction.
            return Err(QueryCompileError::ContradictoryPredicate {
                variable: VarId::new(0),
                detail: ContradictionKind::StrictIdentity,
            });
        }
    } else {
        *target = Some(candidate);
    }
    Ok(())
}

fn merge_subject_relation(
    target: &mut NormalizedSubject,
    candidate: NormalizedSubject,
) -> Result<(), QueryCompileError> {
    if !matches!(target, NormalizedSubject::Direct) && *target != candidate {
        return Err(QueryCompileError::ContradictoryPredicate {
            variable: VarId::new(0),
            detail: ContradictionKind::SubjectRelation,
        });
    }
    if !matches!(candidate, NormalizedSubject::Direct) {
        *target = candidate;
    }
    Ok(())
}
