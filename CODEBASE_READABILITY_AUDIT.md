# Codebase Readability Audit

## Summary

This read-only audit covers the complete Rust source and relevant tests of `glass-lint-core`, `glass-lint-datastructures`, and `glass-lint-project` (216 Rust modules, roughly 52,000 lines). It found 27 actionable issues: 13 High and 14 Medium severity. The highest-priority risks are unsound joins in assignment and exception provenance, unsafe reuse of invalid function effects, namespace/export-resolution inconsistencies, incorrect TypeScript project membership, a Unicode filename panic, and symlink policy bypasses.

The main performance risks in and around `lower_program` are three complete AST traversals with repeated interning and ordered-map lookup, quadratic alias resolution, an ever-growing rollback log, eager source-position indexing on cache hits, and full live-state clones at object-flow joins. These conclusions are based on static path analysis; the repository does not contain a reproducible lowering benchmark corpus, so optimization choices below deliberately require benchmark confirmation.

The three-crate test run passed in full (`cargo test -p glass-lint-core -p glass-lint-datastructures -p glass-lint-project`): 456 core unit tests plus integrations and doctests, 183 datastructure tests, and 61 project tests. The complete repository gate (`make ci`, including workspace check, Clippy, tests, and every harness suite) also passed. The green suite does not cover the adversarial examples identified below.

## Findings

### `glass-lint-core`: parsing and local lowering

#### READ-001 — The pre-AST depth guard still guesses JavaScript token context
- **Severity:** High
- **Fix Complexity** High
- **Category:** Other
- **Location:** `glass-lint-core/src/parse.rs:127-651`
- **Status:** Fixed

`syntax_depth` is a handwritten JavaScript lexer whose regex/division decision depends on the preceding byte or a short keyword list. Valid forms such as a postfix increment followed by division can be treated as a regex and cause the scanner to skip real nesting, so hostile nesting can bypass the pre-AST guard; other grammar positions can be classified in the opposite direction and reject valid input.

Implemented a bounded SWC-token pass before AST construction. It derives delimiter/member depth from token kinds, accounts for postfix `++`/`--` before classifying a following slash, charges each token event against the source bound, rejects lexer failures conservatively, and preserves the existing depth diagnostic; template expression boundaries remain covered by a dedicated parser-frontend state path because SWC exposes them through parser-driven rescans.

**Check:** `cargo test -p glass-lint-core parse::tests --no-default-features` passed (25 tests), including the postfix-increment/division regression, and `make ci` passed in full on 2026-07-28.

#### READ-002 — `lower_program` performs three full AST walks and repeats name work
- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:119-148`, `glass-lint-core/src/analysis/lowering/mod.rs:230-300`, `glass-lint-core/src/analysis/scope/mod.rs:88-103`, `glass-lint-core/src/analysis/scope/build/plan.rs:164-238`

Every cache miss walks the complete AST for scope planning, again for source-order scope collection, and again for fact building. The declaration planner also interns every identifier, member property, and property name, after which the collector and resolver repeat many of the same lookups; this is a structural multiplier in the hottest per-file path.

Retain one declaration/scope-shape prepass, but make it intern declarations and structural property keys only; let collection own interning for use sites and reuse those IDs through fact construction. Preserve explicit planner outputs for hoisting, shadowing, reassignment, and scope validation, and charge the same bounded events regardless of phase. Recommendation: instrument stage visits, intern hits/misses, and semantic charges, then accept the change only when those invariants and lowering benchmarks remain stable.

#### READ-003 — Conditional assignments fall back to an older strict identity
- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/frozen_assignments.rs:46-98`, `glass-lint-core/src/analysis/scope/query/bindings.rs:21-36`

When the latest textual assignment is conditional, `latest_at` returns `None`, and `binding_at` treats that as “no assignment” and falls back to a parameter alias or the declaration provenance. Thus `let f = fetch; if (flag) f = local; f()` can retain the strict `fetch` identity even though the use may observe the local function, contrary to the fail-closed contract.

Change `FrozenAssignmentIndex::latest_at` to return an explicit state such as `Absent`, `Known(AliasAssignment)`, or `Ambiguous`, and make `binding_at` map `Ambiguous` directly to unknown. Build that state with a bounded reaching-definition join that retains identity only when every reachable definition agrees, including zero-iteration loops, conditional arms, fallthrough, abrupt exits, and exceptional edges. Recommendation: add strict negative tests for strict-to-local and local-to-strict reassignment in each branch, and assert that no older declaration identity survives an `Ambiguous` result.

#### READ-004 — Source-order provenance leaks between sibling branches during collection
- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/build/assignments.rs:12-67`, `glass-lint-core/src/analysis/scope/build/history.rs:13-59`, `glass-lint-core/src/analysis/scope/build/traversal.rs:188-245`

`ScopeCollector.latest_assignments` is overwritten while visiting a conditional body, but `enter_conditional` and `exit_conditional` only change a depth counter; they do not checkpoint or join the history. Provenance inferred in an `else` arm or after a construct can therefore depend on whichever sibling was visited first or last, even though that path is not necessarily reachable.

Introduce an `AssignmentEnvironment` owned by the scope builder with checkpoint, rollback, and conservative join operations, and route every conditional, loop, and switch through it. Restore the incoming environment before each sibling, join only reachable exits, and represent disagreement as unknown while keeping source-order versions monotonic but separate from path-sensitive state. Recommendation: make branch isolation and join behavior unit-testable on the environment itself, then cover nested conditionals, loop fallthrough, switches, and abrupt exits through the collector.

#### READ-005 — The origin rollback log grows even when rollback is impossible
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/origin_map.rs:5-76`, `glass-lint-core/src/analysis/facts/control.rs:46-208`

Every `OriginMap` insert and remove clones the old value into `log`, including long straight-line regions with no active checkpoint. Rollback truncates only to a checkpoint and never commits or discards pre-checkpoint history, so lowering retains a duplicate mutation history for the whole file and copies `SmolStr` origin payloads unnecessarily.

Make `OriginMap` transaction-aware: maintain an active-checkpoint count, append inverse entries only while that count is nonzero, and commit each completed control region by discarding entries older than its surviving checkpoint. Charge every logged mutation and snapshot to the semantic budget before allocation, and preserve deterministic intersection order at joins. Recommendation: add memory-growth tests for long straight-line streams and deeply nested control, with assertions that retained storage is bounded by live state plus active deltas.

#### READ-006 — Alias resolution is quadratic and repeated
- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/model/value.rs:126-153`

`ValueTable::resolve` linearly searches a growing `SmallVec` on every alias hop, making one permitted chain O(depth²), and every consumer resolves the same chain again. With a value capacity of 65,536 and no resolution charge, a generated alias chain can consume disproportionate CPU inside lowering and matching.

Enforce that every binding target references an already-interned value, cache each binding's terminal value/root ID, and reject cycles at insertion. Make `ValueTable::resolve` charge one bounded step per hop and return unknown when the chain or budget is exhausted. Recommendation: add a maximum-length alias-chain benchmark plus malformed-cycle tests, and verify that repeated consumers hit the terminal cache instead of traversing the chain again.

#### READ-007 — Cache hits rebuild a full source line index eagerly
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/diagnostic.rs:48-134`, `glass-lint-core/src/analysis/local.rs:91-108`, `glass-lint-core/src/project/session/artifacts.rs:93-107`

A semantic artifact cache hit still constructs `LocatedSourceContext`, scans the complete source for line starts, and scans every line of at least 256 bytes again to build Unicode checkpoints. Projects with unchanged large sources therefore pay O(source bytes) position-index work even when no finding needs a position.

Store the source-derived `SourceLineIndex` in the reusable source/fingerprint artifact, with project-relative path context remaining outside the artifact. Use an ASCII fast path and materialize Unicode checkpoints only when a non-ASCII range lookup requires them, while keeping cache identity independent of path. Recommendation: benchmark cold lowering, clean cache hits, and no-finding runs separately and assert that a clean hit does not rescan source bytes until position mapping is requested.

#### READ-008 — Scope lookup pays ordered-tree and repeated interner costs per ancestor
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:90-99`, `glass-lint-core/src/analysis/scope/frozen_assignments.rs:11-43`, `glass-lint-core/src/analysis/scope/build/history.rs:13-54`, `glass-lint-core/src/analysis/scope/query/bindings.rs:120-135`

Bindings and assignment indexes use nested `BTreeMap`s in lookup-heavy local analysis, and `binding_with_scope_at` calls `name_id` once per ancestor rather than once per query. Deterministic output does not consume these maps directly, so logarithmic pointer-heavy lookup is being paid for internal state whose keys are dense IDs.

Resolve `NameId` once per query, then store bindings and assignments in a dense scope-indexed vector whose per-scope map is keyed by `NameId`; sort keys only when freezing or producing observable output. Keep the working representation private to the scope model so deterministic ordering remains an output concern. Recommendation: benchmark deep scopes and minified repeated identifiers against the current index and require equivalent lookup results before replacing it.

#### READ-009 — Invalid function effects are still turned into helper summaries
- **Severity:** High
- **Fix Complexity** Low
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:1-10`, `glass-lint-core/src/analysis/flow/summary/summaries.rs:42-95`

The effect contract says summaries invalidated by unsupported control or budget exhaustion are not used for propagation, and cross-module flow explicitly checks `is_invalid`. `FunctionSummaries::collect_facts` nevertheless iterates every effect and copies its calls into local helper summaries, allowing the local object-flow path to consume information declared incomplete.

Filter `is_invalid` effects before `FunctionSummaries::collect_facts` creates any helper summary, and make the filtered collection the only input to local and cross-module propagation. Calls to or through an invalid helper must produce neither projected sinks nor return identity. Recommendation: add local/cross-module parity tests for member writes, unsupported control, and effect-budget exhaustion, including a direct assertion that no invalid summary is retained.

#### READ-010 — Object-flow joins clone all live aliases and states
- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:25-45`, `glass-lint-core/src/analysis/flow/projector/state.rs:174-258`

`FlowEnvironment` is described as an O(1) snapshot, but `join_environments` restores branches and clones the complete alias and state maps before retaining common entries, then rebuilds reference counts. On branch-heavy flows the work is O(live state × reachable branches), independent of the mutation-log budget and in addition to ordered-map costs.

Represent each `FlowEnvironment` branch as an incoming snapshot plus a bounded mutation delta, and compute `join_environments` by intersecting those deltas without cloning the complete live alias/state maps. Charge each compared entry and retained requirement key so the flow limit bounds CPU as well as output. Recommendation: add a selected-rule benchmark with many live objects across nested diamonds and require allocation and operation counts to scale with changed entries rather than total live state.

### `glass-lint-core`: project linking and projection

#### READ-011 — An unresolved internal namespace is labeled external
- **Severity:** High
- **Fix Complexity** Low
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/identities.rs:202-225`

`resolve_namespace` maps a missing resolution-table entry to `ExportResolution::External` without checking the request syntax. An unresolved `import * as ns from "./missing"` can therefore acquire an external wildcard identity, while named-import resolution correctly treats an absent internal outcome as unknown.

Centralize linked-target-to-export-resolution conversion in one helper used by named and namespace imports, and classify every unresolved relative, absolute, or `#` request as unknown. Only a confirmed external or builtin target may produce an external namespace identity. Recommendation: add parity tests for missing, outside, unsupported, builtin, and bare-package namespace requests and assert that namespace and named imports share the same classification.

#### READ-012 — Star-export collection can overwrite an authoritative direct export
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/identities.rs:142-200`

The method documents direct exports as authoritative, inserts them first, then inserts each star-exported child and changes any conflicting previous value to `Ambiguous`. A module with a direct `foo` and `export *` from a module containing another `foo` therefore loses the direct export that should win.

Make the resolved fixed-point `ExportTable` the sole input to namespace expansion, eliminating the second star-edge walk. Preserve direct-export precedence in that table and mark only conflicting star-derived candidates ambiguous before exposing the namespace. Recommendation: test direct-versus-star precedence, two conflicting stars, cycles, and traversal-order independence against the final namespace identity.

#### READ-013 — An immutable project model contains a mutable `RefCell` lookup cache
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/model.rs:189-209`, `glass-lint-core/src/analysis/project/exports.rs:107-156`, `glass-lint-core/src/analysis/project/state.rs:118-147`

`ProjectSemanticModel` query methods mutate `lookup_cache` through `RefCell`, making the frozen semantic model non-`Sync`, adding runtime borrow checks, and obscuring when cache state is valid. A second bounded export cache abstraction exists in linker state, so capacity and invalidation policy are split across owners.

Move `lookup_cache` into an explicit mutable `LinkingSession` that owns the model version, negative entries, capacity, and eviction policy; keep `ProjectSemanticModel` immutable and `Sync`. Require every export lookup to receive that session, and invalidate the session when the linked model version changes. Recommendation: add a concurrency test proving the frozen model is shareable and a bounded-cache test proving positive and negative entries have deterministic capacity behavior.

#### READ-014 — Handwritten graph algorithms add avoidable complexity and quadratic edge deduplication
- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/project/linker/scc.rs:1-135`

The project linker maintains its own iterative Kosaraju decomposition and topological sort over nested ordered collections. SCC DAG construction checks `Vec::contains` for each edge before insertion, making high-fanout duplicate edges quadratic in the component out-degree, and the custom implementation expands the correctness surface for cycles and ordering.

Replace the handwritten SCC and topological-sort implementation with [`petgraph`'s maintained SCC primitive](https://docs.rs/petgraph/latest/petgraph/algo/fn.kosaraju_scc.html) over dense module indexes. Sort component members and the final ready order only at the deterministic output boundary, and convert graph errors into the linker's existing bounded diagnostic state. Recommendation: benchmark a dense re-export graph and retain cycle, fixed-point, and traversal-order golden tests before deleting the bespoke graph code.

### `glass-lint-datastructures`

#### READ-015 — `ParentPathStore` exposes unchecked mixed-domain construction
- **Severity:** High
- **Fix Complexity** High
- **Category:** API
- **Location:** `glass-lint-datastructures/src/path_trie/store.rs:26-112`, `glass-lint-datastructures/src/path_trie/store.rs:164-235`, `glass-lint-datastructures/src/path_trie/types.rs:5-31`

The general path store publicly exposes `raw_path_id` and `append_linked`, accepts caller-supplied parent/depth values, and uses a tag bit whose interpretation belongs to core's summary overlay. It also checks capacity before reusing an existing linked edge, and `segments` silently returns an empty iterator when collection fails, conflating an invalid ID with the empty path.

Create a private core-owned linked-path store that translates validated `PathId` values into a dedicated overlay newtype, and remove raw-ID construction and `append_linked` from the public path-store API. Have the store find an existing edge before checking capacity, derive depth from the parent, and return `Result` for invalid segment traversal so invalid IDs cannot look like empty paths. Recommendation: add forged-tag, foreign-parent, full-store reuse, and invalid-versus-empty tests at both the datastructure boundary and the core adapter.

#### READ-016 — `IndexTable::insert` conflates replacement with resource rejection
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** API
- **Location:** `glass-lint-datastructures/src/table.rs:3-87`

The generic table hard-codes a 2²⁰ maximum and returns `false` both when it successfully replaces an occupied value and when it rejects a sparse/oversized ID. Owners therefore cannot distinguish normal replacement from exhaustion or record an incomplete analysis status, and one trusted sparse ID can still allocate almost a million `Option` slots.

Make `IndexTable` require an owner-supplied capacity and return a typed `InsertOutcome` distinguishing insertion, replacement, and `OutOfRange`. Keep sparse allocation behind the capacity check and require each production owner to handle `OutOfRange` as incomplete analysis rather than treating it as replacement. Recommendation: test sparse IDs just below and above the configured limit, replacement payloads, and every owner's exhaustion path.

#### READ-017 — Cache hashing is a handwritten scalar FNV loop
- **Severity:** Medium
- **Fix Complexity** Low
- **Category:** Other
- **Location:** `glass-lint-datastructures/src/fingerprint.rs:1-48`, `glass-lint-core/src/analysis/local.rs:48-88`

Every artifact fingerprint scans the complete source through a byte-at-a-time FNV-1a loop, and raw `fnv_init`/`fnv_write` functions couple core to the algorithm. The full cache key collision-checks content, so cryptographic hashing is unnecessary, but FNV is usually a poor throughput choice for this hot whole-source pass.

Replace the scalar FNV loop with `xxhash-rust`'s streaming XXH3 implementation behind `Fingerprint`'s `write`/finish API, and remove raw algorithm functions from callers. Benchmark short and large sources before setting the chunking policy, then bump `FINGERPRINT_VERSION` and retain full-key collision verification. Recommendation: make the hasher choice private to `glass-lint-datastructures` and add compatibility tests proving old cache entries miss cleanly after the version bump.

### `glass-lint-project`: filesystem admission and loading

#### READ-018 — Unicode filenames can panic during extension checks
- **Severity:** High
- **Fix Complexity** Low
- **Category:** Other
- **Location:** `glass-lint-project/src/options.rs:95-120`

`SourceExtensionSet::supports` computes a byte offset and slices `str` without checking a UTF-8 boundary. A filename whose byte length is sufficient but whose suffix start falls inside a multibyte character, such as `éjs` against `.js`, panics during discovery instead of returning unsupported.

Centralize extension matching in a helper that obtains the suffix with `str::get(start..)` and returns unsupported for a non-boundary start. Use that helper for both normal extensions and declaration-file rejection, preserving the validated ASCII policy for configured suffixes. Recommendation: add Unicode filenames at every suffix-length boundary plus custom Unicode-extension tests and assert discovery never panics.

#### READ-019 — `files` and `include` are incorrectly mutually exclusive
- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig/selection.rs:20-28`, `glass-lint-project/src/tsconfig/selection.rs:100-193`, `glass-lint-project/src/discovery.rs:270-300`

The merge discards `include` and `exclude` whenever `files` is present, and discovery chooses either explicit files or patterns. TypeScript instead selects the union of `files` and `include`, with `exclude` filtering only the `include` side; missing explicit files should also produce a diagnostic rather than disappear. See the [TypeScript 2.0 configuration semantics](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-2-0.html).

Represent explicit files and compiled include patterns as separate members of one effective selection, preserving each field's declaring-config origin. Admit explicit files first with a diagnostic for every missing or unsupported entry, then apply `exclude` only to include matches and deduplicate through the shared admission set. Recommendation: add same-config and inherited fixtures for `files`, `include`, `exclude`, and output directories, and compare the admitted set with TypeScript's union semantics.

#### READ-020 — Generic glob matching does not implement TypeScript directory-pattern semantics
- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig/selection.rs:247-334`

The implementation passes raw patterns to `glob::Pattern`; consequently `include: ["src"]` does not include `src/a.ts`, and an `exclude` or `outDir` of `"dist"` does not exclude `dist/a.js`. TypeScript treats a final segment without an extension or wildcard as a directory and applies supported-extension rules; the current code is also O(files × include/exclude patterns). See the [TypeScript TSConfig include reference](https://www.typescriptlang.org/tsconfig/explainFiles.html).

Normalize TypeScript directory and wildcard semantics into repository-relative patterns, then compile them into a [`globset::GlobSet`](https://docs.rs/globset/latest/globset/struct.GlobSet.html) for one multi-pattern matching pass. Keep extension admission in the existing validated source policy and make declaration-file exclusion an explicit selection stage. Recommendation: build `tsc --listFiles` fixtures for plain directories, basename excludes, `**/`, extensionless patterns, dotfiles, separators, and invalid patterns, and require identical membership.

#### READ-021 — Missing and package-based `extends` diagnostics still fail open
- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig/mod.rs:339-381`, `glass-lint-project/src/tsconfig/mod.rs:478-529`

Package-based `extends` is declared unsupported and a missing relative target is diagnosed, but both paths continue as if no parent existed. A child with no local `files`/`include` then falls back to `**/*`, broadening a project whose unresolved base may have been restrictive; TypeScript permits Node-style resolution for `extends`. See the [official `extends` contract](https://www.typescriptlang.org/tsconfig/extends.html).

Resolve `extends` through the existing Oxc/Node resolution stack, including package-based targets, and propagate a typed invalid-inheritance state for missing targets, cycles, invalid types, and paths outside the project boundary. Source selection must admit nothing from an invalid inheritance chain while retaining deterministic diagnostics. Recommendation: add fixtures for relative files, package bases, cycles, invalid `extends` values, and boundary escapes, and verify that every failure is both diagnosed and fail-closed.

#### READ-022 — Shared `extends` ancestors are reparsed and recharged
- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Complexity
- **Location:** `glass-lint-project/src/discovery.rs:159-267`, `glass-lint-project/src/tsconfig/mod.rs:384-530`

Each referenced project calls `build_effective_config` with a fresh inheritance chain, so shared base configs are canonicalized, read, JSONC-stripped, parsed, rebased, and charged once per descendant. `config_count` therefore counts traversals rather than unique configuration documents and can reject a diamond graph despite the reference walk's own `visited` set.

Introduce one `ConfigTraversalContext` owning the canonical parsed/effective cache, diagnostics, active chain, counters, deadline, and byte budget. Count and charge each canonical config document once, detect cycles against the active chain, and cache origin-relative fields before child-specific rebasing. Recommendation: route every `extends` and project-reference load through this context, remove the current `too_many_arguments` coordinator, and add a diamond-inheritance test proving shared ancestors are parsed and charged once.

#### READ-023 — Invalid output-directory options disappear silently
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Other
- **Location:** `glass-lint-project/src/tsconfig/mod.rs:172-212`, `glass-lint-project/src/tsconfig/selection.rs:162-185`

A non-object `compilerOptions`, or wrong-type `outDir`/`declarationDir`, becomes `Absent` and emits no field diagnostic. Generated output can then be selected by the default include even though the configuration attempted to exclude it, and callers cannot distinguish omission from malformed policy.

Parse `compilerOptions` and each supported nested field into an explicit `Valid`/`Invalid` state, emit deterministic field-level diagnostics, and make an invalid output-exclusion field fail closed for project membership. Carry that invalid state through inherited effective configuration so a child cannot silently broaden selection. Recommendation: test null, scalar, array, and wrong nested types in both base and child configs and assert that malformed output settings never select generated files.

#### READ-024 — Config byte accounting trusts stale metadata and ignores remaining budget
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/budget.rs:3-59`, `glass-lint-project/src/tsconfig/mod.rs:300-329`

Config loading charges `metadata.len()` before reading, then allows every file to read `max_config_bytes + 1` rather than the remaining aggregate allowance and never reconciles the actual byte count. Filesystem growth races and repeated shared ancestors can therefore make I/O exceed the intended aggregate bound, while errors are mislabeled `ProjectSourceTooLarge`.

Give the project budget a remaining config-byte reservation, limit each read to remaining plus one byte, and commit the actual bytes consumed after the read completes. Return a config-specific typed exhaustion error and charge each canonical config only once through `ConfigTraversalContext`. Recommendation: test truncation and growth races with a controllable reader plus aggregate-boundary cases across several small configs, asserting that no read exceeds the remaining allowance.

#### READ-025 — Early canonicalization bypasses the no-symlink policy
- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:256-299`, `glass-lint-project/src/admission.rs:173-200`, `glass-lint-project/src/walk.rs:14-40`

The initial selection is canonicalized before discovery calls `resolve_root`, and general admission canonicalizes before policy classification. An explicitly selected symlink is therefore replaced by its target before `follow_symlinks = false` can inspect it; resolver targets take the same path, so the option governs directory walking more reliably than direct admission.

Preserve the lexical path through policy evaluation and inspect `symlink_metadata` for the selected path and every traversed component before canonicalization. Reject any symlink when `follow_symlinks` is false; when it is true, canonicalize once and apply containment and exclusion to the target path. Recommendation: add file, directory, intermediate-component, in-root, out-of-root, and resolver-target symlink fixtures for both policy settings, including direct explicit selections.

#### READ-026 — Files consume admission budget before their bytes are accepted
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:508-584`, `glass-lint-project/src/admission.rs:229-240`, `glass-lint-project/src/corpus.rs:27-73`

`process_wave` commits a path to `AdmissionSet` before aggregate-byte reservation; if that file is rejected, it still consumes file capacity and later inflates `metrics.files`. The loop also stats the file, then `read_source_bytes` opens and stats it again, while actual-size reconciliation happens only after the admission mutation.

Create a bounded read/reservation result that opens each file once, validates per-file and remaining aggregate limits, reads at most remaining plus one byte, and commits admission and progress only after acceptance. Preserve partial-report behavior by retaining only successfully committed files and continue deterministically after a rejected middle file. Recommendation: add an oversized-middle-file fixture followed by a small file and assert both the admitted set and file metrics exclude the rejected file.

#### READ-027 — `SourceCorpus` does not uphold its one-root contract
- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/corpus.rs:75-128`, `glass-lint-project/src/corpus.rs:135-215`

The type says it owns one canonical project root and derives it from the first discovery root, but `canonical_root` remains `None`; every root and every later load creates a new admission authority. `discover(&[root_a, root_b])` can therefore combine unrelated trees, and `load` without a configured root trusts the target file's parent as a new project boundary.

Make a canonical root mandatory in `SourceCorpus::new`, retain it for the corpus lifetime, and reject every discovery root or load outside that boundary. Return project-relative admitted paths when callers need to carry the admission proof across operations, while keeping multi-root aggregation outside this type. Recommendation: add tests for two roots, a later outside load, and a relative-path round trip, and assert that one corpus can never create a second admission authority.

## Systemic Themes

- **Control state uses several incompatible approximations.** Scope collection, frozen assignment lookup, fact identity tracking, function effects, and object-flow projection each implement branch semantics independently. A shared bounded environment/join vocabulary would remove the current “none means absent or ambiguous” bugs and make strict identity fail closed consistently.
- **Bounds often cap outputs but not work.** Alias walks, origin logs, map clones, graph edge deduplication, config rereads, and glob matching can perform substantially more work than their counters describe. Budgets should charge mutations, comparisons, bytes actually read, and unique documents.
- **Determinism is enforced too early with ordered trees.** Local scope, assignment, flow, and linker state frequently uses `BTreeMap`/`BTreeSet` even where only final output needs ordering. Dense IDs and insertion-ordered/hash storage can serve hot queries, with sorting confined to freeze/report boundaries.
- **TypeScript configuration is a semantic subsystem, not generic JSON plus globs.** Inheritance, directory patterns, explicit files, output exclusions, package resolution, and diagnostic behavior interact. Model those phases in one cached traversal context and validate behavior against small `tsc --listFiles` fixtures.
- **Existing crates should own commodity algorithms.** `globset` is a better multi-pattern engine once TypeScript patterns are normalized; `petgraph` can own SCC/toposort; a maintained xxHash implementation can own cache hashing; and the already-used Oxc resolver should be reused for Node-style config resolution where possible. Domain-specific identity and fail-closed joins should remain in Glass Lint.

## Open Questions

No unresolved questions remain from this audit. The following decisions should guide remediation:

1. Ambiguous, conditional, incomplete, or budget-exhausted identity is **unknown**; it must never fall back to an older strict declaration.
2. Use SWC tokenization for the pre-AST syntax-depth boundary and reject any tokenization uncertainty before AST construction.
3. Retain a declaration/hoisting prepass, remove repeated non-declaration interning, and move hot lookup ordering to the freeze/output boundary.
4. Implement TypeScript project membership as `files ∪ (include − exclude)` with origin-relative inherited paths, directory-pattern expansion, Node-style `extends`, and diagnostics for missing explicit files; invalid inheritance fails closed.
5. `SourceCorpus` owns exactly one stable project root, and multi-root aggregation belongs in a separate explicitly named abstraction.
6. Place third-party algorithms behind Glass Lint domain APIs, require workload benchmarks before adoption, and keep deterministic ordering at observable boundaries.

## Coverage

Reviewed all production and test modules under:

- `glass-lint-core/src`, including parsing, local lowering, scope planning/collection/query, facts, effects, object and cross-module flow, matching, project linking, artifact caching, reports, public configuration, and limits
- `glass-lint-datastructures/src`, including budgets, diagnostics, names, paths, path tries, fingerprints, and dense tables
- `glass-lint-project/src`, including options, admission, walking, corpus loading, project loading, resolution, resource budgets, tsconfig parsing/inheritance/selection, and tests

Also reviewed root and owning-crate architecture documents, `TESTING.md`, `CONTRIBUTING.md`, the historical audit, current dependency manifests, and official TypeScript configuration semantics. Validation was read-only apart from this report; no Rust source, tests, configuration, dependencies, or other documentation were changed.
