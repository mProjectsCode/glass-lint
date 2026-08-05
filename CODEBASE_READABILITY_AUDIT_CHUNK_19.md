# Codebase Readability Audit — Chunk 19

## Summary

Chunk 19 covers core configuration, diagnostics, ECMAScript feature
detection, environment registration, analysis limits, batch execution,
catalog/linter construction, reports, rule selection, and parsing. The
boundaries are mostly explicit: limits validate through constructors, batch
completion preserves input order, source ranges validate UTF-8 boundaries,
and parser depth checks are documented as conservative admission controls.

The concrete issues found here are concentrated in mutation and ownership
protocols. Batch environment registration can leave partial state after an
error, linter construction validates selectors and then independently
re-evaluates them, and the ECMAScript detector has two different definitions
of a default parameter that disagree for destructured arrow parameters.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Environment registration

#### [x] READ-094 — Make bulk global registration atomic or explicitly fallible

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Mutation protocol / Error handling / API
- **Location:** `glass-lint-core/src/environment.rs:161-173`

`Environment::add_globals` delegates each item to `add_global` and mutates
the environment immediately. If a later name is empty, invalid, or reserved,
the method returns `EnvironmentError` after earlier names have already been
registered. Callers therefore cannot treat a failed batch as an unchanged
configuration, and retrying the same input can produce a different starting
state. `add_global_object_with_members` already validates its complete member
set before registering, so the two bulk-registration APIs have different
failure semantics.

**Recommendation:** Validate and canonicalize all names into a temporary
bounded/deduplicated collection before applying any mutation, then register
the validated collection in one step; alternatively introduce a transaction
or document and name the operation as incremental. Preserve idempotent
insertion, deterministic ordering, and the existing identifier validation
rules. Delete the per-item mutation loop once one owner establishes the batch
commit boundary.

**Fix Applied:** `Environment::add_globals` now validates the complete input
into a temporary ordered set before mutating the environment. Failed batches
leave prior state unchanged, while successful registration remains idempotent
and deterministic; added an atomicity regression test.

**Verification:** `cargo test -p glass-lint-core environment::tests --lib`
(13 passed) and `make fmt && make ci` (passed).

### ECMAScript feature detection

#### [ ] READ-095 — Use the recursive default-pattern detector for arrows

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Semantic duplication / Feature detection / Drift
- **Location:** `glass-lint-core/src/ecma_version.rs:227-241,284-303,458-474`

`FeatureDetector::visit_function` calls the recursive `contains_default`
helper, which recognizes defaults nested in array, object, rest, and assign
patterns. `visit_arrow_expr` instead records `DefaultParameters` only when a
top-level parameter is `Pat::Assign`. A destructured arrow such as
`({ value = 1 }) => value` is consequently reported differently from an
equivalent ordinary function, despite both having a default parameter and
both being consumed by `EcmaVersionReport`.

**Recommendation:** Have arrow detection call `contains_default` for each
parameter pattern, preserving the current async/arrow feature recording and
child traversal. Delete the top-level-only predicate after the shared helper
owns the default-parameter definition, and add focused tests for nested object,
array, and rest patterns in both function forms.

**Fix Applied:** None so far.

### Rule selection ownership

#### [ ] READ-096 — Resolve rule selectors once during linter construction

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Selection lifecycle / Validation / Performance
- **Location:** `glass-lint-core/src/lint/selection.rs:208-229`,
  `glass-lint-core/src/lint/linter.rs:111-145`

`RuleSelection::validate_against` scans every catalog rule for every override
to decide whether a selector is known. After that succeeds,
`Linter::new` scans every catalog rule and every override again to compute the
effective state. Selector matching, declaration-order precedence, and
unknown/wildcard failure behavior therefore have two execution owners. A
future change to selector matching or override precedence can make validation
accept a configuration that construction resolves differently, while large
catalogs pay the duplicated `overrides × rules` work.

**Recommendation:** Add one selection-resolution operation that validates
against the catalog and returns the ordered enabled `RuleIndex` set (or a
validated selection phase type). Keep baseline confidence semantics and
last-declared override precedence in that owner, and have `Linter::new`
consume the result directly. Delete the separate validation scan and matching
loop after callers use the single resolution path; preserve deterministic
catalog order and the existing fail-closed treatment of unknown selectors.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 19’s APIs generally validate inputs at useful boundaries, but several
operations expose intermediate protocol details to callers. Bulk mutation
does not have the same commit semantics as other bulk configuration paths,
feature definitions are duplicated between AST visitors, and selection
validation is separate from selection resolution. Domain-owned commit,
pattern, and resolution phases would reduce semantic drift while keeping the
current deterministic behavior.

No findings are marked applied.

## Open Questions

- If incremental mutation after a failed `add_globals` call is intentional,
  the method should document that contract and callers should not infer
  transactional behavior from the `Result` return type.
- A validated selection phase should retain the catalog association or an
  equivalent generation identity if it is stored beyond `Linter::new`; a
  plain vector of indices is safe only for the catalog that produced it.
- The arrow default-pattern fix should confirm whether parser lowering ever
  represents destructured defaults differently for TypeScript parameters; the
  existing recursive helper is the natural shared semantic definition.
- The next unreviewed handoff is Chunk 20: project session, input,
  resolution/source tables, and report types listed in
  `CODEBASE_STRUCTURE_CORE.md`.

## Coverage

Reviewed the Chunk 19 modules listed in `CODEBASE_STRUCTURE_CORE.md` across
core configuration, diagnostics and source positions, ECMAScript version
detection, environment/global registration, analysis limits, batch execution,
catalog/linter construction, report assembly, rule selection, and parsing.
Representative callers were traced through environment setup, feature report
construction, linter initialization, catalog validation, batch completion,
and parser admission. Prior findings READ-036, READ-081, READ-082, and
READ-080 were checked to avoid repeating their root causes. No source, test,
configuration, dependency, or documentation changes were made.
