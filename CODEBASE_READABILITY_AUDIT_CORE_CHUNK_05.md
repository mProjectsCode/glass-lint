# Codebase Readability Audit — glass-lint-core, Chunk 5 (scope storage and queries)

## Summary

Chunk 5 is the post-freeze read-only query surface of `analysis::scope`: the
frozen assignment index, mutation index, lexical scope index, and the five
typed query modules (bindings / constants / functions / provenance / rooted)
that `analysis::resolution::Resolver` and the fact builders consume. The
separation of `ScopeGraph` (collection phase) → `FrozenScopeGraph` (frozen
phase) around a generic `ScopeData<M>` is by-and-large justified: the
`MutationIndexBuilder -> MutationIndex` swap in `freeze()` is a genuine
lifecycle boundary, and the shared phase-agnostic lookups are the right idea.

The audit found five findings. The read surface has grown a second relay layer
(`ScopeReadView`) that mostly forwards verbatim to `BindingIndex` /
`LexicalScopeIndex`, an explicit per-phase re-implementation of the same
binding-resolution query (`preferred_binding_witness_at`), a leaked
sort-into-storage invariant in `MutationIndex` (slices out, ordering assumed),
and two small API/vocabulary defects (`BindingKey::lexical` assembled in three
places; `AssignmentAt::{Known, Ambiguous}` behaving identically).

## Findings

### Scope storage and queries

#### [x] READ-001 — `FrozenScopeGraph` delegates through a second relay layer (`ScopeReadView`) that mostly forwards verbatim to the owning indexes

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/graph.rs:209-404`, `glass-lint-core/src/analysis/scope/graph/storage.rs:55-160`, `glass-lint-core/src/analysis/scope/query/functions.rs:9-11`

`FrozenScopeGraph` (graph.rs:209-404) relays ~25 query methods to
`ScopeReadView` (storage.rs:60-159), which in turn forwards most of them
verbatim to `self.data.scopes` / `self.data.bindings` — `scope_kind` (61),
`scope_span` (65), `parameter_alias_for_scope` (85), `assignment_at` (93),
`binding_id_at` (102), `binding_version` (124), `reassigned_between` (133),
and the five `function_*` lookups (storage.rs:73-74 `enclosing_function_at` plus
the four at 145-159). Only `scope_at`, `nearest_binding_at`,
and `binding_key_for_name` actually combine the `scope_shape_valid` flag with
storage; the other eleven are one-line passes with no vocabulary or invariant
added. The same conceptual operation is additionally re-exposed twice under
two names: `enclosing_function_at` (graph.rs:273-275) and `function_scope_at`
(functions.rs:9-11) return the identical `FunctionId`.

**Recommendation:** Keep `ScopeReadView` for the three compositing/flag-gated
lookups only (`scope_at`, `nearest_binding_at`, `binding_key_for_name`) and the
shared phase-generic resolver from READ-002; move the pure relays either to
`ScopeData<M>` (where `ancestors`, `binding_with_scope_at`,
`parameter_alias_for_scope`, `enclosing_function_at` already live) or to
direct `self.data.scopes`/`self.data.bindings` calls in graph.rs, deleting the
storage.rs copies. Drop `FrozenScopeGraph::enclosing_function_at` in favor of
`function_scope_at`, keeping `ScopeData::enclosing_function_at` as the single
internal implementation (storage.rs:48-52, 73-74). Guardrails: all visibility
may stay `pub(super)`/`pub(in crate::analysis)`; do not change which owner
computes `scope_shape_valid` gating; keep the documented "shared by the
collection and frozen query phases" invariant on `binding_key_for_name`
(storage.rs:106-109).

**Fix Applied:** Reduced `ScopeReadView` to its shape-gated and composite
lookups, moved frozen-phase leaf reads directly to `ScopeData` and its owning
indexes, and made `function_scope_at` the single frozen function-scope API.
The shape-validity gating and shared binding-key construction remain unchanged.

#### [x] READ-002 — Collection- and frozen-phase `preferred_binding_witness_at` are the same query implemented twice

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/graph.rs:183-197`, `glass-lint-core/src/analysis/scope/query/bindings.rs:32-43`

`ScopeGraph::preferred_binding_witness_at` (graph.rs:185-197) and
`FrozenScopeGraph::preferred_binding_witness_at` (bindings.rs:37-43, which
wraps `binding_resolution_at` at 60-73 → `resolve_binding` at 81-93) perform
the identical resolution — nearest visible binding, parameter alias, assignment
resolution, preferred witness — through different plumbing (`ScopeReadView`
vs. `FrozenScopeGraph` name delegation). The section comment at graph.rs:179
("also on FrozenScopeGraph") acknowledges the intended sharing; today the two
bodies can drift independently (e.g., a change to the `Absent`/scope-missing
early returns in one path would not be mirrored in the other). Both bodies are
exercised: scope/tests.rs:66,139 on the collection graph and
query/provenance/object.rs:71 on the frozen graph.

**Recommendation:** Move one generic implementation onto the phase-generic
owner that already serves both phases — `ScopeReadView<'a, M>` (or `ScopeData<M>`)
— and have `ScopeGraph::preferred_binding_witness_at` (graph.rs:185-197) and
`FrozenScopeGraph::preferred_binding_witness_at` delegate the preferred-witness
chain (name lookup → `scope_at` → nearest binding → parameter alias →
assignment resolve → preferred witness) to it, deleting the collection-phase
body (graph.rs:185-197). Precise scope of the move: `binding_resolution_at`
returns a status-bearing `BindingResolution` (bindings.rs:60-73), so it cannot
delegate wholesale to an `Option`-shaped view method; only the preferred-witness
chain moves, while `binding_resolution_at`/`resolve_binding` (bindings.rs:60-93)
must remain to serve the status-aware consumers (`definite_binding_at`
bindings.rs:48-57, `ident_value_seed` via `ident_binding_seed`→`resolve_binding`
callable.rs:178, rooted.rs:27-31, chain.rs:96, callable.rs:50). Guardrails:
preserve the fail-closed `Absent` semantics for a missing name or scope and the
first-non-local-witness order; `scope_shape_valid` must continue gating
`scope_at`; keep `Resolver` and the fact builders feeding from
`ident_value_seed` (callable.rs:114-159) unchanged.

**Fix Applied:** Moved the preferred-witness chain onto the phase-generic
`ScopeReadView` and delegated both `ScopeGraph` and `FrozenScopeGraph` to it.
Status-aware `binding_resolution_at`/`resolve_binding` remain separate for
completeness and fallback decisions.

#### [ ] READ-003 — `MutationIndex` exposes storage-shaped sorted slices and leaks the sort invariant

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/scope/mutation_index.rs:88-132`, `glass-lint-core/src/analysis/scope/query/provenance/chain.rs:115-133, 286-301, 316-333`

`MutationIndex::property_aliases` (mutation_index.rs:103-112) and
`rooted_mutations` (114-121) return raw `&[Fact]` slices that are only
meaningful because `sort()` (89-101) orders them by `span().lo`; callers must
remember and re-derive that ordering. chain.rs re-scans those slices with
`partition_point(|a| a.span().lo <= member.span.lo)` plus a reversed
scope-containment scan (115-133), an `.any` at-or-before-scan (286-301), and a
near-identical `.any` mutation scan (316-333) — three callers re-implementing
"latest/was there a write at or before span in the enclosing scope". The
sibling `FrozenAssignmentIndex` (frozen_assignments.rs:152-214) encapsulates
exactly this class of query (`latest_at`, `version_at`, `changed_between`); the
mutation index does not, so the invariant and the scan logic live at the call
sites.

**Recommendation:** Give `MutationIndex` narrow domain queries that take the
position and any cross-index predicate, e.g. `latest_assignment_at(receiver,
path, span)` and `changed_at_or_before(root, property, span)`, accepting an
`impl Fn(ScopeId) -> bool` scope-containment callback; delete the
`partition_point`/reversed-scan/`.any` blocks in chain.rs:121-131,
294-300, 322-331. Keep the slices as `pub(super)` for the prefix-loop in
`resolve_assigned_prefix` only if no cleaner shape exists. Guardrails: absent
data must keep failing closed as "not written"; the scope-containment check
(`span_contains` over `scope_span`, chain.rs:124-127) must remain applied, not
dropped; do not change which writes count for `is_mutable_static_object` /
`has_prior_eval`.

**Fix Applied:** None so far.

#### [ ] READ-004 — `BindingKey::lexical(function, binding, version)` is re-assembled in three places

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/query/bindings.rs:140-149`, `glass-lint-core/src/analysis/scope/query/provenance/callable.rs:186-192`, `glass-lint-core/src/analysis/scope/graph/storage.rs:110-122`

The same construction — `BindingKey::lexical(enclosing_function, binding_id,
binding_version)` for a located `(scope, name, span)` — appears in
`lexical_identifier_key` (bindings.rs:144-148), `ident_binding_seed`
(callable.rs:186-192), and `binding_key_for_name` (storage.rs:115-119). This is
the identity key used for flow/aliasing, and it is easy to get subtly wrong
(e.g., mixing `function_scope_at` vs `enclosing_function_at`, or dropping the
version) in a new copy.

**Recommendation:** Add one method — `lexical_binding_key(&self, scope:
ScopeId, name: NameId, span: Span) -> Option<BindingKey>` — that composes
`enclosing_function_at` (storage.rs:48-52), `binding_id_at`, and
`binding_version`, and call it from all three sites, deleting the inline
triples in bindings.rs and callable.rs and the inline triple inside
storage.rs:110-122. Corrected home: the helper must live on
`ScopeReadView` (or `ScopeData<M>`), **not** on `FrozenScopeGraph`, because
`binding_key_for_name` — the proposed third caller — is invoked from *both*
phases (collection graph.rs:200-202 and frozen graph.rs:342-348), and the
READ-001 guardrail requires preserving that phase-generic sharing. A
`FrozenScopeGraph::lexical_binding_key` could not serve the collection phase.
Guardrails: keep the global-root fallback in `binding_key_for_name` (returns
`BindingKey::global(name)` when unbound) exactly as-is (storage.rs:111-121);
keep the `Option` semantics (callable.rs currently maps a missing binding id
to `None`, matching `?` in the other sites).

**Fix Applied:** None so far.

#### [ ] READ-005 — `AssignmentAt::{Known, Ambiguous}` are behaviorally identical

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/frozen_assignments.rs:92-119, 168-182`

`AssignmentAt::{Known, Ambiguous}` are declared as "precise branch write" vs
"synthetic post-join write," but the only consumer, `AssignmentAt::resolve`
(frozen_assignments.rs:109-116), matches both to the same arm:
`BindingResolution::assignment(assignment)` — the status is then re-derived from
`assignment.is_incomplete()`/`is_joined()` inside `BindingResolution::assignment`
(52-64), so the two variants can never produce different behavior. `latest_at`
(168-182) splits on `is_joined()` purely to pick the variant. The `Known`/
`Ambiguous` vocabulary adds a second, redundant encoding of a fact the
assignment already carries.

**Recommendation:** Collapse to a single `Assignment(&AliasAssignment)` variant
(`AssignmentAt::Absent | Assignment(&'a AliasAssignment)`), remove the
`is_joined()` branch in `latest_at` (175-181), and let `resolve` keep its single
arm while `BindingResolution::assignment` continues to compute `Incomplete`/
`Joined`/`Complete`. Guardrails: preserve the fail-closed rule documented at
frozen_assignments.rs:163-166 — when a conditional write is present, `latest_at`
must still return a non-`Absent` variant so callers never fall back to the
declaration/parameter witness; that behavior is carried by the `Assignment`
variant, not by the removed distinction.

**Fix Applied:** None so far.

## Systemic Themes

- **Phase-generic sharing done right, then abandoned:** `binding_key_for_name`
  was deliberately made generic over the collection/frozen phases, but the
  sibling query `preferred_binding_witness_at` was re-implemented per phase
  instead of using the same machinery (READ-002). The fix pattern (generic
  composite on `ScopeData<M>`/`ScopeReadView<M>`) already has a precedent in
  the file.
- **Two relay layers instead of one:** `FrozenScopeGraph` → `ScopeReadView` →
  owning indexes produces a "wall of views" with ~35 one-line forwards; the
  composing methods that actually justify the extra layer are the minority
  (READ-001, READ-003).
- **Sorted-slice contracts are implicit:** `FrozenAssignmentIndex` documents
  and encapsulates its ordering; `MutationIndex` does not, pushing
  `partition_point`-based scans into provenance callers.
- **Defensive vocabulary that does nothing:** `AssignmentAt::{Known, Ambiguous}`
  has no behavioral difference anywhere in the crate; its invariant is already
  recorded on `AliasAssignment::is_joined`.

## Open Questions

- **ScopeReadView survival (Q1) — Resolved: keep it, trimmed as READ-001
  prescribes.** Only `LexicalScopeIndex::scope_at` actually reads the
  `scope_shape_valid` flag (scope_index.rs:49-52); the flag is a per-graph
  phase property (graph.rs:41, 53-54), and the view is a zero-allocation
  two-field bundle built inline per call (graph.rs:58-63, 210-215).
  Parameterizing three compositing helpers with `scope_shape_valid` instead
  would push the cross-cutting boolean through every signature
  (storage.rs:69-71, 77-83, 110-122) and reintroduce a threading hazard the
  view removes — not simpler.
- **Duplicated dynamic-lookup predicate (Q2) — Resolved: deliberate hot-path
  probe; keep it.** The PERF note at callable.rs:130-133 asks for a single
  joined-binding result per identifier, and `ident_binding_seed` already
  holds `use_scope` from its own `scope_at` (callable.rs:162), so calling
  `has_dynamic_lookup_at` (which re-runs `scope_at`, bindings.rs:163) would
  double the hottest index query. Both paths fail open identically on an
  unmapped span (`bindings.rs:163-164` → `true`; callable.rs:162-167 →
  `dynamic_lookup: true`), so there is no behavioral divergence today. If
  drift-proofing is wanted, extract the callable.rs:169-170 body into
  `dynamic_lookup_at_scope(scope, span)` and make `has_dynamic_lookup_at` a
  thin wrapper — optional, not an audit finding. The audit's exclusion is
  correct.
- **BindingResolutionStatus::Joined (Q3) — Resolved: keep the three-way
  split.** `frozen_assignments.rs:8-17` documents Joined ("multiple complete
  joined alternatives") and Incomplete ("at least one joined alternative was
  unknown or exhausted") as distinct model states, and the `BindingResolution`
  contract (frozen_assignments.rs:26-30) routes fallback/certainty decisions
  through `status`. Production consumers test only `== Complete`
  (bindings.rs:54-57) and `== / != Absent` (callable.rs:103, chain.rs:192,
  rooted.rs:31-32, bindings.rs:131-133, 186-189); this is a conservative
  consumer, not evidence of a dead variant — `Joined` is precisely what keeps
  `definite_binding_at` "definite" vs `preferred_witness`. Collapsing Joined
  into Complete would falsify `status()`'s certainty meaning; the framing as
  a future extension point stands, with no change recommended.

## Coverage

Reviewed modules (all of Chunk 5): `scope/expression.rs`,
`scope/frozen_assignments.rs` (+ `tests.rs`), `scope/graph.rs`,
`scope/graph/storage.rs`, `scope/mutation_index.rs`, `scope/name_env.rs`,
`scope/scope_index.rs`, `scope/static_value.rs`, `scope/query/{mod,bindings,
constants,functions,rooted}.rs`, `scope/query/provenance/{mod,callable,chain,
object}.rs`. Supporting types inspected:
`analysis/model/scope.rs`, `analysis/model/scope/provenance.rs`,
`analysis/scope/binding_index.rs`, `analysis/scope/mod.rs`.

Callers traced across the crate: `analysis/resolution/mod.rs` (Resolver holds
`FrozenScopeGraph`, uses `ident_value_seed`, `member_value_seed`,
`call_provenance`, `instance_member_available_at`, `constructed_instance_at`,
`unshadowed_*`); `analysis/resolution/expression.rs`, `call.rs`,
`expression/static_values.rs`; `analysis/facts/*` (function_scope_at,
rooted_write_chain, name lookups); the build phase via
`analysis/scope/build/{provenance,assignments,collector}.rs`
(`RootedExprContext`, `normalize_scope_expression`; coverage nit: no
`build/*` module calls `preferred_binding_witness_at` directly — the
collection-phase callers are graph.rs:171 (inside `finish_collected_properties`,
invoked from `build/freeze.rs:56`) and scope/tests.rs:66,139, the frozen-phase
caller is object.rs:71); and the scope unit tests (scope/tests.rs,
frozen_assignments/tests.rs).

Only `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_05.md` was written; no source,
test, or configuration file was modified.
