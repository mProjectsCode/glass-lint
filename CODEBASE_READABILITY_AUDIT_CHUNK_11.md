# Codebase Readability Audit — glass-lint-core Chunk 11: Retained model facts and flow

## Summary

Chunk 11 owns the retained semantic model: `analysis/model/fact.rs` (fact
identities, payloads, call events and argument views) and
`analysis/model/flow.rs` with `flow/limits.rs` and `flow/state.rs` (flow
identities, lifecycle evidence indexes, limits, and per-object flow state).
The code is well-factored overall: retained types hide representation behind
accessors (`CallEvent`, `ParameterBinding`, `FlowState`, `FlowReadiness`), the
`LifecycleEvidence`/`IndexedEvidence`/`EvidenceValues` stack is a genuine
domain collection with a documented sorted-mask invariant, and phase markers
(`Building`/`Frozen`) plus the `FactStreamToken` capability enforce the
freeze-ordering invariant at compile time.

Findings cluster around three themes: (1) a parallel builder/retained call
model (`ResolvedCallee` → `CallEvent`) that is hand-mapped through a 14-arg
positional constructor, (2) internal API surface inconsistency — visibility
tiers and index types are applied unevenly across sibling model types even
though only `FlowReadiness` is consumed outside `crate::analysis`, and (3)
evidence read paths that materialize or recompute derived data
(`requirement_entries`/`sink_entries` Vec clones; a fully constructed
`FlowLimits` discarded after reading back its pass-through field). No
production `unwrap`/`expect`/`panic` or `dead_code` allowances were found in
the chunk's own files; the guardrails around distinct local vs. cross-file
identity and fail-closed bounded analysis are respected and must be preserved.

## Findings

### Model fact types — call model and argument views

#### [ ] READ-001 — `CallEvent::resolved` is a 14-argument positional constructor fed by a hand-written mapping from the parallel `ResolvedCallee`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Conversion
- **Location:** `analysis/model/fact.rs:262-295`, `analysis/facts/calls/mod.rs:88-104`, `analysis/facts/calls/callee.rs:14-29`

`CallEvent` (14 fields) and the producer-side `ResolvedCallee` (12 fields,
`SymbolPath`-typed) are parallel models of the same resolved call, differing
only in interning (`SymbolPath` → `NamePath`) and the derived `args`/`unwrap`
slots. The single producer `emit_call` destructures `ResolvedCallee`
field-by-field and re-assembles `CallEvent` through the
`#[allow(clippy::too_many_arguments)]` 14-positional-argument `resolved()`
constructor (`analysis/facts/calls/mod.rs:88-104`). Every shared field
(`value`, `receiver`, `callee_span`, `callee_name`, `call_provenance`,
`syntactic_path`, `rooted_chain`, `module_member`, `returned_member`,
`instance_class`, `target_function`) is therefore declared three times — in
each struct and in the mapping — and a field addition or rename must be kept
in sync in all three places. Positional argument lists of this length also
invite silent field swaps (e.g., the two `Option<ValueId>` arguments).

**Recommendation:** Make `ResolvedCallee` (or a method on it, which already
owns the builder-phase vocabulary) responsible for lowering itself into the
retained `CallEvent`, taking the interning context plus the derived
`result`/`args`/`unwrap` inputs. This centralizes the mapping in one place,
removes the 14-arg constructor, and leaves `CallEvent` immutable-retained.
Guardrails: keep interning in the producer that holds the `Resolver`; do not
expose `CallEvent`'s storage or an interner from the model; keep the two
lifecycle phases (buildable vs. retained) distinct.

**Fix Applied:** None so far.

#### [ ] READ-002 — `ArgumentView` duplicates the argument-derived-data logic of `ArgumentData` and its prepared overlays only partially memoize

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `analysis/model/fact.rs:126-157`, `analysis/flow/matcher.rs:135-179`, `analysis/matching/arguments/evaluator.rs:227-248`

`ArgumentView` wraps a `CallArgInfo` plus three optional prepared overlays
(`static_string`, `object`, `rooted_chain`) and is constructed at exactly one
site, `argument_with_overlay` (`evaluator.rs:227-248`), which re-derives the
same `Value::StaticObject`/`Value::RootedMember` resolution already encoded in
`ArgumentData for CallArgInfo` (`flow/matcher.rs:135-153`). The memoization is
partial: `ArgumentData::static_string`/`static_object`/`rooted_chain`
(`matcher.rs:103-133`) fall back to the `ValueTable` arena lookups whenever the
prepared overlay is `None`, so a dynamic argument pays the lookup per predicate
anyway while the overlay builder has already paid it once. The model module
owns a type whose only behavior is defined by a trait living in
`flow/matcher.rs` and which is exercised only by the matching argument
evaluator.

**Recommendation:** Consolidate the "resolve `ValueId` → static string /
static object / rooted chain" derivation into one place (either keep only the
trait fallbacks and drop the prepared overlay type, or keep the overlay and
have the trait consult it without a second resolution path). Move the
definition beside its consumers (`flow/matcher.rs`/`matching/arguments`) if it
stays. Guardrails: preserve single-pass matcher cost — do not reintroduce a
second `ValueTable` traversal per predicate — and keep the flow side able to
match on raw `CallArgInfo` without the overlay.

**Fix Applied:** None so far.

### Model flow types — lifecycle evidence and limits

#### [ ] READ-003 — `LifecycleEvidence::{requirement_entries,sink_entries}` materialize a `Vec<E>` per index that one consumer immediately discards

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `analysis/model/flow.rs:388-400`, `analysis/flow/projector/evidence.rs:262-266`, `analysis/flow/projector/state/tables.rs:412-425`

`requirement_entries`/`sink_entries` clone every stored event into a fresh
`Vec<E>` per index (`flow.rs:388-400`). The trace builder
`build_flow_trace` (`projector/evidence.rs:262-266`) consumes only
`values.into_iter().next()` — the first event of each index — on every flow
finding emission, allocating a Vec only to drop all but its first element.
The loop-fixed-point snapshot (`state/tables.rs:412-425`) genuinely needs all
values, so the full clone is justified for exactly one consumer. Separately,
`prior_sink_events` (`flow.rs:410-419`) re-sorts its filtered result although
`EvidenceValues` maintains ascending order by construction (binary-search
insert), so the `.sort()` is redundant on the same hot read path.

**Recommendation:** Add a first-event-per-index accessor (or a borrowing
iterator such as `impl Iterator<Item = (RequirementIndex, &E)>`) on
`LifecycleEvidence` for the trace consumer, keeping the full-valued
`*_entries` view only for the snapshot consumer; drop the redundant `.sort()`
in `prior_sink_events` while retaining `.dedup()`. Guardrails: preserve
deterministic declaration order in both consumers and keep the sorted,
deduplicated contract of `prior_sink_events` (the `dedup` is still required).

**Fix Applied:** None so far.

#### [ ] READ-004 — `FlowState` and sibling flow types mix three visibility tiers (`pub`, `pub(crate)`, `pub(in crate::analysis)`) even though only `FlowReadiness` is consumed outside `crate::analysis`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `analysis/model/flow/state.rs:45-123`, `analysis/model/flow.rs:19-39,90-128`, `analysis/model/fact.rs:13-48`, `api/compiler/object_flow.rs:7,45-59`

On `FlowState` alone, `record_requirement`/`record_sink`/`remove_*`/`new`/getters
are `pub`, `clear_requirement`/`restore_requirement`/`requirement_entries`/
`sink_entries` are `pub(crate)`, and `is_ready`/`sinks_ready`/`prior_sinks` are
`pub(in crate::analysis)` — three tiers on one type, with every consumer inside
`crate::analysis`. The same split is visible across sibling types: `FactId` and
`ControlRegionId` are disciplined `pub(in crate::analysis)` newtypes
(`fact.rs:13-66`), while `FlowId`, `FlowLimits`, `RequirementIndex`, and
`SinkIndex` use crate-wide `pub` constructors/accessors (`flow.rs:19-39,
90-128`, `limits.rs:23-79`). Only `FlowReadiness` (and its two enums) is
actually referenced outside `crate::analysis` — by
`api/compiler/object_flow.rs:7,45-59`. The boundary therefore looks like three
enforcement tiers that correspond to no real consumer distinction, which makes
the intended API surface hard to reason about and invites future methods to be
added at the wrong tier.

**Recommendation:** Narrow the analysis-only flow types and all `FlowState`
methods to a uniform `pub(in crate::analysis)` (matching the fact-side
discipline), keeping `FlowReadiness` + `RequirementReadiness`/`SinkReadiness`
crate-visible for `api/compiler`; if any type must stay crate-wide, document
which cross-module consumer requires it. Guardrails: verify no consumer
outside `crate::analysis` references `FlowId`/`FlowLimits`/`FlowState`/
`RequirementIndex`/`SinkIndex`/`LifecycleRollback` before narrowing; the
`From<{RequirementIndex,SinkIndex}> for usize` impls used by `EvidenceIndex`
stay with the types.

**Fix Applied:** None so far.

#### [ ] READ-005 — `RequirementIndex` and `SinkIndex` are identical parallel newtypes and the 64-key cap invariant is re-encoded across the evidence stack

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `analysis/model/flow.rs:90-128,279-304`

`RequirementIndex` and `SinkIndex` are byte-for-byte identical newtypes
(`new` with the `u64::BITS as usize` bound, `get`, and `From<…> for usize`).
The same 64-cap invariant is re-encoded in `IndexedEvidence::ready` and `bit`
(`flow.rs:279-304`, including the `count == u64::BITS` special case and the
`(1u64 << count).saturating_sub(1)` mask), so a change to the key-domain bound
must be kept in sync in four places. The `ready(count, all)` helper is also a
bool-flag-driven phase (`all` selects any vs. all semantics) that obscures two
distinct outcomes for callers. Note: `IndexedEvidence::len` (test-only) counts
keys, not values, which is a mildly misleading name for a "length" accessor.

**Recommendation:** Consolidate the cap-bound and the bit-mask arithmetic into
one owner (a shared `BoundedIndex<const N>` core or a single module-private
helper), and split `ready` into named `ready_any`/`ready_all` (or equivalent)
so the semantics are explicit. Guardrails: preserve the type-level
distinctness between requirement and sink indices — they must not be
interchangeable, and the `EvidenceIndex` trait exists precisely to keep the
domains separate; keep the `u64::BITS` overflow special case for `count == 64`.

**Fix Applied:** None so far.

#### [ ] READ-006 — `cross/mod.rs` constructs a full `FlowLimits` only to read back its pass-through operation budget

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `analysis/model/flow/limits.rs:24-79`, `analysis/flow/cross/mod.rs:271`

`FlowLimits::from_flow_operations(project.flow_limit()).operation_limit()`
(`cross/mod.rs:271`) runs all five `scaled_limit` computations (with saturating
math and clamping) only to extract `operation_limit()`, which returns the
`flow_operations` value passed straight through (`limits.rs:24-54,76-79`). The
constructed `FlowLimits` is immediately discarded. This is an
immediately-consumed wrapper that hides the fact that the operation budget is
simply the project flow budget, and it forces `cross` to depend on the whole
scaling contract when it needs one number.

**Recommendation:** Use `project.flow_limit()` directly for the operation
budget at `cross/mod.rs:271` (the projector's full `FlowLimits` construction at
`analysis/project/projection.rs:390` remains the single scaling site), or
expose a named operation-budget accessor that documents the pass-through.
Guardrails: keep `from_flow_operations` for the projector path and keep the
min/clamp saturation behavior; do not change the cross-phase budget semantics
while collapsing the call.

**Fix Applied:** None so far.

## Systemic Themes

- **Visibility discipline is split by module age, not by consumer need.** The
  fact model consistently uses `pub(in crate::analysis)`; the flow model uses
  crate-wide `pub` (see READ-004). A future pass could audit every `pub` item
  under `analysis::model` against its actual consumers.
- **The bounded 64-key domain is the same invariant stated in several ways**
  (`RequirementIndex::new`, `SinkIndex::new`, `IndexedEvidence::ready`,
  `IndexedEvidence::bit`, plus the test asserting index 63/64 boundaries at
  `flow/tests.rs:110-143`); see READ-005.
- **Test-only surface is heavier than necessary:** `FactId` carries three
  `#[cfg(test)]` helpers (`from_test`, `raw_for_test`, `from_index`) and
  `FlowLimits` carries two near-identical test constructors
  (`test_new`/`test_with_operation_limit`, `limits.rs:81-109`) that differ only
  in the operation budget. These are small and test-local; consolidate if the
  chunk is touched for another reason, but not independently.
- **Evidence ordering discipline is consistent:** `EvidenceValues` keeps
  sorted inserts, `prior_sink_events` guarantees deterministic sorted+dedup
  output, and `IndexedEvidence::ready` is fail-closed (any out-of-range index
  disqualifies readiness). These invariants are correct and should be kept.

## Open Questions

- `SemanticFact::new(_authority: FactStreamToken, …)` makes the retained model
  (`model/fact.rs:438-451`) depend on the producer's capability token defined
  in `facts/stream.rs:34-47`. This enforces a genuine construction invariant
  and the parameter is deliberately unused, but the dependency direction is
  model→producer. Keep the token near its only creator (current state) or move
  the token type into `model/fact.rs` if the model should stop knowing about
  `facts::stream`.
- `CallUnwrap::effective_args` (`fact.rs:210-214`) stores a `Vec<CallArgInfo>`
  that overlaps the authored `CallEvent::args`; for wrapper calls both lists
  are retained because the bound projection is only derivable at build time.
  Whether the authored list can be dropped for wrapper calls was not resolved
  by this audit and would need provenance/spread tracing through
  `flow/summary/parameter.rs`.
- Whether `ArgumentView`'s partial memoization pays for its added type (see
  READ-002) could be measured with the harness profiling before deciding
  between the two consolidation directions.

## Coverage

- Chunk files read in full: `analysis/model/mod.rs`,
  `analysis/model/fact.rs`, `analysis/model/fact/tests.rs`,
  `analysis/model/flow.rs`, `analysis/model/flow/tests.rs`,
  `analysis/model/flow/limits.rs`, `analysis/model/flow/state.rs`.
- Representative producers/consumers traced: `analysis/facts/` (mod, stream,
  arguments, calls/mod, calls/callee, calls/wrapper, effect, control,
  functions), `analysis/flow/matcher.rs`, `analysis/flow/effect/*`,
  `analysis/flow/projector/*` (driver, evidence, state/tables,
  state/tables/updates, history), `analysis/flow/cross/*` (mod, evidence,
  state), `analysis/flow/summary/*` (parameter, sink, summaries),
  `analysis/matching/arguments/*` (evaluator), `analysis/api/compiler/object_flow.rs`.
- Guidance consulted: `AGENTS.md`, root and core `ARCHITECTURE.md`, `TESTING.md`.
- No source, test, config, Cargo, or documentation file was modified; only
  this audit file was created.
