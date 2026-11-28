// ES module entry for Node: the same API as index.js, but the init functions read the wasm from
// disk by default (Node's fetch can't load file: URLs, which wasm-bindgen's default init uses).
import { readFileSync } from "node:fs";
import { initCarciniScript as initFrom, initCarciniScriptSync as initSyncFrom } from "./index.js";

export * from "./index.js";

const wasmUrl = new URL("./pkg/carciniscript_bg.wasm", import.meta.url);

/** Load the compiler. Call once at startup; `compile` is synchronous afterwards. */
export async function initCarciniScript(moduleOrPath) {
  if (moduleOrPath === undefined) initSyncFrom(readFileSync(wasmUrl));
  else await initFrom(moduleOrPath);
}

/** Synchronous init, from the package's own wasm unless given bytes or a compiled module. */
export function initCarciniScriptSync(module) {
  initSyncFrom(module === undefined ? readFileSync(wasmUrl) : module);
}
