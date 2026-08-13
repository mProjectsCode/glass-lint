# Codebase Readability Audit — Chunk 04

## Summary

Chunk 04 owns the retained semantic model, module-interface model, value arena,
module-request recognition, and position-sensitive resolution cache. The
typestate boundaries, artifact-local IDs, bounded evidence storage, and
provider-neutral request policy are justified. The findings below focus on
duplicated state representations and avoidable copies or allocations at those
boundaries. They preserve fail-closed resolution, correlated provenance,
deterministic evidence, and the distinction between incomplete and complete
values.

## Findings

### Retained module-interface model

#### [x] READ-015 — Module requests store their vector identity twice

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Internal API Design
- **Location:** `glass-lint-core/src/analysis/model/module.rs:35-40,167-174,220-239,359-360,401-418`

`ModuleRequestId` is defined as the index into `ModuleInterface::requests`:
`add_request` assigns `ModuleRequestId(self.requests.len())`, pushes the
request, and `request` later indexes the vector with that ID. The request also
stores the same ID in its `id` field, and `ModuleRequest::id()` returns that
copy. The lookup path does not validate that the embedded ID agrees with the
vector position, so every insertion and any future reordering must preserve an
invariant that the owning collection already establishes.

**Recommendation:** Make the interface the sole owner of request identity:
remove the embedded field and derive the ID while iterating, or change the
request table to return an `(ModuleRequestId, &ModuleRequest)` view from one
owner-level operation. Keep the ID-based project/linker APIs and append-order
stability, while eliminating the possibility of two identities for one
request.

**Fix Applied:** Removed the embedded request ID from `ModuleRequest` and
added the owner-level `request_entries()` view, deriving IDs from stable vector
positions for linker, identity, resolver, and session consumers. Append order
and ID-based project APIs are unchanged. Verified with `make fmt && make ci`.

### Bounded flow evidence

#### [x] READ-017 — Readiness checks rebuild a boolean vector despite a stored bit mask

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:283-310,314-317,324-329,348-380,488-506`

`IndexedEvidence` already stores presence in a `u64` mask because requirement
and sink indexes are bounded to 64 entries. `LifecycleEvidence` nevertheless
passes the indexed entries back to `FlowReadiness::ready`, which allocates a
`Vec<bool>` for every readiness query, walks the entries, and then scans the
vector for `Any` or `All`. Readiness is queried from both local and cross-flow
state transitions, so the existing compact representation is not used for the
hot predicate it was introduced to support.

**Recommendation:** Keep the mask as the readiness representation and have
`IndexedEvidence` expose an owner-level readiness operation, or precompute the
required mask in `FlowReadiness` and compare masks directly. Retain the current
fail-closed result for an out-of-range index, the special `Configuration` and
`Any` sink modes, and the empty-set semantics for `All`.

**Fix Applied:** Moved readiness evaluation onto `IndexedEvidence`, comparing
its bounded presence mask directly and validating indexes against the declared
count. Configuration/Any sinks, All empty-set behavior, and fail-closed
out-of-range handling remain unchanged. Verified with `make fmt && make ci`.

### Position-sensitive resolution

#### [ ] READ-018 — Resolution-cache hits clone the complete provenance record

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:125-145`; `glass-lint-core/src/analysis/resolution/expression.rs:88-110,282-312`

Each cached `ResolvedValue` owns all resolution provenance, including several
optional symbol paths and an optional vector of bound arguments. A cache hit
clones that complete record in `start_resolution`; committing a new result
clones both the key and the complete value into the cache before returning the
original. The resolver has already added separate `resolve_ident_id`,
`resolve_member_id`, and `resolve_expr_id` paths specifically to avoid cloning
the full record, which makes the remaining cache-copy cost visible at the
central boundary.

**Recommendation:** Store cached results behind shared ownership such as an
`Arc<ResolvedValue>` and let full-provenance callers borrow or clone only at
their explicit ownership boundary; an `Arc` that is immediately deep-cloned
would not fix this finding. Keep the existing ID-only fast paths, recursive
cycle guards, and position-sensitive keys. Sharing must not make cached
provenance mutable or mix results from different source positions.

**Fix Applied:** None so far.

### Value-arena construction

#### [ ] READ-019 — Constant interning constructs a `Value` only to convert it back

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/resolution/constant.rs:60-99`; `glass-lint-core/src/analysis/model/value.rs:103-155,226-263`

`Resolver::intern_const_value` first maps a bounded `ConstValue` into a
temporary `Value`, then immediately matches that temporary to produce the
parallel `ValueConstruction` enum passed to `ValueTable::intern_construction`.
The conversion is not an arena lookup or a semantic validation step; the
temporary `Value` is never retained. `ValueConstruction` also repeats most of
the `Value` variants, so the intermediate path creates a synchronization point
where the two enums can drift.

**Recommendation:** Map `ConstValue` directly to `ValueConstruction`, retaining
the dedicated static-object branch that needs the name table, or centralize the
shared construction mapping behind the value-table owner. Preserve bounded
constant conversion, recursive child interning, optional binding identity, and
unknown/exhausted results.

**Fix Applied:** None so far.

## Systemic Themes

- Collection-owned identities should have one authoritative storage location;
  duplicated IDs make invariants implicit. `ExportObservation` remains a
  deliberate delta type: it describes one partial observation, while
  `ExportEntry` owns accumulated contradiction and unknown state, so those
  types are not a duplicate finding.
- The model already encodes bounded domains compactly. Readiness and cache APIs
  should consume those representations directly rather than reconstructing
  temporary vectors or cloning rich values.
- Resolution and value construction must remain provider-neutral and fail
  closed. Simplifying the conversions must not widen static values or discard
  position-sensitive provenance.

## Open Questions

- Prefer shared ownership for full provenance cache results while retaining
  the existing ID-only fast paths. Any owned projection should be created only
  at a caller that actually needs provenance; request IDs remain append-order
  identities derived by the interface owner and are not exposed as storage.

## Coverage

Reviewed the chunk-04 structure entries and their implementation/test support:

- `analysis/model/{fact,flow,module,scope,static_properties,value}.rs`
- `analysis/module_request.rs`
- `analysis/resolution/{mod,call,constant,expression,tests}.rs`
- Related callers in `analysis/facts`, `analysis/flow`, `analysis/project`, and
  `analysis/scope` were traced where required to validate ownership and copy
  boundaries.

No source, test, configuration, dependency, or other documentation files were
changed by this audit.
