# Possible-path matching and multi-trace evidence

## Status

Proposed implementation plan. No implementation has started.

This is a breaking semantic and report-schema change. Clean breaks are
explicitly allowed. Implement it as one forward migration: update every
caller, fixture, adapter, serializer, renderer, test, and document in the same
change sequence.

The completed change must have exactly one path:

- no deprecated aliases for old evidence types;
- no legacy `Finding` constructors;
- no conversion bridge from flat evidence to traces;
- no serde defaults that accept reports without certainty or traces;
- no reader that accepts both the old and new report versions;
- no feature flag selecting must-only versus possible-path behavior; and
- no temporary compatibility modules left after the migration.

Logical phases below are review and verification boundaries, not a requirement
to preserve source or schema compatibility between them. A working branch may
temporarily break downstream crates while all callers are migrated. Before a
phase is considered complete, the workspace should compile with only the new
path.

## Goal

Change Glass Lint from a must-only analysis at control-flow joins to a bounded
may-and-must analysis:

- emit a finding when at least one statically feasible path reaching the
  occurrence proves the rule;
- classify the finding as `Definite` when every modeled path reaching the
  occurrence proves the rule;
- classify it as `Possible` when at least one, but not every, modeled path
  reaching the occurrence proves the rule;
- retain strict identity and flow proof within each path;
- never combine facts from incompatible paths into a synthetic match;
- explain matches with bounded, deterministic alternative evidence traces;
- keep rule-level `Confidence` separate from per-finding
  `MatchCertainty`; and
- preserve explicit diagnostics and truncation when analysis limits prevent a
  complete result.

Static branch pruning is useful but is not part of the first semantic
migration. Add it only after possible-path matching and evidence traces are
stable.

## Motivation

The current join behavior is a must-analysis. For example:

```js
let api = host.files;
if (flag) api = local.files;
api.read();
```

The joined assignment is `Unknown`, so `host.files.read` is not reported even
though one path reaching `api.read()` has a strictly proven host identity.

For security and capability discovery, the more useful question is normally
"can this capability execute?" rather than "does this capability execute on
every path?" The desired result is therefore:

| Program shape | Result |
|---|---|
| Every path reaching the occurrence matches | `Definite` finding |
| Some path reaching the occurrence matches | `Possible` finding |
| No path reaching the occurrence matches | No finding |
| A matching-looking trace requires combining incompatible paths | No finding |

`Definite` and `Possible` describe path coverage, not rule quality. Existing
rule `Confidence` remains author-declared catalog metadata and continues to
control profile selection.

## Non-goals

- Do not attempt general symbolic execution or SAT/SMT solving.
- Do not assign a numeric confidence or a percentage of paths.
- Do not count runtime paths. Loops make that count infinite and syntactic
  refactoring makes percentages unstable.
- Do not weaken rooted, global, module, returned-object, constructed-instance,
  or flow identities within an individual path.
- Do not make rules traverse syntax or build private control-flow models.
- Do not add provider-specific behavior to `glass-lint-core`.
- Do not silently claim `Definite` after dropping alternatives.
- Do not initially prune `if (false)`, constant globals, or other statically
  false conditions. That is a later phase described at the end of this plan.

## Terminology and semantic contract

### Modeled path

A modeled path is one control-flow alternative represented by the bounded core
analysis. It is not a claim that the path is reachable in a real execution.
Until static pruning is implemented, both arms of an ordinary conditional are
modeled unless an existing abrupt control transfer prevents an arm from
reaching the occurrence.

### Path reaching an occurrence

Certainty quantifies only over paths that can reach the primary occurrence or
sink. Paths ending in `return`, `throw`, `break`, or `continue` must be routed
according to the existing control-frame semantics and excluded when they do
not reach the occurrence.

For example:

```js
let api = local.files;
if (flag) {
    api = host.files;
} else {
    return;
}
api.read();
```

The finding is `Definite`: the only modeled path reaching `api.read()` has the
host identity.

### Strict path proof

A `Possible` finding is not a heuristic identity match. It requires at least
one complete path-local proof satisfying the same identity and matcher
constraints currently required for a strict finding.

Unknown provenance, unsupported behavior, dynamic values, or ambiguous module
resolution may prevent a path from matching. They count against `Definite`,
but they must not invalidate a separate complete witness for `Possible`.

### Certainty lattice

Introduce:

```rust
pub enum MatchCertainty {
    Definite,
    Possible,
}
```

Use a stable serialized spelling:

```text
definite
possible
```

The ordering used when merging duplicate evidence is:

```text
Definite > Possible
```

If one complete proof establishes that all modeled paths reaching an
occurrence match, a weaker possible proof at the same rule and primary
location does not downgrade it.

Do not infer `Definite` merely because all retained alternatives match when
the alternative set is incomplete or truncated.

### Analysis completeness

Keep certainty and completeness independent:

- `Definite`, complete: all modeled reaching paths were considered and match.
- `Possible`, complete: at least one modeled reaching path matches and at
  least one does not.
- `Possible`, incomplete: a complete matching witness exists, but limits
  prevented determining all path coverage.
- no finding, incomplete: no complete witness survived before exhaustion;
  report the existing analysis diagnostic.

Never emit an incomplete trace that only appears to prove a match. A
`Possible` finding still needs one complete witness.

## Evidence model

### User-facing shape

Evidence should be a bounded collection of alternative linear traces.
Conceptually:

```text
Trace 1
  script element created here
  src configured here
  script element inserted here

Trace 2
  script element created here
  textContent configured here
  script element inserted here
```

Linear traces are intuitive to render like stack traces and preserve the
correlation between source, configuration, aliases, calls, and sink. Multiple
traces express alternatives.

The public report model should contain domain types rather than a recursive
JSON-shaped enum:

```rust
pub struct Finding {
    // existing fields
    certainty: MatchCertainty,
    evidence: EvidenceTraces,
}

pub struct EvidenceTraces {
    traces: Vec<EvidenceTrace>,
    truncated: bool,
}

pub struct EvidenceTrace {
    steps: Vec<EvidenceStep>,
}

pub struct EvidenceStep {
    role: EvidenceRole,
    message: String,
    location: SourceLocation,
}

pub enum EvidenceRole {
    Source,
    Assignment,
    Requirement,
    Call,
    Return,
    Sink,
    Occurrence,
}
```

The exact role list may be adjusted while implementing, but it must remain
provider-neutral, small, and semantically meaningful. Do not encode provider
or rule names in roles.

Every report type must:

- validate invariants in its constructor;
- expose read-only accessors rather than internal vectors;
- serialize deterministically;
- reject empty traces;
- ensure the last or designated step corresponds to the finding's primary
  occurrence;
- expose truncation explicitly; and
- avoid exposing internal fact, module, object, path, or checkpoint IDs.

### Internal representation

Do not store duplicated full traces throughout analysis. Add a bounded,
interned trace arena in core:

```text
TraceNodeId -> {
  parent: optional TraceNodeId,
  event: qualified semantic event,
  role: evidence role
}
```

Alternative semantic states carry a `TraceNodeId` or a small canonical set of
trace heads. Common prefixes are shared automatically. Convert trace heads
into public `EvidenceTrace` values only during report assembly.

Requirements:

- intern identical `(parent, event, role)` nodes;
- charge every insertion and comparison to an explicit analysis limit;
- keep node and alternative ordering canonical;
- reconstruct traces in source/event order where that reflects execution
  order;
- preserve cross-module `(module, event)` qualification internally;
- resolve locations only during report assembly;
- deduplicate identical rendered traces;
- apply a per-finding trace limit and a per-trace step limit; and
- set `truncated` whenever additional distinct canonical traces were omitted.

Do not use the current flat `related` evidence bag for trace correlation.
Related evidence attached at rule scope cannot distinguish which source,
requirement, and sink belong to one witness.

### Existing evidence migration

The present model groups `ClassificationEvidence` by `(kind, symbol)` and
stores:

- primary occurrences;
- a flat list of related cross-module events; and
- report-level flat `Evidence` items.

Replace this cleanly:

1. Make an internal classified occurrence own its certainty and trace heads.
2. Normalize and deduplicate at occurrence granularity, because two
   occurrences with the same symbol can have different certainty and traces.
3. Resolve each internal trace into public report evidence for its own
   finding.
4. Remove rule-wide shared related evidence from `ReportAssembly`.
5. Remove `EvidenceList`'s shared/local split if it no longer enforces a useful
   invariant.
6. Delete obsolete flat evidence constructors and compatibility paths after
   every caller is migrated.

Direct matches still get a one-step trace at their primary occurrence.

## Central correctness invariant

Never independently union aliases, sources, requirements, and sinks across a
join. A complete witness must be carried by one correlated alternative.

This program must not match:

```js
const script = document.createElement("script");
let inserted;

if (flag) {
    script.src = "/app.js";
    inserted = localElement;
} else {
    inserted = script;
}

document.head.appendChild(inserted);
```

One branch configures the script but inserts a different object. The other
branch inserts the script without configuring it. A union of independent
"possible configuration" and "possible alias" sets would invent a match.

The implementation must therefore retain relational alternatives. Prefer a
bounded collection of complete semantic environments, backed by existing
checkpoint/mutation-log machinery, over independent per-field unions.

## Bounded disjunctive analysis

### Alternative environments

Introduce an internal domain collection such as:

```rust
struct AlternativeEnvironments<E> {
    alternatives: Vec<E>,
    completeness: AlternativeCompleteness,
}
```

Each alternative represents a correlated semantic environment. It includes
the state needed by its owning subsystem and trace support for how the state
was reached.

The collection must:

- remove unreachable alternatives;
- canonicalize deterministic ordering;
- coalesce semantically equal alternatives;
- retain distinct evidence support when equal states arrived by different
  paths;
- cap semantic alternatives independently from evidence traces;
- record incompleteness when the cap is reached;
- never promote an incomplete collection to `Definite`; and
- expose operations for branch, transfer, abrupt exit, join, and certainty
  classification.

Do not use source-order path count as the limit. Add an explicit alternative
state limit derived from `AnalysisLimits`, with operation charging for
transfers, equality checks, coalescing, and trace-node insertion.

### Join operation

At a control-flow join:

1. discard unreachable exits;
2. gather complete correlated environments;
3. canonicalize each semantic state;
4. merge semantically equal states while unioning bounded trace support;
5. retain semantically different states as separate alternatives;
6. propagate an incomplete marker from any incoming collection or limit
   exhaustion; and
7. continue subsequent transfers over every retained alternative.

This replaces:

- assignment joins that collapse disagreement to `AssignmentValue::Unknown`;
  and
- object-flow joins that intersect aliases and requirement keys.

`Unknown` remains a valid semantic value for genuinely unknown provenance. It
must no longer be used merely because two known branch values disagree.

### Certainty calculation

At a primary occurrence, evaluate the matcher independently against every
reaching alternative:

```text
matching alternatives == 0
  -> no finding

matching alternatives > 0
and every reaching alternative matches
and the alternative collection is complete
  -> Definite

matching alternatives > 0
otherwise
  -> Possible
```

Every matching alternative must produce or reference a complete evidence
trace. Nonmatching alternatives need not appear as traces, but their presence
must participate in certainty calculation.

When equivalent matching alternatives differ only in evidence, emit one
finding with multiple bounded traces.

## Implementation sequence

Each phase is a logical implementation and review boundary. Use narrow tests
while iterating. Because clean breaks are allowed, tightly coupled public
report phases may be implemented together rather than preserving a compiling
old contract between commits. At every completed boundary, however, all
workspace callers must use the new path. Run `make ci` before considering the
complete migration finished.

### Phase 0: Freeze examples and invariants

1. Add a design test matrix to the relevant core integration tests before
   changing behavior.
2. Mark currently failing desired cases with temporary ignored tests only if
   necessary; prefer adding them in the same commit that enables the behavior.
3. Record exact expected primary locations, certainty, trace roles, trace
   locations, and trace order.
4. Add the incompatible-branch adversarial case above.
5. Add abrupt-exit cases showing that certainty quantifies over paths reaching
   the occurrence.
6. Add limit-exhaustion cases proving incomplete analysis cannot become
   `Definite`.

Initial semantic cases:

```js
// Possible: host only on the incoming/false path.
let api = host.files;
if (flag) api = local.files;
api.read();

// Possible: host only on the true path.
let api = local.files;
if (flag) api = host.files;
api.read();

// Definite: every reaching path has the same identity.
let api = host.files;
if (flag) api = host.files;
else api = host.files;
api.read();

// Definite: the conflicting path returns before the occurrence.
function run(flag) {
    let api = local.files;
    if (flag) api = host.files;
    else return;
    api.read();
}

// No finding: neither path has the identity.
let api = local.files;
if (flag) api = other.files;
api.read();
```

Rename
`conditional_assignment_never_falls_back_to_an_older_identity` to describe
the new invariant, for example
`conditional_assignment_preserves_each_feasible_identity`.

### Phase 1: Add the public certainty contract

Owning crate: `glass-lint-core`.

1. Define `MatchCertainty` once in the provider-neutral public API.
2. Re-export it from the crate root and project report surface as appropriate;
   do not create separate internal and public certainty enums.
3. Add `certainty` to `Finding`.
4. Update `Finding::new`, accessors, equality, serialization, report
   combination, and all test helpers.
5. Initially mark every existing finding `Definite`. This phase must not
   change finding counts.
6. Add serialization tests for stable `definite` and `possible` spellings.
7. Coordinate the report-version bump with Phase 2 so the final certainty and
   trace schema receives one new version rather than exposing an intermediate
   certainty-only schema.
8. Update the harness manual deserializer and adapter proxy in the same
   cutover; do not accept reports that omit certainty.
9. Add optional `certainty` matching to `FindingExpectation` and fixture
   directives so tests can assert it directly.
10. Update JSON, pretty-output, and any summary tests affected by the new
    field.

Acceptance criteria:

- all existing findings and locations are unchanged;
- every existing finding is `Definite`;
- JSON reports contain certainty;
- adapters and report combination use the new report version; and
- rule `Confidence` behavior and profiles are unchanged.

### Phase 2: Introduce public evidence traces

Owning crate: `glass-lint-core`; update harness and CLI callers in the same
phase.

1. Add validated `EvidenceRole`, `EvidenceStep`, `EvidenceTrace`, and
   `EvidenceTraces` report types.
2. Replace flat `Evidence`/`EvidenceList` with the trace collection. Update
   every caller directly; do not keep conversion constructors or deprecated
   aliases.
3. Give every existing direct finding a one-step `Occurrence` trace.
4. Preserve existing evidence messages where useful, but centralize message
   construction by semantic role.
5. Change pretty rendering to:
   - show certainty beside the finding;
   - render traces as numbered alternatives only when there is more than one;
   - render steps in execution order with source locations; and
   - visibly mark omitted traces.
6. Change JSON serialization tests to pin the new shape.
7. Update `glass-lint-harness` expectations, adapter serialization,
   evidence-order digests, profiling digests, and report generation.
8. Delete the rule-wide shared evidence path after all callers use
   finding-specific traces.
9. Increment `REPORT_VERSION` exactly once for the combined certainty and
   trace schema. Readers must reject the old version rather than silently
   supplying defaults.

Acceptance criteria:

- no flat related-event bag remains in a public finding;
- each finding owns only evidence relevant to that occurrence;
- evidence order is deterministic;
- all trace collections are bounded and expose truncation; and
- current semantic finding counts remain unchanged.

### Phase 3: Add the internal trace arena

Owning crate: `glass-lint-core`.

1. Add provider-neutral qualified evidence events:
   - module identity;
   - fact/event identity;
   - evidence role; and
   - parent trace node.
2. Implement a bounded interned trace arena.
3. Add explicit limits and operation accounting for:
   - trace nodes;
   - trace heads per semantic state;
   - rendered traces per finding; and
   - steps per rendered trace.
4. Store trace heads in classified occurrences rather than flat related
   evidence.
5. Update normalization to deduplicate by primary occurrence, certainty, and
   canonical trace identity.
6. Resolve qualified events to `SourceLocation` only in report assembly.
7. If location resolution fails for a required witness step, drop that witness
   rather than emitting a misleading incomplete trace.
8. Add unit tests for interning, deterministic ordering, deduplication,
   truncation, cross-module qualification, and reconstruction.

Acceptance criteria:

- common trace prefixes share storage;
- repeated analysis produces byte-identical JSON;
- no internal IDs leak into serialized reports; and
- a bounded complete witness can always be reconstructed.

### Phase 4: Enrich existing definite object-flow evidence

Do this before changing flow join semantics. It validates the trace model
against the current must-analysis.

1. When an object source matches, create a `Source` trace step.
2. When a configuration requirement matches, append a `Requirement` step.
3. When a helper call/return transports the object, append `Call` and `Return`
   steps where they materially explain the path.
4. At completion, append the `Sink` step.
5. For multiple requirements, preserve matcher declaration order and actual
   event order deterministically.
6. Replace `RequirementSet<FactId>` with a representation that retains the
   evidence support needed for every satisfied requirement.
7. When identical requirements are satisfied in both branches under current
   must semantics, retain both evidence alternatives instead of arbitrarily
   keeping the first branch's event ID.
8. Extend cross-module flow evidence to use the same trace representation.

Acceptance criteria:

- script-element findings show creation, configuration, and insertion;
- helper and cross-module cases show connected trace steps;
- no source or requirement from an unrelated object appears in a trace; and
- finding counts remain the same as before possible-path semantics.

### Phase 5: Preserve alternative assignment provenance

Owning subsystem: `analysis/scope`.

1. Replace the "known values disagree, therefore `Unknown`" join with a
   bounded provenance alternative domain.
2. Each provenance alternative must retain:
   - the strict `BindingProvenance`;
   - its trace head;
   - whether it is reachable; and
   - enough path support to keep correlated transfers distinct.
3. Keep genuinely unsupported/dynamic provenance as an explicit unknown
   alternative.
4. Change visible-binding queries to return bounded alternatives rather than
   one provenance reference.
5. Update rooted/member resolution to evaluate identity constraints against
   each alternative.
6. Emit a classified occurrence when at least one alternative matches.
7. Calculate `MatchCertainty` from every alternative reaching the occurrence.
8. Coalesce identical provenances while retaining multiple bounded traces.
9. Preserve current shadowing, reassignment, lexical scope, callback, and
   binding-version rules within each alternative.
10. Update all consumers in facts, value resolution, occurrence indexing, and
    argument matching in the same migration; do not add a legacy
    single-provenance wrapper.

Required tests:

- the three conditional assignment examples;
- nested `if`/`else`;
- conditional expressions;
- switches with and without `default`;
- `try`/`catch`/`finally`;
- zero-iteration and guaranteed-once loops;
- branch-local shadowing;
- reassignment before and after the branch;
- aliases of each branch value;
- destructuring;
- callback parameter projection;
- abrupt exits; and
- deterministic trace ordering.

Acceptance criteria:

- both host/local disagreement examples emit `Possible`;
- equal host identities emit `Definite`;
- an unknown alternative prevents `Definite` but does not erase a complete
  possible witness; and
- no older assignment is used on a path where a newer assignment replaced it.

### Phase 6: Convert object flow to correlated alternative environments

Owning subsystem: `analysis/flow/projector`.

1. Replace intersecting `FlowStateTable` joins with bounded disjunctive flow
   environments.
2. Reuse the existing checkpoint/mutation-log machinery where possible so
   alternatives do not clone complete maps at every branch.
3. Apply every subsequent fact transfer to each retained alternative.
4. Coalesce alternatives only when their semantic alias and lifecycle state
   are equal.
5. Keep trace support separate from semantic equality so equivalent states can
   expose multiple evidence traces without multiplying future semantic work.
6. Calculate readiness independently per alternative.
7. At a sink:
   - emit no finding when no alternative is ready;
   - emit `Definite` when every complete reaching alternative is ready;
   - emit `Possible` when only some are ready; and
   - emit `Possible` when a witness exists but alternative coverage is
     incomplete.
8. Ensure invalidations, compound writes, updates, deletes, and reassignment
   kill state only in the alternatives where they execute.
9. Preserve abrupt exits through loop, switch, and try/finally frames.
10. Propagate trace heads through helper summaries and cross-call projection
    without copying complete traces.

Required positive tests:

- configuration in only the true arm;
- configuration in only the false/incoming arm;
- distinct valid configurations in both arms;
- source created in one branch and used after the join;
- sink reached by only one matching branch;
- a matching branch plus an unreachable nonmatching branch;
- loop body representing a possible iteration;
- do/while body representing a guaranteed first iteration where applicable;
- try and catch each producing valid but different traces; and
- cross-helper and cross-module possible paths.

Required adversarial negatives:

- source, requirement, and sink split across incompatible branches;
- alias points to the matched object only in a branch where the requirement is
  absent;
- requirement is invalidated on the only branch reaching the sink;
- one branch reassigns the object before insertion;
- same-name local object on the other branch;
- unsupported dynamic alias combined with an unrelated valid requirement;
- loop-carried state that would require mixing different iterations without a
  valid execution sequence; and
- exhausted alternative or trace budgets.

Acceptance criteria:

- `one_arm_requirement_does_not_leak_after_join` becomes a `Possible` finding
  test with a complete trace;
- `identical_branch_requirements_are_definite` remains `Definite` and retains
  both branch traces;
- incompatible branch facts never form a finding; and
- object-flow operation counts remain bounded by explicit limits.

### Phase 7: Loops and fixed points

Do not model loops by enumerating a configured number of whole runtime paths.

1. Define loop semantics over a finite bounded set of canonical semantic
   alternatives.
2. Include the zero-iteration baseline for `while`, `for`, `for-in`, and
   `for-of`.
3. Exclude the zero-iteration baseline for `do/while`.
4. Route `break` and `continue` alternatives through their correct frames.
5. Iterate semantic transfer to a fixed point where the state domain is
   finite and monotone.
6. When a fixed point cannot be reached within the operation/alternative
   limit:
   - retain complete witnesses already found;
   - mark coverage incomplete;
   - never emit `Definite`; and
   - record an analysis diagnostic.
7. Coalesce repeated iterations with identical semantic state and trace
   shape. Do not attempt to serialize infinitely many iteration-distinct
   traces.
8. Define a deterministic representative trace for repeated equivalent
   iterations and set trace truncation when alternatives are omitted.

Acceptance criteria:

- possible matches from a loop body are detected;
- zero-iteration paths correctly downgrade certainty;
- guaranteed-once bodies can still produce `Definite` when every exit reaching
  the sink matches;
- loop-carried kills and reassignments remain path-correct; and
- runtime and memory are bounded independently of runtime iteration count.

### Phase 8: Cross-call and cross-module propagation

1. Add certainty and trace support to function effects and flow summaries.
2. Keep call-site alternatives correlated with parameter alternatives.
3. Do not combine a source from one call site with a requirement or sink from
   another incompatible call site.
4. Carry qualified module/event steps through project linking.
5. Treat ambiguous, unresolved, or unsupported linking as unknown
   alternatives:
   - they prevent `Definite`;
   - they do not erase an independent complete possible witness.
6. Update summary joins to coalesce semantic equality while retaining bounded
   trace alternatives.
7. Charge fixed-point and trace propagation to existing or new explicit
   project-flow limits.
8. Ensure findings remain in the file containing the primary occurrence or
   sink.

Required tests:

- two call sites with different provenances;
- only one call site matching;
- all reaching call sites matching;
- callback and returned-object flows;
- re-exported module identity on one project path;
- ambiguous export alongside an independent strict witness;
- cross-module source, requirement, and sink trace ordering; and
- deterministic behavior under module input reordering.

### Phase 9: Report assembly, CLI, and harness completion

1. Make `Finding` sorting include only stable user-facing keys. Certainty
   should not destabilize primary source ordering.
2. When duplicate findings share rule and primary range:
   - merge traces deterministically;
   - keep `Definite` if any complete all-path proof establishes it;
   - otherwise keep `Possible`; and
   - preserve truncation/incompleteness.
3. Update pretty output wording. Suggested concise labels:

   ```text
   definite
   possible path
   ```

4. Add a short explanation for `Possible`:

   ```text
   Proven on at least one modeled control-flow path; runtime reachability is
   not established.
   ```

5. Do not call a possible-path finding "low confidence."
6. Add `certainty=` to harness fixture expectations and documentation.
7. Update adapter request/response schemas and manual deserialization.
8. Update evidence order digests and profiling comparisons.
9. Update rule fixtures whose expected counts change under possible-path
   semantics.
10. Add realistic bundled/minified e2e cases, because bundled code is the
    primary target.
11. Document `MatchCertainty` in the core and CLI READMEs.
12. Update `ARCHITECTURE.md` and `glass-lint-core/ARCHITECTURE.md`:
    - strict identity is path-local;
    - joins retain bounded alternatives;
    - findings distinguish possible and definite path coverage; and
    - incomplete analysis cannot claim definite coverage.
13. Update `TESTING.md` with mandatory incompatible-path negatives and
    certainty assertions.
14. Update `AGENTS.md` wording that currently says ambiguity and exhausted
    budgets always fail closed, clarifying the complete-witness rule.

Acceptance criteria:

- JSON, pretty output, fixture expectations, adapters, and docs agree on the
  same certainty semantics;
- the report version is bumped exactly once for the final schema;
- old-version reports and reports missing required certainty/trace fields are
  rejected;
- no obsolete evidence compatibility path remains; and
- external consumers can distinguish possible from definite findings without
  parsing messages.

### Phase 10: Performance and limit validation

1. Add operation-count tests for:
   - deeply nested branches;
   - many equivalent branches that should coalesce;
   - many distinct alternatives that hit the cap;
   - loops reaching a fixed point;
   - many trace alternatives sharing a prefix; and
   - cross-call fan-out.
2. Confirm every potentially multiplicative loop charges an operation budget.
3. Add debug/test-only counters for:
   - maximum live semantic alternatives;
   - trace nodes;
   - trace heads;
   - coalescing comparisons;
   - fixed-point iterations; and
   - rendered traces.
4. Profile against representative bundled corpora with fixed manifests and
   worker counts.
5. Compare finding IDs, certainty, evidence digests, diagnostics, completion,
   and operation counts before comparing wall time.
6. Optimize only measured bottlenecks.

Potential optimizations, only if profiling justifies them:

- fingerprint semantic environments before equality comparison;
- intern small alternative sets;
- separate semantic-state equality from trace support;
- lazily reconstruct traces only for emitted findings;
- reuse checkpoint transitions across alternatives with common ancestry; and
- cap traces more aggressively than semantic alternatives.

Acceptance criteria:

- memory and CPU remain bounded for adversarial branch fan-out;
- equivalent alternatives coalesce;
- exhaustion is deterministic and visible; and
- no wall-clock assertion is added to the normal test suite.

## Later phase: static branch pruning

Static pruning is intentionally deferred. It should reuse the shared value
model and control markers rather than add a second evaluator.

### Initial pruning scope

Start only with truthiness that core can prove exactly:

- literal `true` and `false`;
- exact constant expressions already supported by the shared value model;
- lexically proven immutable constants whose value is exactly known; and
- direct negation or similarly simple operations with exact JavaScript
  semantics.

Do not initially infer environment globals, bundler define replacements, or
arbitrary boolean algebra.

### Motivating example

```js
const DEBUG = false;

if (DEBUG) {
    console.log("debug");
}
```

Once the shared value model proves `DEBUG` is immutable and false, the
`console.log` path should not be modeled and no console rule finding should be
emitted.

### Implementation steps

1. Add a provider-neutral `StaticTruthiness` domain:

   ```rust
   enum StaticTruthiness {
       AlwaysTruthy,
       AlwaysFalsy,
       Unknown,
   }
   ```

2. Evaluate the condition once during matcher-independent lowering.
3. Attach the result to the control region/marker consumed by scope and flow.
4. Skip the impossible branch while still evaluating condition side effects.
5. Handle loop tests:
   - `while (false)` has no body path;
   - `do { ... } while (false)` executes once;
   - unknown tests retain current possible-path behavior.
6. Treat mutation, shadowing, dynamic access, unsupported operators, and
   budget exhaustion as `Unknown`.
7. Add positives and adversarial negatives for:
   - immutable `DEBUG = false`;
   - shadowed `DEBUG`;
   - reassigned `DEBUG`;
   - imported or dynamic configuration;
   - side-effecting conditions;
   - negation;
   - conditional expressions; and
   - minified constant forms produced by common bundlers.
8. Confirm pruning can upgrade `Possible` to `Definite` only when the pruned
   branch is proven impossible and analysis is otherwise complete.

Static pruning must improve precision without becoming a prerequisite for
possible-path matching.

## Detailed test placement

### Core unit tests

Place private algebra and invariant tests beside:

- alternative environment collection;
- certainty calculation;
- trace arena;
- trace reconstruction;
- assignment environment joins;
- object-flow state joins;
- loop fixed points; and
- static truthiness, when implemented.

### Core integration tests

Use:

- `glass-lint-core/tests/scope_precision.rs` for conditional provenance,
  shadowing, aliases, and reassignment;
- `glass-lint-core/tests/declarative_matching/flow.rs` for public object-flow
  matcher behavior;
- `glass-lint-core/tests/compact_source.rs` for constructed-instance and
  compact/minified shapes;
- project tests for cross-file certainty and trace locations; and
- report tests for exact serialized and pretty output.

### Provider contracts

Add possible/definite cases beside affected `positive.js` and `negative.js`
fixtures. At minimum cover:

- browser script injection;
- remote resource flow;
- file dialog lifecycle flow;
- an Obsidian object-flow rule if one exercises configuration and sink stages;
  and
- a direct identity rule using conditional assignment.

### End-to-end tests

Add bundled-style examples under `tests/e2e` that verify:

- possible capability detection after conditional reassignment;
- multi-step trace rendering;
- certainty in JSON output;
- no incompatible-branch stitching; and
- stable output under minified formatting.

## Suggested narrow commands

Run the narrowest relevant command after each phase:

```sh
cargo test -p glass-lint-core --test scope_precision
cargo test -p glass-lint-core --test declarative_matching
cargo test -p glass-lint-core flow::projector
cargo test -p glass-lint-core project::report
cargo test -p glass-lint-core --test report_pretty
cargo test -p glass-lint-harness
cargo test -p glass-lint-cli
```

Run affected rule contracts through the harness while migrating provider
fixtures. Before completion run:

```sh
make ci
```

## Documentation wording changes

Replace must-only statements such as:

```text
Strict matches require proven identity/flow.
Unknown or ambiguous semantics fail closed.
```

with wording that preserves the intended invariant:

```text
Every finding contains at least one complete, strictly proven semantic
witness. Match certainty states whether that proof holds on every modeled
path reaching the occurrence or only some paths. Unknown, ambiguous,
unsupported, or exhausted alternatives cannot establish a witness and prevent
definite certainty, but do not erase an independent complete witness.
```

Do not describe possible-path evidence as heuristic, guessed, or low
confidence. The identity proof is strict; only runtime path coverage differs.

## Completion checklist

- [ ] `MatchCertainty` is a single public provider-neutral type.
- [ ] Every finding serializes `definite` or `possible`.
- [ ] Rule `Confidence` remains unchanged and separate.
- [ ] Public evidence is a bounded collection of correlated traces.
- [ ] Direct findings have a valid one-step trace.
- [ ] Flow findings show source, requirements, and sink.
- [ ] Assignment disagreements retain known alternatives.
- [ ] Object-flow joins retain correlated semantic alternatives.
- [ ] Some-path matches emit `Possible`.
- [ ] All-reaching-path matches emit `Definite`.
- [ ] Abrupt exits are excluded from paths they cannot reach.
- [ ] Incompatible branch facts never form a finding.
- [ ] Unknown alternatives do not erase independent complete witnesses.
- [ ] Incomplete analysis never emits `Definite`.
- [ ] Alternative, trace, and fixed-point work is explicitly bounded.
- [ ] Output and trace ordering is deterministic.
- [ ] Report schema/version, adapters, and harness expectations are migrated.
- [ ] Old report versions and missing certainty/trace fields are rejected.
- [ ] Provider fixtures and bundled e2e cases cover the new behavior.
- [ ] Architecture, testing, CLI, and core documentation are updated.
- [ ] Obsolete flat/shared evidence paths are deleted.
- [ ] No compatibility alias, constructor, serde default, feature flag, or
      dual semantic path remains.
- [ ] `make ci` passes.

Static pruning is a separate follow-up:

- [ ] Exact `StaticTruthiness` is shared by scope and flow.
- [ ] Immutable `DEBUG = false` prunes the debug branch.
- [ ] Shadowing, reassignment, mutation, and unsupported expressions remain
      unknown.
- [ ] Pruning can upgrade certainty only with a complete exact proof.
