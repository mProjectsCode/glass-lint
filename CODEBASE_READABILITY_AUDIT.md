# Codebase Readability Audit

## Summary

This report replaces the completed audit that previously occupied this file. It describes only issues present in the current tree; completed findings and implementation notes from the previous report were intentionally removed.

The scan found 16 maintainability issues. The most important theme is that several analysis boundaries have gained typed owners while still exchanging their contents through raw indices, positional tuples, parallel collections, or strings. That weakens the invariants promised by the surrounding APIs and makes changes to flow evidence, parse status, query construction, and adapter serialization harder to validate locally. The recommendations below preserve the repository's strict path-local identity, bounded and deterministic analysis, fail-closed behavior, and provider-neutral core boundary.

## Findings

### Core query and resolution APIs

#### READ-001 — Member-chain validation has multiple competing owners

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:177-211`; `glass-lint-core/src/api/rule/query/value.rs:87-115`; `glass-lint-core/src/api/rule/query/lifecycle.rs:334-412`; `glass-lint-datastructures/src/path/name_path.rs:130-172`

Query builders repeat the definition of a valid dotted chain, sometimes returning the original invalid value and sometimes a generic replacement string. `SymbolPath::from_chain` deliberately normalizes empty segments, so every caller that needs strict input must remember to validate before constructing the path; lifecycle sinks additionally retain a raw `String` beside the parsed path.

Introduce a core-owned validated member-chain value that validates once, retains the canonical display spelling when needed, and provides a `SymbolPath` view or conversion. Route query, value-matcher, composition, and lifecycle constructors through it so error variants and offending values remain consistent. Keep the generic datastructures path parser permissive if that behavior is useful elsewhere, and do not move rule-policy validation into the provider-neutral container merely to share code.

#### READ-002 — `Rc<ResolvedValue>` leaks cache ownership into fact construction

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:42-109`; `glass-lint-core/src/analysis/resolution/expression.rs:18-390`; `glass-lint-core/src/analysis/facts/calls/callee.rs:14-50`

The resolution cache stores every result behind `Rc`, almost every resolver entry point returns an `Rc`, and `ResolvedCallee::from_resolved` names that storage choice in its signature before cloning most of the record into a fact-owned structure. There is no shared mutable graph here; reference counting primarily works around resolver borrowing and makes ephemeral cache ownership part of unrelated analysis APIs.

Give the cache an arena-backed `ResolutionId`, or expose focused snapshot/accessor methods that keep cached records owned by the resolver while callers extract only the data they persist. Keep recursion guards and unknown-on-exhaustion behavior explicit, and avoid replacing `Rc` with `RefCell` or a lifetime web that makes fact traversal harder to use. Measure allocation and clone changes because provenance paths and bound arguments can be nontrivial.

### Core matching and flow state

#### READ-003 — Requirement and sink indices are interchangeable `usize` values

- **Severity:** High
- **Fix Complexity** High
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:147-266,323-366`; `glass-lint-core/src/analysis/flow/projector/history.rs:16-25,139-202`; `glass-lint-core/src/analysis/flow/projector/evidence.rs:106-125`; `glass-lint-core/src/analysis/flow/cross/propagation.rs:120-181`

`RequirementSet` hard-codes `usize` keys and is used for both lifecycle requirements and sinks. Flow-state operations, mutation-history variants, local evidence emission, and cross-file propagation all pass raw indices, so a requirement index can be supplied where a sink index is expected and still satisfy every type and bounds check.

Introduce bounded `RequirementIndex` and `SinkIndex` newtypes at flow compilation and parameterize the compact evidence set by the index type, or provide separate requirement- and sink-evidence owners over one internal implementation. Keep conversion to `usize` at the narrow vector-access boundary and retain the current 64-bit readiness mask, sorted evidence values, and deterministic iteration. Add tests that exercise the maximum supported index and restoration history for both domains independently.

#### READ-004 — Constrained matching relies on positional tuples and parallel vectors

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:33-79`; `glass-lint-core/src/analysis/matching/arguments/mod.rs:28-181`

Projection first converts `RuleIndex` to `usize`, then constrained matching expands roots into five- and six-element tuples, prepares paths in a parallel vector, zips the collections, and maintains another parallel occurrence vector for fallback scans. The alignment is correct only because several construction and iteration orders remain identical, and tuple positions obscure which borrowed matcher component is being used.

Create a named prepared-root record containing the typed rule index, identity, event, constraints, evidence descriptor, and prepared paths, with fallback occurrences stored on that record or in a named fallback result. Let methods on the record perform indexed and fallback evaluation, and bundle the stable evaluator inputs if that materially reduces the eight-argument coordinator. Preserve the two-phase indexed/fallback behavior, overlay semantics, operation accounting, and evidence order.

#### READ-005 — Project projection has parallel local and final outcome models

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:131-174,190-297,299-376`

`LocalProjectionOutcome` and `ProjectionOutcome` duplicate exhaustion, effect-observation, module, alternative, coalescing, fixed-point, and trace counters. `project_modules` mutates the local structure field by field, and `project_facts` manually maps and combines it with cross-file results, leaving the meaning of each aggregate spread across the coordinator.

Add an internal projection accumulator with named `record_local`, `record_cross`, and `finish` operations, or make the local result a value that knows how to combine with the cross result. Use a context value for stable inputs to the nine-argument entry point while keeping mutable evidence and trace ownership explicit. Preserve saturation rules, the distinction between observed and exhausted effects, module counts, and deterministic trace-head accounting.

#### READ-006 — Cross-flow evidence emission owns too many policies at once

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/evidence.rs:78-190,213-332`

`EmissionContext::emit` resolves source locations, assembles source/requirement/prior-sink/current-sink trace nodes, handles arena exhaustion, derives certainty, builds occurrences and classification evidence, performs deduplication, and updates metrics. It also labels prior events that satisfied sink clauses as `EvidenceRole::Requirement`, conflating their semantic role with the fact that only the completing sink is the terminal trace node and finding anchor.

Extract a trace-assembly owner that appends the ordered evidence roles and returns a typed complete/exhausted result, then let `ModuleEvidence` own witness deduplication and recording. Classify every event that satisfies a sink clause as `EvidenceRole::Sink`, while separately retaining the completing sink as the terminal node and finding anchor; cover both local and cross-file multi-sink traces. Preserve current ordering, certainty downgrade rules, deduplication keys, and bounded arena behavior.

### Core reporting and status boundaries

#### READ-007 — Parse failures are serialized to strings and classified again

- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/parse.rs:26-33,121-133`; `glass-lint-core/src/lint/report.rs:44-58,328-363`; `glass-lint-core/src/analysis/project/model.rs:393-403`; `glass-lint-core/src/analysis/lowering/status.rs:1-31`; `glass-lint-core/src/project/types/report/code.rs:8-55`

The parse layer already has typed diagnostic and failure concepts, but report assembly copies each diagnostic code into a `String`, passes that side map into the project model, and matches two string literals to recover `ParseFailureKind`; every other value silently becomes `Syntax`. This duplicates the naming table and makes a newly added parse diagnostic compile successfully while being assigned the wrong completion cause.

Carry a typed parse-failure cause alongside the presentation diagnostic, or add an exhaustive conversion between the relevant diagnostic kind and `ParseFailureKind` that can report unsupported values. Record the typed status before moving diagnostics into file reports so no string side channel is needed. Preserve the intentional separation between user-facing diagnostics and analysis completion, but centralize the mapping and test every parse resource limit plus ordinary syntax failure.

#### READ-008 — Finding assembly indexes evidence through raw coordinate pairs

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/report.rs:179-285`

`findings_for_capability` groups display ranges into `Vec<(usize, usize)>` pairs that index evidence and occurrence collections, copies those pairs through entries and parallel groups, then repeatedly dereferences them to build traces, truncation, and certainty. Range containment, evidence identity, aggregation policy, and final rendering are all embedded in one suppressed long function.

Represent an evidence-occurrence reference with named typed fields and introduce a finding-group owner that performs containment grouping and exposes aggregated traces, certainty, and truncation. Split group formation from final `Finding` construction so each phase has one level of abstraction. Retain the current sorted map, containment semantics, fallback trace behavior, trace deduplication, and stable finding order.

#### [x] READ-009 — `ReportCompletion` aggregation is duplicated at each consumer

- **Severity:** Medium
- **Fix Complexity** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/types/report/analysis_report.rs:5-11`; `glass-lint-core/src/project/report/mod.rs:55-81`; `glass-lint-harness/src/profile/types.rs:50-66,93-105,190-196`; `glass-lint-harness/src/profile/metrics.rs:70-94`; `glass-lint-harness/src/profile/runner.rs:350-365,526-540`

The enum that models complete versus partial reporting has no behavior, while core report merging and several profiling accumulators independently implement its monotone “partial wins” rule. This is a small policy, but its repetition makes omission easy whenever a new aggregate is introduced.

Put `combine`/`join` and `is_partial` behavior on `ReportCompletion`, and use it in core and harness accumulators; a small `FromIterator` implementation may also fit the aggregation sites. Keep CLI failure policy outside the enum because deciding whether partial analysis fails a command is not the same concern as combining states. Add truth-table tests on the owning type and retain one integration assertion for report merging.

Implemented `ReportCompletion::join` and `is_partial` as the single monotone aggregation policy, with an owning truth-table test. Core report merging and all profile workload, repetition, file, and run accumulators now use `join`, removing their duplicated partial-state branches while leaving CLI failure decisions at the CLI boundary.

### Harness APIs and adapter protocol

#### [x] READ-010 — Adapter response serialization and deserialization use different schemas

- **Severity:** High
- **Fix Complexity** Medium
- **Category:** API
- **Location:** `glass-lint-harness/src/types/mod.rs:336-461,463-581`

`AdapterResponse` serializes `Vec<Finding>` using the core report representation, while its manual deserializer declares a second set of proxy structs that shadow finding locations, traces, steps, rule IDs, severity, certainty, and truncation. A core serde change can therefore alter emitted adapter JSON without changing or even compiling the handwritten input schema, and the large module combines protocol DTOs with domain validation and case models.

Define an explicit private adapter finding/response DTO that derives serialization and deserialization symmetrically, then convert it to core `Finding` through a validating `TryFrom` boundary. Keep path, rule-ID, nonempty-trace, terminal-location, and project-resolution validation in the harness rather than exposing weak constructors from core. Because this is not a public compatibility contract, update both ends in one change without a versioning or migration layer; add JSON round-trip and shape tests and move protocol types into a cohesive submodule.

Replaced the handwritten deserializer proxy set with one private symmetric DTO family that derives both serde directions. DTO findings convert through a validating `TryFrom` boundary that preserves path/rule validation, nonempty evidence, and terminal-location checks; a JSON round-trip assertion now covers the shared wire shape without changing protocol versioning.

#### [x] READ-011 — Harness model validation exposes message strings as its error API

- **Severity:** Low
- **Fix Complexity** Medium
- **Category:** API
- **Location:** `glass-lint-harness/src/types/mod.rs:38-88,149-247,277-296,420-448`

Public case, expectation, and adapter-conversion constructors return `Result<_, String>`, and some conversions erase typed core errors with `to_string()`. Callers can only distinguish validation failures by matching prose, while unrelated model and protocol errors accumulate in the same string-shaped boundary.

Introduce focused error enums for case construction, expectations, and adapter conversion, with source errors retained where useful and `Display` implementations for CLI output. Avoid one catch-all harness error that merely recreates the string bucket as a large enum. Keep clap value parsers or other framework adapters free to convert the typed errors to strings at their outermost boundary.

Added focused `CaseError`, `ExpectationError`, `FindingExpectationError`, and `AdapterConversionError` types with `Display`/`Error` implementations. Core rule/path/target validation errors remain typed as sources, while case loading and adapter orchestration convert them to `anyhow` only at their outer boundary; no catch-all harness error was introduced.

### Project and provider composition

#### [x] READ-012 — Provider catalog topology is repeated across layers

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-js/src/lib.rs:13-78`; `glass-lint-obsidian/src/lib.rs:53-66`; `glass-lint-harness/src/adapters.rs:75-83`; `glass-lint-cli/src/rules_doc.rs:8-37`

The ordered relationships among JavaScript, browser, Node, Electron, and Obsidian catalogs are reconstructed in provider configuration, adapter selection, and rule-document generation. The lists already differ in incidental ordering, so adding or renaming a catalog requires coordinated edits that are not enforced by one owning API.

Let the JavaScript provider expose a typed ordered catalog bundle or target composition, and let the Obsidian provider extend that bundle with its own catalog. Have harness and documentation consumers iterate named entries from those owners while preserving access to isolated catalogs for tests and selective configuration. Keep provider names, target policy, and profiles out of core, and add an ordering/uniqueness contract test where the topology is defined.

Added provider-owned `JavaScriptCatalogBundle` target composition and an `ObsidianCatalogBundle` that extends it in the canonical renderer order. JavaScript/Obsidian configs, the built-in adapter, and rule-document generation now consume those bundles; isolated catalog functions remain available, and the Obsidian provider has an ordering contract test.

#### [x] READ-013 — Tsconfig inheritance accepts a representable invalid parent state

- **Severity:** Medium
- **Fix Complexity** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-project/src/tsconfig/selection.rs:24-29,92-141`

`merge_selection` accepts `parent: Option<MergedSelection>` and `parent_dir: Option<&Path>` independently, then panics if a parent selection arrives without its directory. The function also exposes every intermediate merged field publicly even though the type is documented as a construction-only value consumed by compilation.

Bundle the selection and directory in an optional `ParentSelection` value, or make rebased inheritance a method whose receiver necessarily supplies its origin directory. Make merged fields private and expose only the named consumption/access operations needed by the compiler. Preserve the current move-only merge, path rebasing, fail-closed invalid-field handling, and default exclusion behavior.

Introduced an opaque `ParentSelection` that owns each inherited selection together with its origin directory, eliminating the independently optional directory and its panic. `MergedSelection` fields are private and consumed through a named `into_parts` operation; merge callers now pass the bundled parent while preserving move-only rebasing, fail-closed invalid fields, and default exclusions.

### Testing and public-surface hygiene

#### READ-014 — The logical/physical reference oracle excludes lifecycle plans

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Testing
- **Location:** `glass-lint-core/src/api/compiler/reference.rs:1-8,119-135,245-266`; `glass-lint-core/src/api/compiler/tests/reference.rs:1-470`

The compiler's equivalence oracle explicitly panics for lifecycle roots, so the most stateful query family cannot be compared through the same normalized-versus-physical semantic model used for event roots. Lifecycle behavior has other tests, but planner changes can bypass this independent equivalence check and the oracle's “supported subset” boundary is enforced by runtime panic.

Extend the synthetic relation model with the minimum object, path, requirement, sink, and completeness state needed for representative lifecycle witnesses, or build a separate lifecycle oracle if combining the models would make either unreadable. If support remains intentionally partial, return a typed unsupported result and make tests select supported roots explicitly rather than panic. Include aliases, correlated paths, unknown alternatives, completion order, and exhausted evidence while keeping the oracle small enough to remain independently understandable.

#### [x] READ-015 — Datastructures exposes two public paths to nearly every type

- **Severity:** Low
- **Fix Complexity** Low
- **Category:** API
- **Location:** `glass-lint-datastructures/src/lib.rs:17-39`

All implementation modules are public while their principal types are also re-exported from the crate root, creating parallel public import paths and making module organization part of the compatibility surface. Workspace consumers use the root facade, so the duplicated surface currently provides little demonstrated value.

Make implementation modules private, retain the crate root as the canonical facade, and re-export intended public constants deliberately. Update any downstream direct-path call sites in the same breaking change rather than keeping a compatibility layer. Add a small public-surface check so future modules are not exposed accidentally.

Made every datastructures implementation module private and kept the crate-root re-exports as the sole public facade. A workspace-wide call-site search found no downstream module-path imports to migrate, and the existing crate consumers continue to compile through the canonical exports.

#### [x] READ-016 — Narrow dead-code exceptions conceal removable production paths

- **Severity:** Low
- **Fix Complexity** Low
- **Category:** Other
- **Location:** `glass-lint-harness/src/report.rs:13-22`; `glass-lint-project/src/loader.rs:761-793`; `glass-lint-core/src/analysis/flow/projector/history.rs:55-58`; `glass-lint-core/src/analysis/flow/projector/state.rs:274-276`; `glass-lint-js/src/rules/node/archive_compression/mod.rs:8-44`

The tree still contains a dead `active_tool_runs` report helper, an internal-target enqueue method whose sole caller always supplies `Some`, test-only mutation counters compiled into production behind allowances, and a stale `too_many_lines` suppression on a short declarative rule. Each is small, but together they make it harder to tell whether an exception denotes a real architectural constraint or leftover migration scaffolding.

Delete the unused report helper, remove the needless optional target branch, gate pure test introspection with `cfg(test)`, and remove stale lint suppressions after rerunning the workspace lints. Keep any instrumentation that feeds production diagnostics or profiling, and validate call sites before deleting fields rather than treating an allowance alone as proof of dead state. Prefer an explicit test-support module when several internal counters need privileged access.

Removed the unused harness report iterator and the optional internal-target parameter, whose sole caller always provided a path. Mutation-log counters are now compiled only for tests, while production exhaustion accounting remains intact, and the obsolete archive-rule line-count suppression is gone.

## Systemic Themes

- Several recently introduced owners still expose their semantics through primitive indices or positional storage. The next readability gains will come from carrying domain types through phase boundaries, not from adding more coordinator helpers around raw collections.
- Boundedness and fail-closed behavior are generally documented well, but their causes and stop states are sometimes converted into strings, booleans, or field-by-field aggregates. Typed terminal and combination operations would make those guarantees easier to review.
- Provider ownership is clean at the crate boundary, yet target composition is duplicated by consumers. Keeping catalog topology with the provider crates would reduce drift without putting policy into core.
- The harness is doing valuable validation at external boundaries, but its wire schema and error model are less explicit than the core models they protect.

## Resolved Decisions

- Harness adapter JSON is an internal protocol, not a public compatibility contract. READ-010 therefore calls for one private symmetric DTO and a coordinated one-step migration, without version negotiation or compatibility scaffolding.
- The `glass-lint-datastructures` crate root is the intended public facade. READ-015 therefore recommends making implementation modules private and updating any direct-path consumers rather than preserving both import surfaces.
- Prior lifecycle sink events are semantically sinks, even when a later sink is the terminal trace node and finding anchor. READ-006 therefore recommends `EvidenceRole::Sink` for every satisfied sink clause and keeps terminal position as a separate trace property.

## Open Questions

None at this time.

## Coverage

The audit reviewed the workspace architecture and testing guidance; mapped all Rust crates and large modules; inspected core query construction, parsing, resolution, facts, matching, local and cross-file flow, projection, reporting, and compiler tests; and sampled project discovery/tsconfig, provider composition, harness protocol/profile code, CLI documentation, output, and datastructures APIs. Call-site searches were used to validate ownership and dead-code claims. The workspace passed an all-targets, all-features Clippy run with additional complexity, argument-count, large-value, and pass-by-value lints, followed by the full `make ci` gate (workspace checks, tests, e2e and provider rule verification, generated-rule validation, and examples).
