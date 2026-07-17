# Harness architecture

`glass-lint-harness` is a reusable development library above the production
engine, project loader, and provider crates.

```text
fixture files / project manifests
  -> normalized Case values
  -> built-in or external Adapter runs
  -> normalized ToolResult values
  -> verification and deterministic SuiteReport
  -> summary, Markdown, JSON, or comparison output

profile roots / manifest
  -> deterministic corpus
  -> selected provider linter or project loader
  -> correctness check + phase metrics
  -> ProfileSummary
```

## Ownership

- `cases` parses snippet directives and project manifests.
- `types` defines normalized case, adapter, expectation, and report data.
- `adapters` defines the adapter boundary and external process protocol.
- `builtins` connects provider linters and the project loader.
- `runner` executes cases and verifies expectations.
- `report` renders suite and comparison output.
- `profile` owns corpus execution and metrics.
- `profile_manifest` owns immutable corpus selection and verification.

Case IDs, file order, adapter results, diagnostics, and reports are
deterministic. External adapters run in a fresh process for each case so
process-global state cannot leak between cases.

Harness behavior must remain reusable and independent of CLI presentation.
Production crates must not depend on this crate.
