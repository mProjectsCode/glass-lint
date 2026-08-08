# Codebase Readability Audit

## Summary

Chunk 6 owns the matching boundary between immutable semantic facts and
query-selected evidence. The index and overlay representations are carefully
bounded and deterministic, but several contracts remain encoded in call-site
protocols: identity inputs can diverge inside one matcher context, indexes are
temporarily observable before normalization, a complete physical plan is
accepted by an API that silently handles only some root kinds, and a typed
evidence-capacity error is converted into a panic. These are architectural
API issues rather than local implementation style.

## Findings

### Matcher identity and overlay ownership

#### [ ] READ-021 — Build matcher context from one coherent identity source

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:140-230`; `glass-lint-core/src/analysis/project/projection.rs:138-163`

`MatcherProjectContext::from_facts` accepts `overlay_identities` for linked
occurrence remapping and a separate `identities` map for constrained argument
evaluation, while `MatcherArtifact::from_facts` and
`MatcherProjectOverlay::from_identities` retain those channels independently.
The production caller currently derives both from the same module-identity
lookup, but the constructor permits different maps, so indexed matching can
use one project identity while fallback argument matching uses another.

**Recommendation:** Make one matcher-input owner derive the occurrence
overlay and evaluator identity view from a typed set of project inputs; keep
call-result identities as a separately named input because they are a
different value-resolution domain. The constructor should make the optional
overlay policy explicit without accepting two independently supplied module
identity references, and the context should retain the invariant stated in
its documentation. Preserve the distinction between “no overlay requested”
and an incomplete/unknown identity map: either must continue to fail closed.

**Fix Applied:** None so far.

### Occurrence-index lifecycle

#### [ ] READ-022 — Seal occurrence indexes after normalization

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/matching/build.rs:68-101`; `glass-lint-core/src/analysis/facts/mod.rs:605-615`; representative callers `glass-lint-core/src/analysis/matching/arguments/mod.rs:459-464` and `glass-lint-core/src/analysis/matching/mod.rs:493-495`

`OccurrenceIndexes::build_from_stream` and `normalize_occurrences` are
separate crate-visible mutations, although the matching query APIs depend on
the latter having run to establish sorted, deduplicated buckets. The protocol
is repeated in production construction and several test builders, and an
intermediate index can be borrowed by a matcher without any type-level or API
state distinguishing it from a normalized index.

**Recommendation:** Put fact projection and normalization behind one
constructor or consuming `finish` operation that returns the queryable index;
make the raw recording phase private to that construction path. Preserve the
empty disabled-phase index and the existing event/span ordering, but expose
only the sealed form to `SemanticFacts::matcher_index` and overlay builders.
Delete the repeated `build_from_stream`/`normalize_occurrences` call protocol
from production and test callers.

**Fix Applied:** None so far.

### Physical-plan and query-facing boundaries

#### [ ] READ-023 — Do not silently discard unsupported physical roots

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/matching/query/mod.rs:36-90`; `glass-lint-core/src/analysis/project/projection.rs:428-445`

`OccurrenceIndexes::evidence_for_with_overlay` accepts a complete
`CompiledMatcherPlan`, but its root match deliberately ignores
`ConstrainedScan` and `Lifecycle` variants. `ProjectModuleProjection::evidence_for`
then appends constrained evidence produced by a separate fact-stream path,
while lifecycle evidence comes from flow projection. This split is valid, but
the accepted plan type and method name imply complete plan evaluation, so a
new caller can omit one projection phase and receive a plausible partial
result rather than an explicit unsupported-root outcome.

**Recommendation:** Pass a typed collection/iterator containing only the
indexed, returned-subject, and instance-subject roots to the occurrence-index
owner, or return a partitioned plan whose other phases must be consumed by
their owning project/flow APIs. Keep the separate constrained and lifecycle
lifecycles, but make partial evaluation visible in the type or result instead
of relying on the comment and a silent no-op match arm. Remove the broad
complete-plan entry point once the caller uses the typed partition.

**Fix Applied:** None so far.

### Evidence capacity and failure propagation

#### [ ] READ-024 — Preserve typed evidence-capacity errors at the matcher boundary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:278-313`; `glass-lint-core/src/api/classification.rs:218-305`; `glass-lint-core/src/analysis/project/projection.rs:329-379`

`RuleEvidenceTable::record` explicitly returns `RuleEvidenceError`, but
`push_owned_rule_evidence` calls it with `expect`, and
`compute_constrained_evidence` returns `()` rather than carrying the failure
to `project_facts`. The catalog-derived capacity normally makes the index
valid, yet a stale root/table pairing, malformed internal plan, or future
projection change turns a modeled boundary error into a process panic.

**Recommendation:** Return a matcher/projection error from constrained
publication and propagate it through `project_facts` and the project
projection outcome, mapping capacity mismatch to the existing compiler or
analysis diagnostic channel. Keep `RuleIndex` and `RuleEvidenceCapacity`
validation as the normal guardrail, but do not discard the explicit error
type or rely on a hidden catalog-capacity invariant at the final write.
Preserve bounded evidence behavior and deterministic ordering on successful
paths.

**Fix Applied:** None so far.

## Systemic Themes

- Matching has several correct but separately assembled views of one semantic
  artifact: fact stream, occurrence index, linked occurrence overlay, module
  identities, and call-result identities. Constructors should own the
  compatibility rules instead of making project projection assemble them by
  convention.
- Phase ownership is split intentionally between indexed matching, constrained
  fact scans, and lifecycle flow projection. The APIs should expose that
  partition so a partial result cannot be mistaken for a complete plan result.
- Deterministic normalization and bounded evidence are valuable invariants,
  but their current construction/error protocols are procedural. Sealed
  construction and typed propagation would make the invariants easier to
  preserve during future matcher additions.

## Open Questions

- Should a compiled physical plan expose typed root partitions, or should each
  matching owner receive a dedicated plan view that cannot contain roots it
  does not execute?
- Should an incomplete matcher projection be represented as a result error or
  as a status-bearing evidence table, given that empty evidence is a valid
  analyzed outcome elsewhere?
- Can the shared module identity map be retained by both indexed remapping and
  constrained evaluation without cloning, while still making the optional
  overlay policy explicit in the matcher-input type?

## Coverage

Reviewed only Chunk 6, “Matching,” from `CODEBASE_STRUCTURE_CORE.md`,
including occurrence index construction and normalization, direct and
project-linked occurrence views, identity maps, event-index resolution,
argument-constrained evaluation, and evidence publication. Existing Chunk 1
through Chunk 5 audit history was used to continue IDs at READ-021. No source,
test, configuration, dependency, or other documentation files were changed;
this chunk audit file is the only new artifact.
