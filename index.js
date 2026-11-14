import init, {
  initSync,
  compileJson,
  hoverJson,
  completionsJson,
  signatureHelpJson,
  definitionJson,
  referencesJson,
  renameJson,
  setNsBindings as setNsBindingsJson,
  version,
} from "./pkg/carciniscript.js";

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

/**
 * Compile Rust to an ES module. `resolve(path)` returns the source of another file, for `mod name;`
 * (which reads `name.rs` next to `filename`), or undefined when there's no such file.
 */
export function compile(source, filename = "main.rs", resolve) {
  if (!ready) throw new Error("CarciniScript is not initialized; call initCarciniScript() first");
  return JSON.parse(compileJson(source, filename, resolve));
}

/** Hover at a 1-based line and UTF-16 column: `{ contents (markdown), line, column, endLine, endColumn }` or null. */
export function hover(source, filename, line, column, resolve) {
  if (!ready) throw new Error("CarciniScript is not initialized; call initCarciniScript() first");
  return JSON.parse(hoverJson(source, filename, line, column, resolve));
}

/** Completions after a `.` at a 1-based line and UTF-16 column: `[{ label, kind, detail, documentation }]`. */
export function completions(source, filename, line, column, resolve) {
  if (!ready) throw new Error("CarciniScript is not initialized; call initCarciniScript() first");
  return JSON.parse(completionsJson(source, filename, line, column, resolve));
}

/** Parameter hints for the call around a 1-based line and UTF-16 column: `{ label, params, active, documentation }` or null. */
export function signatureHelp(source, filename, line, column, resolve) {
  if (!ready) throw new Error("CarciniScript is not initialized; call initCarciniScript() first");
  return JSON.parse(signatureHelpJson(source, filename, line, column, resolve));
}

/**
 * Where the name at a 1-based line and UTF-16 column is defined: `{ file, line, column, endLine,
 * endColumn }` (`file` is `filename`, or another file's path as passed to `resolve`) or null.
 */
export function definition(source, filename, line, column, resolve) {
  if (!ready) throw new Error("CarciniScript is not initialized; call initCarciniScript() first");
  return JSON.parse(definitionJson(source, filename, line, column, resolve));
}

/** Every use in this file of the name at a 1-based line and UTF-16 column, its declaration included. */
export function references(source, filename, line, column, resolve) {
  if (!ready) throw new Error("CarciniScript is not initialized; call initCarciniScript() first");
  return JSON.parse(referencesJson(source, filename, line, column, resolve));
}

/** The ranges to replace with `newName` to rename the name at a 1-based line and UTF-16 column (empty: can't). */
export function rename(source, filename, line, column, newName, resolve) {
  if (!ready) throw new Error("CarciniScript is not initialized; call initCarciniScript() first");
  return JSON.parse(renameJson(source, filename, line, column, newName, resolve));
}

/**
 * Install typed `NS` bindings: Rust declarations generated from NetscriptDefinitions.d.ts by
 * `tools/dts2rs.mjs`. Returns a list of problems (empty when the bindings parsed cleanly).
 */
export function setNsBindings(text) {
  if (!ready) throw new Error("CarciniScript is not initialized; call initCarciniScript() first");
  return JSON.parse(setNsBindingsJson(text));
}

export { version };
