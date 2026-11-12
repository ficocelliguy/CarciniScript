// Test runner.
//  - fixtures/*.rs: differential. Compiled natively with rustc and to JS; stdout and exit code
//    (101 on panic) must match.
//  - ns/*.rs: async NS scripts run against a mock `ns`; output must match ns/*.expected.
//  - Source map and diagnostics checks.
import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { initCarciniScriptSync, compile } from "../index.js";
import "../runtime/rt.js";
import { nativePanic } from "../tools/native-panic.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(here, "out");
fs.mkdirSync(outDir, { recursive: true });
initCarciniScriptSync(fs.readFileSync(path.join(here, "../pkg/carciniscript_bg.wasm")));

let failures = 0;
const fail = (name, msg) => {
  failures++;
  console.log(`FAIL ${name}\n${msg}\n`);
};
const pass = (name) => console.log(`ok   ${name}`);

function compileFixture(file) {
  const src = fs.readFileSync(file, "utf8");
  const name = path.basename(file);
  const out = compile(src, name);
  const errors = out.diagnostics.filter((d) => d.severity === "error");
  if (errors.length) {
    throw new Error(errors.map((d) => `${name}:${d.line}:${d.column}: ${d.message}`).join("\n"));
  }
  const jsPath = path.join(outDir, name.replace(/\.rs$/, ".mjs"));
  const inline = Buffer.from(out.sourceMap).toString("base64");
  fs.writeFileSync(jsPath, `${out.code}\n//# sourceMappingURL=data:application/json;base64,${inline}\n`);
  return { src, out, jsPath };
}

async function runJs(jsPath, args = []) {
  const lines = [];
  __crs.$setOutput(
    (l) => lines.push(l),
    (l) => lines.push(`[stderr] ${l}`),
  );
  let code = 0;
  let panic = null;
  try {
    const mod = await import(pathToFileURL(jsPath).href + `?t=${Date.now()}`);
    await mod.main(...args);
  } catch (e) {
    if (e instanceof __crs.CarciniPanic) {
      code = 101;
      panic = `thread 'main' panicked at ${e.location}:\n${e.panicMessage}`;
    } else throw e;
  }
  __crs.$flush();
  const stdout = lines.filter((l) => !l.startsWith("[stderr] ")).join("\n");
  return { stdout: stdout ? stdout + "\n" : "", code, panic };
}

function runNative(file) {
  const exe = path.join(outDir, path.basename(file, ".rs") + (process.platform === "win32" ? ".exe" : ""));
  execFileSync("rustc", ["-O", "--edition", "2024", "-A", "warnings", "-o", exe, file], { stdio: "pipe" });
  const r = spawnSync(exe, { encoding: "utf8" });
  return {
    stdout: r.stdout.replace(/\r\n/g, "\n"),
    code: r.status,
    panic: r.status === 101 ? nativePanic(r.stderr) : null,
  };
}

// --- differential fixtures
for (const f of fs.readdirSync(path.join(here, "fixtures")).filter((f) => f.endsWith(".rs"))) {
  const file = path.join(here, "fixtures", f);
  const name = `diff ${f}`;
  try {
    const { jsPath } = compileFixture(file);
    const js = await runJs(jsPath);
    const native = runNative(file);
    if (js.stdout !== native.stdout || js.code !== native.code || js.panic !== native.panic) {
      const a = native.stdout.split("\n");
      const b = js.stdout.split("\n");
      const diff = [];
      for (let i = 0; i < Math.max(a.length, b.length); i++) {
        if (a[i] !== b[i]) diff.push(`  line ${i + 1}: rustc=${JSON.stringify(a[i])} js=${JSON.stringify(b[i])}`);
      }
      if (js.panic !== native.panic) {
        diff.push(`  panic rustc=${JSON.stringify(native.panic)}\n        js=${JSON.stringify(js.panic)}`);
      }
      fail(name, `exit rustc=${native.code} js=${js.code}\n${diff.join("\n")}`);
    } else pass(name);
  } catch (e) {
    fail(name, e.message);
  }
}

// --- NS scripts against a mock
function mockNs(log) {
  const state = { security: 10, minSecurity: 1, money: 100, maxMoney: 1000 };
  return {
    getServerMaxMoney: () => state.maxMoney,
    getServerMinSecurityLevel: () => state.minSecurity,
    getServerSecurityLevel: () => state.security,
    getServerMoneyAvailable: () => state.money,
    async weaken(t) {
      log.push(`weaken ${t}`);
      state.security = 1;
      return 9;
    },
    async grow(t) {
      log.push(`grow ${t}`);
      state.money = 1000;
      return 10;
    },
    async hack(t) {
      log.push(`hack ${t}`);
      return 1234.5678;
    },
    tprint: (s) => log.push(s),
  };
}
for (const f of fs.readdirSync(path.join(here, "ns")).filter((f) => f.endsWith(".rs"))) {
  const name = `ns ${f}`;
  try {
    const { out, jsPath } = compileFixture(path.join(here, "ns", f));
    const log = [];
    await runJs(jsPath, [mockNs(log)]);
    const expected = fs
      .readFileSync(path.join(here, "ns", f.replace(/\.rs$/, ".expected")), "utf8")
      .replace(/\r\n/g, "\n");
    if (log.join("\n") + "\n" !== expected) {
      fail(name, `expected:\n${expected}\ngot:\n${log.join("\n")}`);
      continue;
    }
    // RAM calculation needs literal camelCase member calls on `ns`.
    const missing = ["ns.getServerMaxMoney(", "ns.weaken(", "ns.hack(", "await ns.grow("].filter(
      (s) => !out.code.includes(s),
    );
    if (missing.length) fail(name, `generated code lacks ${missing.join(", ")}`);
    else pass(name);
  } catch (e) {
    fail(name, e.message);
  }
}

// --- source maps
function decodeMappings(mappings) {
  const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const result = [];
  let srcLine = 0;
  let srcCol = 0;
  mappings.split(";").forEach((line, genLine) => {
    let genCol = 0;
    for (const seg of line.split(",").filter(Boolean)) {
      const vals = [];
      let shift = 0;
      let value = 0;
      for (const ch of seg) {
        const d = B64.indexOf(ch);
        value += (d & 31) << shift;
        if (d & 32) shift += 5;
        else {
          vals.push(value & 1 ? -(value >> 1) : value >> 1);
          value = shift = 0;
        }
      }
      genCol += vals[0];
      srcLine += vals[2];
      srcCol += vals[3];
      result.push({ genLine, genCol, srcLine, srcCol });
    }
  });
  return result;
}
{
  const name = "sourcemap control.rs";
  try {
    const { src, out } = compileFixture(path.join(here, "fixtures/control.rs"));
    const maps = decodeMappings(JSON.parse(out.sourceMap).mappings);
    const genLines = out.code.split("\n");
    const srcLines = src.split(/\r?\n/);
    // Each probe: a JS snippet whose line must map back to the Rust line containing the needle.
    const probes = [
      ["return fib(", "fib(n - 1) + fib(n - 2)"],
      ["n = ((", "n = if n % 2 == 0"],
      ["$outer: for", "'outer: for a in 1..20"],
      ["break $outer;", "break 'outer;"],
      ["export function collatz_steps", "fn collatz_steps"],
    ];
    const problems = [];
    for (const [js, rust] of probes) {
      const gl = genLines.findIndex((l) => l.includes(js));
      const want = srcLines.findIndex((l) => l.includes(rust));
      const got = maps.filter((m) => m.genLine === gl).map((m) => m.srcLine);
      if (gl < 0 || want < 0 || got[0] !== want) {
        problems.push(`  ${JSON.stringify(js)}: js line ${gl + 1} -> rust line ${got[0] + 1}, want ${want + 1}`);
      }
    }
    // Column check: the second `fib(` call maps to its exact Rust column.
    const gl = genLines.findIndex((l) => l.includes("return fib("));
    const genCol = genLines[gl].lastIndexOf("fib(");
    const srcLine = srcLines.findIndex((l) => l.includes("fib(n - 1) + fib(n - 2)"));
    const want = srcLines[srcLine].lastIndexOf("fib(");
    const m = maps.find((m) => m.genLine === gl && m.genCol === genCol);
    if (!m || m.srcLine !== srcLine || m.srcCol !== want) {
      problems.push(`  second fib( call: got ${JSON.stringify(m)}, want line ${srcLine + 1} col ${want}`);
    }
    if (problems.length) fail(name, problems.join("\n"));
    else pass(name);
  } catch (e) {
    fail(name, e.message);
  }
}

// --- source maps: items with leading comments map to the `fn`, not the comment
{
  const name = "sourcemap leading comments";
  const src = "// a comment\n/// docs\n#[inline]\nfn f() -> i32 {\n    1\n}\n";
  const out = compile(src, "c.rs");
  const maps = decodeMappings(JSON.parse(out.sourceMap).mappings);
  const header = maps.filter((m) => m.genLine === 0).map((m) => m.srcLine);
  const close = maps.filter((m) => m.genLine === 2).map((m) => m.srcLine);
  if (header[0] !== 3 || close[0] !== 5) fail(name, `header -> ${header}, closing brace -> ${close}`);
  else pass(name);
}

// --- diagnostics
{
  const name = "diagnostics";
  const out = compile("fn main() {\n    let x = 1;\n    x = 2;\n    let s = match x { _ => 1 };\n}\n", "bad.rs");
  const msgs = out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`);
  const want = ["3:5 cannot assign twice", "4:13 `match` is not supported yet"];
  const missing = want.filter((w) => !msgs.some((m) => m.startsWith(w)));
  if (out.code !== null || missing.length) fail(name, `missing ${missing.join("; ")}\ngot:\n${msgs.join("\n")}`);
  else pass(name);
}

console.log(failures ? `\n${failures} failure(s)` : "\nall passed");
process.exit(failures ? 1 : 0);
