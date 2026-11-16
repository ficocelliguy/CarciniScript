# CarciniScript gaps found by the Bitburner corpus

What the scripts in this folder need that CarciniScript doesn't do yet, ranked by how much it
hurts real Bitburner scripts. Every item was hit by a corpus script, then reduced to a minimal
repro. Results are from the working tree as of 2026-11-15 (on top of `3ec3593`, including the
uncommitted compiler changes at the time), compiled with `bindings/ns.rs` loaded.

## How this was measured

- **The corpus is real Rust.** Every file type-checks and borrow-checks under rustc 2024 against
  an NS stub generated from `bindings/ns.rs`. The stub models what typed bindings would look
  like: `RunOptions` and friends constructible, intersection fields present, `Player: Into<Person>`.
  The only rustc errors filtered out are CarciniScript's documented NS extensions: omitted
  optional args, variadics, and integer literals for `f64`. `dn_models.rs` also passes its unit
  tests natively.
- **As written, 1 of 14 files compiles** (`early_hack.rs`).
- **With workarounds, 14 of 14 compile.** A scratch copy was patched one error at a time to
  uncover errors hidden behind earlier ones (26 diff hunks).
- **Runtime checks:**
  - A program exercising every `dn_models` decoder and all 23 contract solvers was run under
    rustc and under CarciniScript. All logic matched, but answer serialization differed (A4).
  - The darknet crawler ran against a mock darknet: it cracked 6 of the 7 servers (one has an
    unknown model on purpose), wrote the vault, and spread.
  - The batcher ran against a mock network and produced correct HWGW delays (3000/20/840/60 ms).
    Both runs needed the A1 to A3 workarounds first.
- `npm run sweep` over the corpus: no compiler panics.

## A. Compiles but is wrong at run time (fix first)

These are the worst kind: the editor shows no error, and the script crashes or does the wrong thing.

| # | Gap | Hit by | Symptom | Minimal repro |
|---|---|---|---|---|
| A1 | **A local fn named like a `pub fn` in a `mod` file is emitted with the module prefix** | `batcher.rs` (`mod hwgw;`, both define `main`) | `export async function hwgw.main(ns)`: invalid JS, the script never loads. Compile reports OK. | `util.rs`: `pub fn helper()`. Main file: `mod util; fn helper() {..}` gives `export function util.helper()` |
| A2 | **Methods on `JsValue` are emitted as raw JS calls**: `as_string()`, `as_f64()`, `clone()` | 18 call sites in 12 files: every `ns.args` read, port reads, darknet `data` | `TypeError: a.as_string is not a function` | `ns.args.first().and_then(\|a\| a.as_string())` |
| A3 | **`Default` for `[T; N]` is an empty array** | `batcher.rs` `#[derive(Default)] struct Threads([u32; 4])` | Index panic `len is 0`. `{:?}` prints `T([])` where rustc prints `T([0, 0, 0, 0])` | `let x: [u8; 2] = Default::default();` gives `[]` |
| A4 | **`#[serde(untagged)]` is ignored** | `contracts.rs` `enum Answer` | `serde_json::to_string(&Answer::Int(6))` gives `{"Int":6}`, not `6`, so every contract attempt would fail | `#[serde(untagged)] enum A { I(i64) }` |
| A5 | `let a: String = ns.args[0];` is accepted | found while probing A2 | rustc rejects it (move out of index; `JsValue` isn't `String`). It works by accident, which hides A2 | — |

For A2, scripts need a typed way to read arguments. Two options: implement wasm-bindgen's
`JsValue::{as_string, as_f64, as_bool}` (and `clone`), or type `ns.args` as `Vec<ScriptArg>`
with `enum ScriptArg { Str(String), Num(f64), Bool(bool) }` (the d.ts says
`string | number | boolean`). `ns.flags(..)` would ideally return a typed struct too.
Workaround today: `format!("{a}")`.

## B. Netscript bindings

| # | Gap | Hit by | Why it matters |
|---|---|---|---|
| B1 | **NS interface types can't be constructed or named in user code.** `RunOptions { threads: Some(t), temporary: Some(true), ..Default::default() }`, `BasicHGWOptions { additional_msec }`, `HeartbleedOptions { peek }`, and `Server { money_available, ..base }` fail with "cannot find struct". A field of type `DarknetServerDetails` or an `impl AddAssign<CacheReward>` fails with "type `X` is not supported yet". | `batcher.rs`, `hwgw.rs`, `darknet.rs`, `dn_tasks.rs` | **Batchers can't set `additionalMsec`**, which is the standard HWGW timing mechanism. Without it there's no `temporary`, `preventDuplicates` or `ramOverride` on exec/run, and no heartbleed `peek`. Can't keep server details in a struct, or model a "prepped" server for formulas (clone-and-mutate works). |
| B2 | **Fields from TS intersection types are dropped**: `heartbleed()` returns `DarknetResult & { logs: string[] }`, `authenticate()` returns `& { data?: any }`, `getServerDetails()` returns `& { isOnline: boolean }`, `getServer()` returns `& { isOnline?: boolean }` | `darknet.rs` | **Most darknet models can't be solved without these.** GuessNumber, Yesn't, DeepGreen (mastermind), SpiceLevel, Factori-Os, BigMo%od, KingOfTheHill and OpenWebAccessPoint all read `data` or `logs`, and the labyrinth's map is in `data`. Only the statically decodable models work today. |
| B3 | **`Player` isn't accepted where `Person` is expected** (TS `Player extends Person`) | `batcher.rs`, every `ns.formulas.hacking.*` call | Formulas are the core of any serious batcher. The workaround `ns.formulas.mock_person()` compiles but gives wrong numbers. |
| B4 | **NS returns that matter are untyped `JsValue`**: `stock.getPosition` (a 4-tuple), `go.makeMove`/`passTurn`/`opponentNextTurn`/`getGameState` (objects), `codingcontract.getData` (`any`), `ResetInfo.ownedSF` (`Map<number, number>`), `ns.args` | `stocks.rs`, `go.rs`, `contracts.rs` | Combined with A2, `JsValue` has no safe operations, so these can only be reached through `JSON::stringify`/`parse` round trips. Tuples could map to tuples or `[f64; 4]`, and `Map` to `HashMap`. |
| B5 | No `JsValue` ↔ serde bridge (`serde_wasm_bindgen::{from_value, to_value}`, or `JsValue::into_serde`) | `contracts.rs` | Needed for `getData` → `serde_json::Value` and for `attempt(answer)`. Workaround: `serde_json::from_str(&JSON::stringify(&v))`. |

## C. Language and type system

| # | Gap | Hit by | Minimal repro / note |
|---|---|---|---|
| C1 | **Trait impls and type aliases don't cross `mod` file boundaries.** `Deref` coercion (`&Host` to `&str`), explicit `&**h`, autoderef methods (`h.len()`) and `pub type Op = fn(..)` all fail when declared in `net.rs`. All of them work within one file. | `net.rs` + `batcher.rs`/`deploy.rs`/`contracts.rs` | A hostname newtype in a shared module is the natural design. 25 call sites needed `&host.0` instead. |
| C2 | **A user type named like a std type collides with it**: `enum Cell` inside `Vec<Cell>`, `type Output = Cell`, or `Result<Self, char>` in a `TryFrom` impl gives "expected `RefCell<_>`, found `Cell`" | `go.rs` | `enum Cell {..} struct B { cells: Vec<Cell> }`. Board games name their squares `Cell`. |
| C3 | **`entry(k).or_default()` with a struct value** fails: "`B: Default` is not satisfied" even with `#[derive(Default)]` | `dashboard.rs`, `stocks.rs` | `HashMap<i32, B>` then `m.entry(1).or_default().n += 1`. Works for numbers. Per-target stats maps are everywhere. |
| C4 | **Format strings can't capture `const`s**: `"{CYAN}text{RESET}"` gives "cannot find value `CYAN`" | `dashboard.rs` | ANSI color consts in tail output are standard Bitburner style. Locals work. |
| C5 | **Irrefutable array patterns are rejected**: `let [w, x, y, z] = arr;` gives "refutable pattern" | `stocks.rs`, `contracts.rs` | Also `let [r, c] = <[i64; 2]>::try_from(v).ok()?` (`try_from` is missing too) |
| C6 | **`Option<()>` and `Option<Option<T>>`** are rejected (documented representation limit) | `servers.rs` (`.then_some(()).with_context(..)?`), `stocks.rs` (`.then(\|\| opt).flatten()`) | Both are idioms that never store the nested value. Could be special-cased: `then_some(())` with an immediate combinator, and `flatten()` on a temporary. |
| C7 | **`Option<T>` comparisons** (`a > b`, `None < Some(1)`) | `batcher.rs` | Rust orders `None` below `Some`. Comes up with optional `Server` fields. |
| C8 | **Two `From` impls on one type are mis-registered**: `From<&str>` + `From<String>` gives "implements `From<String>`, `From<String>`" | `net.rs` `Host` | Separate from the documented `impl Into<X>` parameter gap, which `Crawl::from(ns, "home")` also hit |
| C9 | `impl AsRef<str>` for your types | `net.rs` `Host` | (`impl Drop` landed in the working tree during this survey and now works) |
| C10 | **`Into<f64>` and `DeserializeOwned` as type-parameter bounds** | `stocks.rs` `fn mean<I>() where I::Item: Into<f64>`; generic port reader `fn drain<T: DeserializeOwned>` | Typed port messages between scripts want `DeserializeOwned` |
| C11 | `derive(Copy)` on a struct with a `[T; N]` field: "`Copy` is not implemented for `Vec<u32>`" | `batcher.rs` | The README says `Copy` arrays are supported, but this derive fails |
| C12 | A named `const` as a const-generic argument in a type: `History<WINDOW>` | `stocks.rs` | Literal `History<20>` works. FEATURES says const items are supported, but not in field types. |
| C13 | `?` inside a `try_fold` closure without an explicit return type: "this function returns `_`" | `dn_models.rs` | rustc infers `Option<_>` from the accumulator. `\|acc, c\| -> Option<f64> {..}` works. |
| C14 | False mutability error: `let s = v.iter_mut().min_by(..)?; s.f -= 1.0;` gives "cannot borrow `s` as mutable" | `net.rs` `RamPool::reserve` | `s` is a `&mut`, so `let mut` isn't needed (rustc warns if added) |
| C15 | `&mut String::new()` as an argument gives a misleading "`&mut` inside a struct, tuple or Vec" error | `contracts.rs` | Binding it to a local first works |
| C16 | Async closures (`async \|x\| ..`, stable since 1.85) and `Pin<Box<dyn Future>>` / `Box::pin` for recursive async fns | darknet labyrinth DFS (written iteratively in the corpus to avoid this) | `async fn` in traits **does** work |
| C17 | Inference through labeled-`break` values when one arm is a `JsValue` | `go.rs` | Needs an annotation; minor |

## D. Standard library surface

All hit by corpus code; each fails with "no method" or "cannot find function".

- **Slices and collections:** `split_last`, `BinaryHeap::into_iter` (`into_vec` works),
  `VecDeque: From<Vec<T>>` (and `.into()`), `<[T; N]>::try_from(vec)`, the `hash_map::Entry`
  enum (`if let Entry::Vacant(e) = m.entry(k)`, or `entry` without `or_insert`), `Rc::default()`.
- **Integers:** `usize::checked_add_signed` (grid neighbours), `u64::is_multiple_of` (1.87; now
  what clippy suggests over `% == 0`).
- **Iterators:** `std::iter::repeat_n`.
- **Paths as values:** `Vec::len`, `char::from`, `Into::into`, `JsValue::as_string` (method calls
  work, paths to them don't).
- **UFCS on std traits:** `std::error::Error::source(&e)` (`e.source()` with the trait imported
  works).

## What worked

Nearly everything else the corpus leans on compiled and behaved like rustc:

- **Traits and generics:** `async fn` in traits; generic `async fn first_success<I>(..) where
  I::Item: AsRef<str>`; `impl Iterator` structs with lifetimes; `Index`/`IndexMut` keyed by an
  enum; `BinaryHeap<(Reverse<_>, usize)>` with a hand-written `Ord`; `Box<dyn Fn(&NS, &str) -> f64>`
  strategy maps; fn-pointer tables in a `const` (same file); const generics with literals;
  `#[macro_export]` macros through `#[macro_use] mod`.
- **Control flow:** `let`-`else`, `let` chains, labeled `break`/`continue` with values.
- **Errors:** thiserror (`#[from]`, `#[source]`, `transparent`) and anyhow (`bail!`,
  `with_context`, `main -> Result`).
- **Crates:** serde derive (`rename_all`, `default`); itertools (`permutations`, `unique`,
  `chunk_by`, `counts`); a regex in a `LazyLock` static; a `Peekable<Chars>` lexer with a
  recursive-descent parser.
- **Formatting and time:** `f.pad`, `{:.3e}`, `{:.1?}` on `Duration`, `Instant`.
- **Shared state:** `Rc<RefCell<..>>` captured by an `ns.at_exit` closure.

## Suggested order

1. A1 (invalid JS) and A2/B4 (a typed `ns.args` and `JsValue` methods): both break ordinary scripts.
2. B1 (constructible option structs, nameable NS types) and B3 (`Player` → `Person`): batchers.
3. B2 (intersection fields): darknet.
4. A3, A4: silent data bugs.
5. C1 to C5: the errors a Rust user hits first in multi-file scripts.
