# Codebase Readability Audit — Chunk 16

## Summary

Chunk 16 owns the public runtime boundary around environments, limits, rule
catalogs, linter selection, batch execution, project admission, phase-state
transitions, and report values. The phase-state design is generally strong:
sources are owned by the project session, local analysis precedes authored
resolution, and reports expose provider-neutral deterministic values. The main
risks are parallel storage for one catalog identity, duplicated single-source
and batch analysis paths, repeated normalization of already-validated project
types, and public transformations that can bypass report ordering guarantees.
Several numeric and metric APIs also make important state or field identity
depend on positional `usize` arguments and caller sequencing.

The project/cache findings were cross-checked against Chunk 3’s cache identity
and lowering-completion findings; the report/evidence findings were checked
against its evidence-normalization finding; and phase-state, flow-budget, and
identity-storage findings from Chunks 4, 8, 11, and 14 were not repeated.

## Findings

### Catalog and selection ownership

#### [ ] READ-074 — Store catalog identity and compiled records under one index owner

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/catalog.rs:41-175`; selection caller in `lint/selection.rs:204-224`; metadata and evidence consumers in `lint/report.rs` and `lint/report/evidence.rs`

`RuleCatalog` stores the same ordered domain in three related structures:
`records: Vec<CompiledRuleRecord>`, `rule_ids: Vec<RuleId>`, and
`rule_indices: BTreeMap<RuleId, RuleIndex>`. `metadata` and rule selection
zip the first two vectors, while matching and report assembly pass a
`RuleIndex` through the catalog and compiled-record slices separately. The
length and positional correspondence of the vectors is an invariant maintained
by `new` and `combine`, not by the type.

This makes catalog identity storage caller-shaped: a future insertion,
filtering operation, or catalog transformation must update every collection in
the same order. The existing `combine` loop also moves records and IDs through
parallel iterators before rebuilding the map, and a misalignment would produce
the wrong rule metadata or matcher for a valid-looking `RuleIndex`.

**Recommendation:** Make one private catalog entry own the fully qualified
`RuleId` and its `CompiledRuleRecord`, with the entry vector defining stable
catalog order. Let the catalog own `RuleIndex` lookup and expose narrow
operations such as `entry`, `id`, `compiled`, and metadata projection; delete
the parallel-vector zips and direct `records` access after callers migrate.
Preserve stable index order across `combine`, duplicate fully qualified ID
rejection, confidence-based selection, compiled-plan immutability, and the
absence of source query declarations after compilation.

**Fix Applied:** None so far.

### Local execution and batch lifecycle

#### [ ] READ-075 — Give one project owner the cache/lowering transition

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/session/mod.rs:75-125,187-203,233-280,315-417`; execution protocol in `project/session/execution.rs:43-76,190-263`

The single-source path in `ProjectCollection::analyze_source_at_path_with_observer`
performs cache-key construction, cache lookup, observer events, lowering,
parse-failure recording, cache insertion, and `record_lowered`. The parallel
path rebuilds the same lifecycle in `LocalAnalysisCallbacks::prepare` and
`release`; it additionally captures a test-sensitive fingerprint closure and
feeds the same artifacts through a callback protocol. The two paths can drift
in cache-event counts, error recording, or the point at which a source becomes
complete even though they promise the same local-analysis semantics.

The executor should own worker dispatch and bounded submission, but the
project session should own the semantic transition from an admitted source to
either a cached/lowered artifact or a recorded parse failure. At present that
transition is split between the session, callback implementation, and executor
observer protocol, making `analyze_pending_sources_with` coordinate storage,
fingerprinting, scheduling, and deterministic request sorting in one flow.

**Recommendation:** Introduce one session-owned local-analysis operation that
computes the canonical cache key, applies cache hit/miss behavior, lowers one
source, and commits the result to `AnalysisArtifacts`; let the synchronous and
parallel executors call that operation through a narrow job result boundary.
Delete the duplicate cache/lowering/recording branches and the callback-owned
fingerprint closure after migration. Preserve one-analysis-per-path admission,
cache hit and eviction telemetry, parse failures as ordinary per-source
outcomes, bounded worker submission, completion-order independence, and final
request sorting.

**Fix Applied:** None so far.

#### [ ] READ-076 — Make pending batch entries own index, path, and completion state

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Lifecycle
- **Location:** `glass-lint-core/src/lint/batch.rs:101-197,240-300`

`PendingBatch` keeps submitted paths in one `BTreeMap<usize,
ProjectRelativePath>` and completed values in another `BTreeMap<usize,
CompletedBatch>`, while `CompletedBatch` repeats both the index and path. The
state machine must maintain `next_index`, `next_expected`, `in_flight`, both
maps, and the invariant that the two copies of a path agree. `take_ready`
therefore removes from one map, asserts that the item exists, and separately
debug-checks the path stored in the completion message.

The duplicate path/index representation is especially visible in cancellation
and missing-worker synthesis: `synthesize_missing` reconstructs completion
messages from the path map, while normal workers send the same path through the
channel. A future completion or cancellation branch can leave one map out of
sync and turn an ordering invariant into the `expect` in `take_ready`.

**Recommendation:** Represent each submitted index with one pending entry that
owns its path and either a waiting or completed result. Worker messages need
only the index and result (or a single owned entry transition), and readiness
should consume the entry without a second path lookup or `expect`. Keep the
index-order cursor, in-flight window, synthesized worker-panic results,
input-order delivery, `size_hint`, and cancellation behavior unchanged.

**Fix Applied:** None so far.

### Project input and phase boundaries

#### [ ] READ-077 — Trust validated project types at the phase boundary

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/mod.rs:37-67`; `project/types/input.rs:239-318,331-418`; re-normalization in `project/session/mod.rs:219-222` and `project/session/artifacts.rs:135-164`

The public project types already establish canonical forms: `SourceFile` owns
a `ProjectRelativePath`, `ResolutionRequestKey` owns a validated importer
path, and resolver variants contain `PackageSpecifier`, `BuiltinModuleName`,
or `NormalizedOutsidePath`. The session nevertheless calls
`normalize_relative` and `SourceFile::set_path` again when admitting every
source, and `into_link_input` calls `ResolutionRequestKey::normalize` and
`ResolverOutcome::normalize` for values whose typed fields have already been
validated. The only genuinely raw resolver payload is the free-form
`Unsupported { reason: String }` variant, which can represent an empty reason
until the consuming transition rejects it.

This weakens the meaning of the public types and makes the phase transition
carry normalization policy that should belong to construction. It also gives
callers two different failure times: path/target constructors can reject bad
values immediately, while a raw unsupported reason or equivalent malformed
outcome survives until `resolve`. The repeated conversions obscure which
state is actually being frozen for linking.

**Recommendation:** Keep `ResolverOutcome` as the external adapter DTO, but
give each variant validated constructors and one linker-boundary conversion
that canonicalizes any raw adapter values (especially unsupported reasons).
Remove repeated normalization from already-validated internal values while
retaining that single boundary for external input. Preserve relative-path
escape rejection, explicit Missing/Unsupported/OutsideProject states,
authored-request membership checks, deterministic normalization, and
fail-closed rejection of unknown requests.

**Fix Applied:** None so far.

#### [x] READ-078 — Keep public report transformations inside the deterministic finalization boundary

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/report/analysis_report.rs:87-127`; final assembly and sorting in `project/report/mod.rs:36-49` and `lint/report.rs:72-93`

`AnalysisReport::combine` validates schema/tool identity, merges reports, and
then calls the private `sort_deterministically`. The public
`with_project_diagnostics` and `into_partial` transformations append
diagnostics after that finalization path without sorting them. In particular,
the order of messages supplied to `with_project_diagnostics` becomes observable
in serialized output, and a report that was deterministic can become
order-dependent after a caller adds a project diagnostic or marks it partial.

The report type therefore exposes two contracts: reports assembled or combined
by core are ordered, while public post-processing relies on caller discipline.
This is a report-level invariant, not a presentation concern; downstream CLI,
JSON, and report-comparison consumers all observe the stored order.

**Recommendation:** Make every consuming report transformation call one
private finalization operation that sorts files and diagnostics, or return a
small report builder whose `finish` is the only way to obtain a finalized
report. Delete direct append paths that bypass the owner. Preserve schema and
tool-version checks, file/path ordering, diagnostic code/path/message ordering,
monotone `ReportCompletion::join`, and the ability to add project-level
diagnostics without changing finding semantics.

**Fix Applied:** Added one report-owned `finalize` operation and routed
combination, project-diagnostic enrichment, and partial-report conversion
through it. Public transformations now preserve deterministic file and
diagnostic ordering; a regression test covers chained transformations and
partial completion. Verified with `make fmt && make ci`.

### Numeric and metric APIs

#### [ ] READ-079 — Replace positional limit construction with named limit operations

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/limits.rs:53-139,169-228`; deserialization call site in `limits.rs:274-311`

`AnalysisLimits::new` accepts seven indistinguishable `usize` values in a
fixed order: syntax depth, semantic operations, effect operations, evidence,
link, flow, and trace nodes. The type correctly hides its validated
`PositiveLimit` fields, but the public constructor makes field identity depend
on argument position; existing tests use runs of `1` values and therefore do
not communicate which limit is being exercised. The seven `with_*` methods
already provide a more self-describing operation surface, but callers must
still choose between two construction styles.

**Recommendation:** Make `Default` plus named builder methods (or a dedicated
`AnalysisLimitsBuilder`) the canonical public construction path and keep one
private validation/freeze operation that builds the trusted `AnalysisLimits`.
Use a small deserialization DTO to feed the same named builder rather than
reconstructing the positional call. Preserve positive-value validation and
error specificity, serde defaults/unknown-field rejection, all seven limit
values, and the ability to configure limits before a linter or lowerer is
created.

**Fix Applied:** None so far.

#### [ ] READ-080 — Give operation-count accumulation a staged owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Lifecycle
- **Location:** `glass-lint-core/src/project/types/report/operations.rs:1-124`; initial construction in `analysis/project/model.rs:498-511`; later mutation in `lint/report.rs:72-75`

`AnalysisOperationCounts::new` initializes only the first seven counters with
another positional list of `usize` values, defaulting six path/evidence
metrics to zero. `ProjectSemanticModel::operation_counts` supplies the graph
metrics, then `ReportAssembly::finish` mutates the same value through
`set_effect_projections` and `set_path_metrics` after matching and report
rendering. The operation-count value is therefore both a public report DTO and
an internal partially initialized accumulator, with phase order and field
meaning maintained by callers.

This split makes a new metric easy to initialize in the wrong phase or omit
from `AddAssign`, serialization, and aggregate reporting. The raw constructor
also lets tests and external callers silently swap same-typed graph counters.

**Recommendation:** Keep `AnalysisOperationCounts` as the finalized immutable
report value and give the linking/matching pipeline a private staged accumulator
with named recording methods or phase-specific constructors. Let one report
owner consume that accumulator into the final DTO, deleting the public raw
constructor and broad setter pair. Preserve saturating aggregate addition,
zero values for stages that did not run, all current counter meanings, and
deterministic operation totals for combined and partial reports.

**Fix Applied:** None so far.

## Systemic Themes

- Stable identities are still represented through parallel collections or
  positional primitives at several public boundaries. Catalog entries, batch
  items, validated project outcomes, limits, and operation metrics would be
  easier to audit if their owners carried the identity and lifecycle directly.
- The project session correctly uses consuming phase types, but local analysis
  and report construction still duplicate transitions around those types.
  Shared operations should own cache/artifact commits and report finalization,
  leaving executors and presentations as narrow adapters.
- Determinism is an architectural contract. It should be enforced by report
  and batch owners rather than inferred from callers preserving map alignment,
  message order, or setter sequencing.

## Decisions

- `RuleCatalog` retains a private entry wrapper containing the fully qualified
  `RuleId` and compiled record. Compiler records stay provider-neutral, while
  one catalog-owned `RuleIndex` controls metadata, selection, and evidence.
- `ResolverOutcome` is intentionally the typed DTO accepted from arbitrary
  external resolver adapters. Its public variants remain authorable, and one
  linker-boundary normalization/validation step owns canonicalization rather
  than pretending every adapter has already supplied normalized data.
- Public post-assembly diagnostic injection is intentional for callers that
  aggregate project status. The report type owns a finalization step that
  re-sorts files and diagnostics after every consuming transformation.

## Coverage

- **Reviewed modules:** `config`, `diagnostic`, `ecma_version`, `environment`,
  `limits`, `lint`, `lint::{batch,catalog,linter,ranges,report,selection}`,
  `lint::report::{diagnostics,evidence,summary}`, `project`,
  `project::{input,report,session,tables,types}`,
  `project::session::{artifacts,execution}`, and
  `project::types::{input,report}` including report analysis, code, diagnostic,
  evidence, file, finding, location, and operation types.
- **Workflow traced:** linter configuration → catalog combination and rule
  selection → single/batch local lowering and cache admission → consuming
  project resolution/linking → report assembly, aggregation, and public report
  transformations.
- **Prior overlap check:** Cache identity/completion, evidence grouping,
  project flow budgets, phase-state storage, and semantic identity findings
  from earlier chunks were considered and not repeated.
- **Fixes:** None; this is a read-only structural audit.
- **Tests:** Not run; no source behavior was changed.
