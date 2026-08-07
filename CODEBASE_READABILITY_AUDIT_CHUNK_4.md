# Codebase Readability Audit — Chunk 4

## Summary

Chunk 4 owns the retained semantic domain models, module interfaces, local
value resolution, project graph/linking, qualified identities, and
matcher-facing project projection. The model types enforce important
artifact-local and fail-closed invariants, and the linker keeps local arenas
partitioned while resolving only qualified identities. The main readability
and API risks are an incomplete model migration, redundant retained module
data, duplicated lookup/cache plumbing, indistinguishable local/project
budget fields, and a linked model that also acts as mutable report assembly
state.

## Findings

### Retained module-model migration

#### [x] READ-015 — Remove the obsolete `analysis::module` re-export boundary

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/module.rs:1-8`; representative callers in `analysis/facts/interface/mod.rs:8-28`, `analysis/local.rs:20`, and `analysis/project/linker/mod.rs:16-24`

`analysis::module` explicitly says its types moved to
`analysis::model::module`, but the production code still imports the old path
throughout interface construction, lowering, linking, and resolution. This
leaves two internal module boundaries for the same retained types and makes a
future model change look as though it must preserve a compatibility surface
that is only an in-crate re-export.

**Recommendation:** Migrate the remaining callers to
`crate::analysis::model::module` and delete `analysis/module.rs` once the
search shows no consumers. Keep the public contracts and constants on the
retained model module, preserve the provider-neutral module-interface
boundary, and do not reintroduce parser or provider types into the model.

**Fix Applied:** Migrated all remaining production callers to
`analysis::model::module` and deleted the obsolete `analysis::module`
re-export. Verified with `make fmt && make ci`.

### Module request representation

#### [x] READ-016 — Delete redundant retained re-export bindings

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/model/module.rs:19-40`; construction in `analysis/facts/interface/exports.rs:115-143`

`ModuleRequestRole::ReExport` retains a `Vec<ReExportBinding>`, but no
production consumer reads the binding fields: callers only pattern-match the
role as `ReExport { .. }`. Each named re-export is already retained as a
`ModuleExport::ReExport { request, imported }` entry, which is the structure
used by the linker and export resolver to resolve the mapping. The retained
binding vector therefore duplicates export mapping information while adding a
second representation whose fields have no accessors or downstream owner.

**Recommendation:** Make the re-export request role carry only the fact that
the request is a re-export, remove `ReExportBinding`, and delete the
construction of the unused vector. Keep the request role for filtering
namespace/import/re-export behavior, and preserve every per-name exported
mapping in `ModuleExport`, star-export handling, deterministic request order,
and conservative behavior for unsupported or ambiguous exports.

**Fix Applied:** Reduced `ModuleRequestRole::ReExport` to a unit role and
removed the unused `ReExportBinding` representation and construction. Named
and namespace export mappings remain in `ModuleExport`, preserving linker
behavior and deterministic request ordering. Verified with `make fmt && make ci`.

### Flow budget model

#### [x] READ-017 — Give local and project flow budgets distinct owners

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:12-129`; consumers in `analysis/flow/projector/mod.rs:366` and `analysis/flow/cross/mod.rs:248-253`

`FlowLimits` stores both `operations` and `local_operations`, exposes both
`operation_limit()` and `local_operation_limit()`, and every production and
test constructor sets them to the same value. The names imply independently
configurable budgets, while the actual policy is one configured flow budget
used in two scopes; callers must know which accessor corresponds to which
phase and future changes can accidentally diverge or silently keep the wrong
field synchronized.

**Recommendation:** Represent the one configured flow-operation value with an
explicit budget type that exposes named local and project-wide factories. Make
the local projector and cross-project propagator consume those scoped budgets,
then delete the duplicated stored field, accessor, and constructor assignment.
Preserve the current minimums, overflow-safe scaling, the distinction between
per-module local charging and project-wide charging, and deterministic
exhaustion reporting; introduce separate validated limit types only if policy
diverges later.

**Fix Applied:** Already satisfied by READ-051 (`1b3eca0`), which removed
the duplicate local budget field/accessor and made the shared configured flow
operation limit the sole stored budget value.

### Position-sensitive resolution

#### [x] READ-018 — Consolidate cached resolution construction

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:16-58,64-164`; `glass-lint-core/src/analysis/resolution/mod.rs:216-273`

`ResolutionSeed` repeats every provenance-bearing field of `ResolvedValue`,
and `resolve_seed` manually rebuilds the second struct after applying
arena-derived callable/module provenance. In the same module,
`resolve_ident_id` and `resolve_member_id` independently construct a
`ResolutionKey`, check `resolved_values`, and fall back to the full resolver;
the full `resolve_ident` and `resolve_member` paths then repeat the key
construction before entering `resolve_seed`. The cache and conversion
invariants are sound, but their repeated plumbing makes it easy for a new
resolution field or key dimension to be updated in only one path.

**Recommendation:** Let one private `resolve_cached` operation own key lookup,
recursion admission, and cache cleanup, with a narrow ID projection for
identity-only callers. Give `ResolutionSeed` a conversion or builder method
owned by the resolver so `resolve_seed` does not reconstruct the parallel
field list manually; delete the repeated identifier/member cache branches
after migration. Preserve position-sensitive keys, cycle guards, fresh-object
identity, value/name budgets, arena-derived provenance, and fail-closed
unknown or exhausted results.

**Fix Applied:** Added resolver-owned identifier/member key and cached-ID
helpers, plus a `ResolutionSeed` conversion into the full resolved value.
Position-sensitive keys, recursion admission, arena-derived provenance, and
unknown/exhausted behavior remain unchanged.

### Project lookup boundary

#### [x] READ-019 — Share the project lookup adapter used by linking

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/project/linker/mod.rs:48-99`; `glass-lint-core/src/analysis/project/model.rs:250-264,363-372`

The `ProjectLookup` trait’s two operations are implemented three times: on
`ProjectLinker`, on the borrow-splitting `LinkerLookup` adapter, and on
`ProjectSemanticModel`. `ProjectLinker::with_export_resolver` and the final
model’s `resolve_imported_identity` then construct the same
`ExportResolver` boundary around those implementations. The borrow split is
legitimate, but the lookup contract and its qualified request keying are
duplicated rather than represented by one reusable view, so changes to module
admission or request lookup can drift between transient and final linking.

**Recommendation:** Introduce one private `ProjectLookupView` over the module
and resolution maps, use it for both the transient linker and final model, and
keep the resolver accepting that view rather than multiple nearly identical
implementations. Delete the duplicated `module`/`request_target` bodies and
retain only the narrow construction needed to satisfy borrow lifetimes.
Preserve the check that the importer module exists, qualified request IDs,
lookup-cache ownership, cycle/depth bounds, and unknown results for missing,
unsupported, outside-project, or conflicting resolutions.

**Fix Applied:** Added a shared borrowed `ProjectLookupView` and migrated
both transient linking and final-model export resolution to it. Qualified
request validation, lookup caching, bounds, and unknown-result behavior are
unchanged.

### Linked model lifecycle

#### [ ] READ-020 — Separate immutable linked semantics from report-session state

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/model.rs:225-248,457-537`; `analysis/project/projection.rs:541-567`; caller `glass-lint-core/src/lint/report.rs:46-75`

`ProjectSemanticModel` is documented as the linked project model, but it also
owns a mutable `TraceArena`, accepts parse failures after linking through
`record_parse_failure`, records flow/effect exhaustion after matching, and
stores projection-generated traces through `classify_with_evidence_limit`.
Report assembly consequently mutates the semantic model across linking,
matching, and diagnostics phases, while evidence rendering later reads that
report-specific arena from the same object. This obscures the phase boundary
that should make linked semantics stable and makes the model responsible for
both semantic lookup and report lifecycle accumulation.

**Recommendation:** Keep `ProjectSemanticModel` immutable after linking and
move parse-status additions, projection outcomes, trace storage, and report
operation metrics into a project-analysis/session result owned by report
assembly. Have classification return its matcher catalog, outcome, and trace
arena together instead of writing the arena back into the linked model; have
status aggregation consume explicit inputs rather than mutating the model.
Delete the mutable trace/status forwarding methods from the semantic model
after callers migrate. Preserve post-parse diagnostic coverage, trace identity
resolution, deterministic operation counts, and the rule that incomplete
linking or exhausted projection cannot become definite coverage.

**Fix Applied:** None so far.

## Systemic Themes

- The retained models correctly keep artifact-local IDs opaque and preserve
  unknown, ambiguous, and exhausted states, but migration and lifecycle seams
  still expose duplicate representations to neighboring modules.
- Project linking has a sound transient-to-final transition and bounded SCC
  resolution; shared lookup and identity types should own the common mechanics
  without merging transient mutable state into the final model.
- Budget and report changes must remain explicit and deterministic. Any
  refactor must preserve local/project charging scope, fixed-point bounds,
  trace limits, diagnostics, and possible-versus-definite certainty.

## Decisions

- `ReExportBinding` has no consumer outside its construction and ignored
  pattern fields in the core workspace. Remove the retained vector and keep
  only the request-level re-export role; no compatibility wrapper is needed.
- Local and project flow operations intentionally use the same configured
  `flow_operations` value today, but they have different charging scopes.
  Replace the duplicated fields with one validated configuration value and
  named local/project budget factories; add separate limit types only if the
  policy later diverges.
- Parse failures are report-session state, not linked semantic state. Keep
  the linked model immutable and pass parse-failure maps into an explicit
  report-status accumulator owned by assembly.

## Coverage

Reviewed all modules listed in Chunk 4 of `CODEBASE_STRUCTURE_CORE.md`:

- `analysis::model`, `analysis::model::fact`, `analysis::model::flow`,
  `analysis::model::module`, `analysis::model::scope`,
  `analysis::model::static_properties`, `analysis::model::value`,
  `analysis::module`, `analysis::module_request`, `analysis::project`,
  `analysis::project::identities`, `analysis::project::linker`,
  `analysis::project::linker::export`, `analysis::project::linker::graph`,
  `analysis::project::model`, `analysis::project::projection`,
  `analysis::project::resolver`, `analysis::project::state`,
  `analysis::resolution`, `analysis::resolution::call`,
  `analysis::resolution::constant`, and `analysis::resolution::expression`.

Representative callers in fact/interface construction, flow projection,
cross-file propagation, project session assembly, and report evidence
rendering were checked for ownership, budget scope, lookup lifecycle,
diagnostic mutation, and deterministic output behavior.
