use std::collections::HashMap;

use super::error::{QueryCompileError, is_identity_empty, is_valid_identity_event_pair};
use crate::api::rule::query::{
    EmissionDecl, EventQuery, EventSpec, IdentitySpec, QueryDecl, QueryExpr, QueryExprKind,
    QueryPredicate, VarId, VarType,
};

// ── Individual validation passes ─────────────────────────────────────────

/// Pass 1: Declaration well-formedness.
///
/// Rejects invalid identity/event/subject combinations, empty identities,
/// and constraints on non-call events.
pub(crate) fn pass_well_formedness(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    match &decl.expression().kind {
        QueryExprKind::Event(eq) => {
            validate_event_query(eq)?;
        }
        QueryExprKind::SelectEvent(_) | QueryExprKind::Require(_) => {}
        QueryExprKind::Any(any) => {
            for branch in &any.branches {
                pass_well_formedness_inner(branch)?;
            }
        }
        QueryExprKind::All(all) => {
            for branch in &all.branches {
                pass_well_formedness_inner(branch)?;
            }
        }
        QueryExprKind::Lifecycle(lc) => {
            for src in lc.sources() {
                validate_event_query(src)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn pass_well_formedness_inner(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::Event(eq) => validate_event_query(eq),
        QueryExprKind::SelectEvent(_) => Ok(()),
        QueryExprKind::Require(predicate) => match predicate {
            QueryPredicate::ReturnedObject { identity, .. }
                if !matches!(identity, IdentitySpec::Rooted { .. }) =>
            {
                Err(QueryCompileError::UnsupportedRelation {
                    relation: "returned_object",
                    detail: "returned objects require a rooted producer identity".into(),
                })
            }
            QueryPredicate::ConstructedObject { identity, .. }
                if !matches!(
                    identity,
                    IdentitySpec::ModuleExport { .. } | IdentitySpec::PackageModuleExport { .. }
                ) =>
            {
                Err(QueryCompileError::UnsupportedRelation {
                    relation: "constructed_object",
                    detail: "constructed objects require a module export identity".into(),
                })
            }
            _ => Ok(()),
        },
        QueryExprKind::Any(any) => {
            for b in &any.branches {
                pass_well_formedness_inner(b)?;
            }
            Ok(())
        }
        QueryExprKind::All(all) => {
            for b in &all.branches {
                pass_well_formedness_inner(b)?;
            }
            Ok(())
        }
        QueryExprKind::Lifecycle(lc) => {
            for src in lc.sources() {
                validate_event_query(src)?;
            }
            Ok(())
        }
    }
}

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
    if !eq.constraints().is_empty() && !event_supports_constraints(eq.event()) {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: "direct".to_string(),
            detail: "argument constraints require a call-bearing event",
        });
    }
    Ok(())
}

use super::error::event_supports_constraints;

/// Pass 2: Variable binding collection.
///
/// Semantics:
/// - `Any` branches have independent binding scopes.
/// - `All` branches share one correlation scope.
/// - `SelectEvent` and `Event` atoms bind their variable.
/// - `Require` atoms (except `ReturnedObject`/`ConstructedObject`) only
///   reference variables.
/// - `ReturnedObject` and `ConstructedObject` bind their object variable.
pub(crate) fn pass_variable_collection(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    collect_and_check_scope(decl.expression(), &mut Vec::new(), false)?;
    Ok(())
}

/// Recursively check binding scopes.
///
/// `seen` accumulates bindings in the current scope.
/// `has_separate_branch_scopes` controls whether branches of the current
/// composite operator introduce independent scopes (true for `Any`) or share
/// the caller's scope (true for `All` and top-level).
fn collect_and_check_scope(
    expr: &QueryExpr,
    seen: &mut Vec<VarId>,
    _has_separate_branch_scopes: bool,
) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::Event(eq) => {
            // Event always binds its var.
            if seen.contains(&eq.var()) {
                return Err(QueryCompileError::DuplicateBinding { var: eq.var() });
            }
            seen.push(eq.var());
        }
        QueryExprKind::SelectEvent(s) => {
            if seen.contains(&s.bind) {
                return Err(QueryCompileError::DuplicateBinding { var: s.bind });
            }
            seen.push(s.bind);
        }
        QueryExprKind::Require(p) => match p {
            // ReturnedObject and ConstructedObject bind their object variable.
            QueryPredicate::ReturnedObject { bind, .. }
            | QueryPredicate::ConstructedObject { bind, .. } => {
                if seen.contains(bind) {
                    return Err(QueryCompileError::DuplicateBinding { var: *bind });
                }
                seen.push(*bind);
            }
            // Other Require atoms only reference previously bound vars.
            QueryPredicate::EventKind { event, .. }
            | QueryPredicate::EventIdentity { event, .. } => {
                if !seen.contains(event) {
                    return Err(QueryCompileError::MissingBinding {
                        primary_var: *event,
                    });
                }
            }
            QueryPredicate::Argument { call, .. } => {
                if !seen.contains(call) {
                    return Err(QueryCompileError::MissingBinding { primary_var: *call });
                }
            }
            QueryPredicate::MemberSubject { event, object } => {
                if !seen.contains(event) {
                    return Err(QueryCompileError::MissingBinding {
                        primary_var: *event,
                    });
                }
                if !seen.contains(object) {
                    return Err(QueryCompileError::MissingBinding {
                        primary_var: *object,
                    });
                }
            }
        },
        QueryExprKind::Any(any) => {
            // Each Any branch has an independent scope.
            for branch in &any.branches {
                let mut branch_seen = Vec::new();
                collect_and_check_scope(branch, &mut branch_seen, false)?;
            }
        }
        QueryExprKind::All(all) => {
            // All branches share the same scope (add bindings to `seen`).
            for branch in &all.branches {
                collect_and_check_scope(branch, seen, false)?;
            }
        }
        QueryExprKind::Lifecycle(lc) => {
            // Lifecycle sources have Any-like independent scopes (each
            // source can independently start the lifecycle).
            for src in lc.sources() {
                let mut src_seen = Vec::new();
                if src_seen.contains(&src.var()) {
                    return Err(QueryCompileError::DuplicateBinding { var: src.var() });
                }
                src_seen.push(src.var());
            }
        }
    }
    Ok(())
}

// ── Pass 3: Variable type inference/checking ─────────────────────────────

/// Infers a [`VarType`] for every variable in the expression tree and
/// checks that:
/// - Every variable has a consistent type across all uses.
/// - The emission primary variable is an event type (has a source location).
/// - No type mismatch exists (e.g. treating a static value as an event).
pub(crate) fn pass_type_checking(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    let mut var_types: HashMap<VarId, VarType> = HashMap::new();
    infer_types(decl.expression(), decl.emission(), &mut var_types)?;

    // Verify the emission primary variable is event-typed.
    let primary = decl.emission().primary_var();
    var_types.get(&primary).map_or(
        Err(QueryCompileError::MissingBinding {
            primary_var: primary,
        }),
        |ty| match ty {
            VarType::Event | VarType::CallEvent | VarType::MemberEvent => Ok(()),
            VarType::Object => Err(QueryCompileError::UnavailablePrimaryLocation { var: primary }),
        },
    )
}

#[allow(clippy::only_used_in_recursion)]
fn infer_types(
    expr: &QueryExpr,
    emission: &EmissionDecl,
    types: &mut HashMap<VarId, VarType>,
) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::Event(eq) => {
            let ty = var_type_for_event(eq.event(), eq.identity());
            set_type(eq.var(), ty, types)?;
        }
        QueryExprKind::SelectEvent(s) => {
            // Without full EventSpec/IdentitySpec, we use a generic Event type.
            set_type(s.bind, VarType::Event, types)?;
        }
        QueryExprKind::Require(p) => match p {
            QueryPredicate::EventKind { event, expected } => {
                // The kind constrains what type the event variable must be.
                let implied = var_type_for_event_kind(expected);
                check_type(*event, implied, types)?;
            }
            QueryPredicate::EventIdentity { event, .. } => {
                check_type(*event, VarType::Event, types)?;
            }
            QueryPredicate::Argument { call, .. } => {
                check_type(*call, VarType::CallEvent, types)?;
            }
            QueryPredicate::ReturnedObject { bind, .. }
            | QueryPredicate::ConstructedObject { bind, .. } => {
                set_type(*bind, VarType::Object, types)?;
            }
            QueryPredicate::MemberSubject { event, object } => {
                check_type(*event, VarType::MemberEvent, types)?;
                check_type(*object, VarType::Object, types)?;
            }
        },
        QueryExprKind::Any(any) => {
            // Each branch is independently typed. Merge branch types
            // back to the outer scope so that the primary variable's
            // type is visible after Any. Incompatible types on the
            // same variable across branches are an error.
            let mut merged: HashMap<VarId, VarType> = HashMap::new();
            for branch in &any.branches {
                let mut branch_types = HashMap::new();
                infer_types(branch, emission, &mut branch_types)?;
                for (var, ty) in branch_types {
                    match merged.entry(var) {
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
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
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(ty);
                        }
                    }
                }
            }
            types.extend(merged);
        }
        QueryExprKind::All(all) => {
            for branch in &all.branches {
                infer_types(branch, emission, types)?;
            }
        }
        QueryExprKind::Lifecycle(lc) => {
            for src in lc.sources() {
                let ty = var_type_for_event(src.event(), src.identity());
                set_type(src.var(), ty, types)?;
            }
        }
    }
    Ok(())
}

fn set_type(
    var: VarId,
    ty: VarType,
    types: &mut HashMap<VarId, VarType>,
) -> Result<(), QueryCompileError> {
    if let Some(existing) = types.get(&var) {
        if !types_compatible(*existing, ty) {
            return Err(QueryCompileError::TypeMismatch {
                var,
                expected: ty.variant_name(),
                actual: existing.variant_name(),
            });
        }
        if is_more_specific(ty, *existing) {
            types.insert(var, ty);
        }
        return Ok(());
    }
    types.insert(var, ty);
    Ok(())
}

fn check_type(
    var: VarId,
    expected: VarType,
    types: &mut HashMap<VarId, VarType>,
) -> Result<(), QueryCompileError> {
    if let Some(actual) = types.get(&var) {
        if !types_compatible(*actual, expected) {
            return Err(QueryCompileError::TypeMismatch {
                var,
                expected: expected.variant_name(),
                actual: actual.variant_name(),
            });
        }
        if is_more_specific(expected, *actual) {
            types.insert(var, expected);
        }
        return Ok(());
    }
    types.insert(var, expected);
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

fn var_type_for_event(event: &EventSpec, _identity: &IdentitySpec) -> VarType {
    match event {
        EventSpec::Call | EventSpec::Construct => VarType::CallEvent,
        EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. } => VarType::MemberEvent,
        EventSpec::ClassReference | EventSpec::Import | EventSpec::StringReference => {
            VarType::Event
        }
    }
}

fn var_type_for_event_kind(kind: &EventSpec) -> VarType {
    match kind {
        EventSpec::Call | EventSpec::Construct => VarType::CallEvent,
        EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. } => VarType::MemberEvent,
        EventSpec::ClassReference | EventSpec::Import | EventSpec::StringReference => {
            VarType::Event
        }
    }
}

impl VarType {
    fn variant_name(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::CallEvent => "call_event",
            Self::MemberEvent => "member_event",
            Self::Object => "object",
        }
    }
}
