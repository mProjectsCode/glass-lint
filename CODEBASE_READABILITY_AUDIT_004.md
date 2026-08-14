# Codebase Readability Audit

## Summary

Chunk 04 (`analysis::model` and `analysis::resolution`) has strong ownership
around bounded values, immutable fact tables, resolution caching, module
interfaces, and uncertainty-preserving scope data. The main opportunities are
at two identity boundaries: one public model type represents an import binding
with an invalid boolean/optional combination, and one object identity type is
allocated independently by value resolution and flow projection. Both issues
make invariants depend on caller discipline and leave future code without a
type-level indication of which identity domain it is handling.

## Findings

### [analysis/model/module.rs, analysis/facts/mod.rs, analysis/project]

#### [x] READ-012 — Encode imported-binding variants instead of a boolean/optional pair

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API/Newtype
- **Location:** `glass-lint-core/src/analysis/model/module.rs:29-32,175-190`; `glass-lint-core/src/analysis/facts/mod.rs:495-512`; `glass-lint-core/src/analysis/project/linker/export.rs:157-165`; `glass-lint-core/src/analysis/project/identities.rs:119-140`

`ImportedBinding` stores `imported: Option<SmolStr>` and `namespace: bool`, while
`ImportedBinding::new` accepts both fields independently. This exposes states
that do not describe a JavaScript import: `(Some(name), true)` is a namespace
binding whose name is ignored, while `(None, false)` is a non-namespace binding
that both consumers silently skip. The fact builder currently creates only
valid combinations, but the public constructor makes the invariant a caller
obligation. Both linker consumers must therefore first branch on
`is_namespace()` and then defensively unwrap `imported()`.

**Recommendation:** Replace the pair with an enum such as
`ImportedBinding::Named(SmolStr)` and `ImportedBinding::Namespace`, and expose
variant-specific constructors or matching accessors. Update the fact builder
and delete the independent `is_namespace`/`imported` state once all callers
match the enum. Preserve default and named import export lookup, namespace
identity handling, deterministic binding order, and a test that makes every
constructed variant exhaustive; do not permit a namespace value to carry a
discarded imported name.

**Fix Applied:** `ImportedBinding` is now an exhaustive `Named`/`Namespace`
enum with variant-specific constructors; fact construction and linker
consumers no longer coordinate an optional name with a boolean flag. Existing
import lookup and namespace identity behavior is covered by the module model
tests. Verified with `cargo test -p glass-lint-core analysis::model::module --lib`.

### [analysis/model/value.rs, analysis/resolution, analysis/flow/projector]

#### [x] READ-013 — Separate resolver object identities from flow-projector object identities

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype/Architecture
- **Location:** `glass-lint-core/src/analysis/model/value.rs:29-34,108-116,136-147,294-303`; `glass-lint-core/src/analysis/resolution/expression.rs:498-520`; `glass-lint-core/src/analysis/flow/projector/mod.rs:41-44,929-936`; `glass-lint-core/src/analysis/flow/projector/transfer.rs:58-76`

`ObjectId` is defined in the value model and is used both as the payload of
`Value::Object` and as the identity stored in `FlowState`. These are not one
allocator: `ValueTable::allocate_object_id` increments `ValueTable::next_object`
for fresh resolver values, while `ObjectFlowProjector::allocate_object_id`
increments its own `run.next_object_id` for matched flow sources. Both counters
start in the same numeric space, so a resolver object and a flow object can
share the same raw ID while denoting unrelated entities. The shared type and
constructor do not communicate that distinction, and future code can pass a
resolver object to flow-state APIs (or compare/report the two domains) without
the compiler requiring an explicit bridge.

**Recommendation:** Introduce owner-specific opaque types, for example
`ResolvedObjectId` for `Value::Object` and `FlowObjectId` for `FlowState` and
projector evidence, with allocation owned by `ValueTable` and
`ObjectFlowProjector` respectively. Delete the cross-domain `ObjectId` imports
and conversions; if a real semantic relationship is required later, add an
explicit mapping type owned by the phase that establishes it. Preserve one
shared flow object across all matching flows, resolver fresh-value caching,
checkpoint/rollback identity, deterministic evidence keys, and independent
per-phase exhaustion limits. Add compile-time-facing tests or constructors
that prevent IDs from the two domains being interchanged.

**Fix Applied:** Resolver values now use `ResolvedObjectId`, while flow
projection state, history, and evidence use the distinct `FlowObjectId`. Their
allocators and test constructors are separate, so the two identity domains
cannot be interchanged accidentally. Verified with the resolver and flow
projector test suites.

## Systemic Themes

- The chunk generally uses semantic wrappers and bounded owners well; the
  remaining identity weaknesses occur where compact storage-shaped fields or a
  shared primitive cross module boundaries.
- Invalid-state prevention should happen at construction boundaries. Defensive
  filtering at linker consumers preserves current behavior but distributes the
  import-binding invariant across multiple callers.
- Numeric identity reuse is safe only when the owner domain is explicit. The
  resolver and flow projector already have separate lifetimes, budgets, and
  rollback semantics, which supports separate newtypes.

## Review Resolutions

- Resolver fresh objects and flow-source objects are deliberately separate:
  they have independent allocators, lifetimes, rollback behavior, and limits.
  READ-013 should use owner-specific IDs; any future relationship must be an
  explicit phase-owned mapping.
- Keep `ImportedBinding` at the module-interface boundary used by linking, but
  replace its invalid boolean/optional pair with named variants. The linker
  still needs to distinguish named and namespace imports, so moving conversion
  entirely into fact construction would not remove that ownership requirement.

## Coverage

Reviewed Chunk 04: retained fact, flow, module, scope, static-property, and
value models; module-request recognition; resolution seeds, caches, recursion
guards, expression resolution, call resolution, constant conversion, and
freeze boundaries. Traced representative callers in fact construction,
linking, module identities, resolver fresh-object creation, flow-source
matching, and flow evidence. Read the root/core architecture,
testing/contributing guidance, the complete readability-audit skill
instructions, and existing audits 001–003. No source or test files were
changed.
