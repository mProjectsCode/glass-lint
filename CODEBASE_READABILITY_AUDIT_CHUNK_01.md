# Codebase Readability Audit

## Summary

Chunk 1 owns the one-pass source fact builder, its bounded provenance state,
the phase-typed fact stream, and the module-interface side effects collected
while traversing the AST. The overall ownership direction is sound: facts are
constructed once and the stream is frozen before indexing. The main
readability and API risks are concentrated in protocol boundaries inside that
walk: module recognition is performed twice, transaction state is not encoded
in checkpoint handles, and the provenance owner repeats channel-specific
bookkeeping that obscures intentionally asymmetric branch semantics.

## Findings

### Module-request recognition and call emission

#### [x] READ-001 — Make one canonical module-call observation drive interface and fact output

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:397-415`; `glass-lint-core/src/analysis/facts/calls/mod.rs:15-73`; `glass-lint-core/src/analysis/facts/calls/wrapper.rs:995-999`

`FactBuilder::record_module_call_request` recognizes a call with the interface
policy and mutates `ModuleInterfaceBuilder`, but returns only a special
`(String, Span)` pair for dynamic imports. `record_call_expr` then calls
`emit_require_import`, which invokes `Resolver::require_module_name` and runs
module recognition again for direct `require` calls. The same semantic event is
therefore split across a side effect, an ad-hoc tuple, and a second policy
path, making policy drift and ordering changes difficult to detect.

**Recommendation:** Have the first recognition produce one internal,
typed module-call observation (or an equivalent enum) that the caller
consumes to record both the interface request and the `Import` fact. Remove the
dedicated second recognition path and its forwarding helper once all callers
use the canonical result. Keep the interface policy's dynamic-import and
single-argument rules, keep wrapped `require` distinct from direct `require`,
and preserve child-visitation and fact insertion order.

**Fix Applied:** A typed `ModuleCallObservation` now comes from the interface
recognizer and is consumed for both module-interface recording and `Import`
fact emission. The second direct-`require` recognition and forwarding helper
were removed; wrapped requires remain distinct, dynamic imports and
single-argument requires keep their policies, and child/fact ordering is
preserved. Verified with `make fmt && make ci`.

### Bounded provenance transaction state

#### [x] READ-002 — Encode checkpoint ownership and restoration lifecycle in the provenance API

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/origin_map.rs:32-122`; `glass-lint-core/src/analysis/facts/control.rs:141-169`

`OriginMap::restore_snapshot` replaces the map and clears `log` and
`open_checkpoints`, but active `OriginCheckpoint` values remain marked active
because the method receives an owned snapshot rather than the checkpoint
handles. `record_try` restores a snapshot and later finishes the original
checkpoint, so the correctness of `commit`/`rollback` depends on their
current saturating counter behavior and on the caller knowing that the
checkpoint was silently invalidated. The API also permits `restore` to reset a
checkpoint without closing it, leaving two meanings of “restored” in adjacent
methods.

**Recommendation:** Give the transaction owner one explicit lifecycle model:
either make snapshot restoration illegal while checkpoints are open, or return
a new transaction state/handle that invalidates and replaces the old handles.
Keep ordinary branch rollback, nested checkpoints, and full join snapshots as
separate named operations so callers cannot accidentally continue a stale
journal. Preserve bounded logging and semantic-budget charging, and retain the
fact builder's distinction between instance-origin and class-origin merge
rules.

**Fix Applied:** Full snapshot restoration is now an explicitly owned
`restore_snapshot` operation: the caller supplies the active checkpoint, which
is rebased to the replacement journal and remains active for its later commit
or rollback. `record_try` threads that provenance checkpoint through every
instance snapshot restore, and a focused unit test verifies balanced lifecycle
accounting. Verified with `make fmt && make ci`.

### Fact provenance state

#### [x] READ-003 — Consolidate repeated provenance-channel operations without erasing their asymmetry

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:74-208`

`FactProvenanceState` stores two `OriginMap<(SmolStr, SmolStr)>` channels and
then duplicates checkpoint, snapshot, common-intersection, restoration, and
replacement plumbing for `instance_origins` and `class_origins`. The repeated
`replace_targets` sequence also clears and conditionally reinserts four
independent stores for every target. This forces readers to compare parallel
methods to discover the important exception in `finish_control_region`, where
instance origins are committed but class origins are rolled back.

**Recommendation:** Keep `FactProvenanceState` as the owner, but introduce a
small internal channel aggregate or domain operation that performs the common
target replacement and paired branch bookkeeping once. Make the asymmetric
operations explicit at that owner rather than exposing separate forwarding
methods for every map. Do not merge instance and class origins into one
uncertainty state: loops, branches, try/catch/finally, and class provenance
must retain their current fail-closed behavior.

**Fix Applied:** A private `OriginChannels` aggregate now owns paired
instance/class checkpointing, branch snapshots, intersections, restoration,
and target replacement. The channel owner keeps try-only instance operations
and the instance-commit/class-rollback control-region asymmetry explicit, while
callers no longer duplicate map plumbing. Verified with `make fmt && make ci`.

### Construction fact visitor

#### [ ] READ-004 — Separate construction resolution, child traversal, and fact emission phases

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/visitor.rs:196-255`

`visit_new_expr` simultaneously allocates and records instance provenance,
resolves the effective callee, derives rooted and module provenance, branches
on callee syntax to choose names, recursively visits children, normalizes the
callee span, interns a name, and emits the final construction fact. The
ordering is semantically significant—provenance is prepared before children,
while the final fact is emitted after them—but the mixed levels make that
protocol hard to audit and make future changes likely to duplicate resolution
or move a side effect across the traversal boundary.

**Recommendation:** Split the function into cohesive internal phases with
names that state the ordering, such as construction metadata resolution,
construction-child traversal, and construction-fact emission. Pass a compact
resolved metadata value between phases so span normalization and name
interning remain owned by `FactBuilder`; do not add a second AST traversal.
Preserve the fresh result identity, superclass/module provenance rules,
source-order fact placement, and fail-closed behavior when spans or names
cannot be represented.

**Fix Applied:** None so far.

## Systemic Themes

- The canonical one-pass traversal is a valuable architectural invariant; the
  recommended changes should clarify its phases rather than split rule-specific
  traversals or create parallel semantic models.
- Internal APIs frequently pass raw tuples and `Option` fields across adjacent
  fact-building phases. Typed observations and lifecycle handles would make
  uncertainty and ownership visible without exposing SWC or storage details.
- Boundedness and precision are already explicit in the code. Refactors must
  preserve deterministic order, budget charging, independent possible
  witnesses, and the distinct instance/class provenance rules.

## Decisions

- Keep the fact stream's issue set typed and private, but do not expose a
  generic incompleteness summary to lowering. `LoweringCompletionPolicy` must
  preserve precedence between fact capacity, path capacity, resolver arena
  exhaustion, structural invalidity, parser spans, and name exhaustion; those
  checks also depend on resolver state and configured limits. A future narrow
  `FactStream::completion_issue()` may consolidate only the stream-owned
  precedence, while the lowering policy remains the owner of the combined
  `IncompleteReason` decision. This resolves the question without collapsing
  distinct fail-closed reasons into a boolean or opaque aggregate.

## Coverage

Reviewed the Chunk 1 source-fact construction area described in
`CODEBASE_STRUCTURE_CORE.md`, including `facts/mod.rs`, `stream.rs`,
`origin_map.rs`, `visitor.rs`, argument/pattern/assignment lowering, call and
callee lowering, control markers, function/instance state, and module
interface construction. Representative downstream callers in lowering, flow,
and project projection were inspected to validate ownership and API impact.

Only this audit file was changed. No source, test, configuration, dependency,
or other documentation changes were made.

## Handoff

Chunk 1 is reviewed. The next unreviewed work is **Chunk 2: Scope, syntax, and
evidence frontend**. Continue with `CODEBASE_STRUCTURE_CORE.md:61-178`, create
`CODEBASE_READABILITY_AUDIT_CHUNK_02.md`, and preserve this file as historical
evidence rather than re-reporting its unchecked findings.
