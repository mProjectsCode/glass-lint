# Codebase Readability Audit — glass-lint-core Chunk 5: Scope storage and queries

## Summary

Chunk 5 owns the provider-neutral scope storage and query surface of
`glass-lint-core`: `analysis::scope::expression`, `frozen_assignments`
(`AssignmentAt`, `BindingResolution`, `FrozenAssignmentIndex`), `graph`
(`ScopeGraph`/`FrozenScopeGraph`, `ScopeData`, `ScopeReadView`),
`mutation_index`, `name_env`, `scope_index`, `static_value`, and the
`query` submodule (`bindings`, `constants`, `functions`, `provenance`
incl. `callable`/`chain`/`object`, `rooted`). The chunk's contract is the
immutable `FrozenScopeGraph` facade queried by `analysis::resolution`,
`analysis::facts`, and the constant evaluator, produced by freezing a
mutable `ScopeGraph` collected in `build/`.

The storage and query design is fundamentally sound: the two-phase
mutable-collection → immutable-query split is real, fail-closed certainty
(`BindingResolutionStatus` Absent/Complete/Joined/Incomplete) is carefully
preserved, and provenance witness retention is bounded and documented. The
weaknesses are duplicated construction and resolution pipelines that exist
in parallel across the two phases and across sibling query paths, plus a
few naming/forwarding hazards. Findings below are grouped by root cause,
not by file.

## Findings

### Scope query surface (bindings / keys)

#### [ ] READ-001 — `binding_key_for_name` duplicated verbatim across the collection and query phases; `ScopeGraph::binding_version_at` is a pure forwarding wrapper

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `graph.rs:260-273`, `query/bindings.rs:144-157`

`ScopeGraph::binding_key_for_name` (graph.rs:260-269) and
`FrozenScopeGraph::binding_key_for_name` (query/bindings.rs:144-157) are
semantically identical: both walk `binding_with_scope_at`, build
`BindingKey::lexical(function_scope_at, binding_id, binding_version_at)`,
and fall back to `BindingKey::global`. The collection-phase version is
backed by `ScopeGraph::binding_version_at` (graph.rs:271-273), a one-line
private wrapper that only forwards to `binding_version` (graph.rs:168) and
has no caller besides `binding_key_for_name`. The same lexical-key
construction now lives in two phases and must be kept in sync by hand.

**Recommendation:** Extract one helper on the shared owner — `BindingIndex`
or a free `lexical_key_for(scope, name, span)` — that both phases call, and
delete `ScopeGraph::binding_version_at` by having `binding_key_for_name`
call `binding_version` directly. Guardrails: both phases must keep the exact
fallback order (position-versioned lexical key when a binding exists,
`BindingKey::global` when unbound) and must not change the version-at
`BindingVersion::new(0)` default.

**Fix Applied:** None so far.

#### [ ] READ-002 — `&str` vs `NameId` arguments and the `binding_version`/`binding_version_at` names mean different things on `ScopeGraph` vs `FrozenScopeGraph`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `graph.rs:146,153,168,271,376,385,403`; `query/bindings.rs:131-141`

On `ScopeGraph`, `assignment_at` (graph.rs:146), `binding_id_at`
(graph.rs:153), and `binding_version` (graph.rs:168) take `name: &str` and
intern internally, while `binding_version_at` (graph.rs:271) is a pure
forwarder. On `FrozenScopeGraph` the same-named `assignment_at`
(graph.rs:376) and `binding_id_at` (graph.rs:385) take `name: NameId`,
`binding_version` (graph.rs:403) takes `NameId`, and `binding_version_at`
(query/bindings.rs:131) takes `&str` and performs the interning. So the
"resolve at a source position" operation has swapped `_at` naming and
swapped parameter conventions between two types that represent the same
artifact in two phases, and within the frozen type the two names coexist
with different argument types. Readers and extenders must know per call
which graph type and which interning convention applies.

**Recommendation:** Pick one convention per operation and keep the names
stable across the phase boundary — either pass `NameId` everywhere and
resolve the string once at the facade boundary, or keep `&str` on the
facade and rename the `NameId` variants — so `binding_version_at` and
`binding_version` do not swap meaning per type. Guardrails: never-interned
names must keep returning `AssignmentAt::Absent`, `None`, or
`BindingVersion::new(0)` exactly as today; fail-closed results feed
matching certainty and must not change.

**Fix Applied:** None so far.

#### [ ] READ-003 — `ident_binding_seed` reimplements the `binding_resolution_at` resolution pipeline and the dynamic-lookup predicate

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `query/provenance/callable.rs:161-203` vs `query/bindings.rs:59-73,170-176`

`ident_binding_seed` (callable.rs:161-203) manually re-runs the exact
pipeline that `binding_resolution_at` (bindings.rs:59-73) performs —
`scope_at`, `nearest_binding_from_scope`, `parameter_alias_for_scope`,
`assignment_at(...).resolve(...)` — and inline-repeats the dynamic-lookup
predicate of `has_dynamic_lookup_at` (bindings.rs:170-176). The PERF
comment at callable.rs:130-133 documents the motive (keep all seed
projections on one joined-binding result), which is legitimate, but the
rules now exist twice: a change to scope-kind checks or assignment
resolution updates one path and silently leaves the other.

**Recommendation:** Extract a private `NameId`-based core used by both —
e.g. `resolve_binding(name: NameId, use_scope: ScopeId, span) ->
(scope, BindingResolution, Option<BindingKey>)` — then have
`binding_resolution_at` and `ident_binding_seed` derive from it, keeping
the seed's single-resolution guarantee. Guardrails: preserve the seed's
`dynamic_lookup: true` when `scope_at` yields nothing and the identical
`BindingResolutionStatus` results; do not add extra scope/assignment
searches to the hot seed path.

**Fix Applied:** None so far.

#### [ ] READ-008 — `unshadowed_global_at` and `unshadowed_unbound_at` are the same predicate differing by one conjunct

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `query/bindings.rs:196-206`

`unshadowed_unbound_at` (bindings.rs:203-206) is
`!has_dynamic_lookup_at && status == Absent`; `unshadowed_global_at`
(bindings.rs:196-200) adds `is_global(name)` and is used by the constant
evaluator via `Lookup::unshadowed_global` (query/constants.rs:46-48). The
unshadowed/unbound rule is stated twice and must be kept identical.

**Recommendation:** Define `unshadowed_global_at` as
`self.is_global(name) && self.unshadowed_unbound_at(name, span)` so the
predicate lives once. Guardrails: keep the global check first and the
fail-closed result (`false` under any dynamic scope or prior eval); neither
method may consult mutation state.

**Fix Applied:** None so far.

### Rooted-chain queries

#### [ ] READ-004 — `RootedExprContext::rooted_member_chain` forwards to an inherent method of the same name and reads as self-recursion

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `query/rooted.rs:61-63`, `query/provenance/chain.rs:41-47`

The trait impl's `rooted_member_chain` body is `Self::rooted_member_chain(
self, member)` (rooted.rs:62). It compiles only because inherent methods
shadow trait methods, resolving to the real resolver at chain.rs:41. Every
reader must know that shadowing rule to see the non-recursive call, and a
rename or move of chain.rs:41 silently converts this into infinite
recursion at runtime.

**Recommendation:** Rename one of the two methods so they cannot collide —
e.g. name the inherent resolver `resolve_rooted_member_chain` and keep the
trait method as a thin explicit delegator, or move the logic into the trait
impl. Guardrails: keep the trait with both implementors (`ScopeCollector`,
build/collector.rs:222, and `FrozenScopeGraph`), and keep the member-root
resolution semantics unchanged.

**Fix Applied:** None so far.

#### [ ] READ-005 — Three parallel `BindingProvenance` → rooted-path extractions disagree on witness selection and rootedness

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `query/rooted.rs:28-48`, `query/provenance/chain.rs:142-167`, `query/provenance/callable.rs:220-239`

`rooted_ident_chain` (rooted.rs:28-48), `resolve_provenance_alternatives`
(chain.rs:142-167), and `callable_member_chain_from_resolution`
(callable.rs:220-239) each iterate `for_each_witness` and match the same
`ValueAlias`/`BoundCallable` target and `ReturnedObject` source variants,
but they disagree: rooted.rs overwrites `rooted` per witness (last rooted
witness wins, no `rooted_path_available` gate), chain.rs keeps the first via
`resolved.is_none()` and requires `rooted_path_available` (chain.rs:160),
and callable.rs requires `rooted_path_available` per variant. For a joined
assignment carrying several rooted alternatives, sibling query paths can
return different chains, and one may return a chain the others reject —
a latent behavioral divergence for the same binding.

**Recommendation:** Extract one helper on `FrozenScopeGraph`, e.g.
`fn witness_rooted_path(&self, provenance) -> Option<SymbolPath>`, applying
a single rootedness rule and a single witness-selection rule; the three
callers then differ only in the suffix/appending step. Guardrails: keep the
global-absent fallback in rooted.rs (status `Absent` + `is_global`), which
is not a witness path, and preserve the write-occurrence behavior of
`rooted_write_member_chain` (chain.rs:54-65), which intentionally bypasses
the read resolver.

**Fix Applied:** None so far.

#### [ ] READ-006 — Parallel expression-shape normalizers with divergent `Seq` coverage

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `scope/expression.rs:37-73` vs `query/bindings.rs:82-104` vs `query/rooted.rs:66-87`

`normalize_scope_expression` is documented as the shared syntax-shape
adapter with "parenthesized expressions and the final value of a sequence
[as] transparent" (expression.rs:34-36), yet the sibling query paths
re-implement wrapper handling: `expression_key` unwraps `Paren` and `Seq`
(bindings.rs:97-101), while `rooted_expr_chain_with` unwraps `Paren` but
not `Seq` (rooted.rs:84). The same expression `(a, b.c)` therefore resolves
to a member chain in the bindings/object paths but to nothing in the rooted
path, and the divergence is not documented as intentional.

**Recommendation:** Route the transparent-wrapper handling (Paren/Seq)
through `normalize_scope_expression`/its unwrap logic in both query paths,
or extend the canonical adapter so coverage cannot drift. Guardrails: the
rooted path legitimately handles `Call`/`OptChain`/`This` and rejects
`Await` — keep those distinctions; decide the `Seq` case deliberately
(fail-closed is safe in either direction).

**Fix Applied:** None so far.

### Scope storage / index plumbing

#### [ ] READ-007 — Five hand-rolled scope-ancestor walks

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `graph/storage.rs:21-32,43-52`, `graph.rs:461-470`, `query/bindings.rs:179-193`, `query/functions.rs:50-58`

`ScopeData::binding_with_scope_at` (storage.rs:21-32),
`ScopeData::enclosing_function_at` (storage.rs:43-52),
`FrozenScopeGraph::has_prior_eval` (graph.rs:461-470),
`scope_or_ancestor_has_kind` (bindings.rs:179-193), and
`function_binding_at` (functions.rs:50-58) each write the same "start at a
scope, walk `scope_parent` until a match or the root" loop with subtly
different stop conditions. Any change to scope-parent semantics or the root
fallback must be replicated in all five places.

**Recommendation:** Add one iterator/helper on the owning type, e.g.
`ScopeData::ancestors(scope)` yielding the scope then its parents, and let
each caller express its logic as a `find` over that iterator.
Guardrails: preserve the per-caller stop rules — first binding wins
(storage.rs:21), first function wins with the `FunctionId::new(0)` fallback
(storage.rs:43), any-eval-wins (graph.rs:461), first-kind match
(bindings.rs:179), first function-binding wins (functions.rs:50) — and do
not combine facts across different owners in the helper.

**Fix Applied:** None so far.

#### [ ] READ-009 — Single-use `ScopeGraphInput` data-passing struct

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `graph.rs:57-64`, `build/freeze.rs:44-51`

`ScopeGraphInput` is a six-field plain-data struct constructed exactly once
(freeze.rs:44) and immediately destructured by `ScopeGraph::from_collected`
(graph.rs:90-97). It adds a type name and a destructure step without an
invariant or vocabulary beyond "everything needed to assemble a
`ScopeGraph`", functioning as an argument bundle rather than a boundary.

**Recommendation:** Pass the six fields directly to
`ScopeGraph::from_collected(environment, names, scopes, bindings,
mutations, scope_shape_valid)` and delete `ScopeGraphInput`. Guardrails:
keep `scope_shape_valid` and the assembly order (binding index resolved
before property facts are folded in, then `freeze`) exactly as-is; this is
assembly plumbing, not an independent lifecycle owner.

**Fix Applied:** None so far.

## Systemic Themes

- **Loose field visibility on internal wrappers:** `NameEnvironment`'s
  fields (name_env.rs:8-10) and `ScopeData`'s fields (storage.rs:14-17) are
  `pub(super)` but only ever accessed through methods; the module-boundary
  convention here is looser than the surrounding code (most storage types
  keep fields private). Low-value cleanup if the crate is ever split.
- **`ScopeExpression` stores redundant projections:** the `Member` variant
  carries `expression` and `object` alongside `member`, and `Call` carries
  `expression` and `callee`; these are re-derivable from the SWC node and
  must stay consistent with it (expression.rs:12-30). They are consumed
  differently (build/provenance.rs:216 uses `expression`; object.rs:60 uses
  `member`/`object`), so removal needs a per-caller check rather than a
  blind delete.
- **`FrozenScopeGraph` is an 8-method forwarding facade over
  `NameEnvironment`** (graph.rs:294-324): `name_snapshot`, `resolve_name_id`,
  `name_id`, `name_path`, `symbol_path`, `is_global`, `is_global_member`,
  `global_objects` each just call `self.data.names.X`. Justified as the
  single query facade, but the pass-through surface grows with every name
  query; consolidate if a facade trait ever appears.
- **Sort-by-source-position normalization is repeated per fact family:**
  `FrozenAssignmentIndex::from_assignments` (frozen_assignments.rs:145-147)
  and `MutationIndex::sort` (mutation_index.rs:89-101) each sort collected
  facts by `span().lo`, and each consumer implements its own
  `partition_point`-based position lookup. The fact types differ, so this is
  acceptable, but any new index should copy the pattern rather than invent a
  variant.

## Open Questions

- Which `Seq`-wrapped behavior is intended for rooted chains? READ-006
  asserts the coverage divergence; the *intended* behavior for
  `(a, b.c)` under `rooted_expr_chain_with` is not documented anywhere.
- For joined assignments with several rooted alternatives, is the intended
  witness the first or the last retained one? READ-005 documents the
  divergence; the spec for multi-witness rooted identity is not explicit in
  the chunk's docs.
- Is the single-entry `Cell`-based `last_scope_query` cache in
  `LexicalScopeIndex` (scope_index.rs:14,49-61) worth its interior-mutability
  and per-span equality cost if callers query alternating spans?

## Coverage

Reviewed (definitions plus traced callers):

- `scope/expression.rs`, `scope/frozen_assignments.rs` (+ `frozen_assignments/tests.rs`),
  `scope/graph.rs`, `scope/graph/storage.rs`, `scope/mutation_index.rs`,
  `scope/name_env.rs`, `scope/scope_index.rs`, `scope/static_value.rs`
- `scope/query/{mod,bindings,constants,functions,rooted}.rs`,
  `scope/query/provenance/{mod,callable,chain,object}.rs`
- Supporting model and builders: `model/scope.rs`,
  `model/scope/provenance.rs`, `scope/binding_index.rs`,
  `scope/build/{aliases,freeze,provenance,constants,collector}.rs`,
  `syntax/constant/eval.rs`
- Representative external callers: `analysis/resolution/{mod,expression}.rs`,
  `analysis/semantic/mod.rs`, `analysis/facts/{interface/exports,calls/callee}.rs`
- Tests: `scope/tests.rs`, `frozen_assignments/tests.rs`, `model/scope/tests.rs`,
  and integration `glass-lint-core/tests/{matching/scope.rs,query/*,public_surface.rs}`

No `unwrap`/`expect`/`panic` hazards found beyond test-only `.expect` on
parsing and the justified `scope_index.rs:21` "scope index is allocated"
invariant check. No discarded `Result`s, `dead_code` allowances, or obsolete
aliases found in the chunk.
