# Glass Lint command-line structure

The CLI crates keep executable concerns outside the reusable analysis and
harness libraries.

## `glass-lint-cli` library target

This library parses user configuration, dispatches lint operations, and
formats command-specific output for the `glass-lint` binary.

### Modules

- `glass_lint_cli::args` — Defines the user-facing command-line grammar.
- `glass_lint_cli::config` — Loads and validates CLI configuration and provider selection.
- `glass_lint_cli::lint` — Dispatches snippet and project lint operations.
- `glass_lint_cli::output` — Selects CLI output formats and rule-list presentation.
- `glass_lint_cli::rules_doc` — Builds generated rule documentation data.
- `glass_lint_cli::telemetry` — Configures CLI telemetry verbosity.

### Structs and enums

- `args::Args` — Stores parsed top-level CLI arguments.
- `args::Command` — Selects the requested CLI operation.
- `config::CliConfig` — Stores effective CLI-wide configuration.
- `config::Config` — Stores validated application configuration.
- `config::FailOn` — Selects the finding threshold that changes the exit status.
- `config::OutputFormat` — Selects pretty or machine-readable output.
- `config::PreparedConfig` — Caches a validated rule selection together with its provider, profile, and core settings so repeated linter builds can reuse it.
- `config::ProjectConfig` — Stores project-loading configuration.
- `config::Provider` — Selects the provider catalog and environment.
- `config::RawConfig` — Stores the unvalidated configuration file shape.
- `config::RuleSelectionProfile` — Selects a predefined rule-selection profile.
- `config::Verbosity` — Selects CLI logging verbosity.
- `output::FileOutput` — Stores rendered output for one file.
- `output::Row` — Stores one CLI table row.
- `output::Table` — Stores a CLI-rendered table.
- `rules_doc::CatalogDocumentation` — Stores generated documentation for one rule catalog.
- `telemetry::TelemetryLevel` — Selects the telemetry detail level.
- `telemetry::TelemetryOptions` — Stores telemetry configuration.

## `glass-lint-cli` binary target

- `glass_lint` — Starts the `glass-lint` executable and maps library results to process exit status.

## `glass-lint-harness-cli` library target

This library translates command-line profiling and comparison requests into
operations on `glass-lint-harness`.

### Modules

- `glass_lint_harness_cli::args` — Defines harness command-line arguments and subcommands.
- `glass_lint_harness_cli::compare` — Reports comparison progress and output destinations.
- `glass_lint_harness_cli::profile` — Translates profile arguments into harness configuration.
- `glass_lint_harness_cli::telemetry` — Configures harness-CLI telemetry verbosity.

### Structs and enums

- `args::Args` — Stores parsed harness-CLI arguments.
- `args::Command` — Selects verification, comparison, or profiling work.
- `args::Format` — Selects the harness output format.
- `args::ProfileArgs` — Stores profiling command arguments.
- `args::ProfileCatalogProviderArg` — Selects the provider catalog from the CLI.
- `args::RuleSelectionProfileArg` — Selects the rule profile from the CLI.
- `compare::ProgressLayer` — Emits structured comparison progress events.
- `compare::ProgressVisitor` — Converts comparison events into progress output.
- `telemetry::TelemetryLevel` — Selects the telemetry detail level.
- `telemetry::TelemetryOptions` — Stores telemetry configuration.

## `glass-lint-harness-cli` binary target

- `glass_lint_harness` — Starts the harness executable and maps library results to process exit status.
