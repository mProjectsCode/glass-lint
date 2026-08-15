# Codebase Readability Audit — glass-lint-core Chunk 16: Occurrence and query indexes

## Summary

Chunk 16 owns the occurrence storage and query-facing index access inside
`analysis::matching`: typed occurrence storage and deterministic normalization
(`occurrence`, `occurrence/storage`), occurrence selection and merging
(`OccurrenceSelection`, `OrderedOccurrences`, `BorrowedOccurrenceIter`,
`BorrowedPackageOccurrenceIter`), package-key predicates and overlay access
(`PackageKeyPredicate`, `PackageOverlay`), indexed-query resolution
(`query/mod.rs`, `IndexedRootIter`, the `OccurrenceIndexes` query impl), and the
event-index view layer (`query/view`, `query/view/private_network`).

The chunk is coherent: normalization is centralized in `OccurrenceIndex`,
selection is lazy and fail-closed, the merged-iterator fast paths avoid
allocation, and the private-network scanner is well tested and guarded against
regex false positives. The concrete readability problems are: one
deterministic ordering key `(event, span.start, span.end)` is rebuilt at three
independent sites and then re-established by a *different* key in evidence
normalization; a few small hand-rolled mechanics reimplement standard or
domain-owned operations (`ScannedOccurrences` reimplements
`std::vec::IntoIter`, the package iterator encodes its two-phase traversal with
flags and nullable options, and the private-network scanners each hand-roll a
variant of the same character-boundary check); and one duplicated sentinel
(`"*"`) plus a two-layer enum pair in the view module model the same concept
twice.

Findings are ordered with the broadest, most centralizable issue first
(ordering key), then the mechanical reimplementations, then the parallel-model
and sentinel duplication. No source was modified; findings are read-only
evidence.

## Findings

### Occurrence storage, ordering, and merging (`analysis/matching/occurrence`)

#### [x] READ-001 — The `(event, span.start, span.end)` ordering key is rebuilt at three sites and then discarded by a different evidence sort

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:70-82,149-164`; `glass-lint-core/src/analysis/matching/occurrence/storage.rs:88-105`; `glass-lint-core/src/analysis/matching/evidence.rs:85-94`

The sort key `(event, span.start, span.end)` is constructed independently in
`OrderedOccurrences::sorted` (`occurrence.rs:73-79`), `MergeItem::cmp`
(`occurrence.rs:151-156`, plus the bucket tie-break), and
`OccurrenceIndex::normalize` (`storage.rs:90-96`). Each site is a call site that
must stay byte-identical with the others: normalize and merge both rely on the
key for determinism, and `OrderedOccurrences::sorted` also uses it. Moreover,
the order produced at the evidence boundary is not the final order: every
production evidence path calls `normalize_evidence`
(`analysis/project/projection.rs:482`), which re-sorts each group by
`(span.start, span.end, fact)` (`evidence.rs:85-91`), a different primary key
than the occurrence boundary's `(event, ...)`. The contract is thus split
across two layers with two different key orders, and the occurrence-level sort
is recomputation whose result is replaced downstream.

**Recommendation:** Give `Occurrence` one canonical ordering owner — a
`sort_key()` method or a derived `Ord` on `Occurrence` — and use it in
`normalize`, `OrderedOccurrences::sorted`, and the merge comparator (keeping the
bucket index as an extra, separate tie-break in `MergeItem` only). Guardrails:
`sort_unstable_by_key` and `dedup_by_key` in `normalize` must keep the identical
key so deduplication never diverges from sorting; the bucket tie-break in
`MergeItem::cmp` must stay distinct from the event ordering; and the boundary
sort cannot simply be removed even though `normalize_evidence` re-sorts in
production, because the occurrence unit test
`ordered_selection_sorts_without_deduplicating_physical_events`
(`occurrence/tests.rs:117-136`) asserts the `(event, span.start, span.end)`
order directly (the `evidence_for` integration test at `matching/tests.rs:49`
exercises only the lazy Indexed path and is indifferent to it).

**Fix Applied:** Added `Occurrence::sort_key()` as the single canonical ordering owner and used it in `OccurrenceIndex::normalize` (both `sort_unstable_by_key` and `dedup_by_key`), `OrderedOccurrences::sorted`, and `MergeItem::cmp` (which keeps only the bucket tie-break); `MergeItem` no longer stores redundant `event`/`start`/`end` copies and `occurrence.rs` dropped its unused `FactId` import. The evidence-boundary sort in `normalize_evidence` is unchanged.

#### [ ] READ-002 — `ScannedOccurrences` reimplements `std::vec::IntoIter` by hand

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:35-38,45-50,59,96-111`

`ScannedOccurrences { values: Vec<Occurrence>, next: usize }`
(`occurrence.rs:35-38`) is a manual `Vec`-plus-index cursor that reimplements
`std::vec::IntoIter<Occurrence>`: `OccurrenceSelection::scanned` wraps a `Vec`
(`occurrence.rs:45-50`), `Iterator::next` hand-increments `next`
(`occurrence.rs:104-109`), and `into_ordered` moves `scanned.values` into the
sorted path (`occurrence.rs:59`). The same module already uses
`std::vec::IntoIter` directly for `OrderedOccurrences::Sorted`
(`occurrence.rs:67`), so the two variants of "owned Vec iterator" are modeled
inconsistently. The struct adds no invariant and no vocabulary over the std
type; it only adds a hand-rolled `next`.

**Recommendation:** Replace the `Scanned(ScannedOccurrences)` variant's payload
with `std::vec::IntoIter<Occurrence>` and delete the struct and its manual
`next`. `into_ordered` still routes the Scanned selection through
`OrderedOccurrences::sorted`, so the sorting behavior is unchanged. Guardrail:
the selection must keep yielding the exact occurrences (including duplicates)
recorded by `OccurrenceIndex::matching` (`storage.rs:62-73`), and the
`matching`-built selections must remain sorted at the evidence boundary exactly
as today.

**Fix Applied:** None so far.

#### [ ] READ-003 — `BorrowedPackageOccurrenceIter` encodes its two-phase traversal with a flag plus two nullable fields

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:287-296,333-349,351-388`

`BorrowedPackageOccurrenceIter` carries `masked: Option<&BTreeSet<...>>`,
`overlay_iter: Option<btree_map::Iter<...>>`, and `checking_base: bool`
(`occurrence.rs:290-295`) to encode a two-phase scan (base buckets, then overlay
buckets). `overlay_iter` being `None` means both "no overlay was configured"
(`occurrence.rs:339`) and "overlay exhausted" (`occurrence.rs:380`), so the
state is ambiguous on inspection; the phase transition is a bare
`self.checking_base = false` flip (`occurrence.rs:373`). The masking policy is
also conditioned on the same Option (`occurrence.rs:366`), so three fields
jointly encode what is really a small state machine plus an optional overlay.

**Recommendation:** Replace the flag/`Option` pair with an explicit phase type
(e.g. `enum Phase { Base, Overlay(btree_map::Iter<...>), Done }`) and hold the
masking set inside an `Option<&Overlay>` that exists only when an overlay is
present. Guardrail: preserve the exact masking semantics — masked base keys are
skipped while overlay buckets still contribute (`occurrence.rs:363-384`) — and
keep the deterministic base-then-overlay iteration order and the fail-closed
`None` when neither base nor overlay yields a match.

**Fix Applied:** None so far.

### Query view and private-network scan (`analysis/matching/query`)

#### [ ] READ-004 — `EventIndexCapabilities` is a parallel, 1:1 representation of `EventIndexView`

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/query/view.rs:21-63,65-98,133-213,216-234`

`EventIndexView` (eight variants, `view.rs:21-63`) is built by
`OccurrenceIndexes::build_event_view` (`query/mod.rs:195-240`) and immediately
converted by the mechanical `capabilities()` mapping
(`view.rs:133-213`) into a second enum-struct layer — `EventIndexCapabilities`
with the `AnyIndex`, `LiteralIndex`, `ModuleIndex`, and `RootedIndex` helpers
(`view.rs:65-98`). Every reference is copied from the view variant into the
capabilities (e.g. `MemberCall`'s `paths/module/rooted/environment` become
`AnyIndex::Members` plus `ModuleIndex`/`RootedIndex` at `view.rs:146-158`), so
the two layers are parallel model types joined by a conversion that adds no
invariant: the same references already named as fields of `EventIndexView` are
destructured from `(ModuleOverlayKind, &ModuleOccurrences)` /
`(&OccurrenceIndex<NamePath>, &Environment)` tuples in
`EventIndexCapabilities::indexed` (`view.rs:216-234`). A prior chunk audit
(Chunk 15) noted this tuple destructuring in its systemic themes and deferred
it on the ground that the capabilities layer owns the resolve dispatch; this
finding argues the whole two-enum structure is one redundant conversion path.

**Recommendation:** Pick one owner. Either move the `resolve_*` methods
(`view.rs:247-397`) onto `EventIndexView` — adding small per-variant accessors
for the optional `member`/`global`/`rooted` slots — and delete
`EventIndexCapabilities`, `AnyIndex`, `LiteralIndex`, `ModuleIndex`, and
`RootedIndex`, or drop `EventIndexView` and have `build_event_view` construct
the capabilities directly. Guardrails: keep the per-event "only meaningful
indexes" restriction (do not give every event every index), preserve the
overlay-consult order (`resolve_global`/`resolve_module_key`,
`view.rs:273-283,367-382`), and keep the identity dispatch for all nine
`IdentityConstraint` variants exactly as it is today.

**Fix Applied:** None so far.

#### [ ] READ-005 — The `"*"` namespace-export sentinel is duplicated as a raw literal while `NAMESPACE_EXPORT` already exists

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:463-465`; `glass-lint-core/src/analysis/model/module.rs:14`; `glass-lint-core/src/analysis/project/linker/export.rs:290`; `glass-lint-core/src/analysis/resolution/call.rs:64`; `glass-lint-core/src/analysis/project/identities.rs:235`

`ModuleExportKey::wildcard` builds its key with the raw literal `Self::new(module, "*")`
(`occurrence.rs:463-465`), and the same `"*"` namespace marker appears as a raw
literal in `resolution/call.rs:64` (`export: "*".into()`) and
`project/identities.rs:235`, while `analysis/model/module.rs:14` already
declares `pub const NAMESPACE_EXPORT: &str = "*"` and
`project/linker/export.rs:290` uses that constant for the identical concept
(namespace/star export marker). Callers of `ModuleExportKey::wildcard`
(`matching/mod.rs:216-227`, `project/identities.rs:155,176,183`) rely on the
`"*"` export value being the magic "any member" marker, so the sentinel's
meaning is split across a named constant and three unlabeled literals that can
silently drift.

**Recommendation:** Consolidate the marker on one named owner — reference
`crate::analysis::model::module::NAMESPACE_EXPORT` from `ModuleExportKey::wildcard`
and `resolution/call.rs`, or, if the query layer must not depend on the
module-interface model, declare a matching-owned named constant and use it at
all three sites. Guardrails: `"*"` must remain distinguishable from every real
export name (real exports may not be `"*"`), the wildcard lookup semantics in
`identity_for` (`matching/mod.rs:212-227`) must not change, and `DEFAULT_EXPORT`
and the other model constants stay where they are.

**Fix Applied:** None so far.

#### [ ] READ-006 — Three private-network scanners each hand-roll a variant of the same token-boundary check, and the IPv4 parser match has a dead IPv6 arm

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/query/view/private_network.rs:12-22,24-58,75-104`

`contains_localhost` (`private_network.rs:18-19`), `contains_private_ipv4`
(`private_network.rs:39-44`), and `private_ipv6_token` (`private_network.rs:92-98`)
each implement a "character before/after the candidate is a boundary" check
with subtly different rule sets (alphanumeric + `.`; alphanumeric + `.` + `\\`
on the left only; and `?`/`\\` only). The three scanners are the same
"scan string, find an address-like token, validate, return `(start, end)`"
sequence repeated three times with hand-rolled index arithmetic. In addition,
`contains_private_ipv4` validates with `IpAddr::from_str(candidate)` and matches
both `IpAddr::V4` and `IpAddr::V6` (`private_network.rs:48-51`), but the
candidate is a run of digits and dots with exactly three dots, which can only
parse as an IPv4 address, so the `V6` arm is unreachable.

**Recommendation:** Parse the dotted IPv4 candidate directly as `Ipv4Addr` so
`contains_private_ipv4` has no dead `V6` arm, and extract only the genuinely
shared piece — the alphanumeric/dot boundary predicate used identically by
`contains_localhost` on both sides and as the IPv4 after-check — into one
small helper instead of forcing the three scans into a single scanner.
Guardrails: the boundary rule sets differ per scan (the IPv6 tokenizer already
delimits on whitespace/punctuation and only rejects `?`/`\\`, and the IPv4
before-check additionally excludes `\\`), so the three predicates must not be
merged; the `localhost` → IPv4 → IPv6 precedence in `private_network_match`
(`private_network.rs:6-10`) and the regex non-match guarantees asserted in
`query/view/tests.rs:3-15` must remain.

**Fix Applied:** None so far.

## Systemic Themes

- **The deterministic occurrence ordering contract is scattered.** The key
  `(event, span.start, span.end)` is rebuilt by `normalize`, the merge
  comparator, and the evidence-boundary sort, and then replaced by a different
  key (`span.start, span.end, fact`) in `normalize_evidence`; READ-001
  centralizes this on `Occurrence`.
- **Hand-rolled mechanics replace standard or domain-owned operations.**
  `ScannedOccurrences` reimplements `std::vec::IntoIter` (READ-002), the
  package iterator drives its two-phase scan with a flag plus two nullable
  fields (READ-003), and the private-network module hand-rolls three variants
  of one boundary check (READ-006).
- **Parallel representations of one concept.** `EventIndexView` /
  `EventIndexCapabilities` model the same per-event index bundle twice with a
  1:1 conversion (READ-004), and the `"*"` namespace marker exists both as
  `NAMESPACE_EXPORT` and as raw literals at three sites (READ-005).
- **Chunk overlap.** `occurrence.rs`, `occurrence/storage.rs`, `query/mod.rs`,
  `query/view.rs`, and `query/view/private_network.rs` are also within the
  Chunk 15 audit scope. Chunk 15 already numbered the "Phase 7" comment
  (`query/mod.rs:189-193`, READ-005), the `OccurrenceIndexes` test facade and
  `query::record` (READ-004), and the `push_owned_evidence` /
  `push_owned_rule_evidence` helpers (READ-007); those are not re-reported
  here.

## Open Questions

- **Resolved.** The asymmetry is real, and a call recorded only as global
  provenance does evade an `Any` identity — but that is consistent with the
  authoring surface. `Any` is lowered only from `IdentitySpec::Heuristic`
  (`api/compiler/mod.rs:130`), while global calls are matched through the
  dedicated `Global` identity (`call_global` → `resolve_global` →
  `global_calls`, e.g. `glass-lint-js/src/rules/browser/request/mod.rs:15`).
  A call whose callee is not a plain identifier has no `callee_name`
  (`facts/calls/callee.rs:69` sets it only for `Expr::Ident`) yet can still
  carry `Global` provenance (`resolution/call.rs:155-177`), so e.g. `fetch?.()`
  lands only in `global_calls` and is invisible to `Any`. Whether the
  `Constructors` global fallback (`view.rs:260-268`) is the intentional
  outlier or an inconsistency is a policy decision, not determinable from code.
- **Resolved.** Yes — the sort is load-bearing for the occurrence unit tests:
  `ordered_selection_sorts_without_deduplicating_physical_events`
  (`occurrence/tests.rs:117-136`) asserts the `(event, span.start, span.end)`
  order directly, and `ordered_normalized_selections_keep_their_lazy_order`
  (`occurrence/tests.rs:138-149`) pins the Indexed/Borrowed lazy order. The
  `evidence_for` integration test (`matching/tests.rs:49`) exercises only the
  lazy Indexed path and is indifferent to the sort. Production output is
  insensitive to it because `normalize_evidence` re-sorts and dedups each
  group deterministically; the boundary sort exists for the occurrence-level
  ordering contract, not for final report order.
- **Resolved.** Cross-layer reuse of the existing `NAMESPACE_EXPORT` is the
  cleaner owner: it already sits in the model constant group
  (`model/module.rs:14`), is already used by the project layer for the
  identical concept (`project/linker/export.rs:290`), and the matching and
  resolution layers already depend on `analysis::model` (`matching/build.rs:89`,
  `resolution/call.rs:10`), so referencing it introduces no new coupling. A
  matching-owned duplicate would split the concept's ownership (see READ-005).

## Coverage

Files reviewed (read-only; no source changes):

- `glass-lint-core/src/analysis/matching/occurrence.rs`, `occurrence/storage.rs`, `occurrence/tests.rs`
- `glass-lint-core/src/analysis/matching/query/mod.rs`, `query/view.rs`, `query/view/private_network.rs`, `query/view/tests.rs`
- `glass-lint-core/src/analysis/matching/mod.rs`, `indexes.rs`, `build.rs`, `evidence.rs` (context for the occurrence/query contracts)
- Callers traced: `analysis/matching/arguments/mod.rs`, `analysis/project/projection.rs`, `analysis/project/identities.rs`, `analysis/project/state.rs`, `analysis/facts/mod.rs`, `analysis/model/module.rs`, `analysis/project/linker/export.rs`, `analysis/resolution/call.rs`, `glass-lint-datastructures/src/path/name_path.rs`, `glass-lint-datastructures/src/name.rs`, `glass-lint-core/src/environment.rs`

Verification performed: traced `OccurrenceSelection`/`OrderedOccurrences`
construction and the `into_ordered` → `EvidenceGroup` → `normalize_evidence`
path; confirmed the `(event, span.start, span.end)` key at three sites and the
re-sort in `evidence.rs:85-91`; confirmed `ScannedOccurrences` is used only as a
hand-rolled iterator; confirmed the `"*"` literal at three sites against
`NAMESPACE_EXPORT`; confirmed the private-network boundary rules differ per
scan; and confirmed `git status --short` shows only this audit file as new.
