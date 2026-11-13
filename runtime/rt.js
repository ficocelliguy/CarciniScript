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
    $collectString() {
      let s = "";
      for (const x of iterable(this.it)) s += x;
      return s;
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
    $asciiUpper(c) {
      return c >= "a" && c <= "z" ? c.toUpperCase() : c;
    },
    $asciiLower(c) {
      return c >= "A" && c <= "Z" ? c.toLowerCase() : c;
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
      return Iter.of(function* () {
        if (hi === null) for (let i = lo; ; i++) yield i;
        const end = inclusive ? hi + 1 : hi;
        for (let i = lo; i < end; i++) yield i;
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
    $fixed(x, p) {
      if (!Number.isFinite(x)) return fmtF(x);
      return expandExponent(x.toFixed(p));
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
      if (x === undefined) return "()";
      return String(x);
    },
    $dbg(x) {
      if (typeof x === "number") return dbgF(x);
      if (typeof x === "string") return rt.$dbgStr(x);
      if (x === undefined) return "()";
      return String(x);
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
        const sign = s[0] === "-" || s[0] === "+" ? s[0] : "";
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
