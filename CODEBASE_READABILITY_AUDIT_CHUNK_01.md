# Codebase Readability Audit

## Summary

Chunk 1 is the single-source semantic fact-construction boundary: one
scope-prepared AST traversal populates the bounded fact stream, provenance
state, call-result state, and module interface before the lowering phase freezes
the artifact. The phase-typed `FactStream` and `BuiltFacts` transition are good
ownership boundaries, but several internal APIs still let individual visitor
modules manipulate shared invariants directly or silently collapse invalid
states into valid empty states.

## Findings

### Provenance state and fact-builder collaborators

#### [x] READ-001 — Keep provenance transitions behind one semantic owner

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:77-203, 301-315`; `glass-lint-core/src/analysis/facts/calls/callee.rs:164-239`; `glass-lint-core/src/analysis/facts/visitor.rs:275-281, 516-531`

`FactProvenanceState` owns instance callables, instance/class origin maps, and
static-string origins, and it already provides composite branch operations.
However, child `FactBuilder` implementations reach through
`self.provenance.origins.instances` and `.classes` to perform raw `get` and
`insert` operations, while `replace_targets` separately updates three maps.
The invariant that a value's callable, instance origin, class origin, and
static-string origin are replaced consistently is therefore split between the
state owner and syntax-specific modules; a new path can update one channel
without preserving the correlated-alternative rules enforced by the branch
methods.

**Recommendation:** Make `FactProvenanceState` the only owner of semantic
provenance operations: expose methods such as instance/class lookup and
recording, static-string origin recording, callable extraction, and an atomic
target replacement, while keeping `OriginChannels` and `OriginMap` fields
private to that owner. Consolidate the branch checkpoint/snapshot operations
there as well, then remove direct map accesses from `callee.rs` and
`visitor.rs`; retain separate instance and class channels, bounded snapshots,
and the rule that unknown or exhausted alternatives never become witnesses.

**Fix Applied:** `FactProvenanceState` now owns semantic provenance lookup and
recording for instance/class origins, instance callables, and static-string
origins. Fact visitor and callee code no longer reach into the backing maps;
correlated replacement and branch checkpoint behavior remains centralized.
Verified with `cargo test -p glass-lint-core analysis::facts` and
`make fmt && make ci`.

### Traversal lifecycle state

#### [x] READ-002 — Make function and class traversal context scoped

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/state.rs:13-92`; `glass-lint-core/src/analysis/facts/functions.rs:76-128, 166-215`

`TraversalState` exposes paired `enter_*`/`leave_*` methods, a mutable current
function slot, and saturating depth counters. `record_function_decl`,
`record_function_body`, and both class visitors manually arrange those calls
around nested AST visits; an omitted leave or an early-return path silently
clamps the counter and changes later provenance interpretation instead of
making the lifecycle violation explicit. The duplicated enter/leave plumbing
also leaves the current-function restoration as a separate operation from the
depth restoration.

**Recommendation:** Give `TraversalState` closure-based `with_function_*` and
`with_class_*` operations that save the previous context, run the body, and
restore it through one owner. Keep the function-boundary fact emission outside
or at explicit points in that closure so the exit fact is emitted while the
required context is still active. Replace the manual pairs and saturating
leave API without requiring a `Drop` guard that would make the builder borrow
awkwardly; preserve enter/body/exit order, static-method visibility, nested
class provenance, and deterministic region allocation.

**Fix Applied:** Centralized function, static-method, class, and current-
function restoration in closure-scoped `FactBuilder` helpers, preserving
boundary fact order and nested provenance behavior. Verified with
`make fmt && make ci`.

### Function parameter lookup contract

#### [x] READ-003 — Distinguish an unknown function from a function with no parameters

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:159-182`; representative callers `glass-lint-core/src/analysis/flow/effect/mod.rs:311-315, 585-592` and `glass-lint-core/src/analysis/flow/summary/summaries.rs:189-196`

`FactStream::function_parameters` returns the same empty slice for the
program-level/exit slots, a registered zero-parameter function, and a
`FunctionId` that cannot convert to or index the parameter table. Flow effect
and summary code therefore cannot tell a valid empty parameter list from an
invalid or stale function identity. That weakens the API at a phase boundary:
an invalid target can be treated as a valid no-argument function rather than
propagating an incomplete/unsupported result.

**Recommendation:** Return an explicit lookup result, for example
`Option<&[ParameterBinding]>` where `Some(&[])` is a valid empty list and
`None` means the identity is not registered, or use a domain lookup type that
also represents the intentional program/exit slots. Update effect and summary
owners to handle `None` as incomplete and keep the existing valid behavior for
program-level and zero-parameter functions; do not turn malformed identities
into definite flow matches.

**Fix Applied:** Changed parameter lookup to return `Option`, preserving
`Some(&[])` for the program-level and registered zero-parameter slots while
propagating missing identities as fail-closed effect/summary/worklist paths.
Verified with `make fmt && make ci`.

### Module-interface recording boundary

#### [x] READ-004 — Route static import recording through the builder’s domain API

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/visitor.rs:213-257`; comparison boundary `glass-lint-core/src/analysis/facts/mod.rs:469-535` and `glass-lint-core/src/analysis/facts/interface/mod.rs:98-129`

Most module-interface mutations are expressed through syntax-facing
`FactBuilder` methods such as `record_named_export`, `record_default_decl`,
and `observe_module_call`, which centralize span normalization and interface
ownership. `visit_import_decl` instead reaches directly into
`self.interface.add_request` and independently constructs the request role,
while also emitting the corresponding import fact. This leaves two recording
styles for the same module-request concept and makes future request-specific
validation or budget handling easy to add in one path but omit from static
imports.

**Recommendation:** Add one `FactBuilder` operation for a validated static
import that records the request and import fact together, then remove the
visitor’s direct `ModuleInterfaceBuilder` access. Preserve type-only filtering,
the source-literal span, imported-binding provenance, the no-child-traversal
rule, and the distinction between static imports, dynamic imports, require,
and wrapped require.

**Fix Applied:** Moved static-import request construction, local recording,
span normalization, and import-fact emission into one `FactBuilder` operation;
the AST visitor now delegates to it. Verified with `make fmt && make ci`.

## Systemic Themes

- The strongest boundary is the consuming `Building -> Frozen` stream
  transition. Preserve that phase typing while narrowing the APIs used by
  visitor submodules before the transition.
- Fact construction is intentionally one traversal, but one traversal does
  not require every helper module to manipulate the aggregate builder’s raw
  state. Semantic owners should expose operations that encode correlated
  provenance and lifecycle invariants.
- Fail-closed behavior is already present at indexing/projection boundaries;
  lookup APIs should preserve the same distinction between unsupported input,
  invalid identity, and a valid empty result.

## Decisions

- The static-method context must remain active through the `FunctionBoundary::Exit`
  fact. The current sequence leaves function depth, emits the exit fact, then
  leaves static-method depth and restores the enclosing function; any scoped
  guard must encode that ordering rather than restore all state immediately
  after the body callback.
- Different request spans are intentional: static imports and re-exports use
  the source literal span, while dynamic imports and `require` use the
  recognized specifier span. READ-004 should consolidate recording ownership
  only; it must not normalize these spans.

## Coverage

Reviewed only Chunk 1, “Source fact construction,” from
`CODEBASE_STRUCTURE_CORE.md`, including the owning crate architecture,
fact-builder entry points, stream freeze transition, provenance checkpoints,
visitor lifecycle paths, module-interface builder, and representative flow
consumers. No source, test, configuration, dependency, or other documentation
files were changed. No prior applied-finding audit history existed in the
worktree.
