# Query and matcher architecture plan

## Status

Implementation in progress. Phase 0 (baseline) is partially complete — the
`MatcherDeclBuilder` entry points and core integration tests are in place, and
the analysis engine (flow projection, cross-file flow, scope precision,
evidence, operations tracking, harness types) has been substantially improved
across 10 implementation phases. The query/matcher architecture migration
described below begins from this foundation.

Phase 1 (collapse `QueryPlan` into `CompiledMatcherPlan`) is complete.
Phase 2 (declaration/compiler ownership) is complete:

- Declaration-owned types (`IdentitySpec`, `EventSpec`, `SubjectSpec`) live in
  `api/rule/query/`.
- `MatcherDecl` no longer stores compiler IR; `api/rule/decl.rs` imports only
  from `api::rule::query`.
- The compiler has a directional lowering pass (`lower_to_clause`).
- Object-flow declarations are first-class: `Rule` stores `ObjectFlowMatcher`
  values separately from `MatcherDecl`; `MatcherDecl::from_object_flow` and the
  old `object_flow` placeholder field are removed.
- All `MatcherBuildError::Generic(String)` uses have been replaced with
  structured variants.

Phase 3 (typed logical query algebra) is the next step.

This plan deliberately separates two efforts:

1. **Query and matcher architecture** is the primary work. It strengthens the
   semantic query model, compiler, execution plans, validation, and matcher
   authoring API while preserving Glass Lint's existing precision and
   boundedness contracts.
2. **A textual query language** is future work. It begins only after the query
   algebra and compiler have been exercised by the complete built-in rule
   catalog and at least one genuinely new query capability.

The language must be a frontend to the same query compiler and executor. It
must not create a second matcher path, syntax traversal, semantic model, flow
engine, or report pipeline.

Breaking changes are allowed. Implement each migration as one forward path:

- update all callers in the same change sequence;
- remove superseded matcher types and compiler paths;
- do not retain deprecated aliases or compatibility wrappers;
- do not keep two executors selected by a feature flag;
- do not deserialize both old and new query formats unless a public persisted
  format has intentionally been introduced and versioned; and
- do not expose compiler storage merely to make migration easier.

The phases below are review and verification boundaries. A development branch
may be temporarily incomplete, but a phase is complete only when the workspace
builds and tests through the single architecture described by that phase.

## Executive decision

Glass Lint should evolve toward a small typed relational query system, inspired
by Datalog and CodeQL but specialized for Glass Lint's semantic facts,
certainty model, path correlation, budgets, and evidence.

Do not start by designing textual syntax.

The implementation order is:

```text
semantic contract
  -> declaration/IR/compiler ownership
  -> typed logical query algebra
  -> validation and normalization
  -> physical planning
  -> execution through existing indexes and flow machinery
  -> full matcher migration
  -> new query capabilities
  -> stable authoring API
  -> optional textual language frontend
```

The architecture may keep three meaningful semantic stages when each stage
enforces a different contract:

```text
authored QueryDecl
  -> normalized logical query
  -> CompiledMatcherPlan
```

The normalized logical query is compiler-only and need not be retained in the
catalog after physical planning. No stage may be a one-field wrapper or a
field-for-field clone without a distinct invariant.

If parsing a textual language is later added, its source-preserving parser AST
is another ephemeral frontend representation:

```text
text
  -> parsed syntax with source spans
  -> authored QueryDecl
  -> normalized logical query
  -> CompiledMatcherPlan
```

There must not be a permanent `CompiledMatcherPlan(QueryPlan(...))` wrapper
whose only operation is returning the inner plan.

## Goals

### Primary goals

- Give all matchers one compositional semantic query model.
- Make query power grow by adding reusable relations and operators rather than
  adding a builder method for every identity/event/subject combination.
- Separate author declarations from compiler IR and execution storage.
- Preserve matcher-independent parsing and fact construction once per file.
- Compile catalogs once into immutable plans reusable across files and
  projects.
- Select physical operators that reuse the existing occurrence indexes,
  constrained fact stream, flow engine, and project overlays.
- Make correlations explicit through typed variables or equivalent semantic
  keys.
- Preserve strict path-local identity and never synthesize a witness from
  incompatible alternatives.
- Preserve `Definite` and `Possible` path coverage and explicit incomplete
  analysis.
- Keep all work, state, evidence, recursion, and output bounded.
- Keep query results, evidence, diagnostics, and operation counts
  deterministic.
- Improve validation diagnostics so an invalid query identifies the failing
  operator, operands, and semantic reason.
- Make query plans inspectable in tests and profiling without exposing
  internal artifact-local IDs publicly.
- Make the eventual language a thin frontend to a proven query system.

### Authoring goals

- Keep simple API-use rules concise.
- Allow reusable provider-local helpers without moving policy into core.
- Express alternatives, conjunctions, value constraints, subject
  relationships, and bounded lifecycles consistently.
- Avoid requiring rule authors to know which execution subsystem handles a
  query.
- Produce errors at catalog construction rather than silently ignoring an
  unsupported query.
- Retain Rust type checking and IDE support for built-in rules until a textual
  language clearly provides more value.

### Engine goals

- Distinguish logical meaning from physical execution strategy.
- Allow a query to be normalized and planned without retaining the authored
  declaration tree in the runtime catalog.
- Deduplicate equivalent query branches deterministically.
- Share identical compiled subplans when this is measurably useful and does
  not complicate limits or evidence ownership.
- Make expensive or unbounded shapes impossible to construct or reject them
  during compilation.
- Support later relational operations without replacing the entire semantic
  engine with a generic database.

## Non-goals

- Do not replace SWC parsing or introduce Tree-sitter as a second parser.
- Do not query the SWC AST directly from rules.
- Do not let enabling a rule trigger another source traversal.
- Do not initially implement a general-purpose database or full CodeQL clone.
- Do not initially implement arbitrary user recursion.
- Do not initially implement unrestricted negation.
- Do not initially implement aggregation, arithmetic, or general scripting.
- Do not initially expose raw facts, binding IDs, `NameId`, `FactId`, object
  IDs, path IDs, checkpoints, or flow-state IDs.
- Do not move provider API names, rule categories, profiles, or policy into
  `glass-lint-core`.
- Do not weaken strict global, module, rooted, returned-object, instance, value,
  or flow matching.
- Do not treat an unknown predicate as false when doing so would incorrectly
  establish a definite result.
- Do not adopt an external Datalog runtime before a representative prototype
  proves that it preserves Glass Lint's correctness and budget contracts.
- Do not create a textual DSL solely to shorten Rust builder syntax.
- Do not migrate rule metadata into the query language as part of the first
  language version. Query semantics and provider rule metadata are separate
  concerns.

## Current state and motivating problems

### Current declaration model

`MatcherDecl` is the single public matcher declaration. It stores:

- one `IdentityConstraint`;
- one `EventPredicate`;
- one `SubjectConstraint`;
- zero or more `QueryConstraint` values;
- evidence kind and symbol; and
- an optional raw `ObjectFlowMatcher`.

Most builder entry points choose a pre-approved identity/event pair. This is
safe and convenient for existing rules, but the number of methods grows with
the cross-product of:

- identity source;
- event kind;
- subject relationship;
- value constraint;
- provenance strength;
- module matching mode; and
- evidence behavior.

Adding richer relationships through more convenience methods will eventually
make the builder the de facto grammar while hiding the underlying
compositional model.

### Current declaration/compiler overlap

The rule declaration layer directly stores types defined by the compiler
layer. `MatcherDecl::to_query_clause` mostly clones those fields into a
`QueryClause`. Validation is split between builder construction and compiler
validation.

This causes several problems:

- the declaration and compiled forms are almost isomorphic;
- ownership points in the wrong conceptual direction;
- public builder choices are coupled to physical compiler enums;
- some invalid combinations can be represented internally and rejected only
  later;
- adding a new query capability tends to touch the public builder, compiler
  enums, validators, and multiple executors together; and
- there is no clean frontend boundary for a future parser.

### Current plan overlap

`CompiledMatcherPlan` currently wraps one `QueryPlan`, and consumers immediately
borrow the inner value. `QueryPlan` contains ordinary clauses and separately
compiled object flows.

This is historical layering, not a meaningful invariant. Collapse the two
types before building more architecture on them.

### Current execution split

The runtime does not yet execute a generic plan:

- unconstrained clauses use occurrence indexes;
- constrained call clauses use fact-stream projection;
- object flows use local and cross-call flow machinery;
- project matching adds module-identity overlays; and
- callers inspect clauses or flows to select those paths.

These specialized paths are valuable and should remain available as physical
operators. The problem is that their selection is currently implicit and
distributed rather than represented by a compiled physical plan.

### Current logical limitations

The existing model naturally expresses:

- a union of independent matcher declarations;
- a single event selected by identity;
- a direct, returned-from, or instance-of subject;
- conjunction over argument constraints on call-bearing events; and
- a specialized object lifecycle.

It does not provide general author-visible concepts for:

- binding the same semantic object or value across several predicates;
- joining two independently selected events;
- nested `any` and `all` expressions;
- ordering between arbitrary related events;
- reusable predicates;
- choosing a primary result independently from supporting witnesses;
- explaining why a query was rejected or how it was planned; or
- extending lifecycle matching without growing another parallel matcher API.

## Target architecture

### Layering

```text
Provider rule code
  |
  | constructs validated provider-neutral query declarations
  v
api/rule/query
  QueryDecl
  QueryExpr
  typed identity/value/event terms
  evidence projection
  |
  | compile once at catalog construction
  v
api/compiler/query
  logical validation
  type checking
  normalization
  physical planning
  |
  v
CompiledMatcherPlan
  index scans
  constrained-event scans
  keyed joins
  bounded lifecycle plans
  project-overlay requirements
  evidence projection
  |
  | execute against immutable semantic artifacts
  v
analysis/matching
  occurrence indexes
  fact stream
  value/identity services
  local flow engine
  cross-call/project flow
  |
  v
deterministic classified witnesses
  -> findings and evidence
```

### Ownership

`api/rule` owns:

- author-constructible validated declaration types;
- semantic names used by rule authors;
- declaration builders or typed combinators;
- provider-neutral value predicates;
- provider-neutral flow/lifecycle declarations;
- errors caused by invalid authored declarations; and
- no physical execution storage.

`api/compiler` owns:

- lowering declarations into logical query nodes;
- type and shape validation across composed expressions;
- normalization and deterministic canonicalization;
- physical operator selection;
- compiled plan storage;
- compile diagnostics;
- plan summaries used by tests and profiling; and
- no provider policy.

`analysis/matching` owns:

- physical operator execution;
- occurrence and relation access;
- witness construction;
- budget charging;
- deterministic result ordering; and
- no authored declaration parsing.

`analysis/facts`, `analysis/resolution`, `analysis/value`, `analysis/flow`, and
`analysis/project` retain their current semantic ownership. Query work may add
reusable provider-neutral facts or indexes, but rules must not add their own
traversals or semantic models.

### Core representations

The exact Rust spelling should be refined during implementation, but the
architecture should converge on concepts equivalent to:

```rust
pub struct QueryDecl {
    expression: QueryExpr,
    emission: EmissionDecl,
}

pub enum QueryExpr {
    Event(EventDecl),
    All(AllExpr),
    Any(AnyExpr),
    Lifecycle(LifecycleDecl),
}

pub struct CompiledMatcherPlan {
    roots: Box<[PhysicalQuery]>,
    requirements: PlanRequirements,
}
```

Do not make `QueryExpr` a JSON-like recursive enum with unconstrained
`Vec<QueryExpr>` everywhere. Introduce semantic newtypes and domain collections
where they enforce:

- non-empty alternatives;
- variable uniqueness;
- compatible variable types;
- stable operator ordering;
- bounded child counts;
- keyed joins;
- valid result projection; and
- validated lifecycle stages.

Physical storage does not need to mirror the logical tree. A normalized
logical conjunction might compile directly into one indexed operator with
attached filters rather than a generic `Join(Filter(Scan(...)))` allocation.

## Semantic model

### Query result

Every successful query produces one or more classified witnesses. A witness
must contain enough internal information to derive:

- the rule index;
- primary occurrence;
- evidence kind and stable evidence symbol;
- certainty;
- supporting correlated evidence;
- analysis completeness relevant to that result; and
- deterministic ordering and deduplication keys.

A query is not merely boolean. It selects primary events and supporting
semantic evidence.

### Variables and types

Variables are the main extensibility mechanism. Each variable has one semantic
type, such as:

- `Event`;
- `CallEvent`;
- `MemberEvent`;
- `Object`;
- `CallableIdentity`;
- `NamespaceIdentity`;
- `StaticValue`;
- `ModuleIdentity`;
- `SymbolPath`; or
- another small provider-neutral domain type justified by the fact model.

Do not use one untyped `EntityId` for all variables. The compiler must reject:

- binding an event variable as a value;
- comparing artifact-local identities from different artifacts;
- joining unrelated key domains;
- projecting a non-location-bearing value as the primary occurrence;
- applying argument predicates to non-call events;
- applying member predicates to non-member subjects; and
- using a local-only relation in an unsupported project query.

Variable names are authoring concerns and should not remain in physical plans
unless needed for diagnostics or plan inspection. Runtime slots should use
dense validated indexes or semantic newtypes.

### Semantic relations

Define an explicit relation catalog before implementing a general query tree.
Each relation specification must document:

- input and output types;
- whether it can establish a strict witness;
- whether it can return unknown/incomplete;
- path-correlation key;
- local versus project scope;
- index or semantic service used to execute it;
- deterministic ordering;
- worst-case result bound;
- budget charged;
- evidence location behavior; and
- supported physical access paths.

The initial catalog should cover existing matcher semantics, approximately:

| Relation family | Existing source |
|---|---|
| Calls and constructions | call and construction occurrence indexes |
| Member calls and reads | member occurrence indexes |
| Classes | class occurrence indexes |
| Imports and strings | literal occurrence indexes |
| Global identity | scope/provenance-backed global indexes |
| Module export identity | local facts plus project identity overlays |
| Module namespace identity | local facts plus project identity overlays |
| Rooted member identity | rooted member indexes and environment aliases |
| Call arguments | canonical fact stream and static value arena |
| Returned subject | returned-member indexes or equivalent keyed relation |
| Constructed instance | instance-member indexes or equivalent keyed relation |
| Property writes | canonical fact stream and object-flow events |
| Object aliases | flow state with correlated alternatives |
| Lifecycle completion | local/cross-call flow engine |

This catalog is a semantic interface, not a public dump of internal maps.

### Boolean composition

Initial logical composition should support:

- `Any`: union of independently complete witnesses;
- `All`: conjunction over predicates sharing explicitly compatible variables;
  and
- leaf predicates that bind or constrain variables.

The compiler must reject an `All` expression whose branches have no legal
correlation key if it would require an uncontrolled Cartesian product.

`Any` semantics:

- normalize nested `Any`;
- reject empty `Any`;
- deduplicate equivalent branches;
- preserve deterministic branch order;
- merge duplicate results through the existing certainty/evidence policy; and
- do not let an unknown branch erase an independent complete witness.

`All` semantics:

- normalize nested `All`;
- reject empty `All`;
- attach single-event filters to the selecting scan where possible;
- require explicit shared variables for multi-event joins;
- preserve one path-correlation token across all contributing predicates;
- never combine evidence from incompatible alternatives;
- propagate incomplete state without fabricating a witness; and
- select one explicit primary event for result emission.

### Negation

Negation is not part of the first algebra migration.

Before adding it, specify:

- closed-world versus open-world behavior for every negatable relation;
- how unknown and budget exhaustion affect the result;
- stratification or another termination rule;
- whether absence is local, path-local, function-local, module-local, or
  project-wide;
- how negation affects `Definite` and `Possible`;
- which indexes can prove absence; and
- how evidence explains a result based on absence.

Only relations for which complete absence can be proven may participate in
strict negation. Unsupported or incomplete searches cannot establish a
negative witness.

### Ordering and lifecycle

Do not encode arbitrary temporal ordering as an integer-span comparison.
Source order alone is insufficient across:

- control-flow branches;
- callbacks;
- returns;
- aliases;
- cross-call effects; and
- modules.

Represent lifecycle and ordering using the flow engine's semantic event
sequence and correlated alternatives. Logical lifecycle declarations should
compile to specialized bounded state machines, not generic pairwise joins over
all events.

The first lifecycle model should retain the existing contract:

```text
one of source events
  -> configured by any/all required object events
  -> completed by configuration or one of the sinks
  -> emit correlated evidence
```

Later lifecycle improvements may add:

- multiple ordered stages;
- optional stages;
- named state transitions;
- sanitizing or invalidating transitions;
- bounded repetition;
- explicit escape/unknown transitions; and
- more general parameter/return propagation.

Each addition requires focused incompatible-path negatives.

### Certainty and incomplete analysis

The query system must treat certainty and completeness as semantic data, not
report decoration.

For every physical operator, document how it transforms:

- complete matching alternatives;
- complete non-matching alternatives;
- unknown alternatives;
- exhausted alternatives;
- `Definite` coverage;
- `Possible` coverage; and
- absence of a complete witness.

Required invariants:

- a `Definite` result means every modeled path reaching the primary occurrence
  satisfies the query and analysis is sufficiently complete to prove it;
- a `Possible` result contains at least one complete correlated witness;
- unknown or exhaustion cannot create a witness;
- unknown or exhaustion can prevent `Definite`;
- an unknown branch does not erase an independent complete witness;
- joining two possible facts is legal only when one correlated alternative
  contains both facts; and
- dropping alternatives due to a limit must be recorded and must never upgrade
  certainty.

### Evidence

Evidence emission must be explicit in logical queries. It should identify:

- which bound event is primary;
- evidence kind;
- stable evidence symbol;
- which supporting bindings may contribute related evidence; and
- any role labels required by the report model.

The compiler must validate that the primary binding:

- exists on every successful logical branch;
- has a source location;
- is semantically compatible with the requested evidence kind; and
- remains available after normalization.

Physical operators should carry compact witness references. They should not
format user-facing evidence during query execution.

## Boundedness and determinism contract

### Compile-time boundedness

Reject or specially plan query shapes that can produce uncontrolled work:

- unkeyed multi-event conjunctions;
- recursive predicates without a finite domain and explicit iteration bound;
- nested alternatives exceeding declaration limits;
- value predicates with unbounded authored collections;
- result projections without an evidence limit;
- lifecycle stages without bounded state;
- unrestricted transitive closure over open domains; and
- cyclic predicate dependencies without an accepted monotone fixed-point
  strategy.

Add query-specific declaration limits only when existing catalog/rule limits
cannot express the invariant. Limits must be validated domain newtypes rather
than magic integers distributed through the compiler.

### Runtime boundedness

Every physical operator must declare and charge work:

- index probes;
- candidate occurrences visited;
- filters evaluated;
- join candidates considered;
- correlated alternatives created or compared;
- flow transitions;
- fixed-point iterations;
- witness/evidence heads retained; and
- project/module overlay probes.

Prefer the existing unified analysis limits and operation accounting. Add a
new component only when it is independently actionable in diagnostics.

### Determinism

Canonicalize:

- logical branch order;
- predicate order where semantics permit reordering;
- variable slot assignment;
- normalized constants and symbol paths;
- physical root order;
- index probe order;
- join output order;
- lifecycle transition order;
- witnesses;
- evidence; and
- compile diagnostics.

Do not rely on hash iteration order. Optimization must not change finding or
evidence ordering.

## Migration principles

### One semantic path

During migration, existing authoring types may temporarily lower into the new
logical query declaration, but they must not retain their old executor.

Allowed temporary direction:

```text
MatcherDecl -> QueryDecl -> CompiledMatcherPlan -> one executor
```

Forbidden:

```text
MatcherDecl -> old QueryClause executor
QueryDecl   -> new relational executor
```

Once every rule uses the stable authoring API, remove obsolete declarations
and lowering adapters in the same migration.

### Behavior preservation

Before intentionally adding power, the new architecture must reproduce:

- exact rule IDs;
- finding counts;
- exact primary locations;
- evidence ordering;
- certainty;
- incomplete diagnostics;
- project/module linking behavior;
- operation-count expectations; and
- provider fixtures.

Any behavior change discovered during structural migration must be either:

- fixed as an implementation regression; or
- separated into a documented semantic change with focused positive and
  adversarial negative tests.

### Performance preservation

Do not replace indexed lookups with scans merely to obtain a uniform
abstraction. The logical layer should compile to the existing efficient paths.

Measure:

- catalog compilation;
- local lowering, which should remain matcher-independent;
- query projection operations;
- flow operations;
- maximum live alternatives;
- project fixed-point iterations;
- peak retained plan state; and
- representative corpus wall time as descriptive supporting data.

Operation counts and allocations are more useful regression contracts than
single-run wall time.

# Part I: Query and matcher implementation

## Phase 0: Establish the baseline and capability inventory

### Objective

Create a reviewed, test-backed inventory of current matcher semantics and
execution paths before changing types.

### Tasks

- [x] 1. Enumerate every `MatcherDeclBuilder` entry point.
- [ ] 2. Map each entry point to:
   - identity constraint;
   - event predicate;
   - subject constraint;
   - allowed value/argument constraints;
   - evidence kind;
   - local physical index;
   - project overlay behavior; and
   - certainty behavior.
- [ ] 3. Enumerate every `ObjectFlowMatcher` source, condition, completion, and sink
     form.
- [ ] 4. Record which rules use:
   - strict globals;
   - heuristic names;
   - exact modules;
   - package module patterns;
   - rooted members;
   - returned subjects;
   - constructed instances;
   - static values;
   - object properties;
   - object flows; and
   - cross-file/project identity.
- [ ] 5. Identify duplicated convenience methods and validation logic.
- [ ] 6. Record all execution entry points that inspect `QueryClause` or
     `CompiledObjectFlow`.
- [ ] 7. Add or update an internal capability matrix in compiler tests or a focused
     design appendix.
- [ ] 8. Capture baseline operation counts for representative simple, constrained,
     flow, and project queries.
- [ ] 9. Run the full provider fixture suite and save a deterministic report
     comparison suitable for development review.

### Required tests

- [x] One public core integration case for every distinct matcher capability.
- [x] Focused shadowing, reassignment, alias, lookalike, dynamic-value, and
      ambiguous-module negatives for strict identities.
- [x] Connected and disconnected object-flow cases.
- [ ] An incompatible-path negative for every flow relationship that crosses a
      join.
- [x] Existing minified/bundled-shape coverage.

### Exit criteria

- [x] Every public matcher constructor is accounted for.
- [ ] Every current execution path has a named owner.
- [ ] No known matcher behavior exists only implicitly in provider fixtures.
- [ ] Baseline report and operation data can detect structural migration
      regressions.

## Phase 1: Collapse the redundant plan wrapper

### Objective

Make `CompiledMatcherPlan` the single compiled plan container and remove the
one-field `QueryPlan` wrapper.

### Tasks

- [x] 1. Move clause and flow storage onto `CompiledMatcherPlan`.
- [x] 2. Move `clauses()` and `flows()` accessors onto `CompiledMatcherPlan`.
- [x] 3. Update occurrence matching, constrained projection, flow planning,
     project projection, and tests to accept `&CompiledMatcherPlan`.
- [x] 4. Remove `.query()` calls.
- [x] 5. Remove `QueryPlan`.
- [x] 6. Consolidate test-only and production compilation paths so flow
     validation is not accidentally bypassed by unit helpers.
- [x] 7. Keep storage private and expose only behavior needed by owning
     modules. (Fields are private; accessors remain `pub(crate)` for
     analysis-layer consumers.)
- [x] 8. Correct visibility: types in the private compiler module should not be
     declared `pub` without a cross-crate consumer.
- [x] 9. Update compiler documentation to describe the plan as compiled physical
     storage, not as a wrapper around another plan.

### Required tests

- [x] Compilation remains order-independent.
- [x] Equivalent declarations deduplicate identically.
- [x] Invalid clauses and flows fail through the same production validator.
- [x] All existing matcher and provider tests remain unchanged in behavior.

### Exit criteria

- [x] There is exactly one compiled-plan type. (`QueryPlan` removed)
- [x] No consumer unwraps another plan to execute it. (`.query()` removed)
- [x] Test compilation uses the same validation path as catalog compilation.
     (`compile` removed; all paths use `compile_decls`)

## Phase 2: Restore declaration/compiler ownership

### Objective

Stop storing compiler IR types directly inside public matcher declarations.

### Tasks

- [x] 1. Introduce declaration-owned semantic types under `api/rule/query`:
   - `IdentitySpec` — authored identity;
   - `EventSpec` — authored event;
   - `SubjectSpec` — authored subject;
   - `ArgumentConstraint` already lives in `api/rule/matcher` (pre-existing).
   - Evidence fields remain inline on `MatcherDecl`; lifecycle declaration is
     deferred to Task 9.
- [x] 2. Keep these types validated and provider-neutral.
- [x] 3. Move compiler-only `IdentityConstraint`, `EventPredicate`,
   `SubjectConstraint`, `QueryConstraint`, and `EvidenceDescriptor` out of the
   declaration representation. `MatcherDecl` now stores only declaration-owned
   types.
- [x] 4. Make lowering directional:

   ```text
   declaration types -> compiler logical types
   ```

   The compiler module has `lower_to_clause()` — the sole path from
   `MatcherDecl` to `QueryClause`.
- [x] 5. Remove all compiler-type imports from `api/rule/decl.rs`. The compiler
   depends directionally on declaration types through `use crate::api::rule::query::*`.
- [x] 6. Centralize module-pattern and symbol-path parsing in their owning semantic
   types (`ModuleSpecifierPattern` and `SymbolPath`, both pre-existing).
- [x] 7. Centralize validation. Builder methods reject local malformed input
   (empty names, malformed chains); the compiler's `QueryClause::validate()`
   enforces cross-dimension invariants. The builder checks argument-index
   bounds before compilation, and the compiler validates every lowered clause.
- [x] 8. Structured errors replace all `Generic(String)` variants:
   `ConstraintsOnNonCallEvent`, `InvalidLoweredQuery`, `EmptyFlowSymbol`,
   `EmptyFlowSources`, `MissingFlowCondition`, `MissingFlowSource`,
   `MissingFlowCompletion`, `DuplicateFlowOperation`. The `Generic` variant
   and its `From` impls have been removed.
- [x] 9. Object-flow declarations are first-class declaration variants. The
   `Rule` type now has a separate `flows: Vec<ObjectFlowMatcher>` collection
   instead of wrapping flows in a fake `MatcherDecl`. `RuleBuilder::object_flow()`
   accepts them directly; the compiler extracts flows from `rule.flow_matchers()`.
- [x] 10. Remove placeholder identity/event/evidence fields. `MatcherDecl` no
   longer has an `object_flow` field or `from_object_flow` constructor. All
   callers pass `ObjectFlowMatcher` values through the rule builder instead.

### Required tests

- [x] Declaration types cannot construct empty identities, paths, module
  specifiers, alternative lists, or invalid argument indexes (pre-existing
  builder tests).
- [x] Object-flow declarations no longer create a synthetic ordinary clause
  (verified by `make ci` — all flow fixtures pass without the old adapter).
- [x] Compiler lowering preserves every existing ordinary and flow behavior
  (verified by full `make ci` suite — 270+ tests pass).
- [x] Error ordering is deterministic when several authored fields are invalid
  (pre-existing determinism).

### Exit criteria

- [x] `api/rule` no longer stores compiler query types.
- [x] Object flow is represented honestly as a declaration form.
- [x] Compiler lowering is the only declaration-to-plan transition.

## Phase 3: Define the typed logical query algebra

### Objective

Introduce a small logical algebra that can express all current matchers without
encoding every combination as a distinct builder method.

### Initial operators

The initial algebra should include:

- event selection;
- identity constraint;
- value/argument constraint;
- subject relationship;
- `Any`;
- `All` over a shared event or explicitly bound compatible variables;
- lifecycle;
- evidence emission; and
- no general negation or recursion.

### Tasks

- [ ] 1. Define semantic variable types and dense compiler variable IDs.
- [ ] 2. Define leaf predicates independently from evidence metadata.
- [ ] 3. Define explicit binding:
   - select an event into a variable;
   - bind its subject, arguments, result, or identity where supported;
   - constrain an existing variable; and
   - prevent implicit cross-domain comparisons.
- [ ] 4. Define one explicit result/emission declaration per logical query root.
- [ ] 5. Define `Any` as a validated non-empty domain collection.
- [ ] 6. Define `All` as a validated non-empty domain collection with a legal shared
   correlation.
- [ ] 7. Define lifecycle as a semantic logical operator with typed source object,
   requirements, completion, and emission.
- [ ] 8. Define whether a rule contains:
   - one query with alternatives; or
   - several query roots whose results are unioned.

   Prefer one explicit query-set abstraction rather than relying on repeated
   builder calls as an undocumented union operator.
- [ ] 9. Specify equality and hashing for normalized logical nodes.
- [ ] 10. Specify stable diagnostic names for every operator and predicate.
- [ ] 11. Add a debug plan representation for tests. Keep it stable enough for
    focused assertions but do not make raw `Debug` snapshots a public schema.

### Design constraints

- Simple queries must not require manually declaring obvious variable types.
- Inference must remain local and deterministic.
- Ambiguous inference is a compile error.
- Logical nodes must not reference SWC types.
- Logical nodes must not reference artifact-local runtime IDs.
- Provider crates may compose declaration helpers but cannot add relations or
  executor callbacks.
- Any escape hatch requires a separately reviewed provider-neutral compiler
  extension, not an arbitrary closure.

### Example logical meaning

The existing global call matcher:

```rust
MatcherDecl::builder().call_global("fetch")
```

should lower conceptually to:

```text
select $call: CallEvent
where callee($call) is strict global "fetch"
emit $call as Call("fetch")
```

A static argument matcher should add:

```text
and argument($call, 0, $value)
and static_string($value)
```

The syntax above is explanatory, not a commitment to a textual grammar.

### Required tests

- [ ] Every old builder method lowers to a valid logical query.
- [ ] Semantically equivalent builder forms normalize equally.
- [ ] Invalid variable reuse is rejected.
- [ ] An uncorrelated multi-event `All` is rejected.
- [ ] Evidence projection from a variable absent in one `Any` branch is rejected.
- [ ] Empty `Any`, `All`, and lifecycle stages are rejected.

### Exit criteria

- [ ] All current matcher semantics have a representation in the logical algebra.
- [ ] Adding a new identity/event combination does not inherently require a new
  logical operator.
- [ ] Variables and correlation are explicit in compiler tests.

## Phase 4: Implement validation and type checking

### Objective

Make invalid, ambiguous, unsupported, or unbounded logical queries fail before
physical planning.

### Validation passes

Implement explicit passes rather than one large recursive validator:

- [ ] 1. declaration well-formedness;
- [ ] 2. symbol and variable collection;
- [ ] 3. variable type inference/checking;
- [ ] 4. operator compatibility;
- [ ] 5. correlation and scope checking;
- [ ] 6. evidence projection checking;
- [ ] 7. boundedness checking;
- [ ] 8. relation availability checking;
- [ ] 9. lifecycle validation; and
- [ ] 10. final invariant validation after normalization.

### Tasks

- [ ] 1. Introduce a structured `QueryCompileError`.
- [ ] 2. Assign stable diagnostic codes or variants for:
   - missing binding;
   - duplicate binding;
   - type mismatch;
   - invalid event predicate;
   - unsupported relation;
   - uncorrelated conjunction;
   - unavailable primary location;
   - invalid lifecycle;
   - unbounded query;
   - invalid module pattern;
   - invalid static-value predicate; and
   - internal lowering invariant violation.
- [ ] 3. Separate authored errors from internal compiler bugs.
- [ ] 4. Never panic on unsupported authored input.
- [ ] 5. Validate relation scope:
   - local;
   - function/call graph;
   - module;
   - project.
- [ ] 6. Validate strict versus heuristic identity requirements.
- [ ] 7. Validate evidence-kind compatibility.
- [ ] 8. Validate all authored collection limits before allocating large compiler
   structures.
- [ ] 9. Make compile errors deterministic regardless of hash or declaration order.
- [ ] 10. Add compact display messages and richer internal context for tests.

### Required tests

- [ ] One focused unit test per error variant.
- [ ] Stable error precedence for queries with multiple problems.
- [ ] Fuzz or property tests for validator non-panicking behavior if a suitable
  existing test dependency is available.
- [ ] Round-trip builder-to-logical validation for the full built-in catalog.

### Exit criteria

- [ ] The physical planner can assume a documented set of invariants.
- [ ] No invalid query shape reaches runtime.
- [ ] Errors identify the authored concept, not compiler enum debug output.

## Phase 5: Normalize logical queries

### Objective

Produce a canonical logical form suitable for deterministic planning,
deduplication, equivalence tests, and later language compilation.

### Tasks

- [ ] 1. Flatten nested `Any` and `All`.
- [ ] 2. Remove exact duplicate branches.
- [ ] 3. Canonicalize symbol paths and module patterns through owning types.
- [ ] 4. Canonicalize predicate ordering where conjunction semantics permit it.
- [ ] 5. Preserve authored order only where it affects:
   - lifecycle sequence;
   - evidence ordering;
   - diagnostic source selection; or
   - another explicitly documented semantic.
- [ ] 6. Assign dense variable slots deterministically.
- [ ] 7. Merge compatible filters on the same selected event.
- [ ] 8. Detect contradictions that can be rejected statically.
- [ ] 9. Do not simplify unknown-sensitive expressions using ordinary two-valued
   Boolean identities unless the certainty semantics prove the rewrite sound.
- [ ] 10. Compute plan requirements:
    - needed indexes;
    - needed fact-stream fields;
    - value resolution;
    - local flow;
    - cross-call flow;
    - project overlays; and
    - evidence trace support.
- [ ] 11. Give normalized queries structural equality independent of authored
    construction order where semantics are order-independent.

### Required tests

- [ ] Normalization is idempotent.
- [ ] Equivalent declaration order produces identical normalized queries.
- [ ] Duplicate alternatives do not duplicate findings.
- [ ] Lifecycle order is preserved.
- [ ] Unknown-sensitive expressions are not over-simplified.
- [ ] Variable slots are stable.

### Exit criteria

- [ ] Equivalent logical queries have one canonical representation.
- [ ] Planning never depends on incidental builder order.
- [ ] Normalization does not change certainty or evidence semantics.

## Phase 6: Introduce explicit physical plans

### Objective

Compile normalized logical queries into specialized executable operators rather
than letting runtime consumers inspect logical clauses and flows.

### Initial physical operators

Use domain-specific operators such as:

- indexed occurrence scan;
- module/global/rooted identity scan;
- literal/package scan;
- candidate filter;
- constrained call projection;
- returned-subject scan;
- instance-subject scan;
- keyed union;
- keyed correlated join where required;
- local lifecycle plan;
- cross-call lifecycle plan;
- project-overlay application; and
- evidence projection.

The exact enum should reflect executable ownership. Avoid a generic relational
operator when it would discard an existing optimized index or hide flow
semantics.

### Tasks

- [ ] 1. Add a planner from normalized logical roots to physical roots.
- [ ] 2. Move clause/flow routing decisions from runtime consumers into the planner.
- [ ] 3. Store plan-wide requirements once.
- [ ] 4. Select the narrowest available index for each event/identity pair.
- [ ] 5. Attach same-event value predicates directly to the scan or constrained
   projection.
- [ ] 6. Compile alternatives into deterministic root order.
- [ ] 7. Compile lifecycles into validated flow plans.
- [ ] 8. Represent project overlay needs explicitly rather than probing every plan.
- [ ] 9. Add physical plan validation.
- [ ] 10. Add a stable plan summary for tests/profiling, for example:

    ```text
    roots=3
    indexed_scans=2
    constrained_scans=1
    lifecycle_plans=0
    project_overlay=module_exports
    ```

- [ ] 11. Do not expose physical storage publicly.

### Physical-plan correctness

The planner must preserve:

- logical result set;
- certainty;
- correlation;
- primary occurrence;
- evidence order;
- incomplete diagnostics; and
- deterministic deduplication.

An optimization that changes any of these is a semantic change, not merely a
planner change.

### Required tests

- [ ] Each logical leaf selects the expected physical access path.
- [ ] Same-event filters fuse into one constrained operator.
- [ ] Alternatives retain deterministic order.
- [ ] Project-independent queries do not request project overlays.
- [ ] Planner output is stable across equivalent normalized queries.
- [ ] A reference evaluator or focused equivalence harness compares logical
  meaning and physical execution on small test artifacts where practical.

### Exit criteria

- [ ] Runtime execution receives physical plans only.
- [ ] No runtime consumer branches on authored declaration types.
- [ ] No runtime consumer independently decides whether a query is constrained,
  flow-based, or project-linked.

## Phase 7: Migrate ordinary indexed matchers

### Objective

Execute unconstrained calls, members, constructions, classes, imports, and
strings exclusively through physical query plans.

### Migration order

- [ ] 1. exact import and package import;
- [ ] 2. literal string reference;
- [ ] 3. global and heuristic call;
- [ ] 4. module and package-export call;
- [ ] 5. global/module/heuristic construction;
- [ ] 6. class reference;
- [ ] 7. rooted member call/read;
- [ ] 8. module/package namespace member call/read.

This order begins with simple index scans and ends with more identity-sensitive
member cases.

### Tasks for each family

- [ ] 1. Add logical lowering.
- [ ] 2. Add physical planning.
- [ ] 3. Execute through the owning occurrence index.
- [ ] 4. Preserve project overlay behavior.
- [ ] 5. Preserve masking of unresolved/relinked identities.
- [ ] 6. Preserve environment global-object aliases.
- [ ] 7. Preserve exact evidence kind, symbol, and location.
- [ ] 8. Port unit and integration tests.
- [ ] 9. Remove the corresponding old clause dispatch.

### Required adversarial coverage

- [ ] lexical shadowing;
- [ ] reassignment before use;
- [ ] reassignment after use;
- [ ] local same-name lookalikes;
- [ ] ESM aliases;
- [ ] CommonJS aliases;
- [ ] namespace imports;
- [ ] destructuring;
- [ ] interop forms;
- [ ] package root versus lookalike prefix;
- [ ] exact module versus package-boundary match;
- [ ] dynamic computed members;
- [ ] supported static computed members;
- [ ] ambiguous project exports; and
- [ ] minified identifier shapes.

### Exit criteria

- [ ] Ordinary indexed matching uses only the new physical plan executor.
- [ ] Old event/identity dispatch for migrated families is deleted.
- [ ] Findings and operation counts match the baseline unless explicitly updated.

## Phase 8: Migrate value and argument constraints

### Objective

Represent constrained call matching as ordinary logical predicates compiled to
the specialized constrained-event operator.

### Tasks

- [ ] 1. Define argument binding by index.
- [ ] 2. Define missing-argument behavior explicitly.
- [ ] 3. Lower all current `ArgumentMatcher` and `ValueMatcher` forms.
- [ ] 4. Preserve:
   - static string requirement;
   - equality alternatives;
   - contains alternatives;
   - prefix alternatives;
   - object keys;
   - object property values; and
   - any other current value matcher.
- [ ] 5. Keep value resolution in its owning analysis layer.
- [ ] 6. Compile all same-call constraints into one projection operation.
- [ ] 7. Avoid evaluating static values repeatedly for separate predicates on the
   same argument.
- [ ] 8. Preserve fail-closed behavior for dynamic, ambiguous, unsupported, or
   exhausted values.
- [ ] 9. Make value-predicate evidence explicit rather than changing it as a side
   effect of attaching an argument.
- [ ] 10. Remove duplicate `with_arg_*` and `arg_*` semantics after the authoring API
    has one canonical route.

### Required tests

- [ ] Accepted static values.
- [ ] Rejected dynamic values.
- [ ] Aliased constants.
- [ ] Reassigned constants.
- [ ] Object literal keys and properties.
- [ ] Missing argument.
- [ ] Sparse argument positions.
- [ ] Several constraints on one call.
- [ ] Several constraints on one argument.
- [ ] Constraint order independence.
- [ ] Bounded large alternative sets.

### Exit criteria

- [ ] Constrained calls are logical queries, not a special authored matcher
  family.
- [ ] Static-value semantics remain centralized.
- [ ] The physical executor performs one bounded projection per candidate call.

## Phase 9: Migrate returned-object and instance relationships

### Objective

Express subject relationships through typed bindings and keyed relations rather
than dedicated builder combinations.

### Tasks

- [ ] 1. Model a returned subject as a relation between:
   - producer identity/event;
   - returned object identity; and
   - member event.
- [ ] 2. Model an instance subject as a relation between:
   - constructor identity/event;
   - constructed instance identity; and
   - instance member event.
- [ ] 3. Require the member event and producer/constructor to share the same object
   correlation.
- [ ] 4. Preserve supported chained forms and intentionally unsupported forms.
- [ ] 5. Preserve strict module identity for constructed instances.
- [ ] 6. Preserve rooted/environment semantics for returned objects.
- [ ] 7. Compile supported shapes to existing returned and instance indexes where
   possible.
- [ ] 8. Add a keyed join only if the existing index cannot express a new supported
   relationship.
- [ ] 9. Remove duplicated producer/constructor identity stored in both identity and
   subject fields.
- [ ] 10. Make evidence projection choose the member occurrence while optionally
    retaining producer/constructor support.

### Required tests

- [ ] Direct returned-object member use.
- [ ] Alias of returned object.
- [ ] Reassignment of alias.
- [ ] Disconnected same-name object.
- [ ] Direct and aliased constructed instance.
- [ ] Static method lookalike.
- [ ] Wrong constructor module.
- [ ] Subclass behavior according to current contract.
- [ ] Chained constructor behavior according to current contract.
- [ ] Incompatible-branch producer/member negative.

### Exit criteria

- [ ] Subject relations have one explicit correlation model.
- [ ] Existing returned/instance convenience methods lower to that model or are
  replaced.
- [ ] No identity is duplicated solely to satisfy old validation.

## Phase 10: Unify object flow with the logical query model

### Objective

Make lifecycle matching a first-class logical operator compiled into the
existing bounded flow engine.

### Tasks

- [ ] 1. Replace `MatcherDecl::from_object_flow` with a direct lifecycle query
   declaration.
- [ ] 2. Remove synthetic heuristic call fields from flow declarations.
- [ ] 3. Define typed lifecycle components:
   - source event;
   - tracked object binding;
   - condition;
   - requirement events;
   - completion;
   - sink argument relationship;
   - invalidation/unknown policy; and
   - emission.
- [ ] 4. Lower current `AnyOf` and `AllOf` conditions.
- [ ] 5. Lower configuration completion and any-sink completion.
- [ ] 6. Lower exact argument and any-argument sinks.
- [ ] 7. Compile lifecycle declarations to immutable local/cross-call flow plans.
- [ ] 8. Move all flow validation into declaration/compiler ownership.
- [ ] 9. Reuse query value predicates for source, requirement, and sink constraints.
- [ ] 10. Preserve correlated alternatives at joins.
- [ ] 11. Preserve object alias and reassignment behavior.
- [ ] 12. Preserve cross-call summaries and fixed-point bounds.
- [ ] 13. Preserve exact evidence source/requirement/sink ordering.
- [ ] 14. Remove parallel `CompiledObjectFlow` compilation entry points once the
    physical lifecycle plan owns that state.

### Required tests

- [ ] Every current object-flow provider rule.
- [ ] Any requirement.
- [ ] All requirements.
- [ ] Configuration completion.
- [ ] Exact sink argument.
- [ ] Any sink argument.
- [ ] Multiple sources.
- [ ] Multiple sinks.
- [ ] Aliased tracked objects.
- [ ] Reassigned tracked objects.
- [ ] Escaped/unsupported objects.
- [ ] Dynamic source discriminator.
- [ ] Dynamic requirement value.
- [ ] Disconnected source and sink.
- [ ] Requirement on one path and sink on another.
- [ ] Source on one path and requirement on another.
- [ ] Cross-call source/requirement/sink combinations.
- [ ] Budget exhaustion without fabricated evidence.

### Exit criteria

- [ ] Object lifecycle is part of the logical query model.
- [ ] There is no fake ordinary clause for a flow-only declaration.
- [ ] There is one compiled lifecycle representation and one execution route.

## Phase 11: Make project and cross-file requirements explicit

### Objective

Plan project linking, identity overlays, and cross-call flow as query
requirements rather than implicit behavior in projection callers.

### Tasks

- [ ] 1. Annotate physical plans with required project capabilities.
- [ ] 2. Build overlays only for selected plans that require them.
- [ ] 3. Preserve exact versus package module identity.
- [ ] 4. Preserve ambiguous, missing, and unknown resolution behavior.
- [ ] 5. Preserve masking when a local imported identity is remapped or unresolved.
- [ ] 6. Make cross-file flow plans explicitly reference compiled lifecycle roots.
- [ ] 7. Keep findings in the file containing the primary event.
- [ ] 8. Keep module interfaces matcher-independent.
- [ ] 9. Charge project overlay and fixed-point work deterministically.
- [ ] 10. Add plan summaries showing local/project/cross-call requirements.

### Required tests

- [ ] Direct external module import.
- [ ] Re-export chain.
- [ ] Namespace re-export.
- [ ] CommonJS/ESM interop.
- [ ] Ambiguous export.
- [ ] Missing resolution.
- [ ] Package-boundary matching.
- [ ] Cross-file call/return flow.
- [ ] Finding location in the sink/primary file.
- [ ] Independent complete witness coexisting with an unknown project alternative.

### Exit criteria

- [ ] Project projection does not inspect logical predicates.
- [ ] Queries that need no project semantics incur no project-query preparation.
- [ ] Cross-file execution uses the same compiled plan roots as local execution.

## Phase 12: Replace the authoring builder with compositional query declarations

### Objective

Expose a coherent Rust authoring API over the logical query model.

### Decision gate

Choose between:

- typed combinators;
- a declarative `macro_rules!` frontend;
- a small procedural macro in a dedicated crate; or
- a combination where combinators are canonical and a macro is sugar.

Do not make this decision from aesthetics alone. Rewrite representative rules
and compare:

- readability;
- invalid states representable;
- compile errors;
- IDE navigation;
- provider-local helper composition;
- generated code size;
- Rust compile time; and
- ease of future textual lowering.

### Representative rules

The authoring spike must include:

- [ ] one simple global call;
- [ ] one exact/package module API family;
- [ ] one rooted member family;
- [ ] one static argument/value rule;
- [ ] one returned-object rule;
- [ ] one constructed-instance rule;
- [ ] one object lifecycle;
- [ ] one helper-generated rule family such as remote DOM resources; and
- [ ] one rule with many alternatives.

### API requirements

- [ ] Simple matchers remain compact.
- [ ] Alternatives are explicit.
- [ ] Conjunction and variable sharing are visible.
- [ ] Evidence emission is explicit or has a safe obvious default.
- [ ] Lifecycle queries read in semantic order.
- [ ] Provider-local helpers can accept ordinary Rust values and iterators.
- [ ] All constructed queries pass through the same validator.
- [ ] No API exposes compiler physical types.
- [ ] No arbitrary callback can inspect facts.
- [ ] Names reflect semantics rather than syntax-tree shapes.

### Migration tasks

- [ ] 1. Introduce the selected authoring API.
- [ ] 2. Migrate core integration tests first.
- [ ] 3. Migrate `glass-lint-js` rule families.
- [ ] 4. Migrate `glass-lint-obsidian` rule families.
- [ ] 5. Migrate project test support and harness fixtures.
- [ ] 6. Remove `MatcherDeclBuilder` methods superseded by composition.
- [ ] 7. Rename `MatcherDecl` to `QueryDecl` or another final term if the new type is
   no longer meaningfully a matcher record.
- [ ] 8. Remove old constructors and compatibility aliases.
- [ ] 9. Update public examples and crate READMEs.

### Required tests

- [ ] Compile-fail tests for invalid authoring combinations if practical.
- [ ] Full catalog compilation.
- [ ] Full provider fixtures.
- [ ] Exact equivalence for representative rules before and after migration.

### Exit criteria

- [ ] Every built-in rule uses the compositional authoring API.
- [ ] The old builder and old matcher record are deleted.
- [ ] Provider rules no longer reveal physical executor families.

## Phase 13: Add the first genuinely new relational capability

### Objective

Prove that the architecture increases query power rather than merely renaming
existing matchers.

### Recommended first capability

Add bounded multi-event correlation over one explicit semantic identity or
value. Candidate examples:

- [ ] a proven API result later passed to a second proven API;
- [ ] two calls on the same returned or constructed object;
- [ ] a call argument related to a later sink argument; or
- [ ] a required event before a completion event using existing flow semantics.

Choose a capability demanded by at least two real provider rules or one
high-value rule that cannot be accurately expressed today.

### Tasks

- [ ] 1. Write positive and adversarial negative examples before implementation.
- [ ] 2. Specify the relation and correlation key.
- [ ] 3. Specify certainty under joins and incomplete alternatives.
- [ ] 4. Specify evidence projection.
- [ ] 5. Specify physical access path and bounds.
- [ ] 6. Add logical validation.
- [ ] 7. Add normalization.
- [ ] 8. Add physical planning.
- [ ] 9. Add execution and operation accounting.
- [ ] 10. Implement provider rules without callbacks or custom traversal.
- [ ] 11. Profile representative projects.

### Required adversarial tests

- [ ] same spelling, different identity;
- [ ] correct events on different objects;
- [ ] correct events on incompatible branches;
- [ ] reassignment between events;
- [ ] dynamic or unknown connecting value;
- [ ] ambiguous module identity;
- [ ] unsupported escape;
- [ ] exhausted alternatives; and
- [ ] independent complete witness alongside unknown alternatives.

### Exit criteria

- [ ] At least one useful query is expressible without adding a family-specific
  builder method.
- [ ] The query compiles to bounded specialized operators.
- [ ] No precision invariant is weakened.

## Phase 14: Query optimizer and plan quality

### Objective

Improve physical plan selection only after semantic equivalence is well tested.

### Candidate optimizations

- [ ] choose the narrowest identity/event index;
- [ ] push static filters into indexed scans;
- [ ] share repeated static-value resolution;
- [ ] reorder commutative keyed predicates by estimated candidate count;
- [ ] deduplicate identical scans within one rule;
- [ ] share immutable compiled constants across rules;
- [ ] pre-group selected physical roots by required index;
- [ ] batch module-overlay probes;
- [ ] fuse evidence projection with the final operator;
- [ ] use semijoins rather than materialized joins where only existence matters;
  and
- [ ] use specialized lifecycle indexes for common source/sink shapes.

### Constraints

- No optimizer rule may change evidence order.
- Cost estimates must be deterministic.
- Runtime data-dependent optimization must charge its own work.
- Plan caching must include every semantic and limit input that affects the
  plan.
- Sharing between rules must not mix rule indexes, evidence symbols, limits,
  or certainty.

### Required tests

- [ ] Optimized and unoptimized reference plans produce identical results.
- [ ] Canonical plan choice is stable.
- [ ] Operation-count tests demonstrate the intended improvement.
- [ ] Worst-case query shapes remain bounded.

### Exit criteria

- [ ] Optimization is driven by measured query workloads.
- [ ] Plan explanations show why a physical path was selected.
- [ ] The unoptimized semantic contract remains understandable and testable.

## Phase 15: Stabilize query diagnostics, inspection, and documentation

### Objective

Make the query system maintainable before considering a textual language.

### Tasks

- [ ] 1. Add compiler documentation for:
   - logical operators;
   - semantic relation catalog;
   - certainty;
   - correlation;
   - boundedness;
   - evidence emission; and
   - physical planning.
- [ ] 2. Add an internal plan-explain facility for tests and profiling.
- [ ] 3. Include compile diagnostics in catalog construction errors without leaking
   internal IDs.
- [ ] 4. Document how to add a provider-neutral relation.
- [ ] 5. Document when to add a specialized physical operator.
- [ ] 6. Document required tests for new query behavior.
- [ ] 7. Update `ARCHITECTURE.md`, `glass-lint-core/ARCHITECTURE.md`,
   `CONTRIBUTING.md`, and `TESTING.md`.
- [ ] 8. Remove obsolete matcher terminology and diagrams.
- [ ] 9. Add examples for every supported query capability.
- [ ] 10. Audit public API size and visibility.

### Exit criteria

- [ ] A contributor can add a query capability without discovering hidden
  executor routes.
- [ ] Plan explanations are deterministic and useful in regression tests.
- [ ] Architecture documents describe only the new path.

## Part I completion gate

Matcher/query architecture is complete when:

- `CompiledMatcherPlan` is the sole physical plan container;
- declaration types do not store compiler IR;
- all built-in rules use one compositional query declaration API;
- all ordinary, constrained, subject, lifecycle, local, and project matching
  compiles through one logical query compiler;
- runtime executes physical plans without inspecting authored declarations;
- variables and correlation are typed and explicit;
- at least one new multi-event capability uses the algebra;
- strict identity, path correlation, certainty, incomplete analysis, evidence,
  bounds, and determinism are preserved;
- old matcher and flow compilation paths are deleted;
- operation counts show no unexplained regressions;
- documentation describes the final architecture; and
- `make ci` passes.

# Part II: Future textual query language

## Status and prerequisites

The textual language is future work. Do not begin implementation until every
Part I completion criterion is satisfied.

Additionally, require evidence that a textual language solves an actual
authoring or distribution problem:

- external users need to write rules without recompiling Rust;
- provider catalogs need data-driven distribution;
- query iteration speed is materially limited by Rust compilation;
- tooling needs portable query files; or
- a sufficiently large internal rule corpus demonstrates that textual queries
  are clearer than the stable Rust API.

If built-in Rust rules remain the only consumers and typed combinators remain
clear, the correct outcome may be not to add a textual language.

## Language principles

- The language is a frontend, not an engine.
- It compiles to the same validated logical queries as the Rust API.
- It cannot access SWC nodes or private facts directly.
- It cannot register callbacks.
- It cannot weaken query limits.
- It cannot bypass strict identity or certainty semantics.
- It cannot introduce provider policy into core.
- It has a small versioned grammar.
- It reports precise source spans.
- It is deterministic and safe to parse from untrusted input within explicit
  size and complexity limits.
- It starts with the smallest feature set proven by built-in rules.

## External systems to study

Study, but do not copy wholesale:

- CodeQL/QL for typed predicates, variables, result projection, modules, and
  recursive relational thinking;
- Datalog and Soufflé for finite relations, monotone fixed points, and static
  analysis formulation;
- Semgrep for approachable rule files, source/sink authoring, diagnostics, and
  editor workflow;
- Tree-sitter query syntax for compact captures and source-span diagnostics,
  while explicitly rejecting its syntax-tree execution model;
- ast-grep for readable relational operators, while retaining Glass Lint's
  semantic rather than tree-relative relations;
- Ascent for Rust-embedded Datalog and lattice experiments; and
- Datafrog for a small monotone fixed-point prototype.

Run a bounded representative spike before adopting any evaluator. Compare an
external engine against the existing specialized executor on:

- certainty representation;
- correlated path alternatives;
- unknown/incomplete propagation;
- deterministic limits;
- evidence;
- local/project scope;
- performance;
- binary size;
- build complexity; and
- maintenance ownership.

Default to retaining Glass Lint's physical executor unless the prototype shows
a clear correctness and maintenance improvement.

## Language phase L0: Define users and distribution

### Questions to answer

- [ ] Who authors query files?
- [ ] Are they trusted provider maintainers or arbitrary end users?
- [ ] Are queries compiled into provider crates, loaded at startup, or loaded per
  project?
- [ ] Are query files allowed outside an installed provider?
- [ ] How are provider namespaces assigned?
- [ ] Can a query define only matching semantics, or also rule metadata?
- [ ] How are versions declared?
- [ ] What is the compatibility policy?
- [ ] Are compiled plans cacheable across runs?
- [ ] How are query files discovered without moving filesystem responsibility into
  core?
- [ ] Which crate owns loading and admission?

### Architectural rule

`glass-lint-project` or provider/catalog code may load source text according to
its ownership, but `glass-lint-core` owns parsing validated query text into
provider-neutral declarations. Core must not discover query files.

### Exit criteria

- [ ] There is a written user and distribution model.
- [ ] The need for text rather than Rust API/macros is demonstrated.
- [ ] Crate ownership is explicit.

## Language phase L1: Design the surface grammar

### Initial language features

The first language version should support only:

- query declaration;
- typed or inferable variables;
- event selection;
- strict/heuristic identity predicates;
- module and package identities;
- member/rooted identities;
- argument and static-value predicates;
- returned and constructed subjects;
- `any` and correlated `all`;
- lifecycle source/requirements/completion;
- result/evidence emission;
- comments;
- string literals;
- lists; and
- imports of a small provider-neutral standard query library if needed.

Exclude initially:

- arbitrary recursion;
- unrestricted negation;
- aggregation;
- user-defined functions with general computation;
- regex unless a bounded regex engine and clear need are established;
- filesystem access;
- environment access;
- network access;
- arbitrary Rust calls;
- query-generated fixes;
- report formatting; and
- provider profile logic.

### Strawman syntax

The following illustrates semantics only:

```text
query network_request {
  find $call: call
  where {
    callee($call) == global("fetch")
    argument($call, 0, $url)
    static_string($url)
  }
  emit $call as call_argument("fetch")
}
```

An alternative:

```text
query request_api {
  any {
    find call(global("fetch"))
    find call(module("obsidian", "requestUrl"))
  }
  emit primary as call("request")
}
```

A lifecycle:

```text
query script_injection {
  let $element = returned_by call(rooted("document.createElement"))
    where argument(0) == static_string("script")

  require any {
    property_write($element, "src", static_string())
    property_write($element, "text", static_string())
    property_write($element, "textContent", static_string())
  }

  complete any {
    argument_of($element, rooted("document.head.appendChild"), 0)
    argument_of($element, rooted("document.body.appendChild"), 0)
  }

  emit completion as flow("script-element")
}
```

Do not finalize tokens until representative queries are written in at least
two candidate syntaxes and reviewed for:

- readability;
- ambiguity;
- compositionality;
- source diagnostics;
- diff quality;
- formatter behavior; and
- one-to-one mapping to logical declarations.

### Exit criteria

- [ ] The grammar maps directly to the stable logical algebra.
- [ ] No syntax feature exists without defined semantics, validation, and bounds.
- [ ] Representative built-in rules are readable without hidden defaults.

## Language phase L2: Parser and source model

### Tasks

- [ ] 1. Choose a parser implementation based on the finalized grammar:
   - a small hand-written recursive descent parser;
   - `winnow`;
   - another focused Rust parser library; or
   - a dedicated generated parser if diagnostics justify it.
- [ ] 2. Keep the parser dependency private to core.
- [ ] 3. Introduce:
   - query source ID;
   - byte spans;
   - line index integration;
   - tokens;
   - parsed nodes;
   - parse diagnostics; and
   - explicit source-size/nesting/token limits.
- [ ] 4. Reject invalid UTF-8 at the loading boundary or define the accepted source
   encoding explicitly.
- [ ] 5. Support comments and trailing separators consistently.
- [ ] 6. Recover from syntax errors only where recovery cannot create misleading
   secondary diagnostics.
- [ ] 7. Never execute partially parsed queries.
- [ ] 8. Fuzz the lexer/parser for panics, excessive allocation, and pathological
   nesting.

### Parsed representation

The parsed syntax tree exists to retain:

- authored names;
- exact source spans;
- syntactic grouping;
- comments if a formatter needs them; and
- diagnostics.

It must lower promptly into the same declaration/logical types used by Rust
rules. It must not reach physical execution.

### Exit criteria

- [ ] Parser resource use is bounded.
- [ ] Diagnostics point to precise spans.
- [ ] Malformed queries never produce executable plans.

## Language phase L3: Name resolution and type checking

### Tasks

- [ ] 1. Resolve query names and local variables.
- [ ] 2. Resolve standard predicates.
- [ ] 3. Resolve provider-local helper predicates only through an explicit module
   system.
- [ ] 4. Infer obvious variable types.
- [ ] 5. Require annotations when inference is ambiguous.
- [ ] 6. Detect duplicate, unused, unbound, and shadowed names.
- [ ] 7. Validate relation scope.
- [ ] 8. Validate evidence projection.
- [ ] 9. Preserve multiple diagnostics only when their order and independence are
   deterministic.
- [ ] 10. Lower successful queries into the stable logical declaration model.

### Module system

Keep the initial module system small:

- explicit imports;
- no implicit filesystem search inside core;
- no cyclic imports initially;
- no wildcard imports initially;
- provider namespace isolation;
- deterministic name resolution; and
- a bounded import graph validated by the loader.

### Exit criteria

- [ ] Text and Rust declarations produce equivalent normalized logical queries.
- [ ] Type errors use authored names and spans.
- [ ] Name resolution cannot access undeclared provider policy.

## Language phase L4: Versioning and persisted format

### Tasks

- [ ] 1. Add an explicit language version.
- [ ] 2. Define whether version appears per file, catalog, or package.
- [ ] 3. Reject unsupported future versions.
- [ ] 4. Decide whether minor additive evolution exists or every grammar change
   increments one integer.
- [ ] 5. Version any serialized compiled-plan cache independently from the language.
- [ ] 6. Include:
   - engine version;
   - language version;
   - provider/catalog identity;
   - normalized query fingerprint;
   - relevant analysis limits; and
   - relation schema version
   in any compiled cache key.
- [ ] 7. Never treat compiled plans as a stable public interchange format unless
   explicitly designed and audited as one.

### Compatibility

Prefer a clean current language version during active development. Once
third-party queries exist, document:

- support window;
- migration tooling;
- deprecation policy;
- error behavior; and
- whether old language versions are translated or rejected.

Do not leave ad hoc compatibility code in the physical planner.

### Exit criteria

- [ ] Query source has an explicit compatibility contract.
- [ ] Stale compiled plans cannot be reused under changed semantics.

## Language phase L5: Diagnostics and author tools

### Required tools

- [ ] parser/type-check command;
- [ ] deterministic formatter;
- [ ] normalized logical query display;
- [ ] physical plan explanation;
- [ ] relation/predicate reference;
- [ ] query test harness;
- [ ] source-span diagnostics;
- [ ] unused binding warning;
- [ ] unreachable/contradictory branch warning where sound; and
- [ ] operation/limit profiling for a query against a fixture.

### Integration

Decide whether these tools belong in:

- the existing CLI;
- harness CLI;
- a dedicated development binary; or
- library APIs used by provider tooling.

Keep production CLI behavior thin. Reusable checking and formatting logic
belongs in the owning core/project/provider layer.

### Exit criteria

- [ ] Authors can parse, check, format, test, and explain a query without running a
  full production scan.
- [ ] Tool output is deterministic and fixture-testable.

## Language phase L6: Catalog loading and security

### Tasks

- [ ] 1. Define trusted and untrusted query sources.
- [ ] 2. Validate source byte, token, nesting, declaration, alternative, predicate,
   and literal limits.
- [ ] 3. Compile each catalog once.
- [ ] 4. Deduplicate identical normalized queries where safe.
- [ ] 5. Report all catalog errors with query source locations.
- [ ] 6. Keep filesystem discovery and loading out of core.
- [ ] 7. Prevent query text from selecting unrestricted analysis limits.
- [ ] 8. Prevent query packages from registering native callbacks.
- [ ] 9. Prevent query names from escaping provider namespaces.
- [ ] 10. Decide whether unsigned third-party catalogs are supported; if so, keep
    signature/distribution policy outside core matching semantics.
- [ ] 11. Treat query strings and evidence symbols as untrusted display content.
- [ ] 12. Bound formatter and diagnostic output.

### Exit criteria

- [ ] Loading untrusted malformed queries cannot panic or allocate without bound.
- [ ] Catalog compilation produces immutable ordinary `CompiledMatcherPlan`
  values.
- [ ] Runtime cannot distinguish whether a plan originated in Rust or text.

## Language phase L7: Representative migration

### Objective

Prove authoring value before moving the entire built-in catalog.

### Pilot set

Migrate or duplicate in a non-production equivalence harness:

- [ ] one simple JS global rule;
- [ ] one Node module rule;
- [ ] one browser rooted-member rule;
- [ ] one Electron module/instance rule;
- [ ] one Obsidian module rule;
- [ ] one constrained static-value rule;
- [ ] one object lifecycle; and
- [ ] one large generated alternative family.

### Comparison

For each pilot, compare:

- [ ] source length;
- [ ] readability;
- [ ] duplication;
- [ ] diagnostics;
- [ ] compilation time;
- [ ] normalized plan;
- [ ] findings;
- [ ] certainty;
- [ ] evidence;
- [ ] operation counts;
- [ ] provider fixture ergonomics; and
- [ ] ability to express provider-local shared constants/helpers.

### Decision

After the pilot, explicitly choose one of:

1. textual queries become the canonical built-in rule form;
2. text is supported for external catalogs while built-in rules remain Rust;
3. text is useful only for selected simple rules; or
4. the language does not justify its maintenance cost and remains
   experimental or is removed.

Do not assume full migration is automatically desirable.

## Language phase L8: Optional full rule migration

### Preconditions

- [ ] Pilot decision selects textual queries as canonical or materially useful.
- [ ] Language diagnostics and tooling are production-ready.
- [ ] Provider-local composition has a satisfactory design.
- [ ] Query loading does not violate crate ownership.
- [ ] Performance is acceptable.

### Tasks

- [ ] 1. Migrate rule families in small reviewable groups.
- [ ] 2. Keep metadata ownership in provider catalogs unless intentionally redesigned.
- [ ] 3. Compare every migrated rule against its previous normalized plan and
   fixtures.
- [ ] 4. Remove Rust query declarations only when the language fully replaces them.
- [ ] 5. Remove duplicate provider helpers made obsolete by language modules.
- [ ] 6. Update documentation and contributor workflow.
- [ ] 7. Retain the Rust declaration API if it remains the supported embedding API;
   otherwise remove it in one clean breaking migration.

### Exit criteria

- [ ] Each rule has one canonical declaration source.
- [ ] No runtime or compiler path differs between Rust-authored and text-authored
  queries.
- [ ] Full provider, project, e2e, and comparison suites pass.

## Language phase L9: Later language capabilities

Consider only in response to concrete rule requirements:

- named reusable predicates;
- parameterized predicates;
- stratified negation over provably complete relations;
- monotone recursion over finite domains;
- transitive closure;
- lattice-valued certainty or flow state;
- bounded aggregation;
- reusable lifecycle fragments;
- query modules/packages;
- result projections with richer evidence roles; and
- language-server support.

Every capability requires:

- semantics;
- type rules;
- normalization;
- physical planning;
- bounds;
- certainty behavior;
- evidence behavior;
- diagnostics;
- adversarial tests; and
- measured provider demand.

Do not add general language features merely because the parser can express
them.

# Testing strategy

## Unit tests

Place focused unit tests beside:

- declaration invariants;
- logical type checking;
- correlation validation;
- normalization;
- physical planning;
- each physical operator;
- lifecycle compilation;
- compile diagnostics;
- parser and language lowering when added; and
- formatter/tooling when added.

## Core integration tests

Use `glass-lint-core/tests` for:

- public query authoring behavior;
- strict identity;
- values;
- returned/instance subjects;
- flow;
- certainty;
- evidence;
- project-independent semantic behavior; and
- adversarial negatives.

Do not expose compiler internals merely to test them from integration tests.

## Provider contracts

Every migrated or newly expressive provider rule retains colocated
`positive.js` and `negative.js` coverage. Add:

- supported import/call forms;
- shadowing/lookalikes;
- reassignment;
- aliases;
- dynamic values;
- disconnected flow;
- incompatible paths;
- minified shapes; and
- exact expected certainty/location.

## Project tests

Use explicit virtual-project resolutions for:

- external modules;
- re-exports;
- ambiguity;
- unknown/missing resolution;
- cross-file flow;
- primary finding location; and
- project-level certainty.

## Equivalence tests

During migration, build a test-only comparison harness that can:

- compile an old declaration and its new logical equivalent;
- execute both against the same immutable artifact only while both paths are
  needed for the test;
- compare normalized findings, certainty, evidence, and status; and
- be deleted when the old executor is deleted.

Do not ship dual production executors.

Where possible, compare normalized plans rather than relying on duplicated
execution.

## Property tests

Good candidates:

- normalization idempotence;
- commutative alternative/conjunction order independence;
- deterministic variable slot assignment;
- deduplication;
- validator non-panicking behavior;
- parser non-panicking behavior;
- optimized/unoptimized plan equivalence; and
- evidence ordering stability.

## Performance tests

Add operation-count regression tests for:

- large alternative families;
- many constrained calls;
- many module identities;
- returned/instance indexes;
- object flows with joins;
- cross-call fixed points;
- project overlays; and
- queries that approach declared bounds.

Use corpus profiling as supporting evidence, not as the only regression test.

## Phase completion commands

Run the narrowest relevant test while iterating. Before completing any phase
that changes matching semantics or execution:

```sh
make ci
```

For provider migrations, also run the narrow provider harness command from
`TESTING.md` before the full gate.

# Documentation changes

Update documentation in the phase that makes it inaccurate:

- root `ARCHITECTURE.md` for pipeline or crate-boundary changes;
- `glass-lint-core/ARCHITECTURE.md` for query compiler/executor ownership;
- `glass-lint-core/README.md` for public authoring examples;
- provider READMEs only when provider authoring changes;
- `CONTRIBUTING.md` for adding queries and relations;
- `TESTING.md` for query-specific coverage and tools;
- CLI/harness documentation for language checking or formatting commands; and
- report schema documentation only if query evidence changes it.

Avoid maintaining a separate exhaustive relation reference in several files.
Generate or link to one source of truth where practical.

# Risks and mitigations

## Risk: A generic query layer slows simple rules

Mitigation:

- compile logical queries to current indexes;
- fuse same-event predicates;
- avoid allocating generic rows for simple scans;
- measure operation counts from Phase 0 onward.

## Risk: Relational joins break path correlation

Mitigation:

- make correlation keys typed and mandatory;
- reject unkeyed conjunctions;
- carry correlated alternatives through physical operators;
- require incompatible-path negatives for every new join.

## Risk: Datalog-style closed-world assumptions break unknown semantics

Mitigation:

- do not implement ordinary negation initially;
- model completeness explicitly;
- specify certainty transfer per operator;
- require complete witnesses for findings.

## Risk: Declaration, logical, and physical types become three near-copies

Mitigation:

- declaration types enforce author-facing invariants;
- logical types exist only where composition/type checking requires them;
- physical types reflect execution;
- remove fields that are cloned without transformation;
- keep parser AST ephemeral if the language is added.

## Risk: The language freezes the wrong abstractions

Mitigation:

- complete Part I first;
- add a new capability before syntax;
- pilot several syntaxes;
- version the language;
- keep text and Rust frontends targeting the same logical API.

## Risk: External engines conflict with Glass Lint limits

Mitigation:

- prototype representative strict and flow queries;
- measure certainty, evidence, bounds, and performance;
- adopt only isolated useful techniques unless full equivalence is proven.

## Risk: Query errors become hard to understand

Mitigation:

- structured compiler errors;
- authored source spans;
- stable relation/operator names;
- plan explanations;
- focused error tests.

## Risk: Two matcher paths survive migration

Mitigation:

- route old declarations through the new compiler immediately;
- delete each old dispatch family as it migrates;
- forbid production feature flags selecting executors;
- include obsolete-path removal in every phase exit criterion.

## Risk: Provider helpers move policy into core

Mitigation:

- core exposes only provider-neutral relations and values;
- provider-local query helpers remain in provider crates or future provider
  query modules;
- require at least two provider-neutral uses before adding a convenience
  primitive to core unless it represents fundamental semantic structure.

# Review checkpoints

Require an explicit architecture review after:

1. the relation catalog and logical algebra are proposed;
2. certainty and correlation semantics are written;
3. physical plan operators are selected;
4. ordinary and constrained matchers are migrated;
5. lifecycle flow is unified;
6. the first new relational capability is implemented;
7. the stable Rust authoring API is selected; and
8. any decision to start the textual language.

Each review should answer:

- Is the capability provider-neutral?
- Does it preserve matcher-independent facts?
- Can it be planned through existing semantic owners?
- Is every join correlated?
- Is work bounded and charged?
- Are results deterministic?
- Are certainty and incomplete analysis correct?
- Is evidence explainable?
- Is the public API smaller or at least more compositional?
- Is obsolete code removed?

# Recommended first implementation sequence

The first practical series of changes should be:

1. add baseline capability/operation tests;
2. collapse `QueryPlan` into `CompiledMatcherPlan`;
3. make object flow an honest declaration variant;
4. introduce declaration-owned identity/event/subject types;
5. define logical event selection, `Any`, same-event `All`, and emission;
6. lower current ordinary declarations into the logical form;
7. add validation and canonical normalization;
8. compile ordinary logical queries back to the existing occurrence indexes;
9. migrate constrained arguments and values;
10. migrate returned and instance subjects;
11. compile lifecycle queries to the existing flow planner;
12. make project requirements explicit;
13. replace the family-oriented builder with the selected compositional Rust
    API;
14. implement one bounded multi-event query required by real rules;
15. optimize and document the stable system; and
16. revisit whether a textual language is justified.

This order improves the matcher/query system immediately, creates clean
compiler boundaries, and avoids committing to syntax before the semantic
language is known.
