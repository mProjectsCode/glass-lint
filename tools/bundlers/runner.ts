import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const PROTOCOL_VERSION = 1;
const MAX_REQUEST_BYTES = 4 * 1024 * 1024;
const MAX_FILES = 256;
const MAX_FILE_BYTES = 512 * 1024;
const MAX_OUTPUT_BYTES = 4 * 1024 * 1024;

type Profile = "web" | "obsidian";
type Transformer = "vite" | "esbuild";
type Target = "ES5" | "ES6" | "ES2017" | "ES2022" | "ESNEXT";
type Request = {
  protocol_version: number;
  transformer: Transformer;
  profile: Profile;
  entry: string;
  language: string;
  minified: boolean;
  target: Target;
  files: Array<{ path: string; language: string; source: string }>;
};

const targetMap: Record<Target, string> = {
  ES5: "es5",
  ES6: "es2015",
  ES2017: "es2017",
  ES2022: "es2022",
  ESNEXT: "esnext",
};

function target(request: Request): string {
  const value = targetMap[request.target];
  if (!value) throw new Error("unsupported target " + request.target);
  return value;
}

function normalizePath(path: string): string {
  const parts: string[] = [];
  for (const part of path.replaceAll("\\", "/").split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!parts.length) throw new Error("path escapes project: " + path);
      parts.pop();
    } else {
      parts.push(part);
    }
  }
  return parts.join("/");
}

function isBareImport(id: string): boolean {
  return !id.startsWith(".") && !id.startsWith("/");
}

function isExternal(profile: Profile, id: string): boolean {
  if (!isBareImport(id)) return false;
  if (profile === "web") return true;
  const root = id.split("/")[0];
  return root === "obsidian" || root === "electron";
}

function bareImports(source: string): string[] {
  const imports = new Set<string>();
  const pattern = /(?:import|export)\s+(?:[^"']*?\s+from\s+)?["']([^"']+)["']|require\(\s*["']([^"']+)["']\s*\)/g;
  for (const match of source.matchAll(pattern)) {
    const id = match[1] ?? match[2];
    if (id && isBareImport(id)) imports.add(id);
  }
  return [...imports];
}

async function bundleWithEsbuild(request: Request, files: Map<string, string>): Promise<string> {
  const entry = normalizePath(request.entry);
  const root = await mkdtemp(join(tmpdir(), "glass-lint-bundle-"));
  try {
    await Promise.all([...files].map(([path, source]) => Bun.write(join(root, path), source)));
    const output = join(root, "bundle.js");
    const args = [
      "node_modules/esbuild/bin/esbuild",
      join(root, entry),
      "--bundle",
      "--outfile=" + output,
      "--format=esm",
      "--target=" + target(request),
      "--legal-comments=none",
      "--log-level=error",
    ];
    if (request.minified) args.push("--minify");
    for (const file of files.values()) {
      for (const id of bareImports(file)) {
        if (request.profile === "web" || isExternal(request.profile, id)) {
          args.push("--external:" + id);
        }
      }
    }
    const process = Bun.spawn(args, { stdout: "pipe", stderr: "pipe", cwd: "." });
    const status = await process.exited;
    const stderr = await new Response(process.stderr).text();
    if (status !== 0) throw new Error(stderr.slice(0, 8192) || "bundler process failed");
    const source = (await Bun.file(output).text()).replace(
      /^\/\/ .*glass-lint-bundle-[^\n]*\n/m,
      "",
    );
    if (!source || (await Bun.file(output).size) === 0) throw new Error("bundler produced no JavaScript output");
    return source;
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function bundleWithVite(request: Request, files: Map<string, string>): Promise<string> {
  const entry = normalizePath(request.entry);
  const root = await mkdtemp(join(tmpdir(), "glass-lint-vite-"));
  try {
    await Promise.all([...files].map(([path, source]) => Bun.write(join(root, path), source)));
    const output = join(root, "out");
    const config = join(root, "vite.config.mjs");
    const stdoutLog = join(root, "vite.stdout.log");
    const stderrLog = join(root, "vite.stderr.log");
    const configSource = [
      "const profile = " + JSON.stringify(request.profile) + ";",
      "const target = " + JSON.stringify(target(request)) + ";",
      "const minify = " + JSON.stringify(request.minified ? "esbuild" : false) + ";",
      "const entry = " + JSON.stringify(join(root, entry)) + ";",
      "const output = " + JSON.stringify(output) + ";",
      "const host = new Set(['obsidian', 'electron']);",
      "const bare = (id) => !id.startsWith('.') && !id.startsWith('/');",
      "const external = (id) => {",
      "  if (id === entry) return false;",
      "  if (!bare(id)) return false;",
      "  if (profile === 'web') return true;",
      "  if (host.has(id.split('/')[0])) return true;",
      "  throw new Error('non-host bare import is not allowed: ' + id);",
      "};",
      "export default {",
      "  root: " + JSON.stringify(root) + ",",
      "  build: { write: true, outDir: output, emptyOutDir: true, sourcemap: false, target, minify,",
      "    rollupOptions: { input: entry, external, preserveEntrySignatures: 'strict', output: { format: 'es', inlineDynamicImports: true, entryFileNames: 'bundle.js', chunkFileNames: 'bundle.js', assetFileNames: 'asset.bin' } } }",
      "};",
    ].join("\n");
    await Bun.write(config, configSource);
    const process = Bun.spawn(
      ["node", "node_modules/vite/bin/vite.js", "build", "--config", config],
      { stdout: Bun.file(stdoutLog), stderr: Bun.file(stderrLog), cwd: "." },
    );
    const status = await process.exited;
    const stdout = await Bun.file(stdoutLog).text();
    const stderr = await Bun.file(stderrLog).text();
    if (status !== 0) throw new Error((stderr || stdout).slice(0, 8192) || "vite process failed, status=" + status);
    const outputs: string[] = [];
    for await (const file of new Bun.Glob("**/*").scan({ cwd: output, onlyFiles: true })) {
      if (!file.endsWith(".js")) throw new Error("vite produced a non-JavaScript asset: " + file);
      outputs.push(file);
    }
    if (outputs.length !== 1) throw new Error("vite must produce exactly one JavaScript asset");
    return (await Bun.file(join(output, outputs[0])).text()).replace(
      /^\/\/ .*glass-lint-vite-[^\n]*\n/m,
      "",
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function main(request: Request): Promise<object> {
  const encoded = JSON.stringify(request);
  if (encoded.length > MAX_REQUEST_BYTES) throw new Error("request exceeds size limit");
  if (request.protocol_version !== PROTOCOL_VERSION) throw new Error("unsupported protocol version");
  if (!Array.isArray(request.files) || request.files.length === 0 || request.files.length > MAX_FILES) {
    throw new Error("invalid file count");
  }
  const files = new Map<string, string>();
  for (const file of request.files) {
    const path = normalizePath(file.path);
    if (file.source.length > MAX_FILE_BYTES) throw new Error("input exceeds size limit: " + path);
    if (files.has(path)) throw new Error("duplicate input file: " + path);
    files.set(path, file.source);
  }
  if (!files.has(normalizePath(request.entry))) throw new Error("entry is not supplied");
  const source = request.transformer === "vite"
    ? await bundleWithVite(request, files)
    : await bundleWithEsbuild(request, files);
  if (Buffer.byteLength(source) > MAX_OUTPUT_BYTES) throw new Error("generated source exceeds size limit");
  return {
    protocol_version: PROTOCOL_VERSION,
    transformer: request.transformer,
    transformer_version: request.transformer === "vite" ? "vite@6.3.5" : "esbuild@0.25.5",
    profile: request.profile,
    generated_source: source,
  };
}

const input = await Bun.stdin.text();
try {
  const request = JSON.parse(input) as Request;
  console.log(JSON.stringify(await main(request)));
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
