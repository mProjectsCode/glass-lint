# Codebase Readability Audit — Chunk 10

## Summary

Chunk 10 owns core configuration, source-coordinate diagnostics, ECMAScript
feature detection, host-environment modeling, global analysis limits, and the
bounded parser. The main type boundaries are purposeful: parser diagnostics
need stable serialized codes while retaining an internal failure kind, the
environment uses copy-on-write state for cacheable configuration, and the
depth guard separates a conservative pre-parse check from a post-parse check.
The findings below target duplicated per-file coordinate state, duplicated
source-size policy defaults, and a repeated reduction over the detected
feature set.

## Findings

### Parser and semantic coordinate ownership

#### [ ] READ-044 — Successful parsing builds and then discards a duplicate source-line index

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/parse.rs:146-193,208-214`; `glass-lint-core/src/analysis/semantic/mod.rs:51-91,176-185`

`SourceParser` constructs a `SourceLineIndex` as part of its parser state so
that SWC parser errors can be converted to authored source ranges. A
successful `SourceParser::parse` returns only the lowered program and SWC
source start, so that index is dropped. `SemanticAnalyzer::analyze_source`
then constructs another `SourceLineIndex` from the same `SourceText` for
`SpanNormalizer`, and moves that second index into `LocatedSourceContext`.
`SourceText` cloning is cheap because it retains the `Arc<str>`, but each
`SourceLineIndex::from_text` still allocates its own line-start vector and
lazy checkpoint state. Every successfully analyzed file therefore pays for
two coordinate indexes even though only the semantic one survives.

**Recommendation:** Carry the parser-owned index through `ParsedSource`, or
transfer it directly into the semantic coordinate/context boundary after a
successful parse. Keep the parser index available while mapping parser
diagnostics, preserve the source start, path, UTF-8 validation, and context
ownership, and remove only the second index construction. Standalone parser
errors and the public source-coordinate behavior must remain unchanged.

**Fix Applied:** None so far.

### Source-size policy ownership

#### [x] READ-045 — The project loader copies the core source-byte default while treating the core constant as authority

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lib.rs:73-76`; `glass-lint-project/src/options.rs:10-13,229-236,292-299`; `glass-lint-cli/src/config.rs:186-188`

Core declares `MAX_SOURCE_BYTES` as `8 * 1024 * 1024`. The project loader
declares the same expression again as its private
`DEFAULT_MAX_SOURCE_BYTES`, uses that copy for `ProjectLoadOptions::default`,
and validates the option against `glass_lint_core::MAX_SOURCE_BYTES`. The CLI
already derives its default from the core constant. This leaves the project
default with a second value owner even though validation establishes the core
constant as the upper bound; a future change can make the default and the
accepted maximum diverge or force a second edit without a compiler error.

**Recommendation:** Either derive the project loader’s default from the core
constant, or make a deliberately different project policy explicit with a
distinct name and documentation. Keep the project-level per-file and
aggregate-source budgets separate, retain validation before I/O, and keep the
core parser’s own direct-use limit as defense in depth. The simplification is
only to remove the unintentional copied default, not to collapse the
project-loading and parser ownership boundaries.

**Fix Applied:** The project loader's default per-file source budget now derives
from `glass_lint_core::MAX_SOURCE_BYTES`, while project aggregate budgets and
validation remain independently owned. Verified with `make fmt && make ci`.

### ECMAScript feature reduction

#### [ ] READ-046 — Feature-version reduction traverses the detected feature set twice

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/ecma_version.rs:134-169,219-245`

`FeatureDetector::finish` first walks the complete `BTreeSet<EcmaFeature>` to
check whether any feature has no standard ECMAScript version, then walks the
same set again to compute the maximum version when all features are
versioned. The second traversal is conditional, but every ordinary report
still performs two ordered-set iterations and calls the same
`EcmaFeature::minimum_version` mapping in separate expressions. The feature
set is bounded and the cost is small, but the two-pass shape duplicates the
reduction logic at the report boundary.

**Recommendation:** Fold the set once while tracking both whether an
unversioned feature was seen and the maximum version of versioned features,
then derive `None` or the maximum from that accumulator. Preserve the empty
set default of `Es5`, the fail-closed `None` result for JSX/decorators and
other unversioned syntax, deterministic feature ordering, and the public
`EcmaVersionReport` shape.

**Fix Applied:** None so far.

## Systemic Themes

- Parser and semantic phases have distinct responsibilities, but source
  coordinate ownership should cross that boundary once rather than rebuilding
  equivalent indexes after parsing.
- Hard safety limits should have one value owner. Project-specific budgets can
  remain distinct, while defaults that intentionally equal a core hard cap
  should derive from it or clearly declare independent policy.
- Small bounded reductions still benefit from one owner for the state they
  compute. A single fold can preserve the current fail-closed semantics while
  removing repeated iteration and feature-version mapping.
- Most configuration, environment, and diagnostic wrappers in this chunk are
  justified by validation, deterministic ordering, copy-on-write cache
  identity, or serialization roles; they were not reported merely for being
  layered.

## Open Questions

- The project loader currently validates against the core hard maximum and
  exposes the same value as its default, so the default should derive from
  `glass_lint_core::MAX_SOURCE_BYTES`; project aggregate and discovery budgets
  remain separate policies.
- Carry the parser-owned line index through `ParsedSource` unless profiling
  shows the phase boundary requires a dedicated wrapper; either form must
  transfer one index, not rebuild it.
- Keep `ParseDiagnostic.code` as the serialized stable identity and
  `ParseFailureKind` as the internal typed classification; they serve
  different consumers and are not duplicate fields.

## Coverage

Reviewed the chunk-10 structure entries and their implementation/test support:

- `config.rs`, `diagnostic.rs`, `ecma_version.rs`, `environment.rs`,
  `limits.rs`, and `parse.rs`
- semantic parser callers in `analysis/semantic/mod.rs`
- core source-size boundary in `lib.rs`
- project loading defaults and validation in `glass-lint-project/src/options.rs`
- CLI source-size default in `glass-lint-cli/src/config.rs`
- public-surface integration coverage for environment, parser diagnostics,
  and ECMAScript-version reporting
- Existing numbered audit reports 001–009 were checked to avoid duplicating
  their historical findings.

No source, test, configuration, dependency, or other documentation files
were changed by this audit.
