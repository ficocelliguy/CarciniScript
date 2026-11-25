// The real-world corpus survey, optional step 4: a finer breakdown of what `corpus-report.mjs`
// leaves uncategorized. Streams the AST step's output, applies the report's reclassification,
// then sorts each remaining error into a specific bucket from its message, the construct it is
// on, and a look at the source file (is the method, type or trait it names defined in this same
// file? is it inside a macro expansion?), so a bucket says what is a real gap and what is
// downstream of another file. Prints files, errors and crates per bucket with examples.
//   node tools/corpus-uncategorized.mjs [in.jsonl] [registry root]
import fs from "node:fs"; import os from "node:os"; import path from "node:path"; import readline from "node:readline";
const input = process.argv[2] || path.join(os.tmpdir(), "carciniscript-corpus.ast.jsonl");
const root = process.argv[3] || (() => { const src = path.join(process.env.CARGO_HOME || path.join(os.homedir(), ".cargo"), "registry", "src"); const d = fs.existsSync(src) ? fs.readdirSync(src).filter(d => d.startsWith("index.")) : []; if (d.length !== 1) throw new Error(`pass the registry root: found ${d.length} registry indexes in ${src}`); return path.join(src, d[0]); })();
const probes = new Map(JSON.parse(fs.readFileSync(input + ".probes.json", "utf8")).map(p => [p.id, p]));
const missing = id => probes.get(id) && !probes.get(id).ok;
const FIXTURE = /test_data|[\\/]ui[\\/]|compile[-_]fail|[\\/]trybuild|[\\/]tests[\\/]ui/;
const STD_TYPES = new Set("Box Option Result String Vec HashMap HashSet BTreeMap BTreeSet VecDeque BinaryHeap LinkedList Rc Cell RefCell Cow Path PathBuf OsStr OsString File Duration Instant Formatter Arguments Range RangeInclusive Bound Chars Iter IterMut IntoIter Peekable Rev Map Filter Enumerate Zip Chain Take Skip Cursor BufReader BufWriter Command Child Ordering Entry Drain Keys Values Error Wrapping Saturating NonZero".split(" "));
const STD_ENUMS = new Set("Bound Ordering Cow Poll Entry IpAddr SocketAddr Option Result ErrorKind Ordering SeekFrom ControlFlow Shutdown Alignment Component Prefix TryLockError Infallible FpCategory".split(" "));
const STD_DERIVES = new Set("Clone Copy Debug Default PartialEq Eq PartialOrd Ord Hash".split(" "));
const src = new Map();
const file = f => { if (!src.has(f)) { try { src.set(f, fs.readFileSync(path.join(root, f), "utf8")); } catch { src.set(f, ""); } } return src.get(f); };
const esc = s => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const hasFn = (f, n) => new RegExp(`\\bfn\\s+${esc(n)}\\s*[<(]`).test(file(f));
const hasType = (f, n) => new RegExp(`\\b(struct|enum|union|type|trait)\\s+${esc(n)}\\b`).test(file(f));
const hasTrait = (f, n) => new RegExp(`\\btrait\\s+${esc(n)}\\b`).test(file(f));
const hasMacroRules = (f, n) => new RegExp(`macro_rules!\\s*${esc(n)}\\b`).test(file(f));
// Is `fn n` declared inside a `trait X { .. }` block of this file (brace-matched)?
const traitBodies = new Map();
const hasTraitFn = (f, n) => {
  if (!traitBodies.has(f)) {
    const s = file(f), bodies = [];
    const re = /\btrait\s+\w+[^{;]*\{/g; let m;
    while ((m = re.exec(s))) { let d = 1, i = re.lastIndex; while (i < s.length && d) { if (s[i] === "{") d++; else if (s[i] === "}") d--; i++; } bodies.push(s.slice(re.lastIndex, i)); }
    traitBodies.set(f, bodies.join("\n"));
  }
  return new RegExp(`\\bfn\\s+${esc(n)}\\s*[<(]`).test(traitBodies.get(f));
};
const isParam = t => /^[A-Z][A-Za-z0-9]?$/.test(t);
const head = t => t.replace(/^(&(?:'\w+ )?(?:mut )?|\*const |\*mut |dyn |impl |mut )+/, "").replace(/<.*$/s, "").split("::").pop();
const inMacro = e => e.node?.kinds?.includes("MACRO_CALL") || e.node?.kinds?.includes("MACRO_ITEMS") || /^\s*(\w+::)*\w+!\s*[({\[]/.test(e.node?.text || "");
const ctxOf = e => { // the nearest construct around an expression that gives it an expected type
  const k = e.node?.kinds || [];
  for (let i = 1; i < k.length; i++) {
    const x = k[i];
    if (x === "ARG_LIST") return "function argument";
    if (x === "RETURN_EXPR") return "return expression";
    if (x === "LET_STMT") return "let initializer";
    if (x === "RECORD_EXPR_FIELD") return "struct literal field";
    if (x === "BIN_EXPR") return "binary operand";
    if (x === "MATCH_ARM") return "match arm";
    if (x === "CLOSURE_EXPR") return "closure body";
    if (x === "METHOD_CALL_EXPR") return "receiver / chained";
    if (x === "BLOCK_EXPR" || x === "STMT_LIST") return "block tail";
    if (x === "MACRO_CALL" || x === "TOKEN_TREE") return "inside a macro";
    if (x === "TUPLE_EXPR" || x === "ARRAY_EXPR") return "tuple / array element";
    if (x === "INDEX_EXPR") return "index";
    if (x === "IF_EXPR") return "if";
  }
  return "other";
};
const PRIM = /^(u|i)(8|16|32|64|128|size)$|^f(32|64)$|^bool$|^char$/;
const shape = (a, b) => {
  if (/^\{(integer|float)\}$/.test(b)) return `numeric literal where \`${head(a) === a ? a : head(a) + "<..>"}\` is expected`;
  if (/^\{(integer|float)\}$/.test(a)) return `\`${head(b)}\` where a numeric literal's type is expected`;
  if (isParam(a) && isParam(b)) return "two type parameters";
  if (isParam(a)) return "concrete type where a type parameter is expected";
  if (isParam(b)) return "type parameter where a concrete type is expected";
  if (/\bdyn\b/.test(a) && !/\bdyn\b/.test(b)) return "dyn Trait expected, concrete found (unsized coercion)";
  if (/\bdyn\b/.test(b) && !/\bdyn\b/.test(a)) return "concrete expected, dyn Trait found";
  if (/\bimpl\b/.test(a) || /\bimpl\b/.test(b)) return "impl Trait involved";
  if (a.replace(/^&(mut )?/, "") === b.replace(/^&(mut )?/, "")) return "same type, reference level differs";
  if (/^&/.test(a) !== /^&/.test(b)) return "reference vs value of different types";
  if (/^Option</.test(a) && !/^Option</.test(b)) return "Option expected, bare value found";
  if (/^Option</.test(b) && !/^Option</.test(a)) return "bare value expected, Option found";
  if (/^Result</.test(a) !== /^Result</.test(b)) return "Result wrapping differs";
  if (/^Box</.test(a) !== /^Box</.test(b)) return "Box wrapping differs";
  if (head(a) === head(b)) return `same head \`${head(a)}\`, different type arguments`;
  if (/^\(/.test(a) || /^\(/.test(b)) return "tuple involved";
  if (/^\[/.test(a) || /^\[/.test(b) || /^Vec</.test(a) || /^Vec</.test(b)) return "array / slice / Vec involved";
  if (/^(fn|unsafe fn|extern)/.test(a) || /^(fn|unsafe fn|extern)/.test(b)) return "fn pointer involved";
  if (PRIM.test(a) && PRIM.test(b)) return `two primitive types (\`${a}\` vs \`${b}\`)`;
  if (/^(String|&str|str|&'static str)$/.test(a) && /^(String|&str|str|&'static str)$/.test(b)) return "String vs &str";
  return "two unrelated named types";
};
function bucket(f, e) {
  const m = e.m, t = e.node?.text || "", mac = inMacro(e) ? " [in a macro expansion]" : "";
  let x;
  if ((x = m.match(/^no method named `(\w+)` found for (struct|enum|union) `(\w+)`/))) {
    const [, meth, , ty] = x;
    if (STD_TYPES.has(ty) && !hasType(f, ty)) return `no method: std type \`${ty}\` lacks \`${meth}\` (reported as a struct)` + mac;
    if (hasFn(f, meth)) return "no method: `fn NAME` IS in this file (impl missed)" + mac;
    if (!hasType(f, ty)) return "no method: receiver type is defined in another file" + mac;
    return "no method: type is local, method is in another file (other module / trait impl)" + mac;
  }
  if ((x = m.match(/^no method named `(\w+)` found for type parameter `(\w+)`/))) {
    const [, meth] = x;
    return hasTraitFn(f, meth) ? "no method on a type parameter: a trait of this file declares it" + mac : "no method on a type parameter: no local trait declares it (bound trait in another file / supertrait)" + mac;
  }
  if ((x = m.match(/^no method named `(\w+)` found for `([^`]+)` \(the std surface/))) return `std method missing: \`${head(x[2])}::${x[1]}\``;
  if ((x = m.match(/^no method named `(\w+)` found for `([^`]+)`$/))) {
    const [, meth, ty] = x, h = head(ty);
    if (STD_TYPES.has(h)) return `std method missing: \`${h}::${meth}\``;
    if (hasFn(f, meth)) return "no method: `fn NAME` IS in this file (impl missed), receiver not a struct/enum" + mac;
    return "no method on a non-struct receiver: method is in another file" + mac;
  }
  if ((x = m.match(/^no function or associated item named `(\w+)` found for (struct|enum|type parameter) `(\w+)`/))) {
    const [, fn_, k, ty] = x;
    if (k === "type parameter") return "associated fn on a type parameter: not in its bounds" + (hasFn(f, fn_) ? " (declared in this file)" : " (trait in another file)");
    if (STD_TYPES.has(ty) && hasType(f, ty)) return `crate's own \`${ty}\` shadows the std name (associated fn)`;
    if (STD_TYPES.has(ty)) return `std associated fn missing: \`${ty}::${fn_}\``;
    if (hasFn(f, fn_)) return "associated fn IS in this file (impl missed)" + mac;
    return "associated fn on a type whose impl is in another file" + mac;
  }
  if ((x = m.match(/^no field `(\w+)` on type `(\w+)/))) {
    const ty = x[2];
    if (STD_TYPES.has(ty) && hasType(f, ty)) return `crate's own \`${ty}\` shadows the std name (field access)`;
    if (STD_TYPES.has(ty)) return `field access on std type \`${ty}\``;
    if (!hasType(f, ty)) return "field of a type defined in another file" + mac;
    return "no field on a local type" + mac;
  }
  if ((x = m.match(/^mismatched types: expected `(.+?)`, found `(.+?)`(?: \((.*)\))?$/))) {
    const [, a, b, hint] = x;
    if (hint && /NS numbers/.test(hint)) return "mismatched types: NS number kinds";
    return `mismatched types: ${shape(a, b)} -- at a ${ctxOf(e)}` + mac;
  }
  if (/^mismatched types: expected `(.+?)`, found a tuple/.test(m)) return "mismatched types: tuple found" + mac;
  if (/^mismatched types: expected items of type/.test(m)) return "mismatched types: iterator item type" + mac;
  if (/^this block kind is not supported(?: yet)?/.test(m)) {
    const k = e.node?.kinds?.[0];
    const mm = t.match(/(\w+)!\s*[({\[]/);
    if (mm) return `item-position macro call not expanded (${hasMacroRules(f, mm[1]) ? "a local macro_rules" : "macro from elsewhere"})`;
    return `unsupported block kind: ${k}`;
  }
  if (/^duplicate definitions with name/.test(m)) return "duplicate definitions (no #[cfg] seen on the node)" + mac;
  if (/^the name `(\w+)` is defined multiple times/.test(m)) return "name defined multiple times" + (t ? "" : " (a use import)") + mac;
  if (/^fields of this type is not supported(?: yet)?/.test(m)) {
    if (/\)\s*\.\d+$/.test(t)) return "tuple field (`.0`) of a call result";
    if (/\.\d+$/.test(t)) return "tuple field of a non-call expression";
    return "tuple field: other" + mac;
  }
  if ((x = m.match(/^can't call method `(\w+)` on ambiguous numeric type/))) return `method on an unsuffixed literal: \`${x[1]}\`` + mac;
  if (/comes from a generic trait implemented more than once/.test(m)) return "generic trait implemented more than once, called through T / dyn (known)";
  if (/^enum discriminants must be constant/.test(m)) return /::/.test(t) ? "enum discriminant: a path constant from another module" : /^\w+$/.test(t) ? "enum discriminant: a bare const name" : "enum discriminant: other expression";
  if ((x = m.match(/^conflicting implementations of trait `(\w+)` for type `(\w+)` \(/))) return "conflicting impls: slice / array / Vec / Box<[T]> are one JS type (known)";
  if ((x = m.match(/^conflicting implementations of trait `(\w+)` for type `(\w+)`/))) return "conflicting impls of the same trait for the same type (cfg'd or macro-generated twins)" + mac;
  if (/is implemented for both `(\w+)` and `(\w+)`, which are one t/.test(m)) return "trait implemented for two types that are one JS type (known)";
  if (/for both a tuple and a slice, array or/.test(m)) return "trait for both a tuple and a slice / Vec (known)";
  if (/for two tuples of 2 isn't supported/.test(m)) return "trait for two tuples of one length (known)";
  if (/an impl for `&T` \/ `&mut T`/.test(m)) return "blanket impl for &T / &mut T / Box<T> (known)";
  if ((x = m.match(/^the trait bound `(.+?): (\w+)(<.*>)?` is not satisfied(?: \((.*)\))?/))) {
    const [, ty, tr, args, hint] = x;
    if (hint && /derive/.test(hint)) return `bound not satisfied: \`${tr}\` on a type without the derive`;
    if (hint && /implements/.test(hint)) return `bound not satisfied: wrong trait implemented (${hint.slice(0, 40)})`;
    if (tr === "From" || tr === "Into" || tr === "TryFrom") return `bound not satisfied: \`${tr}\` conversion (${hasTrait(f, tr) ? "local" : "impl in another file?"})`;
    if (isParam(head(ty))) return `bound not satisfied on a type parameter: \`${tr}\``;
    return `bound not satisfied: \`${tr}\` for \`${head(ty)}\`` + (hasType(f, head(ty)) ? " (type local)" : " (type elsewhere)");
  }
  if ((x = m.match(/^the trait `(\w+)` is not implemented for `(.+?)` \(field/))) {
    const [, tr, ty] = x;
    if (!STD_DERIVES.has(tr)) return `derive of a non-std trait \`${tr}\` checks field types`;
    if (/\bdyn\b/.test(ty)) return `derive \`${tr}\`: field is a \`dyn Trait\` (${ty.match(/^(\w+)</)?.[1] || "bare"})`;
    if (/^impl /.test(ty)) return `derive \`${tr}\`: field is an \`impl Trait\``;
    if (/^(unsafe )?(extern "\w+" )?fn\(/.test(ty)) return `derive \`${tr}\`: field is a fn pointer`;
    if (isParam(head(ty))) return `derive \`${tr}\`: field is a type parameter (derive should add the bound)`;
    if (/^\[/.test(ty)) return `derive \`${tr}\`: field is an array`;
    if (/^\(/.test(ty)) return `derive \`${tr}\`: field is a tuple`;
    if (/^&/.test(ty)) return `derive \`${tr}\`: field is a reference`;
    const h = head(ty);
    if (STD_TYPES.has(h) || /^(PhantomData|NonNull|MaybeUninit|UnsafeCell|ManuallyDrop|Pin|Arc|Mutex|Atomic\w+)/.test(h)) return `derive \`${tr}\`: field is std \`${h}\``;
    return hasType(f, h) ? `derive \`${tr}\`: field type is local without that derive` : `derive \`${tr}\`: field type from another file`;
  }
  if (/^(struct|constant) and enum patterns is not supported(?: yet)?/.test(m)) {
    const kind = m.startsWith("struct") ? "tuple/struct" : "constant/unit";
    const p = t.replace(/\(.*$/s, "").replace(/\s*\{.*$/s, "").trim();
    const segs = p.replace(/^::/, "").split("::");
    if (segs.length === 1) return `${kind} pattern: bare name` + mac;
    const en = segs[segs.length - 2];
    if (STD_ENUMS.has(en)) return `${kind} pattern of std enum \`${en}\`` + mac;
    if (/^[a-z_][a-z0-9_]*$/.test(segs[0]) && !/^(crate|self|super|std|core|alloc)$/.test(segs[0])) return `${kind} pattern: path through another crate` + mac;
    if (/^(crate|self|super)$/.test(segs[0])) return `${kind} pattern: \`${segs[0]}::\` path` + mac;
    return hasType(f, en) ? `${kind} pattern of a local enum` + mac : `${kind} pattern of an enum from another file` + mac;
  }
  if (/say what `\.into\(\)` converts to/.test(m)) return `\`.into()\` target unknown at a ${ctxOf(e)}` + mac;
  if (/say what `\.try_into\(\)`|say what `\.parse\(\)`|say what to parse into/.test(m)) return `\`.parse()\` / \`.try_into()\` target unknown at a ${ctxOf(e)}` + mac;
  if (/say what to collect into/.test(m)) return `\`.collect()\` target unknown at a ${ctxOf(e)}` + mac;
  if (/type annotations needed for `Default::default\(\)`/.test(m)) return `\`Default::default()\` target unknown at a ${ctxOf(e)}` + mac;
  if ((x = m.match(/^type annotations needed for `(.+?)`/))) return `type annotations needed for \`${x[1].replace(/::<.*/, "")}\` at a ${ctxOf(e)}` + mac;
  if (/^type annotations needed/.test(m)) return `type annotations needed: other (${m.slice(0, 60)})` + mac;
  if ((x = m.match(/^(comparing with )?`(.+?)` on type parameter `(\w+)` needs a bound: add `\w+: (\w+)`/))) return `operator on a type parameter needs a bound: \`${x[4]}\` (${x[2]})`;
  if ((x = m.match(/^struct `(\w+)` has no field named `(\w+)`/))) return /__h\d+$/.test(x[2]) ? "BUG: macro hygiene renames a struct-literal field name (`ids__h1`)" : "struct literal names a field the struct lacks" + mac;
  if ((x = m.match(/^missing fields? `(\w+)/))) return mac ? "missing field in a macro-generated struct literal (hygiene twin of the above)" : "missing field in initializer";
  if ((x = m.match(/^variant `(\w+)` has no field named/))) return "variant literal names a field the variant lacks" + mac;
  if ((x = m.match(/^cannot borrow `(\w+)` as mutable, as it is not declared as mutable/))) {
    const n = x[1], k = e.node?.kinds?.[0];
    const mutParam = n === "self" ? /&(?:'\w+ )?mut self/.test(file(f)) : new RegExp(`\\b${n}\\s*:\\s*&(?:'\\w+ )?mut\\b`).test(file(f));
    return `E0596 "${n === "self" ? "self" : "x"}" not declared mutable on a ${k}` + (mutParam ? " (a `&mut` parameter: FALSE POSITIVE?)" : "") + mac;
  }
  if ((x = m.match(/^expected tuple struct or tuple variant, found `(\w+)::(\w+)`/))) return hasType(f, x[1]) ? "tuple-variant pattern on a local enum" + (/macro_rules!/.test(file(f)) ? " (file has macro_rules)" : "") + mac : "tuple-variant pattern on an enum from another file" + mac;
  if (/^expected tuple struct or tuple variant/.test(m)) return "expected tuple struct or variant: other" + mac;
  if ((x = m.match(/^`(.+?)` doesn't implement `std::fmt::(\w+)`/))) {
    const ty = x[1];
    if (ty === "()") return `\`()\` doesn't implement ${x[2]}: a binding typed as unit (lost type)` + mac;
    if (isParam(head(ty))) return `type parameter doesn't implement ${x[2]} (bound missing or via supertrait)`;
    return hasType(f, head(ty)) ? `local type doesn't implement ${x[2]}` + mac : `type from another file doesn't implement ${x[2]}` + mac;
  }
  if (/^assignment to this place is not supported(?: yet)?/.test(m)) return /\)\s*\[[^\]]*\]$/.test(t) ? "assignment target: index of a call result" : /\)\s*\.\w+$/.test(t) ? "assignment target: field of a call result" : "assignment target: other" + mac;
  if ((x = m.match(/^cannot apply binary operator `(.+?)` to type `(\w+)/))) return `binary operator \`${x[1]}\` on \`${x[2]}\`` + (STD_TYPES.has(x[2]) ? " (std op impl missing)" : hasType(f, x[2]) ? " (local type)" : " (type elsewhere)");
  if ((x = m.match(/arms have incompatible types: expected `(.+?)`, found `(.+?)`/))) return `match arms differ: ${shape(x[1], x[2])}` + mac;
  if (/^attempted to take value of method/.test(m)) return "method used as a field" + mac;
  if ((x = m.match(/^non-exhaustive patterns: `(.+?)` not covered/))) return x[1] === "_" ? "non-exhaustive: `_` not covered (scrutinee type unknown / cfg_if)" + mac : /\[/.test(x[1]) ? `non-exhaustive: slice pattern (${x[1]})` : /\.\./.test(x[1]) ? `non-exhaustive: range pattern (${x[1]})` : `non-exhaustive: \`${x[1].slice(0, 30)}\`` + mac;
  if ((x = m.match(/^no implementation for `(.+?)`/))) return `no implementation for \`${x[1]}\`` + mac;
  if (/name a concrete type/.test(m)) return "TypeId::of / type_name on a type parameter (erased)";
  if ((x = m.match(/^this (function|method) takes (\d+) arguments? but (\d+) (?:were|was) supplied/))) return `arity: ${x[1]} takes ${x[2]}, got ${x[3]}` + mac;
  if (/^`(\w+)` takes (\d+) argument/.test(m)) return "arity: other" + mac;
  if (/environment variable `(.+?)` not defined at compile time/.test(m)) return "env! of a cargo build variable";
  if ((x = m.match(/^cannot (subtract|add|multiply|divide) `(.+?)` (?:from|to|by) `(.+?)`/))) return `arithmetic \`${x[1]}\`: \`${head(x[2])}\` with \`${head(x[3])}\`` + mac;
  if (/backend|feature|target|WASI|zlib|errno|benchmarks must be run|serde_json requires/.test(m) && !/^the trait/.test(m)) return "compile_error! from cfg selection (not a gap)";
  if (/^refutable pattern/.test(m)) return "refutable pattern in let / for" + mac;
  if ((x = m.match(/^the type `(.+?)` cannot be indexed by `(.+?)`/))) return `indexing \`${head(x[1])}\` by \`${x[2]}\``;
  if (/^strings cannot be indexed by integers/.test(m)) return "string indexed by an integer" + mac;
  if (/^cannot index into a value of type/.test(m)) return "index into non-indexable type" + mac;
  if (/^cannot slice a value of type/.test(m)) return "slice of a non-sliceable type" + mac;
  if ((x = m.match(/^non-primitive cast: `(.+?)` as `(.+?)`/))) return `non-primitive cast: \`${head(x[1])}\` as \`${head(x[2])}\``;
  if (/^a range is expected here|takes a range/.test(m)) return "range expected" + mac;
  if (/couldn't convert the error to/.test(m)) return "`?` error conversion: no `From` impl found" + mac;
  if (/^not all trait items implemented/.test(m)) return "not all trait items implemented" + mac;
  if (/^expected a .* pattern, found/.test(m)) return "pattern kind mismatch" + mac;
  if (/^there is no field `(\w+)` here/.test(m)) return "format string names an unknown field" + mac;
  if ((x = m.match(/`(.+?)` is not an iterator/))) return `not an iterator: \`${head(x[1])}\`` + mac;
  if (/cannot be built from an iterator/.test(m)) return "FromIterator missing" + mac;
  if (/method `(\w+)` is not a member of trait/.test(m)) return "impl defines a method the trait lacks" + mac;
  if (/pattern does not mention field/.test(m)) return "struct pattern misses a field" + mac;
  if (/cannot call associated function on trait without specifying/.test(m)) return "`Trait::f()` without a type" + mac;
  if (/is a field of .*, not a method/.test(m)) return "field called as a method" + mac;
  if (/^the `\?` operator can only be used/.test(m)) return "`?` in a non-Result function" + mac;
  if (/can't be told apart at run time/.test(m)) return "types that can't be told apart at run time" + mac;
  if (/^missing type for function parameter/.test(m)) return "missing type for function parameter" + mac;
  return "OTHER: " + m.replace(/`[^`]*`/g, "`_`").slice(0, 70) + mac;
}
const groups = new Map(); let hits = 0;
const rl = readline.createInterface({ input: fs.createReadStream(input) });
for await (const line of rl) {
  if (!line) continue; const r = JSON.parse(line); if (FIXTURE.test(r.f)) continue;
  const seen = new Set();
  for (const e of r.errs) {
    if (e.cat !== "GENERIC") continue;
    if (/^no rules of/.test(e.m)) continue;
    if (/defined multiple times|^duplicate definitions/.test(e.m) && e.node?.cfg) continue;
    if (e.node?.feat && missing(e.node.feat)) continue;
    if (e.ext?.length || e.ctx_ext?.length || /\{unknown\}/.test(e.m)) continue;
    const old = `${e.why}  @ ${e.node ? e.node.feat || e.node.kinds[0] : "?"}`;
    if (!seen.has(old)) { seen.add(old); hits++; }
    const k = bucket(r.f, e);
    const g = groups.get(k) || { n: 0, files: new Set(), crates: new Set(), ex: [] };
    g.n++; g.files.add(r.f); g.crates.add(r.f.split(/[\\/]/)[0]);
    const crate = r.f.split(/[\\/]/)[0];
    if (g.ex.length < 3 && !g.ex.some(x => x.startsWith(crate))) g.ex.push(`${r.f}:${e.line}  ${e.m.slice(0, 110)}  @ ${(e.node?.text || "").slice(0, 50).replace(/\s+/g, " ")}`);
    groups.set(k, g);
  }
}
console.log(`${hits} file hits by the report's (message @ construct) key; ${[...groups.values()].reduce((s, g) => s + g.files.size, 0)} by the finer key\n`);
console.log("files  errors crates  bucket");
for (const [k, g] of [...groups].sort((a, b) => b[1].files.size - a[1].files.size || b[1].n - a[1].n)) {
  console.log(`${String(g.files.size).padStart(5)} ${String(g.n).padStart(7)} ${String(g.crates.size).padStart(6)}  ${k}`);
  for (const x of g.ex) console.log(`                       ${x}`);
}
