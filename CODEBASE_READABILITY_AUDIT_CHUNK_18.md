# Codebase Readability Audit — glass-lint-core Chunk 18: Rule authoring and query declarations

## Summary

Chunk 18 owns the validated provider-neutral rule-authoring boundary:
`api::rule` (`Rule`, `RuleBuilder`, `CatalogRuleBuilder`, `FirstError`,
`ModuleSpecifierPattern`, construction errors) and the full `api::rule::query`
declaration tree (`EventQuery`, `LifecycleQuery`, `QueryDecl`, `QueryExpr`,
`EmissionDecl`, `EventRequirement`, value/argument matchers, lifecycle
stages, explanations, limits). This is the external rule-authoring API that
provider crates (`glass-lint-js`, `glass-lint-obsidian`) and core examples
consume; the compiler (`api/compiler`, `analysis/matching`) consumes the same
types internally.

The chunk is generally well-factored: validation is centralized at
construction, `FirstError` is a good shared first-error owner, matcher
semantics live on the owning types, and the sealed `Into*` adapters are
sensibly macro-generated. The main problems are (1) parallel builder pairs
with divergent structure and duplicated `build` bodies, (2) a public contract
doc that names a method which does not accept what the doc claims, (3) a
magic-string sentinel shipped into the declared query model and re-detected
by string equality across the matching boundary, (4) duplicate bounded
canonical-collection helpers with a wrong limit constant for sinks, and (5)
several dead/redundant branches plus an inconsistent fallible-input
convention that forces callers into `Ok(...)` wrapping. No `unwrap`/`expect`/
`panic` exists in chunk production code (only one justified `debug_assert`).

## Findings

### [api/rule/query/lifecycle]

#### [ ] READ-001 — Parallel lifecycle builders duplicate state wiring and build logic; `CatalogLifecycleQueryBuilder` does not compose `LifecycleQueryBuilder`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:139-199, 201-245`

`LifecycleQueryBuilder` and `CatalogLifecycleQueryBuilder` both wrap the same
`LifecycleBuilderState` (lines 118-137) and duplicate the identical `build()`
body — destructure `LifecycleBuilderState`, take `invalid_operation`, then
`stages.build()` — at lines 189-198 and 235-244. They also each define their
own `record_operation` wiring (lines 132-137 vs 207-215). By contrast, the
parallel rule builders in `api/rule/mod.rs` solve the same deferred-vs-
immediate problem by composition: `CatalogRuleBuilder { inner: RuleBuilder,
first_query_error }` (rule/mod.rs:264-269). The two lifecycle builders are
parallel model types representing the same concept with a divergent surface:
`LifecycleQueryBuilder::source(EventQuery)` and
`CatalogLifecycleQueryBuilder::source(impl IntoLifecycleSource)` are the same
method name with different accepted types, and the same for `condition` /
`completion`. Production callers (all `glass-lint-js`/`glass-lint-obsidian`
lifecycle rules and core integration tests) use only
`LifecycleQuery::catalog_builder`; `LifecycleQuery::builder` is exercised only
by unit tests. A new stage or validation rule must be added to both builders
and both `build()` bodies.

**Recommendation:** Mirror the `CatalogRuleBuilder` pattern: have
`CatalogLifecycleQueryBuilder { inner: LifecycleQueryBuilder, first_error:
FirstError<QueryBuildError> }` so stage handling and `build()` are
single-sourced, deleting the duplicated `build` bodies and the two duplicate
`record_operation` helpers. Guardrails: keep deferred-error catalog behavior
(fail at `build`) distinct from immediate `try_*` propagation, and keep
`DuplicateLifecycleStage`, per-stage limits, and error identity unchanged.

**Fix Applied:** None so far.

### [api/rule/query/mod.rs, api/rule/mod.rs]

#### [ ] READ-002 — Module doc names `RuleBuilder::query` as the `IntoQueryDecl` entry point, but that method only accepts a finished `QueryDecl`; two same-named `query` methods collide

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:7-11`, `glass-lint-core/src/api/rule/mod.rs:151-154, 162-166, 272-276`

The query module doc states: "Rule authors create event queries and pass them
directly to [`RuleBuilder::query`] (via the [`IntoQueryDecl`] adapter)". In
fact `RuleBuilder::query(mut self, query: QueryDecl)` (rule/mod.rs:151)
accepts only a finished `QueryDecl`; the `IntoQueryDecl` adapter is honored
only by `RuleBuilder::try_query` (rule/mod.rs:162) and
`CatalogRuleBuilder::query` (rule/mod.rs:272). Two public builders therefore
share the method name `query` with different signatures and different error
timing (infallible-finished vs deferred-`Result`), both reachable through the
`rules` re-export (lib.rs:49-59). A rule author following the documented
primary API gets a compile error, and the same-named methods are an easy
trap. All provider crates happen to use `catalog_builder`, so the collision is
latent today, but the documented contract is wrong.

**Recommendation:** Either change `RuleBuilder::query` to accept
`impl IntoQueryDecl` (and keep `try_query` as the immediate-propagating
variant), or fix the module doc to name `try_query`/`catalog_builder` and
drop the "pass directly" claim. Guardrails: keep the deferred-vs-immediate
error semantics and the behaviors asserted in `api/rule/tests.rs`; provider
crates only use `catalog_builder`, so changing `RuleBuilder::query`'s
signature is low-risk.

**Fix Applied:** None so far.

### [api/rule/query/declarations.rs, constructors.rs, event.rs; analysis/matching]

#### [ ] READ-003 — Private-network identity is erased into a magic-string sentinel that the matching and evidence layers re-detect by `==`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/query/declarations.rs:79-80`, `glass-lint-core/src/api/rule/query/constructors.rs:193-200`, `glass-lint-core/src/api/rule/query/event.rs:48-50`, `glass-lint-core/src/analysis/matching/query/view.rs:345`, `glass-lint-core/src/analysis/matching/evidence.rs:176`

`EventQuery::string_private_network_address()` builds an
`IdentitySpec::LiteralString { predicate: PRIVATE_NETWORK_LITERAL }`, where
`PRIVATE_NETWORK_LITERAL` is the magic string
`"__glass_lint_private_network_literal__"`. The typed query model therefore
does not record "this is a private-network predicate"; three downstream sites
must re-detect the fact by string comparison: `IdentitySpec::display_name`
(event.rs:48-50) maps the sentinel to `PRIVATE_NETWORK_EVIDENCE_SYMBOL`, the
matching layer (view.rs:345) switches on `predicate == PRIVATE_NETWORK_LITERAL`
to select `private_network_match` instead of `literal.contains(predicate)`, and
the evidence span logic (evidence.rs:176) switches on the symbol. The declared
`LiteralString` type is overloaded with an internal marker, so the marker must
stay byte-identical across three modules or private-network matching silently
degrades to substring matching; an authored string equal to the sentinel would
also be misclassified.

**Recommendation:** Add a dedicated `IdentitySpec::PrivateNetworkAddress`
variant produced by `string_private_network_address`, keeping `LiteralString`
for genuine substring predicates, and delete the marker comparisons in
event.rs, view.rs, and evidence.rs (which then match on the variant).
Guardrails: preserve fail-closed boundary-aware private-network matching
(`literals.matching(private_network_match)`), the evidence symbol "private
network address", and the `StringContains` evidence kind.

**Fix Applied:** None so far.

### [api/rule/query/value.rs, lifecycle/types.rs]

#### [ ] READ-005 — Duplicated bounded canonical-collection helpers, with the sink pre-bound keyed to the events limit

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/value.rs:73-126`, `glass-lint-core/src/api/rule/query/lifecycle/types.rs:141-216`

`value.rs` implements "collect → bound → trim/normalize → sort → dedup" via
`canonicalize_strings` + `bounded_canonical_values` + `bounded_strings`, while
`lifecycle/types.rs` implements the same pattern generically via
`CanonicalLifecycleItems::new` (sort/dedup/bound into `Box<[T]>`) plus
`bounded_lifecycle_items` for fallible conversion. Two parallel constructions
of the same invariant (bounded, canonical, deterministic collection) live in
the same module tree with parallel error mapping. `bounded_lifecycle_items`
(line 207) pre-bounds every collection with `MAX_LIFECYCLE_EVENTS`, including
sink collections whose final bound is `MAX_LIFECYCLE_SINKS`
(types.rs:287-294); the two constants coincide at 64 today, so the wrong
limit is only latent, but a future divergence would silently change which
bound applies to sinks.

**Recommendation:** Consolidate into one generic bounded canonical collection
owned by `api::rule::query` that takes the limit, empty-error, and label as
parameters; have `bounded_lifecycle_items` pass the correct per-collection
limit instead of hardcoding `MAX_LIFECYCLE_EVENTS`. Guardrails: keep the
distinct error variants (`EmptyLifecycleCondition` vs `EmptyLifecycleSinks`,
`EmptyStaticValue` vs `EmptyCollection`), stable `CollectionTooLarge` labels,
sort determinism, and the separate `MAX_LIFECYCLE_EVENTS` / `MAX_LIFECYCLE_SINKS`
bounds.

**Fix Applied:** None so far.

### [api/rule/query/composition.rs; api/rule/query/mod.rs]

#### [ ] READ-007 — Fallible-input convention is applied three different ways, forcing callers into ad-hoc `Ok(...)` wrapping

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/composition.rs:252-267`, `glass-lint-core/src/api/rule/query/mod.rs:445-471`, `glass-lint-core/src/api/rule/query/lifecycle/types.rs:11-34`, `glass-lint-core/examples/query_declarations.rs:74, 80, 95, 101, 117`

The authoring boundary accepts fallible inputs in three incompatible shapes:
(a) plain value or `Result` via a sealed trait (`IntoQueryDecl` mod.rs:445-471,
`IntoLifecycleSource`/`IntoLifecycleEvent`/`IntoLifecycleCondition`/
`IntoLifecycleSink`/`IntoLifecycleCompletion` types.rs:11-34); (b) bare
`Result` only — `QueryDecl::lifecycle(Result<LifecycleQuery, QueryBuildError>)`
(composition.rs:252); (c) plain validated value only — `RuleBuilder::query`,
`LifecycleQueryBuilder::condition`. The example file — the crate's own
teaching material — demonstrates the friction: it wraps already-`unwrap`ed
values in `Ok(...)` for `.source(Ok(EventQuery::...))` (query_declarations.rs:74)
and wraps a plain `LifecycleQuery` in `Ok(...)` for
`QueryDecl::lifecycle(Ok(lifecycle))` (lines 95, 117), while
`LifecycleCondition::event(...)` (a `Result`) is passed unwrapped to
`.condition(...)`. Rule authors must learn per-method which shape is
accepted, and no-op `Ok` wrappers hide the real error flow.

**Recommendation:** Standardize on the adapter pattern across the boundary:
have `QueryDecl::lifecycle` (and `QueryDecl::any`/`all`) accept plain-or-
`Result` inputs so callers never write `Ok(...)`, and align the
plain-only builder methods on the same adapters. Guardrails: keep `Result`
inputs working at the deferred catalog boundary, keep the sealed traits (no
blanket impls on foreign types), and preserve `QueryBuildError` mapping.

**Fix Applied:** None so far.

### [api/rule/query/mod.rs]

#### [ ] READ-004 — `EventQuery::constraints()` exposes storage-shaped access publicly while sibling accessors are `pub(crate)` and no external caller exists

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:174-176`

On the public `EventQuery`, `constraints()` returns the raw `&[ArgumentConstraint]`
storage slice, while the sibling accessors `var()`, `event()`, and `identity()`
(lines 162-172) are `pub(crate)`. Every consumer is internal to the crate — the
compiler (`api/compiler/normalize_all.rs:171`, `validate/pass1_3.rs:26`,
`validate/pass4_10.rs:77`, `normalize.rs:471`) and tests; no provider crate,
harness, or example reads it. The accessor's ordering/boundedness invariant
(the vec is kept sorted by `ArgumentIndex` via `push_argument_constraint`) is
not documented, so an external reader cannot rely on it. This widens the
public surface against the AGENTS.md rule "Do not expose internal storage for
caller convenience."

**Recommendation:** Make `constraints()` `pub(crate)` to match the sibling
accessors, or if a provider need genuinely exists, expose a narrow derived
operation instead of the raw slice. Guardrails: the compiler and tests keep
full access; do not change the sorted ordering or the per-argument/predicate
bounds.

**Fix Applied:** None so far.

### [api/rule/query/composition.rs, value.rs, expression.rs, lifecycle.rs]

#### [ ] READ-006 — Dead placeholder and redundant checks in bounded construction paths, plus stale no-op `allow` attributes

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/query/composition.rs:174, 241-247`, `glass-lint-core/src/api/rule/query/value.rs:104-109`, `glass-lint-core/src/api/rule/query/expression.rs:81`, `glass-lint-core/src/api/rule/query/lifecycle.rs:8, 10, 16`

Three dead/redundant artifacts exist in the chunk. (1) In
`QueryDecl::any_impl`, `first_emission` is always `Some` after the branch loop
because empty input already returns `QueryBuildError::EmptyAlternatives`, so
`unwrap_or_else(Self::default_emission)` (composition.rs:174) can never take
the fallback and `default_emission` (241-247) — which fabricates
`EmissionDecl { primary_var: VarId::new(0), kind: MatchKind::Call, symbol: "" }`
— is unreachable; it also implies placeholder emissions are legitimate when
the public API cannot produce one. (2) In `bounded_canonical_values`, the
pre-push bound (value.rs:92-99) guarantees `parsed.len() <= MAX` before
canonicalization, so the post-sort re-check `if parsed.len() > MAX` (value.rs:104-109)
is unreachable. (3) Stale allowances: `#[allow(dead_code)]` on
`QueryExpr::kind()` (expression.rs:81) is a no-op because `kind()` is used by
`explanation.rs`, `compiler/normalize.rs`, `normalize_all.rs`, and
`validate/pass4_10.rs`; the three `#[allow(unused_imports)]` blocks in
lifecycle.rs guard `pub(crate) use` re-exports that are referenced by
`analysis/flow/planning.rs:31` and `compiler/normalize.rs:373-437`, so they are
no-ops too.

**Recommendation:** Delete `default_emission` and restructure `any_impl` to
use the already-required `first_emission` (unwrap only after the empty-input
guard is documented); delete the redundant post-canonicalization check in
value.rs; remove the four stale no-op `allow` attributes. Guardrails: keep
`EmptyAlternatives` on empty input, keep `MAX_STATIC_ALTERNATIVES` enforced on
input size, and keep the re-export paths used by planning.rs and normalize.rs.

**Fix Applied:** None so far.

### [api/rule/query/mod.rs, composition.rs]

#### [ ] READ-008 — `EmissionDecl::is_compatible_with` hides a behavior switch behind a boolean

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:359-363`, `glass-lint-core/src/api/rule/query/composition.rs:162`

`EmissionDecl::is_compatible_with(&self, other, allow_symbol_difference: bool)`
toggles materially different outcomes (ignore vs compare the evidence symbol)
behind a single boolean, with one call site
(composition.rs:162: `explicit_symbol.is_some()`). The caller must know that a
`true` flag means the symbol comparison is skipped entirely, and the two
behaviors are not separately discoverable or testable.

**Recommendation:** Split into two narrow methods, e.g.
`is_compatible(&self, other)` and `is_compatible_with_aggregate_symbol(&self,
other)`, so the distinct semantics are named. Guardrails: keep the primary-var
and `MatchKind` comparisons identical in both, and keep the
`EvidenceProjection` failure behavior in `any_impl` unchanged.

**Fix Applied:** None so far.

### [api/rule/module.rs]

#### [ ] READ-009 — Single-variant `PatternValue` enum adds only indirection today

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/module.rs:13-16`

`ModuleSpecifierPattern { value: PatternValue }` wraps a one-variant enum
`PatternValue::Package(PackageSpecifier)`; `matches` and `as_str` both
destructure the single variant and delegate. The enum adds no vocabulary or
invariant over a plain field. The core `ARCHITECTURE.md` states "exact module
identities remain distinct from package-root patterns," so a second variant
may be genuinely planned, but today this is a wrapper whose only purpose is a
hypothetical future.

**Recommendation:** Collapse to `ModuleSpecifierPattern { package:
PackageSpecifier }` (or a tuple struct) unless an exact-module variant is
actively being added; if the variant is retained, document the planned
extension point. Guardrails: keep the boundary-aware `matches` semantics
(root or `/subpath`) and the `MatcherBuildError::InvalidModuleSpecifier`
conversion used by `checked_package` (constructors.rs:354-357).

**Fix Applied:** None so far.

## Systemic Themes

- **Three fallible-input conventions.** The chunk applies plain-only,
  `Result`-only, and plain-or-`Result` (sealed adapter) shapes across
  `RuleBuilder`/`CatalogRuleBuilder`, `LifecycleQueryBuilder`/
  `CatalogLifecycleQueryBuilder`, `QueryDecl::lifecycle`, and the
  `IntoLifecycle*`/`IntoQueryDecl` traits. This is the root of both READ-002
  and READ-007; a single convention would remove the `Ok(...)` wrapping and
  the doc mismatch.
- **Repeated magic variable IDs.** `VarId::new(0)` is hardcoded as the event
  variable and as a placeholder primary var in
  `query/mod.rs:183` (`EventQuery::from_parts`), `composition.rs:243`
  (`default_emission`), and `composition.rs:261` (`QueryDecl::lifecycle`);
  `VarId::new(1)` is the member-subject object var (composition.rs:284). The
  sentinel "0 is the event var" is an implicit invariant spread across files
  rather than a named constant or constructor.
- **Parallel renderers.** `QueryExpr`'s `Display` impl
  (expression.rs:166-219) and `explain_expression` (explanation.rs:6-27) each
  match the full `QueryExprKind`/`QueryPredicate`/`EventSpec`/`IdentitySpec`
  surface; adding a predicate or event variant requires coordinated edits in
  both renderers (plus `diagnostic_name`).
- **Stale allowances.** Several `#[allow(dead_code)]` /
  `#[allow(unused_imports)]` attributes in the chunk (expression.rs:81,
  lifecycle.rs:8-20) are no-ops given the current callers; they obscure which
  code is genuinely in use.

## Open Questions

- **`RuleBuilder` / `LifecycleQueryBuilder` (immediate builders) have no
  production callers** — only unit tests exercise them, while all provider
  crates and integration tests use `catalog_builder`. Is the immediate builder
  an intentional public surface for non-catalog rule authors, or legacy that
  should be removed (which would also shrink the READ-002/READ-007 surface)?
- **`IdentitySpec`, `EventSpec`, and `EmissionDecl`'s fields stay
  `pub(crate)` while the enclosing types are public.** Is this deliberate to
  keep the compiler's view internal, or is external introspection of event
  shape an intended future provider capability?
- **`PatternValue` (module.rs:13) and `EventRequirementKind` (query/mod.rs:386)
  are single-variant enums.** The architecture text hints that exact-module
  patterns may be added; until that lands, READ-009's recommendation applies
  only if no variant is planned.

## Coverage

- **Files reviewed (chunk):** `api/rule/mod.rs`, `api/rule/error.rs`,
  `api/rule/module.rs`, `api/rule/query/mod.rs`, `constructors.rs`,
  `composition.rs`, `declarations.rs`, `error.rs`, `event.rs`,
  `explanation.rs`, `expression.rs`, `value.rs`, `limits.rs`,
  `lifecycle.rs`, `lifecycle/types.rs`, `lifecycle/endpoint.rs`, plus the
  chunk's unit tests (`rule/tests.rs`, `query/tests.rs`,
  `query/tests_extended.rs`, `value/tests.rs`, `lifecycle/tests.rs`).
- **Callers traced:** provider crates `glass-lint-js` (network, browser,
  node, electron, dynamic-code rules; lifecycle rules in `file_dialog`,
  `script_injection`, `remote_resource`), `glass-lint-obsidian`
  (`lifecycle/events`, `network/request`, and the catalog-rule rules),
  `glass-lint-core/examples/query_declarations.rs`,
  `tests/integration/public_surface.rs`, and the internal consumers
  `api/compiler` (normalize.rs, normalize_all.rs, validate/pass1_3.rs,
  pass4_10.rs, contradiction.rs), `analysis/matching/query/view.rs`,
  `analysis/matching/evidence.rs`, `analysis/flow/planning.rs`.
- **Verified with:** `rg` traces of every public symbol's consumers;
  `cargo check -p glass-lint-core` (no warnings); `git status --short`
  confirms no source files were modified by this audit.
