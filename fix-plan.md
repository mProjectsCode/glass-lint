# Fix plan for the new declarative matcher regressions

## Goal and current baseline

Make the new provider-neutral matcher contracts pass without adding rule-specific traversal or weakening strict provenance. Keep fact construction query-independent, preserve deterministic evidence, and keep all changes in `glass-lint-core`.

The initial `make ci` run passes workspace checking, Clippy, all core unit tests, and every integration test before `matching::declarative`; it then reports the eight failures introduced by commit `6b6d81a`. The independent harness gates still pass:

- `make test-e2e`: 14/14 cases passed.
- `make test-rules`: 70/70 JavaScript and 98/98 Obsidian cases passed.

Five adjacent gaps and one repeated-write edge case were added to `glass-lint-core/tests/integration/matching/declarative.rs`. The focused suite now has 14 intentional failures and 55 passing tests. Implement the fixes below as one coherent core migration, then leave no compatibility-only path behind.

## 1. Resolve rooted property-write receivers before the write

Failing tests:

- `rooted_property_writes_follow_receiver_aliases`
- `rooted_property_writes_keep_receiver_identity_after_prior_writes` (added during investigation)

Root cause: `FactBuilder::record_member_assignment` correctly asks `FrozenScopeGraph::rooted_write_member_chain` for write-specific provenance, but that method delegates alias resolution to `resolve_member_chain`. The ordinary read resolver considers property-alias mutations at or before the use. The current assignment is therefore seen as an alias entry with no target and returns `None`; merely changing `<=` to `<` would still lose the path after an earlier write to the same property. A write changes the property value, not the already-proven identity of its receiver.

Implementation:

1. In `analysis/scope/query/provenance/chain.rs`, split receiver resolution from property-read availability. Add a write-specific path that resolves the receiver binding/alias at the assignment position and appends the statically known property without consulting writes to that property.
2. Continue honoring receiver shadowing, receiver reassignment, dynamic computed properties, dynamic scopes, global-object canonicalization, and mutations that replace an ancestor/receiver. Do not reuse the relaxed write path for member reads or calls.
3. Keep `analysis/facts/assignments.rs` as the sole emitter of `FactPayload::PropertyWrite`, with the canonical alias-expanded `rooted_chain` stored on the fact before downstream invalidation.
4. Add focused scope/fact tests for a direct alias, repeated writes, a multi-hop alias, receiver reassignment to a local object, a shadowed lookalike, and a dynamic property. Assert exact rooted paths and source order.

Acceptance: both writes through `const nav = navigator` are indexed as `navigator.onLine`; reads/calls after invalidating mutations remain fail-closed.

## 2. Index heuristic constructor spellings independently of strict provenance

Failing tests:

- `heuristic_constructors_match_unconfigured_names`
- `heuristic_constructors_follow_transparent_callee_wrappers` (added during investigation)

Root cause: construction facts retain a `callee_name` only for direct identifier/member shapes, and `OccurrenceIndexes::record_construction_fact` adds that name to the heuristic constructor index only when provenance is `Global`. Heuristic calls already index their syntactic spelling independently of environment/provenance; heuristic constructors do not. Parenthesized and sequence-wrapped constructor targets also lose their spelling in `visit_new_expr`.

Implementation:

1. In `analysis/facts/visitor.rs`, resolve the effective constructor target through the same transparent parenthesis/sequence logic used for calls. Store its stable identifier spelling and effective span on `FactPayload::Construction`; unsupported/dynamic targets keep `callee_name = None`.
2. In `analysis/matching/build.rs`, always add a present `callee_name` to `constructions.constructors`. Continue populating `global_constructors` and `module_constructors` only from proven strict provenance.
3. Add fact/index tests covering direct, parenthesized, and sequence-wrapped heuristic constructors. Add negatives for computed/dynamic constructor expressions. Re-run existing strict global/module shadowing tests to prove the heuristic index does not leak into strict indexes.

Acceptance: heuristic constructor queries behave like heuristic call queries, while `constructor_global` and `constructor_module` retain their current strict identity rules.

## 3. Model default imports as callable default exports while preserving member behavior

Failing tests:

- `default_imports_preserve_module_export_identity`
- `default_import_aliases_preserve_module_export_identity` (added during investigation)

Root cause: `for_each_import_binding` classifies `ImportSpecifier::Default` as `BindingProvenance::ModuleNamespace`. Direct calls, constructions, superclass references, and aliases therefore never carry `(module, "default")` provenance. Simply changing the binding to `ModuleExport("default")` would break the existing contract `follows_default_import_namespace_members_through_aliases`, because default-import objects are also used as SDK clients for member queries.

Implementation:

1. Introduce one internal semantic representation for a default import (for example `BindingProvenance::DefaultImport { module }`) in `analysis/model/scope.rs`; do not scatter checks for the string `"default"` through the resolver.
2. Produce that provenance from `analysis/scope/build/bindings.rs` in both scope passes.
3. Centralize its contextual lowering in the scope query layer:
   - direct call/constructor/class use lowers to `SymbolCallProvenance::ModuleExport { export: "default" }`;
   - aliases preserve the default-import identity until the use site;
   - member access continues to lower to the existing module-member identity so `import sdk from "sdk"; sdk.send()` and extracted/deep member aliases keep working;
   - `.bind`, parentheses, sequences, and supported TypeScript-transparent wrappers preserve the same identity;
   - reassignment, ambiguity, and local lookalikes fail closed.
4. Update exhaustive provenance handling in `analysis/scope/query/provenance/{callable,object,chain}.rs`, scope collection/classification, and resolution helpers. Avoid a second module-provenance model.
5. Add unit tests for provenance construction and integration tests for direct and aliased call/construct/extends forms, existing default-import member calls, `{ default as Name }` parity, and alias reassignment negatives.

Acceptance: the two failing tests each produce three findings, and the existing default-import member/namespace tests remain unchanged.

## 4. Treat ordinary and optional calls as one returned-object source shape

Failing tests:

- `lifecycle_sources_follow_optional_calls`
- `returned_member_queries_follow_optional_producer_calls`
- `returned_member_queries_follow_optional_receiver_calls` (added during investigation)

Root cause: optional calls already emit one `Call` fact, but returned-object provenance and value identity only recognize `Expr::Call`. `ScopeCollector::returned_object_provenance`, `FrozenScopeGraph::returned_object_source`, and `FactBuilder::value_for_expr` omit `Expr::OptChain(OptChainBase::Call)`. As a result, the declaration target is not connected to the optional call's result ID, and later member/lifecycle events cannot correlate with the producer.

Implementation:

1. Add one bounded internal call-like expression helper in the syntax/analysis layer that exposes the effective callee, arguments, and result span for ordinary calls and both optional forms (`object.method?.()` and `object?.method()`). Use it instead of duplicating AST shape matching.
2. Extend `ScopeCollector::returned_object_provenance` in `analysis/scope/build/provenance.rs` and `FrozenScopeGraph::returned_object_source` in `analysis/scope/query/provenance/object.rs` through that helper. Preserve rooted producer identity and reject bind/dynamic/unrooted producers exactly as ordinary calls do.
3. Extend `FactBuilder::value_for_expr` in `analysis/facts/calls/mod.rs` so an optional-call initializer uses the same `call_result(span)` ID that `visit_opt_chain_expr` stores on its `Call` fact. Include transparent parentheses/sequences/TypeScript wrappers if they currently hide that same result identity.
4. Confirm the flow projector requires no optional-specific branch: it should consume the repaired declaration-to-call-result identity and existing returned/lifecycle indexes.
5. Add fact tests asserting exactly one optional `Call` fact and an identical declaration source/result ID, plus returned-member and lifecycle positives for both optional forms. Add negatives for dynamic producer properties, reassignment before the sink, disconnected objects, and optional local lookalikes.

Acceptance: all three tests match through the ordinary returned-object/lifecycle pipeline without a rule-specific traversal or optional-only matcher.

## 5. Emit dynamic-import occurrences into the canonical import fact stream

Failing test:

- `import_queries_match_dynamic_imports`

Root cause: `record_module_call_request` creates a resolver request for literal `import(...)`, but the `Callee::Import` call path emits only `FactPayload::Call`. Static imports and proven CommonJS requires also emit `FactPayload::Import`, which is the sole input to `literals.imports`; dynamic imports therefore never reach import queries.

Implementation:

1. Refactor literal module-call recognition in `analysis/facts/mod.rs` so the validated specifier/span is computed once and used for both the dynamic resolution request and a single `FactPayload::Import` emission.
2. Keep dynamic/non-literal specifiers unknown and unindexed. Do not infer a module from arbitrary runtime strings.
3. Anchor evidence consistently at the module specifier span and ensure the import fact is emitted exactly once for bare, awaited, parenthesized, and assigned dynamic imports.
4. Add fact/index tests for exact and package matching, a subpath, a non-literal negative, and deterministic ordering alongside static import/require occurrences.

Acceptance: the exact rule matches `sdk`, the package rule covers both `sdk` and `sdk/client`, producing the two expected rule findings without duplicate evidence.

## 6. Represent class declarations and class-reference operands separately

Failing tests:

- `heuristic_class_queries_match_instanceof_operands`
- `heuristic_class_queries_match_superclass_operands` (added during investigation)

Root cause: `record_instanceof` emits a class fact with provenance but `name = None`, while `OccurrenceIndexes` only indexes heuristic class names for `ClassFactRole::Declaration`. A derived-class declaration stores the child name and superclass module provenance on one fact, so there is no heuristic occurrence for the `extends` operand. The current payload conflates declaration identity with referenced superclass identity.

Implementation:

1. Make class fact roles explicit in `analysis/model/fact.rs`: retain declaration facts and add/reference operand roles for `extends` and `instanceof` (for example `SuperclassOperand` and `InstanceofOperand`). Each operand fact should carry its own static spelling, exact operand span, and optional strict module provenance.
2. In `analysis/facts/functions.rs`, emit the declaration fact for the declared class name and a separate superclass operand fact when present; do the same for class expressions. Emit `instanceof` operand names for supported identifier/member spellings. Visit children once and keep fact order deterministic.
3. In `analysis/matching/build.rs`, index heuristic class names from every supported class-name/reference role. Index module class provenance from reference operands exactly once; avoid double-counting the old declaration-plus-provenance representation.
4. Update fact model tests and the existing `module_class_references_preserve_class_provenance` contract to assert two occurrences (extends and instanceof) at their operand locations. Add local lookalike, dynamic RHS, member spelling, class expression, and shadowing coverage.

Acceptance: heuristic class queries match declarations, superclass operands, and `instanceof` operands by spelling; module class queries still require proven module provenance and emit no duplicate occurrence.

## 7. Index complete statically evaluated string expressions

Failing tests:

- `string_queries_match_constant_compositions`
- `string_queries_match_compositions_of_constant_aliases` (added during investigation)

Root cause: the bounded constant evaluator already resolves string addition and template substitutions, including constant aliases, but fact construction emits references only for literal strings, template quasis, and later identifier uses. It never emits a reference for the complete binary/template expression at its declaration, so the matcher sees `"to"` and `"ken"` but not `"token"`.

Implementation:

1. Reuse `analysis/syntax/constant` through the resolver; do not add another evaluator. Extend `Resolver::resolve_expr` (or a focused resolver-owned helper) to intern bounded static results for supported `+`, template, and transparent TypeScript expression shapes.
2. In the fact visitor, visit child expressions in evaluation order and emit one `Reference` fact for the complete expression when its resolved value is `StaticString`. Use the full expression span.
3. Preserve useful quasi references only when the complete template is unknown, or otherwise deduplicate the full/no-substitution template case. Ensure a composed expression produces one composite occurrence, not one per evaluator path.
4. Keep numeric addition, dynamic substitutions, unsupported coercions, oversized strings, and exhausted constant budgets unknown. They must not establish a witness.
5. Add fact/index tests for literal composition, constant-alias composition, nested addition, constant template substitutions, dynamic negatives, numeric addition, bounds, exact locations, and deterministic evidence order.

Acceptance: each complete `"token"` expression in the two failing tests contributes one finding, with no duplicate for its component literals or quasis.

## Suggested implementation order

1. Repair/default the shared semantic representations: default-import provenance, call-like optional expressions, and class operand facts.
2. Update fact construction for optional call result IDs, dynamic imports, constructors, class operands, property writes, and composite strings.
3. Update matcher indexes only where facts now expose the missing reusable identity/name/value.
4. Add the focused unit/adversarial tests listed above, then run the declarative integration suite.
5. Run `cargo test -p glass-lint-core`, followed by the full `make ci` gate.

## Completion checklist

- All 14 currently failing declarative tests pass.
- Existing default-import member behavior, strict global/module shadowing, optional-call deduplication, and module class counts remain green.
- New facts/indexes are built once per file and do not consult selected rules.
- Dynamic, shadowed, reassigned, ambiguous, and unsupported forms remain non-witnesses.
- Evidence spans, occurrence order, fact counts, fingerprints, and plan operation counts are deterministic; update explicit baselines only for intentional new facts/work.
- No provider names or policies enter core, no compatibility wrapper remains, and `make ci` passes.
