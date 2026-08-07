# Codebase Readability Audit — Chunk 1

## Summary

Chunk 1 covers the provider-neutral fact-construction pass: the single AST
visitor, its traversal/provenance state, the bounded fact stream, call and
pattern lowering, and module-interface extraction. The pass has a sound
single-owner goal, but several internal APIs expose phase mechanics to callers
or encode important semantic modes as loosely related operations and booleans.

## Findings

### Fact provenance and control-flow joins

#### [x] READ-001 — Encapsulate provenance transaction outcomes

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:72-175`; `glass-lint-core/src/analysis/facts/control.rs:108-228`

`FactProvenanceState` exposes separate `checkpoint`, `restore`,
`restore_instances`, `commit`, `commit_instances`, `rollback`, and
`rollback_classes` operations. The control visitor must therefore know the
representation-level policy that loops, switches, `try`, and branches commit
instance origins but roll back class origins, while an if-without-else rolls
back both maps. This is a semantic join invariant distributed across the
callers rather than an operation owned by the state that represents it.

**Recommendation:** Make `FactProvenanceState` expose named control-flow
transitions (for example, an explicit branch/loop transaction or join result)
that perform the appropriate restore/intersection/commit policy internally;
the control visitor should only identify the construct and visit its arms.
Delete the paired `*_instances`/`*_classes` calls from `control.rs` once the
policy has one owner. Preserve correlated path-local alternatives, the
intersection semantics at joins, bounded snapshots, and the distinction that
an unknown or exhausted origin cannot become a proven origin.

**Fix Applied:** `FactProvenanceState` now owns the named completion
transitions for control regions and branch joins, including the instance/class
commit and rollback policy. `control.rs` only visits construct arms and invokes
those transitions; it no longer performs paired checkpoint operations.
Verified with `make fmt && make ci` (including the focused
`cargo test -p glass-lint-core` coverage).

### Fact target provenance transitions

#### [x] READ-002 — Centralize target replacement and invalidation

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/facts/assignments.rs:50-83`; `glass-lint-core/src/analysis/facts/visitor.rs:582-613`; `glass-lint-core/src/analysis/facts/mod.rs:156-216`

`FactBuilder::record_identifier_assignment` directly clears and reseeds the
instance-callable, instance-origin, and class-origin maps, while declaration
handling separately calls `seed_declaration` and
`remember_static_string_alias`. These are two callers implementing the same
target-state transition with different operation sequences, and the invariant
that a write invalidates stale derived provenance is consequently not
represented by one API. Adding another derived provenance category or another
declaration/assignment form requires finding and updating both paths.

**Recommendation:** Put a single `FactProvenanceState::replace_target` (or
equivalent domain operation) on the provenance owner. It should clear all
derived state for the target, then install only the values proven by the new
expression, and update static-string origin state through the same transition;
declaration and assignment lowering should supply the target set and proven
source facts rather than manipulate maps. Delete the duplicated map surgery
from `assignments.rs` and the separate partial seeding path after migration.
Keep destructuring writes conservative, preserve current evaluation order, and
ensure repeated hoisted declarations, unknown, dynamic, reassigned, and
unsupported values remain unable to establish a witness.

**Fix Applied:** `FactProvenanceState::replace_targets` now owns clearing and
replacing instance callables, instance/class origins, and static-string
origins. Identifier assignments, declarations, and destructuring writes all
use that operation, so uninitialized or unsupported replacements cannot retain
stale derived provenance. Added regressions for redeclarations and
destructuring writes. Verified with `make fmt && make ci`.

### Function boundary fact construction

#### [x] READ-003 — Replace boolean function-body modes with an explicit boundary API

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/functions.rs:25-127`

`record_function_body` accepts two independent booleans,
`track_function_depth` and `static_method`, even though they select materially
different traversal semantics. Its callers use combinations such as
`(true, false)` for ordinary function nodes, `(false, false)` for arrows, and
`(false, method.is_static)` for class methods; the special declaration path in
`record_function_decl` separately manages function depth. The same parameter
patterns are cloned and passed to the exit call even though
`emit_function_fact` documents that only enter facts consume parameter
bindings. This makes the lifecycle difficult to read and easy to extend with
an invalid combination.

**Recommendation:** Model the boundary as an explicit function/body kind or
separate enter/visit/exit operations that own the depth and static-method
guards. Make the exit operation accept only the span and identity it needs, so
the parameter vector is not cloned or reconstructed for an exit marker. Delete
the boolean branches and unused exit parameter plumbing while preserving the
enter/exit fact order, lexical function IDs, declaration-vs-body nesting
behavior, static-method `this` handling, and restoration of the enclosing
function after nested traversal.

**Fix Applied:** Replaced the boolean function-body flags with an explicit
`FunctionBodyKind` covering functions, arrows, and instance/static methods.
Exit facts now carry no parameter bindings, while enter/exit ordering and
function-depth/static-method restoration remain unchanged.

### Fact-builder construction boundary

#### [ ] READ-004 — Hide internal fact-builder construction behind the lowering owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:178-248`; `glass-lint-core/src/analysis/lowering/mod.rs:300-327`

`FactBuilder::with_limit` is a public constructor on the private facts module
that accepts a mutable `Resolver` and a raw `usize` fact limit, while the
lowering module is the lifecycle owner that creates the resolver, derives the
limit, walks the program, and freezes the resulting tables. The constructor
therefore exposes an internal assembly seam and lets callers couple fact
construction directly to resolver ownership and stream sizing. The builder
also forwards a broad set of interface-recording methods, making its surface a
transport for two subsystems rather than a small lowering contract.

**Recommendation:** Move construction behind a lowering-owned entry point or a
focused private build context that receives validated analysis limits and
returns `BuiltFacts`; keep resolver borrowing, stream limits, and freeze
ordering inside that owner. Remove the externally callable raw `with_limit`
surface and narrow the builder’s interface forwarding to the operations the
visitor genuinely needs. Preserve one AST traversal, matcher-independent
facts plus module interface output, deterministic ordering, and fail-closed
handling of budget/resource exhaustion.

**Fix Applied:** None so far.

## Systemic Themes

- Semantic state transitions are mostly centralized in dedicated types, but
  control-flow and target-replacement callers can still reach into the
  provenance representation and choose partial policies themselves.
- The chunk correctly keeps SWC traversal private and facts append-only; any
  refactor must not introduce rule-specific traversal or a second semantic
  model.
- Boundedness and uncertainty are architectural contracts. Encapsulation work
  must preserve incomplete streams, unknown values, deterministic fact order,
  and strict path-local identity.

## Decisions

- Repeated `var` declarations resolve through the same hoisted lexical binding;
  the replacement operation therefore covers redeclarations as well as
  assignments. The decision does not relax invalidation: each declaration
  still replaces stale derived provenance in source order.
- `FactBuilder::with_limit` has one production caller, the lowering owner; the
  other callers are module-local tests using `new`. Narrow the production
  constructor to the lowering boundary and keep test construction as a
  test-only fixture, rather than preserving a broader internal API.

## Coverage

Reviewed all modules listed in Chunk 1 of `CODEBASE_STRUCTURE_CORE.md`:

- `analysis`, `analysis::facts`, `arguments`, `assignments`, `call_results`,
  `calls`, `calls::callee`, `calls::wrapper`, `control`, `functions`,
  `instance`, `interface`, `interface::commonjs`, `interface::exports`,
  `model`, `origin_map`, `pattern`, `state`, `stream`, and `visitor`.

Representative callers in lowering and the direct fact-construction tests
were also checked to validate ownership, lifecycle, and public-boundary claims.
