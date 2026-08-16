# Codebase Readability Audit

## Summary

This audit covers Chunk 22 ("Configuration, parsing, and runtime environment")
of `glass-lint-core`: `config.rs`, `diagnostic.rs` (with
`diagnostic/tests.rs`), `ecma_version.rs` (with `ecma_version/detector.rs`),
`environment.rs`, `limits.rs`, and `parse.rs` (with `parse/depth.rs`). It is
read-only; no source was modified. Only
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_22.md` was created; the pre-existing
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_*.md` files from parallel sessions were
left untouched.

The chunk is well structured overall. `CoreConfig` is a small validated
aggregate consumed by the CLI; `SourceLineIndex` centralizes byte-to-position
conversion with a justified ASCII fast path and lazily computed checkpoints;
`Environment`/`EnvironmentInner`/`GlobalObjectMembers` is a clean
value-equality cheap-clone layering consumed by providers; the two-phase
`SyntaxDepthGuard`/`DepthScanner` design bounds hostile nesting before SWC sees
the input and is not over-engineered; and the validated `AnalysisLimits` family
preserves fail-closed positivity plus a separate deserialization path. The
`ValidatedByteOffset`/`ValidatedByteRange` newtype pair is private, sound, and
kept inside `SourceLineIndex`; it is not over-built.

Findings below target four real, narrow issues: a hand-rolled `PositiveLimit`
that re-implements `NonZeroUsize` while sibling limits in the same module
validate the same invariant a second way; a now-drifted two-vocabulary parse
failure message where the project completion status loses the numeric detail
carried by the standalone `ParseDiagnostic`; an unused public re-export of
`SourceLineIndex` (`new`/`from_text`/`try_range` have no consumer outside this
crate); and a per-error full line-index rebuild on the parser diagnostic path.

## Findings

### Limits (`limits.rs`)

#### [ ] READ-001 — `PositiveLimit` re-implements `NonZeroUsize`, and the same non-zero invariant is enforced two ways in one module

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Newtype
- **Location:** `glass-lint-core/src/limits.rs:54-69, 90-94, 108-123, 166-186`

`PositiveLimit` is a one-field `struct PositiveLimit(usize)` whose only
operations are `new()` (rejecting zero) and `get()`. That is exactly the
contract of `std::num::NonZeroUsize`, whose `.get()` is a no-op and whose
construction cannot hold zero. Meanwhile `ProjectAdmissionLimits` (lines
91-94, 108-123) validates the identical "must be positive" invariant by hand
(`if max_sources == 0 { return Err(...) }`) against raw `usize` fields, so the
two sibling validated-limit types in the same file enforce one invariant with
two representations. `PositiveLimit::new` also returns `Result<Self, ()>`,
forcing every `AnalysisLimits::with_*` builder through
`Self::validated(...).map_err(|()| error)` (lines 181-186, 267-276) and every
`Default` through seven `.unwrap()` calls whose failure is statically
impossible (lines 166-178).

**Recommendation:** Delete `PositiveLimit` and store `NonZeroUsize` in
`AnalysisLimits`, or keep one shared positivity carrier and reuse it for the
`ProjectAdmissionLimits` fields so the invariant is proven in exactly one type.
Guardrails: keep the typed error mapping (`AnalysisLimitError` /
`ProjectAdmissionLimitError` per field), the `Copy`-ability of
`ProjectAdmissionLimits`, and the existing serde shape (`limits.rs:279-316`),
which can keep routing through the validated builders unchanged.

**Fix Applied:** None so far.

### Parsing diagnostics (`parse.rs`, `analysis/semantic/status.rs`)

#### [ ] READ-002 — Parse-failure messages are authored in two places and have drifted for the same failure kinds

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/parse.rs:100-113, 199-209, 270-280`; `glass-lint-core/src/analysis/semantic/status.rs:219-222`

`ParseFailureKind::diagnostic()` (parse.rs:100-113) pairs each kind with a
canonical `&'static str` ("source exceeds the analysis limit", "source exceeds
the nesting-depth analysis limit"). Every constructor in `parse.rs` ignores
that text: `validate_source` (199-209) writes "source exceeds the
{MAX_SOURCE_BYTES} byte analysis limit" and `syntax_depth_diagnostic`
(270-280) writes "…{limit} nesting-depth analysis limit". The status-layer
report path then reuses only the static strings (status.rs:219-222), which
lack the byte/depth numbers. The result is two vocabularies for the same
`SourceTooLarge` / `SyntaxDepth` conditions, with the richer wording appearing
only in the standalone `ParseDiagnostic` and the context-free wording in the
project completion status.

**Recommendation:** Make the canonical text a single owned formatter on
`ParseFailureKind` (or `ParseDiagnostic`) that takes the relevant context
(byte limit / depth limit), and have both `parse.rs` constructors and
`status.rs:219-222` route through it. Guardrails: keep the three kinds, the
`DiagnosticKind` mapping used by the report schema (`code.rs`), and the
distinction between the user-facing parse diagnostic (with range) and the
completion-status code; only the message text should be consolidated.

**Fix Applied:** None so far.

#### [ ] READ-003 — `parser_range` rebuilds a full `SourceLineIndex` for a single error span

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/parse.rs:299-309`

On a parser error, `parser_range` constructs
`SourceLineIndex::from_text(self.source.source().clone())` (parse.rs:306-307)
— a complete line-start index (with the `SourceText` `Arc` copy) — to convert
two offsets to one `SourceRange`, even though `SourceParser` already carries
the source text, and both `parse()` (parse.rs:213) and
`SpanNormalizer::with_index` (analysis/semantic/mod.rs:194) build the same
index on every successful parse. The rebuild is bounded (only the single
materialized SWC error does this) but it is unnecessary work that does not
share the index machinery the success path already pays for.

**Recommendation:** Convert the two offsets against the parser's existing
`file`/`start_pos` state with a narrow local compute, or lazily build and share
one index via `OnceLock` on `SourceParser` so the diagnostic path never
allocates a second line index per source. Guardrails: keep returning `None`
for dummy and out-of-order spans, keep returning
`InvalidSourceBoundary::OutOfBounds` on failure, and keep the existing
`parser_range` test coverage (parse/tests.rs:53-77).

**Fix Applied:** None so far.

### Diagnostics (`diagnostic.rs`, `lib.rs`)

#### [ ] READ-004 — `SourceLineIndex` is re-exported publicly with no consumer outside `glass-lint-core`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/lib.rs:29`; `glass-lint-core/src/diagnostic.rs:114-241`

`lib.rs:29` re-exports `SourceLineIndex` next to `RuleMetadata` and `Severity`,
with public constructors `new(&str)` and `from_text(SourceText)` plus
`try_range` (diagnostic.rs:114-241). A full-workspace search shows every use of
`SourceLineIndex` is inside `glass-lint-core` (parse.rs, analysis/semantic,
analysis/local, matching/evidence, lint/report/evidence, project/report); the
providers, CLI, output, and harness crates consume report `SourceRange`/byte
ranges instead. `RuleMetadata` and `Severity` are genuinely consumed by the
providers and CLI rules documentation, so only `SourceLineIndex` is exported
publicly while still being engine plumbing.

**Recommendation:** Drop `SourceLineIndex` from the `lib.rs` re-export and keep
the constructors/`try_range` crate-internal, or explicitly document and test it
as an intended integration contract (e.g., in `tests/integration/public_surface.rs`,
which today does not reference it). Guardrails: preserve `SourceLineIndex::from_text`
as the owned `SourceText` path used by the project boundary, and keep the
`new`/`from_text` equivalence test (diagnostic/tests.rs:60-80) until one
constructor is removed.

**Fix Applied:** None so far.

## Systemic Themes

- **Validated positive-limit family settled into two shapes.** `AnalysisLimits`
  uses a dedicated newtype with a typed error mapping; `ProjectAdmissionLimits`
  uses hand-checked `usize` fields; `FlowLimits`
  (`analysis/model/flow/limits.rs`) scales its internal counters from
  `AnalysisLimits::flow_operations()`. Each is individually documented, but a
  single "validated non-zero bound" carrier would make the invariant uniform.
- **Parse-failure wording lives next to the kind definition yet is never used
  there.** `ParseFailureKind::diagnostic()` returns a `(DiagnosticKind,
  &'static str)` pair whose second element is only read by the completion-status
  path; every `parse.rs` constructor supplies its own prose.
- **Line-index construction is repeated at several entry points** (parse
  success, parser diagnostics, `SpanNormalizer`, `LocatedSourceContext`), all
  funneling through `SourceLineIndex::from_text`; the index is cheap to build
  but is rebuilt for transient purposes on the diagnostic path.

## Open Questions

- `analyze_ecma_version` / `analyze_ecma_version_with_limits`,
  `EcmaVersionReport`, and `FeatureDetector` are public API exercised only by
  core's own tests and `tests/integration/public_surface.rs`; no provider or CLI
  path calls them and the detector performs its own SWC visitor traversal. Is a
  provider-front-end consumer planned, and should the walk be derived from the
  shared artifact (parse-once) rather than a fresh `parse_program_only` per
  call?
- `Linter::with_project_limits` / `ProjectAdmissionLimits`
  (`lint/linter.rs:85`) are only set by test helpers; the CLI configures
  admission through `ProjectConfig` / `ValidatedProjectLoadOptions` in
  `glass-lint-project` instead. Are direct-session admission limits intended to
  be CLI-configurable, or is the public builder a placeholder?
- `SyntaxDepthOutcome::WithinLimit(usize)` carries the measured maximum depth
  that production only consumes via `is_exceeded()`; the payload exists for the
  `#[cfg(test)]` depth-counting assertions in `parse/tests.rs`. Should the
  payload be `#[cfg(test)]`-gated or dropped in favor of a dedicated test
  accessor?
- The `SourceLineIndex` checkpoint scheme (per-line intervals above 256 bytes,
  computed only on the first non-ASCII lookup) is a performance trade-off; is
  the complexity justified versus computing columns directly, and is it part of
  the intended integration contract?
- Environment identifier validation layers `$`/`_` handling on top of SWC's
  `Ident::is_valid_start/is_valid_continue` (environment.rs:117-129); confirm
  whether SWC already accepts these characters so the local branches can be
  removed.

## Coverage

Read (full): configuration (`config.rs`), diagnostics and severity
(`diagnostic.rs`, `diagnostic/tests.rs`), ECMAScript detection
(`ecma_version.rs`, `ecma_version/detector.rs`, `ecma_version/tests.rs`),
environment (`environment.rs`, `environment/tests.rs`), limits (`limits.rs`,
`limits/tests.rs`), parsing (`parse.rs`, `parse/depth.rs`, `parse/tests.rs`).
Traced callers: `lib.rs` re-exports and `parse_test_source`; CLI wiring
(`glass-lint-cli/src/config.rs` `CoreConfig`/`with_limits`/`rule_selection`);
provider environment construction (`glass-lint-js/src/lib.rs`,
`glass-lint-obsidian/src/lib.rs`); project loader language selection
(`glass-lint-project/src/options.rs`); semantic analysis boundary and cache key
(`analysis/semantic/mod.rs`, `project/session/mod.rs`, `analysis/local.rs`);
report/evidence consumption (`lint/report/evidence.rs`,
`analysis/matching/evidence.rs`, `analysis/model/module.rs`,
`lint/report/files.rs`, `analysis/semantic/status.rs`); public-surface
integration test (`tests/integration/public_surface.rs`).