# Codebase Readability Audit — Chunk 09

## Summary

Chunk 09 owns classification and the compiler pipeline from validated query
declarations through normalized IR, physical roots, preparation requirements,
and evidence accumulation. The phase-separated representations are justified:
normalization removes authoring syntax, physical roots select executable access
paths, and classification preserves path certainty and evidence. The findings
below target immediately flattened compiler allocations, per-root requirement
reconstruction, evidence storage sized by the whole catalog rather than the
selection, and a leaked artifact-local identifier.

## Findings

### Physical plan construction

#### [ ] READ-039 — Leaf physical planning allocates vectors that are immediately flattened

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:190-233`; `glass-lint-core/src/api/compiler/physical.rs:486-564`

`plan_event` returns a `Vec<PhysicalRoot>` even though every event relation
produces exactly one root, and the lifecycle branch wraps its one root in
`vec![planned]`. `plan_root` immediately extends another vector with those
one-element allocations, while `compile_query` returns the resulting vector to
`QueryPlanAccumulator::add`, whose only operation is another `extend`. A
multi-branch query consequently allocates a temporary vector at each leaf
before the aggregate compiler buffer can own the roots.

**Recommendation:** Make the recursive planner own one output collector (or
return a single root from leaf planning and collect only at `Any` boundaries),
and let query compilation append directly to the aggregate plan buffer. Keep
the shared `RootBudget` admission before each retained root, deterministic
root ordering and deduplication, and the existing error mapping; remove only
the one-element vectors and forwarding `add` layer.

**Fix Applied:** None so far.

#### [ ] READ-040 — Physical requirement aggregation creates a temporary requirement set per root

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/api/compiler/physical.rs:146-167,293-311,595-601`

`PhysicalRoot::requirements` constructs a fresh `PlanRequirements`, including
its ordered sets, for every root. `requirements_for_roots` then immediately
merges each temporary into one plan-level accumulator. The executable root is
the correct owner of the root-to-preparation mapping, but the current return
shape makes every plan build allocate and copy short-lived requirement sets
before storing the only aggregate that callers use.

**Recommendation:** Keep the mapping on `PhysicalRoot`, but expose an
owner-level operation that merges its requirements into a caller-owned
`PlanRequirements` accumulator, or let the planner update that accumulator as
roots are admitted. Preserve the exact union semantics, requirement ordering,
and the `PhysicalPlan::from_roots` invariant check; delete only the per-root
temporary set and its associated clone/merge pass.

**Fix Applied:** None so far.

### Selection-sized classification state

#### [ ] READ-041 — Evidence matrices allocate one bucket for every catalog rule on every module

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/rule.rs:61-77`; `glass-lint-core/src/analysis/project/projection.rs:89-120,211-237,252-292`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:61-84,138-151`; `glass-lint-core/src/api/classification.rs:218-228`

`CompiledRuleSelection::evidence_capacity` reports the length of the entire
compiled catalog, not the number of selected rules. `ProjectionPlan` carries
that capacity into `RuleEvidenceTable::new`, which allocates an empty
`Vec<ClassificationEvidence>` for every catalog index for every projected
module. Cross-file flow repeats the same full-catalog allocation in
`ModuleEvidence::new` and materializes another full matrix before merging it
back into the module projection. A selection containing a small subset of a
large catalog therefore pays catalog-sized storage and initialization costs at
each module even though all writes and reads concern selected `RuleIndex`
values.

**Recommendation:** Make the selected-rule boundary own a dense selected-index
mapping, or use a sparse evidence store keyed by validated `RuleIndex`, and
adapt the cross-flow and local projection sinks to that owner. Preserve
catalog-index identity when assembling findings, reject foreign or unselected
indices as today, retain deterministic rule order, and keep any capacity
checks at the selection boundary; do not trade the full matrix for an
unbounded evidence collection.

**Fix Applied:** None so far.

### Classification representation boundary

#### [ ] READ-042 — Classification evidence exposes an artifact-local fact ID through a public accessor

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/classification.rs:78-121,180-196`; `glass-lint-core/src/analysis/matching/evidence.rs:44-105`; `glass-lint-core/ARCHITECTURE.md` (public-surface invariant)

`ClassificationEvidenceOccurrence` is part of the public classification value
surface and its `fact()` method is `pub`, returning a raw `u32`. That number is
an internal fact-stream identity used only for evidence sorting,
deduplication, and truncation; the same type already keeps the trace identity
crate-private. Core’s architecture explicitly keeps fact IDs private, yet a
caller can currently depend on this storage-shaped identifier or mistake it
for a stable cross-artifact identity.

**Recommendation:** Restrict `fact()` to `pub(crate)` and keep fact identity
inside the matcher/evidence owners. If a future consumer needs a stable public
correlation, introduce a deliberately documented semantic value rather than
exposing the artifact index; preserve public spans, symbols, certainty, and
serialized evidence behavior.

**Fix Applied:** None so far.

### Evidence table merge boundary

#### [ ] READ-043 — Evidence-table merging rechecks an impossible index failure for every rule

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/classification.rs:240-305`; `glass-lint-core/src/analysis/project/projection.rs:601-608`

`RuleEvidenceTable::merge` first verifies that both tables have equal lengths,
then enumerates every vector from `other` and calls the fallible `extend`
method with a freshly reconstructed `RuleIndex`. Equal lengths make every one
of those indexed lookups valid, so the method repeats its own capacity check
and returns an error that the production caller immediately converts to
`expect("projected evidence uses one catalog capacity")`. The result is an
extra lookup and error path for every rule at the local/cross-flow merge
boundary, while the table’s actual invariant is already the equal-capacity
check.

**Recommendation:** Give the table a dedicated equal-capacity merge operation
that consumes the second vector-of-vectors and appends by position after one
length check, or make the checked outer boundary separate from an infallible
internal append. Preserve `CapacityMismatch`, per-rule evidence order, and
fail-closed handling for invalid indices; remove only the per-rule
`RuleIndex` reconstruction and impossible `Result` branch.

**Fix Applied:** None so far.

## Systemic Themes

- Compiler phase types are useful when each phase owns a distinct invariant;
  the remaining simplifications should remove temporary transport allocations,
  not collapse normalized and physical semantics into one representation.
- Aggregate state should be sized by the active selection and owned by the
  boundary that validates that selection. Catalog positions can remain stable
  without allocating storage for every unselected rule in every module.
- Internal correlation IDs belong behind matcher and report owners. Public
  classification values should expose semantic evidence, not fact-stream or
  trace storage identities.
- Checked mutation APIs should align with the invariant established by their
  caller. Once equal capacity has been established, repeated fallible indexed
  updates obscure the actual ownership and failure boundary.

## Open Questions

- Measure local and cross-file projection with a large catalog and sparse
  selection before choosing a dense selected-index map versus a sparse
  `RuleIndex` store; both must preserve deterministic output and bounded
  evidence growth.
- Confirm whether `RootBudget` intentionally limits pre-deduplication roots.
  If so, document that it bounds planning work rather than only retained plan
  size; otherwise move the budget boundary to the canonical root owner.
- Audit downstream crate code before changing `fact()` visibility. Current
  workspace uses are internal sorting/deduplication paths, but public API
  consumers may have compiled against the raw accessor if the classification
  module is re-exported in a future surface.

## Coverage

Reviewed the chunk-09 structure entries and their implementation/test support:

- `api/classification.rs`
- `api/compiler/{mod,limits,normalized,normalize,normalize_all,physical,object_flow,requirements,rule}.rs`
- `api/compiler/validate/{error,pass1_3,pass4_10}.rs`
- `analysis/project/projection.rs`, `analysis/flow/cross/evidence.rs`, and
  matcher/report evidence callers
- Compiler normalization, physical-plan, rule-selection, reference, and
  classification tests
- Existing numbered audit reports 001–008 were checked to avoid duplicating
  their historical findings.

No source, test, configuration, dependency, or other documentation files
were changed by this audit.
