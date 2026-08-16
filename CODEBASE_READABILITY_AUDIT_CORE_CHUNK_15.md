# Codebase Readability Audit

Audit scope: Chunk 15 "Matching and argument evaluation" of `glass-lint-core`
(`src/analysis/matching/mod.rs`, `arguments/{mod, evaluator, identity}.rs`,
`build.rs`, `evidence.rs`, `identity_map.rs`, `indexes.rs`). Read-only; no
source changed.

## Summary

Matching is a coherent pipeline where it matters. The fact-to-occurrence
projection is the single matcher-independent path (`build.rs::from_stream`,
"the sole projection from semantic facts into shared matcher indexes"), built
and normalized before rule selection. The overlay/identity machinery is
well-owned: `ModuleIdentityMap` plus `ModuleIdentityContributions` keep the
star-vs-direct disagreement policy in one place; `LinkedOccurrenceView` hides
masking/remapping behind a build-then-resolve contract; and `MatcherArtifact`,
`MatcherProjectOverlay`, and `MatcherProjectContext` each carry a written
invariant that keeps one fact stream, its indexes, and one project's identity
maps together. `EvidenceGroup` and `normalize_evidence` are one shared
occurrence-to-evidence boundary used by both the direct and constrained paths.
That structure is good and is not re-reported here.

The problems concentrate in the constrained-evaluation preparation:

1. **A second copy of the project overlay.** `EffectiveIdentityResolver` stores
   exactly the same two option-references as `MatcherProjectOverlay` and is
   populated by a field-for-field `From` conversion (READ-001). One
   representation, two types.
2. **Accounting computed in production and read only in tests.**
   `EvaluationOperations` is charged on every candidate/group/predicate and
   then discarded by `try_compute_constrained_evidence`; `ProjectionOutcome`
   totals overlay and flow operations but never these (READ-002).
3. **Clause data re-shaped and then re-threaded.** `ConstrainedRootInput` is
   immediately flattened into `ConstrainedRoot`, and every evaluation loop
   re-destructures `root.identity`, `root.event`, `root.constraints` and
   `prepared_root.paths` into a six-parameter predicate call (READ-003).
4. **The same call event can emit two different evidence spans.** Indexed call
   occurrences use `callee_span`; the linear fallback path builds occurrences
   from `fact.span`, so evidence locations depend on which evaluation path a
   root took (READ-004).
5. Two smaller papers: `MatcherArtifact::from_facts` is handed the full project
   overlay but reads only `identities` (READ-005), and the four index groups
   are parallel but carry inconsistent `Clone` derives while nothing clones
   them (READ-006).

On the task's specific questions: the `Matcher*`/`Overlay` family is a coherent
per-module preparation pipeline, not an over-split — the real duplication is
`EffectiveIdentityResolver` and the `ConstrainedRootInput` to `ConstrainedRoot`
double shape. Evidence accumulation is not duplicated with
`flow::cross::evidence` (per-fact trace merging and `EvidenceKey` include a
`FactId`) or `project::types::report::evidence` (the report serialization
model); both build on the same `ClassificationEvidence` API, and the shared
normalization runs once at the report boundary.

Implementation order: READ-001 first (deletes a type; call-site ripple confined
to `evaluator.rs`), then READ-002, READ-004, READ-006 (small, independent),
then READ-003 (largest surface, touches both evaluation loops and the
preparation types), then READ-005 (last; depends on which overlay accessors
survive).

## Findings

### Constrained evaluation preparation (`arguments/mod.rs`, `arguments/evaluator.rs`)

#### [ ] READ-001 — `EffectiveIdentityResolver` is a field-for-field copy of `MatcherProjectOverlay` joined by a one-way `From`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:94-155`; `arguments/mod.rs:225-229,265-275`

`MatcherProjectOverlay<'a>` (`arguments/mod.rs:226-229`) holds
`identities: Option<&ModuleIdentityMap>` and
`result_identities: Option<&BTreeMap<ValueId, ExportResolution>>`.
`EffectiveIdentityResolver<'a>` (`evaluator.rs:103-106`) holds exactly those two
fields, and `From<MatcherProjectOverlay> for EffectiveIdentityResolver`
(`evaluator.rs:108-115`) copies them verbatim. The precedence behavior
(`effective_identity`: call-result identity, then module identity, then local
value) and the `static_string`/`call_provenance` lookups are real domain
operations, but `EffectiveIdentityResolver` exists only to become
`MatcherEvaluator.identity` (`evaluator.rs:97`). Any change to the overlay's
shape must now be mirrored in the resolver.

**Recommendation:** Delete `EffectiveIdentityResolver` and its `From` impl, and
implement `module_identity`, `result_identity`, `effective_identity`,
`static_string`, and `call_provenance` directly on `MatcherProjectOverlay`
(already `Copy`, built at the matching boundary, and passed by value to
`MatcherEvaluator::new`). `MatcherEvaluator` stores `MatcherProjectOverlay<'a>`
instead. Guardrail: preserve the precedence order (call-result identity, then
module identity, then the local `Value`) and the
`unwrap_or_else(|| raw.clone())` fallback of `call_provenance`; keep
`MatcherProjectOverlay::new` unchanged so `projection.rs` call sites are
untouched.

#### [ ] READ-002 — `EvaluationOperations` is charged in production and its result is dropped

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:290-298,311-335`; `arguments/evaluator.rs:53-92`; `project/projection.rs:171-173`; `project/projection/outcome.rs:157,167`

`try_compute_constrained_evidence` creates `EvaluationOperations::default()`,
passes it through `compute_constrained_inner`, and drops it — the counter is
unread by any production consumer. Charging happens on every candidate, group,
predicate, argument preparation, and value resolution inside
`MatcherEvaluator::fact_matches_clause`/`constraints_match`. `ProjectionMetrics.operations`
totals only overlay construction (`projection.rs:173`) and local/cross flow
operations (`outcome.rs:157,167`), so constrained-evaluation work is never
counted. The only reader is the test shell `run_with_ops`
(`arguments/tests.rs:87-105`), which bypasses the production entry point to
observe the counters.

**Recommendation:** Either (a) return `EvaluationOperations` from
`try_compute_constrained_evidence` and fold it into
`outcome.metrics.operations` next to the existing `overlay_ops` addition in
`project_modules` (`projection.rs:171-173`) — the honest reading of the
"deterministic operation counts" invariant in ARCHITECTURE.md — or (b) mark the
`charge_*` bodies `#[cfg(test)]` so the production build stops doing accounting
work it never uses. Guardrail: keep the exact candidate, group, predicate,
preparation, and value-resolution totals the extended tests assert
(`arguments/tests/extended.rs:141-145`); if (a), use the same per-module
saturating-add pattern as `overlay_ops`, and keep the count deterministic under
rule ordering.

#### [ ] READ-003 — Prepared root clause fields are re-shaped once, then re-threaded through a six-parameter predicate

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:31-84,339-410`; `arguments/evaluator.rs:170-223`

`PreparedConstrainedRoot::from_input` (`arguments/mod.rs:63-84`) takes
`ConstrainedRootInput` (a `RuleIndex` + `&PhysicalRoot` carrier built once in
`ProjectionPlan::from_selection`), flattens the `PhysicalRoot::ConstrainedScan`
arms into a second struct `ConstrainedRoot` (`rule`, `identity`, `event`,
`constraints`, `evidence`), and separately derives `PreparedClausePaths`. Every
evaluation use then re-destructures those fields: both loops read
`root.identity`, `root.event`, `root.constraints` and pass them alongside
`&prepared_root.paths` into `MatcherEvaluator::fact_matches_clause(fact,
identity, event, constraints, paths, ops)` (`evaluator.rs:170-177`). The data
already has one owner — `PreparedConstrainedRoot` holds `root` and `paths`
together — but the match predicate is written as an external function that must
be fed the same fields at every call site, and the carrier struct's content is
copied into a second struct before it is even read.

**Recommendation:** Move the clause match-check onto the prepared root, e.g.
`PreparedConstrainedRoot::matches(&self, fact: &SemanticFact,
evaluator: &MatcherEvaluator<'_>, ops: &mut EvaluationOperations) -> bool`,
delegating identity/constraint evaluation to the evaluator while receiving
`self.root` and `self.paths` without field forwarding. Collapse
`ConstrainedRootInput`/`ConstrainedRoot` into one prepared shape. Guardrail:
preserve the `Indexed → Fallback → Published` state machine, the rule that a
root that resolved candidates never falls back, and the constraint short-circuit
order (groups iterate with `all`, predicates within a group with `all`).

#### [ ] READ-004 — Fallback evidence uses `fact.span` while every indexed call occurrence uses `callee_span`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:402`; `build.rs:34-56,177-199`; `facts/calls/mod.rs:32-40,98`; `analysis/model/fact.rs:293-295`

The matcher index records call occurrences with the callee token span:
`CallProjection::occurrence` builds `Occurrence::new(id, callee_span)`
(`build.rs:53-56`) and `record_call_fact` uses it for every call-backed index
entry (`build.rs:181-213`). The constrained fallback path builds occurrences
from the whole call-expression span: `Occurrence::new(fact.id, fact.span)`
(`arguments/mod.rs:402`), where `call.span` covers the entire `CallExpr`
(`facts/calls/mod.rs:32-40,98`) and `callee_span` is the callee expression
(`model/fact.rs:293-295`). Because fallback is the only path for identities an
index view cannot express (for example `IdentityConstraint::Rooted` on a
`Call` event, which `EventIndexView::rooted` declines — `query/view.rs:146-173`),
the same `fetch('/api')` event reports a whole-call span under one rule and a
callee span under another, and both flows feed the same
`EvidenceGroup`/`normalize_evidence` pipeline.

**Recommendation:** Extract one occurrence-from-fact constructor shared by the
projection and the fallback scanner (for example an
`Occurrence::for_call_event`-style factory or a `matching`-scoped helper) and
use it in `evaluate_fallback_roots` so a given fact always yields the same
span. Guardrail: keep the caliber of indexed behavior unchanged (calls use the
callee span, member reads use the fact span), preserve deterministic
(span, fact) ordering, and keep the constraints-only fallback semantics (the
scan must still emit no occurrences for facts that fail `fact_matches_clause`).

#### [ ] READ-005 — `MatcherArtifact::from_facts` is handed the full project overlay but reads only `identities`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:158-185,242-263`

`MatcherArtifact::from_facts(facts, project: MatcherProjectOverlay, policy)`
receives a two-field overlay and inspects exactly one field
(`project.identities`, `arguments/mod.rs:167`); `result_identities` is needed
only later by the constrained evaluator. `MatcherProjectContext::from_facts`
forwards the whole overlay to it (`arguments/mod.rs:243-250`). The reader must
hold the whole overlay in mind to see that the artifact build does not depend
on call-result identities, and a future `from_facts` change could silently start
depending on the second field.

**Recommendation:** Narrow `from_facts` to
`Option<&ModuleIdentityMap>` (or have the caller pass `project.identities()`
through a narrow accessor) so the preparation step declares its actual
dependency, keeping `MatcherProjectContext` as the owner that pairs artifact and
overlay. Guardrail: the `Disabled`/`Enabled` policy, the
`facts.matcher_index().is_available()` gate, and the returned `(artifact,
operations)` pair must keep their current shape and accounting.

### Occurrence indexes and evidence (`indexes.rs`, `build.rs`, `evidence.rs`, `identity_map.rs`)

#### [ ] READ-006 — The four index groups carry inconsistent `Clone` derives that no caller uses

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/matching/indexes.rs:8,60,183,279`; `arguments/mod.rs:33-41`

`MemberIndexes`, `ConstructionIndexes`, and `LiteralIndexes` are
`#[derive(Clone, Debug, Default)]`, while the parallel `CallIndexes` is only
`#[derive(Debug, Default)]` — and the owning `OccurrenceIndexes` is not `Clone`.
A search of all callers finds no `.clone()` of any group or its `Occurrence`
collections; `LinkedOccurrenceView::build` and every query path borrow. The
derives are stale and inconsistent, so a reader cannot tell which is canonical,
and each `#[derive(Clone)]` on a container of `BTreeMap`s invites accidental
deep-copies of the matcher indexes.

**Recommendation:** Drop `Clone` from the three groups (or, if a copy is ever
intended, add it to all four plus `OccurrenceIndexes` deliberately). Guardrail:
the normalization contract stays on `OccurrenceIndexes::from_stream` → per-group
`normalize`; no caller currently needs an owned index copy, so keep the shared
artifact immutable and borrowed.

## Systemic Themes

- **One representation, two names.** `EffectiveIdentityResolver` copies
  `MatcherProjectOverlay`'s fields and is fed by a mechanical `From`
  (READ-001); `ConstrainedRootInput` is likewise re-shaped into
  `ConstrainedRoot` before any use (READ-003). Both are instances of the
  prepare-without-behavior double shape: a type that only carries data, copied
  into a sibling that only carries data.
- **Production-accounted, test-consumed.** `EvaluationOperations` is charged on
  the hot path and read only by a test helper that opens an inner function
  (READ-002). The same inner/outer split appears in `compute_constrained_evidence`
  vs `try_compute_constrained_evidence`, where only the test variant's
  accounting escapes.
- **Same event, path-dependent evidence.** The fallback scanner and the index
  projection disagree on call spans (READ-004); both write into the same
  `ClassificationEvidence` stream, so evidence appears path-dependent.

## Open Questions

- Are two `EvidenceKey` structs — `matching::evidence::EvidenceKey(MatchKind,
  String)` and `flow::cross::evidence::EvidenceKey(MatchKind, String, FactId)` —
  justified by the different lifetimes (normalization-time grouping vs
  per-fact trace assembly), or should the per-fact variant be a distinct name to
  avoid the collision risk? No finding: merging them would couple distinct
  normalization and flow-trace lifecycles.
- `ModuleIdentityMap::insert` lets any caller (currently only
  `project/identities.rs`) bypass the star-vs-direct disagreement policy that
  `merge_star_from`/`merge_missing_from` encode; the "single source of truth"
  doc on `ModuleIdentityContributions` (identity_map.rs:51-58) covers the
  policy between those types but not the raw-insert escape hatch. Is a document
  comment or a dedicated `record_direct` method warranted?
- Is the fallback scanner reachable for `Rooted`-on-`Call` roots in the shipped
  catalogs, or only for future/foreign queries? (The code path exists and
  renders the READ-004 span divergence observable on any such root.)
- Does the `Clone` on `MemberIndexes`/`ConstructionIndexes`/`LiteralIndexes`
  exist as intended state for an upcoming owned-overlay step, or is it leftover?

## Coverage

Files read: `matching/mod.rs`, `matching/arguments/{mod,evaluator,identity}.rs`,
`matching/arguments/tests.rs`, `matching/arguments/tests/extended.rs`,
`matching/build.rs`, `matching/evidence.rs`, `matching/evidence/tests.rs`,
`matching/identity_map.rs`, `matching/identity_map/tests.rs`, `matching/indexes.rs`,
`matching/occurrence.rs`, `matching/occurrence/storage.rs`, `matching/query/mod.rs`,
`matching/query/view.rs`, `analysis/model/fact.rs`, `analysis/facts/mod.rs`,
`analysis/facts/calls/mod.rs`, `analysis/project/projection.rs`,
`analysis/project/projection/outcome.rs`, `analysis/project/identities.rs`,
`analysis/flow/cross/evidence.rs`, `project/types/report/evidence.rs`,
`lint/report/evidence.rs`, `api/classification.rs`.

Consumers traced: `OccurrenceIndexes::from_stream` (facts/mod.rs:401),
`MatcherProjectContext`/`MatcherProjectOverlay`/`try_compute_constrained_evidence`
and overlay accounting (project/projection.rs:156-200,242-274),
`evidence_for_indexed_with_overlay` + `normalize_evidence` at the report
boundary (project/projection.rs:462), `display_span` (lint/report/evidence.rs:263),
`ModuleIdentityContributions` policy use (project/identities.rs:192-217).

Checked and left clean: `EvidenceGroup` (single-field validated newtype with a
real invariant), `EvidenceAccumulator`/`EvidencePresenter`
(normalization/truncation policy with deterministic ordering),
`ModuleIdentityMap`/`ModuleIdentityContributions` merge policy,
`LinkedOccurrenceView` build/resolve masking contract, `prepared`-once
`PreparedClausePaths`, the `ConstrainedState` `Indexed/Fallback/Published`
state machine, and the projection eager-vs-lazy income decisions.
`publish_fallback`'s defensive non-fallback arm (`arguments/mod.rs:124-127`) is
unreachable today but was left as a guardrail note, not a finding.

`git status` re-checked; the only new file is this audit document.