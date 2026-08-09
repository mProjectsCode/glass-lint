# Harness bundling and minification plan

## Goal

Extend the conformance harness so selected fixture cases are linted both as
authored and after being bundled/minified by several JavaScript toolchains.
For every selected bundle profile and transformer, the harness must assert
that the total number of findings for each rule ID is unchanged.

The first profiles are:

- `web`: a generic browser/web application entry point;
- `obsidian`: an Obsidian plugin entry point, with host-provided modules kept
  external.

The first implementation should exercise two pinned transformer
implementations managed by Bun: Vite and esbuild. Each implementation runs the
same matrix of:

| Dimension | Values |
|---|---|
| Minification | `yes`, `no` |
| ECMAScript target | `ES5`, `ES6`, `ES2017`, `ES2022`, `ESNEXT` |

That is 10 invocations per selected profile and transformer, before adding
more profiles or tools. A case selecting both profiles therefore has 20
invocations per transformer. The matrix should be represented by stable names
and run every selected profile through every combination; adding a transformer
must not require editing each fixture.

## Fixture contract

Add a leading `//`-comment directive, parsed only with the existing header
directive rules:

```js
// @case description A browser API remains detectable after transforms
// @bundle web,obsidian
// @tool glass-lint rules=browser:clipboard.write
```

`@bundle` selects logical bundle profiles, not individual toolchains. Each
selected profile runs through the complete configured transformer matrix. A
case with no `@bundle` directive keeps today’s behavior. Normalize selected
profiles to the canonical profile order. The parser should reject an unknown
profile, an empty value, duplicate profiles, a duplicate `@bundle` directive,
and a `@bundle` directive that appears outside the leading comment block. A
later `@bundle`-looking comment must not silently be treated as ordinary
fixture text.

Initially require a configured `glass-lint` tool for bundled cases. This is a
case-load validation error, not an implicit skipped run. External adapters
continue to receive and verify the authored case only; transformed count
invariance is a Glass Lint harness assertion and must not silently turn into
an assertion about another tool's rule catalog.

For project fixtures, read `@bundle` only from the declared entry file and
apply it to that whole project. A bundled project must explicitly declare
exactly one entry; reject an omitted entry, multiple entries, and a
`@bundle` directive in a non-entry file, with explicit actionable case
errors. Do not guess how findings from duplicated or multiple bundle entry
points should be counted.
The first fixture set can remain snippet-focused while this boundary is
finalized. The analyzer's explicit resolution records are not bundler
configuration: the bundler resolves local files from the supplied project and
uses only the bundle profile's external-module policy.

## Execution model

Keep transformation policy in `glass-lint-harness`; no bundler dependency or
provider-specific policy belongs in core.

1. Load and normalize the case as today.
2. Run the normal `glass-lint` adapter against the authored source and retain
   its complete findings. Existing expectation matching remains unchanged,
   including locations, messages, certainty, and forbidden findings.
3. For each selected profile and each transformer, send a normalized input to
   the bundler runner. A snippet becomes a temporary one-entry project; a
   supported project passes its files and its one declared entry to the runner.
   The runner must produce exactly one JavaScript asset: disable code
   splitting, make output selection deterministic, and treat extra or
   non-JavaScript artifacts according to an explicit bounded error policy.
4. Lint the generated JavaScript using the same Glass Lint rule selector as
   the authored run. The transformed run must not use source-line
   expectations because bundling/minification changes locations and may
   remove comments.
5. Aggregate findings into an ordered map keyed only by fully qualified rule
   ID. Missing rules count as zero. Compare the authored map with the
   transformed map for every profile/transformer pair.
6. Record a deterministic mismatch for each differing rule, including the
   profile, transformer, before count, and after count. A transformer failure,
   generated-source parse failure, or analyzer operational error is an
   operational failure rather than a count match.

The invariant deliberately ignores finding locations, messages, evidence
traces, and certainty. It compares all actual findings, not merely the rules
listed in expectations, so a transformation that introduces a new rule ID is
also detected.

The authored run should be performed once per case/tool and reused as the
baseline for all transforms. Transformation checks should still be attempted
when ordinary expectations fail so one run exposes both fixture regressions
and transformation regressions, unless the authored run has an operational
error and therefore has no trustworthy baseline.

## Bundler toolchain

Create `tools/bundlers/`, separate from production crates and beside the
existing external adapter tooling. It should contain pinned Vite and esbuild
dependencies, a pinned Bun runtime/version file, a lockfile, and one protocol
runner that dispatches to the registered transformer implementations. The
runner should:

- accept one JSON request and return one JSON response per invocation;
- include a versioned protocol and transformer name in the response;
- receive the selected logical profile, entry filename, source/files, language,
  minification setting, and ECMAScript target;
- produce one deterministic JavaScript output (no source maps, timestamps,
  random banners, or absolute paths);
- report stderr, output-size violations, and tool errors in bounded, useful
  form; and
- use explicit arguments/data rather than shell interpolation.

Bound request size, file count, generated-source size, process lifetime, and
stderr. Exhaustion is a deterministic operational failure, never a panic or
an implicit pass.

Define profile configuration once and pass it to every transformer:

- `web`: browser platform, the selected ECMAScript target, one ESM output,
  local files bundled, and unresolved bare imports externalized rather than
  resolved from the tool runner's own installation;
- `obsidian`: the same single-entry discipline, with an explicit finite set of
  host modules (`obsidian`, `electron`, and their supported subpaths) external
  and local modules bundled. Do not infer host externals from whatever happens
  to be installed beside the runner.

Define the target mapping once as well. `ES5`, `ES6`, `ES2017`, `ES2022`, and
`ESNEXT` map to `es5`, `es2015`, `es2017`, `es2022`, and `esnext`, respectively
(`ES6` is the historical name for `ES2015`). `minified=no` must disable
minification while still bundling;
`minified=yes` must enable the selected tool’s deterministic minification
path. The exact external-module policy, output format, and target must be
shared by both transformers. Add tool-specific adapters only for translating
the common profile into Vite and esbuild options. Pin versions and record the
tool version, profile, minification setting, and target in results so upgrades
are intentional.

Before integrating the matrix, run a compatibility probe against the exact
locked versions. It must verify both minification modes and all five targets,
including a minimal `es5` build. Do not silently substitute the latest Vite:
current Vite releases document `es2015` as their lowest build target, while
esbuild may reject syntax it cannot lower to `es5`. If the pinned Vite line
cannot satisfy the requested `es5`/single-output contract, revise the matrix
or transformer choice before adding fixture expectations. A fixture whose
source cannot be lowered for a selected target remains an operational failure;
it is not a count match or a skipped matrix cell.

On the Rust side, add a small `Bundler` abstraction in the harness rather than
making `Adapter` perform two unrelated jobs. Keep the process protocol and
process lifecycle isolated from case parsing and count comparison. The default
process implementation should invoke the Bun runner with a structured request;
tests should be able to inject a fake `Bundler`, so Rust unit tests do not
require JS dependency installation.

## Data model and reporting

Add normalized bundle-profile metadata to `Case` and a transformation result
collection to `CaseResult`/`SuiteReport`. Use validated profile and transformer
names (not ad hoc strings), and keep results ordered by profile, transformer,
minification, and target using the existing deterministic ordering rules.

Each transformation result should retain at least:

- profile and transformer names and versions;
- pass/fail status;
- authored and transformed rule-count maps (or the maps needed to explain a
  failure);
- count mismatches; and
- bounded operational errors;
- the generated-source byte count and digest, with generated source retained
  only for bounded detailed failure output.

Include transformation failures in `SuiteReport::passed()`. Update the
summary, failure report, Markdown report, and the `SuiteReport.schema_version`
JSON contract together. Keep authored adapter finding totals separate from
bundle-run totals so transformed findings are not double-counted. Adapter
comparison reports should remain about adapters and should not reinterpret
bundle checks as cross-tool comparison data.

Return a separate deterministic bundle timing map keyed by profile,
transformer, minification, and target. Keep the existing adapter timing type
and comparison aggregation unchanged; do not mix bundle timings into adapter
columns or adapter comparison totals.

## Code and documentation areas

Expected implementation touch points:

- `glass-lint-harness/src/cases/snippet.rs` and `types/case.rs`: parse,
  normalize, and validate `@bundle` metadata;
- `glass-lint-harness/src/cases/project.rs`: apply entry-file metadata and
  enforce the single-entry project boundary;
- new harness bundler/protocol modules plus `types` exports: process boundary,
  profile definitions, and normalized transformation results;
- `glass-lint-harness/src/runner.rs`: authored baseline reuse, transform
  execution, and rule-count comparison;
- `glass-lint-harness/src/types/report.rs` and `report.rs`: result schema and
  renderers;
- `glass-lint-harness-cli`: only orchestration/help changes, if needed; keep
  transformation semantics in the library;
- `tools/bundlers/` JS package, Bun version file, and lockfile: transformer
  implementations and their protocol runner;
- `Makefile`, `.github/workflows/ci.yml`, `CONTRIBUTING.md`, `TESTING.md`, and
  harness README: installation, targeted commands, directive syntax, profile
  semantics, and CI ownership.

Do not add bundler logic to `glass-lint-core`, provider crates, or the
production CLI.

## Test plan

Add focused coverage at the owning layers:

- parser tests for valid profile lists, defaults, unknown/duplicate profiles,
  malformed directives, and project-entry behavior;
- protocol tests for version mismatches, malformed output, bounded tool errors,
  request/output limits, deterministic profile/transformer names, and
  generated-source failures;
- harness unit tests for rule-count aggregation, zero counts, newly introduced
  rules, removed rules, multiple transformers, and preservation of ordinary
  expectation failures;
- runner tests using a fake bundler for pass, count mismatch, and operational
  failure paths;
- JS tool tests for both profiles, local-module bundling, external Obsidian
  imports, every ES target after the compatibility probe, both minification
  modes, one-output enforcement, and deterministic output;
- one or more end-to-end fixtures with `@bundle web,obsidian` covering imports,
  aliases, wrapped callbacks, and a minified/bundled shape; include negatives
  that must not acquire findings; and
- a project fixture only after its entry/count semantics are explicitly
  supported.

The fixture’s existing `@expect-error` assertions prove authored behavior;
the new bundle assertion proves only per-rule count invariance. Do not assert
transformed line or column locations.

## Commands and rollout

Add a dedicated `make test-bundles` (or equivalent) that verifies the pinned
Bun runtime, runs `bun install --frozen-lockfile` in `tools/bundlers/`, and
runs the bundle-enabled harness suite. Keep ordinary Rust-only commands usable
without the JS dependencies, but make `make ci` depend on this gate once the
toolchain is part of the repository's required test environment. Document a
narrow command for iterating on one fixture and one command for the full
matrix; both commands must use the same lockfile and runner protocol.

Implement in this order:

1. Lock the directive, profile, project-entry, and count-comparison contracts.
2. Run and record the exact-version compatibility probe for all target and
   minification combinations; resolve the Vite ES5/output-format decision.
3. Add normalized case/report types and pure count-comparison tests.
4. Add the versioned bundler protocol and one fake/in-process test seam.
5. Add the Bun tool directory with Vite and esbuild, then implement the full
   profile × transformer × minification × ES target matrix for `web` and
   `obsidian`.
6. Integrate runner execution, failure reporting, and separate bundle timings.
7. Add focused fixtures, documentation, Make targets, and CI coverage.
8. Run the narrow harness tests, the bundle matrix, and finally `make ci` plus
   the documented Bun-backed command.

## Non-goals for the first version

- Comparing transformed locations, evidence, certainty, or messages.
- Proving semantic equivalence beyond Glass Lint’s per-rule finding counts.
- Supporting arbitrary user-supplied bundler configuration from fixture
  comments.
- Running every external comparison adapter against transformed output.
- Defining multi-entry project aggregation without an explicit fixture
  contract.
- Moving bundling or minification into the production analysis engine.
