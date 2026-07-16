# glass-lint-harness

`glass-lint-harness` is the reusable library for conformance cases, tool
adapters, result comparison, reports, and profiling. It keeps those behaviors
independent of any command-line front end so callers share the same loading,
normalization, and verification rules.

## Cases and suite execution

`load_cases` reads annotated JavaScript or TypeScript snippets and `case.toml`
project fixtures. `run_suite` runs the selected cases through a list of
`Adapter` implementations and returns a deterministic `SuiteReport` plus
per-case timings.

Snippet comments describe expected tool findings and diagnostics. Project
fixtures can either provide explicit resolution records for a virtual project
or set `filesystem = true` to exercise filesystem-backed loading. Adapters that
only support snippets are skipped deterministically for project cases.

The report helpers render the same suite result in several forms:

- `summary` and `failure_details` for verification output;
- `markdown` and `report_json` for normal reports; and
- `comparison` for side-by-side adapter results.

## Built-in and external adapters

`GlassLintAdapter` runs the built-in analyzer. `ExternalAdapter` starts a fresh
tool process for every case, which prevents one case's process-global state
from leaking into another.

External tools receive one serialized `AdapterRequest` on stdin and must write
one `AdapterResponse` to stdout. Both sides use `ADAPTER_PROTOCOL_VERSION`.
Project requests include their root, entries, language-tagged files, and any
explicit resolution records.

## Profiling

`profile_folder` profiles supported runtime sources described by
`ProfileConfig`. Discovery is recursive, deterministic, and configurable with
include/exclude patterns, sampling, warm-up passes, repeat counts, and worker
counts. Source files are loaded before measured single-file linting so normal
lint time excludes discovery and reads.

Project profiling keeps discovery and reads in the measured operation and
reports separate discovery, read, local-analysis, resolution, and
linking/matching phases. `ProfileSummary` also includes deterministic file,
request, edge, and evidence counts.

See [TESTING.md](../TESTING.md) for the case format and expectation syntax.
