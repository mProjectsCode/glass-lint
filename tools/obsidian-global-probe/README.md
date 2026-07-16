# Obsidian plugin global probe

This development-only probe inventories globals visible to a loaded Obsidian
plugin. It helps verify which unqualified bindings and global-object aliases,
including `activeWindow`, are actually available in main and pop-out windows.

In addition to enumerating global-object properties, the plugin checks every
valid identifier exported by the `obsidian` module using unqualified binding
resolution. This discovers global lexical bindings that cannot be found with
`Object.getOwnPropertyNames(globalThis)`.

The report contains property names, descriptor flags, value types and tags,
and object-identity relationships. It does not serialize global values, vault
paths, filenames, or note content. It does not invoke arbitrary property
getters.

## Run it as a plugin

Copy `manifest.json` and the self-contained `main.js` into a disposable vault
as:

```text
<vault>/.obsidian/plugins/glass-lint-global-probe/
```

Enable **Glass Lint Global Probe** under **Settings → Community plugins**. Run
**Glass Lint Global Probe: Collect plugin runtime globals** from the command
palette, then use **Copy report** in the modal.

For multi-window coverage, open one or more Obsidian pop-out windows before
collecting. The probe finds realms through the workspace leaves and merges
references that identify the same window.

## Drive the plugin with Obsidian CLI

Obsidian 1.12.7 or newer can reload the copied development plugin and invoke
its command:

```sh
obsidian plugin:reload id=glass-lint-global-probe
obsidian command id=glass-lint-global-probe:collect
```

The command opens the same report modal and also logs the structured report to
the developer console. The most recent formatted JSON is retained as
`lastReport` on the loaded plugin instance for debugging.

The CLI also supports direct JavaScript evaluation. If CommonJS `require` is
available in your Obsidian renderer, the reusable probe can be called without
installing the plugin (replace the path with an absolute path):

```sh
obsidian eval code="JSON.stringify(require('/absolute/path/to/probe.cjs').collectObsidianGlobals({app, globalObject: globalThis}), null, 2)"
```

The plugin entry point does not import or require any sibling files. The plugin
route is preferred because it is less dependent on renderer developer settings
and is easy to repeat after opening pop-out windows. `probe.cjs` is only the
standalone CLI payload and is not loaded by the plugin.

## Verify the probe itself

The probe has no package dependencies. Run its smoke test with Node.js:

```sh
node tools/obsidian-global-probe/probe.test.cjs
```

When reporting results, send the complete JSON. The most useful fields are
`candidates.globalBindings`, `candidates.globalObjects`, `realms[*].sources`,
`obsidianModuleBindings`, and any differences in
`realms[*].identifierProperties`.
