// CommonJS entry for Node and jest. The wasm loads synchronously on require, so init is a no-op.
const {
  compileJson,
  hoverJson,
  completionsJson,
  signatureHelpJson,
  definitionJson,
  referencesJson,
  renameJson,
  setNsBindings,
  setOverflowChecks,
  setStrictBorrows,
  version,
} = require("./pkg-node/carciniscript.js");

exports.initCarciniScript = async () => {};
exports.initCarciniScriptSync = () => {};
exports.compile = (source, filename = "main.rs", resolve) => JSON.parse(compileJson(source, filename, resolve));
exports.hover = (source, filename, line, column, resolve) =>
  JSON.parse(hoverJson(source, filename, line, column, resolve));
exports.completions = (source, filename, line, column, resolve) =>
  JSON.parse(completionsJson(source, filename, line, column, resolve));
exports.signatureHelp = (source, filename, line, column, resolve) =>
  JSON.parse(signatureHelpJson(source, filename, line, column, resolve));
exports.definition = (source, filename, line, column, resolve) =>
  JSON.parse(definitionJson(source, filename, line, column, resolve));
exports.references = (source, filename, line, column, resolve, others = []) =>
  JSON.parse(referencesJson(source, filename, line, column, resolve, others.join("\n")));
exports.rename = (source, filename, line, column, newName, resolve, others = []) =>
  JSON.parse(renameJson(source, filename, line, column, newName, resolve, others.join("\n")));
exports.setNsBindings = (text) => JSON.parse(setNsBindings(text));
exports.setOverflowChecks = (on) => setOverflowChecks(on);
exports.setStrictBorrows = (on) => setStrictBorrows(on);
exports.version = version;
