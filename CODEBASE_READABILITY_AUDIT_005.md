# Codebase Readability Audit — Chunk 05

## Summary

Chunk 05 owns the local-artifact cache, the parser-to-semantic-artifact
boundary, bounded semantic work, and scoped completeness diagnostics. The
separation between reusable semantic state and path-local source context is
sound, as are the consuming freeze transitions and fail-closed capability
flags. The findings below target phase bundles that only forward data,
avoidable wrapper copies, and two resource/diagnostic APIs whose ownership is
less explicit than the surrounding architecture.

## Findings

### Semantic phase transitions

#### [x] READ-020 — `SealedAnalysis` is an immediately consumed forwarding bundle

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:292-309,380-417`

`ResolvedProgram::seal` creates a private `SealedAnalysis` containing the same
facts, export origins, capabilities, and status that
`SemanticArtifact::from_analysis` accepts. `freeze` immediately calls
`into_artifact` on that value, and `SealedAnalysis` has no other operation or
consumer. The type therefore names no invariant or ownership transition; it
only adds a field-for-field hop between two methods in the same phase.

**Recommendation:** Let `seal` construct and return `SemanticArtifact`
directly, passing the effect limit from `freeze`, or inline the forwarding
operation into `freeze`. Preserve the order in which export origins are
derived, resolver tables are frozen, name exhaustion is annotated, and
capabilities/status are retained; the simplification should delete only the
single-use bundle and its conversion method.

**Fix Applied:** Removed the single-use `SealedAnalysis` wrapper; `seal` now
constructs the final `SemanticArtifact` directly and receives the effect limit
at that boundary. Export-origin ordering, frozen tables, capabilities, and
status retention are unchanged. Verified with `make fmt && make ci`.

#### [x] READ-021 — Cache adaptation clones an entire analysis wrapper to extract two fields

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/local.rs:230-253`; `glass-lint-core/src/analysis/semantic/mod.rs:100-113`

`SharedSemanticArtifact::from_analyzed` receives `&AnalyzedSource`, clones the
whole wrapper, and then calls `into_parts`. The cache keeps only the semantic
`Arc` and the source line-index `Arc`; it does not use the cloned path or need
an owned `LocatedSourceContext`. The clone consequently copies a
path-specific context solely to reach two reusable fields, while
`LocalArtifact::from_analyzed` legitimately consumes the wrapper for the
path-attached case.

**Recommendation:** Put a narrow borrowed projection on `AnalyzedSource` for
the cache owner, such as a semantic handle plus line-index access, and have
`SharedSemanticArtifact` clone only those `Arc`s. Keep the cache path-neutral,
retain `LocalArtifact`'s consuming conversion for the session path, and do not
expose the internal semantic artifact or source-index storage directly.

**Fix Applied:** Added narrow borrowed projections on `AnalyzedSource` for its
semantic handle and source-line index. Cache adaptation now clones only those
Arcs, while consuming `LocalArtifact` conversion remains path-attached. Verified
with `make fmt && make ci`.

### Bounded local analysis

#### [x] READ-022 — Semantic operation budget also acts as fact-retention capacity

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:145-163,324-334`; `glass-lint-core/src/limits.rs:60-68`; `glass-lint-core/src/analysis/facts/stream.rs:100-112,222-228`

`SemanticAnalyzer::analyze_program` creates a `SemanticBudget` from
`semantic_operations`, then passes that same limit as `ResolvedProgram`'s
`max_facts`, which becomes `FactStream::with_limit(max_facts)`. Work charged
against the semantic budget and retained fact count are different resources,
and the status model already reports them as different outcomes
(`SemanticBudgetExhausted` versus `FactCapacityExhausted`). A caller setting a
small operation limit therefore also silently changes the fact arena's
retention bound, coupling analysis behavior and making the fact-capacity
diagnostic depend on an unrelated knob.

**Recommendation:** Give fact retention an explicit owner and bound (a
dedicated validated limit or the canonical `MAX_FACTS` policy), while the
`SemanticBudget` remains the operation counter. Thread the two values through
the semantic phase as distinct inputs and preserve both fail-closed statuses,
the dense fact-ID invariant, and the existing hard maximum.

**Fix Applied:** The semantic analyzer now passes the canonical `MAX_FACTS`
retention policy independently of `semantic_operations`; the operation counter
remains owned by `SemanticBudget`. Added a regression assertion for tiny
operation budgets. Verified with `make fmt && make ci`.

### Scoped completeness diagnostics

#### [x] READ-023 — Resolution diagnostics match the same enum twice

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/semantic/status.rs:194-278`

The `IncompleteReason::UnsupportedResolution` arm matches `ResolutionKind`
once to select the message suffix and immediately matches it again to select
the diagnostic code. The enum currently has two variants, but the duplicated
dispatch can drift as soon as a new resolution outcome is added; the nearby
`ParseFailureKind::diagnostic` and `AnalysisComponent::budget_diagnostic`
helpers already demonstrate the single-owner mapping pattern.

**Recommendation:** Put the `(DiagnosticKind, message)` mapping on
`ResolutionKind`, or use one match that returns both values, and let
`IncompleteReason::diagnostic` only format the request-specific text. Preserve
the distinct unsupported-versus-outside-project codes and the deterministic
diagnostic message.

**Fix Applied:** Added the owner-level `ResolutionKind::diagnostic` mapping
and used it for both diagnostic code and message text, removing duplicate
dispatch in status formatting. Verified with `make fmt && make ci`.

## Systemic Themes

- A phase wrapper is justified when it enforces a consuming transition or
  changes ownership. A private bundle that is immediately converted into the
  next struct should be deleted or folded into the owner.
- Cache boundaries should project reusable semantic handles without cloning
  path-specific context. The cache must remain matcher-independent and
  collision-safe.
- Resource limits should name the resource they bound. Operation budgets,
  retained facts, and downstream phase capacities may all be bounded, but they
  should not share a public knob or status path accidentally.

## Open Questions

- Separate the fact-retention limit from the semantic operation limit while
  retaining the existing operation charge for every admitted fact. The two
  status reasons then continue to describe distinct exhausted resources.
- Keep any `AnalyzedSource` cache projection private and borrow both reusable
  handles from the same analyzed value so a cache artifact cannot pair a
  semantic artifact with a mismatched source index.

## Coverage

Reviewed the chunk-05 structure entries and their implementation/test support:

- `analysis/local.rs`
- `analysis/semantic/{mod,budget,status}.rs`
- Supporting capability and fact-construction boundaries in
  `analysis/{mod,facts/stream}.rs`
- Related cache/session, project-linking, reporting, and limit callers were
  traced to validate ownership, status propagation, and resource semantics.

No source, test, configuration, dependency, or other documentation files were
changed by this audit.
