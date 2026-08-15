# Codebase Readability Audit — glass-lint-core Chunk 13: Module requests and resolution

## Summary

Chunk 13 covers `analysis::module_request` (recognition of `require` / interop-wrapper /
dynamic-import request shapes) and `analysis::resolution` (`Resolver`, `ResolverCache`,
`ResolvedValue`, `ResolutionProvenance`, `ResolutionKey`, `ResolutionSeed`/`ResolutionGuard`,
`FrozenFactTables`, and the `call`, `constant`, `expression`, and `expression::static_values`
submodules). The chunk is the single adapter between low-level scope/fact data and the
versioned values matchers consume.

Overall the chunk is well structured: module-request recognition is centralized behind one
`ModuleRequestContext` trait with three consumers (facts interface, scope collector, resolver),
the resolver/cache phase boundary is explicit (`FrozenFactTables` freezes both ID spaces in one
consuming transition), and fail-closed behavior is preserved carefully (unknown, cycle, and
budget-exhausted stay distinct from successful-empty). The main readability debts are
concentrated in `resolution`:

1. `resolve_call_expression` re-implements the dynamic-import recognition that
   `module_request` already owns, then round-trips the provenance it just interned back
   through the value arena (READ-001).
2. The "walk an expression through its transparent shapes" logic is implemented three
   times across syntax/scope/resolution, and a `member_expression_chain` name is reused on
   three different owners (READ-002).
3. Small helper hygiene issues: a dead `is_unknown` flag, a trivial `archive_local`
   forwarder, a duplicated depth constant, and repeated evaluate+intern+archive sequences
   (READ-003, READ-004, READ-005).
4. API naming: `Resolver` exposes three `static_string*` entry points with incompatible
   meanings (READ-006).

No source changes were made; this document is read-only.

## Findings

### Module request recognition

#### [x] READ-001 — `resolve_call_expression` duplicates dynamic-import recognition and round-trips the provenance it just interned

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `analysis/resolution/call.rs:55-97`; `analysis/module_request.rs:160-173`

The `Callee::Import` branch of `Resolver::resolve_call_expression` (call.rs:55-80) re-implements
the literal dynamic-import recognition that `module_request` already owns: it matches
`Callee::Import(_)`, takes `call.args.first()`, checks `argument.spread.is_none()`, and evaluates
`static_string(&argument.expr, self)`. `module_request::dynamic_import` (module_request.rs:160-173)
performs the exact same checks through `recognize_module_call`, which is exactly what the
`ModuleRequestContext for Resolver` impl (call.rs:187-199) exists to support. The resolver then
re-derives the provenance it just constructed: it interns
`SymbolCallProvenance::ModuleExport { module, export: "*" }` at call.rs:61-68 and immediately
re-reads it with `self.call_provenance_for_value(id)` at call.rs:73, which for a fresh ModuleExport
value deterministically returns the same provenance. Finally it hand-spells all six
`ResolutionProvenance` fields at call.rs:71-78 — a struct literal no constructor yet covers for a
non-local `call` field (compare `ResolutionProvenance::local()` at mod.rs:63-74). Impact: two
recognition paths for the same syntax can drift (new spread/argument handling must be edited
twice), and the interning round-trip re-derives a provenance the resolver already constructed,
spelling out all six fields by hand.

**Recommendation:** Route the import case through
`recognize_module_call(call, self, ModuleRequestPolicy::alias_with_dynamic_import())` and map the
resulting `DynamicImport` request to the `SymbolCallProvenance::ModuleExport { module, export: "*" }`
value, setting the provenance's `call` field to that known value directly (via a small
`ResolutionProvenance` constructor) instead of re-deriving it from the interned id. Guardrails:
keep `export: "*"`, keep the `call` field carrying the ModuleExport provenance (callers read it
positionally, e.g. facts/calls/mod.rs:35), and keep failing closed for spread or non-literal
specifiers.

**Fix Applied:** `resolve_call_expression` now routes the dynamic-import case through
`recognize_module_call(call, self, ModuleRequestPolicy::alias_with_dynamic_import())`, mapping only
the `DynamicImport` request to `SymbolCallProvenance::ModuleExport { module, export: "*" }` and
setting the provenance via a new `ResolutionProvenance::with_call` constructor; the
`call_provenance_for_value` round-trip is gone. The budget-exhausted fail-closed reason is preserved
when the interned id is `UNKNOWN` and the value arena is exhausted, and non-import calls still fall
through to the existing callee handling.

### Expression and static-value resolution

#### [x] READ-002 — Three parallel transparent-shape expression-chain walkers and a same-named `member_expression_chain` trio

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `analysis/resolution/expression/static_values.rs:26-75`; `analysis/scope/query/rooted.rs:66-87`; `analysis/syntax/names.rs:106-168`; `analysis/scope/query/provenance/callable.rs:212-218`; `analysis/resolution/expression.rs:97-101`

The "recurse through transparent expression shapes (Ident, Member, Call callee, OptChain base,
Paren, Seq-last, TS wrapper nodes)" walk is implemented three times: `Resolver::rooted_expr_chain`
(static_values.rs:26-61), `rooted_expr_chain_with` (scope/query/rooted.rs:66-87), and
`syntax::names::expression_name` (names.rs:106-128). Each enumerates the same AST shape set with a
different terminal step (resolver provenance with a synthetic-ident fallback, scope-graph binding
resolution, structural spelling). Separately, the name `member_expression_chain` is used for three
different functions: the structural one (names.rs:131-168), the scope-contextual one
(scope/query/provenance/callable.rs:212-218), and the resolver cache-aware one
(static_values.rs:63-75). Within the chunk, `Resolver::member_expression_chain` reconstructs
`ResolutionKey::Member { range: member.span.into() }` at static_values.rs:67-69 instead of reusing
`Resolver::member_key` (expression.rs:97-101), and reaches into `self.cache.resolved_values`
directly. Impact: every new transparent wrapper node (or key shape change) must be edited in three
places, and the three same-named `member_expression_chain` functions force callers to distinguish
them only by receiver.

**Recommendation:** Add one shared transparent-shape walker to `analysis::syntax` (the narrowest
owner for AST-shape mechanics, alongside `syntax::names`) that returns the effective terminal
expression / recurses for the supported shapes; each of the three owners then supplies only its
terminal identity step (structural spelling, scope graph, or resolver provenance) through a small
trait or callback. Rename the resolver and scope `member_expression_chain` methods to distinguish
them from the structural free function, and make `Resolver::member_expression_chain` reuse
`Resolver::member_key`. Guardrails: keep `Expr::This` handled in the scope walker, keep Seq/TS
handling only where behavior differs, keep the resolver's dummy-span fallback for synthetic
identifiers, and do not collapse the distinct identity semantics.

**Fix Applied:** Added one shared walker `syntax::effective_terminal_expr` plus a
`TransparentTerminal` result type in `syntax::names`; it recurses through the shapes transparent to
every caller (call callee, optional-chain base, parentheses) and returns the terminal
expression/member, with each owner supplying only its terminal identity step. `expression_name`,
`scope::query::rooted_expr_chain_with`, and `Resolver::rooted_expr_chain` now route through it; Seq
and TS-wrapper handling stays owner-side because the transparency differs (`expression_name` rejects
Seq, the scope walker rejects TS wrappers). Renamed the resolver and scope member-chain methods to
`syntactic_member_chain` / `contextual_member_chain`, and `Resolver::syntactic_member_chain` now
reuses `Resolver::member_key`; all callers (`facts/reads.rs`, `facts/calls/callee.rs`,
`scope/query/provenance/{object,chain}.rs`) were updated. Added unit tests for the shared walker and
`expression_name` terminal mapping in `syntax/names.rs`.

#### [x] READ-003 — Dead `is_unknown` flag on `interned_value` and a trivial `archive_local` forwarder

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `analysis/resolution/expression/static_values.rs:101-118`; `analysis/resolution/expression.rs:84,274,293`

`Resolver::interned_value(id, is_unknown)` (static_values.rs:105-118) takes an `is_unknown` flag
that is `false` at every one of its seven call sites (call.rs:96; static_values.rs:124, 131, 138,
148, 158, 165). The flag was evidently meant to distinguish "caller knowingly produced UNKNOWN" from
"intern failed from exhaustion," but no caller ever passes `true`, so the parameter is dead and the
`is_unknown` check never changes behavior. Separately, `Resolver::archive_local(id)`
(static_values.rs:101-103) is a one-line forwarder to `ResolvedValue::local(id)` with a misleading
"archive" name (nothing is archived into the cache), used at expression.rs:84, 274, and 293.
Impact: readers must reconcile a parameter that cannot be `true` and a wrapper that merely renames
an existing constructor.

**Recommendation:** Drop the `is_unknown` parameter from `interned_value` (behavior is unchanged —
all callers pass `false`), keeping the fail-closed `BudgetExhausted` reason when `id == UNKNOWN &&
value_arena_exhausted()`. Replace the three `archive_local` calls with `ResolvedValue::local(id)` or
fold them into a single clearly named constructor, and keep `archive_unknown_with_reason` for
cycle/unsupported reasons so uncertainty states stay distinct.

**Fix Applied:** `Resolver::interned_value` lost its `is_unknown` parameter and all seven call
sites (`static_string`, `static_number`, `static_array`, `static_object_shape`,
`intern_object_id`, `rooted_member`, and the `.bind()` path in `resolve_call_expression`) now pass
only the id; the fail-closed `BudgetExhausted` reason when `id == UNKNOWN && value_arena_exhausted()`
is unchanged. The `archive_local` forwarder was deleted and the three call sites
(`resolve_template`, the Object/Bin arm of `resolve_expr`, and `resolve_binary`) use
`ResolvedValue::local(id)` directly; `archive_unknown_with_reason` remains for cycle/unsupported
reasons.

#### [ ] READ-004 — Repeated evaluate + intern + archive sequence across three resolve entry points

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `analysis/resolution/expression.rs:79-85,272-275,287-294`

`resolve_template` (expression.rs:79-85), the `Expr::Object(_) | Expr::Bin(_)` arm of
`resolve_expr` (expression.rs:272-275), and `resolve_binary` (expression.rs:287-294) each repeat
the identical sequence `syntax_constant::evaluate(node, self)` → `intern_const_value(value, None)`
→ `archive_local(id)`. In particular `resolve_binary` (a public(in crate::analysis) method consumed
by facts/visitor.rs:276) duplicates the body of the `Expr::Bin(_)` arm of `resolve_expr`, so the two
must be kept in sync manually. Impact: a change to the intern/archive policy (e.g. adding a binding
or a reason) must be applied in three places.

**Recommendation:** Extract one private helper, e.g. `Resolver::intern_evaluated(&mut self, node) ->
ResolvedValue`, that performs evaluate + `intern_const_value(.., None)` + local archive, and have
`resolve_template`, the Object/Bin arm of `resolve_expr`, and `resolve_binary` call it (with
`resolve_expr`'s Bin arm delegating to `resolve_binary` or vice versa). Guardrail: preserve the
evaluate/`Lookup` semantics exactly; do not change which expressions are admitted.

#### [ ] READ-005 — `MAX_CONST_DEPTH` duplicates the shared constant-tree depth limit

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `analysis/resolution/constant.rs:15,28-31`; `analysis/syntax/constant/types.rs:5,93-123`

`MAX_CONST_DEPTH: usize = 32` in resolution/constant.rs:15 and `MAX_DEPTH: usize = 32` in
syntax/constant/types.rs:5 both bound the depth of a constant value tree: `ConstValue::bounded`
admissions on the write side and `Resolver::const_value_depth`'s materialization guard on the read
side. The two limits are stored as separate literals in different modules, so changing one without
the other silently changes the effective bound asymmetry. Impact: a future depth-bump edits one side
and leaves the arena-materialization guard stale.

**Recommendation:** Re-export `MAX_DEPTH` from `syntax::constant` (alongside the already-re-exported
`MAX_OBJECT_KEYS`) and reference it from `const_value_depth` in
resolution/constant.rs, making read and write depth limits one constant. Guardrail: keep the value
at 32; do not alter admission or recursion semantics.

#### [ ] READ-006 — Inconsistent `static_string*` naming family on `Resolver`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `analysis/resolution/expression/static_values.rs:120-125`; `analysis/resolution/mod.rs:367-369`; `analysis/resolution/call.rs:196-198`; `analysis/syntax/constant/mod.rs:14-19`

`Resolver` exposes three `static_string*` entry points with incompatible meanings: the inherent
`Resolver::static_string(String) -> ResolvedValue` (static_values.rs:120) *interns* a string
(callers: facts/calls/wrapper.rs:83, facts/arguments.rs:85,193, facts/visitor.rs:231); the inherent
`Resolver::static_string_value(ValueId) -> Option<String>` (mod.rs:367) *reads* a string from the
arena (callers: facts/interface/exports.rs:54, facts/visitor.rs:225,277); and the trait method
`<Resolver as ModuleRequestContext>::static_string(&mut self, &Expr) -> Option<String>` (call.rs:196)
*evaluates* an expression, alongside the free `syntax::constant::static_string(expr, &impl Lookup)`
(mod.rs of syntax/constant:14). All three names suggest "get the static string," but one is a write
that builds a value and one is a read, so callers must disambiguate by arity and argument type.
Impact: `resolver.static_string(x)` reads ambiguously, and the intern path violates the
verb-as-verb convention used elsewhere (`intern_const_value`, `intern_call_value`).

**Recommendation:** Rename the interner to `intern_static_string` (matching the existing
`intern_const_value`/`intern_call_value` verbs) so the read/write/evaluate surfaces are
distinguishable by name; keep `static_string_value` and the trait/`syntax` evaluator as-is.
Guardrails: pure rename; no behavior, provenance, or fail-closed changes.

#### [ ] READ-007 — `resolve_seed` uses let-else plus an unreachable match arm

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `analysis/resolution/expression.rs:296-316`

`resolve_seed` (expression.rs:296-316) destructures `ResolutionStart` with a `let-else` binding
`Active(guard)`, then in the `else` branch re-matches the same value including an
`Active(_) => unreachable!("active resolution handled above")` arm. Because the let-else already
rejected `Active`, that arm is structurally unreachable and exists only to satisfy the match; the
double dispatch obscures the three distinct outcomes (Cached, Cycle, Active). Impact: readers must
reason through a reachability argument to confirm the panic path is dead, and any future edit that
adds a variant changes the reachability reasoning in two places.

**Recommendation:** Replace the let-else + re-match with a single `match start { Active(guard) => { …
commit }, Cached(value) => value, Cycle => archive_unknown_with_reason(Cycle) }`, moving the
build/finalize/commit body into the `Active` arm. Guardrail: keep cycle → unknown fail-closed and
keep the guard commit ordering identical.

#### [ ] READ-008 — Borrow-alias noise in `intern_bounded_const_value`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `analysis/resolution/constant.rs:86-102`

In the `ConstValue::Object` arm of `intern_bounded_const_value` (constant.rs:86-99), the code
introduces `let arena = &mut self.values;` (constant.rs:91) solely to call
`arena.intern_construction(...)`. The child-intern closure's `&mut self` borrow ends when
`collect::<Vec<_>>()` returns, so `self.values.intern_construction(...)` compiles without the alias.
Impact: a local alias that looks like a borrow-checker workaround but adds no constraint, confusing
readers about why `self` cannot be used directly.

**Recommendation:** Remove the `arena` binding and call `self.values.intern_construction(...)`
directly after the child map is collected. Guardrail: no behavior change; the recursion and the
`StaticObject { values, names }` construction stay identical.

## Systemic Themes

- **Transparent-shape walking is re-implemented per owner.** The Ident/Member/Call/OptChain/Paren/
  Seq/TS-wrappers recurrence appears in `syntax::names::expression_name`, `scope::query::rooted`
  and `resolution::expression::static_values::Resolver::rooted_expr_chain`; this is the chunk's
  largest duplication and the primary place new AST wrappers must be added three times (READ-002).
- **The `Resolver` adapter re-implements logic that shared modules already own.** `module_request`
  centralizes `require`/wrapper/dynamic-import recognition, but the resolver's call path bypasses it
  (READ-001); constant bounds live in `syntax::constant`, but the resolver re-declares the depth
  limit (READ-005).
- **Small constructor/helper hygiene.** A dead flag (`is_unknown`), a trivial forwarder
  (`archive_local`), and a repeated evaluate+intern+archive sequence add vocabulary without adding
  invariants (READ-003, READ-004, READ-007, READ-008).
- **Naming collisions on one type.** The `static_string*` family mixes write, read, and evaluate on
  `Resolver` (READ-006), and `member_expression_chain` names three different functions (READ-002).

## Open Questions

- `Resolver.budget` is a `pub(super)` field read directly by the fact layer (facts/visitor.rs:24,42,63,
  facts/assignments.rs:47,120, facts/functions.rs:205, semantic/mod.rs:299). Resolved: the direct
  field exposure is the right contract. Callers need the `&SemanticBudget` reference itself, not a
  boolean — it is passed by value into free functions such as
  `record_static_string_origin(..., self.resolver.budget)` (facts/visitor.rs:216) and into
  `check_facts_budget(stream, resolver, limits, resolver.budget)` (semantic/mod.rs:295-299) — so a
  narrow `budget_exhausted()` accessor would not replace the reads.
- `resolve_member` converts `module_member` (`SymbolMemberProvenance::ModuleNamespace`) into
  `scoped_call` (`SymbolCallProvenance::ModuleExport`) and `finalize_seed` converts the final
  `call` back into `module_member`. The dual encoding is documented as intentional (member matchers
  vs call matchers), but a single representation could remove the two-way conversion. Left as a
  design question rather than a finding because the two surfaces are genuinely distinct.
- `ModuleRequestPolicy` exposes `pub(super)` const-fn constructors (`interface()`, `alias()`,
  `alias_with_dynamic_import()`) that are exact synonyms of the equally-visible enum variants.
  Keeping the constructors (self-documenting call sites) or using variants directly is a style
  choice, so no finding was raised.
- `ResolvedValue` both implements `Deref<Target = ResolutionProvenance>` and exposes a public
  `provenance` field used directly (facts/calls/mod.rs:35, static_values.rs:31,39). The double
  access path is deliberate convenience; not reported as a defect.

## Coverage

Files audited (read in full):

- `analysis/module_request.rs` and `analysis/module_request/tests.rs`
- `analysis/resolution/mod.rs` (incl. `ResolutionProvenance`, `ResolvedValue`, `ResolutionKey`,
  `ResolverCache`, `Resolver`, `FrozenFactTables`, `Lookup for Resolver`)
- `analysis/resolution/call.rs`, `analysis/resolution/constant.rs`,
  `analysis/resolution/expression.rs`, `analysis/resolution/expression/static_values.rs`
- `analysis/resolution/tests.rs`

Representative call sites traced (crate-wide): `facts/mod.rs:284`, `facts/interface/mod.rs:116-133`,
`facts/interface/commonjs.rs`, `facts/interface/exports.rs:53-55`, `facts/arguments.rs:37-64,228-239`,
`facts/assignments.rs:42-86`, `facts/calls/mod.rs:20,130`, `facts/calls/callee.rs:62-129`,
`facts/calls/wrapper.rs:83`, `facts/visitor.rs:203-277`, `facts/construction.rs:24-43`,
`facts/functions.rs:201-262`, `semantic/mod.rs:299-391`, `scope/build/provenance.rs:80-283`,
`scope/query/rooted.rs`, `scope/query/provenance/callable.rs:212-218`,
`analysis/model/value.rs` (`Value`, `ValueTable`, `ValueConstruction`),
`analysis/syntax/constant/{mod,types,eval}.rs` (`ConstValue`, `bounded`, `Lookup`).

Verified via `rg`: no `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!` in the chunk outside test
and `unreachable!` (READ-007) sites; `interned_value`'s `is_unknown` flag is `false` at all seven
call sites; `MAX_CONST_DEPTH` and `MAX_DEPTH` are both 32.
