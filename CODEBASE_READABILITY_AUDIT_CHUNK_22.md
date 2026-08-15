# Codebase Readability Audit — glass-lint-core Chunk 22: Configuration, parsing, and runtime environment

## Summary

Chunk 22 covers the provider-neutral front-end and runtime-environment types:
`config` (`CoreConfig`), `diagnostic` (`RuleMetadata`, `Severity`,
`SourceLineIndex`), `ecma_version` (edition/feature detection and
`FeatureDetector`), `environment` (`Environment`, `EnvironmentInner`,
`GlobalObjectMembers`), `limits` (`AnalysisLimits`, `PositiveLimit`,
`ProjectAdmissionLimits`), and `parse` (`ParseDiagnostic`, `ParseFailureKind`,
`ParsedSource`, `SourceLanguage`, `SourceParser`, `SyntaxDepthGuard`/`Phase`/
`Outcome`/`Error`, and `depth::DepthScanner`).

The chunk is generally well-factored: validated-config invariants are enforced
through `PositiveLimit`, environment mutation is centralized behind
`register_global`, depth scanning is owned by `DepthScanner`, and the
feature detector is deterministic. Findings concentrate in four areas:
(1) a redundant promoted-member predicate on `Environment`; (2) duplicated
fail-state representations in the bounded-depth machinery
(`SyntaxDepthError` vs `SyntaxDepthOutcome`, Result-then-bool); (3) duplicated
constants and validation sequences (`MAX_SYNTAX_DEPTH` vs
`limits::default_syntax_depth`, the `validated_identifier` collect pattern,
`Severity` Display/as_str); and (4) a test-only mutation surface on
`AnalysisLimits` that contradicts the type's documented constructor guarantee.

No source files were modified; only this audit file was created.

## Findings

### [diagnostic / Severity]

#### [ ] READ-004 — `Severity::Display` and `Severity::as_str` duplicate the same variant-to-string mapping

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/diagnostic.rs:24-48`

`impl fmt::Display for Severity` (lines 24-36) and `Severity::as_str` (lines
38-48) each contain an independent `match` mapping the three variants to the
same string literals (`"info"`, `"warning"`, `"error"`). The serialized
spelling also appears a third time in the `serde(rename_all = "lowercase")`
attribute on the enum (line 13). Every addition of a severity variant must now
touch two (effectively three) parallel places, and a typo in one would silently
desync `Display`, `as_str`, and the wire format.

**Recommendation:** Have `Display` delegate to `Self::as_str()` (single source
of truth for the public spelling), keeping `as_str` as the `const fn` public
surface. Guardrail: the exact strings `"info"`, `"warning"`, `"error"` are
part of the serialized report schema and must not change.

**Fix Applied:** None so far.

### [ecma_version / detector]

#### [ ] READ-006 — `FeatureDetector::visit_object_lit` is a no-op override identical to the SWC default

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/ecma_version/detector.rs:293-295`

`visit_object_lit` only calls `object.visit_children_with(self)`, which is byte
for byte the default `swc_ecma_visit::Visit::visit_object_lit` implementation
(verified against swc_ecma_visit 26.0.0). It records nothing and adds no
behavior, but it implies object literals are special-cased here, which misleads
readers trying to understand where `ObjectRestSpread` is recorded (the real
hook is `visit_prop_or_spread` at lines 297-302).

**Recommendation:** Delete the override so object-literal traversal relies on
the default visitor; the existing object-spread tests
(`ecma_version/tests.rs:26-37`) guard the behavior. Guardrail: keep
`visit_prop_or_spread` (the actual spread-recording hook) intact.

**Fix Applied:** None so far.

#### [ ] READ-007 — `in_parameter_pattern` save/set/restore sequence is duplicated across the two function visitors

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/ecma_version/detector.rs:47-52` and `109-114`

`visit_arrow_expr` and `visit_function` each repeat the same five-step pattern:
save `in_parameter_pattern`, set it true, visit `params`, set it false, visit
the body, then restore the saved value. The two copies differ only in the node
field names. This is subtle stateful-visitor logic; having it in two places
makes it easy to fix one and forget the other when the flag semantics evolve.

**Recommendation:** Extract one private helper that runs the params visit with
the flag set and the body visit with it cleared, then restores the saved outer
value (e.g. `fn visit_under_parameter_pattern(&mut self, visit_params:
impl FnOnce(&mut Self), visit_body: impl FnOnce(&mut Self))`), and call it from
both visitors. Guardrail: the ordering must stay exactly "set true — visit
params — set false — visit body — restore", so nested functions inside default
parameter values keep the correct context while destructuring assignments in
the body are not misdetected as default parameters.

**Fix Applied:** None so far.

### [environment]

#### [ ] READ-001 — `Environment::is_promoted_global_member` duplicates `is_global_member` with a misleading name

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/environment.rs:297-299` (definition), `289-295` (`is_global_member`), `372-374` (`is_global_object`), `352-370` (`is_promoted_global_member_path`, sole internal caller at line 369)

`is_promoted_global_member(object, member)` is
`self.is_global_object(object) && self.is_global_member(object, member)`, but
`is_global_member` already returns `false` when `object` is absent from
`global_objects` (the `None => false` arm). The two predicates are therefore
logically identical, and the `is_promoted_global_member` name suggests a
distinction (a "promoted" callable identity) that the code does not make. The
extra `is_global_object` method exists only to serve this redundant wrapper.

**Recommendation:** Consolidate on the public `is_global_member` (whose doc
already describes the promoted-identity semantics) and delete
`is_promoted_global_member` and the now-unused private `is_global_object`,
updating `is_promoted_global_member_path` (line 369) to call `is_global_member`.
Guardrail: `global_object_name_paths_match` (lines 320-350) must keep its
current alias/promoted-member behavior; the existing environment tests
(`environment/tests.rs:83-136`) cover both paths.

**Fix Applied:** None so far.

#### [ ] READ-008 — `add_globals` and `add_global_object_with_members` repeat the same validate-and-collect sequence

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/environment.rs:196-206` and `225-241`

Both mutation methods map `Self::validated_identifier` over an iterator and
`collect::<Result<BTreeSet<_>, _>>()?` before inserting, preserving the
atomic validate-then-commit behavior. The sequence is identical except for the
destination (`global_bindings` vs a `Restricted` member set).

**Recommendation:** Extract one private helper, e.g.
`fn validated_identifiers<I, S>(names: I) -> Result<BTreeSet<SmolStr>, EnvironmentError>`, and call it from both builders. Guardrail: keep the atomic
semantics (all-or-nothing on validation failure), asserted by
`environment/tests.rs:176-180`.

**Fix Applied:** None so far.

### [limits]

#### [ ] READ-005 — Test-only `AnalysisLimits::set_*` mutation API contradicts the documented construction guarantee and panics instead of returning a typed error

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/limits.rs:276-316`; type doc `71-75`; representative callers `glass-lint-core/src/project/tests/status_policy.rs:249,260,281,305`

`AnalysisLimits` documents "Every field is guaranteed positive. The only way to
obtain a value is through `Default` and the named builder methods, all of which
reject zero." The seven `#[cfg(test)] set_*` methods (via `set_limit`, line
313-316) contradict that documented construction surface in every test build:
they write a field directly and `expect("test setter requires positive value")`
(line 314) instead of returning `Result`, so a zero value produces a panic
rather than the typed `AnalysisLimitError` every production path returns. The
surface exists so the limit matrix in `status_policy.rs` can pass a
`fn(&mut AnalysisLimits, usize)` setter pointer (the `setter` parameter of
`assert_limit_triplet` at status_policy.rs:178 and `assert_flow_limit_transition`
at :213), which the by-value `Result`-returning `with_*` builders cannot
satisfy.

**Recommendation:** Remove the seven public `set_*` methods and express the test
matrix with small closures or a single `#[cfg(test)]` helper owned by the test
module, keeping the documented "Default + named builders" as the only
construction path. Guardrail: production `with_*` behavior (rejecting zero with
`AnalysisLimitError`) is unchanged; `Default::default()` and the serde
deserializer must keep producing the same values.

**Fix Applied:** None so far.

### [parse]

#### [ ] READ-003 — `SyntaxDepthError` is a single-variant unit error used only as a boolean control-flow flag

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/parse.rs:317-327` (`SyntaxDepthOutcome`), `339-388` (`SyntaxDepthGuard`), `390-393` (`SyntaxDepthError`), `230-235` and `246-252` (call sites); `glass-lint-core/src/parse/depth.rs:116,140` (`observe`/`push_delimiter`)

`SyntaxDepthError` has exactly one unit variant, `Exceeded`, which duplicates
`SyntaxDepthOutcome::Exceeded` as a second representation of the same event.
The `Result<(), SyntaxDepthError>` returned by `check_before_parse` and
`check_after_parse` is immediately reduced to a boolean at both call sites
(`.is_err()`), and the error value is discarded; the diagnostic is synthesized
separately in `syntax_depth_diagnostic`. Inside `DepthScanner`, `observe`/
`push_delimiter` return the same unit error only so `scan` can short-circuit to
`Exceeded`. Two parallel fail-state types plus a Result-that-is-used-as-bool
make the bounded-depth path read as flag-driven rather than outcome-driven.

**Recommendation:** Have `DepthScanner::observe`/`push_delimiter` and the
guard's `check_before_parse`/`check_after_parse` return a plain bool and delete
`SyntaxDepthError`, keeping `SyntaxDepthOutcome::WithinLimit(maximum)` for the
test helper (`parse.rs:257-259`, `syntax_depth_for_test`). Guardrail: the
early-abort behavior on exceeding `max_depth` (before SWC recursion) and the
pre/post-parse phase selection in `SyntaxDepthGuard::new` (parse.rs:345-352)
must be preserved.

**Fix Applied:** None so far.

#### [ ] READ-002 — Test-only `MAX_SYNTAX_DEPTH` duplicates `limits::default_syntax_depth`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/parse.rs:25-26,171-173,395-406` and `glass-lint-core/src/limits.rs:142-144,164-176`

`MAX_SYNTAX_DEPTH` is `#[cfg(test)]` and hard-coded to `512`, the same value as
`limits::default_syntax_depth()`, the production default used by
`AnalysisLimits::default()` and the serde `default = "default_syntax_depth"`
hooks. `SourceParser::new` and `syntax_depth_for_test` build their bound from
the test constant while production runs `limits.syntax_depth()`. If the
production default changes, tests will silently keep exercising a stale,
different depth bound.

**Recommendation:** Single-source the value, e.g. have the test constant derive
from `AnalysisLimits::default().syntax_depth()` (or a shared
`pub(crate) const DEFAULT_SYNTAX_DEPTH`) so the test bound and the production
default cannot drift. Guardrail: keep the tests' rejection threshold (limit + 1
parens) exactly aligned with whatever bound is used.

**Fix Applied:** None so far.

## Systemic Themes

- **Validated-config boilerplate:** `AnalysisLimits` repeats the same
  shape seven times (getter, `with_*` builder, `#[cfg(test)] set_*`,
  `default_*` const, serde default, error variant). The per-field machinery is
  already factored through `with_limit`/`set_limit`, but the test-only setter
  half of that surface conflicts with the type's documented invariant
  (READ-005).
- **Multiple fail-state representations:** Bounded parsing and limit
  validation each express "rejected" through more than one channel
  (`SyntaxDepthError` + `SyntaxDepthOutcome`, `PositiveLimit::new` returning
  `Result<_, ()>` mapped by callers, Result-then-`is_err` in
  `parse_program`). READ-003 is the concrete instance.
- **Duplicated constants and validation sequences:** `512` appears twice as
  an analysis bound (READ-002); the `validated_identifier` → `BTreeSet`
  collect appears twice in `Environment` (READ-008); the severity spelling
  lives in three places (READ-004).
- **Redundant predicates on `Environment`:** `is_promoted_global_member`
  reimplements the public `is_global_member` (READ-001).

## Open Questions

- `SourceLineIndex::new(&str)` vs `SourceLineIndex::from_text(SourceText)`
  (diagnostic.rs:122-133) are deliberately parallel constructors, and
  `diagnostic/tests.rs:61-80` asserts they agree. Resolved: `new` has no
  production callers — every production construction site uses `from_text`
  (parse.rs:213, parse.rs:311, analysis/local.rs:106,
  analysis/semantic/mod.rs:66), while `new` is exercised only by tests and the
  doc example. Keeping the borrowed constructor is a low-cost test/doc
  convenience, not a correctness concern; dropping it would leave `from_text`
  as the sole public constructor.
- `analyze_ecma_version` (ecma_version.rs:204-206) is an immediately-consumed
  wrapper over `analyze_ecma_version_with_limits` with `AnalysisLimits::default()`.
  Resolved: it is the re-exported public entry point (lib.rs:31), exercised by
  the public-surface integration test (public_surface.rs:40) and the unit
  tests; dropping it would push default-limits construction onto every
  external caller. It earns its place as the convenience vocabulary for the
  standalone public API.
- `AnalysisLimits` (Clone, `Default` + per-field builders, manual serde) and
  `ProjectAdmissionLimits` (Copy, `new` + two `with_*` builders, no serde) are
  parallel validated-limit types in one module with different construction
  surfaces and derives. The differences appear intentional (different
  consumers), but a shared shape was not pursued.
- `SourceLineIndex` retains a full copy of the source text for char-counting
  and `source_slice`, which is re-cloned at multiple construction sites
  (parse.rs:213, parse.rs:311, analysis/local.rs:106, analysis/semantic/mod.rs:66).
  This is a cost question (source is held in several copies during analysis),
  not a correctness issue, and is left for the owner to weigh.

## Coverage

Read-only review of the chunk's source and tests:

- `glass-lint-core/src/config.rs`
- `glass-lint-core/src/diagnostic.rs`, `glass-lint-core/src/diagnostic/tests.rs`
- `glass-lint-core/src/ecma_version.rs`,
  `glass-lint-core/src/ecma_version/detector.rs`,
  `glass-lint-core/src/ecma_version/tests.rs`
- `glass-lint-core/src/environment.rs`, `glass-lint-core/src/environment/tests.rs`
- `glass-lint-core/src/limits.rs`, `glass-lint-core/src/limits/tests.rs`
- `glass-lint-core/src/parse.rs`, `glass-lint-core/src/parse/depth.rs`,
  `glass-lint-core/src/parse/tests.rs`

Representative callers traced for API-surface judgments:
`glass-lint-core/src/lib.rs`, `analysis/semantic/mod.rs`, `analysis/local.rs`,
`analysis/matching/evidence.rs`, `analysis/scope/*`, `analysis/matching/query/*`,
`lint/report/files.rs`, `lint/linter.rs`, `project/session/mod.rs`,
`project/tests/status_policy.rs`, `glass-lint-js/src/lib.rs`,
`glass-lint-obsidian/src/lib.rs`, `glass-lint-cli/src/config.rs`, and
`glass-lint-core/tests/integration/public_surface.rs`. The swc_ecma_visit
default `visit_object_lit` implementation was verified in the vendored crate
sources to confirm READ-006.

Guardrails respected: no changes collapse the pre/post-parse depth phases, the
project-admission limits, the provider-hosted environment construction, or the
`PositiveLimit` invariant; all findings keep fail-closed behavior and
deterministic output intact. No speculative abstractions were proposed.
