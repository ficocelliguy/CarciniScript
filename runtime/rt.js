// CarciniScript runtime. Installs `globalThis.κσ` once. Works as a classic script (Bitburner
// injects it like RapydScript's baselib) or as a side-effect ES module import.
//
// Every helper name starts with `$` so none collide with Netscript function names: Bitburner's
// RAM calculator charges for any identifier that matches an NS function, member properties included.
(function () {
  "use strict";
  if (globalThis.κσ) return;

  class CarciniPanic extends Error {
    constructor(message, location) {
      super(`panicked at ${location}:\n${message}`);
      this.name = "CarciniPanic";
      this.panicMessage = message;
      this.location = location;
    }
  }

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

  // Rust's `==` for the values CarciniScript represents: structural for Vecs (JS arrays).
  function eq(a, b) {
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

  // Rust's `Ord`: numbers, bools, strings and (lexicographically) Vecs.
  function cmp(a, b) {
    // `None` sorts before every `Some`.
    if (a == null || b == null) return a == null ? (b == null ? 0 : -1) : 1;
    if (typeof a.$cmp === "function") return a.$cmp(b);
    if (Array.isArray(a) && Array.isArray(b)) {
      const n = Math.min(a.length, b.length);
      for (let i = 0; i < n; i++) {
        const c = cmp(a[i], b[i]);
        if (c !== 0) return c;
      }
      return Math.sign(a.length - b.length);
    }
    return a < b ? -1 : a > b ? 1 : 0;
  }

  function clone(v) {
    if (v instanceof HashSet || v instanceof HashMap) return v.$clone();
    if (v != null && typeof v.$clone === "function") return v.$clone();
    return Array.isArray(v) ? v.map(clone) : v;
  }

  // Map key for a HashSet element: primitives as themselves, composite values (tuples, Vecs)
  // structurally. A set holds one element type, so the two kinds never mix.
  function key(x) {
    return typeof x === "object" && x !== null ? JSON.stringify(x) : x;
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
        return old;
      }
      this.m.set(kk, [k, v]);
      return null;
    }
    $get(k) {
      const slot = this.m.get(key(k));
      return slot ? slot[1] : null;
    }
    $containsKey(k) {
      return this.m.has(key(k));
    }
    $remove(k) {
      const kk = key(k);
      const slot = this.m.get(kk);
      if (!slot) return null;
      this.m.delete(kk);
      return slot[1];
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
      const src = this.it;
      return Iter.of(function* () {
        for (const x of iterable(src)) yield f(x);
      });
    }
    $filter(p) {
      const src = this.it;
      return Iter.of(function* () {
        for (const x of iterable(src)) if (p(x)) yield x;
      });
    }
    $filterMap(f) {
      const src = this.it;
      return Iter.of(function* () {
        for (const x of iterable(src)) {
          const y = f(x);
          if (y != null) yield y;
        }
      });
    }
    $flatMap(f) {
      const src = this.it;
      return Iter.of(function* () {
        for (const x of iterable(src)) yield* items(f(x));
      });
    }
    $flatten() {
      const src = this.it;
      return Iter.of(function* () {
        for (const x of iterable(src)) yield* items(x);
      });
    }
    $enumerate() {
      const src = this.it;
      return Iter.of(function* () {
        let i = 0;
        for (const x of iterable(src)) yield [i++, x];
      });
    }
    $zip(other) {
      const src = this.it;
      return Iter.of(function* () {
        const b = items(other)[Symbol.iterator]();
        for (const x of iterable(src)) {
          const y = b.next();
          if (y.done) return;
          yield [x, y.value];
        }
      });
    }
    $chain(other) {
      const src = this.it;
      return Iter.of(function* () {
        yield* iterable(src);
        yield* items(other);
      });
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
    // Collects first: JS iterators only go forward.
    $rev() {
      const all = Array.from(iterable(this.it));
      return Iter.of(function* () {
        for (let i = all.length - 1; i >= 0; i--) yield all[i];
      });
    }
    $take(n) {
      const src = this.it;
      return Iter.of(function* () {
        if (n <= 0) return;
        let i = 0;
        for (const x of iterable(src)) {
          yield x;
          if (++i >= n) return;
        }
      });
    }
    $skip(n) {
      const src = this.it;
      return Iter.of(function* () {
        let i = 0;
        for (const x of iterable(src)) if (i++ >= n) yield x;
      });
    }
    $stepBy(n) {
      if (n === 0) throw new CarciniPanic("assertion failed: step != 0", "<std>");
      const src = this.it;
      return Iter.of(function* () {
        let i = 0;
        for (const x of iterable(src)) if (i++ % n === 0) yield x;
      });
    }
    $takeWhile(p) {
      const src = this.it;
      return Iter.of(function* () {
        for (const x of iterable(src)) {
          if (!p(x)) return;
          yield x;
        }
      });
    }
    $skipWhile(p) {
      const src = this.it;
      return Iter.of(function* () {
        let skipping = true;
        for (const x of iterable(src)) {
          if (skipping && p(x)) continue;
          skipping = false;
          yield x;
        }
      });
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
      return r.done ? null : r.value;
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
    $count() {
      let n = 0;
      for (const _ of iterable(this.it)) n++;
      return n;
    }
    $last() {
      let last = null;
      for (const x of iterable(this.it)) last = x;
      return last;
    }
    $nth(n) {
      let i = 0;
      for (const x of iterable(this.it)) if (i++ === n) return x;
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
      return best;
    }
    $maxBy(f) {
      let best = null, first = true;
      for (const x of iterable(this.it)) {
        if (first || f(x, best) >= 0) best = x;
        first = false;
      }
      return best;
    }
    $minByKey(f) {
      let best = null, bestKey, first = true;
      for (const x of iterable(this.it)) {
        const k = f(x);
        if (first || cmp(k, bestKey) < 0) [best, bestKey] = [x, k];
        first = false;
      }
      return best;
    }
    $maxByKey(f) {
      let best = null, bestKey, first = true;
      for (const x of iterable(this.it)) {
        const k = f(x);
        if (first || cmp(k, bestKey) >= 0) [best, bestKey] = [x, k];
        first = false;
      }
      return best;
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
      for (const x of iterable(this.it)) if (p(x)) return x;
      return null;
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
      return acc;
    }
    $forEach(f) {
      for (const x of iterable(this.it)) f(x);
    }
    $collectVec() {
      return Array.from(iterable(this.it));
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

  // A JS iterator as something `for...of` accepts without restarting it.
  function iterable(it) {
    return { [Symbol.iterator]: () => it };
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
    "Utf8Error:Invalid": "invalid utf-8 sequence",
    "ParseFloatError:Empty": "cannot parse float from empty string",
    "ParseFloatError:Invalid": "invalid float literal",
    "ParseBoolError:": "provided string was not `true` or `false`",
    "ParseCharError:EmptyString": "cannot parse char from empty string",
    "ParseCharError:TooManyChars": "too many characters in string",
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
      return this.kind ? `${this.type} { kind: ${this.kind} }` : this.type;
    }
    toString() {
      return PARSE_MESSAGES[`${this.type}:${this.kind}`];
    }
  }

  // `RefCell<T>` / `Cell<T>`: the value at `.v`. No `$clone`: cloning an `Rc<RefCell<T>>` shares it.
  class RefCell {
    constructor(v, kind) {
      this.v = v;
      this.kind = kind;
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

  // Rust's BTreeSet / BTreeMap: the hash classes, re-sorted by key whenever they are read in order.
  // `n` is the size when the map was last sorted: a new key (from `$insert` or an `entry` slot)
  // grows the map past it. Removals keep the order, so they sort first and then record the size.
  function btreeSorting(Base, keyOf) {
    return class extends Base {
      constructor() {
        super();
        this.n = 0;
      }
      $sort() {
        if (this.m.size === this.n) return;
        this.m = new Map([...this.m].sort((a, b) => cmp(keyOf(a[1]), keyOf(b[1]))));
        this.n = this.m.size;
      }
      $synced(f) {
        this.$sort();
        const r = f();
        this.n = this.m.size;
        return r;
      }
      $remove(x) {
        return this.$synced(() => super.$remove(x));
      }
      $retain(p) {
        this.$synced(() => super.$retain(p));
      }
      $clear() {
        this.$synced(() => super.$clear());
      }
      $clone() {
        this.$sort();
        const c = new this.constructor();
        c.m = super.$clone().m;
        c.n = c.m.size;
        return c;
      }
      $append(other) {
        this.$extend(Array.from(other));
        other.$clear();
      }
      // The first / last entry, as stored (an element, or a `[k, v]` slot).
      $firstSlot() {
        this.$sort();
        for (const x of this.m.values()) return x;
        return null;
      }
      $lastSlot() {
        this.$sort();
        let last = null;
        for (const x of this.m.values()) last = x;
        return last;
      }
      $popSlot(first) {
        const x = first ? this.$firstSlot() : this.$lastSlot();
        if (x === null) return null;
        this.$synced(() => this.m.delete(key(keyOf(x))));
        return x;
      }
    };
  }

  class BTreeSet extends btreeSorting(HashSet, (x) => x) {
    static from(items) {
      const s = new BTreeSet();
      for (const x of items) s.$insert(x);
      return s;
    }
    [Symbol.iterator]() {
      this.$sort();
      return super[Symbol.iterator]();
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

  class BTreeMap extends btreeSorting(HashMap, (slot) => slot[0]) {
    static from(pairs) {
      const h = new BTreeMap();
      for (const [k, v] of pairs) h.$insert(k, v);
      return h;
    }
    $slots() {
      this.$sort();
      return super.$slots();
    }
    $entries() {
      this.$sort();
      return super.$entries();
    }
    $keys() {
      this.$sort();
      return super.$keys();
    }
    $values() {
      this.$sort();
      return super.$values();
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

  const extRegs = {};

  const rt = {
    CarciniPanic,

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
      return v[rt.$ix(v, i, location)];
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
      const x = v[rt.$ix(v, a, location)];
      v[a] = v[rt.$ix(v, b, location)];
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
    $pop(v) {
      return v.length ? v.pop() : null;
    },
    $first(v) {
      return v.length ? v[0] : null;
    },
    $last(v) {
      return v.length ? v[v.length - 1] : null;
    },
    $vget(v, i) {
      return i >= 0 && i < v.length ? v[i] : null;
    },
    $eq: eq,
    HashSet,
    HashMap,
    BTreeSet,
    BTreeMap,
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
      const r = BigInt(a) << BigInt(n & 63);
      return Number(signed ? BigInt.asIntN(64, r) : BigInt.asUintN(64, r));
    },
    $shr64(a, n) {
      if (a >= 0 && a <= 0x7fffffff) return (n & 63) > 31 ? 0 : a >> (n & 63);
      return Number(BigInt(a) >> BigInt(n & 63));
    },
    $not64(a, signed) {
      return signed ? -a - 1 : Number(BigInt.asUintN(64, ~BigInt(a)));
    },
    // A value of a type with `impl Iterator`, as a runtime iterator (for `take`, `map`, ..).
    $iterOf(o) {
      return new Iter(o[Symbol.iterator]());
    },
    // `fn main() -> Result<..>` returned `Err(e)`: fail with rustc's `Error: {e:?}`.
    $mainResult(r, fmt) {
      if (r != null && r.$tag === 1) {
        const err = new Error(`Error: ${fmt(r[0])}`);
        err.name = "CarciniError";
        throw err;
      }
    },
    // User trait impls for built-in types (`impl Money for f64`), by JS kind, for calls through
    // a type parameter or `dyn Trait` (`$dcall`).
    $extReg(kind, cls, integral) {
      (extRegs[kind] ??= []).push({ cls, integral });
    },
    $dcall(x, name, ...args) {
      if (x !== null && typeof x === "object" && !Array.isArray(x) && typeof x[name] === "function") {
        return x[name](...args);
      }
      const kind = Array.isArray(x) ? "array" : typeof x;
      let pick = null;
      for (const r of extRegs[kind] ?? []) {
        if (typeof r.cls.prototype[name] !== "function") continue;
        pick ??= r;
        // A number goes to an integer type's impl when it's whole, else to a float type's.
        if (kind === "number" && r.integral === Number.isInteger(x)) {
          pick = r;
          break;
        }
      }
      if (pick) return pick.cls.prototype[name].call(x, ...args);
      return x[name](...args);
    },
    // `i128` / `u128` are BigInts, wrapped to 128 bits like Rust's release builds.
    $w128(x, signed) {
      return signed ? BigInt.asIntN(128, x) : BigInt.asUintN(128, x);
    },
    $div128(a, b) {
      if (b === 0n) throw new CarciniPanic("attempt to divide by zero", "<std>");
      return a / b;
    },
    $rem128(a, b) {
      if (b === 0n) throw new CarciniPanic("attempt to calculate the remainder with a divisor of zero", "<std>");
      return a % b;
    },
    $abs128(x) {
      return x < 0n ? -x : x;
    },
    // `x as i128`: from a number (truncated) or another BigInt.
    $to128(x, signed) {
      const b = typeof x === "bigint" ? x : BigInt(Math.trunc(x));
      return signed ? BigInt.asIntN(128, b) : BigInt.asUintN(128, b);
    },
    // `x as u32` from an `i128`: wrapped to the target's bits, as a number.
    $from128(x, bits, signed) {
      return Number(signed ? BigInt.asIntN(bits, x) : BigInt.asUintN(bits, x));
    },
    $parseInt128(s, signed) {
      const err = (kind) => new Res(1, new ParseError("ParseIntError", kind));
      if (s === "") return err("Empty");
      if (!/^[+-]?[0-9]+$/.test(s) || s === "+" || s === "-") return err("InvalidDigit");
      if (!signed && s[0] === "-") return err("InvalidDigit");
      const v = BigInt(s);
      const max = signed ? (1n << 127n) - 1n : (1n << 128n) - 1n;
      const min = signed ? -(1n << 127n) : 0n;
      if (v > max) return err("PosOverflow");
      if (v < min) return err("NegOverflow");
      return new Res(0, v);
    },
    $dqRemove(a, i) {
      return i < a.length ? a.splice(i, 1)[0] : null;
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
      } else if (modify) {
        modify(slot);
      }
      return slot;
    },
    // A place handed out by `index_mut`: `[0]` reads and writes it.
    $cell(get, set) {
      return {
        get 0() {
          return get();
        },
        set 0(v) {
          set(v);
        },
      };
    },
    // `map.get_mut(k)` of numbers, strings or bools: the `[k, v]` slot, or null.
    $slotOf(map, k) {
      return map.m.get(key(k)) ?? null;
    },
    // `v.get_mut(i)` / `first_mut` (0) / `last_mut` (-1) of numbers, strings or bools: a slot whose
    // `[1]` reads and writes the element, or null.
    $vecSlot(a, i) {
      const j = i < 0 ? a.length + i : i;
      if (j < 0 || j >= a.length) return null;
      return {
        get 1() {
          return a[j];
        },
        set 1(v) {
          a[j] = v;
        },
      };
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
      return x;
    },
    $expect(x, message, location) {
      if (x == null) throw new CarciniPanic(message, location);
      return x;
    },
    $cmp: cmp,
    $clone: clone,
    // `partial_cmp`: `None` when either side is NaN.
    $partialCmp(a, b) {
      if (Number.isNaN(a) || Number.isNaN(b)) return null;
      return cmp(a, b);
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
      return x == null ? null : f(x);
    },
    $optFilter(x, p) {
      return x != null && p(x) ? x : null;
    },
    $optOrElse(x, f) {
      return x == null ? f() : x;
    },
    $optMapOr(x, d, f) {
      return x == null ? d : f(x);
    },
    $optMapOrElse(x, d, f) {
      return x == null ? d() : f(x);
    },
    $optIsSomeAnd(x, p) {
      return x != null && p(x);
    },
    $optIsNoneOr(x, p) {
      return x == null || p(x);
    },
    $optOr(x, y) {
      return x == null ? y : x;
    },
    $optZip(x, y) {
      return x == null || y == null ? null : [x, y];
    },
    $optInspect(x, f) {
      if (x != null) f(x);
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
      return new Iter((x == null ? [] : [x])[Symbol.iterator]());
    },

    // Iterator sources.
    Iter,
    $iter(x) {
      return new Iter(x[Symbol.iterator]());
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
      const one = typeof lo === "bigint" ? 1n : 1;
      return Iter.of(function* () {
        if (hi === null) for (let i = lo; ; i += one) yield i;
        const end = inclusive ? hi + one : hi;
        for (let i = lo; i < end; i += one) yield i;
      });
    },
    $split(s, p) {
      return new Iter(s.split(p)[Symbol.iterator]());
    },
    $splitWhitespace(s) {
      return new Iter(s.split(/\s+/).filter((w) => w !== "")[Symbol.iterator]());
    },
    $lines(s) {
      return new Iter(lines(s)[Symbol.iterator]());
    },
    $windows(v, n) {
      if (n === 0) throw new CarciniPanic("window size must be non-zero", "<std>");
      return Iter.of(function* () {
        for (let i = 0; i + n <= v.length; i++) yield v.slice(i, i + n);
      });
    },
    $chunksExact(v, n) {
      if (n === 0) throw new CarciniPanic("chunk size must be non-zero", "<std>");
      return Iter.of(function* () {
        for (let i = 0; i + n <= v.length; i += n) yield v.slice(i, i + n);
      });
    },
    // `s.split_at(mid)`, with slicing's panics.
    $splitAt(s, mid, location) {
      return [rt.$strSlice(s, 0, mid, false, location), rt.$strSlice(s, mid, null, false, location)];
    },
    $chunks(v, n) {
      if (n === 0) throw new CarciniPanic("chunk size must be non-zero", "<std>");
      return Iter.of(function* () {
        for (let i = 0; i < v.length; i += n) yield v.slice(i, i + n);
      });
    },

    // Vec methods taking closures.
    $forEachIndex(v, f) {
      for (let i = 0; i < v.length; i++) f(i);
    },
    $sortByKey(v, f) {
      const keyed = v.map((x) => [f(x), x]);
      keyed.sort((a, b) => cmp(a[0], b[0]));
      for (let i = 0; i < v.length; i++) v[i] = keyed[i][1];
    },
    $retain(v, p) {
      let j = 0;
      for (let i = 0; i < v.length; i++) if (p(v[i])) v[j++] = v[i];
      v.length = j;
    },
    $dedup(v) {
      rt.$dedupByKey(v, (x) => x);
    },
    $dedupByKey(v, f) {
      if (v.length === 0) return;
      let j = 1;
      let last = f(v[0]);
      for (let i = 1; i < v.length; i++) {
        const k = f(v[i]);
        if (!eq(k, last)) {
          v[j++] = v[i];
          last = k;
        }
      }
      v.length = j;
    },

    // Strings.
    $utf8len(s) {
      let n = 0;
      for (const c of s) {
        const cp = c.codePointAt(0);
        n += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
      }
      return n;
    },
    $truncChars(s, n) {
      return Array.from(s).slice(0, n).join("");
    },

    // Formatting.
    $fmtF: fmtF,
    $dbgF: dbgF,
    // `{:.p}`: like `toFixed`, except that an exact tie rounds to even, as Rust does
    // (`format!("{:.0}", 2.5)` is "2"; JS's `toFixed` rounds ties up).
    $fixed(x, p) {
      if (!Number.isFinite(x)) return fmtF(x);
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
      const s = (p == null ? x.toExponential() : x.toExponential(p)).replace("e+", "e");
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
      return x == null ? "None" : `Some(${fmt(x)})`;
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
    // `*r = v` for a `&mut` struct or enum: the object keeps its identity, its contents change.
    $replace(target, value) {
      if (target === value) return;
      for (const k of Object.keys(target)) delete target[k];
      Object.assign(target, value);
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
    RefCell,
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
    $fromStrRadix(s, radix, min, max) {
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
      return new Res(0, Number(v));
    },
    // `s.as_bytes()`: UTF-8.
    $utf8(s) {
      return Array.from(new TextEncoder().encode(s));
    },
    $fromUtf8(bytes) {
      try {
        return new Res(0, new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(bytes)));
      } catch {
        return new Res(1, new ParseError("Utf8Error", "Invalid"));
      }
    },
    $fromUtf8Lossy(bytes) {
      return new TextDecoder("utf-8").decode(Uint8Array.from(bytes));
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
      const len = end - lo;
      const index = (k) => (typeof k === "string" && /^(0|[1-9]\d*)$/.test(k) ? +k : -1);
      return new Proxy([], {
        get(_, k) {
          if (k === "length") return len;
          const i = index(k);
          if (i !== -1) return i < len ? v[lo + i] : undefined;
          return Array.prototype[k];
        },
        set(_, k, x) {
          const i = index(k);
          if (i !== -1 && i < len) v[lo + i] = x;
          return true;
        },
        has(_, k) {
          const i = index(k);
          return i !== -1 ? i < len : k in Array.prototype;
        },
        deleteProperty() {
          return true;
        },
        ownKeys() {
          return [...Array(len).keys()].map(String).concat("length");
        },
        getOwnPropertyDescriptor(_, k) {
          if (k === "length") return { value: len, writable: true, enumerable: false, configurable: false };
          const i = index(k);
          if (i !== -1 && i < len) return { value: v[lo + i], writable: true, enumerable: true, configurable: true };
          return undefined;
        },
      });
    },
    // `v.split_at_mut(mid)`: views of both halves.
    $splitAtMut(v, mid, location) {
      if (mid > v.length) throw new CarciniPanic("mid > len", location);
      return [this.$sliceMut(v, 0, mid, false, location), this.$sliceMut(v, mid, null, false, location)];
    },
    // `&s[lo..hi]` with UTF-8 byte offsets, panicking like `str` indexing does.
    $strSlice(s, lo, hi, inclusive, location) {
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
      return at < 0 ? null : rt.$utf8len(s.slice(0, at));
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
      return p.length <= v.length && p.every((x, i) => eq(x, v[i]));
    },
    $vecEndsWith(v, p) {
      const off = v.length - p.length;
      return off >= 0 && p.every((x, i) => eq(x, v[off + i]));
    },
    $vecRepeatAll(v, n) {
      const out = [];
      for (let i = 0; i < n; i++) for (const x of v) out.push(clone(x));
      return out;
    },
    $isSorted(v) {
      for (let i = 1; i < v.length; i++) if (cmp(v[i - 1], v[i]) > 0) return false;
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
      return v.splice(at);
    },
    $append(v, other) {
      for (const x of other) v.push(x);
      other.length = 0;
    },
    $fill(v, x) {
      for (let i = 0; i < v.length; i++) v[i] = clone(x);
    },
    $copyFromSlice(v, src, location) {
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
      const moved = v.splice(0, n);
      for (const x of moved) v.push(x);
    },
    $swapRemove(v, i, location) {
      if (i >= v.length) {
        throw new CarciniPanic(`swap_remove index (is ${i}) should be < len (is ${v.length})`, location);
      }
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
    // A trait's default methods, added to an implementing class unless it defines its own.
    $mixin(cls, defaults) {
      for (const k of Object.getOwnPropertyNames(defaults)) {
        if (!Object.prototype.hasOwnProperty.call(cls.prototype, k)) {
          Object.defineProperty(cls.prototype, k, Object.getOwnPropertyDescriptor(defaults, k));
        }
      }
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

  globalThis.κσ = rt;
})();
