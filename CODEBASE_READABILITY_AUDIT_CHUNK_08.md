# Codebase Readability Audit

## Summary

Chunk 8 owns the provider-neutral rule-authoring surface: validated metadata,
module patterns, typed event/value/lifecycle declarations, bounded logical
expressions, adapter traits, and the public `rules` re-export boundary. The
separation between authored declarations and private compiler IR, canonical
bounded collections, sealed query inputs, and deferred catalog errors is
appropriate. Five current opportunities remain: one semantic argument index
has two public accessors, boolean variable checks allocate full shape facts,
lifecycle adapter traits expose an inconsistent extension boundary, the
deferred lifecycle builder carries unreachable inner error state, and lifecycle
property names bypass the API’s normal canonicalization.

## Findings

### Argument-index API

#### [ ] READ-067 — Keep `ArgumentConstraint` argument positions semantic

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/value.rs:273-300`; callers in `glass-lint-core/src/api/compiler/{contradiction.rs,normalize.rs,physical.rs,reference.rs}`

`ArgumentConstraint` exposes both `index() -> usize` and `arg_index() ->
ArgumentIndex` for the same stored value. Compiler callers split between the
lossy primitive accessor and the semantic newtype accessor, so the authoring
type’s bounded-index invariant is preserved only by convention in some paths.
The duplicate surface also makes it unclear whether callers should compare
argument positions as validated domain values or raw array offsets.

**Recommendation:** Keep `arg_index()` as the owner-facing accessor and delete
the duplicate primitive method, updating display/sorting/grouping code to call
`.arg_index().get()` only at a real raw-index boundary. Preserve the validated
`ArgumentIndex` bound, ordering/equality behavior, public constructor ergonomics
through `usize`-accepting query methods, and all compiler normalization and
reference behavior.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. Keep `ArgumentIndex` through
compiler code and unwrap only at genuine raw-index boundaries.

### Query-shape membership

#### [ ] READ-068 — Make variable-membership checks short-circuit without shape allocations

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Performance
- **Location:** `glass-lint-core/src/api/rule/query/expression.rs:99-146, 253-273`; callers in `glass-lint-core/src/api/rule/query/composition.rs:156-171` and `glass-lint-core/src/api/compiler/validate/pass4_10.rs:190-210`

`QueryExpr::contains_var` computes `shape_facts()` and allocates both a
variable vector and a binding vector, even though the caller needs only a
boolean. `AnyExpr::all_branches_contain` invokes that path for every branch
during authored `any` construction and again during compiler validation. The
existing `walk_vars_until` traversal already supports early termination, so
membership validation repeats a full allocation-heavy shape pass instead of
using the narrow operation it requires.

**Recommendation:** Implement `contains_var` with `walk_vars_until` and a
short-circuiting target predicate, then remove the now-unneeded
`QueryShapeFacts::contains` path if no other caller remains. Preserve variable
role traversal, branch-scope validation, deterministic diagnostics, expression
depth/child bounds, and the full `shape_facts` result used by compiler passes
that genuinely need all variables and bindings.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. Use the existing early-stop
visitor for the boolean query and retain full shape construction only where
the compiler needs its complete result.

### Lifecycle adapter boundary

#### [ ] READ-069 — Make lifecycle conversion traits consistently sealed and public

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:120-141, 324-350, 449-480, 605-619`; re-exports in `glass-lint-core/src/api/rule/mod.rs:40-50` and `glass-lint-core/src/lib.rs:70-86`

Lifecycle builder methods use five conversion traits, but their contracts are
inconsistent. `IntoLifecycleSource` is explicitly sealed, while event,
condition, completion, and sink adapters are not; at the public root only the
condition and source traits are re-exported, even though the other traits occur
in public generic method bounds. This leaves callers with a partially visible
extension surface and makes it unclear whether custom conversions are a
supported API or an accidental escape from validated built-in declarations.

**Recommendation:** Establish one public lifecycle-adapter policy at the
`rules` boundary: seal every adapter and re-export every trait appearing in a
public bound. The existing validated `LifecycleEvent`, `LifecycleCondition`,
`LifecycleCompletion`, `LifecycleSink`, `EventQuery`, and `Result` adapters must
remain ergonomic; preserve the provider-neutral construction limits and avoid
allowing external implementations to inject unvalidated lifecycle state. Do
not introduce a second concrete-input API merely to work around the visibility
inconsistency.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed with the policy chosen: adapters
are convenience conversions, not an extension point, so they should all be
sealed and all public bounds should be nameable from the public rules API.

### Deferred lifecycle builder state

#### [ ] READ-070 — Separate lifecycle stage storage from immediate-builder error policy

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:487-601, 621-663`

`CatalogLifecycleQueryBuilder` wraps `LifecycleQueryBuilder`, while both store a
`FirstError<QueryBuildError>` field. The catalog wrapper intercepts every
fallible condition/completion/source operation: failed `try_set_condition`
calls are recorded only by the outer builder, and successful completion is
prechecked before the inner builder is called. Consequently the inner
`invalid_operation` field is never populated on the catalog path, but
`inner.build()` still checks it. The wrapper therefore mixes shared lifecycle
stage state with an error policy that belongs to only one builder mode.

**Recommendation:** Give the lifecycle stages (sources, condition, completion,
and symbol) one shared owner, and let immediate and catalog builders own only
their respective error-retention policies; delete the unreachable inner error
field from the deferred path and redundant forwarding state. Preserve first
error ordering, duplicate-stage diagnostics, deferred catalog construction,
stage relationship validation, and the immediate builder’s call-site error
behavior.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed with a minimality constraint.
Use one shared lifecycle-stage state owner plus one error-retention field per
builder mode; do not replace the current duplication with another forwarding
wrapper.

### Lifecycle property-name validation

#### [ ] READ-071 — Canonicalize lifecycle property names at construction

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:65-75`; comparable canonicalization in `glass-lint-core/src/api/rule/query/mod.rs:309-315` and `glass-lint-core/src/api/rule/query/value.rs:247-261`

`LifecycleEvent::property_write` checks `property.trim().is_empty()` but stores
the untrimmed `SmolStr`. Other authored identifier-like inputs use
`checked_name`, and `ArgumentMatcher::object_property_value` trims before
storing. A caller passing `" type "` therefore receives a successful lifecycle
declaration whose property cannot equal the authored source property `type`,
despite the surrounding API treating whitespace as presentation noise.

**Recommendation:** Withdraw this recommendation. A lifecycle property write
uses a JavaScript property key, not an identifier-like binding name. Trimming or
passing it through identifier canonicalization would change valid literal keys
such as `" type "`, and could reject punctuation or numeric keys that the
matcher can represent. Retain the current non-empty check unless a separate
property-key grammar is specified and verified against the source matcher.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Withdrawn as unsound. The apparent
whitespace inconsistency is not a bug without an explicit canonicalization
contract; no implementation work should be derived from READ-071.

## Systemic Themes

- Public authoring types should expose one semantic representation and one
  extension policy. Duplicate raw/newtype accessors and partially sealed
  adapter traits make callers reconstruct invariants that the API already
  owns.
- Query validation should charge only for the information it needs. Full
  shape facts remain valuable for type analysis, but boolean membership should
  short-circuit without allocating an unrelated aggregate.
- Immediate and deferred construction are distinct error lifecycles, not two
  owners of the same error state. Shared lifecycle data should be centralized
  while error retention stays with the mode that defines it.
- Typed event/value/lifecycle constructors, bounded canonical collections,
  compiler-private expression internals, and the `rules` re-export boundary
  were reviewed and retained as necessary architecture. The declaration API
  was not collapsed into compiler types because the authored/compiled phase
  boundary is an explicit precision and stability contract.

## Open Questions

- None remain. READ-069 is resolved in favor of sealed adapters re-exported
  wherever they occur in public bounds. READ-071 is withdrawn because property
  keys are literal strings rather than identifier names.

## Coverage

Reviewed only Chunk 8, “Rule authoring and catalog integration,” from
`CODEBASE_STRUCTURE_CORE.md`: rule metadata/builders and deferred catalog
builders, rule IDs, module-specifier patterns, event/query composition,
argument/value matchers, bounded logical expressions, lifecycle source/
condition/completion/sink builders, conversion traits, taxonomy, root `rules`
re-exports, and unit/public integration tests. The root and core architecture
documents, testing/contribution guidance, current audit chain, public-surface
test, query-composition tests, and compiler call sites were inspected. Focused
tests passed: `cargo test -p glass-lint-core api::rule --lib` (103 passed),
`cargo test -p glass-lint-core --test integration public_surface` (3 passed),
and `cargo test -p glass-lint-core --test integration query::composition` (30
passed). No source, test, configuration, dependency, or other documentation
files were changed; this chunk audit file was updated only with review
dispositions. The next
chunk is Chunk 9, “Query classification and compilation,” which should continue
finding IDs at READ-072.
