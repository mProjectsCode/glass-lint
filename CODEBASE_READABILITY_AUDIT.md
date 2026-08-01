# Codebase Readability Audit

## Summary

This is a replacement for the previous audit. The previous report described
17 refactors as completed and therefore no longer represented the current
tree. This review scanned the Rust workspace, including the extracted
datastructures crate, core analysis/compiler/flow code, project loading,
output, harness, CLI, provider catalogs, and the largest test modules.

The strongest remaining maintenance risks are duplicated checkpoint-history
implementations, stateful flow coordinators whose fields span several phases,
and public path-storage APIs that expose raw representation details. The
workspace otherwise has good boundaries, bounded work, deterministic storage,
and conservative error behavior. The findings below are maintainability and
simplification observations, not confirmed correctness bugs.

## Findings

### Shared storage and state history

#### READ-001 — Checkpoint/LCA history is implemented repeatedly

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Duplication / Architecture
- **Location:** `glass-lint-core/src/analysis/scope/build/history.rs:73-238,241-365`; `glass-lint-core/src/analysis/flow/projector/history.rs:23-137`; `glass-lint-core/src/analysis/facts/origin_map.rs:5-115`

Assignment provenance, write tracking, flow mutation state, and origin maps
all implement variations of the same parent-linked checkpoint algorithm:
cursor tokens, parent chains, restore transitions, inverse deltas, and
branch cleanup. The domain payloads differ, but the repeated path/LCA and
rollback mechanics mean fixes to invalid checkpoints, budget behavior, or
transition handling can drift between phases.

Extract a bounded checkpoint tree/history primitive that owns cursor validity,
parent traversal, and transition ordering while allowing each owner to supply
typed delta application. Keep domain-specific reference counts and map
invariants in the owning state, and preserve the current budget and
single-use-token semantics in focused tests.

**Decision/implementation:** Generalize the storage mechanics in
`glass-lint-datastructures` while keeping delta application in core. The new
`ParentLinkedHistory` is used by scope assignment/write state and the flow
mutation log; the facts origin map remains a separate transactional owner
because its checkpoint lifecycle also discards committed logs.

#### READ-002 — Path identities still expose a raw tagged integer protocol

- **Severity:** High
- **Fix Complexity:** High
- **Category:** API / Newtype / Encapsulation
- **Location:** `glass-lint-datastructures/src/path_trie/types.rs:5-31`; `glass-lint-datastructures/src/path_trie/store.rs:5-117,159-216`; `glass-lint-core/src/analysis/flow/summary/store.rs:6-207`

`PathId`, `PathNode::parent`, `ParentPathStore::by_edge`, and
`SummaryPathId` repeatedly convert between `u32` values and manually apply
the overlay bit. `ParentPathStore::insert_edge` is public across the crate
boundary, accepts a caller-supplied depth, and explicitly skips parent
validation; `SummaryPathStore` then rebuilds frozen/overlay dispatch with raw
masking. The identical `intern_frozen` and `resolve_frozen` methods are
another sign that the representation, rather than a path-domain type, is
driving the API.

Make parent and overlay identity opaque and typed at the storage boundary.
Expose an operation that computes depth and validates the parent, or make the
unchecked edge insertion a narrowly scoped internal capability. Centralize
tagging and frozen/overlay dispatch in one owner, return `PathId` rather than
raw parent integers, and remove duplicate conversion methods.

**Decision/implementation:** `PathNode` is now private, parent links use
`PathId`, and the unchecked `insert_edge(parent, depth)` shape is replaced by
`append_linked(parent, parent_depth, segment)`, which computes child depth.
Summary-path tagging and frozen/overlay dispatch are centralized in
`SummaryPathId`; the duplicate frozen-resolution method was removed. The
remaining follow-up is to decide whether `append_linked` should move behind a
summary-owned wrapper entirely.

#### READ-003 — `ScopeCollector` remains a cross-phase mutable coordinator

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Cohesion / Complexity
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:45-117`; `glass-lint-core/src/analysis/scope/build/collector.rs:23-195`

The collector still combines predeclared scope storage, names, function
aliases and callbacks, pending function metadata, source-order assignments,
two independent checkpointed histories, control-flow frames, reachability,
alternative limits, and test counters. Splitting finalized artifacts helped,
but the constructor and most visitor helpers still need to understand the
whole mutable state graph when a new invariant is introduced.

Group the state by ownership: lexical/name state, callback/function state,
path-sensitive assignment state, and control-flow traversal state. Give each
group a small API and keep the SWC visitor as a façade that coordinates those
APIs; this should not add a second AST traversal or duplicate provenance
logic.

### Core flow, projection, and compiler layers

#### READ-004 — Local object-flow projection has a mixed-mode state machine

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Complexity / State ownership
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:129-255,270-372,378-751`

`ObjectFlowProjector` owns fact indexing, flow state, object allocation,
control frames, alternative paths, pending requirements, binding slots,
reachability, summary exhaustion, emission mode, trace output, and several
independent counters. The `struct_excessive_bools` allowance reflects real
state coupling: normal transfer, loop fixed-point replay, evidence emission,
and exhaustion finalization all mutate the same object.

Introduce explicit sub-state types for limits/outcomes, emission/replay,
active path traversal, and object-flow storage. Keep the top-level transfer
loop, but move mode transitions and final exhaustion classification onto the
state that owns them; use enums where booleans currently describe mutually
exclusive lifecycle states.

#### READ-005 — Cross-file collection still relies on an intentionally huge loop

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Complexity / Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:104-215`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:187-306`

The collector suppresses `too_many_lines` and documents that extracting the
loop would require passing “12+ context fields.” One loop performs lifecycle
root discovery, source collection, budget management, plan caching, effect
lookup, per-context projection, worklist expansion, exhaustion cleanup, and
evidence conversion. The separate `EmissionContext` improves one call site,
but the orchestration still makes phase ordering and ownership difficult to
review.

Retain one bounded worklist but move setup, one-context execution, budget
termination, and finalization behind named operations on a collector context.
Use an input/output context struct for the per-context projector and give the
worklist a typed termination reason so exhaustion cleanup cannot be separated
from the condition that caused it.

#### READ-006 — Evidence is re-exposed as rule-indexed nested vectors

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation / Newtype
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:134-188,283-289,410-446`; `glass-lint-core/src/analysis/flow/projector/mod.rs:73-126`; `glass-lint-core/src/analysis/flow/cross/mod.rs:110-112,202-207`

Cross-flow has a `ModuleEvidence` owner, but the projection boundary converts
back to `BTreeMap<ModuleId, Vec<Vec<ClassificationEvidence>>>`; local flow
and `project_facts` use the same shape. Callers then index by
`RuleIndex::get()`, extend vectors, and rely on rule capacity and ordering
being aligned. This makes the most important evidence invariants implicit
again immediately after the recent encapsulation work.

Carry a module/rule evidence type through local and cross projection, with
methods such as `for_rule`, `merge`, `clear`, and `into_report_evidence`.
Keep nested vectors only at the final report adapter, and make out-of-range
rule access an explicit no-op or typed error rather than an indexing
convention.

#### READ-007 — Compiler correlation validation contains duplicate logic

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/validate/pass4_10.rs:170-213,234-263`

`check_correlation_evidence` and `check_correlation_scope_inner` both compute
branch variables for `All`, build a `BTreeSet` from the first branch, check for
a shared variable, and recurse through `Any`/`All`. The file describes these
as consolidated validation passes, so keeping the same conjunction check in a
second helper creates a drift point whenever query correlation semantics
change.

Extract one `validate_correlated_branches` helper that accepts the recursion
mode or a small callback for the follow-up traversal. Keep evidence-primary
checks separate from correlation checks, but make the shared-variable rule a
single implementation with one focused test set.

### Public authoring APIs and provider policy

#### READ-008 — Fluent builders hide invalid operations until `build`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** API / Error handling
- **Location:** `glass-lint-core/src/api/rule/mod.rs:118-149`; `glass-lint-core/src/api/rule/query/lifecycle.rs:460-577`

`RuleBuilder::query` and `LifecycleQueryBuilder::{source,condition,completion}`
accept fallible inputs but return `Self`, storing only the first failure in
`first_query_error` or `invalid_operation`. Callers can continue composing an
invalid builder, later operations can be silently ignored or superseded, and
the eventual error is reported far from the operation that caused it. This
also forces each builder to maintain hidden error-state rules.

Since breaking changes are allowed, make fallible operations return
`Result<Self, ...>` or add explicit `try_*` methods while keeping infallible
metadata setters fluent. If deferred validation is retained for compatibility,
document first-error semantics and centralize the error accumulator so the two
builders do not evolve separate hidden-state contracts.

**Decision/implementation:** Preserve existing catalog ergonomics and add
strict `try_query`/`try_queries` and lifecycle `try_*` methods. New code can
propagate construction errors at the operation boundary without forcing plain
`Rule` catalogs into awkward error plumbing; the legacy methods retain their
documented deferred first-error behavior.

#### READ-009 — Several provider catalogs still encode data as long fluent code

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Declarative data / Boilerplate
- **Location:** `glass-lint-js/src/rules/browser/environment/mod.rs:8-54`; `glass-lint-js/src/rules/node/crypto_operation/mod.rs:8-71`; `glass-lint-js/src/rules/node/process_environment/mod.rs:9-55`; `glass-lint-js/src/rules/node/subprocess/mod.rs:8-51`; `glass-lint-js/src/rules/js/telemetry_indicator/mod.rs:12-48`; `glass-lint-obsidian/src/rules/metadata/traversal/mod.rs:15-192`

These rules are primarily lists of module names or member paths, yet each
keeps the list in a long chain of `.query(...)` calls and several require a
`too_many_lines` allowance. The core already provides `RuleBuilder::queries`,
and some neighboring catalogs use typed slices, so the remaining forms make
policy review and additions unnecessarily noisy while preserving ordering as
an incidental source-code property.

Move pure catalog rows to typed constant slices and register them through the
existing iterator API. Keep exceptional queries (such as the metadata
argument predicate) explicit, preserve source order and generated rule output,
and avoid a generic registry that would obscure provider policy.

#### [x] READ-010 — Module-specifier pattern uses a boolean mode with dead-code escapes

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** API / Naming
- **Location:** `glass-lint-core/src/api/rule/module.rs:7-79`

`ModuleSpecifierPattern` stores `name` plus `package: bool`, so `matches`
branches on a mode bit rather than representing exact and package-root
patterns as distinct variants. The `exact` constructor and `is_package`
accessor are individually hidden behind `dead_code` allowances, suggesting
the public surface and the actual call sites have not settled on one model.

Use an enum or a private pattern kind with constructors and matching behavior
owned by that kind. Either expose the exact/package distinction intentionally
or keep both constructors internal; remove the allowances once the API has a
single documented ownership path.

**Implementation:** Replaced the `package: bool` representation with a private
`PatternKind` enum that owns matching and package classification, retained the
public exact/package constructors and documented `is_package` distinction, and
removed the dead-code allowances. The requested bundle’s release lint took
**0.86 s wall time** after the change (0.83 s baseline).

### Project loading and rendering

#### [x] READ-011 — Resolution cache uses two maps and an invariant-dependent unwrap

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation / Error handling
- **Location:** `glass-lint-project/src/loader.rs:341-382`

`ResolutionCache::resolve_or_get` checks `by_key`, derives a second semantic
key, conditionally populates both maps, and finally retrieves the result with
`get(...).unwrap()`. The unwrap is logically justified by the preceding
branches, but the cache invariant is spread across mutation order and a later
caller cannot tell whether the returned reference came from occurrence-level
or semantic caching.

Give the cache a single `get_or_resolve` operation backed by an entry-oriented
implementation and return an owned outcome or a cache-entry handle. Centralize
the relationship between occurrence and semantic keys, and use an explicit
internal error or `debug_assert` if the two-map invariant is ever violated.

**Implementation:** Made `resolve_or_get` the single entry-oriented cache
operation, centralized semantic-key construction, and populated the occurrence
index through `BTreeMap::entry`; a dedicated `CacheInvariant` error plus
`debug_assert` handles an impossible missing entry without `unwrap`. The
requested bundle’s release lint took **0.83 s wall time** after the change
(0.83 s baseline).

#### [x] READ-012 — Closed project-frontier state repeats lifecycle plumbing

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Duplication / State ownership
- **Location:** `glass-lint-project/src/loader.rs:510-594,687-777`

`ProjectLoadState` and `ClosedFrontier` each implement the same deadline check,
and `ClosedFrontier::finish` and `finish_partial` are wrappers around the same
`finish_inner` operation differing only in whether they check the deadline.
The wave refactor made admission, analysis, and resolution clearer, but this
second state split still duplicates termination policy and makes it easy for a
future finish path to forget the appropriate timeout behavior.

Represent the deadline as a small shared guard or pass a checked/partial
termination mode into one finalization method. Keep the distinction between
complete and partial outcomes explicit, but make the timeout policy one
implementation rather than two copies of the same expression.

**Implementation:** Added the shared `LoadDeadline` guard and replaced the
separate complete/partial frontier finish wrappers with one `finish` method
that takes an explicit `FinishMode`; partial reports retain their existing
timeout semantics while complete reports share one deadline check. The
requested bundle’s release lint took **0.87 s wall time** after the change
(0.83 s baseline).

#### [x] READ-013 — Resolver classification repeats external-package fallback

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-project/src/resolver.rs:81-105,109-150`

Not-found bare requests, outside-project paths, and excluded paths each repeat
the same `PackageSpecifier::new(package_name(request))` success/error mapping.
The surrounding branches are correctly different for internal requests, but
the repeated fallback makes changes to package classification or its error
wording a multi-site edit.

Extract `external_outcome(request)` (or a similarly named resolver-owned
helper) and use it from the not-found, outside, and excluded branches. Keep
internal/external policy at the branch sites so the helper only owns the
shared package conversion.

**Implementation:** Centralized external package classification in the
resolver-owned `external_outcome` helper while retaining the distinct
not-found error context and internal-request branches. The release lint of
`/home/lemon/src/obsidian-stats/data/out/plugin-release-mainjs/byoc/1.0.13-main.js`
took **0.88 s wall time** after the change (0.83 s baseline).

#### READ-014 — Pretty rendering uses manual interior-mutability caching

- **Severity:** Low
- **Fix Complexity:** Medium
- **Category:** Encapsulation / Complexity
- **Location:** `glass-lint-output/src/report/types.rs:31-120`; `glass-lint-output/src/report/render.rs:83-117`

`PrettyFile` owns a `RefCell<BTreeMap<usize, Vec<Cell>>>`, while
`PrettyReport` has separate cached and uncached constructors. `excerpt` first
borrows to check the map, borrows mutably to populate it, then borrows again
to render; the cache is an implementation detail that leaks into the renderer
type and requires careful borrow ordering.

Precompute per-line cells when building `PrettyFile`, or use a per-line
`OnceLock`/immutable cache representation that can be shared by renderers.
Collapse `new` and `new_with_cache` around one cache abstraction and keep
source excerpt formatting independent from cache ownership.

### Tests and documentation hygiene

#### [x] READ-015 — Query-composition tests contain obsolete failure claims

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Testing / Documentation
- **Location:** `glass-lint-core/tests/integration/query/composition.rs:1-7,32-39,67-74,109-115,138-145`

The module says its tests “currently fail,” attributes behavior to unfinished
packages, and references `q-fix.md`, which is not present in the workspace.
The targeted test currently passes all 29 composition tests, including the
cases described as expected failures. These comments now mislead maintainers
about the status and intended contract of the query compiler.

Rewrite the module-level and test-level comments to describe the behavior
actually being guarded, remove the missing-document reference, and retain
historical design context only in a real linked decision record if it is still
needed.

**Implementation:** Rewrote the composition test comments to describe the
currently enforced branch, conjunction, lifecycle, contradiction, and bounded
input contracts, removed the missing `q-fix.md` reference and obsolete
package-status claims, and renamed the misleading empty-conjunction test. The
requested bundle’s release lint took **0.84 s wall time** after the change
(0.83 s baseline).

#### READ-016 — Large test modules mix fixture construction with assertions

- **Severity:** Low
- **Fix Complexity:** Medium
- **Category:** Testing / Readability
- **Location:** `glass-lint-project/src/tsconfig/tests.rs:1-1028`; `glass-lint-core/src/api/compiler/tests/validate.rs:1-1012`; `glass-lint-core/src/api/compiler/tests/normalize.rs:1-958`; `glass-lint-core/tests/integration/matching/declarative.rs:1-1162`

The largest test files repeatedly hand-build raw JSON, `QueryDecl` values,
`EmissionDecl` values, and full source snippets. The behavior under test is
often only one field or one diagnostic, but fixture plumbing dominates the
local context and makes related cases harder to compare. A few table-driven
tests also use long boxed closures or large inline cases to cover constructor
errors.

Add narrowly scoped factories/builders with explicit defaults, split
independent scenario families into submodules, and use small data tables for
repeated invalid inputs. Keep each assertion specific to its semantic
boundary; do not hide the adversarial source snippets that document matching
precision.

## Systemic Themes

1. **The project has good phase names but several coordinators still own the
   transitions between phases.** Flow, scope collection, project loading, and
   evidence assembly would benefit most from typed contexts and explicit
   termination states.
2. **Raw representation details are resurfacing at abstraction boundaries.**
   Tagged path integers, nested rule-indexed vectors, parallel cache maps, and
   deferred error slots make invariants depend on caller discipline.
3. **Recent migrations are incomplete in a few areas.** The core already has
   iterator-based rule registration and some evidence/history owners, but
   neighboring code still uses the older repeated forms.
4. **The testing foundation is strong but its maintenance signal is noisy.**
   Deterministic adversarial coverage is valuable; stale roadmap comments and
   oversized fixture-heavy modules make it harder to tell which behavior is
   current and where a new regression belongs.

## Decisions and follow-up

1. Use a generic parent-linked history in `glass-lint-datastructures`, but
   keep domain deltas, budgets, and transactional commit semantics in core.
2. Keep path identity typed at the storage boundary and continue narrowing
   the summary overlay capability; the next cleanup would wrap
   `append_linked` so callers cannot construct overlay nodes directly.
3. Preserve existing fluent catalog ergonomics while offering strict `try_*`
   operations for callers that can propagate authoring errors.

## Coverage

Reviewed the root and owning-crate architecture documents, testing and
contribution guidance, all Rust workspace crates, the existing audit, the
largest core/project/flow/harness modules, datastructure APIs, provider
catalogs, output rendering, and representative unit/integration tests.

Validation performed against the current tree:

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  passed.
- `cargo test -p glass-lint-core --test integration query::composition`
  passed: 29 tests.
- `cargo test -p glass-lint-project tsconfig` passed: 43 tests.
- `cargo test -p glass-lint-datastructures path_trie` passed: 40 tests.
- `cargo test --workspace` passed after the refactor.
- `cargo test -p glass-lint-core summary::store` passed: 16 tests.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  passed after the refactor.

The implementation changes are limited to the shared history/path APIs and
rule-builder ergonomics described above; no matching policy or external
configuration was changed.
