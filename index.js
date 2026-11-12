import init, { initSync, compileJson, version } from "./pkg/carciniscript.js";

let ready = false;

/** Load the compiler. Call once at startup; `compile` is synchronous afterwards. */
export async function initCarciniScript(moduleOrPath) {
  if (ready) return;
  await init(moduleOrPath === undefined ? undefined : { module_or_path: moduleOrPath });
  ready = true;
}

/** Synchronous init from wasm bytes or a compiled WebAssembly.Module (used in Node and tests). */
export function initCarciniScriptSync(module) {
  if (ready) return;
  initSync({ module });
  ready = true;
}

export function compile(source, filename = "main.rs") {
  if (!ready) throw new Error("CarciniScript is not initialized; call initCarciniScript() first");
  return JSON.parse(compileJson(source, filename));
}

export { version };
