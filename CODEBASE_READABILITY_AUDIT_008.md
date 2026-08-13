# Codebase Readability Audit — Chunk 08

## Summary

Chunk 08 owns rule authoring and catalog integration: validated rule and query
builders, lifecycle declarations, bounded value and event collections, and the
public catalog error boundary. The validation model is conservative and the
immediate/deferred APIs preserve useful authoring ergonomics. The findings
below target work performed before boundedness is enforced, repeated
canonicalization state, duplicated builder state machines, and a public error
boundary that discards structured diagnostics.

## Findings

### Bounded authoring inputs

#### [ ] READ-035 — Generic authoring iterators are fully materialized before limits are enforced

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/rule/query/value.rs:81-104`; `glass-lint-core/src/api/rule/query/composition.rs:147-167,196-221`; `glass-lint-core/src/api/rule/query/lifecycle.rs:156-174,248-265,493-583`; `glass-lint-core/src/api/rule/mod.rs:226-239`

Several public authoring paths accept arbitrary iterators but enforce their
documented bounds only after collecting the entire input. Static alternatives
parse every item into a `Vec` before checking `MAX_STATIC_ALTERNATIVES`;
`Any` and same-event `All` expressions append every branch before
`AnyExpr::new`/`AllExpr::new` can reject `MAX_EXPR_CHILDREN`; lifecycle source,
event, and sink collections likewise sort and deduplicate full vectors before
checking their limits; and a rule retains all query roots before checking
`MAX_QUERY_ROOTS_PER_RULE`. A caller can therefore cause unbounded source
iteration, parsing, allocation, cloning, and sorting work before receiving a
bounded-input error. This weakens the architecture’s bounded-work guarantee at
the public rule API even though the resulting validated objects are bounded.

**Recommendation:** Put bounded collection/admission operations on the owning
collection builders. Count and stop at the relevant limit plus one while
preserving first-error ordering, canonical sorting, and deduplication; where
deduplication affects the final count, use a bounded canonical admission
strategy rather than retaining an unbounded input vector. Apply the same
boundary to query roots and lifecycle sources. Preserve empty-collection
diagnostics and all existing limit values; only reject excessive input before
the rest of the iterator is consumed.

**Fix Applied:** None so far.

### Fluent constraint construction

#### [ ] READ-036 — Each chained lifecycle or event argument rebuilds all prior constraints

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:95-116`; `glass-lint-core/src/api/rule/query/constructors.rs:262-283`; `glass-lint-core/src/api/rule/query/value.rs:295-339`

`LifecycleEventBuilder::arg` and `EventQuery::with_arg_index` reconstruct an
`ArgumentConstraintsBuilder` from the complete existing constraint slice for
every fluent `.arg(...)` call. Reconstruction clones every prior matcher and
replays the count map, then `finish` sorts the complete constraint vector
again. A declaration with many chained arguments therefore repeatedly scans,
clones, and sorts its prefix, producing quadratic construction work and
temporary allocations even though the builder already owns the mutable event
or query state. The provider catalogs use these fluent chains extensively.

**Recommendation:** Keep the validated `ArgumentConstraintsBuilder` as the
mutable construction state, or add an owner-level append operation that
updates its counts and retains the canonical constraints without reconstructing
the prefix. Continue producing the same sorted constraints at the final
boundary and preserve argument-group, predicate-count, index, matcher, and
error-order validation; remove only the per-call replay and full-prefix sort.

**Fix Applied:** None so far.

### Immediate and deferred builder APIs

#### [ ] READ-037 — Immediate and deferred rule/lifecycle builders duplicate the same mutation state machine

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/mod.rs:131-255,258-310`; `glass-lint-core/src/api/rule/query/lifecycle.rs:493-645,667-705`

The immediate and catalog-facing builders expose two versions of the same
authoring state machine. `RuleBuilder` owns metadata, query storage,
duplicate-field tracking, and final validation; `CatalogRuleBuilder` wraps it
and repeats query iteration, metadata forwarding, and deferred error handling.
`LifecycleQueryBuilder` and `CatalogLifecycleQueryBuilder` repeat the same
pattern around `LifecycleStages`, including source/condition/completion
mutation and final build sequencing. The only material policy difference is
whether fallible operations return immediately or record the first error.
Changes to stage validation or fluent surface behavior must consequently be
kept synchronized across parallel forwarding layers.

**Recommendation:** Centralize the shared mutable state and validation in one
internal owner, with a small explicit error-policy boundary for immediate
versus deferred operations. Keep thin public entry points only where the two
error-timing modes are part of the supported authoring ergonomics. Preserve
first-error precedence, immediate `try_*` behavior, duplicate-stage and
duplicate-metadata diagnostics, and the existing provider-facing method names;
remove duplicated forwarding/state-machine logic rather than introducing a
second compatibility layer.

**Fix Applied:** None so far.

### Catalog diagnostic boundary

#### [ ] READ-038 — Public catalog construction flattens structured compiler diagnostics into strings

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/error.rs:128-160`; `glass-lint-core/src/lint/catalog.rs:14-76`

The compiler-facing `CompiledCatalogError` preserves typed
`QueryDiagnostic`, `CompilerInvariantDiagnostic`, and
`PhysicalPlanDiagnostic` values, including their structured codes and fields.
The public `ProviderCatalogError`/`RuleCompilationError` boundary then maps
each of those diagnostics through `to_string()` into a `String`. Callers of
the public `RuleCatalog::new` API therefore receive only a category and
rendered text, cannot inspect stable diagnostic data, and must parse or
snapshot display output if they need to classify the failure. Core exposes
two parallel representations of the same catalog failure: structured errors
inside the rule/compiler API and an immediately flattened provider catalog
error.

**Recommendation:** Choose one deliberate public diagnostic boundary. Either
retain a stable structured diagnostic payload in `RuleCompilationError`, or
make compiler diagnostics private and define a provider-facing diagnostic type
with stable codes and fields. Keep the provider-level category and rule ID,
preserve exact display wording where compatibility requires it, and avoid
forcing callers to recover semantics from formatted strings.

**Fix Applied:** None so far.

## Systemic Themes

- Boundedness needs to be enforced while inputs are admitted, not only after
  generic iterators have been consumed and canonicalized. The same ownership
  rule should cover values, expression branches, lifecycle stages, and rule
  roots.
- Fluent builders should own incremental validation state. Reconstructing a
  canonical builder from its already-canonical output obscures ownership and
  turns ordinary declaration chains into repeated whole-prefix work.
- Immediate and deferred authoring APIs are policy variants of the same
  state machine. Shared mutation and validation should have one owner, with
  error timing selected at the boundary.
- Catalog errors are an API boundary, not merely display strings. Structured
  compiler diagnostics should either remain available to callers or be
  intentionally replaced by a stable provider-facing diagnostic model.

## Open Questions

- For bounded collections whose final size changes after deduplication, decide
  whether the contract limits raw inputs, unique canonical values, or both;
  preserve the current error precedence while making that contract explicit.
- Confirm whether immediate and deferred builder constructors are both part
  of the intended external API, or whether one should become an internal
  implementation mode behind the public catalog authoring surface.
- Decide which diagnostic fields are stable enough for the public catalog
  API before changing `RuleCompilationError`; callers may currently depend on
  its broad category variants even though they cannot inspect compiler codes.

## Coverage

Reviewed the chunk-08 structure entries and their implementation/test support:

- `api/rule/{mod,error,module,taxonomy}.rs`
- `api/rule/query/{mod,composition,constructors,error,event,expression,lifecycle,limits,value}.rs`
- `api/compiler/{mod,catalog,rule}.rs` at the rule-compilation boundary
- `lint/catalog.rs`, the public catalog exports, and JavaScript/Obsidian rule
  builder call sites
- Existing numbered audit reports 001–007 were checked to avoid duplicating
  their historical findings.

No source, test, configuration, dependency, or other documentation files
were changed by this audit.
