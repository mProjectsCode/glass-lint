use std::collections::HashMap;

use super::error::{QueryCompileError, is_identity_empty, is_valid_identity_event_pair};
use crate::api::rule::query::{
    AnyExpr, EventQuery, EventSpec, IdentitySpec, QueryDecl, QueryExpr, QueryExprKind,
    QueryPredicate, VarId, VarType,
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
        EventSpec::MemberCall { .. }
        | EventSpec::MemberRead { .. }
        | EventSpec::PropertyWrite { .. } => VarType::MemberEvent,
        EventSpec::ClassReference | EventSpec::Import | EventSpec::StringReference => {
            VarType::Event
        }
    }
}

fn var_type_for_event_kind(kind: &EventSpec) -> VarType {
    match kind {
        EventSpec::Call | EventSpec::Construct => VarType::CallEvent,
        EventSpec::MemberCall { .. }
        | EventSpec::MemberRead { .. }
        | EventSpec::PropertyWrite { .. } => VarType::MemberEvent,
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

// ── Consolidated scope + types pass ──────────────────────────────────────

/// Consolidated variable-collection and type-checking pass.
///
/// Collects bindings and infers variable types in a single recursive
/// traversal, combining the work of the former `pass_variable_collection`
/// and `pass_type_checking` passes.
pub(crate) fn pass_scope_types(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    let mut seen: Vec<VarId> = Vec::new();
    let mut var_types: HashMap<VarId, VarType> = HashMap::new();
    collect_scope_and_types(decl.expression(), &mut seen, &mut var_types)?;

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

fn collect_scope_and_types(
    expr: &QueryExpr,
    seen: &mut Vec<VarId>,
    types: &mut HashMap<VarId, VarType>,
) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::Event(eq) => {
            if seen.contains(&eq.var()) {
                return Err(QueryCompileError::DuplicateBinding { var: eq.var() });
            }
            seen.push(eq.var());
            let ty = var_type_for_event(eq.event(), eq.identity());
            set_type_internal(eq.var(), ty, types)?;
        }
        QueryExprKind::SelectEvent(s) => {
            if seen.contains(&s.bind) {
                return Err(QueryCompileError::DuplicateBinding { var: s.bind });
            }
            seen.push(s.bind);
            set_type_internal(s.bind, VarType::Event, types)?;
        }
        QueryExprKind::Require(p) => collect_require_scope(p, seen, types)?,
        QueryExprKind::Any(any) => collect_any_scope(any, types)?,
        QueryExprKind::All(all) => {
            for branch in all.iter() {
                collect_scope_and_types(branch, seen, types)?;
            }
        }
        QueryExprKind::Lifecycle(lc) => {
            for src in lc.sources() {
                let mut src_seen = Vec::new();
                if src_seen.contains(&src.var()) {
                    return Err(QueryCompileError::DuplicateBinding { var: src.var() });
                }
                src_seen.push(src.var());
                let ty = var_type_for_event(src.event(), src.identity());
                set_type_internal(src.var(), ty, types)?;
            }
        }
    }
    Ok(())
}

fn collect_require_scope(
    pred: &QueryPredicate,
    seen: &mut Vec<VarId>,
    types: &mut HashMap<VarId, VarType>,
) -> Result<(), QueryCompileError> {
    match pred {
        QueryPredicate::ReturnedObject { bind, .. }
        | QueryPredicate::ConstructedObject { bind, .. } => {
            if seen.contains(bind) {
                return Err(QueryCompileError::DuplicateBinding { var: *bind });
            }
            seen.push(*bind);
            set_type_internal(*bind, VarType::Object, types)?;
        }
        QueryPredicate::EventKind { event, expected } => {
            if !seen.contains(event) {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: *event,
                });
            }
            let implied = var_type_for_event_kind(expected);
            check_type_internal(*event, implied, types)?;
        }
        QueryPredicate::EventIdentity { event, .. } => {
            if !seen.contains(event) {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: *event,
                });
            }
            check_type_internal(*event, VarType::Event, types)?;
        }
        QueryPredicate::Argument { call, .. } => {
            if !seen.contains(call) {
                return Err(QueryCompileError::MissingBinding { primary_var: *call });
            }
            check_type_internal(*call, VarType::CallEvent, types)?;
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
            check_type_internal(*event, VarType::MemberEvent, types)?;
            check_type_internal(*object, VarType::Object, types)?;
        }
    }
    Ok(())
}

fn collect_any_scope(
    any: &AnyExpr,
    types: &mut HashMap<VarId, VarType>,
) -> Result<(), QueryCompileError> {
    let mut merged: HashMap<VarId, VarType> = HashMap::new();
    for branch in any.iter() {
        let mut branch_seen = Vec::new();
        let mut branch_types = HashMap::new();
        collect_scope_and_types(branch, &mut branch_seen, &mut branch_types)?;
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
    Ok(())
}

fn set_type_internal(
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

fn check_type_internal(
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
