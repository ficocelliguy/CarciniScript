// CarciniScript runtime. Installs `globalThis.κσ` once. Works as a classic script (Bitburner
// injects it like RapydScript's baselib) or as a side-effect ES module import.
//
// Every helper name starts with `$` so none collide with Netscript function names: Bitburner's
// RAM calculator charges for any identifier that matches an NS function, member properties included.
(function () {
  "use strict";
  if (globalThis.κσ) return;

  // A Rust panic. Hosts show an uncaught error as `String(e)` or `e.stack`, so both are rustc's
  // report (the name is empty); the JS stack stays at `jsStack`. `panicMessage` and `location`
  // are the parts.
  class CarciniPanic extends Error {
    constructor(message, location) {
      const report =
        `thread '${threadName}' panicked at ${location}:\n${message}\n` +
        "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace";
      super(report);
      this.name = "";
      this.panicMessage = message;
      this.location = location;
      asReport(this, report);
    }
  }

  // `fn main() -> Result<..>` returned `Err(e)`: rustc's `Error: {e:?}`, shown like a panic.
  class CarciniError extends Error {
    constructor(report) {
      super(report);
      this.name = "";
      asReport(this, report);
    }
  }

  function asReport(e, report) {
    e.jsStack = e.stack;
    Object.defineProperty(e, "stack", { value: report, writable: true, configurable: true });
  }

  // `std::process::exit(code)`: unwinds to the host, which ends the script with `exitCode`.
  class CarciniExit extends Error {
    constructor(code) {
      super(`process exited with code ${code}`);
      this.name = "CarciniExit";
      this.exitCode = code;
    }
  }
  // (`Intl` formatters by zone, for chrono-tz: making one is slow.)
  const tzFormats = new Map();
  const tzNames = ["UTC"];
  function tzFormat(zone, loc) {
    const k = zone + "|" + loc;
    let f = tzFormats.get(k);
    if (!f) {
      f = new Intl.DateTimeFormat(loc, {
        timeZone: zone,
        hourCycle: "h23",
        year: "numeric",
        month: "numeric",
        day: "numeric",
        hour: "numeric",
        minute: "numeric",
        second: "numeric",
      });
      tzFormats.set(k, f);
    }
    return f;
  }
  // `std::env::args()`: what the host set with `$setArgs` (the program's name first).
  let argv = ["main"];

  let outSink = (line) => console.log(line);
  let errSink = (line) => console.error(line);
  let outBuf = "";
  let errBuf = "";

  function write(chunk, isErr) {
    let buf = (isErr ? errBuf : outBuf) + chunk;
    const sink = isErr ? errSink : outSink;
    let nl;
    while ((nl = buf.indexOf("\n")) !== -1) {
      sink(buf.slice(0, nl));
      buf = buf.slice(nl + 1);
    }
    if (isErr) errBuf = buf;
    else outBuf = buf;
  }

  function expandExponent(s) {
    // "1.5e-7" -> "0.00000015", "1e+21" -> "1000000000000000000000"
    const m = /^(-?)(\d)(?:\.(\d+))?e([+-]\d+)$/.exec(s);
    if (!m) return s;
    const [, sign, lead, frac = "", expStr] = m;
    const exp = Number(expStr);
    const digits = lead + frac;
    if (exp < 0) return `${sign}0.${"0".repeat(-exp - 1)}${digits}`;
    const intLen = 1 + exp;
    if (digits.length <= intLen) return sign + digits + "0".repeat(intLen - digits.length);
    return `${sign}${digits.slice(0, intLen)}.${digits.slice(intLen)}`;
  }

  // Display for f64: like JS, but `inf`, `-0`, and never exponent notation.
  function fmtF(x) {
    if (Number.isNaN(x)) return "NaN";
    if (x === Infinity) return "inf";
    if (x === -Infinity) return "-inf";
    if (Object.is(x, -0)) return "-0";
    return expandExponent(String(x));
  }

  function shortF32(x) {
    if (!Number.isFinite(x) || x === 0) return x;
    for (let p = 1; p <= 9; p++) {
      const y = Number(x.toPrecision(p));
      if (Math.fround(y) === x) return y;
    }
    return x;
  }

  // Debug for f64: integral values get ".0"; very large/small values use exponent notation.
  function dbgF(x) {
    if (!Number.isFinite(x) || Object.is(x, -0)) return Object.is(x, -0) ? "-0.0" : fmtF(x);
    const a = Math.abs(x);
    if (a !== 0 && (a >= 1e16 || a < 1e-4)) return x.toExponential().replace("e+", "e");
    const s = String(x);
    return Number.isInteger(x) ? `${s}.0` : s;
  }

  function escapeChar(c, quote) {
    switch (c) {
      case "\\": return "\\\\";
      case "\n": return "\\n";
      case "\r": return "\\r";
      case "\t": return "\\t";
      case "\0": return "\\0";
      case quote: return "\\" + quote;
    }
    const cp = c.codePointAt(0);
    if (cp < 0x20 || cp === 0x7f) return `\\u{${cp.toString(16)}}`;
    return c;
  }

  // `Some(x)` is `x` itself, except when `x` is itself "nothing": `Some(None)` is a marker
  // (`Some(Some(None))` the next one, ..), and `Some(())` is `true` (`()` is `undefined`).
  class SomeNone {
    constructor(k) {
      this.k = k;
    }
  }
  const someNones = [null];
  const someNone = (k) => {
    while (someNones.length <= k) someNones.push(new SomeNone(someNones.length));
    return someNones[k];
  };
  // `Some(x)`.
  function some(x) {
    if (x === null) return someNone(1);
    if (x === undefined) return true;
    if (x instanceof SomeNone) return someNone(x.k + 1);
    return x;
  }
  // What a `Some` holds (an inner `Option`'s own representation).
  function unsome(x) {
    if (x instanceof SomeNone) return x.k === 1 ? null : someNone(x.k - 1);
    return x;
  }

  // Rust's `==` for the values CarciniScript represents: structural for Vecs (JS arrays).
  function eq(a, b) {
    if (a instanceof SomeNone || b instanceof SomeNone) return a === b;
    // `None` is null, but values from JS (NS results) may use undefined.
    if (a == null || b == null) return a == b;
    // Structs and enums: their derived `$eq`.
    if (typeof a.$eq === "function") return a.$eq(b);
    if (a instanceof HashSet && b instanceof HashSet) return a.m.size === b.m.size && a.$isSubset(b);
    if (a instanceof HashMap && b instanceof HashMap) {
      if (a.m.size !== b.m.size) return false;
      for (const [kk, [, v]] of a.m) {
        const other = b.m.get(kk);
        if (!other || !eq(v, other[1])) return false;
      }
      return true;
    }
    if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) return false;
      for (let i = 0; i < a.length; i++) if (!eq(a[i], b[i])) return false;
      return true;
    }
    return a === b;
  }

  // Whether `a` (a struct or enum) has its own method `m` and `b` isn't of its class.
  function otherClass(a, b, m) {
    return (
      a !== null &&
      typeof a === "object" &&
      typeof a[m] === "function" &&
      !(b !== null && typeof b === "object" && b.constructor === a.constructor)
    );
  }

  // Rust's `Ord`: numbers, bools, strings and (lexicographically) Vecs.
  function cmp(a, b) {
    // `None` sorts before every `Some`.
    if (a == null || b == null) return a == null ? (b == null ? 0 : -1) : 1;
    if (a instanceof SomeNone || b instanceof SomeNone) return cmp(unsome(a), unsome(b));
    if (typeof a.$cmp === "function") return a.$cmp(b);
    if (Array.isArray(a) && Array.isArray(b)) {
      const n = Math.min(a.length, b.length);
      for (let i = 0; i < n; i++) {
        const c = cmp(a[i], b[i]);
        if (c !== 0) return c;
      }
      return Math.sign(a.length - b.length);
    }
    // A NaN is incomparable (`partial_cmp` gives `None`): NaN makes every `<`/`>` on it false.
    if (a !== a || b !== b) return NaN;
    return a < b ? -1 : a > b ? 1 : 0;
  }

  function clone(v) {
    if (v instanceof HashSet || v instanceof HashMap) return v.$clone();
    if (v != null && typeof v.$clone === "function") return v.$clone();
    return Array.isArray(v) ? v.map(clone) : v;
  }

  // Map key for a HashSet element: primitives as themselves, composite values (tuples, Vecs)
  // structurally: the text `JSON.stringify(x, bigKey)` gives. A set holds one element type, so
  // the two kinds never mix. Arrays of primitives (tuple keys) are written by hand, which is
  // many times faster than `JSON.stringify` with a replacer, and gives the same text.
  function key(x) {
    if (typeof x !== "object" || x === null) return x;
    return Array.isArray(x) ? arrayKey(x) : JSON.stringify(x, bigKey);
  }
  function arrayKey(a) {
    let s = "[";
    for (let i = 0; i < a.length; i++) {
      const v = a[i];
      if (i !== 0) s += ",";
      switch (typeof v) {
        case "number":
          // (JSON writes NaN and the infinities as `null`; `-0` as `0`, like `String`.)
          s += v - v === 0 ? v : "null";
          break;
        case "string":
          s += JSON.stringify(v);
          break;
        case "boolean":
          s += v;
          break;
        case "bigint":
          s += `"${v}n"`;
          break;
        case "object":
          s += v === null ? "null" : Array.isArray(v) ? arrayKey(v) : JSON.stringify(v, bigKey);
          break;
        default:
          // (`undefined` — `()` — is `null` in a JSON array.)
          s += "null";
      }
    }
    return s + "]";
  }
  // (`JSON.stringify` can't write a BigInt; a key only needs to be distinct.)
  function bigKey(_, v) {
    return typeof v === "bigint" ? `${v}n` : v;
  }

  // A `std::hash::Hasher`: collects the values written to it. `finish()` is FNV-1a (64-bit) of
  // them, which differs from Rust's SipHash (its values aren't specified anyway), kept to its low
  // 53 bits: a `u64` is a JS number, and an exact one keeps `h % buckets` exact.
  class Hasher {
    constructor() {
      this.parts = [];
    }
    $write(x) {
      this.parts.push(typeof x === "object" && x !== null ? key(x) : x);
    }
    $finish() {
      const s = JSON.stringify(this.parts, bigKey);
      let h = 0xcbf29ce484222325n;
      for (let i = 0; i < s.length; i++) {
        h ^= BigInt(s.charCodeAt(i));
        h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
      }
      return Number(h & 0x1fffffffffffffn);
    }
  }

  // What `f.debug_struct("P")` / `debug_tuple` / `debug_list` / `debug_set` / `debug_map`
  // return: collects each value's `{:?}` text and writes `P { x: 1 }`, `P(1)`, `[1]` or
  // `{"k": 1}` into the Formatter at `finish()` (`{:#?}` re-lays it like any `{:?}`).
  class DbgBuilder {
    constructor(f, kind, name) {
      this.f = f;
      this.kind = kind;
      this.name = name;
      this.parts = [];
      this.k = null;
    }
    $field(name, s) {
      this.parts.push(name === null ? s : `${name}: ${s}`);
      return this;
    }
    $entry(a, b) {
      this.parts.push(b === undefined ? a : `${a}: ${b}`);
      return this;
    }
    $key(k) {
      this.k = k;
      return this;
    }
    $value(v) {
      this.parts.push(`${this.k}: ${v}`);
      this.k = null;
      return this;
    }
    $entries(items, fmt) {
      for (const x of items) this.parts.push(fmt(x));
      return this;
    }
    $finish(more) {
      const p = more ? [...this.parts, ".."] : this.parts;
      let s;
      if (this.kind === "struct") s = p.length ? `${this.name} { ${p.join(", ")} }` : this.name;
      else if (this.kind === "tuple") s = p.length ? `${this.name}(${p.join(", ")})` : this.name;
      else if (this.kind === "list") s = `[${p.join(", ")}]`;
      else s = `{${p.join(", ")}}`;
      // (`{:#?}`: one item per line, as Rust's builders write in alternate mode.)
      if (this.f.o?.alt) s = rt.$pretty(s);
      this.f.s += s;
      return new Res(0, undefined);
    }
  }

  // A number as a `dyn Trait` whose trait has impls on two number types (`impl Name for i32`,
  // `impl Name for f64`): the value `v` and its type's impl key `k`, since a whole `f64` looks
  // like an integer. `$dcall` takes `k`'s impl.
  class DynNum {
    constructor(v, k) {
      this.v = v;
      this.k = k;
    }
    toString() {
      return typeof this.v === "number" ? fmtF(this.v) : String(this.v);
    }
    $dbg() {
      if (this.k === "$own") return rt.$dbgAny(this.v);
      return this.k === "f32" || this.k === "f64" ? dbgF(this.v) : String(this.v);
    }
  }

  // Rust's VecDeque<T>: a ring buffer. `b` is the backing array, `h` the head's position in it and
  // `n` the length; element `i` is at `b[(h + i) % b.length]`, so `pop_front` and `push_front`
  // are O(1). The compiler indexes a deque through `$i` (checked) / `$p`; everything a `Vec`
  // shares with it (its view type) reaches the deque as the Array methods a `Vec` would get, so
  // those are here too, running on `$arr()`: the contents made contiguous (head at 0, `b` trimmed
  // to the length), which IS `b`, so in-place changes through it stay in the deque.
  class Deque {
    constructor() {
      this.b = [];
      this.h = 0;
      this.n = 0;
    }
    static from(items) {
      const d = new Deque();
      d.b = Array.from(items);
      d.n = d.b.length;
      return d;
    }
    get length() {
      return this.n;
    }
    // (`length = m`: `truncate` / `clear` from the back; growing isn't reached from Rust.)
    set length(m) {
      if (m < this.n) {
        if (m === 0) {
          this.b.length = 0;
          this.h = 0;
        } else {
          for (let i = m; i < this.n; i++) this.b[this.$p(i)] = undefined;
        }
        this.n = m;
      } else if (m > this.n) {
        this.$arr().length = m;
        this.n = m;
      }
    }
    // The backing index of element `i` (in range).
    $p(i) {
      const j = this.h + i;
      return j < this.b.length ? j : j - this.b.length;
    }
    // `$p` with a Vec's bounds check and panic (`loc`: the `[`).
    $i(i, loc) {
      if (i >= 0 && i < this.n) {
        const j = this.h + i;
        return j < this.b.length ? j : j - this.b.length;
      }
      return rt.$oob(this, i, loc);
    }
    $at(i) {
      return this.b[this.$p(i)];
    }
    $arr() {
      if (this.h !== 0) {
        const a = [];
        for (let i = 0; i < this.n; i++) a.push(this.$at(i));
        this.b = a;
        this.h = 0;
      } else if (this.b.length !== this.n) {
        this.b.length = this.n;
      }
      return this.b;
    }
    // After a helper changed the length of `$arr()`.
    $sync() {
      this.n = this.b.length;
    }
    // Contiguous, with room for as many again (a full ring whose head isn't at 0).
    $grow() {
      const a = [];
      for (let i = 0; i < this.n; i++) a.push(this.$at(i));
      for (let i = Math.max(8, 2 * this.n); i > this.n; i--) a.push(undefined);
      this.b = a;
      this.h = 0;
    }
    push(x) {
      const b = this.b;
      if (this.n === b.length) {
        if (this.h !== 0) {
          this.$grow();
          this.b[this.n] = x;
        } else {
          b.push(x);
        }
      } else {
        let j = this.h + this.n;
        if (j >= b.length) j -= b.length;
        b[j] = x;
      }
      this.n++;
    }
    pop() {
      if (this.n === 0) return undefined;
      const j = this.$p(--this.n);
      const x = this.b[j];
      this.b[j] = undefined;
      if (this.n === 0) this.h = 0;
      return x;
    }
    shift() {
      if (this.n === 0) return undefined;
      const b = this.b;
      const x = b[this.h];
      b[this.h] = undefined;
      this.h = this.h + 1 === b.length ? 0 : this.h + 1;
      if (--this.n === 0) this.h = 0;
      return x;
    }
    unshift(x) {
      if (this.n === this.b.length) this.$grow();
      this.h = this.h === 0 ? this.b.length - 1 : this.h - 1;
      this.b[this.h] = x;
      this.n++;
    }
    [Symbol.iterator]() {
      return new DequeIt(this);
    }
    values() {
      return new DequeIt(this);
    }
    keys() {
      return this.$arr().keys();
    }
    entries() {
      return this.$arr().entries();
    }
    toJSON() {
      return this.$arr();
    }
    toString() {
      return this.$arr().toString();
    }
    // (The result of `splice` is the removed part: a `VecDeque` for `split_off`, see `$splitOff`.)
    splice(...args) {
      const out = this.$arr().splice(...args);
      this.$sync();
      return out;
    }
    reverse() {
      this.$arr().reverse();
      return this;
    }
    sort(f) {
      this.$arr().sort(f);
      return this;
    }
    fill(x, a, b) {
      this.$arr().fill(x, a, b);
      return this;
    }
    copyWithin(t, a, b) {
      this.$arr().copyWithin(t, a, b);
      return this;
    }
    $eq(o) {
      if (o instanceof Deque) o = o.$arr();
      if (!Array.isArray(o) || o.length !== this.n) return false;
      for (let i = 0; i < this.n; i++) if (!eq(this.$at(i), o[i])) return false;
      return true;
    }
    $cmp(o) {
      return cmp(this.$arr(), o instanceof Deque ? o.$arr() : o);
    }
    $clone() {
      const d = new Deque();
      for (let i = 0; i < this.n; i++) d.b.push(clone(this.$at(i)));
      d.n = this.n;
      return d;
    }
    $dbg() {
      return rt.$dbgAny(this.$arr());
    }
  }
  // The rest of the Array surface runs on the contiguous contents.
  for (const name of [
    "slice", "at", "map", "filter", "some", "every", "find", "findIndex", "findLast", "findLastIndex",
    "forEach", "reduce", "reduceRight", "includes", "indexOf", "lastIndexOf", "join", "concat", "flat",
    "flatMap", "toSorted", "toReversed", "with",
  ]) {
    if (typeof Array.prototype[name] === "function") {
      Deque.prototype[name] = function (...args) {
        return this.$arr()[name](...args);
      };
    }
  }
  class DequeIt {
    constructor(d) {
      this.d = d;
      this.i = 0;
    }
    [Symbol.iterator]() {
      return this;
    }
    next() {
      const d = this.d;
      if (this.i < d.n) return { value: d.b[d.$p(this.i++)], done: false };
      return { value: undefined, done: true };
    }
  }
  // A plain array with `v`'s elements: `v` itself unless it's a deque.
  function arr(v) {
    return v instanceof Deque ? v.$arr() : v;
  }

  // `&mut v[lo..hi]`: a Proxy over `v[lo..lo + len)`, so index reads and writes reach `v`. Its
  // `[VIEW]` is the base and offset, for helpers and views of views. Array methods run on the
  // base rather than through the traps: `slice` and iteration directly, the other read-only ones
  // on a copy of the range, and the in-place ones (`sort`, `reverse`, ..) on a copy written back.
  const VIEW = Symbol("view");
  const VIEW_COPY = new Set([
    "at", "map", "filter", "some", "every", "find", "findIndex", "findLast", "findLastIndex", "forEach",
    "reduce", "reduceRight", "includes", "indexOf", "lastIndexOf", "join", "concat", "flat", "flatMap",
    "toSorted", "toReversed", "with", "keys", "entries", "toString",
  ]);
  const VIEW_INPLACE = new Set(["sort", "reverse", "fill", "copyWithin"]);
  // The index a property name is (`"0"`, `"12"`; not `"01"` or `"x"`), or -1.
  function indexName(k) {
    if (typeof k !== "string") return -1;
    const n = k.length;
    if (n === 0 || n > 15) return -1;
    let c = k.charCodeAt(0);
    if (c < 48 || c > 57 || (c === 48 && n > 1)) return -1;
    let i = c - 48;
    for (let j = 1; j < n; j++) {
      c = k.charCodeAt(j);
      if (c < 48 || c > 57) return -1;
      i = i * 10 + (c - 48);
    }
    return i;
  }
  // `slice(a, b)` arguments relative to a view of `len`, as `[from, to)` in its base.
  function viewRange(a, b, lo, len) {
    let from = a === undefined ? 0 : Math.trunc(a);
    from = from < 0 ? Math.max(0, len + from) : Math.min(from, len);
    let to = b === undefined ? len : Math.trunc(b);
    to = to < 0 ? Math.max(0, len + to) : Math.min(to, len);
    return [lo + from, lo + Math.max(from, to)];
  }
  // (The target is an empty array, so `Array.isArray` holds; it carries the base `v`, offset `lo`
  // and length `len`, and one handler serves every view.)
  const viewHandler = {
    get(t, k, proxy) {
      const { v, lo, len } = t;
      if (typeof k === "string") {
        if (k === "length") return len;
        const i = indexName(k);
        if (i !== -1) return i < len ? v[lo + i] : undefined;
        if (k === "slice") return (a, b) => v.slice(...viewRange(a, b, lo, len));
        if (k === "values") return () => new ArrayIt(v, lo, lo + len);
        if (VIEW_COPY.has(k)) return (...args) => v.slice(lo, lo + len)[k](...args);
        if (VIEW_INPLACE.has(k)) {
          return (...args) => {
            const c = v.slice(lo, lo + len);
            c[k](...args);
            for (let j = 0; j < len; j++) v[lo + j] = c[j];
            return proxy;
          };
        }
        return Array.prototype[k];
      }
      if (k === VIEW) return t;
      if (k === Symbol.iterator) return () => new ArrayIt(v, lo, lo + len);
      return Array.prototype[k];
    },
    set(t, k, x) {
      const i = indexName(k);
      if (i !== -1 && i < t.len) t.v[t.lo + i] = x;
      return true;
    },
    has(t, k) {
      const i = indexName(k);
      return i !== -1 ? i < t.len : k in Array.prototype;
    },
    deleteProperty() {
      return true;
    },
    ownKeys(t) {
      return [...Array(t.len).keys()].map(String).concat("length");
    },
    getOwnPropertyDescriptor(t, k) {
      if (k === "length") return { value: t.len, writable: true, enumerable: false, configurable: false };
      const i = indexName(k);
      if (i !== -1 && i < t.len) return { value: t.v[t.lo + i], writable: true, enumerable: true, configurable: true };
      return undefined;
    },
  };
  function sliceView(v, lo, len) {
    const t = [];
    t.v = v;
    t.lo = lo;
    t.len = len;
    return new Proxy(t, viewHandler);
  }

  // Rust's HashSet<T>. Methods are `$`-prefixed so Bitburner's RAM calculator never mistakes them
  // for NS functions. Iterates in insertion order (Rust's order is unspecified).
  class HashSet {
    constructor() {
      this.m = new Map();
    }
    static from(items) {
      const s = new HashSet();
      for (const x of items) s.$insert(x);
      return s;
    }
    $insert(x) {
      const k = key(x);
      if (this.m.has(k)) return false;
      this.m.set(k, x);
      return true;
    }
    $contains(x) {
      return this.m.has(key(x));
    }
    $remove(x) {
      return this.m.delete(key(x));
    }
    $len() {
      return this.m.size;
    }
    $clear() {
      this.m.clear();
    }
    $clone() {
      const s = new HashSet();
      for (const [k, v] of this.m) s.m.set(k, clone(v));
      return s;
    }
    $isSubset(other) {
      for (const k of this.m.keys()) if (!other.m.has(k)) return false;
      return true;
    }
    $isSuperset(other) {
      return other.$isSubset(this);
    }
    $isDisjoint(other) {
      for (const k of this.m.keys()) if (other.m.has(k)) return false;
      return true;
    }
    $retain(p) {
      for (const [k, x] of this.m) if (!p(x)) this.m.delete(k);
    }
    // `drain()`: the items, the set emptied at once.
    $drain() {
      const xs = [...this.m.values()];
      this.m.clear();
      return Iter.of(function* () {
        yield* xs;
      });
    }
    $extend(xs) {
      for (const x of xs) this.$insert(x);
    }
    $union(other) {
      const self = this;
      return Iter.of(function* () {
        yield* self.m.values();
        for (const [k, x] of other.m) if (!self.m.has(k)) yield x;
      });
    }
    $intersection(other) {
      const self = this;
      return Iter.of(function* () {
        for (const [k, x] of self.m) if (other.m.has(k)) yield x;
      });
    }
    $difference(other) {
      const self = this;
      return Iter.of(function* () {
        for (const [k, x] of self.m) if (!other.m.has(k)) yield x;
      });
    }
    $symmetricDifference(other) {
      const self = this;
      return Iter.of(function* () {
        for (const [k, x] of self.m) if (!other.m.has(k)) yield x;
        for (const [k, x] of other.m) if (!self.m.has(k)) yield x;
      });
    }
    [Symbol.iterator]() {
      return this.m.values();
    }
  }

  // Rust's HashMap<K, V>. Each entry is a `[key, value]` slot, so `entry(k).or_insert(v)` can hand
  // out `slot` and code can write `slot[1]`. Iterates in insertion order.
  class HashMap {
    constructor() {
      this.m = new Map();
    }
    static from(pairs) {
      const h = new HashMap();
      for (const [k, v] of pairs) h.$insert(k, v);
      return h;
    }
    // Returns the previous value, or null (`None`).
    $insert(k, v) {
      const kk = key(k);
      const slot = this.m.get(kk);
      if (slot) {
        const old = slot[1];
        slot[1] = v;
        return some(old);
      }
      this.m.set(kk, [k, v]);
      return null;
    }
    $get(k) {
      const slot = this.m.get(key(k));
      return slot ? some(slot[1]) : null;
    }
    $containsKey(k) {
      return this.m.has(key(k));
    }
    // `get_key_value(&k)` / `remove_entry(&k)`: the stored key with its value.
    $getKeyValue(k) {
      const slot = this.m.get(key(k));
      return slot ? [slot[0], slot[1]] : null;
    }
    $removeEntry(k) {
      const slot = this.m.get(key(k));
      if (!slot) return null;
      this.$remove(k);
      return [slot[0], slot[1]];
    }
    $remove(k) {
      const kk = key(k);
      const slot = this.m.get(kk);
      if (!slot) return null;
      this.m.delete(kk);
      return some(slot[1]);
    }
    $len() {
      return this.m.size;
    }
    $clear() {
      this.m.clear();
    }
    $clone() {
      const h = new HashMap();
      for (const [kk, [k, v]] of this.m) h.m.set(kk, [clone(k), clone(v)]);
      return h;
    }
    $slots() {
      return this.m.values();
    }
    // `retain(|k, v| ..)`: `keep` gets the slot so the closure can write `v`.
    $retain(keep) {
      for (const [kk, slot] of this.m) if (!keep(slot)) this.m.delete(kk);
    }
    $extend(pairs) {
      for (const [k, v] of pairs) this.$insert(k, v);
    }
    // `drain()`: the `(k, v)` pairs (the slots, no longer the map's), the map emptied at once.
    $drain() {
      const slots = [...this.m.values()];
      this.m.clear();
      return Iter.of(function* () {
        yield* slots;
      });
    }
    // Fresh `(k, v)` pairs, so callers can't disturb the slots.
    $entries() {
      const m = this.m;
      return Iter.of(function* () {
        for (const [k, v] of m.values()) yield [k, v];
      });
    }
    $keys() {
      const m = this.m;
      return Iter.of(function* () {
        for (const slot of m.values()) yield slot[0];
      });
    }
    $values() {
      const m = this.m;
      return Iter.of(function* () {
        for (const slot of m.values()) yield slot[1];
      });
    }
  }

  // Rust's iterator adapters over a JS iterator. Lazy like Rust's: nothing runs until a consumer
  // (`$collectVec`, `$sum`, `for...of`, ...) pulls items. Methods are `$`-prefixed for Bitburner's
  // RAM calculator.
  // Adapter iterators. Classes rather than generators: V8 runs them several times faster.
  const DONE = Object.freeze({ value: undefined, done: true });
  class AdapterIt {
    [Symbol.iterator]() {
      return this;
    }
  }
  class MapIt extends AdapterIt {
    constructor(src, f) {
      super();
      this.src = src;
      this.f = f;
    }
    next() {
      const r = this.src.next();
      return r.done ? r : { value: this.f(r.value), done: false };
    }
  }
  class FilterIt extends AdapterIt {
    constructor(src, p) {
      super();
      this.src = src;
      this.p = p;
    }
    next() {
      for (;;) {
        const r = this.src.next();
        if (r.done || this.p(r.value)) return r;
      }
    }
  }
  class FilterMapIt extends AdapterIt {
    constructor(src, f) {
      super();
      this.src = src;
      this.f = f;
    }
    next() {
      for (;;) {
        const r = this.src.next();
        if (r.done) return r;
        const y = this.f(r.value);
        if (y != null) return { value: y, done: false };
      }
    }
  }
  class EnumerateIt extends AdapterIt {
    constructor(src) {
      super();
      this.src = src;
      this.i = 0;
    }
    next() {
      const r = this.src.next();
      return r.done ? r : { value: [this.i++, r.value], done: false };
    }
  }
  class TakeIt extends AdapterIt {
    constructor(src, n) {
      super();
      this.src = src;
      this.n = n;
    }
    next() {
      if (this.n <= 0) return DONE;
      this.n--;
      return this.src.next();
    }
  }
  class SkipIt extends AdapterIt {
    constructor(src, n) {
      super();
      this.src = src;
      this.n = n;
    }
    next() {
      // (Over an array: jump.)
      if (this.n > 0 && this.src instanceof ArrayIt) {
        const src = this.src;
        src.i = Math.min(src.end, src.i + this.n);
        this.n = 0;
      }
      while (this.n > 0) {
        this.n--;
        const r = this.src.next();
        if (r.done) return r;
      }
      return this.src.next();
    }
  }
  // A Vec's `iter()`: `a[i..end)`, by index.
  class ArrayIt extends AdapterIt {
    constructor(a, i, end) {
      super();
      this.a = a;
      this.i = i;
      this.end = end;
    }
    next() {
      if (this.i < this.end) return { value: this.a[this.i++], done: false };
      return DONE;
    }
  }
  // `iter().rev()` of an `ArrayIt`: its items from the back (`next_back`: the source keeps the
  // front, as `by_ref().rev()` needs).
  class ArrayRevIt extends AdapterIt {
    constructor(src) {
      super();
      this.src = src;
    }
    next() {
      const src = this.src;
      if (src.end > src.i) return { value: src.a[--src.end], done: false };
      return DONE;
    }
  }
  // `windows(n)` / `chunks(n)` / `chunks_exact(n)`: each item is a copy of `a[i..i + n)`, `step`
  // apart; `exact` drops a short last one.
  class SlicesIt extends AdapterIt {
    constructor(a, n, step, exact) {
      super();
      this.a = a;
      this.n = n;
      this.step = step;
      this.exact = exact;
      this.i = 0;
    }
    next() {
      const i = this.i;
      const end = i + this.n;
      if (i >= this.a.length || (this.exact && end > this.a.length)) return DONE;
      this.i += this.step;
      return { value: this.a.slice(i, end), done: false };
    }
    // `chunks_exact(n).remainder()`: the elements past the last whole chunk.
    remainder() {
      return this.a.slice(this.a.length - (this.a.length % this.n));
    }
  }
  class StepByIt extends AdapterIt {
    constructor(src, n) {
      super();
      this.src = src;
      this.n = n;
      this.first = true;
    }
    next() {
      if (this.first) {
        this.first = false;
        return this.src.next();
      }
      // (Over an array: jump.)
      if (this.src instanceof ArrayIt) {
        const src = this.src;
        src.i = Math.min(src.end, src.i + this.n - 1);
        return src.next();
      }
      // (Over a range: jump, so `(0..1e12).step_by(1e9)` takes 1000 steps, not 1e12.)
      if (this.src instanceof RangeIt) {
        const r = this.src;
        const skip = typeof r.one === "bigint" ? BigInt(this.n - 1) : (this.n - 1) * r.one;
        r.i = r.end - r.i <= skip ? r.end : r.i + skip;
        return r.next();
      }
      for (let k = 1; k < this.n; k++) {
        const r = this.src.next();
        if (r.done) return r;
      }
      return this.src.next();
    }
  }
  class TakeWhileIt extends AdapterIt {
    constructor(src, p) {
      super();
      this.src = src;
      this.p = p;
      this.over = false;
    }
    next() {
      if (this.over) return DONE;
      const r = this.src.next();
      if (r.done) return r;
      if (!this.p(r.value)) {
        this.over = true;
        return DONE;
      }
      return r;
    }
  }
  class SkipWhileIt extends AdapterIt {
    constructor(src, p) {
      super();
      this.src = src;
      this.p = p;
      this.skipping = true;
    }
    next() {
      if (this.skipping) {
        for (;;) {
          const r = this.src.next();
          if (r.done) return r;
          if (!this.p(r.value)) {
            this.skipping = false;
            return r;
          }
        }
      }
      return this.src.next();
    }
  }
  class ZipIt extends AdapterIt {
    constructor(a, b) {
      super();
      this.a = a;
      this.b = b;
    }
    next() {
      const x = this.a.next();
      if (x.done) return x;
      const y = this.b.next();
      if (y.done) return y;
      return { value: [x.value, y.value], done: false };
    }
  }
  class ChainIt extends AdapterIt {
    constructor(a, other) {
      super();
      this.a = a;
      this.other = other;
      this.second = null;
    }
    next() {
      if (this.second === null) {
        const r = this.a.next();
        if (!r.done) return r;
        this.second = items(this.other)[Symbol.iterator]();
      }
      return this.second.next();
    }
  }
  // `flat_map` / `flatten`: each item's items in turn.
  class FlatMapIt extends AdapterIt {
    constructor(src, f) {
      super();
      this.src = src;
      this.f = f;
      this.inner = null;
    }
    next() {
      for (;;) {
        if (this.inner !== null) {
          const r = this.inner.next();
          if (!r.done) return r;
          this.inner = null;
        }
        const o = this.src.next();
        if (o.done) return o;
        this.inner = items(this.f(o.value))[Symbol.iterator]();
      }
    }
  }
  // An integer range (`lo..hi`, `lo..=hi`, `lo..`: `end` is Infinity); BigInts for `i128`.
  class RangeIt extends AdapterIt {
    // (`hi` and `incl`: the bounds as written, for a range used as a value; `open`: no start,
    // `..hi` / `..`, which counts from 0.)
    constructor(i, end, one, hi, incl, open = false) {
      super();
      this.i = i;
      this.end = end;
      this.one = one;
      this.hi = hi;
      this.incl = incl;
      this.open = open;
    }
    // The `kind` your impls of its range type are registered under (`$extReg`).
    get kind() {
      if (this.open) return this.hi === null ? "rangefull" : this.incl ? "rangetoincl" : "rangeto";
      return this.hi === null ? "rangefrom" : this.incl ? "rangeincl" : "range";
    }
    next() {
      if (this.i < this.end) {
        const v = this.i;
        this.i += this.one;
        return { value: v, done: false };
      }
      return DONE;
    }
  }

  class Iter {
    constructor(it) {
      this.it = it;
    }
    static of(gen) {
      return new Iter(gen());
    }
    [Symbol.iterator]() {
      return this.it;
    }
    // ---- adapters
    $map(f) {
      return new Iter(new MapIt(this.it, f));
    }
    $filter(p) {
      return new Iter(new FilterIt(this.it, p));
    }
    $filterMap(f) {
      return new Iter(new FilterMapIt(this.it, f));
    }
    $flatMap(f) {
      return new Iter(new FlatMapIt(this.it, f));
    }
    $flatten() {
      return new Iter(new FlatMapIt(this.it, (x) => x));
    }
    $enumerate() {
      return new Iter(new EnumerateIt(this.it));
    }
    $zip(other) {
      return new Iter(new ZipIt(this.it, items(other)[Symbol.iterator]()));
    }
    $chain(other) {
      return new Iter(new ChainIt(this.it, other));
    }
    // Repeats what the source yields; the source is read once and remembered.
    $cycle() {
      const src = this.it;
      return Iter.of(function* () {
        const seen = [];
        for (const x of iterable(src)) {
          seen.push(x);
          yield x;
        }
        if (seen.length === 0) return;
        for (;;) yield* seen;
      });
    }
    // Collects first: JS iterators only go forward. (A custom iterator with its own
    // `next_back` runs backwards through it.)
    $rev() {
      const it = this.it;
      if (it instanceof ArrayIt) return new Iter(new ArrayRevIt(it));
      if (it instanceof ArrayRevIt) return new Iter(it.src);
      const back = ownMethod(this.src, "next_back");
      if (back !== null) {
        this.src = null;
        return Iter.of(function* () {
          for (;;) {
            const v = back();
            if (v == null) return;
            yield v;
          }
        });
      }
      const all = Array.from(iterable(this.it));
      return Iter.of(function* () {
        for (let i = all.length - 1; i >= 0; i--) yield all[i];
      });
    }
    $take(n) {
      return new Iter(new TakeIt(this.it, n));
    }
    $skip(n) {
      return new Iter(new SkipIt(this.it, n));
    }
    $stepBy(n) {
      if (n === 0) throw new CarciniPanic("assertion failed: step != 0", "<std>");
      return new Iter(new StepByIt(this.it, n));
    }
    // `scan(init, |state, x| ..)`: `boxed` when the state is a number, string, .. (the closure
    // gets a cell); stops at the first `None`.
    $scan(init, f, boxed) {
      const src = this.it;
      return Iter.of(function* () {
        const state = boxed ? [init] : init;
        for (const x of iterable(src)) {
          const y = f(state, x);
          if (y == null) return;
          yield y;
        }
      });
    }
    $mapWhile(f) {
      const src = this.it;
      return Iter.of(function* () {
        for (const x of iterable(src)) {
          const y = f(x);
          if (y == null) return;
          yield y;
        }
      });
    }
    $takeWhile(p) {
      return new Iter(new TakeWhileIt(this.it, p));
    }
    $skipWhile(p) {
      return new Iter(new SkipWhileIt(this.it, p));
    }
    // A range value: `{:?}` (`0..5`, `1..=3`) and `==`. Another iterator shows what's left, as
    // `slice::Iter` does (`Iter([1, 2])`); like `$clone`, it reads the source once.
    $dbg(f = rt.$dbg, name = "Iter") {
      const r = this.it;
      if (!(r instanceof RangeIt)) {
        const all = Array.from(iterable(r));
        this.it = all[Symbol.iterator]();
        return `${name}([${all.map((x) => f(x)).join(", ")}])`;
      }
      return `${r.open ? "" : f(r.i)}${r.incl ? "..=" : ".."}${r.hi === null ? "" : f(r.hi)}`;
    }
    $eq(o) {
      const a = this.it;
      const b = o?.it;
      return a instanceof RangeIt && b instanceof RangeIt && a.i === b.i && a.hi === b.hi && a.incl === b.incl && a.open === b.open;
    }
    // `it.clone()`: an iterator over what's left, independent of this one.
    $clone() {
      if (this.it instanceof RangeIt) {
        const r = this.it;
        return new Iter(new RangeIt(r.i, r.end, r.one, r.hi, r.incl, r.open));
      }
      const all = Array.from(iterable(this.it));
      this.it = all[Symbol.iterator]();
      return new Iter(all.slice()[Symbol.iterator]());
    }
    // `peekable()`: this iterator, with one item of lookahead for `peek` and `next_if`.
    $peekable() {
      if (!(this.it instanceof PeekIt)) this.it = new PeekIt(this.it);
      return this;
    }
    $peek() {
      return this.$peekable().it.peek();
    }
    // `peek_mut()` of numbers, strings or bools: `null`, or a slot over the buffered item.
    $peekMut() {
      const p = this.$peekable().it;
      if (p.peek() === null) return null;
      return {
        get 1() {
          return p.v;
        },
        set 1(x) {
          p.v = x;
        },
      };
    }
    $nextIf(p) {
      const x = this.$peek();
      if (x === null || !p(unsome(x))) return null;
      return this.$next();
    }
    // Double-ended steps collect what's left first: JS iterators only go forward.
    $nextBack() {
      const all = Array.from(iterable(this.it));
      if (all.length === 0) return null;
      const x = all.pop();
      this.it = all[Symbol.iterator]();
      return some(x);
    }
    $nthBack(n) {
      const all = Array.from(iterable(this.it));
      if (n >= all.length) {
        this.it = [][Symbol.iterator]();
        return null;
      }
      const x = all[all.length - 1 - n];
      this.it = all.slice(0, all.length - 1 - n)[Symbol.iterator]();
      return some(x);
    }
    // `size_hint()`: exact here (what's left is counted), or a custom iterator's own (`(0,
    // None)` when it has none).
    $sizeHint() {
      if (this.src != null) {
        const hint = ownMethod(this.src, "size_hint");
        return hint !== null ? hint() : [0, null];
      }
      const n = this.$len();
      return [n, n];
    }
    // `len()`: how many are left, without using them up (a custom iterator's `size_hint`).
    $len() {
      const hint = ownMethod(this.src, "size_hint");
      if (hint !== null) return hint()[0];
      const all = Array.from(iterable(this.it));
      this.it = all[Symbol.iterator]();
      return all.length;
    }
    $inspect(f) {
      const src = this.it;
      return Iter.of(function* () {
        for (const x of iterable(src)) {
          f(x);
          yield x;
        }
      });
    }
    // ---- consumers
    $next() {
      const r = this.it.next();
      return r.done ? null : some(r.value);
    }
    // `chars.as_str()`: the chars left, as a string (the iterator goes on over them).
    $asStr() {
      const s = [...iterable(this.it)].join("");
      this.it = s[Symbol.iterator]();
      return s;
    }
    // `c.to_uppercase().to_string()`, and `{}` of it.
    $toString() {
      return this.$asStr();
    }
    toString() {
      return this.$asStr();
    }
    $rfind(p) {
      const all = Array.from(iterable(this.it));
      for (let i = all.length - 1; i >= 0; i--) if (p(all[i])) return some(all[i]);
      return null;
    }
    $rfold(acc, f) {
      const all = Array.from(iterable(this.it));
      for (let i = all.length - 1; i >= 0; i--) acc = f(acc, all[i]);
      return acc;
    }
    // `try_fold` / `try_for_each`: stops at the first `Err` (or `None`, for `option`).
    $tryFold(acc, f, option, fold) {
      for (const x of iterable(this.it)) {
        const r = fold ? f(acc, x) : f(x);
        if (option ? r == null : r.$tag === 1) return r;
        acc = option ? r : r[0];
      }
      return option ? acc : new Res(0, acc);
    }
    // Lexicographic comparison with another iterable (`eqOnly`: just equality, so floats and
    // unordered items compare too).
    $cmpWith(other, eqOnly) {
      const a = Array.from(iterable(this.it));
      const b = Array.from(items(other));
      if (eqOnly) return eq(a, b) ? 0 : 1;
      return cmp(a, b);
    }
    $remainder() {
      return this.it.remainder();
    }
    $isSorted() {
      return this.$isSortedBy((a, b) => cmp(a, b) <= 0);
    }
    $isSortedByKey(f) {
      return this.$map(f).$isSorted();
    }
    $isSortedBy(le) {
      let prev;
      let first = true;
      for (const x of iterable(this.it)) {
        if (!first && !le(prev, x)) return false;
        prev = x;
        first = false;
      }
      return true;
    }
    $count() {
      const it = this.it;
      if (it instanceof ArrayIt) {
        const n = it.end - it.i;
        it.i = it.end;
        return n;
      }
      let n = 0;
      for (const _ of iterable(it)) n++;
      return n;
    }
    $last() {
      const it = this.it;
      if (it instanceof ArrayIt) {
        if (it.i >= it.end) return null;
        it.i = it.end;
        return some(it.a[it.end - 1]);
      }
      let last = null;
      let any = false;
      for (const x of iterable(it)) {
        last = x;
        any = true;
      }
      return any ? some(last) : null;
    }
    $nth(n) {
      const it = this.it;
      if (it instanceof ArrayIt) {
        const j = it.i + n;
        if (j >= it.end) {
          it.i = it.end;
          return null;
        }
        it.i = j + 1;
        return some(it.a[j]);
      }
      let i = 0;
      for (const x of iterable(it)) if (i++ === n) return some(x);
      return null;
    }
    $sum(op, start) {
      return this.$fold(start, op);
    }
    $product(op, start) {
      return this.$fold(start, op);
    }
    // Ties: `min` keeps the first, `max` the last, as in Rust.
    $min() {
      return this.$minBy(cmp);
    }
    $max() {
      return this.$maxBy(cmp);
    }
    $minBy(f) {
      let best = null, first = true;
      for (const x of iterable(this.it)) {
        if (first || f(x, best) < 0) best = x;
        first = false;
      }
      return first ? null : some(best);
    }
    $maxBy(f) {
      let best = null, first = true;
      for (const x of iterable(this.it)) {
        if (first || f(x, best) >= 0) best = x;
        first = false;
      }
      return first ? null : some(best);
    }
    $minByKey(f) {
      let best = null, bestKey, first = true;
      for (const x of iterable(this.it)) {
        const k = f(x);
        if (first || cmp(k, bestKey) < 0) [best, bestKey] = [x, k];
        first = false;
      }
      return first ? null : some(best);
    }
    $maxByKey(f) {
      let best = null, bestKey, first = true;
      for (const x of iterable(this.it)) {
        const k = f(x);
        if (first || cmp(k, bestKey) >= 0) [best, bestKey] = [x, k];
        first = false;
      }
      return first ? null : some(best);
    }
    $any(p) {
      for (const x of iterable(this.it)) if (p(x)) return true;
      return false;
    }
    $all(p) {
      for (const x of iterable(this.it)) if (!p(x)) return false;
      return true;
    }
    $find(p) {
      for (const x of iterable(this.it)) if (p(x)) return some(x);
      return null;
    }
    // `unzip()`: pairs into two Vecs.
    $unzip() {
      const a = [];
      const b = [];
      for (const [x, y] of iterable(this.it)) {
        a.push(x);
        b.push(y);
      }
      return [a, b];
    }
    $partition(p) {
      const yes = [];
      const no = [];
      for (const x of iterable(this.it)) (p(x) ? yes : no).push(x);
      return [yes, no];
    }
    // From the end; the index counts from the front, as in Rust.
    $rposition(p) {
      const all = Array.from(iterable(this.it));
      for (let i = all.length - 1; i >= 0; i--) if (p(all[i])) return i;
      return null;
    }
    $position(p) {
      let i = 0;
      for (const x of iterable(this.it)) {
        if (p(x)) return i;
        i++;
      }
      return null;
    }
    $findMap(f) {
      for (const x of iterable(this.it)) {
        const y = f(x);
        if (y != null) return y;
      }
      return null;
    }
    $fold(acc, f) {
      for (const x of iterable(this.it)) acc = f(acc, x);
      return acc;
    }
    $reduce(f) {
      const it = this.it;
      const first = it.next();
      if (first.done) return null;
      let acc = first.value;
      for (const x of iterable(it)) acc = f(acc, x);
      return some(acc);
    }
    $forEach(f) {
      for (const x of iterable(this.it)) f(x);
    }
    $collectVec() {
      const it = this.it;
      if (it instanceof ArrayIt) {
        const out = it.a.slice(it.i, it.end);
        it.i = it.end;
        return out;
      }
      return Array.from(iterable(it));
    }
    $collectDeque() {
      return Deque.from(iterable(this.it));
    }
    $collectSet() {
      return HashSet.from(iterable(this.it));
    }
    $collectMap() {
      return HashMap.from(iterable(this.it));
    }
    $collectBTreeSet() {
      return BTreeSet.from(iterable(this.it));
    }
    $collectBTreeMap() {
      return BTreeMap.from(iterable(this.it));
    }
    $collectIndexSet() {
      return IndexSet.from(iterable(this.it));
    }
    $collectIndexMap() {
      return IndexMap.from(iterable(this.it));
    }
    $collectHeap() {
      const a = Array.from(iterable(this.it));
      heapRebuild(a);
      return a;
    }
    $collectString() {
      let s = "";
      for (const x of iterable(this.it)) s += x;
      return s;
    }
    // `collect::<Result<C, E>>()`: the first `Err` stops it; otherwise `Ok` of the values
    // collected with `into` (`$collectVec`, ...).
    $collectResult(into) {
      const ok = [];
      for (const x of iterable(this.it)) {
        if (x.$tag === 1) return x;
        ok.push(x[0]);
      }
      return new Res(0, new Iter(ok[Symbol.iterator]())[into]());
    }
    // `collect::<Option<C>>()`: the first `None` stops it.
    $collectOption(into) {
      const some = [];
      for (const x of iterable(this.it)) {
        if (x == null) return null;
        some.push(x);
      }
      return new Iter(some[Symbol.iterator]())[into]();
    }
  }

  // `peekable()`'s source: a JS iterator with one item of lookahead.
  class PeekIt {
    constructor(it) {
      this.it = it;
      this.has = false;
      this.v = undefined;
    }
    [Symbol.iterator]() {
      return this;
    }
    next() {
      if (this.has) {
        this.has = false;
        return { done: false, value: this.v };
      }
      return this.it.next();
    }
    peek() {
      if (!this.has) {
        const r = this.it.next();
        if (r.done) return null;
        this.has = true;
        this.v = r.value;
      }
      return some(this.v);
    }
  }

  // A class's `std::ops` method by trait method name (`index_mut`): its key is numbered among
  // the type's operator impls (`index_mut$1`), so found by prefix once per class.
  const OP_FNS = new WeakMap();
  function opFn(c, name) {
    const proto = c == null ? null : Object.getPrototypeOf(c);
    if (proto == null || typeof proto !== "object") return undefined;
    let fns = OP_FNS.get(proto);
    if (!fns) OP_FNS.set(proto, (fns = {}));
    if (!(name in fns)) {
      let found;
      for (let p = proto; p && p !== Object.prototype && !found; p = Object.getPrototypeOf(p)) {
        const k = Object.getOwnPropertyNames(p).find((k) => k === name || k.startsWith(name + "$"));
        if (k && typeof p[k] === "function") found = p[k];
      }
      fns[name] = found;
    }
    return fns[name];
  }

  const NUM_TYPES = {};
  const NUM_EXT = new WeakMap();
  // `{:p}`'s made-up addresses: one per object, a new one per formatting of anything else.
  const PTRS = new WeakMap();
  let nextPtr = 0;
  const OP_METHODS = {
    "+": "add",
    "-": "sub",
    "*": "mul",
    "/": "div",
    "%": "rem",
    "&": "bitand",
    "|": "bitor",
    "^": "bitxor",
    "<<": "shl",
    ">>": "shr",
  };
  // `a & b`, `a << n` (and `|`, `^`, `>>`) on integers of kind `k` (or unknown: `null`). Shift
  // amounts are masked to the width, as Rust does in release builds.
  function bitOp(op, a, b, k) {
    if (typeof a === "bigint") {
      const n = BigInt(Number(b) & ((k ? k.bits : 128) - 1));
      const r = op === "&" ? a & b : op === "|" ? a | b : op === "^" ? a ^ b : op === "<<" ? a << n : a >> n;
      return k ? rt.$w128(r, k.signed, k.bits) : r;
    }
    const bits = k ? k.bits : 64;
    if (op === "<<" || op === ">>") {
      const n = Number(b) & (bits - 1);
      if (bits === 64) return op === "<<" ? rt.$shl64(a, n, k.signed) : rt.$shr64(a, n);
      return op === "<<" ? wrapInt(a << n, k) : k.signed ? a >> n : a >>> n;
    }
    if (bits === 64) return rt.$bit64(a, b, op, k ? k.signed : true);
    return wrapInt(op === "&" ? a & b : op === "|" ? a | b : a ^ b, k);
  }
  function divZero(op) {
    return op === "/" ? "attempt to divide by zero" : "attempt to calculate the remainder with a divisor of zero";
  }
  // An integer result wrapped to its type's width (`usize` / `isize` are JS numbers: as is).
  function wrapInt(r, k) {
    if (k.bits === 32) return k.signed ? r | 0 : r >>> 0;
    if (k.bits === 8 || k.bits === 16) {
      const m = 2 ** k.bits;
      r = ((r % m) + m) % m;
      return k.signed && r >= m / 2 ? r - m : r;
    }
    return r;
  }

  // A JS iterator as something `for...of` accepts without restarting it.
  function iterable(it) {
    return { [Symbol.iterator]: () => it };
  }

  // A struct's own method by its Rust name (`write`, or `write$` where Bitburner charges RAM for
  // the name), bound; null if it has none.
  function ownMethod(o, name) {
    if (o == null || typeof o !== "object") return null;
    const f = typeof o[name] === "function" ? o[name] : o[name + "$"];
    return typeof f === "function" ? f.bind(o) : null;
  }

  // What Rust's `IntoIterator` gives for a value: maps yield `(k, v)` pairs, strings their chars,
  // options zero or one item.
  function items(x) {
    if (x instanceof HashMap) return x.$entries();
    if (x == null) return [];
    if (typeof x === "object" || typeof x === "string") return x;
    return [x];
  }

  // Lines as Rust's `str::lines` splits them: on `\n`, dropping a trailing `\r`, with no final
  // empty line.
  function lines(s) {
    if (s === "") return [];
    const parts = s.split("\n");
    if (parts[parts.length - 1] === "") parts.pop();
    return parts.map((l) => (l.endsWith("\r") ? l.slice(0, -1) : l));
  }


  // Rust's `Result<T, E>`: `$tag` 0 is `Ok`, 1 is `Err`; the value is at `[0]`.
  class Res {
    constructor(tag, v) {
      this.$tag = tag;
      this[0] = v;
    }
    $eq(o) {
      return this.$tag === o.$tag && eq(this[0], o[0]);
    }
    $cmp(o) {
      if (this.$tag !== o.$tag) return this.$tag < o.$tag ? -1 : 1;
      return cmp(this[0], o[0]);
    }
    $clone() {
      return new Res(this.$tag, clone(this[0]));
    }
    $dbg() {
      return `${this.$tag ? "Err" : "Ok"}(${rt.$dbg(this[0])})`;
    }
  }

  // The error values of `str::parse`, with rustc's `Debug` and `Display` text.
  const PARSE_MESSAGES = {
    "ParseIntError:Empty": "cannot parse integer from empty string",
    "ParseIntError:InvalidDigit": "invalid digit found in string",
    "ParseIntError:PosOverflow": "number too large to fit in target type",
    "ParseIntError:NegOverflow": "number too small to fit in target type",
    "ParseFloatError:Empty": "cannot parse float from empty string",
    "ParseFloatError:Invalid": "invalid float literal",
    "ParseBoolError:": "provided string was not `true` or `false`",
    "ParseCharError:EmptyString": "cannot parse char from empty string",
    "ParseCharError:TooManyChars": "too many characters in string",
    "TryFromIntError:PosOverflow": "number too large to fit in target type",
    "TryFromIntError:NegOverflow": "number too small to fit in target type",
    "TryFromSliceError:": "could not convert slice to array",
    "FromUtf16Error:LoneSurrogate": "invalid utf-16: lone surrogate found",
    "CharTryFromError:": "converted integer out of range for `char`",
    "TryFromCharError:": "unicode code point out of range",
    "TryReserveError:": "memory allocation failed because the computed capacity exceeded the collection's maximum",
    "Error:": "an error occurred when formatting an argument",
    "BorrowError:": "RefCell already mutably borrowed",
    "BorrowMutError:": "RefCell already borrowed",
  };
  class ParseError {
    constructor(type, kind) {
      this.type = type;
      this.kind = kind;
    }
    $eq(o) {
      return this.type === o.type && this.kind === o.kind;
    }
    $dbg() {
      if (this.type === "TryFromIntError") return `TryFromIntError(${this.kind})`;
      if (this.type === "TryFromSliceError" || this.type === "CharTryFromError" || this.type === "TryFromCharError") return `${this.type}(())`;
      return this.kind ? `${this.type} { kind: ${this.kind} }` : this.type;
    }
    toString() {
      return PARSE_MESSAGES[`${this.type}:${this.kind}`];
    }
  }

  // `str::from_utf8`'s error: where the valid prefix ends, and how long the bad sequence there
  // is (`null` when the bytes end inside a character), found as std finds them.
  class Utf8Error {
    constructor(bytes, owned) {
      this.type = "Utf8Error";
      if (owned) this.bytes = bytes;
      this.valid_up_to = bytes.length;
      this.error_len = null;
      const n = bytes.length;
      let i = 0;
      scan: while (i < n) {
        const b = bytes[i];
        if (b < 0x80) {
          i++;
          continue;
        }
        let w = 0;
        let lo = 0x80;
        let hi = 0xbf;
        if (b >= 0xc2 && b <= 0xdf) w = 2;
        else if (b >= 0xe0 && b <= 0xef) {
          w = 3;
          if (b === 0xe0) lo = 0xa0;
          if (b === 0xed) hi = 0x9f;
        } else if (b >= 0xf0 && b <= 0xf4) {
          w = 4;
          if (b === 0xf0) lo = 0x90;
          if (b === 0xf4) hi = 0x8f;
        }
        this.valid_up_to = i;
        if (!w) {
          this.error_len = 1;
          break;
        }
        for (let k = 1; k < w; k++) {
          if (i + k >= n) break scan;
          const c = bytes[i + k];
          if (c < (k === 1 ? lo : 0x80) || c > (k === 1 ? hi : 0xbf)) {
            this.error_len = k;
            break scan;
          }
        }
        i += w;
        this.valid_up_to = n;
      }
    }
    $eq(o) {
      return this.valid_up_to === o.valid_up_to && this.error_len === o.error_len;
    }
    $dbg() {
      const len = this.error_len === null ? "None" : `Some(${this.error_len})`;
      const e = `Utf8Error { valid_up_to: ${this.valid_up_to}, error_len: ${len} }`;
      return this.bytes ? `FromUtf8Error { bytes: [${this.bytes.join(", ")}], error: ${e} }` : e;
    }
    toString() {
      return this.error_len === null
        ? `incomplete utf-8 byte sequence from index ${this.valid_up_to}`
        : `invalid utf-8 sequence of ${this.error_len} bytes from index ${this.valid_up_to}`;
    }
  }

  // `char::decode_utf16`'s error: the unpaired surrogate (`unpaired_surrogate()`).
  class DecodeUtf16Error {
    constructor(code) {
      this.type = "DecodeUtf16Error";
      this.code = code;
    }
    $eq(o) {
      return this.code === o.code;
    }
    $dbg() {
      return `DecodeUtf16Error { code: ${this.code} }`;
    }
    toString() {
      return `unpaired surrogate found: ${this.code.toString(16)}`;
    }
  }

  // `RefCell<T>` / `Cell<T>`: the value at `.v`. No `$clone`: cloning an `Rc<RefCell<T>>` shares it.
  // `b` counts the `Ref`s held by `let` bindings, or is -1 while a `RefMut` is (`$refHold`).
  class RefCell {
    constructor(v, kind) {
      this.v = v;
      this.kind = kind;
      this.b = 0;
    }
    $eq(o) {
      return eq(this.v, o.v);
    }
  }

  function assertFailed(op, a, b, location, debug, message) {
    const fmt = debug ?? String;
    const extra = message ? `: ${message()}` : "";
    throw new CarciniPanic(
      `assertion \`left ${op} right\` failed${extra}
  left: ${fmt(a)}
 right: ${fmt(b)}`,
      location,
    );
  }


  // `std::cmp::Reverse(x)`: orders the other way round (a min-heap from a `BinaryHeap`).
  class Reverse {
    constructor(v) {
      this[0] = v;
    }
    $eq(o) {
      return eq(this[0], o[0]);
    }
    $cmp(o) {
      return cmp(o[0], this[0]);
    }
    $clone() {
      return new Reverse(clone(this[0]));
    }
    $dbg(f) {
      return `Reverse(${(f ?? rt.$dbgAny)(this[0])})`;
    }
  }

  // indexmap's IndexMap / IndexSet: the hash classes (a JS `Map` keeps insertion order) with
  // positions. `swap_remove` (what `remove` is) moves the last entry into the hole; `shift_remove`
  // closes it up. `valueOf` is a slot's key (`[k, v]` for a map, the item for a set).
  function indexed(Base, keyOfSlot, out) {
    return class extends Base {
      static from(items) {
        const h = new this();
        if (keyOfSlot === null) for (const x of items) h.$insert(x);
        else for (const [k, v] of items) h.$insert(k, v);
        return h;
      }
      $clone() {
        const h = new this.constructor();
        for (const [kk, slot] of this.m) h.m.set(kk, clone(slot));
        return h;
      }
      $slotAt(i) {
        if (i < 0 || i >= this.m.size) return undefined;
        let j = 0;
        for (const s of this.m.values()) if (j++ === i) return s;
        return undefined;
      }
      $getIndex(i) {
        const s = this.$slotAt(i);
        return s === undefined ? null : out(s);
      }
      // `map[i]` / `set[i]` (indexmap panics like a slice), and `map[i]` as a place.
      $at(i) {
        const s = this.$slotAt(i);
        if (s === undefined) throw new CarciniPanic(`index out of bounds: the len is ${this.m.size} but the index is ${i}`);
        return keyOfSlot === null ? s : s[1];
      }
      $atMut(i) {
        const s = this.$slotAt(i);
        if (s === undefined) throw new CarciniPanic(`index out of bounds: the len is ${this.m.size} but the index is ${i}`);
        return {
          get 0() {
            return s[1];
          },
          set 0(v) {
            s[1] = v;
          },
        };
      }
      $getIndexOf(k) {
        const kk = key(k);
        let j = 0;
        for (const x of this.m.keys()) {
          if (x === kk) return j;
          j++;
        }
        return null;
      }
      $first() {
        return this.$getIndex(0);
      }
      $last() {
        return this.$getIndex(this.m.size - 1);
      }
      $pop() {
        return this.$shiftRemoveIndex(this.m.size - 1);
      }
      $shiftRemoveIndex(i) {
        const s = this.$slotAt(i);
        if (s === undefined) return null;
        this.m.delete(key(keyOfSlot === null ? s : s[0]));
        return out(s);
      }
      $swapRemoveIndex(i) {
        const entries = [...this.m];
        if (i < 0 || i >= entries.length) return null;
        const removed = entries[i][1];
        const last = entries.pop();
        if (i < entries.length) entries[i] = last;
        this.m = new Map(entries);
        return out(removed);
      }
      $reverse() {
        this.m = new Map([...this.m].reverse());
      }
      $truncate(n) {
        if (n < this.m.size) this.m = new Map([...this.m].slice(0, n));
      }
      $sortWith(f) {
        this.m = new Map([...this.m].sort((a, b) => f(a[1], b[1])));
      }
    };
  }
  class IndexMap extends indexed(HashMap, (slot) => slot[0], (s) => [s[0], s[1]]) {
    // `get_index_mut(i)`: `[k, v]`, or `[k, slot]` (`asSlot`) for a number, string or bool value,
    // whose `[1]` reads and writes the map's.
    $getIndexMut(i, asSlot) {
      const s = this.$slotAt(i);
      if (s === undefined) return null;
      return asSlot ? [s[0], s] : [s[0], s[1]];
    }
    // `map[&k]` as a place.
    $keyMut(k) {
      const s = this.m.get(key(k));
      if (!s) throw new CarciniPanic("no entry found for key");
      return {
        get 0() {
          return s[1];
        },
        set 0(v) {
          s[1] = v;
        },
      };
    }
    $getFull(k) {
      const i = this.$getIndexOf(k);
      if (i === null) return null;
      const s = this.m.get(key(k));
      return [i, s[0], s[1]];
    }
    $insertFull(k, v) {
      const old = this.$insert(k, v);
      return [this.$getIndexOf(k), old];
    }
    $swapRemove(k) {
      const i = this.$getIndexOf(k);
      return i === null ? null : some(this.$swapRemoveIndex(i)[1]);
    }
    $shiftRemove(k) {
      return this.$remove(k);
    }
    $sortKeys() {
      this.$sortWith((a, b) => cmp(a[0], b[0]));
    }
    $sortBy(f) {
      this.$sortWith((a, b) => f(a[0], a[1], b[0], b[1]));
    }
  }
  class IndexSet extends indexed(HashSet, null, (x) => some(x)) {
    $insertFull(x) {
      const added = this.$insert(x);
      return [this.$getIndexOf(x), added];
    }
    $swapRemove(x) {
      const i = this.$getIndexOf(x);
      if (i === null) return false;
      this.$swapRemoveIndex(i);
      return true;
    }
    $shiftRemove(x) {
      return this.$remove(x);
    }
    $sort() {
      this.$sortWith(cmp);
    }
    $sortBy(f) {
      this.$sortWith(f);
    }
  }

  // Rust's BTreeSet / BTreeMap: the hash classes (for `get` / `contains`) plus `s`, what the Map
  // holds (elements, or `[k, v]` slots) in key order from index `h` on: `pop_first` moves `h`
  // instead of shifting. A single new key goes in by binary search; after `BTREE_BULK` of those
  // between ordered reads (a bulk load) the rest are appended and `s` is sorted at the next read.
  // `out` is what iteration yields for a stored item.
  const BTREE_BULK = 512;
  function btreeSorting(Base, keyOf, out) {
    return class extends Base {
      constructor() {
        super();
        this.s = [];
        this.h = 0;
        this.sorted = true;
        this.pending = 0;
      }
      $sort() {
        if (this.sorted) return;
        if (this.h !== 0) {
          this.s = this.s.slice(this.h);
          this.h = 0;
        }
        this.s.sort((a, b) => cmp(keyOf(a), keyOf(b)));
        this.sorted = true;
        this.pending = 0;
      }
      // The stored items in order, from index 0.
      $items() {
        this.$sort();
        if (this.h !== 0) {
          this.s = this.s.slice(this.h);
          this.h = 0;
        }
        return this.s;
      }
      // The index in `s` of the first item whose key is `>= k` (`> k` when `after`).
      $bound(k, after) {
        const s = this.s;
        let lo = this.h;
        let hi = s.length;
        while (lo < hi) {
          const mid = (lo + hi) >>> 1;
          const c = cmp(keyOf(s[mid]), k);
          if (c < 0 || (after && c === 0)) lo = mid + 1;
          else hi = mid;
        }
        return lo;
      }
      // A new item, just put in the Map.
      $added(x) {
        const s = this.s;
        if (!this.sorted) {
          s.push(x);
          return;
        }
        const k = keyOf(x);
        if (s.length === this.h || cmp(keyOf(s[s.length - 1]), k) < 0) {
          s.push(x);
        } else if (this.h !== 0 && cmp(k, keyOf(s[this.h])) < 0) {
          s[--this.h] = x;
        } else if (++this.pending > BTREE_BULK) {
          this.sorted = false;
          s.push(x);
        } else {
          s.splice(this.$bound(k, false), 0, x);
        }
      }
      // A stored item, just deleted from the Map.
      $removed(x) {
        this.$sort();
        const s = this.s;
        let i = this.$bound(keyOf(x), false);
        if (s[i] !== x) i = s.indexOf(x, this.h);
        if (i === this.h) {
          s[i] = undefined;
          this.h++;
          if (this.h > 64 && this.h * 2 > s.length) {
            this.s = s.slice(this.h);
            this.h = 0;
          }
        } else if (i >= 0) {
          s.splice(i, 1);
        }
      }
      $retain(p) {
        const s = this.$items();
        let j = 0;
        for (let i = 0; i < s.length; i++) {
          const x = s[i];
          if (p(x)) s[j++] = x;
          else this.m.delete(key(keyOf(x)));
        }
        s.length = j;
      }
      $clear() {
        super.$clear();
        this.s = [];
        this.h = 0;
        this.sorted = true;
        this.pending = 0;
      }
      $clone() {
        const c = new this.constructor();
        for (const x of this.$items()) {
          const y = clone(x);
          c.m.set(key(keyOf(y)), y);
          c.s.push(y);
        }
        return c;
      }
      $append(other) {
        this.$extend(Array.from(other));
        other.$clear();
      }
      // The first / last entry, as stored (an element, or a `[k, v]` slot).
      $firstSlot() {
        this.$sort();
        return this.h < this.s.length ? this.s[this.h] : null;
      }
      $lastSlot() {
        this.$sort();
        return this.h < this.s.length ? this.s[this.s.length - 1] : null;
      }
      $popSlot(first) {
        this.$sort();
        if (this.h >= this.s.length) return null;
        const x = first ? this.s[this.h] : this.s[this.s.length - 1];
        this.m.delete(key(keyOf(x)));
        this.$removed(x);
        return x;
      }
      // `range(lo..hi)` / `range(lo..=hi)` / `range(lo..)` / `range(..hi)`: the entries with
      // keys in it, in order (`undefined` for an open end).
      // (`after`: the start is `Excluded`.)
      $range(lo, hi, inclusive, after = false) {
        if (lo !== undefined && hi !== undefined) {
          const c = cmp(lo, hi);
          if (c > 0) {
            throw new CarciniPanic(`range start is greater than range end in ${this.constructor.name}`, "<std>");
          }
          if (c === 0 && after && !inclusive) {
            throw new CarciniPanic(`range start and end are equal and excluded in ${this.constructor.name}`, "<std>");
          }
        }
        this.$sort();
        const i = lo === undefined ? this.h : this.$bound(lo, after);
        const j = hi === undefined ? this.s.length : this.$bound(hi, inclusive);
        const part = i < j ? this.s.slice(i, j) : [];
        return new Iter(new MapIt(part[Symbol.iterator](), out));
      }
      // `range((Bound::Excluded(a), Bound::Included(b)))` (the bound prelude's enum: `$tag` 0
      // `Included`, 1 `Excluded`, 2 `Unbounded`), or a range value (`let r = a..b; m.range(r)`).
      $rangeArg(a) {
        if (Array.isArray(a)) {
          const [s, e] = a;
          const at = (b) => (b.$tag === 2 ? undefined : b[0]);
          return this.$range(at(s), at(e), e.$tag === 0, s.$tag === 1);
        }
        const it = a.it;
        return this.$range(it.open ? undefined : it.i, it.hi ?? undefined, it.incl);
      }
      // Iteration, in key order.
      [Symbol.iterator]() {
        return new MapIt(this.$items()[Symbol.iterator](), out);
      }
    };
  }

  class BTreeSet extends btreeSorting(HashSet, (x) => x, (x) => x) {
    static from(items) {
      const s = new BTreeSet();
      for (const x of items) s.$insert(x);
      return s;
    }
    $insert(x) {
      const k = key(x);
      if (this.m.has(k)) return false;
      this.m.set(k, x);
      this.$added(x);
      return true;
    }
    $remove(x) {
      const k = key(x);
      if (!this.m.has(k)) return false;
      const stored = this.m.get(k);
      this.m.delete(k);
      this.$removed(stored);
      return true;
    }
    [Symbol.iterator]() {
      return this.$items()[Symbol.iterator]();
    }
    $first() {
      return this.$firstSlot();
    }
    $last() {
      return this.$lastSlot();
    }
    $popFirst() {
      return this.$popSlot(true);
    }
    $popLast() {
      return this.$popSlot(false);
    }
    // Set operations yield in order, like a merge of the two sorted sets.
    $union(other) {
      return sortedIter(super.$union(other));
    }
    $intersection(other) {
      return sortedIter(super.$intersection(other));
    }
    $difference(other) {
      return sortedIter(super.$difference(other));
    }
    $symmetricDifference(other) {
      return sortedIter(super.$symmetricDifference(other));
    }
  }

  function sortedIter(it) {
    const all = Array.from(it).sort(cmp);
    return new Iter(all[Symbol.iterator]());
  }

  class BTreeMap extends btreeSorting(HashMap, (slot) => slot[0], (slot) => [slot[0], slot[1]]) {
    static from(pairs) {
      const h = new BTreeMap();
      for (const [k, v] of pairs) h.$insert(k, v);
      return h;
    }
    $insert(k, v) {
      const kk = key(k);
      const slot = this.m.get(kk);
      if (slot) {
        const old = slot[1];
        slot[1] = v;
        return some(old);
      }
      const s = [k, v];
      this.m.set(kk, s);
      this.$added(s);
      return null;
    }
    $remove(k) {
      const kk = key(k);
      const slot = this.m.get(kk);
      if (!slot) return null;
      this.m.delete(kk);
      this.$removed(slot);
      return some(slot[1]);
    }
    $slots() {
      return this.$items()[Symbol.iterator]();
    }
    $entries() {
      return new Iter(this[Symbol.iterator]());
    }
    $keys() {
      return new Iter(new MapIt(this.$items()[Symbol.iterator](), (slot) => slot[0]));
    }
    $values() {
      return new Iter(new MapIt(this.$items()[Symbol.iterator](), (slot) => slot[1]));
    }
    $append(other) {
      this.$extend(Array.from(other.$entries()));
      other.$clear();
    }
    $firstKeyValue() {
      const s = this.$firstSlot();
      return s && [s[0], s[1]];
    }
    $lastKeyValue() {
      const s = this.$lastSlot();
      return s && [s[0], s[1]];
    }
    $popFirst() {
      const s = this.$popSlot(true);
      return s && [s[0], s[1]];
    }
    $popLast() {
      const s = this.$popSlot(false);
      return s && [s[0], s[1]];
    }
  }

  // Rust's BinaryHeap is a JS array kept in std's heap order (same sift algorithms, so `{:?}` and
  // `iter()` show the same order rustc's does).
  function siftUp(a, start, pos) {
    const elem = a[pos];
    while (pos > start) {
      const parent = (pos - 1) >> 1;
      if (cmp(elem, a[parent]) <= 0) break;
      a[pos] = a[parent];
      pos = parent;
    }
    a[pos] = elem;
    return pos;
  }
  function siftDownRange(a, pos, end) {
    const elem = a[pos];
    let child = 2 * pos + 1;
    while (child <= end - 2) {
      if (cmp(a[child], a[child + 1]) <= 0) child += 1;
      if (cmp(elem, a[child]) >= 0) {
        a[pos] = elem;
        return;
      }
      a[pos] = a[child];
      pos = child;
      child = 2 * pos + 1;
    }
    if (child === end - 1 && cmp(elem, a[child]) < 0) {
      a[pos] = a[child];
      pos = child;
    }
    a[pos] = elem;
  }
  function siftDownToBottom(a, pos) {
    const end = a.length;
    const start = pos;
    const elem = a[pos];
    let child = 2 * pos + 1;
    while (child <= end - 2) {
      if (cmp(a[child], a[child + 1]) <= 0) child += 1;
      a[pos] = a[child];
      pos = child;
      child = 2 * pos + 1;
    }
    if (child === end - 1) {
      a[pos] = a[child];
      pos = child;
    }
    a[pos] = elem;
    siftUp(a, start, pos);
  }
  function heapRebuild(a) {
    let n = a.length >> 1;
    while (n > 0) {
      n -= 1;
      siftDownRange(a, n, a.length);
    }
  }
  // std's `rebuild_tail`: after appending items from `start`, rebuild or sift each up.
  function heapRebuildTail(a, start) {
    if (start === a.length) return;
    const tail = a.length - start;
    const log2 = (x) => 31 - Math.clz32(x);
    const rebuild =
      start < tail ? true : a.length <= 2048 ? 2 * a.length < tail * log2(start) : 2 * a.length < tail * 11;
    if (rebuild) heapRebuild(a);
    else for (let i = start; i < a.length; i++) siftUp(a, 0, i);
  }

  // UTF-8 byte offsets of a JS (UTF-16) string. A string's UTF-8 length counts each unit below
  // 0x80 as 1, below 0x800 as 2, a surrogate pair as 4 and any other unit (a lone surrogate too,
  // as `for (const c of s)` sees it) as 3.
  function utf8Len(s) {
    const n = s.length;
    let b = n;
    for (let i = 0; i < n; i++) {
      const c = s.charCodeAt(i);
      if (c < 0x80) continue;
      if (c < 0x800) b += 1;
      else if (c >= 0xd800 && c < 0xdc00 && i + 1 < n && (s.charCodeAt(i + 1) & 0xfc00) === 0xdc00) {
        b += 2;
        i++;
      } else b += 2;
    }
    return b;
  }
  // What `s.len()`, `s.as_bytes()[i]`, `&s[a..b]` and `is_char_boundary` need to know about a
  // string, for the last two long strings seen (strings are values, so a loop over one string
  // passes the same one each time): its UTF-8 length, whether it's ASCII (byte offsets are then
  // UTF-16 offsets), and its bytes once asked for.
  const strInfoMin = 32;
  let strInfoA = { s: "", len: 0, ascii: true, bytes: null };
  let strInfoB = strInfoA;
  function strInfo(s) {
    if (strInfoA.s === s) return strInfoA;
    if (strInfoB.s === s) {
      const t = strInfoA;
      strInfoA = strInfoB;
      strInfoB = t;
      return strInfoA;
    }
    const len = utf8Len(s);
    const info = { s, len, ascii: len === s.length, bytes: null };
    if (s.length >= strInfoMin) {
      strInfoB = strInfoA;
      strInfoA = info;
    }
    return info;
  }
  function strBytes(info) {
    return (info.bytes ??= new TextEncoder().encode(info.s));
  }


  const rt = {
    CarciniPanic,
    CarciniError,
    CarciniExit,
    // `std::env::args()`: hosts set the program's arguments (its name first).
    $setArgs(args) {
      argv = Array.from(args, String);
    },
    $envArgs() {
      return rt.$iter(argv.slice());
    },
    $exit(code) {
      rt.$flush();
      throw new CarciniExit(code);
    },
    $exitWith(code, text, toStdout) {
      write(text, !toStdout);
      rt.$flush();
      const e = new CarciniExit(code);
      e.text = text;
      throw e;
    },
    // The bundled `chrono-tz`, on the host's `Intl` time zone data.
    $tzOffset(zone, ms) {
      if (zone === "UTC") return 0;
      const parts = tzFormat(zone, "en-US").formatToParts(new Date(ms));
      const get = (t) => Number(parts.find((p) => p.type === t)?.value);
      const wall = Date.UTC(get("year"), get("month") - 1, get("day"), get("hour") % 24, get("minute"), get("second"));
      return Math.round((wall - Math.floor(ms / 1000) * 1000) / 1000);
    },
    $tzAbbr(zone, ms) {
      if (zone === "UTC" || zone === "Etc/UTC") return "UTC";
      const d = new Date(ms);
      for (const loc of ["en-US", "en-GB", "en-AU", "en-IN", "ja-JP", "en-NZ", "en-CA"]) {
        const name = new Intl.DateTimeFormat(loc, { timeZone: zone, timeZoneName: "short" })
          .formatToParts(d)
          .find((p) => p.type === "timeZoneName")?.value;
        if (name && /^[A-Z]{2,5}$/.test(name) && !(name === "GMT" && rt.$tzOffset(zone, ms) !== 0)) return name;
      }
      // (The tz database's numeric names: `+04`, `+0530`.)
      const off = rt.$tzOffset(zone, ms);
      // (At offset 0 the tz database's name is GMT: London in winter, Reykjavik, Accra.)
      if (off === 0) return "GMT";
      const a = Math.abs(off);
      const h = String(Math.floor(a / 3600)).padStart(2, "0");
      const m = Math.floor(a / 60) % 60;
      return (off < 0 ? "-" : "+") + h + (m ? String(m).padStart(2, "0") : "");
    },
    $tzValid(zone) {
      try {
        tzFormat(zone, "en-US");
        return zone.includes("/") || zone === "UTC" || zone === "GMT";
      } catch {
        return false;
      }
    },
    $tzIntern(zone) {
      let i = tzNames.indexOf(zone);
      if (i < 0) i = tzNames.push(zone) - 1;
      return i;
    },
    $tzName(id) {
      return tzNames[id];
    },
    // A Netscript `main(ns)` that can exit: a `CarciniExit` ends the script quietly, showing
    // what it was given to print (clap's help or error) in the terminal.
    $nsExit(ns, e) {
      if (e?.name !== "CarciniExit") return false;
      if (e.text) ns.tprint(e.text.replace(/\n$/, ""));
      return true;
    },

    // Output. Bitburner and tests can redirect with $setOutput.
    $setOutput(out, err) {
      outSink = out ?? outSink;
      errSink = err ?? errSink;
      outBuf = errBuf = "";
    },
    $flush() {
      if (outBuf) outSink(outBuf);
      if (errBuf) errSink(errBuf);
      outBuf = errBuf = "";
    },
    $println(s) { write(s + "\n", false); },
    $print(s) { write(s, false); },
    $eprintln(s) { write(s + "\n", true); },
    $eprint(s) { write(s, true); },

    $panic(message, location) {
      throw new CarciniPanic(message, location);
    },
    $assertEq(a, b, location, debug, message) {
      if (!eq(a, b)) assertFailed("==", a, b, location, debug, message);
    },
    $assertNe(a, b, location, debug, message) {
      if (eq(a, b)) assertFailed("!=", a, b, location, debug, message);
    },

    // Integer arithmetic with debug-build overflow checks (`min`/`max` are BigInts).
    $ckArith(a, b, op, min, max, location) {
      let r;
      if (typeof a === "bigint") r = op === "+" ? a + b : op === "-" ? a - b : a * b;
      else {
        r = op === "+" ? a + b : op === "-" ? a - b : a * b;
        // Past 2^53 a JS number is inexact: redo it in BigInt.
        if (!Number.isSafeInteger(r)) {
          const x = BigInt(a);
          const y = BigInt(b);
          const e = op === "+" ? x + y : op === "-" ? x - y : x * y;
          if (e < min || e > max) r = e;
          else return Number(e);
        }
      }
      if (r < min || r > max) {
        const what = op === "+" ? "add" : op === "-" ? "subtract" : "multiply";
        throw new CarciniPanic(`attempt to ${what} with overflow`, location);
      }
      return r;
    },
    $ckDiv(a, b, rem, min, max, location) {
      const big = typeof a === "bigint";
      if (big ? b === 0n : b === 0) {
        throw new CarciniPanic(
          rem ? "attempt to calculate the remainder with a divisor of zero" : "attempt to divide by zero",
          location,
        );
      }
      if (min < 0 && a == min && b == -1) {
        throw new CarciniPanic(
          rem ? "attempt to calculate the remainder with overflow" : "attempt to divide with overflow",
          location,
        );
      }
      if (big) return rem ? a % b : a / b;
      return rem ? a % b : Math.trunc(a / b);
    },
    $ckNeg(v, min, location) {
      // `v` is already negated: `-min` came out past `max`.
      if (v != 0 && -v == min) throw new CarciniPanic("attempt to negate with overflow", location);
      return v;
    },
    $ckAbs(v, min, location) {
      if (v == min && min < 0) throw new CarciniPanic("attempt to negate with overflow", location);
      return v < 0 ? -v : v;
    },
    $ckPow(x, n, min, max, location) {
      const big = typeof x === "bigint";
      const r = BigInt(x) ** BigInt(n);
      if (r < min || r > max) throw new CarciniPanic("attempt to exponentiate with overflow", location);
      return big ? r : Number(r);
    },
    $ckShift(n, bits, what, location) {
      if (n < 0 || n >= bits) throw new CarciniPanic(`attempt to ${what} with overflow`, location);
      return n;
    },
    // Integer arithmetic.
    $idiv(a, b) {
      if (b === 0) throw new CarciniPanic("attempt to divide by zero", "<arith>");
      return Math.trunc(a / b);
    },
    $irem(a, b) {
      if (b === 0) throw new CarciniPanic("attempt to calculate the remainder with a divisor of zero", "<arith>");
      return a % b;
    },
    $remEuclid(a, b) {
      const r = rt.$irem(a, b);
      return r < 0 ? r + Math.abs(b) : r;
    },
    $divEuclid(a, b) {
      const q = rt.$idiv(a, b);
      return a % b < 0 ? (b > 0 ? q - 1 : q + 1) : q;
    },
    // Float `rem_euclid` / `div_euclid`: no panic on zero (NaN, like Rust).
    $fremEuclid(a, b) {
      const r = a % b;
      return r < 0 ? r + Math.abs(b) : r;
    },
    $fdivEuclid(a, b) {
      const q = Math.trunc(a / b);
      return a % b < 0 ? (b > 0 ? q - 1 : q + 1) : q;
    },
    // `f64::signum`: 1.0 for +0.0, -1.0 for -0.0, NaN for NaN.
    $fsignum(x) {
      if (Number.isNaN(x)) return NaN;
      return x > 0 || Object.is(x, 0) ? 1 : -1;
    },
    $fract(x) {
      return x - Math.trunc(x);
    },
    $isSignNegative(x) {
      return x < 0 || Object.is(x, -0);
    },
    // `x.classify()`: the `FpCategory` variant's name (`f32`: its own subnormal range).
    $fpClass(x, f32) {
      if (Number.isNaN(x)) return "Nan";
      if (!Number.isFinite(x)) return "Infinite";
      if (x === 0) return "Zero";
      return Math.abs(x) < (f32 ? 1.1754943508222875e-38 : 2.2250738585072014e-308) ? "Subnormal" : "Normal";
    },
    $isNormal(x) {
      return Number.isFinite(x) && Math.abs(x) >= 2.2250738585072014e-308;
    },
    $copysign(a, b) {
      const neg = b < 0 || Object.is(b, -0) || (Number.isNaN(b) && rt.$transmute(b, "f64", "u64") >= 2 ** 63);
      return neg ? -Math.abs(a) : Math.abs(a);
    },
    // `ilog2` / `ilog10`: panics for zero and negatives, like Rust.
    $ilog(x, base) {
      if (x <= 0) throw new CarciniPanic("argument of integer logarithm must be positive", "<std>");
      let n = 0;
      for (let v = typeof x === "bigint" ? x : Math.trunc(x); v >= base; v = typeof v === "bigint" ? v / BigInt(base) : Math.floor(v / base)) n++;
      return n;
    },
    $isqrt(x) {
      if (x < 0) throw new CarciniPanic("argument of integer square root cannot be negative", "<std>");
      let r = Math.floor(Math.sqrt(x));
      while (r * r > x) r--;
      while ((r + 1) * (r + 1) <= x) r++;
      return r;
    },
    $ipow32(x, n) {
      let result = 1;
      for (let i = 0; i < n; i++) result = Math.imul(result, x);
      return result;
    },
    // `float as int`: truncate, saturate, NaN -> 0.
    $f2i(x, min, max) {
      if (Number.isNaN(x)) return 0;
      return Math.min(Math.max(Math.trunc(x), min), max);
    },
    // Rust rounds half away from zero; Math.round rounds half up.
    $round(x) {
      return x < 0 ? -Math.round(-x) : Math.round(x);
    },
    $clamp(x, lo, hi) {
      return Math.min(Math.max(x, lo), hi);
    },

    // Vec (JS arrays). Panic messages match rustc's.
    $ix(v, i, location) {
      if (i >= 0 && i < v.length) return i;
      throw new CarciniPanic(`index out of bounds: the len is ${v.length} but the index is ${i}`, location);
    },
    $get(v, i, location) {
      return arr(v)[rt.$ix(v, i, location)];
    },
    // The failing side of an inline bounds check.
    $oob(v, i, location) {
      throw new CarciniPanic(`index out of bounds: the len is ${v.length} but the index is ${i}`, location);
    },
    $vecRepeat(value, n, cloneEach) {
      const v = new Array(n);
      for (let i = 0; i < n; i++) v[i] = cloneEach ? clone(value) : value;
      return v;
    },
    $vecInsert(v, i, x, location) {
      if (i > v.length) {
        throw new CarciniPanic(`insertion index (is ${i}) should be <= len (is ${v.length})`, location);
      }
      v.splice(i, 0, x);
    },
    $vecRemove(v, i, location) {
      if (i >= v.length) {
        throw new CarciniPanic(`removal index (is ${i}) should be < len (is ${v.length})`, location);
      }
      return v.splice(i, 1)[0];
    },
    $vecSwap(v, a, b, location) {
      rt.$ix(v, a, location);
      rt.$ix(v, b, location);
      const view = v[VIEW];
      if (view !== undefined) {
        v = view.v;
        a += view.lo;
        b += view.lo;
      } else {
        v = arr(v);
      }
      const x = v[a];
      v[a] = v[b];
      v[b] = x;
    },
    $vecTruncate(v, n) {
      if (n < v.length) v.length = n;
    },
    $vecClear(v) {
      v.length = 0;
    },
    $contains(v, x) {
      return v.some((e) => eq(e, x));
    },
    // Not `push(...other)`: spreading a large array overflows the call stack.
    $extend(v, other) {
      for (const x of other) v.push(x);
    },
    // `c.extend(xs)` on a `C: Extend<T>`: by what `c` is (a string's new value is returned).
    $extendAny(c, xs) {
      if (typeof c === "string") {
        for (const x of xs) c += x;
      } else if (typeof c.$extend === "function") {
        c.$extend(xs);
      } else if (typeof c.extend === "function") {
        c.extend(xs);
      } else {
        for (const x of xs) c.push(x);
      }
      return c;
    },
    $pop(v) {
      return v.length ? some(v.pop()) : null;
    },
    $first(v) {
      return v.length ? some(v instanceof Deque ? v.$at(0) : v[0]) : null;
    },
    $last(v) {
      return v.length ? some(v instanceof Deque ? v.$at(v.n - 1) : v[v.length - 1]) : null;
    },
    $vget(v, i) {
      return i >= 0 && i < v.length ? some(v instanceof Deque ? v.$at(i) : v[i]) : null;
    },
    $some: some,
    $unsome: unsome,
    // `opt.unwrap_or(d)` of an `Option<Option<T>>`.
    $unwrapOr(x, d) {
      return x == null ? d : unsome(x);
    },
    $eq: eq,
    HashSet,
    Deque,
    HashMap,
    BTreeSet,
    BTreeMap,
    IndexSet,
    IndexMap,
    Reverse,
    $heapPush(a, x) {
      a.push(x);
      siftUp(a, 0, a.length - 1);
    },
    $heapPop(a) {
      if (a.length === 0) return null;
      let item = a.pop();
      if (a.length > 0) {
        [item, a[0]] = [a[0], item];
        siftDownToBottom(a, 0);
      }
      return item;
    },
    $heapFrom(a) {
      heapRebuild(a);
      return a;
    },
    $heapExtend(a, items) {
      const start = a.length;
      for (const x of items) a.push(x);
      heapRebuildTail(a, start);
    },
    $heapAppend(a, other) {
      // std moves the smaller heap into the larger one.
      if (a.length < other.length) {
        const mine = a.splice(0);
        a.push(...other.splice(0));
        other.push(...mine);
      }
      const start = a.length;
      a.push(...other.splice(0));
      heapRebuildTail(a, start);
    },
    $heapSorted(a) {
      return a.slice().sort(cmp);
    },
    // `(lo..hi).contains(&x)`; a missing bound is `null`. Compares with Rust's `Ord`.
    $inRange(x, lo, hi, inclusive) {
      if (lo != null && cmp(x, lo) < 0) return false;
      if (hi == null) return true;
      const c = cmp(x, hi);
      return inclusive ? c <= 0 : c < 0;
    },
    // Bitwise operators on 64-bit integers (`u64`, `i64`, `usize`, `isize`). JS's operators are
    // 32-bit, so values outside that range go through BigInt; results wrap to 64 bits and are
    // exact up to 2^53, like every 64-bit value here.
    $bit64(a, b, op, signed) {
      if (a >= 0 && b >= 0 && a <= 0x7fffffff && b <= 0x7fffffff) {
        return op === "&" ? a & b : op === "|" ? a | b : a ^ b;
      }
      const x = BigInt(a);
      const y = BigInt(b);
      const r = op === "&" ? x & y : op === "|" ? x | y : x ^ y;
      return Number(signed ? BigInt.asIntN(64, r) : BigInt.asUintN(64, r));
    },
    // `<<` masks the shift amount to 6 bits, as Rust does in release builds.
    $shl64(a, n, signed) {
      // (A product by a power of two is exact while it stays within ±2^53, where it can't wrap.)
      const p = a * 2 ** (n & 63);
      if (Number.isSafeInteger(p)) return p;
      const r = BigInt(a) << BigInt(n & 63);
      return Number(signed ? BigInt.asIntN(64, r) : BigInt.asUintN(64, r));
    },
    $shr64(a, n) {
      if (a >= 0 && a <= 0x7fffffff) return (n & 63) > 31 ? 0 : a >> (n & 63);
      // (A quotient by a power of two is exact; `floor` is the arithmetic shift of a negative.)
      return Math.floor(a / 2 ** (n & 63));
    },
    $not64(a, signed) {
      return signed ? -a - 1 : Number(BigInt.asUintN(64, ~BigInt(a)));
    },
    // A value of a type with `impl Iterator`, as a runtime iterator (for `take`, `map`, ..).
    $iterOf(o) {
      const it = new Iter(o[Symbol.iterator]());
      it.src = o;
      return it;
    },
    // ---- std gaps (`typeck_gaps` / `codegen_gaps`)
    // `x.fmt(f)` / `fmt::Display::fmt(&x, f)`: `x` with `f`'s spec (`base` renders it plainly).
    $fmtDelegate(f, x, base, kind, debug) {
      if (x !== null && typeof x === "object") {
        if (!debug && typeof x.fmt === "function") return x.fmt(f);
        if (debug && typeof x.fmt$dbg === "function") return x.fmt$dbg(f);
      }
      // (A generic value: its kind by what it is now; a whole float counts as an integer.)
      if (kind === "other") {
        if (typeof x === "bigint" || (typeof x === "number" && Number.isInteger(x))) kind = "int";
        else if (typeof x === "number") kind = "float";
        else if (typeof x === "string") kind = "str";
      }
      const o = f.o;
      let s;
      if (o != null && o.p != null && kind === "float") s = rt.$fixed(x, o.p);
      else if (o != null && o.p != null && kind === "str" && !debug) s = rt.$truncChars(x, o.p);
      else s = base(x);
      if (o != null) {
        const numeric = kind === "float" || kind === "int";
        if (debug && o.alt) s = rt.$pretty(s);
        if (o.plus && numeric) s = rt.$plus(s);
        if (o.w != null) s = rt.$pad(s, o.w, o.align, o.fill, o.zero, numeric);
      }
      return rt.$write(f, s, false);
    },
    $okOf(x) {
      return new Res(0, x);
    },
    // `s.get(lo..hi)`: `None` out of bounds or off a char boundary.
    $strGet(s, lo, hi, inclusive) {
      try {
        return rt.$strSlice(s, lo, hi, inclusive, "");
      } catch {
        return null;
      }
    },
    $resize(v, n, x) {
      if (n < v.length) v.length = n;
      while (v.length < n) v.push(clone(x));
    },
    $resizeWith(v, n, f) {
      if (n < v.length) v.length = n;
      while (v.length < n) v.push(f());
    },
    // `chunks_mut(n)` / `chunks_exact_mut(n)` / `rchunks_mut(n)`: views, so writes reach `v`.
    $chunksMut(v, n, exact, rev, location) {
      if (n === 0) throw new CarciniPanic("chunk size must be non-zero", location);
      v = arr(v);
      const out = [];
      if (rev) {
        for (let hi = v.length; hi > 0; hi -= n) out.push(rt.$sliceMut(v, Math.max(0, hi - n), hi, false, location));
      } else {
        for (let lo = 0; lo < v.length; lo += n) {
          if (exact && lo + n > v.length) break;
          out.push(rt.$sliceMut(v, lo, Math.min(v.length, lo + n), false, location));
        }
      }
      return new Iter(out[Symbol.iterator]());
    },
    $copyWithin(v, lo, hi, inclusive, dest, location) {
      v = arr(v);
      const part = rt.$slice(v, lo, hi, inclusive, location);
      if (dest + part.length > v.length) throw new CarciniPanic("dest is out of bounds", location);
      for (let i = 0; i < part.length; i++) v[dest + i] = part[i];
    },
    // `retain_mut(f)`: `f` gets a cell holding each item.
    $retainMut(v, f) {
      const a = arr(v);
      let j = 0;
      for (let i = 0; i < a.length; i++) {
        const cell = [a[i]];
        if (f(cell)) a[j++] = cell[0];
      }
      a.length = j;
      if (a !== v) v.$sync();
    },
    $splice(v, lo, hi, inclusive, src, location) {
      const out = rt.$slice(v, lo, hi, inclusive, location);
      v.splice(lo, out.length, ...Array.from(items(src)));
      return new Iter(out[Symbol.iterator]());
    },
    // `[1, 2].strip_prefix(&[1])` (`suffix`: `strip_suffix`).
    $vecStrip(v, p, suffix) {
      v = arr(v);
      if (p.length > v.length) return null;
      const at = suffix ? v.length - p.length : 0;
      for (let i = 0; i < p.length; i++) if (!eq(v[at + i], p[i])) return null;
      return suffix ? v.slice(0, at) : v.slice(p.length);
    },
    // `v.split_at_checked(mid)` (`mut`: views, so writes through either half reach `v`).
    $splitAtChecked(v, mid, mut, location) {
      if (mid > v.length) return null;
      if (mut) return [rt.$sliceMut(v, 0, mid, false, location), rt.$sliceMut(v, mid, null, false, location)];
      v = arr(v);
      return [v.slice(0, mid), v.slice(mid)];
    },
    // `v.split_first_mut()` / `split_last_mut()` of JS objects: the element and a view of the rest.
    $splitEndMut(v, last, location) {
      const n = v.length;
      if (n === 0) return null;
      return last
        ? [arr(v)[n - 1], rt.$sliceMut(v, 0, n - 1, false, location)]
        : [arr(v)[0], rt.$sliceMut(v, 1, null, false, location)];
    },
    // `v.partition_point(pred)`: the first index where `pred` is false (std's binary search).
    $partitionPoint(v, pred) {
      return rt.$binarySearchBy(v, (x) => (pred(x) ? -1 : 1))[0];
    },
    $sliceIsSortedBy(v, le) {
      v = arr(v);
      for (let i = 1; i < v.length; i++) if (!le(v[i - 1], v[i])) return false;
      return true;
    },
    $sliceIsSortedByKey(v, f) {
      v = arr(v);
      let prev;
      for (let i = 0; i < v.length; i++) {
        const k = f(v[i]);
        if (i > 0 && cmp(prev, k) > 0) return false;
        prev = k;
      }
      return true;
    },
    // `v.dedup_by(|a, b| ..)`: `a` is the later element, `b` the last one kept.
    $dedupBy(v, same) {
      const a = arr(v);
      if (a.length === 0) return;
      let j = 1;
      for (let i = 1; i < a.length; i++) if (!same(a[i], a[j - 1])) a[j++] = a[i];
      a.length = j;
      if (a !== v) v.$sync();
    },
    // `v.split(pred)` (`mode` 0), `split_inclusive` (1), `rsplit` (2): the runs between matches.
    $sliceSplit(v, pred, mode) {
      v = arr(v);
      const out = [];
      let lo = 0;
      for (let i = 0; i < v.length; i++) {
        if (!pred(v[i])) continue;
        out.push(v.slice(lo, mode === 1 ? i + 1 : i));
        lo = i + 1;
      }
      if (mode !== 1 || lo < v.length) out.push(v.slice(lo));
      if (mode === 2) out.reverse();
      return new Iter(out[Symbol.iterator]());
    },
    // `v.first_chunk::<n>()` / `last_chunk` (`mut`: a view, so writes reach `v`).
    $firstChunk(v, n, last, mut, location) {
      if (n > v.length) return null;
      const lo = last ? v.length - n : 0;
      return mut ? rt.$sliceMut(v, lo, lo + n, false, location) : arr(v).slice(lo, lo + n);
    },
    // `s.is_ascii()` on a string or a byte slice.
    $isAscii(v) {
      if (typeof v === "string") return /^[\0-\x7f]*$/.test(v);
      for (let i = 0; i < v.length; i++) if (v[i] > 127) return false;
      return true;
    },
    $bytesEqIgnoreCase(a, b) {
      if (a.length !== b.length) return false;
      const low = (x) => (x >= 65 && x <= 90 ? x + 32 : x);
      for (let i = 0; i < a.length; i++) if (low(a[i]) !== low(b[i])) return false;
      return true;
    },
    // `c.encode_utf8(&mut buf)`: the bytes go to the front of `buf`; the char as a `&str`.
    $encodeUtf8(c, buf) {
      const bytes = new TextEncoder().encode(c);
      if (bytes.length > buf.length) {
        throw new CarciniPanic(`encode_utf8: need ${bytes.length} bytes to encode U+${c.codePointAt(0).toString(16).toUpperCase()}, but the buffer has ${buf.length}`, "<std>");
      }
      for (let i = 0; i < bytes.length; i++) buf[i] = bytes[i];
      return c;
    },
    // `c.escape_debug()` / `escape_default()` / `escape_unicode()`.
    $escapeChar(c, how) {
      const cp = c.codePointAt(0);
      const uni = "\\u{" + cp.toString(16) + "}";
      if (how === "unicode") return uni;
      switch (c) {
        case "\t": return "\\t";
        case "\r": return "\\r";
        case "\n": return "\\n";
        case "\\": return "\\\\";
        case "'": return "\\'";
        case '"': return '\\"';
      }
      if (how === "default") return cp >= 0x20 && cp <= 0x7e ? c : uni;
      if (c === "\0") return "\\0";
      // (Rust's: grapheme extenders (combining marks), and what isn't printable: control,
      // format, private-use, unassigned and separator characters but the space.)
      return /^(?:\p{Grapheme_Extend}|[\p{C}\p{Z}])$/u.test(c) && c !== " " ? uni : c;
    },
    // `x.to_le_bytes()` / `to_be_bytes()`: the two's complement bytes of a `bits`-wide integer.
    // `x.rotate_left(n)` / `rotate_right(n)`: the bits of `x` at the type's width, rotated.
    $rotateBits(x, left, bits, signed, n) {
      const B = BigInt(bits);
      const v = BigInt.asUintN(bits, BigInt(x));
      const s = BigInt(n) % B;
      const k = left ? s : (B - s) % B;
      const r = BigInt.asUintN(bits, (v << k) | (v >> (B - k)));
      return Number(signed ? BigInt.asIntN(bits, r) : r);
    },
    // `x.swap_bytes()` / `to_be()` / `from_be()`: the bytes in the other order.
    $swapBytes(x, bits, signed) {
      let v = BigInt.asUintN(bits, BigInt(x));
      let r = 0n;
      for (let i = 0; i < bits / 8; i++) {
        r = (r << 8n) | (v & 0xffn);
        v >>= 8n;
      }
      return Number(signed ? BigInt.asIntN(bits, r) : r);
    },
    $reverseBits(x, bits, signed) {
      let v = BigInt.asUintN(bits, BigInt(x));
      let r = 0n;
      for (let i = 0; i < bits; i++) {
        r = (r << 1n) | (v & 1n);
        v >>= 1n;
      }
      return Number(signed ? BigInt.asIntN(bits, r) : r);
    },
    // `wrapping_shl` / `checked_shr` / `overflowing_shl`: a shift by `n` taken modulo the width;
    // `checked_*` is `None` and `overflowing_*` reports when `n` is the width or more.
    $intShift(x, name, bits, signed, n) {
      const over = n >= bits;
      const [kind, op] = name.split("_");
      if (kind === "checked" && over) return null;
      const s = BigInt(n % bits);
      const v = BigInt(x);
      let r = op === "shl" ? v << s : (signed ? v : BigInt.asUintN(bits, v)) >> s;
      r = signed ? BigInt.asIntN(bits, r) : BigInt.asUintN(bits, r);
      return kind === "overflowing" ? [Number(r), over] : Number(r);
    },
    $checkedEuclid(x, op, y) {
      if (y === 0) return null;
      return op === "div" ? rt.$divEuclid(x, y) : rt.$remEuclid(x, y);
    },
    // `x.overflowing_add(y)` and the like: `[wrapped, overflowed]`.
    $overflowing(x, op, bits, signed, y) {
      const a = BigInt(x);
      const b = op === "neg" || op === "abs" ? 0n : BigInt(y);
      const wrap = (v) => (signed ? BigInt.asIntN(bits, v) : BigInt.asUintN(bits, v));
      let r;
      switch (op) {
        case "add": r = a + b; break;
        case "sub": r = a - b; break;
        case "mul": r = a * b; break;
        case "div":
          if (b === 0n) throw new CarciniPanic("attempt to divide by zero", "<std>");
          r = a / b;
          break;
        case "rem":
          if (b === 0n) throw new CarciniPanic("attempt to calculate the remainder with a divisor of zero", "<std>");
          r = a % b;
          break;
        case "pow": {
          // (Exponent by exponent, so a huge `y` can't build a huge BigInt.)
          let acc = 1n;
          let base = a;
          let over = false;
          for (let e = b; e > 0n; e >>= 1n) {
            if (e & 1n) {
              acc *= base;
              if (wrap(acc) !== acc) over = true;
              acc = wrap(acc);
            }
            if (e > 1n) {
              base *= base;
              if (wrap(base) !== base) over = true;
              base = wrap(base);
            }
          }
          return [Number(acc), over];
        }
        case "neg": r = -a; break;
        default: r = a < 0n ? -a : a;
      }
      const w = wrap(r);
      return [Number(w), w !== r];
    },
    $divCeil(x, y) {
      if (y === 0) throw new CarciniPanic("attempt to divide by zero", "<std>");
      const r = x % y;
      const q = (x - r) / y;
      return r !== 0 && r > 0 === y > 0 ? q + 1 : q;
    },
    $nextMultipleOf(x, checked, y) {
      if (y === 0) {
        if (checked) return null;
        throw new CarciniPanic("attempt to calculate the remainder with a divisor of zero", "<std>");
      }
      const r = x % y;
      return r === 0 ? x : x + (y - r);
    },
    $u8EqIgnoreCase(a, b) {
      const lower = (c) => (c >= 65 && c <= 90 ? c + 32 : c);
      return lower(a) === lower(b);
    },
    $floatToBytes(x, f32, little) {
      const dv = new DataView(new ArrayBuffer(f32 ? 4 : 8));
      if (f32) dv.setFloat32(0, x, little);
      else dv.setFloat64(0, x, little);
      return Array.from(new Uint8Array(dv.buffer));
    },
    $floatFromBytes(bytes, f32, little) {
      const dv = new DataView(Uint8Array.from(bytes).buffer);
      return f32 ? dv.getFloat32(0, little) : dv.getFloat64(0, little);
    },
    $toBytes(x, bits, little) {
      let v = BigInt.asUintN(bits, BigInt(x));
      const out = [];
      for (let i = 0; i < bits / 8; i++) {
        out.push(Number(v & 0xffn));
        v >>= 8n;
      }
      return little ? out : out.reverse();
    },
    $fromBytes(bytes, bits, little, signed, big) {
      const bs = little ? bytes.slice().reverse() : bytes;
      let v = 0n;
      for (const b of bs) v = (v << 8n) | BigInt(b);
      if (signed) v = BigInt.asIntN(bits, v);
      return big ? v : Number(v);
    },
    $uabs(x) {
      return x < 0 ? -x : x;
    },
    // `x.next_power_of_two()` (`checked`: `None` past the type's range).
    $nextPow2(x, bits, checked) {
      const big = typeof x === "bigint";
      let p = big ? 1n : 1;
      while (p < x) p *= big ? 2n : 2;
      if (checked) return BigInt(p) >= 1n << BigInt(bits) ? null : p;
      return p;
    },
    // `fn main() -> Result<..>` returned `Err(e)`: fail with rustc's `Error: {e:?}`.
    $mainResult(r, fmt) {
      if (r != null && r.$tag === 1) throw new CarciniError(`Error: ${fmt(r[0])}`);
    },
    // User trait impls for built-in types (`impl Money for f64`), by JS kind, for calls through
    // a type parameter or `dyn Trait` (`$dcall`). Each compiled script has its own registry
    // (`$newRegs`), so one script's impls never answer another's calls.
    $dynNum(v, k) {
      return new DynNum(v, k);
    },
    $newRegs() {
      return { kinds: {}, blankets: [] };
    },
    // (`arity`: a tuple's impl, for arrays of that length only.)
    $extReg(regs, kind, cls, integral, arity) {
      (regs.kinds[kind] ??= []).push({ cls, integral, arity });
    },
    // Blanket impls (`impl<T: Display> Shout for T`): tried for any value, last.
    $blanketReg(regs, cls) {
      regs.blankets.push(cls);
    },
    // A type's class `d` passed for a type parameter (`T::make()`), with the associated
    // functions of the blanket impls it lacks (`impl<T: Default> Make for T`), each called with
    // the class as `this`.
    $bdict(regs, d) {
      let w = null;
      for (const cls of regs.blankets) {
        for (const k of Object.getOwnPropertyNames(cls)) {
          if (k === "length" || k === "name" || k === "prototype" || typeof cls[k] !== "function" || k in d) continue;
          w ??= Object.create(d);
          w[k] = (...a) => cls[k].call(w, ...a);
        }
      }
      return w ?? d;
    },
    // A file's impls of a `mod` file's traits also join that module's registry (`m.$extRegs`),
    // so the module's generic functions find them. Modules are shared between scripts: the
    // newest script's class goes first, and replaces an older run's class of the same name and
    // methods.
    $extRegIn(m, kind, cls, integral, arity) {
      const regs = m?.$extRegs;
      if (!regs) return;
      const list = (regs.kinds[kind] ??= []);
      const sig = (c) => `${c.name}:${Object.getOwnPropertyNames(c.prototype).sort().join(",")}`;
      const i = list.findIndex((r) => r.cls !== cls && sig(r.cls) === sig(cls) && r.from === "mod");
      if (i >= 0) list.splice(i, 1);
      if (!list.some((r) => r.cls === cls)) list.unshift({ cls, integral, arity, from: "mod" });
    },
    $blanketRegIn(m, cls) {
      const regs = m?.$extRegs;
      if (!regs) return;
      const sig = (c) => `${c.name}:${Object.getOwnPropertyNames(c.prototype).sort().join(",")}`;
      regs.blankets = regs.blankets.filter((c) => c === cls || sig(c) !== sig(cls) || !c.$fromMod);
      cls.$fromMod = true;
      if (!regs.blankets.includes(cls)) regs.blankets.unshift(cls);
    },
    // `w.put(..)` for a `w: &mut W` (its cell `c`): an impl's `&mut self` method on a string or
    // number (listed in its class's `$mut`) gets the cell as `this`, others the value.
    $dcallMut(regs, c, name, ...args) {
      const x = c[0];
      // (Through a cell over its value, written back.)
      if (x instanceof DynNum) {
        const cell = [x.v];
        const r = rt.$dcallMutT(regs, { kind: x.k, own: x.k === "$own" }, cell, name, ...args);
        x.v = cell[0];
        return r;
      }
      if (x !== null && typeof x === "object" && !Array.isArray(x) && typeof x[name] === "function") {
        return x.constructor.$mut?.has(name) ? x[name].call(c, ...args) : x[name](...args);
      }
      const kind = x instanceof Deque && regs.kinds.deque ? "deque" : Array.isArray(x) || x instanceof Deque ? "array" : x instanceof RefCell ? "refcell" : x instanceof Iter ? "iter" : typeof x;
      for (const r of regs.kinds[kind] ?? []) {
        if (typeof r.cls.prototype[name] !== "function") continue;
        if (r.arity !== undefined && !(Array.isArray(x) && x.length === r.arity)) continue;
        if (kind === "number" && r.integral !== Number.isInteger(x) && (regs.kinds[kind] ?? []).some((o) => o !== r && o.integral === Number.isInteger(x) && typeof o.cls.prototype[name] === "function")) continue;
        return r.cls.prototype[name].call(r.cls.$mut?.has(name) ? c : x, ...args);
      }
      for (const r of regs.kinds.option ?? []) {
        if (typeof r.cls.prototype[name] === "function") {
          return r.cls.prototype[name].call(r.cls.$mut?.has(name) ? c : x, ...args);
        }
      }
      // (A blanket impl's method that assigns `*self` takes the cell too.)
      for (const cls of regs.blankets) {
        if (typeof cls.prototype[name] === "function") {
          return cls.prototype[name].call(cls.$mut?.has(name) ? c : x, ...args);
        }
      }
      return rt.$dcall(regs, x, name, ...args);
    },
    $dcall(regs, x, name, ...args) {
      if (x instanceof DynNum) return rt.$dcallT(regs, { kind: x.k, own: x.k === "$own" }, x.v, name, ...args);
      // (A range value is an `Iter` over a `RangeIt`: your `Range` impls before its own methods.)
      if (x instanceof Iter && x.it instanceof RangeIt) {
        for (const r of regs.kinds[x.it.kind] ?? []) {
          if (typeof r.cls.prototype[name] === "function") return r.cls.prototype[name].call(x, ...args);
        }
      }
      if (x !== null && typeof x === "object" && !Array.isArray(x) && typeof x[name] === "function") {
        return x.constructor.$mut?.has(name) ? x[name].call(c, ...args) : x[name](...args);
      }
      // (A `VecDeque` is a `Deque`: its own impls', else a `Vec`'s.)
      const kind = x instanceof Deque && regs.kinds.deque
        ? "deque"
        : Array.isArray(x) || x instanceof Deque
        ? "array"
        : x instanceof BTreeMap
          ? "btreemap"
          : x instanceof BTreeSet
            ? "btreeset"
            : x instanceof HashMap
              ? "map"
              : x instanceof HashSet
                ? "set"
                : x instanceof Res
                  ? "result"
                  : x instanceof RefCell
                    ? "refcell"
                    : x instanceof Iter
                      ? "iter"
                      : typeof x;
      let pick = null;
      for (const r of regs.kinds[kind] ?? []) {
        if (typeof r.cls.prototype[name] !== "function") continue;
        // (A tuple's impl: arrays of its length only.)
        if (r.arity !== undefined && !(Array.isArray(x) && x.length === r.arity)) continue;
        pick ??= r;
        // A number goes to an integer type's impl when it's whole, else to a float type's.
        if (kind === "number" && r.integral === Number.isInteger(x)) {
          pick = r;
          break;
        }
      }
      if (pick) return pick.cls.prototype[pick.cls.$alts ? rt.$tupAlt(pick.cls, name, x, null) : name].call(x, ...args);
      // An `Option` is `null` or its value: its impl serves what no other impl takes (`None`
      // always; `Some(x)` unless `x`'s own type has the method).
      for (const r of regs.kinds.option ?? []) {
        if (typeof r.cls.prototype[name] === "function") return r.cls.prototype[name].call(x, ...args);
      }
      for (const cls of regs.blankets) {
        if (typeof cls.prototype[name] === "function") return cls.prototype[name].call(x, ...args);
      }
      return x.constructor.$mut?.has(name) ? x[name].call(c, ...args) : x[name](...args);
    },
    // `i128` / `u128` are BigInts, wrapped to their width like Rust's release builds.
    $w128(x, signed, bits = 128) {
      return signed ? BigInt.asIntN(bits, x) : BigInt.asUintN(bits, x);
    },
    // (`MIN / -1` overflows, which panics in release builds too.)
    $div128(a, b, signed, bits = 128) {
      if (b === 0n) throw new CarciniPanic("attempt to divide by zero", "<std>");
      if (signed && b === -1n && a === -(1n << BigInt(bits - 1))) {
        throw new CarciniPanic("attempt to divide with overflow", "<std>");
      }
      return a / b;
    },
    $rem128(a, b, signed, bits = 128) {
      if (b === 0n) throw new CarciniPanic("attempt to calculate the remainder with a divisor of zero", "<std>");
      if (signed && b === -1n && a === -(1n << BigInt(bits - 1))) {
        throw new CarciniPanic("attempt to calculate the remainder with overflow", "<std>");
      }
      return a % b;
    },
    $abs128(x) {
      return x < 0n ? -x : x;
    },
    // The integer methods of the BigInt kinds, exact at `bits`: `checked_*` / `saturating_*` /
    // `wrapping_*` (add, sub, mul, div, rem, pow, neg, abs), `abs`, `pow`, `signum`, `min`,
    // `max`, `clamp`, `rem_euclid`, `div_euclid`, `abs_diff`, the bit counts, `isqrt`, `ilog*`.
    // Debug builds pass a location after the arguments of `abs` / `pow`, which then panic on
    // overflow.
    // `n.is_multiple_of(m)` (unsigned): `m == 0` only for `n == 0`.
    $splitFirst(v) {
      v = arr(v);
      return v.length === 0 ? null : [v[0], v.slice(1)];
    },
    $splitLast(v) {
      v = arr(v);
      return v.length === 0 ? null : [v[v.length - 1], v.slice(0, -1)];
    },
    // `serde_json::from_str::<T>()` in a function generic over `T: DeserializeOwned`: the
    // schema of what the caller passed for `T` (a class's own, or a number kind's).
    $schemaOf(d) {
      if (d == null) return { t: "any" };
      if (d.$jsonSchema) return d.$jsonSchema;
      if (rt.$p(d) === rt.$prims.str) return { t: "str" };
      if (rt.$p(d) === rt.$prims.bool) return { t: "bool" };
      if (rt.$p(d) === rt.$prims.char) return { t: "char" };
      if (d.kind !== undefined) {
        if (d.float) return { t: "float", f32: d.kind === "f32" };
        const bits = BigInt(d.bits);
        const min = d.signed ? -(1n << (bits - 1n)) : 0n;
        const max = d.signed ? (1n << (bits - 1n)) - 1n : (1n << bits) - 1n;
        return { t: d.big ? "big" : "int", name: d.kind, min: String(min), max: String(max) };
      }
      return { t: "any" };
    },
    // An `Option<()>` from a call: `undefined` there is `Some(())`.
    $someUnit(x) {
      return x === undefined ? true : x;
    },
    $isMultipleOf(n, m) {
      return m == 0 ? n == 0 : n % m == 0;
    },
    $big(name, x, bits, signed, ...args) {
      const B = BigInt(bits);
      const min = signed ? -(1n << (B - 1n)) : 0n;
      const max = signed ? (1n << (B - 1n)) - 1n : (1n << B) - 1n;
      const wrap = (v) => (signed ? BigInt.asIntN(bits, v) : BigInt.asUintN(bits, v));
      const arg = (i) => (typeof args[i] === "bigint" ? args[i] : BigInt(args[i] ?? 0));
      const panic = (msg, loc = "<std>") => {
        throw new CarciniPanic(msg, loc);
      };
      // `a ** e` exactly, or just past the range when it overflows (`e` can be huge).
      const powExact = (a, e) =>
        (a > 1n || a < -1n) && e > B ? (a < 0n && e & 1n ? min - 1n : max + 1n) : a ** e;
      const powWrap = (a, e) => {
        let r = 1n;
        let base = wrap(a);
        for (let n = e; n > 0n; n >>= 1n) {
          if (n & 1n) r = wrap(r * base);
          base = wrap(base * base);
        }
        return r;
      };
      const euclid = (nm, a, y) => {
        const r = a % y;
        const fix = r < 0n;
        if (nm === "rem_euclid") return fix ? r + (y < 0n ? -y : y) : r;
        const q = a / y;
        return fix ? (y > 0n ? q - 1n : q + 1n) : q;
      };
      const nextMultiple = (a, y) => {
        const r = a % y;
        return r === 0n ? a : a + (y - r);
      };
      const m = /^(checked|saturating|wrapping|overflowing)_(.*)$/.exec(name);
      if (m) {
        const [, kind, op] = m;
        const b = op === "neg" || op === "abs" ? 0n : arg(0);
        if (op === "shl" || op === "shr") {
          const over = b >= B;
          if (kind === "checked" && over) return null;
          const s = b % B;
          const r = op === "shl" ? wrap(x << s) : wrap((signed ? x : BigInt.asUintN(bits, x)) >> s);
          return kind === "overflowing" ? [r, over] : r;
        }
        if (op === "div_euclid" || op === "rem_euclid" || op === "next_multiple_of") {
          if (b === 0n) {
            if (kind === "checked") return null;
            panic(op === "div_euclid" ? "attempt to divide by zero" : "attempt to calculate the remainder with a divisor of zero");
          }
          const r = op === "next_multiple_of" ? nextMultiple(x, b) : euclid(op, x, b);
          const ok = r >= min && r <= max;
          return kind === "checked" ? (ok ? r : null) : wrap(r);
        }
        if ((op === "div" || op === "rem") && b === 0n) {
          if (kind === "checked") return null;
          panic(op === "div" ? "attempt to divide by zero" : "attempt to calculate the remainder with a divisor of zero");
        }
        if (op === "pow" && kind === "wrapping") return powWrap(x, b);
        if (op === "pow" && kind === "overflowing") {
          const r = powExact(x, b);
          return [powWrap(x, b), !(r >= min && r <= max)];
        }
        if (op === "rem" && signed && x === min && b === -1n) return kind === "checked" ? null : 0n;
        const r =
          op === "add" || op === "add_signed" ? x + b
          : op === "sub" || op === "sub_signed" ? x - b
          : op === "mul" ? x * b
          : op === "div" ? x / b
          : op === "rem" ? x % b
          : op === "pow" ? powExact(x, b)
          : op === "neg" ? -x
          : x < 0n ? -x : x;
        const ok = r >= min && r <= max;
        if (kind === "overflowing") return [wrap(r), !ok];
        if (kind === "checked") return ok ? r : null;
        if (kind === "saturating") return ok ? r : r < min ? min : max;
        return wrap(r);
      }
      const u = BigInt.asUintN(bits, x);
      switch (name) {
        case "abs":
          if (signed && x === min) {
            if (args.length) panic("attempt to negate with overflow", args[0]);
            return min;
          }
          return x < 0n ? -x : x;
        case "pow": {
          const e = arg(0);
          const r = powExact(x, e);
          if (r >= min && r <= max) return r;
          if (args.length > 1) panic("attempt to exponentiate with overflow", args[1]);
          return powWrap(x, e);
        }
        case "signum":
          return x > 0n ? 1n : x < 0n ? -1n : 0n;
        case "min":
          return arg(0) < x ? arg(0) : x;
        case "max":
          return arg(0) > x ? arg(0) : x;
        case "clamp":
          if (arg(0) > arg(1)) panic("assertion failed: min <= max");
          return x < arg(0) ? arg(0) : x > arg(1) ? arg(1) : x;
        case "rem_euclid":
        case "div_euclid": {
          const y = arg(0);
          if (y === 0n) {
            panic(name === "div_euclid" ? "attempt to divide by zero" : "attempt to calculate the remainder with a divisor of zero");
          }
          const r = x % y;
          const fix = r < 0n;
          if (name === "rem_euclid") return fix ? r + (y < 0n ? -y : y) : r;
          const q = x / y;
          return fix ? (y > 0n ? q - 1n : q + 1n) : q;
        }
        case "abs_diff":
          return x > arg(0) ? x - arg(0) : arg(0) - x;
        case "count_ones":
          return [...u.toString(2)].filter((c) => c === "1").length;
        case "count_zeros":
          return bits - [...u.toString(2)].filter((c) => c === "1").length;
        case "leading_zeros":
          return u === 0n ? bits : bits - u.toString(2).length;
        case "trailing_zeros": {
          if (u === 0n) return bits;
          const s = u.toString(2);
          return s.length - 1 - s.lastIndexOf("1");
        }
        case "is_power_of_two":
          return x > 0n && (x & (x - 1n)) === 0n;
        case "is_positive":
          return x > 0n;
        case "is_negative":
          return x < 0n;
        case "isqrt": {
          if (x < 0n) panic("argument of integer square root cannot be negative");
          if (x < 2n) return x;
          let r = BigInt(Math.floor(Math.sqrt(Number(x))));
          while (r * r > x) r--;
          while ((r + 1n) * (r + 1n) <= x) r++;
          return r;
        }
        case "ilog2":
        case "ilog10":
          if (x <= 0n) panic("argument of integer logarithm must be positive");
          return x.toString(name === "ilog2" ? 2 : 10).length - 1;
        case "rotate_left":
        case "rotate_right": {
          const s = arg(0) % B;
          const k = name === "rotate_left" ? s : (B - s) % B;
          return wrap((u << k) | (u >> (B - k)));
        }
        case "reverse_bits": {
          let v = u;
          let r = 0n;
          for (let i = 0; i < bits; i++) {
            r = (r << 1n) | (v & 1n);
            v >>= 1n;
          }
          return wrap(r);
        }
        case "swap_bytes":
        case "to_be":
        case "from_be": {
          let v = u;
          let r = 0n;
          for (let i = 0; i < bits / 8; i++) {
            r = (r << 8n) | (v & 0xffn);
            v >>= 8n;
          }
          return wrap(r);
        }
        case "to_le":
        case "from_le":
          return x;
        case "div_ceil": {
          const y = arg(0);
          if (y === 0n) panic("attempt to divide by zero");
          const r = x % y;
          const q = (x - r) / y;
          return r !== 0n && r > 0n === y > 0n ? q + 1n : q;
        }
        case "next_multiple_of": {
          const y = arg(0);
          if (y === 0n) panic("attempt to calculate the remainder with a divisor of zero");
          return nextMultiple(x, y);
        }
        case "cast_signed":
          return BigInt.asIntN(bits, x);
        case "cast_unsigned":
          return BigInt.asUintN(bits, x);
        case "to_string":
          return x.toString();
        default:
          panic(`\`${name}\` isn't supported on 64- and 128-bit integers yet`);
      }
    },
    // `x as i64` / `x as u128`: from a number or another BigInt. A float saturates and NaN is
    // 0, as Rust's `as` does.
    $to128(x, signed, bits = 128) {
      if (typeof x === "bigint") return signed ? BigInt.asIntN(bits, x) : BigInt.asUintN(bits, x);
      // An integer within ±2^53 (every number kind's value): in range, unless negative into an
      // unsigned kind, which wraps.
      if (Number.isSafeInteger(x)) {
        const b = BigInt(x);
        return signed || x >= 0 ? b : BigInt.asUintN(bits, b);
      }
      if (typeof x === "boolean") return x ? 1n : 0n;
      // (`'A' as u64`: the code point.)
      if (typeof x === "string") return BigInt(x.codePointAt(0));
      const min = signed ? -(1n << BigInt(bits - 1)) : 0n;
      const max = signed ? (1n << BigInt(bits - 1)) - 1n : (1n << BigInt(bits)) - 1n;
      if (Number.isNaN(x)) return 0n;
      if (x === Infinity) return max;
      if (x === -Infinity) return min;
      const b = BigInt(Math.trunc(x));
      if (Number.isInteger(x) && !Number.isSafeInteger(x)) {
        // A float past 2^53: saturate rather than wrap.
        return b < min ? min : b > max ? max : b;
      }
      if (b < min || b > max) {
        // An integer from a smaller kind can't be out of range; a float saturates.
        return Number.isInteger(x) && Number.isSafeInteger(x) && !(x % 1)
          ? signed
            ? BigInt.asIntN(bits, b)
            : BigInt.asUintN(bits, b)
          : b < min
            ? min
            : max;
      }
      return b;
    },
    // `x as u32` from an `i128`: wrapped to the target's bits, as a number.
    $from128(x, bits, signed) {
      return Number(signed ? BigInt.asIntN(bits, x) : BigInt.asUintN(bits, x));
    },
    $parseInt128(s, signed, bits = 128) {
      const err = (kind) => new Res(1, new ParseError("ParseIntError", kind));
      if (s === "") return err("Empty");
      if (!/^[+-]?[0-9]+$/.test(s) || s === "+" || s === "-") return err("InvalidDigit");
      if (!signed && s[0] === "-") return err("InvalidDigit");
      const v = BigInt(s);
      const B = BigInt(bits);
      const max = signed ? (1n << (B - 1n)) - 1n : (1n << B) - 1n;
      const min = signed ? -(1n << (B - 1n)) : 0n;
      if (v > max) return err("PosOverflow");
      if (v < min) return err("NegOverflow");
      return new Res(0, v);
    },
    // `v[a..b].sort()` and friends: the changed copy of the range, written back.
    $writeBack(v, lo, c) {
      for (let i = 0; i < c.length; i++) v[lo + i] = c[i];
    },
    // `q[i]` of a deque whose expression isn't safe to evaluate twice.
    $dqGet(d, i, location) {
      return d instanceof Deque ? d.b[d.$i(i, location)] : rt.$get(d, i, location);
    },
    // `sort()` comparators for integers that are JS numbers, and for strings / chars.
    $cmpNum: (a, b) => a - b,
    $cmpStr: (a, b) => (a < b ? -1 : a > b ? 1 : 0),
    $dqRemove(a, i) {
      return i < a.length ? some(a.splice(i, 1)[0]) : null;
    },
    $dqShift(a) {
      return a.length ? some(a.shift()) : null;
    },
    // `map.entry(k).or_insert(v)`: the `[k, v]` slot for `k`, inserting `v` (or `v()` when `lazy`)
    // if `k` is missing.
    // `modify` is an `and_modify` closure, run on an existing slot. A lazy default gets the key, for
    // `or_insert_with_key`.
    $slot(map, k, v, lazy, modify) {
      const kk = key(k);
      let slot = map.m.get(kk);
      if (!slot) {
        slot = [k, lazy ? v(k) : v];
        map.m.set(kk, slot);
        if (map.$added !== undefined) map.$added(slot);
      } else if (modify) {
        modify(slot);
      }
      return slot;
    },
    // A place handed out by `index_mut`, or a `&mut` to a field / element passed to a `&mut i32`
    // parameter: `[0]` reads and writes it (`[1]` too, as a slot does).
    $cell(get, set) {
      return {
        get 0() {
          return get();
        },
        set 0(v) {
          set(v);
        },
        get 1() {
          return get();
        },
        set 1(v) {
          set(v);
        },
      };
    },
    // `m.lock()` (or `read` / `write` / `try_*`) of the lock prelude's `Mutex` / `RwLock`, not
    // unwrapped: an `Ok` whose `[0]` reads and writes the lock's value, so a guard bound by a
    // pattern (`if let Ok(mut g) = m.lock()`) writes through.
    $lockRes(l) {
      const r = new Res(0, undefined);
      Object.defineProperty(r, 0, {
        get: () => l.__c.v,
        set: (v) => {
          l.__c.v = v;
        },
        enumerable: true,
      });
      return r;
    },
    // A box (`[v]`: a local whose `&mut` is taken, a `&mut i32` parameter's cell) kept where a
    // slot is (`let r = &mut x;` of a `&mut i32`-typed `r`, `Some(&mut x)`, a `&mut i32` field):
    // `[1]` reads and writes it too.
    $cellSlot(c) {
      return {
        // (The box itself, for `ptr::eq`.)
        $base: c.$base ?? c,
        get 0() {
          return c[0];
        },
        set 0(v) {
          c[0] = v;
        },
        get 1() {
          return c[0];
        },
        set 1(v) {
          c[0] = v;
        },
      };
    },
    // A slot (`[1]` is the value: a map entry, a `&mut i32` a function returned) passed to a
    // `&mut i32` parameter, which reads `[0]`.
    $slotCell(s) {
      return {
        get 0() {
          return s[1];
        },
        set 0(v) {
          s[1] = v;
        },
        get 1() {
          return s[1];
        },
        set 1(v) {
          s[1] = v;
        },
      };
    },
    // A call of a generic `-> Option<&mut T>` (`null` or a slot) whose `T` is a struct or
    // collection: `null` or the object.
    $unslot(s) {
      return s == null ? null : s[1];
    },
    // What such a function returns where it has no place to point at (`m.get_mut(k)` of a
    // map): a slot holding the value, or `null` for `None` (`$optValSlot`).
    $valSlot(v) {
      return {
        get 0() {
          return v;
        },
        set 0(x) {
          v = x;
        },
        get 1() {
          return v;
        },
        set 1(x) {
          v = x;
        },
      };
    },
    $optValSlot(v) {
      return v == null ? null : rt.$valSlot(v);
    },
    // The map entry prelude's `get_mut` / `insert` / `or_insert(_with)`: the slot for its key.
    $entrySlot(e, mode, v) {
      const map = e.__m();
      const k = e.__k();
      if (mode === "get") return map.m.get(key(k));
      return rt.$slot(map, k, v, mode === "lazy");
    },
    // `map.get_mut(k)` of numbers, strings or bools: the `[k, v]` slot, or null.
    $slotOf(map, k) {
      return map.m.get(key(k)) ?? null;
    },
    // `v.get_mut(i)` / `first_mut` (0) / `last_mut` (-1) of numbers, strings or bools: a slot whose
    // `[1]` reads and writes the element, or null.
    // `let r = &mut v[i];` / `&mut s.count`: a slot over that element or field (`loc`: the
    // index is checked once, here, as Rust does when it makes the reference).
    $placeSlot(o, k, loc) {
      if (loc !== null && !(k >= 0 && k < o.length)) rt.$oob(o, k, loc);
      if (o instanceof Deque) {
        k = o.$p(k);
        o = o.b;
      }
      return {
        get 0() {
          return o[k];
        },
        set 0(v) {
          o[k] = v;
        },
        get 1() {
          return o[k];
        },
        set 1(v) {
          o[k] = v;
        },
      };
    },
    // `place.as_mut().unwrap()` of an `Option` of a number, string or bool in a field, an
    // element or a cell: null when it's `None`, else a slot whose `[1]` reads and writes it.
    $optSlot(o, k, loc) {
      if (loc !== null && !(k >= 0 && k < o.length)) rt.$oob(o, k, loc);
      if (o instanceof Deque) {
        k = o.$p(k);
        o = o.b;
      }
      if (o[k] == null) return null;
      return {
        get 0() {
          return o[k];
        },
        set 0(v) {
          o[k] = v;
        },
        get 1() {
          return o[k];
        },
        set 1(v) {
          o[k] = v;
        },
      };
    },
    // `rs.get_mut(i)` / `first_mut` (0) / `last_mut` (-1) of a `Vec<&mut i32>`: the slot stored
    // there, or null.
    $vecItem(a, i) {
      const j = i < 0 ? a.length + i : i;
      if (j < 0 || j >= a.length) return null;
      return a instanceof Deque ? a.b[a.$p(j)] : a[j];
    },
    $vecSlot(a, i) {
      let j = i < 0 ? a.length + i : i;
      if (j < 0 || j >= a.length) return null;
      if (a instanceof Deque) {
        j = a.$p(j);
        a = a.b;
      }
      return {
        get 0() {
          return a[j];
        },
        set 0(v) {
          a[j] = v;
        },
        get 1() {
          return a[j];
        },
        set 1(v) {
          a[j] = v;
        },
        // (What `$slotElem` looks for: a generic `IterMut`'s `{:?}` reads the element.)
        $slot: true,
        // (`max()` / `min()` of slots compare the elements.)
        $cmp(o) {
          return cmp(a[j], o[1]);
        },
        $eq(o) {
          return eq(a[j], o[1]);
        },
      };
    },
    // `{:?}` of a `Vec<&mut i32>` / `Vec<(usize, &mut i32)>` of slots or an item of one: a copy
    // with each slot read. `shape` is `1` for a slot, a tuple's fields (`0`: no slot) or `{e}`
    // for a Vec's items, `{o}` for an `Option` of one (`None` kept).
    $readSlots(x, shape) {
      if (shape === 1) return x[1];
      if (shape.o !== undefined) return x == null ? null : rt.$readSlots(x, shape.o);
      if (Array.isArray(shape)) return x.map((f, i) => (shape[i] ? rt.$readSlots(f, shape[i]) : f));
      return Array.from(x, (e) => rt.$readSlots(e, shape.e));
    },
    // An item of a `slice::IterMut` whose element type is generic: a slot's element, else itself.
    $slotElem(x) {
      return x !== null && typeof x === "object" && x.$slot === true ? x[1] : x;
    },
    // `v.iter_mut()` of numbers, strings or bools outside a loop by index: each item is a slot
    // whose `[1]` reads and writes the element.
    $iterMutSlots(a) {
      a = arr(a);
      return Iter.of(function* () {
        for (let i = 0; i < a.length; i++) yield rt.$vecSlot(a, i);
      });
    },
    // `map.values_mut()` / `map.iter_mut()` of numbers, strings or bools outside a `for` loop: an
    // iterator of slots over the values (`pairs`: `[k, slot]` items).
    $mapMutSlots(map, pairs) {
      const slots = map.$slots();
      return Iter.of(function* () {
        for (const s of slots) {
          const slot = {
            get 0() {
              return s[1];
            },
            set 0(v) {
              s[1] = v;
            },
            get 1() {
              return s[1];
            },
            set 1(v) {
              s[1] = v;
            },
            $cmp(o) {
              return cmp(s[1], o[1]);
            },
            $eq(o) {
              return eq(s[1], o[1]);
            },
          };
          yield pairs ? [s[0], slot] : slot;
        }
      });
    },
    $andModify(map, k, modify) {
      const slot = map.m.get(key(k));
      if (slot) modify(slot);
    },
    $mapIndex(map, k, location) {
      const slot = map.m.get(key(k));
      if (!slot) throw new CarciniPanic("no entry found for key", location);
      return slot[1];
    },

    // Option: `None` is null (or undefined), `Some(x)` is x.
    $unwrap(x, location) {
      if (x == null) throw new CarciniPanic("called `Option::unwrap()` on a `None` value", location);
      return unsome(x);
    },
    $expect(x, message, location) {
      if (x == null) throw new CarciniPanic(message, location);
      return unsome(x);
    },
    $cmp: cmp,
    $clone: clone,
    // `partial_cmp`: `None` when the values are incomparable (a NaN decides it).
    // `a < b` through a `PartialOrd<B>` impl: its `partial_cmp`'s `Option<Ordering>` against the
    // operator (`None` compares false, as in Rust).
    $pord(o, op) {
      if (o == null) return false;
      return op === "<" ? o < 0 : op === "<=" ? o <= 0 : op === ">" ? o > 0 : o >= 0;
    },
    $partialCmp(a, b) {
      const c = a instanceof Iter ? a.$cmpWith(b, false) : cmp(a, b);
      return Number.isNaN(c) ? null : c;
    },
    // A comparison's result as `partial_cmp`'s `Option<Ordering>` (NaN: incomparable).
    $ordOpt(c) {
      return Number.isNaN(c) ? null : c;
    },
    // `a == b` / `a < b` through a `T: PartialEq<R>` / `PartialOrd<R>` bound: with `b` not of
    // `a`'s class, `a`'s `impl PartialEq<R>`'s `eq` (the first such impl) / `partial_cmp`.
    $eqTo(a, b) {
      return otherClass(a, b, "eq") ? a.eq(b) : eq(a, b);
    },
    $pcmpTo(a, b) {
      if (otherClass(a, b, "partial_cmp")) return a.partial_cmp(b);
      const c = cmp(a, b);
      return Number.isNaN(c) ? null : c;
    },
    $ordReverse(o) {
      return o === 0 ? 0 : -o;
    },
    $ordThen(o, other) {
      return o !== 0 ? o : other;
    },
    $ordThenWith(o, f) {
      return o !== 0 ? o : f();
    },
    $dbgOrdering(o) {
      return o < 0 ? "Less" : o > 0 ? "Greater" : "Equal";
    },

    // char methods.
    // ASCII-only case mapping, for chars and strings.
    $asciiUpper(s) {
      return s.replace(/[a-z]+/g, (m) => m.toUpperCase());
    },
    $asciiLower(s) {
      return s.replace(/[A-Z]+/g, (m) => m.toLowerCase());
    },
    $toDigit(c, radix) {
      if (radix < 2 || radix > 36) throw new CarciniPanic("to_digit: invalid radix -- radix must be in the range 2 to 36 inclusive", "<std>");
      const d = /^[0-9a-zA-Z]$/.test(c) ? parseInt(c, 36) : 99;
      return d < radix ? d : null;
    },

    // Option methods taking closures.
    $optMap(x, f) {
      return x == null ? null : f(unsome(x));
    },
    $optMapSome(x, f) {
      return x == null ? null : some(f(unsome(x)));
    },
    $optFilter(x, p) {
      return x != null && p(unsome(x)) ? x : null;
    },
    $optOrElse(x, f) {
      return x == null ? f() : x;
    },
    $optMapOr(x, d, f) {
      return x == null ? d : f(unsome(x));
    },
    $optMapOrElse(x, d, f) {
      return x == null ? d() : f(unsome(x));
    },
    $optIsSomeAnd(x, p) {
      return x != null && p(unsome(x));
    },
    $optIsNoneOr(x, p) {
      return x == null || p(unsome(x));
    },
    $optOr(x, y) {
      return x == null ? y : x;
    },
    $optZip(x, y) {
      return x == null || y == null ? null : [unsome(x), unsome(y)];
    },
    $optInspect(x, f) {
      if (x != null) f(unsome(x));
      return x;
    },
    $optTranspose(x) {
      if (x == null) return new Res(0, null);
      return x.$tag === 1 ? x : new Res(0, x[0]);
    },
    $resInspect(r, f) {
      if (r.$tag === 0) f(r[0]);
      return r;
    },
    $resInspectErr(r, f) {
      if (r.$tag === 1) f(r[0]);
      return r;
    },
    $resTranspose(r) {
      if (r.$tag === 1) return r;
      return r[0] == null ? null : new Res(0, r[0]);
    },
    $optIter(x) {
      return new Iter((x == null ? [] : [unsome(x)])[Symbol.iterator]());
    },

    // Iterator sources.
    Iter,
    $iter(x) {
      return new Iter(Array.isArray(x) && x[VIEW] === undefined ? new ArrayIt(x, 0, x.length) : x[Symbol.iterator]());
    },
    Hasher,
    // `x.hash(state)`: a type's own `hash`, else the value's key.
    $hashInto(v, h) {
      if (v !== null && typeof v === "object" && typeof v.hash === "function") v.hash(h);
      else if (typeof h.$write === "function") h.$write(v);
      else ownMethod(h, "write")(rt.$hashBytes(v));
    },
    // `b.hash_one(x)`: the hasher `b` builds (std's `Hasher` for a `RandomState`, or your own),
    // fed `x`, finished.
    $hashOne(b, x) {
      const h = ownMethod(b, "build_hasher")();
      rt.$hashInto(x, h);
      return h instanceof Hasher ? h.$finish() : ownMethod(h, "finish")();
    },
    // The bytes std's `Hash` gives your own `Hasher` for a value: integers as 8 bytes
    // (`usize` / `u64`'s), a string's UTF-8 then `0xff`, a Vec's length then its items.
    $hashBytes(v) {
      if (typeof v === "string") return rt.$utf8(v).concat([0xff]);
      if (typeof v === "boolean") return [v ? 1 : 0];
      if (typeof v === "number" || typeof v === "bigint") return rt.$toBytes(Math.trunc(Number(v)) === Number(v) ? v : 0, 64, true);
      if (Array.isArray(v)) return rt.$toBytes(v.length, 64, true).concat(...v.map((x) => rt.$hashBytes(x)));
      if (v == null) return [0];
      return rt.$utf8(key(v)).concat([0xff]);
    },
    // A provided `Hasher::write_u32(x)` of your own hasher.
    $hasherWrite(h, x, bits) {
      return ownMethod(h, "write")(rt.$toBytes(x, bits, true));
    },
    // `x.into()` with `x: T` where `T: Into<i64>` (or `u64`, `i128`, `u128`): any narrower
    // integer (a number) or a `bool` becomes a BigInt.
    $intoBig(x) {
      return typeof x === "bigint" ? x : BigInt(x);
    },
    // ... and into a number kind (`T: Into<f64>`, `T: Into<i32>`): a `bool` becomes 0 or 1.
    $intoNum(x) {
      return typeof x === "boolean" ? Number(x) : x;
    },
    // `u8::try_from(x)` / `x.try_into()` between integer types: `Err` when out of range.
    $tryInt(x, bits, signed, big) {
      const v = BigInt(x);
      const b = BigInt(bits);
      const min = signed ? -(1n << (b - 1n)) : 0n;
      const max = (signed ? 1n << (b - 1n) : 1n << b) - 1n;
      if (v < min || v > max) return new Res(1, new ParseError("TryFromIntError", v < min ? "NegOverflow" : "PosOverflow"));
      return new Res(0, big ? v : Number(v));
    },
    // `u8::try_from(c)`: `Err` past U+00FF.
    $tryU8Char(c) {
      const v = c.codePointAt(0);
      return v > 255 ? new Res(1, new ParseError("TryFromCharError", "")) : new Res(0, v);
    },
    // `char::try_from(n)`: `Err` past U+10FFFF and for surrogates.
    $tryChar(n) {
      const v = Number(n);
      if (v > 0x10ffff || (v >= 0xd800 && v <= 0xdfff)) return new Res(1, new ParseError("CharTryFromError", ""));
      return new Res(0, String.fromCodePoint(v));
    },
    // An `IntoIterator` argument (a Vec, set, map or iterator) as an iterator.
    $intoIter(x) {
      return new Iter((x instanceof HashMap ? x.$entries() : x)[Symbol.iterator]());
    },
    // A token value's text for `quote!` (`None` is nothing).
    $tt(x) {
      return x == null ? "" : x.__tt();
    },
    // Tokens as text: a token value, or an iterable of them (a `Punct` that's `Joint` gets no
    // space after it).
    $tokensJoin(x) {
      if (x == null) return "";
      if (typeof x.__tt === "function") return x.__tt();
      let s = "";
      for (const t of x) {
        const text = rt.$tokensJoin(t);
        if (!text) continue;
        s += text;
        if (!(t != null && typeof t.__joint === "function" && t.__joint())) s += " ";
      }
      return s.trim();
    },
    // `quote!`'s `#( .. ) sep *`: `f` over the variables' items together (as far as the shortest
    // goes), joined by the separator.
    $quoteRep(lists, f, sep) {
      const arrays = lists.map((x) => (Array.isArray(x) ? x : Array.from(x instanceof HashMap ? x.$entries() : x)));
      const n = arrays.length ? Math.min(...arrays.map((a) => a.length)) : 0;
      const out = [];
      for (let i = 0; i < n; i++) out.push(f(...arrays.map((a) => a[i])));
      return out.join(sep ? `${sep} ` : " ");
    },
    // `std::iter::repeat(x)` (each item a clone), `once`, `empty`, `repeat_with`, `from_fn`,
    // `successors`.
    $iterRepeat(x) {
      return Iter.of(function* () {
        for (;;) yield clone(x);
      });
    },
    $iterOnce(x) {
      return Iter.of(function* () {
        yield x;
      });
    },
    $iterEmpty() {
      return Iter.of(function* () {});
    },
    $repeatWith(f) {
      return Iter.of(function* () {
        for (;;) yield f();
      });
    },
    $fromFn(f) {
      return Iter.of(function* () {
        for (;;) {
          const y = f();
          if (y == null) return;
          yield y;
        }
      });
    },
    $successors(first, f) {
      return Iter.of(function* () {
        for (let x = first; x != null; x = f(x)) yield x;
      });
    },
    // A range value's bounds (`r.start`, `r.end`, `r.start()`), where it has got to.
    $rangeOf(r) {
      const it = r.it;
      return { start: it.i, end: it.hi };
    },
    $rangeHas(r, x) {
      const it = r.it;
      return x >= it.i && (it.hi === null || (it.incl ? x <= it.hi : x < it.hi));
    },
    // `len()` without using the range up.
    $rangeLen(r) {
      const it = r.it;
      const n = it.end - it.i;
      return n > 0 ? n : typeof n === "bigint" ? 0n : 0;
    },
    $rangeEmpty(r) {
      const it = r.it;
      return it.hi !== null && (it.incl ? !(it.i <= it.hi) : !(it.i < it.hi));
    },
    // `v[r]` / `&s[r]` with a range value: `slice` / `strSlice` with its bounds.
    $sliceBy(name, v, r, loc) {
      const it = r.it;
      return rt[name](v, it.i, it.hi, it.incl, loc);
    },
    // `lo..hi`, `lo..=hi` and `lo..` (hi null); chars when `isChar`, skipping surrogates.
    $range(lo, hi, inclusive, isChar) {
      if (isChar) {
        const a = lo.codePointAt(0);
        const b = hi.codePointAt(0) + (inclusive ? 1 : 0);
        return Iter.of(function* () {
          for (let c = a; c < b; c++) if (c < 0xd800 || c > 0xdfff) yield String.fromCodePoint(c);
        });
      }
      // `i128` / `u128` ranges step in BigInts.
      const one = typeof (lo ?? hi) === "bigint" ? 1n : 1;
      const end = hi === null ? Infinity : inclusive ? hi + one : hi;
      return new Iter(new RangeIt(lo ?? one - one, end, one, hi, inclusive, lo === null));
    },
    $split(s, p) {
      return new Iter(rt.$strPieces(s, p)[Symbol.iterator]());
    },
    $splitWhitespace(s) {
      return new Iter(s.split(/\s+/).filter((w) => w !== "")[Symbol.iterator]());
    },
    $lines(s) {
      return new Iter(lines(s)[Symbol.iterator]());
    },
    $windows(v, n) {
      if (n === 0) throw new CarciniPanic("window size must be non-zero", "<std>");
      return new Iter(new SlicesIt(arr(v), n, 1, true));
    },
    $chunksExact(v, n) {
      if (n === 0) throw new CarciniPanic("chunk size must be non-zero", "<std>");
      return new Iter(new SlicesIt(arr(v), n, n, true));
    },
    // `s.split_at(mid)`, with slicing's panics.
    $splitAt(s, mid, location) {
      return [rt.$strSlice(s, 0, mid, false, location), rt.$strSlice(s, mid, null, false, location)];
    },
    $chunks(v, n) {
      if (n === 0) throw new CarciniPanic("chunk size must be non-zero", "<std>");
      return new Iter(new SlicesIt(arr(v), n, n, false));
    },

    // Vec methods taking closures.
    $forEachIndex(v, f) {
      for (let i = 0; i < v.length; i++) f(i);
    },
    $sortByKey(v, f) {
      v = arr(v);
      const keyed = v.map((x) => [f(x), x]);
      keyed.sort((a, b) => cmp(a[0], b[0]));
      for (let i = 0; i < v.length; i++) v[i] = keyed[i][1];
    },
    $retain(v, p) {
      const a = arr(v);
      let j = 0;
      for (let i = 0; i < a.length; i++) if (p(a[i])) a[j++] = a[i];
      a.length = j;
      if (a !== v) v.$sync();
    },
    $dedup(v) {
      rt.$dedupByKey(v, (x) => x);
    },
    $dedupByKey(v, f) {
      const a = arr(v);
      if (a.length === 0) return;
      let j = 1;
      let last = f(a[0]);
      for (let i = 1; i < a.length; i++) {
        const k = f(a[i]);
        if (!eq(k, last)) {
          a[j++] = a[i];
          last = k;
        }
      }
      a.length = j;
      if (a !== v) v.$sync();
    },

    // Strings.
    $utf8len(s) {
      return s.length < strInfoMin ? utf8Len(s) : strInfo(s).len;
    },
    // `s.as_bytes()[i]`, without building the bytes when `s` is ASCII.
    $byteAt(s, i, location) {
      const info = strInfo(s);
      if (info.ascii) {
        if (i >= 0 && i < s.length) return s.charCodeAt(i);
        return rt.$oob(s, i, location);
      }
      const b = strBytes(info);
      return i >= 0 && i < b.length ? b[i] : rt.$oob(b, i, location);
    },
    $truncChars(s, n) {
      return Array.from(s).slice(0, n).join("");
    },

    // Formatting.
    $fmtF: fmtF,
    $dbgF: dbgF,
    // `f32`: the shortest digits that read back as the same f32 (`0.1`, not the double's
    // `0.10000000149011612`), then formatted like an f64.
    $fmtF32: (x) => fmtF(shortF32(x)),
    $dbgF32: (x) => dbgF(shortF32(x)),
    // A 64-bit integer: past 2^53 the double's exact digits (`1 << 60` is
    // 1152921504606846976, where JS prints 1152921504606847000).
    $fmtI64(x) {
      return typeof x === "number" && !Number.isSafeInteger(x) && Number.isInteger(x) ? BigInt(x).toString() : String(x);
    },
    // `{:.p}`: like `toFixed`, except that an exact tie rounds to even, as Rust does
    // (`format!("{:.0}", 2.5)` is "2"; JS's `toFixed` rounds ties up). From 1e21 on (where
    // `toFixed` gives exponent notation) the double's exact digits; `-0.0` keeps its sign.
    $fixed(x, p) {
      if (!Number.isFinite(x)) return fmtF(x);
      if (Object.is(x, -0)) return "-" + rt.$fixed(0, p);
      if (Math.abs(x) >= 1e21) return BigInt(x).toString() + (p > 0 ? "." + "0".repeat(p) : "");
      const up = x.toFixed(p);
      if (Math.abs(x) < 1e21) {
        // Every double this size has an exact decimal expansion within 100 digits.
        const exact = Math.abs(x).toFixed(100);
        const [int, frac] = exact.split(".");
        if (frac[p] === "5" && /^0*$/.test(frac.slice(p + 1))) {
          const kept = p > 0 ? `${int}.${frac.slice(0, p)}` : int;
          if (Number(kept[kept.length - 1]) % 2 === 0) {
            return expandExponent((x < 0 ? "-" : "") + kept);
          }
        }
      }
      return expandExponent(up);
    },
    // `{:p}`: JS has no addresses, so a made-up one (`0x10000` up, 16 apart), the same each time
    // for one object.
    $fmtPtr(x) {
      const obj = (x !== null && typeof x === "object") || typeof x === "function";
      let n = obj ? PTRS.get(x) : undefined;
      if (n === undefined) {
        n = nextPtr++;
        if (obj) PTRS.set(x, n);
      }
      return "0x" + (0x10000 + n * 16).toString(16);
    },
    // `{:x}` / `{:o}` / `{:b}` (`#` adds the prefix): negative values print the two's
    // complement of their `bits`-wide type, as Rust does.
    $radix(x, radix, upper, alt, bits) {
      let n = x;
      if (n < 0) n = bits > 32 ? BigInt.asUintN(bits, BigInt(n)) : n + 2 ** bits;
      let digits = n.toString(radix);
      if (upper) digits = digits.toUpperCase();
      const prefix = alt ? (radix === 16 ? "0x" : radix === 8 ? "0o" : "0b") : "";
      return prefix + digits;
    },
    // `{:e}` / `{:.2e}` / `{:E}`: `1.2345e3`, Rust's form (no `+` in the exponent).
    $exp(x, p, upper) {
      if (!Number.isFinite(x)) return fmtF(x);
      const s = (Object.is(x, -0) ? "-" : "") + (p == null ? x.toExponential() : x.toExponential(p)).replace("e+", "e");
      return upper ? s.toUpperCase() : s;
    },
    // `{:#?}`: re-lays the one-line `{:?}` text one item per line, indented by four spaces,
    // with a trailing comma, like Rust's pretty `Debug`. String and char literals are kept.
    $pretty(s) {
      s = String(s);
      let out = "";
      let depth = 0;
      const nl = () => "\n" + "    ".repeat(depth);
      for (let i = 0; i < s.length; i++) {
        const ch = s[i];
        if (ch === '"' || (ch === "'" && s[i + 2] === "'") || (ch === "'" && s[i + 1] === "\\")) {
          // A string or char literal, with its escapes.
          const q = ch;
          let j = i + 1;
          while (j < s.length && s[j] !== q) j += s[j] === "\\" ? 2 : 1;
          out += s.slice(i, j + 1);
          i = j;
          continue;
        }
        if (ch === "{" || ch === "[" || ch === "(") {
          const close = ch === "{" ? "}" : ch === "[" ? "]" : ")";
          if (s[i + 1] === close) {
            out += ch + close;
            i += 1;
            continue;
          }
          depth++;
          out += ch + nl();
          if (ch === "{" && s[i + 1] === " ") i += 1;
          continue;
        }
        if (ch === "}" || ch === "]" || ch === ")") {
          depth--;
          out = out.replace(/ $/, "");
          // `(1,)` already ends its item with a comma.
          if (!out.endsWith(",")) out += ",";
          out += nl() + ch;
          continue;
        }
        if (ch === "," && s[i + 1] === " " && depth > 0) {
          out += "," + nl();
          i += 1;
          continue;
        }
        if (ch === " " && s[i + 1] === "}") continue;
        out += ch;
      }
      return out;
    },
    $dbgStr(s) {
      return `"${Array.from(s, (c) => escapeChar(c, '"')).join("")}"`;
    },
    $dbgTuple(t, fmts) {
      const parts = t.map((x, i) => fmts[i](x));
      return parts.length === 1 ? `(${parts[0]},)` : `(${parts.join(", ")})`;
    },
    $dbgMap(m, kf, vf) {
      return `{${Array.from(m.$slots(), ([k, v]) => `${kf(k)}: ${vf(v)}`).join(", ")}}`;
    },
    $dbgSet(s, fmt) {
      return `{${Array.from(s, (x) => fmt(x)).join(", ")}}`;
    },
    $dbgOpt(x, fmt) {
      return x == null ? "None" : `Some(${fmt(unsome(x))})`;
    },
    $dbgVec(v, fmt) {
      return `[${v.map((x) => fmt(x)).join(", ")}]`;
    },
    $dbgChar(c) {
      return `'${escapeChar(c, "'")}'`;
    },
    // Values whose type the compiler doesn't know yet (NS results until phase 2 bindings).
    $disp(x) {
      if (typeof x === "number") return fmtF(x);
      if (x !== null && typeof x === "object" && typeof x.fmt === "function") return rt.$display(x);
      if (x === undefined) return "()";
      return String(x);
    },
    $dbg(x) {
      if (typeof x === "number") return dbgF(x);
      if (typeof x === "string") return rt.$dbgStr(x);
      if (x === undefined) return "()";
      if (x !== null && typeof x.$dbg === "function") return x.$dbg();
      // `js::Date`: `Date(2024-03-01T12:00:00.000Z)`
      if (x instanceof Date) return `Date(${Number.isNaN(x.getTime()) ? "Invalid Date" : x.toISOString()})`;
      return String(x);
    },
    // `{:?}` of a generic value, whose type is erased: whole numbers print as integers (so a
    // whole-number float prints without `.0`).
    $dbgAny(x) {
      if (typeof x === "number" && Number.isInteger(x) && !Object.is(x, -0)) return String(x);
      if (typeof x === "string") return rt.$dbgStr(x);
      if (Array.isArray(x)) return `[${x.map(rt.$dbgAny).join(", ")}]`;
      return rt.$dbg(x);
    },
    // A struct or variant's `{:?}`: `Point { x: 1, y: 2 }`, or `Meters(5.0)` when `names` is null.
    $dbgStruct(name, names, vals) {
      if (names === null) return `${name}(${vals.join(", ")})`;
      if (vals.length === 0) return name;
      return `${name} { ${vals.map((v, i) => `${names[i]}: ${v}`).join(", ")} }`;
    },
    // `*r = v` for a `&mut` struct or enum (or a `Vec` or other collection): the object keeps
    // its identity, its contents change.
    $replace(target, value) {
      if (target === value) return;
      if (Array.isArray(target)) {
        // (A view of part of a `Vec`, `*arr = [..]` through a `&mut [T; N]`: same length.)
        if (target[VIEW]) {
          for (let i = 0; i < target.length; i++) target[i] = value[i];
          return;
        }
        const items = Array.from(value);
        target.length = 0;
        for (const x of items) target.push(x);
        return;
      }
      for (const k of Object.keys(target)) delete target[k];
      Object.assign(target, value);
    },
    // `p.as_ref()` on a raw pointer to a cell: the value, or `None` for a null pointer.
    $ptrGet(cell) {
      return cell == null ? null : cell[0];
    },
    // `c[i]` with `c` of a type parameter bounded `Index<Ix>`: a Vec's (or deque's) element, or
    // your type's `index`.
    $indexAny(c, i, loc) {
      if (Array.isArray(c) || c instanceof Deque) {
        if (!(i >= 0 && i < c.length)) rt.$oob(c, i, loc);
        return c instanceof Deque ? c.b[c.$p(i)] : c[i];
      }
      const f = opFn(c, "index");
      if (typeof f === "function") return f.call(c, i);
      throw new TypeError(`${loc}: this value can't be indexed`);
    },
    // `c[i] = v` / `c[i] += 1` with `c` of a type parameter bounded `IndexMut<Ix>`: a cell over
    // the Vec's (or deque's) element, or what your type's `index_mut` hands out.
    $indexAnyMut(c, i, loc) {
      if (Array.isArray(c) || c instanceof Deque) {
        if (!(i >= 0 && i < c.length)) rt.$oob(c, i, loc);
        if (c instanceof Deque) {
          const k = c.$p(i);
          return rt.$cell(() => c.b[k], ($v) => {
            c.b[k] = $v;
          });
        }
        return rt.$cell(() => c[i], ($v) => {
          c[i] = $v;
        });
      }
      const f = opFn(c, "index_mut");
      if (typeof f === "function") return f.call(c, i);
      throw new TypeError(`${loc}: this value can't be indexed`);
    },
    // `slice[i]` with `i: I` of `I: SliceIndex<[T]>` (smallvec's `impl<I: SliceIndex<[T]>>
    // Index<I>`, also `Index::index(s, r)` of a string): an element for a number, a copy of that
    // part for a range.
    $sliceIndex(v, i, loc) {
      if (i instanceof Iter && i.it instanceof RangeIt) {
        return rt.$sliceBy(typeof v === "string" ? "$strSlice" : "$slice", v, i, loc);
      }
      return rt.$get(v, i, loc);
    },
    // `&mut slice[i]` of the same: a cell over the element, a view of that part for a range (a
    // string's part is a copy: JS strings are values).
    $sliceIndexMut(v, i, loc) {
      if (i instanceof Iter && i.it instanceof RangeIt) {
        return rt.$sliceBy(typeof v === "string" ? "$strSlice" : "$sliceMut", v, i, loc);
      }
      const a = arr(v);
      const k = rt.$ix(v, i, loc);
      return rt.$cell(() => a[k], ($v) => {
        a[k] = $v;
      });
    },
    // `ptr::eq(a, b)`: the same object, or slots over the same box.
    $ptrEq(a, b) {
      return a === b || (a != null && b != null && (a.$base ?? a) === (b.$base ?? b));
    },
    // `p.as_mut()` of a pointer to a number, string or bool: null, or a slot (`[1]` too) over
    // the place its cell or slot reads and writes.
    $ptrSlot(cell) {
      return cell == null ? null : rt.$cellSlot(cell);
    },
    // `mem::transmute::<From, To>(x)` between same-size numbers (and `char`, `bool`): the bytes
    // reinterpreted. 64-bit integers are JS numbers, 128-bit ones BigInts.
    $transmute(x, from, to) {
      const dv = new DataView(new ArrayBuffer(16));
      switch (from) {
        case "i8": case "u8": dv.setUint8(0, Number(x) & 0xff); break;
        case "bool": dv.setUint8(0, x ? 1 : 0); break;
        case "i16": case "u16": dv.setUint16(0, Number(x) & 0xffff, true); break;
        case "i32": case "u32": dv.setUint32(0, Number(x) >>> 0, true); break;
        case "char": dv.setUint32(0, x.codePointAt(0), true); break;
        case "f32": dv.setFloat32(0, x, true); break;
        case "f64": dv.setFloat64(0, x, true); break;
        case "i64": case "u64": dv.setBigUint64(0, BigInt.asUintN(64, BigInt(x)), true); break;
        default: {
          const v = BigInt.asUintN(128, BigInt(x));
          dv.setBigUint64(0, v & 0xffffffffffffffffn, true);
          dv.setBigUint64(8, v >> 64n, true);
        }
      }
      switch (to) {
        case "i8": return dv.getInt8(0);
        case "u8": return dv.getUint8(0);
        case "bool": return dv.getUint8(0) !== 0;
        case "i16": return dv.getInt16(0, true);
        case "u16": return dv.getUint16(0, true);
        case "i32": return dv.getInt32(0, true);
        case "u32": return dv.getUint32(0, true);
        case "char": return String.fromCodePoint(dv.getUint32(0, true));
        case "f32": return dv.getFloat32(0, true);
        case "f64": return dv.getFloat64(0, true);
        case "i64": return Number(dv.getBigInt64(0, true));
        case "u64": return Number(dv.getBigUint64(0, true));
        default: {
          const v = dv.getBigUint64(0, true) | (dv.getBigUint64(8, true) << 64n);
          return to === "i128" ? BigInt.asIntN(128, v) : v;
        }
      }
    },
    // `f.debug_struct("P")` and friends; see `DbgBuilder`.
    DbgBuilder,
    // `fmt::Error`.
    $fmtError: new ParseError("Error", ""),
    // `w.write_str(s)` / `write!(w, ..)` for a `w: &mut W` with `W: fmt::Write`: `c` is its cell,
    // holding a `String`, a Formatter or your own writer.
    $fmtWriteCall(c, method, s) {
      const w = c[0];
      if (typeof w === "string") {
        c[0] = w + s;
        return new Res(0, undefined);
      }
      if (w !== null && typeof w === "object" && typeof w[method] === "function") return w[method](s);
      return rt.$write(w, s, false);
    },
    // A `String` or `Formatter` as a `&mut dyn fmt::Write` (a struct field, a closure's
    // parameter, a `Box`): a writer whose methods write into the cell `c` over it.
    $fmtWriter(c) {
      const put = (s) => rt.$fmtWriteCall(c, "", s);
      return {
        write_str: put,
        write_char: put,
        write_fmt: put,
        __write_str: put,
      };
    },
    // Awaiting a hand-written future (`impl Future for X`, whose class's `then` calls this):
    // poll it with a context from `mkCx` (the task prelude's `__task_context`), and after each
    // `Pending` wait until its waker is woken (a future that never wakes it never finishes).
    async $drive(fut, method, mkCx) {
      for (;;) {
        let wake;
        const woken = new Promise((r) => (wake = r));
        const r = fut[method](mkCx(() => wake()));
        if (r.$tag === 0) return r[0];
        await woken;
      }
    },
    // `fut.poll(cx)` on a future that may be a promise (an async fn's or block's): `Pending`
    // until it settles (then the context's waker is woken), then `Ready` with its value. A
    // hand-written future's own `poll` is called.
    $poll(fut, cx, method, Poll) {
      if (fut !== null && typeof fut === "object" && typeof fut[method] === "function") {
        return fut[method](cx);
      }
      let st = fut.$pollState;
      if (st === undefined) {
        st = fut.$pollState = { done: false, failed: false, value: undefined, wakers: [] };
        Promise.resolve(fut).then(
          (v) => {
            st.done = true;
            st.value = v;
            for (const w of st.wakers.splice(0)) w.wake_by_ref();
          },
          (e) => {
            st.failed = true;
            st.value = e;
            for (const w of st.wakers.splice(0)) w.wake_by_ref();
          },
        );
      }
      if (st.failed) throw st.value;
      if (st.done) return Poll.Ready(st.value);
      st.wakers.push(cx.waker());
      return Poll.Pending;
    },
    // `Display` impls write into a Formatter: `{ s: "" }`.
    $write(f, s, nl) {
      f.s += nl ? s + "\n" : s;
      return new Res(0, undefined);
    },
    $display(v) {
      const f = { s: "" };
      v.fmt(f);
      return f.s;
    },
    // `{}` / `{:?}` of a `dyn Tr` through `impl Display` / `Debug for Box<dyn Tr>`: `fmt` is
    // that impl's, called with the value as `self`.
    $fmtVia(v, fmt) {
      const f = { s: "" };
      fmt.call(v, f);
      return f.s;
    },
    // `==` on `Vec`s / `Option`s of such a `dyn Tr` (`impl PartialEq for Box<dyn Tr>`): element
    // by element through that impl's `eq`.
    $eqBy(a, b, eq) {
      return a.length === b.length && a.every((x, i) => eq(x, b[i]));
    },
    $eqOptBy(a, b, eq) {
      return a == null ? b == null : b != null && eq(unsome(a), unsome(b));
    },
    // `{:>8}` / `{:.2?}` of a value with its own `fmt`: the impl gets the spec
    // `o = { w, p, align, fill, plus, alt, zero }` (`f.width()`, `f.precision()`, ..) and, like
    // Rust, pads only if it asks to (`f.pad(..)`). A derived `{:?}` (`dbg`) is padded here.
    $displaySpec(v, o) {
      const f = { s: "", o };
      v.fmt(f);
      return f.s;
    },
    $dbgSpec(v, o, dbg) {
      if (v !== null && typeof v === "object" && typeof v.fmt$dbg === "function") {
        const f = { s: "", o };
        v.fmt$dbg(f);
        return f.s;
      }
      let s = dbg(v);
      if (o.alt) s = rt.$pretty(s);
      if (o.w != null) s = rt.$pad(s, o.w, o.align, o.fill, o.zero, false);
      return s;
    },
    // `f.pad(s)`: cut to the precision, then pad to the width (left-aligned by default).
    $fmtPad(f, s, cut) {
      const o = f.o;
      if (o && cut && o.p != null) s = rt.$truncChars(s, o.p);
      if (o && o.w != null) s = rt.$pad(s, o.w, o.align, o.fill, false, false);
      return rt.$write(f, s, false);
    },
    RefCell,
    // `c.borrow()` / `c.borrow_mut()` as a temporary: panics while a held borrow conflicts.
    $refBorrow(c, mut, location) {
      if (mut ? c.b !== 0 : c.b < 0) {
        throw new CarciniPanic(mut ? "RefCell already borrowed" : "RefCell already mutably borrowed", location);
      }
      return c;
    },
    // `let g = c.borrow_mut();`: the borrow is held until the returned release runs (the end of
    // the block, or `drop(g)`).
    $refHold(c, mut, location) {
      rt.$refBorrow(c, mut, location);
      c.b = mut ? -1 : c.b + 1;
      let held = true;
      return () => {
        if (held) c.b = mut ? 0 : c.b - 1;
        held = false;
      };
    },
    // `c.try_borrow()` / `c.try_borrow_mut()`: `Ok` of the value, or the `Err` a held borrow makes.
    $tryBorrow(c, mut) {
      if (mut ? c.b !== 0 : c.b < 0) return rt.Err(new ParseError(mut ? "BorrowMutError" : "BorrowError", ""));
      return rt.Ok(c.v);
    },
    $cellReplace(c, v) {
      const old = c.v;
      c.v = v;
      return old;
    },
    // Result.
    Ok: (v) => new Res(0, v),
    Err: (e) => new Res(1, e),
    $dbgRes(r, tf, ef) {
      return r.$tag ? `Err(${ef(r[0])})` : `Ok(${tf(r[0])})`;
    },
    $resUnwrap(r, ef, location) {
      if (r.$tag) throw new CarciniPanic(`called \`Result::unwrap()\` on an \`Err\` value: ${ef(r[0])}`, location);
      return r[0];
    },
    $resExpect(r, msg, ef, location) {
      if (r.$tag) throw new CarciniPanic(`${msg}: ${ef(r[0])}`, location);
      return r[0];
    },
    $resUnwrapErr(r, tf, location) {
      if (!r.$tag) throw new CarciniPanic(`called \`Result::unwrap_err()\` on an \`Ok\` value: ${tf(r[0])}`, location);
      return r[0];
    },
    $resExpectErr(r, msg, tf, location) {
      if (!r.$tag) throw new CarciniPanic(`${msg}: ${tf(r[0])}`, location);
      return r[0];
    },
    $resOr(r, d) {
      return r.$tag ? d : r[0];
    },
    $resOrElse(r, f) {
      return r.$tag ? f(r[0]) : r[0];
    },
    // `r.as_mut()` unwrapped, of a number: `Ok` of a slot over the payload, which stays in `r`.
    $resSlot(r) {
      if (r.$tag) return r;
      return new Res(
        0,
        rt.$cell(
          () => r[0],
          (v) => {
            r[0] = v;
          },
        ),
      );
    },
    $resOk(r) {
      return r.$tag ? null : r[0];
    },
    $resErr(r) {
      return r.$tag ? r[0] : null;
    },
    $resMap(r, f) {
      return r.$tag ? r : new Res(0, f(r[0]));
    },
    $resMapErr(r, f) {
      return r.$tag ? new Res(1, f(r[0])) : r;
    },
    $resAndThen(r, f) {
      return r.$tag ? r : f(r[0]);
    },
    $resOrElseRes(r, f) {
      return r.$tag ? f(r[0]) : r;
    },
    // `r.or(other)` / `r.and(other)`.
    $resOr2(r, o) {
      return r.$tag ? o : r;
    },
    $resAnd2(r, o) {
      return r.$tag ? r : o;
    },
    $resIsOkAnd(r, f) {
      return !r.$tag && f(r[0]);
    },
    $resIsErrAnd(r, f) {
      return r.$tag === 1 && f(r[0]);
    },
    $resMapOr(r, d, f) {
      return r.$tag ? d : f(r[0]);
    },
    $okOr(x, e) {
      return x == null ? new Res(1, e) : new Res(0, x);
    },
    $okOrElse(x, f) {
      return x == null ? new Res(1, f()) : new Res(0, x);
    },
    // `str::parse`, following std's rules: an optional sign (`-` only for signed types), then
    // ASCII digits; no whitespace.
    $parseInt(s, min, max) {
      const err = (kind) => new Res(1, new ParseError("ParseIntError", kind));
      if (s === "") return err("Empty");
      let body = s;
      let neg = false;
      if (body[0] === "+" || body[0] === "-") {
        neg = body[0] === "-";
        body = body.slice(1);
        if (neg && min >= 0) return err("InvalidDigit");
      }
      if (body === "" || !/^[0-9]+$/.test(body)) return err("InvalidDigit");
      const v = BigInt(neg ? "-" + body : body);
      if (v > BigInt(max)) return err("PosOverflow");
      if (v < BigInt(min)) return err("NegOverflow");
      return new Res(0, Number(v));
    },
    // `i32::from_str_radix(s, 16)`: digits up to the radix (either case), an optional sign.
    // `u128::from_str_radix(s, 16)`: a BigInt.
    $fromStrRadixBig(s, radix, signed, bits) {
      const B = BigInt(bits);
      const min = signed ? -(1n << (B - 1n)) : 0n;
      const max = signed ? (1n << (B - 1n)) - 1n : (1n << B) - 1n;
      const r = rt.$fromStrRadix(s, radix, min, max, true);
      return r;
    },
    $fromStrRadix(s, radix, min, max, big = false) {
      const err = (kind) => new Res(1, new ParseError("ParseIntError", kind));
      if (s === "") return err("Empty");
      let body = s;
      let neg = false;
      if (body[0] === "+" || body[0] === "-") {
        neg = body[0] === "-";
        body = body.slice(1);
        if (neg && min >= 0) return err("InvalidDigit");
      }
      if (body === "") return err("InvalidDigit");
      let v = 0n;
      for (const c of body) {
        const d = /^[0-9a-zA-Z]$/.test(c) ? parseInt(c, 36) : 99;
        if (d >= radix) return err("InvalidDigit");
        v = v * BigInt(radix) + BigInt(d);
      }
      if (neg) v = -v;
      if (v > BigInt(max)) return err("PosOverflow");
      if (v < BigInt(min)) return err("NegOverflow");
      return new Res(0, big ? v : Number(v));
    },
    // `t.as_ref()` with an impl's `T: AsRef<[u8]>`: a string's UTF-8 bytes, else the bytes as they are.
    $asRefBytes(v) {
      return typeof v === "string" ? rt.$utf8(v) : v;
    },
    // `s.as_bytes()`: UTF-8.
    $utf8(s) {
      const info = strInfo(s);
      if (!info.ascii) return Array.from(strBytes(info));
      const a = new Array(s.length);
      for (let i = 0; i < s.length; i++) a[i] = s.charCodeAt(i);
      return a;
    },
    // (`owned`: `String::from_utf8`, whose error is a `FromUtf8Error` holding the bytes.)
    $fromUtf8(bytes, owned) {
      try {
        return new Res(0, new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(Uint8Array.from(bytes)));
      } catch {
        return new Res(1, new Utf8Error(bytes, owned));
      }
    },
    $fromUtf8Lossy(bytes) {
      return new TextDecoder("utf-8", { ignoreBOM: true }).decode(Uint8Array.from(bytes));
    },
    // `String::from_utf8_lossy` as a `Cow<str>`: `mk(owned, s)` builds `Owned` when a byte was replaced.
    $fromUtf8LossyCow(bytes, mk) {
      const u = Uint8Array.from(bytes);
      try {
        return mk(false, new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(u));
      } catch {
        return mk(true, new TextDecoder("utf-8", { ignoreBOM: true }).decode(u));
      }
    },
    $parseFloat(s) {
      const err = (kind) => new Res(1, new ParseError("ParseFloatError", kind));
      if (s === "") return err("Empty");
      if (!/^[+-]?(inf|infinity|nan|([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?)$/i.test(s)) return err("Invalid");
      const neg = s[0] === "-";
      const body = s.replace(/^[+-]/, "").toLowerCase();
      if (body === "inf" || body === "infinity") return new Res(0, neg ? -Infinity : Infinity);
      if (body === "nan") return new Res(0, NaN);
      return new Res(0, Number(s));
    },
    $parseBool(s) {
      if (s === "true") return new Res(0, true);
      if (s === "false") return new Res(0, false);
      return new Res(1, new ParseError("ParseBoolError", ""));
    },
    $parseChar(s) {
      const cs = Array.from(s);
      if (cs.length === 1) return new Res(0, cs[0]);
      return new Res(1, new ParseError("ParseCharError", cs.length ? "TooManyChars" : "EmptyString"));
    },

    // `&v[lo..hi]` (hi null for `lo..`): a copy, with core's bounds checks and messages.
    $slice(v, lo, hi, inclusive, location) {
      const n = v.length;
      if (hi === null) {
        if (lo > n) throw new CarciniPanic(`range start index ${lo} out of range for slice of length ${n}`, location);
        return v.slice(lo);
      }
      const end = inclusive ? hi + 1 : hi;
      if (lo > end) throw new CarciniPanic(`slice index starts at ${lo} but ends at ${end}`, location);
      if (end > n) throw new CarciniPanic(`range end index ${hi} out of range for slice of length ${n}`, location);
      return v.slice(lo, end);
    },
    // `&mut v[lo..hi]`: a view of that part of `v` (a Proxy), so writes, sorts and swaps through it
    // reach `v`. Bounds checks as `$slice`.
    $sliceMut(v, lo, hi, inclusive, location) {
      const n = v.length;
      const end = hi === null ? n : inclusive ? hi + 1 : hi;
      if (hi === null && lo > n) throw new CarciniPanic(`range start index ${lo} out of range for slice of length ${n}`, location);
      if (lo > end) throw new CarciniPanic(`slice index starts at ${lo} but ends at ${end}`, location);
      if (end > n) throw new CarciniPanic(`range end index ${hi} out of range for slice of length ${n}`, location);
      // A view of a view is a view of its base; a deque's contents are its backing array.
      const base = v[VIEW];
      if (base !== undefined) return sliceView(base.v, base.lo + lo, end - lo);
      return sliceView(arr(v), lo, end - lo);
    },
    // `v.get(lo..hi)` / `v.get_mut(lo..hi)` (hi null for `lo..`): the slice (a view for
    // `get_mut`), or `None` (null) when the range doesn't fit.
    $getRange(v, lo, hi, inclusive, mutable) {
      const n = v.length;
      const end = hi === null ? n : inclusive ? hi + 1 : hi;
      if (lo > end || end > n) return null;
      return mutable ? this.$sliceMut(v, lo, hi, inclusive, "<std>") : v.slice(lo, end);
    },
    // `v.get(r)` with a range value.
    $getRangeBy(v, r) {
      const it = r.it;
      return this.$getRange(v, it.i, it.hi, it.incl, false);
    },
    // `v.split_at_mut(mid)`: views of both halves.
    $splitAtMut(v, mid, location) {
      if (mid > v.length) throw new CarciniPanic("mid > len", location);
      return [this.$sliceMut(v, 0, mid, false, location), this.$sliceMut(v, mid, null, false, location)];
    },
    // `&s[lo..hi]` with UTF-8 byte offsets, panicking like `str` indexing does.
    $strSlice(s, lo, hi, inclusive, location) {
      // ASCII: byte offsets are UTF-16 offsets.
      if (strInfo(s).ascii) {
        const len = s.length;
        const end = hi === null ? len : inclusive ? hi + 1 : hi;
        if (lo > len) throw new CarciniPanic(`start byte index ${lo} is out of bounds for string of length ${len}`, location);
        if (end > len) throw new CarciniPanic(`end byte index ${end} is out of bounds for string of length ${len}`, location);
        if (lo > end) throw new CarciniPanic(`byte range starts at ${lo} but ends at ${end}`, location);
        return s.slice(lo, end);
      }
      // UTF-8 offset of each code point, and the matching UTF-16 offset.
      const bytes = [];
      const units = [];
      const chars = [];
      let b = 0;
      let u = 0;
      for (const c of s) {
        bytes.push(b);
        units.push(u);
        chars.push(c);
        const cp = c.codePointAt(0);
        b += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
        u += c.length;
      }
      const len = b;
      const end = hi === null ? len : inclusive ? hi + 1 : hi;
      const fail = (msg) => {
        throw new CarciniPanic(msg, location);
      };
      if (lo > len) fail(`start byte index ${lo} is out of bounds for string of length ${len}`);
      if (end > len) fail(`end byte index ${end} is out of bounds for string of length ${len}`);
      if (lo > end) fail(`byte range starts at ${lo} but ends at ${end}`);
      const unit = (byte) => {
        if (byte === len) return u;
        const i = bytes.indexOf(byte);
        return i < 0 ? -1 : units[i];
      };
      for (const [which, at] of [["start", lo], ["end", end]]) {
        if (unit(at) < 0) {
          const i = bytes.findIndex((x, k) => x < at && (k + 1 === bytes.length ? len : bytes[k + 1]) > at);
          const next = i + 1 < bytes.length ? bytes[i + 1] : len;
          fail(
            `${which} byte index ${at} is not a char boundary; it is inside ${rt.$dbgChar(chars[i])} (bytes ${bytes[i]}..${next} of string)`,
          );
        }
      }
      return s.slice(unit(lo), unit(end));
    },
    // ---- std surface: strings
    // A `[char]` pattern as the predicate the pattern helpers below take.
    $charPat(cs) {
      return (c) => cs.includes(c);
    },
    // A string pattern as a predicate over one char, or null for a `&str` pattern.
    $strFind(s, p, rev) {
      let at;
      if (typeof p === "function") {
        const units = [];
        let u = 0;
        for (const c of s) {
          if (p(c)) units.push(u);
          u += c.length;
        }
        at = units.length ? (rev ? units[units.length - 1] : units[0]) : -1;
      } else {
        at = rev ? s.lastIndexOf(p) : s.indexOf(p);
      }
      return at < 0 ? null : strInfo(s).ascii ? at : utf8Len(s.slice(0, at));
    },
    // Where pattern `p` matches in `s`: [start, end] in UTF-16 units, first or last.
    $splitOnce(s, p, rev) {
      let at, len;
      if (typeof p === "function") {
        let u = 0;
        at = -1;
        for (const c of s) {
          if (p(c) && (rev || at < 0)) {
            at = u;
            len = c.length;
          }
          u += c.length;
        }
      } else {
        at = rev ? s.lastIndexOf(p) : s.indexOf(p);
        len = p.length;
      }
      return at < 0 ? null : [s.slice(0, at), s.slice(at + len)];
    },
    $stripPrefix(s, p) {
      if (typeof p === "function") {
        const c = Array.from(s)[0];
        return c !== undefined && p(c) ? s.slice(c.length) : null;
      }
      return s.startsWith(p) ? s.slice(p.length) : null;
    },
    $stripSuffix(s, p) {
      if (typeof p === "function") {
        const cs = Array.from(s);
        const c = cs[cs.length - 1];
        return c !== undefined && p(c) ? s.slice(0, s.length - c.length) : null;
      }
      return s.endsWith(p) ? s.slice(0, s.length - p.length) : null;
    },
    $trimMatches(s, p, start, end) {
      if (typeof p === "function" || Array.from(p).length === 1) {
        const test = typeof p === "function" ? p : (c) => c === p;
        const cs = Array.from(s);
        let i = 0;
        let j = cs.length;
        if (start) while (i < j && test(cs[i])) i++;
        if (end) while (j > i && test(cs[j - 1])) j--;
        return cs.slice(i, j).join("");
      }
      if (p === "") return s;
      if (start) while (s.startsWith(p)) s = s.slice(p.length);
      if (end) while (s.endsWith(p)) s = s.slice(0, s.length - p.length);
      return s;
    },
    $strContains(s, p) {
      return typeof p === "function" ? Array.from(s).some(p) : s.includes(p);
    },
    $strStartsWith(s, p) {
      return rt.$stripPrefix(s, p) !== null;
    },
    $strEndsWith(s, p) {
      return rt.$stripSuffix(s, p) !== null;
    },
    // `replace` (n = Infinity) / `replacen`.
    $strReplace(s, p, to, n) {
      let out = "";
      let count = 0;
      if (typeof p === "function") {
        for (const c of s) {
          if (count < n && p(c)) {
            out += to;
            count++;
          } else out += c;
        }
        return out;
      }
      if (p === "") {
        // Rust inserts `to` between every char (and at both ends).
        const cs = Array.from(s);
        for (let i = 0; i <= cs.length; i++) {
          if (count < n) {
            out += to;
            count++;
          }
          if (i < cs.length) out += cs[i];
        }
        return out;
      }
      let i = 0;
      while (count < n) {
        const at = s.indexOf(p, i);
        if (at < 0) break;
        out += s.slice(i, at) + to;
        i = at + p.length;
        count++;
      }
      return out + s.slice(i);
    },
    $charIndices(s) {
      return Iter.of(function* () {
        let b = 0;
        for (const c of s) {
          yield [b, c];
          const cp = c.codePointAt(0);
          b += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
        }
      });
    },
    $bytes(s) {
      return new Iter(new TextEncoder().encode(s)[Symbol.iterator]());
    },
    // The pieces between matches of `p`, left to right.
    $strPieces(s, p) {
      if (typeof p !== "function") return s.split(p);
      const out = [];
      let cur = "";
      for (const c of s) {
        if (p(c)) {
          out.push(cur);
          cur = "";
        } else cur += c;
      }
      out.push(cur);
      return out;
    },
    $rsplit(s, p) {
      return new Iter(rt.$strPieces(s, p).reverse()[Symbol.iterator]());
    },
    $splitTerminator(s, p) {
      const parts = rt.$strPieces(s, p);
      if (parts.length && parts[parts.length - 1] === "") parts.pop();
      return new Iter(parts[Symbol.iterator]());
    },
    $strMatches(s, p) {
      const out = [];
      if (typeof p === "function") {
        for (const c of s) if (p(c)) out.push(c);
      } else if (p !== "") {
        let i = 0;
        for (;;) {
          const at = s.indexOf(p, i);
          if (at < 0) break;
          out.push(p);
          i = at + p.length;
        }
      }
      return new Iter(out[Symbol.iterator]());
    },
    // `splitn` / `rsplitn`: at most n pieces; the last holds the rest of the string.
    $splitn(s, n, p, rev) {
      const parts = rt.$strPieces(s, p);
      if (n === 0) return new Iter([][Symbol.iterator]());
      if (parts.length <= n) return new Iter((rev ? parts.reverse() : parts)[Symbol.iterator]());
      const sep = typeof p === "function" ? null : p;
      if (sep === null) {
        // Closure patterns: rebuild the rest from the original string.
        const cs = Array.from(s);
        const out = [];
        let cur = "";
        let i = rev ? cs.length - 1 : 0;
        const step = rev ? -1 : 1;
        while (rev ? i >= 0 : i < cs.length) {
          if (out.length === n - 1) {
            cur = rev ? cs.slice(0, i + 1).join("") : cs.slice(i).join("");
            break;
          }
          if (p(cs[i])) {
            out.push(cur);
            cur = "";
          } else cur = rev ? cs[i] + cur : cur + cs[i];
          i += step;
        }
        out.push(cur);
        return new Iter(out[Symbol.iterator]());
      }
      const out = rev ? parts.slice(parts.length - n + 1).reverse() : parts.slice(0, n - 1);
      const rest = rev ? parts.slice(0, parts.length - n + 1).join(sep) : parts.slice(n - 1).join(sep);
      out.push(rest);
      return new Iter(out[Symbol.iterator]());
    },
    $eqIgnoreAsciiCase(a, b) {
      return rt.$asciiLower(a) === rt.$asciiLower(b);
    },
    $isCharBoundary(s, i) {
      if (i === 0) return true;
      const info = strInfo(s);
      if (info.ascii) return i <= s.length;
      if (s.length >= strInfoMin) {
        // Not a UTF-8 continuation byte (0b10xxxxxx).
        const bytes = strBytes(info);
        return i === bytes.length || (i < bytes.length && (bytes[i] & 0xc0) !== 0x80);
      }
      let b = 0;
      for (const c of s) {
        if (b === i) return true;
        if (b > i) return false;
        const cp = c.codePointAt(0);
        b += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
      }
      return b === i;
    },
    // The UTF-16 offset of byte offset `i`, or -1 if it isn't on a char boundary.
    $unitOf(s, i) {
      if (strInfo(s).ascii) return i <= s.length ? i : -1;
      let b = 0;
      let u = 0;
      for (const c of s) {
        if (b === i) return u;
        if (b > i) return -1;
        const cp = c.codePointAt(0);
        b += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
        u += c.length;
      }
      return b === i ? u : -1;
    },
    $strInsert(s, i, x, location) {
      const u = rt.$unitOf(s, i);
      if (u < 0) throw new CarciniPanic("assertion failed: self.is_char_boundary(idx)", location);
      return s.slice(0, u) + x + s.slice(u);
    },
    $strTruncate(s, n, location) {
      if (n >= rt.$utf8len(s)) return s;
      const u = rt.$unitOf(s, n);
      if (u < 0) throw new CarciniPanic("assertion failed: self.is_char_boundary(new_len)", location);
      return s.slice(0, u);
    },
    $strPop(s) {
      const cs = Array.from(s);
      if (!cs.length) return [null, s];
      const c = cs.pop();
      return [c, s.slice(0, s.length - c.length)];
    },
    $strRemove(s, i, location) {
      const u = rt.$unitOf(s, i);
      if (i >= rt.$utf8len(s)) throw new CarciniPanic("cannot remove a char from the end of a string", location);
      if (u < 0) throw new CarciniPanic("assertion failed: self.is_char_boundary(idx)", location);
      const c = String.fromCodePoint(s.codePointAt(u));
      return [c, s.slice(0, u) + s.slice(u + c.length)];
    },

    // `s.retain(f)`: the chars `f` keeps.
    $strRetain(s, f) {
      let out = "";
      for (const c of s) if (f(c)) out += c;
      return out;
    },
    // `s.replace_range(range, x)`, with slicing's panics.
    $strReplaceRange(s, lo, hi, inclusive, x, location) {
      rt.$strSlice(s, lo, hi, inclusive, location);
      const end = hi === null ? null : inclusive ? hi + 1 : hi;
      return rt.$strSlice(s, 0, lo, false, location) + x + (end === null ? "" : rt.$strSlice(s, end, null, false, location));
    },
    // `s.drain(range)`: [the range's chars, the string without them].
    $strDrain(s, lo, hi, inclusive, location) {
      const mid = rt.$strSlice(s, lo, hi, inclusive, location);
      const end = hi === null ? null : inclusive ? hi + 1 : hi;
      const rest = rt.$strSlice(s, 0, lo, false, location) + (end === null ? "" : rt.$strSlice(s, end, null, false, location));
      return [new Iter(Array.from(mid)[Symbol.iterator]()), rest];
    },
    // `s.split_off(at)`: [the tail, the head].
    $strSplitOff(s, at, location) {
      const u = rt.$unitOf(s, at);
      if (u < 0) throw new CarciniPanic("assertion failed: self.is_char_boundary(at)", location);
      return [s.slice(u), s.slice(0, u)];
    },
    // `s.match_indices(p)` / `rmatch_indices(p)`: (byte offset, matched text), non-overlapping.
    $matchIndices(s, p, rev) {
      const out = [];
      let u = 0;
      let b = 0;
      const bytesTo = (to) => {
        // Advance the byte count from UTF-16 offset `u` to `to`.
        b += utf8Len(s.slice(u, to));
        u = to;
        return b;
      };
      if (typeof p === "function") {
        for (const c of s) {
          if (p(c)) out.push([bytesTo(u), c]);
          bytesTo(u + c.length);
        }
      } else if (p === "") {
        for (const c of s) {
          out.push([bytesTo(u), ""]);
          bytesTo(u + c.length);
        }
        out.push([bytesTo(s.length), ""]);
      } else if (rev) {
        // From the end, as std's searcher does (matters for overlapping patterns like "aa").
        const at = [];
        let i = s.length;
        while (i >= p.length) {
          const k = s.lastIndexOf(p, i - p.length);
          if (k < 0) break;
          at.push(k);
          i = k;
        }
        for (const k of at.reverse()) out.push([bytesTo(k), p]);
      } else {
        let i = 0;
        for (;;) {
          const k = s.indexOf(p, i);
          if (k < 0) break;
          out.push([bytesTo(k), p]);
          i = k + p.length;
        }
      }
      return new Iter((rev ? out.reverse() : out)[Symbol.iterator]());
    },
    // `s.split_inclusive(p)`: the pieces, each ending with its match; no empty last piece.
    $splitInclusive(s, p) {
      const out = [];
      let cur = "";
      if (typeof p === "function") {
        for (const c of s) {
          cur += c;
          if (p(c)) {
            out.push(cur);
            cur = "";
          }
        }
      } else if (p === "") {
        for (const c of s) out.push(c);
      } else {
        let i = 0;
        for (;;) {
          const k = s.indexOf(p, i);
          if (k < 0) break;
          out.push(s.slice(i, k + p.length));
          i = k + p.length;
        }
        cur = s.slice(i);
      }
      if (cur !== "") out.push(cur);
      return new Iter(out[Symbol.iterator]());
    },
    $splitAsciiWhitespace(s) {
      return new Iter(s.split(/[ \t\n\f\r]+/).filter((w) => w !== "")[Symbol.iterator]());
    },
    $encodeUtf16(s) {
      return Iter.of(function* () {
        for (let i = 0; i < s.length; i++) yield s.charCodeAt(i);
      });
    },
    // `s.escape_debug()` / `escape_default()` / `escape_unicode()`, as a string. (Debug escapes
    // grapheme extenders only at the start.)
    $strEscape(s, how) {
      let out = "";
      let first = true;
      for (const c of s) {
        out += how === "debug" && !first && /^\p{Grapheme_Extend}$/u.test(c) ? c : rt.$escapeChar(c, how);
        first = false;
      }
      return out;
    },
    // `char::decode_utf16(units)`: `Ok` of each character, `Err` of each unpaired surrogate (a
    // high one's next unit is read again).
    $decodeUtf16(units) {
      const it = rt.$iter(units)[Symbol.iterator]();
      return Iter.of(function* () {
        let held = null;
        for (;;) {
          let u;
          if (held !== null) {
            u = held;
            held = null;
          } else {
            const r = it.next();
            if (r.done) return;
            u = r.value;
          }
          if (u < 0xd800 || u > 0xdfff) yield new Res(0, String.fromCharCode(u));
          else if (u >= 0xdc00) yield new Res(1, new DecodeUtf16Error(u));
          else {
            const r = it.next();
            if (r.done) {
              yield new Res(1, new DecodeUtf16Error(u));
              return;
            }
            const lo = r.value;
            if (lo >= 0xdc00 && lo <= 0xdfff) yield new Res(0, String.fromCharCode(u, lo));
            else {
              held = lo;
              yield new Res(1, new DecodeUtf16Error(u));
            }
          }
        }
      });
    },
    // `String::from_utf16(&units)`: an error on a lone surrogate.
    $fromUtf16(units) {
      for (let i = 0; i < units.length; i++) {
        const u = units[i];
        if (u >= 0xd800 && u <= 0xdbff && i + 1 < units.length && units[i + 1] >= 0xdc00 && units[i + 1] <= 0xdfff) i++;
        else if (u >= 0xd800 && u <= 0xdfff) return new Res(1, new ParseError("FromUtf16Error", "LoneSurrogate"));
      }
      return new Res(0, rt.$fromUtf16Lossy(units));
    },
    $fromUtf16Lossy(units) {
      let out = "";
      for (let i = 0; i < units.length; i++) {
        const u = units[i];
        if (u >= 0xd800 && u <= 0xdbff && i + 1 < units.length && units[i + 1] >= 0xdc00 && units[i + 1] <= 0xdfff) {
          out += String.fromCharCode(u, units[i + 1]);
          i++;
        } else out += u >= 0xd800 && u <= 0xdfff ? "�" : String.fromCharCode(u);
      }
      return out;
    },


    // ---- std surface: numbers
    $checked(r, min, max) {
      return Number.isFinite(r) && r >= min && r <= max ? r : null;
    },
    $checkedDiv(a, b) {
      return b === 0 ? NaN : Math.trunc(a / b);
    },
    $checkedRem(a, b) {
      return b === 0 ? NaN : a % b;
    },
    $saturate(r, min, max) {
      return r < min ? min : r > max ? max : r;
    },
    // Bits of `x` as a two's-complement integer of `bits` width.
    $bitsOf(x, bits) {
      return BigInt.asUintN(bits, BigInt(Math.trunc(x)));
    },
    $countOnes(x, bits) {
      let v = rt.$bitsOf(x, bits);
      let n = 0;
      while (v) {
        n += Number(v & 1n);
        v >>= 1n;
      }
      return n;
    },
    $countZeros(x, bits) {
      return bits - rt.$countOnes(x, bits);
    },
    $leadingZeros(x, bits) {
      const v = rt.$bitsOf(x, bits);
      return v === 0n ? bits : bits - v.toString(2).length;
    },
    $trailingZeros(x, bits) {
      let v = rt.$bitsOf(x, bits);
      if (v === 0n) return bits;
      let n = 0;
      while (!(v & 1n)) {
        v >>= 1n;
        n++;
      }
      return n;
    },
    $isPowerOfTwo(x) {
      return x > 0 && rt.$countOnes(x, 64) === 1;
    },
    // `f64::total_cmp`: -0 < +0, and NaN after everything.
    $totalCmp(a, b) {
      const key = (x) => (Number.isNaN(x) ? [2, 0] : [1, Object.is(x, -0) ? -Number.MIN_VALUE / 2 : x]);
      const [ka, va] = key(a);
      const [kb, vb] = key(b);
      if (ka !== kb) return ka < kb ? -1 : 1;
      if (Object.is(a, -0) && Object.is(b, 0)) return -1;
      if (Object.is(a, 0) && Object.is(b, -0)) return 1;
      return va < vb ? -1 : va > vb ? 1 : 0;
    },
    $fromDigit(d, radix) {
      if (radix > 36) throw new CarciniPanic("from_digit: radix is too high (maximum 36)", "<std>");
      return d < radix ? d.toString(radix) : null;
    },
    $fromU32(n) {
      return n > 0x10ffff || (n >= 0xd800 && n <= 0xdfff) ? null : String.fromCodePoint(n);
    },

    // ---- std surface: Vec
    // std's binary search: `f` compares an element with the target.
    $binarySearchBy(v, f) {
      v = arr(v);
      let size = v.length;
      if (size === 0) return new Res(1, 0);
      let base = 0;
      while (size > 1) {
        const half = size >> 1;
        const mid = base + half;
        if (f(v[mid]) <= 0) base = mid;
        size -= half;
      }
      const c = f(v[base]);
      return c === 0 ? new Res(0, base) : new Res(1, base + (c < 0 ? 1 : 0));
    },
    $splitAt(v, n, location) {
      if (n > v.length) throw new CarciniPanic("mid > len", location);
      return [v.slice(0, n), v.slice(n)];
    },
    $vecStartsWith(v, p) {
      v = arr(v);
      return p.length <= v.length && p.every((x, i) => eq(x, v[i]));
    },
    $vecEndsWith(v, p) {
      v = arr(v);
      const off = v.length - p.length;
      return off >= 0 && p.every((x, i) => eq(x, v[off + i]));
    },
    $vecRepeatAll(v, n) {
      const out = [];
      for (let i = 0; i < n; i++) for (const x of v) out.push(clone(x));
      return out;
    },
    $isSorted(v) {
      v = arr(v);
      for (let i = 1; i < v.length; i++) if (!(cmp(v[i - 1], v[i]) <= 0)) return false;
      return true;
    },
    $drain(v, lo, hi, inclusive, location) {
      const out = rt.$slice(v, lo, hi, inclusive, location);
      v.splice(lo, out.length);
      return new Iter(out[Symbol.iterator]());
    },
    $splitOff(v, at, location) {
      if (at > v.length) {
        throw new CarciniPanic(`\`at\` split index (is ${at}) should be <= len (is ${v.length})`, location);
      }
      const out = v.splice(at);
      return v instanceof Deque ? Deque.from(out) : out;
    },
    $append(v, other) {
      for (const x of other) v.push(x);
      other.length = 0;
    },
    $fill(v, x) {
      v = arr(v);
      for (let i = 0; i < v.length; i++) v[i] = clone(x);
    },
    $copyFromSlice(v, src, location) {
      v = arr(v);
      if (src.length !== v.length) {
        throw new CarciniPanic(
          `source slice length (${src.length}) does not match destination slice length (${v.length})`,
          location,
        );
      }
      for (let i = 0; i < v.length; i++) v[i] = clone(src[i]);
    },
    $rotate(v, k, left, location) {
      if (k > v.length) {
        throw new CarciniPanic(left ? "assertion failed: mid <= self.len()" : "assertion failed: k <= self.len()", location);
      }
      const n = left ? k : v.length - k;
      // (A view can't grow or shrink: rotate a copy and write it back.)
      if (v[VIEW] !== undefined) {
        const c = v.slice();
        rt.$writeBack(v, 0, c.slice(n).concat(c.slice(0, n)));
        return;
      }
      const moved = v.splice(0, n);
      for (const x of moved) v.push(x);
    },
    $swapRemove(v, i, location) {
      if (i >= v.length) {
        throw new CarciniPanic(`swap_remove index (is ${i}) should be < len (is ${v.length})`, location);
      }
      v = arr(v);
      const x = v[i];
      const last = v.pop();
      if (i < v.length) v[i] = last;
      return x;
    },

    // ---- std::mem on `&mut` parameters (objects shared with the caller)
    $swapContents(a, b) {
      if (Array.isArray(a) && Array.isArray(b)) {
        const t = a.splice(0, a.length, ...b);
        b.splice(0, b.length, ...t);
        return;
      }
      const t = Object.assign(Object.create(Object.getPrototypeOf(a)), a);
      rt.$replace(a, b);
      rt.$replace(b, t);
    },
    $replaceContents(a, value) {
      if (Array.isArray(a)) return a.splice(0, a.length, ...value);
      const old = Object.assign(Object.create(Object.getPrototypeOf(a)), a);
      rt.$replace(a, value);
      return old;
    },
    // A generic type's class with its type arguments' classes (`W::<f64>` as `T`): its consts that
    // read them (`const ZERO: Self = W(T::ZERO);`) read `this.$targs`.
    $tv(cls, targs) {
      const view = Object.create(cls);
      view.$targs = targs;
      return view;
    },
    // A struct's class with its const generic arguments (`Buf::<4>` as `T`, or for
    // `Default::default()`): its associated functions read them as `this.$cargs`.
    $cv(cls, args) {
      const view = Object.create(cls);
      view.$cargs = args;
      return view;
    },
    // A generic type's class passed for a type parameter (`go::<W<A>>()`): its associated
    // functions with the classes they take for the `impl`'s parameters filled in. `fns` maps a
    // function to its parameter count and its class arguments, `null` for the ones the caller
    // passes (the function's own type parameters'); the rest of the caller's arguments follow.
    $sv(cls, fns) {
      const view = Object.create(cls);
      for (const k of Object.keys(fns)) {
        const [n, ds] = fns[k];
        const f = cls[k];
        view[k] = function (...a) {
          const out = a.slice(0, n);
          let j = n;
          for (const d of ds) out.push(d === null ? a[j++] : d);
          return f.apply(this, out.concat(a.slice(j)));
        };
      }
      return view;
    },
    // A trait's default methods, added to an implementing class unless it defines its own.
    // A trait's default methods, unless the class defines its own; also as `Trait$name`, for
    // `Trait::name(&x)` calls.
    $mixin(cls, defaults, tr, statics) {
      const into = (target, from) => {
        for (const k of Object.getOwnPropertyNames(from)) {
          for (const name of tr ? [k, `${tr}$${k}`] : [k]) {
            // (A class's own `name` and `length` aren't its items: those may be replaced. Its
            // own constants (`static N = 5`) override a trait's default ones.)
            const own = Object.getOwnPropertyDescriptor(target, name);
            if (!own || (target === cls && (name === "name" || name === "length"))) {
              Object.defineProperty(target, name, Object.getOwnPropertyDescriptor(from, k));
            }
          }
        }
      };
      into(cls.prototype, defaults);
      // Default functions without `self`: on the class itself.
      if (statics) into(cls, statics);
    },
    // Impls of one trait for other instantiations of a class (`impl Get for W<u16>` next to
    // `impl Get for W<u8>`), whose methods are `name$1`, ..: each map (`{get: "get$1", twice:
    // "twice$1"}`) gives the default methods that impl inherits under its keys, called on a view
    // of `self` whose method names go to that impl's.
    $altMixin(cls, defaults, maps) {
      for (const map of maps) {
        for (const [name, key] of Object.entries(map)) {
          if (Object.prototype.hasOwnProperty.call(cls.prototype, key) || typeof defaults[name] !== "function") continue;
          const f = defaults[name];
          cls.prototype[key] = function (...a) {
            return f.call(rt.$altSelf(this, map), ...a);
          };
        }
      }
    },
    // The class of `W<u16>` passed for a type parameter where `W<u8>` has impls of the same
    // traits: `cls` reading the names in `map` (`T::make`, `T::ZERO`) as that impl's, and
    // `$dcallT` calling its methods.
    $altT(cls, map) {
      return new Proxy(cls, {
        get(t, k, r) {
          if (k === "$altMap") return map;
          return Reflect.get(t, Object.prototype.hasOwnProperty.call(map, k) ? map[k] : k, r);
        },
      });
    },
    $altSelf(x, map) {
      return new Proxy(x, {
        get(t, k) {
          // (`Self::K` in a default: `this.constructor.K`, that impl's.)
          if (k === "constructor") return rt.$altT(t.constructor, map);
          return Reflect.get(t, Object.prototype.hasOwnProperty.call(map, k) ? map[k] : k);
        },
      });
    },
    // Stand-ins for a built-in type's class, passed for `T::default()` and friends.
    $prims: {
      num: { default: () => 0, dbg: String },
      big: { default: () => 0n, dbg: String },
      str: { default: () => "", from_iter: (it) => rt.$iter(it).$collectString(), from_str: (s) => new Res(0, s), dbg: (s) => rt.$dbgStr(s) },
      bool: { default: () => false, from_str: (s) => rt.$parseBool(s), dbg: String },
      char: { default: () => "\0", from_str: (s) => rt.$parseChar(s), dbg: (c) => rt.$dbgChar(c) },
      vec: { default: () => [], from_iter: (it) => Array.from(it), vec: true },
      option: { default: () => null, opt: true },
      refcell: { refcell: true },
      unit: { default: () => undefined },
      // (Collections: `C::default()`, and `collect()` into a `C: FromIterator<T>`.)
      ...Object.fromEntries(
        ["Map", "Set", "Deque", "BTreeMap", "BTreeSet", "IndexMap", "IndexSet", "Heap"].map((k) => {
          const from_iter = (it) => rt.$iter(it)[`$collect${k}`]();
          return [k, { default: () => from_iter([]), from_iter }];
        }),
      ),
    },
    // A fixed-size array's class (`[u8; 4]`): a `Vec`'s, whose `default()` has `n` elements.
    $arrT(n, elem) {
      const d = elem === undefined ? rt.$prims.vec : rt.$withElem(rt.$prims.vec, elem);
      return { ...d, len: n, default: () => Array.from({ length: n }, () => elem?.default?.()) };
    },
    // A `Vec<T>`'s or `Option<T>`'s class with `T`'s (`impl<T> S<Vec<T>>` reads it as `.elem`).
    $withElem(prim, elem) {
      return { ...prim, elem };
    },
    // The built-in class a wrapped one (`$withStatics`) stands for, for identity checks.
    $p(d) {
      return d?.$prim ?? d;
    },
    // A built-in type's class with the associated functions and consts of your trait impls for
    // it (`ext`: `impl Array for [T; 4]`, `impl Kind for String`), for `A::size()`.
    $withStatics(d, ext) {
      const o = { ...d };
      o.$prim = rt.$p(d);
      for (const k of Object.getOwnPropertyNames(ext)) {
        if (k === "prototype" || ((k === "name" || k === "length") && typeof ext[k] !== "function")) continue;
        const desc = Object.getOwnPropertyDescriptor(ext, k);
        if (!desc.get && typeof desc.value === "function") o[k] = ext[k].bind(ext);
        // (Associated consts, `T::SIZE`: read off the impl class, with this class as `this`, so
        // a const of `impl<T> Tr for Vec<T>` reads `T`'s class as `this.elem`.)
        else if (k[0] !== "$")
          Object.defineProperty(o, k, {
            get: () => (desc.get ? desc.get.call(o) : ext[k]),
            enumerable: true,
            configurable: true,
          });
      }
      return o;
    },
    // A reference's (or box's) class passed for a `T` (`g(&x)` for `fn g<T: Tr>(t: T)`), whose
    // trait has an impl for `&T` that does more than `T`'s: `wrap` is `ref`, `refmut` or `box`,
    // `elem` the referent's class. Its associated functions are the `&T` impl's (handed the
    // referent's class where `$wrapReg` says), else the referent's.
    $wrapT(regs, kind, inner) {
      const o = Object.create(inner ?? null);
      o.wrap = kind;
      o.elem = inner;
      for (const { cls, statics } of regs.wraps?.[kind] ?? []) {
        for (const [k, n] of Object.entries(statics)) {
          o[k] = (...a) => {
            const out = a.slice(0, n);
            out.push(inner);
            return cls[k](...out);
          };
        }
      }
      return o;
    },
    $wrapReg(regs, kind, cls, statics) {
      ((regs.wraps ??= {})[kind] ??= []).push({ cls, statics: statics ?? {} });
    },
    // `x.f()` through a reference's class: the `&T` impl's method, handed the referent's class
    // (`(**self).f()` in it goes by that), else the referent's impl.
    $wrapCall(regs, cls, x, name, args) {
      const w = (regs.wraps?.[cls.wrap] ?? []).find((r) => typeof r.cls.prototype[name] === "function");
      if (w) return rt.$callElem(cls.elem, w.cls.prototype[name], x, ...args);
      // (`impl Inv for &f32` next to `impl Inv for f32`: the referent's class's `inv$ref`.)
      const kind = cls.elem?.kind;
      const r = (regs.kinds[Array.isArray(x) ? "array" : typeof x] ?? []).find(
        (r) => typeof r.cls.prototype[`${name}$${cls.wrap}`] === "function" && (kind === undefined || r.cls.name === "$ext$" + kind),
      );
      if (r) return r.cls.prototype[`${name}$${cls.wrap}`].call(x, ...args);
      return rt.$dcallT(regs, cls.elem, x, name, ...args);
    },
    // An owned `String`'s or `Vec<T>`'s class `d` passed for a `T` whose trait also has a `str` /
    // `[T]` impl (one type in JS): `own` makes `$dcallT` take the owned impl's methods (`m$own`),
    // and its associated functions are the owned impl's (`T::tag()`).
    $ownT(d) {
      const o = { ...d, own: true };
      for (const k of Object.keys(d)) if (k.endsWith("$own")) o[k.slice(0, -4)] = d[k];
      return o;
    },
    // The registered impl with an owned twin of `name` (`name$own`) for the string or array `x`.
    $ownImpl(regs, x, name) {
      const kind = Array.isArray(x) ? "array" : typeof x;
      return (regs.kinds[kind] ?? []).find((r) => typeof r.cls.prototype[name + "$own"] === "function") ?? null;
    },
    // A tuple's class: its length, and its elements' classes as `elem` (for a tuple impl's
    // methods, see `$dcallT`).
    $tupT(elem) {
      return { tup: elem.length, elem };
    },
    // With impls of one trait for two tuples of one length (`(u8, u8)`, `(String, String)`):
    // the key of `name` whose impl's element classes fit the elements' classes `elem` when the
    // caller passed them, else the values of the tuple `x` (a whole number reaches an integer
    // impl before a float one, as in `$dcall`).
    $tupAlt(cls, name, x, elem) {
      // (Concrete impls before generic ones, as specialization: `[u8]` before `[T]`.)
      const nulls = (ds) => ds.filter((d) => d == null).length;
      const alts = (cls.$altsV ??= Object.fromEntries(Object.entries(cls.$alts()).map(([n, es]) => [n, es.sort((a, b) => nulls(a[1]) - nulls(b[1]))])))[name];
      if (!alts) return name;
      // (A `Vec`'s element class, against `[T]`'s one parameter.)
      if (elem != null && !Array.isArray(elem)) elem = [elem];
      for (const strict of [true, false]) {
        for (const [k, ds] of alts) {
          if (ds.every((d, i) => (elem ? rt.$classFits(d, elem[i]) : rt.$valueFits(d, x?.[i], strict)))) return k;
        }
      }
      return name;
    },
    // Several impls of one generic trait for one type (`Scale<f64>` and `Scale<Feet>` for
    // `Meters`), whose methods are `name`, `name$1`, ..: `P[name]` becomes a dispatcher to the
    // impl whose parameter classes (`table()`: `[key, [class or null per parameter]]`) the
    // arguments fit, for calls through a type parameter or `dyn` (a whole number reaches an
    // integer impl before a float one, as in `$tupAlt`).
    $altDispatch(P, name, table) {
      const first = P[name];
      let t = null;
      P[name] = function (...a) {
        t ??= table().map(([k, ds, ts, own]) => [k === name ? first : P[k], ds, ts ?? [], own ?? 0]);
        // (Called through a bound: the trait arguments' classes come after the arguments and
        // `own` classes of the method's own type parameters, and pick the impl where the values
        // can't, `Scale<f64>` against `Scale<i32>`.)
        for (const [f, ds, ts, own] of t) {
          const at = ds.length + own;
          if (!ts.length || a.length < at + ts.length) continue;
          const cs = a.slice(at, at + ts.length);
          if (cs.some((c) => c != null) && ts.every((d, i) => rt.$classFits(d, cs[i]))) return f.call(this, ...a);
        }
        for (const strict of [true, false]) {
          for (const [f, ds] of t) if (ds.every((d, i) => rt.$valueFits(d, a[i], strict))) return f.call(this, ...a);
        }
        return (first ?? t[0][0]).call(this, ...a);
      };
    },
    // Whether an impl's element class `d` (`null`: a type parameter) is the class `c` passed for
    // the element (`undefined` when the caller had none).
    $classFits(d, c) {
      if (d == null || c == null || d === c) return true;
      if (typeof d === "function") return Object.prototype.isPrototypeOf.call(d, c);
      if (d.kind !== undefined || c.kind !== undefined) return d.kind === c.kind;
      if (d.tup !== undefined) return c.tup === d.tup && d.elem.every((e, i) => rt.$classFits(e, c.elem[i]));
      if (d.vec || d.opt) return !!d.vec === !!c.vec && !!d.opt === !!c.opt && rt.$classFits(d.elem, c.elem);
      for (const p of ["str", "bool", "char", "unit"]) if (rt.$p(d) === rt.$prims[p] || rt.$p(c) === rt.$prims[p]) return false;
      return true;
    },
    // Whether the element value `v` could be of the impl's element class `d`. `strict`: a whole
    // number isn't a float's.
    $valueFits(d, v, strict) {
      if (d == null) return true;
      if (typeof d === "function") return v instanceof d;
      if (d.kind !== undefined) {
        if (d.big) return typeof v === "bigint";
        return typeof v === "number" && (!strict || d.float !== Number.isInteger(v));
      }
      if (rt.$p(d) === rt.$prims.str || rt.$p(d) === rt.$prims.char) return typeof v === "string";
      if (rt.$p(d) === rt.$prims.bool) return typeof v === "boolean";
      if (d.tup !== undefined) {
        return Array.isArray(v) && v.length === d.tup && d.elem.every((e, i) => rt.$valueFits(e, v[i], strict));
      }
      if (d.vec) return Array.isArray(v);
      return true;
    },
    // A tuple's or `Vec`'s class `cls`: the impl of `name` among the array ones (a tuple's of its
    // length, a `Vec`'s of none), which all JS arrays reach by value alike.
    $arrayImpl(regs, cls, name) {
      if (cls?.tup === undefined && !cls?.vec) return null;
      for (const r of regs.kinds.array ?? []) {
        if (typeof r.cls.prototype[name] === "function" && r.arity === cls.tup) return r.cls;
      }
      return null;
    },
    // `impl<T> Tr for Option<T>`'s methods that use `T`'s class take it on their first line
    // (`$takeElem`) from the caller that knows it (`$callElem`); anyone else leaves none.
    $nextElem: undefined,
    $callElem(elem, f, self, ...args) {
      rt.$nextElem = elem;
      return f.call(self, ...args);
    },
    $takeElem() {
      const e = rt.$nextElem;
      rt.$nextElem = undefined;
      return e;
    },
    // The `{:?}` formatter of a type parameter's class (`this.$T` of an `impl<T>`'s value): a
    // number kind's or primitive's, a struct's or enum's `$dbg`, else by the run-time value.
    $dbgOf(c) {
      if (c == null) return rt.$dbgAny;
      if (typeof c.dbg === "function") return c.dbg;
      if (typeof c === "function" && c.prototype && typeof c.prototype.$dbg === "function") return (v) => v.$dbg();
      return rt.$dbgAny;
    },
    // `x.money()` on a `T: Money` with `impl Money for i32` and `impl Money for f64`: `T`'s
    // class says which impl (a whole `f64` would otherwise reach the integer impl).
    // The same for a `&mut self` method, with the receiver's cell `c` (`$dcallMut`).
    $dcallMutT(regs, cls, c, name, ...args) {
      if (cls?.$altMap && c[0] !== null && typeof c[0] === "object") return rt.$dcallT(regs, cls, c[0], name, ...args);
      if (cls?.wrap) return rt.$wrapCall(regs, cls, c[0], name, args);
      if (cls?.own) {
        const r = rt.$ownImpl(regs, c[0], name);
        if (r) return r.cls.prototype[name + "$own"].call(r.cls.$mut?.has(name + "$own") ? c : c[0], ...args);
      }
      const kind = cls?.kind;
      const x = c[0];
      const arr = rt.$arrayImpl(regs, cls, name);
      if (arr) {
        const f = arr.prototype[arr.$alts ? rt.$tupAlt(arr, name, x, cls.elem) : name];
        return arr.$elem?.has(name) ? rt.$callElem(cls.elem, f, x, ...args) : f.call(x, ...args);
      }
      if (kind !== undefined) {
        for (const r of regs.kinds[typeof x] ?? []) {
          if (r.cls.name === "$ext$" + kind && typeof r.cls.prototype[name] === "function") {
            return r.cls.prototype[name].call(r.cls.$mut?.has(name) ? c : x, ...args);
          }
        }
      }
      return rt.$dcallMut(regs, c, name, ...args);
    },
    $dcallT(regs, cls, x, name, ...args) {
      // (`W<u16>`'s class with impls of its own next to `W<u8>`'s: that impl's method.)
      const alt = cls?.$altMap;
      if (alt && Object.prototype.hasOwnProperty.call(alt, name) && x !== null && typeof x === "object" && typeof x[alt[name]] === "function") {
        return x[alt[name]](...args);
      }
      if (cls?.wrap) return rt.$wrapCall(regs, cls, x, name, args);
      if (cls?.own) {
        const r = rt.$ownImpl(regs, x, name);
        if (r) return r.cls.prototype[name + "$own"].call(x, ...args);
      }
      // (An `Option`'s class: its impl, whatever the value, handed the element's class.)
      if (cls?.opt) {
        for (const r of regs.kinds.option ?? []) {
          const f = r.cls.prototype[name];
          if (typeof f === "function") return r.cls.$elem?.has(name) ? rt.$callElem(cls.elem, f, x, ...args) : f.call(x, ...args);
        }
      }
      // (A `Cell<T>`'s or `RefCell<T>`'s: its impl, handed the element's class.)
      if (cls?.refcell && x instanceof RefCell) {
        for (const r of regs.kinds.refcell ?? []) {
          if (typeof r.cls.prototype[name] !== "function") continue;
          // (Impls for two instantiations, `Cell<u8>` and `Cell<u16>`: by the element's class.)
          const f = r.cls.prototype[r.cls.$alts ? rt.$tupAlt(r.cls, name, x, cls.elem ?? null) : name];
          if (typeof f === "function") return r.cls.$elem?.has(name) ? rt.$callElem(cls.elem, f, x, ...args) : f.call(x, ...args);
        }
      }
      // (A tuple's or `Vec`'s: the impl for it, handed the elements' classes.)
      const arr = rt.$arrayImpl(regs, cls, name);
      if (arr) {
        const f = arr.prototype[arr.$alts ? rt.$tupAlt(arr, name, x, cls.elem) : name];
        return arr.$elem?.has(name) ? rt.$callElem(cls.elem, f, x, ...args) : f.call(x, ...args);
      }
      // (A `HashMap<K, V>`'s or `HashSet<T>`'s: its impl, handed the key's and value's classes
      // or the element's.)
      if (cls?.elem !== undefined && (x instanceof HashMap ? !(x instanceof BTreeMap) : x instanceof HashSet && !(x instanceof BTreeSet))) {
        for (const r of regs.kinds[x instanceof HashMap ? "map" : "set"] ?? []) {
          const f = r.cls.prototype[name];
          if (typeof f === "function") return r.cls.$elem?.has(name) ? rt.$callElem(cls.elem, f, x, ...args) : f.call(x, ...args);
        }
      }
      const kind = cls?.kind;
      if (kind !== undefined) {
        for (const r of regs.kinds[typeof x] ?? []) {
          if (r.cls.name === "$ext$" + kind && typeof r.cls.prototype[name] === "function") {
            return r.cls.prototype[name].call(x, ...args);
          }
        }
      }
      return rt.$dcall(regs, x, name, ...args);
    },
    // A number type as a type parameter's class (`T = u8`): its `default()` and kind, for
    // generic arithmetic.
    // With `ext` (the class of your trait impls for the type: `impl Zero for i32`), also its
    // associated functions, for `T::zero()`.
    $numT(kind, ext) {
      if (ext) {
        // (Per impl class: each script has its own `$ext$u32`.)
        let byKind = NUM_EXT.get(ext);
        if (byKind === undefined) NUM_EXT.set(ext, (byKind = {}));
        let d = byKind[kind];
        if (d === undefined) {
          d = byKind[kind] = Object.create(rt.$numT(kind));
          for (const k of Object.getOwnPropertyNames(ext)) {
            // (A class's own `name` and `length`, unless the impl declares `fn name()`.)
            if (k === "prototype" || ((k === "name" || k === "length") && typeof ext[k] !== "function")) continue;
            const desc = Object.getOwnPropertyDescriptor(ext, k);
            if (!desc.get && typeof desc.value === "function") d[k] = ext[k].bind(ext);
            // (Its associated consts, `T::ZERO`: lazy getters that turn into values.)
            else if (k[0] !== "$") Object.defineProperty(d, k, { get: () => ext[k] });
          }
        }
        return d;
      }
      let t = NUM_TYPES[kind];
      if (t === undefined) {
        const float = kind[0] === "f";
        // (Only the 128-bit kinds are BigInts; `i64` / `u64` are numbers like `usize`.)
        const big = /^[iu]128$/.test(kind);
        const bits = float ? 64 : kind.endsWith("size") ? 64 : Number(kind.slice(1));
        const signed = kind[0] !== "u";
        t = NUM_TYPES[kind] = {
          kind,
          float,
          big,
          bits,
          signed,
          dbg: float ? dbgF : String,
          default: big ? () => 0n : () => 0,
          from_str: float
            ? (s) => rt.$parseFloat(s)
            : big
              ? (s) => rt.$parseInt128(s, signed, bits)
              : (s) => rt.$parseInt(s, signed ? -(2 ** (bits - 1)) : 0, signed ? 2 ** (bits - 1) - 1 : 2 ** bits - 1),
        };
      }
      return t;
    },
    // `&a | &b` (and `&`, `-`, `^`) on two sets: a new set of the same kind, of clones.
    $setOp(op, a, b) {
      const m =
        op === "|" ? "$union" : op === "&" ? "$intersection" : op === "-" ? "$difference" : "$symmetricDifference";
      const s = new a.constructor();
      for (const x of a[m](b)) s.$insert(clone(x));
      return s;
    },
    // `a + b` (and `-`, `*`, `/`, `%`) where `a: T`: a struct's own operator, else arithmetic
    // as `T`'s kind does it (integer division truncates and panics on zero, results wrap).
    $gop(op, a, b, T, loc) {
      if (a !== null && typeof a === "object") return a["$op$" + OP_METHODS[op]](b);
      const k = T?.kind === undefined ? null : T;
      if (op.length === 2 || op === "&" || op === "|" || op === "^") {
        if (typeof a === "boolean") return op === "&" ? a && b : op === "|" ? a || b : a !== b;
        return bitOp(op, a, b, k);
      }
      if (typeof a === "bigint") {
        if ((op === "/" || op === "%") && b === 0n) throw new CarciniPanic(divZero(op), loc);
        const r = op === "+" ? a + b : op === "-" ? a - b : op === "*" ? a * b : op === "/" ? a / b : a % b;
        return k ? rt.$w128(r, k.signed, k.bits) : r;
      }
      if (!k || k.float) {
        const r = op === "+" ? a + b : op === "-" ? a - b : op === "*" ? a * b : op === "/" ? a / b : a % b;
        return k?.kind === "f32" ? Math.fround(r) : r;
      }
      if (op === "/" || op === "%") {
        if (b === 0) throw new CarciniPanic(divZero(op), loc);
        if (k.signed && b === -1 && k.bits <= 32 && a === -(2 ** (k.bits - 1))) {
          throw new CarciniPanic(op === "/" ? "attempt to divide with overflow" : "attempt to calculate the remainder with overflow", loc);
        }
        return op === "/" ? Math.trunc(a / b) : a % b;
      }
      const r = op === "+" ? a + b : op === "-" ? a - b : k.bits <= 32 ? Math.imul(a, b) : a * b;
      return wrapInt(r, k);
    },
    // `s.parse::<T>()` with `T: FromStr`.
    $tparse(T, s) {
      return T.from_str(s);
    },
    // `T::from(x)` with `T: From<X>` (`src` names `X`): a number conversion, or the type's
    // `From<X>` impl.
    $tfrom(T, src, x) {
      if (T.kind !== undefined) {
        const v = typeof x === "string" ? x.codePointAt(0) : x;
        return T.big ? BigInt(v) : Number(v);
      }
      if (rt.$p(T) === rt.$prims.str) return String(x);
      if (rt.$p(T) === rt.$prims.char) return String.fromCodePoint(Number(x));
      const f = T["$from$" + src];
      return f ? f.call(T, x) : T.from(x);
    },
    // `T::try_from(x)` with `T: TryFrom<X>`: an integer kind checks the range; a type's own
    // `TryFrom<X>` impl, else its `From<X>` (std's blanket impl) as `Ok`.
    $ttryfrom(T, src, x) {
      if (T.kind !== undefined) {
        const v = typeof x === "string" ? x.codePointAt(0) : typeof x === "boolean" ? Number(x) : x;
        return T.float ? new Res(0, Number(v)) : rt.$tryInt(v, T.bits, T.signed, T.big);
      }
      if (rt.$p(T) === rt.$prims.char) return rt.$tryChar(x);
      if (rt.$p(T) === rt.$prims.str) return new Res(0, String(x));
      const f = T["$tryfrom$" + src];
      if (f) return f.call(T, x);
      const g = T["$from$" + src];
      return g ? new Res(0, g.call(T, x)) : T.try_from(x);
    },
    // `iter.sum::<T>()` / `product` with `T: Sum`: the type's own `sum`, else by its kind.
    $gsum(it, T, product) {
      const own = product ? T?.product : T?.sum;
      if (T?.kind === undefined && typeof own === "function") return own.call(T, it);
      let acc = T?.big ? (product ? 1n : 0n) : product ? 1 : T?.float ? -0 : 0;
      for (const x of it) acc = rt.$gop(product ? "*" : "+", acc, x, T, "<std>");
      return acc;
    },
    // `a += b` where `a: T`: a struct's `add_assign` changes it in place; numbers get the result.
    $gopAssign(op, a, b, T, loc) {
      if (a !== null && typeof a === "object") {
        a["$op$" + OP_METHODS[op] + "_assign"](b);
        return a;
      }
      return rt.$gop(op, a, b, T, loc);
    },
    // `!a` where `a: T`.
    $gnot(a, T) {
      if (a !== null && typeof a === "object") return a.$op$not();
      if (typeof a === "boolean") return !a;
      if (typeof a === "bigint") return rt.$w128(~a, T.signed, T.bits);
      if (T.bits === 64) return rt.$not64(a, T.signed);
      return wrapInt(~a, T);
    },
    // `-a` where `a: T`.
    $gneg(a, T) {
      if (a !== null && typeof a === "object") return a.$op$neg();
      if (typeof a === "bigint") return T?.kind ? rt.$w128(-a, T.signed, T.bits) : -a;
      if (!T?.kind || T.float) return -a;
      return wrapInt(-a, T);
    },
    // `Ord::max` / `Ord::min`: on ties `max` returns the second argument and `min` the first.
    $max(a, b) {
      return cmp(b, a) >= 0 ? b : a;
    },
    $min(a, b) {
      return cmp(b, a) < 0 ? b : a;
    },
    $plus(s) {
      s = String(s);
      return s.startsWith("-") || s === "NaN" ? s : "+" + s;
    },
    $pad(s, width, align, fill, zero, numeric) {
      s = String(s);
      const len = Array.from(s).length;
      if (len >= width) return s;
      const n = width - len;
      if (zero && numeric) {
        // Zeros go after the sign and any `0x` / `0o` / `0b` prefix.
        const sign = /^[+-]?(0[xob])?/.exec(s)[0];
        return sign + "0".repeat(n) + s.slice(sign.length);
      }
      const a = align ?? (numeric ? ">" : "<");
      if (a === "<") return s + fill.repeat(n);
      if (a === ">") return fill.repeat(n) + s;
      const left = Math.floor(n / 2);
      return fill.repeat(left) + s + fill.repeat(n - left);
    },
  };

  // ---- `js::JSON`: serde_json's format, by a schema of the static type that codegen writes
  // (`{t:"struct", cls, fields: [{key, js, s, def}]}`, ..). Definitions of structs and enums are
  // `{t:"lazy", get}`, so recursive types work.

  /// Why `JSON::parse` failed (`message()`, `{}`), as serde_json words it, without a position.
  class JsonError {
    constructor(msg) {
      this.msg = msg;
    }
    message() {
      return this.msg;
    }
    toString() {
      return this.msg;
    }
    $dbg() {
      return `JsonError(${rt.$dbgStr(this.msg)})`;
    }
  }
  rt.JsonError = JsonError;

  function jsonSchema(s) {
    if (s.t === "lazy") {
      s.v ??= s.get();
      return s.v;
    }
    return s;
  }

  // A float as serde_json writes it (ryu): shortest digits, `1.0` for whole numbers, an
  // exponent past 1e16 (f64) / 1e13 (f32) or below 1e-5 / 1e-6.
  function ryu(x, f32) {
    if (x === 0) return Object.is(x, -0) ? "-0.0" : "0.0";
    let text = x.toExponential();
    if (f32) {
      for (let p = 1; p <= 9; p++) {
        const s = x.toPrecision(p);
        if (Math.fround(Number(s)) === x) {
          text = Number(s).toExponential();
          break;
        }
      }
    }
    const m = /^(-?)(\d)(?:\.(\d+))?e([+-]\d+)$/.exec(text);
    const sign = m[1];
    const digits = m[2] + (m[3] ?? "");
    const kk = Number(m[4]) + 1;
    const len = digits.length;
    const k = kk - len;
    const [maxK, minK] = f32 ? [13, -6] : [16, -5];
    let out;
    if (k >= 0 && kk <= maxK) out = digits + "0".repeat(k) + ".0";
    else if (kk > 0 && kk <= maxK) out = digits.slice(0, kk) + "." + digits.slice(kk);
    else if (kk > minK && kk <= 0) out = "0." + "0".repeat(-kk) + digits;
    else {
      // (serde_json writes `1e+16` / `1e-7`.)
      const e = kk - 1 >= 0 ? `e+${kk - 1}` : `e${kk - 1}`;
      out = len === 1 ? digits + e : `${digits[0]}.${digits.slice(1)}${e}`;
    }
    return sign + out;
  }

  rt.$ryu = (x) => ryu(x, false);

  // ---- `Drop`: a value going out of scope runs its type's `drop`, then its fields' (in
  // declaration order) and its elements'. Each object is dropped once, however it's reached.
  const dropped = new WeakSet();
  // Fields moved out of a struct (`let x = s.inner;`), by struct: dropping it skips them.
  const movedOut = new WeakMap();
  rt.$moveOut = (o, k) => {
    const v = o[k];
    if (v !== null && typeof v === "object") {
      let s = movedOut.get(o);
      if (!s) movedOut.set(o, (s = new Set()));
      s.add(k);
    }
    return v;
  };
  rt.$drop = (v) => {
    // A closure drops the values it owns (`move` captures, see `$own`).
    if (typeof v === "function" && v.$caps && !dropped.has(v)) {
      dropped.add(v);
      for (const x of v.$caps) rt.$drop(x);
      return;
    }
    if (v === null || typeof v !== "object" || dropped.has(v)) return;
    dropped.add(v);
    if (v.constructor?.$dropImpl) v.drop();
    if (Array.isArray(v)) {
      for (const x of v) rt.$drop(x);
    } else if (v instanceof Map) {
      for (const x of v.values()) rt.$drop(x);
    } else if (typeof v.$entries === "function") {
      for (const [k, x] of v.$entries()) {
        rt.$drop(k);
        rt.$drop(x);
      }
    } else if (typeof v[Symbol.iterator] === "function" && !(v instanceof Iter)) {
      for (const x of v) rt.$drop(x);
    } else if (v instanceof rt.RefCell) {
      rt.$drop(v.v);
    } else if (!(v instanceof Iter)) {
      const gone = movedOut.get(v);
      for (const k of Object.keys(v)) if (k !== "$tag" && !gone?.has(k)) rt.$drop(v[k]);
    }
  };
  // `mem::forget(v)`: never dropped (nor what it holds).
  rt.$forget = (v) => {
    if ((v !== null && typeof v === "object") || typeof v === "function") dropped.add(v);
  };
  // `panic::catch_unwind(f)`: `Ok(f())`, or `Err` of the panic's message once the panic is
  // reported (to stderr, as Rust's hook does before unwinding).
  // (`f` may be an `AssertUnwindSafe` of the closure: a tuple struct, the closure at `[0]`.)
  rt.$catchUnwind = (f) => {
    try {
      return rt.Ok(typeof f === "function" ? f() : f[0]());
    } catch (e) {
      if (!(e instanceof CarciniPanic)) throw e;
      rt.$eprintln(`thread '${threadName}' panicked at ${e.location}:\n${e.panicMessage}`);
      return rt.Err(e.panicMessage);
    }
  };
  // The thread a panic report names: the test running, in a test build.
  let threadName = "main";
  // A test build's `main(ns)`: runs the `#[test]` functions in name order and reports like
  // libtest (`cargo test -- --test-threads=1`), to the terminal under Bitburner or to stdout.
  // Each test's output is kept and shown only if it fails. `ns.args` are libtest's: name
  // filters (substrings; whole names with `--exact`), `--ignored` and `--include-ignored`.
  // Each test is `{ name, f, at, shouldPanic, ignore, dbg }`: `at` is where the test is
  // (`file:line:col`), `shouldPanic` is `true` or the expected substring, `ignore` is `true` or
  // the reason, and `dbg` formats the `Err` of a test that returns a `Result`.
  rt.$runTests = async (ns, tests) => {
    const say = typeof ns?.tprintf === "function" ? (s) => ns.tprintf("%s", s) : (s) => rt.$println(s);
    const args = (ns?.args ?? []).map(String);
    const filters = args.filter((a) => !a.startsWith("--"));
    const exact = args.includes("--exact");
    const onlyIgnored = args.includes("--ignored");
    const withIgnored = args.includes("--include-ignored");
    const picked = tests
      .filter((t) => !filters.length || filters.some((f) => (exact ? t.name === f : t.name.includes(f))))
      .filter((t) => !onlyIgnored || t.ignore)
      .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    const start = Date.now();
    let [passed, ignored] = [0, 0];
    const failures = [];
    // (Rust's panic hook adds the backtrace note to the first panic only.)
    let noted = false;
    say("");
    say(`running ${picked.length} test${picked.length === 1 ? "" : "s"}`);
    for (const t of picked) {
      const head = `test ${t.name}${t.shouldPanic ? " - should panic" : ""} ...`;
      if (t.ignore && !onlyIgnored && !withIgnored) {
        ignored++;
        say(`${head} ignored${t.ignore === true ? "" : `, ${t.ignore}`}`);
        continue;
      }
      // The test's output (stdout and stderr, as libtest captures them).
      let out = "";
      const sinks = [outSink, errSink];
      rt.$flush();
      outSink = errSink = (line) => {
        out += line + "\n";
      };
      threadName = t.name;
      let ok;
      try {
        try {
          const r = await t.f();
          if (r != null && r.$tag === 1 && t.dbg) {
            rt.$eprintln(`Error: ${t.dbg(r[0])}`);
            ok = false;
          } else if (t.shouldPanic) {
            rt.$print(`note: test did not panic as expected at ${t.at}`);
            ok = false;
          } else ok = true;
        } catch (e) {
          if (e?.name === "CarciniExit") throw e;
          const message = e instanceof CarciniPanic ? e.panicMessage : String(e?.stack ?? e);
          const location = e instanceof CarciniPanic ? e.location : "<js>";
          rt.$eprintln(`\nthread '${t.name}' panicked at ${location}:\n${message}`);
          if (!noted) rt.$eprintln("note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace");
          noted = true;
          if (typeof t.shouldPanic === "string" && !message.includes(t.shouldPanic)) {
            rt.$print(
              "note: panic did not contain expected string\n" +
                `      panic message: ${rt.$dbgStr(message)}\n` +
                ` expected substring: ${rt.$dbgStr(t.shouldPanic)}`,
            );
            ok = false;
          } else ok = !!t.shouldPanic;
        }
      } finally {
        // (A line the test left unfinished is part of its output.)
        out += outBuf + errBuf;
        outBuf = errBuf = "";
        [outSink, errSink] = sinks;
        threadName = "main";
      }
      say(`${head} ${ok ? "ok" : "FAILED"}`);
      if (ok) passed++;
      else failures.push([t.name, out]);
    }
    if (failures.length) {
      say("");
      say("failures:");
      const shown = failures.filter(([, out]) => out);
      if (shown.length) say("");
      for (const [name, out] of shown) {
        say(`---- ${name} stdout ----`);
        for (const line of out.split("\n")) say(line);
      }
      say("");
      say("failures:");
      for (const [name] of failures) say(`    ${name}`);
    }
    const secs = ((Date.now() - start) / 1000).toFixed(2);
    say("");
    say(
      `test result: ${failures.length ? "FAILED" : "ok"}. ${passed} passed; ${failures.length} failed; ` +
        `${ignored} ignored; 0 measured; ${tests.length - picked.length} filtered out; finished in ${secs}s`,
    );
    say("");
    return !failures.length;
  };
  // A closure that owns captured values with a `Drop` impl: they drop when it does.
  rt.$own = (f, caps) => {
    f.$caps = caps;
    return f;
  };
  // An `async move` block that owns captured values with a `Drop` impl: they drop when it
  // finishes (before whoever awaits it goes on).
  rt.$ownAsync = (p, caps) => {
    const done = () => {
      for (const x of caps) rt.$drop(x);
    };
    p.then(done, done);
    return p;
  };
  // The end of a block: its values, last declared first.
  rt.$dropAll = (list) => {
    for (let i = list.length - 1; i >= 0; i--) list[i]();
  };
  // A `Cursor`'s data as bytes: a string's UTF-8 (kept for the next read), or the array itself.
  let ioBytesOf = null;
  let ioBytes = null;
  rt.$ioBytes = (x) => {
    if (typeof x !== "string") return x;
    if (ioBytesOf !== x) {
      ioBytes = rt.$utf8(x);
      ioBytesOf = x;
    }
    return ioBytes;
  };
  // `Cow::to_mut`: a borrowed cow (tag 0) becomes owned (tag 1) by a copy, in place.
  rt.$cowToMut = (c) => {
    if (c.$tag === 0) {
      c.$tag = 1;
      c[0] = rt.$clone(c[0]);
    }
    return c[0];
  };

  function jsonOut(v, s, pretty, ind) {
    s = jsonSchema(s);
    const inner = ind + "  ";
    const arr = (items) => {
      if (items.length === 0) return "[]";
      return pretty ? `[\n${items.map((i) => inner + i).join(",\n")}\n${ind}]` : `[${items.join(",")}]`;
    };
    const obj = (pairs) => {
      if (pairs.length === 0) return "{}";
      const sep = pretty ? ": " : ":";
      const items = pairs.map(([k, x]) => JSON.stringify(k) + sep + x);
      return pretty ? `{\n${items.map((i) => inner + i).join(",\n")}\n${ind}}` : `{${items.join(",")}}`;
    };
    const sub = (x, ss) => jsonOut(x, ss, pretty, inner);
    switch (s.t) {
      case "int":
        return String(v);
      case "big":
        return v.toString();
      case "float":
        return Number.isFinite(v) ? ryu(v, s.f32) : "null";
      case "bool":
        return v ? "true" : "false";
      case "str":
      case "char":
        return JSON.stringify(v);
      case "unit":
        return "null";
      case "opt":
        return v == null ? "null" : jsonOut(unsome(v), s.s, pretty, ind);
      case "vec":
      case "set":
        return arr(Array.from(v, (x) => sub(x, s.s)));
      case "tuple":
        return arr(s.s.map((e, i) => sub(v[i], e)));
      case "map":
        return obj(Array.from(v.$entries(), ([k, x]) => [String(k), sub(x, s.v)]));
      case "cell":
        return jsonOut(v.v, s.s, pretty, ind);
      case "jsonvalue":
        return s.out(v, pretty, ind);
      case "result":
        return obj([[v.$tag === 0 ? "Ok" : "Err", sub(v[0], v.$tag === 0 ? s.ok : s.err)]]);
      case "struct":
        // `#[serde(tag = "..")]` on a struct: its name as a field before its own.
        if (s.tag !== undefined) return obj(jsonPairs(v, s, sub));
        return jsonFields(v, s.shape, s.fields, pretty, ind, obj, arr, sub);
      case "enum": {
        const variant = s.variants[v.$tag];
        // `#[serde(untagged)]`: the content alone (`null` for a unit variant).
        if (s.untagged) return jsonFields(v, variant.shape, variant.fields, pretty, ind, obj, arr, sub);
        // `#[serde(tag = "..")]`: an object with the variant's name as a field.
        if (s.tag !== undefined) return obj(jsonPairs(v, s, sub));
        if (variant.shape === "unit") return JSON.stringify(variant.key);
        return obj([[variant.key, sub(v, variantSchema(variant))]]);
      }
      default:
        return JSON.stringify(v) ?? "null";
    }
  }

  // A variant's content as a struct schema (the variant object has the same properties).
  const variantSchema = (variant) => ({ t: "struct", shape: variant.shape, fields: variant.fields });

  // The `key: text` pairs of a value written as a JSON object (`sub` writes a nested value), or
  // `null` for a value that isn't one: a struct's fields, a map's entries, a tagged enum's tag
  // and content, a `#[serde(flatten)]` field's pairs in place of its own key.
  function jsonPairs(v, s, sub) {
    s = jsonSchema(s);
    const tag = (key) => [s.tag, JSON.stringify(key)];
    switch (s.t) {
      case "struct": {
        if (s.shape !== "named") return null;
        const pairs = fieldPairs(v, s.fields, sub);
        return s.tag === undefined ? pairs : [tag(s.name), ...pairs];
      }
      case "map":
        return Array.from(v.$entries(), ([k, x]) => [String(k), sub(x, s.v)]);
      case "opt":
        return v == null ? [] : jsonPairs(unsome(v), s.s, sub);
      case "cell":
        return jsonPairs(v.v, s.s, sub);
      case "enum": {
        const variant = s.variants[v.$tag];
        const content = () => {
          if (variant.shape === "named") return fieldPairs(v, variant.fields, sub);
          if (variant.shape === "unit") return [];
          return variant.fields.length === 1 ? jsonPairs(v[0], variant.fields[0].s, sub) : null;
        };
        if (s.untagged) return content();
        if (s.tag === undefined) return variant.shape === "unit" ? null : [[variant.key, sub(v, variantSchema(variant))]];
        // Adjacently tagged (`content = ".."`): the content under its own key.
        if (s.content !== undefined) {
          return variant.shape === "unit" ? [tag(variant.key)] : [tag(variant.key), [s.content, sub(v, variantSchema(variant))]];
        }
        // Internally tagged: the content's own pairs after the tag (a newtype variant's struct).
        const c = content();
        return c && [tag(variant.key), ...c];
      }
      default:
        return null;
    }
  }

  function fieldPairs(v, fields, sub) {
    const out = [];
    for (const f of fields) {
      if (f.skipOut) continue;
      if (f.flatten) {
        const pairs = jsonPairs(v[f.js], f.s, sub);
        if (pairs === null) throw new JsonError("can only flatten structs and maps");
        out.push(...pairs);
      } else out.push([f.key, sub(v[f.js], f.s)]);
    }
    return out;
  }

  // A struct's (or variant's) fields: an object, an array (tuple), the value itself (one
  // unnamed field), or `null` (unit).
  function jsonFields(v, shape, fields, pretty, ind, obj, arr, sub) {
    if (shape === "unit") return "null";
    if (shape === "tuple") {
      if (fields.length === 1) return jsonOut(v[0], fields[0].s, pretty, ind);
      return arr(fields.map((f, i) => sub(v[i], f.s)));
    }
    return obj(fieldPairs(v, fields, sub));
  }

  rt.$jsonStringify = (v, schema, pretty) => jsonOut(v, schema, pretty, "");
  // `serde_wasm_bindgen::from_value::<T>(v)` / `to_value(&x)`: through JSON, by `T`'s schema.
  rt.$swbFrom = (v, schema) => rt.$jsonParse(v === undefined ? "null" : JSON.stringify(v), schema);
  rt.$swbTo = (v, schema) => rt.Ok(JSON.parse(jsonOut(v, schema, false, "")));

  // ---- the `js` crate's other helpers
  // An array-like value (`NodeList`, `HTMLCollection`) as an array; null as empty.
  rt.$arrayOf = (x) => (x == null ? [] : Array.from(x));
  // `Url::parse`: the URL, or null when it doesn't parse.
  rt.$urlParse = (text, base) => {
    try {
      return new URL(text, base);
    } catch {
      return null;
    }
  };
  // The bundled `rand` crate, as rand 0.8 computes it. A seeded generator gives rand's own
  // numbers: `StdRng` (and `ChaCha12Rng`) is ChaCha12, `ChaCha8Rng` / `ChaCha20Rng` ChaCha with
  // 8 / 20 rounds, `SmallRng` Xoshiro256++, each seeded by `seed_from_u64` / `from_seed` as rand
  // does. `thread_rng()` (one shared generator seeded from `Math.random`) and `from_entropy()`
  // are xoshiro128**, which is fast in JS. Sampling (`gen`, `gen_range`, `gen_bool`, `shuffle`,
  // `choose`, ..) follows rand's algorithms word for word, by the type's class
  // (`κσ.$numT("u8")`). A `u64` is a JS number: rand's 64 bits, rounded past 2^53.
  class RandRng {
    // `next_u64()` as `[lo, hi]` 32-bit halves.
    u64() {
      const lo = this.u32();
      return [lo, this.u32()];
    }
    // `fill_bytes`: the words' bytes, little-endian; a partly used word is dropped.
    bytes(n) {
      const out = [];
      while (out.length < n) {
        const w = this.u32();
        for (let b = 0; b < 4 && out.length < n; b++) out.push((w >>> (8 * b)) & 255);
      }
      return out;
    }
    // `gen::<f64>()`: 53 bits of `next_u64()`, in [0, 1).
    f64() {
      const [lo, hi] = this.u64();
      return (hi * 2097152 + (lo >>> 11)) / 9007199254740992;
    }
    // `next_u64()` as a BigInt.
    bigU64() {
      const [lo, hi] = this.u64();
      return (BigInt(hi) << 32n) | BigInt(lo);
    }
  }
  // xoshiro128**: `thread_rng()` and `from_entropy()`.
  class XoshiroRng extends RandRng {
    constructor(seed) {
      super();
      // splitmix32 spreads the seed over the four words of state.
      let x = seed >>> 0;
      const next = () => {
        x = (x + 0x9e3779b9) >>> 0;
        let z = x;
        z = Math.imul(z ^ (z >>> 16), 0x85ebca6b);
        z = Math.imul(z ^ (z >>> 13), 0xc2b2ae35);
        return (z ^ (z >>> 16)) >>> 0;
      };
      this.s = [next(), next(), next(), next()];
      if (this.s.every((w) => w === 0)) this.s[0] = 1;
    }
    u32() {
      const s = this.s;
      const r = Math.imul(rotl(Math.imul(s[1], 5), 7), 9) >>> 0;
      const t = s[1] << 9;
      s[2] ^= s[0];
      s[3] ^= s[1];
      s[1] ^= s[2];
      s[0] ^= s[3];
      s[2] ^= t;
      s[3] = rotl(s[3], 11);
      return r;
    }
  }
  // rand_chacha's `ChaChaXRng`: the key is the seed's eight little-endian words, the block
  // counter is 64 bits from 0 and the stream is 0; the words come out block by block.
  class ChaChaRng extends RandRng {
    constructor(key, rounds) {
      super();
      this.key = key;
      this.rounds = rounds;
      this.ctr = 0;
      this.buf = new Uint32Array(16);
      this.i = 16;
    }
    block() {
      const k = this.key;
      const input = [
        0x61707865, 0x3320646e, 0x79622d32, 0x6b206574,
        k[0], k[1], k[2], k[3], k[4], k[5], k[6], k[7],
        this.ctr >>> 0, Math.floor(this.ctr / 4294967296) >>> 0, 0, 0,
      ];
      const x = input.slice();
      const qr = (a, b, c, d) => {
        x[a] = (x[a] + x[b]) | 0;
        x[d] = rotl(x[d] ^ x[a], 16);
        x[c] = (x[c] + x[d]) | 0;
        x[b] = rotl(x[b] ^ x[c], 12);
        x[a] = (x[a] + x[b]) | 0;
        x[d] = rotl(x[d] ^ x[a], 8);
        x[c] = (x[c] + x[d]) | 0;
        x[b] = rotl(x[b] ^ x[c], 7);
      };
      for (let r = 0; r < this.rounds; r += 2) {
        qr(0, 4, 8, 12);
        qr(1, 5, 9, 13);
        qr(2, 6, 10, 14);
        qr(3, 7, 11, 15);
        qr(0, 5, 10, 15);
        qr(1, 6, 11, 12);
        qr(2, 7, 8, 13);
        qr(3, 4, 9, 14);
      }
      for (let j = 0; j < 16; j++) this.buf[j] = (x[j] + input[j]) >>> 0;
      this.ctr++;
      this.i = 0;
    }
    u32() {
      if (this.i >= 16) this.block();
      return this.buf[this.i++];
    }
  }
  // rand 0.8's `SmallRng` on a 64-bit target: Xoshiro256++, in 32-bit halves (`[lo, hi]`).
  class Xoshiro256Rng extends RandRng {
    constructor(words) {
      super();
      // (Four u64s, each as `[lo, hi]`.)
      this.s = words;
    }
    u64() {
      const [s0, s1, s2, s3] = this.s;
      const r = add64(rotl64(add64(s0, s3), 23), s0);
      const t = shl64(s1, 17);
      xor64(s2, s0);
      xor64(s3, s1);
      xor64(s1, s2);
      xor64(s0, s3);
      xor64(s2, t);
      this.s[3] = rotl64(s3, 45);
      return r;
    }
    // (The upper half: the low bits are weaker.)
    u32() {
      return this.u64()[1];
    }
    // `fill_bytes`: whole `next_u64()`s, then the rest from one more `next_u64` (5..7 bytes)
    // or `next_u32` (1..4).
    bytes(n) {
      const out = [];
      const push = (w, k) => {
        for (let b = 0; b < k; b++) out.push((w >>> (8 * b)) & 255);
      };
      while (n - out.length >= 8) {
        const [lo, hi] = this.u64();
        push(lo, 4);
        push(hi, 4);
      }
      const left = n - out.length;
      if (left > 4) {
        const [lo, hi] = this.u64();
        push(lo, 4);
        push(hi, left - 4);
      } else if (left > 0) {
        push(this.u32(), left);
      }
      return out;
    }
  }
  function add64(a, b) {
    const lo = (a[0] + b[0]) >>> 0;
    const carry = lo < a[0] >>> 0 ? 1 : 0;
    return [lo, (a[1] + b[1] + carry) >>> 0];
  }
  function xor64(a, b) {
    a[0] = (a[0] ^ b[0]) >>> 0;
    a[1] = (a[1] ^ b[1]) >>> 0;
  }
  function shl64(a, k) {
    return k < 32 ? [(a[0] << k) >>> 0, ((a[1] << k) | (a[0] >>> (32 - k))) >>> 0] : [0, (a[0] << (k - 32)) >>> 0];
  }
  function rotl64(a, k) {
    if (k >= 32) {
      a = [a[1], a[0]];
      k -= 32;
    }
    if (k === 0) return [a[0], a[1]];
    return [((a[0] << k) | (a[1] >>> (32 - k))) >>> 0, ((a[1] << k) | (a[0] >>> (32 - k))) >>> 0];
  }
  function rotl(x, k) {
    return (x << k) | (x >>> (32 - k));
  }
  const M64 = (1n << 64n) - 1n;
  // A `u64` seed (a number or a BigInt) as a BigInt.
  function seedU64(n) {
    return BigInt.asUintN(64, typeof n === "bigint" ? n : BigInt(Math.trunc(n)));
  }
  // `SeedableRng::seed_from_u64` for the ChaCha generators: PCG32 fills the 32-byte seed.
  function pcgSeed(n) {
    let state = seedU64(n);
    const words = [];
    for (let i = 0; i < 8; i++) {
      state = (state * 6364136223846793005n + 11634580027462260723n) & M64;
      const xs = Number((((state >> 18n) ^ state) >> 27n) & 0xffffffffn);
      const rot = Number(state >> 59n);
      words.push(((xs >>> rot) | (xs << ((32 - rot) & 31))) >>> 0);
    }
    return words;
  }
  // `Xoshiro256PlusPlus::seed_from_u64`: SplitMix64 fills the seed (for an all-zero `from_seed`).
  function splitMixSeed(n) {
    let state = seedU64(n);
    const words = [];
    for (let i = 0; i < 4; i++) {
      state = (state + 0x9e3779b97f4a7c15n) & M64;
      let z = state;
      z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & M64;
      z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & M64;
      z ^= z >> 31n;
      words.push([Number(z & 0xffffffffn), Number(z >> 32n)]);
    }
    return words;
  }
  const leWords = (bytes) => Array.from({ length: bytes.length >> 2 }, (_, i) => (bytes[4 * i] | (bytes[4 * i + 1] << 8) | (bytes[4 * i + 2] << 16) | (bytes[4 * i + 3] << 24)) >>> 0);
  function makeRng(kind, words) {
    if (kind === "small") {
      const s = [0, 1, 2, 3].map((i) => [words[2 * i], words[2 * i + 1]]);
      // (An all-zero seed is `seed_from_u64(0)`'s.)
      return new Xoshiro256Rng(words.every((w) => w === 0) ? splitMixSeed(0) : s);
    }
    return new ChaChaRng(words, kind === "chacha8" ? 8 : kind === "chacha20" ? 20 : 12);
  }
  let threadRng = null;
  // `(hi, lo)` of the 64-bit product of two `u32`s, exactly.
  function wmul32(v, r) {
    const hi = Math.floor(((v >>> 16) * r + Math.floor(((v & 0xffff) * r) / 65536)) / 65536);
    return [hi, Math.imul(v, r) >>> 0];
  }
  // What `rng.gen::<u32 / u64 / u128>()` of an integer type's "large" unsigned kind gives:
  // rand samples 8- to 32-bit integers through `u32`, 64-bit ones through `u64` (BigInt here)
  // and 128-bit ones through `u128`.
  function largeOf(T) {
    return T.big ? 128 : T.bits === 64 ? 64 : 32;
  }
  function genLarge(rng, large) {
    if (large === 32) return rng.u32();
    if (large === 64) return rng.bigU64();
    const x = rng.bigU64();
    return (rng.bigU64() << 64n) | x;
  }
  // `UniformInt::sample_single_inclusive(low, high)`: widening multiply with rejection (the
  // zone a fast approximation, except for 8- and 16-bit types).
  function intInclusive(rng, lo, hi, T) {
    const large = largeOf(T);
    const bits = T.bits;
    if (large === 32) {
      const range = (hi - lo + 1) % 2 ** bits;
      if (range === 0) return rand.gen(rng, T);
      const zone = bits <= 16 ? 4294967295 - ((4294967296 - range) % range) : ((range << Math.clz32(range)) >>> 0) - 1;
      for (;;) {
        const [h, l] = wmul32(rng.u32(), range);
        if (l <= zone) return lo + h;
      }
    }
    const L = BigInt(large);
    const mask = (1n << L) - 1n;
    const blo = BigInt(lo);
    const range = (BigInt(hi) - blo + 1n) & ((1n << BigInt(bits)) - 1n);
    if (range === 0n) return rand.gen(rng, T);
    const lz = L - BigInt(range.toString(2).length);
    const zone = ((range << lz) & mask) - 1n;
    for (;;) {
      const p = genLarge(rng, large) * range;
      if ((p & mask) <= zone) {
        const v = blo + (p >> L);
        return T.big ? rt.$w128(v, T.signed, T.bits) : Number(v);
      }
    }
  }
  // `Uniform::new_inclusive(low, high).sample(rng)` of an integer type: the zone kept exactly.
  function intUniform(rng, lo, hi, T) {
    const large = largeOf(T);
    if (large === 32) {
      const range = (hi - lo + 1) % 2 ** T.bits;
      if (range === 0) return rand.gen(rng, T);
      const zone = 4294967295 - ((4294967296 - range) % range);
      for (;;) {
        const [h, l] = wmul32(rng.u32(), range);
        if (l <= zone) return lo + h;
      }
    }
    const L = BigInt(large);
    const mask = (1n << L) - 1n;
    const blo = BigInt(lo);
    const range = (BigInt(hi) - blo + 1n) & ((1n << BigInt(T.bits)) - 1n);
    if (range === 0n) return rand.gen(rng, T);
    const zone = mask - (((mask + 1n) - range) % range);
    for (;;) {
      const p = genLarge(rng, large) * range;
      if ((p & mask) <= zone) {
        const v = blo + (p >> L);
        return T.big ? rt.$w128(v, T.signed, T.bits) : Number(v);
      }
    }
  }
  const U32T = { kind: "u32", bits: 32, signed: false, big: false, float: false };
  // rand's `gen_index`: a `u32` `gen_range(0..n)` (a `usize` one past `u32::MAX`).
  function genIndex(rng, n) {
    return n <= 4294967295 ? intInclusive(rng, 0, n - 1, U32T) : intInclusive(rng, 0, n - 1, rt.$numT("usize"));
  }
  // A float in [0, 1) from the top 52 (`f64`) / 23 (`f32`) bits of a word: `value1_2 - 1.0`.
  function unitFloat(rng, f32) {
    if (f32) return (rng.u32() >>> 9) / 8388608;
    const [lo, hi] = rng.u64();
    return (hi * 1048576 + (lo >>> 12)) / 4503599627370496;
  }
  // The float one ulp below `x` (rand's `decrease_masked`).
  function floatDown(x, f32) {
    const dv = new DataView(new ArrayBuffer(8));
    if (f32) {
      dv.setFloat32(0, x);
      dv.setUint32(0, dv.getUint32(0) - 1);
      return dv.getFloat32(0);
    }
    dv.setFloat64(0, x);
    dv.setBigUint64(0, dv.getBigUint64(0) - 1n);
    return dv.getFloat64(0);
  }
  // `UniformFloat::sample_single(low, high)` / `new_inclusive(low, high).sample(rng)`.
  function floatRange(rng, lo, hi, inclusive, f32) {
    const r = f32 ? Math.fround : (x) => x;
    if (inclusive) {
      if (!(lo <= hi)) throw new CarciniPanic("Uniform::new_inclusive called with `low > high`", "<rand>");
      const maxRand = f32 ? 1 - 2 ** -23 : 1 - 2 ** -52;
      let scale = r(r(hi - lo) / maxRand);
      if (!Number.isFinite(scale)) throw new CarciniPanic("Uniform::new_inclusive: range overflow", "<rand>");
      while (r(r(scale * maxRand) + lo) > hi) scale = floatDown(scale, f32);
      return r(r(unitFloat(rng, f32) * scale) + lo);
    }
    if (!(lo < hi)) throw new CarciniPanic("UniformSampler::sample_single: low >= high", "<rand>");
    let scale = r(hi - lo);
    if (!Number.isFinite(scale)) throw new CarciniPanic("UniformSampler::sample_single: range overflow", "<rand>");
    for (;;) {
      const res = r(r(unitFloat(rng, f32) * scale) + lo);
      if (res < hi) return res;
    }
  }
  // `Bernoulli { p_int }.sample(rng)`: `u64::MAX` is always true.
  function bernoulli(rng, pInt) {
    return pInt === null || rng.bigU64() < pInt;
  }
  const SCALE64 = 2 ** 64;
  function bernoulliInt(p) {
    if (p === 1) return null;
    const x = p * SCALE64;
    return x >= SCALE64 ? M64 : BigInt(Math.trunc(x));
  }
  // `rand::seq::index::sample(rng, length, amount)`: the indices `choose_multiple` picks.
  function indexSample(rng, length, amount) {
    if (amount > length) throw new CarciniPanic("`amount` of samples must be less than or equal to `length`", "<rand>");
    const f = Math.fround;
    const j = length < 500000 ? 0 : 1;
    let inplace;
    if (length > 4294967295) inplace = null;
    else if (amount < 163) {
      const C = [
        [1.6, f(8 / 45)],
        [10, f(70 / 9)],
      ];
      const m4 = f(f(C[0][j]) * amount);
      inplace = amount > 11 && f(length) < f(f(f(C[1][j]) + m4) * amount);
      if (!inplace) {
        // Floyd's algorithm (shuffled by insertion when `amount < 50`).
        const shuffled = amount < 50;
        const idx = [];
        for (let k = length - amount; k < length; k++) {
          const t = intInclusive(rng, 0, k, U32T);
          const pos = idx.indexOf(t);
          if (pos >= 0) {
            if (shuffled) idx.splice(pos, 0, k);
            else idx.push(k);
            continue;
          }
          idx.push(t);
        }
        if (!shuffled) {
          for (let i = amount - 1; i > 0; i--) {
            const k = intInclusive(rng, 0, i, U32T);
            [idx[i], idx[k]] = [idx[k], idx[i]];
          }
        }
        return idx;
      }
    } else {
      const C = [270, f(330 / 9)];
      inplace = f(length) < f(f(C[j]) * amount);
    }
    if (inplace) {
      const idx = Array.from({ length }, (_, i) => i);
      for (let i = 0; i < amount; i++) {
        const k = intInclusive(rng, i, length - 1, U32T);
        [idx[i], idx[k]] = [idx[k], idx[i]];
      }
      return idx.slice(0, amount);
    }
    // Rejection sampling with a set of the indices seen.
    const T = length > 4294967295 ? rt.$numT("usize") : U32T;
    const seen = new Set();
    const idx = [];
    for (let i = 0; i < amount; i++) {
      let pos = intUniform(rng, 0, length - 1, T);
      while (seen.has(pos)) pos = intUniform(rng, 0, length - 1, T);
      seen.add(pos);
      idx.push(pos);
    }
    return idx;
  }
  const rand = {
    thread() {
      if (threadRng === null) threadRng = new XoshiroRng(Math.floor(Math.random() * 4294967296));
      return threadRng;
    },
    // `StdRng::seed_from_u64(n)` (`kind`: "std", "chacha8", "chacha20" or "small"): rand_core's
    // PCG32 fill of the seed for each (`SmallRng` doesn't forward to Xoshiro's own).
    seeded(n, kind = "std") {
      return makeRng(kind, pcgSeed(n));
    },
    // `StdRng::from_seed(bytes)`.
    fromSeed(bytes, kind = "std") {
      return makeRng(kind, leWords(bytes));
    },
    entropy() {
      return new XoshiroRng(Math.floor(Math.random() * 4294967296));
    },
    // The other generators' `seed_from_u64` / `from_seed`.
    seededSmall: (n) => rand.seeded(n, "small"),
    fromSeedSmall: (b) => rand.fromSeed(b, "small"),
    seededChaCha8: (n) => rand.seeded(n, "chacha8"),
    fromSeedChaCha8: (b) => rand.fromSeed(b, "chacha8"),
    seededChaCha20: (n) => rand.seeded(n, "chacha20"),
    fromSeedChaCha20: (b) => rand.fromSeed(b, "chacha20"),
    // `rng.gen::<T>()`: integers over their whole range, floats in [0, 1), bools.
    gen(rng, T) {
      if (rt.$p(T) === rt.$prims.bool) return (rng.u32() | 0) < 0;
      if (rt.$p(T) === rt.$prims.char) {
        let n = intUniform(rng, 0x800, 0x10ffff, U32T);
        if (n <= 0xdfff) n -= 0x800;
        return String.fromCodePoint(n);
      }
      if (T?.kind === undefined || T.float) {
        if (T?.kind === "f32") return (rng.u32() >>> 8) / 16777216;
        return rng.f64();
      }
      if (T.big) return rt.$w128(genLarge(rng, 128), T.signed, T.bits);
      if (T.bits === 64) {
        const [lo, hi] = rng.u64();
        return (T.signed ? hi | 0 : hi) * 4294967296 + lo;
      }
      return wrapInt(rng.u32(), T);
    },
    // `rng.gen_range(lo..hi)` / `lo..=hi`.
    range(rng, lo, hi, inclusive, T) {
      if (inclusive ? !(lo <= hi) : !(lo < hi)) throw new CarciniPanic("cannot sample empty range", "<rand>");
      if (T?.float || (T?.kind === undefined && (!Number.isInteger(lo) || !Number.isInteger(hi)))) {
        return floatRange(rng, lo, hi, inclusive, T?.kind === "f32");
      }
      T = T?.kind === undefined ? rt.$numT("i32") : T;
      return intInclusive(rng, lo, inclusive ? hi : hi - (typeof hi === "bigint" ? 1n : 1), T);
    },
    range2(rng, lo, hi, T) {
      return rand.range(rng, lo, hi, false, T);
    },
    // `rng.fill(&mut buf)`: `fill_bytes` over the buffer's bytes, little-endian per element.
    fill(rng, buf, T) {
      if (T?.kind === undefined || T.float || rt.$p(T) === rt.$prims.bool) {
        for (let i = 0; i < buf.length; i++) buf[i] = rand.gen(rng, T);
        return new Res(0, undefined);
      }
      const size = T.big ? 16 : T.bits / 8;
      const bytes = rng.bytes(buf.length * size);
      for (let i = 0; i < buf.length; i++) {
        let v = 0n;
        for (let b = size - 1; b >= 0; b--) v = (v << 8n) | BigInt(bytes[i * size + b]);
        buf[i] = T.big ? rt.$w128(v, T.signed, T.bits) : Number(T.signed ? BigInt.asIntN(size * 8, v) : v);
      }
      return new Res(0, undefined);
    },
    bool(rng, p) {
      if (!(p >= 0 && p <= 1)) throw new CarciniPanic(`p=${rt.$dbgF(p)} is outside range [0.0, 1.0]`, "<rand>");
      return bernoulli(rng, bernoulliInt(p));
    },
    ratio(rng, num, den) {
      if (den === 0 || num > den) throw new CarciniPanic(`p=${num}/${den} is outside range [0, 1]`, "<rand>");
      return bernoulli(rng, num === den ? null : bernoulliInt(num / den));
    },
    // `v.shuffle(&mut rng)`: Fisher-Yates from the end, in place.
    shuffle(v, rng) {
      for (let i = v.length - 1; i > 0; i--) {
        const j = genIndex(rng, i + 1);
        const t = v[i];
        v[i] = v[j];
        v[j] = t;
      }
    },
    choose(v, rng) {
      return v.length === 0 ? null : v[genIndex(rng, v.length)];
    },
    // `v.choose_multiple(&mut rng, n)`: `n` distinct elements (all of them if fewer), as an
    // iterator.
    chooseMultiple(v, rng, n) {
      const idx = indexSample(rng, v.length, Math.min(n, v.length));
      return rt.$iter(idx.map((i) => v[i]));
    },
    // `iter.choose(&mut rng)`, following rand's use of the iterator's `size_hint`: `mode` is
    // "exact" (a Vec's or a range's iterator: one index), "chars" (`s.chars()`: at least a
    // quarter of the bytes left) or neither (a `filter`ed one: reservoir sampling).
    iterChoose(it, rng, mode) {
      const all = Array.from(it);
      const n = all.length;
      let pos = 0;
      let bytesLeft = mode === "chars" ? all.reduce((s, c) => s + utf8Len(c), 0) : 0;
      const hint = () => {
        if (mode === "exact") return [n - pos, n - pos];
        if (mode === "chars") return [Math.floor((bytesLeft + 3) / 4), bytesLeft];
        return [0, null];
      };
      const nth = (k) => {
        if (pos + k >= n) {
          pos = n;
          bytesLeft = 0;
          return null;
        }
        for (let i = 0; i < k; i++) bytesLeft -= mode === "chars" ? utf8Len(all[pos + i]) : 0;
        const x = all[pos + k];
        if (mode === "chars") bytesLeft -= utf8Len(x);
        pos += k + 1;
        return x;
      };
      let [lower, upper] = hint();
      if (upper === lower) return lower === 0 ? null : all[pos + genIndex(rng, lower)];
      let consumed = 0;
      let result = null;
      for (;;) {
        if (lower > 1) {
          const ix = genIndex(rng, lower + consumed);
          let skip = lower;
          if (ix < lower) {
            result = nth(ix);
            skip = lower - (ix + 1);
          }
          if (upper === lower) return result;
          consumed += lower;
          if (skip > 0) nth(skip - 1);
        } else {
          if (pos >= n) return result;
          const elem = nth(0);
          consumed += 1;
          if (genIndex(rng, consumed) === 0) result = elem;
        }
        [lower, upper] = hint();
      }
    },
    iterChooseExact(it, rng) {
      return rand.iterChoose(it, rng, "exact");
    },
    iterChooseChars(it, rng) {
      return rand.iterChoose(it, rng, "chars");
    },
    iterChooseMultiple(it, rng, n) {
      const out = [];
      let i = 0;
      for (const x of it) {
        if (out.length < n) {
          out.push(x);
          continue;
        }
        const k = genIndex(rng, i + 1 + n);
        if (k < n) out[k] = x;
        i++;
      }
      return out;
    },
    nextU32(rng) {
      return rng.u32();
    },
    // `rand::random::<T>()` and friends: on the thread's generator.
    random(T) {
      return rand.gen(rand.thread(), T);
    },
    randomRange(lo, hi, inclusive, T) {
      return rand.range(rand.thread(), lo, hi, inclusive, T);
    },
    randomBool(p) {
      return rand.bool(rand.thread(), p);
    },
    randomRatio(num, den) {
      return rand.ratio(rand.thread(), num, den);
    },
    // (A `u64` is a JS number: rand's 64 bits, rounded past 2^53.)
    nextU64(rng) {
      const [lo, hi] = rng.u64();
      return hi * 4294967296 + lo;
    },
    // `rng.sample(d)` / `rng.sample_iter(d)`: the distribution's own `sample`.
    sampleDist(rng, d) {
      return d.sample(rng);
    },
    sampleDistIter(rng, d) {
      return Iter.of(function* () {
        for (;;) yield d.sample(rng);
      });
    },
  };
  // The distributions whose output is the caller's type, as `rng.sample(StandardNormal)` (the
  // RNG first) or `StandardNormal.sample(&mut rng)` (the RNG second), and their `sample_iter`.
  const unitDists = {
    standard: (rng, d, T) => (typeof T === "function" && typeof d?.sample === "function" ? d.sample(rng) : rand.gen(rng, T)),
    standardNormal: (rng) => {
      const u = 1 - rng.f64();
      return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * rng.f64());
    },
    exp1: (rng) => -Math.log(1 - rng.f64()),
    open01: (rng) => {
      for (;;) {
        const f = rng.f64();
        if (f > 0) return f;
      }
    },
    openClosed01: (rng) => 1 - rng.f64(),
  };
  for (const [name, f] of Object.entries(unitDists)) {
    const one = (a, b, T) => {
      const [rng, d] = a instanceof RandRng ? [a, b] : [b, a];
      const v = f(rng, d, T);
      return T?.kind === "f32" ? Math.fround(v) : v;
    };
    rand[name] = one;
    rand[name + "Iter"] = (a, b, T) =>
      Iter.of(function* () {
        for (;;) yield one(a, b, T);
      });
  }
  rt.$rand = rand;
  // The bundled `itertools` crate's `Itertools` methods, on the runtime's iterators: lazy where
  // itertools is lazy, by Rust's equality (`key`) and order (`cmp`).
  const lazy = (g) => new Iter(g());
  const itertools = {
    join(it, sep) {
      let out = "";
      let first = true;
      for (const x of it) {
        if (!first) out += sep;
        out += rt.$disp(x);
        first = false;
      }
      return out;
    },
    collectVec(it) {
      return Array.from(it);
    },
    sorted(it) {
      return rt.$iter(Array.from(it).sort(cmp));
    },
    sortedBy(it, f) {
      return rt.$iter(Array.from(it).sort((a, b) => f(a, b)));
    },
    sortedByKey(it, f) {
      return rt.$iter(Array.from(it).sort((a, b) => cmp(f(a), f(b))));
    },
    sortedByCachedKey(it, f) {
      const ks = Array.from(it, (x) => [f(x), x]);
      return rt.$iter(ks.sort((a, b) => cmp(a[0], b[0])).map((p) => p[1]));
    },
    unique(it) {
      return itertools.uniqueBy(it, (x) => x);
    },
    uniqueBy(it, f) {
      return lazy(function* () {
        const seen = new Set();
        for (const x of it) {
          const k = key(f(x));
          if (!seen.has(k)) {
            seen.add(k);
            yield x;
          }
        }
      });
    },
    dedup(it) {
      return lazy(function* () {
        let first = true;
        let last;
        for (const x of it) {
          if (first || !eq(last, x)) yield x;
          last = x;
          first = false;
        }
      });
    },
    // Each value seen a second time, once.
    duplicates(it) {
      return lazy(function* () {
        const seen = new Map();
        for (const x of it) {
          const k = key(x);
          const n = seen.get(k) ?? 0;
          if (n === 1) yield x;
          seen.set(k, n + 1);
        }
      });
    },
    // `tuple_windows::<(_, _)>()`: overlapping `n`-tuples (arrays).
    tupleWindows(it, n) {
      return lazy(function* () {
        const w = [];
        for (const x of it) {
          w.push(x);
          if (w.length > n) w.shift();
          if (w.length === n) yield w.map(clone);
        }
      });
    },
    circularTupleWindows(it, n) {
      return lazy(function* () {
        const xs = Array.from(it);
        if (xs.length === 0) return;
        for (let i = 0; i < xs.length; i++) {
          const t = [];
          for (let j = 0; j < n; j++) t.push(clone(xs[(i + j) % xs.length]));
          yield t;
        }
      });
    },
    tuples(it, n) {
      return lazy(function* () {
        let t = [];
        for (const x of it) {
          t.push(x);
          if (t.length === n) {
            yield t;
            t = [];
          }
        }
      });
    },
    // Index lists of `combinations(k)`, in itertools' (lexicographic) order.
    combinations(it, k) {
      const pool = Array.from(it);
      return lazy(function* () {
        const n = pool.length;
        if (k > n) return;
        const idx = Array.from({ length: k }, (_, i) => i);
        for (;;) {
          yield idx.map((i) => clone(pool[i]));
          let i = k - 1;
          while (i >= 0 && idx[i] === i + n - k) i--;
          if (i < 0) return;
          idx[i]++;
          for (let j = i + 1; j < k; j++) idx[j] = idx[j - 1] + 1;
        }
      });
    },
    combinationsWithReplacement(it, k) {
      const pool = Array.from(it);
      return lazy(function* () {
        const n = pool.length;
        if (n === 0 && k > 0) return;
        const idx = new Array(k).fill(0);
        for (;;) {
          yield idx.map((i) => clone(pool[i]));
          let i = k - 1;
          while (i >= 0 && idx[i] === n - 1) i--;
          if (i < 0) return;
          const v = idx[i] + 1;
          for (let j = i; j < k; j++) idx[j] = v;
        }
      });
    },
    permutations(it, k) {
      const pool = Array.from(it);
      return lazy(function* () {
        const n = pool.length;
        if (k > n) return;
        const used = new Array(n).fill(false);
        const cur = [];
        function* go() {
          if (cur.length === k) {
            yield cur.map((i) => clone(pool[i]));
            return;
          }
          for (let i = 0; i < n; i++) {
            if (used[i]) continue;
            used[i] = true;
            cur.push(i);
            yield* go();
            cur.pop();
            used[i] = false;
          }
        }
        yield* go();
      });
    },
    // By size, then in combinations' order.
    powerset(it) {
      const pool = Array.from(it);
      return lazy(function* () {
        for (let k = 0; k <= pool.length; k++) yield* itertools.combinations(pool, k);
      });
    },
    cartesianProduct(it, other) {
      const ys = Array.from(items(other));
      return lazy(function* () {
        for (const x of it) for (const y of ys) yield [clone(x), clone(y)];
      });
    },
    multiCartesianProduct(it) {
      const pools = Array.from(it, (p) => Array.from(items(p)));
      return lazy(function* () {
        if (pools.length === 0 || pools.some((p) => p.length === 0)) return;
        const idx = new Array(pools.length).fill(0);
        for (;;) {
          yield idx.map((i, j) => clone(pools[j][i]));
          let j = pools.length - 1;
          while (j >= 0 && ++idx[j] === pools[j].length) idx[j--] = 0;
          if (j < 0) return;
        }
      });
    },
    interleave(it, other) {
      const b = items(other)[Symbol.iterator]();
      const a = it[Symbol.iterator]();
      return lazy(function* () {
        let aDone = false;
        let bDone = false;
        while (!aDone || !bDone) {
          if (!aDone) {
            const r = a.next();
            if (r.done) aDone = true;
            else yield r.value;
          }
          if (!bDone) {
            const r = b.next();
            if (r.done) bDone = true;
            else yield r.value;
          }
        }
      });
    },
    intersperse(it, sep) {
      return lazy(function* () {
        let first = true;
        for (const x of it) {
          if (!first) yield clone(sep);
          yield x;
          first = false;
        }
      });
    },
    // `it.set_from(src)` over `iter_mut()`: slots (numbers, strings, bools) are set, objects
    // replaced in place; the number assigned.
    setFromSlots(it, src) {
      const s = items(src)[Symbol.iterator]();
      let n = 0;
      for (const slot of it) {
        const r = s.next();
        if (r.done) break;
        slot[1] = r.value;
        n++;
      }
      return n;
    },
    setFrom(it, src) {
      const s = items(src)[Symbol.iterator]();
      let n = 0;
      for (const x of it) {
        const r = s.next();
        if (r.done) break;
        rt.$replace(x, r.value);
        n++;
      }
      return n;
    },
    merge(it, other) {
      const xs = Array.from(it);
      const ys = Array.from(items(other));
      const out = [];
      let i = 0;
      let j = 0;
      while (i < xs.length && j < ys.length) out.push(cmp(ys[j], xs[i]) < 0 ? ys[j++] : xs[i++]);
      return rt.$iter(out.concat(xs.slice(i), ys.slice(j)));
    },
    zipEq(it, other, loc) {
      const a = it[Symbol.iterator]();
      const b = items(other)[Symbol.iterator]();
      return lazy(function* () {
        for (;;) {
          const x = a.next();
          const y = b.next();
          if (x.done && y.done) return;
          if (x.done || y.done) throw new CarciniPanic("itertools: .zip_eq() reached end of one iterator before the other", loc);
          yield [x.value, y.value];
        }
      });
    },
    takeWhileInclusive(it, p) {
      return lazy(function* () {
        for (const x of it) {
          yield x;
          if (!p(x)) return;
        }
      });
    },
    positions(it, p) {
      return lazy(function* () {
        let i = 0;
        for (const x of it) {
          if (p(x)) yield i;
          i++;
        }
      });
    },
    dropping(it, n) {
      const i = it[Symbol.iterator]();
      for (let k = 0; k < n && !i.next().done; k++);
      return new Iter(i);
    },
    counts(it) {
      return itertools.countsBy(it, (x) => x);
    },
    countsBy(it, f) {
      const m = new HashMap();
      for (const x of it) {
        const k = f(x);
        m.$insert(k, (m.$get(k) ?? 0) + 1);
      }
      return m;
    },
    allEqual(it) {
      let first = true;
      let v;
      for (const x of it) {
        if (first) v = x;
        else if (!eq(v, x)) return false;
        first = false;
      }
      return true;
    },
    allUnique(it) {
      const seen = new Set();
      for (const x of it) {
        const k = key(x);
        if (seen.has(k)) return false;
        seen.add(k);
      }
      return true;
    },
    contains(it, v) {
      for (const x of it) if (eq(x, v)) return true;
      return false;
    },
    findPosition(it, p) {
      let i = 0;
      for (const x of it) {
        if (p(x)) return [i, x];
        i++;
      }
      return null;
    },
    // `minmax()` and friends, as itertools' `minmax_impl` (two items at a time; the first
    // minimum and the last maximum), into the prelude's `MinMaxResult` class `M`.
    minmaxImpl(it, key, lt, M) {
      const i = it[Symbol.iterator]();
      const a = i.next();
      if (a.done) return M.NoElements;
      const b = i.next();
      if (b.done) return M.OneElement(a.value);
      const [x, y] = [a.value, b.value];
      const [xk, yk] = [key(x), key(y)];
      let [min, max, mink, maxk] = !lt(y, x, yk, xk) ? [x, y, xk, yk] : [y, x, yk, xk];
      for (;;) {
        const f = i.next();
        if (f.done) break;
        const first = f.value;
        const fk = key(first);
        const s = i.next();
        if (s.done) {
          if (lt(first, min, fk, mink)) min = first;
          else if (!lt(first, max, fk, maxk)) max = first;
          break;
        }
        const second = s.value;
        const sk = key(second);
        const [lo, lok, hi, hik] = !lt(second, first, sk, fk)
          ? [first, fk, second, sk]
          : [second, sk, first, fk];
        if (lt(lo, min, lok, mink)) [min, mink] = [lo, lok];
        if (!lt(hi, max, hik, maxk)) [max, maxk] = [hi, hik];
      }
      return M.MinMax(min, max);
    },
    minmax(it, M) {
      return itertools.minmaxImpl(it, () => undefined, (x, y) => cmp(x, y) < 0, M);
    },
    minmaxBy(it, f, M) {
      return itertools.minmaxImpl(it, () => undefined, (x, y) => f(x, y) < 0, M);
    },
    minmaxByKey(it, f, M) {
      return itertools.minmaxImpl(it, f, (x, y, xk, yk) => cmp(xk, yk) < 0, M);
    },
    // `exactly_one()`: `Ok` of the one item, else `Err` of the prelude's `ExactlyOneError`
    // (`E`) holding what was taken (everything, past two).
    exactlyOne(it, E) {
      const i = it[Symbol.iterator]();
      const a = i.next();
      if (a.done) return rt.Err(new E([], false));
      const b = i.next();
      if (b.done) return rt.Ok(a.value);
      const rest = [a.value, b.value];
      for (let n = i.next(); !n.done; n = i.next()) rest.push(n.value);
      return rt.Err(new E(rest, true));
    },
    // `fold_while(init, f)`: stops at the first `Done` (the prelude's `FoldWhile`, `W`), leaving
    // the rest of the items in the iterator.
    foldWhile(it, init, f, W) {
      const i = it[Symbol.iterator]();
      let acc = init;
      for (let n = i.next(); !n.done; n = i.next()) {
        const r = f(acc, n.value);
        if (r.$tag === 1) return r;
        acc = r[0];
      }
      return W.Continue(acc);
    },
    // `kmerge()` / `kmerge_by(less)`: itertools' heap of (head, rest) pairs, so equal items come
    // out in the same order.
    kmerge(it) {
      return itertools.kmergeBy(it, (a, b) => cmp(a, b) < 0);
    },
    kmergeBy(it, less) {
      const heap = [];
      for (const src of it) {
        const tail = items(src)[Symbol.iterator]();
        const h = tail.next();
        if (!h.done) heap.push({ head: h.value, tail });
      }
      const lt = (a, b) => less(heap[a].head, heap[b].head);
      const swap = (a, b) => ([heap[a], heap[b]] = [heap[b], heap[a]]);
      const sift = (index) => {
        let pos = index;
        let child = 2 * pos + 1;
        while (child + 1 < heap.length) {
          if (lt(child + 1, child)) child++;
          if (!lt(child, pos)) return;
          swap(pos, child);
          pos = child;
          child = 2 * pos + 1;
        }
        if (child + 1 === heap.length && lt(child, pos)) swap(pos, child);
      };
      for (let i = (heap.length >> 1) - 1; i >= 0; i--) sift(i);
      return lazy(function* () {
        while (heap.length) {
          const top = heap[0];
          const result = top.head;
          const n = top.tail.next();
          if (!n.done) top.head = n.value;
          else {
            const last = heap.pop();
            if (heap.length) heap[0] = last;
          }
          sift(0);
          yield result;
        }
      });
    },
    kSmallest(it, k) {
      return rt.$iter(Array.from(it).sort(cmp).slice(0, k));
    },
    kLargest(it, k) {
      return rt.$iter(Array.from(it).sort((a, b) => cmp(b, a)).slice(0, k));
    },
    // `chunk_by(key)`: runs of equal keys, as (key, the run's iterator).
    chunkBy(it, f) {
      return lazy(function* () {
        let run = null;
        let k;
        for (const x of it) {
          const kx = f(x);
          if (run !== null && eq(k, kx)) run.push(x);
          else {
            if (run !== null) yield [k, rt.$iter(run)];
            run = [x];
            k = kx;
          }
        }
        if (run !== null) yield [k, rt.$iter(run)];
      });
    },
    intoGroupMap(it) {
      const m = new HashMap();
      for (const [k, v] of it) {
        const vs = m.$get(k);
        if (vs === null) m.$insert(k, [v]);
        else vs.push(v);
      }
      return m;
    },
    intoGroupMapBy(it, f) {
      const m = new HashMap();
      for (const x of it) {
        const k = f(x);
        const vs = m.$get(k);
        if (vs === null) m.$insert(k, [x]);
        else vs.push(x);
      }
      return m;
    },
    // `collect_tuple()`: `Some` only when there are exactly `n`.
    collectTuple(it, n) {
      const xs = [];
      for (const x of it) {
        xs.push(x);
        if (xs.length > n) return null;
      }
      return xs.length === n ? xs : null;
    },
    nextTuple(it, n) {
      const i = it[Symbol.iterator]();
      const xs = [];
      while (xs.length < n) {
        const r = i.next();
        if (r.done) return null;
        xs.push(r.value);
      }
      return xs;
    },
    repeatN(x, n) {
      return lazy(function* () {
        for (let i = 0; i < n; i++) yield clone(x);
      });
    },
  };
  rt.$itertools = itertools;
  // The bundled `anyhow` crate: an `anyhow::Error` is a message and the error it was caused by
  // (another one, a std error, or your `impl Error` type, whose `source()` continues the chain).
  // `{}` shows the message; `{:?}` (what `main` prints) adds the chain as anyhow does.
  class AnyhowError {
    constructor(msg, cause, inner) {
      this.msg = msg;
      this.cause = cause ?? null;
      // The error it wraps as is (`?` on a `ParseIntError`), for `root_cause()`.
      this.inner = inner;
    }
    toString() {
      return this.msg;
    }
    // Every error in the chain, as values that display themselves.
    $chain() {
      const out = [this];
      let cur = this.cause;
      while (cur != null) {
        out.push(cur);
        cur = cur instanceof AnyhowError ? cur.cause : typeof cur.source === "function" ? cur.source() : null;
      }
      return out;
    }
    $dbg() {
      const causes = this.$chain()
        .slice(1)
        .map((c) => rt.$disp(c));
      if (causes.length === 0) return this.msg;
      const lines = causes.length === 1 ? [`    ${causes[0]}`] : causes.map((c, i) => `    ${i}: ${c}`);
      return `${this.msg}\n\nCaused by:\n${lines.join("\n")}`;
    }
    $eq(o) {
      return this === o;
    }
  }
  const anyhow = {
    // `anyhow!("..")` / `Error::msg(..)`.
    msg(m) {
      return new AnyhowError(typeof m === "string" ? m : rt.$disp(m));
    },
    // `anyhow!(e)`: a string becomes the message, an error is wrapped.
    wrap(x) {
      return typeof x === "string" ? new AnyhowError(x) : anyhow.from(x);
    },
    // `?` (and `Error::new` / `Error::from`): a std error or your error type, wrapped.
    from(e) {
      if (e instanceof AnyhowError) return e;
      const cause = e != null && typeof e.source === "function" ? e.source() : null;
      return new AnyhowError(rt.$disp(e), cause, e);
    },
    // `result.context(c)` on a `Result<T, E>`, or on a `Result<T, anyhow::Error>`.
    context(r, c) {
      return r.$tag === 0 ? r : new Res(1, new AnyhowError(rt.$disp(c), anyhow.from(r[0])));
    },
    withContext(r, f) {
      return r.$tag === 0 ? r : new Res(1, new AnyhowError(rt.$disp(f()), anyhow.from(r[0])));
    },
    // `option.context(c)`: `None` becomes an error with the message.
    optContext(o, c) {
      return o == null ? new Res(1, new AnyhowError(rt.$disp(c))) : new Res(0, o);
    },
    optWithContext(o, f) {
      return o == null ? new Res(1, new AnyhowError(rt.$disp(f()))) : new Res(0, o);
    },
    // `err.context(c)` on an `anyhow::Error`.
    errContext(e, c) {
      return new AnyhowError(rt.$disp(c), e);
    },
    chain(e) {
      return rt.$iter(e.$chain());
    },
    rootCause(e) {
      const c = e.$chain();
      const last = c[c.length - 1];
      return last instanceof AnyhowError && last.inner !== undefined ? last.inner : last;
    },
  };
  rt.$anyhow = anyhow;
  // A `dyn Error`'s methods: any error value (a std error, yours, an `anyhow::Error`).
  rt.$err = {
    toString: (e) => rt.$disp(e),
    source: (e) => (e instanceof AnyhowError ? e.cause : typeof e?.source === "function" ? e.source() : null),
  };
  rt.$randomBytes = (n) => Array.from(globalThis.crypto.getRandomValues(new Uint8Array(n)));
  rt.$sleep = (ms) => new Promise((resolve) => globalThis.setTimeout(resolve, ms));
  // `js::structured_clone`: a deep copy. Rust values copy by their `Clone` (keeping their
  // classes), JS values (NS objects, parsed JSON) by the browser's `structuredClone`.
  rt.$structuredClone = (v) => {
    if (v === null || typeof v !== "object") return v;
    if (Array.isArray(v)) return v.map(rt.$structuredClone);
    if (typeof v.$clone === "function") return v.$clone();
    if (v instanceof RefCell) return new RefCell(rt.$structuredClone(v.v), v.kind);
    return globalThis.structuredClone(v);
  };
  // `<[T; N]>::try_from(v)`: `v` when it has `n` elements (a copy of a slice); else `Err` of the
  // `Vec`, or a `TryFromSliceError` for a slice.
  rt.$arrayTryFrom = (v, n, slice) =>
    v.length === n ? rt.Ok(slice ? rt.$clone(v) : v) : rt.Err(slice ? new ParseError("TryFromSliceError", "") : v);
  rt.$global = (name) => globalThis[name];
  rt.$setGlobal = (name, value) => {
    globalThis[name] = value;
  };
  // An NS interface built in Rust (`RunOptions { threads: Some(t), ..base }`): a plain object,
  // `base`'s properties overridden by `fields`; a `None` field is left out.
  rt.$iface = (fields, base) => {
    const o = base == null ? {} : { ...base };
    for (const k of Object.keys(fields)) {
      if (fields[k] == null) delete o[k];
      else o[k] = fields[k];
    }
    return o;
  };
  // A `HashMap` the NS API hands out as a JS `Map` or a plain object (`Record`).
  rt.$mapOfJs = (x) => {
    if (x == null || x instanceof HashMap) return x;
    return HashMap.from(x instanceof Map ? x : Object.entries(x));
  };
  // A bindings enum (a TS union, `ScriptArg`, `PlayerRequirement`): the value is the JS value
  // itself. `variants` is `[[test, name, fmt], ..]`: `test` is the value's JS kind for an
  // untagged enum (`"string"`, `"number"`, `"array"`, `"object"`, ..), or the tag field's value
  // when `tag` names that field.
  const jsKind = (v) => (Array.isArray(v) ? "array" : v === null ? "null" : typeof v);
  const jsEnumVariant = (v, variants, tag) =>
    variants.find(([test]) => (tag === undefined ? jsKind(v) === test : v != null && v[tag] === test));
  // `{:?}` of one: `Str("n00dles")`, `Money(MoneyRequirement { .. })`.
  rt.$dbgJsEnum = (v, variants, tag) => {
    const hit = jsEnumVariant(v, variants, tag);
    return hit ? `${hit[1]}(${(hit[2] ?? rt.$dbg)(v)})` : rt.$dbg(v);
  };
  // `{:?}` of an NS interface value: `Server { hostname: "home", max_ram: 8.0 }` over its
  // declared fields (`[[jsName, rustName, fmt], ..]`); a field the object lacks is left out
  // (an `Option` one prints `None`).
  // With `keys` (`#[debug_keys]`), the object's other own keys come first, their values as JS
  // has them (`Flags { target: "n00dles", threads: 8.0, rest: [] }`).
  rt.$dbgIface = (name, o, fields, keys) => {
    if (o === null || typeof o !== "object") return rt.$dbg(o);
    const shown = fields.filter(([js, , fmt]) => js in o || /\$dbgOpt/.test(String(fmt)));
    const declared = new Set(fields.map(([js]) => js));
    const extra = keys ? Object.keys(o).filter((k) => !declared.has(k)) : [];
    const plain = (v) => (Array.isArray(v) ? `[${v.map(plain).join(", ")}]` : rt.$dbg(v));
    return rt.$dbgStruct(
      name,
      [...extra, ...shown.map(([, rust]) => rust)],
      [...extra.map((k) => plain(o[k])), ...shown.map(([js, , fmt]) => fmt(o[js] ?? null))],
    );
  };
  // `ScriptArg::try_from(v)`: `Ok(v)` when the value fits a variant, else `Err(v)`.
  rt.$jsEnumTry = (v, variants, tag) => (jsEnumVariant(v, variants, tag) ? rt.Ok(v) : rt.Err(v));
  // `JsValue`'s methods (wasm-bindgen's): `v.as_string()` is `κσ.$jsv.$asString(v)`. `None` is
  // `null`.
  rt.$jsv = {
    // (A primitive union's `to_string()`: JS's text for it.)
    $asText: (v) => String(v),
    // (`NsResult::into_result()`: the game's `{ success, message }` as a `Result`.)
    $nsResult: (v) => (v != null && v.success ? rt.Ok(v) : rt.Err(v == null ? "" : String(v.message ?? ""))),
    // (`Flags`' readers: `ns.flags(..)`'s plain object, by flag name.)
    $flag: (o, k) => (o != null && Object.hasOwn(o, k) && k !== "_" ? o[k] : null),
    $flagStr: (o, k) => (typeof o?.[k] === "string" ? o[k] : null),
    $flagF64: (o, k) => (typeof o?.[k] === "number" ? o[k] : null),
    $flagBool: (o, k) => (typeof o?.[k] === "boolean" ? o[k] : null),
    $flagList: (o, k) => (k !== "_" && Array.isArray(o?.[k]) ? o[k] : null),
    $asString: (v) => (typeof v === "string" ? v : null),
    $asF64: (v) => (typeof v === "number" ? v : null),
    $asBool: (v) => (typeof v === "boolean" ? v : null),
    $isString: (v) => typeof v === "string",
    $isUndefined: (v) => v === undefined,
    $isNull: (v) => v === null,
    $isNullOrUndefined: (v) => v == null,
    $isObject: (v) => typeof v === "object" && v !== null,
    $isArray: (v) => Array.isArray(v),
    $isFunction: (v) => typeof v === "function",
    $isBigint: (v) => typeof v === "bigint",
    $isSymbol: (v) => typeof v === "symbol",
    $isTruthy: (v) => !!v,
    $isFalsy: (v) => !v,
    $looseEq: (a, b) => a == b,
    $asDebugString: (v) => {
      if (typeof v === "string") return JSON.stringify(v);
      if (typeof v === "bigint") return `${v}n`;
      if (typeof v === "object" && v !== null) {
        try {
          return JSON.stringify(v);
        } catch {
          return String(v);
        }
      }
      return String(v);
    },
    $from: (v) => v,
  };

  // ---- parsing: JSON text to tagged values (`{k:"num", raw, int}` keeps `1` and `1.0` apart,
  // as serde_json does), with serde_json's syntax errors.
  function jsonLex(text) {
    let i = 0;
    // (`at`: how far serde_json has read when it fails: past the offending character, or the
    // end. Values carry their end, `e`, for the errors found while converting them.)
    const fail = (msg, at = i) => {
      const e = new JsonError(msg);
      e.at = at < text.length ? at + 1 : text.length;
      e.kind = msg.startsWith("EOF") ? "eof" : "syntax";
      throw e;
    };
    const ws = () => {
      while (i < text.length && " \t\n\r".includes(text[i])) i++;
    };
    // (`b`: where a value begins, for "trailing characters" after a tuple's last element.)
    const value = (what) => {
      ws();
      const b = i;
      const node = value0(what);
      node.b = b;
      return node;
    };
    const value0 = (what) => {
      ws();
      if (i >= text.length) fail(`EOF while parsing ${what}`);
      const c = text[i];
      if (c === "{") {
        i++;
        const pairs = [];
        ws();
        if (text[i] === "}") {
          i++;
          return { k: "obj", v: pairs, e: i };
        }
        for (;;) {
          ws();
          if (i >= text.length) fail("EOF while parsing an object");
          if (text[i] !== '"') fail(text[i] === "}" ? "trailing comma" : "key must be a string");
          const key = string();
          ws();
          if (i >= text.length) fail("EOF while parsing an object");
          if (text[i] !== ":") fail("expected `:`");
          i++;
          pairs.push([key, value("a value")]);
          ws();
          if (i >= text.length) fail("EOF while parsing an object");
          if (text[i] === ",") {
            i++;
            continue;
          }
          if (text[i] === "}") {
            i++;
            return { k: "obj", v: pairs, e: i };
          }
          fail("expected `,` or `}`");
        }
      }
      if (c === "[") {
        i++;
        const items = [];
        ws();
        if (text[i] === "]") {
          i++;
          return { k: "arr", v: items, e: i };
        }
        for (;;) {
          ws();
          if (text[i] === "]") fail("trailing comma");
          items.push(value("a list"));
          ws();
          if (i >= text.length) fail("EOF while parsing a list");
          if (text[i] === ",") {
            i++;
            continue;
          }
          if (text[i] === "]") {
            i++;
            return { k: "arr", v: items, e: i };
          }
          fail("expected `,` or `]`");
        }
      }
      if (c === '"') {
        const v = string();
        return { k: "str", v, e: i };
      }
      for (const [word, node] of [
        ["true", { k: "bool", v: true }],
        ["false", { k: "bool", v: false }],
        ["null", { k: "null" }],
      ]) {
        if (c === word[0]) {
          for (let j = 0; j < word.length; j++) {
            if (i + j >= text.length) fail(`EOF while parsing ${what}`, i + j);
            if (text[i + j] !== word[j]) fail("expected ident", i + j);
          }
          i += word.length;
          return { ...node, e: i };
        }
      }
      const m = /^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?/.exec(text.slice(i));
      if (!m || m[0] === "-") fail(c === "-" ? "invalid number" : "expected value");
      i += m[0].length;
      return { k: "num", raw: m[0], int: !m[2] && !m[3], e: i };
    };
    const string = () => {
      i++;
      let out = "";
      for (;;) {
        if (i >= text.length) fail("EOF while parsing a string");
        const c = text[i];
        if (c === '"') {
          i++;
          return out;
        }
        if (c < " ") fail("control character (\\u0000-\\u001F) found while parsing a string");
        if (c === "\\") {
          const e = text[i + 1];
          if (e === undefined) fail("EOF while parsing a string");
          const simple = { '"': '"', "\\": "\\", "/": "/", b: "\b", f: "\f", n: "\n", r: "\r", t: "\t" }[e];
          if (simple !== undefined) {
            out += simple;
            i += 2;
            continue;
          }
          if (e === "u") {
            const hex = text.slice(i + 2, i + 6);
            if (!/^[0-9a-fA-F]{4}$/.test(hex)) fail(hex.length < 4 ? "EOF while parsing a string" : "invalid escape");
            out += String.fromCharCode(parseInt(hex, 16));
            i += 6;
            continue;
          }
          fail("invalid escape");
        }
        out += c;
        i++;
      }
    };
    const v = value("a value");
    ws();
    if (i < text.length) fail("trailing characters");
    return v;
  }

  // What a JSON value is, as serde words an unexpected one.
  function jsonUnexpected(n) {
    switch (n.k) {
      case "null":
        return "null";
      case "bool":
        return `boolean \`${n.v}\``;
      case "num": {
        if (n.int) return `integer \`${n.raw}\``;
        let f = fmtF(Number(n.raw));
        if (!/[.eEi]|NaN/.test(f)) f += ".0";
        return `floating point \`${f}\``;
      }
      case "str":
        return `string ${rt.$dbgStr(n.v)}`;
      case "arr":
        return "sequence";
      default:
        return "map";
    }
  }

  // What a schema expects, as serde's visitors word it.
  function jsonExpecting(s) {
    s = jsonSchema(s);
    switch (s.t) {
      case "int":
      case "big":
        return s.name;
      case "float":
        return s.f32 ? "f32" : "f64";
      case "bool":
        return "a boolean";
      case "char":
        return "a character";
      case "str":
        return "a string";
      case "unit":
        return "unit";
      case "opt":
        return jsonExpecting(s.s);
      case "vec":
      case "set":
        return "a sequence";
      case "tuple":
        return `a tuple of size ${s.s.length}`;
      case "map":
        return "a map";
      case "cell":
        return jsonExpecting(s.s);
      case "result":
        return "enum Result";
      case "struct":
        if (s.shape === "unit") return `unit struct ${s.name}`;
        if (s.shape === "tuple") return s.fields.length === 1 ? `tuple struct ${s.name}` : `tuple struct ${s.name}`;
        return `struct ${s.name}`;
      case "enum":
        return s.tag === undefined ? `enum ${s.name}` : s.content === undefined ? `internally tagged enum ${s.name}` : `adjacently tagged enum ${s.name}`;
      case "jsonvalue":
        return "any valid JSON value";
      default:
        return "any value";
    }
  }

  function jsonInvalid(n, s) {
    return new JsonError(`invalid type: ${jsonUnexpected(n)}, expected ${typeof s === "string" ? s : jsonExpecting(s)}`);
  }

  function oneOf(names) {
    const q = names.map((n) => `\`${n}\``);
    if (q.length === 0) return "there are no variants";
    if (q.length === 1) return `expected ${q[0]}`;
    if (q.length === 2) return `expected ${q[0]} or ${q[1]}`;
    return `expected one of ${q.join(", ")}`;
  }

  // A plain JS value of an untyped (`JsValue`) part.
  function jsonPlain(n) {
    switch (n.k) {
      case "null":
        return null;
      case "num":
        return Number(n.raw);
      case "arr":
        return n.v.map(jsonPlain);
      case "obj":
        return Object.fromEntries(n.v.map(([k, x]) => [k, jsonPlain(x)]));
      default:
        return n.v;
    }
  }

  // serde_json reads an integer past both `u64` and `i64` as a float.
  function jsonPast64(n, big, s) {
    if (big > 18446744073709551615n || big < -9223372036854775808n) {
      throw new JsonError(`invalid type: floating point \`${ryu(Number(n.raw), false)}\`, expected ${s.name}`);
    }
  }

  function jsonIn(n, s) {
    try {
      return jsonIn0(n, s);
    } catch (e) {
      if (e instanceof JsonError && e.at === undefined) {
        e.at = n.e;
        e.kind = "data";
      }
      throw e;
    }
  }

  // A JSON value as a `serde_json::Value`, made by the prelude's functions (`s.mk`).
  function jsonValue(n, s) {
    switch (n.k) {
      case "null":
        return s.mk.nul();
      case "bool":
        return s.mk.bool(n.v);
      case "num":
        return s.mk.num(n.raw, n.int);
      case "str":
        return s.mk.str(n.v);
      case "arr":
        return s.mk.arr(n.v.map((x) => jsonValue(x, s)));
      default:
        return s.mk.obj(
          n.v.map((p) => p[0]),
          n.v.map((p) => jsonValue(p[1], s)),
        );
    }
  }

  function jsonIn0(n, s) {
    s = jsonSchema(s);
    switch (s.t) {
      case "jsonvalue":
        return jsonValue(n, s);
      case "int": {
        if (n.k !== "num") throw jsonInvalid(n, s);
        if (!n.int) throw jsonInvalid(n, s);
        const big = BigInt(n.raw);
        jsonPast64(n, big, s);
        if (big < BigInt(s.min) || big > BigInt(s.max)) {
          throw new JsonError(`invalid value: integer \`${n.raw}\`, expected ${s.name}`);
        }
        return Number(n.raw);
      }
      case "big": {
        if (n.k !== "num" || !n.int) throw jsonInvalid(n, s);
        const big = BigInt(n.raw);
        if (s.min != null) jsonPast64(n, big, s);
        if (s.min != null && (big < BigInt(s.min) || big > BigInt(s.max))) {
          throw new JsonError(`invalid value: integer \`${n.raw}\`, expected ${s.name}`);
        }
        return big;
      }
      case "float": {
        if (n.k !== "num") throw jsonInvalid(n, s);
        const x = Number(n.raw);
        return s.f32 ? Math.fround(x) : x;
      }
      case "bool":
        if (n.k !== "bool") throw jsonInvalid(n, s);
        return n.v;
      case "str":
        if (n.k !== "str") throw jsonInvalid(n, s);
        return n.v;
      case "char":
        if (n.k !== "str") throw jsonInvalid(n, s);
        if ([...n.v].length !== 1) {
          throw new JsonError(`invalid value: string ${rt.$dbgStr(n.v)}, expected a character`);
        }
        return n.v;
      case "unit":
        if (n.k !== "null") throw jsonInvalid(n, s);
        return undefined;
      case "opt":
        return n.k === "null" ? null : some(jsonIn(n, s.s));
      case "vec": {
        if (n.k !== "arr") throw jsonInvalid(n, s);
        const items = n.v.map((x) => jsonIn(x, s.s));
        return s.cls === "Deque" ? Deque.from(items) : items;
      }
      case "set": {
        if (n.k !== "arr") throw jsonInvalid(n, s);
        const items = n.v.map((x) => jsonIn(x, s.s));
        if (s.cls === "BTreeSet") return rt.BTreeSet.from(items);
        const out = new rt[s.cls]();
        for (const x of items) out.$insert(x);
        return out;
      }
      case "tuple": {
        if (n.k !== "arr") throw jsonInvalid(n, s);
        if (n.v.length < s.s.length) {
          throw new JsonError(`invalid length ${n.v.length}, expected a tuple of size ${s.s.length}`);
        }
        if (n.v.length > s.s.length) throw new JsonError("trailing characters");
        return s.s.map((e, i) => jsonIn(n.v[i], e));
      }
      case "map": {
        if (n.k !== "obj") throw jsonInvalid(n, s);
        const pairs = [];
        const ks = jsonSchema(s.k);
        for (const [k, x] of n.v) {
          let key = k;
          if (ks.t === "int" || ks.t === "big") {
            if (!/^-?\d+$/.test(k)) throw jsonInvalid({ k: "str", v: k }, ks);
            key = jsonIn({ k: "num", raw: k, int: true }, ks);
          } else if (ks.t === "char") {
            key = jsonIn({ k: "str", v: k }, ks);
          }
          pairs.push([key, jsonIn(x, s.v)]);
        }
        if (s.cls === "BTreeMap") return rt.BTreeMap.from(pairs);
        const out = new rt.HashMap();
        for (const [k, x] of pairs) out.$insert(k, x);
        return out;
      }
      case "cell":
        return new rt.RefCell(jsonIn(n, s.s), "RefCell");
      case "result": {
        if (n.k !== "obj" || n.v.length !== 1) throw jsonInvalid(n, s);
        const [k, x] = n.v[0];
        if (k === "Ok") return rt.Ok(jsonIn(x, s.ok));
        if (k === "Err") return rt.Err(jsonIn(x, s.err));
        throw new JsonError(`unknown variant \`${k}\`, ${oneOf(["Ok", "Err"])}`);
      }
      case "struct":
        return jsonBuild(n, s.shape, s.fields, s, (args) => new s.cls(...args));
      case "enum": {
        // `#[serde(untagged)]`: the first variant whose content matches.
        if (s.untagged) {
          for (const variant of s.variants) {
            if (variant.shape === "unit") {
              if (n.k === "null") return s.cls[variant.js];
              continue;
            }
            try {
              return jsonBuild(n, variant.shape, variant.fields, `${s.name}::${variant.js}`, (args) => s.cls[variant.js](...args));
            } catch (e) {
              if (!(e instanceof JsonError)) throw e;
            }
          }
          const e = new JsonError(`data did not match any variant of untagged enum ${s.name}`);
          // (serde_json reports it without a position.)
          e.noPos = true;
          throw e;
        }
        const unknown = (key) => new JsonError(`unknown variant \`${key}\`, ${oneOf(s.variants.map((v) => v.key))}`);
        // (An error in the content is placed at its end, as serde_json places it.)
        const err = (node, e) => {
          if (e instanceof JsonError && e.at === undefined) {
            e.at = node.e;
            e.kind = "data";
          }
          return e;
        };
        const build = (variant, content) => {
          const expecting = variant.shape === "named" ? `struct variant ${s.name}::${variant.js}` : `tuple variant ${s.name}::${variant.js}`;
          try {
            return jsonBuild(content, variant.shape, variant.fields, expecting, (args) => s.cls[variant.js](...args));
          } catch (e) {
            throw err(content, e);
          }
        };
        // `#[serde(tag = "..")]`: an object whose tag field names the variant; the content is
        // the other fields (internally tagged) or the `content` field (adjacently tagged).
        if (s.tag !== undefined) {
          let entry;
          let rest;
          if (n.k === "obj") {
            entry = n.v.find(([k]) => k === s.tag);
            if (!entry) throw new JsonError(`missing field \`${s.tag}\``);
            entry = entry[1];
            rest = { k: "obj", v: n.v.filter(([k]) => k !== s.tag), e: n.e };
          } else if (n.k === "arr" && s.content === undefined && n.v.length > 0) {
            // (Internally tagged, from a sequence: the tag first, then the content.)
            entry = n.v[0];
            rest = n.v[1] ?? { k: "obj", v: [], e: n.e };
          } else throw jsonInvalid(n, s);
          if (entry.k !== "str") throw err(entry, jsonInvalid(entry, "variant identifier"));
          const variant = s.variants.find((v) => v.key === entry.v);
          if (!variant) throw err(entry, unknown(entry.v));
          if (s.content === undefined) {
            // serde reads the content from a buffer once the text is read: its errors have no
            // position.
            try {
              if (variant.shape === "unit") return s.cls[variant.js];
              if (variant.shape === "tuple" && variant.fields.length === 1) return s.cls[variant.js](jsonIn(rest, variant.fields[0].s));
              return build(variant, rest);
            } catch (e) {
              if (e instanceof JsonError) e.noPos = true;
              throw e;
            }
          }
          const content = n.v.find(([k]) => k === s.content);
          if (variant.shape === "unit") {
            if (content && content[1].k !== "null") throw err(content[1], jsonInvalid(content[1], `unit variant ${s.name}::${variant.js}`));
            return s.cls[variant.js];
          }
          if (!content) throw new JsonError(`missing field \`${s.content}\``);
          return build(variant, content[1]);
        }
        let key;
        let content = null;
        if (n.k === "str") key = n.v;
        else if (n.k === "obj" && n.v.length === 1) [key, content] = n.v[0];
        else throw jsonInvalid(n, s);
        const variant = s.variants.find((v) => v.key === key);
        if (!variant) throw unknown(key);
        if (variant.shape === "unit") {
          if (content !== null && content.k !== "null") throw err(content, jsonInvalid(content, "unit"));
          return s.cls[variant.js];
        }
        if (content === null) {
          const what = variant.shape === "named" ? "struct variant" : variant.fields.length === 1 ? "newtype variant" : "tuple variant";
          throw new JsonError(`invalid type: unit variant, expected ${what}`);
        }
        return build(variant, content);
      }
      default:
        return jsonPlain(n);
    }
  }

  // A struct's (or variant's) value from an object, an array, its one field's value, or null.
  function jsonBuild(n, shape, fields, expecting, make) {
    const expect = typeof expecting === "string" ? expecting : jsonExpecting(expecting);
    if (shape === "unit") {
      if (n.k !== "null") throw jsonInvalid(n, expect);
      return make([]);
    }
    if (shape === "tuple" && fields.length === 1) return make([jsonIn(n, fields[0].s)]);
    if (n.k === "arr") {
      const live = fields.filter((f) => !f.skipIn);
      if (n.v.length < live.length) {
        throw new JsonError(`invalid length ${n.v.length}, expected ${expect} with ${live.length} element${live.length === 1 ? "" : "s"}`);
      }
      if (n.v.length > live.length) {
        // (A syntax error to serde_json, where it finds the extra element.)
        const e = new JsonError("trailing characters");
        e.at = n.v[live.length].b + 1;
        e.kind = "syntax";
        throw e;
      }
      let k = 0;
      return make(fields.map((f) => (f.skipIn ? f.def() : jsonIn(n.v[k++], f.s))));
    }
    if (shape === "tuple" || n.k !== "obj") throw jsonInvalid(n, expect);
    const seen = new Set();
    const got = new Map();
    // (The entries no named field takes: a `#[serde(flatten)]` field's.)
    const rest = [];
    for (const [k, x] of n.v) {
      const f = fields.find((f) => f.key === k && !f.skipIn && !f.flatten);
      if (!f) {
        rest.push([k, x]);
        continue;
      }
      if (seen.has(k)) throw new JsonError(`duplicate field \`${k}\``);
      seen.add(k);
      got.set(f, x);
    }
    return make(
      fields.map((f) => {
        if (f.flatten && !f.skipIn) return jsonFlat(rest, f.s, n.e);
        if (got.has(f)) return jsonIn(got.get(f), f.s);
        if (f.def) return f.def();
        if (jsonSchema(f.s).t === "opt") return null;
        throw new JsonError(`missing field \`${f.key}\``);
      }),
    );
  }

  // A `#[serde(flatten)]` field from the entries no named field took (`rest`, which it takes its
  // own from, in field order: a struct its fields', a map all that are left). An `Option` is
  // `None` when they don't fit.
  function jsonFlat(rest, s, end) {
    s = jsonSchema(s);
    const node = { k: "obj", v: rest.slice(), e: end };
    let out;
    try {
      out = jsonIn(node, s.t === "opt" ? s.s : s);
    } catch (e) {
      if (!(e instanceof JsonError)) throw e;
      if (s.t === "opt") return null;
      // (serde reads it from a buffer of the entries once the object is read: the error is at
      // the object's end.)
      e.at = end;
      e.noPos = false;
      throw e;
    }
    if (s.t === "opt") out = some(out);
    const taken = jsonTakes(s);
    if (taken === null) rest.length = 0;
    else for (let i = rest.length - 1; i >= 0; i--) if (taken.has(rest[i][0])) rest.splice(i, 1);
    return out;
  }

  // The keys a flattened value takes from the object: a struct's fields' (and its own flattened
  // fields'), or `null` for all of them (a map).
  function jsonTakes(s) {
    s = jsonSchema(s);
    switch (s.t) {
      case "map":
        return null;
      case "opt":
      case "cell":
        return jsonTakes(s.s);
      case "struct": {
        const out = new Set();
        if (s.shape !== "named") return out;
        if (s.tag !== undefined) out.add(s.tag);
        for (const f of s.fields) {
          if (!f.flatten) {
            out.add(f.key);
            continue;
          }
          const inner = jsonTakes(f.s);
          if (inner === null) return null;
          for (const k of inner) out.add(k);
        }
        return out;
      }
      default:
        return new Set();
    }
  }

  // `mode`: undefined for `js::JSON` (its `JsonError`), "pos" for `serde_json::from_str` (a
  // `serde_json::Error` at a line and column), "nopos" for `serde_json::from_value`.
  rt.$jsonParse = (text, schema, mode) => {
    try {
      return rt.Ok(jsonIn(jsonLex(text), schema));
    } catch (e) {
      if (!(e instanceof JsonError)) throw e;
      if (mode === undefined) return rt.Err(e);
      let line = 0;
      let col = 0;
      if (mode === "pos" && !e.noPos) {
        const before = text.slice(0, e.at ?? text.length);
        const nl = before.lastIndexOf("\n");
        line = before.split("\n").length;
        col = utf8Len(before.slice(nl + 1));
      }
      return rt.Err(new SerdeJsonError(e.msg, line, col, e.kind ?? "data"));
    }
  };

  /// `serde_json::Error`: the message, and where in the text (`{}` adds "at line L column C").
  class SerdeJsonError extends JsonError {
    constructor(msg, line, col, kind) {
      super(msg);
      this.ln = line;
      this.col = col;
      this.kind = kind;
    }
    toString() {
      return this.ln === 0 ? this.msg : `${this.msg} at line ${this.ln} column ${this.col}`;
    }
    $dbg() {
      return `Error(${rt.$dbgStr(this.msg)}, line: ${this.ln}, column: ${this.col})`;
    }
  }
  rt.SerdeJsonError = SerdeJsonError;
  rt.$serde_json = {
    line: (e) => e.ln,
    column: (e) => e.col,
    isEof: (e) => e.kind === "eof",
    isSyntax: (e) => e.kind === "syntax",
    isData: (e) => e.kind === "data",
    isIo: (e) => e.kind === "io",
    toString: (e) => e.toString(),
  };
  // `serde_json::to_writer`: the writer's `io::Result<()>`, its error as a `serde_json::Error`.
  const sjIoError = (e) =>
    new SerdeJsonError(typeof e?.fmt === "function" ? rt.$display(e) : String(e), 0, 0, "io");
  rt.$sjIo = (r) => (r.$tag === 0 ? r : rt.Err(sjIoError(r[0])));
  // `from_slice(&bytes)`: the bytes as UTF-8 text.
  rt.$sjFromBytes = (bytes, schema) => {
    let text;
    try {
      text = new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(bytes));
    } catch {
      return rt.Err(new SerdeJsonError("invalid unicode code point", 0, 0, "syntax"));
    }
    return rt.$jsonParse(text, schema, "pos");
  };
  // `from_reader(r)`: everything `read(b)` (its `read_to_end`) appends to `b`.
  rt.$sjFromReader = (read, schema) => {
    const b = [];
    const r = read(b);
    return r.$tag === 0 ? rt.$sjFromBytes(b, schema) : rt.Err(sjIoError(r[0]));
  };

  // ---- the bundled `regex`: Rust's syntax as a JS `RegExp` (the `u` flag; `\d` and `\w` as
  // Unicode, as Rust has them), with byte offsets in the haystack, as Rust has them.
  const regexCache = new Map();
  const POSIX = {
    alnum: "0-9A-Za-z",
    alpha: "A-Za-z",
    ascii: "\\x00-\\x7F",
    blank: "\\t ",
    cntrl: "\\x00-\\x1F\\x7F",
    digit: "0-9",
    graph: "!-~",
    lower: "a-z",
    print: " -~",
    punct: "!-\\/:-@\\[-`{-~",
    space: "\\t\\n\\v\\f\\r ",
    upper: "A-Z",
    word: "0-9A-Za-z_",
    xdigit: "0-9A-Fa-f",
  };
  const WORD = "\\p{Alphabetic}\\p{M}\\p{Nd}\\p{Pc}\\p{Join_Control}";
  const regexFail = (pat, msg) => {
    throw `regex parse error:\n    ${pat}\nerror: ${msg}`;
  };
  // `\p{Greek}` is a script in Rust (`Script=Greek` in JS); categories stay.
  const uniProp = (name) =>
    /^(L[ultmo]?|M[nce]?|N[dlo]?|P[cdseifo]?|S[mcko]?|Z[slp]?|C[cfson]?|Letter|Number|Alphabetic|White_Space|Uppercase|Lowercase|Emoji|Any|ASCII|Script=.*|sc=.*|gc=.*)$/.test(name)
      ? name
      : `Script=${name}`;
  function regexCompile(pat) {
    let hit = regexCache.get(pat);
    if (hit) return hit;
    let i = 0;
    let flags = "";
    // Leading flags: `(?i)`, `(?ms)`.
    const lead = /^\(\?([imsxU]+)\)/.exec(pat);
    if (lead) {
      for (const f of lead[1]) {
        if (f === "x" || f === "U") regexFail(pat, `the flag \`${f}\` is not supported here`);
        flags += f;
      }
      i = lead[0].length;
    }
    let out = "";
    let inClass = 0;
    const names = [""];
    while (i < pat.length) {
      const c = pat[i];
      if (c === "\\") {
        const e = pat[i + 1];
        if (e === undefined) regexFail(pat, "incomplete escape sequence, reached end of pattern prematurely");
        if (e === "d") out += "\\p{Nd}";
        else if (e === "D") out += "\\P{Nd}";
        else if (e === "w") out += inClass ? WORD : `[${WORD}]`;
        else if (e === "W") out += inClass ? "\\W" : `[^${WORD}]`;
        else if (e === "A") out += "^";
        else if (e === "z") out += "$";
        else if (e === "p" || e === "P") {
          if (pat[i + 2] === "{") {
            const end = pat.indexOf("}", i + 3);
            if (end < 0) regexFail(pat, "unclosed Unicode class");
            out += `\\${e}{${uniProp(pat.slice(i + 3, end))}}`;
            i = end + 1;
            continue;
          }
          out += `\\${e}{${uniProp(pat[i + 2])}}`;
          i += 3;
          continue;
        } else if (e === "x" && pat[i + 2] === "{") {
          const end = pat.indexOf("}", i + 3);
          out += `\\u{${pat.slice(i + 3, end)}}`;
          i = end + 1;
          continue;
        } else if (/[1-9]/.test(e)) regexFail(pat, "backreferences are not supported");
        else if ("tnrfvbBsSx0".includes(e)) out += c + e;
        else if (/[a-zA-Z0-9]/.test(e)) regexFail(pat, "unrecognized escape sequence");
        else if ("\\^$.|?*+()[]{}/-".includes(e)) out += c + e;
        // (Other punctuation Rust lets you escape is itself; JS's `u` mode rejects the escape.)
        else out += e;
        i += 2;
        continue;
      }
      if (inClass) {
        const posix = /^\[:(\^?)([a-z]+):\]/.exec(pat.slice(i));
        if (posix) {
          const set = POSIX[posix[2]];
          if (!set) regexFail(pat, "invalid ASCII class");
          if (posix[1]) regexFail(pat, "negated ASCII classes are not supported here");
          out += set;
          i += posix[0].length;
          continue;
        }
        if (c === "[") {
          // (A nested class, `[a[bc]]`: its members.)
          inClass++;
          i++;
          continue;
        }
        if (c === "]") {
          inClass--;
          if (inClass === 0) out += "]";
          i++;
          continue;
        }
        if (c === "&" && pat[i + 1] === "&") regexFail(pat, "class intersections are not supported here");
        out += c === "/" ? "\\/" : c;
        i++;
        continue;
      }
      if (c === "[") {
        inClass = 1;
        out += "[";
        i++;
        if (pat[i] === "^") {
          out += "^";
          i++;
        }
        // (A `]` first is a member.)
        if (pat[i] === "]") {
          out += "\\]";
          i++;
        }
        continue;
      }
      if (c === "(") {
        if (pat[i + 1] === "?") {
          const rest = pat.slice(i + 2);
          if (/^(=|!|<=|<!)/.test(rest)) regexFail(pat, "look-around, including look-ahead and look-behind, is not supported");
          const named = /^P?<([A-Za-z_][A-Za-z0-9_.\[\]]*)>/.exec(rest);
          if (named) {
            names.push(named[1]);
            out += `(?<${named[1]}>`;
            i += 2 + named[0].length;
            continue;
          }
          if (rest[0] === ":") {
            out += "(?:";
            i += 3;
            continue;
          }
          regexFail(pat, "inline flags are only supported at the start of the pattern here");
        }
        names.push("");
        out += "(";
        i++;
        continue;
      }
      out += c === "/" ? "\\/" : c;
      i++;
    }
    if (inClass) regexFail(pat, "unclosed character class");
    let re;
    try {
      re = new RegExp(out, `gdu${flags}`);
    } catch (e) {
      regexFail(pat, String(e?.message ?? e).replace(/^Invalid regular expression: /, ""));
    }
    hit = { re, names };
    if (regexCache.size > 256) regexCache.clear();
    regexCache.set(pat, hit);
    return hit;
  }
  // UTF-16 index <-> UTF-8 byte offset in `s`.
  const isAscii = (s) => /^[\x00-\x7F]*$/.test(s);
  function u16ToByte(s, k) {
    return isAscii(s) ? k : utf8Len(s.slice(0, k));
  }
  function byteToU16(s, b) {
    if (isAscii(s)) return b;
    let n = 0;
    let k = 0;
    while (k < s.length && n < b) {
      const c = s.codePointAt(k);
      n += c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
      k += c >= 0x10000 ? 2 : 1;
    }
    return k;
  }
  // The bundled `react` crate and `jsx!`, over the page's React (`globalThis.React`, which
  // Bitburner provides).
  const reactOf = () => {
    const R = globalThis.React;
    if (!R) {
      throw new CarciniPanic(
        "`jsx!` needs React as `globalThis.React`: Bitburner provides it; elsewhere load React 17 (a <script> of its UMD build) before running the script",
        "<react>",
      );
    }
    return R;
  };
  // A child as React takes it: Rust's numbers as text (`i128`s are BigInts), `None` as nothing,
  // `Vec`s as arrays.
  const reactChild = (v) => {
    if (v == null || typeof v === "string") return v ?? null;
    if (typeof v === "number" || typeof v === "bigint") return String(v);
    if (Array.isArray(v)) return v.map(reactChild);
    if (typeof v === "object" && v.$$typeof) return v;
    throw new CarciniPanic(
      `a \`jsx!\` child must be a node, text, a number, or an Option / Vec of them, not ${typeof v === "object" ? (v.constructor?.name ?? "an object") : typeof v}`,
      "<react>",
    );
  };
  // `"color: red; font-size: 12px"` as `{color: "red", fontSize: "12px"}`.
  const reactStyle = (css) => {
    const out = {};
    for (const decl of String(css).split(";")) {
      const at = decl.indexOf(":");
      if (at < 0) continue;
      const name = decl.slice(0, at).trim();
      if (!name) continue;
      const key = name.startsWith("--") ? name : name.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
      out[key] = decl.slice(at + 1).trim();
    }
    return out;
  };
  // A `{..spread}` or a `style`'s entries: a `Vec` of pairs, a `HashMap`, or an object.
  const reactPairs = (v) => {
    if (v == null) return [];
    if (Array.isArray(v) || (typeof v === "object" && typeof v[Symbol.iterator] === "function")) {
      return [...v];
    }
    return Object.entries(v);
  };
  const reactStyleOf = (v) => {
    if (typeof v === "string") return reactStyle(v);
    const out = {};
    for (const [k, x] of reactPairs(v)) {
      if (x == null) continue;
      const key = k.startsWith("--") || !k.includes("-") ? k : k.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
      out[key] = typeof x === "bigint" ? String(x) : x;
    }
    return out;
  };
  const reactDeps = (deps) => (deps === undefined || deps === null ? [] : Array.isArray(deps) ? deps : [deps]);
  const reactComponents = new WeakMap();
  rt.$react = {
    // `React.createElement(tag, props, ...children)`; `props` is a list of `[name, value]`
    // (a `None` value leaves the prop out).
    h(tag, props, children) {
      const p = {};
      const put = (k, v) => {
        if (v == null) return;
        p[k] = k === "style" ? reactStyleOf(v) : typeof v === "bigint" ? String(v) : v;
      };
      for (const [k, v] of props ?? []) {
        // `{..spread}`: a list of pairs, a map, or an object.
        if (k === "...") for (const [k2, v2] of reactPairs(v)) put(k2, v2);
        else put(k, v);
      }
      const R = reactOf();
      return R.createElement(tag === "" ? R.Fragment : tag, p, ...(children ?? []).map(reactChild));
    },
    node: reactChild,
    // (Like the children of a JSX element: written out, not a list, so they need no keys.)
    static(nodes) {
      for (const n of nodes) if (n && typeof n === "object" && n._store) n._store.validated = true;
      return nodes;
    },
    any: (v) => v,
    style: reactStyle,
    // A Rust fn or closure as a component: called with its props' values in `names` order. One
    // wrapper per function, so React keeps the component's identity (and state) across renders.
    component(f, names) {
      let w = reactComponents.get(f);
      if (!w) {
        w = (props) => {
          const r = f(...names.map((n) => props[n]));
          return reactChild(r);
        };
        w.displayName = f.name || "component";
        reactComponents.set(f, w);
      }
      return w;
    },
    useState: (init) => reactOf().useState(init),
    // `()` deps: once, after the first render. A cleanup closure is returned as is.
    useEffect(f, deps) {
      reactOf().useEffect(() => {
        const r = f();
        return typeof r === "function" ? r : undefined;
      }, reactDeps(deps));
    },
    useRef: (init) => reactOf().useRef(init),
    useMemo: (f, deps) => reactOf().useMemo(f, reactDeps(deps)),
    useCallback: (f, deps) => reactOf().useCallback(f, reactDeps(deps)),
    // The reducer gets a copy of the state: a Rust reducer may change it in place and return it,
    // and React only renders again for a new object.
    useReducer: (reducer, init) => reactOf().useReducer((s, a) => reducer(rt.$clone(s), a), init),
    createContext: (dflt) => reactOf().createContext(dflt),
    useContext: (ctx) => reactOf().useContext(ctx),
    provide(ctx, key, value, children) {
      const p = { value };
      if (key != null) p.key = typeof key === "bigint" ? String(key) : key;
      return reactOf().createElement(ctx.Provider, p, ...(children ?? []).map(reactChild));
    },
    call: (f, a) => f(a),
    // `set(v)`: a closure `v` is the new state, not an updater.
    setState: (f, v) => f(typeof v === "function" ? () => v : v),
    // `update(|old| new)`: the updater gets a copy, which it may change in place and return.
    updateState: (f, g) => f((old) => g(rt.$clone(old))),
    get: (r) => r.current,
    set(r, v) {
      r.current = v;
    },
  };
  rt.$regex = {
    // "" for a pattern that compiles, else the error.
    check(pat) {
      try {
        regexCompile(pat);
        return "";
      } catch (e) {
        if (typeof e === "string") return e;
        throw e;
      }
    },
    // The group names ("" unnamed), the whole match's first.
    names: (pat) => regexCompile(pat).names.slice(),
    // The first match at or after byte `start`: start and end bytes of each group (-1 if it
    // didn't take part), or nothing.
    find(pat, hay, start) {
      const { re } = regexCompile(pat);
      re.lastIndex = byteToU16(hay, start);
      const m = re.exec(hay);
      if (!m) return [];
      const out = [];
      for (const span of m.indices) {
        if (span === undefined) out.push(-1, -1);
        else out.push(u16ToByte(hay, span[0]), u16ToByte(hay, span[1]));
      }
      return out;
    },
    // The bytes of the character at byte `at` (to step past an empty match).
    charLen(hay, at) {
      const c = hay.codePointAt(byteToU16(hay, at));
      return c === undefined ? 1 : c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
    },
  };

  globalThis.κσ = rt;

  // Proc macros: the compiler calls this with a proc-macro crate compiled to JS, a function of
  // it, and the input (and an attribute macro's arguments); the result is the output's source.
  // Failures are thrown as strings, which the compiler reports at the invocation.
  const macroCrates = new Map();
  // Loop steps of the macro running now (`κσ.$tick()` at the top of each loop).
  let macroSteps = 0;
  rt.$tick = () => {
    if (++macroSteps > 10_000_000) throw "the proc macro ran too long (more than 10,000,000 loop steps)";
  };
  globalThis.__carciniRunMacro = (code, name, input, attr) => {
    macroSteps = 0;
    let crate = macroCrates.get(code);
    if (!crate) {
      // (`export { Error__file as Error };`: a renamed type's other name, for importers only.)
      const body = code.replace(/^export (?=(async )?function |class |const |let )/gm, "").replace(/^export \{[^}]*\};$/gm, "");
      const names = [...code.matchAll(/^export (?:async )?function ([\w$]+)/gm)].map((m) => m[1]);
      try {
        crate = new Function(`${body}\nreturn { ${names.join(", ")} };`)();
      } catch (e) {
        throw `the proc macro crate didn't load: ${e?.message ?? e}`;
      }
      if (macroCrates.size > 16) macroCrates.clear();
      macroCrates.set(code, crate);
    }
    const f = crate[name];
    if (typeof f !== "function") throw `the proc macro crate has no function \`${name}\``;
    let out;
    try {
      out = attr === undefined || attr === null ? f(input) : f(attr, input);
    } catch (e) {
      throw e instanceof CarciniPanic ? e.panicMessage : String(e?.message ?? e);
    }
    return String(out);
  };
})();
