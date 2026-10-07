// Module loader for the store contract tests: redirects the Tauri JS APIs
// to programmable mocks so the REAL appStore.ts / tauri.ts run against a
// fake IPC layer. Registered via node:module register() before any import
// of the store.
import path from "node:path";
import { fileURLToPath } from "node:url";

const dir = path.dirname(fileURLToPath(import.meta.url));

export async function resolve(specifier, context, nextResolve) {
  if (specifier === "@tauri-apps/api/core") {
    return {
      url: `file://${dir}/mock-tauri-core.mjs`,
      shortCircuit: true,
    };
  }
  if (specifier === "@tauri-apps/api/event") {
    return {
      url: `file://${dir}/mock-tauri-event.mjs`,
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
