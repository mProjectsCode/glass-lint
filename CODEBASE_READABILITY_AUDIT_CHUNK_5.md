# Codebase Readability Audit — Chunk 5

## Summary

Chunk 5 owns lexical scope planning and source-order collection, bounded
assignment history, control-flow joins, mutation indexes, and the frozen
scope-query API. The phase split is clear and the implementation preserves
important fail-closed behavior for invalid shapes, dynamic lookup, ambiguous
assignments, and exhausted alternatives. The main readability and API risks
are that path joins manipulate several raw checkpoint stores in the collector,
the traversal repeats balanced lifecycle code, the collector is a broad
mutable coordinator, freeze-time artifacts expose tuple-shaped storage, and
the build and query phases repeat expression-shape normalization.

## Findings

### Control-flow state ownership

#### [ ] READ-021 — Move path joins behind a path-state operation

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Ownership
- **Location:** `glass-lint-core/src/analysis/scope/build/assignments.rs:130-250`; callers in `:252-431`

`ScopeCollector::join_paths` coordinates the assignment mutation log, the
write-generation log, reachability, lexical fallback lookup, bounded
provenance union, and synthetic assignment creation. It repeatedly restores
the live `AssignmentEnvironment` and `WriteSet` to each checkpoint, manually
collects touched `ScopedName` values, then restores the incoming checkpoint
again before recording each joined value. The algorithm is correct but its
invariants are distributed across `CollectorCheckpoint`, `PathCollectionState`,
`AssignmentEnvironment`, and `WriteSet`; a change to checkpoint validity,
branch admission, or alternative handling must be coordinated from the
collector-facing method.

**Recommendation:** Give `PathCollectionState` (or a dedicated
`PathJoiner`) an operation that accepts the incoming checkpoint and branch
checkpoints, owns cursor transitions and touched-key collection, and returns
validated joined assignments or an explicit invalid-checkpoint outcome. Keep
lexical declaration fallback and assignment-record construction behind narrow
callbacks or domain methods owned by the scope state. Delete the raw restore/
iteration sequence from `ScopeCollector::join_paths` after migration. Preserve
reachable-path filtering, incoming values for paths that do not write, complete
witnesses alongside uncertainty, the `alternative_limit`, deterministic key
ordering, and fail-closed `InvalidCheckpoint` behavior.

### Phase-neutral traversal lifecycle

#### [x] READ-022 — Centralize balanced scope and loop traversal helpers

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Control flow
- **Location:** `glass-lint-core/src/analysis/scope/build/traversal.rs:158-212,215-220,321-361`

`ScopeTraversal` repeats the same lifecycle protocol across function
declarations, function expressions, arrows, blocks, and all three loop forms:
call `push_scope`, conditionally enter phase state, visit children only while
the budget permits, call the matching exit hook, and finally call
`pop_scope(entered)`. The function forms differ only in their phase-specific
pre-child hook and parameter/body visitor, while `for`, `for-in`, and `for-of`
duplicate the same scope and loop sequencing. Because the balancing protocol is
distributed across visitor methods, a future scope-forming syntax can easily
forget an exit hook, visit after exhaustion, or pop an unentered planned
scope.

**Recommendation:** Add private traversal helpers such as a scoped-body
wrapper and a loop-body wrapper that own the `entered` guard, budget gate,
paired enter/exit hooks, and final pop. Pass only the syntax-specific child
visitor and phase hook into those helpers. Keep `ScopePass` as the
phase-neutral semantic contract, preserve the planner/collector reuse, and
retain the current ordering for tests, parameters, decorators, loop headers,
`break`/`continue`, and exhaustion short-circuiting.

**Fix Applied:** Added shared scoped-body, function-body, and loop-body
helpers to own entered-scope balancing, budget gates, and paired lifecycle
hooks. Function, block, and loop visitors now supply only syntax-specific
hooks and child traversal.

### Collector responsibility boundary

#### [ ] READ-023 — Split the broad mutable scope collector state

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:176-228`; construction in `build/collector.rs:30-49`; mutations in `build/visitor.rs`, `build/assignments.rs`, `build/provenance.rs`, and `build/freeze.rs`

`ScopeCollector` is the `ScopePass` implementation and simultaneously owns the
predeclared lexical arena and stack, name interning and exhaustion, assignment
events and version counters, path-sensitive mutation/control-flow state,
function metadata and callback captures, call records, property artifacts, and
collection diagnostics. The visitor can therefore reach every subsystem while
handling a single declaration or call, and `freeze` must know how to extract
all of those unrelated stores. The current modules separate method bodies but
not the state boundary: provenance helpers, assignment joins, callback
collection, and artifact finalization all mutate the same coordinator.

**Recommendation:** Introduce narrow internal owners for (1) scope/name
lookup, (2) path and assignment state, (3) function/callback facts, and (4)
collection artifacts, with the collector retaining only traversal
coordination and explicit phase transitions. Have visitor hooks receive the
smallest owner needed, and make each owner provide its own finish/seal result
instead of exposing fields to `freeze`. Delete the direct cross-subsystem
field access after callers migrate. Preserve the planner-produced scope
identity, source-order assignment versions, callback parameter rules,
bounded budgets, deterministic artifact ordering, and conservative handling of
unknown or dynamically invalidated values.

### Freeze representation

#### [x] READ-024 — Replace tuple-shaped scope artifact extraction with a sealing API

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:85-141`; `build/freeze.rs:13-56`

`ScopeCollectionArtifacts::finish_into` creates
`FrozenScopeCollectionArtifacts`, whose `into_parts` returns a tuple of
issues, mutable-object names, and property artifacts. A second
`FrozenPropertyArtifacts::into_parts` returns three more vectors, and
`ScopeCollector::freeze` then manually combines those vectors with binding
ID allocation, parameter aliases, function maps, mutation-index construction,
and graph freezing. The tuple decomposition exposes storage instead of the
semantic transition from mutable collection records to a validated frozen
scope artifact, so adding a retained artifact requires synchronized changes in
multiple destructuring sites.

**Recommendation:** Give the artifact bundle a named sealing operation that
consumes collection records and produces a `ScopeGraphInput`-adjacent result
with named fields or performs property-index construction itself. Let the
binding and mutation owners expose validated constructors rather than requiring
`freeze` to rebuild their inputs from tuples. Delete the two `into_parts`
storage APIs after migration. Preserve issue accumulation, shape validity,
invalid binding-index fallback, dynamic-eval filtering, property assignment
ordering, and the immutable `FrozenScopeGraph` boundary.

**Fix Applied:** Replaced tuple-shaped artifact extraction with the named
`seal` transition and named field destructuring. Removed both `into_parts`
storage APIs while preserving property indexing, binding allocation, mutation
construction, issue accumulation, and graph-freeze ordering.

### Expression-shape normalization

#### [x] READ-025 — Share wrapper and member-chain normalization across scope phases

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Semantic conversion
- **Location:** `glass-lint-core/src/analysis/scope/build/provenance.rs:46-102,222-280`; `analysis/scope/query/provenance/object.rs:43-87,105-141`; `analysis/scope/query/provenance/chain.rs:37-106`

The mutable collector and frozen query graph each independently recurse through
`Ident`, `Member`, `Call`, `Paren`, and sequence expressions to recover a
semantic path. `module_alias_provenance` and `returned_object_provenance` in
the build phase duplicate wrapper and last-expression handling, while
`collect_module_member`, `returned_object_source`, and
`resolve_member_chain` repeat member-chain construction and suffix decisions in
the query phase. The phases legitimately use different lookup state, but the
syntax normalization and chain-shape policy are represented by several
parallel matches. A new supported wrapper or member form can therefore be
added to one path and silently omitted from another.

**Recommendation:** Add one scope-owned, provider-neutral expression-shape
normalizer that handles transparent wrappers, final sequence expressions,
literal member names, and callee/member chain structure. Have the collector
and frozen graph apply their own lexical/provenance/mutation policy to that
normalized shape, and delete the duplicated wrapper/member recursion after
migration. Preserve lexical shadowing, dynamic-lookup rejection, write-time
identity handling, module-request recognition, rooted-path mutation checks,
and the distinction between collector-time mutable state and frozen query
state.

**Fix Applied:** Added the scope-owned `ScopeExpression` normalizer for
parentheses, sequence tails, literal member names, calls, optional calls, and
await expressions. Collector and frozen-query provenance now consume the same
shape adapter while retaining their separate lexical, module-request,
rooted-path, and mutation policies; dynamic-import recognition and all
existing fail-closed behavior remain covered by the full gate.

## Systemic Themes

- Scope collection has a sound planner → source-order visitor → freeze
  lifecycle, but the central collector still owns too many independent state
  machines for that lifecycle to remain easy to extend.
- Checkpointed assignment history and bounded alternatives are valuable
  semantic invariants. Refactors must keep invalid checkpoints fail-closed,
  retain independent complete witnesses, and preserve deterministic joins.
- The frozen query graph correctly hides the mutable collection phase, but
  tuple extraction and repeated expression normalization weaken that boundary
  internally.

## Decisions

- Path joining returns a domain-level joined-state result; the collector
  remains responsible for source-order timestamps and assignment facts. This
  keeps path-state invariants together without making the joiner own fact
  emission.
- The shared expression normalizer covers only wrappers already accepted by
  both build and frozen-query semantics. Unsupported wrappers continue to
  return `None`; the refactor must not broaden matching by convenience.
- Traversal helpers are safe only when they consume the planner’s predeclared
  shapes in the existing order. Tests must pin `for` headers, catch
  parameters, decorators, and exhaustion short-circuiting before migration.

## Coverage

Reviewed all modules listed in Chunk 5 of `CODEBASE_STRUCTURE_CORE.md`:

- `analysis::scope`, `analysis::scope::binding_index`,
  `analysis::scope::build`, `analysis::scope::build::aliases`,
  `analysis::scope::build::analysis`,
  `analysis::scope::build::analysis::assignment`,
  `analysis::scope::build::analysis::classification`,
  `analysis::scope::build::assignments`,
  `analysis::scope::build::bindings`,
  `analysis::scope::build::callbacks`,
  `analysis::scope::build::collector`,
  `analysis::scope::build::compact_pat`,
  `analysis::scope::build::constants`,
  `analysis::scope::build::freeze`,
  `analysis::scope::build::history`,
  `analysis::scope::build::plan`,
  `analysis::scope::build::program`,
  `analysis::scope::build::projection`,
  `analysis::scope::build::provenance`,
  `analysis::scope::build::shape`,
  `analysis::scope::build::traversal`,
  `analysis::scope::build::visitor`,
  `analysis::scope::frozen_assignments`,
  `analysis::scope::graph`, `analysis::scope::mutation_index`,
  `analysis::scope::name_env`, `analysis::scope::query`,
  `analysis::scope::query::bindings`,
  `analysis::scope::query::constants`,
  `analysis::scope::query::functions`,
  `analysis::scope::query::provenance`,
  `analysis::scope::query::provenance::callable`,
  `analysis::scope::query::provenance::chain`,
  `analysis::scope::query::provenance::object`,
  `analysis::scope::query::rooted`, and `analysis::scope::scope_index`.

Representative callers in scope collection, assignment/control-flow
transitions, callback capture, freeze construction, and frozen provenance
queries were traced. No source changes or fixes were applied.
