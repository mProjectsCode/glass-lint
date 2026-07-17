# Obsidian plugin global probe

This development-only probe inventories globals visible to a loaded Obsidian
plugin, including unqualified bindings and global-object aliases in main and
pop-out windows.

It records property names, descriptor flags, value types, tags, and object
identity relationships. It does not serialize global values, vault paths,
filenames, or note content, and it does not invoke arbitrary getters.

## Run as a plugin

Copy `manifest.json` and the self-contained `main.js` into a disposable vault:

```text
<vault>/.obsidian/plugins/glass-lint-global-probe/
```

Enable the plugin and run **Glass Lint Global Probe: Collect plugin runtime
globals** from the command palette. Open any desired pop-out windows first,
then copy the complete JSON from the result modal.

If the Obsidian CLI is available, reload and invoke the plugin with:

```sh
obsidian plugin:reload id=glass-lint-global-probe
obsidian command id=glass-lint-global-probe:collect
```

The latest report is also logged to the developer console and retained as
`lastReport` on the plugin instance.

`probe.cjs` is a standalone payload for renderer environments where CommonJS
`require` is available:

```sh
obsidian eval code="JSON.stringify(require('/absolute/path/to/probe.cjs').collectObsidianGlobals({app, globalObject: globalThis}), null, 2)"
```

The plugin path is preferred because it is easier to repeat and does not rely
on renderer evaluation settings.

## Verify

The probe has no package dependencies:

```sh
node tools/obsidian-global-probe/probe.test.cjs
```

When reporting results, include the complete JSON. The key fields are
`candidates.globalBindings`, `candidates.globalObjects`, `realms[*].sources`,
`obsidianModuleBindings`, and differences in
`realms[*].identifierProperties`.
