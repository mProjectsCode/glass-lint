# glass-lint-harness

`glass-lint-harness` is the reusable library for conformance cases, built-in
and external adapters, verification, reports, comparisons, and profiling.
The adapter protocol is versioned; response findings must include certainty
and at least one structured evidence trace.

`load_cases` reads annotated JavaScript or TypeScript snippets and `case.toml`
project fixtures. `run_suite` executes them through `Adapter`
implementations and returns a deterministic `SuiteReport` with timings.

`GlassLintAdapter` runs the built-in analyzer. `ExternalAdapter` starts a fresh
process for each case and exchanges one `AdapterRequest` and
`AdapterResponse` using `ADAPTER_PROTOCOL_VERSION`. Snippet-only adapters are
skipped deterministically for project cases.

Cases may opt into bundling with a leading @bundle web,obsidian directive.
The harness sends each selected profile through the locked Vite and esbuild
matrix and asserts authored/transformed per-rule count invariance. Bundle
timings are returned separately from adapter timings; bundle findings are not
included in adapter comparison reports.

Report helpers provide verification summaries, failure details, Markdown,
JSON, and side-by-side comparison output.

Profiling APIs support deterministic discovery, include/exclude patterns,
sampling, warm-up passes, repetitions, worker counts, independent-file and
project modes, and content-verified corpus manifests. Correctness is checked
before timing summaries are accepted, including finding identity/certainty,
diagnostics, completion, operation counts, and evidence-order digests.

See [ARCHITECTURE.md](ARCHITECTURE.md) for module ownership and
[TESTING.md](../TESTING.md) for the fixture format.
