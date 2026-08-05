# Codebase Readability Audit — Chunk 5

## Summary

Chunk 5 covers the two-pass lexical scope collector, declaration planning,
source-order assignment history, callback parameter projection, mutation and
scope indexes, and the frozen semantic query surface. The phase split is a
good fit for the architecture: declarations are planned before use-position
collection, assignment history is reversible, and frozen queries keep
shadowing, mutation, and dynamic lookup checks together. The main risks are
phase-boundary protocols that are not carried by the resulting types.

The most important issue is that scope-shape validity is checked and reported
but is discarded when the mutable graph becomes frozen; a failed shape lookup
can also be paired with an unconditional scope pop. Other findings concern
duplicated binding semantics between mutable and frozen graphs, raw correlated
arguments during graph assembly, duplicated lexical/global key traversal, and
the multi-stage member-chain resolver.

No source, test, configuration, dependency, or documentation changes were made
by this audit.

## Findings

### Scope-plan validity and freeze boundary

#### [x] READ-028 — Preserve scope-shape invalidity through the frozen graph

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture / fail-closed state
- **Location:** `glass-lint-core/src/analysis/scope/build/collector.rs:152-172`,
  `analysis/scope/build/freeze.rs:12-47`,
  `analysis/scope/graph.rs:44-59, 76-99, 337-339`,
  `analysis/scope/scope_index.rs:49-58`

The collector records `ShapeMismatch` when a planned child scope cannot be
consumed, but `push_scope` does not push a replacement scope and
`pop_scope` later pops the current parent unconditionally. A mismatch can
therefore corrupt the collector's active stack before the issue is reported.
The freeze path computes `scope_shape_valid` and passes it to `ScopeGraph`,
but `ScopeGraph::freeze` drops the field; `FrozenScopeGraph::scope_at` then
calls the index with `true` unconditionally. The lowering layer reports the
scope-shape issue while subsequent frozen resolution can still use a
misaligned scope rather than the intended root/fail-closed result.

Make scope entry return an explicit success/invalid token or track whether the
corresponding push occurred so every traversal exit balances the same entry.
Carry the validity bit into `FrozenScopeGraph`, or construct a frozen graph
whose scope queries are permanently conservative when collection was invalid.
Delete the raw unconditional pop and the discarded validity field after
migration. Preserve planner/collector shape matching, deterministic scope and
binding IDs, the existing `ScopeShapeMismatch` diagnostic, root fallback for
invalid scope lookup, and conservative handling of all later provenance and
constant queries. Add a test that forces a mismatch and verifies frozen
queries do not use a non-root scope.

**Fix Applied:** Scope-pass entry now returns an explicit success bit and
scope exits receive that bit, so failed planned entries neither descend into
the subtree nor pop a parent scope. The validity bit is carried into
`FrozenScopeGraph`, whose scope lookup permanently falls back to root after a
shape mismatch. Added a regression covering the frozen lookup boundary.

**Verification:** `make fmt && make ci` passes, including 780 core tests,
scope-shape divergence tests, workspace checks, end-to-end/provider harnesses,
doctests, generated-rule validation, and examples.

#### [x] READ-029 — Share binding lookup semantics across mutable and frozen graphs

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Architecture / certainty policy
- **Location:** `glass-lint-core/src/analysis/scope/graph.rs:200-255`,
  `analysis/scope/query/bindings.rs:29-70, 155-184`,
  `analysis/scope/query/functions.rs:7-22`

The collection graph and frozen graph each implement nearest-scope lookup,
binding fallback, parameter precedence, and enclosing-function discovery.
`ScopeGraph::binding_at` uses the collector-side parameter lookup, while the
frozen query reconstructs a function ID before looking up the same parameter
alias. `binding_with_scope_at` and `function_scope_at` are also repeated on
both sides of the freeze boundary. These are phase-specific storage adapters
around one semantic decision, but the current duplication allows declaration,
parameter, joined-assignment, and function-scope behavior to drift.

Introduce a private binding-query owner or shared decision helper whose phase
adapter supplies only the mutable/frozen index access. Delete the duplicate
nearest-scope and enclosing-function algorithms after migration. Preserve
use-position scope selection, declaration-versus-assignment precedence,
parameter aliases, joined alternatives, TDZ/local shadowing, dynamic lookup
invalidity, and the zero/root fallback for malformed or absent identities.

**Fix Applied:** `ScopeData` now owns nearest-binding lookup, parameter-alias
selection, and enclosing-function fallback. Mutable and frozen graph adapters
provide only their phase-specific scope selection, and frozen queries delegate
to the shared decisions; the duplicate traversal loops and unused forwarding
methods were removed.

**Verification:** `make fmt && make ci` passes, including the focused scope
tests and full workspace, end-to-end, rule, doctest, and example checks.

### Scope graph assembly and identity queries

#### [x] READ-030 — Encapsulate correlated `ScopeGraph` construction inputs

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Ownership / phase boundary
- **Location:** `glass-lint-core/src/analysis/scope/build/freeze.rs:20-47`,
  `analysis/scope/graph.rs:76-99`,
  `analysis/scope/binding_index.rs:35-88`,
  `analysis/scope/mutation_index.rs:15-37`

The freeze transition separately allocates binding/function IDs, converts
assignment history into `BindingIndex`, converts mutable-object state into
`MutationIndex`, and passes names, lexical scopes, both indexes, mutation
state, and a validity flag as independent arguments to
`ScopeGraph::from_collected`. These values are all one correlated snapshot of
the same source and scope plan. The raw constructor makes it possible for a
future caller to pair indexes made from different scope vectors or to omit a
new derived index while still satisfying the function signature.

Give the freeze owner one private collection snapshot/input type, or make
`ScopeGraph::from_collected` consume the complete `ScopeCollector` freeze
product and assemble its indexes internally. Delete the multi-argument
constructor protocol after migration. Preserve stable allocation order,
source-order assignment sorting, property/mutation finalization, name-table
identity, scope-shape validity, and the separation between mutable collection
state and immutable query state.

**Fix Applied:** Added the private `ScopeGraphInput` snapshot type at the
freeze/graph boundary and changed `ScopeGraph::from_collected` to consume it
as one coherent bundle. The freeze phase still owns index construction and
scope-shape validity calculation, while the graph constructor no longer
accepts six correlated values independently.

**Verification:** Focused scope build/query tests pass, and `make fmt && make
ci` passes, including the full workspace, end-to-end, rule, doctest, and
example checks.

#### [x] READ-031 — Unify lexical and global binding-key traversal

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / API / identity policy
- **Location:** `glass-lint-core/src/analysis/scope/query/bindings.rs:75-123, 144-153`,
  `analysis/scope/query/provenance/object.rs:15-25`

`binding_key_for_expr` and `global_key_for_expr` repeat the same recursive
handling for member expressions, `this`, parentheses, and sequence tails.
They differ only in the root policy: one requires a lexical binding and the
other permits a root only when no binding shadows it. `binding_key_for_name`
then repeats part of the same root decision for name-based callers. The
member-seed path in `provenance/object.rs` must manually try the lexical form
and then the global form to reconstruct one key.

This spreads the identity rule across traversal copies and makes a new
transparent expression form or member-name restriction easy to add to only
one path. Create one private expression-key traversal with an explicit root
mode/result, and have name-based lookup adapt to it. Delete the duplicated
recursive matches while preserving lexical-before-global precedence, shadowing
rejection, `this` roots, property segment interning, sequence/parenthesis
transparency, and `None` for dynamic or unsupported expressions.

**Fix Applied:** Added one root-mode-aware expression-key traversal for
lexical, global, and lexical-or-global roots. Name lookup reuses the same
identifier owner, member-value seeding no longer retries separate traversals,
and the obsolete global traversal API was removed.

**Verification:** `cargo test -p glass-lint-core analysis::scope::query --lib`
and the scope integration tests (31 passed); `make fmt && make ci` (all
passed).

### Hard-to-read provenance resolution

#### [ ] READ-032 — Split member-chain resolution into named resolution stages

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Hard-to-read function / Complexity / API
- **Location:** `glass-lint-core/src/analysis/scope/query/provenance/chain.rs:41-136`

`FrozenScopeGraph::resolve_member_chain` combines four distinct policies in
one function: dynamic-lookup rejection, prefix backtracking through property
assignment aliases, fallback through all binding provenance alternatives, and
global-root promotion with mutation checks. Each policy has different
certainty rules and different path representations, but the function moves
between them through local `NamePath`/`SymbolPath` suffix calculations and a
large provenance match. The current comments explain why it is linear, yet a
caller still has to understand alias-index ordering, branch scope containment,
returned-object targets, and global promotion to verify the result.

Split the operation into private stages with a single precedence owner, for
example assigned-prefix resolution, alias/provenance resolution, and global
fallback, returning a small internal resolution outcome before suffix
assembly. Delete the interleaved policy branches after migration. Preserve
current assignment-before-provenance precedence, use-position scope checks,
all complete alternatives, dynamic/eval invalidation, global-object aliases,
`this` handling, property mutation rejection, and the rule that unsupported or
ambiguous paths establish no rooted witness.

**Fix Applied:** None so far.

## Systemic Themes

The scope subsystem has a well-defined conceptual lifecycle, but several
phase transitions are represented by raw data and booleans: shape validity,
scope stack entry, the seven correlated graph inputs, and binding-key root
selection. These are exactly the boundaries where the architecture requires
fail-closed behavior, so the owning types should carry the state rather than
leaving callers to preserve it by convention.

The query surface also repeats semantic decisions across storage adapters and
expression traversals. Refactors should centralize those decisions without
sharing mutable collection storage with frozen artifacts or collapsing lexical,
global, dynamic, and project-linked identities. Existing reversible history,
bounded alternatives, deterministic indexes, and explicit unknown/local
provenance are useful foundations.

Search signals used for this chunk included validity flags dropped during
freeze, scope push/pop methods with no paired token, duplicate mutable/frozen
query implementations, constructors taking correlated index inputs, repeated
expression-key matches, and long member-chain resolution functions with
multiple certainty policies.

## Open Questions

- Scope-shape mismatch currently remains reportable while facts are retained;
  the intended conservative frozen query behavior should be confirmed before
  choosing root fallback versus a dedicated invalid-query result.
- A shared binding-query owner needs adapters for mutable collector lookups and
  frozen function-ID/parameter indexes without reintroducing AST traversal or
  exposing index storage.
- The next unreviewed handoff is Chunk 6: syntax, trace, and value modules.

## Coverage

Reviewed every source file listed for Chunk 5 in `CODEBASE_STRUCTURE_CORE.md`:
the scope root and binding index; all scope-build analysis, alias, assignment,
binding, callback, collector, compact-pattern, constant, freeze, history,
plan, program, projection, provenance, shape, traversal, visitor, and test
modules; the frozen-assignment, lexical graph, mutation-index, name-environment,
query, scope-index, and all listed binding/constant/function/provenance/rooted
query modules. Representative callers in lowering, resolution, facts, and
matching were traced with `rg`. Existing Chunk 1–4 findings were checked to
avoid re-reporting fact-branch transactions, pattern ownership, flow control
state, exhaustion aggregation, lowering lifecycle, or project identity
overlay findings. READ-028, READ-029, READ-030, and READ-031 are marked
applied above.
