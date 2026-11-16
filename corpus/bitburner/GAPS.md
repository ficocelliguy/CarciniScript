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
| A1 ✅ | **A local fn named like a `pub fn` in a `mod` file is emitted with the module prefix** | `batcher.rs` (`mod hwgw;`, both define `main`) | `export async function hwgw.main(ns)`: invalid JS, the script never loads. Compile reports OK. | `util.rs`: `pub fn helper()`. Main file: `mod util; fn helper() {..}` gives `export function util.helper()` |
| A2 ✅ | **Methods on `JsValue` are emitted as raw JS calls**: `as_string()`, `as_f64()`, `clone()` | 18 call sites in 12 files: every `ns.args` read, port reads, darknet `data` | `TypeError: a.as_string is not a function` | `ns.args.first().and_then(\|a\| a.as_string())` |
| A3 ✅ | **`Default` for `[T; N]` is an empty array** | `batcher.rs` `#[derive(Default)] struct Threads([u32; 4])` | Index panic `len is 0`. `{:?}` prints `T([])` where rustc prints `T([0, 0, 0, 0])` | `let x: [u8; 2] = Default::default();` gives `[]` |
| A4 ✅ | **`#[serde(untagged)]` is ignored** | `contracts.rs` `enum Answer` | `serde_json::to_string(&Answer::Int(6))` gives `{"Int":6}`, not `6`, so every contract attempt would fail | `#[serde(untagged)] enum A { I(i64) }` |
| A5 ✅ | `let a: String = ns.args[0];` is accepted | found while probing A2 | rustc rejects it (move out of index; `JsValue` isn't `String`). It works by accident, which hides A2 | — |

For A2, scripts need a typed way to read arguments. Two options: implement wasm-bindgen's
`JsValue::{as_string, as_f64, as_bool}` (and `clone`), or type `ns.args` as `Vec<ScriptArg>`
with `enum ScriptArg { Str(String), Num(f64), Bool(bool) }` (the d.ts says
`string | number | boolean`). `ns.flags(..)` would ideally return a typed struct too.
Workaround today: `format!("{a}")`.

## B. Netscript bindings

| # | Gap | Hit by | Why it matters |
|---|---|---|---|
| B1 ✅ | **NS interface types can't be constructed or named in user code.** `RunOptions { threads: Some(t), temporary: Some(true), ..Default::default() }`, `BasicHGWOptions { additional_msec }`, `HeartbleedOptions { peek }`, and `Server { money_available, ..base }` fail with "cannot find struct". A field of type `DarknetServerDetails` or an `impl AddAssign<CacheReward>` fails with "type `X` is not supported yet". | `batcher.rs`, `hwgw.rs`, `darknet.rs`, `dn_tasks.rs` | **Batchers can't set `additionalMsec`**, which is the standard HWGW timing mechanism. Without it there's no `temporary`, `preventDuplicates` or `ramOverride` on exec/run, and no heartbleed `peek`. Can't keep server details in a struct, or model a "prepped" server for formulas (clone-and-mutate works). |
| B2 ✅ | **Fields from TS intersection types are dropped**: `heartbleed()` returns `DarknetResult & { logs: string[] }`, `authenticate()` returns `& { data?: any }`, `getServerDetails()` returns `& { isOnline: boolean }`, `getServer()` returns `& { isOnline?: boolean }` | `darknet.rs` | **Most darknet models can't be solved without these.** GuessNumber, Yesn't, DeepGreen (mastermind), SpiceLevel, Factori-Os, BigMo%od, KingOfTheHill and OpenWebAccessPoint all read `data` or `logs`, and the labyrinth's map is in `data`. Only the statically decodable models work today. |
| B3 ✅ | **`Player` isn't accepted where `Person` is expected** (TS `Player extends Person`) | `batcher.rs`, every `ns.formulas.hacking.*` call | Formulas are the core of any serious batcher. The workaround `ns.formulas.mock_person()` compiles but gives wrong numbers. |
| B4 ✅ | **NS returns that matter are untyped `JsValue`**: `stock.getPosition` (a 4-tuple), `go.makeMove`/`passTurn`/`opponentNextTurn`/`getGameState` (objects), `codingcontract.getData` (`any`), `ResetInfo.ownedSF` (`Map<number, number>`), `ns.args` | `stocks.rs`, `go.rs`, `contracts.rs` | Combined with A2, `JsValue` has no safe operations, so these can only be reached through `JSON::stringify`/`parse` round trips. Tuples could map to tuples or `[f64; 4]`, and `Map` to `HashMap`. |
| B5 ✅ | No `JsValue` ↔ serde bridge (`serde_wasm_bindgen::{from_value, to_value}`, or `JsValue::into_serde`) | `contracts.rs` | Needed for `getData` → `serde_json::Value` and for `attempt(answer)`. Workaround: `serde_json::from_str(&JSON::stringify(&v))`. |

## C. Language and type system

| # | Gap | Hit by | Minimal repro / note |
|---|---|---|---|
| C1 ✅ | **Trait impls and type aliases don't cross `mod` file boundaries.** `Deref` coercion (`&Host` to `&str`), explicit `&**h`, autoderef methods (`h.len()`) and `pub type Op = fn(..)` all fail when declared in `net.rs`. All of them work within one file. | `net.rs` + `batcher.rs`/`deploy.rs`/`contracts.rs` | A hostname newtype in a shared module is the natural design. 25 call sites needed `&host.0` instead. |
| C2 ✅ | **A user type named like a std type collides with it**: `enum Cell` inside `Vec<Cell>`, `type Output = Cell`, or `Result<Self, char>` in a `TryFrom` impl gives "expected `RefCell<_>`, found `Cell`" | `go.rs` | `enum Cell {..} struct B { cells: Vec<Cell> }`. Board games name their squares `Cell`. |
| C3 ✅ | **`entry(k).or_default()` with a struct value** fails: "`B: Default` is not satisfied" even with `#[derive(Default)]` | `dashboard.rs`, `stocks.rs` | `HashMap<i32, B>` then `m.entry(1).or_default().n += 1`. Works for numbers. Per-target stats maps are everywhere. |
| C4 ✅ | **Format strings can't capture `const`s**: `"{CYAN}text{RESET}"` gives "cannot find value `CYAN`" | `dashboard.rs` | ANSI color consts in tail output are standard Bitburner style. Locals work. |
| C5 ✅ | **Irrefutable array patterns are rejected**: `let [w, x, y, z] = arr;` gives "refutable pattern" | `stocks.rs`, `contracts.rs` | Also `let [r, c] = <[i64; 2]>::try_from(v).ok()?` (`try_from` is missing too) |
| C6 ✅ | **`Option<()>` and `Option<Option<T>>`** are rejected (documented representation limit) | `servers.rs` (`.then_some(()).with_context(..)?`), `stocks.rs` (`.then(\|\| opt).flatten()`) | Both are idioms that never store the nested value. Could be special-cased: `then_some(())` with an immediate combinator, and `flatten()` on a temporary. |
| C7 ✅ | **`Option<T>` comparisons** (`a > b`, `None < Some(1)`) | `batcher.rs` | Rust orders `None` below `Some`. Comes up with optional `Server` fields. |
| C8 ✅ | **Two `From` impls on one type are mis-registered**: `From<&str>` + `From<String>` gives "implements `From<String>`, `From<String>`" | `net.rs` `Host` | Separate from the documented `impl Into<X>` parameter gap, which `Crawl::from(ns, "home")` also hit |
| C9 ✅ | `impl AsRef<str>` for your types | `net.rs` `Host` | (`impl Drop` landed in the working tree during this survey and now works) |
| C10 ✅ | **`Into<f64>` and `DeserializeOwned` as type-parameter bounds** | `stocks.rs` `fn mean<I>() where I::Item: Into<f64>`; generic port reader `fn drain<T: DeserializeOwned>` | Typed port messages between scripts want `DeserializeOwned` |
| C11 ✅ | `derive(Copy)` on a struct with a `[T; N]` field: "`Copy` is not implemented for `Vec<u32>`" | `batcher.rs` | The README says `Copy` arrays are supported, but this derive fails |
| C12 ✅ | A named `const` as a const-generic argument in a type: `History<WINDOW>` | `stocks.rs` | Literal `History<20>` works. FEATURES says const items are supported, but not in field types. |
| C13 ✅ | `?` inside a `try_fold` closure without an explicit return type: "this function returns `_`" | `dn_models.rs` | rustc infers `Option<_>` from the accumulator. `\|acc, c\| -> Option<f64> {..}` works. |
| C14 ✅ | False mutability error: `let s = v.iter_mut().min_by(..)?; s.f -= 1.0;` gives "cannot borrow `s` as mutable" | `net.rs` `RamPool::reserve` | `s` is a `&mut`, so `let mut` isn't needed (rustc warns if added) |
| C15 ✅ | `&mut String::new()` as an argument gives a misleading "`&mut` inside a struct, tuple or Vec" error | `contracts.rs` | Binding it to a local first works |
| C16 ✅ | Async closures (`async \|x\| ..`, stable since 1.85) and `Pin<Box<dyn Future>>` / `Box::pin` for recursive async fns | darknet labyrinth DFS (written iteratively in the corpus to avoid this) | `async fn` in traits **does** work |
| C17 ✅ | Inference through labeled-`break` values when one arm is a `JsValue` | `go.rs` | Needs an annotation; minor |

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

## Status (2026-11-16)

Fixed since the survey (each with a differential fixture, an NS test or a rejection test):

- **A1** fixed: a module's functions and consts are also registered as `util.helper`, which a
  written `util::helper` path resolves to first; a local fn of the same name keeps the bare name
  (`test/modules/shadow`).
- **A2 / B4** fixed: `JsValue` is a real type with wasm-bindgen's methods (`as_string`, `as_f64`,
  `as_bool`, `is_*`, `clone`, `as_debug_string`, `JsValue::from_str` ..), also as function values
  (`JsValue::as_string`), run by the runtime's `κσ.$jsv`. Chosen over a `ScriptArg` enum because
  it's what wasm-bindgen users already write, it covers every untyped NS value (port reads,
  `getData`, `data?: any`), not only `ns.args`, and it keeps `ns.args` the `Vec<JsValue>` the
  bindings generate. Typed results where the d.ts has a shape: tuples are `[f64; 4]` / `(A, B)`,
  `Map` / `Record` are `HashMap`s (converted where read), inline object results are structs
  (`GoMakeMoveResult`). `ns.flags(..)` is still a `JsValue`.
- **A3** fixed: `Default` of `[T; N]` is N default elements, from the written type (derived
  `Default` fields, `let` annotations; literal or `const` lengths).
- **A4** fixed: `#[serde(untagged)]` (written as the content alone, read as the first variant
  that fits, serde_json's error otherwise).
- **A5** fixed: `JsValue` no longer unifies with everything; `let a: String = ns.args[0];` is a
  type error (a `JsValue` *parameter* still takes any value).
- **B1** fixed: NS interfaces are nameable types and buildable structs (`RunOptions { .., ..Default::default() }`,
  `Server { .., ..base }`, `RunOptions::default()`), lowered to plain objects without `None` fields.
- **B2** fixed: `Named & { extra }` results are structs of their own (`DarknetResultWithLogs`,
  `ServerWithIsOnline`, `DarknetServerDetailsWithIsOnline`) extending the named interface; `type X = A & {..}` aliases too (`CacheResult`).
- **B3** fixed: `interface X extends Y` is `#[extends = "Y"]`; an `X` is accepted where a `Y` is
  expected (arguments, `let` annotations, struct fields, `..base`, operator impls).
- **B5** fixed: `serde_wasm_bindgen::{from_value, to_value}` (through JSON with the static type's
  schema; errors are `js::JsonError` with serde's messages).
- **C1** fixed: a module's `Deref` / `DerefMut` targets, iterator items, associated types and const
  parameters, and its `pub type` aliases (without generics), are exported with its types
  (`test/modules/crossmod`). A comma in a closure's parameters inside a macro call
  (`vec![|a, b| a + b]`) no longer splits the arguments (found while testing this).
- **C2** fixed: a user type named `Cell` / `RefCell` is that type (also `Cell::new` of its own).
- **C3** fixed: `or_default()` accepts structs and enums with `Default`.
- **C4** fixed: format strings capture `const` / `static` items (also as `WIDTH$`).
- **C5** fixed: `let [a, b] = arr;` is irrefutable when the length is known (annotation, array
  literal, `<[T; N]>::try_from(..)`); `<[T; N]>::try_from` checks the length (`Err` of the `Vec`,
  or `TryFromSliceError` for a slice).
- **C7** fixed: `Option` ordering (`None` below `Some`).
- **C8** fixed: `From<&str>` next to `From<String>` (one source type here: the first impl serves).
- **C9** fixed: `impl AsRef` / `AsMut` / `Borrow` for your types.
- **C11** fixed: `derive(Copy)` on a struct with `Copy` array fields (tuple fields too).
- **C12** fixed: a `const` item as a const argument in a field type (`History<WINDOW>`).
- **C13** fixed: `?` in a closure without a return type makes it return `Option` / `Result`.
- **C14** fixed: an element taken from `iter_mut()` (`min_by(..)?`, `find`, `next().unwrap()`) is
  a `&mut`.

Still open: C6, C10, C15 to C17, the `impl Into<X>` parameter given another type
(`Crawl::from(ns, "home")`), `(*h).clone()` on a `&&Host` (refs are erased, so `*` derefs the
`Host` to its `str`), section D, and CarciniScript's extension of integer literals for `f64`
only at NS calls (`stocks.rs` compares an `f64` with `2`, which rustc rejects too).

As written, 5 of the 14 files compile now (`dashboard.rs`, `dn_tasks.rs`, `early_hack.rs`,
`hacknet.rs`, `hwgw.rs`); the rest stop at the open items above (mostly section D).

### Re-check (2026-11-16)

Rebuilt against the working tree and re-ran the rustc stub, the 47 probes, and the mock runs.

- **Confirmed:** every emitted module parses (A1). The 17 probes for fixed items print what
  rustc prints. The batcher scores, preps and plans against the mock with real
  `RunOptions`/`BasicHGWOptions`. The darknet crawler cracks all 9 solvable mock servers, including
  the feedback-driven ones (guess-number, Yesn't, DeepGreen) that B2 unblocked.
- **Corpus fix:** `stocks.rs` compared an `f64` level from `owned_sf` (now typed) to `2`; it's `2.0` now.
- **Partly fixed:**
  - C5: `let [a, b] = x;` still fails when `x` is a variable, a fn result or a field. Only literals, annotations and `try_from` work.
  - C9: a user type passed to an `impl AsRef<str>` / `S: AsRef<str>` parameter fails ("expected `String`, found `H`"). `h.as_ref()` works.
- **New, compiles but wrong at run time (A6):** `for (slot, v) in opts.iter_mut().zip(..)` over a
  `Vec<Option<T>>` with `*slot = Some(..)` emits an assignment to a `const` and throws
  "Assignment to constant variable". The same loop over numbers is correctly rejected; plain
  `iter_mut()` over Options works. Hit by `darknet.rs` `yesnt`.
- With ~20 one-line workarounds (all for the open items above), all 14 files compile again.

### All 25 darknet models (2026-11-16, later)

`darknet.rs` now solves the three models it reported as unsupported, following bitburner-src's
`authentication.ts`:

- **TopPass:** a dictionary attack over the game's 93 common passwords
  (`common_passwords.txt`, embedded with `include_str!`).
- **BigMo%od:** a probe `n` ≤ 32 answers `password % n`. The 11 probes 32, 27, 25, 7 … 31 pin the password
  modulo lcm(1..=32) ≈ 1.4e14 (`i128` extended-gcd CRT). Then it tries the few numbers with the right digit count.
- **KingOfTheHill:** the solver scans the range at half a hill width, so the best sample is on the 10 000 m hill and not a side
  hill. It then jumps to `x ± w·√ln(10000/alt)` until it lands on the peak.

Checks: the new math has native unit tests (8 pass). The corpus passes rustc against the stub, apart from the four
stub-only subtype mismatches. The darknet mock now cracks 14 servers. TopPass took 93 tries, a 15-digit BigMo%od 17,
and KingOfTheHill 182 (6 digits) and 28 (2 digits), with the game's hill formula including side hills.
`u128`/`i128` arithmetic (BigInt) matches rustc exactly.

New gaps this hit:

| # | Gap | Where | Note |
|---|---|---|---|
| A7 ✅ | **`step_by` on a range is O(range), not O(steps)** | `Congruence::with_digits` `(first..hi).step_by(lcm)`, the KingOfTheHill scan `(lo..=hi).step_by(w/2)` | `StepByIt` calls `next()` n times per step. `(0..2e8).step_by(5e7).count()` takes 1.7 s. The 15-digit CRT candidate walk never finishes, and a 10-digit hill takes 165 s of synchronous CPU (~0.9 s between attempts) where rustc is instant. Compiles fine, then freezes the game. Ranges could jump with `nth`. |
| C18 ✅ | `Option::into_iter()` / `Option::iter()`, and `Option` as `IntoIterator` (`.chain(None)`, `.flatten()` over `Option<[T; N]>`) | `king_of_the_hill` | "no method named `into_iter` found for `Option<..>`" |
| C19 ✅ | `include_str!` | `dn_models::common_passwords` | Natural way to ship a word list next to a script; the resolver that loads `mod` files could serve it |
| C6 ✅ (idioms) | `.map(Some)` making a temporary `Option<Option<f64>>` before `ok_or` | `king_of_the_hill`'s `altitude` | Still rejected, though `Option<()>` and `then(..).flatten()` now work |

What else the new code used that worked: nested `fn` / `async fn` items, recursive tuple-returning
fns, `i128::rem_euclid`, `u128::from`, `10u128.pow`, `const fn`, `impl Iterator` returning a
`move` closure, `drain(..)`, `dedup`, `try_fold` into a struct.

Since the first re-check the working tree also fixed most of section D, C6's `Option<()>`, C10 and
`VecDeque::from(vec)`: 9 of 14 files compile as written. Still failing: `contracts.rs`
(C15), `deploy.rs` (`(*h).clone()`), `net.rs` (`impl Into` param), and the darknet pair on the
gaps above. Probes still failing: C5 (`let [a, b] = fn_result;`), C9 (`impl AsRef<str>` param),
C15, C16, `Option<Option<T>>`, `impl Into<X>` params, and `std::error::Error::source` on a user error
(now "no method named `source`"). A6 is still open.

### Section D, C6, C10 and C18 (2026-11-16, later)

Fixed, each diff-tested against rustc (`test/fixtures/std_surface_gaps.rs`,
`test/fixtures/option_unit_bounds.rs`):

- **D** fixed: `split_first` / `split_last`, `BinaryHeap::into_iter`, `VecDeque::from(vec)` and
  `vec.into()`, the `hash_map::Entry` enum (`if let Entry::Vacant(e) = m.entry(k)`, `match` with
  `*e.get_mut() += 1`), `Rc::default()`, `checked_add_signed`, `is_multiple_of`,
  `std::iter::repeat_n`, `Vec::len` / `char::from` / `Into::into` as values, and
  `std::error::Error::source(&e)` (also on a user error without its own `source`).
- **C6** fixed for the idioms: `Option<()>` (`then_some(())` with `?` / `with_context`, `HashMap<K, ()>`),
  and `Option<Option<T>>` as a temporary that `.flatten()` reads (`.then(|| opt).flatten()`,
  `v.first().copied().flatten()`) or that `.map(Some)` makes (`altitude`). A stored nested
  option is still rejected.
- **C10** fixed: `T: Into<f64>` / `where I::Item: Into<f64>` (any number type), and
  `T: DeserializeOwned` (`serde_json::from_str::<T>` in a generic port reader).
- **C18** fixed: `Option::iter()` / `into_iter()`.

As written, 10 of the 14 files compile now. Still failing: `contracts.rs` (C15), `deploy.rs`
(`(*h).clone()` on a `&&Host`), `net.rs` (an `impl Into<Host>` parameter given a `&str`),
`dn_models.rs` (C19, `include_str!`).

### The rest of the corpus (2026-11-16, evening)

All 14 files compile as written now. Fixed, each with a differential fixture
(`test/fixtures/corpus_idioms.rs`, `array_len_asref.rs`, `crate_chrono_tz.rs`) or a
module test (`test/modules/include`):

- **A6** fixed: `for (slot, v) in opts.iter_mut().zip(..)` is an index loop over `opts`, so
  `*slot = Some(..)` writes through (numbers too, which were rejected).
- **A7** fixed: `step_by` over a range jumps; `(0..2e9).step_by(5e8).count()` is 4 steps.
- **C5** fixed: `let [a, b] = x;` with `x`'s length from a local's `let`, a parameter, a
  function's or method's return type, a field or a `const`.
- **C9** fixed: your type passed for an `impl AsRef<str>` / `S: AsRef<str>` parameter.
- **C15** fixed: `&mut String::new()` (any temporary) as a `&mut` argument.
- **C19** fixed: `include_str!` / `include_bytes!`, through the resolver that loads `mod` files.
- `impl Into<Host>` parameters given a `&str` / `String` (`Crawl::from(ns, "home")`), also
  `T: Into<X>`, in functions, associated functions and methods.
- `(*h).clone()` where `h` is a `filter` / `find` / `retain` / `sort_by` .. closure's parameter:
  the `*` drops the reference (a `Host`), not `Host`'s `Deref` (a `str`).

### C16, C17 and the re-check's leftovers (2026-11-16, evening)

- **C16** fixed: async closures (`async |x| ..`, `async move`), `AsyncFn` /
  `F: Fn(A) -> Fut, Fut: Future<Output = R>` bounds, and `Pin<Box<dyn Future>>` with
  `Box::pin` for recursive async fns (`test/fixtures/async_closures.rs`).
- **C17** fixed: `go.rs` already compiled; the remaining shape, a `Default::default()` typed by
  a later `break` / arm, is checked at the end of the function (`labeled_break_infer.rs`).
- `Some(NaN)` ordering (re-check) fixed: NaN makes every partial comparison false, through
  `Option`, tuples, `Vec`s and derived `PartialOrd` (`option_nan_cmp.rs`).

Every item in the tables above is fixed. C6 in full since: `Some(None)` is a marker of its
own, so an `Option<Option<T>>` can be stored, passed and read back (`test/fixtures/option_nested.rs`).

### React scripts (2026-11-16, evening)

`cat.rs` (a port of `cat.jsx`) and `monitor.rs` (a live table of rooted servers) draw their tail
windows with `jsx!` and the bundled `react` hooks; both compile as written, and `monitor.rs` was
run with React 17 in jsdom against a mock network (refresh, sort and filter buttons). The
corpus' rustc stub needs `jsx!` as a `macro_rules!` taking any tokens (see PLAN.md).

## Suggested order

1. A1 (invalid JS) and A2/B4 (a typed `ns.args` and `JsValue` methods): both break ordinary scripts.
2. B1 (constructible option structs, nameable NS types) and B3 (`Player` → `Person`): batchers.
3. B2 (intersection fields): darknet.
4. A3, A4: silent data bugs.
5. C1 to C5: the errors a Rust user hits first in multi-file scripts.
