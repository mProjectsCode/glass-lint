# Codebase Readability Audit

## Summary

The audit found 50 readability and maintainability issues across `glass-lint-core` and `glass-lint-project`: 6 high, 28 medium, and 16 low severity. The most important theme is that several nominal pipeline boundaries do not carry sufficiently rich typed state, so later phases reconstruct, revalidate, or clone information that an earlier phase already had; this both obscures the lint lifecycle and increases memory churn. Other recurring themes are parallel representations of the same concept, collection APIs that make borrowing difficult, and duplicated semantic branching that can drift.

## Findings

### READ-001 — Scope-collection issues are silently discarded

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/lowering.rs:243-246`

Lowering ignores every `ScopeCollectionIssue`, even though `ScopedProgram` explicitly documents those values as conservative shape diagnostics for callers to translate into analysis status. They are not encoded as unreachable invariants, so convert them into the artifact's conservative status or make scope collection return a typed valid/incomplete outcome.

### READ-002 — The resolved-project phase reconstructs already validated request state

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/model.rs:84-153`

`ValidatedProjectInput` is consumed only for the constructor to re-enumerate authored requests, rebuild IDs, count exports, and recheck resolution coverage already checked by `LocallyAnalyzedProject::resolve`. Carry a typed request table and validated module metadata across the local-to-resolved boundary so linking consumes the exact state produced by resolution instead of recreating it.

### READ-003 — A legal tsconfig value doubles as the extends-cycle sentinel

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig.rs:496-526`

Recursive config construction represents a cycle by returning a synthetic config containing `files: []`, and its caller recognizes that value as the cycle marker. A legitimate parent config with an explicitly empty `files` list is therefore indistinguishable from failure; return a dedicated `Built`, `Cycle`, or error outcome instead.

### READ-004 — Tsconfig inheritance loses absent-versus-empty state

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig.rs:276-324`

Include and exclude fields are collapsed with `ok().unwrap_or_default()` before inheritance is applied, so an explicit empty array and an absent field become the same value despite the parser having modeled their distinction. Preserve the parsed field state through merging and decide inheritance from presence, not collection emptiness.

### READ-005 — Cross-function flow deduplication forgets completed contexts

- **Severity:** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:99-121`

`ContextWorklist` removes a context from `seen` when it is popped, making the set a queued-only deduplicator rather than a record of processed states. `CallContext` contains every input used by projection and accumulated evidence does not feed back into it, so replaying an identical context cannot refine the result and can re-enqueue call cycles until exhaustion; retain a permanent processed set, as the neighboring source-propagation worklist does.

### READ-006 — Branch checkpoints clone complete flow state at every join

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:312-357`

Although checkpoints themselves are described as cheap, joining branches clones the full reachable-alias and state tables for every branch before intersecting them. A streaming accumulator or persistent state representation would make the branch/join phase explicit and substantially reduce peak memory for wide or deeply nested control flow.

### READ-007 — `EvidenceList` exposes inconsistent views and allocation-heavy traits

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/tables.rs:20-118`

The collection's `len`, indexing, and iteration cover shared and local evidence, while `as_slice` exposes only local evidence; equality and serialization additionally allocate temporary `Vec<&Evidence>` values, and owned iteration clones shared content. Rename or remove the partial slice view and implement traits directly over a unified iterator or a storage model with one coherent borrowed view.

### READ-008 — Artifact fingerprints allocate source-sized temporary buffers

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/local.rs:47-77`

Fingerprinting concatenates source text and all inputs into a temporary byte vector before hashing, and environment fingerprinting follows the same buffer-building pattern. Feed fields into a deterministic streaming hash sink so cache-key computation does not temporarily duplicate large source files.

### READ-009 — Matcher projection plans are rebuilt for every module

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:99-157`

Each module independently flattens selected rules into new constrained-clause and flow-matcher vectors even though the selection is constant for the whole match run. Compile one borrowed `ProjectionPlan` before the module loop and reuse it, making rule selection a visible phase and avoiding repeated allocation.

### READ-010 — Module identity has two isomorphic representations

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/model.rs:58-72`

`ExportResolution` and `LinkedModuleIdentity` carry the same six variants and are converted one-for-one during matching. Use one domain identity type, or expose a narrow borrowed view where a layer-specific API is required, to remove conversion code and future variant synchronization.

### READ-011 — Matcher declarations mix unvalidated input with compiled state

- **Severity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/decl.rs:651-713`

Module documentation, tests, and catalog construction establish the catalog as the intentional matcher validation-and-compilation boundary, but `MatcherDecl` and its builder are documented as validated while performing only partial checks and storing compiler IR plus a precompiled object flow. Treat declarations consistently as unvalidated source data and compile flows only at the catalog boundary, so names and types match the established lifecycle.

### READ-012 — Final analysis coordination is hidden in a positional timing tuple

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/linter.rs:243-300`

One method links modules, mutates parse status, matches rules, handles exhaustion, assembles reports, measures phases, and returns two durations positionally beside the report. Split report assembly from phase execution and return a named result/timing type so the link, match, and report boundaries remain visible to `glass-lint-project`.

### READ-013 — Validated project paths are repeatedly normalized and allocated

- **Severity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/project/input.rs:39-87`

The input validator creates normalized path newtypes, but collection admission and resolution normalize the same strings again and rebuild ownership; validation also creates a cloned set where map membership is sufficient. Let the typed admitted path be the phase currency and keep raw-string normalization at the public boundary.

### READ-014 — Authored requests are projected and copied through multiple layers

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/module.rs:320-340`

`ModuleInterface` and `ProjectModule` independently convert authored requests, and artifact recording then clones request keys and values into an index while retaining the original vector. Give request projection one owner and pass stable IDs or shared records through local analysis, resolution, and linking.

### READ-015 — Pending project sources are cloned to cross an internal borrow boundary

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/session.rs:617-645`

Local analysis snapshots every pending `SourceFile` and path into a new vector because source storage and artifact mutation cannot otherwise be borrowed together. A draining work queue or phase-owned pending batch could move the work items and make the collection-to-local-analysis transition explicit.

### READ-016 — Lowering clones the complete name table during artifact assembly

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/lowering.rs:285-291`

The resolver can only yield its value table, so lowering clones `NameTable` before consuming the resolver. Add an owned `into_parts`/freeze operation that moves both tables into the semantic artifact and encodes the end of resolution as a single transition.

### READ-017 — Function-effect extraction scans the entire fact stream twice

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:372-425`

Collection first traverses all facts to initialize function slots and then traverses them again to record effects, although function-enter facts precede their owned events. Create function/program builders while performing one ordered pass, which also makes ownership of each effect more apparent.

### READ-018 — Flow-summary mutation uses unsafe borrowing and compensating clones

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/table.rs:45-76`

`FunctionTable` uses raw pointers to obtain disjoint mutable entries, while summary construction clones function IDs and each summary's call list to work around adjacent borrowing constraints. A safe disjoint-access API plus narrower field-level methods would remove the unsafe block and allow callers to borrow paths, summaries, and indexes without staging clones.

### READ-019 — Effective-call-argument semantics are implemented in several places

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:219-233`

Normal calls versus `.call`/`.apply` argument interpretation is repeated in the fact stream, flow effects, and call-fact construction. Centralize it in a canonical borrowed `CallView` so every downstream phase sees the same callee and effective arguments.

### READ-020 — `NamePath` and `SymbolPath` duplicate the same path algebra

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/value/identity.rs:35-295`

Both types independently implement root, first/last, append, removal, binding, and descendant operations, while conversions exist both on the path type and `NameTable`. Share an internal path container/algorithm layer and keep name-to-symbol conversion on the table that owns resolution knowledge.

### READ-021 — Assignment-history operations expose repeated nested-map mechanics

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/scope/model.rs:264-370`

Latest assignment, binding version, and reassignment-range queries repeat nested lookups and `partition_point` logic against raw maps and vectors. Encapsulate that storage in an `AssignmentHistory` domain collection with named `latest_at`, `version_at`, and `changed_between` operations.

### READ-022 — Scope freezing combines too many representation transitions

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/collect/mod.rs:284-393`

`freeze` validates structural state, clones issues, allocates two ID spaces, remaps function aliases, builds multiple indexes, constructs the graph, and post-processes properties in one method. Move those steps into cohesive owned builders and move, rather than clone, collector state to make the mutable-collection-to-immutable-graph boundary reviewable.

### READ-023 — Module-interface extraction is a second policy-heavy subsystem inside fact building

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/build/interface.rs:28-512`

The visitor contains repeated branches for dynamic import versus `require`, default declarations versus expressions, and function versus value exports while also participating in the general fact traversal. Normalize syntax events through a focused `ModuleInterfaceBuilder` during the single AST walk so interface policy has one cohesive owner without adding another traversal.

### READ-024 — The artifact cache maintains three synchronized collections

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/local.rs:204-326`

Entries, fingerprint lookup, and FIFO order are stored separately and manually updated during insertion and eviction, with internal `expect` calls depending on synchronization. A single ordered entry collection or a dedicated index type would reduce invariants and make eviction behavior easier to verify at the cache's small fixed capacity.

### READ-025 — Validated and unvalidated load options duplicate policy behavior

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-project/src/options.rs:188-352`

Both option types implement near-identical extension support and exclusion checks, while the validated wrapper retains the original options alongside normalized extensions and forwards numerous getters. Put behavior on normalized storage and use small validated budget/value types to centralize the repeated nonzero checks.

### READ-026 — Tsconfig field wrappers duplicate parsing state and weaken array validation

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-project/src/tsconfig.rs:33-100`

String and string-array fields separately implement the same absent/null/wrong-type/present state machine, and the array parser silently drops non-string elements with `filter_map`. A generic parsed-field representation with strict element parsing would preserve diagnostics and remove parallel state logic.

### READ-027 — Discovery reads and parses each tsconfig twice

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/discovery.rs:155-269`

Effective-config construction reads and parses the file, after which reference discovery rereads and reparses the same DTO. Return reference metadata with the effective node or cache parsed configs in the traversal graph so discovery has one source of config truth.

### READ-028 — Tsconfig diagnostics conflate cycles with general config errors

- **Severity:** Medium
- **Category:** API
- **Location:** `glass-lint-project/src/tsconfig.rs:430-439`

The diagnostic shape always exposes a `cycle_target`, and malformed fields or patterns populate it with the config's own path; discovery then sorts every diagnostic as though it described a cycle edge. Use explicit variants or an error code plus optional related path so callers cannot misinterpret diagnostic meaning.

### READ-029 — Deadline enforcement is scattered and misses recursive config work

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig.rs:487-569`

Walkers and loader loops each perform their own deadline checks, but recursive `extends` processing has no deadline context and reference traversal has no uniform phase budget. Pass one deadline/budget object through discovery, config construction, reading, and resolution so boundedness is both comprehensive and visible.

### READ-030 — Loader frontier expansion interleaves several nominal phases

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:424-558`

Reading a path immediately invokes core local analysis, resolution mutates caches and queues more paths, and final linking/reporting later returns unnamed timings. Model frontier expansion as a typed stage/result, followed by an explicit closed-project link/match stage, so lifecycle and partial-failure handling are apparent without reading the whole loader.

### READ-031 — Resolution-cache use duplicates keys and performs multiple lookups

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-project/src/loader.rs:467-490`

Request recording clones cache keys, probes the cache, resolves on a miss, inserts, and then looks up again with an `expect`. Give `ResolutionCache` an entry-style `resolve_or_get` operation that owns this state transition and returns the stored outcome directly.

### READ-032 — Resolver modes duplicate almost-complete option structures

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-project/src/resolver.rs:24-81`

Import and require resolution build two nearly identical option values, cloning extensions and aliases for each and differing mainly in conditions. Build common options once and express the semantic difference with a request-mode enum or a small mode-specific adjustment.

### READ-033 — Export fixed-point propagation clones keys and resolves twice  ✅

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/graph.rs:39-63`

Each propagation attempt clones the export name and performs an initial resolution followed by another update lookup. Let `ExportTable` accept borrowed keys and expose a monotone `set_if_changed`/entry operation that both applies the lattice update and reports whether the fixed point changed.

### READ-034 — Tsconfig tests use collision-prone manual temporary directories  ✅

- **Severity:** Medium
- **Category:** Testing
- **Location:** `glass-lint-project/src/tsconfig.rs:739-915`

These tests repeatedly create fixed paths, write fixtures, and manually remove them, even though the crate already has an RAII temporary-project helper. Reuse a unique RAII fixture so parallel tests cannot collide and panic paths cannot leak state into later runs.

### READ-035 — `AdmittedSourcePath` is exported without a public producer  ✅

- **Severity:** Low
- **Category:** API
- **Location:** `glass-lint-project/src/lib.rs:22-22`

`SourceAdmission` is an intentional low-level public API and its public `canonicalize` method explains the exported `CanonicalProjectPath`, but no public operation constructs or returns `AdmittedSourcePath`; only crate-private classification does. Stop re-exporting that unreachable implementation type unless a public admitted-path workflow is deliberately added.

### READ-036 — Callback-scope modeling repeats lookup and concentrates special cases  ✅

- **Severity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/collect/callbacks.rs:114-250`

Lexical parent-chain lookup is duplicated, and one long conditional encodes IIFE, `forEach`, and `Promise.then` behavior. Share the lookup primitive and split callback models into named handlers or declarative cases so supported semantics are easier to audit.

### READ-037 — Fact visiting repeats member-read and target extraction mechanics  ✅

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/build/visitor.rs:49-97`

Ordinary and optional member reads emit the same fact through separate branches, and variable declarations compute pattern targets before recomputing them for later work. Reuse one member-read emitter and retain the first target projection.

### READ-038 — Callable provenance duplicates rooted-target logic  ✅

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/query/provenance.rs:284-323`

`ValueAlias` and `BoundCallable` repeat the same global/module/rooted-path provenance branches. Extract provenance derivation for a resolved target path and leave only variant-specific preparation at the call site.

### READ-039 — Returned-from subject queries mirror event branches  ✅

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/query.rs:326-352`

Member-call and member-read cases perform the same returned-value filtering against different indexes. Select the relevant event iterator once and apply one shared predicate.

### READ-040 — Analysis-limit invariants are repeated for every field  ✅

- **Severity:** Low
- **Category:** Newtype
- **Location:** `glass-lint-core/src/limits.rs:35-263`

Six limits repeat positivity checks, getters, builders, test setters, and raw serialization plumbing. A private positive-limit type or table-driven validation helper would centralize the invariant while retaining descriptive public accessors.

### READ-041 — Pretty-report assembly duplicates identical display-cell creation  ✅

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/report.rs:340-398`

Cached and uncached source-line branches build the same display-cell vector after obtaining line data differently. Extract the shared line-to-cells transformation so caching concerns do not duplicate formatting policy.

### READ-042 — Include and exclude matching repeat the same path predicate  ✅

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-project/src/tsconfig.rs:397-423`

Include and exclude processing independently perform the same relative-path and pattern match operation. Use one named predicate to make their only intended difference—the surrounding any/all policy—obvious.

### READ-043 — Project-phase timing is fieldwise boilerplate  ✅

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-project/src/loader.rs:68-171`

Getters, recorders, and `AddAssign` repeat the same operation for each duration field. An internal phase key plus indexed durations, with named public accessors, would make adding a phase less error-prone and align timings with the explicit pipeline.

### READ-044 — Dead-code suppressions obscure the intended flow-state surface  ✅

- **Severity:** Low
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:28-125`

Several flow-state fields and methods are kept behind `allow(dead_code)`, while related effect identifiers/accessors and a no-op state binding remain elsewhere in the flow modules. Remove obsolete surfaces or document the conditional consumer so readers can distinguish deliberate future-facing state from abandoned machinery.

### READ-045 — Fact-builder tests repeat the complete parse-and-build harness  ✅

- **Severity:** Low
- **Category:** Testing
- **Location:** `glass-lint-core/src/analysis/facts/build/mod.rs:255-626`

Many unit tests repeat parsing, resolver creation, builder creation, visiting, and stream finalization before reaching their assertion. Route them through a focused fixture helper so each test foregrounds the semantic difference it covers.

### READ-046 — Resolver outcome classification repeats the same policy decision  ✅

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-project/src/resolver.rs:107-151`

Outside-project and excluded-path branches separately derive the same internal-versus-external request classification. Compute that classification once and retain only the path-specific outcome construction in each branch.

### READ-047 — Effective-tsconfig implementation fields are exposed and suppressed as dead  ✅

- **Severity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-project/src/tsconfig.rs:253-274`

Config path/base fields are public despite being unused by current consumers and requiring dead-code allowances. Make implementation state private or expose intentional query methods if it is part of the supported contract.

### READ-048 — Module-identity construction scans authored requests twice  ✅

- **Severity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/identities.rs:81-134`

Named-import and namespace-import identity handling traverse the same request collection separately. Classify each request once and route it to the appropriate identity update.

### READ-049 — Strongly connected component tracking uses a map as a set  ✅

- **Severity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/graph.rs:212-279`

SCC membership is represented as `BTreeMap<ModuleId, bool>`, producing mixed `get`, `entry`, and boolean handling where only membership is needed. A `BTreeSet` expresses the invariant directly and shortens the traversal.

### READ-050 — A committed integration test retains a debugging name  ✅

- **Severity:** Low
- **Category:** Naming
- **Location:** `glass-lint-core/tests/compact_source/constructors.rs:154-180`

`debug_urlalias_global_constructor` does not describe the behavior being protected and reads as temporary instrumentation. Rename it around the constructor-alias contract, and consider placing it in the surrounding alias case matrix.

## Systemic Themes

- Pipeline phase types are thinner than the actual invariants. Local analysis, request resolution, linking, matching, and reporting frequently exchange generic collections or tuples, forcing the next phase to reconstruct identities, counts, validation, or timing meaning.
- Ownership follows orchestration rather than data lifetime. Source snapshots, names, requests, matcher clauses, flow summaries, fingerprints, and resolver options are cloned primarily because owning collections do not provide draining, entry-style, `into_parts`, or coherent borrowed-view APIs.
- Parallel representations have accumulated around boundaries. Module identities, semantic paths, validated options, tsconfig fields, and call semantics each have multiple implementations that must evolve together.
- Several large methods conceal meaningful transitions: scope freezing, module-interface extraction, cross-function flow propagation, loader frontier expansion, and final report assembly. Smaller domain-owned builders would clarify when data becomes validated, immutable, resolved, linked, or reportable.
- Tests cover substantial semantic behavior, but repeated harness setup and ad hoc filesystem fixtures make intent harder to scan and introduce avoidable maintenance and isolation risk.

## Open Questions

None remain after tracing current invariants, call sites, tests, module documentation, and the changes that introduced the relevant APIs:

- `ScopeCollectionIssue` is explicitly a conservative diagnostic intended for conversion into analysis status; it is not an unreachable parser invariant. READ-001 therefore remains a concrete defect.
- `CallContext` is a closed projection input: no external monotone state changes how an identical value is processed. Completed contexts should remain deduplicated, confirming READ-005.
- `SourceCorpus` is intentionally a multi-root, non-linking corpus utility, as shown by its API, module documentation, and CLI/profiling consumers. Owning validated options and constructing an admission boundary per independent root are consistent with that role and do not require a finding.
- `SourceAdmission` is deliberately public, and `CanonicalProjectPath` is part of its public `canonicalize` result. Only `AdmittedSourcePath` lacks a public producer or consumer, so READ-035 has been narrowed to that type.
- Catalog construction is deliberately the matcher validation and compilation boundary. READ-011 is therefore specifically about declarations claiming validated status and carrying compiled flow state before that established boundary, not about an unresolved compatibility choice.

## Coverage

This was a static, file-by-file audit of the entirety of `glass-lint-core` and `glass-lint-project` (approximately 43,000 lines of Rust), including all production modules, inline unit tests, integration-test modules, crate architecture documents, and the root architecture guidance. Core coverage included parsing, syntax normalization, scope/binding/provenance, value resolution, facts, local artifacts and caching, intraprocedural and cross-function flow, project linking, matching/compiler APIs, project session/tables, lint orchestration, and reporting. Project coverage included admission, source corpora, walking/discovery, options, tsconfig parsing/inheritance/references, module resolution, loader orchestration, diagnostics, timings, and filesystem-backed tests. The open-question follow-up additionally traced relevant repository-wide call sites, tests, API documentation, blame, and introducing commits.

No implementation or configuration files were changed, and no runtime tests were run because the requested deliverable is a static readability report. Other workspace crates were outside the requested scope.
