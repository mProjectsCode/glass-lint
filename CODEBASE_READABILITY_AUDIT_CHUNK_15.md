# Codebase Readability Audit — Chunk 15

## Summary

Chunk 15 owns the public rule/query authoring API, validation passes,
order-independent normalization, physical query planning, lifecycle flow
lowering, plan requirements, and compiled rule selection. The phase split is
appropriate: provider rules remain declarative while execution receives one
private physical plan. The main readability risks are that several semantic
contracts are represented independently at adjacent phase boundaries. Plan
capabilities are calculated from both normalized and physical forms, valid
identity/event/subject combinations are checked by multiple owners, and the
canonical argument representation is repeatedly expanded into caller-owned
vectors. Query-shape analysis and fallible authoring errors also have APIs that
make callers coordinate state and failure timing themselves.

The capability, lowering, normalized-IR, and matcher findings were cross-checked
against Chunk 3; the query declaration and flow findings were cross-checked
against Chunks 1, 7, and 8. Those findings are not repeated here.

## Findings

### Compiler phase contracts

#### [x] READ-068 — Give one plan phase ownership of executable requirements

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/requirements.rs:110-225`; `api/compiler/normalize.rs:36-88`; `api/compiler/physical.rs:334-465`; orchestration in `api/compiler/mod.rs:229-245`

`PlanRequirements` is first derived from the normalized tree by
`PlanRequirements::for_root`: event arguments request local static values,
identities request value/project capabilities, and lifecycles request flow.
The same capability mapping is then reimplemented by
`physical::executable_requirements` over `PhysicalRoot` variants. The
normalized requirements are copied into `PhysicalPlan`, and
`validate_physical_plan` compares them with the second derivation. The
normalized validator also recomputes the first derivation before planning.

This is a useful consistency check, but the mapping itself has two semantic
owners. Adding an identity kind, physical root, or flow operator requires
editing both trees; otherwise compilation fails late with the generic
`RequirementsMismatch` instead of identifying the missing capability. The
`cross_file` flag is also carried through the same structure without a
corresponding `require_*` transition, so its lifecycle is caller-visible but
not owned by the compiler capability API.

**Recommendation:** Make the lowered physical operator own its capability
description, for example through a private `requirements()` operation on each
`PhysicalRoot`, and derive the final plan requirements from the validated
physical roots. If normalized requirements are retained for planning or
diagnostics, compare them as an explicit assertion while keeping one mapping
implementation. Add named transitions for every flow capability that the
compiler can request, or remove an unrequestable flag until its owner exists.
Preserve exact project/value preparation, flow short-circuiting, deterministic
plan explanations, and the fail-closed mismatch check during migration.

**Fix Applied:** Removed capability requirements from normalized IR and made
`PhysicalRoot::requirements` the single executable mapping. `PhysicalPlan`
now derives requirements from validated roots; callers may provide an
expected set only for an explicit fail-closed mismatch check. Normalization
and physical compiler tests continue to cover constrained values, project
overlays, flow, alternatives, and deterministic plans. Verified with
`make fmt && make ci`.

#### [ ] READ-069 — Centralize the identity/event/subject compatibility matrix

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/validate/error.rs:173-234`; `api/compiler/normalize.rs:125-151`; `api/compiler/physical.rs:81-152,365-413`; lifecycle conversion in `api/compiler/object_flow.rs:151-188`

The supported semantic shapes are described in several independent forms.
`is_valid_identity_event_pair` checks direct identity/event dimensions;
normalized validation separately allows returned and constructed subjects on
member calls, member reads, and property writes; `plan_event` extracts a
member with a fallback to `SymbolPath::default`; and `PhysicalRoot::validate`
then rejects empty members or any event other than the variants it can
execute. Lifecycle validation similarly accepts only global calls and rooted
member calls, while `CompiledObjectSource::from_normalized_event` repeats that
matrix and returns `None` for an unsupported shape.

These are not just redundant checks. The normalized subject check currently
admits `PropertyWrite` for returned/instance subjects, while the physical
planner does not extract a member for that event and the physical validator
rejects the resulting empty member. A new or internally assembled declaration
can therefore pass validation and normalization before failing as an opaque
`InvalidLoweredQuery`. The same split means a lifecycle shape can be accepted
by one phase and silently dropped by an `Option`-returning lowering helper.

**Recommendation:** Introduce typed relation-specific normalized/lowered
constructors (for example, direct event, returned member, constructed member,
and lifecycle source) whose constructors own the compatibility matrix. Have
validation and lowering call those constructors rather than maintaining
parallel `matches!` tables and default-member fallbacks. Replace `Option` at
the compiler boundary with a structured unsupported-shape error, and delete
the now-obsolete matrix copies. Preserve rooted-versus-module identity rules,
member-path equality, property-write support where it is semantically valid,
bounded lifecycle sources, and structured diagnostics before physical plan
construction.

**Fix Applied:** None so far.

### Canonical data and query analysis

#### [x] READ-070 — Keep canonical argument constraints in one executable representation

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:72-147`; lowering in `api/compiler/object_flow.rs:173-239`; downstream copy in `analysis/flow/planning.rs:230-236`

`CanonicalArgumentConstraints` owns the important invariant: groups are sorted
by argument index and predicates are sorted and deduplicated. Its
`to_flat_vec` method reconstructs new `ArgumentConstraint` values from that
representation. Compiled lifecycle sources and member-call requirements both
store those flattened vectors, and flow planning immediately clones the source
constraints again into `BoundSource`. Runtime matching then consumes the flat
view, so the canonical owner is no longer present at the execution boundary.

The repeated expansion is more than allocation noise. It gives normalized
groups and runtime vectors separate storage and makes callers responsible for
remembering that grouping, ordering, deduplication, and per-index bounds were
already established. If argument matching gains a group-sensitive operation or
another lowering path is added, the flat copies can drift from the canonical
semantics.

**Recommendation:** Define one private compiled constraint value that retains
the canonical groups and exposes zero-copy predicate/group iterators plus the
matching operation needed by flow planning. Use that value in
`CompiledObjectSource`, `CompiledObjectRequirement`, and `BoundSource`; only
materialize a flat vector at a boundary that genuinely requires one, and make
that boundary explicit. Preserve argument-index conjunction semantics,
deterministic ordering, deduplication, static-alternative limits, and the
existing matcher behavior for absent or dynamic arguments.

**Fix Applied:** Preserved `CanonicalArgumentConstraints` through compiled
lifecycle sources, member-call requirements, and bound flow sources. Added a
zero-copy indexed predicate iterator used by local and cross-file matching;
flat vectors remain only at the reference-oracle boundary. Verified argument
ordering, deduplication, dynamic/absent argument behavior, and flow matching
with `make fmt && make ci`.

#### [x] READ-071 — Share bounded query-shape facts across validation and normalization

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/rule/query/expression.rs:97-147`; callers in `api/compiler/validate/pass4_10.rs:165-246` and `api/compiler/normalize_all.rs:29-105`

The expression type has one recursive walker, but exposes three separate
derived operations: `vars()` allocates every encountered variable,
`contains_var()` rescans until a target is found, and `binding_vars()` walks
again while filtering roles. Correlation validation calls `vars()` for each
`All` branch and then calls `contains_var()` for evidence checks. Same-event
normalization first collects `vars()` for every branch and then
`find_common_event_var` calls `binding_vars()` and `contains_var()` over the
same branches. Scope/type validation is another recursive pass before these
operations.

The query limits keep this bounded, but nested alternatives multiply the same
tree walks and temporary vectors. More importantly, the branch-local meaning
of “bound”, “referenced”, “present for evidence”, and “common event” is spread
between callers rather than represented by one analysis result. A future
correlation rule can easily use a raw variable list where binding role or
branch scope matters.

**Recommendation:** Add one private, bounded `QueryShapeFacts`/branch-analysis
operation that collects role-partitioned variables and the evidence/common-
event facts needed by both correlation validation and `normalize_all_root`.
Keep public `QueryExpr` opaque and preserve the current validation order and
Any-branch scope isolation; the shared result should be an internal analysis
artifact, not a new public query model. Delete the repeated `vars`,
`binding_vars`, and `contains_var` scans after callers use the fact owner.
Preserve duplicate-binding diagnostics, primary-variable evidence rules,
uncorrelated-conjunction rejection, depth/child bounds, and deterministic
variable renumbering.

**Fix Applied:** Added the private bounded `QueryShapeFacts` artifact, built by
one role-aware expression walk. Correlation/evidence validation and
same-event normalization now share its variable membership and binding-role
facts; legacy `vars`/`contains_var` behavior remains available where the
query API still uses it. Preserved branch scope, duplicate-binding and
uncorrelated-conjunction diagnostics, and verified with `make fmt && make ci`.

### Rule authoring API

#### [x] READ-072 — Do not encode invalid value matchers as empty accepted sets

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/value.rs:147-164`; canonicalization in `api/rule/query/value.rs:80-118`

`ValueMatcher::try_equals` correctly propagates `canonical_exact` errors, but
the infallible `ValueMatcher::equals` catches every error and constructs an
`Exact(Vec::new())` predicate. The returned value still reports the public
`StaticString` matcher kind, yet it represents no accepted value and can flow
through an otherwise successful rule declaration. This differs from the
other public predicate constructors, which return `Result` for empty values,
empty collections, and bounded alternatives. The existing empty-value test
documents the behavior, but it also confirms that invalid author input is
being converted into a valid-looking semantic object rather than rejected.

**Recommendation:** Make exact matching consistently fallible: either change
`equals` to return `Result` and migrate callers to `?`, or remove it in favor
of `try_equals` while retaining a clearly named internal constructor only if
an empty accepted set is ever a deliberate semantic value. Make the matcher
type unable to represent an accidental empty exact set, and keep trimming,
deterministic canonicalization, alternative limits, and compile-time
diagnostics at the authoring boundary.

**Fix Applied:** Removed the infallible `equals` constructor and migrated all
callers to `try_equals`, preserving the existing canonicalization and error
path. Empty exact values now remain rejected and cannot be represented as an
`Exact(Vec::new())` matcher. Verified with the focused value-matcher tests and
`make fmt && make ci`.

#### [x] READ-073 — Choose one error-timing contract for fluent rule builders

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/mod.rs:107-175`; `api/rule/query/lifecycle.rs:486-575`

`RuleBuilder::query` and `LifecycleQueryBuilder::{source,condition,completion}`
accept fallible inputs but record only the first error in an internal
`Option`, continue collecting later state, and report it from `build`.
Parallel `try_query`, `try_queries`, `try_source`, `try_condition`, and
`try_completion` return the same construction errors immediately. The two
contracts are easy to mix: a caller can append valid declarations after an
invalid one, later duplicate-stage errors are retained or ignored according
to call order, and the eventual diagnostic is detached from the operation
that caused it. The same deferred-state pattern is separately implemented in
two builders, so error precedence and lifecycle behavior must be kept in sync.

**Recommendation:** Keep immediate `try_*` methods as the strict construction
path, and move the existing deferred behavior behind an explicitly named
catalog builder used by provider declarations. Remove deferred error state
from ordinary lifecycle builders after callers migrate. Preserve declarative
catalog ergonomics, first-error determinism in the named deferred wrapper,
duplicate metadata/stage diagnostics, collection bounds, and the rule that
invalid declarations never reach query compilation.

**Fix Applied:** Made `Rule::builder` and `LifecycleQuery::builder` strict
builders whose `query`/stage methods accept validated values, with `try_*`
methods returning construction errors immediately. Moved first-error deferred
collection behind explicitly named `catalog_builder` wrappers and migrated
provider/catalog callers. Verified with `make fmt && make ci`.

## Systemic Themes

- The compiler has a sound phase outline, but semantic dimensions and
  preparation capabilities are represented in several adjacent forms. Typed
  phase outputs should carry the invariants needed by the next phase instead
  of relying on late equality checks, default values, or `Option` fallbacks.
- Canonical representations are useful only while their owners remain visible:
  normalized constraint groups and query-shape roles are repeatedly expanded
  into caller-owned collections before execution.
- The public authoring surface mixes validated values, deferred errors, and
  silently unmatchable sentinels. A single failure-timing policy would make
  provider rule declarations easier to review and diagnostics easier to trust.

## Decisions

- Physical roots own executable requirements. Normalization may retain
  capability facts for diagnostics, but lowering derives the final requirements
  from validated physical roots and checks the normalized view only as an
  assertion.
- Returned/constructed property-write matching is intentionally unsupported in
  the current relation model. The authoring validator must reject it with a
  structured diagnostic; no physical-root fallback or silent `None` lowering
  is permitted until a first-class relation is designed.
- Provider catalogs do rely on deferred fluent errors for declarative
  ergonomics. Keep that behavior behind an explicitly named deferred catalog
  builder, while ordinary `try_*` APIs remain immediate and lifecycle stage
  errors do not leak into an unrelated builder state.

## Coverage

- **Reviewed modules:** `api`, `api::classification`, `api::compiler`,
  `api::compiler::{catalog,contradiction,error,normalize,normalize_all,
  normalized,object_flow,physical,requirements,rule,validate}`,
  `api::rule`, `api::rule::{error,module,query,taxonomy}`, and
  `api::rule::query::{composition,constructors,error,event,expression,
  lifecycle,limits,private,value}`.
- **Workflow traced:** public rule/query constructors → validation passes →
  normalization and same-event merging → physical roots and requirements →
  compiled rule selection → local/flow consumers of lifecycle constraints.
- **Prior overlap check:** Chunk 3’s cache/lowering/evaluation-context
  findings and Chunks 1, 7, and 8’s fact/flow findings were considered and
  are not repeated.
- **Fixes:** None; this is a read-only structural audit.
- **Tests:** Not run; no source behavior was changed.
