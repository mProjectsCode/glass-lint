# Codebase Readability Audit — glass-lint-core Chunk 15: Matching and argument evaluation

## Summary

Chunk 15 owns the `analysis::matching` module in `glass-lint-core`: occurrence
indexes and their normalization (`occurrence`, `indexes`, `build`), indexed
query resolution with project overlays (`query`, `mod`, `identity_map`),
deterministic evidence normalization (`evidence`), and argument-constrained
matcher evaluation (`arguments`, `arguments/evaluator`, `arguments/identity`).

The chunk is overall well-bounded: occurrence normalization is centralized in
one container, `EvidenceGroup` gives a single occurrence-to-evidence
conversion, overlay resolution is consistently fail-closed, and operation
counts are threaded deterministically. The concrete readability problems
cluster in the argument-evaluation boundary (`arguments/*`): two types with
identical shape (`MatcherProjectOverlay` / `EffectiveIdentityResolver`) are
destructured and rebuilt at every layer, one grouping struct
(`MatcherEvaluationContext`) is created and immediately destructured, the
`ConstrainedState` phase machine leans on `mem::replace` recovery plus a
needless clone, and the public entry point takes `impl Borrow<MatcherArtifact>`
only so tests can pass an owned value. A large `#[cfg(test)]` index-test facade
also duplicates the real fact-to-index projection in `build.rs`.

Findings are ordered by implementation dependence: API-shape consolidation in
the `arguments` boundary first, then the constrained-root phase machine, then
the test facade and leftover markers.

## Findings

### Matching / argument evaluation boundary (`analysis/matching/arguments`)

#### [ ] READ-001 — `EffectiveIdentityResolver` is a parallel type to `MatcherProjectOverlay`; the identity pair is destructured and rebuilt at each layer

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:103-106,159-170`; `glass-lint-core/src/analysis/matching/arguments/mod.rs:230-233,350-356`; `glass-lint-core/src/analysis/project/projection.rs:170`

`MatcherProjectOverlay` (`arguments/mod.rs:230-233`) and
`EffectiveIdentityResolver` (`evaluator.rs:103-106`) have byte-identical
shapes: `identities: Option<&ModuleIdentityMap>` plus
`result_identities: Option<&BTreeMap<ValueId, ExportResolution>>`. The pair
travels the pipeline as a construct–destructure–reconstruct chain:
`MatcherProjectOverlay::new` (`projection.rs:170`) → `try_compute_constrained_evidence`
(`mod.rs:308`) → destructured at `mod.rs:350-353` → `MatcherEvaluator::new(names,
values, identities, result_identities)` (`mod.rs:356`) → `EffectiveIdentityResolver::new`
(`evaluator.rs:168`). Maintaining two owners for the same two-field borrow pair
means every caller learns the tuple shape, and any future change to how
project identities reach the evaluator must be mirrored in both types.

**Recommendation:** Collapse the pair into one owner. Have
`MatcherEvaluator::new` (or `EffectiveIdentityResolver`) accept a
`MatcherProjectOverlay` and destructure once internally, or implement
`From<MatcherProjectOverlay<'a>> for EffectiveIdentityResolver<'a>`; delete the
now-redundant destructure at `arguments/mod.rs:350-353`. Guardrails: both
structs are `Copy` borrow-only bundles with no owned state, so collapsing them
merges no ownership domain; the resolver's documented precedence
(call-result identity, then module identity, then local value) in
`EffectiveIdentityResolver::effective_identity`/`static_string`
(`evaluator.rs:128-149`) must be preserved exactly, and `MatcherProjectOverlay`
remains the production-facing type constructed by `projection.rs:170` and the
argument tests.

**Fix Applied:** None so far.

#### [ ] READ-002 — `MatcherEvaluationContext` is an immediately-consumed grouping struct

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:284-288,314-323,338-348`; `glass-lint-core/src/analysis/matching/arguments/tests.rs:96-104`

`MatcherEvaluationContext` (`arguments/mod.rs:284-288`) bundles
`artifact`, `project`, and `operations` purely to shorten
`compute_constrained_inner`'s parameter list. It is constructed at the single
production call site (`mod.rs:315-322`) and destructured into its three fields
at the top of `compute_constrained_inner` (`mod.rs:343-347`); the only other
construction site is the test helper `run_with_ops`
(`arguments/tests.rs:96-104`). The struct enforces no invariant and adds no
vocabulary beyond the field names; `try_compute_constrained_evidence` is the
only production caller.

**Recommendation:** Pass `artifact: &MatcherArtifact`, `project:
MatcherProjectOverlay`, and `operations: &mut EvaluationOperations` as direct
parameters of the private `compute_constrained_inner` and delete
`MatcherEvaluationContext`; update `run_with_ops` accordingly. Guardrail: the
shared `'borrow` lifetime currently ties the artifact borrow and project borrow
together, but Rust infers the same region when they are separate parameters, so
no lifecycle distinction is lost.

**Fix Applied:** None so far.

#### [ ] READ-003 — `ConstrainedState` phase machine relies on `mem::replace` recovery and a needless clone of the fallback list

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:49-53,85-133,373-443`

`ConstrainedState::{Indexed, Fallback(Vec<Occurrence>), Published}`
(`arguments/mod.rs:49-53`) encodes three phases inside each
`PreparedConstrainedRoot`, with transitions driven by `mark_fallback` /
`is_fallback` / `record_fallback` / `publish` / `publish_fallback`
(`mod.rs:85-133`). `publish_fallback` extracts the list with
`std::mem::replace(..., Published)` and then, on the success path, calls
`publish` with `occurrences.clone()` (`mod.rs:123`) even though the original is
about to be dropped — a full clone of every recorded fallback occurrence purely
to enable restoring state on the error path. `mark_fallback` and
`record_fallback` silently depend on call-order invariants (overwriting any
prior phase, no-op outside `Fallback`) rather than making the transitions
explicit.

**Recommendation:** Make the fallback path a separate, explicit flow: keep
`PreparedConstrainedRoot` for indexed publication and hold the recorded
fallback occurrences in a dedicated list (or `Option<Vec<Occurrence>>` that
only exists during the fallback pass), so `publish_fallback` can move
occurrences instead of cloning and error recovery does not require restoring a
replaced phase. Guardrails: preserve the distinct outcomes — an indexed root
that finds zero candidates is published with no evidence and is never rescanned
(`mod.rs:382-409`), a root that cannot use an index is scanned only in the
bounded linear pass (`mod.rs:414-443`), and a `RuleEvidenceError` from
publication must leave the recorded occurrences recoverable (fail-closed).

**Fix Applied:** None so far.

#### [ ] READ-006 — `try_compute_constrained_evidence` takes `impl Borrow<MatcherArtifact>` and a test-only `MatcherLocalInput` alias so tests can pass an owned artifact

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:282,308-313,326-335`; `glass-lint-core/src/analysis/matching/arguments/tests.rs:55-60,137-142`

The public-in-analysis entry point is generic over
`artifact: impl Borrow<MatcherArtifact<'artifact>>`
(`arguments/mod.rs:308-313`), but production always passes a reference
(`projection.rs:270` passes `matcher_context.artifact()`). The generality only
exists so tests can hand in an owned `MatcherArtifact` built by the
`#[cfg(test)]` constructors `from_parts` / `from_parts_with_overlay`
(`mod.rs:192-211`), using the `#[cfg(test)] type MatcherLocalInput<'a> =
MatcherArtifact<'a>` alias (`mod.rs:282`) that adds no vocabulary. The same
test helper already demonstrates the reference form works
(`tests.rs:98` constructs `&MatcherArtifact::from_parts_with_overlay(...)`).

**Recommendation:** Drop the `Borrow` bound and take `&MatcherArtifact<'artifact>`;
have tests pass `&MatcherLocalInput::from_parts(...)` exactly as `run_with_ops`
already does, and delete the `MatcherLocalInput` alias and the owned-passing
convention in `tests.rs`/`tests/extended.rs`. Guardrail: the `'artifact` inner
lifetime stays tied to the borrowed `FactStream`/`OccurrenceIndexes`, so the
alias removal must not decouple the stream and index borrows that
`MatcherArtifact` intentionally groups.

**Fix Applied:** None so far.

### Matching / indexed query execution

#### [ ] READ-004 — `OccurrenceIndexes` carries a large `#[cfg(test)]` facade that duplicates the real fact-to-index projection

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:41-42,300-376`; `glass-lint-core/src/analysis/matching/query/mod.rs:55-65,242-300`; `glass-lint-core/src/analysis/matching/build.rs:67-83`

`OccurrenceIndexes` embeds a test-only `test_names: NameTable` field
(`mod.rs:41-42`) plus eleven `#[cfg(test)]` helpers (`is_empty`, `has_call`,
`has_import`, `has_string`, `has_any_class`, `has_module_class`,
`has_module_constructor`, `has_constructor`, `has_member_call`,
`has_any_member_call`, `test_name`, `mod.rs:300-376`). In parallel,
`query::record` (`query/mod.rs:242-300`) is a second, test-only projection that
maps `MatchKind` + symbol text straight into the same four index types that
`build.rs::record_fact` (`build.rs:85-157`) populates from semantic facts.
Because the facade re-implements the index shapes, every change to a key type,
bucket, or normalization rule must be mirrored in `record` and the `has_*`
predicates; `OccurrenceIndexes::build_from_stream` (`build.rs:74`) is `pub(super)`
with a single production caller (`mod.rs:279`) only because the "collect then
normalize" invariant documented on `from_stream` (`mod.rs:270-272`) is tested
through `normalize_occurrences` (`build.rs:67`).

**Recommendation:** Reduce the facade to what focused index tests genuinely
need. Prefer parsing real source and using `OccurrenceIndexes::from_stream` in
tests (as `build_from_stream_populates_all_occurrence_indexes` at
`matching/tests.rs:101` already does), fold `query::record` into a smaller
test-only projection, and make `build_from_stream` private while keeping only
`normalize_occurrences` test-visible. Guardrails: do not collapse distinct
index families (calls vs members vs constructions vs literals) or their
deduplication policy — the deterministic `(event, span)` normalization in
`occurrence/storage.rs:88-105` is a real invariant; keep the fail-closed empty
index produced by `from_stream` when availability is disabled.

**Fix Applied:** None so far.

#### [ ] READ-005 — Stale "Phase 7" comment references functions removed during refactoring

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Documentation
- **Location:** `glass-lint-core/src/analysis/matching/query/mod.rs:189-193`

`query/mod.rs:189-193` contains a comment block: "occurrences_for_clause,
occurrences_for_subject, and occurrences_for_event were removed in Phase 7. The
constrained evidence path now uses occurrences_for_indexed directly, and
returned/instance subject lookups use occurrences_for_returned /
occurrences_for_instance." The "Phase 7" label appears nowhere else in the
workspace, and the surrounding code already documents `occurrences_for_indexed`
(`query/mod.rs:123-124`), `occurrences_for_returned` (`:136`), and
`occurrences_for_instance` (`:163`), so the comment only re-states current
routing under an obsolete migration label.

**Recommendation:** Delete the comment; keep the routing documentation on the
three live functions. Guardrail: if the history matters, fold the routing
summary into the module-level doc comment at `query/mod.rs` rather than
preserving a phase label with no current meaning.

**Fix Applied:** None so far.

### Matching / evidence accumulation

#### [ ] READ-007 — Parallel evidence-push helpers `push_owned_evidence` and `push_owned_rule_evidence`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:378-392`; `glass-lint-core/src/analysis/matching/arguments/mod.rs:290-306`; `glass-lint-core/src/analysis/matching/evidence.rs:17-42`

Two helpers build the same conversion — `EvidenceGroup::from_occurrences(kind,
symbol, Definite, occurrences)` then `into_classification` — and differ only in
destination: `push_owned_evidence` (`matching/mod.rs:378-392`, three call sites
at `query/mod.rs:87,100,112`) pushes into `Vec<ClassificationEvidence>` and
returns nothing, while `push_owned_rule_evidence` (`arguments/mod.rs:290-306`,
one call site at `:105`) records into `RuleEvidenceTable` with a `RuleIndex` and
returns `Result<(), RuleEvidenceError>`. The shared conversion is already owned
by `EvidenceGroup`, so the remaining helper bodies are parallel push wrappers
with different table types and iterator inputs (`OccurrenceSelection` vs
`impl IntoIterator<Item = Occurrence>`).

**Recommendation:** Since `EvidenceGroup` is already the narrow owner of the
conversion, either have both call sites inline `EvidenceGroup::from_occurrences`
and push/record directly, or keep a single helper that returns the optional
`ClassificationEvidence` and let each caller record it. Guardrails: keep the
`RuleEvidenceTable::record` fallible path (capacity errors must stay typed) and
keep the `Definite` certainty and `into_ordered`/iterator differences that the
two inputs legitimately require.

**Fix Applied:** None so far.

## Systemic Themes

- **Destructure/rebuild churn in the `arguments` boundary.** The identity pair
  (`MatcherProjectOverlay` → destructure → `EffectiveIdentityResolver`), the
  `MatcherEvaluationContext` bundle, and the `ConstrainedRootInput` →
  `ConstrainedRoot` regroup (`arguments/mod.rs:62-83`) all unpack small
  borrow-structs and repack them one layer later. Several findings above stem
  from this same pattern at different levels.
- **Operation-count threading via `(T, usize)` returns.** `LinkedOccurrenceView::build`
  (`mod.rs:170-210`), `MatcherArtifact::from_facts` (`arguments/mod.rs:161-189`),
  and `MatcherProjectContext::from_facts` (`arguments/mod.rs:246-254`) all
  return `(Self, usize)`, summed at `projection.rs:173`. The convention is
  consistent and documented; it was not flagged as a defect.
- **`#[cfg(test)]` test scaffolding embedded in production types.** Beyond
  `OccurrenceIndexes` (READ-004), `MatcherArtifact::from_parts*`
  (`arguments/mod.rs:192-211`) and the `MatcherLocalInput` alias (READ-006) put
  test construction in the production module, forcing production signatures to
  accommodate test call shapes.
- **Tuple-plus-small-struct mixing in event views.** `EventIndexCapabilities::indexed`
  (`query/view.rs:216-234`) accepts `(ModuleOverlayKind, &ModuleOccurrences)` and
  `(&OccurrenceIndex<NamePath>, &Environment)` tuples and immediately
  destructures them into `ModuleIndex` / `RootedIndex`, while the same references
  are already named fields of `EventIndexView` variants (`query/view.rs:21-63`).
  Low priority because the capabilities layer genuinely owns the resolution
  dispatch across event kinds.

## Open Questions

- `query/mod.rs:116` uses `unreachable!("indexed root iterator yielded a
  non-indexed root")` in production for `PhysicalRoot::ConstrainedScan` /
  `PhysicalRoot::Lifecycle`. The invariant is sound (`IndexedRootIter` only
  yields the three indexed variants, `query/mod.rs:36-49`), but it is a panic
  path in a `pub(in crate::analysis)` surface; is a fail-fast panic preferred
  over silently skipping non-indexed roots if the iterator contract ever
  changes?
- In `MatcherArtifact::from_facts` (`arguments/mod.rs:167-180`), three distinct
  "no overlay" causes — policy `Disabled`, index unavailable, and missing
  `identities` — all collapse to `(None, 0)`. This is correct today; would any
  future operation-count or coverage verification need to distinguish
  "overlay intentionally disabled" from "projection data absent"?
- `module_identities` / `collect_exported_identities` in
  `project/identities.rs:109-220` build `ModuleIdentityMap` mostly through raw
  `insert` calls while `ModuleIdentityContributions` (`identity_map.rs:51-74`)
  exists to separate direct/star precedence. The split is coherent, but the
  direct-vs-star distinction is enforced only by caller discipline — would the
  contribution type be a better owner for `module_identities`'s insert loop too?

## Coverage

Files reviewed (read-only; no source changes):

- `glass-lint-core/src/analysis/matching/mod.rs`
- `glass-lint-core/src/analysis/matching/occurrence.rs`, `occurrence/storage.rs`, `occurrence/tests.rs`
- `glass-lint-core/src/analysis/matching/indexes.rs`
- `glass-lint-core/src/analysis/matching/build.rs`
- `glass-lint-core/src/analysis/matching/evidence.rs`, `evidence/tests.rs`
- `glass-lint-core/src/analysis/matching/identity_map.rs`, `identity_map/tests.rs`
- `glass-lint-core/src/analysis/matching/query/mod.rs`, `query/view.rs`, `query/view/private_network.rs`, `query/view/tests.rs`
- `glass-lint-core/src/analysis/matching/arguments/mod.rs`, `arguments/evaluator.rs`, `arguments/identity.rs`, `arguments/tests.rs`, `arguments/tests/extended.rs`
- `glass-lint-core/src/analysis/matching/tests.rs`
- Callers traced: `analysis/project/projection.rs`, `analysis/project/identities.rs`, `analysis/project/state.rs`, `analysis/facts/mod.rs`, `lint/report/evidence.rs`

Verification performed: traced `MatcherProjectOverlay` /
`EffectiveIdentityResolver` / `MatcherEvaluator` construction and call sites;
confirmed `build_from_stream` has a single production caller; confirmed the
"Phase 7" label appears nowhere else; confirmed `impl Borrow` call sites; ran
`git status --short` after writing this file (only this audit file is new).
