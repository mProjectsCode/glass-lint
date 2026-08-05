# Codebase Readability Audit — Chunk 14

## Summary

Chunk 14 covers scope planning and collection state, assignment-history
checkpoints, syntax constant evaluation/provenance, and trace types. The
scope pipeline has a useful declaration-plan/source-order split, bounded
constant evaluation, explicit unsupported projection errors, and reversible
path-local mutation logs. Trace ownership and scope-graph validity findings
were already recorded in earlier chunks and are not repeated here.

The new issues are concentrated in internal state ownership: scope-shape
records retain a production vector used only by tests, binding-index freezing
silently discards entries whose scope cannot be mapped to a function, inline
callback bindings live for the whole collector after consumption, and history
checkpoints have no owner identity and convert foreign-cursor failure into a
panic.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Scope-shape storage

#### [x] READ-069 — Remove production scope-shape storage used only by tests

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Dead state / Memory ownership / Test seam
- **Location:** `glass-lint-core/src/analysis/scope/build/shape.rs:47-75,93-117`

`ScopeShapeTable` stores every `ScopeShape` in `shapes` and also stores each
shape’s `ScopeId` in the keyed `children` queues. Production operations use
the queues for `take_child` and `is_consumed`; the only consumer of `shapes`
is the `#[cfg(test)] shapes_len` helper. Scope planning therefore retains a
second vector proportional to every lexical scope solely to support test
introspection, while the collector and freeze path already have the scope
collection itself.

Remove the duplicate vector and keep a test-only count, or make the count an
explicit invariant counter that does not retain full `ScopeShape` values.
Preserve keyed child consumption, unconsumed-shape detection, deterministic
planner/collector matching, and the existing shape-mismatch status behavior.

**Fix Applied:** Replaced the production `Vec<ScopeShape>` with a scalar
recorded-shape count. Child queues remain the sole production lookup state,
while the test count helper retains its introspection contract without
retaining duplicate shape values.

**Verification:** `cargo test -p glass-lint-core analysis::scope::build --lib`
(30 passed) and `make fmt && make ci` (passed).

### Binding-index freeze

#### [x] READ-070 — Validate binding-index inputs instead of silently dropping them

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Invariant validation / Fail-closed state
- **Location:** `glass-lint-core/src/analysis/scope/binding_index.rs:28-95`,
  `analysis/scope/build/freeze.rs:22-36`

`BindingIndexInput` contains correlated assignments, scope-to-binding IDs,
function scopes, aliases, and parameter aliases. `BindingIndex::from` converts
scope references into `FunctionId`s with `filter_map`, silently dropping every
function binding, function alias, or parameter alias whose scope has no
function ID. The freeze transition records no issue for discarded entries, so
a malformed or partially exhausted scope snapshot can produce a valid-looking
frozen index with missing callback/function identity rather than an explicit
incomplete result.

Give the input a fallible validation/normalization owner that reports an
invalid scope-to-function relationship, or have freeze record a dedicated
scope-index issue for every rejected entry. Preserve intentional omission of
non-function scopes, deterministic ID allocation, parameter-alias conflict
handling, and conservative failure of unsupported identity; delete the silent
`filter_map` drops after callers handle the validation result.

**Fix Applied:** Replaced the silent `filter_map` conversions with a fallible
`BindingIndex::try_from` that rejects missing function IDs for function
bindings, aliases, and parameter aliases. Freeze records
`InvalidBindingIndex`, uses an empty conservative index, and carries the
invalidity into frozen scope fallback.

**Verification:** `cargo test -p glass-lint-core analysis::scope::build --lib`
(30 passed) and `cargo test -p glass-lint-core analysis::scope::query --lib`;
`make fmt && make ci` (all passed).

### Inline callback state

#### [x] READ-071 — Retire inline callback bindings after function entry

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Lifecycle / Bounded state / Memory retention
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:197-201`,
  `analysis/scope/build/callbacks.rs:135-153`,
  `analysis/scope/build/visitor.rs:303-336`

`ScopeCollector::bind_inline_parameters` stores callback bindings in
`inline_parameters` keyed by `BytePos`. `after_function` and `after_arrow`
clone the map with `get(...).cloned()` and install its assignments, but never
remove the consumed entry. The collector consequently retains every callback’s
provenance map until the whole source pass finishes, even though each span is
entered once and the bindings are no longer needed after parameter setup.
Large callback-heavy files can therefore keep a second copy of path-local
provenance alongside the assignment history and final assignments.

Use `remove` at the consuming function/arrow hook and handle the owned map
directly. Preserve callback span matching, nested callback setup, unsupported
argument omission, assignment-version ordering, and the rule that only
unambiguous callback arguments become parameter aliases.

**Fix Applied:** Function and arrow entry now remove consumed inline callback
bindings from the collector instead of cloning them through a retained map
entry, releasing the path-local provenance immediately after installation.

**Verification:** `cargo test -p glass-lint-core analysis::scope --lib`
(33 passed) and `make fmt && make ci` (passed).

### Assignment-history checkpoints

#### [x] READ-072 — Make assignment checkpoints owner-safe and non-panicking

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / State ownership / Error handling
- **Location:** `glass-lint-core/src/analysis/scope/build/history.rs:22-24,102-120,162-228`,
  `glass-lint-core/src/analysis/scope/build/mod.rs:245-260`

`Cursor` and `WriteCheckpoint` wrap only the underlying
`HistoryCursor`; they do not identify the `AssignmentEnvironment` or
`WriteSet` that created them. `restore` delegates to
`ParentLinkedHistory::transition` and panics when the cursor belongs to a
different history or otherwise cannot be reached. The checkpoint types are
internal, but they are passed through several control-flow frames, making a
future frame mix-up or refactor failure terminate analysis instead of producing
an explicit unsupported/incomplete scope result. This is the same ownership
invariant the surrounding scope model otherwise represents conservatively.

Give checkpoints an owner/generation token and return a typed restore failure,
or make the history owner perform restoration so foreign cursors cannot be
passed. Convert failure into a scope-collection issue and preserve the current
LCA rollback/redo behavior, O(1) checkpoint creation, branch joins, and
deterministic assignment versions; remove the direct panic paths after the
caller handles the error.

**Fix Applied:** Added per-history owner tokens to assignment and write
checkpoints, made restore return a typed `HistoryRestoreError`, and routed
collector restore failures into the conservative `InvalidCheckpoint` scope
collection issue. Direct join-path restores now use the same failure path;
foreign checkpoint tests cover both history owners.

**Verification:** `cargo test -p glass-lint-core analysis::scope::build --lib`
passes (32 tests), and `make fmt && make ci` passes, including the full
workspace, end-to-end, rule, doctest, and example checks.

## Systemic Themes

Chunk 14’s scope collector has strong explicit lifecycle phases, but some
state transitions remain storage- or caller-shaped: test-only scope records
are retained in production, correlated binding inputs can be discarded by a
conversion, callback state outlives its consumption point, and checkpoint
ownership is implicit. These are good candidates for small owner methods and
fallible transitions rather than additional traversal or semantic models.

The bounded constant evaluator and syntax provenance enums were reviewed for
their budget, unknown, and unsupported contracts. Trace arena identity and
parent validation, plus scope-shape validity propagation, were intentionally
left to the existing Chunk 5–6 findings. READ-070 and READ-071 are marked
applied above.

## Open Questions

- A rejected binding-index entry may warrant one aggregate scope-index issue
  rather than one diagnostic per dropped map entry; the key requirement is
  that freeze cannot silently claim a complete index.
- If checkpoint failure is considered an internal bug rather than malformed
  analysis input, the owner-safe token should still preserve a debug assertion
  without making production analysis panic.
- The next unreviewed handoff is Chunk 15: rule authoring and query compiler
  modules.

## Coverage

Reviewed the Chunk 14 types listed in `CODEBASE_STRUCTURE_CORE.md` across
binding indexes, scope planning/shape matching, collector checkpoints and
control-flow frames, assignment history, destructuring projection, bounded
constant evaluation, syntax provenance, and trace model ownership.
Representative callers were traced through two-pass scope collection, freeze
into the immutable graph, callback parameter projection, constant lookup,
lowering status propagation, and evidence construction. Existing Chunk 5–6
findings were checked to avoid re-reporting scope-graph construction and
validity loss, contextual property-name APIs, and trace arena/parent identity.
No source, test, configuration, dependency, or documentation changes were
made.
