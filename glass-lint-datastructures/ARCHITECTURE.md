# Datastructures architecture

`glass-lint-datastructures` owns provider-neutral reusable storage primitives.
It contains no semantic analysis policy, no filesystem access, and no
provider names or APIs.

## Ownership

- `name` — bounded interned-name table with deterministic insertion order.
- `path` — owned (`NamePath`, `SymbolPath`) and borrowed (`PathView`) property
  path representations.
- `path_trie` — compact path interner (`PathInterner`, `ParentPathStore`) and
  overlay store for summary propagation.
- `table` — dense `IndexTable<T>` backed by `Vec<Option<T>>`, keyed by
  `IdIndex` types.
- `budget` — generic bounded-budget counter with exhaustion tracking.
- `fingerprint` — FNV-based hashing for deterministic content fingerprints.
- `diagnostic` — source-location types (`ByteRange`, `Position`, `SourceRange`)
  with validated construction.

## Invariants

- Every structure enforces its invariants at construction and mutation time.
- `NameId`, `PathId`, and similar indexes are opaque and crate-local; raw
  integer construction is not part of the supported public API.
- Storage is deterministic and allocation-conscious; order is preserved where
  semantically observable.
- Exhaustion is fail-closed: once a budget or capacity is reached, further
  insertions return an explicit error and the structure tracks the exhausted
  state.

## Relationship to core

This crate was extracted from `glass-lint-core` to eliminate duplicate path and
table implementations (see READ-009 in the workspace audit). Core retains
semantic newtypes and analysis policy; every reusable bounded data structure
lives here.
