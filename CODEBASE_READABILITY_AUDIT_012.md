# Codebase Readability Audit

## Summary

Chunk 12 covers `glass-lint-core/src/project`: project input DTOs, staged
session execution, source/resolution tables, and report types. The strongest
opportunities are to give project admission one owner, reduce duplicated
resolution-domain representations at the local/link boundary, and centralize
the repeated validated-string wrapper behavior without collapsing their
distinct semantic types.

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

### Resolution phase boundary

#### [ ] READ-032 — Share the resolver-target domain across the phase boundary

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/input.rs:390-429`, `glass-lint-core/src/analysis/project/model.rs:206-224`

`ResolverOutcome` and `LinkedModuleTarget` each encode `External`, `Builtin`,
`Missing`, `OutsideProject`, and `Unsupported` with the same payload types;
only `Internal` changes from a validated `ProjectRelativePath` to an assigned
`ModuleId`. `resolve_record` then repeats the six-way mapping and performs the
path-to-ID lookup. The phase distinction is legitimate, but the duplicated
closed-world variants make additions unsafe: a new resolver outcome can be
accepted at the public input boundary and silently omitted from linking, and
the representation must be kept synchronized by hand.

**Recommendation:** Introduce one private shared target-kind representation
or a typed conversion that owns the common variants and makes the internal
path-to-`ModuleId` transition explicit; delete the duplicate enum arms and
centralize the exhaustive conversion at the linker-input boundary. Keep
`ResolverOutcome` as the validated public resolver contract if needed, retain
the distinction between unresolved paths and linked IDs, and preserve invalid
internal-target errors plus all downstream matching behavior. Add compile-time
exhaustiveness coverage and tests for every target variant, missing internal
path, and unsupported empty reason.

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

**Recommendation:** Keep the three semantic types distinct, but centralize
their common storage/accessor/formatting behavior in a private validated-text
core (or a narrowly scoped local macro) and leave only policy-specific
constructors and conversions in each type. Delete repeated forwarding
implementations after migration; do not expose the storage or permit
cross-domain conversion merely to reduce lines. Preserve trimming versus
normalization rules, borrowed lookup behavior, path conversion only where
valid, and the existing error payloads; add constructor and trait-contract
tests for each wrapper, including whitespace, NUL, relative/scoped, and
outside-path cases.

**Fix Applied:** None so far.

## Systemic Themes

- Project phase boundaries are generally explicit, but validation and target
  representation are repeated across those boundaries instead of being
  centralized behind narrow conversion owners.
- Public domain newtypes protect important invariants; the next refactor
  should reduce their boilerplate without erasing semantic distinctions.

## Open Questions

- Should source admission limits be a reusable project policy object shared by
  the core session and the project crate, or remain private to
  `ProjectSession`?
- Does serialization or a downstream resolver require the public
  `ResolverOutcome` shape to remain independent from the internal linked
  target shape?

## Coverage

Reviewed the project module, input and report DTOs, source/resolution tables,
session/artifact transitions, local executor, and project tests. No source
changes were made; this artifact records recommendations only.
