# Codebase Readability Audit

## Summary

Chunk 10 has cohesive ownership of source limits, syntax-depth protection,
source-coordinate validation, host-environment identity, and ECMAScript
feature reporting. The depth guard correctly chooses a source scan before
parsing or a token scan after parsing, and the environment keeps mutable host
configuration behind copy-on-write storage. The concrete opportunities are
unnecessary work at those boundaries: standalone syntax-version analysis
retains an unused coordinate index, and default environment construction
rebuilds immutable baseline data on every call.

## Findings

### [parse.rs and ecma_version.rs]

#### [ ] READ-026 — Avoid building discarded source coordinates

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/parse.rs:146-154,176-195,210-216`; `glass-lint-core/src/ecma_version.rs:202-216`

`SourceParser::with_syntax_depth` always constructs a `SourceLineIndex`, and
`SourceParser::parse` always returns it in `ParsedSource` because semantic
analysis needs parser-span conversion. `analyze_ecma_version_with_limits`,
however, parses only to visit `parsed.program`; its standalone syntax-reporting
path builds and then immediately drops the complete line-start/checkpoint
index. That adds source-sized bookkeeping to an API whose result contains no
source ranges, and it makes the parser’s coordinate representation a hidden
requirement of every parse mode.

**Recommendation:** Split parser output ownership so syntax-only consumers can
request a program plus source start without constructing `SourceLineIndex`,
while semantic analysis continues to use the coordinate-bearing parse path.
Keep one parser/depth/diagnostic implementation underneath the two output
shapes rather than duplicating parsing. Preserve source-size rejection,
pre/post depth phase selection, TypeScript lowering, parser diagnostics,
standalone `EcmaVersionReport` feature ordering, and the semantic path’s exact
span conversion behavior. Add allocation-focused coverage or a construction
counter in tests to prove syntax-only analysis does not initialize coordinates.

**Fix Applied:** None so far.

### [environment.rs]

#### [ ] READ-027 — Share the immutable ECMAScript environment baseline

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Performance
- **Location:** `glass-lint-core/src/environment.rs:12-29,77-89,169-186,384-443`; representative consumers `glass-lint-core/src/analysis/semantic/mod.rs:194-203` and `glass-lint-core/src/lint/linter.rs:41-51`

`Environment` already uses `Arc<EnvironmentInner>` and `Arc::make_mut`, so
clones share immutable state until a caller adds or extends globals. The
`ecmascript()` constructor nevertheless allocates and fills a fresh
`BTreeSet<SmolStr>` from the fixed `ECMASCRIPT_GLOBALS` table and a fresh
global-object map on every construction. Default environments are used as
baseline inputs to analyzers and linter configurations, so repeated setup
does work that the copy-on-write design is otherwise structured to avoid.

**Recommendation:** Store one lazily initialized canonical
`Arc<EnvironmentInner>` for the ECMAScript baseline and clone that handle from
`ecmascript()`. Retain `Arc::make_mut` for all user additions, and ensure the
baseline is never exposed for mutation through an alias. Preserve deep equality
and fingerprint bytes, deterministic BTree iteration, the `globalThis`
promotion policy, and the semantics of restricted foreign-realm objects. Add
tests that mutate one baseline environment and verify another remains
unchanged, while equality and fingerprints remain identical for independently
constructed defaults.

**Fix Applied:** None so far.

## Systemic Themes

- The parser’s pre-parse and post-parse depth checks are complementary safety
  phases; they should remain separate even if parser output construction is
  simplified.
- `SourceLineIndex` is a useful single owner for byte-boundary and display
  position validation. The optimization should avoid constructing it only for
  consumers that do not need positions.
- Environment identity is correctly value-based rather than pointer-based.
  Sharing the immutable baseline must not change `Eq`, cache fingerprints, or
  copy-on-write mutation semantics.
- `AnalysisLimits` already centralizes positive-value validation through
  `PositiveLimit` and `with_limit`; no additional generic limit abstraction is
  recommended from this chunk.

## Open Questions

- Should syntax-only parsing return the same `ParsedSource` shape with an
  optional coordinate index, or should a private parser result enum make the
  semantic and syntax-only ownership explicit?
- Is environment construction frequent enough in production to justify the
  `OnceLock<Arc<EnvironmentInner>>` baseline, or should the optimization wait
  until profiling confirms default setup is material?

## Coverage

Reviewed Chunk 10: core configuration; severity and rule metadata; source-line
indexing and validated byte ranges; analysis and project limits; host
environment/global-object identity; ECMAScript versions and feature detection;
source parser diagnostics; TypeScript lowering; bounded syntax-depth scanning;
and parser test seams. Read the root/core architecture,
testing/contributing guidance, the complete readability-audit skill
instructions, and existing audits 001–009. No source or test files were
changed.
