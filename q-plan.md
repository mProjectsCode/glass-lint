# Query and matcher architecture plan

## Status

Phases 0–12 are complete for the implemented query architecture. The current
public path is typed `EventQuery`/`QueryDecl` authoring, compiler validation and
normalization, explicit physical planning, and execution through the existing
indexed, constrained, lifecycle, and project-projection owners. A textual query
language and the capabilities in Phases 13–15 remain future work.

Current evidence is maintained in:

- [query capability matrix](glass-lint-core/QUERY_CAPABILITIES.md), including
  normalized relation and runtime owner for each supported capability;
- [query migration baseline](reports/QUERY_MIGRATION_BASELINE.md), including
  exact operation assertions and reviewed fixture commands;
- [query composition and baseline tests](glass-lint-core/tests/query_composition.rs)
  and [query baseline tests](glass-lint-core/tests/query_baseline.rs); and
- [the final completion checklist](q-fix.md).

The migration is one forward path: provider rules construct validated logical
queries, the compiler lowers them once into immutable physical plans, and
runtime consumers execute those plans through named subsystem owners. Public
compiler storage and artifact-local IDs are not exposed.

## Completion record for Phases 0–12

### Phase 0 — baseline and capability inventory

- [x] All public query constructors and supported identity/event/value/subject
      capabilities are recorded in the capability matrix.
- [x] Lifecycle sources, conditions, completions, sinks, project requirements,
      and cross-file behavior are covered by the matrix and provider fixtures.
- [x] Execution ownership is explicit: indexed queries in
      `analysis/matching/query`, grouped arguments in
      `analysis/matching/arguments`, lifecycle in `analysis/flow`, and project
      preparation in `analysis/project/projection`.
- [x] Baseline operation counts and deterministic fixture-review commands are
      recorded without brittle suite totals.
- [x] Shadowing, reassignment, aliases, lookalikes, dynamic values, ambiguous
      identities, incompatible paths, and minified shapes have focused coverage.

### Phase 1 — single compiled plan

- [x] `CompiledMatcherPlan` is the only compiled plan container; the historical
      `QueryPlan` wrapper and its accessors are gone.
- [x] Runtime routing consumes physical roots and plan-wide requirements.

### Phase 2 — declaration/compiler ownership

- [x] Public declarations own provider-neutral semantic types; compiler IR is
      private to `api/compiler`.
- [x] Lowering is directional and structured errors replace generic authoring
      errors.
- [x] Object-flow declarations are first-class lifecycle queries rather than
      synthetic ordinary clauses.

### Phase 3 — typed logical algebra

- [x] Variables, event selections, requirements, alternatives, conjunctions,
      lifecycle expressions, evidence emission, equality, hashing, and stable
      diagnostics are implemented.
- [x] Empty collections, invalid bindings, unsupported correlations, and
      evidence projection errors are rejected deterministically.

### Phase 4 — validation and boundedness

- [x] Declaration well-formedness, variable/type checks, operator and relation
      compatibility, scope, evidence, boundedness, lifecycle, and normalized
      invariant checks are separate compiler passes.
- [x] Public limits are checked before large allocations; malformed input is
      fallible and non-panicking.

### Phase 5 — normalization

- [x] Alternatives and conjunctions flatten, deduplicate, order
      deterministically, and receive dense variable slots.
- [x] Same-event argument predicates are retained in one grouped constrained
      plan; contradiction and unsupported-relationship cases fail closed rather
      than being simplified unsafely.
- [x] Requirements are computed from executable normalized roots and validated
      exactly against the physical plan.

### Phase 6 — physical plans

- [x] Normalized roots select indexed, constrained, returned-subject,
      instance-subject, lifecycle, and project-aware operators.
- [x] Physical validation, deterministic summaries, reference evaluation, and
      logical/physical equivalence coverage are present.

### Phase 7 — ordinary indexed matchers

- [x] Ordinary identity/event matchers use the shared logical and physical
      pipeline; runtime consumers no longer inspect authored declarations.

### Phase 8 — value and argument constraints

- [x] Static, alternative, prefix, object-key, object-property, missing,
      dynamic, sparse, and grouped argument semantics use one bounded prepared
      projection path with focused tests.

### Phase 9 — returned and instance relationships

- [x] Returned and constructed subjects carry explicit producer/constructor
      relations and object slots; member evidence is distinct from support
      evidence, with incompatible-path negatives.

### Phase 10 — lifecycle

- [x] Lifecycle declarations compile through the same validate → normalize →
      physical-plan route into the bounded local/cross-call flow engine.
- [x] Source, requirement, completion, sink, alias, reassignment, escape,
      disconnected-path, ordering, and exhaustion behavior is covered.

### Phase 11 — project and cross-file requirements

- [x] Executable requirements distinguish local, module, project, and flow
      preparation; selected plans alone request overlays and fixed-point work.
- [x] Exact module identity, ambiguity, missing resolution, re-exports,
      interop, package boundaries, and primary-file evidence are covered.

### Phase 12 — compositional authoring API

- [x] Typed `EventQuery`/`QueryDecl` constructors and `RuleBuilder::query()` are
      the only authoring path; provider crates and examples use them.
- [x] Historical matcher builders, compatibility bridges, compiler storage
      leaks, and obsolete terminology are removed.
- [x] Public examples compile through the core example check in `make ci`, and
      current documentation describes one authoring path.

Phases 0–12 are the completed migration boundary. Future phases may add new
relations, optimization, diagnostics, or a textual frontend only by reusing
this query compiler and executor.

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
2. keep `CompiledMatcherPlan` as the single compiled-plan container;
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
