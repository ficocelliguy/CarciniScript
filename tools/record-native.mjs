// Records native rustc output for each test fixture into test/fixtures/manifest.json, so the web
// REPL's self-test can check browser-transpiled output without a Rust toolchain.
import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { nativePanic } from "./native-panic.mjs";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const dir = path.join(root, "test/fixtures");
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "crs-native-"));
const manifest = [];
for (const f of fs.readdirSync(dir).filter((f) => f.endsWith(".rs")).sort()) {
  const exe = path.join(tmp, f.replace(/\.rs$/, process.platform === "win32" ? ".exe" : ""));
  execFileSync("rustc", ["-O", "--edition", "2024", "-A", "warnings", "-o", exe, path.join(dir, f)]);
  const r = spawnSync(exe, { encoding: "utf8" });
  const panic = r.status === 101 ? nativePanic(r.stderr) : null;
  manifest.push({ file: f, stdout: r.stdout.replace(/\r\n/g, "\n"), exitCode: r.status, panic });
  console.log(`${f}: exit ${r.status}, ${r.stdout.split("\n").length - 1} lines`);
}
fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
fs.rmSync(tmp, { recursive: true, force: true });
