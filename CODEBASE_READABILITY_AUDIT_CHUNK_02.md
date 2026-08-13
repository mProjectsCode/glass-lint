# Codebase Readability Audit

## Summary

Chunk 2 is the provider-neutral scope, syntax, and evidence frontend. Its
planner/collector split, frozen query boundary, bounded constant evaluator,
and arena-owned trace handles are appropriate architectural seams. The
current opportunities are narrower: test-only constructors leak budgets to
manufacture a `'static` API, production scope-shape bookkeeping exists only
for unit-test counters, and two syntax paths duplicate small pieces of
bounded conversion logic.

## Findings

### Scope planner and collector test fixtures

#### [ ] READ-046 — Replace leaked test budgets with owned test fixtures

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/build/plan.rs:44-55`; `glass-lint-core/src/analysis/scope/build/collector.rs:24-28`; callers in `glass-lint-core/src/analysis/scope/build/tests.rs:8-74` and `glass-lint-core/src/analysis/scope/build/analysis/tests.rs:11-21`

The production planner and collector correctly borrow one shared
`SemanticBudget`, but their test constructors call `Box::leak` and return
`ScopePlanner<'static>` / `ScopeCollector<'static>`. Each test fixture therefore
leaks a heap allocation solely to satisfy the production lifetime shape; the
returned collector also makes the fixture appear to own a process-lifetime
resource when it is only needed for one test traversal. The helper functions
`collect` and `run` spread that artificial lifetime through their signatures.

**Recommendation:** Add a test-only fixture/helper that owns the default
budget for the duration of planner and collector use, such as a closure-based
`with_test_planner` / `with_test_collector` API or a small test fixture that
keeps the budget and traversal together. Remove both `Box::leak` constructors
and the `'static` return types. Preserve the production `&SemanticBudget`
borrowing API, the default budget limits, the two traversal phases, and the
existing scope-lookup assertions; the fixture must not make an exhausted test
budget accidentally unlimited or alter scope identity allocation.

**Fix Applied:** None so far.

### Scope-shape diagnostics

#### [ ] READ-047 — Compile the scope-shape count only for tests

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Performance
- **Location:** `glass-lint-core/src/analysis/scope/build/shape.rs:47-75, 93-96`; consumers in `glass-lint-core/src/analysis/scope/build/tests.rs:16, 143, 191, 231, 527, 542`

`ScopeShapeTable::recorded` is read only by the `#[cfg(test)]` helper
`shapes_len`, but the field is present in production builds and every planned
scope performs a saturating increment before inserting its real shape into the
`children` index. The production collector already has the authoritative
shape queues and `is_consumed` check, so this counter is test instrumentation
that currently adds storage and work to every scope collection.

**Recommendation:** Gate `recorded` and its increment with `#[cfg(test)]`,
leaving `children`, `take_child`, and `is_consumed` unchanged. Keep
`shapes_len` as test-only instrumentation and preserve the exact linear-scope
assertions; do not replace the queue with a count that could weaken the
planner/collector shape-mismatch or unconsumed-shape checks.

**Fix Applied:** None so far.

### Bounded constant evaluation

#### [ ] READ-048 — Centralize the `EvalState` node-entry transition

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Correctness
- **Location:** `glass-lint-core/src/analysis/syntax/constant/eval.rs:119-149`

`EvalState::evaluate` and `EvalState::evaluate_binary` independently implement
the same depth/node admission check, increment both counters, run a nested
operation, and decrement depth. The binary path is currently limited to `+`,
but its duplicated transition means a future change to depth accounting,
overflow handling, or an additional binary operator can update one path and
silently change the evaluator's boundedness. The surrounding evaluator already
uses the shared state specifically to make nested computed keys consume the
same budget.

**Recommendation:** Add one private node-evaluation helper that owns the
admission check and balanced depth transition, then pass the operation body
for ordinary expressions and binary expressions through it. Preserve
fail-closed `Unknown` on either bound, count one node for a binary expression,
keep nested operands on the same state, and retain the current `+` semantics
and deterministic string-size checks.

**Fix Applied:** None so far.

### Constant property-key conversion

#### [ ] READ-049 — Share scalar property-text conversion

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/syntax/constant/types.rs:123-147`; callers in `glass-lint-core/src/analysis/syntax/constant/eval.rs:223-228, 343-349, 361` and `glass-lint-core/src/analysis/syntax/names.rs:216-217`

`ConstValue::property_key` and `ConstValue::to_property_string` repeat the
same scalar domain test for `String` and `NonNegativeInteger`, differing only
in the owned output type (`SmolStr` versus `String`). This gives computed
property names and string concatenation two places to evolve the accepted
constant-key domain. The distinction in allocation/result type is valid, but
the domain rule itself should have one owner.

**Recommendation:** Introduce one private scalar property-text conversion
primitive and build the two existing result-shaped methods on top of it (or
use a small internal conversion type if that avoids an unnecessary allocation).
Keep arrays/objects/unknown values rejected, preserve the current integer
stringification and string bounds, and retain the narrower visibility of the
existing APIs so callers do not gain a new general-purpose conversion surface.

**Fix Applied:** None so far.

## Systemic Themes

- The scope frontend has a sound typed phase progression, but test helpers
  currently force production lifetimes into `'static` rather than expressing
  fixture ownership directly.
- Bounded analysis is centralized conceptually in `EvalState`; small duplicated
  transitions and conversion rules should be routed through that owner so
  fail-closed semantics remain consistent.
- Test observability should not add production fields or per-scope operations
  when the underlying semantic index already supplies the required invariant.
- The planner/collector two-pass design, frozen graph boundary, validity
  propagation, and arena-owned trace identity were reviewed and retained as
  necessary architectural structure.

## Open Questions

- None blocking these findings. Historical Chunk 2 findings READ-005 through
  READ-007 were checked in the prior audit history and remain applied; they
  were not re-reported.

## Coverage

Reviewed only Chunk 2, “Scope, syntax, and evidence frontend,” from
`CODEBASE_STRUCTURE_CORE.md`: scope planning, shape matching, collection and
freeze, binding/assignment resolution, provenance queries, constant values and
evaluation, syntax names, and the trace arena/evidence handles, including
their focused tests and callers. No source, test, configuration, dependency,
or other documentation files were changed; this chunk audit file is the only
new artifact. The next chunk is Chunk 3, “Flow analysis,” which should continue
finding IDs at READ-050.
