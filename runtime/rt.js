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
    if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) return false;
      for (let i = 0; i < a.length; i++) if (!eq(a[i], b[i])) return false;
      return true;
    }
    return a === b;
  }

  // Rust's `Ord`: numbers, bools, strings and (lexicographically) Vecs.
  function cmp(a, b) {
    if (Array.isArray(a) && Array.isArray(b)) {
      const n = Math.min(a.length, b.length);
      for (let i = 0; i < n; i++) {
        const c = cmp(a[i], b[i]);
        if (c !== 0) return c;
      }
      return a.length - b.length;
    }
    return a < b ? -1 : a > b ? 1 : 0;
  }

  function clone(v) {
    return Array.isArray(v) ? v.map(clone) : v;
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
