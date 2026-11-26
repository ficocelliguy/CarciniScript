// React / `jsx!` tests (`node test/react/run.mjs`, after `npm run build`).
//  - test/react/<name>.rs with a <name>.jsx twin: both scripts run against a mock `ns`; what each
//    passes to `ns.printRaw` is rendered with React 17 (Bitburner's): static markup, then mounted
//    in jsdom, timers run for 60 ms (a fake clock), and each <button> clicked once. The two transcripts must match.
//    The twin is compiled by TypeScript (JSX → React.createElement, as for a Bitburner .jsx script).
//  - Diagnostics: `jsx!` mistakes and the error each must produce.
// React, react-dom, jsdom and TypeScript come from a Bitburner checkout (`../bitburner-src`, or
// $BITBURNER_SRC); without one, the tests are skipped.
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import { initCarciniScriptSync, compile, setNsBindings } from "../../index.js";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, "../..");
const bitburner = process.env.BITBURNER_SRC ?? path.join(root, "../bitburner-src");
if (!fs.existsSync(path.join(bitburner, "node_modules/react"))) {
  console.log(`skip react tests: no React under ${bitburner} (set BITBURNER_SRC)`);
  process.exit(0);
}
const require = createRequire(path.join(bitburner, "package.json"));
const React = require("react");
const ReactDOM = require("react-dom");
const Server = require("react-dom/server");
const { act } = require("react-dom/test-utils");
const { JSDOM } = require("jsdom");
const ts = require("typescript");

const dom = new JSDOM("<!doctype html><div id=root></div>");
globalThis.window = dom.window;
globalThis.document = dom.window.document;
globalThis.React = React;
await import("../../runtime/rt.js");

initCarciniScriptSync(fs.readFileSync(path.join(root, "pkg/carciniscript_bg.wasm")));
setNsBindings(fs.readFileSync(path.join(root, "bindings/ns.rs"), "utf8"));
const outDir = path.join(here, "../out");
fs.mkdirSync(outDir, { recursive: true });

let failures = 0;
const fail = (name, msg) => {
  failures++;
  console.log(`FAIL ${name}\n${msg}\n`);
};
const pass = (name) => console.log(`ok   ${name}`);

// A fake clock for `setInterval` / `setTimeout`, so timers fire the same way in every run.
const clock = {
  now: 0,
  next: 1,
  timers: new Map(),
  install() {
    this.now = 0;
    this.timers.clear();
    globalThis.setTimeout = (f, ms = 0) => this.add(f, ms, false);
    globalThis.setInterval = (f, ms = 0) => this.add(f, ms, true);
    globalThis.clearTimeout = globalThis.clearInterval = (id) => this.timers.delete(id);
  },
  add(f, ms, repeat) {
    const id = this.next++;
    this.timers.set(id, { f, at: this.now + Math.max(ms, 1), ms: Math.max(ms, 1), repeat });
    return id;
  },
  // Runs the timers due in the next `ms`, in order.
  advance(ms) {
    const end = this.now + ms;
    for (;;) {
      let due = null;
      for (const [id, t] of this.timers) if (t.at <= end && (!due || t.at < due[1].at)) due = [id, t];
      if (!due) break;
      const [id, t] = due;
      this.now = t.at;
      if (t.repeat) t.at += t.ms;
      else this.timers.delete(id);
      t.f();
    }
    this.now = end;
  },
};
const realTimers = [globalThis.setTimeout, globalThis.setInterval, globalThis.clearTimeout, globalThis.clearInterval];

// Runs a compiled script's `main(ns)`; renders what it printed. React's warnings count too.
async function transcript(jsPath) {
  const out = [];
  const printed = [];
  const ns = {
    args: [],
    printRaw: (n) => printed.push(n),
    tprintRaw: (n) => printed.push(n),
    print: (s) => out.push(`print ${s}`),
    disableLog() {},
    ui: { openTail() {}, resizeTail() {}, moveTail() {} },
  };
  const warn = console.error;
  clock.install();
  console.error = (...a) => out.push(`console.error ${String(a[0]).split("\n")[0]}`);
  try {
    const mod = await import(pathToFileURL(jsPath).href + `?t=${Date.now()}`);
    await mod.main(ns);
    for (const node of printed) {
      out.push(`static ${Server.renderToStaticMarkup(node)}`);
      const el = document.createElement("div");
      document.body.append(el);
      await act(async () => {
        ReactDOM.render(node, el);
      });
      await act(async () => {
        clock.advance(60);
      });
      out.push(`live ${el.innerHTML}`);
      for (const b of [...el.querySelectorAll("button")]) {
        await act(async () => {
          b.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true }));
        });
        out.push(`click ${b.textContent}: ${el.innerHTML}`);
      }
      ReactDOM.unmountComponentAtNode(el);
      el.remove();
    }
  } catch (e) {
    out.push(`error ${e.panicMessage ?? e.message}`);
  } finally {
    console.error = warn;
    [globalThis.setTimeout, globalThis.setInterval, globalThis.clearTimeout, globalThis.clearInterval] = realTimers;
  }
  return out.join("\n");
}

for (const f of fs.readdirSync(here).filter((f) => f.endsWith(".rs"))) {
  const name = `react ${f}`;
  const base = f.replace(/\.rs$/, "");
  try {
    const out = compile(fs.readFileSync(path.join(here, f), "utf8"), f);
    const errors = out.diagnostics.filter((d) => d.severity === "error");
    if (errors.length) {
      fail(name, errors.map((d) => `${f}:${d.line}:${d.column}: ${d.message}`).join("\n"));
      continue;
    }
    const jsPath = path.join(outDir, `react_${base}.mjs`);
    fs.writeFileSync(jsPath, out.code);
    const twin = path.join(here, `${base}.jsx`);
    const refPath = path.join(outDir, `react_${base}_ref.mjs`);
    const js = ts.transpileModule(fs.readFileSync(twin, "utf8"), {
      compilerOptions: { jsx: ts.JsxEmit.React, module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
    });
    fs.writeFileSync(refPath, js.outputText);
    const [got, want] = [await transcript(jsPath), await transcript(refPath)];
    if (got !== want) fail(name, `jsx twin:\n${want}\n\njsx!:\n${got}`);
    else pass(`${name} (${got.split("\n").length} renders match the .jsx)`);
    if (process.env.SHOW_RENDERS) console.log(got);
  } catch (e) {
    fail(name, e.stack);
  }
}

// --- diagnostics
// A diagnostic on one line: the headline, its label after a colon, its help lines in parentheses.
const oneLine = (d) => `${d.message}${d.label ? `: ${d.label}` : ""}${(d.helps ?? []).map((h) => ` (${h})`).join("")}`;
const prelude = "use react::{jsx, Node};\nfn gauge(label: &str, value: f64) -> Node { jsx! { <b>{label} {value}</b> } }\n";
const cases = [
  ["jsx! { <p>hello world</p> }", "text in `jsx!` is written as a string literal"],
  ["jsx! { <Gauge value={1.0} /> }", "`Gauge` isn't a fn or closure in scope"],
  ['jsx! { <gauge label="x" /> }', "`<gauge>` needs `value=..`"],
  ['jsx! { <gauge label="x" value={1.0} size={2} /> }', "`gauge` has no parameter `size` (it takes: `label`, `value`)"],
  ["jsx! { <gauge label={3} value={1.0} /> }", "mismatched types"],
  ['jsx! { <div><p>"a"</div> }', "`</div>` closes `<p>`"],
  ['jsx! { <p on_click={|e: i32| {}}>"x"</p> }', "mismatched types: expected `impl Fn(Event)`"],
  ['jsx! { <gauge label="x" value={1.0}>"kid"</gauge> }', "`gauge` takes no children"],
  ['{ let ticker = 3; jsx! { <ticker /> } }', "`ticker` is bound here, but not to a fn or closure"],
  ["jsx! { <p>{nope}</p> }", "cannot find value `nope`"],
  // Typed children, attributes and events; `ns.print_raw`'s argument.
  ["jsx! { <p>{Some(vec![true])}</p> }", "React can't show a `Option<Vec<bool>>`: a `jsx!` child is"],
  ["jsx! { <p>{(1, 2)}</p> }", "React can't show a `(i32, i32)`"],
  ["(1, 2)", "React can't show a `(i32, i32)`: this takes a `react::Node`"],
  ['jsx! { <div colour="red">"x"</div> }', "`<div>` has no attribute `colour` (did you mean `color`?)"],
  ['jsx! { <div onclick={|| {}}>"x"</div> }', "`<div>` has no attribute `onclick` (did you mean `onClick`?)"],
  ['jsx! { <div tab_indx={1}>"x"</div> }', "`<div>` has no attribute `tab_indx` (did you mean `tab_index`?)"],
  ['jsx! { <button disabled="yes">"x"</button> }', "this attribute takes a `bool`, not `String`"],
  ['jsx! { <div class={3}>"x"</div> }', "this attribute takes text (`&str` or `String`), not `i32`"],
  ['jsx! { <div style={5}>"x"</div> }', "`style` takes a CSS string"],
  ['jsx! { <div key={vec![1]}>"x"</div> }', "`key` takes text or a number, not `Vec<i32>`"],
  ['jsx! { <p on_click="x">"x"</p> }', "`on_click` takes a closure"],
  ['jsx! { <p on_click={|e: js::Event| e.nope}>"x"</p> }', "no field `nope` on type `Event`"],
  ['jsx! { <div {..3}>"x"</div> }', "`{..spread}` on an HTML element takes `(name, value)` pairs"],
  ['jsx! { <p>{..vec![("a", 1)]}</p> }', "`{..spread}` goes among an element's attributes"],
  ['{ let c = react::create_context(1); jsx! { <c.Provider>"x"</c.Provider> } }', "`<c.Provider>` needs `value=..`"],
  ['{ let c = react::create_context(1); jsx! { <c.Provider value={"a"}>"x"</c.Provider> } }', "mismatched types"],
  ["jsx! { <a.b /> }", "`<a.b>`: a dotted tag is a context's provider"],
];
for (const [body, want] of cases) {
  const name = `react rejects: ${body}`;
  const src = `${prelude}pub async fn main(ns: NS) { ns.print_raw(${body}); }\n`;
  const msgs = compile(src, "bad.rs").diagnostics.filter((d) => d.severity === "error").map(oneLine);
  if (!msgs.some((m) => m.startsWith(want))) fail(name, `want: ${want}\ngot:\n${msgs.join("\n")}`);
  else pass(name);
}

// Typed attributes and children point at the value written.
{
  const name = "react: positions of attribute and child type errors";
  const src = [
    "use react::{jsx, Node};",
    "pub async fn main(ns: NS) {",
    "    let node: Node = jsx! {",
    '        <div class={3} hidden="no">',
    "            {(1, 2)}",
    "        </div>",
    "    };",
    "    ns.print_raw(node);",
    "}",
    "",
  ].join("\n");
  const got = compile(src, "pos.rs")
    .diagnostics.filter((d) => d.severity === "error")
    .map((d) => `${d.line}:${d.column}`);
  const want = ["4:21", "4:31", "5:14"];
  if (got.join(" ") !== want.join(" ")) fail(name, `want ${want.join(" ")}\ngot  ${got.join(" ")}`);
  else pass(name);
}

// A component from a `mod` file (`use ui::Gauge;`): its parameters are read from there.
{
  const name = "react: a component from a mod file";
  const files = {
    "ui.rs": ["use react::{jsx, Node};", "pub fn Gauge(label: &str, value: f64) -> Node {", '    jsx! { <p class="g">{label} ": " {value}</p> }', "}", ""].join("\n"),
    "main.rs": [
      "mod ui;",
      "use react::jsx;",
      "use ui::Gauge;",
      "pub async fn main(ns: NS) {",
      '    ns.print_raw(jsx! { <div><Gauge label="hp" value={0.5} /></div> });',
      "}",
      "",
    ].join("\n"),
  };
  try {
    const dir = path.join(outDir, "react-mod");
    fs.mkdirSync(dir, { recursive: true });
    for (const [f, src] of Object.entries(files)) {
      const out = compile(src, f, (p) => files[p]);
      const errors = out.diagnostics.filter((d) => d.severity === "error");
      if (errors.length) throw new Error(errors.map((d) => `${f}:${d.line}:${d.column} ${d.message}`).join("\n"));
      fs.writeFileSync(path.join(dir, f.replace(/\.rs$/, ".mjs")), out.code.replace(/\.rs";/g, '.mjs";'));
    }
    const printed = [];
    const mod = await import(pathToFileURL(path.join(dir, "main.mjs")).href + `?t=${Date.now()}`);
    await mod.main({ printRaw: (n) => printed.push(n) });
    const html = Server.renderToStaticMarkup(printed[0]);
    const want = '<div><p class="g">hp: 0.5</p></div>';
    if (html !== want) fail(name, `want ${want}\ngot  ${html}`);
    else pass(name);
  } catch (e) {
    fail(name, e.message);
  }
}

// Errors in an embedded expression point into it (the markup around it is rewritten, the
// expression stays where it was written).
{
  const name = "react: error positions inside jsx!";
  const src = [
    "use react::{jsx, Node};",
    "pub async fn main(ns: NS) {",
    "    let n = 3u32;",
    "    let node: Node = jsx! {",
    "        <div>",
    '            <p>{n + "x"}</p>',
    '            <span title={undefined_thing}>"hi"</span>',
    "        </div>",
    "    };",
    "    ns.print_raw(node);",
    "}",
    "",
  ].join("\n");
  const got = compile(src, "pos.rs")
    .diagnostics.filter((d) => d.severity === "error")
    .map((d) => `${d.line}:${d.column} ${d.message}`);
  const want = ["6:19 cannot add `String` to `u32`", "7:26 cannot find value `undefined_thing` in this scope"];
  if (got.join("\n") !== want.join("\n")) fail(name, `want:\n${want.join("\n")}\ngot:\n${got.join("\n")}`);
  else pass(name);
}

console.log(failures ? `\n${failures} failure(s)` : "\nall passed");
process.exit(failures ? 1 : 0);
