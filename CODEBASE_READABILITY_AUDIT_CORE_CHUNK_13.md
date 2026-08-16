# Codebase Readability Audit — `glass-lint-core` Chunk 13 (Module requests and resolution)

## Summary

Chunk 13 covers module-request recognition (`analysis::module_request`) and bounded
semantic value resolution (`analysis::resolution::{self,call,constant,expression,
expression::static_values}`). The design is disciplined: fail-closed unknown vs.
resolved-empty is preserved everywhere (`ValueId::UNKNOWN`, `ConstValue::Unknown`,
`UnknownReason`), resolution is position-keyed and recursion-guarded, and the guard
state machine (`ResolutionStart`/`ResolutionGuard`) is sound. The main readability
debt is duplication across phase boundaries: `Resolver` re-implements a `Lookup`
adapter that the frozen scope graph already owns, transparent expression peeling and
`.bind` shape detection are hand-copied in several modules, and `ResolutionProvenance`
is constructed and rebuilt in five places with destructure–rebuild round-trips.
`Resolver` is a genuine coordinator (its forwarding methods are consumed by the facts
builder), not a god object; it should stay one owner, but its hand-written delegations
should be reduced.

7 findings: READ-001 … READ-007. No fixes applied.

## Findings

### Resolution (`analysis::resolution`) — value provenance and constant adaptation

#### [ ] READ-001 — `Lookup for Resolver` re-implements the frozen scope graph's constant adapter

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:186-205`; `glass-lint-core/src/analysis/scope/query/constants.rs:24-48`

`impl Lookup for Resolver` supplies `ident`, `member`, `spread`, and `unshadowed_global`
by re-pronouncing constant evaluation decisions that `analysis::scope` already owns.
`spread` (mod.rs:191-196 → `mutable_static_object_at` + `state.evaluate`) and
`unshadowed_global` (mod.rs:202-204 → `scopes.unshadowed_global_at`) are line-for-line
the `FrozenScopeGraph` arms at scope/query/constants.rs:38-48; `member` (mod.rs:198-200)
only forwards to the graph's default `Lookup::member`; `ident` (mod.rs:187-189) is a
parallel derivation of the same lexical constant via `ident_value_seed(ident).constant`
that duplicates the graph's `definite_binding_at` + `provenance_to_const_value` arm.
Two implementations of the same semantic lookup invite divergence under shadowing,
dynamic lookups, and `eval` scopes. Because `intern_evaluated` (expression.rs:88-94),
`static_string_array_expr` (static_values.rs:16-24), and `ModuleRequestContext::static_string`
(call.rs:181-183) all pass `self` into `syntax::constant::evaluate`, the entire
`impl Lookup for Resolver` must stay in sync by hand.

**Recommendation:** Make `FrozenScopeGraph` the single owner of the constant
`Lookup`. Replace its `ident` arm (query/constants.rs:26-35) with the joined
`ident_value_seed(ident).constant` projection (the PERF comment at the seed, one keep
line) or fix any real divergence in a unit test; keep `member`, `spread`,
`unshadowed_global` there; then change evaluation call sites in `resolution` to pass
`&self.scopes` instead of `self` and delete `impl Lookup for Resolver`
(mod.rs:186-205). Guardrails: preserve the dynamic-lookup guard, the
mutable-static-object spread rejection, the `EvalState`-shared `member` behavior, and
the distinction between `ConstValue::Unknown` and empty containers; run shadowing and
`eval` adversarial negatives in `syntax::constant/tests.rs` and
`resolution/tests.rs` before and after.

**Fix Applied:** None so far.

#### [ ] READ-002 — `ResolutionProvenance` is hand-built and destructure-rebuilt in five places

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:46-85`; `glass-lint-core/src/analysis/resolution/expression.rs:42-68, 148-171, 209-249, 330-363`

The six-field `ResolutionProvenance` is constructed in five places: `local()`
(mod.rs:63-73), `with_call()` (mod.rs:75-85), inline struct literals in `resolve_ident`
(expression.rs:161-168) and `resolve_member` (expression.rs:239-246), and — most
tellingly — `ResolutionSeed::into_resolved` (expression.rs:42-68), which destructures a
whole provenance only to rebuild the same struct with `call` and `module_member`
replaced. Adding any provenance field or changing its defaults silently requires
updating all five sites. The seed also stores *provisional* `call`/`module_member`
copies (resolve_ident:163, resolve_member:220,241) even though `finalize_seed`
(expression.rs:330-363) recomputes both through `call_provenance_at`, so the 
"destructure, swap two fields, rebuild" ceremony encodes a phase transition that the
owning type should express.

**Recommendation:** Move the swap onto `ResolutionProvenance` in `analysis::resolution`
(mod.rs) as a small `with_call_identity(self, call, module_member) -> Self` (or a
`seal`-style builder) and have `into_resolved` call it; keep the `local()`/`with_call()`
constructors as the only two "empty defaults" entry points, and express the
`resolve_ident`/`resolve_member` literals through one owned constructor that takes the
seed's remaining fields. Guardrails: keep the six fields' default-absent/local
invariants centralized so a future field cannot accidentally inherit provenance; keep
`provisional_id` driving the intern-then-finalize order, and add a test that a new
field stays `None` through `resolve_ident`, `resolve_member`, and the finalize path.

**Fix Applied:** None so far.

### Module requests (`analysis::module_request`) and call/expression resolution

#### [ ] READ-003 — Transparent expression peeling is re-implemented in three resolvers

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:251-291`; `glass-lint-core/src/analysis/resolution/expression/static_values.rs:26-55`; `glass-lint-core/src/analysis/module_request.rs:143-158`; `glass-lint-core/src/analysis/syntax/names.rs:122-138`

The same "unwrap transparent expression shells" logic is hand-written in three
analysis modules with only partial centralization. `effective_terminal_expr`
(names.rs:122-138) centralizes Member/Call/OptChain/Paren but deliberately leaves
Seq and the four TypeScript assertion wrappers terminal, so every consumer re-copies
them: `resolve_expr` (expression.rs:262-265, 284-287), `rooted_expr_chain`
(static_values.rs:41-48), and `expression_name` (names.rs:147-150) each repeat the
TsAs/TsNonNull/TsSatisfies/TsTypeAssertion four-arm peel, and `recognize_module_expression`
(module_request.rs:148-156) repeats the Paren/Seq peel for a third audience. Three
modules drift whenever a new transparent shape (e.g. `TsAsExpression` wrappers, chain
shells) is added.

**Recommendation:** Add one `analysis::syntax` helper that peels the shapes every
resolution domain treats as transparent (Paren, Seq-last, and the four TS assertion
wrappers) down to a core expression, and route `resolve_expr`, `rooted_expr_chain`,
and `recognize_module_expression` through it (they already rely on
`effective_terminal_expr` for the Member/Call/OptChain half). Guardrails: do not fold
call-callee transparency into the helper (`resolve_expr` must resolve `foo()` as a call
value, not skip it); preserve `Seq.exprs.last()` semantics and the discriminator of the
existing `ResolutionStart` guard; keep `recognize_module_expression`'s Member-object
descent separate because it descends receivers, not callables.

**Fix Applied:** None so far.

#### [ ] READ-004 — `.bind` call shape detection is repeated across four modules

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/resolution/call.rs:79-83`; `glass-lint-core/src/analysis/scope/build/provenance.rs:66-67, 150-152, 233-241`; `glass-lint-core/src/analysis/scope/build/analysis/classification.rs:236-242`; `glass-lint-core/src/analysis/facts/calls/callee.rs:258-266`

The "callee is `x.bind(...)`" predicate and the literal `"bind"` string are re-derived
in four modules: `resolve_call_expression` (call.rs:79-83), `bound_callable_provenance`
and `returned_object_from_callee` (provenance.rs:150-152, 235-241) plus the "bind"
property branch at provenance.rs:65-67, `callee_is_bind_call`
(classification.rs:236-242), and the instance-callable `bind` arm
(facts/calls/callee.rs:261). Six comparisons of the same member shape with no shared
predicate; a rename of the interop method family (or a spelling/literal-property change)
would need edits in all four files.

**Recommendation:** Add a `syntax` helper, e.g. `is_bind_member(member)` next to the
existing `is_function_constructor_member` (names.rs:203-206), returning
`literal_member_property_name(&member.prop).as_deref() == Some("bind")`, used by all
listed call sites. Guardrails: keep it a pure shape check — do not fold bound-argument
validation (`bound_callable_provenance`) or `this`-receiver checks
(facts/calls/callee.rs:264) into it; the `provenance.rs:65-67` branch compares a member
name inside `module_alias_provenance` and may call the same helper but keep its
`Export` chaining semantics.

**Fix Applied:** None so far.

#### [ ] READ-005 — `resolve_call_expression` runs the full module-request recognizer for one variant

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/resolution/call.rs:52-72, 172-184`; `glass-lint-core/src/analysis/module_request.rs:93-141`

`resolve_call_expression` calls `recognize_module_call(call, self,
ModuleRequestPolicy::alias_with_dynamic_import())` and discards every result that is not
`DynamicImport` (call.rs:57-71). For a `require('x')` call this re-runs the unshadowed
global check and string-literal check, then throws the request away; the wrapped-require
branch is unreachable for the resolver because its `ModuleRequestContext::is_unshadowed_wrapper`
(call.rs:177-179) is hard-coded `false`, so the `allows_interop_wrapper` policy bit is
configured but inert on this path. The policy enum (module_request.rs:28-59) is
legitimate for the scope collector and facts phases, but this call site silently depends
on only one of its variants.

**Recommendation:** Give the resolver a narrow entry, e.g. `recognize_dynamic_import_call`
(or a `recognize_module_call` with a `ModuleRequestKind` filter), and delete the
immediately-discarded require recognition for call expressions. Guardrails: keep exactly
the current dynamic-import criteria — first argument, non-spread, `static_string` through
the constant evaluator — and keep fail-closed behavior for every other call shape;
retain the full `ModuleRequestPolicy` for the collector (`alias`, `alias_with_dynamic_import`)
and facts (`interface`) call sites.

**Fix Applied:** None so far.

#### [ ] READ-006 — `ResolvedValue: Deref` and a public field create two access notations

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:94-125`

`ResolvedValue` exposes a `pub(super) provenance: Arc<ResolutionProvenance>` field and
also derefs to that same field (mod.rs:119-125), so consumers write both styles with no
policy: `resolved.call.clone()` via Deref (facts/calls/callee.rs:42, facts/visitor.rs:35,
static_values.rs:73) and `resolved.provenance.call.clone()` explicitly
(facts/arguments.rs:45,71; facts/construction.rs:39,59,63; static_values.rs:52,89). Two
equivalent spellings for the same six-field record make reads and mechanical refactors
ambiguous, and Deref-through-`Arc` hides the shared-ownership/`make_mut` behavior of the
provenance record.

**Recommendation:** Pick one access path. Keep the `pub(super) provenance` field (needed
for `Arc::make_mut` in `archive_unknown_with_reason` at static_values.rs:85-91 and by
`into_resolved`) and remove `impl Deref` (mod.rs:119-125), mechanically rewriting the ~8
Deref call sites to `resolved.provenance.<field>`. Guardrails: preserve `.id` field
access and the cheap `Arc` clone semantics of the resolved-value clone used by
`ResolutionStart::Cached` and `ResolutionGuard::commit`.

**Fix Applied:** None so far.

#### [ ] READ-007 — `Resolver::static_string_value` re-clones through `const_value` instead of the arena fast path

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:355-357`; `glass-lint-core/src/analysis/model/value.rs:280-285`

`Resolver::static_string_value` implements `self.const_value(id).string().map(str::to_owned)`,
which for a string id clones the `String` and, for a non-string id, materializes a whole
`ConstValue` (array/object clone trees) just to discard it at the `string()` filter.
`ValueTable::static_string(id)` (value.rs:280-285) already resolves binding chains via the
terminal cache and returns `Option<&str>` with no tree materialization. The two helpers
are equivalent for every id the caller passes (facts/interface/exports.rs:71,
facts/visitor.rs:179,231).

**Recommendation:** Implement `static_string_value` as
`self.values.static_string(id).map(str::to_owned)` (owners: resolver delegates to the
value arena), and keep `const_value` for the shapes that genuinely need full-tree
materialization. Guardrails: preserve binding-chain resolution semantics
(`ValueTable::resolve` follows the terminal cache exactly like `const_value`), and add a
guard assertion if a divergence between the two paths is ever suspected.

**Fix Applied:** None so far.

## Systemic Themes

- **Single artifact-bound adapter, hand-written delegations.** `Resolver` is the one
  coordinator from scope facts to matcher-facing values, and its forwarding API is
  justified by consumers in `facts`, `semantic`, and `interface`. The readability debt is
  not the wide surface but the *hand-written* re-delegations: the `Lookup` impl
  (READ-001) and `rooted_expr_chain` (README-003 half) re-derive scope behavior instead of
  passing `&self.scopes` through.
- **Separation between ownership and derivation.** Constant probing is owned by
  `scope::query`; `resolution` should consume it (READ-001), while provenance fields
  remain owned by `ResolutionProvenance` (READ-002). Resolver should delegate, not
  re-implement.
- **Shape detection recurs at every phase boundary.** Module-request recognition, call
  resolution, and scope collection each hand-verify the membership of `bind` calls
  (READ-004) and transparent expression shells (READ-003), because
  `effective_terminal_expr` is deliberately conservative and `syntax` has no bind member
  predicate. One new helper each eliminates the drift.
- **Fail-closed discipline holds throughout.** Unknown stays distinct from resolved-empty
  (`ValueId::UNKNOWN` vs interned values, `ConstValue::Unknown` vs empty array/object),
  cycles and unresolved positions return `Unknown` without caching, and budget-exhausted
  is reported as a distinct `UnknownReason` from unsupported. No finding proposes
  collapsing those distinctions.

## Open Questions

1. `finalize_seed` (expression.rs:331-348) re-interns a plain `Value::Global` and replaces
   `seed.provisional_id` when `call_provenance_at` yields a `Global` provenance. When the
   provisional value was a `Binding { key, target: Global }` (an alias such as
   `const w = window`), the binding-wrapped entry remains in the arena unreachable from
   the final resolved value. Was the lost `BindingKey` association intended, and does any
   `BindingSlot`/reassignment consumer (`facts::assignments`) depend on it? If not, the
   bind-then-replace sequence is redundant work per alias-to-global resolution.
2. `Lookup for Resolver::ident` (`ident_value_seed(ident).constant`) and
   `Lookup for FrozenScopeGraph::ident` (`definite_binding_at` + `provenance_to_const_value`)
   are two projections of the same lexical constant that could diverge
   (`preferred_witness` vs `definite_binding_at`). Is any divergence intended, or should
   the joined seed be the single source (see READ-001)?
3. `ResolverCache` keeps two position-keyed maps — `fresh_values` (span → `ValueId`) and
   `resolved_values` (`ResolutionKey` → `ResolvedValue`) — plus a `resolving` guard set
   that covers only the latter. Call results are never cached under a resolution key
   (only the fresh object id is). Is the split deliberate so that fresh-object allocation
   bypasses the recursion guard, or can `fresh_object_value_at` be folded into the
   keyed cache for one cache architecture?
4. Does `ModuleRequestPolicy` need `alias()`'s dynamic-import exclusion at all, or is the
   three-variant policy masking that two consumers (`facts::interface` and the resolver)
   each use exactly one admission rule? This depends on whether the scope collector's
   `alias` path is exercised on dynamic imports elsewhere.
5. `is_unshadowed_require` differs between the two `ModuleRequestContext` impls
   (collector: `is_unbound("require")`; resolver: `scopes.unshadowed_unbound_at(...)`).
   Both implement position-sensitive shadowing; confirm they are intentionally
   equivalent before any consolidation.

## Coverage

- Inspected definitions: `analysis/module_request.rs` (+ `tests.rs`),
  `analysis/resolution/{mod.rs, call.rs, constant.rs, expression.rs,
  expression/static_values.rs, tests.rs}`.
- Inspected owners/peers for cross-checks: `analysis/model/value.rs`,
  `analysis/model/module.rs`, `analysis/syntax/names.rs`, `analysis/syntax/constant/*
  (mod.rs, types.rs, eval.rs)`, `analysis/scope/{static_value.rs, query/rooted.rs,
  query/constants.rs, query/provenance/{callable.rs,object.rs}, build/provenance.rs,
  build/analysis/classification.rs, build/constants.rs, graph.rs, name_env.rs}`.
- Inspected consumers: `analysis/facts/{mod.rs, visitor.rs, reads.rs, pattern.rs,
  arguments.rs, assignments.rs, functions.rs, construction.rs, control.rs,
  calls/{mod.rs, callee.rs, wrapper.rs}, interface/{mod.rs, exports.rs, commonjs.rs}}`,
  `analysis/semantic/mod.rs`, `analysis/project/resolver.rs`,
  `analysis/project/linker/{mod.rs,graph.rs}`, `analysis/project/identities.rs`,
  `analysis/facts/tests/build.rs`, `project/tests/session_and_link_validation.rs`.
- Traced runtime behavior: recursion guard (`start_resolution`/`ResolutionGuard::commit`/
  `Cycle`), position keying (`ParserSpanKey`/`ResolutionKey`), `finalize_seed` provenanc
  computation and global re-intern, fresh-object caching, and the `Lookup`/constant
  binding-chain paths.
- Not re-reported (verified already distinct or healthy): the sealed `ResolutionStart`
  state machine, `RecognizedModuleRequest`/`ModuleRequestKind` roles vs
  `model::module::ModuleRequestRole` interface handling, and bounded bounds
  (`ConstValue::bounded`, `MAX_*` constants) on the arena conversion paths.