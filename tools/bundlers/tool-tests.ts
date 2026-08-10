import { spawnSync } from "node:child_process";

type Transformer = "vite" | "esbuild";

function run(request: object): string {
  const result = spawnSync("bun", ["run", "runner.ts"], {
    input: JSON.stringify(request),
    encoding: "utf8",
  });
  if (result.status !== 0) throw new Error(result.stderr || "bundler test failed");
  return (JSON.parse(result.stdout) as { generated_source: string }).generated_source;
}

for (const transformer of ["vite", "esbuild"] as Transformer[]) {
  const local = {
    protocol_version: 1,
    transformer,
    profile: "web",
    entry: "main.js",
    language: "javascript",
    minified: true,
    target: "ES2022",
    files: [
      { path: "main.js", language: "javascript", source: "import { value } from './local.js'; globalThis.value = value;" },
      { path: "local.js", language: "javascript", source: "export var value = 1;" },
    ],
  };
  const first = run(local);
  const second = run(local);
  if (first !== second || !first.includes("globalThis.value")) {
    throw new Error(transformer + " local bundling is not deterministic or complete");
  }

  const obsidian = {
    ...local,
    profile: "obsidian",
    minified: false,
    files: [
      {
        path: "main.js",
        language: "javascript",
        source: "import { requestUrl } from 'obsidian'; globalThis.requestUrl = requestUrl;",
      },
    ],
  };
  const external = run(obsidian);
  if (!external.includes("obsidian")) {
    throw new Error(transformer + " did not preserve the Obsidian host external");
  }
}

console.log("bundler tool tests passed: profiles, local imports, externals, deterministic output");
