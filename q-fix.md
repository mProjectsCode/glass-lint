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

- [ ] Make every public lifecycle constructor that accepts text, a raw
  argument index, or a collection return `Result<_, QueryBuildError>`.
  This includes `LifecycleSource::returned_by`,
  `LifecycleSource::with_arg`, lifecycle member/property events,
  `LifecycleCondition::{any_of,all_of}`,
  `LifecycleCompletion::any_sink`, and both lifecycle sink constructors.
- [ ] Remove the `usize as u8` lifecycle argument casts. Index 256 currently
  truncates to zero instead of producing `InvalidArgumentIndex`.
- [ ] Make static-string alternatives, object-key sets, and rooted-expression
  sets validated non-empty bounded domain collections. Construction must sort,
  deduplicate, validate paths/text, and reject more than
  `MAX_STATIC_ALTERNATIVES`.
- [ ] Enforce `MAX_ARGUMENT_GROUPS` and
  `MAX_PREDICATES_PER_ARGUMENT` while authoring, not only during physical-plan
  validation.
- [ ] Enforce `MAX_QUERY_ROOTS_PER_RULE`, `MAX_EXPR_CHILDREN`,
  `MAX_LIFECYCLE_EVENTS`, and `MAX_LIFECYCLE_SINKS`. These constants are
  currently unused or enforced only after an oversized declaration exists.
- [ ] Add an explicit expression-depth/recursion bound and validate it before
  recursive compiler passes.
- [ ] Make `LifecycleQuery` impossible to construct without at least one valid
  source and exactly one valid completion. Remove or privatize the
  `#[doc(hidden)] LifecycleQuery::new` bypass, which currently accepts no
  completion.
- [ ] Give duplicate lifecycle stages and other lifecycle failures precise
  `QueryBuildError` variants rather than reporting them as empty collections.

### Close raw mutation routes

- [ ] Remove or restrict public `QueryDecl::{with_expression,with_primary_var,
  with_evidence}`, `EventQuery::with_var`, and raw expression constructors that
  let external callers bypass the Phase 12 authoring grammar. Keep any required
  malformed-IR builders as `pub(crate)` test helpers.
- [ ] Expose only the supported Phase 12 composition surface:
  compact `EventQuery`/`QueryDecl` constructors, `QueryDecl::any`,
  same-event `QueryDecl::all`, and `QueryDecl::lifecycle`.
- [ ] Validate `Any` branch emissions before discarding branch declarations.
  Compatible primary location and evidence kind must be proved on every branch;
  incompatible evidence must produce the structured
  `EvidenceProjection` contradiction rather than silently taking the first
  branch's emission.
- [ ] Validate evidence symbols at construction and remove infallible mutation
  that can install an empty symbol.

### Bounded normalization input

- [ ] Replace the dense `Vec` indexed by the largest authored `VarId` in
  `alpha_renumber_slots`. A public `VarId::new(u32::MAX)` must not request a
  multi-gigabyte allocation; remap only the bounded variables actually present.
- [ ] Make post-normalization validation actually verify dense typed slots,
  canonical collections, and all normalized invariants.

### Required tests

- [ ] Add limit and limit-plus-one tests for every declared collection and
  index limit, including lifecycle arguments/events/sinks and rule query roots.
- [ ] Add table-driven malformed lifecycle/value/path cases that prove no
  panic, truncation, silent ignore, or delayed stringified physical-plan error.
- [ ] Add a large/sparse `VarId` regression and a maximum-depth regression.
- [ ] Replace the current “predicate alternatives at limit succeeds” test with
  both at-limit success and limit-plus-one construction failure.

## 2. Finish logical validation and canonical normalization

The compiler has a real event/object type pass, but it still contains nominal
types and errors that no authored predicate can produce, and some canonical
forms are only descriptive.

### Type and relation model

- [ ] Either implement real producers and consumers for `StaticValue`,
  `CallableIdentity`, `ModuleIdentity`, and `SymbolPath` variables or remove
  those dead `VarType` variants and reconcile `q-plan.md`. Do not retain nominal
  types solely to claim Phase 4 coverage.
- [ ] Make relation-availability validation enforce an actual semantic scope.
  It currently repeats empty identity checks already prevented by constructors
  and does not prove local/project or artifact-local availability.
- [ ] Encode structurally that artifact-local IDs never cross artifact
  boundaries. Add the focused local/project scope tests promised by the plan,
  or remove inapplicable claims from `q-plan.md` when the public algebra cannot
  express such a relation.
- [ ] Remove or produce the dead `InvalidModulePattern` and
  `InvalidStaticValuePredicate` compiler errors. Expected malformed input should
  normally fail in declaration construction.

### Returned-object and instance correlation

- [ ] Replace the current flag-like `NormalizedSubject::{Direct,Returned,
  Instance}` with the explicit normalized relation required by the plan. It
  must retain the producer/constructor specification, correlated object slot,
  primary member event, scope, and evidence contract.
- [ ] Stop duplicating the producer/constructor identity as the selected member
  event identity merely to fit the old record shape.
- [ ] Select `ReturnedSubject` and `InstanceSubject` physical operators only
  after validating that the explicit normalized correlation is supported by
  the corresponding index.
- [ ] Make physical validation reject unsupported returned/instance identity,
  event, scope, and correlation shapes rather than checking only for a non-empty
  identity and member event.
- [ ] Extend the reference oracle so returned/instance support evidence is tied
  to an actual producer/constructor event and path key, not inferred from a
  single row's object number.

### Lifecycle and ordering canonicalization

- [ ] Canonicalize and bound order-independent lifecycle alternatives,
  especially `AnyOf` conditions and `AnySink` completions.
- [ ] Replace `format!("{:?}")` lifecycle ordering in normalization with a
  semantic deterministic ordering implementation.
- [ ] Add reversed-order and duplicate lifecycle condition/sink tests proving
  equal declarations normalize to equal plans without changing meaningful
  `AllOf` order or evidence order.

### Composition scope

- [ ] Reconcile nested `Any` inside `All` with the Phase 12 contract. General
  multi-event composition is not supported before Phase 13, so seal the raw
  public construction route and state that limitation explicitly rather than
  implying that arbitrary nested conjunctions execute.
- [ ] Retain focused incompatible-path negatives for every supported
  correlation: lifecycle, returned object, constructed instance, caller
  argument/callee parameter, and callee return/caller result.
- [ ] Replace the current normalization “idempotency” test, which normalizes the
  same authored input twice, with a test of the canonical normalized invariant
  or a real normalize-normalized round trip.

## 3. Complete per-argument preparation

Constraints are grouped by argument index and `ArgumentView` is constructed
once per group, but static values, object entries, and rooted paths are still
looked up again by each predicate through `ArgumentMatcher::matches`.

- [ ] Introduce one prepared argument value per referenced index containing the
  overlay-aware static value, object entries/properties, and rooted path needed
  by that group's predicates.
- [ ] Apply every predicate in the group to that prepared value without
  repeating `ValueTable` or name-path resolution.
- [ ] Charge deterministic candidate, group, preparation, resolution, and
  predicate operations. The operation model must expose repeated semantic
  resolution rather than counting only `ArgumentView` construction.
- [ ] Add mixed-predicate tests on one argument proving each semantic projection
  occurs once, plus multi-index and duplicate-predicate operation-count tests.

## 4. Make plan requirements execution-driving

`PlanRequirements` produces occurrence-index, fact-field, value-resolution,
flow, and project sets. Runtime code currently consumes only coarse project
and flow queries; `occurrence_indexes()` and `fact_fields()` are test-only, and
the occurrence index builder still populates every index family.

- [ ] For each requirement set, either add a real runtime preparation consumer
  or delete the set when the preparation is intentionally matcher-independent.
  Do not keep descriptive requirements.
- [ ] Preserve the rule-independent local artifact/cache boundary. If occurrence
  indexes are intentionally built once independent of selected rules, remove
  the false conditional-preparation claim instead of making cached facts depend
  on rule selection.
- [ ] Make project requirements select exact overlay families. Avoid
  `!project.is_empty()` as the implementation of several semantically distinct
  requirement queries.
- [ ] Cross-check physical roots and requirements in
  `validate_physical_plan`, including lifecycle flow levels, returned/instance
  indexes, constrained value preparation, and exact project overlays.
- [ ] Extend the stable plan summary to list every retained executable
  requirement, not only root counts and three coarse booleans.
- [ ] Add skip-work tests for every retained requirement and exact operation
  assertions for identity maps, result identities, overlays, local flow, call
  graphs, and fixed-point work.

## 5. Repair the Phase 0 inventory and baselines

The checked-in inventory and baseline report do not yet prove their stated
claims.

- [ ] Update [`glass-lint-core/QUERY_CAPABILITIES.md`](glass-lint-core/QUERY_CAPABILITIES.md)
  to the final logical model. It still describes the deleted `SubjectSpec`
  representation.
- [ ] Ensure every capability row names its authoring constructor, normalized
  relation, physical operator, runtime owner, project behavior, certainty
  behavior, provider users, and focused non-provider test.
- [ ] Add executable ambiguous-project and cross-file-flow baselines or remove
  those cases from [`reports/QUERY_MIGRATION_BASELINE.md`](reports/QUERY_MIGRATION_BASELINE.md).
  They are described in the report but absent from `query_baseline.rs`.
- [ ] Make baseline assertions exact. Remove assertions such as
  `files() == 0 || files() == 1` and broad `operation > 0` checks.
- [ ] Assert exact completion, finding/evidence order, certainty, relevant
  physical route/summary, and stable operation fields for each representative
  case.
- [ ] Record the commands and reviewed results for e2e, project, JavaScript, and
  Obsidian fixtures without embedding brittle obsolete test counts.

## 6. Finish the intended module and terminology cleanup

The final package layout in the remediation design has not been completed:
`api/rule/query/mod.rs`, `normalize.rs`, and `validate.rs` remain oversized,
and compiler errors still use historical clause terminology.

- [ ] Split `api/rule/query` into cohesive `error`, `limits`, `value`, `event`,
  `expression`, and `lifecycle` modules, leaving `mod.rs` as public re-exports.
- [ ] Split compiler query code into cohesive `error`, `validate`, `normalize`,
  `physical`, and test-only `reference` modules with orchestration in `mod.rs`.
- [ ] Rename `InvalidQueryClause` to a physical-plan validation term and remove
  obsolete clause/matcher terminology from current source and documentation.
- [ ] Keep indexed execution in `analysis/matching/query`, grouped argument
  execution in `analysis/matching/arguments`, lifecycle execution in
  `analysis/flow`, and project preparation in `analysis/project/projection`.
- [ ] Perform module moves separately from semantic changes and delete emptied
  paths in the same migration; do not leave compatibility re-exports.

## 7. Update public examples and documentation

The core README still uses removed `CallMatcher`, `.label`, `.category(&str)`,
and `.matcher` APIs. There are no compiled core examples, and `make ci` does
not check them.

- [ ] Add compiled examples under `glass-lint-core/examples` for:
  - a compact ordinary rule;
  - a constrained rule;
  - alternatives;
  - same-event conjunction;
  - returned-object and instance rules;
  - a lifecycle rule; and
  - structured construction/catalog errors.
- [ ] Make [`glass-lint-core/README.md`](glass-lint-core/README.md) mirror those
  examples using `description`, validated `Category`, `QueryDecl`, `query`,
  current report accessors, and fallible construction.
- [ ] Update the root README, affected provider READMEs, core architecture,
  public Rustdoc, contributing examples, and [`test.md`](test.md) where they
  describe the removed API or old lifecycle path.
- [ ] Document strict versus heuristic identity and the supported/unsupported
  composition boundary.
- [ ] Add this exact command to `make ci`:

  ```sh
  cargo check -p glass-lint-core --examples
  ```

- [ ] Restrict current-code matches for removed authoring terms to intentional
  historical documents:

  ```sh
  rg 'CallMatcher|MatcherDecl|MatcherDeclBuilder|QueryClause|QueryPlan|\.matcher\(|\.object_flow\(' \
    --glob '*.rs' --glob '*.md'
  ```

## 8. Reconcile `q-plan.md`

After Packages 1 through 7 are complete, make the original plan a truthful
completion record.

- [ ] Replace the historical Phase 0 inventory with links to the corrected
  capability matrix, exact baselines, execution owners, and flow-join tests.
- [ ] Rewrite Phase 2 around final `QueryDecl` ownership; remove statements that
  `MatcherDecl`, `SubjectSpec`, or a separate `flows` collection still exists.
- [ ] Describe the actual Phase 3 binding/reference grammar and supported
  variable types.
- [ ] Describe only validation errors that the compiler can produce and name
  the tests for each.
- [ ] Check Phase 5 filter merging and contradiction tasks only after evidence
  projection and lifecycle canonicalization are complete.
- [ ] Name the Phase 6 reference-oracle tests without unstable exact test counts.
- [ ] Update Phases 8 through 11 for prepared arguments, explicit subject
  correlations, the single lifecycle root path, and retained executable
  requirements.
- [ ] Check Phase 12 documentation only after all public examples compile.
- [ ] Remove obsolete implementation-status prose and exact suite counts.
- [ ] Leave no unchecked item in Phases 0 through 12 and ensure every checked
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

- [ ] malformed public authoring input is rejected early without panic,
  truncation, silent ignore, or unbounded allocation;
- [ ] normalized correlations and requirements contain enough information to
  justify every selected physical operator and runtime preparation;
- [ ] exact baselines and the logical/physical oracle pass;
- [ ] public examples compile and current documentation shows one authoring
  path;
- [ ] no obsolete authoring or lifecycle storage path remains outside
  intentional historical documentation;
- [ ] `q-plan.md` has no unchecked or false claim through Phase 12;
- [ ] e2e, project, JavaScript, and Obsidian fixtures pass; and
- [ ] the final worktree contains no unrelated or generated changes.
