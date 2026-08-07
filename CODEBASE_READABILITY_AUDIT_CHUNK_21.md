# Codebase Readability Audit

## Summary

Chunk 21 records cross-cutting findings that cannot be resolved cleanly in a
single crate. The review covered the existing twenty chunks, the root and
owning-crate architecture documents, and the phase boundaries used by core,
project loading, provider selection, CLI output, and harness profiling.

The existing chunks contain decisions for their local open questions. The
findings below are architectural follow-ups: they identify where otherwise
narrow APIs currently meet through duplicated adapters, flattened status, or
unclear ownership.

## Findings

### Analysis phase status

#### [ ] READ-101 — Define one phase-status protocol across analysis boundaries

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/session/mod.rs:233-243,458-510`, `glass-lint-core/src/project/types/input.rs:480-529`, `glass-lint-project/src/error.rs:5-47,134-148`, `glass-lint-project/src/loader.rs:36-76,85-137`, `glass-lint-harness/src/adapters.rs:150-172`, `glass-lint-cli/src/lint.rs:60-69`

Core exposes most session failures as `ProjectInputError`, the filesystem
loader wraps them as `InvalidProjectInput`, and recoverable incompleteness is
communicated separately through `ProjectLoadOutcome::partial_reason`. The
harness and CLI then infer recovery policy from diagnostics and side-channel
fields, so one semantic phase state has several representations at the crate
boundaries.

**Recommendation:** Introduce a small typed phase-status contract at the
core/project boundary: core owns semantic completion kinds, project owns
filesystem failures and fatal-versus-partial mapping, and callers consume one
status accessor. Preserve timeout fatality, fail-closed harness behavior,
partial-report diagnostics, and the distinction between authored findings and
operational errors; do not replace them with a generic string result or broad
cross-crate error enum.

**Fix Applied:** None so far.

### Provider selection boundary

#### [x] READ-102 — Centralize provider catalog and profile selection contracts

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-js/src/lib.rs:13-125`, `glass-lint-obsidian/src/lib.rs:14-53,70-95`, `glass-lint-cli/src/config.rs:305-368`, `glass-lint-harness/src/builtins.rs:22-80`, `glass-lint-harness/src/profile/runner/support.rs:53-100`

Provider crates compose catalogs and environments, while the CLI and harness
each maintain provider/profile mappings, namespace checks, and catalog
construction. These parallel contracts can drift in accepted namespaces,
catalog order, baseline semantics, or the combined Obsidian host environment.

**Recommendation:** Keep `RuleSelection` and its generic mechanics in core,
and have each provider crate expose one provider-owned target descriptor or
constructor carrying catalog composition, environment, accepted namespaces,
and profile conversion. Delete the CLI and harness prefix/baseline mappings
after migration, preserving independent crate dependencies, exact assembled-
selection validation, the harness's explicit-rule `None` baseline, canonical
catalog ordering, and Obsidian's combined host environment; provider names and
policy must not move into core.

**Fix Applied:** Added provider-owned JavaScript target descriptors for catalog/environment composition and namespace acceptance, plus Obsidian complete/isolated acceptance operations. CLI and harness construction now use those descriptors, and profile runner filtering no longer maintains provider-prefix tables.

### Profile configuration and identity

#### [ ] READ-103 — Make limits, run configuration, and profiling identity one contract

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/limits.rs:53-138`, `glass-lint-core/src/project/types/report/operations.rs:1-17,126-140`, `glass-lint-project/src/options.rs:51-93`, `glass-lint-project/src/budget.rs:3-59`, `glass-lint-harness/src/profile/config.rs:25-54`, `glass-lint-harness/src/profile/runner/projects.rs:17-24`

Profile execution constructs a default project loader and does not include
core or project limits in `ProfileWorkloadIdentity`, while harness operation
counts and project metrics arrive through separate DTO paths. A profile can
therefore appear reproducible without recording its effective limits, provider
selection, or worker policy.

**Recommendation:** Add one validated harness run descriptor that composes
provider/profile selection, core analysis limits, project load options, worker
policy, and workload identity, with effective defaults included in its stable
identity digest. Let core and project continue to validate and charge their own
budgets, and keep semantic and filesystem counters distinct rather than
introducing a shared mutable budget implementation.

**Fix Applied:** None so far.

### Source admission boundary

#### [x] READ-104 — Establish one source-admission identity boundary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-project/src/admission.rs:185-241`, `glass-lint-core/src/project/types/input.rs:238-321`, `glass-lint-core/src/project/session/mod.rs:206-222`, `glass-lint-core/src/project/session/artifacts.rs:135-164`

The project crate canonicalizes filesystem paths, enforces the project root,
and produces a validated `ProjectRelativePath`, but core session admission
normalizes again and mutates `SourceFile` through `set_path`. Virtual projects
and direct core callers use another route, so the API does not make the single
source-identity guarantee obvious.

**Recommendation:** Make `SourceFile` the canonical validated phase input:
direct and virtual callers should use its public constructors, while project
loading passes the admitted relative path exactly once through `from_relative`.
Delete the session's second normalization and mutating path setter after
migration, preserving project-root rejection, language selection,
virtual-project support, and the separation between filesystem admission and
provider-neutral core values.

**Fix Applied:** Already satisfied by the validated `SourceFile` boundary:
filesystem admission constructs sources with `SourceFile::from_relative`,
virtual callers use the public validated constructors, and project sessions
accept those values without a second normalization or mutable path setter.
This closes the duplicate identity path identified by the finding.

**Verified:** `make fmt && make ci` on the current source-admission boundary.

### Machine report boundary

#### [x] READ-105 — Give finalized machine reports one serialization boundary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/report/analysis_report.rs:33-128`, `glass-lint-core/src/project/report/mod.rs:49-70`, `glass-lint-cli/src/output.rs:204-217,254-268`, `glass-lint-harness/src/report.rs:242-244`, `glass-lint-harness/src/profile/metrics.rs:36-47`

Core owns the serializable `AnalysisReport` DTO and deterministic combination,
but the CLI and harness call `serde_json` independently, with the harness also
serializing selected fields for its digest. Callers can therefore serialize a
report before the canonical combination or ordering policy has been applied.

**Recommendation:** Expose one core-owned finalized report view or
serialization entry point for the machine schema, and make CLI JSON output
and harness report serialization use it. Keep pretty rendering in the output
crate, format selection in the CLI, and profile envelopes/digests in the
harness; keep schema/version checks and deterministic ordering in the core
contract.

**Fix Applied:** Added the core-owned `AnalysisReport::to_json_pretty` machine
serialization entry point behind the existing `serde` feature. CLI JSON
output and harness report serialization now use that boundary; pretty output,
profile envelopes, and field-specific evidence digests remain local to their
own crates.

**Verified:** `make fmt && make ci` (workspace check, clippy with warnings as
errors, 811 core tests, doctests, E2E/rule harnesses, rules documentation
check, and examples).

### Metrics ownership

#### [x] READ-106 — Separate report metrics from loader and profile metrics

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/types/report/operations.rs:1-17,104-140`, `glass-lint-project/src/loader_metrics.rs:5-15,87-124`, `glass-lint-project/src/loader.rs:36-50,91-96`, `glass-lint-harness/src/profile/types.rs:104-129,480-490`, `glass-lint-harness/src/profile/metrics.rs:36-47`

Core report operation counts, project loader timings/bytes, and harness profile
summaries describe different scopes, but they meet through aliases and
ad-hoc conversion; `ProfileOperationCounts` aliases the core count type while
profile aggregation recomputes values from the report. Adding or renaming a
metric therefore requires knowing which crate mutates it, snapshots it, and
aggregates it.

**Recommendation:** Keep the three scopes as separate domain types and make
conversion explicit at the project-to-harness boundary. Remove the profile
type alias, add read-only snapshot constructors such as
`ProfileOperationCounts::from_report` and `ProfileProjectMetrics::from_loader`,
and document each field's scope while preserving core operation semantics,
project resource accounting, deterministic aggregation, and the existing
report schema.

**Fix Applied:** Replaced the profile-to-core type alias with an explicit
`ProfileOperationCounts` domain wrapper. It owns profile-facing accessors,
construction, saturating accumulation, and conversion from core report counts;
loader and profile scopes remain separate while preserving all metric fields
and aggregation behavior.

**Verified:** `make fmt && make ci` (workspace check, clippy with warnings as
errors, 811 core tests, doctests, E2E/rule harnesses, rules documentation
check, and examples).

## Systemic Themes

- Cross-crate adapters currently carry policy that belongs to a provider or
  phase owner; narrow descriptors should replace parallel enums and mappings.
- “Complete”, “partial”, “failed”, and “measured” are related but not
  interchangeable. APIs should make scope and lifecycle explicit rather than
  infer them from diagnostics, optional fields, or aliases.
- Keep ownership narrow: core owns validated semantic values and finalized
  reports, project owns filesystem admission and loading, providers own policy,
  output owns presentation, and harness owns experiment composition.

## Open Questions

None. These six findings are recommendations for future changes; they do not
authorize source changes in this audit.

## Coverage

- Read the root architecture, testing, and contributing guidance plus the
  architecture document for each owning crate.
- Re-read all twenty existing audit chunks and incorporated their prior
  decisions into the recommendations above.
- Traced representative definitions and callers across core, project,
  JavaScript, Obsidian, CLI, output, and harness crates.
- No Rust source, fixtures, tests, or configuration files were changed by this
  audit pass.
