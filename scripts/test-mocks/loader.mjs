// Module loader for the store contract tests: redirects the Tauri JS APIs
// to programmable mocks so the REAL appStore.ts / tauri.ts run against a
// fake IPC layer. Registered via node:module register() before any import
// of the store.
import path from "node:path";
import { readFile } from "node:fs/promises";
import ts from "typescript";
import { fileURLToPath, pathToFileURL } from "node:url";

const dir = path.dirname(fileURLToPath(import.meta.url));
// Use pathToFileURL so the URL is canonical on every OS. A hand-built
// `file://${dir}/...` string keeps native backslashes on Windows, which
// creates a second module-cache entry: the test's direct import and the
// store's redirected import would then see different mock instances and
// registered invoke() handlers would silently miss.
const mockCoreUrl = pathToFileURL(path.join(dir, "mock-tauri-core.mjs")).href;
const mockEventUrl = pathToFileURL(path.join(dir, "mock-tauri-event.mjs")).href;

// Use the already-installed compiler rather than Node's version-dependent
// type stripping, so tests run on every version declared in package.json.
export async function load(url, context, nextLoad) {
  if (url.startsWith("file:") && new URL(url).pathname.endsWith(".ts")) {
    const source = await readFile(new URL(url), "utf8");
    return {
      format: "module",
      source: ts.transpileModule(source, {
        compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
        fileName: fileURLToPath(url),
      }).outputText,
      shortCircuit: true,
    };
  }
  return nextLoad(url, context);
}

export async function resolve(specifier, context, nextResolve) {
  if (specifier === "@tauri-apps/api/core") {
    return {
      url: mockCoreUrl,
      shortCircuit: true,
    };
  }
  if (specifier === "@tauri-apps/api/event") {
    return {
      url: mockEventUrl,
      shortCircuit: true,
    };
  }
  // TypeScript-style extensionless relative imports: fall back to .ts,
  // then to directory index. (Node's ESM loader requires explicit paths;
  // the app source omits them, which is fine under the bundler.)
  try {
    return await nextResolve(specifier, context);
  } catch (err) {
    const isRelative = specifier.startsWith("./") || specifier.startsWith("../");
    const hasExt = /\.(ts|mjs|js|json)$/.test(specifier);
    if (isRelative && !hasExt) {
      try {
        return await nextResolve(`${specifier}.ts`, context);
      } catch {
        return nextResolve(`${specifier}/index.ts`, context);
      }
    }
    throw err;
  }
}
