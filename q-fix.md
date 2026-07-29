# Outstanding query architecture remediation

## Purpose

This document contains only the work that remains after re-auditing the
implementation against Phases 0 through 12 of [`q-plan.md`](q-plan.md).
Completed migration history has been removed.

The production catalog path now supports ordinary `Any`, same-event `All`,
canonical grouped argument constraints, lifecycle roots, project preparation,
and the test-only logical/physical reference evaluator. The focused suites pass:

```sh
cargo test -p glass-lint-core --test query_composition --test query_baseline
cargo test -p glass-lint-core api::compiler::reference
cargo test -p glass-lint-core analysis::matching::arguments
```

Those passing tests do not establish the remaining claims below. Several
tests and documents currently assert weaker behavior than the original plan
requires.

This remains a forward migration. Breaking changes are allowed. Do not add
compatibility aliases, parallel executors, or deprecated authoring routes.

## Contracts to preserve

1. Parse and construct matcher-independent facts once per file.
2. Keep strict witnesses path-local, provenance-aware, and deterministic.
3. Never combine facts from incompatible control-flow alternatives.
4. Unknown, dynamic, ambiguous, unsupported, or exhausted analysis cannot
   establish a witness or a definite result.
5. Keep work, state, recursion, alternatives, evidence, and diagnostics
   explicitly bounded.
6. Keep provider policy outside `glass-lint-core`.
7. Compile every authored query through one validation, normalization, and
   physical-planning pipeline.
8. Give runtime code physical plans rather than authored declarations.
9. Return structured errors for expected invalid author input.

## 1. Make public declarations invariant-preserving

The ordinary `EventQuery` constructors are fallible, but the value and
lifecycle APIs still admit invalid or unbounded declarations.

### Fallible text, index, and collection construction

- [x] Make every public lifecycle constructor that accepts text, a raw
  argument index, or a collection return `Result<_, QueryBuildError>`,
  including sources, events, conditions, completions, and sinks.
- [x] Remove the `usize as u8` casts from lifecycle sources and sinks. Index
  256 now produces `InvalidArgumentIndex` instead of truncating to zero.
- [x] Make static-string alternatives validated non-empty bounded domain
  collections. Construction sorts, deduplicates, validates text, and rejects
  more than `MAX_STATIC_ALTERNATIVES`.
- [x] Make object-key sets and rooted-expression sets validated non-empty
  bounded domain collections. Construction sorts, deduplicates, validates
  paths/text, and rejects more than `MAX_STATIC_ALTERNATIVES`.
- [x] Enforce `MAX_ARGUMENT_GROUPS` and `MAX_PREDICATES_PER_ARGUMENT` while
  authoring, not only during physical-plan validation.
- [x] Enforce `MAX_QUERY_ROOTS_PER_RULE` and `MAX_EXPR_CHILDREN` while
  authoring. Limit-plus-one tests cover both boundaries.
- [x] Enforce `MAX_LIFECYCLE_EVENTS` and `MAX_LIFECYCLE_SINKS` at lifecycle
  collection construction, rather than relying on compiler validation.
- [x] Add an explicit expression-depth/recursion bound and validate it before
  recursive compiler passes.
- [x] Make `LifecycleQuery` impossible to construct without at least one
  source and exactly one completion. The raw `new` path is crate-private and
  validates completion, while the public builder enforces the same invariant.
- [x] Give duplicate lifecycle stages and other lifecycle failures precise
  `QueryBuildError` variants rather than reporting them as empty collections.

### Close raw mutation routes

- [x] Remove or restrict public `QueryDecl::{with_expression,with_primary_var,
  with_evidence}`, `EventQuery::with_var`, and raw expression constructors that
  let external callers bypass the Phase 12 authoring grammar. Keep any required
  malformed-IR builders as `pub(crate)` test helpers.
- [x] Expose only the supported Phase 12 composition surface:
  compact `EventQuery`/`QueryDecl` constructors, `QueryDecl::any`,
  same-event `QueryDecl::all`, and `QueryDecl::lifecycle`.
- [x] Validate `Any` branch emissions before discarding branch declarations.
  Compatible primary location and evidence kind must be proved on every branch;
  incompatible evidence must produce the structured
  `EvidenceProjection` contradiction rather than silently taking the first
  branch's emission.
- [x] Validate evidence symbols at construction and remove infallible mutation
  that can install an empty symbol.

### Bounded normalization input

- [x] Replace the dense `Vec` indexed by the largest authored `VarId` in
  `alpha_renumber_slots`. A public `VarId::new(u32::MAX)` must not request a
  multi-gigabyte allocation; remap only the bounded variables actually present.
- [x] Make post-normalization validation actually verify dense typed slots,
  canonical collections, requirements, and all normalized invariants.

### Required tests

- [x] Add limit and limit-plus-one tests for every declared collection and
  index limit, including lifecycle arguments/events/sinks, expression children,
  and rule query roots.
- [x] Add table-driven malformed lifecycle/value/path cases that prove no
  panic, truncation, silent ignore, or delayed stringified physical-plan error.
- [x] Add a large/sparse `VarId` regression.
- [x] Add a maximum-depth regression.
- [x] Replace the current “predicate alternatives at limit succeeds” test with
  both at-limit success and limit-plus-one construction failure.

## 2. Finish logical validation and canonical normalization

The compiler has a real event/object type pass, but it still contains nominal
types and errors that no authored predicate can produce, and some canonical
forms are only descriptive.

### Type and relation model

- [x] Remove the dead `StaticValue`, `CallableIdentity`, `ModuleIdentity`,
  and `SymbolPath` `VarType` variants; authored predicates only produce event
  and object types.
- [x] Make relation-availability validation enforce an actual semantic scope.
  Identity/event combinations and returned/constructed producer scopes are
  now rejected when their corresponding semantic relation is unavailable.
- [x] Artifact-local fact/object/flow IDs are confined to the private analysis
  module; the public query algebra exposes only provider-neutral `VarId` slots
  and has no relation accepting artifact IDs. The inapplicable public-algebra
  claim was removed from `q-plan.md`.
- [x] Remove the dead `InvalidModulePattern` and
  `InvalidStaticValuePredicate` compiler errors. Expected malformed input now
  fails in declaration construction.

### Returned-object and instance correlation

- [x] Replace the current flag-like `NormalizedSubject::{Direct,Returned,
  Instance}` with the explicit normalized relation required by the plan. It
  retains the producer/constructor specification and correlated object slot;
  the primary member event and evidence contract remain on the normalized
  event/emission that owns the relation.
- [x] Stop duplicating the producer/constructor identity as the selected member
  event identity merely to fit the old record shape. Direct events retain an
  event identity; returned/instance relations retain their producer identity.
- [x] Select `ReturnedSubject` and `InstanceSubject` physical operators only
  after validating that the explicit normalized correlation is supported by
  the corresponding index.
- [x] Make physical validation reject unsupported returned/instance identity,
  event, scope, and correlation shapes rather than checking only for a non-empty
  identity and member event.
- [x] Extend the reference oracle so returned/instance support evidence is tied
  to an actual producer/constructor event and matching path key, not inferred
  from a single row's object number.

### Lifecycle and ordering canonicalization

- [x] Canonicalize and bound order-independent lifecycle alternatives,
  especially `AnyOf` conditions and `AnySink` completions.
- [x] Replace `format!("{:?}")` lifecycle ordering in normalization with a
  semantic deterministic ordering implementation.
- [x] Add reversed-order and duplicate lifecycle condition/sink tests proving
  equal declarations normalize to equal plans without changing meaningful
  `AllOf` order or evidence order.

### Composition scope

- [x] Reconcile nested `Any` inside `All` with the Phase 12 contract. General
  multi-event composition is not supported before Phase 13; the raw public
  construction route is sealed and the limitation is documented explicitly.
- [x] Retain focused incompatible-path negatives for every supported public
  correlation: lifecycle, returned objects, and constructed instances. Caller
  argument/callee-parameter and callee-return/caller-result relations are not
  supported by the Phase 12 algebra, so those inapplicable claims are absent
  from the reconciled `q-plan.md`.
- [x] Replace the current normalization “idempotency” test, which normalized the
  same authored input twice, with direct canonical normalized-invariant
  assertions.

## 3. Complete per-argument preparation

Constraints are grouped by argument index and `ArgumentView` is constructed
once per group, but static values, object entries, and rooted paths are still
looked up again by each predicate through `ArgumentMatcher::matches`.

- [x] Introduce one prepared argument view per referenced index containing the
  overlay-aware static value, object entries, and rooted path needed by that
  group's predicates.
- [x] Apply grouped predicates to that prepared view without repeating the
  argument value/object-entry/rooted-path resolution.
- [x] Charge deterministic candidate, group, preparation, value-resolution,
  and predicate operations; operation tests assert one preparation and one
  value resolution per referenced argument index.
- [x] Add mixed-predicate tests on one argument proving each semantic projection
  occurs once, plus multi-index and duplicate-predicate operation-count tests.

## 4. Make plan requirements execution-driving

`PlanRequirements` retains value-resolution, flow, and project preparation
sets. Occurrence indexes and fact fields are matcher-independent artifact
state and are intentionally not represented as rule-selected requirements.

- [x] For each retained requirement set, runtime consumers now drive value
  resolution, flow projection, or project preparation; matcher-independent
  occurrence-index and fact-field sets were removed.
- [x] Preserve the rule-independent local artifact/cache boundary. Occurrence
  indexes and fact fields remain built once per artifact, independently of
  selected rules.
- [x] Make project requirements select exact overlay families. Project
  requirement predicates distinguish module exports, namespaces, and result
  identities.
- [x] Cross-check physical roots and requirements in
  `validate_physical_plan`, including lifecycle flow levels, returned/instance
  operators, constrained value preparation, and exact project overlays.
- [x] Extend the stable plan summary to list every retained executable
  requirement, not only root counts and coarse booleans.
- [x] Add skip-work coverage through exact requirement/route validation and
  exact operation assertions for the representative local, constrained,
  lifecycle, and project cases in `query_baseline.rs`; zero-work fields are
  asserted explicitly where the selected plan does not require that facility.

## 5. Repair the Phase 0 inventory and baselines

The checked-in inventory and baseline report do not yet prove their stated
claims.

- [x] Update [`glass-lint-core/QUERY_CAPABILITIES.md`](glass-lint-core/QUERY_CAPABILITIES.md)
  to use the final logical relation and lifecycle type names.
- [x] Ensure every capability row names its authoring constructor, normalized
  relation, physical operator, runtime owner, project behavior, certainty
  behavior, provider users, and focused non-provider test.
- [x] Remove the ambiguous-project and cross-file-flow cases from
  [`reports/QUERY_MIGRATION_BASELINE.md`](reports/QUERY_MIGRATION_BASELINE.md)
  until executable baselines cover them; the report no longer claims coverage
  that `query_baseline.rs` does not provide.
- [x] Make baseline assertions exact. The baseline suite uses exact file,
  finding, completion, and operation-field assertions; broad `operation > 0`
  and optional-file assertions are absent.
- [x] Assert exact completion, finding order, evidence cardinality/order,
  certainty, physical route/summary, and stable operation fields across the
  representative baseline cases and compiler physical/reference tests.
- [x] Record the commands and reviewed results for e2e, project, JavaScript, and
  Obsidian fixtures without embedding brittle obsolete test counts.

## 6. Finish the intended module and terminology cleanup

The final package layout in the remediation design has not been completed:
`api/rule/query/mod.rs`, `normalize.rs`, and `validate.rs` remain oversized,
and compiler errors still use historical clause terminology.

- [x] Split `api/rule/query` into cohesive `error`, `limits`, `value`, `event`,
  `expression`, and `lifecycle` modules, leaving `mod.rs` as public
  declarations/re-exports and construction orchestration.
- [x] Split compiler query code into cohesive `error`, `validate`, `normalize`,
  `physical`, and test-only `reference` modules with compilation orchestration
  in `mod.rs`.
- [x] Rename `InvalidQueryClause` to `PhysicalPlanValidationError`; current
  compiler diagnostics now use physical-plan validation terminology.
- [x] Keep indexed execution in `analysis/matching/query`, grouped argument
  execution in `analysis/matching/arguments`, lifecycle execution in
  `analysis/flow`, and project preparation in `analysis/project/projection`.
- [x] Complete the module migration and remove emptied historical paths; the
  remaining internal `rule` module contains rule-record/lowering ownership,
  not a compatibility wrapper.

## 7. Update public examples and documentation

The core README still uses removed `CallMatcher`, `.label`, `.category(&str)`,
and `.matcher` APIs. There are no compiled core examples, and `make ci` does
not check them.

- [x] Add a compiled example under `glass-lint-core/examples` covering:
  - a compact ordinary rule;
  - a constrained rule;
  - alternatives;
  - same-event conjunction;
  - returned-object and instance rules;
  - a lifecycle rule; and
  - structured construction/catalog errors.
- [x] Make [`glass-lint-core/README.md`](glass-lint-core/README.md) mirror the
  compiled example using `description`, validated `Category`, `QueryDecl`,
  `query`, and fallible construction.
- [x] Update the root README, affected provider READMEs, core architecture,
  public Rustdoc, contributing examples, and [`test.md`](test.md) where they
  described the removed API or old lifecycle path; current documentation now
  uses query declarations and lifecycle terminology.
- [x] Document strict versus heuristic identity and the supported/unsupported
  composition boundary.
- [x] Add this exact command to `make ci`:

  ```sh
  cargo check -p glass-lint-core --examples
  ```

- [x] Restrict current-code matches for removed authoring terms to intentional
  historical documents. Current Rust/docs matches are clean after renaming the
  remaining public `ArgumentConstraint::matcher()` accessor to `predicate()`:

  ```sh
  rg 'CallMatcher|MatcherDecl|MatcherDeclBuilder|QueryClause|QueryPlan|\.matcher\(|\.object_flow\(' \
    --glob '*.rs' --glob '*.md'
  ```

## 8. Reconcile `q-plan.md`

After Packages 1 through 7 are complete, make the original plan a truthful
completion record.

- [x] Replace the historical Phase 0 inventory with links to the corrected
  capability matrix, exact baselines, execution owners, and flow-join tests.
- [x] Rewrite Phase 2 around final `QueryDecl` ownership; remove statements that
  `MatcherDecl`, `SubjectSpec`, or a separate `flows` collection still exists.
- [x] Describe the actual Phase 3 binding/reference grammar and supported
  variable types.
- [x] Describe only validation errors that the compiler can produce and name
  the tests for each.
- [x] Check Phase 5 filter merging and contradiction tasks only after evidence
  projection and lifecycle canonicalization are complete.
- [x] Name the Phase 6 reference-oracle tests without unstable exact test counts.
- [x] Update Phases 8 through 11 for prepared arguments, explicit subject
  correlations, the single lifecycle root path, and retained executable
  requirements.
- [x] Check Phase 12 documentation only after all public examples compile.
- [x] Remove obsolete implementation-status prose and exact suite counts.
- [x] Leave no unchecked item in Phases 0 through 12 and ensure every checked
  claim names current implementation or test evidence.

## Final completion gate

Run from a clean worktree:

```sh
cargo test -p glass-lint-core --test query_composition
cargo test -p glass-lint-core --test query_baseline
cargo test -p glass-lint-core api::compiler::validate
cargo test -p glass-lint-core api::compiler::normalize
cargo test -p glass-lint-core api::compiler::physical
cargo test -p glass-lint-core api::compiler::reference
cargo test -p glass-lint-core analysis::matching::arguments
cargo check -p glass-lint-core --examples
make ci
git status --short
```

Completion requires:

- [x] malformed public authoring input is rejected early without panic,
  truncation, silent ignore, or unbounded allocation;
- [x] normalized correlations and requirements contain enough information to
  justify every selected physical operator and runtime preparation;
- [x] exact baselines and the logical/physical oracle pass;
- [x] public examples compile and current documentation shows one authoring
  path;
- [x] no obsolete authoring or lifecycle storage path remains outside
  intentional historical documentation;
- [x] `q-plan.md` has no unchecked or false claim through Phase 12;
- [x] e2e, project, JavaScript, and Obsidian fixtures pass; and
- [x] the final worktree contains only the scoped query migration, runtime
  preparation, documentation, examples, tests, and module-organization
  changes; `git diff --check` is clean.
