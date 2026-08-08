# Codebase Readability Audit

## Summary

Chunk 2 owns the scope frontend and its transition from bounded, mutable
collection state to immutable query artifacts. It also owns the syntax-level
constant domain and the bounded trace arena used to materialize evidence.
The phase split and explicit incomplete states are strong foundations, but
some query and collection APIs project away uncertainty or expose mutation
authority more broadly than their owners can safely support.

## Findings

### Scope query uncertainty boundary

#### [ ] READ-005 — Return binding witnesses with completeness state

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/frozen_assignments.rs:7-29, 71-90`; `glass-lint-core/src/analysis/scope/query/bindings.rs:31-74, 211-220`; `glass-lint-core/src/analysis/scope/query/rooted.rs:19-50`; `glass-lint-core/src/analysis/scope/query/provenance/chain.rs:95-178`

`AssignmentAt` and `AliasAssignment` retain whether a reaching assignment is
absent, known, joined, or incomplete, but `FrozenScopeGraph::binding_at` and
`binding_alternatives_at` expose only an optional witness or a `Vec` of
complete witnesses. In particular, `binding_alternatives_at` drops unknown
and exhausted alternatives, so an empty vector can mean an unbound global or
an existing lexical binding for which no complete witness survived. The
rooted and chain consumers then use emptiness as a control signal for global
fallback, while callers of the compatibility `binding_at` query receive the
first witness without its joined/uncertain status; the internal uncertainty
model is therefore not carried across the query boundary.

**Recommendation:** Introduce a scope-owned resolution result containing the
complete witnesses plus explicit status such as absent, complete, joined, and
incomplete, or expose an equivalent typed view over `AssignmentAt` and the
declaration/parameter fallback. Make rooted, chain, global, and certainty
queries consume that result rather than infer state from `Option`/`Vec`
emptiness. Preserve the existing rules: unknown or exhausted alternatives
cannot establish a witness, an independent complete witness remains usable,
and a lexical declaration or dynamic lookup must prevent global fallback.

**Fix Applied:** None so far.

### Static-value conversion boundary

#### [ ] READ-006 — Centralize partial constant/provenance conversion

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/scope/build/provenance.rs:115-137`; `glass-lint-core/src/analysis/scope/mod.rs:48-65`; related bounded domain `glass-lint-core/src/analysis/syntax/constant/types.rs`

The scope frontend manually converts `ConstValue` into the supported static
`BindingProvenance` variants in `ScopeCollector::const_provenance`, while
`scope::provenance_to_const_value` manually converts the same static variants
back into `ConstValue`. These are intentionally partial mappings—object
values, name-table interning, and unsupported provenance must not be treated
as ordinary constants—but the conversion policy and bound handling are still
duplicated in two semantic owners. A new static container variant or changed
bound can consequently be implemented in one direction while the other
direction silently becomes `Unknown` or loses the distinction between object
keys and object values.

**Recommendation:** Define one scope/syntax adapter for the supported static
binding-value subset, with an explicit fallible or `Unknown`-preserving
contract and a callback/context for name interning and name resolution. Keep
runtime/module/callable provenance outside that adapter, retain bounded
arrays and objects, and preserve the current rejection of dynamic or
unsupported nested values. Replace the two hand-maintained match families
with that adapter and delete the duplicate conversion logic after callers are
updated.

**Fix Applied:** None so far.

### Scope collection mutation authority

#### [ ] READ-007 — Narrow generic collector mutation entry points

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/build/collector.rs:55-113`; `glass-lint-core/src/analysis/scope/build/assignments.rs:80-117`; `glass-lint-core/src/analysis/scope/build/callbacks.rs:19-67`; freeze caller `glass-lint-core/src/analysis/scope/build/freeze.rs:12-57`

`ScopeCollector::insert`, `record_assignment`, and `parameter_aliases` are
declared `pub` even though their implementations live in private build
modules and their current callers are sibling scope-build code (plus the
collector’s own freeze path). The generic `insert` operation accepts an
arbitrary scope, name, and provenance and does not carry the assignment path,
active-scope, or reachability context that the more specialized collection
methods maintain; crate-wide access can therefore bypass the phase and
invariant checks encoded by the build owner. This is not an external library
API leak because `analysis` is private, but it is an unnecessarily broad
internal mutation surface that makes future collection passes harder to
constrain.

**Recommendation:** Restrict these operations to the build module boundary
(`pub(super)` or the narrowest `pub(in ...)` needed), and expose semantic
operations for declaration insertion, assignment recording, and final
parameter projection rather than a generic raw mutation path. Keep the
existing freeze ordering and deterministic parameter-alias conflict handling;
move test-only construction through a scoped helper if tests need access.
After callers are migrated, remove the crate-wide visibility so new analysis
modules cannot write collection state without choosing an owner-approved
operation.

**Fix Applied:** None so far.

## Systemic Themes

- The immutable `FrozenScopeGraph` is the right consumer boundary, but its
  query methods should preserve the uncertainty information already modeled
  by `AssignmentAt`, `AliasAssignment`, and `ProvenanceAlternatives`.
- Scope collection is phase-structured and bounded; narrow mutation authority
  will make those guarantees architectural rather than dependent on caller
  discipline.
- Constant evaluation and provenance are related domains, not identical
  domains. A shared partial adapter should centralize their supported overlap
  without collapsing provider-neutral provenance forms into syntax constants.

## Open Questions

- Which consumers need a complete witness versus certainty coverage should be
  confirmed before selecting the exact resolution-result shape; the result
  must support both without exposing `FrozenAssignmentIndex` storage.
- The static-value adapter should preserve the current asymmetry where scope
  provenance can represent object values but `const_provenance` accepts only
  the static object shapes it can prove from syntax; this audit does not
  recommend widening that semantic set.

## Coverage

Reviewed only Chunk 2, “Scope, syntax, and evidence frontend,” from
`CODEBASE_STRUCTURE_CORE.md`, including scope planning and collection,
assignment history and frozen queries, scope graph phase transitions,
constant evaluation/provenance conversion, syntax provenance types, and the
bounded `TraceArena`/trace handle boundary. Existing Chunk 1 audit history was
used to continue finding IDs at READ-005. No source, test, configuration,
dependency, or other documentation files were changed; this chunk audit file
is the only new artifact.
