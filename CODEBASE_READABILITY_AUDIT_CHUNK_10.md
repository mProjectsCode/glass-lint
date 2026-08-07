# Codebase Readability Audit — Chunk 10

This audit covers Chunk 10 of `CODEBASE_STRUCTURE_CORE.md`: configuration,
parsing, and runtime environment. It is an architectural review only; no
source changes were made.

## Summary

The configuration and parser boundary has strong defensive behavior: host
globals are validated and deterministic, source ranges reject invalid UTF-8
boundaries, syntax depth is bounded before recursive parser work when needed,
and ECMAScript feature output is stable. The main readability risks are
ownership splits around identity and source coordinates. Environment equality
and cache fingerprinting are maintained separately, global-path matching is
implemented twice, parse diagnostics carry two path identities and two range
conversion paths, project discovery duplicates language admission policy,
depth protection is a hidden two-phase protocol, and the standalone syntax
API silently chooses default limits.

## Findings

### Configuration and environment identity

#### [x] READ-001 — Environment equality and cache fingerprinting encode the same state independently

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/environment.rs:21-58,423-454`; cache consumer at `glass-lint-core/src/analysis/local.rs:43-79`
- **Representative callers:** `ArtifactFingerprint::compute` calls `Environment::write_fingerprint_bytes`, while `Environment::PartialEq` compares `EnvironmentInner` directly

`Environment` uses structural `EnvironmentInner` equality for cache-key
semantics, but its cache fingerprint is a second hand-written traversal of
the same global-binding and global-object representation. Adding a field,
changing the `GlobalObjectMembers` encoding, or changing merge semantics now
requires keeping equality and fingerprint serialization synchronized; a
missed fingerprint update can make distinct environments share a cache hash
and force the cache key to rely on a later equality check to recover.

**Recommendation:** Give `Environment` one private canonical identity or
encoding operation and derive both equality support and fingerprint input from
that owner, retaining the `Arc::ptr_eq` fast path if useful. Keep deterministic
ordering, the distinction between configured and restricted global objects,
and all fields that affect semantic resolution in the identity. Preserve the
existing collision-safe `ArtifactCacheKey` equality behavior and cache
versioning.

**Fix Applied:** The canonical fingerprint encoding now belongs to private
`EnvironmentInner` and its `GlobalObjectMembers` policy, alongside derived
structural equality; `Environment` delegates cache-key encoding to that owner.
Deterministic ordering, configured versus restricted object identity, and
collision-safe cache-key equality are unchanged. Verified with `make fmt &&
make ci`.

#### [x] READ-002 — Global-object path matching duplicates one semantic relation across two path types

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Encapsulation
- **Location:** `glass-lint-core/src/environment.rs:285-421`
- **Representative callers:** resolver paths use `global_object_paths_match` with `SymbolPath`; name-table-backed matching uses `global_object_name_paths_match` with `NamePath`

The two matchers implement the same three-way relation—identical paths,
configured global-object aliases with equal tails, and one promoted global
member with the remaining path—but each repeats the left/right symmetric
checks. The `NamePath` version additionally resolves IDs through `NameTable`,
so a future change to alias or promotion semantics must be applied in both
representations without a shared semantic operation.

**Recommendation:** Centralize the global-object relation in one private
path-comparison owner, with a narrow adapter for resolving a `NamePath` root
or member to text. The owner should expose named operations for alias-tail
matching and promoted-member matching rather than making callers reproduce
the symmetry. Preserve exact path identity, restricted foreign-realm
behavior, unresolved `NameId` fail-closed behavior, and tail ordering.

**Fix Applied:** A private `GlobalObjectPath` now owns exact, configured-alias,
and promoted-member path relations. `SymbolPath` uses a direct string adapter;
`NamePath` resolves all `NameId` segments through a fail-closed adapter before
using the same relation owner. Restricted foreign-realm behavior, tail
ordering, and exact-path identity are preserved, with a name-path contract
test added. Verified with `make fmt && make ci`.

### Diagnostic and source-coordinate ownership

#### [x] READ-003 — Parse diagnostics carry both a filename string and an outer validated path

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/parse.rs:28-38,174-184,240-268`; wrapper at `glass-lint-core/src/project/types/report/diagnostic.rs:48-66,77-93`
- **Representative callers:** `SourceParser` writes `ParseDiagnostic::filename`; `Diagnostic::parse` later overwrites it from its `ProjectRelativePath` while also retaining that path separately

`ParseDiagnostic` stores an authored filename as an unconstrained `String`,
while the public project diagnostic wrapper stores the same identity as a
validated `ProjectRelativePath`. The wrapper’s `parse` constructor mutates
the inner diagnostic to reconcile the two values. This leaves callers with
two sources of truth and makes it possible for a directly constructed or
serialized parse diagnostic to disagree with the path used for report
ordering and locations.

**Recommendation:** Keep `ParseDiagnostic` independently constructible for the
standalone parser and ECMAScript-version APIs, where its authored `filename`
is meaningful. Make the outer `Diagnostic::Parse` the sole owner of validated
project identity: stop mutating the inner diagnostic in `Diagnostic::parse`
and use the outer `ProjectRelativePath` for ordering and locations. Preserve
standalone parser diagnostics, serialized field compatibility if required by
the report contract, deterministic path ordering, and the distinction between
parser failure metadata and project context.

**Fix Applied:** `Diagnostic::parse` now preserves standalone parser metadata and
leaves validated project identity solely in the outer `ProjectRelativePath`.
The report combination test asserts both identities independently. Verified with
`make fmt && make ci`.

#### [x] READ-004 — Parser ranges and semantic spans maintain competing byte-to-position validators

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Conversion / API
- **Location:** `glass-lint-core/src/diagnostic.rs:108-215`; parser conversion at `glass-lint-core/src/parse.rs:271-306`; semantic conversion at `glass-lint-core/src/analysis/lowering/mod.rs:45-88`
- **Representative callers:** parser errors call `SourceParser::parser_range`; lowering normalizes SWC spans to `ByteRange`, and `LocatedSourceContext` then delegates to `SourceLineIndex::try_range`

The diagnostic index is the shared source-coordinate owner for report and
evidence ranges, including byte bounds, UTF-8 boundaries, line starts, and
Unicode columns. Parser diagnostics instead validate the same byte conditions
locally and ask `SourceMap::lookup_char_pos` for display positions, while
semantic spans use a separate `SpanNormalizer` before the index converts
them. The two parser-facing paths can diverge on malformed spans, CRLF or
Unicode columns, and future position-policy changes.

**Recommendation:** Introduce one private source-coordinate transition that
normalizes a SWC span to a checked `ByteRange` and delegates display
conversion to the existing `SourceLineIndex`, or make an equivalent shared
adapter the sole owner of both operations. Keep parser-specific dummy-span
handling and SWC offset subtraction at the adapter boundary. Preserve
fail-closed invalid-span behavior, UTF-8 boundary checks, one-based display
positions, CRLF/EOF handling, and the zero-copy source-slice path.

**Fix Applied:** `SourceLineIndex` now owns offset-to-`ByteRange` validation and
display-range conversion. Parser diagnostics retain dummy-span handling and
SWC base-offset subtraction, then use the shared line index; semantic
`SpanNormalizer` uses the same checked-byte-range adapter. Invalid spans,
UTF-8 boundaries, Unicode/CRLF/EOF positions, and zero-copy source slicing
remain fail-closed and unchanged. Verified with `make fmt && make ci`.

### Parser and runtime policy boundaries

#### [x] READ-005 — Source-language admission is split between core and project discovery

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture / API
- **Location:** `glass-lint-core/src/parse.rs:49-88`; source construction at `glass-lint-core/src/project/types/input.rs:241-292`; project admission at `glass-lint-project/src/options.rs:8-18,95-119`
- **Representative callers:** `SourceFile::new` and `SourceFile::from_relative` infer a language from the filename; `SourceExtensionSet::supports` independently checks configured suffixes and declaration-file exclusions

Core’s `SourceLanguage` owns filename extension and declaration-file
classification, while the filesystem project crate owns a second extension
set with the same default suffixes and `.d.ts`/`.d.cts`/`.d.mts` exclusions.
The project loader can therefore admit a path according to one policy and
construct a `SourceFile` whose inferred language comes from another. The
duplication is especially easy to miss when project extensions become
configurable, because `SourceLanguage::from_filename` still silently falls
back to JavaScript for unknown names.

**Recommendation:** Keep filesystem discovery and configurable suffix policy
in `glass-lint-project`, and pass an explicit `SourceLanguage` when creating
core `SourceFile` values. Reduce core `SourceLanguage` to parser-mode
semantics, retaining an explicit helper for virtual/extensionless inputs if
needed rather than a second discovery predicate. Preserve case-insensitive
configured extensions, declaration-file exclusion, direct virtual-source
construction, and the project/core boundary that keeps filesystem policy out
of core.

**Fix Applied:** Core `SourceLanguage` and `SourceFile` no longer infer or
admit filenames. Validated project options now own suffix admission and map
admitted paths to an explicit parser language; CLI, harness, profile, and test
callers pass that language at construction. Verified with `make fmt && make ci`.

#### [x] READ-006 — Syntax-depth protection is a hidden two-phase protocol inside `SourceParser`

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/parse.rs:135-226,362-393`
- **Representative callers:** `SourceParser::with_syntax_depth` computes `requires_depth_prescan`; `parse_program` chooses raw-source scanning before SWC parsing or token scanning after it; `DepthScanner` supplies both scanner modes through a generic callback

The parser’s safety boundary is distributed across a stored boolean, a raw
punctuation bound, a source lexer with regex recovery, the SWC parser, and a
post-parse token scan. `parse_program` is consequently responsible for both
the recursive-parser admission decision and the parser’s syntax result, while
the generic `DepthScanner::scan` callback hides whether tokens are source
recovered or already parser-produced. The behavior is defensively tested,
but the lifecycle is hard to reconstruct and a depth-policy change can alter
which phase is safe without a single named owner.

**Recommendation:** Encapsulate the safety decision in a private
`SyntaxDepthGuard`/parser-policy transition that states when a conservative
pre-scan is required and when post-parse token scanning is permitted. Keep
source-specific regex recovery separate from the common delimiter/member
state, and let the parser orchestrator consume a named bounded outcome rather
than a boolean mode. Preserve pre-parse rejection for hostile bounds,
post-parse validation for safe inputs, deferred SWC syntax diagnostics, and
all delimiter, template, optional-chain, regex, and division semantics.

**Fix Applied:** `SyntaxDepthGuard` now owns the conservative raw-bound phase
decision and exposes named pre-parse and post-parse checks. `SourceParser`
consumes those transitions while `DepthScanner` retains the shared delimiter
state and source-specific regex recovery. Pre-parse hostile rejection,
post-parse validation, deferred syntax diagnostics, and existing delimiter,
template, optional-chain, regex, and division behavior are preserved. Verified
with `make fmt && make ci`.

#### [x] READ-007 — The standalone ECMAScript-version API bypasses caller-configured analysis limits

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/ecma_version.rs:186-195`; configurable parser boundary at `glass-lint-core/src/parse.rs:151-171`; canonical lowering caller at `glass-lint-core/src/analysis/lowering/mod.rs:173-176`
- **Representative callers:** `analyze_ecma_version` always passes `AnalysisLimits::default().syntax_depth()`, while `Lowerer::lower_source` passes the linter’s configured syntax-depth limit to the same parser

The public syntax-report helper has no limits parameter and unconditionally
creates default analysis limits. A caller can configure a linter with a
different syntax-depth policy, yet an independent version analysis of the
same `SourceFile` follows a different resource contract. The parser already
accepts an explicit bound, so the hidden default is an API boundary choice
rather than a parser requirement.

**Recommendation:** Keep `analyze_ecma_version` as the fixed-cost,
zero-configuration convenience endpoint, and add a clearly named
`analyze_ecma_version_with_limits` sibling only for callers that need parity
with a configured `Lowerer`. Do not change the existing signature or make
catalog/environment state part of the standalone API. Preserve deterministic
feature ordering, fail-closed syntax-depth errors, and the existing default
behavior for callers that do not opt into custom limits.

**Fix Applied:** The existing `analyze_ecma_version` remains the fixed-cost
default convenience endpoint, while new
`analyze_ecma_version_with_limits` accepts caller-configured syntax limits and
shares the parser boundary. Deterministic feature ordering and default
behavior are unchanged; an explicit-limit test verifies fail-closed depth
errors. Verified with `make fmt && make ci`.

## Systemic Themes

- **ENCAPSULATE:** Environment identity, parse paths, source-language
  admission, and source-coordinate conversion should each have one owner.
- **SIMPLIFY:** Parser safety and standalone syntax analysis expose hidden
  policy transitions through booleans and implicit defaults.
- **DEDUPLICATE:** Global-path comparison and byte-range validation repeat the
  same semantic operations across representations and phases.

## Decisions

- `ParseDiagnostic` remains independently constructible and keeps its authored
  filename; `Diagnostic::Parse` owns validated project path identity without
  mutating the inner value.
- `analyze_ecma_version` is deliberately fixed-cost and independent of a
  configured linter. A limits-taking sibling may be added for explicit parity,
  but the ergonomic default API remains unchanged.

## Open Questions

None recorded.

## Coverage

Reviewed `CoreConfig`, `AnalysisLimits` and its validated serialization path,
`Environment` construction/merge/equality/fingerprinting/global-object
matching, `SourceLineIndex`, parse diagnostics and range conversion,
`SourceLanguage` and `SourceFile` construction, project extension admission,
`SourceParser` source admission and TypeScript lowering, syntax-depth
scanning, and ECMAScript feature detection. Existing tests were not changed
or run because this is a read-only audit.

## Handoff

Chunk 10 is complete. The next unreviewed chunk is **Chunk 11 — Lint
execution and reporting** (`CODEBASE_STRUCTURE_CORE.md` lines 723-761),
covering linter configuration, batch execution, selection, report assembly,
evidence, summaries, and diagnostics.
