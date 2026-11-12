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
- **Literal type inference is approximate.** An unsuffixed integer literal bound to a local
  (`let mut total = 0;`) is typed by its first use against a known type. Operations that run
  before that use are lowered as `i32`. Annotate the local (`let mut total: u64 = 0;`) when it
  matters.

## Strings

- **UTF-16 under the hood.** `len()` returns the UTF-8 byte length like Rust, but byte-offset
  slicing and indexing are not supported yet.

## Netscript values

- **NS results are untyped until phase 2.** Calls on `ns` return an unknown type. Arithmetic on
  them uses plain JS semantics, and float-only method names (`floor`, `sqrt`, ...) assume `f64`.

## Checking

- **No borrow checker.** Borrows and lifetimes are parsed and erased. Programs rustc rejects for
  aliasing or lifetime reasons still run. See the borrow-checking section of [PLAN.md](PLAN.md).
- **No use-after-move or definite-initialization checks yet** (phase 1). A `let x;` local may be
  assigned more than once.
- **Few type errors.** The spike reports unknown names, wrong argument counts, assignment to
  immutable locals and some literal mismatches, but not general type mismatches.

## Evaluation order

- A block-like expression (`if`, `loop`, `{ ... }`) nested inside a larger expression is hoisted
  into statements before it. If an earlier operand in the same expression has side effects, those
  now run after the hoisted block instead of before it.
