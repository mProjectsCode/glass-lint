# Codebase Readability Audit

## Summary

This is a read-only audit of the complete Rust workspace. It covers 85,340
Rust lines in 481 files, reviewed in nine natural chunks of approximately
10,000 lines. READ-001 through READ-011 are completed historical findings and
are not repeated as open findings.

The current audit consolidates 25 residual findings, READ-012 through
READ-036. They identify concrete encapsulation, simplification, and
deduplication opportunities with named owners, representative call sites,
deletion or consolidation targets, and behavior guardrails. No source, test,
configuration, dependency, or generated documentation files were changed.

## Findings

### Core flow, facts, and matching

#### [ ] READ-012 — Call-effect references are constructed outside their owner

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:87-143`; raw
  constructions in `flow/cross/propagation.rs:108,145`,
  `flow/summary/sink.rs:233-236`, `flow/projector/transfer.rs:24-27`, and
  `flow/projector/mod.rs:607-610`.

`CallEffectRef` pairs a frozen fact stream with a call fact ID and owns all
call-fact interpretation methods. `EffectCall::as_ref` already provides the
owner-side constructor, but five production paths reconstruct the same
two-field record directly. That repeats the representation contract and
leaves construction able to bypass any future validation or lifetime-facing
operation.

**Recommendation:** Add a narrow factory on the owning stream/effect boundary,
such as `FactStream::call_effect(event)`, or route all callers through an
owner method that preserves the current `Option` behavior for invalid fact
IDs. Keep the stream borrow and event identity separate; this is a construction
boundary, not a reason to merge effect events with semantic facts.

**Fix Applied:** None so far.

#### [ ] READ-013 — Shared artifact types expose their `Arc` storage

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/local.rs:86-114,216-237`;
  `glass-lint-core/src/analysis/lowering/mod.rs:93-113`;
  `glass-lint-core/src/project/session/artifacts.rs:92-102,160-167`.

`LocatedSourceContext::lines`, `LoweredSource::semantic`, and
`SharedSemanticArtifact::{semantic,source_index}` return `&Arc<T>` even though
their consumers need either `&T` or an explicit cloned handle. This leaks the
cache/reference-counting representation into the lowering and project-session
boundary; `requests_with_ids` currently relies on implicit dereferencing, and
cache transfer separately clones the exposed `Arc`.

**Recommendation:** Return `&SourceLineIndex` and `&SemanticArtifact` for
borrowed queries, and provide explicitly named clone/transfer operations for
cache handoff, such as `clone_semantic`, `clone_source_index`, or an owning
`into_shared`. Retain `Arc` internally because cached semantic artifacts are
intentionally shared; preserve path-local source context and collision-checked
cache identity.

**Fix Applied:** None so far.

#### [x] READ-014 — Module call occurrences are written to two indexes by hand

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/build.rs:134-142` and
  `:172-180`.

Both the `ModuleExport` call-provenance branch and the `ModuleNamespace`
member-provenance branch construct the same `ModuleExportKey` and `Occurrence`,
then call `record_module_call` on both `call_indexes` and `members`. The two
indexes are intentionally distinct, but the synchronization rule is duplicated
in the fact builder.

**Recommendation:** Give the owning index aggregate one operation such as
`record_module_call(key, occurrence)` that records the occurrence in both
sub-indexes, or a small shared helper that receives the already-built key and
occurrence. Preserve the distinction between call and member indexes, their
overlay behavior, and deterministic occurrence ordering.

**Fix Applied:** Added an `OccurrenceIndexes::record_module_call` operation so
call and member module indexes are updated together, and extended the stream
projection test to assert both destinations. Verified with `make fmt && make ci`.

#### [ ] READ-015 — Export declaration collection traverses each variable pattern twice

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/interface/exports.rs:51-72`;
  `glass-lint-core/src/analysis/facts/interface/mod.rs:34-43`.

For every exported variable declarator, `record_export_decl` calls
`record_pattern_locals`, which collects all pattern bindings, then allocates a
second `BTreeSet` and calls `collect_pat_bindings` again before producing
export metadata. The same ordered binding set drives both local registration
and export recording, so the duplicate traversal and allocation can drift if
pattern handling changes.

**Recommendation:** Collect once in `ModuleInterfaceBuilder` and use the
result for local registration and export metadata, or provide one builder
operation that records both. Preserve deduplication, source-independent
deterministic order, and the current restriction that function/static export
metadata is added only for simple identifier patterns.

**Fix Applied:** None so far.

#### [ ] READ-016 — Internal derived-flow records retain broader-than-needed visibility

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:291-300,522-527`;
  `glass-lint-core/src/analysis/flow/summary/summaries.rs:19-26`;
  `glass-lint-core/src/analysis/flow/summary/store.rs:5-9,38-42`.

`FunctionEffect`, `FunctionEffects`, `FunctionSummaries`, `SummaryPathId`, and
`SummaryPathStore` are declared `pub`, while their parent analysis modules are
private and their production methods are restricted to `crate::analysis` or
narrower. The declarations therefore advertise a public Rust surface without
providing an external API, and make later visibility review less reliable.

**Recommendation:** Narrow each declaration to the smallest boundary required
by its actual callers (`pub(in crate::analysis)`,
`pub(in crate::analysis::flow)`, or `pub(super)`). Verify test-module access as
part of the migration and retain the existing frozen/overlay path distinction
and effect lifecycle.

**Fix Applied:** None so far.

### Core project model and scope analysis

#### [ ] READ-017 — Project evidence assembly performs two lookups for one projection

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:516-552`.

`ProjectMatcherModel::evidence_for` looks up the same module projection once
to produce indexed/overlay evidence and a second time to obtain projected rule
evidence. It then owns the merge and normalization logic even though
`ProjectModuleProjection` is the record that owns all three evidence sources.
The repeated lookup is small but makes the merge contract easy to change in
one branch and not the other.

**Recommendation:** Fetch the projection once and let it own a single
`evidence_for` operation, or have the model pass one borrowed projection through
a helper that performs the merge. Preserve selected-rule short-circuiting,
base-before-projected evidence order, overlay matching, module-local name
tables, and the existing bounded normalization.

**Fix Applied:** None so far.

#### [ ] READ-018 — Project semantic queries repeatedly traverse module storage

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/model.rs:350-405`.

`effect`, `module_names`, `module_fact_stream`, `fact`, `source_call_result`,
and `fact_location` independently reach through `self.modules.get(...)` and
then through `ProjectModule::local()`. The model already owns a semantic
`module` accessor, so these methods duplicate the storage traversal and make
the map representation part of every query implementation.

**Recommendation:** Add one private local-artifact accessor, or use the
existing semantic module operation consistently, and implement the queries
against that owner. Keep `Option`/unknown fallbacks exactly as they are,
especially `source_call_result` returning `ValueId::UNKNOWN` for missing or
non-call facts and `fact_location` retaining the module path and source-range
checks.

**Fix Applied:** None so far.

#### [ ] READ-019 — Module-interface accessors leak `SmolStr` and `String` storage

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/module.rs:137-180,303-323`.

The semantic module model returns `Option<&SmolStr>` from
`ImportedBinding::imported`, `&SmolStr` from `ModuleRequest::specifier`,
`(&SmolStr, &ModuleExport)` from `ModuleInterface::exports`, and
`Option<&String>` from `ModuleInterface::static_string`. The first three are
used by identity and export-linking paths that clone names into semantic keys.
Replacing those borrows with `&str` would require reconstructing `SmolStr`;
that can allocate for values longer than its inline capacity, whereas
`SmolStr::clone` is allocation-free for inline values and cheaply shares heap
storage. Only the `String` wrapper on `static_string` is an unnecessary view
leak.

**Recommendation:** Keep `&SmolStr` on the clone-bearing internal accessors
and change only `static_string` to return `Option<&str>`. Preserve the
internal compact storage, deterministic `BTreeMap` order, and the distinction
between unresolved and unknown exports. Do not introduce a semantic wrapper or
an additional compatibility accessor without a measured performance need.

**Fix Applied:** None so far.

#### [ ] READ-020 — Collector visibility lookup is implemented twice with different result shapes

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/assignments.rs:96-135`.

`visible_binding` and `visible_binding_scope` each walk the collector's scope
stack from the innermost scope outward, checking the assignment environment
before lexical scope bindings. The first returns preferred provenance or the
unknown sentinel; the second returns only the matching scope. Keeping the
search policy in two loops risks shadowing, assignment, or path-state changes
being applied to only one query. `constants.rs` already needs both results in
one semantic operation.

**Recommendation:** Add one internal lookup operation that returns the matching
scope and the assignment/declaration source, then derive the provenance and
scope-only views from it. Preserve assignment precedence, preferred-witness
selection, the unknown fallback, and the current no-binding result.

**Fix Applied:** None so far.

#### [ ] READ-021 — Collector returned-object provenance duplicates call and optional-call resolution

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/provenance.rs:222-260`.

The `Expr::Call` and `Expr::OptChain` arms perform the same bind exclusion,
callee classification, rooted-member/name resolution, and `ReturnedObject`
construction. Only the AST wrapper differs. This is a semantic branch where
duplicated code can accidentally make optional calls less strict than ordinary
calls.

**Recommendation:** Extract one helper that accepts the normalized callee
expression and keeps the `.bind` rejection, then have both arms delegate to it.
Keep the recursive identifier/member/paren/sequence behavior, path-root check,
and fail-closed handling of non-expression callees unchanged.

**Fix Applied:** None so far.

#### [ ] READ-022 — Frozen returned-object source repeats ordinary and optional-call branches

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/query/provenance/object.rs:106-121`.

`FrozenScopeGraph::returned_object_source` repeats the same `rooted_expr_chain`
and non-root check for `Expr::Call` and an optional call. The collector has the
analogous duplication in READ-021, but this frozen query has a separate return
type and lifecycle and should retain its own owner-level helper rather than
merge collector and frozen state.

**Recommendation:** Normalize the two call forms to one callee helper and
preserve the strict non-root requirement, recursive member handling, and
`None` behavior for unsupported optional-chain bases. Keep this helper within
the frozen-query owner; do not combine it with the collector helper merely
because the AST branches look alike.

**Fix Applied:** None so far.

#### [ ] READ-023 — Assignment recording repeats key/version/write bookkeeping

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/assignments.rs:36-94`.

`record_assignment_value` and `record_join_assignment` independently create a
`ScopedName` for the version counter, increment and wrap it in `BindingVersion`,
then create the same `ScopedName` again for `assignment_writes`. Their
environment update differs, but the binding identity/version transition does
not.

**Recommendation:** Centralize the common transition in a collector helper
that returns the `ScopedName` and next `BindingVersion`, or accepts a closure
for the differing environment operation. Keep joined alternatives, unknown
joins, saturation, and the existing write-set checkpoint behavior intact.

**Fix Applied:** None so far.

### Core API, limits, and session

#### [ ] READ-024 — Query variable inspection repeatedly traverses the expression tree

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/expression.rs:97-148`.

`vars`, `contains_var`, and `binding_vars` all invoke the same recursive
`walk_vars` traversal. The first collects every occurrence, the second scans
all occurrences without short-circuiting after a match, and the third filters
the same role stream. The repeated traversal is especially visible when
validation asks several related questions about a query expression.

**Recommendation:** Keep `walk_vars` as the single semantic traversal and add
focused owner operations that can short-circuit (`contains_var`) or collect a
combined summary once when multiple views are needed. Preserve occurrence
order for `vars`, role distinctions between binding/reference variables, and
lifecycle source handling.

**Fix Applied:** None so far.

#### [ ] READ-025 — Argument convenience builders repeat index validation

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/constructors.rs:275-357`.

`with_arg`, `with_arg_static_string`, `with_arg_static_strings`,
`with_arg_static_string_contains`, `with_arg_object_property_value`, and
`with_arg_object_keys` each perform the same `MAX_ARGUMENT_INDEX` check before
constructing an `ArgumentIndex`. The convenience methods then delegate to
`with_arg`, which repeats the check. A limit change or diagnostic adjustment
can therefore be applied inconsistently.

**Recommendation:** Centralize checked index conversion in one private helper
and let `with_arg` and all convenience methods use it, or let convenience
methods delegate without prechecking after the helper has been made
authoritative. Preserve the current error variant, `usize` input API, and
bounded constraint-builder behavior.

**Fix Applied:** None so far.

#### [ ] READ-026 — Analysis-limit accessors and setters repeat a field table

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/limits.rs:135-203,206-246`.

`AnalysisLimits` contains seven validated fields, then spells out seven
getters, seven checked `with_*` setters, and seven test-only setters. Each
method repeats the same `PositiveLimit` access or conversion and the matching
`AnalysisLimitError` variant. The repetition is a maintenance surface for
adding a limit or changing validation behavior.

**Recommendation:** Centralize the field operation through a small typed helper
or a narrowly scoped macro/table that still names each public method and error
variant at the API boundary. Preserve the distinct error values, serde
representation, and the invariant that every stored limit is positive; do not
replace the validated newtype with raw `usize` fields.

**Fix Applied:** None so far.

#### [ ] READ-027 — Session entry points repeat source admission loops

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/session/mod.rs:262-270,348-375`.

`analyze_sources`, `analyze_sources_controlled`, and
`analyze_sources_counted` all iterate the supplied sources and call
`admit_normalized_source` before dispatching to the corresponding executor.
The controlled and counted variants are test hooks, but their repeated phase
transition can diverge from the production path when admission rules change.

**Recommendation:** Add one private `admit_sources` operation returning the
normalized collection state, then let each execution mode call its own
analysis dispatcher. Keep the consuming phase boundary, duplicate-path errors,
and worker/order semantics unchanged.

**Fix Applied:** None so far.

#### [ ] READ-028 — Project resolution requests expose `SmolStr` storage

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/input.rs:337-400`.

`ResolutionRequest::specifier` returns `&SmolStr`, while the request type's
semantic role is a module-specifier string view. Callers compare it, pass it to
string predicates, or explicitly clone it for owned resolution keys. The
concrete small-string representation is therefore part of a public project
input API without being needed by callers.

**Recommendation:** Retain `&SmolStr` for the borrowed public accessor: it
supports string comparisons through the existing view while preserving the
cheapest ownership path for callers that need to clone a specifier. A blanket
conversion to `&str` would make `SmolStr::new(value)` the ownership path and
can regress long-specifier allocations. Revisit this boundary only with a
benchmark showing that representation decoupling outweighs the clone cost;
do not add a second speculative accessor now. Preserve normalized keys,
specifier equality, and deterministic sorting behavior.

**Fix Applied:** None so far.

#### [ ] READ-029 — Environment global registration repeats validated insertion

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/environment.rs:145-203`.

`add_global`, `add_global_object`, and
`add_global_object_with_members` each validate the root identifier and then
mutate `global_bindings`; the object variants additionally install a global
object description. The shared validation and root-binding mutation are
duplicated around the policy-specific member handling.

**Recommendation:** Introduce one private registration helper that accepts the
already-validated root and object-member policy, then keep the three public
methods as explicit policy adapters. Preserve copy-on-write `Arc` behavior,
configured-versus-restricted precedence, and fail-closed identifier
validation.

**Fix Applied:** None so far.

### Datastructures

#### [x] READ-030 — Name interning has an unchecked exhausted-index path

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-datastructures/src/name.rs:41-61`.

`NameTable::intern` handles a newly inserted index with a fallible `u32`
conversion and returns `NameExhausted`, but a repeated name converts the
existing index with `expect("index fits in u32")`. A custom table whose index
space is exhausted can therefore panic on a path that otherwise promises a
typed exhaustion error. The capacity invariant is split between the table
constructor and two different insertion branches.

**Recommendation:** Make the existing-name conversion use the same typed
failure path as the new insertion branch, or validate the configured maximum
at construction and retain a checked conversion at lookup. Preserve stable
IDs, insertion-order semantics, and the sticky exhaustion flag; do not silently
alias an unrepresentable index.

**Fix Applied:** Reused the typed `u32` conversion failure path for existing
names, preserving sticky exhaustion and returning `NameExhausted` instead of
panicking. Verified with `cargo test -p glass-lint-datastructures`.

#### [ ] READ-031 — Dense-table disjoint lookup repeatedly converts the same IDs

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Conversion
- **Location:** `glass-lint-datastructures/src/table.rs:100-118`.

`IndexTable::get_disjoint` converts `read` and `write` to `u32` repeatedly for
the equality test and then for each index conversion. The method already owns
the alias and bounds policy, so repeated generic-ID conversion obscures the
actual split-at ordering and makes future conversion changes harder to audit.

**Recommendation:** Convert each ID once into local raw/index values, then
perform the equality, bounds, and split-at branches on those values. Preserve
the `None` alias result, the `(None, None)` out-of-current-storage result, and
the two mutable borrow layouts.

**Fix Applied:** None so far.

### Harness and provider tooling

#### [ ] READ-032 — Harness expectation handling repeats required/forbidden traversal

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-harness/src/types/case.rs:234-245` and
  `glass-lint-harness/src/runner.rs:131-180`.

`ToolExpectation::qualify_for_file` maps the required and forbidden vectors
through the same qualification operation separately. `compare` then repeats
the same finding-count traversal for required and forbidden expectations before
performing a third required/forbidden membership traversal for unexpected
findings. The policy categories are intentionally different, but the matching
and counting mechanics are duplicated.

**Recommendation:** Add owner helpers for qualifying both expectation lists and
for counting a single expectation against findings, then keep required/forbidden
policy and error wording at the caller. Preserve exact-count versus
at-least-one semantics, forbidden diagnostics, and the rule/path/location/
certainty predicate order.

**Fix Applied:** None so far.

#### [x] READ-033 — Adapter-specific linter construction bypasses the harness factory

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-harness/src/builtins.rs:22-40`;
  `glass-lint-harness/src/adapters.rs:75-103`;
  `glass-lint-harness/src/profile/runner/support.rs:55-105`.

The harness already owns `builtins::linter(provider, profile)`, but
`configured_linter` reconstructs the Obsidian environment/catalog bundle and
baseline selection itself, while profile setup constructs a built-in linter and
then rebuilds a second `LinterConfig` from its catalog/environment to apply
explicit rule overrides. These paths duplicate provider selection and make
catalog/environment defaults able to drift between adapters and profiling.

**Recommendation:** Extend the harness factory with one explicit-rule operation,
for example `linter_with_selection` or `linter_for_rules`, and let both callers
use it. Keep the provider profile baseline, `RuleBaseline::None` override
semantics, provider-prefix validation, and the existing provider-specific
catalog composition owned by the harness boundary.

**Fix Applied:** Added a harness-owned explicit-rule factory that validates
target catalog namespaces and reused it from built-in adapters and profiling;
provider catalog/environment composition is now centralized. Verified with
`make fmt && make ci`.

### Project loading and output

#### [ ] READ-034 — Resolution cache checks the same key twice before returning a hit

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-project/src/loader_phases.rs:60-85`.

`ResolutionCache::resolve_or_get` calls `contains_key` and then `get` for the
same occurrence key, with a debug assertion and typed error for the impossible
intermediate state. The map owns the cache invariant, so the two-look-up path
adds control flow without providing a meaningful recovery and obscures the
separate semantic-specifier cache branch.

**Recommendation:** Replace the pair with one `if let Some(outcome) =
by_key.get(...)` lookup, then retain the specifier-key cache and final
occurrence-key insertion. Keep the returned reference lifetime, `did_resolve`
metric, and deterministic request-key behavior unchanged.

**Fix Applied:** None so far.

#### [ ] READ-035 — Discovery checks extension support before a classifier that checks it again

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-project/src/discovery.rs:230-251,291-322`;
  `glass-lint-project/src/admission.rs:183-200`.

Both entry selection and explicit tsconfig-file selection call
`admission.supports(path)` and then call `admission.classify(path)`, whose
authoritative policy sequence canonicalizes, checks containment, exclusion,
and support again. This duplicates the extension-policy decision at a phase
boundary that explicitly documents `classify` as the one complete admission
operation.

**Recommendation:** Let `classify` own the support decision and adapt its
`PathAdmission` result at the callers, or add an outcome operation that carries
the selection-specific error context. Preserve the distinct entry-selection
errors, fail-closed unsupported behavior, containment checks, and tsconfig
source ordering.

**Fix Applied:** None so far.

#### [ ] READ-036 — Pretty-report constructors duplicate the renderer state layout

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-output/src/report/types.rs:85-120`.

`PrettyReport::new` and `new_with_cache` initialize the same report, filename,
source, options, line-start, and cache fields. The only difference is whether
the constructor creates or clones the shared `LineCache`; duplicated field
lists make future renderer state additions easy to apply to only one path.

**Recommendation:** Delegate both constructors to one private constructor that
accepts a cache handle, keeping `new` responsible for allocating the per-file
cache and `new_with_cache` responsible for sharing it across evidence rows.
Preserve line-cache identity and source/line lifetime invariants.

**Fix Applied:** None so far.

## Systemic Themes

1. Several phase owners have already been migrated successfully, but residual
   callers still reach through a semantic owner to construct or reinterpret
   its internal record.
2. The most reliable simplifications preserve separate domains while
   centralizing repeated operations: collector versus frozen scope state,
   local versus linked project identity, and provider policy versus harness
   orchestration must remain distinct.
3. Representation leaks are concentrated in borrowed `Arc`, `SmolStr`, and
   `String` accessors. Returning semantic views while making ownership clones
   explicit would reduce coupling without changing storage.
4. Boundedness and fail-closed behavior are part of readability here: the
   recommended changes must retain typed exhaustion, unknown values, path
   identity, deterministic order, and source/report lifecycles.
5. Provider rule declarations were intentionally not abstracted merely because
   their builder syntax repeats; their repeated shapes encode separate policy
   identities and are clearer as declarative definitions.

## Resolved Decisions

The following decisions close the audit questions without expanding public
surfaces or introducing abstractions that have no current consumer.

1. **Narrow internal visibility in the affected flow/API pass.** Apply
   READ-016 using the smallest existing visibility boundary required by its
   production callers and tests; do not wait for, or create, a workspace-wide
   visibility convention first. The core architecture already treats parser,
   scope, fact, compiler, cache, and budget internals as private, so this is a
   local consistency cleanup rather than a new policy layer. Keep the
   collector, frozen, and summary lifecycles distinct.

2. **Optimize the ownership path, not the borrowed type uniformly.** Keep
   `&SmolStr` for READ-019's clone-bearing semantic accessors and for the
   public `ResolutionRequest::specifier` in READ-028. `SmolStr` is inline for
   short values and its clone shares long values; rebuilding it from `&str`
   can allocate and is not a performance-neutral encapsulation. Narrow the
   one needless `&String` view (`static_string`) to `&str`, but do not add a
   semantic wrapper or a second accessor without a benchmark from the
   module-linking/resolution workload. This preserves the current narrow API
   surface while keeping the fast ownership operation available.

3. **Keep explicit harness rule selection crate-private.** Implement the
   READ-033 selection path in the private `builtins` module as a
   `pub(crate)`-level factory operation used by adapters and profiling. It
   should remain the owner of provider catalog/environment construction and
   baseline-to-explicit-selection mapping, while preserving provider-prefix
   validation and `RuleBaseline::None` semantics. Do not export a new harness
   API merely to share two in-crate callers.

4. **Retain explicit `AnalysisLimits` methods; do not add a field-table
   macro.** The seven named getters, validated builders, and distinct error
   variants are the public contract and remain easy to review directly. If
   READ-026 is implemented, use only a small private helper where it removes
   a concrete validation duplication while keeping each field and error
   mapping explicit; do not introduce generated method surfaces, a generic
   field table, or a new configuration abstraction.

## Prior Audit Status

READ-001 through READ-011 were checked against the current tree and treated as
completed historical migrations. In particular, the scan did not re-report the
project lookup facade, compiled tsconfig selection encapsulation, scope
artifact/fact visibility, extension-alias iterator, path-store consolidation,
request encapsulation, flow-plan ownership, lifecycle rollback, parameter
projection, artifact-table transition, load metrics, path transformations, or
lowering/cache transitions.

## Coverage

The workspace was reviewed in the following nine coverage partitions; their
findings are consolidated above and the temporary chunk reports are no longer
part of the audit:

- Chunk 01: 9,934 Rust lines across CLI wiring, core facts, and core flow
  effect/cross/planning modules; findings READ-012 through READ-016. Every
  assigned file and associated unit-test module was reviewed. The parallel
  `BoundFlowPlan` storage was checked but not reported because flow-plan
  ownership is an applied historical finding.
- Chunk 02: 9,454 Rust lines across core flow projection/summary/local/lowering
  and matching modules; no additional independent finding beyond READ-012 and
  READ-014. Worklist, budget, overlay, and evidence lifecycles were treated as
  phase-specific state rather than generic consolidation targets. Projector,
  summary, local-artifact, lowering, and matcher files were reviewed; cache
  ownership remains part of READ-013 because its accessors cross boundaries.
- Chunk 03: 9,961 Rust lines across matching occurrence/query/model code,
  project identities/linking/model/projection/resolution, and initial scope
  build modules; findings READ-017 through READ-019. The prior export-resolver
  map migration was checked through the current `ProjectLookup` boundary and
  not re-reported.
- Chunk 04: 9,775 Rust lines across the remaining scope builder, scope
  graph/query, syntax/trace/value identity, and initial API compiler modules;
  findings READ-020 through READ-023. Scope-build, scope-query, syntax, trace,
  value, freeze-transition, and API-compiler tests were included. The raw
  scope-artifact handoff and mutation-fact visibility migrations are
  historical READ-003/READ-005 findings and were not re-reported.
- Chunk 05: 9,851 Rust lines across the remainder of the core API compiler and
  public rule-query construction/validation surface; findings READ-024 and
  READ-025. Normalization, contradiction, physical-plan, reference,
  requirement, validation, lifecycle, value, constructor, expression, and
  focused test code were reviewed. Canonicalization passes that intentionally
  require separate pre/post traversals were not treated as duplication.
- Chunk 06: 9,798 Rust lines across core limits/configuration, diagnostics,
  parsing, environment, project session execution, and project input types;
  findings READ-026 through READ-029. Parsing and source-line bounds,
  environment identity, validated limits, session phase transitions and
  observers, and project input outcomes were reviewed. The artifact-cache
  transition is historical and was not re-reported.
- Chunk 07: 9,967 Rust lines across core project reporting/types and the
  datastructures budget, diagnostics, fingerprint, history, name, path,
  path-trie, and table implementations; findings READ-030 and READ-031.
  Datastructure implementations and tests, project reports, report types,
  paths, and rule IDs were reviewed. Serialization-facing report fields and
  deliberate borrowed-buffer path APIs were treated as explicit boundaries.
- Chunk 08: 9,970 Rust lines across the harness CLI, harness cases/profiling/
  reporting, all JavaScript provider rules, and the first two-thirds of the
  Obsidian provider rules; findings READ-032 and READ-033. Provider rule
  declarations were not proposed for generic abstraction: their repeated
  shape is the intended declarative policy boundary and their matchers carry
  provider-specific semantics.
- Chunk 09: 6,630 Rust lines across the remaining Obsidian rules, output
  rendering, and the complete project loader/discovery/resolver/tsconfig
  crate; findings READ-034 through READ-036. Obsidian rules, output report
  types/rendering, project admission, corpus, loader, resolver, options,
  tsconfig, and tests were reviewed. The completed tsconfig-selection
  encapsulation migration and provider rule declaration repetition were not
  re-reported.

The review also included the root and owning-crate architecture documents,
`TESTING.md`, `CONTRIBUTING.md`, the existing audit history, representative
production call sites, unit and integration tests, all provider crates,
output, harness, project, and datastructures code. The report is read-only;
only `CODEBASE_READABILITY_AUDIT.md` is retained as the audit artifact.
