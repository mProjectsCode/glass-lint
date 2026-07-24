# Complexity Audit

Audited 293 Rust source files (60,463 total lines) for excessive length and
complexity. Rule catalog files (per-rule `mod.rs`) and their declarative
`rule()` functions are **exempt** — they are intentionally large because they
describe many match/flow clauses via the builder API, not because they contain
dense imperative logic.

## Files exceeding 500 lines

34 files exceed 500 lines (12% of all Rust files).

### glass-lint-core (27 files)

| Lines | File |
|------:|------|
|  1081 | `src/analysis/flow/cross/mod.rs` |
|  1077 | `src/analysis/flow/summary.rs` |
|  1039 | `src/analysis/scope/model.rs` |
|   924 | `src/analysis/flow/effect.rs` |
|   920 | `tests/declarative_matching.rs` |
|   847 | `src/analysis/project/linker.rs` |
|   830 | `tests/compact_source.rs` |
|   742 | `src/analysis/flow/projector/mod.rs` |
|   722 | `src/analysis/matching/arguments.rs` |
|   720 | `src/api/rule/decl.rs` (rule catalog — exempt) |
|   692 | `src/analysis/flow/projector/state.rs` |
|   686 | `src/analysis/scope/collect/mod.rs` |
|   682 | `src/project/types/report.rs` |
|   682 | `src/analysis/facts/build/mod.rs` |
|   664 | `src/analysis/facts/build/calls.rs` |
|   656 | `src/api/rule/matcher/flow.rs` (rule catalog — exempt) |
|   649 | `src/project/tests/linking_and_flow.rs` |
|   628 | `src/analysis/resolution/mod.rs` |
|   627 | `tests/linter.rs` |
|   621 | `src/api/compiler/rule.rs` (rule catalog — exempt) |
|   608 | `src/analysis/scope/query/provenance.rs` |
|   598 | `src/analysis/facts/build/interface.rs` |
|   566 | `src/analysis/matching/mod.rs` |
|   564 | `src/analysis/syntax/constant.rs` |
|   556 | `src/analysis/scope/collect/analysis.rs` |
|   518 | `src/analysis/matching/query.rs` |
|   512 | `src/report.rs` |

Most touch points: flow analysis (cross-module, summary, projector, effect) —
these are the most lengthy sub-systems in core. Large test files add to the
count.

### glass-lint-datastructures (2 files)

| Lines | File |
|------:|------|
|   910 | `src/path_trie.rs` |
|   697 | `src/path.rs` |

### glass-lint-project (2 files)

| Lines | File |
|------:|------|
|   695 | `src/tsconfig/mod.rs` |
|   689 | `src/loader.rs` |

### glass-lint-harness (3 files)

| Lines | File |
|------:|------|
|   744 | `src/profile/runner.rs` |
|   667 | `src/types.rs` |
|   632 | `src/cases.rs` |

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

1. **`glass-lint-core/src/analysis/flow/`** — This is the most sprawling
   directory. The cross-module flow module (`cross/mod.rs`, 1081 lines) and
   summary module (`summary.rs`, 1077 lines) are the two largest files in
   the project. The projector is split across 6 files totaling ~2300 lines.
   These modules contain the most dense logic in the project.

2. **`glass-lint-core/src/analysis/scope/model.rs`** (1039 lines) — the scope
   model type and all its query/construction logic live in one file.

3. **`glass-lint-datastructures/src/path_trie.rs`** (910 lines) — a data
   structure with significant algorithmic complexity in a single file.

4. **`glass-lint-core/src/analysis/project/linker.rs`** (847 lines) — the
   project linker is a single large module.

5. **`glass-lint-harness/src/profile/runner.rs`** (744 lines) — the profiling
   runner has several moderately complex functions and is one of the larger
   harness files.

6. **`render_adapter_comparison`** (103 lines, score 17) — the most complex
   function by branch count. It renders comparison output from adapter test
   reports with nested conditionals.

7. **`parse_case`** (53 lines, score 15) — high density of branching for its
   size. Parses test case expectations with many conditional paths.
