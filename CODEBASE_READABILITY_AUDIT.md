# Codebase Readability Audit

## Summary

The recent `ProjectSession::finish` cleanup exposed the clearest remaining
opportunities in the filesystem loader and profiling boundary. The loader still
has a short-lived closed-frontier phase object and a one-use result carrier;
profiling also wraps a core value without adding profile semantics. Rule
selection has a smaller but concrete duplicated validation path.

## Findings

### Project loading lifecycle

#### [x] READ-001 — Collapse the closed-frontier finishing phase

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-project/src/loader.rs:307-347`, `glass-lint-project/src/loader.rs:505-558`

`ProjectLoadState::close_frontier` consumes the state and always packages most
of it into `ClosedFrontier`, while the caller separately receives a
`Result<(), ProjectLoadError>` and immediately calls `ClosedFrontier::finish`
with one of two `FinishMode` values (`loader.rs:158-167`). The wrapper's
`finish` method only performs the complete-load deadline check before delegating
to `finish_inner`; the actual finalization is otherwise identical.

**Recommendation:** Let the consumed `ProjectLoadState` own the finalization
path and pass the frontier-expansion result into one finishing operation. Delete
`ClosedFrontier`, `FinishMode`, and the `finish`/`finish_inner` split, while
keeping the existing `ProjectSession::finish`, resolver outcomes, diagnostics,
source retention, and metrics updates in that owner. Preserve the distinction
that a complete load timing out is fatal, whereas a recoverable expansion error
still produces a partial report from all successfully analyzed sources.

**Fix Applied:** Moved frontier completion and report finalization back onto
`ProjectLoadState`, deleting `ClosedFrontier`, `FinishMode`, and the split
`finish`/`finish_inner` path. Verified with `cargo test -p glass-lint-project`.

#### [x] READ-002 — Remove the one-use request-resolution carrier

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-project/src/loader.rs:247-252`, `glass-lint-project/src/loader.rs:450-481`

`RequestResolutionOutcome` is constructed by `resolve_requests` and consumed
once by `apply_request_resolution`. It only carries an internal-target vector
and the elapsed duration; no invariant or lifecycle boundary is attached to the
type, so it adds a named allocation of state between two methods that already
belong to the same `ProjectLoadState`.

**Recommendation:** Return the two values directly, or have
`resolve_requests` enqueue the internal targets and return only the measured
duration. Keep resolver-cache hits from contributing to resolution timing and
retain deterministic request order and the existing deadline checks.

**Fix Applied:** Replaced `RequestResolutionOutcome` with a direct tuple return
and passed the internal-target vector directly to the applying method. Verified
with `cargo test -p glass-lint-project`.

#### [x] READ-003 — Transfer load metrics without a clone-only snapshot

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-project/src/loader.rs:116-127`, `glass-lint-project/src/loader_metrics.rs:122-124`

`ProjectLoadMetrics::snapshot` is a direct `self.clone()` used only when
`load_and_lint` assigns the finished metrics to the outcome. The outcome is
already mutable at that point, so the snapshot method adds a copy and a second
API surface without providing an immutable point-in-time boundary.

**Recommendation:** Move the metrics value into the outcome after recording the
total, or otherwise make the ownership transfer explicit, and delete
`snapshot`. Keep `ProjectLoadMetrics: Clone` only if independent callers still
need actual copies; the public values and budget accounting should remain
unchanged.

**Fix Applied:** Transferred the completed metrics value directly into
`ProjectLoadOutcome` and removed the clone-only `snapshot` method and test
terminology. Verified with `cargo test -p glass-lint-project`.

### Profiling boundary

#### [x] READ-004 — Use core operation counts directly in the harness

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-harness/src/profile/types.rs:123-187`, `glass-lint-core/src/project/types/report/operations.rs:1-77`

`ProfileOperationCounts` is a one-field wrapper around
`AnalysisOperationCounts`. It forwards every accessor, implements only the
same addition behavior, and converts back and forth at each report aggregation
boundary (`glass-lint-harness/src/profile/metrics.rs:10-38`); it introduces no
profile-specific invariant or vocabulary.

**Recommendation:** Store `AnalysisOperationCounts` directly in profile
summaries and accumulators, then remove the forwarding methods, conversion, and
wrapper re-export. Preserve the core `AddAssign` semantics, especially the
maximum aggregation for live alternatives and saturating counters; update the
harness public API deliberately because this is a breaking type simplification.

**Fix Applied:** Replaced `ProfileOperationCounts` throughout the harness
profile summaries and accumulators with core `AnalysisOperationCounts`, removed
the forwarding façade and re-export, and kept core aggregation semantics.
Verified with `cargo test -p glass-lint-harness`.

### Rule selection

#### [x] READ-005 — Centralize catalog evaluation and override validation

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/selection.rs:310-331`

`RuleSelection::validate`, `prepare`, and `resolve` each evaluate the catalog
and validate the matched overrides before returning different projections of
the same `SelectionEvaluation`. The repeated two-step invariant is exercised
by CLI configuration (`glass-lint-cli/src/config.rs:282-293`) and linter
construction (`glass-lint-core/src/lint/linter.rs:126-138`), so future changes
to selector validation can drift between entry points.

**Recommendation:** Add one private validated-evaluation operation and make the
three public/internal entry points project its result, or make `prepare` the
canonical path and derive the other projections from it. Preserve declaration
order for overrides, unmatched wildcard errors, exact unknown-rule errors, and
the catalog alignment captured by `PreparedRuleSelection`.

**Fix Applied:** Added one private `validated_evaluation` path and made
`validate`, `prepare`, and `resolve` project its result. Verified with
`cargo test -p glass-lint-core --test integration linter`.

## Systemic Themes

- Short-lived phase structs are useful when they enforce an ownership or
  uncertainty invariant; `ClosedFrontier` does not currently add one beyond the
  result and deadline checks already held by its caller.
- Several domain types correctly hide storage and enforce invariants, notably
  `SourceTable`, `ResolutionTable`, `AuthoredRequests`, and the accepted-path
  boundary types. They were not reported as wrappers merely because they have
  one field.
- The strongest search signals for the next cleanup pass are one-field structs,
  types with a single construction and consumption site, clone-only
  `snapshot` methods, and repeated `evaluate`/`validate` sequences.

## Open Questions

- `ProjectAnalysis` remains a two-field core result whose report is consumed
  directly by most callers while `glass-lint-project` is the only production
  caller that extracts linking and matching timings. If phase timings are
  profiling-only, consider moving their collection out of the core public
  result; if they are part of the core contract, keep the wrapper as the
  explicit report-plus-timing boundary rather than introducing another
  compatibility method.

## Coverage

Reviewed the workspace agent instructions and architecture/testing/contribution
guidance; mapped the core, project, harness, and datastructures public
boundaries; searched for wrappers, `into_parts`/`snapshot` conversions,
phase-transition structs, repeated validation, and public storage access; and
traced representative callers for each finding. No source, test, configuration,
dependency, or documentation file other than this audit was changed.
