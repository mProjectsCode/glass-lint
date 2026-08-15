# Codebase Readability Audit — glass-lint-core Chunk 6: Syntax helpers and trace

## Summary

Chunk 6 covers `glass-lint-core/src/analysis/syntax/` (`name`, `names`,
`provenance`, and `constant/{eval,types}`) and `analysis/trace.rs`. The chunk
owns the provider-neutral syntax normalization helpers consumed by scope
collection and fact building, the bounded constant evaluator, the
provenance enums, and the bounded trace arena that backs cross-module
evidence reconstruction.

Overall the code is cohesive and well documented: the bounded constant
evaluation design (fresh/shared budget, `NoLookup` isolation, re-admission
via `ConstValue::bounded`) is a deliberate, verified invariant and the trace
arena's foreign-handle rejection and fail-closed exhaustion are genuinely
enforced. The main issues are (1) two parallel property-name conversion
paths in `names.rs` and `constant/eval.rs` whose semantics diverge subtly,
(2) a few speculative or sentinel-shaped API choices (`BudgetComponent`,
`TraceArena::new(0)`, a public pattern walker with no external callers), and
(3) small internal duplication/indirection inside the evaluator and trace
arena. No findings propose collapsing distinct lifecycles, uncertainty
states, or provider boundaries; the fail-closed budget semantics are treated
as guardrails throughout.

## Findings

### [analysis/syntax — constant and names]

#### [x] READ-001 — Two parallel property-name conversion paths with divergent bounds semantics

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/syntax/names.rs:89-103,171-177,216-218`, `glass-lint-core/src/analysis/syntax/constant/eval.rs:357-375,377-389`

`PropName`/`MemberProp` → `Option<SmolStr>` conversion is implemented twice.
`names.rs::literal_property_name` and `names.rs::literal_member_property_name`
(with private helper `static_property_name` at names.rs:216) dispatch over the
same AST types as `eval.rs::contextual_property_name` and
`eval.rs::contextual_member_property_name`. Only the member-property pair is a
true duplicate: `literal_member_property_name` is behaviorally identical to
`contextual_member_property_name(prop, &NoLookup)` under a fresh `EvalState`
(Ident/PrivateName/Computed handling is the same). `literal_property_name` is a
distinct pure-syntax path that genuinely differs from `contextual_property_name`
on `PropName::Str` (no `MAX_STRING_BYTES` bound), `PropName::Num` (any number
vs. non-negative integers only), and `PropName::Computed` (only
`Expr::Lit(Lit::Str)` vs. full bounded evaluation via `property_key`). Both are
used widely — representative call sites: `literal_member_property_name` at
`scope/build/visitor.rs:335`, `facts/interface/commonjs.rs:36`,
`resolution/call.rs:87`; `literal_property_name` at
`scope/build/provenance.rs:263`, `facts/pattern.rs:196`,
`scope/build/projection.rs:54` — so any future edit to one path's bounds or
accepted shapes will silently diverge from the other.

**Recommendation:** Consolidate only the behaviorally identical pair.
Re-express `literal_member_property_name` as
`contextual_member_property_name(prop, &NoLookup)` and delete the private
`static_property_name` (plus the then-unused `evaluate` import in names.rs):
the Ident/PrivateName/Computed arms and the fresh `EvalState` make the two
exactly equivalent, so every call site keeps its current accepted shape. Leave
`literal_property_name` as its own documented pure-syntax path — unlike
`contextual_property_name` it does not bound string keys with
`MAX_STRING_BYTES`, accepts arbitrary numeric keys, and only handles
literal-string computed keys — so re-expressing it through
`contextual_property_name` would change accepted shapes at its call sites
(`facts/pattern.rs:196`, `scope/build/provenance.rs:263`,
`scope/build/projection.rs:54`). Document the divergence in
`literal_property_name`'s doc comment so the two paths are not mistaken for
variants of one another. Guardrail: the string bound must not be loosened on
any path.

**Fix Applied:** `literal_member_property_name` now delegates to `contextual_member_property_name(prop, &NoLookup)`; the private `static_property_name` and the unused `evaluate` import in names.rs were deleted. `literal_property_name` was kept as its own pure-syntax path and its doc comment now documents the divergence (no `MAX_STRING_BYTES` bound, arbitrary numeric keys, literal-string computed keys only).

#### [x] READ-002 — Shorthand object-property arm allocates a cloned `Ident`/`Expr` to reach `lookup_ident`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/syntax/constant/eval.rs:280-282`

In `EvalState::evaluate_object`, the `Prop::Shorthand` arm evaluates the key
via `self.evaluate(&Expr::Ident(ident.clone()), lookup)`, which boxes a fresh
`Expr::Ident`, clones the `Ident`, and immediately dispatches through the
`Expr::Ident` arm of `evaluate_inner` to `self.lookup_ident(lookup, ident)`
(eval.rs:180). The intermediate allocation is consumed at a single call site
and adds no behavior; `self.lookup_ident(lookup, ident)` is the equivalent
direct call.

**Recommendation:** Replace the clone-and-wrap with `self.lookup_ident(lookup, ident)`.
Guardrail: `EvalState::evaluate` also charges one node-budget increment for the
wrapper expression; if that accounting is meant to model the syntactic node,
charge it explicitly (e.g. a small `consume_node` helper) so budget behavior
for shorthand properties stays unchanged.

**Fix Applied:** The `Prop::Shorthand` arm now calls `self.lookup_ident(lookup, ident)` directly. A new `consume_node` helper (shared with `EvalState::evaluate`) charges the same node/depth increment the wrapped-`Expr::Ident` path used to, so the shorthand budget accounting is unchanged. Added a focused unit test `evaluates_shorthand_properties_through_the_lookup`.

### [analysis/syntax — name bounds and provenance]

#### [x] READ-003 — `MAX_NAMES` duplicates `DEFAULT_MAX_NAMES` across the crate boundary

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/syntax/name.rs:8`, `glass-lint-datastructures/src/name.rs:10`

`name.rs::MAX_NAMES` and `glass_lint_datastructures::DEFAULT_MAX_NAMES` are
both `1 << 20`. The doc comment on `MAX_NAMES` states it "matches the default
semantic-operation bound," which refers to core's own
`limits.rs::default_semantic_operations` (also `1 << 20`), not to the
datastructures `NameTable` default; the two constants happen to share the
value. Core re-declares the value instead of referencing the datastructures
default (used at `analysis/semantic/mod.rs:147,431` and
`analysis/resolution/mod.rs:283`), so a future change to the datastructures
default silently diverges from core's pinned value. Divergence matters: the
name bound affects resolution output but is excluded from the artifact cache
key (`LocalAnalysisConfig`), so it must not drift silently.

**Recommendation:** Keep `MAX_NAMES` as core's deliberate artifact bound and
make the alignment explicit: add a compile-time assertion that
`MAX_NAMES == glass_lint_datastructures::DEFAULT_MAX_NAMES`, and extend the
doc comment to state that the value also matches the default semantic-operation
bound. Do not replace `MAX_NAMES` with a reference to `DEFAULT_MAX_NAMES`:
core's name bound affects resolution output but is excluded from the artifact
cache key (`LocalAnalysisConfig`), so silently tracking the `NameTable` default
would change artifacts without invalidating the cache. Guardrail: the
cache-identity and `NameTable` capacity semantics must not change.

**Fix Applied:** `MAX_NAMES` stays core's deliberate artifact bound, now pinned at compile time via `const _: () = assert!(MAX_NAMES == glass_lint_datastructures::DEFAULT_MAX_NAMES)`. `DEFAULT_MAX_NAMES` was re-exported at the datastructures crate root (`pub use name::{DEFAULT_MAX_NAMES, ...}`), and the `MAX_NAMES` doc comment now states it matches both the datastructures default and the default semantic-operation bound. Cache-identity and `NameTable` capacity semantics are unchanged.

#### [ ] READ-004 — `BudgetComponent` is a single-variant enum with an always-`None` `observed` payload

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/syntax/provenance.rs:17-33`, construction at `glass-lint-core/src/analysis/resolution/expression.rs:333-337`, `glass-lint-core/src/analysis/resolution/call.rs:141-145`, `glass-lint-core/src/analysis/resolution/expression/static_values.rs:111-115`

`UnknownReason::BudgetExhausted { component, limit, observed }` carries a
`BudgetComponent` enum with exactly one variant (`Values`) and an
`observed: Option<usize>` that is `None` at every construction site. The
`UnknownReason` payload is only ever constructed (never matched for distinct
behavior anywhere in the workspace), so `component` and `observed` are
speculative vocabulary for a hypothetical second budget component and
observed-value reporting.

**Recommendation:** Collapse `BudgetExhausted` to `BudgetExhausted { limit }`
and delete `BudgetComponent` until a second component exists; if the reason
payload is intended to be surfaced in future diagnostics, document which
fields are part of the public reason contract. Guardrail: keep the
`BudgetExhausted` variant distinct from `Unresolved`/`Unsupported`/`Missing`/
`Cycle` — budget exhaustion is a materially different fail-closed outcome and
must not be collapsed.

**Fix Applied:** None so far.

### [analysis/syntax — names module surface]

#### [ ] READ-005 — Public `walk_pat_ident_bindings` has a single consumer and speculative callback surface

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/syntax/names.rs:50-78` (re-exported via `analysis/syntax/mod.rs:18`)

`walk_pat_ident_bindings` is `pub` and re-exported at `analysis::syntax`, but
its only caller in the workspace is the sibling `collect_pat_bindings`
(names.rs:74), which itself has exactly two callers
(`facts/interface/mod.rs:51`, `scope/build/bindings.rs:93`), both of which want
the same `BTreeSet<SmolStr>`. The callback-based walker adds public surface
and an indirection layer for one concrete use.

**Recommendation:** Keep `collect_pat_bindings` as the public operation and
make `walk_pat_ident_bindings` private (or fold it into
`collect_pat_bindings`) until a second consumer needs a non-`BTreeSet` walk.
Guardrail: no other caller depends on the callback form today; if a future
caller needs to emit binding events, promote the walker back to `pub` at that
point.

**Fix Applied:** None so far.

### [analysis/trace]

#### [ ] READ-006 — `TraceArena::new(0)` used as a magic "traces disabled" sentinel

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/report/mod.rs:199-212`, `glass-lint-core/src/analysis/trace.rs:78-86`

On rule-selection failure, `match_project` substitutes a
`TraceArena::new(0)` for the real arena. Zero is not a legitimate trace limit
— `AnalysisLimits::trace_nodes` is a `PositiveLimit` that rejects 0
(`glass-lint-core/src/limits.rs:59-69`) — so the sentinel silently borrows an
invariant owned by `limits.rs`, and the `limit` field doubles as a disabled
flag. The owning session already stores `Option<TraceArena>` defaulting to
`None` (`lint/report/mod.rs:66-77,107-109,124-132`), so the failure path can
express "no traces" without a sentinel arena.

**Recommendation:** Pass `None` (or skip `set_trace_arena`) on the failure
path instead of constructing a zero-limit arena, and keep
`TraceArena::new(limit)` for real positive limits only. Guardrail:
`reconstruct_trace` and `trace_node_count` already return `None`/`0` for a
missing arena, so report output and operation counts are unchanged; do not
change the positive-limit contract of `TraceArena`.

**Fix Applied:** None so far.

#### [ ] READ-007 — Trace arena API-shape nits: redundant constructor argument and a coordination free function

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/trace.rs:106,120-129,162-183,185-193`

Two small API-shape issues in the trace module. First,
`TraceNodeId::from_node_count(self.arena, self.nodes.len())` (trace.rs:106)
threads the arena id through a constructor argument that is always
`self.arena` and that the constructor never validates; an inherent
`TraceArena::node_id(&self, count) -> Option<TraceNodeId>` would remove the
redundant parameter. Second, `intern_lifecycle_trace` (trace.rs:162-183) is a
`pub(super)` free function that only ever calls `TraceArena::intern_chain`
(two callers: `flow/cross/evidence.rs:186`, `flow/projector/evidence.rs:271`),
whereas the repo convention is to put behavior on the type that owns the state
(AGENTS.md: "Use free functions for genuine coordination across independent
types").

**Recommendation:** The arena id is already a field of `TraceArena`; replace
the `TraceNodeId::from_node_count(self.arena, self.nodes.len())` call at
trace.rs:106 with an inherent `TraceArena::node_id(&self, count: usize) ->
Option<TraceNodeId>` that uses `self.arena`, and drop the redundant arena
parameter from `TraceNodeId::from_node_count` (its only caller). Move
`intern_lifecycle_trace` onto `TraceArena` as an inherent method that calls
`self.intern_chain(steps)`. Guardrail: keep the foreign-handle rejection
(`parent.arena != self.arena`) and fail-closed exhaustion behavior exactly as
tested in `analysis/trace/tests.rs`.

**Fix Applied:** None so far.

## Systemic Themes

- **Parallel expression-to-`SymbolPath` / property-name builders.** Beyond
  the chunk-owned paths in READ-001, the analysis crate has three distinct
  `member_expression_chain` operations with the same name and different
  semantics: the pure-syntax chain `analysis::syntax::member_expression_chain`
  (`names.rs:131`), the contextual `FrozenScopeGraph::member_expression_chain`
  (`scope/query/provenance/callable.rs:212`, which recombines
  `expression_name` with `contextual_member_property_name`), and the cached
  `Resolver::member_expression_chain` with syntax fallback
  (`resolution/expression/static_values.rs:63-75`). The naming collision makes
  the distinct contexts hard to distinguish at call sites
  (`facts/visitor.rs:48`, `facts/calls/callee.rs:112`).
- **Sentinel-limit design pattern.** `limits.rs`'s `PositiveLimit` guarantees
  positive trace/operation bounds, and code overloads `0` as a "disabled"
  marker (`TraceArena::new(0)`); the invariant lives in a different module
  than the consumer. Any new limit-taking constructor should document whether
  `0` is meaningful.

## Open Questions

- Resolved: verified that no workspace code matches `UnknownReason` for
  distinct behavior — every consumer pattern-matches
  `SymbolCallProvenance::Unknown(_)` and ignores the payload as a fail-closed
  sentinel. At all three construction sites `component` is
  `BudgetComponent::Values`, `observed` is `None`, and `limit` is `MAX_VALUES`,
  so `component` and `observed` carry no information today and READ-004's
  collapse to `{ limit }` is safe. Whether a future diagnostic will surface the
  reason (and which fields become contract) is a product decision the current
  code does not answer.
- Resolved: the `MAX_NAMES` doc comment predates the datastructures crate and
  describes the value as matching the default semantic-operation bound
  (`limits.rs::default_semantic_operations`, also `1 << 20`);
  `DEFAULT_MAX_NAMES` was introduced with the datastructures crate from the
  same value. The pin is deliberate core policy — `MAX_NAMES` affects
  resolution output but is not part of the artifact cache key — so READ-003 now
  recommends keeping the pin and asserting equality at compile time.

## Coverage

- `glass-lint-core/src/analysis/syntax/mod.rs`, `name.rs`, `names.rs`,
  `provenance.rs`
- `glass-lint-core/src/analysis/syntax/constant/mod.rs`, `eval.rs`,
  `types.rs`, `tests.rs`
- `glass-lint-core/src/analysis/trace.rs`, `trace/tests.rs`
- Representative callers traced: `scope/build/{visitor,collector,provenance,
  projection,bindings,constants,plan,aliases,compact_pat,analysis/classification}.rs`,
  `scope/query/{constants.rs, provenance/callable.rs, provenance/chain.rs,
  provenance/object.rs}`, `facts/{mod,visitor,assignments,construction,
  arguments,functions,pattern}.rs`,
  `facts/interface/{mod,commonjs,exports}.rs`, `facts/calls/{mod,callee,wrapper}.rs`,
  `resolution/{mod,call}.rs`, `resolution/expression.rs`,
  `resolution/expression/static_values.rs`, `scope/static_value.rs`,
  `model/static_properties.rs`, `flow/cross/{evidence,mod,state,worklist,
  sources,propagation,graph}.rs`, `flow/projector/{mod,evidence,driver}.rs`,
  `project/projection.rs`, `project/model.rs`, `lint/report/mod.rs`,
  `lint/report/evidence.rs`, `api/classification.rs`, `limits.rs`
- `glass-lint-datastructures/src/name.rs` (for `DEFAULT_MAX_NAMES`)

Verified with `git status --short` after writing this file: only
`CODEBASE_READABILITY_AUDIT_CHUNK_06.md` is untracked; no source, test, or
configuration file was modified.
