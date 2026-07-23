# Glass Lint Core and Project Readability Audit

Audit date: 2026-07-23

Scope: the entirety of `glass-lint-core` and `glass-lint-project`, with emphasis on duplication, duplicated logic, borrowing, allocation and cloning, simplification, organization, and visibility of pipeline phases.

## Summary

This audit originally found 34 actionable readability and maintainability issues: 3 high severity, 25 medium severity, and 6 low severity. After resolution, 11 open findings remain (2 high, 7 medium, 2 low). The highest-risk findings are local-flow budget exhaustion that is silently converted into missing evidence, non-canonical `tsconfig` cycle detection, and project resolver errors that are collapsed into ordinary missing-module outcomes. Scope planning and source-order collection intentionally remain separate passes; the narrower concern is duplicated structural traversal policy, not the existence of two traversals.

The broad architectural opportunity is to make each pipeline transition consume one phase-owned type and produce the next. Today, several boundaries retain raw and compiled forms together, erase semantic newtypes and reconstruct them later, or build parallel maps that describe one logical record. Those choices obscure the intended pipeline and cause avoidable clones, repeated validation, repeated indexing, and transient collections.

## Findings

### Core: Pipeline Architecture and Ownership

#### READ-001 — Two intentional scope passes duplicate their structural grammar
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/plan.rs:84-340`, `glass-lint-core/src/analysis/scope/collect/mod.rs:415-687`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:31-269`

The declaration planner must finish before source-order provenance collection so hoisted and TDZ bindings are known at every use position. The issue is therefore not that the AST is traversed twice; it is that `ScopePlanner` and `ScopeCollector` independently enumerate scope-forming syntax and its child order, so adding or changing support for a function, arrow, block, loop, switch, `with`, or catch scope requires synchronized edits.

**Historical constraint:** Commit `f14cf0f` previously combined the phases in one `LexicalScopeCollector` with a `Pass::Predeclare`/`Pass::Collect` mode. That design still traversed the AST twice, kept source-order state alive during predeclaration, spread pass-conditioned behavior across the collector, and required divergence/reuse modes in the same mutable owner. Commit `d101254` intentionally restored a consuming `ScopePlan` boundary and distinct `ScopePlanner` and `ScopeCollector` state; that separation should not be reverted.

**Decision:** Keep declaration planning, source-order collection, and freezing as three explicit phases. Do not merge the planner and collector, do not restore a `Pass` enum, and do not make one phase stateful object mutate itself from planning mode into collection mode. This finding should be implemented only as consolidation of the shared traversal grammar; leaving the current two visitors is preferable to a design that weakens phase ownership or introduces `Rc`, `RefCell`, unsafe aliasing, or clone-to-release-borrow workarounds.

**Implementation guidance:**

1. First add a test-only structural trace emitted by both current visitors. Each trace entry should contain the parent scope occurrence, scope kind, complete span, and a sibling occurrence number; assert exact trace equality before any refactor. Keep the existing `ScopeShapeTable` mismatch and unconsumed-shape tests because the production path must remain fail closed even after the trace test exists.
2. Introduce a phase-neutral `ScopeTraversal<P>` whose `P` is an owned phase policy, with separate `PlanningScopePass` and `CollectionScopePass` implementations. The traversal, not either pass, should be the sole owner of the `Visit` methods for the overlapping structural syntax. Running `ScopeTraversal<PlanningScopePass>` and then `ScopeTraversal<CollectionScopePass>` still performs two AST walks and produces a consuming `ScopePlan` between them.
3. Keep phase storage disjoint. The planning policy may own only names, lexical scopes, declaration bindings, structural identities, and exhaustion state. The collection policy may consume the immutable plan and own only source-order assignments, aliases, calls, mutations, callback facts, dynamic-eval effects, and mismatch diagnostics.
4. Give the traversal explicit hooks around children rather than a general callback that can recursively visit arbitrary nodes. The traversal must define child order once, while hooks perform phase-specific work at named points such as declaring a parent binding, entering a planned scope, declaring parameters, observing an initializer, and leaving the scope. A hook must not call `visit_with` itself, because that would restore two authorities over nesting order.
5. Migrate one syntax family at a time while both old visitors still compile: plain blocks first, then loops and switch, then catch and `with`, and functions/arrows last. After each family, remove the corresponding old `Visit` methods immediately and run the focused scope tests; do not leave delegating old methods as a compatibility layer.
6. Preserve the current non-obvious order exactly. A function declaration name is declared in its parent before entering the function scope; parameters belong to the function scope; a catch parameter and its body statements share the catch scope without adding a second body-block scope; a switch discriminant is visited before the switch scope; a `with` object is visited before entering the dynamic scope; loop initializers/tests/updates execute within the loop scope; and `var` walks outward to the nearest function/program owner while `let` and `const` remain lexical.
7. Do not identify scopes by span alone. Retain a structural identity containing parent `ScopeId`, kind, full span, and deterministic sibling occurrence, or preserve the current per-key queue with an explicit occurrence in the trace. Equal-span/generated nodes must consume distinct planned scopes deterministically.
8. On any plan/collection mismatch, never allocate a fallback scope and never continue resolving globals as if the plan were valid. Record `ShapeMismatch`, keep provenance local/unknown for the affected path, detect unconsumed planned shapes during freeze, and propagate the existing incomplete-analysis status.
9. Require parity tests for hoisting, TDZ use-before-declaration, `var` across nested blocks, named function/class declarations, default/rest/destructured parameters, imports, catches, every loop form, switch, `with`, direct and aliased `eval`, nested functions/arrows, equal-span synthetic nodes, minified source, name exhaustion, and unsupported/dynamic patterns. Add one structural test fixture containing every scope-forming syntax so a missing traversal arm fails in a single focused test.
10. Completion means there are still two invocations and two phase-owned state types, but only one production implementation of the overlapping scope-forming `Visit` methods. Benchmark representative large/minified files and reject the refactor if the generic driver increases peak memory materially, requires cloned AST nodes, or makes the traversal order harder to audit than the current duplication.

If `ScopeTraversal<P>` cannot be expressed with ordinary exclusive borrowing and narrow hooks, close this finding as accepted duplication and retain the current plan/collector design. A macro that generates two visitors is a fallback only if its declaration makes child order readable and adding a new scope-forming syntax cannot compile without defining both phase hooks.

#### READ-002 — Local-flow resource exhaustion silently becomes absent evidence
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/index.rs:14-49`, `glass-lint-core/src/analysis/flow/projector/state.rs:25-126`, `glass-lint-core/src/analysis/flow/projector/mod.rs:39-76`

Local flow uses hard-coded limits for objects, states, emissions, and mutation-log entries. When those limits are reached, paths are dropped or restoration fails without a typed outcome, so callers cannot distinguish “no flow” from “analysis exhausted”; cross-module flow already treats exhaustion as a first-class partial-analysis condition.

Return a `LocalFlowProjectionOutcome` containing evidence, exhaustion state, and bounded counters. Derive all flow budgets from the validated analysis limits; any local-flow exhaustion should record a file-scoped flow-budget diagnostic and make the combined report `Partial`, matching facts, effects, linking, and cross-module flow. Add focused tests for each limit so exhaustion consistently fails closed instead of looking like a clean negative result.

#### READ-003 — The link graph retains metadata that does not drive linking - DONE
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/state.rs:10-76`, `glass-lint-core/src/analysis/project/graph.rs:27-80`

The link graph stores SCC components and a provenance map, but the global-export fixed point iterates the full export set rather than the component graph, and provenance has no production reader. SCCs are used for a size check and then retained through matching, while the architecture documentation describes SCC-driven convergence.

**Decision:** Implement the SCC-DAG linker now (see `private/scc-plan.md`). The global monotone fixed-point over all modules is replaced with a topological walk of the SCC DAG: single-node SCCs resolve in one pass, multi-node SCCs use a cycle-local fixed-point. This eliminates the `O(modules × rounds)` loop for acyclic graphs (the common case) while keeping the same `ExportResolution` semantics, budget enforcement, and monotone memo table. The existing `MAX_SCC_SIZE` check remains as a pre- gate before the DAG walk. Remove edge provenance now because there is no reader, and reintroduce it only with the diagnostic that consumes it. Correct the architecture comment to describe bounded SCC-DAG convergence.

#### READ-004 — Validated project input is represented as parallel maps and a positional tuple
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/input.rs:101-190`, `glass-lint-core/src/analysis/project/model.rs:104-161`

`ValidatedProjectInput` keeps sources, resolutions, module IDs, and request IDs in separate maps keyed by overlapping identities, then exposes them as a five-element tuple. The linking phase uses only part of that payload, while source text and root information remain attached beyond the phase that owns them.

Introduce named, consuming phase types such as `AdmittedProject`, `LocallyAnalyzedProject`, and `ResolvedLinkInput`. Store each resolution beside its qualified request identity instead of maintaining synchronized key spaces. Split report file-roster data from linker input so source text can be dropped or borrowed as soon as local analysis finishes.

#### READ-005 — Direct project linting assigns identities and then rebuilds the project
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/linter.rs:192-213`, `glass-lint-core/src/project/input.rs:40-89`, `glass-lint-core/src/project/session.rs:785-833`

`Linter::lint_project` validates `ProjectInput` and computes module/request IDs, then discards those maps and re-admits the same data through `ProjectCollection`. Session resolution subsequently recomputes identities and revalidates outcomes, leaving two project-construction paths with overlapping responsibilities.

Keep `Linter::lint_project(ProjectInput)` as the public convenience API documented by core, but make it a thin adapter into the same staged session pipeline. Its initial admission should normalize the root and sources and enforce DTO size/duplicate constraints without assigning module/request IDs; after local analysis discovers the authoritative request set, `LocallyAnalyzedProject::resolve` should validate outcomes and assign both identity tables exactly once. Delete the early ID maps and the second normalization path rather than maintaining a separate direct-input linker.

#### READ-006 — Authored requests are materialized and projected more than once
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/session.rs:315-359`, `glass-lint-core/src/project/session.rs:785-828`, `glass-lint-core/src/analysis/module.rs:320-357`

Local artifacts store a `BTreeMap` whose value repeats the complete request even though later code primarily needs membership. Resolution then calls `requests_with_ids` again, recreating request strings and keys to recover local IDs; the module interface also converts typed importer paths to strings and rebuilds them.

Build an `AuthoredRequestTable` once per module, containing the local request ID and the public request record. Keep a key-to-ID index for validation and qualify it once after module IDs are assigned. Expose borrowed iteration internally and reserve owned request conversion for the API boundary.

### Core: Facts, Flow, Matching, and Linking

#### READ-010 — Function and call metadata is copied across facts, effects, and summaries
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/model.rs:192-305`, `glass-lint-core/src/analysis/flow/effect.rs:342-469`, `glass-lint-core/src/analysis/flow/summary.rs:279-400`

Parameter bindings and call relationships originate in semantic facts, are cloned into function effects, and are copied again into function summaries. These representations coexist within one analysis lifetime, so the extra ownership obscures which structure is canonical and increases memory churn for call-heavy files.

Keep canonical function and call tables addressed by typed IDs. Let effects and summaries store IDs plus only genuinely derived state, borrowing immutable metadata during projection. If summaries must outlive facts, make that ownership transition explicit and consuming rather than cloning at every intermediate phase.

#### READ-011 — Flow symbols are rebound repeatedly instead of once per module
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/index.rs:85-113`, `glass-lint-core/src/analysis/flow/projector/evidence.rs:25-139`, `glass-lint-core/src/analysis/flow/summary.rs:605-660`, `glass-lint-core/src/analysis/flow/cross/mod.rs:338-460`

Sources are converted to module-local name paths for indexing, but requirements and sinks are converted again inside event/state loops, and cross-module propagation reconstructs owned symbol paths. Cross-flow candidate discovery also scans every flow for each effect/call instead of sharing the source index used by local projection.

Add a `BoundFlowPlan` phase between catalog compilation and flow execution. Bind sources, requirements, sinks, and present-argument indexes to module-local identities once, then share those indexes between local and cross projection. Return iterators for small index sets and keep owned `SymbolPath` values only in the provider-neutral catalog.

#### READ-012 — Flow-state edits clone full states and log unchanged mutations
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:304-416`

State mutation clones the prior state, then clones the new state on guard drop even if nothing changed. Object removal clones matching entries before cloning again into the mutation log, while joins clone and clear whole alias/state collections before reinsertion.

Make the edit guard compare old and new values and skip no-op log entries. Use ordered-range removal or draining for one object's states, and merge into scratch storage rather than repeatedly clearing and reinserting. Consider shared or persistent state values only after measuring; the first guardrail is to stop unconditional full-state snapshots.

#### READ-015 — Export storage clones flat keys and namespace resolution retraverses exports
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/state.rs:80-170`, `glass-lint-core/src/analysis/project/identities.rs:76-148`

The export table uses `(ModuleId, SmolStr)` flat keys, requiring owned name construction for common lookups and updates. Namespace resolution recursively collects exported names into temporary sets and then performs another recursive lookup for those names, repeating graph traversal and allocation.

Store exports as `ModuleId -> ModuleExports` so module lookup and borrowed name lookup are distinct operations. Expose a deterministic iterator over the resolved export table and use it for namespace projection. Cache only at a phase boundary with a clear invalidation rule; do not add another parallel export model.

### Project: Discovery, Resolution, and Configuration

#### READ-029 — `tsconfig` parsing, inheritance, and selection remain one clone-heavy representation
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig.rs:242-321`, `glass-lint-project/src/tsconfig.rs:487-562`

Parent configs are owned by the recursive loader but passed by reference into construction, which clones inherited file/include/exclude data. DTO `extends` and references are also cloned, and the resulting config retains raw include/exclude strings beside compiled pattern sets even though production selection uses the compiled form.

Separate `ParsedTsconfig`, consuming inheritance, and `CompiledTsconfigSelection` phases. Move owned parent and DTO fields when constructing the child, compile effective patterns once, and discard raw selection text unless diagnostics require it. If tests need the parsed form, test that intermediate type rather than retaining duplicate production state.

### Cross-Cutting Organization and Tests

#### READ-034 — Large inline test modules hide production phase structure
- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Testing
- **Status:** Complete
- **Location:** `glass-lint-core/src/project/report/tests.rs`, `glass-lint-project/src/tsconfig/tests.rs`, `glass-lint-core/src/analysis/scope/collect/tests.rs`

Several already-large production modules included hundreds of lines of inline tests, making the production state transitions and ownership boundaries harder to scan. All three identified locations have been extracted into sibling `tests.rs` modules: `glass-lint-core/src/project/report/tests.rs`, `glass-lint-project/src/tsconfig/tests.rs`, and the previously extracted `glass-lint-core/src/analysis/scope/collect/tests.rs`.

## Systemic Themes

- **Make phases consume ownership.** Parsed, validated, locally analyzed, resolved, linked, and reported data should be different types. Consuming transitions would eliminate several parallel maps, raw-plus-compiled fields, and clones whose only purpose is preserving an earlier phase.
- **Keep semantic identities typed.** Paths, module IDs, request IDs, function IDs, and export entries should remain typed inside the pipeline. Reconstructing them from strings obscures invariants and repeatedly pays validation and allocation costs.
- **Bind immutable policy once.** Catalog queries and flow declarations should be compiled once globally and bound once per module. Hot matching and propagation loops should operate on IDs and borrowed slices, not reparse symbol paths or recreate requests.
- **Make bounded failure observable.** Every exhausted budget should produce a typed, deterministic partial-analysis outcome. Silent truncation is both a correctness risk and an architectural leak.
- **Store one logical record once.** Parallel maps for exports, project input, and request metadata invite drift and inhibit borrowing. Cohesive records behind owner types make invariants enforceable and reduce key cloning.
- **Let transient structures be transient.** Line indexes, graph components, provenance, raw `tsconfig` patterns, and intermediate evidence matrices should be consumed or discarded at their owning boundary unless a downstream feature demonstrably uses them.

## Open Questions

None remain after tracing the current status model, public documentation, tests, consumers, and the Git history of the scope collector:

1. **Local-flow exhaustion makes the report partial.** This matches every other semantic/linking budget and prevents an exhausted negative result from being presented as complete. Record the reason at file scope for the affected module and let normal report combination raise project completion to `Partial`.
2. **Resolved by the SCC-DAG linker.** See `private/scc-plan.md`. SCCs become the primary driver of export resolution order; single-node SCCs resolve in one pass, cycles get a local fixed-point. Edge provenance is removed as dead data until a concrete diagnostic consumes it.
3. **`Linter::lint_project(ProjectInput)` remains a public convenience facade, not a second internal pipeline.** Its documented API is useful for in-memory callers, but it should feed the staged collection/local/resolution pipeline before IDs are assigned. Module and request identities should be created once, after the authoritative source and authored-request sets exist.
4. **A `tsconfig` extends cycle drops only the offending edge.** Emit a deterministic diagnostic, retain the current config's local settings and previously resolved acyclic inheritance, and do not broaden selection with the cyclic parent. This is the behavior asserted by the existing cycle tests; canonicalizing the candidate parent before membership checks closes the remaining alias-path hole.
5. **Cheap `Linter` cloning is a public contract.** The type documentation promises it for concurrent use, so immutable compiled configuration should be shared with `Arc`; only runtime handles with intentionally different sharing semantics should remain separate.

## Coverage

The audit enumerated and inspected all Rust source modules in `glass-lint-core` and `glass-lint-project`, including inline and dedicated test modules. Core coverage included API/catalog compilation, parsing/lowering, scope and semantic facts, matching, local and cross-module flow, project identities/input/linking/reporting, session/cache/execution, lint assembly, environment, diagnostics, and limits. Project coverage included options, admission, discovery, source loading, resolution, module path handling, `tsconfig` parsing/inheritance/selection, loader orchestration, corpus assembly, profiling, and tests.

Repository-level and owning-crate architecture documents, `TESTING.md`, `CONTRIBUTING.md`, manifests, and the prior report history were reviewed for intended boundaries and already-resolved findings. `cargo clippy -p glass-lint-core -p glass-lint-project --all-targets -- -W clippy::pedantic` completed successfully; its remaining output was predominantly documentation and `must_use` suggestions and was not duplicated here as readability findings.
