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

#### [ ] READ-001 — Keep provenance transitions behind one semantic owner

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

**Fix Applied:** None so far.

### Traversal lifecycle state

#### [ ] READ-002 — Make function and class traversal context scoped

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

**Recommendation:** Introduce a scoped traversal-context guard owned by
`TraversalState` (or one guard per context kind) that records the previous
function and class/depth state and restores it on drop, with explicit commit
points for the existing function boundary facts. Replace the manual pairs in
the three visitor paths and delete the saturating leave API, while preserving
the precise order of enter/body/exit facts, static-method visibility during
the body, nested class provenance, and deterministic region allocation.

**Fix Applied:** None so far.

### Function parameter lookup contract

#### [ ] READ-003 — Distinguish an unknown function from a function with no parameters

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

**Fix Applied:** None so far.

### Module-interface recording boundary

#### [ ] READ-004 — Route static import recording through the builder’s domain API

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

**Fix Applied:** None so far.

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

## Open Questions

- Whether the function-boundary facts intentionally require the static-method
  flag to remain active while the exit fact is emitted should be confirmed
  before introducing a guard; the guard must preserve the current ordering.
- Whether static imports and dynamic/require observations are deliberately
  allowed to use different request spans should be confirmed when consolidating
  the builder API; this audit does not recommend changing their semantics.

## Coverage

Reviewed only Chunk 1, “Source fact construction,” from
`CODEBASE_STRUCTURE_CORE.md`, including the owning crate architecture,
fact-builder entry points, stream freeze transition, provenance checkpoints,
visitor lifecycle paths, module-interface builder, and representative flow
consumers. No source, test, configuration, dependency, or other documentation
files were changed. No prior applied-finding audit history existed in the
worktree.
