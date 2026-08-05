# Codebase Readability Audit — Chunk 6

## Summary

Chunk 6 covers syntax-directed naming and provenance, bounded constant
evaluation, trace storage, and value identity helpers. The modules have good
provider-neutral boundaries: syntax names are explicitly not runtime proof,
constant evaluation has shared recursion/node/lookup limits, trace storage is
bounded and interned, and value identity keeps artifact-local handles opaque.

The main opportunities are at the boundaries between those helpers and their
consumers. Trace IDs are not tied to an arena even though they are exposed in
public evidence, trace-chain construction is duplicated in local and cross
flow, and property-name resolution has several similarly named APIs with
different support for contextual constants. The global-object alias matcher
also repeats a symmetric policy that already has an environment-level owner.

No source, test, configuration, dependency, or documentation changes were made
by this audit.

## Findings

### Trace ownership and evidence construction

#### [ ] READ-033 — Give trace-chain assembly one owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Architecture / bounded evidence
- **Location:** `glass-lint-core/src/analysis/trace.rs:60-130`,
  `analysis/flow/projector/evidence.rs:259-304`,
  `analysis/flow/cross/evidence.rs:220-274`

The local projector's `build_flow_trace` and cross-flow's `assemble_trace`
both manually intern a source, requirement events, prior correlated events,
and a sink through the same `TraceArena`. They independently decide how to
handle an empty requirement, which role prior sinks receive, and how arena
exhaustion aborts the trace. The storage owner only knows how to intern one
parented node, so the chain lifecycle remains duplicated in the two evidence
producers.

Give `TraceArena` or a private `TraceAssembler` a bounded chain operation that
accepts an ordered sequence of `(QualifiedEvent, EvidenceRole)` steps and
returns a head or an exhaustion result. Keep any local-versus-cross event
selection outside that assembler, then delete the repeated parent/tail and
`Option` handling. Preserve source-before-requirement-before-sink ordering,
the existing role assignment for correlated prior sinks, node interning,
trace-head counts, deterministic requirement order, and fail-closed behavior
when the trace limit is exhausted.

**Fix Applied:** None so far.

#### [x] READ-034 — Make `TraceNodeId` arena-local and validate trace parents

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Newtype / ownership
- **Location:** `glass-lint-core/src/analysis/trace.rs:8-10, 60-130`,
  `analysis/flow/cross/evidence.rs:222-252`,
  `api/classification.rs:1-70`

`TraceNodeId` is a public copyable `u32` wrapper and is re-exported through
classification evidence, but it carries no identity for the `TraceArena` that
created it. `TraceArena::intern` accepts a parent from any arena without
checking membership, and `reconstruct_trace` accepts a head from any arena
and silently stops when an ID is absent. A stale or cross-project trace can
therefore produce a truncated evidence chain, or—if numeric IDs overlap—walk
an unrelated node chain, while the API reports an ordinary vector rather than
an invalid/incomplete trace.

Keep trace handles behind an arena-owned query/assembler API, or add an
arena-generation/owner token and validate both parent and head before use.
Replace silent truncation with an explicit invalid/exhausted result at the
private boundary and adapt report assembly to preserve its fallback evidence.
Preserve public evidence serialization expectations, bounded allocation,
interning of repeated nodes, deterministic reconstruction, and the current
fail-closed fallback when a trace cannot be reconstructed.

**Fix Applied:** `TraceNodeId` now carries a stable owner token allocated by
its `TraceArena`. Foreign parents are rejected at interning time, and trace
reconstruction validates the head and every parent, returning `None` instead
of silently truncating an invalid chain. Report assembly continues to use its
existing fallback evidence path; focused coverage exercises foreign handles.

**Verification:** `cargo test -p glass-lint-core --lib analysis::trace`
and `make fmt && make ci` pass.

### Syntax and bounded evaluation APIs

#### [ ] READ-035 — Make contextual and syntax-only property names distinct

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Duplication / semantic contract
- **Location:** `glass-lint-core/src/analysis/syntax/names.rs:88-103, 170-217`,
  `analysis/syntax/constant/eval.rs:12-100, 310-330`,
  `analysis/scope/query/provenance/callable.rs:115-126`

The syntax layer exposes `property_name` and `member_property_name` for
literal/structural shapes, while the constant evaluator has another
`property_name`/`member_property_name` pair that shares an `EvalState` and can
resolve contextual static values. The scope query then adds a third method
with the same member-name concept and delegates to the contextual evaluator.
The contracts differ intentionally—syntax-only helpers reject most computed
expressions, while contextual evaluation may use a proven binding—but the
names do not express that distinction. Callers across facts, scope, and
resolution can therefore choose a helper that has the wrong proof strength.

Define explicit names or small typed operations for syntax-literal property
names versus context-resolved static property names, and centralize their
shared direct-member cases. Delete ambiguous wrapper names after migration.
Preserve rejection of dynamic/computed names without proof, support for
literal numeric/string keys, shared evaluation budgets for contextual keys,
lexical shadowing checks, and the rule that a structural spelling alone never
establishes runtime identity.

**Fix Applied:** None so far.

### Value identity helpers

#### [ ] READ-036 — Centralize symmetric global-object alias matching

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication / API / identity policy
- **Location:** `glass-lint-core/src/analysis/value/identity.rs:14-80`,
  `environment.rs:263-296`,
  `analysis/matching/query/view.rs:19, 220-236`

`matches_global_object_alias_with` repeats the global-object alias policy for
`NamePath` values, including one branch where the expected root is the global
object and the reverse branch where the found root is the global object. The
environment already owns the corresponding configured-root/path policy for
`SymbolPath`, and the `NamePath` version repeats the member-promotion and
tail-equality rules around name-table conversion. The two representations can
drift in alias direction, configured global-object handling, or member
promotion even though they answer the same identity question.

Move the shared alias/promotion predicate behind an environment-owned helper
that accepts resolved root/member views, and keep only `NamePath` conversion
and fail-closed name-table lookup in the adapter. Delete the mirrored expected
/found branches after migration. Preserve exact-path equality, bidirectional
configured aliases, promoted global members, identical tails, missing-name
rejection, and deterministic matching for both name and symbol paths.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 6 has clear low-level owners, but several APIs expose representation
choices rather than semantic contracts: arena-free trace IDs, similarly named
property-name operations, and a name-table-specific copy of environment alias
matching. The safest refactors should make those contracts explicit while
keeping syntax, contextual evaluation, local value identity, and project trace
storage separate.

Bounded behavior is consistently fail-closed, but the failure result must stay
visible at each boundary. In particular, a trace that cannot be reconstructed
must not look like a valid shorter trace, and contextual property evaluation
must not be accidentally replaced with syntax-only spelling. The bounded
ValueTable intern/terminal-cache behavior was reviewed as part of this chunk
but is covered by the retained-model audit in Chunk 4 rather than repeated.

Search signals used for this chunk included public IDs without owner tokens,
parented arena APIs with no membership validation, duplicated trace assembly,
same-named helpers with different proof strengths, and mirrored environment
identity predicates.

## Open Questions

- Trace IDs are currently used as report-facing references into one project’s
  trace arena; the desired public lifetime may determine whether an owner token
  or an arena-scoped resolver is the smaller API.
- The property-name API should state whether contextual identifiers are allowed
  before callers are migrated, because that choice affects computed member
  matching and static object evaluation.
- The next unreviewed handoff is Chunk 7: fact and cross-flow types.

## Coverage

Reviewed every source file listed for Chunk 6 in `CODEBASE_STRUCTURE_CORE.md`:
the syntax root, constant evaluator/types/tests, name and names helpers,
syntax provenance, trace arena, value root/arena/identity modules, and their
representative callers in scope, facts, resolution, flow, matching, public
classification, and report evidence. Existing Chunk 1–5 findings were checked
to avoid re-reporting fact construction, scope-query, project overlay,
evidence-normalization, or retained ValueTable findings. No findings are
marked applied.
