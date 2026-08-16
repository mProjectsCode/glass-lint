# Codebase Readability Audit — glass-lint-core Chunk 23: Lint execution and reporting

## Summary

Chunk 23 owns the lint-execution and reporting layer of `glass-lint-core`:
`lint/mod.rs` (public re-exports), `lint/batch.rs` (bounded input-ordered
batch driver), `lint/catalog.rs` (`RuleCatalog` + catalog/compiler error
surface), `lint/linter.rs` (`Linter`, `LinterConfig`, `LinterSharedConfig`,
`LinterRuleInputs`), `lint/selection.rs` (baseline/override/selector
evaluation), and `lint/report/{mod,diagnostics,evidence,files,summary}.rs`
(project report assembly).

Prior history from parallel chunk audits is respected: chunk 19 already
reports the per-rule name validation duplicated between `Rule::build` and
`RuleCatalog::new` (`RuleBuildError::InvalidId`/`from_provider_and_name`), and
chunks 20/21 already cover the compiler-side `RuleSelectionError`/
`CompiledRuleSelection` layer in `api/compiler`; none of that work is
re-reported here.

Overall the chunk is well-bounded: the batch machinery is genuinely private
(`BatchDriver`/`PendingBatch`/`CompletedBatch`/`CompletionError` are hidden
behind a four-type public surface), `LinterSharedConfig` is a justified
Arc-shared immutable config bucket, the report assembly is a clean
orchestrator (`ProjectReportAssembler`) over owned accumulators
(`ProjectReportSession`, `ReportFiles`) and single-responsibility renderers
(`FindingRenderer`, `attach_project_diagnostics`, `assemble_project_report`),
and the selector parser decomposition (`RuleSelector`/`RulePattern`/
`PatternSegment`) is cohesive and private. The problems concentrate in the
error surface (one dead variant plus a production `unreachable!`, a
parallel build error type translated by a hand-rolled mapper), one redundant
validation/parse path in the exact-id selector case, a double serde vocabulary
on `RuleState`, module-visibility overreach, and one mixed-level range→finding
conversion. Findings are ordered by the breadth of the change each implies.

## Findings

### Error surface and conversion (`lint/catalog`, `lint/linter`, `lint/selection`)

#### [ ] READ-001 — `LintConfigError::InvalidRule` is never constructed and its sibling `unreachable!` panics on a public path

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/lint/selection.rs:391-392`; `glass-lint-core/src/lint/linter.rs:154-157`

`LintConfigError::InvalidRule(RuleId, RuleCompilationError)`
(`selection.rs:392`) is dead: `rg` finds no construction anywhere in the
workspace. Compilation failures cannot surface at linter construction because
`RuleCatalog::new` compiles each catalog (`catalog.rs:118`), so the `Unprepared`
path in `Linter::new` only combines already-validated catalogs. Its sibling
mapping arm is a production `unreachable!` for the other `ProviderCatalogError`
variants (`linter.rs:156`), a panic that is currently masked only by a
structural invariant of `RuleCatalog::combine` (its only error is
`DuplicateRule`, `catalog.rs:140`). The declared three-way error split
(`CompiledCatalogError` → `ProviderCatalogError` → `LintConfigError`) therefore
advertises a compiler-failure vocabulary on the lint-selection boundary that
the flow cannot reach, and the comment claims the panic is unreachable while
still producing a panic on a supported public construction path.

**Recommendation:** Delete `LintConfigError::InvalidRule` (and its doc),
removing the unreachable compiler re-host from `lint/selection` entirely, and
narrow `RuleCatalog::combine` to return its only possible failure directly
(e.g. `Result<Self, RuleId>` for the duplicate id) so
`Linter::new` becomes `RuleCatalog::combine(catalogs).map_err(LintConfigError::DuplicateRule)?`
with no match on `ProviderCatalogError` and no panic arm; remove
`ProviderCatalogError::DuplicateRule` and its Display arm at the same time.
Guardrails: `DuplicateRule`, `UnknownRule`, and `InvalidSelector` retain their
messages and ordering on `LintConfigError`; `RuleCatalog::new`'s
`InvalidRuleId`/`InvalidRule` errors keep flowing through `ProviderCatalogError`;
integration tests that assert `LintConfigError::UnknownRule`
(`tests/integration/linter.rs:208,248`) stay unchanged.

#### [ ] READ-002 — `map_compiled_catalog_error` hand-translates `CompiledCatalogError` when a canonical `From` would delete the mapper and the redundant rebuild

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/lint/catalog.rs:49-77,118`

`map_compiled_catalog_error` is a free function that destructures every
`CompiledCatalogError` variant, lifts the `rule_id` out of each payload, and
rebuilds it as `ProviderCatalogError::InvalidRule(rule_id, RuleCompilationError)`
(`catalog.rs:51-76`), used once at `compile_records(...).map_err(map_compiled_catalog_error)`
(`catalog.rs:118`). The rebuild is the same destructure/reassemble round trip
the selection layer otherwise avoids, and `RuleCompilationError`
(`catalog.rs:27-36`) is a full parallel model of the four `CompiledCatalogError`
variant kinds (`InvalidMatcher`/`InvalidQuery`/`CompilerInvariant`/
`InvalidPhysicalPlan`) that also discards the structured
`CompilerInvariantDiagnostic`/`PhysicalPlanDiagnostic` payloads into `String`
and drops the variant context from `Display` (all four arms write only the
message, `catalog.rs:44`).

**Recommendation:** Give the conversion its canonical owner: implement
`impl From<CompiledCatalogError> for ProviderCatalogError` in `lint/catalog`
and change `RuleCatalog::new` to `compile_records(&rules_and_ids).map_err(ProviderCatalogError::from)?`,
deleting the free function. Guardrails: keep the `stderr`-visible message text
stable (provider-prefixed `rule \`{id}\`: ...`), keep the flat
`RuleCompilationError` vocabulary as the stable provider-boundary shape (it is
the public type callers match on), and where the structured diagnostics matter
for future serialization, keep the question open (see Open Questions) rather
than re-adding field access mid-refactor.

### Selector parsing (`lint/selection`)

#### [ ] READ-003 — exact-id selectors build a `RulePattern` that is never consumed and re-validate the same string under a second grammar

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/selection.rs:195-213,219-229`; `selection.rs:100-123`

`RuleSelector::parse` always builds the full parsed `RulePattern`
(`selection.rs:203`), but `RuleSelector::matches` short-circuits on
exact (no-wildcard) selectors to `id == self.raw` (`selection.rs:225-227`), so
the constructed segment list is dead weight for the common exact override
case while also being validated a second, independent time by
`RuleId::parse(selector.clone())` (`selection.rs:205-206`) under the canonical
rule-ID policy. The two grammars (`valid_pattern_part`, `selection.rs:126-138`,
vs `RuleId::parse`, `rule_id.rs:43`) can drift apart for exact IDs, and the
`parse` call's `ProviderCatalogError` is always erased into
`LintConfigError::InvalidSelector`, hiding the canonical rule-ID error. This is
a separate instance of the same "rule-ID policy enforced in multiple places"
theme chunk 19 reports for the authoring side (`Rule::build` vs
`RuleCatalog::new`).

**Recommendation:** Detect `*` presence before parsing: for selectors without a
wildcard, validate with `RuleId::parse` only and store no pattern; for wildcard
selectors, build `RulePattern` (whose grammar already handles the literal
checks) and skip the `RuleId::parse` pass. Keep `RuleSelector::matches`'s
exact branch as the sole matching path for non-wildcards. Guardrails: preserve
the `InvalidSelector` failure mode for malformed wildcards and the empty
selector, keep the `?[]{}\` rejection in `RuleSelector::parse`
(`selection.rs:196-199`), and preserve the deterministic `UnknownRule` vs
`InvalidSelector` distinction in `validate_override_matches`
(`selection.rs:358-375`).

#### [ ] READ-004 — `RuleState` declares a lowercase-string serde vocabulary that no site uses; only the bool shim is reachable

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/selection.rs:30-36,44-47,161-185`

`RuleState` derives serde with `rename_all = "lowercase"`
(`selection.rs:32`), yet its only serialization site — `RuleOverride`'s
`state` field — bypasses that representation entirely with
`#[serde(rename = "enabled", with = "rule_state_as_bool")]`
(`selection.rs:44-47`), so the derived vocabulary is unreachable and the two
grammars ("enabled"/"disabled" vs `true`/`false`) must be held in sync by hand.
`rule_state_as_bool` (`selection.rs:161-185`) is a private inline `mod`
referenced only through a serde attribute whose `pub(super)` functions are also
broader than their single consumer needs. The two-variant enum exists to give
the boolean meaning, which is good vocabulary, but the model currently asserts
two incompatible serializations for one field.

**Recommendation:** Let `RuleOverride` own its serialization outright: either
implement `Serialize`/`Deserialize` for `RuleOverride` (keep field name
`enabled` and the `deserialize_selector` validation) and delete the
`RuleState` derive plus `rule_state_as_bool`, or convert the field to a plain
`enabled: bool` and keep `RuleState` only as a conversion API. Choose one
canonical wire shape (the CLI schema currently expects `enabled: true|false`).
Guardrails: config round-trips in `glass-lint-cli/src/config.rs` and the
`CoreConfig.overrides` file format must keep serializing booleans, selector
validation must still reject invalid selectors at deserialize time, and the
`RuleOverride::new`/`state()` public surface stays intact for linter tests
(`linter/tests.rs:89`).

### Module visibility (`lint/batch`, `lint/report`)

#### [ ] READ-005 — `pub(super)` items used only inside their own module subtree leak into the whole `lint` module

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/batch.rs:111,245`; `glass-lint-core/src/lint/report/mod.rs:111,120,124,130`

Several items are declared `pub(super)` even though every consumer lives in the
declaring module or its child modules, where plain privacy already suffices:
`CompletedBatch` (`batch.rs:111`, referenced only inside `batch.rs` and
`batch/tests.rs`), `BatchDriver::new` (`batch.rs:245`, used only by
`batch/tests.rs` via `use super::*`), and `ProjectReportSession`'s
`status_diagnostics`/`is_complete`/`reconstruct_trace`/`trace_node_count`
(`report/mod.rs:111,120,124,130`, consumed only by the sibling modules
`diagnostics.rs`, `evidence.rs`, and `summary.rs`). As written, `pub(super)`
makes these reachable from the entire `crate::lint` subtree (`linter.rs`,
`catalog.rs`, `selection.rs`), widening the surface with no consumer.

**Recommendation:** Drop the `pub(super)` qualifier on these items so they are
module-private; unit-test modules keep working because they are child modules
and import via `use super::*`. Keep the qualifier where a real cross-module
call exists today (`BatchResults::new` is consumed by `linter.rs` and must stay
`pub(super)`). Guardrails: no behavior change; the batch protocol types remain
crate-internal, and `report`'s public surface stays just
`ProjectAnalysis`/`ProjectAnalysisTimings` (re-exported at `lint/mod.rs:16`).

### Finding range conversion (`lint/report/evidence`)

#### [ ] READ-006 — `EvidenceRangeEntry::into_evidence` mixes trace resolution, certainty joining, and truncation policy, and leaks a 3-tuple plus an implicit `Some`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:48-91,158,168`

`into_evidence` (`evidence.rs:48-91`) performs four distinct jobs in one body:
per-occurrence trace resolution through the renderer, `MatchCertainty` joining
(`evidence.rs:60`), the truncated-vs-full `EvidenceTraces` policy
(`evidence.rs:85-89`), and the empty-trace fallback (`evidence.rs:79-84`), then
returns `Option<(SourceRange, EvidenceTraces, MatchCertainty)>` — a bare
3-tuple the single caller immediately destructures (`evidence.rs:159`) — while
the caller finishes the construction with `Finding::new(...).into()`
(`evidence.rs:168`), an `Into<Option<Finding>>` (`From<T> for Option<T>`) that
obscures what is simply `Some(finding)` in a `filter_map`.

**Recommendation:** Split the policy from the plumbing: extract the
certainty/truncation/fallback merge (the loop body, `evidence.rs:57-89`) into a
small owned helper on `MatchCertainty`/`EvidenceTraces` or a named
`ResolvedRange` payload struct, and replace the trailing `.into()` with
`Some(...)`. Guardrails: keep the `Definite`-wins certainty join, the
`EvidenceTraces::from_truncated` vs `new(...).ok()?` distinction, the
empty-trace `EvidenceTrace::occurrence` fallback, and the deterministic
`BTreeSet`-ordered trace set, which are pinned by the deterministic report
contract.

## Systemic Themes

- **Error surfaces over-state reachable failures.** `LintConfigError` declares
  a compiler-failure variant that cannot occur at linter construction
  (README-001), while `RuleCompilationError` (a four-kind parallel model of the
  internal `CompiledCatalogError`) erases structured diagnostics and is
  re-hosted through a hand roll (README-002). The intended layering
  (internal compiler error → stable provider-catalog error → single
  lint-construction error) is sound; the reachable sets and the translation
  should be tightened to match the layers actually reached.
- **The batch module is not over-built, and the driver/pending/completed
  machinery is well encapsulated.** The public surface is exactly four types
  (`BatchOptions`, `BatchResult`, `BatchResults`, `BatchStartError`,
  `lint/mod.rs:13`); `BatchDriver`, `PendingBatch`, `PendingEntry`,
  `CompletedBatch`, and `CompletionError` are crate-private
  (`batch.rs:111-139,225`), and the bounded, input-ordered protocol
  (`can_submit`/`submit`/`complete`/`take_ready`, `fail_protocol`,
  `synthesize_missing`) matches the ARCHITECTURE.md "Results are yielded in
  input order, and dropping the iterator cancels queued work" contract
  (`core/ARCHITECTURE.md:60-66`). `BatchResult::index()` is the only
  exposed protocol artifact, and it is the meaningful differentiator for
  duplicate-path batches (`tests/integration/batch.rs:80-81`). Note that
  `lint_batch` currently has no in-repo production caller — the CLI and
  `glass-lint-project` loader drive linting through a single shared
  `ProjectSession` with wave parallelism (`loader.rs:237-299`) — so the
  public batch surface is exercised only by integration tests today (see Open
  Questions).
- **Selection is decomposed, not sprawled.** `RuleSelection`/
  `RuleOverride`/`RuleBaseline`/`RuleState`/`PreparedRuleSelection` form the
  public configuration vocabulary and are each single-meaning; the parsing
  internals (`RuleSelector`/`RulePattern`/`PatternSegment`) and the one-shot
  `SelectionEvaluation` are private helpers, which is the correct ownership
  split. The real defects are the second validation grammar for exact IDs
  (README-003) and the double serde vocabulary (README-004), not the number of
  types. `LinterSharedConfig` is a justified Arc-shared immutable config bucket
  (its five fields are all consumed by `Linter`, and cloning the `Arc` is what
  makes batch worker clones cheap); it is not an immediately-consumed wrapper.
- **Report assembly is a clean four-owner pipeline.** `ProjectReportAssembler`
  is the transient orchestrator (`link`/`assemble`/`finish`);
  `ProjectReportSession` owns the assembly-scoped status snapshot and trace
  arena; `ReportFiles` is the deterministic accumulator (BTreeMap keyed by
  normalized path); and each renderer is single-purpose (`FindingRenderer` for
  findings, `attach_project_diagnostics` for diagnostics, `assemble_project_report`
  for aggregation/ops). Finding assembly is not duplicated with
  `analysis::matching::evidence`: the two layers operate on different semantics
  (classification capabilities → `Finding` here vs occurrence groups there).
  `ReportFiles`' BTreeMap order and `AnalysisReport::finalize`'s path sort are
  both by path, so files are double-ordered — harmless canonicalization, since
  `finalize` is the merge/append-safe owner.
- **Masked-invariant patterns.** `Linter::new`'s `unreachable!`
  (`linter.rs:156`) and `BatchOptions::from_workers`'s
  `NonZeroUsize::new(...).unwrap_or(NonZeroUsize::MIN)` (`batch.rs:39-40`)
  silently coerce construction-time guarantees instead of modeling them; the
  batch coercion is provably safe (inputs are always ≥ 1), but both are the
  same "assume the invariant, hide it" style. These stay minor because the
  invariants are cheap and local; README-001 proposes restructuring the
  unreachable one structurally.

## Open Questions

- `RuleCompilationError` currently string-erases the structured
  `CompilerInvariantDiagnostic`/`PhysicalPlanDiagnostic` payloads it mirrors.
  If the report/diagnostics schema ever wants to serialize provider-catalog
  compile failures, the stable boundary should carry the structured types
  instead; the right time to decide is the README-002 conversion, not later.
- Is `lint_batch` public API deliberately awaiting a production host? The
  integration suite (`tests/integration/batch.rs`) is thorough and the
  semantics are documented in core ARCHITECTURE.md, but neither the CLI nor the
  project loader calls it. If no host is planned, the four public
  `Batch*` exports (`lib.rs:38-43`) are currently contract pinned by tests
  alone.
- `LinterConfig::with_rules` round-trips a `Prepared` selection back to
  `Unprepared`, discarding the validated enabled indexes and re-evaluating the
  selection at `Linter::new` (`linter.rs:55-68`). No caller in the repo does
  `with_prepared_rules` then `with_rules`; the arm exists for builder
  order-independence. Is that property worth keeping, or should the builder
  document that `with_prepared_rules` is terminal?
- `ReportFiles::replace_findings` manufactures a fresh `FileReport` (no parse
  diagnostics) when a module path is not already present (`files.rs:56-59`).
  Since every `ProjectModule` path comes from a source that `initialize`
  already keyed, the branch appears unreachable; if that ever changes, this
  silent fallback would drop existing per-file data instead of failing.
- `Linter::enabled_rule_ids()` (`linter.rs:186-192`) has no caller in the
  current workspace, including tests. It is plausible external-consumer API,
  but it is the only public `Linter` accessor without in-repo evidence of use;
  confirm it is a designed part of the contract before relying on it.

## Coverage

Files reviewed (read-only; no source changes):

- `glass-lint-core/src/lint/mod.rs`, `batch.rs`, `batch/tests.rs`,
  `catalog.rs`, `catalog/tests.rs`, `linter.rs`, `linter/tests.rs`,
  `selection.rs`, `selection/tests.rs`
- `glass-lint-core/src/lint/report/mod.rs`, `report/diagnostics.rs`,
  `report/evidence.rs`, `report/evidence/tests.rs`, `report/files.rs`,
  `report/files/tests.rs`, `report/summary.rs`
- Context (not re-audited here): `glass-lint-core/src/lib.rs`,
  `glass-lint-core/src/config.rs`, `glass-lint-core/src/rule_id.rs`,
  `glass-lint-core/src/api/rule/error.rs`
  (`CompiledCatalogError`/`RuleCompilationError` boundary),
  `glass-lint-core/src/project/types/report/{analysis_report,finding,file_report}.rs`,
  `glass-lint-core/src/project/session/mod.rs` (assembler call site),
  `glass-lint-core/tests/integration/{batch,public_surface,linter}.rs`,
  `glass-lint-project/src/loader.rs`, `glass-lint-cli/src/config.rs`,
  `glass-lint-harness/src/adapters.rs`

Callers traced: `Linter::begin_project` → `SessionState::new`
(`linter.rs:130-141`, consumed by `ProjectSession::finish` →
`ProjectReportAssembler::link/assemble`, `project/session/mod.rs:416-426`);
`ProjectAnalysis::{into_report,into_parts}` (`adapters.rs:169`,
`loader.rs:414-417`); `RuleSelection::{prepare,resolve}` (CLI `config.rs:283-286`,
`linter.rs:158`); `PreparedRuleSelection::into_parts` (`linter.rs:149`);
`RuleCatalog::{combine,metadata,rule_ids,rule_count,rule_id,compiled}`
(provider crates and CLI `config.rs:327-396`); the batch protocol
(`tests/integration/batch.rs:27-150`, unit tests `batch/tests.rs:19-141`);
the report pipeline (`FindingRenderer::populate_project_files` →
`merge_duplicate_findings` → `Finding::has_primary/merge_duplicate`,
`report/mod.rs:220-228`, `evidence.rs:113-249`).

Verification performed: `rg` confirms `LintConfigError::InvalidRule` is never
constructed and `Linter::enabled_rule_ids` has no callers; confirmed
`RuleCatalog::combine`'s only error is `DuplicateRule`; confirmed the
`into_evidence` 3-tuple destructure and the `Finding::new(...).into()`
`Option` conversion; confirmed every `pub(super)` item's consumers lie inside
its module subtree; confirmed `rule_state_as_bool` is the sole serde path for
`RuleState` and the `rename_all = "lowercase"` derive is unreachable; confirmed
`ReportFiles` path-BTreeMap ordering plus `AnalysisReport::finalize`
re-sorting; and confirmed `git status --short` shows only this audit file as
new among the chunk files I created.