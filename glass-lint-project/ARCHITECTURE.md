# Project architecture

`glass-lint-project` converts a filesystem selection into the owned sources and
typed resolution results consumed by `glass-lint-core`.

```text
ProjectSelection + validated ProjectLoadOptions
  -> canonical root and boundary checks
  -> deterministic discovery or tsconfig membership
  -> bounded source reads
  -> core `ProjectSession` source acceptance and local analysis
  -> `ProjectSession` authored resolution requests
  -> Oxc module resolution
  -> core linking and matching
  -> ProjectLoadOutcome
```

## Ownership

- `options` owns selection modes and all filesystem budgets.
- `boundary` owns the canonical project root, path acceptance classification,
  and accepted source-path invariants.
- `discovery` owns canonical paths, traversal, exclusions, `tsconfig`
  membership, and symlink policy.
- `resolver` owns Oxc configuration and classification of internal, external,
  missing, and unsupported requests.
- `loader` is the public loading interface and coordinates phase transitions and
  partial outcomes.
- `loader_metrics` owns phase timings and bounded load counters.
- `loader_phases` owns the path queue, resolution cache, and frontier progress
  state used by the loading loop.
- `source_collection` owns reusable, deterministic source-file discovery and
  loading.
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
- Preserve deterministic acceptance and resolution order.
- Treat unresolved or ambiguous internal requests as typed partial outcomes;
  never guess provenance.
- Keep filesystem limits separate from core semantic limits.
