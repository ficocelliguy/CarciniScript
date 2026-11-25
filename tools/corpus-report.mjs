// The real-world corpus survey, step 3: summarize `corpus-classify.mjs` output, with the syntax
// `compiler/examples/corpus_ast.rs` adds when it's there. Prints each file's verdict by kind
// (src / tests / benches / examples), the language gaps (files, files where it's the only one,
// crates), every syntactic feature with its use and whether its probe compiles, what's left
// uncategorized, and the crates that pass in every file; writes a CSV next to the input.
//   node tools/corpus-report.mjs [in.jsonl]   (defaults to the AST step's output, else the
//   classifier's, in the OS temp dir)
import fs from "node:fs"; import os from "node:os"; import path from "node:path";

const astOut = path.join(os.tmpdir(), "carciniscript-corpus.ast.jsonl");
const input = process.argv[2] || (fs.existsSync(astOut) ? astOut : path.join(os.tmpdir(), "carciniscript-corpus.jsonl"));
const probeFile = input + ".probes.json";
const probes = new Map(fs.existsSync(probeFile) ? JSON.parse(fs.readFileSync(probeFile, "utf8")).map(p => [p.id, p]) : []);
const missing = id => probes.get(id) && !probes.get(id).ok;
console.log(`reading ${input}${probes.size ? ` (${probes.size} feature probes)` : " (no syntax: run the corpus_ast step for features)"}`);
// Parser and compile-fail fixtures aren't meant to compile.
const FIXTURE = /test_data|[\\/]ui[\\/]|compile[-_]fail|[\\/]trybuild|[\\/]tests[\\/]ui/;
const KR = /[\/\\](tests?|benches|examples)[\/\\]/;
const kind = f => KR.test(f) ? f.match(KR)[1].replace(/^test$/, "tests") : "src";
const crateOf = f => f.split(/[\\/]/)[0];
// Gaps folded into one row, and those not planned (JS values have no memory layout).
const norm = w => w.startsWith("unsafe/memory") ? "unsafe/memory" : w.startsWith("generic bound on std trait") ? "generic bound on a std trait" : w.startsWith("item:") ? "other item kind" : w;
const NOT_PLANNED = new Set(["unsafe/memory", "union", "extern block / FFI", "extern block items", "proc macro API outside proc-macro crate"]);

const rows = [];
let fixtures = 0;
for (const line of fs.readFileSync(input, "utf8").split("\n").filter(Boolean)) {
  const r = JSON.parse(line);
  if (FIXTURE.test(r.f)) { fixtures++; continue; }
  for (const e of r.errs) {
    if (e.cat === "GENERIC" && /^no rules of/.test(e.m)) { e.cat = "LANG"; e.why = "macro_rules matching"; }
    if (e.cat !== "GENERIC") continue;
    // With the syntax: an error in a construct naming something from another crate or file (or
    // a type the compiler couldn't resolve) is downstream of that name; one in a feature whose
    // probe fails is that feature's gap.
    if (/defined multiple times|^duplicate definitions/.test(e.m) && e.node?.cfg) { e.cat = "LANG"; e.why = "same item under alternative #[cfg]s"; }
    else if (e.node?.feat && missing(e.node.feat)) { e.cat = "LANG"; e.why = "feature: " + probes.get(e.node.feat).label; }
    else if (e.ext?.length || e.ctx_ext?.length || /\{unknown\}/.test(e.m)) { e.cat = "EXT"; e.why = "downstream of an external name"; }
  }
  const cats = new Set(r.errs.map(e => e.cat));
  const ext = cats.has("EXT") || cats.has("THREAD");
  r.v = r.panic ? "G compiler panic"
    : r.errs.length === 0 ? "A compiles as-is"
    : cats.has("LANG") ? "E language gap"
    : cats.has("PARSE") ? "F parse error"
    : cats.has("GENERIC") && !ext ? "D other errors, nothing unresolved"
    : cats.has("STD") ? "C std-library gap only"
    : "B only threads/external names";
  r.k = kind(r.f);
  r.lang = [...new Set(r.errs.filter(e => e.cat === "LANG").map(e => norm(e.why)))];
  rows.push(r);
}

const kinds = ["all", "src", "tests", "benches", "examples"];
const n = (v, k) => rows.filter(r => (!v || r.v === v) && (k === "all" || r.k === k)).length;
const pct = (a, b) => (100 * a / b).toFixed(1) + "%";
console.log(`${rows.length} files (${fixtures} fixture files excluded)`);
console.log("".padEnd(36) + kinds.map(k => k.padStart(9)).join(""));
for (const v of [...new Set(rows.map(r => r.v))].sort()) console.log(v.padEnd(36) + kinds.map(k => String(n(v, k)).padStart(9)).join(""));
const ok = r => "ABC".includes(r.v[0]);
const lines = f => rows.filter(f).reduce((s, r) => s + r.lines, 0);
console.log(`language-OK: ${rows.filter(ok).length} = ${pct(rows.filter(ok).length, rows.length)}; counting std gaps: ${pct(n("A compiles as-is", "all") + n("B only threads/external names", "all"), rows.length)}; by lines: ${pct(lines(ok), lines(() => true))}`);

const gaps = new Map();
for (const r of rows) for (const w of r.lang) {
  const g = gaps.get(w) || { files: 0, solo: 0, crates: new Set() };
  g.files++; if (r.lang.length === 1) g.solo++; g.crates.add(crateOf(r.f));
  gaps.set(w, g);
}
console.log("\nlanguage gaps: files, files where it's the only one, crates");
for (const [w, g] of [...gaps].sort((a, b) => b[1].files - a[1].files))
  console.log(`${String(g.files).padStart(6)}${String(g.solo).padStart(6)}${String(g.crates.size).padStart(6)}  ${w}${NOT_PLANNED.has(w) ? " (not planned)" : ""}`);

// Every syntactic feature: files and crates using it, files where an uncategorized error lands in
// it, and whether its probe compiles.
if (probes.size) {
  const use = new Map(), landed = new Map();
  for (const r of rows) {
    for (const id of Object.keys(r.feats || {})) {
      const u = use.get(id) || { files: 0, crates: new Set() };
      u.files++; u.crates.add(crateOf(r.f)); use.set(id, u);
    }
    for (const id of new Set(r.errs.filter(e => e.node?.feat && (e.cat === "GENERIC" || e.why.startsWith("feature:"))).map(e => e.node.feat)))
      landed.set(id, (landed.get(id) || 0) + 1);
  }
  const row = id => { const p = probes.get(id), u = use.get(id) || { files: 0, crates: new Set() };
    return `${String(u.files).padStart(6)}${String(u.crates.size).padStart(6)}${String(landed.get(id) || 0).padStart(7)}  ${p.label}`; };
  const order = [...probes.keys()].sort((a, b) => (use.get(b)?.files || 0) - (use.get(a)?.files || 0));
  console.log("\nmissing features (probe fails): files using it, crates, files with errors in it");
  for (const id of order.filter(id => missing(id) && !probes.get(id).not_planned)) console.log(`${row(id)}\n${"".padEnd(21)}probe: ${probes.get(id).error}`);
  console.log("\nnot planned:");
  for (const id of order.filter(id => probes.get(id).not_planned)) console.log(row(id));
  console.log("\nsupported features (probe compiles) where errors still land: files using it, crates, files with errors in it");
  for (const id of order.filter(id => !missing(id) && !probes.get(id).not_planned && landed.get(id))) console.log(row(id));
  console.log("\nsupported features, no errors landing in them:");
  console.log(order.filter(id => !missing(id) && !probes.get(id).not_planned && !landed.get(id)).map(id => `${id} (${use.get(id)?.files || 0})`).join(", "));
}

// What's still uncategorized (no unresolved names involved), by message and the construct it's in.
const left = new Map();
for (const r of rows) for (const e of r.errs.filter(e => e.cat === "GENERIC")) {
  const k = `${e.why}  @ ${e.node ? e.node.feat || e.node.kinds[0] : "?"}`;
  const g = left.get(k) || { files: new Set(), ex: `${r.f}:${e.line} ${e.node?.text || ""}` };
  g.files.add(r.f); left.set(k, g);
}
console.log(`\nuncategorized errors with nothing unresolved, by message @ construct (${[...left.values()].reduce((s, g) => s + g.files.size, 0)} file hits):`);
for (const [k, g] of [...left].sort((a, b) => b[1].files.size - a[1].files.size).slice(0, 30))
  console.log(`${String(g.files.size).padStart(6)}  ${k}\n${"".padEnd(10)}e.g. ${g.ex.slice(0, 140)}`);

const by = new Map();
for (const r of rows) { const c = crateOf(r.f), e = by.get(c) || { n: 0, ok: 0 }; e.n++; if (ok(r)) e.ok++; by.set(c, e); }
const all = [...by].map(([c, e]) => ({ c, ...e, p: e.ok / e.n }));
const full = all.filter(x => x.p === 1).sort((a, b) => b.n - a.n || a.c.localeCompare(b.c));
console.log(`\ncrates: ${all.length}; language-OK in every file: ${full.length}; in 80% or more: ${all.filter(x => x.p >= 0.8).length}; under 50%: ${all.filter(x => x.p < 0.5).length}`);
console.log(full.map(x => `${x.c} (${x.n})`).join(", "));

// The compiler's slow files (fixtures included): by time, and by time per line against the
// median (counting the module files a file reads), which puts the superlinear ones first. Their
// profiles are the single-thread lever.
const timed = [];
for (const line of fs.readFileSync(input, "utf8").split("\n").filter(Boolean)) {
  const r = JSON.parse(line);
  if (r.ms !== undefined) timed.push({ f: r.f, ms: r.ms, per: r.ms / Math.max(r.lines + (r.modLines || 0), 1) });
}
if (timed.length) {
  const median = [...timed].sort((a, b) => a.per - b.per)[timed.length >> 1].per;
  const show = r => `${String(r.ms).padStart(9)} ms ${(r.per / median).toFixed(0).padStart(6)}x  ${r.f}`;
  const total = timed.reduce((s, r) => s + r.ms, 0);
  console.log(`\ncompile time: ${(total / 1000).toFixed(0)} s over ${timed.length} files, median ${(median * 1000).toFixed(0)} ms per 1,000 lines; slowest (ms, per line against the median):`);
  for (const r of [...timed].sort((a, b) => b.ms - a.ms).slice(0, 10)) console.log(show(r));
  console.log("slowest per line, of files over a second:");
  for (const r of timed.filter(r => r.ms >= 1000).sort((a, b) => b.per - a.per).slice(0, 10)) console.log(show(r));
}

const csv = input.replace(/\.jsonl$/, "") + ".csv";
const q = x => '"' + String(x).replace(/"/g, '""') + '"';
fs.writeFileSync(csv, "file,kind,lines,ms,verdict,language_gaps\n" +
  rows.map(r => [r.f.split("\\").join("/"), r.k, r.lines, r.ms ?? "", r.v.slice(2), r.lang.join("; ")].map(q).join(",")).join("\n") + "\n");
console.log(`\nper-file results: ${csv}`);
