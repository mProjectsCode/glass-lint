# Codebase Readability Audit — Chunk 18

## Summary

Chunk 18 covers provider-neutral rule declarations, query construction and
composition, module-specifier patterns, lifecycle declaration types, argument
value matchers, and rule/query error APIs. The authoring surface is generally
well bounded: constructors validate collections and paths, logical branches
enforce depth limits, and compiler-owned query structure remains private.

The new issues are concentrated in authoring consistency and repeated
construction protocols. Rule metadata errors do not preserve the first
duplicate, symbolic query names are validated for emptiness but not
canonicalized, three correlated member-query constructors duplicate one graph
assembly, and the one-value static matcher path bypasses the canonical
multi-value validation path.

Earlier findings READ-075 and READ-078 cover lifecycle sink representation and
deferred lifecycle-builder error retention respectively; they are not repeated
here.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Rule builder diagnostics

#### [x] READ-090 — Preserve the first duplicate rule-metadata error

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Builder lifecycle / Error determinism / API
- **Location:** `glass-lint-core/src/api/rule/mod.rs:107-224`

`RuleBuilder` deliberately retains the first query construction error in
`first_query_error`, but every repeated `description`, `category`, `severity`,
or `confidence` call overwrites `duplicate_field`. A chain that duplicates two
metadata fields therefore reports whichever duplicate happened last, making
the deferred error depend on unrelated later builder operations and giving
metadata a different error-selection policy from queries.

**Recommendation:** Store the first duplicate field with the same
first-error policy as `first_query_error`, or retain a bounded ordered error
list if aggregate diagnostics are desired. Preserve the current “duplicate
metadata fails before other validation” precedence and the fact that the last
value is not semantically accepted; delete the overwrite assignment once one
owner determines deferred error ordering.

**Fix Applied:** Added a first-error accumulator to `RuleBuilder`. Repeated
description, category, severity, and confidence calls now preserve the first
duplicate field, with a regression test covering multiple duplicate fields.

**Verification:** `cargo test -p glass-lint-core api::rule::tests --lib`
(7 passed) and `make fmt && make ci` (passed).

### Query-input canonicalization

#### [ ] READ-091 — Canonicalize symbolic query names at the query boundary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Validation / Canonicalization / Semantic identity
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:248-266`,
  `glass-lint-core/src/api/rule/query/constructors.rs:23-61,92-149,224-284`,
  `glass-lint-core/src/api/rule/module.rs:17-53`

`checked_name` and the module-namespace constructors reject values whose
trimmed form is empty but retain the original whitespace. Thus a global,
heuristic, class, constructor, export, or namespace identity authored as
`" fetch "` is stored with spaces and will not match the canonical semantic
name `fetch`; `ModuleSpecifierPattern` and `Category` instead trim before
storing. The same boundary mixes validation-only helpers with canonicalizing
helpers, so equivalent authored inputs can produce different identities and
evidence symbols.

**Recommendation:** Give symbolic names and module/export components one
canonical constructor that trims and validates before creating `SmolStr`, and
use it across the event constructors and composition helpers. Preserve
literal-string matcher contents where whitespace is semantic, and preserve
member-chain canonical spelling; only delete the repeated validation-only
paths for identifiers/module components once all symbolic identity owners use
the same normalization contract.

**Fix Applied:** None so far.

### Correlated query composition

#### [ ] READ-092 — Centralize correlated member-query graph assembly

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Query construction / Semantic drift
- **Location:** `glass-lint-core/src/api/rule/query/composition.rs:33-175`

`member_call_instance`, `member_call_returned`, and
`member_read_returned` each hand-build the same five-branch `All` expression:
event selection, event kind, event identity, object binding, and member
subject correlation. The copies differ in exactly the dimensions that need to
remain explicit—returned versus constructed identity, member call versus read,
and evidence symbol/kind—so a new correlation predicate or branch-order rule
must be updated in three constructors.

**Recommendation:** Add one private composition helper that owns the common
selection/identity/object/member-subject assembly and accepts the event
specification, object-binding identity, and evidence metadata as typed inputs.
Preserve variable numbering, branch order, rooted-versus-module identity
restrictions, and the distinct call/read evidence; delete the three repeated
branch vectors after their callers use the shared owner.

**Fix Applied:** None so far.

### Static-string matcher construction

#### [ ] READ-093 — Route single-value equality through canonical static-value validation

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Validation / Matcher API / Semantic drift
- **Location:** `glass-lint-core/src/api/rule/query/value.rs:58-118,128-178`

`ValueMatcher::equals` directly stores one untrimmed string and cannot reject an
empty value, while `equals_any`, `starts_with_any`, `contains_any`, and
`contains_all` all use `bounded_strings`, which trims, rejects empty values,
sorts, deduplicates, and enforces the alternative limit. The two API shapes
therefore give different results for equivalent calls such as `equals(" x ")`
and `equals_any([" x "])`, and `equals("")` bypasses the `EmptyStaticValue`
contract used by the collection forms.

**Recommendation:** Make the one-value form use the same canonical value
parser (with a fallible return if empty values are invalid), or explicitly
split an exact-literal API from canonical static alternatives. Preserve the
meaning of exact versus prefix/contains predicates, bounded alternative
counts, and intentional literal-string whitespace; remove the direct vector
construction once one static-value owner establishes the invariant.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 18’s authoring API has strong bounded constructors, but canonicalization
and deferred-error policy are not fully centralized. Several helpers validate
the same domain values differently, and correlated query declarations rebuild
the same compiler-facing graph manually. A small number of domain-owned
constructors would make the public authoring behavior deterministic while
preserving the intentional distinction between semantic identifiers and
literal string content.

No findings are marked applied.

## Open Questions

- If whitespace is intentionally meaningful for any symbolic identity, that
  identity should be represented as a literal-string predicate rather than
  sharing the global/module-name constructors; the current APIs do not expose
  that distinction clearly.
- Changing `ValueMatcher::equals` to return `Result` would be a breaking API
  change; retaining its signature requires documenting whether empty and
  whitespace-padded exact values are supported rather than silently differing
  from the collection methods.
- The shared correlated-query helper should preserve the current explicit
  branch form so compiler validation and contradiction diagnostics remain
  unchanged.
- The next unreviewed handoff is Chunk 19: configuration, linting, parsing, and
  rule-selection types listed in `CODEBASE_STRUCTURE_CORE.md`.

## Coverage

Reviewed the Chunk 18 modules listed in `CODEBASE_STRUCTURE_CORE.md` across
rule metadata/builders, catalog and matcher error types, module patterns,
event/identity declarations, query composition and expressions, lifecycle
declarations, argument/value matchers, taxonomy, and query diagnostics.
Representative callers were traced through provider rule construction,
compiler lowering, query normalization, and catalog validation. Prior
compiler findings READ-074, READ-075, READ-076, READ-077, and READ-078 were
checked to avoid repeating their root causes. No source, test, configuration,
dependency, or documentation changes were made.
