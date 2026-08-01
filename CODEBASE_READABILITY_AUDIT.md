# Codebase Readability and Maintainability Audit

## Summary

This review covered the Rust workspace, with extra attention to the largest
core analysis modules, project loading, harness profiling, provider catalogs,
and public rule-query APIs. The code is generally disciplined about explicit
budgets, deterministic collections, and conservative failure behavior. The
main maintainability risk is that several boundaries described by the
architecture are being crossed by orchestration code, while older and newer
implementations coexist.

The 17 findings below are ordered by code area rather than by severity. They
are readability and technical-debt observations, not confirmed correctness
bugs.

## Findings

### Core project linking and resolution

#### RL-001 — Export resolution has two independent implementations

- Severity: Medium
- Fix complexity: High
- Category: Duplication / Architecture
- Location: `glass-lint-core/src/analysis/project/linker/export.rs:24-490`; `glass-lint-core/src/analysis/project/exports.rs:18-197`; `glass-lint-core/src/analysis/project/linker/mod.rs:49-85`; `glass-lint-core/src/analysis/project/linker/state.rs:119-150`

The linker export module and the project semantic model both implement direct
exports, star re-exports, cycle handling, and imported-identity lookup. They
also use different cache ownership models: the older-looking linker path uses
`RefCell<ExportLookupCache>`, while `LinkingSession` carries an explicit
mutable cache. The duplication makes it difficult to identify the authoritative
semantics and creates a realistic drift risk when an edge case is fixed in only
one path.

Recommendation: keep the two phase-specific entry points: `ProjectLinker` owns
SCC/fixed-point export-table construction and import validation, while
`ProjectSemanticModel` owns post-link identity lookup for provenance and flow.
Extract their shared direct/star lookup and cache policy into one resolver with
an explicit `LinkingSession`; then delete the duplicated recursive helpers.
Preserve cycle, fixed-point, budget, and deterministic-order tests during the
migration rather than deleting one whole phase by assumption.

#### [x] RL-002 — Single-threaded `Rc` is aliased as `Arc`

- Severity: Medium
- Fix complexity: Low
- Category: Naming / API clarity
- Location: `glass-lint-core/src/analysis/resolution/mod.rs:16,123-124`; `glass-lint-core/src/analysis/resolution/expression.rs:3,32-33`; `glass-lint-core/src/analysis/resolution/call.rs:73`

The resolver imports `std::rc::Rc as Arc`, while nearby code alternates between
that alias and explicit `std::rc::Rc`. Comments then describe the arena as
using “Arc”. The implementation is intentionally single-threaded, but the
name communicates the opposite ownership and thread-safety model and makes
future imports easy to get wrong.

Recommendation: rename the alias to `Rc` or remove it, and update the comments
to describe reference-counted arena handles accurately. Treat the resolver as
thread-confined local-analysis state; reserve `Arc` for artifacts that are
actually shared across the file-level parallel boundary. Do not redesign the
resolver for hypothetical future sharing without a concrete ownership need.

Implementation: renamed the resolver's `std::rc::Rc` alias to `Rc` and updated
all resolver signatures, cache operations, constructors, and documentation to
use the explicit single-threaded ownership name. No ownership or caching
behavior changed; the resolver remains thread-confined analysis state.

Runtime (release): profiling
`/home/lemon/src/obsidian-stats/data/out/plugin-release-mainjs/byoc/1.0.13-main.js`
with one warm-up-free repetition and one worker took 721.4ms measured lint
wall time (720.8ms input time; 11.92s process wall time including the one-time
release build).

### Core fact, matching, scope, and flow phases

#### RL-003 — Fact storage owns matcher and flow projection orchestration

- Severity: Medium
- Fix complexity: Medium
- Category: Module boundaries / Cohesion
- Location: `glass-lint-core/src/analysis/facts/mod.rs:386-487,583-630`

`ProjectionPlan` stores compiled matcher roots, rule capacity, matcher-derived
identity requirements, and flow requirements. `SemanticFacts::project` then
calls both constrained matching and object-flow collection and returns
classification evidence. This makes the facts module responsible for a
query-independent artifact and for query-selected execution at the same time,
despite the architecture assigning occurrence indexes to matching and flow
projection to flow.

Recommendation: move the plan and projection coordination to a focused
analysis projection module, leaving `SemanticFacts` as the immutable stream,
interface, and index owner. Keep the single fact-building/indexing pass, but
make the matching and flow calls explicit at their owning boundary.

#### RL-004 — Source-order collectors are broad mutable “god objects”

- Severity: Medium
- Fix complexity: Medium
- Category: State ownership / Complexity
- Location: `glass-lint-core/src/analysis/scope/build/mod.rs:45-106`; `glass-lint-core/src/analysis/facts/mod.rs:77-109`

`ScopeCollector` carries lexical scopes, assignments, property mutations,
dynamic-eval state, function aliases, callback state, name interning,
versioning, shape diagnostics, budgets, and control-flow checkpoints.
`FactBuilder` similarly combines traversal, call results, instance and class
origins, string provenance, and module-interface construction. The comments
explain each field well, but the constructors and visitor methods still need
to know about many independent invariants whenever state is added or moved.

Recommendation: group state into cohesive sub-states such as collection
artifacts, provenance, control flow, and interface building, while retaining a
small visitor façade. Give each sub-state its own construction and mutation
methods so invariants remain local without introducing extra traversals.

#### RL-005 — Cross-file flow collection is a context-heavy worklist method

- Severity: Medium
- Fix complexity: High
- Category: Complexity / Encapsulation
- Location: `glass-lint-core/src/analysis/flow/cross/mod.rs:50-178`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:128-260`; `glass-lint-core/src/analysis/flow/projector/mod.rs:73-205`

The cross-flow collector intentionally keeps one worklist loop, but that loop
coordinates budgets, call graphs, source propagation, per-context plans,
usage projection, call propagation, evidence, exhaustion, and final output.
The source comments acknowledge “12+ context fields”; `emit` suppresses both
`too_many_arguments` and `too_many_lines`, and `ObjectFlowProjector::new` also
requires a long positional argument list. These are signs that the state model
is difficult to understand locally even though the behavior is bounded.

Recommendation: introduce named context objects for cross-projection state and
trace/evidence emission, and split the worklist into small phase operations
around the same bounded loop. Replace positional constructors with input
structs so adding a budget or requirement cannot silently shift call sites.

#### RL-006 — Evidence is represented by parallel vectors and raw tuples

- Severity: Medium
- Fix complexity: Medium
- Category: Encapsulation / Domain modeling
- Location: `glass-lint-core/src/analysis/flow/cross/evidence.rs:78-260`; `glass-lint-core/src/analysis/project/projection.rs:33-38,133-140`; `glass-lint-core/src/analysis/facts/mod.rs:597-600`; `glass-lint-core/src/analysis/flow/projector/mod.rs:73-81`; `glass-lint-core/src/analysis/matching/evidence.rs:9-20`

Cross-flow evidence keeps `evidence`, `seen`, and `nonmatching` as separate
vectors indexed by `rule_idx`, and repeatedly reconstructs tuple keys from
match kind, symbol, and event ID. Other modules pass the same concept around
as `Vec<Vec<ClassificationEvidence>>`, while local matching already has an
`EvidenceKey` and `EvidenceAccum`. The representation makes capacity, index
alignment, deduplication, and merge rules implicit at every call site.

Recommendation: add a bounded `EvidenceStore`/`RuleEvidence` domain type with
rule-key access, shared occurrence keys, and explicit insert, merge, and
nonmatching operations. Reuse it for local and cross-flow projection, keeping
the final report conversion as the one place that exposes nested vectors.

#### RL-007 — Semantic IDs expose raw storage and invite duplicated arithmetic

- Severity: Medium
- Fix complexity: Medium
- Category: Newtypes / Invariant ownership
- Location: `glass-lint-core/src/analysis/model/fact.rs:11-31`; `glass-lint-core/src/analysis/model/scope.rs:12-18,47-55`; `glass-lint-core/src/analysis/model/value.rs:6-15`; examples at `glass-lint-core/src/analysis/flow/projector/control.rs:105`, `glass-lint-core/src/analysis/facts/stream.rs:137,218,234,267`, and `glass-lint-core/src/analysis/flow/cross/evidence.rs:113,146,220`

Several semantic identifiers are public tuple structs (`FactId`, `ScopeId`,
`BindingId`, `ValueId`, and others), so callers can forge values and access raw
indices. Callers also perform their own `.0` arithmetic, including constructing
the next `FactId` with saturating arithmetic. `FactId::from_index` is test-only
even though related raw construction remains broadly available, which leaves
the intended invariant boundary unclear.

Recommendation: make fields private and provide checked constructors plus
domain operations such as `index`, `next`, or bounded offset methods. Keep raw
conversion at a narrow serialization/debug boundary and update callers in one
migration so invalid IDs cannot become an ambient convention.

#### RL-008 — CommonJS `require` recognition is scattered across phases

- Severity: Medium
- Fix complexity: Medium
- Category: Semantic duplication / Module boundaries
- Location: `glass-lint-core/src/analysis/resolution/call.rs:39-57`; `glass-lint-core/src/analysis/scope/build/provenance.rs:75-153`; `glass-lint-core/src/analysis/facts/mod.rs:277-318`; `glass-lint-core/src/analysis/facts/calls/wrapper.rs:106-113`

Direct unshadowed `require` recognition, wrapper handling, dynamic-import
handling, alias provenance, and module-request emission are distributed among
the resolver, scope builder, fact builder, and call wrapper. The code has
legitimate phase-specific responsibilities, but each layer partially decides
what counts as a module loader, so direct versus wrapped behavior can drift.

Recommendation: define one provider-neutral module-request recognizer or value
type with explicit direct, alias, and wrapper policies. Let provenance, fact
interface collection, and import emission consume that result, with shared
tests for shadowing, reassignment, literals, wrappers, and dynamic values.

#### RL-009 — Report assembly contains a matcher-specific private-network branch

- Severity: Medium
- Fix complexity: Medium
- Category: Layering / Special cases
- Location: `glass-lint-core/src/lint/report.rs:183-330`; related query constants at `glass-lint-core/src/api/rule/query/mod.rs:172-185,638-659`

`findings_for_capability` combines evidence grouping, range containment,
trace-to-location conversion, certainty/truncation aggregation, and finding
construction. `narrow_string_span` then knows that one private-network symbol
requires a special matcher, while ordinary string evidence uses a substring
search. The source itself labels this as an unwanted special case, so report
assembly is currently carrying a piece of matcher semantics that does not
generalize to other rules.

Recommendation: have the matcher/evidence layer provide the display span or a
generic span-narrowing strategy as part of classification evidence, and keep
report assembly focused on grouping and serialization. Independently extract
range grouping and trace conversion into small report-owned helpers so adding a
new evidence kind does not enlarge this method.

### Project loading and configuration

#### RL-010 — Project load waves mix I/O, analysis, budgets, and graph mutation

- Severity: Medium
- Fix complexity: High
- Category: Orchestration / State ownership
- Location: `glass-lint-project/src/loader.rs:527-630`

`ProjectLoadState::process_wave` handles admission, metadata and source reads,
cumulative byte accounting, deferred errors, source retention, parallel
analysis, timing, request limits, resolution recording, and enqueueing. The
adjacent `record_requests` path also combines timeout, timing, cache lookup,
resolution insertion, and frontier mutation. A change to one policy therefore
requires reasoning about filesystem behavior, partial results, and graph state
in the same method.

Recommendation: split the wave into typed outcomes for read/admission,
analysis, and request resolution, with a small coordinator applying the budget
and frontier transitions. Preserve deferred-error semantics explicitly in the
outcome types so partial project results remain deterministic and observable.

#### RL-011 — Recursive tsconfig building threads a large positional context

- Severity: Low
- Fix complexity: Medium
- Category: API shape / Recursion context
- Location: `glass-lint-project/src/tsconfig/mod.rs:384-439`

Both `build_effective_config` and its recursive helper take eight parameters,
including deadline, diagnostics, budget, config count, resource budget, and the
mutable extends chain. The paired `too_many_arguments` suppressions are
understandable, but the repeated positional context makes recursive call sites
and ownership of traversal policy harder to scan.

Recommendation: introduce a `TsconfigTraversal` context owning diagnostics,
budgets, deadline, count, and extends-chain state, with a method that accepts
only the config path and fallback base. Keep parsing, merging, and compilation
as separate value-level operations and retain the current deterministic
diagnostics.

### Harness profiling

#### [x] RL-012 — Scoped profile workers use shared ownership and a mutex unnecessarily

- Severity: Medium
- Fix complexity: Medium
- Category: Concurrency / Ownership
- Location: `glass-lint-harness/src/profile/runner.rs:671-721`

`execute_file_profile` receives immutable prepared files and linters as
`&Arc<Vec<_>>`, clones those `Arc`s into `thread::scope`, and collects every
worker result through `Arc<Mutex<Vec<...>>>`. The vectors do not need ownership
inside scoped threads, and locking once per result obscures the actual
parallelism and creates a serial collection bottleneck before
`Arc::try_unwrap`.

Recommendation: accept slices, borrow them directly in the scoped workers, and
let each worker build a local result vector that is flattened after the scope.
Retain `Arc` only for linter state that genuinely must be shared across
workers. Keep the final path sort as the intentional deterministic-output
contract, and add tests for sorted results and stable evidence digests across
different worker schedules.

Implementation: changed file profiling to borrow the prepared files and linter
collections as slices, while retaining `Arc` only inside each shareable linter
and for worker coordination. Each scoped worker now accumulates a private
result buffer, which the coordinator joins, flattens, and path-sorts without a
mutex. The worker-count test now checks sorted paths and per-file evidence
digest stability across serial and parallel schedules.

Runtime (release): profiling
`/home/lemon/src/obsidian-stats/data/out/plugin-release-mainjs/byoc/1.0.13-main.js`
with one warm-up-free repetition and one worker took 744.6ms measured lint
wall time (743.4ms input time; 2.96s process wall time including the
incremental release build).

### Public query API and provider declarations

#### RL-013 — Lifecycle query API retains aliases and repeated adapters

- Severity: Low
- Fix complexity: Medium
- Category: Public API / Naming
- Location: `glass-lint-core/src/api/rule/query/lifecycle.rs:324-468,470-590`; related adapter pattern at `glass-lint-core/src/api/rule/query/mod.rs:1138-1170`

Lifecycle sinks expose explicit global/member methods plus legacy
`argument_of` and `any_argument_of` spellings. The file also repeats the same
fallible-value adapter pattern for event, completion, sink, and source types.
The builder stores `invalid_operation` and continues returning `Self`, so an
invalid intermediate operation leaves a partially populated builder whose
failure is only visible at `build`.

Recommendation: since breaking changes are permitted, converge on one naming
path and centralize the fallible-input conversion pattern. Consider a builder
state or result-based API that makes invalid intermediate operations visible at
the operation boundary, while keeping final semantic validation centralized.

#### [x] RL-014 — Provider rule catalogs encode data as long fluent expressions

- Severity: Low
- Fix complexity: Low
- Category: Declarative data / Boilerplate
- Location: `glass-lint-js/src/rules/node/network/mod.rs:9-57`; `glass-lint-js/src/rules/js/service_indicator/mod.rs:13-48`; `glass-lint-obsidian/src/rules/codemirror/extension/mod.rs:9-34`; related pattern at `glass-lint-js/src/rules/node/filesystem/mod.rs:5-69`

Several provider modules are mostly repeated `.query(...)` calls or parallel
arrays of names and query methods, with `too_many_lines` or related lint
allowances. The policy is declarative, but its data is embedded in control
flow, so adding or reviewing a catalog entry requires navigating a long fluent
expression and maintaining ordering manually.

Recommendation: represent catalog rows in typed constant slices and provide a
small helper for registering them, or add an iterator-based query registration
method. Preserve declaration order and generated catalog output, and keep
provider-specific policy in the row definitions rather than introducing a
generic registry layer.

Implementation: added an iterator-based `RuleBuilder::queries` method and
converted the Node network/filesystem, service-indicator, and CodeMirror
catalog rows to typed constant slices. Registration still occurs in source
order with the same deferred query validation, while provider policy remains
in the row data and the obsolete line-count allowances were removed where the
catalogs became compact.

Runtime (release): profiling
`/home/lemon/src/obsidian-stats/data/out/plugin-release-mainjs/byoc/1.0.13-main.js`
with one warm-up-free repetition and one worker took 726.3ms measured lint
wall time (725.0ms input time; 3.33s process wall time including the
incremental release build).

#### RL-015 — Query authoring is concentrated in one large module with repeated constructors

- Severity: Medium
- Fix complexity: Medium
- Category: Module cohesion / Boilerplate
- Location: `glass-lint-core/src/api/rule/query/mod.rs:125-660,773-1136,1450-2006`

The module contains the typed algebra, all `EventQuery` constructors, argument
constraint adapters, `QueryDecl` composition, explanation formatting, and a
large constructor-oriented test suite. Many constructors repeat the same
`VarId::new(0)`, empty-constraint initialization, name validation, and
`EventSpec`/`IdentitySpec` assembly. The existing `event`, `value`, `expression`,
and `lifecycle` submodules show that the API already has natural seams, but
the central module remains difficult to navigate and extend.

Recommendation: add private construction helpers for validated names, member
paths, and event/identity pairs, then move declaration composition and its
tests into focused modules. Keep the public constructors stable and explicit;
the simplification should remove repeated invariant code, not hide the
provider-neutral query vocabulary behind a generic builder.

#### [x] RL-016 — CLI provider/profile construction builds and then disassembles a linter

- Severity: Low
- Fix complexity: Low
- Category: Duplication / Construction flow
- Location: `glass-lint-cli/src/config.rs:304-361`

`base_linter` maps the provider and profile to a catalog/environment and builds
a linter with the baseline selection. `selected_linter` calls `base_linter`,
extracts its catalog and environment, recreates a new `LinterConfig`, and
repeats the profile-baseline mapping before applying overrides. This makes the
construction path harder to follow and leaves two places that must stay in
sync when a provider or profile gains another policy.

Recommendation: factor provider configuration and profile selection into
shared helpers, then construct the linter once from those values. Keep
`base_linter` as the validated default entry point, but have both public paths
use the same provider-config and baseline-selection functions instead of
building an intermediate linter solely to read its fields.

Implementation: centralized provider configuration, profile baseline mapping,
and profile-plus-override selection in private helpers shared by `base_linter`
and `selected_linter`. The complete CLI path now constructs one `Linter` from
the selected provider config, profile selection, and core limits, while
preserving the existing provider environments and override semantics.

Runtime (release): profiling
`/home/lemon/src/obsidian-stats/data/out/plugin-release-mainjs/byoc/1.0.13-main.js`
with one warm-up-free repetition and one worker took 840.6ms measured lint
wall time (839.3ms input time; 0.96s process wall time).

### Test and fixture ergonomics

#### RL-017 — Some unit tests exercise storage through verbose raw payloads

- Severity: Low
- Fix complexity: Low
- Category: Test readability
- Location: `glass-lint-core/src/analysis/model/fact.rs:75-205`; related fixture-heavy areas at `glass-lint-project/src/tsconfig/tests.rs` and `glass-lint-core/tests/integration/matching/declarative.rs`

The fact-model tests construct large payload values field by field and then
mostly destructure or compare stored fields. The tsconfig and declarative
matching test modules are also large and repeat setup for raw JSON/config and
rule-builder cases. This makes a behavioral regression harder to distinguish
from fixture plumbing and increases the cost of adding a focused edge case.

Recommendation: add narrowly scoped test factories/builders with explicit
defaults, and make each test override only the field relevant to its behavior.
Split genuinely independent scenario families into submodules while retaining
the existing adversarial coverage and deterministic fixture names.

## Systemic themes

1. **Phase ownership is becoming implicit.** Facts, scope, matching, flow, and
   project loading each have clear architectural roles, but coordinator types
   increasingly own the state and calls of several phases. Small context/value
   types would make those boundaries visible without adding passes.
2. **Parallel collections conceal invariants.** Nested vectors, tuple keys, and
   raw IDs are efficient representations, but they move correctness obligations
   to every caller. Domain-owned bounded collections and semantic ID methods
   would improve both readability and reviewability.
3. **Compatibility and incremental evolution are visible in the API.** The
   lifecycle aliases, repeated adapters, duplicate export resolver, and
   explicit clippy suppressions suggest useful migrations have accumulated
   without a final cleanup pass. A deliberate breaking cleanup would be less
   risky than adding another compatibility layer.
4. **The code already has good safety foundations.** Budget types, deterministic
   maps/sets, conservative unsupported-input handling, and several existing
   domain newtypes are strong patterns to extend; the recommendations above
   should follow those conventions rather than introduce broad frameworks.

## Decisions and guidance

1. **Export resolution:** retain the distinct link-time and post-link entry
   points, but consolidate their shared direct/star lookup and cache behavior.
   This is a staged deduplication, not a wholesale deletion of either module.
2. **Resolver ownership:** make the resolver's `Rc` usage explicit now. The
   resolver is thread-confined; cross-thread sharing belongs at the artifact
   boundary and should use real `Arc` values.
3. **Profile ordering:** treat path-sorted results as the observable contract.
   Replace mutex-based result collection with per-worker buffers, flatten them,
   sort once, and test both ordering and evidence-digest determinism.

Recommended implementation order: export lookup consolidation first, profile
result collection second, and the `Rc` rename alongside either low-risk
cleanup.

## Coverage and verification

Reviewed the root architecture, testing and contribution guidance, all Rust
crates in the workspace, the largest analysis and orchestration modules, and
representative provider and test modules. The initial worktree was clean.

`cargo clippy --workspace --all-targets --all-features -- -D warnings` passed.
This is useful baseline evidence, but the audit treats explicit
`#[allow(clippy::too_many_arguments)]` and `#[allow(clippy::too_many_lines)]`
annotations as maintainability signals rather than lint failures.

No Rust source, tests, configuration, dependencies, or existing documentation
were modified by this audit; this report is the only intended workspace change.
