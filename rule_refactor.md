# Rule refactor plan

## Goal

Refactor the existing rule catalog into smaller, more precise rules and move rules that are not Obsidian-specific out of `glass-lint-obsidian`.

This plan is deliberately limited to rule definitions, catalog organization, disclosure mappings, and rule fixtures. It does not propose changes to the parser, semantic indexes, linter context, report schema, finding model, matcher API, or public core API.

## Constraints from the current implementation

- Rules remain declarative `glass_lint_core::rules::Rule` values built with the existing builder and `Matcher` types.
- Continue to prefer global, rooted, module-provenance, argument-constrained, and connected-flow matchers. Use heuristic matchers only for the opt-in heuristic profile.
- A `RuleCatalog` supplies one namespace for all of its rules. Do not write rule IDs containing a second provider namespace into an individual rule definition.
- Category is rule metadata, not a second rule namespace or a rule kind.
- Confidence controls recommended-versus-heuristic selection in the Obsidian provider. Severity remains `Info` or `Warning` at the rule-definition level.
- Do not add compatibility aliases. This project permits clean breaks; update callers, disclosures, tests, and documentation together.
- A split is useful only when its parts express meaningfully different capabilities or disclosure policy. Avoid turning every individual method into a rule.

## Package and namespace layout

Keep `glass-lint-core` generic and keep only Obsidian-specific rules and policy in `glass-lint-obsidian`.

Create one new provider crate, `glass-lint-js`, for all non-Obsidian JavaScript, browser, Node.js, and Electron rules. Use the `js:*` provider namespace and use categories and source folders to distinguish `language`, `browser`, `node`, and `electron`. `js` describes the shared ecosystem without confusing the provider with `glass-lint-core`, which is the lint engine rather than a rule provider.

For example:

```text
js:network.request             category: browser/network
js:browser.clipboard-read      category: browser/clipboard
js:node.filesystem             category: node/filesystem
js:electron.ipc                category: electron/ipc
obsidian:network.request            category: network
obsidian:vault.read                 category: vault
```

This is preferable to putting `js:*`, `node:*`, and `electron:*` in one catalog: the current catalog API would require three separate catalogs and linters to obtain those three namespaces. If separate provider namespaces become a product requirement later, that is a core composition project and is outside this rule refactor.

## Colocated rule and fixture layout

Organize each provider by category, then by rule. Put the Rust rule definition and its JavaScript fixtures in the same rule folder:

```text
glass-lint-obsidian/src/rules/
  vault/
    mod.rs
    read/
      mod.rs
      positive.js
      negative.js
    write/
      mod.rs
      positive.js
      negative.js

glass-lint-js/src/rules/
  browser/
    network_request/
      mod.rs
      positive.js
      negative.js
    clipboard_read/
      mod.rs
      positive.js
      negative.js
  node/
    filesystem/
      mod.rs
      positive.js
      negative.js
  electron/
    ipc/
      mod.rs
      positive.js
      negative.js
```

Category `mod.rs` files should only assemble their child rules. The provider-level `rules/mod.rs` should only assemble categories. Rule definitions use `Rule::builder` and `Matcher` directly; do not add provider-local builder wrappers or support modules.

Each rule folder has exactly two JavaScript fixture files. `positive.js` contains one or more examples that must produce the containing rule ID; `negative.js` contains examples that must not produce it. Label examples with comments and put each expectation directive immediately before the code whose source location it asserts, for example:

```js
// direct global
// @expect-error glass-lint rule=js:network.request message_id=detected
fetch("https://example.com");

// aliased global
const request = fetch;
// @expect-error glass-lint rule=js:network.request message_id=detected
request("https://example.com");
```

Use the shared `glass-lint-harness` to recursively discover fixtures in both provider rule directories. It evaluates the inline expectations against the finding's rule ID and source location, so adding a rule folder requires no central Rust test list. Run both provider trees through the `provider-fixtures` Make target. Keep catalog-wide invariants such as unique IDs and namespace validation as ordinary Rust tests.

Each rule moved or split must have focused fixtures for the matcher mechanisms it uses. Where applicable include ESM, CommonJS, namespace access, destructuring, aliases, reassignment, shadowing, local lookalikes, computed properties supported by core, and compact/minified syntax. Do not manufacture a fixture for behavior the current matcher cannot prove; keep that behavior out of the rule.

## Refactor decisions

### Move generic JavaScript rules to `glass-lint-js`

Move these rules rather than reimplementing them:

| Existing rule | New rule | Decision |
| --- | --- | --- |
| `obsidian:network.browser` | `js:network.request` | Keep `fetch`, XHR, WebSocket, EventSource, and `sendBeacon` together as network access. Splitting each transport adds noise without changing policy. |
| `obsidian:network.node` | `js:node.network` | Detect Node HTTP module provenance. |
| `obsidian:filesystem.node` | `js:node.filesystem` | Restrict this to filesystem/path APIs. Move compression, streams, buffers, and OS inspection to their own rules. |
| `obsidian:process.node` | `js:node.process-environment` and `js:node.subprocess` | Subprocess execution is materially different from reading process metadata. |
| `obsidian:electron.desktop` and `obsidian:electron.ipc_shell` | `js:electron.module`, `js:electron.ipc`, and `js:electron.shell` | Remove their current overlap. Add clipboard/dialog/webContents rules only when matchers and fixtures exist. |
| `obsidian:browser.clipboard` | `js:browser.clipboard-read` and `js:browser.clipboard-write` | Read and write have different disclosure implications. |
| `obsidian:browser.storage` | `js:browser.persistent-storage` | Keep storage mechanisms together initially. Split only if consumers need different disclosures. |
| `obsidian:browser.permissions` | Per-capability `js` rules | Geolocation, media capture, Bluetooth, and notifications should not share one finding. |
| `obsidian:browser.permission_availability` | Remove initially | A property-existence check is not use of the capability. Add narrowly scoped availability rules only if there is a consumer. |
| `obsidian:browser.environment` | `js:browser.environment` | Keep as a heuristic inventory rule until receiver provenance is strong enough for recommended. |
| `obsidian:browser.broad_input_hooks` | `js:browser.global-input-hook` | Keep event-name argument constraints; remove duplicate clipboard calls, which belong to clipboard rules. |
| `obsidian:archive.compression` | `js:archive.compression` | Keep imports and strong API evidence; broad `"zip"`/`"gzip"` literals should remain heuristic or be removed. |
| `obsidian:crypto.hashing` | `js:crypto.operation` | The existing rule also detects encryption/decryption, so `hashing` is inaccurate. Split hashing from encryption later only if evidence and policy benefit. |
| `obsidian:dynamic_code` | `js:dynamic-code.eval`, `js:dynamic-code.string-timer`, and `js:dynamic-code.script-injection` | These are distinct behaviors. Do not classify ordinary dynamic `import()` as eval. |
| `obsidian:network.remote_dom_loading` | `js:dom.remote-resource` | Preserve the existing connected-flow matchers. |

Also move generic URL construction, private-address literals, service/SDK indicators, telemetry indicators, and header indicators to `glass-lint-js`. Keep them separate from `network.request`: mentioning an endpoint or constructing a URL does not prove a request. These indicator rules should remain heuristic unless their evidence is connected to a request sink.

Do not introduce an aggregate `capability:network-access` rule. It would require cross-catalog derivation that the current rule API does not provide and would duplicate the underlying findings.

### Keep and refine Obsidian network rules

- Rename `obsidian:network.obsidian` to `obsidian:network.request`.
- Continue to match module-provenance calls to Obsidian `request` and `requestUrl`.
- Keep Obsidian disclosure mapping in `glass-lint-obsidian`; add equivalent provider policy in `glass-lint-js` if `js` findings need disclosures.

### Vault

Use capability-sized rules rather than one rule per method:

```text
obsidian:vault.access
obsidian:vault.read
obsidian:vault.write
obsidian:vault.delete
obsidian:vault.move-copy
obsidian:vault.enumerate
obsidian:vault.events
obsidian:vault.adapter
obsidian:vault.config-directory
obsidian:vault.resource-url
```

- Split the current `vault.destructive`: permanent delete/trash belong in `delete`; rename/copy do not.
- Keep create, modify, append, and binary variants in `write` unless disclosure consumers need to distinguish them.
- Keep read, cached read, and binary read in `read` for the same reason.
- Remove `vault.folder_ops` if all of its evidence is covered by enumerate, write, delete, or move/copy.
- Rename `vault.resources` to `vault.resource-url`.
- Remove `vault.open_create_flows`; assign its matchers to Vault, Workspace, or FileManager rules according to the proven receiver/API provenance.
- Keep adapter access as a capability. Do not add “adapter used where Vault API exists” until the engine can prove the surrounding intent.

### Metadata and FileManager

Use:

```text
obsidian:metadata.cache-read
obsidian:metadata.frontmatter-read
obsidian:metadata.events
obsidian:metadata.traversal
obsidian:metadata.extract
obsidian:file-manager.frontmatter-write
obsidian:file-manager.link
```

- Move `processFrontMatter` out of MetadataCache terminology and into FileManager.
- Keep tag/link/embed/heading extraction together initially. They have the same capability and likely the same disclosure. Split only in response to a real consumer requirement.
- Prefer Obsidian module and rooted receiver provenance. Do not add a generated API catalog or semantic selector layer as part of this refactor.

### Workspace, views, and UI

Split only the broad rules whose members have different capabilities:

```text
obsidian:workspace.active-file
obsidian:workspace.active-editor
obsidian:workspace.open
obsidian:workspace.leaf-management
obsidian:workspace.layout
obsidian:view.register
obsidian:ui.command
obsidian:ui.ribbon
obsidian:ui.status-bar
obsidian:ui.modal
obsidian:ui.notice
obsidian:ui.menu
obsidian:ui.settings-tab
obsidian:ui.suggest
```

- Remove `ui.dom_heavy`. Raw counts or broad DOM names are not a precise Obsidian capability. Move concrete generic DOM behaviors to the `js` provider when they can be expressed accurately.
- Split `ui.file_dialog` between browser file input and Electron dialog rules; neither belongs under Obsidian UI.
- Do not add view cleanup, layout readiness, duplicate registration, or unsafe HTML data-flow rules in this refactor. The current declarative rules cannot prove the lifecycle or cross-call relationships described by those proposals.

### Editor and Markdown

Use:

```text
obsidian:editor.extension
obsidian:editor.suggest
obsidian:markdown.postprocessor
obsidian:markdown.code-block-processor
obsidian:markdown.render
obsidian:markdown.link
obsidian:codemirror.extension
```

Split the existing broad rules along these API boundaries. Do not create public/private or CodeMirror-version findings from raw names alone. Such rules need authoritative symbol provenance and dedicated evidence beyond this refactor.

### Settings, lifecycle, platform, and plugin access

- Split `settings.persistence` into `storage.plugin-data-read` and `storage.plugin-data-write` only if the existing matchers can distinguish `loadData` from `saveData`; otherwise rename it to `storage.plugin-data`.
- Keep `settings.ui` as `ui.settings-tab`.
- Keep `lifecycle.methods` only if lifecycle presence is a useful reported capability. Otherwise remove it; method names such as `onload` are weak evidence in bundled code.
- Refactor `lifecycle.events` into positive capabilities for Obsidian event registration, DOM event registration, and interval registration only when their calls can be matched precisely.
- Do not add unmanaged-event, unmanaged-interval, timeout, child-component, or unload-cleanup warnings. They require absence/escape/lifecycle analysis that is not expressible as a rule-only matcher refactor.
- Keep `platform.branching` as one rule. Its flags all express the same capability; splitting every OS flag would create noisy findings.
- Split `plugins.internal_access` into other-plugin access and Obsidian internal-manager access only where receiver provenance supports the distinction. Keep Dataview detection as an Obsidian integration rule; do not generalize it to an unimplemented metadata-bearing integration finding.

## Explicitly out of scope

Remove the following proposals from this plan rather than carrying them as later phases:

- new analysis-fact, finding, evidence, rule-kind, or linter-context schemas;
- generated public/private API catalogs and version snapshots;
- manifest consistency and minimum-version analysis;
- cross-catalog aggregate findings;
- reachability, call graphs, taint analysis, or source-map enrichment;
- lifecycle absence and cleanup analysis;
- credential inference, path normalization, read/write race, bulk-scan, raw-frontmatter-rewrite, and unsafe-input-flow rules;
- backward-compatible rule aliases.

They may be worthwhile separate projects, but they are not rule refactors and several require core engine work.

## Implementation order

1. Add `glass-lint-js` to the workspace with one `js` catalog and the category/rule/fixture layout above.
2. Configure the shared recursive fixture harness for both provider rule trees and run it through `make provider-fixtures`.
3. Move generic JavaScript, browser, Node.js, and Electron rules without changing matcher behavior; update namespaces, disclosure mappings, callers, and docs in the same change.
4. Remove overlaps in Node/Electron/browser/dynamic-code rules and add positive and adversarial negative fixtures for every new rule.
5. Reorganize the Obsidian rules into category/rule folders, initially without semantic changes.
6. Apply the Vault, Metadata/FileManager, Workspace/UI, and Editor/Markdown splits listed above.
7. Review every medium/low-confidence rule against the accuracy policy. Tighten it, keep it heuristic, or remove it.
8. Run formatting, workspace tests, Clippy with warnings denied, and all provider fixture harnesses.

## Current progress (2026-07-10)

Completed:

- `glass-lint-js` is a workspace provider with one `js` catalog; CLI and shared-harness adapter selection support both providers.
- Generic JavaScript, browser, Node.js, Electron, dynamic-code, and indicator rules have moved out of the Obsidian provider.
- Both providers use category/rule folders with colocated `positive.js` and `negative.js` fixtures. The shared harness verifies both trees recursively.
- Rule definitions use the core builder and matcher API directly; provider-local `support.rs` builder layers were removed.
- The Vault, Metadata, Workspace, UI, Editor, Markdown, CodeMirror, storage, lifecycle, platform, and plugin rule reorganizations are substantially in place.
- Current formatting, workspace tests, Clippy with warnings denied, shared provider fixtures, and legacy harness suites pass.

Remaining work:

- Rename and relocate `obsidian:metadata.frontmatter-write` to the planned `obsidian:file-manager.frontmatter-write` rule folder and update its disclosure mapping and fixtures.
- Add the Electron-dialog half of the former file-dialog capability, if it can be expressed with provenance-aware matchers and focused fixtures.
- Finish the medium/low-confidence audit and expand adversarial fixtures for rules whose matcher mechanisms support ESM, CommonJS, aliases, reassignment, shadowing, computed properties, or compact syntax.

## Completion criteria

- `glass-lint-core` contains no provider or product policy changes.
- `glass-lint-obsidian` contains no generic JavaScript, browser, Node.js, or Electron rules.
- The new `js` provider owns those generic rules in one crate and one valid catalog namespace.
- Every rule lives under a category/rule folder with colocated `positive.js` and `negative.js` fixtures.
- No moved or split rule relies on a weaker matcher solely to preserve old coverage.
- All rule IDs, profiles, disclosure mappings, documentation, fixtures, and workspace callers agree after the clean break.
- Formatting, workspace tests, Clippy with warnings denied, and fixture harnesses pass.
