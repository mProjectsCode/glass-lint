# Codebase Readability Audit — glass-lint-core Chunk 21: Physical planning and validation

## Summary

This chunk owns the tail of the query-compiler pipeline in `glass-lint-core/src/api/compiler`:
declaration/query validation passes (`validate`, `validate/error`, `validate/pass1_3`,
`validate/pass4_10`), compiled-rule records (`rule`), object-flow lowering
(`object_flow`), physical-root selection and sealing (`physical`, `physical/planner`,
`physical/validation`), and exact preparation requirements (`requirements`).

The chunk's contract to sibling modules and consumers:

- `validate` must reject invalid authored queries with stable, categorized diagnostics
  (`QueryCompileError`) before normalization.
- `physical::planner` lowers `NormalizedQuery` into a root set under a `RootBudget`;
  `PhysicalPlan` seals that root set at one validation boundary and derives exact
  `PlanRequirements`.
- `object_flow` lowers normalized lifecycle IR into executable `CompiledObjectFlow`
  matchers consumed by the analysis flow engines.
- `requirements` exposes the capability sets the runtime preparation must consult.
- The production compile path (`mod.rs::compile_queries` → `QueryPlanAccumulator`)
  is the only sealed caller; most other entry points are `#[cfg(test)]` conveniences.

Overall the chunk is well factored: validation is organized as a small number of
consolidated passes, the physical boundary re-validates fail-closed with a dedicated
error type, and requirements are accumulated through narrow `require_*` mutations.
Findings below are about duplicated/conflicting expressions of the same invariant
across layers, a leaked enum representation consumed outside its owner, and a few
test-support surfaces and single-use wrappers that add naming or conversion noise.

## Findings

### api/compiler/validate — duplicate empty-identity validation

#### [x] READ-001 — `check_identity_not_empty` is unreachable after `validate_event_query`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `api/compiler/validate/pass4_10.rs:71-83, 122-130`; `api/compiler/validate/pass1_3.rs:18-25`

`check_structure`'s `Event` branch calls `validate_event_query(eq)?` (pass4_10.rs:72)
and then `check_identity_not_empty(eq.identity())?` (pass4_10.rs:74). Both use the
same `is_identity_empty` predicate (pass1_3.rs:18-25, pass4_10.rs:123), so whenever
the identity is empty the first check already returns
`InvalidEventPredicate` and the second can never fire. `check_identity_not_empty`
and its `UnsupportedRelation` diagnostic (pass4_10.rs:122-130) are dead code that
advertises a second error category for a condition that cannot reach it.

**Recommendation:** Delete `check_identity_not_empty` and its `// pass_relation_availability`
call site, leaving `validate_event_query` as the single empty-identity check with its
stable `invalid_event_predicate` diagnostic; drop the now-unused `is_identity_empty`
import (pass4_10.rs:4). Guardrail: keep the empty-identity rejection classified as an
authored `InvalidEventPredicate`, not as `UnsupportedRelation`.

**Fix Applied:** Deleted `check_identity_not_empty` and its `pass_relation_availability`
call site; empty-identity rejection now flows solely from `validate_event_query`
(`invalid_event_predicate`); dropped the unused `is_identity_empty` import.

### api/compiler/object_flow — conflicting accessor names for one field

#### [x] READ-002 — `CompiledObjectSource` exposes the same field under two names split by `cfg(test)`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `api/compiler/object_flow.rs:198-205`; callers `api/compiler/reference.rs:132,158`, `analysis/flow/planning.rs:305`, `analysis/flow/cross/sources.rs:222`

`CompiledObjectSource` defines `arguments()` (object_flow.rs:198-201, `#[cfg(test)]`)
and `argument_constraints()` (object_flow.rs:203-205, production) for the identical
`&CanonicalArgumentConstraints` value. The names are both meaningful in isolation,
so a reader must inspect `cfg(test)` gates to know which to call, and the two names
make the accessor surface look like two different operations.

**Recommendation:** Keep one accessor name for production (either one) and have the
test-only reference representation call it. Delete the other. Guardrail: none beyond
keeping the returned reference read-only.

**Fix Applied:** Deleted the `#[cfg(test)] arguments()` accessor; the test-only
reference representation now calls the single production `argument_constraints()`.

### api/compiler — the "call-bearing event" predicate is defined three times

#### [x] READ-003 — `event_supports_constraints` wrapper plus a hand-written duplicate in `PhysicalRoot::validate`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `api/compiler/validate/error.rs:400-402`; `api/compiler/validate/pass1_3.rs:26`; `api/compiler/physical.rs:188`; canonical owner `api/rule/query/event.rs:85-87`

`EventSpec::supports_arguments` is the single canonical predicate (`matches!` on
`Call | MemberCall`, event.rs:85-87). It is (a) re-exported as the one-line free
function `event_supports_constraints` (error.rs:400-402) that only its single caller
pass1_3.rs:26 uses, and (b) re-implemented by hand at the physical boundary as
`matches!(event, EventSpec::Call | EventSpec::MemberCall { .. })` (physical.rs:188).
Two of the three expressions must be kept in lock-step with the canonical owner; a
future event kind that supports arguments would silently diverge across layers.

**Recommendation:** Delete `event_supports_constraints` and call
`eq.event().supports_arguments()` at pass1_3.rs:26; replace the hand-written
`matches!` at physical.rs:188 with `event.supports_arguments()`. Guardrail: the
physical check must keep failing closed with `ConstraintsRequireCallEvent`.

**Fix Applied:** Deleted `event_supports_constraints`; both validation layers now
delegate to the canonical `EventSpec::supports_arguments`, with the physical boundary
still failing closed on `ConstraintsRequireCallEvent`.

### api/compiler/physical — parallel `ObjectSlot` newtype with duplicated conversion

#### [x] READ-004 — physical `ObjectSlot` duplicates `normalized::ObjectSlot`; the fallible conversion is inlined twice

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `api/compiler/physical.rs:73-81, 116-130, 132-144`; `api/compiler/normalized.rs:183-197`; `api/compiler/physical/planner.rs:93-109`

`physical::ObjectSlot(u32)` (physical.rs:73) is a near-copy of
`normalized::ObjectSlot(u32)` (normalized.rs:183) that exists only to reject the
`u32::MAX` sentinel at the executable boundary. The conversion is written out twice,
identically, inside the two constructors `returned_subject` (physical.rs:125) and
`instance_subject` (physical.rs:140) as `ObjectSlot::new(object_slot.get())?`; the
planner drives both via `NormalizedObjectSlot` arguments (planner.rs:93-109). Two
parallel newtypes with an ad-hoc, repeated fallible boundary make the slot invariant
easy to bypass and hard to audit.

**Recommendation:** Express the boundary as one canonical `TryFrom<NormalizedObjectSlot>
for ObjectSlot` (or `TryFrom<u32>`) and call it from both constructors, folding
`ObjectSlot::new` into it. Guardrail: keep the two IR layers distinct and keep the
fail-closed `ImpossibleDimensions` rejection; do not loosen the sentinel check.

**Fix Applied:** Folded `ObjectSlot::new` into one canonical
`TryFrom<NormalizedObjectSlot> for ObjectSlot`; both `returned_subject` and
`instance_subject` now call it, keeping the IR layers distinct and the sentinel
rejection fail-closed on `ImpossibleDimensions`.

### api/compiler/object_flow — enum representation matched outside its owner

#### [x] READ-005 — `CompiledObjectSinkArguments` variants are matched by hand in `analysis/flow/planning.rs`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `analysis/flow/planning.rs:249-254, 237`; owner `api/compiler/object_flow.rs:250-253, 256-264`

`BoundSink` stores a cloned `CompiledObjectSinkArguments` (planning.rs:237) and its
`matches_argument` (planning.rs:249-254) re-implements the `Any`/`Indices`
interpretation by matching the enum variants directly, while the owner already
encodes the same semantics in `present_indices` (object_flow.rs:256-264). The
`Any`-vs-`Indices` decision is therefore duplicated across the chunk and the flow
engine, so a change to the sink-argument model (e.g. adding a bounded index form)
requires touching both places.

**Recommendation:** Move the membership test onto `CompiledObjectSinkArguments` as a
narrow domain operation (e.g. `matches_argument(usize) -> bool`) and have
`BoundSink::matches_argument` delegate to it, so flow consumers stop matching the
public enum. Guardrail: preserve the distinction between unbounded membership
(`Indices.contains`) and the count-bounded `present_indices` iteration.

**Fix Applied:** Added `CompiledObjectSinkArguments::matches_argument` as the owner's
membership operation; `BoundSink::matches_argument` now delegates to it, so the flow
engines no longer match the enum by hand.

### api/compiler/object_flow — `Indices(Vec<usize>)` over-generalizes a single-index value

#### [x] READ-006 — `CompiledObjectSinkArguments::Indices` is always constructed with one element

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `api/compiler/object_flow.rs:250-253, 300, 317-323`; `analysis/flow/planning.rs:252`

The only construction of `Indices` is `Indices(vec![*index])` from
`NormalizedLifecycleSink::ArgumentOf` (object_flow.rs:300); the `Vec` never holds
more than one index, yet its shape forces consumers to handle the general case:
`matches_argument` uses `indices.contains(&argument)` (planning.rs:252) and the
test-only `fixed_argument` reads `indices.first()` (object_flow.rs:321). The
multi-index vocabulary invites callers to assume a capability the model never
exercises.

**Recommendation:** Model the two real states directly (`Any` | `Single(usize)`).
Multi-index is unreachable from the normalized lifecycle IR (see Open Questions), so
the `Vec` form is a leftover generalization. Guardrail: keep `present_indices` bounded
and keep `Any` meaning "all arguments of the target call".

**Fix Applied:** Replaced `Indices(Vec<usize>)` with `Any` | `Single(usize)`;
`present_indices` stays count-bounded and `Any` still means all target-call
arguments.

### api/compiler/validate — immediately-consumed `validate_subject_relation` wrapper

#### [x] READ-007 — `validate_subject_relation` only discards the classification result

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `api/compiler/validate/error.rs:303-308`; sole caller `api/compiler/normalize.rs:113-117`

`validate_subject_relation` is `classify_subject_relation(event, subject).map(|_| ())`
(error.rs:303-308): it adds no vocabulary, invariant, or error mapping, and its only
caller (normalize.rs:113) uses just success/failure plus `error.detail()`. Meanwhile
the planner calls the underlying `classify_subject_relation` directly
(planner.rs:64). The wrapper is a redundant façade between the two callers.

**Recommendation:** Have normalize.rs call `classify_subject_relation` directly and
delete `validate_subject_relation`. Guardrail: preserve the `InternalInvariant`
classification of the post-normalization failure in normalize.rs.

**Fix Applied:** Deleted `validate_subject_relation`; normalize.rs now calls
`classify_subject_relation` directly, keeping the post-normalization failure
classified as `InternalInvariant`.

### api/compiler/physical — misleading test-only constructor surface on `PhysicalPlan`

#### [ ] READ-008 — `from_roots` duplicates `from_planned_roots` and is documented as an independent boundary

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `api/compiler/physical.rs:288-306, 316-334`; callers `planner.rs:21`, `mod.rs:166`, `tests/physical.rs:287, 383-386`, `tests/physical_extended.rs:10`

`PhysicalPlan` has four entry points: production `from_planned_roots` (physical.rs:294)
plus three `#[cfg(test)]` constructors — `from_roots` (physical.rs:302), which is a
byte-identical delegation to `from_planned_roots`, `try_new` (physical.rs:317), and
unvalidated `new` (physical.rs:329). The doc comment on `from_planned_roots`
(physical.rs:289-293) claims `from_roots` is "the independent validation boundary for
callers that can supply physical roots directly", but it calls the very same function.
The extra name and misleading boundary documentation make the sealing story harder to
trust.

**Recommendation:** Delete `from_roots` and have test callers use `from_planned_roots`
(including via `plan_normalized`). Keep `new` and `try_new`: their unvalidated and
requirements-mismatch forms are genuinely exercised (tests/physical.rs:383-386, 426,
444; tests/physical_extended.rs:10, 123, 157), and note in their docs why tests bypass
the sealing boundary. Drop the `from_planned_roots` doc claim that `from_roots` is an
independent boundary. Guardrail: the production path must keep the single
`from_planned_roots` validation boundary.

**Fix Applied:** None so far.

### api/compiler/object_flow — inconsistent conversion names

#### [ ] READ-009 — `from_matcher` names on requirements/sinks, `from_normalized_event` on sources

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `api/compiler/object_flow.rs:181, 221, 296`

The three parallel conversions from normalized lifecycle IR to compiled form use
inconsistent names: `CompiledObjectSource::from_normalized_event`
(object_flow.rs:181) is accurate, while `CompiledObjectRequirement::from_matcher`
(object_flow.rs:221) and `CompiledObjectSink::from_matcher` (object_flow.rs:296)
name the same category of conversion by the input's informal role rather than its
type. The mismatch obscures that all three consume `NormalizedLifecycle*` values.

**Recommendation:** Rename the two `from_matcher` constructors to
`from_normalized_event`-style names (e.g. `from_normalized_lifecycle_event` and
`from_normalized_lifecycle_sink`) to match `from_normalized_event`. Guardrail: none
beyond keeping the one-to-one normalized-IR mapping.

**Fix Applied:** None so far.

## Systemic Themes

- **One predicate per invariant, reused across layers.** The identity/event
  compatibility matrix (`is_valid_identity_event_pair` / `classify_subject_relation`),
  the call-bearing predicate (`EventSpec::supports_arguments`), canonical constraint
  construction (`CanonicalArgumentConstraints::from_constraints`), and lifecycle source
  classification (`classify_lifecycle_source`) are the canonical owners; validation,
  normalization, and physical sealing each re-invoke them. This layering is sound
  (fail-closed sealing), but every hand-written re-implementation of a predicate
  (physical.rs:188, planning.rs:249) forks the semantics. Keep the canonical owner and
  delegate.

- **Flow engines share one hand-written sink-argument re-implementation.** The
  engines consume `CompiledObjectSinkArguments` only through `BoundSink` accessors
  (`summary/sink.rs:216` via `present_indices`, `cross/propagation.rs:173` and
  `projector/evidence.rs:89` via `matches_argument`), but `BoundSink::matches_argument`
  (planning.rs:249-254) still re-implements `Any`/`Indices` membership by hand.
  Narrowing the owner's domain operations (READ-005) is the smallest step that keeps
  the engines independent.

- **Test-support surface on production types.** `PhysicalPlan` constructors
  (READ-008), `PlanRequirements::value_resolution()`/`project_requirements()` returning
  raw `BTreeSet`s, `compile_argument_constraints`, `test_with_evidence_counts`, and the
  `#[cfg(test)]` accessors `arguments()`/`fixed_argument()` all live behind `cfg(test)`.
  They are convenient and bounded; keep them gated and prefer behavioral assertions
  (e.g. `needs_*`, `summary()`, `explain()`) where the exact set is not itself the
  contract being asserted.

## Open Questions

- **Resolved:** `PlanRequirements::value_resolution()` / `project_requirements()`
  (requirements.rs:78, 87) are used by `tests/normalize/algebra.rs` and
  `algebra_extended.rs` as the exact-set contract (`contains` at algebra.rs:380-382,
  full-set equality at algebra.rs:393-398). Both accessors are `#[cfg(test)]`-gated
  immutable references, so they are the minimal way to assert the exact requirement
  sets; a dedicated test-side assertion API would re-wrap the same sets without
  narrowing the surface. Keep the accessors.
- **Resolved:** `CompiledObjectSinkArguments::Indices` is constructed only at
  object_flow.rs:300 with `vec![*index]`, and the normalized lifecycle IR
  (`NormalizedLifecycleSink`) defines only single-index `ArgumentOf` plus
  `AnyArgumentOf`, so multi-index sinks are not reachable from authored queries.
  The `Vec` is a leftover generalization (see READ-006).
- **Resolved:** Authored queries cannot produce the sentinel: `normalize_query_decl`
  alpha-renumbers every slot to dense `0..n` (normalize.rs:35-40, 212-222), so
  `u32::MAX` is reachable only through direct IR construction such as
  `ObjectSlot::from_raw(u32::MAX)` in `tests/physical.rs:400`. Enforcing the check
  during normalization would add nothing for authored input; the physical boundary is
  the correct sealing point because executable roots are also assembled directly in
  tests (READ-004).

## Coverage

Files reviewed (read-only) for this chunk:

- `glass-lint-core/src/api/compiler/object_flow.rs`
- `glass-lint-core/src/api/compiler/physical.rs`
- `glass-lint-core/src/api/compiler/physical/planner.rs`
- `glass-lint-core/src/api/compiler/physical/validation.rs`
- `glass-lint-core/src/api/compiler/requirements.rs`
- `glass-lint-core/src/api/compiler/rule.rs`
- `glass-lint-core/src/api/compiler/validate/mod.rs`
- `glass-lint-core/src/api/compiler/validate/error.rs`
- `glass-lint-core/src/api/compiler/validate/pass1_3.rs`
- `glass-lint-core/src/api/compiler/validate/pass4_10.rs`

Supporting files traced for contracts and callers:

- `glass-lint-core/src/api/compiler/mod.rs` (compile path, `IdentityConstraint`,
  `lower_identity`, `QueryPlanAccumulator`)
- `glass-lint-core/src/api/compiler/normalized.rs` (`ObjectSlot`, `EventSlot`,
  `CanonicalArgumentConstraints`, `NormalizedLifecycle*`)
- `glass-lint-core/src/api/compiler/normalize.rs`, `normalize_all.rs`
- `glass-lint-core/src/api/compiler/error.rs` (`PhysicalPlanValidationError`)
- `glass-lint-core/src/api/compiler/contradiction.rs`, `reference.rs`
- `glass-lint-core/src/api/compiler/tests/{physical.rs, physical_extended.rs,
  normalize.rs, reference.rs, reference_extended.rs, normalize/pipeline.rs,
  normalize/algebra.rs, normalize/algebra_extended.rs}`
- `glass-lint-core/src/api/rule/query/event.rs`, `query/constructors.rs`
- `glass-lint-core/src/analysis/flow/planning.rs`, `summary/sink.rs`,
  `cross/{propagation.rs, evidence.rs, sources.rs, state.rs}`, `projector/evidence.rs`
- `glass-lint-core/src/analysis/matching/query/mod.rs`, `matching/arguments/{tests.rs,
  tests/extended.rs}`
- `glass-lint-core/src/analysis/project/projection.rs`
