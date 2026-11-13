// CommonJS entry for Node and jest. The wasm loads synchronously on require, so init is a no-op.
const { compileJson, setNsBindings, version } = require("./pkg-node/carciniscript.js");

exports.initCarciniScript = async () => {};
exports.initCarciniScriptSync = () => {};
exports.compile = (source, filename = "main.rs") => JSON.parse(compileJson(source, filename));
exports.setNsBindings = (text) => JSON.parse(setNsBindings(text));
exports.version = version;
