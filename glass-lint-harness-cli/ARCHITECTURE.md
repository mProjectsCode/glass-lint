# Harness CLI architecture

`glass-lint-harness-cli` is the thin front end for the
`glass-lint-harness` executable.

```text
arguments
  -> harness library operation
  -> terminal output or generated comparison report
  -> exit status
```

- `args` owns commands and option parsing.
- `compare` owns progress output and the repository report destination.
- `profile` translates CLI options into harness profiling configuration and
  renders the summary.
- `lib` wires built-in and external adapters to harness operations.
- `bin` maps success or failure to the process exit status.

Case parsing, adapter protocol semantics, verification, report generation, and
profiling execution belong in `glass-lint-harness`. This crate depends on core
directly only for telemetry initialization.
