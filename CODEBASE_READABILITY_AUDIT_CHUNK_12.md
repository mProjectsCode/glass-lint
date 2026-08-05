# Codebase Readability Audit — Chunk 12

## Summary

Chunk 12 covers retained scope identities and provenance, bounded static
objects and values, and provider-neutral module-request recognition. The
types already provide useful opaque IDs, explicit binding-provenance variants,
bounded property storage, and one shared request recognizer for resolver,
scope, and fact phases.

The main risks are incomplete ownership of model invariants: value bindings
can index the terminal cache without validation, static-property conversion
silently drops unresolved keys, and the same object-shape limit is declared in
two modules. Callable metadata is stored in the value arena but consumed only
through a separate resolver record, while request policy is an open set of
booleans despite having a small closed set of supported modes.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Value identity safety

#### [x] READ-061 — Make binding-target interning reject invalid value IDs

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Error handling / Resource bounds
- **Location:** `glass-lint-core/src/analysis/model/value.rs:163-198`

`ValueTable::intern` special-cases `Value::Binding` and indexes
`terminal_cache[target.raw()]` with `unwrap`. `Value::Binding` is a public enum
variant and `ValueTable::intern` is a public method within the analysis API,
so a binding whose target is not already present can reach this branch even
though ordinary reads such as `get` and `resolve` fail closed with `Option`.
The same malformed or stale identity therefore panics during insertion rather
than becoming `ValueId::UNKNOWN` or an explicit exhausted/invalid result.

Move target validation into the value-table owner: check the target against
the existing terminal-cache entry before inserting, and return the existing
unknown/exhaustion representation when it is absent. Keep the no-forward-
reference invariant, deduplication, terminal-cache fast path, and ordinary
invalid-ID read behavior intact; add a regression test for an invalid binding
target.

**Fix Applied:** Validated binding targets against the existing terminal cache
before interning, returning `ValueId::UNKNOWN` and recording exhaustion for
invalid IDs instead of indexing with an unchecked `unwrap`. Added a regression
test that verifies malformed targets do not insert a value. Verified with
`cargo test -p glass-lint-core --lib analysis::model::value` and
`make fmt && make ci`.

### Static-shape conversion

#### [x] READ-062 — Make unresolved static-property names fail closed

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Completeness / Fail-closed semantics
- **Location:** `glass-lint-core/src/analysis/model/static_properties.rs:77-89`,
  `analysis/scope/mod.rs:45-58`, `analysis/scope/query/constants.rs:24-35`

`StaticProperties::to_const_object` uses `filter_map(resolve_name)`, so a
property whose `NameId` cannot be resolved is silently omitted from the
resulting `ConstValue::Object`. The caller treats that result as a valid
static shape, and both `StaticObjectKeys` and `StaticObjectValues` use the
same projection. A mismatched or exhausted name table can consequently turn
an incomplete object into a smaller apparently-known object, allowing later
constant or matcher logic to reason about a shape that was never completely
resolved.

Make the projection return `Option<ConstValue>` or an explicit unknown result,
and let the owning constant query map any missing key to `ConstValue::Unknown`.
Preserve deterministic key order, last-write-wins storage, and the distinction
between unknown property values and an unknown property name; do not silently
discard an unresolved identity.

**Fix Applied:** `StaticProperties::to_const_object` now returns `None` when
any retained `NameId` cannot be resolved, and the scope constant projection
maps that incomplete shape to `ConstValue::Unknown`. Added regression coverage
for both complete and unresolved key projections while preserving deterministic
ordering and last-write-wins storage.

**Verification:** `cargo test -p glass-lint-core --lib analysis::model::static_properties`
and `make fmt && make ci` pass.

### Static object bounds

#### [ ] READ-063 — Share the static-object property budget with constant evaluation

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Resource bounds / Cross-module contract
- **Location:** `glass-lint-core/src/analysis/model/static_properties.rs:6-10`,
  `analysis/syntax/constant/types.rs:5-10`,
  `analysis/syntax/constant/eval.rs:210-232`

`StaticProperties` declares a private `MAX_STATIC_PROPERTIES: usize = 256`,
while the constant evaluator independently declares
`MAX_OBJECT_KEYS: usize = 256`. `StaticObject::new`, scope provenance
construction, and constant evaluation are intended to accept the same static
object shapes, but the limit is not owned by a shared model or syntax-boundary
constant. A future change to one bound can make one construction path retain a
shape that another path maps to `Unknown`, with no compiler signal identifying
the policy drift.

Give the shared static-shape budget one owner and import it from both the
constant evaluator and `StaticProperties`, or make the bound an explicit
constructor parameter supplied by the analysis budget. Delete the duplicate
literal and retain the current rejection-before-partial-retention behavior
for over-budget objects.

**Fix Applied:** None so far.

### Callable value representation

#### [x] READ-064 — Remove or consume callable metadata stored outside resolver state

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Model / Ownership / Dead state
- **Location:** `glass-lint-core/src/analysis/model/value.rs:120-139`,
  `analysis/resolution/call.rs:93-108,155-162`,
  `analysis/resolution/mod.rs:48-64`,
  `analysis/facts/calls/mod.rs:109-125`

`CallableValue` stores `target`, `receiver`, and `bound_arguments`, but the
only production accessor is `target()`. `Resolver::call_provenance_for_value`
follows only the target, while receiver and bound-argument semantics are
carried separately by `ResolvedValue` and consumed by
`FactBuilder::effective_call_args`. The arena therefore interns equality and
hash distinctions for receiver/argument vectors that no value consumer reads,
and the same callable concept has two independent metadata representations.

Choose one owner for callable application metadata. The smallest migration is
to make the arena value carry only the target identity and keep receiver and
bound arguments in `ResolvedValue`; alternatively, expose and consume all
three fields through value-table APIs and delete the parallel resolver fields.
Preserve callable target chaining, receiver-sensitive facts, bound-argument
ordering, and cache determinism while removing the unused state.

**Fix Applied:** Reduced `CallableValue` to its consumed target identity.
Receiver and bound-argument metadata remain in resolver/scope state, where fact
construction applies them to effective calls, removing duplicate inert arena
state.

**Verification:** `cargo test -p glass-lint-core analysis::model::value --lib`
(24 passed), `cargo test -p glass-lint-core analysis::resolution --lib` (12
passed), and `make fmt && make ci` (passed).

### Module-request policy

#### [x] READ-065 — Encode supported module-request modes as a closed policy

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API / State space / Request policy
- **Location:** `glass-lint-core/src/analysis/module_request.rs:28-67,101-148`,
  callers in `analysis/facts/mod.rs:267-285`,
  `analysis/resolution/call.rs:40-43`, and
  `analysis/scope/build/provenance.rs:80-112`

`ModuleRequestPolicy` has three independent boolean fields, but all current
callers use one of four named constructors: interface, direct require, alias,
or alias with dynamic import. The recognizer branches on those flags in
separate direct-require, wrapper, and dynamic-import paths, so combinations
such as allowing wrappers while requiring exactly one direct-require argument
are representable without a defined semantic contract. A future caller can
therefore create a policy that is accepted by the type but has no documented
meaning, and every new request form expands the flag matrix rather than one
closed policy decision.

Replace the boolean bag with a private policy enum (or a small typed mode
enum plus explicit options) whose variants correspond to the supported
callers. Keep shadowing checks in `ModuleRequestContext`, preserve recursive
wrapper recognition and request-kind reporting, and delete the duplicated
field literals once all call sites use the closed modes.

**Fix Applied:** Replaced the independent module-request policy booleans with a
closed enum of the four supported recognition modes. Recognition now branches
on those modes, so unsupported combinations cannot be constructed while
shadowing, wrapper recursion, and request-kind reporting remain unchanged.

**Verification:** `cargo test -p glass-lint-core analysis::module_request --lib`
(3 passed) and `make fmt && make ci` (passed).

## Systemic Themes

Chunk 12’s model types are generally opaque and provider-neutral, but several
invariants cross the boundaries between scope, value, syntax, and request
phases. Invalid value references should be rejected by `ValueTable`; unresolved
name identities should remain unknown; and static-shape/resource policy should
have one owner. Callable metadata also needs a single authoritative carrier so
the value arena does not retain state that only a parallel resolver DTO uses.

The module-request recognizer is a valuable shared boundary. Its context-based
shadowing and static-string checks should remain centralized while the policy
surface becomes explicit enough that resolver, scope, and fact callers cannot
drift into undocumented combinations.

Search signals used for this chunk included unchecked terminal-cache indexing,
`filter_map` over semantic name IDs, duplicate static-object limits, callable
fields with no production accessors, and a boolean request-policy product
space with only named mode constructors. No findings are marked applied.

## Open Questions

- Whether invalid binding targets should map to `Unknown`, a value-arena
  exhaustion status, or a distinct malformed-input status should follow the
  existing diagnostic vocabulary, but insertion must not panic.
- If static-shape limits are intentionally different in future, the two
  budgets should become explicitly named policies rather than equal literals
  implying one shared contract.
- The next unreviewed handoff is Chunk 13: project, resolution, and module
  identity types.

## Coverage

Reviewed the Chunk 12 types listed in `CODEBASE_STRUCTURE_CORE.md` across
retained scope identities/provenance, static-property collections, value and
callable identities, value-table bounds, and module-request policy and
recognition. Representative callers were traced through scope collection and
queries, resolver expression/call handling, fact construction, constant
evaluation, matcher value access, and module-interface request recording.
Existing Chunk 1–11 findings were checked to avoid re-reporting scope-graph
freeze/lookup issues, provenance-alternative capacity behavior, value-table
pairing, effective-call-argument projection, and fact/module-interface state
issues. No source, test, configuration, dependency, or documentation changes
were made.
