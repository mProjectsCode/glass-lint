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

**Audit disposition (2026-08-13):** Confirmed. This is a test-fixture lifetime
change only; the production planner and collector should continue borrowing the
shared bounded budget.

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

**Audit disposition (2026-08-13):** Confirmed. The instrumentation is
test-only and must not replace the production shape queue or its validation.

### Bounded constant evaluation

#### [ ] READ-048 — Make one evaluator accept borrowed expression nodes

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API / Performance
- **Location:** `glass-lint-core/src/analysis/syntax/constant/eval.rs:78-149`; borrowed
  binary caller in `glass-lint-core/src/analysis/facts/visitor.rs:290-296`

The duplicate `EvalState::evaluate`/`evaluate_binary` entry points are not the
root problem. `evaluate_binary` was added because the fact visitor has a
borrowed `&BinExpr` and wrapping it in `Expr::Bin(binary.clone())` clones the
entire nested expression subtree on hot bundled inputs. The current split now
duplicates the evaluator boundary and forces callers and tests to choose
between two ways of evaluating the same semantic node. The shared evaluator
already owns the recursion, node, lookup, and string/container bounds; those
bounds should not be represented by separate APIs merely to preserve the
borrowed fast path.

**Recommendation:** Replace the two entry points with one borrowed-node
`evaluate` operation that accepts either `&Expr` or `&BinExpr` through one
private evaluator-input abstraction (for example, an `EvalNode<'a>` enum with
conversions for both borrowed node types). Have `EvalState::evaluate` perform
the depth/node admission and balanced unwind exactly once, then dispatch the
already-borrowed node to the existing expression or binary semantics without
wrapping or cloning a subtree. Remove the standalone `evaluate_binary` helper
and update the resolver, fact visitor, and tests to call the single operation.

The abstraction must be an input view, not an owned AST conversion or a second
expression model. Preserve fail-closed `Unknown` on either bound, count one
node for a binary expression (including an unsupported operator), keep nested
operands on the same state, retain the current `+` and deterministic
string-size semantics, and leave identifier/member lookup ownership with
`Lookup`.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Superseded by this root-cause review. A
private admission helper alone would remove duplicated bookkeeping but leave
the unnecessary two-node API in place. The accepted fix is one borrowed-node
evaluator that removes the cloning workaround and makes the shared budget
boundary canonical.

### Constant property-key conversion

#### [x] READ-049 — Share scalar property-text conversion

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

**Audit disposition (2026-08-13):** Confirmed. Centralize only the accepted
scalar domain; keep the two existing result types and allocation boundaries
separate so no general conversion API is introduced.

**Fix Applied:** Added a private borrowed `ScalarPropertyText` conversion
that owns the accepted string/non-negative-integer domain. `property_key` and
`to_property_string` now select their existing output types from that shared
primitive; rejection and allocation boundaries are unchanged. Verified with
`make fmt && make ci`.

## Systemic Themes

- The scope frontend has a sound typed phase progression, but test helpers
  currently force production lifetimes into `'static` rather than expressing
  fixture ownership directly.
- Bounded analysis is centralized conceptually in `EvalState`; the evaluator
  should expose one borrowed-node entry point so performance exceptions do not
  create parallel semantic paths or duplicate budget policy.
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
or other documentation files were changed; this chunk audit file was updated
only with review dispositions. The next chunk is Chunk 3, “Flow analysis,” which should continue
finding IDs at READ-050.
