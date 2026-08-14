# Codebase Readability Audit

## Summary

Chunk 12 covers `glass-lint-core/src/project`: project input DTOs, staged
session execution, source/resolution tables, and report types. The strongest
opportunities are to give project admission one owner and centralize the
repeated validated-string wrapper behavior without collapsing distinct
semantic types. The separate resolver and linked-target enums are retained:
the public input contract must not expose internal `ModuleId` identity, and
Rust’s exhaustive conversion already forces new variants to be handled.

## Findings

### Project input admission

#### [ ] READ-031 — Give source admission one owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/session/mod.rs:222-266`, `glass-lint-core/src/project/tables.rs:19-58`

`ProjectSession::accept_sources` first materializes the incoming iterator into
a `Vec`, sums source bytes, checks source count and byte limits against the
existing `SourceTable`, and only then calls `SourceTable::insert_all`. The
table then stages another `BTreeMap`, rechecks duplicate paths and checked byte
addition, and appends the staged entries. The session owns admission policy,
while the table independently owns enough admission mechanics to reject the
same inputs. This forces every future source-ingestion path to keep two
validation sequences aligned and pays for an extra batch collection/staging
pass.

**Recommendation:** Make one project-owned admission operation validate the
limits and atomically insert the batch, with `SourceTable` retaining only its
path/byte accounting invariant (or accepting a small admission-policy value
from the session). Delete the duplicate preflight arithmetic and redundant
staging boundary after all callers use the owner. Preserve atomic duplicate
rejection, count/byte overflow behavior, normalized-path order, and the
public `ProjectInputError` values; add tests for duplicate-in-batch,
duplicate-against-session, count limit, byte limit, overflow, and unchanged
state after every failed insertion.

**Fix Applied:** None so far.

### Validated scalar wrappers

#### [ ] READ-033 — Centralize repeated validated-string wrapper behavior

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype
- **Location:** `glass-lint-core/src/project/types/input.rs:50-232`

`PackageSpecifier`, `BuiltinModuleName`, and `NormalizedOutsidePath` each own
a `SmolStr`, expose `as_str`, `Deref<Target = str>`, `AsRef<str>`,
`Borrow<str>`, `PartialEq<&str>`, and `Display`, while each constructor adds a
different validation/normalization policy. This is roughly three copies of
the same public wrapper surface around the same storage. The duplication is
already visible in small inconsistencies (for example `AsRef<Path>` exists
only for outside paths), and every new validated scalar requires manually
repeating the forwarding implementations without a single owner for the
shared invariant.

**Recommendation:** Keep the three semantic types distinct and use a narrowly
scoped local macro (or equivalent private forwarding helper) for only the
repeated accessor/formatting trait implementations. Leave policy-specific
constructors and conversions in each type; do not add a shared validated-text
domain or permit cross-domain conversion merely to reduce lines. Preserve
trimming versus normalization rules, borrowed lookup behavior, path
conversion only where valid, and the existing error payloads; add constructor
and trait-contract tests for each wrapper, including whitespace, NUL,
relative/scoped, and outside-path cases.

**Fix Applied:** None so far.

## Systemic Themes

- Project phase boundaries are generally explicit. Source admission is the
  concrete duplicated policy; the resolver/link target conversion is an
  intentional public-to-internal type transition.
- Public domain newtypes protect important invariants; the next refactor
  should reduce their boilerplate without erasing semantic distinctions.

## Review Resolutions

- Keep source-admission limits private to `ProjectSession`; `SourceTable`
  should own only atomic path and byte accounting, with the session supplying
  project limits once.
- Keep `ResolverOutcome` independent from `LinkedModuleTarget`: the former is
  the validated resolver-facing contract and the latter contains assigned
  project identities. The exhaustive `resolve_record` match is the correct
  synchronization point.

## Coverage

Reviewed the project module, input and report DTOs, source/resolution tables,
session/artifact transitions, local executor, and project tests. No source
changes were made; this artifact records recommendations only.
