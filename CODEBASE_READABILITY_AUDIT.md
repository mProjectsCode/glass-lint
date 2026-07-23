# Glass Lint Core and Project Readability Audit

Audit date: 2026-07-23

Scope: every Rust source and test file in `glass-lint-core` and
`glass-lint-project`, with emphasis on hot-path performance, simplification,
architecture, and ownership.

## Summary

The two crates have unusually strong correctness foundations: semantic
identities are typed, incomplete analysis generally fails closed, output is
normalized deterministically, and the provider/project boundary is clean.
The principal risk is scale. Several bounded algorithms enforce a limit on
the wrong quantity, while some of the hottest operations repeat recursive
resolution, allocate equivalent paths, or rescan whole fixed-point state.

This audit records 37 findings: 18 high severity, 15 medium severity, and 4 low
severity. 10 findings have been addressed (1 high, 5 medium, 4 low). The highest-priority
changes remaining are:

1. make project admission limits global and count unique files;
2. parallelize local lowering in `ProjectLoader`;
3. bound configuration traversal independently of directory traversal;
4. eliminate repeated member-provenance, export, and cross-flow resolution;
5. make retained-state limits account for all retained state, not only the
   pending frontier; and
6. use the permitted clean break to replace the duplicated project API and
   stringly identity contracts with one validated semantic surface;
7. separate transient linker state from the final project model; and
8. limit serde to intentional configuration and output contracts behind an
   opt-in core feature.

The findings are intentionally not marked “done” based on historical edits.
Each item below was revalidated against the current source.

## Findings

### Project discovery, admission, and loading

#### READ-001 — File limits are local to traversals and count attempts rather than unique files

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/corpus.rs:93-123`, `glass-lint-project/src/discovery.rs:87-114`, `glass-lint-project/src/discovery.rs:124-203`, `glass-lint-project/src/loader.rs:308-330`, `glass-lint-project/src/loader.rs:528-543`

`SourceCorpus::discover_filtered` creates a new `FileBudget` for each root and
does not charge explicit file roots. Tsconfig references recursively perform
fresh directory walks and validate the union only after all referenced
projects have been traversed. `AdmissionSet::admit` charges its budget before
the `BTreeSet` insertion, so repeated imports of an already admitted file can
exhaust `max_files` while the project contains fewer than `max_files` unique
files. The final corpus-size check is only a `debug_assert`, so release builds
can return an over-limit corpus.

Define `max_files` as the maximum number of unique, admitted files for one
top-level operation. Make the admission set own the budget and use the entry
API: reject a vacant insertion when the set is at its limit, but do not charge
an occupied entry. Share that set through roots and referenced configs and
stop traversal immediately on exhaustion. Track attempted edges separately
if the metric is useful. Add boundary tests for duplicate imports, overlapping
roots, overlapping tsconfig references, explicit file roots, and exactly-at-
limit admission.

#### READ-002 — SourceCorpus does not establish one project containment boundary

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/corpus.rs:75-143`

Corpus construction stores options but no canonical admission authority.
Discovery creates a `SourceAdmission` from each caller-supplied root, ignoring
the configured `options.root`. Loading uses the configured root when present
and otherwise uses each file's parent. Discovery and loading can therefore
disagree about whether a path is inside the project, and a multi-root corpus
can apply a different containment boundary to every file. Loading also
canonicalizes the root repeatedly.

Make corpus construction establish one canonical `SourceAdmission` and return
a typed error if it cannot. Require discovery roots to be inside that
authority. If no root is configured, require the caller to provide one
top-level root when constructing the corpus rather than deriving authority
per file. Reuse the same admission object for discovery and loading.

#### READ-003 — ProjectLoader serializes the per-file parse and lowering hot path

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:447-519`, `glass-lint-core/src/project/session/mod.rs:269-345`

`ProjectLoader` pops one path, reads it, and calls
`session.analyze_source(source)` before advancing the queue. This serializes
parsing, scope construction, fact building, indexing, and local matching for
directory and tsconfig projects even though the core session already exposes
deterministic bounded parallel analysis through `analyze_sources`.

Process the current import frontier in bounded waves: read a bounded batch,
pass it to `ProjectCollection::analyze_sources` with the validated worker
count, sort returned requests deterministically, resolve them, and build the
next wave. Preserve the existing deadline, cache, partial-report, and source-
byte semantics, and cap the wave size independently of total files so
parallelism does not create an unbounded memory spike.

#### READ-004 — Tsconfig inheritance and references have no structural traversal budget

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig/mod.rs:525-627`, `glass-lint-project/src/discovery.rs:124-203`

`extends` and project references recurse with an active `Vec`, using
`contains` for cycle detection. The wall-clock deadline is the only bound on
the number and depth of configuration files; `max_visited_entries` applies to
directory entries, not configs. A deep acyclic inheritance chain can consume
quadratic cycle-check work and overflow the Rust call stack before the timeout
is observed. A wide reference graph can compile and retain many configs
before the final membership check.

Introduce a semantic `ConfigTraversalBudget` with maximum config count and
maximum inheritance/reference depth. Use an explicit work stack for reference
graphs and an active set paired with an ordered stack for cycle diagnostics.
Resolve an `extends` chain iteratively and merge it in reverse order. Return a
typed project-load error on structural exhaustion and test below, at, and
above both limits.

#### READ-005 — Directory discovery performs a realpath operation for every entry

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-project/src/walk.rs:52-101`, `glass-lint-project/src/admission.rs:153-171`

The walk predicate classifies every directory and every candidate file, and
classification always calls `fs::canonicalize`. This adds a filesystem
round-trip per entry to the principal discovery hot path. Classification
errors encountered by `filter_entry` are converted to a false predicate,
which also makes an inaccessible directory indistinguishable from an
intentionally excluded one.

With symlink following disabled and a canonical root already established,
prune ordinary excluded directories lexically. Canonicalize only symlink
boundaries and files being admitted, while retaining the existing containment
check for every admitted file. Propagate directory metadata/canonicalization
errors as typed load errors instead of silently pruning. Benchmark on a large
dependency-heavy tree before and after the change.

#### READ-006 — Loading discards validated path identity and reconstructs it from strings

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Newtype
- **Location:** `glass-lint-project/src/admission.rs:211-219`, `glass-lint-project/src/corpus.rs:31-72`
- **Status:** Done — added `SourceFile::from_relative(path: ProjectRelativePath, source: impl Into<SourceText>)` that infers language from the typed path without re-parsing; updated `load_admitted_source_file` to use it

`load_admitted_source_file` reads a `CorpusFile`, discards its path wrapper,
converts the already validated relative path to a `String`, and asks
`SourceFile::new` to validate it again. `read_source_bytes` also clones the
path into the temporary result and grows an initially empty `Vec` even though
file metadata provides a bounded size.

Add a crate-internal `SourceFile` constructor that accepts a proven
`ProjectRelativePath` and validated source text. Factor the bounded byte/text
reader below both `CorpusFile` and `SourceFile`, and reserve at most the
validated metadata length. Keep unchecked construction private so callers
cannot bypass project-relative path validation.

#### READ-007 — Tsconfig test instrumentation runs in production and one test does not test it

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Testing
- **Location:** `glass-lint-project/src/tsconfig/mod.rs:13-24`, `glass-lint-project/src/tsconfig/mod.rs:400-402`, `glass-lint-project/src/tsconfig/tests.rs:102-109`, `glass-lint-project/src/tsconfig/tests.rs:174-194`
- **Status:** Done — counter gated behind `#[cfg(test)]`, uses relaxed ordering, test now resets and asserts

Every effective-config compilation performs a sequentially consistent atomic
increment in non-test builds. The counter is global, so reset-and-read tests
can race when tests run concurrently. The test named
`compile_counter_increments_once_per_effective_config` neither resets nor
asserts the counter.

Compile the counter and increment only under `cfg(test)`. If retained, use
relaxed ordering, isolate the observer per test or serialize counter tests,
and make both named tests assert an exact delta.

### Core local analysis, scope, and matching

#### READ-008 — A cold artifact-cache miss hashes the full source twice

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/local.rs:55-84`, `glass-lint-core/src/analysis/local.rs:234-253`, `glass-lint-core/src/project/session/artifacts.rs:90-94`, `glass-lint-core/src/project/session/mod.rs:218-231`, `glass-lint-core/src/project/session/mod.rs:314-334`
- **Status:** Done — `ArtifactFingerprint` computed eagerly in `ArtifactCacheKey::from_inputs` and stored as a field; `fingerprint()` is now a trivial field access

Cache lookup computes the artifact fingerprint over the complete source,
environment, and limits. A miss carries only the cache key, so insertion
recomputes the same fingerprint after the expensive analysis. Cold-cache
project runs therefore hash every source twice.

Compute the fingerprint once. Carry it in `CacheLookup::Miss` or in a
validated cache-key object and require insertion to consume that object.
Retain the full cache-key equality check so hash collisions cannot produce a
false hit.

#### READ-009 — Scope planning repeatedly materializes member chains and interns unrelated literals

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/collect/plan.rs:200-237`
- **Status:** Partially done — removed redundant full-chain interning from `visit_member_expr`; `visit_lit` string-literal interning retained because constant string resolution for computed properties depends on the name table

For every `MemberExpr`, the planner interns the direct static property, then
constructs the complete member chain and interns every segment again. Because
the visitor also sees each nested member node, a chain of depth `d` performs
quadratic chain construction and repeated hash lookups. The planner also puts
every standalone string literal into the name table, although literal values
belong to the value/literal indexes; computed property strings are already
handled by their owning member/property nodes. This wastes both time and the
bounded name budget.

Intern identifiers and static property names only at their owning syntax
nodes. Remove full-chain and general string-literal eager interning after
adding adversarial tests for computed properties, destructuring, and literal
matchers. The name-planning pass should reserve identities required by the
collector, not mirror every textual string in the AST.

#### READ-010 — Member provenance repeatedly allocates every prefix of every nested chain

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/query/provenance.rs:55-145`, `glass-lint-core/src/analysis/scope/query/provenance.rs:187-211`, `glass-lint-core/src/analysis/scope/query/provenance.rs:461-485`, `glass-lint-core/src/analysis/facts/build/visitor.rs:49-65`

Provenance resolution builds owned `Vec`, `SymbolPath`, and `NamePath` values
for successive prefixes. Rooted-mutation checks allocate receiver prefixes
again. `member_value_seed` materializes the syntactic chain, attempts one
resolution path, may reconstruct it for the fallback, and formats a string
key. Fact building invokes this work for every nested `MemberExpr`. A single
depth-`d` node can do quadratic work; visiting every nested node can make a
minified fluent chain approach cubic path work.

Analyze a member chain once into a canonical borrowed or interned path and
cache prefix results by stable syntax identity/range. Let mutation and
property indexes query borrowed `NameId` slices or a path-trie ID instead of
owned prefixes. Preserve source-position-sensitive mutation checks,
shadowing, reassignment, and fail-closed ambiguity. Add a deep minified-chain
profile and exact-behavior tests before replacing the existing paths.

#### READ-011 — DeclarationFacts eagerly performs seven overlapping analyses even when the result is discarded

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/collect/analysis.rs:50-92`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:97-150`

`DeclarationFacts::compute` eagerly runs callable, module-alias, require,
static-object, constant, returned-object, and rooted-path analysis. “Computed
once” prevents duplicate calls by the views, but each helper can recursively
inspect the same expression. Assignment computes the entire set before
examining its left-hand side: member assignment discards the provenance
result, and pattern assignment only needs a rooted path.

Dispatch first on the assignment target and coarse expression shape. Make
initializer analysis lazy or create one shape analysis whose derived views
share constant/root/callee subresults. Keep precedence explicit and represent
exhaustion separately from “not proven”; add lookalike, alias, reassignment,
destructuring, and dynamic-expression tests around the refactor.

#### READ-012 — ScopeGraph still owns unrelated build, index, mutation, and query state

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/model.rs:59-95`, `glass-lint-core/src/analysis/scope/query`

`ScopeGraph` holds name/environment state, lexical intervals and query cache,
binding/function/assignment indexes, property mutations, dynamic-eval state,
static objects, and validity. Its methods span mutable interning, collection,
freezing, binding history, provenance, and resolution. This makes invariants
such as “indexes are frozen before query” implicit and forces unrelated
features to borrow the same large owner.

Split owned concerns into `NameEnvironment`, `LexicalScopeIndex`,
`BindingIndex`, and `MutationIndex`, then expose a consuming freeze operation
that creates a read-only resolver view. Put queries on the component that owns
their state; keep a small coordinator only for queries that genuinely combine
components. Centralize strict-identity and dynamic-scope checks in that
coordinator rather than duplicating them.

#### READ-013 — Constrained matching prepares paths and allocates evidence inside the occurrence loop

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/arguments.rs:85-192`, `glass-lint-core/src/analysis/matching/arguments.rs:200-250`, `glass-lint-core/src/analysis/matching/occurrence.rs:308-324`, `glass-lint-core/src/analysis/matching/mod.rs:391-416`

Each candidate occurrence converts callee/provenance paths into owned forms
for each clause. The evaluator collects matched occurrences into a `Vec` and
then converts them into another owned evidence collection. Fallback scans
materialize a scanned occurrence vector before filtering. The cost multiplies
with rules, clauses, and occurrences even though clause paths and overlays are
module-constant.

Compile a `PreparedClause` once per module/clause with resolved `NamePath`
inputs and a borrowed argument overlay. Stream accepted occurrences directly
into one evidence accumulator and normalize once. Keep source-order
determinism, deduplication, and the single fallback stream scan for matcher
shapes that lack an index.

### Core project linking and cross-module analysis

#### READ-014 — Export-limit checking makes successful insertion quadratic

- **Severity:** High
- **Fix Complexity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/state.rs:81-118`, `glass-lint-core/src/analysis/project/graph.rs:149-163`
- **Status:** Done — `ExportTable` now owns a `total_entries` counter updated on vacant insertion; `len()` returns it directly instead of summing every module's export map

After each newly changed export, the linker calls `ExportTable::len`, which
sums the lengths of every module export map. Building `E` exports can
therefore perform quadratic aggregate counting, up to a configured limit of
roughly one million entries.

Make `ExportTable` own an exact `total_entries` count updated only on vacant
insertion, or better, expose a single `try_set_monotone(limit, ...)` operation
that owns insertion, transition validation, and the limit. Encode the allowed
resolution-state lattice so “monotone” is enforced rather than only named.

#### READ-015 — The final semantic model retains linker work state and linking clones around its own borrows

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/model.rs:191-214`, `glass-lint-core/src/analysis/project/model.rs:450-466`, `glass-lint-core/src/analysis/project/graph.rs:45-119`, `glass-lint-core/src/analysis/project/state.rs:51-58`

`ProjectSemanticModel` retains the module graph, SCC partition (including an
otherwise unread DAG), and link budget after linking. Only an edge count is
needed later for metrics. During linking, methods clone component order,
whole SCC member lists, and export descriptors to work around borrowing a
single owner for immutable graph and mutable export state. Large projects pay
both transient clone cost and permanent memory for phase-local structures.

Move graph, SCCs, mutable exports/status, and the link budget into a consuming
`ProjectLinker`/`LinkState`. Return a compact semantic model plus final
operation counts. Separate owners permit disjoint borrows without cloning and
make it impossible to call link-only operations on a completed model. Keep the
link budget in the transient linker; it is real enforcement state, not merely
a metric.

#### READ-016 — Graph and SCC-DAG construction use linear duplicate checks before sorting and deduplicating

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/state.rs:21-37`, `glass-lint-core/src/analysis/project/graph.rs:235-238`, `glass-lint-core/src/analysis/project/graph.rs:376-395`
- **Status:** Done — removed `contains` check from `insert_edge`; caller was already ignoring the boolean return value. `normalize` still handles sort/dedup.

Module adjacency calls `Vec::contains` before every insertion even though a
later normalization pass sorts and deduplicates. SCC DAG construction repeats
the same pattern and then sorts. Dense import or re-export graphs can spend
quadratic time per adjacency list; the caller ignores the insertion boolean.

Append edges and perform one deterministic sort/dedup pass. If online
uniqueness is required for budget accounting, use a temporary set or dense
module-ID bitset and state whether the budget counts authored requests or
unique edges.

#### READ-017 — Export and namespace resolution repeatedly traverses star-export graphs without negative memoization

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/exports.rs:108-184`, `glass-lint-core/src/analysis/project/exports.rs:199-285`, `glass-lint-core/src/analysis/project/identities.rs:79-169`

Imported-identity and export lookup create fresh visiting sets and recursively
walk star exports. Explicit positive exports are retained, but missing,
unknown, and ambiguous results are not memoized. Namespace resolution removes
a module from the visiting set on unwind, so diamond graphs revisit shared
subgraphs. Call identity, matcher identity, and cross-flow stages ask many of
the same `(module, export)` questions.

Build a bounded lookup table keyed by `(ModuleId, ExportName)` with explicit
`Resolved`, `Missing`, `Unknown`, and `Ambiguous` states. Treat it as derived
link state, separate from authored exports, and charge new entries to the
link budget. Avoid the temporary matching-request vector. Preserve the
semantic distinction between proven absence and incomplete analysis, plus
default-export and ambiguity rules.

#### READ-018 — Cross-flow resolves the same qualified calls and matcher paths in several phases

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:185-233`, `glass-lint-core/src/analysis/flow/cross/mod.rs:288-337`, `glass-lint-core/src/analysis/flow/cross/propagation.rs:71-192`, `glass-lint-core/src/analysis/flow/cross/propagation.rs:223-260`, `glass-lint-core/src/analysis/project/identities.rs:20-76`

Seeding, return-adjacency construction, propagation, and call-result identity
each resolve qualified call targets independently, feeding the repeated
export traversal above. Property, receiver, and argument propagation also
rebuild requirement and sink `NamePath`s for each usage/context even though
the matcher plan is constant for a module.

Build one bounded `QualifiedEffectGraph` keyed by qualified call event and one
`CrossBoundFlowPlan` per `(module, flow)` before the fixed point. Reuse these
for seeds, return edges, propagation, requirements, and sinks. Preserve stable
IDs, deterministic iteration order, and explicit unknown/incomplete outcomes.

#### READ-019 — Cross-flow bounds the pending frontier but not all retained state

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:99-125`, `glass-lint-core/src/analysis/flow/cross/mod.rs:421-444`, `glass-lint-core/src/analysis/flow/cross/mod.rs:500-546`

`ContextWorklist` keeps every seen `CallContext`, but `len()` reports only the
pending queue; the `MAX_CONTEXTS` check therefore allows the retained seen set
to grow past the advertised limit whenever processing keeps the frontier
small. Source propagation similarly retains `pending_seen` and growing source
sets while limiting only current pending length and iteration rounds. A long,
narrow graph can retain unbounded unique state without tripping the frontier
limit.

Charge a typed budget before every new unique retained context, source key,
and source candidate. Track pending and total-retained counts separately and
return an explicit exhausted outcome before insertion. Keep round limits only
as a secondary convergence guard. Add adversarial narrow-chain and high-
fanout tests with injectable low limits.

#### READ-020 — Function-summary convergence rescans every function and call each round

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:290-310`, `glass-lint-core/src/analysis/flow/summary.rs:423-505`

For up to 64 rounds, summary propagation scans every function and every
function call even when only one callee acquired new sinks. Delta offsets
avoid replaying old sinks but do not avoid inspecting unaffected callers.
`SinkSet` uses linear membership checks while accumulating. Large helper
graphs therefore combine full-graph fixed-point scans with increasingly
expensive deduplication.

Build a reverse call graph and use a deterministic worklist that schedules
only callers of a changed callee. Use ordered or hashed membership during
construction, then emit a sorted stable representation. Retain an operation
budget and round guard so malformed or unexpectedly cyclic inputs still fail
closed.

#### READ-021 — FlowStateTable uses sorted vectors for mutation-heavy keyed state

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:233-379`, `glass-lint-core/src/analysis/flow/projector/state.rs:402-459`

Alias and object-state tables are sorted `Vec`s. Binding performs repeated
binary searches followed by shifting insertion/removal; reverse alias checks
scan the full alias table; `states_for` scans the full state table despite its
object-leading key. Branch joins clone both tables and perform repeated
lookups. The default state limit is large enough for these asymptotics to
matter in flow-heavy generated code.

Give the state to two semantic owners: an `AliasTable` with dense
`ValueId`-to-`ObjectId` lookup plus object reference counts, and an
`ObjectFlowStates` index with object ranges or a keyed map. Preserve
deterministic checkpoint, rollback, mutation-log, and join behavior; benchmark
branch-heavy alias churn rather than selecting a container only from theory.

#### READ-022 — Parameter projection linearly rescans function parameters for each argument

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:620-699`

`parameter_for` finds a copied argument's root and linearly searches the
function's parameter descriptions. Call-argument and returned-parameter
projection invoke this repeatedly, making high-arity or destructured calls
perform roughly `arguments × parameters` lookup work.

Build a per-function `ValueId -> ParameterRef` index once with the function
effect table. Resolve copied roots and perform one lookup. Keep duplicate or
unknown roots fail closed, and test nested destructuring and default
parameters.

#### READ-023 — Cross-flow evidence deduplication scans all previously emitted evidence

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:606-645`
- **Status:** Done — `emit` now uses a `BTreeSet` keyed by `(MatchKind, symbol, event)` per module/rule, making dedup O(log n) per emission and eliminating the quadratic scan of all prior evidence

Each emitted cross-flow occurrence scans existing rule evidence and its
occurrences to detect a duplicate. A rule with many proven cross-module sinks
therefore grows quadratically even though evidence identity already has a
stable tuple of kind, symbol, and event.

Use a per-module/rule `EvidenceAccumulator` with a set keyed by semantic
evidence identity and a deterministic output vector. Reuse it for local and
cross evidence if both obey the same normalization contract, then sort once
at publication.

### API, naming, and documentation

#### READ-024 — has_eval_after tests for a prior eval

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Naming
- **Location:** `glass-lint-core/src/analysis/scope/model.rs:369-378`, `glass-lint-core/src/analysis/scope/query/bindings.rs:119-124`
- **Status:** Done — renamed to `has_prior_eval` in both definition and call site

`has_eval_after(span)` uses `partition_point` to determine whether an eval
ends before the supplied span; its caller and comment both interpret it as a
prior eval. The current behavior is correct, but the opposite name invites a
future condition inversion in strict-identity logic.

Rename it to `has_prior_eval` or `has_eval_before` and add boundary tests for
eval before, overlapping, and after the queried use.

#### READ-025 — A public ProjectCollection constructor requires a state external callers cannot construct

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** API
- **Location:** `glass-lint-core/src/project/mod.rs:14-18`, `glass-lint-core/src/project/session/mod.rs:42-67`, `glass-lint-core/src/project/session/mod.rs:156-172`
- **Status:** Done — `SessionState` changed to `pub(crate)` and its `pub use` export removed; `ProjectCollection::new` kept public because `glass-lint-project` still uses it

`ProjectCollection::new` is public and requires `SessionState`, but
`SessionState::new` and its fields are crate-private. The meaningful public
construction path is `Linter::begin_project`; the exposed constructor is not
usable by an external caller and unnecessarily publishes an internal phase
type.

Delete the public `SessionState` export and make the constructor crate-private
in the clean break. Rename the caller-facing type to `ProjectSession` if it
remains the canonical staged API, constructed only by
`Linter::begin_project`; do not expose engine storage or retain an unusable
constructor for compatibility.

#### READ-026 — Lowering documentation says one pass while the implementation deliberately traverses the AST three times

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/lowering.rs:196-199`, `glass-lint-core/src/analysis/lowering.rs:300-320`, `glass-lint-core/src/analysis/scope/mod.rs:46-59`
- **Status:** Done — `lower_source` doc now describes "three sequential passes: scope planning, collection against the plan, and fact building against the frozen resolver"

The lowering documentation says scopes, facts, and indexes "all happen in one
pass." Current lowering runs a scope-planning traversal, a collection
traversal against the plan, and a fact-building traversal against the frozen
resolver. The separation is defensible—it enables hoisting and strict
identity—but the inaccurate claim obscures the fixed per-file performance
cost and the reason for the phases.

Document the three semantic passes and their invariants. Do not combine them
solely to satisfy the old comment; only fuse work proven redundant by
profiling, while preserving planned-scope validation and the frozen resolver
boundary.

### Public API and serialized contracts

#### READ-027 — Core exposes the project contract through duplicate root and nested namespaces

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/lib.rs:13-57`, `glass-lint-core/src/project/mod.rs:8-27`

Project types are reachable both as root exports and through the public
`project::{input, types}` tree, while rule types are split among root exports,
`rules`, and a `RuleBuilder as Builder` alias. This preserves several import
styles but makes every re-export path part of the contract and exposes
implementation-oriented modules such as `project::input`.

Use the clean break to choose one canonical namespace per domain:
`project`, `rules`, `report`, and `config`, with a deliberately small crate
root. Remove duplicate re-exports and the generic `Builder` alias; export
builders by their semantic names. Keep module layout independent from public
namespace layout so future internal moves do not create API churn.

#### READ-028 — ProjectInput is a legacy bulk adapter alongside the canonical staged session

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/types/input.rs:132-139`, `glass-lint-core/src/project/input.rs:15-119`, `glass-lint-core/src/lint/linter.rs:176-236`, `glass-lint-core/src/project/session/mod.rs:69-493`

The core exposes both a staged collect/analyze/resolve API and
`Linter::lint_project(ProjectInput)`. The bulk adapter repeats source and
resolution budgets, normalization, and admission. `ProjectInput::validate`
builds a second `ValidatedProjectInput` representation, but `lint_project`
does not use it because only the staged pipeline can validate outcomes against
authored requests. No non-test workspace caller uses the bulk path.

Delete `ProjectInput`, `ValidatedProjectInput`, their conversion, and
`lint_project` in the clean break. Make the type-state session the one
in-memory project contract and keep `lint_snippet` as the sole convenience
adapter. If a future wire protocol needs bulk project input, define it at the
adapter boundary and translate directly into the staged API rather than
reintroducing an engine DTO.

#### READ-029 — SourceFile can represent a path, language, and source combination that was never validated

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/project/types/input.rs:7-61`

`SourceFile` has public fields and derives deserialization, so callers can
bypass `SourceFile::new`, provide an escaping path, or select a language that
contradicts the filename. Its constructor accepts strings, reparses a
`ProjectRelativePath`, and always infers language, which also forces trusted
filesystem callers to discard semantic path identity.

Make the fields private and accept `ProjectRelativePath` plus `SourceText` in
the primary constructor. Infer language once from the typed path; provide an
explicit named constructor for virtual or extensionless sources that require
a language override. Remove direct deserialization with the rest of the
operational input DTOs, and expose borrowing accessors rather than mutable
storage.

#### READ-030 — Module-resolution identities remain plain strings across the public boundary

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Newtype
- **Location:** `glass-lint-core/src/project/types/input.rs:73-124`, `glass-lint-core/src/project/types/mod.rs:27-29`, `glass-lint-core/src/project/input.rs:133-218`

`ResolutionRequest.request`, external package names, builtin names, outside-
project paths, and linked targets use unrelated `String` fields. Their
grammars and normalization are enforced by free functions after construction,
and `is_internal_module_request(&str)` repeats classification over an
unvalidated string. This weakens the strict-identity boundary and makes
internal/package/builtin/path values interchangeable at call sites.

Introduce semantic types such as `ModuleRequest`, `PackageSpecifier`,
`BuiltinModuleName`, and `NormalizedOutsidePath`. Put parsing,
classification, boundary-aware package behavior, and normalization on those
types; let `ResolverOutcome` carry them directly. Human-readable unsupported
reasons can remain strings because they are diagnostics, not identities.

#### READ-031 — Rule declarations accept several semantic grammars as deferred-validation strings

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Newtype
- **Location:** `glass-lint-core/src/api/rule/taxonomy.rs:3-47`, `glass-lint-core/src/api/rule/module.rs:7-83`, `glass-lint-core/src/api/rule/decl.rs:140-642`, `glass-lint-core/src/api/rule/matcher/flow.rs:145-570`

`Category::new` and its `From<String>` implementations create invalid
categories, `ModuleSpecifierPattern::{exact, package}` create values whose
validation is crate-private and deferred, and matcher/flow builders accept
rooted chains, property names, package roots, exports, and evidence symbols as
interchangeable strings. Catalog compilation eventually rejects many bad
shapes, but public declaration values do not uphold the invariants their names
imply.

Use fallible constructors for standalone semantic values and types such as
`RuleName`, `Category`, `ModuleSpecifierPattern`, `RootedSymbolPath`,
`PropertyName`, and `EvidenceSymbol`. Builder methods may remain ergonomic by
accepting `TryInto` values, but they should validate at the method/build
boundary and store the semantic type. Remove infallible `From<String>` impls
that can create invalid states.

#### READ-032 — ObjectFlowMatcherBuilder returns an invalid matcher and defers failure to catalog construction

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/matcher/flow.rs:224-427`, `glass-lint-core/src/api/rule/matcher/flow.rs:476-570`

Empty alternatives, missing sources/completion, and duplicate
`configured_by`/`complete_at` operations are stored in an
`ObjectFlowMatcher` with an `invalid_operation` side channel.
`ObjectFlowMatcherBuilder::build` cannot fail, so an object named “matcher”
may remain invalid until a containing catalog is compiled.

Make `build` return `Result<ObjectFlowMatcher, MatcherBuildError>` and validate
nested event/condition/sink values before returning. Remove
`invalid_operation` from the finished type and update all provider callers in
the same clean break. A validated declaration should never need a later
compatibility validation pass.

#### READ-033 — glass-lint-project exports filesystem mechanisms rather than one loading contract

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** API
- **Location:** `glass-lint-project/src/lib.rs:10-28`, `glass-lint-project/src/admission.rs:22-224`, `glass-lint-project/src/corpus.rs:18-143`

The crate root exports `CanonicalProjectPath`, `SourceAdmission`,
`CorpusFile`, and `SourceCorpus` beside the high-level `ProjectLoader`.
`FileBudget`, `PathAdmission`, discovery, and resolver types also use broad
`pub` visibility inside private modules. The low-level corpus is currently
used by CLI snippet mode and profiling, but it exposes the same root and
admission inconsistencies described in READ-001/002 as a supported contract.

Keep `ProjectLoader` plus validated selection/policy types as the primary
public API. Replace corpus use with a focused high-level
`discover_sources`/`load_source` service whose constructor establishes one
root authority, or move profiling-only discovery to the harness. Make
canonical paths, admission outcomes, budgets, raw readers, discovery, and
resolver machinery `pub(crate)`; do not retain aliases for the old exports.

#### READ-034 — Project loading exposes both unchecked and checked option types with duplicated behavior

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** API
- **Location:** `glass-lint-project/src/options.rs:18-104`, `glass-lint-project/src/options.rs:106-332`

`ProjectLoadOptions`, its builder, and `ValidatedProjectLoadOptions` are all
public. The unchecked type has public `validate`, `validated`, `supports`, and
`excludes_path` behavior, while the checked type repeats support/exclusion
accessors. This makes it possible to write substantial caller logic against
policy that has not passed cross-field validation and creates two owners for
extension matching.

Expose one finished `ProjectLoadPolicy` with private validated fields and one
builder whose `build` returns it. Keep any raw deserialization shape private
to the CLI and convert it through the builder. Delete the unchecked public
type and duplicated methods rather than preserving `validated()` as a
compatibility path.

#### READ-035 — Serde is mandatory and implemented on operational and intermediate engine types

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/Cargo.toml:7-24`, `glass-lint-core/src/api/classification.rs:23-99`, `glass-lint-core/src/project/types/input.rs:7-139`, `glass-lint-core/src/parse.rs:21-39`, `glass-lint-core/src/project/types/report.rs:1-307`

Core requires serde for every consumer, and serialization traits appear on
session inputs, source text, resolver outcomes, parser data, classification
intermediates, configuration, metadata, and final reports without one stated
wire boundary. Several intermediate classification fields are skipped, so
their serialized form is lossy; the workspace's adapter protocol already
uses its own DTOs and does not serialize core project inputs.

Make serde an optional core feature enabled explicitly by CLI and harness
crates. Under that feature, support deserialization for configuration inputs
and serialization for final reports/rule metadata only. Remove serde from
source/session/resolver and classification-intermediate types. Keep
`serde_json` mandatory in `glass-lint-project` because parsing tsconfig JSONC
is runtime project functionality, not optional API serialization.

#### READ-036 — Output reports are freely mutable and deserializable without an import contract

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/types/report.rs:112-307`, `glass-lint-core/src/project/tables.rs:19-61`, `glass-lint-core/src/project/report/mod.rs:31-100`

The complete report tree derives `Deserialize` and exposes public fields, so
callers can construct zero-count evidence, inconsistent completion, duplicate
files, arbitrary operation counts, or a report claiming any schema/tool
version. Production workspace code only serializes reports; deserialization
exists for round-trip tests. `AnalysisReport::combine` then has to defend
against states the engine itself never creates.

Treat `AnalysisReport` and children as engine-produced output values: private
fields, internal constructors, read-only accessors, and consuming
`into_parts` methods where ownership is useful. Retain `Serialize` behind the
serde feature and remove `Deserialize`. If report ingestion becomes a real
feature, add an explicitly versioned validating reader rather than deriving
construction for the domain model.

#### READ-037 — The report schema carries unused compatibility scaffolding and primitive identities

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/lib.rs:47`, `glass-lint-core/src/diagnostic.rs:393-407`, `glass-lint-core/src/project/types/report.rs:118-159`, `glass-lint-core/src/project/types/report.rs:251-307`, `glass-lint-core/src/lint/catalog.rs:135-148`, `glass-lint-core/src/lint/report.rs:245-271`, `glass-lint-core/src/project/report/mod.rs:7-79`

Every rule advertises one hard-coded `"detected"` message and every finding
stores the same `message_id`, so the message map/ID pair is a compatibility
shape rather than a modeled capability. Schema and tool versions are mutable
`u32`/`String` fields, combining rejects different tool builds even when the
schema matches, and serialized operation counters use platform-sized
`usize`.

Delete `message_id` and the singleton `messages` map unless multiple message
templates are implemented now; expose one canonical finding message and rule
description. Rename the constant/type to `ReportSchemaVersion`, use an
immutable `ProducerInfo` value, and combine reports by schema compatibility
rather than exact tool build. Use `u64` or semantic counter newtypes in the
wire shape, with stable snake-case enum spellings and explicit optional-field
rules.

## Systemic Themes

- **Budgets must own the state they bound.** File, context, source, export, and
  configuration limits should be checked by the collection that retains the
  unique item. Frontier size, insertion attempts, and post-hoc aggregate
  checks do not provide the intended memory bound.
- **Resolve canonical identities once per phase.** Member chains, exports,
  qualified calls, matcher paths, artifact fingerprints, and parameter roots
  are repeatedly reconstructed. A bounded phase-local cache or compiled plan
  is both faster and easier to reason about than repeated string/path
  materialization.
- **Fixed points need delta-driven worklists.** Export linking, function
  summaries, and cross-flow already have stable semantic keys. Scheduling only
  dependents of changed keys makes convergence and operation accounting
  explicit.
- **Transient state should be consumed.** Configuration traversal stacks,
  linker graphs/SCCs/budgets, and mutable flow accumulators should not survive
  into final public models. Consuming phase types make ordering invariants
  compiler-visible and reduce retained memory.
- **Optimize without weakening strict identity.** Every proposed cache and
  index must retain shadowing, reassignment order, ambiguity, dynamic-scope,
  and exhaustion distinctions. Performance work should share proven
  identities, never invent fallback identities.
- **A clean break should remove paths, not add adapters.** The duplicate bulk
  project DTO, unchecked loading options, low-level filesystem exports, root
  re-export aliases, singleton message IDs, and deferred-invalid matcher
  objects should be removed in one workspace-wide migration.
- **Serde is a boundary capability, not a property of every public type.**
  Config files are inputs and reports are outputs. Operational session values
  and intermediate semantic models should remain ordinary Rust types unless a
  supported wire protocol specifically requires them.

## Resolved Decisions

1. **`max_files` counts unique files across the complete top-level load or
   discovery operation.** It does not reset per root/config and duplicate
   import attempts do not consume it. Edge-attempt metrics are separate.
2. **Configuration traversal gets its own structural limits.** A wall-clock
   deadline remains useful but is not a substitute for maximum config count
   and depth.
3. **ProjectLoader should use bounded parallel frontier waves.** This reuses
   core's existing deterministic parallel analysis while preserving import
   discovery order and memory bounds.
4. **`SpanNormalizer` remains at the parser-to-domain boundary.** The current
   compact `CharBoundaryMap` makes the defensive UTF-8 boundary check cheap
   and protects the `ByteRange` invariant.
5. **`FunctionTable::get_disjoint` remains a `split_at_mut`-based internal
   operation.** It is safe, encapsulated, and directly expresses the
   fixed-point need for one readable and one writable function. A session-
   token abstraction would add complexity without a stronger invariant.
6. **The link budget remains real enforcement state but moves into a
   transient linker.** The completed semantic model retains only final
   operation counts.
7. **The two provenance path representations remain distinct.** `NamePath`
   represents artifact-local, arena-validated paths; `SmolStr` represents
   cross-artifact module/global identity. Document and enforce that boundary
   rather than forcing both into one storage model.
8. **Timeout behavior remains cooperative.** Recheck after large opaque
   phases when deciding whether to publish a complete result, but do not
   promise hard preemption without worker/process isolation.
9. **The three local AST passes remain separate unless profiling justifies a
   targeted fusion.** Their planned-scope and frozen-resolver boundaries carry
   correctness value; the immediate fix is accurate documentation and removal
   of redundant per-node work within the passes.
10. **The staged project session is the sole project-analysis API.** Remove
    the bulk `ProjectInput`/`ValidatedProjectInput`/`lint_project` path and
    update all callers without a compatibility adapter.
11. **Breaking public and serialized changes happen as one clean migration.**
    Remove old exports, aliases, constructors, fields, and schema members;
    update every workspace caller, fixture, and snapshot in the same change.
12. **Identity-bearing public values use semantic types.** Paths, module
    requests, package/builtin names, rule/category names, rooted chains,
    properties, evidence symbols, and schema versions validate at
    construction. Human-readable messages and unsupported reasons remain
    strings.
13. **Core serde support becomes opt-in.** The supported feature covers config
    deserialization and report/rule-metadata serialization. Operational
    project inputs and semantic intermediates do not implement serde.
14. **Reports are output-only domain values for now.** Remove report
    deserialization and public mutation. A future reader must be explicitly
    versioned and validate into the domain type.
15. **The report schema is simplified now.** Remove the always-`"detected"`
    message ID/map, use semantic schema/counter types, and do not reject
    combining reports solely because producer tool versions differ.

### Serde contract matrix

| Type family | Serialize | Deserialize | Rationale |
| --- | --- | --- | --- |
| `CoreConfig`, `AnalysisLimits`, rule selection | No | Yes | Accepted from JSON/TOML configuration; the engine does not emit config. |
| `AnalysisReport` and report children | Yes | No | Stable machine output; there is no supported report-import workflow. |
| `RuleMetadata` and its semantic field types | Yes | No | Emitted by the CLI rules command. |
| `SourceFile`, `SourceText`, requests, resolver outcomes, session phases | No | No | Operational Rust API, not a wire protocol. |
| Classification and semantic-analysis intermediates | No | No | Internal state; current serialization is lossy because semantic fields are skipped. |
| Tsconfig parsing inside `glass-lint-project` | Internal JSON input | Internal JSON input | Runtime JSONC parsing remains mandatory and is independent of core's public serde feature. |

All core serde implementations in the first three rows are enabled by one
opt-in `serde` feature. If report ingestion is implemented later, add a
separate versioned reader feature instead of silently adding `Deserialize` to
the output model.

## Open Questions

None. The design choices encountered during this audit are resolved above.

## Coverage

The audit inspected all 129 Rust source files (36,818 lines) and 11 Rust
integration-test files (4,227 lines) in `glass-lint-core`, plus all 12 Rust
source/test files (3,298 lines) in `glass-lint-project`: approximately 44,343
lines total. It also reviewed the repository and crate architecture documents,
`TESTING.md`, `CONTRIBUTING.md`, and the repository agent guide. The clean-
break revision additionally traced public exports, every core serde
implementation, Cargo feature declarations, and all workspace callers that
serialize or deserialize these types.

Validation on 2026-07-23:

- `cargo clippy -p glass-lint-core -p glass-lint-project --all-targets -- -D warnings`
- `cargo test -p glass-lint-core -p glass-lint-project`

Both commands passed. No Rust source, test, configuration, dependency, or
documentation file other than this audit report was modified.
