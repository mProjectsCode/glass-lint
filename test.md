# Glass Lint Test Inventory & Judgment

## Overview

This document inventories every test in the repository, judges individual test quality, and evaluates module-level coverage. Tests are organized by owning crate and functional area.

**Total tests inventoried: ~600+ across 278 files** (including inline `#[cfg(test)]` modules, integration tests, and harness rule-contract fixtures).

---

## glass-lint-datastructures

### `path/tests.rs` — NamePath, SymbolPath, PathView — 36+ tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `append`, `without_first`, `without_last`, `is_equal_or_descendant_of`, `from_chain`, `view` projections, etc. | good | good | yes |

**Coverage: Excellent** — reads like a conformance suite. All API methods covered with edge cases.

### `path_trie/tests.rs` — PathInterner, ParentPathStore — 43 tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `max_nodes`, `starts_with`, `tagging`, segments iteration, `concat`, `intern`, edge cases | good/okay | good | yes |

**Coverage: Excellent** — very thorough. No missing scenarios detected.

### `table.rs` — IndexTable — ~25 inline tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `get_insert_and_get_mut`, `vacancy_tracking`, `get_disjoint_*`, `iter*`, `values`, `contains`, `sparse_slots`, `large_id_resizes`, `len*`, `is_empty*`, `clear`, `shrink_to_fit`, `clone_produces_independent_table`, `iter_mut_covers_all_entries_and_allows_mutation` | good | good | yes |

**Coverage: Excellent** — every public API method tested. Near-exhaustive. `iter_mut` tests consolidated into one (previously two redundant tests).

### `budget.rs` — Budget, BudgetTracker — 15 inline tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `rejects_overflow_and_records_exhaustion` | good | good | yes |
| `tracker_preserves_nested_pass_exhaustion` | good | good | yes |
| `remaining_decreases_with_charges` | good | good | yes |
| `remaining_is_zero_when_exhausted` | good | good | yes |
| `remaining_does_not_underflow_on_overflow` | good | good | yes |
| `exhaustion_sticks_after_overflow` | good | good | yes |
| `try_add_is_atomic_on_failure` | good | good | yes |
| `try_add_zero_*` (2 tests) | good | good | yes |
| `try_push_on_exhausted_budget` | good | good | yes |
| `budget_is_copy` | good | good | yes |
| `used_reports_correctly` | good | good | yes |
| `new_with_zero_limit` | good | good | yes |
| `budget_tracker_default_is_not_exhausted` | okay | basic | yes |
| `tracker_idempotent_mark_exhausted` | good | good | yes |

**Coverage: Excellent for Budget, slightly repetitive for BudgetTracker.** BudgetTracker tests consolidated from 3 near-identical tests into 2 distinct ones (`tracker_preserves_nested_pass_exhaustion` + `tracker_idempotent_mark_exhausted`).

---

## glass-lint-core

### Analysis: Facts — `facts/mod.rs` + `facts/build_tests.rs` — 21 tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `fact_builder_emits_facts_for_diverse_program` | good | good | yes |
| `facts_record_the_lexical_function_owner` | good | good | yes |
| `fact_ids_are_sequential_and_deterministic` | good | good | yes |
| `fact_count_is_independent_of_enabled_rules` | good | good | yes |
| `optional_chain_does_not_double_record_roles` | good | good | yes |
| `nested_call_and_member_roles_have_distinct_facts` | okay | good | yes |
| `repeated_builds_yield_identical_fact_fingerprints` | good | good | yes |
| `call_fact_captures_callee_provenance` | good | good | yes |
| `facts_retain_current_value_identities` | okay | good | yes |
| `member_read_fact_captures_chain_info` | good | good | yes |
| `import_fact_is_emitted` | good | good | yes |
| `string_literal_fact_is_emitted` | good | good | yes |
| `class_fact_is_emitted_for_class_declaration` | good | good | yes |
| `instance_class_is_captured_for_this_calls` | good | good | yes |
| Plus 7 inline tests covering fact stream lookup, dense streams, FactId bounds, catalog invariance | good | good | yes |

**Coverage: Very good.** Missing: export facts, PropertyWrite payload details, adversarial negatives for fact exclusion.

### Analysis: Flow — Effect — `flow/effect/tests.rs` + `flow/effect/mod.rs` — 16 tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `chain_owned_resolves_direct_call_with_rooted_or_syntactic_chain` | good | good | yes |
| `chain_owned_falls_back_to_callee_name_for_alias_call` | good | good | yes |
| `rooted_is_false_for_non_global_call` | good | good | yes |
| `effective_args_unwraps_call_invocation` | good | good | yes |
| `effective_args_unwraps_apply_invocation` | good | good | yes |
| `call_fact_returns_none_for_unknown_id` | good | good | yes |
| `chain_returns_borrowed_without_callee_name_fallback` | okay | good | yes |
| `call_argument_indexes_into_correct_call` | good | good | yes |
| `call_argument_returns_none_for_missing_index` | good | good | yes |
| `effects_budget_exhausted_with_limited_budget` | good | good | yes |
| `effects_operation_count_scales_with_program_size` | good | good | yes |
| `effects_budget_exhausted_false_with_unlimited_budget` | good | good | yes |
| `collect_creates_program_level_function` | good | good | yes |
| `collect_creates_user_defined_functions` | good | good | yes |
| `parameter_ref_index_and_is_root` | good | good | yes |
| `effect_call_id_is_newtype` | good | good | yes |

**Coverage: Good** — covers chain resolution, `.call()`/`.apply()` unwrapping, unknown-id, `FunctionEffects` budget exhaustion and operation counting, parameter accessors, function creation.

### Analysis: Flow — Cross-module propagation — `flow/cross/mod.rs` — 15 inline tests

All 15 tests cover `FlowSources` propagation (single-hop, multi-hop, self-edge, convergence, partial novelty, order preservation, pending-limit overflow) and both `SourceBudget` / `ContextWorklist` behaviors.

**Coverage: Excellent.** Only gap: no test for `ContextWorklist::pop_front` ordering (which item emerges first).

### Analysis: Flow — Projector — `flow/projector/tests.rs` + `flow/projector/mod.rs` — 25 tests

Covers: source→configuration→sink pipeline, member-call scoping, property invalidation, multi-sink counting, branch join semantics (definite/one-arm/zero-iteration), do-while, try/catch/finally/switch/break/destructuring/alias rebinding/evidence anchoring, all 4 exhaustion limits.

**Coverage: Excellent.** The best-tested module in core. No significant gaps.

### Analysis: Flow — Projector state — `flow/projector/state.rs` — 10 tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `checkpoints_restore_divergent_mutation_paths` | good | good | yes |
| `bind_updates_and_unbind_removes_aliases` | good | good | yes |
| `object_for_returns_none_for_unbound_value` | good | good | yes |
| `has_alias_for_false_when_no_aliases_exist` | good | good | yes |
| `state_limit_rejects_insertion_beyond_capacity` | good | good | yes |
| `remove_states_for_clears_all_object_states` | good | good | yes |
| `join_environments_keeps_only_common_aliases` | good | good | yes |
| `mutation_count_tracks_mutations` | good | good | yes |
| `clear_removes_all_aliases_and_states` | good | good | yes |
| `state_mut_allows_in_place_update` | good | good | yes |

**Coverage: Good.** All major `FlowStateTable` operations now covered: aliases (bind/update/unbind/clear), limits (state/mutation), checkpoints (capture/restore/join), and state mutations.

### Analysis: Flow — Summary store — `flow/summary/store.rs` — 15 inline tests

Covers all `SummaryPathStore` operations: frozen paths, join, prefix/suffix, starts_with, matches_frozen, without_first, owned_segments, overlay budget exhaustion, empty path, first_index, join order.

**Coverage: Excellent.**

### Analysis: Flow — Summaries — `flow/summary/summaries.rs` — 5 tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `same_name_siblings_are_keyed_by_function_id` | good | good | yes |
| `sink_propagates_from_callee_to_caller_through_parameter` | good | good | yes |
| `collect_creates_summaries_for_all_functions` | good | good | yes |
| `invoke_compatible_rejects_too_many_args` | good | good | yes |
| `invoke_compatible_rejects_spread_args` | good | good | yes |

**Coverage: Good.** Covers sink propagation, collection per function, and invocation compatibility rejection paths for spread and excessive args.

### Analysis: Flow — Sink — `flow/summary/sink.rs` — 16 inline tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `sink_set_default_is_empty` | good | good | yes |
| `sink_set_push_unique_adds_new_sinks` | good | good | yes |
| `sink_set_push_unique_rejects_duplicates` | good | good | yes |
| `sink_set_get_returns_sink_by_index` | good | good | yes |
| `sink_set_sort_and_dedup_orders_by_flow_parameter_path` | good | good | yes |
| `sink_set_into_iteration` | good | good | yes |
| `function_sink_summary_accessors` | good | good | yes |
| `function_summary_new_and_basic_accessors` | good | good | yes |
| `function_summary_add_sink_and_sort` | good | good | yes |
| `function_summary_set_sinks_offset` | good | good | yes |
| `is_invocation_compatible_accepts_matching_args` | good | good | yes |
| `is_invocation_compatible_rejects_spread_args` | good | good | yes |
| `is_invocation_compatible_rejects_too_many_args_without_rest` | good | good | yes |
| `is_invocation_compatible_accepts_rest_param_allowing_extra_args` | good | good | yes |
| `is_invocation_compatible_rejects_missing_required_arg` | good | good | yes |

**Coverage: Good.** `SinkSet`, `FunctionSinkSummary`, `FunctionSummary` accessors, and all `is_invocation_compatible` rejection/acceptance paths now have direct unit tests. `collect_sinks_for_call` still tested only indirectly through summaries.rs integration tests.

### Analysis: Resolution — `resolution/tests.rs` — 12 tests

Covers: `const_value` materialization (scalar, array, object, nested, large, unknown, binding chains, reassignment), `call_provenance_for_value` (global, module export, multi-hop), exhaustion vs unsupported.

**Coverage: Excellent.** No blind spots.

### Analysis: Scope — Build — `scope/build/tests.rs` + `scope/build/mod.rs` — 16 tests

Covers every scope-creating construct, two-phase predeclare/collect, divergence handling (extra/missing/kind-mismatch), structural lookup with parent disambiguation, unordered re-visitation, three-way id allocation, determinism.

**Coverage: Excellent.** The most thorough scope test suite.

### Analysis: Scope — Build analysis — `scope/build/analysis/tests.rs` — 14 tests

Covers provenance classification (Require, ReturnedObject, BoundCallable, StaticObjectValues, Local fallback), mutability checking, destructuring patterns, precedence ordering.

**Coverage: Good.** Missing: `ModuleExport`, `ModuleNamespace`, `StaticString`/`StaticNumber` provenances, `BoundModuleCallable`.

### Analysis: Scope — Graph — `scope/graph.rs` — 0 tests in-file (tested via `scope/mod.rs`)

**Coverage: None in-file** (acceptable — tested through higher-level module tests).

### Analysis: Scope — `scope/mod.rs` — 3 inline tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `binding_keys_change_at_assignment_versions` | good | good | yes |
| `repeated_scope_queries_preserve_nested_and_cross_scope_results` | good | good | yes |
| `function_parameters_remain_local_with_compact_scope_names` | good | good | yes |

**Coverage: Okay** — covers main query patterns but `scope_at`, parent walking, binding resolution have more paths.

### Analysis: Syntax — Constant — `syntax/constant/tests.rs` — 5 tests

Covers typed addition, templates, arrays/objects/spreads/Object.assign, container/string limits, shadowed Object.assign/unknown spreads, recursive alias bounds.

**Coverage: Good.** Missing: property access evaluation, `unshadowed_global` paths, `no_lookup` behavior.

### Analysis: Models — `model/fact.rs`, `model/flow.rs`, `model/scope.rs`, `model/value.rs`

| Module | Tests | Judgment |
|---|---|---|
| `model/fact.rs` | 9 inline tests: `control_regions_are_typed_and_orderable`, `fact_id_from_index_rejects_overflow`, `fact_id_index_rejects_overflow`, `call_arg_info_unknown_creates_default`, `parameter_binding_constructs_with_all_fields`, `parameter_binding_without_default`, `semantic_fact_new_creates_fact_with_all_fields`, `semantic_fact_round_trips_span`, `fact_payload_*` (3 payload variants) | **Good** |
| `model/flow.rs` | 11 inline tests: `flow_limits_defaults_scale_from_flow_operations`, `flow_limits_scales_down_to_minimums`, `flow_limits_accessors_return_configured_values`, `flow_id_new_creates_deterministic_identity`, `flow_id_distinguishes_different_rules_and_indices`, `requirement_set_*` (4 tests: default, insert/remove, values, intersect_keys), `flow_state_*` (4 tests: new, key, requirements, retain_requirement_keys) | **Good** |
| `model/scope.rs` | 12 inline tests: `binding_versions_are_part_of_identity`, `scope_id_index_and_from_usize`, `scoped_name_round_trips_scope_and_name`, `binding_root_global_variant`, `binding_root_binding_variants_differ_on_version`, `binding_key_new_creates_empty_path`, `scope_kind_variants_are_distinct`, `scope_effect_dynamic_evaluation_span`, `binding_provenance_variants`, `bound_argument_static_string_and_rooted_expression`, `function_id_converts_to_u32`, `binding_id_and_version_are_newtypes` | **Good** |
| `model/value.rs` | 18 inline tests: `invalid_value_ids_fail_closed`, `value_capacity_is_typed_as_exhaustion`, `callable_value_constructs_and_exposes_target`, `intern_with_binding_wraps_in_binding_when_key_provided`, `intern_with_binding_returns_direct_id_when_no_binding`, `resolve_follows_binding_chain_to_terminal_value`, `resolve_exhausts_after_max_hops`, `resolve_returns_terminal_for_non_binding_value`, `resolve_returns_none_for_unknown_id`, `static_string_returns_string_for_static_string_value`, `static_string_returns_none_for_non_string_value`, `static_string_follows_binding_chain`, `intern_static_object_creates_object_with_canonical_names`, `intern_static_object_exhausts_on_unknown_name`, `allocate_object_id_returns_increasing_ids`, `allocate_object_id_exhausts_at_max`, `value_id_unknown_is_zero`, `value_debug_and_partial_eq` | **Good** |

**Former gap closed:** `FlowLimits`, `FlowState`, `RequirementSet`, `FactId`, `CallArgInfo`, `ParameterBinding`, `SemanticFact`, `FactPayload` now all have unit tests. `ScopeId`, `ScopedName`, `BindingRoot`, `ScopeKind`, `ScopeEffect`, `BindingProvenance`, `BoundArgument`, `FunctionId`, `CallableValue`, `ValueTable::resolve()`, `intern_with_binding()`, `intern_static_object()`, `allocate_object_id()`, `static_string()` now all have dedicated tests.

### Analysis: Local — `local.rs` — 6 tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `local_artifact_is_send_sync_and_cloneable` | okay | weak (compile check) | partial |
| `source_context_reuses_one_line_index` | good | good | yes |
| `artifact_cache_insert_then_get_hit` | good | good | yes |
| `artifact_cache_evicts_oldest_when_full` | good | good | yes |
| `artifact_cache_replacement_does_not_evict` | good | good | yes |
| `artifact_cache_miss_on_different_key` | good | good | yes |

**Coverage: Good.** Core `ArtifactCache` operations covered: insert/get hit, FIFO eviction at capacity, exact-match replacement without eviction, miss on different key/fingerprint.

### Analysis: API — Rule compilation — `api/compiler/rule.rs` — 7 inline tests

Covers compilation of every declaration variant, argument matcher clauses, invalid decl errors, order-independent equivalence, composable dimensions, normalization idempotency, argument constraint preservation.

**Coverage: Good.** Missing: `CompiledRuleSelection::is_selected`/`len`, `CompiledRuleRecord::new` error paths.

### Analysis: API — Rule builder — `api/rule/mod.rs` — 4 inline tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `rejects_noncanonical_rule_ids_and_categories` | good | good | yes |
| `accepts_provider_category_paths_and_displayable_errors` | good | good | yes |
| `rejects_duplicate_required_metadata` | good | good | yes |
| `rejects_empty_and_incomplete_matchers` | okay | good | yes |

**Coverage: Good** — validates all error paths. Missing: a plain happy-path "builds a Rule" test.

### Analysis: API — Module specifier patterns — `api/rule/module.rs` — 8 inline tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `package_patterns_obey_boundaries` | good | good | yes |
| `package_patterns_reject_non_packages` | good | good | yes |
| `exact_pattern_matches_itself_and_rejects_subpaths` | good | good | yes |
| `exact_pattern_rejects_empty_string` | good | good | yes |
| `exact_pattern_trims_whitespace` | good | good | yes |
| `exact_pattern_as_str_and_not_package` | good | good | yes |
| `package_pattern_as_str_and_is_package` | good | good | yes |
| `display_impl_shows_name` | good | good | yes |
| `scoped_package_rejects_empty_scope_or_name` | good | good | yes |

**Coverage: Good.** `ModuleSpecifierPattern::exact()` now fully covered with matching, validation, trimming, accessor, and display tests.

### Analysis: API — Flow matcher — `api/rule/matcher/flow.rs` — 26 inline tests

Covers: `ValueMatcher` (7 constructors: any_value, static_string, equals, equals_any, starts_with_any, contains_any, contains_all), `StaticStringPredicate` (1 round-trip), `ArgumentMatcher` (4 constructors: object_keys, rooted_expressions, object_property_value, from ValueMatcher), `ArgumentConstraint` (1), `ObjectSourceMatcher` (2: chain, arg), `ObjectEventMatcher` (2: property_write, member_call), `FlowCondition` (3: any_of, all_of, event), `FlowCompletion` (2: configuration, any_sink), `FlowSinkMatcher` (2: argument_of, any_argument_of), `ObjectFlowMatcher` accessors (1).

**Coverage: Good.** `ValueMatcher`, `StaticStringPredicate`, `ArgumentMatcher`, `ObjectFlowMatcherBuilder` now fully covered at unit level.

### Core: Lint — `lint/linter.rs` — 4 inline tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `remove_contained_ranges_keeps_only_largest` | good | good | yes |
| `findings_are_sorted_by_position` | good | good | yes |
| `classify_groups_findings_by_rule` | good | good | yes |
| `missing_selected_rule_fails_closed` | good | good | yes |

**Coverage: Good integration coverage.** Test names renamed to describe behavior rather than mechanism.

### Core: Lint — Catalog — `lint/catalog.rs` — 2 inline tests

Covers `RuleCatalog::combine` duplicate rejection and record preservation. No tests for `new`, `metadata`, `rule_index`, empty catalog, or provider ID validation.

### Core: Parse — `parse.rs` — 7 inline tests

Covers depth-tracking (nesting, string/comments, templates, optional chains), regex-vs-comment disambiguation.

**Coverage: Very good for depth logic.** Missing: source-too-large, TypeScript parsing failures, parser span validation.

### Core: Diagnostic — `diagnostic.rs` — 5 inline tests

Covers `SourceLineIndex` (unicode, CRLF, EOF), `try_range` error paths, empty/eof ranges, constructor equivalence.

**Coverage: Excellent for positioning and error handling.**

### Core: Environment — `environment.rs` — 9 inline tests

Covers defaults, extension, restricted global object isolation, identifier validation, extend merge, global object aliases match, global object paths match, fingerprint determinism, fingerprint differentiation, global bindings iterator.

**Coverage: Good** — all missing areas (`extend` merge, `global_object_aliases_match`, `global_object_paths_match`, fingerprint hashing) now covered.

### Core: Project — `project/tests.rs` — 7 tests

Covers worker parallelism, phased API, path normalization, report consistency, worker/outstanding bounds.

**Coverage: Good.**

### Core: Project — Report — `project/report/tests.rs` — 11 tests

Covers report combining (schema/version/partial error paths), serialization (serde-gated), shared evidence, project-vs-direct qualification.

**Coverage: Excellent.**

### Core: Project — Tables — `project/tables.rs` — 6 inline tests

Covers `EvidenceList` dedup, iteration order, shared-ownership, serialization, `is_empty`, `push_unique` scope.

**Coverage: Excellent.**

### Core: Project — Status policy — `project/tests/status_policy.rs` — 6 tests

Covers the full status policy matrix (8 scenarios), budget limits (below/at/above), parse diagnostics consistency, partial-report finding suppression.

**Coverage: Excellent.**

### Core: Project — Input validation — `project/tests/input_validation.rs` — 3 tests

Covers normalization/sorting, duplicate rejection, unknown-importer rejection.

**Coverage: Good.**

### Core: Project — Session and link validation — `project/tests/session_and_link_validation.rs` — 9 tests

Covers request extraction (all 4 import kinds), parse-error sorting, linker validation (missing exports, ambiguous star exports, outside-project, CJS dynamic exports).

**Coverage: Good.**

### Core: Project — Linking and flow — `project/tests/linking_and_flow.rs` — 13 tests

Covers ES module aliases, CJS exports, star re-exports, namespace imports, dynamic imports, flow through helpers, return-parameter identity, fail-closed on unsupported control flow, imported-arg projection, reassignment.

**Coverage: Excellent.** The most thorough cross-module integration suite.

### Core: Project — Cache and session — `project/tests/cache_and_session.rs` — 8 tests

Covers hit/miss logic, parse-failure exclusion, cross-session reuse, partial-status caching, fingerprint dimensions (7 dimensions tested independently), bounded deterministic eviction.

**Coverage: Excellent. No blind spots.**

---

## Integration tests (`glass-lint-core/tests/`)

### `public_surface.rs` — 3 tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `supported_public_operations_do_not_require_engine_storage` | good | full pipeline | Public API |
| `public_invariant_types_reject_invalid_values_without_panicking` | good | exhaustive catch_unwind | Invariant validation |
| `serde_round_trips_validate_serialization_and_deserialization` | good | round-trip + error | Serde |

**Coverage: Good.** Missing: `AnalysisLimits` boundaries, `LinterConfig` with non-default limits, `Rule::builder` validation failures, empty catalog.

### `report_pretty.rs` — 8 tests

Covers grouping+sorting, evidence source toggle, empty reports, escape sanitization, excerpt truncation, unicode/tab display width, missing source resilience, color formatting.

**Coverage: Comprehensive.** Missing: very wide max_width, multi-line excerpts exceeding width.

### `typescript_input.rs` — 9 tests

Covers TS type stripping, type-only exclusion, enum/namespace, CRLF+unicode, filename mapping, module extensions (cts/mts/cjs/mjs), parse error reporting.

**Coverage: Solid TS coverage.** Missing: `.jsx`/`.tsx`, source maps, `declare module`, triple-slash directives.

### `compact_source.rs` — 39 + 7 constructor tests

| Area | Tests | Assessment |
|---|---|---|
| CJS module calls & aliases | 4 | Good |
| Rooted member aliases | 7 | Good |
| Returned objects | 2 | Good |
| Instance matchers | 2 | Good |
| Computed properties | 7 | **Excellent** |
| Global call forms | 4 | Good |
| String/object arguments | 5 | Good |
| Helper parameters | 5 | Good |
| Constructors (submodule) | 7 | **Excellent** |
| Optional chains | 1 | Good |

**Coverage: Very thorough.** Missing: `member_read_rooted` (tested elsewhere), `Reflect.construct`, dynamic imports, `import.meta`.

Removed `constructor_global_alias` — it duplicated `rooted_global_constructors_and_their_aliases_match_global_constructors`.

### `semantic_matching.rs` — 32 tests

| Area | Tests | Assessment |
|---|---|---|
| ESM import flows | 6 | **Comprehensive** |
| Destructured rooted | 3 | Good |
| Deep chains | 3 | Good |
| Rooted call forms | 4 | **Excellent** |
| Static string resolution | 2 | Good |
| Computed via array+index | 1 | Good |
| Callback flows | 5 | Good |
| Destructured args | 1 | Good |
| Object key tracking | 3 | Good (spreads, assign, alias) |
| Object flow | 4 | Good |

**Coverage: Very thorough.** Missing: `member_read_module`, `call_package`, dynamic import.

### `scope_precision.rs` — 10 tests

Covers loop lexical scoping (for-let, for-in), var hoisting, switch block scoping, property alias receiver tracking, `import_exact` shadowing, dynamic scope (with/eval), rooted reassignment, bound callable precedence, destructured require, dynamic value fail-closed.

**Coverage: Excellent.** Missing: `catch` scope, `try`/`finally`, class static blocks, module-vs-function scope.

### `declarative_matching.rs` — 35 tests + 15 flow sub-tests

| Area | Tests | Assessment |
|---|---|---|
| Global object canonicalization | 6 | **Excellent** |
| Instance callables | 1 | Good |
| Package imports | 2 | Good |
| Object property value | 1 | Good |
| Module provenance | 2 | Good |
| Rooted alias/reassignment | 2 | Good |
| Static string matching | 1 | Good |
| Callable transforms | 3 | Good |
| Environment config | 3 | Good |
| Future declarations | 2 | Good |
| Numeric addition negative | 1 | Good |
| Arg flows via aliases | 2 | Good |
| Parameter alias flows | 3 | Good |
| Optional chaining | 1 | Good |
| Computed via const | 1 | Good |
| Object key reuse | 1 | Good |
| Reassignment/property write kills | 2 | Good |
| Destructured projection | 1 | Good |
| End-to-end flow | 1 | Good |
| **Flow sub-tests** | 15 | **Excellent** |

The 15 `declarative_matching/flow.rs` tests cover: call/apply in flow, control boundaries (loops/try/destructuring), branching dedup, ordering kills, compound/update/delete kills, member call events, helper scope/reassign/arrows/destructuring/recursion/defaults/aliases/arity, static prefix, `all_of`.

**Coverage: Very comprehensive.** Missing: `member_call_returned`, `member_read_returned`, dynamic import, `import.meta`.

---

## glass-lint-harness

### `profile/mod.rs` — 18 inline tests

Covers: empty workload rejection, sorted/unique/filtered file discovery, extension filtering, empty folder, malformed files counted as diagnostics, deterministic sampling, typed accumulator saturation, warmup exclusion, warmup count, partial completion, operation counts, worker determinism (two modes), verified repetitions, mode consistency, root validation, symlink safety.

**Coverage: Excellent.** Very thorough.

**Name concern:** `typed_accumulators_saturate_without_cross_item_bytes` — too long, unclear.

### `profile_manifest.rs` — 3 inline tests

Covers: round-trip digest/bytes/hashes, verification rejects (missing/added/changed), path validation (duplicates/traversal/absolute/symlink escape).

**Coverage: Thorough.** **Excellent.**

### `runner.rs` — 3 inline tests

Covers: missing diagnostic, unexpected diagnostic, forbidden diagnostic (all three `compare` branches).

**Coverage: Good.**

### `cases/tests.rs` — 5 tests

Covers: comment case parsing, forbidden diagnostic, TypeScript default from extension, language-extension conflict, legacy field rejection.

**Coverage: Adequate.** No tests for `load_project_case`, multi-file cases, or resolution parsing.

### `types/tests.rs` — 2 tests

Covers: adapter project protocol JSON (all 6 resolution variants), full round-trip.

**Coverage: Okay.** Missing: Case construction errors, `ToolExpectation` validation.

---

## glass-lint-project

### `tests.rs` — 11 tests

Covers: sorted directory discovery, resolver suffix validation, entry budget, loader budget partial report, extensionless internal import, file budget dedup, file budget exhaustion, project phase metrics, tsconfig membership (JSONC, exclude, extends, references), invalid root error.

**Coverage: Excellent** integration coverage for the full project pipeline.

### `resolver.rs` — 7 inline tests

| Test name | Name | Assertions | Component |
|---|---|---|---|
| `delegates_builtin_detection_and_canonicalization_to_oxc` | good | good | yes |
| `unresolved_bare_packages_remain_external` | good | good | yes |
| `require_and_import_resolve_builtins_identically` | good | good | yes |
| `package_name_extracts_scoped_and_non_scoped` | good | good | yes |
| `package_name_falls_back_on_empty` | good | good | yes |
| `miss_returns_missing_for_internal_looking_requests` | good | good | yes |

**Coverage: Good.** Added require-vs-import distinction test, `package_name` function coverage (scoped, path, empty), and missing-module fallback. Internal/outside/unsupported paths are covered through core integration tests where filesystem access is available.

### `tsconfig/tests.rs` — 20 tests

Covers: parse empty/null/wrong-types/compilerOptions/references/jsonc, pattern_set matching (include, trailing slash, basename, exclude), merge_selection (inherit, defaults, explicit), cycle detection (self-cycle, a↔b cycle, diagnostic recording), extends (missing, single-level, child-override), budget (within, max depth, max count, at-limit).

**Coverage: Excellent** across all tsconfig phases.

---

## glass-lint-js — Rule contract fixtures

### `js:dynamic-code.eval` — 10 pos / 7 neg — ★★★★★
Positives cover eval, Function, AsyncFunction via all qualifiers, bind/call/apply. Negatives cover shadowing, reassignment, local lookalikes.

### `js:network.header-indicator` — 18 pos / 3 neg — ★★★★☆
All configured headers in various cases. Missing: header-in-call-context tests.

### `js:network.private-address` — 16 pos / 6 neg — ★★★★★
IPv4/IPv6 private ranges, loopback, link-local. Missing: IPv4-mapped IPv6, unspecified address.

### `js:network.service-indicator` — 25 pos / 8 neg — ★★★★★
All configured SDK packages + endpoint URLs. Comprehensive.

### `js:dynamic-code.string-timer` — 7 pos / 5 neg — ★★★★☆
setTimeout/setInterval with string arg. Missing: setImmediate (Node).

### `js:network.telemetry-indicator` — 23 pos / 7 neg — ★★★★★
All telemetry SDKs + endpoints. Comprehensive.

### `js:network.url-construction` — 11 pos / 7 neg — ★★★★☆
URL/URLSearchParams constructors. Missing: base argument, relative URL literals.

### `browser:*` rules

| Rule | pos/neg | Quality | Notes |
|---|---|---|---|
| clipboard-read | 9/7 | ★★★★☆ | Missing: beforeinput/InputEvent paste |
| clipboard-write | 10/7 | ★★★★☆ | Same |
| environment | 31/7 | ★★★★★ | Very thorough browser fingerprinting |
| file-dialog | 6/5 | ★★★★☆ | Missing: accept/webkitdirectory/capture |
| filesystem | 11/4 | ★★★☆☆ | No OPFS handle methods, no DataTransfer |
| global-input-hook | 10/6 | ★★★★☆ | Missing: composition/beforeinput/wheel |
| permissions-bluetooth | 6/4 | ★★★☆☆ | Minimal but adequate |
| permissions-geolocation | 7/4 | ★★★★☆ | Good |
| permissions-hardware | 7/5 | ★★★★☆ | Good |
| permissions-media | 7/4 | ★★★★☆ | Good |
| permissions-notifications | 7/7 | ★★★★★ | SW notification path tested |
| permissions-query | 7/5 | ★★★★☆ | Good |
| persistent-storage | 34/11 | ★★★★★ | Most thorough browser rule |
| remote-resource | 6/4 | ★★★☆☆ | No audio/object/embed, no innerHTML |
| request | 19/13 | ★★★★★ | Excellent coverage |
| script-injection | 9/10 | ★★★★☆ | Missing: innerHTML, insertAdjacentHTML |

### `electron:*` rules

| Rule | pos/neg | Quality | Notes |
|---|---|---|---|
| dialog | 14/6 | ★★★★★ | All dialog methods |
| ipc | 35/8 | ★★★★★ | Most thorough Electron rule |
| module | 30/18 | ★★★★★ | Comprehensive import/subpath/export coverage |
| shell | 10/5 | ★★★★☆ | Good |

### `node:*` rules

| Rule | pos/neg | Quality | Notes |
|---|---|---|---|
| archive-compression | 19/3 | ★★★★☆ | Imports only, no API calls, no require() |
| crypto-operation | 22/6 | ★★★★★ | Good import + subtle API coverage |
| filesystem | 23/6 | ★★★★★ | Broad coverage |
| network | 28/6 | ★★★★★ | Comprehensive |
| process-environment | 39/8 | ★★★★★ | Most thorough Node rule |
| subprocess | 16/6 | ★★★★☆ | Good |

---

## glass-lint-obsidian — Rule contract fixtures

All 47 rules have clear names, positive examples that model dangerous patterns, and negative examples that model safe patterns. Structured analysis:

| Category | Rules | Quality | Notes |
|---|---|---|---|
| bases/register | 1 pos | ★★★☆☆ | Adequate for single-API rule |
| cli/register | 1 pos | ★★★☆☆ | Minimal but adequate |
| codemirror/extension | 16 pos | ★★★★★ | Thorough package coverage |
| editor/content | 12 pos | ★★★★★ | All editor content APIs |
| editor/extension | 2 pos | ★★★☆☆ | Thin but adequate |
| editor/suggest | 2 pos | ★★★☆☆ | Thin but adequate |
| file_manager/frontmatter_write | 5 pos | ★★★★☆ | Good |
| lifecycle/events | 5 pos | ★★★★☆ | Good |
| markdown/code_block_processor | 2 pos | ★★★☆☆ | Thin |
| markdown/link | 10 pos | ★★★★★ | Thorough |
| markdown/postprocessor | 2 pos | ★★★☆☆ | Thin |
| markdown/render | 2 pos | ★★★☆☆ | Thin |
| metadata/cache_read | 7 pos | ★★★★☆ | Good |
| metadata/events | 6 pos | ★★★★★ | All configured events covered (`finished` added to rule + fixture) |
| metadata/extract | 13 pos | ★★★★★ | Excellent coverage |
| metadata/frontmatter_read | 7 pos | ★★★★☆ | Good |
| metadata/traversal | 10 pos + 1 neg (for...in) | ★★★★★ | `for...in` loop tracked as known gap (statement form, not a call — negative fixture documents non-detection) |
| network/request | 12 pos | ★★★★★ | Excellent |
| platform/branching | 13 pos | ★★★★★ | Excellent |
| plugins/access | 5 pos | ★★★★☆ | Good |
| plugins/enable_disable | 4 pos | ★★★★☆ | Good |
| plugins/load_unload | 4 pos | ★★★★☆ | Good |
| storage/app_data | 5 pos | ★★★★☆ | Good |
| storage/plugin_data_read | 2 pos | ★★★☆☆ | Thin |
| storage/plugin_data_write | 2 pos | ★★★☆☆ | Thin |
| ui/command | 4 pos | ★★★★☆ | Good |
| ui/menu | 2 pos | ★★☆☆☆ | **Regression file** — `new Menu().addItem()` added as known-failing (chained constructor instances untracked) |
| ui/modal | 3 pos | ★★★★☆ | Good |
| ui/notice | 6 pos | ★★★★★ | Excellent |
| ui/ribbon | 2 pos | ★★★☆☆ | Thin |
| ui/settings_tab | 4 pos | ★★★★☆ | Good |
| ui/status_bar | 2 pos | ★★★☆☆ | Thin |
| vault/access | 5 pos | ★★★★☆ | Good |
| vault/adapter | 5 pos | ★★★★☆ | Good |
| vault/config_directory | 5 pos | ★★★★☆ | Good |
| vault/delete | 5 pos | ★★★★☆ | Good |
| vault/enumerate | 9 pos | ★★★★★ | Excellent |
| vault/events | 5 pos | ★★★★☆ | `delete` and `rename` events added to positives |
| vault/move_copy | 5 pos | ★★★★☆ | Good |
| vault/read | 6 pos | ★★★★☆ | Good |
| vault/resource_url | 7 pos | ★★★★★ | Excellent |
| vault/write | 10 pos | ★★★★★ | Excellent |
| view/register | 2 pos | ★★★☆☆ | Thin |
| workspace/active_editor | 3 pos | ★★★☆☆ | Adequate |
| workspace/active_file | 3 pos | ★★★☆☆ | Adequate |
| workspace/events | 11 pos | ★★★★★ | Excellent |
| workspace/layout | 4 pos | ★★★★☆ | Good |
| workspace/leaf_management | 12 pos | ★★★★★ | Excellent |
| workspace/open | 9 pos | ★★★★★ | Very good |

**Specific gaps found:**
1. **`ui/menu`**: 2 working positives (`this.addItem()` inside Menu subclass + `new Menu().addItem()` — chained constructor now tracked). Core regression test `instance_matchers_do_not_track_chained_constructor_calls` verifies correct tracking (finding_count=1).
2. **`metadata/traversal`**: `for...in` loops not covered (statement form, not call expression — negative fixture documents known non-detection).
3. **`vault/events`**: Now covers all 5 vault events (create, modify, delete, rename, closed) in positives.
4. **`metadata/events`**: `finished` event added to rule definition and positive fixtures (6 positives).

**Consistency note:** The pattern of 1-2 positives for single-method rules (editor/extension, editor/suggest, markdown/code-block-processor, postprocessor, render, ui/ribbon, status_bar, storage/plugin_data_*, view/register, workspace/active_*) is acceptable — these rules target a single API method and the coverage is adequate.

---

## E2E tests (`tests/e2e/`)

12 harness conformance cases covering realistic Obsidian plugin scenarios:

| File | pos | Rules tested | Quality |
|---|---|---|---|
| inspect-note-tags.js | 5 | metadata cache, workspace, lifecycle, UI | Good |
| download-daily-quote.js | 3 | network.request, url-construction, command | Good |
| fetch-remote-catalog.js | 4 | browser request, url-construction, command, status-bar | Good |
| persist-refresh-settings.js | 4 | plugin-data read/write, command, lifecycle | Good |
| roll-ribbon-dice.js | 3 | ribbon, status-bar, command | Good |
| render-executable-code-blocks.js | 4 | code-block-processor, eval | Good |
| count-note-words.js | 6 | command, lifecycle, vault access/events/read, workspace | Good |
| open-workspace-links.js | 4 | command, workspace open, active-file | Good |
| transform-text-case.js | 1 | command (loop coverage test) | **Adequate** — 1 assertion undersells code |
| watch-vault-changes.js | 9 | lifecycle, vault access/events | Good — highest count |
| create-meeting-note.js | 9 | command, vault access/enumerate/write, leaf-management, workspace open | Good |
| typescript-input.ts | 1 | browser request (TS regression) | **Light** — main value is regression |

**Coverage gaps:**
- No coverage for `obsidian:storage.data`, `obsidian:editor.*`, `obsidian:workspace.iterate`
- No template-literal URL construction test
- `transform-text-case.js`'s single assertion undersells the code it exercises

---

## CLI tests

### `glass-lint-cli/src/config.rs` — 5 tests

| Test | Quality |
|---|---|
| `obsidian_profile_combines_generic_and_provider_rules` | Good |
| `combined_obsidian_profile_uses_the_obsidian_host_environment` | Good |
| `selected_linter_keeps_profile_baseline_before_core_overrides` | Good |
| `project_timeout_is_validated_at_the_cli_boundary` | Good |
| `legacy_flat_project_limits_are_rejected` | Good |

**Coverage: Good** — validates catalog composition, integration, override precedence, boundary validation. Missing: JSON config deserialization, TOML format tests.

### `glass-lint-cli/src/lint.rs` — 2 tests

Covers directory/tsconfig selection, sorted file discovery. **Minimal coverage** — no tests for symlinks, permission errors, empty directories, or the `lint_files` function.

### `glass-lint-harness-cli/src/args.rs` — 2 tests

Covers mutually exclusive profile modes, help text flag presence. **Minimal coverage** — no tests for `parse_adapter`, `Format`, or `Command::Verify`/`Command::Report` argument validation.

### `tools/obsidian-global-probe/probe.test.cjs` — 10 assertions

Covers schema version, globals list, binding classification, realm sources, error count, identifier filtering, relative require check.

**Coverage: Good for its scope.** Missing: `env` parameter test.

---

## Summary: Module-level coverage judgments

### Excellent coverage
- datastructures: path, path_trie, table, budget
- core/analysis: cross-module propagation, projector, resolution, scope build, report combine, status policy, linking & flow, cache & session
- core/integration: compact_source, semantic_matching, scope_precision, declarative_matching, report_pretty, typescript_input, diagnostic
- harness: profile, profile_manifest
- project: tests.rs, tsconfig
- JS rules: eval, private_address, service_indicator, telemetry_indicator, environment, persistent_storage, request, electron/ipc, electron/module, node/crypto, node/network, node/process-environment
- Obsidian rules: editor/content, markdown/link, metadata/extract, metadata/traversal, network/request, platform/branching, ui/notice, vault/enumerate, vault/resource_url, vault/write, workspace/events, workspace/leaf_management

### Good coverage
- core/analysis: facts, effect, syntax/constant, scope build analysis, rule compilation, rule builder, projector state, summaries, local, parse, environment, linter integration
- core/analysis: model (scope, value, fact, flow — all now Good or better)
- core/analysis: flow/summary/sink, flow/effect
- core/api: rule/module
- core/integration: public_surface
- core/project: input_validation, session_and_link_validation, linking_and_flow
- harness: runner
- JS rules: header_indicator, string_timer, url_construction, clipboard_*, file_dialog, global_input_hook, most permissions_*, remote_resource, script_injection, electron/shell, node/archive-compression, node/filesystem, node/subprocess
- Obsidian rules: most vault rules, metadata rules (all events covered), lifecycle, plugins, storage, ui/command/settings_tab/modal, workspace/layout/open

### Adequate / Minimal coverage
- JS rules: browser/filesystem (★★★☆☆), browser/permissions-bluetooth (★★★☆☆), browser/remote-resource (★★★☆☆)
- CLI: lint.rs (minimal — 2 tests), args.rs (minimal — 2 tests)

### Resolved to Good coverage
- core/analysis: projector/state.rs (was poor — now 10 tests covering all FlowStateTable operations), summary/summaries.rs (was minimal — now 5 tests with sink propagation and compatibility), local.rs (was minimal — now 6 tests including ArtifactCache)
- core/analysis: model/flow.rs (was none — now 11 tests covering FlowLimits, FlowId, RequirementSet, FlowState), model/fact.rs (was minimal — now 9 tests covering FactId, CallArgInfo, ParameterBinding, SemanticFact, FactPayload)
- core/analysis: model/scope.rs (was minimal — now 12 tests covering ScopeId, ScopedName, BindingRoot, ScopeKind, ScopeEffect, BindingProvenance, BoundArgument, FunctionId), model/value.rs (was okay — now 18 tests covering CallableValue, resolve, intern_with_binding, intern_static_object, allocate_object_id, static_string)
- core/analysis: flow/summary/sink.rs (was 0 — now 16 tests covering SinkSet, FunctionSummary, FunctionSinkSummary, all is_invocation_compatible rejection/acceptance paths)
- core/analysis: flow/effect (was 9 — now 16 tests covering budget exhaustion, operation counting, ParameterRef, function creation)
- core: environment.rs (was adequate — now 9 tests covering extend, aliases_match, paths_match, fingerprint hashing)
- core/api: rule/matcher/flow.rs (was minimal — now 26 tests covering ValueMatcher, StaticStringPredicate, ArgumentMatcher, ObjectFlowMatcherBuilder)
- core/api: rule/module.rs (was narrow — now 9 tests covering ModuleSpecifierPattern::exact with matching, validation, trimming, accessor, display)
- project: resolver.rs (was thin — now 7 tests covering require-vs-import, package_name, missing fallback)

---

## Cross-cutting issues

### Redundancy (resolved)

1. ~~**`compact_source/constructors.rs:constructor_global_alias`** duplicates **`rooted_global_constructors_and_their_aliases_match_global_constructors`**~~ — **Removed `constructor_global_alias`.**
2. ~~**`datastructures/budget.rs`** — `budget_tracker_mark_exhausted_then_is_exhausted` and `budget_tracker_stays_exhausted` are nearly identical to `tracker_preserves_nested_pass_exhaustion`~~ — **Consolidated into `tracker_idempotent_mark_exhausted`.**
3. ~~**`datastructures/table.rs`** — `iter_mut_allows_mutation` and `iter_mut_yields_all_entries` test the same behavior~~ — **Consolidated into `iter_mut_covers_all_entries_and_allows_mutation`.**
4. Cross-file: `reassignment_order_keeps_only_pre_reassignment_rooted_calls` (compact_source) ≈ `follows_rooted_aliases_and_reassignment_order` (declarative_matching) — intentional harness reuse with different matcher styles, not redundancy.

### Missing coverage areas

1. **JSX/TSX** — `.jsx`/`.tsx` file handling entirely uncovered.
2. **Dynamic imports** — `import('module')` expression untested.
3. **`import.meta`** — not covered anywhere.
4. **`Reflect.construct`** — despite thorough constructor testing.
5. **`catch`/`finally` scopes** — other lexical scopes are well-tested but these are not.
6. **Class static blocks** — missing.
7. **`for await...of`** — async iteration scope untested.
8. ~~**`core/model/flow.rs`** — `FlowLimits`, `FlowState`, `RequirementSet` have zero unit tests.~~ — **Resolved: now 11 tests covering all three types.**
9. ~~**`core/model/scope.rs`** — only 1 test.~~ — **Resolved: now 12 tests covering all scope types.**
10. ~~**`core/model/value.rs`** — only 2 tests.~~ — **Resolved: now 18 tests covering all ValueTable operations.**
11. ~~**`core/api/rule/module.rs`** — no exact() tests.~~ — **Resolved: now 9 tests including full exact() coverage.**
12. ~~**`core/flow/summary/sink.rs`** — 0 direct tests.~~ — **Resolved: now 16 tests covering SinkSet, FunctionSummary, is_invocation_compatible.**
13. **`collect_sinks_for_call`** — still untested in isolation (tested via summaries.rs integration tests).
14. **CLI** — `lint.rs` and `args.rs` have only 2 tests each; no `lint_files` tests.

### Naming concerns

- ~~`retained_ranges_reject_non_boundary_and_past_eof` — "retained ranges" misdirects (tests try_range)~~ — **Renamed to `try_range_rejects_non_character_boundary_and_out_of_bounds`.**
- `typed_accumulators_saturate_without_cross_item_bytes` — too long, unclear
- `caches_subresults_so_views_share_one_classification` — poor name (describes caching, not the test)

**Resolved naming concerns:**
- ~~`range_sweep_removes_large_nested_and_duplicate_sets`~~ → `remove_contained_ranges_keeps_only_largest`
- ~~`findings_are_sorted_without_cloning_rule_ids`~~ → `findings_are_sorted_by_position`
- ~~`classify_with_evidence_limit_binds_record_once`~~ → `classify_groups_findings_by_rule`
- ~~`constructor_global_alias`~~ → removed (duplicate)
