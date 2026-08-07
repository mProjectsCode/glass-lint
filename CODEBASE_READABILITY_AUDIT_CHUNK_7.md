# Codebase Readability Audit — Chunk 7

## Summary

Chunk 7 owns the immutable fact-stream boundary and the provider-neutral
cross-file flow data model: source candidates, qualified call contexts,
bounded propagation, lifecycle state, and evidence assembly. The semantic
invariants are mostly explicit and deterministic, but several internal APIs
still make lifecycle policy visible to neighboring callers. In particular,
fact append failures have two parallel representations, building and frozen
streams share storage that is only meaningful in one phase, and cross-flow
matching, projection orchestration, and evidence grouping each duplicate
parts of their owning policy.

## Findings

### Fact-stream construction outcomes

#### [x] READ-031 — Unify fact append failures behind one stream outcome

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:23-55,90-113,197-218`; `glass-lint-core/src/analysis/facts/mod.rs:288-312`

`FactStream` maintains a bitset of retained issues (`FactStreamIssueSet`),
while `try_push` separately returns `Result<FactId, FactIssue>`. The returned
`FactIssue` currently has only `BudgetExhausted`, and the only production
caller (`FactBuilder::emit`) discards the result with `let _ =`. The stream
itself records the same budget failure, while path exhaustion, invalid parser
spans, and name exhaustion are reported only through the side-channel issue
flags. A caller therefore cannot handle one coherent append outcome, and a
new append failure must be threaded through two representations even though
the stream owns the validity decision.

**Recommendation:** Let the stream own a single append outcome/status type,
or make append return a small domain result whose variants are derived from
the stream’s issue state. Replace the ignored `FactIssue` result and the
parallel flag writes with one explicit transition that marks the stream
invalid or incomplete as appropriate. Keep dense ID assignment, suffix
discarding after an invariant failure, separate diagnostic reasons, and
fail-closed indexing of incomplete streams.

**Fix Applied:** Replaced the discarded `FactIssue` result with a stream-owned
`append` transition that assigns dense IDs or records budget invalidation in
the stream itself. Path, parser-span, and name exhaustion remain distinct
diagnostic issues. Verified with `make fmt && make ci`.

### Fact-stream phase storage

#### [ ] READ-032 — Keep building and frozen stream storage in separate owners

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:57-86,175-195,279-317`; `glass-lint-core/src/analysis/facts/mod.rs:327-340`

`FactStream<Building>` physically contains `NameTable` and `ValueTable`
placeholders initialized with defaults, even though only
`FactStream<Frozen>` exposes those tables and `freeze` replaces the building
values with resolver-owned tables. The phase marker prevents the ordinary
query methods from being called too early, but the storage and the sealing
contract still span two owners: the stream allocates placeholder tables, the
resolver supplies the final tables, and lowering chooses when the overwrite
occurs. This makes the one-way identity/freeze invariant harder to see and
allows test or in-crate callers to assemble a frozen stream from arbitrary
tables without a single stream-owned sealing boundary.

**Recommendation:** Separate the building representation from the frozen
representation, with the builder retaining only mutable facts, paths,
parameters, limits, and issues; or introduce a focused stream-sealing type
that accepts the resolver-owned tables and performs the consuming transition.
Delete the placeholder name/value storage and broad `freeze` assembly once
callers migrate. Preserve typestate ordering, resolver identity tables,
deterministic paths and facts, and the ability to retain incomplete streams
for diagnostics while refusing to index them.

**Fix Applied:** None so far.

### Cross-flow context matching

#### [ ] READ-033 — Centralize context-to-use connectivity semantics

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Semantics
- **Location:** `glass-lint-core/src/analysis/flow/cross/state.rs:262-281`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:24-70`; `glass-lint-core/src/analysis/flow/cross/propagation.rs:226-263`

`CallContext` exposes low-level `matches_parameter` and
`matches_source_root` methods that accept raw root booleans. Both
`usage_matches_context` and `CallPropagation::propagate` then assemble their
own connectivity predicates. The evidence path handles property writes,
receivers, and arguments, while call propagation repeats the parameter/root
and source-root relation for arguments. These callers therefore jointly own
the rule that only a matching root and a compatible path may cross a context;
changing root handling, unknown values, or parameter provenance can make
evidence filtering and call propagation disagree.

**Recommendation:** Give the cross-flow context boundary one domain-level
relation operation, such as a context match over a typed effect use or call
argument, and have both evidence filtering and propagation consume it. Hide
the raw boolean-based matching helpers after migration. Preserve the
distinction between source-root and target-parameter origins, required root
precision, crossed-module state, and the rule that unknown or unsupported
connectivity cannot establish a witness.

**Fix Applied:** None so far.

### Cross-flow projection orchestration

#### [ ] READ-034 — Make one context runner own projection state and helpers

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:59-103,112-185`; `glass-lint-core/src/analysis/flow/cross/propagation.rs:22-30,214-222`

`CrossProjectionSession` owns project-wide mutable sinks, graph, worklist,
names, and trace arena. `ContextProjection` adds the current context,
effect, flow, plan, and state, then creates `UsageProjector` and
`CallPropagation` with overlapping subsets of those references. The
orchestrator also creates a cloned state and a per-context propagated-event
set outside both helpers. The helper fields are `pub(super)` so the parent
module can assemble the structs directly, leaving call ordering, state
ownership, and the relationship between usage projection and call propagation
as conventions spread across three types.

**Recommendation:** Introduce one private context runner owned by the
cross-flow module that retains the current state and propagated-event set and
offers named operations for usage projection and call propagation. Pass a
narrow session capability to that runner, or make those operations methods on
`ContextProjection`; remove the overlapping public field bags and repeated
construction. Preserve usage-before-call ordering, cloned per-context state,
fact-level propagation deduplication, bounded worklist admission, and
evidence emission through the shared trace arena.

**Fix Applied:** None so far.

### Cross-flow evidence storage

#### [ ] READ-035 — Key rule evidence by its domain identity while accumulating

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Data model
- **Location:** `glass-lint-core/src/analysis/flow/cross/evidence.rs:72-189,192-301`

`RuleEvidence` stores report items in a `Vec` and nonmatching alternatives in
a separate `BTreeSet<EvidenceKey>`. `mark_nonmatching` and `record` both
manually scan the vector by comparing kind, symbol, and an occurrence fact;
`record` then repeats certainty merging and trace deduplication policy. The
existing `EvidenceKey` therefore identifies the semantic item but does not
own the item lookup, and the invariant that a possible alternative downgrades
the corresponding definite item is maintained by parallel collection logic.
This also makes the current `expect` in `into_evidence` depend on the
capacity-shaped vector remaining synchronized with the catalog.

**Recommendation:** Make a private per-rule evidence accumulator keyed by
`EvidenceKey`, with operations for recording a witness, marking a
nonmatching alternative, and finalizing deterministic report items. Keep
trace occurrence merging and certainty downgrade inside that accumulator;
convert to the externally required `RuleEvidenceTable` only at the final
boundary. Preserve separate incompatible alternatives, possible-versus-
definite certainty, trace identity deduplication, catalog capacity bounds,
stable ordering, and clearing all evidence on cross-flow exhaustion.

**Fix Applied:** None so far.

## Systemic Themes

- The fact stream has deliberate typestate and fail-closed behavior, but its
  append outcome and final table attachment are represented by neighboring
  mechanisms instead of one sealing contract.
- Cross-flow types correctly preserve source witnesses, unknown alternatives,
  deterministic order, and bounded traversal. The remaining risk is semantic
  policy duplicated in context predicates and evidence accumulation.
- Refactors must preserve path-local identity, correlated alternatives,
  explicit incompleteness, deterministic evidence order, and exhaustion
  behavior. A possible or unsupported path must never become a definite
  witness merely because storage or traversal code was simplified.

## Decisions

- `FactIssue` is an internal append diagnostic, not a recovery API. The stream
  owns the durable issue state; replace the ignored `Result` with one explicit
  append transition while keeping detailed status aggregation separate.
- Resolver-owned name/value tables remain the artifact identity owner. A
  consuming stream-seal operation may attach those tables, but it must not
  copy or re-home cache/artifact ownership.
- Cross-flow accumulation retains a separate semantic item for each evidence
  key until certainty and trace merging are complete. Presentation grouping
  may merge only after that boundary and must not erase fact-level identity.

## Coverage

Reviewed all types listed in Chunk 7 of `CODEBASE_STRUCTURE_CORE.md`:

- Fact types: `BuiltFacts`, `FactBuilder`, `FactProvenanceState`,
  `ProvenanceCheckpoint`, `SemanticFacts`, `CallResultTable`,
  `ResolvedCallee`, `InstanceCallable`, `ModuleInterfaceBuilder`,
  `CommonJsExportEntry`, `LogEntry`, `OriginCheckpoint`, `OriginMap`,
  `OriginSnapshot`, `PatternLeaf`, `PatternLeafKind`, `TraversalState`,
  `FactIssue`, `FactStream`, `FactStreamIssue`, and
  `FactStreamIssueSet`.
- Cross-flow types: `ContextProjection`, `CrossProjectionOutcome`,
  `CrossProjectionSession`, `CrossWorklist`, `FlowPlanKey`, `WorklistStop`,
  `EmissionContext`, `EvidenceKey`, `ModuleEvidence`, `RuleEvidence`,
  `QualifiedCallGraph`, `QualifiedCallSite`, `CallPropagation`,
  `UsageProjector`, `FlowSources`, `PropagationAdmission`,
  `PropagationItem`, `SourceCandidate`, `SourceIndex`, `SourceKey`,
  `CallContext`, `CallContextOrigin`, `CrossFlowState`,
  `EvidenceTransition`, `ContextAdmission`, and `ContextWorklist`.

Representative callers in lowering, fact control/assignment traversal,
cross-flow propagation, worklist seeding, and evidence emission were checked.
Previously audited Chunk 1 provenance-transition and Chunk 2 generic
worklist-admission findings were not duplicated here.
