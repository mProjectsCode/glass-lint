# Codebase Readability Audit

## Summary

Chunk 6 owns matcher-independent occurrence indexes, project occurrence and
identity overlays, indexed and fallback argument evaluation, and deterministic
evidence publication. The typed index families, borrowed k-way occurrence
merge, ambiguity masking, and evidence normalization are appropriate
boundaries. Three current opportunities remain: project identity inputs are
wrapped in two indistinguishable types, constrained-root preparation allocates
an intermediate borrow-only vector, and constrained argument evaluation repeats
a value-table lookup that its operation budget reports only once.

## Findings

### Project overlay API

#### [ ] READ-059 — Collapse duplicate project identity input/view wrappers

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** API design
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:222-320`; caller in `glass-lint-core/src/analysis/project/projection.rs:149-168`; test constructors throughout `glass-lint-core/src/analysis/matching/arguments/mod.rs`

`MatcherProjectInputs` and `MatcherProjectOverlay` store the same two borrowed
maps: module-export identities and call-result identities. The former exposes a
constructor and two trivial accessors, while `MatcherProjectOverlay::from_inputs`
copies those fields without validation or transformation. The production path
therefore creates one identity-only wrapper to construct the artifact and then
immediately converts it into another identity-only wrapper for evaluation. The
test-only `MatcherProjectOverlay::new` also accepts an `_occurrence` argument it
does not use; most callers pass `None`, and the one passing an occurrence still
has no effect.

**Recommendation:** Make one narrow project-identity view own this pair of
references and use it as the input to artifact construction and as the stored
project view, deleting the duplicate wrapper, conversion, trivial accessors,
and unused occurrence parameter. Keep `MatcherProjectContext` as the production
pairing boundary so its artifact and project view cannot be assembled from
different project inputs. Keep `LinkedOccurrenceView` separate: it remaps
physical occurrence buckets and is a real artifact overlay, unlike these
identical identity-only wrappers. Preserve call-result-over-module identity
precedence, overlay enablement, and all unknown/ambiguous fail-closed behavior;
update tests to construct only the remaining semantic input.

**Audit disposition (2026-08-13):** Confirmed. The remaining context type is
the useful pairing boundary; only the identical identity-only input/view
wrappers should be collapsed.

### Constrained-root preparation

#### [x] READ-060 — Prepare constrained roots without temporary borrow storage

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Performance / complexity
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:30-63, 117-147`

`ConstrainedEvaluation::prepare` first filters physical roots into a
`Vec<ConstrainedRoot<'_>>`, then immediately iterates that vector to create the
owned `Vec<PreparedConstrainedRoot<'_>>`. `ConstrainedRoot` contains only
references plus a copied rule index, and `PreparedConstrainedRoot::new` copies
those same references into its own field before deriving prepared clause paths.
The intermediate vector has no independent lifetime, ownership, or reuse
purpose; every constrained root can be converted directly into its prepared
form in one iterator pipeline.

**Recommendation:** Replace the temporary `constrained` collection with a
direct `filter_map`/`map` into `PreparedConstrainedRoot`, preserving physical
root order, one prepared entry per constrained root, fallback state, prepared
path semantics, and the existing bounded evaluation accounting. Do not merge
this with the later fallback scan: preparation and evidence publication remain
separate lifecycle operations.

**Audit disposition (2026-08-13):** Confirmed. The direct pipeline removes a
borrow-only intermediate without changing root order, preparation state, or
the later fallback lifecycle.

**Fix Applied:** Constrained evaluation now filters and prepares roots in one
iterator pipeline, eliminating the intermediate `Vec<ConstrainedRoot>` while
preserving root order, fallback state, and separate evidence publication.
Verified with `make fmt && make ci`.

### Argument value resolution

#### [x] READ-061 — Reuse one resolved value during constrained argument matching

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Performance / API design
- **Location:** `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:104-150, 232-251`; `glass-lint-core/src/analysis/matching/arguments/mod.rs:254-273`

`MatcherEvaluator::argument_with_overlay` resolves the argument value once to
derive a static-object or rooted-member projection. When no project identity
supplies a static string, `EffectiveIdentityResolver::static_string` then
falls back to `ValueTable::static_string`, which resolves the same value ID
again through the terminal-binding cache. This happens for each constrained
argument group, while `constraints_match` charges one value resolution before
calling the helper. The tests therefore verify the intended public operation
count, but that count does not include the second local lookup or expose the
duplicate work.

**Recommendation:** Retain the borrowed `Option<&Value>` (or a narrow
resolution result) from the first lookup and pass it to the identity resolver
for local static-string fallback. Preserve result-identity, module-identity,
then local-value precedence; static-object and rooted-chain projections;
binding-terminal behavior; dynamic/unknown fail-closed matching; and one
operation charge per prepared argument group. Avoid cloning values or exposing
`ValueTable` internals to the matcher.

**Audit disposition (2026-08-13):** Confirmed. Reuse must stay inside the
matcher/value-table boundary; do not clone values or broaden the table API.

**Fix Applied:** `argument_with_overlay` now retains the single terminal
`Value` lookup and passes it to local static-string fallback. Result-identity,
module-identity, local-value precedence, object/rooted projections, and
operation charging are unchanged. Verified with `make fmt && make ci`.

## Systemic Themes

- A matcher-facing API should represent one semantic project input once. The
  context pairing invariant is valuable, but duplicate wrappers around the same
  references obscure that invariant rather than enforcing another one.
- Preparation should materialize only the state needed by the next phase. The
  constrained-root temporary vector and repeated value resolution add work
  without changing matching semantics or evidence determinism.
- Operation budgets should correspond to actual bounded work. Reusing the
  resolved value keeps the budget honest while preserving the existing
  fail-closed identity and argument semantics.
- Typed occurrence indexes, borrowed overlay merging, direct-versus-star
  identity contributions, capability-bearing query views, and final evidence
  ordering were reviewed and retained as necessary architecture. Their
  separate representations encode distinct matching or certainty behavior.

## Open Questions

- None blocking these findings. No source or test changes were made; this
  audit file was updated only with review dispositions. The
  existing operation-count assertions should be revisited if READ-061 is
  implemented so they document both the intended abstraction and its actual
  bounded work.

## Coverage

Reviewed only Chunk 6, “Matching,” from `CODEBASE_STRUCTURE_CORE.md`:
occurrence index families and query views, global/module bucket overlays,
linked occurrence remapping and ambiguity masking, constrained-root planning,
argument evaluation and prepared clause paths, indexed versus fallback scans,
evidence construction, identity maps, and matching tests/callers. The root and
core architecture documents, testing/contribution guidance, current audit
chain, project projection caller, value-table resolution behavior, and focused
operation-count tests were inspected. The focused matching test suite passed:
`cargo test -p glass-lint-core analysis::matching --lib` (44 passed). No
source, test, configuration, dependency, or other documentation files were
changed; this chunk audit file was updated only with review dispositions. The next chunk is
Chunk 7, “Project linking,” which should continue finding IDs at READ-062.
