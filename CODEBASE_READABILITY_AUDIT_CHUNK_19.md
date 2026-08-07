# Codebase Readability Audit — Chunk 19

## Summary

Chunk 19 owns the provider-neutral configuration, environment and limit
values, parsing and syntax-depth admission, ECMAScript feature reporting, and
the public linter, selection, batch, catalog, and report boundaries. The
phase boundaries are mostly explicit: `Linter::new` freezes a catalog and
selection, `SourceParser` admits bounded input before AST construction, and
`SourceLineIndex` centralizes source-position conversion. The main risks are
configuration authority split across core and CLI, duplicated parser safety
state, raw source-offset invariants behind fallible APIs, and public state
representations that do not match their domain enums.

The catalog parallel-storage, batch pending-state, report-finalization, and
positional `AnalysisLimits::new` findings from Chunk 16 were checked and are
not repeated. The evidence and compiler-invariant findings from Chunk 17 and
the provider-taxonomy and query-assembly findings from Chunk 18 are likewise
kept separate.

## Findings

### Configuration and linter construction

#### [ ] READ-091 — Give rule-baseline composition one configuration owner

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/config.rs:16-32`; `glass-lint-core/src/lint/selection.rs:168-224`; composition and validation in `glass-lint-cli/src/config.rs:264-274,310-360`

`CoreConfig` documents `selection` as the baseline and ordered overrides for
the catalog, and `Config::validate` resolves that complete selection against
the catalog. The actual CLI linter construction then discards
`core.selection.baseline()` and builds a new `RuleSelection` from
`cli.profile` plus only `core.selection.overrides()`. Thus the serialized core
configuration and the selection used to run the linter are different policy
objects; validation proves one object while execution consumes another. The
test named `selected_linter_keeps_profile_baseline_before_core_overrides`
codifies this split rather than giving the boundary one owner.

The duplication makes baseline semantics difficult to explain and easy to
change inconsistently. A future profile or core-baseline change must update
`CoreConfig::validate`, `profile_selection`, and `selected_linter` together,
and a caller can reasonably believe a configured core baseline is active when
only its overrides are applied.

**Recommendation:** Define one composition operation at the CLI/core
boundary that produces the exact `RuleSelection` executed by `Linter::new`,
and validate that same value against the same assembled catalog. Either make
the profile the sole baseline owner and remove the baseline claim from
`CoreConfig`, or explicitly merge the two policies in a named, documented
operation. Delete the separate `CoreConfig::validate` path after migration.
Preserve ordered override precedence, profile defaults, unknown-rule
diagnostics, and provider-specific catalog construction.

**Fix Applied:** None so far.

### Bounded parsing and source positions

#### [ ] READ-092 — Give syntax-depth admission one scanner execution path

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/parse.rs:195-223,308-382`

`SourceParser::parse_program` selects between a source-text pre-scan and a
post-parse token scan using `requires_depth_prescan`. `DepthScanner::scan_source`
and `scan_tokens` then implement two loops over the same mutable depth state;
both stop at `Token::Error` and feed tokens into `observe`, while the source
loop additionally performs regex recovery and offset skipping. The safety
contract is therefore spread across the `raw_bound` heuristic, parser branch,
two scanner entry points, and the shared mutable fields
`previous_postfix`/`expression_can_end`.

This is more than presentation duplication: the scanner determines whether
recursive SWC parsing and later visitors may run. A new token, literal, or
regex-recovery rule can be added to one path and not the other, producing
different depth decisions for the same source. The branch also makes it hard
to see which scan has established the admission guarantee before AST
construction.

**Recommendation:** Keep the conservative `raw_bound` fast path, but give
`DepthScanner` one token-observation driver with a source-specific token
adapter for regex recovery and a parser-token adapter for the captured stream.
Make the admission phase return a named scan outcome consumed by
`parse_program`, rather than comparing two independent `Result` calls. Delete
the duplicated error-stop and observation loops after migration. Preserve
pre-AST rejection for hostile input, post-parse checking when the raw bound is
small, member-chain depth, delimiter matching, regex handling, and the
configured maximum.

**Fix Applied:** None so far.

#### [ ] READ-093 — Seal source offsets before `SourceLineIndex` position math

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/diagnostic.rs:50-60,125-196,199-204`; callers in `analysis/local.rs:90-120` and `lint/report/evidence.rs:165-185`

`SourceLineIndex::try_range` performs the required integer, bounds, and UTF-8
boundary checks, but immediately converts the result back into raw `usize`
values for private `range(start, length)` and `position(offset)`. Those
functions assume an offset is within the indexed source and that the two
positions are ordered; they use slicing and `expect` to enforce those
assumptions. The neighboring `source_slice` path independently converts and
slices a `ByteRange` without sharing the checked boundary object.

The index is the shared source-location owner for local analysis and report
evidence, so a malformed parser/evidence span must remain an ordinary
fallible outcome. At present the checked public method is safe, but internal
callers and future methods can bypass the only validation boundary and turn a
bad span into a panic or a different silent omission. The source-offset
invariant is not represented by a type after validation.

**Recommendation:** Introduce a private validated byte-range/offset handle
owned by `SourceLineIndex`, and make position, range, and source-slice
operations consume that handle. Return `Result`/`Option` only at the raw
`ByteRange` admission boundary, then remove the raw `range` helper and the
duplicate conversion in `source_slice`. Preserve Unicode and CRLF handling,
EOF ranges, lazy checkpoints, invalid-boundary rejection, and the report
layer's fail-closed behavior for spans that cannot be converted.

**Fix Applied:** None so far.

#### [ ] READ-094 — Keep parser diagnostics fallible at the SWC span boundary

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Error Handling
- **Location:** `glass-lint-core/src/parse.rs:252-288`

`SourceParser::parser_diagnostic` maps an SWC error span into display
positions with saturating integer conversions, then immediately asserts that
each `Position` is valid and that the resulting `SourceRange` is ordered.
Those `expect` calls sit on the parser error path, which is supposed to turn
unsupported or malformed input into a typed `ParseDiagnostic`. The normal
source-map invariant makes the assertions likely today, but it is owned by a
third-party parser span and is not checked by the function's return type.

This mixes diagnostic rendering with an unchecked assumption about external
span data. A source-map edge case, future parser change, or integer overflow
would panic while trying to report a syntax error, bypassing the ordinary
partial-analysis path. The same conversion policy is separate from the
validated `SourceLineIndex` path used for authored evidence.

**Recommendation:** Make span-to-range conversion a small fallible helper
that returns `None`/a typed conversion error for dummy, invalid, or unordered
spans; let `parser_diagnostic` retain the syntax code and message with
`range: None` when location conversion fails. Keep one-based positions and
the current language-specific message, while deleting the parser-path
`expect` assertions. Preserve deterministic diagnostics and fail-closed
partial reports.

**Fix Applied:** None so far.

### Rule-selection API

#### [ ] READ-095 — Store `RuleState` as the override’s domain value

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/lint/selection.rs:27-34,146-165,204-224`

`RuleOverride::new` accepts the domain enum `RuleState`, but the struct stores
an unrelated `enabled: bool`. `state()` reconstructs the enum for callers,
while `RuleSelection::resolve` reads the private boolean directly. Serde
derivation also serializes the storage spelling (`enabled`) rather than the
public state abstraction, so construction, serialization, inspection, and
execution each use a slightly different representation of one override.

The bool is not merely an implementation detail once it controls the
selection result and config schema. Adding a third state, changing the
serialized form, or introducing a validation rule would require coordinated
changes to the constructor, accessor, resolver, and derived serde behavior;
the type itself cannot express that its state is one of the supported rule
states.

**Recommendation:** Store `RuleState` directly and make serde serialize the
explicit selector/state representation intended by the configuration API.
Have `resolve` consume `override_.state()` or a private state predicate, then
delete the bool-to-enum conversion. Preserve ordered last-match-wins
semantics, wildcard validation, unknown-rule failures, and the existing
enabled/disabled configuration compatibility through an explicit migration
if that wire format is required.

**Fix Applied:** None so far.

## Systemic Themes

- Configuration and execution boundaries still duplicate policy composition;
  the value validated at one boundary should be the value consumed by the
  next phase.
- Bounded safety relies on comments and caller sequencing in addition to
  types. Validated offsets and named parser-admission outcomes would make
  fail-closed behavior local and easier to preserve.
- The remaining panic sites in this chunk are mostly invariant assertions,
  but parser and source-location diagnostics are externally observable error
  paths and should not require those assertions for ordinary malformed input.

## Open Questions

- Is `CoreConfig.selection.baseline` intentionally a legacy compatibility
  field, or should it become the source of truth alongside the CLI profile?
  The current test and `selected_linter` implementation indicate that the
  profile owns the baseline, but the core schema and validation API say
  otherwise.
- Is the `enabled` serde spelling a deliberate on-disk compatibility contract?
  If so, the enum-backed representation should retain it through an explicit
  serde adapter rather than exposing storage through derived serialization.

## Coverage

Reviewed the Chunk 19 scope in `CODEBASE_STRUCTURE_CORE.md`: `config`,
`diagnostic`, `ecma_version`, `environment`, `limits`, lint batch/catalog/
linter/report/selection, and `parse`, plus their core callers and the CLI
configuration bridge. Traced configuration validation and linter construction,
source admission and both depth-scan paths, source-position conversion,
selection resolution, batch iterator ownership, catalog access, report
assembly, ECMAScript feature detection, and environment fingerprinting.
Inspected `unwrap`/`expect`/panic sites in the scoped modules. No source or
test changes were made; this audit file is the only Chunk 19 change.
