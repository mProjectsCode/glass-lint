# Codebase Readability Audit — Chunk 17

## Summary

Chunk 17 covers classification evidence and the compiler’s normalized and
physical plan types: identity/event constraints, argument groups, lifecycle
flow plans, plan requirements, physical roots, and compiled rule records. The
pipeline has a clear validation → normalization → physical-plan shape and
keeps runtime compiler storage private to the core crate.

The remaining issues are mostly invariant ownership at those phase seams:
evidence tables are indexed by unbound catalog positions, a canonical
constraint constructor trusts a precondition it does not check, requirement
capabilities are classified by several parallel match lists, a lifecycle
lowering fallback turns an impossible source into an empty target, and the
physical-plan constructor can produce an unvalidated executable-plan value.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Classification evidence ownership

#### [ ] READ-085 — Bind evidence tables to a validated rule selection

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Index ownership / Silent omission
- **Location:** `glass-lint-core/src/api/classification.rs:69-153`,
  `glass-lint-core/src/analysis/project/projection.rs:105-166,430-435`,
  `glass-lint-core/src/analysis/flow/cross/evidence.rs:96-188`

`RuleEvidenceTable` is sized from a raw `usize`, while every write uses an
opaque `RuleIndex`. `record`, `extend`, and `replace` silently ignore an index
outside the table; `merge` only checks equal lengths with `debug_assert!` and
then truncates any mismatch through the same ignored writes. Local projection
and cross-flow projection currently derive compatible capacities, but neither
table carries the catalog/selection owner that makes that relationship true,
so stale or foreign rule indices can drop evidence without a runtime signal.

**Recommendation:** Have the table be constructed from the validated compiled
selection/catalog (or a shared rule-index domain) and make mutations return a
typed mismatch error where the invariant can fail. Preserve empty selections,
deterministic rule order, and the current local/cross evidence merge behavior;
remove the silent out-of-range paths and debug-only length check once the table
owns its capacity relationship.

**Fix Applied:** None so far.

### Normalized argument constraints

#### [x] READ-086 — Make canonical constraint construction establish its invariant

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype / Validation / Compiler IR
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:72-136`,
  callers in `glass-lint-core/src/api/compiler/normalize.rs:435-455,482-503`,
  `glass-lint-core/src/api/compiler/normalize_all.rs:243-271`

`CanonicalArgumentConstraints` documents ordered, deduplicated, non-empty
groups, and `from_canonicalized` says it panics when its precondition is
violated. The implementation neither checks nor canonicalizes that input:
empty input creates an empty value and duplicate or out-of-order constraints
are retained. Every current production caller sorts/deduplicates first, so the
invariant lives in scattered call-site protocols rather than in the type that
is consumed by physical validation and matcher evaluation.

**Recommendation:** Move sorting/deduplication into one fallible or total
constructor, or make the pre-canonicalized constructor private to a validated
builder and give it a name that makes the unchecked precondition explicit.
Preserve argument-index ordering, predicate ordering, bounded group sizes,
and the distinction between an intentionally empty set and a non-empty
canonical group; update `compile_argument_constraints` and normalization to use
the single owner and delete their repeated preconditioning.

**Fix Applied:** Replaced the pre-canonicalized constructor with
`CanonicalArgumentConstraints::from_constraints`, which sorts and deduplicates
inputs before grouping them. Normalization and physical compilation now use
that single owner, while empty input remains an explicit unconstrained set and
each retained group is non-empty.

**Verification:** `cargo test -p glass-lint-core api::compiler::tests --lib`
(144 passed) and `make fmt && make ci` (passed).

### Plan capability requirements

#### [x] READ-087 — Centralize project-requirement capability classification

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Architecture / Preparation policy
- **Location:** `glass-lint-core/src/api/compiler/requirements.rs:53-180`

`PlanRequirements` stores a `BTreeSet<ProjectRequirement>`, but three separate
methods hand-code overlapping classifications over that set:
`needs_module_identities`, `needs_call_result_identities`, and
`needs_project_overlay`. The same `ProjectRequirement` variant appears in
multiple match lists with intentionally different meanings, so adding a new
project capability requires synchronized edits across compiler and projection
preparation policy; omission can make the runtime skip required linking work
while the plan still reports the requirement.

**Recommendation:** Put capability predicates on `ProjectRequirement` (or
derive one aggregate preparation object while requirements are inserted), then
have `PlanRequirements` delegate to that owner. Preserve the distinction
between module identities, call-result identities, and the broader overlay
need; remove the repeated variant lists after the capability taxonomy has one
authoritative owner.

**Fix Applied:** Added capability predicates to `ProjectRequirement` for
module identities, call-result identities, and project overlays. The
`PlanRequirements` queries now delegate to those predicates while preserving
the separate value-resolution call-result requirement.

**Verification:** `cargo test -p glass-lint-core
api::compiler::tests::normalize::algebra --lib` (23 passed) and
`make fmt && make ci` (passed).

### Lifecycle physical lowering

#### [x] READ-088 — Reject impossible lifecycle sources instead of lowering an empty target

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Conversion / Fail-closed behavior / Compiler invariant
- **Location:** `glass-lint-core/src/api/compiler/object_flow.rs:138-158`,
  validation in `glass-lint-core/src/api/compiler/validate/pass4_10.rs:12-31`

`validate_lifecycle` guarantees that every lifecycle source is either a global
call or a rooted member call, but `CompiledObjectSource::from_normalized_event`
still has a wildcard branch that converts every other event/identity pair into
`LifecycleCallTarget::RootedMember(SymbolPath::default())`. If a new source
shape bypasses validation or the validation/lowering order changes, the
compiler will produce an empty rooted target that can match the wrong semantic
path instead of preserving the invalid-state error. The fallback also hides
the exact invariant that the validator is supposed to establish.

**Recommendation:** Represent the conversion as a checked `TryFrom`-style
operation returning the existing invalid-lifecycle error, or use an explicit
unreachable invariant boundary after a single typed validator owns the
restriction. Preserve global versus rooted identity, argument constraints, and
fail-closed unsupported-source behavior; delete the empty-path fallback once
the lowerer cannot silently fabricate a target.

**Fix Applied:** Made normalized lifecycle-source lowering return `None` for
unsupported event/identity pairs instead of fabricating an empty rooted path.
The physical plan then reaches its existing validation boundary and fails
closed; validated global and rooted-member sources retain their behavior.

**Verification:** `cargo test -p glass-lint-core
api::compiler::tests::physical --lib` (32 passed) and `make fmt && make ci`
(passed).

### Physical plan boundary

#### [ ] READ-089 — Construct only validated physical plans

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Phase transition / Invariant validation
- **Location:** `glass-lint-core/src/api/compiler/physical.rs:50-192,414-424`,
  call sites in `glass-lint-core/src/api/compiler/mod.rs:217-244`

`PhysicalPlan::new` returns a plan directly from arbitrary roots and
`PlanRequirements`; validation is a separate operation that production calls
after assembling all query roots. The value therefore has no type-level or
constructor guarantee that roots are executable, canonical, non-empty where
required, or consistent with preparation requirements. The same raw plan can
be held and inspected before validation, and a future compiler caller can
forget the check while still passing a `PhysicalPlan` to runtime-facing code.

**Recommendation:** Make the construction transition fallible and return a
validated plan type, or keep an unvalidated builder private and expose only a
validated `PhysicalPlan` after root/requirement checks. Preserve deterministic
root optimization, the explicit `RequirementsMismatch` diagnostics used by
tests, and the ability to validate deliberately malformed test fixtures; move
the production call path to one checked owner and remove the unchecked plan
construction from normal compilation.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 17’s compiler pipeline already separates declaration validation,
canonical normalization, and physical execution well. The main readability
cost is that several types are “validated by convention”: table capacities,
canonical argument groups, requirement categories, lifecycle source shapes,
and physical-plan validity are all established by neighboring callers or
debug-only checks. Giving each phase a consuming validated transition would
make unsupported states explicit and reduce duplicated caller protocols while
keeping the reference evaluator independent for differential testing.

No findings are marked applied.

## Open Questions

- The evidence table may intentionally support partial selections, but its
  capacity should still be tied to the same catalog identity as every
  `RuleIndex`; if cross-catalog evidence is ever supported, it needs an
  explicit remapping step.
- `CanonicalArgumentConstraints` may allow an empty value internally for
  intermediate construction; if so, split that intermediate type from the
  canonical non-empty physical constraint rather than weakening the current
  documentation.
- `PhysicalPlan` is crate-private today, but the compiler/reference tests and
  neighboring analysis modules use it as a phase value; a validated wrapper
  should not prevent tests from constructing malformed fixtures through a
  clearly named test-only path.
- The next unreviewed handoff is Chunk 18: remaining analysis and project
  model types listed in `CODEBASE_STRUCTURE_CORE.md`.

## Coverage

Reviewed the Chunk 17 modules listed in `CODEBASE_STRUCTURE_CORE.md` across
classification evidence, rule indexes, compiled matcher plans, normalized
query IR, argument-constraint groups, lifecycle object-flow plans, physical
roots/plans, requirement sets, compiled rule records, and physical validation
errors. Representative callers were traced through local projection,
cross-flow evidence collection, normalization, physical planning, runtime
preparation, and reference-plan evaluation. Earlier compiler catalog and
selection findings READ-076 and READ-077, and the contradiction finding
READ-074, were checked to avoid duplicating their root causes. No source,
test, configuration, dependency, or documentation changes were made.
