# CarciniScript: Rust compatibility gaps

Places where CarciniScript silently behaves differently from rustc, or accepts programs rustc
would reject. The counterpart to RapydScript-NS's `PYTHON_GAPS.md`. Unsupported features (which
produce a compile error) are tracked in [FEATURES.md](FEATURES.md) instead.

## Integers

- **Overflow wraps.** Arithmetic follows rustc's release-mode semantics (two's-complement
  wrapping), not debug-mode panics. An opt-in checked mode is planned for phase 4.
- **64-bit integers are JS numbers.** `i64`, `u64`, `isize` and `usize` are exact only up to 2^53.
  They don't wrap at 2^64, and casting a negative value to an unsigned 64-bit type keeps the sign.
  Literals above 2^53 produce a warning.
- **Unconstrained types aren't errors.** rustc reports "type annotations needed" for a value
  whose type nothing determines (e.g. an empty `Vec::new()` that's never used). CarciniScript
  treats it as untyped instead.

## Vec, arrays and slices

- **Fixed-size arrays are growable.** `[T; N]` and `&[T]` are the same JS array as `Vec<T>`, so
  the length isn't fixed by the type.
- **`for x in &mut v` needs a stable name for `v`.** The loop variable aliases `v[i]`. When `v` is
  a more complex expression, it's evaluated once into a temporary first.
- **Index panics on underflowed `usize`s show a negative index.** `usize` arithmetic doesn't wrap
  (see 64-bit integers above), so `v[i - 1]` with `i == 0` panics with "the index is -1" instead
  of rustc's 18446744073709551615.
- **String sort order** compares UTF-16 code units; Rust compares UTF-8 bytes. The orders differ
  only between characters above U+FFFF and those in U+E000–U+FFFF.

## Option and patterns

- **`None` is `null`.** `Option<Option<T>>` and `Option<()>` can't be represented (`Some(None)`
  would equal `None`), so they're rejected at compile time. `undefined` from JS reads as `None`.
- **`unwrap_or(default)` evaluates `default` lazily** (it lowers to `??`). Rust evaluates it
  eagerly, so the difference shows only if the default expression has side effects.
- **Exhaustiveness is approximate.** Catch-all arms, `true`/`false` and `None`/`Some(_)` are
  understood. Integer or char ranges that happen to cover a whole type still need a `_` arm, which
  rustc wouldn't require.
- **`ref mut` bindings to primitives don't write back.** `Some(ref mut n) => *n += 1` changes a
  copy. Write back through the original place (or use `take`/`replace`) instead.

## Strings

- **UTF-16 under the hood.** `len()` returns the UTF-8 byte length like Rust, but byte-offset
  slicing and indexing are not supported yet.

## Netscript values

- **NS numbers are `f64`.** TypeScript has one number type, so ports, threads and counts are
  `f64` too. Convert with `as u32` before matching on integer patterns or indexing.
- **Deliberate extensions over Rust for NS calls:**
  - Trailing optional parameters may be omitted (`ns.hack(target)`). When passed, they take the
    plain type, not `Option<T>`.
  - A `...rest` parameter takes any number of arguments (`ns.tprint(a, b, c)`).
  - An unsuffixed integer literal is accepted for an `f64` parameter (`ns.sleep(100)`). Integer
    *variables* still need `as f64`.
- **Some NS types are untyped (`JsValue`).** Unions of different types, generics, tuples,
  callbacks and inline object types have no Rust equivalent yet; values of those types are
  unchecked, as all NS values were before the bindings. An intersection like
  `Server & { isOnline?: boolean }` is typed as the named interface; its extra fields are dropped.
- **Without bindings** (`setNsBindings` not called), `NS` is entirely untyped and snake_case names
  map to camelCase.

## Checking

- **No borrow checker.** Borrows and lifetimes are parsed and erased. Programs rustc rejects for
  aliasing or lifetime reasons still run. See the borrow-checking section of [PLAN.md](PLAN.md).
- **No use-after-move or definite-initialization checks yet** (phase 1). A `let x;` local may be
  assigned more than once.
- **Type checking covers the supported subset only.** Within it, mismatches and mutability
  errors are reported like rustc's. NS calls are unchecked until typed bindings land, and trait
  bounds don't exist yet (only `sort` on floats is special-cased).

## Evaluation order

- A block-like expression (`if`, `loop`, `{ ... }`) nested inside a larger expression is hoisted
  into statements before it. If an earlier operand in the same expression has side effects, those
  now run after the hoisted block instead of before it.
