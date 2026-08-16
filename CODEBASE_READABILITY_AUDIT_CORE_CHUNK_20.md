# Codebase Readability Audit — Chunk 20: Classification and Compilation

## Summary

Read-only audit of `glass-lint-core/src/api/classification*.rs` and
`glass-lint-core/src/api/compiler/**` ("Classification and compilation"). The
pipeline is well documented and the compile boundary is correctly sealed
(`CompiledMatcherPlan` is the only plan type consumed by `analysis` and `lint`;
normalized IR is `pub(crate)` in a `pub(crate) mod compiler`). The findings
below center on repeated re-derivation of the same decisions across the
validate → normalize → plan phases, duplicated correlation/limit bookkeeping
between `validate` and `normalize_all`, a phantom `Option` in the lifecycle IR,
an unprincipled split of classification accessor methods across files, and two
crate-private wrapper/owner questions.

No source was modified; the only file created is this audit.

## Findings

### Classification evidence API

#### [ ] READ-001 — Classification accessors for the same type are split between `classification.rs` and `classification/result.rs` with no stated criterion

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/api/classification.rs:90-215`; `glass-lint-core/src/api/classification/result.rs:22-47`

Documentation-method accessors for one type are separated across two modules
with no rule stated in either. On `MatchedCapability`, `rule_index()` lives in
`classification.rs:105-107` while `label()`, `severity()`, and `evidence()`
live in `result.rs:22-37`. On `ClassificationEvidence`, `count()`,
`is_truncated()`, `certainty()`, and `occurrences()` live in
`classification.rs:179-193` while `kind()` and `symbol()` live in
`result.rs:39-47`. `ClassificationEvidenceOccurrence` accessors all stay in
`classification.rs`. The split is not visibility-driven (`result.rs` is not the
"public accessor" file - several `pub` accessors stay in `classification.rs`),
so a reader must hold both files to enumerate one type's surface; `result.rs`
is 49 lines and exists mainly to host the `ClassificationResult` re-export.

**Recommendation:** Consolidate every accessor onto its owning type in
`classification.rs`, leaving `ClassificationResult` and its two
`push_capability`/`capabilities` methods (the only type actually defined in
`result.rs`) in the submodule, or delete `classification/` and define
`ClassificationResult` in `classification.rs` directly. Guardrails: keep the
pub/`pub(crate)` visibility split exactly as it is (the report serialization
path in `lint/report/evidence.rs` depends on it), and leave the `MatchKind`
re-export untouched.

**Fix Applied:** None so far.

### Normalize/validate correlation

#### [ ] READ-002 — Same-event correlation is computed twice on identical `QueryShapeFacts`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/validate/pass4_10.rs:137-233`; `glass-lint-core/src/api/compiler/normalize_all.rs:30-64,67-76`

`pass_correlation_evidence` builds `branch_facts` from a same-event `All`
(`pass4_10.rs:165-166`) and runs `validate_correlated_branches`
(`pass4_10.rs:217-233`); `normalize_all_root` then builds the same
`QueryShapeFacts` list again (`normalize_all.rs:35`) and runs the identical
predicate `first_branch_correlates_with_any` (`normalize_all.rs:67-76`): "first
branch shares a variable with any other branch". Because `compile_queries`
runs `validate_query_decl` before `normalize_query_decl`
(`compiler/mod.rs:187-190`), validation has already rejected a multi-branch
`All` with no shared variable, so the `else` arm of the `map_or_else`
(`normalize_all.rs:58-60`, `UncorrelatedConjunction`) is unreachable in the
production path and the correlate-vs-merge decision restates a decision
validation already made. The two predicates are byte-for-byte the same test and
have no shared owner.

**Recommendation:** Enumerate the correlation decision once in `validate`
(or a shared helper on `QueryShapeFacts`) and have `normalize_all` consume the
already-validated outcome - either pass the vetted `find_common_event_var`
result through, or drop `first_branch_correlates_with_any` and make
`normalize_all_root` report only `UnsupportedRelation` (the only reachable
error after validation). Guardrails: keep the distinct diagnostics
`UncorrelatedConjunction` (no shared variables) versus `UnsupportedRelation`
(correlated but no single common event variable) at the validate boundary, and
keep the `Require(ReturnedObject | ConstructedObject)` binding-only carve-out
in `find_common_event_var` (`normalize_all.rs:96-102`), which `validate` does
not express.

**Fix Applied:** None so far.

#### [ ] READ-005 — Identity/event dimension compatibility and subject classification are re-derived at four sites per query

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/validate/pass1_3.rs:9-34`; `glass-lint-core/src/api/compiler/contradiction.rs:7-34`; `glass-lint-core/src/api/compiler/normalize.rs:57-135,376-390`; `glass-lint-core/src/api/compiler/physical/planner.rs:54-62`

The identity/event dimension rule is evaluated at four points for a single
simple event: `validate_event_query` (`pass1_3.rs:10`),
`check_dimension_contradictions` during normalization
(`contradiction.rs:27-32`, reached from `normalize_event_from_query` at
`normalize.rs:382` for the always-`Direct` subject),
`validate_normalized` → `classify_subject_relation` (`normalize.rs:108-112`,
168 with `validate_normalized_root`), and again `classify_subject_relation` in
`planner.rs:58-60`. The same post-condition invariants - no nested `Any`,
dense slots, ascending canonical groups, valid subject relation - are also
re-checked in `validate_normalized` (`normalize.rs:57-135`) even though the
normalizer establishes them by construction (`normalize_any_root` flattens,
`alpha_renumber_slots` densifies, `CanonicalArgumentConstraints::from_constraints`
sorts), and `validate_canonical_constraints` (`physical/validation.rs:34-72`)
re-verifies group order a third time. Each additional entry point means a
future change to the dimension rule must be kept consistent at four locations.

**Recommendation:** Make `normalize_query_decl` the single sealing boundary for
authored queries: have normalization return the classified
`SubjectRelation` (or reject) so the planner consumes it instead of calling
`classify_subject_relation` again (`planner.rs:58-60`), and keep
`validate_normalized` only for invariants that are not structurally guaranteed.
Guardrails: keep the declared phase fail-closed (`CompilerInvariantDiagnostic`
for internal bugs, `QueryCompileError` for authored input), preserve
`PhysicalPlanValidationError::ImpossibleDimensions` behavior for hand-built
plans, and keep `detect_event_contradictions` for the merged same-event path
(`normalize_all.rs:278`), where a merged identity/event pair genuinely needs a
fresh check.

**Fix Applied:** None so far.

### Lifecycle IR

#### [ ] READ-003 — `Option<NormalizedLifecycleCompletion>` is a phantom option

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:321-353`; `glass-lint-core/src/api/compiler/normalize.rs:306,120-124`; `glass-lint-core/src/api/compiler/object_flow.rs:142-143`

`NormalizedLifecycle::completion` is `Option<NormalizedLifecycleCompletion>`
but its `None` state is unreachable in production: the authored
`LifecycleQuery` requires completion (`api/rule/query/lifecycle.rs:96`),
`normalize_lifecycle_root` always constructs `Some(...)`
(`normalize.rs:306`), and `validate_normalized` rejects `None` as an internal
invariant ("missing a required stage", `normalize.rs:120-124`). The option
forces a `map_or_else(|| (Vec::new(), CompletionMode::AnySink), ...)` default
branch in `object_flow::from_normalized_lifecycle` (`object_flow.rs:142-143`)
that can never fire, and its `Option`/`get` plumbing lets a caller thread a
"completion absent" state that the invariant says is invalid.

**Recommendation:** Make `completion: NormalizedLifecycleCompletion`
non-optional on `NormalizedLifecycle`, enforcing the invariant at the
constructor (`NormalizedLifecycle::new`) and the only successor
(`normalize_lifecycle_root`); delete the `map_or_else` default in
`object_flow.rs` and the `completion().is_none()` check in `validate_normalized`.
Guardrails: keep `condition` as the single genuinely optional stage
(`ObjectFlow` must still handle a missing condition as `AnyRequired`), and keep
the fail-closed seal so unsupported test-constructed lifecycles cannot bypass
the invariant.

**Fix Applied:** None so far.

### Limits

#### [ ] READ-004 — Per-rule physical-root bound has two unrelated constants and three enforcement sites

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/limits.rs:1`; `glass-lint-core/src/api/rule/query/limits.rs:1`; `glass-lint-core/src/api/compiler/normalize.rs:173,181`; `glass-lint-core/src/api/compiler/physical.rs:282-288,411-422`

The final physical-root budget per rule is stated twice with equal values and
no cross-reference: `compiler::limits::MAX_PHYSICAL_ROOTS_PER_RULE = 256` and
`query::limits::MAX_QUERY_ROOTS_PER_RULE = 256` (governing number of queries
per rule at `api/rule/mod.rs:240`). The physical bound is then enforced in
three places: `normalize_any_root` checks the pre-dedup branch count of one
`Any` (`normalize.rs:173-185`), `RootBudget::reserve` caps the accumulated root
count across a rule's queries (`physical.rs:282-288`), and `validate_root_set`
re-checks the sealed set (`physical.rs:411-422`). Only `RootBudget`/sealing
implements the true per-rule bound; the `normalize.rs` check is a redundant
early abort that can even reject a query whose branches would dedup below the
limit, on a different error variant (`UnboundedQuery` vs `TooManyRoots`).

**Recommendation:** Treat `RootBudget` + `validate_root_set` as the sole owner
of the per-rule physical-root bound, make the `normalize.rs` early check either
reuse the same single constant definition with a comment tying it to the
sealing limit or drop it, and consolidate the two `256` constants by having
`compiler/limits.rs` reference the query-limit constant (or delete it
entirely). Guardrails: preserve the distinct diagnostics `UnboundedQuery`
(authored shape) versus `TooManyRoots` (sealing), keep the budget
shared across all queries of one rule in `compile_queries`, and keep the
`Any`-flatten dedup behavior exact.

**Fix Applied:** None so far.

### Documentation and newtypes

#### [ ] READ-006 — Declaration and IR emptiness policies are two implementations of one rule

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/validate/error.rs:374-395`; `glass-lint-core/src/api/compiler/mod.rs:105-127`

`is_identity_empty` (on the authored `IdentitySpec`) and
`IdentityConstraint::is_empty` (on the compiler IR) implement the same
trimmed-whitespace emptiness policy with matching doc comments that only warn
"the two cannot diverge" (`validate/error.rs:377-379`,
`compiler/mod.rs:106-109`). Maintaining a documented coupling by copying is a
latent divergence risk already recognized by the comments themselves, and the
policy will grow with each new identity variant.

**Recommendation:** Extract one emptiness predicate over the shared dimension
of the two enums (for example a private helper implemented once and called by
both, or a small `IdentityEmptiness` trait whose contract documents the trim
policy and the `Rooted`/`PrivateNetworkAddress` exceptions) and delete the
second `match`. Guardrails: the emtpty policy must keep `Rooted` paths and
`PrivateNetworkAddress` never-empty, apply trimmed whitespace to every other
variant, and remain fail-closed (`EmptySubjectIdentity` /
`ImpossibleDimensions` still fire through the resulting boolean).

**Fix Applied:** None so far.

### Compiler boundaries

#### [ ] READ-007 — `CompiledMatcherPlan` is a one-field façade over `PhysicalPlan`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:66-70,223-241`; `glass-lint-core/src/api/compiler/physical.rs:266-344`

`CompiledMatcherPlan` stores only `physical_plan: PhysicalPlan` and forwards
`physical_roots()`, `requirements()`, and `plan_explanation()` verbatim
(`compiler/mod.rs:224-236`); its only added logic is the `compile` entry point
and the naming. Both types are `pub(crate)` in a `pub(crate) mod compiler`, so
the wrapper is not providing an externally controlled visibility boundary -
every consumer in `analysis/*` and `lint/*` reaches it through the same
crate-private path.

**Recommendation:** Collapse the two types into one executable-plan type (keep
the `CompiledMatcherPlan` name and its `compile` method as the crate-internal
entry, folding `PhysicalPlan`'s `from_planned_roots`/`roots`/`requirements`
onto it and moving `optimize_roots`/`seal_planned_roots` as its constructors),
or drop `CompiledMatcherPlan` and have consumers use `PhysicalPlan` directly if
the naming boundary is judged not to matter. Guardrails: preserve the
single sealing boundary in `from_planned_roots`, the `#[cfg(test)]`
test-only constructors (`try_new`, `new`, `summary`, `explain`), and the plan
exposure swallowed by projection (`ProjectionPlan::from_selection`).

**Fix Applied:** None so far.

#### [ ] READ-008 — "Which rules ran" bookkeeping and its index-bound validation are duplicated across lint selection, projection, and evidence

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/selection.rs:257-268,316-348`; `glass-lint-core/src/lint/linter.rs:97-104`; `glass-lint-core/src/api/compiler/rule.rs:26-76`; `glass-lint-core/src/api/classification.rs:237-308`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:71-98`

The set of enabled rule indexes is carried as `Vec<RuleIndex>` in lint
selection and `LinterSharedConfig::enabled`, validated again at session time by
`CompiledRuleSelection::new` (`rule.rs:36-58`), and then iterated a third time
in `assemble_classification_results` with a tolerant `records.get(index) else
continue` (`projection.rs:68-85`) that can silently skip a rule. The same
`index_get < capacity` bound is then re-derived on every write by
`RuleEvidenceTable::items_mut` (`classification.rs:241-250`) and again by
`ModuleEvidence::rule_mut` (`analysis/flow/cross/evidence.rs:96-98`). The
capacity invariant and the "rules that ran" list have no single owner, so each
future caller must remember and re-prove the same range/sortedness.

**Recommendation:** Make `CompiledRuleSelection` the single validated owner of
the selected rule index slice (constructed once per project boundary) and have
both `assemble_classification_results` and every `ModuleEvidence`
construction take it or its validated capacity; delete the redundant
`records.get(...) continue` fallback and the per-slice re-validation in lint
once selection flows through one owner. Guardrails: preserve the fail-closed
`RuleEvidenceError::RuleOutOfRange` and `CapacityMismatch` surfaces (AGENTS.md:
model expected errors, do not panic), keep the bounded, deterministic catalog
order, and do not collapse `RuleIndex`'s opacity.

**Fix Applied:** None so far.

## Systemic Themes

- **Phase re-validation:** `compile_queries` (validate → normalize → seal) plus
  a post-normalization audit plus physical-plan verification re-derive the same
  invariants (empty identity, identity/event compatibility, canonical
  constraint order, per-rule root bound) at each boundary. Fail-closed
  layering is correct, but the current structure gives each phase its own error
  vocabulary (`QueryCompileError`, `PhysicalPlanValidationError`,
  `MatcherBuildError`) for the same rule, which is the root cause of
  READ-002/004/005.
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
  (catalog record count vs roots per query), but both are re-derived at
  consumers and neither is documented as an invariant owner.
- `#![allow(clippy::redundant_pub_crate)]` (`compiler/mod.rs:27`) plus
  explicit `pub(crate)` on every sub-module and re-export is a consistent but
  redundant style; harmless, not reported as a finding.

## Open Questions

- Is the three-file normalization pipeline (`normalize.rs` +
  `normalize_all.rs` + `normalized.rs`) proportionate? `normalize.rs:376-390`
  and `normalize_all.rs:170-175` both lower `EventQuery`→`NormalizedEvent` and
  both call `CanonicalArgumentConstraints::from_constraints` and
  `detect_event_contradictions`; merging `normalize_all` into `normalize` would
  remove a file but grow a function. No finding without a stronger criterion.
- Is the post-normalization `validate_normalized` audit intended as a
  test-only compiler-invariant check or a production seal? If the former, it
  could be `#[cfg(test)]`-gated like the similar codepaths in `physical.rs`.
- Is the eased clone of `record.description` per module per rule in
  `assemble_classification_results` (`projection.rs:81`) cheap enough, or
  should `MatchedCapability` borrow from the `CompiledRuleRecord` to avoid
  per-module string copies (bounded by evidence limit, so currently fine)?
- `SameEventMerge` (`normalize_all.rs:137-287`) and `contradiction.rs` both
  produce `ContradictoryPredicate`/`ContradictionKind` for merged
  same-variable facts; can the merge-time checks be unified with
  `detect_event_contradictions` after READ-002 resolves the correlation
  decision, without losing first-wins determinism?

## Coverage

Read and cited files:

- `glass-lint-core/src/api/classification.rs` (all), `classification/result.rs`
  (all), `classification/tests.rs` (all)
- `glass-lint-core/src/api/compiler/mod.rs`, `catalog.rs`, `contradiction.rs`,
  `error.rs`, `limits.rs`, `normalize.rs`, `normalize_all.rs`, `normalized.rs`,
  `object_flow.rs`, `requirements.rs`, `rule.rs`, `physical.rs`,
  `physical/planner.rs`, `physical/validation.rs`, `validate/mod.rs`,
  `validate/error.rs`, `validate/pass1_3.rs`, `validate/pass4_10.rs`,
  `reference.rs`, `tests.rs`
- Callers traced: `analysis/project/projection.rs`, `analysis/project/model.rs`,
  `analysis/flow/cross/evidence.rs`, `analysis/flow/cross/mod.rs`,
  `analysis/flow/projector/{mod,driver,state}.rs`,
  `analysis/matching/{mod,arguments/mod,evidence}.rs`, `lint/linter.rs`,
  `lint/catalog.rs`, `lint/selection.rs`, `lint/report/{mod,evidence}.rs`,
  `api/rule/mod.rs`, `api/rule/query/{limits,lifecycle}.rs`,
  `project/session/mod.rs`, `lib.rs`, `tests/integration/public_surface.rs`
- References read: `AGENTS.md`, workspace `ARCHITECTURE.md`,
  `glass-lint-core/ARCHITECTURE.md`, skill at
  `/home/lemon/.codex/skills/rust-readability-audit/SKILL.md`

`git status` confirms no source changes; only
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_20.md` was created. The existing
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_{01..16}.md` files from parallel
sessions were left untouched.