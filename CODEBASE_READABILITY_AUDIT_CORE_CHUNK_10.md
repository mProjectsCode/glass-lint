# Codebase Readability Audit

Chunk 10 — "Flow projector state and summaries"
(`glass-lint-core/src/analysis/flow/projector/state.rs`, `state/tables.rs`,
`state/tables/{aliases,updates}.rs`, `projector/transfer.rs`,
`analysis/flow/summary/{mod,parameter,sink,store,summaries}.rs`)

## Summary

This chunk owns the projector's reversible state (environments, control stack,
alias/flow tables, mutation log), evidence accumulation, value transfer, and the
reusable function-summary pipeline. Overall the module tree is well-factored:
`FlowStateTable` is a single owner of aliases + state + rollback, `AliasTable` +
`ObjectRefCounts` are a cohesive private pair, the `Canonical*` snapshot family
is module-local and single-purpose, and the `summary` modules have clear per-file
responsibilities. The main problems found are one undo/redo asymmetry in the
requirement delta that resurrects stale evidence, a sink-to-parameter binding
search repeated verbatim in three places, an admission enum whose only
production caller discards the result, and a few over-broad or off-site
vocabulary/API choices. Findings READ-001..READ-006 below; no fixes applied.

## Findings

### Flow projector state and tables

#### [ ] READ-001 — `RequirementRemove` redo re-adds the events a property-write removed

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:86-93`

`InverseDelta::apply` handles `RequirementRemove` with
`if !undo { state.clear_requirement(*index); } state.restore_requirement(*index, events);`
(`state.rs:90-91`). `events` is the pre-removal value set captured by
`FlowStateTable::clear_requirement` (`tables.rs:249-265`), so the redo arm clears
the index and then immediately re-inserts the very set that the recorded
property-write removal erased. Redo is genuinely reachable: every path admission
and restoration funnels through `MutationLog::transition` (`history.rs:77-88`),
and `restore_path`/`admit_path` (`driver.rs:238`, `293`) move the projector
between sibling branch checkpoints, which undoes one chord of deltas and redoes
another. After a redo over a branch containing `RequirementRemove` followed by a
new `RequirementInsert` (per `apply_property_write`, `tables.rs:283-295`), the
index ends up `{removed_events, new_event}` instead of `{new_event}`. Existing
tests only restore to ancestors (`state/tests.rs:365-432`,
`state/tests.rs:74-96`), so the branch-fan-out forward path is never exercised.

**Recommendation:** Fix on the owner `FlowStateTable`/`InverseDelta::apply`: the
redo arm should be `state.clear_requirement(*index)` only; undo alone restores
`events`, which is already the correct inverse. Add a divergence regression in
`state/tests.rs` mirroring `checkpoints_restore_divergent_mutation_paths` but
with sibling captures whose branch deltas include a requirement removal, and
assert `requirement_entries()` equals the post-write set after restoring the
sibling. Guardrails: `RequirementInsert`/`SinkInsert` and the alias/state deltas
are symmetric and must stay unchanged; keep the fail-closed empty-index no-op.

**Fix Applied:** None so far.

#### [ ] READ-002 — Sink-to-parameter binding searches are written three times with different acceptance rules

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/summary/evidence.rs:135-147`, `glass-lint-core/src/analysis/flow/summary/sink.rs:220-241`, `glass-lint-core/src/analysis/flow/summary/summaries.rs:317-341`

The same two predicates are re-implemented in three modules:
`parameter.parameter_index() == sink.parameter_index() && parameter.matches_sink_path(sink.path(), paths)`
appears verbatim in `evidence.rs:136-139` (`record_helper_sink`) and
`summaries.rs:326-328` (`try_project_sink`), and the value-to-parameter match
`parameter.value() != ValueId::UNKNOWN && parameter.value() == argument`
appears in `sink.rs:221-224` (`collect_sinks_for_call`, on
`argument.base_value`) and `summaries.rs:331-333` (with an extra
`!parameter.is_rest()` gate). Any change to how a summarized sink binds to a
parameter (e.g., rest-prefix or path fallback handling) must now be applied in
three files at risk of divergence; the subtle `is_rest()`/UNKNOWN differences
already differ between the copies.

**Recommendation:** Consolidate on `summary::sink` (or on `ParameterBinding` in
`summary/parameter.rs`) with two narrow helpers — e.g.
`find_sink_parameter<'a>(parameters, sink, paths) -> Option<&'a ParameterBinding>`
and `parameter_for_value<'a>(parameters, value) -> Option<&'a ParameterBinding>` —
and route all three call sites through them, deleting the inline loops.
Guardrails: keep `collect_sinks_for_call`'s distinct prefix/suffix `join`
(present-index `paths.join`), keep the `!is_rest()` gate only in the
cross-function propagation path, and keep UNKNOWN rejection in every copy.

**Fix Applied:** None so far.

#### [ ] READ-003 — `StateAdmission` is a discarded production result that duplicates the fail-closed flag

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/projector/state/tables.rs:90-97`, `tables.rs:351-373`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:30`

`admit_object` returns `StateAdmission::{Admitted, Rejected}` (`tables.rs:92-97`),
but its only production caller discards the result
(`transfer.rs:30`), so `Admitted` is never read in production. The `Rejected`
outcome is already surfaced through the table's `state_limit_rejected` flag,
which `FlowCompletion::from_sources` consumes (`projector/mod.rs:63-67`), making
the enum pure test vocabulary. Separately, the test-only
`insert_state` (`tables.rs:320-330`) re-implements the same capacity decision
(only reject for a brand-new key) with slightly different arithmetic than
`admit_object`'s batch check, so the two admission paths can drift.

**Recommendation:** On `FlowStateTable`, drop `StateAdmission` and have
`admit_object` return `()` (or `bool` if a test needs it), asserting rejection
through `state_limit_rejected()` plus table contents in `state/tests.rs`. Remove
or reroute `#[cfg(test)] insert_state`: rewrite its 11 tests to build states via
`admit_object` with the aliases they already construct. Guardrails: keep the
atomic all-or-nothing batch decision before `bind_aliases`/`insert_state_unchecked`,
keep the flag read by `from_sources`, and keep `StateAdmission`'s ordering
(`Admitted`/`Rejected`) out of any future public surface.

**Fix Applied:** None so far.

#### [ ] READ-004 — Invocation-compatibility gate recomputes the projection fallback decisions

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/summary/parameter.rs:10-91`

`accepts_invocation_projection` (`parameter.rs:10-26`) re-derives the same
"missing argument → default_value" and "empty path → argument value" decisions
that `project_argument_at` (`parameter.rs:47-91`) computes later, and the two are
deliberately more permissive in the gate (short-circuits `is_rest()` and empty
path back to `true` even when the argument value is `ValueId::UNKNOWN` or spread).
Every call site does gate-then-project (`evidence.rs:120-146`,
`summaries.rs:196-198`, `summaries.rs:317-329`), so the permissiveness contract
must be held in sync across two code paths with no single statement of "can this
invocation bind a value."

**Recommendation:** Make the acceptance a thin wrapper over one projection
decision — e.g. have `accepts_invocation_projection` call `project_argument` and
combine the rest/empty-path shortcuts explicitly, or expose
`project_argument_at`-based helper that both phases share. Guardrails: preserve
the exact permissiveness boundary (rest accepts all shapes; spread and UNKNOWN
never bind a value; default applies only when the argument is absent and the path
is empty), covered by the existing `summary/sink/tests.rs` and
`summary/summaries/tests.rs` cases so the refactor stays behavior-identical.

**Fix Applied:** None so far.

#### [ ] READ-005 — `FunctionSummary`/`FunctionSignature` live under `sink`, split from their aggregate in `summaries`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary/sink.rs:86-182`, `glass-lint-core/src/analysis/flow/summary/summaries.rs:19-70`

`summary::sink` owns `FunctionSinkSummary` and `SinkSet` (its namesakes), but also
`FunctionSignature` and `FunctionSummary` (`sink.rs:86-182`) — the function shape
and the complete per-function row (id, signature, calls, sinks) — while the
aggregate `FunctionSummaries` and its fixed-point machinery live one module over
(`summaries.rs:19-70`). The pairing sentence in the chunk inventory
("`summary::sink::FunctionSummary — Stores the complete summary for one function`")
reads bent precisely because the "sink" module owns the whole function. The
per-file split itself is defensible (each file is 128-344 lines and cohesive);
the incoherence is that the `Function*` family is straddled across two files.

**Recommendation:** Move `FunctionSummary` and `FunctionSignature` beside their
aggregate — fold them into `summaries.rs` or introduce a `function.rs`, leaving
`sink.rs` with `FunctionSinkSummary` + `SinkSet` only. Preserve the
`pub(in crate::analysis::flow)` surface and move the corresponding tests
(`summary/sink/tests.rs` sections) intact. Guardrails: keep `FunctionSummary`
field visibility unchanged (no caller-written filters outside the modules), keep
`SinkSet::sort_and_dedup` as the only ordering contract, and do not merge the
fixed-point worklist types.

**Fix Applied:** None so far.

#### [ ] READ-006 — `object_range` encodes a range scan through fabricated `FlowId` sentinels

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/flow/projector/state/tables.rs:206-209`

`states_for`/`remove_states_for` retrieve one object's state rows via
`FlowStateKey::new(object, FlowId::new(RuleIndex::new(0), 0)) ..= FlowStateKey::new(object, FlowId::new(RuleIndex::new(usize::MAX), usize::MAX))`
(`tables.rs:206-209`). This relies on `FlowId`'s private
`(rule_index, flow_index)` `Ord` shape (model/flow.rs:20-36) and on the ordering
of `FlowStateKey` being `(object, flow)`, with no comment at the sentinel site;
nothing documents that `RuleIndex::new(usize::MAX)` is a range bound rather than
a real index. It is correct and bounded today, but the bound mirrors the model's
private representation.

**Recommendation:** On `FlowStateKey` (the type that owns the ordering) add a
documented helper such as `FlowStateKey::object_range(object)` or a
`FlowStateTable::states_for` doc note stating the constraint, moving the
min/max construction next to the key's copies. Guardrails: keep the
`BTreeMap::range` scan (no per-object index growth), keep deterministic ascending
(flow) iteration order asserted by `states_for` callers in `evidence.rs:47`,
`evidence.rs:86`, and the state tests.

**Fix Applied:** None so far.

## Systemic Themes

- **Single rollback authority.** The reversible-state design is correctly
  concentrated: `MutationLog` (chunk 9 `history.rs`) owns position/branch
  mechanics, `FlowStateTable` owns aliases + states + log (`tables.rs:78-88`),
  and `InverseDelta::apply` (`state.rs:27-105`) is the only place deltas touch
  the tables. There is no overlap between chunk 9 `history` and
  `state::tables`; the reverse log is genuinely one concept. READ-001 is the one
  place the apply function is not inverse-symmetric.
- **Admission vocabulary.** The projector has two parallel capacity/admission
  enums — `driver::PathAdmission {Admitted, Duplicate, RestoreFailed,
  Exhausted}` (alive and exercised) and the effectively unused
  `state::tables::StateAdmission` (READ-003). One admission concept should live
  in one shape.
- **Parameter projection is the most duplicated logic** in the chunk: the sink
  binding search (READ-002), the gate/projection duality (READ-004), and
  `value_at_path` (`parameter.rs:94-128`) all encode the same acceptance rules
  with slightly different constants.
- **Well-owned parts that need no change:** `AliasTable` + `ObjectRefCounts`
  (`tables/aliases.rs`) is a tight private pair whose refcount invariant is
  enforced solely by `AliasTable`; the `Canonical*` family
  (`tables.rs:40-74`) is module-local, Ord-shaped, and only exists to build
  `FlowSemanticSnapshot`; `SummaryPathStore` reuses the shared `PathStore`
  intern/overlay machinery rather than duplicating value/identity storage, so no
  parallel model exists there.

## Open Questions

- **`Canonical*` family (explicitly asked):** verdict is "not a finding." The
  five structs are private to `tables.rs`, encode exactly one idea (projection-
  local object normalization for fixed-point convergence, `tables.rs:384-435`),
  and give `FlowSemanticSnapshot` deterministic `Ord`. The two leaf structs
  (`CanonicalRequirementState`, `CanonicalSinkState`, `tables.rs:49-59`) merely
  restate what `requirement_entries()/sink_entries()` already yield
  (model/flow/state.rs:126-142); whether that parallel is worth keeping depends
  on whether a future snapshot needs to absorb other event shapes.
- **`summary` four-module split (explicitly asked):** mostly justified by size
  and cohesion; the only incoherence is READ-005. `FunctionSignature` and
  `SinkSet` do add vocabulary: `SinkSet` hides ordering/dedup, and
  `FunctionSignature` hides the `(parameter_count, has_rest)` shape — both are
  used only through their owning `FunctionSummary`.
- **`summary::store` dual representation (explicitly asked):** the
  `Frozen`/`Overlay` `SummaryPathId` enum plus `parent()` mapping
  `ParentRef::Linked` to frozen and the `without_first_from`/`join_suffix`/
  `SummaryPathWalk` walkers encode the same leaf-to-root traversal several ways
  (`store.rs:49-93, 124-135, 251-275`). This is the most complex part of the
  chunk but is thoroughly regression-tested (`store/tests.rs`); a future pass
  could see whether one walk primitive can serve `starts_with`, `join`, and
  `without_first`.
- **Doc drift:** `CODEBASE_STRUCTURE_CORE.md:307` describes
  `tables::FlowEnvironment` as "Maps tracked values to their flow state", but the
  type is an O(1) `(Checkpoint, reachable)` snapshot (`tables.rs:27-33`); the
  structure reference should be corrected to match the code's own doc comment.
- **Redo reachability deserves a fixing test** even if READ-001's analysis is
  judged wrong: no current test performs a forward transition across a sibling
  branch containing a requirement/sink delta (`state/tests.rs` only restores to
  ancestors).

## Coverage

Inspected (definitions plus representative callers):

- `src/analysis/flow/projector/state.rs` — `InverseDelta::apply`, `FlowEvidence`,
  `ControlFrame`, `ControlStack`, `LoopSeed`, `ControlStackError`, `AbruptExit`.
- `src/analysis/flow/projector/state/tables.rs` — `FlowEnvironment`,
  `Canonical*`, `FlowSemanticSnapshot`, `FlowStateTable`, `StateAdmission`,
  mutation-log round trip, `semantic_snapshot`, `capture`/`restore`.
- `src/analysis/flow/projector/state/tables/aliases.rs` — `AliasTable`,
  `ObjectRefCounts`.
- `src/analysis/flow/projector/state/tables/updates.rs` — `PropertyWriteUpdate`.
- `src/analysis/flow/projector/state/tests.rs` — all table/stack/evidence tests.
- `src/analysis/flow/projector/{driver,control,loops,evidence,mod}.rs` — call
  sites for restore/admit/join/loop fixed point and direct-sink inlining.
- `src/analysis/flow/projector/transfer.rs` — `assign`/`match_source`.
- `src/analysis/flow/projector/history.rs` — `InverseDelta`, `MutationLog`,
  `Checkpoint`, `ReportEvidenceKey` (chunk 9 boundary).
- `src/analysis/flow/summary/{mod,parameter,sink,store,summaries}.rs` and their
  `*/tests.rs`.
- Supporting model/datastructure owners: `analysis/model/flow.rs` +
  `flow/state.rs`, `analysis/model/fact.rs` (`ParameterBinding`),
  `analysis/facts`, `api/classification.rs` (`RuleIndex`,
  `RuleEvidenceTable::record/mark_event_truncated`), and
  `glass-lint-datastructures/src/{history.rs,table.rs}` (`ParentLinkedHistory`,
  `IndexTable`).

Only this audit file was created; no source, test, configuration, or other
documentation was modified (`git status` clean apart from the new file).