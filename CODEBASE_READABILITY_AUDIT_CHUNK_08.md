# Codebase Readability Audit — Chunk 8

This audit covers Chunk 8 of `CODEBASE_STRUCTURE_CORE.md`: rule authoring and
catalog integration. It is an architectural review only; no source changes
were made.

## Summary

The authoring boundary generally has the right direction: provider code builds
validated semantic declarations, Core owns bounded normalization, and catalog
construction drops declarations after compiling immutable plans. The main
risks are at the seams between those stages. Catalog identity is repeatedly
converted back to strings, derived catalog indexes are maintained beside
their source vector, deferred builder modes repeat the same state machine,
compiler variable slots leak into the public API, and several declaration
invariants are validated or reconstructed in multiple owners. One public
module-pattern type also promises a broader contract than it implements.

## Findings

#### [x] READ-001 — Catalog identity is lost and rebuilt through sentinel strings

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Conversion / API
- **Location:** `glass-lint-core/src/lint/catalog.rs:49-83`; error shapes at `glass-lint-core/src/api/rule/error.rs:43-55`; compilation mapping at `glass-lint-core/src/api/compiler/catalog.rs:11-43`
- **Representative callers:** `RuleCatalog::new` validates the provider with `"{provider}:placeholder"`, creates full IDs by formatting strings, then reparses compiler-error `rule_id` strings with `expect`

The catalog boundary already has validated local and fully-qualified rule
identity, but it discards that type at the compiler error boundary. Provider
validation is performed by synthesizing a placeholder rule ID, each local ID
is formatted and reparsed, and all four compiler-error branches carry the ID
as `String` before `RuleCatalog::new` parses it again. The invariant that the
compiler preserves a validated ID is therefore enforced by repeated string
round-trips and `expect("compiler preserves validated rule ID")` rather than
by the owning identity type.

This makes namespace normalization and error mapping harder to evolve
consistently. A future change to provider validation or rule-ID formatting
must update the sentinel construction, full-ID construction, and every error
arm while preserving an implicit invariant that is not represented in the
types.

**Recommendation:** Introduce a private validated provider namespace value,
or add a typed constructor on `RuleId` that combines a validated provider
part with a validated local name. Preserve `RuleId` in compiler/catalog error
variants instead of converting it to `String`, then delete the repeated parse
and `expect` arms. Keep namespaced uniqueness, deterministic declaration
order, provider-local IDs, and the no-recompilation guarantee of catalog
combination.

**Fix Applied:** `RuleId` now validates provider namespaces and constructs
fully-qualified IDs from provider/local parts. `CompiledCatalogError` retains
the validated `RuleId` through compilation, so `RuleCatalog::new` maps
compiler failures directly without string reparsing or `expect`; provider
validation no longer uses a placeholder ID. Namespaced uniqueness, stable
ordering, and no-recompilation catalog combination are preserved. Verified
with `make fmt && make ci`.

#### [x] READ-002 — `RuleCatalog` stores a derived index beside its source records

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Encapsulation
- **Location:** `glass-lint-core/src/lint/catalog.rs:41-47,85-127`
- **Representative callers:** both `RuleCatalog::new` and `RuleCatalog::combine` rebuild `rule_indices` from `records`; `rule_id` reads the vector while `rule_index` reads the map

The catalog’s `rule_indices` map is entirely derived from the ordered
`records` vector, but the two constructors each repeat the map-building
sequence. The catalog must preserve two synchronized views of one invariant:
every fully-qualified ID is unique and maps to the stable position in the
records vector. The fields are private, but the duplicated construction path
means future record transformations can update one path without preserving
the other.

The lookup map is justified for the public ID-to-index operation; the issue is
that its construction and consistency invariant have no single owner.

**Recommendation:** Add one private `from_compiled_records` transition or a
small catalog-records owner that builds and validates the vector/map pair.
Have both `new` and `combine` supply records to that transition, retaining
the duplicate-ID check where catalogs are combined. Preserve stable index
ordering, logarithmic ID lookup, no recompilation, and the existing
all-or-nothing error behavior.

**Fix Applied:** Already addressed by `878c42c fix read cross-005 chunk 13`,
which removed the derived `rule_indices` map and kept catalog identity in the
ordered compiled records. The finding predates that fix and is stale in this
audit chunk; no duplicate source change is needed.

#### [x] READ-003 — Deferred rule and lifecycle builders duplicate an error-policy state machine

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication / API
- **Location:** `glass-lint-core/src/api/rule/mod.rs:104-285`; `glass-lint-core/src/api/rule/query/lifecycle.rs:481-667`
- **Representative callers:** provider catalogs use `Rule::catalog_builder` and `LifecycleQuery::catalog_builder`; immediate callers use the corresponding `builder` plus `try_*` methods

There are two parallel builder families, each with an immediate and deferred
mode. `CatalogRuleBuilder` repeats the query/queries and metadata forwarding
surface around `RuleBuilder`, while `CatalogLifecycleQueryBuilder` repeats
source, condition, completion, and first-error retention around
`LifecycleQueryBuilder`. Each deferred wrapper owns a separate optional
error field and its own “record only the first error” protocol.

The modes are not fully consistent: lifecycle’s non-`try_` condition and
completion setters silently ignore duplicates, its `try_` versions report
them, and the catalog wrapper reports them; the rule builder records duplicate
metadata even in its fluent non-`try_` setters. This spreads construction
semantics across forwarding methods and makes a future authoring-stage rule
easy to apply to one builder but not its sibling.

**Recommendation:** Keep domain-specific builders and both authoring modes,
but centralize the mutation/error policy in one private construction-state
owner or deferred-error accumulator. Make immediate and deferred setters share
the same duplicate-stage behavior, differing only in whether the error is
returned now or retained for `build`. Preserve ergonomic catalog chains,
first-error determinism, immediate `try_*` propagation, bounded collections,
and the distinct lifecycle relationship checks.

**Fix Applied:** A private generic `FirstError` accumulator now owns
first-error retention for rule metadata, deferred rule queries, and deferred
lifecycle operations. The immediate and deferred builders keep their existing
first-error precedence and validation behavior while sharing the state-policy
implementation. Verified with `make fmt && make ci`.

#### [x] READ-004 — Raw compiler variable slots leak through the public rule API

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Newtype
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:50-69`; public re-export at `glass-lint-core/src/lib.rs:34-42`; internal slot construction at `glass-lint-core/src/api/rule/query/composition.rs:222-247,258-288`
- **Representative callers:** public event constructors always create the primary variable as `VarId::new(0)`, while member composition creates an object slot with `VarId::new(1)`; the compiler later alpha-renumbers slots

`VarId` is documented as a dense compiler variable ID, yet `new` and `get`
are public and the type is re-exported from the public `rules` module. The
authoring API does not expose public predicates or bindings that make
arbitrary caller-chosen IDs meaningful; provider authors instead use typed
constructors that select fixed internal slots. The compiler subsequently
alpha-renumbers these IDs independently of their authored values.

This exposes compiler storage vocabulary without exposing a complete semantic
operation that uses it. Callers can become coupled to slot numbers even though
those numbers are explicitly not part of the physical plan contract, and the
public constructor bypasses the “dense and validated” description.

**Recommendation:** Keep variable IDs private to the declaration/compiler
boundary, or expose only a semantic authoring binding type if user-defined
multi-event variables become part of the supported API. Remove the raw public
constructor/accessor from the provider-facing re-export while retaining
internal IDs, alpha-renumbering, diagnostics, and test-only construction.

**Fix Applied:** `VarId`, its raw constructor/accessor, and variable
inspection methods are now crate-private; provider-facing rule exports no
longer expose compiler slot vocabulary. Internal compiler/test construction
and alpha-renumbering remain intact, with the test-only variable collection
helper retained under `cfg(test)`. Verified with `make fmt && make ci`.

#### [x] READ-005 — `ModuleSpecifierPattern` promises exact matching but implements only package roots

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Documentation
- **Location:** `glass-lint-core/src/api/rule/module.rs:7-46`; exact/package split at `glass-lint-core/src/api/rule/query/constructors.rs:161-181`
- **Representative callers:** `ModuleSpecifierPattern::package` is the only constructor and `PatternValue` has only `Package`; `EventQuery::import_exact` instead stores a raw literal predicate while `import_package` uses the pattern type

The public type is documented as “an exact module specifier or a package
root,” and its internal enum suggests a pattern algebra, but the type can only
be constructed as a package-root pattern. Exact module matching is represented
by a different `IdentitySpec::LiteralString` path. As a result, the public
vocabulary and the implementation disagree, while exact and package matching
carry different validation and display semantics.

This is more than documentation wording: a future caller or catalog author
cannot tell whether `ModuleSpecifierPattern` is intended to be the common
module-pattern abstraction or only the package-root abstraction, and adding a
second variant later could change matching behavior at an API boundary.

**Recommendation:** Either narrow the type’s contract and name/document it
as a package-root pattern, or add an explicit validated exact constructor and
make exact/package variants deliberate. Keep exact identity matching distinct
from boundary-aware package-subpath matching, preserve package-root boundary
checks, and keep provider policy out of Core.

**Fix Applied:** Narrowed the public contract to the implemented package-root
pattern with boundary-aware subpath matching. Exact module imports remain
deliberate literal identities through `EventQuery::import_exact`; package and
exact matching are no longer presented as one incomplete abstraction.
Verified with `make fmt && make ci`.

#### [x] READ-006 — Argument-index validity is checked in several builders and reconstructed as a primitive

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype / Duplication
- **Location:** `glass-lint-core/src/api/rule/query/constructors.rs:9-14,270-347`; `glass-lint-core/src/api/rule/query/mod.rs:456-477`; `glass-lint-core/src/api/rule/query/lifecycle.rs:97-111`; storage at `glass-lint-core/src/api/rule/query/value.rs:5-17,260-334`
- **Representative callers:** `EventQuery`, `EventRequirement`, `LifecycleEventBuilder`, and `ArgumentConstraintsBuilder` each validate the public index limit; `ArgumentConstraint::arg_index` casts its stored `usize` back to `u8`

The bounded `ArgumentIndex` newtype owns the representation, but its
validation is repeated in `checked_argument_index`, `EventRequirement::argument`,
`LifecycleEventBuilder::arg`, and `ArgumentConstraintsBuilder::push`.
`ArgumentConstraint` then stores the index as `usize` and reconstructs an
`ArgumentIndex` with a truncating cast in `arg_index`; the builder performs a
separate checked conversion followed by an `expect`.

The current limit makes these operations safe, but the invariant is spread
across authoring paths and storage conversions. Changing the bound or adding
another constraint source requires checking several copies, and the
borrowed/public constraint API can no longer make the newtype’s guarantee
obvious.

**Recommendation:** Give `ArgumentIndex` one fallible conversion from
`usize`, store it directly in `ArgumentConstraint`, and have all event,
requirement, lifecycle, and constraint builders use that owner operation.
Delete the repeated casts and `expect` after the conversion is centralized.
Preserve the maximum index and per-group/per-predicate budgets, canonical
ordering, public `get`/`index` behavior, and the existing error variant.

**Fix Applied:** `ArgumentIndex::try_from_usize` is now the single checked
conversion from public positions. `ArgumentConstraint` stores the validated
newtype directly, and event, lifecycle, and grouped-constraint builders pass
that type through without repeated bound checks, primitive storage, or cast
back-construction. Verified with `make fmt && make ci`.

#### [x] READ-007 — `Any` composition and compilation duplicate evidence-projection validation

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Complexity
- **Location:** `glass-lint-core/src/api/rule/query/composition.rs:105-176`; compiler validation at `glass-lint-core/src/api/compiler/validate/pass4_10.rs:157-223`
- **Representative callers:** `QueryDecl::any` and `any_with_evidence` validate branch emissions while `pass_correlation_evidence` rewalks every `Any` branch before compilation

`QueryDecl::any_impl` manually checks that alternatives retain the first
branch’s primary variable, contain that variable, use the same evidence kind,
and—unless an aggregate symbol is supplied—use the same symbol. The compiler
then performs a second recursive evidence check over the resulting expression,
including primary-variable presence in every `Any` branch and binding checks
through nested `All` expressions.

Both layers need defensive validation at their boundary, but the semantic
compatibility rule is split between two implementations with different error
types and traversal details. The composition function also carries a mutable
first-emission accumulator and an optional symbol mode, so additions to
emission metadata can easily make authoring and compiler validation disagree.

**Recommendation:** Put branch-emission compatibility in one declaration-owned
helper or value object that both `any_impl` and the compiler validation pass
can call, with each boundary translating failures to its own error type. Keep
early `QueryBuildError` feedback, compiler-side defensive validation, explicit
aggregate symbols, branch-local variable scope, and the rule that incomplete
branches cannot emit unsupported evidence.

**Fix Applied:** `EmissionDecl::is_compatible_with` now owns branch emission
compatibility, while `AnyExpr::all_branches_contain` owns the shared primary
projection check used by both `QueryDecl::any_impl` and compiler validation.
The compiler retains its recursive defensive traversal and translates the
shared predicate into its own diagnostic. Verified with `make fmt && make ci`.

## Systemic Themes

- **ENCAPSULATE:** Provider namespaces, compiler variable slots, module
  patterns, and bounded argument indexes need one typed owner instead of raw
  strings, public slot constructors, or repeated conversions.
- **SIMPLIFY:** Authoring has parallel immediate/deferred builder protocols and
  a composition function that carries evidence state through several modes.
- **DEDUPLICATE:** Catalog index construction, builder error retention,
  argument validation, and evidence compatibility are repeated across
  adjacent stages.

## Open Questions

None recorded.

## Coverage

Reviewed the public rule facade, metadata and catalog builders, rule/query
errors, module patterns, rule IDs, event and lifecycle declaration APIs,
argument/value constraints, logical composition, evidence emission, compiler
catalog conversion, and provider catalog callers. The physical compiler plan
and classification storage are intentionally handed off to Chunk 9; they were
used only where necessary to verify the authoring/compiler boundary.

The stable authoring boundary exposes validated semantic constructors,
catalog metadata, and useful diagnostics. Compiler IR, artifact-local slots,
and storage-shaped indexes remain private; public inspection should use
semantic accessors or explanations rather than raw compiler structures. This
decision preserves an ergonomic rule-authoring API without turning compiler
storage into a compatibility contract.

## Handoff

Chunk 8 is complete. The next unreviewed chunk is **Chunk 9 — Query
classification and compilation** (`CODEBASE_STRUCTURE_CORE.md` lines
614-690), covering classification evidence, compiler validation and
normalization, physical planning, object-flow compilation, and compiled rule
selection.
