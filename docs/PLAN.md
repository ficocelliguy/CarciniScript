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
   borrows make them errors. Also through calls of the file's own functions and methods (elided
   and named lifetimes in their signatures), down to fields (disjoint fields don't conflict), and
   ended by a `break` / `return` right after the change. Not yet: closures, struct literals,
   references handed between locals, loop-carried borrows, `mod` files' functions.
3. **NLL proper** (sketched 2026-11-17, below): rustc's shipping borrow checker is NLL, not
   Polonius, and NLL is intra-procedural, so each function is checked and cached on its own.
   That is what makes a full checker fit a browser compiler and a language service.

Optional debug mode: RefCell-style runtime guards on collections (e.g. mutate-while-iterating
panics instead of silently misbehaving).

### NLL sketch

The algorithm is not the hard part. The hard part is that the compiler erases `&T` to `T` before
anything downstream sees it (`typeck.rs` `lower_type`; `Ty` has no `Ref` variant, `MutRef` only
for closure parameters and cells), and a borrow checker needs reference structure for every local.

**What has to exist that doesn't today**

- *A per-function IR with explicit places and a CFG* ("BIR"). Everything today walks the
  ra_ap_syntax CST keyed by byte ranges. Borrow checking needs basic blocks, temporaries for
  subexpressions, places as a local plus projections (field, deref, index, enum downcast), and
  `StorageDead` at scope ends. Patterns lower to switches plus downcast projections, `?` to a
  branch and return, loops to back edges, closures to separate bodies with upvar places.
- *Reference-aware types for BIR locals* ("RTy"): the typeck `Ty` shape with
  `Ref(region, mut, inner)` nodes reinstated and region inference variables attached. ADTs with
  lifetime parameters (`Parser<'a>`) carry regions as generic arguments.
- *A borrow table for the std surface*: which input a result borrows from (`iter`, `get`,
  `first`, `as_str`, `chars`, `split_whitespace`, `entry`, `iter_mut`, `windows`,
  `HashMap::get`, the std iterators' `Item` types, ..). `borrows::ref_sig` already does this for
  the file's own functions by rustc's elision rules; it generalizes to a table of signature text.
- *Liveness and region inference.* Backward liveness per local over the CFG; drop-liveness only
  for ADTs with an `impl Drop`. Each region is a set of CFG points. Constraints `'a: 'b` from
  assignment, call instantiation (fresh regions per call), return, and reborrow propagate point
  sets to a fixed point. The signature's named and elided lifetimes are universal (free) regions.
- *Loans and conflict detection.* Each `&p` / `&mut p` is a loan live over its region's points,
  killed by an assignment to a prefix of `p`. Every access is checked against the active loans:
  E0499, E0502, E0505, E0506 within the function; E0597, E0515, E0716 when a loan's region
  reaches the place's `StorageDead` or a free region.
- *Move dataflow on the same IR*, replacing `moves.rs`: maybe-uninitialized places over BIR
  gives use after move, partial moves and definite initialization with less special casing than
  the CST walker, and supplies `move_sites` for `Drop` codegen as today.
- *Two-phase borrows and implicit reborrows.* `v.push(v.len())` and handing a `&mut` local on
  to a `&mut` parameter are everywhere in the corpus; without these the checker false-positives
  constantly.
- *Closure capture analysis.* Rust 2018 whole-variable captures first; RFC 2229 precise path
  captures later.
- *Variance* computed from field types by a fixpoint over ADTs (`&mut T` and interior
  mutability make `T` invariant); cheap, and needed to avoid both false positives and holes.

Deliberately left out of "most of one": Polonius location sensitivity, higher-ranked bounds
(`for<'a>`), `dyn Trait + 'a` object lifetime defaults, drop-check subtleties (`#[may_dangle]`),
`T: 'a` well-formedness on struct definitions, `impl Trait` capture rules (treat as capturing
everything). `async` bodies are checked as if synchronous (`.await` is a call), which matches
rustc for non-`Send` futures. Unknown shape (a `T` instantiated with a reference the overlay
can't see) defaults to owned: silence, never a false positive, as the current checks do.

**The one real design decision: where reference-ness lives**

- *Option A, an overlay.* BIR lowering rebuilds `Ref` nodes from syntax (`&e`, `*e`),
  annotations, user signatures, the std table, autoref/autoderef at method calls and match
  ergonomics (default binding modes: `for (k, v) in &map`, `if let Some(x) = opt.as_ref()`),
  and runs a small unification restricted to ref-or-not shape. Keeps the erasure behaviors the
  rest of the compiler depends on (one impl serving `T` and `&T`; `String` and `&str` one JS
  type; the `&mut` cell and write-back machinery). Deletable if it fails.
- *Option B, `Ty::Ref` in the type checker*, erased at codegen. One inference instead of two,
  and it would close some gaps that come from erasure. Touches the 9k-line typeck and every
  `Ty` match across ~30 files.
- Recommendation: A first. Promote to B only if the overlay's duplicated method resolution
  becomes the dominant maintenance cost.

**Performance (browser compile and language service)**

- *Function-granularity caching.* A function's result depends only on its expanded text, the
  signatures of the callees it names and the ADT definitions it touches. Hash those and keep
  results across edits: `memo.rs` one level down, so a keystroke re-checks one function.
- *Dense indices, no hashing per point.* Lower once to arrays indexed by `LocalId`, `PointId`,
  `RegionId`, `LoanId` (u32); regions as word-packed bitsets over points. Functions here are
  typically under a few hundred points, so the fixpoint is microseconds to low milliseconds.
- *Run after typeck, only on functions with no type errors* (incomplete types make garbage
  loans). The service already runs one analysis per edit for hover, completions and signature
  help, so borrow diagnostics ride along at no extra parse cost.
- *A budget escape hatch.* Functions above a point threshold fall back to NLL-lite or skip with
  a note, so a pathological file can never make the editor stutter.
- *No new crates.* The wasm is 5.9 MB; bitsets and index newtypes are a few hundred lines
  in-house. Expect roughly 300 to 500 KB more wasm.
- Target against the performance scan's numbers: under 10 ms cold for `darknet.rs` (78 ms
  compile today), near zero for an unchanged function in the service.

**Staging**

1. BIR lowering, CFG, liveness. Port move checking onto it and retire `moves.rs`; the existing
   move fixtures validate the IR before any region work starts.
2. RTy overlay, std borrow table, constraint generation, NLL inference, loans. Retire
   `borrows.rs`. Closes every gap listed in RUST_GAPS "Checking": loop-carried borrows,
   local-to-local handoff, struct literals, closures, other modules' functions.
3. Dangling and escape errors (E0597, E0515, E0716), struct lifetime parameters, two-phase
   borrows, reborrows, closure captures.
4. Per-function caching, service wiring, and strict borrows as the default once the
   false-positive rate is measured.

**Test oracle.** Every corpus file rustc accepts must produce zero borrow errors, so
`npm run corpus` is a false-positive counter. For true positives, rustc's own
`tests/ui/borrowck` suite with its `.stderr` files covers the subset well, and `record-native`
can diff messages; keep rustc's exact wording, as `borrows.rs` does, since the editor shows it.

**Size.** 5 to 7k lines of Rust: BIR lowering 2 to 3k (every expression form in the subset),
region typing and inference ~1.5k, std table and diagnostics under 1k each. About three times
the current `moves.rs` + `borrows.rs`, under a tenth of the compiler.

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
     adapter chains), `x % 7` / `x / 7` without the divide-by-zero helper. Compile time was linear
     (about 1 ms per KB); it no longer is, see "Performance scan", below
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
     the call doesn't say them); traits and enums ✅
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
hide later errors, so this is a fair per-file check. To re-run it: `npm run corpus` (about 5
minutes; results in the OS temp dir). Its three steps:
- `tools/corpus-classify.mjs [root] [out.jsonl]` compiles each file and sorts each error (another
  crate or file, thread, std, language, parse, or uncategorized), with its byte range
- `compiler/examples/corpus_ast.rs` parses each file with rust-analyzer's parser and adds the
  Rust features it uses (about 90 syntactic detectors, so a feature counts even where an
  unresolved name stops the compiler first), the construct each error is on, and the names
  from other crates or files in that construct, its statement, its function's signature and its
  `impl`'s header. Each feature has a probe program (all accepted by rustc, 2021 edition); a
  feature whose probe doesn't compile is missing. `--probes` prints just those results
- `tools/corpus-report.mjs` reclassifies an uncategorized error as downstream of another crate
  or file when such names are involved, or as a language gap when it's on a missing feature
  or is a `#[cfg]`'d item defined twice, then prints the verdicts, language gaps, every feature
  with its use and probe result, what's still uncategorized by message and construct, and the
  crates that pass in every file, and writes a per-file CSV

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
- **Re-run after `db63e10`**: language features all supported in **74.5%** (3,888 of 5,220);
  counting std gaps, 61.5%; by lines, 52.2%. Files with a language gap fell from 1,594 to 1,242
  (of which 611 are unsafe/memory, not planned), std-only files rose from 594 to 678 as language
  fixes uncovered them. Gone from the table: nested `fn` shadowing, target `cfg`, `gen`, block
  operands, trait default consts, enum discriminants; "macro_rules matching" fell 67 → 10. std
  source: 60.0% (from 55.5%). 35 crates pass in every file (from 25). Of the plan's language repros, all pass except those listed below.
  The leading "impl on a std type" rows (273 files) are now mostly std traits on std types
  (`impl<T> Index<Id> for Vec<T>`, `impl PartialEq<N> for str`, `impl Default for Box<..>`), a
  trait on `Box<str>`, or the crate's own types named like std ones (anyhow's `Chain`).
- **Re-run after `1c180cd`, on the uncommitted tree** (2026-11-17): language features all
  supported in **79.0%** (4,126 of 5,220); counting std gaps, 69.7%; by lines, 61.6%. 1,009 files
  have a language gap (631 of them unsafe/memory, not planned), 488 are std-only, and 64 have other
  errors with nothing unresolved (15 methods on unknown types, 11 type mismatches, 10 `env!`).
  49 crates pass in every file (from 35); 132 pass in 80% or more. Two causes:
  - The classifier (now `tools/corpus-classify.mjs`) no longer counts a crate's own types and
    traits with std names as std ones: a name from a `crate::` / `self::` / `super::` path, from
    another crate's `use`, or that a `struct`, `enum`, `union` or `trait` in the crate defines
    (aliases don't count: `type Result<T> = std::result::Result<..>` is still std's). That moved
    about 85 files out of "language gap" and 223 out of "std-only" (anyhow's `Chain` / `Ref` /
    `Backtrace`, `crate::File`, `crate::Path`, `syn::Path`). Earlier runs' "impl on a std type"
    and std-type counts include these.
  - Compiler changes since `db63e10`: "type parameter in this position" fell 211 → 24 files and
    "generic trait implemented more than once" 49 → 0.
  The open items are under "Open from the 2026-11-17 re-run" in Language gaps, below.
- **Re-run after `049fcee`, on the uncommitted tree, with the syntax step** (2026-11-17): language
  features all supported in **81.8%** (4,272 of 5,220); counting std gaps, 72.2%; by lines,
  64.9%. 896 files have a language gap (631 unsafe/memory, not planned), 505 are std-only, 44
  have only uncategorized errors (from 64). 70 crates pass in every file (from 49); 153 in 80%
  or more. Uncategorized errors fell from 3,987 file hits to 1,814: most were downstream of a
  type, method or trait in another file of the crate (a method that file's impl defines, a field
  of its struct, `self` in an `impl` of its type). New items are under "Open from the syntax
  re-run", below. Counts left in the gaps above: a trait on a std type 63 / 32 / 38, generics in
  this position 29 / 12 / 24, writes through references 28 / 18 / 17, a type parameter in this
  position 26 / 16 / 13, std-trait bounds 14 / 9 / 8, format strings 8 / 4 / 8, const generics
  6 / 1 / 5, renames 4 / 3 / 4, macros 7 files.
- [x] **Regressions in `3c15b78`**, both in files that compiled before:
  - a user `enum Ordering` in a pattern is taken as `std::cmp::Ordering`: "expected `Ordering`,
    found `Ordering`" (`ra_ap_syntax` `ast/operators.rs`); a file's own `Ordering` now shadows
    std's in patterns too (`cmp::Ordering::Less` still means std's)
  - `#[derive(Clone, Copy)] struct W<'a> { s: &'a [u8] }` gives "the trait `Copy` is not
    implemented for `Vec<u8>`" (`miniz_oxide`); a shared-reference field (`&'a T`,
    `Option<&T>`) is `Copy` / `Clone` whatever it points at. 10 more registry/std files with
    `&'static str` / `&[u8]` fields in `Copy` types compile (`fixtures/shadowed_std_names.rs`)
- [x] **Found in the `db63e10` re-run** (all done):
  - Nested `fn` renaming in files with no other pre-pass trigger: the gate in `codegen.rs` checks
    `nested::has_duplicate_fns` now (`fixtures/nested_fns_plain.rs`).
  - One generic trait implemented more than once for a type (`impl IntoS<u8> for Vec<u8>` and
    `impl IntoS<u16> for Vec<u8>`): the later impls' methods get keys of their own (`into_s$1`),
    and a call on the concrete type picks the impl from its arguments or the expected type
    (`fixtures/generic_trait_twice.rs`); calling one through a type parameter or `dyn` is an
    error (it couldn't be told apart at run time). The same impl twice under opposite feature
    gates (`#[cfg(feature = "x")]` / `#[cfg(not(feature = "x"))]`): the `not(..)` one stays, as a
    script has no features (`fixtures/cfg_feature_twins.rs`).
  - Hygiene names leaking (`Vec__h1`): the hygiene pass took `impl Tr for Vec<T>` in a macro body
    for a `for` loop and renamed `Vec`; only `for pat in` binds now (`fixtures/macro_impl_for.rs`).
  - `#[derive(Debug)]` on a struct holding `Box<dyn Error>` (`dyn Error` is `Debug` and
    `Display`), and `"x".into()` / `format!(..).into()` as a `Box<dyn Error>` (the string itself)
    (`fixtures/box_dyn_error.rs`).
  - `<Token![,]>::default()`: one parenthesized path in a qualified path (`<(Comma)>::default`)
    is that path (`fixtures/macro_type_qualified_path.rs`).
  - `indexmap`: `get_index_mut` (see the crates section).
  - From the Bitburner corpus: `.into()` on an `impl Into<H>` / `H: Into<Host>` parameter with no
    other hint is the bound's target (and `.into()` no longer looks through `Deref`); `let r =
    &mut <element or field>` of a number, string, bool or `Option` is a slot over that place,
    made once (`κσ.$placeSlot`; `fixtures/place_refs_into.rs`). Re-pointing an existing `&mut`
    variable at such a place is still an error.

### Language gaps

Counts are files that hit the gap, then files where it's the only language gap. Status: `[x]`
done · 🚧 in progress · `[ ]` open (the same 🚧 as FEATURES.md's legend).

- [x] **Trait impls on std types** (336 / 182): your traits on `Option<T>`, `Result<T, E>`,
  `HashMap` / `BTreeMap`, `HashSet` / `BTreeSet` and `u128` / `i128` (generic impls too, through a
  generic `T: Tr` for all but `Option`), and `impl From<A> for String` / for a number type
  (`String::from(a)`, `a.into()`, `impl Into<String>`). `fixtures/std_type_impls.rs`.
  - [x] The rest: your traits on `Box<str>` / `Rc<[u8]>` / `Arc<String>` (a box is its contents),
    `Path` and `PathBuf` (or `str` and `String`, `[T]` and `Vec<T>`) implemented together (the
    borrowed type's impl serves; `fixtures/owned_borrowed_impls.rs`), `std::num::Wrapping` (a
    prelude, `fixtures/std_wrapping.rs`), operator traits on numbers with your type as the other
    operand (`impl Mul<V2> for f64`, `2.0 * v`) and `impl PartialEq<Id> for &str` / `for String`
    / `impl PartialEq<u32> for Id` (`"home" == h`, `id != 3`; `fixtures/std_trait_builtins.rs`),
    an `Option` through a generic `T: Tr` (`None` always, `Some(x)` unless `x`'s own type has the
    method; a nested `Option<Option<_>>` there doesn't print its inner value right), and `&mut
    self` methods of your traits on a `String`, number, bool, char or `Option` (`this` is a cell;
    `fixtures/ext_mut_self.rs`). `File` is the no-OS prelude's (see Std-library gaps).
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
  - [x] Macros of other files, by Rust's textual scoping: a module file sees the macros above its
    `mod` line in the file that declares it (and that file's `#[macro_use] mod`s), up to the
    crate root, and every `#[macro_export]` one (`crate::m!`, `use crate::m;`); a crate's own
    `info!` then beats the bundled `log`'s (`modules/macro_scope`).
  - Left: rules relying on finer fragment parsing (since done: fragments are parsed by
    rust-analyzer's parser, see "`macro_rules!` expansions that don't parse" below).
- [x] **Generic bounds on std traits** (49, down from 82): `A: Allocator` (ignored, with
  `Vec<T, A>`, `Global`, `Vec::new_in`; `fixtures/allocator_params.rs`, built natively with
  `RUSTC_BOOTSTRAP` for its `#![feature(allocator_api)]`) and `impl Future for ..` (hand-written
  futures, `fixtures/hand_written_futures.rs`). Before: `W: fmt::Write` (given a `String`, a `Formatter` or your own
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
  `fixtures/io_write_vec.rs`. `&mut dyn fmt::Write` / `&mut dyn io::Write` parameters (written
  as type parameters by a pre-pass) and `Box<dyn io::Write>` (`fixtures/dyn_writers.rs`). A
  `Vec::new()` that only a generic writer call types is a `Vec<u8>` (the trait's one `Vec` impl
  names it). Not yet: a `&mut dyn Write` struct field.
- [x] **Pre-2024 editions** (42 / 37): files parse as the 2021 edition, so `rng.gen()` and
  `let gen = ..` work (gen blocks are unstable anyway). A fixture's `// edition: 2021` line builds
  its native side as 2021 (`fixtures/rand_gen_2021.rs`).
- [x] **`&mut T` with `T` a type parameter** (found on the way, a silent miscompile): `fn bump<T:
  AddAssign>(x: &mut T, d: T)` lost its writes when `T` was a number, string, bool or `Option`. Such
  parameters now take a cell like `&mut i32` does (a getter/setter cell over a struct, field or
  element). `fixtures/generic_mut_refs.rs`.
- [x] Smaller ones (12 to 39 files each). Done: `iter_mut()` outside a `for` (slots over numbers
  and strings, `fixtures/iter_mut_outside_for.rs`; closure adapters too: `filter`, `map`, `find`
  (also kept in a local), `fold`, `max_by_key`, `skip_while`, `flat_map(|row| row.iter_mut())`, a
  `for` over a filtered one, `max()` / `min()` by value; `fixtures/iter_mut_adapters.rs`), `use x as y` renames of std types and modules (crates already worked;
  `fixtures/use_renames.rs`), default associated consts in traits
  (`fixtures/trait_default_consts.rs`), non-literal enum discriminants (`A = B as isize`, a
  constant evaluator; `fixtures/enum_const_discriminants.rs`), `{ .. }` blocks on the right of `&&` / `||`
  (`fixtures/logic_block_operands.rs`); const generics on traits (`trait Buf<const N: usize>`,
  `impl Buf<4> for X`, `impl<const N: usize> Tr for [u8; N]`; the trait's own default methods
  can't read `N` yet; `fixtures/const_generic_traits.rs`).

#### Open from the 2026-11-17 re-run

Files hitting each, then files where it's the only language gap, then crates. Each item gives the
first run's counts; "now" is the second run (fixed classifier, compiler as of `1c180cd` plus
uncommitted work). Unsafe/memory (now 631 / 412 / 112), `union` (126 / 3 / 8), `extern` blocks
and their items (66 / 28 / 16) and the proc-macro API outside a proc-macro crate (55 / 51 / 12)
aren't listed (not planned).

- [x] **Your trait on a std/built-in type, what's left** (228 / 153 / 79):
  - [x] impls on `VecDeque` (their own `$ext$VecDeque` class and run-time kind, apart from a
    `Vec` impl, also through a generic `T: Tr`), on `Box<Node>` / `Rc<Node>` / `Pin<Box<Fut>>`
    (the contents' impl), and a `Box<dyn Tr>` struct field declared before the trait
    (`fixtures/more_std_type_impls.rs`). `Option`, `HashMap` / `HashSet`, `Vec`, `str` / `&str`
    and forwarding `impl<'a, T: Tr> Tr for &'a T` already worked (probed against rustc).
  - [x] A silent miscompile: `impl Tr for String` next to `impl Tr for str` (or `Vec<T>` next to
    `[T]`) was always dropped for the borrowed impl, even when its body did something else. Now
    it's dropped only when each method hands on to the borrowed one (`self.as_str().f()`,
    `Tr::f(&self.as_str())`, `self[..].f()`), and is an error otherwise (zeroize's `Vec` impls,
    which also truncate, now say so).
  - [x] `impl From<X> for Box<dyn Tr>`: its `from`s go on a `$ext$dyn$Tr` class, picked by the
    source type at `x.into()`, `Box::from(x)` and `<Box<dyn Tr>>::from(x)`. Your traits on
    `Range<T>` / `RangeInclusive<T>` (`$ext$Range`, a `range` run-time kind for generic calls),
    with `impl PartialEq<Span> for Range<usize>` (`r == span`) and `impl From<Span> for
    Range<usize>`; a literal range is a `Range` value when it calls one of those methods, is
    borrowed (`&(1..2)`) or is an array element (`fixtures/box_dyn_from_range_impls.rs`).
  - [x] Many of the counted hits were the crate's own types named like std ones (anyhow's
    `Chain`, `crate::File`, `crate::Path`). The classifier is in the repo now
    (`tools/corpus-classify.mjs`, `tools/corpus-report.mjs`) and drops them (see the re-run
    bullet above): **now 113 / 75 / 52**. Most remaining hits are impls a `macro_rules!` body
    writes for numbers, arrays and `Vec` (`vectors! {..}` 240 hits, `array_impl!(..)` 106,
    wasm-bindgen's `type_wasm_native!` / `type_abi_as_u32!` 130, `impl_values!` 39,
    `ref_by_copy!` 18), so the error points at the macro call. Written out: `Option` (26),
    `Vec` (25), `String` (18), `Box` (17), `str` (14).
  - [x] Re-run (2026-11-17, 116 files): 1,050 of the 1,200 hits were impls of a trait the file
    can't see (declared in another file of the crate, like defmt's `Format` in `impls/*.rs` or
    wasm-bindgen's `IntoWasmAbi` in its `vectors!` / `type_wasm_native!` expansions). The error
    named the built-in type ("cannot find type `u8`"); it now names the trait ("`impl Format
    for ..` is not supported yet"), as for a struct, which the classifier counts as another
    file's name. The same shape loaded with `mod` (trait in `fmt.rs`, `impl fmt::Format for u8`
    in the script) already compiled, but was a silent miscompile: the module's own generic
    functions (`pub fn describe<T: Format>(x: &T)`) and the trait's default methods called
    through them didn't find the script's impls (`x.twice is not a function`). A file with
    `pub` traits now exports its registry (`$extRegs`), calls their methods through it, and a
    file loading it registers its impls there too (`κσ.$extRegIn`); the trait's default
    methods are mixed into the built-in type's class (`modules/ext_impls`).
  - [x] Tuples: `impl Area for (f64, f64)`, `impl<A: Encode, B: Encode> Encode for (A, B)`,
    nested, with default methods and through a generic `T: Tr` (a `$ext$Tuple2` class, picked
    by the array's length; `fixtures/tuple_trait_impls.rs`). Their impls used to be dropped
    without a word. One trait for both a tuple and a slice / array / `Vec` is an error: a
    call through a type parameter can't tell them apart.
  - Measured on the 53 affected crates (1,762 files): files with the gap 116 → 66, only gap
    75 → 32; no file that compiled stopped compiling. Left: std types with no value here
    (`RangeFrom` / `RangeTo` / `RangeFull` / `RangeToInclusive` 7 files each, `CStr` /
    `CString`, `Cell`, `LinkedList`, iterator adapters like `Peekable` / `Chars` / `Keys`,
    `TcpStream`, `Instant`, `Wrapping`, `ControlFlow`), the unsafe ones (`MaybeUninit`,
    `NonNull`, `ManuallyDrop`, `Layout`), `impl PartialOrd<Bytes> for str` (only `PartialEq`
    takes your type as the argument), tuple and `Vec` impls of one trait (borsh), and rayon's
    `impl IntoParallelIterator for &[T; N]` / `&Vec<T>` inside rayon itself.
- [x] **A type parameter in an unsupported position** (211 / 113 / 83), "type `T` is not supported
  yet" (`fixtures/type_param_positions.rs`). Registry files whose first error was a one-letter
  "type `T` is not supported yet" fell from 22 to 1, and 8 more files compile, with no file that
  compiled before failing:
  - a type parameter as a trait's type argument (`impl<T> Store<T> for Bag<T>`,
    `impl<'a, T, U> Alloc<T> for Arena<'a, T, U>`): the impl's parameters are in scope for its
    trait arguments and associated types; a generic call's `S: Store<T>` settles `T` from `S`'s
    impl (when there's one), as rustc infers it
  - an associated type set to a parameter (`type Item = T;`): `C::Item` of a `Stack<i32>` is `i32`
  - `Item = P` in a bound with `P: Into<i64>`: it compiled, but `p.into()` stayed a JS number
    and failed mixing with BigInts (`T: Into<i64>` too, and `impl Into<u128>` given `3u8`). Now
    `.into()` to a 64/128-bit kind makes a BigInt, and a `bool` into a number is 0 or 1
  - `F: FnOnce() -> T` in a trait or impl method's `where` clause, and any bound naming a
    parameter declared after it (`F: FnOnce() -> P, P: Into<PathBuf>`, `T: Fn(i32) -> A, A:
    Iterator`): parameters are lowered in the order their bounds need, on fns and methods
  - left: `I: IntoParallelIterator<Iter: IndexedParallelIterator, ..>` (associated type bounds),
    generic associated types (`type List<'a, T: 'a> = &'a [T];`)
  - now 24 / 13 / 13, most of them inside macro expansions: rayon's `impl_unzip_iter!(A =>
    FromA, ..)` (63 hits) and rand's `uniform_int_impl! { i8, u8, u32 }` (8 per invocation)
- [x] **One generic trait implemented more than once, remaining shapes** (49 / 18 / 28):
  several `TryFrom`, `FromIterator` and `Extend` impls on one type, picked by the argument's (or
  items') type at `T::try_from(x)`, `x.try_into()`, `.collect()` and `t.extend(..)`
  (`fixtures/std_traits_twice.rs`). Impls whose types differ only by references (`TryFrom<&str>` /
  `TryFrom<String>` / `TryFrom<&String>`, `Extend<&'a T>` / `Extend<T>`) are told apart by how the
  argument is written (`s.as_str()`, `s.to_string()`, `&s`, `v.iter()`, `vec![..]`), and a call
  never picks the impl it's in, so `Self::try_from(s.as_str())` in the `String` impl hands on.
  Not looked at: `extend_one` / `extend_reserve` (nightly), `TokenStream` impls in proc-macro
  crates.
- [x] **Writes through references into containers** (34 / 21 / 21)
  (`fixtures/writes_through_refs.rs`). `buf[a..b].copy_from_slice(..)`, in-place methods on a
  range of a `Vec` and `dest = &mut dest[n..]` already worked (probed against rustc). New:
  - `v.get(a..b)` / `dest.get_mut(n..)`: `Some` of the slice (a view for `get_mut`), `None` past
    the end, so `dest = dest.get_mut(n..).ok_or(..)?` re-slices
  - silent miscompile: `*output = vec![..]` through a `&mut Vec` parameter left the caller's
    `Vec` alone (only structs were replaced in place); now `Vec`s, maps, sets and the other
    collections are too
  - silent miscompile: a closure taking `&mut` to a number or string (`F: FnMut(&mut String)`
    with `|f| write!(f, "-")`, `|v: &mut i32| *v += 1`, `Box<dyn Fn(&mut i64)>`, a function item
    called through a variable) wrote to a copy. Such a parameter is now a `Ty::MutRef` in the
    closure's type: the closure takes a cell and a call passes one, also for `F: FnMut(&mut T)`
    over structs or numbers
  - now 28 / 19 / 17: a range of a range (`buf[current..][..length].fill(x)`), `let Some(r) =
    &mut self.0 else { .. }`, slices of a method's result (`self.buf.write_buf()[..n]
    .copy_from_slice(..)`), and `write!` into a closure's writer (which may predate the fix in
    the wasm the re-run used)
- [x] **Generic calls on type parameters** (29 / 5 / 25) (`fixtures/generic_param_calls.rs`):
  `T::len(x)` / `T::drive(t, v)` (a bound's method with `self`, by path) is a method call on its
  first argument, as `Trait::len(x)` is; a `&mut` cell argument of a path call
  (`Trait::add_to(&x, &mut n)`) now passes its cell. `A::size()`, `S::default()` passed to a
  constructor and `M::INFO.label(..)` already worked. `P::value_parser()` is clap's, not looked
  at. Now 29 / 14 / 25: mostly rayon's `impl_unzip_iter!` expansions, then hashbrown's
  `Self::with_capacity_and_hasher_in(.., S::default(), A::default())` and rayon's
  `T::drive_unindexed(self, consumer)` / `T::opt_len(self)` (which may predate the fix in the
  wasm the re-run used).
- [x] **`macro_rules!` expansions that don't parse** (first run: not counted, a bug in the tally;
  now 77 / 77 / 16, every one the file's only language gap): thin-vec's `array_impls!` (128
  hits), serde_json's `number_partialeq_ok!` (72) and `json!` (24), time's `assert_errs!` (60),
  bitvec's `safe!` (27), autocfg's `try!` (23), serde_derive's `quote_block!` (22), syn's
  `snapshot!` (18 per version) and `Token!` (13). (`fixtures/macro_fragments.rs`,
  `fixtures/try_macro_2015.rs`, `fixtures/own_try_macro_2015.rs`.) Registry re-run with this
  classifier: 87 / 87 / 17 → **10 / 10 / 3**, and the 10 are files rustc rejects too
  (ra_ap_parser's `test_data/**/err` (5), serde_json's `tests/ui` (4)) and anyhow's
  `ensure!(&0 as &dyn EqDebug<i32, Assoc = bool> != &0)`, which rust-analyzer's parser doesn't
  take. No file that compiled before fails (one more compiles).
  - `expr`, `ty`, `pat`, `pat_param`, `path`, `stmt`, `item`, `meta` captures go through
    rust-analyzer's prefix parser (`ra_ap_parser`, the parse rustc's matcher does), falling
    back to the token scan when it reports an error: `$($N:expr)*` of `0 1 2 ..`, a qualified
    path's commas (`<R<(), ()> as Clone>::clone(..)`); a `pat` the rule follows with `|` is a
    `pat_param` (itertools' `|$it:pat|`, 2018 code)
  - written-out captures stay one piece of syntax: no parentheses around a block `expr` (`$body
    $i += 1;`), parentheses around a `ty` before `<` (`as $t << 4`); repetitions don't glue into
    `"x"as` (a suffixed literal) or `ident#` (a reserved prefix)
  - an expression-position expansion's trailing `;` is dropped (multipart's `return Err(..);`)
  - a matcher or transcriber `$` with nothing after it is the token `$` (syn's `[$] => ..`)
  - `<Token![,]>::default()` is left as it is by the `<T>::f` rewrite
  - a macro named `safe` (`Safe` before parsing) and 2015-edition `try!` (`r#try!`; std's is
    `$e?`, a file's own `macro_rules! try` wins)
  - `json!`'s values keep a qualified path's commas
  - `//!` docs and `#![..]` of an inline module are dropped when it's flattened (they ended up
    after other items: simd-adler32)
- [x] **`macro_rules!` with no rule matching** (likewise not counted the first time; now 12 / 12 /
  9): "rules relying on finer fragment parsing", left from the macro work above. Registry re-run:
  13 / 13 / 10 → **4 / 4 / 3**. Fixed: a `tt` takes joined punctuation as one token (`$delim:tt`
  of `=>`); hygiene doesn't rename a rule marker (`@closure` next to `let closure`:
  wasm-bindgen's `closures!`); calls inside `quote!` / `parse_quote!` aren't expanded (icu's
  `langid!(#repr)`, tinystr); a bundled crate's macro gives way to the crate's own (also
  `macro_rules! log_enabled` no longer hides the bundled `log!`); the bundled `log` takes
  `logger: ..`; `parse_macro_input!(input)` typed by its `let` (clap_derive). Left: anyhow's
  `ensure!()` UI test (rustc rejects it), and regex-automata's `log! { .. }` (2) and wasmparser's
  `bail!(offset, ..)` (1), which are the crate root's macros: the survey's resolver only reads
  files next to the one compiled, so they get the bundled `log` / `anyhow` ones.
- [x] **Bounds on std traits not yet accepted** (27 / 19 / 13; now 14 / 10 / 8)
  (`fixtures/std_trait_bounds.rs`):
  - `UnwindSafe` / `RefUnwindSafe` bounds are markers (all 140 hits are time's `meta.rs`)
  - `T: TryFrom<X>`: `T::try_from(x)` is `Result<T, T::Error>` (`T::Error` unchecked, also in
    a signature). The caller's class decides at run time (`κσ.$ttryfrom`): an integer kind is
    range-checked, `char` too, your type's `TryFrom<X>` impl (`$tryfrom$X`) or else its
    `From<X>` as `Ok`. With several bounds (`TryFrom<u64> + TryFrom<i64>`) the argument picks
  - `B: AsMut<[u8]>` / `S: AsMut<Vec<i32>>` / `BorrowMut<X>`: the target, like `AsRef`;
    `b.as_mut()[0] = 7` and `v.as_mut_slice()[i] = x` assign into the same `Vec`
  - `Read` / `Write`: std's were already accepted (the io prelude's `IoRead` / `IoWrite`), also
    in `where` with `Send + 'static`. The hits left are `crate::io::{Read, Write}` re-exports
    (flate2, borsh), serde_json's own `Read<'de>` trait and `dyn Read + Send` fields (tiny_http):
    not resolvable from one file
- [x] **Format strings** (24 / 15 / 20; now 21 / 13 / 18) (`fixtures/hex_debug_ptr_format.rs`):
  - `{:x?}` / `{:X?}` (28 hits): `Debug` with each integer in hex, `#` / `0` / the width
    applying per integer (`{:02x?}` of bytes, `{:#x?}` pretty with `0x`), on numbers, strings
    and collections, options, results and tuples of them; an error on structs, enums and
    generic values, whose `Debug` is compiled without it (rustls's `{:x?}` is on integers)
  - `{:p}` (8): a made-up address (`κσ.$fmtPtr`), the same for one object
  - `info!(logger: l, ..)` now expands (to the global logger); log's own tests still need its `Log` /
    `Record` API, not bundled) and defmt's `{=str}` (defmt isn't bundled: its `write!` is its
    own macro)
- [x] **Renaming imported types or crates** (19 / 13 / 19; now 19 / 14 / 19)
  (`fixtures/crate_renames.rs`, `modules/renames`). Pre-passes in `alias.rs`:
  - `extern crate itertools as it;` (std or a bundled crate): the rename goes and `it::` /
    `crate::it::` paths (also in `use`) are written `itertools::`; `extern crate alloc as
    core_alloc;` is `std::`; between std's crates (`extern crate core as std;`, `std as alloc`)
    only the rename goes
  - `use geo::Point as P;` / `use serde_json::Value as Json;` (a capitalized type, trait or
    constant of a `mod` file or crate, at the top of the file): the rename goes and `P` is written
    `Point`, also in the `mod` file's exports (`pub fn f() -> M`). A renamed re-export (`pub use
    area::Error as AreaError;`) exports the type by its own name, and a file that imports
    `geo::AreaError` by name writes it `Error`
  - still errors (types share one namespace): two different `Error`s renamed apart in one file
    (rand_distr's `Error as BinomialError`, `Error as CauchyError`, .., borsh's), a renamed `Error`
    next to the file's own, a rename inside a block, one new name for `#[cfg]` alternatives
    (tiny_http's `SslContextImpl`). `use super::X as Y` at the top of a module file is the
    general `super::` gap.
  - re-measured on the 19 affected crates' files: files with the gap 19 → 4 (borsh, jiff,
    rand_distr, tiny_http; all for the reasons above), 3 more files compile (base64, bitflags,
    icu_collections' `codepointtrie`); the others now stop at later gaps (bumpalo at its raw
    `alloc` / `dealloc`)
- [x] **`iter_mut()` outside `for`, remaining adapters** (15 / 1 / 15; now 14 / 1 / 14)
  (`fixtures/iter_mut_more_adapters.rs`). Every hit was a type from another file of the crate
  (`ArgGroup`, `Header`, `Limb`), whose `Ty::Unknown` elements codegen reported again; it no
  longer does when the checker has already reported an error (0 hits; in those 14 crates, 9
  "cannot borrow `x` as mutable" errors went too). The shapes behind them, probed against rustc:
  - `if let Some(g) = v.iter_mut().find(..)` / `.rev().find(..)` of structs or tuples, and
    `let mut it = v.iter_mut(); while let Some(h) = it.next()` (bindings weren't `&mut`)
  - `try_for_each`, `try_fold` and `find_map` over numbers, strings and bools (slots)
  - `&mut` iterators returned (`-> impl Iterator<Item = &mut T>`, `Box<dyn ..>`,
    `slice::IterMut<'_, T>`), passed as parameters and kept in fields; over numbers they're slot
    iterators (`self.it.next()` writes through)
  - a map's `values_mut()` / `iter_mut()` outside a `for` (`κσ.$mapMutSlots`, or the values
    themselves when they're objects)
  - `let (src, dst) = (a, &mut *b);`: `dst` is a `&mut`
  - silent miscompile: `VecDeque::iter_mut().for_each(..)` wrote to the deque object's
    properties, not its elements
  - silent miscompile, now an error: indexing a `Vec<&mut i32>` collected from `iter_mut()`
    (`*rs[1] = 9`) wrote to a copy
  - left: items that are tuples holding a `&mut` in a returned iterator (`Item = (usize, &mut
    u8)`), `partition`, iterators over generic elements (smallvec's `A::Item`), rayon's
    `mem::replace(&mut self.iter, [].iter_mut())`, and `MaybeUninit` (anstyle-parse, http)
- [x] **Const generics, remaining** (15 / 3 / 11; unchanged) (`fixtures/const_arg_exprs.rs`,
  `modules/const_args`):
  - const arguments are constant expressions, worked out at compile time (`consteval.rs`, which
    enum discriminants use too): arithmetic, comparisons and `as` casts over `const` items
    (`{ 2 * D_CODES + 1 }`), associated consts of the file's impls, also on numbers
    (`{ CpuFeatures::NONE }`, `{ u128::MAX_STR_LEN - 16 }`), `u8::MAX` / `usize::BITS`,
    `size_of::<u32>()`, `crate::m::ID`, and calls of the file's `const fn`s (`let`, `if`,
    early `return`, turbofish types for `size_of::<T>()`), also inside macro arguments
  - another file's `pub const` / associated consts count (`Buf<{ sizes::WORDS * 3 }>`)
  - `u64` / `i64` / `u128` / `i128` const parameters: a BigInt in the body, whatever number
    the caller passes (`u128::MAX` too)
  - silent miscompile: a `mod` file's function or associated function with a const parameter
    (`sizes::Buf::<4>::new()`, `let b: Buf<4> = Buf::new()`) wasn't passed it from another
    file (`[1; N]` made one element). Modules now export which const parameters they take
  - silent miscompile: an associated const naming a `const` declared below its type
    (`const ALL: usize = Self::AVX | LATE;`) threw "Cannot access before initialization" when
    the module loaded. Such consts are computed on first read now
  - re-measured on the 12 affected crates (1,220 files): files with the gap 16 → 6 (deranged's
    `{ u128::MAX }` and itoa's `{ u128::MAX_STR_LEN - 16 }` went), one more compiles. Left:
    constants of other crates or other files the survey doesn't load (bitvec's `elts`, zlib-rs'
    `CpuFeatures`, ring, zerocopy's `crate::REPR_C_..`) and a `const` struct's method
    (`{ Config::DEFAULT.encode() }`); also an associated const reading its `impl`'s const
    parameter (`impl<const P: u128> U<P> { const PER: u128 = P * 1000; }`)
- [x] **Blanket `impl<T> Tr for T`** (12 / 3 / 9; now 12 / 6 / 9)
  (`fixtures/blanket_impls_more.rs`). Methods of a blanket impl on any type meeting its bounds
  already worked; probed against rustc, what failed:
  - `?Sized` (and `Send` / `Sync` / `Unpin` ..) among the bounds made every struct fail them
  - a method returning the impl's `T` (`fn into_source(self) -> T`) lost the receiver's type
  - extension traits on iterators: `impl<I: Iterator> Stats for I` with `I::Item` in its
    signatures (the receiver's item), and `trait Stats: Iterator` with `Self::Item` and default
    methods over `self` (`for x in self`, `self.count()`)
  - associated functions: `i32::zeroed()`, `Vec::<u8>::zeroed()`, `Cfg::zeroed()`,
    `<Cfg as Zeroed>::pair()`, `let v: Vec<f64> = Zeroed::zeroed_n(3)` and `T::zeroed()` in a
    generic function, with `T::default()` / `Self::zeroed()` in their bodies: they run with the
    type's class as `this` (`$ext$Zeroed$all.zeroed.call(κσ.$bdict(..))`, which adds the blanket
    functions a class lacks)
  - silent miscompiles, also without blanket impls: `let x: i32 = Make::make();` (your impl for a
    built-in type, by the expected type) emitted `undefined.make()`; `Inc::inc(&mut n)` /
    `<f64 as Money>::money(&x)` on a built-in type called a method the number doesn't have; a
    trait's default associated function calling `Self::make()` failed for built-in types
  - a blanket impl of another file's trait (gimli's `impl<T: RelocateWriter> Writer for T`) now
    says what any impl of such a trait says, so the survey counts it as another crate
  - an error now: `*self = ..` / `*self += ..` in a blanket impl's `&mut self` method (it used to
    be rejected by accident; with the rest supported it compiled to invalid JS)
  - re-measured on the affected crates (638 files): files with the gap 12 → 0, one more compiles
    (num-traits' `tests/cast.rs`). The registry's other hits are the classifier's: parser test
    data with an undeclared `S`, and windows-sys' `CY`, which aren't type parameters
  - found on the way, not fixed: a derived `Default` of a generic struct with a bare `T` field
    (`struct W<T> { n: T }`) leaves it `undefined` (`W::<u8>::default()`, also through
    `T::default()`), in the baseline too
- [x] **Macro recursion limit** (9 / 9 / 6; unchanged; 8 / 8 / 5 after the macro work above)
  (`fixtures/macro_recursion_limit.rs`, `modules/recursion_limit`). Only syn's were really deep;
  the rest recursed by mistake:
  - writeable's `assert_writeable_eq!` hands on to `$crate::assert_writeable_eq!(@internal, ..)`,
    and its second rule's `$actual:expr` took `@internal`: where the fragment parser rejects the
    tokens, the fallback scan took anything. A fragment no longer starts with a token rustc's
    can't (`@`, `,`, `=>`, `+`, ..; `::`, `..`, `&&` can, and `?` starts a `ty`)
  - aho-corasick's `debug!` is `log!(log::debug!(..))`: `log::debug!` was the crate's own `debug!`
    again. A `log::` / `anyhow::` / `tokio::` / `futures::` path is that crate's macro (the
    bundled one, left out when the crate defines its own of that name)
  - `#![recursion_limit = "N"]` is honored (up to 1024), in the file or its crate root, found
    through the `mod` items that declare it (`imported_macros` passes it on)
  - the wasm stack is 8 MB (`compiler/.cargo/config.toml`): an expression ~125 levels deep (a
    macro accumulator's `(($acc) * 2 % 1000 + 1)`, or written out) overflowed 1 MB and trapped
    the module, also before this change; now ~600 levels, then the JS engine's `RangeError`
  - now 3 / 3 / 3, all syn's `tests/common/eq.rs` (`spanless_eq_enum!`, ~170 levels): its
    `#![recursion_limit = "1024"]` is in each `tests/test_*.rs` root, which no `mod` path from
    `common/` reaches. writeable's `impls.rs` now stops at `impl Writeable for u8` (a trait on a
    std type, above); aho-corasick's files at external names. No other file's errors changed
- [x] Smaller: iterator chain shapes over numbers, strings or bools (6 / 1; now 5 / 1),
  `macro_rules!` in an unsupported position (5 / 5), nested `use` paths (4 / 2), byte / C string
  literals in one position (1 / 0). (`fixtures/c_strings_set_from.rs`, `modules/nested_use`,
  `modules/include_nested`.) Re-measured on the hit files: all four gaps 0 but jiff's one (below);
  `unicode-properties`' `lib.rs` and the two icu data crates' files compile.
  - iterator chains: the hits were items of a type from another file (clap's `ArgGroup`,
    tiny_http's `Header`, ring's `Limb`, zerovec's `T::ULE`); the checker no longer reports the
    chain again after an error, as codegen already didn't. The shapes behind them (`zip` +
    `for_each`, `find`, `try_for_each` over numbers) already matched rustc. New: itertools'
    `set_from` over `iter_mut()` (slots over numbers, strings and bools, else replaced in place)
  - `macro_rules!` positions: `include!` of files that define and call macros (icu's generated
    data) is pasted in before macro expansion, its own `include!`s read next to it; `macro_rules
    ! name` with a space (ra_ap_parser's generated code); `pub use __make_provider as
    make_provider;` renaming a macro of the file
  - nested `use` paths into a `mod` file's inline modules (`use geo::shapes::Circle;`, `use
    stamp::{context::NoContext, ..}`, `geo::shapes::area(..)`, also in macro arguments, `super::`
    / `crate::` from this file's inline modules, a `use` in a block): `alias.rs` writes them as
    `modflat`'s flattened names (`shapes__Circle`), and a module's exports include its inline
    modules' items. Left: re-exporting a `mod` file's item through an inline module (`mod
    prelude { pub use crate::geo::Point; }`, also without nesting); jiff's hit is the survey
    compiling `error/mod.rs` alone, where `crate::util` finds the sibling `util.rs`
  - C strings: `c"..."` / `cr"..."` are the cstr prelude's `CStr` (`to_bytes`,
    `to_bytes_with_nul`, `to_str`, `to_string_lossy`, `count_bytes`, `from_bytes_with_nul` /
    `until_nul` with std's errors, comparisons, std's `{:?}`). No `CString`. On the way:
    `char::escape_debug` escapes combining marks and non-printable separators / format
    characters as Rust does (`\u{301}`, `\u{a0}`, `\u{200d}`)

#### Open from the syntax re-run (2026-11-17)

Features whose probe doesn't compile (`cargo run --release --example corpus_ast -- --probes`),
by files using them / crates / files where an error lands on them. "Using" counts every use of
the syntax, many of which work (the moved-value one needs the value used again), so the last
number is the measured harm; the first is what a fix could touch.

- [x] **`let S { x, .. } = p` then using `p`** (215 / 65 / 0): binding a `Copy` field by value
  moves `p` here ("use of moved value: `p`"); rustc only moves the bound fields. The error lands
  on the later use of `p`, so the survey can't count the files it breaks.
  - a `let` pattern now moves only what it binds by value and whose type moves (`ref` bindings
    and bindings under a `&` pattern move nothing); a partial move records the moved top-level
    fields, so `p.x` stays readable and using `p` whole is "use of partially moved value", as
    in rustc (`moves.rs`, `fixtures/let_partial_moves.rs`). A `String` field bound this way
    isn't counted as a move (strings aren't tracked unless certainly owned)
- [x] **Inherent impls per instantiation with the same method name** (151 / 55; errors are
  "duplicate definitions with name `find`"): `impl W<i32> { fn get(..) }` next to `impl
  W<String> { fn get(..) }`, aho-corasick's `impl<V: Vector> Slim<V, 1>` / `Slim<V, 2>`. Probe:
  "duplicate definitions with name `get`".
  - a later impl whose type arguments are disjoint from every earlier one's
    (`adt::disjoint_args`: a type parameter overlaps anything) keeps its methods under keys of
    their own (`get$1`, the `method_alts` of generic traits; `Adts::inherent_alts` marks them).
    A method call picks by the receiver's type arguments, a path call by the path's
    (`W::<i32>::kind()`) or the impl's (`Self::kind()`); `W::kind()`, and a receiver whose
    integer literal isn't settled (`W(2).get()` with `W<i32>` and `W<u8>` impls), are rustc's
    E0034 (`fixtures/inherent_per_instantiation.rs`)
- [x] **Associated const as a pattern** (99 / 38 / 39; 27 files where it's the only gap):
  `match x { Self::MAX => .., S::Z => .. }`. Probe: "cannot find tuple struct, struct or
  variant `S::Z`".
  - path patterns naming an associated const (`Self::MAX`, `S::Z`, `<S as Tr>::TOP`) or a std
    one (`u8::MAX`) compare with the value (`===`, or `κσ.$eq` for structs and enums); they
    and plain `const`s are range bounds too (`0..=S::LIMIT`, `B::LO..=B::MID`). Integer values
    (`consteval::eval_segs`) count for exhaustiveness (`fixtures/assoc_const_patterns.rs`)
  - on the way, a silent miscompile: a range bound that wasn't a literal (`0..=LIMIT` with a
    plain `const`) was dropped from the test, so `0..=LIMIT` matched everything above 0. A bound
    codegen can't read is now an error
- [x] **Slice patterns on arrays** (89 / 39 / 17; 12 only gap): `match [1, 2, 3] { [first, ..,
  last] => .. }` is "non-exhaustive patterns: `[]` not covered"; an array's length is known, so
  rustc accepts it.
  - the scrutinee's dimensions come from the source (`typeck_arrays::array_dims_in_file`: an
    array literal, a `let`'s type or initializer, a parameter's, field's or function's return
    type, `[T; N]` with a `const N`), nested ones too (`[[u8; 2]; 3]`), and the exhaustiveness
    check sees that column as `Coll::FixedArray(n)`: only length `n` is a value to cover. Also
    for `let` (`let [[p, _], [_, q]] = grid;`) and `for` items (`for [k, v] in pairs`,
    `pairs.iter()`) (`fixtures/array_slice_patterns.rs`). A pattern of the wrong length is
    "non-exhaustive", not rustc's E0527 "pattern requires 2 elements but array has 3"
- [x] **`extern crate x as y` of `core`** (30 / 28 / 0): `extern crate core as c;
  c::cmp::max(1, 2)` is "cannot find function `c::cmp::max`" (the rename pass handles
  `itertools as it` and `alloc as ..`).
  - the rename pass already handled `core as c`, but not inside a macro call's arguments
    (`println!("{}", c::cmp::max(1, 2))`), where `::` is two `:` tokens; the same fix reaches
    `it::join(..)` in `format!` for every renamed crate (`fixtures/extern_crate_core_alias.rs`)
  - found on the way, not fixed: a full std path to an enum variant or a generic std function
    (`std::option::Option::Some(3)`, `core::result::Result::Ok(1)`, `std::mem::size_of::<u32>()`)
    is "cannot find function"
- [x] **Associated type bounds** (17 / 3 / 0): `I: IntoIterator<Item: Display>` loses the bound
  ("`I::Item` doesn't implement `std::fmt::Display`").
  - a pre-pass (`assocbounds.rs`) writes them as `where I::Item: Display` (on a type parameter
    or a `where` predicate of a plain name, in a `fn`, `impl` or `trait`), and in argument
    position (`it: impl Iterator<Item: Into<i64>>`) as a type parameter of the function
    (`Item = __Item0`, `__Item0: Into<i64>`) (`fixtures/assoc_type_bounds.rs`). Left alone:
    return position, `dyn`, nested bounds, and a subject that isn't a plain name
- [ ] **Inline `const { .. }` blocks** (13 / 8 / 9): "this block kind is not supported yet".
- [ ] **The same item under alternative `#[cfg]`s** (20 / 11 / 16, from "defined multiple
  times" / "duplicate definitions" on an item with a `#[cfg]`): combinations beyond `x` /
  `not(x)`, such as brotli's `#[cfg(any(not(feature = "std"), feature = "pass-through-ffi-panics"))]`
  next to its opposite, or anyhow's `#[cfg(any(feature = "std", not(anyhow_no_core_error)))]`.
- [ ] Smaller: your trait on a function pointer type (`impl Tr for fn() -> i32`, 1 file).

Still uncategorized (1,814 file hits, none involving another file's names that the syntax step
can see), by message and construct: "no method named" on a method call (166 files), mismatched
types (85 on a name, 37 on a method call, 34 on a `&`), a `#[derive]` whose field type lacks the
trait (81, e.g. `Clone` for `Arc<dyn PrefilterI>`), a tuple field of a method's result
(`self.0.get_ref().0`, 81), `.into()` needing a target (67), "needs a bound" comparisons and
operators on type parameters (41 + 28), a tuple-struct pattern of another crate's enum
(`Bound::Included(&i)`, 39). Worth sorting next: several are probably std types the checker
doesn't know (`Bound`, `Arc<dyn Tr>: Clone`).

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
  `String`; `Cursor` is in the io prelude. Since then: `fs::File` / `Metadata` (41 / 16) and
  `process::Command` (25) compile and fail with `NotFound` / `Unsupported` (`os_prelude.rs`,
  `fixtures/no_os_std.rs`), hand-written futures (`Pin` 39, `task::Context` 37, `Poll` 20,
  `Waker` 14; `task_prelude.rs`), `TypeId` / `type_name` / `dyn Any` (16; `typeck_any.rs`,
  `fixtures/any_type_ids.rs`), `IoSlice` (12) and `SocketAddr` / `IpAddr` (16;
  `net_prelude.rs`, `fixtures/net_addrs.rs`).
- [x] **Implementing std traits:** `ExactSizeIterator` (87), `FusedIterator` (84),
  `DoubleEndedIterator` (65), `io::Read` (72), `io::Write` (39), `Hasher` (17), `BuildHasher` /
  `BufRead` (11 each), and the markers `UnwindSafe` / `RefUnwindSafe` (20 / 17), accepted and
  ignored. `len()` comes from `size_hint`, `.rev()` runs `next_back`; `Read` / `BufRead` are
  traits of the io prelude with std's provided methods (`read_to_end`, `read_to_string`,
  `read_exact`, `bytes`, `read_line`, `read_until`, `lines`, `split`), which `BufReader` and
  `Cursor` implement; a `Hasher` gets `write_u8` .. `write_i128`, a `BuildHasher` `hash_one`, and
  `HashMap::with_hasher` / `default` take one (keys still compare by value). `Future` (51) too:
  `impl Future for X` (see above).
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
  Indexing by position (`map[0]`, `set[1]`) and by key (`map[&k]`), both as places (`map[0] += 1`),
  and `get_index_mut` (a slot for number, string and bool values where it's matched).

Not in the corpus (it's library internals), but likely to matter more to players:

- [x] **`ordered-float`**: `OrderedFloat<f64>` / `NotNan` make `f64` scores usable as `BinaryHeap` and
  `BTreeMap` keys, for ranking targets by money/sec without a hand-written `Ord`. A Rust-source
  prelude (`ordered_float_prelude.rs`); `js-node/crate_ordered_float.rs` (the crate isn't in the
  cargo cache, so the expected output is rustc's on the program with the prelude).
- [x] **`petgraph`**: `Graph` / `DiGraph` / `UnGraph`, `GraphMap` / `DiGraphMap` / `UnGraphMap`,
  `Bfs` / `Dfs` / `DfsPostOrder` and `algo`'s `dijkstra`, `astar`, `bellman_ford`, `toposort`,
  `is_cyclic_*`, `connected_components`, `kosaraju_scc`, `tarjan_scc`, `has_path_connecting`,
  `min_spanning_tree`: a Rust-source prelude (`petgraph_prelude.rs`) that follows petgraph's
  code, plus a rewrite in `bundled.rs` (algorithms become methods of the graph, so one name
  serves both graph kinds; a walker's `next(&g)` takes the graph's neighbors as a closure).
  `js-node/crate_petgraph.rs` and `crate_petgraph_more.rs`, checked against rustc on the
  rewritten program with the prelude (not in the cargo cache either). Left: `StableGraph`,
  `Topo` / `Reversed`, more `algo` functions, `Dot`, a `Graph` index type other than `u32`.

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
- [x] Bindings: `dts2rs` maps `ReactNode` / `ReactElement` to `ReactNode` (`react::Node`), so
  `print_raw` / `tprint_raw` / `ui.set_tail_title` / `ui.render_page` take a node, and
  `ui.create_connect_link` returns one; such a parameter takes whatever a `jsx!` child may be
  (`ns.print_raw("text")`, a number, a `JsValue`, `Option` / `Vec`), converted for React
  (`κσ.$react.node`: an `i64` as text). A field that may hold one (a script's `title`) stays a
  `JsValue`.
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
- [x] Typed children: `{expr}` is `__rx_child<T: ReactIntoNode>` (`react::IntoNode`). Which types
  meet it is decided by the checker by shape (`typeck_react.rs`: `Node`, text, numbers,
  `JsValue`, and `Option` / `Vec` of those, nested), not by trait impls: the impls for
  `Option<T: IntoNode>` would need the inner bound checked, which extension-trait impls on std
  types don't do. The same check serves the NS bindings' node parameters. Errors point at the
  expression ("React can't show a `(i32, i32)`").
- [x] Hover and completions inside `jsx!` (`jsx_service.rs`, on the source): a component's
  signature and docs on its tag, a prop's type or an HTML attribute's kind on its name;
  completions of tag names (components in scope, HTML tags), a component's props not given yet,
  an element's attributes (`on_click`, or `onClick` once a capital is typed). Embedded
  expressions use the ordinary service (each is a kept span; HTML children and props are slices,
  `&[..]`, not `vec![..]`, whose tokens the service can't see into). A `jsx!` call with a
  mistake (being typed) no longer stops the rest of the file being analyzed: the service
  rewrites it into its expressions.
- [x] Typed intrinsic attributes and DOM events (`jsx_attrs.rs`, React 17's `possibleStandardNames`
  and event names): handlers take a closure of `js::Event`, of nothing, or of a `JsValue` (by
  the closure's parameters); text, `bool`, number-or-text, `style` (string, pairs or map), `key`,
  `ref` (a `use_ref`'s box, set to the element); unknown attributes are errors with a suggestion.
  Literal values are kept spans too, so type errors point at them.
- [x] `use_memo`, `use_callback`, `use_reducer` (`Dispatch<A>`), `create_context` /
  `use_context` with `<ctx.Provider value={..}>` (`Context<T>`, also `ctx.provide(..)`);
  `use_state`'s `update` and the reducer get a copy of the state (a Rust closure may change it in
  place and return it); `set` of a closure value isn't taken for an updater.
- [x] `{..spread}` props: on an HTML element, `(name, value)` pairs, a map or a `JsValue` object;
  on a component, the parameters not given come from the fields of a struct.
- Class components: not planned.

## Performance scan (2026-11-16)

Measured on the build of `db63e10` plus the working tree, with node 24: compile times from
`compile()` calls (warm, averaged) and `--cpu-prof` (the wasm keeps function names), run times of
compiled probe programs against `runtime/rt.js`. Ranked by impact within each part.

### Compile time

Today: 3 to 30 ms for most corpus scripts, but `darknet.rs` (784 lines) takes 146 ms,
`batcher.rs` / `contracts.rs` about 95 ms, and `use chrono` alone costs 186 ms (`clap` 99 ms;
`std::time`, `regex`, `io`, `Wrapping`, `serde` 14 to 20 ms each). N small functions: 100 → 28 ms,
400 → 160 ms, 800 → 460 ms, so it's superlinear. Hover, completions and signature help each
re-run all of it (`service.rs` `analyze`).

- [x] **`Checker::uses_crate` scans the whole source per call** (`typeck_crates.rs:84`: two
  `format!`s and two `src.contains` per `Vec` / iterator method call, unresolved path call and
  some `lower_type`s). 28-48% of all compile time in the profiles (865 of 2,043 ms for 800
  functions; 441 of 920 ms for the chrono script, whose prelude text is scanned too). Done: the
  answers are kept per crate name in the checker (`crate_uses`), and codegen reads
  `crate_imports` once (`Codegen.crate_imports`). 800 functions 243 → 107 ms, `use chrono` 184
  → 67 ms (this machine; see the table below).
- [x] **`{CONST}` captured in a format string re-parses the file** (`codegen_consts.rs:18`, from
  `codegen.rs` `captured_const`): a full `SourceFile::parse` with the preludes plus `all_items`
  per capture, which also evicts `all_items`' one-tree cache. 400 `println!("{LIMIT} ..")`: 615
  ms, against 27 ms with `println!("{} ..", LIMIT)`. Done: `Codegen.const_bodies` (name → the
  value's range, from `emit_file`'s const pass) and the checker's type at that range: 317 →
  6.5 ms.
- [x] **Preludes are parsed, checked and emitted in full on every compile** (`stdtime.rs`,
  `stdgaps.rs`: chrono 2.7k lines, num 1.9k, clap 1.2k, wrapping 1.2k, rand_distr 1.1k, io
  0.6k), behind loose substring gates (`"Cow"`, `"Lazy"`, `"time::"`). Done in part: the gates
  match whole words (`stdtime::has_word`: `Cow`, `Either`, the lazy / once cells, `time::`; and
  `stdgaps::has_rewrites` names exactly what the pass rewrites, so `RefCell`, `Ready` or
  `Pathfinder` no longer run it), and the prelude's parse is shared (next item). The cache of a
  prelude's declarations and JS is skipped: a prelude's output depends on the program it's
  compiled with (its locals are renamed around the program's globals by `fresh`, its impls for
  built-in types join the program's `$ext$i32` class, its generic bodies are checked against
  the program's impls), so a cache keyed by the prelude text alone would change output, and
  emitting per reached item is a larger refactor than its gain now (`use chrono` 186 → 68 ms
  after `uses_crate`).
- [x] **The source is re-parsed 6 to 10 times per compile**: nearly every program goes through
  `preprocess_with` (the `codegen.rs:555` gate includes `"::"` and `"use "`), and each pass
  parses the whole text, several in their gate and again in `expand` (`variants`, `alias`,
  `modflat`), `procmacro::expand` always, `expand_with` once per macro round, plus the final
  parse. In `darknet.rs`, `preprocess_with` is 27% and parsing 21%. Done: every whole-file
  parse goes through `macros::parse_file` / `parse_tree`, which keep the last 8 parses by text
  and hand out the same tree (so `adt::all_items`' root-keyed cache hits too): a pass re-parses
  only after it edited the text, and a `mod` file read for macros, exports and `jsx` components
  is parsed once. `bundled::renamed_crates` gates on the root's last segment (`io::`), not its
  first (`std::`, which made every program walk the tree twice per std module); `rules_of`
  runs once per definition per macro round. darknet.rs 106 → 78 ms (`preprocess_with` was 42%
  of it). Merging the read-only rewrites into one walk is left: each pass's edits feed the next
  one's parse, and the shared parse took most of the cost.
- [x] **No caching across calls**: the IDE requests and a compile of the same text each redo
  preprocessing, preludes, parsing and type checking. Done: `memo::cached` keeps the last 6
  results per cache (`compile_with`'s `CompileOutput`, `service::analyze`'s `Analysis`) keyed
  by text, file name, the bindings generation (`bindings::set` bumps it), the overflow flag and
  strict borrows, plus every file the resolver returned, which is read again and compared on a
  hit (a changed, added or removed `mod` file misses). Within a compile the recording resolver
  reads each file once. The same text again: 0.7 ms; hover, completions and signature help on
  an unchanged `darknet.rs` 150 → 0.3 ms, and after an edit one analysis serves all three.
  `test/run.mjs` "kept results" compiles, changes a module file / the bindings / a setting, and
  compiles again.
- [x] Smaller ones: `OffsetMap::orig` / `replace` are linear in segments (`mbe.rs:51, 94`; called
  per source-map mapping): binary search, one rebuild per pass. `used_names` is cloned per
  top-level item and per method (`codegen.rs:1194`, `codegen_adt.rs:338`): an undo log.
  `borrows::check` is uses × changes per borrow (`borrows.rs:136-185`): index by declaration.
  Macro arguments are parsed separately by typeck, moves and codegen, and `rules_of` re-tokenizes
  a definition per invocation. Done: `orig` is a binary search (`replace` stays: its text
  splice is the linear part anyway); `used_names` has an undo log (`name_log`, `undo_names`;
  the clone was 8% of the 800-function compile); `rules_of` once per round. Skipped:
  `borrows::check` (under 1% of any profile) and the macro arguments (small per-pass snippet
  parses; a text-keyed cache of them would thrash on the whole-file parses).

Results (this machine, node 24, `compile()` warm averages; "before" is the staged tree):

| case | before | after |
| --- | --- | --- |
| `corpus/bitburner/darknet.rs` | 195 ms | 78 ms |
| `batcher.rs` / `contracts.rs` | 202 / 119 ms | 64 / 59 ms |
| `use chrono` script | 184 ms | 68 ms |
| 100 / 400 / 800 small functions | 15 / 93 / 243 ms | 11 / 43 / 85 ms |
| 400 `println!("{LIMIT} ..")` | 317 ms | 6.5 ms |
| the same text compiled again | 161 ms | 0.7 ms |
| hover / completions / signature help on darknet.rs, unchanged | 150 / 145 / 144 ms | 0.3 / 0.5 / 0.3 ms |
| the same, right after an edit (one analysis serves all three) | 150 ms each | 55 to 59 ms the first, 0.3 ms the rest |
| crash sweep, cargo registry (5,922 files) / std (3,004) | 561 s / 88 s | 218 s / 63 s |

The sweeps' outcomes (876 / 474 compiled, 0 panics) and every file's first diagnostic are the
same before and after; `test/run.mjs` and the React tests pass; no fixture's output changed.

### Run time

Verified on compiled programs (Rust's time for the same work is milliseconds or less):

- [x] **`VecDeque` is an array using `shift` / `unshift`** (`codegen_coll.rs:92-153`,
  `rt.js` `$dqShift`): O(n) per `pop_front` once V8 stops shifting in place (measured: O(1) up
  to about 16k elements, then 30 µs per shift). A BFS-shaped loop over 50k items: **12 s**. BFS
  over the server graph is the most common Bitburner pattern. Done: `κσ.Deque`, a ring buffer
  (backing array, head, length) that also answers the Array methods the compiler emits for a
  `Vec` (on its contents made contiguous, which is the backing array itself, so in-place changes
  stick); `q[i]` is emitted as `q.b[q.$i(i, loc)]` (a place, so writes, `+=` and `&mut q[i]`
  work), index loops (`iter_mut`, `enumerate`) use `q.b[q.$p(i)]`, and the runtime helpers that
  index arrays (`$first`, `$vecSwap`, `$retain`, `$placeSlot`, ..) take a deque. `collect`,
  `from`, `into`, `Default`, `mem::take`, serde and `$clone` all build a `Deque`. BFS over 50k
  nodes plus a 20k-long push/pop queue: **4,397 → 32 ms** (`fixtures/vecdeque_ring.rs`).
- [x] **`BTreeMap` / `BTreeSet` re-sort the whole map on the first ordered read after an insert**
  (`rt.js` `$sort`: `new Map([...m].sort(cmp))`; `$lastSlot` walks all entries, `$range`
  copies). Insert + `pop_first` (Dijkstra, schedulers) is O(n² log n): 3k rounds, 394 ms, growing
  quadratically. Done: the hash classes keep a sorted array `s` of their stored items from a head
  index `h`; a new key goes in by binary search (`splice`), `pop_first` advances `h`, `range` is
  two binary searches and a slice, `first` / `last` are O(1); after 512 binary inserts between
  ordered reads (a bulk load) the rest are appended and sorted at the next read. The entry API
  (`$slot`) tells the map about new slots. Dijkstra on a 120×120 grid with a `BTreeSet` queue
  plus the PLAN's insert + `pop_first` rounds: **556 → 19 ms** (`fixtures/btree_ordered.rs`).
- [x] **String byte offsets re-scan the string per call**: `s.len()` is `$utf8len` (a full
  walk; only `for` ranges hoist it), `s.as_bytes()[i]` runs a `TextEncoder` and copies to an
  Array per access, `&s[a..b]` builds three per-code-point arrays, `is_char_boundary` scans from
  the start. A `while i < s.len() { s.as_bytes()[i] .. }` loop over 10k chars: **2.1 s**. Fix: an
  ASCII fast path (`s.length`, `charCodeAt`, `slice`) checked once per string, and hoisting
  `as_bytes()` out of loops.
  Done (2026-11-16): `rt.js` `strInfo` remembers the last two long strings' UTF-8 length,
  whether they're ASCII and (once asked) their bytes, so `$utf8len`, `$strSlice`, `$find`,
  `$isCharBoundary` and `$unitOf` take an ASCII path (`s.length`, `slice`, `indexOf`) and
  otherwise reuse the bytes; `s.as_bytes()[i]` is `κσ.$byteAt(s, i, loc)` (`charCodeAt` on
  ASCII, the cached bytes otherwise; `$oob` with the byte count). The scanning loop over 10k
  chars (`count_digits` + `tokens`, `p3-bench/b1_str.rs`): **12.7 s → 3 ms**. Unicode results,
  the "is not a char boundary" and "index out of bounds" panics are unchanged
  (`fixtures/str_byte_offsets.rs`, `panic_byte_index.rs`). `as_bytes()` isn't hoisted: the
  cache makes each access O(1) already.
- [x] **`&mut place` cells** (`rt.js` `$cell`: two closures and a get/set object per call, for
  `&mut self.n`, `&mut v[i]`): about 35x a write-back temp (`const c = [o.n]; f(c); o.n = c[0]`),
  which works whenever the place has no side effects.
  Done: a call of a local function or method whose signature can't keep the borrow
  (`writeback_safe`: not `async`, no lifetime parameters on it or its parameters, a return
  type without `&`, lifetimes, `impl`, `dyn`, type parameters or types with lifetime
  parameters) passes a pure `&mut place` (`&mut self.n`, `&mut v[i]`, `&mut n`) as
  `(($wb0, x) => { try { return f($wb0, x); } finally { place = $wb0[0]; } })([place], x)`:
  the place is read once, the callee reads and writes `$wb0[0]`, and the write-back runs on
  return or unwind (a `Drop` running during a panic sees the written value, as in Rust). Other
  arguments are evaluated outside the arrow, in order, so `await` and `?` stay in the caller.
  Everything else (lifetimes, generic returns, trait-object and generic calls, std functions)
  keeps the cell. A struct-field counter through `&mut` (`b2_cell.rs`, 2M calls): **2.0 s →
  12 ms** (`fixtures/mut_place_writeback.rs`).
- [x] **Slices**: `&v[a..b]` copies (`v.slice()`), so `&v[1..]` recursion is quadratic; `&mut
  v[a..b]` is a Proxy that runs a regex per property access (130 ns per read). Done: the view
  Proxy checks digits, flattens a view of a view to the base, and answers the Array methods on
  the base (`slice` and iteration directly, read-only ones on a copy of the range, `sort` /
  `reverse` / `fill` on a copy written back; `$vecSwap` and `$rotate` work on the base); views
  share one handler with the state on the target. `f(&v[a..b])` passes a view when `f` is one of
  the file's functions whose `&[T]` parameter only reads a little of it (`codegen_views.rs`:
  length, elements outside loops and closures, sub-slices for such calls, iteration, slice
  patterns; aliasing, comparing, returning, storing, or passing it to a function that isn't
  view-safe keep the copy). `v[a..b].sort()` writes back with a loop, not a spread. Recursive
  `sum(&v[1..])` over 4k `i64`s ×20, 100k recursive binary searches over 100k elements and 200
  rounds of `bump(&mut a[lo..lo + 1000])` + range sorts: **19.8 s → 0.61 s**
  (`fixtures/slice_views_ops.rs`, `slice_param_views.rs`).
- [x] **Composite `HashMap` keys go through `JSON.stringify` per lookup** (`rt.js` `key()`): tuple
  and struct keys are about 10x numeric ones. Done in the runtime: `key()` writes arrays of
  primitives (tuple keys, nested too) by hand, giving exactly `JSON.stringify(x, bigKey)`'s text
  (`-0` / NaN / strings / BigInts / `null` / `undefined` as JSON writes them), and falls back
  to it for structs and other objects; 580 → 10 ns per pair key, the same as a compile-time key
  function measured (20 ns), which therefore wasn't added. Grid BFS with `(i32, i32)` keys in a
  `HashSet` + `HashMap`, 200k struct-key inserts and 50k `(String, u8)` entries: **365 → 160
  ms** (`fixtures/hashmap_keys.rs`).
- [x] **64-bit integers are BigInt** with a helper call per op (`$w128`, `$to128` rebuilding its
  bounds per call; `for i in 0..n` over `u64` loops over BigInts): about 18x `u32`. Inlining
  `BigInt.asUintN(64, ..)` alone is 4.5x faster. `usize` bit operations call `$bit64` with an
  operator string (31 ns); an inline fast path for operands below 2^31 avoids it.
  Done: `+ - * << ~ -` and `sum` / `product` on `i64` / `u64` / `i128` / `u128` wrap inline
  (`BigInt.asUintN(64, a + b)`, 9x a `$w128` call), a literal shift amount is masked at compile
  time (`h << 5n`), `wrapping_add` / `wrapping_sub` / `wrapping_mul` with a BigInt argument are
  inline too (`$big` ran a regex on the method name per call), a number kind's value cast to a
  BigInt kind is `BigInt(x)` (wrapped only for a negative into an unsigned kind), and `$to128`
  takes safe integers first (also fixing `'A' as u64`, which threw). `usize` / `isize`: `x & lit`
  with a literal below 2^31 is JS's `&`; `$shl64` / `$shr64` stay in numbers while the result is
  a safe integer (`$bit64`'s call is 1.3 ns when V8 inlines it, so its operator string stays).
  The u64 hashing loop (`b3_u64.rs`): **145 → 28 ms**. Debug builds still panic where rustc's
  do (`$ckArith` runs before the inline wrap; `test/debug/overflow_u64_inline.rs`). `for i in
  0..n` over `u64` still iterates BigInts: the representation is the cost
  (`fixtures/u64_inline_ops.rs`).
- [x] **Shapes**: `Result` and tuple fields are indexed properties (`this[0]`), about 30x a
  named field (`this._0`) in a construct + tag check loop; unit enum variants allocate on each
  `Dir::N` (`static get`), and `d == Dir::S` is a generic `κσ.$eq` (47x an int compare);
  variants add different fields, so `$tag` reads on enums with more than 4 variants go
  megamorphic. Fix: named fields, `d.$tag === k` for unit variants, singletons where possible,
  one field layout per enum.
  Done in part: `d == Dir::S` / `Dir::S != d` on an enum with a derived `PartialEq` is
  `d.$tag === 2`, and `p == q` on a struct or enum with a derived `PartialEq` calls its own
  `p.$eq(q)` (no `κσ.$eq` dispatch). Skipped: unit-variant singletons (`*r = Dir::E` through a
  `&mut` is `κσ.$replace`, which rewrites the object in place, so a shared instance would change
  every holder; an allocation is cheap next to that risk); one field layout per enum (a
  6-shape vs 1-shape `$tag` loop measured the same, 38 vs 42 ms, so V8 handles the maps);
  named `Result` / tuple fields (tuples are arrays everywhere, `Res` is read as `[0]` across
  the runtime, no measured win to justify it). The interpreter `match` + `Dir` loop
  (`b4_interp.rs`): **940 → 210 ms** with the `match` temp below.
- [ ] Moderate:
  - `match &prog[pc]` re-evaluates the scrutinee with its bounds check in every `else if`, and
    `grid[i][j] += x` evaluates `grid[i]` four times: use temps, and `switch` on `$tag`.
    Done: a `match` whose subject is an indexed struct or enum goes through a `const` (the
    object is the same, so field bindings and `&mut` patterns still write it); a read of
    `grid[i][j]` is `κσ.$get(grid[i'], j, loc)` (the row once instead of three times) when the
    expression isn't a place (assignment target, `&mut`, method receiver, prefix operand), and
    `grid[i][j] = ..` / `+= ..` hoist the row into a `const` (`recv_override`). `switch` on
    `$tag` not done: the `else if` chain on `$t1.$tag` is cheap once the subject is a name. The
    grid update + Copy struct + generic score loop (`b5_grid.rs`): **860 → 95 ms**
    (`fixtures/nested_index_places.rs`, `panic_nested_index_assign.rs`,
    `enum_tag_compare.rs`).
  - Generic traits on numbers go through `$dcall`: spread arguments plus a registry scan.
    Measured: a per-site cache was slower than the scan (23 vs 19 ms per 1M calls; the scan
    is one entry), and a direct `.call` is 11 ms: the rest/spread and `.call` are the cost,
    not the registry. Left as is.
  - Copy structs are cloned and compared through the generic `κσ.$clone` / `κσ.$eq` instead of
    `p.$clone()` and inline compares, and the clone isn't skipped at a value's last use.
    Done: a `Copy` struct with a derived `Clone` read from a place is `(p).$clone()`, and `==`
    on a derived `PartialEq` is `p.$eq(q)` (above). Last-use skipping not done.
  - [x] `Vec::contains` is `v.some(eq)`, 4.5x `includes` for numbers and strings. Done:
    `includes` for integers, strings, chars and bools (not floats: `includes(NaN)` is true,
    Rust's `contains(&NAN)` false).
  - [x] Integer `sort()` uses `κσ.$cmp` rather than `(a, b) => a - b`. Done: `κσ.$cmpNum` for
    integers that are JS numbers and `κσ.$cmpStr` for strings and chars; BigInts and the
    rest keep `$cmp`.
  - [x] `.iter().rev()`, `step_by` and `windows` / `chunks` copy or allocate per item. Done:
    `v.iter()` is an `ArrayIt` (index, end), and `rev` (an `ArrayRevIt` sharing it, so
    `by_ref().rev()` still consumes the same iterator), `step_by`, `skip`, `nth`, `last`,
    `count` and `collect` work on the indices; `windows` / `chunks` / `chunks_exact` are one
    class iterator (`SlicesIt`) instead of generators. `contains` over 2k ints and 500
    strings, 60 sorts of 20k ints, `rev` / `step_by` / `windows` / `chunks` over 200k: **319 →
    197 ms** (`fixtures/vec_contains_sort.rs`, `iter_array_paths.rs`).
- [x] **Correctness bug found on the way**: `for x in a.iter_mut() { bump(x); }` with `fn bump(v:
  &mut i32)` emitted `bump(a[$t1])` and failed with "TypeError: Cannot create property '0' on
  number '1'". The loop's `x` (the place `a[$t1]`) is now passed as a cell over that place
  (`fixtures/iter_mut_loop_args.rs`).

Already fine: `for i in 0..n` and reversed ranges are plain `for` loops, `?` and panics are
returns and throws (try/finally only for `Drop` types), `push_str` is `+=`, and plain
map/filter/sum chains are about 1.3x a hand loop.

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
