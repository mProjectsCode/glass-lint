# Codebase Readability Audit — Chunk 11

## Summary

Chunk 11 owns the runtime lint boundary: compiling and combining catalogs,
resolving selections, running one-file or bounded batch linting, and converting
classified evidence into deterministic findings and diagnostics. The phase
types in report assembly are justified because linking, matching, rendering,
and finalization have different available data and status transitions. The
findings below target state that the prepared configuration path stores and
then ignores, a per-rule grouping map that is immediately flattened before a
global sort, and a trace arena initialized only to be replaced at the next
phase.

## Findings

### Prepared linter configuration

#### [x] READ-047 — Prepared linter construction retains a complete fallback configuration that it discards

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/linter.rs:20-30,51-71,122-139`; `glass-lint-core/src/lint/selection.rs:260-279`; `glass-lint-cli/src/config.rs:277-290,378-390`

`LinterConfig` carries both the unprepared inputs (`catalogs` and
`selection`) and an optional `PreparedRuleSelection`. `with_prepared_rules`
clones the prepared value's complete `RuleSelection` into the ordinary
`selection` field while retaining the prepared catalog and enabled indexes.
When `Linter::new` sees `prepared_selection`, it consumes only that prepared
catalog and index vector; the copied selection and the original catalog vector
are never read and are dropped with the builder. The CLI exercises this path
after it has already prepared an exact combined catalog, so construction also
holds a second compiled-catalog representation until the builder is consumed.

**Recommendation:** Make the prepared and unprepared construction modes
explicitly exclusive. Keep the prepared selection as the sole owner of the
effective selection in that mode (the accessor can read through
`prepared_selection`), move/discard the fallback catalogs when the prepared
artifact is installed, and let `Linter::new` consume only the prepared catalog
and enabled indexes. Preserve the public builder's ability to inspect the
effective selection, keep `PreparedRuleSelection` catalog/index alignment
authoritative, and retain the existing unprepared path's catalog combination
and error mapping.

**Fix Applied:** Linter rule inputs now use an explicit private unprepared or
prepared mode. Installing prepared rules discards fallback catalogs and the
selection accessor reads the prepared selection directly; the unprepared path
still combines catalogs and resolves overrides with the same errors. Verified
with `make fmt && make ci`.

### Finding assembly

#### [ ] READ-048 — Finding assembly groups by rule in a map that is immediately flattened and globally resorted

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:92-109,135-171`

`findings_for_module` inserts every capability's findings into a
`BTreeMap<RuleIndex, Vec<Finding>>`, then consumes the map with
`into_values().flatten().collect()`. The caller immediately passes that
vector to `merge_duplicate_findings`, which sorts all findings by source
range, rule ID, message, and severity; consequently the map's rule-index
ordering does not survive to the report. The intermediate map and one vector
per encountered rule add grouping allocations and a flatten pass without
providing an ordering or ownership invariant that the next stage uses.

**Recommendation:** Accumulate findings directly in one module-owned vector
and retain the existing global `merge_duplicate_findings` sort/merge as the
single ordering and duplicate boundary. Preserve capability traversal,
cross-capability duplicate merging, empty-evidence behavior, deterministic
rule/location ordering, and the catalog lookup used to construct each
finding; remove only the immediately consumed rule-index map and flattening.

**Fix Applied:** None so far.

### Report-session lifecycle

#### [ ] READ-049 — Project report sessions allocate an unused trace arena before matching supplies the real one

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/lint/report/mod.rs:63-73,95-113,143-160,178-210`; `glass-lint-core/src/analysis/trace.rs:69-91`

`ProjectReportSession::new` creates a `TraceArena` and stores it in the
session while linking. `LinkedReport::match_project` then receives the arena
created and populated by `classify_with_evidence_limit` and immediately
replaces the session field through `set_trace_arena`; every trace lookup and
trace count occurs only after that replacement. The initial arena is empty,
but it still creates a separate arena identity and requires a setter whose
only production call is this one phase handoff, leaving trace ownership split
between a placeholder and the classification owner.

**Recommendation:** Let the matching phase install the arena as part of the
session transition, or move trace storage to the matched/rendered report
state and pass the completed arena into the report session constructor.
Preserve the arena identity returned with classification results, invalid or
foreign trace-handle rejection, zero-arena behavior on a rule-selection
failure, status recording, and final trace-node metrics; remove only the
synthetic pre-match arena and replacement setter.

**Fix Applied:** None so far.

## Systemic Themes

- A prepared selection is a phase-owned compiled artifact. Builder fields for
  the unprepared path should not remain parallel authoritative-looking state
  once that artifact is installed.
- Report assembly already has a single deterministic sort/merge boundary;
  upstream collections should feed that owner directly unless their ordering
  or invariant is consumed by an intermediate phase.
- Trace handles are valid only with the arena that created them. The report
  lifecycle should transfer that arena once from projection/matching to
  evidence rendering rather than create a placeholder with a different
  identity.
- Catalog combination, batch backpressure, and linked/matched/rendered phase
  types were not reported merely for having multiple states: each preserves a
  distinct validation, resource, or lifecycle invariant.

## Open Questions

- `LinterConfig::selection()` remains an effective-selection accessor after
  `with_prepared_rules`; it should read the prepared value rather than retain
  a clone.
- Direct finding accumulation remains bounded by the classification/evidence
  limits and is followed by the existing sort/merge boundary; no intermediate
  rule map is required for determinism.
- No production caller needs a report session between linking and matching.
  The trace arena should therefore transfer once at the match transition,
  with the zero-capacity fallback retained only for selection failure.

## Coverage

Reviewed the chunk-11 structure entries and their implementation/test support:

- `lint/{batch,catalog,linter,ranges,selection}.rs`
- `lint/report/{mod,diagnostics,evidence,summary}.rs`
- catalog compilation and prepared-selection callers in the CLI and harness
- batch ordering, bounded input, cancellation, and canonical-report tests
- core architecture guidance for linter cache ownership, batch bounds,
  deterministic reports, and private executor storage
- Existing numbered audit reports 001–010 were checked to avoid duplicating
  their historical findings.

No source, test, configuration, dependency, or other documentation files
were changed by this audit.
