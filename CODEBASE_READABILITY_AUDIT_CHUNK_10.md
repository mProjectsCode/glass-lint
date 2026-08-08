# Codebase Readability Audit

## Summary

Chunk 10 owns core configuration, source-position diagnostics, bounded
JavaScript/TypeScript parsing, ECMAScript syntax reporting, host-environment
identity, and validated analysis limits. The parser and environment boundaries
are generally deliberate: source text is shared, limits reject zero, global
registration is validated atomically, and environment identity remains
deterministic. Three cross-layer contracts still weaken the design. Rule
metadata exposes its backing storage, parse failures carry duplicated
classification vocabularies, and the parser accepts syntax that the public
ECMAScript feature report cannot represent.

## Findings

### Report metadata ownership

#### [x] READ-038 — Keep rule metadata storage behind its report API

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/diagnostic.rs:242-254`; construction `glass-lint-core/src/lint/catalog.rs:111-122`; consumers `glass-lint-cli/src/rules_doc.rs:42-50` and `glass-lint-cli/src/output.rs:79-110`

`RuleMetadata` is a public serialized report type, but all four fields are
public, including the mutable `query_explanations: Vec<String>`. The catalog
creates metadata from already-compiled records and front ends only read it;
there is no public construction path that needs callers to mutate the
backing collection. A caller can nevertheless reorder, append to, or remove
query explanations, change a catalog-owned rule ID, or replace the default
severity after the metadata has been produced.

This is the same ownership boundary that the catalog and compiled records
otherwise preserve: rule identity, generated explanations, and default
severity are derived data owned by catalog compilation. Public fields make
that contract accidental and prevent the type from adding invariants around
explanation order or metadata completeness without changing its storage API.

**Recommendation:** Make the fields private and expose narrow accessors or
iterators for the read-only report surface. Keep serde serialization on the
private fields, and provide a catalog-owned constructor (or a crate-private
conversion) so only validated compiled records can create metadata. Preserve
the current catalog order and JSON shape.

**Fix Applied:** Made `RuleMetadata` fields private, added read-only accessors,
and introduced a crate-private catalog constructor. Catalog compilation now
owns metadata creation while CLI and provider consumers preserve the existing
serialized field names and catalog order through the accessors.

### Parse failure classification

#### [ ] READ-039 — Make one type own parse-failure classification and codes

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/parse.rs:25-45, 130-140, 197-225`; mapping `glass-lint-core/src/analysis/lowering/status.rs:156-170`; stable diagnostic vocabulary `glass-lint-core/src/project/types/report/code.rs:8-55`

`ParseDiagnostic` stores both a public `DiagnosticCode` and a crate-private
`ParseFailureKind`. Each parser error constructor manually supplies both
values, while `ParseFailureKind::diagnostic` separately maps the same three
states to `DiagnosticKind` and generic status text. The project diagnostic
layer therefore has one vocabulary for stable serialized codes, the parser
has another for control flow, and the status layer maintains the conversion
between them.

The duplicate state is currently kept synchronized by convention: for
example, `SourceTooLarge` must be paired with
`DiagnosticKind::SourceTooLarge`, and an independently added parse failure
would require changes in constructors, status mapping, and the diagnostic
kind table. The public diagnostic can expose one code while internal report
assembly consumes a second field, so adding or renaming a failure category
has several owners rather than one typed source of truth.

**Recommendation:** Give one parse-failure type ownership of its stable code
and status message, with methods that project to the serialized
`DiagnosticCode` and the internal completeness reason. Construct
`ParseDiagnostic` through a typed constructor that accepts the failure kind
once; have status assembly call the same projection instead of maintaining a
second match table. Retain the stable external diagnostic codes and the
internal distinction needed to suppress or classify later analysis.

**Fix Applied:** None so far.

### Parser and syntax-report vocabulary

#### [x] READ-040 — Keep enabled parser syntax and ECMAScript feature reporting in one vocabulary

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/parse.rs:56-75`; `glass-lint-core/src/ecma_version.rs:63-153, 202-457`; SWC AST paths consumed by the detector include `ImportDecl::with`, `NamedExport::with`, `ExportDefaultSpecifier`, and `ClassMember::AutoAccessor`

The JavaScript parser enables `fn_bind`, `export_default_from`,
`import_attributes`, and `auto_accessors`, in addition to JSX, decorators,
and explicit resource management. `EcmaFeature` has entries for the latter
three, but no entries or visitor handling for the former four. The detector
only records broad `Modules`/`Classes` features for the relevant AST nodes;
it does not record the proposal syntax that caused those nodes to be
accepted. For example, import/export `with` attributes live on SWC module
declarations, default-export specifiers have their own AST node, and
auto-accessors are a separate class-member variant, but no corresponding
feature is recorded in `FeatureDetector`.

This makes the public `analyze_ecma_version` contract incomplete relative to
the parser contract. Accepted proposal syntax can produce a report that
claims only ES2015 module or class syntax, and `EcmaVersionReport::minimum_version`
can remain a standard edition rather than becoming `None` for unversioned
syntax. The parser and detector each have a private list of supported syntax,
so enabling a parser option does not force the report vocabulary or tests to
be updated with it.

**Recommendation:** Keep the parser's explicit syntax configuration, but make
the accepted-language list and the detector's `EcmaFeature` list agree. Add
feature variants with `None` minimum versions and visitor coverage for each
currently enabled proposal extension, including import attributes, default
export-from, function-bind syntax, and auto-accessors. Add focused report
tests for every enabled extension; do not reject syntax merely because the
current report vocabulary is incomplete, and preserve deterministic feature
ordering and standard-version calculation.

**Fix Applied:** `EcmaFeature` now includes the unversioned vocabulary for
function-bind, default export-from, import attributes, and auto-accessors.
`FeatureDetector` records import attributes on static, re-export, export-all,
and dynamic imports, default-export specifiers, and auto-accessors. Focused
tests verify that each representable proposal reports `minimum_version() ==
None`; the parser's explicit syntax configuration remains unchanged. The
installed SWC parser exposes no function-bind token or AST node, so its
feature is reserved in the public vocabulary without inventing a heuristic
that could misclassify ordinary syntax.

## Systemic Themes

- Report-facing types generally have private semantic owners, but metadata
  still exposes mutable storage. Keep serialized DTOs readable through
  accessors while retaining construction authority in the catalog or report
  assembler.
- Parse diagnostics cross the parser, completeness-status, and project-report
  boundaries. Their stable code and internal failure meaning should be
  projections of one typed classification rather than parallel match tables.
- Parser configuration and syntax reporting are separate vocabularies. The
  accepted-language boundary must be auditable against the feature report so
  bounded parsing does not silently under-report compatibility requirements.

## Decisions

- Keep `RuleMetadata` as the public serialized catalog type. Make its fields
  private and add read-only accessors while preserving the current serde
  field names and JSON shape; a second CLI DTO would duplicate the contract.
- Let `ParseFailureKind` own the stable code and completeness classification,
  but keep generic status text in the status/report layer. Parser diagnostics
  need richer source-specific messages, while incomplete-status rendering is a
  separate report concern; both must project from the same failure kind.
- Proposal syntax is intentionally accepted for semantic linting. Extend the
  feature vocabulary and detector coverage to match the parser configuration;
  do not silently under-report accepted syntax and do not narrow the parser to
  the current incomplete report list.

## Coverage

Reviewed only Chunk 10, “Configuration, parsing, and runtime environment,”
from `CODEBASE_STRUCTURE_CORE.md`, including `CoreConfig`, diagnostic and
source-line types, ECMAScript version/feature analysis, host environment
identity and global-object policies, validated analysis limits, parser
admission and diagnostics, source-language syntax selection, syntax-depth
guarding/scanning, and TypeScript lowering. Existing Chunk 1 through Chunk 9
audit history was used to continue IDs at READ-038. No source, test,
configuration, dependency, or other documentation files were changed; this
chunk audit file is the only new artifact for Chunk 10.
