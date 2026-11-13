# Rust feature support chart

Status reflects phase 1 progress (type checker, `Vec`, `Option`, `match`, `?` and typed NS bindings done). Legend — **Phase**: `0` spike · `1` core · `2` multi-file/NS · `3` language service · `4` later ·
`—` out of scope. **Status**: ✅ done · 🚧 in progress · ⬜ planned · ❌ won't support ·
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
| `unsafe { }` blocks, `unsafe fn`, `unsafe impl` | 1 | ⬜ | Keyword erased; `static mut` covered by the `static` row |
| Raw pointers used as references (`*const T`, `*mut T`, `*p`, `ptr::null`, `is_null`, `ptr::eq`), same-size numeric `transmute` | 4 | ⬜ | Treated like references; null is `null`; `ptr::eq` → `===`; `transmute` via `DataView` like `f64::to_bits` |
| Pointer arithmetic, pointer ↔ integer casts, `union`, layout-dependent `transmute`, `extern "C"` / FFI | — | ❌ | Error: values have no memory layout to reinterpret |
| User-written proc macros (function-like, derive, attribute) | 4 | ⬜ | Macro file (e.g. `#![proc_macro]`) compiled first and run at compile time on a runtime `proc_macro` API; built-in `quote!`-lite and a `DeriveInput`-style field parser stand in for `syn`/`quote`; output re-parsed by `ra_ap_syntax`. Step/time limit, no file I/O unless the host passes a sync read callback, `compile_error!` for diagnostics |
| External crates (including `syn`/`quote`-based proc macros) | — | ❌ | Error |

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
| `for x in v` / `&v` / `.iter()` / `.into_iter()` over a `Vec` | 1 | ✅ | `for...of` |
| `for x in &mut v` / `v.iter_mut()` | 1 | ✅ | Index loop; `x` aliases `v[i]`, so `*x += 1` writes through |
| `for x in` other iterators (adapters, maps, strings) | 1 | ⬜ | `for...of` over runtime iterator |
| `if let` / `while let` / `let ... else` | 1 | ✅ | Pattern test + bind; `while let` re-evaluates its scrutinee each iteration. Let-chains (`if let .. && ..`) ⬜ |
| `match` on literals (incl. negative), ranges (`a..=b`, `a..b`, `a..`, `..=b`; ints and chars), `|`, guards, `x @ p`, `Some`/`None`, as a statement or a value | 1 | ✅ | if/else chain; a labeled block when guards are present |
| Tuple, slice, struct and enum patterns | 1 | ⬜ | Arrive with tuples, structs and enums |
| Exhaustiveness checking | 1 | ⚠️ | Catch-alls, `bool`, `Option`; guarded arms don't count. Integer ranges covering a whole type still need `_` |
| Tuples, destructuring | 1 | ⬜ | Arrays |
| Closures (`\|x\| ...`, `move`) | 1 | ⬜ | Arrow functions |
| `?` on `Option` | 1 | ✅ | `const $t = x; if ($t == null) return null;` |
| `?` on `Result` | 1 | ⬜ | Arrives with enums |
| References `&x`, `&mut x`, `*x` | 0 | ✅ | Erased |
| `&mut` on a primitive local | 1 | ⬜ | Local boxed into a cell |
| `Copy` structs by value | 1 | ⬜ | Cloned on assignment and by-value pass |
| `return` | 0 | ✅ | `return` |
| Indexing `v[i]` (read, assign, compound assign) | 1 | ✅ | `v[κσ.$ix(v, i, loc)]`; rustc's out-of-bounds panic message and location |
| Slicing `v[a..b]`, `&s[1..]` | 1 | ⬜ | |
| Array literals `[a, b]`, `[x; n]` | 1 | ⚠️ | Same as `vec!` (fixed-size arrays are growable JS arrays) |

## Macros

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `println!` / `print!` / `eprintln!` | 0 | ✅ | `console.log` / `console.error` |
| `format!` with `{}` / `{name}` / positional | 0 | ✅ | Template literal |
| `{:?}`, width, precision, fill, alignment, `+`, `0` | 0 | ✅ | Runtime helpers; `#` and `{:width$}` still planned |
| `panic!`, `assert!`, `assert_eq!`, `assert_ne!`, `unreachable!`, `todo!` | 0 | ✅ | `CarciniPanic` with `file:line:col` |
| `vec![a, b]`, `vec![x; n]` | 1 | ✅ | `[a, b]`; `κσ.$vecRepeat` clones non-primitive elements (`vec![vec![0; 3]; 3]` gets 3 rows) |
| `matches!`, `write!`, `writeln!` | 1 | ⬜ | Built in |
| `macro_rules!` | 4 | ⬜ | Macro-by-example expander |

## Standard library surface

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `Option<T>` | 1 | ✅ | `None` = `null` (`undefined` from JS also reads as `None`), `Some(x)` = `x`. `is_some`, `is_none`, `unwrap`, `expect`, `unwrap_or`, `unwrap_or_default`, `take`, `replace`, `as_ref`, `cloned`, `==`, `{:?}`. `Option<Option<T>>` / `Option<()>` rejected |
| `Option` methods taking closures (`map`, `and_then`, `unwrap_or_else`, ...) | 1 | ⬜ | Arrive with closures; currently a clear error |
| `Result<T, E>` | 1 | ⬜ | Tagged object |
| `String`, `&str` common methods | 0–1 | ⚠️ | JS string; `len()`/slicing via UTF-8 helpers |
| `char` | 1 | ⬜ | One-code-point string |
| `Vec<T>`, `&[T]`, `[T; N]` | 1 | ✅ | JS `Array`. `new`, `with_capacity`, `len`, `is_empty`, `push`, `insert`, `remove`, `swap`, `truncate`, `clear`, `reverse`, `sort`/`sort_unstable` (Rust `Ord`), `contains`, `extend_from_slice`, `join`, `concat`, `clone`/`to_vec`, `==`/`!=`, `{:?}`. Panic messages match rustc |
| `Vec` methods returning `Option` (`pop`, `first`, `last`, `get`) | 1 | ✅ | `get_mut`/`first_mut`/`last_mut` ⬜ (they hand out references into the Vec) |
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
| `ns.method(args).await` | 0 | ✅ | `ns.snake_case()` → literal `ns.camelCase()` member call (JS name from the bindings), so RAM cost is detected |
| Typed NS bindings generated from `NetscriptDefinitions.d.ts` | 1 | ✅ | `tools/dts2rs.mjs` → `bindings/ns.rs`, loaded with `setNsBindings`. Typed arguments, returns (`Promise<T>` → `async`), interface fields (`server.max_ram`), namespaces (`ns.hacknet`); typo suggestions; a forgotten `.await` is a type error |
| NS parameters: omit trailing optionals, `...rest`, integer literals for `f64` | 1 | ⚠️ | Deliberate extensions over Rust; see `RUST_GAPS.md` |
| `Option` ↔ `undefined`/`null` at the NS boundary | 1 | ✅ | Optional fields are `Option<T>`; `undefined` reads as `None` |
| `.rs` file type in Bitburner | 0 | ✅ | `bitoxidizer-src` (`bitoxidizer` branch) |

## Checking and tooling

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| Syntax errors with Rust spans | 0 | ✅ | From ra_ap_syntax, error-resilient |
| Source maps (v3) | 0 | ✅ | Statement and expression granularity |
| Type inference and type errors | 1 | ✅ | `typeck.rs`: whole-function inference with type variables and unification; unsuffixed literals default to `i32`/`f64` like rustc. Reports mismatched types, `cannot add`, ambiguous numeric methods, bad index types, missing `Display` |
| Mutability errors | 1 | ✅ | rustc's "cannot assign twice", "cannot borrow `v` as mutable", "behind a `&` reference" |
| Move checking (use after move) | 1 | ⬜ | Init/move dataflow |
| NLL-lite borrow warnings | 4 | ⬜ | Warnings; strict setting makes them errors |
| Full borrow checker (Polonius) | — | ❌ | Not planned |
| Monaco diagnostics / completion / hover / signature help | 2–3 | ⬜ | `registerCarciniScript(monaco)` |
| Warn on `loop`/`while` with no `.await` | 2 | ⬜ | Replaces Bitburner's JS infinite-loop check |

## Potential future features (from Bitburner Discord discussion)

Rust features that Bitburner Discord users have talked about using, liking, or missing in JS,
gathered from a server-wide "rust" search and from #beyond-javascript (November 2026 snapshot).
Most of them already appear in the tables above. This section records the details users cared about,
so the work can be prioritized against real interest. The **Status** column repeats the main chart.

| Feature | Status | What came up |
|---|---|---|
| `let ... else` | ✅ | Called a "favorite pattern" several times: `let Some(x) = opt else { return; };`, `let Ok(lock) = m.write() else { ... };` |
| `if let` / `while let` | ✅ | `while let Some(node) = tree.get(&seq) { match ... }`; `if let` in expression position |
| `match` as an expression, guards, unwrapping by pattern | ✅ / ⬜ (enums, tuples) | The most praised feature overall. Users want "unwrap the value if it matches" plus `if`/`match`/`loop` that yield values |
| Rest and slice patterns (`..`, `[first, .., last]`), destructuring | ⬜ | Rest patterns compared to JS spread; "rust has destructuring" |
| `?` on `Result` / `Option` | `Option` ✅; `Result` ⬜ | Named as the reason Rust doesn't need exceptions |
| Collecting into `Result`/`Option` (`collect::<Result<Vec<_>, _>>()`) | ⬜ | Short-circuits on the first `Err`; needs the turbofish and `FromIterator` for `Result` |
| Turbofish `::<T>` | ⬜ | Needed for `collect`, `parse`, `throw_error::<...>` style calls |
| `Option`/`Result` combinators | ⚠️ (`unwrap_or`, `unwrap_or_default` ✅; closure-taking ones ⬜) | `unwrap_or`, `unwrap_or_default`, `map_or`, `transpose`, `ok`, `flatten`, `inspect_err`, `bool::then` |
| Iterator chains | ⬜ | `iter`/`into_iter`, `enumerate`, `zip`, `map`, `filter`, `sum`, `chars`, `cycle`, `collect::<String>()`. Long dot-chains are a common style |
| Range values and range indexing | ⚠️ | `for` over ranges works. Still missing: `0..10` as a value and `arr[2..5]` / `s[1..]` slicing |
| Enums with data (ADTs) | ⬜ | "What Rust calls enums are algebraic data types". Used for union types instead of TS enums |
| `impl Default` + `unwrap_or_default` | ⬜ | `enum CardKey { SIX([u8; 6]) }` with a hand-written `Default` |
| Method receivers `&self` / `&mut self` / `self` | ⬜ | Consuming `self` for builder/finalize patterns; assoc fns without `self` as static methods |
| UFCS call form `Foo::method(foo)` | ⬜ | Shown alongside `foo.method()` as equivalent |
| Traits with default methods, generic impls, bounds, `where` | ⬜ | `impl<R: BufRead> Reader<R>`, `T: Float + From<f32>`, `where f64: From<T>` |
| Extension traits (methods visible only when the trait is in scope) | ⬜ | Contrasted with JS prototype patching. Needs `use` of a trait to gate method lookup |
| Fully qualified disambiguation `<T as Trait>::method` | ⬜ | Rust's answer to same-named methods from two traits |
| `From` / `Into` conversions | ⬜ | `impl From<X> for Y` then `.into()`, very common in binding code |
| `std::ops` traits (`Deref`, `Add`, ...) | ⬜ | `impl Deref for Wrapper`. Operator overloading "possible but rarely used" |
| Closures, including `move` | ⬜ | "Closures are extra incredible in rust" |
| `macro_rules!` | ⬜ (4) | Discussed at length: `$(...)*` repetition, fragment specifiers (`ident`, `ty`, `tt`, `expr`, `literal`), recursive macros, hygiene, macros used to stamp out impls per numeric type |
| `u128` / `i128` | ⬜ (4) | Widening multiply macros map `u64 → u128` |
| Fixed-size arrays and boxed slices (`[u8; 6]`, `Box<[u8]>`) | ⚠️ / ⬜ | Bitboard-style code with `&= !mask` on array elements |
| `Cell` / `RefCell` (interior mutability) | ⬜ | Explained in chat as "mutates opaquely through a non-mut reference" |
| Overflow panics | ⬜ (4) | "rust panics on overflow if you dont specifically allow it". A user ran into it right away |
| Short-circuit block expressions | ✅ | `a && { b += 1; true }` works when the types match |
| Shadowing | ✅ | "I wish JS had the same capabilities of shadowing as rust has" |
| `#[cfg(test)] mod tests` + `#[test]` | ⬜ | Rust testing is described as much nicer than JS testing. Possible in-game test runner |
| `///` doc comments (`# Arguments` sections) | ⬜ | Could feed Monaco hover/signature help |
| `#[allow(...)]` and naming-convention lints | ⬜ | `#[allow(dead_code)]`; rustc warns on non-snake-case names |
| Module paths (`module::function`, `foo.rs` vs `foo/mod.rs`) | ⬜ (2) | "I kinda miss rusts module::function" |
| `matches!`, `vec!`, `format!` family | `vec!`, `format!` ✅; `matches!` ⬜ | Used constantly in pasted snippets |
| Panic surfacing | ✅ | Wasm users had to install `panic::set_hook` to see panics in Bitburner. `CarciniPanic` with `file:line:col` already covers this |

Out of scope, even though they came up: threads and `mpsc` channels, pointer arithmetic and layout `transmute`,
crates.io proc macros (user-written ones are phase 4, above), external crates (`serde`, `rayon`,
`wasm_bindgen`), and generic code over numeric types that Rust itself can't express (SFINAE).
