# glass-lint-js

`glass-lint-js` provides rules for capabilities available in common JavaScript
runtimes. Its `js:` catalog covers browser and DOM APIs, networking, Node.js,
Electron, cryptography, archives, storage, and dynamic code execution.

The crate owns the rule definitions, profile membership, default runtime
environment, and disclosure mapping for that namespace.

## Choose a profile

```rust
let linter = glass_lint_js::recommended_linter();
let report = linter.lint(source, "bundle.js");
```

`recommended_linter()` selects high-confidence rules intended for normal use.
`heuristic_linter()` selects the complete catalog, including broader literal
and syntactic discovery. `rule_catalog()` returns serializable metadata for
every rule, regardless of profile.

Rule IDs are namespaced with `js:`, for example `js:network.request`.

## Describe the runtime

`default_environment()` models the combined browser, Node.js, and Electron
globals expected by this catalog. If the analyzed host injects more names,
extend that environment before constructing the linter:

```rust
let mut environment = glass_lint_js::default_environment();
environment.add_global("customRuntimeApi")?;
environment.add_global_object_with_members("activeWindow", ["eval", "fetch"])?;

let linter = glass_lint_js::recommended_linter_with_environment(environment);
```

Use `add_global_object` for an unrestricted current-realm global alias. Use
`add_global_object_with_members` when another realm should expose only a known
set of identities. The no-argument constructors use the crate's default
environment.

## Derive disclosures

`disclosures_for_report(&report)` returns a sorted set of stable disclosure
identifiers implied by `js:` findings. Findings from other namespaces are
ignored.

Rule fixtures live beside their implementations. See the repository
[testing guide](../TESTING.md) for authoring and validation conventions.
