# Codebase Readability Audit — glass-lint-core Chunk 10: Flow projector state and summaries

## Summary

Chunk 10 owns the reversible flow-state tables (`projector/state`, `state/tables`,
`state/tables/aliases`, `state/tables/updates`), statement transfer
(`projector/transfer`), and the function-summary pipeline
(`summary/{mod,parameter,sink,store,summaries}`). The chunk's contract with its
siblings is: the projector (`projector/driver`, `control`, `loops`, `evidence`)
captures/restores `FlowEnvironment` checkpoints through `FlowStateTable`,
transfers values via `ObjectFlowProjector::assign`, and consumes
`FunctionSummaries` to project helper sinks and invocation compatibility. The
public/external surface is small and validated (`FlowStateTable`,
`FlowSemanticSnapshot`, `FlowEnvironment`, `StateAdmission`, `SinkSet`,
`FunctionSummary`, `SummaryPathStore`, `SummaryPathId`), mostly `pub(in
crate::analysis::flow)`, so the main readability cost is internal.

Overall the chunk is well-factored: state mutation is consistently routed
through the `MutationLog` with fail-closed budget flags, canonical semantic
snapshots keep loop fixed points deterministic, and `resolve_call_target` /
`value_at_path` free functions are sensibly shared. The findings below are
concentrated on dead/discarded control flow in `transfer.rs`, mechanical
frozen/overlay dispatch in `SummaryPathStore`, clone-and-revalidate loop frame
accessors in `ControlStack`, and a few one-field/plain-carrier types whose
visibility or construction surface is inconsistent.

## Findings

### [transfer.rs — value transfer and source matching]

#### [ ] READ-001 — Dead conditional around `admit_object`; `StateAdmission` result discarded

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/transfer.rs:34-40`

In `ObjectFlowProjector::assign`, both arms of the `matches!(
self.flow_state.admit_object(&aliases, object, states), StateAdmission::Admitted
)` conditional execute `return;` (transfer.rs:38 and :40). The conditional is
dead: whether the batch is `Admitted` or `Rejected`, the function returns and
`StateAdmission` is fully discarded. A reader is led to believe the admission
outcome changes behavior (e.g., falling through to the plain alias-binding path
at transfer.rs:43-47), but the only remaining effect of rejection is the
side-effect `state_limit_rejected` flag read later by
`FlowCompletion::from_sources` (projector/mod.rs:63-66). This is a leftover
branch that obscures the fail-closed path and invites future "fixes" that
mistake the second `return` for an `else`.

**Recommendation:** Collapse to `let _ = self.flow_state.admit_object(&aliases,
object, states); return;` (or make the intended fall-through explicit if a
`Rejected` batch should retry binding via `object_for(source)`). Keep the
guarantee that a rejected batch leaves aliases and states untouched and still
flags `state_limit_rejected`; do not let rejection silently bind aliases.

**Fix Applied:** None so far.

### [state/tables.rs — canonical flow state table]

#### [ ] READ-002 — Per-method frozen/overlay dispatch repeated across six `SummaryPathStore` methods

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/summary/store.rs:103-202`

`SummaryPathStore` repeats the same `match id { Frozen(p) => self.frozen.X(p),
Overlay(p) => self.overlay.X(p) }` dispatch in `is_valid` (:103-108), `depth`
(:116-121), `parent` (:123-134), `segment` (:166-171), `first_segment_of`
(:173-178), and `find_edge_impl` (:187-198). On top of that, `find_edge`
(:200-202) is a one-line forwarder to `find_edge_impl` that adds no vocabulary,
and `SummaryPathId::path_id()` (:31-35) re-packs the discriminant that the
dispatch already matches on. Every new `PathStore`-shaped operation on the
wrapper must hand-write the same match, and each variant is only safe because
the two `PathStore`s stay in lock-step.

**Recommendation:** Add a private `fn store_for(&self, id: SummaryPathId) ->
&PathStore` on `SummaryPathStore` and rewrite `is_valid`, `depth`, `segment`,
`first_segment_of`, and `find_edge_impl` as `self.store_for(id).X(id.path_id())`.
Delete the `find_edge`/`find_edge_impl` split. Keep `parent`'s match because it
must translate `ParentRef::Linked(link)` to `Frozen(link.path())`; keep the
overlay-capacity fail-closed behavior (`PathStore::with_max_nodes`) intact.

**Fix Applied:** None so far.

### [state.rs — control stack]

#### [ ] READ-003 — Loop frame accessors clone the whole frame and re-validate on every path

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:284-311, 334-343`

`ControlStack::loop_frame` is a read operation (`&self`) that returns an owned
clone of the entire `ControlFrame::Loop`, including every `Vec<FlowEnvironment>`
field (:293). `transfer_loop` destructures that clone and moves the vectors into
`finish_loop` (control.rs:106-119), and `finish_loop` then calls
`pop_loop(body_start)` (driver.rs:220), which re-validates the top frame kind
and `body_start` (state.rs:298-311) a second time. So every `LoopEnd` copies the
whole frame (`FlowEnvironment` is `Copy`, so this is an O(paths) clone per loop
exit) and validates the frame twice. `new_loop_breaks_since` (:334-343) does the
same clone-to-`Vec` dance with `breaks.get(count..).to_vec()`. The clone is a
borrow-checker workaround: `finish_loop` needs `&mut self` while `loop_frame`
holds `&self`.

**Recommendation:** Rework the loop hand-off so `transfer_loop` takes the
`LoopEnd` frame out by value once (e.g., a `take_loop(region)`/`pop_loop`
returning the owned frame, or `loop_frame` returning a reference consumed inside
a closure) and delete the duplicate kind/`body_start` validation from the
follow-up pop. Guardrail: every error path (wrong region, wrong kind, empty
stack) must leave the stack unchanged and mark `alternatives_complete` incomplete
(fail-closed), as control.rs:115-118 and loops.rs:136-139 currently do.

**Fix Applied:** None so far.

### [state/tables/updates.rs — property-write update carrier]

#### [ ] READ-004 — `PropertyWriteUpdate` exposes both public fields and a constructor at a single call site

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/state/tables/updates.rs:1-20`

`PropertyWriteUpdate` is a two-field plain carrier (`index`, `value_matches`)
whose fields are `pub(in crate::analysis::flow::projector)` *and* which also
ships a `new` constructor that merely assigns those fields. It has exactly one
producer (`record_property_write`, driver.rs:407-416) and one consumer
(`FlowStateTable::apply_property_write`, tables.rs:272-297), so the double
surface (public fields + constructor) has no reason to exist and the module
exists solely to host it. This is the "immediately-consumed wrapper" shape: no
invariant or vocabulary is enforced by the type itself.

**Recommendation:** Pick one exposure: either keep the fields private and route
construction through `new` (making the struct an immutable unit), or drop the
constructor and treat the struct as a plain data record — or pass the
`(RequirementIndex, bool)` pair directly through the callback closure if no
reuse appears. Guardrail: preserve the clear-then-conditional-record protocol
in `apply_property_write` (tables.rs:287-293) and the Copy-ness of the carrier.

**Fix Applied:** None so far.

### [sink.rs — sink summary set]

#### [ ] READ-005 — `SinkSet::sort_and_dedup` re-implements the field-order comparison manually

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/summary/sink.rs:12-17, 63-71`

`FunctionSinkSummary` derives `PartialEq, Eq, Hash` but not `Ord`, even though
all three fields (`FlowId`, `usize`, `SummaryPathId`) implement `Ord`.
`SinkSet::sort_and_dedup` then hand-writes the lexicographic comparison as
`(left.flow(), left.parameter_index(), left.path()).cmp(&(...))` — exactly the
tuple order `#[derive(Ord)]` would generate. The manual closure must be kept in
sync with the field list whenever the struct grows, and it duplicates what the
derived trait already knows.

**Recommendation:** Derive `Ord` on `FunctionSinkSummary` and replace the
closure with `self.set.sort()` (or keep `sort_by` with `|a, b| a.cmp(b)` if
dedup must run after sorting). Guardrail: iteration order of the sorted set must
remain deterministic, since `finalize`/propagation depend on stable ordering.

**Fix Applied:** None so far.

### [tables.rs / state.rs — flow environment]

#### [ ] READ-006 — `FlowEnvironment::reachable` field is more visible than its semantic accessor

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/state/tables.rs:29-33; state.rs:438-441`

`FlowEnvironment` stores `reachable` as a `pub(in crate::analysis::flow::projector)`
field (tables.rs:32) while also providing the semantic accessor `is_reachable()`
(state.rs:438-441, `pub(super)`). The field's visibility is strictly wider than
the accessor's, so the accessor adds no encapsulation — the invariant "a
snapshot knows whether execution can reach it" is encoded twice with no rule for
which surface callers should use. (The sibling `checkpoint` field is at least
narrower at `pub(super)`.)

**Recommendation:** Make `reachable` private to the state module (field
`pub(super)` or private) and keep `is_reachable()` as the sole public read
surface, which driver.rs already uses both directly (:276) and as a function
pointer in `paths.retain(FlowEnvironment::is_reachable)` (:340). Guardrail:
nothing may mutate `reachable` directly; it is set only by
`FlowStateTable::capture`.

**Fix Applied:** None so far.

### [summaries.rs — summary collection and propagation]

#### [ ] READ-007 — Index-based call iteration with per-index re-lookup duplicated across two callers

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/summary/summaries.rs:148-182, 271-300`

Both `FunctionSummaries::collect_direct_sinks` (:154-170) and
`SummaryPropagation::run` (:275-287) implement the same shape: snapshot a call
count, then iterate `for index in 0..count { calls().get(index) }` — with `run`
re-fetching `summaries.by_id.get(caller)` on *every* index (:282-286). Both are
borrow-conflict workarounds: the sink-collection calls take `&mut self.paths` /
`&mut self`, so the summary reference cannot be held across the call. The result
is a fragile index convention (empty slots must be `continue`d) repeated in two
places.

**Recommendation:** Give `FunctionSummary` or `FunctionSummaries` one narrow
operation that yields the call ids for a caller (e.g., iterate a cloned
`Vec<FactId>` once per caller, or a `call_ids(id) -> Vec<FactId>` helper), and
have both `collect_direct_sinks` and `run` consume it. Guardrail: keep the
snapshot semantics — a summary's call list must not change while it is being
collected, and the mutation log / budget charges must stay unchanged.

**Fix Applied:** None so far.

## Systemic Themes

- **Peek-that-clones accessors.** `ControlStack::loop_frame` and
  `new_loop_breaks_since` (state.rs:284-343) return owned clones of storage
  instead of a borrow, and are consumed only via immediate destructuring.
  Prefer take-by-value or borrow-inside-closure so ownership is explicit.
- **Discarded outcomes in bounded-flow code.** `StateAdmission` (transfer.rs),
  `IndexTable::insert` (`let _ =`, summaries.rs:92), and
  `mark_event_truncated` (`let _ =`, state.rs:193) all discard fallible results;
  only the fail-closed flag side effects carry the outcome. When a result is
  truly unobservable, say so in a comment; when it is observable, don't discard
  it.
- **Manual ordering.** `SinkSet::sort_and_dedup` re-implements field-order
  comparison that derived `Ord` would supply (sink.rs:63-71).
- **Visibility inconsistency across related types.** `AliasTable` mixes
  `pub(super)` and `pub(in crate::analysis::flow::projector)` on different
  methods (aliases.rs:13-59), `ObjectRefCounts` is a private struct whose
  methods are `pub(in crate::analysis::flow::projector)` (aliases.rs:61-91), and
  `FlowEnvironment::reachable` is wider than its accessor. Within one module the
  same visibility conventions should be used consistently.
- **Admission vocabulary collision.** `StateAdmission` (Admitted/Rejected,
  tables.rs:92-97) and `PathAdmission` (Admitted/Duplicate/RestoreFailed/
  Exhausted, driver.rs:32-37) are distinct domains but use overlapping names for
  overlapping concepts; keep them distinct (do not merge) but consider names
  that signal the capacity vs. frontier distinction.

## Open Questions

- **Intended fall-through in `assign`?** `ObjectFlowProjector::assign`
  (transfer.rs:27-47) returns after `admit_object` regardless of the result. It
  is unclear whether a `StateAdmission::Rejected` batch was originally meant to
  fall through to the plain `object_for(source)` alias binding (transfer.rs:43-47)
  or whether unconditional return is intended. Either way the `matches!` is dead;
  the semantics of rejection (never bind) must be preserved.
- **Reachability of the `RequirementRemove` redo path.** `InverseDelta::apply`
  (state.rs:86-93) on redo performs `clear_requirement` then
  `restore_requirement(events)`, which re-adds the very events the removal was
  supposed to delete. Since `FlowStateTable::restore` only transitions between
  checkpoints, this redo branch may be unreachable in practice; if it is ever
  reached, verify that moving forward re-applies the removal correctly.
- **Recursion depth of `SummaryPathWalk::visit`** (store.rs:69-82) and
  `join_suffix` / `without_first_from` is bounded by path depth but recurses per
  segment; with `MAX_OVERLAY_NODES = 4096` and path-depth limits this is bounded,
  but worth confirming against `PathStore` depth caps.

## Coverage

- Read fully: `projector/state.rs`, `projector/state/tables.rs`,
  `projector/state/tables/aliases.rs`, `projector/state/tables/updates.rs`,
  `projector/state/tests.rs`, `projector/transfer.rs`,
  `summary/mod.rs`, `summary/parameter.rs`, `summary/sink.rs`,
  `summary/store.rs`, `summary/summaries.rs`.
- Read for context and representative call sites: `projector/mod.rs`,
  `projector/driver.rs`, `projector/control.rs`, `projector/loops.rs`,
  `projector/evidence.rs`, `projector/history.rs`, `model/flow.rs`,
  `model/flow/state.rs`, `model/fact.rs`, `api/classification.rs`.
- Searched (`rg`) for `unwrap`/`expect`/`panic`/`unreachable!`/`let _ =`,
  `admit_object`/`StateAdmission`, `loop_frame`/`pop_loop`/`loop_break_count`,
  `starts_with_frozen`/`matches_frozen`/`path_interner`, and summary-store method
  usage. The `unreachable!` calls found are guarded by prior matches and are not
  findings.
- Chunk contract verified against `CODEBASE_STRUCTURE_CORE.md` Chunk 10
  (state, tables, aliases, updates, transfer, summary and its submodules);
  no sibling-chunk (driver/control/loops/evidence/history) type was proposed for
  deletion or merger.
- `git status --short` after the audit shows only this file as a new/untracked
  change; no source, test, config, or Cargo file was modified.
