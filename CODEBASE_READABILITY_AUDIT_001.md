# Codebase Readability Audit — Chunk 01

## Summary

Chunk 01 builds the matcher-independent fact stream and module interface from
one source-order SWC traversal. The overall boundary is sound: facts are
shared across rules, the stream is phase-typed, and provenance joins are
centralized. The findings below target duplicated work and split ownership in
the traversal/interface boundary; none require changing provider neutrality,
path-local certainty, or fail-closed behavior.

## Findings

### Fact traversal and module-interface bookkeeping

#### [ ] READ-001 — Export declaration collection registers locals twice

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/interface/mod.rs:36-45`; `glass-lint-core/src/analysis/facts/interface/exports.rs:14-65`; `glass-lint-core/src/analysis/facts/visitor.rs:72-86,436-442`; `glass-lint-core/src/analysis/facts/functions.rs:104-106,182-184`

`record_export_decl` uses `record_pattern_locals` for exported variables and
calls `record_local` for exported classes/functions, then the same declaration
is visited normally and the fact visitor records those locals again. The
`BTreeSet` makes the duplicate writes harmless, but it still repeats pattern
walks and spreads the ownership of the local-binding invariant across the
export prepass and ordinary fact traversal.

**Recommendation:** Make the normal visitor the sole owner of `ModuleInterface`
local registration. Let export collection use a side-effect-free pattern-name
collector when it needs names for export entries, and remove its local writes;
the existing `record_pattern_locals` call in `visit_var_declarator` and the
function/class visitor paths then remain the single registration points.
Preserve export entries, function IDs, static-string metadata, and the current
behavior for anonymous default declarations.

**Fix Applied:** None so far.

#### [ ] READ-002 — CommonJS uncertainty is split between two owners

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/assignments.rs:18-35`; `glass-lint-core/src/analysis/facts/interface/commonjs.rs:12-52`

`FactBuilder::record_assignment` performs a direct `exports = ...`/`module =
...` unshadowed-name check to mark the interface unknown, and then delegates
to `ModuleInterfaceBuilder::record_commonjs_export`, which independently
recognizes member-shaped CommonJS writes. The split makes one module-interface
invariant depend on caller-side resolver checks and invites future assignment
forms to update only one path.

**Recommendation:** Move the direct-name invalidation into the interface
builder's single CommonJS assignment operation and have the fact builder call
that operation once. Keep the resolver's unshadowed-name checks, the `Assign`
operator restriction, and all existing member/property export recognition;
only the ownership and duplicated dispatch should change.

**Fix Applied:** None so far.

### Fact argument and call dispatch

#### [ ] READ-003 — Call arguments are traversed once for facts and again for projections

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/calls/mod.rs:53-70,107-123`; `glass-lint-core/src/analysis/facts/visitor.rs:145-177`; `glass-lint-core/src/analysis/facts/arguments.rs:28-69,119-212`

Ordinary and optional calls first run `call.args.visit_with(self)` to emit
nested semantic facts, then `emit_call` calls `args_info`, whose `arg_info`
recursively walks object/array argument trees again through
`analyze_argument_tree`. This is more than a forwarding wrapper: bundled
literal arguments pay for two syntax walks and two sets of child dispatches in
the canonical per-file pass.

**Recommendation:** Give call handling one argument-walk operation that emits
nested facts and returns `CallArgInfo` for the same nodes, then use that result
when constructing the call fact. A shared private routine can preserve
ordinary calls, optional calls, `.call`/`.apply`, spread flags, static object
shapes, and dynamic fallback while removing either the separate visitor walk
or the second `analyze_argument_tree` walk. Do not merge away child fact order
or the distinction between incomplete static shapes and precise values.

**Fix Applied:** None so far.

#### [ ] READ-004 — Ordinary and optional calls duplicate dispatch policy

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/calls/mod.rs:15-74`; `glass-lint-core/src/analysis/facts/visitor.rs:145-195`

`record_call_expr` and `visit_opt_chain_expr` each decide whether a callee is
a `.call`/`.apply` wrapper, visit callee/argument children, resolve the
callee, and emit either a wrapped or ordinary call. The optional path has a
second copy of this policy, so changes to effective-callee handling or module
request emission can diverge between syntactically equivalent calls.

**Recommendation:** Normalize both syntax forms at the visitor boundary into
the small set of call inputs needed by a shared private `record_call_like`
routine (span, callee expression, arguments, and optional module request), or
otherwise share that routine without introducing a public wrapper type. Keep
the optional-member fact span, import-fact timing, and unsupported wrapper
behavior unchanged.

**Fix Applied:** None so far.

### Literal resolution boundary

#### [ ] READ-005 — Literal visitors clone AST nodes for immediate resolution

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/visitor.rs:216-237,240-271`; `glass-lint-core/src/analysis/resolution/expression.rs:226-260`

`visit_str` clones a `Str` into a temporary `Expr::Lit` solely to call
`Resolver::resolve_expr`, and `visit_tpl` clones the complete template into
`Expr::Tpl` for the same reason. The resolver already has literal-specific
branches and the visitor immediately consumes the resulting ID, so these
owned AST wrappers add allocation/copy work on every literal/template without
providing a semantic boundary.

**Recommendation:** Add narrow borrowed-node resolver entry points (for
example, string-literal and template resolution) or a shared resolver helper
that performs the existing branches without constructing an `Expr`. Preserve
the value-arena identity, static-string origin location, quasi fallback, and
the existing behavior for dynamic interpolations.

**Fix Applied:** None so far.

## Systemic Themes

- The fact stream and provenance state are appropriately private and
  fail-closed; no newtype or storage exposure is recommended for them in this
  chunk.
- The strongest repeated signal is a syntax visitor doing both semantic
  collection and secondary projection work. Future reviews should look for
  `visit_with` followed by a second recursive expression walk for the same
  subtree.
- Interface mutation should remain behind the module-interface builder during
  syntax collection. Caller-side checks that decide whether the interface is
  known or unknown are a signal for an ownership finding.

## Open Questions

- A one-pass argument projection may need a small internal result object to
  carry both emitted-child state and `CallArgInfo`; verify that this does not
  expose matcher-specific data to the provider-neutral fact stream.
- Before changing export local registration, confirm that any declaration
  kinds not visited by the normal fact visitor still need to populate
  `ModuleInterface::locals`.

## Coverage

Reviewed the chunk-01 structure entries and their implementation/test support:

- `analysis/facts/mod.rs`
- `analysis/facts/visitor.rs`
- `analysis/facts/arguments.rs`
- `analysis/facts/assignments.rs`
- `analysis/facts/call_results.rs`
- `analysis/facts/calls/{mod,callee,wrapper}.rs`
- `analysis/facts/control.rs`
- `analysis/facts/functions.rs`
- `analysis/facts/instance.rs`
- `analysis/facts/interface/{mod,commonjs,exports}.rs`
- `analysis/facts/model.rs`
- `analysis/facts/origin_map.rs`
- `analysis/facts/pattern.rs`
- `analysis/facts/state.rs`
- `analysis/facts/stream.rs`
- `analysis/facts/tests/*.rs`

No source, test, configuration, dependency, or other documentation files were
changed by this audit.
