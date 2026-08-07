# Codebase Readability Audit — Chunk 7

This audit covers Chunk 7 of `CODEBASE_STRUCTURE_CORE.md`: project linking.
It is an architectural review only; no source changes were made.

## Summary

The project layer preserves the important ownership boundaries: resolved
module targets enter Core through `ResolvedLinkInput`, lexical arenas remain
module-local, graph linking is deterministic, and ambiguous or incomplete
export information remains conservative. The main readability risks are at
the phase boundaries. The transient linker and final model duplicate a large
state shape, graph/export linking combines several mutable responsibilities,
recursive export lookup carries cache and uncertainty state through branchy
control flow, projection coordinates too many lifecycles in one session, and
two internal APIs make important ownership or merge policies implicit.

## Findings

### READ-001 — Transient and final linked state duplicate the same project payload

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Architecture
- **Location:** `glass-lint-core/src/analysis/project/model.rs:227-269`; linker transfer at `glass-lint-core/src/analysis/project/linker/mod.rs:126-146`
- **Representative callers:** `ProjectLinker::finish` constructs `LinkedProjectState`, then `ProjectSemanticModel::from_linker` copies every linked field into the final model

`LinkedProjectState` and `ProjectSemanticModel` contain the same seven linked
fields: modules, resolutions, exports, edge count, cycle rounds, diagnostics,
and status. The intermediate type is a useful consuming boundary, but its
storage shape is duplicated rather than represented as one owned aggregate.
Every new project-linking field must currently be added to both structs, the
linker transfer, and the model constructor. A field that is copied in one
place but omitted in another would produce a valid-looking but incomplete
semantic model.

The distinction between transient and final state should remain: the linker
must be consumed, and analysis limits belong to the final model. The issue is
that the lifecycle distinction is encoded by two parallel field lists instead
of by one owned data object.

**Recommendation:** Introduce a private linked-data aggregate owned by the
project model boundary, or make the final model contain `LinkedProjectState`
alongside its projection limits. Keep `ProjectLinker::finish` as a consuming
transition and keep storage private; the change should centralize field
ownership without exposing linker internals. Preserve deterministic maps and
diagnostics, fixed-point export state, status propagation, and the final
model’s independent flow/effect/trace limits.

**Fix Applied:** None so far.

### READ-002 — `ProjectLinker` combines graph construction, export resolution, and reporting state

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/analysis/project/linker/mod.rs:31-147`; graph phase at `glass-lint-core/src/analysis/project/linker/graph.rs:15-84`; export validation at `glass-lint-core/src/analysis/project/linker/export.rs:142-194`
- **Representative callers:** `ProjectSemanticModel::link_with_limits` drives `propagate_local_status`, `build_graph_and_exports`, and `finish`; `build_graph_and_exports` then mutates the same linker through three phases

`ProjectLinker` owns modules and resolutions, the normalized graph and SCC
partition, the export table and lookup cache, the link budget, diagnostics,
and analysis status. Its public phase coordinator is short, but the phase
helpers use that broad mutable state for several different jobs. In
particular, `collect_graph_edges` both builds edges and records missing,
outside-project, unsupported, and budget outcomes; export resolution then
uses the same owner for fixed-point updates; and
`validate_imported_exports` first materializes a separate check list before
performing lookups and emitting diagnostics.

This makes the project-linking protocol depend on field-level conventions:
graph construction must happen before SCC export resolution, the export table
must be populated before imported-export validation, and all phases must
share the same budget/status semantics. A new diagnostic or budget path has
several plausible owners and can accidentally couple graph data to reporting
state.

**Recommendation:** Keep `ProjectLinker` as the top-level consuming
coordinator, but give graph construction and export resolution private phase
owners or result values. A graph phase can return the normalized graph, SCC
partition, and graph-local status/diagnostics; an export phase can own its
fixed-point table and lookup session; the linker can merge bounded outcomes
and perform final diagnostic ordering. Preserve the current phase ordering,
SCC cycle bound, link budget, missing-resolution diagnostics, fail-closed
status propagation, and deterministic sorting/deduplication.

**Fix Applied:** None so far.

### READ-003 — Recursive export lookup spreads cache, cycle, and uncertainty state across early returns

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / State Protocol
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:135-228`
- **Representative callers:** `ExportResolver::resolve_imported_identity`, `ProjectLinker::validate_imported_exports`, and linker export resolution call `lookup_export`; `lookup_export_inner` recursively calls itself through star exports

`lookup_export_inner` simultaneously manages authoritative fixed-point exports,
the bounded cache, the active `visiting` set, default-export special cases,
unknown module interfaces, recursive star traversal, ambiguity, and cache
publication. The active-set lifecycle is distributed across several early
returns and one normal-path removal: depth/cycle exits, default exports,
missing modules, and unknown interfaces do not all pass through the same
cleanup and publication sequence. `walk_star_exports` adds a second protocol
for collecting a candidate while separately remembering whether an unknown
branch was seen.

The conservative semantics are intentional, but the representation makes the
state machine difficult to audit. A future change to cache an unknown result,
add another export form, or alter cycle handling must reason about which
branches remove the active key, which results are cacheable, and whether a
partial candidate is allowed to survive an unknown sibling. Those rules are
currently encoded by return shape (`Option`, `saw_unknown`, and
`Ambiguous`) rather than by a named lookup outcome.

**Recommendation:** Encapsulate one lookup operation in a private context
that owns active-path entry/exit and separates authoritative hits, cached
answers, bounded/cyclic uncertainty, ambiguity, and unresolved results. A
typed lookup outcome can still be lowered to the existing `Option` and
`ExportResolution` behavior at the boundary. Preserve direct exports taking
precedence over the cache, default-export behavior, the depth bound,
star-export ambiguity, unknown masking, and the rule that incomplete branches
cannot establish a complete export identity.

**Fix Applied:** None so far.

### READ-004 — Project projection coordinates local, cross-file, evidence, and outcome phases in one session

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:476-580`
- **Representative callers:** `project_for_classification` and `ProjectSemanticModel::classify_with_evidence_limit` enter `project_with_arena`; it delegates to `project_modules`, cross-file collection, outcome finalization, and evidence merging

`project_with_arena` creates the matcher plan, derives flow limits, creates a
linking session, decides whether cross-flow is needed, projects every module,
collects cross-file flow, merges cross evidence, finalizes outcome status, and
constructs the matcher model. `project_modules` adds another set of phase
decisions: whether to build module identities, call-result identities,
overlays, effects, and local flow, followed by matcher-context construction,
operation accounting, local evidence projection, and artifact assembly.

The function boundaries do not make the lifecycle explicit. A caller must
understand that the same `TraceArena` and `LinkingSession` span local and
cross-file work, that cross evidence is merged only after all module
projections exist, and that `ProjectionOutcome::finish` derives the observed
flow count from the accumulated operations. The plan’s independent boolean
requirements are useful for laziness, but they also make the projection
session’s valid states positional and flag-driven.

**Recommendation:** Introduce a private projection-run owner with explicit
phases such as plan preparation, local module projection, optional cross-file
projection, evidence merge, and outcome finalization. Keep resource needs in a
named capability/requirements value rather than passing parallel booleans
through the session. Preserve one-pass fact consumption, shared arena and
lookup-cache lifetimes, lazy identity/effect construction, evidence capacity
checks, deterministic module ordering, and the existing budget outcome
semantics.

**Fix Applied:** None so far.

### READ-005 — Foreign or invalid module handles silently look like empty evidence

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Error Semantics
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:301-312,583-617`
- **Representative callers:** `ProjectMatcherModel::modules` creates owner-tagged handles; `assemble_classification_results` passes them back to `evidence_for`

`ProjectModuleHandle` carries a borrowed module reference plus a private global
`ProjectMatcherIdentity`. `ProjectMatcherModel::evidence_for` checks the owner
identity, selected rule, matcher lookup, and module lookup, returning an empty
vector for every failed check. The owner check correctly prevents a handle
from one projection model from exposing evidence from another, but the API
collapses an invalid foreign handle, an unknown module, an unselected rule,
and a valid module with no evidence into the same result.

That fail-closed behavior is safe for classification, yet it makes misuse
hard to detect at internal call sites and makes diagnostics or future tooling
unable to distinguish “no match” from “wrong model.” The global atomic token
also expresses ownership indirectly instead of making the query operation
model-owned in its type or return value.

**Recommendation:** Keep the ownership guard, but make invalid-handle cases
explicit through a model-owned query type or a small `Result`/error outcome;
reserve an empty evidence vector for a valid query with no evidence. If the
classification API must remain infallible, add a private checked method and
have the public adapter intentionally lower invalid queries to empty. Preserve
borrowed handles, cross-model isolation, selected-rule filtering, evidence
normalization, and deterministic output.

**Fix Applied:** None so far.

### READ-006 — Identity-map merge semantics are encoded by caller-selected methods

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Domain Invariant
- **Location:** `glass-lint-core/src/analysis/matching/identity_map.rs:19-48`; sequencing at `glass-lint-core/src/analysis/project/identities.rs:194-221`
- **Representative callers:** `collect_exported_identities` accumulates star entries with `merge_star_from`, then copies direct exports and calls `merge_missing_from`

`ModuleIdentityMap` exposes general insertion plus two consuming merge
operations. `merge_star_from` marks disagreements as `Ambiguous`, while
`merge_missing_from` preserves existing entries. The project identity walker
depends on a precise sequence: star-derived maps are merged with conflict
detection, direct exports are copied as authoritative entries, and the star
map is then merged only into missing keys.

The method names explain the immediate operation, but the map type does not
encode whether entries are direct or star-derived, nor does it prevent a
future caller from applying the methods in the wrong order. That would turn
the export precedence and ambiguity invariant into a silent API-level
mistake. The distinction is especially important because an ambiguous or
unknown identity must mask a possible witness rather than be overwritten by
later traversal order.

**Recommendation:** Move the precedence protocol behind a named identity
collector or introduce an explicit merge policy owned by the map, with direct
and star contributions represented as distinct inputs. Keep raw insertion
private to the collector where possible. Preserve star-vs-star ambiguity,
direct-export authority, unknown wildcard masking, bounded recursive walks,
and deterministic key ordering.

**Fix Applied:** None so far.

## Systemic Themes

- **ENCAPSULATE:** Linked-state transfer, handle ownership, and identity
  precedence should be represented by domain owners rather than parallel
  fields, tokens, or caller-selected merge sequences.
- **SIMPLIFY:** Linker and projection sessions expose multi-phase protocols
  through broad mutable state, flags, and branch-sensitive recursive results.
- **DEDUPLICATE:** The transient/final project state shape is repeated across a
  lifecycle boundary even though the semantic payload is the same.

## Decisions and Coverage

Reviewed project input validation, qualified request/export identities,
module graphs and SCC ordering, bounded export tables and lookup caches,
linker phase orchestration, recursive direct/star export resolution, module
identity projection, project-level matcher projection, outcome aggregation,
and matcher-module handle validation. The underlying resolver adapters and
provider-specific project boundaries were not reported: they are outside this
Core project-linking chunk and should remain owned by `glass-lint-project` or
the provider crates.

The shared linked-data aggregate remains private to the Core project boundary,
beside the linker/project semantic model. It may combine link-owned module
graph, identity, and status data, but must not merge lexical arenas or move
module discovery/resolution policy into Core. This keeps the project linker as
the owner of cross-file identity while the project crate remains the owner of
host resolution.

## Handoff

Chunk 7 is complete. The next unreviewed chunk is **Chunk 8 — Rule authoring
and catalog integration** (`CODEBASE_STRUCTURE_CORE.md` lines 527-613),
covering rule compilation, matcher catalogs, rule selection, and rule-facing
Core APIs.
