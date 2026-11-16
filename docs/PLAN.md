# CarciniScript plan

CarciniScript compiles a practical subset of Rust to ES modules, with v3 source maps, synchronously,
in the browser, at runtime. The first consumer is Bitburner (`.rs` scripts), following the same
integration shape as RapydScript-NS (`.py` scripts) in `bytburner-src`.

> *Carcinisation*: the tendency of evolution to keep producing crabs. Every language eventually
> becomes Rust.

## Contracts inherited from the RapydScript integration

| Concern | Python (RapydScript-NS) today | CarciniScript |
|---|---|---|
| Compile call | `web_repl().compile_mapped()`, sync, pure JS | Sync after one async `init()` (same as `@swc/wasm-web`) |
| Output | ES module, `export async function main(ns)`; evaluator rewrites `import` specifiers to blob URLs | Same |
| Runtime library | Baselib injected once onto `window` (`ρσ_*`) | Prebuilt runtime installed once on `globalThis.κσ` |
| Source maps | v3 JSON from compiler mappings, inlined by `NetscriptJSEvaluator` | Same; no evaluator change |
| Cross-file imports | `buildPythonVirtualFiles()` → `{ "dir/mod": code }` | Same with `.rs`; the type checker needs other files' signatures |
| RAM cost | `parseAST()` compiles then acorn-parses the JS | Same, so output must keep literal `ns.hack(...)` member calls |
| Editor | `registerRapydScript(monaco)` + `addDts(netscriptDefinitions)` | `registerCarciniScript(monaco)` + generated `ns.rs` bindings |

## Why not real rustc → wasm

- rustc + sysroot is hundreds of MB; compiles take seconds.
- Every NS call returns a Promise. Wasm can only block on one via JSPI, which is not universal.
- Bitburner's RAM calculator needs a JS AST.
- Wasm debugging needs DWARF tooling; JS + source maps just works in devtools.

## Architecture

```
carciniscript/
  compiler/          Rust crate → wasm-bindgen
    parse            ra_ap_syntax (rust-analyzer's error-resilient parser)
    lower            CST → simplified HIR                     (phase 1)
    typeck           whole-function inference + checking      (done for the current subset)
    codegen          HIR/AST → JS text + mapping records
    sourcemap        `sourcemap` crate → v3 JSON
  prelude/std.rs     std surface as Rust signatures + #[js = "..."] mappings   (phase 1)
  runtime/rt.js      Option/Result, iterators, HashMap, fmt, panics
  language-service/  registerCarciniScript(monaco, opts)                       (phase 2–3)
  tools/dts2rs.ts    NetscriptDefinitions.d.ts → ns.rs bindings               (phase 2)
  test/              fixtures + differential tests (native rustc vs node)
  docs/RUST_GAPS.md  counterpart to RapydScript's PYTHON_GAPS.md
```

**Parser: `ra_ap_syntax`, not `syn`.** syn stops at the first error, which makes for poor editor
diagnostics. ra_ap_syntax recovers, keeps byte ranges, and covers the full grammar. Its API churns,
so the version is pinned.

**Types are the hard part.** Unlike Python, Rust semantics depend on static types:
`7 / 2` is integer division for `i32`, float division for `f64`, and JS can't tell `4` from `4.0`
at runtime. Method resolution, `.into()`, `Default::default()` and `{}` vs `{:?}` all go through
types. Function signatures are always annotated, so local bidirectional inference is tractable.
We check against `prelude/std.rs` and generated `ns.rs`; we do not use `ra_ap_hir` (needs the whole
std source, unstable API).

## Lowering decisions

See [FEATURES.md](FEATURES.md) for the per-feature chart. The load-bearing decisions:

- **Borrows and lifetimes** are erased in codegen. Exceptions: `&mut` taken on a primitive local
  boxes that local into a cell; `Copy` structs are cloned on assignment and by-value passing.
- **Integers:** `i8..i32`/`u8..u32` wrap (`|0`, `>>>0`, masks). `usize`/`isize` are JS numbers
  (exact to 2^53). `i64`/`u64` and `i128`/`u128` are BigInt, wrapped to their width. Integer `/` truncates and panics on zero;
  JS `%` already matches Rust's truncated remainder.
- **Option:** `None` is `null`, `Some(x)` is `x`; `Some(None)` (per depth) and `Some(())` are markers (`κσ.$some` / `$unsome`).
  The NS boundary is free: `undefined` results read as `None`.
- **Result:** tagged object; `?` is an early return.
- **Panics** throw `CarciniPanic` with `file:line:col` baked in at compile time.
- **Structs/enums:** structs → classes with all `impl` blocks merged; field-less enums → numbers;
  data enums → tagged objects; `match` → decision chain.
- **Collections:** `Vec` = JS `Array`; `HashMap`/`HashSet` = runtime class with structural key
  hashing for tuple/struct keys; iterators = lazy runtime adapter class.
- **Strings:** JS strings; `len()` and byte-range slicing go through UTF-8 helpers.
- **Async:** `pub async fn main(ns: NS)` → `export async function main(ns)`; `.await` → `await`.
- **`println!`** → `console.log`. Modules are cached across scripts, so there is no single `ns` to
  print to; scripts should use `ns.tprint(&format!(..))`.
- **Modules:** each `.rs` file is a module; `use crate::a::b::item` → `import { item } from "a/b"`.
- **Runtime names** live under `κσ` so none collide with NS function names in RAM calculation.

## Borrow checking (static, in the compiler)

Borrowing and lifetimes are compile-time concepts; the emitted JS is identical either way.
Erasing them only means programs rustc would reject still run. Tiers:

1. **Move checking** (phase 1, done): use-after-move via init/move dataflow. Needs only
   `Copy`-ness.
2. **NLL-lite** (done, `borrows.rs`): borrows bound by `let` and borrowing `for` loops, live to
   the holder's last use, conflicts reported with rustc's messages. Shown as warnings; strict
   borrows make them errors. Not yet: borrows through calls (elided/annotated signatures) and
   fields.
3. **Polonius** (not planned): would need MIR-like lowering and region inference.

Optional debug mode: RefCell-style runtime guards on collections (e.g. mutate-while-iterating
panics instead of silently misbehaving).

## NS bindings

`tools/dts2rs.ts` uses the TypeScript compiler API on `NetscriptDefinitions.d.ts`:
`number` → `f64`, `string` → `String`, `T[]` → `Vec<T>`, optional / `T | undefined` → `Option<T>`,
`Promise<T>` → `impl Future<Output = T>`, interfaces → extern structs (with `#[extends = ".."]`
for `interface X extends Y`), tuples → `[T; N]` / `(A, B)`, `Map` / `Record` → `HashMap`, inline
object results and `Named & { .. }` intersections → structs of their own, anything else →
`JsValue` (wasm-bindgen's, with `as_string()` and friends; a `JsValue` parameter takes any value).
Scripts can name the interfaces and build the option ones (`RunOptions { .., ..Default::default() }`).

## Source maps

The codegen writer tracks output line/column and records a mapping at each statement and
expression back to the Rust byte offset, converted to line + UTF-16 column. The `sourcemap` crate
encodes v3 JSON. Bitburner's existing inlining in `NetscriptJSEvaluator.ts` is unchanged.

## Bitburner integration (in `bytburner-src`)

- `src/Paths/ScriptFilePath.ts`: add `".rs"`.
- `src/utils/ScriptTransformer.ts`: `FileType.RS`, `getFileType("rs")`, `isRust`, RS branch in
  `parseAST`; generalize `buildPythonVirtualFiles` over extension.
- `src/utils/rust/compileCarciniScript.ts` (modeled on `compileRapydscript.ts`): `init()` at
  startup next to swc, install runtime once, `compile` / `compileWithSourceMap`.
- `src/NetscriptJSEvaluator.ts`: `case FileType.RS`.
- `src/ScriptEditor/ScriptEditor.ts`: register the language service; feed `ns.rs`.
- `ScriptEditorRoot.tsx`: virtual files; skip JS infinite-loop detection for `.rs` (language
  service warns on `loop`/`while` with no `.await` instead).
- `ui/utils.ts`: language `"rust"` (Monaco has Rust highlighting built in).
- `Terminal/commands/common/editor.ts`: `.rs` template.

## Phases

0. **Spike** (done):
   - [x] ra_ap_syntax → wasm (~700 KB). Needs `--cfg no_salsa_async_drops`, see
     `compiler/.cargo/config.toml`.
   - [x] Codegen for `fn`, `let`, `if`/`loop`/`while`/`for`-over-range, labeled loops, typed
     arithmetic and casts, `.await`, NS method calls, `println!`/`format!` (incl. width,
     precision, `{:?}`), `panic!`/`assert*!`.
   - [x] Source maps (statement and expression granularity).
   - [x] Node harness: differential tests vs native rustc, NS mock scripts, source map and
     diagnostics checks.
   - [x] Web REPL with in-browser self-test; crash sweep over 8.7k real-world `.rs` files
     (cargo registry + std) with 0 panics.
   - [x] Wire `.rs` into Bitburner: `ns.hack(&t).await` runs, devtools breakpoints land on Rust
     lines, RAM cost is right (`test/jest/Netscript/RustScripts.test.ts`).
1. **Core language:**
   - [x] Type checker (`typeck.rs`): inference across a function body, rustc-style type and
     mutability errors. Codegen reads its type table.
   - [x] `Vec<T>`: `vec!`, array literals, bounds-checked indexing, mutation methods, `for` loops
     (including `&mut`), `{:?}`, `==`, `clone`.
   - [x] `Option`, `if let` / `while let` / `let ... else`, `match` (literals, ranges, `|`,
     guards, `@`), exhaustiveness for `bool` / `Option` / catch-alls.
   - [x] `?` on `Option` (`Result` comes with enums)
   - [x] Typed NS bindings: `tools/dts2rs.mjs` generates `bindings/ns.rs` from
     NetscriptDefinitions.d.ts; Bitburner regenerates its copy with `npm run generate:rust-bindings`
     and a jest test fails when it's stale.
   - [x] Tuples and destructuring (patterns, `enumerate`, swap assignment, value copies)
   - [x] Closures and iterator adapters (plus `Ordering`, sorting with closures, and the `Option` / `HashSet` / `HashMap` methods they unblock)
   - [x] `HashSet` (the starter-script milestone: `test/ns/crawl.rs`)
   - [x] `HashMap` (including the `entry` API and write-through iteration)
   - [x] `struct` + `impl` + derives, then `enum` with data (plus `Result`, `?` on `Result`,
     `impl Display`, `str::parse`, `matches!`)
   - [x] Warn on `loop`/`while` with no `.await`
   - [x] `const` / `static` items, generic functions (erased, with bounds), range slicing
   - [x] Move checking (`moves.rs`: use after move, definite initialization)
   - [x] Since: operator overloading, `From`/`Into`, `impl Trait for i32`, custom iterators,
     `i128`/`u128`, std collections (`VecDeque`, `BinaryHeap`, `BTreeMap`, `BTreeSet`), `Error` /
     `Box<dyn Error>`, `type` aliases, `use Enum::*`, `macro_rules!` (pulled in from phase 4)
   - [x] `get_mut` of numbers/strings/bools, integer and `char` range exhaustiveness (and no false
     alarms on unknown types), `char::to_uppercase`, `IndexMut` writes, destructured parameters,
     block-scoped `type` aliases, `vis`/`meta` macro fragments, macros from `mod` files,
     `&mut v[a..b]` views and `split_at_mut`, `unsafe` blocks and `static mut`, `Deref`/`DerefMut`, `<T as Trait>::method`, `pub(super)`/`pub(in ..)`
   - [x] `macro_rules!` hygiene for local bindings
2. **Multi-file:** `use` across files, virtual files, `dts2rs`, Monaco diagnostics.
   - [x] `mod name;` + `use` through a host resolver; Bitburner passes the server's scripts (runtime,
     RAM calculation and editor markers)
   - [x] `foo/mod.rs`, `pub use` re-exports (renamed and `*`), privacy across files
   - [x] Inline `mod x { .. }` with `self::` / `super::` / `crate::` paths (`modflat.rs`)
   - [x] Nested module paths through loaded files (`a::b::item` where `a.rs` has `pub mod b;`)
   - Not planned: `super::` at the top of a module file (each file compiles on its own)
3. **Language service:** completion, hover, signature help from the checker's symbol tables.
   - [x] Hover (with doc comments) and member completion (`hover`, `completions`), wired into
     Bitburner's Monaco
   - [x] Signature help (`signature_help`) and live error markers
   - [x] Go to definition (also into `mod x;` files), find references and rename (`nav.rs`), in
     Bitburner's Monaco
   - [x] References and rename across files (Bitburner passes the server's `.rs` scripts)
4. **Later:** wider std, more JS globals (see FEATURES).
   (`macro_rules!` is done.)
   - [x] Optional overflow checks (debug-build semantics), with `test/debug/` against `rustc` without `-O`
   - [x] `T::assoc()` / `T::CONST` / `T::default()` on generic `T` in free functions and on a
     method's own type parameters (hidden class
     arguments); default trait functions without `self`
   - [x] NLL-lite borrow warnings (and a strict mode)
   - [x] Raw pointers as nullable references (cells for numbers), `ptr::null`/`eq`,
     `Box::into_raw`/`from_raw`, same-size `transmute`; `unsafe { }` bodies are type-checked
   - [x] Wider std: float bits and math, `div_euclid`/`ilog`/`isqrt`, `scan`/`unzip`/`map_while`,
     `std::iter` sources, nested `concat`, `BTreeMap::range`, `Weak`, `u64::pow(..)` path calls
     and `i32::max`-style method values (`ufcs.rs`)
   - [x] Performance: bounds checks inline when the Vec and index are simple (a DP loop ran 1.6x
     faster), iterator adapters and ranges as iterator classes instead of generators (2-3x on
     adapter chains), `x % 7` / `x / 7` without the divide-by-zero helper. Compile time is linear
     (about 1 ms per KB)
   - [x] JS globals, first `js::Date` and `js::JSON` (serde_json's format and errors, by the
     static type; `#[serde(..)]` renames), tested against serde_json through a native shim
     crate (`test/js/`)
   - [x] The rest of the JS globals, declared in `bindings/js.rs` (associated and free
     functions, getters, array-like results): DOM, storage, window, location, navigator,
     history, screen, console, Math, performance, crypto, URL, fetch, base64, timers,
     `js::sleep`, `Promise::all` / `race`, `js::global`. Node tests (`test/js-node/`), a
     browser-check DOM scenario, and a Bitburner RAM test for `document` / `window`
   - [x] Fixed-size arrays of `Copy` values are `Copy`; exact 64-bit integers (`i64` / `u64` as
     BigInt); associated types, generic traits and blanket impls; `impl FromStr`; `Into` /
     `AsRef` parameters and bounds; operators on references; iterator-bounded struct parameters;
     one trait-impl registry per script (impls on built-in types no longer leak between scripts)
   - [x] `{:?}` in generic code by the caller's type (a hidden formatter per type parameter and
     projection, like the classes for `T::make()`); associated types and generic trait arguments
     checked through function type parameters (`S::Unit`, `S: Shape<Unit = f64>`)
   - [x] Const generics on functions, structs, impls and methods (hidden `$N` arguments like the
     classes for `T::make()`; struct values keep theirs as `$N`; read off `[T; N]` arguments when
     the call doesn't say them); enums and traits ⬜
   - [x] Hand-written `Debug` through `f.debug_struct(..)` and the other builders (also `{:#?}`,
     and generic types' formatters); `debug_assert!` family, `cfg!`, `stringify!`; hand-written
     `Hash` (with `Hasher` / `DefaultHasher`), `Copy`, `TryFrom` (and integer `try_from` /
     `try_into`), `FromIterator`, `Extend`, `IntoIterator` parameters; or-patterns that bind
     variables
   - [x] Iterator adapters: `peekable` (`peek`, `next_if`, `next_if_eq`), `fuse`, `by_ref`,
     `try_fold`, `try_for_each`, `next_back`, `nth_back`, `rfind`, `rfold`, `len`, iterator
     comparisons, `is_sorted*`; std iterator types in signatures (`&mut Peekable<Chars>`)
   - [x] Trait bounds: `Div` / `Rem` / `Neg` / `*Assign` and every operator on a `T` by the caller's
     class (exact integer division and wrapping; your types' `impl Add`); `IntoIterator` (and
     `I::Item`), `FromStr`, `From<X>`, `Sum` / `Product`, `Send` / `Sync`
   - [x] Bitwise bounds: `BitAnd` / `BitOr` / `BitXor` / `Not` / `Shl` / `Shr` and their `*Assign`
     forms on a `T` (bools, every integer width, masked shifts; your types' `impl BitOr` / `Not`);
     `impl BitAndAssign` etc. for your types
   - [x] An `impl<T>`'s `T` in methods with `self` (structs): values carry `T`'s class
     (`this.$T`) when a method needs it, for operators, `parse::<T>()`, `T::default()` and
     generic calls; derived `Default` of a generic struct is typed `S<T>`
   - [x] rustc's "type annotations needed for `Vec<_>`" on `let` bindings nothing types
   - [x] Bundled crates `rand` (RNGs, `gen_range`, `shuffle`, `choose`, ..) and `itertools` (the
     common `Itertools` methods), typed in `typeck_crates.rs` and run by runtime helpers; tests
     build natively against the real crates (`tools/native-build.mjs`, cargo offline)
   - [x] More bundled crates, picked from the crates that alone block the most registry files:
     `anyhow` (runtime error class with the cause chain), `thiserror`, `bitflags!`, `cfg_if!` and
     `lazy_static!` (source rewrites, `bundled.rs`), `once_cell` / std's `LazyLock` / `OnceLock`
     and `either` (Rust-source preludes), `hashbrown` (std's maps); `HashMap::<K, V>::new()` and
     the other collections' turbofish; `{:?}` of `Ok(())`
   - [x] The next crates real programs reach for: `serde_json` (typed like `js::JSON`, with
     serde_json's error positions; `Value` / `Number` as a Rust-source prelude reached from the
     runtime's JSON reader, `json!` as a token-tree rewrite), `regex` (a Rust-source prelude on a
     small runtime engine that translates Rust's syntax to `RegExp`, byte offsets), `rayon`
     (sequential, a rewrite) and `extern crate`; `?` checks an error type that `parse()` settles
     later; JSON schemas use the class's property names (`ram$`)
   - [x] The remaining std gaps real programs hit: `impl Drop` (try/finally per block, a flag
     per binding cleared where the move checker sees a move), ranges as values (`Ty::Range`, the
     same iterator at run time), `Cow` and `std::io` (Rust-source preludes, `std::io` names
     renamed into them), `b"..."`, `async` blocks; and the crates `log` / `env_logger`, `num`
     (`num-traits`, `num-integer`) and `tokio` / `futures` (preludes, bundled macros and renames
     of their paths)
   - [x] `rand_distr` (and `rand::distributions`), `chrono` and `clap`: Rust-source preludes, the
     paths renamed into them; clap's derives written out by the pre-pass (`clap_derive.rs`).
     Along the way `std::env::args` / `process::exit`, operator impls on generic types
     (`impl<Tz> Sub for DateTime<Tz>`), `parse::<T>()` into a generic `FromStr` impl, a
     per-script cache of trait statics on numbers (`$numT`)
   - [x] Bitburner corpus C16 and C17, and two `Drop` / ordering gaps: async closures and
     `AsyncFn` / `Future<Output = T>` bounds, `Pin<Box<dyn Future>>` with `Box::pin` (erased) for
     recursive async fns; `Default::default()` typed by a later `break` or arm (checked at the
     end of the function); closures owning their by-value captures (`κσ.$own`, dropped with the
     closure, whose moves the move checker records without reporting them) and by-value `self`
     dropped at the end of its method; NaN-aware partial comparisons (`$cmp` gives NaN)
   - [x] `Some(None)` as a marker (C6 in full), `JsValue` from Rust values and its `Default`,
     fields moved out of a struct (`κσ.$moveOut`), closures given to std methods dropped at the
     end of their statement, Rust 2021 field captures of `move` closures
   - [x] The Bitburner corpus's section D, C6 and C10 (GAPS.md): std methods and paths as values
     (the UFCS pass), a `hash_map::Entry` prelude over the map's slots, `Option<()>` markers and
     `.flatten()` temporaries, `Into<f64>` bounds kept generic, `T: DeserializeOwned` through a
     class's `$jsonSchema`
   - [x] The rest of the Bitburner corpus (all 14 files compile as written): `include_str!`
     through the module resolver, `impl Into` / `AsRef` parameters converted at calls,
     temporaries as `&mut` arguments, `iter_mut().zip(..)` index loops, `step_by` jumping over
     ranges, array lengths from declarations, `*h` of a `filter` closure's reference; chrono-tz on
     `Intl`; clap's exit in Netscript scripts
   - [x] `std::time` (`Duration`, `Instant`, `SystemTime`) as a Rust-source prelude compiled in
     when used, diff-tested against rustc; `js::structured_clone`; Formatter specs for
     hand-written `Display` / `Debug` (`f.width()`, `f.precision()`, `f.pad(..)`); `impl Sum` /
     `impl Product`; `I: Iterator<Item = T>` type parameters
   - [x] The Bitburner corpus' gaps (`corpus/bitburner/GAPS.md`): typed `JsValue` with
     wasm-bindgen's methods and `serde_wasm_bindgen`; NS interfaces nameable and buildable,
     `extends` subtyping, intersection / tuple / `Map` / inline-object results; `#[serde(untagged)]`;
     `Default` of `[T; N]`; a local fn shadowing a module's; `mod` files' `Deref` / iterator impls
     and `pub type` aliases; `Cell` as a user type name; `or_default()` of structs; `const`s in
     format strings; irrefutable array patterns and `<[T; N]>::try_from`; `impl AsRef`; two
     `From` impls of one source type; `Option` ordering; `Copy` structs with array fields
   - [x] User-written proc macros (function-like, derive, attribute) with `quote!` (nested
     repetitions), `format_ident!`, token trees and a `syn` subset with `ParseStream` parsers,
     `Token![..]` and `parse_quote!` (a Rust prelude stands in for
     `proc_macro`, `proc_macro2`, `quote` and `syn`; `impl IntoIterator` support came with it): the
     crate compiled to JS and run at compile time with a step limit (`procmacro.rs`); tests build
     the same files with cargo (`test/procmacro/`); Bitburner recompiles a script when a macro
     crate it uses changes

## Real-world corpus gaps (2026-11-16)

Every `.rs` file in the local cargo registry (226 crates, 5,197 files after dropping 698 parser and
compile-fail fixtures) was compiled file by file, and each error was sorted into "missing crate or
module", "thread", "std library", or "language". Errors from names in other crates or other files of
the same crate, and from threads, `Arc`, `Mutex` and atomics, don't count. Unresolved names don't
hide later errors, so this is a fair per-file check.

- **Language features all supported:** 67% of files (61% of `src/`, 85% of `examples/`, 86% of
  `tests/`, 93% of `benches/`). Counting std-library gaps as failures, 50.5%. By lines, 39%.
- **27 crates** pass in every file (strsim, shlex, percent-encoding, utf8parse, anstyle, unicode-ident ..).
- **Not planned:** raw pointers, `transmute`, `MaybeUninit`, `alloc`, `union` and `extern` blocks
  (602 + 130 + 53 files). JS values have no memory layout.
- **Re-run after `3c15b78`** (230 crates, 5,220 files): language features all supported in 67.7%;
  counting std gaps, 56.4% (up from 50.5%); by lines, 42.6%. Files whose only problem is std fell
  from 861 to 594. Language gaps barely moved (1,629 to 1,594 files), since that commit was std work.
  25 crates pass in every file: the two icu `*_data` crates dropped out because `include!` now
  works and pulls in a `macro_rules!` in an unsupported place. Counts below are from this run.
  Most "std type" hits that remain (`Path`, `Ref`, `Cursor`, `Output`) are the crate's own types
  with std names, such as `syn::Path` or chrono's private `Cursor`, not std gaps.
- [ ] **Regressions in `3c15b78`**, both in files that compiled before:
  - a user `enum Ordering` in a pattern is taken as `std::cmp::Ordering`: "expected `Ordering`,
    found `Ordering`" (`ra_ap_syntax` `ast/operators.rs`)
  - `#[derive(Clone, Copy)] struct W<'a> { s: &'a [u8] }` gives "the trait `Copy` is not
    implemented for `Vec<u8>`" (`miniz_oxide`)

### Language gaps

Counts are files that hit the gap, then files where it's the only language gap. Status: `[x]`
done · 🚧 in progress · `[ ]` open (the same 🚧 as FEATURES.md's legend).

- [x] **Trait impls on std types** (336 / 182): your traits on `Option<T>`, `Result<T, E>`,
  `HashMap` / `BTreeMap`, `HashSet` / `BTreeSet` and `u128` / `i128` (generic impls too, through a
  generic `T: Tr` for all but `Option`), and `impl From<A> for String` / for a number type
  (`String::from(a)`, `a.into()`, `impl Into<String>`). `fixtures/std_type_impls.rs`.
  - [ ] Still open: `Box<str>`, `Path`, `File`, `Wrapping<u32>`; std traits other than `From`
    (`impl PartialEq<Id> for &str`); an `Option` reached through a generic `T: Tr` (an `Option` is
    `null` or its value, so `κσ.$dcall` can't tell it apart); `&mut self` methods of your traits on
    a `String`, number, bool or `Option` (JS can't write those through `this`).
- [x] **`where` clauses with Fn bounds over type parameters** (200 / 85): `fn run<F, T>(f: F) -> T
  where F: FnOnce() -> T` and `<F: FnOnce() -> T, T>` (a bound naming a parameter declared after
  it), and `impl<F, T> Memo<F, T> where F: Fn(u64) -> T` (an impl's closure parameters).
  `fixtures/where_fn_bounds.rs`. (A struct literal's closure doesn't take its parameter types from
  the struct's bound yet: `Memo { f: |x: u64| .. }` needs the annotation.)
- [x] **Nested `fn`s share the file's namespace** (125 / 55): a nested `fn` named like another
  function is renamed `imp__1`, .. with the calls in its block (`compiler/src/nested.rs`), so two
  helpers can each define `fn imp()`, and a block's `fn` hides the file's. `fixtures/nested_fns.rs`.
  (`#[cfg(windows)] fn x` next to `#[cfg(not(windows))] fn x` works too: see target `cfg`, below.)
- [x] **`macro_rules!` robustness** (205 files: 72 expansions that don't parse, 67 "no rules matched",
  66 definitions in unsupported places; first errors now: "no rules matched" 109 → 14, "not
  supported" 40 → 2; `fixtures/macro_robust.rs`):
  - [x] doc comments inside a repetition (`$( /// doc\n $name, )+`): doc comments are
    `#[doc = ".."]` tokens, in the input and in the transcriber (where a `///` used to comment out
    the rest of the line)
  - [x] a `ty` fragment repeated without a separator (`$($t:ty)*`): one type per repetition; also
    an `item` fragment with generics, closures and turbofish commas in an `expr`, `::` separators,
    and repetitions without a separator kept apart (`$($a : $b)*` used to glue `b` and the next `a`)
  - [x] `<$t>::MAX` / `<$t>::method()`: qualified type paths, which failed outside macros too
    (`<u8>::MAX`, `<Vec<u8>>::new()`); a pre-pass writes them as plain paths (`ufcs.rs`,
    `fixtures/qualified_type_paths.rs`)
  - [x] `#[cfg(..)]`-gated alternative definitions of one macro (the extra definitions were left in
    and reported as "`macro_rules!` is not supported"), and a macro defined again
  - [x] `pub(crate) use my_macro;` re-exports in the defining file
  - [x] `std::name!` / `$crate::__priv::name!` aren't the file's `name!` (memoffset recursed)
  - Left: macros of other files that aren't `mod`-loaded (`#[macro_use] mod macros;` in `lib.rs`
    used from a sibling file), bundled `log!` / `bail!` / `anyhow!` shadowing a crate's own
    cross-file macros, and rules relying on finer fragment parsing.
- [ ] **Generic bounds on std traits** (49, down from 82): left: `A: Allocator`,
  `impl Future for ..`. Done: `W: fmt::Write` (given a `String`, a `Formatter` or your own
  writer) and `Vec<u8>: io::Write` (`put(&mut vec)`); `&[u8]` (`"abc".as_bytes()`, `&data[..]`)
  satisfies `R: Read` / `R: BufRead` / `impl Read` / `Box<dyn Read>` parameters and
  `BufReader::new`, read through a `Cursor` the function makes (a `&mut &[u8]` argument, whose
  reads would advance the caller's slice, is an error); `F: Future<Output = T>` with `T` another
  type parameter and a bare `F: Future` (`F::Output`); `S: BuildHasher` (`HashMap<K, V, S>`,
  `RandomState::new()`, ignored). `fixtures/byte_readers.rs`, `fixtures/future_bounds.rs`.
- [x] **Blanket impls on references** (66): `impl<T: Tr + ?Sized> Tr for &T` / `&mut T` / `Box<T>` /
  `Rc<T>` / `Arc<T>` / `Box<dyn Tr>` whose methods each call `T`'s (`(**self).f(a)`,
  `T::f(self, a)`, `self.as_ref().f(a)`) are left out (`&T` is `T` in JS); one that does more is
  an error. `impl<T: Display> Tr for &T` and `impl<I: Iterator> Tr for &mut I` are blanket impls
  over `T`. `fixtures/blanket_ref_impls.rs`. (Impls whose bodies are macro calls, `Pin`
  forwarding, and impls for `Box<Concrete>` remain.)
- [x] **Target `cfg`** (62 / 35): `cfg!(target_os = ..)`, `windows`, `unix`, `target_arch`,
  `target_endian`, `target_pointer_width`, `target_feature`, `miri`, and the matching `#[cfg]` on
  `fn`s. Evaluated against one fixed JS target (`wasm32-unknown-unknown`, not windows / unix,
  little-endian, `target_pointer_width = "64"` like `usize` here), and a pre-pass blanks out
  items, statements, fields, variants and arms under a `#[cfg]` that's off (features count as
  undecided there, so their items stay; a `#[cfg]` inside macro arguments isn't evaluated).
  `fixtures/cfg_items.rs`, `js-node/target_cfg.rs`.
- [x] **`write!` into anything but a `String` or `Formatter`** (63 / 42): `write!(vec_u8, ..)`
  (`io::Write`), `write!(w, ..)` for a generic `W: fmt::Write` or `io::Write`, and a type with its
  own `impl fmt::Write` (`write_str` / `write_char`, `Err(fmt::Error)`). `fixtures/fmt_write.rs`,
  `fixtures/io_write_vec.rs`. Not yet: `&mut dyn fmt::Write`, and a `Vec::new()` with no element
  type that only a generic writer call would make a `Vec<u8>`.
- [x] **Pre-2024 editions** (42 / 37): files parse as the 2021 edition, so `rng.gen()` and
  `let gen = ..` work (gen blocks are unstable anyway). A fixture's `// edition: 2021` line builds
  its native side as 2021 (`fixtures/rand_gen_2021.rs`).
- [x] **`&mut T` with `T` a type parameter** (found on the way, a silent miscompile): `fn bump<T:
  AddAssign>(x: &mut T, d: T)` lost its writes when `T` was a number, string, bool or `Option`. Such
  parameters now take a cell like `&mut i32` does (a getter/setter cell over a struct, field or
  element). `fixtures/generic_mut_refs.rs`.
- [x] Smaller ones (12 to 39 files each). Done: `iter_mut()` outside a `for` (slots over numbers
  and strings, `fixtures/iter_mut_outside_for.rs`; closure adapters such as `filter` on them are
  still an error), `use x as y` renames of std types and modules (crates already worked;
  `fixtures/use_renames.rs`), default associated consts in traits
  (`fixtures/trait_default_consts.rs`), non-literal enum discriminants (`A = B as isize`, a
  constant evaluator; `fixtures/enum_const_discriminants.rs`), `{ .. }` blocks on the right of `&&` / `||`
  (`fixtures/logic_block_operands.rs`); const generics on traits (`trait Buf<const N: usize>`,
  `impl Buf<4> for X`, `impl<const N: usize> Tr for [u8; N]`; the trait's own default methods
  can't read `N` yet; `fixtures/const_generic_traits.rs`).

### Std-library gaps

Files hitting each, among files that compile apart from external names. In all, 888 files miss a std
type, 760 a std method, 357 a std trait impl, and 142 a std macro. After `3c15b78`: 569, 603, 148
and 60, and most of what's left is raw pointers (`as_ptr` and friends), `Future`, the io types
without a script meaning, or crate types named like std ones.

- [x] **Types:** `PhantomData` (232 files), `std::path::{Path, PathBuf}` (146 / 66), `OsStr` /
  `OsString` (34 / 29), `cell::Ref` / `RefMut` named as types (37), `io::Cursor` (36),
  `fmt::Arguments` (16), `NonZeroUsize` (15), `Infallible` (14). Done: `PhantomData` and
  `Infallible` are a Rust-source prelude (`marker_prelude.rs`); `Path` / `PathBuf` are the path
  prelude's `PathBuf` (`path_prelude.rs`: Unix-style, `exists()` is `false`), and `OsStr` /
  `OsString` are `String`, by a pre-pass (`stdgaps.rs`) that also writes `Ref<'_, T>` as `T`,
  `NonZeroU32` as `u32` (`NonZeroU32::new` gives an `Option<u32>`) and `fmt::Arguments` as
  `String`; `Cursor` is in the io prelude. Still open: `fs::File` / `Metadata` (41 / 16),
  hand-written futures (`Pin` 39, `task::Context` 37, `Poll` 20, `Waker` 14), `process::Command`
  (25), `TypeId` (16), `IoSlice` (12), `SocketAddr` (16): none of them means much in a script.
- [x] **Implementing std traits:** `ExactSizeIterator` (87), `FusedIterator` (84),
  `DoubleEndedIterator` (65), `io::Read` (72), `io::Write` (39), `Hasher` (17), `BuildHasher` /
  `BufRead` (11 each), and the markers `UnwindSafe` / `RefUnwindSafe` (20 / 17), accepted and
  ignored. `len()` comes from `size_hint`, `.rev()` runs `next_back`; `Read` / `BufRead` are
  traits of the io prelude with std's provided methods (`read_to_end`, `read_to_string`,
  `read_exact`, `bytes`, `read_line`, `read_until`, `lines`, `split`), which `BufReader` and
  `Cursor` implement; a `Hasher` gets `write_u8` .. `write_i128`, a `BuildHasher` `hash_one`, and
  `HashMap::with_hasher` / `default` take one (keys still compare by value). Still open: `Future`
  (51).
- [x] **Methods:**
  - `Vec`: `into_boxed_slice` (43), `resize` (32), `capacity` / `reserve` / `reserve_exact` /
    `shrink_to_fit` (24 / 18 / 7 / 13, no-ops), `chunks_mut` / `chunks_exact_mut` (18 / 9, views),
    `copy_within` (9), `retain_mut` (9), `splice` (7)
  - arrays and slices: `[T; N]::map` (17), `eq_ignore_ascii_case` (8), `strip_prefix` (8)
  - `Display::fmt` called on an inner field to delegate (`self.0.fmt(f)`, 32), with the
    caller's width, precision and flags; also `fmt::Display::fmt(&x, f)`
  - `char`: `len_utf8` (30), `encode_utf8` (12), `escape_debug` (10)
  - integers: `to_le_bytes` / `to_be_bytes` / `to_ne_bytes` (about 40 combined), `unsigned_abs`
    (18), `next_power_of_two` (9)
  - `str::get(range)` (11), `String::into_boxed_str` (12)
  - `Iterator::size_hint` (53), and `for` over a type's own `iter()` (20)
  - `Option::get_or_insert_with` (11), with `get_or_insert`, `get_or_insert_default` and
    `insert`: `(o ??= v)` on the option's place (`fixtures/option_get_or_insert.rs`).
- [x] **Macros:** `format_args!` (29, a `String` here), `compile_error!` (30: an error unless
  it's under a `#[cfg]` that's off for the JS target), `include!` /
  `include_bytes!` (17 / 14), `env!` / `option_env!` (15: a script has no build environment, so
  `env!` is an error and `option_env!` is `None`, except Cargo's `CARGO_*` variables, which get
  placeholders).

The raw-pointer methods (`as_ptr` 112, `as_mut_ptr` 82, `get_unchecked` 24, `set_len` 21), `asm!`
and `is_x86_feature_detected!` fall under "not planned", above.

### Crates for Bitburner scripts

Of the crates that two or more registry crates import, the useful ones are already bundled (serde,
rand, log, anyhow, rayon, itertools, hashbrown, once_cell, either, num-traits, clap). Most import
errors in the survey were library files reaching into their own crate. What's left worth adding,
cheapest first:

- [x] **`rustc_hash`** (4 crates): `FxHashMap` / `FxHashSet` as std's `HashMap` / `HashSet`
  (`default()`, `with_capacity_and_hasher`, `with_hasher(FxBuildHasher)`; `FxHasher` is
  `DefaultHasher`). `fixtures/crate_rustc_hash.rs`, against rustc-hash 2.
  It's the usual "fast map" import in puzzle-style code (BFS, memoized contract DP).
- [x] **`smallvec`** (3): `SmallVec<[T; N]>` and `smallvec![..]` as `Vec` (a rewrite in
  `bundled.rs`; `from_vec` / `from_slice` / `from_buf` are `Vec::from`, which works now too).
  `fixtures/crate_smallvec.rs`. (`spilled()` / `inline_size()` aren't there: there's no inline buffer.)
- [x] **`indexmap`** (4): `IndexMap` / `IndexSet` (a `Ty::Coll` over `HashMap` / `HashSet`, and
  runtime classes extending them: a JS `Map` already keeps insertion order), with positions
  (`get_index`, `get_index_of`, `get_full`, `insert_full`), `swap_remove` (what `remove` is) and
  `shift_remove`, `first` / `last` / `pop`, `sort_keys` / `sort_by` / `sort`, `reverse`,
  `truncate`, collecting, and `indexmap! {..}` / `indexset! {..}`. `fixtures/crate_indexmap.rs`.
  (`map[i]` by position isn't there yet.)

Not in the corpus (it's library internals), but likely to matter more to players:

- [ ] **`ordered-float`**: `OrderedFloat<f64>` / `NotNan` make `f64` scores usable as `BinaryHeap` and
  `BTreeMap` keys, for ranking targets by money/sec without a hand-written `Ord`.
- [ ] **`petgraph`**: `Graph` / `DiGraph` with its algorithms (shortest paths, components, toposort).

Marginal: `static_assertions`, `tap`, `strsim`, a `base64` alias over `js::btoa` / `js::atob`, and a
`js_sys` alias over the `js` crate. Not useful in a script: proc-macro, test and bench tooling
(criterion, quickcheck, proptest, insta), system and FFI crates, zero-copy and binary serialization,
thread crates, and networking.

## React and `jsx!` (for `ns.printRaw` / `tprintRaw`)

Bitburner scripts draw tail windows and terminal widgets with React: `ns.printRaw(<Dash />)` from a
`.jsx` / `.tsx` script, with `React` and `ReactDOM` as globals (React 17). RapydScript got JSX by
extending its own parser (`from __python__ import jsx`, `src/output/jsx.pyj`, and a `react` module of
hooks). CarciniScript can't do that: it parses with rust-analyzer's parser, so a bare `<div>` would
break the IDE features and the rustc checks of the corpus. A macro's token tree takes JSX-shaped
tokens as they are, though, which is how Yew (`html!`) and Leptos (`view!`) write markup in Rust.
Today there's no way to make a React element: `print_raw` takes a `JsValue`, and a parsed JSON
object isn't an element (no `$$typeof`).

```rust
use react::{jsx, use_state, use_effect, Node};

let cat = move || -> Node {
    let (frame, set_frame) = use_state(|| 0u32);
    use_effect(move || {
        let id = js::set_interval(move || set_frame.update(|f| f + 1), 50.0);
        move || js::clear_interval(id)
    }, ());
    jsx! { <div style="font-family: monospace"><pre>{pose(frame)}</pre> "frame " {frame}</div> }
};
ns.print_raw(jsx! { <cat /> });
```

Design:

- **A bundled `react` crate** (a Rust prelude over runtime helpers, like `regex`): an opaque `Node`
  (`From` strings and numbers, `Option<Node>` → `null`, `Vec<Node>` → an array); hooks with Rust
  signatures: `use_state(|| init) -> (T, SetState<T>)` with `set(v)` and `update(|old| new)` (the
  updater form avoids stale closures), `use_effect(f, deps)` (`f` may return a cleanup closure; deps
  a tuple), `use_ref`, `use_memo`, `use_callback`; `react::h(tag, props, children)` underneath.
- **`jsx!`, a source rewrite** (like `json!`) into calls of that crate, so the embedded
  expressions are type-checked, borrow-checked and compiled as ordinary Rust:
  - Tags: a name bound in scope to a fn or closure is a component; anything else is an HTML tag
    (Babel can only go by capitalization). `<>..</>` fragments, `key=`.
  - Attributes: `name="lit"` or `name={expr}`; `class` → `className`, `on_click` / `onClick` →
    `onClick` (closures become JS functions), `aria-label` / `data-x` joined from their tokens.
    `style="color: red; font-size: 12px"` becomes `{color: "red", fontSize: "12px"}` at compile time.
  - Children: string literals (Rust's lexer drops whitespace and breaks on `don't` in bare text,
    so text is quoted, as in Leptos) and `{expr}` (`impl Into<Node>`, `Option`, `Vec` / iterators).
  - Components with props: `<Gauge label="hp" value={x} />` calls `fn Gauge(label: &str, value: f64)
    -> Node`; the props object is keyed by parameter name. `κσ.$component(f, names)` wraps it, cached
    per function so React keeps its identity (closure components capturing `ns` and local state,
    like a tail-window dashboard's, are the common case).
- **Bindings:** `dts2rs` maps `ReactNode` / `ReactElement` to `react::Node`, so `print_raw` /
  `tprint_raw` take `impl Into<Node>` (`ns.print_raw("text")` still works).
- **rustc checks:** the corpus' stub crate has `jsx!` as a `macro_rules!` taking any tokens.

Steps (the prototype from branch `jsx-prototype` / worktree `../carciniscript-jsx` is merged
into the main tree):

- [x] Runtime helpers (`κσ.$react`: `h`, `component`, `static`, style parsing) on
  `globalThis.React`, and the `react` prelude (`compiler/src/react_prelude.rs`: `Node`, `h`,
  `use_state`, `use_effect`, `use_ref`) over private `js::__react_*` bindings.
- [x] `jsx!` rewrite (`compiler/src/jsx.rs`): tags, quoted text, `{expr}` children, attributes,
  closure and fn components (a `let` of a closure or of a block ending in one), `children`,
  fragments, `key`, `jsx!` inside `{..}`; errors for unquoted text, unknown components, missing or
  unknown props, mismatched closing tags.
- [ ] Bindings: `print_raw` / `tprint_raw` take a `Node` (they take a `JsValue` today, which
  already accepts one).
- [x] Tests (`test/react/run.mjs`, `npm run test:react`): each fixture has a `.jsx` twin compiled by
  TypeScript; both run against a mock `ns` and what they print is rendered with React 17: static
  markup, mounted in jsdom with 60 ms of a fake clock, then each button clicked; the transcripts must
  match. Plus 10 diagnostics, error positions, and a component from a `mod` file.
- [x] Examples: the web REPL's "React: a tail-window dashboard" (`test/react/hack_monitor.rs`, run
  on the page with React 17 from jsdelivr, checked by `tools/browser-check.mjs`), and the corpus'
  `cat.rs` and `monitor.rs` (a live table of rooted servers).
- [x] Error positions inside embedded expressions: the rewrite leaves each `{expr}` where it was
  written and edits the markup around it (`jsx::edits`).
- [x] Components from `mod` files (`pub fn` components, found through the module resolver).
- [x] `React` missing outside Bitburner: a panic that says how to load it.
- [ ] Still open: typed children (`IntoNode` needs trait impls for `&str` / `Option<T>` / `Vec<T>`,
  which the compiler only allows for the file's own types today), hover and completions inside
  `jsx!`.
- [ ] Later: typed intrinsic attributes and DOM events, `use_context` / `use_reducer` / `use_memo`,
  `{..spread}` props, class components (not planned).

## Testing

- **Differential:** a corpus of subset programs runs under native `cargo run` and as transpiled JS
  under node; stdout must match.
- **Source maps:** assert specific Rust lines map to the expected JS.
- **Bitburner:** RAM cost and import integration tests.

## Risks

- **Type inference scope creep:** iterator chains with closures need generic inference over std
  signatures. Keep the std surface small and grow on demand.
- **Silent divergence:** `Copy` structs, `&mut` primitives, UTF-8 vs UTF-16, `i64` precision.
  Every one goes in `RUST_GAPS.md`.
- **No full borrow checker:** programs rustc rejects can still run. Documented.
- **ra_ap_syntax churn:** pinned version; upgrades are deliberate.
