// Test runner.
//  - fixtures/*.rs: differential. Compiled natively with rustc and to JS; stdout and exit code
//    (101 on panic) must match.
//  - ns/*.rs: async NS scripts run against a mock `ns`; output must match ns/*.expected.
//  - Source map and diagnostics checks.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import {
  initCarciniScriptSync,
  compile,
  setNsBindings,
  setOverflowChecks,
  setStrictBorrows,
  hover,
  completions,
  signatureHelp,
  definition,
  references,
  rename,
} from "../index.js";
import "../runtime/rt.js";
import { buildNative } from "../tools/native-build.mjs";
import { nativePanic } from "../tools/native-panic.mjs";

// `js::Date`'s local time is UTC in the tests (the native shim has no time zones).
process.env.TZ = "UTC";
const here = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(here, "out");
fs.mkdirSync(outDir, { recursive: true });
initCarciniScriptSync(fs.readFileSync(path.join(here, "../pkg/carciniscript_bg.wasm")));
// Typed NS bindings generated from Bitburner's NetscriptDefinitions.d.ts (tools/dts2rs.mjs).
const bindingProblems = setNsBindings(fs.readFileSync(path.join(here, "../bindings/ns.rs"), "utf8"));
if (bindingProblems.length) {
  console.log(`FAIL bindings/ns.rs\n${bindingProblems.join("\n")}`);
  process.exit(1);
}

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
      panic = `thread 'main' panicked at ${e.location}:\n${e.panicMessage}`;
    } else if (e.name === "CarciniError") {
      // `fn main() -> Result<..>` returned `Err`: rustc's process exits with 1.
      code = 1;
    } else if (e.name === "CarciniExit") {
      // `std::process::exit(code)`.
      code = e.exitCode;
    } else throw e;
  }
  κσ.$flush();
  const stdout = lines.filter((l) => !l.startsWith("[stderr] ")).join("\n");
  return { stdout: stdout ? stdout + "\n" : "", code, panic };
}

function runNative(file) {
  const exe = path.join(outDir, path.basename(file, ".rs") + (process.platform === "win32" ? ".exe" : ""));
  buildNative(file, exe);
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

// --- multi-file programs: test/modules/<name>/main.rs plus its `mod` files
for (const dir of fs.readdirSync(path.join(here, "modules"))) {
  const root = path.join(here, "modules", dir);
  const name = `modules ${dir}`;
  try {
    const read = (p) => {
      try {
        return fs.readFileSync(path.join(root, p), "utf8");
      } catch {
        return undefined;
      }
    };
    const outRoot = path.join(outDir, `modules-${dir}`);
    fs.mkdirSync(outRoot, { recursive: true });
    // Every `.rs` file, also in subdirectories (`geo/mod.rs`), by its path from the root.
    const files = fs
      .readdirSync(root, { recursive: true })
      .map((f) => f.replaceAll("\\", "/"))
      .filter((f) => f.endsWith(".rs"));
    for (const f of files) {
      const out = compile(read(f), f, read);
      const errors = out.diagnostics.filter((d) => d.severity === "error");
      if (errors.length) throw new Error(errors.map((d) => `${f}:${d.line}:${d.column}: ${d.message}`).join("\n"));
      // Node can't import `.rs`; the compiled modules are written as `.mjs`.
      const dest = path.join(outRoot, f.replace(/\.rs$/, ".mjs"));
      fs.mkdirSync(path.dirname(dest), { recursive: true });
      fs.writeFileSync(dest, out.code.replace(/\.rs";/g, '.mjs";'));
    }
    const js = await runJs(path.join(outRoot, "main.mjs"));
    const native = runNative(path.join(root, "main.rs"));
    if (js.stdout !== native.stdout || js.code !== native.code) {
      fail(name, `rustc:\n${native.stdout}\njs:\n${js.stdout}`);
    } else pass(name);
  } catch (e) {
    fail(name, e.message);
  }
}

// --- cross-folder modules by absolute path: `#[path = "/lib/util.rs"]` from a script in another
// folder (Bitburner's `/lib/` layout), also `include_str!("/lib/..")`. The import is written
// `/lib/util.rs` for the host to resolve from the root. rustc has no such root, so this case has
// no native oracle; the expected output is written out.
{
  const name = "modules absolute #[path]";
  try {
    const files = {
      "hack/main.rs":
        '#[path = "/lib/util.rs"]\nmod util;\n\nfn main() {\n    println!("{}", util::greet("hack"));\n    println!("{}", util::FIRST.trim());\n}\n',
      "lib/util.rs": 'pub const FIRST: &str = include_str!("/lib/first.txt");\n\npub fn greet(who: &str) -> String {\n    format!("hi {who}")\n}\n',
      "lib/first.txt": "one\n",
    };
    const read = (p) => files[p];
    const outRoot = path.join(outDir, "modules-absolute");
    for (const f of Object.keys(files).filter((f) => f.endsWith(".rs"))) {
      const out = compile(read(f), f, read);
      const errors = out.diagnostics.filter((d) => d.severity === "error");
      if (errors.length) throw new Error(errors.map((d) => `${f}:${d.line}:${d.column}: ${d.message}`).join("\n"));
      if (f === "hack/main.rs" && !out.code.includes('from "/lib/util.rs"')) throw new Error(`the import isn't from the root:\n${out.code}`);
      const dest = path.join(outRoot, f.replace(/\.rs$/, ".mjs"));
      fs.mkdirSync(path.dirname(dest), { recursive: true });
      // Node has no project root: an absolute import becomes a relative one.
      const code = out.code
        .replace(/from "\/(.*?)\.rs";/g, (_, p) => `from "${path.relative(path.dirname(f), p).replaceAll("\\", "/")}.mjs";`)
        .replace(/\.rs";/g, '.mjs";');
      fs.writeFileSync(dest, code);
    }
    const js = await runJs(path.join(outRoot, "hack/main.mjs"));
    const want = "hi hack\none\n";
    if (js.stdout !== want || js.code !== 0) fail(name, `want:\n${want}\njs (exit ${js.code}):\n${js.stdout}`);
    else pass(name);
  } catch (e) {
    fail(name, e.message);
  }
}

// --- proc macros: test/procmacro/<name>/main.rs plus proc-macro crates (`<crate>.rs`, the files
// with `#[proc_macro..]` functions). Natively each crate is a `proc-macro = true` crate of a
// cargo workspace; the build directory is shared between cases.
for (const dir of fs.readdirSync(path.join(here, "procmacro"))) {
  const root = path.join(here, "procmacro", dir);
  const name = `procmacro ${dir}`;
  try {
    const read = (p) => {
      try {
        return fs.readFileSync(path.join(root, p), "utf8");
      } catch {
        return undefined;
      }
    };
    const out = compile(read("main.rs"), "main.rs", read);
    const errors = out.diagnostics.filter((d) => d.severity === "error");
    if (errors.length) throw new Error(errors.map((d) => `main.rs:${d.line}:${d.column}: ${d.message}`).join("\n"));
    const jsPath = path.join(outDir, `procmacro-${dir}.mjs`);
    fs.writeFileSync(jsPath, out.code);
    const js = await runJs(jsPath);
    // The cargo workspace.
    const ws = path.join(outDir, `procmacro-${dir}`);
    const crates = fs.readdirSync(root).filter((f) => f !== "main.rs" && /#\[proc_macro/.test(read(f)));
    fs.rmSync(ws, { recursive: true, force: true });
    fs.mkdirSync(path.join(ws, "app", "src"), { recursive: true });
    const members = ["app"];
    const deps = [];
    for (const f of crates) {
      const c = path.basename(f, ".rs");
      members.push(c);
      deps.push(`${c} = { path = "../${c}" }`);
      fs.mkdirSync(path.join(ws, c, "src"), { recursive: true });
      // The `quote` crate (and `proc_macro2`) when the macros use them: from cargo's cache.
      const text = read(f);
      const crateDeps = [
        /\bquote\b/.test(text) ? 'quote = "1"' : "",
        /\bproc_macro2\b/.test(text) ? 'proc-macro2 = "1"' : "",
        /\bsyn\b/.test(text) ? 'syn = { version = "2", features = ["full"] }' : "",
      ].filter(Boolean);
      fs.writeFileSync(
        path.join(ws, c, "Cargo.toml"),
        `[package]\nname = "${c}"\nversion = "0.1.0"\nedition = "2024"\n[lib]\nproc-macro = true\n[dependencies]\n${crateDeps.join("\n")}\n`,
      );
      fs.copyFileSync(path.join(root, f), path.join(ws, c, "src", "lib.rs"));
    }
    fs.writeFileSync(path.join(ws, "Cargo.toml"), `[workspace]\nmembers = ${JSON.stringify(members)}\nresolver = "2"\n`);
    fs.writeFileSync(path.join(ws, "app", "Cargo.toml"), `[package]\nname = "app_${dir}"\nversion = "0.1.0"\nedition = "2024"\n[dependencies]\n${deps.join("\n")}\n`);
    fs.copyFileSync(path.join(root, "main.rs"), path.join(ws, "app", "src", "main.rs"));
    const r = spawnSync("cargo", ["run", "-q", "--offline", "-p", `app_${dir}`, "--target-dir", path.join(outDir, `procmacro-target-${dir}`)], {
      cwd: ws,
      encoding: "utf8",
    });
    // (101 is also cargo's exit code for a build error: only a panic message makes it a run.)
    if (r.status !== 0 && !(r.status === 101 && r.stderr.includes("panicked at"))) {
      throw new Error(`cargo failed:\n${r.stderr}`);
    }
    const nativeStdout = r.stdout.replace(/\r\n/g, "\n");
    if (js.stdout !== nativeStdout || js.code !== r.status) {
      fail(name, `cargo:\n${nativeStdout}\njs:\n${js.stdout}`);
    } else pass(name);
  } catch (e) {
    fail(name, e.message);
  }
}

// --- JS globals: test/js/*.rs use the built-in `js` crate (`Date`, `JSON`). Natively that's
// test/js/shim (serde_json, a UTC `Date`), in one cargo workspace with each test as a binary.
{
  const programs = fs.readdirSync(path.join(here, "js")).filter((f) => f.endsWith(".rs"));
  const ws = path.join(outDir, "js-ws");
  fs.mkdirSync(path.join(ws, "js", "src"), { recursive: true });
  fs.mkdirSync(path.join(ws, "app", "src", "bin"), { recursive: true });
  fs.writeFileSync(path.join(ws, "Cargo.toml"), `[workspace]\nmembers = ["js", "app"]\nresolver = "2"\n`);
  fs.writeFileSync(
    path.join(ws, "js", "Cargo.toml"),
    `[package]\nname = "js"\nversion = "0.1.0"\nedition = "2024"\n[dependencies]\nserde = "1"\nserde_json = "1"\n`,
  );
  fs.copyFileSync(path.join(here, "js", "shim", "lib.rs"), path.join(ws, "js", "src", "lib.rs"));
  fs.writeFileSync(
    path.join(ws, "app", "Cargo.toml"),
    `[package]\nname = "app"\nversion = "0.1.0"\nedition = "2024"\n[dependencies]\njs = { path = "../js" }\nserde = { version = "1", features = ["derive"] }\n`,
  );
  for (const f of programs) fs.copyFileSync(path.join(here, "js", f), path.join(ws, "app", "src", "bin", f));
  for (const f of programs) {
    const name = `js ${f}`;
    try {
      const out = compile(fs.readFileSync(path.join(here, "js", f), "utf8"), f);
      if (out.code === null) throw new Error(out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`).join("\n"));
      const jsPath = path.join(outDir, `js-${f.replace(/\.rs$/, ".mjs")}`);
      fs.writeFileSync(jsPath, out.code);
      const js = await runJs(jsPath);
      const r = spawnSync("cargo", ["run", "-q", "--offline", "--bin", path.basename(f, ".rs")], { cwd: ws, encoding: "utf8" });
      if (r.status !== 0 && !(r.status === 101 && r.stderr.includes("panicked at"))) {
        throw new Error(`cargo failed:\n${r.stderr}`);
      }
      const nativeStdout = r.stdout.replace(/\r\n/g, "\n");
      if (js.stdout !== nativeStdout || js.code !== r.status) {
        fail(name, `cargo:\n${nativeStdout}\njs:\n${js.stdout}`);
      } else pass(name);
    } catch (e) {
      fail(name, e.message);
    }
  }
}

// --- JS globals with no native counterpart (timers, URLs, crypto): test/js-node/*.rs under
// Node, against the expected output in the matching .out file.
for (const f of fs.readdirSync(path.join(here, "js-node")).filter((f) => f.endsWith(".rs"))) {
  const name = `js-node ${f}`;
  try {
    const out = compile(fs.readFileSync(path.join(here, "js-node", f), "utf8"), f);
    if (out.code === null) throw new Error(out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`).join("\n"));
    const jsPath = path.join(outDir, `js-node-${f.replace(/\.rs$/, ".mjs")}`);
    fs.writeFileSync(jsPath, out.code);
    const js = await runJs(jsPath);
    const want = fs.readFileSync(path.join(here, "js-node", f.replace(/\.rs$/, ".out")), "utf8").replace(/\r\n/g, "\n");
    if (js.stdout !== want || js.code !== 0) fail(name, `want:\n${want}\ngot (exit ${js.code}):\n${js.stdout}${js.panic ?? ""}`);
    else pass(name);
  } catch (e) {
    fail(name, e.message);
  }
}

// --- debug-build overflow checks: test/debug/*.rs with `setOverflowChecks(true)` against
// `rustc` without `-O`; stdout, exit code and the panic (message and location) must match.
{
  setOverflowChecks(true);
  for (const file of fs.readdirSync(path.join(here, "debug")).filter((f) => f.endsWith(".rs"))) {
    const name = `debug ${file}`;
    try {
      const src = fs.readFileSync(path.join(here, "debug", file), "utf8");
      const out = compile(src, file);
      if (out.code === null) throw new Error(out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`).join("\n"));
      const jsPath = path.join(outDir, `debug-${file.replace(/\.rs$/, ".mjs")}`);
      fs.writeFileSync(jsPath, out.code);
      const js = await runJs(jsPath);
      const exe = path.join(outDir, `debug-${path.basename(file, ".rs")}${process.platform === "win32" ? ".exe" : ""}`);
      buildNative(path.join(here, "debug", file), exe, { debug: true });
      const r = spawnSync(exe, { encoding: "utf8" });
      const nativeStdout = r.stdout.replace(/\r\n/g, "\n");
      const at = (s) => {
        if (!s) return undefined;
        const m = /panicked at [^\n]*?([^\\/\n]+\.rs:\d+:\d+):\r?\n([^\r\n]*)/.exec(s);
        return m ? m.slice(1) : ["", s.split("\n").at(-1).trim()];
      };
      // The location counts when it's in the program (rustc may report one inside std).
      const show = (p, other) => p && (p[0].startsWith(file) && other?.[0].startsWith(file) ? p.join(" ") : p[1]);
      const n = at(r.stderr);
      const j = at(js.panic);
      const nativePanic = show(n, j ?? n);
      const jsPanic = show(j, n);
      if (js.stdout !== nativeStdout || js.code !== r.status || nativePanic !== jsPanic) {
        fail(name, `rustc: ${nativeStdout}exit ${r.status} ${nativePanic}\njs:    ${js.stdout}exit ${js.code} ${jsPanic}`);
      } else pass(name);
    } catch (e) {
      fail(name, e.message);
    }
  }
  setOverflowChecks(false);
}

// --- NS scripts against a mock
function mockNs(log) {
  const state = { security: 10, minSecurity: 1, money: 100, maxMoney: 1000 };
  // A small network for crawling scripts: neighbors, port requirements, root access.
  const network = {
    home: ["n00dles", "foodnstuff", "darkweb"],
    n00dles: ["home", "zer0"],
    foodnstuff: ["home", "zer0", "max-hardware"],
    darkweb: ["home"],
    zer0: ["n00dles", "foodnstuff", "phantasy"],
    "max-hardware": ["foodnstuff"],
    phantasy: ["zer0"],
  };
  const ports = { zer0: 1, "max-hardware": 1, phantasy: 2 };
  const rooted = new Set(["home"]);
  return {
    scan: (host) => [...(network[host] ?? [])],
    getServerNumPortsRequired: (host) => ports[host] ?? 0,
    hasRootAccess: (host) => rooted.has(host),
    nuke(host) {
      log.push(`nuke ${host}`);
      rooted.add(host);
      return true;
    },
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
    async hack(t, opts) {
      log.push(opts ? `hack ${t} ${JSON.stringify(opts)}` : `hack ${t}`);
      return 1234.5678;
    },
    tprint: (s) => log.push(s),
    getHostname: () => "home",
    getScriptName: () => "deploy.js",
    getServer: (host) => ({
      hostname: host,
      maxRam: 32,
      ramUsed: 4.5,
      cpuCores: 2,
      moneyMax: host === "home" ? undefined : 1750000,
    }),
    hacknet: { numNodes: () => 3 },
    // (For typed_values.rs: arguments, option objects, intersections, tuples and maps.)
    args: ["n00dles", 4, "--fast"],
    exec(script, host, opts, ...args) {
      log.push(`exec ${script} ${host} ${JSON.stringify(opts)} ${JSON.stringify(args)}`);
      return 7;
    },
    run(script, opts) {
      log.push(`run ${script} ${JSON.stringify(opts)}`);
      return 8;
    },
    getPlayer: () => ({ skills: { hacking: 50 }, money: 1e6, karma: -3 }),
    formulas: {
      hacking: {
        hackPercent(server, person) {
          log.push(`hackPercent ${server.hostname} ${server.moneyAvailable} ${server.hackDifficulty} ${person.skills.hacking}`);
          return 0.0125;
        },
      },
    },
    dnet: {
      authenticate: async (host, pw) => ({ success: false, code: 401, message: "denied", data: "7" }),
      heartbleed: async (host, opts) => {
        log.push(`heartbleed ${host} ${JSON.stringify(opts)}`);
        return { success: true, code: 200, message: "ok", logs: ["a", "b"] };
      },
      getServerDetails: (host) => ({ modelId: "ZeroLogon", isOnline: true, data: "" }),
    },
    stock: { getPosition: () => [10, 1.5, 0, 0] },
    getResetInfo: () => ({ ownedSF: new Map([[1, 3], [8, 2]]) }),
    go: { makeMove: async (x, y) => ({ type: "move", x: 3, y: null }) },
    // The TS unions: an array of tagged requirements, a tagged task, a `Result` by `success`.
    singularity: {
      getFactionInviteRequirements: (faction) =>
        faction === "CyberSec"
          ? [{ type: "money", money: 1e6 }, { type: "skills", skills: { hacking: 50 } }, { type: "not", condition: { type: "employedBy", company: "FulcrumTech" } }]
          : [],
      getCurrentWork: () => ({ type: "CLASS", cyclesWorked: 3, classType: "Computer Science", location: "Rothman University", nextCompletion: Promise.resolve() }),
    },
    sleeve: {
      getTask: (n) => (n === 0 ? { type: "CRIME", crimeType: "Shoplift", cyclesWorked: 1, cyclesNeeded: 5, tasksCompleted: 0, nextCompletion: Promise.resolve() } : null),
      purchaseSleeve: () => ({ success: false, message: "You cannot afford a sleeve" }),
      upgradeMemory: (n, amount) => ({ success: true }),
    },
    prompt: async (txt, options) => (options?.type === "text" ? "typed" : true),
    kill: (filename, host, ...args) => {
      log.push(`kill ${JSON.stringify(filename)} ${host} ${JSON.stringify(args)}`);
      return true;
    },
    codingcontract: {
      getData: () => [[1, 3], [2, 6]],
      attempt(answer, file, host) {
        log.push(`attempt ${JSON.stringify(answer)} ${file} ${host}`);
        return "reward";
      },
      getContract: (file) =>
        file === "a.cct"
          ? { type: "Find Largest Prime Factor", data: 91, description: "Find it.", difficulty: 1, numTriesRemaining: () => 9, submit: (answer) => (answer === 13 ? "$1000" : "") }
          : { type: "Merge Overlapping Intervals", data: [[1, 3], [2, 6]], description: "Merge.", difficulty: 2.5, numTriesRemaining: () => 4, submit: (answer) => `got ${JSON.stringify(answer)}` },
    },
  };
}
for (const f of fs.readdirSync(path.join(here, "ns")).filter((f) => f.endsWith(".rs"))) {
  const name = `ns ${f}`;
  try {
    const { out, jsPath } = compileFixture(path.join(here, "ns", f));
    const log = [];
    const ns = mockNs(log);
    // (clap reads the script's arguments.)
    if (f === "clap_args.rs") ns.args = ["--target", "n00dles", "-t", 4, "--dry-run"];
    await runJs(jsPath, [ns]);
    const expected = fs
      .readFileSync(path.join(here, "ns", f.replace(/\.rs$/, ".expected")), "utf8")
      .replace(/\r\n/g, "\n");
    if (log.join("\n") + "\n" !== expected) {
      fail(name, `expected:\n${expected}\ngot:\n${log.join("\n")}`);
      continue;
    }
    // RAM calculation needs literal camelCase member calls on `ns`.
    const literalCalls = {
      "hack.rs": ["ns.getServerMaxMoney(", "ns.weaken(", "ns.hack(", "await ns.grow("],
      "crawl.rs": ["ns.scan(", "ns.getServerNumPortsRequired(", "ns.hasRootAccess(", "ns.nuke("],
      "typed.rs": ["ns.getServer(", "ns.hacknet.numNodes(", "ns.getHostname("],
    };
    const missing = (literalCalls[f] ?? []).filter((s) => !out.code.includes(s));
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
      ["return BigInt.asUintN(64, fib(", "fib(n - 1) + fib(n - 2)"],
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
    const gl = genLines.findIndex((l) => l.includes("return BigInt.asUintN(64, fib("));
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

// --- diagnostics: programs rustc rejects, and the error each must produce (line:col prefix).
// The `fn main` wrapper is added; bodies start on line 2.
const rejected = [
  ["assign to immutable", "let x = 1;\n    x = 2;", "3:5 cannot assign twice to immutable variable `x`"],
  ["unsupported construct", "let v = vec![1];\n    let r = v.as_ptr();", "3:13 no method named `as_ptr`"],
  ["an unknown type", "let x: Foo = 1;", "2:12 cannot find type `Foo` in this scope"],
  ["a misspelled type", "}\nstruct Stack { n: i32 }\nfn f() {\n    let s: Stak = Stack { n: 1 };", "5:12 cannot find type `Stak` in this scope (a struct with a similar name exists: `Stack`)"],
  ["a misspelled std type", "let s: Strng = String::new();", "2:12 cannot find type `Strng` in this scope (a type with a similar name exists: `String`)"],
  ["a std function the compiler doesn't model", "let c = std::cell::UnsafeCell::new(1);", "2:13 the standard library function `std::cell::UnsafeCell::new` is not supported yet"],
  ["a std type the compiler doesn't model, imported by name", "}\nuse std::cell::UnsafeCell;\nfn f() {\n    let c: UnsafeCell<i32> = UnsafeCell::new(1);", "5:12 the standard library type `UnsafeCell<i32>` is not supported yet"],
  ["a constructor of a misspelled type", "}\nstruct Stack { n: i32 }\nfn f() {\n    let s = Stak::make();", "5:13 failed to resolve: use of undeclared type `Stak` (a struct with a similar name exists: `Stack`)"],
  ["a missing associated function", "}\nstruct Stack { n: i32 }\nfn f() {\n    let s = Stack::make();", "5:13 no function or associated item named `make` found for struct `Stack` in the current scope"],
  [
    "element type inferred from a later push",
    "let mut v = Vec::new();\n    v.push(1u8);\n    v.push(\"x\");",
    "4:12 mismatched types: expected `u8`, found `String`",
  ],
  ["mutating an immutable Vec", "let v = vec![1];\n    v.push(2);", "3:5 cannot borrow `v` as mutable, as it is not declared as mutable (declare it with `let mut v`)", "Variables are read-only unless declared `let mut`."],
  ["`&mut` of an immutable binding", "let v = vec![1];\n    let r = &mut v;", "3:18 cannot borrow `v` as mutable"],
  ["indexing with i32", "let v = vec![1];\n    let i: i32 = 0;\n    let x = v[i];", "4:15 the type `[{integer}]` cannot be indexed by `i32`"],
  ["Display of a Vec", "let v = vec![1];\n    println!(\"{}\", v);", "3:14 `Vec<{integer}>` doesn't implement `std::fmt::Display`"],
  ["integer literal as f64", "let x: f64 = 1;", "2:18 mismatched types: expected `f64`, found `{integer}`", "Rust never converts between number types for you"],
  ["incompatible if/else", "let x = if true { 1 } else { \"no\" };", "2:32 `if` and `else` have incompatible types: expected `{integer}`, found `String`"],
  ["sorting floats", "let mut v = vec![1.5, 0.5];\n    v.sort();", "3:5 the trait bound `{float}: Ord` is not satisfied"],
  ["ambiguous numeric method", "let x = 2.pow(3);", "2:13 can't call method `pow` on ambiguous numeric type `{integer}`"],
  [
    "non-exhaustive Option match",
    "let x: Option<i32> = None;\n    match x {\n        Some(v) => {}\n    }",
    "3:11 non-exhaustive patterns: `None` not covered",
  ],
  ["non-exhaustive integer match", "match 5 {\n        1 => {}\n    }", "2:11 non-exhaustive patterns: `i32::MIN..=0_i32`"],
  [
    "guarded arms don't count toward exhaustiveness",
    "match true {\n        true if 1 > 2 => {}\n        false => {}\n    }",
    "2:11 non-exhaustive patterns: `true` not covered",
  ],
  ["refutable let", "let Some(x) = Some(1);", "2:9 refutable pattern in local binding"],
  [
    "let-else that falls through",
    'let Some(x) = Some(1) else {\n        println!("no");\n    };',
    "2:32 `else` clause of `let...else` does not diverge",
  ],
  ["Display of an Option", 'println!("{}", Some(1));', "2:14 `Option<{integer}>` doesn't implement `std::fmt::Display`"],
  [
    "Some pattern on an integer",
    "match 5 {\n        Some(x) => {}\n        _ => {}\n    }",
    "3:9 mismatched types: expected `{integer}`, found `Option<_>`",
  ],
  ["incompatible match arms", 'let x = match 1 {\n        1 => 2,\n        _ => "s",\n    };', "4:14 `match` arms have incompatible types"],
  ["floats in a HashSet", "let mut s = std::collections::HashSet::new();\n    s.insert(1.5);", "the trait bound `f64: Hash` is not satisfied"],
  ["tuple field out of range", "let t = (1, 2);\n    let x = t.2;", "3:13 no field `2` on type `({integer}, {integer})`"],
  [
    "non-exhaustive tuple match names the missing case",
    "let t = (true, Some(1));\n    match t {\n        (true, _) => {}\n        (false, None) => {}\n    }",
    "3:11 non-exhaustive patterns: `(false, Some(_))` not covered",
  ],
  ["Display of a tuple", 'println!("{}", (1, 2));', "2:14 `({integer}, {integer})` doesn't implement `std::fmt::Display`"],
  ["tuple pattern arity", "let (a, b, c) = (1, 2);", "2:9 mismatched types: expected a tuple with 2 elements, found one with 3"],
  ["assigning a field of an immutable tuple", "let t = (1, 2);\n    t.0 = 5;", "3:5 cannot borrow `t` as mutable"],
  ["ordering sets", "let a = std::collections::HashSet::from([1]);\n    let b = a.clone();\n    let x = a < b;", "4:13 `HashSet<{integer}>` can't be compared"],
  ["float HashMap keys", "let mut m = std::collections::HashMap::new();\n    m.insert(0.5, 1);", "the trait bound `f64: Hash` is not satisfied"],
  [
    "assigning through a HashMap index",
    'let mut m = std::collections::HashMap::new();\n    m.insert("a", 1);\n    m["a"] = 2;',
    "4:5 cannot assign to data in an index of a `HashMap`",
  ],
  ["Display of a HashMap", 'let m: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();\n    println!("{}", m);', "3:14 `HashMap<i32, i32>` doesn't implement `std::fmt::Display`"],
  [
    "or_insert_with returning the wrong type",
    'let mut m: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();\n    m.entry(1).or_insert_with(|| "five");',
    "3:34 mismatched types: expected `i32`, found `String`",
  ],
  [
    "mutating an immutable HashMap",
    "let m: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();\n    m.insert(1, 2);",
    "3:5 cannot borrow `m` as mutable, as it is not declared as mutable",
  ],
  ["closure argument type", 'let f = |x: i32| x + 1;\n    f("a");', "3:7 mismatched types: expected `i32`, found `String`"],
  ["closure arity", "let f = |x: i32| x + 1;\n    f(1, 2);", "3:5 this closure takes 1 argument but 2 were supplied"],
  ["calling a non-function", "let x = 5;\n    x(1);", "3:5 expected function, found `{integer}`"],
  ["closure returning the wrong type", "let v = vec![1, 2];\n    v.iter().filter(|x| 1);", "3:25 mismatched types: expected `bool`, found `{integer}`"],
  ["sort_by without an Ordering", "let mut v = vec![1, 2];\n    v.sort_by(|a, b| a < b);", "3:22 mismatched types: expected `Ordering`, found `bool`"],
  ["max of floats", "let v = vec![1.5, 2.0];\n    let m = v.iter().max();", "3:13 the trait bound `{float}: Ord` is not satisfied"],
  ["collect without a target", "let v = vec![1, 2];\n    let w = v.iter().map(|x| x * 2).collect();", "3:13 type annotations needed"],
  ["Display of an Ordering", 'println!("{}", 1.cmp(&2));', "2:14 `Ordering` doesn't implement `std::fmt::Display`"],
  ["missing struct field", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let p = P { x: 1 };", "9:13 missing field `y` in initializer of `P`"],
  ["unknown struct field", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let p = P { x: 1, y: 2, z: 3 };", "9:29 struct `P` has no field named `z`"],
  ["non-exhaustive enum match names the variant", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let e = E::A;\n    match e { E::A => {}, E::C { .. } => {} }", "10:11 non-exhaustive patterns: `E::B(_)` not covered"],
  ["unknown method on a struct", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let p = P::new();\n    p.len();", "10:5 no method named `len` found for struct `P` in the current scope"],
  ["== without PartialEq", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let a = N { v: 1.0 };\n    let c = a == N { v: 2.0 };", "10:13 the trait bound `N: PartialEq` is not satisfied"],
  ["Display of a struct", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    println!(\"{}\", P::new());", "9:14 `P` doesn't implement `std::fmt::Display`"],
  ["Debug without the derive", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    println!(\"{:?}\", N { v: 1.0 });", "9:14 `N` doesn't implement `std::fmt::Debug`"],
  ["assigning a field of an immutable struct", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let p = P::new();\n    p.x = 5;", "10:5 cannot borrow `p` as mutable, as it is not declared as mutable"],
  ["unknown variant", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let e = E::D;", "9:13 no variant or associated item named `D` found for enum `E`"],
  ["associated function called as a method", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let p = P::new();\n    p.new();", "(`new` is an associated function: call `P::new(..)`)"],
  ["HashSet of a struct without Hash", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let mut s = std::collections::HashSet::new();\n    s.insert(N { v: 1.0 });", "the trait bound `N: Hash` is not satisfied"],
  ["field access on an enum", "}\n#[derive(Debug, Clone, Copy, PartialEq)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { v: i32 } }\nstruct N { v: f64 }\nimpl P { fn new() -> P { P { x: 0, y: 0 } } }\nfn f() {\n    let e = E::B(1);\n    let v = e.v;", "10:13 no field `v` on type `E`"],
  ["Copy without Clone", "}\n#[derive(Copy)]\nstruct Q { a: i32 }\nfn f() {", "the trait bound `Q: Clone` is not satisfied"],
  ["Eq on a struct with an f64 field", "}\n#[derive(PartialEq, Eq)]\nstruct Q { a: f64 }\nfn f() {", "the trait `Eq` is not implemented for `f64` (field `a` of `Q`)"],
  ["mutating through &self", "}\nstruct Q { a: i32 }\nimpl Q { fn f(&self) { self.a = 1; } }\nfn f() {", "4:24 cannot borrow `*self` as mutable, as it is behind a `&` reference"],
  ["&mut self method on an immutable binding", "}\nstruct Q { a: i32 }\nimpl Q { fn f(&mut self) { self.a = 1; } }\nfn g() {\n    let q = Q { a: 0 };\n    q.f();", "7:5 cannot borrow `q` as mutable, as it is not declared as mutable"],
  ["trait impls", "}\nstruct Q;\nimpl std::ops::Fn<()> for Q {}\nfn f() {", "4:6 `impl Fn for ..` is not supported yet"],
  ["peek on an iterator not declared mut", "let v = vec![1, 2];\n    let it = v.iter().peekable();\n    let p = it.peek();", "4:13 cannot borrow `it` as mutable"],
  ["try_fold with a closure returning a plain value", "let s = (1..4).try_fold(0, |a, x| a + x);", "2:13 `try_fold` needs a closure returning `Result<{integer}, E>` or `Option<{integer}>`"],
  ["an or-pattern binding a name in one alternative only","let p = (Some(1), 2);\n    match p {\n        (Some(x), _) | (None, _) => {}\n    }", "4:24 variable `x` is not bound in all patterns"],
  ["an or-pattern binding a name at two types", "let r: Result<i32, String> = Ok(1);\n    match r {\n        Ok(x) | Err(x) => {}\n    }", "4:17 mismatched types: `x` is `i32` in one alternative and `String` in another"],
  ["a struct builder method on a list builder", "}\nstruct P;\nimpl std::fmt::Debug for P {\n    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {\n        f.debug_list().field(\"a\", &1).finish()\n    }\n}\nfn g() {", "6:9 no method named `field` found for struct `DebugList` in the current scope"],
  ["a builder value without Debug", "}\nstruct Q;\nstruct P {\n    q: Q,\n}\nimpl std::fmt::Debug for P {\n    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {\n        f.debug_struct(\"P\").field(\"q\", &self.q).finish()\n    }\n}\nfn g() {", "9:40 `Q` doesn't implement `std::fmt::Debug`"],
  ["cfg! with a malformed predicate", "let w = cfg!(not(unix, windows));", "2:13 `not(..)` takes one predicate"],
  ["enum discriminant that isn't constant", "}\nfn f() -> isize { 1 }\nenum E { A = f() }\nfn g() {", "4:14 enum discriminants must be constant integer expressions"],
  ["a required trait const next to a default one", "}\ntrait T { const N: usize = 1; const M: usize; }\nstruct S;\nimpl T for S {}\nfn g() {", "5:12 not all trait items implemented, missing: `M`"],
  ["a trait const default of the wrong type", "}\ntrait T { const N: usize = \"x\"; }\nfn g() {", "3:28 mismatched types: expected `usize`, found `String`"],
  ["get_or_insert on an immutable binding", "let o: Option<i32> = None;\n    *o.get_or_insert(1) += 1;", "3:6 cannot borrow `o` as mutable, as it is not declared as mutable"],
  ["str and String impls that disagree", "}\ntrait S {\n    fn s(&self) -> String;\n}\nimpl S for str {\n    fn s(&self) -> String {\n        self.to_uppercase()\n    }\n}\nimpl S for String {\n    fn s(&self) -> String {\n        format!(\"{}!\", self)\n    }\n}\nfn g() {", "11:6 `S` is implemented for both `str` and `String`, which are one type in JS"],
  ["slice and Vec impls that disagree", "}\ntrait S {\n    fn s(&self) -> usize;\n}\nimpl S for [u8] {\n    fn s(&self) -> usize {\n        self.len()\n    }\n}\nimpl S for Vec<u8> {\n    fn s(&self) -> usize {\n        self.len() + 1\n    }\n}\nfn g() {", "11:6 `S` is implemented for both `[T]` and `Vec<T>`, which are one type in JS"],
  ["a tuple and a Vec implementing one trait", "}\ntrait S {\n    fn s(&self) -> usize;\n}\nimpl S for (u8, u8) {\n    fn s(&self) -> usize {\n        2\n    }\n}\nimpl S for Vec<u8> {\n    fn s(&self) -> usize {\n        self.len()\n    }\n}\nfn g() {", "11:12 `S` for both a tuple and a slice, array or `Vec` isn't supported yet"],
  ["an impl of a trait that isn't declared or loaded, on a built-in type", "}\nimpl Format for u8 {\n    fn format(&self) {}\n}\nfn g() {", "3:6 `impl Format for ..` is not supported yet"],
  ["TryFrom twice, neither taking the argument", "}\nstruct E(i32);\nimpl TryFrom<i32> for E {\n    type Error = ();\n    fn try_from(v: i32) -> Result<Self, ()> {\n        Ok(E(v))\n    }\n}\nimpl TryFrom<bool> for E {\n    type Error = ();\n    fn try_from(v: bool) -> Result<Self, ()> {\n        Ok(E(v as i32))\n    }\n}\nfn g() {\n    let _ = E::try_from(\"x\");", "17:25 mismatched types"],
  ["try_into without a target", "let x = 5i64.try_into();", "2:13 type annotations needed: say what `.try_into()` converts to"],
  ["hash of a float", "}\nuse std::hash::{Hash, Hasher};\nstruct P(f64);\nimpl Hash for P {\n    fn hash<H: Hasher>(&self, state: &mut H) {\n        self.0.hash(state);\n    }\n}\nfn g() {", "7:9 the trait bound `f64: Hash` is not satisfied"],
  ["`<` on a type parameter without PartialOrd", "}\nfn big<T>(a: T, b: T) -> bool {\n    a > b", "4:5 comparing with `<` on type parameter `T` needs a bound: add `T: PartialOrd`"],
  ["generic call with an unsatisfied bound", "}\nfn show<T: std::fmt::Display>(x: T) {}\nfn f() {\n    show(vec![1]);", "5:5 the trait bound `Vec<i32>: Display` is not satisfied"],
  ["division on a type parameter without Div", "}\nfn half<T: Copy>(x: T, y: T) -> T {\n    x / y", "4:5 `/` on type parameter `T` needs a bound: add `T: Div` to the function's generics"],
  ["T::from with the wrong argument type", "}\nfn w<T: From<u8>>(s: String) -> T {\n    T::from(s)", "4:13 mismatched types: expected `u8`, found `String`"],
  ["shift on a type parameter without Shl", "}\nfn up<T: Copy>(x: T) -> T {\n    x << 1", "4:5 `<<` on type parameter `T` needs a bound: add `T: Shl` to the function's generics"],
  ["! on a type parameter without Not", "}\nfn inv<T: Copy>(x: T) -> T {\n    !x", "4:5 `!` on type parameter `T` needs a bound: add `T: Not` to the function's generics"],
  ["shift of a type parameter by a string", "}\nfn up<T: std::ops::Shl<Output = T>>(x: T) -> T {\n    x << \"a\"", "4:10 no implementation for `T << String`"],
  ["a float where a bitwise bound is needed", "}\nfn and<T: std::ops::BitAnd<Output = T>>(x: T, y: T) -> T {\n    x & y\n}\nfn g() {\n    and(1.5, 2.0);", "7:5 the trait bound `f64: BitAnd` is not satisfied"],
  ["an empty Vec nothing types", "let v = Vec::new();\n    println!(\"hi\");", "2:9 type annotations needed for `Vec<_>`"],
  ["a HashMap nothing types", "let m = std::collections::HashMap::new();\n    println!(\"{}\", m.len());", "2:9 type annotations needed for `HashMap<_, _>`"],
  ["a None nothing types", "let x = None;\n    println!(\"{}\", x.is_some());", "2:9 type annotations needed for `Option<_>`"],
  ["a generic struct nothing types", "}\nstruct Stack<T> { items: Vec<T> }\nimpl<T> Stack<T> { fn new() -> Self { Stack { items: Vec::new() } } }\nfn g() {\n    let s = Stack::new();\n    println!(\"{}\", s.items.len());", "6:9 type annotations needed for `Stack<_>`"],
  ["an Itertools method without the import", "let s = (1..3).map(|x| x * 2).join(\",\");", "2:13 no method named `join` found for iterator"],
  ["gen_range of a range in a variable", "}\nuse rand::Rng;\nfn g() {\n    let mut rng = rand::thread_rng();\n    let r = 0..5;\n    let x = rng.gen_range(r);", "7:13 `gen_range` takes a range written out here (`lo..hi` or `lo..=hi`)"],
  ["an RNG that isn't mut", "}\nuse rand::Rng;\nfn g() {\n    let rng = rand::thread_rng();\n    let x: u8 = rng.gen_range(0..5);", "6:17 cannot borrow `rng` as mutable, as it is not declared as mutable"],
  ["sorted() on floats", "}\nuse itertools::Itertools;\nfn g() {\n    let v = vec![1.5, 0.5];\n    let s: Vec<f64> = v.into_iter().sorted().collect();", "6:23 the trait bound `{float}: Ord` is not satisfied"],
  [".context() without anyhow", "let r: Result<i32, String> = Ok(1);\n    let x = r.context(\"x\");", "3:13 no method named `context` found for `Result<i32, String>` (if you meant `anyhow::Context`'s, add `use anyhow::Context;`)"],
  ["a thiserror message naming no field", "}\nuse thiserror::Error;\n#[derive(Error, Debug)]\nenum E {\n    #[error(\"bad {nope}\")]\n    A { x: i32 },\n}\nfn g() {", "6:5 there is no field `nope` here"],
  ["an unknown bitflags flag", "}\nbitflags::bitflags! {\n    struct F: u8 {\n        const A = 1;\n    }\n}\nfn g() {\n    let f = F::B;", "9:13 no function or associated item named `B` found for struct `F`"],
  ["serde_json::from_str with nothing to parse into", "let v = serde_json::from_str(\"1\");", "2:13 type annotations needed: say what to parse into, e.g. `serde_json::from_str::<Config>(..)`"],
  ["serde_json::to_string of a closure", "let f = |x: i32| x;\n    let s = serde_json::to_string(&f);", "3:35 the trait bound"],
  ["an extern crate that isn't bundled", "}\nextern crate reqwest;\nfn g() {", "3:1 unresolved import `reqwest`"],
  ["a regex::Error of the wrong kind", "}\nfn g() -> Result<(), regex::Error> {\n    let x: i32 = \"a\".parse()?;\n    Ok(())", "4:18 `?` couldn't convert the error to `RegexError`"],
  ["parse()? into an error without From, beside a FromStr impl", "}\nstruct E;\nstruct P;\nimpl std::str::FromStr for P {\n    type Err = E;\n    fn from_str(s: &str) -> Result<P, E> { Ok(P) }\n}\nfn g() -> Result<(), E> {\n    let x: i32 = \"a\".parse()?;\n    Ok(())", "10:18 `?` couldn't convert the error to `E` (add `impl From<ParseIntError> for E`"],
  ["the start field of an inclusive range", "let r = 1..=3;\n    let s = r.start;", "3:13 no field `start` on type `RangeInclusive<"],
  ["a value used after drop()", "}\nstruct G { n: i32 }\nimpl Drop for G { fn drop(&mut self) {} }\nfn g() {\n    let a = G { n: 1 };\n    drop(a);\n    let b = a;", "8:13 use of moved value: `a`"],
  ["await on an async block outside async", "let f = async { 1 };\n    let x = f.await;", "3:13 `await` is only allowed inside `async` functions and blocks"],
  ["await on an async closure's call outside async", "let f = async |x: i32| x;\n    let y = f(1).await;", "3:13 `await` is only allowed inside `async` functions and blocks"],
  ["an async closure's call not awaited", "}\nasync fn g() {\n    let f = async |x: i32| x;\n    let y: i32 = f(1);", "5:18 mismatched types: expected `i32`, found `impl Future<Output = i32>` (did you forget `.await`?)"],
  ["Default::default() that nothing types", "let x = Default::default();", "2:13 type annotations needed for `Default::default()`"],
  ["a break Default::default() typed by a later break without Default", "}\nstruct S;\nfn g() {\n    let z = 'l: loop {\n        if true {\n            break 'l Default::default();\n        }\n        break 'l S;\n    };", "7:22 the trait bound `S: Default` is not satisfied"],
  ["a JoinHandle of the wrong type", "}\nasync fn g() {\n    let h: tokio::task::JoinHandle<String> = tokio::spawn(async { 1u8 });", "4:46 mismatched types"],
  ["bail! in a function that doesn't return a Result", "}\nuse anyhow::bail;\nfn g() -> i32 {\n    bail!(\"no\");", "5:5 mismatched types"],
  ["parse into a type parameter without FromStr", "}\nfn p<T: Copy>(s: &str) -> T {\n    s.parse::<T>().unwrap()", "4:5 the trait bound `T: FromStr` is not satisfied"],
  ["a user type without the operator a bound needs","}\n#[derive(Clone, Copy)]\nstruct M(i32);\nfn dbl<T: std::ops::Add<Output = T> + Copy>(x: T) -> T {\n    x + x\n}\nfn g() {\n    let m = dbl(M(1));", "9:13 the trait bound `M: Add` is not satisfied"],
  ["unsupported trait bound", "}\nfn f<T: Read>(x: T) {", "3:9 the trait bound `Read` is not supported yet on type parameters"],
  ["T::try_from without a TryFrom bound", "}\nfn p<T: From<u8>>(x: u8) -> T {\n    T::try_from(x).ok().unwrap()", "4:5 no function or associated item named `try_from` found for type parameter `T`"],
  ["{:x?} on a struct", "}\n#[derive(Debug)]\nstruct S { a: u8 }\nfn g() {\n    println!(\"{:x?}\", S { a: 1 });", "6:14 `{:x?}` is supported on numbers, strings and collections"],
  ["{:x?} with a width argument", "let w = 4;\n    println!(\"{:w$x?}\", vec![1u8]);", "3:14 `{:x?}` is supported on numbers"],
  ["method outside a type parameter's bounds", "}\nfn f<T>(x: T) -> usize {\n    x.len()", "4:5 no method named `len` found for type parameter `T`"],
  ["mutable string slice", "let mut s = String::from(\"abc\");\n    let t = &mut s[1..];", "3:13 mutable string slices (`&mut s[a..b]`) are not supported"],
  ["trait impl missing a method", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nimpl T for S {}\nfn f() {", "5:12 not all trait items implemented, missing: `a`"],
  ["method that isn't in the trait", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nimpl T for S { fn a(&self) -> i32 { 0 } fn z(&self) {} }\nfn f() {", "5:44 method `z` is not a member of trait `T`"],
  ["generic call with an unimplemented user trait", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nfn g<X: T>(x: X) -> i32 { x.a() }\nfn f() {\n    g(5);", "7:5 the trait bound `i32: T` is not satisfied"],
  ["unknown method on dyn Trait", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nimpl T for S { fn a(&self) -> i32 { 0 } }\nfn f() {\n    let d: Box<dyn T> = Box::new(S);\n    d.c();", "8:5 no method named `c` found for `dyn T`"],
  ["Box<dyn Trait> of a type without the impl", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nfn f() {\n    let d: Box<dyn T> = Box::new(S);", "6:34 the trait bound `S: T` is not satisfied"],
  ["calling a function the bound doesn't have", "}\ntrait M { fn make() -> Self; }\nfn g<T: M>() -> T {\n    T::build()\n}\nfn f() {", "5:5 no function or associated item named `build` found for type parameter `T`"],
  ["non-exhaustive slice match names the missing length", "let v = vec![1];\n    match v.as_slice() { [] => {}, [x] => {} }", "3:11 non-exhaustive patterns: `[_, _, ..]` not covered"],
  ["non-exhaustive match on an array names the missing value", "match [true, false] { [true, _] => {} }", "2:11 non-exhaustive patterns: `[false, true]` not covered"],
  ["array pattern of the wrong length", "match [1, 2, 3] { [a, b] => {} }", "2:11 non-exhaustive patterns: `[_, _, _]` not covered"],
  ["refutable array pattern in let", "let [a, 1] = [1, 2];", "2:9 refutable pattern in local binding"],
  ["refutable nested array pattern in for", "for [k, 0] in [[1, 2], [3, 4]] {}", "2:9 refutable pattern in `for` loop binding"],
  ["associated type bound not met", "}\nfn f<I: IntoIterator<Item: std::fmt::Display>>(i: I) {}\nfn g() {\n    f(vec![vec![1]]);", "5:5 the trait bound `Vec<i32>: Display` is not satisfied"],
  ["associated type bound in an argument not met", "}\nfn f(i: impl Iterator<Item: std::fmt::Display>) {}\nfn g() {\n    f(vec![vec![1]].into_iter());", "5:5 the trait bound `Vec<i32>: Display` is not satisfied"],
  ["break with a value from a while loop", "while true { break 5; }", "2:18 `break` with value from a `while` or `for` loop"],
  ["break outside a loop", "break;", "2:5 `break` outside of a loop"],
  ["loop breaking with different types", "let x = loop { if true { break 1; } break \"s\"; };", "2:47 mismatched types: expected `{integer}`, found `String`"],
  ["`&mut` to a number field of a call's result", "}\nstruct P { x: i32 }\nfn make() -> P { P { x: 1 } }\nfn bump(v: &mut i32) { *v += 1; }\nfn f() {\n    bump(&mut make().x);", "7:15 passing a `&mut` to a number, string or bool inside a struct, tuple or Vec isn't supported yet"],
  ["wrong type into a Stack<i32>", "}\nstruct Stack<T> { items: Vec<T> }\nimpl<T> Stack<T> { fn new() -> Self { Stack { items: Vec::new() } } fn push(&mut self, x: T) { self.items.push(x); } }\nfn f() {\n    let mut s: Stack<i32> = Stack::new();\n    s.push(\"x\");", "7:12 mismatched types: expected `i32`, found `String`"],
  ["a generic struct's type inferred from first use", "}\nstruct Stack<T> { items: Vec<T> }\nimpl<T> Stack<T> { fn new() -> Self { Stack { items: Vec::new() } } fn push(&mut self, x: T) { self.items.push(x); } }\nfn f() {\n    let mut s = Stack::new();\n    s.push(1);\n    s.push(2.5);", "8:12 mismatched types: expected `{integer}`, found `{float}`"],
  ["use after `let b = a`", "let a = vec![1];\n    let b = a;\n    println!(\"{:?} {:?}\", a, b);", "4:27 use of moved value: `a` (moved on line 3:13 by `let`"],
  ["use after passing to a function", "}\nfn take(v: Vec<i32>) -> usize { v.len() }\nfn f() {\n    let v = vec![1];\n    take(v);\n    v.len();", "7:5 use of moved value: `v` (moved on line 6:10 as an argument"],
  // An owned value where the parameter is `&T`: rustc's E0308 at the argument, with the borrow
  // suggestion (only where the value certainly owns itself: a `let` of a made value, an owned
  // parameter, an NS result).
  ["an owned String for a `&str` parameter", "}\nfn take(s: &str) -> usize { s.len() }\nfn f() {\n    let t = String::from(\"abc\");\n    take(t);", "6:10 mismatched types: expected `&str`, found `String` (consider borrowing here: `&t`)"],
  ["a Vec for a `&[T]` parameter", "}\nfn total(v: &[i32]) -> i32 { v.iter().sum() }\nfn f() {\n    let v = vec![1];\n    total(v);", "6:11 mismatched types: expected `&[i32]`, found `Vec<i32>` (consider borrowing here: `&v`)"],
  ["a struct for a method's `&Self` parameter", "}\nstruct P { x: i32 }\nimpl P {\n    fn far(&self, o: &P) -> i32 { (self.x - o.x).abs() }\n}\nfn f() {\n    let p = P { x: 1 };\n    let q = P { x: 2 };\n    p.far(q);", "10:11 mismatched types: expected `&P`, found `P` (consider borrowing here: `&q`)"],
  ["a made value for a `&str` parameter", "}\nfn take(s: &str) -> usize { s.len() }\nfn f(n: i32) {\n    take(format!(\"{}\", n));", "5:10 mismatched types: expected `&str`, found `String` (consider borrowing here: `&format!(\"{}\", n)`)"],
  ["use after a `for` loop", "let v = vec![1, 2];\n    for x in v { println!(\"{x}\"); }\n    v.len();", "4:5 use of moved value: `v` (moved on line 3:14 by a `for` loop"],
  ["use after `into_iter()`", "let v = vec![1];\n    let it = v.into_iter();\n    v.len();", "4:5 use of moved value: `v` (moved on line 3:14 by a method that takes `self`"],
  ["use after a partial move out of a tuple", "let p = (1, vec![1]);\n    let (_, v) = p;\n    println!(\"{:?} {}\", v, p.0);\n    println!(\"{:?}\", p);", "5:22 use of partially moved value: `p` (a field moved on line 3:18 by `let`"],
  ["use after a partial move out of a struct", "}\nstruct P { x: i32, v: Vec<i32> }\nfn f() {\n    let p = P { x: 1, v: vec![1] };\n    let P { v, .. } = p;\n    println!(\"{:?} {}\", v, p.x);\n    let q = p;", "8:13 use of partially moved value: `p`"],
  ["a moved field read after a partial move", "}\nstruct P { x: i32, v: Vec<i32> }\nfn f() {\n    let p = P { x: 1, v: vec![1] };\n    let P { v, .. } = p;\n    let w = p.v;", "7:13 use of partially moved value: `p`"],
  ["overlapping inherent impls", "}\nstruct W<T>(T);\nimpl<T> W<T> { fn get(&self) -> i32 { 1 } }\nimpl W<i32> { fn get(&self) -> i32 { 2 } }\nfn f() {", "5:18 duplicate definitions with name `get`"],
  ["per-instantiation function named without type arguments", "}\nstruct W<T>(T);\nimpl W<i32> { fn kind() -> i32 { 1 } }\nimpl W<u8> { fn kind() -> i32 { 2 } }\nfn f() {\n    W::kind();", "7:5 multiple applicable items in scope: `kind` is defined in `impl`s for several instantiations of `W`"],
  ["per-instantiation method on an unsettled literal", "}\nstruct W<T>(T);\nimpl W<i32> { fn get(&self) -> i32 { 1 } }\nimpl W<u8> { fn get(&self) -> i32 { 2 } }\nfn f() {\n    W(2).get();", "7:5 multiple applicable items in scope: `get`"],
  ["associated const pattern of the wrong type", "}\nstruct S;\nimpl S { const N: u8 = 1; }\nfn f() {\n    match \"a\" { S::N => {}, _ => {} }", "6:17 mismatched types: expected `String`, found `u8`"],
  ["range up to an associated const, not exhaustive", "}\nstruct B;\nimpl B { const MID: u8 = 127; }\nfn f() {\n    match 3u8 { 0..=B::MID => {} }", "6:11 non-exhaustive patterns: `128_u8..=u8::MAX` not covered"],
  ["a move in a previous loop iteration", "}\nfn take(s: String) {}\nfn f() {\n    let s = String::from(\"a\");\n    for _ in 0..3 {\n        take(s);\n    }", "7:14 use of moved value: `s` (moved here as an argument in the loop's previous iteration"],
  ["use after a `move` closure", "let v = vec![1];\n    let f = move || v.len();\n    v.len();", "4:5 use of moved value: `v` (moved on line 3:13 into a `move` closure"],
  ["use after a `self` method", "}\nstruct P { x: i32 }\nimpl P { fn eat(self) -> i32 { self.x } }\nfn f() {\n    let p = P { x: 1 };\n    p.eat();\n    p.x;", "8:5 use of moved value: `p` (moved on line 7:5 by a method that takes `self`"],
  ["use after `String + &str`", "let s = String::from(\"a\");\n    let t = s + \"b\";\n    println!(\"{s}{t}\");", "4:5 use of moved value: `s` (moved on line 3:13 by `+`"],
  ["use after a move in one branch", "}\nfn take(v: Vec<i32>) {}\nfn f() {\n    let v = vec![1];\n    if v.len() > 3 { take(v); }\n    v.len();", "7:5 use of moved value: `v` (moved on line 6:27 as an argument"],
  ["use after a move into a struct", "}\nstruct W { v: Vec<i32> }\nfn f() {\n    let v = vec![1];\n    let w = W { v };\n    v.len();", "7:5 use of moved value: `v` (moved on line 6:17 into a struct"],
  ["`Y::from` with no matching impl", "}\nstruct V(i64);\nimpl From<i64> for V { fn from(n: i64) -> Self { V(n) } }\nfn f() {\n    let v = V::from(1.5);", "6:13 the trait bound `V: From<{float}>` is not satisfied (`V` implements `From<i64>`)"],
  ["`.into()` with no matching impl", "}\nstruct V(i64);\nimpl From<i64> for V { fn from(n: i64) -> Self { V(n) } }\nfn f() {\n    let v: V = \"x\".into();", "6:16 the trait bound `V: From<String>` is not satisfied"],
  ["`.into()` with no target type", "}\nstruct V(i64);\nimpl From<i64> for V { fn from(n: i64) -> Self { V(n) } }\nfn f() {\n    let v = 5i64.into();", "6:13 type annotations needed: say what `.into()` converts to"],
  ["`?` without a `From` impl for the error", "}\nstruct A;\nstruct B;\nfn g() -> Result<i32, A> { Err(A) }\nfn h() -> Result<i32, B> {\n    let x = g()?;\n    Ok(x)", "7:13 `?` couldn't convert the error to `B` (add `impl From<A> for B`"],
  ["lossy `.into()`", "let a: i64 = 5;\n    let b: i32 = a.into();", "3:18 the trait bound `i32: From<i64>` is not satisfied"],
  ["`Vec` method on a `VecDeque`", "let mut q = std::collections::VecDeque::new();\n    q.push(1);", "3:5 no method named `push` found for `VecDeque<_>`"],
  ["push_back on an immutable `VecDeque`", "let q: std::collections::VecDeque<i32> = std::collections::VecDeque::new();\n    q.push_back(1);", "3:5 cannot borrow `q` as mutable, as it is not declared as mutable"],
  ["float keys in a `BTreeSet`", "let mut s = std::collections::BTreeSet::new();\n    s.insert(1.5);", "2:13 the trait bound `f64: Ord` is not satisfied"],
  ["wrong item type in a `BinaryHeap`", "let mut h = std::collections::BinaryHeap::new();\n    h.push(1);\n    h.push(\"x\");", "4:12 mismatched types: expected `{integer}`, found `String`"],
  ["struct without `Ord` as a `BTreeMap` key", "}\n#[derive(PartialEq, Eq)]\nstruct K(i32);\nfn f() {\n    let mut m = std::collections::BTreeMap::new();\n    m.insert(K(1), 2);", "6:13 the trait bound `K: Ord` is not satisfied"],
  ["`cmp::max` on floats", "let m = std::cmp::max(1.5, 2.0);", "2:13 the trait bound `{float}: Ord` is not satisfied (floats aren't totally ordered; use `a.max(b)` on `f64`)"],
  ["operator with no impl for the right operand", "}\n#[derive(Clone, Copy)]\nstruct V(f64);\nimpl std::ops::Add for V { type Output = V; fn add(self, o: V) -> V { V(self.0 + o.0) } }\nfn f() {\n    let v = V(1.0) + 2.0;", "7:13 no implementation for `V + {float}` (`V` implements `Add<V>`)"],
  ["operator with no impl at all", "}\n#[derive(Clone, Copy)]\nstruct V(f64);\nimpl std::ops::Add for V { type Output = V; fn add(self, o: V) -> V { V(self.0 + o.0) } }\nfn f() {\n    let v = V(1.0) * V(2.0);", "7:13 cannot apply `*` to `V` and `V` (add `impl Mul<V> for V` from `std::ops`)"],
  ["`+=` without `AddAssign`", "}\n#[derive(Clone, Copy)]\nstruct V(f64);\nimpl std::ops::Add for V { type Output = V; fn add(self, o: V) -> V { V(self.0 + o.0) } }\nfn f() {\n    let mut v = V(1.0);\n    v += V(2.0);", "8:5 cannot apply `+=` to `V` and `V`"],
  ["unary minus without `Neg`", "}\n#[derive(Clone, Copy)]\nstruct V(f64);\nimpl std::ops::Add for V { type Output = V; fn add(self, o: V) -> V { V(self.0 + o.0) } }\nfn f() {\n    let v = -V(1.0);", "7:13 cannot apply unary operator `-` to type `V`"],
  ["a type error inside a `From` impl", "}\nstruct A(i32);\nimpl From<i32> for A {\n    fn from(n: i32) -> Self { A(\"x\") }\n}\nfn f() {", "5:33 mismatched types: expected `i32`, found `String`"],
  ["indexing by a type without an `Index` impl", "}\nstruct G(Vec<i32>);\nimpl std::ops::Index<usize> for G { type Output = i32; fn index(&self, i: usize) -> &i32 { &self.0[i] } }\nfn f() {\n    let g = G(vec![1]);\n    let x = g[\"a\"];", "7:13 the type `G` cannot be indexed by `String`"],
  ["assigning through an index without `IndexMut`", "}\nstruct G(Vec<i32>);\nimpl std::ops::Index<usize> for G { type Output = i32; fn index(&self, i: usize) -> &i32 { &self.0[i] } }\nfn f() {\n    let mut g = G(vec![1]);\n    g[0] = 2;", "7:5 cannot assign to data in an index of `G` (add `impl IndexMut<usize> for G`"],
  ["an `index_mut` that doesn't hand out a place", "}\nstruct G(Vec<i32>);\nimpl std::ops::Index<usize> for G { type Output = i32; fn index(&self, i: usize) -> &i32 { &self.0[i] } }\nimpl std::ops::IndexMut<usize> for G { fn index_mut(&mut self, i: usize) -> &mut i32 { self.0.get_mut(i).unwrap() } }\nfn f() {\n    let mut g = G(vec![1]);\n    g[0] = 2;", "8:5 assigning through `G`'s `index_mut` needs it to end in `&mut <place>`"],
  ["use of a `let x;` never assigned", "let x: i32;\n    println!(\"{}\", x);", "3:20 used binding `x` isn't initialized"],
  ["use of a `let x;` assigned on one path", "}\nfn c() -> bool { true }\nfn f() {\n    let x;\n    if c() { x = 1; }\n    println!(\"{}\", x);", "7:20 used binding `x` is possibly-uninitialized"],
  ["assigning an immutable `let x;` twice", "let x;\n    x = 1;\n    x = 2;", "4:5 cannot assign twice to immutable variable `x`"],
  ["assigning an immutable `let x;` in a loop", "let x;\n    for i in 0..3 {\n        x = i;\n    }", "4:9 cannot assign twice to immutable variable `x`"],
  ["a trait const the impl leaves out", "}\ntrait U { const S: f64; }\nstruct M;\nimpl U for M {}\nfn f() {", "5:12 not all trait items implemented, missing: `S`"],
  ["an associated const of the wrong type", "}\nstruct G;\nimpl G { const W: usize = \"wide\"; }\nfn f() {", "4:27 mismatched types: expected `usize`, found `String`"],
  ["an associated const that doesn't exist", "}\nstruct G;\nimpl G { const W: usize = 3; }\nfn f() {\n    let h = G::H;", "6:13 no function or associated item named `H` found for struct `G`"],
  ["`{:x}` on a string", "let s = \"a\";\n    println!(\"{:x}\", s);", "doesn't implement `std::fmt::LowerHex`"],
  ["a width argument that isn't `usize`", "let w = 2.5;\n    println!(\"{:w$}\", 1);", "a width or precision must be `usize`, found `{float}`"],
  ["`{}` on a `VecDeque`", "let q: std::collections::VecDeque<i32> = std::collections::VecDeque::new();\n    println!(\"{}\", q);", "`VecDeque<i32>` doesn't implement `std::fmt::Display`"],
  ["casting to a trait object the type doesn't implement", "}\ntrait T { fn f(&self); }\nstruct S;\nfn g() {\n    let b = Box::new(S) as Box<dyn T>;", "6:22 the trait bound `S: T` is not satisfied"],
  ["`impl PartialOrd` without `PartialEq`", "}\nstruct T(i32);\nimpl PartialOrd for T {\n    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { self.0.partial_cmp(&o.0) }\n}\nfn f() {", "4:1 the trait bound `T: PartialEq` is not satisfied (`PartialOrd` requires it"],
  ["a macro that expands itself forever", "}\nmacro_rules! forever {\n    () => { forever!() };\n}\nfn f() {\n    forever!();", "7:5 recursion limit reached while expanding macros (128 levels; a `#![recursion_limit = \"256\"]` attribute on the crate raises it)"],
  ["a macro 130 levels deep, without a `#![recursion_limit]`", "}\nmacro_rules! count {\n    () => { 0usize };\n    ($h:tt $($t:tt)*) => { 1usize + count!($($t)*) };\n}\nfn f() {\n    let n = count!(" + "a b c d e f g h i j ".repeat(13) + ");", "8:13 recursion limit reached while expanding macros (128 levels"],
  ["`log::debug!` isn't the crate's own `debug!` (which wraps it)", "}\nmacro_rules! log {\n    ($($tt:tt)*) => { $($tt)* };\n}\nmacro_rules! debug {\n    ($($tt:tt)*) => { log!(log::debug!($($tt)*)) };\n}\nfn f() {\n    debug!(\"x {}\", 1);", "10:5 macro `debug!`"],
  ["a macro call no rule matches","}\nmacro_rules! twice {\n    ($x:expr) => { $x + $x };\n}\nfn f() {\n    let a = twice!();", "7:13 no rules of `macro_rules! twice` matched this invocation"],
  ["a call matching only the `#[cfg]`-off definition of a macro", "}\n#[cfg(feature = \"x\")]\nmacro_rules! m {\n    () => { 1 };\n}\n#[cfg(not(feature = \"x\"))]\nmacro_rules! m {\n    ($x:expr) => { $x };\n}\nfn f() {\n    let a = m!();", "12:13 no rules of `macro_rules! m` matched this invocation"],
  ["a type error inside a macro expansion", "}\nmacro_rules! twice {\n    ($x:expr) => { $x + $x };\n}\nfn f() {\n    let a: i32 = twice!(\"s\");", "7:18 mismatched types: expected `i32`, found `String`"],
  ["a type error after a macro expansion keeps its position", "}\nmacro_rules! twice {\n    ($x:expr) => { $x + $x };\n}\nfn f() {\n    let a = twice!(1);\n    let b: String = a;", "8:21 mismatched types: expected `String`, found `{integer}`"],
  ["mixed arithmetic", "let a: i32 = 1;\n    let b: f64 = 2.0;\n    let c = a + b;", "4:13 cannot add `f64` to `i32`"],
  ["a type error through a type alias", "}\ntype Score = u64;\nfn f() {\n    let s: Score = \"x\";", "5:20 mismatched types: expected `u64`, found `String`"],
  ["a generic alias given a wrong element", "}\ntype Grid<T> = Vec<Vec<T>>;\nfn f() {\n    let g: Grid<i32> = vec![vec![\"a\"]];", "5:34 mismatched types: expected `i32`, found `String`"],
  ["a variant import misused as a function", "}\nenum K {\n    A,\n    B(u32),\n}\nuse K::*;\nfn f() {\n    let k = A(1);", "9:13 expected function, found unit variant `K::A`"],
  ["an integer range left out of a match", "}\nfn f(b: u8) -> u8 {\n    match b {\n        0..=100 => 1,\n        102..=255 => 2,\n    }", "4:11 non-exhaustive patterns: `101_u8` not covered"],
  ["a char left out of a match", "}\nfn f(c: char) -> u8 {\n    match c {\n        '\\0'..='a' => 1,\n        'c'..=char::MAX => 2,\n    }", "4:11 non-exhaustive patterns: `'b'` not covered"],
  ["a `static mut` used outside `unsafe`", "}\nstatic mut COUNT: u32 = 0;\nfn f() -> u32 {\n    COUNT", "5:5 use of mutable static is unsafe and requires unsafe block"],
  ["assigning to an immutable static", "}\nstatic LIMIT: u32 = 3;\nfn f() {\n    LIMIT = 4;", "5:5 cannot assign to immutable static item `LIMIT`"],
  ["a trait's static function without its type", "}\ntrait Named {\n    fn name() -> String;\n}\nfn f() {\n    let n = Named::name();", "7:13 cannot call associated function on trait without specifying the corresponding `impl` type"],
  ["a `pub(super)` function used from further away", "}\nmod a {\n    pub mod b {\n        pub(super) fn f() {}\n    }\n}\nfn g() {\n    a::b::f();", "9:5 `f` is private to module `a::b`"],
  ["a macro's own `let` isn't the caller's", "}\nmacro_rules! def {\n    () => {\n        let hidden = 1;\n    };\n}\nfn f() {\n    def!();\n    let y = hidden;", "10:13 cannot find value `hidden` in this scope"],
  ["calling a private function of an inline module", "}\nmod geo {\n    fn secret() {}\n}\nfn f() {\n    geo::secret();", "7:5 `secret` is private to module `geo`"],
  ["importing a private item of an inline module", "}\nmod geo {\n    struct Hidden;\n}\nuse geo::Hidden;\nfn f() {", "6:1 `Hidden` is private to module `geo`"],
  ["a missing item of an inline module", "}\nmod geo {\n    pub fn area() {}\n}\nfn f() {\n    geo::volume();", "7:5 cannot find `volume` in module `geo`"],
  ["a missing item named inside a macro", "}\nmod geo {\n    pub const UNIT: &str = \"m\";\n}\nfn f() {\n    println!(\"{}\", geo::UNITS);", "7:20 cannot find `UNITS` in module `geo`"],
  ["reaching into a private nested module", "}\nmod a {\n    mod b {\n        pub fn f() {}\n    }\n}\nfn g() {\n    a::b::f();", "9:5 module `b` is private"],
  ["a `super::` path to a missing item", "}\nmod a {\n    pub mod b {\n        pub fn f() -> i32 {\n            super::missing()\n        }\n    }\n}\nfn g() {", "6:13 cannot find `missing` in module `a`"],
  ["pointer arithmetic", "let v = [1, 2];\n    let p: *const i32 = &v[0];\n    let q = unsafe { p.add(1) };", "4:22 `add()` on a raw pointer isn't supported"],
  ["JSON of a closure", "let f = |x: i32| x + 1;\n    let s = js::JSON::stringify(&f);", "3:33 the trait bound `impl Fn(i32) -> i32: Serialize` is not satisfied"],
  ["JSON::parse without a target type", "let v = js::JSON::parse(\"1\");", "2:13 type annotations needed: say what to parse into"],
  ["a misspelled Date method", "let d = js::Date::new();\n    let h = d.get_hour();", "3:13 no method named `get_hour` found for `Date`; did you mean `get_hours`?"],
  ["Date::utc with too few arguments", "let t = js::Date::utc(2024);", "2:13 `Date::utc` takes 2 to 7 arguments but 1 was supplied"],
  ["a misspelled js associated function", "let t = js::Date::pars(\"x\");", "2:13 no function or associated item named `pars` found for `Date`; did you mean `parse`?"],
  ["a js method called as an associated function", "let t = js::Date::get_time();", "2:13 `Date::get_time` is a method: call it on a value (`x.get_time(..)`)"],
  ["a js associated function called as a method", "let d = js::Date::new();\n    let t = d.now();", "3:13 `now` is an associated function of `Date`, not a method: call `Date::now(..)`"],
  ["a js function with no arguments", "let s = js::btoa();", "2:13 `js::btoa` takes 1 argument but 0 were supplied"],
  ["a js function given the wrong type", "let s = js::btoa(5);", "2:22 mismatched types"],
  ["a timer handler that isn't a closure", "js::set_timeout(5, 10.0);", "2:21 mismatched types"],
  ["Promise::all of plain values", "let x = js::Promise::all(vec![1, 2]);", "2:30 `Promise::all` takes a `Vec` of futures (unawaited async calls), found `Vec<{integer}>`"],
  ["a js function that isn't imported", "let s = btoa(\"a\");", "2:13 cannot find function `btoa` in this scope"],
  ["an associated type used as a concrete type", "}\ntrait Shape {\n    type Unit;\n    fn area(&self) -> Self::Unit;\n}\nfn f<S: Shape>(s: &S) -> f64 {\n    s.area()", "8:5 mismatched types: expected `f64`, found `S::Unit`"],
  ["a method an associated type's bounds don't have", "}\ntrait Shape {\n    type Unit;\n    fn area(&self) -> Self::Unit;\n}\nfn f<S: Shape>(s: &S) {\n    let y = s.area().sqrt();", "8:13 no method named `sqrt` found for type parameter `S::Unit`"],
  ["{:?} of an associated type without Debug", "}\ntrait Shape {\n    type Unit;\n    fn area(&self) -> Self::Unit;\n}\nfn f<S: Shape>(s: &S) {\n    println!(\"{:?}\", s.area());", "8:14 `S::Unit` doesn't implement `std::fmt::Debug`"],
  ["arithmetic on an associated type without Add", "}\ntrait Shape {\n    type Unit;\n    fn area(&self) -> Self::Unit;\n}\nfn f<S: Shape>(s: &S) {\n    let a = s.area();\n    let b = a + a;", "9:13 `+` on associated type `S::Unit` needs a bound: declare it in the trait (`type Unit: Add`)"],
  ["two type parameters' associated types", "}\ntrait Shape {\n    type Unit;\n    fn area(&self) -> Self::Unit;\n}\nfn f<S: Shape, T: Shape>(s: &S, t: &T) -> S::Unit {\n    t.area()", "8:5 mismatched types: expected `S::Unit`, found `T::Unit`"],
  ["a generic trait's argument through a bound", "}\ntrait Convert<T> {\n    fn convert(&self) -> T;\n}\nfn f<C: Convert<String>>(c: &C) -> i32 {\n    c.convert()", "7:5 mismatched types: expected `i32`, found `String`"],
  ["an impl missing an associated type", "}\ntrait Shape {\n    type Unit;\n    fn area(&self) -> Self::Unit;\n}\nstruct Sq;\nimpl Shape for Sq {\n    fn area(&self) -> f64 {\n        1.0\n    }\n}\nfn f() {", "8:16 not all trait items implemented, missing: `Unit`"],
  ["an associated type the trait doesn't have", "}\ntrait Named {\n    fn name(&self) -> String;\n}\nstruct A;\nimpl Named for A {\n    type Extra = i32;\n    fn name(&self) -> String {\n        String::new()\n    }\n}\nfn f() {", "8:5 type `Extra` is not a member of trait `Named`"],
  ["an associated type default", "}\ntrait Shape {\n    type Unit = f64;\n}\nfn f() {", "4:5 associated type defaults are unstable"],
  ["a blanket impl's method on a type without the bound", "}\ntrait Shout {\n    fn shout(&self) -> String;\n}\nimpl<T: std::fmt::Display> Shout for T {\n    fn shout(&self) -> String {\n        format!(\"{}!\", self)\n    }\n}\nstruct P;\nfn f() {\n    let s = P.shout();", "13:13 no method named `shout` found for struct `P`"],
  ["a const argument nothing gives", "}\nfn zeros<const N: usize>() -> [f64; N] {\n    [0.0; N]\n}\nfn f() {\n    let z = zeros();", "7:13 type annotations needed: cannot infer the value of the const parameter `N` declared on the function `zeros`"],
  ["mismatched const arguments", "}\nstruct M<const R: usize> {\n    d: [i32; R],\n}\nimpl<const R: usize> M<R> {\n    fn z() -> Self {\n        M { d: [0; R] }\n    }\n}\nfn f() {\n    let m: M<2> = M::<3>::z();", "12:19 mismatched types: expected `M<2>`, found `M<3>`"],
  ["an expression as a const argument", "}\nfn one<const N: usize>() -> usize {\n    N\n}\nfn two<const N: usize>() -> usize {\n    one::<{ N + 1 }>()\n}\nfn f() {", "7:13 generic parameters may not be used in const operations"],
  ["a const method on a struct const as a const argument", "}\nstruct C {\n    n: usize,\n}\nimpl C {\n    const fn get(&self) -> usize {\n        self.n\n    }\n}\nconst K: C = C { n: 2 };\nfn mk<const N: usize>() -> [u8; N] {\n    [1; N]\n}\nfn f() {\n    let a = mk::<{ K.get() }>();", "16:20 this const argument isn't supported yet"],
  ["a struct's const argument nothing gives", "}\nstruct S<const N: usize> {\n    v: Vec<i32>,\n}\nfn f() {\n    let s = S { v: vec![] };", "7:13 type annotations needed: cannot infer the value of the const parameter `N` declared on the struct `S`"],
  ["an enum's const argument nothing gives", "}\nenum E<const N: usize> {\n    A,\n    B([u8; N]),\n}\nfn f() {\n    let e = E::A;", "8:13 type annotations needed: cannot infer the value of the const parameter `N` declared on the enum `E`"],
  ["parse into a type without FromStr","}\nstruct P;\nfn f() {\n    let p = \"1\".parse::<P>();", "5:13 the trait bound `P: FromStr` is not satisfied"],
  ["using an array of Strings after moving it", "let a = [String::from(\"x\")];\n    let b = a;\n    println!(\"{:?} {:?}\", a, b);", "4:27 use of moved value: `a`"],
  ["structured_clone of a type without Clone", "}\nstruct P {\n    x: i32,\n}\nfn f() {\n    let p = P { x: 1 };\n    let q = js::structured_clone(&p);", "8:34 the trait bound `P: Clone` is not satisfied (`structured_clone` copies Rust values by their `Clone`)"],
  ["a pointer cast to an integer", "let x = 1;\n    let p = &x as *const i32;\n    let a = p as usize;", "4:13 casting a raw pointer to an integer isn't supported"],
  ["an integer cast to a pointer", "let p = 0x1000 as *const i32;", "2:13 casting an integer to a raw pointer isn't supported"],
  ["transmute between sizes", "let x: u64 = unsafe { std::mem::transmute(1u32) };", "2:27 cannot transmute between types of different sizes"],
  ["transmute of a Vec", "let x: u64 = unsafe { std::mem::transmute(vec![1u8]) };", "2:27 `transmute` from `Vec<u8>` to `u64` isn't supported"],
  ["a pointer to a number in a struct", "}\nstruct S {\n    p: *mut i32,\n}\nfn g() {", "4:8 a raw pointer to a number, string or bool in a struct isn't supported yet"],
  ["reference counts", "let r = std::rc::Rc::new(1);\n    let n = std::rc::Rc::strong_count(&r);","3:13 `Rc::strong_count` isn't supported"],
  ["a mismatch inside `u32::pow(..)`", "let x = u32::pow(2, \"a\");", "2:25 mismatched types: expected `u32`, found `String`"],
  ["unzip of non-pairs","let v: (Vec<i32>, Vec<i32>) = vec![1, 2].into_iter().unzip();", "2:35 `unzip` needs an iterator of pairs"],
  ["returning a pointer to a number", "}\nfn g(p: *mut i32) -> *mut i32 {\n    p", "3:22 returning a raw pointer to a number, string or bool isn't supported yet"],
  // Language gaps (PLAN): impls for `&T` / `Box<T>`, byte readers, const generics on traits.
  [
    "an impl for `Box<T>` that doesn't forward to `T`'s",
    "}\ntrait T { fn a(&self) -> i32; }\nstruct S;\nimpl T for S { fn a(&self) -> i32 { 1 } }\nimpl<X: T> T for Box<X> { fn a(&self) -> i32 { (**self).a() + 1 } }\nfn f() {",
    "6:18 an impl for `&T` / `&mut T` / `Box<T>` must hand each method on to `T`'s impl",
  ],
  [
    "an impl for `&T` leaving a default method another impl overrides",
    "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 0 } }\nstruct S;\nimpl T for S { fn a(&self) -> i32 { 1 } fn b(&self) -> i32 { 2 } }\nimpl<X: T + ?Sized> T for &X { fn a(&self) -> i32 { (**self).a() } }\nfn f() {",
    "6:27 an impl for `&T` / `&mut T` / `Box<T>` must hand each method on to `T`'s impl",
  ],
  [
    "a blanket impl of another file's trait",
    "}\nuse crate::io::Sink;\nimpl<T: Clone> Sink for T {}\nfn f() {",
    "4:16 `impl Sink for ..` is not supported yet",
  ],
  [
    "reading through a `&mut &[u8]`",
    "}\nuse std::io::Read;\nfn r<R: Read>(r: &mut R) -> usize { 0 }\nfn f() {\n    let d = vec![1u8];\n    let mut s = &d[..];\n    r(&mut s);",
    "8:7 reading from a `&mut &[u8]` would advance the caller's slice",
  ],
  [
    "a trait's const parameter in a default method",
    "}\ntrait B<const N: usize> { fn n(&self) -> usize { N } }\nfn f() {",
    "3:50 a trait's const generic parameter (`N`) in its default methods is not supported yet",
  ],
  // Std gaps (PLAN "Std-library gaps").
  ["`env!` of a variable the build doesn't have", 'let v = env!("CARCINI_NOPE");', "2:13 environment variable `CARCINI_NOPE` not defined at compile time"],
  ["`compile_error!` that's reached", 'compile_error!("boom");', "2:5 boom"],
  ["`compile_error!` item not under a `cfg`", '}\ncompile_error!("unsupported");\nfn g() {', "3:1 unsupported"],
  [
    "`x.fmt(f)` with both `Display` and `Debug` in scope",
    "}\nuse std::fmt::{self, Debug, Display};\nstruct S(u32);\nimpl Display for S {\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {\n        self.0.fmt(f)\n    }\n}\nfn g() {",
    "7:9 multiple applicable items in scope",
  ],
  ["`resize` with the wrong fill value", 'let mut v = vec![1u8];\n    v.resize(3, "x");', "3:17 mismatched types: expected `u8`, found `String`"],
  ["`next_power_of_two` on a signed integer", "let x = 5i32.next_power_of_two();", "2:13 no method named `next_power_of_two`"],
  [
    "`iter_mut()` over numbers into `partition`",
    "let mut v = vec![1, 2, 3];\n    let (a, b): (Vec<&mut i32>, Vec<&mut i32>) = v.iter_mut().partition(|x| **x > 1);",
    "3:50 `iter_mut()` followed by `.partition()` isn't supported yet over numbers",
  ],
  [
    "itertools' `set_from` on `iter()`",
    "}\nuse itertools::Itertools;\nfn g() {\n    let mut v = vec![1];\n    v.iter().set_from(vec![2]);",
    "6:5 `set_from` assigns through the items: call it on `iter_mut()`",
  ],
  [
    "indexing a `Vec<&mut i32>` collected from `iter_mut()`",
    "let mut w = vec![1, 2];\n    let mut rs: Vec<&mut i32> = w.iter_mut().collect();\n    *rs[1] = 9;",
    "4:6 indexing a `Vec<&mut T>` collected from `iter_mut()` over numbers, strings or bools isn't supported yet",
  ],
  [
    "returning a `&mut` iterator with `&mut` inside tuple items",
    "}\nfn f(v: &mut Vec<u8>) -> impl Iterator<Item = (usize, &mut u8)> {\n    v.iter_mut().enumerate()\n}\nfn g() {",
    "4:5 mismatched types",
  ],
  ["`get_index_mut` on an `IndexMap` of numbers, not matched", "let mut m: indexmap::IndexMap<&str, i32> = indexmap::IndexMap::new();\n    m.insert(\"a\", 1);\n    *m.get_index_mut(0).unwrap().1 += 1;", "4:6 `IndexMap::get_index_mut` on a map of `i32` isn't supported here"],
  // Help lines for players who don't know Rust (`diag_help.rs`): the rule under the message.
  ["use after move", "let s = String::from(\"a\");\n    let t = s;\n    let u = s;\n    println!(\"{} {}\", t, u);", "4:13 use of moved value: `s` (moved on line 3:13 by `let`; `.clone()` it there to keep a copy)", "Passing or assigning a `String`, `Vec` or struct hands it over; the old name can't be used again."],
  ["two `&mut` borrows", "let mut v = vec![1];\n    let a = &mut v;\n    let b = &mut v;\n    a.push(1);\n    b.push(2);", "4:13 cannot borrow `v` as mutable more than once at a time", "`&v` lends read access and `&mut v` write access; while a `&mut` is alive nothing else may touch `v`"],
  ["push while borrowed", "let mut v = vec![1];\n    let r = &v[0];\n    v.push(2);\n    println!(\"{}\", r);", "4:5 cannot borrow `v` as mutable because it is also borrowed as immutable", "`&v` lends read access and `&mut v` write access"],
  ["f64 times i32", "let x = 2.0 * 3i32;", "2:13 cannot multiply `{float}` by `i32`", "Rust never converts between number types for you; all NS numbers are `f64`."],
  ["an Option where a value is expected", "let v = vec![1];\n    let x: i32 = v.first();", "3:18 mismatched types: expected `i32`, found `Option<{integer}>`", "`Option<T>` is a value that may be missing (`null` in JavaScript)"],
  ["JavaScript's `length` as a method", "let v = vec![1];\n    let n = v.length();", "3:13 no method named `length` found for `Vec<{integer}>` (`length` is `.len()` in Rust)", "Rust's std names differ from JavaScript's"],
  ["JavaScript's `length` as a field", "let s = String::new();\n    let n = s.length;", "3:13 no field `length` on type `String` (`length` is `.len()` in Rust)", "Rust's std names differ from JavaScript's"],
  ["JavaScript's `push_back`", "let mut v = vec![1];\n    v.push_back(2);", "3:5 no method named `push_back` found for `Vec<{integer}>` (`push_back` is `.push(x)` in Rust)", "Rust's std names differ from JavaScript's"],
  ["a missing `;` (the parser's token name written as the token)", "let q = 1\n    let r = 2;", "2:14 expected `;`", "Every statement ends with `;`"],
  ["an unknown method has the FEATURES pointer and no help", "let v = vec![1];\n    let x = v.frobnicate();", "3:13 no method named `frobnicate` found for `Vec<{integer}>` (the std surface is still small; see docs/FEATURES.md)", null],
];
// An optional fourth column is the expected `help` prefix (`null`: the diagnostic must carry none).
function checkRejected(name, out, want, wantHelp) {
  const msgs = out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`);
  const matches = (m) => (/^\d/.test(want) ? m.startsWith(want) : m.includes(want));
  const hit = out.diagnostics.find((d) => matches(`${d.line}:${d.column} ${d.message}`));
  const helpOk =
    wantHelp === undefined || (wantHelp === null ? hit?.help === undefined : (hit?.help ?? "").startsWith(wantHelp));
  if (out.code !== null || !hit || !helpOk) {
    const wantedHelp = wantHelp === undefined ? "" : `\nwant help: ${wantHelp}`;
    const gotHelp = hit?.help ? `\nhelp: ${hit.help}` : "";
    fail(name, `want: ${want}${wantedHelp}\ngot:\n${msgs.join("\n") || "(no diagnostics)"}${gotHelp}`);
  } else pass(name);
}
for (const [label, body, want, wantHelp] of rejected) {
  checkRejected(`rejects: ${label}`, compile(`fn main() {\n    ${body}\n}\n`, "bad.rs"), want, wantHelp);
}

// Modules: each case is a set of files; the first is compiled.
const rejectedFiles = [
  [
    "a generic trait implemented twice, called through a type parameter",
    { "main.rs": "trait Conv<T> {\n    fn conv(&self) -> T;\n}\nimpl Conv<u8> for u32 {\n    fn conv(&self) -> u8 {\n        1\n    }\n}\nimpl Conv<u16> for u32 {\n    fn conv(&self) -> u16 {\n        2\n    }\n}\nfn go<X: Conv<u16>>(x: X) -> u16 {\n    x.conv()\n}\nfn main() {\n    println!(\"{}\", go(5u32));\n}\n" },
    "15:5 `conv` comes from a generic trait implemented more than once for one type",
  ],
  [
    "importing a missing item through a nested module file",
    { "main.rs": "mod geo;\nuse geo::shapes::nope;\nfn main() {}", "geo.rs": "pub mod shapes;", "geo/shapes.rs": "pub fn area() {}" },
    "2:5 unresolved import `geo::shapes::nope` (it doesn't exist or isn't `pub`)",
  ],
  [
    "a runaway macro under a huge `#![recursion_limit]`",
    { "main.rs": "#![recursion_limit = \"100000\"]\nmacro_rules! forever {\n    () => { forever!() };\n}\nfn main() {\n    forever!();\n}\n" },
    "6:5 recursion limit reached while expanding macros (1024 levels, the most CarciniScript expands)",
  ],
  [
    "a module file's deep macro when the crate root's limit is too low",
    // (`deep.rs` is the one compiled; `main.rs` declares it.)
    { "deep.rs": "macro_rules! count {\n    () => { 0usize };\n    ($h:tt $($t:tt)*) => { 1usize + count!($($t)*) };\n}\npub fn f() -> usize {\n    count!(" + "a b c d e f g h i j ".repeat(14) + ")\n}\n", "main.rs": "#![recursion_limit = \"130\"]\nmod deep;\nfn main() {\n    deep::f();\n}\n" },
    "6:5 recursion limit reached while expanding macros (130 levels",
  ],
  ["module file not found",{ "main.rs": "mod nope;\nfn main() {}" }, "1:1 file not found for module `nope` (looked for `nope.rs` and `nope/mod.rs`)"],
  // (A `#[path]` is looked for on its own, relative to the file's directory.)
  ["a `#[path]` module file not found", { "hack/main.rs": "#[path = \"../lib/nope.rs\"]\nmod nope;\nfn main() {}" }, "1:1 file not found for module `nope` (looked for `lib/nope.rs`)"],
  ["a `#[path]` above the root", { "main.rs": "#[path = \"../nope.rs\"]\nmod nope;\nfn main() {}" }, "1:1 file not found for module `nope` (`#[path = \"../nope.rs\"]` leaves the project root)"],
  [
    "a proc macro that panics",
    {
      "main.rs": "fn main() {\n    let x = mac::boom!(1 2);\n}",
      "mac.rs": "use proc_macro::TokenStream;\n#[proc_macro]\npub fn boom(input: TokenStream) -> TokenStream {\n    panic!(\"can't use {}\", input)\n}",
    },
    "2:13 proc macro panicked: can't use 1 2",
  ],
  [
    "a Vec spliced outside a `quote!` repetition",
    {
      "mac.rs": "use proc_macro::TokenStream;\nuse quote::quote;\n#[proc_macro]\npub fn m(input: TokenStream) -> TokenStream {\n    let v = vec![1, 2];\n    quote!(#v).into()\n}",
    },
    "6:13 the trait bound `Vec<{integer}>: ToTokens` is not satisfied",
  ],
  [
    "a `quote!` repetition over a number",
    {
      "mac.rs": "use proc_macro::TokenStream;\nuse quote::quote;\n#[proc_macro]\npub fn m(input: TokenStream) -> TokenStream {\n    let n = 3;\n    quote!(#(#n),*).into()\n}",
    },
    "6:15 `#(..)*` needs a collection or iterator to repeat over, and `n` is `{integer}`",
  ],
  [
    "an unknown variable in `quote!`",
    {
      "mac.rs": "use proc_macro::TokenStream;\nuse quote::quote;\n#[proc_macro]\npub fn m(input: TokenStream) -> TokenStream {\n    quote!(fn #name() {}).into()\n}",
    },
    "5:16 cannot find value `name` in this scope",
  ],
  ["`quote!` outside a proc-macro crate", { "main.rs": "fn main() {\n    let t = quote!(x);\n}" }, "2:13 `quote!` works in proc-macro crates"],
  [
    "a `syn::Error` from a derive",
    {
      "main.rs": "use mac::OnlyStructs;\n#[derive(OnlyStructs)]\nenum E { A }\nfn main() {}",
      "mac.rs": "use proc_macro::TokenStream;\nuse syn::{parse_macro_input, Data, DeriveInput};\n#[proc_macro_derive(OnlyStructs)]\npub fn only(input: TokenStream) -> TokenStream {\n    let input = parse_macro_input!(input as DeriveInput);\n    match input.data {\n        Data::Struct(_) => TokenStream::new(),\n        _ => syn::Error::new(input.ident.span(), \"OnlyStructs needs a struct\").to_compile_error().into(),\n    }\n}",
    },
    "2:1 OnlyStructs needs a struct",
  ],
  [
    "a `parse_macro_input!` failure",
    {
      "main.rs": "use mac::label;\n#[label(42)]\nfn f() {}\nfn main() {}",
      "mac.rs": "use proc_macro::TokenStream;\nuse syn::{parse_macro_input, LitStr};\n#[proc_macro_attribute]\npub fn label(attr: TokenStream, item: TokenStream) -> TokenStream {\n    let _l = parse_macro_input!(attr as LitStr);\n    item\n}",
    },
    "2:1 expected string literal, found `42`",
  ],
  [
    "a `lookahead1` error from a custom parser",
    {
      "main.rs": "use mac::pick;\nfn main() {\n    let x = pick!(;);\n}",
      "mac.rs": "use proc_macro::TokenStream;\nuse syn::parse::{Parse, ParseStream};\nuse syn::{parse_macro_input, Ident, LitInt, Result, Token};\nstruct Pick;\nimpl Parse for Pick {\n    fn parse(input: ParseStream) -> Result<Self> {\n        let la = input.lookahead1();\n        if la.peek(Ident) || la.peek(LitInt) || la.peek(Token![-]) {\n            Ok(Pick)\n        } else {\n            Err(la.error())\n        }\n    }\n}\n#[proc_macro]\npub fn pick(input: TokenStream) -> TokenStream {\n    let _p = parse_macro_input!(input as Pick);\n    TokenStream::new()\n}",
    },
    "3:13 expected one of: identifier, integer literal, `-`",
  ],
  [
    "a missing `Token![=]`",
    {
      "main.rs": "use mac::assign;\nfn main() {\n    assign!(x 1);\n}",
      "mac.rs": "use proc_macro::TokenStream;\nuse syn::{parse::ParseStream, parse_macro_input, Ident, LitInt, Result, Token};\nfn parse(input: ParseStream) -> Result<(Ident, LitInt)> {\n    let name: Ident = input.parse()?;\n    input.parse::<Token![=]>()?;\n    Ok((name, input.parse()?))\n}\n#[proc_macro]\npub fn assign(input: TokenStream) -> TokenStream {\n    let _a = parse_macro_input!(input with parse);\n    TokenStream::new()\n}",
    },
    "3:5 expected `=`, found `1`",
  ],
  [
    "a proc macro that never finishes",
    {
      "main.rs": "use mac::spin;\nfn main() {\n    spin!();\n}",
      "mac.rs": "use proc_macro::TokenStream;\n#[proc_macro]\npub fn spin(input: TokenStream) -> TokenStream {\n    let mut n = 0u64;\n    loop {\n        n += 1;\n    }\n}",
    },
    "3:5 the proc macro ran too long",
  ],
  [
    "a proc macro crate that doesn't compile",
    {
      "main.rs": "use mac::Nothing;\n#[derive(Nothing)]\nstruct S;\nfn main() {}",
      "mac.rs": "use proc_macro::TokenStream;\n#[proc_macro_derive(Nothing)]\npub fn nothing(input: TokenStream) -> TokenStream {\n    let n: i32 = \"x\";\n    TokenStream::new()\n}",
    },
    "2:1 the proc macro crate doesn't compile: mac.rs:4:18: mismatched types",
  ],
  [
    "a type error in a proc macro's output",
    {
      "main.rs": "use mac::make;\nfn main() {\n    let n: i32 = make!();\n}",
      "mac.rs": "use proc_macro::TokenStream;\n#[proc_macro]\npub fn make(_input: TokenStream) -> TokenStream {\n    \"\\\"text\\\"\".parse().unwrap()\n}",
    },
    "3:18 mismatched types: expected `i32`, found `String`",
  ],
  [
    "importing a private function",
    { "main.rs": "mod u;\nuse u::secret;\nfn main() {}", "u.rs": "fn secret() {}" },
    "2:5 unresolved import `u::secret` (it doesn't exist or isn't `pub`)",
  ],
  [
    "calling a private function through its module",
    { "main.rs": "mod u;\nfn main() {\n    u::secret();\n}", "u.rs": "fn secret() {}" },
    "3:5 cannot find function `secret` in module `u`",
  ],
  [
    "circular modules",
    { "main.rs": "mod a;\nfn main() {}", "a.rs": "mod main;\npub fn f() {}" },
    "1:1 circular modules",
  ],
  ["crates other than std", { "main.rs": "use reqwest::blocking;\nfn main() {}" }, "1:5 unresolved import `reqwest`"],
  [
    "renaming a crate other than std",
    { "main.rs": "extern crate reqwest as http;\nfn main() {}" },
    "1:1 unresolved import `reqwest`",
  ],
  [
    "two imported types of one name, renamed",
    {
      "main.rs": "mod a;\nmod b;\nuse a::Error as AError;\nuse b::Error as BError;\nfn main() {}",
      "a.rs": "pub struct Error;",
      "b.rs": "pub struct Error(pub i32);",
    },
    "3:5 renaming the imported type `Error` isn't supported when another `Error` is in scope",
  ],
  [
    "a renamed imported type next to the file's own of that name",
    { "main.rs": "mod a;\nuse a::Error as AError;\nstruct Error;\nfn main() {}", "a.rs": "pub struct Error;" },
    "2:5 renaming the imported type `Error` isn't supported when another `Error` is in scope",
  ],
  [
    "`TypeId::of` a type parameter (erased at run time)",
    { "main.rs": "use std::any::{Any, TypeId};\nfn id<T: 'static>() -> TypeId {\n    TypeId::of::<T>()\n}\nfn main() {}\n" },
    "3:5 the type here is a type parameter",
  ],
  [
    "`type_id()` of a `dyn Any`",
    { "main.rs": "use std::any::Any;\nfn f(x: &dyn Any) -> bool {\n    x.type_id() == std::any::TypeId::of::<i32>()\n}\nfn main() {}\n" },
    "3:5 the type of a `dyn Any` isn't known at run time",
  ],
  [
    "`is::<Option<T>>()` on a `dyn Any`",
    { "main.rs": "use std::any::Any;\nfn f(x: &dyn Any) -> bool {\n    x.is::<Option<i32>>()\n}\nfn main() {}\n" },
    "3:5 `is::<Option<i32>>()` can't be told apart at run time",
  ],
];
for (const [label, files, want] of rejectedFiles) {
  const name = `rejects: ${label}`;
  const entry = Object.keys(files)[0];
  const out = compile(files[entry], entry, (p) => files[p]);
  const msgs = out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`);
  if (out.code !== null || !msgs.some((m) => m.startsWith(want))) {
    fail(name, `want: ${want}\ngot:\n${msgs.join("\n") || "(no diagnostics)"}`);
  } else pass(name);
}

// --- language service: hover and completions (no NS bindings needed for these)
{
  const src = [
    "#[derive(Debug, Clone)]",
    "struct Target { host: String, money: f64 }",
    "impl Target { fn score(&self) -> f64 { self.money * 2.0 } }",
    "fn main() {",
    "    let t = Target { host: String::from(" + '"' + "a" + '"' + "), money: 1.5 };",
    "    let s = t.score();",
    "    let v = vec![1, 2, 3];",
    "    t.",
    "}",
  ].join("\n");
  const checks = [
    ["hover on a binding", () => hover(src, "svc.rs", 6, 9)?.contents, "s: f64"],
    ["hover on a method", () => hover(src, "svc.rs", 6, 15)?.contents, "fn score(&self) -> f64\n```\n*Target*"],
    // Std methods: the signature with the receiver's type arguments, the owner, and std's summary.
    [
      "hover on a std method",
      () => hover(src.replace("    t.", "    let f = v.first();"), "svc.rs", 8, 17)?.contents,
      "fn first(&self) -> Option<&i32>\n```\n*[i32]*\n\nReturns the first element of the slice, or `None` if it is empty.",
    ],
    [
      "hover on an Option method",
      () => hover(src.replace("    t.", "    let f = v.first().unwrap_or(&0);"), "svc.rs", 8, 25)?.contents,
      "fn unwrap_or(self, default: i32) -> i32\n```\n*Option<i32>*\n\nReturns the contained `Some` value or a provided default.",
    ],
    [
      "hover on an iterator method",
      () => hover(src.replace("    t.", "    let n = v.iter().map(|x| x + 1);"), "svc.rs", 8, 23)?.contents,
      "fn map<B>(self, f: impl FnMut(i32) -> B) -> impl Iterator<Item = B>\n```\n*impl Iterator<Item = i32>*\n\nTakes a closure",
    ],
    [
      "hover on a std associated function",
      () => hover(src, "svc.rs", 5, 37)?.contents,
      "fn from(s: &str) -> String\n```\n*String*\n\nConverts a `&str` into a `String`.",
    ],
    [
      "hover on an integer method",
      () => hover(src.replace("    t.", "    let m = 3i32.pow(2);"), "svc.rs", 8, 19)?.contents,
      "fn pow(self, exp: u32) -> i32\n```\n*i32*\n\nRaises self to the power of `exp`",
    ],
    [
      "std completions carry signatures and docs",
      () => {
        const c = completions(src.replace("    t.", "    v."), "svc.rs", 8, 7).find((c) => c.label === "push");
        return c && `${c.detail} | ${c.documentation}`;
      },
      "fn push(&mut self, value: i32) | Appends an element to the back of a collection.",
    ],
    [
      "signature help on a std method",
      () => {
        const h = signatureHelp(src.replace("    t.", "    v.insert("), "svc.rs", 8, 14);
        return h && `${h.label} ${JSON.stringify(h.params)} @${h.active} ${h.documentation}`;
      },
      'fn insert(&mut self, index: usize, element: i32) ["index: usize","element: i32"] @0 Inserts an element at position `index`',
    ],
    [
      "hover on an itertools method",
      () => {
        const it = "use itertools::Itertools;\n" + src.replace("    t.", "    let s = v.iter().sorted();");
        return hover(it, "svc.rs", 9, 25)?.contents;
      },
      "fn sorted(self) -> impl Iterator<Item = i32>\n```\n*impl Iterator<Item = i32>*\n\nSort all iterator elements",
    ],
    [
      "hover on a bundled crate's method shows the crate's names",
      () => {
        const pg = "use petgraph::graph::Graph;\nfn main() {\n    let mut g: Graph<i32, ()> = Graph::new();\n    let a = g.add_node(1);\n}\n";
        return hover(pg, "pg.rs", 4, 16)?.contents;
      },
      "fn add_node(&mut self, weight: N) -> NodeIndex\n```\n*Graph*",
    ],
    [
      "signature help on a std associated function",
      () => {
        const h = signatureHelp(src.replace("    t.", "    let w = Vec::with_capacity("), "svc.rs", 8, 32);
        return h && `${h.label} @${h.active}`;
      },
      "fn with_capacity(capacity: usize) -> Vec<T> @0",
    ],
    ["hover on a Vec binding", () => hover(src, "svc.rs", 7, 9)?.contents, "v: Vec<i32>"],
    ["struct members after `.`", () => completions(src, "svc.rs", 8, 7).map((c) => c.label).join(","), "clone,host,money,score"],
    ["locals in scope", () => completions(src.replace("    t.", "    s"), "svc.rs", 8, 6).map((c) => c.label).join(","), "s"],
    [
      "signature help in a call",
      () => {
        const h = signatureHelp(src.replace("    t.", "    let q = t.score("), "svc.rs", 8, 24);
        return h && `${h.label} @${h.active}`;
      },
      "fn score(&self) -> f64 @0",
    ],
    [
      "Vec methods after `.`",
      () => completions(src.replace("    t.", "    v."), "svc.rs", 8, 7).some((c) => c.label === "binary_search"),
      true,
    ],
  ];
  // `///` doc comments show in hover.
  const documented = [
    "/// A server we can hack.",
    "struct Target {",
    "    /// Hostname, e.g. `n00dles`.",
    "    host: String,",
    "}",
    "impl Target {",
    "    /// Seconds until the next hack.",
    "    fn wait(&self) -> f64 { 1.0 }",
    "}",
    "/// Doubles a number.",
    "fn double(x: i32) -> i32 { x * 2 }",
    "fn main() {",
    "    let t = Target { host: String::new() };",
    "    let w = t.wait();",
    "    let d = double(2);",
    "    let h = t.host;",
    "}",
  ].join("\n");
  // Files with `macro_rules!`: the service works on the expansion, positions stay the user's.
  const withMacro = [
    "macro_rules! many {",
    "    ($($x:expr),*) => {{ let mut v = Vec::new(); $( v.push($x); )* v }};",
    "}",
    "fn main() {",
    "    let nums = many!(1, 2, 3, 4, 5, 6, 7, 8, 9);",
    "    let total: i32 = nums.iter().sum();",
    "    total.",
    "}",
  ].join("\n");
  checks.push(
    ["hover on a binding a macro made", () => hover(withMacro, "mac.rs", 5, 10)?.contents, "nums: Vec<i32>"],
    ["hover after a macro keeps its position", () => { const h = hover(withMacro, "mac.rs", 6, 10); return h && `${h.line}:${h.column}-${h.endColumn} ${h.contents}`; }, "6:9-14 ```rust\ntotal: i32"],
    ["completions after a macro", () => completions(withMacro, "mac.rs", 7, 11).some((c) => c.label === "checked_add"), true],
  );
  // `jsx!`: hover on tags and attributes (markup) and in embedded expressions; completions of
  // tags, a component's props and an element's attributes.
  const withJsx = [
    "use react::{jsx, Node};",
    "/// A labelled bar.",
    "fn gauge(label: &str, value: f64) -> Node {",
    "    jsx! { <b>{label} {value}</b> }",
    "}",
    "fn main() {",
    "    let hp = 0.5f64;",
    '    let n: Node = jsx! { <div class="x" on_click={|e| {}}><gauge label="hp" value={hp} /></div> };',
    "    let m: Node = jsx! { <div on_cl><gauge val /><ga </div> };",
    "    let k: Node = jsx! { <p>{hp.}</p> };",
    "}",
  ].join("\n");
  const jsxLines = withJsx.split("\n");
  // (1-based column of `text` on `line`, plus `k`.)
  const col = (line, text, k = 0) => jsxLines[line - 1].indexOf(text) + 1 + k;
  checks.push(
    ["jsx: hover on a component's tag", () => hover(withJsx, "jsx.rs", 8, col(8, "gauge", 2))?.contents, "fn gauge(label: &str, value: f64) -> Node\n```\n*component*\n\nA labelled bar."],
    ["jsx: hover on a component's prop", () => hover(withJsx, "jsx.rs", 8, col(8, "label", 1))?.contents, "label: &str\n```\n\nA parameter of the component `gauge`."],
    ["jsx: hover on an HTML attribute", () => hover(withJsx, "jsx.rs", 8, col(8, "class", 1))?.contents, "class: text\n```\n\nAn attribute of `<div>` (React's `className`)."],
    ["jsx: hover on an event handler", () => hover(withJsx, "jsx.rs", 8, col(8, "on_click", 1))?.contents, "on_click: impl Fn(js::Event)"],
    ["jsx: hover in an embedded expression", () => hover(withJsx, "jsx.rs", 8, col(8, "{hp}", 1))?.contents, "hp: f64"],
    ["jsx: hover in a component's body", () => hover(withJsx, "jsx.rs", 4, col(4, "{value}", 1))?.contents, "value: f64"],
    ["jsx: an event closure's parameter is the event", () => hover(withJsx, "jsx.rs", 8, col(8, "|e|", 1))?.contents, "e: Event"],
    ["jsx: attributes of an element", () => completions(withJsx, "jsx.rs", 9, col(9, "on_cl", 5)).map((c) => c.label).join(","), "on_click,on_close"],
    ["jsx: props of a component", () => completions(withJsx, "jsx.rs", 9, col(9, "val", 3)).map((c) => `${c.label}: ${c.detail}`).join(","), "value: f64"],
    ["jsx: tag names", () => completions(withJsx, "jsx.rs", 9, col(9, "<ga", 3)).map((c) => c.label).join(","), "gauge"],
    ["jsx: members in an embedded expression", () => completions(withJsx, "jsx.rs", 10, col(10, "hp.", 3)).some((c) => c.label === "sqrt"), true],
  );
  checks.push(
    ["doc comment of a method", () => hover(documented, "doc.rs", 14, 15)?.contents, "Seconds until the next hack."],
    ["doc comment of a called function", () => hover(documented, "doc.rs", 15, 13)?.contents, "fn double(x: i32) -> i32\n```\n\nDoubles a number."],
    ["doc comment of a declared function", () => hover(documented, "doc.rs", 11, 4)?.contents, "Doubles a number."],
    ["doc comment of a field", () => hover(documented, "doc.rs", 16, 15)?.contents, "Hostname, e.g. `n00dles`."],
  );
  for (const [label, run, want] of checks) {
    const name = `service: ${label}`;
    try {
      const got = run();
      const ok = typeof want === "string" && typeof got === "string" ? got.includes(want) : got === want;
      if (ok) pass(name);
      else fail(name, `want ${JSON.stringify(want)}\ngot  ${JSON.stringify(got)}`);
    } catch (e) {
      fail(name, e.message);
    }
  }
}

// --- language service: go to definition, references, rename
{
  const src = [
    "mod geo {",
    "    pub struct Point { pub x: f64 }",
    "    impl Point {",
    "        pub fn new(x: f64) -> Self { Point { x } }",
    "        pub fn len(&self) -> f64 { self.x }",
    "    }",
    "}",
    "#[derive(Debug)]",
    "enum Dir { Up, Down }",
    "type Num = i32;",
    "fn twice(n: Num) -> i32 { n * 2 }",
    "fn main() {",
    "    let total = twice(3);",
    "    let p = geo::Point::new(1.0);",
    "    let total = total + 1;",
    '    println!("{} {}", total, p.len());',
    "    let d = Dir::Down;",
    "    let x = p.x;",
    "    let y = twice(total);",
    "}",
  ].join("\n");
  const lines = src.split("\n");
  // 1-based column of the `nth` occurrence of `needle` on 1-based `line`.
  const col = (line, needle, nth = 0) => {
    const re = new RegExp(`\\b${needle}\\b`, "g");
    let m;
    for (let k = 0; k <= nth; k++) m = re.exec(lines[line - 1]);
    return m.index + 1;
  };
  const def = (line, needle, nth) => {
    const d = definition(src, "nav.rs", line, col(line, needle, nth));
    return d && `${d.file} ${d.line}:${d.column}-${d.endColumn}`;
  };
  const at = (line, needle, nth = 0) => `nav.rs ${line}:${col(line, needle, nth)}-${col(line, needle, nth) + needle.length}`;
  const refs = (line, needle, nth) =>
    references(src, "nav.rs", line, col(line, needle, nth)).map((l) => `${l.line}:${l.column}`).join(" ");
  const files = {
    "main.rs": "mod util;\nuse util::helper;\nfn main() {\n    helper();\n    util::Tool::make();\n}\n",
    "util.rs": "pub fn helper() {}\npub struct Tool;\nimpl Tool {\n    pub fn make() {}\n}\n",
  };
  const checks = [
    ["definition of a function", () => def(13, "twice"), at(11, "twice")],
    ["definition of a local", () => def(15, "total", 1), at(13, "total")],
    ["definition of a shadowing local in a macro", () => def(16, "total"), at(15, "total")],
    ["definition of a type in an inline module", () => def(14, "Point"), at(2, "Point")],
    ["definition of an associated function", () => def(14, "new"), at(4, "new")],
    ["definition of a method", () => def(16, "len"), at(5, "len")],
    ["definition of a field", () => def(18, "x", 1), at(2, "x")],
    ["definition of a variant", () => def(17, "Down"), at(9, "Down")],
    ["definition of a type alias", () => def(11, "Num"), at(10, "Num")],
    ["definition of a parameter", () => def(11, "n", 1), at(11, "n", 0)],
    ["definition of a struct literal's type", () => def(4, "Point"), at(2, "Point")],
    ["definition in another file", () => {
      const d = definition(files["main.rs"], "main.rs", 4, 5, (p) => files[p]);
      return d && `${d.file} ${d.line}:${d.column}-${d.endColumn}`;
    }, "util.rs 1:8-14"],
    ["definition of an associated function in another file", () => {
      const d = definition(files["main.rs"], "main.rs", 5, 17, (p) => files[p]);
      return d && `${d.file} ${d.line}:${d.column}`;
    }, "util.rs 4:12"],
    ["references of a local", () => refs(15, "total"), `15:${col(15, "total")} 16:${col(16, "total")} 19:${col(19, "total")}`],
    ["references of a function", () => refs(11, "twice"), `11:${col(11, "twice")} 13:${col(13, "twice")} 19:${col(19, "twice")}`],
    ["rename a function", () => rename(src, "nav.rs", 13, col(13, "twice"), "double").length, 3],
    ["rename refuses a bad name", () => rename(src, "nav.rs", 13, col(13, "twice"), "1x").length, 0],
    ["references across files, from a use", () => {
      const rs = references(files["main.rs"], "main.rs", 4, 5, (p) => files[p], ["main.rs", "util.rs"]);
      return rs.map((l) => `${l.file} ${l.line}:${l.column}`).join(", ");
    }, "main.rs 2:11, main.rs 4:5, util.rs 1:8"],
    ["references across files, from the definition", () => {
      const rs = references(files["util.rs"], "util.rs", 1, 8, (p) => files[p], ["main.rs", "util.rs"]);
      return rs.map((l) => `${l.file} ${l.line}:${l.column}`).join(", ");
    }, "util.rs 1:8, main.rs 2:11, main.rs 4:5"],
    ["rename across files", () => rename(files["util.rs"], "util.rs", 1, 8, "assist", (p) => files[p], ["main.rs"]).length, 3],
  ];
  for (const [label, run, want] of checks) {
    const name = `service: ${label}`;
    try {
      const got = run();
      if (got === want) pass(name);
      else fail(name, `want ${JSON.stringify(want)}\ngot  ${JSON.stringify(got)}`);
    } catch (e) {
      fail(name, e.message);
    }
  }
}

// Typed NS calls and `?`. Bodies go inside `pub async fn main(ns: NS)`, starting on line 2.
// --- warnings: programs that compile, with the warning each must produce (line:col prefix).
const warned = [
  ["loop that never awaits", "let mut n = 0;\n    loop {\n        n += 1;\n    }", "3:5 this loop never ends and never awaits"],
  ["while true that never awaits", "let mut n = 0;\n    while true {\n        n += 1;\n    }", "3:5 this loop never ends and never awaits"],
];
// --- borrow conflicts rustc rejects: errors with rustc's message and position, nothing emitted.
const borrowRejected = [
  ["a push while a reference into the Vec is used later", "let mut v = vec![1, 2, 3];\n    let first = &v[0];\n    v.push(4);\n    println!(\"{}\", first);", "4:5 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["assigning to a borrowed String", "let mut s = String::from(\"a\");\n    let r = &s;\n    s = String::from(\"b\");\n    println!(\"{} {}\", r, s);", "4:5 cannot assign to `s` because it is borrowed"],
  ["changing a Vec inside a `for` over it", "let mut v = vec![1, 2, 3];\n    for x in v.iter() {\n        v.push(*x);\n    }", "4:9 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["two `&mut` borrows at once", "let mut v = vec![1];\n    let a = &mut v;\n    let b = &mut v;\n    b.push(2);\n    a.push(3);", "4:13 cannot borrow `v` as mutable more than once at a time"],
  ["a map changed while `get`'s result is held", "let mut m = std::collections::HashMap::new();\n    m.insert(1, 2);\n    let g = m.get(&1);\n    m.insert(2, 3);\n    println!(\"{:?}\", g);", "5:5 cannot borrow `m` as mutable because it is also borrowed as immutable"],
  // Through calls (the callee's signature says what the result borrows) and down to fields.
  ["a push while a function's returned reference is used", "}\nfn first(v: &Vec<i32>) -> &i32 {\n    &v[0]\n}\nfn g() {\n    let mut v = vec![1, 2, 3];\n    let f = first(&v);\n    v.push(4);\n    println!(\"{}\", f);", "9:5 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["a `&mut self` call while a `&self` method's result is used", "}\nstruct S { name: String }\nimpl S {\n    fn name(&self) -> &str { &self.name }\n    fn set(&mut self, n: &str) { self.name = n.to_string(); }\n}\nfn g() {\n    let mut s = S { name: String::new() };\n    let r = s.name();\n    s.set(\"b\");\n    println!(\"{}\", r);", "11:5 cannot borrow `s` as mutable because it is also borrowed as immutable"],
  ["assigning a borrowed field", "}\nstruct P { a: Vec<i32>, b: i32 }\nfn g() {\n    let mut p = P { a: vec![1], b: 2 };\n    let r = &p.a;\n    p.b = 3;\n    p.a = vec![];\n    println!(\"{:?} {}\", r, p.b);", "8:5 cannot assign to `p.a` because it is borrowed"],
  ["printing a mutably borrowed field", "let mut t = (vec![1], 2);\n    let r = &mut t.0;\n    println!(\"{:?} {}\", t.0, t.1);\n    r.push(2);", "4:25 cannot borrow `t.0` as immutable because it is also borrowed as mutable"],
  ["the argument a named lifetime ties the result to", "}\nfn pick<'a>(x: &'a Vec<i32>, _y: &Vec<i32>) -> &'a i32 {\n    &x[0]\n}\nfn g() {\n    let mut v = vec![1];\n    let mut w = vec![2];\n    let p = pick(&v, &w);\n    w.push(1);\n    v.push(2);\n    println!(\"{}\", p);", "11:5 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["a constructor keeping its argument borrowed", "}\nstruct Parser<'a> { s: &'a str }\nimpl<'a> Parser<'a> {\n    fn new(s: &'a str) -> Parser<'a> { Parser { s } }\n}\nfn g() {\n    let mut text = String::from(\"a\");\n    let p = Parser::new(&text);\n    text.push('b');\n    println!(\"{}\", p.s);", "10:5 cannot borrow `text` as mutable because it is also borrowed as immutable"],
  ["a field changed in a `for` over a method's result", "}\nstruct S { v: Vec<i32>, n: i32 }\nimpl S { fn items(&self) -> &Vec<i32> { &self.v } }\nfn g() {\n    let mut s = S { v: vec![1], n: 0 };\n    for x in s.items() {\n        s.n += *x;\n    }", "8:9 cannot assign to `s.n` because it is borrowed"],
  // NLL proper (`nll.rs`): borrows handed between locals, through struct literals, closures,
  // iterator chains and loops; references that outlive what they borrow.
  ["a reference handed on to another local", "let mut v = vec![1];\n    let r = &v[0];\n    let s = r;\n    v.push(1);\n    println!(\"{}\", s);", "5:5 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["a struct literal holding a borrow", "}\nstruct H<'a> { r: &'a i32 }\nfn g() {\n    let mut v = vec![1];\n    let h = H { r: &v[0] };\n    v.push(1);\n    println!(\"{}\", h.r);", "7:5 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["a closure borrowing a Vec that is then changed", "let mut v = vec![1];\n    let c = || v.len();\n    v.push(1);\n    println!(\"{}\", c());", "4:5 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["a closure mutating a Vec that is printed meanwhile", "let mut v = vec![1];\n    let mut c = || v.push(1);\n    println!(\"{:?}\", v);\n    c();", "4:22 cannot borrow `v` as immutable because it is also borrowed as mutable"],
  ["a borrow kept across loop iterations", "let mut v = vec![1];\n    let mut last = &v[0];\n    for i in 0..3 {\n        v.push(i);\n        if i == 1 { last = &v[0]; }\n    }\n    println!(\"{}\", last);", "5:9 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["an `if let` binding used after the push", "let mut v = vec![1];\n    if let Some(x) = v.first() {\n        v.push(*x);\n        println!(\"{}\", x);\n    }", "4:9 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["a `while let` over an iterator of the Vec", "let mut v = vec![1, 2];\n    let mut it = v.iter();\n    while let Some(x) = it.next() {\n        v.push(*x);\n    }", "5:9 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["references collected through `map` into a Vec", "let mut v = vec![String::from(\"a\")];\n    let firsts: Vec<&str> = v.iter().map(|s| s.as_str()).collect();\n    v.push(String::from(\"b\"));\n    println!(\"{:?}\", firsts);", "4:5 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["an entry's slot held across an insert", "let mut m: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();\n    let e = m.entry(1).or_insert(0);\n    m.insert(2, 2);\n    *e += 1;", "4:5 cannot borrow `m` as mutable more than once at a time"],
  ["a `&mut` used after the Vec was changed through its name", "let mut v = vec![1];\n    let x = &mut v;\n    x.push(1);\n    v.push(2);\n    x.push(3);", "5:5 cannot borrow `v` as mutable more than once at a time"],
  ["a `&self` method's borrow across a `&mut` field change", "}\nstruct S { items: Vec<i32>, n: i32 }\nimpl S {\n    fn first(&self) -> &i32 { &self.items[0] }\n    fn bump(&mut self) {\n        let f = self.first();\n        self.items.push(1);\n        self.n += *f;\n    }", "8:9 cannot borrow `self.items` as mutable because it is also borrowed as immutable"],
  ["moving a borrowed String", "let s = String::from(\"a\");\n    let r = &s;\n    let t = s;\n    println!(\"{} {}\", r, t);", "4:13 cannot move out of `s` because it is borrowed"],
  ["a reference outliving its block", "let r;\n    {\n        let s = String::from(\"a\");\n        r = &s;\n    }\n    println!(\"{}\", r);", "5:13 `s` does not live long enough"],
  ["a borrow of a block's local pushed into an outer Vec", "let mut names: Vec<&str> = Vec::new();\n    {\n        let s = String::from(\"x\");\n        names.push(&s);\n    }\n    println!(\"{:?}\", names);", "5:20 `s` does not live long enough"],
  ["returning a reference to a local", "}\nfn f() -> &'static str {\n    let s = String::from(\"a\");\n    &s", "5:5 cannot return reference to local variable `s`"],
  ["returning a reference to a local, elided", "}\nfn pick(a: &str) -> &str {\n    let s = a.to_string();\n    &s", "5:5 cannot return reference to local variable `s`"],
  ["a temporary dropped while borrowed", "let r = String::from(\"a\").as_str();\n    println!(\"{}\", r);", "2:13 temporary value dropped while borrowed"],
];
for (const [label, body, want] of warned) {
  const name = `warns: ${label}`;
  const out = compile(`fn main() {\n    ${body}\n}\n`, "warn.rs");
  const msgs = out.diagnostics.filter((d) => d.severity === "warning").map((d) => `${d.line}:${d.column} ${d.message}`);
  if (out.code === null || !msgs.some((m) => m.startsWith(want))) {
    fail(name, `want: ${want}\ngot:\n${out.diagnostics.map((d) => `${d.severity} ${d.line}:${d.column} ${d.message}`).join("\n") || "(no diagnostics)"}`);
  } else pass(name);
}
for (const [label, body, want] of borrowRejected) {
  const name = `rejects: ${label}`;
  const out = compile(`fn main() {\n    ${body}\n}\n`, "reject.rs");
  const msgs = out.diagnostics.filter((d) => d.severity === "error").map((d) => `${d.line}:${d.column} ${d.message}`);
  if (out.code !== null || !msgs.some((m) => m.startsWith(want))) {
    fail(name, `want: ${want}\ngot:\n${out.diagnostics.map((d) => `${d.severity} ${d.line}:${d.column} ${d.message}`).join("\n") || "(no diagnostics)"}`);
  } else pass(name);
}
// `setStrictBorrows(false)`: the same conflict is a warning that says so, and the JS is emitted.
{
  setStrictBorrows(false);
  const out = compile("fn main() {\n    let mut v = vec![1];\n    let r = &v[0];\n    v.push(2);\n    println!(\"{}\", r);\n}\n", "lax.rs");
  setStrictBorrows(true);
  const d = out.diagnostics.find((x) => x.severity === "warning");
  // The message stays rustc's; the help says it runs anyway.
  if (
    out.code !== null &&
    !out.diagnostics.some((x) => x.severity === "error") &&
    d?.message === "cannot borrow `v` as mutable because it is also borrowed as immutable" &&
    d.help?.startsWith("`&v` lends read access") &&
    d.help.endsWith("This runs anyway here (rustc would refuse); turn on 'Rust: borrow conflicts are errors' to match rustc.")
  ) {
    pass("lax borrows: a conflict is a warning");
  } else fail("lax borrows: a conflict is a warning", JSON.stringify(out.diagnostics));
}
const quiet = [
  ["loop that awaits", "pub async fn main(ns: NS) {\n    loop {\n        ns.sleep(10.0).await;\n    }\n}\n"],
  // `&str` values for a `&str` parameter: a `&`, a literal, a slice method, a reference local, a
  // `Vec<&str>` element, an iterated item; a borrowed argument isn't moved.
  ["references for a `&str` parameter", "fn take(s: &str) -> usize { s.len() }\nfn main() {\n    let t = String::from(\"abc\");\n    let r = &t;\n    let names: Vec<&str> = vec![\"a\"];\n    let mut n = take(r) + take(\"lit\") + take(&t) + take(t.as_str()) + take(names[0]);\n    for name in names.iter() { n += take(name); }\n    n += take(&t);\n    println!(\"{} {}\", n, t);\n}\n"],
  ["loop that breaks", "fn main() {\n    let mut n = 0;\n    loop {\n        n += 1;\n        if n > 3 {\n            break;\n        }\n    }\n}\n"],
  // Borrows rustc accepts: one that ends before the change, a copy, other branches, indexing.
  ["a borrow that ends before the push", "fn main() {\n    let mut v = vec![1, 2, 3];\n    let first = &v[0];\n    println!(\"{}\", first);\n    v.push(4);\n    println!(\"{:?}\", v);\n}\n"],
  ["a copied element", "fn main() {\n    let mut v = vec![1, 2, 3];\n    let first = v[0];\n    v.push(4);\n    println!(\"{} {:?}\", first, v);\n}\n"],
  ["a borrow used only in the other branch", "fn main() {\n    let mut v = vec![1, 2];\n    let r = &v;\n    if v.len() > 5 {\n        v.push(1);\n    } else {\n        println!(\"{:?}\", r);\n    }\n}\n"],
  // Calls, fields and leaving a loop: only what rustc rejects.
  ["disjoint fields", "fn main() {\n    let mut p = (vec![1], vec![2]);\n    let r = &p.0;\n    p.1.push(3);\n    let m = &mut p.1;\n    println!(\"{:?}\", p.0);\n    m.push(4);\n    println!(\"{:?} {:?}\", r, p.1);\n}\n"],
  ["a method of your own named `get` that returns a copy", "struct S { a: Vec<i32> }\nimpl S {\n    fn get(&self, i: usize) -> i32 { self.a[i] }\n    fn add(&mut self, x: i32) { self.a.push(x); }\n}\nfn main() {\n    let mut s = S { a: vec![1] };\n    let g = s.get(0);\n    s.add(2);\n    println!(\"{} {:?}\", g, s.a);\n}\n"],
  ["a change right before leaving the loop", "fn main() {\n    let mut v = vec![1, 2, 3];\n    for x in v.iter() {\n        if *x == 2 {\n            v.push(9);\n            break;\n        }\n    }\n    let r = &v[0];\n    if v.len() > 1 {\n        v.push(1);\n        return;\n    }\n    println!(\"{} {:?}\", r, v);\n}\n"],
  ["an argument the result's lifetime isn't tied to", "fn pick<'a>(x: &'a Vec<i32>, _y: &Vec<i32>) -> &'a i32 {\n    &x[0]\n}\nfn main() {\n    let v = vec![1];\n    let mut w = vec![2];\n    let p = pick(&v, &w);\n    w.push(1);\n    println!(\"{} {:?}\", p, w);\n}\n"],
  ["an iterator shadowing the Vec it borrows", "fn main() {\n    let v = vec![1, 2];\n    let mut v = v.iter();\n    let a = v.next();\n    println!(\"{:?} {:?}\", a, v.next());\n}\n"],
  ["a loop over indices that pushes", "fn main() {\n    let mut v = vec![1, 2, 3];\n    for i in 0..v.len() {\n        let x = v[i];\n        v.push(x);\n    }\n    println!(\"{}\", v.len());\n}\n"],
  // NLL: two-phase borrows, borrows dead before the change, closures and iterators that don't
  // keep borrowing, disjoint fields through `self`.
  ["a two-phase borrow", "fn main() {\n    let mut v = vec![1];\n    v.push(v.len());\n    let mut s = String::new();\n    s.push_str(&s.clone());\n    println!(\"{:?} {}\", v, s);\n}\n"],
  ["a borrow overwritten before its next use in a loop", "fn main() {\n    let mut v = vec![1];\n    let mut last = &v[0];\n    for i in 0..3 {\n        v.push(i);\n        last = &v[0];\n    }\n    println!(\"{}\", last);\n}\n"],
  ["an `if let` binding whose last use is the push's argument", "fn main() {\n    let mut v = vec![Some(1), None];\n    if let Some(Some(x)) = v.first() {\n        v.push(Some(*x));\n    }\n    if let Some(x) = v.first() {\n        println!(\"{:?}\", x);\n    }\n    v.push(None);\n}\n"],
  ["a collected Vec of owned values", "fn main() {\n    let mut v = vec![1];\n    let x = v.iter().map(|x| x * 2).collect::<Vec<_>>();\n    let n = v.first().copied();\n    let total: i32 = v.iter().sum();\n    v.push(total);\n    println!(\"{:?} {:?} {:?}\", x, n, v);\n}\n"],
  ["a closure called before the Vec changes", "fn main() {\n    let mut v = vec![1];\n    let c = || v.len();\n    println!(\"{}\", c());\n    v.push(1);\n    let min = 1;\n    v.retain(|x| *x >= min);\n    println!(\"{:?}\", v);\n}\n"],
  ["an entry slot released before the insert", "use std::collections::HashMap;\nfn main() {\n    let mut m: HashMap<&str, i32> = HashMap::new();\n    *m.entry(\"a\").or_insert(0) += 1;\n    m.insert(\"b\", 2);\n    let keys: Vec<_> = m.keys().cloned().collect();\n    for k in keys { m.remove(k); }\n    println!(\"{}\", m.len());\n}\n"],
  ["a `while let` iterator done before the push", "fn main() {\n    let mut v = vec![1, 2];\n    let mut sum = 0;\n    let mut it = v.iter();\n    while let Some(x) = it.next() { sum += x; }\n    v.push(sum);\n    for x in v.iter_mut() { *x += 1; }\n    v.push(1);\n    println!(\"{:?}\", v);\n}\n"],
  ["disjoint fields through `self` and `split_at_mut`", "struct S { a: Vec<i32>, b: Vec<i32>, count: i32 }\nimpl S {\n    fn f(&mut self) {\n        let p = &mut self.a;\n        let q = &mut self.b;\n        p.push(1);\n        q.push(2);\n        self.a.push(self.count);\n    }\n}\nfn main() {\n    let mut s = S { a: vec![], b: vec![], count: 3 };\n    s.f();\n    let mut v = vec![1, 2];\n    let (a, b) = v.split_at_mut(1);\n    a[0] += b[0];\n    println!(\"{:?} {:?} {:?}\", s.a, s.b, v);\n}\n"],
  ["`take` and a `match` on a field that assigns it", "struct Node { next: Option<Box<Node>>, v: i32 }\nstruct List { head: Option<Box<Node>> }\nimpl List {\n    fn pop(&mut self) -> Option<i32> {\n        if let Some(n) = self.head.take() {\n            self.head = n.next;\n            return Some(n.v);\n        }\n        None\n    }\n}\nenum St { A, B }\nstruct M { state: St, n: i32 }\nimpl M {\n    fn step(&mut self) {\n        match self.state {\n            St::A => self.state = St::B,\n            St::B => self.n += 1,\n        }\n    }\n}\nfn main() {\n    let mut l = List { head: Some(Box::new(Node { next: None, v: 1 })) };\n    let mut m = M { state: St::A, n: 0 };\n    m.step();\n    m.step();\n    println!(\"{:?} {}\", l.pop(), m.n);\n}\n"],
  ["references returned from parameters and `impl Iterator`", "fn evens(v: &[i32]) -> impl Iterator<Item = &i32> + '_ { v.iter().filter(|x| *x % 2 == 0) }\nfn longest<'a>(a: &'a str, b: &'a str) -> &'a str { if a.len() > b.len() { a } else { b } }\nfn get<'a>(pairs: &'a [(&str, &str)], key: &str) -> Option<&'a str> {\n    pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)\n}\nfn main() {\n    let v = vec![1, 2];\n    let pairs = [(\"a\", \"1\")];\n    println!(\"{:?} {} {:?}\", evens(&v).collect::<Vec<_>>(), longest(\"ab\", \"c\"), get(&pairs, \"a\"));\n}\n"],
  ["an index assignment reading the same Vec", "fn main() {\n    let mut nums = vec![vec![1, 2, 3], vec![4, 5]];\n    nums[0][2] = nums[1][1];\n    let mut grid = vec![vec![0; 3]; 3];\n    grid[1][2] = grid[0][0] + 1;\n    let row = &grid[1];\n    let total: usize = row.iter().sum();\n    grid[0][0] = total;\n    println!(\"{:?} {:?}\", nums, grid);\n}\n"],
  ["`mem::swap` and `mem::take`", "fn main() {\n    let mut a = vec![1];\n    let mut b = vec![2];\n    std::mem::swap(&mut a, &mut b);\n    let t = std::mem::take(&mut a);\n    println!(\"{:?} {:?} {:?}\", a, b, t);\n}\n"],
  ["format arguments evaluated before the captures are borrowed", "fn bump(v: &mut Vec<i32>) -> usize { v.push(0); v.len() }\nfn main() {\n    let mut v = vec![1];\n    println!(\"{} {v:?}\", bump(&mut v));\n}\n"],
];
for (const [label, src] of quiet) {
  const name = `no warning: ${label}`;
  const out = compile(src, "quiet.rs");
  if (out.diagnostics.length) fail(name, out.diagnostics.map((d) => `${d.severity} ${d.line}:${d.column} ${d.message}`).join("\n"));
  else pass(name);
}

// Cargo's build variables get placeholders (a script is crate `script`, version 0.1.0).
{
  const name = "`env!` / `option_env!` of Cargo's variables";
  const src = 'fn main() {\n    let v: &str = env!("CARGO_PKG_VERSION");\n    println!("{} {} {:?} {:?}", env!("CARGO_PKG_NAME"), v, option_env!("CARGO_MANIFEST_DIR"), option_env!("CARGO_NOPE"));\n}\n';
  const out = compile(src, "cargo_env.rs");
  if (out.code === null) fail(name, out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`).join("\n"));
  else {
    const jsPath = path.join(outDir, "cargo_env.mjs");
    fs.writeFileSync(jsPath, out.code);
    const js = await runJs(jsPath);
    const want = 'script 0.1.0 Some(".") None\n';
    if (js.stdout !== want) fail(name, `want ${want}got ${js.stdout}${js.panic ?? ""}`);
    else pass(name);
  }
}

const rejectedNs = [
  ["`.await` in a closure", "let f = || ns.sleep(10).await;", "2:16 `await` is only allowed inside `async` functions and blocks"],
  ["an owned String for an NS `&str` parameter", "let target = ns.args[0].as_string().unwrap();\n    ns.nuke(target);", "3:13 mismatched types: expected `&str`, found `String` (consider borrowing here: `&target`)", "`&` in a parameter type means the function borrows the value instead of taking it; add `&` in front of the argument."],
  ["an NS result's element for an NS `&str` parameter", "let servers = ns.scan(\"home\");\n    ns.nuke(servers[0]);", "3:13 mismatched types: expected `&str`, found `String` (consider borrowing here: `&servers[0]`)"],
  [
    "forgotten .await",
    'let stolen: f64 = ns.hack("n00dles");',
    "2:23 mismatched types: expected `f64`, found `impl Future<Output = f64>` (did you forget `.await`?)",
    "An NS call returns a future, not its value, until it is awaited (like a Promise).",
  ],
  // The mistakes a JavaScript author makes first: a value in a function with no `-> T` (the `()`
  // comes from the missing return type), and the stray `;` that makes a function return nothing.
  [
    "a value in a function with no return type",
    'fn name() { String::from("x") }',
    "2:13 mismatched types: expected `()`, found `String` (`name` has no return type: add `-> String` after the parameters, or end the line with `;`)",
    "A function without `-> Type` returns nothing, and its last expression without `;` is its return value.",
  ],
  [
    "`return` in a function with no return type",
    'fn name() { return String::from("x"); }',
    "2:20 mismatched types: expected `()`, found `String` (`name` has no return type: add `-> String` after the parameters, or end the line with `;`)",
    "A function without `-> Type` returns nothing",
  ],
  [
    "a number in a function with no return type",
    "fn name() { 1 }",
    "2:13 mismatched types: expected `()`, found `{integer}` (`name` has no return type: add `-> i32` after the parameters, or end the line with `;`)",
    "A function without `-> Type` returns nothing",
  ],
  [
    "a stray `;` before the closing brace",
    'fn name() -> String { String::from("x"); }',
    "2:23 mismatched types: expected `String`, found `()` (the last line ends with `;`, so the function returns nothing; remove the `;` to return its value)",
    "A function's value is its last expression written without `;` (`return x;` also works).",
  ],
  [
    "a wrong tail type with a declared return type keeps rustc's message alone",
    "fn name() -> String { 1 }",
    "2:23 mismatched types: expected `String`, found `{integer}`",
    null,
  ],
  [
    "returning a reference to a local",
    "fn f() -> &String { let s = String::new(); &s }",
    "2:44 cannot return reference to local variable `s`",
    "A reference points at a variable that is deleted when the function ends.",
  ],
  [
    "an argument count error stays bare",
    "fn f(a: i32) {}\nfn g() { f(1, 2); }",
    "3:10 this function takes 1 argument but 2 were supplied",
    null,
  ],
  [
    "JavaScript's `length` on an NS result",
    'let servers = ns.scan("home");\n    let n = servers.length;',
    "3:13 no field `length` on type `Vec<String>` (`length` is `.len()` in Rust)",
    "Rust's std names differ from JavaScript's; docs/FEATURES.md lists what is supported.",
  ],
  [
    "typo'd NS method",
    'let m = ns.get_server_max_mony("n00dles");',
    "2:13 no method named `get_server_max_mony` found for `NS`; did you mean `get_server_max_money`?",
  ],
  ["wrong argument type", "let m = ns.get_server_max_money(5);", "2:37 mismatched types: expected `String`, found `{integer}`"],
  [
    "too many arguments",
    'let m = ns.get_server_max_money("a", "b");',
    "2:13 `NS::get_server_max_money` takes 0 to 1 arguments but 2 were supplied",
  ],
  [
    "NS numbers are f64",
    'let ports: u32 = ns.get_server_num_ports_required("a");',
    "2:22 mismatched types: expected `u32`, found `f64` (NS numbers are `f64`; convert with `as`)",
  ],
  [
    "camelCase field name",
    'let s = ns.get_server("a");\n    let r = s.maxRam;',
    "3:13 no field `maxRam` on type `ServerWithIsOnline`; did you mean `max_ram`?",
  ],
  // A `JsValue` is read with `as_string()` / `as_f64()`, not taken as a `String` or `f64`.
  ["a JsValue argument as a String", "let a: String = ns.args[0];", "2:21 mismatched types: expected `String`, found `ScriptArg`"],
  ["a port read as a number", "let n: f64 = ns.read_port(1);", "2:18 mismatched types: expected `f64`, found `JsValue`"],
  ["an unknown JsValue method", "let n = ns.args[0].as_strin();", "2:13 no method named `as_strin` found for `ScriptArg`; did you mean `as_string`?"],
  [
    "a misspelled option field",
    "let o = RunOptions { thread: Some(2.0), ..Default::default() };",
    "2:26 struct `RunOptions` has no field named `thread`; did you mean `threads`?",
  ],
  [
    "an interface option without all its fields",
    "let o = BasicHGWOptions { threads: Some(2.0) };",
    "2:13 missing fields `additional_msec` and `stock` in initializer of `BasicHGWOptions`",
  ],
  [
    "Default of an interface with required fields",
    "let s: Server = Default::default();",
    "2:21 the trait bound `Server: Default` is not satisfied",
  ],
  ["building an NS namespace", "let h = Hacknet { };", "2:13 `Hacknet` is an NS namespace, not a struct"],
  [
    "a Person where a Player is expected",
    "fn f(ns: &NS) -> Player {\n    let p: Person = ns.get_player();\n    p\n}",
    "4:5 mismatched types: expected `Player`, found `Person`",
  ],
  ["method used as a field", "let h = ns.get_hostname;", "2:13 `get_hostname` is a method on `NS`, not a field"],
  [
    "`?` outside an Option function",
    "let x = Some(1)?;",
    "2:13 the `?` operator can only be used in a function that returns `Option` (this function returns `()`)",
  ],
  ["`?` on a non-Option", "fn f() -> Option<i32> {\n    let x = 5?;\n    Some(x)\n}", "3:13 the `?` operator can only be applied to `Option`"],
];
for (const [label, body, want, wantHelp] of rejectedNs) {
  // A body starting with `fn` is a whole helper function placed before main.
  const src = body.startsWith("fn ")
    ? `pub async fn main(ns: NS) {}\n${body}\n`
    : `pub async fn main(ns: NS) {\n    ${body}\n}\n`;
  checkRejected(`rejects: ${label}`, compile(src, "bad.rs"), want, wantHelp);
}

// A mutation through `&` vs `&mut` parameters.
{
  const name = "rejects: push through a `&` reference";
  const out = compile("fn add(v: &Vec<i32>) {\n    v.push(1);\n}\nfn ok(v: &mut Vec<i32>) {\n    v.push(1);\n}\n", "bad.rs");
  const msgs = out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`);
  const want = "2:5 cannot borrow `*v` as mutable, as it is behind a `&` reference";
  if (msgs.length !== 1 || !msgs[0].startsWith(want)) fail(name, `want only: ${want}\ngot:\n${msgs.join("\n")}`);
  else pass(name);
}

// --- results kept across calls: the same text compiled (or analyzed) again after a `mod` file,
// the bindings or a setting changed must see the change.
{
  const check = (name, ok, detail) => (ok ? pass(`kept results: ${name}`) : fail(`kept results: ${name}`, detail));
  const errs = (o) => o.diagnostics.filter((d) => d.severity === "error").map((d) => `${d.line}:${d.column} ${d.message}`);
  const main = "mod util;\nfn main() {\n    let x: i32 = util::f();\n    println!(\"{} {}\", x, util::m!());\n}\n";
  const files = { "util.rs": "pub fn f() -> i32 {\n    1\n}\n#[macro_export]\nmacro_rules! m {\n    () => {\n        \"one\"\n    };\n}\n" };
  const read = (p) => files[p];
  const a = compile(main, "main.rs", read);
  const again = compile(main, "main.rs", read);
  check("the same compile twice", a.code !== null && again.code === a.code, JSON.stringify(errs(a)));
  files["util.rs"] = files["util.rs"].replace('"one"', '"two"');
  const b = compile(main, "main.rs", read);
  check("a `mod` file's macro changed", b.code !== null && b.code.includes('"two"') && !b.code.includes('"one"'), b.code ?? JSON.stringify(errs(b)));
  files["util.rs"] = files["util.rs"].replace("pub fn f() -> i32 {\n    1\n}", 'pub fn f() -> String {\n    String::new()\n}');
  const c = compile(main, "main.rs", read);
  check("a `mod` file's signature changed", c.code === null && errs(c).some((e) => e.includes("mismatched types")), JSON.stringify(errs(c)));
  const h = hover(main, "main.rs", 3, 9, read);
  check("hover after a `mod` file changed", h?.contents.includes("i32"), JSON.stringify(h));
  const h2 = hover(main.replace("let x: i32 =", "let x ="), "main.rs", 3, 9, read);
  check("hover sees the `mod` file's new type", h2?.contents.includes("String"), JSON.stringify(h2));
  files["util.rs"] = files["util.rs"].replace("pub fn f() -> String {\n    String::new()\n}", "pub fn f() -> i32 {\n    1\n}");
  const h3 = hover(main.replace("let x: i32 =", "let x ="), "main.rs", 3, 9, read);
  check("hover after the `mod` file changed back", h3?.contents.includes("i32"), JSON.stringify(h3));
  delete files["util.rs"];
  const d = compile(main, "main.rs", read);
  check("a `mod` file removed", d.code === null, d.code);

  const ovf = "fn main() {\n    let v: Vec<u8> = vec![200, 100];\n    let s: u8 = v[0] + v[1];\n    println!(\"{}\", s);\n}\n";
  const off = compile(ovf, "ovf.rs").code;
  setOverflowChecks(true);
  const on = compile(ovf, "ovf.rs").code;
  setOverflowChecks(false);
  check("overflow checks toggled", off !== null && on !== null && on !== off && compile(ovf, "ovf.rs").code === off, on);

  const conflict = "fn main() {\n    let mut v = vec![1];\n    let r = &v[0];\n    v.push(2);\n    println!(\"{}\", r);\n}\n";
  const strict = compile(conflict, "conflict.rs");
  setStrictBorrows(false);
  const lax = compile(conflict, "conflict.rs");
  setStrictBorrows(true);
  check("strict borrows toggled", strict.code === null && lax.code !== null && compile(conflict, "conflict.rs").code === null, JSON.stringify(lax.diagnostics));

  // The TS unions in the editor: `ns.args` is a `Vec<ScriptArg>`, a variant shows its payload,
  // the enum its TS union; a missing variant is reported like an enum's.
  {
    const u = "pub async fn main(ns: NS) {\n    let args = ns.args;\n    match &args[0] {\n        ScriptArg::Str(s) => ns.tprint(s),\n        ScriptArg::Num(n) => {}\n    }\n}\n";
    const h1 = hover(u, "u.rs", 2, 9)?.contents ?? "";
    const h2 = hover(u, "u.rs", 4, 21)?.contents ?? "";
    const h3 = hover(u, "u.rs", 4, 12)?.contents ?? "";
    const d = errs(compile(u, "u.rs"));
    check(
      "hover on the TS unions",
      h1.includes("args: Vec<ScriptArg>") && h2.includes("ScriptArg::Str(String)") && h3.includes("enum ScriptArg { Str(String), Num(f64), Bool(bool) }") && h3.includes("TypeScript: `string | number | boolean`"),
      JSON.stringify([h1, h2, h3]),
    );
    check("a union's missing variant", d.some((m) => m.includes("non-exhaustive patterns: `ScriptArg::Bool(_)` not covered")), JSON.stringify(d));
  }
  // (Without typed bindings an NS method's result isn't checked.)
  const ns = "pub async fn main(ns: NS) {\n    let m: String = ns.get_server_money_available(\"home\");\n    ns.tprint(&format!(\"{}\", m));\n}\n";
  const full = compile(ns, "ns.rs");
  const nsHover = hover(ns, "ns.rs", 2, 25);
  setNsBindings("");
  const bare = compile(ns, "ns.rs");
  const bareHover = hover(ns, "ns.rs", 2, 25);
  setNsBindings(fs.readFileSync(path.join(here, "../bindings/ns.rs"), "utf8"));
  const back = compile(ns, "ns.rs");
  check(
    "NS bindings replaced",
    full.code === null && bare.code !== null && back.code === null && nsHover?.contents !== bareHover?.contents,
    JSON.stringify([errs(full), errs(bare), nsHover, bareHover]),
  );
}

console.log(failures ? `\n${failures} failure(s)` : "\nall passed");
process.exit(failures ? 1 : 0);
