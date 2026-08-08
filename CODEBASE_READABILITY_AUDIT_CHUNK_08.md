# Codebase Readability Audit

## Summary

Chunk 8 owns the provider-facing rule declaration boundary: typed event and
lifecycle queries, bounded value predicates, rule builders, catalog adapters,
and the public `rules` facade. The semantic newtypes and sealed adapters keep
most invalid inputs out of the compiler, but the boundary still has several
split ownership paths. Argument-bearing events can remain physically invalid
until compilation, deferred builders duplicate the immediate builder API,
and public declaration types are either inconsistent adapters or opaque
implementation carriers. These issues make rule-authoring behavior depend on
which builder path a caller selects and make the public facade harder to use
without improving the underlying semantic model.

## Findings

### Event and value declaration boundary

#### [x] READ-029 — Reject argument constraints at the event declaration boundary

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/constructors.rs:264-282`; `glass-lint-core/src/api/rule/query/mod.rs:462-479`; `glass-lint-core/src/api/compiler/physical.rs:180-200`

`EventQuery::with_arg` accepts every `EventQuery` and only validates the
argument index and constraint budget. `QueryDecl::all` likewise turns an
`EventRequirement::argument` into an argument predicate without checking the
selected event kind. Consequently an import, string reference, member read,
property write, or other non-call event can carry argument constraints in the
authored logical declaration. The invalid combination is rejected later by
the compiler’s structural validation pass (and is checked defensively again
by physical-plan validation), rather than when the declaration is built.

This splits ownership of a basic declaration invariant between the public
query API and the compiler validation/planning layers. A catalog author can
therefore construct and retain a `QueryDecl` that the API presents as
validated, while the actual failure appears later as a matcher/compiler error.
It also makes the two argument entry points (`with_arg` and `QueryDecl::all`)
disagree about where the same semantic rule is enforced.

**Recommendation:** Give the event declaration layer one `supports_arguments`
predicate owned by `EventSpec`, and use it from both `with_arg` and the
same-event composition path. Return a query-construction error for unsupported
event kinds before building the constraint-bearing declaration; constructors
are intentionally rejected because the physical planner has no constructor
argument operator. Keep physical validation as a defensive invariant check,
preserve call/member-call support, and retain existing argument/group bounds.

**Fix Applied:** `EventSpec::supports_arguments` now owns the call/member-call
predicate. Both `EventQuery::with_arg` and `QueryDecl::all` reject unsupported
event kinds with `QueryBuildError::ArgumentsRequireCallEvent`, while compiler
validation delegates to the same predicate as a defensive check. Integration
tests cover both public construction paths.

**Fix Applied:** None so far.

### Rule builder ownership

#### [x] READ-030 — Centralize immediate and deferred rule-builder state

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/mod.rs:130-306`

`RuleBuilder` owns metadata, query storage, duplicate-field tracking, and
metadata validation, while `CatalogRuleBuilder` wraps it and republishes a
second `query`, `queries`, `description`, `severity`, `confidence`, and
`build` surface. The wrapper adds a separate `FirstError<QueryBuildError>` and
reimplements the query loop and stage of error precedence, even though both
builders ultimately construct the same `Rule` and use the same limits.

The split is visible in normal usage: provider catalogs use the deferred
builder so fallible constructor results can remain in fluent expressions,
whereas the ordinary builder exposes a different immediate-error contract.
Fixing or adding a metadata rule now requires checking both public surfaces,
and the two paths do not report failures at the same construction boundary.

**Recommendation:** Keep one owner for rule metadata, query storage, limits,
and final validation. Retain `CatalogRuleBuilder` because provider catalogs
and public catalog helpers use deferred `Result` handling, but reduce it to a
thin named adapter over shared state/error policy rather than a second
independently validated builder. Make error precedence and first-error policy
explicit and shared, while preserving deterministic query order.

**Fix Applied:** Added one shared `RuleBuilder::try_add_query` operation for
query conversion and storage. Immediate and deferred builder APIs now use the
same insertion path, while `CatalogRuleBuilder` retains only its deferred
first-error policy and metadata adapter behavior.

### Lifecycle authoring adapters

#### [x] READ-031 — Make deferred lifecycle stages accept the same typed inputs

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:494-545, 585-646`

The immediate lifecycle builder accepts a prebuilt `LifecycleCondition` via
`condition` and accepts either a condition or a `Result` through
`try_condition(impl IntoLifecycleCondition)`. The deferred builder is not
substitutable: its `condition` method takes exactly
`Result<LifecycleCondition, QueryBuildError>`, while its `source` and
`completion` methods use the broader adapter traits and accept both validated
values and fallible results. A caller holding an already-built condition can
use `LifecycleQueryBuilder::condition` but cannot pass that same value to
`LifecycleQuery::catalog_builder(...).condition(...)`.

The mismatch exposes the construction timing of one field in the public API
and forces callers to wrap or reconstruct values solely to select the catalog
error policy. It also leaves duplicate-stage handling split between the
immediate setter, which silently ignores a second non-fallible stage, and the
deferred setter, which records a typed error.

**Recommendation:** Change the deferred condition setter to use
`IntoLifecycleCondition`, centralize stage assignment and duplicate detection
in the lifecycle builder owner, and let the deferred adapter only record
fallible errors. Keep the immediate `try_*` methods for call-site propagation,
and make both non-fallible setters reject duplicate stages consistently rather
than silently ignoring one path.

**Fix Applied:** Centralized condition and completion insertion and duplicate
stage detection in `LifecycleQueryBuilder`. The catalog condition adapter now
accepts both `IntoLifecycleCondition` values and fallible results, while
non-fallible immediate setters retain duplicate errors for `build()`.

### Public declaration surface

#### [ ] READ-032 — Remove or complete opaque public logical-value types

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:40-46, 482-498`; `glass-lint-core/src/api/rule/query/expression.rs:9-24, 225-299`; `glass-lint-core/src/api/rule/query/value.rs:38-70`

`QueryExpr`, `AnyExpr`, and `AllExpr` are publicly re-exported, and
`QueryDecl::expression()` exposes a `&QueryExpr`, but their constructors,
expression kind, branch iterators, and branch contents are crate-private.
Likewise `ValueMatcherKind::StaticString` contains the public
`StaticStringPredicate`, while the predicate constructor and kind accessor
are crate-private. External rule authors can hold, compare, display, or ask
for a diagnostic name from these values, but cannot inspect their validated
semantic contents or use them to build a public transformation.

This is an incomplete boundary: the types look like public declaration data
but behave as compiler-owned opaque carriers. It increases the supported API
surface and exposes internal expression/value representation without giving
callers a stable semantic view. It also makes `QueryDecl::expression()` a
misleading public accessor, since consumers cannot meaningfully traverse the
returned declaration.

**Recommendation:** Keep `QueryExpr` as the small read-only diagnostic view
already used by the public composition API, but remove the unused public
`AnyExpr`/`AllExpr` re-exports and do not expose `QueryExprKind`, branch
storage, compiler slots, or mutable collections. For value predicates, expose
only semantic accessors that callers can use today; otherwise keep the
compiler-facing predicate kind private. Preserve centralized constructors and
the private physical-plan representation.

**Fix Applied:** None so far.

### Top-level facade

#### [ ] READ-033 — Give rule builders one stable public name and path

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/lib.rs:69-80`; `glass-lint-core/src/api/rule/mod.rs:79-97, 130-306`

The public `glass_lint_core::rules` facade re-exports
`CatalogRuleBuilder` under the generic name `Builder`, but does not re-export
`RuleBuilder` or `CatalogRuleBuilder` under their actual names. At the same
time, `Rule::builder` and `Rule::catalog_builder` expose two different
builder contracts from the `Rule` impl. Callers therefore encounter a
generic `rules::Builder` for the deferred catalog path, an otherwise unnamed
ordinary builder returned by `Rule::builder`, and duplicated methods with
different error timing.

The alias hides the important distinction that this builder deliberately
defers query errors, while the ordinary builder is the immediate propagation
path. It also makes helper functions and public signatures harder to express:
the facade does not provide a stable named type for either builder even though
both are public authoring concepts.

**Recommendation:** Re-export `RuleBuilder` and `CatalogRuleBuilder` under
their actual semantic names from `rules`; remove the generic `Builder` alias.
Keep the two names because immediate and deferred error timing are real
policies, but share their state/validation owner as in READ-030. Make
`Rule::builder` and `Rule::catalog_builder` return those named types without
changing provider catalog ergonomics.

**Fix Applied:** None so far.

## Systemic Themes

- The query API validates names, paths, collections, and budgets well, but
  event-kind compatibility is still owned by the physical planner for some
  authored combinations. Declaration invariants should be established before
  lowering, with later phases retaining only defensive checks.
- Immediate and deferred authoring paths are useful policies, but their state,
  stage semantics, and adapter vocabulary should have one domain owner. A
  wrapper should change error timing, not duplicate the rule/lifecycle model.
- Public declaration values should either be stable semantic views or private
  implementation details. Opaque public structs and generic facade aliases
  make the API larger without making extensions or inspection safer.

## Decisions

- Argument constraints are supported only for calls and member calls. The
  declaration layer should reject constructor arguments, matching the current
  physical access path, and retain the later validator as a defensive check.
- Deferred catalog construction is an active API used by provider catalogs,
  examples, tests, and downstream integrations. Keep it as a named adapter,
  but do not duplicate rule-builder state or validation policy.
- Keep the small read-only `QueryExpr::diagnostic_name` inspection used by the
  public composition surface. Do not add a general logical-tree view without
  a current consumer; explanations remain the supported detailed inspection.

## Coverage

Reviewed only Chunk 8, “Rule authoring and catalog integration,” from
`CODEBASE_STRUCTURE_CORE.md`, including the validated rule builder boundary,
query constructors and composition, lifecycle builders and sealed adapters,
bounded value matchers, module patterns, rule taxonomy and IDs, and the
top-level `rules` facade. Existing Chunk 1 through Chunk 7 audit history was
used to continue IDs at READ-029. No source, test, configuration, dependency,
or other documentation files were changed; this chunk audit file is the only
new artifact.
