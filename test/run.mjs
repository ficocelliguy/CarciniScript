// Test runner.
//  - fixtures/*.rs: differential. Compiled natively with rustc and to JS; stdout and exit code
//    (101 on panic) must match.
//  - ns/*.rs: async NS scripts run against a mock `ns`; output must match ns/*.expected.
//  - Source map and diagnostics checks.
import { execFileSync, spawnSync } from "node:child_process";
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
import { nativePanic } from "../tools/native-panic.mjs";

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
    } else throw e;
  }
  κσ.$flush();
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
      execFileSync("rustc", ["--edition", "2024", "-A", "warnings", "-o", exe, path.join(here, "debug", file)], { stdio: "pipe" });
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
    async hack(t) {
      log.push(`hack ${t}`);
      return 1234.5678;
    },
    tprint: (s) => log.push(s),
    getHostname: () => "home",
    getServer: (host) => ({
      hostname: host,
      maxRam: 32,
      ramUsed: 4.5,
      cpuCores: 2,
      moneyMax: host === "home" ? undefined : 1750000,
    }),
    hacknet: { numNodes: () => 3 },
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

// --- diagnostics: programs rustc rejects, and the error each must produce (line:col prefix).
// The `fn main` wrapper is added; bodies start on line 2.
const rejected = [
  ["assign to immutable", "let x = 1;\n    x = 2;", "3:5 cannot assign twice to immutable variable `x`"],
  ["unsupported construct", "let v = vec![1];\n    let r = v.as_ptr();", "3:13 no method named `as_ptr`"],
  [
    "element type inferred from a later push",
    "let mut v = Vec::new();\n    v.push(1u8);\n    v.push(\"x\");",
    "4:12 mismatched types: expected `u8`, found `String`",
  ],
  ["mutating an immutable Vec", "let v = vec![1];\n    v.push(2);", "3:5 cannot borrow `v` as mutable, as it is not declared as mutable"],
  ["`&mut` of an immutable binding", "let v = vec![1];\n    let r = &mut v;", "3:18 cannot borrow `v` as mutable"],
  ["indexing with i32", "let v = vec![1];\n    let i: i32 = 0;\n    let x = v[i];", "4:15 the type `[{integer}]` cannot be indexed by `i32`"],
  ["Display of a Vec", "let v = vec![1];\n    println!(\"{}\", v);", "3:14 `Vec<{integer}>` doesn't implement `std::fmt::Display`"],
  ["integer literal as f64", "let x: f64 = 1;", "2:18 mismatched types: expected `f64`, found `{integer}`"],
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
  // No fixed column: reported at the first expression whose type is ambiguous.
  ["nested Option", "let x: Option<Option<i32>> = None;", "`Option<Option<i32>>` is not supported yet"],
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
  [
    "get_mut on number values",
    'let mut m = std::collections::HashMap::new();\n    m.insert("a", 1);\n    let o = m.get_mut("a");\n    if let Some(v) = o {\n        *v += 1;\n    }',
    "4:13 `HashMap::get_mut` on a map of `{integer}` values isn't supported here",
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
  ["map over iter_mut", "let mut v = vec![1, 2];\n    let w: Vec<i32> = v.iter_mut().map(|x| *x).collect();", "`iter_mut()` followed by `.map()` isn't supported yet"],
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
  ["trait impls", "}\nstruct Q;\nimpl std::hash::Hash for Q { fn hash<H: std::hash::Hasher>(&self, h: &mut H) {} }\nfn f() {", "4:6 `impl Hash for ..` is not supported yet"],
  ["`<` on a type parameter without PartialOrd", "}\nfn big<T>(a: T, b: T) -> bool {\n    a > b", "4:5 comparing with `<` on type parameter `T` needs a bound: add `T: PartialOrd`"],
  ["generic call with an unsatisfied bound", "}\nfn show<T: std::fmt::Display>(x: T) {}\nfn f() {\n    show(vec![1]);", "5:5 the trait bound `Vec<i32>: Display` is not satisfied"],
  ["division on a type parameter", "}\nfn half<T: Copy>(x: T, y: T) -> T {\n    x / y", "4:5 `/` on type parameter `T` isn't supported"],
  ["unsupported trait bound", "}\nfn f<T: Into<f64>>(x: T) {", "3:9 the trait bound `Into` is not supported yet on type parameters"],
  ["method outside a type parameter's bounds", "}\nfn f<T>(x: T) -> usize {\n    x.len()", "4:5 no method named `len` found for type parameter `T`"],
  ["mutable string slice", "let mut s = String::from(\"abc\");\n    let t = &mut s[1..];", "3:13 mutable string slices (`&mut s[a..b]`) are not supported"],
  ["trait impl missing a method", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nimpl T for S {}\nfn f() {", "5:12 not all trait items implemented, missing: `a`"],
  ["method that isn't in the trait", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nimpl T for S { fn a(&self) -> i32 { 0 } fn z(&self) {} }\nfn f() {", "5:44 method `z` is not a member of trait `T`"],
  ["generic call with an unimplemented user trait", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nfn g<X: T>(x: X) -> i32 { x.a() }\nfn f() {\n    g(5);", "7:5 the trait bound `i32: T` is not satisfied"],
  ["unknown method on dyn Trait", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nimpl T for S { fn a(&self) -> i32 { 0 } }\nfn f() {\n    let d: Box<dyn T> = Box::new(S);\n    d.c();", "8:5 no method named `c` found for `dyn T`"],
  ["Box<dyn Trait> of a type without the impl", "}\ntrait T { fn a(&self) -> i32; fn b(&self) -> i32 { 1 } }\nstruct S;\nfn f() {\n    let d: Box<dyn T> = Box::new(S);", "6:34 the trait bound `S: T` is not satisfied"],
  ["`T::make()` on a method's type parameter", "}\ntrait M { fn make() -> Self; }\nstruct S;\nimpl S {\n    fn g<T: M>(&self) -> T {\n        T::make()\n    }\n}\nfn f() {", "7:9 `T::..` on a type parameter of a method isn't supported yet"],
  ["calling a function the bound doesn't have", "}\ntrait M { fn make() -> Self; }\nfn g<T: M>() -> T {\n    T::build()\n}\nfn f() {", "5:5 no function or associated item named `build` found for type parameter `T`"],
  ["non-exhaustive slice match names the missing length", "let v = vec![1];\n    match v.as_slice() { [] => {}, [x] => {} }", "3:11 non-exhaustive patterns: `[_, _, ..]` not covered"],
  ["break with a value from a while loop", "while true { break 5; }", "2:18 `break` with value from a `while` or `for` loop"],
  ["break outside a loop", "break;", "2:5 `break` outside of a loop"],
  ["loop breaking with different types", "let x = loop { if true { break 1; } break \"s\"; };", "2:47 mismatched types: expected `{integer}`, found `String`"],
  ["`&mut` to a number field of a call's result", "}\nstruct P { x: i32 }\nfn make() -> P { P { x: 1 } }\nfn bump(v: &mut i32) { *v += 1; }\nfn f() {\n    bump(&mut make().x);", "7:15 passing a `&mut` to a number, string or bool inside a struct, tuple or Vec isn't supported yet"],
  ["keeping a `&mut` to an `Option` field in a variable", "}\nstruct T { root: Option<i32> }\nfn f() {\n    let mut t = T { root: None };\n    let cur = &mut t.root;\n    *cur = Some(1);", "6:15 keeping a `&mut` to a `Option<i32>` inside a struct, tuple or Vec in a variable isn't supported yet"],
  ["two nested functions with the same name", "}\nfn b() { fn h() {} }\nfn c() { fn h() {} }\nfn d() {", "4:13 the name `h` is defined multiple times"],
  ["wrong type into a Stack<i32>", "}\nstruct Stack<T> { items: Vec<T> }\nimpl<T> Stack<T> { fn new() -> Self { Stack { items: Vec::new() } } fn push(&mut self, x: T) { self.items.push(x); } }\nfn f() {\n    let mut s: Stack<i32> = Stack::new();\n    s.push(\"x\");", "7:12 mismatched types: expected `i32`, found `String`"],
  ["a generic struct's type inferred from first use", "}\nstruct Stack<T> { items: Vec<T> }\nimpl<T> Stack<T> { fn new() -> Self { Stack { items: Vec::new() } } fn push(&mut self, x: T) { self.items.push(x); } }\nfn f() {\n    let mut s = Stack::new();\n    s.push(1);\n    s.push(2.5);", "8:12 mismatched types: expected `{integer}`, found `{float}`"],
  ["use after `let b = a`", "let a = vec![1];\n    let b = a;\n    println!(\"{:?} {:?}\", a, b);", "4:27 use of moved value: `a` (moved on line 3:13 by `let`"],
  ["use after passing to a function", "}\nfn take(v: Vec<i32>) -> usize { v.len() }\nfn f() {\n    let v = vec![1];\n    take(v);\n    v.len();", "7:5 use of moved value: `v` (moved on line 6:10 as an argument"],
  ["use after a `for` loop", "let v = vec![1, 2];\n    for x in v { println!(\"{x}\"); }\n    v.len();", "4:5 use of moved value: `v` (moved on line 3:14 by a `for` loop"],
  ["use after `into_iter()`", "let v = vec![1];\n    let it = v.into_iter();\n    v.len();", "4:5 use of moved value: `v` (moved on line 3:14 by a method that takes `self`"],
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
  ["`get_mut` on a Vec of numbers, not matched or unwrapped", "let mut v = vec![1, 2];\n    let o = v.get_mut(0);\n    if let Some(x) = o {\n        *x += 1;\n    }", "3:13 `Vec::get_mut` on a Vec of `{integer}` isn't supported here"],
  ["`impl PartialOrd` without `PartialEq`", "}\nstruct T(i32);\nimpl PartialOrd for T {\n    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> { self.0.partial_cmp(&o.0) }\n}\nfn f() {", "4:1 the trait bound `T: PartialEq` is not satisfied (`PartialOrd` requires it"],
  ["a macro call no rule matches", "}\nmacro_rules! twice {\n    ($x:expr) => { $x + $x };\n}\nfn f() {\n    let a = twice!();", "7:13 no rules of `macro_rules! twice` matched this invocation"],
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
];
for (const [label, body, want] of rejected) {
  const name = `rejects: ${label}`;
  const out = compile(`fn main() {\n    ${body}\n}\n`, "bad.rs");
  const msgs = out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`);
  const matches = (m) => (/^\d/.test(want) ? m.startsWith(want) : m.includes(want));
  if (out.code !== null || !msgs.some(matches)) {
    fail(name, `want: ${want}\ngot:\n${msgs.join("\n") || "(no diagnostics)"}`);
  } else pass(name);
}

// Modules: each case is a set of files; the first is compiled.
const rejectedFiles = [
  [
    "importing a missing item through a nested module file",
    { "main.rs": "mod geo;\nuse geo::shapes::nope;\nfn main() {}", "geo.rs": "pub mod shapes;", "geo/shapes.rs": "pub fn area() {}" },
    "2:5 unresolved import `geo::shapes::nope` (it doesn't exist or isn't `pub`)",
  ],
  ["module file not found", { "main.rs": "mod nope;\nfn main() {}" }, "1:1 file not found for module `nope` (looked for `nope.rs` and `nope/mod.rs`)"],
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
  ["crates other than std", { "main.rs": "use rand::Rng;\nfn main() {}" }, "1:5 unresolved import `rand`"],
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
    ["hover on a method", () => hover(src, "svc.rs", 6, 15)?.contents, "fn score() -> f64"],
    ["hover on a Vec binding", () => hover(src, "svc.rs", 7, 9)?.contents, "v: Vec<i32>"],
    ["struct members after `.`", () => completions(src, "svc.rs", 8, 7).map((c) => c.label).join(","), "clone,host,money,score"],
    ["locals in scope", () => completions(src.replace("    t.", "    s"), "svc.rs", 8, 6).map((c) => c.label).join(","), "s"],
    [
      "signature help in a call",
      () => {
        const h = signatureHelp(src.replace("    t.", "    let q = t.score("), "svc.rs", 8, 24);
        return h && `${h.label} @${h.active}`;
      },
      "fn score() -> f64 @0",
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
  // Borrow conflicts rustc rejects (the JS still runs): rustc's message and position.
  ["a push while a reference into the Vec is used later", "let mut v = vec![1, 2, 3];\n    let first = &v[0];\n    v.push(4);\n    println!(\"{}\", first);", "4:5 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["assigning to a borrowed String", "let mut s = String::from(\"a\");\n    let r = &s;\n    s = String::from(\"b\");\n    println!(\"{} {}\", r, s);", "4:5 cannot assign to `s` because it is borrowed"],
  ["changing a Vec inside a `for` over it", "let mut v = vec![1, 2, 3];\n    for x in v.iter() {\n        v.push(*x);\n    }", "4:9 cannot borrow `v` as mutable because it is also borrowed as immutable"],
  ["two `&mut` borrows at once", "let mut v = vec![1];\n    let a = &mut v;\n    let b = &mut v;\n    b.push(2);\n    a.push(3);", "4:13 cannot borrow `v` as mutable more than once at a time"],
  ["a map changed while `get`'s result is held", "let mut m = std::collections::HashMap::new();\n    m.insert(1, 2);\n    let g = m.get(&1);\n    m.insert(2, 3);\n    println!(\"{:?}\", g);", "5:5 cannot borrow `m` as mutable because it is also borrowed as immutable"],
];
for (const [label, body, want] of warned) {
  const name = `warns: ${label}`;
  const out = compile(`fn main() {\n    ${body}\n}\n`, "warn.rs");
  const msgs = out.diagnostics.filter((d) => d.severity === "warning").map((d) => `${d.line}:${d.column} ${d.message}`);
  if (out.code === null || !msgs.some((m) => m.startsWith(want))) {
    fail(name, `want: ${want}\ngot:\n${out.diagnostics.map((d) => `${d.severity} ${d.line}:${d.column} ${d.message}`).join("\n") || "(no diagnostics)"}`);
  } else pass(name);
}
// `setStrictBorrows(true)`: the same conflict is an error, and nothing is emitted.
{
  setStrictBorrows(true);
  const out = compile("fn main() {\n    let mut v = vec![1];\n    let r = &v[0];\n    v.push(2);\n    println!(\"{}\", r);\n}\n", "strict.rs");
  setStrictBorrows(false);
  const d = out.diagnostics.find((x) => x.severity === "error");
  if (out.code === null && d?.message === "cannot borrow `v` as mutable because it is also borrowed as immutable") {
    pass("strict borrows: a conflict is an error");
  } else fail("strict borrows: a conflict is an error", JSON.stringify(out.diagnostics));
}
const quiet = [
  ["loop that awaits", "pub async fn main(ns: NS) {\n    loop {\n        ns.sleep(10.0).await;\n    }\n}\n"],
  ["loop that breaks", "fn main() {\n    let mut n = 0;\n    loop {\n        n += 1;\n        if n > 3 {\n            break;\n        }\n    }\n}\n"],
  // Borrows rustc accepts: one that ends before the change, a copy, other branches, indexing.
  ["a borrow that ends before the push", "fn main() {\n    let mut v = vec![1, 2, 3];\n    let first = &v[0];\n    println!(\"{}\", first);\n    v.push(4);\n    println!(\"{:?}\", v);\n}\n"],
  ["a copied element", "fn main() {\n    let mut v = vec![1, 2, 3];\n    let first = v[0];\n    v.push(4);\n    println!(\"{} {:?}\", first, v);\n}\n"],
  ["a borrow used only in the other branch", "fn main() {\n    let mut v = vec![1, 2];\n    let r = &v;\n    if v.len() > 5 {\n        v.push(1);\n    } else {\n        println!(\"{:?}\", r);\n    }\n}\n"],
  ["a loop over indices that pushes", "fn main() {\n    let mut v = vec![1, 2, 3];\n    for i in 0..v.len() {\n        let x = v[i];\n        v.push(x);\n    }\n    println!(\"{}\", v.len());\n}\n"],
];
for (const [label, src] of quiet) {
  const name = `no warning: ${label}`;
  const out = compile(src, "quiet.rs");
  if (out.diagnostics.length) fail(name, out.diagnostics.map((d) => `${d.severity} ${d.line}:${d.column} ${d.message}`).join("\n"));
  else pass(name);
}

const rejectedNs = [
  ["`.await` in a closure", "let f = || ns.sleep(10).await;", "2:16 `await` is only allowed inside `async` functions and blocks"],
  [
    "forgotten .await",
    'let stolen: f64 = ns.hack("n00dles");',
    "2:23 mismatched types: expected `f64`, found `impl Future<Output = f64>` (did you forget `.await`?)",
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
    "3:13 no field `maxRam` on type `Server`; did you mean `max_ram`?",
  ],
  ["method used as a field", "let h = ns.get_hostname;", "2:13 `get_hostname` is a method on `NS`, not a field"],
  [
    "`?` outside an Option function",
    "let x = Some(1)?;",
    "2:13 the `?` operator can only be used in a function that returns `Option` (this function returns `()`)",
  ],
  ["`?` on a non-Option", "fn f() -> Option<i32> {\n    let x = 5?;\n    Some(x)\n}", "3:13 the `?` operator can only be applied to `Option`"],
];
for (const [label, body, want] of rejectedNs) {
  const name = `rejects: ${label}`;
  // A body starting with `fn` is a whole helper function placed before main.
  const src = body.startsWith("fn ")
    ? `pub async fn main(ns: NS) {}\n${body}\n`
    : `pub async fn main(ns: NS) {\n    ${body}\n}\n`;
  const out = compile(src, "bad.rs");
  const msgs = out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`);
  if (out.code !== null || !msgs.some((m) => m.startsWith(want))) {
    fail(name, `want: ${want}\ngot:\n${msgs.join("\n") || "(no diagnostics)"}`);
  } else pass(name);
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

console.log(failures ? `\n${failures} failure(s)` : "\nall passed");
process.exit(failures ? 1 : 0);
