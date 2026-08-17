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
`ProviderCatalogError::DuplicateRule` and its Display arm (`catalog.rs:22,84`)
at the same time. Guardrails: `DuplicateRule`, `UnknownRule`, and
`InvalidSelector` retain their messages and ordering on `LintConfigError`;
`RuleCatalog::new`'s `InvalidRuleId`/`InvalidRule` errors keep flowing through
`ProviderCatalogError`; the only `combine` callers (`linter.rs:154`,
`catalog/tests.rs:20,30`) are updated together, with
`combined_catalog_rejects_duplicate_namespaced_ids` (`catalog/tests.rs:19-26`)
asserting the returned `RuleId` directly; and integration tests that assert
`LintConfigError::UnknownRule` (`tests/integration/linter.rs:208,248`,
`linter/tests.rs:97`) stay unchanged.

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
(`catalog.rs:118`). The rebuild is a hand-written destructure/reassemble of
every variant's payload — exactly the mechanical conversion a `From` impl (or
the existing `Display` on `CompiledCatalogError`, `error.rs:206-236`) would
own. `RuleCompilationError` (`catalog.rs:27-36`) is a full parallel model of
the four `CompiledCatalogError` variant kinds (`InvalidMatcher`/`InvalidQuery`/
`CompilerInvariant`/`InvalidPhysicalPlan`) that also discards the structured
`CompilerInvariantDiagnostic`/`PhysicalPlanDiagnostic` payloads into `String`
and drops the variant context from `Display` (all four arms write only the
message, `catalog.rs:44`).

**Recommendation:** Give the conversion its canonical owner: implement
`impl From<CompiledCatalogError> for ProviderCatalogError` in `lint/catalog`
and change `RuleCatalog::new` to `compile_records(&rules_and_ids).map_err(ProviderCatalogError::from)?`,
deleting the free function. Guardrails: keep the `ProviderCatalogError::InvalidRule`
Display shape (`invalid rule \`{id}\`: {message}`, `catalog.rs:83`) and the
flat message text stable — that is the string callers see; keep the flat
`RuleCompilationError` vocabulary as the stable provider-boundary shape (it is
the payload type of the exported `ProviderCatalogError::InvalidRule`, exported
at `lib.rs:41`); update `catalog_mapping_preserves_compiler_error_categories`
(`catalog/tests.rs:44-85`) to `ProviderCatalogError::from(compiled)`; and, per
the resolved open question (Open Questions — Resolved), do not retrofit the
structured `CompilerInvariantDiagnostic`/`PhysicalPlanDiagnostic` payloads onto
this boundary — no consumer reads them here and the report schema can never see
a compile failure.

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
wildcard, validate with `RuleId::parse` only and store no pattern (a
`has_wildcard` flag or `Option<RulePattern>` in `RuleSelector`); for wildcard
selectors, build `RulePattern` (whose grammar already handles the literal
checks) and skip the `RuleId::parse` pass. Keep `RuleSelector::matches`'s
exact branch as the sole matching path for non-wildcards. Guardrails: preserve
the `InvalidSelector` failure mode for malformed wildcards and the empty
selector, keep the `?[]{}\` rejection in `RuleSelector::parse`
(`selection.rs:196-201`), and preserve the deterministic `UnknownRule` vs
`InvalidSelector` distinction in `validate_override_matches`
(`selection.rs:358-375`); the exact-matching unit tests (`selection/tests.rs:7-29`)
and the wildcard-part validation tests (`selection/tests.rs:101-109`) pin both
grammars and must pass unchanged.

#### [ ] READ-004 — `RuleState` declares a lowercase-string serde vocabulary that no site uses; only the bool shim is reachable

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/selection.rs:30-36,44-47,161-185`

`RuleState` derives serde with `rename_all = "lowercase"`
(`selection.rs:31-32`), yet its only serialization site — `RuleOverride`'s
`state` field — bypasses that representation entirely with
`#[serde(rename = "enabled", with = "rule_state_as_bool")]`
(`selection.rs:44-47`), so the derived vocabulary is unreachable and the two
grammars ("enabled"/"disabled" vs `true`/`false`) must be held in sync by hand.
`rule_state_as_bool` (`selection.rs:161-185`) is a private inline `mod`
referenced only through that serde attribute; its `pub(super)` functions have
exactly the visibility the attribute needs (the derive on `RuleOverride`
resolves the `with` path from the parent `selection` module, so the functions
must be visible there), so the shim is not over-exposed — the defect is the
dead lowercase derive, not the shim. The two-variant enum exists to give the
boolean meaning, which is good vocabulary, but the model currently asserts two
incompatible serializations for one field.

**Recommendation:** Choose one wire shape and delete the other: remove the dead
`rename_all = "lowercase"` serde derive from `RuleState` (`selection.rs:31-32`)
and keep `rule_state_as_bool` as the single representation of the `state`
field, so `RuleOverride`'s derived serialization is the only vocabulary in
play. If the boolean meaning is better modeled as data, the further step is
converting the field to a plain `enabled: bool` with `RuleState` kept only as a
conversion API — but the minimal change is removing the unreachable derive.
Guardrails: the `CoreConfig.overrides` wire shape (each override as a
`selector` string plus `enabled: true|false`, the shape CLI config files
document) must keep serializing booleans, selector validation must still reject
invalid selectors at deserialize time (`deserialize_selector`,
`selection.rs:152-159`), and the `RuleOverride::new`/`state()` public surface
stays intact for linter tests (`linter/tests.rs:89`).

### Module visibility (`lint/batch`, `lint/report`)

#### [ ] READ-005 — `pub(super)` items used only inside their own module subtree leak into the whole `lint` module

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/batch.rs:111,245`; `glass-lint-core/src/lint/report/mod.rs:66,111,120,124,130`

Several items are declared `pub(super)` even though every consumer lives in the
declaring module or its child modules, where plain privacy already suffices:
`CompletedBatch` (`batch.rs:111`, referenced only inside `batch.rs` and
`batch/tests.rs`), `BatchDriver::new` (`batch.rs:245`, used within `batch.rs`
by `BatchResults::new` at `batch.rs:358` and by `batch/tests.rs` via
`use super::*`), and `ProjectReportSession` itself (`report/mod.rs:66`) plus
its `status_diagnostics`/`is_complete`/`reconstruct_trace`/`trace_node_count`
(`report/mod.rs:111,120,124,130`, consumed only by the sibling modules
`diagnostics.rs`, `evidence.rs`, and `summary.rs`). As written, `pub(super)`
makes these reachable from the entire `crate::lint` subtree (`linter.rs`,
`catalog.rs`, `selection.rs`), widening the surface with no consumer.

**Recommendation:** Drop the `pub(super)` qualifier on these items so they are
module-private; unit-test modules keep working because they are child modules
and import via `use super::*` (e.g. `batch/tests.rs:3`). Keep the qualifier
where a real cross-module call exists today (`BatchResults::new` is consumed by
`linter.rs:258` and must stay `pub(super)`). Guardrails: no behavior change;
the batch protocol types remain crate-internal, and `report`'s public surface
stays just `ProjectAnalysis`/`ProjectAnalysisTimings` (re-exported at
`lint/mod.rs:16`).

### Finding range conversion (`lint/report/evidence`)

#### [ ] READ-006 — `EvidenceRangeEntry::into_evidence` mixes trace resolution, certainty joining, and truncation policy, and leaks a 3-tuple plus an implicit `Some`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:48-91,158,167`

`into_evidence` (`evidence.rs:48-91`) performs four distinct jobs in one body:
per-occurrence trace resolution through the renderer, `MatchCertainty` joining
(`evidence.rs:59-61`), the truncated-vs-full `EvidenceTraces` policy
(`evidence.rs:85-89`), and the empty-trace fallback (`evidence.rs:79-84`), then
returns `Option<(SourceRange, EvidenceTraces, MatchCertainty)>` — a bare
3-tuple the single caller immediately destructures (`evidence.rs:158`) — while
the caller finishes the construction with `Finding::new(...).into()`
(`evidence.rs:167`), an `Into<Option<Finding>>` (`From<T> for Option<T>`) that
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
  (READ-001), while `RuleCompilationError` (a four-kind parallel model of the
  internal `CompiledCatalogError`) erases structured diagnostics and is
  re-hosted through a hand roll (READ-002). The intended layering
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
  Questions — Resolved).
- **Selection is decomposed, not sprawled.** `RuleSelection`/
  `RuleOverride`/`RuleBaseline`/`RuleState`/`PreparedRuleSelection` form the
  public configuration vocabulary and are each single-meaning; the parsing
  internals (`RuleSelector`/`RulePattern`/`PatternSegment`) and the one-shot
  `SelectionEvaluation` are private helpers, which is the correct ownership
  split. The real defects are the second validation grammar for exact IDs
  (READ-003) and the double serde vocabulary (READ-004), not the number of
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
  invariants are cheap and local; READ-001 proposes restructuring the
  unreachable one structurally.

## Open Questions — Resolved

- **Should `RuleCompilationError` carry the structured
  `CompilerInvariantDiagnostic`/`PhysicalPlanDiagnostic` payloads it currently
  string-erases? Resolved: no.** The structured types are produced and consumed
  entirely inside the authoring/compiler layer (`api/rule/error.rs:33-78`,
  `api/compiler/mod.rs:209-214`); at the lint boundary nothing reads them —
  `RuleCompilationError` is only ever `Display`-rendered (`catalog.rs:38-47`)
  or compared in the mapping unit test (`catalog/tests.rs:44-85`). A compile
  failure also cannot reach the report/diagnostics schema by construction:
  `RuleCatalog::new` compiles during catalog construction (`catalog.rs:118`),
  before a linter or report exists, so there is no serialization consumer to
  serve. Keep the flat vocabulary as the stable provider-boundary shape (it is
  the public payload type of `ProviderCatalogError::InvalidRule`, exported at
  `lib.rs:41`); the READ-002 conversion should not re-add field access.
- **Is `lint_batch` public API deliberately awaiting a production host?
  Resolved: keep it public; it is a documented, test-pinned contract.**
  `Linter::lint_batch` (`linter.rs:239-266`) is the only batch entry point and
  the four `Batch*` types are deliberately exported (`lib.rs:38-43`,
  `lint/mod.rs:13`). No production crate calls it — the CLI drives analysis
  through `glass-lint-project`'s `ProjectLoader` (`cli/lint.rs:51-63`) →
  `Linter::begin_project` (`loader.rs:212`) with wave parallelism
  (`loader.rs:237-303`), and the harness uses `lint_source`/`begin_project`
  (`adapters.rs:86,117,134`, `profile/runner/workers.rs:41-50`) — so the
  surface is contract-pinned by the integration suite alone
  (`tests/integration/batch.rs`, six tests covering laziness, bounding,
  input order, duplicate paths, malformed items, cancellation, and cache
  reuse). The semantics are a documented core contract
  (`core/ARCHITECTURE.md:60-66`), and the harness, which lints many independent
  one-file snippets across threads, is a plausible future host that could adopt
  `lint_batch` without API change. No in-repo evidence suggests a demotion is
  planned.
- **Should the builder keep `with_rules` total over a `Prepared` state?
  Resolved: keep it, and document it.** `with_rules` (`linter.rs:55-68`)
  extracts the prepared selection's catalog and re-stores the config as
  `Unprepared` with the new selection, discarding the validated enabled
  indexes. No in-repo caller chains `with_prepared_rules` then `with_rules` —
  the CLI's `selected_linter` picks exactly one branch (`cli/config.rs:381-388`)
  — but the arm is what keeps `with_rules` total and order-independent: when
  the prepared selection was built against the same catalogs (as
  `Config::validate` does, `cli/config.rs:283-286`), the round-trip is
  idempotent. That is a real builder property worth keeping; add a doc note on
  `with_prepared_rules` (`linter.rs:70-76`) stating that a later `with_rules`
  re-evaluates the new selection against the prepared catalog and discards the
  prepared validation.
- **Is `ReportFiles::replace_findings`'s fresh-`FileReport` fallback reachable?
  Resolved: no — make the invariant explicit instead of silent.**
  `populate_project_files` (`evidence.rs:113-125`) calls it with
  `module.path()` for every module in the linked model, and every module path
  is a source-table path by construction: `ResolvedLinkInput::build`
  (`model.rs:143-155`) requires each analyzed path to resolve through
  `sources.module_ids()` (`tables.rs:103-116`), while `ReportFiles::initialize`
  keys exactly `sources.in_normalized_path_order()` (`files.rs:21-39`), and
  `validate_complete` (`artifacts.rs:108-119`) requires every source to be
  analyzed. The `else` branch (`files.rs:55-60`) therefore never runs today;
  because it would silently drop existing per-file data (e.g. parse
  diagnostics) in a hypothetical flow that bypassed `initialize`, replace it
  with an invariant check (`debug_assert!`/`expect` on `self.files` containing
  the path) so the failure is loud if the invariant ever breaks.
- **Is `Linter::enabled_rule_ids()` dead API? Resolved: no — it has in-repo
  callers and is part of the contract.** The premise was wrong: it is exercised
  by the integration suite (`tests/integration/linter.rs:236-238` asserts the
  exact enabled set after override ordering) and by the CLI config tests
  (`cli/config/tests.rs:53,71,94` assert profile baseline plus override
  composition). It is the accessor the CLI's own tests rely on to verify rule
  selection, so it is contract-pinned public API; keep it as-is.

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
`RuleCatalog::combine` (only `linter.rs:154` and `catalog/tests.rs:20,30`;
provider crates and the CLI never call it — they use `metadata`/`rule_ids`/
`rule_count`/`rule_id`/`compiled`: `js/lib.rs:86,121`, `obsidian/lib.rs:33,67`,
`cli/output.rs:38`, `cli/config.rs:392`, `cli/config/tests.rs:6,12`,
`selection.rs:333`, `report/mod.rs:201`); the batch protocol
(`tests/integration/batch.rs:27-150`, unit tests `batch/tests.rs:19-141`);
the report pipeline (`FindingRenderer::populate_project_files` →
`merge_duplicate_findings` → `Finding::has_primary/merge_duplicate`,
`report/mod.rs:220-228`, `evidence.rs:113-249`).

Verification performed: `rg` confirms `LintConfigError::InvalidRule` is never
constructed; `Linter::enabled_rule_ids` does have callers — the integration
suite (`tests/integration/linter.rs:236`) and the CLI config tests
(`cli/config/tests.rs:53,71,94`) — so the earlier "no callers" premise is
retracted and the open question is resolved to "keep it"; confirmed
`RuleCatalog::combine`'s only error is `DuplicateRule`; confirmed the
`into_evidence` 3-tuple destructure (`evidence.rs:158`) and the
`Finding::new(...).into()` `Option` conversion (`evidence.rs:167`); confirmed
every `pub(super)` item's consumers lie inside its module subtree (with
`BatchDriver::new` additionally used within `batch.rs` itself at line 358);
confirmed `rule_state_as_bool` is the sole serde path for `RuleState` and the
`rename_all = "lowercase"` derive is unreachable; confirmed `ReportFiles`
path-BTreeMap ordering plus `AnalysisReport::finalize` re-sorting
(`file_report.rs:51`, `analysis_report.rs:130-137`); and confirmed `git status
--short` shows only this audit file as new among the chunk files I created.