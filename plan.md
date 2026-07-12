# Glass Lint open work plan

## Scope and current baseline

This file contains only unfinished work. `glass-lint-core` owns the generic
JavaScript engine; JavaScript platform policy belongs in `glass-lint-js`, and
Obsidian policy belongs in `glass-lint-obsidian`.

Repository audit baseline (2026-07-12):

- The former P0 semantic-fact migration is complete. A lexical prepass and one
  authoritative `FactBuilder` produce the rule-independent fact stream.
  Occurrence indexes, argument predicates, function summaries, and object flow
  consume facts rather than walking the AST. Catalog matchers are normalized at
  the rule boundary and compiled outside per-file analysis. Evidence retains
  originating `FactId`s, summaries use `FunctionId`, and definite flow state is
  joined conservatively across branches, loops, switch, and `try`/`finally`.
- The completed folder profiler and Samply target replace the old Cargo bench.
  The profiler preloads sources, measures lint calls separately from setup, and
  supports deterministic filtering, sampling, repetition, and worker counts.
- Verification is green: `cargo test --workspace` passes, and `make test-rules`
  passes all 64 JavaScript and 90 Obsidian provider cases.
- The first production profile covered 100 files, 86,498,585 bytes, and 3,521
  findings. Setup took 148 ms and linting took 66.9 s. One 20.1 s bundle
  accounts for 30% of lint wall time; the ten slowest files account for 48.5 s
  (72%). Aggregate throughput is about 1.29 MB/s. Two parse/analysis
  diagnostics occurred and must be classified before this corpus becomes a
  regression baseline.
- The baseline command was `make profile
  PROFILE_PATH=/home/lemon/src/obsidian-stats/data/out/plugin-release-mainjs
  PROFILE_ARGS="--quiet --sample 100 --seed 0"`. Samply attributes about 77%
  to resolver/scope collection and 18% to semantic fact construction. Source
  audit found two explicit quadratic candidates in scope collection:
  `record_assignment` rescans every prior assignment to calculate a binding
  version, and second-pass `push_scope` linearly searches all predeclared
  scopes for every scope entry. These are the first optimization candidates to
  confirm with stacks/counters.

## Invariants for every task

1. Strict matching requires proven lexical identity and provenance at the use
   position. Raw-name matching remains an explicit heuristic mode.
2. Unknown, dynamic, ambiguous, unsupported, or budget-exhausted analysis
   fails closed without leaking facts across identities or control-flow paths.
3. Analysis and output are bounded and deterministic. Optimization must not
   change findings, diagnostics, source locations, or ordering.
4. Shared parse and semantic work is built once per file. Adding or selecting
   a rule must not add an AST traversal or matcher-dependent fact construction.
5. Core remains provider-neutral.

## P0 — optimize production-bundle throughput

The profile is dominated by a small number of large/minified bundles, so
optimize the slow tail rather than the 100-file average.

For a smaller faster optimization benchmark consider running the linter on 
`/home/lemon/src/obsidian-stats/data/out/plugin-release-mainjs/obsidian-meta-bind-plugin/1.5.0-main.js`
the baseline before any optimization lies at roughly 4.1 seconds for 
`time cargo run --release --bin glass-lint -- check /home/lemon/src/obsidian-stats/data/out/plugin-release-mainjs/obsidian-meta-bind-plugin/1.5.0-main.js`

Work:

- Check in a reproducible benchmark manifest for the sampled corpus: file list
  or stable corpus-relative identifiers, byte sizes, seed, provider/profile,
  release build revision, command, worker count, warm-up/repeat counts, and
  diagnostic/finding totals. Do not check in third-party bundles.
- Re-run the baseline with `--release`, one worker, warm-up, and at least three
  measured repetitions. Report median per-file and total lint time. Separate
  parser, scope/resolver, fact construction, index/predicate evaluation,
  summaries, object flow, and final normalization with low-overhead stage
  timings or profiler markers.
- Capture and retain a Samply profile for the 20.1 s `code-workbench` outlier
  and one representative median file. Use stacks and allocation evidence to
  select work; do not infer a hot path solely from file size.
- Audit suspected scaling hazards in the measured hot stages: repeated
  `resolve_expr`/constant evaluation, string/path cloning and `format!`,
  `BTreeMap<String, ...>` scans, repeated sorting/deduplication, summary
  fixed-point rounds, and object-flow state cloning. Add counters or complexity
  tests before changing an algorithm.
- Replace assignment-history rescans with a per-`(scope, binding)` version
  counter, and replace scope-reuse scans with a preindexed deterministic scope
  key or traversal cursor. Measure these separately because both are currently
  capable of quadratic behavior in the 77% collection stage.
- Precompute reverse `FunctionId -> scope/end`, dynamic-eval ancestry, and
  enclosing-function data if samples reach the current linear scans. Avoid
  allocating `name.to_string()` merely to probe tuple-keyed maps; use nested
  maps or borrowed-key-compatible typed keys.
- Implement optimizations behind the existing semantic APIs. Prefer interned
  typed keys, memoized rule-independent projections, indexed queries, and
  compact state snapshots. Do not introduce matcher-specific fact building or
  a second fast-but-weaker analysis path.
- Add a deterministic performance smoke command that detects large regressions
  without flaky wall-clock assertions in normal unit tests. Keep full corpus
  timing manual; compare the same machine/build using medians.

Exit criteria:

- Findings and diagnostics match the recorded baseline exactly on every corpus
  file, and all correctness suites remain green.
- The selected optimization has profiler evidence identifying the old cost and
  showing it reduced. Record before/after total time, slowest-file time,
  throughput, peak memory, and build revision in this plan or a linked report.
- Set a numeric speed target only after the repeatable release baseline exists.
  The first milestone is to remove any superlinear hot path demonstrated by
  counters or stacks.

## P1 — make incomplete analysis explicit

The source-size limit has a structured diagnostic, but other semantic limits
still invalidate or truncate internal state without a caller-visible reason.
For example, `FactStream` becomes invalid at `MAX_FACTS` and the result is an
empty semantic match set, which is indistinguishable from a clean file.

Work:

- Define a provider-neutral semantic diagnostic with a stable code, optional
  range, component/limit name, and observed and capped values. Keep syntax
  diagnostics distinct from incomplete semantic analysis in both Rust and JSON
  reports and in profiler totals.
- Thread one internal analysis-budget/status object through fact construction,
  resolution, constants, indexes, summaries, and object flow. Centralize
  defaults and expose smaller budgets only through test support.
- Retain sound facts already proven where possible, mark affected components
  incomplete, and prevent absence from being used as proof. Strict provenance
  chains that cross incomplete data must fail closed.
- Fix fact-limit handling so exhaustion is recorded once and cannot attempt a
  duplicate/out-of-range `FactId` emission.
- Add below/at/above tests for every limit, multiple simultaneous exhaustions,
  deterministic diagnostics, and ID conversion boundaries.
- Classify the two diagnostics in the 100-file profile. If either is semantic
  exhaustion, preserve the file as a focused regression case or synthetic
  equivalent.

## P1 — compact and index semantic queries

Several semantic views still use owned strings and ordered maps even after the
fact migration. These are both a maintainability issue and likely candidates
for production-bundle cost, but changes should follow the profiling evidence.

Work:

- Intern module names and property/path segments into file-owned compact IDs;
  keep strings only at parsing and report boundaries.
- Ensure instance, class, return, exact-member, suffix-member, and returned-
  member relationships are rule-independent and directly indexed for compiled
  matcher keys. Remove query-time full-map scans and formatted prefix/suffix
  allocation.
- Consolidate lexical versions, `ValueId`, constant values, callable
  provenance, and object identity around documented ownership and invalidation
  rules. Intern each fact once and project matcher views from it.
- Make alias-mediated property mutation invalidate constant objects, argument
  predicates, and object flow consistently. Audit numeric keys, templates,
  concatenation, spreads, duplicate properties, accessors, methods, and
  `Object.assign`; unsupported forms return unknown uniformly.
- Compare every optimized query against a deliberately simple reference
  implementation on generated bounded inputs.

## P1 — split responsibility-heavy modules

The fact migration concentrated code in a new 1,800+ line `fact_builder.rs`;
`scope/collector.rs`, `object_flow.rs`, and `index.rs` are also about 40–44 KiB.
This now conflicts with the repository's focused-module invariant.

Split by ownership without recreating visitors or parallel models:

- fact emission versus call/value projection, patterns/writes, functions and
  classes, and control-boundary emission;
- lexical declaration construction versus version/provenance seeds and
  dynamic-scope invalidation;
- occurrence storage versus index construction and argument queries;
- object-flow transfer versus joins/exits, alias lifecycle, helper effects,
  and emission.

Keep `FactBuilder` as the only post-scope visitor and keep internal collector
types private. Remove dead compatibility helpers while splitting. Add concise
module-level invariant documentation and retain the structural no-downstream-
AST test.

## P2 — generated semantic checks

- Add property tests for matcher normalization idempotence/permutation,
  deterministic evidence, constant evaluation boundedness, and stable binding-
  version resolution.
- Generate small lexical programs containing shadowing, aliases,
  reassignment, property writes, closures, destructured/default/rest
  parameters, spread calls, recursion, and same-name siblings. Compare
  optimized resolution/index results with reference implementations.
- Add stress cases for deep ASTs, many aliases/functions/rules/facts, huge
  static containers, recursive summaries, and flow-state explosion. Generated
  failures must print a reproducible source and seed.

## P2 — transformed end-to-end matrix

Prove detection survives production transformations rather than relying only
on hand-written minified-looking fixtures.

Work:

- Define canonical ESM and CommonJS source projects with expected positive and
  adversarial-negative finding multisets.
- Pin a small maintained matrix containing at least one bundler, one minifier,
  and one transpiler, including a combined production pipeline. Cover tree
  shaking, scope hoisting, mangling, helper injection, interop wrappers,
  downlevel async/classes/optional chaining, and source concatenation.
- Compare baseline and transformed artifacts by rule ID and occurrence count.
  Record intentional behavior-erasing exceptions explicitly in fixture
  metadata; never hide them by weakening the comparison.
- Store source/configuration rather than generated bundles where practical.
  Failures must identify tool version, flags, artifact, and count delta.
- Keep a lightweight workspace smoke subset and a documented full release
  matrix. Distinguish transformation-tool failures from lint failures.

Required semantics include direct globals, ESM import forms, CommonJS
`require`, aliases/destructuring, callback propagation, constructors and
instances, object flow, `.bind`/`.call`/`.apply`, local lookalikes, shadowing,
and reassignment.

## Suggested order

1. Make the release corpus baseline reproducible and classify its two
   diagnostics.
2. Profile the slowest and median files by analysis stage, then implement and
   verify the highest-impact scaling fix.
3. Surface all semantic-budget exhaustion before increasing limits or relying
   on corpus absence as proof.
4. Compact/index the hot semantic keys and split modules along stable ownership
   boundaries.
5. Add generated reference comparisons and the pinned transformation matrix.

## Definition of done

- Production-corpus performance is repeatable, stage-attributed, and improved
  without finding, diagnostic, ordering, or location drift.
- Every bounded semantic component reports deterministic incompleteness; a
  clean report cannot mean silent budget exhaustion.
- Query hot paths are indexed and typed, and semantic modules remain focused
  without adding AST traversals or duplicate models.
- Generated/reference and transformed e2e suites cover the precision
  invariants and preserve positive and negative behavior.
- Formatting, workspace tests, warnings-denied Clippy, provider fixtures, and
  harness suites pass.
