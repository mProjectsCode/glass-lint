# CLI architecture

`glass-lint-cli` is the thin front end for the `glass-lint` executable.

```text
arguments + optional versioned configuration
  -> provider/profile catalog selection
  -> core configuration
  -> snippet analysis or project loading
  -> pretty or JSON presentation
  -> exit status
```

- `args` owns the Clap command surface.
- `config` owns loading precedence, schema validation, provider/profile
  selection, output settings, and project budgets.
- `lint` dispatches to `glass-lint-core` or `glass-lint-project`.
- `output` owns rule listing and report presentation.
- `bin` maps the library result to a process exit status.

The CLI may combine the JavaScript and Obsidian catalogs for the Obsidian
provider. Reusable analysis, project loading, provider rules, and report types
stay in their owning library crates. Operational errors use exit code 2;
completed invocations that cross the configured finding threshold or produce
partial analysis use exit code 1.
