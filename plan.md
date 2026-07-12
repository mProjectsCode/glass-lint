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

## Implementation review (2026-07-12)

The implementation has moved materially beyond the July 11 review, but the
plan's definition of done is not yet met.

Completed or substantially addressed:

- Constant evaluation now threads one `EvalState` through identifier, member,
  spread, computed-property, and `Object.assign` lookup. Depth, node, lookup,
  string, array, and object limits fail closed, with direct tests for recursive
  alias exhaustion and container/string limits (`constant.rs:88-309,
  427-501`). This resolves the previous budget-bypass finding in item 6.
- Object flow now uses an alias map from `BindingKey` to `ObjectId`, stores live
  state by `(ObjectId, FlowId)`, pre-indexes sources and sinks, deduplicates
  emissions, and caps objects, states, and emissions (`object_flow.rs:31-105,
  141-167,238-318,527-550`). A configurable small-budget unit test verifies
  fail-closed object exhaustion. This resolves the previous identity,
  linear-source/sink-scan, and unbounded-allocation findings in item 4.
- Callable transforms now cover `.bind`, `.call`, `.apply`, optional calls,
  and sequence-wrapped calls with effective argument remapping, including
  bounded static-array handling for `.apply` (`calls.rs:106-357,600-711`).
  Focused positive and negative tests exercise these forms.
- Evidence from ordinary indexes, argument predicates, and flows now crosses
  one normalization boundary: occurrences are source-sorted, deduplicated by
  `(span, kind, symbol)`, and truncated once before regrouping
  (`semantic/mod.rs:74-119`). Report evidence includes Unicode-aware ranges and
  snippets, and tests cover ordering and the single evidence limit. Rule IDs,
  categories, builder errors, oversized-source diagnostics, and parse locations
  also have direct validation tests.
- Future declarations, reassignment, sibling scopes, property writes, dynamic
  scopes, branch flow invalidation, repeated sinks, optional flow sinks, and
  incompatible helper invocations now have regression coverage in the core
  integration suites.

Remaining work, in priority order:

1. **P0 — finish the authoritative semantic-model migration.**
   `SemanticModel` now owns one `SemanticFacts` build result and the catalog
   inputs are compiled at that boundary, but the build still delegates to
   separate canonical-call and object-flow visitors. `EventLog` now
   orders evidence and enforces its event budget, but is not yet the complete
   fact stream. `ScopeGraph` still computes string-shaped provenance before the
   resolver interns position-versioned values. Items 1, 2, 5, and 15 therefore
   remain transitional.

2. **P0 — remove the remaining textual binding/provenance machinery.**
   Flow aliases now use `BindingId`, `BindingVersion`, `FunctionId`, and
   structured paths, but scope collection and member provenance are still
   selected through name/span maps and string-shaped alias targets. The
   resolver interns a versioned binding wrapper and caches it, but the arena is
   not yet the sole source of all call/member facts. Complete this clean break
   before treating the semantic identity migration as finished.

3. **P1 — finish callback and flow-helper summaries.** A shared
   `semantic/summary.rs` layer now owns recursive parameter projection,
   lexical named-function alias joining, and flow-helper sink summaries keyed
   by `FunctionId`. Callback invocation discovery and the remaining modeled
   calls/writes/returns still live in the scope collector, so the complete
   summary required by item 7 is not finished.

4. **P1 — split the monoliths and finish the compiled index design.** The
   principal modules remain responsibility-heavy: `scope/collector.rs` is
   about 1,360 lines, `calls.rs` about 820, `object_flow.rs` about 930, and
   `rule/matcher.rs` about 1,330. Call/flow/instance matcher inputs now borrow
   compiled records and occurrence maps use a typed container, but the index
   still has parallel provenance views and the largest modules have not all
   been split by responsibility. Items 8, 9, 11, and 15 remain partial.

5. **P1/P2 — add the missing invariant, property, and adversarial coverage.**
   Direct invariant tests now cover event ordering, invalid value IDs,
   normalization permutation, typed occurrence ordering, flow budgets, and
   compound/update/delete invalidation. A property-test framework and
   reference-index differential test are still absent, as are dedicated
   loop, `try`/`finally`, switch-fallthrough, and destructuring flow cases.

6. **P2 — replace the smoke benchmark with actionable measurements.** The
   benchmark still exercises the five requested source shapes but times only
   the complete lint path and prints elapsed time, findings, diagnostics, and
   bytes (`benches/core.rs`). It does not isolate parse/semantic/match phases,
   measure allocations or peak fact/flow counts, repeat samples statistically,
   or enforce a budget. It cannot yet substantiate the performance and resource
   parts of the definition of done.

Review verification performed on 2026-07-12: `cargo fmt --all -- --check`,
`cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings` all pass. The workspace
suite includes 19 core unit tests, 104 core integration tests, provider tests,
and harness tests. Provider fixture suites and a defined benchmark budget still
need to be run after the remaining architectural work.

## Implementation progress (2026-07-12, continuation)

The remaining P0/P1 work has now received a second implementation pass:

- Lexical declarations have stable per-file `BindingId`s, position-sensitive
  `BindingVersion`s, and `FunctionId`s. Resolver-owned flow keys use those IDs
  plus structured property paths; property-write invalidation compares the
  same identity rather than formatted receiver names. Version changes are
  covered by unit tests, and compound writes, updates, and `delete` now kill
  flow state conservatively.
- `ValueArena` stores versioned binding values and reuses existing interned
  values before applying its capacity bound. Invalid IDs fail closed. The
  ordered event log now participates in evidence ordering and rejects an
  overlarge event stream instead of producing ambiguous IDs.
- Direct and optional flow source calls share the canonical `ResolvedCall`
  argument representation, including effective `.call`/`.apply` receiver and
  argument remapping. Bound callables preserve static strings and rooted
  expression arguments. Flow helper sink discovery is separated into the shared
  `semantic/summary.rs` layer, and named callback summaries are keyed by
  lexical owner `FunctionId` rather than a global function-name map.
- Semantic assembly now has one `SemanticFacts::build` boundary; the matcher
  model owns the resolver and compiled fact indexes. Class/instance facts are
  now collected by the canonical call visitor; `MatcherFacts` owns typed
  occurrence-index storage with one span insertion/normalization policy, and
  resolver results are cached per source node so the value arena is the stable
  source for repeated matcher queries.
- Argument, flow, and instance matcher inputs are now borrowed from the
  normalized catalog during fact construction instead of cloned into visitor
  records; the flow index retains references to the compiled flow matchers.
- Matcher-shape validation now lives in its own rule-validation module, and the
  scope collector delegates recursive callback parameter projection and
  invocation joining to the shared summary layer. Control-boundary tests cover
  loops, try/catch, and destructuring fail-closed behavior.
- Summary invocation joins now invalidate aliases for missing, dynamic, or
  extra arguments instead of relying on `zip`; focused regressions cover both
  incomplete and over-specified helper calls.
- Normalization permutation invariance and typed occurrence ordering are now
  directly tested in addition to the existing evidence and ID invariants.
- Nested matcher normalization is now in `rule/normalization.rs`, while flow
  indexing, canonical flow calls, call-argument predicates, constructors, and
  scope predeclaration/constant/callback passes have focused modules. A
  reference-query test checks the optimized occurrence index.
- Alias targets now use segmented `SymbolPath` values, property aliases are
  indexed by `(BindingKey, path)` with typed targets, and the raw receiver AST
  reference is confined to collection-time conversion. This removes formatted
  receiver strings from the authoritative scope graph.
- Resolver construction now consumes typed `IdentValueSeed` and
  `MemberValueSeed` snapshots from scope collection. Scope provenance is
  converted once at that boundary; resolver interning no longer reconstructs
  call/member/constant facts by issuing a sequence of independent
  `ScopeGraph` queries. The shared event log also records declarations,
  assignments, calls, constructions, references, and member reads under one
  bounded source-order policy.
- Rule builders validate matcher shape before normalization: empty or malformed
  chains, invalid predicates and argument indices, incomplete flows, and empty
  sink index sets are rejected with displayable `InvalidMatcher` errors.
- New adversarial coverage covers future declarations for builtin provenance
  seeds, same-name helpers in sibling lexical scopes, binding versions, event
  ordering/limits, invalid value IDs, and compound/update/delete flow writes.

Verification after this pass: `cargo fmt --all -- --check`,
`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
the 11-case e2e harness (all passed), and both provider fixture suites (64 JS
cases and 90 Obsidian cases, all passed).

The remaining architectural gap is replacing the two stateful call and
object-flow AST visitors with one immutable event/fact stream. Both visitors
now receive and validate against the same bounded event log, so source order
and node coverage are shared, but their state transitions are still separate.
The resolver boundary itself now consumes typed seeds, and the new IDs,
segmented paths, expanded event ordering, canonical call container, and
summary/index boundaries are the migration seam. The P2
benchmark/property-test expansion remains separate work.
