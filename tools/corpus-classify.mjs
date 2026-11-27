// The real-world corpus survey (PLAN.md, "Real-world corpus gaps"), step 1: compile every .rs
// file under a root (one directory per crate, like the cargo registry) and classify each error as
// EXT (another crate or file), THREAD, STD (std-library gap), LANG (language gap), PARSE or
// GENERIC. Writes one JSON line per file; `corpus-report.mjs` summarizes them.
//   npm run corpus   (this, then the report, over the local cargo registry; takes 3-6 minutes)
//   node tools/corpus-classify.mjs [--resume] [root] [out.jsonl]
// `root` defaults to the cargo registry's source directory, `out.jsonl` to one in the OS temp dir.
// Files compile on worker threads (`CORPUS_JOBS`, default all cores but two), largest first so
// the slowest files start at once; the output is in sorted file order either way.
// A file the compiler panics on (or overflows the stack, or runs out of memory) gets a line with
// `panic` and no errors, and its worker is replaced. `--resume` keeps the files the output
// already has a line for (from an interrupted run) and compiles the rest.
import fs from "node:fs"; import os from "node:os"; import path from "node:path";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { initCarciniScriptSync, compile, setTestCodeChecks } from "../index.js";
const DEFAULT_OUT = path.join(os.tmpdir(), "carciniscript-corpus.jsonl");
// Per-file compile times (ms) from the last run, by absolute path, to order the next one.
const TIMES = path.join(os.tmpdir(), "carciniscript-corpus-ms.json");
function registryRoot() {
  const src = path.join(process.env.CARGO_HOME || path.join(os.homedir(), ".cargo"), "registry", "src");
  const dirs = fs.existsSync(src) ? fs.readdirSync(src).filter(d => d.startsWith("index.")) : [];
  if (dirs.length !== 1) throw new Error(`pass the root: found ${dirs.length} registry indexes in ${src}`);
  return path.join(src, dirs[0]);
}
const resume = process.argv.includes("--resume");
const positional = process.argv.slice(2).filter(a => a !== "--resume");
const root = isMainThread ? positional[0] || registryRoot() : workerData.root;

const set = s => new Set(s.split(/\s+/).filter(Boolean));
const THREAD = set(`Arc Weak Mutex MutexGuard RwLock RwLockReadGuard RwLockWriteGuard Condvar Barrier Once OnceLock
  OnceState JoinHandle ScopedJoinHandle Scope Thread ThreadId Builder LocalKey thread_local spawn scope park park_timeout
  unpark yield_now sleep available_parallelism current panicking mpsc Sender Receiver SyncSender channel sync_channel
  TryLockError PoisonError LockResult AtomicBool AtomicU8 AtomicU16 AtomicU32 AtomicU64 AtomicUsize AtomicI8 AtomicI16
  AtomicI32 AtomicI64 AtomicIsize AtomicPtr fence compiler_fence Exclusive ReentrantLock Send Sync`);
const STD = set(`Box Option Result String str Vec u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize f32 f64 bool char
  HashMap HashSet BTreeMap BTreeSet VecDeque BinaryHeap LinkedList Rc Cell RefCell Ref RefMut OnceCell LazyCell LazyLock
  UnsafeCell Cow Path PathBuf OsStr OsString File OpenOptions Metadata DirEntry ReadDir Permissions FileType MaybeUninit
  NonNull ManuallyDrop PhantomData PhantomPinned c_void c_char c_int c_uint c_long c_ulong c_short c_ushort c_uchar
  c_schar c_double c_float c_longlong c_ulonglong CStr CString Pin Context Poll Waker RawWaker RawWakerVTable Duration
  Instant SystemTime UNIX_EPOCH TypeId Any Layout LayoutError Wrapping Saturating NonZero NonZeroU8 NonZeroU16 NonZeroU32
  NonZeroU64 NonZeroU128 NonZeroUsize NonZeroI8 NonZeroI16 NonZeroI32 NonZeroI64 NonZeroI128 NonZeroIsize Formatter
  Arguments Alignment Range RangeInclusive RangeFrom RangeTo RangeFull RangeToInclusive Bound Chars CharIndices Bytes
  Lines Split SplitWhitespace Iter IterMut IntoIter Peekable Rev Map Filter Enumerate Zip Chain Take Skip Cloned Copied
  Utf8Error FromUtf8Error ParseIntError ParseFloatError ParseBoolError ParseCharError TryFromIntError
  TryFromSliceError Infallible Stdout Stderr Stdin StdoutLock StderrLock StdinLock BufReader BufWriter LineWriter Cursor
  Empty Sink Repeat ErrorKind SocketAddr SocketAddrV4 SocketAddrV6 IpAddr Ipv4Addr Ipv6Addr TcpStream TcpListener
  UdpSocket Shutdown Command Child ChildStdin ChildStdout ChildStderr Output ExitStatus ExitCode Stdio Hasher
  DefaultHasher RandomState BuildHasherDefault SipHasher Reverse Location PanicInfo PanicHookInfo Backtrace
  Global System Allocator GlobalAlloc AllocError Vars Args ArgsOs VarError Entry OccupiedEntry VacantEntry Drain
  Ordering Ipv6MulticastScope Termination ControlFlow Discriminant ThinBox Sum Product Step FromResidual Try Residual
  Unique Alloc Fn FnMut FnOnce Iterator IntoIterator DoubleEndedIterator ExactSizeIterator FusedIterator TrustedLen
  Extend FromIterator Read Write BufRead Seek SeekFrom IoSlice IoSliceMut Future IntoFuture Hash BuildHasher Borrow
  BorrowMut ToOwned Deref DerefMut AsRef AsMut From Into TryFrom TryInto FromStr ToString Display Debug Default Clone
  Copy Eq PartialEq Ord PartialOrd Drop Error Index IndexMut Add Sub Mul Div Rem Neg Not BitAnd BitOr BitXor Shl Shr
  AddAssign SubAssign MulAssign DivAssign RemAssign BitAndAssign BitOrAssign BitXorAssign ShlAssign ShrAssign Unpin
  UnwindSafe RefUnwindSafe Sized Pattern Searcher Float Fuse Flatten FlatMap StepBy Scan TakeWhile SkipWhile MapWhile
  Inspect Cycle Chunks ChunksExact Windows RChunks Successors FromFn Once OnceWith RepeatWith Splice ExtractIf Keys
  Values ValuesMut CStr IntErrorKind Display EscapeDefault EscapeDebug EscapeUnicode ToLowercase ToUppercase DecodeUtf16`);
// `Error` alone is almost always the crate's own error type in another module.
STD.delete("Error"); STD.delete("Float");
const STD_MACROS = set(`format_args include include_bytes include_str concat env option_env cfg compile_error asm
  global_asm stringify line column file module_path matches assert assert_eq assert_ne debug_assert debug_assert_eq
  debug_assert_ne write writeln print println eprint eprintln format panic unreachable todo unimplemented vec dbg
  is_x86_feature_detected is_aarch64_feature_detected concat_idents trace_macros log_syntax const_format_args
  addr_of addr_of_mut offset_of`);
const PRIMS = set("u8 u16 u32 u64 u128 usize i8 i16 i32 i64 i128 isize f32 f64 bool char str");
const isGenericParam = n => /^[A-Z][A-Z0-9]?$/.test(n);
const base = n => n.replace(/<.*$/s, "").replace(/^(&|\*const |\*mut |mut |dyn |impl )+/, "").split("::").pop().trim();
const firstName = m => (m.match(/`([^`]*)`/) || [])[1] || "";
const THREAD_USE = /\bstd::(thread|sync::(mpsc|atomic|Arc|Mutex|RwLock|Condvar|Barrier))|\bthread::spawn|\brayon\b|\bcrossbeam\b/;

// Types and traits each crate defines (anywhere in its files), by name. Type aliases don't count:
// `type Result<T> = std::result::Result<T, Error>` still names std's type.
const crateDefs = new Map();
function definedIn(crate) {
  let defs = crateDefs.get(crate);
  if (defs) return defs;
  defs = new Set();
  (function walk(d) {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) { if (e.name !== "target") walk(p); }
      else if (e.name.endsWith(".rs")) for (const m of fs.readFileSync(p, "utf8").matchAll(/\b(?:struct|enum|union|trait)\s+([A-Za-z_]\w*)/g)) defs.add(m[1]);
    }
  })(path.join(root, crate));
  crateDefs.set(crate, defs);
  return defs;
}
const STD_ROOT = /^(::)?(std|core|alloc)::/, LOCAL_ROOT = /^(::)?(crate|self|super)::/;
// Whether a std-named type (`Chain`, `Path`, `File`) is really the crate's own or another crate's:
// anyhow's `Chain`, `crate::File`, `syn::Path`. `p` is the name as written, maybe with a path.
function ownType(name, p, file) {
  if (LOCAL_ROOT.test(p)) return true;
  if (STD_ROOT.test(p)) return false;
  if (p.includes("::")) return !/^(::)?(std|core|alloc)\b/.test(p);
  const re = new RegExp(`\\b${name}\\b`);
  for (const u of file.uses) {
    if (!re.test(u)) continue;
    const r = u.replace(/^::/, "");
    if (STD_ROOT.test(r)) return false;
    // `use crate::Result` may be an alias of std's; an external crate's name is its own type.
    return LOCAL_ROOT.test(r) ? definedIn(file.crate).has(name) : true;
  }
  return definedIn(file.crate).has(name);
}

function classify(m, lineText, file) {
  const n = firstName(m), b = base(n);
  const thr = x => THREAD.has(x) || /^Atomic/.test(x);
  // External names: other crates, other files of this crate.
  if (/^unresolved import/.test(m)) return /std::(thread|sync)\b/.test(n) ? ["THREAD", n] : ["EXT", "import"];
  if (/^(file not found for module|circular modules|couldn't read)/.test(m)) return ["EXT", "module file"];
  if (/^`#\[derive\(/.test(m)) return ["EXT", "derive " + n];
  if (/^cannot find (function|value)/.test(m)) {
    if (thr(b)) return ["THREAD", b];
    // `u16::to_be_bytes(x)` / `usize::max_value()` is a std gap, `T::SIZE` stays generic, and a
    // trait method used as a value (`.map(Source::count)`) a language gap when the trait is here.
    const first = n.includes("::") ? n.split("::")[0] : "";
    if (PRIMS.has(first)) return ["STD", "function " + n];
    // (`T::SIZE`: the report decides from the signature whether the bound is from elsewhere.)
    if (isGenericParam(first)) return ["GENERIC", "associated item on a type parameter"];
    if (file.traits.has(first)) return ["LANG", "trait method used as a value"];
    if (/^(alloc|dealloc|realloc|alloc_zeroed|handle_alloc_error|copy_nonoverlapping|copy|from_raw_parts|from_raw_parts_mut|transmute|unreachable_unchecked|null|null_mut|write_bytes|read_volatile|write_volatile|size_of|align_of|zeroed|uninitialized|forget|catch_unwind)$/.test(b)) return ["LANG", "unsafe/memory: " + b];
    return ["EXT", "name"];
  }
  if (/^cannot find type `[^`]*` in this scope \(impls are for structs and enums/.test(m)) {
    if (thr(b)) return ["THREAD", b];
    const target = (lineText.match(/\bfor\s+(?:&\s*(?:'\w+\s+)?(?:mut\s+)?)?((?:::)?[\w:]+)/) || [])[1] || n;
    if (STD.has(b) && ownType(b, target.includes("::") ? target : n, file)) return ["EXT", "crate type named like std"];
    if (STD.has(b)) return ["LANG", "impl on a built-in/std type"];
    if (isGenericParam(b)) return ["LANG", "blanket impl<T> for T"];
    return ["EXT", "name"];
  }
  if (/^cannot find (type|struct|tuple struct|this struct)/.test(m)) return thr(b) ? ["THREAD", b] : ["EXT", "name"];
  if (/^failed to resolve: use of undeclared (type|crate or module|crate|module)/.test(m)) return thr(b) ? ["THREAD", b] : ["EXT", "name"];
  if (/^type `[^`]*` is not supported(?: yet)?/.test(m)) {
    if (thr(b)) return ["THREAD", b];
    if (/^(c_|MaybeUninit|NonNull|ManuallyDrop|UnsafeCell|PhantomPinned|Layout|CStr|CString)/.test(b)) return ["LANG", "unsafe/memory: " + b];
    if (STD.has(b) && ownType(b, n.replace(/<.*$/s, "").replace(/^(&|\*const |\*mut |mut |dyn |impl )+/, ""), file)) return ["EXT", "crate type named like std"];
    if (STD.has(b)) return ["STD", "type " + b];
    if (isGenericParam(b)) return ["LANG", "type parameter in this position"];
    return ["EXT", "type"];
  }
  // `the standard library type `std::cell::UnsafeCell<i32>`` / `function `core::ascii::escape_default``.
  if (/^the standard library \w+ `[^`]*` is not supported(?: yet)?/.test(m)) {
    if (thr(b)) return ["THREAD", b];
    if (/::(ptr|alloc|intrinsics)::|::mem::(size_of|align_of|size_of_val|align_of_val|transmute|transmute_copy|zeroed|uninitialized|forget)\b|::hint::unreachable_unchecked|^(c_|MaybeUninit|NonNull|ManuallyDrop|UnsafeCell|PhantomPinned|Layout|CStr|CString)/.test(n)) return ["LANG", "unsafe/memory: " + b];
    const kind = m.match(/^the standard library (\w+)/)[1];
    return ["STD", kind + " " + (kind === "type" ? b : n.replace(/^(std|core|alloc)::/, ""))];
  }
  if (/^the iterator adapter `\.[^`]*\(\)` is not supported(?: yet)?/.test(m)) return ["STD", "iterator adapter " + n];
  if (/^`(size_of|align_of|size_of_val|align_of_val)` of `/.test(m)) return ["LANG", "unsafe/memory: " + b];
  // `dyn Debug` / `impl Display` of a trait the checker has no model for; `for<'a> ..` bounds.
  if (/^`(dyn |impl |for<)[^`]*` types are not supported(?: yet)?/.test(m)) {
    if (/^`for</.test(m)) return ["LANG", "higher-ranked trait bounds"];
    const tr = base(n.replace(/^(dyn|impl)\s+/, "").split(/\s*\+\s*|\(/)[0]);
    if (thr(tr)) return ["THREAD", tr];
    if (STD.has(tr) || tr === "Error") return ["STD", (n.startsWith("dyn") ? "dyn " : "impl ") + tr];
    return ["EXT", "dyn/impl external trait"];
  }
  if (/^`impl [^`]* for \.\.` is not supported(?: yet)?/.test(m)) {
    const t = n.replace(/^impl /, "").replace(/ for \.\.$/, ""); const tb = base(t);
    if (thr(tb)) return ["THREAD", tb];
    if ((STD.has(tb) || tb === "Error") && ownType(tb, t.replace(/<.*$/s, ""), file)) return ["EXT", "crate trait named like std"];
    return STD.has(tb) || tb === "Error" ? ["STD", "impl " + tb] : ["EXT", "impl external trait"];
  }
  if (/^macro `[^`]*!`/.test(m)) {
    const mb = n.replace(/!$/, "").split("::").pop();
    // (`defmt::write!`: a qualified std name from another crate; the compiler keeps the path.)
    if (/::/.test(n) && !/^(::)?(std|core|alloc)::/.test(n)) return ["EXT", "macro"];
    if (mb === "thread_local") return ["THREAD", mb];
    return STD_MACROS.has(mb) ? ["STD", "macro " + mb + "!"] : ["EXT", "macro"];
  }
  if (/^this item is not supported(?: yet)?/.test(m)) {
    const t = lineText;
    if (/^(pub(\([^)]*\))?\s+)?union\b/.test(t)) return ["LANG", "union"];
    if (/^(pub(\([^)]*\))?\s+)?(unsafe\s+)?extern\b/.test(t)) return ["LANG", "extern block / FFI"];
    const mc = t.match(/^([\w:]+)!/);
    if (mc) { const mb = mc[1].split("::").pop(); if (mb === "thread_local") return ["THREAD", mb];
      return STD_MACROS.has(mb) ? ["STD", "item macro " + mb + "!"] : ["EXT", "item macro"]; }
    return ["LANG", "item: " + t.slice(0, 30)];
  }
  if (/^the trait bound `[^`]*` is not supported(?: yet)? on type parameters/.test(m)) {
    if (thr(b)) return ["THREAD", b];
    return STD.has(b) && !ownType(b, n.replace(/<.*$/s, ""), file) ? ["LANG", "generic bound on std trait " + b] : ["EXT", "bound on external trait"];
  }
  // Std library surface.
  if (/^iterator adapters like|^`for` over this iterator|^`\.entry\(\.\.\)` without|^`[^`]*` isn't supported: JS values have no reference counts/.test(m)) return ["STD", m.slice(0, 50)];
  if (/^(no method named|no function or associated item named|no variant or associated item named) `[^`]*` found for/.test(m)) {
    const t = (m.match(/found for (?:struct |enum |type parameter |type )?`([^`]*)`/) || [])[1] || "";
    const tb = base(t);
    if (thr(tb)) return ["THREAD", tb];
    const own = STD.has(tb) && /^[A-Z]/.test(tb) && ownType(tb, tb, file);
    if (STD.has(tb) && !own || /^\[|^&?\[|^\(|^&?str$/.test(t)) return ["STD", "method " + tb + "::" + n];
    // A method on the crate's own type (or a type parameter) that this file doesn't define is in
    // another file's impl, or another crate's trait.
    if (!file.fns.has(n)) return ["EXT", "method defined elsewhere"];
    return ["GENERIC", "method on unknown/external type"];
  }
  if (/^no field `[^`]*` on type `/.test(m)) {
    const t = base((m.match(/on type `([^`]*)`/) || [])[1] || "");
    if (!file.types.has(t)) return ["EXT", "field of a type defined elsewhere"];
  }
  if (/^`cfg!\(/.test(m) || /scripts run in JS, not on rustc's target/.test(m)) return ["LANG", "cfg on target"];
  // Language features.
  const lang = [
    [/raw pointer|no memory layout|no memory addresses|transmute/, "unsafe/memory: raw pointers, transmute"],
    [/^`macro_rules!` is not supported(?: yet)?/, "macro_rules in this position"],
    [/functions nested in other functions share the file's namespace/, "nested fn name shadowing"],
    [/^implementing (the generic trait )?`[^`]*` more than once/, "multiple impls of one generic trait"],
    [/only functions are supported in `[^`]*` blocks/, "extern block items"],
    [/only methods, `[^`]*` and `[^`]*` declarations are supported in traits/, "associated consts in traits"],
    [/enum discriminants must be integer literals/, "non-literal enum discriminant"],
    [/const generic parameters on traits|this const argument isn't supported|const parameters of type/, "const generics"],
    [/^renaming (crates|imported types|the imported type)/, "use/extern crate renaming"],
    [/nested module paths in/, "nested use paths"],
    [/would be indistinguishable/, "nested Option"],
    [/on a type parameter (of an|isn't supported)/, "generics in this position"],
    [/byte strings and C strings/, "byte/C string literals"],
    [/`let` (chains|expressions)/, "let chains"],
    [/followed by `[^`]*` isn't supported yet/, "iterator chain shape"],
    [/outside a `for` loop/, "iter_mut outside for"],
    [/in-place method on a range|on a Vec of `[^`]*` isn't supported here|keeping a `[^`]*` to a `[^`]*` inside|cannot write into a value of type/, "references into containers"],
    [/block expressions on the right of/, "block expression operand"],
    [/^(struct and enum patterns|constant and enum patterns|this pattern|this expression|assignment to this place|this block kind|fields of this type|`[^`]*` on this expression) is not supported/, "GENERIC:unsupported construct"],
    [/unsupported format spec|format argument must be a string literal|invalid format string/, "format strings"],
    [/proc-macro crates/, "proc macro API outside proc-macro crate"],
    [/recursion limit reached while expanding macros/, "macro expansion"],
  ];
  for (const [re, label] of lang) if (re.test(m)) return label.startsWith("GENERIC:") ? ["GENERIC", label.slice(8)] : ["LANG", label];
  // Not valid standalone Rust (or the parser rejects it).
  if ((/^expected /.test(m) && !/found|expected `[^`]*`, found/.test(m) && !/arguments?|tuple|closure/.test(m)) ||
      /^(Missing trailing|Invalid raw string|unknown literal prefix|empty character literal|character literal may only|Lifetime name|right curly brace|Unnecessary visibility|At least one trait|unmatched|unicode escape|unknown (byte|character) escape|invalid (character|start|unicode)|overlong|numeric character escape|incorrect unicode|unterminated|empty unicode|in `[^`]*`: expected)/.test(m)) return ["PARSE", m.slice(0, 40)];
  return ["GENERIC", m.replace(/`[^`]*`/g, "_").slice(0, 50)];
}

// One file's JSON line, or "" when it can't be read.
function classifyFile(f) {
  let src; try { src = fs.readFileSync(f, "utf8"); } catch { return ""; }
  const L = src.split("\n"); const dir = path.dirname(f);
  const t = performance.now();
  // (`modLines`: the lines of the module files it reads, which a `mod.rs` or `lib.rs` costs.)
  let modLines = 0;
  const r = compile(src, path.basename(f), p => { try { const s = fs.readFileSync(path.join(dir, p), "utf8"); modLines += s.split("\n").length; return s; } catch {} });
  // The compile alone (the classifying is under 0.5 % of a run), for the report's slow files.
  const ms = Math.round(performance.now() - t);
  const file = {
    crate: path.relative(root, f).split(path.sep)[0],
    uses: [...src.matchAll(/^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+([^;]+);/gm)].map(m => m[1].replace(/\s+/g, " ")),
    fns: new Set([...src.matchAll(/\bfn\s+(\w+)/g)].map(m => m[1])),
    types: new Set([...src.matchAll(/\b(?:struct|enum|union)\s+(\w+)/g)].map(m => m[1])),
    traits: new Set([...src.matchAll(/\btrait\s+(\w+)/g)].map(m => m[1])),
  };
  const errs = (r.diagnostics||[]).filter(d=>d.severity==="error").map(d => {
    let i = d.line-1, depth = 0;
    while (i < L.length) { const t = L[i].trim(); if (depth > 0 || t.startsWith("#") || t.startsWith("//") || t === "") { depth = Math.max(0, depth + (t.match(/\[/g)||[]).length - (t.match(/\]/g)||[]).length); i++; continue; } break; }
    let [cat, why] = classify(d.message, (L[i]||"").trim(), file);
    // A parse error on a line with a macro call is in that macro's expansion.
    if (cat === "PARSE" && /[\w:]+!\s*[\(\[\{]/.test(L[d.line-1] || "")) [cat, why] = ["LANG", "macro_rules expansion"];
    // (A bare "mismatched types" keeps its types in the label: `lb`, for the finer sort.)
    const lb = d.message === "mismatched types" && d.label ? d.label + (/NS numbers/.test(d.help ?? "") ? " (NS numbers)" : "") : undefined;
    return { cat, why, line: d.line, start: d.start, end: d.end, m: d.message.slice(0, 160), ...(lb ? { lb: lb.slice(0, 200) } : {}) };
  });
  const thread = THREAD_USE.test(src);
  return JSON.stringify({ f: path.relative(root, f), lines: L.length, ok: !!r.code, thread, ms, modLines, errs }) + "\n";
}

// A trap (a Rust panic is `unreachable`), a stack overflow or a worker out of memory, with the
// innermost wasm frames (node keeps the compiler's function names).
function trapMessage(e) {
  const frames = String(e?.stack || "").split("\n").filter(l => /wasm/.test(l)).slice(0, 3).map(l => l.trim().replace(/^at /, ""));
  return [`${e?.name || "Error"}: ${e?.message ?? e}`, ...frames].join(" < ").slice(0, 400);
}
// The JSON line for a file the compiler crashed on: no errors, `ok` false, and `panic`.
function panicLine(f, panic, ms) {
  let src = ""; try { src = fs.readFileSync(f, "utf8"); } catch {}
  return JSON.stringify({ f: path.relative(root, f), lines: src.split("\n").length, ok: false, thread: THREAD_USE.test(src), ms, errs: [], panic }) + "\n";
}

if (!isMainThread) {
  initCarciniScriptSync(workerData.module);
  // The survey reads only errors; test code's would come back as warnings.
  setTestCodeChecks(false);
  parentPort.on("message", ({ i, f }) => {
    const t = performance.now();
    let line, panic = false;
    // After a trap this instance's state is unusable, so the main thread replaces the worker.
    try { line = classifyFile(f); } catch (e) { line = panicLine(f, trapMessage(e), Math.round(performance.now() - t)); panic = true; }
    parentPort.postMessage({ i, line, ms: Math.round(performance.now() - t), panic });
  });
} else {
  const outFile = positional[1] || DEFAULT_OUT;
  const files = [];
  (function walk(d){ for (const e of fs.readdirSync(d,{withFileTypes:true})) { const p = path.join(d,e.name); if (e.isDirectory()) { if (e.name !== "target") walk(p); } else if (e.name.endsWith(".rs")) files.push(p); } })(root);
  files.sort();
  // `--resume`: files with a line in the output already (a panic included) aren't compiled again.
  const lines = new Array(files.length);
  if (resume && fs.existsSync(outFile)) {
    const index = new Map(files.map((f, i) => [path.relative(root, f), i]));
    for (const l of fs.readFileSync(outFile, "utf8").split("\n")) {
      let r; try { r = JSON.parse(l); } catch { continue; }
      const i = index.get(r.f); if (i !== undefined) lines[i] = l + "\n";
    }
  }
  const size = files.map(f => { try { return fs.statSync(f).size; } catch { return 0; } });
  // Slowest first, by each file's time in the last run (TIMES), else ~6 ms per KB: a few files
  // compile far slower than their size says (zerocopy's `impls.rs`, 2,400 lines, takes 7 minutes).
  let times = {}; try { times = JSON.parse(fs.readFileSync(TIMES, "utf8")); } catch {}
  const cost = files.map((f, i) => times[f] ?? size[i] * 0.006);
  const queue = files.map((_, i) => i).filter(i => lines[i] === undefined).sort((a, b) => cost[b] - cost[a]);
  const todo = queue.length, kept = files.length - todo;
  const jobs = Math.min(todo, Math.max(1, Number(process.env.CORPUS_JOBS) || os.availableParallelism() - 2));
  console.log(`classifying ${root} -> ${outFile} (${files.length} files${kept ? `, ${kept} kept from the last run` : ""}, ${jobs} workers)`);
  const module = new WebAssembly.Module(fs.readFileSync(new URL("../pkg/carciniscript_bg.wasm", import.meta.url)));
  // Lines are appended as files finish, so an interrupted run can `--resume`, and the output is
  // rewritten in file order at the end (diffable run to run).
  fs.writeFileSync(outFile, lines.filter(Boolean).join(""));
  const out = fs.openSync(outFile, "a");
  const ran = [], panics = [];
  let done = 0, active = 0, stopped = false;
  const start = Date.now(), secs = () => ((Date.now() - start) / 1000).toFixed(0);
  const progress = setInterval(() => console.log(`${secs()} s: ${done}/${todo}`), 30000).unref();
  const workers = new Set();
  const failed = msg => { console.error(msg); process.exitCode = 1; stopped = true; clearInterval(progress); for (const w of workers) w.terminate(); };
  const finish = () => {
    clearInterval(progress);
    fs.closeSync(out);
    fs.writeFileSync(outFile, lines.filter(Boolean).join(""));
    console.log(`${todo} files in ${secs()} s; slowest:`);
    for (const f of ran.sort((a, b) => times[b] - times[a]).slice(0, 5)) console.log(`  ${times[f]} ms  ${path.relative(root, f)}`);
    if (panics.length) console.log(`${panics.length} compiler panics:\n` + panics.map(f => "  " + path.relative(root, f)).join("\n"));
    fs.writeFileSync(TIMES, JSON.stringify(times));
  };
  const spawn = () => {
    const w = new Worker(new URL(import.meta.url), { workerData: { root, module }, resourceLimits: { stackSizeMb: 16 } });
    workers.add(w); active++;
    let current, sent;
    const next = () => {
      if (!queue.length) return w.terminate();
      current = queue.shift(); sent = Date.now();
      w.postMessage({ i: current, f: files[current] });
    };
    const record = (i, line, ms) => {
      lines[i] = line; if (line) fs.writeSync(out, line);
      times[files[i]] = ms; ran.push(files[i]); done++; current = undefined;
    };
    // A worker whose compile trapped is replaced; the run goes on.
    const replace = i => { panics.push(files[i]); w.terminate(); if (queue.length) spawn(); };
    w.on("message", ({ i, line, ms, panic }) => {
      record(i, line, ms);
      if (panic) replace(i); else next();
    });
    w.on("error", e => {
      if (current === undefined) return failed(`worker: ${e.stack || e}`);
      // Out of memory (`resourceLimits`) and the like end the worker: the file is a panic too.
      const i = current;
      const ms = Date.now() - sent;
      record(i, panicLine(files[i], trapMessage(e), ms), ms);
      replace(i);
    });
    w.on("exit", () => { workers.delete(w); if (--active === 0 && !stopped) finish(); });
    next();
  };
  if (!todo) finish();
  for (let k = 0; k < jobs; k++) spawn();
}
