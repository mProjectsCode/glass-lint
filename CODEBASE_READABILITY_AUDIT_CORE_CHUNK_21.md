# Codebase Readability Audit — Chunk 21: Physical planning and validation

## Summary

Chunk 21 covers the compiled-plan backend of `api::compiler`:
`object_flow.rs`, `physical`/`planner`/`validation`, `requirements.rs`,
`rule.rs`, and `validate/` (error, pass1_3, pass4_10), with supporting reads of
`normalized.rs`, `reference.rs`, and their consumers across analysis
(`flow/planning`, `flow/cross`, `flow/projector`, `matching/query`,
`matching/arguments`, `project/projection`, `model/flow`).

The overall architecture is sound: compiler IR and physical roots stay private
(`pub(crate)`), the plan is sealed and validated once, lifecycle IR lowers into
execution-shaped compiled flows, and the `requirements` gating drives
projection. `ScopeTypes` is well-owned inside `pass_scope_types`, `EvidenceScope`
is a small legitimate walk flag, `RuleSelectionError`/`CompiledRuleSelection`
validation is clean, and the compiled object families (`CompiledObjectSource`
/Sink/Requirement) are a genuine lowering of the normalized lifecycle IR rather
than a parallel grab-bag.

Four medium and one low finding. The two strongest root causes are (1) a fully
duplicated mode vocabulary (`RequirementMode`/`CompletionMode` vs the model's
`RequirementReadiness`/`SinkReadiness`) that is already lowered one-to-one, and
(2) `PlanRequirements` maintaining two parallel capability sets where the
`value_resolution` dimension has no production reader and must be kept in
lockstep with `project` by every `require_identity` arm. The remaining
findings are a duplicated emptiness policy that is explicitly required not to
diverge, stale validation-pass module naming that no longer matches the
consolidated three-pass pipeline, and a physical `ObjectSlot` retained in
compiled roots that production execution never reads.

## Findings

### [api::compiler::object_flow / analysis::model::flow]

#### [ ] READ-001 — RequirementMode/CompletionMode duplicate the model's RequirementReadiness/SinkReadiness

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/object_flow.rs:21-32`, `object_flow.rs:44-59`; `glass-lint-core/src/analysis/model/flow.rs:160-197`

`RequirementMode { AllRequired, AnyRequired }` and
`CompletionMode { Configuration, AnySink, AllSinks }` are isomorphic to the
model-level `RequirementReadiness { Any, All }` and
`SinkReadiness { Configuration, Any, All }`, and `CompiledObjectFlow::readiness`
(`object_flow.rs:44-59`) maps them one-to-one into `FlowReadiness`. The two
compiler enums buy nothing over the model vocabulary: analysis consumers reach
into the compiler IR for the raw mode anyway
(`analysis/flow/cross/propagation.rs:200`,
`analysis/flow/projector/evidence.rs:187` both compare
`CompletionMode::Configuration`), so a distinct "compile-time" vocabulary is not
actually enforced. Every mode addition must be mirrored in both enums plus the
mapping plus the model.

**Recommendation:** Store `requirement_mode: RequirementReadiness` and
`completion_mode: SinkReadiness` directly on `CompiledObjectFlow`; fold the
`requirement_mode`/`completion_mode` match arms of `readiness()` into
`from_normalized_lifecycle`, keeping `FlowReadiness::new(mode, count, mode,
count)` with the existing default for an absent condition (`Any`/zero
requirements) unchanged. Delete both compiler enums and update the two analysis
comparisons to `SinkReadiness::Configuration`. Guardrail: preserve the
distinction between "no completion policy yet" and an explicit empty `AnySink`
list only through the existing `Option<NormalizedLifecycleCompletion>` source so
`Configuration` remains distinct from a zero-count `AnySink`.

#### [ ] READ-002 — Two lockstepped emptiness policies: IdentityConstraint::is_empty vs is_identity_empty

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:105-127`; `glass-lint-core/src/api/compiler/validate/error.rs:374-396`

`IdentityConstraint::is_empty` (authored IR) and `is_identity_empty`
(declaration side) implement the identical trimmed emptiness policy over the
same component families (name, module/export, module, rooted path, literal,
package specifier, private-network). The doc comments explicitly require the
"two cannot diverge" (`mod.rs:106-109`, `error.rs:374-380`), which is exactly
the pattern that causes silent drift if one side is tightened (e.g. a new
`ModuleSpecifierPattern` emptiness rule). Every trimmed-whitespace and
`as_str`/`path.is_empty` decision is duplicated.

**Recommendation:** Extract narrow component helpers at a single compiler-owned
location (e.g. `fn module_export_empty(module: &str, export: &str) -> bool`,
`fn name_empty(name: &str) -> bool`) and have both `is_empty` implementations
delegate so the policy text exists once; the two match arms collapse to their
vocabulary mappings. Guardrail: keep `Rooted`/`PrivateNetworkAddress`
never-empty semantics and the `trim()`/`as_str()` exactness identical, and add a
test asserting `IdentityConstraint::from(spec).is_empty() ==
is_identity_empty(spec)` for each variant to prove the parity contract.

### [api::compiler::validate]

#### [ ] READ-003 — Stale pass numbering hides the consolidated three-pass pipeline and the orchestrator lives in the wrong module

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/validate/pass1_3.rs:55-67`, `validate/pass4_10.rs:28-52`, `validate/mod.rs:1-13`

The module names `pass1_3` and `pass4_10` encode the retired ten-pass scheme:
`pass4_10.rs:48-52` documents that the passes were consolidated into three
traversals (structure, scope+types, correlation+evidence), yet the filenames
still reflect passes 1-3 and 4-10. The orchestration entry `validate_query_decl`
(`pass4_10.rs:39-46`) lives inside "pass4_10" while importing
`pass_scope_types` from "pass1_3" through a function-local `use super::pass1_3`
(`pass4_10.rs:40`), and `validate_event_query` (defined in `pass1_3.rs:9`) is
the structural validator only consumed by pass4_10's `pass_structure` and
`validate_lifecycle`. The name boundary no longer matches the ownership
boundary.

**Recommendation:** Rename the modules to the consolidated pass names — e.g.
`structure.rs`, `scope_types.rs`, `correlation_evidence.rs` — and move the
`validate_query_decl` orchestration into `validate/mod.rs`, which then owns
pass ordering as its single responsibility. Guardrail: keep the current pass
order (structure before scope before correlation) and the `#[cfg(test)]`
re-exports (`pass_scope_types`, `pass_structure`, `pass_correlation_evidence`)
identical so `tests/validate/*` needs no behavioral change.

### [api::compiler::requirements]

#### [ ] READ-004 — PlanRequirements: parallel capability sets where value_resolution is unread in production and require_identity must update both in lockstep

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/compiler/requirements.rs:69-179`; consumers `glass-lint-core/src/analysis/project/projection.rs:145-148`, `glass-lint-core/src/api/compiler/physical.rs:363-386`

`PlanRequirements` holds two capability collections — `value_resolution`
(`ValueResolutionRequirement`) and `project` (`ProjectRequirement`) — together
with `FlowRequirements`. In production, `value_resolution` is read only through
the cross-set OR in `needs_call_result_identities` (`requirements.rs:156-163`);
`LocalStaticValues` and `ModuleIdentityValues` have no production reader (their
only appearance outside tests is the `#[cfg(test)]` `summary`/`explain`
output at `physical.rs:370-386`), while `require_identity`
(`requirements.rs:99-135`) must insert the same capability into both sets in
lockstep (`CallResultIdentities` is added to each). Any future capability arm
that forgets one set silently mis-gates the project phase, and the OR predicate
is required merely to paper over the split. The runtime gates in
`projection.rs:145-148` consult only the `project` dimension plus the OR.

**Recommendation:** Make the capability a single owned dimension — either keep
`project` as the only capability set and derive `needs_call_result_identities`
from it, or introduce one `BTreeSet` of a small capability enum and span
derived `needs_*` predicates over it — and have the executor gate on it
directly so recorded capabilities are actually consumed. Deletion target:
`ValueResolutionRequirement::LocalStaticValues`/`ModuleIdentityValues` and the
cross-set OR, updating `tests/normalize/algebra.rs`, `algebra_extended.rs`, and
`tests/physical.rs` assertions accordingly. Guardrail: preserve
`ProjectRequirement`'s module-identity vs call-result-identity distinction that
drives `needs_module_identities`/`needs_project_overlay` in `projection.rs`.

### [api::compiler::physical]

#### [ ] READ-005 — Physical ObjectSlot is written in the compiled plan but never read by production execution

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/physical.rs:41-90`, `physical.rs:463-476`; `glass-lint-core/src/api/compiler/physical/planner.rs:80-100`

`PhysicalRoot::ReturnedSubject`/`InstanceSubject` carry a second
`physical::ObjectSlot` (a newtype distinct from `normalized::ObjectSlot`,
physically aliased as `NormalizedObjectSlot` at `physical.rs:9`). Every
production execution site destructures these roots with `..` and matches purely
on producer/constructor + member
(`analysis/matching/query/mod.rs:69-93`, `matching/arguments/mod.rs:64`), and
`reference.rs` also ignores it. The slot's only production effects are the
sentinel rejection inside `TryFrom` (`physical.rs:75-84`, needed so `u32::MAX`
never enters the plan) and test-only text in `explain_root`
(`physical.rs:463-476`). This is the one element of the physical-planning stack
with no runtime consumer, which makes the physical `ObjectSlot` newtype plus its
`TryFrom` boundary look over-built for the plan IR.

**Recommendation:** Keep the sentinel rejection in the `returned_subject`/
`instance_subject` constructors, but either drop the `object_slot` field from
the compiled root variants (the slot stays in `normalized::ObjectSlot`, where it
is genuinely consumed by alpha-renumbering) or add a comment on
`PhysicalRoot` explaining why the plan IR must retain an artifact-local slot
that execution ignores. Deletion target: `physical::ObjectSlot`,
`TryFrom`, and the `Display` impl if the field is dropped, adjusting
`explain_root` and `tests/physical.rs:393` accordingly. Guardrail: never expose
the slot value, and keep the plan's `PartialOrd/Ord` determinism independent of
it.

## Systemic Themes

- **Two-vocabulary duplication:** both `RequirementMode`/`CompletionMode`
  (READ-001) and the emptiness policies (READ-002) maintain a second copy of a
  concept whose primary owner already exists nearby. Each carries an explicit
  "must not diverge" contract, which is the maintenance smell to look for.
- **Parallel capability sets:** `PlanRequirements` (READ-004) store one
  capability across two dimensions that every `require_identity` arm must
  update together, with an OR predicate needed to read it back — the same
  "keep both in lockstep" pattern.
- **Consolidated pipeline, stale names:** the validation passes were merged
  from ten to three but the module filenames still encode the old numbering
  (READ-003).
- **Retained-but-unread compiler IR:** physical roots keep a slot field only
  tests print (READ-005); the "private IR" rule is respected, but retained
  fields invite future dependents.

## Open Questions

- Is the `physical::ObjectSlot` deliberately reserved for an upcoming
  subject-correlation feature (the planner budget and object-slot plumbing
  exist end to end)? If so, a doc comment stating that intent would settle
  READ-005 without code change.
- Should `FlowRequirements`'s two bools stay a struct once `PlanRequirements`
  is reworked in READ-004, or are they only an aggregate for `summary`/`explain`
  output?
- Do the `pass1_3`/`pass4_10` filenames appear in any external documentation or
  chunk manifests that a rename in READ-003 would need to update in the same
  change?

## Coverage

Files read: `api/compiler/{mod.rs, object_flow.rs, physical.rs,
physical/planner.rs, physical/validation.rs, requirements.rs, rule.rs,
rule/tests.rs, catalog.rs, limits.rs, error.rs, normalized.rs, reference.rs,
validate/{mod.rs, error.rs, pass1_3.rs, pass4_10.rs}}`. Consumers traced for
planner budget, requirements computation, and validation ordering:
`analysis/flow/planning.rs`, `analysis/flow/cross/{mod.rs, propagation.rs,
evidence.rs}`, `analysis/flow/projector/evidence.rs`, `analysis/matching/query/mod.rs`,
`analysis/matching/arguments/mod.rs`, `analysis/project/projection.rs`,
`analysis/model/flow.rs`, `lint/{linter.rs, catalog.rs, selection.rs}`,
and `api/compiler/tests/{physical.rs, physical_extended.rs, reference.rs,
validate.rs}`.

Checked and left clean: `RuleSelectionError`/`CompiledRuleSelection`
(constructor-validated typed window with structured errors);
`CompiledObjectSource`/`Sink`/`Requirement` vs `NormalizedLifecycleEvent`/
`Sink` (a genuine lowering with a single direction of conversion);
`ScopeTypes` (private, cohesive owner of the scope/type walk); `EvidenceScope`
(small, legitimate walk state); `RootBudget` (simple bounded reservation,
correctly owned by the planner); and `PresentIndices` (single-consumer
bounded iterator).