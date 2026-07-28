# Codebase Readability Audit

## Summary

This audit covers all Rust production modules and the relevant tests in `glass-lint-core`, `glass-lint-datastructures`, and `glass-lint-project` (about 50,000 lines total). It found 32 actionable issues: 16 High, 15 Medium, and 1 Low severity. 21 have been fixed (6 High, 14 Medium, 1 Low), leaving 10 open (9 High, 1 Medium). The most important remaining correctness risks are control-insensitive assignment provenance, exceptional-path identity leakage, and tsconfig inheritance being rebased to the wrong directory. The most important remaining boundedness risks are an unbounded function-summary pass and public dense-ID structures that can be driven into enormous sparse allocations.

The existing `profile.json.gz` was also inspected against its matching profiling binary. It is supporting rather than dispositive evidence because it does not carry a reproducible workload manifest, but roughly half of the main worker's samples include `FactBuilder` statement traversal, with resolver/name operations prominent below it. That agrees with the static conclusion that lowering work inside `FactBuilder`, interning, and resolver-owned indexes deserves priority.

The three-crate test run passed: 393 core unit tests plus integration tests, 181 datastructure tests, 39 project tests, and doctests. The findings below therefore also identify adversarial cases that the current suite does not exercise.

## Findings

### `glass-lint-core`: parsing and local lowering

#### READ-001 — The syntax-depth prepass mis-tokenizes valid regex and escaped templates
- **Severity:** High
- **Fix Complexity** High
- **Category:** Other
- **Location:** `glass-lint-core/src/parse.rs:214-339`

The byte scanner handles comments and quoted strings but has no regex-literal state, so dots and escaped delimiters inside a valid regex contribute to member or nesting depth; template content likewise treats an escaped backtick as a closing delimiter. A valid, flat source containing a long `/...../` pattern or many `\(` atoms can therefore be rejected as `syntax_depth_exceeded` before SWC sees it. Replace the partial tokenizer with a bounded lexer-level depth guard that recognizes regex/context and template escapes, or derive depth while consuming SWC lexer tokens before AST construction. Preserve the pre-AST resource guard, and add valid delimiter-heavy regex, character-class, division, escaped-template, comment, and hostile-depth tests.

#### READ-002 — Source-order assignment history confuses possible and definite provenance
- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/frozen_assignments.rs:11-83`

`latest_at` selects the last textual assignment without distinguishing an assignment that is possible from one that is definite at the use position. Consequently, `let f = local; if (flag) f = fetch; f()` can acquire strict global provenance even though the use may still observe `local`, violating the documented fail-closed identity policy. The analysis does not need exact reachability for every condition: treat both sides of an unknown condition as possible, optionally discard a side when a bounded constant evaluator proves it impossible, and join the resulting provenance states. Retain a strict identity only when all reachable definitions agree; otherwise return unknown. Keep the existing fast source-order index as a candidate index, but make the final decision path-aware enough to account for conditional aliases, zero-iteration loops, fallthrough, abrupt exits, and exceptional edges.

#### READ-003 — `try`, `catch`, and `finally` share impossible instance state
- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/control.rs:149-165`

`record_try` visits the handler with the state left by the end of the `try`, then visits `finally` with the state left by `try` or `catch`; neither state is definite on those exceptional paths. This disagrees with the later object-flow projector, which restores a baseline at `CatchStart`, and can attach a proven constructed-instance identity inside a handler or finalizer that cannot observe it. Give fact-time identity provenance the same explicit exceptional-edge join semantics as the projector, conservatively intersecting reachable states before `finally`. Add negatives where construction occurs after a possibly throwing statement, in only the handler, and in only one path reaching `finally`.

#### READ-004 — Control constructs clone whole identity maps without charging their cost
- **Severity:** High
- **Fix Complexity** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/control.rs:37-193`

Every branch, loop, switch, conditional, and `try` clones the full `instance_origins` and `class_origins` maps, and a switch clones the incoming instance map once per case. This makes control-heavy or minified input approach O(live identities × control regions) allocation and tree-copy work, while the semantic budget charges only nearby events rather than the number of copied entries. Use rollback logs, persistent snapshots, or dense copy-on-write state keyed by `ValueId`, with budget charges proportional to changed entries. Preserve deterministic joins and validate the change with nested-control stress cases and a lowering benchmark containing many live instances.

#### READ-005 — Name interning constructs an owned key before checking for a hit
- **Severity:** Medium
- **Fix Complexity** Low
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-datastructures/src/name.rs:47-75`

`NameTable::intern` converts every input to `SmolStr` and calls `insert_full`, even for the overwhelmingly common repeated-name case; on exhaustion it also inserts and pops every novel key. The captured profile's hottest identifiable leaf was `SmolStrBuilder::finish`, consistent with this cost inside name-heavy lowering. Perform a borrowed `str` lookup first, reject a new key before mutation when capacity is exhausted, and allocate only on a true miss. Keep existing-name lookup valid after exhaustion and retain the first precise exhaustion record.

#### READ-006 — Numeric constant conversion round-trips through text
- **Severity:** Medium
- **Fix Complexity** Low
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/syntax/constant/types.rs:12-18`

Every finite integral numeric literal handled as a static index is converted with `to_string()` and parsed back into `usize`, adding formatting, allocation, and parsing in the lowering hot path. Validate non-negativity, integrality, and the `usize` upper bound numerically, then use a checked conversion whose round-trip semantics are covered by tests. Include values around 2^53 and the platform `usize` boundary so the optimization does not admit imprecise floating-point integers.

#### READ-007 — Local lowering performs avoidable full-source copying and eager column indexing
- **Severity:** Medium
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:48-88`

`SpanNormalizer::new` copies the already shared `SourceText` into a new `Arc<str>`, while `SourceLineIndex` later scans every long line and materializes character checkpoints even when no evidence is emitted; cache hits rebuild that index. Share the admitted source allocation with the normalizer and cache the source-derived line-start index alongside reusable semantic state. Make Unicode checkpoints lazy or ASCII-specialized, while retaining path-specific report attachment and collision-safe artifact cache keys.

#### READ-008 — Lookup-only lowering indexes use ordered trees for dense local IDs
- **Severity:** Medium
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:261-269`

`FunctionEffect` uses `BTreeMap<ValueId, ...>` for value roots and parameter indexes, and the collection pass adds another global `BTreeMap<ValueId, SymbolCallProvenance>`; `FactBuilder` and `CallResultTable` have similar lookup-only trees. `ValueId` is a dense artifact-local ID, and these maps are not iterated to produce output order, so every fact pays logarithmic pointer-heavy lookups unnecessarily. Introduce an owner-specific dense table or hash index and sort only at actual deterministic output boundaries. Keep unknown-ID rejection and per-function reset semantics explicit rather than exposing a generic raw vector.

#### READ-009 — Valid alias chains silently stop resolving after 16 bindings
- **Severity:** Medium
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/model/value.rs:127-140`

`ValueTable::resolve` returns `None` after an undocumented fixed 16 binding hops, even though construction permits longer acyclic alias chains and no incomplete status is recorded. A valid chain of local aliases can therefore lose a static value or strict identity solely because it crosses this hidden threshold. Since binding targets are arena-owned and should form a backward DAG, resolve to the root with checked cycle detection/path compression or a configured semantic budget. If a hard cap remains, return a typed unknown reason and surface it consistently in analysis status.

#### READ-010 — Function summaries are outside the flow budget and can return a partial fixed point
- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary/summaries.rs:40-205`

`FunctionSummaries::collect` runs before the bounded projector, has no cap on summaries, call edges, or sink combinations, and stops after 64 rounds without reporting whether work remains. A large helper graph can consume unbounded memory, while a propagation chain longer than the round cap yields incomplete evidence that is treated as complete. Pass a flow budget into summary construction, bound retained sinks and worklist entries, and return an explicit exhaustion outcome. On exhaustion, discard affected evidence and record the same project-level incomplete status used by the other flow passes.

#### READ-011 — Matcher builders silently normalize malformed identity paths
- **Severity:** Medium
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/decl.rs:161-195`

Builder methods accept empty global names and member chains, while `SymbolPath::from_chain` drops empty segments, so `a..b` becomes `a.b` and its evidence label still retains the malformed spelling. `MatcherBuildError::EmptyChain` exists but is never applied at these boundaries. Centralize construction through validated `GlobalName` and `MemberChain` types that reject empty, whitespace-only, leading/trailing-dot, and repeated-dot input. Syntax-derived paths may keep their infallible constructor, but provider-authored matcher input must use the checked path.

### `glass-lint-core`: matching, flow, and project linking

#### READ-012 — Borrowed occurrence merging is O(bucket count × occurrence count)
- **Severity:** Medium
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:62-133`

Each `BorrowedOccurrenceIter::next` scans the head of every base/overlay bucket with `min_by_key`, so a linked namespace with `k` buckets and `n` occurrences costs O(k·n). Package queries repeatedly construct this iterator, including for single buckets. Use a deterministic binary-heap k-way merge or pre-normalized flattened overlay, with a zero-allocation single-bucket fast path. Preserve the current event/span/bucket tie-break and deduplication contract in equivalence tests.

#### READ-013 — Cross-flow adjacency retains duplicate edges
- **Severity:** Medium
- **Fix Complexity** Low
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/cross/sources.rs:70-117`

Return and argument analysis push destination keys into adjacency vectors without sorting or deduplicating them, although repeated returns and equivalent arguments can create the same edge. Every propagated source candidate then revisits each duplicate, spending budget and tree lookups without changing the result. Normalize every adjacency bucket once after construction, or insert through a domain adjacency set and freeze to a sorted vector. Charge edge construction to the flow budget and add repeated-edge stress coverage.

#### READ-014 — The export lookup cache ignores its capacity and forces `RefCell` mutation
- **Severity:** High
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/state.rs:120-140`

`ExportLookupCache::new(_capacity)` discards the link-operation limit and the `BTreeMap` grows for every distinct negative star-export lookup. Because the cache is embedded in the frozen project model, `lookup_export(&self)` also requires `RefCell`, moving borrow correctness to runtime and preventing an otherwise immutable model from being naturally shareable. Enforce a typed capacity with fail-closed exhaustion, or move memoization into a mutable linking/projection query session and freeze any reusable result. A nested module/name map can also avoid cloning `SmolStr` merely to perform `get`.

#### READ-015 — Namespace collection overwrites star-export ambiguity
- **Severity:** High
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/identities.rs:128-169`

`collect_exported_identities` first inserts the authoritative export-table entries, then recursively walks each star-exported module and overwrites entries with the same namespace key. If two star exports provide the same name, an `Ambiguous` result computed for the aggregator can be replaced by whichever child is visited last, creating a strict false positive. Either consume only the fixed-point `ExportTable` for namespace members or merge recursively discovered candidates with the same ambiguity lattice used by the linker. Add same-name star exports in both declaration orders, cycles, and a non-conflicting control case.

#### READ-016 — Call-result identity selects the first return from an invalid effect
- **Severity:** High
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/identities.rs:22-90`

`call_result_identities` does not check `target.is_invalid()` and uses the first parameter-independent return, ignoring other returns with conflicting provenance. A conditionally returning helper can therefore assign its caller the identity of one arbitrary branch even though the effect pass explicitly marked the function unsafe. Reject invalid target effects and join every reachable return, accepting an identity only when all proven candidates agree. Cover multiple returns, branch order reversal, unknown returns, parameter projections, and one unambiguous helper.

#### READ-017 — Outside-path normalization corrupts absolute parents and accepts drive paths
- **Severity:** High
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Newtype
- **Location:** `glass-lint-core/src/project/input.rs:23-79`

For absolute paths, every `..` component is discarded instead of popping the preceding component, so `/a/../b` becomes `/a/b`; meanwhile `normalize_relative` accepts `C:/project/file` because only a leading slash is considered absolute. These normalized strings participate in containment diagnostics and resolver result identity. Define a platform-neutral path grammar that recognizes POSIX roots, drive prefixes, and UNC forms, and perform stack normalization without crossing a root. Add cross-platform string tests independent of the host OS and keep filesystem canonicalization in `glass-lint-project`.

### `glass-lint-datastructures`

#### READ-018 — Public raw ID constructors defeat store-local identity
- **Severity:** High
- **Fix Complexity** High
- **Category:** API
- **Location:** `glass-lint-datastructures/src/path_trie/types.rs:5-36`

`PathId::from_raw`, `NameId::from_raw`, the public link tag, and `IdIndex::from_raw` let callers forge IDs or carry them across stores, contrary to the crate architecture's store-local identity invariant. This weakens strict identity and makes downstream bounds checks responsible for repairing invalid public states. Make raw constructors and tags crate-private, and expose translation only through the store that can validate provenance and range. If serialization requires raw values, use a checked deserialization boundary tied to an owning table rather than a universally constructible ID.

#### READ-019 — `IndexTable` can allocate billions of empty slots
- **Severity:** High
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Encapsulation
- **Location:** `glass-lint-datastructures/src/table.rs:33-75`

`insert` resizes directly to any caller-supplied `u32` ID with no capacity or density validation, so a forged or sparse ID can request roughly four billion `Option<T>` slots. `len` and `is_empty` then scan the entire allocated capacity. Require an explicit maximum/dense-ID allocator or return a typed insertion error when an index is outside the admitted range, and maintain occupied length incrementally. Keep the O(1) table for trusted dense owners, but do not make that trust an undocumented generic precondition.

#### READ-020 — `ParentPathStore` exposes two incompatible ID domains
- **Severity:** High
- **Fix Complexity** Extreme
- **Category:** API
- **Location:** `glass-lint-datastructures/src/path_trie/store.rs:46-208`

Linked nodes return tagged IDs, but public methods handle them inconsistently: `depth`, `parent`, and `segment` untag them, while `is_valid`, `collect_segments`, `rebuild_without_first`, and the final `starts_with` comparison do not. `segments(tagged_id)` can silently return an empty iterator, and `append_linked` accepts caller-supplied depth/parent state and refuses to reuse an existing edge once capacity is full. Split ordinary interned paths from overlay/link paths with distinct ID and store types, or make the linked mechanism private to a validated overlay owner. Ensure every public operation either accepts one coherent typed ID domain or returns an explicit invalid-ID error; add round-trip tests over every operation on linked IDs.

### `glass-lint-project`: tsconfig and discovery

#### READ-021 — Inherited tsconfig paths are rebased to the child config
- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig/mod.rs:367-500`

Inheritance merges raw `files`, `include`, `exclude`, `outDir`, and `declarationDir` strings, then compiles the result once using the extending config's directory. TypeScript paths are relative to the config file where each field was declared, so a parent in another directory selects and excludes the wrong files. Normalize each selection field into an origin-aware path/pattern representation before merging, and preserve that origin through compilation. Add parent/child fixtures in different directories for every supported field, including overridden and inherited combinations.

#### READ-022 — Malformed tsconfig selection fields fail open
- **Severity:** High
- **Fix Complexity:** Medium
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-project/src/tsconfig/selection.rs:25-95`

`WrongType` and `Null` fields produce diagnostics but are then treated like absence by `.ok()` or wildcard match arms. With no valid parent, a malformed `files` or `include` can therefore fall back to `**/*`, broadening project membership despite the repository's fail-closed policy. Carry field validity into `MergedSelection` and make an invalid membership-controlling field select no files until corrected. Preserve diagnostics, distinguish deliberate absence from invalid input, and add integration tests that assert no broad fallback.

#### READ-023 — Missing and package-based config edges disappear silently
- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Other
- **Location:** `glass-lint-project/src/tsconfig/mod.rs:339-355`

Package-based `extends` returns `None` by design, nonexistent relative parents are filtered out, and missing project references are skipped after `exists()` checks; none produces an unsupported/missing-edge diagnostic. The child then proceeds with defaults or partial inheritance, which can broaden or silently shrink membership. Resolve package extends according to TypeScript rules, or emit a typed unsupported diagnostic and fail closed; missing relative extends/references should always be diagnosed. Keep cycles separately classified because their recovery policy is already explicit.

#### READ-024 — Shared extends ancestors are reparsed and charged repeatedly
- **Severity:** Medium
- **Fix Complexity** High
- **Status:** ✅ Fixed
- **Category:** Complexity
- **Location:** `glass-lint-project/src/discovery.rs:159-258`

The reference graph deduplicates referenced configs, but every `build_effective_config` call creates a fresh extends chain and recursively rereads its ancestors. Multiple referenced projects sharing one base config therefore repeat I/O, JSONC stripping, parsing, merging, config-count charges, and byte charges, potentially exhausting a “config files” budget on fewer unique files. Introduce a traversal context that owns a canonical parsed-config cache, origin-aware effective-config cache, diagnostics, chain state, and budgets; this also removes the two `too_many_arguments` suppressions. Count unique canonical documents for count/byte budgets while retaining per-chain cycle detection.

#### READ-025 — Coarse Timeouts — REJECTED

#### READ-026 — Aggregate source bytes are admitted only after full reads
- **Severity:** Medium
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:505-565`

Each file is admitted and fully allocated before `record_source_bytes` checks the aggregate project limit; the offending file remains counted as admitted even though it is not analyzed. This contradicts the option's “reserved before parsing” contract and permits avoidable memory/I/O spikes at the boundary. Reserve from metadata before reading, limit the read to the remaining aggregate allowance, and reconcile actual length to handle file races. Publish file/byte metrics from accepted analyzed sources, while retaining the existing partial-report behavior for earlier files.

#### READ-027 — Resolver failures for bare requests are all classified as external
- **Severity:** High
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-project/src/resolver.rs:67-90`

Every non-builtin error for a bare request becomes `ResolverOutcome::External`, including malformed requests, permission/I/O failures, and unsupported resolver states. That converts operational ambiguity into positive package provenance instead of failing closed. Match resolver error variants explicitly: preserve the authored external fallback only for a deliberate “bare package not installed” not-found case, and map invalid or operational failures to typed `Unsupported` or a load error. Add tests for malformed scoped packages, unreadable metadata, invalid package manifests, and ordinary absent dependencies.

#### READ-028 — `SourceCorpus` resets shared budgets for each root
- **Severity:** Medium
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Architecture
- **Location:** `glass-lint-project/src/corpus.rs:136-193`

`discover_filtered` creates a new `ProjectResourceBudget` inside the roots loop, so `max_visited_entries` is enforced per root even though the budget type promises counters shared across all walks. Its one-hour deadline is ineffective because `check_deadline` is unused and `collect_files` receives `None`. Create one budget and one real configured deadline outside the loop, or remove deadline state from `ProjectResourceBudget` and state clearly that corpus discovery has no timeout. Canonicalize each supplied root before containment checks and test aggregate limits across overlapping and disjoint roots.

#### READ-029 — Filesystem admission repeats allocation and metadata work
- **Severity:** Medium
- **Fix Complexity** Low
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-project/src/options.rs:95-111`

Every support check allocates a lowercase filename and linearly scans suffixes; entry, explicit-tsconfig-file, and imported-target paths often call `is_file`/`exists` or `supports` before `classify`, which canonicalizes and checks again. On large discovery trees and import frontiers this is repeated hot-path allocation and syscall work, with TOCTOU windows between decisions. Pre-index normalized suffixes and perform ASCII-insensitive suffix checks without allocating, then give `SourceAdmission` one typed classify-or-missing operation that owns metadata, canonicalization, containment, exclusion, and extension policy. Preserve non-UTF-8 fail-closed behavior and declaration-file exclusions.

#### READ-030 — Custom source suffixes cannot select an explicit parser language
- **Severity:** Medium
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** API
- **Location:** `glass-lint-core/src/parse.rs:31-57`

Project options accept arbitrary source suffixes, but `SourceFile` always infers language from the six built-in extensions and defaults every unknown suffix to JavaScript. The documentation says callers with extensionless TypeScript must provide the language directly, yet no public `SourceFile` constructor permits it; enabling `.tsx` therefore admits a file that is parsed as JavaScript. Add an explicit-language constructor and make project extension policy map suffixes to a supported language, rejecting unsupported syntax families at option validation. Test extensionless TypeScript, `.tsx` policy, custom JavaScript suffixes, and declaration exclusions.

### Cross-crate API and cleanup

#### READ-031 — Resolver target newtypes do not enforce their documented semantics
- **Severity:** Medium
- **Fix Complexity** Medium
- **Status:** ✅ Fixed
- **Category:** Newtype
- **Location:** `glass-lint-core/src/project/types/input.rs:44-203`

`PackageSpecifier`, `BuiltinModuleName`, and `NormalizedOutsidePath` reject only trimmed emptiness; they accept surrounding whitespace, relative/package-invalid syntax, and non-normalized paths, and `normalize_result` does not revalidate external or builtin outcomes. Public virtual-project callers can therefore construct identities the types claim are impossible. Define the exact provider-neutral grammar for each type and enforce it in the public constructor; keep catalog knowledge out of core, but validate structure such as bare package roots, scoped package completeness, canonical builtin spelling, NULs, and normalized path form. Make unchecked construction private to the boundary that proves the invariant.

#### READ-032 — Dead state and suppressions obscure the real phase model
- **Severity:** Low
- **Fix Complexity** Low
- **Status:** ✅ Fixed
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/facts/state.rs:14-40`

`TraversalState.current_scope`, `SccPartition.dag`, `SemanticBudget::limit`, and several outcome fields are retained behind `allow(dead_code)` even though their owners never use them; `ProjectResourceBudget` similarly stores a deadline whose checker has no caller. These fields make the phase model look richer than it is and invite future code to depend on stale abstractions. Delete truly obsolete state and its suppressions, or connect each field to a documented invariant and test. Keep computed transient data local when it is needed only to derive another field, rather than storing it in the frozen owner.

## Systemic Themes

1. **Control semantics are duplicated.** Scope provenance, fact-time instance provenance, function effects, and object flow each model control edges differently. The strict-identity layer needs one shared control-region vocabulary and one conservative join policy, even if individual consumers retain specialized state.
2. **Budgets do not always bound actual work.** Map cloning, summary propagation, cache growth, and sparse vector resizing can do work disproportionate to the charged operation. Budgets should cover retained state and fan-out, not only loop iterations or emitted facts.
3. **Determinism is being purchased too early.** Several hot local indexes use `BTreeMap` even though order is observed only at freeze/report boundaries. Dense or hashed working state can remain deterministic by sorting once when it becomes externally observable.
4. **Semantic newtypes are present but underpowered.** IDs, paths, package names, and matcher chains have wrapper types, yet public raw constructors or permissive normalization still admit invalid states. The owner that proves an invariant should also own construction.
5. **Filesystem/config policy needs one traversal context.** Tsconfig caches, origin rebasing, diagnostics, canonical paths, deadlines, and resource counters currently travel through separate arguments and checks. A single project-owned context would simplify signatures and close several correctness gaps together.

## Open Questions

None remain. The audit resolves the relevant design decisions as follows:

- Conditional or exceptional assignments are **unknown unless all reaching definitions agree**; “may have executed” is not enough for strict identity.
- A tsconfig path remains relative to **the config that declared that field**, including inherited output-directory exclusions.
- Missing or unsupported config edges are **diagnosed and fail closed**; package extends should be implemented rather than silently ignored.
- Namespace star-export collisions remain **ambiguous**, independent of declaration or traversal order.
- Multiple function returns produce a call-result identity only when **all valid return candidates agree**; an invalid effect produces unknown.
- A flow fixed-point or cache capacity limit that is reached produces **typed incomplete status and no partial evidence**.
- `max_timeout_ms` means an **end-to-end cooperative deadline**. If that cannot be implemented, the public option must be renamed to describe its narrower scope.
- Bare-package not-found may remain authored external provenance, but malformed input and operational resolver errors are **unsupported**, not external.
- Raw IDs remain **store-owned**; serialization or cross-store translation must pass through a validating owner.

## Coverage

- Read the repository, core, datastructure, and project architecture documents plus testing and contribution guidance before review.
- Inspected every production Rust module under `glass-lint-core/src`, `glass-lint-datastructures/src`, and `glass-lint-project/src`, with targeted review of tests for control flow, identity, budgets, linking, tsconfig, path tries, and project loading.
- Traced the complete local pipeline from parse and TypeScript normalization through scope planning/collection, resolver/value interning, fact construction, effect collection, occurrence indexes, local/cross flow, linking, and report attachment.
- Traced filesystem loading from option validation through canonical admission, walks, tsconfig inheritance/references, source reads, resolution, frontier closure, linking, and reporting.
- Inspected the existing sampled profile and symbolized the matching binary sufficiently to corroborate lowering/`FactBuilder` priority; no claim depends only on that profile.
- Ran `cargo test -p glass-lint-datastructures -p glass-lint-core -p glass-lint-project`; all tests and doctests passed. Default-feature core tests emitted four warnings for serde-gated controlled-release test support, but no production warning.
