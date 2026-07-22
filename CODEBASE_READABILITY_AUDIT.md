# Codebase Readability Audit

## Summary

The re-audit retains 8 actionable readability and maintainability issues
across `glass-lint-core` and `glass-lint-project`: 5 high severity and 3
medium severity. Nine findings were removed after verification: the retained
value arena, local fact-path representation, function-summary round state,
summary path storage, lazy package occurrence scans, resolver cache sharing,
core test-helper/project-test organization, the positional scope reuse plan,
and the partial declaration-classification cache are now materially complete.

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

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/resolution/constant.rs:7-65`, `glass-lint-core/src/analysis/resolution/mod.rs:119-130`, `glass-lint-core/src/analysis/value/arena.rs:87-180`

Separating `ValueTable` from the resolution cache removed deep clones of cached `ResolvedValue` records, but `const_value` still copies every static string, array vector, and object-entry vector into `ConstAction` before recursively inspecting it. Expose a bounded borrowed shape visitor or stable value handles for recursive queries, with an explicit mutable interning phase and immutable inspection API so large static values are not copied merely to determine their variant or descendants.

**Implementation guidance:** Do not wrap every value in another `Arc`, return references that outlive the `RefCell` borrow, or hide full clones behind `Cow`/helper functions. The query API must preserve cycle detection, reassignment/version identity, malformed IDs, value exhaustion, and bounded recursion while making the common “inspect one variant/child” path borrow-only.

**Proposed implementation direction:** Split resolver construction from querying: finish interning into a frozen `ValueTable`, then expose a bounded `ValueView`/visitor that follows `ValueId` chains iteratively and visits array/object children by borrowed slices. Keep owned `ConstValue` materialization only at an explicit external boundary, remove `ConstAction` collection clones from recursive inspection, and add operation/allocation-oriented tests for large arrays/objects plus cycle, invalid-ID, reassignment, and exhaustion cases.

### READ-004 — Argument projection still rebuilds shapes in a second traversal

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/build/arguments.rs:39-175`, `glass-lint-core/src/analysis/syntax/constant.rs:123-220`

`arg_info` resolves the expression and conditionally runs the constant evaluator, then `walk_argument_projections` traverses member, object, and array syntax again, re-resolving descendants and reconstructing composite values. The walks do not share one evaluator budget or one cached shape, so introduce a single bounded `ArgumentAnalysis` result from which value identity, projections, static strings, and exhaustion state are derived.

**Implementation guidance:** Do not preserve the current resolver walk and syntax walk behind a larger wrapper. One traversal must own the depth, node, lookup, path, name, and value budgets; dynamic keys, spreads, unsupported properties, and partial shapes must produce one typed fail-closed outcome shared by every derived view.

**Proposed implementation direction:** Build a resolver-owned `ArgumentAnalysis` tree while visiting the expression, with each node carrying its `ValueId`, optional base/path relation, static scalar/shape result, and child projections. Derive `CallArgInfo`, object-key/property predicates, rooted identity, and bound-argument projections from that tree, then delete the independent `syntax_constant::evaluate` fallback and descendant re-resolution from `walk_argument_projections`; preserve parity tests for templates, aliases, spreads, dynamic keys, nested containers, destructuring, and minified forms.

### READ-005 — Effects and the local projector still duplicate derived call relations

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:35-83`, `glass-lint-core/src/analysis/flow/effect.rs:236-265`, `glass-lint-core/src/analysis/flow/projector/mod.rs:108-220`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:19-58`

`record_call` clones the derived `EffectArgument` vector into `EffectCall` and stores the same per-argument relations again in `EffectUse::CallArgument`. In addition, the projector maintains its own `projector_chain`/`projector_rooted` effective-call selection beside `CallEffectRef`, with slightly different fallback behavior; retain fact IDs plus one relation index and route local, summary, and cross-file projection through one borrowed call view.

**Implementation guidance:** Effects may own derived parameter relations, but they must not own a second copy of the same argument record for each use. Effective-call selection, `.call()`/`.apply()` unwrapping, callee-name fallback, rootedness, and missing-fact behavior must be defined once; use typed `Option` results at invalid boundaries rather than `expect`-based assumptions.

**Proposed implementation direction:** Make `EffectCall` contain the call `FactId` and compact argument relation/index data, and make `EffectUse::CallArgument` refer to the call plus an argument index or relation ID. Extend `CallEffectRef` to cover every projector query, remove `projector_chain`/`projector_rooted`, and have local, summary, and cross-flow code consume that view; add parity tests for direct calls, aliases, `.call()`, `.apply()`, unknown facts, and qualified cross-module propagation before deleting duplicate fields and helpers.

### READ-006 — Cross-flow refinement still copies entire source buckets and sweeps all edges

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:259-301`, `glass-lint-core/src/analysis/flow/cross/mod.rs:430-498`

`FlowSources` now deduplicates with `BTreeSet`, but `extend_from_key` still materializes the complete source bucket into a temporary vector for every edge. Each refinement round also scans every module, effect, and call and uses `changed_keys` only to skip insertion, rather than driving a worklist from changed source keys; make insertion report deltas directly and index affected edges so unchanged buckets and edges are not revisited.

**Implementation guidance:** Do not replace the temporary `Vec` with another cloned collection or claim a fixed point is efficient while retaining a full project sweep per round. The propagation state must distinguish newly inserted candidates from already propagated candidates, preserve deterministic order, and fail closed on both operation-budget and round-limit exhaustion.

**Proposed implementation direction:** Give each source bucket a stable `SourceKeyId`, a deduplicating candidate set, and a monotone delta cursor; build an adjacency index from source keys to affected call edges. Drive a FIFO/BTree worklist with `(source key, new candidate)` deltas, inserting directly into destination buckets and enqueueing only destinations that changed; retain the existing bounded convergence result and add high-fanout/cyclic tests that assert no-op propagation, deterministic ordering, and exact exhaustion behavior.

### READ-007 — Finding assembly still rescans ranges and clones related evidence

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/findings.rs:33-86`, `glass-lint-core/src/lint/linter.rs:338-395`

`by_range` avoids cloning primary evidence records, but every retained range filters the full range map again, and project enrichment first builds owned related-evidence vectors before cloning them into each finding. Use one deterministic range sweep with a range/group index and attach related events while the final report DTOs are emitted, preserving nested-range, truncation, and deduplication semantics.

**Implementation guidance:** Do not introduce another owned finding struct that mirrors `Finding`, and do not trade the current scans for a boxed iterator or nondeterministic hash iteration. Keep classification evidence borrowed until the report boundary, make containment/grouping one operation, and ensure each related event is attached according to the same rule/range ownership that produced the primary finding.

**Proposed implementation direction:** Add a report-local range accumulator that records evidence indices and related-event indices while scanning each capability once, performs one sorted containment sweep, and emits final `Finding`/`Evidence` values directly. Key related evidence by `RuleIndex` and final finding group rather than building a rule-wide cloned vector; add nested-range, duplicate-occurrence, truncation, and multi-capability tests that compare exact ordering and counts.

### READ-008 — Matcher-family metadata remains split between the macro, storage, and lowering

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/matcher/mod.rs:21-43`, `glass-lint-core/src/api/rule/matcher/mod.rs:58-137`, `glass-lint-core/src/api/compiler/rule.rs:230-256`

The family macro generates matcher enums, family views, push/flatten behavior, and validation/normalization dispatch, but `MatcherSet` storage remains a separate hand-maintained field list and compiler lowering remains a separate exhaustive family match. Make the declaration also generate storage and lowering metadata, or introduce a typed family visitor whose required operations cannot silently omit a family; the current contract test exercises known families but does not remove these parallel authorities.

**Implementation guidance:** Do not add a second registry beside the macro or rely only on an exhaustive match whose arm can silently perform the wrong lowering. Adding a family must require its storage type, normalization, validation, flattening, and lowering behavior at the declaration site, and the migration must remove the old parallel lists rather than preserve compatibility wrappers.

**Proposed implementation direction:** Extend the family declaration with field metadata and a lowering function/visitor hook, then generate `MatcherSet` fields, family views, conversions, mutation, normalization, validation, and compiler dispatch from that one list. If macro limitations make generated storage impractical, use a typed `MatcherFamilySpec` trait with associated storage and lowerer types; add a compile-time-shaped contract test and a deliberately added test-only family fixture to prove every operation is required.

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

- **Partial consolidation is the dominant risk.** Several changes add a cache, typed ID, set, or borrowed view while leaving the old traversal, vector, field list, or effective-call implementation active beside it.
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

Verification completed with 227 `glass-lint-core` library tests and 10
`glass-lint-project` tests passing. Only this Markdown report was modified.
