# Bugs Exposed by Harness Cases

This document tracks behaviors that the migrated harness cases describe but the current engine does not satisfy yet. The default `tests/cases` suite keeps passing; known false negatives that would otherwise require failing expectations are summarized here or covered by `tests/cases-regressions`.

## False Positives

- No currently tracked false-positive regressions. Keep new precision regressions in `tests/cases-regressions`.

## False Negatives

- `tests/cases/system/dynamic-code-dom-injection.js`: dynamic script insertion through `script.src`, `textContent`, `setAttribute("src", ...)`, `append(...)`, and DOM insertion currently produces no `obsidian:dynamic_code` findings.
- `tests/cases/system/dynamic-code-helper-flow.js`: dynamic script nodes passed into a direct helper that appends to `document.head` are not followed by `obsidian:dynamic_code`.
- `tests/cases/system/remaining-static-risk-groups.js`: `obsidian:network.remote_dom_loading` does not report the remote image/script DOM loading examples built with `document.createElement`, `src`, and DOM insertion.

## Location Precision

- No currently tracked location-precision regressions.
