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
    QuerySet, VarId,
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
            needs_local_flow: matches!(&decl.expression, QueryExpr::Lifecycle(_)),
            needs_cross_call_flow: false,
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
    match expr {
        QueryExpr::Event(eq) => !eq.constraints.is_empty(),
        QueryExpr::Any(any) => any.branches.iter().any(has_any_constraint),
        QueryExpr::All(all) => all.branches.iter().any(has_any_constraint),
        QueryExpr::Lifecycle(_) => false,
    }
}

fn requires_project_overlay(expr: &QueryExpr) -> bool {
    match expr {
        QueryExpr::Event(eq) => matches!(
            &eq.identity,
            IdentitySpec::ModuleExport { .. }
                | IdentitySpec::PackageModuleExport { .. }
                | IdentitySpec::ModuleNamespace { .. }
                | IdentitySpec::PackageModuleNamespace { .. }
        ),
        QueryExpr::Any(any) => any.branches.iter().any(requires_project_overlay),
        QueryExpr::All(all) => all.branches.iter().any(requires_project_overlay),
        QueryExpr::Lifecycle(lc) => requires_project_overlay(&QueryExpr::Event(lc.source.clone())),
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
    match expr {
        QueryExpr::Any(any) => {
            let mut branches = flatten_branches::<true>(&any.branches);
            sort_exprs(&mut branches);
            branches.dedup();
            // Must be non-empty (validated at construction)
            debug_assert!(!branches.is_empty(), "normalized Any must be non-empty");
            QueryExpr::Any(AnyExpr { branches })
        }
        QueryExpr::All(all) => {
            let mut branches = flatten_branches::<false>(&all.branches);
            sort_exprs(&mut branches);
            branches.dedup();
            debug_assert!(!branches.is_empty(), "normalized All must be non-empty");
            QueryExpr::All(AllExpr { branches })
        }
        QueryExpr::Event(_) | QueryExpr::Lifecycle(_) => expr.clone(),
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
            if let QueryExpr::Any(inner) = &flat {
                result.extend(inner.branches.clone());
            } else {
                result.push(flat);
            }
        } else if let QueryExpr::All(inner) = &flat {
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

    let disc_a = expr_discriminant(a);
    let disc_b = expr_discriminant(b);
    if disc_a != disc_b {
        return disc_a.cmp(&disc_b);
    }

    match (a, b) {
        (QueryExpr::Event(ae), QueryExpr::Event(be)) => compare_event_fields(ae, be),
        (QueryExpr::Lifecycle(la), QueryExpr::Lifecycle(lb)) => {
            compare_event_fields(&la.source, &lb.source)
                .then_with(|| la.condition.is_some().cmp(&lb.condition.is_some()))
                .then_with(|| la.completion.is_some().cmp(&lb.completion.is_some()))
        }
        (QueryExpr::Any(aa), QueryExpr::Any(ba)) => {
            compare_branch_slices(&aa.branches, &ba.branches)
        }
        (QueryExpr::All(aa), QueryExpr::All(ba)) => {
            compare_branch_slices(&aa.branches, &ba.branches)
        }
        _ => Ordering::Equal,
    }
}

fn expr_discriminant(e: &QueryExpr) -> u8 {
    match e {
        QueryExpr::Event(_) => 0,
        QueryExpr::Lifecycle(_) => 1,
        QueryExpr::Any(_) => 2,
        QueryExpr::All(_) => 3,
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
    match expr {
        QueryExpr::Event(eq) => vars.push(eq.var),
        QueryExpr::Any(any) => {
            for b in &any.branches {
                collect_vars_preorder(b, vars);
            }
        }
        QueryExpr::All(all) => {
            for b in &all.branches {
                collect_vars_preorder(b, vars);
            }
        }
        QueryExpr::Lifecycle(lc) => vars.push(lc.source.var),
    }
}

fn remap_vars(expr: &QueryExpr, var_map: &BTreeMap<VarId, VarId>) -> QueryExpr {
    match expr {
        QueryExpr::Event(eq) => QueryExpr::Event(EventQuery {
            var: var_map.get(&eq.var).copied().unwrap_or(eq.var),
            event: eq.event.clone(),
            identity: eq.identity.clone(),
            subject: eq.subject.clone(),
            constraints: eq.constraints.clone(),
        }),
        QueryExpr::Any(any) => {
            let branches: Vec<QueryExpr> = any
                .branches
                .iter()
                .map(|b| remap_vars(b, var_map))
                .collect();
            QueryExpr::Any(AnyExpr { branches })
        }
        QueryExpr::All(all) => {
            let branches: Vec<QueryExpr> = all
                .branches
                .iter()
                .map(|b| remap_vars(b, var_map))
                .collect();
            QueryExpr::All(AllExpr { branches })
        }
        QueryExpr::Lifecycle(lc) => {
            let source = EventQuery {
                var: var_map
                    .get(&lc.source.var)
                    .copied()
                    .unwrap_or(lc.source.var),
                ..lc.source.clone()
            };
            QueryExpr::Lifecycle(LifecycleQuery {
                source,
                condition: lc.condition.clone(),
                completion: lc.completion.clone(),
            })
        }
    }
}

/// Normalize all queries in a [`QuerySet`].
///
/// Each query is normalized independently. Returns the normalized set and
/// a vector of plan requirements (one per query).
#[allow(dead_code)]
pub(crate) fn normalize_query_set(set: &QuerySet) -> (QuerySet, Vec<PlanRequirements>) {
    let mut normalized = Vec::with_capacity(set.queries.len());
    let mut requirements = Vec::with_capacity(set.queries.len());
    for query in &set.queries {
        let (norm, req) = normalize_query_decl(query);
        normalized.push(norm);
        requirements.push(req);
    }
    (QuerySet::new(normalized), requirements)
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
            query::{EmissionDecl, EventQuery, EventSpec, IdentitySpec, QuerySet, SubjectSpec},
        },
    };

    // ── Helpers ────────────────────────────────────────────────────

    fn event(var: u32, name: &str) -> QueryExpr {
        QueryExpr::Event(EventQuery {
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
            AnyExpr::new(vec![event(2, "c"), QueryExpr::Any(inner), event(3, "d")]).unwrap();
        let expr = QueryExpr::Any(outer);
        let normalized = normalize_expr(&expr);
        match &normalized {
            QueryExpr::Any(a) => {
                assert_eq!(a.branches.len(), 4);
                for b in &a.branches {
                    assert!(matches!(b, QueryExpr::Event(_)));
                }
            }
            _ => panic!("expected Any"),
        }
    }

    #[test]
    fn flattens_nested_all() {
        let inner = AllExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap();
        let outer =
            AllExpr::new(vec![event(2, "c"), QueryExpr::All(inner), event(3, "d")]).unwrap();
        let expr = QueryExpr::All(outer);
        let normalized = normalize_expr(&expr);
        match &normalized {
            QueryExpr::All(a) => {
                assert_eq!(a.branches.len(), 4);
                for b in &a.branches {
                    assert!(matches!(b, QueryExpr::Event(_)));
                }
            }
            _ => panic!("expected All"),
        }
    }

    #[test]
    fn does_not_flatten_any_into_all_or_vice_versa() {
        let inner_any = AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap();
        let outer = AllExpr::new(vec![event(2, "c"), QueryExpr::Any(inner_any)]).unwrap();
        let expr = QueryExpr::All(outer);
        let normalized = normalize_expr(&expr);
        match &normalized {
            QueryExpr::All(a) => {
                // Two branches: one Event, one Any.
                // Events sort before Any (discriminant 0 < 2).
                assert_eq!(a.branches.len(), 2);
                assert!(matches!(&a.branches[0], QueryExpr::Event(_)));
                assert!(matches!(&a.branches[1], QueryExpr::Any(_)));
            }
            _ => panic!("expected All"),
        }
    }

    #[test]
    fn deeply_nested_any_is_fully_flattened() {
        let level3 = AnyExpr::new(vec![event(0, "a")]).unwrap();
        let level2 = AnyExpr::new(vec![event(1, "b"), QueryExpr::Any(level3)]).unwrap();
        let level1 = AnyExpr::new(vec![QueryExpr::Any(level2), event(2, "c")]).unwrap();
        let expr = QueryExpr::Any(level1);
        let normalized = normalize_expr(&expr);
        match &normalized {
            QueryExpr::Any(a) => {
                assert_eq!(a.branches.len(), 3);
                for b in &a.branches {
                    assert!(matches!(b, QueryExpr::Event(_)));
                }
            }
            _ => panic!("expected Any"),
        }
    }

    // ── Deduplication tests ────────────────────────────────────────

    #[test]
    fn deduplicates_identical_branches_in_any() {
        let branches = vec![event(0, "a"), event(0, "a"), event(1, "b")];
        let expr = QueryExpr::Any(AnyExpr::new(branches).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized {
            QueryExpr::Any(a) => {
                assert_eq!(a.branches.len(), 2);
            }
            _ => panic!("expected Any"),
        }
    }

    #[test]
    fn deduplicates_identical_branches_in_all() {
        let branches = vec![event(0, "a"), event(0, "a")];
        let expr = QueryExpr::All(AllExpr::new(branches).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized {
            QueryExpr::All(a) => {
                assert_eq!(a.branches.len(), 1);
            }
            _ => panic!("expected All"),
        }
    }

    // ── Canonical ordering tests ───────────────────────────────────

    #[test]
    fn branches_are_sorted_canonically() {
        let branches = vec![event(1, "z"), event(0, "a")];
        let expr = QueryExpr::Any(AnyExpr::new(branches).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized {
            QueryExpr::Any(a) => {
                // After normalization, var slots are reassigned and
                // branches are sorted.  Event with var=0 (smallest)
                // comes first.
                assert_eq!(a.branches.len(), 2);
            }
            _ => panic!("expected Any"),
        }
    }

    #[test]
    fn equivalent_builder_forms_normalize_equally() {
        let a_branches = vec![event(0, "fetch"), event(1, "navigate")];
        let b_branches = vec![event(1, "navigate"), event(0, "fetch")];
        let a_expr = QueryExpr::Any(AnyExpr::new(a_branches).unwrap());
        let b_expr = QueryExpr::Any(AnyExpr::new(b_branches).unwrap());
        let a_norm = normalize_expr(&a_expr);
        let b_norm = normalize_expr(&b_expr);
        assert_eq!(a_norm, b_norm);
    }

    // ── Idempotency tests ──────────────────────────────────────────

    #[test]
    fn normalization_is_idempotent() {
        let branches = vec![
            QueryExpr::Any(AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap()),
            event(2, "c"),
        ];
        let expr = QueryExpr::Any(AnyExpr::new(branches).unwrap());
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
        // normalize_query_decl does both flattening and var reassignment.
        let branches = vec![event(5, "a"), event(3, "b")];
        let expr = QueryExpr::Any(AnyExpr::new(branches).unwrap());
        let d = decl(expr, 0, "test");
        let (normalized, _) = normalize_query_decl(&d);
        match &normalized.expression {
            QueryExpr::Any(a) => {
                let vars: Vec<u32> = a
                    .branches
                    .iter()
                    .map(|b| match b {
                        QueryExpr::Event(e) => e.var.get(),
                        _ => panic!("expected Event"),
                    })
                    .collect();
                // Vars should be 0 and 1 (dense, sorted by original var order)
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
                0,
                ValueMatcher::static_string(),
            ));
        let expr = QueryExpr::Event(eq);
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
        let expr = QueryExpr::Event(eq);
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
        let lc = QueryExpr::Lifecycle(LifecycleQuery {
            source,
            condition: Some(crate::api::rule::FlowCondition::event(
                crate::api::rule::ObjectEventMatcher::property_write(
                    "type",
                    ValueMatcher::any_value(),
                ),
            )),
            completion: Some(crate::api::rule::FlowCompletion::configuration()),
        });
        let normalized = normalize_expr(&lc);
        assert!(matches!(normalized, QueryExpr::Lifecycle(_)));
    }

    // ── QuerySet normalization ─────────────────────────────────────

    #[test]
    fn normalize_query_set_preserves_decl_count() {
        let queries = vec![
            decl(event(0, "fetch"), 0, "fetch"),
            decl(event(1, "navigate"), 1, "navigate"),
        ];
        let set = QuerySet::new(queries);
        let (normalized_set, _) = normalize_query_set(&set);
        assert_eq!(normalized_set.queries.len(), 2);
    }

    #[test]
    fn normalize_query_set_makes_vars_dense() {
        let queries = vec![
            decl(event(5, "fetch"), 5, "fetch"),
            decl(event(3, "navigate"), 3, "navigate"),
        ];
        let set = QuerySet::new(queries);
        let (normalized_set, _) = normalize_query_set(&set);
        // Each query gets independent 0-based vars
        assert_eq!(normalized_set.queries[0].emission.primary_var.get(), 0);
        assert_eq!(normalized_set.queries[1].emission.primary_var.get(), 0);
    }

    // ── Edge cases ─────────────────────────────────────────────────

    #[test]
    fn single_branch_any_is_preserved() {
        let expr = QueryExpr::Any(AnyExpr::new(vec![event(0, "a")]).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized {
            QueryExpr::Any(a) => {
                assert_eq!(a.branches.len(), 1);
            }
            _ => panic!("expected Any"),
        }
    }

    #[test]
    fn single_branch_all_is_preserved() {
        let expr = QueryExpr::All(AllExpr::new(vec![event(0, "a")]).unwrap());
        let normalized = normalize_expr(&expr);
        match &normalized {
            QueryExpr::All(a) => {
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
