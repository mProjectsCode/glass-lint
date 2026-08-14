# Codebase Readability Audit

## Summary

Chunk 02 (`analysis::scope`, `analysis::syntax`, and `analysis::trace`) has a
strong overall separation between collection, freezing, and querying. The
shared `ScopePass`/`ScopeTraversal` design is a good example of avoiding
planner/collector traversal duplication, and the history and trace types own
their invariants well.

The main opportunities are at the scope-graph phase boundary: the mutable and
frozen graphs still expose a large duplicated forwarding surface, collector
artifacts are unpacked through positional tuples, and a deliberately lossy
binding projection is used by positive provenance classifiers. The latter is
the most important finding because it makes the API’s uncertainty contract
dependent on every caller remembering which projection is safe.

## Findings

### [analysis/scope/graph.rs]

#### [ ] READ-005 — Scope graph query forwarding is split across three façades

- Severity: Medium
- Fix Complexity: Medium
- Theme: ENCAPSULATE
- Category: API
- Location: `glass-lint-core/src/analysis/scope/graph.rs:30-155`, `195-302`, `401-551`

`ScopeData<M>` owns the shared names, lexical scopes, bindings, and mutation
storage, while `ScopeReadView<'a, M>` forwards roughly a dozen read operations
from that state. `ScopeGraph` then adds collection-phase forwarding methods,
and `FrozenScopeGraph` repeats most of the same forwarding surface for the
frozen phase. Representative methods include `scope_parent`, `scope_at`,
`nearest_binding_at`, `assignment_at`, `binding_id_at`, `binding_version`,
`function_binding`, and `function_alias`; callers use the mutable graph during
`ScopeCollector::freeze`/property finalization and the frozen graph throughout
`scope/query/*`.

The phase distinction is legitimate, but the ownership boundary is diffuse:
adding or changing a shared query requires deciding whether to update
`ScopeReadView`, `ScopeGraph`, `FrozenScopeGraph`, or all three. The repeated
forwarders also obscure which operations truly require the mutable builder
versus merely read `ScopeData`. The current `ScopeReadView` reduces duplicate
logic but does not remove the duplicated public-in-crate API surface.

Recommendation: make the shared read operations methods on the owning
`ScopeData<M>` (passing the shape-valid flag only to operations such as
`scope_at`), and keep phase wrappers limited to phase-specific conversions,
mutation recording, and intentionally different name views. Delete the
`ScopeReadView` wrapper and redundant graph forwarders once callers are moved.
Preserve the mutable/frozen type distinction and add a compile-time or focused
parity test for the shared query set so a new query cannot accidentally be
available in only one phase.

Fix Applied: None so far.

### [analysis/scope/build, analysis/scope/graph.rs]

#### [ ] READ-006 — Freeze artifacts cross the boundary through one-shot tuples

- Severity: Medium
- Fix Complexity: Low
- Theme: SIMPLIFY
- Category: Conversion
- Location: `glass-lint-core/src/analysis/scope/build/mod.rs:52-115`, `117-132`; `glass-lint-core/src/analysis/scope/build/program.rs:20-90`; `glass-lint-core/src/analysis/scope/graph.rs:304-348`

`ScopeCollectionArtifacts::seal` creates `FrozenScopeCollectionArtifacts`,
which immediately nests a `FrozenPropertyArtifacts` bundle. The three property
record types then expose `into_parts` methods returning positional tuples:
`PropertyAliasAssignment` returns five values,
`RootedPropertyMutation` returns four, and `ScopedDynamicEval` returns two.
Each tuple is consumed only by `ScopeGraph::finish_collected_properties`,
which immediately reconstructs `PropertyAliasFact`,
`RootedPropertyMutationFact`, or an `(ScopeId, ScopeEffect)` pair. This creates
two representations and positional coupling solely to move records from the
collector into the mutation index; adding a field requires editing the record,
its tuple signature, and every destructuring site without compiler help from
field names.

Recommendation: let the owning lowering operation consume named domain records
directly— for example, a private `ScopeGraph` conversion method per artifact
kind, or a `FrozenPropertyArtifacts::lower_into` operation that retains named
fields until index insertion. Delete the public-in-analysis `into_parts`
methods and flatten the intermediate frozen bundle if it remains only a
freeze-handoff wrapper. Preserve the current ordering, receiver-key lookup,
dynamic-`eval` filter, and deterministic mutation-index insertion with focused
property-artifact tests.

Fix Applied: None so far.

### [analysis/scope/query/bindings.rs, analysis/scope/query/provenance]

#### [ ] READ-007 — Lossy `binding_at` is too easy to use for definite provenance

- Severity: High
- Fix Complexity: Medium
- Theme: ENCAPSULATE
- Category: API
- Location: `glass-lint-core/src/analysis/scope/query/bindings.rs:31-50`; `glass-lint-core/src/analysis/scope/query/provenance/callable.rs:240-290`; `glass-lint-core/src/analysis/scope/query/provenance/object.rs:48-80`, `101-120`; `glass-lint-core/src/analysis/scope/graph.rs:350-362`

`FrozenScopeGraph::binding_at` is documented as a convenience projection that
calls `BindingResolution::preferred_witness`, intentionally discarding
`Joined` and `Incomplete` status. The lower-level collection graph has a
parallel method with the same name and the same first-witness behavior.
However, `module_export_for_chain` and
`member_call_provenance_for_chain` use `binding_at` to return positive module
provenance, while `module_member_for_member` and returned-object resolution use
it to classify an identifier or reject a shadowed `require`. These are
certainty-producing decisions, not merely display or fallback queries.

This makes the uncertainty invariant caller-dependent: an alias with multiple
possible assignments or an exhausted/incomplete path can still supply a
preferred module witness to a classifier that never inspected the resolution
status. The API documentation warns callers, but the method name and return
type do not communicate that it is a possible-witness projection, and the
strict/compatibility wording is inconsistent with the repository rule that
ambiguity, unsupported semantics, and exhausted alternatives cannot establish
a witness.

Recommendation: centralize the policy on a named API. Either make positive
classifiers consume `BindingResolution` and require an explicit complete
status, or rename the lossy method to something such as
`preferred_binding_witness_at` and introduce a clearly named definite query.
Migrate `module_export_for_chain`, `member_call_provenance_for_chain`,
`module_member_for_member`, and returned-object classification to the explicit
policy. Delete the ambiguous compatibility path if no remaining caller needs
it. Add adversarial tests for joined assignments, reassignment, incomplete
paths, dynamic lookup, and independent complete witnesses so a cleanup cannot
discard valid alternatives.

Fix Applied: None so far.

## Systemic Themes

- The collector/freeze boundary has good domain types, but several are used as
  transport records rather than owners of the conversion into final indexes.
  Keeping conversion policy with the receiving index or a named lowering type
  would reduce positional plumbing.
- Phase ownership is conceptually clear, but the graph API repeats shared
  operations around a generic read view. A single owner for shared reads would
  make the mutable/frozen distinction easier to audit.
- Uncertainty is modeled explicitly by `BindingResolutionStatus` and
  `BindingResolution`; the remaining risk is API ergonomics that allow callers
  to bypass that model with a similarly named lossy projection.

## Open Questions

- Is the mutable `ScopeGraph` intended to remain a general query surface after
  collection, or can its read methods be reduced to the small set required by
  `ScopeCollector` and property finalization?
- Are any callers intentionally relying on `binding_at` as a possible-witness
  query? If so, should that policy be named explicitly rather than sharing the
  strict-looking binding name?

## Coverage

Reviewed Chunk 02: `analysis::scope` (including build, query, graph, binding,
assignment, expression, environment, static-value, and index modules),
`analysis::syntax` (including constant evaluation and provenance), and
`analysis::trace`. Read the root and core architecture guidance, testing and
contributing guidance, the complete readability-audit skill instructions, and
the existing Chunk 01 audit. No source or test files were changed.
