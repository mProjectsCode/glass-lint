# Codebase Readability Audit — Chunk 4

## Summary

Chunk 4 covers retained semantic models, module interfaces and request
recognition, project linking and export resolution, matcher projection
planning, and position-sensitive expression resolution. The ownership split is
generally sound: local value/fact arenas stay local, project linking stores
qualified overlays, and unknown or exhausted states remain explicit. The main
readability risks are smaller state protocols whose invariants are still
encoded by neighboring callers: bounded provenance union checks capacity before
deduplication, a method named `set_monotone` permits arbitrary replacement,
export lookup cache ownership is moved around to satisfy borrows, and linked
target conversion is repeated across resolution paths.

The highest-value improvements are to give bounded unions and export tables
real invariant-owning operations, keep lookup cache state inside one resolver
operation, centralize linked-target identity conversion, and make projection
planning and expression post-processing single-pass/shared operations. These
changes should reduce coordination and semantic drift without merging lexical
arenas or weakening fail-closed behavior.

No source, test, configuration, dependency, or documentation changes were made
by this audit.

## Findings

### Retained semantic model invariants

#### [x] READ-021 — Deduplicate provenance alternatives before applying the bound

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API / Bounded state / correctness
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:423-436`

`ProvenanceAlternatives::add_bounded` checks
`self.provenances.len() >= limit` before checking whether the incoming
provenance is already present. When the retained set is exactly at capacity, a
duplicate alternative marks the accumulator both `exhausted` and `unknown`
even though it would not increase the set. Those flags feed the strict witness
contract, so repeated evidence from a join can be treated as an incomplete
alternative rather than as an unchanged complete witness.

The deduplication and capacity policy belongs to this bounded union type, but
the current loop exposes their ordering and makes the semantic result depend
on whether a duplicate happens to arrive after the bound is reached. Move the
membership check ahead of the capacity check, preferably behind one private
bounded-insert operation, and remove any caller-side workarounds. Add focused
coverage for a duplicate at capacity and a distinct value at capacity.
Preserve joined flags, unknown/exhausted propagation from the other operand,
complete-witness retention, deterministic insertion order, and the rule that a
new distinct alternative beyond the bound remains non-definite.

**Fix Applied:** `ProvenanceAlternatives::add_bounded` now delegates each
candidate to an owned `insert_bounded` transition that checks membership before
capacity. A duplicate at capacity therefore preserves complete-witness state,
while a distinct candidate still marks the union exhausted and unknown. Added
regression coverage for the duplicate-at-capacity case; the existing overflow
test covers a distinct candidate at capacity.

**Verification:** `cargo test -p glass-lint-core --lib analysis::model::scope`
and `make fmt && make ci` pass.

#### [x] READ-022 — Make `ExportTable`’s monotonic-update contract real

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture / fixed-point state
- **Location:** `glass-lint-core/src/analysis/project/state.rs:219-242`,
  `analysis/project/linker/export.rs:72-134`

`ExportTable::set_monotone` advertises a monotonic fixed-point update, but its
implementation only suppresses an equal value and otherwise overwrites the
entry with any `ExportResolution`. The linker calls it while resolving normal
SCC rounds and again to replace unresolved cycle entries with `Unknown`.
There is no resolution-order check or typed transition result that explains
which replacements are legal. As a result, the name suggests an invariant that
the table does not own; future callers can accidentally regress a resolved
identity, overwrite `Ambiguous`, or count a non-monotone transition as ordinary
progress.

Either model the allowed resolution lattice/transition operation in
`ExportTable`, returning a typed “unchanged/advanced/exhausted” result, or
rename the operation to reflect intentional replacement and keep the fixed
point policy in a dedicated linker method. Delete the misleading contract from
callers after migration. Preserve cycle convergence and final `Unknown`
fallback, entry-count budgeting, deterministic SCC order, ambiguity handling,
and the distinction between a missing entry and an explicitly unknown entry.

**Fix Applied:** Renamed `ExportTable::set_monotone` to
`set_resolution` and documented its actual replacement contract. The table
continues to own equal-value suppression and entry accounting, while SCC
provisional updates and terminal cycle fallback remain explicit linker policy.
Added coverage for replacement, no-op equality, and stable entry counts.

**Verification:** `cargo test -p glass-lint-core --lib analysis::project::state`
and `make fmt && make ci` pass.

### Project linking and export lookup

#### [ ] READ-023 — Encapsulate export lookup cache ownership

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Ownership / state protocol
- **Location:** `glass-lint-core/src/analysis/project/state.rs:267-317`,
  `analysis/project/linker/mod.rs:65-73`,
  `analysis/project/resolver.rs:28-139`

`ProjectLinker::with_export_resolver` removes the cache from
`LinkingSession`, replaces it with a zero-capacity cache, constructs an
`ExportResolver` with the detached value, then restores it after the closure.
The same take/restore protocol is also exposed as two session methods. This is
borrow-workaround plumbing rather than a domain operation: every resolver call
must know that the cache temporarily lives outside its owning session, and a
new exit path or nested operation can violate the ownership protocol.

`ExportLookupCache::get` also exposes the storage-shaped
`Option<&Option<ExportResolution>>` needed to distinguish absent from cached
unresolved, rather than exposing that distinction as a named lookup result.
The cache’s bounded first-insert behavior and cached `None` are legitimate
semantics, but they should be owned by the lookup operation instead of leaked
through `LinkingSession`.

Give `ExportResolver` or one linker lookup operation ownership of the bounded
cache for the duration of a lookup, and replace the nested optional API with a
named hit/miss result. Delete `take_lookup_cache` and `restore_lookup_cache`
and the closure-level detach/restore once callers use that owner. Preserve
cache capacity, cached unresolved results, recursive export lookup, stale-value
avoidance when the export table has a direct result, and the bounded depth and
diagnostic behavior.

**Fix Applied:** None so far.

#### [ ] READ-024 — Centralize linked-target-to-export identity conversion

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Architecture / uncertainty policy
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:206-235`,
  `analysis/project/linker/export.rs:275-301, 323-356`,
  `analysis/project/identities.rs:206-219`

The mapping from `LinkedModuleTarget` to `ExportResolution` is repeated in
namespace re-exports, named re-exports, star-export walking, imported identity
resolution, and namespace identity projection. The existing
`target_to_export_resolution` helper handles one form of the mapping, but the
linker has separate matches for internal, external, builtin, missing, and
unsupported targets. These branches differ subtly in the namespace export name
(`*`), recursive internal lookup, and treatment of absent targets.

The target categories and fail-closed policy are shared domain logic, while
the current repetition leaves package spelling, builtin treatment, and
missing/unsupported behavior open to drift between linking and projection.
Give `LinkedModuleTarget` or a private project identity converter the common
non-recursive conversion, with an explicit operation for recursive internal
export lookup and namespace export naming. Delete the repeated external,
builtin, and unknown matches after migration. Preserve internal re-export
recursion, `*` namespace identities, external package names, outside-project
and unsupported unknowns, and ambiguity propagation.

**Fix Applied:** None so far.

### Projection and resolution orchestration

#### [ ] READ-025 — Build `ProjectionPlan` requirements in one selection pass

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity / Duplication / API
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:107-168`

`ProjectionPlan::from_selection` traverses `selected_matchers()` three times:
once for constrained roots, once for overlay and flow requirement booleans,
and once for lifecycle roots. The same matcher and physical-root structure is
therefore interpreted by three independent loops, while six mutable booleans
encode a single aggregate requirement state. A new physical root or matcher
requirement must be added to the correct traversal or the plan can silently
omit one consumer.

Make `ProjectionPlan` own a single accumulator pass that collects constrained
roots, lifecycle roots, and a typed aggregate of overlay/module/result/flow
requirements. Keep the current public query methods as projections of that
owned plan and delete the repeated selection traversals. Preserve selected rule
order, rule/root indexes, omission of empty constrained groups, flow lifecycle
requirements, and the rule that project identity overlays are built only when a
selected matcher needs them.

**Fix Applied:** None so far.

#### [ ] READ-026 — Separate project projection from classification result assembly

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture / Ownership
- **Location:** `glass-lint-core/src/analysis/project/model.rs:498-538`,
  `analysis/project/projection.rs:408-445`

`ProjectSemanticModel::classify_with_evidence_limit` owns two unrelated
lifecycles: it temporarily replaces `self.trace_arena` so projection can
borrow the model, and it converts the projected matcher catalog into
report-facing `ClassificationResult` values by iterating selected rules and
copying descriptions, severities, and evidence. The method therefore couples
trace storage ownership, matcher projection, rule selection validation, and
report assembly. Its caller cannot reuse the projection outcome or catalog
without also entering this trace-swapping/report-building protocol.

Introduce a private project-matching runner or classification boundary that
owns the trace arena for one run and returns a projection result/catalog to a
separate report assembler. Delete the temporary arena swap and report loop
from `ProjectSemanticModel` after migration. Preserve trace limits and stable
trace ownership, invalid selected-rule skipping, evidence limits, empty-result
omission, deterministic module/rule ordering, and propagation of projection
exhaustion into project status.

**Fix Applied:** None so far.

#### [ ] READ-027 — Share identifier/member resolution post-processing

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Complexity / uncertainty policy
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:52-110,
  147-211`

`Resolver::resolve_ident` and `Resolver::resolve_member` each implement the
same cache lookup, recursion guard, value interning, budget-to-unknown
conversion, global canonicalization, and cache insertion sequence. Their
seed-specific work is legitimately different, but the duplicated post-
processing is the policy boundary that turns a value into `ResolvedValue`.
The member path already has extra namespace and returned-member handling, so a
future change to unknown reasons, global re-interning, or cache cleanup can be
applied to one resolver path and not the other.

Introduce a private resolver helper around the shared “seed/value to resolved
value” lifecycle, with explicit hooks for member-only namespace and returned
member data. Delete the duplicated cache/guard/canonicalization code after
migration. Preserve position-sensitive cache keys, cycle-to-unknown behavior,
binding identity, global and module provenance, member rooted/syntactic paths,
value-arena exhaustion, and the narrow ID-query fast paths.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 4’s strongest risks occur where a bounded or linked state object exposes
just enough storage to let neighboring code assemble the protocol: provenance
union flags, export-table replacement, detached lookup caches, target matches,
projection booleans, and trace/report lifecycles. The local/project boundary
itself is a good architectural constraint; the next refactors should make the
existing owners stronger rather than introduce another semantic model or merge
lexical and project identities.

There is also a recurring policy duplication pattern. Resolver branches repeat
the same post-processing, while export-linking branches repeat target-category
conversion. Consolidating those policies should retain the explicit unknown,
ambiguous, budget-exhausted, and cycle states rather than replacing them with
ordinary `Option` or last-write behavior.

Search signals used for this chunk included methods whose names promise
monotonicity or ownership but expose replacement/storage mechanics, nested
optional cache values, repeated linked-target matches, repeated resolver cache
post-processing, multiple selection traversals, and temporary arena swaps.

## Open Questions

- The export resolution values do not currently expose an ordering. Before
  enforcing `set_monotone`, define whether SCC iteration is intended to be a
  true lattice fixed point or an explicitly replacing bounded approximation;
  the API name and the cycle fallback must agree.
- The lookup cache must continue distinguishing “not cached” from “cached
  unresolved”; a named result should make that state visible without exposing
  nested `Option` storage.
- The next unreviewed handoff is Chunk 5: analysis API, semantic budgets, and
  project/public integration modules.

## Coverage

Reviewed every source file listed for Chunk 4 in `CODEBASE_STRUCTURE_CORE.md`:
the retained `analysis/model` value, scope, static-property, flow, module, and
fact models; `analysis/module_request.rs`; all listed `analysis/project`
model, state, identities, linker, resolver, and projection modules; and the
listed `analysis/resolution` module, call, constant, expression, and test
modules. Representative callers in facts, scope construction, flow
projection, matching, and project session code were traced to verify ownership
and phase contracts. Existing Chunk 1–3 findings were checked to avoid
re-reporting cache/lowering, occurrence-overlay, matcher-context, identity
precedence, exhaustion, and evidence-normalization findings. No findings are
marked applied.
