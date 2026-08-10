// Probe the exact locked toolchain before fixture expectations are added.
import { spawnSync } from "node:child_process";

const targets = ["ES5", "ES6", "ES2017", "ES2022", "ESNEXT"];
for (const transformer of ["vite", "esbuild"]) {
  for (const minified of [false, true]) {
    for (const target of targets) {
      const request = {
        protocol_version: 1,
        transformer,
        profile: "web",
        entry: "main.js",
        language: "javascript",
        minified,
        target,
        files: [{ path: "main.js", language: "javascript", source: "var value = 1; globalThis.value = value;" }],
      };
      const result = spawnSync("bun", ["run", "runner.ts"], {
        input: JSON.stringify(request),
        encoding: "utf8",
      });
      if (result.status !== 0) {
        throw new Error(transformer + " " + target + " minified=" + minified + ": " + result.stderr);
      }
      JSON.parse(result.stdout);
    }
  }
}
console.log("bundler compatibility probe passed: 20 matrix cells");
