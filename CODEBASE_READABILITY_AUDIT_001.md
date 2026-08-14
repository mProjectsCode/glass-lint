# Codebase Readability Audit

## Summary

This audit covers Chunk 1 (Source fact construction) from
`CODEBASE_STRUCTURE_CORE.md`. The fact frontend has a sound single-pass
boundary and useful phase separation, but path-local provenance, argument
projection, function-parameter transport, and call-event representation still
leak mechanics across their owners. The findings below are limited to concrete
deletions or consolidations; the `ModuleInterfaceBuilder` and phase-typed
`FactStream` wrappers were reviewed and retained as justified ownership
boundaries.

## Findings

### Fact provenance and control-flow checkpoints

#### [ ] READ-001 — Make all path-local provenance transactional

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:79-223`; `glass-lint-core/src/analysis/facts/control.rs:112-172,195-211`; `glass-lint-core/src/analysis/facts/assignments.rs:35-57,104-131`

`FactProvenanceState::checkpoint` and `OriginChannels` checkpoint only the
instance/class `OriginMap`s. `replace_targets` also mutates
`instance_callables` and `static_string_origins`, but those maps have no
rollback or branch snapshot even though the control helpers restore and merge
state around branches, loops, switches, and exception paths. A branch-local
reassignment can therefore leave callable or static-string provenance in the
state used after the join, violating the path-local identity contract and
making the state owner misleadingly appear transactional.

**Recommendation:** Make one provenance-state owner capture and restore every
path-local channel, including callable and static-string origins, and have
target replacement use that owner exclusively. Hide the raw `OriginMap`
checkpoint operations behind a typed combined checkpoint (or an equivalent
transaction object) so an instance checkpoint cannot accidentally be applied
to the class channel. Preserve the existing conservative intersection rules,
budget charges, and the rule that an independent complete witness survives an
incomplete alternative.

**Fix Applied:** None so far.

### Call argument projection

#### [ ] READ-002 — Remove discarded path state from argument-shape analysis

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/arguments.rs:29-75,125-217`

`arg_info` first visits the argument through the canonical visitor and then
`arg_info_projection` recursively walks object/array expressions again through
`analyze_argument_tree`. That helper returns `(ValueId, ValueId, PathId)`, but
object and array roots always return the same value for both value slots and
the root path, while recursive callers at lines 159-160 and 183-185 discard
the child base value and path. The extra path interning and tuple plumbing are
work and state with no consumer for nested object/array values, and the two
projection paths make future argument semantics harder to keep consistent.

**Recommendation:** Give argument-shape construction one narrow result owner:
the object/array tree should return only the retained value identity, while
member arguments should use the separate member-chain projection that actually
consumes `(base_value, base_path)`. Prefer folding the shape result into the
existing argument visit where practical so the same subtree is not traversed
twice. Preserve static object/array child identities, deterministic path
limits where paths are genuinely used, and fail-closed handling of spreads,
computed keys, dynamic values, and unsupported shapes.

**Fix Applied:** None so far.

### Function boundary construction

#### [ ] READ-003 — Borrow parameter patterns instead of cloning an immediate vector

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/functions.rs:76-102,111-143,146-179`

`record_function`, `record_arrow`, and `record_class_method` clone each AST
parameter pattern into a `Vec<(usize, Pat)>`. `record_function_body` immediately
passes that vector to `emit_function_fact` on entry, where the patterns are
walked synchronously, and then calls the same method with an empty iterator for
exit. The owned vector and cloned SWC subtrees do not establish a lifetime,
ownership, or validation boundary; they only make the parameter transport more
expensive and obscure that parameters are needed only while emitting `Enter`.

**Recommendation:** Change the boundary to consume a borrowed parameter slice
or an iterator of `(index, &Pat)` and pass the existing AST patterns directly.
Delete the three clone/collect pipelines and the exit-side empty parameter
transport, while retaining parameter order, destructuring paths, defaults,
rest markers, and the distinct `FunctionBodyKind` handling for lexical
function/class context.

**Fix Applied:** None so far.

### Call fact model and producers

#### [ ] READ-004 — Encapsulate the multi-field call event contract

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/fact.rs:424-470`; `glass-lint-core/src/analysis/facts/calls/mod.rs:14-65,86-115`; representative consumers in `glass-lint-core/src/analysis/flow/effect/mod.rs:171-230,356-372`, `glass-lint-core/src/analysis/flow/projector/mod.rs:754-779`, and `glass-lint-core/src/analysis/flow/summary/summaries.rs:312-350`

`FactPayload::Call` is a retained storage-shaped record with roughly fifteen
fields, many of them optional (`receiver`, provenance variants, paths, module
and return-member data, target function, and wrapper state). Production code
constructs the full record in more than one path and downstream phases
destructure its representation directly. The shape permits combinations that
are not meaningful for a given call kind and makes every new call property a
fan-out change across producers, tests, and consumers rather than a change to
one semantic owner.

**Recommendation:** Introduce a private call-event domain type owned by the
fact model, with constructors for resolved, unknown/dynamic, and wrapper calls
and narrow accessors for effect, matching, and projection consumers. Consolidate
the duplicate `FactPayload::Call` literals into those constructors, keeping
`SemanticFact` responsible for source span/function ownership and keeping
`CallUnwrap` explicit where wrapper lifecycle data is actually present. Do not
collapse distinct uncertainty states: unknown callee, unsupported provenance,
missing receiver, and absent wrapper must remain distinguishable and all
existing fail-closed matching behavior must be preserved.

**Fix Applied:** None so far.

## Systemic Themes

- Path-local identity is strongest when all related channels share one owner
  and one transaction boundary; parallel raw maps invite semantic drift.
- Fact construction should retain only representations that later phases
  consume. Discarded paths, cloned AST patterns, and repeated payload literals
  are avoidable work and maintenance surface.
- Internal fact payloads are an API between analysis phases. Constructors and
  domain accessors should own invariants instead of exposing a large optional
  record to every consumer.

## Review Resolutions

- READ-001 is a real ownership gap: `instance_callables` and
  `static_string_origins` are mutated by target replacement and must follow
  the same branch/loop/switch/exception transaction as the origin maps. The
  refactor must add those cases to the existing control-flow tests.
- READ-002 has no current consumer for nested object/array base paths; retain
  paths only for member-chain arguments. If a future matcher needs nested
  paths, add that semantic requirement explicitly rather than preserving
  unused tuple fields today.

## Coverage

Reviewed the complete Chunk 1 inventory: fact orchestration and immutable
artifact assembly; provenance and origin-map state; stream phases and issues;
visitor dispatch; calls, callees, and wrappers; arguments; assignments;
control regions; functions and classes; patterns; instances; call results;
module-interface collection; and the retained fact model. Representative
callers in flow, matching, and project summaries were traced where the Chunk 1
types form an inter-phase API. Search signals used included checkpoint/
rollback APIs, repeated `FactPayload::Call {` construction/destructuring,
`analyze_argument_tree`, and cloned parameter-pattern collection.
