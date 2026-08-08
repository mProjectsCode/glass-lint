// ── All normalization ─────────────────────────────────────────────────────

use super::normalize::normalize_root;
use crate::api::{
    compiler::{
        contradiction::detect_event_contradictions,
        normalized::{
            CanonicalArgumentConstraints, EventSlot, NormalizedEvent, NormalizedRoot,
            NormalizedSubject, ObjectSlot,
        },
        validate::{ContradictionKind, QueryCompileError},
    },
    rule::{
        ArgumentConstraint,
        query::{
            AllExpr, EmissionDecl, EventQuery, EventSpec, IdentitySpec, QueryExpr, QueryExprKind,
            QueryPredicate, QueryShapeFacts, VarId,
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
    let branch_facts: Vec<QueryShapeFacts> = all.iter().map(QueryExpr::shape_facts).collect();

    // Single branch — normalize as-is (should be rare after construction).
    if all.len() == 1 {
        return normalize_root(all.iter().next().expect("validated All branch"), emission);
    }

    // Find the common event variable that all branches share.
    let branches = all.iter().collect::<Vec<_>>();
    find_common_event_var(&branches, &branch_facts).map_or_else(
        || {
            // No shared variable — check correlation scope.
            let all_share_some = branch_facts
                .first()
                .is_some_and(|first| {
                    branch_facts
                        .iter()
                        .skip(1)
                        .any(|facts| {
                            facts
                                .variables()
                                .iter()
                                .any(|v| first.variables().contains(v))
                        })
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
fn find_common_event_var(
    branches: &[&QueryExpr],
    branch_facts: &[QueryShapeFacts],
) -> Option<VarId> {
    if branches.is_empty() {
        return None;
    }
    // Collect binding vars from the first branch.
    for var in branch_facts[0].bindings() {
        if branches.iter().enumerate().skip(1).all(|(index, b)| {
            branch_facts[index].contains(*var)
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
    let mut merge = SameEventMerge::new(event_var, all)?;

    for branch in all.iter() {
        match branch.kind() {
            QueryExprKind::Event(eq) => {
                merge.merge_event(eq)?;
            }
            QueryExprKind::SelectEvent(_) => {
                // Just a binding reference, no fields to merge.
            }
            QueryExprKind::Require(predicate) => merge.merge_predicate(predicate)?,
            _ => {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "unexpected branch kind in same-event All".into(),
                });
            }
        }
    }

    merge.finish().into_root()
}

struct SameEventMerge {
    event_var: VarId,
    event: EventSpec,
    identity: IdentitySpec,
    subject: Option<NormalizedSubject>,
    constraints: Vec<ArgumentConstraint>,
    member_objects: Vec<VarId>,
}

impl SameEventMerge {
    fn new(event_var: VarId, all: &AllExpr) -> Result<Self, QueryCompileError> {
        let event = all
            .iter()
            .find_map(|branch| event_kind_for(branch, event_var))
            .ok_or(QueryCompileError::IncompleteSameEvent {
                missing: "event kind",
            })?;
        let identity = all
            .iter()
            .find_map(|branch| event_identity_for(branch, event_var))
            .ok_or(QueryCompileError::IncompleteSameEvent {
                missing: "identity",
            })?;
        Ok(Self {
            event_var,
            event,
            identity,
            subject: None,
            constraints: Vec::new(),
            member_objects: Vec::new(),
        })
    }

    fn merge_event(&mut self, query: &EventQuery) -> Result<(), QueryCompileError> {
        self.merge_event_kind(query.event())?;
        self.merge_identity(query.identity())?;
        self.constraints.extend(query.constraints().iter().cloned());
        Ok(())
    }

    fn merge_predicate(&mut self, predicate: &QueryPredicate) -> Result<(), QueryCompileError> {
        match predicate {
            QueryPredicate::EventKind { expected, .. } => {
                self.merge_event_kind(expected)?;
            }
            QueryPredicate::EventIdentity { expected, .. } => {
                self.merge_identity(expected)?;
            }
            QueryPredicate::Argument { index, matcher, .. } => {
                self.constraints
                    .push(ArgumentConstraint::new(*index, matcher.clone()));
            }
            QueryPredicate::ReturnedObject { bind, identity } => {
                self.merge_subject(NormalizedSubject::Returned {
                    producer: identity.clone(),
                    object_slot: ObjectSlot::from_var(*bind),
                })?;
            }
            QueryPredicate::ConstructedObject { bind, identity } => {
                self.merge_subject(NormalizedSubject::Instance {
                    constructor: identity.clone(),
                    object_slot: ObjectSlot::from_var(*bind),
                })?;
            }
            QueryPredicate::MemberSubject { event, object } => {
                self.merge_member_subject(*event, *object)?;
            }
        }
        Ok(())
    }

    fn merge_event_kind(&self, candidate: &EventSpec) -> Result<(), QueryCompileError> {
        if self.event != *candidate {
            return Err(QueryCompileError::ContradictoryPredicate {
                variable: self.event_var,
                detail: ContradictionKind::EventKind,
            });
        }
        Ok(())
    }

    fn merge_identity(&self, candidate: &IdentitySpec) -> Result<(), QueryCompileError> {
        if self.identity != *candidate {
            return Err(QueryCompileError::ContradictoryPredicate {
                variable: self.event_var,
                detail: ContradictionKind::StrictIdentity,
            });
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
        &mut self,
        event: VarId,
        object: VarId,
    ) -> Result<(), QueryCompileError> {
        if event != self.event_var {
            return Err(QueryCompileError::UncorrelatedConjunction);
        }
        self.member_objects.push(object);
        Ok(())
    }

    fn finish(self) -> CompleteSameEventMerge {
        CompleteSameEventMerge {
            event_var: self.event_var,
            event: self.event,
            identity: self.identity,
            subject: self.subject,
            constraints: self.constraints,
            member_objects: self.member_objects,
        }
    }
}

fn event_kind_for(branch: &QueryExpr, event_var: VarId) -> Option<EventSpec> {
    match branch.kind() {
        QueryExprKind::Event(query) if query.var() == event_var => Some(query.event().clone()),
        QueryExprKind::Require(QueryPredicate::EventKind { event, expected })
            if *event == event_var =>
        {
            Some(expected.clone())
        }
        _ => None,
    }
}

fn event_identity_for(branch: &QueryExpr, event_var: VarId) -> Option<IdentitySpec> {
    match branch.kind() {
        QueryExprKind::Event(query) if query.var() == event_var => Some(query.identity().clone()),
        QueryExprKind::Require(QueryPredicate::EventIdentity { event, expected })
            if *event == event_var =>
        {
            Some(expected.clone())
        }
        _ => None,
    }
}

struct CompleteSameEventMerge {
    event_var: VarId,
    event: EventSpec,
    identity: IdentitySpec,
    subject: Option<NormalizedSubject>,
    constraints: Vec<ArgumentConstraint>,
    member_objects: Vec<VarId>,
}

impl CompleteSameEventMerge {
    fn into_root(self) -> Result<NormalizedRoot, QueryCompileError> {
        let Self {
            event_var,
            event,
            identity,
            subject,
            constraints,
            member_objects,
        } = self;

        let arguments = CanonicalArgumentConstraints::from_constraints(&constraints);
        let constraints = arguments.to_flat_vec();

        let subject = subject.unwrap_or_else(|| NormalizedSubject::Direct {
            identity: identity.clone(),
        });
        for object in member_objects {
            match &subject {
                NormalizedSubject::Returned { object_slot, .. }
                | NormalizedSubject::Instance { object_slot, .. }
                    if *object_slot == ObjectSlot::from_var(object) => {}
                _ => return Err(QueryCompileError::UncorrelatedConjunction),
            }
        }
        detect_event_contradictions(event_var, &event, &identity, &subject, &constraints)?;

        Ok(NormalizedRoot::Event(NormalizedEvent {
            slot: EventSlot::from_var(event_var),
            event,
            subject,
            arguments,
        }))
    }
}
