# Codebase Readability Audit — Chunk 14

## Summary

Chunk 14 owns the declaration plan, source-order scope state, frozen lexical
indexes, bounded constant-evaluation state, and trace-facing syntax identity
types. The overall planner → collector → freeze lifecycle is deliberate and
preserves conservative identity, but several internal boundaries do not fully
enforce that lifecycle. The “frozen” graph still owns a mutable name table,
invalid scope collection is represented as a valid-looking root scope, and the
mutation index relies on a caller-only finalization step. Binding identity,
checkpoint transitions, and function-span lookup also have parallel operations
that should have narrower owners.

The broad collector, path-join, traversal, tuple-freeze, and expression-shape
findings were cross-checked against Chunk 5; constant conversion, raw value
construction, and trace storage were cross-checked against Chunk 6; and
`ScopeId` representation and provenance-join bounds were cross-checked against
Chunk 12. Those findings are not repeated here.

## Findings

### Scope freeze and validity boundaries

#### [ ] READ-001 — Separate the frozen lexical graph from mutable name interning

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/graph.rs:74-149,290-309`; mutation callers in `analysis/resolution/mod.rs:153-160,190-203,270-286` and `analysis/resolution/constant.rs:65-83`

`FrozenScopeGraph` is documented as a read-only query graph, but it retains a
`NameEnvironment` whose `NameTable` is exposed through `name_table_mut`. The
resolver interns names and static-object keys through that accessor after the
scope graph has been frozen, and `into_name_table` later consumes the same
mutable table into the fact-stream freeze. Lexical bindings and paths therefore
share an artifact-local ID space, but the phase boundary called `freeze` does
not actually make the graph immutable.

This forces every reader of `FrozenScopeGraph` to know that “frozen” means only
that lexical maps and mutation indexes are no longer being collected. It also
makes it difficult to reason about whether a query can add names, whether a
new `NameId` is visible to all retained indexes, and which owner is responsible
for name-table exhaustion. A future resolver or report caller could mutate the
table through the graph while treating the rest of the graph as a stable
snapshot.

**Recommendation:** Give the resolver one explicit name-interning session that
shares the artifact-local ID table with lexical queries, while making the
lexical graph itself immutable. Move `name_table_mut` and `into_name_table` off
`FrozenScopeGraph`; construct the resolver with the shared name context or a
separate resolver-owned table view, and make the final table transfer an
explicit resolver freeze operation. Preserve identical IDs for existing scope
paths, bounded interning and exhaustion diagnostics, static-object key
conversion, and the fact that names remain local to one artifact.

**Fix Applied:** None so far.

#### [ ] READ-002 — Represent invalid scope queries separately from the program root

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/build/freeze.rs:13-56`; `analysis/scope/graph.rs:79-105,163-165,358-372`; `analysis/scope/scope_index.rs:49-85`; status handoff in `analysis/lowering/mod.rs:252-263`

`ScopeCollector::freeze` collapses every structural issue into the boolean
`scope_shape_valid`. When that boolean is false, `LexicalScopeIndex::scope_at`
returns `ScopeId::new(0)`, the same identity used for the valid program scope
and for ordinary “no containing scope” fallback. `ScopedProgram` carries the
issues separately for status reporting, but graph query methods receive only a
scope ID and can continue resolving bindings, globals, and function ancestry
against the synthetic root.

The API therefore conflates a valid root lookup with an unavailable or
structurally invalid lookup. Every caller that wants fail-closed behavior must
remember to consult a separate issue vector, and a caller holding only the
frozen graph cannot distinguish the states. That is especially risky for
possible-witness construction: a mismatched scope shape should not silently
look like a complete root binding merely because the enclosing status is
recorded elsewhere.

**Recommendation:** Make scope lookup return a typed result such as
`Result<ScopeId, ScopeQueryError>` or carry a validated `ScopeValidity` token in
the graph, and keep the program root as a distinct valid identity. Propagate
invalidity through binding, function, rooted-path, and dynamic-lookup queries
instead of manufacturing `ScopeId(0)`; delete the parallel boolean/issue
interpretation after migration. Preserve ordinary root queries for valid
program spans, deterministic out-of-range handling, structural diagnostics,
and the rule that invalid or incomplete analysis cannot establish a definite
witness.

**Fix Applied:** None so far.

#### [ ] READ-003 — Seal mutation-index construction before exposing ordered queries

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Lifecycle
- **Location:** `glass-lint-core/src/analysis/scope/mutation_index.rs:14-115`; construction and finalization in `analysis/scope/graph.rs:198-238`

`MutationIndex` exposes `record_property_assignment`,
`record_rooted_mutation`, `record_dynamic_evals`, and `finalize` on the same
type that serves queries. The query `has_prior_eval` uses `partition_point`,
which requires `dynamic_evals_by_scope` to be ordered by `span.hi`, while the
other query slices also rely on final source ordering. The only construction
path happens to call `finalize`, but the type does not prevent a query before
that transition. `record_dynamic_evals` also clears all existing dynamic
evaluations despite its record-like name, so a second call silently replaces
previous state.

The sort and replacement rules are consequently caller-owned and invisible in
the index's phase. A new collection path, incremental builder, or test helper
can produce a non-monotonic slice and make `partition_point` answer incorrectly
without violating the Rust type contract.

**Recommendation:** Split a mutable `MutationIndexBuilder` from an immutable
`FrozenMutationIndex`; make the builder's consuming `finish` perform all
sorting and make `record_dynamic_evals` either append intentionally or be a
named replacement operation. Delete the public-within-analysis `finalize`
transition and have `ScopeGraph::finish_collected_properties` consume the
builder. Preserve source-order assignment and rooted-mutation evidence,
dynamic-eval half-open span semantics, deterministic ordering, and mutable
static-object membership.

**Fix Applied:** None so far.

### Scope identity and history operations

#### [ ] READ-004 — Centralize construction of versioned binding keys

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/graph.rs:265-283`; duplicate construction in `analysis/scope/query/bindings.rs:79-157` and `analysis/scope/query/provenance/callable.rs:91-105`; callers in `analysis/scope/query/provenance/{chain,object}.rs`

The same semantic identity—enclosing function, allocated binding ID, and
source-position binding version—is assembled in the mutable graph's
`binding_key_for_name`, the frozen query's `lexical_identifier_key` and
`binding_key_for_name`, and `ident_value_seed`. The expression path then
appends member IDs on top of whichever copy was used. These paths differ in
whether they permit a global fallback, but their lexical-key construction is
duplicated and each caller must remember to use the same version lookup.

That duplication makes assignment-version changes especially easy to miss in
one path: property mutation indexing, value seeds, and member provenance could
then disagree about whether two uses share an identity. It also leaves the
scope graph and query module with parallel helpers whose names imply one
operation but whose fallback semantics are only visible in their bodies.

**Recommendation:** Put one lexical-key operation on the owning scope data,
for example `binding_key_at(scope, name, span)`, and expose separate thin
wrappers for lexical-only and lexical-or-global expression keys. Have
`ident_value_seed`, property mutation collection, and member provenance call
that operation, deleting the repeated `BindingKey::new(BindingRoot::Binding { …
})` blocks. Preserve artifact-local IDs, assignment versions, enclosing
function identity, lexical shadowing, `this` handling, and the distinction
between an unbound global root and a proven lexical binding.

**Fix Applied:** None so far.

#### [ ] READ-005 — Share owned checkpoint transition plumbing for assignment and write history

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/history.rs:24-175,187-283`; checkpoint callers in `analysis/scope/build/assignments.rs:130-180,252-431`

`AssignmentEnvironment` and `WriteSet` each wrap a
`ParentLinkedHistory`, allocate a `HistoryOwner`, define a typed checkpoint,
validate the owner, call `transition`, and translate undo/redo errors into the
same `ForeignCheckpoint` result. Their delta application is necessarily
different—the first restores provenance maps and the second restores a
generation-tagged write set—but the ownership and cursor lifecycle is copied
in full.

The duplicated protocol makes the correctness boundary harder to review: a
change to invalid-cursor handling, history ownership, or transition semantics
must be mirrored in two state machines, while the collector coordinates both
checkpoints as one path state. The current separate checkpoint structs are
useful, so collapsing the semantic states would be wrong; the reusable part is
the owned transition shell.

**Recommendation:** Add one private typed `OwnedHistory<D>` helper that owns
the unique history token, creates a domain checkpoint, rejects foreign or
invalid positions, and invokes a caller-supplied undo/redo adapter. Keep
`AssignmentDelta` and `WriteDelta` plus their domain application functions
separate, and delete the repeated owner/transition boilerplate after callers
migrate. Preserve branch creation after restore, LCA ordering, fail-closed
invalid checkpoints, bounded provenance alternatives, write generations, and
deterministic join behavior.

**Fix Applied:** None so far.

### Function identity indexing

#### [ ] READ-006 — Let the binding index own function-span lookup

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/binding_index.rs:44-52,189-203`; query scans in `analysis/scope/query/functions.rs:13-73`; representative callers in `analysis/facts/interface/{commonjs,exports}.rs`

`BindingIndex` stores function identity as a scope-indexed
`Vec<Option<FunctionId>>`, and `function_spans` reconstructs `(FunctionId,
Span)` pairs by enumerating every scope whenever it is called. The query layer
then scans that iterator to find a function's end for reassignment checks in
`function_id_for_expr`, and scans it again across all functions to find the
smallest enclosing function in `function_id_for_span`. Interface extraction
calls these queries repeatedly while processing exports and CommonJS values.

The function identity owner therefore exposes a storage-shaped iterator rather
than the operations its callers need, and the query layer repeats global scans
and span reconstruction. This mixes scope storage with function containment
policy and makes a change to function allocation or span indexing a concern of
every caller.

**Recommendation:** Have `BindingIndex` build a private function index during
freeze with direct `FunctionId → Span` lookup and an owner-level
`function_containing(span)` operation (using a deterministic interval order).
Let `function_id_for_expr` request the function end and let
`function_id_for_span` request the containing function, deleting the repeated
`function_spans` scans. Preserve stable program/function IDs, smallest
containing-span selection, reassignment checks between function end and use,
alias resolution, and deterministic export discovery.

**Fix Applied:** None so far.

## Systemic Themes

- Phase names such as `freeze` and `FrozenScopeGraph` currently describe an
  intended lifecycle more strongly than the types enforce it. Mutable name
  interning, mutation-index sorting, and validity status remain external to
  their apparent owners.
- Root-scope sentinels, raw positional indexes, and repeated versioned-key
  construction make semantic identity depend on caller convention. Typed
  invalid outcomes and owner-level identity operations would make the
  precision boundary easier to audit.
- The scope module has two legitimate domain histories, but their checkpoint
  mechanics should be shared without merging provenance and write semantics.
- Prior Chunk 5, Chunk 6, and Chunk 12 findings remain applicable and are not
  duplicated here.

## Open Questions

- Should name interning be a resolver-owned session shared by immutable lexical
  queries, or should all names needed by constant/value resolution be admitted
  before the lexical graph freezes? Either design needs one artifact-local ID
  owner and an explicit exhaustion transition.
- Does a scope-shape mismatch intentionally permit any semantic query at all?
  If not, a typed unavailable result should replace the current root fallback;
  if some queries remain useful, the validity type should say which ones.
- Will function-span queries remain frequent enough to justify an interval
  index, or is a direct function-to-span map plus a sorted span vector the
  intended bounded representation? The ownership should still stay in the
  binding/function index.

## Coverage

Reviewed all types listed in Chunk 14 of `CODEBASE_STRUCTURE_CORE.md`:

- Binding/scope build: `BindingIndex`, `BindingIndexError`,
  `BindingIndexInput`, `ParameterAliasKey`, `CollectorCheckpoint`,
  `ControlFlowFrame`, `FrozenPropertyArtifacts`,
  `FrozenScopeCollectionArtifacts`, `FunctionBinding`, `FunctionCall`,
  `FunctionCheckpoint`, `PathCollectionState`, `PendingFunctionName`,
  `ScopeCollectionArtifacts`, `ScopeCollector`, `ScopedDynamicEval`,
  `Candidate`, `DeclarationClassification`, `CompactPat`, `AssignmentDelta`,
  `AssignmentEnvironment`, `Cursor`, `HistoryOwner`, `HistoryRestoreError`,
  `WriteCheckpoint`, `WriteDelta`, `WriteSet`, `ScopePlan`, `ScopePlanner`,
  `PropertyAliasAssignment`, `RootedPropertyMutation`,
  `ScopeCollectionIssue`, `ScopedProgram`, `ProjectionError`, `ScopeShape`,
  `ScopeShapeKey`, `ScopeShapeTable`, `ScopeTraversal`, and `ScopePass`.
- Frozen/query indexes: `AssignmentAt`, `FrozenAssignmentIndex`,
  `FrozenScopeGraph`, `ScopeData`, `ScopeGraph`, `ScopeGraphInput`,
  `MutationIndex`, `NameEnvironment`, `RootMode`, `RootedExprContext`, and
  `LexicalScopeIndex`.
- Syntax constants: `EvalState`, `Lookup`, `NoLookup`, and `ConstValue`.
- Trace: `QualifiedEvent`, `TraceArena`, `TraceNode`, `TraceNodeId`, and
  `TraceStep`.

Representative planner/collector/freeze transitions, invalid-shape handling,
mutation-index queries, assignment and write checkpoints, binding-key callers,
function export discovery, bounded constant evaluation, and trace ownership
were inspected. Chunk 5, Chunk 6, Chunk 12, and the prior Chunk 13 report were
cross-checked to reject overlapping findings. No source, test, configuration,
dependency, or existing audit files were changed.
