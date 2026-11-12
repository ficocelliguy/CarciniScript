// Builds the compiler to wasm and generates JS bindings: pkg/ for browsers and bundlers (async
// init), pkg-node/ for Node and jest (loads synchronously on require).
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
