// The real-world corpus survey, step 2: summarize `corpus-classify.mjs` output. Prints each file's
// verdict by kind (src / tests / benches / examples), the language gaps (files, files where it's the
// only one, crates), the crates that pass in every file, and writes a CSV next to the input.
//   node tools/corpus-report.mjs [out.jsonl]   (defaults to the classifier's default output)
import fs from "node:fs"; import os from "node:os"; import path from "node:path";

const input = process.argv[2] || path.join(os.tmpdir(), "carciniscript-corpus.jsonl");
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
  for (const e of r.errs) if (e.cat === "GENERIC" && /^no rules of/.test(e.m)) { e.cat = "LANG"; e.why = "macro_rules matching"; }
  const cats = new Set(r.errs.map(e => e.cat));
  const ext = cats.has("EXT") || cats.has("THREAD");
  r.v = r.errs.length === 0 ? "A compiles as-is"
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

const generic = new Map();
for (const r of rows.filter(r => r.v[0] === "D")) for (const w of new Set(r.errs.filter(e => e.cat === "GENERIC").map(e => e.why))) generic.set(w, (generic.get(w) || 0) + 1);
console.log("\nother errors, nothing unresolved:");
for (const [w, c] of [...generic].sort((a, b) => b[1] - a[1]).slice(0, 8)) console.log(String(c).padStart(6), w);

const by = new Map();
for (const r of rows) { const c = crateOf(r.f), e = by.get(c) || { n: 0, ok: 0 }; e.n++; if (ok(r)) e.ok++; by.set(c, e); }
const all = [...by].map(([c, e]) => ({ c, ...e, p: e.ok / e.n }));
const full = all.filter(x => x.p === 1).sort((a, b) => b.n - a.n || a.c.localeCompare(b.c));
console.log(`\ncrates: ${all.length}; language-OK in every file: ${full.length}; in 80% or more: ${all.filter(x => x.p >= 0.8).length}; under 50%: ${all.filter(x => x.p < 0.5).length}`);
console.log(full.map(x => `${x.c} (${x.n})`).join(", "));

const csv = input.replace(/\.jsonl$/, "") + ".csv";
const q = x => '"' + String(x).replace(/"/g, '""') + '"';
fs.writeFileSync(csv, "file,kind,lines,verdict,language_gaps\n" +
  rows.map(r => [r.f.split("\\").join("/"), r.k, r.lines, r.v.slice(2), r.lang.join("; ")].map(q).join(",")).join("\n") + "\n");
console.log(`\nper-file results: ${csv}`);
