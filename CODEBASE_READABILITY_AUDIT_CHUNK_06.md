# Codebase Readability Audit — Chunk 6

This audit covers Chunk 6 of `CODEBASE_STRUCTURE_CORE.md`: matching. It is an
architectural review only; no source changes were made.

## Summary

The matching layer has a strong ownership direction: facts are projected once
into typed occurrence indexes, project linking adds borrowed overlays, and
query evaluation consumes those views without mutating the local artifact.
Deterministic ordering, duplicate physical events, and unknown-identity
masking are explicitly modeled. The main readability risks are in the seams
between those owners. Call projection repeatedly reconstructs one fact shape,
overlay construction encodes policy in a boolean and parallel tuple, query
views repeatedly redispatch the same enum, constrained matching coordinates
several execution phases through storage-shaped arguments, and normal versus
constrained paths duplicate evidence-group construction.

## Findings

#### [ ] READ-001 — Call-fact projection repeatedly reconstructs one payload shape

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Conversion
- **Location:** `glass-lint-core/src/analysis/matching/build.rs:122-218`
- **Representative callers:** `OccurrenceIndexes::record_fact` dispatches to `record_call_fact`, which then calls `record_call_paths` and `record_call_special_cases`

The call projection path destructures `FactPayload::Call` three times. The
top-level helper extracts the callee name, callee span, and call provenance;
`record_call_paths` extracts syntactic/rooted/module/returned/instance paths;
and `record_call_special_cases` extracts the unwrap payload and span again.
Each helper also reconstructs an `Occurrence` from the same fact identity and
callee span while updating different physical indexes.

The split into path and special-case helpers is useful for navigation, but the
payload boundary is not owned by one representation. Adding a call payload
field or changing which span represents a call requires checking several
pattern matches and can make one index category observe a different shape from
the others. This is especially risky because the indexes are the shared
matcher model and must preserve all independent possible witnesses.

**Recommendation:** Add a private call-projection view or extraction method
owned by the matching projection that borrows all call fields once and carries
the canonical occurrence/span. Pass that view to focused index writers, or
make one `record_call` transition own the extraction and delegate only the
index-specific writes. Preserve separate syntactic, rooted, module, returned,
instance, unwrap, global, and local/unknown behaviors, including the current
fact/span identity and deterministic normalization.

**Fix Applied:** None so far.

#### [ ] READ-002 — Overlay remapping uses a boolean mode beside a parallel source tuple

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:67-100,147-192`
- **Representative callers:** `LinkedOccurrenceView::build` supplies five `(source, ModuleOverlayKind, promote_globals)` entries to `ModuleOccurrenceOverlay::remap`

`ModuleOccurrenceOverlay::remap` performs identity lookup, source masking,
external remapping, and optional global promotion. Whether globals may be
promoted is represented by a bare `bool`, while the overlay kind is a separate
enum and the source index is an untyped tuple element. Only ordinary module
calls pass `true`; member calls, reads, classes, and constructors pass
`false`.

The values are currently correct, but the API does not express why a source is
allowed to promote globals or prevent an invalid combination. Adding another
overlay source requires editing the tuple and remembering the policy bit.
That makes a subtle identity rule—only proven global call aliases join the
global-call view—depend on positional configuration rather than a domain
owner.

**Recommendation:** Replace the boolean/tuple contract with a private overlay
source descriptor or policy enum that owns the source index, overlay kind, and
global-promotion behavior. Let `remap` consume that named descriptor and
delete the parallel mode argument. Preserve masking for every resolved source,
fail-closed handling of ambiguous/unknown identities, the existing global-call
promotion restriction, and the bounded operation count.

**Fix Applied:** None so far.

#### [ ] READ-003 — `EventIndexView` redispatches capabilities across many matches

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / API
- **Location:** `glass-lint-core/src/analysis/matching/query/view.rs:21-300`; construction at `glass-lint-core/src/analysis/matching/query/mod.rs:160-190`
- **Representative callers:** `OccurrenceIndexes::occurrences_for_indexed` builds an `EventIndexView` and calls `EventIndexView::resolve`

The event-specific enum is a good way to prevent a query from seeing indexes
that do not apply to its event, but the view then redispatches its capabilities
through multiple independent matches. `resolve` dispatches the identity;
`resolve_any`, `resolve_rooted`, and literal helpers match the view again;
`global_index`, `member_path`, and `module_view` each repeat another variant
mapping. `module_view` also separately translates event variants into
`ModuleOverlayKind`, so the query view and linked overlay must stay aligned by
convention.

This is more than enum verbosity: adding an event or identity kind requires
editing several matches, and an omitted arm can silently return `None` for an
otherwise supported query. The repeated dispatch also obscures which
combinations are deliberately unsupported versus absent because an index was
not constructed.

**Recommendation:** Keep the zero-allocation, explicitly restricted event
views, but centralize their capabilities in one variant-local access object or
one preparation step that carries the applicable identity resolver, member
path, global index, module index, and overlay kind. Make unsupported
combinations explicit in that object rather than rediscovering them in several
helpers. Preserve borrowed lifetimes, package-pattern behavior, rooted global
object matching, and `None` for unsupported or unavailable identity paths.

**Fix Applied:** None so far.

#### [ ] READ-004 — Constrained matching coordinates preparation and two execution modes in one state protocol

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:198-376`
- **Representative callers:** `compute_constrained_evidence` is called by `analysis/project/projection.rs:246-256`; its inner coordinator builds `ConstrainedRoot` and `PreparedConstrainedRoot` values before invoking indexed and fallback passes

`compute_constrained_inner` unwraps the matcher artifact and project overlay,
constructs the evaluator, filters physical roots, prepares name paths, and
then hands a mutable vector of roots through two different execution modes.
`evaluate_indexed_roots` both resolves indexed candidates and publishes
evidence, while `evaluate_fallback_roots` scans every fact, stores occurrences,
and publishes them later. The protocol is split across `fallback` and
`occurrences` fields and several functions with long argument lists, so the
state transition from “prepared” to “indexed or fallback and published” is
implicit.

This coordination is the critical bounded-work boundary for argument
matching. It must preserve one preparation per candidate/group, exact
operation accounting, indexed candidates before fallback scanning, and no
evidence from unsupported argument shapes. A new execution mode or failure
state would have to thread another flag or mutable field through both passes.

**Recommendation:** Give the constrained path one private evaluation-state
owner that prepares roots and exposes explicit `Indexed`, `Fallback`, and
`Published` transitions, or return a named prepared-plan value consumed by
separate execution methods. Centralize evidence publication on that owner so
the indexed and fallback paths cannot diverge in rule identity, symbol, or
capacity handling. Preserve bounded fallback work, operation counters,
duplicate-constraint sharing, dynamic-value rejection, and the current
fail-closed behavior for missing effective arguments.

**Fix Applied:** None so far.

#### [ ] READ-005 — Normal and constrained matchers duplicate evidence-group construction

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / API
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:336-369`; `glass-lint-core/src/analysis/matching/arguments/mod.rs:186-196`; `glass-lint-core/src/api/classification.rs:254-275`; final merge at `glass-lint-core/src/analysis/project/projection.rs:321-337,594-615`
- **Representative callers:** `push_owned_evidence` publishes direct query evidence, while `push_owned_rule_evidence` publishes constrained evidence into `RuleEvidenceTable`

Both matching paths convert the same typed `Occurrence` values into
`ClassificationEvidenceOccurrence` values and then create a
`ClassificationEvidence` group. The direct path builds a vector and calls
`ClassificationEvidence::from_occurrences`; the constrained path converts the
occurrences and calls `RuleEvidenceTable::record_grouped`, which repeats the
empty check and the same constructor before recording by rule. The two paths
are later concatenated by `ProjectModuleProjection::evidence_for` and
normalized together, so the grouping contract is distributed across both
sinks.

The separate timing is justified: constrained and lifecycle evidence is
projected into a per-rule table while ordinary physical roots are queried on
demand. The raw occurrence-to-group transform and its non-empty invariant do
not need to be separate, however. Keeping them split makes certainty, total
count, truncation, and constructor failure behavior harder to keep aligned.

**Recommendation:** Introduce one private raw evidence-group value in
`analysis::matching::evidence`, or one matching-owned method that turns
occurrences plus `(MatchKind, symbol, certainty)` into a validated
classification group, then let both the vector sink and the per-rule table
consume it. Delete the duplicate `from_occurrences`/empty-check sequence while
retaining per-rule capacity errors, definite-versus-possible certainty,
physical duplicate counts, and the single final normalization and truncation
boundary.

**Fix Applied:** None so far.

## Systemic Themes

- **ENCAPSULATE:** Overlay policy and evidence-group validity should be owned by
  named domain transitions instead of booleans and sink-specific constructors.
- **SIMPLIFY:** Query-view dispatch and constrained evaluation expose several
  state protocols through repeated matches, flags, and mutable argument lists.
- **DEDUPLICATE:** Call-payload extraction and occurrence-to-evidence
  conversion are repeated at shared semantic boundaries.

## Open Questions

None recorded.

## Coverage

Reviewed occurrence storage and normalization, call/member/construction/literal
index construction, module identity overlays and masking, borrowed and
package occurrence iterators, query-facing event views, argument preparation
and identity evaluation, operation accounting, evidence accumulation, and
project-level evidence merging. The custom lazy iterators were not reported
separately: their base/overlay and package lifecycles are distinct and their
shared ordering contract is deliberately centralized at
`OccurrenceSelection::into_ordered`.

The unified raw evidence-group type belongs in `analysis::matching::evidence`.
That module already owns deterministic grouping and presentation limits, while
the classification API owns the validated public evidence value. Do not move
occurrence indexes, overlays, or their storage concepts into classification;
the one conversion between the two boundaries is the narrow ergonomic API.

## Handoff

Chunk 6 is complete. The next unreviewed chunk is **Chunk 7 — Project linking**
(`CODEBASE_STRUCTURE_CORE.md` lines 480-524), covering module identities,
resolution tables, linker state, and project semantic models.
