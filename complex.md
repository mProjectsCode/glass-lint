# Complexity Audit

Audited 301 Rust source files (61,318 total lines) for excessive length and
complexity. Rule catalog files (per-rule `mod.rs`) and their declarative
`rule()` functions are **exempt** — they are intentionally large because they
describe many match/flow clauses via the builder API, not because they contain
dense imperative logic.

## Files exceeding 500 lines

31 files exceed 500 lines (10% of all Rust files; down from 34 = 12%).

### glass-lint-core (22 files)

| Lines | File |
|------:|------|
|   720 | `src/api/rule/decl.rs` (rule catalog — exempt) |
|   656 | `src/api/rule/matcher/flow.rs` (rule catalog — exempt) |
|   649 | `src/project/tests/linking_and_flow.rs` |
|   621 | `src/api/compiler/rule.rs` (rule catalog — exempt) |
|   506 | `src/analysis/scope/model/graph.rs` |
|   499 | `src/project/session/mod.rs` |
|   499 | `src/analysis/scope/collect/tests.rs` |
|   487 | `src/analysis/flow/summary/store.rs` |
|   472 | `src/analysis/project/linker/export.rs` |
|   471 | `src/analysis/flow/projector/state.rs` |
|   456 | `src/analysis/project/model.rs` |
|   456 | `src/analysis/local.rs` |
|   453 | `src/analysis/facts/build/visitor.rs` |
|   443 | `src/environment.rs` |
|   434 | `src/analysis/matching/occurrence.rs` |
|   420 | `src/analysis/facts/mod.rs` |
|   408 | `src/analysis/module.rs` |
|   403 | `src/parse.rs` |
|   400 | `src/project/report/tests.rs` |
|   398 | `src/analysis/flow/projector/tests.rs` |
|   397 | `src/analysis/flow/cross/mod.rs` |
|   388 | `src/analysis/resolution/expression.rs` |

### glass-lint-datastructures

No files over 500 lines.

### glass-lint-project

| Lines | File |
|------:|------|
|   689 | `src/loader.rs` |
|   491 | `src/tsconfig/mod.rs` |
|   458 | `src/tsconfig/tests.rs` |

### glass-lint-harness

| Lines | File |
|------:|------|
|   744 | `src/profile/runner.rs` |
|   574 | `src/types/mod.rs` |
|   558 | `src/cases.rs` |
|   411 | `src/profile/mod.rs` |

### glass-lint-js, glass-lint-obsidian, glass-lint-cli, glass-lint-harness-cli

No files over 500 lines. The per-rule module structure keeps each file small.

---

## Large functions (≥80 lines) — non-rule

Only non-catalog functions listed. Rule catalog `rule()` functions are
exempt (declarative builders).

| Lines | File | Function |
|------:|------|----------|
|  111 | `glass-lint-core/src/project/tests/linking_and_flow.rs:8` | `linked_internal_aliases_*` |
|  103 | `glass-lint-harness/src/report.rs:143` | `render_adapter_comparison` |
|   98 | `glass-lint-harness/src/profile/runner.rs:366` | `profile_admitted_projects` |
|   94 | `glass-lint-core/src/project/tests/linking_and_flow.rs:220` | `linked_unknown_exports_*` |
|   90 | `glass-lint-core/src/project/tests/cache_and_session.rs:231` | `all_fingerprint_dimensions_*` |

---

## Functions with highest branch complexity (non-rule)

Branches counted as `if` / `else` / `match` / `for` / `while` / `loop` /
`&&` / `||` occurrences in function body.

| Score | Lines | File | Function |
|------:|------:|------|----------|
|   17  |  103 | `glass-lint-harness/src/report.rs:143` | `render_adapter_comparison` |
|   15  |   53 | `glass-lint-harness/src/cases.rs:92` | `parse_case` |
|   13  |   64 | `glass-lint-project/src/tsconfig/mod.rs:280` | `merge_selection` |
|    8  |   98 | `glass-lint-harness/src/profile/runner.rs:366` | `profile_admitted_projects` |
|    6  |   61 | `glass-lint-harness-cli/src/profile.rs:13` | `run` |

---

## Notable areas of concern

1. **`glass-lint-core/src/analysis/flow/`** — The most sprawling directory.
   The cross-module flow module (`cross/mod.rs`, 397 lines) and summary module
   (`store.rs`, 487 lines) have been significantly reduced. The projector is
   split across 7 files totaling ~2400 lines. Individual files are manageable
   but the directory remains dense.

2. **`glass-lint-core/src/analysis/scope/model/graph.rs`** (506 lines) — The
   scope graph type and all its query/construction logic in one file. Just
   over the threshold; a future split of `FrozenScopeGraph` into `frozen.rs`
   would bring both halves under 300 lines.

3. **`glass-lint-harness/src/profile/runner.rs`** (744 lines) — The profiling
   runner is the largest remaining production file. Contains both file-profile
   and project-profile functions. A split into `file.rs` and `project.rs` would
   bring each under 400 lines.

4. **`glass-lint-project/src/loader.rs`** (689 lines) — The project loader
   bundles public types and internal state machinery. Extracting `PathWorkQueue`,
   `ResolutionCache`, `LoadProgress`, and `ProjectLoadState` into a `state.rs`
   submodule would bring this under 250 lines.

5. **`glass-lint-harness/src/types/mod.rs`** (574) and
   **`glass-lint-harness/src/cases.rs`** (558) — Both are type-and-parse-heavy
   files. Extracting directive parsing helpers from `cases.rs` into a
   `directives.rs` file, and splitting adapter types from case types in
   `types/mod.rs`, would bring each under 400 lines.

6. **`render_adapter_comparison`** (103 lines, score 17) — the most complex
   function by branch count. It renders comparison output from adapter test
   reports with nested conditionals.

7. **`parse_case`** (53 lines, score 15) — high density of branching for its
   size. Parses test case expectations with many conditional paths.

8. **`merge_selection`** (64 lines, score 13) — moved to `selection.rs` but
   still carries complex inheritance logic.
