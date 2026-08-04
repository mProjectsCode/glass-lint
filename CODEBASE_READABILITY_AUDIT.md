# Codebase Readability Audit

## Summary

This focused audit looks for the same structural smell as the recent lowering
cleanup: forwarding wrappers, one-purpose indirection, and domain rules that
are repeated outside their narrowest owner. It reviewed the current workspace
tree and identified five concrete opportunities. The findings are read-only;
no source, test, configuration, dependency, or generated reference files were
changed.

## Findings

### Facts and module-interface construction

#### [x] READ-001 — Module-interface export methods add visibility-only forwarding

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/facts/interface/exports.rs:77-92`; caller bridge in `glass-lint-core/src/analysis/facts/mod.rs:288-301`

`ModuleInterfaceBuilder::record_local_named_exports_only` and
`record_reexports_from_source` only forward their arguments to private methods
with the same operation and no additional validation, ownership, or policy.
They exist solely because the implementation methods are private to the
`interface` child module, leaving the facts builder with two names for each
operation and making the module boundary look more substantial than it is.

**Recommendation:** Make the two implementation methods visible to the
`crate::analysis::facts` boundary and delete the forwarding methods, or move
the operations to the parent builder if that is the intended owner. Update the
two `FactBuilder` call sites to use the canonical names directly. Preserve
type-only export filtering, source-span validation, resolver access, and the
separate local-export versus re-export behavior.

**Fix Applied:** Made the canonical local-export and re-export operations
visible to the facts boundary, deleted the visibility-only forwarding methods,
and updated the `FactBuilder` callers. Verified with
`cargo test -p glass-lint-core --lib analysis::facts`.

### Resolver identity queries

#### [x] READ-002 — Resolver identity APIs retain pass-through and misnamed layers

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:19-56,151-170`

`resolve_ident` and `resolve_member` are one-line forwarding methods to
`resolve_ident_uncached` and `resolve_member_uncached`, but those “uncached”
implementations perform cache lookup, cycle tracking, cache insertion, and
value construction themselves. The identity-only methods then repeat the same
cache-key lookup shape before falling back to the full resolver, so the public
internal vocabulary obscures the actual cached lifecycle and leaves a
wrapper/implementation pair with no distinct invariant.

**Recommendation:** Collapse each forwarding pair into one canonical resolver
operation, and factor only the genuinely shared cache/cycle finalization if it
removes duplication without hiding the identifier/member semantic differences.
Keep the narrow ID fast paths, cycle-to-unknown behavior, value-arena budget
handling, module-member provenance, returned-member provenance, and cache
identity unchanged.

**Fix Applied:** Collapsed the identifier and member resolver forwarding pairs
into their canonical cached operations, retaining each operation's distinct
cache keys, cycle handling, provenance construction, and budget behavior.
Verified with `cargo test -p glass-lint-core --lib analysis::resolution`.

### Query compiler type validation

#### [x] READ-003 — Event-kind variable typing is duplicated across compiler passes

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/validate/pass1_3.rs:58-80,129-181`; parallel mapping in `glass-lint-core/src/api/compiler/normalize.rs:308-337`

The compiler maps `EventSpec` to call, member, or general event types in both
`var_type_for_event` and `var_type_for_event_kind`, whose match arms are
identical, and separately maps the same cases to string labels in
`branch_var_type`. The first helper even accepts an unused identity argument.
This spreads one compatibility rule across validation and normalization, so a
new event kind can be classified consistently in one pass and incorrectly in
another.

**Recommendation:** Give the compiler one provider-neutral event-kind
classification operation, preferably returning the internal type enum, and
reuse it for branch compatibility and scope/type validation; delete the
duplicate helper and string-level match. Preserve the special `Object` and
`Lifecycle` cases, the existing widening rules between `Event`, `CallEvent`,
and `MemberEvent`, and the diagnostic names exposed by compile errors.

**Fix Applied:** Added `EventSpec::variable_type` as the single event-kind
classification operation, reused it for scope/type validation and normalized
branch compatibility, and replaced stringly typed branch classification with
a typed wrapper that preserves the `lifecycle` diagnostic. Verified with
`cargo test -p glass-lint-core --lib api::compiler`.

### Query correlation validation

#### [x] READ-004 — Correlation validation traverses the query tree through two recursive engines

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/validate/pass4_10.rs:161-245`

The advertised consolidated `pass_correlation_evidence` traversal calls
`check_correlation_evidence`, but its `Any` branch delegates correlation
checking to a second recursive function, `check_correlation_scope_inner`.
That helper repeats the `All`/`Any` tree walk and the correlated-branch check,
while the outer function separately walks branches for primary-evidence
coverage. The split makes the order and scope of correlation versus evidence
validation harder to see and leaves two recursive paths to update when query
operators change.

**Recommendation:** Consolidate the recursive walk behind one clearly named
tree validator, carrying the primary-evidence context needed at `All` and
`Any` nodes, then delete the duplicate recursion helper. Keep correlation
checks at conjunctions, the rule that every `Any` branch contains the primary
evidence variable, the top-level/branch distinction, and the current
fail-closed error ordering.

**Fix Applied:** Consolidated correlation and evidence validation into one
recursive walker with an explicit evidence-check context, removed the second
tree traversal, and preserved conjunction correlation checks, Any-branch
evidence requirements, and validation ordering. Verified with
`cargo test -p glass-lint-core --lib api::compiler`.

### Local flow projection

#### [ ] READ-005 — Sink projection duplicates ready-state emission policy

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/evidence.rs:60-110,112-166`

`ObjectFlowProjector::record_sinks` and `record_helper_sink` independently
look up a completed state, check `is_ready` plus `sinks_ready`, and queue it
for emission. The two paths differ in how they discover and update sink
states, but the completion policy and state cloning are repeated in full;
future certainty or exhaustion changes can therefore diverge between direct
sinks and summarized helper sinks.

**Recommendation:** Add a narrow projector-owned operation for emitting a
completed sink state and route both paths through it after their distinct
state updates. Keep `record_configuration`'s configuration-only completion
rule separate, preserve the pending-state/certainty lifecycle, and retain the
different direct-call versus helper-summary lookup behavior.

**Fix Applied:** None so far.

## Systemic Themes

1. The most useful cleanup targets are not generic “long functions”; they are
   places where an internal visibility boundary or phase name creates a second
   API for the same operation.
2. Query compilation has several independently authored representations of
   event compatibility. Centralizing only the event-kind relation would reduce
   drift without merging the distinct normalization, validation, and runtime
   plan lifecycles.
3. Flow projection correctly keeps direct calls, helper summaries, and
   configuration events distinct. The repeated readiness policy can be owned
   centrally without collapsing those semantic paths.

## Open Questions

None for these findings. The recommended owners and deletion targets are
identifiable from current callers; behavior-sensitive changes should still add
focused tests for compile diagnostics, resolver cache/budget behavior, and
direct/helper flow certainty.

## Coverage

Reviewed the workspace architecture, core architecture, testing and
contribution guidance, compiler normalization/validation, local resolution,
facts and module-interface construction, local and cross-file flow projection,
project loading, session artifacts, lint report assembly, and the current
readability-ranked methods from `python3 private/complex.py . --top 100`.

Representative adjacent areas were checked and not reported when the extra
layer established a real ownership or phase boundary: `Lowerer`'s current
scope-to-freeze transition, `ReportAssembly::finish`,
`TsconfigTraversal::build_inner`, bounded flow state tables, and normalized
evidence assembly. Existing unrelated worktree changes were preserved.
