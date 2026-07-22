# Codebase Readability Audit

## Summary

This audit covers all Rust source in `glass-lint-core` and `glass-lint-project`, including unit and integration tests. The codebase has strong domain modeling in several areas—normalized project paths, dense semantic IDs, immutable local artifacts, deterministic collections, and fail-closed status—but its remaining readability and memory costs cluster around duplicated domain logic, raw values crossing validation boundaries, and clone-to-release-borrow workarounds.

The 28 findings below include 8 high-, 16 medium-, and 4 low-severity items. The phase-typed project and local-analysis boundaries, scope split, borrowed linked occurrence view, borrowed matcher projection, fact-ID ownership, extension normalization, resolution-cache ownership, exclusive timings, and leaked-fixture cleanup have been verified and removed from this report. The remaining work is ordered from foundational validation and ownership boundaries through semantic/flow abstractions, matcher composition, caching, and reporting cleanup.

## Findings

### READ-001 — Make analysis limits valid by construction

- **Severity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/limits.rs:5-82`

All six public raw `usize` fields admit zero and require repetitive validation that returns unstructured `String` errors. Private `NonZeroUsize`/domain budget fields with a typed `AnalysisLimitError` would remove invalid post-construction states and let downstream phases borrow trusted limits without defensive validation.

### READ-002 — Keep one canonical project-input admission algorithm

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/input.rs:34-142`

`ProjectInput::validate` and `validate_into_maps` independently normalize sources and resolutions, check duplicates/importers, and build the same maps; comments explicitly acknowledge the map-to-vector-to-map path. Make consuming validation produce one public validated, map-backed project type, with an explicit DTO conversion only for callers that truly need vectors.

### READ-003 — Do not erase the validated-options boundary with raw accessors

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-project/src/options.rs:81-90`, `glass-lint-project/src/options.rs:291-357`, `glass-lint-project/src/corpus.rs:75-105`, `glass-lint-project/src/loader.rs:213-222`

`ValidatedProjectLoadOptions` no longer uses `Deref`, but `ProjectLoader::options` still exposes the raw policy and `SourceCorpus::new_unchecked` recreates validated state with an `expect`. Keep validated fields behind explicit accessors and require the validated policy at every I/O boundary, removing raw and unchecked escape hatches.

### READ-004 — Preserve filesystem admission as a proof-carrying path

- **Severity:** High
- **Category:** Newtype
- **Location:** `glass-lint-project/src/admission.rs:18-190`, `glass-lint-project/src/loader.rs:296-450`, `glass-lint-project/src/discovery.rs:16-220`

`CanonicalProjectPath` and `AdmittedSourcePath` now exist at the admission boundary, but discovery results, `PathWorkQueue`, `AdmissionSet`, and loading APIs immediately convert them back to raw `PathBuf`/`Path`; discovery and loading can therefore re-canonicalize or re-admit the same target. Carry the proof types through discovery, queues, resolver targets, and source loading so one canonical allocation represents one admitted file.

### READ-005 — Compile tsconfig patterns once per configuration

- **Severity:** High
- **Category:** Newtype
- **Location:** `glass-lint-project/src/discovery.rs:173-195`, `glass-lint-project/src/discovery.rs:269-283`

Every discovered path recompiles every include and exclude glob, while `tsconfig_pattern_matches` also allocates normalized pattern strings for each comparison. A validated `TsconfigPatternSet` should normalize and compile patterns once when the effective config is built, then provide allocation-free borrowed matching during discovery.

### READ-006 — Model tsconfig loading with typed intermediate data

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/discovery.rs:105-156`, `glass-lint-project/src/discovery.rs:199-259`, `glass-lint-project/src/discovery.rs:309-325`

Discovery passes mutable `serde_json::Value`s through free functions that interpret string keys, recursively merge only selected subtrees, and share a visitation set across inheritance and references. A small `Tsconfig`, `TsconfigChain`, and `SourceSelection` model would give parsing/merging/reference traversal distinct phases, centralize supported semantics, and make unsupported or malformed fields explicit instead of silently disappearing through `Value` queries.

### READ-007 — Replace catch-all option messages with domain errors

- **Severity:** Medium
- **Category:** API
- **Location:** `glass-lint-project/src/error.rs:43-70`, `glass-lint-project/src/corpus.rs:103-119`, `glass-lint-project/src/discovery.rs:262-267`, `glass-lint-project/src/loader.rs:45-55`

`ProjectOptionError::Message(String)` represents a non-directory corpus root, tsconfig parse failures, and a core partial-load reason, conflating configuration, selection, parsing, and analysis outcomes and discarding structured context. Add dedicated variants (including config path and source error where available), and keep partial analysis reasons out of the options error hierarchy.

### READ-008 — Use a compact indexed artifact-cache key

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/local.rs:46-100`, `glass-lint-core/src/analysis/local.rs:136-201`

Each cache lookup holds a mutex while linearly comparing up to 64 keys containing full `SourceText`, `Environment`, and `AnalysisLimits`, and FIFO eviction shifts the vector with `remove(0)`. Use a precomputed semantic fingerprint/index outside the lock plus a map and queue (retaining collision verification if required by strict identity), so concurrent local analysis does not serialize on repeated full-source comparisons.

### READ-009 — Make the local executor consume an iterator and return errors

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/session.rs:24-32`, `glass-lint-core/src/project/session.rs:163-236`, `glass-lint-core/src/project/session.rs:614-664`

The executor now returns typed worker-panic errors and uses `VecDeque`, but its trait still accepts a materialized `Vec`, then repeatedly builds front-popped batches and worker chunks. An executor over an owned iterator/queue with bounded outstanding work would remove the intermediate batching path and make the session’s error handling fully direct.

### READ-010 — Unify the two parent-linked path interners

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/value/path.rs:14-228`, `glass-lint-core/src/analysis/flow/summary.rs:25-225`

`PathInterner` and `SummaryPathInterner` independently implement bounded IDs, parent/depth nodes, edge interning, prefix checks, first-segment removal, rebuilding, and owned segment extraction. Generalize the canonical interner or add a summary overlay that references frozen `PathId`s and allocates only novel joined paths; this removes duplicated algorithms and avoids copying every referenced fact path into a second trie.

### READ-011 — Centralize constant provenance and member evaluation

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/constants.rs:14-79`, `glass-lint-core/src/analysis/scope/query/constants.rs:19-80`, `glass-lint-core/src/analysis/resolution/mod.rs:130-165`

The collector, frozen scope graph, and resolver still duplicate the `BindingProvenance -> ConstValue` conversion and static array/object member evaluation. Put provenance conversion behind one helper accepting a name resolver and provide shared member behavior as a `Lookup` default/helper, leaving each adapter responsible only for its distinct shadowing and mutability policy.

### READ-012 — Share one destructuring projection walker

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/aliases.rs:17-116`

Declaration aliases and assignment aliases recursively walk the same `Pat::Ident`/object key-value/object shorthand structure, differing only in whether the resulting binding is inserted or recorded at a span. Extract a conservative pattern projection iterator/visitor that yields `(binding, projected_path)` and give the two callers their own sinks; this also centralizes the policy for rest/default/dynamic forms and name exhaustion.

### READ-013 — Give call arguments a stable effect-call identity

- **Severity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:51-92`, `glass-lint-core/src/analysis/flow/effect.rs:515-526`, `glass-lint-core/src/analysis/flow/effect.rs:545-578`

Every `EffectUse::CallArgument` stores a fact event plus index, and consumers linearly search `FunctionEffect.calls` by event to recover its argument. Allocate a dense `EffectCallId` when recording the call and have uses point directly to that slot; the call remains the single owner of event identity and arguments.

### READ-014 — Return a borrowed-or-owned call chain

- **Severity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:189-228`, `glass-lint-core/src/analysis/flow/projector/mod.rs:215-224`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:20-31`

`CallEffectRef::chain_owned` clones an existing interned `NamePath` for the common case because only the callee-name fallback requires construction. Return `Cow<'_, NamePath>` or a small `ResolvedNamePathRef` enum so direct/rooted/syntactic chains remain borrowed in local and cross-flow loops.

### READ-015 — Avoid cloning local statuses before propagation

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/model.rs:278-295`

`propagate_local_status` clones every module's complete `AnalysisStatus` into a temporary vector solely to mutate the project's status field. Split borrows of `modules` and `status`, or add an extension method that accepts borrowed module/status pairs, so status entries are cloned only when they become owned project entries.

### READ-016 — Avoid whole-table copy-on-write for control-flow snapshots

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:61-112`, `glass-lint-core/src/analysis/flow/projector/state.rs:172-184`, `glass-lint-core/src/analysis/flow/projector/state.rs:276-327`

`Arc<Vec<_>>` makes capture cheap, but the first alias/state mutation on each branch clones the entire sorted vector and insert/remove then shifts entries; joins allocate complete replacement tables. A checkpoint/rollback mutation log or persistent delta map would make branch state explicit while copying only changed bindings and requirements, which is especially valuable for deeply nested control flow.

### READ-017 — Separate flow evidence mutation from state inspection

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/evidence.rs:81-180`

Sink and helper projection clone `FlowId` slices, `FlowState`s, `CompiledObjectFlow`s, and whole helper summaries into temporary vectors solely so `&mut self` can emit evidence after immutable lookups. Split the projector into independently borrowed state/index/evidence fields, or make emission a helper that mutates only `FlowEvidence`, so ready states and matchers can remain borrowed throughout the hot path.

### READ-018 — Expose disjoint summary-table access instead of cloning calls

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:365-461`, `glass-lint-core/src/analysis/flow/table.rs:21-69`

Direct-sink and fixed-point propagation repeatedly clone each function's call list, collect function IDs, linearly search prior offsets, and materialize projected sinks before reacquiring a mutable caller. Extend `FunctionTable` with dense indexed snapshots and safe disjoint access (or compute one indexed delta round), allowing borrowed call slices and direct target/caller access without per-round clone-to-release-borrow staging.

### READ-019 — Use one linked-identity domain type

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/model.rs:55-73`, `glass-lint-core/src/analysis/matching/mod.rs:129-166`, `glass-lint-core/src/analysis/project/model.rs:498-509`

`ExportResolution` and `LinkedModuleIdentity` still mirror external, global, qualified, static-string, unknown, and ambiguous states, while their conversion continues to collapse `Ambiguous` into `Unknown`. Keep one authoritative linked identity (or a borrowed matcher view of it) so new variants and ambiguity policy cannot drift and projection does not rebuild equivalent maps.

### READ-020 — Factor event-index selection out of query execution

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/query.rs:27-72`, `glass-lint-core/src/analysis/matching/query.rs:175-342`

The shared helpers now cover exact, package, and merged bucket iteration, but `occurrences_for_event` still repeats family selection, key construction, masking, and dispatch across calls, member calls/reads, classes, and constructions. Introduce typed event-index views whose only variation is the relevant base/overlay family, then centralize identity dispatch once.

### READ-021 — Encapsulate occurrence-index family maintenance

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/matching/build.rs:19-44`, `glass-lint-core/src/analysis/matching/mod.rs:210-239`

Normalization and emptiness checks still manually enumerate every individual index, so adding an index requires updating several distant lists. Give call, member, construction, and literal index families their own `normalize`/`is_empty` behavior and have `OccurrenceIndexes` delegate to those cohesive owners.

### READ-022 — Compile matcher declarations into one compositional public model

- **Severity:** High
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/matcher/call.rs:8-149`, `glass-lint-core/src/api/rule/matcher/member.rs:11-168`, `glass-lint-core/src/api/rule/matcher/derived.rs:5-167`, `glass-lint-core/src/api/compiler/rule.rs:36-139`

Call, member-call, constructor, class, and member-read types repeat identity constructors, evidence formatting, sort keys, normalization, and argument convenience methods, while the compiler immediately lowers them into the more coherent `IdentityConstraint + EventPredicate + SubjectConstraint + constraints` model. Replace the parallel public families with a validated compositional pattern builder around those dimensions; a clean break avoids another wrapper layer and gives argument constraints one owner.

### READ-023 — Compile and retain one catalog representation

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/catalog.rs:34-136`, `glass-lint-core/src/api/compiler/rule.rs:250-333`

`RuleCatalog` retains full normalized matcher declarations alongside compiled plans; compilation first copies `rule.matchers()` into a `MatcherSet`, clones it again for normalization, and `combine` discards incoming compiled catalogs and recompiles every rule. Make catalog construction consume rule definitions into a single compiled rule record containing metadata plus query plan, so combining catalogs moves already-compiled entries and matcher declaration trees do not remain resident.

### READ-024 — Store related evidence once per rule result

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/linter.rs:338-380`

Project reporting groups related evidence by rule and then clones the entire related list into every finding for that rule, producing `findings × related` owned evidence. Represent rule-level related evidence once (or share an immutable evidence slice/`Arc<[Evidence]>`) and let findings reference it during serialization/report assembly.

### READ-025 — Build each project file report once

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/linter.rs:383-417`

`initialize_project_files` first creates an empty `FileReport` for every source, then reconstructs and replaces the same record for parse failures while separately cloning a `parse_paths` list. Consume diagnostics while creating the initial map and derive status input from that typed parse-failure collection instead of maintaining parallel representations.

### READ-026 — Share the `SourceLineIndex` constructor body

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/diagnostic.rs:251-277`

`new` and `from_text` duplicate line-start and checkpoint construction, differing only in whether source text is newly allocated or already shared. Convert to `SourceText` first and delegate both public constructors to one private implementation.

### READ-027 — Do not clone owned values merely to sort

- **Severity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/query.rs:100-103`, `glass-lint-core/src/lint/linter.rs:318-325`

Evidence sorting clones each symbol into the sort key and finding sorting clones each `RuleId`, even though both values can be compared by reference. Use `sort_by` with borrowed tuple components (or a genuinely cached compact key) to keep deterministic ordering without transient ownership churn.

### READ-028 — Remove the redundant selected-rule lookup

- **Severity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/model.rs:445-464`

Classification calls `rules.get(index)` to test for `None` and immediately performs the identical lookup in `let Some(rule)`. Keep only the binding lookup so the control flow expresses the actual guard once.

## Systemic Themes

- **Validated values still lose their proof.** Filesystem paths and project options cross back into raw types, and public project input still has separate vector and map validation paths. Proof-carrying values should remain intact through discovery, loading, and analysis.
- **Domain logic remains duplicated.** Path interners, constant evaluation, destructuring projections, occurrence-family maintenance, event dispatch, and tsconfig interpretation each have parallel implementations or manually synchronized lists.
- **Clone-to-release-borrow remains in downstream stages.** Flow projection, summary propagation, status propagation, and report assembly still copy data because broad mutable owners prevent disjoint borrowing or because intermediate models own data they could view.
- **Public and compiled matcher models remain parallel.** The internal query dimensions are already the more compositional abstraction; exposing a validated version would remove duplicated matcher-family knowledge and let catalogs retain only compiled plans.
- **Bounded work is not uniformly indexed or typed.** The artifact cache compares heavyweight keys under a mutex, and raw analysis limits plus iterator/batch boundaries leave resource ownership weaker than the surrounding phase APIs.

## Open Questions

No unresolved design questions remain. Clean public-API breaks are allowed, so the follow-up work should adopt these decisions:

- Make validated project input a distinct map-backed type and let `ProjectInput` remain the unvalidated DTO. If serialization is required, provide an explicit conversion at that boundary; do not preserve the map-to-vector-to-map path for compatibility.
- Use a deterministic fingerprint as the artifact-cache index, with exact source/environment/limits verification within collision buckets. Cryptographic hashing is unnecessary unless a later threat model requires it; strict identity remains enforced by verification.
- Treat tsconfig inheritance and reference cycles as structured discovery diagnostics, stop the cyclic branch, and do not silently admit files whose configuration ancestry is incomplete.
- Make the compiled matcher plan the execution source of truth. If callers need introspection, expose a deliberate read-only compiled-plan view rather than retaining a second full normalized declaration tree.

## Coverage

- Reviewed all 135 Rust files in `glass-lint-core` (38,288 lines), including parsing, configuration, public rule/matcher APIs, compilation, scope/provenance, resolution, semantic facts, value/name arenas, local and cross-module flow, project linking/projection, reporting, cache/session orchestration, and all unit/integration test modules.
- Reviewed all 10 Rust files in `glass-lint-project` (2,136 lines), including options, admission, walking, corpus loading, tsconfig discovery, resolver configuration/classification, loader orchestration, errors, and tests.
- Read the repository and owning-crate architecture documents plus `TESTING.md` and `CONTRIBUTING.md` before evaluating boundaries and public APIs. Re-verified the plan-related implementation with targeted core project tests and the project-crate test suite.
- This was a static readability/maintainability audit. No source code, tests, configuration, or documentation other than this report was changed, and runtime benchmarks were not used to rank memory-churn findings.
