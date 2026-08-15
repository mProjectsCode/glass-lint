use std::collections::{HashMap, hash_map::Entry};

use super::error::{QueryCompileError, is_identity_empty, is_valid_identity_event_pair};
use crate::api::rule::query::{
    AnyExpr, EventQuery, QueryDecl, QueryExpr, QueryExprKind, QueryPredicate, VarId, VarType,
};

/// Validate an event query for well-formedness.
pub(crate) fn validate_event_query(eq: &EventQuery) -> Result<(), QueryCompileError> {
    if !is_valid_identity_event_pair(eq.identity(), eq.event()) {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: "direct".to_string(),
            detail: "identity/event combination cannot select a semantic fact",
        });
    }
    if is_identity_empty(eq.identity()) {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: "direct".to_string(),
            detail: "identity name or pattern is empty",
        });
    }
    if !eq.constraints().is_empty() && !eq.event().supports_arguments() {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: "direct".to_string(),
            detail: "argument constraints require a call-bearing event",
        });
    }
    Ok(())
}

/// Return true if `actual` is compatible with `expected`.
/// Event is compatible with CallEvent and MemberEvent (Event is a supertype).
fn types_compatible(actual: VarType, expected: VarType) -> bool {
    actual == expected
        || (actual == VarType::Event
            && (expected == VarType::CallEvent || expected == VarType::MemberEvent))
        || ((actual == VarType::CallEvent || actual == VarType::MemberEvent)
            && expected == VarType::Event)
}

/// Return true if `candidate` is more specific than `existing`.
fn is_more_specific(candidate: VarType, existing: VarType) -> bool {
    matches!(
        (candidate, existing),
        (VarType::CallEvent | VarType::MemberEvent, VarType::Event)
    )
}

// ── Consolidated scope + types pass ──────────────────────────────────────

/// Consolidated variable-collection and type-checking pass.
///
/// Collects bindings and infers variable types in a single recursive
/// traversal, combining the work of the former `pass_variable_collection`
/// and `pass_type_checking` passes.
pub(crate) fn pass_scope_types(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    let mut context = ScopeTypes::default();
    context.collect(decl.expression())?;
    context.validate_primary(decl.emission().primary_var())
}

#[derive(Default)]
struct ScopeTypes {
    bindings: Vec<VarId>,
    types: HashMap<VarId, VarType>,
}

impl ScopeTypes {
    fn collect(&mut self, expr: &QueryExpr) -> Result<(), QueryCompileError> {
        match expr.kind() {
            QueryExprKind::Event(eq) => self.bind(eq.var(), eq.event().variable_type()),
            QueryExprKind::SelectEvent(s) => self.bind(s.bind, VarType::Event),
            QueryExprKind::Require(predicate) => self.collect_require(predicate),
            QueryExprKind::Any(any) => self.collect_any(any),
            QueryExprKind::All(all) => {
                for branch in all.iter() {
                    self.collect(branch)?;
                }
                Ok(())
            }
            QueryExprKind::Lifecycle(lc) => {
                for src in lc.sources() {
                    self.set_type(src.var(), src.event().variable_type())?;
                }
                Ok(())
            }
        }
    }

    fn collect_require(&mut self, predicate: &QueryPredicate) -> Result<(), QueryCompileError> {
        match predicate {
            QueryPredicate::ReturnedObject { bind, .. }
            | QueryPredicate::ConstructedObject { bind, .. } => self.bind(*bind, VarType::Object),
            QueryPredicate::EventKind { event, expected } => {
                self.require_binding(*event)?;
                self.check_type(*event, expected.variable_type())
            }
            QueryPredicate::EventIdentity { event, .. } => {
                self.require_binding(*event)?;
                self.check_type(*event, VarType::Event)
            }
            QueryPredicate::Argument { call, .. } => {
                self.require_binding(*call)?;
                self.check_type(*call, VarType::CallEvent)
            }
            QueryPredicate::MemberSubject { event, object } => {
                self.require_binding(*event)?;
                self.require_binding(*object)?;
                self.check_type(*event, VarType::MemberEvent)?;
                self.check_type(*object, VarType::Object)
            }
        }
    }

    fn collect_any(&mut self, any: &AnyExpr) -> Result<(), QueryCompileError> {
        let mut merged = Self::default();
        for branch in any.iter() {
            let mut branch_context = Self::default();
            branch_context.collect(branch)?;
            merged.merge_branch(branch_context)?;
        }
        self.types.extend(merged.types);
        Ok(())
    }

    fn merge_branch(&mut self, branch: Self) -> Result<(), QueryCompileError> {
        for (var, ty) in branch.types {
            match self.types.entry(var) {
                Entry::Occupied(mut entry) => {
                    let existing = *entry.get();
                    if !types_compatible(ty, existing) && !types_compatible(existing, ty) {
                        return Err(QueryCompileError::IncompatibleBranchOutput {
                            var,
                            type_a: existing.variant_name(),
                            type_b: ty.variant_name(),
                        });
                    }
                    if is_more_specific(ty, existing) {
                        *entry.get_mut() = ty;
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(ty);
                }
            }
        }
        Ok(())
    }

    fn bind(&mut self, var: VarId, ty: VarType) -> Result<(), QueryCompileError> {
        if self.bindings.contains(&var) {
            return Err(QueryCompileError::DuplicateBinding { var });
        }
        self.bindings.push(var);
        self.set_type(var, ty)
    }

    fn require_binding(&self, var: VarId) -> Result<(), QueryCompileError> {
        if self.bindings.contains(&var) {
            Ok(())
        } else {
            Err(QueryCompileError::MissingBinding { primary_var: var })
        }
    }

    fn set_type(&mut self, var: VarId, ty: VarType) -> Result<(), QueryCompileError> {
        let Some(existing) = self.types.get(&var).copied() else {
            self.types.insert(var, ty);
            return Ok(());
        };
        if !types_compatible(existing, ty) {
            return Err(QueryCompileError::TypeMismatch {
                var,
                expected: ty.variant_name(),
                actual: existing.variant_name(),
            });
        }
        if is_more_specific(ty, existing) {
            self.types.insert(var, ty);
        }
        Ok(())
    }

    fn check_type(&mut self, var: VarId, expected: VarType) -> Result<(), QueryCompileError> {
        let Some(actual) = self.types.get(&var).copied() else {
            self.types.insert(var, expected);
            return Ok(());
        };
        if !types_compatible(actual, expected) {
            return Err(QueryCompileError::TypeMismatch {
                var,
                expected: expected.variant_name(),
                actual: actual.variant_name(),
            });
        }
        if is_more_specific(expected, actual) {
            self.types.insert(var, expected);
        }
        Ok(())
    }

    fn validate_primary(&self, primary: VarId) -> Result<(), QueryCompileError> {
        match self.types.get(&primary) {
            None => Err(QueryCompileError::MissingBinding {
                primary_var: primary,
            }),
            Some(VarType::Event | VarType::CallEvent | VarType::MemberEvent) => Ok(()),
            Some(VarType::Object) => {
                Err(QueryCompileError::UnavailablePrimaryLocation { var: primary })
            }
        }
    }
}
