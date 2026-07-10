# Glass Lint Development Notes

## Architecture

- `glass-lint-core` is the generic JavaScript lint engine. It owns parsing, rule registration, matcher execution, provenance tracking, scope and shadowing analysis, alias flow, symbol indexing, generic reports, and reusable extension APIs.
- `glass-lint-core` must not contain Obsidian-specific module names, API knowledge, categories, manifests, disclosures, or rule policy.
- `glass-lint-obsidian` is a provider package. It owns Obsidian rule definitions, profiles, disclosure mappings, bundle analysis, and the small number of provider-specific custom rule callbacks.
- Obsidian rules should use the declarative matcher API from core whenever possible. Use custom Rust callbacks only for semantic rules that cannot be expressed accurately through the declarative API.
- Rule IDs use `provider:name`, for example `js:network.request`.
- Accuracy is precision-first. Provenance-aware and connected-flow matches are preferred over raw names or substrings. Broad heuristics belong in the opt-in heuristic profile.

## Code Quality

- Keep files focused and reasonably sized. Do not grow monolithic modules; split code by responsibility before a file becomes difficult to navigate or review.
- Prefer structs with clear invariants and member methods over collections of loosely related free functions.
- Keep public APIs small, validated, and difficult to misuse. Require callers to opt in explicitly to weak matching modes.
- Parse and build shared semantic indexes once per file. Avoid repeated AST traversals where analysis can be cached or combined.
- Preserve deterministic output ordering and source locations.
- Avoid duplicated parsers, matcher implementations, report models, or parallel APIs that solve the same problem.
- Add focused positive and adversarial negative tests for matching behavior, especially shadowing, local lookalikes, aliases, reassignment, minified bundles, and module provenance.
- Run formatting, workspace tests, Clippy with warnings denied, and relevant harness suites before considering a change complete.

## Development Status

- The project is still under active development. Breaking Rust APIs, JSON schemas, rule IDs, and internal layouts is allowed when it produces a cleaner architecture.
- Do not retain compatibility wrappers or deprecated abstractions unless explicitly requested.
- When making a clean break, update all workspace callers, fixtures, adapters, documentation, and tests in the same change.
