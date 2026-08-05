# Codebase Readability Audit — Chunk 15

## Summary

Chunk 15 covers the declarative rule API, query expressions and lifecycle
builders, normalized compiler IR, validation passes, physical roots, plan
requirements, and compiled catalog/selection boundaries. The slice has a
clear declaration-to-plan pipeline, bounded constructors, canonical
normalization, structured query diagnostics, and deterministic root ordering.

The new issues are concentrated at boundaries where ownership is duplicated
or validation context is discarded: identity/event compatibility is encoded
twice, same-event contradiction errors lose the actual variable, lifecycle
sink declarations retain two representations of one target, catalog lowering
can lose the rule that failed, selection assumes caller-provided ordering and
range validity, and the deferred lifecycle builder keeps only its latest
construction error.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Query validation and normalization

#### [ ] READ-073 — Centralize identity/event dimension compatibility

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Validation / Compiler ownership / Semantic drift
- **Location:** `glass-lint-core/src/api/compiler/validate/error.rs:175-235`,
  `glass-lint-core/src/api/compiler/contradiction.rs:23-65`

`is_valid_identity_event_pair` and `check_dimension_contradictions` both
encode which `IdentitySpec` variants are compatible with each `EventSpec`
variant. The first is used during the validation pass; the second is called
again while normalization merges a same-event `All`. They already duplicate
the identity/event matrix, but their context and error behavior differ, so a
new event or identity kind can be accepted by one path and rejected by the
other. `is_subject_identity_consistent` adds a third adjacent policy for
rooted and heuristic member spelling.

Give the validation layer one dimension-checking owner that returns a typed
compatibility result (including the subject-spelling check), and have both
the early pass and contradiction normalization consume it. Preserve the
ordered diagnostic precedence, direct-versus-subject distinctions, and
conservative rejection of unsupported pairs; delete the second hand-written
matrix after its callers use the shared result.

**Fix Applied:** None so far.

#### [x] READ-074 — Preserve the merged event variable in contradiction diagnostics

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Diagnostic provenance / API correctness
- **Location:** `glass-lint-core/src/api/compiler/normalize_all.rs:131-224`

`SameEventMerge` receives the actual `event_var` in `merge_predicate` and
`finish`, but `merge_event_kind`, `merge_identity`, and `merge_subject`
construct `QueryCompileError::ContradictoryPredicate` with
`VarId::new(0)`. The public `VarId` type permits authored nonzero IDs, and
the normalizer explicitly alpha-renumbers them later, so a contradictory
same-event query can report the wrong variable even though the compiler
knows which binding it merged.

Store the event variable on `SameEventMerge` or pass it to the three merge
helpers. Preserve contradiction classification and normalization order, and
remove the hardcoded zero once every error carries the owning binding.

**Fix Applied:** `SameEventMerge` now owns the merged event variable and uses
it for event-kind, identity, and subject contradiction diagnostics instead of
hardcoding `VarId::new(0)`. Added coverage with an authored nonzero event
variable.

**Verification:** `cargo test -p glass-lint-core
api::compiler::tests::normalize::algebra --lib` (23 passed) and `make fmt &&
make ci` (passed).

### Lifecycle declaration representation

#### [ ] READ-075 — Give lifecycle sinks one owner for target identity

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Semantic newtype / Representation duplication / API surface
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:9-16,332-431`

`LifecycleSinkKind::ArgumentOf` and `AnyArgumentOf` retain both a
`MemberChain` and a derived `LifecycleCallTarget`. For rooted member sinks,
the `MemberChain` already owns the parsed `SymbolPath` that is copied into
`LifecycleCallTarget::RootedMember`; for global sinks, the target is likewise
derived from the chain's canonical display spelling. Runtime compilation uses
the target, while explanation uses the chain, so every sink carries two
representations of the same endpoint and the invariant that they agree is
implicit.

Let a lifecycle endpoint newtype own the canonical target and expose the
display chain needed by diagnostics, or retain only the target plus a
purpose-specific display accessor. Preserve rooted-path parsing, global-name
validation, deterministic ordering, sink equality/deduplication, and the
runtime target consumed by `CompiledObjectSink`; remove the parallel chain
storage after all explanation callers use the owner.

**Fix Applied:** None so far.

### Catalog and selection boundaries

#### [x] READ-076 — Keep the offending rule ID on every catalog compilation error

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** External API / Error context / Catalog ownership
- **Location:** `glass-lint-core/src/api/compiler/catalog.rs:12-35`,
  `glass-lint-core/src/api/rule/error.rs:61-71`,
  `glass-lint-core/src/lint/catalog.rs:69-83`

`compile_records` preserves `rule.id()` for query and query-build errors,
but maps every other `MatcherBuildError` to `CompiledCatalogError::InvalidMatcher(String)`
without the rule ID. `RuleCatalog::new` then fabricates the provider-local
`provider:compile` ID for that error. A physical-plan or lowering failure is
therefore reported as belonging to the compile sentinel rather than the rule
whose declaration produced it, which is especially misleading in a catalog
with many rules.

Make the catalog error carry the local rule ID for all compilation branches,
or map all matcher failures through one rule-context wrapper before they leave
the compiler. Preserve the existing structured query diagnostic code/message,
provider namespacing, and declaration order; remove the synthetic compile ID
once the real owner is retained.

**Fix Applied:** `CompiledCatalogError::InvalidMatcher` now carries the local
rule ID alongside its message. Provider catalog conversion qualifies that ID
directly and no longer fabricates a `provider:compile` sentinel.

**Verification:** `cargo test -p glass-lint-core api::compiler --lib` and
`cargo test -p glass-lint-core lint::catalog --lib`; `make fmt && make ci`
(all passed).

#### [ ] READ-077 — Validate selected rule indices and ordering at the selection boundary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Invariant validation / Silent omission
- **Location:** `glass-lint-core/src/api/compiler/rule.rs:14-43`

`CompiledRuleSelection::new` accepts an arbitrary `&[RuleIndex]` without
checking that indices are in range or sorted. `selected_matchers` silently
drops out-of-range entries with `filter_map`, while `is_selected` calls
`binary_search` and therefore gives incorrect answers for an unsorted slice.
The type is passed through lint and project selection code as if it were a
canonical selection, but the constructor does not establish that invariant
or return an error when a stale/foreign selection is supplied.

Give the boundary a validated selection type or make `new` return a typed
error after checking range, uniqueness, and sorted order. Preserve stable
rule indices, deterministic iteration, and the ability to represent an empty
selection; delete silent filtering and the unchecked binary-search assumption
once callers receive the validated form.

**Fix Applied:** None so far.

### Deferred authoring errors

#### [x] READ-078 — Preserve the first deferred lifecycle-builder error

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Builder lifecycle / Error determinism / API consistency
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:470-583`,
  compared with `glass-lint-core/src/api/rule/mod.rs:107-224`

`LifecycleQueryBuilder` stores one `invalid_operation`, but every failing
`source`, `condition`, or `completion` call overwrites it. The builder can
therefore report a later duplicate-stage error while hiding an earlier
invalid source, and its deferred-error policy differs from `RuleBuilder`,
which deliberately retains `first_query_error`. This makes chained
declarative construction sensitive to which later operations happen to run
after the first failure and gives callers different error-selection semantics
for equivalent builders.

Use the same first-error accumulator policy as `RuleBuilder`, or retain a
bounded ordered error list if lifecycle diagnostics need aggregation. Preserve
the immediate `try_*` failure behavior and stage-presence validation; delete
the overwrite path after deferred construction has one documented owner.

**Fix Applied:** Added a first-error accumulator to
`LifecycleQueryBuilder`. Deferred source, condition, and completion
failures now preserve the earliest error, matching `RuleBuilder` while
immediate `try_*` failures remain unchanged.

**Verification:** `cargo test -p glass-lint-core
api::rule::query::lifecycle --lib` (26 passed) and `make fmt && make ci`
(passed).

## Systemic Themes

Chunk 15’s strongest design is its typed, bounded declaration boundary and
single normalization pipeline. The remaining complexity is mostly at seams:
validation rules are repeated instead of owned by one semantic predicate,
compiler diagnostics can lose provenance, and wrapper types expose internal
ordering/representation assumptions to callers. Small domain owners for
dimensions, lifecycle endpoints, catalog errors, and selections would make
the compiler easier to extend without adding another matcher path.

READ-074, READ-076, and READ-078 are marked applied above; the remaining
findings in this chunk are open.

## Open Questions

- The compatibility owner should decide whether rooted member spelling
  consistency remains a validation error or becomes part of a typed identity
  constructor; either choice should remove the duplicate event matrix.
- `CompiledRuleSelection` may be intentionally internal and trusted, but its
  public methods already expose behavior that depends on sorted indices; a
  fallible constructor is preferable if stale selections can cross a session
  boundary.
- If compiler failures are considered impossible after validated rule
  construction, retaining the rule ID is still useful for diagnosing internal
  regressions and future compiler extensions.
- The next unreviewed handoff is Chunk 16: runtime, linting, and project API
  modules.

## Coverage

Reviewed the Chunk 15 modules listed in `CODEBASE_STRUCTURE_CORE.md` across
rule metadata/builders, module patterns and taxonomy, event and lifecycle
query declarations, logical composition, value/argument constraints, query
validation, contradiction detection, normalization, object-flow lowering,
physical planning, requirements, compiled records, and selection APIs.
Representative callers were traced through provider rule construction,
catalog compilation, linter selection, local/project analysis, and lifecycle
flow execution. The focused compiler suite passed: 143 tests. Earlier scope,
trace, flow, and static-value findings were checked to avoid re-reporting
those ownership issues.
No source, test, configuration, dependency, or documentation changes were
made.
