# Codebase Readability Audit — Chunk 02

## Summary

Chunk 02 owns the lexical scope frontend, bounded syntax helpers, and trace
arena. The phase split is valuable: planning establishes hoisted visibility,
collection records source-order facts, and freezing exposes immutable query
state. The findings below target work repeated across those boundaries and
small APIs that create data only to consume it immediately. None require
weakening scope-shape validation, path-local identity, bounded evaluation, or
fail-closed behavior.

## Findings

### Scope planning and collection

#### [x] READ-006 — Scope collection repeats planner-owned binding registration

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/mod.rs:63-77`; `glass-lint-core/src/analysis/scope/build/plan.rs:93-117,195-237`; `glass-lint-core/src/analysis/scope/build/collector.rs:66-107,151-157`; `glass-lint-core/src/analysis/scope/build/visitor.rs:119-142,172-205,208-245`; `glass-lint-core/src/analysis/model/scope.rs:278-284`

The planner and collector both register imports, variable-pattern locals,
class/function locals, and function parameters. The collector then inserts
the same keys again while visiting the source, and declaration provenance may
insert the key a third time. `LexicalScope::insert_binding` is an unconditional
`HashMap::insert`, so these passes overwrite equivalent entries rather than
sharing a planned binding slot. Each collector insertion also charges the
shared semantic budget before discovering that the name is already interned.
The catch parameter is a legitimate collector-only case because the planner
does not have a corresponding `visit_catch_param` hook; it should remain
covered separately.

This duplicates declaration work in every two-pass scope build and makes
resource exhaustion depend on phase duplication. A tight budget can therefore
turn an otherwise identical source into an incomplete scope artifact before
the source-order facts have been collected.

**Recommendation:** Make the planner the sole owner of declaration/import
slot registration. Have the collector update the planned binding's provenance
through an explicit update operation, or record source-order provenance in a
separate collector-owned table that is merged at freeze. Keep collector-only
catch bindings, hoisted `var` placement, parameter visibility, provenance
overwrite rules, and the shared budget's fail-closed behavior unchanged.

**Fix Applied:** The planner now owns all ordinary declaration and import slot
registration. The collector updates those planned slots for declaration
provenance and redeclaration resets, while retaining catch-parameter
registration as its collector-only case. Destructuring and CommonJS alias
collection use the same update operation, eliminating duplicate binding
insertions and their budget charges. Added a redeclaration regression test and
updated the semantic-budget boundary from 24 to 22 operations to reflect the
removed duplicate work. Verified with `make fmt && make ci`.

#### [x] READ-007 — Collector records are converted into immediately consumed duplicate data wrappers

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/build/program.rs:20-63,65-102`; `glass-lint-core/src/analysis/scope/build/mod.rs:117-140`; `glass-lint-core/src/analysis/scope/graph.rs:307-361`

`PropertyAliasAssignmentData`, `RootedPropertyMutationData`, and
`ScopedDynamicEvalData` duplicate the fields of their corresponding
collector records. Each `into_data` method moves every field into the duplicate
struct, and `finish_collected_properties` immediately destructures that data
to build the mutation indexes. The `*Data` types have no other consumers, so
the conversion adds three representations and three APIs without enforcing a
phase invariant or changing ownership.

**Recommendation:** Remove the duplicate data structs and let the graph
consume the original private records through narrow owner-defined accessors or
an `apply_to` operation. Preserve the graph's receiver binding lookup,
dynamic-eval shadowing check, sorted mutation-index construction, and the
existing `FrozenPropertyArtifacts` phase boundary; the recommendation is only
to eliminate the immediately consumed field-for-field wrappers.

**Fix Applied:** Removed the field-for-field `*Data` wrappers. Collector records
now expose consuming `into_parts` decompositions directly to graph freezing;
the receiver lookup, dynamic-eval shadow check, sorted mutation indexes, and
`FrozenPropertyArtifacts` phase boundary are unchanged. Verified with
`make fmt && make ci`.

### Scope queries

#### [ ] READ-008 — Rooted member resolution repeats its own chain builder as a dead fallback

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/query/provenance/chain.rs:41-50`; `glass-lint-core/src/analysis/scope/query/provenance/callable.rs:205-218`; `glass-lint-core/src/analysis/syntax/names.rs:105-168`

`FrozenScopeGraph::rooted_member_chain` first calls
`self.member_expression_chain(member)`. Its fallback then reconstructs the
same `expression_name(&member.obj)` plus
`contextual_member_property_name(member)` operation. The callable helper
already implements exactly that pair, so the fallback does not add a
different syntax shape or recovery behavior; it only duplicates the chain
construction and makes future changes liable to update one copy.

**Recommendation:** Call the shared member-chain helper directly and remove
the fallback closure. Keep `resolve_member_chain` as the semantic boundary so
dynamic properties, binding reassignment, mutation invalidation, and
unsupported roots continue to fail closed.

**Fix Applied:** None so far.

### Bounded constant values

#### [ ] READ-009 — Constant interning re-bounds every already admitted subtree

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/syntax/constant/types.rs:86-123`; `glass-lint-core/src/analysis/resolution/constant.rs:60-99`; `glass-lint-core/src/analysis/scope/static_value.rs:43-59`

`ConstValue::bounded` performs a complete depth/node/container/string walk.
`Resolver::intern_const_value` calls it at the root and then recursively calls
it again for every array element and object value while interning the same
tree. Thus a nested constant repeatedly walks each remaining subtree after
the parent admission has already proven those bounds. The scope provenance
adapter also uses `bounded` as a legitimate boundary when reconstructing
values from arena-backed facts, so the issue is specifically the recursive
re-admission inside interning, not the existence of a final bound.

**Recommendation:** Separate the external/materialized-value admission step
from an internal `intern_bounded_const_value` path that assumes the already
validated tree, or pass the remaining depth/node budget through the recursive
interning operation. Preserve unknown results for oversized or malformed
arena values, the array/object limits, and the resolver's stable value-arena
identity. The internal path must remain unreachable from unvalidated callers.

**Fix Applied:** None so far.

## Systemic Themes

- The two-pass scope design is justified by hoisted visibility and source-order
  facts, but declaration slot ownership should remain in one phase; phase
  duplication should not be the mechanism that keeps the maps aligned.
- Several phase-transfer bundles are meaningful (`ScopePlan`,
  `FrozenPropertyArtifacts`, and the graph freeze transition). The weaker
  boundary is the field-for-field `*Data` conversion immediately before one
  consumer.
- The syntax and trace code generally centralize bounded, deterministic,
  fail-closed behavior. The reported constant issue is limited to repeated
  admission during recursive arena interning, not a recommendation to remove
  the safety boundary.

## Open Questions

- `ScopePlan` is consumed directly by `ScopeCollector::from_plan`; no caller
  observes planned provenance between the two passes. Collector-only enriched
  provenance should therefore be applied through a private update operation,
  not by restoring duplicate binding insertion.
- Prefer a consuming graph-application method for the private collector
  records. Narrow borrowed accessors are acceptable only if a record has more
  than one legitimate consumer; collector storage must remain private.

## Coverage

Reviewed the chunk-02 structure entries and their implementation/test support:

- `analysis/scope/{mod,binding_index,expression,frozen_assignments,graph,mutation_index,name_env,scope_index,static_value}.rs`
- `analysis/scope/build/{mod,aliases,analysis,assignments,bindings,callbacks,collector,compact_pat,constants,freeze,history,plan,program,projection,provenance,shape,traversal,visitor}.rs`
- `analysis/scope/build/analysis/{assignment,classification,mod,tests}.rs`
- `analysis/scope/query/{mod,bindings,constants,functions,rooted}.rs`
- `analysis/scope/query/provenance/{callable,chain,mod,object}.rs`
- `analysis/syntax/{mod,name,names,provenance}.rs`
- `analysis/syntax/constant/{eval,mod,tests,types}.rs`
- `analysis/trace.rs`

Supporting callers were inspected where needed to establish ownership and
repeated work, including `analysis/resolution/constant.rs`. No source, test,
configuration, dependency, or other documentation files were changed by this
audit.
