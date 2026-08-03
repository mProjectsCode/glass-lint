# Codebase Readability Audit

## Summary

This report replaces the previous audit. It is a read-only review of the
current workspace, with emphasis on semantic newtypes, representation leaks,
phase ownership, duplicate transformations, and unnecessary public surface.
The prior report's eleven checked-off migrations were verified as historical
work; they are not counted as current findings.

All five residual findings are now completed:

- 2 high-priority phase/API boundaries;
- 2 medium-priority representation leaks;
- 1 low-priority visibility cleanup.

The migrations preserve existing behavior while narrowing phase boundaries,
centralizing representation ownership, and reducing unnecessary public
surface. Focused tests and clippy checks pass; the full gate remains the final
verification step.

## Findings

### High priority

#### [x] READ-001 — Project export resolution still crosses raw module and resolution maps

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation, Architecture, Newtype
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:20-40`; `glass-lint-core/src/analysis/project/exports.rs:16-26`; `glass-lint-core/src/analysis/project/linker/export.rs:293-320`; `glass-lint-core/src/analysis/project/model.rs:311-337`

`ProjectLinker` and `ProjectSemanticModel` correctly make their maps private,
but the export resolver still accepts `&BTreeMap<ModuleId, ProjectModule>` and
`&BTreeMap<QualifiedRequestId, LinkedModuleTarget>`. The resolver repeatedly
performs storage lookups, reconstructs qualified request keys, and reaches
through module records. The linker and post-link model each rebuild the same
`ExportResolver` from their map fields, while `ProjectSemanticModel` already
has semantic `module` and `resolution_for` operations that this path bypasses.

The privacy migration therefore hides the fields without moving ownership of
project lookup semantics. A future change to the module/resolution storage or
keying scheme must still update the linker, post-link wrapper, and resolver
independently.

**Recommendation:** Give one private project-linking owner a narrow lookup
facade: resolve a module, obtain a request target for a module/request, and
perform the qualified export lookup. Have the shared resolver borrow that
facade (or make the linker/model owner the resolver) instead of borrowing raw
maps. Keep `ExportTable`, `QualifiedExportId`, and the distinct transient and
frozen project owners; the goal is to centralize operations, not add another
map wrapper or merge different identity domains.

**Fix Applied:** Added a private `ProjectLookup` capability implemented by the
transient linker and frozen semantic model. Export resolution now asks that
capability for modules and request targets, so qualified request keys and map
lookups remain owned by the project phase. The shared resolver no longer
accepts raw module or resolution maps; its cache is transferred through the
linker's resolver operation, and the post-link wrapper was removed.

#### [x] READ-002 — Compiled tsconfig selection exposes its phase storage

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Encapsulation, API, Architecture
- **Location:** `glass-lint-project/src/tsconfig/selection.rs:294-309`; `glass-lint-project/src/discovery.rs:292-333`; `glass-lint-project/src/tsconfig/mod.rs:418-437`

`CompiledTsconfigSelection` is documented as the semantic source-selection
model, but all four fields are public, including `Option<Vec<String>>`, the
compiled pattern object, and the diagnostic vector. Discovery therefore owns
the representation-specific branch between explicit files and pattern mode,
and directly performs path normalization before calling the pattern set.
Tsconfig diagnostics likewise clone the internal path and iterate the raw
diagnostic storage.

This makes the effective-config phase boundary storage-shaped: callers must
know that `None` means pattern selection, that the explicit list is already
normalized, and that invalid patterns are represented by a fail-closed set.
Those are selection invariants and should not be reconstructed at each use.

**Recommendation:** Make the fields private and add semantic operations such
as `config_path()`, `pattern_diagnostics()`, `explicit_files()`,
`includes(relative_path)`, and/or a selection operation that accepts the
admission callback. Let `CompiledTsconfigSelection` own the explicit-versus-
pattern decision and path normalization while preserving fail-closed
behavior and deterministic diagnostics. Keep parsed tsconfig DTOs public only
where they are genuinely parser records; do not apply this migration to the
raw `ParsedTsconfig` representation.

**Fix Applied:** Made compiled selection storage private and moved selection
semantics behind `config_path()`, `explicit_files()`, `includes(&Path)`, and
`pattern_diagnostics()`. The selection now owns slash normalization and the
explicit-versus-pattern boundary; discovery and diagnostic assembly no longer
inspect the compiled matcher or its vectors. Tsconfig tests were migrated to
the semantic API and preserve fail-closed and rebasing coverage.

### Medium priority

#### [x] READ-003 — Scope collection hands parallel storage vectors across the freeze boundary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, Architecture, Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:46-54`; `glass-lint-core/src/analysis/scope/build/freeze.rs:12-53`; `glass-lint-core/src/analysis/scope/graph.rs:159-204`; `glass-lint-core/src/analysis/scope/build/program.rs:17-32`

`ScopeCollectionArtifacts` exposes five independent collection fields to the
visitor and freeze code. `freeze` then takes three vectors one by one,
constructs a graph, and calls `finish_collected_properties`, where the graph
reinterprets collector records into mutation facts. The collector event types
also expose their fields at crate scope, so visitor code constructs storage
records directly and the graph owns a second, field-by-field representation.

The phase is correct, but the handoff makes the contract a list of vectors
rather than one owned transition. It is easy for a new collector artifact to
be appended without being frozen, and the caller must know which fields are
safe to drain before graph construction.

**Recommendation:** Keep phase-specific records distinct where their identity
really changes, but make `ScopeCollectionArtifacts` own recording methods and
a consuming `finish_into`/`into_frozen_parts` transition. Make the collector
record fields private and expose only constructors or semantic accessors
needed by the owning transition. The graph should receive one typed artifact
bundle or consume the bundle itself; preserve source-order and final sorting
invariants. This is preferable to merging collector events and frozen facts
merely because their fields look similar.

**Fix Applied:** Encapsulated scope collection recording behind named methods
and added a consuming `finish_into` transition that produces one typed frozen
artifact bundle. Collector event fields are private, while the graph consumes
the property bundle in one operation and retains the existing source-order
and final-sort behavior. Scope collection tests pass through the new query
boundary without exposing parallel vectors to the freeze phase.

#### [x] READ-004 — Validated extension aliases leak a resolver-facing map

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Encapsulation, API, Newtype
- **Location:** `glass-lint-project/src/options.rs:387-393`; `glass-lint-project/src/resolver.rs:24-35`

`ValidatedProjectLoadOptions` returns
`&BTreeMap<String, Vec<String>>` from `extension_aliases()`. The Oxc resolver
then clones every key and value to rebuild its own map. The validated options
already own normalized extension policy and expose `extensions()` as a
domain-oriented iterator, so this one option retains a storage-oriented API
and makes the resolver depend on the map representation.

**Recommendation:** Replace the map getter with a narrow iterator over alias
entries, or an owner operation that produces the validated alias entries for
the resolver to copy. Keep the project options crate-independent of Oxc and
retain the existing validation/order guarantees; do not introduce a second
alias newtype unless another consumer needs the same invariant.

**Fix Applied:** Replaced the validated options map getter with a borrowed,
deterministically ordered iterator of alias entries. The Oxc resolver now
copies those entries directly into its own options without depending on the
validated map representation; extension validation and ordering behavior are
unchanged.

### Low priority

#### [x] READ-005 — Scope mutation fact records are public field bags without a public query boundary

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** API, Encapsulation, Naming
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:555-567`; `glass-lint-core/src/analysis/scope/graph.rs:421-436`

`PropertyAliasFact` and `RootedPropertyMutationFact` are publicly exported
types with public fields, but their only production construction and
consumption is inside the scope implementation. `FrozenScopeGraph` exposes
the corresponding queries only within `crate::analysis`, and the repository
has no external production caller that needs to construct or inspect these
records. The public field bags consequently advertise a wider semantic API
than the scope graph provides and invite callers to depend on span/storage
details.

**Recommendation:** Restrict these fact types to the analysis scope boundary
or make their fields private with the smallest accessors required by query
code. If they are intentionally part of a future public model API, add named
fact accessors and document that contract first; otherwise prefer visibility
reduction over another wrapper. Do not merge them with the collector event
types in READ-003: the normalized frozen facts and AST-facing collection
records represent different phases.

**Fix Applied:** Restricted both normalized mutation fact types to the
analysis boundary, made their storage private, and added only the span, scope,
target, and property accessors required by mutation indexing and provenance
queries. Construction now uses named constructors in the graph, so callers
cannot build or inspect these records through public field bags.

## Systemic Themes

1. Recent migrations successfully hid many immediate fields, but a private
   map still leaks when it is passed by reference to a second owner that
   repeats key construction and lookup semantics.
2. The best next simplifications are phase transitions: one owner should
   record, normalize, freeze, or project a domain result rather than expose a
   collection for the next phase to reinterpret.
3. Not every similar struct is a duplicate. Collector records versus frozen
   facts, local versus linked identities, and parsed DTOs versus compiled
   selection are separate domains; consolidation should follow invariant
   ownership, not field similarity.
4. Public report, loader-outcome, and parser DTO fields were excluded where
   they are deliberate serialization/presentation boundaries. Internal
   `pub(crate)` fields remain in scope when multiple phases depend on their
   physical representation.

## Prior Audit Status

The previous report's READ-001 through READ-011 were checked off. The current
scan verified the associated migrations, including path-store consolidation,
request encapsulation, flow-plan ownership, compiled-flow privacy, lifecycle
rollback ownership, parameter projection, artifact-table transitions, load
metrics consolidation, path transformations, and lowering/cache transitions.
Those completed items are not repeated as open findings here. The five
residual findings in this report have now also been migrated and checked off.

## Coverage

Reviewed the workspace Rust sources with emphasis on
`glass-lint-core`, `glass-lint-datastructures`, and `glass-lint-project`, plus
the root and owning-crate architecture documents, `TESTING.md`,
`CONTRIBUTING.md`, the prior audit, and production call sites for candidate
types. The scan included public and crate-internal fields, semantic newtypes,
map/set wrappers, phase records, flow/scope ownership, project linking, and
tsconfig/resolver boundaries.

Each residual finding was implemented as a focused migration, documented above,
and verified with owning-crate tests and targeted clippy checks. The complete
workspace gate is run after the final migration.
