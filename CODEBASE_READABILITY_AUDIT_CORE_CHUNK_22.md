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
failure message whose canonical static text is stranded — the completion-status
path deliberately skips parse failures, and every `parse.rs` constructor authors
its own richer wording; an unused public re-export of `SourceLineIndex`
(`new`/`from_text`/`try_range` have no consumer outside this crate); and a
per-error full line-index rebuild on the parser diagnostic path.

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
(`if max_sources == 0 { return Err(...) }`, limits.rs:113-118) against raw
`usize` fields, so the two sibling validated-limit types in the same file
enforce one invariant with two representations. `PositiveLimit::new` also
returns `Result<Self, ()>`, forcing every `AnalysisLimits::with_*` builder
through `Self::validated(...).map_err(|()| error)` (limits.rs:181-186,
267-276) and every `Default` through seven `.unwrap()` calls whose failure is
statically impossible (limits.rs:166-178).

**Recommendation:** Delete `PositiveLimit` and use `std::num::NonZeroUsize` as
the single shared positivity carrier in **both** `AnalysisLimits` and
`ProjectAdmissionLimits`, so the "must be positive" invariant is proven by
exactly one type in the module (the standard library type). `AnalysisLimits`
fields (limits.rs:79-85) become `NonZeroUsize`; `ProjectAdmissionLimits` fields
(limits.rs:92-93) become `NonZeroUsize` as well, and its `new` (limits.rs:109-123)
replaces the two `if ... == 0` checks with `NonZeroUsize::new(...).ok_or(...)`
per field. This is the minimal root fix: it removes the hand-rolled newtype and
the second hand-checked representation together, rather than leaving the
`ProjectAdmissionLimits` hand-check in place while only swapping the `AnalysisLimits`
carrier. Guardrails: keep the typed error mapping (`AnalysisLimitError` /
`ProjectAdmissionLimitError` per field, including the `Default` path at
limits.rs:166-178 whose constant non-zero defaults make the unwraps equally
impossible on the standard type), the `Copy`-ability of `ProjectAdmissionLimits`
(`NonZeroUsize` is `Copy`), and the existing serde shape (limits.rs:279-316),
which can keep routing through the validated builders unchanged; the `.get()`
accessors (limits.rs:125-131, 188-214) need no changes.

**Fix Applied:** None so far.

### Parsing diagnostics (`parse.rs`, `analysis/semantic/status.rs`)

#### [ ] READ-002 — Parse-failure messages are authored in two places and have drifted for the same failure kinds

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/parse.rs:100-113, 199-209, 270-280`; `glass-lint-core/src/analysis/semantic/status.rs:167-175, 219-222`

`ParseFailureKind::diagnostic()` (parse.rs:100-113) pairs each kind with a
canonical `&'static str` ("source exceeds the analysis limit", "source exceeds
the nesting-depth analysis limit"). `ParseDiagnostic::new` reads only the
`DiagnosticKind` half of that pair (`.0`, parse.rs:51); every constructor in
`parse.rs` authors its own richer prose instead: `validate_source` (199-209)
writes "source exceeds the {MAX_SOURCE_BYTES} byte analysis limit" and
`syntax_depth_diagnostic` (270-280) writes "…{limit} nesting-depth analysis
limit". The static text's only reader is the `ParseFailure` arm of
`IncompleteReason::diagnostic()` (status.rs:219-222), which is unreachable in
production: `AnalysisStatus::diagnostics()` deliberately skips `ParseFailure`
entries (status.rs:167-175) and the report presents the standalone
`ParseDiagnostic` itself (lint/report/files.rs:24-33 via
project/types/report/diagnostic.rs:51-53). The result is two vocabularies for
the same `SourceTooLarge` / `SyntaxDepth` conditions — the richer wording
reaches users only through the standalone `ParseDiagnostic`, while the
canonical static wording is stranded dead text that has drifted from the real
messages without any test noticing.

**Recommendation:** Make the message text a single owned formatter on
`ParseFailureKind` (or `ParseDiagnostic`) that takes the relevant context
(byte limit / depth limit), and have both `validate_source` (parse.rs:205) and
`syntax_depth_diagnostic` (parse.rs:273-276) route through it. For the dead
status arm, stop consuming a separate static string: since the variant exists
only for completion tracking (status.rs:167-175), its message can come from the
same formatter or a plain fallback — either way the static payload at
parse.rs:104-111 is deleted rather than re-derived, which is the root fix.
Guardrails: keep the three kinds, the `DiagnosticKind` mapping used by the
report schema (`code.rs`), and the deliberate separation between the
user-facing parse diagnostic (with range) and the completion-status code
(status.rs:167-175); only the message text should be consolidated.

**Fix Applied:** None so far.

#### [ ] READ-003 — `parser_range` rebuilds a full `SourceLineIndex` for a single error span

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/parse.rs:299-309`; `glass-lint-core/src/parse.rs:213, 306-307`; `glass-lint-core/src/analysis/semantic/mod.rs:194`

On a parser error, `parser_range` constructs
`SourceLineIndex::from_text(self.source.source().clone())` (parse.rs:306-307)
— a complete line-start index plus the `SourceText` `Arc` copy (SourceText is
`Arc<str>`, project/types/input.rs:18) — to convert two offsets to one
`SourceRange`. The success path already builds this exact index once in
`parse()` (parse.rs:213) and then shares it (as `Arc<SourceLineIndex>`) through
`SpanNormalizer::with_index` (analysis/semantic/mod.rs:194) and
`LocatedSourceContext::from_normalizer` (analysis/local.rs:114-122); only the
error path fails to reuse it and rebuilds a fresh index per `ParseDiagnostic`.
The rebuild is bounded (only the single materialized SWC error does this) but it
is unnecessary work that does not share the index machinery the success path
already pays for, and `parse_program_only` (parse.rs:224-227) intentionally
skips the index, so the diagnostic path is the only place the cost is doubled.

**Recommendation:** Build one index lazily per parser with a
`OnceLock<SourceLineIndex>` on `SourceParser` and have both `parser_range`
(parse.rs:306) and `parse()` (parse.rs:213) read it, so the index is
constructed at most once per source across the success and failure paths while
`parse_program_only` still avoids it. Do **not** convert the two offsets with a
narrow local compute — that would re-derive the line/column logic (including
char-boundary validation and the ASCII/Unicode column rules,
diagnostic.rs:134-165) a second way, scattering the very logic `SourceLineIndex`
exists to centralize. Guardrails: keep returning `None` for dummy and
out-of-order spans, keep returning `InvalidSourceBoundary::OutOfBounds` on
failure, and keep the existing `parser_range` test coverage (parse/tests.rs:53-77).

**Fix Applied:** None so far.

### Diagnostics (`diagnostic.rs`, `lib.rs`)

#### [x] READ-004 — `SourceLineIndex` is re-exported publicly with no consumer outside `glass-lint-core`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/lib.rs:29`; `glass-lint-core/src/diagnostic.rs:114-241`

`lib.rs:29` re-exports `SourceLineIndex` next to `RuleMetadata` and `Severity`,
with public constructors `new(&str)` (diagnostic.rs:116-118) and
`from_text(SourceText)` (diagnostic.rs:123-125) plus `try_range`
(diagnostic.rs:231-234). A full-workspace search shows every use of
`SourceLineIndex` is inside `glass-lint-core` (parse.rs, analysis/semantic,
analysis/local, matching/evidence, lint/report/evidence, project/report); the
providers, CLI, output, harness, and project crates consume report
`SourceRange`/byte ranges instead. `RuleMetadata` and `Severity` are genuinely
consumed by the providers and CLI rules documentation, so only `SourceLineIndex`
is exported publicly while still being engine plumbing — a direct violation of
the "Parser, scope, fact, compiler, cache, and budget internals remain private"
invariant in `glass-lint-core/ARCHITECTURE.md:97`.

**Recommendation:** Drop `SourceLineIndex` from the `lib.rs` re-export
(lib.rs:29) and keep the constructors/`try_range` crate-internal. Because the
`diagnostic` module itself is private (`lib.rs:18`), removing the re-export is
sufficient: the `pub` struct and methods become unreachable outside the crate
without further visibility edits. In the same change, convert the `try_range`
doctest (diagnostic.rs:221-230), which imports the public path, into a
crate-internal unit test. Guardrails: preserve `SourceLineIndex::from_text` as
the owned `SourceText` constructor used by the parse/analysis pipeline
(parse.rs:213, 306; semantic/mod.rs:66), and keep the `new`/`from_text`
equivalence test (diagnostic/tests.rs:60-80) until one constructor is removed.
Do not promote `SourceLineIndex` into a documented integration contract:
nothing outside the crate consumes it, and `tests/integration/public_surface.rs`
(which guards the crate's real public API) does not reference it.

**Fix Applied:** Removed `SourceLineIndex` from the core crate’s public
re-export and converted its public-path doctest into an internal diagnostic
unit test. Core-internal callers and the constructor-equivalence coverage are
unchanged, while the parser line-index implementation is no longer part of
the external API facade.

## Systemic Themes

- **Validated positive-limit family settled into two shapes.** `AnalysisLimits`
  uses a dedicated hand-rolled `PositiveLimit` newtype with a typed error
  mapping; `ProjectAdmissionLimits` uses hand-checked `usize` fields;
  `FlowLimits` (`analysis/model/flow/limits.rs:24-54`) is a distinct scaler that
  derives its counters from `AnalysisLimits::flow_operations()` and applies its
  own clamped-minimum policy. `PositiveLimit` and the `ProjectAdmissionLimits`
  hand-check enforce the same "must be positive" invariant two ways; a single
  `NonZeroUsize` carrier for both validated types would make the invariant
  uniform and standard (READ-001). `FlowLimits` is a different, legitimate
  concern and should not be folded into that carrier.
- **Parse-failure wording lives next to the kind definition yet is never used
  there.** `ParseFailureKind::diagnostic()` returns a `(DiagnosticKind,
  &'static str)` pair whose second element is consumed only by an unreachable
  arm: `AnalysisStatus::diagnostics()` skips `ParseFailure` entries
  (status.rs:167-175), and every `parse.rs` constructor supplies its own prose
  (parse.rs:205, 273-276, 286-293). The static text is stranded, not merely
  duplicated (READ-002).
- **Line-index construction is bounded to one rebuild on the parser diagnostic
  path.** Production builds one `SourceLineIndex` in `parse()` (parse.rs:213)
  and shares it via `Arc` through `SpanNormalizer` and `LocatedSourceContext`;
  `parser_range` (parse.rs:306) rebuilds a fresh index per error, the only
  production duplication. Test-only helpers (`SpanNormalizer::for_program`,
  `LocatedSourceContext::new`) construct their own. The index is cheap to build
  but is rebuilt exactly when the failure path could have shared the success
  path's machinery (READ-003).

## Open Questions — Resolved

- `analyze_ecma_version` / `analyze_ecma_version_with_limits`
  (ecma_version.rs:204-216, re-exported at lib.rs:30-33) have no
  provider/CLI/harness consumer, and none is planned. The only uses are core's
  own tests (ecma_version/tests.rs:3-5, 104) and
  `tests/integration/public_surface.rs:38-45`; a workspace-wide search finds no
  other reference. `FeatureDetector` is `pub(super)` (ecma_version/detector.rs:12)
  and `EcmaVersionReport::from_program` is `pub(crate)` (ecma_version.rs:195),
  so the walk cannot be derived from a shared parse artifact without leaking the
  SWC `Program` type across the crate boundary — which the module contract
  forbids: "SWC AST types remain private to the core crate" (ecma_version.rs:1-4)
  and core's `ARCHITECTURE.md:44`. The fresh `parse_program_only` per call
  (ecma_version.rs:213-214) is the only shape consistent with that visibility
  contract, and it deliberately skips coordinate work (parse.rs:221-227). Not a
  parse-once candidate without a concrete consumer.
- `LinterConfig::with_project_limits` (lint/linter.rs:84-88) is a real builder
  for the direct-session API, not a placeholder; it is simply not CLI-wired, by
  design. Its only caller is the test helper `test_linter_with_project_limits`
  (project/tests/support.rs:78-96), used by project/tests/input_validation.rs:64-65.
  The direct-session path (`Linter::begin_project` → `SessionState.project_limits`,
  project/session/mod.rs:43, 58) feeds `SourceTable::admit_all`
  (project/tables.rs:39-85), so `with_project_limits` is the sole way to raise
  those in-memory bounds. The CLI never uses direct sessions: it loads via
  `ProjectLoader` with `ValidatedProjectLoadOptions` built from `CliConfig.project`
  (glass-lint-cli/src/config.rs:302-310; glass-lint-cli/src/lint.rs:31, 59-60, 88),
  and filesystem admission policy belongs to `glass-lint-project` by crate
  boundary (ARCHITECTURE.md). Direct-session admission limits are not intended
  to be CLI-configurable.
- Drop the `SyntaxDepthOutcome::WithinLimit(usize)` payload in favor of a
  dedicated test accessor; cfg-gating the variant is not worth the churn.
  Production consumes only the boolean: `check_before_parse`/`check_after_parse`
  call `.is_exceeded()` (parse.rs:349-365), and the only readers of the payload
  are the `#[cfg(test)]` helpers `SyntaxDepthGuard::scan_source` (parse.rs:367-370)
  and `syntax_depth_for_test` (parse.rs:377-388), which serve the depth-counting
  assertions in parse/tests.rs:85-157. `DepthScanner` already owns the measured
  maximum (depth.rs:23-29, produced at depth.rs:80), so a `#[cfg(test)]` accessor
  there (or on `SyntaxDepthGuard`) can return it without threading it through the
  production outcome. cfg-gating the variant field would fork the enum shape
  between test and non-test builds and force `#[cfg]` on every construction and
  match site (depth.rs:80, parse.rs:318-322) — more churn than value. Since
  `SyntaxDepthOutcome` is already a private enum (parse.rs:312-316), keeping the
  payload is also behavior-safe; the change is low priority.
- The `SourceLineIndex` checkpoint scheme is justified and is not part of any
  integration contract. For non-ASCII sources with lines ≥ 256 bytes, the
  `partition_point` over per-line checkpoints (diagnostic.rs:152-155) bounds
  each column lookup to one 256-byte segment scan plus checkpoint math
  (diagnostic.rs:156) instead of an O(line-length) char count per lookup (the
  fallback at diagnostic.rs:149). Checkpoints are computed at most once per
  index via `OnceLock` (`ensure_checkpoints`, diagnostic.rs:127-132) and never
  for pure-ASCII sources, whose column is the byte delta (diagnostic.rs:143-144;
  `is_ascii` set once at diagnostic.rs:105). The scheme is a pure performance
  acceleration — with or without it the produced columns are identical — so it
  is not a behavioral contract; and per READ-004 the type should be
  crate-internal, keeping the whole scheme private. Complexity justified; keep
  as-is.
- SWC already accepts `$` and `_`; the local branches in environment.rs:117-129
  are redundant and can be removed. In swc_ecma_ast 26.0.0 (Cargo.lock:1564-1566),
  `Ident::is_valid_ascii_start`'s table explicitly covers `$` (36) and `_` (95)
  (swc_ecma_ast ident.rs:242-254), and `is_valid_ascii_continue` covers `$` (36)
  and `_` (95) as well (ident.rs:303-319); `is_valid_start`/`is_valid_continue`
  (ident.rs:268-280, 299-311) delegate to those tables for ASCII characters.
  Both `$` and `_` are ASCII, so the `if c == '$' || c == '_'` guards
  (environment.rs:118-121, 125-128) never change the result. Deleting them
  collapses the two helpers to direct `Ident::is_valid_start(c)` /
  `Ident::is_valid_continue(c)` calls with identical behavior.

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
