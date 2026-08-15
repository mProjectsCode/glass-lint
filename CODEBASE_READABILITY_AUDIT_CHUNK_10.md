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

#### [x] READ-001 — Dead conditional around `admit_object`; `StateAdmission` result discarded

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

**Recommendation:** Collapse the dead conditional to a single call and
unconditional return: `let _ = self.flow_state.admit_object(&aliases, object,
states); return;`. Rejection must not fall through to the `object_for(source)`
path (see the Resolved note under Open Questions): a rejected batch leaves
aliases and states untouched and still flags `state_limit_rejected`; do not let
rejection silently bind aliases.

**Fix Applied:** Already fixed by chunk 09 read 005 (`9dd3a20f`): the
`matches!(... StateAdmission::Admitted)` conditional in
`ObjectFlowProjector::assign` was collapsed to a single `admit_object` call
followed by an unconditional `return`, and the `StateAdmission` import was
removed. Rejection still leaves aliases/states untouched and flags
`state_limit_rejected`; no fall-through to the plain alias-binding path. The
current code matches the recommendation; nothing further to do.

### [state/tables.rs — canonical flow state table]

#### [x] READ-002 — Per-method frozen/overlay dispatch repeated across six `SummaryPathStore` methods

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
Delete the `find_edge`/`find_edge_impl` split; `find_edge` re-wraps the matched
`PathId` in the variant of its parent. Keep `parent`'s match because it must
translate `ParentRef::Linked(link)` to `Frozen(link.path())`; keep the
overlay-capacity fail-closed behavior (`PathStore::with_max_nodes`) intact.

**Fix Applied:** Added private `SummaryPathStore::store_for(&self, id) -> &PathStore`
and rewrote `is_valid`, `depth`, `segment`, and `first_segment_of` as
`self.store_for(id).X(id.path_id())`. Merged `find_edge_impl` into `find_edge`,
which resolves through `store_for(parent)` and re-wraps the found `PathId` in
the parent's variant. `parent` keeps its `ParentRef::Linked(link)` →
`Frozen(link.path())` translation, and the overlay-capacity fail-closed behavior
is unchanged.

### [state.rs — control stack]

#### [x] READ-003 — Loop frame accessors clone the whole frame and re-validate on every path

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:284-311, 334-343`

`ControlStack::loop_frame` is a read operation (`&self`) that returns an owned
clone of the entire `ControlFrame::Loop`, including every `Vec<FlowEnvironment>`
field (:293). `transfer_loop` destructures that clone and moves the vectors into
`finish_loop` (control.rs:106-119), and `finish_loop` then calls
`pop_loop(body_start)` (driver.rs:220), which looks the top frame up and
re-validates its kind a second time (state.rs:298-311); `body_start` itself is
matched only in `pop_loop`, and region only in `loop_frame`. So every `LoopEnd`
copies the whole frame (`FlowEnvironment` is `Copy`, so this is an O(paths)
clone per loop exit) and checks the top-frame kind twice.
`new_loop_breaks_since` (:334-343) does the same clone-to-`Vec` dance with
`breaks.get(count..).to_vec()`. The clone is a borrow-checker workaround:
`finish_loop` needs `&mut self` while `loop_frame` holds `&self`.

**Recommendation:** Keep the loop frame on the control stack for the whole
fixed point — `converge` reads and mutates its `breaks`/`continues` via
`loop_break_count`, `take_loop_continues`, and `new_loop_breaks_since` — so it
cannot be taken off the stack before `finish_loop` runs. Have one accessor
validate region, kind, and `body_start` together and hand the owned
`baseline`/`breaks`/`continues` vectors to `finish_loop` (by value from a
`&mut` accessor, or as the existing clone), and drop the second top-frame lookup
and repeated kind check from the deferred pop. Guardrail: every error path
(wrong region, wrong kind, empty stack) must leave the stack unchanged and mark
`alternatives_complete` incomplete (fail-closed), as control.rs:115-118 and
loops.rs:136-139 currently do.

**Fix Applied:** `ControlStack::loop_frame` was already replaced by chunk 09
read 003 (`3356ffa6`) with `take_loop_seed(region)`, the single accessor that
validates region/kind and hands owned `baseline`/`breaks` plus cloned
`continues` to `finish_loop` via `LoopSeed`, keeping the loop frame on the stack
through the fixed point. Here the deferred pop no longer re-looks up the top
frame or re-validates its kind: `pop_loop` now pops directly and only reports
`Empty` (stack unchanged, run marked incomplete), so the `LoopEnd` path checks
the top-frame kind exactly once.

### [state/tables/updates.rs — property-write update carrier]

#### [x] READ-004 — `PropertyWriteUpdate` exposes both public fields and a constructor at a single call site

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

**Recommendation:** Drop the `new` constructor and treat `PropertyWriteUpdate`
as a plain data record: the fields are already `pub(in
crate::analysis::flow::projector)` — the same scope as the struct — and the sole
producer (`record_property_write`, driver.rs:407-416) constructs it in place;
making the fields private would require getters for `tables.rs`. Guardrail:
preserve the clear-then-conditional-record protocol in `apply_property_write`
(tables.rs:287-293) and the Copy-ness of the carrier.

**Fix Applied:** Dropped the `PropertyWriteUpdate::new` constructor. The sole
producer `record_property_write` (driver.rs) now constructs the plain two-field
record in place with the same `pub(in crate::analysis::flow::projector)` fields;
`apply_property_write`'s clear-then-conditional-record protocol and the Copy
carrier are unchanged.

### [sink.rs — sink summary set]

#### [x] READ-005 — `SinkSet::sort_and_dedup` re-implements the field-order comparison manually

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

**Fix Applied:** Derived `PartialOrd, Ord` on `FunctionSinkSummary` and replaced
the manual field-order closure in `SinkSet::sort_and_dedup` with
`self.set.sort()`. The sorted iteration order matches the former lexicographic
field order exactly, so `finalize`/propagation ordering is unchanged.

### [tables.rs / state.rs — flow environment]

#### [x] READ-006 — `FlowEnvironment::reachable` field is more visible than its semantic accessor

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

**Recommendation:** Make `reachable` `pub(super)` (the state module, which
covers both `state.rs` and `state/tables.rs`) and keep `is_reachable()` as the
sole read surface for callers outside it, which driver.rs already uses both
directly (:276) and as a function pointer in
`paths.retain(FlowEnvironment::is_reachable)` (:340). It cannot be fully private
because `FlowEnvironment::initial` (state.rs:431-436) constructs the field.
Guardrail: `reachable` is written only in `FlowStateTable::capture`
(tables.rs:466-471) and `FlowEnvironment::initial`; outside the state module it
may only be read through `is_reachable()`.

**Fix Applied:** Narrowed `FlowEnvironment::reachable` from
`pub(in crate::analysis::flow::projector)` to `pub(super)` (the state module,
covering both `state.rs` and `state/tables.rs`), leaving `is_reachable()` as the
sole read surface for callers outside the state module. In-module writes in
`FlowStateTable::capture` and `FlowEnvironment::initial` still access the field
directly; driver.rs continues to use `is_reachable()` both directly and as the
`paths.retain` predicate.

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
is a fragile index convention (each `get(index)` must be option-unwrapped)
repeated in two places.

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
  instead of a borrow, and are consumed only via immediate destructuring. Where
  the storage must stay in place (the loop frame must remain on the control
  stack through the fixed point), keep the snapshot hand-off explicit and avoid
  re-validating the frame on the follow-up pop.
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

- **Resolved — intended fall-through in `assign`?** Unconditional return is
  intended: the pre-refactor code (commit `28f5b876`) already returned on the
  state-limit-rejected path without binding via `object_for(source)`, and
  `admit_object` documents that a rejected batch leaves aliases and states
  unchanged while recording the fail-closed outcome (tables.rs:346-373). A
  `Rejected` batch must never bind aliases; the `matches!` (transfer.rs:34-40)
  is a leftover from the refactor and should be collapsed (see READ-001).
- **Resolved — reachability of the `RequirementRemove` redo path.** The redo
  branch is reachable: `restore` transitions move forward (Redo) across deltas
  whenever a checkpoint on a divergent or earlier branch is restored — exercised
  in `checkpoints_restore_divergent_mutation_paths` (state/tests.rs:74-96). And
  it is incorrect: on redo, `clear_requirement(*index)` followed by
  `restore_requirement(*index, events)` (state.rs:86-93) re-adds the very events
  the removal deleted, so moving forward does not re-apply the removal — the only
  delta whose redo is not the inverse of its undo (compare `RequirementInsert`,
  state.rs:77-85). Redo should leave the requirement cleared (clear-only), and
  the branch currently has no test.
- **Resolved — recursion depth of `SummaryPathWalk::visit`** (store.rs:69-82)
  and `join_suffix` / `without_first_from` is bounded by node counts, not a
  dedicated depth cap: `PathStore` tracks `depth: u32` (path_trie/store.rs) with
  no depth limit, but a path's depth cannot exceed its store's `max_nodes` —
  overlay paths are capped at `MAX_OVERLAY_NODES = 4096` and frozen paths at
  `DEFAULT_MAX_PATH_NODES = 1 << 20` (path_trie/types.rs). So recursion is at
  most ~4096 small frames for overlay paths, which is safe on default stacks but
  relies on the node-count cap, not an explicit depth limit.

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
