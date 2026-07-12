# `glass-lint-core` open work plan

## Scope and current baseline

This plan contains only unfinished work. It covers the generic JavaScript
engine in `glass-lint-core` and the core-facing test/profiling support in
`glass-lint-harness`. Provider policy and Obsidian-specific knowledge remain in
`glass-lint-obsidian`.

Repository audit baseline (2026-07-12):

- Core already has stable `BindingId`, `BindingVersion`, `FunctionId`,
  `ObjectId`, and `ValueId` types; typed symbol paths; bounded constant
  evaluation; canonical handling for ordinary/optional/`bind`/`call`/`apply`
  calls; matcher validation; deterministic evidence normalization; structured
  parse locations; and focused precision regressions. Do not recreate these as
  plan items.
- Semantic construction still performs several whole-program traversals:
  resolver/scope collection, event collection, function-summary collection,
  call/index collection, and object-flow collection. `EventLog` is primarily
  an ordering/coverage guard; it is not yet the fact stream consumed by those
  analyses.
- The largest remaining files are `rule/matcher.rs` (about 1,050 lines),
  `scope/collector.rs` (about 940), `scope/mod.rs` (about 800),
  `object_flow.rs` (about 730), `calls.rs` (about 680), and `resolver.rs`
  (about 580).
- `glass-lint-core/benches/core.rs` is a custom `cargo bench` smoke program.
  This is not the desired profiling workflow and should be removed when the
  harness folder runner replaces it.
- The harness currently expects case-oriented `.js` fixtures and executes each
  configured adapter. It cannot yet lint an arbitrary folder of JavaScript for
  profiling, nor generate transformed e2e variants.

## Invariants for every task

1. Strict matching requires proven lexical identity and provenance at the use
   position. Raw-name matching remains an explicit heuristic mode.
2. Unknown, dynamic, ambiguous, unsupported, or budget-exhausted analysis
   fails closed without leaking facts across bindings, objects, functions, or
   control-flow paths.
3. Analysis is bounded and deterministic. Limits must not panic, wrap IDs, or
   make results depend on hash iteration or matcher declaration order.
4. Parse and semantic work shared by rules is built once per file. Adding a
   rule must not add another AST traversal or a parallel resolver.
5. Core remains provider-neutral. Test infrastructure may exercise providers,
   but provider names and policy do not enter the core model.

## P0 — finish the semantic fact architecture

### 1. Replace parallel AST visitors with one immutable per-file fact stream

`SemanticFacts::build` still passes the raw `Program` to both `calls::collect`
and `object_flow::collect`; `summary::collect` performs another visitor; and
the resolver/scope collector retains its own histories. The event log records
only kind, span, and scope, so consumers still revisit AST nodes to recover the
actual declaration, target, receiver, arguments, and write value.

Work:

- Define the minimal typed event/fact payloads needed by downstream analysis:
  declarations and versions, references, assignments/property writes,
  resolved calls/constructions, member reads, control boundaries, and function
  ownership. Facts should reference stable IDs and compact paths, not borrowed
  AST expressions or formatted names.
- Produce these facts during the authoritative semantic build. Resolve calls,
  effective arguments, receiver provenance, constants, and object identities
  once, then let occurrence indexing, argument predicates, summaries, and flow
  consume immutable facts.
- Fold the current event coverage/order checks into the fact builder. Delete
  the call and object-flow whole-program visitors once their consumers operate
  on facts; do not retain fallback AST paths.
- Compile the selected normalized matcher catalog once per `Linter`/catalog,
  outside per-file analysis. Per-file work may build rule-independent indexes
  and evaluate compiled predicates, but must not reconstruct or clone matcher
  plans for every source.
- Add an invariant test that every relevant AST node creates exactly one
  canonical fact and that adding unrelated matchers does not change the fact
  stream.

Exit criteria: one semantic traversal supplies downstream queries; matcher
evaluation receives no `Program`; event ordering is intrinsic to fact IDs; and
the obsolete visitors and adapters are removed in the same change.

### 2. Make event lookup bounded and sublinear before growing the fact stream

`EventLog::order_for` currently uses `Vec::iter().find` for every lookup. It is
called while walking calls and flows and again while sorting evidence. With an
event limit of `1 << 20`, this can turn otherwise linear work into quadratic
behavior. Its containment lookup can also choose an enclosing parent event
rather than the exact semantic event when spans overlap.

Work:

- Give facts direct `EventId`s during construction. For the few span-based
  compatibility lookups left during migration, use an exact `(lo, hi, kind)`
  index or a deterministic binary/range index with documented nesting rules.
- Distinguish exact-node lookup from “smallest enclosing event” lookup; never
  let caller behavior depend on visitor insertion order for equal spans.
- Cache the event/fact order on evidence occurrences so evidence sorting does
  not repeatedly search the log.
- Add nested member/call, equal-span or synthetic-span, and large-event tests.
  The folder profiling workflow below should include an event-dense corpus.

### 3. Complete lexical summary modeling without name-keyed joins

`summary.rs` still exposes `FunctionDeclarations` and `FunctionInvocations`
keyed by `(scope, String)`, while flow sink summaries use
`(FunctionId, String)`. Callback alias discovery remains partly in the scope
collector, and flow helper discovery separately scans function bodies.

Work:

- Resolve every function declaration/expression/arrow and invocation to a
  `FunctionId`; use the ID alone as identity and retain a name only for display
  or lookup diagnostics.
- Build one bounded `FunctionSummary` containing parameter patterns, call/sink
  projections, relevant property writes, return facts, and invalidation flags.
- Join invocation contexts by parameter position and recursive pattern
  projection. Missing, extra, spread, dynamic, recursive, reassigned, or
  conflicting invocations must invalidate only the affected summary facts.
- Model closures and sibling same-name functions by lexical identity. Avoid
  treating arbitrary methods named `then`, `map`, or `forEach` as known
  callback protocols without proven receiver provenance.
- Replace body rescans in object flow and callback collection with queries over
  the shared facts/summaries.

Required adversarial tests: mutually recursive helpers, function aliases,
reassignment before invocation, closures over versioned outer bindings,
destructured/default/rest parameters, spread calls, missing and extra
arguments, sibling functions with the same name, and a local lookalike callback
method.

### 4. Make object flow conservative without discarding definite baseline facts

Object flow has identity-based states and bounded emissions, but control-flow
handling remains coarse. After an `if`, conditional expression, loop, switch,
or `try`, the collector clears all states and aliases. That prevents leaks but
also loses objects and requirements established before the branch. `try` and
`finally` are especially imprecise because the finalizer is evaluated from one
reset branch rather than from a conservative join of all paths.

Work:

- Introduce explicit state snapshot, kill, and intersection/join operations.
  Preserve facts that are identical on every reachable path, including an
  unchanged baseline object; discard branch-only allocation/configuration and
  conflicting aliases.
- Define loop semantics conservatively for zero iterations, one-or-more
  iterations, `break`, `continue`, and writes in the test/update expressions.
- Model switch fallthrough and `default`, and run `finally` against the joined
  normal/exceptional state. If a construct cannot be modeled precisely, kill
  only the identities it can affect rather than clearing the file-wide state.
- Make source/configuration/sink spans explicit in `FlowState`. Document which
  site is reported and deduplicate by `(rule, flow, object, match site)`, not
  solely by the source allocation.
- Extend invalidation to all assignment patterns, destructuring aliases,
  computed/optional member writes, sequence expressions, and helper-mediated
  writes using the same binding/object identity rules.

Required tests: baseline source configured before both branches then sunk
afterward; identical configuration in both branches; conflicting branch
aliases; zero-iteration loops; switch fallthrough; catch-only writes;
`finally` configuration/sink; two valid sinks for one source; and source versus
sink evidence locations.

## P1 — precision, limits, and maintainability

### 5. Surface semantic budget exhaustion instead of silently returning no findings

Source bytes, events, values, objects, flow states/emissions, and constant
evaluation have finite limits, but their failure behavior is inconsistent.
For example, an overlarge event log makes the semantic model empty, which is
indistinguishable from a clean file to callers.

Work:

- Define a provider-neutral analysis diagnostic type with a stable code,
  optional range, limit name, and observed/capped value. Keep parse diagnostics
  distinct from semantic incompleteness.
- Thread a shared analysis budget through fact construction, resolution,
  summaries, constants, indexes, and flow. Centralize defaults and make test
  budgets injectable without exposing a misuse-prone public configuration.
- On exhaustion, retain sound facts already proven where possible, mark the
  affected analysis component incomplete, and prevent downstream consumers
  from interpreting absence as proof. Never emit a strict match from a partial
  provenance chain.
- Test each limit just below, at, and above its boundary; multiple exhausted
  components; ID conversion boundaries; and deterministic diagnostics.

### 6. Finish rule-independent indexes and remove query-time scans

`MatcherFacts` still stores many `BTreeMap<String, Vec<Span>>` views. Returned
member and suffix member-read queries scan whole maps and allocate formatted
prefix/suffix strings. Instance facts are collected with knowledge of the
selected instance matchers, so the index is not fully rule-independent.

Work:

- Intern module names, symbols, and property segments into compact IDs owned by
  the per-file model; use typed provenance keys rather than repeated strings.
- Record rule-independent instance/class/return relationships, then query them
  with compiled matcher keys.
- Pre-index exact, suffix, and returned-member prefix relations needed by the
  API. Avoid `format!` and full-map filters in rule evaluation.
- Keep one occurrence insertion/normalization policy and one evidence
  accumulator. Preserve source ordering, duplicate semantics, and the single
  evidence limit.
- Maintain a simple reference query implementation in tests and compare all
  optimized query kinds against it on generated bounded inputs.

### 7. Consolidate constant and value representations

The bounded constant evaluator is now shared for matcher predicates, but
`BindingProvenance`, resolver `Value`, static object maps, rooted chains, and
call argument representations still overlap. This makes invalidation and new
static syntax easy to implement inconsistently.

Work:

- Define ownership boundaries between lexical binding versions, canonical
  `ValueId`s, `ConstValue`, callable provenance, and object identity. A value
  fact should be interned once and projected into matcher-specific views.
- Ensure property mutation through aliases invalidates constant object facts,
  argument predicates, and flow state consistently.
- Audit JavaScript coercion support for numeric keys, cooked templates,
  concatenation, spreads, duplicate properties, accessors, methods, and
  `Object.assign`; unsupported cases return unknown uniformly.
- Add property/property-alias version tests and property-based tests for
  evaluator boundedness and normalization idempotence.

### 8. Split remaining responsibility-heavy modules after the fact boundary is stable

Do not perform cosmetic moves that preserve duplicate engines. Split along the
new ownership boundaries:

- `rule/matcher.rs`: public matcher shapes/builders versus shared primitives;
- `scope/collector.rs` and `scope/mod.rs`: lexical construction, declaration
  versions, provenance seeds, and dynamic-scope invalidation;
- `calls.rs`: fact extraction, call provenance indexing, member-read indexing,
  and argument predicate evaluation;
- `object_flow.rs`: fact transfer, control-flow joins, alias/object lifecycle,
  and emission;
- `resolver.rs`: value interning, binding-version lookup, callable transforms,
  and public query surface.

Remove obsolete compatibility helpers during the split. Keep public APIs small
and add module-level invariant documentation rather than exposing internal
collector types.

### 9. Add generated and adversarial semantic checks

- Add property tests for matcher normalization idempotence/permutation,
  deterministic evidence, constant evaluator fail-closed behavior, and stable
  binding-version resolution.
- Generate small lexical programs with shadowing, aliases, reassignment, and
  property writes; compare optimized resolution/index queries with deliberately
  simple reference implementations.
- Add stress cases for deep ASTs, many aliases, many functions/rules/events,
  huge static containers, recursive summaries, and flow-state explosion.
- Keep focused regression fixtures readable. Generated failures must print a
  reproducible source and seed.

## P2 — harness-based profiling and transformed e2e coverage

### 10. Replace the core cargo benchmark with a folder profiling mode in the harness

The intended workflow is manual profiling with `perf` or `cargo flamegraph`,
not Rust benchmark targets or `cargo test` benchmark machinery.

Work:

- Add a harness command that accepts a file or directory, recursively discovers
  `.js` files in deterministic path order, and lints every file with the chosen
  Glass Lint catalog/profile. It must not require case metadata or expected
  diagnostics.
- Support useful corpus controls: repeated `--path`, include/exclude globs or a
  documented equivalent, optional warm-up and repeat counts, fail-fast versus
  continue-on-error, and an explicit worker count. Default to a stable
  single-process/single-worker mode suitable for profiler attribution.
- Print a compact final summary: files, bytes, findings, parse/analysis
  diagnostics, total wall time, and slowest files. Provide a quiet mode so
  profiler output is not dominated by per-file logging. Keep deterministic
  behavior; this is an execution driver, not a statistical benchmark suite.
- Document human workflows such as running the debug-symbol-enabled harness
  under `perf record`/`perf report` or `cargo flamegraph`. Do not make tests
  invoke profilers, set performance thresholds, or claim stable benchmark
  numbers across machines.
- Add harness tests for recursive discovery, ordering, unreadable/malformed
  files, empty folders, repeated paths, symlinks, filtering, and summary totals.
- Remove `glass-lint-core/benches/core.rs` and the `[[bench]]` entry once the
  folder mode is available. Do not replace them with Criterion or cargo test
  benchmarks.

### 11. Add a bundler/minifier/transpiler e2e transformation matrix

The goal is to prove that production-style transformations preserve detection,
using well-known tools rather than hand-written “minified-looking” fixtures.

Work:

- Define canonical e2e source projects with an entry point and an expected
  baseline multiset of findings. Compare transformed output by rule ID and
  occurrence count; do not require identical line/column locations after
  transformation.
- Add a harness preparation mode or test driver that runs a pinned matrix of
  representative transformations. Start with at least one bundler, one
  minifier, and one transpiler; include combined production pipelines. Likely
  candidates are esbuild, Rollup, webpack, Terser, SWC, and Babel, but select a
  small maintained matrix and pin versions/flags in the repository.
- Exercise ESM and CommonJS inputs, tree shaking, scope hoisting, helper
  injection, identifier mangling, property mangling where sound, downlevel
  async/classes/optional chaining, interop wrappers, and source concatenation.
- Run Glass Lint on the baseline and every generated artifact and assert the
  same per-rule counts. Keep explicit exceptions only for transformations that
  intentionally erase a behavior; record those as fixture metadata with a
  reason rather than silently weakening the comparison.
- Store source fixtures and transformation configuration, not generated
  bundles, unless reproducibility or tool availability requires checked-in
  golden artifacts. Ensure CI failures identify the tool, version, flags,
  artifact, and count delta and preserve the failing artifact for inspection.
- Separate semantic failures from toolchain failures. Add a lightweight smoke
  subset for normal workspace tests and a documented full matrix target for
  release/architecture validation.

Required cases: direct global call, ESM named/default/namespace imports,
CommonJS `require`, aliases and destructuring, callback propagation, constructor
and instance-member matching, object flow, `.bind`/`.call`/`.apply`, local
lookalikes, shadowing, and reassignment. Include positives and adversarial
negatives so a transformed build cannot preserve counts by adding compensating
false positives.

## Suggested order

1. Add characterization tests for event lookup, baseline-preserving flow joins,
   summary identity, and explicit limit diagnostics.
2. Make event lookup indexed, compile matcher plans outside per-file analysis,
   and introduce the typed fact payloads.
3. Move call indexes and summaries to facts, then move object-flow transfer to
   facts and delete the parallel visitors.
4. Add precise control-flow joins and shared budget reporting.
5. Compact/index semantic keys and split modules along the completed boundary.
6. Add harness folder profiling, document manual `perf`/flamegraph use, and
   remove the cargo benchmark target.
7. Add the pinned transformed-e2e matrix and use its failures to drive further
   provenance/flow regressions.

## Definition of done

- One bounded, typed, source-ordered fact build is authoritative for a file;
  rules, summaries, argument predicates, and object flow do not walk the AST.
- Lexical/function/object identity is never reconstructed from display strings,
  and definite facts survive conservative control-flow joins without leaking
  branch-local state.
- Limit exhaustion is visible and deterministic while strict matching remains
  fail-closed.
- Query hot paths are indexed; event/evidence lookup cannot become quadratic in
  the number of facts.
- The harness can lint arbitrary JavaScript folders as a stable manual
  `perf`/`cargo flamegraph` target, and no cargo benchmark target remains.
- Pinned real-world bundler/minifier/transpiler e2e variants preserve the
  baseline per-rule finding multiset, including negative controls.
- Focused, generated, provider fixture, harness, formatting, workspace test,
  and warnings-denied Clippy suites pass.
