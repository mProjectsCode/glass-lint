use std::collections::BTreeMap;

use super::normalized::{NormalizedEmission, NormalizedQuery};
use crate::api::{
    compiler::{
        contradiction::detect_event_contradictions,
        normalize_all::normalize_all_root,
        normalized::{NormalizedEvent, NormalizedLifecycle, NormalizedRoot, NormalizedSubject},
        requirements::PlanRequirements,
        validate::QueryCompileError,
    },
    rule::{
        ArgumentConstraint,
        query::{
            AnyExpr, EmissionDecl, EventQuery, EventSpec, IdentitySpec, LifecycleQuery, QueryDecl,
            QueryExpr, QueryExprKind, VarId,
        },
    },
};

// ── Entry points ──────────────────────────────────────────────────────────

/// Normalize a [`QueryDecl`] into a canonical [`NormalizedQuery`].
///
/// Steps:
/// 1. Recursively normalize children.
/// 2. Flatten nested same-kind `Any`.
/// 3. Canonicalize semantic paths and predicate sets.
/// 4. Merge compatible same-event filters (All → one NormalizedEvent).
/// 5. Detect contradictions.
/// 6. Sort order-independent branches.
/// 7. Deduplicate equal branches.
/// 8. Alpha-normalize variables into deterministic dense slots.
/// 9. Compute exact plan requirements.
pub(crate) fn normalize_query_decl(decl: &QueryDecl) -> Result<NormalizedQuery, QueryCompileError> {
    let mut root = normalize_root(&decl.expression, decl.emission())?;

    // Step 3: Canonicalize semantic paths and predicate sets.
    canonicalize_normalized(&mut root);

    // Step 8: Alpha-normalize — renumber slots to dense 0..n order independent
    // of author-assigned VarId values.
    let slot_map = alpha_renumber_slots(&mut root);
    let primary_slot = slot_map
        .get(&decl.emission.primary_var().get())
        .copied()
        .ok_or_else(|| QueryCompileError::MissingBinding {
            primary_var: decl.emission.primary_var(),
        })?;

    // Step 9: Compute exact plan requirements.
    let req = PlanRequirements::for_root(&root);

    let nq = NormalizedQuery {
        root,
        emission: NormalizedEmission {
            primary_slot,
            kind: decl.emission.kind,
            symbol: decl.emission.symbol.clone(),
        },
        requirements: req,
    };

    // Post-normalization invariant validation.
    validate_normalized(&nq)?;

    Ok(nq)
}

/// Canonicalize semantic paths and predicate sets in a normalized root.
///
/// Ensures:
/// - `SymbolPath` segments are trimmed and non-empty.
/// - `IdentitySpec` rooted/global/heuristic names are trimmed.
/// - Argument constraints are sorted by index then matcher payload.
/// - Predicate alternatives are sorted and deduplicated.
///
/// The normalizer already produces canonical forms from construction-time
/// validation (Package 2) and the per-node sorting (Package 3).  This
/// function makes the step explicit and catches any edge cases.
fn canonicalize_normalized(root: &mut NormalizedRoot) {
    match root {
        NormalizedRoot::Event(ev) => {
            canonicalize_event(ev);
        }
        NormalizedRoot::Any(branches) => {
            for b in &mut **branches {
                canonicalize_normalized(b);
            }
        }
        NormalizedRoot::Lifecycle(lc) => {
            for src in &mut lc.sources {
                canonicalize_event(src);
            }
        }
    }
}

fn canonicalize_event(ev: &mut NormalizedEvent) {
    // Sort and deduplicate argument constraints by index then matcher.
    let mut args: Vec<crate::api::rule::ArgumentConstraint> = ev.arguments.to_vec();
    args.sort_by(|a, b| {
        a.index()
            .cmp(&b.index())
            .then_with(|| a.predicate().cmp(b.predicate()))
    });
    args.dedup();
    ev.arguments = args.into_boxed_slice();
}

/// Validate a fully normalized query.
///
/// Checks invariants that must hold after normalization:
/// - No `Any` contains a nested `Any` (should have been flattened).
/// - Variable slots are dense (0..n) without gaps.
/// - `Any` branches are non-empty.
fn validate_normalized(nq: &NormalizedQuery) -> Result<(), QueryCompileError> {
    validate_normalized_root(&nq.root, true)?;
    let slots = collect_normalized_slots(&nq.root);
    if slots
        .iter()
        .enumerate()
        .any(|(expected, actual)| usize::try_from(*actual) != Ok(expected))
    {
        return Err(QueryCompileError::InternalInvariant {
            detail: "normalized variable slots are not dense".into(),
        });
    }
    if !slots.contains(&nq.emission.primary_slot) {
        return Err(QueryCompileError::InternalInvariant {
            detail: "normalized emission slot is not bound".into(),
        });
    }
    if nq.emission.symbol.trim().is_empty() {
        return Err(QueryCompileError::InternalInvariant {
            detail: "normalized evidence symbol is empty".into(),
        });
    }
    let expected_requirements = PlanRequirements::for_root(&nq.root);
    if expected_requirements != nq.requirements {
        return Err(QueryCompileError::InternalInvariant {
            detail: "normalized plan requirements do not match normalized root".into(),
        });
    }
    Ok(())
}

fn validate_normalized_root(root: &NormalizedRoot, is_top: bool) -> Result<(), QueryCompileError> {
    match root {
        NormalizedRoot::Any(branches) => {
            if branches.is_empty() {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "normalized Any has zero branches".into(),
                });
            }
            for b in &**branches {
                if matches!(b, NormalizedRoot::Any(_)) {
                    return Err(QueryCompileError::InternalInvariant {
                        detail: "nested Any found after normalization".into(),
                    });
                }
                validate_normalized_root(b, false)?;
            }
            Ok(())
        }
        NormalizedRoot::Event(ev) => {
            if ev.arguments.windows(2).any(|pair| {
                pair[0].index() > pair[1].index()
                    || (pair[0].index() == pair[1].index()
                        && pair[0].predicate() >= pair[1].predicate())
            }) {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "normalized argument constraints are not canonical".into(),
                });
            }
            // Verify subject-specific invariants.
            match &ev.subject {
                NormalizedSubject::Returned {
                    producer,
                    object_slot: _,
                }
                | NormalizedSubject::Instance {
                    constructor: producer,
                    object_slot: _,
                } => {
                    // Returned/Instance must have a member event, not a bare call.
                    if !matches!(
                        ev.event,
                        EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. }
                    ) {
                        return Err(QueryCompileError::InternalInvariant {
                            detail: "returned/instance subject on non-member event".into(),
                        });
                    }
                    if producer.display_name().is_empty() {
                        return Err(QueryCompileError::InternalInvariant {
                            detail: "returned/instance subject relation is incomplete".into(),
                        });
                    }
                }
                NormalizedSubject::Direct => {}
            }
            Ok(())
        }
        NormalizedRoot::Lifecycle(lifecycle) => {
            if !is_top {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "lifecycle root nested inside Any".into(),
                });
            }
            if lifecycle.sources.is_empty() || lifecycle.completion.is_none() {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "normalized lifecycle is missing a required stage".into(),
                });
            }
            Ok(())
        }
    }
}

/// Collect every unique slot value present in the normalised tree.
pub(crate) fn collect_normalized_slots(root: &NormalizedRoot) -> Vec<u32> {
    let mut slots = Vec::new();
    collect_slots_rec(root, &mut slots);
    slots.sort_unstable();
    slots.dedup();
    slots
}

fn collect_slots_rec(root: &NormalizedRoot, slots: &mut Vec<u32>) {
    match root {
        NormalizedRoot::Event(ev) => slots.push(ev.slot),
        NormalizedRoot::Any(branches) => {
            for b in &**branches {
                collect_slots_rec(b, slots);
            }
        }
        NormalizedRoot::Lifecycle(lc) => {
            for src in &lc.sources {
                slots.push(src.slot);
            }
        }
    }
}

/// Remap every slot in the tree using the given old→new mapping.
#[allow(clippy::cast_possible_truncation)]
fn apply_slot_map(root: &mut NormalizedRoot, map: &BTreeMap<u32, u32>) {
    match root {
        NormalizedRoot::Event(ev) => {
            if let Some(&new_slot) = map.get(&ev.slot) {
                ev.slot = new_slot;
            }
        }
        NormalizedRoot::Any(branches) => {
            for b in &mut **branches {
                apply_slot_map(b, map);
            }
        }
        NormalizedRoot::Lifecycle(lc) => {
            for src in &mut lc.sources {
                if let Some(&new_slot) = map.get(&src.slot) {
                    src.slot = new_slot;
                }
            }
        }
    }
}

/// Alpha-renumber: replace author-assigned slot values with dense 0..n slots
/// ordered by the original slot values (deterministic).
#[allow(clippy::cast_possible_truncation)]
fn alpha_renumber_slots(root: &mut NormalizedRoot) -> BTreeMap<u32, u32> {
    let slots = collect_normalized_slots(root);
    if slots.is_empty() {
        return BTreeMap::new();
    }
    let mut map = BTreeMap::new();
    for (new_idx, &old) in slots.iter().enumerate() {
        map.insert(old, new_idx as u32);
    }
    apply_slot_map(root, &map);
    map
}

/// Normalize a [`QueryExpr`] into a [`NormalizedRoot`].
pub(crate) fn normalize_root(
    expr: &QueryExpr,
    emission: &EmissionDecl,
) -> Result<NormalizedRoot, QueryCompileError> {
    match &expr.kind {
        QueryExprKind::Event(eq) => {
            let ev = normalize_event_from_query(eq, emission)?;
            Ok(NormalizedRoot::Event(ev))
        }
        QueryExprKind::SelectEvent(_) | QueryExprKind::Require(_) => {
            // Orphaned atomic forms are invalid at the top level.
            Err(QueryCompileError::InternalInvariant {
                detail: "top-level expression is an atomic form without an enclosing operator"
                    .into(),
            })
        }
        QueryExprKind::Any(any) => normalize_any_root(any, emission),
        QueryExprKind::All(all) => normalize_all_root(all, emission),
        QueryExprKind::Lifecycle(lc) => normalize_lifecycle_root(lc, emission),
    }
}

// ── Any normalization ─────────────────────────────────────────────────────

fn normalize_any_root(
    any: &AnyExpr,
    emission: &EmissionDecl,
) -> Result<NormalizedRoot, QueryCompileError> {
    let mut branches: Vec<NormalizedRoot> = Vec::new();
    for b in &any.branches {
        match &b.kind {
            // Flatten nested Any
            QueryExprKind::Any(inner) => {
                let inner_root = normalize_any_root(inner, emission)?;
                if let NormalizedRoot::Any(inner_branches) = inner_root {
                    branches.extend(inner_branches.iter().cloned());
                }
            }
            _ => {
                branches.push(normalize_root(b, emission)?);
            }
        }
    }

    // Sort and dedup
    sort_roots(&mut branches);
    branches.dedup();

    // Reject contradictions across Any branches: every branch must have
    // a compatible event type for the primary evidence variable.
    if branches.len() > 1 {
        check_branch_evidence_compatibility(&branches, emission)?;
    }

    if branches.len() == 1 {
        Ok(branches.into_iter().next().unwrap())
    } else {
        Ok(NormalizedRoot::Any(branches.into_boxed_slice()))
    }
}

/// Check that all branches produce a compatible primary variable type.
fn check_branch_evidence_compatibility(
    branches: &[NormalizedRoot],
    emission: &EmissionDecl,
) -> Result<(), QueryCompileError> {
    let primary = emission.primary_var;
    let first_type = branch_var_type(&branches[0], primary);
    for branch in branches.iter().skip(1) {
        let other = branch_var_type(branch, primary);
        if first_type != other && first_type.is_some() && other.is_some() {
            return Err(QueryCompileError::IncompatibleBranchOutput {
                var: primary,
                type_a: "some",
                type_b: "other",
            });
        }
    }
    Ok(())
}

fn branch_var_type(root: &NormalizedRoot, _var: VarId) -> Option<&'static str> {
    match root {
        NormalizedRoot::Event(ev) => match ev.event {
            EventSpec::Call | EventSpec::Construct => Some("call_event"),
            EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. } => Some("member_event"),
            EventSpec::ClassReference | EventSpec::Import | EventSpec::StringReference => {
                Some("event")
            }
        },
        NormalizedRoot::Any(_) => None,
        NormalizedRoot::Lifecycle(_) => Some("lifecycle"),
    }
}

// ── Lifecycle normalization ────────────────────────────────────────────────

fn normalize_lifecycle_root(
    lc: &LifecycleQuery,
    emission: &EmissionDecl,
) -> Result<NormalizedRoot, QueryCompileError> {
    let mut sources: Vec<NormalizedEvent> = lc
        .sources
        .iter()
        .map(|src| normalize_event_from_query(src, emission))
        .collect::<Result<Vec<_>, _>>()?;

    // Deduplicate sources — two sources are equal when their event, identity,
    // and arguments match. Deterministic order is preserved (first wins).
    {
        use std::collections::BTreeSet;
        let mut seen: BTreeSet<(EventSpec, Option<IdentitySpec>, Box<[ArgumentConstraint]>)> =
            BTreeSet::new();
        sources.retain(|s| {
            let key = (s.event.clone(), s.identity.clone(), s.arguments.clone());
            seen.insert(key)
        });
        sources.sort_by(|a, b| {
            a.slot
                .cmp(&b.slot)
                .then_with(|| a.event.cmp(&b.event))
                .then_with(|| a.identity.cmp(&b.identity))
                .then_with(|| a.arguments.cmp(&b.arguments))
        });
    }

    Ok(NormalizedRoot::Lifecycle(NormalizedLifecycle {
        sources,
        condition: lc.condition.clone(),
        completion: lc.completion.clone(),
    }))
}

// ── NormalizedEvent construction from EventQuery ───────────────────────────

fn normalize_event_from_query(
    eq: &EventQuery,
    _emission: &EmissionDecl,
) -> Result<NormalizedEvent, QueryCompileError> {
    let mut args: Vec<ArgumentConstraint> = eq.constraints.clone();
    args.sort_by(|a, b| {
        a.index()
            .cmp(&b.index())
            .then_with(|| a.predicate().cmp(b.predicate()))
    });
    args.dedup();

    let subject = NormalizedSubject::Direct;
    detect_event_contradictions(eq.var, &eq.event, &eq.identity, &subject, &args)?;

    Ok(NormalizedEvent {
        slot: eq.var.get(),
        event: eq.event.clone(),
        identity: Some(eq.identity.clone()),
        subject,
        arguments: args.into_boxed_slice(),
    })
}

// ── Sorting and deduplication helpers ──────────────────────────────────────

fn sort_roots(roots: &mut [NormalizedRoot]) {
    roots.sort_by(compare_roots);
}

fn compare_roots(a: &NormalizedRoot, b: &NormalizedRoot) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let disc_a = root_discriminant(a);
    let disc_b = root_discriminant(b);
    if disc_a != disc_b {
        return disc_a.cmp(&disc_b);
    }
    match (a, b) {
        (NormalizedRoot::Event(ae), NormalizedRoot::Event(be)) => ae
            .slot
            .cmp(&be.slot)
            .then_with(|| ae.event.cmp(&be.event))
            .then_with(|| ae.identity.cmp(&be.identity))
            .then_with(|| ae.arguments.cmp(&be.arguments)),
        (NormalizedRoot::Any(aa), NormalizedRoot::Any(ba)) => {
            aa.len().cmp(&ba.len()).then_with(|| {
                aa.iter()
                    .zip(ba.iter())
                    .fold(Ordering::Equal, |acc, (x, y)| {
                        acc.then_with(|| compare_roots(x, y))
                    })
            })
        }
        (NormalizedRoot::Lifecycle(la), NormalizedRoot::Lifecycle(lb)) => la
            .sources
            .len()
            .cmp(&lb.sources.len())
            .then_with(|| {
                la.sources
                    .iter()
                    .zip(lb.sources.iter())
                    .fold(Ordering::Equal, |acc, (a, b)| {
                        acc.then_with(|| {
                            a.slot
                                .cmp(&b.slot)
                                .then_with(|| a.event.cmp(&b.event))
                                .then_with(|| a.identity.cmp(&b.identity))
                                .then_with(|| a.arguments.cmp(&b.arguments))
                        })
                    })
                    .then_with(|| la.sources.len().cmp(&lb.sources.len()))
            })
            .then_with(|| {
                la.condition
                    .as_ref()
                    .map(crate::api::rule::LifecycleCondition::kind)
                    .cmp(
                        &lb.condition
                            .as_ref()
                            .map(crate::api::rule::LifecycleCondition::kind),
                    )
            })
            .then_with(|| {
                la.completion
                    .as_ref()
                    .map(crate::api::rule::LifecycleCompletion::kind)
                    .cmp(
                        &lb.completion
                            .as_ref()
                            .map(crate::api::rule::LifecycleCompletion::kind),
                    )
            }),
        _ => Ordering::Equal,
    }
}

fn root_discriminant(root: &NormalizedRoot) -> u8 {
    match root {
        NormalizedRoot::Event(_) => 0,
        NormalizedRoot::Any(_) => 1,
        NormalizedRoot::Lifecycle(_) => 2,
    }
}
