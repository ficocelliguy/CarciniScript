# Rust feature support chart

Status reflects the end of the phase 0 spike. Legend — **Phase**: `0` spike · `1` core · `2` multi-file/NS · `3` language service · `4` later ·
`—` out of scope. **Status**: ✅ done · ✅ in progress · ⬜ planned · ❌ won't support ·
⚠️ supported with a documented gap (see `RUST_GAPS.md`).

## Items and modules

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `fn` items, params, return types | 0 | ✅ | `function`; all top-level fns exported (like RapydScript `export_all`) |
| `async fn`, `.await` | 0 | ✅ | `async function`, `await` |
| `pub async fn main(ns: NS)` | 0 | ✅ | `export async function main(ns)` |
| `const`, `static` | 1 | ⬜ | `const`; `static mut` → `let` |
| `struct` (named, tuple, unit) | 1 | ⬜ | `class`; tuple fields `_0`, `_1` |
| `impl` blocks (inherent) | 1 | ⬜ | Methods merged into the class; assoc fns → `static` |
| `enum` (field-less) | 1 | ⬜ | Numbers; `as i32` works |
| `enum` (with data) | 1 | ⬜ | Tagged objects `{ $: tag, ... }` |
| `trait` + `impl Trait for Struct` | 1 | ⬜ | Prototype methods; default methods copied in |
| `impl Trait for i32` etc. | 4 | ⬜ | `typeof`-keyed runtime registry |
| Generic fns / structs | 1 | ⬜ | Type params erased |
| `T::assoc()` on generic `T` | 4 | ⬜ | Hidden trait-dictionary params |
| `mod foo;` | 2 | ⬜ | Accepted and ignored; each file is a module |
| Inline `mod foo { ... }` | 2 | ⬜ | Namespace object |
| `use crate::a::b::item` | 2 | ⬜ | `import { item } from "a/b"` |
| `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]` | 1 | ⬜ | Compiler-generated methods |
| Other attributes | 1 | ⬜ | Ignored with a warning |
| Visibility (`pub`, `pub(crate)`) | 1 | ⬜ | Parsed, not enforced |
| Lifetimes (`'a`, `'static`, `for<'a>`) | 0 | ✅ | Parsed and erased |
| `unsafe`, raw pointers, FFI | — | ❌ | Error |
| Proc macros, external crates | — | ❌ | Error |

## Expressions and statements

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `let` / `let mut` with or without type | 0 | ✅ | `let` / `const` |
| Shadowing | 0 | ✅ | Renamed binding (`x$1`) |
| Integer / float / bool / string / char literals | 0 | ✅ | Number / boolean / string |
| Arithmetic on `i32`/`u32` and smaller | 0 | ✅ | Wrapping via `\|0`, `>>>0`, masks |
| Integer `/` | 0 | ✅ | `Math.trunc`, panics on divide by zero |
| `%` | 0 | ✅ | JS `%` (same truncated semantics) |
| `i64`/`u64`/`usize`/`isize` | 0 | ⚠️ | JS number; exact only to 2^53 |
| `i128`/`u128` | 4 | ⬜ | BigInt |
| Overflow panics (debug semantics) | 4 | ⬜ | Opt-in checked mode; default wraps |
| Comparison, logical, bitwise ops | 0 | ✅ | Direct; `>>` on unsigned → `>>>` |
| `as` casts | 0 | ✅ | Truncation/wrap helpers per target type |
| Compound assignment (`+=` ...) | 0 | ✅ | Same typed lowering as the binary op |
| `if` / `else` (statement and expression) | 0 | ✅ | `if` or IIFE/temp for expression position |
| Block expressions with tail value | 0 | ✅ | Temp variable |
| `loop`, `while`, `break`, `continue` | 0 | ✅ | `for(;;)`, `while` |
| Labeled loops | 0 | ✅ | JS labels (`$outer:`) |
| `break value` from `loop` | 1 | ⬜ | Temp variable |
| `for i in a..b` / `a..=b` / `.rev()` | 0 | ✅ | Counting `for` loop, no iterator allocated |
| `for x in iter` | 1 | ⬜ | `for...of` over runtime iterator |
| `while let` / `if let` / `let else` | 1 | ⬜ | Pattern test + bind |
| `match` (literals, ranges, tuples, structs, enums, guards, `@`, slices) | 1 | ⬜ | Decision chain |
| Exhaustiveness checking | 3 | ⬜ | Diagnostic only |
| Tuples, destructuring | 1 | ⬜ | Arrays |
| Closures (`\|x\| ...`, `move`) | 1 | ⬜ | Arrow functions |
| `?` on `Result` / `Option` | 1 | ⬜ | Early return |
| References `&x`, `&mut x`, `*x` | 0 | ✅ | Erased |
| `&mut` on a primitive local | 1 | ⬜ | Local boxed into a cell |
| `Copy` structs by value | 1 | ⬜ | Cloned on assignment and by-value pass |
| `return` | 0 | ✅ | `return` |

## Macros

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `println!` / `print!` / `eprintln!` | 0 | ✅ | `console.log` / `console.error` |
| `format!` with `{}` / `{name}` / positional | 0 | ✅ | Template literal |
| `{:?}`, width, precision, fill, alignment, `+`, `0` | 0 | ✅ | Runtime helpers; `#` and `{:width$}` still planned |
| `panic!`, `assert!`, `assert_eq!`, `assert_ne!`, `unreachable!`, `todo!` | 0 | ✅ | `CarciniPanic` with `file:line:col` |
| `vec!` | 1 | ⬜ | Built in |
| `matches!`, `write!`, `writeln!` | 1 | ⬜ | Built in |
| `macro_rules!` | 4 | ⬜ | Macro-by-example expander |

## Standard library surface

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `Option<T>` | 1 | ⬜ | `null` = `None` unless `T` is an Option |
| `Result<T, E>` | 1 | ⬜ | Tagged object |
| `String`, `&str` common methods | 0–1 | ⚠️ | JS string; `len()`/slicing via UTF-8 helpers |
| `char` | 1 | ⬜ | One-code-point string |
| `Vec<T>`, slices | 1 | ⬜ | JS `Array` |
| Iterator adapters (`map`, `filter`, `enumerate`, `zip`, `sum`, `collect`, ...) | 1 | ⬜ | Lazy runtime iterator |
| `HashMap`, `HashSet` | 1 | ⬜ | Runtime class with structural key hashing |
| `BTreeMap`, `BTreeSet`, `VecDeque`, `BinaryHeap` | 4 | ⬜ | Runtime classes |
| `Box<T>`, `Rc<T>`, `RefCell<T>`, `Cell<T>` | 1 | ⬜ | `Box`/`Rc` erased; `RefCell` optionally guarded |
| `Arc`, `Mutex`, threads | — | ❌ | No threads in Bitburner |
| `std::fs`, `std::net`, `std::process` | — | ❌ | Use the NS API |
| `f64` / integer math methods (`sqrt`, `pow`, `round`, `abs`, `min`, ...) | 0 | ✅ | `Math.*`; `round` is half-away-from-zero |
| `std::cmp::{min, max, Ordering}` | 1 | ⬜ | `Math.*` |

## Netscript

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `ns.method(args).await` | 0 | ✅ | `ns.snake_case()` → literal `ns.camelCase()` member call, so RAM cost is detected |
| Generated `ns.rs` from `NetscriptDefinitions.d.ts` | 2 | ⬜ | `tools/dts2rs.ts` |
| `Option` ↔ `undefined`/`null` at the NS boundary | 2 | ⬜ | Free via the null representation |
| `.rs` file type in Bitburner | 0 | ⬜ | See PLAN.md "Bitburner integration" |

## Checking and tooling

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| Syntax errors with Rust spans | 0 | ✅ | From ra_ap_syntax, error-resilient |
| Source maps (v3) | 0 | ✅ | Statement and expression granularity |
| Type errors | 1 | ⬜ | Local inference |
| Move checking (use after move) | 1 | ⬜ | Init/move dataflow |
| NLL-lite borrow warnings | 4 | ⬜ | Warnings; strict setting makes them errors |
| Full borrow checker (Polonius) | — | ❌ | Not planned |
| Monaco diagnostics / completion / hover / signature help | 2–3 | ⬜ | `registerCarciniScript(monaco)` |
| Warn on `loop`/`while` with no `.await` | 2 | ⬜ | Replaces Bitburner's JS infinite-loop check |
