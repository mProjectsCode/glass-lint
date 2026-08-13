# Codebase Readability Audit

## Summary

Chunk 9 covers classification state and the provider-neutral query compiler:
validation, normalization, contradiction detection, requirement derivation,
physical planning, rule selection, and catalog compilation. The compiler has
good ownership boundaries and preserves bounded, deterministic execution, but
four architectural seams still duplicate canonicalization work or discard
useful error structure. The most correctness-sensitive finding is the custom
normalized-root ordering key, which does not order the full semantic value it
deduplicates.

## Findings

### Query normalization and physical-plan preparation

#### [ ] READ-072 — Let contradiction checking consume canonical argument groups

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:97-140`, `glass-lint-core/src/api/compiler/normalize.rs:469-487`, `glass-lint-core/src/api/compiler/normalize_all.rs:299-311`, `glass-lint-core/src/api/compiler/contradiction.rs:41-55`

`CanonicalArgumentConstraints::from_constraints` already sorts, deduplicates,
and groups every argument predicate. Both event-normalization paths then call
`to_flat_vec` solely to pass the result to `detect_event_contradictions`, whose
`check_argument_contradictions` immediately rebuilds a `BTreeMap` of the same
groups. This allocates and traverses a second representation and gives the
contradiction checker its own copy of the canonical-group invariant.

**Recommendation:** Make `detect_event_contradictions` accept
`&CanonicalArgumentConstraints` and let it inspect each existing
`ArgumentConstraintGroup` directly. Remove the temporary flat vectors from
`normalize_event_from_query` and `CompleteSameEventMerge::into_root`, then
delete the checker’s `BTreeMap` regrouping. Preserve the existing empty-set,
exact/prefix intersection, boundedness, and fail-closed contradiction results;
`to_flat_vec` should remain only for consumers that genuinely need the public
declaration-shaped representation.

**Fix Applied:** None so far.

#### [ ] READ-073 — Make normalized-root ordering total over semantic state

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/normalize.rs:491-541`, `glass-lint-core/src/api/compiler/normalized.rs:197-242`

`sort_roots` and `branches.dedup()` use different notions of equality.
`NormalizedEvent` equality includes `subject`, but the `NormalizedRoot::Event`
arm of `compare_roots` compares only slot, event, identity, and arguments; it
omits the returned/instance/direct subject relation. Consequently, distinct
normalized roots can compare equal to the sort, remain distinct to `dedup`,
and retain authored branch order instead of a canonical order. That weakens
the compiler’s documented deterministic normalization and makes later plan or
explanation ordering sensitive to declaration order for semantically distinct
subjects.

**Recommendation:** Put the canonical semantic ordering key on the normalized
types (or derive/use one complete `Ord` representation) and make both sorting
and equality-based deduplication use the same complete state, including the
subject relation. Add reversed-branch tests covering direct, returned-object,
and constructed-object subjects that share event/identity/argument fields.
Do not deduplicate roots that differ in subject, slot, evidence, or any other
state required by physical planning; only make their ordering total and
stable.

**Fix Applied:** None so far.

#### [ ] READ-074 — Remove the duplicate same-event merge state type

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/normalize_all.rs:135-163`, `glass-lint-core/src/api/compiler/normalize_all.rs:253-314`

`SameEventMerge` accumulates the event, identity, subject, constraints, and
member-object state, then `finish` moves every field into
`CompleteSameEventMerge`, which declares the same six fields again and has only
one consumer, `into_root`. The second type does not establish a new invariant
or ownership boundary; it is a positional reconstruction step that makes the
same-event normalizer harder to follow and easier to change inconsistently.

**Recommendation:** Make `SameEventMerge::into_root(self)` perform the final
member-subject checks, contradiction detection, canonical constraint creation,
and root construction directly, then delete `finish` and
`CompleteSameEventMerge`. Keep the current error ordering and distinctions
between incomplete, uncorrelated, contradictory, and successfully merged
queries.

**Fix Applied:** None so far.

### Catalog and public error boundary

#### [ ] READ-075 — Preserve compiler error categories through catalog APIs

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/lint/catalog.rs:64-80`, `glass-lint-core/src/api/rule/error.rs:140-160`, `glass-lint-core/src/lint/linter.rs:111-120`, `glass-lint-core/src/lint/selection.rs:360-384`

`CompiledCatalogError` deliberately distinguishes an invalid authored query,
compiler invariant failure, invalid physical plan, and invalid matcher. The
provider catalog immediately converts all four to `ProviderCatalogError::InvalidRule`
with a string, and `Linter::new` converts that string-only variant again to
`LintConfigError::InvalidRule`. Callers can display the message but cannot
match the failure category, distinguish user rule defects from internal
compiler failures, or retain a structured source error for diagnostics.

**Recommendation:** Own the conversion at the public catalog boundary with a
structured provider-facing diagnostic (or a categorized `InvalidRule` source)
and thread that value through `LintConfigError`; consolidate the repeated
stringification match arms. Keep provider-neutral compiler types behind the
boundary if necessary, but preserve the distinction between authored-query,
matcher, physical-plan, and compiler-invariant failures and retain stable
rule IDs and display text.

**Fix Applied:** None so far.

## Systemic Themes

- Canonical representations are generally well-owned, but a few callers still
  flatten and regroup them instead of making the canonical type the sole owner
  of the operation.
- Custom ordering keys should cover the same semantic state as equality. The
  current normalized IR otherwise risks declaration-order leakage despite the
  compiler’s deterministic-plan contract.
- The compiler keeps internal diagnostics structured until the provider
  catalog boundary, where public APIs currently collapse them into strings.

## Open Questions

- None blocking these findings. READ-073 should be implemented before changing
  normalization or physical-plan ordering tests, because those tests should
  encode the complete subject-sensitive canonical order.

## Coverage

- Reviewed: `api::classification`, all `api::compiler` modules in the chunk,
  compiler validation/normalization/physical-plan tests, rule selection and
  catalog compilation, `RuleCatalog`, `RuleMetadata`, provider catalog errors,
  and the linter configuration error boundary.
- Verification: `cargo test -p glass-lint-core api::compiler --lib` — 154 passed;
  `cargo test -p glass-lint-core --test integration public_surface` — 3 passed;
  `cargo test -p glass-lint-core --test integration query::composition` — 30
  passed.
- No source, test, configuration, dependency, or existing audit artifact was
  modified. This chunk artifact is the only new file for this review turn.
- Historical audit chain: Chunk 8 ended at READ-071. The next chunk is Chunk
  10, “Configuration, parsing, and runtime environment,” which should continue
  with READ-076.
