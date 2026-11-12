// Drives a real Chromium (Edge or Chrome, headless) over the DevTools protocol against the web REPL:
// runs the in-page self-test, compiles and runs a fresh program, and saves screenshots.
//
//   node tools/serve.mjs &   (in another terminal)
//   node tools/browser-check.mjs [url]
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const url = process.argv[2] ?? "http://127.0.0.1:8765/web-repl/";
const outDir = path.join(root, "test/out");
fs.mkdirSync(outDir, { recursive: true });

const candidates = [
  process.env.CHROME,
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
].filter(Boolean);
const exe = candidates.find((p) => fs.existsSync(p));
if (!exe) throw new Error("no Chromium browser found; set CHROME=/path/to/chrome");

const port = 9333;
const profile = fs.mkdtempSync(path.join(os.tmpdir(), "crs-browser-"));
const browser = spawn(exe, [
  "--headless=new",
  `--remote-debugging-port=${port}`,
  `--user-data-dir=${profile}`,
  "--no-first-run",
  "--window-size=1500,950",
  "about:blank",
]);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function connect() {
  for (let i = 0; i < 50; i++) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = targets.find((t) => t.type === "page");
      if (page) return page.webSocketDebuggerUrl;
    } catch {}
    await sleep(200);
  }
  throw new Error("browser did not start");
}

const ws = new WebSocket(await connect());
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let nextId = 1;
const pending = new Map();
const consoleErrors = [];
ws.addEventListener("message", ({ data }) => {
  const msg = JSON.parse(data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  } else if (msg.method === "Runtime.exceptionThrown") {
    consoleErrors.push(msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
  } else if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
    consoleErrors.push(msg.params.args.map((a) => a.value ?? a.description).join(" "));
  }
});
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, (msg) => (msg.error ? reject(new Error(`${method}: ${msg.error.message}`)) : resolve(msg.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
async function evaluate(expression) {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
  return r.result.value;
}
async function waitFor(expression, timeoutMs = 30_000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (await evaluate(expression).catch(() => false)) return;
    await sleep(100);
  }
  throw new Error(`timed out waiting for ${expression}`);
}
async function screenshot(name) {
  const { data } = await send("Page.captureScreenshot", { format: "png" });
  const file = path.join(outDir, name);
  fs.writeFileSync(file, Buffer.from(data, "base64"));
  return file;
}

let failed = false;
const report = (ok, name, detail = "") => {
  if (!ok) failed = true;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? `: ${detail}` : ""}`);
};

try {
  await send("Runtime.enable");
  await send("Page.enable");
  await send("Emulation.setDeviceMetricsOverride", { width: 1500, height: 950, deviceScaleFactor: 1, mobile: false });
  const ua = await evaluate("navigator.userAgent");
  console.log(`browser: ${ua.match(/(Edg|Chrome)\/[\d.]+/g)?.join(" ")}`);
  await send("Page.navigate", { url });
  await waitFor("window.carcini && document.getElementById('status').textContent.startsWith('compiled')");
  report(true, "REPL loaded", await evaluate("document.getElementById('status').textContent"));

  // 1. Playground compiles and runs (Rust with NS calls against the mock).
  await evaluate("carcini.run()");
  await waitFor("document.getElementById('output-log').textContent.includes('exited with')");
  const playOut = await evaluate("document.getElementById('output-log').textContent");
  report(
    playOut.includes("starts at 6171 (261 steps)") && playOut.includes("-2147483648 0.30000000000000004") && playOut.includes("exited with 0"),
    "playground run",
    playOut.split("\n").filter((l) => l && !l.startsWith("  ns.")).slice(1, 5).join(" | "),
  );
  await screenshot("repl-playground.png");

  // 2. The in-page self-test (differential vs rustc, NS, source maps, robustness).
  const results = await evaluate("carcini.selfTest().then(r => r.map(x => ({ ok: x.ok, name: x.name, detail: x.detail, pre: x.pre })))");
  for (const r of results) report(r.ok, `self-test: ${r.name}`, r.ok ? r.detail : `${r.detail}\n${r.pre ?? ""}`);
  await screenshot("repl-selftest.png");

  // 3. A program the REPL has never seen, typed into the editor.
  const fresh = [
    "fn tri(n: u32) -> u32 {",
    "    let mut t = 0;",
    "    for i in 1..=n {",
    "        t += i;",
    "    }",
    "    t",
    "}",
    "",
    "fn main() {",
    "    let mut line = String::new();",
    "    let mut k = 1;",
    "    while tri(k) < 100 {",
    "        line.push_str(&format!(\"{:>4}\", tri(k)));",
    "        k += 1;",
    "    }",
    "    println!(\"{}|\", line);",
    "    let bytes: u8 = 200;",
    "    println!(\"{}\", bytes + 100);",
    "    assert_eq!(tri(4), 11, \"tri(4) should be {}\", 10);",
    "}",
  ].join("\n");
  await evaluate(`carcini.editor.setValue(${JSON.stringify(fresh)})`);
  await waitFor("carcini.js.getValue().includes('function tri')");
  await evaluate("carcini.run()");
  await waitFor("document.getElementById('output-log').textContent.includes('exited with')");
  const freshOut = await evaluate("document.getElementById('output-log').textContent");
  const expected = "   1   3   6  10  15  21  28  36  45  55  66  78  91|\n44\nthread 'main' panicked at main.rs:19:5:\nassertion `left == right` failed: tri(4) should be 10";
  report(freshOut.includes("   1   3   6  10  15  21  28  36  45  55  66  78  91|") && freshOut.includes("\n44\n") && freshOut.includes("panicked at main.rs:19:5") && freshOut.includes("exited with 101"), "fresh program typed into the editor", freshOut.split("\n").slice(1, 5).join(" | "));
  if (!freshOut.includes(expected)) console.log("     note: full output was:\n" + freshOut);
  await screenshot("repl-fresh.png");

  // 4. Unsupported Rust gets diagnostics, not a crash.
  await evaluate("document.getElementById('examples').value = 'unsupported'; document.getElementById('examples').dispatchEvent(new Event('change'))");
  await waitFor("document.getElementById('diag-summary').textContent.includes('error')");
  const diag = await evaluate("document.getElementById('diag-summary').textContent + ' / ' + document.querySelector('#diagnostics li')?.textContent");
  report(diag.includes("error"), "unsupported features → diagnostics", diag);
  await screenshot("repl-unsupported.png");

  report(consoleErrors.length === 0, "no uncaught page errors", consoleErrors.join("\n"));
} catch (e) {
  report(false, "browser check", e.message);
} finally {
  ws.close();
  browser.kill();
  await sleep(300);
  fs.rmSync(profile, { recursive: true, force: true, maxRetries: 3 });
}
process.exit(failed ? 1 : 0);
