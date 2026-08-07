# Codebase Readability Audit — Chunk 4

This audit covers Chunk 4 of `CODEBASE_STRUCTURE_CORE.md`: retained models and
resolution. It is an architectural review only; no source changes were made.

## Summary

The retained model has several sound foundations: value and object IDs are
opaque, static properties have one owner, resolver caches are position
sensitive, and unknown or exhausted values generally degrade to non-matching
results. The main readability risks are at the boundaries between those
owners. Resolution carries the same provenance shape in two structs and then
performs cache, recursion, exhaustion, and provenance finalization in one
function. The value arena exposes many construction routes, while module
exports use several partially independent mutation methods whose merge rules
are implicit. Finally, the phase-freeze bundle exists to keep names and values
together but is immediately returned as an order-dependent tuple.

## Findings

#### [ ] READ-001 — Resolution seeds duplicate the retained result shape

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Internal API
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:16-43,93-190`; `glass-lint-core/src/analysis/resolution/mod.rs:44-87`
- **Representative callers:** `Resolver::resolve_ident`, `Resolver::resolve_member`, and `Resolver::resolve_seed`

`ResolutionSeed` repeats almost every provenance field in `ResolvedValue`:
rooted and syntactic chains, call and member provenance, returned-member
paths, and bound arguments. Its `into_resolved` conversion then copies those
fields one by one while replacing the ID and selected call/member fields.
`ResolvedValue::local` separately repeats the default initialization policy.

The distinction between a pre-finalization seed and a cached result is valid:
the resolver must derive the final call provenance and may assign a canonical
global ID. The representation duplication is still a readability risk at the
identity boundary. Adding a provenance field requires changes in the seed,
the conversion, and the retained result, and the conversion makes it less
obvious which fields are authoritative after finalization. A missed copy can
silently lose a witness without violating the type system.

**Recommendation:** Introduce one private provenance-parts type or builder
for the shared fields. Let the seed carry only the pre-finalization ID and
parts, and let a named finalization operation supply the canonical ID, final
call provenance, and module-member override. Keep `ResolvedValue::local` (or
an equivalent default constructor) as the single owner of absent/local
defaults. Preserve the current cache identity, independent provenance fields,
and unknown/exhaustion behavior.

**Fix Applied:** None so far.

#### [ ] READ-002 — `resolve_seed` mixes the entire resolution finalization protocol

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity / State API
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:240-292`
- **Representative callers:** `resolve_ident` and `resolve_member` both delegate their position-sensitive work to `resolve_seed`

`resolve_seed` is the single transition from a scope-derived seed to a cached
result, but it performs several different jobs in sequence: cache lookup,
cycle detection, seed construction, value-arena exhaustion interpretation,
call-provenance recovery, global-value re-interning, module-member enrichment,
namespace interning, result construction, and recursion-guard cleanup. The
closure makes the central protocol harder to read because the function's
state machine is split between the caller-provided builder and the finalizer.
`cache_resolution` also owns the only removal of the active recursion key, so
the guard's lifecycle is implicit rather than represented by a scoped state
object.

This is a high-value readability seam because all identifier and member
identity passes through it. Future resolution cases must preserve ordering:
the call must be derived after the seed, global IDs must be canonicalized
before caching, and exhausted values must not retain a definite call witness.
Those constraints are currently encoded as one dense sequence of conditionals
and side effects.

**Recommendation:** Split the protocol into named private phases, such as a
cache/recursion entry guard, seed finalization, and cache commit. A small
scoped guard or typed resolution state can own active-key cleanup, while a
finalizer can make the exhaustion, global canonicalization, and module-member
rules explicit. Keep cycle results uncached if that is the current policy,
retain position-sensitive cache keys, and preserve fail-closed outcomes for
budget exhaustion and unsupported resolution.

**Fix Applied:** None so far.

#### [ ] READ-003 — Value construction policy leaks through a broad arena API

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API / Ownership
- **Location:** `glass-lint-core/src/analysis/model/value.rs:156-337`; `glass-lint-core/src/analysis/resolution/call.rs:100-126`; `glass-lint-core/src/analysis/resolution/constant.rs:58-90`; `glass-lint-core/src/analysis/resolution/expression.rs:376-427`
- **Representative callers:** call, constant, and expression resolution construct values directly through `ValueTable::intern_*`

`ValueTable` correctly owns deduplication, binding wrappers, terminal-cache
maintenance, object allocation, and value-arena exhaustion. It nevertheless
exposes a long family of typed construction methods—global, module export,
rooted member, callable, local, unknown, static primitives, arrays, and
objects—alongside the generic binding wrapper. Resolution code in three
modules calls these low-level methods directly, while other paths add
resolver-level helpers such as `intern_const_value`, `intern_call_value`, and
`intern_object_id`. The semantic construction boundary is therefore split
between the arena and its consumers.

This makes the most important invariants harder to audit. A new value kind or
binding behavior must be threaded through multiple call sites, and callers
must know whether to use a binding-aware method, a raw method, or a resolver
conversion. The risk is not merely API size: terminal identity and exhausted
arena handling are path-local matching inputs, so bypassing or partially
duplicating the construction policy can change whether a value is matchable.

**Recommendation:** Keep `ValueTable` as the owner of interning and terminal
identity, but reduce its semantic surface to one private intern operation plus
a small typed construction specification, or make all typed construction go
through a single resolver-owned adapter. Centralize binding wrapping,
terminal-cache updates, and exhaustion reporting there. Preserve artifact-local
IDs, deterministic deduplication, static-object name validation, and the rule
that exhausted or invalid values return `ValueId::UNKNOWN` without creating a
definite witness.

**Fix Applied:** None so far.

#### [ ] READ-004 — Module export metadata has implicit and inconsistent merge rules

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Domain API / State Invariants
- **Location:** `glass-lint-core/src/analysis/model/module.rs:60-109,197-280`
- **Representative callers:** `glass-lint-core/src/analysis/facts/interface/exports.rs:27-64,84-92`; `glass-lint-core/src/analysis/facts/interface/commonjs.rs:54-127`

`ExportEntry` stores three deliberately compositional pieces of metadata:
module resolution, an optional function identity, and an optional static
string. That composition is needed by normal exports—for example, a function
export records both a local resolution and a function ID, and a CommonJS
property can carry resolution, function, and static-string information.

The mutation protocol for those fields is nevertheless spread across three
public `ModuleInterface` methods with different conflict behavior. A
conflicting `add_export` marks the entry unknown and clears all auxiliary
metadata; a conflicting `add_function_export` clears only the function ID;
`add_static_string` overwrites the static value without checking an existing
conflict. The global `mark_unknown_exports` transition has yet another policy
and clears the whole export map. These rules are only indirectly documented by
call ordering in the interface builders, and the tests cover global unknown
exports but not per-name conflicts or mixed metadata.

The current behavior may be intentional, but its ownership is difficult to
review. A future caller can record the same semantic export in a different
order and obtain a different degree of precision, or accidentally preserve
metadata after a conflict that should have downgraded the entry. That is a
direct risk to fail-closed module resolution.

**Recommendation:** Give `ExportEntry` or a private `ModuleInterface` export
transition one operation that merges resolution, function, and static-value
observations atomically and returns an explicit result such as unchanged,
updated, or unknown. Keep compositional metadata for compatible observations,
clear all per-name metadata on contradiction, and retain the module-wide
unknown barrier. Add focused tests for compatible mixed metadata, conflicting
function IDs, conflicting resolutions, and order independence where the
semantic observation is equivalent.

**Fix Applied:** None so far.

#### [ ] READ-005 — The frozen table bundle is dismantled into an order-dependent tuple

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Phase API
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:137-160`; `glass-lint-core/src/analysis/facts/stream.rs:310-324`
- **Representative callers:** `Resolver::freeze_into` creates the bundle; `FactStream<Building>::freeze` immediately destructures it into `names` and `values`

`FrozenFactTables` exists to express that the resolver's name table and value
arena cross into the frozen fact stream as one artifact-local phase transition.
Its only consuming API is `into_parts(self) -> (NameTable, ValueTable)`, and
the first consumer immediately relies on tuple position to reconstruct
`FrozenStorage`. The test-only path constructs the same bundle separately.

The bundle's invariant is therefore visible in the type name but not retained
at the receiving boundary. A later table addition or reorder requires
coordinated edits to a positional tuple and every destructuring site. This is
small today because there is one production caller, but it weakens the
architecture's explicit guarantee that the retained ID spaces travel together
through the freeze transition.

**Recommendation:** Move the consuming transition onto the bundle or expose
named accessors/`FrozenStorage::from_tables` so the phase boundary preserves a
named table aggregate rather than a positional tuple. Keep the transition
consuming, keep names and values artifact-local, and retain the existing
single-freeze ownership semantics.

**Fix Applied:** None so far.

## Systemic Themes

- **ENCAPSULATE:** Value construction, module-export conflict handling, and
  phase-owned table transfer need narrower domain transitions.
- **SIMPLIFY:** Resolution finalization currently combines cache state,
  recursion state, provenance enrichment, and resource exhaustion in one
  protocol.
- **DEDUPLICATE:** Seed/result provenance fields and their conversion are
  maintained in parallel representations.

## Open Questions

None recorded.

## Coverage

Reviewed the retained fact/value/module/scope models, static properties, flow
evidence model, resolver cache and phase freeze, expression/call/constant
resolution, and module-interface fact builders. The flow evidence model was
examined for duplicate index and forwarding APIs, but its generic
`LifecycleEvidence<E>` plus typed `FlowState` facade appears to preserve a
useful local-versus-qualified event boundary; no separate finding was added.

The shared resolution-parts type remains private to
`analysis::resolution`, beside `ResolvedValue`, rather than moving into the
retained value model. The resolver owns pre-finalization provenance and
canonical-ID enrichment; `ValueTable` owns only interning and terminal
identity. READ-001 therefore recommends a resolver-local parts value, with no
new public or model-level conversion type.

## Handoff

Chunk 4 is complete. The next unreviewed chunk is **Chunk 5 — Report model and
catalogs** (`CODEBASE_STRUCTURE_CORE.md` lines 389-459), covering
`analysis/report`, `catalog`, and `api/classification`.
