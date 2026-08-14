# Codebase Readability Audit

## Summary

Chunk 07 (`analysis::project`) has explicit ownership for module-qualified
identities, SCC partitioning, bounded export tables, lookup recursion, and
matcher projection. The phase transitions and fail-closed export handling are
well represented. The concrete readability opportunity is a small duplicated
snapshot operation in the linker: singleton and cyclic SCC resolution each
reimplement the same module-export extraction before invoking the shared
fixed-point setter.

## Findings

### [analysis/project/linker/export.rs]

#### [ ] READ-018 — Centralize module export snapshot construction

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/linker/export.rs:61-97`; `glass-lint-core/src/analysis/project/linker/mod.rs:42-57`

`ProjectLinker::resolve_single` and `ProjectLinker::resolve_cycle` both
extract the immutable module interface into owned
`Vec<(SmolStr, module::ModuleExport)>` values with the same
`exports().map(|(n, e)| (n.clone(), e.clone())).collect()` expression. The
singleton path snapshots one module and the cycle path snapshots every module
in the SCC, but neither needs a different extraction policy. Keeping the
logic in two places means a new export representation or snapshot invariant
must be updated twice, and makes it less obvious that the fixed-point loop is
the only semantic difference between the paths.

**Recommendation:** Add one private linker helper that snapshots the exports
for a `ModuleId`, then use it from both `resolve_single` and `resolve_cycle`.
Delete the repeated iterator/clone expression while retaining the owned
snapshot required to release `self.modules` borrows before calls to
`try_set_export`. Preserve deterministic interface order, the singleton
one-pass rule, cycle round bounds and `Unknown` fallback on non-convergence,
export-table budget accounting, and all existing export-resolution variants.
Add a regression test that compares singleton and multi-node SCC outcomes for
the same export shapes, including re-exports and namespace exports.

**Fix Applied:** None so far.

## Systemic Themes

- The project layer’s newtypes and state machines make module/request/export
  ownership explicit; no broad façade consolidation is recommended from this
  chunk.
- Export resolution deliberately uses owned snapshots to keep mutable fixed
  point state borrow-safe. The cleanup should centralize only construction,
  not collapse singleton and cyclic resolution policies.
- Lookup caching, SCC bounds, identity overlays, and projection ownership are
  retained as intentional bounded-phase responsibilities.

## Open Questions

- Would a small `ModuleExportsSnapshot` domain type make the ownership and
  deterministic-order contract clearer than a raw vector, or would it add a
  wrapper without behavior? Prefer the helper alone unless the snapshot gains
  additional invariants.
- Should export snapshot size be charged against the link budget separately
  from retained export-table entries if very large interfaces are expected?

## Coverage

Reviewed Chunk 07: qualified request/function identities; resolved-link input;
project semantic model; module graph normalization and SCC partitioning;
bounded export tables and lookup caches; transient linker state; graph and
export fixed-point resolution; import validation; export resolver recursion and
target conversion; module/call-result identity overlays; projection plans,
sessions, evidence handles, completion status, metrics, and cross-flow merge.
Read the root/core architecture, testing/contributing guidance, the complete
readability-audit skill instructions, and existing audits 001–006. No source
or test files were changed.
