# Codebase Readability Audit — Chunk 1

## Summary

Chunk 1 covers fact construction: the canonical AST traversal, provenance and
control-region state, call and pattern lowering, function boundaries, and
module-interface collection. The implementation has a strong single-pass
architecture, but several internal protocols are manually coordinated across
the visitor and helper modules. The highest-value improvements are to move
branch-state ownership into the provenance state, centralize the interface
builder's export invariant, and make the repeated traversal lifecycles
explicit.

## Findings

### Fact construction control state

#### [x] READ-001 — Encapsulate provenance branch transactions

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:67-72`; `glass-lint-core/src/analysis/facts/control.rs:48-246`; `glass-lint-core/src/analysis/facts/origin_map.rs:43-163`

`FactProvenanceState` owns the two path-sensitive origin maps, but every
control construct reaches through it and manually coordinates separate
checkpoints, restores, snapshots, intersections, commits, and rollbacks.
`record_if`, `record_for`, `record_loop`, `record_switch`, `record_try`, and
`record_conditional` therefore duplicate a transaction protocol whose failure
mode is especially costly: `OriginMap::restore_from` clears the map's log and
open-checkpoint count while the caller can still hold an active checkpoint
token. The compiler cannot enforce that both provenance maps receive the
corresponding transition or that a snapshot is joined before a branch is
closed.

**Recommendation:** Make `FactProvenanceState` the owner of branch scopes and
join operations, exposing narrow operations such as a scoped branch guard and
an explicit intersection/restore result rather than its two maps. Move the
paired checkpoint and snapshot protocol out of `control.rs`, and delete the
direct map/token manipulation there. Preserve the independent instance and
class provenance domains, conservative removal at joins, bounded snapshots,
and the special try/catch/finally semantics; this refactor must not combine
alternatives from incompatible paths or turn incomplete state into a witness.

**Fix Applied:** Added a private paired provenance transaction owner to
`FactProvenanceState`. It now owns paired checkpoint, restore, commit, and
rollback operations plus explicit instance/class snapshot, restore, and
intersection helpers. Fact control orchestration no longer reaches into
either origin map or coordinates their tokens directly; asymmetric
switch/loop/try lifecycles remain explicit through owner operations.

**Verification:** `cargo test -p glass-lint-core analysis::facts::tests::control --lib`
(6 passed); `make fmt && make ci` (passed).

### Fact construction control flow

#### [ ] READ-002 — Consolidate branch-region orchestration

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/control.rs:48-74`; `glass-lint-core/src/analysis/facts/control.rs:212-233`; `glass-lint-core/src/analysis/facts/control.rs:76-142`

`record_if` and `record_conditional` implement the same branch skeleton:
allocate a region, emit start/test/then markers, snapshot the then state,
restore the incoming state, visit the alternative, intersect state, and emit
the end marker. The loop family has the same repeated setup and cleanup until
`for_in`, `for_of`, `while`, and `do_while` were partially consolidated into
`record_loop`; `record_for` still carries a second copy with a special init,
test, and update sequence. Keeping these protocols in separate functions
makes marker ordering and state-join changes needlessly easy to diverge.

**Recommendation:** Add private FactBuilder orchestration helpers for a
two-arm branch and for the common loop lifecycle, passing only the syntax
specific visit closures and marker kinds/spans. Delete the duplicated
checkpoint/restore/commit and start/end scaffolding after READ-001 gives the
provenance owner a narrow join API. Keep the distinct `if` versus conditional
spans, loop `guaranteed` value, update marker, switch case behavior, and
try/catch/finally control kinds intact.

**Fix Applied:** None so far.

### Function fact lifecycle

#### [x] READ-003 — Centralize function-boundary state transitions

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/functions.rs:30-58`; `glass-lint-core/src/analysis/facts/functions.rs:67-135`

`record_function`, `record_arrow`, and `record_class_method` each save the
enclosing function, emit an enter fact, change traversal state, visit a body,
emit an exit fact, and restore the enclosing function. They also reconstruct
the same parameter iterator for enter and exit; `emit_function_fact` then
recomputes the function slot and conditionally builds bindings based on the
boundary. This spreads one lifecycle invariant across three syntax adapters
and makes static-method depth and function depth easy to update in the wrong
order.

**Recommendation:** Put the enter/visit/exit and current-function restoration
protocol in one private FactBuilder helper, with a small parameter descriptor
and an explicit option for static-method scope. Let the syntax-specific
methods supply only the body and class-method/static metadata, deleting the
repeated iterator construction and state-save/reset code. Preserve enter-only
parameter registration, lexical function ownership, class provenance, and the
fact order consumed by flow summaries.

**Fix Applied:** Added one private function-body lifecycle helper that owns
enter/visit/exit fact emission, parameter binding reuse, current-function
restoration, and optional function-depth/static-method transitions. Function,
arrow, and class-method adapters now provide only their parameter list, body,
and lifecycle flags.

**Verification:** `cargo test -p glass-lint-core analysis::facts --lib` passes
(32 tests), and `make fmt && make ci` passes, including the full workspace,
end-to-end, rule, doctest, and example checks.

### Pattern lowering

#### [ ] READ-004 — Move pattern semantics to a shared fact-level owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/calls/pattern.rs:5-197`; `glass-lint-core/src/analysis/facts/visitor.rs:88-90`; `glass-lint-core/src/analysis/facts/assignments.rs:137-149`; `glass-lint-core/src/analysis/facts/functions.rs:40-50`

`calls::pattern` is not call-specific: it owns declaration target extraction,
assignment-write extraction, and parameter binding paths, while callers live in
the visitor, assignment, and function modules. In addition,
`pattern_values` and `pattern_write_targets` repeat the same recursive match
over identifier, assignment, rest, array, and object patterns, differing only
at their leaves. The duplicated syntax coverage and misplaced namespace make
new SWC pattern variants likely to be handled in one semantic operation but
forgotten in another.

**Recommendation:** Establish a private fact-level pattern walker/owner and
have declaration, write, and parameter projections use it. A canonical target
description or operation-specific leaf callbacks can remove the repeated
recursive structure without forcing binding extraction and write invalidation
to share their different leaf meanings. Preserve the current treatment of
`Pat::Expr` member receivers, ignored invalid patterns, default values, rest
parameters, property/index paths, and the distinction between a declaration's
unknown source and an assignment's conservative write kill.

**Fix Applied:** None so far.

### Declaration lowering

#### [x] READ-005 — Split declaration traversal from provenance seeding

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/visitor.rs:72-136`

`visit_var_declarator` simultaneously records interface locals, resolves the
initializer, performs evaluation-order traversal, flattens the binding
pattern, applies the destructuring precision rule, seeds three provenance
tables, remembers static-string aliases, and emits one declaration per target.
The three provenance branches repeat the same simple-pattern/init/target loop,
so the visitor method is operating at AST, value-resolution, state-mutation,
and fact-emission levels at once. That makes the important rule that
destructuring gets an unknown source easy to obscure or accidentally bypass.

**Recommendation:** Split the method into named FactBuilder phases for
source/target resolution, simple-declaration provenance seeding, and ordered
fact emission; put the repeated target seeding behind a provenance-state
operation. Delete the repeated `is_simple_pattern` checks and target loops
while retaining initializer-before-binding traversal, unknown sources for
non-simple patterns, static-string alias invalidation, and independent
instance/callable/class provenance updates.

**Fix Applied:** Split declaration processing into named source evaluation,
target extraction, provenance seeding, and emission phases.
`FactProvenanceState` now owns the shared simple-declaration target seeding
for instance callables, instance origins, and class origins; non-simple
patterns remain unknown and initializer traversal remains before declaration
emission.

**Verification:** Focused facts tests (32 passed) and the full `make fmt &&
make ci` gate passed, including workspace Clippy, workspace tests, doctests,
e2e cases, and provider rule cases.

### Module-interface collection

#### [x] READ-006 — Keep export invariants behind ModuleInterfaceBuilder

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/interface/mod.rs:17-19`; `glass-lint-core/src/analysis/facts/interface/commonjs.rs:54-128`; `glass-lint-core/src/analysis/facts/interface/exports.rs:20-260`

`ModuleInterfaceBuilder` is the intended owner of the collected module
interface, but its child modules directly mutate the wrapped
`ModuleInterface` through `self.interface`. ESM and CommonJS code therefore
repeat the ordering-sensitive combination of `add_function_export`,
`add_export`, `add_static_string`, and `mark_unknown_exports`; callers must
know how the underlying model preserves or clears function and static-value
metadata when export resolutions conflict. The builder field is private only
to its module, so descendant modules can bypass the builder's narrow methods
without any API-level signal.

**Recommendation:** Hide the wrapped model behind builder methods such as
validated local/value/function export registration and static-value
association, and make the dialect modules classify syntax then call those
methods. Consolidate the repeated function-export-plus-export sequencing and
delete direct `self.interface` mutations from `commonjs.rs` and `exports.rs`.
Preserve conflict-to-unknown behavior, re-export request identity, star
exports, type-only filtering, and the fact that function/static metadata is
auxiliary to (not a replacement for) the resolved `ModuleExport`.

**Fix Applied:** Added builder-owned wrappers for export, function metadata,
static-value, star-export, request, and export-state operations. ESM and
CommonJS adapters now classify syntax and call those wrappers; no child
module directly mutates the wrapped `ModuleInterface`.

**Verification:** `cargo test -p glass-lint-core analysis::facts --lib` passes
(32 tests), and `make fmt && make ci` passes, including the full workspace,
end-to-end, rule, doctest, and example checks.

## Systemic Themes

- FactBuilder is the correct single-pass boundary, but its owned state is
  exposed as several protocols that each syntax helper must sequence manually.
- Path-local certainty and source-order emission are architectural contracts;
  any refactor must keep unknown, ambiguous, and incomplete alternatives
  fail-closed.
- Syntax dialect adapters (control forms, ESM/CommonJS exports, and pattern
  shapes) should classify input while focused domain owners maintain joins,
  export composition, paths, and lifecycle invariants.

## Open Questions

- No historical chunk audit or applied-finding record exists in the current
  worktree, so all six findings are new for this audit.
- The next handoff is Chunk 2 (`analysis::flow` modules), which should inspect
  whether the flow projector and cross-flow worklist repeat or bypass the
  state ownership protocols identified here.

## Coverage

Reviewed the Chunk 1 modules listed in `CODEBASE_STRUCTURE_CORE.md`: fact
orchestration, arguments, assignments, call results and call lowering,
control, functions, instances, module interfaces and their ESM/CommonJS
handlers, origin maps, traversal state, fact streams, and the associated fact
tests. Representative callers were traced with `rg`; workspace architecture,
core architecture, testing guidance, contribution guidance, and agent
instructions were read before review. This is a read-only audit; no source,
test, configuration, dependency, or documentation files were changed.
