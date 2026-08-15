# Codebase Readability Audit — glass-lint-core Chunk 17: Project linking

## Summary

Chunk 17 owns the staged project-linking pipeline in `analysis::project`:
`identities`, `linker` (+ `export`, `graph`), `model`, `projection`
(+ `outcome`), `resolver`, and `state`. The module structure is clean: local
artifacts stay immutable, linking state is additive, the linker is consumed
into a final `ProjectSemanticModel`, and projection returns outcome side
effects instead of mutating through shared references. There are no panics in
production code, fail-closed behavior (Rejected SCC, Unknown resolutions,
unavailable effects) is preserved, and bounded budgets are enforced.

The main readability opportunities are (a) an immediately-consumed argument
bundle (`ProjectionInputs` in `projection.rs`, which also collides in name
with a genuinely reused type in the flow projector), (b) a redundant export
memo re-read inside the resolver's lookup chain, (c) a repeated
"resolve a named import/export through a request target" sequence spread over
three modules, (d) a transparent one-field `LinkingSession` whose field is
poked directly and whose name overstates its role during projection/matching,
and (e) over-wrapped transient linker state (`Option<NormalizedModuleGraph>` +
a three-state `SccPartitionState` plus a `mem::replace` borrow dance). A few
smaller items cover limit-field duplication with a stale `#[allow(dead_code)]`,
uneven accessor surfaces on the module-qualified ID newtypes, repeated
fall-through guards in `project_facts`, and a doubly-implemented
star-export-disagreement policy.

## Findings

### Projection orchestration (`analysis/project/projection.rs`)

#### [x] READ-001 — `ProjectionInputs` is an immediately-consumed argument bundle that collides with the flow projector's same-named type

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:244-251` (def), `:181-191` (only construction), `:257-264` (immediate destructure)

`ProjectionInputs` groups six fields (`facts`, `effects`, `plan`,
`flow_limits`, `module_id`, `trace_arena`) and is built at exactly one call
site (`project_modules`, projection.rs:181-191) only to be destructured at the
top of `project_facts` (projection.rs:257-264). It adds no invariant or
vocabulary — it exists to shrink the `project_facts` argument list. Its name
also collides with `ProjectionInputs<'rules, 'stream>` in
`analysis/flow/projector/mod.rs:225-244`, a real multi-call owner constructed
via `ProjectionInputs::new` and held across a whole run, so the two names now
denote unrelated shapes.

**Recommendation:** Pass the six already-bound fields directly to
`project_facts` (a single-level helper) and delete the struct, or, if a bundle
is genuinely wanted, reuse/rename so the project-level type stops shadowing the
flow-projector type. Guardrails: keep the `trace_arena: &mut TraceArena`
borrow shape intact, and do not touch the flow projector's
`ProjectionInputs`, which has a distinct lifetime structure and owner.

**Fix Applied:** Deleted `ProjectionInputs` and passed the six fields directly to `project_facts`; no collision remains with the flow projector's type.

#### [x] READ-002 — Redundant re-read of the export memo table inside `ExportResolver::lookup_export_body`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:158-160` and `:189-191`

`lookup_export_inner` (resolver.rs:158-160) already returns early when
`self.exports.resolve(id)` has an entry, then delegates to
`lookup_export_body`, which performs the identical `self.exports.resolve(id)`
check again after `walk_star_exports` (resolver.rs:189-191). Nothing in the
star walk mutates `self.exports`: recursion only reads the export table and
writes the separate `ExportLookupCache` (`self.cache.insert`, resolver.rs:193).
The post-walk check therefore always misses and misleads readers into
believing the star walk can fill the memo.

**Recommendation:** Remove the post-walk re-check in `lookup_export_body`; the
single pre-walk check in `lookup_export_inner` is sufficient. Guardrail: if a
future path ever memoizes into `exports` during lookup, re-introduce the check
with a comment; keep the early `DEFAULT_EXPORT`, unknown-interface, and cache
handling as-is.

**Fix Applied:** Removed the dead post-walk `self.exports.resolve` re-check in `lookup_export_body`; the pre-walk check in `lookup_export_inner` alone covers the memo.

### Export resolution (`analysis/project/resolver.rs`, `linker/export.rs`, `identities.rs`)

#### [x] READ-003 — "Resolve a named import/export through a request target" is reimplemented in four call sites with drifted fallback semantics

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `resolver.rs:130-135`, `linker/export.rs:324-329`, `linker/export.rs:289-291`, `identities.rs:234-235`

The sequence "look up the resolution for a request key, map `Internal` to a
recursive `lookup_export(...).unwrap_or(Unknown)`, map other targets through a
target→resolution conversion" appears in `ExportResolver::resolve_imported_identity`
(resolver.rs:130-135) and `ProjectLinker::resolve_request_export`
(export.rs:324-329); the namespace flavor repeats only the conversion — no
recursion, any target including `Internal` maps to a `Qualified` `"*"`
identity — in `resolve_namespace_export` (export.rs:289-291) and
`ProjectSemanticModel::resolve_namespace` (identities.rs:234-235). The two
conversion helpers already drifted: `resolve_request_export` applies
`linked_target_to_export_resolution` and treats `None` separately, while
`resolve_imported_identity` uses `target_to_export_resolution` (which folds the
`None` case through the authored specifier). identities.rs:235 also uses the
bare literal `"*"` where export.rs uses `NAMESPACE_EXPORT`.

**Recommendation:** Extract one `ExportResolver` helper that owns the shared
target match plus the `Internal` recursive lookup (e.g.
`resolve_request_target(module, request, exported) -> Option<ExportResolution>`,
returning `None` when no resolution is recorded) and route
`resolve_imported_identity` and `resolve_request_export` through it while each
call site keeps its own `None` fallback (authored-specifier fold vs `Unknown`).
Do not route the namespace sites through it: `resolve_namespace_export` and
`resolve_namespace` deliberately resolve any target — including `Internal` —
to a `Qualified` `"*"` identity without walking, so a recursive helper would
change their behavior; just replace the bare literal `"*"` at identities.rs:235
with `NAMESPACE_EXPORT`. Guardrails: keep `target_to_export_resolution` and
`linked_target_to_export_resolution` as separate conversions (the
authored-specifier fallback is a distinct contract), and preserve the
fail-closed `Unknown` for unsupported/missing targets.

**Fix Applied:** Added `ExportResolver::resolve_request_target` owning the target match plus the `Internal` recursive lookup; routed `resolve_imported_identity` and `resolve_request_export` through it with each site's own `None` fallback. The `"*"` literal at identities.rs:235 was already `NAMESPACE_EXPORT`.

### Linking session and linker state (`analysis/project/state.rs`, `linker/mod.rs`, `linker/export.rs`)

#### [x] READ-004 — `LinkingSession` is a transparent one-field wrapper whose field is poked directly and whose name misleads during projection

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `state.rs:299-309` (def), `state.rs:300` (`pub(super) lookup_cache`), `linker/mod.rs:68`, `model.rs:338`

`LinkingSession` wraps exactly one field, `pub(super) lookup_cache:
ExportLookupCache`, and callers reach through it (`&mut self.lookup_session.lookup_cache`
at linker/mod.rs:68; `&mut session.lookup_cache` at model.rs:338), so the type
adds no encapsulation. Its name is also wrong for half its uses: it is
constructed during projection (`ProjectionSession::new`, projection.rs:121) and
threaded through cross-module matching (`flow/cross/graph.rs:20`,
`flow/cross/mod.rs:248`), i.e. it is an export-lookup cache context for both
linking and matching, not a "linking" session.

**Recommendation:** Either delete the type and pass `ExportLookupCache` (its
real owner) directly, or rename it (e.g. `ExportLookupSession`) and hide the
cache behind a narrow `cache_mut()` accessor so the vocabulary becomes real.
Guardrails: keep one cache per pass — the linker pass and the projection pass
must not share a cache — keep the `ExportLookupCache` capacity bound intact,
and update the `assert_send::<LinkingSession>()` check (model/tests.rs:14-16)
to whichever type survives the change.

**Fix Applied:** Deleted `LinkingSession` and threaded `ExportLookupCache` directly through the linker, projection session, model, identities, and cross-flow callers; one cache per pass retained and the `assert_send` check updated to `ExportLookupCache`.

#### [x] READ-005 — `ProjectLinker` transient state is over-wrapped (`Option<graph>` plus a three-state enum) and forces a `mem::replace` borrow dance

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `linker/mod.rs:26-36`, `:48`, `:148-155`; `linker/export.rs:44-46`, `:61`

`graph: Option<NormalizedModuleGraph>` (linker/mod.rs:48) is always `Some`
after `collect_graph_edges` (the only production path is
`link_with_limits` → `propagate_local_status` → `build_graph_and_exports` →
`finish`), so `finish`'s `as_ref().map_or(0, ...)` (linker/mod.rs:152-155)
guards a state that never occurs. `SccPartitionState` (Pending/Ready/Rejected)
is another Option-like: `Pending` exists only as the initial value and inside
`std::mem::replace` (export.rs:44-46) used to hold the partition while calling
`&mut self` methods, then is re-installed as `Ready` (export.rs:61). Only
`Rejected` vs `Ready` is ever observable after construction.

**Recommendation:** Store `NormalizedModuleGraph` directly — `collect_graph_edges`
always sets it before the sole `finish` path, so the `Option` is dead — and
replace `SccPartitionState` with `Option<SccPartition>` (`None` = not yet
partitioned or rejected), turning the `Pending` placeholder in
`resolve_export_table` into a standard `take()`/restore. The take/restore swap
itself is inherent — the partition must move out of `self` to call the
`&mut self` resolution methods — so it survives the change; what is removed is
the bespoke three-state enum and the dead `Option`. Guardrails: keep the
`is_ready()` gate that skips `resolve_export_table`/`validate_imported_exports`
on a rejected partition (fail-closed), and keep the budget-exhaustion
diagnostics for the rejected case in `collect_graph_edges`.

**Fix Applied:** Stored `NormalizedModuleGraph` directly (added `Default`) and replaced `SccPartitionState` with `Option<SccPartition>`, using a standard `take()`/restore in `resolve_export_table` and keeping the fail-closed `is_some()` gate.

### Projection helper and model limits (`analysis/project/projection.rs`, `model.rs`)

#### [ ] READ-006 — `project_facts` repeats the same default early return and double-checks effect availability

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `projection.rs:266`, `:275`, `:278`, `:281`; `:175-177`

`project_facts` returns the identical tuple
`(projected_evidence, LocalFlowProjectionOutcome::default())` from four
guards: not projectable (projection.rs:266), no flow matchers (:275), no
effects (:278), effects unavailable (:281). Three of the four
(`:275`/`:278`/`:281`) are one decision ("no flow to project"); meanwhile the
caller `project_modules` repeats the availability decision when calling
`outcome.record_effects` (:175-177) with the same `effects.is_available()`
guard, so the availability check exists twice.

**Recommendation:** Collapse the three flow/effects early returns in
`project_facts` (projection.rs:275, :278, :281) into one combined guard:
`effects` is `Some` exactly when `plan.flow_matchers` is non-empty
(projection.rs:174), so the empty-list and `None` checks are the same
condition, and the `is_available()` check can ride on the same guard. Leave
`project_modules`'s `record_effects` guard alone — it is a separate
outcome-recording decision (availability plus `completion().is_incomplete()`),
so sharing one `is_available()` decision there would couple unrelated phases.
Guardrails: keep the not-projectable short-circuit distinct — it must also
skip constrained matching, not just flow — and keep the
`effects.completion().is_incomplete()` record distinct from the
not-available case.

**Fix Applied:** None so far.

#### [ ] READ-007 — `ProjectSemanticModel` duplicates a constant as a field and carries a stale `#[allow(dead_code)]`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `model.rs:238`, `:258`, `:290`, `:439-441`, `:443-446`

`export_lookup_capacity` is assigned the constant `MAX_EXPORT_LOOKUP_ENTRIES`
in both constructors (`from_linker` at model.rs:258 and the test-only
`single_with_limits` at :290) and is only read through its getter
(`export_lookup_capacity`, :439-441, used at projection.rs:121), so the field
and getter duplicate a constant. Separately, `trace_limit` carries
`#[allow(dead_code)]` (:443-446) even though it is called from production
paths `project_for_classification` (projection.rs:52) and the `cfg(test)`
`project` method (projection.rs:380), so the allowance is stale.

**Recommendation:** Drop the `export_lookup_capacity` field and getter
(model.rs:238, :258, :290, :439-441) and call `MAX_EXPORT_LOOKUP_ENTRIES`
where the lookup session is built (projection.rs:121), mirroring `into_linker`
(model.rs:196), which already passes the constant; remove the obsolete
`#[allow(dead_code)]` on `trace_limit`. Guardrails: keep `flow_limit`,
`effect_limit`, and `trace_limit` as stored configuration copies — they are
read at multiple phases (`outcome.rs`, `flow/cross/mod.rs`, `projection.rs`)
— and keep the test-only `single_with_limits` constructor in sync.

**Fix Applied:** None so far.

### Module-qualified ID newtypes (`analysis/project/model.rs`, `state.rs`)

#### [ ] READ-008 — The three module-qualified ID newtypes expose inconsistent accessor surfaces

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `model.rs:53-70` (`QualifiedFunctionId`), `state.rs:216-237` (`QualifiedExportId`), `model.rs:46-76` (`QualifiedRequestId`)

`QualifiedFunctionId` exposes `module()`/`function()` (model.rs:63-69) and
`QualifiedExportId` exposes `module()`/`name()` (state.rs:230-236), but
`QualifiedRequestId` — a sibling newtype over `(ModuleId, ModuleRequestId)` —
exposes no accessors at all, only `new`. Its parts are never read, so this is
not a correctness issue, but the surface is inconsistent across three types
that the codebase presents as one family (same placement in the chunk,
identical shape, `ModuleId` + a module-local ID).

**Recommendation:** Document the key-only contract on `QualifiedRequestId`:
its fields are never decomposed anywhere, and it exists as a public map-key
token for the resolutions table (lookups pass the whole key to `resolution_for`
at model.rs:320-325, and `request_target` at resolver.rs:42-51 builds the key
from separate `module`/`request` values). Adding `module()`/`request()`
accessors purely for family symmetry would create unused public API, which the
workspace conventions discourage; if symmetry is still wanted, add read-only
accessors without touching the fields. Guardrails: keep the fields private; do
not expose `ModuleRequestId` beyond `pub(in crate::analysis)`.

**Fix Applied:** None so far.

### Star-export ambiguity policy (`analysis/project/identities.rs`, `resolver.rs`, `matching/identity_map.rs`)

#### [ ] READ-009 — The "star exports that disagree → Ambiguous" policy is implemented twice with different mechanisms

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `identities.rs:189-217` + `matching/identity_map.rs:28-41`, and `resolver.rs:235-247`

The overlay walker collects star contributions through
`ModuleIdentityContributions::add_star` (identity_map.rs:28-41, used at
identities.rs:192-206), which marks differing star-sourced identities
`Ambiguous` and lets direct exports win (`add_direct`/`finish_into`,
identity_map.rs:62-73). The single-export resolver `walk_star_exports`
re-encodes the same rule inline (resolver.rs:235-247): a second differing
`candidate` returns `Ambiguous`, matching values are kept. Both share the
"multiple star paths disagree ⇒ Ambiguous; direct/named wins" core, in
different modules and data shapes, but they handle unresolved paths
differently — the overlay leaves absent entries unset (or inserts `Unknown`),
while the resolver folds any unresolved star path into an unknown result — so
a change to the disagreement policy must still be applied in two places.

**Recommendation:** Make the disagreement policy a single source of truth:
either a documented constant-policy comment referenced by both paths, or — if
shared code is wanted — extract only the compare-and-mark-ambiguous core as
one small helper that merges a new definite value into an existing candidate
and returns `Ambiguous` on disagreement, owned in one place (e.g. `matching`).
Guardrails: do not merge the two traversals — they produce different outputs
(an overlay map vs a single `ExportResolution` with `None`-means-unknown
semantics) in different phases — preserve each path's distinct
unresolved-handling (overlay: absent entries unset or `Unknown` inserted;
resolver: unresolved star path ⇒ unknown result), and keep the
direct-wins-over-star precedence and the cycle bounds.

**Fix Applied:** None so far.

## Systemic Themes

- **One canonical set of BTreeMaps re-assembled per phase.** `ProjectLinker`
  (linker/mod.rs:46-57), `ProjectLookupView` (resolver.rs:20-23), and
  `ExportResolver::from_maps` (resolver.rs:80-91) all rebuild borrowed views
  over the same `modules`/`resolutions`/`exports` maps; the assembly is
  repeated at linker/mod.rs:60-70 and model.rs:327-341. This is a deliberate
  borrow-lifetime split, but any new lookup needs to remember to wire all four
  pieces again.
- **Phase separation is otherwise disciplined.** `ModuleGraph` →
  `NormalizedModuleGraph` (mutable construction vs sealed query, state.rs:13-155),
  `ModuleExport` → `ExportResolution` → matcher overlay, and
  `ProjectionOutcome`/`ProjectionStatus`/`ProjectionMetrics` (outcome.rs) are
  clean lifecycle boundaries with single owners; these should not be collapsed.
- **Fail-closed style is consistent.** Rejected SCCs skip export resolution,
  budget exhaustion is recorded (never panicked on), unknown/ambiguous
  resolutions stay distinct, and unsupported module interfaces surface as
  `IncompleteReason` status rather than empty success.

## Open Questions

- Resolved: the redundant memo re-check in `lookup_export_body`
  (resolver.rs:189-191) is dead. Nothing in `walk_star_exports` writes to
  `self.exports` — recursion only reads the export table and writes the
  separate `ExportLookupCache` (`self.cache.insert`, resolver.rs:193) — so no
  current path can make the post-walk check hit. READ-002's assumption holds;
  keep the guardrail note in case memoization into `ExportTable` is ever added.
- Resolved: dropping `LinkingSession::lookup_cache` at `ProjectLinker::finish`
  is intentional per-phase isolation — each pass constructs its own cache and
  `ProjectSemanticModel` owns no lookup-cache state (READ-004's one-cache-per-
  pass rule). The stated rationale is only half right, though: the fully
  resolved `ExportTable` covers direct/named exports only, while star-exported
  names are still walked per lookup, so a warm cache would be reusable by
  projection. The drop is deliberate; the "redundant" premise is not.
- Resolved: `QualifiedRequestId` is intentionally key-only. Its fields are
  never decomposed anywhere; every use passes the whole value as a map key
  (`resolution_for`, `request_target`, `finish`), and the type is `pub` purely
  as a public key token. READ-008's flag is surface-only, as stated.
- Resolved: `SccPartitionState::Pending` is never observable as a meaningful
  state. It is only the initial value and the `mem::replace`/`take` placeholder
  during `resolve_export_table`; after `collect_graph_edges` the field is
  `Ready` or `Rejected`, and `is_ready()` matches only `Ready`. READ-005's
  assumption holds; `Option<SccPartition>` covers both the transient holder and
  the rejected case.
- Resolved: the bare literal `"*"` at identities.rs:235 is an oversight, not a
  separate constant — `NAMESPACE_EXPORT` (model/module.rs:14) is defined as
  exactly `"*"`. Replace the literal with the constant.

## Coverage

- `glass-lint-core/src/analysis/project/mod.rs`
- `glass-lint-core/src/analysis/project/identities.rs`
- `glass-lint-core/src/analysis/project/linker/mod.rs`
- `glass-lint-core/src/analysis/project/linker/export.rs`
- `glass-lint-core/src/analysis/project/linker/graph.rs` (+ `graph/tests.rs`)
- `glass-lint-core/src/analysis/project/model.rs` (+ `model/tests.rs`)
- `glass-lint-core/src/analysis/project/projection.rs` (+ `projection/tests.rs`)
- `glass-lint-core/src/analysis/project/projection/outcome.rs`
- `glass-lint-core/src/analysis/project/resolver.rs`
- `glass-lint-core/src/analysis/project/state.rs` (+ `state/tests.rs`)
- Representative callers: `glass-lint-core/src/analysis/mod.rs`,
  `glass-lint-core/src/analysis/matching/identity_map.rs`,
  `glass-lint-core/src/analysis/flow/cross/{mod,graph}.rs`,
  `glass-lint-core/src/analysis/flow/projector/mod.rs`,
  `glass-lint-core/src/lint/report/mod.rs`,
  `glass-lint-core/src/project/session/artifacts.rs`,
  `glass-lint-core/src/project/types/input/resolution.rs`
