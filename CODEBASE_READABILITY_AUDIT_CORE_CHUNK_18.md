# Codebase Readability Audit — glass-lint-core Chunk 18 (Rule authoring and query declarations)

## Summary

Chunk 18 covers `glass-lint-core/src/api/rule/**` (plus `api/mod.rs` and the
public `rules` re-export in `lib.rs`): the public rule-authoring boundary. The
audit traced `Rule`/`RuleBuilder`/`CatalogRuleBuilder`, the `FirstError`
deferred-error pattern, `ModuleSpecifierPattern`, the `EventQuery`/
`LifecycleQuery`/`QueryDecl` declaration vocabulary (`api/rule/query/*`),
query explanation, and the error enumerations in `api/rule/error.rs` and
`api/rule/query/error.rs`, then verified representative callers in
`glass-lint-js` (e.g. `browser/remote_resource`, `node/filesystem`), the
compiler (`api/compiler/rule.rs`, `normalize.rs`, `validate/pass1_3.rs`),
`lint/catalog.rs`, and the public-surface integration test.

The design holds up well in the areas the chunk requirements call for:

- The runtime/compiler boundary is respected. All `api/rule` authoring types are
  validated semantic constructors; compiler IR, physical roots, fact IDs, and
  executor storage remain private. `QueryExpr` is public but fully opaque
  (only `Display` and `diagnostic_name()`), and `EmissionDecl`/`EventQuery`
  expose narrow read accessors with `pub(crate)` fields.
- The `FirstError`/`record_first_error` deferral pattern is used consistently
  across `RuleBuilder.duplicate_field`, `CatalogRuleBuilder.first_query_error`,
  and the lifecycle builders, and it is genuinely shared (not copy-pasted).
- The explanation layer lives in one cohesive module (`query/explanation.rs`),
  is `pub(crate)`, and is consumed only through `QueryDecl::explanation`, which
  feeds `CompiledRuleRecord.query_explanations`.
- `MemberChain`/`IdentitySpec`/`EventSpec` are validated once at the query
  boundary and remain typed through normalization and execution.
- Repeated "bounded, sorted, deduplicated" collection logic is already
  consolidated into `CanonicalCollection` (used by `LifecycleEvents`,
  `LifecycleSinks`, and the `ValueMatcher`/`ArgumentMatcher` walled-list
  helpers).

Findings below are the remaining structural fat: a stale test-support constant
on a public type, a single-variant enum that exists only as a bridge to
`QueryPredicate::Argument`, a redundant builder-state wrapper that forces a
three-level builder chain, a thin/near-duplicated validation-helper layer, a
one-field `EventSelection` wrapper around `VarId`, and an error-enum shape
inconsistency (`QueryBuildError` lacks `std::error::Error`).

## Findings

### Rule authoring (`api/rule/mod.rs`)

#### [ ] READ-001 — `Rule::EVIDENCE_LIMIT` is a test-support constant on the public authoring type

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/mod.rs:80-84`

`Rule::EVIDENCE_LIMIT` (16) is a public constant documented as an authoring
contract ("Retain enough matcher evidence for provider rules …"), but the
workspace-wide search shows its only readers are internal matching tests
(`analysis/matching/evidence/tests.rs:31,43,47-49`). No provider, harness, CLI,
or report path reads it. The production per-rule evidence bound is owned by
`AnalysisLimits.evidence_items` (`limits.rs:82,153-155`, default 65 536),
threaded through `project/session` and `lint/report/mod.rs:180-183`. Publicly
exposing a constant whose real owner is the evidence-normalization limit is a
leaked, misleading contract that invites external crates to read a value that
has no effect on their reports.

**Recommendation:** Delete `Rule::EVIDENCE_LIMIT`. Let the matching tests pass
a local limit (or reuse `AnalysisLimits::evidence_items` defaults) and keep the
bound owned by `analysis::matching::evidence::normalize_evidence`'s callers
(`project/session`). Guardrail: the value 16 must not become a second,
competing default; the normalizer's behavior is driven solely by the
caller-supplied `evidence_limit`.

**Fix Applied:** None so far.

### Query declarations (`api/rule/query/mod.rs`, `composition.rs`)

#### [ ] READ-002 — `EventRequirementKind` is a single-variant enum that exists only to bridge into `QueryPredicate::Argument`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:425-463`; `glass-lint-core/src/api/rule/query/composition.rs:227-236`

`EventRequirement` wraps `EventRequirementKind`, whose only variant is
`Argument { index, matcher }`. `EventRequirement::argument` (the sole
constructor) always produces that one variant, and the only consumer is the
single-arm `match req.kind` in `QueryDecl::all` that re-encodes it as
`QueryPredicate::Argument`. The enum adds no coercion, no dispatch, and no
future-ready variant today; it is a second, parallel argument-predicate
representation whose entire purpose is to be immediately converted into the
`QueryPredicate::Argument` atom. This is the exact "single-variant enum
introduced for speculative extension" shape the skill warns about, and it
forces readers to track two argument-predicate types.

**Recommendation:** Fold `index` and `matcher` directly into `EventRequirement`
and delete `EventRequirementKind`; `QueryDecl::all` then maps the pair to
`QueryPredicate::Argument` directly. Guardrail: keep the public
`EventRequirement::argument(usize, impl Into<ArgumentMatcher>)` signature, the
`InvalidArgumentIndex` → `ArgumentIndex::try_from_usize` behavior, and the
`ArgumentsRequireCallEvent` rejection inside `QueryDecl::all`
(composition.rs:224-226). Add `EventRequirementKind` back only when a second
requirement kind actually ships.

**Fix Applied:** None so far.

#### [ ] READ-005 — `EventSelection` is a one-field wrapper around `VarId`; the bound-variable numbering is implicit

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:104-150`; `glass-lint-core/src/api/rule/query/expression.rs:58-62,117-137`; `glass-lint-core/src/api/rule/query/composition.rs:254-258,275-277`

`EventSelection` is a public-field tuple struct (`pub(crate) bind: VarId`) with
no domain operations; it is used in exactly one `QueryExprKind` variant and an
internal constructor. Around it, the variable vocabulary is implicit and
caller-disciplined: every public `EventQuery` constructor hardcodes
`var: VarId::new(0)` (`mod.rs:234-241`), `QueryDecl::lifecycle` hardcodes
`primary_var: VarId::new(0)` (`composition.rs:255`), and
`member_subject_query` mints `VarId::new(1)` ad hoc for the object variable
(`composition.rs:277`). The compiler deliberately ignores these author-level
IDs anyway (alpha-renumbering in `compiler/normalize.rs:32-38`), so the
stored `EventQuery.var` field is effectively constant in every production path
and only meaningful for internal `walk_vars` shape analysis. This is a
single-field wrapper over a primitive plus a hidden numbering convention.

**Recommendation:** Drop the `EventSelection` wrapper and store `VarId` in
`QueryExprKind::SelectEvent(VarId)`; callers read `q.var()` or the variant's
ID directly. For the `$0`-event / `$1`-object numbering, either derive it at
walk time or document it in `VarId`'s doc comment as the bounded authoring
vocabulary. Guardrail: `walk_vars`/`walk_vars_until` role handling
(expression.rs:117-137) and `validate/pass1_3.rs` binding/type inference must
keep distinguishing the event variable from the member-subject object
variable; the compiler's alpha-renumbering must remain the authoritative slot
policy.

**Fix Applied:** None so far.

### Lifecycle builders (`api/rule/query/lifecycle.rs`)

#### [ ] READ-003 — `LifecycleBuilderState` adds a pointless nesting level, making the builder a three-level chain

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:115-143`; used at `145-216` (`LifecycleQueryBuilder`), `218-252` (`CatalogLifecycleQueryBuilder`), `254-267` (`builder`/`catalog_builder`)

`LifecycleQueryBuilder` holds `state: LifecycleBuilderState`, which holds
`stages: LifecycleStages` plus `invalid_operation: FirstError`. The external
face is therefore
`CatalogLifecycleQueryBuilder → LifecycleQueryBuilder → LifecycleBuilderState →
LifecycleStages`. `LifecycleBuilderState` adds no behavior: `new` delegates to
`LifecycleStages::new`, `record_operation` is a one-line call to the shared
`record_first_error`, and `build` just pops the `FirstError` and forwards.
Meanwhile the sibling `RuleBuilder`/`CatalogRuleBuilder` pair (the same
deferral pattern, `rule/mod.rs:139-314`) keeps its fields directly on the
builder with no intermediate state struct. The extra wrapper is indirection
with no owner, vocabulary gain, or lifecycle boundary.

**Recommendation:** Delete `LifecycleBuilderState` and hold
`stages: LifecycleStages` + `invalid_operation: FirstError` directly on
`LifecycleQueryBuilder`, mirroring `RuleBuilder`. `try_add_source`/
`try_add_condition`/`try_add_completion` become
`self.stages.try_*`; `build` destructures the two fields. Guardrail: keep
`LifecycleStages` (the builder-state type distinct from the built
`LifecycleQuery`) and keep `CatalogLifecycleQueryBuilder`'s deferred first-error
precedence identical, so error reporting order for providers does not change.

**Fix Applied:** None so far.

### Query-boundary validation (`api/rule/query/declarations.rs` and callers)

#### [ ] READ-004 — Query-boundary string validation is a thin, near-duplicated helper layer

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/declarations.rs:50-77`; callers at `constructors.rs:12-188`, `composition.rs:41-98`, `value.rs:103-112`, `lifecycle/types.rs:70-84,336-356`

`checked_name` (declarations.rs:50-56) and `checked_module_name`
(declarations.rs:58-64) are byte-for-byte the same "trim, then reject empty"
trunk and differ only in the error variant (`EmptyIdentityName` vs
`EmptyModuleSpecifier`); `checked_chain` (declarations.rs:75-77) is a pure
forwarder to `MemberChain::parse` with no error mapping or vocabulary change,
used at ~14 call sites across `constructors.rs`, `composition.rs`, `value.rs`,
and `lifecycle/types.rs`. Two more "trim → reject empty" checks are inlined in
`LifecycleEvent::member_call` (types.rs:73-76) and
`LifecycleSink::build_call_sink` (types.rs:341-344), and `import_exact`/
`string_contains` (constructors.rs:154-165,176-188) hand-roll the same check to
raise `EmptyModuleSpecifier`/`EmptyStaticValue` while deliberately retaining the
untrimmed string. Six hand-rolled copies of one validation with no single
owner (see the skill's "repeated validation" DEDUPLICATE signal).

**Recommendation:** Merge `checked_name`/`checked_module_name` into one
`checked_specifier(value, empty_error)` helper parameterized by the error
variant, and delete the `checked_chain` alias so callers invoke
`MemberChain::parse` directly. Guardrail: keep each distinct error variant
(`EmptyIdentityName`, `EmptyModuleSpecifier`, `EmptyStaticValue`) and preserve
the two intentionally untrimmed-storage sites (`import_exact`,
`string_contains`); do not force them through a trimming helper.

**Fix Applied:** None so far.

### Query errors (`api/rule/query/error.rs`)

#### [ ] READ-006 — `QueryBuildError` is the only top-level build error that does not implement `std::error::Error`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/error.rs:5-93`; contrast `glass-lint-core/src/api/rule/error.rs:185,204,238`

The three sibling error enumerations in the same boundary —
`RuleBuildError`, `MatcherBuildError`, `CompiledCatalogError` — all implement
`std::error::Error` (rule/error.rs:185,204,238), but `QueryBuildError`, which is
returned directly by every public `EventQuery`/`LifecycleQuery`/`QueryDecl`
constructor (`try_query`, `QueryDecl::all/any/lifecycle`,
`EventRequirement::argument`, all `with_arg_*` adapters), does not. This is an
arbitrary shape divergence across the two `error.rs` files: consumers in other
crates cannot use `QueryBuildError` with `Box<dyn Error>` or error-frameworks,
even though it is the most frequently returned error of the whole authoring
surface.

**Recommendation:** Add `impl std::error::Error for QueryBuildError {}` in
`query/error.rs`, consistent with the sibling build errors. The nested
diagnostics (`QueryDiagnostic`, `PhysicalPlanDiagnostic`,
`CompilerInvariantDiagnostic`) intentionally remain interior Display-only
values because they are always embedded in an outer error; only the
standalone-returned `QueryBuildError` needs the trait. Guardrail: do not change
any variant shapes, Display text, or the wrapping of `QueryBuildError` inside
`RuleBuildError::InvalidQuery` and `MatcherBuildError`.

**Fix Applied:** None so far.

## Systemic Themes

- **Deferred-error pattern is consistent and well-shared.** `FirstError` +
  `record_first_error` (`rule/mod.rs:13-40`) is reused by both the rule
  builders and the lifecycle builders rather than copy-pasted; the only wrinkle
  is the extra `LifecycleBuilderState` nesting (READ-003).
- **Validated-construction boundary holds.** The chunk separates constructors
  that produce `Result` from adapters (`IntoQueryDecl`, `define_lifecycle_adapter`,
  `IntoLifecycleQuery`) that let declarative catalogs defer errors; providers
  like `browser/remote_resource` and `node/filesystem` exercise only the
  catalog path. Public `Rule`, `QueryDecl`, and `EventQuery` expose no compiler
  storage.
- **Parallel authoring-atoms against a richer internal model.** Several authoring
  types (`EventRequirementKind`, `EventSelection`, `PatternValue`) are narrowed
  to a single variant; two are structural dead weight (READ-002, READ-005),
  while `PatternValue` and `ModuleSpecifierPattern`'s single `Package` variant
  are documented planned-extensions and should stay.
- **Bound collection logic is centralized.** `CanonicalCollection` absorbs the
  sort/dedupe/bound behavior for lifecycle and value collections; only
  `LifecycleStages::build` still sorts/dedups `sources` by hand (`lifecycle.rs:91-94`),
  which is too small to report separately given `LifecycleQuery` needs the
  incremental builder shape.

## Open Questions

- **`EventQuery.var` constant zero:** Note READ-005. Because the compiler
  alpha-renumbers (`normalize.rs:36-38`), author-level `VarId`s are cosmetic for
  execution. Should the event-bound variable become an explicit constructor
  parameter (enabling future multi-variable authoring), or is the stable `$0`
  convention an acceptable documented invariant? No error, but the placement of
  the rule is worth a decision.
- **`CatalogRuleBuilder` error precedence:** `build()` pops `first_query_error`
  before delegating to `inner.build()` (`rule/mod.rs:308-313`), so any query
  error beats a duplicate-metadata error even when the metadata setter ran
  first. Is this the intended "query errors win" contract, and should it be
  documented? Currently implicit.
- **`QueryExpr`/`EmissionDecl` public surface:** `QueryDecl::expression()` and
  `emission()` are public, but `QueryExpr` exposes only `Display`/
  `diagnostic_name()` and nothing can build one outside the crate (external
  integration tests construct `SelectorEvent`? no — they use `EventQuery`).
  Would `pub(crate)` `QueryExpr` and a `pub(crate)` `expression()` suffice now
  that providers only construct via `EventQuery`/`QueryDecl::any|all|lifecycle`?
- **`ModuleSpecifierPattern::PatternValue`** single-variant enum is an explicit
  documented extension point ("An exact-module variant is the planned extension
  point", module.rs:18). Confirm it is retained when exact-module identities
  are authored; currently `import_exact` still models exact specifiers as
  `IdentitySpec::LiteralString`.

## Coverage

- `glass-lint-core/src/api/mod.rs` (full)
- `glass-lint-core/src/api/classification.rs`, `classification/result.rs` (skim; owned by the classification chunk)
- `glass-lint-core/src/api/rule/mod.rs` (full)
- `glass-lint-core/src/api/rule/error.rs` (full)
- `glass-lint-core/src/api/rule/module.rs` (full; also `project::PackageSpecifier` contract)
- `glass-lint-core/src/api/rule/taxonomy.rs` (full)
- `glass-lint-core/src/api/rule/query/mod.rs` (full)
- `glass-lint-core/src/api/rule/query/{canonical,composition,constructors,declarations,error,event,explanation,expression,limits,value}.rs` (full)
- `glass-lint-core/src/api/rule/query/lifecycle.rs`, `lifecycle/{endpoint,types}.rs` (full)
- Providers: `glass-lint-js/src/rules/browser/remote_resource/mod.rs`, `glass-lint-js/src/rules/node/filesystem/mod.rs` (representative catalog-builder + deferred-error callers); usage census of `Rule::catalog_builder`/`LifecycleQuery::catalog_builder` across `glass-lint-js` and `glass-lint-obsidian`
- Compiler consumers: `api/compiler/{mod.rs,rule.rs,normalize.rs,validate/pass1_3.rs}` (how declarations are lowered and consumed)
- `lint/catalog.rs` (`CompiledCatalogError` consumption, `Rule::id->RuleId`)
- `glass-lint-core/src/lib.rs` (`rules` re-export), `tests/integration/public_surface.rs`
- Limit/production cross-checks: `limits.rs` (`evidence_items`), `project/session/mod.rs`, `lint/report/mod.rs`, `analysis/matching/evidence.rs` and its tests for `EVIDENCE_LIMIT`