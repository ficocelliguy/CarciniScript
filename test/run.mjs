// Test runner.
//  - fixtures/*.rs: differential. Compiled natively with rustc and to JS; stdout and exit code
//    (101 on panic) must match.
//  - ns/*.rs: async NS scripts run against a mock `ns`; output must match ns/*.expected.
//  - Source map and diagnostics checks.
import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { initCarciniScriptSync, compile, setNsBindings } from "../index.js";
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
  ["unsupported construct", "let p = Point { x: 1 };", "2:13 struct literals is not supported yet"],
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
  ["non-exhaustive integer match", "match 5 {\n        1 => {}\n    }", "2:11 non-exhaustive patterns: `_` not covered"],
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
    'let mut m = std::collections::HashMap::new();\n    m.insert("a", 1);\n    if let Some(v) = m.get_mut("a") {\n        *v += 1;\n    }',
    "4:22 `HashMap::get_mut` on a map of `{integer}` values isn't supported",
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
  ["generic without an Fn bound", "}\nfn id<T>(x: T) -> T {\n    x", "3:6 generic type parameters is not supported yet"],
  ["mixed arithmetic", "let a: i32 = 1;\n    let b: f64 = 2.0;\n    let c = a + b;", "4:13 cannot add `f64` to `i32`"],
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

// Typed NS calls and `?`. Bodies go inside `pub async fn main(ns: NS)`, starting on line 2.
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
