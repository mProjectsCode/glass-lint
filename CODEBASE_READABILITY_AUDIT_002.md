# Codebase Readability Audit

## Summary

Chunk 02 (`analysis::scope`, `analysis::syntax`, and `analysis::trace`) has a
strong overall separation between collection, freezing, and querying. The
shared `ScopePass`/`ScopeTraversal` design is a good example of avoiding
planner/collector traversal duplication, and the history and trace types own
their invariants well.

The main opportunities are at the scope-graph phase boundary: collector
artifacts are unpacked through positional tuples, and a deliberately lossy
binding projection is used by positive provenance classifiers. The latter is
the most important finding because it makes the API’s uncertainty contract
dependent on every caller remembering which projection is safe. The shared
`ScopeReadView` is retained: it already owns the common read implementations,
while the mutable and frozen graph wrappers provide intentional phase APIs.

## Findings

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

Fix Applied: Scope lookup now distinguishes the explicitly named
`preferred_binding_witness_at` compatibility projection from
`definite_binding_at`, which requires `BindingResolutionStatus::Complete`.
Module export/member, returned-object, constructed-instance, and constant
classifiers use the definite query; shadowing checks retain the preferred
witness query. The mutable collection graph uses the same vocabulary. Verified
with `cargo test -p glass-lint-core analysis::scope --lib`.

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
policy. Keep a possible-witness path where the matching contract needs an
independent complete witness, but carry its incomplete/joined status so it
cannot be reported as definite. Add adversarial tests for joined assignments,
reassignment, incomplete paths, dynamic lookup, and independent complete
witnesses so a cleanup cannot discard valid alternatives.

Fix Applied: None so far.

## Systemic Themes

- The collector/freeze boundary has good domain types, but several are used as
  transport records rather than owners of the conversion into final indexes.
  Keeping conversion policy with the receiving index or a named lowering type
  would reduce positional plumbing.
- `ScopeReadView` is already the single owner of shared read logic; the graph
  wrappers are phase-specific access surfaces, not an independent duplication
  finding.
- Uncertainty is modeled explicitly by `BindingResolutionStatus` and
  `BindingResolution`; the remaining risk is API ergonomics that allow callers
  to bypass that model with a similarly named lossy projection.

## Review Resolutions

- The mutable graph is needed during collection and property finalization;
  retain its phase-local methods and the shared `ScopeReadView` implementation.
- `binding_at` is intentionally a possible-witness projection. Positive
  provenance classifiers must use the resolution status alongside that witness;
  no caller should infer definiteness from `Option<&BindingProvenance>` alone.

## Coverage

Reviewed Chunk 02: `analysis::scope` (including build, query, graph, binding,
assignment, expression, environment, static-value, and index modules),
`analysis::syntax` (including constant evaluation and provenance), and
`analysis::trace`. Read the root and core architecture guidance, testing and
contributing guidance, the complete readability-audit skill instructions, and
the existing Chunk 01 audit. No source or test files were changed.
