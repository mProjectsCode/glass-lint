# Codebase Readability Audit

## Summary

Chunk 2 of `glass-lint-core` covers the fact-stream boundary and its
supporting syntax: `analysis::facts::{interface, origin_map, pattern, state,
stream, visitor, mod}`. The chunk's strongest design point is the
building-vs-frozen lifecycle: a single consuming `freeze` transition
(`stream.rs:329-340`, called from `resolver.freeze_into`,
`resolution/mod.rs:224-229`) moves the resolver's name/value tables into
phase-typed `FactStream<Frozen>`, exposing `names`/`values` only after
freezing; the shared-phase read surface (`fact`, `facts`, `paths`,
`function_parameters`) is read-only over private fields, so callers cannot
violate the dense-ID or bounded-budget invariants.

The main defects found are (1) a `FactStreamToken` that presents "construction
authority" but is bypassable because `SemanticFact`'s four fields are
`pub(in crate::analysis)`, so any crate-internal module can build
`SemanticFact { .. }` directly and ignore the token; (2) a
`FactStreamIssue::NameExhausted` bit that is set by two owners but never
distinctly consumed, duplicating the resolver-owned `IncompleteReason::NameExhausted`;
(3) the pattern walk threading `(path, path_known, default, rest)` as four
separate parameters through five recursive functions with constant rebuilds;
(4) a `OriginMap: Clone` impl that silently drops the open journal and is
unused; plus two small duplication findings (repeated `if let Pat::Ident` in
`exports.rs`, and cfg(test) mirror constructors duplicating production ones).
The origin-map checkpoint/snapshot types themselves are **not** over-engineered:
`provenance.rs` exercises `restore`/`rollback`/`commit` asymmetrically across
four channels, and `OriginSnapshot` is the only full-clone capture used at join
points (`control.rs`, `provenance.rs:100-161`).

## Findings

### Fact stream and authority

#### [ ] READ-001 — `FactStreamToken` grants no real construction authority because `SemanticFact` can be built by struct literal

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:33-46`, `glass-lint-core/src/analysis/model/fact.rs:447-474`

`FactStreamToken(())` is documented as "Construction authority held only by the
building fact stream", and `SemanticFact::new` requires the token
(`model/fact.rs:455-468`). But all four `SemanticFact` fields are
`pub(in crate::analysis)` (`model/fact.rs:447-452`), so any module in the crate
can construct `SemanticFact { id, span, function, payload }` by struct literal
and bypass the token entirely; the token is never stored and never checked, so
the only real gate is the module privacy of `FactStreamToken::new`
(`stream.rs:38-40`). The authority type therefore documents an invariant that
the public field surface already allows code to break, and it pushes the crate
to route every append through `stream.rs:262` for no enforcement benefit.

**Recommendation:** Make `FactStream` the owning authority: privatize the four
`SemanticFact` fields and expose read accessors (`id`, `span`, `function`,
`payload`) so `fact()`/`facts()`/`property_write_value` consumers still work,
keeping `SemanticFact::new` (token-gated) as the single constructor, with the
token constructible only from `stream.rs`. Alternatively delete the token and
mark `SemanticFact::new` `pub(super)` in the facts module so module privacy is
the documented authority. Guardrails: keep the dense-ID check in
`FactStream::append`/`push`, preserve the frozen-phase visibility, and keep the
`for_test` token path available to tests.

**Fix Applied:** None so far.

#### [ ] READ-002 — `FactStreamIssue::NameExhausted` has no distinct consumer and is set by two owners

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:25-31`, `stream.rs:135-150`, `stream.rs:294-296`; `glass-lint-core/src/analysis/semantic/mod.rs:204-243`, `semantic/mod.rs:356-364`

`FactStreamIssue::NameExhausted` is recorded by both `mark_name_exhausted`
(`stream.rs:294-296`) and `annotate_name_exhaustion` during seal
(`semantic/mod.rs:356-364`), and it only feeds the generic `issues.is_empty()`
backing `is_valid()` (`stream.rs:137`). Unlike `BudgetExhausted`,
`PathExhausted`, and `InvalidParserSpan` — each of which has an accessor that
`check_fact_construction_incompleteness`/`check_invalid_parser_span` turn into a
distinct `IncompleteReason` — there is no `name_exhausted()` accessor and no
`IncompleteReason` reads the bit; the distinct `NameExhausted` diagnostic comes
entirely from `resolver.name_exhaustion()` (`semantic/mod.rs:236-243`). The
stream therefore tracks the same event twice with one path never readable.

**Recommendation:** Pick one owner. Either surface the bit symmetrically (an
`name_exhausted()` accessor plus an explicit consumer, matching the other three
issue bits), or delete the stream bit, `mark_name_exhausted`, and
`annotate_name_exhaustion`, and route the fail-closed indexing decision through
the resolver's existing `name_table_exhausted`/`IncompleteReason::NameExhausted`.
Guardrails: name-exhausted artifacts must keep failing closed — indexing is
gated by `stream.is_valid()` (`matching/build.rs:69`) and projectability by
`is_projectable` (`facts/mod.rs:419-421`), so the removal must keep those gates
equivalent, not start consuming partial name state.

**Fix Applied:** None so far.

### Pattern normalization

#### [ ] READ-003 — pattern walk threads `(path, path_known, default, rest)` as four scalar parameters with constant destructure/rebuild

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/pattern.rs:84-148`, `pattern.rs:150-169`, `pattern.rs:171-183`, `pattern.rs:185-228`

`walk_pattern` and its three helpers carry `path`, `path_known`, `default`, and
`rest` as separate arguments through the recursion, rebuilding and destructuring
the `(PathId, bool)` pair at every array/object level (`pattern.rs:161-167`,
`196-204`) and re-typing a `PatternLeaf` per node, while only `parameter_bindings`
(`pattern.rs:59-82`) consumes `default`/`rest`. The separate flag invites a
stale path: `path_known` stays `true` even when `append_path` returns
`PathId::EMPTY` under budget exhaustion (`pattern.rs:163-166`, `206-210`), so
the "path is reliably known" invariant is spread across two values that can
disagree. Callers (`functions.rs:89-97`, `construction.rs:113-117`,
`assignments.rs:122-125`) only observe the leaf output.

**Recommendation:** Introduce a private `PatternWalkContext { path, path_known,
default, rest }` carried through `walk_pattern`/array/object walks as one value
(optionally with `default`/`rest` limited to `parameter_bindings`), leaving
`PatternLeaf` as the pure per-node result. Deletion target: the
`(path, path_known)` tuple rebuilds at `pattern.rs:161-167` and `196-204` and
the repeated explicit four-argument parameter lists. Guardrails: preserve
`append_path` budget/`EMPTY` handling, deterministic source-order leaf
emission, and the deliberately optimistic `path_known` semantics.

**Fix Applied:** None so far.

### Origin-map journaling

#### [ ] READ-004 — `OriginMap: Clone` silently drops the open journal and is unused

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/origin_map.rs:192-200`

The manual `Clone` impl copies only `map` and resets `log` and
`open_checkpoints` to `Vec::new()`/`0`, producing a copy that can no longer
participate in the active transactions while the source keeps its journal — a
silently divergent clone if ever taken mid-traversal. No caller uses it:
`provenance.rs:100-116` and `control.rs` capture join state only through
`snapshot`/`restore_snapshot`/`retain_common`, and a crate-wide search finds no
`OriginMap` clone. The impl is therefore misleading unused API surface on a
type whose doc emphasizes bounded, checkpoint-aware storage.

**Recommendation:** Remove the `Clone` impl (deletion target `origin_map.rs:192-200`)
so `OriginMap<V>` is not clonable, and drop the now-unneeded `V: Clone` bound if
none of the remaining methods require it. If a journal-free copy is ever truly
needed, give it an explicit name and document the reset semantics on the
operation itself. Guardrails: `snapshot`/`restore_snapshot`/`retain_common`
remain the only supported join-point capture paths, and the checkpoint-count /
budget invoicing behavior must stay unchanged.

**Fix Applied:** None so far.

### Interface construction

#### [ ] READ-005 — `record_export_decl`'s `Var` arm matches `Pat::Ident` twice per declarator

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/interface/exports.rs:59-76`

Inside the per-name loop the body matches `Pat::Ident(binding)` twice with the
same binding — once to add a function export plus the local export, once to
record a static-string value — expressing one decision about `declarator.name`
as two adjacent guarded blocks (`exports.rs:62-67` and `69-74`).

**Recommendation:** Merge the two `if let Pat::Ident(binding)` guards into one
block containing the function-export, local-export, and static-string
recordings, leaving `add_export(ModuleExport::Local)` unconditional. Guardrails:
keep the order — `record_pattern_locals` still runs first and the visitor's
later idempotent re-insert of locals stays a no-op — and keep the static-string
resolution unchanged so export observation semantics are identical.

**Fix Applied:** None so far.

### Construction paths

#### [ ] READ-006 — cfg(test) mirror constructors duplicate production constructors

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:78-87`; `glass-lint-core/src/analysis/facts/mod.rs:131-145`

`FrozenStorage::for_test` (`stream.rs:83-86`) has an identical body and
identical `pub(in crate::analysis)` visibility to `from_tables`
(`stream.rs:79-81`), so the two names are a pure rename; callers at
`facts/mod.rs:225` and `facts/tests/stream.rs` could use `from_tables` directly.
Relatedly, the 8-field `FactStream<Frozen>` struct literal is spelled twice
(`freeze` at `stream.rs:330-339` and the test `Default` at `stream.rs:368-380`),
and the builder pair `FactBuilder::new`/`with_limit` (`mod.rs:131-145`) follows
the same mirror pattern. Duplicated construction paths must stay in sync, which
is exactly where dense-ID, budget, and issues-handling regressions hide.

**Recommendation:** Delete `FrozenStorage::for_test` and its two call sites, and
reduce `FactStream<Frozen>` construction to one canonical spelling (e.g. a
`From<FactStream<Building>, FrozenStorage>` conversion or a single private
constructor used by both `freeze` and the test `Default`). Keep the cfg(test)
mirrors that genuinely differ in visibility or effect (`FactStreamToken::for_test`,
`FactBuilder::new`) but document them as intentional. Guardrails: the frozen
`Default` must keep `valid: true` and the empty issue set, and `freeze` must
keep transferring `valid`/`issues`/`function_parameters` verbatim.

**Fix Applied:** None so far.

## Systemic Themes

- **Phase-marker ownership is coherent.** The `Building`/`Frozen` phases with
  `Phase::Storage` and the zero-sized `_phase: PhantomData` correctly
  distinguish the mutable tape from the frozen artifact; `freeze` is the single
  consuming transition and the storage-shaped frozen accessors (`names`,
  `values`, `paths`, `facts`) are all immutable borrows over private fields. No
  caller can mutate the tape after freeze — this is the right ownership
  boundary, and `SemanticFacts` (`facts/mod.rs:373-432`) is a thin but
  legitimate artifact facade (its `names()`/`values()` forwarding is fine).
- **Repeated `budget().exhausted()` guards at the top of every `Visit` method**
  (`visitor.rs`, ~25 sites) sit on top of the same check inside
  `emit`/`record_*`. Because each guard must early-return from its own method,
  no single helper can own it without adding indirection; treat as a deliberate
  reliability net unless the visitor is redesigned around a guarded dispatch.
- **`OriginCheckpoint`/`OriginSnapshot` are proportionate, not over-engineered.**
  The asymmetric `restore` vs `rollback` vs `commit` surface maps to real
  four-channel provenance semantics (`provenance.rs:56-161`), and the
  `active` flag correctly prevents double-commit. The single-variant
  `LogEntry` enum (`origin_map.rs:39-41`, matched at `66-77`) was evaluated and
  is not worth a wrapper change.
- **Inconsistency from the chunk focus areas:** all four `interface`,
  `origin_map`, `pattern`, and `state` modules keep behavior on their owning
  types via inherent `impl` blocks with consistent `pub(in crate::analysis)` /
  `pub(super)` scoping, and all are idempotent/deterministic by construction.
  `state.rs` splits nesting counters (`function_depth`, `static_method_depth`)
  from the cached `current_function`; the two stay consistent because
  `with_function_context` saves and restores the cache (`functions.rs:39-55`).

## Open Questions

- `OriginMap::snapshot` charges the semantic budget once per full clone
  regardless of map size (`origin_map.rs:109-114`), and the doc explicitly makes
  the caller responsible for bounding snapshot count. Is that caller-side bound
  sufficient to keep allocations within budget for very large pre-join maps, or
  should the charge scale with the cloned map size?
- With READ-002, if the `NameExhausted` bit were deleted, is the fail-closed
  gate genuinely equivalent when `is_valid()` no longer absorbs name exhaustion
  (i.e. does `IncompleteReason::NameExhausted` + `SemanticFacts::is_projectable`
  cover every indexing path today)? This needs the semantic-layer callers
  verified before any change.
- `FactPayload::Import` is emitted from two equally-shaped sites for static
  imports (`facts/mod.rs:277`) and require/dynamic calls (`facts/calls/mod.rs:29-30`
  and `53-54`); is a single `emit_import_fact(module)` helper worth owning on
  `FactBuilder`, or do the different span/ordering contexts justify the
  duplicate two-liners?

## Coverage

Files inspected in Chunk 2: `facts/mod.rs`, `facts/visitor.rs`,
`facts/stream.rs`, `facts/state.rs`, `facts/pattern.rs`, `facts/origin_map.rs`,
`facts/origin_map/tests.rs`, `facts/stream_tests.rs`,
`facts/interface/{mod,commonjs,exports}.rs`. Lifecycle traced end-to-end from
the Source: `facts::build` -> `BuiltFacts` (holding `FactStream<Building>`) ->
`ResolvedProgram::collect`/`seal` (`semantic/mod.rs:301-379`, including the
redundant `annotate_name_exhaustion`) -> `Resolver::freeze_into`
(`resolution/mod.rs:224-229`) -> `FactStream<Frozen>` -> `SemanticFacts`
(`facts/mod.rs:373-432`), with read consumers in `matching/build.rs`
(`is_valid` gate), `flow/projector` and `flow/cross` (`values`/`names`/`paths`
reads), `effect` (`facts()`), and `propagation` (`property_write_value`).
Origin-map lifecycle callers: `facts/provenance.rs` (four-channel checkpoints,
snapshots, restore/rollback/commit) and `facts/control.rs` (branches, `control.rs:157-163`).
Pattern callers: `facts/construction.rs:98-135`, `facts/assignments.rs:122-125`,
`facts/functions.rs:77-106`. Definition of the model types under audit:
`model/fact.rs` (`FactId`, `SemanticFact`, `Building`/`Frozen` phases),
`model/module.rs` (`ModuleInterface`, `ModuleExport`), `scope/query/functions.rs`
(function-id resolution used by `interface/exports.rs` and `commonjs.rs`).
Only `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_02.md` was created; no source,
tests, configuration, or other documentation were modified.