# Codebase Readability Audit

## Summary

Chunk 08 has a clear provider-neutral authoring boundary: rule metadata,
typed event/value/lifecycle declarations, bounded canonical inputs, and
catalog-bound selection are kept separate from compiler plans. The main
readability debt is concentrated in the fallible-builder surface. Immediate
and deferred builders duplicate the same mutable stages and fluent methods,
while lifecycle collection and conversion adapters repeat small policy
implementations that should have one owner.

## Findings

### [api/rule/mod.rs and api/rule/query/lifecycle.rs]

#### [ ] READ-019 — Unify immediate and deferred builder state

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/mod.rs:137-313`; `glass-lint-core/src/api/rule/query/lifecycle.rs:526-757`

`RuleBuilder` and `CatalogRuleBuilder` both own the same rule metadata and
query vector, with the catalog wrapper forwarding every fluent metadata/query
operation into an inner builder. The lifecycle pair repeats the same design:
`LifecycleQueryBuilder` and `CatalogLifecycleQueryBuilder` each own
`LifecycleStages` plus `FirstError<QueryBuildError>`, then expose overlapping
`source`, `condition`, `completion`, and `build` methods. The only meaningful
axis is whether a fallible operation returns immediately or records the first
error for `build()`. Maintaining two public types per declaration kind makes
new stages, metadata, or error ordering changes easy to apply to only one
surface, and forces catalog callers to use a less expressive duplicate API.

**Recommendation:** Give one private builder core ownership of stages,
metadata, and first-error recording, and parameterize only the error policy
(for example, a small internal immediate/deferred mode or shared operation
helpers). Keep the public immediate `try_*` methods and the catalog's
deferred fluent methods as thin policy adapters, or consolidate them if the
public API can absorb that break. Delete duplicated forwarding methods and
duplicate constructors while preserving first-error ordering, metadata
duplicate detection, query conversion errors, lifecycle duplicate-stage
errors, collection bounds, and the existing `builder`/`catalog_builder`
call-site behavior. Add contract tests covering the same invalid operation
through both policies and verifying that later valid operations cannot erase
the first error.

**Fix Applied:** None so far.

### [api/rule/query/lifecycle.rs]

#### [ ] READ-020 — Centralize canonical bounded lifecycle collections

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:163-213`; `glass-lint-core/src/api/rule/query/lifecycle.rs:279-307`

`LifecycleEvents::new` and `LifecycleSinks::new` independently implement the
same invariant pipeline: reject an empty vector, sort, deduplicate, enforce a
finite collection bound, and store a boxed slice. Their `iter` methods and
test-only `len` methods also repeat the same storage façade. The existing
`bounded_lifecycle_items` helper centralizes conversion and pre-conversion
bounds, but the post-conversion canonicalization and storage policy remain
duplicated. A future lifecycle collection would have to copy this policy and
could accidentally choose a different ordering, deduplication, or boundary
check.

**Recommendation:** Add a private generic canonical bounded collection helper
that owns empty-check, sort/dedup, limit enforcement, boxed-slice storage, and
iteration; retain `LifecycleEvents` and `LifecycleSinks` as semantically named
wrappers that provide their distinct error labels and limits. Delete the
repeated `new`/`iter`/`len` mechanics without merging event and sink domain
types. Guard the migration with tests for duplicate elimination, deterministic
ordering, empty inputs, and the exact event/sink limits, including the
pre-conversion bound in `bounded_lifecycle_items`.

**Fix Applied:** None so far.

### [api/rule/query/lifecycle.rs]

#### [ ] READ-021 — Generate the repeated sealed fallible-input adapters

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:127-148`; `glass-lint-core/src/api/rule/query/lifecycle.rs:357-387`; `glass-lint-core/src/api/rule/query/lifecycle.rs:486-510`; `glass-lint-core/src/api/rule/query/lifecycle.rs:680-698`

`IntoLifecycleEvent`, `IntoLifecycleSource`, and
`IntoLifecycleCondition` each repeat the same sealed conversion pattern for
an already-built value and `Result<value, QueryBuildError>`. The file already
has `define_lifecycle_adapter!` for completion and sink inputs, so the module
contains two implementation styles for the same fallible-input contract.
This is not only extra code: a new lifecycle input kind can be added with
different sealing, forwarding, or error behavior depending on which style a
maintainer copies.

**Recommendation:** Extend the existing private adapter-generation helper to
cover the two-variant adapters and keep explicit extra implementations only
where a builder type needs a special conversion (currently
`LifecycleEventBuilder`). Preserve the distinct public trait names and their
sealed boundary, the identity conversion, exact `QueryBuildError` propagation,
and all generic constructor signatures. Add compile-level tests for both
prebuilt and `Result` inputs on each lifecycle stage, plus the event-builder
conversion path.

**Fix Applied:** None so far.

## Systemic Themes

- Query constructors intentionally expose provider-neutral semantic choices;
  their repeated event/identity combinations are a readable DSL surface, not
  automatically a candidate for generic abstraction.
- Bounds and canonicalization are generally explicit and fail closed. Any
  collection helper must preserve deterministic ordering, deduplication, and
  phase-specific limits rather than hide them behind an untyped container.
- Immediate versus deferred construction is a legitimate policy distinction,
  but its storage and validation ownership should be shared so the two public
  entry points cannot drift.

## Review Resolutions

- Keep the two public builder policies, but share one private stages/metadata
  core and put only first-error versus immediate-return behavior in thin
  adapters. Do not expose a generic policy type or collapse the public APIs.
- Use a private generic canonical bounded collection helper for the two current
  lifecycle collections. Keep the event and sink wrappers responsible for
  their distinct empty-error labels and limits; introduce a richer domain
  collection only if additional behavior appears.

## Coverage

Reviewed Chunk 08: rule metadata and builder APIs; query declarations and
composition; event, value, argument, lifecycle, and bounded input semantics;
query diagnostics and explanations; rule taxonomy and IDs; catalog assembly;
rule selection, prepared selections, and linter-facing catalog integration.
Read the root/core architecture, testing/contributing guidance, the complete
readability-audit skill instructions, and existing audits 001–007. No source
or test files were changed.
