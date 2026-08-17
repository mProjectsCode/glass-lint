# Codebase Readability Audit — Chunk 20: Classification and Compilation

## Summary

Read-only audit of `glass-lint-core/src/api/classification*.rs` and
`glass-lint-core/src/api/compiler/**` ("Classification and compilation"). The
pipeline is well documented and the compile boundary is correctly sealed
(`CompiledMatcherPlan` is the only plan type consumed by `analysis` and `lint`;
normalized IR is `pub(crate)` in a `pub(crate) mod compiler`, `api/mod.rs:8`).
The findings below center on repeated re-derivation of the same decisions
across the validate → normalize → plan phases (READ-002, READ-005), duplicated
limit bookkeeping across normalize/physical/sealing (READ-004), a phantom
`Option` in the lifecycle IR (READ-003), an unprincipled split of
classification accessor methods across files (READ-001), and two crate-private
wrapper/owner questions (READ-007, READ-008). The declaration/IR emptiness
policy duplication is reported in full by chunk 21 (its READ-002); this chunk
records a cross-reference only (READ-006) to avoid double-reporting.

No source was modified; this session edits only this audit document.

## Findings

### Classification evidence API

#### [x] READ-001 — Classification accessors for the same type are split between `classification.rs` and `classification/result.rs` with no stated criterion

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/api/classification.rs:90-215`; `glass-lint-core/src/api/classification/result.rs:22-48`

Documentation-method accessors for one type are separated across two modules
with no rule stated in either. On `MatchedCapability`, `rule_index()` lives in
`classification.rs:105-107` while `label()`, `severity()`, and `evidence()`
live in `result.rs:22-37`. On `ClassificationEvidence`, `count()`,
`is_truncated()`, `certainty()`, and `occurrences()` live in
`classification.rs:179-193` while `kind()` and `symbol()` live in
`result.rs:39-48`. `ClassificationEvidenceOccurrence` accessors all stay in
`classification.rs` (`span()` at 115-117, `fact()` at 119-121, `trace()` at
123-125). The split is not visibility-driven (`result.rs` is not a "public
accessor" file — the `pub` `span`/`count`/`is_truncated`/`certainty`/
`occurrences` accessors stay in `classification.rs`), so a reader must hold
both files to enumerate one type's surface; `result.rs` is 49 lines and exists
mainly to define `ClassificationResult` (re-exported at `classification.rs:314`)
and host accessor impls for the other two types.

**Recommendation:** Consolidate every accessor onto its owning type in
`classification.rs`, leaving `ClassificationResult` and its two
`push_capability`/`capabilities` methods (`result.rs:4-20`) — the only type
actually defined in the submodule — in place, or delete `classification/` and
define `ClassificationResult` in `classification.rs` directly. Guardrails: keep
the pub/`pub(crate)` visibility split exactly as it is — `label`/`severity`/
`evidence`/`kind`/`symbol`/`count`/`is_truncated`/`certainty`/`occurrences`/
`span` are the public classification surface (`api/mod.rs:7` exports `pub mod
classification`), while `rule_index`/`fact`/`trace` stay `pub(crate)` as
internal correlation keys (`classification.rs:16-18,85-87`) that report
rendering still reads (`lint/report/evidence.rs:62,147`); leave the `MatchKind`
re-export (`classification.rs:13`) untouched.

**Fix Applied:** Moved the `MatchedCapability` and `ClassificationEvidence`
accessors into their owning type implementations in `classification.rs`, while
leaving `ClassificationResult` and its accessors in `classification/result.rs`.
Preserved the public and crate-private visibility split. Verified with
`make fmt && make ci`.

### Normalize/validate correlation

#### [x] READ-002 — Same-event correlation is computed twice on identical `QueryShapeFacts`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/validate/pass4_10.rs:137-233`; `glass-lint-core/src/api/compiler/normalize_all.rs:30-64,67-76`

`pass_correlation_evidence` builds `branch_facts` from a same-event `All`
(`pass4_10.rs:165-166`) and runs `validate_correlated_branches`
(`pass4_10.rs:217-233`); `normalize_all_root` then builds the same
`QueryShapeFacts` list again (`normalize_all.rs:35`) and runs the identical
predicate `first_branch_correlates_with_any` (`normalize_all.rs:67-76`) — "first
branch shares a variable with any other branch" — plus the heavier
`find_common_event_var` scan (`normalize_all.rs:85-108`). Because
`compile_queries` runs `validate_query_decl` before `normalize_query_decl`
(`compiler/mod.rs:187-190`), validation has already rejected a multi-branch
`All` with no shared variable (`UncorrelatedConjunction`, `pass4_10.rs:231`), so
the `else` arm of the `map_or_else` (`normalize_all.rs:58-60`,
`UncorrelatedConjunction`) is unreachable in the production path: whenever
normalize runs, `first_branch_correlates_with_any` is necessarily true and only
`UnsupportedRelation` (`normalize_all.rs:52-57`) can fire. The only exerciser of
the normalize-side branch is the direct-normalize test
`tests/normalize/algebra.rs:322-351`, which calls `normalize_query_decl`
without the validating pipeline. The two predicates implement the same
first-branch-overlap test and have no shared owner.

**Recommendation:** Enumerate the correlation decision once. Keep
`UncorrelatedConjunction` owned by `validate_correlated_branches` (the phase
that can actually observe an authored no-shared-variable `All`), drop
`first_branch_correlates_with_any`, and make `normalize_all_root`'s
no-common-event-var arm report `UnsupportedRelation` unconditionally; move the
`algebra.rs:322-351` assertion to the validate boundary. Guardrails: keep the
distinct diagnostics `UncorrelatedConjunction` (no shared variables,
validate-side, `pass4_10.rs:231`) versus `UnsupportedRelation` (correlated but
no single common event variable, `normalize_all.rs:52-57`); keep the
`Require(ReturnedObject | ConstructedObject)` binding-only carve-out in
`find_common_event_var` (`normalize_all.rs:96-102`), which validate does not
express and which cannot be dropped; and leave the reachable
`UncorrelatedConjunction` returns in the merge path (`normalize_all.rs:249,275`)
untouched.

**Fix Applied:** Removed normalization's duplicate first-branch overlap scan and
made its no-common-event-variable fallback unconditionally report
`UnsupportedRelation`. Validation remains the owner of the authored-input
`UncorrelatedConjunction` diagnostic and its focused test; the direct
normalization test was removed accordingly.

#### [ ] READ-005 — Identity/event dimension compatibility and subject classification are re-derived at four sites per query

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/validate/pass1_3.rs:9-35`; `glass-lint-core/src/api/compiler/contradiction.rs:18-34`; `glass-lint-core/src/api/compiler/normalize.rs:57-135,376-390`; `glass-lint-core/src/api/compiler/physical/planner.rs:54-62`

The identity/event dimension rule (`is_valid_identity_event_pair`,
`validate/error.rs:296-301`) is evaluated at four points for a single simple
event: `validate_event_query` (`pass1_3.rs:10`), `check_dimension_contradictions`
during normalization (`contradiction.rs:27-32`, reached from
`normalize_event_from_query` at `normalize.rs:382` for the always-`Direct`
subject), `validate_normalized` → `validate_normalized_root` (`normalize.rs:58`)
→ `classify_subject_relation` (`normalize.rs:108-112`), and again
`classify_subject_relation` in `plan_event` (`planner.rs:58-60`). The same
post-condition invariants — no nested `Any`, dense slots, ascending canonical
groups, valid subject relation — are also re-checked in `validate_normalized`
(`normalize.rs:57-135`) even though the normalizer establishes them by
construction (`normalize_any_root` flattens nested `Any` at `normalize.rs:168-178`,
`alpha_renumber_slots` densifies at `normalized.rs:429-439`,
`CanonicalArgumentConstraints::from_constraints` sorts and dedups at
`normalized.rs:113-153`), and `validate_canonical_constraints`
(`physical/validation.rs:34-72`) re-verifies ascending group order a third time.
Each additional entry point means a future change to the dimension rule must be
kept consistent at four locations.

**Recommendation:** Make `normalize_query_decl` the single sealing boundary for
authored queries: have normalization return the classified `SubjectRelation`
(or reject) so the planner consumes it instead of calling
`classify_subject_relation` again (`planner.rs:58-60`), and keep
`validate_normalized` only for invariants that are not structurally guaranteed.
Guardrails: keep the declared phase fail-closed (`CompilerInvariantDiagnostic`
for internal bugs, `QueryCompileError` for authored input), preserve
`PhysicalPlanValidationError::ImpossibleDimensions` behavior for hand-built
plans (`physical.rs:78-83`, `planner.rs:60`), and keep
`detect_event_contradictions` for the merged same-event path
(`normalize_all.rs:278`), where a merged identity/event pair genuinely needs a
fresh check.

**Fix Applied:** None so far.

### Lifecycle IR

#### [x] READ-003 — `Option<NormalizedLifecycleCompletion>` is a phantom option

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:321-353`; `glass-lint-core/src/api/compiler/normalize.rs:306,120-124`; `glass-lint-core/src/api/compiler/object_flow.rs:142-143`

`NormalizedLifecycle::completion` is `Option<NormalizedLifecycleCompletion>`
(`normalized.rs:326`) but its `None` state is unreachable in production: the
authored `LifecycleQuery` requires completion (`api/rule/query/lifecycle.rs:96`),
`normalize_lifecycle_root` always constructs `Some(...)`
(`normalize.rs:306`), and `validate_normalized` rejects `None` as an internal
invariant ("missing a required stage", `normalize.rs:120-123`). The only other
construction is the `#[cfg(test)]` fixture at `tests/physical_extended.rs:24-39`,
which also passes `Some(...)`. The option forces a
`map_or_else(|| (Vec::new(), CompletionMode::AnySink), ...)` default branch in
`object_flow::from_normalized_lifecycle` (`object_flow.rs:142-143`) that can
never fire, a `completion()?` propagation in the `#[cfg(test)]` reference
lowering (`reference.rs:117`), and a `completion().is_none()` guard in
`validate_normalized` (`normalize.rs:120`) — plumbing that lets a caller thread
a "completion absent" state the invariant says is invalid.

**Recommendation:** Make `completion: NormalizedLifecycleCompletion` non-optional
on `NormalizedLifecycle`, enforcing the invariant at the constructor
`NormalizedLifecycle::new` (`normalized.rs:329-340`) and its only producers
(`normalize_lifecycle_root` at `normalize.rs:330-332`; the test fixture at
`tests/physical_extended.rs:24-39`); delete the `map_or_else` default in
`object_flow.rs:142-143`, the `completion()?` in `reference.rs:117`, and the
`completion().is_none()` check in `validate_normalized` (`normalize.rs:120`).
Guardrails: keep `condition` as the single genuinely optional stage
(`ObjectFlow` must still handle a missing condition as `AnyRequired`,
`object_flow.rs:123-124`), and keep the fail-closed seal so test-constructed
lifecycles cannot bypass the invariant. The `Configuration` versus empty-`AnySink`
distinction stays expressible through the enum variants (`normalized.rs:314-319`),
which chunk 21 READ-001 relies on; only the unreachable `None` state is removed.

**Fix Applied:** Made `NormalizedLifecycle::completion` a required
`NormalizedLifecycleCompletion`, removing the unreachable `None` plumbing from
normalization validation, object-flow lowering, and reference lowering. The
condition stage remains genuinely optional, and the existing normalized test
fixture now constructs the required completion directly.

### Limits

#### [x] READ-004 — Per-rule physical-root bound has two unrelated constants and three enforcement sites

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/limits.rs:1`; `glass-lint-core/src/api/rule/query/limits.rs:1`; `glass-lint-core/src/api/compiler/normalize.rs:173-185`; `glass-lint-core/src/api/compiler/physical.rs:282-288,411-422`

The final physical-root budget per rule is stated twice with equal values and
no cross-reference: `compiler::limits::MAX_PHYSICAL_ROOTS_PER_RULE = 256` and
`query::limits::MAX_QUERY_ROOTS_PER_RULE = 256` (governing number of queries
per rule at `api/rule/mod.rs:240`). The physical bound is then enforced in
three places: `normalize_any_root` checks the pre-dedup flattened branch count
of one `Any` (`normalize.rs:173-185`), `RootBudget::reserve` caps the
accumulated root count across a rule's queries (`physical.rs:282-288`, budget
shared across queries in `compile_queries` at `mod.rs:184`), and
`validate_root_set` re-checks the sealed set (`physical.rs:411-422`). Only
`RootBudget`/sealing implements the true per-rule bound on the
post-optimization root count; the `normalize.rs` check enforces the same limit
on a different quantity (pre-dedup branch count), on a different error variant
(`UnboundedQuery` vs `TooManyRoots`), and can even reject a query whose branches
would dedup below the limit (e.g. nested-`Any` flattening of many identical
branches, `normalize.rs:168-191`).

**Recommendation:** Treat `RootBudget` (`physical.rs:282-288`) plus
`validate_root_set` (`physical.rs:411-422`) as the sole owner of the per-rule
physical-root bound; drop the `normalize.rs` pre-dedup early abort
(`normalize.rs:173-185`), letting sealing enforce the bound — or, if a fail-fast
guard is wanted, tie it to the same single constant and document it as a
strictly conservative pre-dedup approximation. Keep the two `256` constants
independent: they bound different quantities (queries per rule vs physical
roots per rule), so fusing them would silently couple the compile-time bound to
the query-count bound; instead make the relationship explicit with a
cross-reference comment (each query yields at least one root, so the physical
bound must be ≥ the query bound) or rename the query-side constant to name its
quantity (e.g. `MAX_QUERIES_PER_RULE`). Guardrails: preserve the distinct
diagnostics `UnboundedQuery` (authored shape, `pass4_10.rs:62-67,78-97`) versus
`TooManyRoots` (sealing, `physical.rs:284,416`), keep the budget shared across
all queries of one rule in `compile_queries`, and keep the `Any`-flatten dedup
behavior exact.

**Fix Applied:** Removed the normalization-time pre-dedup root-count abort.
`RootBudget` remains the single production admission point for the aggregate
physical-root bound, and sealed plans retain the final `validate_root_set`
check. The query-count and physical-root limits remain independent because
they bound different quantities.

### Documentation and newtypes

#### [x] READ-006 — Declaration and IR emptiness policies are two implementations of one rule

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/validate/error.rs:374-396`; `glass-lint-core/src/api/compiler/mod.rs:105-127`

`is_identity_empty` (on the authored `IdentitySpec`, `validate/error.rs:380-396`)
and `IdentityConstraint::is_empty` (on the compiler IR,
`compiler/mod.rs:110-126`) implement the same trimmed-whitespace emptiness
policy with matching doc comments that only warn "the two cannot diverge"
(`validate/error.rs:376-379`, `compiler/mod.rs:106-109`). Maintaining a
documented coupling by copying is a latent divergence risk already recognized
by the comments themselves, and the policy will grow with each new identity
variant.

**Ownership — reported by chunk 21 (READ-002).** This duplication is fully
owned by `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_21.md` READ-002, which cites the
same two locations (`compiler/mod.rs:105-127`, `validate/error.rs:374-396`) and
recommends extracting narrow component helpers (`name_empty`,
`module_export_empty`) behind one compiler-owned location plus a parity test
asserting `IdentityConstraint::from(spec).is_empty() == is_identity_empty(spec)`
for every variant. Recorded here only so this chunk's compiler-surface coverage
is complete; do not double-apply the fix.

**Fix Applied:** Resolved by the shared implementation in commit `fd7de45a`
(`fix chunk 21 read 002`), which centralizes the identity emptiness policy and
adds parity coverage. No duplicate implementation is introduced in this
cross-reference.

### Compiler boundaries

#### [x] READ-007 — `CompiledMatcherPlan` is a one-field façade over `PhysicalPlan`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:66-70,223-241`; `glass-lint-core/src/api/compiler/physical.rs:266-344`

`CompiledMatcherPlan` stores only `physical_plan: PhysicalPlan` (`mod.rs:68-70`)
and forwards `physical_roots()`, `requirements()`, and `plan_explanation()`
verbatim (`mod.rs:224-236`); its only added logic is the `compile` entry point
(`mod.rs:238-241`) and the naming. Both types are `pub(crate)` in a
`pub(crate) mod compiler` (`api/mod.rs:8`), so the wrapper is not providing an
externally controlled visibility boundary — every consumer in `analysis/*` and
`lint/*` reaches it through the same crate-private path (`projection.rs:216,229`
via `CompiledRuleSelection`, `matching/query/mod.rs:23`). `PhysicalPlan` itself
is consumed only inside `api/compiler` (`planner.rs`, `validation.rs`,
`reference.rs` [`#[cfg(test)]`], and the compiler tests).

**Recommendation:** Collapse the two types into one executable-plan type: keep
the `CompiledMatcherPlan` name and its `compile` method as the crate-internal
entry, fold `PhysicalPlan`'s `from_planned_roots`/`roots`/`requirements` onto
it, and move `optimize_roots`/`seal_planned_roots` (`mod.rs:199-204`) and
`validate_root_set` (`physical.rs:411-422`) in as its constructors. Guardrails:
preserve the single sealing boundary in `from_planned_roots`
(`physical.rs:296-301`), the `#[cfg(test)]` test-only constructors and printers
(`try_new`, `new`, `summary`, `explain`, `physical.rs:316-336,347-408`), and the
plan exposure swallowed by projection (`ProjectionPlan::from_selection`,
`projection.rs:211-237`). The alternative of dropping `CompiledMatcherPlan` and
exposing `PhysicalPlan` directly to consumers is off the table: the chunk
invariant that `CompiledMatcherPlan` is the only plan type consumed by
`analysis`/`lint` is the intended boundary and must be preserved.

**Fix Applied:** Collapsed the physical roots and derived requirements into the
existing `CompiledMatcherPlan`. Its production sealing constructor and
test-only validation/printer helpers now live on that type, while the
`PhysicalPlan` wrapper and all internal references were removed. The single
production sealing boundary and the plan exposure used by projection remain
unchanged.

#### [ ] READ-008 — "Which rules ran" bookkeeping and its index-bound validation are duplicated across lint selection, projection, and evidence

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/selection.rs:255-268,316-356`; `glass-lint-core/src/lint/linter.rs:93-104`; `glass-lint-core/src/api/compiler/rule.rs:32-57`; `glass-lint-core/src/api/classification.rs:241-250`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:96-98`

The set of enabled rule indexes is carried as `Vec<RuleIndex>` in lint
selection (`selection.rs:257,268`, produced by `evaluate` at
`selection.rs:329-356`) and `LinterSharedConfig::enabled` (`linter.rs:99`),
validated again at session time by `CompiledRuleSelection::new`
(`rule.rs:32-57`, invoked at `analysis/project/model.rs:467`), and then iterated
a third time in `assemble_classification_results` with a tolerant
`records.get(index) else continue` (`projection.rs:70-72`) that can silently
skip a rule. The same `index_get < capacity` bound is then re-derived on every
write by `RuleEvidenceTable::items_mut` (`classification.rs:241-250`) and again
by `ModuleEvidence::rule_mut` (`analysis/flow/cross/evidence.rs:96-98`). The
capacity invariant and the "rules that ran" list have no single owner, so each
future caller must remember and re-prove the same range/sortedness.

**Recommendation:** Make `CompiledRuleSelection` the single validated owner of
the selected rule index slice (constructed once per project boundary,
`model.rs:467`) and have both `assemble_classification_results`
(`projection.rs:58-89`) and every `ModuleEvidence` construction take it or its
validated capacity; delete the redundant `records.get(...) continue` fallback
(`projection.rs:70-72`) and the per-write capacity re-checks in
`items_mut`/`rule_mut` once selection flows through one owner. Guardrails:
preserve the fail-closed `RuleEvidenceError::RuleOutOfRange` and
`CapacityMismatch` surfaces (`classification.rs:44-47`; AGENTS.md: model
expected errors, do not panic), keep the bounded, deterministic catalog order,
and do not collapse `RuleIndex`'s opacity (`classification.rs:16-28`).

**Fix Applied:** None so far.

## Systemic Themes

- **Phase re-validation:** `compile_queries` (validate → normalize → seal) plus
  a post-normalization audit plus physical-plan verification re-derive the same
  invariants (empty identity, identity/event compatibility, canonical
  constraint order, per-rule root bound) at each boundary. Fail-closed
  layering is correct, but the current structure gives each phase its own error
  vocabulary (`QueryCompileError`, `PhysicalPlanValidationError`,
  `MatcherBuildError`) for the same rule, which is the root cause of
  READ-002/004/005. READ-003 is the lifecycle slice of the same pattern: the
  IR carries an unreachable `None` state purely so each phase can re-assert it.
- **Parallel any/all discriminators:** the lifecycle any/all operator is
  carried as `LifecycleConditionKind`/`LifecycleCompletionKind` (authored),
  `NormalizedLifecycleCondition`/`NormalizedLifecycleCompletion` (IR), and
  `RequirementMode`/`CompletionMode` (compiled flow) with near-identical
  `AnyOf`/`AllOf` and `AnySink`/`AllSinks` payloads; the normalized copies are
  structural copies whose main transformation is canonicalizing constraints.
  This is a conversion boundary, not yet a duplication finding, but the IR
  should not gain authored vocabulary drift.
- **Classification vs compiler limit vocabulary:** `api::classification`
  carries `RuleEvidenceCapacity`/`RuleEvidenceError` while `api::compiler`
  carries query/physical root limits; the two concern different quantities
  (catalog record count vs roots/queries per rule), but both are re-derived at
  consumers (READ-008, READ-004) and neither is documented as an invariant
  owner.
- `#![allow(clippy::redundant_pub_crate)]` (`compiler/mod.rs:27`) plus
  explicit `pub(crate)` on every sub-module and re-export is a consistent but
  redundant style; harmless, not reported as a finding.

## Open Questions — Resolved

1. **The three-file normalization pipeline is proportionate; no merge.**
   `normalize.rs` owns single-event/`Any`/lifecycle entry normalization,
   `normalize_all.rs` owns the same-event `All` merge (`SameEventMerge`,
   `normalize_all.rs:137-287`), and `normalized.rs` owns the IR types plus the
   canonicalization helpers. The apparent overlap is not duplicated logic but
   reuse of two shared owners: `CanonicalArgumentConstraints::from_constraints`
   (`normalized.rs:113-153`) is called by both `normalize_event_from_query`
   (`normalize.rs:377`) and `SameEventMerge::into_root` (`normalize_all.rs:265`),
   and `detect_event_contradictions` (`contradiction.rs:7-16`) is called at
   `normalize.rs:382` and `normalize_all.rs:278` — but each call validates a
   distinct construction (a single always-`Direct` event vs a merged subject).
   Merging `normalize_all` into `normalize` would grow `normalize.rs` from 396
   to ~700 lines and mix the merge state machine into the entry surface, while
   READ-002 already shrinks `normalize_all`'s correlation code. Keep the split.
2. **`validate_normalized` is a production seal, not test-only.** It runs at
   `normalize.rs:46` inside `normalize_query_decl`, which `compile_queries`
   invokes on the production path (`mod.rs:187-190`), so it cannot be
   `#[cfg(test)]`-gated. The `#[cfg(test)]` items in `physical.rs` are only the
   extra audit (`validate_physical_plan`, `physical/validation.rs:12-23`) and
   the test-only constructors; the production sealing analog is
   `validate_root_set` (`physical.rs:411-422`), called from `from_planned_roots`
   (`physical.rs:296-301`). However, most of `validate_normalized`'s checks
   re-verify invariants the normalizer establishes by construction — that
   redundancy is READ-005's subject, whose fix trims the audit to non-structural
   invariants while keeping it a production fail-closed seal.
3. **The description clone is cheap enough; keep it.** The clone
   (`projection.rs:81`) runs once per (module, selected rule with at least one
   match), so it is bounded by matches — at most modules × selected rules — and
   the strings are short, independent of the evidence limit. Borrowing would
   force `MatchedCapability`/`ClassificationResult` (public types,
   `api/mod.rs:7`) to carry a lifetime tied to the `CompiledRuleRecord` slice
   (`projection.rs:60`), coupling report types to the catalog/session lifetime
   for negligible savings. Revisit only if descriptions grow or the report path
   is reworked.
4. **Merge-time checks cannot be unified with `detect_event_contradictions`
   without losing first-wins determinism.** The merge-time checks
   (`merge_event_kind` `normalize_all.rs:208-216`, `merge_identity`
   `normalize_all.rs:218-226`, `merge_subject` `normalize_all.rs:228-241`)
   compare each incoming authored candidate against the first-seen value and
   reject on inequality — they decide which branch's value is retained, so they
   must run while merging. `detect_event_contradictions` (`contradiction.rs:7-16`)
   instead validates the semantic validity of the already-retained combination
   and operates on canonical form. Running detection in place of the equality
   checks would either lose the first-wins retention contract or require
   detection to also decide retention, and the vocabularies differ (authored
   `EventSpec`/`IdentitySpec` equality vs canonical-constraint validity). Keep
   them separate; READ-002 removes only the correlation re-derivation, which is
   orthogonal to this question.

## Coverage

Read and cited files:

- `glass-lint-core/src/api/classification.rs` (all), `classification/result.rs`
  (all), `classification/tests.rs` (all)
- `glass-lint-core/src/api/compiler/mod.rs`, `catalog.rs`, `contradiction.rs`,
  `error.rs`, `limits.rs`, `normalize.rs`, `normalize_all.rs`, `normalized.rs`,
  `object_flow.rs`, `requirements.rs`, `rule.rs`, `physical.rs`,
  `physical/planner.rs`, `physical/validation.rs`, `validate/mod.rs`,
  `validate/error.rs`, `validate/pass1_3.rs`, `validate/pass4_10.rs`,
  `reference.rs`, `tests.rs`, `tests/normalize/algebra.rs`,
  `tests/physical_extended.rs`
- `glass-lint-core/src/api/rule/mod.rs`, `api/rule/query/mod.rs`,
  `api/rule/query/{limits,lifecycle,expression}.rs`
- Callers traced: `analysis/project/projection.rs`, `analysis/project/model.rs`,
  `analysis/flow/cross/evidence.rs`, `analysis/flow/cross/mod.rs`,
  `analysis/flow/projector/{mod,driver,state}.rs`,
  `analysis/matching/{mod,query/mod,arguments/mod,evidence}.rs`,
  `lint/linter.rs`, `lint/catalog.rs`, `lint/selection.rs`,
  `lint/report/{mod,evidence}.rs`, `project/session/mod.rs`, `lib.rs`,
  `tests/integration/public_surface.rs`
- References read: `AGENTS.md`, workspace `ARCHITECTURE.md`,
  `glass-lint-core/ARCHITECTURE.md`, `TESTING.md`, skill at
  `/home/lemon/.codex/skills/rust-readability-audit/SKILL.md`, and
  `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_21.md` (cross-ownership of READ-006)

`git status` confirms no source changes. This session edits only
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_20.md`; the other
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_{01..17,21..25}.md` files belong to
parallel sessions and were not touched here.
