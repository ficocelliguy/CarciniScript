// Compile one .rs file with CarciniScript and with rustc, run both, and show where the outputs
// differ. For iterating on a feature before its program becomes a fixture.
//   node tools/diff-one.mjs path/to/program.rs [--js]
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { initCarciniScriptSync, compile } from "../index.js";
import "../runtime/rt.js";

const file = process.argv[2];
const showJs = process.argv.includes("--js");
initCarciniScriptSync(fs.readFileSync(new URL("../pkg/carciniscript_bg.wasm", import.meta.url)));
const src = fs.readFileSync(file, "utf8");
const out = compile(src, path.basename(file));
for (const d of out.diagnostics) console.log(`${d.severity} ${d.line}:${d.column} ${d.message}`);
if (out.code === null) process.exit(1);
if (showJs) console.log(out.code);

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "crs-"));
const jsPath = path.join(tmp, "prog.mjs");
fs.writeFileSync(jsPath, out.code);
const lines = [];
κσ.$setOutput(
  (l) => lines.push(l),
  (l) => lines.push(`[stderr] ${l}`),
);
let jsExit = 0;
try {
  const mod = await import(pathToFileURL(jsPath).href);
  await mod.main();
} catch (e) {
  jsExit = 101;
  lines.push(`[panic] ${e.message}`);
}
κσ.$flush();

const exe = path.join(tmp, "prog.exe");
const rc = spawnSync("rustc", ["-O", "--edition", "2024", "-A", "warnings", file, "-o", exe], { encoding: "utf8" });
if (rc.status !== 0) {
  console.log(rc.stderr);
  process.exit(1);
}
const native = spawnSync(exe, [], { encoding: "utf8" });
const want = native.stdout.split("\n").filter((l, i, a) => i < a.length - 1 || l !== "");
const got = lines.filter((l) => !l.startsWith("[stderr]") && !l.startsWith("[panic]"));
let same = native.status === jsExit || (native.status !== 0 && jsExit !== 0);
for (let i = 0; i < Math.max(want.length, got.length); i++) {
  if (want[i] !== got[i]) {
    console.log(`line ${i + 1}: rustc=${JSON.stringify(want[i])} js=${JSON.stringify(got[i])}`);
    same = false;
  }
}
for (const l of lines.filter((l) => l.startsWith("[panic]"))) console.log(l);
if (native.status !== 0) console.log(`[native stderr] ${native.stderr.trim()}`);
console.log(same ? `same (${want.length} lines, exit ${native.status})` : "DIFFERENT");
