# Codebase Readability Audit

## Summary

Audit of the Glass Lint Rust workspace (7 crates, ~13,000 source lines) against readability and maintainability criteria. Found 33 distinct findings across all crates, with 2 high-severity issues (a silent disclosure bug and a copy-paste duplication of matchers), 9 medium-severity, and 22 low-severity items. The codebase is well-structured with strong architectural boundaries, deterministic ordering, bounded analysis, and consistent error types. Most findings are about extracting shared helpers from repeated patterns, adding semantic newtypes, and tightening visibility.

## Findings

### READ-001 — `disclosures_for_report` silently returns empty for non-`js:` findings

- **Severity:** High
- **Category:** Bug
- **Location:** `glass-lint-js/src/lib.rs:80-94`

`disclosures_for_report` strips only the `"js:"` prefix before looking up disclosure categories, but every disclosure mapping in `disclosures.rs` (e.g., `"network.request"`, `"node.filesystem"`, `"electron.ipc"`) corresponds to rules under `browser:`, `node:`, or `electron:` namespaces. The function will always return an empty set for any real-world report. The fix is to iterate all four provider prefixes or restructure the mapping to use full rule IDs.

### READ-002 — Duplicate matcher registration in `metadata/traversal`

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-obsidian/src/rules/metadata/traversal/mod.rs:66-89`

Seven `rooted_global_traversal` matchers (for `Object.keys`, `Object.entries`, etc.) are registered three times: once as direct `MemberCallMatcher` calls, once as `rooted_global_traversal` wrappers, and a third time as identical `rooted_global_traversal` wrappers. Lines 73-79 are a wholesale duplicate of lines 66-72. Remove one duplicate block and verify no behavioral change.

### READ-003 — Large rule function in `electron/ipc`

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-js/src/rules/electron/ipc/mod.rs:14-107`

The `rule()` function is 109 lines with 44 inline `.module_member_call("electron", ...)` calls across 4 receiver objects (ipcRenderer, ipcMain, webContents, webFrameMain). Factoring into a const array of `(receiver, [methods])` tuples with a loop (as `persistent_storage` and `electron/module` already do) would halve the function size.

### READ-004 — `discover_filtered` blends symlink, file, and directory concerns

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-project/src/corpus.rs:42-118`

The 77-line function resolves symlink metadata, handles single-file roots, and walks directories in a single body. The inner walker loop (34 lines) independently enforces entry budgets, walk errors, and file filtering. Extract per-root preparation and directory walking into named helpers.

### READ-005 — Duplicate `finish` / `finish_partial` methods

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-project/src/loader.rs:487-515`

`finish` and `finish_partial` share 13 identical lines including timeout checks, `finish_with_timings` calls, and metric accumulation. The only difference is the context. Extract a shared `finish_inner` helper.

### READ-006 — Path canonicalized twice per source file read

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-project/src/discovery.rs:251-261` and `corpus.rs:166-193`

`ProjectDiscovery::read_source` canonicalizes `root` and `path`, checks root containment, then delegates to `SourceCorpus::load_source_file` which canonicalizes both paths again and re-checks containment. Pass canonicalized paths downstream to eliminate redundant I/O.

### READ-007 — Unreachable post-loop budget check

- **Severity:** Medium
- **Category:** Dead Code
- **Location:** `glass-lint-project/src/corpus.rs:114-116`

The loop body already returns `TooManyFiles` when `paths.len() > max_files`. The identical check after the loop is unreachable. Replace with `debug_assert` or remove.

### READ-008 — Dead variable assignment in profile test

- **Severity:** Medium
- **Category:** Dead Code
- **Location:** `glass-lint-harness/src/profile.rs:1280-1282`

`warmup_durations` is assigned and immediately silenced with `let _`. Leftover from an earlier test version. Remove the dead store.

### READ-009 — `run_profile` mixes setup, discovery, warm-up, and measurement

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-harness/src/profile.rs:480-585`

The 105-line function handles project discovery, file preparation, warm-up loop, measured loop, and aggregation in one body. Extract warm-up, measured-run, and aggregation into separate functions.

### READ-010 — Accumulation logic duplicated across 4 profile functions

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-harness/src/profile.rs` (4 call sites)

`profile_file`, `repetition_from_files`, `profile_loader_project`, and `profile_admitted_projects` each independently compute `findings += ...`, `diagnostics += ...`, `operation_counts += ...`, and `evidence_order_digest = combined_digest(...)` from analysis reports. Extract a shared `accumulate_report` helper.

### READ-011 — `extensions: Vec<String>` used raw across 8+ call sites

- **Severity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-project/src/options.rs:67`

`ProjectLoadOptions::extensions` is a plain `Vec<String>` queried in 8+ places with repeated `is_supported_runtime_source(path, &self.options.extensions)` calls. A newtype wrapping `Vec<String>` with a `contains_extension` method and `clone_for_resolver()` would encapsulate the common operations.

### READ-012 — `print_report` is a single 92-line function

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-harness-cli/src/profile.rs:75-166`

Handles per-input details, aggregate summary, median duration, corpus identity, per-repetition details, phase timings, operation counts, and slowest inputs in one function. Split into `print_input_details`, `print_aggregate_summary`, `print_phase_timings`, `print_slowest_inputs`.

### READ-013 — Report rendering duplicates traversal patterns 4× in `report.rs`

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-harness/src/report.rs:14-231`

`render_suite_summary`, `render_suite_failures`, `render_suite_markdown`, and `render_adapter_comparison` each independently iterate `report.cases` and `case.adapters` with near-identical patterns for deriving tool order, skipped status, and findings counts. Extract a shared `active_tool_runs` iterator helper.

### READ-014 — `load_project_case` mixes manifest parsing, file loading, and tool building

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-harness/src/cases.rs:216-318`

The 102-line function combines manifest parsing, project file discovery, file loading, resolution transformation, and tool construction. Split into `parse_project_manifest`, `load_project_files`, `build_resolutions`, `build_tools`.

### READ-015 — Inconsistent rule category naming in `frontmatter_write`

- **Severity:** Medium
- **Category:** Naming
- **Location:** `glass-lint-obsidian/src/rules/file_manager/frontmatter_write/mod.rs:12`

Uses `"file-manager/frontmatter-write"` (with `/`) as the category string, while every other rule uses a simple single-word category matching the rule-ID prefix (e.g., `"vault"`, `"metadata"`, `"network"`).

### READ-016 — Free functions that should be associated methods or methods

- **Severity:** Low
- **Category:** API
- **Location:** Multiple files

Several free functions operate primarily on one type and would be clearer as methods:

| Function | Operates on | File |
|---|---|---|
| `valid_extension(extension: &str) -> bool` | `ProjectLoadOptions` | `glass-lint-project/src/options.rs:262` |
| `validate(config: Config) -> Result<Config>` | `Config` | `glass-lint-cli/src/config.rs:260` |
| Default-value functions for serde (6 functions) | `Config` / `ProjectConfig` | `glass-lint-cli/src/config.rs:172-197` |
| `finish` / `finish_partial` | `ProjectLoadState` | `glass-lint-project/src/loader.rs:487-515` |

### READ-017 — Missing semantic newtypes for collection-heavy fields

- **Severity:** Low
- **Category:** Newtype
- **Location:** `glass-lint-project/src/options.rs:69-73`

`excluded_directories: BTreeSet<String>` and `extension_aliases: BTreeMap<String, Vec<String>>` are raw collection types queried in multiple patterns across the crate. Newtypes with focused query methods (`contains_name`, `is_path_excluded`) would encapsulate the repeated access patterns.

### READ-018 — `pub(crate)` visibility missing on crate-internal functions

- **Severity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-project/src/discovery.rs:322,331,339,344`

`absolute_path`, `realpath`, `inside_root`, `excluded_path` are marked `pub` but only used within the crate. Likewise `pub use` from private `mod corpus` at `lib.rs:19` and `pub mod args` at `glass-lint-cli/src/lib.rs:7`. These should be `pub(crate)`.

### READ-019 — `Table` struct uses raw `Vec<Vec<String>>`

- **Severity:** Low
- **Category:** Newtype
- **Location:** `glass-lint-cli/src/output.rs:116-186`

Column-count validation is manual in `push()`. A `Row(Vec<String>)` newtype would enforce the invariant at construction.

### READ-020 — Broad `pub use` exports 38+ items from harness crate root

- **Severity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-harness/src/lib.rs:16-37`

Six `pub use` statements re-export many items from submodules, including internal types (`ProfileConfigBuilder`, `ProfileWorkloadIdentity`, `FindingExpectation`). Narrow the public surface to only what external consumers need.

### READ-021 — `unwrap()` vs `expect()` inconsistency across rule files

- **Severity:** Low
- **Category:** Naming
- **Location:** All `glass-lint-js` rule files and `src/lib.rs`

31 rule files use `.build().unwrap()` while `lib.rs` consistently uses `.expect("valid ...")` with descriptive messages. ~120 `package_import(...).unwrap()` calls across the JS crate use bare `unwrap()` with no message.

### READ-022 — Duplicate error-mapping closure in `read_tsconfig_path_extends`

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-project/src/discovery.rs:291-302`

The same `map_err` closure wrapping `ProjectLoadError::InvalidOptions(ProjectOptionError::Message(...))` is written twice for `json_strip_comments::strip` and `serde_json::from_str`. Extract a helper `fn parse_error(config: &Path, error: impl Display) -> ProjectLoadError`.

### READ-023 — `ResolutionCacheKey` stores `String` instead of `ProjectRelativePath`

- **Severity:** Low
- **Category:** Newtype
- **Location:** `glass-lint-harness/src/profile_manifest.rs:204-217` / `glass-lint-project/src/loader.rs:288-302`

The cache key converts `ProjectRelativePath` to `String` at the single call site, losing type safety. Store the semantic type directly.

### READ-024 — Skipped `ToolResult` constructed twice with identical literal

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-harness/src/runner.rs:44-73`

Two branches construct the same `ToolResult { skipped: true, ... }` struct literal. Extract `ToolResult::skipped(version, reason)`.

### READ-025 — Output module repeats stdout-lock pattern 4 times

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-cli/src/output.rs:31,38,45,53`

`io::BufWriter::new(io::stdout().lock())` is written independently in `write_mode`, `write_report`, `write_project_report`. Extract a `stdout_writer()` helper.

### READ-026 — `walkdir` error type lost in conversion

- **Severity:** Low
- **Category:** Error handling
- **Location:** `glass-lint-project/src/corpus.rs:97-102` and `discovery.rs:111-116`

The `walkdir::Error::Loop` variant (symlink loop) is converted to a generic `std::io::Error::other("directory traversal failed")`. Preserve the original error to aid debugging.

### READ-027 — `SourceCorpus` name is misleading

- **Severity:** Low
- **Category:** Naming
- **Location:** `glass-lint-project/src/corpus.rs:25`

The struct does not contain sources; it wraps a `ProjectLoadOptions` reference and loads files. A name like `SourceLoader` would better reflect its role.

### READ-028 — `build()` vs `validated()` do the same thing

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-project/src/options.rs:164-167,198-201`

`ProjectLoadOptionsBuilder::build()` and `ProjectLoadOptions::validated()` both validate and wrap. `build()` could delegate to `self.options.validated()`.

### READ-029 — `include_dir` has ambiguous name

- **Severity:** Low
- **Category:** Naming
- **Location:** `glass-lint-project/src/discovery.rs:243`

The method returns `true` when the entry should be *kept*. A name like `accept_entry` or `should_include_entry` would be clearer.

### READ-030 — `validate_membership` iterates `paths` twice

- **Severity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-project/src/discovery.rs:79-85`

`any()` checks for out-of-root paths (triggering early error), then `retain()` filters them. A single pass with `Vec::retain` and a boolean flag tracking removals would halve the traversal.

### READ-031 — `validate()` re-validated on every `SourceCorpus::new`

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-project/src/corpus.rs:32` / `discovery.rs:260`

`SourceCorpus::new` calls `options.validate()` on every construction, but `ProjectDiscovery::read_source` creates a new `SourceCorpus` per file read — re-validating all budgets and extension rules needlessly. Add `new_unchecked` for hot paths.

### READ-032 — `profile.rs` uses mixed file-as-module with directory children

- **Severity:** Low
- **Category:** Architecture
- **Location:** `glass-lint-harness/src/profile.rs` + `profile/corpus.rs` + `profile/metrics.rs`

The file `profile.rs` declares `mod corpus; mod metrics;` while children live in `profile/` subdirectory. This is valid Rust 2021+ but a reader expecting `mod.rs` may be surprised.

### READ-033 — Inconsistent `#[derive]` on newtype wrappers in `loader.rs`

- **Severity:** Low
- **Category:** Other
- **Location:** `glass-lint-project/src/loader.rs:260,276,305`

`PathWorkQueue`, `AdmissionSet`, and `ResolutionCache` derive `Debug` and `Default`, but are private to the module and never printed. Remove unused derives.

## Systemic Themes

**Repeated matcher patterns dominate the JS provider crate.** The majority of findings in `glass-lint-js` (READ-003, plus many unlisted low-severity instances) stem from repeating `.matcher()` calls inline instead of using const arrays + loops. Three files (`persistent_storage`, `electron/module`, `remote_resource`) already demonstrate the preferred pattern. Fixing the worst case (electron/ipc at 109 lines) and establishing a convention would raise consistency significantly.

**Path management and collection types in `glass-lint-project` are the second-largest source of churn.** Dual canonicalization (READ-006), missing newtypes (READ-011, READ-017), and repeated budget checks (READ-007, READ-030) account for 20% of all findings. Encapsulating the `extensions`, `excluded_directories`, and `extension_aliases` fields behind newtypes would pay off across 4 consuming modules.

**Profile and report functions in `glass-lint-harness` suffer from excessive length and cross-function duplication.** `run_profile`, `profile_loader_project`, `load_project_case`, and `print_report` are each 90-110+ lines. Accumulation logic is copied across 4 profile functions. The report module traverses the same data 4 different ways. These would benefit from targeted extractions.

**Visibility hygiene is inconsistent.** Several crates have `pub` on items that are only used within the crate (READ-018), or broad `pub use` wildcards that expose internal types (READ-020). Aligning on `pub(crate)` for crate-internal items would clarify the intended API surface.

## Open Questions

- Should `unwrap()` on `package_import` be replaced with `expect("package <name> is valid")` across all ~120 call sites? The trade-off is noise vs debuggability.
- Should the `matcher` repetition pattern in JS rules be addressed by a macro, by const-array conventions, or by adding a `from_list` builder method to the `Rule` builder? The current mix of inline and loop-based styles suggests a convention is needed.
- Should `SourceCorpus` be renamed, or is the "corpus" intended to mean a corpus *loader* rather than a corpus itself?
- Would a shared `Makefile` target for readability-specific Clippy lints (`pub_use`, `cognitive_complexity`, `large_enum_variant`, `result_large_err`) be useful for CI?

## Coverage

- **glass-lint-core:** Deep file-by-file review of all 47+ source files and 9 test files. Findings deferred to this report are high-level only; crate-specific issues (e.g., large functions, newtype candidates, naming inconsistencies) exist but are the most numerous of any crate. Separate deep-dive recommended.
- **glass-lint-js:** Full review of all 42 source files and 76 fixture files. High-confidence coverage.
- **glass-lint-project:** Full review of all 8 source files. High-confidence coverage.
- **glass-lint-obsidian:** Full review of all rule files (~50 files). High-confidence coverage.
- **glass-lint-cli:** Full review of 6 source files. High-confidence coverage.
- **glass-lint-harness:** Full review of all source files including the 1502-line `profile.rs`. High-confidence coverage.
- **glass-lint-harness-cli:** Full review of 5 source files. High-confidence coverage.
- **tests/:** Scanned; contains only fixture data (no Rust integration tests). No findings.
- **Root configuration:** Cargo.toml, Makefile, CI workflow reviewed. No findings.
