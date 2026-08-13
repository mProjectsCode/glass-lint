# Codebase Readability Audit

## Summary

Chunk 1 is the single-source fact-construction boundary: one scope-prepared
AST traversal populates the bounded fact stream, provenance state, call-result
state, and module interface before the artifact is frozen. The phase-typed
stream and scoped traversal helpers are sound boundaries, and the historical
Chunk 1 findings were checked before reviewing current code. Two current
opportunities remain: one provenance mutation bypasses its semantic owner,
and one module-call result wrapper adds no domain contract.

## Findings

### Provenance state and fact-builder collaborators

#### [ ] READ-044 — Route class-origin writes through `FactProvenanceState`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:77-227`; `glass-lint-core/src/analysis/facts/functions.rs:182-196`

`FactProvenanceState` already owns semantic instance-origin recording through
`record_instance_origin`, class-origin lookup through `class_origin`, and the
correlated replacement/branch operations. `record_class_decl` nevertheless
reaches through `self.provenance.origins.classes` and calls the raw
`OriginMap::insert`, leaving one class-origin mutation outside the owner that
controls provenance channels and budgeted transitions. This is especially
easy to regress because the same value can also be updated through
`replace_targets`; callers must currently know which path is safe.

**Recommendation:** Add a `FactProvenanceState::record_class_origin` operation
and change `record_class_decl` to use it, keeping `OriginChannels` and its
`OriginMap`s private to the provenance owner. Preserve the separate class and
instance channels, budget charging, branch checkpoint semantics, and the
fail-closed treatment of unknown or exhausted provenance. Historical READ-001
was marked applied, but the current raw write was reintroduced by the later
`OriginChannels` restructuring and should be treated as a regression.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. The recommendation targets the
single raw class-origin mutation and does not require exposing `OriginChannels`;
the provenance owner remains responsible for budget and branch semantics.

### Module-call observation boundary

#### [ ] READ-045 — Remove the non-semantic `ModuleCallObservation` wrapper

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:83-91, 531-536`; `glass-lint-core/src/analysis/facts/calls/mod.rs:15-35, 76-83`

`ModuleCallObservation` contains only a `String`, has no invariant or behavior
beyond `into_module`, and is created by `observe_module_call` only to be
immediately consumed by the call visitor. The underlying
`ModuleInterfaceBuilder::record_module_request` already returns
`Option<String>`, so the wrapper adds a conversion layer without protecting a
phase, distinguishing request kinds, or carrying source context.

**Recommendation:** Return the existing `Option<String>` directly from
`FactBuilder::observe_module_call` and remove `ModuleCallObservation` and
`into_module`. Keep `None` for wrapped `require`, retain the interface request
side effect and the dynamic-import/require distinction, and preserve the
current deterministic import-fact emission order for each call shape.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. Returning the existing optional
module name removes representation-only plumbing while retaining the request
side effect and call-shape distinction.

## Systemic Themes

- The fact builder has the right aggregate ownership boundary, but child
  visitor modules can still bypass it when a backing field is visible within
  the parent module. Semantic provenance operations should be the only route
  for mutations that participate in correlated branch state.
- Small internal types should earn their cost through an invariant, changed
  vocabulary, or lifecycle transition. `FactStream` and
  `ModuleInterfaceBuilder` do provide such boundaries; `ModuleCallObservation`
  does not.
- The existing one-pass design, bounded snapshots, and `Building -> Frozen`
  transition should remain intact while narrowing these APIs.

## Open Questions

- None blocking the two findings. The historical lifecycle, parameter lookup,
  and static-import findings were treated as applied and were not re-reported.

## Coverage

Reviewed only Chunk 1, “Source fact construction,” from
`CODEBASE_STRUCTURE_CORE.md`, including `analysis/mod.rs`, the fact-builder
orchestration and stream freeze boundary, provenance checkpoints and origin
maps, traversal state and function/control visitors, call/argument lowering,
module-interface recording, and the fact-construction tests. Historical audit
files and their applying commits were inspected to avoid re-reporting
completed findings. No source, test, configuration, dependency, or other
documentation files were changed; this chunk audit file was updated only with
review dispositions. The next chunk is Chunk 2, “Scope, syntax, and evidence frontend,”
which should continue finding IDs at READ-046.
