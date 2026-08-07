# Codebase Readability Audit — Chunk 6

## Summary

Chunk 6 owns syntax-directed naming and bounded constant evaluation, qualified
evidence traces, and the internal value identity/arena façade. The core
invariants are mostly explicit: unsupported syntax becomes `Unknown`, constant
work is bounded, trace handles reject foreign arenas, and value IDs fail
closed on exhaustion. The main readability and API risks are an unfinished
value-model migration, duplicate global-object matching policy, split constant
conversion/evaluation ownership, a public trace-storage boundary, and raw
value-arena construction that lets callers assemble representation-level
values directly.

## Findings

### Value-model migration

#### [ ] READ-026 — Remove the obsolete `analysis::value` type façade

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/value/arena.rs:1-7`; `value/identity.rs:1-12`; `value/mod.rs:18-27`; representative callers in `analysis/facts/interface/mod.rs:77`, `analysis/project/identities.rs:29`, and `analysis/matching/build.rs:52-82`

Both value submodules state that their types moved to
`analysis::model::value` or `analysis::model::scope`, but production code
continues to import `ValueTable`, `Value`, `ValueId`, `BindingKey`, and related
types through `analysis::value`. This leaves a compatibility façade that looks
like the owner of retained value identities while the actual model owns their
definitions, and it makes a future type migration appear to require preserving
two internal paths.

**Recommendation:** Migrate callers to the owning model modules, retain only a
narrow value-operation module if an operation still has a genuine value-layer
owner, and delete `value/arena.rs` plus the moved-type reexports from
`value/identity.rs` and `value/mod.rs`. Keep artifact-local ID opacity,
`ValueId::UNKNOWN`, bounded arena exhaustion, and the provider-neutral
identity-comparison behavior while removing the duplicate namespace.

### Global-object identity

#### [ ] READ-027 — Make one owner define global-object path equivalence

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/environment.rs:262-317`; duplicated adapter in `glass-lint-core/src/analysis/value/identity.rs:16-83`; callers in `glass-lint-core/src/api/compiler/mod.rs:129-136` and `glass-lint-core/src/analysis/matching/query/view.rs:195-219`

`Environment` already owns global-object membership, promotion, alias
equivalence, and `SymbolPath` comparison. The `NamePath` helper
`matches_global_object_alias_with` then reimplements the same three matching
cases—equal paths, aliased roots with equal tails, and one-segment promoted
members—by resolving IDs through a `NameTable`. The two representations are
legitimate (artifact-local `NameId` paths must not be converted through an
unrelated table), but the identity policy is duplicated and can drift when a
new realm or promotion rule is added.

**Recommendation:** Put both path-view operations behind one environment-owned
comparison contract, such as a checked `NamePath` view that accepts its
artifact `NameTable`, and delete the free `matches_global_object_alias_with`
algorithm after callers migrate. Keep the existing `SymbolPath` fast path,
same-table ID validation, restricted foreign-realm behavior, promoted-member
rules, and exact-tail matching; do not turn artifact-local IDs into globally
comparable integers.

### Constant projection boundary

#### [ ] READ-028 — Centralize constant conversion and recursive evaluation policy

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/syntax/constant/eval.rs:14-57,101-190`; adapters in `analysis/scope/build/constants.rs:15-44`, `analysis/scope/query/constants.rs:24-48`, and `analysis/resolution/mod.rs:157-176`; reverse projection in `analysis/resolution/constant.rs:20-83`

The syntax evaluator defines `ConstValue`, recursion/node/lookup bounds, and
default member/spread recursion, but each semantic owner supplies a separate
`Lookup` adapter. The scope collector and frozen graph repeat provenance-to-
constant conversion and mutable-spread rejection, while `Resolver::const_value`
and `intern_const_value` independently convert between the value arena and the
same `ConstValue` tree with a separate depth-only bound. Adding a constant
variant, changing an unknown rule, or tightening a container bound therefore
requires synchronized edits across evaluator callbacks, scope adapters, and
arena conversion code.

**Recommendation:** Make the constant domain own canonical bounded conversion:
keep lexical/member resolution callbacks as inputs, but let one
evaluator/context own recursive admission, container limits, property-key
conversion, and the `ConstValue` ↔ arena projection rules. Delete the repeated
scope adapter conversion and resolver mapping branches after migration, or
make them thin policy callbacks rather than parallel tree transforms. Preserve
unknown-on-unsupported behavior, shadowing and mutable-object rejection,
lookup/depth/node/string/container bounds, deterministic object keys, and the
fact that a complete static witness remains distinct from an exhausted one.

### Trace storage boundary

#### [ ] READ-029 — Keep trace arena storage and handles out of the public result API

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/trace.rs:13-157`; public exposure in `analysis/project/model.rs:494-496` and `api/classification.rs:9,76-84`; report callers in `lint/report/evidence.rs:280-303`

`TraceArena::intern`, `node_count`, `is_exhausted`, and
`reconstruct_trace` expose the arena as a general storage API, while
`ProjectSemanticModel::trace_arena` returns that storage publicly and
`ClassificationEvidenceOccurrence` publicly carries `TraceNodeId`. Production
code only needs a private chain-interning and report-reconstruction boundary;
the current surface leaks internal correlation handles, fact/module identity
representation, and arena accounting to callers that should consume finalized
evidence. It also allows public callers to attempt low-level parent insertion,
including foreign-arena handles, even though the arena is an analysis
implementation detail.

**Recommendation:** Make `TraceArena` and `TraceNodeId` crate-private to the
analysis/report integration, make low-level `intern` private, and have report
assembly request a validated reconstructed trace or a private evidence view.
Remove the public `trace_arena` accessor and trace-head field from the public
classification contract after callers migrate. Preserve foreign-handle
rejection, deterministic source-to-sink order, interning, exhaustion metrics,
and the rule that exhausted or invalid traces cannot become definite evidence.

### Value arena construction

#### [ ] READ-030 — Hide raw value variants behind semantic arena operations

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/value.rs:103-221`; façade exports in `analysis/value/arena.rs:5-7`; representative construction in `analysis/resolution/call.rs:90-107`, `analysis/facts/arguments.rs:221-230`, `analysis/facts/calls/wrapper.rs:95`, and `analysis/matching/build.rs:80-82`

`ValueTable::intern` accepts the full representation-level `Value` enum, and
callers construct `Value::Global`, `Value::ModuleExport`, `Value::Callable`,
`Value::RootedMember`, and static values directly. The table separately exposes
`intern_with_binding`, `intern_static_object`, and `allocate_object_id`, so
callers must know when to wrap a target, how a terminal cache is maintained,
which name table belongs to the artifact, and how `UNKNOWN` plus exhaustion
should propagate. This spreads one value-arena invariant across resolution,
fact construction, and matching instead of making the arena or resolver the
semantic owner.

**Recommendation:** Keep raw enum interning private and expose named operations
for the supported identity families—binding-wrapped values, globals/module
exports, rooted members, constants, callables, and fresh objects—with validated
inputs and one exhaustion result policy. Move direct `Value` construction out
of fact and matching callers, then delete redundant wrapper paths such as
`intern_with_binding` once the semantic operations own them. Preserve terminal
identity canonicalization, artifact-local names, bounded object/value IDs,
unknown propagation, and deterministic equality/interning.

## Systemic Themes

- The value layer is in a transitional state: retained model types are already
  centralized, but compatibility imports and raw construction still make the
  old boundary appear authoritative.
- Environment policy, syntax evaluation, and arena storage each have sound
  local invariants; the main risk is that adapters and callers duplicate those
  invariants rather than invoking one owner.
- Trace and value IDs are intentionally opaque and artifact-local. Any API
  cleanup must preserve that opacity and keep incomplete, exhausted, foreign,
  or unsupported results from becoming successful strict witnesses.

## Open Questions

- Confirm whether `TraceNodeId` must remain public for any downstream API user;
  the in-repository callers only use it to connect classification to report
  assembly, which can be replaced by an internal evidence handle or finalized
  trace.
- Decide whether the canonical global-object comparator should live on
  `Environment` or on a checked path-view type; the latter must retain the
  artifact `NameTable` in its ownership boundary.
- Determine whether arena exhaustion should remain a sticky table state or be
  returned as a typed result from each semantic construction operation before
  hiding raw `ValueTable::intern`.

## Coverage

Reviewed all modules listed in Chunk 6 of `CODEBASE_STRUCTURE_CORE.md`:

- `analysis::syntax`, `analysis::syntax::constant`,
  `analysis::syntax::constant::eval`, `analysis::syntax::constant::types`,
  `analysis::syntax::name`, `analysis::syntax::names`,
  `analysis::syntax::provenance`, `analysis::trace`, `analysis::value`,
  `analysis::value::arena`, `analysis::value::identity`.

Representative callers in scope construction, frozen scope queries, resolver
constant projection, fact lowering, matching indexes, project report
assembly, and public classification APIs were traced. The bounded evaluator,
foreign-arena checks, artifact-local name identity, and value/trace exhaustion
paths were inspected. No source changes or fixes were applied.
