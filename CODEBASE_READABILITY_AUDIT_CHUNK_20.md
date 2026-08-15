# Codebase Readability Audit — glass-lint-core Chunk 20: Classification and compilation

## Summary

Chunk 20 covers `api::classification` (classification, classification/result) and the
compiler lowering half of `api::compiler` (mod.rs, catalog, contradiction, error,
limits, normalize, normalize_all, normalized). The chunk owns two responsibilities:
(1) the serializable capability/evidence model (`MatchedCapability`,
`ClassificationEvidence`, `RuleEvidenceTable`, `MatchKind`, `ClassificationResult`)
that the analysis layers accumulate into and the report layer renders; and (2) the
lowering of validated rule declarations into an immutable normalized IR
(`NormalizedQuery`) and the executable `CompiledMatcherPlan`, including contradiction
detection and canonical-form construction.

Overall the code is disciplined: invariants are documented on the canonical
constraint type, bounds are enforced at multiple stages, and the normalized/lowering
separation is coherent. The main readability costs cluster in three systemic patterns:
the normalized IR exposes both raw `pub(crate)` fields and accessor methods (callers
mix both, so the invariant-friendly surface is not the only surface); several layer
boundaries are modeled as parallel types with hand-written conversions and duplicated
checks (`IdentitySpec`/`IdentityConstraint`, `NormalizedEmission`/`EvidenceDescriptor`,
`is_identity_empty`/`IdentityConstraint::is_empty`) whose semantics have already begun
to diverge; and a few constructor/error surfaces carry redundant forwarding or
fail-open shapes (`with_total_count`, `ModuleEvidence::into_evidence`).

## Findings

### [Classification: evidence construction]

#### [x] READ-001 — `ClassificationEvidence` exposes two constructors where one private `from_parts` plus a redundant forwarding wrapper

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/classification.rs:126-185`

`from_parts` (private, lines 126-145) is the only real constructor; `with_total_count`
(pub(crate), lines 176-185) is an identical-signature pure forward to it, and
`from_occurrences`/`from_occurrence` are the convenience wrappers. `with_total_count`
is used only by `analysis/matching/evidence.rs:95,125` and the unit test
`api/classification/tests.rs:93`, so the module carries four constructors with an
inconsistent fallibility shape (one infallible, three returning `Option`) when three
would do. A reader must check all four to learn that the only invariant is
`total_count >= occurrences.len()` with `count` saturated to `u32::MAX`.

**Recommendation:** Delete `with_total_count` and promote `from_parts` to
`pub(crate)` (or rename `from_parts` to `with_total_count`), keeping the
`total_count < occurrences.len() -> None` guard and the `u32::MAX` saturation
unchanged. Guardrail: do not merge `from_occurrence` (infallible) with the fallible
constructors; its non-`Option` shape is the deliberate single-occurrence guarantee.

**Fix Applied:** Deleted the redundant `with_total_count` forwarding wrapper and promoted `from_parts` to `pub(crate)` as the single fallible total-count constructor; updated the two `analysis/matching/evidence.rs` call sites and the capacity-guard unit test to call `from_parts` directly. Guard and `u32::MAX` saturation unchanged; `from_occurrence` keeps its infallible single-occurrence shape.

### [Classification: evidence accumulation boundary]

#### [x] READ-002 — `RuleEvidenceTable` callers must handle structurally-impossible capacity errors; `ModuleEvidence::into_evidence` fails open

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Other
- **Location:** `glass-lint-core/src/api/classification.rs:249-315`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:136-147`

`RuleEvidenceTable::record`, `replace`, `mark_event_truncated`, and
`merge_equal_capacity` return `Result<(), RuleEvidenceError>`, but every production
caller already guarantees the rule index is in range, so the errors are unreachable
and get handled by rollback or silence: `FlowEvidence::record_if_admitted`
(`analysis/flow/projector/state.rs:153`) rolls back on `.is_err()`,
`FlowEvidence::mark_truncated` (`state.rs:193`) discards with `let _ =`, and
`ModuleEvidence::into_evidence` (`analysis/flow/cross/evidence.rs:136-147`) returns
the partial table immediately on `.is_err()`. The `into_evidence` branch is
particularly misleading: because `replace` cannot fail here (capacity was already
checked by `rule_mut` at `cross/evidence.rs:77`), the early `return evidence` would
silently drop the remaining rules if the invariant ever broke — a fail-open path on a
path that must stay fail-closed.

**Recommendation:** Make `ModuleEvidence::into_evidence` infallible
(`expect`/`debug_assert` naming the invariant, since `rule_mut` at
`cross/evidence.rs:77` already bounds every stored key below the table capacity), so
a capacity regression fails loudly instead of silently dropping the remaining rules.
Keep the `Result` on `RuleEvidenceTable::record`/`replace`/`mark_event_truncated`/
`merge_equal_capacity`: the capacity guard is a real boundary exercised as
adversarial negatives in `api/classification/tests.rs` (lines 51-77), and the
in-flow callers' rollback/discard is the correct handling of an unreachable-but-
modeled error. Guardrail: preserve the two-lifecycle split (in-flow accumulation
with `nonmatching` keys vs. final report storage) — do not collapse `ModuleEvidence`
into `RuleEvidenceTable`.

**Fix Applied:** Made `ModuleEvidence::into_evidence` infallible by replacing the fail-open `is_err()` early return with an `expect` naming the invariant that `rule_mut` already bounds every stored rule key below the table capacity, so a capacity regression now fails loudly instead of silently dropping remaining rules. The `Result` surface on `RuleEvidenceTable::record`/`replace`/`mark_event_truncated`/`merge_equal_capacity` and the in-flow rollback/discard callers are unchanged.

### [Compiler: normalized IR surface]

#### [x] READ-003 — Normalized IR types expose both raw `pub(crate)` fields and accessor methods, and callers mix both

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:18-302`

`NormalizedQuery { root, emission }` plus `root()`/`emission()`; `NormalizedEmission
{ kind, symbol }` plus `kind()`/`symbol()`; `NormalizedEvent { slot, event, subject,
arguments }` plus `event()`/`identity()`/`subject()`/`arguments()`; `NormalizedLifecycle
{ sources, condition, completion }` plus `sources()`/`condition()`/`completion()`;
`CanonicalArgumentConstraints.groups` plus `groups()`; `ArgumentConstraintGroup`
fields plus `index()`/`predicates()`. Both surfaces are in active use across the
compiler: field access in `normalize.rs:42-48,64,104-107,473-483`,
`normalize_all.rs:278-283`, and `apply_slot_map` (`normalize.rs:178-207`), while
accessor access is used in `physical/planner.rs:29-30`, `reference.rs:183,203,342`,
`contradiction.rs:40-42`, and `physical/validation.rs:37-63`. With `pub(crate)`
fields, the "groups are sorted, deduplicated, and non-empty" invariant documented on
`CanonicalArgumentConstraints` (normalized.rs:61-64) rests on the convention that
construction happens only via `from_constraints`; the raw field surface admits
struct-literal construction that bypasses that guarantee, and every field change
must be mirrored in the accessor. There are effectively three
encapsulation conventions in the same module family: private fields + public
accessors (`classification.rs`), `pub(crate)` fields + accessors (this module), and
bare `pub(crate)` fields (`EvidenceDescriptor` in `compiler/mod.rs:119-123`,
`IdentityConstraint` in `compiler/mod.rs:69-100`).

**Recommendation:** Pick one surface for the normalized IR and migrate the ~six
in-crate consumers to it. Prefer private fields with narrow accessors so reads and
construction flow through `from_constraints` and the accessors, the surfaces where
the canonical-form invariants are guaranteed; keep `Ord`/`Hash` derives and
`from_constraints` construction unchanged. Guardrail: do not remove the accessors
that the physical planner and test oracle rely on without updating those callers in
the same change; keep the `#[cfg(test)] to_flat_vec` seam.

**Fix Applied:** Made all normalized-IR fields private (`NormalizedQuery`, `NormalizedEmission`, `NormalizedEvent`, `NormalizedLifecycle`, `CanonicalArgumentConstraints`, `ArgumentConstraintGroup`) behind `new` constructors and the existing accessors, adding `NormalizedEvent::slot()` and the `#[cfg(test)]` seams `CanonicalArgumentConstraints::from_groups_for_test`/`ArgumentConstraintGroup::new_for_test` for non-canonical validation fixtures. Slot traversal (`collect_slots`/`remap_slots`/`alpha_renumber_slots`) moved onto `NormalizedRoot` where the internal state is owned. Migrated all in-crate consumers (normalize.rs, normalize_all.rs, and the normalize/physical test suites) to accessors and constructors; `Ord`/`Hash` derives, `from_constraints`, and `to_flat_vec` unchanged.

### [Compiler: identity model]

#### [x] READ-004 — Parallel identity model `IdentitySpec`/`IdentityConstraint` with duplicated, semantically-divergent emptiness checks

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:69-117,127-157`; `glass-lint-core/src/api/compiler/validate/error.rs:382-397`

`IdentityConstraint` (compiler/mod.rs:69-100) is a structural copy of the declaration
`IdentitySpec` (`pub(crate)`, `api/rule/query/event.rs:7`) with the same nine
variants (`Heuristic` renamed to `Any`) and a
hand-written 1:1 `lower_identity` conversion (mod.rs:127-157) plus a parallel
`is_empty` check (mod.rs:102-117). The emptiness logic is duplicated in
`is_identity_empty` (validate/error.rs:382-397), and the two already disagree:
`IdentityConstraint::is_empty` uses `name.is_empty()`/`module.is_empty()` without
trimming, while `is_identity_empty` uses `name.trim().is_empty()`. Every future
identity shape requires coordinated edits in `lower_identity`, `IdentityConstraint`
(definition + `is_empty`), `is_identity_empty`, `requirements.rs:99-135`, and the
matcher matches in `analysis/matching/arguments/identity.rs` — five places for one
concept, with the whitespace semantics already drifting.

**Recommendation:** Keep the declaration-vs-IR layer boundary but make the conversion
the single canonical path: implement `From<&IdentitySpec> for IdentityConstraint` in
place of the free `lower_identity`, and align the two emptiness checks on one
documented whitespace policy so they cannot diverge again (the trim difference is
drift, not intentional — see Open Questions). Guardrail: do not merge the two enums
into one type — the `Heuristic`→`Any` rename stays an explicit lowering step so the
authoring vocabulary and the IR vocabulary remain distinct; both types are
`pub(crate)` with identical field types, so this is purely a boundary-of-record
decision.

**Fix Applied:** Replaced the free `lower_identity` function with `impl From<&IdentitySpec> for IdentityConstraint` (keeping the explicit `Heuristic`→`Any` rename) and migrated the planner, reference oracle, and `rule` re-export to it. Aligned `IdentityConstraint::is_empty` with the declaration-side `is_identity_empty` trimmed-whitespace policy and documented the shared policy on both checks, so they cannot diverge again.

### [Compiler: evidence descriptor]

#### [x] READ-005 — `NormalizedEmission` and `EvidenceDescriptor` are parallel `{kind, symbol}` types with a manual field-by-field copy duplicated per root

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:35-39`; `glass-lint-core/src/api/compiler/mod.rs:119-123`; `glass-lint-core/src/api/compiler/physical/planner.rs:29-30,66-69`

`NormalizedEmission { kind: MatchKind, symbol: String }` (normalized.rs:35-39) and
`EvidenceDescriptor { kind: MatchKind, symbol: String }` (compiler/mod.rs:119-123)
have identical shape and vocabulary, differing only in that one has accessor methods
and one has bare fields. `plan_normalized_roots_into` rebuilds the descriptor by hand
at `planner.rs:66-69` (`EvidenceDescriptor { kind, symbol: symbol.to_owned() }`) once
per physical root, even though every evidence-bearing root in a production plan
carries the same emission (`plan_root` threads the same `kind`/`symbol` to every
event branch; lifecycle roots use only `symbol`). This is a
parallel model type plus a manual conversion path plus per-root duplication of a
plan-constant value.

**Recommendation:** Add one canonical conversion (`From<&NormalizedEmission> for
EvidenceDescriptor`, or derive the descriptor directly from the emission) and use it
at the single construction site; optionally hoist the constant emission to plan level
if the tests that construct roots with distinct descriptors are preserved.
Guardrail: keep the per-root descriptor shape — `optimize_roots`'s dedup rule
(`physical.rs:429-433`) and the physical tests (`api/compiler/tests/physical.rs`
constructs roots with distinct descriptors at 377-437; `tests/rule.rs:58` asserts a
per-root descriptor) deliberately treat differing evidence descriptors as distinct
roots.

**Fix Applied:** Added `From<&NormalizedEmission> for EvidenceDescriptor` as the single canonical conversion and used it once in `plan_normalized_roots_into`, threading the descriptor through `plan_root`/`plan_event` so the manual field-by-field `EvidenceDescriptor { kind, symbol: symbol.to_owned() }` copy per root is gone. Each root still carries its own cloned per-root descriptor, so the `optimize_roots` dedup rule and the physical tests that assert distinct descriptors are unchanged.

**Fix Applied:** None so far.

### [Compiler: vocabulary ownership]

#### [x] READ-006 — `api::rule` declarations and the compiler IR reach into `api::classification` for the `MatchKind` occurrence vocabulary

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/classification.rs:324-345`; `glass-lint-core/src/api/rule/query/mod.rs:220,273-284,365`; `glass-lint-core/src/api/compiler/mod.rs:54`

`MatchKind` is defined in `api::classification`, whose module doc describes it as
"Serializable capability classifications and source evidence" (classification.rs:1-5),
yet it is also the canonical occurrence-kind vocabulary for the public rule layer
(`EmissionDecl::kind() -> MatchKind` at api/rule/query/mod.rs:365, derived by
`evidence_kind_for_event` at mod.rs:273-284) and for the compiler IR
(`EvidenceDescriptor.kind`, `NormalizedEmission.kind`). The report/serialization
module therefore becomes the de-facto home of the semantic event-kind vocabulary used
by two layers upstream of it; adding a kind requires coordinated edits in
classification.rs, api/rule/query/mod.rs, and `analysis/matching/query/mod.rs:246-298`.

**Recommendation:** Either relocate `MatchKind` (and its `as_str` contract) to a lower,
shared vocabulary module under `api/` that classification re-exports for
serialization, or, if classification is intentionally the stable vocabulary home,
update its module doc to state that it is the shared occurrence-kind vocabulary for
rule declarations, compiler IR, and reports. Guardrail: the `as_str()` spellings
(`call`, `member_call`, ...) are a stable serialized contract — preserve exact
strings; do not split the vocabulary into two enums.

**Fix Applied:** Relocated `MatchKind` (definition and `as_str` contract) to `api/rule/query` as the shared occurrence-kind vocabulary, re-exported it from `api::rule`, and made `api::classification` re-export it for serialization (module doc updated accordingly). The public `rules::MatchKind` re-export now points at `api::rule::MatchKind`. Migrated the compiler IR, matching, and flow consumers plus all test imports to the canonical `crate::api::rule::MatchKind` path. Exact `as_str()` spellings preserved; no vocabulary split.

**Fix Applied:** None so far.

### [Compiler: normalization]

#### [ ] READ-007 — `normalize_event_from_query` carries an unused `emission` parameter through every normalize call

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/normalize.rs:225-245,340-348,467-484`

`normalize_root` threads `emission` into every child normalizer, but the only consumer
is `check_branch_evidence_compatibility` (via `emission.primary_var`, normalize.rs:298).
`normalize_event_from_query` accepts it as `_emission` and never uses it
(normalize.rs:467-470), and `normalize_lifecycle_root` passes it through
(normalize.rs:344-348) only to discard it. The parameter adds plumbing to every
signature in the module for no behavior.

**Recommendation:** Drop the parameter from `normalize_event_from_query` (and stop
passing it at the `normalize_lifecycle_root` call site); keep it only on the Any
normalization path where `primary_var` is actually read. Guardrail: none — this is
purely mechanical.

**Fix Applied:** None so far.

#### [ ] READ-008 — `all_share_some` in same-event normalization has a misleading name and a convoluted shape

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/normalize_all.rs:44-71`

After `find_common_event_var` returns `None`, the fallback computes
`all_share_some` (normalize_all.rs:47-59) as "the first branch shares a variable with
at least one other branch", expressed as nested closures over `QueryShapeFacts`, and
uses it to choose between `UnsupportedRelation` and `UncorrelatedConjunction` — two
diagnostics with materially different meanings. The name reads as "all branches share
some variable", which is not what it computes, and the branch explains neither the
"some correlation without a common event var" case nor why that case is "unsupported"
rather than "uncorrelated".

**Recommendation:** Rename the predicate to what it computes (e.g.,
`first_branch_correlates_with_any`), extract the nested closure into a named helper,
and add a comment stating why partial correlation without a shared event variable is
`UnsupportedRelation` rather than `UncorrelatedConjunction`. Guardrail: keep the two
error outcomes distinct — they surface as different author-facing diagnostics.

**Fix Applied:** None so far.

### [Compiler: plan accumulation]

#### [ ] READ-009 — `QueryPlanAccumulator` is a single-use two-field grouping with no added invariant

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:159-169,172-191`

`QueryPlanAccumulator { roots, budget }` is constructed in exactly one function
(`compile_queries`, mod.rs:172-191), its fields are mutated by an external free
function (`physical::plan_normalized_roots_into`) that takes `&mut budget,
&mut roots` separately, and `finish()` just seals `PhysicalPlan`. It adds a name but
no invariant or ownership boundary — the two locals would read identically, and the
free-function coordination is a sign that the struct is grouping unrelated mutable
state rather than owning a phase.

**Recommendation:** Replace the accumulator with the two local variables in
`compile_queries` and keep the seal step inline (or as a small helper taking the
finished `Vec<PhysicalRoot>`), preserving the `MatcherBuildError` mapping in
`finish`. Guardrail: keep the `RootBudget` reserve/limit enforcement and the
`from_planned_roots` validation boundary unchanged.

**Fix Applied:** None so far.

## Systemic Themes

- **Mixed field/accessor surfaces on internal IR.** The normalized IR types
  (READ-003) expose both raw `pub(crate)` fields and accessor methods, while
  `EvidenceDescriptor`/`IdentityConstraint` expose bare fields and `classification.rs`
  keeps everything private behind accessors. A single, consistent encapsulation
  convention across `api/compiler` would remove the recurring "which surface do I
  use?" question.
- **Parallel model types across layer boundaries with hand-written conversions.**
  `IdentitySpec`/`IdentityConstraint` (READ-004) and
  `NormalizedEmission`/`EvidenceDescriptor` (READ-005) repeat a concept across the
  declaration/IR and IR/physical boundaries with manual field copies; the 
  duplicate checks have already started to diverge (trim vs. no-trim emptiness).
- **Result-shaped APIs whose errors are structurally impossible at every call site.**
  Capacity-guarded `RuleEvidenceTable` methods (READ-002) push rollback/discard/fail-
  open handling into every caller, obscuring the real invariant (in-range rule
  index) that is always already satisfied.

## Open Questions

- `pub mod classification` is part of the public API surface, but no crate outside
  `glass-lint-core` (providers, output, harness, CLI) references
  `ClassificationResult`/`MatchedCapability`/`MatchKind` today; every consumer is
  in-crate (`analysis/project/projection.rs`, `lint/report/evidence.rs`,
  `lint/report/mod.rs`), and the external report surface is `project::FileReport`
  (consumed by `glass-lint-output`). Resolved: whether `pub` is reserved for a future
  report consumer is a roadmap call; if no external consumer is planned, the module
  can be `pub(crate)` today.
- The `normalized::ObjectSlot` and `physical::ObjectSlot` (converted with validation
  at `physical.rs:125`) are two u32 newtypes for the same concept across IR stages,
  one of which rejects `u32::MAX` (`physical::ObjectSlot::new`, `physical.rs:76-80`).
  Resolved: this is an intentional stage boundary with a real invariant addition —
  normalized slots are dense and may hold any `u32`, while physical slots reject
  `u32::MAX` as an impossible dimension; acceptable unless the two stages are ever
  merged.
- `IdentityConstraint::is_empty` (no trim) vs `is_identity_empty` (trim) — is the
  whitespace difference intentional (declaration validation vs. validated IR defense)
  or drift? Resolved: drift, not intentional. Declaration validation
  (`is_identity_empty`, `validate/pass1_3.rs:18`) rejects whitespace-only identities
  before lowering, so the no-trim IR check never observes a trim-empty value in
  production; the checks should be aligned per READ-004.

## Coverage

Reviewed files:
- `glass-lint-core/src/api/classification.rs` (all)
- `glass-lint-core/src/api/classification/result.rs` (all)
- `glass-lint-core/src/api/classification/tests.rs` (all)
- `glass-lint-core/src/api/compiler/mod.rs` (all)
- `glass-lint-core/src/api/compiler/catalog.rs` (all)
- `glass-lint-core/src/api/compiler/contradiction.rs` (all)
- `glass-lint-core/src/api/compiler/error.rs` (all)
- `glass-lint-core/src/api/compiler/limits.rs` (all)
- `glass-lint-core/src/api/compiler/normalize.rs` (all)
- `glass-lint-core/src/api/compiler/normalize_all.rs` (all)
- `glass-lint-core/src/api/compiler/normalized.rs` (all)
- `glass-lint-core/src/api/compiler/rule.rs` (re-export hub and record types)
- `glass-lint-core/src/api/compiler/reference.rs` (test oracle consuming the IR)
- `glass-lint-core/src/api/compiler/requirements.rs` (consumer of `IdentityConstraint`)
- `glass-lint-core/src/api/compiler/validate/error.rs` (parallel identity checks)
- `glass-lint-core/src/api/compiler/physical.rs` and `physical/planner.rs`
  (consumers of chunk-20 types; planning itself is chunk 21)
- `glass-lint-core/src/api/rule/query/mod.rs` (declaration-side `MatchKind` usage)

Representative callers traced: `analysis/matching/evidence.rs`,
`analysis/flow/projector/{evidence.rs,state.rs}`, `analysis/flow/cross/evidence.rs`,
`analysis/project/projection.rs`, `lint/report/evidence.rs`, `lint/catalog.rs`,
`analysis/matching/{query/mod.rs,arguments/*}`.

`git status --short` after this audit shows only this file as new; no source files
were modified.
