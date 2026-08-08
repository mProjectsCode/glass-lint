# Codebase Readability Audit

## Summary

Chunk 4 owns the retained semantic models for facts, flow, modules, scopes,
static properties, and values, plus position-sensitive expression and module
request resolution. The model layer generally keeps storage private and
preserves artifact-local identity, but several boundaries still expose
parallel representations: module request kind and role are independently
constructible, value and object capacity share one status, and resolution
results carry duplicated identity/provenance assembly paths. Those seams make
invalid combinations and incomplete-state interpretation depend on caller
conventions.

## Findings

### Module request model and classification

#### [ ] READ-012 — Couple module request kind with its valid role

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/module.rs:19-42, 217-242`; `glass-lint-core/src/analysis/facts/visitor.rs:221-254`; `glass-lint-core/src/analysis/facts/interface/mod.rs:50-58, 106-130`

`ModuleInterface::add_request` accepts an independent `ResolutionRequestKind`
and `ModuleRequestRole`, so callers can construct combinations the retained
model does not validate, such as a `StaticImport` with `Require` or a
`DynamicImport` with `ReExport`. The facts builder has specialized helpers for
some cases, but static imports and re-exports still pass the generic pair, and
the public model API leaves the invariant unenforced. Linkers and resolvers
then pattern-match the two fields separately, so an invalid pair can change
graph behavior without being rejected at the construction boundary.

**Recommendation:** Introduce typed request constructors or a single domain
request-kind enum that encodes the legal role and resolution kind together;
make `ModuleInterface` accept that validated value instead of two unrelated
arguments. Keep static import bindings, re-export targets, star exports,
dynamic imports, and CommonJS requires explicit in the domain type, then
delete the generic `add_request` path or make it private to the constructors.
Preserve request IDs, source spans, specifier indexing, and deterministic
request order while ensuring every retained request is valid by construction.

**Fix Applied:** None so far.

#### [ ] READ-013 — Centralize the three module-request vocabularies

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/module_request.rs:21-85`; `glass-lint-core/src/project/types/input.rs:318-360`; `glass-lint-core/src/analysis/model/module.rs:19-26`; `glass-lint-core/src/analysis/facts/mod.rs:486-499`

The same recognized module-loading expression is classified independently as
`module_request::ModuleRequestKind` (`Require`, dynamic import, wrapped
require), lowered to `project::ResolutionRequestKind` (static import, dynamic
import, require), and retained as `ModuleRequestRole` (import, re-export,
star-export, dynamic import, require). The mapping is performed by callers:
`observe_module_call` recognizes a request and then dispatches to separate
`record_import_request`/`record_require_request` helpers, while other callers
construct the project kind and retained role directly. The separate axes are
legitimate at different phases, but their conversion policy is spread across
the facts visitor and interface builder, allowing the vocabularies to drift.

**Recommendation:** Give the module-request domain one explicit lowering
boundary: syntax recognition should return a validated request descriptor,
and the interface builder should own conversion to the project resolution key
and retained role. Keep `WrappedRequire` as an intermediate recognition detail
only when it is semantically needed, and remove caller-side switch-and-pair
construction once the lowering owner exists. Preserve the distinctions between
static imports, dynamic imports, requires, re-exports, and interop aliases,
including their shadowing and static-specifier checks.

**Fix Applied:** None so far.

### Value identity and bounded resources

#### [ ] READ-014 — Report value and object capacity exhaustion separately

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Error Handling
- **Location:** `glass-lint-core/src/analysis/model/value.rs:135-165, 178-218, 297-343`; `glass-lint-core/src/analysis/resolution/mod.rs:344-356`; `glass-lint-core/src/analysis/lowering/mod.rs:199-204`

`ValueTable` owns two independently bounded resources: the `MAX_VALUES`
interning table and the `MAX_OBJECTS` fresh-object ID counter. Both set the
single `exhausted` flag, and `Resolver::value_arena_exhausted` exposes that
combined state to lowering, which reports `ValueArenaExhausted`. Exhausting
fresh object identities therefore has the same externally visible reason as
exhausting interned values, and callers cannot determine which operations must
be treated as incomplete or whether already-interned values remain usable.

**Recommendation:** Replace the boolean with typed capacity state, such as
separate value and object exhaustion flags or an exhaustion reason set, and
expose narrowly named queries/outcomes through `ValueTable` and `Resolver`.
Map each reason to the appropriate incomplete-analysis status while retaining
fail-closed `ValueId::UNKNOWN` behavior and the existing bounds. Keep the
resource ownership in the value model rather than making lowering infer the
reason from a generic table flag.

**Fix Applied:** None so far.

#### [ ] READ-015 — Keep one authoritative value ID in resolution seeds

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:17-20, 42-67, 121-218`; `glass-lint-core/src/analysis/resolution/expression.rs:302-338`

`ResolutionSeed` stores both an `id` and `ResolutionProvenance`, but
`ResolutionSeed::into_resolved` destructures and discards the stored ID while
accepting a second `id` parameter. `resolve_ident` and `resolve_member` fill
the seed ID, then `finalize_seed` supplies another ID to `into_resolved`, so a
future resolution path can accidentally make the seed and final value
disagree without a type or assertion catching it. This is a small but direct
duplicate channel at the boundary between expression resolution and the
resolved-value model.

**Recommendation:** Choose one owner for the identity: either have
`into_resolved` consume `self.id` and let finalization update the seed before
conversion, or make `ResolutionSeed` provenance-only and pass the final
`ValueId` explicitly. Keep the call/member provenance merge in one helper and
retain the existing distinction between cached identity-only queries and full
provenance queries.

**Fix Applied:** None so far.

### Module interface ownership

#### [ ] READ-016 — Remove duplicated imported-local state

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/model/module.rs:28-33, 177-193`; `glass-lint-core/src/analysis/facts/interface/mod.rs:34-47, 98-103`; `glass-lint-core/src/analysis/facts/visitor.rs:221-252`; `glass-lint-core/src/analysis/project/linker/export.rs:147-184`

Each `ImportedBinding` stores its `local` name, while the interface builder
also records every imported local in `ModuleInterface::locals`. Downstream
linking uses the binding’s imported name and namespace bit but checks local
ownership through `ModuleInterface::is_local`; there is no accessor or
consumer for `ImportedBinding::local`. The same source fact is therefore
retained twice, with two possible owners and no invariant connecting them.

**Recommendation:** Choose one owner for imported-local identity. Prefer
keeping the local name on the binding when import resolution needs it, expose
the narrow query the linker actually requires, and derive any interface-wide
local check from the request bindings; alternatively remove `local` from
`ImportedBinding` and keep the interface set if binding-level lookup is not
needed. Update the builder and linker together, preserving namespace imports,
default/named imports, deterministic binding order, and the handling of local
exports.

**Fix Applied:** None so far.

## Systemic Themes

- Retained domain models are mostly immutable to consumers, but construction
  still accepts parallel primitive/enum fields at important phase boundaries.
  Validated domain constructors should own module-request and resolution
  invariants.
- Bounded resources need reason-preserving status APIs. A shared `exhausted`
  bit is not sufficient when value identities and fresh object identities have
  different downstream consequences.
- Resolution and module interfaces both contain small duplicated channels
  (`ResolutionSeed` IDs, imported-local names, and request classifications).
  Removing those parallel representations will make ownership and later
  migrations easier to verify.

## Open Questions

- Whether the project-level `ResolutionRequestKind` is intentionally part of
  the public project input API or should be derived only when building a
  `ResolutionRequestKey`; the answer determines where the canonical module
  request lowering boundary belongs.
- Whether linker behavior will eventually need imported local names directly;
  this should be decided before removing either the binding field or the
  interface-wide local set.
- Whether fresh semantic object exhaustion should have a distinct diagnostic
  from value-arena exhaustion in the public incomplete-analysis report.

## Coverage

Reviewed only Chunk 4, “Retained models and resolution,” from
`CODEBASE_STRUCTURE_CORE.md`, including retained fact, flow, module, scope,
static-property, and value models; module-request recognition; frozen fact
tables; and expression/call/constant resolution. Existing Chunk 1 through
Chunk 3 audit history was used to continue IDs at READ-012. No source, test,
configuration, dependency, or other documentation files were changed; this
chunk audit file is the only new artifact.
