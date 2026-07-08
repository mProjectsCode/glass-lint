# Bugs Exposed by Harness Cases

This document tracks behaviors that the migrated harness cases describe but the current engine does not satisfy yet. The default `tests/cases` suite keeps passing assertions line-based; precision gaps that would make those cases fail are summarized here or covered by `tests/cases-regressions`.

## False Positives

- No currently tracked false-positive regressions. Keep new precision regressions in `tests/cases-regressions`.

## False Negatives

- `tests/cases/system/dynamic-code-dom-injection.js`: dynamic script insertion through `script.src`, `textContent`, `setAttribute("src", ...)`, and `append(...)` currently produces no `obsidian:dynamic_code` findings.
- `tests/cases/system/dynamic-code-function-constructors.js`: direct and aliased `Function` / async-generator constructor forms currently produce no `obsidian:dynamic_code` findings.
- `tests/cases/system/dynamic-code-helper-flow.js`: dynamic script nodes passed into a direct helper that appends to `document.head` are not followed.
- `tests/cases/system/dynamic-code-string-timers.js`: `window.setInterval` with a template string callback is not reported; only the bare `setTimeout` string callback is detected.
- `tests/cases/system/remaining-static-risk-groups.js`: `obsidian:network.remote_dom_loading` does not report the remote image/script DOM loading examples.
- `tests/cases/vault/open-create-and-mutations.js`: `obsidian:vault.open_create_flows` does not report `workspace.getLeaf(false).openFile(file)`, and `obsidian:workspace.views` has no finding for the same flow.
- `tests/cases/network/commonjs-provenance.js`: most CommonJS Obsidian require variants are not followed beyond the simple namespace require call.
- `tests/cases/network/obsidian-import-provenance.js`: imported request aliases, namespace request calls, and namespace-derived aliases are incompletely located; several findings collapse onto `obsidian.requestUrl(...)` instead of their actual call sites.

## Location Precision

- Several alias-flow matches report the provenance or helper declaration line rather than the ultimate call site, for example `tests/cases/vault/aliases-and-destructuring.js` around `readFrom(this.app.vault)`.
- `tests/cases/network/shadowing-sibling-scopes.js` reports the outer `fetch("https://example.com")` finding on the earlier shadowing-helper declaration line instead of the actual call line.
- `tests/cases/system/dynamic-code-string-timers.js` currently reports the `setTimeout` finding one line later than the physical source line once harness comments are stripped.
