# Codebase Readability Audit — Chunk 11

## Summary

Chunk 11 covers retained fact, flow-state, lifecycle-evidence, and module
interface model types. The model uses useful semantic newtypes for dense fact
and flow identities, a generic lifecycle evidence owner for local and
qualified events, and private indexed storage with deterministic ordering.

The main risks are unowned bounds and state transitions: lifecycle evidence
silently rejects indexes beyond its 64-bit mask, flow-limit scaling can
overflow before clamping, fact kind is passed separately from its payload, and
unknown module exports do not consistently stop all metadata writes. These
contracts can leave a model incomplete or apparently resolved without a
single explicit status transition.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Lifecycle evidence bounds

#### [x] READ-056 — Enforce the lifecycle evidence index bound at the owner

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Bounded state / API / completeness
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:238-345,374-421`,
  `analysis/api/compiler/object_flow.rs:40-73`,
  `analysis/flow/planning.rs:207-249`

`IndexedEvidence` represents requirement and sink readiness with a `u64` mask,
so `insert` returns `false` for any `RequirementIndex` or `SinkIndex` at 64 or
above. `FlowState` and cross-flow state expose that boolean, but most projector
and propagation callers ignore it; the compiled flow model and plan builder
also show no corresponding 64-entry validation. A flow with more than 64
requirements or sinks can therefore retain a syntactically valid compiled
flow whose later evidence can never become ready, without a typed invalid or
incomplete outcome identifying the rejected index.

Move the bound to the compiler/compiled-flow validation boundary, or replace
the mask with a bounded collection that reports exhaustion through the owning
flow-state transition. Delete ignored boolean failure paths after migration.
Preserve compact deterministic readiness for supported flows, conservative
fail-closed behavior for oversized flows, and separate possible witnesses
from definite completion.

**Fix Applied:** Added physical-plan validation for lifecycle requirement and
sink counts beyond the 64-entry indexed-evidence domain. Oversized roots now
fail with a typed `ExcessiveLifecycleEvidence` error carrying both counts,
before projector state can silently reject an out-of-range index; the compact
readiness mask remains unchanged for supported flows.

**Verification:** Added a physical-boundary regression; `cargo test -p glass-lint-core analysis::model::flow --lib` (16 passed); `make fmt && make ci` (passed).

### Flow-limit scaling

#### [x] READ-057 — Make flow-limit scaling overflow-safe

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API / Resource bounds / arithmetic
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:23-53`

`FlowLimits::from_flow_operations` scales several limits using expressions such
as `DEFAULT_OBJECTS * flow` before division. `flow_operations` is converted to
`u64` and is not narrowed or checked before the multiplication, so a very large
configured operation limit can overflow in debug builds or wrap in release
builds before the minimum floor is applied. The resulting limits can panic or
be much smaller than the requested bound, which undermines the model's
resource-exhaustion contract.

Use checked or saturating multiplication/division with an explicit maximum
representable limit, and validate the input at the configuration boundary.
Preserve the current minimum floors, local/project budget distinction, and
deterministic scaling for ordinary values without turning exhaustion into a
panic.

**Fix Applied:** Flow-limit scaling now uses one checked arithmetic helper that
converts the input safely, detects multiplication overflow, and clamps each
derived limit to its representable range while preserving the existing minimum
floors. Added a maximum-budget regression to ensure configuration cannot panic
or wrap into a smaller bound.

**Verification:** `cargo test -p glass-lint-core --lib analysis::model::flow`
and `make fmt && make ci` pass.

### Fact identity construction

#### [x] READ-058 — Derive fact kind from the payload owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Identity representation / test contract
- **Location:** `glass-lint-core/src/analysis/model/fact.rs:262-273,421-500`,
  `analysis/facts/stream.rs:204-225`,
  `analysis/facts/mod.rs:190-223`

`FactStream::try_push` and `SemanticFact::new` accept both a `FactKind` and a
`FactPayload`, even though the payload already determines the semantic kind.
In production the `kind` argument is discarded, while test builds retain a
second `SemanticFact.kind` field and filter test facts through it. The model
therefore has a duplicate tag whose test-only value can disagree with the
payload that all production consumers match, and every new payload variant
requires keeping a separate caller-supplied tag protocol synchronized.

Give `FactPayload` a single kind projection and remove the independent kind
argument/storage after migrating stream construction and test helpers. Keep
dense FactId assignment, payload-specific data, test filtering, and the
Building-to-Frozen validity boundary unchanged.

**Fix Applied:** Removed the caller-supplied `FactKind` parameter and
test-only duplicate field from `SemanticFact` and `FactStream`, then removed
the enum and rewrote tests to match `FactPayload` variants directly. Fact
builders now emit only spans and payloads, leaving semantic identity solely in
the payload owner.

**Verification:** `cargo test -p glass-lint-core analysis::facts --lib`
(32 passed), `cargo check -p glass-lint-core`, and `make fmt && make ci`
(passed).

### Module-interface uncertainty

#### [x] READ-059 — Make unknown export state terminal for all metadata setters

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** State transition / Fail-closed identity
- **Location:** `glass-lint-core/src/analysis/model/module.rs:214-279`,
  `analysis/facts/interface/commonjs.rs:54-129`

`ModuleInterface::mark_unknown_exports` clears resolutions and star exports
and sets `unknown_exports`. `add_export` and `add_star_export` respect that
terminal state, but `add_function_export` and `add_static_string` continue to
write entries afterward. Dynamic or ambiguous CommonJS handling can therefore
mark the export surface unknown and then repopulate function/static metadata
from later assignments; `function_export` or `static_string` can expose that
metadata even though `is_unknown()` says the interface is not trustworthy.

Make unknown state a single owner transition that rejects or clears every
export metadata write, or represent known/unknown exports as an explicit sum
type whose setters cannot mutate the known variant after invalidation. Preserve
local-name tracking, diagnostic retention, CommonJS ambiguity handling, and
the rule that unknown module identity cannot establish a cross-module witness.

**Fix Applied:** `ModuleInterface::add_function_export` and
`add_static_string` now reject writes after `mark_unknown_exports`, matching
the existing guards on regular and star exports. Added a regression covering
both cleared metadata and late metadata writes, preserving the fail-closed
unknown interface state.

**Verification:** `cargo test -p glass-lint-core --lib analysis::model::module`
and `make fmt && make ci` pass.

### Model wrapper shape

#### [x] READ-060 — Encode return-only control data in the control payload shape

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Newtype / Fact model / API
- **Location:** `glass-lint-core/src/analysis/model/fact.rs:275-294,467-475`,
  `analysis/facts/control.rs:1-90`

Every `FactPayload::Control` stores a `return_value`, although only
`ControlKind::Return` consumes it; branch, loop, switch, try, break, and
continue facts carry an irrelevant `ValueId::UNKNOWN`. Builders and consumers
must preserve that incidental field for all control variants, and the
relationship between `ControlKind` and `return_value` is enforced by caller
discipline rather than the type. Adding another control payload with data
would extend the same loosely coupled struct.

Split return data from non-return control data with a small control payload
sum type (or make the field a validated return variant) and delete the
placeholder-value protocol. Preserve control-region identity, canonical fact
ordering, return-value provenance, unsupported-control invalidation, and
fail-closed handling for malformed control facts.

**Fix Applied:** Split return facts into `FactPayload::Return { region, value }`
and removed the placeholder value from non-return `Control` payloads. Effect
collection reads the dedicated return value, while projector control transfer
continues to use `ControlKind::Return` for return control flow.

**Verification:** `cargo test -p glass-lint-core analysis::flow::effect --lib`
(16 passed), `cargo test -p glass-lint-core analysis::flow::projector --lib`
(52 passed), and `make fmt && make ci` (passed).

## Systemic Themes

Chunk 11's retained model types are mostly private behind phase and semantic
newtype boundaries, but several critical invariants remain implicit: the
maximum lifecycle index, arithmetic safety of resource scaling, the relation
between fact tags and payloads, terminal unknown-export state, and
return-specific control data. These should be owned by constructors or typed
state transitions so malformed or exhausted input becomes explicit incomplete
state rather than silent rejection or panic.

The generic `LifecycleEvidence<E>` is a strong reuse point for local and
qualified events; changes should retain its deterministic per-index event
ordering and avoid conflating local FactIds with qualified events. Existing
fact phase markers and module interface separation should remain intact.

Search signals used for this chunk included 64-bit evidence masks without
compiler validation, unchecked limit multiplication, duplicate fact tags,
metadata writes after unknown-export invalidation, and control payload fields
used by only one enum variant.

## Open Questions

- If lifecycle declarations remain capped at 64, the cap should be a named
  compiler validation error; otherwise the evidence owner needs a bounded
  alternative that can report exhaustion without silent `false` results.
- The fact-kind projection should remain available to tests without retaining
  a second mutable tag in production facts.
- The next unreviewed handoff is Chunk 12: retained scope, value, and request
  types.

## Coverage

Reviewed the Chunk 11 types listed in `CODEBASE_STRUCTURE_CORE.md` across
fact identities/payloads/phases, call arguments and parameter bindings,
flow limits/IDs/states/lifecycle evidence, and module requests/exports/
interfaces, with representative callers in fact construction, flow
projection, cross-flow propagation, CommonJS analysis, and project linking.
Existing Chunk 1–10 findings were checked to avoid re-reporting fact-stream
table pairing, summary/matcher effective arguments, projector history
ownership, matching overlays, and generic worklist admission. No findings are
marked applied.
