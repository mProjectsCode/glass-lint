# `glass-lint-core` cleanup and strengthening plan

## Scope and baseline

This plan covers `glass-lint-core` only: parsing, rule construction, semantic
analysis, matcher indexes, flow analysis, and report conversion. It is a
cleanup and strengthening plan, not permission to weaken the precision-first
contract or to move provider policy into core.

Audit baseline (2026-07-11):

- The core is small in file count but has several large, responsibility-heavy
  modules: `scope/collector.rs` (1,070 lines), `scope/mod.rs` (679),
  `semantic/calls.rs` (702), `semantic/object_flow.rs` (625), and
  `rule/matcher.rs` (1,097).
- `cargo fmt --all -- --check`, `cargo test --workspace`, and
  `cargo clippy --workspace --all-targets -- -D warnings` pass.
- The tests are good characterization coverage—44 compact-source tests, 27
  semantic tests, 20 declarative tests, and 6 scope tests—but they mostly test
  curated positive/negative examples. There are no core benchmarks, fuzz or
  property tests, resource-limit tests, or public API validation tests.
- The current architecture has a semantic directory, but it is only partly
  semantic: `EventLog` is collected and checked only by a debug assertion,
  `ValueArena` values are mostly created and immediately discarded, and flow
  state is still keyed by strings. Resolve this drift before adding more
  matcher cases.

## Contract to preserve

Every change should preserve or explicitly revise these invariants:

1. Strict matchers match proven provenance, not a spelling that happens to be
   the same. `Any`/heuristic matchers are the explicit opt-in exception.
2. A binding is identified by lexical identity and read at a source position;
   shadowing, reassignment, TDZ/hoisting behavior, and property writes cannot
   leak facts across bindings or versions.
3. Unknown, dynamic, mutable, ambiguous, or unsupported JavaScript resolves to
   unknown/local and fails closed.
4. Analysis is bounded. Large or adversarial bundles must return unknown or a
   controlled diagnostic rather than consuming unbounded memory/time or
   recursing indefinitely.
5. One parsed file produces deterministic evidence. Occurrences are ordered by
   source position, duplicate spans are removed, and truncation does not depend
   on matcher declaration order or hash iteration.
6. Core remains provider-neutral. Obsidian names, categories, disclosures,
   manifests, and provider policy stay outside this crate.

## Priority legend

- **P0:** correctness or architectural issue that can create false positives,
  false negatives, or make future fixes unsafe.
- **P1:** high-value cleanup/API hardening that reduces duplication and makes
  the P0 work maintainable.
- **P2:** performance, resilience, ergonomics, and long-term test/tooling work.

## P0 — make the semantic model authoritative

### 1. Architectural decision: finish the semantic-model migration

**Decision:** finish the migration to the shared semantic representation. Do
not retain the current string-based `ScopeGraph`/alias machinery as a parallel
semantic engine, and do not retreat to deleting `EventLog`/`ValueArena` as the
end state. The existing semantic directory is the intended architecture; it is
simply incomplete. `ScopeGraph`, string chains, and the rule-specific AST
walks are transitional code that must be absorbed behind the authoritative
semantic model and then removed.

**Evidence:** `semantic/mod.rs:24-55` describes a shared immutable model;
`semantic/events.rs:1-124` builds an event log that no analysis pass consumes;
`semantic/value.rs:16-99` defines interned values, but callers only use the ID
for debug assertions (`semantic/calls.rs:88,202` and
`semantic/resolver.rs:285`); `object_flow.rs:60-89` keys live state with
formatted strings instead.

**Target end state:** one per-file semantic build produces stable lexical
bindings, position-aware versions, canonical values, ordered events, resolved
calls, constant facts, callback summaries, and object identities. Rule
evaluation only queries those facts. There should be no second resolver that
reconstructs provenance from raw AST nodes and no flow engine that reconstructs
identity from formatted strings.

**Work:**

- Finish the intended design: stable `BindingId`/`FunctionId`, versioned
  `ValueId`, ordered events, and `ObjectId`-based flow state. The alternative
  of deleting the scaffolding is not the planned direction.
- Make the semantic pass own declarations, references, assignments, calls,
  constants, and object identities. Matchers should consume facts, not ask
  `ScopeGraph` for a new answer from a raw AST node.
- Remove the `Option<&Program>` shape from the internal analysis boundary if a
  missing program is not a meaningful semantic state. Keep parse failure as a
  report-level diagnostic.
- Add debug assertions that every referenced ID belongs to the current file
  and make invalid IDs impossible to construct in safe code.

**Migration exit criteria:** `EventLog` and `ValueArena` are consumed by the
authoritative model; `ScopeGraph` no longer performs matcher-facing semantic
resolution; string aliases/chains are not semantic identity; and the old
rule-specific AST walks are deleted rather than kept as fallbacks. Each AST node
relevant to calls/reads/writes is represented once in the shared facts. Run the
existing suite unchanged before deleting each old path, and remove the
transitional paths in the same change once their replacement is verified.

### 2. Replace text-based binding identity with symbols and versions

**Evidence:** `scope/mod.rs:22-92` stores bindings by `String`; property
assignments retain `receiver_root: String` (`scope/mod.rs:63-81`); helper
functions and calls are keyed only by names (`scope/collector.rs:36-38`);
flow keys are `"function_id:chain"` strings (`object_flow.rs:63-89`).

**Work:**

- Assign every declaration a lexical `BindingId`, and every assignment a
  monotonically ordered binding version.
- Resolve references to `(BindingId, version)` at the reference span. Keep
  global/module/rooted provenance as value facts, not as strings attached to a
  name.
- Replace property-prefix rewriting in `ScopeGraph::resolve_member_chain`
  (`scope/mod.rs:480-536`) with receiver identity plus property facts. A write
  through an alias must invalidate the same object/property; a write through a
  different shadowed receiver must not.
- Key functions, callbacks, and helper summaries by `FunctionId` and lexical
  scope, not by a global function-name map.

**Adversarial coverage:** same function name in sibling scopes; nested function
  closure over an outer alias; a function declared after a call; duplicate
  declarations; `let`/`var` loop bindings; alias reassignment followed by a
  property write; `const options = {...}; const alias = options; alias.x =
  dynamic; use(options)`.

### 3. Make future shadowing and declaration timing position-correct

**Evidence:** the collector uses the current traversal state in
`visible_binding`/`is_unbound` (`scope/collector.rs:148-169`). It makes
unshadowed decisions while collecting `require`, interop wrappers, `Promise`,
`Object`, `Reflect`, and `globalThis` (`scope/collector.rs:229-272,
574-604, 619-649, 743-753`). A later declaration can therefore be absent when
the earlier expression is collected, even though `ScopeGraph::binding_at`
later sees that declaration.

**Work:** predeclare lexical/function bindings before resolving expression
facts, then evaluate provenance at the use position. Model hoisting/TDZ as a
separate fact from “currently initialized”; do not seed a permanent module or
global alias from a provisional unbound answer.

**Required negatives:**

```js
require('sdk').send(); const require = localRequire;
__toESM(require('sdk')).send(); const __toESM = localInterop;
Promise.resolve(fetch).then(callback => callback('/x')); const Promise = local;
new Object.getPrototypeOf(fn).constructor(); const Object = local;
globalThis.fetch('/x'); const globalThis = local;
```

Each must fail closed according to the position at which the use occurs.

### 4. Rebuild object flow around identity and explicit lifecycle

**Evidence:** `object_flow.rs:44-70` stores only a source span and requirement
bits in `FlowState`; `:120-142` copies/replaces state by string key;
`:182-209` scans every flow rule for each update; `:266-309` clones states and
`emit_state_if_ready` accepts a span but deliberately ignores it; `:311-318`
always reports `source_span`. Sink paths do not mark states emitted, so the
same source can produce repeated evidence at multiple sinks. The visitor also
does not merge or invalidate control-flow branches (`:339-395`).

**Work:**

- Allocate a fresh `ObjectId` for each modeled source call. Bind aliases to the
  same object ID and replace the binding on reassignment.
- Store flow state by `(ObjectId, FlowId)` and track requirement bits plus an
  emission key. Pre-index source/configuration/sink requirements by canonical
  call target rather than repeatedly scanning all rules.
- Decide and document whether a flow finding points to the source, the
  configuration, or the sink. Preserve the relevant span; do not accept an
  ignored parameter as a hidden policy.
- Deduplicate emissions by rule, flow, object, and match site, then sort spans
  before the evidence limit is applied.
- Treat `if`/conditional expressions, loops, `try`/`catch`, and switch cases as
  joins or kill points. A source in one mutually exclusive branch must not
  survive as a definite object into another branch. If full path analysis is
  out of scope, invalidate at the boundary and document the conservative loss.
- Handle destructuring, assignment patterns, sequence/parenthesized callees,
  optional calls, compound writes, updates, and `delete` consistently.

**Adversarial coverage:** source in one branch/sink in the other; sink before
source; two allocations assigned to one variable; alias configured then
original sunk and vice versa; helper sink called with a local lookalike;
reassignment between configuration and sink; repeated sink calls; optional and
sequence-wrapped source/sink calls.

### 5. Centralize call-site resolution, including callable transforms

**Evidence:** direct calls and optional calls have separate paths in
`calls.rs:442-541`; member-read suppression is a mutable string counter at
`calls.rs:46-55,543-559`; ordinary call arguments are matched separately from
member arguments at `:136-188` and `:291-375`. `.bind` is partially modeled in
`resolver.rs:245-273`, while `.call`/`.apply` are handled as a special index
side effect in `calls.rs:410-438` and do not remap arguments.

**Work:** introduce one `ResolvedCall` fact for normal, optional, sequence,
parenthesized, bound, `call`, and `apply` invocation forms. It should contain
the target value, receiver, effective argument list, source span, optionality,
syntactic chain, rooted chain, and module provenance. Use it for all call,
member-call, flow-sink, and argument matching.

- `.bind(receiver, ...bound)` must preserve target provenance and prepend bound
  arguments correctly.
- `.call(receiver, ...args)` must match the target with `args`, not with the
  receiver in argument slot zero.
- `.apply(receiver, args)` should only expose arguments when the array/tuple is
  statically bounded; otherwise preserve the call target but fail closed for
  argument predicates.
- Remove `pending_callee_reads`; the fact collector knows whether an AST member
  is the call target and cannot mistake it for a read.

Add paired positives/negatives for global, rooted, module, shadowed, aliased,
optional, bound, call, and apply forms, including argument predicates.

### 6. Replace the three constant/string implementations with one bounded evaluator

**Evidence:** static logic is duplicated in `semantic/ast.rs:210-236`,
`scope/collector.rs:274-385`, and `scope/mod.rs:193-318`; call matching adds a
second object-literal fallback (`calls.rs:333-346`). These implementations
disagree about aliases, spreads, methods/getters, templates, and computed
properties.

**Work:** add one `semantic/constant.rs` returning a typed bounded
`ConstValue` (`Unknown`, string, non-negative integer, finite array, static
object shape). Use it for static arguments, computed property names, array
indexes, object keys, source arguments, and flow predicates.

- Bound recursion depth, visited nodes, string length, array length, object key
  count, and alias expansion count.
- Use cooked template values, not raw template spelling, and implement only the
  declared subset of JavaScript `+`/ToPropertyKey semantics. In particular,
  do not turn numeric `1 + 2` into the property string `"12"` as
  `ast.rs:217-221` currently can.
- Define one policy for spreads, duplicate keys, accessors, methods, computed
  keys, object mutation, and `Object.assign`; unknown behavior returns
  `Unknown` everywhere.
- Make dynamic `eval`/`with` invalidation apply to constants as well as calls.

Test escape sequences, cooked templates, numeric indexes, numeric addition,
static concatenation, object spreads, `Object.assign`, getters/methods,
reassignment, property mutation through aliases, dynamic lookup, and evaluator
limits.

### 7. Consolidate callback and helper summaries

**Evidence:** inline callback binding lives in `scope/collector.rs:494-605`;
named-function parameter joining is `:654-681`; flow helper discovery and body
scanning are a second implementation in `object_flow.rs:397-546`. Helper maps
are raw names, only simple identifier parameters are accepted, and
`parameter_aliases` uses `zip`, so a call with a missing argument is not a
conflicting invocation.

**Work:** build one bounded summary layer keyed by `FunctionId` with parameter
patterns, modeled calls/writes/sinks, return facts, and invocation contexts.
Require compatible facts for every observed invocation; treat missing or extra
arguments, conflicting calls, recursion, reassignment, and unknown callees as
unknown. Reuse recursive pattern projection for declarations, IIFEs,
`forEach`, `Promise.resolve(...).then(...)`, and flow helpers.

Preserve the explicit library whitelist. Do not infer callback behavior from an
arbitrary method named `then`, `forEach`, or `map`; prove the receiver where the
model requires it.

## P1 — remove duplication and harden the public/core APIs

### 8. Split the monoliths by responsibility

Use focused modules before adding more features:

- `scope/collector.rs`: declarations/scope construction, assignment history,
  module loader recognition, constants, callback summaries, and visitor logic
  are currently interleaved. Split these into scope collection, provenance
  collection, and summary collection.
- `scope/mod.rs`: separate lexical lookup, member/provenance resolution, and
  dynamic-scope handling.
- `semantic/calls.rs`: separate canonical call extraction, argument predicates,
  and class/constructor collection.
- `semantic/object_flow.rs`: separate flow state, event application, helper
  summaries, and matcher indexing.
- `rule/matcher.rs`: separate matcher data types/builders from normalization;
  1,097 lines currently contain both.
- Split `tests/compact_source.rs` by module provenance, constants,
  constructors/classes, reassignment, and callback behavior. Keep shared test
  builders in a small support module.

Do not preserve compatibility wrappers merely to avoid updating the workspace;
the development notes explicitly allow clean internal/API breaks.

### 9. Introduce shared matcher primitives and one normalization pipeline

**Evidence:** `matcher.rs:400-524` duplicates call and constructor provenance,
symbol formatting, and sort keys; `:535-697` duplicates member-call and
member-read provenance; constructors repeatedly initialize the same empty
vectors (`:400-597`); `:738-893` normalizes collections inline and then has
separate near-identical helpers at `:896-1084`.

**Work:**

- Create shared `Provenance`, qualified-symbol, occurrence, and argument
  predicate primitives while retaining provider-facing constructors with clear
  names.
- Normalize argument matcher vectors before sorting/deduplication. Currently
  member calls are sorted/deduped at `:847-849`, then their argument fields are
  normalized at `:878-891`, so semantically identical matchers can survive as
  duplicates.
- Validate matcher invariants in `ApiRuleBuilder::build` and in the catalog
  boundary: non-empty symbols, valid member chains, non-empty predicate sets,
  valid flow source/requirement/sink combinations, and no impossible indices.
- Make `ApiRule` fields private or provide a validated constructor; callers can
  currently construct a public `ApiRule` that bypasses builder normalization.

### 10. Make rule IDs, taxonomy, and errors coherent

**Evidence:** `ApiRuleBuilder` only checks a non-empty ID
(`rule/mod.rs:78-103`); syntax is checked later and only by `RuleCatalog`
(`linter.rs:18-31`). `RuleCatalog` repeatedly reparses IDs and relies on
`expect` (`linter.rs:23,56-57`). `ApiCategory::new` accepts empty/untrimmed
values (`taxonomy.rs:5-13`), while `ApiSeverity` has only info/warning but the
report `Severity` also exposes error (`lib.rs:76-82`). `ApiRuleBuildError` and
`ApiCatalogError` have no `Display`/`Error` implementation.

**Work:** validate and canonicalize IDs once, store the namespaced ID in the
catalog, and remove invariant-dependent reparsing. Decide whether IDs are
validated in `ApiRule::build` or only at provider registration, then document
that boundary. Validate category strings. Unify severity types or explicitly
explain why provider severity cannot be error. Implement useful displayable
errors with offending values and field context.

Add tests for uppercase, leading/trailing separators, repeated dots, multiple
colons, whitespace, empty categories, duplicate normalized IDs, and direct
construction attempts.

### 11. Simplify and strengthen occurrence/evidence indexing

**Evidence:** `MatcherFacts` has many parallel `BTreeMap<String, Vec<Span>>`
views (`index.rs:21-48`); insertion is repeated in `calls.rs:82-288` and
`:410-535`; `index.rs:399-426` has two wrappers for pushing evidence. Returned
member matching scans all entries and repeatedly formats prefixes
(`index.rs:160-207`).

**Work:** use one typed occurrence index with explicit strict/heuristic/module
keys, or intern symbols/modules/chains to compact IDs. Keep separate views only
when their matching semantics genuinely differ. Pre-index returned-member
prefixes. Centralize occurrence insertion and evidence accumulation.

Add an invariant test that all stored spans are source ordered and an index
equivalence test comparing the optimized index to a simple reference matcher.

### 12. Define deterministic evidence semantics once

**Evidence:** ordinary index evidence is assembled by matcher category
(`index.rs:140-157`), then argument/flow evidence is appended and truncated a
second time (`semantic/mod.rs:50-54`). Argument evidence is not source-sorted;
flow emission uses source spans even when the match occurs at a sink. Findings
collapse ranges later in `linter.rs:110-115`, but the report’s evidence entries
have no ranges (`linter.rs:126-139`).

**Work:** create an evidence accumulator that records `(kind, symbol, span,
match-site)`; sort by span/kind/symbol, deduplicate, aggregate counts, and apply
the limit exactly once per rule. Decide whether the limit is a maximum number of
spans, evidence groups, or both. Populate `Evidence.range`/`source` where the
schema promises them, or remove the unused fields. Preserve confidence if it is
part of the provider decision or remove it from the internal result.

Add tests for duplicate matchers, overlapping call/read facts, multiple flow
sinks, >16 matches, source-order truncation, identical spans from different
semantic paths, and findings with only dummy/empty spans.

### 13. Harden parsing and source locations

**Evidence:** `lib.rs:143-173` returns one debug-formatted parser error with no
filename, span, line, column, or error code. Parser options are permissive
(`allow_return_outside_function`, `allow_super_outside_method`), and source
range fallback in `linter.rs:215-230` slices by byte offset while reporting a
character-like column.

**Work:**

- Decide which permissive syntax is intentional and document it at the parser
  boundary; otherwise use strict defaults. Keep the documented JS/JSX scope
  explicit—TypeScript is currently out of scope.
- Return structured parse diagnostics with filename and source range, and use a
  stable display message rather than `{error:?}`.
- Make offset/column conversion Unicode- and CRLF-safe, use checked conversions,
  and test EOF, empty input, multibyte text, CRLF, JSX, and malformed syntax.
- If parser recovery is desired, collect multiple diagnostics deliberately;
  otherwise document that linting stops after the first parse failure.

### 14. Remove invariant-dependent panics and silent state corruption

Audit `expect`/unchecked conversions at `scope/collector.rs:73-94`,
`semantic/object_flow.rs:73-78`, `semantic/value.rs:94-99`,
`semantic/calls.rs:572`, and `linter.rs:23,57`. Replace them with constructors
that establish the invariant, `Option`/`Result` propagation at fallible
boundaries, or debug assertions followed by a safe fail-closed path. Assert
stack balance on `pop_scope`; never silently pop the program scope.

Also audit `as usize`, `as u32`, `BytePos`, `ValueId`, `ObjectId`, and evidence
count conversions. Add a large-file/large-occurrence policy instead of relying
on theoretical limits.

## P2 — performance, resilience, and maintainability

### 15. Eliminate repeated rule conversion and AST work

**Evidence:** `index.rs:58-98` repeatedly clones each rule’s matcher vector to
pre-filter it; `index.rs:140-152` converts every rule again during evidence
lookup; `collect_with_argument_matchers` runs calls, instances, helper flow,
and flow passes independently (`:110-137`). `EventLog`, scope collection,
call collection, instance collection, helper collection, and object flow each
walk the AST.

**Work:** compile/normalize the catalog once into an internal matcher plan;
pass references or compact IDs, not cloned `ApiRule`s. Build shared semantic
facts once, then run rule queries over those facts. Fast-path an empty catalog or
empty enabled set. Add allocation and analysis-time measurements before and
after.

### 16. Bound analysis of hostile bundles

Define configurable or documented limits for source bytes, AST depth, alias
expansion, constant evaluation, object/function summaries, map entries,
evidence spans, and flow states. On a limit breach, return unknown and a
controlled analysis diagnostic where useful. Add deep nesting, huge string,
large object/array, many aliases, recursive helper, and many-rule stress cases.

Avoid repeated `format!`/`String` construction in hot paths (`scope/mod.rs`,
`object_flow.rs:80-82`, `calls.rs`, and `index.rs`); intern stable symbols and
use compact integer IDs after profiling. Keep deterministic ordering independent
of `HashMap` internals.

### 17. Add benchmarks, property tests, and differential checks

Add a core benchmark target with at least:

- a small direct-call file;
- a minified bundle;
- an alias-heavy/import-heavy file;
- a flow-heavy file;
- a hostile deep/large file.

Measure parse time, semantic-build time, match time, allocations if available,
peak fact counts, and report size. Add property tests for normalization
(idempotence, permutation invariance, deduplication), source ordering, range
containment, constant evaluator fail-closed behavior, and alias versioning.
For generated AST/source cases, compare optimized facts to a deliberately simple
reference implementation on bounded inputs.

### 18. Improve crate documentation and API examples

Document strict versus heuristic matchers, provenance guarantees, supported
JavaScript syntax, parse-failure behavior, report range conventions, evidence
limits, and unsupported dynamic semantics. Add rustdoc examples for one rule,
one catalog, custom rule selection, parse diagnostics, and a negative
shadowing case. Remove comments that claim one-pass or rule-independent
behavior until the implementation actually satisfies them.

## Suggested implementation order

1. **Characterization:** add the adversarial tests listed above, especially
   future shadowing, branch flow, alias property mutation, call/apply argument
   positions, evidence truncation, Unicode ranges, and direct invalid API
   construction. Record current behavior where it is intentionally a known
   limitation.
2. **Semantic foundation:** introduce IDs, predeclaration, ordered events, the
   bounded constant evaluator, and one canonical `ResolvedCall`. Keep old
   indexes behind focused adapters only during the migration.
3. **Flow and summaries:** move object flow to `ObjectId`, merge callback/helper
   handling, add control-flow kill/join behavior, and remove string-key state.
4. **Index/evidence:** replace parallel occurrence maps and centralize sorted,
   deduplicated evidence. Verify report ranges and limits.
5. **API cleanup:** validate rules/catalogs once, simplify taxonomy/errors,
   split modules, and remove now-unused scaffolding and compatibility layers.
6. **Performance/resilience:** add benchmarks and limits, then optimize based on
   measurements rather than preserving current `BTreeMap<String, ...>` layouts
   by assumption.

Each stage should run targeted core tests, `cargo fmt --all -- --check`,
`cargo test --workspace`, and workspace Clippy with warnings denied. Do not
delete the old representation until the reference/differential tests and all
provider fixture suites pass.

## Definition of done

- Strict matching is position-, scope-, provenance-, and identity-aware; all
  unsupported/dynamic ambiguity fails closed.
- There is one authoritative semantic model, one constant evaluator, one call
  resolution path, one callback-summary path, and one evidence pipeline.
- Flow findings identify the intended match site and cannot duplicate or leak
  across branches, aliases, functions, or object versions.
- Public builders/catalogs reject invalid states without panic-based validation;
  errors and parse diagnostics are structured and useful.
- Evidence is deterministic, bounded, deduplicated, and accurately ranged.
- Core has regression, adversarial, property, benchmark, and resource-limit
  coverage.
- Formatting, workspace tests, Clippy, provider fixtures, and the benchmark
  budget all pass, with any deliberate semantic gaps documented in the core API
  and provider rule audits.

## Implementation review (2026-07-11)

The current change is a useful partial implementation, but it does not yet meet
the plan's definition of done. The focused core suite passes, and direct unit
coverage has now been added for the constant evaluator's typed addition, cooked
templates, finite arrays/objects, spreads, `Object.assign`, accessors/methods,
shadowed globals, unknown spreads, and string/container limits. The following
points remain before this plan can be considered implemented:

1. **P0 — the semantic-model migration is still transitional.**
   `SemanticModel` retains only `MatcherFacts` and per-rule argument evidence;
   the event log is still consumed solely by a debug ordering assertion
   (`semantic/mod.rs:39-55`), and `ValueArena` is not the authoritative value
   store queried by matchers. `calls`, `instance`, and `object_flow` remain
   separate AST visitors. This falls short of items 1, 5, 7, and 15 and should
   not be described as a shared, rule-independent semantic build yet.

2. **P0 — object-flow identity is only partially implemented.** `FlowState`
   has an `object_id`, but live state is still stored in
   `BTreeMap<String, Vec<FlowState>>` and keyed by formatted function/chain
   strings (`object_flow.rs:48-95`). Consequently the identity is used for
   emission deduplication, not as the authoritative alias/property state key.
   The collector also still linearly scans every flow rule at each source and
   update. Complete item 4 by binding aliases to an `ObjectId`, storing state by
   `(ObjectId, FlowId)`, and pre-indexing flow operations.

3. **P0 — constant-evaluator bounds can be bypassed by nested lookup work.**
   `Lookup::spread`, both `member` implementations, `property_name`, and
   computed `prop_name` call the top-level `evaluate`, which creates a fresh
   `EvalState` (`constant.rs:54-97,206-232` and
   `scope/collector.rs:866-895`). Nested spreads, computed keys, and member
   lookup therefore do not share the advertised depth/node budget. Alias
   expansion also has no explicit counter. Thread one evaluation budget through
   all recursive lookup operations and add adversarial tests that exceed depth,
   node, object-key-after-duplicate/spread, and alias-expansion limits.

4. **P0 — flow allocation is not safely bounded.** `next_object_id` uses
   saturating increment, so after `u32::MAX` distinct modeled sources the same
   ID is reused. The flow-state and emission collections have no practical
   configured cap. Return unknown/stop tracking when allocation is exhausted,
   as `ValueArena::allocate_object_id` now does, and test the limit through a
   configurable small test budget.

5. **P1 — callback/helper consolidation is incomplete.** Flow helpers still
   have their own `HelperCollector`/body scan while callback parameter joining
   remains in the scope collector. Scope-qualified string keys improve sibling
   isolation, but there is still no shared `FunctionId` summary with invocation
   compatibility, recursion handling, returns, and parameter patterns as
   required by item 7.

6. **P1 — the monolith and repeated normalization work remain.** The main
   collector, call, flow, and matcher modules remain responsibility-heavy, and
   `MatcherFacts::collect_for_rules` repeatedly clones and normalizes each
   rule's matcher vectors. The change improves normalization correctness and
   evidence ordering, but items 8, 11, and 15 are not complete.

7. **P1 — add invariant-level tests, not only end-to-end examples.** Direct
   constant-evaluator tests now cover its basic contract, but `EventLog`,
   `ValueArena` exhaustion/foreign-ID behavior, occurrence-index ordering and
   equivalence, and evidence normalization permutation invariance still lack
   focused unit/property tests. Add these alongside adversarial flow tests for
   loops, `try`/`finally`, switch fallthrough, destructuring, compound writes,
   `delete`, optional sources, and sequence-wrapped sources/sinks.

8. **P2 — the benchmark is a smoke timer, not the specified measurement.** It
   covers the five requested source shapes, but measures only the complete lint
   path and prints elapsed time/findings/bytes. It does not separate parse,
   semantic build, and matching; measure allocations or peak fact counts; or
   enforce a benchmark budget. Add stable benchmark methodology before using it
   to substantiate the performance/resource definition of done.

Review verification performed: `cargo fmt --all -- --check` and
`cargo test --workspace` pass (including the new evaluator unit tests), as does
`cargo clippy --workspace --all-targets -- -D warnings`. Provider fixture suites
and the benchmark budget must also pass after the remaining implementation
work.
