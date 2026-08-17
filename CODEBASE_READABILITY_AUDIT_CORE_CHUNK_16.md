# Codebase Readability Audit — glass-lint-core Chunk 16: Occurrence and query indexes

## Summary

Chunk 16 owns occurrence storage, deterministic merges, and the query-facing
indexed access path inside `analysis::matching`:
`occurrence.rs` (`OccurrenceSelection`, `OrderedOccurrences`,
`BorrowedOccurrenceIter`/`MergeState`/`MergeItem`, `BorrowedPackageOccurrenceIter`
/`PackagePhase`/`PackageOverlay`, `PackageKeyPredicate`/`PackageMatchKind`,
`ModuleExportKey`/`InstanceMemberKey`/`ReturnedMemberKey`, and the
`Occurrences`/`NameOccurrences`/`ModuleOccurrences` aliases),
`occurrence/storage.rs` (`Occurrence`, `OccurrenceIndex`), `query/mod.rs`
(`IndexedRootIter`, the `OccurrenceIndexes` evidence/scan resolution impl), and
`query/view/{mod,private_network}` (`EventIndexView`, `private_network_match`).

Prior history: the earlier chunk-16 audit
(`CODEBASE_READABILITY_AUDIT_CHUNK_16.md`, git commits `ebba146b`..`b0db64af`
"fix chunk 16 read 001..006") already found and **applied** six findings — the
scattered `(event, span.start, span.end)` ordering key (now `Occurrence::sort_key`),
the hand-rolled `ScannedOccurrences` cursor (now `std::vec::IntoIter`), the
flag/nullable-`Option` package scan state (now `PackagePhase` + `PackageOverlay`),
the parallel `EventIndexCapabilities` layer (now `EventIndexView` owns its
`resolve_*` dispatch), the raw `"*"` sentinel (now `NAMESPACE_EXPORT`), and the
private-network boundary duplication/dead IPv6 arm (now `is_boundary` +
`Ipv4Addr`). None of that completed work is re-reported here.

The chunk is now generally well-owned: normalization is centralized on
`OccurrenceIndex::normalize` with a single sort key, selection stays lazy to the
evidence boundary, the k-way merge is deterministic and allocation-bounded, and
the package scan keeps base/linked buckets and masking policy separated. The
remaining readable problems are concentrated in the query executor's root
dispatching (a filter + `unreachable!` guard restating one invariant twice), the
physical placement of `private_network_match` (two re-export hops to reach a
sibling consumer), an uneven-accessor API on the `*Key` family and
`EventIndexView`, one dead parameter, and one stale intra-doc link. Findings are
ordered by how broad a change each implies.

## Findings

### Query root dispatch and view placement (`analysis/matching/query`)

#### [ ] READ-001 — `IndexedRootIter` filters by a set its consumer re-states in `unreachable!` match arms

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/query/mod.rs:18-43,56-98`

`IndexedRootIter::next` walks the plan and yields only the three
indexed-roots (`find` on `IndexedScan | ReturnedSubject | InstanceSubject`,
`query/mod.rs:34-41`), then its sole consumer `evidence_for_indexed_with_overlay`
matches the same three kinds and guards the other two with a runtime
`unreachable!("indexed root iterator yielded a non-indexed root")`
(`query/mod.rs:94-96`). The "which physical roots are occurrence-evaluated"
invariant therefore lives in two places that must stay in sync; if a future root
kind is added to the yield set and not to the match (or vice versa), the code
either panics or silently drops evidence. The `unreachable!` also sits on a
public execution path where core policy says not to panic on unexpected shapes.

**Recommendation:** Make the invariant structural instead of guarded: have the
iterator `filter_map` each `PhysicalRoot` into a small enum of the three indexed
kinds (`IndexedScan { .. } | ReturnedSubject { .. } | InstanceSubject { .. }`),
so the consuming match is exhaustive with no `unreachable!` arms — delete
`query/mod.rs:94-96` entirely. Guardrails: keep per-root order and the
deterministic evidence order (push in root order exactly as today), keep the
fail-closed behavior of an empty selection when a plan has no indexed roots
(empty evidence, not an error), and keep `push_owned_evidence` as the single
`into_ordered` boundary for all three branches.

#### [ ] READ-002 — `private_network_match` lives three levels down under `query/view` but is consumed by a sibling module via two re-export hops

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/matching/query/view/private_network.rs:6`; `query/view.rs:4-5`; `query/mod.rs:14-16`; `evidence.rs:191`

`private_network_match` is a provider-neutral string scanner defined in
`matching::query::view::private_network`, re-exported once in `view.rs:5`
(`pub(in crate::analysis::matching) use private_network::private_network_match;`)
and again in `query/mod.rs:16` (`pub(super) use view::private_network_match;`),
so that `matching::evidence::display_span` can reach it as
`super::query::private_network_match` (`evidence.rs:191`). Since the function's
visibility is already `pub(in crate::analysis::matching)`, the two re-export hops
exist only because the module is nested deeper than its consumers; `evidence` is
a sibling of `matching::query`, not a descendant. The strain shows two consumers
of the same primitive (`view.rs:281` selection and `evidence.rs:191` display
narrowing) whose shared owner is the leaf of a three-deep module tree.

**Recommendation:** Hoist the module to the matching layer (e.g.
`analysis/matching/private_network.rs`) so both consumers import it in one hop —
`use super::private_network_match` from `evidence.rs` and a single `use`/`mod`
from `query/view.rs` — and delete the two re-export statements (`view.rs:5`,
`query/mod.rs:16`). Guardrails: keep the `localhost → IPv4 → IPv6` precedence in
`private_network_match`, the token-boundary policy the `query/view/tests.rs`
cases pin (regex shapes return `None`, `query/view/tests.rs:3-15`; URL hosts
still match, `query/view/tests.rs:17-23`, including the `\\`/`?` rejections at
`private_network.rs:46,94-95`), the selection in `resolve_private_network`
(`view.rs:278-285`), and the `PRIVATE_NETWORK_EVIDENCE_SYMBOL` narrowing
contract in `display_span` (`evidence.rs:190-191`), which the integration test
`private_network_findings_cover_only_the_address`
(`tests/integration/matching/declarative/lifecycle.rs:129-143`) pins.

### Occurrence key family (`analysis/matching/occurrence`)

#### [ ] READ-003 — `InstanceMemberKey` hides its module/export components behind a nested `identity`, unlike its sibling keys

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:377-381,403-422`; `query/mod.rs:150-165`; `build.rs:229-236`; `indexes.rs:71`

The three lookup keys have converging but inconsistent surfaces.
`ModuleExportKey` exposes flat `module()`/`export()` accessors
(`occurrence.rs:432-438`), `ReturnedMemberKey` exposes flat `source()`/`member()`
(`occurrence.rs:394-400`), but `InstanceMemberKey` — a
`ModuleExportKey` + `member` composition (`occurrence.rs:377-381`) — exposes only
`identity()` and `member()`, so every consumer reaches through the nested key:
`key.identity().module() == expected_module` and `key.identity().export()`
(`query/mod.rs:155-156,160-161`), twice in the same closure. The constructor
also re-builds the inner key from raw strings (`build.rs:230-233` invokes
`InstanceMemberKey::new(module.clone(), export.clone(), SmolStr::new(name))`)
even though the caller already holds a `ClassIdentity`, while `ReturnedMemberKey`
is constructed from its two typed paths directly (`build.rs:217-220`). The
nested-`identity` navigation is pure ceremony: no code needs the whole
`ModuleExportKey` separately.

**Recommendation:** Give `InstanceMemberKey` the same flat accessor vocabulary as
its siblings — add `module()`/`export()` accessors delegating to the inner key,
and optionally a `From<ModuleExportKey>`-style constructor — and rewrite the
`occurrences_for_instance` closure to compare `key.module()`/`key.export()`
directly (`query/mod.rs:155-161`). Guardrails: keep the fields private with the
same `Ord`/`Eq`/`Hash` semantics (the keys are `BTreeMap` keys in
`MemberIndexes::instance_calls`), keep the `member` field a single identifier
`SmolStr` (distinct from `ReturnedMemberKey`'s path-typed member), and keep the
package pattern match arm using `module.matches(...)` against the module text.

### Event view accessors and executor surface

#### [x] READ-004 — `EventIndexView::member()` duplicates the match of `members()` with no extra meaning

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/query/view.rs:96-105,107-114,241,251`

`EventIndexView::member()` (`view.rs:107-114`) repeats the exact
`MemberCall | MemberRead | PropertyWrite` match that `members()`
(`view.rs:96-105`) already performs, returning only the first component. It is
used in exactly two places — `resolve_module_namespace` (`view.rs:241`) and
`resolve_package_namespace` (`view.rs:251`) — where only the first component of
the pair is needed, and no variant carries a member path without also carrying
the paths collection, so `member()` is always `members().map(|(m, _)| m)`.

**Recommendation:** Delete `member()` and replace its two call sites with
`self.members()?.0`. Guardrails: keep the `name`-free behavior of namespace
resolution fail-closed (`?` on `members()` still returns `None` for the
`Call`/`Construct`/`ClassReference`/`Import`/`StringReference` variants exactly
as today), and leave `resolve_any`'s `_`-fallback (`view.rs:195`) and the
`_ => None` arms in `resolve_literal`/`resolve_private_network`/
`resolve_package_specifier` (`view.rs:274,283,295`) untouched — they are the
deliberate fail-closed answers for unsupported identity/event pairs.

**Fix Applied:** Removed the duplicate `member()` match and changed both
namespace-resolution callers to use `members()?.0`.

#### [x] READ-005 — `occurrences_for_instance` carries a dead `names` parameter that hides behind a `_` prefix

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/query/mod.rs:141-166,89`

`occurrences_for_instance` accepts `_names: &NameTable` (`query/mod.rs:146`) that
is never used — instance identity resolution compares module/export text and the
`SymbolPath::eq_chain` member directly (the closure at `query/mod.rs:150-165`
uses neither the table nor any `NameId`). The parameter exists only to mirror the
signature of `occurrences_for_returned` (which genuinely needs `names` for
`lookup_path`), and the `_` prefix suppresses the dead-code signal that would
otherwise flag it. The caller threads `names` through for this branch
(`query/mod.rs:89`).

**Recommendation:** Drop the parameter from `occurrences_for_instance` and stop
passing `names` at `query/mod.rs:89`, keeping the `names` argument only where it
is actually used (`occurrences_for_indexed`, `occurrences_for_returned`).
Guardrails: the predicate closure and its fail-closed `None` for non-module
`constructor` identities (`_ => false` at `query/mod.rs:164`) are unchanged, and
if a future instance identity needs `NameTable`-based resolution, add the
parameter back explicitly rather than re-introducing an unused one.

**Fix Applied:** Removed the unused `NameTable` parameter from
`occurrences_for_instance` and stopped threading it through the instance-match
caller. Returned-object lookup retains its name-table parameter.

#### [x] READ-006 — `PackageKeyPredicate`'s doc references the deleted `PackageOccurrenceIter` name

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Documentation
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:215-220`

The doc on `PackageKeyPredicate` says "a concrete type so the lazy
[`PackageOccurrenceIter`] can call it without boxing a closure"
(`occurrence.rs:219`). The type it links was renamed to
`BorrowedPackageOccurrenceIter` in the earlier package-scan refactor (the actual
type now appears at `occurrence.rs:254` and is returned by
`OccurrenceIndex::package_candidates`, `storage.rs:112-117`); the intra-doc link
is broken and the wording refers to a symbol that no longer exists.

**Recommendation:** Update the link to
`[`BorrowedPackageOccurrenceIter`]` and reword to name the actual consumer
(the scan in `BorrowedPackageOccurrenceIter::next`, `occurrence.rs:323-364`).
Guardrails: none beyond the doc text; no behavior changes.

**Fix Applied:** Updated the intra-doc link and wording to the existing
`BorrowedPackageOccurrenceIter` scan.

## Systemic Themes

- **Root-kind membership is a repeated two-owner decision.** Which physical
  roots participate in occurrence-index evaluation is asserted both by the
  `IndexedRootIter` filter (`query/mod.rs:34-41`) and by the consuming match's
  `unreachable!` guard (`query/mod.rs:94-96`); the projection planner also
  matches `physical_roots()` for a disjoint purpose
  (`project/projection.rs:216-227`), where a new kind is absorbed by the `_`
  arm. A new root kind therefore needs explicit handling in the indexed-query
  pair (see READ-001) plus a deliberate decision in the planner's own dispatch.
- **Movement of helpers across module trees is done by re-export ceremony**
  rather than placement. `private_network_match` is the clearest instance
  (READ-002); the `EventIndexView`/`IndexedRootIter`/`OccurrenceSelection`
  visibility is otherwise tightly scoped to `crate::analysis::matching`, which
  is good.
- **The `OccurrenceIndex` key-alias family is uneven.** `Occurrences`
  (`SmolStr`), `NameOccurrences` (`NameId`), and `ModuleOccurrences`
  (`ModuleExportKey`) get aliases (`occurrence.rs:367-368,445`), while the most
  frequently written instantiation, `OccurrenceIndex<NamePath>`, is spelled out
  raw in `MemberIndexes`' and `ConstructionIndexes`' field and accessor
  signatures (`indexes.rs:62-67,115,119,127,131,135,189,251`) and in
  `EventIndexView`'s field and resolve-helper signatures
  (`query/view.rs:29,31,36,38,43,54,96,146`). This is cosmetic — no invariant is
  enforced or leaked — so it is not filed; the owner of the alias family should
  decide once whether `NamePath`-keyed and `InstanceMemberKey`/
  `ReturnedMemberKey`-keyed indexes deserve the same treatment (see Open
  Questions — Resolved).
- **`EventIndexView`'s `_ => None` fallbacks are the fail-closed contract.**
  The `_` arms in `resolve_any`/`resolve_literal`/`resolve_private_network`/
  `resolve_package_specifier` (`view.rs:195,274,283,295`) silently answer `None`
  for variants that do not support an identity; this must stay when the enum
  gains variants, and any new variant's supported identity paths should be added
  explicitly so the fallback never becomes a silent trap.
- **Prior applied work is respected.** The six findings of the earlier
  `CODEBASE_READABILITY_AUDIT_CHUNK_16.md` are confirmed fixed in the current
  tree (`Occurrence::sort_key` at `storage.rs:35-37`, `PackagePhase`/
  `PackageOverlay`, `EventIndexView` owning `resolve`, `NAMESPACE_EXPORT` in
  `ModuleExportKey::wildcard`, `is_boundary` + `Ipv4Addr` direct parse) and are
  not re-reported.

## Open Questions — Resolved

1. **`InstanceMemberKey`'s nested `identity()` is not load-bearing.** No caller
   needs the composed `ModuleExportKey`: the key is constructed only at
   `build.rs:229-236`, stored in `MemberIndexes::instance_calls`
   (`indexes.rs:71`), and read only by the `occurrences_for_instance` closure,
   which extracts module/export text (`query/mod.rs:155-156,160-161`) and the
   member (`query/mod.rs:157,162`). Overlay and identity-map lookups never see
   an `InstanceMemberKey` — `resolve_module_key` builds a fresh `ModuleExportKey`
   from the query identity (`view.rs:222`) and `LinkedOccurrenceView::identity_for`
   operates on module-export keys only (`matching/mod.rs:92-99`). Flat
   `module()`/`export()` accessors (READ-003) are therefore safe.
2. **An alias for `OccurrenceIndex<NamePath>` is the minimal consistent
   completion; the decision stays with the alias-family owner.** The raw
   spelling outnumbers the aliased keys: `OccurrenceIndex<NamePath>` appears in
   20 places (`indexes.rs:62-67,115,119,127,131,135,189,251`;
   `view.rs:29,31,36,38,43,54,96,146`) against one alias each for the
   `SmolStr`/`NameId`/`ModuleExportKey` keys
   (`occurrence.rs:367-368,445`), while `InstanceMemberKey` and
   `ReturnedMemberKey` are each instantiated once (`indexes.rs:69-71`). Adding
   `PathOccurrences = OccurrenceIndex<NamePath>` next to the existing aliases
   completes the family without changing any invariant; removing the existing
   aliases instead would churn every call site for no readability gain. Purely
   cosmetic, so it is noted here rather than filed.
3. **The `Construct` global fallback is deliberate per-event policy, not a
   provable inconsistency.** Both events record the same dual name indexes —
   calls intern the callee name (`build.rs:181-183`) and the global provenance
   name (`build.rs:184-188`); constructors do the same (`build.rs:295-298`,
   `build.rs:303-308`). The asymmetry exists only in the query-side lookup: the
   `Construct` arm adds a raw-text fallback to `global_constructors` after the
   named-constructor index (`view.rs:183-191`), while the `Call` arm stops at
   the interned-name lookup (`view.rs:179-182`). Because every recorded name
   originates from file text in the same `NameTable`, the fallback's reachable
   window is at most a raw-text hit that the interned lookup missed, which
   cannot change a witness's certainty; it is policy, not a demonstrable
   inconsistency, and is unchanged here.

## Coverage

Files reviewed (read-only; no source changes):

- `glass-lint-core/src/analysis/matching/occurrence.rs`, `occurrence/storage.rs`, `occurrence/tests.rs`
- `glass-lint-core/src/analysis/matching/query/mod.rs`, `query/view.rs`, `query/view/private_network.rs`, `query/view/tests.rs`
- Context (not re-audited here): `analysis/matching/mod.rs`, `indexes.rs`, `build.rs`, `evidence.rs`, `analysis/matching/arguments/mod.rs`, `analysis/project/projection.rs`, `api/compiler/physical.rs`, `api/compiler/mod.rs`, `api/rule/module.rs`, `api/rule/query/event.rs`

Callers traced: `evidence_for_indexed_with_overlay` →
`IndexedRootIter::from_plan` (`project/projection.rs:332-336`), `push_owned_evidence`
→ `EvidenceGroup::definite_classification` → `normalize_evidence`
(`matching/mod.rs:354-365`, `evidence.rs:44-51,221-230`), the build-path
constructors (`build.rs:177-318`), and `display_span`'s private-network narrowing
(`evidence.rs:190-192`).

Verification performed: traced the merge pipeline
(`BorrowedOccurrenceIter`/`MergeState`/`MergeItem` cursor and heap paths, the
`BorrowedPackageOccurrenceIter` base/overlay two-phase scan and masking, and
`into_ordered` delegation at the evidence boundary, `occurrence.rs:26-91,138-158`);
confirmed `Occurrence` `sort_key` is the single ordering key reused by
`normalize`, `OrderedOccurrences::sorted`, and `MergeItem::cmp`
(`storage.rs:35-37,94-99`; `occurrence.rs:60-66,126-129`); confirmed
selection laziness (`Regionless`-free, fail-closed `None` in
`OccurrenceIndex::matching`, `storage.rs:68-79`); confirmed no `PackageOccurrenceIter`,
`ScannedOccurrences`, or `EventIndexCapabilities` symbols remain in code;
confirmed the `unreachable!` at `query/mod.rs:95`; confirmed `members()`/`member()`
duplicate matches and dead `_names`; confirmed the double re-export chain for
`private_network_match`; and confirmed `git status --short` shows no uncommitted
source changes — the audit file itself is committed, and the only working-tree
changes are the sibling chunk-01..15 audit files from parallel sessions.
