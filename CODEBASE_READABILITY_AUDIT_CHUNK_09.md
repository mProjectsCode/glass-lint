# Codebase Readability Audit — Chunk 9

This audit covers Chunk 9 of `CODEBASE_STRUCTURE_CORE.md`: query
classification and compilation. It is an architectural review only; no source
changes were made.

## Summary

The compiler has a sound high-level contract: validated declarations are
normalized into deterministic roots, physical preparation requirements are
attached to executable plans, and classification evidence remains bounded and
opaque to the report schema. The main readability risks are in the transitions
between those owners. Catalog-wide and per-query compilation are coordinated
by one mutable procedure, capability requirements contain an unproducible
cross-file state, validation is repeated at several physical boundaries, raw
slot integers cross IR layers, structured errors become strings too early, and
the evidence table repeatedly reimplements its index boundary.

## Findings

#### [x] READ-001 — `compile_queries` mixes per-query compilation with catalog aggregation

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:213-235`
- **Representative callers:** `CompiledMatcherPlan::compile` calls `compile_queries` once per rule; the function validates, normalizes, physically plans, accumulates roots and requirements, optimizes the aggregate, and seals the final `PhysicalPlan`

`compile_queries` runs two different lifecycles in one mutable protocol. For
each query it performs validation, normalization, physical planning, and
requirement extraction; across all queries it then merges roots, optimizes the
root set, and checks the aggregate requirements. The same function also
chooses the error conversion boundary for validation and physical-plan
failures.

The distinctions matter: a single query can be normalized independently, but
root deduplication and preparation requirements are catalog-wide. Keeping
both levels in one function makes it unclear which invariants belong to a
query plan versus the final rule plan, and a new per-query phase can
accidentally mutate aggregate state before its result is complete.

**Recommendation:** Give one private per-query compilation transition the
responsibility for validation, normalization, and root planning, and let a
catalog/rule-plan accumulator own root optimization, requirement merging, and
final sealing. Keep error translation at the outer boundary, where the rule ID
is available. Preserve declaration order before canonical optimization,
cross-query root deduplication, exact preparation requirements, deterministic
plans, and the existing fail-closed errors.

**Fix Applied:** Query-local validation, normalization, and physical planning
now live in `compile_query`. The private `QueryPlanAccumulator` owns only
cross-query root merging, requirement aggregation, optimization, and final
physical-plan sealing. Verified with `make fmt && make ci`.

#### [x] READ-002 — Flow requirements expose a cross-file state that no physical root can produce

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/api/compiler/requirements.rs:15-43,150-191`; physical producers at `glass-lint-core/src/api/compiler/physical.rs:150-170`; projection consumer at `glass-lint-core/src/analysis/project/projection.rs:173-214,485-495`
- **Representative callers:** `ProjectionPlan::from_selection` copies `FlowRequirements::cross_file`, and `project_with_arena` branches on it, but compiler physical roots only call `require_local_flow` and `require_cross_call_flow`

`FlowRequirements` carries `local`, `cross_call`, and `cross_file` flags and
has a three-boolean constructor, but `PlanRequirements` has no
`require_cross_file_flow` operation. The only production physical flow root
(`PhysicalRoot::Lifecycle`) sets local and cross-call flow; no compiler path
sets `cross_file`. The flag is therefore always false when it reaches the
project projection branch, even though downstream code treats it as a
meaningful capability.

This leaves an impossible state in the internal contract and makes it unclear
whether cross-file work is intentionally covered by cross-call flow or an
unfinished physical capability. A future planner author could set the flag in
one layer without knowing which project phase is supposed to consume it.

**Recommendation:** Make the capability owner explicit: either remove the
unproduced `cross_file` flag and its downstream branch if cross-file flow is
covered by the existing cross-call requirement, or add a named producer and
validation path for a genuinely distinct cross-file operator. Avoid a raw
three-boolean constructor; use named requirement transitions or a typed
capability set. Preserve lazy projection, shared flow limits, cross-call
semantics, and deterministic operation accounting.

**Fix Applied:** Removed the unproducible `cross_file` flow capability and
its raw three-flag constructor. Cross-file projection remains owned by the
existing cross-call collector, and projection now consumes only the local and
cross-call requirements that physical roots can produce. Verified with
`make fmt && make ci`.

#### [x] READ-003 — Physical roots are validated and their requirements recomputed at multiple boundaries

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Encapsulation
- **Location:** `glass-lint-core/src/api/compiler/physical.rs:88-147,173-267,278-297,540-558`
- **Representative callers:** every root constructor calls `validated`; `PhysicalPlan::from_roots` calls `validate_physical_plan`, which validates every root again and recomputes requirements before `PhysicalPlan::try_new` compares them with caller-supplied requirements

Each physical-root constructor validates its newly created root. The plan
constructor then validates all roots again, derives requirements again, and
`try_new` compares that derived value with a second requirements object built
by `compile_queries`. The defense is useful for detecting malformed internal
state, but the same root invariants and capability mapping are traversed at
several lifecycle boundaries.

The repeated checks make the ownership contract less clear: callers cannot
tell whether a root is guaranteed valid after construction or whether the
plan is expected to repair/validate it, and a new root variant requires
coordinated updates to constructors, root validation, requirement derivation,
and plan validation.

**Recommendation:** Choose one production sealing transition for root-local
invariants and one plan-level transition for collection-wide invariants and
requirements. Retain a test-only malformed-plan constructor or explicit
validation test if defensive coverage is needed, but remove the normal-path
double validation and caller-supplied duplicate requirement object. Preserve
root-specific dimension checks, canonical constraints, requirement mismatch
detection, and bounded fail-closed plan construction.

**Fix Applied:** Query compilation now returns roots without constructing a
per-query `PhysicalPlan`; `PhysicalPlan::from_roots` is the sole production
sealer that validates the optimized aggregate and derives requirements. The
caller-supplied requirement comparison remains test-only for malformed-plan
coverage, and object-slot admission remains validated at construction.
Verified with `make fmt && make ci`.

#### [x] READ-004 — Normalized and physical IR pass raw slot integers through an unused remapping API

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype / Conversion
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:161-205`; slot remapping at `glass-lint-core/src/api/compiler/normalize.rs:143-205`; physical slot boundary at `glass-lint-core/src/api/compiler/physical.rs:53-80,115-140`
- **Representative callers:** `normalize_query_decl` invokes `alpha_renumber_slots(&mut root)` and discards its returned `BTreeMap`; normalized events/subjects store `u32`, while physical subject roots wrap those values in `ObjectSlot`

Variable and object slots move from declaration `VarId` values to raw `u32`
fields in `NormalizedEvent` and `NormalizedSubject`, then are wrapped again
only for physical object roots. The alpha-renumbering function returns the
old-to-new map even though its only production caller ignores it. Slot
collection, remapping, and physical construction consequently communicate
through primitive integers and a return value with no consumer.

The compiler correctly needs deterministic alpha-renumbering, and event slots
and object slots may remain distinct semantic domains. The current API hides
that distinction and makes it easy for a future operator to use an authored
slot, a normalized slot, or an object slot interchangeably.

**Recommendation:** Introduce private compiler-owned slot types with explicit
conversion at the declaration-to-normalized and normalized-to-physical
boundaries, keeping event-variable and object-slot meanings distinct. Make
alpha-renumbering return `()` unless a named remapping value has a real
consumer, and centralize slot rewriting on the normalized tree owner. Preserve
dense deterministic slots, branch correlation, physical object identity, and
the prohibition on exposing artifact-local IDs.

**Fix Applied:** Normalized IR now uses distinct private `EventSlot` and
`ObjectSlot` types, and physical lowering performs an explicit object-slot
conversion. Alpha-renumbering includes both slot domains, centralizes the
rewriting on the normalized tree, and no longer returns its unused map.
Added regression coverage for dense returned-object slots. Verified with
`make fmt && make ci`.

#### [x] READ-005 — Compiler error structure is flattened before the catalog boundary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Conversion
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:213-250`; error definitions at `glass-lint-core/src/api/compiler/validate/error.rs:11-63` and `glass-lint-core/src/api/compiler/error.rs:5-19`; public mapping at `glass-lint-core/src/api/rule/error.rs:31-55`
- **Representative callers:** `map_query_compile_error` converts most structured `QueryCompileError` variants to a string-backed `QueryDiagnostic`, while physical planning maps every `PhysicalPlanValidationError` to `InvalidPhysicalPlan(String)`

The compiler error enums retain structured variables, contradiction kinds,
relation details, and physical validation categories, but the outer
`MatcherBuildError` keeps only strings for compiler invariants, physical-plan
failures, and most query diagnostics. `compile_queries` calls `to_string()` at
both physical-plan boundaries, and the catalog compiler later carries those
messages through another string-backed error variant.

This makes the public construction API unable to distinguish or inspect
stable failure dimensions without parsing display text. It also means adding a
structured diagnostic field requires changing several mapping arms and can
silently alter user-facing text that currently serves as the only data.

**Recommendation:** Preserve typed compiler errors through the internal and
catalog layers, with a single display conversion at the external boundary.
Keep authored-query diagnostics distinct from internal invariant failures and
physical-plan validation, and add a stable diagnostic projection for callers
that need codes/messages. Preserve existing display wording, rule-ID context,
deterministic error selection, and the distinction between invalid input and
compiler bugs.

**Fix Applied:** `MatcherBuildError` and `CompiledCatalogError` now retain
typed `CompilerInvariantDiagnostic` and `PhysicalPlanDiagnostic` values through
compiler and catalog layers. Authored query failures remain structured
`QueryDiagnostic` values, while provider catalog conversion performs the final
string projection at the external boundary; existing display wording and
rule-ID context are preserved. Verified with `make fmt && make ci`.

#### [x] READ-006 — `RuleEvidenceTable` repeats its rule-index boundary in every mutation method

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Encapsulation
- **Location:** `glass-lint-core/src/api/classification.rs:202-326`
- **Representative callers:** `record`, `extend`, `record_grouped`, `mark_event_truncated`, and `replace` each look up `rule.get()` in the backing `Vec` and construct `RuleOutOfRange` independently

The evidence table is a bounded domain collection indexed by an opaque
`RuleIndex`, but its mutation methods all expose the same storage protocol:
convert the index to `usize`, call `get_mut`, and build the same out-of-range
error. `record_grouped` adds a second empty-check/constructor protocol, while
`merge` re-enters the public mutation path using a newly reconstructed index.

The backing vector is private, yet the invariant that every mutation targets
one catalog slot is still implemented repeatedly. New mutation operations can
forget the bounds check, and callers must reason about raw capacity errors
instead of a table-owned slot transition.

**Recommendation:** Add one private table-owned entry operation or bounded
slot view that validates a `RuleIndex` once and supplies the selected evidence
bucket to focused mutation methods. Keep `RuleIndex` opaque, preserve
capacity-mismatch errors when merging tables, retain deterministic rule order,
and keep empty evidence distinct from an invalid rule index.

**Fix Applied:** Already addressed by `55c49d3 fix read cross-004 chunk 13`,
which added the private `items_mut` slot operation and routed all mutating
methods through it. The finding predates that fix and is stale in this audit
chunk; no duplicate source change is needed.

#### [ ] READ-007 — Query validation carries scope, type, correlation, and mode state through raw collections

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/api/compiler/validate/pass1_3.rs:57-256`; correlation mode at `glass-lint-core/src/api/compiler/validate/pass4_10.rs:157-241`
- **Representative callers:** `validate_query_decl` runs the consolidated passes; `collect_scope_and_types` threads `Vec<VarId>` and `HashMap<VarId, VarType>`, while `check_correlation_evidence` changes behavior through a `check_evidence: bool` flag

The consolidated validation passes avoid repeated AST walks, but their state
protocol is difficult to read. Scope collection and type inference share raw
mutable collections whose meaning changes between `All` and `Any` branches;
the `Any` path builds temporary maps and merges types manually. The later
correlation/evidence traversal uses a boolean mode to distinguish top-level
primary-variable checking from nested branch traversal, so the same function
has materially different contracts depending on a flag.

These are semantic states, not incidental implementation details: branch-local
bindings must not leak across alternatives, compatible type refinements must
survive a join, and correlated `All` branches must retain path-local identity.
A new variable or relation kind therefore requires updating several raw map
merges and boolean branches without a single validation-context owner.

**Recommendation:** Keep the consolidated traversal strategy, but encapsulate
branch scope/type state in a private validation context and replace the raw
boolean with a named validation mode or phase value. Give that context
operations for binding, compatible refinement, branch join, and primary-event
availability. Preserve independent `Any` branch scopes, same-event
correlation, type-compatibility rules, bounded work, and the existing
diagnostic precedence.

**Fix Applied:** None so far.

## Systemic Themes

- **ENCAPSULATE:** Compiler slots, physical requirements, typed errors, and
  rule-index access should be owned by domain transitions rather than raw
  integers, strings, or repeated vector lookups.
- **SIMPLIFY:** Compilation and validation coordinate multiple semantic levels
  through one procedure, mutable collections, and boolean modes.
- **DEDUPLICATE:** Root validation/requirement derivation and evidence-table
  index checks are repeated at adjacent boundaries and mutation methods.

## Open Questions

None recorded.

## Coverage

Reviewed classification evidence and bounded rule-index storage, compiler
entry points, query validation passes, normalized IR, alpha-renumbering,
same-event and `Any` normalization, lifecycle/object-flow lowering, physical
roots and requirements, physical-plan validation/optimization, compiler error
mapping, compiled rule selection, and the test-only logical/physical reference
oracle. The reference oracle was not reported as duplication because its
independent evaluator is an intentional semantic equivalence check.

Validation ownership is split by invariant scope: each constructor validates
its local root or declaration, while the consuming phase transition owns
cross-root consistency and requirement derivation. `PhysicalPlan::from_roots`
may remain the single production sealing transition; malformed private IR
tests can call lower constructors directly to exercise their local errors.
This removes duplicate production validation without weakening tests or
exposing compiler storage to providers.

## Handoff

Chunk 9 is complete. The next unreviewed chunk is **Chunk 10 — Configuration,
parsing, and runtime environment** (`CODEBASE_STRUCTURE_CORE.md` lines
691-741), covering Core configuration, parser inputs, environment/global
configuration, analysis limits, and runtime setup.
