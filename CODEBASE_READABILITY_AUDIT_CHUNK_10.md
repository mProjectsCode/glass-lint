# Codebase Readability Audit

## Summary

Chunk 10 owns provider-neutral configuration, parser diagnostics and source
coordinates, ECMAScript feature reporting, host-environment identity policy,
and validated analysis limits. The configuration and environment invariants
are generally well-contained, but the parser error boundary exposes mutable
representation, the syntax feature visitor performs avoidable pre-traversals,
and two small APIs repeat conversion/allocation work at their owning types.

## Findings

### Parsing and public diagnostics

#### [x] READ-076 — Make `ParseDiagnostic` an encapsulated, usable error

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/parse.rs:25-55`, `glass-lint-core/src/lib.rs:31-42`, `glass-lint-core/src/project/types/report/diagnostic.rs:5-35`

`ParseDiagnostic` exposes `code`, `message`, `filename`, and `range` as public
mutable fields while retaining its `ParseFailureKind` privately. A caller can
therefore mutate the serialized fields without updating the internal failure
classification, and the public error does not implement `Display` or
`std::error::Error`. Neighboring public diagnostics and compiler query
diagnostics keep state private and expose read-only accessors, so parser
callers receive a less coherent contract despite `ParseDiagnostic` being a
root-exported result type.

**Recommendation:** Make the diagnostic fields private, add read-only accessors
for the stable code, message, filename, and range, and expose a typed failure
kind accessor if callers need classification beyond the code. Implement
`Display` and `Error` using the structured fields, then update the core/output
callers to use the accessors and remove direct field mutation. Preserve the
serialized shape, stable diagnostic codes, authored filename, optional source
range, and the distinction between syntax, source-size, and syntax-depth
failures.

**Fix Applied:** `ParseDiagnostic` fields are now private with read-only code,
message, filename, range, and failure-kind accessors. It implements
`Display`/`Error`, while retaining the serialized field shape and distinct
failure classification. Updated parser, report, output, and integration
callers. Verified with `make fmt && make ci`.

**Audit disposition (2026-08-13):** Confirmed. Encapsulation must preserve the
serialized field names and give parser callers read-only access to the same
authored values and failure classification.

### ECMAScript feature detection

#### [x] READ-077 — Record parameter and spread features during the normal AST visit

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Performance
- **Location:** `glass-lint-core/src/ecma_version.rs:247-318`, `glass-lint-core/src/ecma_version.rs:430-498`, `glass-lint-core/src/ecma_version.rs:501-515`

`FeatureDetector` recursively scans every function/arrow parameter with
`contains_default` and scans object-pattern/object-literal properties with
`any(...)`, then immediately calls `visit_children_with(self)`, which walks
those same AST nodes again. The duplicate work is bounded, but it adds a
second traversal precisely in the standalone syntax-analysis path and keeps
feature ownership split between ad-hoc pattern walkers and the SWC visitor.

**Recommendation:** Give `FeatureDetector` a narrow parameter-pattern/object-
property context and record default/rest features from the corresponding SWC
visitor callbacks during the single normal traversal; delete
`contains_default` and the property pre-scans once coverage is equivalent.
Keep the context-sensitive distinction that a default inside a function
parameter is `DefaultParameters`, while ordinary destructuring defaults are
not, and preserve the separate `RestAndSpread` versus `ObjectRestSpread`
feature flags and deterministic feature ordering.

**Fix Applied:** `FeatureDetector` now records parameter defaults and pattern /
object spread features from visitor callbacks during one normal traversal;
the recursive default helper and property pre-scans were removed. Verified
with `make fmt && make ci`.

**Audit disposition (2026-08-13):** Confirmed. The normal visitor may carry
only the context needed to distinguish parameter defaults from ordinary
destructuring; it should not add a second whole-tree walk.

### Host-environment input APIs

#### [x] READ-078 — Avoid forced `String` intermediates for global identifiers

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/environment.rs:140-165`, `glass-lint-core/src/environment.rs:187-239`

The public `Environment` registration methods accept `impl Into<String>` and
then immediately borrow that temporary string for validation before converting
the value into `SmolStr`. Literal and borrowed names consequently pay for an
intermediate `String`, and the bulk methods repeat that conversion for every
entry before inserting into the already-owned `BTreeSet`. The environment’s
actual domain value is a validated identifier stored as `SmolStr`, not an
owned `String` at the API boundary.

**Recommendation:** Make the registration boundary accept a borrowed or
directly small-string-compatible input (`AsRef<str>` or `Into<SmolStr>`), and
have one owner-side helper validate it and produce the canonical `SmolStr`.
Reuse that helper in single and bulk global/object registration while keeping
bulk validation atomic. Preserve reserved-word rejection, identifier-only
semantics, deterministic set ordering, and the current configured-versus-
restricted global-object behavior.

**Fix Applied:** Environment registration now accepts `AsRef<str>` inputs and
shares one validated `SmolStr` conversion across single and bulk APIs without
changing atomic validation or identifier semantics. Verified with `make fmt &&
make ci`.

**Audit disposition (2026-08-13):** Confirmed. Prefer direct `SmolStr`
conversion or borrowed validation with one canonical owner; retain atomic bulk
validation and do not change identifier semantics.

### Source-coordinate conversion

#### [x] READ-079 — Centralize offset-to-validated-range conversion

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/diagnostic.rs:179-216`

`SourceLineIndex::byte_range_from_offsets` and
`SourceLineIndex::range_from_offsets` each construct a `ByteRange` from raw
`u32` offsets, map construction failure to `OutOfBounds`, and call
`validate_range`. Only their final output differs. Keeping the same boundary
conversion in two methods makes future changes to byte-range validation or
error mapping easy to apply to one path but not the other.

**Recommendation:** Add one private `SourceLineIndex` operation that turns
raw offsets into a `ValidatedByteRange` (or a validated `ByteRange` plus the
private coordinates), and have both methods delegate to it. Retain the public
`try_range(ByteRange)` contract and the separate raw-byte versus display-range
outputs; preserve UTF-8 boundary checks, out-of-bounds errors, EOF handling,
and parser-versus-semantic caller behavior.

**Fix Applied:** `SourceLineIndex` now centralizes raw-offset conversion and
validated boundary admission for both byte-range and display-range callers.
Verified with `make fmt && make ci`.

**Audit disposition (2026-08-13):** Confirmed. Centralize raw-offset admission
and validation while leaving byte-range and display-range outputs as separate
operations.

## Systemic Themes

- The core configuration and environment types correctly hide their storage
  and preserve deterministic, fail-closed semantics; the remaining API issue
  is consistency at the parser diagnostic boundary.
- Syntax analysis mixes a shared visitor with local pre-scans. Context-aware
  visitor ownership can remove the duplicate work without weakening the
  distinction between ECMAScript feature categories.
- Small conversion helpers should own repeated validation and allocation
  boundaries rather than making each public method reproduce them.

## Open Questions

- None blocking these findings. Historical READ-056 was rechecked and not
  duplicated: it covers the retained semantic artifact’s second source-line
  index, whereas this chunk’s READ-079 concerns duplicated raw-offset
  validation within `SourceLineIndex` itself.

## Coverage

- Reviewed: `config::CoreConfig`, `diagnostic`, `ecma_version`, `environment`,
  `limits`, and `parse`; their root exports, semantic-analysis and CLI callers,
  public diagnostic counterparts, and focused unit/integration tests.
- Verification: default focused tests passed for limits (12), parsing (37),
  environment (13), ECMAScript version detection (9), diagnostics (32), and
  public surface (3). With the `serde` feature, limits (14), parsing (37),
  and public surface (4) also passed.
- No source, test, configuration, or dependency file was modified. This chunk
  artifact was updated with review dispositions only.
- Historical audit chain: Chunk 9 ended at READ-075. The next chunk is Chunk
  11, “Lint execution and reporting,” which should continue with READ-080.
