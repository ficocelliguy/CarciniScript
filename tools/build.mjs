// Builds the compiler to wasm and generates JS bindings: pkg/ for browsers and bundlers (async
// init), pkg-node/ for Node and jest (loads synchronously on require, sharing pkg/'s wasm).
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import path from "node:path";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const run = (cmd, args, cwd) => execFileSync(cmd, args, { cwd, stdio: "inherit" });
const wasm = path.join(root, "compiler/target/wasm32-unknown-unknown/release/carciniscript.wasm");

run("cargo", ["build", "--release", "--target", "wasm32-unknown-unknown"], path.join(root, "compiler"));
run("wasm-bindgen", ["--target", "web", "--out-dir", path.join(root, "pkg"), wasm]);
run("wasm-bindgen", ["--target", "nodejs", "--out-dir", path.join(root, "pkg-node"), wasm]);
// wasm-bindgen's nodejs output is CommonJS, but this package is "type": "module".
fs.writeFileSync(path.join(root, "pkg-node/package.json"), '{ "type": "commonjs" }\n');
// Both targets emit the same wasm, so the package ships one copy: pkg-node's glue loads pkg/'s.
const nodeGlue = path.join(root, "pkg-node/carciniscript.js");
const glue = fs.readFileSync(nodeGlue, "utf8");
const wasmPathLine = "const wasmPath = `${__dirname}/carciniscript_bg.wasm`;";
if (!glue.includes(wasmPathLine)) throw new Error("build.mjs: wasm-bindgen's nodejs glue changed; update the wasm path rewrite");
if (!fs.readFileSync(path.join(root, "pkg/carciniscript_bg.wasm")).equals(fs.readFileSync(path.join(root, "pkg-node/carciniscript_bg.wasm"))))
  throw new Error("build.mjs: the web and nodejs wasm differ; pkg-node can't share pkg's");
fs.writeFileSync(nodeGlue, glue.replace(wasmPathLine, "const wasmPath = `${__dirname}/../pkg/carciniscript_bg.wasm`;"));
fs.rmSync(path.join(root, "pkg-node/carciniscript_bg.wasm"));
fs.rmSync(path.join(root, "pkg-node/carciniscript_bg.wasm.d.ts"));
// Types for `carciniscript/node` (node.cjs). index.d.ts is ESM in a "type": "module" package, and a
// .d.cts can't re-export it, so TypeScript's CommonJS consumers get a copy declared as CommonJS.
fs.copyFileSync(path.join(root, "index.d.ts"), path.join(root, "index.d.cts"));
