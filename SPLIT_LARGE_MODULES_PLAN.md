# Large-module and test-placement plan

## Audit scope

This is a structural audit of every Rust file in the workspace. The plan flags
production modules over 500 lines, and production files containing inline
`#[test]` functions. Files at exactly 500 lines are not size findings. Tests
already under `tests/`, a dedicated `tests.rs`, or a dedicated test subtree are
treated as correctly placed, although oversized test files still need to be
split by concern.

The implementation order should be:

1. Move or delete inline production tests, preserving only behavior that is not
   already covered elsewhere.
2. Split the production modules at the seams below, keeping public APIs and
   provider/crate boundaries stable unless a split exposes a genuinely better
   owner.
3. Split oversized test files into focused test modules or integration files.
4. Run the narrow owning-crate tests after each group, then `make ci`.

## Modules over 500 lines

- `glass-lint-core/src/analysis/flow/summary/summaries.rs` (501 lines): split summary construction, call-target propagation, and sink projection into focused modules, and move its inline tests to a dedicated test module.
- `glass-lint-core/src/analysis/project/model.rs` (510 lines): separate project model definitions from identity/index construction and move the inline model tests to `tests.rs`.
- `glass-lint-core/src/api/compiler/tests/normalize/algebra.rs` (512 lines): split normalization algebra tests by boolean, sequence, predicate, and lifecycle laws into focused test files.
- `glass-lint-harness/src/runner.rs` (518 lines): split snippet execution, bundle execution, expectation comparison, and bundler error handling, and move the inline runner tests to `tests.rs`.
- `glass-lint-project/src/loader.rs` (518 lines): extract project-load orchestration, wave reading/analyzing, resolution enqueueing, and completion state into cohesive loader modules.
- `glass-lint-core/src/api/rule/query/value.rs` (520 lines): separate value-expression definitions, constructors/validation, and predicate evaluation, and move the inline value tests to `tests.rs`.
- `glass-lint-harness/src/types/case.rs` (520 lines): split bundle metadata, project-case conversion, tool expectations, and finding expectations into focused type modules.
- `glass-lint-core/tests/integration/matching/scope.rs` (522 lines): divide scope integration coverage into lexical bindings, branch joins, loops, and abrupt-control-flow files.
- `glass-lint-core/src/api/classification.rs` (526 lines): separate classification data types, normalization/conversion, and validation behavior, and move the inline tests to a dedicated test module.
- `glass-lint-core/src/analysis/model/module.rs` (528 lines): split module identity/interface types from export, import, and re-export operations, and move the inline module tests out of production code.
- `glass-lint-core/src/lint/selection.rs` (531 lines): separate rule-selection parsing, catalog-index resolution, and selection validation, and move the inline selection tests to `tests.rs`.
- `glass-lint-core/src/analysis/resolution/expression.rs` (534 lines): extract recursive expression resolution from static-value helpers and unsupported/dynamic-result handling.
- `glass-lint-core/src/lint/batch.rs` (534 lines): split batch iterator state, worker submission/backpressure, and ordered result/cancellation handling, and move the inline batch tests to `tests.rs`.
- `glass-lint-project/src/tsconfig/mod.rs` (536 lines): separate parsed-field/value decoding, tsconfig parsing/diagnostics, and extends traversal; keep the existing tests subtree as the test owner.
- `glass-lint-cli/src/config.rs` (541 lines): split configuration schema/defaults, file discovery/loading, validation, and provider/profile linter construction, and move all inline config tests to `tests.rs`.
- `glass-lint-core/src/analysis/matching/query/view.rs` (551 lines): separate event-index view types from index capability lookup and private-network predicate logic, and move the inline predicate tests to `tests.rs`.
- `glass-lint-core/tests/integration/linter.rs` (554 lines): split catalog/selection tests, finding-location tests, diagnostics/limits tests, and report-order tests into focused integration files.
- `glass-lint-harness/src/profile/types.rs` (556 lines): separate profile summary/value types, phase timing aggregation, measured repetition accumulation, and run metadata into focused modules.
- `glass-lint-core/src/analysis/flow/cross/mod.rs` (563 lines): keep cross-file projection orchestration in `mod.rs`, extract context propagation and worklist coordination, and move the inline worklist tests to `tests.rs`.
- `glass-lint-core/src/analysis/matching/mod.rs` (572 lines): split occurrence-index ownership, linked module/global overlays, and evidence insertion, and move the inline matching tests to `tests.rs`.
- `glass-lint-core/src/api/compiler/tests/reference.rs` (583 lines): divide reference-plan tests by declaration/reference resolution, physical operator selection, and failure/limit behavior.
- `glass-lint-core/src/analysis/facts/visitor.rs` (589 lines): split AST visitor handling into declarations/expressions, control-flow traversal, and construction/export emission modules.
- `glass-lint-core/tests/integration/matching/declarative/arguments.rs` (593 lines): divide argument matching coverage into provenance/aliases, static values, helper parameters, and constructor/member cases.
- `glass-lint-core/src/analysis/scope/build/assignments.rs` (598 lines): extract assignment classification, destructuring target collection, mutation recording, and provenance updates into separate builder modules.
- `glass-lint-core/src/analysis/scope/graph.rs` (600 lines): separate scope-graph storage, graph construction, and lookup/traversal operations so the graph owner exposes a smaller API.
- `glass-lint-core/src/project/report/tests.rs` (603 lines): split report tests into finding qualification, report combination, diagnostics/operations, and serialization/ordering files.
- `glass-lint-core/src/analysis/local.rs` (606 lines): separate artifact fingerprints/keys, cache synchronization and eviction, semantic artifact construction, and project-module conversion, and move inline cache tests to `tests.rs`.
- `glass-lint-core/src/analysis/semantic/mod.rs` (608 lines): split semantic-analysis orchestration, cache lookup, parse/fact completion, and artifact/error conversion, and move inline semantic tests to `tests.rs`.
- `glass-lint-core/src/api/rule/query/tests.rs` (610 lines): split query-builder tests into expression composition, lifecycle/event construction, constraints, and limit/error cases.
- `glass-lint-core/src/ecma_version.rs` (620 lines): separate ECMAScript-version representation, parsing/display, compatibility checks, and test cases, moving inline tests to `tests.rs`.
- `glass-lint-core/src/environment.rs` (627 lines): split environment capability definitions, provider/global configuration, validation, and test fixtures, moving inline tests to `tests.rs`.
- `glass-lint-core/src/analysis/scope/build/tests.rs` (635 lines): divide scope-builder tests into bindings/aliases, assignment history, control flow, and provenance/index assertions.
- `glass-lint-core/src/api/compiler/tests/physical.rs` (637 lines): split physical-plan tests by index/operator selection, requirements, lifecycle correlation, and bounded execution behavior.
- `glass-lint-core/src/analysis/model/fact.rs` (650 lines): separate fact identifiers/basic views, call/parameter payloads, control/function payloads, and fact-stream operations, moving inline model tests to `tests.rs`.
- `glass-lint-core/tests/integration/query/composition.rs` (661 lines): split query composition tests into valid construction, contradiction/error rejection, limit enforcement, and canonicalization cases.
- `glass-lint-core/src/project/tests/linking_and_flow.rs` (669 lines): split project integration tests into module linking/re-exports and cross-file flow/provenance scenarios.
- `glass-lint-core/src/analysis/model/value.rs` (674 lines): separate value identity/state types, static-value/object operations, and alias/flow conversion logic, and move inline value tests to `tests.rs`.
- `glass-lint-core/src/api/compiler/physical.rs` (683 lines): split physical-plan requirements/indexes, operator lowering, execution helpers, and explanation/diagnostic assembly into cohesive modules.
- `glass-lint-core/src/analysis/flow/effect/mod.rs` (688 lines): separate effect domain types and accessors, per-function effect collection, and multi-function effect storage; retain only the module wiring and move tests to the existing dedicated test file.
- `glass-lint-core/tests/integration/query/baseline.rs` (691 lines): divide baseline positive lifecycle cases, negative connectivity cases, and determinism/operation-budget cases into separate integration files.
- `glass-lint-core/src/analysis/project/projection.rs` (699 lines): separate module-interface projection, cross-file flow projection, and projected-evidence/error handling, and move the inline projection tests to `tests.rs`.
- `glass-lint-core/src/project/types/input.rs` (724 lines): split source/input newtypes, resolution request/outcome types, module-link targets, and project error types, and move inline input tests to `tests.rs`.
- `glass-lint-core/tests/integration/matching/compact.rs` (785 lines): divide compact matching coverage into module/instance provenance, aliases/reassignment, static values, and global/member cases.
- `glass-lint-core/src/analysis/flow/projector/tests.rs` (806 lines): split projector tests into branch/try control flow, loops/fixed points, mutation/alias state, evidence, and resource-limit cases.
- `glass-lint-core/src/analysis/matching/occurrence.rs` (815 lines): separate occurrence iteration/merge algorithms, package/module overlay matching, and occurrence-key/index types, moving inline occurrence tests to `tests.rs`.
- `glass-lint-core/src/api/rule/query/mod.rs` (826 lines): keep public query re-exports small and extract query declarations, expression composition, lifecycle construction, and test-only wiring into their owning modules.
- `glass-lint-core/src/analysis/model/flow.rs` (830 lines): split flow identifiers/state records, effect/alias operations, and evidence/completion modeling, moving inline flow tests to `tests.rs`.
- `glass-lint-core/src/parse.rs` (881 lines): separate parser configuration, source-language/TypeScript normalization, parse diagnostics, and test fixtures, moving inline parser tests to `tests.rs`.
- `glass-lint-core/src/analysis/flow/projector/mod.rs` (957 lines): keep projector entry points and path admission in `mod.rs`, extract run orchestration, pending-state management, and emission/finalization into focused modules, and move inline tests to the dedicated projector test module.
- `glass-lint-core/src/analysis/model/scope.rs` (967 lines): separate scope/binding model types, mutation/history operations, and lookup/provenance views, moving inline scope tests to `tests.rs`.
- `glass-lint-core/src/analysis/facts/mod.rs` (1021 lines): reduce the module to fact-builder wiring and public artifact assembly; extract provenance state, fact emission, module/export recording, and semantic-fact indexing, and move inline stream tests to the existing test module.
- `glass-lint-core/src/api/rule/query/lifecycle.rs` (1161 lines): split lifecycle source/event/sink types, relation and constraint construction, validation/normalization, and test cases into focused modules, moving inline tests to `tests.rs`.
- `glass-lint-core/src/analysis/matching/arguments/mod.rs` (1290 lines): separate matcher input/overlay types, constrained-root preparation, argument evaluation, evidence assembly, and test cases into focused modules.
- `glass-lint-core/src/analysis/flow/projector/state.rs` (1407 lines): split flow environment/alias state, mutation and object tables, control-stack transitions, evidence snapshots, and completion/error handling into separate state modules, moving inline tests to `tests.rs`.

## Production files with inline tests

The 28 size findings above that mention moving inline tests also contain
production-file tests. The following additional 51 files contain inline
`#[test]` functions but are not dedicated test files. Move useful tests to the
owning module's sibling `tests.rs` (or its existing dedicated `tests/` subtree),
and delete tests that duplicate stronger coverage; leave only production
definitions in these files.

- `glass-lint-cli/src/lint.rs`: move the inline lint-dispatch tests to a dedicated CLI test module.
- `glass-lint-cli/src/output.rs`: move the inline output-format tests to a dedicated CLI test module.
- `glass-lint-cli/src/rules_doc.rs`: move the inline rules-document tests to a dedicated CLI test module.
- `glass-lint-core/src/analysis/facts/origin_map.rs`: move the inline origin-map tests to `tests.rs`.
- `glass-lint-core/src/analysis/flow/cross/evidence.rs`: move the inline cross-flow evidence tests to `tests.rs`.
- `glass-lint-core/src/analysis/flow/cross/state.rs`: move the inline cross-flow state tests to `tests.rs`.
- `glass-lint-core/src/analysis/flow/mod.rs`: move the inline flow-module tests to `tests.rs`.
- `glass-lint-core/src/analysis/flow/summary/sink.rs`: move the inline sink-summary tests to `tests.rs`.
- `glass-lint-core/src/analysis/flow/summary/store.rs`: move the inline summary-store tests to `tests.rs`.
- `glass-lint-core/src/analysis/matching/evidence.rs`: move the inline matching-evidence tests to `tests.rs`.
- `glass-lint-core/src/analysis/matching/identity_map.rs`: move the inline identity-map tests to `tests.rs`.
- `glass-lint-core/src/analysis/mod.rs`: move the inline analysis-module tests to `tests.rs`.
- `glass-lint-core/src/analysis/model/static_properties.rs`: move the inline static-property tests to `tests.rs`.
- `glass-lint-core/src/analysis/module_request.rs`: move the inline module-request tests to `tests.rs`.
- `glass-lint-core/src/analysis/project/linker/graph.rs`: move the inline linker-graph tests to `tests.rs`.
- `glass-lint-core/src/analysis/project/state.rs`: move the inline project-state tests to `tests.rs`.
- `glass-lint-core/src/analysis/scope/build/history.rs`: move the inline assignment-history tests to `tests.rs`.
- `glass-lint-core/src/analysis/scope/frozen_assignments.rs`: move the inline frozen-assignment tests to `tests.rs`.
- `glass-lint-core/src/analysis/scope/mod.rs`: move the inline scope-module tests to `tests.rs`.
- `glass-lint-core/src/analysis/semantic/status.rs`: move the inline semantic-status tests to `tests.rs`.
- `glass-lint-core/src/analysis/trace.rs`: move the inline trace tests to `tests.rs`.
- `glass-lint-core/src/api/compiler/rule.rs`: move the inline compiler-rule tests to `tests.rs`.
- `glass-lint-core/src/api/rule/mod.rs`: move the inline rule-API tests to `tests.rs`.
- `glass-lint-core/src/api/rule/module.rs`: move the inline module-pattern tests to `tests.rs`.
- `glass-lint-core/src/api/rule/taxonomy.rs`: move the inline taxonomy tests to `tests.rs`.
- `glass-lint-core/src/diagnostic.rs`: move the inline diagnostic tests to `tests.rs`.
- `glass-lint-core/src/limits.rs`: move the inline limit tests to `tests.rs`.
- `glass-lint-core/src/lint/catalog.rs`: move the inline catalog tests to `tests.rs`.
- `glass-lint-core/src/lint/linter.rs`: move the inline linter tests to `tests.rs`.
- `glass-lint-core/src/lint/report/evidence.rs`: move the inline report-evidence tests to `tests.rs`.
- `glass-lint-core/src/lint/report/files.rs`: move the inline report-file tests to `tests.rs`.
- `glass-lint-core/src/project/session/artifacts.rs`: move the inline session-artifact tests to `tests.rs`.
- `glass-lint-core/src/project/types/report/analysis_report.rs`: move the inline analysis-report tests to `tests.rs`.
- `glass-lint-core/src/project/types/report/code.rs`: move the inline report-code tests to `tests.rs`.
- `glass-lint-core/src/project/types/report/evidence.rs`: move the inline report-evidence tests to `tests.rs`.
- `glass-lint-datastructures/src/budget.rs`: move the inline budget tests to `tests.rs`.
- `glass-lint-datastructures/src/diagnostic.rs`: move the inline diagnostic tests to `tests.rs`.
- `glass-lint-datastructures/src/fingerprint.rs`: move the inline fingerprint tests to `tests.rs`.
- `glass-lint-datastructures/src/history.rs`: move the inline history tests to `tests.rs`.
- `glass-lint-datastructures/src/name.rs`: move the inline name tests to `tests.rs`.
- `glass-lint-datastructures/src/table.rs`: move the inline table tests to `tests.rs`.
- `glass-lint-harness-cli/src/args.rs`: move the inline harness-argument tests to `tests.rs`.
- `glass-lint-harness/src/builtins.rs`: move the inline built-in adapter tests to `tests.rs`.
- `glass-lint-harness/src/bundler.rs`: move the inline bundler tests to `tests.rs`.
- `glass-lint-harness/src/profile/mod.rs`: move the inline profile-module tests to `tests.rs`.
- `glass-lint-harness/src/profile_manifest.rs`: move the inline profile-manifest tests to `tests.rs`.
- `glass-lint-js/src/lib.rs`: move the inline JavaScript-catalog tests to `tests.rs`.
- `glass-lint-obsidian/src/lib.rs`: move the inline Obsidian-catalog tests to `tests.rs`.
- `glass-lint-output/src/lib.rs`: move the inline output-library tests to an integration test or dedicated `tests.rs`.
- `glass-lint-project/src/loader_metrics.rs`: move the inline loader-metrics tests to `tests.rs`.
- `glass-lint-project/src/resolver.rs`: move the inline resolver tests to `tests.rs`.

## Completion check

After the work, verify that no production Rust file contains `#[test]` or an
inline test module, and rerun the structural scan for files over 500 lines.
Any remaining large file should have an explicit owner-based exception added
to this plan rather than being silently ignored.
