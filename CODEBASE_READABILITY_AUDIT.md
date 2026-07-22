# Codebase Readability Audit

## Summary

The re-audit retains 3 actionable readability and maintainability issues
across `glass-lint-core` and `glass-lint-project`: 1 high severity and 2
medium severity. Eleven findings were removed after verification: the retained
value arena, local fact-path representation, function-summary round state,
summary path storage, lazy package occurrence scans, resolver cache sharing,
core test-helper/project-test organization, the positional scope reuse plan,
the partial declaration-classification cache, the duplicated effect
argument/effective-call selection and projector chain/rootedness helpers, and
the repeated range filtering and related-evidence cloning in finding assembly
are now materially complete.

The remaining findings are partial fixes rather than newly discovered
problems. Each one still has a concrete duplicate authority, repeated
allocation, delayed invariant, or boundary that can drift; the recommendations
below describe the missing consolidation.

## Findings

### READ-001 — Scope reuse still depends on positional traversal synchronization

- **Status:** Resolved.
- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/mod.rs:114-198`, `glass-lint-core/src/analysis/scope/collect/mod.rs:540-587`

`ScopePlan` stored an ordered vector plus a cursor, and `push_scope` consumed the next entry before checking its span, kind, and parent. The checks detected divergence but did not replace positional pairing with stable node identities, and a missed entry caused a fallback scope to be allocated after the two passes had already lost alignment.

The cursor and the `ScopePlan` cursor/entry types are gone. `predeclare` now records each scope's full structural identity (`scope_id`, `kind`, `span`, `parent`) into a `ScopeShapeTable` keyed by `(parent, span_lo, kind)`. The main visitor resolves every `push_scope` by popping the next unconsumed child of the current parent from the table, so equal-span siblings consume their predeclared shapes in order and a phase mismatch simply marks the artifact diverged without allocating a fallback scope. Tests cover equal-span siblings, nested functions/arrows, hoisting, catches, loops, `with`, kind/span/parent mismatches, and a deliberately misaligned walker.

### READ-002 — Declaration classification caches only one of several repeated analyses

- **Status:** Resolved.
- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/collect/analysis.rs:29-204`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:36-94`

`DeclarationAnalysis` cached `rooted_path` only; every precedence branch independently re-ran bound-callable, module-alias, returned-object, static-object, constant, and `require` helpers, and the mutable-object precheck in `visit_var_decl` re-ran `static_object_values`/`const_provenance` before classification.

`DeclarationAnalysis` is replaced by `DeclarationFacts`, a record that owns the callable, module-alias, require, static-object, constant, returned-object, and rooted-path subresults in `DeclarationFactState` and exposes three views: `classify_declaration`, `assignment_provenance`, and `is_mutable_static_object`. Each helper is invoked exactly once per initializer, exhaustion/unknown outcomes are carried through the `Option`-typed state, and precedence is applied in one place inside each view. `visit_var_decl` now computes one `DeclarationFacts` up front, runs the mutability probe, then reuses the same state for the declaration classification after the derived-function pattern is known. `visit_assign_expr` consumes the same classifier. Unit tests in `analysis.rs` cover static-object sharing, direct `require`, root-member aliases, reassignment to a rooted member, bound-callable precedence, dynamic-call fall-through, `var`/`let`/`const` mutability, returned-object chains, destructuring patterns, destructured `require`, and constant-vs-bound-callable precedence. Integration tests in `tests/scope_precision.rs` cover reassignment, bound-callable reassignment, destructured `require`, and dynamic-call fall-through.

### READ-003 — Resolver constant materialization still clones whole arena collections

- **Status:** Resolved.
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/resolution/constant.rs:12-60`, `glass-lint-core/src/analysis/resolution/mod.rs:119-130`, `glass-lint-core/src/analysis/value/arena.rs:87-180`

The `ConstAction` enum is removed. `const_value` now holds the immutable `RefCell` borrow on the value arena across the entire recursive traversal because every nested call only performs further immutable reads. Large static arrays and objects are visited by borrowed slice through `ValueTable::resolve`, which follows binding chains (bounded to 16 levels), so no vector or string clones occur for intermediate inspection. The only remaining clones are `StaticString` → `String` conversions and the final `ConstValue` materialization at the query boundary.

A new `MAX_CONST_DEPTH` constant (32) guards recursion at the `const_value_depth` entry point; exhausted depth returns `ConstValue::Unknown` at the element level. The `NameTableCtx::resolve` method (immutable read) replaces the previous `names.with_mut(...)` call for resolving object-entry name IDs. `NameId`'s inner field is now `pub(in crate::analysis)` to match `ValueId`'s visibility convention and enable direct construction in adversarial tests.

Tests cover static objects with mixed value types, unresolvable NameIds in object entries, deeply nested array structures exhausted at the depth guard, a 100-element flat array, and reassignment across distinct binding keys — all in addition to the existing binding-chain, array-with-nested-bindings, and invalid-ID tests.

### READ-004 — Argument projection still rebuilds shapes in a second traversal

- **Status:** Resolved.
- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/build/arguments.rs:39-175`, `glass-lint-core/src/analysis/syntax/constant.rs:123-220`

`arg_info` resolved the expression and conditionally ran the constant evaluator before calling `walk_argument_projections`, which then re-walked object, array, and member syntax, re-resolved descendants, and reconstructed composite values. For object literals the resolver's path went through `syntax_constant::evaluate` (converting runtime `ValueId` to `Unknown`), so the walk was both a duplicate traversal *and* the only correct source of descendant value identities.

`walk_argument_projections` is gone. `arg_info` now dispatches on the expression shape:

- **Member chains:** the resolver provides provenance and the top-level value (one cheap, cached query); `member_chain_projection` walks the chain to extract `(base_value, base_path)` without re-resolving composite shapes.
- **Object and array literals:** `analyze_argument_tree` is the *sole* traversal. It resolves every descendant via `resolve_expr_id` (preserving runtime `ValueId`), constructs one `StaticObject`/`StaticArray`, and returns `(value, base_value, base_path)` in a single pass. The resolver's constant-evaluation path is intentionally not consulted, so non-constant children keep their arena identity.
- **Paren / Seq wrappers:** transparently unwrapped before dispatch.
- **Leaf expressions:** the resolver handles resolution; `syntax_constant::evaluate` is consulted only as a string fallback when the resolver cannot intern a template literal or concatenation.

`resolve_or_eval` is the single point that applies the constant-evaluation fallback, and provenance for object/array arguments is `Local` (no module or global chain). Every existing parity test across templates, aliases, spreads, dynamic keys, nested containers, destructuring, and minified forms passes.

### READ-005 — Effects and the local projector still duplicate derived call relations

- **Status:** Resolved.
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:35-83`, `glass-lint-core/src/analysis/flow/effect.rs:236-265`, `glass-lint-core/src/analysis/flow/projector/mod.rs:108-220`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:19-58`

`record_call` cloned the derived `EffectArgument` vector into `EffectCall` and stored the same per-argument relations again in `EffectUse::CallArgument`. In addition, the projector maintained its own `projector_chain`/`projector_rooted`/`projector_effective_args` effective-call selection beside `CallEffectRef`, duplicating chain resolution, rootedness checks, and `.call()`/`.apply()` unwrapping with a slightly different callee-name fallback.

`EffectUse::CallArgument` now holds `(event, argument_index)` instead of a full `EffectArgument`, with a `FunctionEffect::call_argument` lookup helper. `record_call` pushes uses first (consuming only the index), then moves the argument vector into `EffectCall` without cloning. `CallEffectRef` is the single authority: `call_fact` returns `Option` instead of calling `expect`, `chain_owned` provides the callee-name fallback, `effective_args` handles `.call()`/`.apply()` unwrapping, and every projector query (`transfer_call`, `assign`) goes through `CallEffectRef`. The three free functions (`projector_chain`, `projector_rooted`, `projector_effective_args`) are removed. All four `cref.provenance()` call sites handle the `Option` return. Nine parity tests cover direct-call chains, callee-name fallback for aliases, `.call()`/`.apply()` effective args, rooted vs non-rooted calls, unknown facts, and index-based argument lookup against a missing call or index.

### READ-006 — Cross-flow refinement still copies entire source buckets and sweeps all edges

- **Status:** Resolved.
- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:259-301`, `glass-lint-core/src/analysis/flow/cross/mod.rs:430-498`

`FlowSources` now deduplicates with `BTreeSet`, but `extend_from_key` still materializes the complete source bucket into a temporary vector for every edge. Each refinement round also scans every module, effect, and call and uses `changed_keys` only to skip insertion, rather than driving a worklist from changed source keys; make insertion report deltas directly and index affected edges so unchanged buckets and edges are not revisited.

`extend_from_key` and `refine_through_calls` are removed. A one-shot `build_adjacency` pass (structurally identical to the old refinement scan) records every `SourceKey → [destinations]` edge in a `BTreeMap<SourceKey, Vec<SourceKey>>` index. Propagation is now a candidate-level worklist in `propagate`: all initial candidates seed a FIFO queue (deduplicated by `BTreeSet`), each round dequeues the batch and inserts each `(SourceKey, SourceCandidate)` into every destination listed in the adjacency index, and destinations that receive a new candidate are re-enqueued for the next round. No temporary vectors are materialized per edge, no module/effect/call scan is repeated per refinement round, and the `MAX_PENDING` constant (65 536) fails closed when the worklist exceeds the safety limit, complementing the existing `MAX_SOURCE_REFINEMENT_ROUNDS` budget. Unit tests cover single-edge transfer, multi-hop propagation, cycle convergence, duplicate/no-op rounds, self-edge skipping, missing sources, deterministic ordering, pending-limit exhaustion, and round-budget exhaustion.

### READ-007 — Finding assembly still rescans ranges and clones related evidence

- **Status:** Resolved.
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/findings.rs:33-111`, `glass-lint-core/src/lint/linter.rs:338-382`

`findings_for_capability` collected occurrence ranges into a `BTreeMap`, then for each retained range filtered the entire map (O(N*M) per capability). `project_findings_for_module` maintained separate `related_by_rule` and `findings_by_rule` `BTreeMap`s, then cloned pooled related evidence into every finding in a second loop.

`findings_for_capability` now collects entries from the range map into a sorted vector once. A single cursor-driven sweep assigns each entry to its containing retained range(s) by sliding a window through the entry list: entries that end before a retained range starts are skipped permanently, and scanning stops when entries start after the retained range ends. Containment checks use the same `SourceRange::contains` semantics, but each entry is visited at most once per overlapping retained range rather than once per retained-range × entry pair.

`project_findings_for_module` consolidates the two maps into one `BTreeMap<RuleIndex, (Vec<Finding>, Vec<Evidence>)>` tuple. Related evidence and findings are collected together per capability, and the attach loop iterates each tuple entry once, cloning related evidence only into the findings that share its rule index. The intermediate separate-map cross-reference is eliminated.

Existing test coverage for containment collapsing, per-location evidence, related-evidence deduplication, multi-capability findings, and the 5,000-range containment sweep all pass.

### READ-008 — Matcher-family metadata remains split between the macro, storage, and lowering

- **Status:** Resolved.
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/matcher/mod.rs:21-43`, `glass-lint-core/src/api/rule/matcher/mod.rs:58-137`, `glass-lint-core/src/api/compiler/rule.rs:230-256`

The family macro generated matcher enums, family views, push/flatten behavior, and validation/normalization dispatch, but `MatcherSet` storage remained a separate hand-maintained field list and compiler lowering remained a separate exhaustive family match. The `matcher_families!` declaration now generates the `MatcherSet` struct fields, the `Matcher` enum, family views, push/emptiness/flatten dispatch, normalization, validation, and compiler lowering from the single family list.

A `lower` hook parameter was added to every family entry, specifying its corresponding `pub(crate)` lowering function. `MatcherSet::lower_all` dispatches through the generated family match and returns the combined `Vec<QueryClause>`, replacing the exhaustive match in `QueryPlan::from_matcher`. Adding a family now requires its storage type, normalization, validation, and lowering behavior at the declaration site; the old parallel field list and separate lowering match are removed.

The existing contract test `every_family_validates_normalizes_flattens_and_compiles` and all 47 declarative matching integration tests, including `compiles_every_public_matcher_family_into_one_query` and `query_plan_compiles_public_families_into_composable_dimensions`, pass.

### READ-009 — Public flow factories can still construct invalid runtime declarations

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/matcher/flow.rs:163-215`, `glass-lint-core/src/api/rule/matcher/flow.rs:381-465`, `glass-lint-core/src/api/rule/matcher/mod.rs:21-43`

Private fields and wrapper kinds prevent direct external variant construction, but public factories still accept empty vectors, empty strings, arbitrary argument indexes, and unnormalized member names; validation happens only later when a catalog is built. Separate raw declaration builders from validated matcher types, or make these constructors return validated values, so invalid states cannot circulate through public APIs before the validation boundary.

**Implementation guidance:** Treat the current public constructors as an untrusted input boundary, not as constructors for runtime matcher values. Do not add setters or repeat validation in every consumer; preserve the clean breaking-change policy and ensure no public field, factory, or builder conversion can smuggle an invalid declaration into a compiled catalog.

**Proposed implementation direction:** Introduce explicit raw declaration/specification types for ergonomic construction and a validated `MatcherSet`/compiled-input type produced by one fallible `build` operation. Where invariants are local and cheap, make factories return `Result` with field-specific errors; where validation is cross-field, keep it in the single builder transition and make compiler APIs accept only the validated type. Add API tests for empty alternatives, blank names, duplicate indexes, unnormalized chains, invalid package patterns, and failed-build non-entry into compilation.

### READ-010 — Filesystem boundary and canonicalization policy remains duplicated around the shared walker

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-project/src/walk.rs:25-95`, `glass-lint-project/src/corpus.rs:129-163`, `glass-lint-project/src/discovery.rs:73-109`, `glass-lint-project/src/loader.rs:352-369`

`walk::collect_files` centralizes traversal budgets and filtering, but canonicalization and containment are still enforced separately by `SourceCorpus::load_source_file`, `ProjectDiscovery::validate_membership`, and `ProjectLoader::load_path`; queue admission also repeats exclusion and extension checks. Introduce one canonical selection/source-admission boundary that returns root-contained paths and owns symlink, exclusion, and budget policy, leaving the corpus and loader facades as thin callers.

**Implementation guidance:** Do not merely delegate another reader while retaining `inside_root`, `validate_membership`, queue filtering, and source-extension checks in separate callers. Canonicalize the project root once, define symlink behavior before admission, and make every accepted path pass through one bounded policy; outside-root, excluded, unsupported, duplicate, and budget-edge cases must have one deterministic result.

**Proposed implementation direction:** Create a validated `ProjectRoot`/`SourceAdmission` object that owns the canonical root, options, and admission counters, with operations for resolving roots, selecting files, and loading an admitted `SourceFile`. Make `walk::collect_files`, `SourceCorpus`, `ProjectDiscovery`, and `ProjectLoader` consume that object; remove duplicate containment/exclusion/extension checks and add cross-facade contract tests for root symlinks, nested symlinks, escapes, tsconfig membership, duplicate paths, excluded directories, and exact visit/file/source-byte limits.

## Systemic Themes

- **Partial consolidation is the dominant risk.** Several changes add a cache, typed ID, set, or borrowed view while leaving the old traversal, vector, or field list active beside it.
- **Phase boundaries still force ownership work.** Resolver and argument analysis cross mutable construction, constant evaluation, and projection concerns, which leads to repeated walks and owned snapshots.
- **Indexes should own convergence and evidence assembly.** Cross-flow propagation and finding construction still use broad sweeps or repeated range scans instead of delta/worklist or single-pass domain operations.
- **Validation and policy are delayed or distributed.** Public matcher factories and project loading each permit intermediate states that are checked again by downstream callers.

## Open Questions

- Should scope reuse use stable parser/traversal identities, or should predeclaration and collection be replaced by one scope-forming walker with explicit phase callbacks?
- Should raw matcher declarations be a distinct public builder type, with catalogs accepting only validated runtime matcher values?
- Should `SourceCorpus` remain a public facade, or should canonical source admission be owned by the project loader and shared by corpus callers?

## Coverage

Reviewed the current Rust implementation across all 124 files under
`glass-lint-core/src`, all 11 files under `glass-lint-core/tests`, and all 9
files under `glass-lint-project/src` (39,250 lines total). Coverage included
scope collection, declaration and value resolution, fact lowering, local and
cross-module flow, occurrence matching, finding/report assembly, matcher API
and compiler dispatch, filesystem discovery/loading/resolution, and test
organization. The re-audit used the repository architecture, testing, and
contribution guidance, targeted symbol/allocation scans, and focused tests.

Verification completed with 241 `glass-lint-core` library tests and 10
`glass-lint-project` tests passing. Only this Markdown report was modified.
