# Codebase Readability Audit — Chunk 12

## Summary

Chunk 12 owns the retained lexical identities, path-local binding
provenances, bounded static values, and shared module-request recognition
policy used by scope collection and expression resolution. The model already
keeps artifact-local IDs opaque, preserves complete witnesses alongside
unknown alternatives, and centralizes syntax recognition across resolver,
scope, and fact phases. The main remaining readability risks are that scope
IDs still expose their vector representation, provenance joins accept their
resource policy from arbitrary callers, and module-request policy combines
several independent permissions in one mode enum.

The value-arena construction, constant projection, global-object identity, and
scope-collector lifecycle concerns were cross-checked against earlier Chunk 5
and Chunk 6 reports and are not repeated here.

## Findings

### Scope identity and retained provenance

#### [ ] READ-001 — Keep lexical scope identity independent of vector storage

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:13-29,175-212`; representative storage-dependent callers in `analysis/scope/build/plan.rs:33-99,136-146`, `scope/scope_index.rs:17-82`, `scope/binding_index.rs:99-201`, and `scope/frozen_assignments.rs:48-70`

`ScopeId` is described as a lexical identity, but its public `index()` method
and the `LexicalScopes` implementation expose that it is a direct index into a
`Vec`. The planner and collector keep `Vec<usize>` stacks and repeatedly
reconstruct `ScopeId` values; binding allocation and frozen assignment lookup
then turn those IDs back into vector positions. The scope index also builds
its ordering by enumerating the storage vector, so the semantic identity and
the collection layout are coupled across the build, index, and query phases.

This makes a storage change or an invalid internal ID a cross-module concern,
and it forces each caller to preserve the same position convention. The
fallback `ScopeId::new(0)` paths and direct indexed assignment tables are
especially hard to review because the type itself does not say whether an ID
is valid for a particular scope collection.

**Recommendation:** Make the index representation private and let
`LexicalScopes` own ID admission and lookup, with operations such as
`push_scope`, `get`, `get_mut`, and an internal ordered-ID iterator. Carry
`ScopeId` rather than raw `usize` in planner/collector stacks, and have freeze
and binding indexes request validated IDs from the collection instead of
calling `index()` or rebuilding them from enumeration. Preserve the stable
program-first allocation order, deterministic scope traversal, parent links,
the invalid-shape fallback behavior, and artifact-local identity.

**Fix Applied:** None so far.

#### [ ] READ-002 — Seal bounded provenance joins before creating assignments

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:372-482,484-561`; join caller in `analysis/scope/build/assignments.rs:211-249`

`ProvenanceAlternatives::add_bounded` accepts an arbitrary `limit` on every
merge, while `ScopeCollector` separately owns the configured
`alternative_limit`. `AliasAssignment::joined` then accepts any
`ProvenanceAlternatives`, even one created with `single`, so the constructor
name does not enforce that the retained assignment represents a control-flow
join. The representation intentionally allows witnesses together with
unknown or exhausted flags, but the bound, joined phase, and terminal
assignment state are assembled by convention across three APIs.

That leaks the most important precision invariant: a merge must retain every
independent complete witness that fits the configured bound, mark the result
incomplete when retention is exhausted, and never let a synthetic assignment
silently use a different bound or precise-state interpretation. A new caller
can pass a per-call limit or mismatched state without the type identifying the
error, and reviewers must reconstruct the intended lifecycle from the caller.

**Recommendation:** Move the bound into a scope-owned merge operation or a
validated `ProvenanceJoin` value created with the collector's configured
limit; make raw `add_bounded` private. Have that operation return a sealed
joined result that `AliasAssignment` can accept only through a join-specific
constructor, while keeping precise assignments on the single-provenance path.
Preserve deduplication, deterministic insertion order, complete witnesses
alongside unknown alternatives, exhaustion preventing definite coverage, and
the current behavior for zero or exhausted limits.

**Fix Applied:** None so far.

### Module request recognition

#### [ ] READ-003 — Represent module-request permissions as named capabilities

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/module_request.rs:22-78,86-157`; call sites in `analysis/resolution/call.rs:40-43`, `analysis/facts/mod.rs:359-377`, and `analysis/scope/build/provenance.rs:80-112`

`ModuleRequestPolicy` has four modes—`Interface`, `DirectRequire`, `Alias`,
and `AliasWithDynamicImport`—but those modes encode several independent
decisions inside `recognize_module_call`: whether dynamic import is accepted,
whether interop wrappers are followed, and whether `require` must have exactly
one argument. The recognizer repeats `matches!` checks against the mode at
multiple branches, while callers select opaque constructors such as
`interface()` and `alias_with_dynamic_import()` to obtain the desired
combination.

This makes the policy difficult to extend and obscures why the same syntax is
accepted in one phase but rejected in another. Adding a new request shape or
permission requires editing both the enum's combinations and the recognizer's
branch conditions, increasing the chance that scope provenance, interface
collection, and direct resolver lookup drift apart.

**Recommendation:** Replace the combinatorial mode enum with a small
capability value whose named operations express the independent contract—for
example, `allows_dynamic_import`, `allows_interop_wrapper`, and
`requires_single_require_argument`—or make those operations inherent methods
on the existing policy and centralize all matching there. Delete the repeated
policy pattern matches after migration. Preserve the current four behavior
combinations, shadowing checks supplied by `ModuleRequestContext`, static
specifier requirements, spread rejection, wrapped-request classification, and
fail-closed handling of unsupported or dynamic module names.

**Fix Applied:** None so far.

## Systemic Themes

- Artifact-local IDs are correctly opaque at the outer API, but internal
  storage-shaped accessors (`ScopeId::index`) still make vector layout part of
  the scope lifecycle contract.
- Bounded uncertainty is a semantic state, not merely a collection limit.
  Resource admission and the transition to a joined or incomplete assignment
  should have one owner so complete witnesses cannot be lost or reinterpreted.
- Module recognition is shared across phases; policy should expose semantic
  capabilities while keeping provider-neutral syntax, shadowing, and static
  value checks in one recognizer.
- The prior value-layer findings remain applicable: raw value construction and
  constant/identity conversion must stay behind their owning resolver and
  arena boundaries. This chunk does not duplicate those findings.

## Open Questions

- Should scope IDs remain stable vector positions for deterministic debugging,
  or should the scope collection own a separate validated handle while keeping
  allocation order only as an internal iteration detail?
- Should provenance overflow be represented as a typed incomplete join result
  or remain a sticky state on the alternatives collection? Either choice must
  preserve independent complete witnesses and make the configured bound
  impossible to bypass.
- Are the four current module-request policy combinations the complete intended
  matrix? If more combinations are expected, named capabilities will make the
  contract clearer; if not, policy constructors should document each phase's
  exact acceptance rule.

## Coverage

Reviewed all types listed in Chunk 12 of `CODEBASE_STRUCTURE_CORE.md`:

- Scope model: `AliasAssignment`, `BindingId`, `BindingKey`,
  `BindingProvenance`, `BindingRoot`, `BindingSlot`, `BindingVersion`,
  `BoundArgument`, `FunctionId`, `IdentValueSeed`, `LexicalScope`,
  `LexicalScopes`, `MemberValueSeed`, `PropertyAliasFact`,
  `ProvenanceAlternatives`, `RootedPropertyMutationFact`, `ScopeBindings`,
  `ScopeEffect`, `ScopeId`, `ScopeKind`, and `ScopedName`.
- Static/value model: `StaticProperties`, `CallableValue`, `ObjectId`,
  `StaticObject`, `Value`, `ValueId`, and `ValueTable`.
- Module-request model: `ModuleRequestKind`, `ModuleRequestPolicy`,
  `RecognizedModuleRequest`, and `ModuleRequestContext`.

Representative callers in scope planning, scope indexing, assignment joins,
frozen provenance queries, expression resolution, value matching, fact
interface collection, and module-request recognition were traced. Chunk 5's
collector/traversal and expression-normalization findings and Chunk 6's
value-arena, constant-conversion, global-object, trace, and compatibility
façade findings were cross-checked and intentionally not duplicated. No
source, test, configuration, dependency, or existing audit files were
changed.
