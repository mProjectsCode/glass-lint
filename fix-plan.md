# Fix guidance for the second declarative matcher audit

## Baseline and scope

Commit `8de1da5` fixes the previous declarative regressions, and the clean baseline passes `make ci`:

- all workspace unit, integration, and documentation tests pass;
- all 14 end-to-end cases pass;
- all 70 JavaScript and 98 Obsidian provider cases pass;
- generated rules and core examples pass their checks.

This audit adds seven focused failing tests to `glass-lint-core/tests/integration/matching/declarative.rs`. The focused declarative suite now reports 69 passed and 7 failed. All seven are false negatives; no production code is changed in this audit.

Keep the implementation provider-neutral and inside `glass-lint-core`. Extend the existing scope, fact, resolution, and matcher-independent models rather than adding rule callbacks or selected-rule traversal.

## 1. Emit compound member assignments as property writes

Failing test:

- `compound_assignments_are_rooted_property_writes`

Observed behavior: `document.cookie += suffix` and `document.cookie ||= fallback` produce no `property_write_rooted("document.cookie")` findings.

Root cause: `FactBuilder::record_member_assignment` emits `FactPayload::PropertyWrite` only for `AssignOp::Assign`. Every compound or logical assignment is lowered to a generic `FactPayload::Assignment` with an unknown source and receiver. That generic fact invalidates flow state but is not indexed as a property-write occurrence, even though the source performs a statically rooted member write. It also invalidates the entire receiver object instead of the written property.

Implementation guidance:

1. In `analysis/facts/assignments.rs`, use one member-write path for all `AssignOp` variants after evaluating the receiver, computed key, and RHS in source order.
2. Preserve the current precise RHS value only for plain `=`. For arithmetic, bitwise, and logical compound assignments, store `ValueId::UNKNOWN` because the resulting property value depends on the prior value and, for logical assignments, runtime control.
3. Still record the static property name and the write-specific canonical rooted chain for compound assignments. Dynamic computed keys must remain unknown and must not match a rooted property query.
4. Do not also emit the old receiver-wide generic assignment for the same member write. `FactPayload::PropertyWrite` already gives the flow projector enough information to clear the affected property requirement; double emission would over-invalidate state and perturb operation counts.
5. Keep identifier compound assignments on the existing generic assignment path. This change is specifically about statically identified member writes.

Required lower-layer coverage:

- fact tests for `=`, `+=`, `||=`, `??=`, and a dynamic computed key;
- exact rooted paths through direct globals, aliases, and static computed properties;
- receiver reassignment and shadowed/local lookalike negatives;
- flow tests proving a compound write clears only the affected property requirement and cannot satisfy a static-value matcher from its unknown result;
- deterministic fact/evidence order and exact assignment spans.

Acceptance: the failing test reports two property-write findings, while compound writes never invent a static assigned value or a rooted identity.

## 2. Canonicalize recursive ESM binding and member provenance

Failing tests:

- `named_default_imports_share_default_import_member_semantics`
- `default_import_bound_callables_preserve_default_export_identity`
- `esm_export_bound_callables_preserve_module_provenance`
- `extracted_deep_default_import_members_preserve_module_provenance`
- `extracted_named_export_members_preserve_deep_module_provenance`

These are one semantic gap, not five special cases.

### Root causes

`for_each_import_binding` creates `BindingProvenance::DefaultImport` only for `import Default from "sdk"`. The equivalent `import { default as Default } from "sdk"` becomes `ModuleExport { export: "default" }`, so the two syntaxes diverge for namespace-like member access.

`ScopeCollector::module_alias_provenance` projects one member from a namespace/default import, but it does not append a static property to an existing `ModuleExport`. Consequently, an extracted deep callable such as `const send = client.send` loses `sdk:client.send` identity whether `client` came from a named export or a default import.

`ScopeCollector::bound_callable_provenance` requires `rooted_name_path(member.obj)` before checking module provenance. ESM bindings deliberately are not rooted globals, so `send.bind(...)` and `DefaultExport.bind(...)` return early and never become `BoundModuleCallable` values.

### Implementation guidance

1. Normalize both default-import syntaxes at import binding construction. A named import whose imported name is exactly `default` must use the same internal `DefaultImport` representation as `ImportSpecifier::Default`. Do not branch later on source syntax.
2. Give the scope model one recursive operation for projecting a static member from module provenance:
   - namespace/default-import plus `member` becomes `ModuleExport(module, member)` under the repository's existing default-import member contract;
   - `ModuleExport(module, export)` plus `member` becomes `ModuleExport(module, export.member)`;
   - a static `.bind` access preserves the underlying callable export rather than appending `bind`;
   - dynamic properties, unsupported optional shapes, ambiguity, and exhausted name/path storage fail closed.
3. Use that operation from `module_alias_provenance`, member-value seeding, destructuring/extracted alias collection, callable resolution, and class/constructor provenance. Do not create parallel shallow and deep module-member algorithms.
4. Refactor `bound_callable_provenance` to resolve module provenance before demanding a rooted target:
   - `ModuleExport` maps directly to `BoundModuleCallable` with its complete export path;
   - `DefaultImport` maps to `BoundModuleCallable` with export `default`;
   - only the non-module fallback requires a rooted target and produces `BoundCallable`.
5. Preserve bounded static bound arguments exactly as today. Reassignment of the source export or bound alias must invalidate later strict calls.
6. Keep direct namespace/default-import member behavior and project overlay keys consistent with the same canonical export string. Deep paths such as `client.send` must not be split differently between local matching and project linking.

Required lower-layer and adversarial coverage:

- binding tests proving the two default-import syntaxes normalize identically;
- direct, extracted, destructured, deep, computed-static, `.bind`, `.call`, and `.apply` positives for namespace, default, named-default, and named-export imports;
- CommonJS and interop forms as regression controls;
- reassignment before/after extraction, local lookalikes, dynamic properties, lexical shadowing, and incompatible-branch negatives;
- exact module/export keys and evidence locations for a deep alias;
- a project-linking case that carries a deep export through an explicit resolution record;
- deterministic behavior at path/name budgets.

Acceptance:

- both default import syntaxes produce the same two `sdk:send` member findings;
- bound default and named ESM exports retain their original export identity;
- extracted `sdk.client.send` callables match `call_module("sdk", "client.send")` for default and named-export roots;
- strict module queries still reject reassigned, dynamic, and local same-name values.

## 3. Admit bounded static expressions as dynamic-import specifiers

Failing test:

- `import_queries_match_static_template_dynamic_imports`

The test covers both ``import(`sdk`)`` and `import("s" + "dk")`.

Root cause: `record_module_call_request` accepts only `Expr::Lit(Lit::Str)` as a dynamic-import argument. The existing bounded constant evaluator and resolver can already prove both test expressions equal the static string `sdk`, but module request construction and import-fact emission bypass that semantic value path. The import query therefore sees no occurrence, and project resolution would receive no request.

Implementation guidance:

1. Add one provider-neutral helper for a bounded static module specifier. It should use `analysis/syntax/constant` with the owning scope/resolver lookup and return the proven string plus the original argument span.
2. Use the helper consistently in:
   - `FactBuilder::record_module_call_request` for `ResolutionRequestKind::DynamicImport`;
   - `FactPayload::Import` emission for local import queries;
   - `ScopeCollector::module_alias_provenance` for values returned by `import(...)`/`await import(...)`;
   - any project interface path that currently assumes an AST `Str` node.
3. Change `ModuleInterfaceBuilder::record_import_request` to accept the validated specifier value and span rather than an AST string node. SWC types should not become part of the retained interface.
4. Accept only complete bounded static strings. Unknown identifiers, reassigned aliases, dynamic template substitutions, unsupported coercions, spreads, and exhausted evaluation must produce neither an import fact nor a resolution request.
5. Emit exactly one import fact and one resolution request per dynamic import. Anchor evidence at the complete argument expression, not an invented literal span or each component literal.
6. Preserve the optional second dynamic-import options argument; only the first argument determines the module specifier.

Required lower-layer and adversarial coverage:

- literal, no-substitution template, constant template substitution, string addition, and stable constant-alias positives;
- dynamic/reassigned alias, unknown template substitution, oversized string, non-string, and spread negatives;
- exact/package query behavior for subpaths;
- bare, awaited, parenthesized, and assigned dynamic-import expressions without duplicate facts;
- module-interface tests asserting the exact request kind, specifier, and source span;
- a virtual-project linking test for a statically composed relative specifier;
- stable fact fingerprints and operation counts.

Acceptance: the failing test reports two exact import findings for `sdk`, while non-static expressions remain absent from both matcher indexes and project resolution requests.

## Suggested implementation order

1. Canonicalize module provenance first, because dynamic-import return values also consume module provenance.
2. Add the shared static-module-specifier helper and migrate fact/interface/scope consumers together.
3. Change compound member assignments to emit property-write facts and update flow invalidation tests.
4. Run the focused suite:

   ```sh
   cargo test -p glass-lint-core --test integration matching::declarative
   ```

5. Run `cargo test -p glass-lint-core`, then the full `make ci` gate.

## Completion checklist

- All seven new declarative tests pass.
- Existing default-import, optional-call, class, constructor, literal-composition, and direct property-write regressions remain green.
- Matching remains query-independent and fact construction still runs once per file.
- No dynamic, reassigned, ambiguous, shadowed, or budget-exhausted value establishes strict module/rooted provenance.
- Evidence counts, locations, certainty, fact fingerprints, and operation counts are deterministic.
- Provider fixtures and generated `RULES.md` remain unchanged unless behavior intentionally adds a provider finding that requires an explicit fixture expectation.
- `make ci` passes.
