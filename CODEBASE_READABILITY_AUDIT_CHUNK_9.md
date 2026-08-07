# Codebase Readability Audit — Chunk 9

## Summary

Chunk 9 owns interprocedural flow summaries, the frozen-plus-overlay path
store used to project them, and the local lowering/artifact boundary. The
summary implementation has a sound bounded fixed-point design and the local
artifact types keep reusable semantic state separate from path-specific source
context. The main readability risks are the ownership of summary sink
admission and completion, plus the fact that summary path identity is carried
as a freely copyable store-relative value rather than being validated by its
owning store.

The artifact-cache identity and lowering-completion/freeze concerns were
reviewed but are already covered by earlier chunk reports; they are not
repeated here.

## Findings

### Summary path identity

#### [ ] READ-001 — Make summary path identity store-owned

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/summary/store.rs:5-63,85-115`; `flow/summary/parameter.rs:9-17`; `flow/summary/sink.rs:13-40`

`SummaryPathId` is a copyable enum containing raw `PathId` values, and
`SummaryPathStore::from_frozen_path` is used as a general conversion inside
the path implementation. Most callers correctly pass frozen paths through
`intern_frozen`, but `ParameterBinding::matches_sink_path` calls the static
`SummaryPathStore::matches_frozen`, which reconstructs a frozen ID and compares
it directly without consulting the store or validating either operand. The
same representation is accepted by `FunctionSinkSummary::new`, so the
invariant that a summary path belongs to the current frozen/overlay store is
maintained by callers and conventions rather than by the type that owns the
path graph.

This makes cross-artifact or stale IDs especially hard to reason about: most
operations fail closed through `is_valid`/store lookups, while the exact
parameter-path test can succeed solely because two raw `PathId` values happen
to compare equal. It also forces reviewers to distinguish safe interning from
raw enum construction at every summary-sink boundary.

**Recommendation:** Make frozen-path matching an instance method on
`SummaryPathStore` and validate the supplied ID before comparing it with the
validated frozen base. Keep raw `SummaryPathId` constructors private to the
store, or introduce store-scoped constructors/opaque path handles so
`FunctionSinkSummary` can only receive an ID admitted by its `SummaryPathStore`.
Preserve the frozen-versus-overlay distinction, linked overlay parents,
fail-closed invalid paths, rest-parameter prefix matching, and deterministic
path ordering.

**Fix Applied:** None so far.

### Summary sink admission

#### [ ] READ-002 — Centralize bounded summary-sink admission

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/summary/summaries.rs:98-138,140-192`; `flow/summary/sink.rs:42-75`

`FunctionSummaries::collect_direct_sinks` and
`FunctionSummaries::propagate_call_sinks` both interpret `InsertOutcome`,
charge the shared `Budget` once per inserted sink, update `total_sinks`, and
decide whether `MAX_SUMMARY_SINKS` or budget exhaustion makes the whole
summary computation unusable. They do so with different control shapes:
direct collection inserts into a summary and checks the total afterward,
whereas propagation first builds a projection vector, checks the total while
building it, then charges and inserts in a second loop. The global capacity
policy is therefore split from `SinkSet`, which only knows local uniqueness,
and can drift between direct and propagated paths when limits or admission
semantics change.

The split also obscures what the count means: projected candidates can be
materialized before admission, and `total_sinks` is incremented only after
local deduplication. A future capacity change must preserve both the local
unique-set semantics and the global bounded-analysis semantics in two callers.

**Recommendation:** Add a private summary-sink admission owner on
`FunctionSummaries` (or a focused `SummarySinkBudget`) that accepts a local
`InsertOutcome`/candidate batch, charges the budget for admitted work, updates
the global count, and returns a typed `Accepted`, `Duplicate`, or `Exhausted`
outcome. Have direct collection and propagation use that operation and delete
their repeated budget/count/limit choreography. Preserve uniqueness before
global counting, the current deterministic final sort, the hard
`MAX_SUMMARY_SINKS` bound, and the rule that any exhausted summary result is
cleared rather than treated as a complete witness.

**Fix Applied:** None so far.

### Summary collection lifecycle

#### [ ] READ-003 — Give summary collection one explicit completion owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/summary/summaries.rs:19-75,78-96,201-283`

`FunctionSummaries::collect` coordinates four distinct phases: fact seeding,
direct sink discovery, fixed-point propagation, and terminal sink cleanup or
sorting. The phase boundary is represented by the mutable `exhausted` boolean,
while `SummaryPropagation::run` returns a second `PropagationOutcome`; the
caller translates only one of those outcomes into `exhausted`, then performs
a separate pass that either clears every sink or sorts every sink. Resource
failure can originate in fact insertion, direct sink admission, propagation
rounds, propagation worklist growth, or sink projection, so the lifecycle
contract is distributed across `FunctionSummaries`, `SummaryPropagation`, and
`SinkSet` instead of being represented by a summary-collection result.

That arrangement makes the important semantic rule implicit: a bounded or
incomplete summary must not leak partial sinks as if propagation completed,
whereas a complete summary must be deterministically finalized. Adding a new
bounded phase or completion reason requires threading another boolean/result
through the orchestration and remembering to update the final cleanup branch.

**Recommendation:** Introduce a private collection-completion value that owns
phase transitions and terminal finalization, with typed exhaustion reasons
from fact collection, direct admission, and propagation. Keep the phases
separate, but have the owner perform exactly one `finalize` operation that
clears partial sinks on exhaustion or sorts all sinks on completion. Preserve
fixed-point re-enqueue behavior, `MAX_SUMMARY_WORKLIST`, budget accounting,
deterministic sink order, and fail-closed behavior for incomplete summaries.

**Fix Applied:** None so far.

## Systemic Themes

- Summary paths deliberately combine an immutable frozen graph with a bounded
  overlay, but the identity and validity boundary is weaker than the graph
  operations that consume it.
- Local sink uniqueness, global sink capacity, operation budget, and final
  completeness are separate concepts; each should have one owner while their
  distinct semantics remain visible.
- Interprocedural propagation must remain bounded and deterministic. Partial
  propagation cannot establish a definite witness, and cleanup must not erase
  an independent complete possible witness outside the exhausted summary
  result.

## Open Questions

- Can `SummaryPathId` be made private to `SummaryPathStore` without making
  `FunctionSinkSummary` carry a store reference, or is a lightweight
  store-generation token needed to retain copyable sink values?
- Should the summary sink limit count only unique inserted sinks, as the
  current `InsertOutcome` flow implies, or should candidate projection work
  also consume the global limit? The admission owner should make that choice
  explicit before changing either path.
- Is clearing all summary sinks on any propagation exhaustion the intended
  contract for downstream flow projection, or should completion carry a typed
  incomplete state so callers can distinguish unavailable summaries from empty
  summaries without inspecting a separate boolean?

## Coverage

Reviewed all types listed in Chunk 9 of `CODEBASE_STRUCTURE_CORE.md`:

- Flow summaries: `FunctionSignature`, `FunctionSinkSummary`,
  `FunctionSummary`, `InsertOutcome`, `SinkSet`, `SummaryPathId`,
  `SummaryPathStore`, `FunctionSummaries`, `PropagationOutcome`, and
  `SummaryPropagation`.
- Local artifacts: `ArtifactCache`, `ArtifactCacheHandle`,
  `ArtifactCacheKey`, `ArtifactFingerprint`, `CacheEntry`, `LocalArtifact`,
  `LocalLoweringConfig`, `LocatedSourceContext`, `ProjectModule`,
  `SemanticArtifact`, and `SharedSemanticArtifact`.
- Lowering and status: `InvalidParserSpan`, `LoweredSource`, `Lowerer`,
  `LoweringCapabilities`, `LoweringCompletion`, `ParserSpanKey`,
  `ResolvedProgram`, `SpanNormalizer`, `SemanticBudget`,
  `AnalysisComponent`, `AnalysisStatus`, `IncompleteReason`,
  `ModuleInterfaceKind`, `ResolutionKind`, `StatusEntry`, and `StatusScope`.

No source, test, or configuration files were changed. Cache-key identity and
lowering completion/freeze observations were cross-checked against the
existing Chunk 3 report and intentionally not duplicated here.
