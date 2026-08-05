# Codebase Readability Audit — Chunk 9

## Summary

Chunk 9 covers function sink summaries, summary-path storage and propagation,
local artifact/cache ownership, lowering, budgets, and completeness status.
The design has strong foundations: summary paths distinguish frozen artifact
paths from bounded overlays, semantic artifacts are immutable and lazily derive
effects, cache entries verify full keys after fingerprint lookup, and status
entries are ordered and deduplicated.

The main risks are mismatched ownership and parallel representations. Summary
projection bypasses the canonical effective-call-argument view for wrappers,
local semantic artifacts can be paired with an unrelated source-location
context, function signatures are cached as separate fields beside live
parameter bindings, and status scope conversion relies on an unstated caller
invariant. These are concrete because they can change path projection or
diagnostic identity without changing the type-level phase boundary.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Function-summary call projection

#### [x] READ-047 — Use canonical effective arguments in summary sink projection

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API / Semantic identity / wrapper handling
- **Location:** `glass-lint-core/src/analysis/flow/summary/sink.rs:222-267`,
  `analysis/flow/effect/mod.rs:257-264`,
  `analysis/flow/projector/mod.rs:600-609`

`FunctionSummary::collect_sinks_for_call` pattern-matches the call's stored
`args` and uses that slice for `present_indices`, argument lookup, and summary
path construction. The same call is also wrapped in `CallEffectRef`, whose
`effective_args()` deliberately selects `CallUnwrap::effective_args` for
`.call()`/`.apply()` wrappers. Effect extraction and local projection use that
canonical view, but summary collection does not. For a wrapper call, summary
projection can therefore inspect the receiver/argument-list shape rather than
the target invocation that the other flow phases analyze.

Obtain one effective-argument slice from `CallEffectRef` and use it for sink
indices, argument values, and parameter path projection; delete the parallel
raw-`args` path after migration. Preserve unknown/spread rejection, wrapper
chain matching, parameter-path identity, deterministic sink ordering, and
fail-closed behavior when effective arguments cannot be reconstructed.

**Fix Applied:** `FunctionSummary::collect_sinks_for_call` now obtains its
argument slice from `CallEffectRef::effective_args`, so sink index checks,
argument lookup, and parameter-path projection all use the same canonical
`.call()`/`.apply()` view as effect extraction and local projection. Invalid or
unreconstructable calls still fail closed. Existing effective-argument flow
tests cover the wrapper normalization contract.

**Verification:** `cargo test -p glass-lint-core --lib analysis::flow::summary`
and `make fmt && make ci` pass.

### Summary representation

#### [ ] READ-048 — Encapsulate the summary function signature

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype / Derived state / API
- **Location:** `glass-lint-core/src/analysis/flow/summary/sink.rs:101-119,145-189`,
  `analysis/flow/summary/summaries.rs:81-103`

`FunctionSummary` stores `parameter_count` and `has_rest` as independent
fields, while `parameter_bindings(stream)` reads the authoritative parameter
bindings from the frozen fact stream. `collect_facts` derives the two cached
values from those bindings, and `is_invocation_compatible` first uses the
cached fields before iterating the live bindings for defaults, paths, and rest
behavior. The summary constructor accepts all three pieces independently, so
the representation permits a signature count/rest flag that disagrees with
the parameter bindings used later in the same compatibility check.

Introduce a private `FunctionSignature` value derived from the stream's
parameter bindings, or make compatibility query one signature owner instead of
mixing cached fields with a second source of truth. Remove the independent
constructor arguments after callers migrate. Preserve default-parameter and
rest-parameter compatibility, unknown/spread rejection, parameter path
projection, and deterministic summary propagation.

**Fix Applied:** None so far.

### Local artifact identity

#### [ ] READ-049 — Bind source-location context to its semantic artifact

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Identity ownership / reporting
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:97-117`,
  `analysis/local.rs:405-415`,
  `analysis/project/model.rs:277-294`

`LoweredSource::new` and `LocalArtifact::new` accept a source-location context
and an independently supplied `Arc<SemanticArtifact>`. The semantic artifact
contains source-derived facts and spans, while the context supplies the path
and line index used for reporting; the constructors do not express that these
must come from the same source. Normal cache/session callers pair them
correctly, but an internal caller can attach one file's semantic model to
another file's path or line map and still construct a valid `ProjectModule`.
The resulting diagnostics and evidence locations can be silently attributed
to the wrong module.

Make the lowered result the sole construction boundary for the pair, or give
the semantic artifact an owned source identity and validate any separate
location attachment before construction. Delete raw pair constructors after
migration. Preserve cache reuse of immutable semantic state, path-specific
line maps, project module IDs, and the separation between local facts and
project linking overlays.

**Fix Applied:** None so far.

### Completeness status

#### [ ] READ-050 — Make local-to-file status conversion explicit and checked

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Diagnostic scope / invariant
- **Location:** `glass-lint-core/src/analysis/lowering/status.rs:71-112`,
  `analysis/project/linker/mod.rs:100-113`

`AnalysisStatus::for_file` rewrites every stored `StatusEntry` to
`StatusScope::File(path)`, regardless of whether the entry was originally
file-scoped or project-scoped. The linker calls it on each local artifact to
attach local incompleteness to a module, so the current behavior relies on the
unstated invariant that local lowering records only project-scoped entries.
If a new local lowering path records a file scope, or a project-level reason is
added to the local status, this method silently changes the diagnostic's
aggregation scope and can produce misleading per-file output.

Replace the broad remap with an explicit local-status-to-file operation that
accepts reasons (or validates the expected source scope) and keep project
status extension separate. Preserve B-tree deduplication, completion being
driven by status entries, parser-diagnostic de-duplication, and deterministic
file/project diagnostic ordering.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 9 generally keeps artifacts immutable and bounded, but several important
relationships remain caller-enforced: effective call arguments versus raw
wrapper syntax, summary signature fields versus fact-stream bindings, source
locations versus semantic artifacts, and status scope versus the phase that
recorded it. These are identity and certainty boundaries, not cosmetic API
concerns; each should be represented by one owner or an explicit conversion.

The summary-path overlay is a useful bounded abstraction and should remain
separate from frozen fact paths. Cache fingerprinting should continue to use
full-key verification, and status refactors must not turn incomplete analysis
into a complete artifact or change possible evidence into a witness.

Search signals used for this chunk included raw/effective argument consumers,
duplicated function-signature inputs, constructors pairing independent source
and semantic handles, and scope-remapping methods that rewrite all variants.

## Open Questions

- The effective-argument accessor should remain the single source for wrapper
  projection; raw syntax arguments may still be retained only where evidence
  explicitly needs the wrapper call site.
- A source/semantic pairing type should preserve cache sharing without copying
  semantic facts or merging report-local line indexes into matcher-independent
  state.
- The next unreviewed handoff is Chunk 10: matching types.

## Coverage

Reviewed the Chunk 9 types listed in `CODEBASE_STRUCTURE_CORE.md` across sink
summaries, summary-path storage, summary propagation, local cache/artifact
types, lowering and span normalization, semantic budgets, and completeness
status, with representative callers in local projection, cache sessions, and
project linking. Existing Chunk 1–8 findings were checked to avoid re-reporting
fact traversal, generic worklist admission, effect-builder lifecycle, flow
projector state/history, and cross-flow emission ownership. No findings are
marked applied.
