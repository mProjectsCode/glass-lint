# Project architecture

`glass-lint-project` converts a filesystem selection into the owned sources and
typed resolution results consumed by `glass-lint-core`.

```text
ProjectSelection + validated ProjectLoadOptions
  -> canonical root and boundary checks
  -> deterministic discovery or tsconfig membership
  -> bounded source reads
  -> core `ProjectCollection` source admission and local analysis
  -> `LocallyAnalyzedProject` authored resolution requests
  -> Oxc module resolution
  -> `ResolvedProject` and typed resolver outcomes
  -> core linking and matching
  -> ProjectLoadOutcome
```

## Ownership

- `options` owns selection modes and all filesystem budgets.
- `discovery` owns canonical paths, traversal, exclusions, `tsconfig`
  membership, and symlink policy.
- `resolver` owns Oxc configuration and classification of internal, external,
  missing, and unsupported requests.
- `loader` coordinates admission, resolution, partial outcomes, and metrics.
- `corpus` owns reusable, deterministic source-corpus loading.
- `error` owns expected loading and boundary failures.

The crate may depend on core's public project types. Resolver handles,
filesystem handles, absolute host paths, and Oxc types must not cross into
core.

## Invariants

- Validate options before I/O.
- Establish one canonical project root and reject escapes.
- Keep discovery, reads, resolver requests, aggregate bytes, and elapsed load
  time bounded.
- Do not follow symlinks unless explicitly enabled.
- Preserve deterministic admission and resolution order.
- Treat unresolved or ambiguous internal requests as typed partial outcomes;
  never guess provenance.
- Keep filesystem limits separate from core semantic limits.
