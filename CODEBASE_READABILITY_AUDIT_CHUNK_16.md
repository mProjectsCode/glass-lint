# Codebase Readability Audit — Chunk 16

## Summary

Chunk 16 covers the public runtime configuration, lint execution and report
surface, parsing entry points, project input/session transitions, resolution
tables, and project-facing report types. The slice generally has strong
validated value types and explicit local-to-resolution phase transitions.

The concrete issues are concentrated at public and phase boundaries: report
constructors enforce non-empty invariants only in debug builds, catalog errors
are collapsed into the wrong linter configuration error, confidence selection
depends on enum declaration order, an obsolete limits error remains exposed,
and project session/artifact transitions silently accept incomplete or
unqualified state.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Report value constructors

#### [ ] READ-079 — Enforce non-empty evidence constructors in all builds

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Invariant validation / Report model
- **Location:** `glass-lint-core/src/project/types/report/evidence.rs:48-84`

`EvidenceTrace::new` and `EvidenceTraces::new` document and assert that their
vectors contain at least one item, but the checks are `debug_assert!`. Release
callers can therefore construct an empty trace or empty trace collection even
though downstream report code treats these values as evidence-bearing
objects. `with_truncation` is a separate constructor that can intentionally
represent a truncated result, so the two APIs currently do not make the
empty-versus-truncated distinction enforceable.

**Recommendation:** Make `new` fallible, or make it private and expose
validated constructors that return an explicit construction error; retain a
named empty/truncated constructor only if an empty collection is a supported
report state. Preserve evidence-step ordering, serialization, and the
intentional semantics of `with_truncation`, and remove the release-only
invariant gap rather than relying on debug assertions.

**Fix Applied:** None so far.

### Linter configuration and selection

#### [ ] READ-080 — Preserve catalog failure kinds at the linter boundary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Error mapping / Catalog ownership
- **Location:** `glass-lint-core/src/lint/linter.rs:111-115`,
  `glass-lint-core/src/lint/catalog.rs:20-24,59-83,112-120`,
  `glass-lint-core/src/lint/selection.rs:224-243`

`ProviderCatalogError::InvalidRule` represents several distinct failures:
rule validation, matcher compilation, invalid query compilation, and duplicate
rule IDs. `Linter::new` maps every one of them to `LintConfigError::DuplicateRule`,
so a malformed matcher or compiler diagnostic is reported as a duplicate
catalog entry. The catalog layer already knows the failure message and rule
identity, but the public linter boundary discards that distinction.

**Recommendation:** Give duplicate identity and rule construction/compilation
failures distinct typed catalog variants, then map them one-to-one into
`LintConfigError` (or preserve the catalog error as a source). Keep the
provider-qualified rule ID and structured query diagnostic available to the
caller, and delete the catch-all `InvalidRule`-to-`DuplicateRule` mapping once
the error owner is explicit. Do not conflate this with the separate compiler
provenance issue in Chunk 15 READ-076.

**Fix Applied:** None so far.

#### [ ] READ-081 — Remove or reconnect the unreachable invalid-limits error

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API / Obsolete path / Error ownership
- **Location:** `glass-lint-core/src/lint/selection.rs:1,234-256`,
  `glass-lint-core/src/lint/linter.rs:111-115,145-146`,
  `glass-lint-core/src/limits.rs:100-230`

`LintConfigError::InvalidLimits(AnalysisLimitError)` is publicly modeled and
formatted, but no construction site exists in the workspace. `AnalysisLimits`
validates through `new`, its `with_*` methods, `Default`, and deserialization,
while `Linter::new` explicitly relies on that construction invariant and never
revalidates `config.limits`. The error variant therefore advertises a failure
path that the owning configuration API cannot produce, while its import adds
another cross-module dependency to selection.

**Recommendation:** Either move limits validation into one explicit
`LinterConfig::validate` transition and construct this variant there, or remove
`InvalidLimits` and its `AnalysisLimitError` dependency from lint selection.
Preserve fallible limit builders and serde validation; the chosen path should
leave one authoritative owner for rejecting zero/invalid limits rather than an
unreachable duplicate error surface.

**Fix Applied:** None so far.

#### [ ] READ-082 — Encapsulate confidence ranking instead of casting enum variants

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Semantic ordering / Selection policy
- **Location:** `glass-lint-core/src/lint/linter.rs:119-126`,
  `glass-lint-core/src/api/rule/taxonomy.rs:48-67`

`Linter::new` implements minimum-confidence selection by comparing
`Confidence` discriminants through `as u8`. The current declaration order
(`High`, `Medium`, `Low`) happens to encode the desired ranking, but the enum
does not expose that as a contract; adding or reordering a variant silently
changes which rules are enabled. The selection policy consequently depends on
representation owned by the taxonomy enum rather than on a named semantic
operation.

**Recommendation:** Put a `rank`, `meets`, or equivalent ordered comparison
method on `Confidence`, and have `Linter::new` call it. Preserve the existing
High/Medium/Low threshold semantics and serialized spellings, and delete the
raw discriminant cast so future taxonomy changes cannot alter selection by
accident.

**Fix Applied:** None so far.

### Project session and artifact transitions

#### [x] READ-083 — Make `finish_local` validate complete local analysis

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Lifecycle state / Silent omission
- **Location:** `glass-lint-core/src/project/session/mod.rs:168-175,270-278,391-399`,
  `glass-lint-core/src/project/session/artifacts.rs:105-109,121-150`

`ProjectCollection::finish_local` consumes its session and returns
`LocallyAnalyzedProject` without checking that every admitted source is no
longer reported by `AnalysisArtifacts::needs_analysis`. `analyze_sources`
admits inputs before analysis and can return an error after partially mutating
the collection; the caller still owns that collection and can finish it.
The resulting link input can contain source-table entries with neither a
lowered artifact nor a parse diagnostic, silently omitting that source from
the resolution/report phase despite the phase-transition documentation.

**Recommendation:** Make `finish_local` return a typed incomplete-project
error, or make the collection record a terminal failed state that prevents the
transition after partial admission/analysis failure. Keep parse failures as
completed local outcomes, preserve consuming phase transitions, and add the
completion check at the collection/artifact owner rather than requiring every
caller to scan `SourceTable` and `AnalysisArtifacts` independently.

**Fix Applied:** Added owner-level completion validation to
`AnalysisArtifacts` and made `ProjectCollection::finish_local` return a typed
`IncompleteLocalAnalysis` error listing admitted paths without an artifact or
parse diagnostic. All phase callers now propagate the fallible transition;
parse failures remain valid completed local outcomes.

**Verification:** `cargo test -p glass-lint-core project::tests --lib`
(48 passed); `make fmt && make ci` (passed).

#### [ ] READ-084 — Do not silently drop authored requests without importer IDs

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Identity ownership / Silent omission
- **Location:** `glass-lint-core/src/project/session/artifacts.rs:26-50,121-150`

`AuthoredRequestTable::qualified_ids` converts each authored request into a
project-qualified request ID by looking up `key.importer()` in a separate
`BTreeMap`, but uses `filter_map` when the lookup fails. Authored requests are
created from lowered source artifacts and their importers should therefore be
the same project sources used to build `module_ids`; a missing importer is an
invariant failure, not an absent optional request. Dropping it leaves the
linker input with incomplete request identity and makes later resolution
behavior depend on whether the inconsistency was noticed elsewhere.

**Recommendation:** Let `qualified_ids` return a typed error naming the
missing importer, or store the module identity alongside the authored request
when it is inserted so qualification cannot lose entries. Preserve
deterministic `BTreeMap` ordering and the distinction between an authored
request with a resolver outcome and an unknown request; delete the silent
`filter_map` omission after the owner establishes that relationship.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 16’s strongest boundary is its use of validated semantic input types and
consuming project-session phases. The remaining readability risk is that some
of those contracts are only comments or debug assertions: public report
constructors can violate their own shape, linter errors lose catalog meaning,
and session/artifact indexes are reconciled with lossy lookups. Moving these
invariants into constructors and phase owners would simplify callers while
preserving the current deterministic ordering and fail-closed behavior.

No findings are marked applied.

## Open Questions

- Empty `EvidenceTraces` may be intended as a serialized representation of a
  truncated or unavailable explanation; if so, that state should have one
  explicit constructor rather than weakening `new` in release builds.
- The catalog error API is shared with provider-facing catalog construction;
  the eventual split should preserve provider identity and avoid re-reporting
  the compiler rule-provenance issue already recorded as READ-076.
- `finish_local` may be intended to support recovery after an error, but that
  requires an explicit failed/partial phase rather than an apparently complete
  `LocallyAnalyzedProject`.
- The next unreviewed handoff is Chunk 17: classification, catalog/compiler
  support, and remaining core API modules listed in `CODEBASE_STRUCTURE_CORE.md`.

## Coverage

Reviewed the Chunk 16 modules listed in `CODEBASE_STRUCTURE_CORE.md` across
configuration, diagnostics, ECMAScript/environment limits, lint construction
and execution, batch/report/selection APIs, parsing, project input/report
types, session artifacts and execution, resolution tables, and top-level rule
ID/rule re-exports. Representative callers were traced from `Linter::new`
through project admission, local artifact lowering, request qualification, and
report assembly. Catalog and compiler findings were checked against Chunk 15
to avoid repeating READ-076 and READ-077. No source, test, configuration,
dependency, or documentation changes were made.
