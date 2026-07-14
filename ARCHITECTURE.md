# Architecture

Glass Lint separates generic JavaScript analysis from provider policy. The
core parses and analyzes each file once, providers describe capabilities with
declarative matchers, and front ends select rules and serialize reports.

## Package boundaries

```text
                    +----------------------+
                    |   glass-lint-cli     |
                    |  glass-lint binary   |
                    +----------+-----------+
                               |
             +-----------------+-----------------+
             |                                   |
             v                                   v
 +----------------------+             +----------------------+
 | glass-lint-harness   |             | provider crates      |
 | cases, adapters,     |------------>| glass-lint-js        |
 | reports, profiling   |             | glass-lint-obsidian  |
 +----------+-----------+             +----------+-----------+
             |                                   |
             +-----------------+-----------------+
                               v
                    +----------------------+
                    | glass-lint-core      |
                    | parser, semantics,   |
                    | matchers, reports    |
                    +----------------------+
```

### `glass-lint-core`

Core is provider-neutral. It owns:

- JavaScript and JSX parsing
- lexical scopes, bindings, shadowing, and reassignment history
- import, CommonJS, global, rooted-chain, alias, and value provenance
- the shared semantic fact stream and bounded flow analysis
- matcher validation, normalization, compilation, and execution
- validated host-environment configuration for global bindings plus
  unrestricted current-realm and member-restricted foreign-realm global
  objects
- rule catalogs, rule selection, deterministic findings, and diagnostics

Core must not contain Obsidian module names, API knowledge, categories,
manifest fields, disclosure mappings, or profile policy. Likewise, generic
JavaScript, browser, Node.js, and Electron policy belongs in
`glass-lint-js`, not in core.

### Provider crates

`glass-lint-js` and `glass-lint-obsidian` construct rules through the public
matcher API and expose complete and recommended `Linter` configurations. A
provider owns its rule names, descriptions, confidence assignments,
categories, disclosures, and any narrowly scoped custom policy.

Rules should be declarative whenever the matcher API can express the intended
semantics accurately. Extend the generic matcher vocabulary when a behavior is
reusable. Provider-specific Rust callbacks are reserved for semantic rules
that cannot be represented faithfully as generic matchers.

Provider crates also own host assumptions. Core supplies a conservative
ECMAScript environment only; browser, Node.js, Electron, Obsidian, and other
runtime globals are declared by provider defaults and may be extended by
library callers. Environment configuration is attached to a `RuleCatalog` and
used by the shared semantic pass, not embedded in individual matchers.
Member-restricted global objects prevent APIs injected into one realm from
being inferred on another window-like realm.

When generic JavaScript and provider rules run as one profile, they share the
provider's host environment. Running the generic JavaScript catalog alone uses
only its own browser/Node/Electron default.

### Harness and CLI

`glass-lint-harness` loads annotated JavaScript cases, invokes built-in or
external adapters, checks diagnostic expectations, produces reports, and
profiles folders. It depends on providers to offer the built-in Glass Lint
adapter but does not implement lint semantics.

`glass-lint-cli` is deliberately thin. It owns argument parsing, configuration,
filesystem discovery, human/JSON output, process exit behavior, and the
`glass-lint` executable. `glass-lint-harness-cli` owns the harness executable;
reusable harness behavior stays in `glass-lint-harness`.

## Per-file analysis pipeline

```text
source
  -> parse JavaScript/JSX once
  -> collect lexical scopes and declarations
  -> emit matcher-independent semantic facts
  -> resolve identities, constants, aliases, calls, and value flow
  -> build shared indexes
  -> query compiled matchers for selected rules
  -> group bounded evidence into findings
  -> sort findings by location and rule ID
```

The selected rule set must not change semantic fact construction or add AST
traversals. Shared analysis is built once per file, then queried by every
enabled rule.

The main core layers are:

- `analysis/syntax`: small AST naming, constant, and provenance helpers
- `analysis/scope`: lexical model, collection, and binding/provenance queries
- `analysis/facts`: matcher-independent semantic events emitted from the AST
- `analysis/resolution`: expression, call, and constant resolution
- `analysis/value`: stable value identities and arenas
- `analysis/flow`: bounded state projection and summary-based flow matching
- `analysis/matching`: occurrence indexes and evidence queries
- `api/rule`: validated public rules and declarative matcher types
- `api/compiler`: immutable matcher plans compiled at catalog construction
- `lint`: catalog validation, rule selection, and report construction

## Rules and profiles

Provider rule factories use local IDs such as `network.request`. A
`RuleCatalog` validates them and adds the provider namespace, producing IDs
such as `js:network.request`. Catalogs reject duplicate or malformed IDs.

Every rule declares a confidence level:

- `High` rules enter the provider's `recommended_linter()`.
- The provider's `heuristic_linter()` enables the complete catalog.

Confidence describes the strength of the matching mechanism, not the
importance of the detected behavior. A broad name-only matcher can still be
useful for discovery, but it must require an explicit heuristic opt-in. The
Obsidian-specific promotion policy is documented in
[`glass-lint-obsidian/ACCURACY.md`](glass-lint-obsidian/ACCURACY.md).

## Precision and failure behavior

The engine is precision-first:

- strict matches require lexical identity or supported provenance at the use
  position;
- unbound names are global only when the catalog environment declares them;
- local lookalikes and shadowed globals must not match;
- reassignment invalidates provenance from that point forward;
- dynamic or unsupported semantics fail closed;
- evidence and source sizes are bounded; and
- output ordering and source locations are deterministic.

These constraints are architectural invariants. A new matcher that cannot
prove identity should be explicitly named and classified as heuristic rather
than silently weakening a strict matcher.

## Public API design

The public extension path is `glass_lint_core::rules`: build validated rules,
place them in a namespaced `RuleCatalog`, and pass that catalog to `Linter`.
Internal AST, scope, fact, and index types remain private so providers cannot
couple themselves to a parallel analysis model.

Breaking changes are currently allowed when they simplify the design. A clean
break must update every workspace caller, fixture, adapter, schema consumer,
and document in the same change; compatibility wrappers are not retained by
default.
