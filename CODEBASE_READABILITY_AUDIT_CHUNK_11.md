# Codebase Readability Audit

## Summary

Chunk 11 owns linter construction and execution, bounded batch delivery,
rule-selection resolution, deterministic range policy, and conversion of
classified evidence into project reports. The execution boundaries are
mostly coherent and the batch contract is well tested, but report ordering
does redundant work, selection validation and resolution expose avoidable
state/scan complexity, and one implementation type is wider than its owning
API requires.

## Findings

### Finding ordering and duplicate assembly

#### [ ] READ-080 — Make duplicate merging own the single finding sort

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Performance
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:99-105`, `glass-lint-core/src/lint/report/evidence.rs:113-151`

`populate_project_files` calls `merge_duplicate_findings` and then sorts the
returned vector with `compare_findings`. The merge helper already sorts its
input with the same comparator before merging adjacent primary-identity
duplicates. Its pop/merge/push loop preserves that order: a merged finding
retains the first sorted item’s ordering key, and non-duplicates are appended
in sorted order. The second sort therefore repeats an O(n log n) pass for
every module, precisely on the report path that is intended to provide
deterministic output.

**Recommendation:** Make one helper own both the canonical ordering and
duplicate reduction, then remove the caller-side `findings.sort_by` (or make
the helper accept an explicitly documented already-sorted input if another
caller needs that contract). Keep the full position/rule/message/severity
ordering, primary identity of `(rule_id, location)`, evidence union,
certainty merge, and deterministic file insertion behavior. Add or retain a
focused test proving duplicate reduction remains ordered when duplicate
messages/evidence are present.

**Fix Applied:** None so far.

### Rule-selection evaluation and construction boundary

#### [ ] READ-081 — Collapse selection modes and avoid repeated catalog scans

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API / Performance
- **Location:** `glass-lint-core/src/lint/selection.rs:251-337`, `glass-lint-core/src/lint/linter.rs:107-120`, `glass-lint-cli/src/config.rs:264-273`, `glass-lint-cli/src/config.rs:348-362`

`SelectionEvaluation` stores enabled indexes in an `Option<Vec<RuleIndex>>`
solely because `validate` and `resolve` select different modes. The resolve
branch then recovers that invariant with an `expect` at lines 299-301. Both
public validation and linter construction otherwise execute the same full
catalog/override loop and the same unmatched-override validation. The direct
CLI path makes the architectural cost visible: configuration validation
constructs a baseline linter/catalog and scans it with `RuleSelection::validate`,
then selected-linter construction builds the provider configuration again and
scans the assembled catalog through `Linter::new` and `resolve`.

**Recommendation:** Give selection evaluation one result shape that always
contains the enabled indexes, so validation can inspect the match bitmap and
discard the indexes without a mode flag or runtime assumption. At the
construction boundary, choose one owner for catalog-bound selection
resolution: either let the linter construction be the sole authoritative
validation and make the CLI reuse that prepared result, or introduce an
explicit validated/prepared catalog-selection value that carries the resolved
indexes into `LinterConfig`/`Linter::new`. Preserve declaration-order override
precedence, deterministic catalog-order indexes, exact-versus-wildcard error
classification, and the CLI’s desired timing for reporting invalid rules.

**Fix Applied:** None so far.

### Selector implementation visibility

#### [ ] READ-082 — Keep `RuleSelector` private to rule selection

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/lint/selection.rs:37-39`, `glass-lint-core/src/lint/selection.rs:136-225`, `glass-lint-core/src/lint/selection.rs:228-240`

`RuleSelector` is declared `pub`, and its methods `as_str` and
`has_wildcard` are also public, but the `lint::selection` module is private at
the crate boundary and the type is neither re-exported nor constructible
except through the private `parse` method. All production use is internal:
`RuleOverride::new` parses it, `RuleOverride` exposes only the selector string
and state, and `RuleSelection` performs matching. The public declaration
therefore advertises implementation detail inside the module without adding a
usable external API, while keeping the wildcard representation and original
raw text coupled to the owner of the rule-override contract.

**Recommendation:** Make `RuleSelector` and its implementation methods
private, retaining `RuleOverride::new`, `selector`, and `state` as the narrow
validated API. Keep the parsed pattern and raw serialization text together
inside the override implementation, preserve serde’s string representation and
all wildcard validation/matching behavior, and keep the existing unit tests
focused on the private parser/matcher.

**Fix Applied:** None so far.

## Systemic Themes

- The batch state machine has a clear bounded, lazy, input-ordered contract;
  its integration tests cover cancellation, malformed inputs, duplicate paths,
  and equivalence with single-source linting. The main execution-level
  simplification opportunity is at the construction/validation boundary,
  where callers can currently repeat catalog work.
- Deterministic output policy is correct but should be owned by one report
  helper rather than enforced by successive sorting layers.
- Selection and override internals preserve fail-closed semantics, but the
  internal parsed representation should remain private and the validation
  result should make its invariants explicit in its type.

## Open Questions

- READ-081 needs a deliberate product choice about whether invalid rule
  selections must fail during CLI config loading or only when the executable
  linter is constructed. The implementation should preserve whichever timing
  contract is selected rather than silently moving the user-visible error.
- No prior finding was duplicated: Chunk 09’s READ-075 covers structured
  catalog error information being collapsed at the catalog/linter error
  boundary, while READ-081 concerns repeated selection evaluation and
  construction work.

## Coverage

- Reviewed: `lint::batch`, `lint::linter`, `lint::ranges`, `lint::selection`,
  `lint::report`, `lint::report::diagnostics`,
  `lint::report::evidence`, and `lint::report::summary`; their project-session
  bridge, CLI construction callers, public exports, and focused tests.
- Verification: `cargo test -p glass-lint-core --test integration batch` (6
  passed), `cargo test -p glass-lint-core --test integration linter` (16
  passed), `cargo test -p glass-lint-core selection --lib` (22 passed), and
  `cargo test -p glass-lint-cli config --lib` (6 passed).
- No source, test, configuration, dependency, or existing audit artifact was
  modified. This chunk artifact is the only new file for this review turn.
- Historical audit chain: Chunk 10 ended at READ-079. The next chunk is Chunk
  12, “Project sessions, inputs, and reports,” which should continue with
  READ-083.
