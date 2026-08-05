use std::collections::BTreeMap;

use super::normalized::{NormalizedEmission, NormalizedQuery};
use crate::api::{
    compiler::{
        contradiction::detect_event_contradictions,
        normalize_all::normalize_all_root,
        normalized::{
            CanonicalArgumentConstraints, NormalizedEvent, NormalizedLifecycle,
            NormalizedLifecycleCompletion, NormalizedLifecycleCondition, NormalizedLifecycleEvent,
            NormalizedLifecycleSink, NormalizedRoot, NormalizedSubject,
        },
        requirements::PlanRequirements,
        validate::QueryCompileError,
    },
    rule::query::{
        AnyExpr, EmissionDecl, EventQuery, EventSpec, IdentitySpec, LifecycleQuery, QueryDecl,
        QueryExpr, QueryExprKind, VarType,
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
    let mut root = normalize_root(decl.expression(), decl.emission())?;

    // Step 8: Alpha-normalize — renumber object slots to dense 0..n order
    // independent of author-assigned VarId values.
    alpha_renumber_slots(&mut root);

    // Step 9: Compute exact plan requirements.
    let req = PlanRequirements::for_root(&root);

    let nq = NormalizedQuery {
        root,
        emission: NormalizedEmission {
            kind: decl.emission().kind(),
            symbol: decl.emission().symbol().to_owned(),
        },
        requirements: req,
    };

    // Post-normalization invariant validation.
    validate_normalized(&nq)?;

    Ok(nq)
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
            // CanonicalArgumentConstraints groups are already in order and
            // each group's predicates are sorted and deduplicated. Verify
            // that groups are in ascending index order.
            if ev
                .arguments
                .groups()
                .windows(2)
                .any(|pair| pair[0].index() > pair[1].index())
            {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "normalized argument constraint groups are not canonical".into(),
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
                        EventSpec::MemberCall { .. }
                            | EventSpec::MemberRead { .. }
                            | EventSpec::PropertyWrite { .. }
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
                NormalizedSubject::Direct { .. } => {}
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
    match expr.kind() {
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
    for b in any.iter() {
        match b.kind() {
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
    let first_type = branch_var_type(&branches[0]);
    for branch in branches.iter().skip(1) {
        let other = branch_var_type(branch);
        if let (Some(a), Some(b)) = (first_type, other)
            && a != b
        {
            return Err(QueryCompileError::IncompatibleBranchOutput {
                var: primary,
                type_a: a.variant_name(),
                type_b: b.variant_name(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchVarType {
    Event(VarType),
    Lifecycle,
}

impl BranchVarType {
    fn variant_name(self) -> &'static str {
        match self {
            Self::Event(ty) => ty.variant_name(),
            Self::Lifecycle => "lifecycle",
        }
    }
}

fn branch_var_type(root: &NormalizedRoot) -> Option<BranchVarType> {
    match root {
        NormalizedRoot::Event(ev) => Some(BranchVarType::Event(ev.event.variable_type())),
        NormalizedRoot::Any(_) => None,
        NormalizedRoot::Lifecycle(_) => Some(BranchVarType::Lifecycle),
    }
}

// ── Lifecycle normalization ────────────────────────────────────────────────

fn normalize_lifecycle_root(
    lc: &LifecycleQuery,
    emission: &EmissionDecl,
) -> Result<NormalizedRoot, QueryCompileError> {
    let mut sources: Vec<NormalizedEvent> = lc
        .sources()
        .iter()
        .map(|src| normalize_event_from_query(src, emission))
        .collect::<Result<Vec<_>, _>>()?;

    // Deduplicate sources — two sources are equal when their event, identity,
    // and arguments match. Deterministic order is preserved (first wins).
    {
        use std::collections::BTreeSet;
        let mut seen: BTreeSet<(EventSpec, IdentitySpec, CanonicalArgumentConstraints)> =
            BTreeSet::new();
        sources.retain(|s| {
            let key = (s.event.clone(), s.identity().clone(), s.arguments.clone());
            seen.insert(key)
        });
        sources.sort_by(|a, b| {
            a.slot
                .cmp(&b.slot)
                .then_with(|| a.event.cmp(&b.event))
                .then_with(|| a.identity().cmp(b.identity()))
                .then_with(|| a.arguments.cmp(&b.arguments))
        });
    }

    let condition = lc
        .condition()
        .as_ref()
        .map(|condition| match condition.kind() {
            crate::api::rule::query::lifecycle::LifecycleConditionKind::AnyOf(events) => {
                NormalizedLifecycleCondition::AnyOf(
                    events
                        .iter()
                        .map(normalize_lifecycle_event)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                )
            }
            crate::api::rule::query::lifecycle::LifecycleConditionKind::AllOf(events) => {
                NormalizedLifecycleCondition::AllOf(
                    events
                        .iter()
                        .map(normalize_lifecycle_event)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                )
            }
        });
    let completion = lc
        .completion()
        .as_ref()
        .map(|completion| match completion.kind() {
            crate::api::rule::query::lifecycle::LifecycleCompletionKind::Configuration => {
                NormalizedLifecycleCompletion::Configuration
            }
            crate::api::rule::query::lifecycle::LifecycleCompletionKind::AnySink(sinks) => {
                NormalizedLifecycleCompletion::AnySink(
                    sinks
                        .iter()
                        .map(normalize_lifecycle_sink)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                )
            }
            crate::api::rule::query::lifecycle::LifecycleCompletionKind::AllSinks(sinks) => {
                NormalizedLifecycleCompletion::AllSinks(
                    sinks
                        .iter()
                        .map(normalize_lifecycle_sink)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                )
            }
        });

    Ok(NormalizedRoot::Lifecycle(NormalizedLifecycle {
        sources,
        condition,
        completion,
    }))
}

fn normalize_lifecycle_event(
    event: &crate::api::rule::query::lifecycle::LifecycleEvent,
) -> NormalizedLifecycleEvent {
    match event.kind() {
        crate::api::rule::query::lifecycle::LifecycleEventKind::PropertyWrite {
            property,
            value,
        } => NormalizedLifecycleEvent::PropertyWrite {
            property: property.clone(),
            value: value.clone(),
        },
        crate::api::rule::query::lifecycle::LifecycleEventKind::MemberCall {
            member,
            arguments,
        } => NormalizedLifecycleEvent::MemberCall {
            member: member.as_str().into(),
            arguments: CanonicalArgumentConstraints::from_constraints(arguments),
        },
    }
}

fn normalize_lifecycle_sink(
    sink: &crate::api::rule::query::lifecycle::LifecycleSink,
) -> NormalizedLifecycleSink {
    match sink.kind() {
        crate::api::rule::query::lifecycle::LifecycleSinkKind::ArgumentOf { endpoint, index } => {
            NormalizedLifecycleSink::ArgumentOf {
                target: endpoint.target().clone(),
                index: *index,
            }
        }
        crate::api::rule::query::lifecycle::LifecycleSinkKind::AnyArgumentOf { endpoint } => {
            NormalizedLifecycleSink::AnyArgumentOf {
                target: endpoint.target().clone(),
            }
        }
    }
}

// ── NormalizedEvent construction from EventQuery ───────────────────────────

fn normalize_event_from_query(
    eq: &EventQuery,
    _emission: &EmissionDecl,
) -> Result<NormalizedEvent, QueryCompileError> {
    let arguments = CanonicalArgumentConstraints::from_constraints(eq.constraints());
    let args = arguments.to_flat_vec();

    let subject = NormalizedSubject::Direct {
        identity: eq.identity().clone(),
    };
    detect_event_contradictions(eq.var(), eq.event(), eq.identity(), &subject, &args)?;

    Ok(NormalizedEvent {
        slot: eq.var().get(),
        event: eq.event().clone(),
        subject,
        arguments,
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
            .then_with(|| ae.identity().cmp(be.identity()))
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
                                .then_with(|| a.identity().cmp(b.identity()))
                                .then_with(|| a.arguments.cmp(&b.arguments))
                        })
                    })
                    .then_with(|| la.sources.len().cmp(&lb.sources.len()))
            })
            .then_with(|| la.condition.cmp(&lb.condition))
            .then_with(|| la.completion.cmp(&lb.completion)),
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
