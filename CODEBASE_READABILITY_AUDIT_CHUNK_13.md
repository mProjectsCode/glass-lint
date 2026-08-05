# Codebase Readability Audit — Chunk 13

## Summary

Chunk 13 covers project linking, qualified export identities, projection
models, and position-sensitive value resolution. The project boundary keeps
module-local arenas isolated, uses deterministic module graphs and SCC order,
and centralizes export lookup across linker and post-link projection.

The new findings are boundary-invariant issues rather than another pass over
the already-audited export cache, target conversion, projection planning,
classification lifecycle, or resolver post-processing. A public projection
method accepts a module that is not proven to belong to its projection model;
export-depth exhaustion drops namespace identities without an explicit
unknown/masked result; and several project lookup APIs carry artifact-local
IDs beside a separate module ID instead of one qualified identity.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Projection ownership

#### [x] READ-066 — Bind projected evidence to the owning project module

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Artifact identity / Cross-project safety
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:242-269,537-558`

`ProjectMatcherModel::evidence_for` receives an arbitrary `&ProjectModule`,
looks up projection state only by `module.id()`, and then reads the name table
from the caller-supplied module. `ModuleId` is assigned per project, so two
different projects can contain the same numeric ID; their local `NameId`,
fact, occurrence, and overlay arenas are not interchangeable. Passing a
module from another project can therefore combine one model's projected
occurrences with another artifact's names, or accidentally return evidence
for a coincident module ID without any ownership error.

Make the projection model own the module handle it serves, accept a
project-qualified module identity, or expose evidence lookup through
`ProjectSemanticModel` so the module, index, overlay, and `NameTable` are
selected from one owner. Preserve selected-rule validation, evidence limits,
normalization, and deterministic ordering while deleting the foreign-module
possibility; add a regression test with two project models that reuse module
IDs.

**Fix Applied:** Retained the producing `ProjectModule` in each projection,
used its own name table during evidence lookup, and rejected modules that are
not the projection owner even when their numeric `ModuleId` coincides. Added a
regression covering two single-module projects that both reuse module ID 0.
Verified with `cargo test -p glass-lint-core --lib analysis::tests` and
`make fmt && make ci`.

### Export-depth exhaustion

#### [x] READ-067 — Represent namespace export-depth exhaustion as unknown

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Bounded state / Completeness / Identity masking
- **Location:** `glass-lint-core/src/analysis/project/identities.rs:168-212`,
  `analysis/project/model.rs:37`,
  `analysis/matching/identity_map.rs:143-157`

`ProjectSemanticModel::collect_exported_identities` stops recursion when
`visiting.len() >= MAX_EXPORT_DEPTH` or a module cycle is encountered, but it
returns without inserting an `Unknown` export identity or a wildcard for the
namespace prefix. `module_identities` then exposes no entry for the unresolved
namespace member. `LinkedOccurrenceView::identity_for` treats a missing map
entry as “no overlay,” so the original module occurrence is not masked even
though the qualified identity was not established. In a deep or cyclic
namespace export chain, matching can consequently retain authored/local
identity data instead of recording an explicit incomplete alternative.

Make the bounded traversal return an exhaustion result owned by the identity
collector, and insert an unknown wildcard or otherwise mask the affected
namespace occurrences when the depth/cycle bound is reached. Preserve direct
exports as authoritative, star-vs-star ambiguity, deterministic traversal,
and the rule that unknown or exhausted qualified identity cannot establish a
strict witness; add a deep-cycle regression that verifies no definite match
survives the cutoff.

**Fix Applied:** Marked namespace prefixes with an `Unknown` wildcard when
star-export traversal reaches the depth bound or revisits a module. The
existing overlay then masks unresolved member occurrences while exact direct
exports remain authoritative. Added overlay-mask and 1,024-hop deep-chain
regressions. Verified with `cargo test -p glass-lint-core --lib
analysis::matching::tests::unknown_namespace_wildcard_masks_base_module_occurrences`,
`cargo test -p glass-lint-core --lib
project::tests::session_and_link_validation::deep_namespace_export_chain_masks_unresolved_members`,
and `make fmt && make ci`.

### Qualified local identities

#### [ ] READ-068 — Bind artifact-local IDs to their module owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Semantic newtype / Identity ownership
- **Location:** `glass-lint-core/src/analysis/project/model.rs:354-405`,
  `analysis/project/model.rs:407-434`,
  `analysis/flow/cross/graph.rs:60-75`

Project APIs repeatedly accept a `ModuleId` beside an artifact-local identity:
`effect(module, FunctionId)`, `module_fact_stream(module)`, `fact(module,
FactId)`, `source_call_result(module, FactId)`, and `fact_location(module,
FactId)` all rely on callers to preserve the pairing. Cross-flow graph lookup
uses the same `(module, event)` shape. The raw IDs are intentionally local to
their module, but the type system does not prevent a `FactId` or `FunctionId`
from another module being supplied; the current fallback is often `None` or
`ValueId::UNKNOWN`, which hides an ownership mistake as an unresolved
semantic result.

Introduce small qualified handles such as `QualifiedFactId` and
`QualifiedFunctionId`, or module-owned lookup handles that make the local
arena explicit at construction. Migrate the project, cross-flow, and report
callers together; preserve `Option` behavior for genuinely missing facts,
invalid functions, and unknown values, while making cross-module identity
mixups unrepresentable or explicitly rejected.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 13’s project model has a strong qualified-export boundary, but that
boundary is not applied uniformly to every public projection and lookup API.
Project-local IDs need an owner at API boundaries, and bounded traversal needs
to produce an explicit unknown/masked state rather than relying on absence.
The matcher projection should also retain one project/module ownership path
through evidence assembly so local name tables and occurrence indexes cannot
be mixed.

The earlier Chunk 3–4 findings remain intentionally separate: cache detach/
restore, export-table monotonicity, linked-target conversion, projection-plan
passes, classification/trace lifecycle, resolver post-processing, and final
evidence normalization were not repeated here. No findings are marked applied.

## Open Questions

- Whether qualified fact/function handles should be public domain types or
  remain private wrappers depends on how much project-level flow API is meant
  to be reused outside analysis internals.
- Namespace-depth exhaustion should use the same `Unknown` overlay behavior
  as missing/ambiguous exports, or a distinct status if reports need to
  distinguish cycle/depth truncation.
- The next unreviewed handoff is Chunk 14: scope, syntax, and trace types.

## Coverage

Reviewed the Chunk 13 types listed in `CODEBASE_STRUCTURE_CORE.md` across
project input validation, linked project state, SCC graph construction,
qualified export lookup, module identity projection, projection outcomes and
metrics, and resolver caches/results. Representative callers were traced
through project sessions, linking, namespace identity overlays, cross-flow
lookup, matcher evidence, report assembly, and expression resolution.
Existing Chunk 3–4 findings were checked to avoid re-reporting export-cache
ownership, export-table monotonicity, linked-target conversion,
`ProjectionPlan` traversal, classification lifecycle, resolver post-processing,
and evidence normalization. No source, test, configuration, dependency, or
documentation changes were made.
