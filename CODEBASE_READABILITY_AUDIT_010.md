# Codebase Readability Audit

## Summary

Chunk 10 has cohesive ownership of source limits, syntax-depth protection,
source-coordinate validation, host-environment identity, and ECMAScript
feature reporting. The depth guard correctly chooses a source scan before
parsing or a token scan after parsing, and the environment keeps mutable host
configuration behind copy-on-write storage. The concrete opportunity is
unnecessary parser work: standalone syntax-version analysis retains an unused
coordinate index. Sharing the environment baseline through global lazy state
would add lifecycle complexity without evidence that construction is material.

## Findings

### [parse.rs and ecma_version.rs]

#### [x] READ-026 — Avoid building discarded source coordinates

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/parse.rs:146-154,176-195,210-216`; `glass-lint-core/src/ecma_version.rs:202-216`

`SourceParser::with_syntax_depth` always constructs a `SourceLineIndex`, and
`SourceParser::parse` always returns it in `ParsedSource` because semantic
analysis needs parser-span conversion. `analyze_ecma_version_with_limits`,
however, parses only to visit `parsed.program`; its standalone syntax-reporting
path builds and then immediately drops the source-sized line-start index (with
its lazy Unicode-checkpoint state). That adds bookkeeping to an API whose
result contains no source ranges, and it makes the parser’s coordinate
representation a hidden requirement of every parse mode.

**Recommendation:** Split parser output ownership so syntax-only consumers can
request a program plus source start without constructing `SourceLineIndex`,
while semantic analysis continues to use the coordinate-bearing parse path.
Keep one parser/depth/diagnostic implementation underneath the two output
shapes rather than duplicating parsing. Preserve source-size rejection,
pre/post depth phase selection, TypeScript lowering, parser diagnostics,
standalone `EcmaVersionReport` feature ordering, and the semantic path’s exact
span conversion behavior. Add allocation-focused coverage or a construction
counter in tests to prove syntax-only analysis does not initialize coordinates.

**Fix Applied:** The parser now builds `SourceLineIndex` only for the
coordinate-bearing semantic result, while standalone ECMAScript analysis uses
a shared parse/lower path that returns the program directly. Parser-error
locations remain available through lazy index construction. Verified with
`cargo test -p glass-lint-core parse` and
`cargo test -p glass-lint-core ecma_version`.

## Systemic Themes

- The parser’s pre-parse and post-parse depth checks are complementary safety
  phases; they should remain separate even if parser output construction is
  simplified.
- `SourceLineIndex` is a useful single owner for byte-boundary and display
  position validation. The optimization should avoid constructing it only for
  consumers that do not need positions.
- Environment identity is correctly value-based rather than pointer-based;
  introducing process-global baseline state is not justified by this audit.
- `AnalysisLimits` already centralizes positive-value validation through
  `PositiveLimit` and `with_limit`; no additional generic limit abstraction is
  recommended from this chunk.

## Review Resolutions

- Use a private parser-result split: syntax-only parsing should return the
  program and source start without a coordinate index, while semantic parsing
  retains the existing coordinate-bearing result. Keep the shared parse and
  depth-check implementation underneath both paths.
- Do not add a `OnceLock` baseline to `Environment` without a profile showing
  construction is material; the existing `Arc::make_mut` boundary is the
  simpler and sufficient ownership design.

## Coverage

Reviewed Chunk 10: core configuration; severity and rule metadata; source-line
indexing and validated byte ranges; analysis and project limits; host
environment/global-object identity; ECMAScript versions and feature detection;
source parser diagnostics; TypeScript lowering; bounded syntax-depth scanning;
and parser test seams. Read the root/core architecture,
testing/contributing guidance, the complete readability-audit skill
instructions, and existing audits 001–009. No source or test files were
changed.
