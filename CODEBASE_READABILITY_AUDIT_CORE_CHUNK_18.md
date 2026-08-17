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
compiler (`api/compiler/rule.rs`, `normalize.rs`, `validate/pass1_3.rs`,
`validate/pass4_10.rs`), `lint/catalog.rs`, and the public-surface integration
test.

The design holds up well in the areas the chunk requirements call for:

- The runtime/compiler boundary is respected. All `api/rule` authoring types are
  validated semantic constructors; compiler IR, physical roots, fact IDs, and
  executor storage remain private (`api/compiler` is `pub(crate)`,
  `api/compiler/mod.rs:29-42`, and `api` itself is crate-private,
  `lib.rs:16`). `QueryExpr` is public but fully opaque (only `Display` and
  `diagnostic_name()`), and `EmissionDecl`/`EventQuery` expose narrow read
  accessors with `pub(crate)` fields.
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
three-level builder chain, a near-duplicated validation-helper layer whose
single "trim, then reject empty" check is re-expressed about ten times, a
one-field `EventSelection` wrapper around `VarId`, and an error-enum shape
inconsistency (`QueryBuildError` lacks `std::error::Error`).

## Findings

### Rule authoring (`api/rule/mod.rs`)

#### [x] READ-001 — `Rule::EVIDENCE_LIMIT` is a test-support constant on the public authoring type

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/mod.rs:80-84`

`Rule::EVIDENCE_LIMIT` (16) is a public constant documented as an authoring
contract ("Retain enough matcher evidence for provider rules …", mod.rs:81-83),
but the workspace-wide search shows its only readers are internal matching
tests (`analysis/matching/evidence/tests.rs:31,43,47-49`). No provider, harness,
CLI, or report path reads it. The production per-rule evidence bound is owned by
`AnalysisLimits.evidence_items` (`limits.rs:82`, default 65 536 at
`limits.rs:153-155`), which enters the session through
`Linter::begin_project` (`lint/linter.rs:137`), is carried by `SessionState`
(`project/session/mod.rs:42,57,65`), and reaches `normalize_evidence` via the
report assembly (`lint/report/mod.rs:180-183`) and `Projection::evidence_for`
(`analysis/project/projection.rs:462`). Publicly exposing a constant whose real
owner is the evidence-normalization limit is a leaked, misleading contract that
invites external crates to read a value that has no effect on their reports.

**Recommendation:** Delete `Rule::EVIDENCE_LIMIT`. Let the matching tests pass a
local constant (e.g. a `const` inside `evidence/tests.rs` chosen to exercise the
truncation path) and keep the bound owned by
`analysis::matching::evidence::normalize_evidence`'s callers (`project/session`).
Guardrail: the value 16 must not become a second, competing default; the
normalizer's behavior is driven solely by the caller-supplied `evidence_limit`
(`analysis/matching/evidence.rs:221-230`).

**Fix Applied:** Removed the test-only `Rule::EVIDENCE_LIMIT` public constant
and kept the evidence-normalization fixture bound local to its tests. Runtime
evidence remains governed solely by the session-supplied limit.

### Query declarations (`api/rule/query/mod.rs`, `composition.rs`)

#### [x] READ-002 — `EventRequirementKind` is a single-variant enum that exists only to bridge into `QueryPredicate::Argument`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:431-463`; `glass-lint-core/src/api/rule/query/composition.rs:227-236`

`EventRequirement` wraps `EventRequirementKind` (mod.rs:431-443), whose only
variant is `Argument { index, matcher }` (mod.rs:436-443).
`EventRequirement::argument` (mod.rs:445-463), the sole constructor, always
produces that one variant, and the only consumer is the single-arm
`match req.kind` in `QueryDecl::all` (composition.rs:227-236) that re-encodes it
as `QueryPredicate::Argument`. The enum adds no coercion, no dispatch, and no
future-ready variant today; it is a second, parallel argument-predicate
representation whose entire purpose is to be immediately converted into the
`QueryPredicate::Argument` atom. This is the exact "single-variant enum
introduced for speculative extension" shape the skill warns about, and it
forces readers to track two argument-predicate types.

**Recommendation:** Fold `index` and `matcher` directly into `EventRequirement`
and delete `EventRequirementKind`; `QueryDecl::all` then maps the pair to
`QueryPredicate::Argument` directly. Guardrail: keep the public
`EventRequirement::argument(usize, impl Into<ArgumentMatcher>)` signature, the
`InvalidArgumentIndex` → `ArgumentIndex::try_from_usize` behavior
(`query/error.rs:14`, `value.rs:17-24`), and the `ArgumentsRequireCallEvent`
rejection inside `QueryDecl::all` (composition.rs:224-226). Add
`EventRequirementKind` back only when a second requirement kind actually ships.

**Fix Applied:** Collapsed the single-variant `EventRequirementKind` into
`EventRequirement`'s `index` and `matcher` fields. `EventRequirement::argument`
keeps the same public signature and bounded-index error behavior, while
`QueryDecl::all` maps the constructed requirement directly to the argument
predicate.

#### [x] READ-005 — `EventSelection` is a one-field wrapper around `VarId`; the bound-variable numbering is implicit

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:98-123,146-150`; `glass-lint-core/src/api/rule/query/expression.rs:58-62,116-137`; `glass-lint-core/src/api/rule/query/composition.rs:254-258,275-277`

`EventSelection` is a public-field tuple struct (`pub(crate) bind: VarId`,
mod.rs:146-150) with no domain operations; it is used in exactly one
`QueryExprKind` variant (`SelectEvent(EventSelection)`, expression.rs:19) and an
internal constructor (expression.rs:58-62). Around it, the variable vocabulary
is implicit and caller-disciplined: every public `EventQuery` constructor
hardcodes `var: VarId::new(0)` via `from_parts` (mod.rs:234-241),
`QueryDecl::lifecycle` hardcodes `primary_var: VarId::new(0)`
(composition.rs:255), and `member_subject_query` mints `VarId::new(1)` ad hoc
for the object variable (composition.rs:277). The compiler deliberately ignores
these author-level IDs anyway (alpha-renumbering in `normalize.rs:36-38`), so
the stored `EventQuery.var` field is effectively constant in every production
path and only meaningful for internal `walk_vars` shape analysis and the
pre-normalization type pass. This is a single-field wrapper over a primitive
plus a hidden numbering convention.

**Recommendation:** Drop the `EventSelection` wrapper and store `VarId` in
`QueryExprKind::SelectEvent(VarId)`; the consumers read the ID directly
(`walk_vars_until` at expression.rs:119, its `Display` arm at expression.rs:175,
the explanation layer at explanation.rs:9, and the type pass at pass1_3.rs:78).
For the `$0`-event / `$1`-object numbering, document it in `VarId`'s doc comment
(mod.rs:98-103) as the bounded authoring vocabulary rather than deriving it at
walk time — the IDs must stay consistent across the select/bind atom, the
`Event(var)` references in the branches, and the emission `primary_var`, and
deriving them at walk time would merely relocate the same convention.
Guardrail: `walk_vars`/`walk_vars_until` role handling (expression.rs:116-137)
and `validate/pass1_3.rs` binding/type inference must keep distinguishing the
event variable from the member-subject object variable (pass1_3.rs:78 vs
pass1_3.rs:112-117); the compiler's alpha-renumbering must remain the
authoritative slot policy.

**Fix Applied:** Removed the behaviorless `EventSelection` wrapper and stored
the bound `VarId` directly in `QueryExprKind::SelectEvent`. Updated compiler
validation and explanation pattern matches without changing query semantics or
diagnostic output.

### Lifecycle builders (`api/rule/query/lifecycle.rs`)

#### [x] READ-003 — `LifecycleBuilderState` adds a pointless nesting level, making the builder a three-level chain

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
`try_add_condition`/`try_add_completion` become `self.stages.try_source`/
`try_condition`/`try_completion`; `condition`/`completion` call
`record_first_error(&mut self.invalid_operation, …)` inline; `build` destructures
the two fields. Guardrail: keep `LifecycleStages` (the builder-state type
distinct from the built `LifecycleQuery`) and keep
`CatalogLifecycleQueryBuilder`'s deferred first-error precedence identical —
`first_error` is popped before `inner.build()` (lifecycle.rs:246-251) — so error
reporting order for providers does not change.

**Fix Applied:** Resolved by commit `66a80d7f` (`fix chunk 19 read 001`),
which removed `LifecycleBuilderState` and the intermediate lifecycle builder,
leaving `CatalogLifecycleQueryBuilder` over `DeferredBuilder<LifecycleStages>`.

### Query-boundary validation (`api/rule/query/declarations.rs` and callers)

#### [ ] READ-004 — Query-boundary string validation is a thin, near-duplicated helper layer with the emptiness check re-expressed about ten times

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/declarations.rs:50-77`; callers at `constructors.rs:12-188`, `composition.rs:41-98,141-144`, `value.rs:73-112,225-239`, `lifecycle/types.rs:70-84,336-356`, `lifecycle.rs:78-80`

`checked_name` (declarations.rs:50-56) and `checked_module_name`
(declarations.rs:58-64) are the same "trim, then reject empty" trunk and differ
only in the error variant (`EmptyIdentityName` vs `EmptyModuleSpecifier`);
`checked_chain` (declarations.rs:75-77) is a pure alias for `MemberChain::parse`
with no error mapping or vocabulary change, used at 17 call sites across
`constructors.rs` (9), `composition.rs` (5), `value.rs` (1), and
`lifecycle/types.rs` (2). The same emptiness check is inlined in eight more
spots: the `bounded_strings` converter (value.rs:84-90),
`object_property_value` (value.rs:229-236), `LifecycleEvent::member_call`
(types.rs:73-76), `LifecycleSink::build_call_sink` (types.rs:341-344),
`import_exact` (constructors.rs:154-165), `string_contains`
(constructors.rs:176-188), `any_with_evidence`'s symbol guard
(composition.rs:141-144), and `LifecycleStages::build`'s symbol guard
(lifecycle.rs:78-80). The copies differ only in the error variant and in whether
the trimmed or the original string is stored: the first four store the trimmed
value, `import_exact`/`string_contains`/the two symbol guards deliberately store
the untrimmed string, and `member_call`/`build_call_sink` pre-check so blank
input yields `EmptyIdentityName` instead of `MalformedChain` from
`MemberChain::parse`. Ten occurrences of one validation with no single owner
(see the skill's "repeated validation" DEDUPLICATE signal).

**Recommendation:** Merge `checked_name`/`checked_module_name` into one
`checked_specifier(value, empty_error)` helper that returns the trimmed value,
and route the trimmed-storage copies (`bounded_strings`, `object_property_value`)
through it as well; delete the `checked_chain` alias so callers invoke
`MemberChain::parse` directly. Guardrails: keep each distinct error variant
(`EmptyIdentityName`, `EmptyModuleSpecifier`, `EmptyStaticValue`,
`EmptyEvidenceSymbol`); preserve the intentionally untrimmed-storage sites
(`import_exact`, `string_contains`, `any_with_evidence`, `LifecycleStages::build`)
as inline checks so stored evidence symbols and literal-string predicates are
never re-written; and keep `member_call`/`build_call_sink` pre-checks so blank
input still surfaces `EmptyIdentityName` rather than `MalformedChain`.

**Fix Applied:** None so far.

### Query errors (`api/rule/query/error.rs`)

#### [x] READ-006 — `QueryBuildError` is the only top-level build error that does not implement `std::error::Error`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/error.rs:5-93`; contrast `glass-lint-core/src/api/rule/error.rs:185,204,238`

The three sibling error enumerations in the same boundary —
`RuleBuildError`, `MatcherBuildError`, `CompiledCatalogError` — all implement
`std::error::Error` (rule/error.rs:185,204,238), but `QueryBuildError`, which is
returned directly by the widest surface of the whole authoring API — every
`EventQuery` constructor and `with_arg_*` adapter (`constructors.rs`), every
`QueryDecl` combinator (`all`/`any`/`lifecycle`/`member_*`, composition.rs),
`EventRequirement::argument`, the lifecycle builders, and `RuleBuilder::try_query`
— does not. This is an arbitrary shape divergence across the two `error.rs`
files: `QueryBuildError` is re-exported publicly (`lib.rs:55`), so consumers in
other crates name it but cannot use it with `Box<dyn Error>` or error-frameworks,
even though it is the most frequently returned error of the authoring surface.

**Recommendation:** Add `impl std::error::Error for QueryBuildError {}` in
`query/error.rs`, consistent with the sibling build errors. The nested
diagnostics (`QueryDiagnostic`, `PhysicalPlanDiagnostic`,
`CompilerInvariantDiagnostic`) intentionally remain interior Display-only
values because they are always embedded in an outer error (`MatcherBuildError`/
`CompiledCatalogError`, and the compiler mapping at `api/compiler/mod.rs:206-221`);
only the standalone-returned `QueryBuildError` needs the trait. Guardrail: do not
change any variant shapes, Display text, or the wrapping of `QueryBuildError`
inside `RuleBuildError::InvalidQuery` (`rule/error.rs:26`, produced at
`rule/mod.rs:310`).

**Fix Applied:** Implemented `std::error::Error` for `QueryBuildError`,
matching the sibling public build errors without changing variants, display
messages, or wrapping behavior.

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
  storage, and the crate-private `api` module plus the explicit `rules`
  re-export (lib.rs:49-57) keep the reachable surface exactly the re-exported
  authoring types.
- **Parallel authoring-atoms against a richer internal model.** Several authoring
  types (`EventRequirementKind`, `EventSelection`, `PatternValue`) are narrowed
  to a single variant; two are structural dead weight (READ-002, READ-005),
  while `PatternValue` and `ModuleSpecifierPattern`'s single `Package` variant
  are documented planned-extensions (module.rs:13-22) and should stay.
- **Bound collection logic is centralized.** `CanonicalCollection` absorbs the
  sort/dedupe/bound behavior for lifecycle and value collections; only
  `LifecycleStages::build` still sorts/dedups `sources` by hand (`lifecycle.rs:91-94`),
  which is too small to report separately given `LifecycleQuery` needs the
  incremental builder shape.
- **Boundary emptiness validation is hand-scattered.** The single "trim, then
  reject empty" rule is re-expressed about ten times at the query boundary —
  two near-identical helpers and eight inline guards/converters with different
  error variants and trimmed-versus-untrimmed storage decisions (READ-004).
  This is the chunk's main duplication family; a single parameterized helper
  plus `MemberChain::parse` used directly is the natural owner.

## Open Questions — Resolved

- **`EventQuery.var` constant zero:** The stable `$0`-event / `$1`-member-subject
  numbering is the enforced authoring invariant and should be **documented, not
  parameterized**. Every public `EventQuery` constructor funnels through
  `EventQuery::from_parts`, which hardcodes `var: VarId::new(0)`
  (`query/mod.rs:234-241`); `QueryDecl::lifecycle` hardcodes
  `primary_var: VarId::new(0)` (`composition.rs:255`); and `member_subject_query`
  mints `VarId::new(1)` for the object variable (`composition.rs:277`). The
  compiler discards author-level IDs for execution — `normalize_query_decl` calls
  `root.alpha_renumber_slots()` (`normalize.rs:36-38`) so physical slots are
  dense regardless of authored IDs. Author-level IDs matter only for the
  pre-normalization checks that must stay internally consistent:
  `walk_vars_until` role analysis (`expression.rs:116-137`) and `pass_scope_types`
  binding/type inference (`pass1_3.rs:62-218`, validating the emission primary
  var at pass1_3.rs:65,208-218). An explicit constructor parameter would add an
  argument every caller must pass `0` for and would allow inconsistent IDs that
  validation would then have to reject — with zero execution effect because of
  the alpha-renumber. READ-005's "document the bounded authoring vocabulary on
  `VarId`" option is the right call.
- **`CatalogRuleBuilder` error precedence:** "Query errors win" is the intended
  contract, it mirrors the fail-fast path, and it should be **documented on both
  catalog builders**. In the direct path, `RuleBuilder::try_query` returns the
  `QueryBuildError` immediately (`rule/mod.rs:162-171`), so a query error always
  surfaces before any build-time metadata validation. `CatalogRuleBuilder::build`
  pops `first_query_error` before delegating to `inner.build()`
  (`rule/mod.rs:308-313`), where `duplicate_field` and the missing-metadata
  checks live (`rule/mod.rs:233-261`); `CatalogLifecycleQueryBuilder::build` does
  the same (`first_error` popped before `inner.build()`, `lifecycle.rs:246-251`).
  The deferred path therefore reproduces the direct-path ordering — the first
  invalid query is the earliest-captured failure and always wins. The only
  asymmetry is that the deferred path wraps the error in
  `RuleBuildError::InvalidQuery` (`rule/mod.rs:310`) while `try_query` returns
  the raw error; that is the documented purpose of the catalog builder, not a
  precedence difference.
- **`QueryExpr`/`EmissionDecl` public surface:** `pub(crate)` for `QueryExpr`,
  `EmissionDecl`, `expression()`, and `emission()` would suffice — nothing
  outside the crate uses them. `api` is crate-private (`lib.rs:16`), and the
  public `rules` re-export (`lib.rs:49-57`) exposes `EventQuery`, `QueryDecl`,
  `EventRequirement`, the lifecycle types, and `QueryBuildError`, but not
  `QueryExpr`, `EmissionDecl`, or `QueryDiagnostic`. A workspace-wide search of
  the non-core crates finds no use of `.expression()`, `.emission()`,
  `QueryExpr`, `EmissionDecl`, or `QueryExpr::diagnostic_name`; providers
  construct only via `EventQuery::*`, `EventRequirement::argument`, and
  `QueryDecl::any|all|lifecycle` (e.g. `browser/remote_resource/mod.rs:15-59`,
  `node/filesystem/mod.rs:56-66`, `obsidian/workspace/events/mod.rs`). All
  consumers of the accessors are in-crate: the compiler validation and
  normalization passes (`pass1_3.rs:64-65`, `normalize.rs:34,42`,
  `pass4_10.rs:54,138-139`) and `QueryDecl`'s own `Display` (`mod.rs:532-536`);
  `QueryExpr::diagnostic_name` is exercised only by in-crate tests
  (`query/tests.rs:424-430`). Note that `expression()`/`emission()` must drop to
  `pub(crate)` **together with** the types, since a `pub` accessor cannot return
  a `pub(crate)` type. Keep `Display` for `QueryDecl`'s formatting and keep the
  `rules` re-export list as the sole public surface.
- **`ModuleSpecifierPattern::PatternValue`:** Confirmed retention. The enum is
  an explicit documented extension point ("An exact-module variant is the
  planned extension point", `module.rs:13-18`); today `import_exact` models exact
  specifiers as `IdentitySpec::LiteralString` (`constructors.rs:154-165`) and the
  module/export identities use `IdentitySpec::ModuleExport`
  (`composition.rs:44-47`), so `PatternValue::Package` (`module.rs:19-22`) is
  the only authored shape. It is not dead weight like READ-002/READ-005; keep it
  and re-point `import_exact` at a future exact variant when exact-module
  identities are authored — no separate decision is needed now.

## Coverage

- `glass-lint-core/src/api/mod.rs` (full)
- `glass-lint-core/src/api/classification.rs`, `classification/result.rs` (skim; owned by the classification chunk)
- `glass-lint-core/src/api/rule/mod.rs` (full)
- `glass-lint-core/src/api/rule/error.rs` (full)
- `glass-lint-core/src/api/rule/module.rs` (full; also `project::PackageSpecifier` contract)
- `glass-lint-core/src/api/rule/taxonomy.rs` (full)
- `glass-lint-core/src/api/rule/query/mod.rs` (full)
- `glass-lint-core/src/api/rule/query/{canonical,composition,constructors,declarations,error,event,explanation,expression,limits,value}.rs` (full)
- `glass-lint-core/src/api/rule/query/{tests,tests_extended}.rs` (full; `EVIDENCE_LIMIT`-style test usage, `diagnostic_name`, explanation output)
- `glass-lint-core/src/api/rule/query/lifecycle.rs`, `lifecycle/{endpoint,types}.rs` (full)
- Providers: `glass-lint-js/src/rules/browser/remote_resource/mod.rs`, `glass-lint-js/src/rules/node/filesystem/mod.rs` (representative catalog-builder + deferred-error callers); usage census of `Rule::catalog_builder`/`LifecycleQuery::catalog_builder` across `glass-lint-js` and `glass-lint-obsidian`
- Compiler consumers: `api/compiler/{mod.rs,rule.rs,normalize.rs,validate/pass1_3.rs,validate/pass4_10.rs}` (how declarations are lowered and consumed)
- `lint/catalog.rs` (`CompiledCatalogError` consumption, `Rule::id->RuleId`)
- `glass-lint-core/src/lib.rs` (`rules` re-export), `tests/integration/public_surface.rs`
- Limit/production cross-checks: `limits.rs` (`evidence_items`), `lint/linter.rs`, `project/session/mod.rs`, `lint/report/mod.rs`, `analysis/project/projection.rs`, `analysis/matching/evidence.rs` and its tests for `EVIDENCE_LIMIT`
