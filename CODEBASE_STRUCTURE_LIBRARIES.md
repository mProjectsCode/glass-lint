# Glass Lint library and provider structure

Each line below is one production module, struct, or enum from the
corresponding `cargo-modules` tree.

## `glass-lint-datastructures`

This crate provides deterministic, bounded storage primitives used by the
provider-neutral engine.

### Modules

- `glass_lint_datastructures::budget` — Tracks generic resource budgets and their exhaustion state.
- `glass_lint_datastructures::diagnostic` — Defines validated source positions and ranges.
- `glass_lint_datastructures::fingerprint` — Computes deterministic content fingerprints.
- `glass_lint_datastructures::history` — Stores parent-linked reversible state histories.
- `glass_lint_datastructures::name` — Interns names into bounded artifact-local identifiers.
- `glass_lint_datastructures::path` — Represents property paths in owned and borrowed forms.
- `glass_lint_datastructures::path::name_path` — Implements the owned name and symbol path types.
- `glass_lint_datastructures::path::view` — Provides borrowed views over property paths.
- `glass_lint_datastructures::path_trie` — Exposes the compact path-trie storage boundary.
- `glass_lint_datastructures::path_trie::store` — Stores linked path nodes and their parent relationships.
- `glass_lint_datastructures::path_trie::types` — Defines path identifiers and input segments.
- `glass_lint_datastructures::table` — Provides dense bounded tables keyed by typed indices.

### Structs and enums

- `FastIndexSet` — Provides a compact insertion-ordered set for small typed indices.
- `budget::Budget` — Describes a fixed resource limit.
- `budget::BudgetTracker` — Counts resource use and records when a budget is exhausted.
- `diagnostic::ByteRange` — Identifies a half-open byte span in source text.
- `diagnostic::InvalidPosition` — Reports an invalid line-and-column position.
- `diagnostic::InvalidSourceBoundary` — Reports a source range boundary outside the input.
- `diagnostic::Position` — Represents a validated line-and-column source position.
- `diagnostic::ReversedByteRange` — Reports byte ranges whose end precedes their start.
- `diagnostic::ReversedSourcePositionRange` — Reports source ranges with reversed positions.
- `diagnostic::SourceRange` — Identifies a validated source span by positions and bytes.
- `fingerprint::Fingerprint` — Holds the deterministic hash used for content identity.
- `history::HistoryCursor` — Points at a position in a parent-linked history.
- `history::HistoryEntry` — Stores one internal history node and its transition data.
- `history::HistoryTransition` — Describes a reversible history change.
- `history::ParentLinkedHistory` — Maintains bounded state snapshots linked to parent states.
- `name::NameExhausted` — Describes failure to allocate another interned name.
- `name::NameId` — Identifies an entry in one artifact-local name table.
- `name::NameTable` — Maps source names to deterministic bounded identifiers.
- `path::name_path::NamePath` — Owns a path of interned name identifiers.
- `path::name_path::Path` — Owns a sequence of property-name segments.
- `path::name_path::SymbolPath` — Owns a path of source-level symbol names.
- `path::view::PathView` — Borrows a property path without copying its segments.
- `path_trie::store::ParentRef` — Identifies the parent relationship of a trie node.
- `path_trie::store::PathLink` — Connects a path segment to a stored child node.
- `path_trie::store::PathNode` — Stores one internal node of the path trie.
- `path_trie::store::PathSegments` — Represents the segments associated with a trie path.
- `path_trie::store::PathStore` — Interns and retrieves compact property paths.
- `path_trie::types::PathId` — Identifies a path in one path store.
- `path_trie::types::PathSegment` — Represents one normalized path segment.
- `path_trie::types::PathSegmentInput` — Represents caller input that can become a path segment.
- `table::IndexTable` — Stores optional values in a dense typed-indexed table.
- `table::InsertOutcome` — Reports whether a table insertion added or replaced a value.

## `glass-lint-project`

This crate turns a filesystem selection into bounded owned sources and typed
module-resolution outcomes for core.

### Modules

- `glass_lint_project::admission` — Validates and records which filesystem paths may enter a project.
- `glass_lint_project::budget` — Defines aggregate filesystem resource limits.
- `glass_lint_project::corpus` — Loads deterministic reusable source corpora.
- `glass_lint_project::discovery` — Discovers source files and follows bounded `tsconfig` membership.
- `glass_lint_project::error` — Defines expected project-loading and option failures.
- `glass_lint_project::loader` — Coordinates discovery, reads, resolution, and core project phases.
- `glass_lint_project::loader_metrics` — Records bounded load counters and phase timings.
- `glass_lint_project::loader_phases` — Owns the path queue, resolution cache, and frontier state.
- `glass_lint_project::options` — Validates project selection and loading configuration.
- `glass_lint_project::resolver` — Adapts Oxc module resolution to core's typed inputs.
- `glass_lint_project::tsconfig` — Parses and expands project configuration references and patterns.
- `glass_lint_project::tsconfig::selection` — Compiles and merges effective `tsconfig` selections.
- `glass_lint_project::walk` — Provides bounded deterministic directory walking support.

### Structs and enums

- `admission::AdmissionSet` — Deduplicates paths admitted to a project.
- `admission::AdmittedSourcePath` — Records one validated source path accepted for loading.
- `admission::CanonicalProjectPath` — Represents a path normalized under the canonical project root.
- `admission::FileBudget` — Tracks per-file admission and read limits.
- `admission::PathAdmission` — Describes whether a candidate path is accepted, skipped, or rejected.
- `admission::SourceAdmission` — Combines path validation with source-file admission metadata.
- `budget::ProjectResourceBudget` — Bounds discovery, reading, resolution, bytes, and elapsed load work.
- `corpus::CorpusFile` — Holds one source file selected for corpus processing.
- `corpus::SourceCorpus` — Provides a deterministic collection of corpus files.
- `discovery::DiscoveryResult` — Returns discovered paths and partial-discovery status.
- `discovery::ProjectDiscovery` — Performs bounded project and `tsconfig` discovery.
- `discovery::RefStackItem` — Stores one pending `tsconfig` reference traversal item.
- `discovery::RefWorkItem` — Stores one pending filesystem discovery work item.
- `discovery::TsconfigExpansion` — Describes the files and references expanded from a `tsconfig`.
- `discovery::TsconfigGraphWalker` — Walks the bounded graph of referenced configurations.
- `error::ProjectLoadError` — Classifies failures encountered while loading a project.
- `error::ProjectOptionError` — Classifies invalid project-loading options.
- `loader::ClosedFrontier` — Records that no more project paths can be admitted.
- `loader::FinishMode` — Selects how the loader completes normal or partial work.
- `loader::LoadDeadline` — Tracks the deadline imposed on one project load.
- `loader::ProjectLoadOutcome` — Returns the loaded core project together with partial status and metrics.
- `loader::ProjectLoadState` — Holds mutable state for the multi-phase loading loop.
- `loader::ProjectLoadStatus` — Classifies whether a project load completed or was partial.
- `loader::ProjectLoader` — Coordinates the complete filesystem-to-core loading workflow.
- `loader::ProjectPaths` — Groups the canonical paths relevant to one load.
- `loader::ReadWaveOutcome` — Summarizes one bounded wave of source reads.
- `loader::RequestResolutionOutcome` — Summarizes resolution requests produced or completed by a wave.
- `loader_metrics::ProjectLoadMetrics` — Aggregates project loading counters and timings.
- `loader_metrics::ProjectPhaseTimings` — Records elapsed time for each loading phase.
- `loader_phases::PathWorkQueue` — Maintains deterministic pending path work.
- `loader_phases::ResolutionCache` — Reuses resolution results for repeated requests.
- `loader_phases::ResolutionSpecifierKey` — Keys cached resolution by importer and specifier.
- `options::ProjectLoadOptions` — Holds caller-provided filesystem loading settings.
- `options::ProjectLoadOptionsBuilder` — Builds validated project loading settings.
- `options::ProjectSelection` — Selects a directory, file, or configuration as the project root.
- `options::SourceExtensionSet` — Defines the source extensions eligible for discovery.
- `options::ValidatedProjectLoadOptions` — Stores options after all boundary checks pass.
- `resolver::ProjectResolver` — Resolves core module requests within project filesystem rules.
- `tsconfig::ConfigTraversalBudget` — Bounds recursive configuration traversal.
- `tsconfig::ParsedField` — Records the parsed state of a configuration field.
- `tsconfig::ParsedTsconfig` — Holds the supported fields from one parsed configuration.
- `tsconfig::ReferenceEntry` — Describes one referenced configuration path.
- `tsconfig::StringArrayField` — Represents a parsed string-array configuration field.
- `tsconfig::StringField` — Represents a parsed string configuration field.
- `tsconfig::TsconfigDiagnostic` — Reports a non-fatal configuration parsing issue.
- `tsconfig::TsconfigTraversal` — Tracks visited configurations and traversal limits.
- `tsconfig::selection::CompiledTsconfigSelection` — Represents compiled include and exclude matching rules.
- `tsconfig::selection::MergedSelection` — Represents selection values after inheritance merging.
- `tsconfig::selection::ParentSelection` — Holds inherited selection data from a parent configuration.
- `tsconfig::selection::TsconfigPatternSet` — Matches paths against normalized configuration patterns.

## `glass-lint-js`

This crate supplies JavaScript, browser, Node, and Electron policy catalogs
over the provider-neutral core engine.

### Modules

- `glass_lint_js::rules` — Groups all JavaScript host-policy rule factories.
- `rules::browser` — Groups browser and DOM policy rules.
- `rules::browser::clipboard_read` — Defines browser clipboard-read policy.
- `rules::browser::clipboard_write` — Defines browser clipboard-write policy.
- `rules::browser::environment` — Defines browser environment-access policy.
- `rules::browser::file_dialog` — Defines browser file-dialog policy.
- `rules::browser::filesystem` — Defines browser filesystem policy.
- `rules::browser::global_input_hook` — Defines browser global-input-hook policy.
- `rules::browser::permissions_bluetooth` — Defines browser Bluetooth permission policy.
- `rules::browser::permissions_geolocation` — Defines browser geolocation permission policy.
- `rules::browser::permissions_hardware` — Defines browser hardware permission policy.
- `rules::browser::permissions_media` — Defines browser media permission policy.
- `rules::browser::permissions_notifications` — Defines browser notification permission policy.
- `rules::browser::permissions_query` — Defines browser permission-query policy.
- `rules::browser::persistent_storage` — Defines browser persistent-storage policy.
- `rules::browser::remote_resource` — Defines browser remote-resource policy.
- `rules::browser::request` — Defines browser network-request policy.
- `rules::browser::script_injection` — Defines browser script-injection policy.
- `rules::browser::worker` — Defines browser worker-creation policy.
- `rules::electron` — Groups Electron host-policy rules.
- `rules::electron::dialog` — Defines Electron dialog policy.
- `rules::electron::ipc` — Defines Electron interprocess-communication policy.
- `rules::electron::module` — Defines Electron module-loading policy.
- `rules::electron::shell` — Defines Electron shell-execution policy.
- `rules::js` — Groups provider-neutral JavaScript runtime policy rules.
- `rules::js::eval` — Defines dynamic-evaluation policy.
- `rules::js::header_indicator` — Defines header-indicator policy.
- `rules::js::private_address` — Defines private-network-address policy.
- `rules::js::service_indicator` — Defines service-indicator policy.
- `rules::js::shared_memory` — Defines shared-memory policy.
- `rules::js::string_timer` — Defines string-based timer policy.
- `rules::js::telemetry_indicator` — Defines telemetry-indicator policy.
- `rules::js::url_construction` — Defines URL-construction policy.
- `rules::js::webassembly` — Defines WebAssembly policy.
- `rules::node` — Groups Node.js host-policy rules.
- `rules::node::archive_compression` — Defines Node archive and compression policy.
- `rules::node::crypto_operation` — Defines Node cryptographic-operation policy.
- `rules::node::filesystem` — Defines Node filesystem policy.
- `rules::node::network` — Defines Node network policy.
- `rules::node::process_environment` — Defines Node process-environment policy.
- `rules::node::subprocess` — Defines Node subprocess policy.

### Structs and enums

- `JavaScriptCatalogBundle` — Bundles the four JavaScript provider catalogs for callers that need them together.
- `JavaScriptTarget` — Selects the JavaScript host environment modeled by a catalog.
- `rules::node::archive_compression::ImportSpec` — Represents the supported archive-module import shapes.

## `glass-lint-obsidian`

This crate supplies Obsidian API policy rules and the Obsidian renderer
environment on top of the JavaScript provider.

### Modules

- `glass_lint_obsidian::api_manifest` — Describes the supported Obsidian API manifest.
- `glass_lint_obsidian::catalog` — Caches and exposes the Obsidian rule catalog.
- `glass_lint_obsidian::rules` — Groups all Obsidian rule factories.
- `rules::bases` — Groups base-class registration rules.
- `rules::bases::register` — Defines base-class registration policy.
- `rules::cli` — Groups command-line interface rules.
- `rules::cli::register` — Defines command registration policy.
- `rules::codemirror` — Groups CodeMirror integration rules.
- `rules::codemirror::extension` — Defines CodeMirror extension policy.
- `rules::editor` — Groups editor API rules.
- `rules::editor::content` — Defines editor-content access policy.
- `rules::editor::extension` — Defines editor-extension policy.
- `rules::editor::suggest` — Defines editor suggestion policy.
- `rules::file_manager` — Groups file-manager rules.
- `rules::file_manager::frontmatter_write` — Defines file-manager frontmatter-write policy.
- `rules::lifecycle` — Groups plugin lifecycle rules.
- `rules::lifecycle::events` — Defines lifecycle event policy.
- `rules::markdown` — Groups Markdown rendering rules.
- `rules::markdown::code_block_processor` — Defines code-block processor policy.
- `rules::markdown::link` — Defines Markdown link policy.
- `rules::markdown::postprocessor` — Defines Markdown postprocessor policy.
- `rules::markdown::render` — Defines Markdown render policy.
- `rules::metadata` — Groups metadata-cache rules.
- `rules::metadata::cache_read` — Defines metadata-cache read policy.
- `rules::metadata::events` — Defines metadata event policy.
- `rules::metadata::extract` — Defines metadata extraction policy.
- `rules::metadata::frontmatter_read` — Defines frontmatter-read policy.
- `rules::metadata::traversal` — Defines metadata traversal policy.
- `rules::network` — Groups Obsidian network rules.
- `rules::network::request` — Defines Obsidian request policy.
- `rules::platform` — Groups platform capability rules.
- `rules::platform::branching` — Defines platform-branching policy.
- `rules::plugins` — Groups plugin-management rules.
- `rules::plugins::access` — Defines plugin access policy.
- `rules::plugins::enable_disable` — Defines plugin enable/disable policy.
- `rules::plugins::load_unload` — Defines plugin load/unload policy.
- `rules::storage` — Groups application and plugin storage rules.
- `rules::storage::app_data` — Defines application-data storage policy.
- `rules::storage::plugin_data_read` — Defines plugin-data read policy.
- `rules::storage::plugin_data_write` — Defines plugin-data write policy.
- `rules::ui` — Groups Obsidian user-interface rules.
- `rules::ui::command` — Defines UI command registration policy.
- `rules::ui::menu` — Defines menu policy.
- `rules::ui::modal` — Defines modal-dialog policy.
- `rules::ui::notice` — Defines notice-display policy.
- `rules::ui::ribbon` — Defines ribbon-action policy.
- `rules::ui::settings_tab` — Defines settings-tab policy.
- `rules::ui::status_bar` — Defines status-bar policy.
- `rules::vault` — Groups vault filesystem API rules.
- `rules::vault::access` — Defines vault access policy.
- `rules::vault::adapter` — Defines vault adapter policy.
- `rules::vault::config_directory` — Defines configuration-directory policy.
- `rules::vault::delete` — Defines vault deletion policy.
- `rules::vault::enumerate` — Defines vault enumeration policy.
- `rules::vault::events` — Defines vault event policy.
- `rules::vault::move_copy` — Defines vault move and copy policy.
- `rules::vault::read` — Defines vault read policy.
- `rules::vault::resource_url` — Defines vault resource-URL policy.
- `rules::vault::write` — Defines vault write policy.
- `rules::view` — Groups view registration rules.
- `rules::view::register` — Defines view registration policy.
- `rules::workspace` — Groups workspace rules.
- `rules::workspace::active_editor` — Defines active-editor access policy.
- `rules::workspace::active_file` — Defines active-file access policy.
- `rules::workspace::events` — Defines workspace event policy.
- `rules::workspace::layout` — Defines workspace-layout policy.
- `rules::workspace::leaf_management` — Defines workspace-leaf management policy.
- `rules::workspace::open` — Defines workspace-open policy.

### Structs and enums

- `ObsidianCatalogBundle` — Bundles the Obsidian catalog exposed to callers.
- `api_manifest::ObsidianApiManifest` — Describes the Obsidian API surface used to build policy rules.

## `glass-lint-output`

This crate renders core reports as deterministic terminal-oriented output.

### Modules

- `glass_lint_output::report` — Owns reusable report presentation.
- `report::render` — Converts report data into rendered terminal structures.
- `report::render::RuleGroupEntry` — Associates a pretty file with one finding during grouping.
- `report::types` — Defines pretty-report values and source-line caching.

### Structs and enums

- `report::types::Cell` — Stores one rendered table cell.
- `report::types::LineCache` — Caches source lines for repeated snippet rendering.
- `report::types::PrettyFile` — Represents one formatted file report.
- `report::types::PrettyOptions` — Configures terminal report formatting.
- `report::types::PrettyReport` — Represents one formatted analysis report.
- `report::types::PrettyReports` — Groups formatted reports for multiple files or projects.

## `glass-lint-harness`

This crate runs fixture cases and profiles the production engine through
normalized adapter and report boundaries.

### Modules

- `glass_lint_harness::adapters` — Connects built-in and external analysis tools to the harness protocol.
- `glass_lint_harness::builtins` — Selects built-in providers and profiling profiles.
- `glass_lint_harness::bundler` — Defines the bounded process boundary for generated bundle assets.
- `glass_lint_harness::cases` — Parses snippet directives and project manifests.
- `cases::project` — Parses multi-file project case manifests.
- `cases::snippet` — Parses single-file fixture directives.
- `glass_lint_harness::profile` — Coordinates deterministic corpus profiling.
- `profile::config` — Defines profiling workload and catalog configuration.
- `profile::corpus` — Selects and prepares profile corpus inputs.
- `profile::metrics` — Collects profiling phase and operation metrics.
- `profile::runner` — Executes profiling workloads.
- `profile::runner::admitted` — Runs profiling over already admitted sources.
- `profile::runner::files` — Prepares file-based profiling workloads.
- `profile::runner::projects` — Runs project-based profiling workloads.
- `profile::runner::summary` — Aggregates profiling run results.
- `profile::runner::support` — Provides shared profiling runner support operations.
- `profile::runner::workers` — Coordinates profiling worker execution.
- `profile::types` — Defines normalized profiling summaries and run values.
- `glass_lint_harness::profile_manifest` — Validates immutable profile corpus manifests.
- `glass_lint_harness::report` — Renders suite and comparison reports.
- `glass_lint_harness::runner` — Executes cases and combines adapter timings.
- `glass_lint_harness::types` — Defines normalized case, protocol, and report types.
- `types::case` — Defines fixture cases, selectors, and expectations.
- `types::protocol` — Defines the JSON adapter request and response protocol.
- `types::report` — Defines per-case, per-tool, and suite results.

### Structs and enums

- `adapters::ExternalAdapter` — Runs an external tool process for one case.
- `adapters::GlassLintAdapter` — Adapts the built-in Glass Lint engine to the harness protocol.
- `builtins::BuiltinProfile` — Selects a built-in profiling configuration.
- `builtins::BuiltinProvider` — Selects a built-in provider catalog and environment.
- `bundler::BundleOutput` — Stores generated bundle source and transformation metadata.
- `bundler::BundleRequest` — Carries a validated bundle transformation request.
- `bundler::BundleResponse` — Carries the normalized response from a bundle transformer.
- `bundler::ProcessBundler` — Runs the external bundle transformer process.
- `cases::project::ManifestResolutionOutcome` — Records how a project manifest resolution entry was interpreted.
- `cases::project::ProjectManifest` — Holds the normalized project case manifest.
- `cases::project::ProjectMetadata` — Stores project-case metadata used by adapters.
- `cases::project::ProjectResolutionManifest` — Stores expected project resolution records.
- `cases::project::ProjectToolManifest` — Stores the tool configuration for a project case.
- `profile::config::ProfileAnalysisLimits` — Records analysis limits used by a profile.
- `profile::config::ProfileCatalogProvider` — Selects the provider catalog used for profiling.
- `profile::config::ProfileConfig` — Holds validated profiling settings.
- `profile::config::ProfileConfigBuilder` — Builds profiling settings from caller options.
- `profile::config::ProfileCorpusIdentity` — Identifies the corpus used by a profile.
- `profile::config::ProfileExecutionIdentity` — Identifies the execution settings of a profile.
- `profile::config::ProfileProjectLoadIdentity` — Identifies project-loading settings used by a profile.
- `profile::config::ProfileWorkload` — Selects file or project profiling work.
- `profile::config::ProfileWorkloadIdentity` — Identifies the exact workload configuration.
- `profile::config::RuleSelectionProfile` — Selects the rule set used in a profile.
- `profile::runner::files::PreparedCorpus` — Holds corpus inputs ready for worker execution.
- `profile::types::MeasuredRepetitionAccumulator` — Accumulates measurements across one profile repetition.
- `profile::types::PreparedFile` — Stores a file prepared for profile execution.
- `profile::types::ProfileOperationCounts` — Counts semantic operations during profiling.
- `profile::types::ProfilePhaseTimings` — Records profiling phase durations.
- `profile::types::ProfileProjectRun` — Stores one project profiling run.
- `profile::types::ProfileProjectRunAccumulator` — Accumulates repeated project-run measurements.
- `profile::types::ProfileRepetitionSummary` — Summarizes one measured repetition.
- `profile::types::ProfileSummary` — Reports the complete profile result.
- `profile::types::ProfileSummaryAccumulator` — Accumulates profile workload summaries.
- `profile::types::ProfileSummaryMetadata` — Stores metadata needed to interpret a profile.
- `profile::types::ProfileWorkloadSummary` — Summarizes one profiled workload.
- `profile::types::RunOutcome` — Records the result of one profile run.
- `profile_manifest::ProfileManifest` — Represents a serialized profile manifest.
- `profile_manifest::ProfileManifestBody` — Stores the internal manifest payload.
- `profile_manifest::ProfileManifestEntry` — Describes one manifest corpus entry.
- `profile_manifest::VerifiedProfileManifest` — Represents a manifest after verification succeeds.
- `runner::AdapterTimings` — Records time spent by an adapter.
- `runner::BundleTimings` — Records time spent generating each bundle variant.
- `types::case::BundleKey` — Identifies one bundle profile, transformer, minification, and target combination.
- `types::case::BundleProfile` — Selects the host profile for bundle verification.
- `types::case::BundleProfileError` — Reports invalid bundle-profile directives.
- `types::case::BundleTarget` — Selects the JavaScript target for bundle transformation.
- `types::case::BundleTransformer` — Selects the bundle transformation tool.
- `types::case::Case` — Represents one normalized fixture case.
- `types::case::CaseError` — Reports invalid fixture-case input.
- `types::case::ExpectationError` — Reports invalid expectation syntax.
- `types::case::ExpectedCount` — Defines an exact or unconstrained expected finding count.
- `types::case::FindingExpectation` — Describes one expected diagnostic.
- `types::case::FindingExpectationError` — Reports invalid finding expectation values.
- `types::case::ProjectCase` — Represents a normalized multi-file project case.
- `types::case::ToolExpectation` — Groups expectations for one tool invocation.
- `types::case::ToolSelector` — Selects which adapter should run a case.
- `types::protocol::AdapterConversionError` — Reports failure converting protocol data into harness values.
- `types::protocol::AdapterEvidenceDto` — Carries one evidence item over the adapter protocol.
- `types::protocol::AdapterFile` — Carries one analyzed file over the adapter protocol.
- `types::protocol::AdapterFindingDto` — Carries one finding over the adapter protocol.
- `types::protocol::AdapterFindingError` — Reports invalid adapter finding data.
- `types::protocol::AdapterProject` — Carries project-level protocol data.
- `types::protocol::AdapterRequest` — Requests one case execution from an adapter.
- `types::protocol::AdapterResolution` — Carries one module-resolution record.
- `types::protocol::AdapterResolutionKind` — Classifies a serialized resolution outcome.
- `types::protocol::AdapterResolutionResult` — Wraps the resolution result and its partial status.
- `types::protocol::AdapterResponse` — Represents a normalized adapter response.
- `types::protocol::AdapterResponseDto` — Carries the wire-format adapter response.
- `types::protocol::AdapterSourceLocation` — Carries a source location over the wire.
- `types::protocol::AdapterStepDto` — Carries one evidence step over the wire.
- `types::protocol::AdapterTraceDto` — Carries one evidence trace over the wire.
- `types::report::AdapterRun` — Records one adapter's result for a case.
- `types::report::BundleResult` — Records verification results for one generated bundle.
- `types::report::CaseResult` — Records verification results for one case.
- `types::report::SuiteReport` — Aggregates all case results in a suite.
- `types::report::ToolResult` — Records one tool's findings and timing for a case.
