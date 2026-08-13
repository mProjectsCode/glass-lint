# Codebase Readability Audit

## Summary

Chunk 4 is the retained semantic domain and the resolver that turns scoped
syntax into artifact-local value identities. Opaque IDs, the consuming freeze
boundary, position-sensitive cache, and fail-closed bounded value arena are
appropriate seams; the historical Chunk 4 findings were checked and remain
applied. Three current opportunities remain: test constructors manufacture a
process-lifetime resolver budget, resolution admits a module-namespace value
whose ID is discarded, and retained flow state depends directly on compiler
IR for a readiness decision.

## Findings

### Resolver test construction

#### [ ] READ-053 — Remove the leaked budget from resolver test fixtures

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:257-293, 392-402`; callers in `glass-lint-core/src/analysis/resolution/tests.rs:14-100`

The production semantic pipeline owns one `SemanticBudget` across scope
collection, resolution, and fact construction (`semantic/mod.rs:143-153`).
The resolver's test helpers instead return `Resolver<'static>` and create the
resolver budget with `Box::leak`; `collect_with_name_limit` also uses one local
budget for scope collection and a separate leaked budget for resolution. This
leaks one allocation per fixture and hides the intended shared-budget lifetime
behind an artificial process-lifetime type.

**Recommendation:** Replace `collect`, `collect_with_environment`,
`collect_with_name_limit`, and `new_for_test` with an owned test fixture or a
closure-based helper that keeps the budget alive while the resolver is used.
Prefer passing one fixture-owned budget through both scope collection and
resolver construction so tests exercise the production budget relationship;
remove the `'static` return types and every resolver-local `Box::leak`. Preserve
the default limits, name-limit behavior, resolver cache/value snapshots, and
the distinction between unsupported and budget-exhausted values. Do not make
the fixture's budget unbounded merely to avoid borrow-checker work.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. Prefer an owned test fixture or
closure that keeps one bounded budget alive through scope and resolution; do
not change the production resolver lifetime contract.

### Resolver value admission

#### [x] READ-054 — Remove ignored module-namespace value interning

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Performance
- **Location:** `glass-lint-core/src/analysis/resolution/expression.rs:331-354`; definitions in `glass-lint-core/src/analysis/model/value.rs:105-142, 233-240`

When finalizing a module-export resolution, `finalize_seed` already retains
the member identity as `SymbolMemberProvenance::ModuleNamespace` and returns
it in `ResolvedValue`. It then interns `ValueConstruction::ModuleNamespace`
and discards the returned `ValueId`. The current codebase has no read path for
the resulting `Value::ModuleNamespace` value; namespace matching consumes the
retained provenance instead. The ignored admission still performs arena
lookup/insertion and can consume the bounded value arena, turning an otherwise
usable resolution into an exhausted artifact without adding a queryable
identity.

**Recommendation:** Delete the discarded interning step and add a regression
test showing that module-namespace provenance remains available without an
extra value-arena entry. If repository-wide search confirms that no retained
consumer needs the variant, remove `ValueConstruction::ModuleNamespace` and
`Value::ModuleNamespace` as obsolete vocabulary in the same change. Preserve
the module/export member provenance, module-export call identity, deterministic
value exhaustion behavior, and strict unknown handling; do not replace this
with a second namespace identity representation.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. The discarded namespace value
has no retained consumer; the provenance identity must remain the sole
namespace-matching representation.

**Fix Applied:** Removed the discarded module-namespace interning from
`finalize_seed` and deleted the now-unused `Value` and `ValueConstruction`
variants. `SymbolMemberProvenance::ModuleNamespace` remains the namespace
matching identity and `Value::ModuleExport` remains the callable identity.
Verified with `make fmt && make ci`.

### Retained flow readiness boundary

#### [x] READ-055 — Decouple retained flow state from compiler IR

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:1-8, 359-437, 473-573`; compiler policy in `glass-lint-core/src/api/compiler/object_flow.rs:82-131`; consumers in `glass-lint-core/src/analysis/flow/projector/evidence.rs:155-196` and `glass-lint-core/src/analysis/flow/cross/state.rs:111-168`

`analysis::model::flow` is the retained evidence/state domain, but it imports
`api::compiler::CompiledObjectFlow` and makes `FlowState::is_ready` and
`sinks_ready` accept that compiler-owned type. The generic
`LifecycleEvidence` repeats this dependency, delegating its readiness decision
to `CompiledObjectFlow::requirements_ready` and `sinks_ready`. As a result,
retained state cannot be used or evolved without compiler IR, and a public
model type exposes methods whose parameter is only `pub(crate)`. Compiler
matcher declarations, counts, and lifecycle policy are therefore coupled to
the storage model instead of crossing a narrow validated boundary.

**Recommendation:** Introduce a small model-owned readiness descriptor (or
typed requirement/completion mode plus bounded counts) and have compiler
planning lower `CompiledObjectFlow` into it at the analysis boundary. Make the
model decide readiness from that descriptor and its evidence indexes; keep
matcher-bearing sources, sinks, and compiler IR in `api::compiler`. Update the
local and cross adapters to pass the descriptor, then remove the direct model
import and the unusable public methods over compiler-private types. Preserve
the `Configuration`, `AnySink`, and `AllSinks` semantics, invalid-index
fail-closed behavior, 64-index bounds, deterministic evidence ordering, and
the distinction between retained state and compiled matcher declarations.

**Fix Applied:** Added model-owned `FlowReadiness` with bounded requirement
and sink completion modes/counts; compiler flows lower into that descriptor at
the analysis boundary, and local/cross evidence no longer accepts compiler IR.
Verified with `make fmt && make ci`.

**Audit disposition (2026-08-13):** Confirmed with a minimality constraint.
Introduce only the smallest model-owned readiness descriptor needed at the
compiler/analysis boundary; do not duplicate matcher declarations or create a
second flow model.

## Systemic Themes

- Retained domain types should own artifact identity and bounded evidence while
  compiler plans cross into analysis through small validated descriptors, not
  through compiler IR embedded in model APIs.
- Test helpers should model the same ownership and shared-budget lifecycle as
  production; leaking budgets creates a false API contract and obscures which
  phase pays for work.
- Every interned value should have a retained consumer or identity contract.
  Provenance that already owns module-namespace matching should not be shadowed
  by an unused arena entry.
- The opaque ID spaces, position-sensitive resolution cache, immutable freeze
  transition, export-observation merge semantics, and fail-closed unknown and
  exhaustion behavior were reviewed and retained as necessary structure.

## Open Questions

- None blocking these findings. Historical Chunk 4 findings READ-012,
  READ-013, READ-015, and READ-016 were revalidated as applied and were not
  re-reported.

## Coverage

Reviewed only Chunk 4, “Retained models/resolution,” from
`CODEBASE_STRUCTURE_CORE.md`: retained facts, flow state and evidence indexes,
module interfaces and requests, static properties, value identity and static
objects, module-request context, resolution cache/guards, constant conversion,
expression resolution, call resolution, and their focused tests and callers.
The root and core architecture documents, current callers, and the historical
Chunk 4 audit were inspected. The focused resolution test suite passed:
`cargo test -p glass-lint-core analysis::resolution --lib` (13 passed). No
source, test, configuration, dependency, or other documentation files were
changed; this chunk audit file was updated only with review dispositions. The next chunk is
Chunk 5, “Analysis artifact assembly and module aggregation,” which should
continue finding IDs at READ-056.
