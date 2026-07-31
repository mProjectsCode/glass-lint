# Rule coverage and query-quality audit

Audit date: 2026-07-31

Scope: all 82 rules listed in [`RULES.md`](RULES.md), including the `js`,
`browser`, `node`, `electron`, and `obsidian` catalogs. The audit compares the
declared queries and colocated rule documentation with the current provider
source where practical, and with the current public API definitions for
Obsidian, web APIs, Electron, and Node.js.

Priority:

- **P0**: correctness problem or likely false positive/false negative that
  should be fixed before relying on the rule for policy decisions.
- **P1**: material coverage gap or query limitation.
- **P2**: completeness, precision, documentation, or maintainability work.

## Cross-cutting findings

1. **P0 — API drift checks are missing.** The Obsidian rules contain at least
   two event names that are not in the current public definitions: `closed` in
   `obsidian:vault.events` and `finished` in `obsidian:metadata.events`.
   `obsidian:markdown.link` also queries `parseSubpath`, which is not a current
   public export, and treats `fileToLinktext` and `generateMarkdownLink` as
   top-level `obsidian` exports even though the current definitions place them
   on `MetadataCache` and `FileManager`. Add a catalog check that resolves each
   module query against a versioned API manifest, and make deprecated/internal
   entries explicit rather than silently treating them as public.
2. **P0 — Public versus internal Obsidian APIs are not distinguished.** The
   `obsidian:plugins.*` rules query `app.plugins.*`, but those names do not
   occur in the current public `obsidian.d.ts`. Either remove them from the
   public Obsidian catalog, or label them as an internal-API profile backed by a
   separate, version-pinned source. Findings from the latter should have lower
   confidence and a clear message.
3. **P1 — Rule intent and implementation comments have drifted.** For example,
   `browser:file-dialog` currently has a `setAttribute("type", "file")`
   condition while its Rust documentation says `setAttribute` is excluded;
   `browser:dom.remote-resource` documents only script/image although it
   queries eight element types; and `browser:global-input-hook` documents
   `window`, `self`, and `globalThis` roots but declares only `document`, a
   bare listener, and `document.body`. Regenerate or update comments as part
   of rule changes.
4. **P1 — Import-only rules over-report unused dependencies and under-report
   direct API use.** This affects service/telemetry indicators and most Node
   rules. Keep an import indicator if desired, but add operation queries or
   split “dependency indicator” from “API operation” rules so policy consumers
   can choose the intended precision.
5. **P1 — Add a standard adversarial fixture matrix.** Every rooted/module
   query should have positives for ESM, CommonJS, namespace/destructured
   aliases, static computed properties, and supported returned-object flow;
   negatives for shadowing, reassignment, local lookalikes, dynamic names and
   values, and incompatible branch joins. Existing comments promise many of
   these properties, but the audit should verify them per rule rather than
   infer them from the matcher constructor.
6. **P2 — Track API versions.** Several current definitions add methods after
   the original API. Store `since`/deprecated metadata alongside query paths so
   profiles can target an Obsidian or Electron minimum version instead of
   silently mixing old and new APIs.

## JavaScript catalog

### `js:dynamic-code.eval`

- **Coverage:** Covers global `eval`, global `Function` calls, and `new
  Function`, including the engine’s supported aliases/bind/call/apply forms.
- **P1 query quality:** Add explicit tests for indirect eval, `globalThis.eval`,
  `window.eval`, `Reflect.construct(Function, ...)`, and destructuring from
  the global object. If the core identity model intentionally handles these,
  keep the tests as regression coverage.
- **P2:** Consider a separate WebAssembly/dynamic-module rule; do not broaden
  this rule with unrelated code-generation APIs without changing its policy
  name.

### `js:network.url-construction`

- **Coverage:** Covers `URL`, `URLSearchParams`, selected static `URL` methods,
  and literal `http://`/`https://` markers.
- **P1:** Add `URLPattern` construction and static `URL.canParse`/`URL.parse`
  coverage tests against the supported runtime version. Decide whether
  `URLSearchParams` should be separated from URL construction because it does
  not itself perform network access.
- **P1 query quality:** Literal substring queries flag prose, tests, and
  unrelated strings. Prefer a boundary-aware static URL matcher and mark
  template quasis as heuristic evidence rather than a URL-use witness.

### `js:network.private-address`

- **Coverage:** Covers common loopback, wildcard, RFC1918, link-local, and
  selected IPv6 prefixes, but only many HTTP(S)-prefixed forms.
- **P0 query quality:** Substring matching makes `localhost` and address
  fragments prone to false positives, and it misses valid static forms such as
  `http://127.0.0.2`, `172.16.0.1` without a scheme, bracketed IPv6, ports,
  and IPv4-mapped IPv6. Replace the literal list with static IP/URL parsing,
  range checks, and token boundaries.
- **P1:** Cover the full `127.0.0.0/8`, `169.254.0.0/16`, `172.16.0.0/12`,
  `fc00::/7`, `fe80::/10`, and relevant special-use ranges. Keep a separate
  policy decision for `0.0.0.0` because it is a bind wildcard rather than a
  destination address.

### `js:network.service-indicator`

- **Coverage:** Covers a useful hand-maintained list of SDK package names and
  endpoint substrings.
- **P1:** The list is necessarily incomplete and currently mixes SDK imports
  with endpoint literals. Build the package set from a maintained manifest and
  add common subpath imports, scoped package families, and static URL parsing.
- **P0 query quality:** Any literal containing a provider domain is reported,
  even in documentation or a test fixture, and an import is reported even when
  unused. Preserve this as a low-confidence indicator or add sink/use
  correlation for requests, client construction, and exported SDK calls.

### `js:network.telemetry-indicator`

- **Coverage:** Covers several major telemetry SDKs and endpoint markers.
- **P1:** Add current package families and common provider aliases only from a
  versioned manifest; include direct `import`/`require` subpaths where they are
  real package entry points.
- **P0 query quality:** It has the same unused-import and arbitrary-string
  problems as `network.service-indicator`. Add operation/use correlation (SDK
  initialization, event capture, transport/exporter calls) or make the rule’s
  heuristic nature explicit in the finding and profile.

### `js:network.header-indicator`

- **Coverage:** Includes selected `fetch` init keys and a broad literal marker
  list.
- **P0 query quality:** HTTP field names are case-insensitive, but the query
  has an incomplete hand-written casing list and reports names in unrelated
  strings. It also treats response-only `Set-Cookie` as a request indicator.
  Normalize static object keys and match actual header APIs (`fetch`, `Headers`,
  XHR `setRequestHeader`, and configured client libraries).
- **P1:** Add `Headers` constructor/set/append, XHR header calls, and static
  computed keys. Preserve a separate source-wide literal heuristic for cases
  where request context cannot be established.

### `js:dynamic-code.string-timer`

- **Coverage:** Correctly limits `setTimeout` and `setInterval` to static string
  first arguments and excludes callback scheduling.
- **P1:** Add rooted `window`/`globalThis` spellings and aliases to fixtures,
  then verify that the global matcher already covers them. Consider Node’s
  timer-module spellings only if they can execute strings in the target runtime;
  otherwise do not broaden it.
- **P2:** Add a companion rule for string-valued DOM handler properties if that
  risk is in scope.

## Browser catalog

### `browser:browser.clipboard-read`

- **Coverage:** Covers the modern Clipboard read/readText calls and legacy
  `execCommand("paste")`.
- **P1:** Add `window`/`globalThis`/worker-qualified roots and tests for
  `ClipboardItem`-based reads if the target policy treats those as clipboard
  access. Keep `execCommand` legacy and lower confidence.
- **P2 query quality:** Match only calls and static command values, as now;
  add explicit negatives for `copy` in the read rule and local `execCommand`.

### `browser:browser.clipboard-write`

- **Coverage:** Covers `write`, `writeText`, and legacy copy/cut commands.
- **P1:** Add static `ClipboardItem` construction/use if it is part of the
  policy, plus worker/global-root fixtures. Ensure `execCommand("paste")` is a
  negative here.

### `browser:browser.persistent-storage`

- **Coverage:** Covers Web Storage methods, IndexedDB entry points, CacheStorage
  methods, StorageManager persistence/OPFS entry points, and Cookie Store calls.
- **P0:** `document.cookie` is read-only in the query, so cookie writes—the
  common persistent-storage operation—are missed. Add a rooted property-write
  occurrence or a dedicated cookie-write query.
- **P1:** Direct `localStorage.foo`/`localStorage.foo = value`, `Storage.length`,
  and static property access are missed. Add supported property reads/writes or
  document the intentional call-only boundary.
- **P1:** Follow `getFileHandle` into file-handle operations (`getFile`,
  `createWritable`, permissions, and writable-stream `write`/`close`) and add
  OPFS methods as returned-object flow rather than stopping at directory
  handles.

### `browser:browser.permissions-geolocation`

- **Coverage/query quality:** The two permission-sensitive geolocation entry
  points are complete and rooted matching is appropriate.
- **P2:** Add global-root and alias fixtures, and decide whether `clearWatch`
  belongs in a separate geolocation lifecycle rule rather than this permission
  rule.

### `browser:browser.permissions-hardware`

- **Coverage:** Covers WebHID, Web Serial, and WebUSB prompt methods.
- **P1:** Add the corresponding `getDevices`/`getPorts` calls only if the policy
  wants hardware enumeration; otherwise keep them out and document that this
  rule is prompt-only. Add explicit negatives for enumeration if so.

### `browser:browser.permissions-media`

- **Coverage:** Covers `getUserMedia`, `getDisplayMedia`, and device
  enumeration.
- **P1:** Review newer permission-relevant APIs such as `selectAudioOutput`
  and `setSinkId` against the minimum browser baseline. Separate permission
  prompts from inventory reads if severity differs.

### `browser:browser.permissions-bluetooth`

- **Coverage/query quality:** `navigator.bluetooth.requestDevice` is the main
  Web Bluetooth permission prompt and the rooted query is precise.
- **P2:** Add `getAvailability` only in a lower-severity availability rule;
  do not conflate it with permission use.

### `browser:browser.permissions-notifications`

- **Coverage:** Covers `Notification.requestPermission`, the constructor, and
  one service-worker spelling of `showNotification`.
- **P1:** Add `registration.showNotification` and other proven
  `ServiceWorkerRegistration` aliases, not only the `self.registration` root.
  Consider `Notification.permission` as a separate permission-state read.
- **P2:** Add tests for `window.Notification`, worker globals, and shadowed
  `Notification`.

### `browser:browser.permissions-query`

- **Coverage/query quality:** Covers the Permissions API `query` call with
  provenance-safe rooted matching.
- **P2:** Add static permission-name value evidence where it improves policy
  usefulness, while retaining a generic call finding for dynamic names.

### `browser:browser.environment`

- **Coverage:** Covers a useful small set of navigator, screen, and connection
  properties.
- **P1:** Review current `Navigator` and `Screen` definitions for omitted
  environment/privacy signals, especially `navigator.userAgentData`,
  `navigator.globalPrivacyControl`, `screen.orientation`, and supported
  display geometry properties. Keep an explicit allowlist and document the
  privacy rationale for every field.
- **P0 query quality:** Member reads are precise, but a read of a broad root
  alias can be missed when the value is obtained through an unsupported getter
  or destructuring path. Add those forms to tests before changing matchers.

### `browser:browser.global-input-hook`

- **Coverage:** Covers selected `addEventListener` registrations and a few
  `document.on*` reads.
- **P0:** Assignments such as `document.onkeydown = handler` are real global
  input hooks but are intentionally not reported. Add rooted property-write
  support and report writes, not reads, for `on*` handler properties.
- **P0:** The documentation claims `window`, `self`, and `globalThis` roots,
  but the declarations do not explicitly query those member paths. Add them
  and add `document.body`/window/self/globalThis tests.
- **P1:** Expand the event allowlist with `beforeinput`, composition events,
  `contextmenu`, wheel, pointer-cancel, touch-move/cancel, and drag-over/drop
  lifecycle events if the policy is intended to cover comprehensive input
  interception.

### `browser:browser.file-dialog`

- **Coverage:** Covers static `createElement("input")`, `type` assignment or
  `setAttribute`, plus the File System Access open/save pickers.
- **P1:** Add HTML/parser construction and relevant `webkitdirectory`/multiple
  file-input configuration if file selection policy needs more than the
  bounded object-flow shape.
- **P2:** Correct the Rust doc comment, which currently contradicts the actual
  `setAttribute` query.

### `browser:browser.filesystem`

- **Coverage:** Covers `showDirectoryPicker` and direct methods on its returned
  directory handle.
- **P1:** Follow returned file handles and writable streams (`getFile`,
  `createWritable`, `write`, `seek`, `truncate`, `close`) and add
  `showOpenFilePicker`/`showSaveFilePicker` correlation if the file-dialog rule
  is not intended to own those operations.
- **P2:** Add `resolve`, permission, iterator, and static-computed-property
  adversarial fixtures; the current bounded-flow limitation is reasonable if
  it is kept explicit.

### `browser:network.request`

- **Coverage:** Covers fetch, beacon, XHR, WebSocket, and EventSource entry
  points.
- **P1:** Review newer browser transports (`WebTransport`, `WebSocketStream`,
  and any supported `fetchLater` baseline) and add them or document why they
  are excluded. Add XHR method/lifecycle queries only if construction alone is
  too noisy for the policy.
- **P2:** Add tests for `window`/`globalThis` roots and direct aliases, plus
  negative local lookalikes and reassignment.

### `browser:dom.remote-resource`

- **Coverage:** Covers static remote URL assignment/`setAttribute` for script,
  image, link, iframe, audio, video, source, object, and embed elements, then
  bounded insertion into selected document containers.
- **P1:** Add insertion sinks such as `document.documentElement`,
  `insertAdjacentElement`, `replaceChildren`, and supported fragment/HTML
  paths. Decide separately whether `data:`, `blob:`, and protocol-relative URLs
  are remote-resource findings.
- **P1 query quality:** Static `http`/`https`/`//` prefixes miss URL values
  derived through a bounded `URL` object or concatenation and can report a
  disconnected element if a sink matcher is too broad. Add source-to-sink
  tests for reassignment, aliases, and disconnected branches.
- **P2:** Update the doc comment from “script or image” to the actual element
  set.

### `browser:dynamic-code.script-injection`

- **Coverage:** Covers script elements with static `src`/text content and
  `document.write`/`writeln` markers.
- **P1:** Add `setAttribute("src", ...)`, `document.documentElement` and other
  insertion sinks, and HTML sinks such as `insertAdjacentHTML`, `innerHTML`,
  `outerHTML`, and `Range.createContextualFragment` where the core can keep
  static-value precision.
- **P1:** Recognize static `javascript:` URL assignments and script text that
  is configured through aliases; add negatives for non-executable script types
  and disconnected elements.

## Node catalog

### `node:node.network`

- **Coverage:** Covers the principal built-in HTTP/TLS/DNS/socket modules and a
  hand-maintained third-party client list.
- **P1:** Add Node’s global `fetch`/`WebSocket` and current transport modules
  where supported, or state that this rule is module-import-only. Keep the
  built-in list synchronized with the Node API index.
- **P0 query quality:** Importing `axios`, `undici`, or `http` is not proof of
  a request. Add operation queries for common built-ins and a separate import
  indicator to avoid treating unused dependencies as network behavior.

### `node:node.filesystem`

- **Coverage:** Covers `fs` module imports, selected filesystem packages, and
  `path` calls.
- **P0:** `path` manipulation is not filesystem I/O. Split it into a path/
  environment rule or lower its severity/category; otherwise consumers cannot
  distinguish path normalization from disk access.
- **P1:** Add operation coverage for `fs`/`fs.promises`, `FileHandle`, streams,
  and common package calls; the current import-only package queries miss direct
  use through re-exports and over-report unused packages.

### `node:node.process-environment`

- **Coverage:** Covers many current `process` environment/platform reads and
  process metadata calls.
- **P1:** Add `process.env.X` writes/deletes if environment mutation matters,
  and review current Node additions such as `process.availableMemory` and
  `process.finalization` against the supported baseline.
- **P2:** Keep destructive/process-control methods (`exit`, `kill`, signal
  events) in a separate rule so this rule remains about environment and
  metadata.

### `node:node.subprocess`

- **Coverage:** Covers built-in and selected subprocess/worker package imports.
- **P0 query quality:** An import is not a subprocess start. Add calls to
  `spawn`, `exec`, `execFile`, `fork` and sync variants, `new Worker`, and
  `cluster.fork`; correlate package imports with operation use where possible.
- **P1:** Review current packages (`tinysh`, task runners, shell wrappers) from
  a maintained manifest rather than a static ad hoc list.

### `node:archive.compression`

- **Coverage:** Covers Node zlib and a useful set of archive/compression
  packages.
- **P1:** Add direct `zlib` operation/use queries and current archive packages
  from a manifest; distinguish archive extraction from ordinary compression if
  policy severity differs.
- **P2 query quality:** Keep import findings as indicators, but add negatives
  for local packages with similar names and unused imports.

### `node:crypto.operation`

- **Coverage:** Covers Node crypto module imports, popular crypto libraries,
  and direct `crypto.subtle` methods.
- **P1:** Add `globalThis.crypto.subtle`/`webcrypto.subtle` and relevant Node
  `webcrypto` import forms. Add operation-level coverage for common Node
  `createHash`, `createCipheriv`, signing, key-generation, and password-hash
  calls if the rule is meant to indicate actual cryptography rather than a
  dependency.
- **P0 query quality:** Import-only library findings cannot distinguish a
  cryptographic operation from an unused dependency; expose that distinction
  in rule IDs or certainty/messages.

## Electron catalog

### `electron:electron.module`

- **Coverage:** Covers Electron module imports, `BrowserWindow` construction,
  and a curated set of high-impact calls/reads.
- **P1:** Compare the curated list against the current Electron module index;
  notable areas to review include `app` lifecycle/events, `BrowserWindow`
  instance methods, `session` operations, `webContents` instance APIs,
  `screen`, `nativeImage`, `protocol`, and `desktopCapturer` additions.
- **P2 query quality:** Module provenance is strong, but the rule name is
  broad while only a subset is reported. Either publish the list as an explicit
  profile or split broad module-use from sensitive operation rules.

### `electron:electron.ipc`

- **Coverage:** Covers the main `ipcRenderer`, `ipcMain`, `webContents`, and
  `webFrameMain` send/listener/cleanup methods.
- **P1:** Verify the method matrix against the current Electron definitions,
  especially newer `webContents`/`webFrameMain` message APIs and listener
  overloads. Add `ipcRenderer`/`ipcMain` event registration fixtures with
  static channel values where channel evidence is useful.
- **P2:** Add inline `require("electron").ipcRenderer` only if core can preserve
  provenance; otherwise retain the documented limitation and test it.

### `electron:electron.shell`

- **Coverage:** Covers the current high-impact shell methods including external
  opening, path opening, reveal, trash, beep, and shortcut-link operations.
- **P1 query quality:** Add tests for inline CommonJS chains and destructured
  exports, or document that only namespace-proven calls are supported. Consider
  separating user-visible external navigation from local shell operations.

### `electron:electron.dialog`

- **Coverage:** Covers the principal async/sync open/save/message/error/trust
  dialog calls.
- **P1:** Review `showAboutPanel` and any current platform-specific dialog APIs
  against Electron’s official dialog module. Add static option-object evidence
  (filters, default path, security-sensitive options) if policy needs more than
  “dialog used”.
- **P2:** Add inline `require` and ESM destructuring fixtures consistently with
  `electron.shell` and `electron.ipc`.

## Obsidian catalog

The authoritative baseline for this section is the current
[Obsidian API type definition](https://raw.githubusercontent.com/obsidianmd/obsidian-api/master/obsidian.d.ts).
The definitions distinguish public APIs, inherited `Component`/`Events`
methods, deprecated APIs, and `since` versions. Internal `app.plugins` APIs do
not appear in that public source.

### Network, vault, and metadata

- **`obsidian:network.request` — P1:** `request` and `requestUrl` are both
  current public exports and the module queries are appropriate. Add tests for
  string and object arguments, aliases, and the fact that these APIs bypass
  browser CORS; keep endpoint/value analysis in the network-indicator rules.
- **`obsidian:vault.access` — P2:** The rooted `app.vault` read is precise for
  “obtains the Vault”, but it reports a broad root even when no operation is
  performed. Consider a separate root-access rule from operation rules.
- **`obsidian:vault.read` — P1:** The three current `Vault` read methods are
  covered. If adapter access is in scope, add returned `DataAdapter` read calls
  to `vault.adapter` rather than silently leaving them uncovered.
- **`obsidian:vault.write` — P1:** The current eight `Vault` write methods are
  covered, including `appendBinary`. Add a returned-object/value-aware variant
  if write policy needs to distinguish content mutation from merely calling the
  API.
- **`obsidian:vault.delete` — P1:** `delete`, `trash`, and `FileManager.trashFile`
  are covered. Review `FileManager.promptForDeletion` as a separate prompt/UI
  event and add adapter `remove`/trash calls to the adapter rule.
- **`obsidian:vault.move-copy` — P1:** `Vault.rename`, `Vault.copy`, and
  `FileManager.renameFile` are covered. Add `DataAdapter.rename`/`copy` and
  returned adapter flow if low-level vault filesystem operations are intended.
- **`obsidian:vault.enumerate` — P0:** The listed lookup/enumeration methods
  are current, but `Vault.recurseChildren` is declared static in the public
  definitions; `app.vault.recurseChildren` is therefore the wrong path. Add
  the static `Vault.recurseChildren` form and test that the instance spelling
  is not falsely accepted.
- **`obsidian:vault.adapter` — P0:** The rule currently reports reading the
  adapter object, not using adapter operations. Add `DataAdapter` methods such
  as `exists`, `stat`, `list`, `read`, `write`, `append`, `process`, `mkdir`,
  `remove`, `rename`, `copy`, and `getFullPath`, with returned-root flow and a
  clear distinction between desktop/mobile adapter implementations.
- **`obsidian:vault.config-directory` — P1:** `configDir` is the authoritative
  API and is covered, but `.obsidian/` literals are only a heuristic; the
  public API explicitly says the directory can differ. Add `.obsidian` boundary
  forms and path construction where statically provable, and lower confidence
  for raw literals.
- **`obsidian:vault.resource-url` — P1:** Vault and adapter `getResourcePath`
  are covered. Add `getFilePath` if local file URI access is part of the same
  policy, and keep the raw `obsidian://` literal query separate from proven API
  use.
- **`obsidian:vault.events` — P0:** Remove `closed`; the current public Vault
  event overloads are `create`, `modify`, `delete`, and `rename`. Add tests for
  every valid event and dynamic/unknown event rejection.
- **`obsidian:metadata.cache-read` — P1:** Root/cache lookup coverage matches
  the current `MetadataCache`. Add `fileToLinktext` as a cache method only if
  this rule owns linktext generation; otherwise keep it in a corrected
  markdown-link rule.
- **`obsidian:metadata.frontmatter-read` — P1:** `getFileCache.frontmatter`
  and the two parser helpers are covered. Add the same `frontmatter` flow from
  `getCache`, plus current `parseFrontMatterEntry` and
  `parseFrontMatterStringArray` exports.
- **`obsidian:metadata.events` — P0:** Remove `finished`; current public
  `MetadataCache` events are `changed`, `deleted`, `resolve`, and `resolved`.
  Add a source-derived event manifest so this cannot drift again.
- **`obsidian:metadata.traversal` — P1:** Rooted `Object`/`Reflect` traversal is
  a good precision boundary. Add `for...in`, static spread/assignment, and
  direct-map alias tests if the query vocabulary supports them; otherwise
  document that only explicit enumeration calls are covered.
- **`obsidian:metadata.extract` — P1:** The field list matches current
  `CachedMetadata`, including `frontmatterLinks`, `blocks`, and
  `frontmatterPosition`. Add returned flow from `getCache`, not only
  `getFileCache`, and keep optional-field reads from claiming the field exists.

### Workspace and view/UI

- **`obsidian:workspace.active-file` — P2:** `getActiveFile` is the recommended
  public API and is correctly matched. Keep deprecated `activeLeaf` access out
  unless a legacy-compatibility profile is explicitly requested.
- **`obsidian:workspace.active-editor` — P2:** `activeEditor` is correctly
  rooted. Add `editorInfoField`/`editorEditorField` only if field-level editor
  access is intended; otherwise retain this narrow workspace rule.
- **`obsidian:workspace.events` — P0:** Add documented `quick-preview`,
  `resize`, `css-change`, `files-menu`, and `url-menu`; the current list covers
  the other selected workspace/editor/menu events. Keep `quit` because it is
  public but not guaranteed to run.
- **`obsidian:workspace.layout` — P1:** `getLayout`, `changeLayout`, and the
  callable `requestSaveLayout` are covered. Add `onLayoutReady` and layout
  event correlation only if lifecycle policy needs readiness rather than
  layout mutation.
- **`obsidian:workspace.leaf-management` — P1:** Add current leaf APIs omitted
  from the list: `createLeafInParent`, `createLeafBySplit`, `splitActiveLeaf`,
  `duplicateLeaf`, `getUnpinnedLeaf`, `getGroupLeaves`, `getMostRecentLeaf`,
  and `getLastOpenFiles` where the category is intended to be comprehensive.
- **`obsidian:workspace.open` — P1:** Add `openFile` returned flow from the
  other public leaf-producing methods (`createLeafInParent`, split/duplicate,
  unpinned, and most-recent leaf paths) and verify `getLeaf` overloads. Keep
  `openLinkText` separate because it returns no leaf.
- **`obsidian:view.register` — P1:** `Plugin.registerView` is covered. Add
  `registerHoverLinkSource` only if the rule is broadened from view creation to
  view integration; otherwise keep it separate.
- **`obsidian:ui.command` — P2:** `Plugin.addCommand` is correctly matched.
  Consider `removeCommand` in a command-lifecycle rule, not as registration.
- **`obsidian:ui.ribbon` — P2:** `Plugin.addRibbonIcon` matches the public API.
  Add a mobile exclusion/evidence note because the API’s availability differs
  from status-bar behavior.
- **`obsidian:ui.status-bar` — P2:** `Plugin.addStatusBarItem` matches the
  public API and its desktop-only limitation should be represented in profile
  metadata.
- **`obsidian:ui.modal` — P1:** `Modal` construction/subclass matching is a
  good base, but current public definitions include modal subclasses such as
  `ConfirmationModal` and other suggest/modal types. Add them or state that
  only direct `Modal` inheritance is supported.
- **`obsidian:ui.notice` — P2:** Global/module `Notice` constructors are
  covered. Add tests for the current `DocumentFragment`/duration overloads
  only if argument-sensitive policy is needed.
- **`obsidian:ui.menu` — P1:** Matching only `Menu.addItem` misses `new Menu()`,
  `addSeparator`, positioning/show methods, and menu use through static
  `Menu.forEvent`. Add constructor and proven `Menu` instance operations, or
  rename the rule to “adds menu items”.
- **`obsidian:ui.settings-tab` — P1:** Registration and
  `PluginSettingTab` construction are covered. Add current declarative
  settings-tab usage (`getSettingDefinitions`, setting controls, nested pages)
  only if the policy targets settings behavior rather than registration.

### Editor, markdown, storage, lifecycle, and platform

- **`obsidian:editor.content` — P1:** The selected core read/write methods are
  covered, but current `Editor` also exposes `getDoc`, `refresh`, `lineCount`,
  `lastLine`, `somethingSelected`, `listSelections`, focus/blur, scrolling,
  undo/redo, `exec`, transactions, coordinate conversion, and `processLines`.
  Add the methods relevant to content or split navigation/interaction into a
  second rule.
- **`obsidian:editor.extension` — P2:** `Plugin.registerEditorExtension` is
  correctly matched. Keep it distinct from `registerExtensions`, which maps
  file extensions to view types.
- **`obsidian:editor.suggest` — P2:** `registerEditorSuggest` is correctly
  matched. Add subclass/alias fixtures and consider whether `EditorSuggest`
  construction belongs in the same policy.
- **`obsidian:file-manager.frontmatter-write` — P2:**
  `FileManager.processFrontMatter` is current and correctly rooted. Add
  callback/value evidence only if the policy needs to distinguish mutation from
  a no-op callback.
- **`obsidian:markdown.postprocessor` — P2:** `registerMarkdownPostProcessor`
  is current and correctly matched. Add `MarkdownPreviewRenderer` static
  registration only in a compatibility rule if legacy code is in scope.
- **`obsidian:markdown.code-block-processor` — P2:** The current plugin
  registration API is correctly matched. Keep language argument matching as a
  possible future refinement; it should remain static-value-safe.
- **`obsidian:markdown.render` — P1:** Add the still-declared deprecated
  `MarkdownRenderer.renderMarkdown` alongside `render`, or explicitly document
  that deprecated render calls are intentionally excluded.
- **`obsidian:markdown.link` — P0:** Correct the query model: `fileToLinktext`
  is on `MetadataCache`, `generateMarkdownLink` is on `FileManager`, and
  `parseSubpath` is absent from the current public definitions. Keep valid
  top-level helpers (`parseLinktext`, `normalizePath`, `getLinkpath`,
  `resolveSubpath`, and frontmatter parsers), and add current
  `parseFrontMatterEntry`/`parseFrontMatterStringArray` if desired.
- **`obsidian:codemirror.extension` — P1:** The package allowlist is useful but
  hand-maintained. Compare it with the CodeMirror packages imported by the
  current Obsidian definitions and distinguish editor extensions from generic
  CodeMirror utilities; add CommonJS/subpath and bundled-import fixtures.
- **`obsidian:storage.app-data` — P2:** `loadLocalStorage`,
  `saveLocalStorage`, and all three current `SecretStorage` methods are
  covered. Add static secret-ID value evidence only if secret handling policy
  needs it; there is no public `deleteSecret` method to add.
- **`obsidian:storage.plugin-data-read` — P2:** `Plugin.loadData` is correctly
  matched. Keep the instance provenance requirement and add subclass/alias
  fixtures.
- **`obsidian:storage.plugin-data-write` — P2:** `Plugin.saveData` is correctly
  matched. Add argument-shape evidence only if policy needs to distinguish
  secrets or large data from ordinary settings persistence.
- **`obsidian:lifecycle.events` — P1:** Add inherited public
  `Component.register`, which is the generic unload cleanup API. The existing
  `registerEvent`, `registerDomEvent`, `registerInterval`, and protocol-handler
  entries are current; add an explicit test that inherited methods are accepted
  on proven Plugin instances.
- **`obsidian:bases.register` — P2:** `Plugin.registerBasesView` matches the
  current public API. Add a minimum-version marker (`since 1.10.0`) and a
  negative for similarly named non-Plugin registration functions.
- **`obsidian:cli.register` — P2:** `Plugin.registerCliHandler` matches the
  current public API (`since 1.12.2`). Add version metadata and static command
  ID evidence if CLI policy needs it.
- **`obsidian:platform.branching` — P2:** The current Platform property list
  matches the public definitions, including `resourcePathPrefix`. Add static
  property-read fixtures and ensure the rule does not confuse boolean checks
  with assignments.

### Plugin access and lifecycle of other plugins

- **`obsidian:plugins.access` — P0:** `app.plugins.getPlugin`, `plugins`,
  `manifests`, and `enabledPlugins` are not public members in the current
  `obsidian.d.ts`. Move this to an explicitly internal Obsidian profile backed
  by a version-pinned runtime source, or remove it from the public API catalog.
- **`obsidian:plugins.enable-disable` — P0:** The queried enable/disable APIs
  are likewise absent from the public definitions. Treat them as internal and
  add a runtime-version compatibility contract before shipping findings.
- **`obsidian:plugins.load-unload` — P0:** The queried load/unload/getPlugin
  paths are internal and are not represented in the public API source. Split
  this from public API coverage, and test that returned-plugin flow cannot
  combine a plugin object from one branch with a load/unload call from another.

## Proposed implementation order

1. Fix P0 API drift and public/internal catalog separation: Vault/metadata
   events, markdown-link paths, static `Vault.recurseChildren`, and plugin
   management.
2. Add missing high-impact sinks and writes: cookie writes, global `on*`
   property writes, File System Access file-handle flow, script/HTML sinks,
   Node subprocess operations, and adapter operations.
3. Add source-derived API manifests and version metadata for Obsidian,
   Electron, Node built-ins, and browser baselines.
4. Split import/literal indicators from operation rules where precision is
   required, then expand the adversarial fixture matrix and regenerate
   `RULES.md`.

## Authoritative references

- [Obsidian API definitions](https://raw.githubusercontent.com/obsidianmd/obsidian-api/master/obsidian.d.ts)
  and the [Obsidian API repository](https://github.com/obsidianmd/obsidian-api).
- [MDN Navigator](https://developer.mozilla.org/en-US/docs/Web/API/Navigator),
  [Clipboard API](https://developer.mozilla.org/en-US/docs/Web/API/Clipboard),
  [File System API](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API),
  [Storage API](https://developer.mozilla.org/en-US/docs/Web/API/Storage), and
  [Permissions API](https://developer.mozilla.org/en-US/docs/Web/API/Permissions).
- [Electron API documentation](https://www.electronjs.org/docs/latest/api) for
  [dialog](https://www.electronjs.org/docs/latest/api/dialog),
  [ipcRenderer](https://www.electronjs.org/docs/latest/api/ipc-renderer),
  [ipcMain](https://www.electronjs.org/docs/latest/api/ipc-main), and
  [shell](https://www.electronjs.org/docs/latest/api/shell).
- [Node.js API documentation](https://nodejs.org/api/) for filesystem,
  networking, crypto, process, child-process, worker, and compression modules.
- [MDN URL API](https://developer.mozilla.org/en-US/docs/Web/API/URL) and the
  [WHATWG URL standard](https://url.spec.whatwg.org/) for static URL parsing
  and boundary-aware URL/IP handling.
