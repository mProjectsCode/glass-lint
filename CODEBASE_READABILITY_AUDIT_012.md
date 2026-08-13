# Codebase Readability Audit — Chunk 12

## Summary

Chunk 12 owns the filesystem-free project boundary: source admission, local
analysis transitions, authored request and resolver-outcome tables, linking
handoff, and public report values. The staged session and report types mostly
encode meaningful lifecycle and validation boundaries: duplicate sources are
rejected atomically, resolver answers are checked against authored requests,
and report combination validates identity before consuming inputs. The
findings below target temporary request representations, cloning at consuming
merge boundaries, an inconsistent borrowed-value API, and an aggregate
resource contract missing from the direct core session API.

## Findings

### Authored request handoff

#### [x] READ-050 — Local request extraction materializes a tuple vector that is immediately split into two collections

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/model/module.rs:401-422`; `glass-lint-core/src/project/session/artifacts.rs:138-152`; callers in `glass-lint-core/src/project/session/mod.rs:130-152`

`ModuleInterface::requests_with_ids` allocates a
`Vec<(ModuleRequestId, ResolutionRequest)>`. `AnalysisArtifacts::record_local`
then walks that vector once to insert every request ID into
`AuthoredRequestTable`, walks it again by consuming it, and allocates a
second `Vec<ResolutionRequest>` for the value returned to the resolver. The
tuple vector has no consumer after this boundary, so every analyzed file with
requests pays for an intermediate collection and a second request-vector
allocation.

**Recommendation:** Make the module-interface owner expose a consuming or
streaming request iterator, and let `record_local` insert each ID while
building the one returned request collection. Alternatively, put the
combined operation on `AuthoredRequestTable` so the table owns the ID
registration and request handoff. Preserve interface order, the
invalid-span filter, the authored-key invariant, and the exact
`AuthoredRequests` values visible to resolver callers; remove only the
immediately consumed tuple vector and second transformation pass.

**Fix Applied:** Module request extraction now streams its owned
`(ModuleRequestId, ResolutionRequest)` pairs to `record_local`, which registers
authored IDs and builds the single resolver-facing request vector in one pass.
Interface order, invalid-span filtering, and authored-key identity are
unchanged. Verified with `make fmt && make ci`.

### Finding duplicate merge ownership

#### [x] READ-051 — Finding deduplication clones evidence from a finding that it already owns

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:135-150`; `glass-lint-core/src/project/types/report/finding.rs:88-98`; `glass-lint-core/src/project/types/report/evidence.rs:136-166`

`merge_duplicate_findings` owns both the previously retained `Finding` and
the current `Finding`, but calls `previous.merge_duplicate(&finding)`,
borrowing the second value. `Finding::merge_duplicate` consumes `self` yet
passes `&self.evidence` to `EvidenceTraces::merge`; that merge clones every
trace from both evidence collections before sorting and deduplicating them.
Duplicate findings can therefore copy the complete retained evidence even
though the merge boundary already owns it and the previous value is removed
from the accumulator.

**Recommendation:** Give the duplicate-merge path an owning operation, such
as `merge_duplicate(self, other: Self)`, and let `EvidenceTraces` consume both
trace vectors before sorting and deduplicating. Preserve trace canonical
ordering, duplicate removal, truncation propagation, certainty promotion,
message/severity selection, and the test-only construction semantics; remove
only the avoidable clone of owned evidence at this consuming boundary.

**Fix Applied:** Duplicate finding merges now consume both owned findings and
their evidence collections. Trace sorting/deduplication, truncation and
certainty propagation, and finding identity selection remain unchanged while
the retained evidence is no longer cloned. Verified with `make fmt && make ci`.

### Borrowed versus owned range APIs

#### [ ] READ-052 — Public range accessors clone values where neighboring path accessors borrow

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/input.rs:315-350,361-379`; `glass-lint-core/src/project/types/report/location.rs:7-27`; `glass-lint-core/src/project/session/mod.rs:355-367`; `glass-lint-project/src/loader_phases.rs:31-82`

`ResolutionRequestKey::range`, `ResolutionRequest::range`, and
`SourceLocation::range` all return a cloned `SourceRange`, while their
neighboring path, key, and location accessors return references. The session
sort invokes `left.range()` and `right.range()` for every comparison, and
project/harness resolution matching also requests owned ranges merely to
compare or forward them. `SourceLocation` already has an internal
`range_ref` helper, demonstrating that the owning type can support borrowed
access without exposing storage; the public request types have no equivalent
borrowed operation.

**Recommendation:** Make borrowed range access the canonical accessor on the
public input and report types, with an explicitly named owned conversion only
where a caller needs to retain a range. Centralize comparison and forwarding
on those borrowed views, preserving the half-open range value, serialization,
equality, and callers that intentionally need ownership; remove repeated
small `SourceRange` clones from sorting and resolution matching.

**Fix Applied:** None so far.

### Direct project-session resource boundary

#### [ ] READ-053 — Direct core sessions have no aggregate source-admission contract

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/session/mod.rs:196-230,291-319`; `glass-lint-core/src/project/tables.rs:13-71`; `glass-lint-core/src/project/types/input.rs:12-50,235-306`; contrasting loader bounds in `glass-lint-project/src/loader.rs:282-290,377-410`

`ProjectSession::analyze_source` and `analyze_sources` accept arbitrary owned
sources and insert them into `SourceTable` before local analysis. The core
session has no maximum source count, aggregate source-byte budget, or
admission bound on the retained `SourceTable`; `SourceText` can hold any
caller-provided string and the parser’s per-file limit is reached only after
the source has been admitted and scheduled. The filesystem project loader
does enforce `max_files`, per-file bytes, and aggregate project bytes, so the
same core session is bounded when reached through that loader but not through
its public direct API, despite core’s architecture requiring bounded work and
intermediate state.

**Recommendation:** Put an explicit validated aggregate-admission policy on
the core session boundary. Core’s architecture promises bounded work and
intermediate state, so documenting an unbounded direct API would leave that
contract contradictory. Keep the core policy separate from filesystem
discovery and loader budgets, reject or defer admission atomically before
retaining sources, and preserve parse-failure-as-report behavior, cache reuse,
worker bounds, deterministic ordering, and the project loader’s stricter
policies.

**Fix Applied:** None so far.

## Systemic Themes

- Phase handoffs should transfer one domain representation. The authored
  request table owns request identity, so extraction should not first create a
  second tuple-owned representation solely to split it apart.
- Consuming APIs should consume owned evidence and reports when the caller has
  already removed the values from their previous owner. Borrowed convenience
  methods at those boundaries can silently turn bounded evidence work into
  repeated full clones.
- Public domain values should make borrowing the default and make ownership
  acquisition explicit. This keeps internal deterministic sorting and external
  resolver adapters on the same representation.
- Filesystem loading and core project sessions are separate ownership domains,
  but both expose source-admission boundaries. Their aggregate resource
  contract must either be enforced at the core boundary or documented as an
  intentional caller responsibility.
- Atomic source insertion, typed resolution outcomes, staged session errors,
  and report schema/path validation were treated as real invariants rather
  than simplifications merely because they introduce wrapper types.

## Open Questions

- The direct core API must own aggregate source admission because the core
  architecture promises bounded intermediate state; its limits must remain
  provider- and filesystem-neutral.
- Add borrowed `range_ref`-style accessors while retaining an explicitly named
  owned conversion where external callers need ownership; preserve the
  current serialized and equality behavior during the API migration.
- `AnalysisReport` finalization postconditions are unrelated to the three
  retained findings and remain unchanged.
- Request extraction should build one bounded returned collection while
  registering authored IDs in the same pass; duplicate-evidence merging should
  consume owned traces at its existing bounded sort/dedup boundary.

## Coverage

Reviewed the chunk-12 structure entries and their implementation/test support:

- `project/{input,mod,tables}.rs`
- `project/session/{mod,artifacts,execution}.rs`
- `project/types/input.rs` and `project/types/mod.rs`
- `project/types/report/{analysis_report,code,diagnostic,evidence,file_report,finding,location,operations}.rs`
- `project/report/mod.rs` and report/session integration tests
- direct core-session callers in the project loader, harness adapters, and
  core integration tests
- existing numbered audit reports 001–011 were checked to avoid duplicating
  their historical findings.

No source, test, configuration, dependency, or other documentation files
were changed by this audit.
