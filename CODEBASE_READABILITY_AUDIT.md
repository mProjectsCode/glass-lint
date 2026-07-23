# Glass Lint Core and Project Readability Audit

Audit date: 2026-07-23

Scope: all Rust source files in `glass-lint-core` and `glass-lint-project`, including inline and dedicated test modules.

## Summary

The codebase is well-factored with strong phase boundaries, typed newtypes for most semantic identities, and consistent fail-closed patterns. This audit identifies 17 open maintainability issues: 2 high severity, 10 medium severity, and 5 low severity.

The dominant themes are a `ScopeGraph` struct that has accumulated too many responsibilities, two parallel provenance models whose relationship is undocumented, a handful of ad-hoc state encodings that could be type-safe state machines, and some error-handling gaps in the project crate where partial failures can mask or replace more relevant diagnostics.

## Findings

### Group 1: Core — Architecture and Ownership

#### READ-001 — ScopeGraph is a god struct with 20 fields across 5 concerns

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/model.rs:61-95`

`ScopeGraph` holds: name management (`names`, `NameTable`), environment queries (`environment`, `Environment`), scope index (`scopes`, `scopes_by_start`, `last_scope_query`), assignment indexing (`assignments`, `FrozenAssignmentIndex`), binding identity maps (`binding_ids`, `function_ids`, `function_bindings`, `function_aliases`), property facts (`property_assignments`, `rooted_property_mutations`), parameter aliases, dynamic evals, mutable static objects, and a validity flag. It directly implements 30+ query methods covering scope resolution, name interning, global lookup, assignment history, binding provenance, and function identity — concerns that span three distinct phases (collection, freezing, and resolution).

Split `ScopeGraph` into cohesive owned pieces behind a coordinator or trait. The frozen query surface (`names`, `assignments`, `binding_ids`, `scopes`) can be one struct; the mutable property/fact collectors can be separate owned types consumed during freeze. The `ScopeGraph::from_parts` constructor and the separated `ScopeGraphParts` struct already point in this direction—the remaining step is to push the resolution queries onto the owning phase struct that already holds the `Resolver`.

#### READ-002 — Two provenance enums with overlapping semantics and undocumented relationship

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/model.rs:481-510`, `glass-lint-core/src/analysis/syntax/provenance.rs:37-46`

`BindingProvenance` (11 variants) in `scope/model.rs` describes how a lexical binding acquired its identity during scope collection. `SymbolCallProvenance` (5 variants) in `syntax/provenance.rs` describes callable provenance at a use position in the fact stream. Both have `ModuleExport` and `Global`-like variants; their semantics overlap but one is not a subset of the other. The module comment on `IdentValueSeed` says the resolver "does not need to reinterpret `BindingProvenance`" but the duality remains undocumented. A new contributor must read both enums to understand why two exist.

Add a doc comment on each variant describing when it is produced and which consumer interprets it. If `BindingProvenance` is only consumed during the resolution phase that builds `ValueId`s and `SymbolCallProvenance`s, mark it explicitly as a build-time intermediate. Consider collapsing `SymbolCallProvenance::ModuleExport` into a shared provenance type once the scope-collection and fact-stream representations converge, but do not merge them until the value-arena resolution boundary is stable.

#### READ-003 — FactStream uses an ad-hoc validity flag and optional arenas instead of a typed state machine

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs`

`FactStream` accumulates `names` and `values` as `Option<NameTable>` / `Option<ValueTable>`, freezes them via `freeze_names` / `freeze_values`, and tracks overall validity with a `valid: bool` flag plus an `issues: BTreeSet`. The `freeze_*` methods return `Result<(), T>` where the error variant is the owned table (so the caller can recover on re-freeze). Accessors like `names()` and `values()` return `Option<&T>`, forcing every downstream consumer to handle `None` even though the stream is always frozen by the time it reaches them.

Introduce a generic `FactStream<Phase>` with marker types `Building` and `Frozen`. Construction starts in `Building` where `push` and `intern` are available; `freeze()` consumes `FactStream<Building>` and returns `FactStream<Frozen>`. The frozen accessors return `&T` unconditionally. This eliminates all `Option` unwrapping from the matching, flow, and linking pipelines and makes the freeze-ordering invariant compiler-checked. Do not attempt this until the `Lowerer` and `FactBuilder` APIs are stable; the refactor is mechanical across ~20 call sites in the fact pipeline alone.

#### READ-004 — ParentPathStore::append_linked encodes overlay identity in a tag bit

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/value/path.rs:109`

`append_linked` writes `id | (1 << 31)` to the edge cache and returns the tagged value. Callers that later pass tagged IDs to `depth`, `parent`, `segment`, or `starts_with` must have the tag stripped (by masking `& 0x7FFFFFFF` or similar), but none of those methods document the tag assumption. The `PathNode::parent` field is typed `u32` and may itself hold a tagged ID. This bit-level encoding is invisible to callers reading at the `PathInterner` or `PathId` API level.

Reserve the top bit explicitly in `PathId` with a `LINK_TAG: u32 = 1 << 31` constant. Add `PathId::is_linked(self) -> bool` and `PathId::untag(self) -> Self` methods. Document on `PathId` that tagged IDs are valid only within the summary overlay that produced them. In `ParentPathStore`, add a debug assertion in the affected accessors that untags the ID before indexing so misuse is caught in tests.

#### READ-005 — FlowLimits scaling uses a bare magic denominator

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/index.rs:39-44`

The `DEFAULT_STATES: u64 = 262_144` value appears three more times as a bare `262_144` literal in `from_flow_operations` — once per dimension. Its role as the denominator for proportional scaling is not obvious to a reader skimming the formula.

Name the denominator `DEFAULT_FLOW_OPERATIONS` (matching the configuration field it derives from) and reference it by name in every dimension's formula.

### Group 2: Core — Complexity and Naming

#### READ-006 — SpanNormalizer retains the full source text for boundary validation

- **Severity:** Low
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/lowering.rs`

`SpanNormalizer` stores an `Option<Arc<str>>` copy of every artifact's source text, used primarily for `is_char_boundary` validation when normalizing byte ranges. For ASCII source (the `is_ascii: bool` fast path), the text is retained but never read. Each artifact therefore carries a full-source `Arc<str>` through the entire analysis pipeline even though neither matching nor flow reads it.

Replace the retained `Arc<str>` with a precomputed `BitSet` of character boundary positions (a dense `Vec<bool>` segment per 4 KiB block, or similar compact structure) that the normalizer can query without retaining the full text. Only compute this when `is_ascii` is false. This reduces peak memory by the sum of all artifact source sizes through the pipeline.

#### READ-007 — intern_name reads but does not intern

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Naming
- **Location:** `glass-lint-core/src/analysis/scope/model.rs:151-153`

`ScopeGraph::intern_name` delegates to `self.names.lookup(name)`, a read-only operation. The name `intern` conventionally implies insertion; a reader expects mutations. Only `intern_name_mut` (line 155) actually inserts via `self.names.intern(name)`. Every call site of `intern_name` today correctly wants the read-only lookup, so renaming is safe.

Rename `intern_name` to `name_id` (which already exists at line 147 and duplicates the same logic) and remove the duplicate. The actual mutating method can remain `intern_name_mut` or be simplified to `intern`.

#### READ-008 — Test-only methods on production types create a large dead-code surface

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Testing
- **Location:** `glass-lint-core/src/analysis/value/path.rs:166-218`, `glass-lint-core/src/analysis/flow/table.rs:99-102`, `glass-lint-core/src/analysis/facts/stream.rs`, `glass-lint-core/src/analysis/scope/model.rs:181-184`

`ParentPathStore::find_linked_edge` (line 178) forwards to `find_edge` without adding logic — dead code even for tests. Seven more methods on `ParentPathStore` and `PathInterner` are `#[cfg(test)]` only. `FunctionTable::len`, `FactStream::new`/`push`/`len`/`facts_at`/`fingerprint`, `ScopeGraph::name_snapshot`, `Resolver::collect`/`collect_with_environment`/`collect_with_name_limit`/`name_snapshot`/`value_snapshot`, and `Factory::into_stream` are all test-only. While each serves test setup, the aggregate signals that production APIs lack ergonomic construction paths for simple scenarios.

Extract test helpers into a `test_util` module with `pub(crate)` visibility so they are reusable across test modules without being `#[cfg(test)]` on the production type. For one-off test convenience methods on production types, prefer a `#[cfg(test)]` extension trait in the test module rather than a method on the production struct.

### Group 3: Core — Pipeline Gaps

#### READ-009 — ProjectSemanticModel stores a link BudgetTracker that is never checked

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/model.rs:211-212`

`ProjectSemanticModel` stores `link_budget: BudgetTracker` and `link_limit: usize`. The limit is exposed via `link_limit()` and used for `operation_counts()` but the `BudgetTracker` is never consumed — no per-operation budget check exists in the linking or matching paths. If a future change relies on this field for enforcement, silent overflow is possible because the field appears active but is never tested.

Remove `link_budget` and store only the limit as a `usize` for metrics. If per-operation budget enforcement is desired later, add a `LinkBudget` parameter to the specific operations that need it rather than carrying unreferenced state through the model.

#### READ-010 — AnalysisStatus uses BTreeSet with String-keyed Ord, making diagnostic grouping fragile

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/status.rs:33-64`, `glass-lint-core/src/analysis/status.rs:78-80`

`IncompleteReason::ScopeShapeMismatch { detail: String }` participates in `PartialOrd`/`Ord` via the derived impl. Two mismatches with different `detail` strings are distinct entries in the `BTreeSet<StatusEntry>`. This means the same structural problem with different detail strings produces separate diagnostics, which may be correct for debugging but produces non-obvious grouping in reports.

Replace the `String` detail with a small enum of known mismatch variants (`PlannedScopeNotConsumed`, `UnconsumedAssignment`, etc.). If arbitrary detail is genuinely needed for debugging, store it in a field excluded from `PartialOrd` via a manual impl that compares only the variant key.

### Group 4: Project — Architecture and Encapsulation

#### READ-011 — Phase enum mapped to array index by manual discriminant values

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:76-83`, `glass-lint-project/src/loader.rs:70-73`

`Phase` enum variants have explicit discriminants (`Discovery = 0`, `Reads = 1`, ..., `Matching = 5`) and the `ProjectPhaseTimings` struct stores a fixed `[Duration; 6]` array. Every accessor and mutator indexes via `Phase::Discovery as usize`. Reordering or inserting a phase silently breaks all indexing — no compile-time error, just wrong duration returned.

Replace the array with a struct of named `Duration` fields (`discovery`, `reads`, `analyze_source`, `resolution`, `linking`, `matching`). Remove the `Phase` enum entirely. Use `impl Index<Phase>` and `impl IndexMut<Phase>` if generic iteration is needed, or simply use the named fields directly since every accessor today maps one-to-one with a phase name.

#### READ-012 — Too-many-files budget checked in four separate locations

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-project/src/discovery.rs` (`validate_membership`), `glass-lint-project/src/walk.rs:97-100`, `glass-lint-project/src/loader.rs:475-478`, `glass-lint-project/src/corpus.rs:118-119`

The file-count budget is enforced at admission time, during directory walking, during work-queue expansion, and during corpus discovery. Each check uses the same `options.max_files()` value but the counter is maintained independently (admitted set size vs walk accumulator vs discovered-file count), so they can disagree. Defense-in-depth is good, but four independent implementations of the same arithmetic are a maintenance liability.

Consolidate into a `FileBudget` wrapper around a `usize` limit + `usize` counter that exposes `try_admit() -> Result<(), TooManyFiles>`. Use it in the single `AdmissionSet::admit` path. The walk-level check is redundant once the admission set enforces the limit; the discovery-level check can delegate to the same budget.

#### READ-013 — SourceCorpus::load creates a new admission root per file

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/corpus.rs:128-141`

`SourceCorpus::load` sets `root = path.parent().unwrap_or(".")` and creates a fresh `SourceAdmission` for each call. When the corpus contains files from different directories, the admission root changes each time. Two files whose shared parent is under different canonical roots produce different `is_inside_root` outcomes. The `SourceCorpus` already owns a `ValidatedProjectLoadOptions` with a configured root; the method should use that root consistently.

Require a root path at `SourceCorpus` construction time or pass it as a parameter to `load`. Reuse one `SourceAdmission` for all `load()` calls so admission results are consistent and canonicalization is not repeated per file.

#### READ-014 — publish() in finish_inner can return Timeout when called from finish_partial

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Other
- **Location:** `glass-lint-project/src/loader.rs:565-607`

`finish_inner` (line 577) checks `Instant::now() > deadline` (line 585) and returns `Err(ProjectLoadError::Timeout)`. This function is called both from `finish` (which already checked timeout) and from `finish_partial` (which intentionally bypasses the pre-check). When a recoverable error preceded a timeout, `finish_partial` raises `Timeout` instead of the original error — masking the more relevant diagnostic.

Remove the deadline check from `finish_inner`. Let `finish` call it after its own pre-check. `finish_partial` doesn't need the check at all because partial output is expected.

### Group 5: Project — Error Handling

#### READ-015 — build_effective_config_inner swallows realpath errors as diagnostics

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-project/src/tsconfig/mod.rs:615-627`

When `realpath` fails on a tsconfig `extends` target, the error is recorded as a diagnostic and the extends resolution returns `None`. The caller never sees a `ProjectLoadError::Io` — the config silently inherits nothing. If the extends path points to a genuinely missing file, the diagnostic is `"failed to resolve extends path ..."` with the IO error string, which is user-visible but structurally different from a loading error.

Propagate `realpath` errors as `ProjectLoadError::Io` when the path exists but cannot be canonicalized (filesystem errors). Keep the diagnostic path only for the case where the path does not exist at all, since that is a user configuration issue, not a filesystem error.

#### READ-016 — Duration::from_millis silently panics on overflow

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Other
- **Location:** `glass-lint-project/src/loader.rs:239`

`Duration::from_millis(self.options.max_timeout_ms())` panics when the value exceeds `u64::MAX / 1000`. The option validation requires max_timeout_ms >= 1 but does not cap the upper bound. A user-provided value of `u64::MAX` triggers a panic at runtime.

Add an upper bound in option validation (e.g., `MAX_TIMEOUT_MS = 86_400_000` for 24 hours) and reject values above it at construction time. Alternatively, use `checked_add` or `saturating_mul` pattern with `Duration::from_secs`.

#### READ-017 — u64::try_from source length with u64::MAX fallback saturates to an unbounded value

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Other
- **Location:** `glass-lint-project/src/loader.rs:484`

`u64::try_from(source.source.len()).unwrap_or(u64::MAX)` replaces an overflow with `u64::MAX`, which exceeds any reasonable byte budget (`DEFAULT_MAX_PROJECT_SOURCE_BYTES` is 512 MiB). The subsequent `record_source_bytes` call will correctly flag the project as too large, but the `u64::MAX` value propagates into `metrics.bytes`, producing misleading profiling output.

Use `.min(limit + 1)` or `.saturating_add(0)` with a `u64` cast after the limit check instead of `u64::MAX`. Since the byte budget check runs immediately after, the behavior is correct but the metrics are corrupted on platforms where `usize` exceeds `u64` (none today, but a correctness smell).

## Systemic Themes

- **Typed state machines prevent ad-hoc validity tracking.** `Option` fields with semantic `valid` flags, tagged bit encodings, and `Result<(), T>` freeze patterns all encode phase transitions that the type system could enforce. Each untagged `Option` in a frozen-phase type is a maintenance burden.
- **One type, one job.** `ScopeGraph`, `FactPayload`, `ExportResolution`, and `ProjectPhaseTimings` each support multiple distinct use cases that would be clearer as separate, consuming-phase types. The `ScopeGraph` case is the most impactful because it couples collection, freezing, and querying.
- **Partial error should not mask prior partial error.** The project crate's timeout check in `finish_inner` replaces a more relevant partial-reason diagnostic with `Timeout`. Every recoverable error path should propagate its typed reason without a later overwrite.
- **Named constants make formulas readable.** The `262_144` literal in `FlowLimits`, the `1 << 31` tag in `append_linked`, and the `Depth`/`MAX_EXPORTS`/`MAX_PROJECT_REQUESTS` constants are all well-named when named and opaque when bare.

## Open Questions

1. **Should `SpanNormalizer` exist at all, or should `ByteRange` validation happen at the parser boundary?** The normalizer converts SWC spans to `ByteRange` and validates boundaries. If SWC's own span conversion is trusted for valid parse output, the `is_char_boundary` check is defense-in-depth. Measuring the memory cost vs. real bugs caught would clarify whether to keep or remove the retained source text.

2. **Can `FunctionTable::get_disjoint` be replaced by a borrowing pattern that does not need `split_at_mut`?** The current approach is safe and well-tested (miri-clean), but a query pattern that passes `&self` and returns both read and write handles via a session token would be more idiomatic. This is a design question, not a correctness issue.

3. **Should `ProjectSemanticModel::link_budget` be re-added with real enforcement?** The field was added with the intention of per-operation budget accounting during export linking but was never wired. If the SCC-DAG linker (READ-003 in prior audit) needs bounded convergence, the budget belongs on the fixed-point loop, not on the model.

4. **Should `BindingProvenance` store `NamePath` instead of both `NamePath` and `SmolStr` variants?** The presence of parallel `NamePath` and `SmolStr` variants (`ValueAlias` vs `ModuleExport`) suggests that some provenances are resolved to the name arena and others are not. Documenting or unifying this would reduce the surface of `BindingProvenance`.

## Coverage

The audit inspected all Rust source modules in `glass-lint-core` (11 top-level modules + ~30 submodules under `analysis/`, `api/`, `lint/`, `project/`) and `glass-lint-project` (9 modules: `admission`, `corpus`, `discovery`, `error`, `loader`, `options`, `resolver`, `tsconfig`, `walk`), including inline and dedicated test modules. Repository-level architecture documents, `CONTRIBUTING.md`, `TESTING.md`, and `AGENTS.md` were reviewed for intended boundaries. No source files were modified.
