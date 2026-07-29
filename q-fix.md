# Query architecture remediation plan

## Purpose

This document describes the work required to make Phases 0 through 12 of
[`q-plan.md`](q-plan.md) genuinely complete.

The current implementation preserves the existing built-in rule behavior and
passes `make ci`, but several architectural claims in `q-plan.md` are either
still unchecked or are contradicted by the implementation. In particular:

- [ ] composed `Any` and `All` queries do not compile through the production
  catalog path;
- [ ] the variable type-checking pass is a no-op;
- [ ] evidence projection is not checked on every successful branch;
- [ ] normalization does not canonicalize or merge event predicates completely;
- [ ] physical `All` planning assumes compatibility it has not proved;
- [ ] argument constraints are not compiled into canonical per-argument groups;
- [ ] returned-object and instance relationships remain special record shapes
  rather than explicit typed correlations;
- [ ] lifecycle rules still use a separate `ObjectFlowMatcher` authoring and
  storage path;
- [ ] several physical plan requirements are descriptive booleans rather than
  execution-driving requirements;
- [ ] Phase 0 baselines and capability inventories are missing;
- [ ] the physical planner lacks the required focused logical-equivalence oracle;
  and
- [ ] public documentation still shows removed APIs.

This is a forward migration. Breaking changes are allowed. Do not add
compatibility aliases, parallel executors, or deprecated authoring paths.

## Required reading before implementation

Read these files in full before changing code:

- [ ] [`ARCHITECTURE.md`](ARCHITECTURE.md)
- [ ] [`glass-lint-core/ARCHITECTURE.md`](glass-lint-core/ARCHITECTURE.md)
- [ ] [`glass-lint-project/ARCHITECTURE.md`](glass-lint-project/ARCHITECTURE.md)
- [ ] [`glass-lint-js/ARCHITECTURE.md`](glass-lint-js/ARCHITECTURE.md)
- [ ] [`glass-lint-obsidian/ARCHITECTURE.md`](glass-lint-obsidian/ARCHITECTURE.md)
- [ ] [`TESTING.md`](TESTING.md)
- [ ] [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [ ] Phases 0 through 12 of [`q-plan.md`](q-plan.md)

Inspect `git status` before every implementation slice and preserve unrelated
changes.

## Non-negotiable contracts

Every change below must preserve these contracts:

1. Parsing and matcher-independent fact construction happen once per file.
2. Rules do not traverse syntax or build private semantic models.
3. Strict witnesses remain path-local and provenance-aware.
4. Facts from incompatible control-flow alternatives never form one witness.
5. Unknown, dynamic, ambiguous, unsupported, or exhausted analysis cannot
   establish a witness or a definite result.
6. An independent complete witness may still produce a possible result when
   another relevant alternative is unknown.
7. Work, state, result counts, recursion, evidence, and diagnostics remain
   bounded.
8. Results, evidence, diagnostics, plans, and operation counts remain
   deterministic.
9. Provider policy stays outside `glass-lint-core`.
10. Runtime code receives physical plans, not authored query declarations.
11. There is one authoring path, one compiler pipeline, and one physical
    execution representation.
12. Expected invalid input returns a structured error. It must not panic or be
    silently ignored.

## Target end state

The final path should be:

```text
RuleBuilder::query(QueryDecl)
  -> validated declaration-owned query terms
  -> scope-aware typed logical validation
  -> canonical normalized logical query
  -> validated physical roots plus executable requirements
  -> occurrence, constrained-value, lifecycle, and project executors
  -> deterministic classified witnesses
```

The final `Rule` should retain only query declarations:

```rust
pub struct Rule {
    // metadata
    queries: Vec<QueryDecl>,
}
```

Delete `QuerySet`. `Rule.queries: Vec<QueryDecl>` is the one explicit query-set
abstraction, and repeated `RuleBuilder::query()` calls are documented union
semantics. Keeping a second unused collection wrapper adds no invariant.

The final compiled matcher should retain only the physical plan:

```rust
pub(crate) struct CompiledMatcherPlan {
    physical_plan: PhysicalPlan,
}
```

Lifecycle plans must be represented only as
`PhysicalRoot::Lifecycle { ... }`. There must be no parallel `flows` field,
`flow_matchers()` accessor, or `RuleBuilder::object_flow()` entry point.

## Binding design decisions

These decisions are final for the Phase 0–12 remediation:

| Area | Decision |
|---|---|
| Constructor errors | Text/index/collection constructors return `Result`; no expected-input panic |
| Fluent rule API | Sealed `IntoQueryDecl` lets `RuleBuilder::query` accept `QueryDecl` or its construction result |
| Declaration storage | Fields are private and collections are validated, bounded domain types |
| Query-set model | Delete `QuerySet`; `Rule.queries: Vec<QueryDecl>` is the documented union |
| Logical variables | Keep opaque `VarId`; infer compiler-only `VarType` |
| Binding semantics | `SelectEvent` binds; `Require` references; object relations bind explicit object variables |
| `Any` | Branch-local scopes with alpha-aligned primary output |
| `All` | Public API supports same-event conjunction only through Phase 12 |
| General joins | Do not add one before Phase 13 |
| Subject relations | Delete `SubjectSpec`; use explicit returned/constructed object predicates |
| Normalized IR | Add compiler-only `NormalizedQuery`; no normalized `All` variant |
| Contradictions | Reject with structured `ContradictoryPredicate` at catalog construction |
| Argument execution | Canonical per-index groups; prepare each argument once per candidate |
| Lifecycle API | Rename flow terms to lifecycle terms and author them only through `QueryDecl` |
| Compiled lifecycle | Store only `PhysicalRoot::Lifecycle`; delete copied `flows` storage |
| Requirements | Use deterministic `BTreeSet` domain requirements plus `FlowRequirements` |
| Equivalence testing | Test-only synthetic logical and physical evaluators in `compiler/query/reference.rs` |
| Public examples | Store as compiled examples and mirror them in README documentation |

## Final module layout

Split the current oversized query and compiler modules into this layout during
the migration:

```text
glass-lint-core/src/api/rule/query/
  mod.rs          public re-exports only
  error.rs        QueryBuildError and bounded-construction errors
  limits.rs       query declaration limits
  value.rs        ArgumentIndex, ArgumentConstraint, ArgumentMatcher, ValueMatcher
  event.rs        IdentitySpec, EventSpec, EventQuery, EventRequirement
  expression.rs   VarId, QueryExpr, AnyExpr, AllExpr, EmissionDecl, QueryDecl
  lifecycle.rs    LifecycleSource/Event/Condition/Completion/Sink/Query/Builder

glass-lint-core/src/api/compiler/query/
  mod.rs          validate -> normalize -> plan orchestration
  error.rs        QueryCompileError and QueryDiagnostic projection
  validate.rs     scope-aware typed passes
  normalize.rs    compiler-only normalized IR
  physical.rs     physical roots, requirements, planner, validation
  reference.rs    #[cfg(test)] logical/physical equivalence oracle
```

Move value and argument declarations out of
`api/rule/matcher/flow.rs`; delete `api/rule/matcher` when no callers remain.
Keep execution in its current owners:

- [ ] indexed scans in `analysis/matching/query`;
- [ ] argument evaluation in `analysis/matching/arguments`;
- [ ] local lifecycle execution in `analysis/flow/projector`;
- [ ] cross-call/cross-file lifecycle execution in `analysis/flow/cross`; and
- [ ] project preparation in `analysis/project/projection`.

Do not combine module movement and semantic behavior in one unreviewable
mechanical commit. Move each type when its owning package is implemented and
delete the emptied old module in Package 8.

## Implementation order

Implement the following work packages in order. A package is complete only
when its focused tests pass and no temporary compatibility route remains.

---

## Package 0: Add regression tests for the known false-complete claims

### Objective

Make the current architectural gaps visible through the production public API
before changing implementation.

### Tests to add first

Add public integration tests, preferably in a focused
`glass-lint-core/tests/query_composition.rs` module:

- [x] `any_branches_compile_through_rule_catalog`
  - [x] construct two event alternatives;
  - [x] use one logical primary event variable in both branch scopes;
  - [x] compile with `RuleCatalog::new`;
  - [x] assert both alternatives match independently.
  - _Currently fails: variable_collection treats Any branches as one flat scope (Package 3)_
- [x] `any_requires_primary_evidence_on_every_branch`
  - [x] construct an `Any` whose emission is unavailable on one branch;
  - [x] assert a stable structured compile error.
  - _Currently passes validation incorrectly (Package 3)_
- [x] `same_event_all_compiles_through_rule_catalog`
  - [x] compose two compatible constraints on one selected event;
  - [x] compile through `RuleCatalog::new`;
  - [x] assert both predicates are required.
  - _Currently fails: variable_collection rejects same-var branches in All (Package 3)_
- [x] `uncorrelated_all_fails_through_rule_catalog`
  - [x] select unrelated events without a keyed relation;
  - [x] assert `uncorrelated_conjunction`.
  - _Works through catalog; error is stringified, not structured_
- [x] `contradictory_same_event_all_fails_at_compilation`
  - [x] apply mutually exclusive constraints to the same event or argument;
  - [x] assert a structured contradiction error.
  - _Currently compiles successfully — no contradiction detection (Package 4)_
- [x] `multiple_lifecycle_sources_compile`
  - [x] declare at least two valid source forms;
  - [x] compile through the same `RuleBuilder::query()` route used by ordinary
    queries.
  - _Currently fails: lifecycle source vars checked in flat scope (Package 8)_
- [x] `invalid_authoring_input_never_panics`
  - [x] cover empty names, malformed symbol paths, invalid module patterns,
    excessive argument indexes, empty value alternatives, and invalid
    lifecycle collections.
  - _Currently panics via assert! — constructors not fallible (Package 2)_
- [x] `query_modifiers_do_not_silently_ignore_non_event_expressions`
  - [x] applying an event-only modifier to `Any`, `All`, or lifecycle must return a
    structured error rather than returning the original query unchanged.
  - _Currently silently ignores via `if let QueryExpr::Event(...)` (Package 2)_

Add compiler unit tests that exercise the complete
validate-normalize-plan pipeline. Do not call `plan_normalized` directly on
declarations that have not passed production validation.

### Exit criteria

- [x] Each listed test fails against the current implementation for the expected
  reason. (7 of 8 fail; uncorrelated_all passes because validation correctly
  detects it through the catalog.)
- [x] The tests use public catalog construction where the behavior is public.
- [x] Unit tests may target individual passes, but they do not substitute for the
  production-pipeline tests.

---

## Package 1: Finish the Phase 0 capability inventory and baselines

### Objective

Replace the obsolete pre-migration inventory with a reviewed inventory of the
current and target query API. Do not resurrect `MatcherDeclBuilder`.

### Capability matrix

Add a checked-in matrix at
`glass-lint-core/QUERY_CAPABILITIES.md`, with one row per author-visible
capability and these columns:

| Field | Required content |
|---|---|
| Authoring constructor | Exact `EventQuery`, `QueryDecl`, or lifecycle entry point |
| Logical identity | Global, heuristic, rooted, exact module, package module, literal |
| Event | Call, construction, member call/read, class, import, string |
| Subject relation | Direct, returned object, constructed instance, lifecycle object |
| Constraints | Supported argument/value forms and applicable event kinds |
| Evidence | Default kind, symbol, primary event, support evidence |
| Local operator | Exact physical root and owning index/service |
| Project behavior | Overlay, masking, cross-file identity, or none |
| Certainty behavior | Definite/possible/unknown rules |
| Provider users | Built-in rule families using the capability |
| Focused tests | Unit, core integration, project, and provider coverage |

The matrix must include:

- [x] strict global calls and constructions;
- [x] heuristic calls, constructions, members, and classes;
- [x] exact and package module exports;
- [x] exact and package module namespaces;
- [x] exact and package imports;
- [x] rooted member calls and reads;
- [x] returned-object calls and reads;
- [x] constructed-instance calls;
- [x] static string predicates;
- [x] exact, prefix, contains-any, and contains-all predicates;
- [x] object key, object property, and rooted-expression arguments;
- [x] lifecycle sources;
- [x] `AnyOf` and `AllOf` lifecycle conditions;
- [x] configuration and sink completion;
- [x] exact and any-argument sinks;
- [x] local, cross-call, and cross-file lifecycle execution; and
- [x] evidence/certainty behavior under incomplete analysis.

### Execution ownership inventory

Record the owner and entry point for:

- [x] indexed occurrence execution;
- [x] constrained fact-stream projection;
- [x] returned-subject execution;
- [x] instance-subject execution;
- [x] local lifecycle projection;
- [x] cross-call lifecycle summaries;
- [x] cross-file lifecycle projection;
- [x] module identity overlay construction;
- [x] evidence normalization and deduplication; and
- [x] operation-count charging.

After the lifecycle migration, this inventory must not mention
`CompiledMatcherPlan::flows()`.

### Baseline artifacts

Create deterministic regression baselines for representative:

- [x] simple indexed query;
- [x] constrained call;
- [x] returned-object query;
- [x] constructed-instance query;
- [x] local lifecycle;
- [x] local lifecycle (within-function);
- [x] project module identity;
- [x] ambiguous project alternative; and
- [x] cross-file flow (via operation counts).

Put the human-readable baseline and regeneration command in
`reports/QUERY_MIGRATION_BASELINE.md`. Put exact executable assertions in
`glass-lint-core/tests/query_baseline.rs`; the Markdown report is explanatory,
not the test oracle. Assert focused stable operation fields rather than one
opaque report snapshot. The report must include:

- [x] fixed source inputs;
- [x] fixed environment and selected rules;
- [x] exact completion state;
- [x] exact finding/evidence order;
- [x] exact stable operation counts; and
- [x] the command used to regenerate it.

Run and record the full provider fixture summary:

```text
tests/e2e
tests/projects
glass-lint-js/src/rules
glass-lint-obsidian/src/rules
```

### Flow join negatives

Add an incompatible-path negative for every flow relationship that joins
independently retained state:

- [x] source to alias — `negative_source_to_alias_no_sink`;
- [x] source to requirement — `negative_source_to_requirement_no_sink`;
- [x] source to sink — `negative_disconnected_source_and_sink`;
- [x] alias to requirement — `negative_alias_to_requirement_no_sink`;
- [x] alias to sink — `negative_alias_to_sink_not_configured`;
- [x] requirement to sink — `negative_requirement_to_sink_disconnected_object`;
- [x] caller argument to callee parameter — existing `helper_summaries_fail_closed_for_incompatible_invocations`;
- [x] callee return to caller result — existing `flow_control_paths_retain_reachable_possible_witnesses`; and
- [x] cross-file source/requirement/sink propagation — existing project flow tests.

Reuse a smaller lower-layer case when it proves the invariant. Do not copy the
same large fixture into every layer.

### Exit criteria

- [x] Every current authoring capability maps to an owner, physical route, provider
  user, and focused test — see `QUERY_CAPABILITIES.md`.
- [x] No behavior is documented only in provider fixtures — every capability row
  links to unit or integration tests.
- [x] Baselines can detect a change in physical routing or operation counts —
  `query_baseline.rs` asserts exact finding counts, completion state, evidence
  traces, and stable operation fields.
- [x] All formerly unchecked Phase 0 items can be checked with linked evidence —
  capability matrix, execution ownership inventory, baseline report, and flow
  join negatives all reference specific files, types, and test assertions.

---

## Package 2: Make declaration construction fallible and invariant-preserving

### Objective

Prevent public declarations from panicking, silently discarding modifiers, or
exposing freely mutable invalid storage.

### Current problems

- [ ] constructors such as `EventQuery::call_global` use `assert!`;
- [ ] package constructors use `expect`;
- [ ] `EventQuery`, `AnyExpr`, `AllExpr`, `LifecycleQuery`, `EmissionDecl`, and
  `QueryDecl` expose public fields;
- [ ] `ArgumentConstraint::new` accepts every `usize`;
- [ ] value predicate alternatives can be empty or excessively large; and
- [ ] `QueryDecl::with_arg*` silently does nothing unless the expression is a
  direct `Event`.

### Required changes

- [ ] Make logical declaration fields private.
- [ ] Provide narrow accessors required by compiler lowering.
- [ ] Introduce validated semantic newtypes or fallible constructors for:
  - [ ] non-empty identity names;
  - [ ] symbol paths;
  - [ ] exact module specifiers;
  - [ ] package module patterns;
  - [ ] evidence symbols;
  - [ ] bounded argument indexes;
  - [ ] non-empty bounded predicate alternative sets;
  - [ ] non-empty `Any`/`All`; and
  - [ ] valid lifecycle stages.
- [ ] Define these declaration limits in `api/rule/query/limits.rs` and use them in
  construction and compiler invariant checks:

  ```rust
  pub const MAX_QUERY_ROOTS_PER_RULE: usize = 256;
  pub const MAX_EXPR_CHILDREN: usize = 256;
  pub const MAX_ARGUMENT_INDEX: usize = 255;
  pub const MAX_ARGUMENT_GROUPS: usize = 64;
  pub const MAX_PREDICATES_PER_ARGUMENT: usize = 32;
  pub const MAX_STATIC_ALTERNATIVES: usize = 256;
  pub const MAX_LIFECYCLE_SOURCES: usize = 64;
  pub const MAX_LIFECYCLE_EVENTS: usize = 64;
  pub const MAX_LIFECYCLE_SINKS: usize = 64;
  ```

  Do not retain the compiler's generic `1_000` limits.
- [ ] Return `Result<_, QueryBuildError>` from every authoring constructor that
  accepts text, a raw index, or a collection.
- [ ] Preserve fluent rule authoring with this exact error propagation contract:

  ```rust
  pub trait IntoQueryDecl {
      fn into_query_decl(self) -> Result<QueryDecl, QueryBuildError>;
  }

  impl IntoQueryDecl for QueryDecl { /* Ok(self) */ }
  impl IntoQueryDecl for Result<QueryDecl, QueryBuildError> { /* identity */ }

  impl RuleBuilder {
      pub fn query(mut self, query: impl IntoQueryDecl) -> Self;
  }
  ```

  Seal `IntoQueryDecl` so external crates cannot invent conversions. Store the
  first `QueryBuildError` on `RuleBuilder` in authored order. Add
  `RuleBuildError::InvalidQuery(QueryBuildError)` and return it after metadata
  duplicate checks but before required-query validation. This keeps provider
  code compact:

  ```rust
  Rule::builder("network.request")
      .query(QueryDecl::call_global("fetch"))
      .build()?
  ```

  Do not add `try_query`, `query_unchecked`, or an infallible duplicate
  constructor.
- [ ] Do not use `unwrap`, `expect`, or assertions for expected provider or user
  input.
- [ ] Event-only modifiers must be methods on `EventQuery`, not generic
  `QueryDecl` methods that can ignore other expression variants.
- [ ] `QueryDecl` exposes only the expression-level combinators specified in
  Package 3. It does not expose raw fields.
- [ ] Canonicalize bounded alternative sets at construction:
  - [ ] validate non-empty values;
  - [ ] sort deterministically;
  - [ ] deduplicate;
  - [ ] reject values or collection sizes over the declared limit.
- [ ] Introduce a bounded semantic type for argument positions. The compiler must
  never receive an unchecked raw index.

### Error model

Keep declaration and compiler errors distinct:

- [ ] `QueryBuildError`: malformed local constructor input;
- [ ] `QueryCompileError`: invalid cross-expression relationship;
- [ ] internal compiler invariant error: a bug after validated lowering.

Do not erase `QueryCompileError` into an unstructured string before the catalog
boundary. Add this public diagnostic projection:

```rust
pub struct QueryDiagnostic {
    code: &'static str,
    message: String,
}

pub enum CompiledCatalogError {
    InvalidQuery {
        rule_id: RuleId,
        diagnostic: QueryDiagnostic,
    },
    // existing non-query catalog errors
}
```

Keep the richer private `QueryCompileError` until the final catalog boundary,
then project it into `QueryDiagnostic`. `Display` formats these structured
fields; it does not become their storage.

### Required tests

- [ ] one test per `QueryBuildError` variant;
- [ ] collection boundary tests at limit and limit plus one;
- [ ] a deterministic table-driven `catch_unwind` test over malformed public query
  inputs; do not add a property-testing dependency for this migration;
- [ ] event-only modifiers reject non-event queries;
- [ ] equivalent predicate alternative order constructs equal declarations; and
- [ ] all built-in provider catalogs compile through the fallible path.

### Exit criteria

- [ ] Invalid author input cannot panic.
- [ ] Public callers cannot directly construct empty or malformed logical nodes.
- [ ] No modifier silently leaves an unsupported query unchanged.
- [ ] Catalog errors preserve stable structured query diagnostics.

---

## Package 3: Implement scope-aware typed variables and usable composition

### Objective

Make `Any`, `All`, emission projection, and semantic correlations work through
the production compiler.

### Semantic decision

Use a private logical kind behind the public `QueryExpr` wrapper. Do not keep
the current public recursive enum with mutable fields:

```rust
pub struct QueryExpr {
    pub(crate) kind: QueryExprKind,
}

pub(crate) enum QueryExprKind {
    SelectEvent(EventSelection),
    Require(QueryPredicate),
    Any(AnyExpr),
    All(AllExpr),
    Lifecycle(LifecycleQuery),
}

pub(crate) struct EventSelection {
    bind: VarId,
}

pub(crate) enum QueryPredicate {
    EventKind {
        event: VarId,
        expected: EventSpec,
    },
    EventIdentity {
        event: VarId,
        expected: IdentitySpec,
    },
    Argument {
        call: VarId,
        index: ArgumentIndex,
        matcher: ArgumentMatcher,
    },
    ReturnedObject {
        bind: VarId,
        producer: ProducerSpec,
    },
    ConstructedObject {
        bind: VarId,
        constructor: ConstructorSpec,
    },
    MemberSubject {
        event: VarId,
        object: VarId,
    },
}
```

`SelectEvent` is the only event-binding atom. `Require` atoms only reference
existing bindings, except `ReturnedObject` and `ConstructedObject`, which bind
their declared object variable. This explicit bind/reference distinction
replaces the current interpretation that every repeated `VarId` is another
binding.

Use this exact semantic type set in the compiler:

```rust
pub(crate) enum VarType {
    Event,
    CallEvent,
    MemberEvent,
    Object,
    StaticValue,
    CallableIdentity,
    ModuleIdentity,
    SymbolPath,
}
```

`VarId` remains the opaque authored identifier. `VarType` is inferred and
stored only during compilation. Do not add separate public `EventVar`,
`ObjectVar`, or `ValueVar` wrappers in this migration.

Keep `EventQuery` as the compact public leaf builder, with private fields and
fallible methods:

```rust
impl EventQuery {
    pub fn call_global(name: impl Into<String>) -> Result<Self, QueryBuildError>;
    pub fn with_arg(
        self,
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError>;
    pub fn into_query(self) -> QueryDecl;
}
```

Lower one `EventQuery` into an `All` containing one `SelectEvent` plus
`EventKind`, `EventIdentity`, argument, and subject `Require` atoms. The
normalizer fuses that form into one normalized event node before planning.

Expose composition with these constructors:

```rust
pub struct EventRequirement {
    pub(crate) kind: EventRequirementKind,
}

pub(crate) enum EventRequirementKind {
    Argument {
        index: ArgumentIndex,
        matcher: ArgumentMatcher,
    },
}

impl EventRequirement {
    pub fn argument(
        index: usize,
        matcher: impl Into<ArgumentMatcher>,
    ) -> Result<Self, QueryBuildError>;
}

impl QueryDecl {
    pub fn any(
        branches: impl IntoIterator<Item = Result<QueryDecl, QueryBuildError>>,
    ) -> Result<Self, QueryBuildError>;

    pub fn all(
        event: Result<EventQuery, QueryBuildError>,
        requirements: impl IntoIterator<
            Item = Result<EventRequirement, QueryBuildError>,
        >,
    ) -> Result<Self, QueryBuildError>;
}
```

`QueryDecl::any` alpha-aligns the branch primary event variables to one output
slot and rejects incompatible evidence kinds. `QueryDecl::all` is the advanced
same-event composition entry point. `EventQuery::with_arg` remains shorthand
that constructs the same `EventRequirement::argument` predicate; both forms
must normalize equally. Do not expose constructors for raw `SelectEvent` or
`Require` atoms publicly. Compiler tests inside the crate construct those atoms
through `pub(crate)` test helpers. Returned, instance, and lifecycle
constructors add their typed object relations internally; arbitrary
author-defined multi-event joins remain out of scope until Phase 13.

Compiler lowering introduces object variables for returned-object, instance,
and lifecycle relationships in deterministic source order. Normalization
assigns dense slots for validation and plan inspection. Physical planning
compiles all variables away; no `VarId` or runtime variable slot is stored in a
Phase 12 physical root.

Delete `QuerySet` and its validation/normalization/planning helpers while
introducing this model. `Rule.queries()` is the compiler input and repeated
`RuleBuilder::query()` calls are the documented union of query roots.

### Scope rules

Implement a scope-aware validator with these semantics:

- [ ] An `Event` selector binds its declared event variable once in its scope.
- [ ] `Any` branches have independent binding scopes.
- [ ] A variable projected after `Any` must be bound with one compatible type on
  every successful branch.
- [ ] `All` evaluates branches in one correlation scope.
- [ ] A binding may occur only once in an `All` scope.
- [ ] Other `All` atoms may reference that binding.
- [ ] A same-event conjunction must refer to one selected event; it must not model
  correlation by selecting the event twice.
- [ ] Public multi-event conjunctions are rejected as `UnsupportedRelation`
  through Phase 12. Returned-object and instance constructors are the only
  multi-event relationships; they introduce compiler-owned object correlation
  predicates and lower to specialized indexes.
- [ ] A local-only relation cannot be used in an unsupported project context.
- [ ] Artifact-local identities cannot be compared across artifacts.

Have validation return or internally compute a typed binding summary rather
than repeatedly rescanning the tree:

```text
expression
  -> branch-local binding environments
  -> merged output environment
  -> emission availability and type
```

### Fix `Any`

- [ ] Permit the same logical output variable name/slot to be bound independently
  in separate alternatives.
- [ ] Validate branch output compatibility.
- [ ] Require the primary evidence variable on every successful branch.
- [ ] Preserve independent complete witnesses when another branch is unknown.
- [ ] Deduplicate equal witnesses after execution without changing certainty.

### Fix `All`

- [ ] Represent same-event filters as references to one event binding.
- [ ] Reject uncorrelated multi-event conjunctions as
  `UncorrelatedConjunction`.
- [ ] Reject other public multi-event conjunctions as `UnsupportedRelation` until
  Phase 13.
- [ ] Reject incompatible uses of a shared variable.
- [ ] Preserve one path-correlation key across every contributing predicate.
- [ ] Do not plan an `All` by taking the first event and copying constraints from
  unrelated event selectors.

### Type checking

Replace the no-op `pass_type_checking` with a fallible pass that can produce:

- [ ] missing binding;
- [ ] duplicate binding;
- [ ] incompatible branch output;
- [ ] type mismatch;
- [ ] invalid relation operand;
- [ ] unavailable primary location; and
- [ ] unsupported relation scope.

Remove `#[allow(dead_code)]` from error variants that should be produced by the
compiler. If a planned error is not meaningful for the final algebra, remove
it and update `q-plan.md`; do not keep nominal variants solely to claim
coverage.

### Required tests

- [ ] `Any` branch-local binding succeeds.
- [ ] `Any` with incompatible output types fails.
- [ ] `Any` missing the primary variable in one branch fails.
- [ ] Same-event `All` succeeds.
- [ ] Duplicate binding in one `All` scope fails.
- [ ] Reference before binding fails.
- [ ] Uncorrelated events fail.
- [ ] Event/value, event/object, and local/project type mismatches fail.
- [ ] Emission from a non-location-bearing variable fails.
- [ ] Nested `Any` in `All` retains correlation and certainty.
- [ ] Incompatible control-flow paths never satisfy a conjunction.
- [ ] Full public catalog construction exercises these cases, not just individual
  validation passes.

### Exit criteria

- [ ] The public regression tests from Package 0 pass.
- [ ] `Any` and same-event `All` are usable through `RuleCatalog::new`.
- [ ] Type checking is real and produces its structured error variants.
- [ ] Emission is valid on every successful branch.
- [ ] No runtime component sees authored variables or logical declarations.

---

## Package 4: Complete canonical normalization

### Objective

Give equivalent logical queries one representation before physical planning.

### Normalized representation

Introduce compiler-only normalized types. Do not reuse `QueryDecl` as the
normalized IR:

```rust
pub(crate) struct NormalizedQuery {
    root: NormalizedRoot,
    emission: NormalizedEmission,
    requirements: PlanRequirements,
}

pub(crate) enum NormalizedRoot {
    Event(NormalizedEvent),
    Any(Box<[NormalizedRoot]>),
    Lifecycle(NormalizedLifecycle),
}

pub(crate) struct NormalizedEvent {
    slot: u32,
    event: EventSpec,
    identity: IdentitySpec,
    subject: NormalizedSubject,
    arguments: Box<[NormalizedArgumentGroup]>,
}

pub(crate) enum NormalizedSubject {
    Direct,
    Returned {
        producer: ProducerSpec,
    },
    Instance {
        constructor: ConstructorSpec,
    },
}
```

There is no normalized `All` variant. Normalization must turn:

- [ ] same-event `All` into one `NormalizedEvent`;
- [ ] an uncorrelated multi-event `All` into `UncorrelatedConjunction`; and
- [ ] every other multi-event `All` into `UnsupportedRelation`.

Do not add a general keyed join in the Phase 0–12 remediation. Returned-object
and instance relationships lower to `NormalizedSubject` and their existing
specialized indexes; lifecycle uses `NormalizedLifecycle`. The first general
keyed join belongs to Phase 13 with the first genuinely new relational
capability.

`NormalizedQuery` fields stay private to `api/compiler`. Physical planning
accepts `&NormalizedQuery` only.

### Required normalization order

Use one documented order such as:

- [ ] 1. recursively normalize children;
- [ ] 2. flatten nested same-kind `Any` and `All`;
- [ ] 3. canonicalize semantic paths, module patterns, and predicate sets;
- [ ] 4. merge compatible same-event filters;
- [ ] 5. detect contradictions;
- [ ] 6. sort order-independent branches;
- [ ] 7. deduplicate equal branches;
- [ ] 8. alpha-normalize variables into deterministic dense slots;
- [ ] 9. validate normalized invariants; and
- [ ] 10. compute exact plan requirements.

Variable renumbering must be independent of author-assigned numeric `VarId`
values and incidental construction order. Add alpha-equivalence tests that use
different original IDs.

### Same-event filter merging

Merge only when the compiler has proved all selectors refer to the same event
binding and have compatible:

- [ ] event kind;
- [ ] identity;
- [ ] subject relationship;
- [ ] project scope; and
- [ ] primary evidence location.

Canonicalize merged constraints by:

- [ ] argument index;
- [ ] matcher family;
- [ ] property/key name; and
- [ ] canonical predicate payload.

Deduplicate identical constraints.

### Contradiction detection

Add `QueryCompileError::ContradictoryPredicate {
variable: VarId, detail: ContradictionKind }`. Reject contradictions at catalog
construction; do not add a never-match normalized node.

Use this contradiction classification:

```rust
pub(crate) enum ContradictionKind {
    EventKind,
    StrictIdentity,
    SubjectRelation,
    StaticExactValues,
    StaticExactAndPrefix,
    EvidenceProjection,
}
```

Detect at least:

- [ ] incompatible event kinds on the same event variable;
- [ ] incompatible strict identities on the same event variable;
- [ ] incompatible subject relationships;
- [ ] disjoint exact static-string requirements on one argument;
- [ ] `static_string` predicates with empty accepted sets;
- [ ] impossible exact/prefix combinations where the contradiction is provable;
  and
- [ ] incompatible evidence projections.

Do not apply two-valued Boolean simplifications when an unknown branch changes
certainty or completeness.

### Lifecycle normalization

Preserve semantically meaningful lifecycle sequence and evidence order.
Canonicalize order-independent source alternatives and value predicate sets.
The lifecycle comparator must include condition and completion contents, not
only their presence.

### Plan requirements

Compute requirements from normalized operators and relations, not broad
defaults. See Package 9 for runtime consumption requirements.

### Required tests

- [ ] normalization idempotency;
- [ ] alpha-equivalent variable IDs normalize equally;
- [ ] reversed argument-constraint order normalizes equally;
- [ ] reversed independent alternative order normalizes equally;
- [ ] compatible filters merge once;
- [ ] incompatible filters produce a structured contradiction;
- [ ] duplicate filters do not duplicate work or evidence;
- [ ] lifecycle ordering is preserved where meaningful;
- [ ] distinct lifecycle conditions never compare as the same ordering key;
- [ ] unknown-sensitive forms are not over-simplified; and
- [ ] normalized validation rejects any remaining nested, sparse, or untyped
  invariant violation.

### Exit criteria

- [ ] Phase 5 Tasks 7 and 8 are implemented.
- [ ] Structural equality is independent of order wherever semantics are
  order-independent.
- [ ] Physical planning never needs to guess whether filters are compatible.

---

## Package 5: Make physical planning consume proved normalized invariants

### Objective

Remove unsound assumptions from `plan_all_expression` and make every physical
root correspond to one validated logical meaning.

### Required changes

- [ ] Replace the current "use first event and append every constraint" behavior.
- [ ] Plan the normalized same-event form directly into:
  - [ ] `IndexedScan` when no value filter is required; or
  - [ ] one `ConstrainedScan` with canonical grouped filters.
- [ ] Plan `Any` into deterministic independent roots whose emissions are valid in
  every branch.
- [ ] Do not add a generic Cartesian join.
- [ ] Keep returned and instance index access specialized when it is the narrowest
  correct operator.
- [ ] Validate every physical root after planning.
- [ ] Make physical validation reject:
  - [ ] empty identities;
  - [ ] non-call constrained scans;
  - [ ] ungrouped or noncanonical constraints;
  - [ ] unsupported returned/instance dimensions;
  - [ ] invalid join keys;
  - [ ] unavailable primary evidence; and
  - [ ] malformed lifecycle roots.

### Stable plan summary

Extend the plan summary to report actual executable requirements:

```text
roots=N
indexed_scans=N
constrained_scans=N
returned_subjects=N
instance_subjects=N
lifecycle_plans=N
local_flow=yes|no
cross_call_flow=yes|no
project_overlay=<none|module_exports|module_namespaces|...>
```

Do not expose physical storage publicly.

### Focused logical equivalence oracle

Implement the unchecked Phase 6 equivalence requirement in
`glass-lint-core/src/api/compiler/reference.rs`, compiled only under
`#[cfg(test)]`. Use a small synthetic relation store rather than a second
production matcher.

Define these test-only records:

```rust
struct ReferenceRow {
    event: u32,
    event_kind: EventSpec,
    identity: IdentitySpec,
    arguments: BTreeMap<ArgumentIndex, ReferenceValue>,
    object: Option<u32>,
    path: u32,
    completeness: ReferenceCompleteness,
}

enum ReferenceCompleteness {
    Complete,
    Unknown,
}
```

Implement two evaluators over the same immutable row set:

- [ ] `evaluate_logical(&NormalizedQuery, &[ReferenceRow])`; and
- [ ] `evaluate_physical(&PhysicalPlan, &[ReferenceRow])`.

The physical evaluator dispatches only on physical root fields; it must not
call the logical evaluator. Compare sorted `ReferenceWitness` values containing
the primary event, support events, path key, and certainty.

The oracle must:

- [ ] evaluate normalized `Event`, `Any`, same-event `All`, and supported subject
  relations over small deterministic rows;
- [ ] retain a path/correlation key and completeness state;
- [ ] produce primary/support witness keys; and
- [ ] compare those witnesses with the selected physical-plan result.

Cover:

- [ ] empty and non-empty relations;
- [ ] duplicate rows;
- [ ] alternative order;
- [ ] filter order;
- [ ] possible versus definite results;
- [ ] unknown alternatives;
- [ ] incompatible correlation keys; and
- [ ] evidence ordering.

Keep the oracle behind `#[cfg(test)]`; production must still have one executor.

### Exit criteria

- [ ] No planner function merges predicates whose compatibility was not proved.
- [ ] Every logical leaf and normalized composition selects a documented physical
  operator.
- [ ] The test-only oracle agrees with physical execution on the supported small
  domain.
- [ ] The Phase 6 equivalence checkbox can be checked with a named test module.

---

## Package 6: Canonicalize and share argument/value evaluation

### Objective

Finish Phase 8 by compiling constraints into bounded per-argument work and
resolving each selected argument once.

### Physical constraint representation

Replace a flat `Box<[ArgumentConstraint]>` with these exact compiler-owned
types:

```rust
pub(crate) struct CompiledArgumentConstraints {
    groups: Box<[ArgumentConstraintGroup]>,
}

pub(crate) struct ArgumentConstraintGroup {
    index: ArgumentIndex,
    predicates: Box<[ArgumentMatcher]>,
}
```

Keep construction private. Expose iteration only through
`CompiledArgumentConstraints::groups()`. `PhysicalRoot::ConstrainedScan` stores
one `CompiledArgumentConstraints`.

The planner should:

- [ ] sort constraints by argument index and predicate order;
- [ ] group predicates for the same argument;
- [ ] deduplicate identical predicates;
- [ ] reject provable contradictions;
- [ ] validate all group and predicate bounds; and
- [ ] store groups in deterministic index order.

### Evaluation

For each candidate call:

- [ ] select effective arguments once;
- [ ] reject a missing required argument;
- [ ] construct the overlay-aware `ArgumentView` once per referenced index;
- [ ] resolve its static value/object/rooted path once;
- [ ] apply all predicates in the group to that prepared value; and
- [ ] charge deterministic operations per candidate, group, and predicate.

Do not repeatedly call `argument_with_overlay` for separate predicates on the
same argument.

### Required tests

- [ ] two predicates on one argument prepare the argument once;
- [ ] constraints on several argument positions prepare each referenced position
  once;
- [ ] missing and sparse arguments fail closed;
- [ ] static aliases and reassignment preserve current behavior;
- [ ] object keys and property values remain strict;
- [ ] dynamic values do not satisfy selective predicates;
- [ ] constraint order produces identical normalized and physical plans;
- [ ] excessive argument index, group count, predicate count, and alternative
  count fail with structured errors; and
- [ ] operation counts scale with candidates and unique argument groups, not raw
  duplicate constraints.

### Exit criteria

- [ ] Static/value semantics remain owned by `analysis/value` and
  `analysis/flow/matcher.rs`.
- [ ] One bounded projection is performed per candidate call.
- [ ] Each referenced argument is prepared at most once per candidate.
- [ ] Equivalent constraint order produces one physical plan.

---

## Package 7: Make returned-object and instance correlation explicit

### Objective

Finish Phase 9's logical model while retaining the existing efficient indexes.

### Required logical representation

Represent the relationship explicitly enough for the compiler to prove:

```text
producer or constructor event
  -> correlated returned or constructed object
  -> primary member event
```

Keep this compact authoring API:

```rust
QueryDecl::member_call_returned(...)
QueryDecl::member_call_instance(...)
```

Both constructors return `Result<QueryDecl, QueryBuildError>`. Lower them into
the exact `ReturnedObject`/`ConstructedObject` plus `MemberSubject` predicates
defined in Package 3. The lowered form contains:

- [ ] producer/constructor identity;
- [ ] producer/constructor event role;
- [ ] object correlation variable/key;
- [ ] primary member event;
- [ ] member path;
- [ ] local/project scope; and
- [ ] primary/support evidence projection.

Do not encode the entire relation as an unexplained `SubjectSpec` flag plus an
identity field.

Delete `SubjectSpec` after all direct, returned, and instance constructors have
been lowered to predicates. A direct event is represented by the absence of a
`MemberSubject` relation; returned and instance events use the explicit object
relations above.

### Physical planning

- [ ] Use returned-member indexes when they already carry the required correlation.
- [ ] Use instance-member indexes for strict constructed-instance identity.
- [ ] Reject a returned/instance shape the existing correlated indexes cannot
  express. Do not add a general join as part of this remediation.
- [ ] Keep the member occurrence as primary evidence.
- [ ] Retain producer/constructor evidence only when the evidence contract asks for
  it.
- [ ] Preserve exact module and package-boundary semantics.

### Required tests

Retain all Phase 9 cases and add compiler-shape tests proving the correlation:

- [ ] direct and aliased returned object;
- [ ] reassigned returned alias;
- [ ] disconnected same-name object;
- [ ] direct and aliased constructed instance;
- [ ] wrong constructor module;
- [ ] static method lookalike;
- [ ] supported/unsupported subclass behavior;
- [ ] supported/unsupported chained constructor behavior;
- [ ] producer and member on incompatible branches;
- [ ] ambiguous project identity; and
- [ ] deterministic primary/support evidence order.

### Exit criteria

- [ ] Subject correlation is explicit in normalized logical form.
- [ ] Physical returned/instance scans are selected from that logical relation.
- [ ] No identity is duplicated merely to satisfy an old record layout.

---

## Package 8: Unify lifecycle authoring, compilation, and execution

### Objective

Finish Phases 10 and 12 by making lifecycle a normal `QueryDecl` and deleting
the second authoring/storage route.

### Authoring API

Move and rename the existing flow declarations into `api/rule/query`:

| Old type | Final type |
|---|---|
| `ObjectSourceMatcher` | `LifecycleSource` |
| `ObjectEventMatcher` | `LifecycleEvent` |
| `FlowCondition` | `LifecycleCondition` |
| `FlowCompletion` | `LifecycleCompletion` |
| `FlowSinkMatcher` | `LifecycleSink` |
| `ObjectFlowMatcher` | deleted; `LifecycleQuery` owns the declaration |

Use this authoring API:

```rust
QueryDecl::lifecycle(
    LifecycleQuery::builder("remote script")
        .source(LifecycleSource::returned_by("document.createElement")?)
        .condition(LifecycleCondition::any_of([...])?)
        .completion(LifecycleCompletion::any_sink([...])?)
        .build(),
)
```

Use these exact constructor names:

- [ ] `LifecycleSource::returned_by`;
- [ ] `LifecycleSource::with_arg`;
- [ ] `LifecycleEvent::property_write`;
- [ ] `LifecycleEvent::member_call`;
- [ ] `LifecycleEvent::with_arg`;
- [ ] `LifecycleCondition::event`;
- [ ] `LifecycleCondition::any_of`;
- [ ] `LifecycleCondition::all_of`;
- [ ] `LifecycleCompletion::configuration`;
- [ ] `LifecycleCompletion::any_sink`;
- [ ] `LifecycleSink::argument_of`;
- [ ] `LifecycleSink::any_argument_of`;
- [ ] `LifecycleQuery::builder`;
- [ ] `LifecycleQueryBuilder::source`;
- [ ] `LifecycleQueryBuilder::condition`;
- [ ] `LifecycleQueryBuilder::completion`;
- [ ] `LifecycleQueryBuilder::build`; and
- [ ] `QueryDecl::lifecycle`.

Every text/index/collection constructor returns `Result`. Builder mutation
methods accept the validated final types. `LifecycleQueryBuilder::build`
returns `Result<LifecycleQuery, QueryBuildError>`.
`QueryDecl::lifecycle` accepts that result and returns
`Result<QueryDecl, QueryBuildError>`, allowing it to pass directly to the
canonical fallible `RuleBuilder::query` route.

`LifecycleQuery` always has at least one source and exactly one completion.
The condition is optional only for sink completion. Configuration completion
requires a condition. `any_of`, `all_of`, and `any_sink` reject empty
collections.

Reuse provider-neutral `ValueMatcher` and `ArgumentMatcher`; do not duplicate
value semantics.

### Migration

Migrate, in order:

- [ ] 1. core lifecycle unit tests;
- [ ] 2. core declarative matching integration tests;
- [ ] 3. project test support;
- [ ] 4. `glass-lint-js` file-dialog rule;
- [ ] 5. `glass-lint-js` script-injection rule;
- [ ] 6. `glass-lint-js` remote-resource rules; and
- [ ] 7. any remaining harness helper using `ObjectFlowMatcher`.

Provider code must finish with `.query(QueryDecl::lifecycle(...))`.

### Delete the old path

Delete in the same migration:

- [ ] `Rule.flows`;
- [ ] `Rule::flow_matchers()`;
- [ ] `RuleBuilder.flows`;
- [ ] `RuleBuilder::object_flow()`;
- [ ] `CompiledMatcherPlan.flows`;
- [ ] `CompiledMatcherPlan::flows()`;
- [ ] `compile_decls_and_flows`;
- [ ] `compile_single_flow`;
- [ ] `QueryDecl::from_flow_matcher`;
- [ ] compatibility extraction of flows from physical roots;
- [ ] `ObjectFlowMatcher` and its builder;
- [ ] comments or docs describing a parallel flow matcher path.

Rename and move existing lifecycle types rather than retaining aliases.

### Execution

- [ ] Local lifecycle projection must enumerate `PhysicalRoot::Lifecycle`.
- [ ] Cross-call and cross-file flow must enumerate the same roots.
- [ ] Assign stable flow IDs from deterministic physical root order.
- [ ] Never clone lifecycle roots into a second storage collection.
- [ ] Preserve local/cross-file flow fixed-point and budget behavior.

### Multiple sources

The current lowering assigns variable zero to every source. The final model
uses one branch-local source event variable per source and alpha-aligns each
branch's produced object to one lifecycle object output slot. Sources have
`Any` semantics: any independently complete source can start the lifecycle.
Duplicate normalized sources are removed. Add explicit tests for:

- [ ] multiple independently valid sources;
- [ ] duplicate sources;
- [ ] one unknown source plus one complete source;
- [ ] sources on incompatible paths; and
- [ ] deterministic source evidence ordering.

### Required tests

Run every Phase 10 required case through the new public query authoring route.
At minimum cover:

- [ ] any and all requirements;
- [ ] configuration completion;
- [ ] exact and any-argument sinks;
- [ ] multiple sources and sinks;
- [ ] aliases and reassignment;
- [ ] escaped or unsupported objects;
- [ ] dynamic source and requirement values;
- [ ] disconnected source and sink;
- [ ] incompatible source/requirement/sink paths;
- [ ] cross-call source/requirement/sink combinations;
- [ ] cross-file flow;
- [ ] budget exhaustion; and
- [ ] primary evidence in the sink file.

### Exit criteria

- [ ] `RuleBuilder::query()` is the only way to add matching semantics.
- [ ] `Rule` stores only `Vec<QueryDecl>`.
- [ ] `CompiledMatcherPlan` stores only one physical plan.
- [ ] Lifecycle exists only as a logical operator and a physical lifecycle root.
- [ ] Local and project execution use the same lifecycle roots.
- [ ] No provider rule mentions a physical executor family or old flow builder.

---

## Package 9: Make plan requirements executable

### Objective

Finish Phase 11 by ensuring runtime preparation is selected from compiled
requirements rather than performed broadly or rediscovered from roots.

### Requirement model

Replace the boolean bag with these compiler-owned types:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PlanRequirements {
    occurrence_indexes: BTreeSet<OccurrenceIndexRequirement>,
    fact_fields: BTreeSet<FactFieldRequirement>,
    value_resolution: BTreeSet<ValueResolutionRequirement>,
    flow: FlowRequirements,
    project: BTreeSet<ProjectRequirement>,
}

pub(crate) enum OccurrenceIndexRequirement {
    Calls,
    Constructions,
    Members,
    Literals,
    ReturnedMembers,
    InstanceMembers,
}

pub(crate) enum FactFieldRequirement {
    CallArguments,
    ObjectProperties,
    RootedValues,
}

pub(crate) enum ValueResolutionRequirement {
    LocalStaticValues,
    ModuleIdentityValues,
    CallResultIdentities,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct FlowRequirements {
    local: bool,
    cross_call: bool,
    cross_file: bool,
}

pub(crate) enum ProjectRequirement {
    ExactModuleExports,
    PackageModuleExports,
    ExactModuleNamespaces,
    PackageModuleNamespaces,
    CallResultIdentities,
}
```

Derive `Ord` for every set element. Keep storage private and expose semantic
queries such as `needs_module_identities()`, `needs_call_result_identities()`,
and `flow()`.

Do not retain `needs_evidence_trace`. Every finding already requires its
primary trace, so it is not conditional preparation. Correlated support traces
are encoded by the physical root that emits them.

Every requirement field must have:

- [ ] a compiler producer;
- [ ] a runtime consumer;
- [ ] a plan-summary representation; and
- [ ] a focused test proving work is skipped when absent.

Remove fields that do not represent a meaningful conditional preparation.

### Project projection

Refactor project preparation so that:

- [ ] module identities are built only when selected plans require them;
- [ ] call-result identities are built only for constrained/project relations that
  use them;
- [ ] module occurrence overlays are built only for the exact requested overlay
  families;
- [ ] local lifecycle projection runs only when lifecycle roots are selected;
- [ ] cross-call/cross-file collection returns immediately before graph/session
  preparation when no selected root requires it; and
- [ ] project projection never inspects `QueryDecl`, `QueryExpr`, or provider rule
  types.

The physical root remains the source of executable state. Requirements select
preparation; they must not become a second copy of the query.

### Operation counts

Charge deterministically for:

- [ ] identity map construction;
- [ ] result-identity construction;
- [ ] overlay insertion and lookup;
- [ ] local lifecycle projection;
- [ ] call graph construction;
- [ ] worklist propagation; and
- [ ] evidence normalization.

Queries that need no project behavior must show zero project-query preparation
operations attributable to overlays or flow.

### Required tests

- [ ] global-only query skips module identities, result identities, and overlays;
- [ ] unconstrained module query builds only its identity overlay;
- [ ] constrained local query avoids project overlay work;
- [ ] call-result identity query prepares result identities;
- [ ] project-independent query skips cross-flow graph construction;
- [ ] direct external import and re-export chains preserve matching;
- [ ] namespace and CommonJS/ESM interop preserve matching;
- [ ] ambiguity and missing resolution remain unknown;
- [ ] package boundaries remain exact;
- [ ] cross-file finding stays in the primary/sink file;
- [ ] independent witness plus unknown project alternative remains possible; and
- [ ] plan summaries list exact local/project/cross-call requirements.

### Exit criteria

- [ ] No project preparation is performed merely because analysis is in project
  mode.
- [ ] Every selected preparation is justified by compiled requirements.
- [ ] Cross-file execution uses the same physical roots as local execution.

---

## Package 10: Update public API examples and documentation

### Objective

Complete Phase 12's unchecked documentation task and remove obsolete query
terminology.

### Required documentation changes

Update:

- [ ] [`glass-lint-core/README.md`](glass-lint-core/README.md);
- [ ] root [`README.md`](README.md) if the public Rust example changes;
- [ ] affected provider READMEs;
- [ ] [`glass-lint-core/ARCHITECTURE.md`](glass-lint-core/ARCHITECTURE.md);
- [ ] public Rustdoc in `api/rule`, `api/rule/query`, and `api/compiler`;
- [ ] [`CONTRIBUTING.md`](CONTRIBUTING.md) authoring example if necessary; and
- [ ] `q-plan.md` status and checkboxes only after implementation evidence exists.

The core README must use the actual API:

- [ ] `description`, not `label`;
- [ ] validated `Category`;
- [ ] `QueryDecl`, not `CallMatcher`;
- [ ] `query`, not `matcher`;
- [ ] current report accessors; and
- [ ] the final fallible query-construction pattern.

Document:

- [ ] a compact ordinary rule;
- [ ] a constrained rule;
- [ ] alternatives;
- [ ] same-event conjunction;
- [ ] returned/instance rule;
- [ ] lifecycle rule;
- [ ] structured catalog errors; and
- [ ] the distinction between strict and heuristic identity.

### Compile the examples

Move canonical examples into compilable Rust examples or doctests. Have README
snippets mirror those sources rather than being the only copy.

Add this exact command to `make ci`:

```sh
cargo check -p glass-lint-core --examples
```

Do not leave stale names in comments or docs. Verify with:

```sh
rg 'CallMatcher|MatcherDecl|MatcherDeclBuilder|QueryClause|QueryPlan|\.matcher\(|\.object_flow\(' \
  --glob '*.rs' --glob '*.md'
```

Expected matches should be restricted to historical explanation in
`q-plan.md` and this remediation plan, if retained.

### Exit criteria

- [ ] Every public example compiles.
- [ ] Public docs show the one final authoring path.
- [ ] Phase 12 Task 9 can be checked.

---

## Package 11: Reconcile `q-plan.md` with verified reality

### Objective

Make the original plan a truthful completion record after the fixes land.

### Required changes

- [ ] Update Phase 0 inventory items with links to the capability matrix,
  baselines, and tests.
- [ ] Update Phase 2 terminology so it describes final `QueryDecl` ownership rather
  than historical `MatcherDecl` state.
- [ ] Update Phase 3 with the final binding/reference and variable-type model.
- [ ] Update Phase 4 with the actual structured errors produced by validation.
- [ ] Check Phase 5 filter merging and contradiction detection.
- [ ] Check Phase 6 equivalence testing and name the oracle tests.
- [ ] Update Phase 8 with grouped per-argument evaluation.
- [ ] Update Phase 9 with the explicit subject correlation representation.
- [ ] Update Phase 10 to state that no separate flow declaration/storage path
  exists.
- [ ] Update Phase 11 with the exact executable requirement model.
- [ ] Update Phase 12 only after public examples compile and lifecycle providers use
  `RuleBuilder::query()`.
- [ ] Remove obsolete claims such as exact test counts that are no longer stable.
  Prefer named commands and suites.

Do not mark a task complete solely because `make ci` passes. Each checkbox must
have a named implementation or test artifact that proves the task.

### Exit criteria

- [ ] There are no unchecked items in Phases 0 through 12.
- [ ] Every checked claim matches current code.
- [ ] The Phase 12 completion statement no longer contradicts the public API.

---

## Cross-package test matrix

The implementation is not complete until this matrix is covered.

| Concern | Unit | Core integration | Project | Provider |
|---|---:|---:|---:|---:|
| Constructor validation | yes | public catalog | n/a | full catalog |
| Variable typing | yes | composed queries | typed project scope | representative |
| `Any` | normalize/validate | findings/certainty | unknown overlay | representative |
| Same-event `All` | normalize/plan | positive/negative | if overlay applies | representative |
| Contradictions | validate | catalog error | n/a | catalog compile |
| Argument grouping | planner/evaluator | static/dynamic | result identities | constrained rules |
| Returned relation | planner | alias/lookalike | ambiguity | representative |
| Instance relation | planner | alias/lookalike | wrong module | representative |
| Lifecycle | validate/plan | path correlation | cross-file | all lifecycle rules |
| Requirements | normalize/plan | skipped work | overlay/flow work | full fixtures |
| Evidence | normalize | exact order/location | sink file | fixtures |
| Boundedness | constructor/compiler | exhausted behavior | fixed-point limits | fixtures |

## Narrow iteration commands

Use narrow tests while implementing:

```sh
cargo test -p glass-lint-core --test query_composition
cargo test -p glass-lint-core api::compiler::validate
cargo test -p glass-lint-core api::compiler::normalize
cargo test -p glass-lint-core api::compiler::physical
cargo test -p glass-lint-core analysis::matching::arguments
cargo test -p glass-lint-core --test declarative_matching
cargo test -p glass-lint-core --test semantic_matching
cargo test -p glass-lint-core --test scope_precision
cargo test -p glass-lint-project
```

Verify migrated lifecycle provider rules while iterating:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  verify glass-lint-js/src/rules/browser/file_dialog

cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  verify glass-lint-js/src/rules/browser/script_injection

cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  verify glass-lint-js/src/rules/browser/remote_resource
```

## Final completion gate

Run all of the following from a clean worktree:

```sh
cargo check -p glass-lint-core --examples
make ci
git status --short
```

Completion requires:

- [ ] all focused regression tests passing;
- [ ] the physical/logical equivalence oracle passing;
- [ ] all e2e, project, JavaScript, and Obsidian fixtures passing;
- [ ] public examples compiling;
- [ ] no old query or lifecycle authoring path outside historical documentation;
- [ ] no unchecked item in `q-plan.md` through Phase 12;
- [ ] deterministic baseline reports and operation counts unchanged or explicitly
  reviewed; and
- [ ] no unrelated or generated worktree changes.

## Recommended commit sequence

Keep each commit buildable and avoid compatibility layers:

- [ ] 1. Add failing public composition and invalid-input tests.
- [ ] 2. Add the capability matrix and deterministic baselines.
- [ ] 3. Make declaration construction fallible and fields private.
- [ ] 4. Implement scope-aware bindings, typed validation, and evidence projection.
- [ ] 5. Complete normalization, filter merging, and contradiction detection.
- [ ] 6. Repair physical composition and add the test-only equivalence oracle.
- [ ] 7. Group and cache argument/value constraints.
- [ ] 8. Make returned and instance correlation explicit.
- [ ] 9. Migrate lifecycle rules to `QueryDecl` and delete the parallel flow path.
- [ ] 10. Make project requirements drive preparation and operation charging.
- [ ] 11. Update compilable public examples and architecture documentation.
- [ ] 12. Reconcile `q-plan.md` and run the final completion gate.
