// CommonJS entry for Node and jest. The wasm loads synchronously on require, so init is a no-op.
const { compileJson, hoverJson, completionsJson, signatureHelpJson, setNsBindings, version } = require("./pkg-node/carciniscript.js");

exports.initCarciniScript = async () => {};
exports.initCarciniScriptSync = () => {};
exports.compile = (source, filename = "main.rs", resolve) => JSON.parse(compileJson(source, filename, resolve));
exports.hover = (source, filename, line, column, resolve) =>
  JSON.parse(hoverJson(source, filename, line, column, resolve));
exports.completions = (source, filename, line, column, resolve) =>
  JSON.parse(completionsJson(source, filename, line, column, resolve));
exports.signatureHelp = (source, filename, line, column, resolve) =>
  JSON.parse(signatureHelpJson(source, filename, line, column, resolve));
exports.setNsBindings = (text) => JSON.parse(setNsBindings(text));
exports.version = version;
