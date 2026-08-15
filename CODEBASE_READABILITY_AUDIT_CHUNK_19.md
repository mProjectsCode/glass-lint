# Codebase Readability Audit — glass-lint-core Chunk 19: Lifecycle and value queries

## Summary

Chunk 19 covers the authored query API for lifecycle correlation and static-value
constraints in `glass-lint-core`: `api/rule/query/lifecycle` (builder, endpoint,
types, private sealing), `api/rule/query/value`, and `api/rule/query/limits`. The
types are validated at construction, canonicalize events/sinks into bounded
collections, and are lowered by the compiler (`api/compiler/normalize.rs`,
`validate/pass4_10.rs`, `object_flow.rs`) and explained by
`api/rule/query/explanation.rs`. The public surface (`api/rule/mod.rs`) re-exports
the lifecycle and value constructors; `glass-lint-js` browser rules are the
principal external callers.

The chunk is generally well-encapsulated (private `*Kind` enums behind
`kind()` accessors, sealed `Into*` adapters, semantic `ArgumentIndex` newtype,
shared `push_argument_constraint` helper). The findings concentrate on
inconsistent canonicalization and representation within the chunk: a raw `usize`
argument position that bypasses the `ArgumentIndex` newtype, two parallel bounded
iterator-conversion helpers, nullable state on the built `LifecycleQuery` that
forces the compiler to re-validate builder-guaranteed invariants, duplicated
builder scaffolding across the two lifecycle builders, an untrimmed property
name that breaks the chunk's trim-normalization convention, redundant
crate-visible storage on `LifecycleEvent`/`LifecycleSink`, a constraint-vector +
count-map pair kept in sync by a free function, and lifecycle sources excluded
from the canonical ordering invariant applied to events and sinks.

## Findings

### [api/rule/query/lifecycle/types.rs]

#### [x] READ-001 — Raw `usize` sink argument index duplicates the `ArgumentIndex` semantic newtype

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle/types.rs:363-372, 421-429`

`LifecycleSinkKind::ArgumentOf { endpoint, index: usize }` stores the authored
argument position as a raw `usize`, and `LifecycleSink::build_call_sink`
validates it by hand (`if index > limits::MAX_ARGUMENT_INDEX`, types.rs:423-425,
returning `QueryBuildError::InvalidArgumentIndex`). `value.rs` already defines
`ArgumentIndex` (`value.rs:9-29`) — a validated `u8` newtype whose
`try_from_usize` performs the same bounds check and returns the same error
variant. The chunk therefore has two parallel representations of "authored
argument position", and the sink's hand-rolled check is a second copy of
`ArgumentIndex::try_from_usize` (the check in `try_from_usize` also makes its
inner `u8::try_from` fallback unreachable, value.rs:21-23). The normalized
compiler IR and flow engine keep `usize` (`normalized.rs:266-268`,
`object_flow.rs:298-300`), so only the authored layer is affected, but two
authors of the same concept produce drift risk (e.g. one side forgetting the
bound check).

**Recommendation:** Store `ArgumentIndex` in `LifecycleSinkKind::ArgumentOf`
and convert in `build_call_sink` via `ArgumentIndex::try_from_usize`, deleting
the manual bound check. Keep the compiler/normalized/flow layers on `usize`
unchanged so the fix stays inside the chunk; guardrail: preserve the
`InvalidArgumentIndex(index)` error and the `index.get()` conversions at the
two `ArgumentOf { index }` consumers (`normalize.rs:454`, `explanation.rs:264`).

**Fix Applied:** `LifecycleSinkKind::ArgumentOf` now stores `ArgumentIndex`; `build_call_sink` converts via `ArgumentIndex::try_from_usize`, deleting the manual bound check. Compiler/normalized/flow layers keep `usize`; `normalize.rs` and `explanation.rs` use `index.get()`.

#### [x] READ-005 — `LifecycleEvent::property_write` stores an untrimmed property, breaking the chunk's name-normalization convention

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle/types.rs:60-71`

`LifecycleEvent::property_write` checks `property.trim().is_empty()` but stores
the raw value (`kind: LifecycleEventKind::PropertyWrite { property, value }`,
types.rs:69). Every other authored-name path in the chunk normalizes before
storing: `ArgumentMatcher::object_property_value` stores
`property.trim().to_owned()` (`value.rs:253-267`), `checked_name` trims and
reuses the trimmed value (`declarations.rs:50-56`), and `MemberChain::parse`
canonicalizes through `SymbolPath`. A lifecycle event authored as
`property_write(" src ")` therefore carries a display string that no runtime
property write can ever equal, and the same logical event written with and
without padding compares unequal despite the chunk's canonical-equality
invariant on events.

**Recommendation:** Normalize in `property_write` by routing through the
existing `checked_name` helper (which trims and returns `SmolStr`) instead of
re-implementing the empty check inline, mirroring `object_property_value`.
Guardrail: keep rejecting empty/whitespace-only properties with
`QueryBuildError::EmptyIdentityName`; the value matcher is unrelated.

**Fix Applied:** `property_write` now routes the property through the existing `checked_name` helper, trimming and canonicalizing before storage; whitespace-only names still fail with `EmptyIdentityName`. Added a focused positive/negative test.

#### [x] READ-006 — Redundant crate-visible storage plus accessor on `LifecycleEvent` and `LifecycleSink`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle/types.rs:50-58, 374-382`

`LifecycleEvent` and `LifecycleSink` expose both a `pub(crate) kind` field and a
`pub(crate) fn kind()` accessor (types.rs:52 and 56; 376 and 380). Every sibling
type in the chunk encapsulates the same state: `LifecycleCondition`
(types.rs:137-139), `LifecycleCompletion` (types.rs:278-280), `ValueMatcher`
(value.rs:33-41) and `ArgumentMatcher` (value.rs:224-231) all keep the field
private and expose only `kind()`. The two crate-visible fields defeat the
accessor and let any crate-internal caller read storage directly, an
inconsistent internal API surface.

**Recommendation:** Make the `kind` field private on `LifecycleEvent` and
`LifecycleSink` and keep the `kind()` accessors (which `normalize.rs:429,450`
and `explanation.rs:262` already use). Guardrail: no behavior change; the
derived `PartialEq/Eq/Hash/Ord` semantics must remain identical.

**Fix Applied:** The `kind` field on both `LifecycleEvent` and `LifecycleSink` is now private; `kind()` remains the sole access path. No behavior change.

### [api/rule/query/lifecycle.rs]

#### [x] READ-004 — Duplicated builder scaffolding across the two lifecycle builders

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:118-137, 189-198, 211-215, 235-244`

`LifecycleBuilderState` (lifecycle.rs:118-137) is the shared state of both
`LifecycleQueryBuilder` and `CatalogLifecycleQueryBuilder`, and it already owns
`record_operation` (lifecycle.rs:132-136). Yet `CatalogLifecycleQueryBuilder`
re-implements the identical first-error recording
(`record_operation`/`record_error`, lifecycle.rs:207-215), and both builders
contain textually identical `build` bodies that destructure
`LifecycleBuilderState`, take the first error, and delegate to
`LifecycleStages::build` (lifecycle.rs:189-198 vs 235-244). The same
"record the first error from a `Result<(), E>`" idiom appears a third time as
the free function `record_first_error` in `api/rule/mod.rs:36-40`. The two
builders differ only in their fluent setter surfaces (immediate `try_*` vs
deferred `Into*`), not in this scaffolding.

**Recommendation:** Move `build` onto `LifecycleBuilderState`
(`fn build(self) -> Result<LifecycleQuery, QueryBuildError>` taking the state
apart exactly once) and delete the per-builder `build` bodies and
`CatalogLifecycleQueryBuilder::record_operation`/`record_error` in favor of the
state's existing method. Collapse the three identical first-error recorders
into the shared `record_first_error` (`api/rule/mod.rs:36`), with
`LifecycleBuilderState::record_operation` delegating to it, so the "record the
first error from a `Result<(), E>`" idiom exists exactly once. Keep the
immediate-vs-deferred setter split: it is the intended public distinction and
must not be collapsed.

**Fix Applied:** `build` now lives on `LifecycleBuilderState`, which destructures the state and takes the first error once; both per-builder `build` bodies delegate to it, and `LifecycleBuilderState::record_operation` delegates to the shared `record_first_error`. The immediate-vs-deferred setter split is unchanged.

#### [x] READ-008 — Lifecycle sources are exempt from the chunk's canonical-ordering invariant

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:26-31, 84-92`

`LifecycleStages.sources` is a plain `Vec<EventQuery>` accumulated in author
order, and `LifecycleStages::build` validates bounds only (lifecycle.rs:84-92);
the code comment even notes that collection invariants are established only for
`LifecycleEvents` and `LifecycleSinks` (lifecycle.rs:94-95). Events and sinks
are sorted/deduplicated into `CanonicalLifecycleItems` at construction
(types.rs:143-171), so `LifecycleQuery` equality and hash are order-independent
for conditions and completions but order-dependent for sources — yet the
compiler immediately re-sorts and de-duplicates sources during normalization
(`normalize.rs:352-367`), discarding author order. The chunk's canonicalization
invariant is thus applied inconsistently, and the compiler must redo work the
chunk could own. `CanonicalLifecycleItems<T>` is not reusable because it
requires `T: Ord` and `EventQuery` derives only `PartialEq/Eq/Hash`
(`query/mod.rs:148-149`).

**Recommendation:** Make source ordering canonical at the builder boundary:
derive `Ord` on `EventQuery` (every field already derives `Ord`) and sort+dedup
`sources` in `LifecycleStages::build` after the existing empty and size checks,
keeping the `Vec` storage so `sources()` is unchanged. `LifecycleQuery`
equality then becomes order-independent and the compiler's dedup/sort at
`normalize.rs:352-367` becomes a no-op safety net. Guardrail: keep the
`MAX_LIFECYCLE_SOURCES` check on the authored count (pre-dedup), the
compiler's `first-wins` duplicate semantics, and its deterministic evidence
order; do not change which source events may appear.

**Fix Applied:** `EventQuery` now derives `Ord`, and `LifecycleStages::build` sorts and dedups `sources` after the empty and size checks; `LifecycleQuery` equality is order-independent and the compiler's sort/dedup at `normalize.rs` is now a no-op safety net. Added a focused order-independence/dedup test.

### [api/rule/query/value.rs]

#### [x] READ-002 — Parallel bounded iterator-conversion helpers in value.rs and lifecycle/types.rs

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/value.rs:81-111` and `glass-lint-core/src/api/rule/query/lifecycle/types.rs:196-216`

`bounded_lifecycle_items` (types.rs:196-216) and `bounded_canonical_values`
(value.rs:81-111) are two implementations of the same operation — "bounded
fallible conversion of an `IntoIterator`, one item at a time, erroring past a
limit" — differing only in whether the result is then sorted/deduped/emptied.
`bounded_canonical_values` additionally contains a dead re-check: the loop
guards `parsed.len() >= MAX_STATIC_ALTERNATIVES` while pushing (value.rs:92-97),
so after sorting and deduplication (which can only shrink) the second check
`if parsed.len() > limits::MAX_STATIC_ALTERNATIVES` (value.rs:104-109) is
unreachable. The lifecycle flow already compensates by calling
`CanonicalLifecycleItems::new` after `bounded_lifecycle_items`, which is where
the empty/sort/dedup/limit checks happen (types.rs:145-161).

**Recommendation:** Consolidate the bounded conversion loop into one helper
(parameterized by the per-item converter and the limit) owned by `value.rs`,
and have the lifecycle constructors convert through it before
`CanonicalLifecycleItems::new`. Delete the dead post-deduplication length check
at value.rs:104-109. Guardrail: keep `EmptyLifecycleCondition` /
`EmptyLifecycleSinks` / `EmptyCollection` as distinct fail-closed errors and
keep the two-step lifecycle behavior (bound during conversion, canonicalize
after) equivalent.

**Fix Applied:** Already satisfied by chunk 18 read 005 (`fefa07e9`): `bounded_lifecycle_items` and `bounded_canonical_values` were deleted in favor of the single shared `CanonicalCollection::collect` (canonical.rs) used by `LifecycleEvents::new`, `LifecycleSinks::new`, `bounded_strings`, and `bounded_paths`, and the dead post-deduplication length check was removed. Verified: distinct `EmptyLifecycleCondition`/`EmptyLifecycleSinks`/`EmptyCollection` errors preserved and bound tests green.

### [api/rule/query/mod.rs] (chunk boundary — `LifecycleQuery`, `EventQuery`)

#### [ ] READ-003 — Nullable `Option<LifecycleCompletion>` on the built `LifecycleQuery` obscures a guaranteed invariant

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:293-302, 318-320`; compiler at `glass-lint-core/src/api/compiler/validate/pass4_10.rs:13-17, 29-33`

`LifecycleStages::build` guarantees `completion` is present
(`MissingLifecycleCompletion` otherwise, lifecycle.rs:105-107), yet the built
`LifecycleQuery` still exposes `completion: Option<LifecycleCompletion>` and
`completion() -> Option<&LifecycleCompletion>` (mod.rs:300, 318-320). Every
consumer must then treat a state that cannot occur: the compiler re-validates
"at least one source" and "at least a condition or completion"
(pass4_10.rs:13-17, 29-33) even though the builder already enforced both, and
`normalize.rs:393-394` must `.as_ref()` a value that is always `Some` in
production (the only `None` constructions are the `#[cfg(test)]`
`from_parts_for_test`, mod.rs:322-335). This nullable state obscures the
builder-guaranteed completion invariant and pushes re-validation onto a
different module.

**Recommendation:** Store `completion: LifecycleCompletion` (non-optional) on
the built `LifecycleQuery`, change `completion()` to return
`&LifecycleCompletion`, and update the touch points: `from_parts_for_test` and
its callers (`compiler/tests/normalize.rs:51-52`,
`validate/well_formedness.rs:290,324,363`) to supply one, the
`completion().as_ref()` at `normalize.rs:393-394` to drop `.as_ref()`, and the
`.is_some()` presence probes at `expression.rs:200` and `lifecycle/tests.rs:21`.
Keep `condition` optional — `AnySink`/`AllSinks` completions legitimately have
no condition — and drop the compiler's now-redundant none-checks at
pass4_10.rs:13-17 and 29-33. Guardrail: preserve the `Configuration`-requires-
condition rule in the builder and the fail-closed `MissingLifecycleCompletion`
error for the deferred catalog builder.

**Fix Applied:** None so far.

#### [ ] READ-007 — Duplicated `Vec<ArgumentConstraint>` + count-map pair synced by a free function

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:157-158` and `glass-lint-core/src/api/rule/query/lifecycle/types.rs:93-94`; sync logic at `glass-lint-core/src/api/rule/query/value.rs:301-326`

`EventQuery` (mod.rs:157-158) and `LifecycleEventBuilder` (types.rs:93-94) each
hold the same two-field state — `Vec<ArgumentConstraint>` plus a
`BTreeMap<ArgumentIndex, usize>` — whose consistency invariant (counts mirror
the vector; vector stays sorted by `(index, matcher)`; per-index and per-group
limits) is maintained only by callers passing both collections to the free
function `push_argument_constraint` (value.rs:301-326). `EventQuery` even
re-derives the map from the vector in its test constructor (mod.rs:198-201),
evidence that the pair is a single logical structure split across two fields.
This is the map/set/index-pair pattern AGENTS.md asks to encapsulate in a
domain collection.

**Recommendation:** Introduce a small domain collection in `value.rs` (the
chunk's owning module) that owns the sorted constraint vector and the count
index, exposing narrow methods (`push`, `iter`, `len`, limit errors) and hiding
the storage, then replace the pair in `EventQuery` and `LifecycleEventBuilder`.
Guardrail: the ordering and the `ExcessivePredicates` /
`ExcessiveArgumentGroups` / `InvalidArgumentIndex` errors must stay identical,
and the public `constraints() -> &[ArgumentConstraint]` surface
(mod.rs:174-176) should be preserved.

**Fix Applied:** None so far.

## Systemic Themes

- **Immediate vs deferred error builders.** `RuleBuilder`/`CatalogRuleBuilder`
  (`api/rule/mod.rs`) and `LifecycleQueryBuilder`/`CatalogLifecycleQueryBuilder`
  (lifecycle.rs) repeat the same two-mode builder design; the pattern itself is a
  deliberate, consistent choice and should be kept, but the first-error recording
  scaffolding around it is re-implemented three times (mod.rs:36,
  lifecycle.rs:132, lifecycle.rs:211) and should be centralized.
- **Canonical bounded collections.** `CanonicalLifecycleItems<T>` is the
  chunk's single mechanism for non-empty, sorted, deduplicated, bounded
  collections, but it is applied only to events and sinks; sources and static
  strings use ad-hoc normalization paths (normalize.rs:352-367, value.rs:73-111)
  that should eventually route through the same invariant.
- **`#[allow(unused_imports)]` re-export scaffolding.** `lifecycle.rs:8-20`
  carries `#[allow(unused_imports)]` on its `pub use`/`pub(crate) use` bridges;
  these exist to route crate-private types to the public API (`api/rule/mod.rs`)
  and to the compiler, and are needed — the attribute is noise rather than a bug.
- **Public accessor hygiene.** Value-query types keep `*Kind` enums private and
  expose `kind()` (value.rs, types.rs), a convention that `LifecycleEvent` and
  `LifecycleSink` violate (READ-006).

## Open Questions

- **`LifecycleSink::chain()` vs `LifecycleCallEndpoint`.** Resolved: the stored
  `MemberChain` is redundant. The endpoint stores both the parsed `MemberChain`
  and the derived `LifecycleCallTarget` (endpoint.rs:15-19); the compiler
  consumes only `target()` (normalize.rs:447-463, object_flow.rs) while
  `chain()` serves display and explanation (explanation.rs:261-269) and tests.
  The display chain round-trips exactly from the target: `Global(name)` stores
  `chain.as_str()` verbatim (types.rs:391) and `RootedMember(path)` holds the
  exact path whose `to_string()` produced the display (declarations.rs:32), so
  `chain()` can be derived on demand. Not reported as a finding because the
  redundancy is not yet a maintenance cost.
- **Source dedup semantics.** Resolved: author order of sources is cosmetic.
  Normalization sorts and deduplicates sources ("first wins",
  normalize.rs:352-367), and every downstream consumer — `planner.rs:113-124`,
  `object_flow.rs:158-162`, and the evidence helper `reference.rs:107` — reads
  the normalized (sorted) list, never the authored order. The compiler's
  sort/dedup is therefore the intended canonical form, and READ-008's framing
  stands.
- **Why `EventQuery` lacks `Ord`.** Resolved: not a deliberate avoidance. Every
  `EventQuery` field already derives `Ord` (`VarId`, `EventSpec` and
  `IdentitySpec`, both at event.rs:6-7 and event.rs:71-72,
  `Vec<ArgumentConstraint>`, `BTreeMap<ArgumentIndex, usize>`), the compiler
  already imposes a deterministic total order on sources at normalization
  (normalize.rs:360-366), and nothing derives semantic order from the type.
  Adding `Ord` is additive and safe; READ-008's recommended fix is unblocked.

## Coverage

- Files fully read: `api/rule/query/lifecycle.rs`, `api/rule/query/lifecycle/endpoint.rs`,
  `api/rule/query/lifecycle/types.rs`, `api/rule/query/lifecycle/tests.rs`,
  `api/rule/query/limits.rs`, `api/rule/query/value.rs`,
  `api/rule/query/value/tests.rs`, `api/rule/query/mod.rs`,
  `api/rule/query/declarations.rs`, `api/rule/query/event.rs`,
  `api/rule/query/composition.rs`, `api/rule/query/expression.rs`,
  `api/rule/mod.rs`, `api/compiler/validate/pass4_10.rs`.
- Traced: `api/rule/query/error.rs` (error variants), `api/rule/query/explanation.rs`
  (sink/event explanation consumers), `api/compiler/normalize.rs`
  (lifecycle lowering + source canonicalization), `api/compiler/normalized.rs`
  (normalized lifecycle IR), `api/compiler/object_flow.rs` (compiled object
  flow), `analysis/flow/planning.rs` + `analysis/flow/cross/*` (sink index
  consumers), and provider callers in `glass-lint-js` (e.g.
  `rules/browser/remote_resource/mod.rs`).
- Boundaries respected: no source, test, config, Cargo, or documentation files
  were modified; `git status --short` shows only this audit file as untracked.
