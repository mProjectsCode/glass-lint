# Codebase Readability Audit — Chunk 2

This audit covers Chunk 2 of `CODEBASE_STRUCTURE_CORE.md`: `analysis::scope`,
`analysis::syntax`, and `analysis::trace`. It is an architectural review only;
no source changes were made.

## Summary

The scope subsystem has a sound high-level phase boundary: one provider-neutral
traversal plans lexical scope shape, the same traversal collects source-order
facts, and the result freezes into a query graph. The main readability risks are
at that boundary: lifecycle correctness is spread across a broad callback
protocol, malformed scope-stack state can fall back to a valid-looking root,
and several freeze APIs pass raw maps or positional tuples between owners.
The collector visitor also combines enough independent responsibilities that
source-order and fail-closed behavior are difficult to audit locally.

## Findings

### READ-001 — Scope traversal lifecycle is an implicit callback protocol

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/analysis/scope/build/traversal.rs:12-78,97-422`; `glass-lint-core/src/analysis/scope/build/visitor.rs:32-117`; `glass-lint-core/src/analysis/scope/build/assignments.rs:282-558`
- **Representative callers:** `glass-lint-core/src/analysis/scope/mod.rs:22-60` runs the same `ScopeTraversal` once with `ScopePlanner` and once with `ScopeCollector`.

`ScopePass` is the private protocol between the AST traversal and both phases,
but its lifecycle is represented by many optional hooks and independent
booleans: `push_scope` returns a bare `bool`, `pop_scope` accepts the bool,
`exit_if` receives `has_else`, and try/switch/loop hooks receive related shape
flags. The collector then forwards most of the protocol verbatim through a
large implementation in `visitor.rs`. Control-flow methods in
`assignments.rs` separately inspect `ControlFlowFrame` variants and silently
return on mismatches. The valid event sequence and the ownership of its
invariants therefore live in several files rather than in one typed boundary.

This is especially hard to reason about because the protocol simultaneously
encodes AST structure, scope entry/exit, path joins, reachability, and semantic
fact callbacks. A change to traversal ordering can remain type-correct while
changing when a checkpoint, join, or finalizer is observed.

**Recommendation:** Encapsulate structural traversal events and their lifecycle
state in a private event/guard abstraction, or split the structural scope
operations from the smaller semantic-fact callback surface. Make impossible
event sequences explicit in the private API and return a fail-closed issue for
shape/control-frame mismatches. Preserve the single shared traversal, planner /
collector parity, finalizer ordering, bounded joins, and deterministic source
order; this is not a recommendation to add another AST walk.

**Fix Applied:** None so far.

### READ-002 — Empty scope stacks become the default scope

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/analysis/scope/build/plan.rs:97-99,147-149`; `glass-lint-core/src/analysis/scope/build/collector.rs:54-56,158-184`
- **Representative callers:** `ScopePass::current_scope` is consumed by declaration, assignment, callback, and property-artifact recording hooks throughout `scope/build/visitor.rs`.

Both planning and collection expose `current_scope()` as a total function that
returns `ScopeId::default()` when the stack is empty. The collector's
`pop_scope` only protects the program root with `debug_assert!`; in production,
an underflow is not returned as a collection issue. Its `push_scope` can already
report a planned-shape mismatch, but later hooks still have a root-looking
scope available through the fallback. The planner's `pop_scope` also discards
the stack entry without checking that the expected scope was actually present.

An AST/traversal imbalance or a future lifecycle bug can consequently attach a
fact to the default/program scope or continue analysis with corrupted shape
state instead of making the affected result unsupported. That weakens the
scope graph's existing fail-closed `scope_shape_valid` contract and makes the
invariant invisible to callers.

**Recommendation:** Give the phase-owned stack a checked pop and an explicit
invalid/absent current-scope state. Route underflow, unexpected root removal,
and unconsumed shape through `ScopeCollectionIssue` (or an equivalent private
phase error), and make fact-recording hooks refuse to establish evidence once
the structure is invalid. Keep the program scope as an intentional root and
retain the existing bounded, deterministic behavior for planned-shape
mismatches.

**Fix Applied:** None so far.

### READ-003 — Mutable and frozen graph query adapters repeat the same delegation surface

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / API
- **Location:** `glass-lint-core/src/analysis/scope/graph.rs:29-70,164-209,256-299,301-477`
- **Representative callers:** `ScopeGraph::freeze` moves the shared `ScopeData` into `FrozenScopeGraph` at `graph.rs:145-161`; query users call the frozen wrappers from scope query modules.

`ScopeData<M>` already owns the names, lexical index, binding index, and
mutation implementation shared by both graph phases. Nevertheless, the graph
module re-expresses a substantial read-only delegation surface on
`FrozenScopeGraph`, while `ScopeGraph` carries parallel collection-time
helpers. The methods are not all textually identical because collection uses
source names and mutable mutation builders while frozen queries often use
interned IDs, but the phase wrapper remains responsible for many repetitive
forwarders and for maintaining consistent scope-validity behavior.

This spreads the graph's public-in-crate API across the phase wrappers and makes
it easy for a query to be added to one phase, or to apply `scope_shape_valid`
in one path but not another. It also obscures which operations are genuinely
collection-only versus read-only operations on the shared data.

**Recommendation:** Define a narrow private read-only query view/trait over the
shared data, with explicit adapters for the string-to-ID collection boundary
and for the mutable/frozen mutation index. Keep `ScopeGraph` and
`FrozenScopeGraph` as distinct phase types, retain shape-validity gating and
the freeze transition, and avoid exposing `ScopeData` or its storage. The goal
is to centralize shared query semantics, not to erase the phase boundary.

**Fix Applied:** None so far.

### READ-004 — Collector visitor hooks mix unrelated source-order responsibilities

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/analysis/scope/build/visitor.rs:123-245,248-273`
- **Representative callers:** `ScopeTraversal` invokes these hooks from `traversal.rs:157-422` while walking the same AST used for lexical and control-flow collection.

`visit_var_decl` handles mutable-static-object detection, pending function
metadata, function aliases, pattern-local insertion, derived function
patterns, declaration classification, require aliases, value aliases, and
binding insertion. `visit_assign_expr` handles identifier assignment history,
rooted property mutation, member-root invalidation, property aliases, and
destructuring aliases. `visit_call_expr` additionally combines modeled callback
recording, dynamic-evaluation detection, budget charging, and function-call
capture. These are coherent with one source-order traversal, but each method
currently acts as a dispatcher and state mutation hub for several independent
domains.

The result is difficult to review against the strict witness rules: a reader
must track lexical state, assignment checkpoints, property artifacts, function
metadata, and budget behavior at once. Small ordering changes can alter later
provenance without making the local method obviously wrong.

**Recommendation:** Keep one visitor and one source-order event, but split each
hook into named target-specific helpers: declaration metadata, lexical pattern
installation, declaration classification, identifier assignment, member
mutation, and destructuring alias collection. Give helpers the narrow state
they mutate where practical and keep the hook responsible only for ordering
and fail-closed early returns. Preserve all existing checkpoint timing,
artifact ordering, budget charges, and the rule that unsupported/dynamic values
cannot create a witness.

**Fix Applied:** None so far.

### READ-005 — Binding-index freeze input and allocation use positional/raw internal APIs

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Conversion
- **Location:** `glass-lint-core/src/analysis/scope/binding_index.rs:25-35,53-105,107-147`; `glass-lint-core/src/analysis/scope/build/freeze.rs:23-44`
- **Representative callers:** `ScopeCollector::freeze` constructs the only observed `BindingIndexInput` and destructures the three-value result of `BindingIndex::allocate_ids`.

The freeze transition assembles seven loosely related maps/vectors into
`BindingIndexInput`, then `TryFrom` converts three scope-keyed collections to
function-keyed collections in three nearly identical iterator blocks. The
preceding `allocate_ids` returns three unrelated maps as a positional tuple.
The relationship between a lexical scope, its function ID, function span, and
the function-binding aliases is therefore conveyed by field names at one
boundary and tuple position at the next. Invalid scope references are handled
only after the aggregate has already crossed the boundary.

This is a private API, but it is an important architectural transition: the
collector owns source-order facts while `BindingIndex` owns frozen query
indexes. Raw map aggregates make the ownership and conversion invariants harder
to see and duplicate the scope-to-function lookup policy.

**Recommendation:** Introduce a named private allocation result (for example,
`BindingAllocation`) and a binding-index construction method that accepts the
collector's semantic freeze record rather than a broad raw-map bag. Centralize
the scope-to-function conversion and retain an explicit error for missing
function scopes; preserve the current `InvalidBindingIndex` fail-closed fallback
and deterministic ID allocation.

**Fix Applied:** None so far.

### READ-006 — Freeze artifacts lose domain names through tuple decomposition

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Conversion / API
- **Location:** `glass-lint-core/src/analysis/scope/build/program.rs:19-89`; `glass-lint-core/src/analysis/scope/build/mod.rs:105-125`; `glass-lint-core/src/analysis/scope/graph.rs:210-253`
- **Representative callers:** `ScopeGraph::finish_collected_properties` destructures `PropertyAliasAssignment`, `RootedPropertyMutation`, and `ScopedDynamicEval` with `into_parts` before rebuilding query facts.

The collector artifacts have meaningful domain types and private fields, but
their phase-boundary APIs expose positional tuples. The graph then coordinates
receiver binding-key lookup, symbol-path normalization, mutation-fact creation,
and dynamic-eval filtering in one conversion method. Once an artifact has been
decomposed, the compiler no longer helps distinguish similarly typed values
such as span/scope/path/property, and the conversion ownership is split between
the artifact definitions and the graph.

The behavior is currently understandable, but this boundary is fragile as
artifact fields evolve: tuple order and the graph's filtering conditions must
stay synchronized manually.

**Recommendation:** Replace positional `into_parts` contracts with named
conversion records or an owner-directed lowering operation (for example, a
graph adapter that consumes each typed artifact). Keep receiver resolution,
path normalization, dynamic-eval filtering, and mutation-index finalization in
one clearly named boundary, and preserve the current sorted/frozen indexes and
unsupported-artifact behavior.

**Fix Applied:** None so far.

## Systemic Themes

- **ENCAPSULATE:** Lifecycle state, scope-stack validity, freeze inputs, and
  artifact conversion are carried through broad or positional internal APIs.
- **SIMPLIFY:** The collector's visitor methods are source-order hubs for
  lexical, provenance, assignment, property, callback, and budget state.
- **DEDUPLICATE:** Shared graph queries and repeated scope-to-function
  conversion policy should have one semantic owner.

## Decisions and Coverage

Reviewed the scope graph, two-pass scope builder, traversal protocol, lexical
and assignment state, binding/provenance queries, syntax-name and bounded
constant helpers, and trace arena. No additional actionable readability issue
was recorded in the small syntax-name or trace modules; their APIs are already
relatively narrow and evidence-oriented. The constant evaluator's fresh
`evaluate` and shared-state `contextual_member_property_name_with_state`
entry points are intentionally distinct: the former starts an independent
bounded evaluation, while the latter charges a computed property against the
caller's existing recursion, node, and lookup budget. Keep both narrow helpers
and document the budget scope; combining them would silently reset or
overcharge a caller's bound.

## Handoff

Chunk 2 is reviewed. The next unreviewed chunk is **Chunk 3**; create
`CODEBASE_READABILITY_AUDIT_CHUNK_03.md` and review the next boundaries listed
in `CODEBASE_STRUCTURE_CORE.md`.
