# Codebase Readability Audit — glass-lint-core Chunk 23: Lint execution and reporting

## Summary

Chunk 23 owns rule-selection validation (`lint/selection`), catalog composition
(`lint/catalog`), linter construction and batch execution (`lint/linter`,
`lint/batch`), and report assembly (`lint/report` plus `diagnostics`, `evidence`,
`files`, `summary`). The core pipeline is well-factored: rule selection is
evaluated once against a combined catalog, prepared selections bind their own
catalog, batch execution preserves input order with bounded in-flight work, and
report assembly is a clean link → match → render → summarize phase machine with
fail-closed completeness tracking.

The concrete problems are concentrated in the *public API surface* and in small
internal duplication. Several public symbols are exported but never consumed
(`LinterConfig::selection()`, `RuleSelection::validate`, and a `selection`
field that exists only to feed them); `ProjectReportAssembler` is exported yet
uncallable from outside the crate because its parameters reference types behind
private modules. `LintConfigError` merges two error domains and contains an
unreachable, misleading error mapping. The report/batch internals repeat a few
small shapes (parallel evidence-group structs, identical worker-panic error
construction, a duplicated fallback evidence step).

## Findings

### Linter construction and configuration surface

#### [ ] READ-001 — Dead public selection surface: `LinterConfig::selection()`, the retained `PreparedRuleSelection::selection`, and `RuleSelection::validate`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/lint/linter.rs:90-95`, `glass-lint-core/src/lint/selection.rs:266-275`, `glass-lint-core/src/lint/selection.rs:311-313`

`LinterConfig::selection()` (public, `linter.rs:90-95`) has no callers anywhere
in the workspace; its only internal reference is `PreparedRuleSelection::selection()`
(`selection.rs:273-275`), which reads the `selection` field
(`selection.rs:269`) that `PreparedRuleSelection` otherwise drops in
`into_parts()` (`selection.rs:277-280`). `RuleSelection::validate()`
(`selection.rs:311-313`) is likewise public with no callers; `prepare()` and the
`pub(crate)` `resolve()` are the used entry points. This is three symbols (one
field + two methods) of exported API and stored state that nothing exercises,
which bloats the public surface a `public_surface`-style guard does not check
(it only covers engine storage). Deleting them changes no caller.

**Recommendation:** Delete `LinterConfig::selection()`, drop the
`PreparedRuleSelection::selection` field and its `selection()` accessor (leaving
`into_parts()` as the sole way out), and delete `RuleSelection::validate()`,
keeping `prepare()`, `resolve()`, `baseline()`, and `overrides()`. Guardrails:
the CLI already clones `PreparedRuleSelection` into its own `PreparedConfig`
(`glass-lint-cli/src/config.rs:286-293`), so nothing depends on the field; keep
`prepare()`/`resolve()` since `Linter::new` and CLI validation both use them.

**Fix Applied:** None so far.

#### [ ] READ-002 — `ProjectReportAssembler` is exported publicly but uncallable outside the crate

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/report/mod.rs:135-186`, `glass-lint-core/src/lint/mod.rs:16`, `glass-lint-core/src/lib.rs:40-41`

`ProjectReportAssembler` is re-exported from `lib.rs:40-41` and `lint/mod.rs:16`,
but its constructor `link()` takes `ResolvedLinkInput` and returns via
`ProjectSemanticModel` (`analysis/project/model.rs:132,233`) — types behind the
private `mod analysis` (`lib.rs:15`) — and `assemble()` takes `&[RuleIndex]`
(`api/classification.rs`), also behind the private `mod api` (`lib.rs:16`).
External crates therefore cannot name the parameter types, and no external
caller exists (the only caller is `ProjectSession::finish` at
`glass-lint-core/src/project/session/mod.rs:438-448`). The chunk contract lists
`ProjectReportAssembler` as a public type, but as exported it is unusable dead
surface; only `ProjectAnalysis`/`ProjectAnalysisTimings` are consumed outside
the crate (`glass-lint-project/src/loader.rs:415`).

**Recommendation:** Make `ProjectReportAssembler` `pub(crate)` (its `assemble`
return type `ProjectAnalysis` stays public), so report-phase types live behind
the owning module boundary. Guardrails: `ProjectSession::finish` must keep
returning `ProjectAnalysis`; the link → assemble phase machine and its
`ProjectReportSession` state holder must not be collapsed, since they mark a
real lifecycle transition.

**Fix Applied:** None so far.

#### [ ] READ-003 — `LintConfigError` mixes catalog and selection error domains; the `InvalidRuleId` mapping arm is unreachable and misleading

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Conversion
- **Location:** `glass-lint-core/src/lint/selection.rs:387-398`, `glass-lint-core/src/lint/linter.rs:161-167`, `glass-lint-core/src/lint/catalog.rs:14-23`

`LintConfigError` (documented as "configuration failure when selecting rules",
`selection.rs:387`) carries both selection-domain variants (`UnknownRule`,
`InvalidSelector`, produced only at `selection.rs:69,75,201,206,373-379`) and
catalog-domain variants (`DuplicateRule`, `InvalidRule`) that are produced only
by the `ProviderCatalogError → LintConfigError` conversion in `Linter::new`
(`linter.rs:163-165`). The fourth mapping arm,
`ProviderCatalogError::InvalidRuleId(id) => LintConfigError::InvalidSelector(id)`
(`linter.rs:166`), is unreachable — `combine` (`catalog.rs:132-147`) can only
yield `DuplicateRule`, while `InvalidRuleId` is produced only by
`RuleCatalog::new` (`catalog.rs:105-107`) — and even if reached it would label a
catalog/provider-naming failure as a selector failure. One error type thus
collapses two distinct failure domains and one mapping silently mislabels a
never-occurring case.

**Recommendation:** Have `Linter::new` return a composed or two-domain error, or
remove the unreachable `InvalidRuleId` arm (and document why catalog errors are
re-hosted). Guardrails: CLI surfaces the error through
`anyhow::anyhow!(error)` (`glass-lint-cli/src/config.rs:389`) and tests match
`LintConfigError::UnknownRule` (`lint/linter/tests.rs:97`); those display and
match behavior must be preserved, and provider-boundary validation must stay out
of core rule policy.

**Fix Applied:** None so far.

### Batch execution

#### [ ] READ-004 — Repeated `WorkerPanic` error construction and near-duplicate pending-failure synthesis

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/batch.rs:162-178`, `glass-lint-core/src/lint/batch.rs:272-277`

The same error value
`ProjectError::Execution(ProjectExecutionError::Local(LocalExecutionError::WorkerPanic))`
is constructed three times in production code — in `fail_protocol`
(`batch.rs:164-166`), `synthesize_missing` (`batch.rs:173-175`), and the
`catch_unwind` fallback in `refill` (`batch.rs:272-277`) — and again in tests
(`batch/tests.rs:53-59,70-79,130-140`). `fail_protocol` and `synthesize_missing`
are near-identical loops over pending entries; only their overwrite policy
differs (fail_protocol replaces all results, synthesize fills only missing
ones). A single named constructor would keep the failure vocabulary in one
place and remove the risk of the four sites drifting.

**Recommendation:** Add a small `worker_panic()` helper (or a constructor on the
local execution error) and use it at all four sites; keep `fail_protocol` and
`synthesize_missing` as separate methods because their overwrite semantics are
genuinely different (a broken protocol invalidates received results, a closed
channel does not). Guardrails: the distinct outcomes (all-failed vs only-missing
failed) must remain distinct, and the input-order guarantee of `take_ready` must
not change.

**Fix Applied:** None so far.

### Report evidence assembly

#### [ ] READ-005 — `EvidenceRangeEntry` and `FindingGroup` are identical-shape parallel structs

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:28-38`, `glass-lint-core/src/lint/report/evidence.rs:297-320`

`EvidenceRangeEntry` and `FindingGroup` both hold exactly
`{ range: SourceRange, occurrences: Vec<ResolvedEvidenceOccurrence> }`
(`evidence.rs:29-38`). `FindingRangeBuilder::into_groups` builds groups by
copying the occurrences out of each retained entry via
`FindingGroup::add_entry` (`evidence.rs:48-52, 297-320`), so the shape is
declared twice and the same data is copied once more. The two types are at
different lifecycle stages (sorted leaf entries vs. merged finding ranges), but
the parallel fields and the copy bridge add no new vocabulary.

**Recommendation:** Collapse the two into one range+occurrences struct used by
both stages, or make `into_groups` move `EvidenceRangeEntry` values into group
occurrences so no field set is declared twice. Guardrails: the retained-range
selection performed by `retained_indices` (`evidence.rs:323-348`) and the
range-containment merge rule (`FindingGroup::add_entry`, `evidence.rs:49-52`)
must keep their exact ordering and containment semantics.

**Fix Applied:** None so far.

#### [ ] READ-006 — Inline fallback occurrence step duplicates `EvidenceTraces::fallback`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:85-93`, `glass-lint-core/src/project/types/report/evidence.rs:168-180`

When no resolved traces remain, `FindingGroup::into_evidence` synthesizes a
single-step trace with role `EvidenceRole::Occurrence` and message
`"evidence occurrence"` (`evidence.rs:85-93`) — the exact step that
`EvidenceTraces::fallback` already builds (`evidence.rs:171-180`, same role and
message). The report layer re-implements a vocabulary the owning type already
owns, so the fallback step's shape and message text now live in two places.

**Recommendation:** Extract the single-step occurrence-trace construction to one
owner (e.g., reuse `EvidenceTraces::fallback` when no truncation flag needs to
be preserved, or a shared one-step `EvidenceTrace` constructor) and call it from
`into_evidence`. Guardrails: the merged `truncated` flag must still be preserved
when a fallback trace is added next to real traces, and trace dedup/order
determinism (`BTreeSet` insert, `evidence.rs:60-83`) must be kept.

**Fix Applied:** None so far.

### Batch options

#### [ ] READ-007 — `Linter::lint_batch` re-queries host parallelism already resolved by `BatchOptions::default`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/lint/linter.rs:257`, `glass-lint-core/src/lint/batch.rs:55-59`

`BatchOptions::default()` calls `available_parallelism()` (`batch.rs:57`), then
`Linter::lint_batch` calls it again (`linter.rs:257`) to clamp `worker_count`.
For the default flow the system is queried twice for the same value; the second
query is only meaningful when a caller explicitly set `workers()` above the
machine's parallelism. The duplicate system call is unnecessary work on every
default batch.

**Recommendation:** Resolve the clamp once — e.g., resolve the effective worker
count inside `BatchOptions` (or accept the availability once and thread it into
`lint_batch`) so `available_parallelism` is queried at most once per batch.
Guardrails: the clamping rule (workers ≤ available, ≤ max_in_flight, ≥ 1) and
the dedicated-per-batch Rayon pool semantics in `lint_batch`
(`linter.rs:257-268`) must be preserved.

**Fix Applied:** None so far.

## Systemic Themes

- **Unexercised public surface.** Multiple exported symbols on the selection /
  linter-config types and on the report phase machine have zero callers, and a
  `public_surface` guard only covers engine storage. The public API surface of
  this chunk should be audited so that "exported" implies "consumable and
  consumed" (READ-001, READ-002).
- **Error-domain merging across the catalog/selection boundary.** `LintConfigError`
  re-hosts catalog-composition failures, and one conversion arm mislabels an
  unreachable case; the two providers of error variants should stay distinct or
  the merge should be explicit and total (READ-003).
- **Small shape repetition in internal report/batch state.** Parallel
  evidence-group structs, repeated worker-panic error construction, and a
  duplicated fallback step show the same root cause: internal helpers rebuild
  a value the owning type already provides (READ-004, READ-005, READ-006).

## Open Questions

- `ProjectReportAssembler` is exported publicly but unusable externally — was it
  exported for a planned harness/profiling consumer? If so, that consumer should
  be added or the export narrowed to `pub(crate)` (READ-002).
- `RuleSelection::validate` (public) and `resolve` (`pub(crate)`) both evaluate
  a selection; if `validate` was intended as the public dry-run API, the
  `pub(crate)` `resolve` in `Linter::new` (`linter.rs:168`) may instead be the
  one to remove (READ-001).
- `LinterConfig::selection()` was possibly intended for future CLI/output
  introspection; no current consumer exists, so its deletion is only safe if no
  such consumer is planned.

## Coverage

Reviewed `glass-lint-core/src/lint/mod.rs` (exports), `lint/linter.rs` and
`lint/linter/tests.rs`, `lint/batch.rs` and `lint/batch/tests.rs`,
`lint/catalog.rs` and `lint/catalog/tests.rs`, `lint/selection.rs` and
`lint/selection/tests.rs`, and `lint/report/mod.rs`, `report/diagnostics.rs`,
`report/evidence.rs` and `report/evidence/tests.rs`, `report/files.rs` and
`report/files/tests.rs`, `report/summary.rs`. Callers traced across
`glass-lint-core/src/project/session/mod.rs`, `project/tables.rs`,
`project/types/report/evidence.rs`, `api/classification.rs`,
`analysis/semantic/status.rs`, `analysis/project/model.rs`,
`glass-lint-cli/src/config.rs`, and `glass-lint-harness/src/builtins.rs`.
No source files were modified.
