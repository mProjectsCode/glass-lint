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

Three medium and two low findings. The two strongest root causes are (1) a fully
duplicated mode vocabulary (`RequirementMode`/`CompletionMode` vs the model's
`RequirementReadiness`/`SinkReadiness`) that is already lowered one-to-one —
chunk 11 READ-001 covers the same duplication from the model side, and both
chunks point at the identical fix — and (2) `PlanRequirements` maintaining two
parallel capability sets where the `value_resolution` dimension has no
production reader and must be kept in lockstep with `project` by every
`require_identity` arm. The remaining findings are a duplicated emptiness
policy that is explicitly required not to diverge, stale validation-pass module
naming that no longer matches the consolidated three-pass pipeline, and a
physical `ObjectSlot` retained in compiled roots that production execution
never reads.

## Findings

### [api::compiler::object_flow / analysis::model::flow]

#### [x] READ-001 — RequirementMode/CompletionMode duplicate the model's RequirementReadiness/SinkReadiness

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/object_flow.rs:21-32,44-59`; `glass-lint-core/src/analysis/model/flow.rs:160-197`

`RequirementMode { AllRequired, AnyRequired }` and
`CompletionMode { Configuration, AnySink, AllSinks }` (`object_flow.rs:21-32`)
are 1:1 mirrors of the model-level `RequirementReadiness { Any, All }` and
`SinkReadiness { Configuration, Any, All }` (`model/flow.rs:160-171`), and
`CompiledObjectFlow::readiness` (`object_flow.rs:44-59`) maps them one-to-one
into `FlowReadiness`. The two compiler enums buy nothing over the model
vocabulary: analysis consumers reach into the compiler IR for the raw mode
anyway (`analysis/flow/cross/propagation.rs:200`,
`analysis/flow/projector/evidence.rs:187` both compare
`completion_mode() == CompletionMode::Configuration`, importing the compiler
enum at `propagation.rs:20` and `evidence.rs:26`), so a distinct
"compile-time" vocabulary is not actually enforced. Every mode addition must be
mirrored in both enums plus the mapping plus the model.

**Recommendation:** Make `model::flow::{RequirementReadiness, SinkReadiness}` the
sole owner — the direction chunk 11 READ-001 agrees on. Delete the two compiler
enums; have `from_normalized_lifecycle` (`object_flow.rs:119-177`) build the
model enums directly and store them on `CompiledObjectFlow` in place of the
`requirement_mode`/`completion_mode` fields; drop the `completion_mode()`
accessor (`object_flow.rs:89-91`) in favor of a `sink_readiness()` one; and
replace the two analysis comparisons with
`sink_readiness() == SinkReadiness::Configuration`. `readiness()` then collapses
to the pass-through `FlowReadiness::new(requirement_mode, requirement_count,
sink_mode, sink_count)` pairing each stored enum with its count, keeping the
existing default for an absent condition (`Any` with zero count) unchanged.
Guardrail: preserve the `Configuration` = "requirement-set completion anchors on
the requirement event" versus `Any` = "completion anchors on the first recorded
sink" distinction through the existing `Option<NormalizedLifecycleCompletion>`
source (`object_flow.rs:142-143`), so an explicit `Configuration` remains
distinct from a zero-count `AnySink`; keep `sinks_ready`'s trivial-true arm for
`Configuration`/`Any` (`model/flow.rs:426`) intact.

**Fix Applied:** Resolved by the shared implementation in commit `7ea7ff4a`
(`fix chunk 11 read 001`): removed the compiler-only mode mirrors, stored the
model readiness enums directly on `CompiledObjectFlow`, and updated analysis
callers to use `SinkReadiness`. The configuration-versus-sink distinction and
readiness behavior remain unchanged; verified with `make fmt && make ci`.

#### [x] READ-002 — Two lockstepped emptiness policies: IdentityConstraint::is_empty vs is_identity_empty

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
`as_str`/`path.is_empty` decision is duplicated, and the only difference
between the two match arms is the variant spelling (`Any`/`Global` on the IR
side vs `Heuristic`/`Global` on the declaration side).

**Recommendation:** Extract narrow component helpers at a single compiler-owned
location (e.g. `fn name_empty(name: &str) -> bool`,
`fn module_export_empty(module: &str, export: &str) -> bool`,
`fn module_specifier_empty(pattern: &ModuleSpecifierPattern) -> bool`, and the
rooted-path/literal/pass-through cases) and have both `is_empty`
implementations delegate so the policy text exists once; the two match arms
collapse to their vocabulary mappings. Guardrail: keep `Rooted`/
`PrivateNetworkAddress` never-empty semantics and the `trim()`/`as_str()`
exactness identical, and add a test asserting
`IdentityConstraint::from(spec).is_empty() == is_identity_empty(spec)` for each
variant — the `From<&IdentitySpec>` lowering at `mod.rs:151-180` makes the
parity contract directly testable on the same `spec`.

**Fix Applied:** Centralized trimmed text and module/export emptiness helpers
in the compiler module and made both identity policies delegate to them.
Added a parity test covering every identity variant, including the distinct
`Any`/`Heuristic` vocabulary. Rooted and private-network identities remain
non-empty. Verified with the focused compiler test and clippy; `make fmt` and
`make ci` follow.

### [api::compiler::validate]

#### [x] READ-003 — Stale pass numbering hides the consolidated three-pass pipeline and the orchestrator lives in the wrong module

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/validate/pass1_3.rs:55-67`, `validate/pass4_10.rs:28-52`, `validate/mod.rs:1-13`

The module names `pass1_3` and `pass4_10` encode the retired ten-pass scheme:
the consolidated entry-point doc (`pass4_10.rs:28-38`) lists the three combined
traversals (structure, scope+types, correlation+evidence), yet the filenames
still reflect passes 1-3 and 4-10. The orchestration entry `validate_query_decl`
(`pass4_10.rs:39-46`) lives inside "pass4_10" while importing `pass_scope_types`
from "pass1_3" through a function-local `use super::pass1_3`
(`pass4_10.rs:40`), and the structural validator `validate_event_query`
(defined in `pass1_3.rs:9`, consumed only by `pass4_10`'s `pass_structure` at
`pass4_10.rs:60` and `validate_lifecycle` at `pass4_10.rs:14`) lives in the file
named for the scope/types pass. Inline comments still name retired passes as
well (`pass4_10.rs:61` `// pass_boundedness`; `pass4_10.rs:48-52` recounts the
five former structure passes). The name boundary no longer matches the
ownership boundary.

**Recommendation:** Rename the modules to the consolidated pass names —
`structure.rs`, `scope_types.rs`, `correlation_evidence.rs` — moving
`pass_structure`, `check_structure`, `check_require_structure`,
`validate_lifecycle`, and `validate_event_query` into `structure.rs`;
`pass_scope_types` and `ScopeTypes` into `scope_types.rs`;
`pass_correlation_evidence`/`check_correlation_evidence`/
`validate_correlated_branches`/`EvidenceScope` into `correlation_evidence.rs`;
and move the `validate_query_decl` orchestration into `validate/mod.rs`, which
then owns pass ordering as its single responsibility. Guardrail: keep the
current pass order (structure before scope/types before correlation/evidence)
and the `#[cfg(test)]` re-exports (`pass_scope_types`, `pass_structure`,
`pass_correlation_evidence` at `mod.rs:9-13`) identical, so `tests/validate/*`
(`correlation.rs`, `identity.rs`, `well_formedness.rs`) needs no behavioral
change.

**Fix Applied:** Renamed the validation modules to `structure.rs`,
`scope_types.rs`, and `correlation_evidence.rs`, moved the three-pass
orchestration into `validate/mod.rs`, and preserved the existing test helper
re-exports and pass order. Verified with validation-focused tests, clippy,
`make fmt`, and `make ci`.

### [api::compiler::requirements]

#### [ ] READ-004 — PlanRequirements: parallel capability sets where value_resolution is unread in production and require_identity must update both in lockstep

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/compiler/requirements.rs:69-179`; consumers `glass-lint-core/src/analysis/project/projection.rs:145-148`, `glass-lint-core/src/api/compiler/physical.rs:346-408`

`PlanRequirements` holds two capability collections — `value_resolution`
(`ValueResolutionRequirement`) and `project` (`ProjectRequirement`) — together
with `FlowRequirements`. In production, `value_resolution` is read only through
the cross-set OR in `needs_call_result_identities` (`requirements.rs:156-163`);
`LocalStaticValues` and `ModuleIdentityValues` have no production reader (their
only appearances outside tests are the `#[cfg(test)]` `summary`/`explain`
prints at `physical.rs:385,401-402`), and the `value_resolution` copy of
`CallResultIdentities` is always inserted together with
`ProjectRequirement::CallResultIdentities` (`requirements.rs:102-109,112-119`),
so the OR leg never fires alone. Meanwhile `require_identity`
(`requirements.rs:99-135`) must insert the same capability into both sets in
lockstep. Any future capability arm that forgets one set silently mis-gates the
project phase, and the OR predicate is required merely to paper over the split.
The runtime gates in `projection.rs:145-148` consult only the `project`
dimension plus the OR.

**Recommendation:** Make the capability a single owned dimension: keep `project`
as the only capability set and derive `needs_call_result_identities` from it.
Delete the `value_resolution` dimension entirely —
`ValueResolutionRequirement` (all three variants), `require_local_static_values`
(`requirements.rs:91-95`) and the `value_resolution` extensions in every
`require_identity` arm, the `#[cfg(test)]` `value_resolution()` accessor
(`requirements.rs:77-80`), the `value_resolution` merge in `merge_from`
(`requirements.rs:172-174`), and the `value_resolution` leg of the OR in
`needs_call_result_identities` (`requirements.rs:156-163`). The
`require_local_static_values` call in the `ConstrainedScan` arm
(`physical.rs:156`) goes with it: argument matching resolves static values from
the `ValueMatcher` itself (`matching/arguments/evaluator.rs:136-146,261`), not
from a plan gate, so no capability is lost. The executor gates in
`projection.rs:145-170` already consume the `project` dimension
(`needs_module_identities`/`needs_project_overlay`/`needs_call_result_identities`)
and stay unchanged. Deletion target: the above, plus updating
`tests/normalize/algebra.rs:187,358,373-374`,
`tests/normalize/algebra_extended.rs:48`, and the `value_resolution=` segments
of the summary assertions at `tests/physical.rs:298,347`. Guardrail: preserve
`ProjectRequirement`'s module-identity vs call-result-identity distinction that
drives `needs_module_identities`/`needs_project_overlay` in `projection.rs`.

### [api::compiler::physical]

#### [ ] READ-005 — Physical ObjectSlot is written in the compiled plan but never read by production execution

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/physical.rs:41-90`, `physical.rs:459-477`; `glass-lint-core/src/api/compiler/physical/planner.rs:79-100`

`PhysicalRoot::ReturnedSubject`/`InstanceSubject` carry a second
`physical::ObjectSlot` (a newtype distinct from `normalized::ObjectSlot`,
aliased as `NormalizedObjectSlot` at `physical.rs:9`). Every production site
that consumes these roots destructures them with `..` and matches purely on
producer/constructor + member: the matcher at
`analysis/matching/query/mod.rs:69-93`, the requirement merge at
`physical.rs:159-162`, the plan validation at `physical.rs:202-245`
(`object_slot: _`), and the reference executor at
`reference.rs:395-397,410-413`. The slot's only production effects are the
sentinel rejection inside `TryFrom` (`physical.rs:75-84`, which keeps `u32::MAX`
from ever entering the plan) and test-only text in `explain_root`
(`physical.rs:459-477`). This is the one element of the physical-planning stack
with no runtime consumer, which makes the physical `ObjectSlot` newtype plus its
`TryFrom` boundary look over-built for the plan IR.

**Recommendation:** Drop the `object_slot` field from the compiled root variants,
keeping the sentinel rejection in the `returned_subject`/`instance_subject`
constructors (`physical.rs:119-147`): they still take the `NormalizedObjectSlot`
parameter and reject `u32::MAX` with `ImpossibleDimensions`, but no longer
convert-and-store a private newtype. Delete `physical::ObjectSlot`, the
`TryFrom`, and the `Display` impl; adjust `explain_root` (`physical.rs:459-477`)
and the sentinel test (`tests/physical.rs:393-411`); and drop the slot plumbing
from the planner arms (`planner.rs:79-100`). The slot stays in
`normalized::ObjectSlot`, where alpha-renumbering genuinely consumes it
(`normalized.rs:360-366,395-439`; `normalize_all.rs:272-274`). Alternatively,
if the plan must retain an artifact-local slot, add a comment on `PhysicalRoot`
explaining why execution ignores it. Guardrail: keep the fail-closed `u32::MAX`
rejection and the plan's `PartialOrd/Ord` determinism (the remaining root fields
fully determine a root); never expose the slot value.

## Systemic Themes

- **Two-vocabulary duplication:** both `RequirementMode`/`CompletionMode`
  (`object_flow.rs:21-32`, READ-001) and the emptiness policies (`mod.rs:105-127`
  vs `validate/error.rs:374-396`, READ-002) maintain a second copy of a concept
  whose primary owner already exists nearby. Each carries an explicit "must not
  diverge" contract (`mod.rs:106-109`, `error.rs:374-380`), which is the
  maintenance smell to look for.
- **Parallel capability sets:** `PlanRequirements` (`requirements.rs:69-179`,
  READ-004) store one capability across two dimensions that every
  `require_identity` arm must update together, with an OR predicate
  (`requirements.rs:156-163`) needed to read it back — the same "keep both in
  lockstep" pattern.
- **Consolidated pipeline, stale names:** the validation passes were merged
  from ten to three (`pass4_10.rs:28-38`) but the module filenames still encode
  the old numbering (READ-003).
- **Retained-but-unread compiler IR:** physical roots keep a slot field only
  tests print (`physical.rs:459-477`, READ-005); the "private IR" rule is
  respected, but retained fields invite future dependents.

## Open Questions — Resolved

1. **`physical::ObjectSlot` is not reserved for a future subject-correlation
   feature.** No doc comment, TODO, or planning note in the compiler or flow
   modules states such intent. The normalized `ObjectSlot` is genuinely
   consumed: `collect_slots`/`remap_slots`/`alpha_renumber_slots` read and
   rewrite it to a dense 0..n range (`normalized.rs:360-366,395-439`), and
   `normalize_all.rs:272-274` compares `ObjectSlot::from_var` during
   normalization. The physical copy is pure overhead: no production reader
   exists (`matching/query/mod.rs:69-93`, `physical.rs:159-162,202-245`,
   `reference.rs:395-397,410-413` all ignore it), and the `u32::MAX` sentinel
   rejection (`physical.rs:78-84`) is defensive only — after alpha-renumbering
   slots are dense and bounded by `MAX_PHYSICAL_ROOTS_PER_RULE` (256,
   `limits.rs:2`), so `u32::MAX` cannot occur in production; only the test at
   `tests/physical.rs:393-411` injects it. READ-005's drop-the-field option is
   therefore safe without a reservation comment.
2. **`FlowRequirements` collapses to a single `needs_flow` capability.** The two
   bools are only ever set together — the sole producer is the `Lifecycle` root,
   which calls both `require_local_flow` and `require_cross_call_flow`
   (`physical.rs:163-165`; no other production caller exists) — and production
   reads them only as the aggregate `local || cross_call` at `projection.rs:371`.
   The per-kind split is exercised only by the `#[cfg(test)]` `summary`/`explain`
   output (`physical.rs:370-375,403-404`) and unit tests. Once READ-004 makes
   capabilities single-owned, the struct can be a single `needs_flow` bool (or
   gain a `needs_flow()` method); keeping the two-bool struct is justified only
   if a root ever needs local-only or cross-call-only flow.
3. **The `pass1_3`/`pass4_10` filenames do appear in other documents.** A
   READ-003 rename must update, in the same change:
   `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_18.md` (lines 13, 133, 299),
   `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_20.md` (lines 71, 74-75, 82, 99, 115,
   119, 230, 429), and `CODEBASE_STRUCTURE_CORE.md` (lines 753-756, which name
   `validate::pass1_3`, `validate::pass1_3::ScopeTypes`, `validate::pass4_10`,
   and `validate::pass4_10::EvidenceScope`).

## Coverage

Files read: `api/compiler/{mod.rs, object_flow.rs, physical.rs,
physical/planner.rs, physical/validation.rs, requirements.rs, rule.rs,
rule/tests.rs, catalog.rs, limits.rs, error.rs, normalized.rs, normalize.rs,
normalize_all.rs, reference.rs, validate/{mod.rs, error.rs, pass1_3.rs,
pass4_10.rs}}`. Consumers traced for planner budget, requirements computation,
and validation ordering: `analysis/flow/planning.rs`,
`analysis/flow/cross/{mod.rs, propagation.rs, evidence.rs}`,
`analysis/flow/projector/evidence.rs`, `analysis/matching/query/mod.rs`,
`analysis/matching/arguments/{mod.rs, evaluator.rs}`,
`analysis/project/projection.rs`, `analysis/model/flow.rs`,
`lint/{linter.rs, catalog.rs, selection.rs}`,
`api/compiler/tests/{physical.rs, physical_extended.rs, reference.rs,
validate.rs}`, `api/compiler/tests/normalize/{algebra.rs, algebra_extended.rs,
canonical.rs}`, and `api/compiler/tests/validate/{correlation.rs, identity.rs,
well_formedness.rs}`.

Checked and left clean: `RuleSelectionError`/`CompiledRuleSelection`
(constructor-validated typed window with structured errors);
`CompiledObjectSource`/`Sink`/`Requirement` vs `NormalizedLifecycleEvent`/
`Sink` (a genuine lowering with a single direction of conversion);
`ScopeTypes` (private, cohesive owner of the scope/type walk); `EvidenceScope`
(small, legitimate walk state); `RootBudget` (simple bounded reservation,
correctly owned by the planner); and `PresentIndices` (single-consumer
bounded iterator).
