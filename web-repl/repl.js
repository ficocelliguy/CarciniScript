import { initCarciniScript, compile, setNsBindings, version } from "../index.js";

const $ = (id) => document.getElementById(id);
const runtimeUrl = new URL("../runtime/rt.js", import.meta.url).href;
const runnerUrl = new URL("./runner.js", import.meta.url);
const RUN_TIMEOUT_MS = 10_000;

const PLAYGROUND = `// Edit me. Compiles as you type; Ctrl+Enter runs.
// Click a line here to highlight the JS it became (and vice versa).
fn collatz_len(mut n: u64) -> u32 {
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
    }
    steps
}

pub async fn main(ns: NS) {
    let mut best = 0;
    let mut best_n = 0;
    for n in 1..10_000u64 {
        let len = collatz_len(n);
        if len > best {
            best = len;
            best_n = n;
        }
    }
    println!("longest chain under 10000 starts at {best_n} ({best} steps)");

    let target = "n00dles";
    let threshold = ns.get_server_max_money(target) * 0.75;
    while ns.get_server_money_available(target) < threshold {
        ns.grow(target).await;
        ns.sleep(100).await;
    }
    let stolen = ns.hack(target).await;
    ns.tprint(&format!("stole \${:.2} from {target}", stolen));

    let x: i32 = i32::MAX;
    println!("{} {:?} {:>8.3}|", x + 1, 0.1 + 0.2, 3.14159);
}
`;

const UNSUPPORTED = `// Rust outside the spike's subset gets compile errors, never a crash.
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Server {
    name: String,
    max_money: f64,
}

trait Score {
    fn score(&self) -> f64;
}

impl Score for Server {
    fn score(&self) -> f64 {
        self.max_money / 2.0
    }
}

fn best<T: Score>(items: &[T]) -> Option<&T> {
    items.iter().max_by(|a, b| a.score().partial_cmp(&b.score()).unwrap())
}

fn main() {
    let servers = vec![Server { name: "a".into(), max_money: 1.0 }];
    let mut seen: HashMap<String, usize> = HashMap::new();
    match best(&servers) {
        Some(s) => println!("{:?}", s),
        None => println!("none"),
    }
    let squares: Vec<i32> = (1..10).map(|x| x * x).collect();
}
`;

// ---------------------------------------------------------------- source maps

function decodeMappings(mappings) {
  const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const out = [];
  let srcLine = 0;
  let srcCol = 0;
  mappings.split(";").forEach((line, genLine) => {
    let genCol = 0;
    for (const seg of line.split(",")) {
      if (!seg) continue;
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
      out.push({ genLine, genCol, srcLine, srcCol });
    }
  });
  return out;
}

/** Rust position (0-based) for a JS position (0-based), using the closest preceding mapping. */
function originalPosition(maps, genLine, genCol) {
  let best = null;
  for (const m of maps) {
    if (m.genLine === genLine && m.genCol <= genCol && (!best || m.genCol >= best.genCol)) best = m;
  }
  return best;
}

// ---------------------------------------------------------------- compile + run

function compileTimed(source, filename) {
  const t0 = performance.now();
  const out = compile(source, filename);
  return { ...out, ms: performance.now() - t0 };
}

/** Run compiled code in a fresh worker. Resolves with { exitCode, events }. */
function runInWorker(code, sourceMap, filename, { mock = "generic", onEvent = () => {}, timeoutMs = RUN_TIMEOUT_MS } = {}) {
  const worker = new Worker(runnerUrl, { type: "module" });
  const events = [];
  let timer;
  const promise = new Promise((resolve) => {
    const finish = (exitCode) => {
      clearTimeout(timer);
      worker.terminate();
      resolve({ exitCode, events });
    };
    worker.onmessage = ({ data }) => {
      events.push(data);
      onEvent(data);
      if (data.type === "done") finish(data.exitCode);
    };
    worker.onerror = (e) => {
      const ev = { type: "error", message: e.message || "worker failed to start" };
      events.push(ev);
      onEvent(ev);
      finish(1);
    };
    timer = setTimeout(() => {
      const ev = { type: "error", message: `stopped after ${timeoutMs / 1000}s (a loop without .await?)` };
      events.push(ev);
      onEvent(ev);
      finish(124);
    }, timeoutMs);
    worker.postMessage({ code, sourceMap, filename, runtimeUrl, mock });
  });
  return { promise, stop: () => worker.terminate() || null, worker };
}

// ---------------------------------------------------------------- UI

let rustEditor;
let jsEditor;
let current = { maps: [], code: "", filename: "main.rs" };
let running = null;
let rustDecorations = [];
let jsDecorations = [];

function setStatus(text, isError = false) {
  $("status").textContent = text;
  $("status").classList.toggle("error", isError);
}

function showTab(id) {
  document.querySelectorAll(".tabs button").forEach((b) => b.classList.toggle("active", b.dataset.tab === id));
  document.querySelectorAll(".tab-panel").forEach((p) => p.classList.toggle("active", p.id === id));
  if (id === "js") jsEditor?.layout();
}

function log(text, cls = "") {
  const span = document.createElement("span");
  if (cls) span.className = cls;
  span.textContent = text + "\n";
  $("output-log").append(span);
}

function recompile() {
  const source = rustEditor.getValue();
  const filename = current.filename;
  let out;
  try {
    out = compileTimed(source, filename);
  } catch (e) {
    setStatus(`compiler crashed: ${e.message}`, true);
    return;
  }
  const errors = out.diagnostics.filter((d) => d.severity === "error");
  const warnings = out.diagnostics.length - errors.length;
  monaco.editor.setModelMarkers(
    rustEditor.getModel(),
    "carciniscript",
    out.diagnostics.map((d) => ({
      startLineNumber: d.line,
      startColumn: d.column,
      endLineNumber: d.endLine,
      endColumn: Math.max(d.endColumn, d.column + 1),
      message: d.message,
      severity: d.severity === "error" ? monaco.MarkerSeverity.Error : monaco.MarkerSeverity.Warning,
    })),
  );
  const list = $("diagnostics");
  list.replaceChildren(
    ...out.diagnostics.map((d) => {
      const li = document.createElement("li");
      li.className = d.severity;
      li.textContent = `${d.line}:${d.column} ${d.severity}: ${d.message}`;
      li.onclick = () => {
        rustEditor.setPosition({ lineNumber: d.line, column: d.column });
        rustEditor.revealLineInCenter(d.line);
        rustEditor.focus();
      };
      return li;
    }),
  );
  $("diag-summary").textContent = out.diagnostics.length
    ? `${errors.length} error(s), ${warnings} warning(s)`
    : "no diagnostics";
  if (out.code !== null) {
    current = { ...current, code: out.code, sourceMap: out.sourceMap, maps: decodeMappings(JSON.parse(out.sourceMap).mappings) };
    jsEditor.setValue(out.code);
    $("map-hint").textContent = `${current.maps.length} mappings`;
  } else {
    current = { ...current, code: "", sourceMap: null, maps: [] };
  }
  $("run").disabled = out.code === null || running !== null;
  setStatus(`compiled ${source.length} chars in ${out.ms.toFixed(1)} ms · v${version()}`);
}

/**
 * 0-based lines covered by an editor's selection. Selecting whole lines (triple-click, clicking
 * line numbers, Shift+Down) leaves the selection ending at column 1 of the *next* line, which
 * must not count, or the highlight lands one line late.
 */
function selectedLines(editor) {
  const sel = editor.getSelection();
  let end = sel.endLineNumber;
  if (end > sel.startLineNumber && sel.endColumn === 1) end--;
  const lines = [];
  for (let l = sel.startLineNumber; l <= end; l++) lines.push(l - 1);
  return { lines, empty: sel.isEmpty(), pos: editor.getPosition() };
}

const wholeLines = (lines) =>
  lines.map((l) => ({ range: new monaco.Range(l + 1, 1, l + 1, 1), options: { isWholeLine: true, className: "map-highlight" } }));

function highlightFromRust() {
  const { lines } = selectedLines(rustEditor);
  const genLines = [...new Set(current.maps.filter((m) => lines.includes(m.srcLine)).map((m) => m.genLine))];
  jsDecorations = jsEditor.deltaDecorations(jsDecorations, wholeLines(genLines));
  if (genLines.length) jsEditor.revealLineInCenterIfOutsideViewport(Math.min(...genLines) + 1);
  rustDecorations = rustEditor.deltaDecorations(rustDecorations, []);
}

function highlightFromJs() {
  const { lines, empty, pos } = selectedLines(jsEditor);
  let srcLines;
  if (empty) {
    // A caret: the mapping in effect at the caret, or the line's first mapping when the caret is
    // in the indentation before it.
    const onLine = current.maps.filter((m) => m.genLine === pos.lineNumber - 1);
    const m = originalPosition(current.maps, pos.lineNumber - 1, pos.column - 1) ?? onLine[0];
    srcLines = m ? [m.srcLine] : [];
    if (m) $("map-hint").textContent = `JS ${pos.lineNumber}:${pos.column} → ${current.filename}:${m.srcLine + 1}:${m.srcCol + 1}`;
  } else {
    srcLines = [...new Set(current.maps.filter((m) => lines.includes(m.genLine)).map((m) => m.srcLine))];
  }
  rustDecorations = rustEditor.deltaDecorations(rustDecorations, wholeLines(srcLines));
  if (srcLines.length) rustEditor.revealLineInCenterIfOutsideViewport(Math.min(...srcLines) + 1);
  jsDecorations = jsEditor.deltaDecorations(jsDecorations, []);
}

/** Rewrite `blob:...:line:col` frames in a stack trace to Rust positions via the source map. */
function mapStack(stack, maps = current.maps, filename = current.filename) {
  return stack
    .split("\n")
    .map((line) => {
      const m = /(?:carciniscript:\/\/\/[^\s:)]+|blob:[^\s)]+?):(\d+):(\d+)/.exec(line);
      if (!m) return null;
      const pos = originalPosition(maps, Number(m[1]) - 1, Number(m[2]) - 1);
      return pos ? `    at ${filename}:${pos.srcLine + 1}:${pos.srcCol + 1}` : null;
    })
    .filter(Boolean)
    .join("\n");
}

async function run() {
  if (!current.code || running) return;
  showTab("output");
  $("output-log").replaceChildren();
  log(`running ${current.filename}…`, "line-meta");
  $("run").disabled = true;
  $("stop").disabled = false;
  const t0 = performance.now();
  running = runInWorker(current.code, current.sourceMap, current.filename, {
    onEvent: (ev) => {
      if (ev.type === "out") log(ev.text);
      else if (ev.type === "err") log(ev.text, "line-err");
      else if (ev.type === "ns") log(ev.text, "line-ns");
      else if (ev.type === "call") log(`  ${ev.text}`, "line-call");
      else if (ev.type === "panic") log(`thread 'main' panicked at ${ev.location}:\n${ev.message}`, "line-err");
      else if (ev.type === "error") {
        log(`error: ${ev.message}`, "line-err");
        const mapped = ev.stack ? mapStack(ev.stack) : "";
        if (mapped) log(mapped, "line-err");
      }
    },
  });
  const { exitCode } = await running.promise;
  log(`exited with ${exitCode} after ${(performance.now() - t0).toFixed(0)} ms`, "line-meta");
  running = null;
  $("stop").disabled = true;
  $("run").disabled = !current.code;
}

function stop() {
  if (!running) return;
  running.worker.terminate();
  running.worker.onmessage?.({ data: { type: "error", message: "stopped" } });
  running.worker.onmessage?.({ data: { type: "done", exitCode: 130 } });
}

// ---------------------------------------------------------------- examples

let manifest = [];

async function loadExample(value) {
  let source;
  let filename;
  if (value === "playground") [source, filename] = [PLAYGROUND, "main.rs"];
  else if (value === "unsupported") [source, filename] = [UNSUPPORTED, "unsupported.rs"];
  else {
    filename = value.split("/").pop();
    source = await (await fetch(`../test/${value}`)).text();
  }
  current.filename = filename;
  $("filename").textContent = filename;
  rustEditor.setValue(source);
}

async function populateExamples() {
  manifest = await (await fetch("../test/fixtures/manifest.json")).json();
  const options = [
    ["playground", "Playground"],
    ["unsupported", "Unsupported features (diagnostics)"],
    ["ns/hack.rs", "ns/hack.rs"],
    ...manifest.map((m) => [`fixtures/${m.file}`, `fixtures/${m.file}`]),
  ];
  $("examples").replaceChildren(
    ...options.map(([value, label]) => Object.assign(document.createElement("option"), { value, textContent: label })),
  );
}

// ---------------------------------------------------------------- self-test

async function selfTest() {
  showTab("selftest-panel");
  $("selftest").disabled = true;
  const results = [];
  const panel = $("selftest-results");
  const render = (doneText) => {
    const passed = results.filter((r) => r.ok).length;
    const rows = results
      .map(
        (r) =>
          `<tr><td class="${r.ok ? "pass" : "fail"}">${r.ok ? "PASS" : "FAIL"}</td><td>${r.name}</td><td>${r.detail ?? ""}${
            r.pre ? `<pre>${r.pre.replace(/</g, "&lt;")}</pre>` : ""
          }</td></tr>`,
      )
      .join("");
    panel.innerHTML = `<p class="summary">${doneText ?? "running…"} <span class="${passed === results.length ? "pass" : "fail"}">${passed}/${results.length} passed</span></p>
      <table><tr><th></th><th>check</th><th>detail</th></tr>${rows}</table>`;
  };
  const add = (r) => {
    results.push(r);
    render();
  };
  const t0 = performance.now();

  // 1. Differential: browser-transpiled output vs recorded native rustc output.
  for (const m of manifest) {
    const src = await (await fetch(`../test/fixtures/${m.file}`)).text();
    const out = compileTimed(src, m.file);
    if (!out.code) {
      add({ ok: false, name: `rustc vs browser: ${m.file}`, detail: "compile errors", pre: out.diagnostics.map((d) => `${d.line}:${d.column} ${d.message}`).join("\n") });
      continue;
    }
    const { exitCode, events } = await runInWorker(out.code, out.sourceMap, m.file).promise;
    const stdout = events.filter((e) => e.type === "out").map((e) => e.text + "\n").join("");
    const p = events.find((e) => e.type === "panic");
    const panic = p ? `thread 'main' panicked at ${p.location}:
${p.message}` : null;
    const ok = stdout === m.stdout && exitCode === m.exitCode && panic === m.panic;
    let pre;
    if (!ok) {
      const a = m.stdout.split("\n");
      const b = stdout.split("\n");
      pre = a.flatMap((l, i) => (l === b[i] ? [] : [`line ${i + 1}: rustc=${JSON.stringify(l)} browser=${JSON.stringify(b[i])}`])).join("\n");
      pre += `\nexit rustc=${m.exitCode} browser=${exitCode}`;
    }
    add({ ok, name: `rustc vs browser: ${m.file}`, detail: `${m.stdout.split("\n").length - 1} lines, exit ${exitCode}, compiled in ${out.ms.toFixed(1)} ms`, pre });
  }

  // 2. NS script against the scripted mock.
  {
    const src = await (await fetch("../test/ns/hack.rs")).text();
    const expected = (await (await fetch("../test/ns/hack.expected")).text()).replace(/\r\n/g, "\n");
    const out = compileTimed(src, "hack.rs");
    const { events } = await runInWorker(out.code, out.sourceMap, "hack.rs", { mock: "hack-fixture" }).promise;
    const got = events.filter((e) => e.type === "ns").map((e) => e.text + "\n").join("");
    const ramVisible = ["ns.getServerMaxMoney(", "await ns.weaken(", "await ns.hack("].every((s) => out.code.includes(s));
    add({ ok: got === expected && ramVisible, name: "NS script: ns/hack.rs", detail: ramVisible ? "output matches; ns.* calls visible to RAM calc" : "ns.* calls not literal", pre: got === expected ? undefined : got });
  }

  // 3. Source map: JS positions map back to the right Rust line and column.
  {
    const src = await (await fetch("../test/fixtures/control.rs")).text();
    const out = compileTimed(src, "control.rs");
    const maps = decodeMappings(JSON.parse(out.sourceMap).mappings);
    const gen = out.code.split("\n");
    const rs = src.split(/\r?\n/);
    const gl = gen.findIndex((l) => l.includes("return fib("));
    const sl = rs.findIndex((l) => l.includes("fib(n - 1) + fib(n - 2)"));
    const pos = originalPosition(maps, gl, gen[gl].lastIndexOf("fib("));
    const ok = pos && pos.srcLine === sl && pos.srcCol === rs[sl].lastIndexOf("fib(");
    add({ ok, name: "source map: control.rs", detail: ok ? `JS ${gl + 1}:${gen[gl].lastIndexOf("fib(") + 1} → control.rs:${sl + 1}:${pos.srcCol + 1}` : JSON.stringify(pos) });
  }

  // 4. Runtime errors map back to Rust through the stack trace.
  {
    // The generic mock's getServer() returns 0, so `.hostname` is undefined and `.len()` throws.
    const src =
      'pub async fn main(ns: NS) {\n    let x = 1;\n    let s = ns.get_server("nowhere");\n    ns.tprint(&format!("{x}"));\n    let n = s.hostname.len();\n    ns.tprint(&format!("{n}"));\n}\n';
    const out = compileTimed(src, "boom.rs");
    const maps = decodeMappings(JSON.parse(out.sourceMap).mappings);
    const { events } = await runInWorker(out.code, out.sourceMap, "boom.rs").promise;
    const err = events.find((e) => e.type === "error");
    const mapped = err ? mapStack(err.stack, maps, "boom.rs") : "";
    add({ ok: mapped.includes("boom.rs:5:"), name: "runtime error → Rust location", detail: err ? `${err.message}` : "no error raised", pre: mapped || err?.stack });
  }

  // 5. Robustness: arbitrary Rust must produce diagnostics, never trap the wasm.
  {
    const files = ["lib.rs", "codegen.rs", "typeck.rs", "macros.rs", "bindings.rs", "fmt.rs", "ty.rs", "sourcemap.rs", "lineindex.rs"];
    let compiled = 0;
    let bytes = 0;
    const tStart = performance.now();
    let crash;
    for (const f of files) {
      const src = await (await fetch(`../compiler/src/${f}`)).text();
      bytes += src.length;
      try {
        compile(src, f);
        compiled++;
      } catch (e) {
        crash = `${f}: ${e.message}`;
      }
    }
    add({ ok: !crash, name: "compiler compiles its own source (unsupported Rust)", detail: crash ?? `${compiled} files, ${(bytes / 1024).toFixed(0)} KB in ${(performance.now() - tStart).toFixed(0)} ms, all rejected cleanly with diagnostics` });

    let prefixes = 0;
    crash = undefined;
    const pStart = performance.now();
    for (const m of [...manifest.map((m) => `fixtures/${m.file}`), "ns/hack.rs"]) {
      const src = await (await fetch(`../test/${m}`)).text();
      for (let i = 0; i <= src.length && !crash; i++) {
        try {
          compile(src.slice(0, i), "prefix.rs");
          prefixes++;
        } catch (e) {
          crash = `${m} prefix of ${i} chars: ${e.message}`;
        }
      }
    }
    add({ ok: !crash, name: "every prefix of every fixture", detail: crash ?? `${prefixes} truncated programs in ${(performance.now() - pStart).toFixed(0)} ms, no traps` });

    let fuzz = 0;
    crash = undefined;
    const tokens = ["fn", "let", "mut", "if", "else", "while", "loop", "for", "in", "..", "..=", "{", "}", "(", ")", "[", "]", ";", ",", "x", "y", "1", "2.5", "\"s\"", "'c'", "+", "-", "*", "/", "%", "<<", ">>", "&", "|", "^", "!", "==", "<", "=", "+=", "as", "u8", "i32", "f64", "return", "break", "continue", "'a", ":", "->", "println!", "format!", "panic!", "\"{}\"", ".await", ".", "ns", "async", "pub", "match", "=>", "_", "::", "#", "struct", "impl", "Some", "None", "@", "if let", "while let", ".0", ".1", "HashSet::new()", "..", "(a, b)", "HashMap::new()", ".entry(k)", ".or_insert(0)", "*", "|x|", "||", "move", ".iter()", ".map(|x| x)", ".filter(|&x| x)", ".collect()", ".sum::<i32>()", "impl Fn(i32)", "Ordering::Less", ".cmp(&y)", ".chars()", ".rev()", "dyn"];
    let seed = 12345;
    const rand = (n) => ((seed = (Math.imul(seed, 1103515245) + 12345) >>> 0) % n);
    const fStart = performance.now();
    for (let i = 0; i < 3000 && !crash; i++) {
      const len = 5 + rand(60);
      const src = Array.from({ length: len }, () => tokens[rand(tokens.length)]).join(" ");
      try {
        compile(src, "fuzz.rs");
        fuzz++;
      } catch (e) {
        crash = `${e.message}\n${src}`;
      }
    }
    add({ ok: !crash, name: "random token soup", detail: crash ? "trap" : `${fuzz} programs in ${(performance.now() - fStart).toFixed(0)} ms, no traps`, pre: crash });
  }

  render(`done in ${((performance.now() - t0) / 1000).toFixed(1)} s.`);
  $("selftest").disabled = false;
  return results;
}

// ---------------------------------------------------------------- boot

async function boot() {
  const t0 = performance.now();
  await initCarciniScript();
  // Typed NS bindings, generated from Bitburner's NetscriptDefinitions.d.ts (tools/dts2rs.mjs).
  const bindingProblems = setNsBindings(await (await fetch("../bindings/ns.rs")).text());
  if (bindingProblems.length) console.warn("NS bindings:", bindingProblems);
  const initMs = performance.now() - t0;

  const base = "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/";
  window.MonacoEnvironment = {
    getWorkerUrl: () =>
      `data:text/javascript;charset=utf-8,${encodeURIComponent(
        `self.MonacoEnvironment = { baseUrl: "${base}" }; importScripts("${base}vs/base/worker/workerMain.js");`,
      )}`,
  };
  require.config({ paths: { vs: `${base}vs` } });
  await new Promise((resolve) => require(["vs/editor/editor.main"], resolve));

  const common = { theme: "vs-dark", automaticLayout: true, minimap: { enabled: false }, fontSize: 13, scrollBeyondLastLine: false };
  rustEditor = monaco.editor.create($("rust-editor"), { ...common, language: "rust", value: "" });
  jsEditor = monaco.editor.create($("js-editor"), { ...common, language: "javascript", readOnly: true, value: "" });

  let timer;
  rustEditor.onDidChangeModelContent(() => {
    clearTimeout(timer);
    timer = setTimeout(recompile, 150);
  });
  rustEditor.onDidChangeCursorSelection(highlightFromRust);
  jsEditor.onDidChangeCursorSelection(highlightFromJs);
  rustEditor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, run);

  document.querySelectorAll(".tabs button").forEach((b) => (b.onclick = () => showTab(b.dataset.tab)));
  $("run").onclick = run;
  $("stop").onclick = stop;
  $("selftest").onclick = selfTest;
  $("examples").onchange = (e) => loadExample(e.target.value);

  await populateExamples();
  await loadExample("playground");
  recompile();
  setStatus(`${$("status").textContent} · wasm init ${initMs.toFixed(0)} ms`);
  window.carcini = { compile, selfTest, run, editor: rustEditor, js: jsEditor };
}

boot().catch((e) => {
  console.error(e);
  setStatus(`failed to start: ${e.message}`, true);
});
