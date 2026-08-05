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
    let branch_vars: Vec<Vec<VarId>> = all.iter().map(QueryExpr::vars).collect();

    // Single branch — normalize as-is (should be rare after construction).
    if all.len() == 1 {
        return normalize_root(all.iter().next().expect("validated All branch"), emission);
    }

    // Find the common event variable that all branches share.
    let branches = all.iter().collect::<Vec<_>>();
    find_common_event_var(&branches).map_or_else(
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
        |var| merge_same_event(all, var),
    )
}

/// Find a variable bound as an event by the first branch that also
/// appears in every other branch (directly referenced or correlated).
///
/// `ReturnedObject` and `ConstructedObject` predicates only bind new
/// object variables; they do not reference the event variable.  The
/// correlation is via a separate `MemberSubject` predicate.  These
/// binding-only predicates are accepted as not breaking the chain.
fn find_common_event_var(branches: &[&QueryExpr]) -> Option<VarId> {
    if branches.is_empty() {
        return None;
    }
    // Collect binding vars from the first branch.
    let first_bindings = branches[0].binding_vars();
    for var in &first_bindings {
        if branches.iter().skip(1).all(|b| {
            b.contains_var(*var)
                || matches!(
                    b.kind(),
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
fn merge_same_event(all: &AllExpr, event_var: VarId) -> Result<NormalizedRoot, QueryCompileError> {
    let mut merge = SameEventMerge::new(event_var);

    for branch in all.iter() {
        match branch.kind() {
            QueryExprKind::Event(eq) => {
                merge.merge_event(eq)?;
            }
            QueryExprKind::SelectEvent(_) => {
                // Just a binding reference, no fields to merge.
            }
            QueryExprKind::Require(predicate) => merge.merge_predicate(predicate, event_var)?,
            _ => {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "unexpected branch kind in same-event All".into(),
                });
            }
        }
    }

    merge.finish(event_var)
}

fn var_to_slot(var: VarId) -> u32 {
    var.get()
}

struct SameEventMerge {
    event_var: VarId,
    event: Option<EventSpec>,
    identity: Option<IdentitySpec>,
    subject: Option<NormalizedSubject>,
    constraints: Vec<ArgumentConstraint>,
}

impl SameEventMerge {
    fn new(event_var: VarId) -> Self {
        Self {
            event_var,
            event: None,
            identity: None,
            subject: None,
            constraints: Vec::new(),
        }
    }

    fn merge_event(&mut self, query: &EventQuery) -> Result<(), QueryCompileError> {
        self.merge_event_kind(query.event().clone())?;
        self.merge_identity(query.identity().clone())?;
        self.constraints.extend(query.constraints().iter().cloned());
        Ok(())
    }

    fn merge_predicate(
        &mut self,
        predicate: &QueryPredicate,
        event_var: VarId,
    ) -> Result<(), QueryCompileError> {
        match predicate {
            QueryPredicate::EventKind { expected, .. } => {
                self.merge_event_kind(expected.clone())?;
            }
            QueryPredicate::EventIdentity { expected, .. } => {
                self.merge_identity(expected.clone())?;
            }
            QueryPredicate::Argument { index, matcher, .. } => {
                self.constraints
                    .push(ArgumentConstraint::new(*index, matcher.clone()));
            }
            QueryPredicate::ReturnedObject { bind, identity } => {
                self.merge_subject(NormalizedSubject::Returned {
                    producer: identity.clone(),
                    object_slot: var_to_slot(*bind),
                })?;
            }
            QueryPredicate::ConstructedObject { bind, identity } => {
                self.merge_subject(NormalizedSubject::Instance {
                    constructor: identity.clone(),
                    object_slot: var_to_slot(*bind),
                })?;
            }
            QueryPredicate::MemberSubject { event, object } => {
                self.merge_member_subject(*event, *object, event_var)?;
            }
        }
        Ok(())
    }

    fn merge_event_kind(&mut self, candidate: EventSpec) -> Result<(), QueryCompileError> {
        if let Some(existing) = &self.event {
            if *existing != candidate {
                return Err(QueryCompileError::ContradictoryPredicate {
                    variable: self.event_var,
                    detail: ContradictionKind::EventKind,
                });
            }
        } else {
            self.event = Some(candidate);
        }
        Ok(())
    }

    fn merge_identity(&mut self, candidate: IdentitySpec) -> Result<(), QueryCompileError> {
        if let Some(existing) = &self.identity {
            if *existing != candidate {
                return Err(QueryCompileError::ContradictoryPredicate {
                    variable: self.event_var,
                    detail: ContradictionKind::StrictIdentity,
                });
            }
        } else {
            self.identity = Some(candidate);
        }
        Ok(())
    }

    fn merge_subject(&mut self, candidate: NormalizedSubject) -> Result<(), QueryCompileError> {
        if self
            .subject
            .as_ref()
            .is_some_and(|subject| *subject != candidate)
        {
            return Err(QueryCompileError::ContradictoryPredicate {
                variable: self.event_var,
                detail: ContradictionKind::SubjectRelation,
            });
        }
        self.subject = Some(candidate);
        Ok(())
    }

    fn merge_member_subject(
        &self,
        event: VarId,
        object: VarId,
        event_var: VarId,
    ) -> Result<(), QueryCompileError> {
        if event != event_var {
            return Err(QueryCompileError::UncorrelatedConjunction);
        }
        match self.subject.as_ref() {
            Some(
                NormalizedSubject::Returned { object_slot, .. }
                | NormalizedSubject::Instance { object_slot, .. },
            ) if *object_slot == var_to_slot(object) => Ok(()),
            _ => Err(QueryCompileError::UncorrelatedConjunction),
        }
    }

    fn finish(mut self, event_var: VarId) -> Result<NormalizedRoot, QueryCompileError> {
        let event = self
            .event
            .ok_or_else(|| QueryCompileError::InternalInvariant {
                detail: "same-event All missing event kind".into(),
            })?;
        let identity = self
            .identity
            .ok_or_else(|| QueryCompileError::InternalInvariant {
                detail: "same-event All missing identity".into(),
            })?;

        self.constraints.sort_by(|a, b| {
            a.index()
                .cmp(&b.index())
                .then_with(|| a.predicate().cmp(b.predicate()))
        });
        self.constraints.dedup();

        let subject = self.subject.unwrap_or_else(|| NormalizedSubject::Direct {
            identity: identity.clone(),
        });
        detect_event_contradictions(event_var, &event, &identity, &subject, &self.constraints)?;

        Ok(NormalizedRoot::Event(NormalizedEvent {
            slot: var_to_slot(event_var),
            event,
            subject,
            arguments: CanonicalArgumentConstraints::from_canonicalized(&self.constraints),
        }))
    }
}
