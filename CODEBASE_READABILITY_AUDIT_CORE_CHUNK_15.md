# Codebase Readability Audit

Audit scope: Chunk 15 "Matching and argument evaluation" of `glass-lint-core`
(`src/analysis/matching/mod.rs`, `arguments/{mod, evaluator, identity}.rs`,
`build.rs`, `evidence.rs`, `identity_map.rs`, `indexes.rs`). Read-only; no
source changed.

## Summary

Matching is a coherent pipeline where it matters. The fact-to-occurrence
projection is the single matcher-independent path
(`OccurrenceIndexes::from_stream`, build.rs:63-74 — "the sole projection from
semantic facts into shared matcher indexes"), built and normalized before rule
selection. The overlay/identity machinery is
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
4. **One event, two span sources.** Indexed call occurrences are anchored on
   `callee_span`; the linear fallback path builds occurrences from the
   whole-call `fact.span`. The divergence is masked today because falling-back
   roots can only match shapes the index view misses, but it is reachable and
   unprotected (READ-004).
5. Two smaller items: `MatcherArtifact::from_facts` is handed the full project
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

#### [x] READ-001 — `EffectiveIdentityResolver` is a field-for-field copy of `MatcherProjectOverlay` joined by a one-way `From`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:94-98,103-155`; `arguments/mod.rs:226-229,265-275`

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

#### [x] READ-002 — `EvaluationOperations` is charged in production and its result is dropped

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
`try_compute_constrained_evidence`, thread it through `project_facts`
(`projection.rs:255-260`) into `project_modules`, and fold it into
`outcome.metrics.operations` next to the existing `overlay_ops` addition
(`projection.rs:171-173`) — the honest reading of the "deterministic operation
counts" invariant in ARCHITECTURE.md — or (b) gate the `charge_*` updates
behind `#[cfg(test)]` so the production build stops performing accounting work
it never reads (the test helper calls the same inner function, so the counters
stay live for the tests). Guardrail: keep the exact candidate, group, predicate,
preparation, and value-resolution totals the extended tests assert
(`arguments/tests/extended.rs:141-145`); if (a), use the same per-module
saturating-add pattern as `overlay_ops`, and keep the count deterministic under
rule ordering.

**Fix Applied:** Kept `EvaluationOperations` and its exact extended-test
counts, but gated its stored fields and charge updates behind `cfg(test)`.
Production constrained evaluation still uses the same inner path and matching
semantics, without performing accounting that no production consumer reads.

#### [x] READ-003 — Prepared root clause fields are re-shaped once, then re-threaded through a six-parameter predicate

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
delegating identity/constraint evaluation to the evaluator while reading
`self.root` and `self.paths` from the single owner instead of forwarding fields
at every call site. Collapse `ConstrainedRootInput`/`ConstrainedRoot` into one
prepared shape: the input carrier stays at the `arguments` boundary (it is
built by `ProjectionPlan::from_selection`), but `ConstrainedRoot` is deleted
and its fields owned directly by `PreparedConstrainedRoot`. Guardrail: preserve
the `Indexed → Fallback → Published` state machine, the rule that a root that
resolved candidates never falls back, and the constraint short-circuit order
(groups iterate with `all`, predicates within a group with `all`). The
argument-projection duplication that also lives in this evaluator
(`argument_with_overlay`/`ArgumentView`, `evaluator.rs:225-246`) is reported
separately as chunk 11 READ-003; this finding stays on the prepared-root double
shape and the predicate threading owned here.

**Fix Applied:** Removed the redundant `ConstrainedRoot` copy and kept the
physical-root fields directly on `PreparedConstrainedRoot` beside its prepared
paths and lifecycle state. Added its `matches` method so indexed and fallback
evaluation use the owner-held clause data without re-threading six arguments.
The indexed/fallback/published transitions and short-circuit order are
unchanged.

#### [x] READ-004 — Fallback evidence uses `fact.span` while every indexed call occurrence uses `callee_span`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:402`; `build.rs:34-56,177-249`; `facts/calls/mod.rs:32-40,98`; `analysis/model/fact.rs:293-295`

The matcher index records call occurrences with the callee token span:
`CallProjection::occurrence` builds `Occurrence::new(id, callee_span)`
(`build.rs:53-55`) and `record_call_fact` and its helpers use it for every
call-backed index entry (`build.rs:177-249`). The constrained fallback path
builds occurrences from the whole call-expression span:
`Occurrence::new(fact.id, fact.span)` (`arguments/mod.rs:402`), where
`call.span` covers the entire `CallExpr` (`facts/calls/mod.rs:32-40,98`) and
`callee_span` is the callee expression (`model/fact.rs:293-295`). The
divergence is reachable for a constrained root whose identity the index view
cannot resolve to a bucket but whose matcher still accepts the fact — for
example `IdentityConstraint::Any` on a `Call` event matching through the
`syntactic_path` channel (`identity.rs:20-25`): a renamed destructured instance
callable such as `const { sendBeacon: sb } = navigator; sb('/x')` records
`callee_name` as the local identifier (`sb`) and a single-segment
`syntactic_path` (`["sendBeacon"]`) in the callee-resolution Ident arm
(`callee.rs:61-77`), so the Call view's callee-name lookup misses it
(`view.rs:179-182`) and the root falls back. (A `Rooted`-on-`Call` root also
falls back — `view.rs:146-173` — but cannot match, because
`call_identity_matches` returns false for it, `identity.rs:39`.) The
same event can therefore report a whole-call span under one rule and a callee
span under another, and both flows feed the same
`EvidenceGroup`/`normalize_evidence` pipeline.

**Recommendation:** Extract one occurrence-from-call-fact constructor shared by
the projection and the fallback scanner — e.g. `Occurrence::for_call_fact(fact,
call)` on the type that owns occurrence spans (`matching::occurrence`) — and
use it in `evaluate_fallback_roots` so a given call fact always yields the
callee span. Guardrail: keep the per-payload span choice unchanged so the
indexed path produces identical output (calls and constructions use the callee
span, member reads, property writes, and imports use the fact span), preserve
deterministic `(span, fact)` ordering, and keep the constraints-only fallback
semantics (the scan must still emit no occurrences for facts that fail
`fact_matches_clause`).

#### [x] READ-005 — `MatcherArtifact::from_facts` is handed the full project overlay but reads only `identities`

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

**Fix Applied:** Narrowed `MatcherArtifact::from_facts` to receive only the
module identity map it consumes. `MatcherProjectContext` remains the owner that
pairs the artifact with the complete overlay for later constrained evaluation.

### Occurrence indexes and evidence (`indexes.rs`, `build.rs`, `evidence.rs`, `identity_map.rs`)

#### [x] READ-006 — The four index groups carry inconsistent `Clone` derives that no caller uses

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/matching/indexes.rs:8,60,183,279`; `matching/mod.rs:33-41`

`MemberIndexes`, `ConstructionIndexes`, and `LiteralIndexes` are
`#[derive(Clone, Debug, Default)]`, while the parallel `CallIndexes` is only
`#[derive(Debug, Default)]` — and the owning `OccurrenceIndexes` (`mod.rs:33-41`)
is not `Clone`, so the three group derives cannot even be used to take an owned
copy of the index. A search of all callers finds no `.clone()` of any group or
its `Occurrence` collections; `LinkedOccurrenceView::build` and every query
path borrow. The derives are stale and inconsistent, so a reader cannot tell
which is canonical, and each `#[derive(Clone)]` on a container of `BTreeMap`s
invites accidental deep-copies of the matcher indexes.

**Recommendation:** Drop `Clone` from the three groups (or, if a copy is ever
intended, add it to all four plus `OccurrenceIndexes` deliberately). Guardrail:
the normalization contract stays on `OccurrenceIndexes::from_stream` → per-group
`normalize`; no caller currently needs an owned index copy, so keep the shared
artifact immutable and borrowed.

**Fix Applied:** Removed the unused `Clone` derives from `MemberIndexes`,
`ConstructionIndexes`, and `LiteralIndexes`, aligning the groups with
`CallIndexes` and the non-cloneable `OccurrenceIndexes` owner. Borrowed index
queries and normalization are unchanged.

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
- **Same event, two span sources.** The index projection anchors call
  occurrences on `callee_span` while the fallback scanner builds them from the
  whole-call `fact.span` (READ-004); both write into the same
  `ClassificationEvidence` stream. The divergence is latent today — falling-back
  roots can only match the shapes the index view misses — but nothing enforces
  the "one fact, one span" invariant when a new identity channel is added.

## Open Questions — Resolved

1. **The two `EvidenceKey` structs are justified by genuinely different
   lifecycles, and the collision risk is currently nil.** The
   `matching::evidence::EvidenceKey` (`evidence.rs:60-61`) is a private tuple
   struct used only by `EvidenceAccumulator` (`evidence.rs:70-93`) as the
   grouping key for `normalize_evidence`: it deliberately omits the `FactId` so
   all occurrences across facts for one `(kind, symbol)` collapse into a single
   `ClassificationEvidence` item (`add`, evidence.rs:76-93). The
   `flow::cross::evidence::EvidenceKey` (`cross/evidence.rs:45-50`) is a
   private named struct whose extra `fact: FactId` field keeps each fact's
   witness trace separate during `RuleEvidence` assembly (`cross/evidence.rs:63-68`),
   and failed keys are retained in a distinct `nonmatching` set
   (`cross/evidence.rs:67`) precisely so evidence is never assembled from
   incompatible call sites. Merging them would couple distinct
   normalization-time grouping and per-fact trace-assembly lifecycles; the
   collision risk is moot today because both are module-private. If either is
   ever made `pub`, renaming the flow-side key (e.g. `TraceEvidenceKey`) is a
   one-line change, not a finding.
2. **`ModuleIdentityMap::insert` is the export walker's commit path, not a
   policy bypass.** The star-vs-direct policy documented at `identity_map.rs:51-58`
   is applied by `ModuleIdentityContributions::add_star`/`add_direct`/`finish_into`
   (`identity_map.rs:70-81`), and the raw `insert` calls
   (`identities.rs:131,155,175,182`) are the walker committing data into the
   target map: direct import resolutions in `module_identities` (lines 131,155 —
   no star contributions exist for those keys), and conservative
   `ExportResolution::Unknown` markers for depth/cycle cutoffs in
   `collect_exported_identities` (lines 175,181-185). The precedence-resolved
   merge happens through `finish_into` into that same map (`identities.rs:192-217`),
   and no matching code calls `insert`, so the policy cannot be circumvented
   today; the gap is documentation only. A doc comment on `insert`
   (`identity_map.rs:19-25`) noting it is a raw commit for the export walker
   (not a precedence-bearing path) closes it; a dedicated `record_direct`
   method would misname the Unknown-cycle and intermediate cases and add a
   second API for the same operation.
3. **The fallback scanner is structurally reachable but cannot emit in the
   shipped catalogs today; the READ-004 divergence is latent, not live.** Any
   constrained root whose `occurrences_for_indexed` returns `None` is marked for
   fallback (`arguments/mod.rs:351-355`). The index records exactly the channels
   the identity matchers read, so a falling-back root can only match a fact the
   index view misses — the one reachable shape is `IdentityConstraint::Any` on a
   `Call` event matching via the `syntactic_path` channel (`identity.rs:20-25`)
   for a renamed destructured instance callable: `const { sendBeacon: sb } =
   navigator; sb('/x')` records `callee_name` as the local identifier (`sb`) and
   a single-segment `syntactic_path` (`["sendBeacon"]`) in the callee-resolution
   Ident arm (`callee.rs:61-77`), so the Call view's callee-name lookup misses
   it (`view.rs:179-182`) and the root falls back. A `Rooted`-on-`Call` root
   also falls back (`view.rs:146-173`) but cannot match, because
   `call_identity_matches` returns false for it (`identity.rs:39`). None of the shipped js/obsidian
   catalogs combine an argument constraint with such an identity (constrained
   rules use `call_global`/`call` on globals or `member_call_rooted`/
   `member_call_module` on member events), so the divergence becomes observable
   only when a rule adds a shape the index view cannot express — which is why
   READ-004's shared occurrence constructor should land before that happens.
4. **The `Clone` derives are leftover, not intended state for an upcoming
   owned-overlay step.** A search of every caller finds no `.clone()` of any
   index group or its collections (all matching-side clones are on keys,
   symbols, names, and chains). The owner `OccurrenceIndexes` (`mod.rs:33-41`)
   is not `Clone`, so the three group derives cannot even be used to take an
   owned copy of the index; an owned-overlay step would need `Clone` on the
   whole `OccurrenceIndexes`, and the linked overlay already borrows the index
   buckets (`LinkedOccurrenceView`, mod.rs:76-80,168-208). Removing the three
   derives (READ-006) is safe.

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
