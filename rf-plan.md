# RF plan: `glass-lint-datastructures` crate

Create a new workspace crate owning general-purpose data structures extracted
from `glass-lint-core`. Core is **not** migrated in this plan; that is a
follow-up task. The new crate should compile, pass its own tests, and present a
clean public API.

---

## What goes in

### Group 1 — Zero-dependency primitives (from `glass-lint-core/src/`)

These depend on nothing outside `std`. Copy, polish, own tests.

| Original file | Types | Notes |
|---|---|---|
| `budget.rs` | `Budget`, `BudgetTracker` | Keep. Add `Budget::remaining()`. |
| `fingerprint.rs` | `fnv_init()`, `fnv_write()` | Keep as free functions. Add `Fingerprint` newtype wrapping `u64` with `write(&mut self, bytes: &[u8])` and `init() -> Self`. |
| `diagnostic.rs` | `ByteRange`, `ReversedByteRange`, `InvalidSourceBoundary` | Keep. No changes needed. |
| `diagnostic.rs` | `Position`, `InvalidPosition` | Keep. No changes needed. |
| `diagnostic.rs` | `SourceRange`, `ReversedSourcePositionRange` | Keep. Depends on `Position`. |

**Not moving**: `Severity`, `SourceLineIndex`, `RuleMetadata` — stay in core.

### Group 2 — Interner suite (from `glass-lint-core/src/analysis/`)

These depend on `indexmap`, `smol_str`, `smallvec` (all already workspace
deps). The core copies have `pub(in crate::analysis)` visibility and methods
that pull in `NameTable`, `Environment`, and other core types; the new crate
versions drop those entangled methods.

| Original file | Types | What changes |
|---|---|---|
| `analysis/name.rs` | `NameId`, `NameTable`, `NameExhausted`, `MAX_NAMES` | Drop `lookup_path()` and `resolve_path()` (they reference `NamePath`/`SymbolPath`). Everything else comes as-is, made `pub`. Rename `MAX_NAMES` to `DEFAULT_MAX_NAMES`. |
| `analysis/value/identity.rs` | `NamePath` (segment-container subset) | Copy only: `new()`, `append()`, `segments()`, `first_segment()`, `last_segment()`, `without_first_segment()`, `without_last_segment()`, `append_path()`, `is_root()`, `from_ids()`, `is_equal_or_descendant_of()`, `Borrow<[NameId]>`. **Drop**: `without_segment()`, `without_this_prefix()`, `without_bind_suffix()`, `from_symbol_path()`, `to_symbol_path()`, `matches_global_object_alias_with()` — these depend on `NameTable` and/or `Environment` and stay in core. |
| `analysis/value/identity.rs` | `SymbolPath` (string-only subset) | Copy only: `from_segments()`, `from_chain()`, `first_segment()`, `without_last_segment()`, `without_first_segment()`, `segments()`, `append_path()`, `append_chain()`, `is_root()`, `is_empty()`, `without_bind_suffix()`, `without_this_prefix()`, `eq_chain()`, `is_equal_or_descendant_of()`, `Display`, `From<String>`, `From<SmolStr>`, `From<&str>`. **Drop**: `matches_global_object_alias()` — depends on `Environment` and stays in core. |
| `analysis/value/path.rs` | `PathId`, `PathSegment`, `PathSegmentInput`, `PathNode`, `ParentPathStore`, `PathInterner` | Keep as-is (already depend only on `NameId` from Group 2). Made `pub`. Rename `MAX_PATH_NODES` to `DEFAULT_MAX_PATH_NODES`. Promote test helpers (`last`, `first_index`, `without_first`, `concat`, `concat_with_buffer`, `node_count`) to production methods — they're generally useful. |
| `analysis/flow/table.rs` | `FunctionTable<T>` | Generalize to `IndexTable<I, T>` where `I: IdIndex` (new trait: `fn from_raw(u32) -> Self; fn into_raw(self) -> u32`). This breaks the dependency on `FunctionId` and makes the table work with any opaque ID. |

**Not moving** (stay in core): `ValueId`, `FunctionId`, `BindingId`,
`BindingVersion`, `BindingRoot`, `BindingKey`, `ObjectId`, `ValueTable`,
`Value`, `CallableValue`, `FactId`, `ControlRegionId`, `ScopeId`,
`ModuleRequestId`, `EvidenceList`, `ProjectRelativePath`.

---

## New crate structure

```
glass-lint-datastructures/Cargo.toml
glass-lint-datastructures/src/
├── lib.rs              # public re-exports
├── budget.rs           # Budget, BudgetTracker
├── fingerprint.rs      # Fingerprint, fnv_init, fnv_write
├── diagnostic.rs       # ByteRange, Position, SourceRange, error types
├── name.rs             # NameId, NameTable, NameExhausted
├── path.rs             # SymbolPath, NamePath, path algebra helpers
├── path_trie.rs        # PathId, PathSegment, PathNode, ParentPathStore, PathInterner
└── table.rs            # IdIndex trait, IndexTable<I, T>
```

**`lib.rs` re-exports**:

```rust
// Group 1
pub mod budget;
pub mod fingerprint;
pub mod diagnostic;

// Group 2
pub mod name;
pub mod path;
pub mod path_trie;
pub mod table;

// Convenience re-exports (flat)
pub use budget::{Budget, BudgetTracker};
pub use fingerprint::Fingerprint;
pub use diagnostic::{ByteRange, Position, SourceRange, ...};
pub use name::{NameId, NameTable, NameExhausted};
pub use path::{SymbolPath, NamePath};
pub use path_trie::{PathId, PathSegment, PathNode, ParentPathStore, PathInterner};
pub use table::{IdIndex, IndexTable};
```

---

## API polish

### `Fingerprint` newtype (`fingerprint.rs`)

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Fingerprint(u64);

impl Fingerprint {
    pub fn init() -> Self;
    pub fn write(&mut self, bytes: &[u8]);
    pub fn into_raw(self) -> u64;
}
```

Keeps the existing free functions as `pub(crate)` helpers.

### `IndexTable<I, T>` (`table.rs`)

```rust
pub trait IdIndex: Copy + Into<u32> {
    fn from_raw(raw: u32) -> Self;
}

pub struct IndexTable<I, T> { values: Vec<Option<T>>, _marker: PhantomData<I> }

impl<I: IdIndex, T> IndexTable<I, T> {
    pub fn new() -> Self;
    pub fn get(&self, id: I) -> Option<&T>;
    pub fn get_mut(&mut self, id: I) -> Option<&mut T>;
    pub fn insert(&mut self, id: I, value: T) -> bool;   // returns vacant
    pub fn get_disjoint(&mut self, read: I, write: I) -> Option<(Option<&T>, Option<&mut T>)>;
    pub fn iter(&self) -> impl Iterator<Item = (I, &T)>;
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (I, &mut T)>;
    pub fn values(&self) -> impl Iterator<Item = &T>;
    pub fn contains(&self, id: I) -> bool;
    pub fn len(&self) -> usize;
}
```

### `NameTable` (`name.rs`)

Remove `lookup_path()` and `resolve_path()`. Keep everything else. Make all
methods and fields `pub`.

### `NamePath` (`path.rs`)

Keep only the segment-container operations listed above. The
`NameTable`-dependent helpers (`without_segment`, `from_symbol_path`,
`to_symbol_path`, `matches_global_object_alias_with`) are **not** included;
they will live in core as free functions in the migration follow-up.

### `SymbolPath` (`path.rs`)

Keep all string-only operations. `matches_global_object_alias()` is **not**
included; it stays in core.

### `PathInterner` (`path_trie.rs`)

Promote the test-helpers to production: `last()`, `first_index()`,
`without_first()`, `concat()`, `concat_with_buffer()`, `node_count()`.

---

## Dependencies (`Cargo.toml`)

```toml
[dependencies]
indexmap.workspace = true
smol_str.workspace = true
smallvec.workspace = true

[dev-dependencies]
serde_json.workspace = true   # for serde diagnostic tests if serde feature added later

[features]
default = []
serde = ["smol_str/serde"]   # ByteRange/Position/SourceRange serde can be feature-gated
```

No dependency on any other glass-lint crate. No SWC. No `tracing`.

---

## Tests

Every type needs:

### Group 1
- `Budget`: overflow, exhaustion stickiness, `try_add` atomicity, `remaining`.
- `BudgetTracker`: mark/is_exhausted, default state.
- `Fingerprint`: deterministic output, different inputs produce different hashes,
  same input produces same hash.
- `ByteRange`: valid ranges, reversed rejection, empty, len, serde round-trip.
- `Position`: valid positions, zero-line rejection, zero-column rejection, serde.
- `SourceRange`: valid ranges, reversed rejection, `contains`, serde.
- `InvalidSourceBoundary`: Display.

### Group 2
- `NameTable`: shared IDs on re-intern, resolve round-trip, exhaustion (copied
  from core tests). Add: `lookup` miss, `with_max_entries` boundary.
- `NamePath`: empty path, single/multi segment, without_{first,last}, append,
  is_root, from_ids, is_equal_or_descendant_of, Borrow impl. Add: large path
  with SmallVec heap spill.
- `SymbolPath`: from_chain with dots, append_chain, display, without_{first,last},
  without_bind_suffix, without_this_prefix, eq_chain, From impls, is_root,
  is_equal_or_descendant_of. Add: edge cases — empty input, trailing/leading
  dots, consecutive dots.
- `PathInterner`/`ParentPathStore`: shared prefix canonicalization, property vs
  index distinction, depth, starts_with, without_first, concat, edge reuse
  (copied from core tests). Add: node_count tracking, max_nodes enforcement,
  invalid ID rejection.
- `IndexTable`: get/insert/get_mut, vacancy tracking, get_disjoint with
  overlapping and equal IDs, iter, values, contains, sparse slots, large-ID
  resize.

---

## What is NOT in scope

- **No changes to `glass-lint-core`** — not even adding a dependency on the new
  crate. Core keeps its own copies of these types for now.
- **No migration of core callers** — that is a separate follow-up task.
- **No extraction of** `Severity`, `SourceLineIndex`, `RuleMetadata`, `Category`,
  `Confidence`, `RuleId`, `AnalysisLimits`, `PositiveLimit`, `ProjectRelativePath`,
  `EvidenceList`, opaque ID markers, `ValueTable`/`Value`, `BindingRoot`/`BindingKey`,
  or any flow/facts/scope types.
- **No `tracing`**, **no `serde`** in the initial build (can be added later
  behind a feature flag).
- **No workspace dependency from the new crate to any other glass-lint crate.**

---

## Deliverables

1. `glass-lint-datastructures/Cargo.toml` with workspace membership.
2. `glass-lint-datastructures/src/lib.rs` with clean public API.
3. Eight source modules as specified above.
4. Unit tests for every public type covering:
   - Normal construction and access
   - Error/exhaustion paths (fail-closed)
   - Edge cases (empty, boundary, overflow)
   - Determinism and identity semantics
5. `make ci` passes (new crate compiles, tests pass, no clippy warnings on the
   new crate, existing crates unaffected).
