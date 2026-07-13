# Rule and test audit record

This document records the completed precision and fixture audit for the rules
registered by the JavaScript (`js`) and Obsidian (`obsidian`) providers at the
time of the audit. Keep it as historical evidence; use
[`TESTING.md`](TESTING.md) for current test-authoring guidance.

If new unaudited rules are appended, group them in sets of 5–10 and follow the
protocol below.

## Audit instructions

An auditor picks the first incomplete group, audits every rule in that group,
and then stops. For each rule:

1. Read the rule implementation and its existing tests.
2. Determine the rule's intended behavior, including its precision boundaries and relevant provenance, alias, reassignment, and shadowing behavior.
3. Document that intent in a Rust doc comment on the rule factory or the most relevant public rule-definition item.
4. Reorganize the rule's tests (the content of `positive.js` and `negative.js` files) so the intent is clear, and add focused positive and negative coverage for it.

Tests that deliberately fail are allowed when they demonstrate a real, documented weakness in a rule. Keep them focused and make both the expected failure and the gap it exposes explicit. Mark every rule and its audit group complete only after the audit work is complete.

Each rule entry links to its directory, which contains `mod.rs` (the implementation) and normally `positive.js` and `negative.js` (the rule fixtures). Start the audit there; trace into the core matcher only when the rule's behavior requires it.

## Audit protocol

### Ownership and ordering

Before editing, claim one incomplete group by recording the auditor and date in its group record. Do not begin a second group until the first is complete. In concurrent work, a claimed group is unavailable to other auditors; choose the next unclaimed incomplete group instead of relying on a race to the first unchecked heading.

### Per-rule record

Every rule has an `Audit:` record immediately below it. For an incomplete rule, fill in its matcher classification (`rooted`, `module provenance`, `heuristic`, `flow`, or `custom`) and replace every `[ ]` only after the corresponding work is complete. Coverage should name the exercised boundaries that apply: direct match, alias, shadowing, reassignment, lookalike, dynamic/static values, imports, and flow lifecycle. Record deliberate gaps precisely in `limitation`.

### Completion gate

Check a rule only after its implementation comment and fixture record are complete. Check a group only when every rule is checked and the following commands have passed:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Also run the targeted rule directories with `cargo run -p glass-lint-cli --bin glass-lint-harness -- verify <rule-directory>`. Run `make test-rules` before closing a JavaScript or Obsidian group. An unrelated failing fixture does not block the group only when its path and exact error are recorded below; never use this exception for a failure in the audited group.

### Group records

Record ownership and completion evidence directly below each group heading using this form:

`Group audit: owner=<name>; claimed=<YYYY-MM-DD>; targeted-fixtures=[ ]; workspace-tests=[ ]; clippy=[ ]; full-suite=[ ]; exception-log=[none|reference]`

## JavaScript rules (30)

- [x] **Audit group 1 — JavaScript browser foundations (8 rules)**
  - Group audit: owner=Codex; claimed=2026-07-10; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[ ]; exception-log=network/eval parse error
  - [x] `js:archive.compression` — [`glass-lint-js/src/rules/node/archive_compression/`](glass-lint-js/src/rules/node/archive_compression/)
    - Audit: module provenance; intent-doc=[x]; coverage=listed packages, similar module, shadowed loader; limitation=reports imports rather than later API use; verified=targeted fixtures
  - [x] `js:browser.clipboard-read` — [`glass-lint-js/src/rules/browser/clipboard_read/`](glass-lint-js/src/rules/browser/clipboard_read/)
    - Audit: rooted; intent-doc=[x]; coverage=direct calls, aliases, shadowing, reassignment; limitation=read and readText only; verified=targeted fixtures
  - [x] `js:browser.clipboard-write` — [`glass-lint-js/src/rules/browser/clipboard_write/`](glass-lint-js/src/rules/browser/clipboard_write/)
    - Audit: rooted; intent-doc=[x]; coverage=direct calls, aliases, shadowing, reassignment; limitation=write and writeText only; verified=targeted fixtures
  - [x] `js:browser.environment` — [`glass-lint-js/src/rules/browser/environment/`](glass-lint-js/src/rules/browser/environment/)
    - Audit: heuristic; intent-doc=[x]; coverage=configured reads, local lookalike, unlisted and dynamic properties; limitation=local same-chain lookalikes report; verified=targeted fixtures
  - [x] `js:browser.file-dialog` — [`glass-lint-js/src/rules/browser/file_dialog/`](glass-lint-js/src/rules/browser/file_dialog/)
    - Audit: flow; intent-doc=[x]; coverage=source, alias, static computed write, setAttribute, reassignment; limitation=bounded direct flow only; verified=targeted fixtures
  - [x] `js:browser.global-input-hook` — [`glass-lint-js/src/rules/browser/global_input_hook/`](glass-lint-js/src/rules/browser/global_input_hook/)
    - Audit: heuristic; intent-doc=[x]; coverage=receivers, static event aliases, shadowing, excluded/dynamic events; limitation=local same-chain lookalikes report; verified=targeted fixtures
  - [x] `js:browser.permissions-bluetooth` — [`glass-lint-js/src/rules/browser/permissions_bluetooth/`](glass-lint-js/src/rules/browser/permissions_bluetooth/)
    - Audit: rooted; intent-doc=[x]; coverage=direct calls, aliases, shadowing, reassignment; limitation=requestDevice only; verified=targeted fixtures
  - [x] `js:browser.permissions-geolocation` — [`glass-lint-js/src/rules/browser/permissions_geolocation/`](glass-lint-js/src/rules/browser/permissions_geolocation/)
    - Audit: rooted; intent-doc=[x]; coverage=direct calls, aliases, shadowing, reassignment; limitation=getCurrentPosition only; verified=targeted fixtures

- [x] **Audit group 2 — JavaScript browser and dynamic code (8 rules)**
  - Group audit: owner=Codex; claimed=2026-07-10; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[x]; exception-log=reference
  - Exception log: `make test-rules` passed all 60 JavaScript cases but reports the pre-existing unrelated Obsidian failures in `vault/resource_url/positive` (expected 1, found 0), `workspace/leaf_management/positive` (expected 1, found 0), and `workspace/open/positive` (expected 1, found 0).
  - [x] `js:browser.permissions-media` — [`glass-lint-js/src/rules/browser/permissions_media/`](glass-lint-js/src/rules/browser/permissions_media/)
    - Audit: rooted; intent-doc=[x]; coverage=direct calls, aliases, shadowing, reassignment, static constraints; limitation=getUserMedia only; verified=targeted fixtures
  - [x] `js:browser.permissions-notifications` — [`glass-lint-js/src/rules/browser/permissions_notifications/`](glass-lint-js/src/rules/browser/permissions_notifications/)
    - Audit: rooted; intent-doc=[x]; coverage=direct calls, aliases, shadowing, reassignment; limitation=requestPermission only; verified=targeted fixtures
  - [x] `js:browser.persistent-storage` — [`glass-lint-js/src/rules/browser/persistent_storage/`](glass-lint-js/src/rules/browser/persistent_storage/)
    - Audit: rooted; intent-doc=[x]; coverage=direct calls, aliases, shadowing, reassignment, listed methods, unlisted lookalike; limitation=only six configured methods; verified=targeted fixtures
  - [x] `js:crypto.operation` — [`glass-lint-js/src/rules/node/crypto_operation/`](glass-lint-js/src/rules/node/crypto_operation/)
    - Audit: module provenance/heuristic; intent-doc=[x]; coverage=all listed imports, similar module, shadowed loader, static Web Crypto call, unlisted method; limitation=reports imports rather than later API use and heuristic chains are syntactic; verified=targeted fixtures
  - [x] `js:dom.remote-resource` — [`glass-lint-js/src/rules/browser/remote_resource/`](glass-lint-js/src/rules/browser/remote_resource/)
    - Audit: flow; intent-doc=[x]; coverage=script/image source, alias, static computed write, setAttribute, insertion sink, dynamic/local values, unsupported tag; limitation=bounded direct flow and supported sinks only; verified=targeted fixtures
  - [x] `js:dynamic-code.eval` — [`glass-lint-js/src/rules/general/eval/`](glass-lint-js/src/rules/general/eval/)
    - Audit: global callable; intent-doc=[x]; coverage=direct eval, global-object access, aliases, bind/call/apply, Function call/construction, derived async Function constructor aliases, shadowing, reassignment, and property mutation; limitation=dynamic metaprogramming and unresolved apply containers fail closed; verified=targeted fixtures and core provenance tests
  - [x] `js:dynamic-code.script-injection` — [`glass-lint-js/src/rules/browser/script_injection/`](glass-lint-js/src/rules/browser/script_injection/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct creation, alias, static concatenation, dynamic/static tag boundaries; limitation=does not prove document provenance and reports creation without insertion; verified=targeted fixtures
  - [x] `js:dynamic-code.string-timer` — [`glass-lint-js/src/rules/general/string_timer/`](glass-lint-js/src/rules/general/string_timer/)
    - Audit: global callable; intent-doc=[x]; coverage=setTimeout/setInterval, global-object access, aliases, call/apply, shadowing, reassignment, property mutation, static strings, and callback/dynamic values; limitation=dynamic metaprogramming and unresolved apply containers fail closed; verified=targeted fixtures

- [x] **Audit group 3 — JavaScript Electron and browser/general network (7 rules)**
  - Group audit: owner=Codex; claimed=2026-07-10; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[x]; exception-log=reference
  - Exception log: `make test-rules` passed all 61 JavaScript cases; its unrelated Obsidian suite failures remain in `vault/resource_url/positive` (expected 1, found 0), `workspace/leaf_management/positive` (expected 1, found 0), and `workspace/open/positive` (expected 1, found 0).
  - [x] `js:electron.dialog` — [`glass-lint-js/src/rules/electron/dialog/`](glass-lint-js/src/rules/electron/dialog/)
    - Audit: module provenance; intent-doc=[x]; coverage=direct calls, namespace aliases, CommonJS aliases, interop wrappers, shadowing, reassignment, lookalikes; limitation=inline require member chains are not followed; verified=targeted fixtures
  - [x] `js:electron.ipc` — [`glass-lint-js/src/rules/electron/ipc/`](glass-lint-js/src/rules/electron/ipc/)
    - Audit: module provenance; intent-doc=[x]; coverage=direct calls, namespace aliases, CommonJS aliases, interop wrappers, shadowing, reassignment, lookalikes; limitation=inline require member chains are not followed and only send/invoke are listed; verified=targeted fixtures
  - [x] `js:electron.module` — [`glass-lint-js/src/rules/electron/module/`](glass-lint-js/src/rules/electron/module/)
    - Audit: module provenance; intent-doc=[x]; coverage=ESM imports, CommonJS and interop loads, similar module, shadowed require, load-time reporting; limitation=reports the module load and does not infer later API use; verified=targeted fixtures
  - [x] `js:electron.shell` — [`glass-lint-js/src/rules/electron/shell/`](glass-lint-js/src/rules/electron/shell/)
    - Audit: module provenance; intent-doc=[x]; coverage=direct calls, namespace aliases, CommonJS aliases, interop wrappers, shadowing, reassignment, lookalikes; limitation=inline require member chains are not followed and only openExternal/openPath are listed; verified=targeted fixtures
  - [x] `js:network.header-indicator` — [`glass-lint-js/src/rules/general/header_indicator/`](glass-lint-js/src/rules/general/header_indicator/)
    - Audit: heuristic; intent-doc=[x]; coverage=marker substrings, configured casing, irrelevant context, casing lookalikes, concatenated and dynamic values; limitation=does not prove request-header use and only configured marker spellings are covered; verified=targeted fixtures
  - [x] `js:network.private-address` — [`glass-lint-js/src/rules/general/private_address/`](glass-lint-js/src/rules/general/private_address/)
    - Audit: heuristic; intent-doc=[x]; coverage=all configured markers, public and unlisted ranges, partial markers, concatenated and dynamic values; limitation=does not parse IP/URL semantics or expand unlisted private ranges; verified=targeted fixtures
  - [x] `js:network.request` — [`glass-lint-js/src/rules/browser/request/`](glass-lint-js/src/rules/browser/request/)
    - Audit: rooted/global; intent-doc=[x]; coverage=direct calls, global-object access, call/apply, rooted/global aliases, constructors, shadowing, reassignment, property mutation, local lookalikes, and static/dynamic request values; limitation=only the five configured browser APIs are covered; verified=targeted fixtures

- [x] **Audit group 4 — JavaScript general and Node.js (7 rules)**
  - Group audit: owner=Codex; claimed=2026-07-10; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[x]; exception-log=reference
  - Exception log: `make test-rules` passed all 64 JavaScript cases; its unrelated Obsidian suite failures remain in `vault/resource_url/positive` (expected 1, found 0), `workspace/leaf_management/positive` (expected 1, found 0), and `workspace/open/positive` (expected 1, found 0).
  - [x] `js:network.service-indicator` — [`glass-lint-js/src/rules/general/service_indicator/`](glass-lint-js/src/rules/general/service_indicator/)
    - Audit: module provenance/heuristic; intent-doc=[x]; coverage=all listed SDK packages and endpoint markers, similar modules, static template fragments, concatenated and dynamic values; limitation=reports module loads and literal markers without proving network use or reconstructing concatenated/dynamic values; verified=targeted fixtures
  - [x] `js:network.telemetry-indicator` — [`glass-lint-js/src/rules/general/telemetry_indicator/`](glass-lint-js/src/rules/general/telemetry_indicator/)
    - Audit: module provenance/heuristic; intent-doc=[x]; coverage=all listed SDK packages and endpoint markers, similar modules, static template fragments, concatenated and dynamic values; limitation=reports module loads and literal markers without proving telemetry use or reconstructing concatenated/dynamic values; verified=targeted fixtures
  - [x] `js:network.url-construction` — [`glass-lint-js/src/rules/general/url_construction/`](glass-lint-js/src/rules/general/url_construction/)
    - Audit: rooted/global; intent-doc=[x]; coverage=both constructors, aliases, shadowing, reassignment, static and dynamic values, URL-like lookalikes; limitation=only `URL` and `URLSearchParams` construction is covered and arguments/static methods are not analyzed; verified=targeted fixtures
  - [x] `js:node.filesystem` — [`glass-lint-js/src/rules/node/filesystem/`](glass-lint-js/src/rules/node/filesystem/)
    - Audit: module provenance; intent-doc=[x]; coverage=all listed ESM/CommonJS loads, similar modules, shadowed loader, dynamic module name; limitation=reports exact static module loads rather than later API use; verified=targeted fixtures
  - [x] `js:node.network` — [`glass-lint-js/src/rules/node/network/`](glass-lint-js/src/rules/node/network/)
    - Audit: module provenance; intent-doc=[x]; coverage=all listed ESM/CommonJS loads, similar modules, shadowed loader, dynamic module name; limitation=reports exact static module loads rather than later API use; verified=targeted fixtures
  - [x] `js:node.process-environment` — [`glass-lint-js/src/rules/node/process_environment/`](glass-lint-js/src/rules/node/process_environment/)
    - Audit: rooted; intent-doc=[x]; coverage=direct reads, root/member aliases, static computed properties, shadowing, reassignment, unlisted and dynamic properties; limitation=only `process.env` and `process.platform` reads are covered and values are not analyzed; verified=targeted fixtures
  - [x] `js:node.subprocess` — [`glass-lint-js/src/rules/node/subprocess/`](glass-lint-js/src/rules/node/subprocess/)
    - Audit: module provenance; intent-doc=[x]; coverage=all listed ESM/CommonJS loads, similar modules, shadowed loader, dynamic module name; limitation=reports exact static module loads rather than particular subprocess API use; verified=targeted fixtures

## Obsidian rules (44)

- [x] **Audit group 5 — Obsidian editor and Markdown (8 rules)**
  - Group audit: owner=Codex; claimed=2026-07-10; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[x]; exception-log=reference
  - Exception log: `make test-rules` passed all 64 JavaScript cases and all eight audited Obsidian directories; its unrelated Obsidian failures remain in `vault/resource_url/positive` (expected 1, found 0), `workspace/leaf_management/positive` (expected 1, found 0), and `workspace/open/positive` (expected 1, found 0).
  - [x] `obsidian:codemirror.extension` — [`glass-lint-obsidian/src/rules/codemirror/extension/`](glass-lint-obsidian/src/rules/codemirror/extension/)
    - Audit: module provenance; intent-doc=[x]; coverage=all four configured packages, ESM/CommonJS static loads, similar module, dynamic module name, shadowed loader; limitation=reports module loads rather than later API use; verified=targeted fixtures
  - [x] `obsidian:editor.extension` — [`glass-lint-obsidian/src/rules/editor/extension/`](glass-lint-obsidian/src/rules/editor/extension/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct this chain, static computed name, same-shaped receiver gap, alias/dynamic/near-name exclusions; limitation=does not prove an Obsidian receiver or follow aliases/reassignment; verified=targeted fixtures
  - [x] `obsidian:editor.suggest` — [`glass-lint-obsidian/src/rules/editor/suggest/`](glass-lint-obsidian/src/rules/editor/suggest/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct this chain, static computed name, same-shaped receiver gap, alias/dynamic/near-name exclusions; limitation=does not prove an Obsidian receiver or follow aliases/reassignment; verified=targeted fixtures
  - [x] `obsidian:file-manager.frontmatter-write` — [`glass-lint-obsidian/src/rules/file_manager/frontmatter_write/`](glass-lint-obsidian/src/rules/file_manager/frontmatter_write/)
    - Audit: rooted; intent-doc=[x]; coverage=direct calls, this.app/root aliases, destructured alias, static computed properties, pre/post reassignment, shadowed app, dynamic/unlisted lookalikes; limitation=arguments and callback contents are not analyzed; verified=targeted fixtures
  - [x] `obsidian:lifecycle.events` — [`glass-lint-obsidian/src/rules/lifecycle/events/`](glass-lint-obsidian/src/rules/lifecycle/events/)
    - Audit: heuristic; intent-doc=[x]; coverage=all three configured chains, static computed name, same-shaped receiver gap, alias/dynamic/near-name exclusions; limitation=does not prove an Obsidian receiver or follow aliases/reassignment; verified=targeted fixtures
  - [x] `obsidian:markdown.code-block-processor` — [`glass-lint-obsidian/src/rules/markdown/code_block_processor/`](glass-lint-obsidian/src/rules/markdown/code_block_processor/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct chain, static computed name, same-shaped receiver gap, alias/dynamic/near-name exclusions; limitation=does not prove an Obsidian receiver or follow aliases/reassignment; processor arguments are not analyzed; verified=targeted fixtures
  - [x] `obsidian:markdown.link` — [`glass-lint-obsidian/src/rules/markdown/link/`](glass-lint-obsidian/src/rules/markdown/link/)
    - Audit: module provenance; intent-doc=[x]; coverage=all three exports, ESM namespace aliases, CommonJS destructuring, similar module, dynamic module, shadowed loader, reassigned alias, local lookalike; limitation=reports configured calls without analyzing arguments or helper behavior; verified=targeted fixtures
  - [x] `obsidian:markdown.postprocessor` — [`glass-lint-obsidian/src/rules/markdown/postprocessor/`](glass-lint-obsidian/src/rules/markdown/postprocessor/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct chain, static computed name, same-shaped receiver gap, alias/dynamic/near-name exclusions; limitation=does not prove an Obsidian receiver or follow aliases/reassignment; processor arguments are not analyzed; verified=targeted fixtures

- [x] **Audit group 6 — Obsidian Markdown, metadata, and plugins (8 rules)**
  - Group audit: owner=Codex; claimed=2026-07-10; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[x]; exception-log=reference
  - Exception log: `make test-rules` passed all 64 JavaScript cases and all eight audited Obsidian directories; its unrelated Obsidian failures remain in `vault/resource_url/positive` (expected 1, found 0), `workspace/leaf_management/positive` (expected 1, found 0), and `workspace/open/positive` (expected 1, found 0).
  - [x] `obsidian:markdown.render` — [`glass-lint-obsidian/src/rules/markdown/render/`](glass-lint-obsidian/src/rules/markdown/render/)
    - Audit: heuristic; intent-doc=[x]; coverage=both configured chains, static computed methods, other receiver, alias, dynamic property, near-name exclusions; limitation=does not prove renderer provenance or follow aliases/reassignment; verified=targeted fixtures
  - [x] `obsidian:metadata.cache-read` — [`glass-lint-obsidian/src/rules/metadata/cache_read/`](glass-lint-obsidian/src/rules/metadata/cache_read/)
    - Audit: rooted; intent-doc=[x]; coverage=all six configured reads/calls, rooted aliases, static computed properties, shadowing, reassignment, dynamic/unlisted lookalikes; limitation=broad `app.metadataCache` reads can still report through dynamic/unlisted suffixes, and call arguments are not analyzed; verified=targeted fixtures
  - [x] `obsidian:metadata.events` — [`glass-lint-obsidian/src/rules/metadata/events/`](glass-lint-obsidian/src/rules/metadata/events/)
    - Audit: rooted/flow; intent-doc=[x]; coverage=all three literal events, rooted aliases, shadowing, reassignment, dynamic event value, computed member, unsupported event; limitation=only literal first arguments and direct rooted member chains match; verified=targeted fixtures
  - [x] `obsidian:metadata.extract` — [`glass-lint-obsidian/src/rules/metadata/extract/`](glass-lint-obsidian/src/rules/metadata/extract/)
    - Audit: rooted; intent-doc=[x]; coverage=all six configured collections, rooted aliases, static computed property, shadowing, reassignment, dynamic/unlisted lookalikes; limitation=reads member chains only and does not infer collections from arbitrary `getFileCache(...)` return values; verified=targeted fixtures
  - [x] `obsidian:metadata.frontmatter-read` — [`glass-lint-obsidian/src/rules/metadata/frontmatter_read/`](glass-lint-obsidian/src/rules/metadata/frontmatter_read/)
    - Audit: rooted; intent-doc=[x]; coverage=direct read, rooted aliases, static computed properties, shadowing, reassignment, dynamic/unlisted lookalikes; limitation=does not analyze the returned frontmatter value or arbitrary cache objects; verified=targeted fixtures
  - [x] `obsidian:metadata.traversal` — [`glass-lint-obsidian/src/rules/metadata/traversal/`](glass-lint-obsidian/src/rules/metadata/traversal/)
    - Audit: flow; intent-doc=[x]; coverage=all three Object methods, both rooted maps, aliases, static computed Object member, local/dynamic/reassigned/unlisted inputs; limitation=only the first argument's proven rooted values are followed and Object receiver provenance is syntactic; verified=targeted fixtures
  - [x] `obsidian:network.request` — [`glass-lint-obsidian/src/rules/network/request/`](glass-lint-obsidian/src/rules/network/request/)
    - Audit: module provenance; intent-doc=[x]; coverage=both exports, ESM/CommonJS namespace and export aliases, shadowed loader/namespace, similar module, dynamic module, reassignment; limitation=inline `require('obsidian').member` chains and request arguments are not analyzed; verified=targeted fixtures
  - [x] `obsidian:platform.branching` — [`glass-lint-obsidian/src/rules/platform/branching/`](glass-lint-obsidian/src/rules/platform/branching/)
    - Audit: module provenance; intent-doc=[x]; coverage=all seven flags, namespace aliases, optional/static computed reads, similar module, shadowed namespace, dynamic property, reassignment, unlisted flag; limitation=reads are reported without control-flow/value analysis and destructured `Platform` exports are not followed; verified=targeted fixtures

- [x] **Audit group 7 — Obsidian plugins, storage, and UI (8 rules)**
  - Group audit: owner=Codex; claimed=2026-07-11; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[x]; exception-log=reference
  - Exception log: `make test-rules` passed all 64 JavaScript cases and all 90 Obsidian cases; no exceptions.
  - [x] `obsidian:plugins.access` — [`glass-lint-obsidian/src/rules/plugins/access/`](glass-lint-obsidian/src/rules/plugins/access/)
    - Audit: rooted; intent-doc=[x]; coverage=plugin instances, manifests, enabled state, static/dynamic keys, aliases, shadowing, reassignment, and string lookalikes; limitation=plugin IDs are not inferred from dynamic keys; verified=targeted fixtures
  - [x] `obsidian:plugins.enable-disable` — [`glass-lint-obsidian/src/rules/plugins/enable_disable/`](glass-lint-obsidian/src/rules/plugins/enable_disable/)
    - Audit: rooted mutation; intent-doc=[x]; coverage=enable/disable methods, aliases, static computed methods, shadowing, dynamic methods, near names, and local lookalikes; verified=targeted fixtures
  - [x] `obsidian:plugins.load-unload` — [`glass-lint-obsidian/src/rules/plugins/load_unload/`](glass-lint-obsidian/src/rules/plugins/load_unload/)
    - Audit: rooted plus returned-object provenance; intent-doc=[x]; coverage=manager and returned plugin lifecycle calls, keyed plugin instances, shadowing, reassignment, dynamic methods, and local lookalikes; verified=targeted fixtures
  - [x] `obsidian:storage.plugin-data-read` — [`glass-lint-obsidian/src/rules/storage/plugin_data_read/`](glass-lint-obsidian/src/rules/storage/plugin_data_read/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct call, static computed property, same-shaped receiver, reassignment, alias, dynamic property, other receiver, near-name method; limitation=does not prove plugin receiver or follow aliases, and reports same-shaped `this` calls even outside plugin code; arguments are not analyzed; verified=targeted fixtures
  - [x] `obsidian:storage.plugin-data-write` — [`glass-lint-obsidian/src/rules/storage/plugin_data_write/`](glass-lint-obsidian/src/rules/storage/plugin_data_write/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct call, static computed property, same-shaped receiver, reassignment, alias, dynamic property, other receiver, near-name method; limitation=does not prove plugin receiver or follow aliases, and reports same-shaped `this` calls even outside plugin code; arguments are not analyzed; verified=targeted fixtures
  - [x] `obsidian:ui.command` — [`glass-lint-obsidian/src/rules/ui/command/`](glass-lint-obsidian/src/rules/ui/command/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct call, static computed property, same-shaped receiver, reassignment, alias, dynamic property, other receiver, near-name method; limitation=does not prove plugin receiver or follow aliases, and reports same-shaped `this` calls outside plugin code; command descriptors are not analyzed; verified=targeted fixtures
  - [x] `obsidian:ui.menu` — [`glass-lint-obsidian/src/rules/ui/menu/`](glass-lint-obsidian/src/rules/ui/menu/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct call, static computed property, reassignment, alias, dynamic property, other receiver, near-name method; limitation=does not prove Obsidian menu provenance or follow aliases, and only the exact `menu.addMenuItem` chain is covered; arguments are not analyzed; verified=targeted fixtures
  - [x] `obsidian:ui.modal` — [`glass-lint-obsidian/src/rules/ui/modal/`](glass-lint-obsidian/src/rules/ui/modal/)
    - Audit: module provenance; intent-doc=[x]; coverage=ESM named and namespace imports, CommonJS destructuring, constructor aliases, subclass, unbound/local/shadowed/reassigned aliases, dynamic module, lookalikes; limitation=only constructor and subclass syntax is matched, and constructor arguments/class bodies are not analyzed; verified=targeted fixtures and Obsidian 1.12.7 runtime probe

- [x] **Audit group 8 — Obsidian UI and vault access (7 rules)**
  - Group audit: owner=Codex; claimed=2026-07-11; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[x]; exception-log=reference
  - Exception log: `make test-rules` passed all 64 JavaScript cases and all 90 Obsidian cases; no exceptions.
  - [x] `obsidian:ui.notice` — [`glass-lint-obsidian/src/rules/ui/notice/`](glass-lint-obsidian/src/rules/ui/notice/)
    - Audit: global/module provenance; intent-doc=[x]; coverage=global-object and aliased constructors, ESM named/namespace/CommonJS imports, subclass, shadowing, reassignment, foreign realms, dynamic module, lookalike; limitation=global `Notice` subclasses are not matched, and constructor arguments/class bodies are not analyzed; verified=targeted fixtures and Obsidian 1.12.7 runtime probe
  - [x] `obsidian:ui.ribbon` — [`glass-lint-obsidian/src/rules/ui/ribbon/`](glass-lint-obsidian/src/rules/ui/ribbon/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct call, static computed property, same-shaped receiver, reassignment, other receiver, alias, dynamic property, lookalike; limitation=does not prove an Obsidian receiver or follow aliases/reassignment, and arguments are not analyzed; verified=targeted fixtures
  - [x] `obsidian:ui.settings-tab` — [`glass-lint-obsidian/src/rules/ui/settings_tab/`](glass-lint-obsidian/src/rules/ui/settings_tab/)
    - Audit: module provenance/heuristic; intent-doc=[x]; coverage=registration direct/static-computed/same-shaped/reassigned calls, ESM named/namespace/CommonJS constructors, subclass, shadowing, reassignment, dynamic property, lookalike; limitation=registration is syntactic and constructor arguments/class bodies are not analyzed; verified=targeted fixtures
  - [x] `obsidian:ui.status-bar` — [`glass-lint-obsidian/src/rules/ui/status_bar/`](glass-lint-obsidian/src/rules/ui/status_bar/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct call, static computed property, same-shaped receiver, reassignment, other receiver, alias, dynamic property, lookalike; limitation=does not prove an Obsidian receiver or follow aliases/reassignment, and arguments are not analyzed; verified=targeted fixtures
  - [x] `obsidian:ui.suggest` — [`glass-lint-obsidian/src/rules/ui/suggest/`](glass-lint-obsidian/src/rules/ui/suggest/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct call, static computed property, same-shaped receiver, reassignment, other receiver, alias, dynamic property, lookalike; limitation=does not prove an Obsidian receiver or follow aliases/reassignment, and arguments are not analyzed; verified=targeted fixtures
  - [x] `obsidian:vault.access` — [`glass-lint-obsidian/src/rules/vault/access/`](glass-lint-obsidian/src/rules/vault/access/)
    - Audit: rooted; intent-doc=[x]; coverage=direct read, `this.app` and receiver aliases, static computed property, shadowing, declared-alias reassignment, dynamic property, local/unlisted lookalikes; limitation=does not follow a bare vault alias or analyze values, methods, arguments, or undeclared-global reassignment; verified=targeted fixtures
  - [x] `obsidian:vault.adapter` — [`glass-lint-obsidian/src/rules/vault/adapter/`](glass-lint-obsidian/src/rules/vault/adapter/)
    - Audit: rooted; intent-doc=[x]; coverage=direct read/call, `this.app` and receiver aliases, static computed properties, shadowing, declared-alias reassignment boundary, dynamic property, local/lookalike receiver; limitation=does not follow a bare adapter alias or analyze later method names, values, or arguments; verified=targeted fixtures

- [x] **Audit group 9 — Obsidian vault operations (7 rules)**
  - Group audit: owner=Codex; claimed=2026-07-11; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[x]; exception-log=reference
  - Exception log: `make test-rules` passed all 64 JavaScript cases and 86 of 88 Obsidian cases. Its two unrelated pre-existing failures are `workspace/leaf_management/positive` (expected 1, found 0; two findings) and `workspace/open/positive` (expected 1, found 0; two findings). All seven audited group-9 directories passed.
  - [x] `obsidian:vault.config-directory` — [`glass-lint-obsidian/src/rules/vault/config_directory/`](glass-lint-obsidian/src/rules/vault/config_directory/)
    - Audit: heuristic; intent-doc=[x]; coverage=forward/backslash marker substrings, static templates, case boundary, split/dynamic values; limitation=raw literal matching has no vault/path provenance and does not reconstruct dynamic or concatenated values; verified=targeted fixtures
  - [x] `obsidian:vault.delete` — [`glass-lint-obsidian/src/rules/vault/delete/`](glass-lint-obsidian/src/rules/vault/delete/)
    - Audit: rooted; intent-doc=[x]; coverage=all three configured calls, this.app, aliases, static computed properties, shadowing, reassignment, dynamic/unlisted lookalikes; limitation=arguments, returned objects, and unlisted methods are not analyzed; verified=targeted fixtures
  - [x] `obsidian:vault.enumerate` — [`glass-lint-obsidian/src/rules/vault/enumerate/`](glass-lint-obsidian/src/rules/vault/enumerate/)
    - Audit: rooted; intent-doc=[x]; coverage=all six configured calls, this.app, aliases, static computed properties, shadowing, reassignment, dynamic/unlisted lookalikes; limitation=arguments and other vault APIs are not analyzed; verified=targeted fixtures
  - [x] `obsidian:vault.events` — [`glass-lint-obsidian/src/rules/vault/events/`](glass-lint-obsidian/src/rules/vault/events/)
    - Audit: rooted; intent-doc=[x]; coverage=on calls, this.app, aliases, static computed property, shadowing, reassignment, dynamic and other event methods; limitation=event names, handlers, arguments, and methods other than on are not analyzed; verified=targeted fixtures
  - [x] `obsidian:vault.move-copy` — [`glass-lint-obsidian/src/rules/vault/move_copy/`](glass-lint-obsidian/src/rules/vault/move_copy/)
    - Audit: rooted; intent-doc=[x]; coverage=all three configured calls, this.app, aliases, static computed properties, shadowing, reassignment, dynamic/unlisted lookalikes; limitation=arguments, returned objects, and unlisted methods are not analyzed; verified=targeted fixtures
  - [x] `obsidian:vault.read` — [`glass-lint-obsidian/src/rules/vault/read/`](glass-lint-obsidian/src/rules/vault/read/)
    - Audit: rooted; intent-doc=[x]; coverage=all three configured calls, aliases, this.app, static computed properties, bounded rooted argument flow, shadowing, reassignment, dynamic/unlisted lookalikes; limitation=arguments and other read-like methods are not analyzed; verified=targeted fixtures
  - [x] `obsidian:vault.resource-url` — [`glass-lint-obsidian/src/rules/vault/resource_url/`](glass-lint-obsidian/src/rules/vault/resource_url/)
    - Audit: rooted/heuristic; intent-doc=[x]; coverage=vault/adapter calls, this.app, aliases, static computed properties, literal/template URL markers, shadowing, reassignment, dynamic/unlisted lookalikes; limitation=rooted calls do not analyze arguments and URL detection is raw literal matching without dynamic reconstruction or scheme semantics; verified=targeted fixtures

- [x] **Audit group 10 — Obsidian vault, views, and workspace (7 rules)**
  - Group audit: owner=Codex; claimed=2026-07-11; targeted-fixtures=[x]; workspace-tests=[x]; clippy=[x]; full-suite=[x]; exception-log=none
  - [x] `obsidian:vault.write` — [`glass-lint-obsidian/src/rules/vault/write/`](glass-lint-obsidian/src/rules/vault/write/)
    - Audit: rooted; intent-doc=[x]; coverage=all eight configured calls, `this.app`, aliases, static computed properties, shadowing, dynamic/unlisted methods, reassignment; limitation=call arguments and later API use are not analyzed; verified=targeted fixtures
  - [x] `obsidian:view.register` — [`glass-lint-obsidian/src/rules/view/register/`](glass-lint-obsidian/src/rules/view/register/)
    - Audit: heuristic; intent-doc=[x]; coverage=direct `this` call, static computed name, same-shaped receiver gap, reassignment, other receiver, alias, dynamic property, near-name exclusion; limitation=does not prove an Obsidian receiver or follow aliases/reassignment; verified=targeted fixtures
  - [x] `obsidian:workspace.active-editor` — [`glass-lint-obsidian/src/rules/workspace/active_editor/`](glass-lint-obsidian/src/rules/workspace/active_editor/)
    - Audit: rooted; intent-doc=[x]; coverage=direct read, `this.app`, aliases, static computed property, shadowing, dynamic/unlisted properties, reassignment; limitation=the read value is not analyzed; verified=targeted fixtures
  - [x] `obsidian:workspace.active-file` — [`glass-lint-obsidian/src/rules/workspace/active_file/`](glass-lint-obsidian/src/rules/workspace/active_file/)
    - Audit: rooted; intent-doc=[x]; coverage=direct call, `this.app`, aliases, static computed property, shadowing, dynamic/unlisted methods, reassignment; limitation=call arguments and returned file values are not analyzed; verified=targeted fixtures
  - [x] `obsidian:workspace.layout` — [`glass-lint-obsidian/src/rules/workspace/layout/`](glass-lint-obsidian/src/rules/workspace/layout/)
    - Audit: rooted; intent-doc=[x]; coverage=all three configured calls, `this.app`, aliases, static computed property, shadowing, dynamic/unlisted methods, reassignment; limitation=layout arguments and values are not analyzed; verified=targeted fixtures
  - [x] `obsidian:workspace.leaf-management` — [`glass-lint-obsidian/src/rules/workspace/leaf_management/`](glass-lint-obsidian/src/rules/workspace/leaf_management/)
    - Audit: rooted; intent-doc=[x]; coverage=all three configured calls, `this.app`, aliases, static computed property, shadowing, dynamic/unlisted methods, reassignment; limitation=call arguments, returned leaves, and intermediate API flow are not analyzed; verified=targeted fixtures
  - [x] `obsidian:workspace.open` — [`glass-lint-obsidian/src/rules/workspace/open/`](glass-lint-obsidian/src/rules/workspace/open/)
    - Audit: rooted; intent-doc=[x]; coverage=both configured chains, `this.app`, aliases, static computed property, shadowing, dynamic/unlisted methods, reassignment, intermediate-call gap; limitation=`getLeaf().openFile()` is not followed because rooted provenance does not cross intermediate calls, and arguments/returned objects are not analyzed; verified=targeted fixtures
