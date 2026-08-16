# Codebase Readability Audit

Chunk 6 of `glass-lint-core`: `analysis::syntax` (name, names, provenance,
constant/eval, constant/types) and `analysis::trace` (`TraceArena`,
`QualifiedEvent`, `TraceNodeId`, `TraceStep`). Read-only audit; no source was
modified.

## Summary

The chunk is generally well-factored: `syntax` exposes syntax-directed,
AST-independent helpers that fail closed with `None`/`Unknown`; the constant
evaluator keeps one budget owner (`EvalState`) with a documented no-lookup
zero-type (`NoLookup`); and the trace layer defers evidence materialization to
report time behind a real seam rather than duplicating report evidence types.
The findings below target three concrete, repeated operations: a TypeScript
assertion-unwrapping block repeated across eight match sites, a bounded
object-merge capacity pattern duplicated inside the evaluator, and a manual
mirror of the evaluator's node/depth accounting in the shorthand-property
arm.

## Findings

### `analysis::syntax` — names, constant

#### [ ] READ-001 — Repeated four-arm TypeScript assertion unwrap

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/syntax/names.rs:147-150`,
  `glass-lint-core/src/analysis/syntax/names.rs:188-191`,
  `glass-lint-core/src/analysis/syntax/constant/eval.rs:213-216`,
  `glass-lint-core/src/analysis/resolution/expression/static_values.rs:45-48`

The identical `Expr::TsAs` / `Expr::TsNonNull` / `Expr::TsSatisfies` /
`Expr::TsTypeAssertion` unwrap arm block is repeated verbatim in eight
match sites (four within this chunk, four outside:
`analysis/facts/functions.rs:291-294`, `analysis/facts/calls/callee.rs:195-198`
and `:226-229`, `analysis/resolution/expression.rs:284-287`). The root cause is
documented in `names.rs:117-121`: `effective_terminal_expr` deliberately leaves
"TypeScript assertion wrappers … terminal here", so every consumer re-implements
the peel with its own recursion target. Adding or renaming a transparent wrapper
(TS has at least `TsInstantiation`) silently requires updating all eight sites;
`scope/query/rooted.rs:51-59` currently omits the block, showing policy drift is
already possible.

**Recommendation:** Add a single `analysis::syntax` helper (owner
`crate::analysis::syntax`) such as
`pub(in crate::analysis) fn peer_ts_assertion(expr: &Expr) -> Option<&Expr>`
that returns the inner expression for the four wrapper variants, and replace
the repeated arm blocks in all eight sites. Do **not** fold the unwrap into
`effective_terminal_expr`: `rooted.rs` intentionally does not peel assertions,
so a global change would alter which expressions yield a rooted chain and could
change match certainty. Each consumer keeps its current transparency policy;
only the shared peeling step is consolidated. Sequence handling (`Expr::Seq`
"last expression") is a separate, per-caller policy and stays untouched.

**Fix Applied:** None so far.

#### [ ] READ-002 — Duplicated bounded object merge in the constant evaluator

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/syntax/constant/eval.rs:277-285`,
  `glass-lint-core/src/analysis/syntax/constant/eval.rs:335-343`

The object-spread arm of `evaluate_object` and the `Object.assign` loop of
`evaluate_object_assign` perform the same bounded merge twice: destructure a
`ConstValue::Object`, fail to `Unknown` otherwise, check
`values.len().saturating_add(added.len()) > MAX_OBJECT_KEYS`, then extend.
The distinct-key bound invariant is thus encoded in two places inside one
module; a future merge site (e.g. a new spread-like call shape) can forget the
capacity check and silently grow an object past the evaluator's own bound.

**Recommendation:** Extract one narrow helper owned by the constant domain
(in `constant/types.rs` or `constant/eval.rs`), e.g.
`fn merge_bounded(target: &mut BTreeMap<SmolStr, ConstValue>, added: BTreeMap<SmolStr, ConstValue>) -> bool`
encoding exactly the saturating-add capacity check plus extend, and call it
from both sites. Guardrail: preserve the exact fail-closed behavior and the
saturating-add math; keep `ConstValue::object`'s constructor check and
`bounded()` re-admission as the two documented outer layers of the same bound.

**Fix Applied:** None so far.

#### [ ] READ-003 — Shorthand-property arm manually mirrors evaluator budget accounting

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/syntax/constant/eval.rs:290-296`

The `Prop::Shorthand` arm in `evaluate_object` re-implements
`EvalState::evaluate`'s accounting inline: `consume_node()`, then
`lookup_ident(...)`, then a manual `self.depth -= 1`. This is the only place in
the file that mutates `depth`/`nodes` outside `evaluate` and `consume_node`, so
the node/depth invariant for a budgeted node has two owners. If `evaluate`'s
accounting ever changes (e.g. an early-return path or a renamed phase), this arm
drifts without any compiler error, changing which oversize object literals fail
closed.

**Recommendation:** Extract a private `EvalState` method, e.g.
`fn evaluate_ident(&mut self, lookup: &impl Lookup, ident: &Ident) -> ConstValue`,
that performs exactly the `consume_node` + `lookup_ident` + `depth -= 1`
sequence the shorthand arm needs (mirroring `evaluate`), and call it from the
shorthand arm. Guardrail: shorthand property keys must keep charging one
node/depth unit plus one lookup so object literals with many shorthand keys
observe the same `MAX_DEPTH`/`MAX_NODES`/`MAX_LOOKUPS` bounds as other shapes.

**Fix Applied:** None so far.

## Systemic Themes

- **Evidence layering is a real seam, not a duplicate.** `QualifiedEvent` +
  `TraceArena` + `TraceStep` (internal, fact-id based, deferred) and
  `project::types::report::evidence::{EvidenceStep, EvidenceTrace}` (external,
  message + location) do not overlap with `matching::evidence`'s occurrence
  grouping or the report materialization at
  `lint/report/evidence.rs:172-194`. The `EvidenceRole` enum is correctly
  shared (`project` types) so the two layers speak the same vocabulary while
  owning different state. No consolidation recommended.
- **Parallel representations are deliberately distinct.** `ConstValue`
  (syntax-shape tree) vs `model::value::Value`/`ValueTable` (interned arena)
  is bridged by documented bounded conversions
  (`resolution/constant.rs`, `model/static_properties.rs::to_const_object`);
  `SymbolCallProvenance` (call-site outcome) vs `model::scope::BindingProvenance`
  (binding lifecycle) model different lifecycles; `expression_name` vs
  `rooted_expr_chain` serve different identity contracts. These are
  boundaries, not unification candidates.
- **`EvalNode` and `NoLookup` are justified.** `EvalNode` exists because the
  resolver passes bare `&Expr`/`&BinExpr`/`&Tpl`
  (`resolution/expression.rs:83,281,297`) while sharing one budget;
  `NoLookup` is a zero-sized `Lookup` implementation, not a flag hack, and it
  lets `literal_member_property_name` reuse the contextual member-name evaluator
  with no name resolution. Both kept as-is.
- **`TransparentTerminal` is appropriately scoped.** Its two variants and the
  explicit "sequences and TS assertions remain terminal here" contract
  (`names.rs:117-121`) are what let `expression_name`, `rooted_expr_chain`, and
  `rooted_expr_chain_with` share one walker; the per-consumer divergence on
  sequences and TS assertions is documented policy and is the root cause of
  READ-001.
- **Bound reinforcement is mostly intended.** Repeated array/object/string
  bound checks (syntax construction, `ConstValue::array`/`object`, and
  `ConstValue::bounded()` re-admission) are documented defense-in-depth in
  `types.rs:86-134`; the two genuinely redundant merge checks are READ-002.

## Open Questions

1. `Resolver::const_value` (`resolution/constant.rs:23-57`) bounds the
   `ValueId -> ConstValue` read path by `MAX_DEPTH` only, with no node or
   lookup accounting, while `.bounded()` (node capped) is applied only when
   interning constants back. With per-level container fanout of
   `MAX_ARRAY_ITEMS`/`MAX_OBJECT_KEYS` (256) and 32 depth levels, the read path
   can materialize far more nodes than the syntax evaluator would admit. Is this
   asymmetry intentional (values are already bounded by `MAX_VALUES` at
   interning), or should the read path also charge a node budget?
2. `UnknownReason` (`syntax/provenance.rs:11-22`) carries five variants and a
   `BudgetExhausted { limit }` payload, but every consumer matches it as a
   blanket fail-closed `Unknown(_)` (e.g.
   `matching/build.rs:195,316`, `project/linker/export.rs:240`,
   `project/identities.rs:93`); only resolver unit tests inspect the precise
   reasons. Is the taxonomy meant to feed future diagnostics, or is it
   currently over-built bookkeeping?
3. `EvalNode::Binary`/`EvalNode::Template` (`eval.rs:175-183`) duplicate the
   `Expr::Bin`/`Expr::Tpl` arms (`eval.rs:197-200`); the wrapper is justified
   by the resolver's bare-node call sites, but the Add/template semantics now
   have two dispatch paths that must stay in sync. Would separate
   `evaluate_binary`/`evaluate_template` entry points be clearer?
4. `name.rs::MAX_NAMES` re-derives `DEFAULT_MAX_NAMES` with a compile-time pin
   because the bound is excluded from the artifact cache key. Deliberate, but
   could the pin live in `glass-lint-datastructures` so the second constant and
   its assert never diverge from the real default?
5. The `Lookup` service is threaded through ~10 `EvalState` methods (e.g.
   `evaluate_inner`, `evaluate_template`, `evaluate_add`, `evaluate_object`,
   `evaluate_object_assign`, both `contextual_*_property_name`). Binding it into
   `EvalState<'a, L: Lookup>` would remove the repeated parameter but ripple
   through every `Lookup` impl (`FrozenScopeGraph`, `Resolver`, `ScopeCollector`).
   Deferred as a conscious trade-off; revisit if the parameter count grows.

## Coverage

- `glass-lint-core/src/analysis/syntax/mod.rs` (28 lines) — re-exports,
  `span_contains`.
- `glass-lint-core/src/analysis/syntax/name.rs` (13 lines) — `MAX_NAMES` pin.
- `glass-lint-core/src/analysis/syntax/names.rs` (367 lines) — root/callee/
  terminal walkers, `TransparentTerminal`, `literal_property_name`,
  `member_expression_chain`, `expression_name`, builtin function detection.
- `glass-lint-core/src/analysis/syntax/provenance.rs` (68 lines) —
  `UnknownReason`, `SymbolCallProvenance`, `SymbolMemberProvenance`.
- `glass-lint-core/src/analysis/syntax/constant/mod.rs` (19 lines) —
  re-exports and `static_string`.
- `glass-lint-core/src/analysis/syntax/constant/types.rs` (171 lines) —
  `ConstValue`, bounds, `non_negative_integer`, `ScalarPropertyText`.
- `glass-lint-core/src/analysis/syntax/constant/eval.rs` (414 lines) —
  `EvalState`, `EvalNode`, `Lookup`, `NoLookup`, `evaluate` family.
- `glass-lint-core/src/analysis/syntax/constant/tests.rs` (173 lines).
- `glass-lint-core/src/analysis/trace.rs` (205 lines) — `TraceArena`,
  `TraceNodeId`, `QualifiedEvent`, `TraceStep`.
- `glass-lint-core/src/analysis/trace/tests.rs` (125 lines).
- Callers traced: `resolution/constant.rs`, `resolution/expression.rs`,
  `resolution/expression/static_values.rs`, `resolution/call.rs`,
  `scope/static_value.rs`, `scope/query/constants.rs`, `scope/query/rooted.rs`,
  `scope/query/provenance/callable.rs`, `model/static_properties.rs`,
  `model/fact.rs`, `facts/{arguments,functions,calls/callee}.rs`,
  `flow/cross/*`, `flow/projector/evidence.rs`, `analysis/project/projection.rs`,
  `api/classification.rs`, `matching/{build,evidence,arguments/evaluator}.rs`,
  `lint/report/{mod,evidence}.rs`, `project/types/report/evidence.rs`.

Only `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_06.md` was created; no source,
test, configuration, or documentation was modified.