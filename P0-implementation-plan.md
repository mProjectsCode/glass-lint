# P0 implementation plan — semantic fact architecture

This plan implements only P0 items 1–4 from `plan.md`. It intentionally does
not remove `glass-lint-core/benches/core.rs`: `plan.md` makes that removal part
of P2.10, after the harness folder runner exists.

The four items are one architectural migration, but they should land as a
sequence of reviewable commits. Every commit must compile and keep the existing
strict-matching regressions green. Temporary adapters are allowed only within
the branch; the final P0 commit removes every downstream `Program` consumer.

## Implementation review status — 2026-07-12

This status is against the current working tree, including the in-progress
semantic migration. The migration is not P0-complete yet.

### Landed in the current implementation

- `CompiledCatalog` is constructed at `Linter` construction and the normal
  lint path no longer normalizes matchers on every file.
- `FactStream`/`FactBuilder` exist, carry typed payloads and deterministic
  `FactId`s, and replace the old `EventLog`, call visitors, constructor
  visitors, and value-fact collector.
- The occurrence index, argument predicates, function summaries, and object
  flow now consume the fact stream rather than accepting `Program`.
- Function summaries are keyed by `FunctionId`; object flow has identity-based
  state, definite branch joins, basic loop/switch/try handling, separate
  emission keys, and focused regressions.
- `cargo fmt --all -- --check`, `cargo test --workspace`, and
  `cargo clippy --workspace --all-targets -- -D warnings` pass. The e2e
  harness also passes: 11/11 cases.

### Remaining work before the P0 exit criteria can be claimed

1. **Finish catalog compilation and selection.** `ApiRule` still stores only
   normalized `Matcher`s, so `CompiledCatalog::from_rules` clones and
   normalizes them again. Store the compiled form in catalog state, pass stable
   enabled slots into semantic evaluation, and add tests for reversed catalog
   order, subset selection, duplicate normalization, and disabled rules not
   adding per-file matcher work.

2. **Carry fact identity all the way to evidence.** `MatcherFacts` and flow
   evidence still retain spans instead of originating `FactId`s. Final
   normalization calls `FactStream::order_for_span`, which maps equal spans to
   the earliest fact and therefore can misidentify overlapping call/member
   roles. Replace this compatibility fallback with event-bearing occurrences;
   retain only an explicitly indexed synthetic-span fallback. Add the missing
   equal-span, synthetic-span, and 10,000+ dense-fact correctness tests.

3. **Tighten the fact-stream invariant.** The stream currently emits standalone
   `StringLiteral` facts even though the target architecture projects static
   constants from value facts. Remove that parallel representation (or record
   and document a deliberate exception). Add catalog A/A+B/B+A/no-rule
   fingerprint tests, canonical evaluation-order tests, and a structural test
   that downstream index/summary/flow modules neither visit SWC nor accept
   `Program`.

4. **Complete the lexical/summary split.** Callback binding and protocol
   discovery still live in `scope/collector.rs` and `scope/collector/callbacks.rs`;
   move them into fact/summary construction. Expand `FunctionSummary` beyond
   parameters and sinks to include calls, writes, returns, and scoped
   invalidation. Join all compatible and incompatible invocation contexts by
   `FunctionId`, model closure/version capture and reassignment explicitly,
   and add the remaining recursive, alias, destructuring, spread, and local
   lookalike adversarial tests.

5. **Finish conservative object-flow transfer.** `FlowState` still reports
   `source_span` for every emission and does not retain explicit
   source/configuration/sink sites. Model `finally` for normal, exceptional,
   `break`, `continue`, and `return` exits; make switch fallthrough and loop
   transfer/update semantics explicit; and complete identity-aware
   invalidation for destructuring, computed/optional writes, sequences,
   deletes, updates, compound assignments, and helper-mediated writes.
   Add location assertions and multi-sink/source-site tests.

6. **Restore provider fixture conformance.** `make test-rules` currently has
   one JavaScript failure (`network/script_injection/negative`) and four
   Obsidian failures (`ui/modal` positive/negative, `ui/notice` negative, and
   `ui/settings_tab` negative). These regressions must be diagnosed and fixed;
   the P0 verification gate is not green until both provider fixture suites
   pass.

7. **Remove the remaining compatibility surface.** Delete
   `FactStream::order_for_span` and any span-only downstream adapters after
   event-bearing evidence is in place, then re-run the structural/source audit
   for obsolete visitors, matcher-dependent fact construction, and
   downstream AST access.

## Target architecture and invariants

Per file, analysis has two construction stages:

1. A lexical construction prepass builds `ScopeGraph`, declaration versions,
   stable `BindingId`/`BindingVersion`/`FunctionId` ownership, and the source
   information needed to resolve later occurrences. This remains the only
   scope-building walk.
2. One source-ordered semantic walk emits an immutable `FactStream`. It interns
   values and objects and records resolved occurrences, calls, writes,
   function/control boundaries, and their stable `EventId`s.

Everything after those stages consumes the `FactStream`: occurrence indexes,
argument predicates, function summaries, object-flow transfer, and evidence
normalization. These consumers receive no `Program` and do not invoke SWC
visitors.

The fact stream is rule-independent. Adding, removing, or reordering matchers
must not change its bytes, IDs, limits, or resolution decisions. Compiled
matcher plans are catalog state and are applied only after the stream exists.
Strict queries continue to require lexical identity and provenance; unknown or
ambiguous facts fail closed.

## Phase 0 — characterization and internal test access (partially complete)

Before changing construction, add focused internal tests beside the private
semantic modules. Do not create an integration test that depends on
`pub(super)` types merely to inspect implementation details.

- `facts.rs`/`fact_builder.rs` now cover nested call/member roles and
  duplicate same-span kinds; `summary.rs` covers same-name sibling functions;
  and `object_flow.rs` has the initial branch/loop/switch/try regressions.
- Strengthen the existing enabled-rule-order test so it actually uses reversed
  catalog/enabled order. Add the stronger A/A+B/B+A/no-rule fact-stream
  fingerprint assertion.
- Add the missing indexed lookup tests listed in Phase 2.2, especially the
  dense 10,000+ fact case. There is no longer an `events.rs`; the replacement
  tests belong beside `FactStream`.

Do not use wall-clock assertions to prove lookup complexity. Use an indexed
representation whose implementation is testable, plus a large deterministic
correctness test.

## Phase 1 — compile the catalog once (partially complete)

### 1.1 Introduce `CompiledCatalog`

Files: `matcher/rule/mod.rs` (or a focused new `rule/compiled.rs`),
`matcher/mod.rs`, `linter.rs`.

Create a private compiled catalog with one normalized `ApiMatcher` per catalog
rule in catalog order. Preserve a stable rule-slot mapping; filtering enabled
rules must not rely on two independently constructed vectors remaining aligned.

```rust
struct CompiledRule {
    catalog_index: usize,
    matcher: ApiMatcher,
}

struct CompiledCatalog {
    rules: Vec<CompiledRule>,
}
```

`Linter::new` and `Linter::with_rules` build this once from `RuleCatalog`.
`Linter::lint` derives enabled compiled-rule references/slots without cloning
`ApiRule` or normalizing its matchers. Keep `ApiRule` metadata in
`RuleCatalog` for `ApiCapability` and report construction.

Change the private semantic entry point to accept compiled matchers plus stable
rule slots. If the public `classify_api_usage(program, rules)` API remains, it
may compile for that one standalone call because it has no persistent catalog;
the normal `Linter` path must not. Prefer a private
`classify_compiled_api_usage` shared by `Linter` and the public wrapper.

Tests:

- enabling a subset maps evidence to the correct rule metadata;
- catalog order and enabled-set order do not change results;
- duplicate/equivalent matcher clauses normalize once;
- `Linter::lint` performs no matcher compilation (verify through a test-only
  compilation counter or by constructing the compiled state explicitly).

## Phase 2 — canonical fact stream and bounded event lookup (partially complete)

### 2.1 Replace `EventLog` with facts that own their order

Files: `semantic/events.rs`, `semantic/facts.rs`, `semantic/value.rs`.

Keep `EventId`, but assign it exactly once when the final canonical stream is
ordered. Define one fact record:

```rust
struct SemanticFact {
    id: EventId,
    span: Span,
    scope: ScopeId,             // use the project's concrete scope ID type
    function: FunctionId,
    payload: FactPayload,
}

struct FactStream {
    facts: Vec<SemanticFact>,
    exact: BTreeMap<ExactEventKey, EventId>,
}

struct ExactEventKey {
    lo: BytePos,
    hi: BytePos,
    kind: FactKind,
    ordinal: u16,               // distinguishes canonical same-span facts
}
```

Use a checked wider ordinal if `u16` cannot be proven sufficient under the
global fact budget. IDs and ordinals must fail closed rather than wrap.

`FactPayload` must be compact and typed. It must not contain borrowed AST
nodes, formatted strings used as identity, or matcher/rule indexes. Add only
payloads required by current P0 consumers:

- declaration/version and binding invalidation facts;
- identifier/member references with `ValueId`, `BindingKey` where known,
  typed provenance, and an optional `SymbolPath` display projection;
- assignment and property-write facts with target identity, written value,
  static property segment when known, and invalidation classification;
- canonical call/construction facts with callable `ValueId`/provenance,
  receiver identity, effective arguments (including `bind`/`call`/`apply` and
  spread/unknown markers), optionality, and result `ValueId`;
- member-read facts;
- function declaration/entry/exit facts with `FunctionId`, parameter-pattern
  projections, owning function, and binding identity when named;
- branch, loop, switch/case, try/catch/finally, break, continue, and return
  boundaries sufficient to construct a bounded control-flow representation;
- class/import facts only where existing constructor/import queries need them.

Do not add standalone string/template facts: static constants belong in
`ValueId`/`ConstValue` and are projected from resolved argument/value facts.

### 2.2 Define exact and containment lookup separately

Every fact consumer should carry `EventId` directly. Exact compatibility
lookup, while migration adapters still exist, is keyed by `(lo, hi, kind)` and
returns all canonical IDs in deterministic ordinal order; it must not silently
choose among distinct same-span facts.

If a genuine enclosing query remains, implement it as a separate interval
index with the documented result order: smallest containing width, then `lo`,
`hi`, `FactKind`, and ordinal. A backwards linear scan from a binary-search
insertion point is not sublinear in nested input and is therefore not an
acceptable final implementation.

Evidence occurrences gain their fact order when created:

```rust
struct EvidenceOccurrence {
    event: Option<EventId>,
    span: Span,
    kind: ApiMatchKind,
    symbol: String,
}
```

`normalize_evidence` sorts these records and never queries spans against the
event log. Synthetic evidence uses a documented deterministic fallback key.

Tests:

- exact lookup distinguishes call and member facts with equal spans;
- duplicate/synthetic spans have deterministic ordinals;
- enclosing lookup chooses the documented innermost fact;
- 10,000+ dense facts return the same answers as a simple reference lookup;
- evidence sorting performs zero compatibility lookups.

## Phase 3 — one authoritative semantic fact walk (partially complete)

### 3.1 Split lexical construction from occurrence resolution

Files: `scope/collector.rs`, `scope/mod.rs`, `resolver.rs`, `facts.rs`.

Refactor `Resolver::collect` into:

- `ScopeGraph::collect(program)`, the lexical construction prepass; and
- a resolver created from the finished graph and a mutable/interior-mutable
  `ValueArena`, used only by the authoritative fact builder.

Remove `ValueFactCollector`. Its identifier/member resolution is performed at
the corresponding canonical fact emission point. Cache keys remain typed and
position-sensitive. The fact builder must resolve an occurrence once and store
the result in its payload; later consumers may not call back into the resolver
with AST nodes.

Audit the scope collector during this split. Callback aliases and function
invocations currently collected in `scope/collector.rs` are semantic facts,
not scope construction, and move to the fact/summary pipeline in Phase 5.
Declaration/version histories needed for position-sensitive resolution remain
in lexical construction.

### 3.2 Implement `FactBuilder`

`FactBuilder` is the only post-scope SWC visitor. Its visitor methods must
preserve JavaScript evaluation order, not merely sort final facts by
`(span.lo, span.hi)`. In particular, assignment RHS/LHS effects, call callee
and arguments, computed properties, loop test/update, and `finally` require an
explicit order contract. Record source span independently from evaluation
order.

At each relevant node it:

1. resolves identities, values, constants, and callable provenance;
2. interns values/objects using existing bounded arenas;
3. emits exactly one canonical fact for each semantic role;
4. emits the control/function boundaries needed by later transfer; and
5. assigns checked, deterministic `EventId`s.

It does **not** receive `ApiMatcher`, argument matcher slices, flow matchers, or
rule counts, and it does not populate `MatcherFacts` or evidence.

### 3.3 Establish the fact-stream invariant

Add test-only read access or a stable fingerprint to compare streams.

- A diverse program produces the expected unique semantic roles. Uniqueness
  is `(FactKind, canonical AST role, span)`, not just `(kind, span)`, because a
  node may legitimately produce several different facts.
- Building with catalogs A, A+B, B+A, and no enabled rules yields the identical
  fact fingerprint and identical IDs.
- Repeated builds yield identical facts.
- Nested optional calls and `bind`/`call`/`apply` do not double-record their
  callee/member roles.

## Phase 4 — fact-driven occurrence indexes and predicates (mostly complete)

Files: `facts.rs`, `calls.rs`, `call_arguments.rs`, `constructors.rs`,
`index.rs`, `semantic/mod.rs`.

Turn `calls.rs` from an AST visitor into a `FactStream` projector. Prefer a
focused `OccurrenceIndex::build(&FactStream)` that records all
rule-independent call, import, class/instance, member-read, and returned-member
relations. Instance facts must not be selected based on the active
`InstanceMemberCallMatcher`s.

Evaluate compiled matcher predicates after the index exists:

- ordinary matcher queries use the rule-independent index;
- call/member argument predicates read canonical effective-argument facts and
  `ConstValue` projections;
- constructor/instance queries read class and object relationships already in
  facts;
- evidence records carry the originating `EventId`.

Delete `ResolvedCallCollector`, `CallContext`, and all `Program` parameters
from calls, constructors, argument predicates, and occurrence indexing. Do not
retain an AST fallback.

Run all existing declarative, semantic, scope, and compact-source tests here.
Add a regression that adding an unrelated instance matcher neither changes the
index nor creates additional work during fact construction.

## Phase 5 — `FunctionId` summaries from facts (partially complete)

Files: `summary.rs`, `scope/collector.rs`, `scope/collector/callbacks.rs`,
`object_flow.rs`.

### 5.1 Replace name-keyed joins

Remove `FunctionDeclarations` and `FunctionInvocations` keyed by
`(scope, String)`. Introduce:

```rust
struct FunctionSummary {
    id: FunctionId,
    owner: FunctionId,
    parameters: Vec<ParameterPattern>,
    calls: Vec<CallProjection>,
    sinks: Vec<FunctionSinkSummary>,
    writes: Vec<PropertyWriteProjection>,
    returns: Vec<ReturnProjection>,
    invalid: SummaryInvalidation,
}

struct FunctionSummaries {
    by_id: BTreeMap<FunctionId, FunctionSummary>,
}
```

Names are optional display/diagnostic fields only. A callee joins to a summary
only when its callable value proves a `FunctionId`; arbitrary same-named
methods never participate.

If the current `FunctionId` is scope-derived rather than declaration-derived,
first document and test that every function declaration, expression, arrow,
method, and callback gets a unique stable ID. Fix that invariant before using
the ID as the sole key.

### 5.2 Build and join bounded summaries

Build summaries by streaming facts partitioned by owning `FunctionId`; never
search by source-range containment or rescan bodies. Project invocations by
parameter index and recursively through the supported destructuring patterns.
Represent omitted, extra, spread, dynamic, default, and rest arguments
explicitly.

Join each projected fact across all compatible invocations. A missing,
dynamic, conflicting, recursive, reassigned, or budget-exhausted input
invalidates only the affected parameter/projection, not unrelated summary
facts. Detect recursive SCCs by `FunctionId` and use a bounded fixed point or
conservatively invalidate recursive projections; never key recursion by name.

Captured values use the outer `BindingKey`/version visible at the capture/use
semantics defined by the resolver. Aliases to functions preserve the target
`FunctionId` until reassignment invalidates that value.

Move callback alias discovery out of the scope collector. Only model callback
protocols when receiver provenance proves a supported protocol; local methods
named `then`, `map`, or `forEach` remain unknown.

Required tests:

- mutually recursive helpers and a bounded termination assertion;
- function aliases and reassignment before invocation;
- closure capture across outer binding versions;
- object/array destructuring, defaults, and rest;
- spread, missing, extra, and dynamic arguments;
- sibling and nested same-name functions;
- local lookalike callback methods.

## Phase 6 — fact-driven conservative object flow (partially complete)

Files: `object_flow.rs`, `flow_state.rs`, `flow_index.rs`, `flow_calls.rs`.

### 6.1 Separate transfer state from emission

Replace the AST visitor with a bounded transfer engine over facts/control
regions. State contains object/alias identity and explicit evidence sites:

```rust
struct FlowState {
    flow: FlowId,
    object: ObjectId,
    source_event: EventId,
    requirements: BTreeMap<RequirementId, EventId>,
}

struct FlowEnvironment {
    states: BTreeMap<(ObjectId, FlowId), FlowState>,
    aliases: BTreeMap<BindingKey, ObjectId>,
}
```

Do not keep an `emitted` bit in joinable state. Emission is a separate set
keyed by `(rule slot, flow index, ObjectId, match EventId)`. The match event is
the sink for sink flows and the documented configuration event for
requirement-only flows. Evidence may include source/configuration/sink spans,
but the finding anchor must be specified and tested.

### 6.2 Define lattice operations correctly

Implement and unit-test:

- `snapshot`/`restore`;
- `kill_binding`, `kill_object`, and property-specific invalidation;
- `join_definite(paths)`: retain a state/alias only when the same identity is
  present on every reachable path; retain only requirements present on every
  path (set intersection, not union); and retain an alias only when every path
  maps the same `BindingKey` to the same `ObjectId`.

There is no special “baseline object.” Pre-branch states survive naturally if
unchanged on every path. Branch-local allocations and conflicting aliases do
not.

### 6.3 Model control constructs

Build explicit reachable exits for normal flow plus `break`, `continue`,
`return`, and exceptional/unknown exits where required.

- `if`/conditional: evaluate the test once, transfer both reachable arms from
  that environment, then definite-join normal exits. A missing `else` is an
  unchanged arm.
- `while`/`for`/`for-in`/`for-of`: include the zero-iteration path, one
  transferred iteration, `continue` through update/test as applicable, and
  `break` exits. Use a bounded fixed point only if one iteration cannot express
  the supported facts. A `do-while` has no zero-body path.
- `switch`: evaluate the discriminant once; model possible entry at every
  matching case/default, fallthrough, `break`, and the no-match path when no
  default exists. Do not analyze every case independently from baseline.
- `try`: join normal try completion with reachable catch completion for the
  modeled exceptional path. Apply `finally` independently to every incoming
  exit kind, because it executes on normal, exceptional, break/continue, and
  return exits and may replace them.
- unsupported dynamic effects kill only identities reachable from affected
  values when that can be proven; otherwise invalidate the current function's
  flow environment, never another function's state.

### 6.4 Complete invalidation

Consume assignment/write facts for identifier and member targets, nested
destructuring, defaults/rest, computed properties, deletes, updates, compound
assignments, sequence expressions, optional-chain semantics supported by the
parser, and helper-mediated writes from `FunctionSummary`. Unknown computed
properties invalidate property-sensitive facts for the receiver object.

Required tests:

- source/configuration before an `if` survives both unchanged arms;
- identical configuration in both arms survives;
- one-arm configuration and conflicting aliases do not leak;
- zero-iteration and do-while behavior;
- `continue` update and `break` exits;
- switch fallthrough, default, and no-match paths;
- catch-only writes do not become definite after `try`, while sinks inside a
  catch can emit for a source reaching that catch;
- `finally` configuration/sink on every incoming path;
- two valid sinks for one source produce two match-site occurrences;
- source, configuration, and sink evidence locations are correct;
- destructuring, computed/optional writes, sequence expressions, and
  helper-mediated mutation invalidate only affected identities.

## Phase 7 — delete adapters and verify P0 exit criteria (incomplete)

Remove:

- `EventCollector` and span-containment guards from downstream analysis;
- `ValueFactCollector`;
- `ResolvedCallCollector` and `calls::collect(program, ...)`;
- `FunctionSummaryCollector`/`FunctionBodySummary` AST visitors;
- `ObjectFlowCollector` as an SWC visitor;
- name-keyed function declaration/invocation maps;
- all matcher-dependent inputs to fact construction;
- all downstream `Program` parameters and temporary AST adapters.

Keep the lexical `ScopeGraph` prepass and the single `FactBuilder` walk. Add a
structural test or narrow source check that the index, summary, and flow modules
do not import `swc_ecma_visit` or accept `Program`.

Verification after each commit, and once more at the end:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Also run the existing provider/harness suites exposed by the workspace. Do not
delete the core bench target during P0.

## P0 definition of done

- Matcher normalization is cached on `Linter`/catalog state.
- A lexical prepass plus one authoritative semantic fact walk supplies every
  downstream query; index, predicate, summary, and flow code receives no AST.
- Facts own stable `EventId`s; evidence never performs repeated span lookup;
  any retained exact/enclosing compatibility lookup is indexed and
  deterministic for equal/nested spans.
- The fact stream and rule-independent occurrence index are unchanged by the
  selected matcher catalog.
- Every function and invocation joins by `FunctionId`, with names used only
  for display.
- Definite baseline facts survive control-flow joins, while branch-local and
  ambiguous facts fail closed.
- Source/configuration/sink sites and multi-sink deduplication are explicit.
- Obsolete visitors and adapters are removed in the same P0 changeset.
- Formatting, workspace tests, provider/harness tests, and warnings-denied
  Clippy pass.
