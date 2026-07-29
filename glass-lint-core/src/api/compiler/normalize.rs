use std::collections::{BTreeMap, BTreeSet};

use crate::api::{
    classification::MatchKind,
    compiler::validate::{ContradictionKind, QueryCompileError},
    rule::{
        ArgumentConstraint,
        query::{
            AllExpr, AnyExpr, EmissionDecl, EventQuery, EventSpec, IdentitySpec, LifecycleQuery,
            QueryDecl, QueryExpr, QueryExprKind, QueryPredicate, VarId,
        },
    },
};

// ── Normalized IR ──────────────────────────────────────────────────────────

/// A canonical normalized logical query with no `All` variant.
///
/// Normalization merges same-event conjunctions into one event node,
/// detects contradictions, and assigns dense deterministic variable slots.
/// Fields are private to `api/compiler`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedQuery {
    root: NormalizedRoot,
    emission: NormalizedEmission,
    requirements: PlanRequirements,
}

impl NormalizedQuery {
    pub(crate) fn root(&self) -> &NormalizedRoot {
        &self.root
    }

    pub(crate) fn emission(&self) -> &NormalizedEmission {
        &self.emission
    }

    pub(crate) fn requirements(&self) -> &PlanRequirements {
        &self.requirements
    }
}

/// Evidence emission for a normalized query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedEmission {
    primary_slot: u32,
    kind: MatchKind,
    symbol: String,
}

impl NormalizedEmission {
    pub(crate) fn kind(&self) -> MatchKind {
        self.kind
    }

    pub(crate) fn symbol(&self) -> &str {
        &self.symbol
    }
}

/// Normalized root expression — no `All` variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NormalizedRoot {
    Event(NormalizedEvent),
    Any(Box<[Self]>),
    Lifecycle(NormalizedLifecycle),
}

/// A single normalized event node with merged subject and arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedEvent {
    slot: u32,
    event: EventSpec,
    identity: Option<IdentitySpec>,
    subject: NormalizedSubject,
    arguments: Box<[ArgumentConstraint]>,
}

impl NormalizedEvent {
    pub(crate) fn slot(&self) -> u32 {
        self.slot
    }

    pub(crate) fn event(&self) -> &EventSpec {
        &self.event
    }

    pub(crate) fn identity(&self) -> Option<&IdentitySpec> {
        self.identity.as_ref()
    }

    pub(crate) fn subject(&self) -> &NormalizedSubject {
        &self.subject
    }

    pub(crate) fn arguments(&self) -> &[ArgumentConstraint] {
        &self.arguments
    }
}

/// Subject relationship in a normalized event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NormalizedSubject {
    Direct,
    Returned {
        producer: IdentitySpec,
        object_slot: u32,
    },
    Instance {
        constructor: IdentitySpec,
        object_slot: u32,
    },
}

/// Normalized lifecycle — preserves sources, condition, and completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedLifecycle {
    sources: Vec<NormalizedEvent>,
    condition: Option<crate::api::rule::LifecycleCondition>,
    completion: Option<crate::api::rule::LifecycleCompletion>,
}

impl NormalizedLifecycle {
    pub(crate) fn sources(&self) -> &[NormalizedEvent] {
        &self.sources
    }

    pub(crate) fn condition(&self) -> Option<&crate::api::rule::LifecycleCondition> {
        self.condition.as_ref()
    }

    pub(crate) fn completion(&self) -> Option<&crate::api::rule::LifecycleCompletion> {
        self.completion.as_ref()
    }
}

// ── Plan requirements ─────────────────────────────────────────────────────

/// Which value-resolution capabilities the physical plan needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ValueResolutionRequirement {
    LocalStaticValues,
    ModuleIdentityValues,
    CallResultIdentities,
}

/// Whether local, cross-call, or cross-file flow projection is required.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct FlowRequirements {
    pub(crate) local: bool,
    pub(crate) cross_call: bool,
    pub(crate) cross_file: bool,
}

/// Which project-level preparation the physical plan needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ProjectRequirement {
    ExactModuleExports,
    PackageModuleExports,
    ExactModuleNamespaces,
    PackageModuleNamespaces,
    CallResultIdentities,
}

/// Requirements computed during normalization for physical planning.
///
/// Each field contains the exact set of capabilities needed by the
/// normalized query.  Runtime preparation must consult these sets rather
/// than performing work unconditionally.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PlanRequirements {
    pub(crate) value_resolution: BTreeSet<ValueResolutionRequirement>,
    pub(crate) flow: FlowRequirements,
    pub(crate) project: BTreeSet<ProjectRequirement>,
}

#[allow(dead_code)]
impl PlanRequirements {
    pub(crate) fn value_resolution(&self) -> &BTreeSet<ValueResolutionRequirement> {
        &self.value_resolution
    }

    pub(crate) fn flow(&self) -> &FlowRequirements {
        &self.flow
    }

    #[allow(dead_code)]
    pub(crate) fn project_requirements(&self) -> &BTreeSet<ProjectRequirement> {
        &self.project
    }

    /// Whether any project-level identity work (module identities, overlays)
    /// is needed.
    pub(crate) fn needs_module_identities(&self) -> bool {
        self.project.iter().any(|requirement| {
            matches!(
                requirement,
                ProjectRequirement::ExactModuleExports
                    | ProjectRequirement::PackageModuleExports
                    | ProjectRequirement::ExactModuleNamespaces
                    | ProjectRequirement::PackageModuleNamespaces
            )
        })
    }

    /// Whether call-result identity resolution is needed.
    pub(crate) fn needs_call_result_identities(&self) -> bool {
        self.project
            .contains(&ProjectRequirement::CallResultIdentities)
            || self
                .value_resolution
                .contains(&ValueResolutionRequirement::CallResultIdentities)
    }

    /// Whether a project identity overlay is needed for any matched plan.
    pub(crate) fn needs_project_overlay(&self) -> bool {
        self.project.iter().any(|requirement| {
            matches!(
                requirement,
                ProjectRequirement::ExactModuleExports
                    | ProjectRequirement::PackageModuleExports
                    | ProjectRequirement::ExactModuleNamespaces
                    | ProjectRequirement::PackageModuleNamespaces
                    | ProjectRequirement::CallResultIdentities
            )
        })
    }

    pub(crate) fn merge_from(&mut self, other: &Self) {
        self.value_resolution
            .extend(other.value_resolution.iter().cloned());
        self.flow.local |= other.flow.local;
        self.flow.cross_call |= other.flow.cross_call;
        self.flow.cross_file |= other.flow.cross_file;
        self.project.extend(other.project.iter().cloned());
    }

    fn for_event(event: &NormalizedEvent) -> Self {
        Self {
            value_resolution: Self::value_resolution_for_event(event),
            flow: FlowRequirements::default(),
            project: Self::project_for_event(event),
        }
    }

    fn for_lifecycle(_lc: &NormalizedLifecycle) -> Self {
        Self {
            value_resolution: BTreeSet::new(),
            flow: FlowRequirements {
                local: true,
                cross_call: true,
                cross_file: false,
            },
            project: BTreeSet::new(),
        }
    }

    fn for_root(root: &NormalizedRoot) -> Self {
        match root {
            NormalizedRoot::Event(ev) => Self::for_event(ev),
            NormalizedRoot::Any(branches) => {
                let mut req = Self::default();
                for b in branches {
                    req.merge_from(&Self::for_root(b));
                }
                req
            }
            NormalizedRoot::Lifecycle(lc) => Self::for_lifecycle(lc),
        }
    }

    fn value_resolution_for_event(event: &NormalizedEvent) -> BTreeSet<ValueResolutionRequirement> {
        let mut set = BTreeSet::new();
        if !event.arguments.is_empty() {
            set.insert(ValueResolutionRequirement::LocalStaticValues);
        }
        if let Some(identity) = event_identity(event) {
            match identity {
                IdentitySpec::ModuleExport { .. } | IdentitySpec::PackageModuleExport { .. } => {
                    set.insert(ValueResolutionRequirement::ModuleIdentityValues);
                    set.insert(ValueResolutionRequirement::CallResultIdentities);
                }
                IdentitySpec::ModuleNamespace { .. }
                | IdentitySpec::PackageModuleNamespace { .. } => {
                    set.insert(ValueResolutionRequirement::ModuleIdentityValues);
                }
                _ => {}
            }
        }
        set
    }

    fn project_for_event(event: &NormalizedEvent) -> BTreeSet<ProjectRequirement> {
        let mut set = BTreeSet::new();
        if let Some(identity) = event_identity(event)
            && requires_project_overlay_spec(identity)
        {
            match identity {
                IdentitySpec::ModuleExport { .. } => {
                    set.insert(ProjectRequirement::ExactModuleExports);
                    set.insert(ProjectRequirement::CallResultIdentities);
                }
                IdentitySpec::PackageModuleExport { .. } => {
                    set.insert(ProjectRequirement::PackageModuleExports);
                    set.insert(ProjectRequirement::CallResultIdentities);
                }
                IdentitySpec::ModuleNamespace { .. } => {
                    set.insert(ProjectRequirement::ExactModuleNamespaces);
                }
                IdentitySpec::PackageModuleNamespace { .. } => {
                    set.insert(ProjectRequirement::PackageModuleNamespaces);
                }
                _ => {}
            }
        }
        set
    }
}

fn event_identity(event: &NormalizedEvent) -> Option<&IdentitySpec> {
    event.identity.as_ref().or(match &event.subject {
        NormalizedSubject::Returned { producer, .. } => Some(producer),
        NormalizedSubject::Instance { constructor, .. } => Some(constructor),
        NormalizedSubject::Direct => None,
    })
}

fn requires_project_overlay_spec(identity: &IdentitySpec) -> bool {
    matches!(
        identity,
        IdentitySpec::ModuleExport { .. }
            | IdentitySpec::PackageModuleExport { .. }
            | IdentitySpec::ModuleNamespace { .. }
            | IdentitySpec::PackageModuleNamespace { .. }
    )
}

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
fn collect_normalized_slots(root: &NormalizedRoot) -> Vec<u32> {
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
    // Build a sparse mapping from old slot value → position in sorted list.
    // VarId is public and can legally contain u32::MAX; indexing by the
    // largest authored value would otherwise attempt a multi-gigabyte
    // allocation before validation can reject or normalize the query.
    let mut map = BTreeMap::new();
    for (new_idx, &old) in slots.iter().enumerate() {
        map.insert(old, new_idx as u32);
    }
    apply_slot_map(root, &map);
    map
}

/// Normalize a [`QueryExpr`] into a [`NormalizedRoot`].
fn normalize_root(
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

// ── All normalization ─────────────────────────────────────────────────────

/// Normalize an `All` expression.
///
/// Same-event conjunction: all branches reference the same VarId →
/// merge into one `NormalizedEvent`.
///
/// Uncorrelated multi-event: no shared variable → error.
///
/// Other multi-event: reject as unsupported.
fn normalize_all_root(
    all: &AllExpr,
    emission: &EmissionDecl,
) -> Result<NormalizedRoot, QueryCompileError> {
    // Collect the set of distinct binding variables across branches.
    let branch_vars: Vec<Vec<VarId>> = all.branches.iter().map(collect_expr_vars).collect();

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
    let first_bindings = collect_binding_vars(&branches[0]);
    for var in &first_bindings {
        if branches.iter().skip(1).all(|b| {
            expr_references_var(b, *var)
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

/// Collect variables that are *bound* (not just referenced) in an expression.
fn collect_binding_vars(expr: &QueryExpr) -> Vec<VarId> {
    let mut vars = Vec::new();
    match &expr.kind {
        QueryExprKind::Event(eq) => vars.push(eq.var),
        QueryExprKind::SelectEvent(s) => vars.push(s.bind),
        QueryExprKind::Require(p) => match p {
            QueryPredicate::ReturnedObject { bind, .. }
            | QueryPredicate::ConstructedObject { bind, .. } => vars.push(*bind),
            _ => {}
        },
        QueryExprKind::Any(any) => {
            for b in &any.branches {
                vars.extend(collect_binding_vars(b));
            }
        }
        QueryExprKind::All(all) => {
            for b in &all.branches {
                vars.extend(collect_binding_vars(b));
            }
        }
        QueryExprKind::Lifecycle(lc) => {
            for src in &lc.sources {
                vars.push(src.var);
            }
        }
    }
    vars
}

/// Check whether an expression references a given variable.
fn expr_references_var(expr: &QueryExpr, target: VarId) -> bool {
    match &expr.kind {
        QueryExprKind::Event(eq) => eq.var == target,
        QueryExprKind::SelectEvent(s) => s.bind == target,
        QueryExprKind::Require(p) => match p {
            QueryPredicate::EventKind { event, .. }
            | QueryPredicate::EventIdentity { event, .. } => *event == target,
            QueryPredicate::Argument { call, .. } => *call == target,
            QueryPredicate::ReturnedObject { bind, .. }
            | QueryPredicate::ConstructedObject { bind, .. } => *bind == target,
            QueryPredicate::MemberSubject { event, object } => {
                *event == target || *object == target
            }
        },
        QueryExprKind::Any(any) => any.branches.iter().any(|b| expr_references_var(b, target)),
        QueryExprKind::All(all) => all.branches.iter().any(|b| expr_references_var(b, target)),
        QueryExprKind::Lifecycle(lc) => lc.sources.iter().any(|src| src.var == target),
    }
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
        arguments: constraints.into_boxed_slice(),
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

// ── Contradiction detection ────────────────────────────────────────────────

fn detect_event_contradictions(
    var: VarId,
    event: &EventSpec,
    identity: &IdentitySpec,
    subject: &NormalizedSubject,
    constraints: &[ArgumentConstraint],
) -> Result<(), QueryCompileError> {
    check_dimension_contradictions(var, event, identity, subject)?;
    check_argument_contradictions(var, constraints)
}

/// Check event/identity/subject dimension compatibility.
fn check_dimension_contradictions(
    var: VarId,
    event: &EventSpec,
    identity: &IdentitySpec,
    subject: &NormalizedSubject,
) -> Result<(), QueryCompileError> {
    if !matches!(subject, NormalizedSubject::Direct) {
        return Ok(());
    }
    let valid = match event {
        EventSpec::Call | EventSpec::Construct => matches!(
            identity,
            IdentitySpec::Global { .. }
                | IdentitySpec::Heuristic { .. }
                | IdentitySpec::ModuleExport { .. }
                | IdentitySpec::PackageModuleExport { .. }
        ),
        EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. } => matches!(
            identity,
            IdentitySpec::Rooted { .. }
                | IdentitySpec::Heuristic { .. }
                | IdentitySpec::ModuleNamespace { .. }
                | IdentitySpec::PackageModuleNamespace { .. }
        ),
        _ => true,
    };
    if !valid {
        return Err(QueryCompileError::ContradictoryPredicate {
            variable: var,
            detail: ContradictionKind::EventKind,
        });
    }
    Ok(())
}

/// Check contradictory argument constraints on the same index.
fn check_argument_contradictions(
    var: VarId,
    constraints: &[ArgumentConstraint],
) -> Result<(), QueryCompileError> {
    let mut by_index: BTreeMap<usize, Vec<&crate::api::rule::ArgumentMatcher>> = BTreeMap::new();
    for c in constraints {
        by_index.entry(c.index()).or_default().push(c.predicate());
    }

    for matchers in by_index.values() {
        check_empty_accepted_sets(var, matchers)?;
        check_disjoint_exact_sets(var, matchers)?;
        check_exact_prefix_contradiction(var, matchers)?;
    }
    Ok(())
}

/// Reject any static-string predicate whose accepted set is empty.
fn check_empty_accepted_sets(
    var: VarId,
    matchers: &[&crate::api::rule::ArgumentMatcher],
) -> Result<(), QueryCompileError> {
    use crate::api::rule::{ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcherKind};
    for m in matchers {
        if let ArgumentMatcherKind::Value(vm) = m.kind()
            && let ValueMatcherKind::StaticString(sp) = &vm.kind
        {
            let is_empty = matches!(
                &sp.kind,
                StaticStringPredicateKind::Exact(values)
                | StaticStringPredicateKind::ContainsAny(values)
                | StaticStringPredicateKind::ContainsAll(values)
                    if values.is_empty()
            );
            if is_empty {
                return Err(QueryCompileError::ContradictoryPredicate {
                    variable: var,
                    detail: ContradictionKind::StaticExactValues,
                });
            }
        }
    }
    Ok(())
}

/// Reject two disjoint exact sets on the same argument.
fn check_disjoint_exact_sets(
    var: VarId,
    matchers: &[&crate::api::rule::ArgumentMatcher],
) -> Result<(), QueryCompileError> {
    use crate::api::rule::{ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcherKind};
    let exact_sets: Vec<std::collections::BTreeSet<String>> = matchers
        .iter()
        .filter_map(|m| match m.kind() {
            ArgumentMatcherKind::Value(vm) => match &vm.kind {
                ValueMatcherKind::StaticString(sp) => match &sp.kind {
                    StaticStringPredicateKind::Exact(values) if !values.is_empty() => {
                        Some(values.iter().cloned().collect())
                    }
                    _ => None,
                },
                ValueMatcherKind::Any => None,
            },
            _ => None,
        })
        .collect();

    if exact_sets.len() >= 2 {
        for i in 0..exact_sets.len() {
            for j in (i + 1)..exact_sets.len() {
                if exact_sets[i].is_disjoint(&exact_sets[j]) {
                    return Err(QueryCompileError::ContradictoryPredicate {
                        variable: var,
                        detail: ContradictionKind::StaticExactValues,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Reject exact + prefix constraints when no exact value starts with any
/// prefix.
fn check_exact_prefix_contradiction(
    var: VarId,
    matchers: &[&crate::api::rule::ArgumentMatcher],
) -> Result<(), QueryCompileError> {
    use crate::api::rule::{ArgumentMatcherKind, StaticStringPredicateKind, ValueMatcherKind};
    let exact_values: Vec<&String> = matchers
        .iter()
        .filter_map(|m| match m.kind() {
            ArgumentMatcherKind::Value(vm) => match &vm.kind {
                ValueMatcherKind::StaticString(sp) => match &sp.kind {
                    StaticStringPredicateKind::Exact(values) => Some(values.iter()),
                    _ => None,
                },
                ValueMatcherKind::Any => None,
            },
            _ => None,
        })
        .flatten()
        .collect();

    let prefix_sets: Vec<&Vec<String>> = matchers
        .iter()
        .filter_map(|m| match m.kind() {
            ArgumentMatcherKind::Value(vm) => match &vm.kind {
                ValueMatcherKind::StaticString(sp) => match &sp.kind {
                    StaticStringPredicateKind::Prefix(values) => Some(values),
                    _ => None,
                },
                ValueMatcherKind::Any => None,
            },
            _ => None,
        })
        .collect();

    if exact_values.is_empty() || prefix_sets.is_empty() {
        return Ok(());
    }

    let any_compatible = exact_values.iter().any(|e| {
        prefix_sets
            .iter()
            .any(|prefixes| prefixes.iter().any(|p| e.starts_with(p.as_str())))
    });
    if !any_compatible {
        return Err(QueryCompileError::ContradictoryPredicate {
            variable: var,
            detail: ContradictionKind::StaticExactAndPrefix,
        });
    }
    Ok(())
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

// ── Variable collection helper ─────────────────────────────────────────────

fn collect_expr_vars(expr: &QueryExpr) -> Vec<VarId> {
    let mut vars = Vec::new();
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
                vars.extend(collect_expr_vars(b));
            }
        }
        QueryExprKind::All(all) => {
            for b in &all.branches {
                vars.extend(collect_expr_vars(b));
            }
        }
        QueryExprKind::Lifecycle(lc) => {
            for src in &lc.sources {
                vars.push(src.var);
            }
        }
    }
    vars
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
            query::{EmissionDecl, EventQuery, EventRequirement, EventSpec, IdentitySpec},
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

    fn normalize_ok(decl: &QueryDecl) -> NormalizedQuery {
        normalize_query_decl(decl).unwrap()
    }

    // ── Basic normalization tests ──────────────────────────────────

    #[test]
    fn simple_event_normalizes_to_event_root() {
        let d = decl(event(0, "fetch"), 0, "fetch");
        let nq = normalize_ok(&d);
        assert!(matches!(nq.root(), NormalizedRoot::Event(_)));
    }

    #[test]
    fn lifecycle_normalizes_to_lifecycle_root() {
        let source = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("document.createElement"),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("document.createElement"),
            },
            constraints: vec![],
        };
        let lc = LifecycleQuery::new(
            "remote-script",
            vec![source],
            Some(
                crate::api::rule::LifecycleCondition::event(
                    crate::api::rule::LifecycleEvent::property_write(
                        "src",
                        ValueMatcher::any_value(),
                    ),
                )
                .unwrap(),
            ),
            Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
        )
        .unwrap();
        let d = QueryDecl {
            expression: QueryExpr::lifecycle(lc),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::CallArgument,
                symbol: "remote-script".into(),
            },
        };
        let nq = normalize_ok(&d);
        assert!(matches!(nq.root(), NormalizedRoot::Lifecycle(_)));
    }

    // ── Any flattening tests ───────────────────────────────────────

    #[test]
    fn flattens_nested_any() {
        let inner = AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap();
        let outer =
            AnyExpr::new(vec![event(2, "c"), QueryExpr::any(inner), event(3, "d")]).unwrap();
        let d = decl(QueryExpr::any(outer), 0, "test");
        let nq = normalize_ok(&d);
        match nq.root() {
            NormalizedRoot::Any(branches) => {
                assert_eq!(branches.len(), 4);
            }
            other => panic!("expected Any, got {other:?}"),
        }
    }

    #[test]
    fn does_not_flatten_any_into_all() {
        let inner_any = AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap();
        // All containing an Any: not flattened.
        let all = AllExpr::new(vec![event(2, "c"), QueryExpr::any(inner_any)]).unwrap();
        let d = decl(QueryExpr::all(all), 2, "test");
        let result = normalize_query_decl(&d);
        // If the All is multi-event without common var, it fails as uncorrelated.
        assert!(result.is_err());
    }

    // ── Deduplication tests ────────────────────────────────────────

    #[test]
    fn deduplicates_identical_branches_in_any() {
        let branches = vec![event(0, "a"), event(0, "a"), event(1, "b")];
        let d = decl(QueryExpr::any(AnyExpr::new(branches).unwrap()), 0, "test");
        let nq = normalize_ok(&d);
        match nq.root() {
            NormalizedRoot::Any(branches) => {
                assert_eq!(branches.len(), 2);
            }
            other => panic!("expected Any, got {other:?}"),
        }
    }

    // ── Canonical ordering tests ───────────────────────────────────

    #[test]
    fn branches_are_sorted_canonically() {
        let branches = vec![event(1, "z"), event(0, "a")];
        let d = decl(QueryExpr::any(AnyExpr::new(branches).unwrap()), 0, "test");
        let nq = normalize_ok(&d);
        match nq.root() {
            NormalizedRoot::Any(roots) => {
                assert_eq!(roots.len(), 2);
                // Event slots should be sorted.
                assert_eq!(roots[0].slot_or_zero(), 0);
                assert_eq!(roots[1].slot_or_zero(), 1);
            }
            other => panic!("expected Any, got {other:?}"),
        }
    }

    // Helper to extract slot from NormalizedRoot
    trait SlotAccess {
        fn slot_or_zero(&self) -> u32;
    }
    impl SlotAccess for NormalizedRoot {
        fn slot_or_zero(&self) -> u32 {
            match self {
                Self::Event(ev) => ev.slot,
                _ => 0,
            }
        }
    }

    #[test]
    fn equivalent_builder_forms_normalize_equally() {
        let a_branches = vec![event(0, "fetch"), event(1, "navigate")];
        let b_branches = vec![event(1, "navigate"), event(0, "fetch")];
        let a_expr = QueryExpr::any(AnyExpr::new(a_branches).unwrap());
        let b_expr = QueryExpr::any(AnyExpr::new(b_branches).unwrap());
        let a_d = decl(a_expr, 0, "test");
        let b_d = decl(b_expr, 0, "test");
        let a_nq = normalize_ok(&a_d);
        let b_nq = normalize_ok(&b_d);
        assert_eq!(a_nq, b_nq);
    }

    // ── Idempotency tests ──────────────────────────────────────────

    #[test]
    fn normalization_is_idempotent() {
        let branches = vec![
            QueryExpr::any(AnyExpr::new(vec![event(0, "a"), event(1, "b")]).unwrap()),
            event(2, "c"),
        ];
        let d = decl(QueryExpr::any(AnyExpr::new(branches).unwrap()), 0, "test");
        let normalized = normalize_ok(&d);
        let slots = collect_normalized_slots(normalized.root());
        assert_eq!(slots, vec![0, 1, 2]);
        assert_eq!(
            normalized.requirements(),
            &PlanRequirements::for_root(normalized.root())
        );
        match normalized.root() {
            NormalizedRoot::Any(branches) => {
                assert!(
                    branches
                        .iter()
                        .all(|branch| { !matches!(branch, NormalizedRoot::Any(_)) })
                );
            }
            other => panic!("expected normalized Any, got {other:?}"),
        }
    }

    #[test]
    fn normalization_of_simple_event_is_idempotent() {
        let d = decl(event(0, "fetch"), 0, "fetch");
        let once = normalize_ok(&d);
        let twice = normalize_ok(&d);
        assert_eq!(once, twice);
    }

    // ── Same-event All merging tests ───────────────────────────────

    #[test]
    fn same_event_all_merges_into_one_normalized_event() {
        let event_query = EventQuery::call_global("fetch").unwrap();
        let req: Result<EventRequirement, _> =
            Ok(EventRequirement::argument(0, ValueMatcher::static_string()).unwrap());
        let d = QueryDecl::all(Ok(event_query), [req]).unwrap();
        let nq = normalize_ok(&d);
        match nq.root() {
            NormalizedRoot::Event(ev) => {
                assert_eq!(ev.arguments.len(), 1);
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn same_event_all_with_multiple_constraints_merges_all_constraints() {
        let event_query = EventQuery::call_global("fetch").unwrap();
        let reqs: Vec<Result<EventRequirement, _>> = vec![
            Ok(EventRequirement::argument(0, ValueMatcher::static_string()).unwrap()),
            Ok(
                EventRequirement::argument(1, ValueMatcher::static_string().equals("/api"))
                    .unwrap(),
            ),
        ];
        let d = QueryDecl::all(Ok(event_query), reqs).unwrap();
        let nq = normalize_ok(&d);
        match nq.root() {
            NormalizedRoot::Event(ev) => {
                assert_eq!(ev.arguments.len(), 2);
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    // ── Contradiction detection tests ──────────────────────────────

    #[test]
    fn incompatible_event_kinds_in_all_produce_contradiction() {
        // Two EventQuery branches with different event kinds but same var.
        let a = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        let b = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Construct, // different from Call
            identity: IdentitySpec::Global {
                name: SmolStr::new("URL"),
            },
            constraints: vec![],
        });
        let all = AllExpr::new(vec![a, b]).unwrap();
        let d = decl(QueryExpr::all(all), 0, "test");
        let result = normalize_query_decl(&d);
        assert!(
            matches!(
                result,
                Err(QueryCompileError::ContradictoryPredicate {
                    detail: ContradictionKind::EventKind,
                    ..
                })
            ),
            "expected ContradictoryPredicate(EventKind), got {result:?}"
        );
    }

    #[test]
    fn incompatible_identities_in_all_produce_contradiction() {
        let a = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        let b = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("navigate"), // different name
            },
            constraints: vec![],
        });
        let all = AllExpr::new(vec![a, b]).unwrap();
        let d = decl(QueryExpr::all(all), 0, "test");
        let result = normalize_query_decl(&d);
        assert!(
            matches!(
                result,
                Err(QueryCompileError::ContradictoryPredicate {
                    detail: ContradictionKind::StrictIdentity,
                    ..
                })
            ),
            "expected ContradictoryPredicate(StrictIdentity), got {result:?}"
        );
    }

    #[test]
    fn compatible_identities_in_all_pass() {
        // Same global name, therefore compatible.
        let event_query = EventQuery::call_global("fetch").unwrap();
        let d = QueryDecl::all(Ok(event_query), []).unwrap();
        assert!(normalize_query_decl(&d).is_ok());
    }

    // ── Uncorrelated multi-event All tests ─────────────────────────

    #[test]
    fn uncorrelated_all_fails_with_uncorrelated_conjunction() {
        let a = QueryExpr::event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            constraints: vec![],
        });
        let b = QueryExpr::event(EventQuery {
            var: VarId::new(1), // different var
            event: EventSpec::MemberCall {
                member: SymbolPath::from("doc.createElement"),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("doc.createElement"),
            },
            constraints: vec![],
        });
        let all = AllExpr::new(vec![a, b]).unwrap();
        let d = decl(QueryExpr::all(all), 0, "test");
        let result = normalize_query_decl(&d);
        assert!(
            matches!(result, Err(QueryCompileError::UncorrelatedConjunction)),
            "expected UncorrelatedConjunction, got {result:?}"
        );
    }

    // ── Plan requirements tests ────────────────────────────────────

    #[test]
    fn simple_query_has_no_matcher_specific_preparation_requirements() {
        let d = decl(event(0, "fetch"), 0, "fetch");
        let nq = normalize_ok(&d);
        let req = nq.requirements();
        assert!(req.value_resolution().is_empty());
        assert!(!req.flow().local);
        assert!(!req.needs_project_overlay());
    }

    #[test]
    fn constrained_query_has_fact_stream() {
        let eq = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string())
            .unwrap();
        let d = eq.into_query();
        let nq = normalize_ok(&d);
        let req = nq.requirements();
        assert!(
            req.value_resolution()
                .contains(&ValueResolutionRequirement::LocalStaticValues)
        );
    }

    #[test]
    fn module_query_has_project_overlay() {
        let d = QueryDecl::call_module("fs", "readFile").unwrap();
        let nq = normalize_ok(&d);
        let req = nq.requirements();
        assert!(req.needs_project_overlay());
        assert_eq!(
            req.project_requirements(),
            &BTreeSet::from([
                ProjectRequirement::ExactModuleExports,
                ProjectRequirement::CallResultIdentities,
            ])
        );
    }

    #[test]
    fn global_query_does_not_need_project_overlay() {
        let d = decl(event(0, "fetch"), 0, "fetch");
        let nq = normalize_ok(&d);
        let req = nq.requirements();
        assert!(!req.needs_project_overlay());
    }

    // ── Any requirement merging tests ──────────────────────────────

    #[test]
    fn any_merges_requirements_from_branches() {
        let branches = vec![
            QueryDecl::call_global("fetch").unwrap(),
            QueryDecl::call_module("fs", "readFile").unwrap(),
        ];
        let any = QueryDecl::any(branches.into_iter().map(Ok)).unwrap();
        let d = any.with_evidence(MatchKind::Call, "test");
        let nq = normalize_ok(&d);
        let req = nq.requirements();
        assert!(
            req.needs_project_overlay(),
            "Any with module branch should need project overlay"
        );
    }

    #[test]
    fn lifecycle_has_flow_requirements() {
        let source = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("document.createElement"),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("document.createElement"),
            },
            constraints: vec![],
        };
        let lc = LifecycleQuery::new(
            "test",
            vec![source],
            Some(
                crate::api::rule::LifecycleCondition::event(
                    crate::api::rule::LifecycleEvent::property_write(
                        "src",
                        ValueMatcher::any_value(),
                    ),
                )
                .unwrap(),
            ),
            Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
        )
        .unwrap();
        let d = QueryDecl {
            expression: QueryExpr::lifecycle(lc),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::CallArgument,
                symbol: "test".into(),
            },
        };
        let nq = normalize_ok(&d);
        let req = nq.requirements();
        assert!(req.flow().local, "lifecycle should need local flow");
        assert!(
            req.flow().cross_call,
            "lifecycle should need cross-call flow"
        );
    }

    #[test]
    fn global_query_has_only_calls_requirement() {
        let d = QueryDecl::call_global("fetch").unwrap();
        let nq = normalize_ok(&d);
        let req = nq.requirements();
        assert!(req.value_resolution().is_empty());
        assert!(!req.flow().local);
        assert!(!req.flow().cross_call);
        assert!(!req.needs_project_overlay());
    }

    // ── Lifecycle preservation ─────────────────────────────────────

    #[test]
    fn lifecycle_is_not_flattened_or_sorted() {
        let source = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("document.createElement"),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("document.createElement"),
            },
            constraints: vec![],
        };
        let lc = LifecycleQuery::new(
            "test",
            vec![source],
            Some(
                crate::api::rule::LifecycleCondition::event(
                    crate::api::rule::LifecycleEvent::property_write(
                        "type",
                        ValueMatcher::any_value(),
                    ),
                )
                .unwrap(),
            ),
            Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
        )
        .unwrap();
        let d = QueryDecl {
            expression: QueryExpr::lifecycle(lc),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::CallArgument,
                symbol: "test".into(),
            },
        };
        let nq = normalize_ok(&d);
        assert!(matches!(nq.root(), NormalizedRoot::Lifecycle(_)));
    }

    // ── Edge cases ─────────────────────────────────────────────────

    #[test]
    fn single_branch_any_is_preserved() {
        let d = decl(
            QueryExpr::any(AnyExpr::new(vec![event(0, "a")]).unwrap()),
            0,
            "test",
        );
        let nq = normalize_ok(&d);
        // Single branch should be unwrapped from Any.
        assert!(matches!(nq.root(), NormalizedRoot::Event(_)));
    }

    #[test]
    fn single_branch_all_is_normalized_to_event() {
        let all = AllExpr::new(vec![event(0, "a")]).unwrap();
        let d = decl(QueryExpr::all(all), 0, "a");
        let nq = normalize_ok(&d);
        assert!(matches!(nq.root(), NormalizedRoot::Event(_)));
    }

    // ── NormalizedQuery emission accessors ─────────────────────────

    #[test]
    fn normalize_preserves_emission_kind_and_symbol() {
        let d = decl(event(0, "fetch"), 0, "fetch");
        let nq = normalize_ok(&d);
        assert_eq!(nq.emission().symbol(), "fetch");
        assert_eq!(nq.emission().kind(), MatchKind::Call);
    }

    // ── Reversed order tests ───────────────────────────────────────

    #[test]
    fn reversed_argument_orders_normalize_equally() {
        let eq = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(1, ValueMatcher::static_string())
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().equals("/api"))
            .unwrap();
        let d = eq.into_query();
        let nq = normalize_ok(&d);
        match nq.root() {
            NormalizedRoot::Event(ev) => {
                // Arguments should be sorted by index.
                assert_eq!(ev.arguments.len(), 2);
                assert_eq!(ev.arguments[0].index(), 0);
                assert_eq!(ev.arguments[1].index(), 1);
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn reversed_alternative_order_normalizes_equally() {
        let a_branches = vec![event(0, "fetch"), event(1, "navigate")];
        let b_branches = vec![event(1, "navigate"), event(0, "fetch")];
        let a_d = decl(QueryExpr::any(AnyExpr::new(a_branches).unwrap()), 0, "test");
        let b_d = decl(QueryExpr::any(AnyExpr::new(b_branches).unwrap()), 0, "test");
        let a_nq = normalize_ok(&a_d);
        let b_nq = normalize_ok(&b_d);
        assert_eq!(a_nq, b_nq);
    }

    // ── No NormalizedAll variant test ──────────────────────────────

    #[test]
    fn no_normalized_all_variant_exists() {
        // Check that there is no All variant by exhaustiveness.
        // Any All expression should be normalized away.
        let event_query = EventQuery::call_global("fetch").unwrap();
        let d = QueryDecl::all(Ok(event_query), []).unwrap();
        let nq = normalize_ok(&d);
        match nq.root() {
            NormalizedRoot::Event(_) => {} // OK
            _ => panic!("expected Event after normalization of single-branch All"),
        }
    }

    // ── Alpha-equivalence tests ───────────────────────────────────

    #[test]
    fn alpha_equivalent_variable_ids_normalize_equally() {
        // Same structure, different VarIds — should produce the same normalized form.
        let a_branches = vec![event(10, "fetch"), event(20, "navigate")];
        let b_branches = vec![event(30, "fetch"), event(40, "navigate")];
        let a_d = decl(
            QueryExpr::any(AnyExpr::new(a_branches).unwrap()),
            10,
            "test",
        );
        let b_d = decl(
            QueryExpr::any(AnyExpr::new(b_branches).unwrap()),
            30,
            "test",
        );
        let a_nq = normalize_ok(&a_d);
        let b_nq = normalize_ok(&b_d);
        assert_eq!(a_nq, b_nq);
        // Verify slots are dense 0..n after alpha-renumbering.
        match a_nq.root() {
            NormalizedRoot::Any(branches) => {
                let mut slots: Vec<u32> = branches
                    .iter()
                    .map(|b| match b {
                        NormalizedRoot::Event(ev) => ev.slot,
                        _ => u32::MAX,
                    })
                    .collect();
                slots.sort_unstable();
                assert_eq!(slots, vec![0, 1], "slots should be dense 0..n");
            }
            other => panic!("expected Any, got {other:?}"),
        }
    }

    #[test]
    fn alpha_equivalent_single_event_normalizes_equally() {
        // Single events with different VarIds produce the same slot after remapping.
        let a = decl(event(5, "fetch"), 5, "fetch");
        let b = decl(event(99, "fetch"), 99, "fetch");
        assert_eq!(normalize_ok(&a), normalize_ok(&b));
        match normalize_ok(&a).root() {
            NormalizedRoot::Event(ev) => assert_eq!(ev.slot, 0, "slot should be 0 after alpha"),
            other => panic!("expected Event, got {other:?}"),
        }
    }

    // ── Contradiction – exact/prefix ───────────────────────────────

    #[test]
    fn exact_and_prefix_contradiction_is_detected() {
        // exact("foo") and prefix("bar") cannot both match.
        let eq = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().equals("foo"))
            .unwrap()
            .with_arg(
                0,
                ValueMatcher::static_string()
                    .starts_with_any(["bar"])
                    .unwrap(),
            )
            .unwrap();
        let result = normalize_query_decl(&eq.into_query());
        assert!(
            matches!(
                result,
                Err(QueryCompileError::ContradictoryPredicate {
                    detail: ContradictionKind::StaticExactAndPrefix,
                    ..
                })
            ),
            "expected StaticExactAndPrefix contradiction, got {result:?}"
        );
    }

    #[test]
    fn exact_and_non_contradictory_prefix_passes() {
        // exact("foobar") and prefix("foo") are compatible.
        let eq = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().equals("foobar"))
            .unwrap()
            .with_arg(
                0,
                ValueMatcher::static_string()
                    .starts_with_any(["foo"])
                    .unwrap(),
            )
            .unwrap();
        assert!(normalize_query_decl(&eq.into_query()).is_ok());
    }

    // ── Contradiction – empty accepted sets ────────────────────────

    #[test]
    fn empty_exact_set_is_rejected_at_construction() {
        let empty: Vec<&str> = vec![];
        assert!(matches!(
            ValueMatcher::static_string().equals_any(empty),
            Err(crate::api::rule::QueryBuildError::EmptyCollection(_))
        ));
    }

    #[test]
    fn empty_contains_any_set_is_rejected_at_construction() {
        let empty: Vec<String> = vec![];
        assert!(matches!(
            ValueMatcher::static_string().contains_any(empty),
            Err(crate::api::rule::QueryBuildError::EmptyCollection(_))
        ));
    }

    // ── Normalized validation tests ────────────────────────────────

    #[test]
    fn normalized_root_slots_are_dense_after_alpha_renumber() {
        // Event with var=99 should become slot=0 after alpha renumbering.
        let d = decl(event(99, "fetch"), 99, "fetch");
        let nq = normalize_ok(&d);
        match nq.root() {
            NormalizedRoot::Event(ev) => {
                assert_eq!(ev.slot, 0, "slot should be renumbered to 0");
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn normalized_any_branches_have_dense_slots() {
        // Any with vars 10, 20, 5 should produce slots 0, 1, 2.
        let branches = vec![event(10, "a"), event(20, "b"), event(5, "c")];
        let d = decl(QueryExpr::any(AnyExpr::new(branches).unwrap()), 10, "test");
        let nq = normalize_ok(&d);
        match nq.root() {
            NormalizedRoot::Any(roots) => {
                let mut slots: Vec<u32> = roots
                    .iter()
                    .map(|r| match r {
                        NormalizedRoot::Event(ev) => ev.slot,
                        _ => u32::MAX,
                    })
                    .collect();
                slots.sort_unstable();
                assert_eq!(slots, vec![0, 1, 2], "slots should be dense 0..2");
            }
            other => panic!("expected Any, got {other:?}"),
        }
    }

    // ── Compatibility with catalog compilation ─────────────────────

    #[test]
    fn normalized_query_compiles_through_full_pipeline() {
        let d = QueryDecl::call_global("fetch").unwrap();
        let nq = normalize_query_decl(&d).unwrap();
        // Physical planning uses NormalizedQuery
        let _plan = crate::api::compiler::physical::plan_normalized(&nq);
    }

    // ── Duplicate filter deduplication ──────────────────────────────

    #[test]
    fn duplicate_filters_do_not_duplicate_work_or_evidence() {
        // Same constraint specified twice should produce only one normalized
        // constraint (deduplication in canonicalization step).
        let eq = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().equals("/api"))
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().equals("/api"))
            .unwrap();
        let nq = normalize_query_decl(&eq.into_query()).unwrap();
        match nq.root() {
            NormalizedRoot::Event(ev) => {
                assert_eq!(
                    ev.arguments.len(),
                    1,
                    "duplicate constraints must be deduplicated"
                );
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_filters_in_all_are_deduplicated() {
        // Same constraint on the same index across two branches should be
        // deduplicated during All merging.
        let eq = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::static_string().equals("/api"))
            .unwrap();
        let req =
            EventRequirement::argument(0, ValueMatcher::static_string().equals("/api")).unwrap();
        let d = QueryDecl::all(Ok(eq), [Ok(req)]).unwrap();
        let nq = normalize_query_decl(&d).unwrap();
        match nq.root() {
            NormalizedRoot::Event(ev) => {
                assert_eq!(
                    ev.arguments.len(),
                    1,
                    "duplicate constraints from All branches must be deduplicated"
                );
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }

    // ── Distinct lifecycle condition ordering ───────────────────────

    #[test]
    fn distinct_lifecycle_conditions_never_compare_as_same_ordering_key() {
        // Two lifecycle queries with different condition contents must
        // compare differently, not just by condition presence.
        let source_a = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("document.createElement"),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("document.createElement"),
            },
            constraints: vec![],
        };
        let source_b = EventQuery {
            var: VarId::new(1),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("doc.createElement"),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("doc.createElement"),
            },
            constraints: vec![],
        };

        // Different conditions: property_write("src") vs property_write("href")
        let lc_a = LifecycleQuery::new(
            "test-a",
            vec![source_a],
            Some(
                crate::api::rule::LifecycleCondition::event(
                    crate::api::rule::LifecycleEvent::property_write(
                        "src",
                        ValueMatcher::any_value(),
                    ),
                )
                .unwrap(),
            ),
            Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
        )
        .unwrap();

        let lc_b = LifecycleQuery::new(
            "test-b",
            vec![source_b],
            Some(
                crate::api::rule::LifecycleCondition::event(
                    crate::api::rule::LifecycleEvent::property_write(
                        "href",
                        ValueMatcher::any_value(),
                    ),
                )
                .unwrap(),
            ),
            Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
        )
        .unwrap();

        let d_a = QueryDecl {
            expression: QueryExpr::lifecycle(lc_a),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::CallArgument,
                symbol: "test-a".into(),
            },
        };
        let d_b = QueryDecl {
            expression: QueryExpr::lifecycle(lc_b),
            emission: EmissionDecl {
                primary_var: VarId::new(1),
                kind: MatchKind::CallArgument,
                symbol: "test-b".into(),
            },
        };

        let nq_a = normalize_ok(&d_a);
        let nq_b = normalize_ok(&d_b);
        // Different conditions -> different ordering keys -> not equal.
        assert_ne!(
            nq_a, nq_b,
            "lifecycle queries with different conditions must not compare equal"
        );
    }

    // ── Unknown-sensitive forms are not over-simplified ─────────────

    #[test]
    fn unknown_sensitive_forms_are_not_over_simplified() {
        // A value matcher that accepts any (dynamic/unknown) value must
        // not be simplified away or converted to a different predicate.
        // The Any value matcher is valid and must be preserved.
        let eq = EventQuery::call_global("fetch")
            .unwrap()
            .with_arg(0, ValueMatcher::any_value())
            .unwrap();
        let nq = normalize_query_decl(&eq.into_query()).unwrap();
        match nq.root() {
            NormalizedRoot::Event(ev) => {
                assert_eq!(
                    ev.arguments.len(),
                    1,
                    "AnyValue constraint must be preserved through normalization"
                );
                let matcher = ev.arguments[0].predicate();
                assert_eq!(
                    matcher.kind(),
                    &crate::api::rule::ArgumentMatcherKind::Value(
                        crate::api::rule::ValueMatcher::any_value()
                    )
                );
            }
            other => panic!("expected Event, got {other:?}"),
        }
    }
}
