# Glass Lint open work plan

## Scope and current baseline

This file contains only unfinished work. `glass-lint-core` owns the generic
JavaScript engine; JavaScript platform policy belongs in `glass-lint-js`, and
Obsidian policy belongs in `glass-lint-obsidian`.

Current baseline (2026-07-13):

- The semantic-fact architecture and folder/Samply profiler are complete.
- The scope-reuse cursor removed the measured quadratic collection hotspot.
  On the deterministic 100-file, 86,498,585-byte sample, one measured pass is
  now about 15.1 seconds versus 66.9 seconds before the fix (about 4.4x
  faster). The slowest file is about 1.7 seconds per pass versus 20.1 seconds.
  Each pass retains 3,521 findings and two diagnostics.
- Reproduction command:

  ```sh
  target/profiling/glass-lint-harness profile \
    --path /home/lemon/src/obsidian-stats/data/out/plugin-release-mainjs \
    --provider obsidian --profile recommended --workers 1 \
    --sample 100 --seed 0 --warm-up 1 --repeat 3 --quiet
  ```

- The post-fix sampled profile has no replacement dominant hotspot. Further
  optimization is therefore evidence-driven P2 work, not an architectural
  blocker.
- Workspace tests are green, including deterministic scope-reuse and linear
  operation-count coverage.

## Invariants for every task

1. Strict matching requires proven lexical identity and provenance at the use
   position. Raw-name matching remains an explicit heuristic mode.
2. Unknown, dynamic, ambiguous, unsupported, or budget-exhausted analysis
   fails closed without leaking facts across identities or control-flow paths.
3. Analysis and output are bounded and deterministic. Changes must preserve
   finding and diagnostic ordering and source locations.
4. Shared parse and semantic work is built once per file. Adding or selecting
   a rule must not add an AST traversal or matcher-dependent fact construction.
5. Core remains provider-neutral.

## P0 — report incomplete semantic analysis

The source-size limit has a structured diagnostic, but other semantic limits
can invalidate or truncate internal state without a caller-visible reason.
For example, exhausting `MAX_FACTS` produces an empty semantic match set that
is indistinguishable from a clean file.

Work:

- Define a provider-neutral semantic diagnostic with a stable code, optional
  range, component/limit name, and observed and capped values. Keep syntax
  diagnostics distinct from incomplete semantic analysis in Rust reports,
  JSON output, and profiler totals.
- Thread one internal analysis-budget/status object through fact construction,
  resolution, constants, indexes, summaries, and object flow. Centralize
  defaults and expose smaller budgets only through test support.
- Retain sound facts already proven where possible, mark affected components
  incomplete, and prevent downstream absence from being treated as proof.
  Strict provenance chains crossing incomplete data must fail closed.
- Make fact exhaustion record one diagnostic without attempting a duplicate or
  out-of-range `FactId` emission.
- Add below/at/above tests for every limit, simultaneous exhaustions, ID
  conversion boundaries, deterministic diagnostics, and stable partial
  results.
- Classify the two diagnostics in the production sample. Preserve a minimal
  synthetic regression for any semantic exhaustion found there.

Exit criteria:

- A clean report cannot conceal semantic budget exhaustion.
- Every limit has deterministic boundary coverage and a stable diagnostic.
- Existing findings, ordering, and precision regressions remain unchanged for
  files that do not exhaust a budget.

## P1 — split responsibility-heavy semantic modules

The fact migration left `fact_builder.rs` above 1,800 lines, while
`scope/collector.rs`, `object_flow.rs`, and `index.rs` remain roughly 40 KiB or
larger. Split them along established ownership boundaries without recreating
visitors or parallel semantic models.

Work:

- Separate fact-stream storage and emission from call/value projection,
  pattern/write handling, function/class handling, and control boundaries.
- Separate lexical declaration construction from version/provenance state,
  function/callback registration, and dynamic-scope invalidation.
- Separate occurrence storage from index construction, argument predicates,
  and evidence queries.
- Separate object-flow transfer from control-flow joins/exits, alias/object
  lifecycle, helper effects, and emission.
- Keep `FactBuilder` as the only post-scope visitor, keep collector types
  private, and retain the structural test preventing downstream AST access.
- Remove dead compatibility helpers during the split and document each
  module's invariants at its boundary.

## P1 — generated semantic reference checks

- Add property tests for matcher normalization idempotence/permutation,
  deterministic evidence, constant evaluation boundedness, and stable binding-
  version resolution.
- Generate small lexical programs with shadowing, aliases, reassignment,
  property writes, closures, destructured/default/rest parameters, spread
  calls, recursion, and same-name siblings.
- Compare optimized binding, provenance, and occurrence queries with simple
  reference implementations on bounded generated inputs.
- Add stress cases for deep ASTs, many aliases/functions/rules/facts, large
  static containers, recursive summaries, and flow-state explosion. Generated
  failures must print their source and seed.

## P2 — measured performance follow-ups

The current throughput is acceptable enough that speculative data-structure
rewrites are not prioritized. Reprofile before selecting any item below, and
implement only costs shown to be material on the slow tail.

Candidate opportunities:

- Replace `record_assignment` history rescans with a per-`(scope, binding)`
  version counter if assignment-heavy operation counts or samples demonstrate
  the remaining quadratic worst case.
- Remove or test-gate the `FactStream` exact-span indexes if production callers
  no longer use them. They currently impose multiple ordered-map operations per
  fact despite evidence carrying its originating `FactId`.
- Replace per-fact `scope_chain_at` allocation where callers need only the
  innermost scope, and precompute enclosing-function or reverse function-scope
  data if those lookups appear in samples.
- Preindex dynamic-eval ancestry if files with many eval sites make the current
  repeated scan measurable.
- Intern semantic path segments or replace string-keyed ordered maps only when
  allocation or comparison samples justify the complexity.
- Add low-overhead stage timings or profiler markers if sampled stacks remain
  too diffuse to select work confidently.

For every optimization:

- Compare the same profiling build, corpus sample, worker count, and repeat
  policy using per-pass medians.
- Preserve the exact finding and diagnostic multiset for every sampled file.
- Record before/after total time, slowest-file time, throughput, peak memory,
  and sampled hot stacks.
- Use operation-count complexity tests rather than wall-clock assertions in
  the normal test suite.

## P2 — transformed end-to-end matrix

Prove detection survives production transformations rather than relying only
on hand-written minified-looking fixtures.

Work:

- Define canonical ESM and CommonJS projects with expected positive and
  adversarial-negative finding multisets.
- Pin a small maintained matrix containing at least one bundler, one minifier,
  and one transpiler, including a combined production pipeline. Cover tree
  shaking, scope hoisting, mangling, helper injection, interop wrappers,
  downlevel async/classes/optional chaining, and source concatenation.
- Compare baseline and transformed artifacts by rule ID and occurrence count.
  Record intentional behavior-erasing exceptions explicitly in fixture
  metadata rather than weakening comparisons.
- Store source/configuration rather than generated bundles where practical.
  Failures must identify tool version, flags, artifact, and count delta.
- Keep a lightweight workspace smoke subset and a documented full release
  matrix. Distinguish transformation-tool failures from lint failures.

Required semantics include direct globals, ESM import forms, CommonJS
`require`, aliases/destructuring, callback propagation, constructors and
instances, object flow, `.bind`/`.call`/`.apply`, local lookalikes, shadowing,
and reassignment.

## Suggested order

1. Surface and classify semantic budget exhaustion.
2. Split the largest modules along the stable fact-model boundaries.
3. Add generated/reference semantic checks.
4. Build the pinned transformation matrix.
5. Reprofile periodically and promote a performance candidate only when new
   evidence shows a material bottleneck or regression.

## Definition of done

- Every bounded semantic component reports deterministic incompleteness; a
  clean report cannot mean silent budget exhaustion.
- Semantic modules are focused without adding AST traversals, duplicate
  indexes, or parallel models.
- Generated/reference and transformed e2e suites preserve positive and
  adversarial-negative behavior.
- Performance remains reproducible and regression-free, with further
  optimization selected from measurements rather than source speculation.
- Formatting, workspace tests, warnings-denied Clippy, provider fixtures, and
  harness suites pass.
