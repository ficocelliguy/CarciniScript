// The MVP acceptance test: test/mvp/ holds scripts written the way a Rust user writes them
// (scanners and hack loops, contract solvers, a two-file script, a `#[test]` module, panics, and
// the usual mistakes). Each must match rustc 2024 as `cargo build` runs it (a debug build):
//  - the rendered errors and warnings, line for line apart from paths;
//  - if it compiles, stdout, the exit code and the panic text;
//  - with `#[test]`s, libtest's report from `rustc --test`.
// `x.rs` is a script; `x/` is a program of several files, `x/main.rs` its root. A Netscript script
// (`pub async fn main(ns: NS)`) runs natively against tools/ns-shim.rs and here against a mock
// of the same network; `// args: ..` gives its `ns.args`. Arguments filter the cases by name.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { initCarciniScriptSync, compile, compileTests, setNsBindings, setOverflowChecks } from "../../index.js";
import "../../runtime/rt.js";
import { nativePanic } from "../../tools/native-panic.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(here, "../out/mvp");
fs.mkdirSync(outDir, { recursive: true });
initCarciniScriptSync(fs.readFileSync(path.join(here, "../../pkg/carciniscript_bg.wasm")));
setNsBindings(fs.readFileSync(path.join(here, "../../bindings/ns.rs"), "utf8"));
setOverflowChecks(true);
const shimPath = path.join(here, "../../tools/ns-shim.rs");
const exeSuffix = process.platform === "win32" ? ".exe" : "";
const slashes = (p) => p.replaceAll("\\", "/");

let failures = 0;
const fail = (name, msg) => {
  failures++;
  console.log(`FAIL ${name}\n${msg}\n`);
};
const pass = (name) => console.log(`ok   ${name}`);

// Differences this test found that are left for after the MVP: the case runs and its
// differences are shown, but it doesn't fail the run until it passes (then it must come off
// the list).
const KNOWN = {
};
let known = 0;

// Paths in rustc's output are the file names alone (`--> x.rs:3:5`, `panicked at x.rs:3:5:`).
const normalize = (s) =>
  s
    .replace(/\r\n/g, "\n")
    .replace(/^(thread '[^']*') \(\d+\)/gm, "$1")
    .replace(/[^\s'"`]*[\\/]([^\\/\s]+\.rs:\d+:\d+)/g, "$1")
    .replace(/finished in \d+\.\d+s/, "finished in 0.00s");

// The mock network: tools/ns-shim.rs's `SERVERS`, with the same behavior.
function mockNs(args) {
  const shim = fs.readFileSync(shimPath, "utf8");
  const servers = new Map();
  for (const m of shim.matchAll(/^\s*\("([^"]+)", (\d+), (\d+), ([\d.]+), ([\d.]+), ([\d.]+), ([\d.]+), ([\d.]+), &\[(.*)\]\),$/gm)) {
    const [, name, ports, level, maxMoney, minSec, sec, money, ram, next] = m;
    servers.set(name, {
      ports: +ports,
      level: +level,
      maxMoney: +maxMoney,
      minSec: +minSec,
      security: +sec,
      money: +money,
      ram: +ram,
      next: [...next.matchAll(/"([^"]+)"/g)].map((n) => n[1]),
      opened: 0,
      rooted: name === "home",
    });
  }
  const hackingLevel = +/const HACKING_LEVEL: i32 = (\d+);/.exec(shim)[1];
  const programs = [.../const PROGRAMS: &\[&str\] = &\[(.*)\];/.exec(shim)[1].matchAll(/"([^"]+)"/g)].map((p) => p[1]);
  const get = (host) => {
    const s = servers.get(host);
    if (!s) throw new Error(`unknown host ${host}`);
    return s;
  };
  return {
    args,
    tprint: (s) => κσ.$println(String(s)),
    print: (s) => κσ.$println(`[log] ${s}`),
    getHostname: () => "home",
    getHackingLevel: () => hackingLevel,
    scan: (host) => [...get(host).next],
    fileExists: (file, host) => host === "home" && programs.includes(file),
    brutessh: (host) => (get(host).opened++, true),
    ftpcrack: (host) => (get(host).opened++, true),
    nuke(host) {
      const s = get(host);
      if (s.opened < s.ports) return false;
      return (s.rooted = true);
    },
    hasRootAccess: (host) => get(host).rooted,
    getServerNumPortsRequired: (host) => get(host).ports,
    getServerRequiredHackingLevel: (host) => get(host).level,
    getServerMaxMoney: (host) => get(host).maxMoney,
    getServerMinSecurityLevel: (host) => get(host).minSec,
    getServerSecurityLevel: (host) => get(host).security,
    getServerMoneyAvailable: (host) => get(host).money,
    getServerMaxRam: (host) => get(host).ram,
    getServerUsedRam: (host) => (host === "home" ? 8 : 0),
    async hack(host) {
      const s = get(host);
      if (!s.rooted || s.level > hackingLevel) return 0;
      const stolen = s.money * 0.1;
      s.money -= stolen;
      s.security += 0.5;
      return stolen;
    },
    async grow(host) {
      const s = get(host);
      if (!s.rooted) return 1;
      const before = s.money;
      s.money = Math.min(s.money * 1.5 + 10000, s.maxMoney);
      s.security += 1;
      return s.money / before;
    },
    async weaken(host) {
      const s = get(host);
      if (!s.rooted) return 0;
      const before = s.security;
      s.security = Math.max(s.security - 2, s.minSec);
      return before - s.security;
    },
    sleep: async () => true,
  };
}

async function runJs(jsPath, args) {
  const lines = [];
  κσ.$setOutput(
    (l) => lines.push(l),
    (l) => lines.push(`[stderr] ${l}`),
  );
  let code = 0;
  let panic = null;
  try {
    const mod = await import(pathToFileURL(jsPath).href + `?t=${Date.now()}`);
    await mod.main(...args);
  } catch (e) {
    if (e instanceof κσ.CarciniPanic) {
      code = 101;
      panic = String(e).split("\nnote: ")[0];
    } else if (e instanceof κσ.CarciniError) {
      code = 1;
    } else if (e.name === "CarciniExit") {
      code = e.exitCode;
    } else throw e;
  }
  κσ.$flush();
  const stdout = lines.filter((l) => !l.startsWith("[stderr] ")).map((l) => `${l}\n`).join("");
  return { stdout, code, panic };
}

// The script's arguments, as the JS has them and as Rust source for the shim.
function scriptArgs(src) {
  const words = /^\/\/ args: (.*)$/m.exec(src)?.[1].trim().split(/\s+/) ?? [];
  const js = words.map((w) => (w === "true" || w === "false" ? w === "true" : isNaN(Number(w)) ? w : Number(w)));
  const rs = js.map((v) =>
    typeof v === "string" ? `ns::ScriptArg::Str(${JSON.stringify(v)}.to_string())` : typeof v === "number" ? `ns::ScriptArg::Num(${v}f64)` : `ns::ScriptArg::Bool(${v})`,
  );
  return { words, js, rs: `vec![${rs.join(", ")}]` };
}

const filter = process.argv.slice(2);
for (const entry of fs.readdirSync(here).sort()) {
  const isDir = fs.statSync(path.join(here, entry)).isDirectory();
  if (!isDir && !entry.endsWith(".rs")) continue;
  const caseName = entry.replace(/\.rs$/, "");
  if (filter.length && !filter.some((f) => caseName.includes(f))) continue;
  const name = `mvp ${entry}`;
  try {
    const root = isDir ? path.join(here, entry) : here;
    const file = isDir ? "main.rs" : entry;
    const read = (p) => {
      try {
        return fs.readFileSync(path.join(root, p), "utf8");
      } catch {
        return undefined;
      }
    };
    const src = read(file);
    const isNs = /pub async fn main\(ns: NS\)/.test(src);
    const args = scriptArgs(src);
    const work = path.join(outDir, caseName);
    fs.mkdirSync(work, { recursive: true });

    // rustc, as `cargo build` runs it.
    let crateRoot = path.join(root, file);
    if (isNs) {
      crateRoot = path.join(work, "wrapper.rs");
      fs.writeFileSync(
        crateRoot,
        `#[path = "${slashes(shimPath)}"] mod ns;\n` +
          `mod script { #[allow(unused_imports)] use crate::ns::*; include!("${slashes(path.join(root, file))}"); }\n` +
          `fn main() { ns::block_on(script::main(ns::NS::new(${args.rs}))); }\n`,
      );
    }
    const exe = path.join(work, `native${exeSuffix}`);
    const built = spawnSync("rustc", ["--edition", "2024", "--crate-type", "bin", "-o", exe, path.relative(root, crateRoot)], { cwd: root, encoding: "utf8" });
    const want = normalize(built.stderr);

    // Ours: every file of the program, compiled to an ES module each.
    const files = isDir ? fs.readdirSync(root, { recursive: true }).map(slashes).filter((f) => f.endsWith(".rs")) : [file];
    let rendered = "";
    let compiled = true;
    for (const f of files) {
      const out = compile(read(f), f, read);
      if (f === file) rendered = out.rendered;
      if (out.code === null) {
        compiled = false;
        continue;
      }
      const dest = path.join(work, f.replace(/\.rs$/, ".mjs"));
      fs.mkdirSync(path.dirname(dest), { recursive: true });
      fs.writeFileSync(dest, out.code.replace(/\.rs";/g, '.mjs";'));
    }
    const problems = [];
    if (normalize(rendered) !== want) problems.push(`--- rustc\n${want}--- ours\n${normalize(rendered)}`);
    if ((built.status === 0) !== compiled) problems.push(`rustc ${built.status === 0 ? "compiles it" : "rejects it"}, ours ${compiled ? "compiles it" : "rejects it"}`);

    if (built.status === 0 && compiled) {
      const native = spawnSync(exe, isNs ? [] : args.words, { encoding: "utf8" });
      const nativeOut = { stdout: native.stdout.replace(/\r\n/g, "\n"), code: native.status, panic: native.status === 101 ? normalize(nativePanic(native.stderr)) : null };
      const js = await runJs(path.join(work, file.replace(/\.rs$/, ".mjs")), isNs ? [mockNs(args.js)] : []);
      if (js.panic) js.panic = normalize(js.panic);
      if (js.stdout !== nativeOut.stdout) {
        const a = nativeOut.stdout.split("\n");
        const b = js.stdout.split("\n");
        const diff = [];
        for (let i = 0; i < Math.max(a.length, b.length); i++) if (a[i] !== b[i]) diff.push(`  line ${i + 1}: rustc=${JSON.stringify(a[i])} js=${JSON.stringify(b[i])}`);
        problems.push(`stdout differs\n${diff.slice(0, 20).join("\n")}`);
      }
      if (js.code !== nativeOut.code) problems.push(`exit rustc=${nativeOut.code} js=${js.code}`);
      if (js.panic !== nativeOut.panic) problems.push(`panic\n  rustc=${JSON.stringify(nativeOut.panic)}\n  js=   ${JSON.stringify(js.panic)}`);
    }

    // The test build.
    if (/#\[test\]/.test(src)) {
      const out = compileTests(src, file, read);
      if (out.code === null) problems.push(`test build:\n${out.rendered}`);
      else {
        const jsPath = path.join(work, "tests.mjs");
        fs.writeFileSync(jsPath, out.code);
        const js = await runJs(jsPath, [{ args: [] }]);
        const testExe = path.join(work, `tests${exeSuffix}`);
        spawnSync("rustc", ["--test", "--edition", "2024", "-A", "warnings", "-o", testExe, path.join(root, file)], { encoding: "utf8" });
        const r = spawnSync(testExe, ["--test-threads=1"], { encoding: "utf8" });
        const [w, g] = [normalize(r.stdout), normalize(js.stdout)];
        if (w !== g) problems.push(`--- rustc --test\n${w}--- ours\n${g}`);
      }
    }
    if (KNOWN[entry] && problems.length) {
      known++;
      console.log(`known ${name}: ${KNOWN[entry]}\n${problems.join("\n")}\n`);
    } else if (KNOWN[entry]) fail(name, "passes now: take it off KNOWN");
    else if (problems.length) fail(name, problems.join("\n"));
    else pass(name);
  } catch (e) {
    fail(name, e.stack);
  }
}

const knownNote = known ? ` (${known} known difference(s), above)` : "";
console.log(failures ? `\n${failures} failure(s)${knownNote}` : `\nall passed${knownNote}`);
process.exit(failures ? 1 : 0);
