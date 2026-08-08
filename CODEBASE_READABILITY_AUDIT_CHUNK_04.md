# Codebase Readability Audit

## Summary

Chunk 4 owns the retained semantic models for facts, flow, modules, scopes,
static properties, and values, plus position-sensitive expression and module
request resolution. The model layer generally keeps storage private and
preserves artifact-local identity, but several boundaries still expose
parallel representations: module request kind and role are independently
constructible, resolution results have a two-phase identity transition, and
imported-local state is duplicated. Value and object capacity intentionally
share one fail-closed status; that is not treated as a separate public
diagnostic concern. The remaining seams make invalid combinations and
identity interpretation depend on caller conventions.

## Findings

### Module request model and classification

#### [x] READ-012 — Couple module request kind with its valid role

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

**Recommendation:** Introduce typed request constructors on the facts-side
interface builder that encode the legal retained role and resolution kind;
make the raw `ModuleInterface::add_request` path private to that owner. Keep
static import bindings, re-export targets, star exports, dynamic imports, and
CommonJS requires explicit in the domain operations rather than exposing a
new public sum type. Preserve request IDs, source spans, specifier indexing,
and deterministic request order while ensuring every retained request is
valid by construction.

**Fix Applied:** Replaced the generic module-request insertion path with typed
constructors for imports, re-exports, star exports, dynamic imports, and
requires, keeping kind/role pairs valid by construction. Verified with
`make fmt && make ci`.

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
boundary: syntax recognition should return a validated request descriptor, and
the interface builder should own conversion to the project resolution key and
retained role. Keep `WrappedRequire` as an intermediate recognition detail
because it controls alias handling, then remove caller-side switch-and-pair
construction once the lowering owner exists. Keep the project
`ResolutionRequestKind` as the public project-input vocabulary; it is the
correct boundary for externally supplied resolution keys, not duplicate
retained-module policy. Preserve static imports, dynamic imports, requires,
re-exports, interop aliases, shadowing, and static-specifier checks.

**Fix Applied:** None so far.

#### [ ] READ-015 — Name provisional and finalized value IDs distinctly

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:17-20, 42-67, 121-218`; `glass-lint-core/src/analysis/resolution/expression.rs:302-338`

`ResolutionSeed` stores the provisional arena identity produced by lexical
resolution, while `finalize_seed` may intentionally replace it with a
canonical global identity before constructing `ResolvedValue`. The current
two IDs are therefore not duplicate owners, but their identical `id` naming
makes the two-phase identity transition easy to misunderstand and easy to
wire incorrectly in a new resolution path.

**Recommendation:** Keep both identities because finalization can canonicalize
global values, but name them `provisional_id` and `final_id` (or use a small
private finalization value) so the conversion boundary states which one is
authoritative after finalization. Keep the call/member provenance merge in
one helper and retain the distinction between cached identity-only queries and
full provenance queries.

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

**Recommendation:** Keep the interface-wide local set as the current owner:
the linker asks `ModuleInterface::is_local`, while no current consumer reads
an imported binding's local spelling. Remove `local` from `ImportedBinding`
and make its constructor carry only imported-name and namespace semantics.
Update the builder and linker together, preserving namespace imports,
default/named imports, deterministic binding order, and local-export handling.

**Fix Applied:** None so far.

## Systemic Themes

- Retained domain models are mostly immutable to consumers, but construction
  still accepts parallel primitive/enum fields at important phase boundaries.
  Validated domain constructors should own module-request and resolution
  invariants.
- Bounded resources should expose separate reasons only when downstream
  behavior differs. Value identities and fresh object identities currently
  share one fail-closed consequence, so no second public diagnostic vocabulary
  is warranted.
- Resolution and module interfaces contain small ownership seams (the
  provisional/final value-ID transition, imported-local names, and request
  classification). Naming or removing only the redundant channels keeps the
  model simpler without hiding legitimate phase transitions.

## Decisions

- `ResolutionRequestKind` remains part of the project input API because callers
  construct validated resolution keys at that boundary. READ-013 should move
  only facts-side recognition/lowering into one owner; it should not collapse
  the project input type into the retained module model.
- The current linker needs the interface-wide local set and does not need
  imported binding local names. READ-016 should remove the unused field rather
  than preserve both representations for a possible future linker feature.
- Keep one public value-arena incomplete diagnostic. Fresh object IDs and
  interned values currently have the same fail-closed consequence, so a second
  diagnostic would add vocabulary without a distinct behavior. Internal state
  may remain typed if it simplifies correct ownership of the two bounds.

## Coverage

Reviewed only Chunk 4, “Retained models and resolution,” from
`CODEBASE_STRUCTURE_CORE.md`, including retained fact, flow, module, scope,
static-property, and value models; module-request recognition; frozen fact
tables; and expression/call/constant resolution. Existing Chunk 1 through
Chunk 3 audit history was used to continue IDs at READ-012. No source, test,
configuration, dependency, or other documentation files were changed; this
chunk audit file is the only new artifact.
