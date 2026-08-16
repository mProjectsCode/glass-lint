# Codebase Readability Audit

## Summary

Chunk 4 — scope collection subsystems (`analysis::scope::build/*`): the
`history` reversible-binding machinery (`AssignmentEnvironment`/`WriteSet`,
`Cursor`/`WriteCheckpoint`, `OwnedHistory`, `HistoryRestoreError`), the
plan→traversal split (`plan`/`traversal`/`visitor`/`shape`), pattern
normalization (`compact_pat`/`projection`/`bindings`), and the name/binding
helpers (`collector`/`constants`/`callbacks`/`provenance`/`program`/`freeze`).

The plan→traversal contract and the shape table are deliberate, tested, and
fail-closed: the non-positional `(parent, span_lo, kind)` keying is what lets a
diverged walker degrade via `ShapeMismatch`/`UnconsumedShape` instead of
corrupting scope identity (`tests_extended.rs`), so the interning is not
reported as speculative. The reversible-history family is split by genuine
delta families, and the `Cursor`/`WriteCheckpoint` family types are load
bearing; the concrete problems are (1) a seven-site repeated
charge−intern−exhausted triplet, (2) history identity-guard/error machinery
whose two failure causes are conflated and whose only consumers are the test
suite, (3) a two-level duplicate restore sequence, (4) CompactPat binding-name
collection re-implementing what the raw-`Pat` walk already does, (5) triplicated
`ScopeShapeKey` construction, and (6) two free helpers that mutate
`ScopeCollector`.

6 findings: READ-001 — READ-006.

## Findings

### Scope collection subsystems

#### [ ] READ-001 — The charge-intern-exhausted name-interning triplet is repeated seven times with no single owner

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/plan.rs:64-67`, `plan.rs:173-176`, `plan.rs:181-185`, `plan.rs:190-194`, `glass-lint-core/src/analysis/scope/build/collector.rs:96-99`, `collector.rs:102-107`, `glass-lint-core/src/analysis/scope/build/bindings.rs:34-39`

Every interning site repeats the same fail-closed invariant: charge the
semantic budget, attempt `names.intern`, and on error latch `name_exhausted`.
The sequence appears verbatim in the planner's name seeding loop (plan.rs:
64-67), `visit_ident` (173-176), `visit_member_expr` (181-185),
`visit_prop_name` (190-194), and in the collector's `intern_provenance_strings`
for both `StaticString` (96-99) and `StaticStringArray` (102-107);
`register_declaration_binding` (bindings.rs:34-39) embeds the same triplet with
a `return` instead of a `continue`. Because the invariant is re-stated at each
site, a future change to exhaustion handling (for example, recording an issue
instead of a flag) must be coordinated across two phases that share the same
`names`/`name_exhausted` / budget fields.

**Recommendation:** Add a shared helper in `build/bindings.rs` next to
`register_declaration_binding`, e.g.
`fn intern_checked(names: &mut NameTable, budget: &SemanticBudget, name_exhausted: &mut bool, name: &str) -> Option<NameId>`
that charges once, interns, latches the flag, and returns the id; route the
seven sites (and `register_declaration_binding` itself) through it. Guardrails:
charge exactly once per attempted intern, keep `name_exhausted` latched
(never reset), keep `lookup_or_intern_name` (collector.rs:118-123) distinct as
the read-first helper that does not charge, and preserve planner/collector
budget-allocation equivalence so artifact identity does not change.

**Fix Applied:** None so far.

#### [ ] READ-002 — Reversible-history identity guard and error type conflate two distinct failure causes that only the test suite can reach

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/build/history.rs:24-33`, `history.rs:36-40`, `history.rs:48-52`, `history.rs:55-58`, `history.rs:79-102`, `glass-lint-core/src/analysis/scope/build/assignments.rs:74-87`, `assignments.rs:231-254`, `glass-lint-core/src/analysis/scope/build/history/tests.rs:31-39`, `history/tests.rs:81-89`

`ForeignCheckpoint` (history.rs:37, undoc'd, unlike `StateDesync`) names two
materially different failure conditions: `target.owner != self.owner` (history.rs:
84-86, a truly foreign history instance) and `!reachable` (history.rs:95-97, a
position outside this same history's range). Both are structurally unreachable
in the pipeline — every `Cursor`/`WriteCheckpoint` is produced by the single
`AssignmentEnvironment`/`WriteSet` owned by the collector's one
`PathCollectionState`, and `StateDesync` is provably impossible for the
`WriteSet` family because `apply_write_inverse`/`apply_write_forward`
(history.rs:310-340) always return `true`. All three consumers
(`restore_checkpoint`, `ScopeCollector::restore` assignments.rs:231-248, and
`join_paths`) consume the `Result` as a bare boolean and record the same
`InvalidCheckpoint` issue, so the two-variant error type plus the global
`NEXT_HISTORY_OWNER` counter add a namespace and a `HistoryCheckpoint` wrapper
whose only discriminating evidence lives in the foreign-restore tests. This is
fail-closed machinery whose guard, meanwhile, is private and undocumented; if
it ever fires, the trigger becomes indistinguishable from a range check.

**Recommendation:** Keep the owner guard as a cheap, tested correctness net but
make it observable and honest: either split the enum so owner mismatch and
unreachable-position are distinct variants (or single-variant error with a
documented cause), give `ForeignCheckpoint` a contract doc matching
`StateDesync`'s, and document the "every checkpoint was minted by the single
path-owned history" invariant on `OwnedHistory`/`HistoryCheckpoint`. Do not
delete the owner (`history/tests.rs:31-39` and `81-89` encode the cross-family
rejection invariant), but if the guard is to remain, the `HistoryCheckpoint`
owner field and `cursor`-mint should move next to the restore logic it guards.
Guardrails: preserve fail-closed behavior (every `Err` still poisons the path
via `record_checkpoint_failure`), keep `Cursor`/`WriteCheckpoint` type-distinct
so the assignment family and write-set family cannot cross-restore, and keep
the foreign-history tests passing.

**Fix Applied:** None so far.

#### [ ] READ-003 — The two-part restore sequence is duplicated across path-state and collector levels

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/assignments.rs:74-87`, `assignments.rs:231-248`

`PathCollectionState::restore_checkpoint` (assignments.rs:74-87) and
`ScopeCollector::restore` (assignments.rs:231-248) perform the identical
two-restore sequence — `assignment_environment.restore(checkpoint.cursor)`
then `assignment_writes.restore(checkpoint.writes)`, treating any error the
same way — differing only in who records the failure: `restore_checkpoint`
sets `reachable = false` and returns `Err` for the caller to turn into an
issue, while `ScopeCollector::restore` calls `record_checkpoint_failure()` and
also restores `reachable` from the checkpoint on success. Every future member
of a checkpoint (a third reversible owner) must be added to both copies, and
the split forces readers to keep two spellings of the same rollback in mind.

**Recommendation:** Make `PathCollectionState::restore_checkpoint` the single
restore primitive (returning `Result<(), HistoryRestoreError>`), implement
`ScopeCollector::restore` as that primitive plus failure/`reachable` handling
(`self.assignment.path.restore_checkpoint(checkpoint)` → `record_checkpoint_failure`
on `Err`, `checkpoint.reachable` on `Ok`), and keep `join_paths` on the
internal primitive. Guardrails: preserve the exact reachability semantics —
`restore_checkpoint` must not touch `reachable` on `Ok` (join owns it
separately) while `ScopeCollector::restore` still restores it on success — and
keep `InvalidCheckpoint` issue recording exactly once per failure.

**Fix Applied:** None so far.

#### [ ] READ-004 — CompactPat binding-name collection re-implements, at one level removed, the raw-`Pat` name walk

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/scope/build/callbacks.rs:42-77`, `callbacks.rs:268-281`, `glass-lint-core/src/analysis/scope/build/bindings.rs:118-124`

`parameter_aliases` answers "which names does this parameter pattern bind" via
the free `collect_compact_binding_names` (callbacks.rs:268-281) plus a
caller-side `sort`/`dedup` in `parameter_binding_names` (callbacks.rs:72-78),
walking the `CompactPat` tree — the very domain operation
`compile/build::bindings::for_each_pat_binding` → `collect_pat_bindings`
(bindings.rs:118-124) already defines for raw `Pat`, and the same name-set is
recomputed once per parameter per call site. The name collection logic lives
in a free function while the type it interprets (`CompactPat`) is otherwise
established as the owning domain shape for callback projection.

**Recommendation:** Move binding-name derivation onto `CompactPat` as a method
(e.g. `CompactPat::binding_names(&self) -> Vec<SmolStr>` or a
`for_each_binding` iterator) that collects, sorts, and deduplicates once, and
call it from `parameter_aliases`/`parameter_binding_names`; delete
`collect_compact_binding_names`. Do not merge it with `collect_pat_bindings`,
whose raw-`Pat` walk sees computed keys and defaults that `CompactPat` has
already dropped; keep the stable sorted order that callback-alias determinism
relies on.

**Fix Applied:** None so far.

#### [ ] READ-005 — `ScopeShapeKey` construction is triplicated and `ScopeShape`'s getters exist only to feed it

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/shape.rs:54-59`, `shape.rs:66-80`, `shape.rs:82-95`, `shape.rs:97-116`

`ScopeShapeKey` is rebuilt field-by-field in `record` (shape.rs:67-71, from the
`ScopeShape` getters `parent()`/`span()`/`kind()`), in `take_child`
(shape.rs:88-93), and in test-only `remaining` (shape.rs:107-112), while the
four `ScopeShape` accessors (shape.rs:30-44) exist solely to project the same
tuple into that key. The structural-lookup contract — "a planned scope is
found by `(parent, span_lo, kind)`, never by position" — is stated implicitly
three times, which is exactly the invariant the shape table exists to enforce
per `tests_extended.rs:111-133`.

**Recommendation:** Give the key derivation one owner, e.g. `impl From<ScopeShape> for ScopeShapeKey` or a private `ScopeShapeTable::key_of(parent, span_lo, kind)`, and route `record`, `take_child`, and `remaining` through it; make the shape accessors private where plan.rs's `ScopeShape::new` is the only external constructor. Guardrails: preserve the `(parent, span_lo, kind)` identity and the `VecDeque` FIFO pop order that lets equal-key siblings resolve to distinct `ScopeId`s, keep `is_consumed`/`UnconsumedShape` behavior, and do not make the table positional.

**Fix Applied:** None so far.

#### [ ] READ-006 — Two free helpers mutate `ScopeCollector` beside an inherent sibling with the same role

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/build/visitor.rs:476-489`, `visitor.rs:649-670`, `visitor.rs:672-684`

`record_declaration_metadata` (visitor.rs:477-489) is an inherent method, but
the two operations it dispatches to — `collect_derived_function_pattern`
(visitor.rs:649-670) and `record_mutable_static_object` (visitor.rs:672-684) —
are free functions that take `&mut ScopeCollector` plus pattern/scope/declarator
arguments and mutate collector artifacts/bindings. The free-function split
adds an indirection layer without an owning vocabulary: readers must cross
from `&mut self` style to `collector.`-style to see the same declaration-metadata
recording, and the role boundary ("free function for genuine coordination",
per AGENTS.md) does not apply here since all state is the collector's own.

**Recommendation:** Move `collect_derived_function_pattern` and
`record_mutable_static_object` onto `ScopeCollector` as private inherent
methods beside `record_declaration_metadata`. Guardrails: preserve the exact
`function_prototype_builtin` + `is_unbound` recognition and the fail-closed
`name_path(&"Function".into())` behavior of the derived-constructor path, and
keep `record_mutable_static_object` guarded by `Pat::Ident` + `scoped_name`
success so only rootable mutable-object names are recorded.

**Fix Applied:** None so far.

## Systemic Themes

- **The plan→traversal split and shape-table "interning" are justified, not
  speculative.** The visitor resolves each planned scope structurally by
  `(parent, span_lo, kind)` rather than positionally, so a deliberately
  diverged walker fails closed (`tests_extended.rs:111-133`,
  `tests_extended.rs:136-184`) and sibling scopes with equal spans stay
  distinct across parents (`tests_extended.rs:65-108`). `freeze.rs:13-16`
  turns any unconsumed shape into `UnconsumedShape`. Any refactor of `shape.rs`
  must preserve this non-positional fail-closed contract.
- **The reversible-history family is split by genuine delta families, and the
  checkpoint types are not over-engineered.** `Cursor` (assignment
  provenance) vs `WriteCheckpoint` (write set) are opaque per-family cursors
  whose type separation prevents cross-restore; `CollectorCheckpoint` /
  `FunctionCheckpoint` are the legitimate combined cursors consumed by
  control-flow and function exits. The only over-machined slice is the
  identity-guard/error layer, captured in READ-002. The `WriteSet` generation
  tag is a plain `u64` with a single owner; a newtype was considered and
  rejected as not carrying meaning.
- **`CompactPat` does not duplicate `ScopeExpression` or the facts
  `PatternLeaf`.** `ScopeExpression` (scope/expression.rs) normalizes `Expr`;
  `CompactPat` normalizes `Pat`; `facts::pattern::walk_pattern` normalizes
  `Pat` into value/path `PatternLeaf`s against a different layer's resolver.
  Three pattern walkers exist in the crate (syntax `collect_pat_bindings`,
  build `compact_pat`, facts `pattern_values`), but they keep deliberately
  different precision (names only vs compact object shape vs
  path/default/rest/value); consolidation would collapse distinct lifecycles,
  so it is left as an Open Question rather than a finding.
- **Budget-charge discipline is caller-scattered.** `lookup_or_intern_name`
  (collector.rs:118-123) interns without charging, while every other interning
  site charges first (see READ-001); `name_path`/`append_name_path`
  (collector.rs:125-142) rely on the planner having pre-interned the
  identifiers/member properties they resolve. Observationally benign, but the
  invariant "every interning operation charges the shared budget"
  (mod.rs:189) is enforced by convention, not by an owner.
- **The `enter_*`/`exit_*` control-flow hooks structurally repeat one shape**
  (checkpoint → push frame → `assignment_writes.clear()` → depth bump, and
  pop frame → assemble path vec → `join_paths`). The branches differ in
  fields that matter (`guaranteed`, `breaks`/`continues`, handler/finally
  conditionality), so a shared reducer would trade clarity for lines; left as
  coordination, not reported.
- **`current_scope` gating on `artifacts.has_issues()`** (visitor.rs:71-77)
  turns any first collection issue into a cascade of rejected scopes and
  further issue records. It is reachable, bounded, and fail-closed, and
  changing it is a behavioral decision — recorded as an Open Question.
- History restore error handling at all call sites is by-design boolean;
  distinct starification is unwarranted today.
- `super::` intra-build imports remain pervasive in this chunk (`visitor.rs:19-27`,
  `assignments.rs:24`, `callbacks.rs:15`, `collector.rs:11`) despite AGENTS.md
  preferring `crate::`; cosmetic, consolidated only incidentally.
- Test-only members (`scope_lookups`, `ScopeShapeTable::recorded`,
  `remaining`, `shapes_len`) live in production structs behind `#[cfg(test)]`;
  accepted, not dead code.

## Open Questions

- **Are the three pattern walkers intentionally divergent?** syntax
  `collect_pat_bindings` (names, raw `Pat`), build `compact_pat` (compact
  object shape for callback projection), and `facts::pattern::walk_pattern`
  (path-contained value `PatternLeaf`s). A shared syntax-owned pattern descent
  could serve all three, but each consumer's precision, fail-closed policy, and
  resolver differ; confirm before consolidating across the build/facts module
  boundary.
- **Is the uncharged `intern` inside `lookup_or_intern_name` (collector.rs:
  118-123) deliberate?** The planner's pass already interns and charges every
  identifier, member name, and property name, so collection-time misses should
  be rare; if exhausted-name accounting must be airtight, either route those
  intern arms through READ-001's helper or document why the planner absorbs
  the charge.
- **Should `current_scope` keep silencing on any prior issue (visitor.rs:
  71-77)?** It is the safety switch that makes later scopes `Rejected` and
  paints subsequent issues; it works, but the cascade means the issues list
  over-reports. Intended behavior should be confirmed before anyone
  "simplifies" it, and READ-003's consolidation must not disturb it.
- **Cross-chunk dependency:** this chunk's `freeze.rs` still drives the
  `FrozenScopeCollectionArtifacts`/`seal()` boundary and `BindingIndex`
  degrade flagged by Chunk 3 as READ-002/READ-003; those findings are not
  re-reported here, and their fix will touch `lexical.scope_shapes` and
  `assignment.assignments` consumers cited in this audit.

## Coverage

Files reviewed for this chunk (Chunk 4, scope collection subsystems):

- `glass-lint-core/src/analysis/scope/build/history.rs` (all reversible types
  and apply functions) and `build/history/tests.rs`
- `glass-lint-core/src/analysis/scope/build/assignments.rs`
- `glass-lint-core/src/analysis/scope/build/bindings.rs`
- `glass-lint-core/src/analysis/scope/build/callbacks.rs`
- `glass-lint-core/src/analysis/scope/build/collector.rs`
- `glass-lint-core/src/analysis/scope/build/compact_pat.rs`
- `glass-lint-core/src/analysis/scope/build/constants.rs`
- `glass-lint-core/src/analysis/scope/build/freeze.rs`
- `glass-lint-core/src/analysis/scope/build/plan.rs`
- `glass-lint-core/src/analysis/scope/build/program.rs`
- `glass-lint-core/src/analysis/scope/build/projection.rs`
- `glass-lint-core/src/analysis/scope/build/provenance.rs`
- `glass-lint-core/src/analysis/scope/build/shape.rs`
- `glass-lint-core/src/analysis/scope/build/traversal.rs`
- `glass-lint-core/src/analysis/scope/build/visitor.rs`
- `glass-lint-core/src/analysis/scope/build/mod.rs`, `build/tests.rs`,
  `build/tests_extended.rs`
- Cross-references traced: `glass-lint-datastructures/src/history.rs`
  (`ParentLinkedHistory`/`HistoryCursor`/`HistoryTransition` semantics),
  `glass-lint-core/src/analysis/scope/expression.rs`
  (`ScopeExpression`/`normalize_scope_expression`),
  `glass-lint-core/src/analysis/facts/pattern.rs` (`PatternLeaf` walker),
  `glass-lint-core/src/analysis/scope/binding_index.rs` and
  `build/aliases.rs` (consumers of `freeze.rs`/`projection.rs` outputs),
  `scope/query/rooted.rs` (`rooted_expr_chain_with`).

Read-only audit; no source files were modified.