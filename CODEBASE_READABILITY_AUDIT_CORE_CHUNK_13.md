# Codebase Readability Audit — `glass-lint-core` Chunk 13 (Module requests and resolution)

## Summary

Chunk 13 covers module-request recognition (`analysis::module_request`) and bounded
semantic value resolution (`analysis::resolution::{self,call,constant,expression,
expression::static_values}`). The design is disciplined: fail-closed unknown vs.
resolved-empty is preserved everywhere (`ValueId::UNKNOWN`, `ConstValue::Unknown`,
`UnknownReason`), resolution is position-keyed and recursion-guarded, and the guard
state machine (`ResolutionStart`/`ResolutionGuard`) is sound. The main readability
debt is duplication across phase boundaries: `Resolver` re-implements a `Lookup`
adapter that the frozen scope graph already owns (four evaluation call sites pass the
resolver itself as the constant evaluator's lookup), transparent expression peeling and
`.bind` shape detection are hand-copied in several modules, and `ResolutionProvenance`
is constructed and rebuilt in five places with destructure–rebuild round-trips.
`Resolver` is a genuine coordinator (its forwarding methods are consumed by the facts
builder), not a god object; it should stay one owner, but its hand-written delegations
should be reduced. One cross-phase inconsistency stands out and is reported below: the
scope collector's `is_unshadowed_require` is looser than the resolver's equivalent
(no `with`/`eval` dynamic-lookup guard), which must be reconciled before the two module
request paths can share anything.

7 findings: READ-001 … READ-007. No fixes applied.

## Findings

### Resolution (`analysis::resolution`) — value provenance and constant adaptation

#### [x] READ-001 — `Lookup for Resolver` re-implements the frozen scope graph's constant adapter

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:186-205`; `glass-lint-core/src/analysis/scope/query/constants.rs:24-48`

`impl Lookup for Resolver` supplies `ident`, `member`, `spread`, and `unshadowed_global`
by re-pronouncing constant evaluation decisions that `analysis::scope` already owns.
`spread` (mod.rs:191-196 → `mutable_static_object_at` + `state.evaluate`) and
`unshadowed_global` (mod.rs:202-204 → `scopes.unshadowed_global_at`) are line-for-line
the `FrozenScopeGraph` arms at scope/query/constants.rs:38-43 and 46-48; `member`
(mod.rs:198-200) only forwards to the graph's default `Lookup::member` (the graph has no
override, so the trait default at syntax/constant/eval.rs:24-44 re-dispatches into the
graph's `ident`/`spread`/`unshadowed_global`). Only `ident` (mod.rs:187-189) is a real
re-derivation rather than a copy: it projects `ident_value_seed(ident).constant`, while
the graph's `ident` arm (constants.rs:26-35) re-runs `binding_resolution_at` and applies
`definite_binding_at`'s Complete-only filter — the two can diverge for joined bindings
(see Resolved Open Question 2). Two implementations of the same semantic lookup invite
divergence under shadowing, dynamic lookups, and `eval` scopes. Four evaluation call
sites pass the resolver itself into `syntax::constant::evaluate`: `intern_evaluated`
(expression.rs:92), `static_string_array_expr` (static_values.rs:17),
`ModuleRequestContext::static_string` (call.rs:182), and the facts-phase fallback in
`resolve_or_eval` (facts/arguments.rs:83) — so the entire `impl Lookup for Resolver`
must stay in sync with the graph by hand.

**Recommendation:** Make `FrozenScopeGraph` the single owner of the constant
`Lookup` and delete `impl Lookup for Resolver` (mod.rs:186-205). Unify the `ident` arm
on the joined seed: replace query/constants.rs:26-35 with
`ident_value_seed(ident).constant`, dropping the duplicated `binding_resolution_at`
search and the Complete-only filter that no other `Lookup` impl applies (both
`ScopeCollector::ident` at scope/build/constants.rs:17-23 and `Resolver::ident` are
preferred-witness projections); keep `member`, `spread`, and `unshadowed_global` there
unchanged; then change the evaluation call sites to pass `&self.scopes` (resolver side)
or `self.resolver.scopes` (facts side) instead of the resolver, and delete
mod.rs:186-205. Guardrails: preserve the dynamic-lookup guard, the
mutable-static-object spread rejection, the `EvalState`-shared `member` default, and
the distinction between `ConstValue::Unknown` and empty containers; keep
`definite_binding_at` (bindings.rs:48-57) for the strict provenance queries that must
stay fail-closed (`constructed_instance_at` bindings.rs:24, `module_export_for_chain`
callable.rs:249, `member_call_provenance_for_chain` callable.rs:284); pin the
joined-binding constant behavior — currently untested — with a resolution test before
and after (see Resolved Open Question 2), and pin the member-into-joined-object path
(the one observable change) with a negative test; run shadowing and `eval` adversarial
negatives in `syntax/constant/tests.rs` and `resolution/tests.rs`.

**Fix Applied:** Made `FrozenScopeGraph` the sole constant-evaluation lookup,
removed `Lookup for Resolver`, and routed resolver/facts constant evaluation
through the frozen graph. Its identifier lookup now uses the shared joined
`ident_value_seed`, while strict provenance queries retain their complete-only
checks. Added joined-binding and joined-object-member regression tests for the
preferred-witness and fail-closed behavior.

#### [x] READ-002 — `ResolutionProvenance` is hand-built and destructure-rebuilt in five places

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:46-85`; `glass-lint-core/src/analysis/resolution/expression.rs:42-68, 148-171, 209-249, 330-363`

The six-field `ResolutionProvenance` is constructed in five places: `local()`
(mod.rs:64-73), `with_call()` (mod.rs:75-84), the inline struct literals in `resolve_ident`
(expression.rs:161-168) and `resolve_member` (expression.rs:240-247), and — most
tellingly — `ResolutionSeed::into_resolved` (expression.rs:42-68), which destructures a
whole provenance only to rebuild the same struct with `call` and `module_member`
replaced. The seed's provisional `call`/`module_member` are genuinely load-bearing (the
provisional `call` is consulted by the budget-exhausted guard at expression.rs:331-335,
and the provisional `module_member` is the primary value on the member path — the
call-derived `ModuleNamespace` in `finalize_seed` is only an `or_else` fallback,
expression.rs:349-361), so the debt is the manual six-field transcription in
`into_resolved`, which re-lists fields the owning type should update in place. Adding any
provenance field or changing its defaults silently requires updating all five sites.

**Recommendation:** Move the swap onto `ResolutionProvenance` in `analysis::resolution`
(mod.rs) as a small `with_call_identity(self, call, module_member) -> Self` (or a
`seal`-style builder) and have `into_resolved` call it; keep the `local()`/`with_call()`
constructors as the only two "empty defaults" entry points, and express the
`resolve_ident`/`resolve_member` literals through one owned constructor that takes the
seed's remaining fields. Guardrails: keep the six fields' default-absent/local
invariants centralized so a future field cannot accidentally inherit provenance; keep
`provisional_id` driving the intern-then-finalize order and the budget-exhausted guard
unchanged; add a test that a new field stays `None` through `resolve_ident`,
`resolve_member`, and the finalize path.

**Fix Applied:** Centralized six-field provenance construction in
`ResolutionProvenance::from_parts` and moved finalized call/member replacement
to `with_call_identity`, eliminating the destructure/rebuild path in
`ResolutionSeed::into_resolved`. Added a focused test proving non-identity
provenance survives finalization.

### Module requests (`analysis::module_request`) and call/expression resolution

#### [x] READ-003 — Transparent expression peeling is re-implemented in three resolvers

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:251-291`; `glass-lint-core/src/analysis/resolution/expression/static_values.rs:26-55`; `glass-lint-core/src/analysis/module_request.rs:143-158`; `glass-lint-core/src/analysis/syntax/names.rs:122-138`

The same "unwrap transparent expression shells" logic is hand-written across the
resolution and module-request domains with only partial centralization.
`effective_terminal_expr` (names.rs:122-138) centralizes Member/Call/OptChain/Paren but
deliberately leaves Seq and the four TypeScript assertion wrappers terminal (documented
at names.rs:117-121), so every consumer re-copies them: `resolve_expr` peels Paren
(expression.rs:255), Seq (expression.rs:262-265), and the four TsAs/TsNonNull/TsSatisfies/
TsTypeAssertion arms (expression.rs:284-287); `rooted_expr_chain` peels Seq
(static_values.rs:41-44) and the same four TS arms (static_values.rs:45-48);
`expression_name` repeats the four TS arms (names.rs:147-150); and
`recognize_module_expression` repeats the Paren/Seq peel (module_request.rs:151-155) for
a further audience. A fifth copy of the Paren/Seq peel lives in the scope normalization
adapter `unwrap_scope_expression` (scope/expression.rs:75-83). Note that `resolve_expr`
does *not* rely on `effective_terminal_expr` (it matches the shells directly), so the
Member/Call/OptChain half is only shared by `rooted_expr_chain` and `expression_name`.
Four modules drift whenever a new transparent shape (e.g. a further `TsAsExpression`
wrapper, or a chain shell) is added.

**Recommendation:** Add one `analysis::syntax` helper that peels the shapes every
resolution domain treats as transparent (Paren, Seq-last, and the four TS assertion
wrappers) down to a core `&Expr`, and route `resolve_expr`, `rooted_expr_chain`, and
`recognize_module_expression` through it (`rooted_expr_chain` already relies on
`effective_terminal_expr` for the Member/Call/OptChain half; `recognize_module_expression`
keeps its own Member-object descent and gains only the shell peel). The `expression_name`
TS arms (names.rs:147-150), living in the same `syntax` module, collapse onto the same
helper. Routing
`recognize_module_expression` through the helper also admits TS-wrapped module requests
such as `(import('x') as any)` — a consistency gain, since the constant evaluator already
peels TS wrappers for dynamic-import specifiers (eval.rs:213-216). Guardrails: do not
fold call-callee transparency into the helper (`resolve_expr` must resolve `foo()` as a
call value, not skip it; call-callee transparency stays in `effective_terminal_expr`);
preserve `Seq.exprs.last()` semantics (empty sequences stay unknown/fail closed) and the
discriminator of the existing `ResolutionStart` guard; keep `recognize_module_expression`'s
Member-object descent separate because it descends receivers, not callables; pin the
newly accepted TS-wrapped shapes with a positive test and require/dynamic-import
negatives so the acceptance is deliberate rather than accidental.

**Fix Applied:** Added the shared `unwrap_transparent_expr` implementation for
parentheses, sequence-last, and TypeScript assertion wrappers, and routed
`resolve_expr`, `rooted_expr_chain`, and `recognize_module_expression` through it.
`expression_name` reuses the same recursive implementation while retaining its
intentional sequence-terminal behavior. Added a TypeScript-wrapped dynamic-import
positive test and a dynamic require-name negative test.

#### [x] READ-004 — `.bind` call shape detection is repeated across four modules

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/resolution/call.rs:79-83`; `glass-lint-core/src/analysis/scope/build/provenance.rs:65-67, 92-94, 150-152, 235-241`; `glass-lint-core/src/analysis/scope/build/analysis/classification.rs:236-242`; `glass-lint-core/src/analysis/facts/calls/callee.rs:254-266`

The "callee/property is `bind`" predicate and the literal `"bind"` string are re-derived
in seven comparisons across six functions in four files: `resolve_call_expression`
(call.rs:79-80), the member branch and the call branch of `module_alias_provenance`
(provenance.rs:65-67 compares `property == "bind"` and provenance.rs:92-94 compares
`literal_property.as_deref() == Some("bind")`), `bound_callable_provenance`
(provenance.rs:150-151), `returned_object_from_callee` (provenance.rs:235-236),
`callee_is_bind_call` (classification.rs:236-242), and the instance-callable `bind` arm
(facts/calls/callee.rs:261). Five of the seven compare `literal_member_property_name(&prop)`
against `"bind"` (call.rs:79, provenance.rs:150, provenance.rs:235, classification.rs:240,
callee.rs:261); the two `module_alias_provenance` sites (provenance.rs:65-67, 92-94)
compare a `ScopeExpression` `literal_property` (`SmolStr`) instead. A rename of the
interop method family (or a spelling/literal-property change) would need edits in all
four files.

**Recommendation:** Add a name-level predicate in `analysis::syntax`, e.g.
`is_bind_property(name: &str) -> bool` (or a `BIND_MEMBER: &str` constant), next to the
existing `is_function_constructor_member` (names.rs:202-206). The five `MemberExpr` sites
use `is_bind_property(literal_member_property_name(&member.prop))`; the two
`module_alias_provenance` sites compare their `SmolStr` property name against the same
predicate. Guardrails: keep it a pure shape check — do not fold bound-argument validation
(`bound_callable_provenance`) or `this`-receiver filtering (facts/calls/callee.rs:264)
into it; the `provenance.rs:65-67` branch must keep its `Export` chaining semantics and
the provenance.rs:92-94 branch its `.then(...).flatten()` structure when the predicate is
shared.

**Fix Applied:** Added the shared syntax-level `is_bind_property` predicate
and routed all seven member/property shape checks through it, leaving bound
argument validation and receiver filtering at their existing owners. Verified
with `make fmt && make ci`.

#### [x] READ-005 — `resolve_call_expression` runs the full module-request recognizer for one variant

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/resolution/call.rs:53-89, 172-184`; `glass-lint-core/src/analysis/module_request.rs:28-59, 93-141`

`resolve_call_expression` calls `recognize_module_call(call, self,
ModuleRequestPolicy::alias_with_dynamic_import())` and discards every result that is not
`DynamicImport` (call.rs:57-71). For a `require('x')` call this re-runs the
unshadowed-global check and string-literal check, then throws the request away; the
interop-wrapper branch is unreachable on this path because `Resolver`'s
`ModuleRequestContext::is_unshadowed_wrapper` (call.rs:177-179) is hard-coded `false`, so
the `allows_interop_wrapper` policy bit is configured but inert here. The policy enum
(module_request.rs:28-59) is legitimate — all three variants are live elsewhere
(`Interface` at facts/mod.rs:286, `Alias` at classification.rs:132,177, and
`AliasWithDynamicImport` at provenance.rs:80 — see Resolved Open Question 4) — but this
call site silently depends on only one of them and pays the other two admission checks.

**Recommendation:** Give the resolver a narrow entry, e.g. `recognize_dynamic_import_call`
(the `Callee::Import` guard from module_request.rs:98-103 in front of the existing
`dynamic_import` helper at module_request.rs:160-173), and delete the
immediately-discarded require/interop recognition from the call-expression path.
Guardrails: keep exactly the current dynamic-import criteria — first argument, non-spread,
`static_string` through the constant evaluator — and keep fail-closed behavior for every
other call shape; retain the full `ModuleRequestPolicy` for the collector
(`alias`, `alias_with_dynamic_import`) and facts (`interface`) call sites.

**Fix Applied:** Added the narrow `recognize_dynamic_import_call` entry point
and routed resolver call-result handling through it, removing discarded
require/interop recognition while preserving the policy-based recognizer for
collector and interface callers. Verified with `make fmt && make ci`.

#### [x] READ-006 — `ResolvedValue: Deref` and a public field create two access notations

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:94-125`

`ResolvedValue` exposes a `pub(super) provenance: Arc<ResolutionProvenance>` field
(mod.rs:98) and also derefs to that same field (mod.rs:119-125), so consumers write both
styles with no policy. The Deref form is the majority: `resolved.call.clone()` /
`resolved.rooted_chain` / `resolved.module_member` / `resolved.returned_member` /
`resolved.bound_arguments` at facts/calls/callee.rs:42,44-47, facts/visitor.rs:35,
facts/construction.rs:31,47, facts/reads.rs:19-21, and static_values.rs:73; the explicit
form `resolved.provenance.<field>` appears at facts/arguments.rs:45,71,
facts/construction.rs:39,59,63, facts/calls/mod.rs:37, and static_values.rs:52,89. Two
equivalent spellings for the same six-field record make reads and mechanical refactors
ambiguous, and Deref-through-`Arc` hides the shared-ownership/`make_mut` behavior of the
provenance record.

**Recommendation:** Pick one access path. Keep the `pub(super) provenance` field (needed
for `Arc::make_mut` in `archive_unknown_with_reason` at static_values.rs:85-91 and by
every explicit `.provenance.<field>` call site) and remove `impl Deref` (mod.rs:119-125),
mechanically rewriting the ~12 Deref call sites to `resolved.provenance.<field>`.
Guardrails: preserve `.id` field access and the cheap `Arc` clone semantics of the
resolved-value clone used by `ResolutionStart::Cached` (expression.rs:321-322) and
`ResolutionGuard::commit` (expression.rs:33-39); the rewrite must not reorder evidence or
change the provenance record's sharing.

**Fix Applied:** Removed `ResolvedValue`'s `Deref` implementation and
rewrote all provenance reads to use the explicit `resolved.provenance` field.
The `Arc` ownership and clone paths remain unchanged, including cached
resolution commits and unknown-value mutation. Verified with
`make fmt && make ci`.

#### [x] READ-007 — `Resolver::static_string_value` re-clones through `const_value` instead of the arena fast path

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:355-357`; `glass-lint-core/src/analysis/model/value.rs:280-285`

`Resolver::static_string_value` implements `self.const_value(id).string().map(str::to_owned)`,
which for a string id clones the `String` and, for a non-string id, materializes a whole
`ConstValue` (array/object clone trees via `const_value_depth` at constant.rs:27-57) just
to discard it at the `string()` filter.
`ValueTable::static_string(id)` (value.rs:280-285) already resolves binding chains via the
terminal cache and returns `Option<&str>` with no tree materialization. The two helpers
are equivalent for every id the callers pass (facts/interface/exports.rs:71,
facts/visitor.rs:179,231; both follow the terminal cache — `ValueTable::resolve` at
value.rs:259-262 vs `const_value_depth` at constant.rs:32), so the tree materialization
is pure waste on every non-string call.

**Recommendation:** Implement `static_string_value` as
`self.values.static_string(id).map(str::to_owned)` (owners: resolver delegates to the
value arena), and keep `const_value` for the shapes that genuinely need full-tree
materialization. Guardrails: preserve binding-chain resolution semantics (`static_string`
follows the terminal cache exactly like `const_value`); the residual `String` clone is
required because the method returns an owned value; add a `debug_assert` comparing the
two paths if a divergence is ever suspected.

**Fix Applied:** Changed `Resolver::static_string_value` to delegate to the
value arena's terminal-cache-backed `static_string` lookup, retaining only the
required owned-string clone. The full-tree `const_value` conversion and its
iterators remain available for resolution unit tests without adding dead
production methods. Verified with `make fmt && make ci`.

## Systemic Themes

- **Single artifact-bound adapter, hand-written delegations.** `Resolver` is the one
  coordinator from scope facts to matcher-facing values, and its forwarding API is
  justified by consumers in `facts`, `semantic`, and `interface`. The readability debt is
  not the wide surface but the *hand-written* re-delegations: the `Lookup` impl
  (READ-001) re-derives scope behavior instead of passing `&self.scopes` through, and the
  four constant-evaluation call sites (expression.rs:92, static_values.rs:17, call.rs:182,
  facts/arguments.rs:83) each accept the resolver as the lookup, so the hand copy must
  track the graph forever.
- **Separation between ownership and derivation.** Constant probing is owned by
  `scope::query`; `resolution` should consume it (READ-001), while provenance fields
  remain owned by `ResolutionProvenance` (READ-002) and value identity by `ValueTable`
  (READ-007). Resolver should delegate, not re-implement — including the
  alias-to-global canonicalization, which already resolves to the shared bare
  `Value::Global` regardless of the provisional binding wrapper (Resolved Open Question 1).
- **Shape detection recurs at every phase boundary.** Module-request recognition, call
  resolution, scope collection, and scope normalization each hand-verify the membership
  of `bind` calls (READ-004, seven comparisons in six functions) and transparent
  expression shells (READ-003, four plus the `scope/expression.rs` Paren/Seq adapter),
  because `effective_terminal_expr` is deliberately conservative and `syntax` has no
  bind-member predicate. One helper each eliminates the drift.
- **Fail-closed discipline holds throughout, with one cross-phase exception.** Unknown
  stays distinct from resolved-empty (`ValueId::UNKNOWN` vs interned values,
  `ConstValue::Unknown` vs empty array/object), cycles and unresolved positions return
  `Unknown` without caching, and budget-exhausted is reported as a distinct
  `UnknownReason` from unsupported. No finding proposes collapsing those distinctions.
  The exception is the scope collector's `is_unbound` (scope/build/assignments.rs:213-215),
  which lacks the `with`/`eval` dynamic-lookup guard that the resolver applies in
  `unshadowed_unbound_at` (bindings.rs:186-189) — the two `is_unshadowed_require` impls
  diverge inside dynamic scopes and must be reconciled to the stricter standard before
  the module-request paths are consolidated (Resolved Open Question 5).

## Open Questions — Resolved

1. **The lost `BindingKey` association on alias-to-global resolution is intentional
   canonicalization; the bind-then-replace sequence is redundant work and can be
   trimmed.** Evidence: `finalize_seed` (expression.rs:343-348) re-interns bare
   `Value::Global(name)` with `binding: None` whenever `call_provenance_at` yields
   `Global`, so the final resolved id never carries the provisional binding. The only
   binding-slot consumer in the crate is the flow projector's `value_aliases`
   (flow/projector/driver.rs:413-414), and it operates on the *final* resolved ids from
   the facts stream — which are already bare globals — while `facts::assignments` never
   reads `Value::Binding` at all. The orphaned `Value::Binding { key, target: Global }`
   entry is therefore unobservable and wasted arena space per alias-to-global resolution.
   The minimal fix is in `intern_call_value`'s `Global` arm (call.rs:99-100): intern the
   bare `Value::Global` without the binding wrapper, making the `finalize_seed` re-intern
   idempotent (same id, since direct `window` and `const w = window` already share the
   bare global entry) and eliminating the orphan. The budget-exhausted guard
   (expression.rs:331-335) is unaffected. The `BindingKey` on *non-global* terminals is
   preserved and must stay (module-export aliases keep `seed.provisional_id` as their
   final id).
2. **The `ident` divergence (`preferred_witness` vs `definite_binding_at`) is real,
   untested, and the joined seed is the intended single source.** Evidence:
   `FrozenScopeGraph::ident` (constants.rs:26-35) re-runs `binding_resolution_at` and
   applies `definite_binding_at`'s Complete-only filter (bindings.rs:48-57), while
   `ident_value_seed` (callable.rs:114-159) projects `preferred_witness` with no status
   filter and carries the PERF note "Keep every projection on the same joined-binding
   result" (callable.rs:130-133). Both `ScopeCollector::ident` (build/constants.rs:17-23)
   and `Resolver::ident` (mod.rs:187-189) are preferred-witness projections, so the
   Complete-only filter is the outlier. No test pins either behavior for joined bindings
   (neither `resolution/tests.rs` nor `syntax/constant/tests.rs` contains a
   joined-constant case). Unify on the seed per READ-001, and pin the joined-binding
   constant behavior with a test; keep `definite_binding_at` for the strict provenance
   queries (`constructed_instance_at` bindings.rs:24, `module_export_for_chain`
   callable.rs:249, `member_call_provenance_for_chain` callable.rs:284), which remain
   fail-closed.
3. **The two-map `ResolverCache` split is deliberate: fresh-object allocation is a leaf
   and cannot participate in a cycle, so it correctly bypasses the recursion guard.**
   Evidence: `fresh_object_value_at` (static_values.rs:157-168) caches by `ParserSpanKey`
   in `fresh_values` and is reached only from leaf paths — `resolve_expr`'s `Call` and
   `New` arms (expression.rs:282,288) and `resolve_call_expression`'s fresh-object
   returns (call.rs:77,82). `fresh_object_value` → `allocate_object_id` →
   `intern_object_id` (static_values.rs:150-155,133-141) never recurses into
   ident/member resolution, so no fresh key can ever form a cycle and the `resolving`
   guard (mod.rs:149) has nothing to protect for them. Folding fresh values into
   `ResolutionKey` would require a third enum variant plus a guard entry that can never
   fire, and would conflate provenance-less leaf identities with provenance-bearing
   resolutions. The doc comment on `ResolverCache` (mod.rs:139-150) states exactly this
   split; keep it.
4. **All three `ModuleRequestPolicy` variants are live, so the policy is not masking
   anything; `alias()`'s dynamic-import exclusion is best read as an explicit "a dynamic
   import is never a `require`" contract rather than a load-bearing admission gate.**
   Evidence: `Interface` is used only at facts/mod.rs:286, `Alias` at classification.rs:132,177,
   and `AliasWithDynamicImport` at provenance.rs:80 (plus the resolver's call.rs:58).
   Removing the exclusion would not change any recorded binding: `classify_call`
   (classification.rs:125-159) would return `Require { module }` for `const x = import('m')`,
   but that fast path (`collect_require_aliases`, aliases.rs:91-99) records the same
   `BindingProvenance::ModuleNamespace` for an ident pattern that the current ModuleAlias
   candidate already records (classification.rs:173-175 → provenance.rs:80), and for
   destructuring patterns the ModuleAlias candidate falls back to `Require` anyway
   (classification.rs:227-232). The exclusion only selects the classification *form*
   (`Binding` vs `Require`) on the fast path, so keep all three variants — each consumer's
   admission rule is distinct — and let READ-005 remove only the resolver's one-variant
   call site.
5. **The two `is_unshadowed_require` impls are NOT intentionally equivalent — the
   collector is looser and must be brought up to the resolver's standard.** Evidence:
   `ScopeCollector::is_unshadowed_require` is `is_unbound("require")`
   (scope/build/provenance.rs:273-275), where `is_unbound` is
   `!has_issues() && visible_binding(name).is_none()` (assignments.rs:213-215) and
   `has_issues()` only reflects structural `ScopeCollectionIssue` variants
   (ShapeMismatch, ScopeStackUnderflow, UnconsumedShape, InvalidBindingIndex,
   InvalidCheckpoint; program.rs:11-18) — it never consults `with`/`eval` dynamic
   lookups. `Resolver::is_unshadowed_require` goes through `unshadowed_unbound_at`
   (bindings.rs:186-189), which applies `!has_dynamic_lookup_at(span)`
   (bindings.rs:162-168: a `ScopeKind::Dynamic` ancestor or a prior eval) in addition to
   the Absent status. Inside `with (obj) { require('x') }` the collector recognizes the
   request while the resolver fails closed. Any consolidation must lift the collector to
   the resolver's standard (check the live scope stack for `ScopeKind::Dynamic` and
   recorded `ScopedDynamicEval`s before the use position), never relax the resolver.

## Coverage

- Inspected definitions: `analysis/module_request.rs` (+ `tests.rs`),
  `analysis/resolution/{mod.rs, call.rs, constant.rs, expression.rs,
  expression/static_values.rs, tests.rs}`.
- Inspected owners/peers for cross-checks: `analysis/model/value.rs`,
  `analysis/model/module.rs`, `analysis/syntax/names.rs`, `analysis/syntax/constant/*
  (mod.rs, types.rs, eval.rs)`, `analysis/scope/{static_value.rs, expression.rs,
  query/bindings.rs, query/rooted.rs, query/constants.rs,
  query/provenance/{callable.rs,object.rs}, build/provenance.rs,
  build/analysis/classification.rs, build/constants.rs, build/assignments.rs, graph.rs,
  name_env.rs, frozen_assignments.rs}`.
- Inspected consumers: `analysis/facts/{mod.rs, visitor.rs, reads.rs, pattern.rs,
  arguments.rs, assignments.rs, functions.rs, construction.rs, control.rs,
  calls/{mod.rs, callee.rs, wrapper.rs}, interface/{mod.rs, exports.rs, commonjs.rs}}`,
  `analysis/semantic/mod.rs`, `analysis/project/resolver.rs`,
  `analysis/project/linker/{mod.rs,graph.rs}`, `analysis/project/identities.rs`,
  `analysis/flow/projector/{mod.rs,driver.rs}`, `analysis/facts/tests/build.rs`,
  `project/tests/session_and_link_validation.rs`.
- Traced runtime behavior: recursion guard (`start_resolution`/`ResolutionGuard::commit`/
  `Cycle`), position keying (`ParserSpanKey`/`ResolutionKey`), `finalize_seed` provenance
  computation and global re-intern (including the budget-exhausted arm),
  fresh-object caching, the `Lookup`/constant binding-chain paths, and the
  collector-vs-resolver `is_unshadowed_require` divergence.
- Not re-reported (verified already distinct or healthy): the sealed `ResolutionStart`
  state machine, `RecognizedModuleRequest`/`ModuleRequestKind` roles vs
  `model::module::ModuleRequestRole` interface handling, and bounded bounds
  (`ConstValue::bounded`, `MAX_*` constants) on the arena conversion paths.
