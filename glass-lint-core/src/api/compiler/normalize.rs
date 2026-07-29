//! Logical query normalization and canonicalization.
//!
//! Produces a canonical logical form suitable for deterministic planning,
//! deduplication, equivalence tests, and later language compilation.
//!
//! Normalization is idempotent: applying it twice produces the same result.
//! Equivalent logical queries have one canonical representation regardless
//! of incidental builder order.
//!
//! ## Normalization steps
//!
//! 1. **Flatten** nested `Any` and `All` by hoisting inner branches.
//! 2. **Deduplicate** equivalent branches using structural equality.
//! 3. **Sort** branches by a deterministic canonical order.
//! 4. **Reassign** variable slots to dense `0..n` ordering.
//! 5. **Compute** plan requirements (needed indexes, fact stream, etc.).

use std::collections::BTreeMap;

use crate::api::rule::query::{
    AllExpr, AnyExpr, EmissionDecl, EventQuery, IdentitySpec, LifecycleQuery, QueryDecl, QueryExpr,
    QueryExprKind, QueryPredicate, VarId,
};

// ── Plan requirements ─────────────────────────────────────────────────────

/// Requirements computed during normalization for physical planning.
///
/// These describe which execution subsystems a plan needs. Physical plan
/// selection (Phase 6) will use these to choose operators and reserve
/// resources.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PlanRequirements {
    /// Whether the plan needs occurrence indexes (always true for any query).
    pub(crate) needs_occurrence_indexes: bool,
    /// Whether the plan needs fact-stream projection (argument constraints).
    pub(crate) needs_fact_stream: bool,
    /// Whether the plan needs value resolution (static value matching).
    pub(crate) needs_value_resolution: bool,
    /// Whether the plan needs local flow tracking.
    pub(crate) needs_local_flow: bool,
    /// Whether the plan needs cross-call flow tracking.
    pub(crate) needs_cross_call_flow: bool,
    /// Whether the plan needs project identity overlays.
    pub(crate) needs_project_overlay: bool,
    /// Whether the plan needs evidence trace support.
    pub(crate) needs_evidence_trace: bool,
}

impl PlanRequirements {
    /// Compute requirements for a normalized query declaration.
    pub(crate) fn for_query(decl: &QueryDecl) -> Self {
        Self {
            needs_occurrence_indexes: true,
            needs_fact_stream: has_any_constraint(&decl.expression),
            needs_value_resolution: has_any_constraint(&decl.expression),
            needs_local_flow: matches!(&decl.expression.kind, QueryExprKind::Lifecycle(_)),
            needs_cross_call_flow: matches!(&decl.expression.kind, QueryExprKind::Lifecycle(_)),
            needs_project_overlay: requires_project_overlay(&decl.expression),
            needs_evidence_trace: true,
        }
    }

    /// Merge another set of requirements into this one (union semantics).
    pub(crate) fn merge_from(&mut self, other: &Self) {
        self.needs_occurrence_indexes |= other.needs_occurrence_indexes;
        self.needs_fact_stream |= other.needs_fact_stream;
        self.needs_value_resolution |= other.needs_value_resolution;
        self.needs_local_flow |= other.needs_local_flow;
        self.needs_cross_call_flow |= other.needs_cross_call_flow;
        self.needs_project_overlay |= other.needs_project_overlay;
        self.needs_evidence_trace |= other.needs_evidence_trace;
    }
}

fn has_any_constraint(expr: &QueryExpr) -> bool {
    match &expr.kind {
        QueryExprKind::Event(eq) => !eq.constraints.is_empty(),
        QueryExprKind::SelectEvent(_) | QueryExprKind::Require(_) | QueryExprKind::Lifecycle(_) => {
            false
        }
        QueryExprKind::Any(any) => any.branches.iter().any(has_any_constraint),
        QueryExprKind::All(all) => all.branches.iter().any(has_any_constraint),
    }
}

fn requires_project_overlay(expr: &QueryExpr) -> bool {
    match &expr.kind {
        QueryExprKind::Event(eq) => matches!(
            &eq.identity,
            IdentitySpec::ModuleExport { .. }
                | IdentitySpec::PackageModuleExport { .. }
                | IdentitySpec::ModuleNamespace { .. }
                | IdentitySpec::PackageModuleNamespace { .. }
        ),
        QueryExprKind::SelectEvent(_) | QueryExprKind::Require(_) => false,
        QueryExprKind::Any(any) => any.branches.iter().any(requires_project_overlay),
        QueryExprKind::All(all) => all.branches.iter().any(requires_project_overlay),
        QueryExprKind::Lifecycle(lc) => lc
            .sources
            .iter()
            .any(|src| requires_project_overlay(&QueryExpr::event(src.clone()))),
    }
}

// ── Entry points ──────────────────────────────────────────────────────────

/// Normalize a [`QueryDecl`] and compute its [`PlanRequirements`].
///
/// Returns the normalized query (same types, canonical form) and the
/// plan requirements derived from it.
pub(crate) fn normalize_query_decl(decl: &QueryDecl) -> (QueryDecl, PlanRequirements) {
    let expression = normalize_expr(&decl.expression);
    let (expression, var_map) = reassign_vars(&expression);
    let primary_var = var_map
        .get(&decl.emission.primary_var)
        .copied()
        .unwrap_or(decl.emission.primary_var);
    let normalized = QueryDecl {
        expression,
        emission: EmissionDecl {
            primary_var,
            kind: decl.emission.kind,
            symbol: decl.emission.symbol.clone(),
        },
    };
    let requirements = PlanRequirements::for_query(&normalized);
    (normalized, requirements)
}

/// Normalize a [`QueryExpr`] into canonical form.
///
/// 1. Flatten nested `Any` and `All` (hoist inner same-type branches).
/// 2. Sort branches by a deterministic canonical order.
/// 3. Remove exact duplicate branches (using `Eq`).
pub(crate) fn normalize_expr(expr: &QueryExpr) -> QueryExpr {
    match &expr.kind {
        QueryExprKind::Any(any) => {
            let mut branches = flatten_branches::<true>(&any.branches);
            sort_exprs(&mut branches);
            branches.dedup();
            // Must be non-empty (validated at construction)
            debug_assert!(!branches.is_empty(), "normalized Any must be non-empty");
            QueryExpr::any(AnyExpr { branches })
        }
        QueryExprKind::All(all) => {
            let mut branches = flatten_branches::<false>(&all.branches);
            sort_exprs(&mut branches);
            branches.dedup();
            debug_assert!(!branches.is_empty(), "normalized All must be non-empty");
            QueryExpr::all(AllExpr { branches })
        }
        QueryExprKind::Event(_)
        | QueryExprKind::SelectEvent(_)
        | QueryExprKind::Require(_)
        | QueryExprKind::Lifecycle(_) => expr.clone(),
    }
}

// ── Flattening ────────────────────────────────────────────────────────────

/// Flatten branches by hoisting nested same-type expressions.
///
/// `IS_ANY = true`  flattens nested `Any` within `Any`.
/// `IS_ANY = false` flattens nested `All` within `All`.
fn flatten_branches<const IS_ANY: bool>(branches: &[QueryExpr]) -> Vec<QueryExpr> {
    let mut result: Vec<QueryExpr> = Vec::new();
    for b in branches {
        let flat = normalize_expr(b);
        if IS_ANY {
            if let QueryExprKind::Any(inner) = &flat.kind {
                result.extend(inner.branches.clone());
            } else {
                result.push(flat);
            }
        } else if let QueryExprKind::All(inner) = &flat.kind {
            result.extend(inner.branches.clone());
        } else {
            result.push(flat);
        }
    }
    result
}

// ── Canonical ordering ────────────────────────────────────────────────────

/// Sort a slice of expressions in deterministic canonical order.
fn sort_exprs(exprs: &mut [QueryExpr]) {
    exprs.sort_by(compare_exprs);
}

/// Deterministic ordering for query expressions.
///
/// Order: `Event` < `Lifecycle` < `Any` < `All`.
/// Within the same variant, fields are compared lexicographically.
fn compare_exprs(a: &QueryExpr, b: &QueryExpr) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let disc_a = expr_discriminant(&a.kind);
    let disc_b = expr_discriminant(&b.kind);
    if disc_a != disc_b {
        return disc_a.cmp(&disc_b);
    }

    match (&a.kind, &b.kind) {
        (QueryExprKind::Event(ae), QueryExprKind::Event(be)) => compare_event_fields(ae, be),
        (QueryExprKind::Lifecycle(la), QueryExprKind::Lifecycle(lb)) => la
            .sources
            .len()
            .cmp(&lb.sources.len())
            .then_with(|| {
                la.sources
                    .iter()
                    .zip(lb.sources.iter())
                    .fold(std::cmp::Ordering::Equal, |acc, (a, b)| {
                        acc.then_with(|| compare_event_fields(a, b))
                    })
            })
            .then_with(|| la.condition.is_some().cmp(&lb.condition.is_some()))
            .then_with(|| la.completion.is_some().cmp(&lb.completion.is_some())),
        (QueryExprKind::Any(aa), QueryExprKind::Any(ba)) => {
            compare_branch_slices(&aa.branches, &ba.branches)
        }
        (QueryExprKind::All(aa), QueryExprKind::All(ba)) => {
            compare_branch_slices(&aa.branches, &ba.branches)
        }
        _ => Ordering::Equal,
    }
}

fn expr_discriminant(e: &QueryExprKind) -> u8 {
    match e {
        QueryExprKind::Event(_) => 0,
        QueryExprKind::SelectEvent(_) => 1,
        QueryExprKind::Require(_) => 2,
        QueryExprKind::Lifecycle(_) => 3,
        QueryExprKind::Any(_) => 4,
        QueryExprKind::All(_) => 5,
    }
}

fn compare_event_fields(a: &EventQuery, b: &EventQuery) -> std::cmp::Ordering {
    a.var
        .cmp(&b.var)
        .then_with(|| a.event.cmp(&b.event))
        .then_with(|| a.identity.cmp(&b.identity))
        .then_with(|| a.subject.cmp(&b.subject))
        .then_with(|| a.constraints.cmp(&b.constraints))
}

fn compare_branch_slices(a: &[QueryExpr], b: &[QueryExpr]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    a.len().cmp(&b.len()).then_with(|| {
        a.iter().zip(b.iter()).fold(Ordering::Equal, |acc, (x, y)| {
            acc.then_with(|| compare_exprs(x, y))
        })
    })
}

// ── Dense variable slot assignment ───────────────────────────────────────

/// Reassign variable slots to dense `0..n` ordering.
///
/// Variables are collected in pre-order (depth-first, left-to-right).
/// Returns the remapped expression and the mapping from old to new `VarId`.
fn reassign_vars(expr: &QueryExpr) -> (QueryExpr, BTreeMap<VarId, VarId>) {
    let mut old_vars = Vec::new();
    collect_vars_preorder(expr, &mut old_vars);

    // Remove duplicates while preserving the first occurrence order.
    // In a validated query each variable is bound exactly once, so there
    // should be no duplicates.  We dedup anyway for robustness.
    let mut seen = std::collections::BTreeSet::new();
    old_vars.retain(|v| seen.insert(*v));

    let var_map: BTreeMap<VarId, VarId> = old_vars
        .iter()
        .enumerate()
        .map(|(i, old)| {
            (
                *old,
                VarId::new(u32::try_from(i).expect("variable count fits in u32")),
            )
        })
        .collect();

    let new_expr = remap_vars(expr, &var_map);
    (new_expr, var_map)
}

fn collect_vars_preorder(expr: &QueryExpr, vars: &mut Vec<VarId>) {
    match &expr.kind {
        QueryExprKind::Event(eq) => vars.push(eq.var),
        QueryExprKind::SelectEvent(s) => vars.push(s.bind),
        QueryExprKind::Require(p) => match p {
            QueryPredicate::EventKind { event, .. }
            | QueryPredicate::EventIdentity { event, .. } => vars.push(*event),
            QueryPredicate::Argument { call, .. } => vars.push(*call),
            QueryPredicate::ReturnedObject { bind, .. }
            | QueryPredicate::ConstructedObject { bind, .. } => vars.push(*bind),
            QueryPredicate::MemberSubject { event, object } => {
                vars.push(*event);
                vars.push(*object);
            }
        },
        QueryExprKind::Any(any) => {
            for b in &any.branches {
                collect_vars_preorder(b, vars);
            }
        }
        QueryExprKind::All(all) => {
            for b in &all.branches {
                collect_vars_preorder(b, vars);
            }
        }
        QueryExprKind::Lifecycle(lc) => {
            for src in &lc.sources {
                vars.push(src.var);
            }
        }
    }
}

fn remap_vars(expr: &QueryExpr, var_map: &BTreeMap<VarId, VarId>) -> QueryExpr {
    match &expr.kind {
        QueryExprKind::Event(eq) => QueryExpr::event(EventQuery {
            var: var_map.get(&eq.var).copied().unwrap_or(eq.var),
            event: eq.event.clone(),
            identity: eq.identity.clone(),
            subject: eq.subject,
            constraints: eq.constraints.clone(),
        }),
        QueryExprKind::SelectEvent(s) => {
            QueryExpr::select_event(var_map.get(&s.bind).copied().unwrap_or(s.bind))
        }
        QueryExprKind::Require(p) => QueryExpr::require(remap_predicate(p, var_map)),
        QueryExprKind::Any(any) => {
            let branches: Vec<QueryExpr> = any
                .branches
                .iter()
                .map(|b| remap_vars(b, var_map))
                .collect();
            QueryExpr::any(AnyExpr { branches })
        }
        QueryExprKind::All(all) => {
            let branches: Vec<QueryExpr> = all
                .branches
                .iter()
                .map(|b| remap_vars(b, var_map))
                .collect();
            QueryExpr::all(AllExpr { branches })
        }
        QueryExprKind::Lifecycle(lc) => {
            let sources: Vec<EventQuery> = lc
                .sources
                .iter()
                .map(|src| EventQuery {
                    var: var_map.get(&src.var).copied().unwrap_or(src.var),
                    ..src.clone()
                })
                .collect();
            QueryExpr::lifecycle(LifecycleQuery {
                sources,
                condition: lc.condition.clone(),
                completion: lc.completion.clone(),
            })
        }
    }
}

fn remap_predicate(p: &QueryPredicate, var_map: &BTreeMap<VarId, VarId>) -> QueryPredicate {
    match p {
        QueryPredicate::EventKind { event, expected } => QueryPredicate::EventKind {
            event: var_map.get(event).copied().unwrap_or(*event),
            expected: expected.clone(),
        },
        QueryPredicate::EventIdentity { event, expected } => QueryPredicate::EventIdentity {
            event: var_map.get(event).copied().unwrap_or(*event),
            expected: expected.clone(),
        },
        QueryPredicate::Argument {
            call,
            index,
            matcher,
        } => QueryPredicate::Argument {
            call: var_map.get(call).copied().unwrap_or(*call),
            index: *index,
            matcher: matcher.clone(),
        },
        QueryPredicate::ReturnedObject { bind, identity } => QueryPredicate::ReturnedObject {
            bind: var_map.get(bind).copied().unwrap_or(*bind),
            identity: identity.clone(),
        },
        QueryPredicate::ConstructedObject { bind, identity } => QueryPredicate::ConstructedObject {
            bind: var_map.get(bind).copied().unwrap_or(*bind),
            identity: identity.clone(),
        },
        QueryPredicate::MemberSubject { event, object } => QueryPredicate::MemberSubject {
            event: var_map.get(event).copied().unwrap_or(*event),
            object: var_map.get(object).copied().unwrap_or(*object),
        },
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::SymbolPath;
    use smol_str::SmolStr;

    use super::*;
    use crate::api::{
        classification::MatchKind,
        rule::{
            ValueMatcher,
            query::{EmissionDecl, EventQuery, EventSpec, IdentitySpec, SubjectSpec},
        },
    };

    // ── Helpers ────────────────────────────────────────────────────

    fn event(var: u32, name: &str) -> QueryExpr {
        QueryExpr::event(EventQuery {
            var: VarId::new(var),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new(name),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        })
    }

    fn decl(expr: QueryExpr, primary_var: u32, symbol: &str) -> QueryDecl {
        QueryDecl {
            expression: expr,
            emission: EmissionDecl {
                primary_var: VarId::new(primary_var),
                kind: MatchKind::Call,
                symbol: symbol.into(),
            },
        }
    }

    // ── Flattening tests ───────────────────────────────────────────

    #[test]
    fn flattens_nested_any() {
        // inner has 2 branches; outer has 3 including the inner Any.
        // After flattening: 3 - 1 (inner Any) + 2 (its branches) = 4.
        let inner = AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap();
        let outer =
            AnyExpr::new(vec![event(2, "c"), QueryExpr::any(inner), event(3, "d")]).unwrap();
        let expr = QueryExpr::any(outer);
        let normalized = normalize_expr(&expr);
        match &normalized.kind {
            QueryExprKind::Any(a) => {
                assert_eq!(a.branches.len(), 4);
                for b in &a.branches {
                    assert!(matches!(&b.kind, QueryExprKind::Event(_)));
                }
            }
            _ => panic!("expected Any"),
        }
    }

    #[test]
    fn flattens_nested_all() {
        let inner = AllExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap();
        let outer =
            AllExpr::new(vec![event(2, "c"), QueryExpr::all(inner), event(3, "d")]).unwrap();
        let expr = QueryExpr::all(outer);
        let normalized = normalize_expr(&expr);
        match &normalized.kind {
            QueryExprKind::All(a) => {
                assert_eq!(a.branches.len(), 4);
                for b in &a.branches {
                    assert!(matches!(&b.kind, QueryExprKind::Event(_)));
                }
            }
            _ => panic!("expected All"),
        }
    }

    #[test]
    fn does_not_flatten_any_into_all_or_vice_versa() {
        let inner_any = AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap();
        let outer = AllExpr::new(vec![event(2, "c"), QueryExpr::any(inner_any)]).unwrap();
        let expr = QueryExpr::all(outer);
        let normalized = normalize_expr(&expr);
        match &normalized.kind {
            QueryExprKind::All(a) => {
                // Two branches: one Event, one Any.
                // Events sort before Any (discriminant 0 < 4).
                assert_eq!(a.branches.len(), 2);
                assert!(matches!(&a.branches[0].kind, QueryExprKind::Event(_)));
                assert!(matches!(&a.branches[1].kind, QueryExprKind::Any(_)));
            }
            _ => panic!("expected All"),
        }
    }

    #[test]
    fn deeply_nested_any_is_fully_flattened() {
        let level3 = AnyExpr::new(vec![event(0, "a")]).unwrap();
        let level2 = AnyExpr::new(vec![event(1, "b"), QueryExpr::any(level3)]).unwrap();
        let level1 = AnyExpr::new(vec![QueryExpr::any(level2), event(2, "c")]).unwrap();
        let expr = QueryExpr::any(level1);
        let normalized = normalize_expr(&expr);
        match &normalized.kind {
            QueryExprKind::Any(a) => {
                assert_eq!(a.branches.len(), 3);
                for b in &a.branches {
                    assert!(matches!(&b.kind, QueryExprKind::Event(_)));
                }
            }
            _ => panic!("expected Any"),
        }
    }

    // ── Deduplication tests ────────────────────────────────────────

    #[test]
    fn deduplicates_identical_branches_in_any() {
        let branches = vec![event(0, "a"), event(0, "a"), event(1, "b")];
        let expr = QueryExpr::any(AnyExpr::new(branches).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized.kind {
            QueryExprKind::Any(a) => {
                assert_eq!(a.branches.len(), 2);
            }
            _ => panic!("expected Any"),
        }
    }

    #[test]
    fn deduplicates_identical_branches_in_all() {
        let branches = vec![event(0, "a"), event(0, "a")];
        let expr = QueryExpr::all(AllExpr::new(branches).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized.kind {
            QueryExprKind::All(a) => {
                assert_eq!(a.branches.len(), 1);
            }
            _ => panic!("expected All"),
        }
    }

    // ── Canonical ordering tests ───────────────────────────────────

    #[test]
    fn branches_are_sorted_canonically() {
        let branches = vec![event(1, "z"), event(0, "a")];
        let expr = QueryExpr::any(AnyExpr::new(branches).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized.kind {
            QueryExprKind::Any(a) => {
                assert_eq!(a.branches.len(), 2);
            }
            _ => panic!("expected Any"),
        }
    }

    #[test]
    fn equivalent_builder_forms_normalize_equally() {
        let a_branches = vec![event(0, "fetch"), event(1, "navigate")];
        let b_branches = vec![event(1, "navigate"), event(0, "fetch")];
        let a_expr = QueryExpr::any(AnyExpr::new(a_branches).unwrap());
        let b_expr = QueryExpr::any(AnyExpr::new(b_branches).unwrap());
        let a_norm = normalize_expr(&a_expr);
        let b_norm = normalize_expr(&b_expr);
        assert_eq!(a_norm, b_norm);
    }

    // ── Idempotency tests ──────────────────────────────────────────

    #[test]
    fn normalization_is_idempotent() {
        let branches = vec![
            QueryExpr::any(AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap()),
            event(2, "c"),
        ];
        let expr = QueryExpr::any(AnyExpr::new(branches).unwrap());
        let once = normalize_expr(&expr);
        let twice = normalize_expr(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn normalization_of_simple_event_is_idempotent() {
        let expr = event(0, "fetch");
        let once = normalize_expr(&expr);
        let twice = normalize_expr(&once);
        assert_eq!(once, twice);
    }

    // ── Variable reassignment tests ───────────────────────────────

    #[test]
    fn reassigns_vars_to_dense_slots() {
        let branches = vec![event(5, "a"), event(3, "b")];
        let expr = QueryExpr::any(AnyExpr::new(branches).unwrap());
        let d = decl(expr, 0, "test");
        let (normalized, _) = normalize_query_decl(&d);
        match &normalized.expression.kind {
            QueryExprKind::Any(a) => {
                let vars: Vec<u32> = a
                    .branches
                    .iter()
                    .map(|b| match &b.kind {
                        QueryExprKind::Event(e) => e.var.get(),
                        _ => panic!("expected Event"),
                    })
                    .collect();
                assert_eq!(vars, vec![0, 1]);
            }
            _ => panic!("expected Any"),
        }
    }

    #[test]
    fn emission_primary_var_is_remapped() {
        let expr = event(42, "fetch");
        let d = decl(expr, 42, "fetch");
        let (normalized, _) = normalize_query_decl(&d);
        assert_eq!(normalized.emission.primary_var.get(), 0);
    }

    // ── Plan requirements tests ────────────────────────────────────

    #[test]
    fn simple_query_needs_occurrence_indexes() {
        let d = decl(event(0, "fetch"), 0, "fetch");
        let (_, req) = normalize_query_decl(&d);
        assert!(req.needs_occurrence_indexes);
        assert!(!req.needs_fact_stream);
        assert!(!req.needs_local_flow);
        assert!(!req.needs_project_overlay);
    }

    #[test]
    fn constrained_query_needs_fact_stream() {
        let mut eq = EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        eq.constraints
            .push(crate::api::rule::ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string(),
            ));
        let expr = QueryExpr::event(eq);
        let d = decl(expr, 0, "fetch");
        let (_, req) = normalize_query_decl(&d);
        assert!(req.needs_fact_stream);
        assert!(req.needs_value_resolution);
    }

    #[test]
    fn module_query_needs_project_overlay() {
        let eq = EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::ModuleExport {
                module: SmolStr::new("fs"),
                export: SmolStr::new("readFile"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let expr = QueryExpr::event(eq);
        let d = decl(expr, 0, "readFile");
        let (_, req) = normalize_query_decl(&d);
        assert!(req.needs_project_overlay);
    }

    #[test]
    fn global_query_does_not_need_project_overlay() {
        let d = decl(event(0, "fetch"), 0, "fetch");
        let (_, req) = normalize_query_decl(&d);
        assert!(!req.needs_project_overlay);
    }

    // ── Lifecycle preservation ─────────────────────────────────────

    #[test]
    fn lifecycle_expr_is_not_flattened_or_sorted() {
        let source = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("document.createElement"),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("document.createElement"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let lc = LifecycleQuery {
            sources: vec![source],
            condition: Some(crate::api::rule::FlowCondition::event(
                crate::api::rule::ObjectEventMatcher::property_write(
                    "type",
                    ValueMatcher::any_value(),
                ),
            )),
            completion: Some(crate::api::rule::FlowCompletion::configuration()),
        };
        let normalized = normalize_expr(&QueryExpr::lifecycle(lc));
        assert!(matches!(&normalized.kind, QueryExprKind::Lifecycle(_)));
    }

    // ── Edge cases ─────────────────────────────────────────────────

    #[test]
    fn single_branch_any_is_preserved() {
        let expr = QueryExpr::any(AnyExpr::new(vec![event(0, "a")]).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized.kind {
            QueryExprKind::Any(a) => {
                assert_eq!(a.branches.len(), 1);
            }
            _ => panic!("expected Any"),
        }
    }

    #[test]
    fn single_branch_all_is_preserved() {
        let expr = QueryExpr::all(AllExpr::new(vec![event(0, "a")]).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized.kind {
            QueryExprKind::All(a) => {
                assert_eq!(a.branches.len(), 1);
            }
            _ => panic!("expected All"),
        }
    }

    // ── QueryDecl normalization ────────────────────────────────────

    #[test]
    fn normalize_query_decl_preserves_symbol_and_kind() {
        let d = decl(event(0, "fetch"), 0, "fetch");
        let (normalized, _) = normalize_query_decl(&d);
        assert_eq!(normalized.emission.symbol, "fetch");
        assert_eq!(normalized.emission.kind, MatchKind::Call);
    }
}
