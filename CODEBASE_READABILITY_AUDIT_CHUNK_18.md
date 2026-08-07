# Codebase Readability Audit — Chunk 18

## Summary

Chunk 18 owns the rule-authoring boundary: rule metadata and errors, module
patterns, event/query declarations, logical composition, lifecycle builders,
argument/value matchers, and taxonomy values. The declaration types are mostly
opaque and bounded, and the sealed adapters keep provider callers on the
validated construction path. The main architectural risks are a provider
taxonomy retained in the provider-neutral core and then discarded, mandatory
query validation delayed until catalog construction, public event/identity
enums whose variants bypass constructor validation, duplicated logical-query
assembly, and error information lost when module patterns cross the query
boundary.

The deferred builder-error and `ValueMatcher::equals` findings from Chunk 15
were reviewed and are not repeated. Compiler IR, physical-plan, normalized
constraint, and internal compiler-error findings from Chunk 17 were also
excluded; this report stays at the authored declaration and public API
boundary.

## Findings

### Rule metadata and crate ownership

#### [x] READ-086 — Remove provider taxonomy from the provider-neutral rule core

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/rule/taxonomy.rs:1-85`; `api/rule/mod.rs:28-103,177-215,245-252`; catalog compilation in `api/compiler/rule.rs:80-103` and metadata projection in `lint/catalog.rs:141-153`

`Category` is explicitly a provider-defined hierarchical category, but it is
implemented and re-exported by `glass-lint-core`, stored on the core `Rule`,
and accepted by every provider rule factory. The compiled record does not
retain it, and `RuleCatalog::metadata` does not emit it; a search of production
callers finds no read of `Rule::category()`. Thus core owns validation and
storage for policy metadata that disappears at the catalog boundary.

This violates the core/provider boundary and creates dead state in the rule
builder. A provider adding category semantics must change core taxonomy and
core rule APIs even though matching, compilation, and the current report
metadata do not need the value. It also makes the public `Rule` appear to
promise category preservation when the compiled catalog silently drops it.

**Recommendation:** Move category construction and storage to the provider
catalog or provider-owned metadata layer. Core reports currently do not
serialize categories, so remove `Category`, the `Rule` field/accessor,
duplicate-field builder branch, and unused compiled-path storage from core
after provider callers migrate; if categories must be serialized later, attach
them in the provider/report adapter where they are actually consumed. Preserve
rule IDs, descriptions, severity, confidence, query explanations,
duplicate-field diagnostics for metadata that remains in core, and the absence
of provider names/categories from core semantics.

**Fix Applied:** Removed the provider-defined `Category` type, rule storage and
accessors, category validation and duplicate-field handling from core, then
migrated provider, test, example, and CLI declarations to the metadata that
core actually consumes. Verified with `make fmt && make ci`.

#### [x] READ-087 — Seal the nonempty-query invariant at rule construction

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/mod.rs:28-45,107-116,223-263`; delayed check in `glass-lint-core/src/lint/catalog.rs:59-73`

`RuleBuilder::build` can return a `Rule` with zero query declarations even
though `Rule` is documented as a validated rule with canonical query
declarations. The only production caller of `Rule::require_queries` is
`RuleCatalog::new`, which consumes the apparently valid rule and converts the
missing-query case to the broader `MatcherBuildError::MissingRequired` and
then `ProviderCatalogError::InvalidRule` path.

The declaration boundary therefore has two validity states: a public rule can
be built successfully but cannot enter a catalog. This forces callers to know
that “build” is not the point at which a rule becomes usable and leaves the
query-presence invariant owned by a catalog adapter rather than the type that
stores the query vector.

**Recommendation:** Require at least one query in `RuleBuilder::build`, add a
dedicated `RuleBuildError::MissingQuery` (or equivalent), and delete
`Rule::require_queries`, `MatcherBuildError::MissingRequired`, and the catalog
preflight branch after callers migrate. Preserve the existing query-count
bound, deferred construction-error ordering where intentionally supported,
catalog duplicate-ID validation, and the rule that no empty matcher reaches
compiler planning.

**Fix Applied:** Added `RuleBuildError::MissingQuery`, enforced the nonempty
query invariant in `RuleBuilder::build`, and removed the delayed catalog
preflight plus obsolete `require_queries`/`MissingRequired` path. Verified
with `make fmt && make ci`.

### Query declaration surface

#### [x] READ-088 — Make event and identity specifications opaque validated values

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** External API
- **Location:** `glass-lint-core/src/api/rule/query/event.rs:6-81`; re-export in `api/rule/query/mod.rs:43-46` and `lib.rs:33-43`; constructor validation in `api/rule/query/constructors.rs:19-273`; compiler validation in `api/compiler/validate/error.rs:173-257`

`EventSpec` and `IdentitySpec` are exported through `glass_lint_core::rules`
as public enums with public fields. Their authored constructors validate names,
chains, module patterns, and literal values, but callers can also construct
variants such as `IdentitySpec::Global { name: ... }`,
`IdentitySpec::Rooted { path: ... }`, or `EventSpec::MemberCall { member: ... }`
directly. The compiler later checks emptiness and dimension compatibility, so
the public declaration types can carry invalid or incompatible values despite
the query module documentation saying that types are validated at
construction. The two enums are primarily returned by `EventQuery` getters;
there is no corresponding public operation that needs arbitrary enum values
to be fed back into a query.

This leaks the internal dimension vocabulary and makes the compiler’s late
validation a second constructor for public values. A future caller or public
helper that accepts these enums can accidentally bypass the canonical
`EventQuery` matrix, producing a declaration that fails far from its creation
site and requiring every consumer to retain empty/path compatibility checks.

**Recommendation:** Keep event and identity representations private or expose
opaque read-only views, and provide validated semantic constructors/predicate
methods for the combinations that external rule authors actually need. If
pattern matching is part of the supported API, make the variants carry only
validated private newtypes and remove raw field construction; retain compiler
validation as a defensive check. Preserve all existing constructor coverage,
provider-neutral identity distinctions, stable diagnostic names, fail-closed
handling of malformed test/internal declarations, and rooted/module/package
semantics.

**Fix Applied:** Made `EventSpec` and `IdentitySpec` crate-private compiler
representations, removed their public re-exports and narrowed `EventQuery`
accessors, while retaining the validated constructor matrix and defensive
internal validation. Removed matcher error and module-pattern paths that became
unreachable after the boundary was closed. Verified with `make fmt && make ci`.

#### [x] READ-089 — Centralize event-selection expression assembly

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/constructors.rs:354-377`; `api/rule/query/composition.rs:196-243,281-322`; query expression types in `api/rule/query/expression.rs:11-148`

The same authored event is lowered into logical expression atoms in three
different ways. `EventQuery::into_query` wraps the leaf and derives emission;
`QueryDecl::all` destructures an `EventQuery` and manually creates
`SelectEvent`, `EventKind`, and `EventIdentity` branches before adding
argument requirements; and `member_subject_query` repeats those same three
branches before adding object binding and member-subject predicates. The
event/identity/emission mapping is therefore distributed across the leaf
constructor and two composition paths.

This is a maintenance hazard at the authoring/compiler seam: adding a new
event field, evidence rule, or selection predicate requires finding every
manual atom assembly site. It also makes the high-level convenience methods
look equivalent while relying on different code to establish the same binding
and emission invariants.

**Recommendation:** Put one private operation on `EventQuery` (or a focused
query-declaration assembler) that emits the validated selection, event-kind,
identity, and argument atoms plus inferred emission metadata. Have `into_query`,
`QueryDecl::all`, and `member_subject_query` add only their genuinely distinct
composition predicates, then delete the repeated branch construction and
parallel symbol/kind derivation. Preserve variable bindings, same-event
correlation, member-object relations, argument ordering and bounds, evidence
projection checks for `Any`, and the existing public convenience constructors.

**Fix Applied:** Added a private `EventSelectionAssembly` owned by `EventQuery` that centralizes event selection, identity, argument predicates, and inferred emission metadata. Leaf, same-event, and member-subject constructors now reuse it while retaining their distinct expression composition.

### Error and bounded collection adapters

#### [x] READ-090 — Preserve module-pattern errors through query constructors

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Conversion
- **Location:** `glass-lint-core/src/api/rule/module.rs:19-40`; query conversions in `api/rule/query/constructors.rs:51-61,102-113,151-186`; error definitions in `api/rule/query/error.rs:5-29`

`ModuleSpecifierPattern::package` returns a detailed
`MatcherBuildError::InvalidModuleSpecifier`, but each package-aware query
constructor discards that error with `map_err(|_|
QueryBuildError::InvalidScopePackage)`. The same conversion is repeated for
package calls, package member calls, package member reads, and package
imports. As a result, an invalid package name loses its input/context and all
failure modes collapse to one query error, while exact module construction
uses a different `EmptyModuleSpecifier` path.

The query authoring API consequently has multiple owners for module-pattern
validation and error translation. Adding another package-aware constructor
requires copying the mapping, and callers cannot distinguish an invalid
package grammar from an unrelated scope-policy failure or report the original
invalid value.

**Recommendation:** Define one query-facing module-pattern error conversion
or a shared structured `ModuleSpecifierError`, implement the conversion once,
and use it from all package-aware constructors. Preserve the distinction
between exact and package-root matching, package-boundary semantics, stable
`QueryBuildError` diagnostics, and the no-panics/fail-closed behavior of
`ModuleSpecifierPattern`; delete the repeated wildcard `map_err` arms after
the canonical conversion is adopted.

**Fix Applied:** Centralized package-pattern construction in one query helper
and changed `InvalidScopePackage` to retain the underlying module-specifier
message. Verified with `make fmt && make ci`.

## Systemic Themes

- The rule API mixes provider metadata with provider-neutral semantic
  declarations. Core should own validated matching intent and generic report
  fields; provider catalogs should own policy taxonomy and presentation data.
- Construction is generally validated, but public enum variants and delayed
  rule checks let invalid or unusable declarations survive past the natural
  owner. Phase boundaries should return one sealed declaration type.
- Query convenience constructors are valuable, but they need one internal
  assembler so the public surface remains broad while binding, evidence, and
  identity invariants have one implementation.

## Decisions

- Categories are not part of the provider-neutral serialized core report;
  current classification output has no category field. Provider catalogs own
  optional category metadata and any provider-facing serialization or
  namespacing after the core taxonomy is removed.
- `EventSpec` and `IdentitySpec` are authored through fully validated
  constructors but are opaque after construction. Consumers can inspect
  semantic accessors, not build compiler-invalid variants by struct literal.
- Query-level errors retain stable codes and preserve the underlying typed
  module-pattern error as source context. This gives callers deterministic
  diagnostics without discarding the detailed package-pattern cause.

## Coverage

- **Reviewed modules:** `api::rule::{mod,error,module,taxonomy}` and
  `api::rule::query::{mod,composition,constructors,error,event,expression,
  lifecycle,value}`; representative compiler validation, catalog
  compilation, provider rule factories, and public-surface tests.
- **Workflow traced:** provider rule construction → query/lifecycle/value
  declaration → catalog query-presence validation → compiler-facing query
  conversion → compiled metadata and provider catalog/report boundaries.
- **Prior overlap check:** Deferred fluent-builder errors and silent empty
  `ValueMatcher::equals` were compared with Chunk 15 and excluded; compiler
  IR/physical compatibility and internal diagnostic-boundary findings were
  compared with Chunk 17 and excluded.
- **Verification:** Read-only audit; no source, test, configuration, or
  dependency files were changed.
