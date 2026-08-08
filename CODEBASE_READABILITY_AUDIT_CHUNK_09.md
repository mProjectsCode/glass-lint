# Codebase Readability Audit

## Summary

Chunk 9 owns the compiler transition from validated query declarations to
immutable physical matcher plans, plus classification evidence and runtime
requirement aggregation. The phase boundaries are mostly explicit and the
compiler keeps its IR crate-private, but several contracts still expose state
or duplicate meaning across layers. Classification results expose mutable
storage, compiler identity strength carries redundant state, logical
alternative limits are local rather than aggregate, and projection rebuilds
plan requirements through a collection of boolean accessors. These are
readability and ownership problems at the compiler/consumer boundary rather
than matcher-policy changes.

## Findings

### Classification result ownership

#### [x] READ-034 — Keep classification result storage behind its collection API

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/classification.rs:450-462`; construction `glass-lint-core/src/analysis/project/projection.rs:53-82`

`ClassificationResult` exposes `pub capabilities: Vec<MatchedCapability>` and
also provides `capabilities()` as a read-only accessor. Any caller that can
hold a result can therefore reorder, remove, or append capabilities through
the field, bypassing the result’s documented catalog-order contract and the
classification module’s construction path. The accessor is not the actual
ownership boundary while the field remains public.

This is especially inconsistent with `MatchedCapability`, whose rule index,
label, severity, and evidence are private and are created only by the owning
module. It also makes future result invariants difficult to add without a
breaking field change, while the existing project assembler already has a
single append owner.

**Recommendation:** Make `ClassificationResult::capabilities` private and
retain the slice accessor. Add a crate-visible constructor or append method
owned by classification assembly, and keep catalog-order and evidence
invariants there. Preserve `Default` for internal accumulation if needed, but
do not expose the backing `Vec` as the external construction API.

**Fix Applied:** Made `ClassificationResult` capability storage private and
added the crate-visible `push_capability` assembly method. Project projection
now appends through that owner while public consumers retain the read-only
slice accessor.

### Compiler identity representation

#### [ ] READ-035 — Remove redundant identity-strength state from compiler IR

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:68-107, 146-155`; matching consumers `glass-lint-core/src/analysis/matching/arguments/identity.rs:12-39` and `glass-lint-core/src/analysis/matching/query/view.rs:102-132`

`IdentityConstraint::Global` and `IdentityConstraint::Any` both carry an
`IdentityStrength`, but lowering always assigns `Strict` to `Global` and
`Heuristic` to `Any`. All matching consumers pattern-match the variants with
`..` and never inspect the strength. The field therefore duplicates the
semantic distinction already encoded by the identity variant and creates
internal states that the lowering path never produces.

The extra state enlarges every physical root identity, participates in derived
ordering/equality, and suggests that matching behavior can vary independently
of the variant when it cannot. A future caller constructing test or internal
IR values could create a `Global { strength: Heuristic }` combination with no
defined owner for its meaning.

**Recommendation:** Remove `IdentityStrength` from the `Any` and `Global`
variants, or replace the pair with one semantic identity type whose strictness
is represented exactly once. If strength is intended to become an independent
policy later, give it an owner and make each matcher consume it explicitly;
otherwise preserve the current strict-versus-heuristic behavior in the
variant shape and delete the unused field and enum.

**Fix Applied:** None so far.

### Logical expression and physical-plan bounds

#### [x] READ-036 — Enforce one aggregate bound on alternative expansion

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/rule/query/expression.rs:228-317`; `glass-lint-core/src/api/compiler/validate/pass4_10.rs:86-117`; `glass-lint-core/src/api/compiler/normalize.rs:250-285`; `glass-lint-core/src/api/compiler/physical.rs:449-473`

Expression construction limits each `Any`/`All` node to
`MAX_EXPR_CHILDREN` and limits nesting to `MAX_EXPR_DEPTH`, while rule
construction limits only the number of top-level query declarations. There
is no aggregate expression-node, leaf, or physical-root budget. Normalization
recursively flattens nested `Any` branches, and physical planning recursively
expands every alternative into the root vector without checking its final
size.

The result is a collection of individually valid expressions whose combined
normalization and planning work is not bounded by one compiler-owned limit.
The same gap also means `PhysicalPlan` can be constructed from an empty root
set when the lower-level compiler entry point is called directly, even though
a validated rule is expected to contain executable query roots.

**Recommendation:** Define one aggregate physical-root budget owned by the
compiler limits module and charge it as normalization/planning expands
alternatives. Existing per-node child and depth checks already bound authored
expression shape; the aggregate bound should cover the actual executable roots
without adding a speculative second logical-node budget. Propagate a typed
`UnboundedQuery`/plan diagnostic when the bound is exceeded, reject an empty
physical plan at sealing, and preserve deterministic flattening and
deduplication.

**Fix Applied:** Added a compiler-owned `MAX_PHYSICAL_ROOTS_PER_RULE` budget
that is shared across all queries in a rule and charged as normalized
alternatives become physical roots. Normalization stops flattening when the
same bound would be exceeded, and physical-plan sealing rejects both empty
and oversized root sets with typed diagnostics. Nested-alternative and
empty-plan tests cover the bounded expansion and sealing boundaries while
preserving deterministic optimization and deduplication.

### Compiler requirements consumed by projection

#### [ ] READ-037 — Keep compiled-plan requirements as one consumer-facing value

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:258-285`; `glass-lint-core/src/api/compiler/requirements.rs:66-182`; `glass-lint-core/src/analysis/project/projection.rs:253-315`

`PlanRequirements` is the compiler-owned aggregate of value-resolution,
project, and flow preparation needs, but `CompiledMatcherPlan` exposes it to
projection through four separate projections: `needs_project_overlay`,
`needs_module_identities`, `needs_call_result_identities`, and
`flow_requirements`. `ProjectionPlan::from_selection` then repeats the
aggregation with independent booleans and a separately merged
`FlowRequirements` value. The consumer has to know which booleans overlap
(`needs_project_overlay` is derived from project requirements) and preserve
the same OR semantics as the compiler.

This distributes a plan’s preparation contract across the compiler and
projection orchestration. Adding a new requirement can compile successfully
while being omitted from one accessor or one aggregation branch, causing the
runtime to skip required preparation without a type-level indication. The
existing `PlanRequirements::merge_from` already provides the domain operation
that should own this union.

**Recommendation:** Expose one crate-private immutable requirement view from
`CompiledMatcherPlan`, or expose a narrow `merge_requirements_into` operation
that keeps the storage private while letting `ProjectionPlan` aggregate the
compiler-owned value. Derive convenience predicates (`needs_flow`, overlay,
module identities, and result identities) from that one value at the final
consumer boundary. Preserve the current separation between compiler IR and
analysis projection, and retain deterministic rule/root traversal.

**Fix Applied:** None so far.

## Systemic Themes

- Classification and compiler values often have the right private
  constructors, but a few public fields/accessors and redundant enum payloads
  weaken those ownership boundaries. Storage should remain private while
  semantic views stay small and stable.
- Boundedness is enforced at several local declaration sites, yet the
  compiler’s expansion stages need an aggregate budget that follows work from
  logical alternatives into physical roots.
- Runtime preparation requirements are a cross-phase contract. The compiler
  should own their representation and consumers should merge/query that
  representation rather than reconstructing parallel boolean vocabularies.

## Decisions

- Charge the aggregate bound against executable physical roots, because that
  is the resource consumed by planning and projection. Keep the existing
  authored-node depth/child limits as early validation diagnostics rather than
  inventing a second aggregate logical-node budget.
- `IdentityStrength` is not an independent policy in the current compiler:
  `Global` is strict and `Any` is heuristic, and consumers ignore the payload.
  Remove the redundant field and enum; do not preserve an extension point for
  an unimplemented confidence policy.
- `ClassificationResult` is an internal assembly value in the current crate
  graph. Keep mutation private to classification assembly and expose only its
  read-only slice accessor; a separate report-facing type would duplicate the
  same data without a current boundary to justify it.

## Coverage

Reviewed only Chunk 9, “Query classification and compilation,” from
`CODEBASE_STRUCTURE_CORE.md`, including classification evidence and capacity
types, compiler lowering, normalized IR, physical roots and planning,
object-flow compilation, requirements, ordered validation passes, compiled
rule selections, catalog compilation, and the projection consumer boundary.
Existing Chunk 1 through Chunk 8 audit history was used to continue IDs at
READ-034. No source, test, configuration, dependency, or other documentation
files were changed; this chunk audit file is the only new artifact for Chunk
9.
