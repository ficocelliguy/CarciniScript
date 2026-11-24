# CarciniScript: Rust compatibility gaps

Places where CarciniScript silently behaves differently from rustc, or accepts programs rustc
would reject. The counterpart to RapydScript-NS's `PYTHON_GAPS.md`. Unsupported features (which
produce a compile error) are tracked in [FEATURES.md](FEATURES.md) instead.

The gaps fall into three parts:

1. [What a Bitburner script may run into](#1-what-a-bitburner-script-may-run-into): differences
   that ordinary scripts hit, whether or not they're planned to change.
2. [Not planned](#2-not-planned): what follows from running on one JS thread with a garbage
   collector and no OS (threads, pointers, addresses, files, processes).
3. [Minor differences](#3-minor-differences): edge cases that scripts are unlikely to meet.

# 1. What a Bitburner script may run into

## Netscript values

- **NS numbers are `f64`, except whole numbers, which are `i32`.** TypeScript has one number
  type; the bindings generator picks the whole-number members out by name (threads, pids, ports,
  cores, levels, counts, indexes, Go and Stanek coordinates: `RunningScript.threads`,
  `ns.get_server_num_ports_required()`, `ns.hacknet.num_nodes()`, `Server.cpu_cores`, the
  `Skills` fields; `isWholeNumber` in `tools/dts2rs.mjs`), so `pid == 0` and
  `for i in 0..ns.hacknet.num_nodes()` type-check and `v[n as usize]` is the only cast. Money,
  RAM, time, rates, multipliers, reputation, favor, hashes, shares, contract data and anything that
  can be `Infinity` stay `f64`; convert with `as i32` / `as usize` before matching on integer
  patterns or indexing. An `i32` result is the same JS number: `/` is integer division on it, as
  in Rust, so write `threads as f64 / cores as f64` for a ratio.
- **Deliberate extensions over Rust for NS calls:**
  - Trailing optional parameters may be omitted (`ns.hack(target)`). When passed, they take the
    plain type, not `Option<T>`.
  - A `...rest` parameter takes any number of arguments (`ns.tprint(a, b, c)`).
  - An unsuffixed integer literal is accepted for an `f64` parameter (`ns.sleep(100)`). Integer
    *variables* still need `as f64`. For an enum payload that is a number (`ns.exec(.., threads)`
    with `RunThreadOrOptions::Num(i32)`, `FilenameOrPID::Num(i32)`), any integer that is a JS
    number (`usize`, `i64`, ..) is accepted as it is.
- **Some NS values are `JsValue`s.** Unions of different types, generics, callbacks and inline
  object types in parameters have no Rust equivalent yet. A `JsValue` result is read with
  wasm-bindgen's `as_string()` / `as_f64()` / `as_bool()` (or `serde_wasm_bindgen::from_value`);
  a `JsValue` parameter takes any value, unchecked.
- **NS interfaces are structurally typed in TS.** Here an interface is accepted where one it
  `extends` is expected (`Player` for `Person`), and an intersection result is its own struct
  (`ServerWithIsOnline` for `Server & { isOnline?: boolean }`) accepted as the named interface.
  Other structural matches (two unrelated interfaces with the same fields) aren't.
- **NS interfaces built in Rust are plain JS objects**: `RunOptions { .. }` needs every field
  unless it has `..base`, as in Rust; `Default::default()` exists only for interfaces whose
  fields are all optional.
- **`Map` and `Record` results are copied into a `HashMap`** where they're read (`owned_sf` is
  rebuilt on each read; changing it doesn't change the game's). `Map<number, V>` keys are `i32`.

## Integers

- **The 64-bit integers (`i64`, `u64`, `usize`, `isize`) are JS numbers**, exact up to 2^53
  (lengths, indexes and every figure the game hands out stay far below). Past that, values round
  (a literal that would warns), `+ - *` and the `wrapping_*` methods don't wrap at 2^64, `MAX` /
  `MIN` are ±(2^53 − 1), and casting a negative value to `u64` / `usize` keeps the sign. Bit
  operations and shifts are exact and do wrap to 64 bits. So a 64-bit hash or random number
  generator (FNV-1a, xorshift, SplitMix, `wrapping_mul` by a large odd constant) doesn't give
  rustc's values as `u64`; write it over `u128` (exact BigInts, wrapping at 128 bits) and mask
  with `& 0xffff_ffff_ffff_ffff` where the width matters. The trade is speed: a `u64` loop runs
  at `f64`'s pace instead of 10x slower as a BigInt, and `u64` is what players write for money
  and thread math. `DefaultHasher::finish()` and `rng.next_u64()` give 53-bit values. This is a
  deliberate choice and isn't planned to change.
- **Overflow wraps by default.** Arithmetic follows rustc's release-mode semantics (two's-complement
  wrapping). With overflow checks on (`setOverflowChecks(true)`, or Bitburner's "Rust: panic on
  integer overflow" option) it panics like a debug build.
- **Index panics on underflowed `usize`s show a negative index.** `usize` arithmetic doesn't wrap
  (see above), so `v[i - 1]` with `i == 0` panics with "the index is -1" instead of rustc's
  18446744073709551615.

## Scripts split over several files

- **Every file is its own crate root for `mod`.** `mod x;` in `util.rs` loads `x.rs` next to
  `util.rs` when there is one, since each Bitburner script is compiled on its own (and may be run
  directly); otherwise `x/mod.rs`, then `util/x.rs` like rustc. For the same reason `super::`
  works only inside inline `mod x { .. }` blocks, not at the top of a module file, and `crate::`
  names this file. A module file's generic `pub type` aliases (`pub type Grid<T> = ..`) aren't
  usable from the file that loads it (plain ones are).
- **Types share one namespace across files.** A `mod` file's struct or enum named like one of
  the loading file's, or like one an earlier `mod` file exports, goes by a mangled name there
  (`bytes::Shlex` is `Shlex__bytes`), which hover and error messages show. A file's own
  top-level `Vec` is `Vec__file` in the same way, so the bundled preludes keep std's. Traits, type aliases
  and constants aren't renamed that way: a `mod` file's trait named like this file's is still
  "defined multiple times". A renamed import of another file's or a crate's type, trait or
  constant (`use geo::Point as P;`, `pub use area::Error as AreaError;`) is written back under
  its own name, so such a rename inside a block is an error. A renamed re-export reaches other
  files through a named `use` (`use geo::AreaError;`), not through `use geo::*`.
- **Macros from other files come through `mod`**: a file sees the `#[macro_export]` macros of the
  files it loads with `mod x;` (and all of them with `#[macro_use]`), not those of files further
  away. A macro's path import is dropped: `pub(crate) use my_macro;` and `use crate::{my_macro,
  ..}` are removed with the definitions, unless an item of the file has that name too. Another
  file's `use crate::my_macro;` is still unresolved.
- **Inline modules of `mod` files are reached by their items' flattened names.** `use
  geo::shapes::Circle;` (with `pub mod shapes { .. }` in `geo.rs`) works, but an error about
  the type may name it `shapes__Circle`. A `use` inside a block (`fn main() { use geo::Point as
  P; .. }`) is recorded with the enclosing module's, so the alias is visible past the block.
- **A `mod` file's traits implemented for built-in types in the file that loads it** join the
  module's registry, which is shared by every script that loads that module: when two
  scripts implement one of its traits for the same type, the module's generic functions use
  the last script's impl (a re-exported trait's module isn't reached).

## Not supported yet

Compile errors in code a script could plausibly contain:

- **A whole item of a `Vec<(usize, &mut T)>` collected from `iter_mut()`** over numbers,
  strings or bools, passed on as `&mut` (`f(&mut rs[1])`, `Some(&mut rs[1])`). Bind it
  (`let t = &mut rs[1];`, `let (i, r) = &mut rs[1];`), write through its fields
  (`*rs[1].1 = 9`), or pass `&rs[1]` (a copy with the values read, which is all a
  `&(usize, &mut T)` can see). (A `Vec<&mut T>` indexes: `*rs[1] = 9`.)
- **A `get_mut` / `peek_mut` result turned into a `Result` kept in a local and then used other
  than matched, `?`-ed or unwrapped.** Of a number, string or bool, `HashMap::get_mut`,
  `Vec::get_mut` and `Peekable::peek_mut` are a slot over the entry, element or buffered item:
  matched (`if let Some(v) = m.get_mut(k)`), unwrapped (`*m.get_mut(k).unwrap() += 1`,
  `.unwrap_or(&mut 0)`, `.unwrap_or_else(|| &mut spare)`, `m.get_mut(k).unwrap().push('!')`),
  kept in a local, returned from your own `-> Option<&mut f64>`, passed to an `Option<&mut i32>`
  parameter, given to `map` / `and_then` / `map_or` / `map_or_else` / `is_some_and` /
  `is_none_or` / `inspect` (`m.get_mut(k).map(|v| *v += 1)`), or turned into a `Result`
  (`*m.get_mut(k).ok_or(e)? += 1`; `let r = m.get_mut(k).ok_or(e);` then `r?`, `r.unwrap()`,
  `r.is_ok()`, `match r { Ok(v) => .. }` / `if let Ok(v) = r`); `copied()`, `cloned()`,
  `is_some()` and `== None` read a copy. A kept `Result` given to `map`, printed or passed on,
  and an `ok_or(e)` returned as a `Result<&mut i32, E>`, are a compile error.

## Silent differences in common code

- **A tuple holding a `&mut` to a number, string or bool holds a slot only where it's built or
  received in sight**: a `let` of a tuple literal (`let t = (1, &mut x);`) or with a written
  tuple type, a parameter written `(usize, &mut i32)` / `&mut (usize, &mut i32)`, and what
  those are bound to or borrowed into (`let (i, r) = t;`, `let u = &t;`). A tuple returned
  from a function declared `-> (u8, &mut i32)`, or stored in a struct field or a collection,
  holds a copy, so writes through its `.1` don't reach `x`.
- **`v.try_into()` from a slice or `Vec` to an array checks the length only where the code
  around it writes one**: a `let` annotation (`let a: [u8; 4] = ..` / `Result<[u8; 4], _>`),
  a parameter of one of the file's functions, the function's return type, or
  `u32::from_le_bytes(..)` and the like, through `?`, `.unwrap()`, `.expect(..)` and `.ok()`,
  with the length a literal or a `const` of the file. Elsewhere (a struct field's value, an
  assignment to an existing local) array lengths aren't in the types, and the conversion is
  `Ok` of a copy whatever the length. `<[T; N]>::try_from(v)` always checks.
- **Seeded random numbers are rand 0.8's, except where a `u64` is involved.** `StdRng`
  (ChaCha12) and `SmallRng` (Xoshiro256++) seeded with `seed_from_u64` or `from_seed` give
  rand's sequence through `gen`, `gen_range`, `gen_bool`, `gen_ratio`, `fill`, `shuffle`,
  `choose`, `choose_multiple` and an iterator's `choose`. But a 64-bit value is a JS number,
  so `gen::<u64>()` / `next_u64()` are rand's values rounded past 2^53, and a seed above
  2^53 (`seed_from_u64(u64::MAX)`) isn't rand's seed. The `rand_distr` distributions and
  `Uniform` sample their own way (so differ from rand's for a seed), as does `iter.choose`
  after an adapter whose `size_hint` isn't followed (`chain`, `flat_map`, `skip` of
  `chars()`: rand's result depends on the hint). `thread_rng()` and `from_entropy()` are
  unseeded and use a faster generator.

## Bundled crates

- **The bundled crates are `rand`, `itertools`, `anyhow`, `thiserror`, `bitflags`, `once_cell`,
  `lazy_static`, `either`, `cfg_if`, `hashbrown`, `serde`, `serde_json`, `regex`, `rayon`, `log`
  (with `env_logger` / `simple_logger`), `num` (`num-traits`, `num-integer`), `tokio`,
  `futures`, `rand_distr`, `chrono`, `clap`, `rustc_hash`, `smallvec`, `indexmap`, `ordered_float`
  and `petgraph`**; any other crate is an error. Their smaller gaps are under
  [Bundled crates' details](#bundled-crates-details).

## JS globals (the `js` crate)

- **DOM values are handles, not owned data.** `Element`, `Document`, `Storage` and friends are
  the live JS objects: `clone()` copies the handle, not the node, and changes through any copy
  show everywhere (a `let mut` is still needed to assign a field, as in Rust). There's no
  `web_sys`-style `Result` for a JS exception: a bad selector or a failed `fetch` throws.
- **`JsValue` is wasm-bindgen's, with two extensions.** `js::global(..)`, `Response::json()` and
  untyped NS results are `JsValue`s, read with `as_string()` / `as_f64()` / `as_bool()` or
  `serde_wasm_bindgen::from_value::<T>`; a `JsValue` parameter (`console::log`) takes any value
  without `JsValue::from`; and `{}` prints a `JsValue` as JS's `String(v)` (wasm-bindgen has only
  `{:?}`). `serde_wasm_bindgen::to_value` makes maps plain objects, not JS `Map`s.
- **`JsValue` doesn't solve inference.** `let v: JsValue = JSON::parse(s).unwrap()` asks for
  a type; write `JSON::parse::<JsValue>(s)`.
- **`JSON` goes by the static type.** It writes and reads what serde_json would for the type in
  the source, so a value behind a type parameter in a generic function is written as JS sees it
  (`JSON::stringify(&x)` with `x: T` gives JS's `JSON.stringify`). The serde derives aren't
  needed, and only the `#[serde(..)]` attributes listed in FEATURES apply (`with` /
  `serialize_with` / `deserialize_with` and custom `Serialize` impls are compile errors;
  `deny_unknown_fields`, `alias`, `borrow`, `bound`, `transparent`, `from` / `into` and
  `skip_serializing_if` are ignored). A flattened field's errors are at the end of its object,
  as serde's; a flattened externally tagged enum reads only when no other key is left for it.
- **Timer ids are `f64`.** `set_timeout` returns what JS does (a number in the browser), only
  good for `clear_timeout`. Closures given to timers and event listeners stay alive as long as JS
  holds them; there's no `Closure::forget`.
- **`Promise::all` / `race` take a `Vec` of futures** (unawaited async calls). They start when
  called, as JS promises do, not when polled.
- **`Date` is JS's.** Local time is the browser's time zone, and the string forms (`{}`,
  `to_locale_string`) are the browser's.

# 2. Not planned

## Threads and concurrency

There is one JS thread, and scripts share it with the game.

- **`std::sync` is single-threaded.** Locks never block, fail or poison (`try_lock` is always
  `Ok`, a second `lock()` while the first guard lives is no deadlock), atomics ignore their
  `Ordering`, and a std `mpsc` `recv()` on an empty channel is `Err(RecvError)` instead of
  waiting. `LockResult<T>` / `TryLockResult<T>` aren't names here (write the `Result`).
- **rayon runs on one thread.** Results are what one thread gives, so they match rayon's except
  where rayon's own order isn't fixed (`par_bridge`, `for_each` side effects). There's no
  `ThreadPoolBuilder`, `rayon::scope` or `par_iter().update(..)`, and rayon's method names
  inside other macros' arguments are rewritten only for the plain ones (`par_iter`, ..), not
  `reduce(|| id, op)`.
- **tokio runs on the script's event loop.** `join!` awaits its futures in turn after starting
  them all (they run together, as in tokio); a spawned task starts when `spawn` is called; a
  panicking task ends the script instead of giving `Err(JoinError)`. `select!` polls its branches
  in order (tokio picks at random) and can't cancel a losing `async fn` (a JS promise runs on;
  a channel's `recv()` is cancel-safe). Locks never wait, so a tokio `Mutex` held across an
  `.await` doesn't keep other tasks out. No `interval`, `oneshot`, `broadcast`, `watch` or
  `Notify`, and `time::timeout` with `use tokio::time;` must be spelled `tokio::time::timeout`.
- **Async fns and blocks are JS promises**, which start when called and can't be polled to
  completion synchronously: `poll` on one is `Pending` until it settles (then the waker is
  woken), so a hand-written `block_on` loop over an async fn never ends; drive those with
  `.await` (an `async fn main`). Awaiting a hand-written future polls it from the event loop:
  after `Pending` it waits for its waker (a future that never wakes it never finishes, as in
  Rust). A hand-written future's class can be awaited because it has a `then` method, so an
  async fn returning one (unawaited) resolves to its output. `std::task::Wake`
  (`Waker::from(Arc<W>)`) and `RawWaker` aren't there.

## Raw pointers

- **Pointers have no addresses.** A raw pointer is the object it points at (or a cell for a
  number, string, bool or Option), so there's no pointer arithmetic, no casts between pointers
  and integers, and `transmute` only reinterprets same-size numbers, `char` and `bool`. All of
  these are errors.
- **Dangling pointers don't dangle.** A pointer outlives what it points at: reading through it
  after the value is dropped (or after `Box::from_raw` freed it) still sees the last value
  instead of being undefined behavior. A null pointer read throws a JS `TypeError`, not a
  segfault.
- **`*const` writes aren't rejected.** Writing through a `*const T` is accepted.
- **Pointers to numbers stay in locals and parameters.** Such a pointer in a struct field or as a
  function's return value is an error (a pointer to a struct can go anywhere).
- **`{:p}` prints a made-up address** (`0x10000` and up): the same for one object each time,
  but a number or string gets a new one each time it's formatted, and two references to one
  number variable don't print the same address. A function pointer's `{:?}` (a derived `Debug`
  with an `fn(..)` field) is one of these too.
- **`panic::catch_unwind` gives `Err` of the panic's message**, a string, where rustc's payload is
  a `Box<dyn Any>` holding a `&str` or `String` (`downcast_ref::<&str>()` finds it either way
  here). The panic is reported to stderr first, as rustc's hook does, without the
  `RUST_BACKTRACE` note. `String::capacity` is the string's length in bytes.
- **`c"..."` is a `CStr` value, and `CString` an owned one.** There's no C to pass them to:
  `as_ptr()` isn't there.

## Garbage-collected memory

- **`Rc` is erased.** `Rc::strong_count` / `weak_count` aren't available, `.clone()` on a struct
  without `#[derive(Clone)]` is taken to be an `Rc` clone (it shares the value), and an `Rc<T>`
  holding a `Drop` value drops it with the first `Rc`, not the last.
- **A `Weak` never expires.** Nothing is freed while reachable, so `upgrade()` returns `Some`
  even after every `Rc` is dropped.

## No OS

- **`std::fs` and `process::Command` always fail**: there's no filesystem or process to reach.
  Reading (`File::open`, `fs::read_to_string`, `fs::read`, `fs::metadata`, `fs::read_dir`) gives
  `ErrorKind::NotFound`, anything else (`File::create`, `fs::write`, `create_dir`, `remove_*`,
  `copy`, `rename`, `Command::output` / `status` / `spawn`) `Unsupported`, so programs that handle
  the error run; their messages say so (Rust's are the OS's). Use `ns.read` / `ns.write`.
- **`stdin()` is always at its end**, as a script has no input stream: read input from
  `ns.args`, a file or `ns.prompt` instead.
- **`std::process::exit` ends a script by unwinding**: code in `Drop` impls still runs (Rust's
  doesn't). A Netscript `main(ns)` whose program can exit ends quietly, showing what clap's
  `Error::exit` prints (help, errors) with `ns.tprint`; other output of `print!` goes to the
  browser console, as always.
- **`Path` and `PathBuf` are one type**, a path's text, so a `&Path` and a `PathBuf` convert
  freely, and paths are Unix-style on every host (`join` uses `/`; native Windows uses `\`).
  `exists()`, `is_file()` and `is_dir()` are always `false`. Comparing and hashing paths goes by
  their text, not their components (`a//b` != `a/b`).
- **`OsStr` / `OsString` are `String`**, so string methods work on them too and `into_string()`
  never fails. `OsString::push` of a string or a path works on a binding written or made as an
  `OsString` (`let s: OsString`, `OsString::from(..)`, `x.to_os_string()`, an `OsStr`'s
  `to_owned()`, a parameter); on a
  field or an element, `push` is `String`'s, which takes a `char`.
- **`env!` is an error and `option_env!` `None`**, except for Cargo's own variables, which get
  placeholders (`CARGO_PKG_NAME` `"script"`, `CARGO_PKG_VERSION` `"0.1.0"`, `CARGO_MANIFEST_DIR`
  `"."`, the rest empty): a script isn't built by cargo.
- **`cfg_if!` and `cfg` decide for a script**: features and every OS are off, so code that picks
  `#[cfg(windows)]` natively takes another branch here. The target is JS, described as rustc's
  `wasm32-unknown-unknown` but with `target_pointer_width = "64"` (`usize` is 64 bits here).
  `#[cfg(feature = "..")]` items are kept (a crate's code may have the feature on), unless two
  alternatives of one name would both stay: then the one on with every feature off is kept, or
  the first written when none is (so a file that rustc builds only with some feature compiles
  as with the first such feature set). `#[cfg]`s inside macro arguments (`vec![#[cfg(x)] 1]`)
  aren't evaluated. `compile_error!` under a target `#[cfg]` fires as it would on wasm:
  `#[cfg(not(any(unix, windows)))] compile_error!(..)` is an error here.
- **`Instant` has the browser's resolution**: `performance.now()` is microseconds or coarser
  (browsers round it), and it counts from page load, so `{:?}` of an `Instant` isn't rustc's.
  `SystemTime::now()` is `Date.now()`, whole milliseconds; its `{:?}` is Linux's form.

## Types erased at run time

- **`dyn Any` tests values at run time**: a struct or enum by its class (not its type
  arguments), numbers by their JS type and the target's range (so a boxed `5u8` is also an
  `i32`, a `u64` and an `f64`; `i128` / `u128` are BigInts and tell apart), a `String` and a
  `&str` alike (also a one-char string and a `char`). `Option` / `Result` / closures can't be
  told apart (an error). `type_id()` of a `dyn Any` isn't known (use `is::<T>()`); `TypeId`'s
  `{:?}` is the type's name, not a hash, and `String` / `&str` share one. `type_name` puts the
  file's name as the crate (`my_script::Point`), ignoring the `mod` a type is in.
- **`PhantomData`'s `{:?}` is `PhantomData<_>`**: Rust names the type (`PhantomData<i32>`), whose
  name isn't known at run time.
- **Strings are JS's UTF-16 strings.** They compare by UTF-16 code units, not UTF-8 bytes, in
  `cmp`, `sort` and `min` / `max`: the order differs from Rust's only between characters above
  U+FFFF and those in U+E000–U+FFFF. They're immutable, so `Cow::to_mut()` can't change a
  string in place: use it on slices, or `into_owned()` and change the `String`.
  `Cow::from(String)` is borrowed (a `String` and a `&str` are one type here), and `is_borrowed`
  / `is_owned` (unstable in Rust) aren't there.

# 3. Minor differences

## Type checking

- **The borrow checker is rustc's NLL over an overlay.** Conflicts are errors, as in rustc
  (lifetimes are erased in the emitted JS, so `setStrictBorrows(false)` can make them warnings
  and let the program run). The checker (`nll.rs`) rebuilds references from annotations,
  signatures, struct fields and a table of std's borrowing methods, and reports E0499, E0502,
  E0503, E0505, E0506, E0597, E0515, E0716 and "lifetime may not live long enough" with rustc's
  wording: through `let`s, struct literals, this file's functions and methods, `?`, `match` and
  `if let`, loops, closures and std's iterators and adapters. It errs toward silence where it
  can't see: functions of other modules and of bundled crates return owned values as far as it
  knows; a generic `T` holding a reference is owned; a closure's return type is inferred only
  when std hands it an item (`iter().map(..)`, `opt.map(..)`); use after move is `moves.rs`'s
  (see below). Such programs rustc rejects still run.
- **Use after move is checked only where the checker is sure a local owns its value** (see
  FEATURES). It misses moves out of fields by expression (`let v = s.items;` then `s.items`),
  `match` / `if let` bindings that move, a `String` field bound by a `let` pattern (`let S {
  name, .. } = s;` leaves `s` usable: a `String` field looks like a `&str` one here), and moves
  into closures without `move`; those run with the JS value still shared.
  Definite initialization is checked for `let x;` (use before assignment, assigning an immutable
  one twice); names declared by a `let (a, b);` pattern aren't checked yet.
- **Type checking covers the supported subset only.** Within it, mismatches and mutability
  errors are reported like rustc's. NS calls are checked against the typed bindings.
- **An owned value for a `&T` parameter is reported only where ownership is certain.** `f(t)`
  with `fn f(s: &str)` and `t: String` is rustc's "expected `&str`, found `String`" when `t` is
  a by-value parameter, a `let` of a made value (`String::from(..)`, `x.clone()`, `format!`,
  a struct literal, a function declared to return an owned type) or an NS result (nothing from
  the bindings holds a reference). A `v[i]`, a field, a `match` binding or an `if` result may
  be a reference as far as the checker can tell (`Vec<&str>` and `Vec<String>` are one type
  here), so passing one bare is accepted and runs with the JS value shared.
- **"Type annotations needed" is reported for `let` bindings only.** `let v = Vec::new();` that
  nothing types is rustc's error (also `HashMap<_, _>`, `Option<_>`, `Stack<_>`), but a value
  that's never bound (`Vec::new().len()`) or an unused closure parameter is treated as untyped.
  The check runs only when nothing else is wrong, and skips struct arguments no field's type
  uses (const parameters, iterator-bounded ones).
- **`sum()` / `product()` without a type use the element type** where rustc asks for an
  annotation.
- **Closure parameters are lenient about references.** `|x| ..`, `|&x| ..` and `|&&x| ..` all bind
  the element itself; numbers and strings are values in JS either way. Rust's own type check
  still applies to what you write, but CarciniScript accepts some forms rustc would reject.
- **Exhaustiveness is approximate.** Catch-all arms, `true`/`false`, `None`/`Some(_)` and integer
  or char ranges are understood, but checking gives up on very large matches (it assumes
  they're covered).
- **`RefCell` doesn't track borrows**, so a `borrow_mut` while another borrow is alive doesn't
  panic.

## Vec, arrays and slices

- **Fixed-size arrays are growable.** `[T; N]` and `&[T]` are the same JS array as `Vec<T>`, so
  the length isn't fixed by the type. They copy like rustc's (`Copy` elements) when read from a
  local, parameter, field or row into a new place; a copy made through a type parameter or a
  closure capture isn't tracked. `[T; N]::map` and slice `strip_prefix` also work on a `Vec`.
- **An array's length is known only where it's written.** `let [a, b] = arr;` is accepted when
  `arr`'s length shows in the `let` (an annotation, an array literal, `<[T; N]>::try_from(..)`);
  an array from a function call needs the annotation (`let [a, b]: [f64; 2] = f();`). The same
  goes for `Default` of `[T; N]` (a `#[derive(Default)]` field or a `let` annotation).
- **A range of a Vec is a copy, unless borrowed `&mut`.** `v[1..4].sort()` and the other in-place
  slice methods copy the range, change it and write it back; `&mut v[a..b]` is a view (a JS
  `Proxy`, slower than the Vec itself) whose writes reach `v`, as are other writes through a range
  (`v[0..2][1] = 9`, `v[1..].iter_mut()`) and `split_at_mut`.
- **Shared slices are copies**, except as the argument of one of the file's own functions whose
  `&[T]` parameter only reads a little of it (its length, elements outside loops, sub-slices for
  such calls, iteration): that call gets a view, so recursion over `&v[1..]` is linear. `&v[1..3]`
  reads like Rust either way; `&mut v[a..b]` is always a view, and `&mut s[a..b]` of a string is
  rejected.
- **`for x in &mut v` needs a stable name for `v`.** The loop variable aliases `v[i]`. When `v` is
  a more complex expression, it's evaluated once into a temporary first.
- **`windows` / `chunks` yield copies.** Rust yields borrowed slices; reading them is identical.

## Collections

- **Tuples are JS arrays, copied at value boundaries.** Reading a tuple out of a local, field or
  element into a `let`, an assignment, an argument, a return value or a tuple copies it, so later
  mutation doesn't alias. A tuple stored inside a struct-like JS object from an NS call isn't
  tracked.
- **`HashSet` and `HashMap` iterate in insertion order.** Rust's order is unspecified (and
  randomized), so code that depended on CarciniScript's order would already be wrong in Rust.
- **`BTreeMap` and `BTreeSet` are the hash classes plus a sorted array** of their entries: a
  single insert or remove is a binary search plus a memmove (O(n), but a fast one), `pop_first`
  / `first` / `last` / `range` are O(1) or O(log n) to find; a bulk load (more than 512 inserts
  between ordered reads) is appended and sorted at the next ordered read. `VecDeque` is a ring
  buffer (`κσ.Deque`): `pop_front` / `push_front` are O(1); `q[i]` goes through its index method,
  and the `Vec` methods it shares run on its contents made contiguous (`make_contiguous()` is that
  array).
- **`VecDeque::insert` past the end appends instead of panicking.**
- **`HashMap::with_hasher(..)` ignores the hasher**: keys compare by value.
- **A hand-written `Hash` sets map keys only with a hand-written `PartialEq`.** Keys are
  structural (every field) for a derived `PartialEq`, and what `hash` writes for a hand-written
  one, so the two should use the same fields (as Rust requires anyway). `DefaultHasher::finish()`
  is FNV-1a, not SipHash: its values differ from rustc's (which aren't specified).
- **`x.hash(&mut your_hasher)` writes 8 bytes for every integer** (a `usize` / `u64`'s; Rust
  writes 4 for an `i32`), and a struct's derived `Hash` writes its runtime key (a string), so the
  `finish()` values match Rust's for `usize` / `u64` / `String` / `&str` keys and their `write_*`
  calls only.

## Iterators and closures

- **`rev()` collects first.** JS iterators only go forward, so `.rev()` drains what's before it.
  The output is the same unless the earlier adapters have side effects (`inspect`) or the
  iterator is endless. `next_back`, `nth_back`, `rfind`, `rfold` and `len` collect what's left
  the same way (so a `next()` / `next_back()` loop is quadratic), and `len()` works on any
  iterator, not only `ExactSizeIterator`s.
- **`peek` works on any iterator**, not only one from `peekable()`: the lookahead is added on the
  first `peek` / `next_if`. Cloning an iterator collects what's left (so the two go on
  independently, as in Rust).
- **`{:?}` of any shared iterator is `slice::Iter`'s** (`Iter([2, 3])`, what's left): Rust
  writes each iterator its own way (`IntoIter([..])`, `Chars(['a'])`, `Map { iter: Iter([..]) }`).
  Like cloning, it collects what's left, so an endless iterator never finishes printing.
- **`size_hint()` on std's iterators is exact** (what's left is counted, through a copy): Rust
  gives `(0, Some(n))` after a `filter`. A custom iterator's comes from its own `size_hint`, or
  `(0, None)`; its `len()` (with `impl ExactSizeIterator`) is that hint's lower bound, where Rust
  also asserts the bounds agree.
- **A closure passed where `&mut T` is expected can only write back through collections.** Map
  slots (`retain`, `and_modify`) and `iter_mut().for_each` alias the real element, and a named
  `fn bump(x: &mut i32)` given to `iter_mut().for_each` / `values_mut().for_each` gets a cell
  over each element; one given to another adapter (`iter_mut().map(bump)`) gets a copy. (Your
  own `F: FnMut(&mut String)` / `Fn(&mut i32)` closures, a `|v: &mut i32|` closure and a
  function item called through a variable take a cell, so their writes reach the caller's value.)
- **`iter_mut()` over numbers, strings or bools outside a loop yields slots** (objects whose
  `[1]` is the element), followed through `next()`, adapters (with closures too: `filter`, `map`,
  `find`, `try_for_each`, `try_fold`, `find_map`, ..), `collect()` into a `Vec<&mut T>`, and
  functions, parameters and fields that hand on a `&mut` iterator (also one whose items are tuples
  holding a `&mut`: `-> impl Iterator<Item = (usize, &mut u8)>`, boxed or from a method); so is a
  map's `values_mut()` / `iter_mut()`. A slot passed on to a function that takes `&mut i32` is
  a copy of the element. `{:?}` of a collected `Vec<&mut T>` / `Vec<(usize, &mut T)>`, one of
  its items or a loop variable over it prints the elements (also as `{refs:?}`); `{:?}` of a
  slot anywhere else (`it.next()`) prints the slot object. A generic wrapper's `values_mut()`
  (`fn vm(&mut self) -> ValuesMut<'_, K, V>`) doesn't make slots for a caller whose `V` is a
  number: `*v += 1` through it throws a JS `TypeError`.
- **`unwrap_or(default)` evaluates `default` lazily** (it lowers to `??`). Rust evaluates it
  eagerly, so the difference shows only if the default expression has side effects.
- **A range made by `a..b` is an iterator until something asks for a `Range`** (a `let r =
  a..b`, a `Range<T>` field, parameter or return type, or with your `impl Tr for Range<T>`, a
  call of its method, `&(a..b)` or an array element), so `(0..5).start` isn't there.

## Structs, enums, Option and references

- **Values of a struct or enum are JS objects shared by reference.** Moves make that invisible
  (using a moved local is rejected as in rustc), and `Copy` types are copied wherever Rust would
  copy them. A non-`Copy` value reached through two paths at once (which Rust's borrow checker
  would reject) would show the aliasing.
- **A field named like one of its type's methods is stored as `name$f`** in JS (`fn name(&self)`
  next to a `name` field), so the instance property doesn't hide the method. This only shows when
  JS code reads the object directly.
- **`None` is `null`, and `Some(x)` is `x`**, except where `x` is itself "nothing":
  `Some(None)` is a marker object (one per depth: `Some(Some(None))` is the next), and
  `Some(())` is `true`. So `Option<Option<T>>` and `Option<()>` work everywhere, but a value
  handed to JS code as an `Option<Option<T>>` shows the marker, and serde_json writes `Some(None)`
  as `null` (as serde does) and reads `null` as `None`. `undefined` from JS reads as `None`.
- **A `&mut` to a number, string, bool or `Option` inside a struct, tuple or Vec is a slot object
  over that place** (`κσ.$placeSlot`: `[1]` reads and writes it) wherever the `&mut` is kept: in
  a local (`let n = &mut self.count;`, `let mut cur = &mut self.head;` re-pointed with `cur =
  &mut node.next;` walking a list), returned from your `fn at(&mut self, i: usize) -> &mut i32`
  / `-> Option<&mut i32>`, passed as an `Option<&mut i32>` (`bump(self.timer.as_mut())`,
  `Some(&mut x)`), held in a field (`struct W<'a> { n: &'a mut i32 }`), picked by an `if`,
  destructured from a tuple (`let (a, b) = (&mut t.0, &mut t.1);`), or mapped
  (`self.timer.as_mut().map(|n| *n += 1)`, `.filter(..)`, `.is_some_and(..)`). As an argument to a
  `&mut i32` parameter it's a cell over the place, or a temp written back after the call when the
  callee can't keep the borrow. A `Vec<&mut i32>` (or `Vec<(usize, &mut i32)>`) written as the
  type of a `let` holds slots too: `refs.push(&mut a)`, `vec![(0, &mut c)]`, `Slot::Live(&mut a)`
  for a variant declared `Live(&'a mut i32)`, and `v.iter_mut().partition(..)` into two such
  Vecs. Your generic `-> &mut T` / `-> Option<&mut T>` (`fn first_mut<T>(v: &mut [T])`, an
  `impl<T>` method) returns a slot whatever `T` is, which the caller reads the value out of when
  `T` is a struct or collection; where it has no place to point at (`m.get_mut(k)` of a map) it
  returns a slot holding the value, so writes to a number there are still a copy. Trait methods
  (`deref_mut`, `index_mut`) aren't made slots this way.
- **`*r = value` for a `&mut` struct (or `Vec`, map, set or other collection) replaces the
  object's contents in place** (`κσ.$replace`), since JS can't rebind the caller's variable. The
  result is the same as Rust's.
- **`NonZeroU32` (and the other widths) is a `u32`**: arithmetic on it type-checks and nothing
  stops a 0 that isn't made through `new`. `NonZeroU32::MIN` is `u32::MIN`.

## Generics

- **Generic functions are erased, not monomorphized.** A `T` allows only operations from its
  bounds. Operators on a `T` (`+ - * / % & | ^ << >>`, `+=` etc., unary `-` and `!`) go by the
  class the caller passes for `T`, which knows its number kind (integer division truncates,
  results wrap, shift amounts are masked to the width) or calls a struct's own operator; so do
  `parse::<T>()`, `T::from(x)` and `sum::<T>()`. In a struct's or enum's `impl<T> S<T>`, methods
  with `self` read `T`'s class off the value (`this.$T`), which each construction stores when
  some method needs it (also for a derived `Default`'s bare `T` field). `impl<T> S<Vec<T>>`
  (or `S<Option<T>>`) reads `T`'s class off the `Vec`'s, which the value stores.
  A struct built where its `T` isn't known (an unannotated empty `Vec` field nothing settles)
  carries no class, and its `/` divides like a float. Overflow on a `T` wraps even in a debug
  build (`--debug` comparisons), where Rust panics. `T::Err` of a `T: FromStr` and `T::Error`
  of a `T: TryFrom<X>` are unchecked, and a `T: TryFrom<X>` given a type without that impl fails
  when the call runs, not at compile time.
- **Generic functions take each `Fn`-bounded type parameter as a single concrete closure type**,
  so one function can't be called with closures of different signatures.
- **`T: Into<PathBuf>` (and other non-number targets) is the target type itself**, so a closure
  for `F: FnOnce() -> P, P: Into<PathBuf>` must return a `PathBuf`, not a `&str`.
- **A generic `T: DeserializeOwned` gets its schema from `T`'s class**, so `T` can be a number,
  `String`, `bool`, `char`, a `Vec` of those or a non-generic struct or enum, not a generic
  struct.
- **Several `TryFrom`, `FromIterator` or `Extend` impls on one type whose argument types differ
  only by references** (`TryFrom<&str>` / `TryFrom<String>` / `TryFrom<&String>`, `Extend<&'a T>` /
  `Extend<T>`) take the same JS value, so a call picks by how its argument is written: a string
  literal, `s.as_str()` or `&v[..]` for `&str` / `&[T]`, `&s` for another reference,
  `s.to_string()` / `format!(..)` / `vec![..]` for a value; for items, `v.iter()` / `&v` for
  references and `vec![..]` / `.into_iter()` / `.copied()` for values. A plain variable says
  nothing: the first impl serves (never the one the call is in), which is right whenever the
  impls agree, as std's and most crates' do.
- **Array lengths aren't part of types, so const generics lean on arguments.** A const parameter
  is inferred from a written argument or a struct's type (`Matrix<2, 3>`), or read at run time off
  an argument (`[T; N]`'s length), but not from an array type alone: `let z: [f64; 4] = zeros();`
  needs `zeros::<4>()` (it's a "type annotations needed" error, not a wrong value). For the same
  reason a mismatched length (`sum::<2>([1, 2, 3])`) isn't an error. A const argument is a
  const parameter or a constant integer or `bool` expression the compiler works out: literals,
  `const` items, associated consts, arithmetic, `as` casts, `size_of::<u32>()`, calls of the
  file's `const fn`s, and a `const` struct's `const fn` methods (`{ CONFIG.encode() }`); not
  another crate's `const fn`.
- **Associated types are checked through function type parameters, not `impl<T>`'s or `dyn`.**
  In `fn f<S: Shape>`, `S::Unit` is its own type with the bounds the trait declares, as in
  rustc; through an `impl<S: Shape>`'s parameter in a method, or `dyn Shape`, it matches any
  type. An associated type a `where` clause constrains (`where S::Unit: PartialEq<T>`), or one
  of a trait with a blanket impl, is unchecked rather than given those bounds.
- **An operator bound's `Output` is the parameter itself.** With `L: Add<R>` (or a plain
  `T: Add`) and no `Output = X`, `L::Output` and `a + b` are an `L`, so `let x: L = a + b;`
  compiles where rustc wants `L::Output`. The values are right whenever the impl's output is its
  own type, as for numbers.

## Traits on built-in types

- **A trait implemented for both an integer type and a float type** (or two integer types),
  called through a generic `T: Trait`, goes by `T`'s class when the caller passes it
  (`pay(2.0f64)` reaches the `f64` impl, `pay(2u8)` the `u8` one; `κσ.$dcallT`), as a call on a
  value of known type does. Through `dyn Trait` (`Box<dyn Money>`, `&dyn Money`) the number
  carries its type from where it was coerced (`κσ.$dynNum`). Through a `T` no caller passes a
  class for (a function value), the impl is picked by the value: a whole number (`2.0f64`
  included) goes to the integer impl (`i128` / `u128` values are BigInts and always reach their
  own impl; an `i64` / `u64` is a number like any other).
- **A tuple is a JS array**, as are a slice, array and `Vec`. Through a generic `T: Trait` whose
  trait has impls on both, the impl goes by `T`'s class when the caller passes it
  (`κσ.$tupT`, `$dcallT`); through a `T` no caller passes a class for (a function value), or
  `dyn Trait`, by the value: a tuple's impl takes any array of its length. Two impls for tuples
  of one length (`(u8, u8)` and `(String, String)`) conflict.
- **An `Option` through a generic `T: Trait`** is `null` or its value at run time, so it goes
  by the class the caller passes for `T` (`Some(Some(4))` reaches the `Option` impl twice).
  Where no class comes with it (a `T` the caller can't name, a `dyn Trait`), by the value: the
  `Option` impl serves `None` and any `Some(x)` whose value's own type has no impl of that
  method, and `Some(3u8)` reaches `u8`'s impl when there is one.
- **A trait on a function pointer type** (`impl Tr for fn() -> i32`) is one class for every
  signature: `impl Tr for fn(i32) -> i32` next to it conflicts, and a closure reaches it too
  (both are JS functions).
- **A trait on an iterator type** (`impl Tr for Peekable<I>`, `Chars`, `Keys<K, V>`) is one
  class for every iterator and adapter: a second iterator type's impl of the same trait
  conflicts. `Cell<T>` shares `RefCell<T>`'s. (Each range type has its own: `Range`,
  `RangeFrom`, `RangeTo`, `RangeFull`, ..)
- **A trait for both `str` and `String`** (or `[T]` and `Vec<T>`, `Path` and `PathBuf`): one
  type in JS, so the borrowed type's impl serves both. The owned one must hand each call on to it
  (`self.as_str().f(a)`, `Tr::f(&self.as_str())`, `self[..].f()`); one that does something else
  (zeroize's `Vec` impl also truncates) is an error.
- **Arrays and slices share `Vec`'s impls**: `impl Tr for [u8; 4]` and `impl Tr for Vec<u8>` (or
  `[u16]`) would collide, and the element type isn't checked.
- **`impl Tr for &T` / `&mut T` / `Box<T>` must forward to `T`'s impl.** References and boxes are
  erased, so `&x` is `x` and its methods are `T`'s: an impl whose methods each call `T`'s
  (`(**self).f(a)`, `T::f(self, a)`, `self.as_ref().f(a)`) is left out, and one that does
  anything else (or leaves to the default a method another impl overrides) is an error.
- **`Box<dyn Error>` from a string** (`"x".into()`, `format!(..).into()`) is the string itself:
  `{}`, `{:?}` and `to_string()` match std, but `downcast_ref::<String>()` style checks see a
  plain string.
- **A trait of yours named like a std trait** (`trait Write`) is the one `impl fmt::Write for X`
  is checked against, since traits are matched by their last path segment.
- **A `&mut dyn fmt::Write` parameter is a type parameter**, so the function is generic over
  its writer; a `&mut dyn Write` struct field or closure parameter isn't supported.

## Formatting

- **`{:?}` of a generic value is guessed from the run-time value where no caller says its type.**
  A generic function or method that prints a `T` with `{:?}` takes a formatter for `T` from each
  call (so `Some(..)`, `1.0` and `'c'` print as in Rust), and so does a blanket impl's method
  called on a known type; an `impl<T: Debug>`'s method with `self` uses the class its value
  carries. Where nothing passes one (a generic function used as a value, a call through `dyn`),
  `Some(x)` prints as `x` and a whole float without `.0`.
- **`self.0.fmt(f)` on a generic value** picks `Display` or `Debug` by the impl it's in when both
  could apply, and a whole-number float in it takes no precision (`{:.2}` of `Wrap(2.0)` prints
  `2`).
- **A width on a derived `{:?}` pads the whole value.** Rust passes the width to each field
  (`{:>8?}` of `P { x: 1 }` is `P { x:        1 }`, and of `vec![1]` is `[       1]`); here the
  result is padded as one string. Hand-written `Debug` / `Display` impls get the width through
  the Formatter, as in Rust.
- **`{:>10}` on a type with a `Display` impl ignores the width**, which matches Rust for impls
  that use `write!` (rather than `f.pad`).
- **`{:x?}` doesn't reach into structs, enums or generic values**: their `Debug` is compiled
  without it, so it's an error there (Rust prints their integer fields in hex).
- **`{:#?}` of a value holding a hand-written `Debug`** spreads that impl's own parentheses and
  braces over lines too (the pretty form is made from the compact one), where Rust prints what
  the impl wrote: a serde_json `Value` in `{:#?}` shows `Bool(` / `true,` / `)` for
  `Bool(true)`.
- **`fmt::Arguments` is a `String`**: `format_args!(..)` formats when it's made.

## Drop

- **`Drop` follows moves the move checker sees**: by `let`, assignment, argument, method that
  takes `self`, struct / tuple / `vec!` literal, `return`, `break`, `?`, a closure capturing it
  by value, and a field moved out of a struct (`let x = s.inner;`: the struct drops the rest).
  A closure given to a std method (`it.map(f)`) drops what it captured at the end of that
  statement, with the adapter. A `move` closure that reads only a field with nothing to drop
  (`tag.0`) leaves the value with its scope, as Rust 2021 captures the field. A method taking
  `self` by value drops what's left of `self` at its end, unless it takes `self` apart with a
  pattern. `v.clear()`, `truncate` and assigning a new value to a field don't drop the old
  elements. On a panic the values in scope are dropped before the panic message is printed
  (Rust prints it first).

## std::io

- **`write_vectored` writes every buffer** (as a `Vec<u8>` does in Rust; std's default, and
  Windows' stdout, write only the first).
- **`Read` / `BufRead` are the io prelude's traits.** `BufRead::fill_buf` gives a copy of the
  buffer (a `Vec<u8>`), and `lines()` / `split()` / `bytes()` read everything up front rather
  than lazily. `Seek` isn't there. `Cursor::new(Vec::new())` is a `Cursor<Vec<u8>>` (as its
  `Write` impl would say in Rust); another untyped `Vec` inside a `Cursor` needs an annotation. A
  `Cursor` over a `String` accepts writes (Rust has no such impl), which don't change the string.
- **`&[u8]` reads through a `Cursor` of its own.** Passed for an `R: Read` parameter, the bytes
  are wrapped on entry, so reading leaves the caller's slice where it was, as passing `&[u8]` by
  value does in Rust. Passing `&mut slice` (whose reads advance `slice` in Rust) is an error: use
  a `Cursor`. `get_ref()` of a `BufReader` over bytes gives that `Cursor`.

## std::time

- **Panics inside these types are located in `<std>`.** rustc reports the caller's line for
  `Duration / 0` and `Instant ± Duration` overflow (`#[track_caller]`, which user code can't use
  here either); the messages match.
- **Only programs that name `time::` get the types**, and not when they declare their own
  `Duration`, `Instant` or `SystemTime`. `2 * duration` (`u32 * Duration`) isn't available:
  write `duration * 2`.

## Bundled crates' details

- **serde_json reads the whole text before converting it**, so when a document has both a
  syntax error and, earlier, a value of the wrong type, the syntax error is reported where
  serde_json (which streams) reports the type error. A `Value` keeps object keys sorted (no
  `preserve_order`). `JsonError` has no position: serde_json says `missing field `x` at line 1
  column 8`; here it's `missing field `x``, and some syntax errors are worded differently.
  `HashMap` keys come out in insertion order (serde_json's follow its hash order, which varies
  anyway); `BTreeMap` is sorted in both. `from_slice` / `from_reader` of bytes that aren't UTF-8
  fail with "invalid unicode code point" and no position, wherever the bad byte is.
- **Integers past `u64` / `i64` are floats to serde_json**, so they're rejected with its "invalid
  type: floating point" error, and a JSON integer past 2^53 read as `u64` / `i64` rounds like any
  64-bit number here (`i128` / `u128` read them exactly).
- **regex runs on JS regular expressions.** Inline flags work only at the start of the pattern
  (`(?i)abc`, not `a(?i:b)c`), `x` (verbose) mode, class set operations (`[a&&b]`) and negated
  POSIX classes are errors, `\b` is an ASCII word boundary, and `regex::Error`'s text isn't
  regex's exact report (no caret line). `replace` and friends give a `String` where regex gives
  a `Cow<str>` (`into_owned()` and `to_string()` both work), `Match::range()` is missing, and
  there's no `RegexBuilder`, `RegexSet` or `regex::bytes`.
- **rand's distributions:** the built-in ones have `sample` / `sample_iter` methods but don't
  implement the `Distribution` trait, so a `D: Distribution<f64>` bound takes your own types only;
  your `impl Distribution<T> for Standard` can be for one `T`. `Uniform::from(lo..hi)`,
  `WeightedIndex::update_weights`, `Slice`, `Dirichlet`, `Zipf`, `Hypergeometric`, the unit
  shapes (`UnitCircle`, ..) and `DistIter` as a named type aren't there, and `rng.sample_iter(..)`
  doesn't move `rng` (rustc rejects using it afterwards).
- **itertools' `ExactlyOneError` is generic over the items, not the iterator**
  (`ExactlyOneError<i32>`, where itertools has `ExactlyOneError<vec::IntoIter<i32>>`), and its
  `{:?}` shows the rest of the items as a slice (`inner: [3, 4]`) where itertools shows the
  iterator's own `Debug`. `izip!` / `iproduct!` take up to six iterables. A bundled enum's
  variants come in by a `use` of their own (`use itertools::FoldWhile::{Continue, Done};`), not
  from inside a list of other names (`use itertools::{Itertools, FoldWhile::Done};` is an
  error).
- **`tuple_windows()` / `collect_tuple()` sizes are read from the code nearby** (a turbofish,
  `|(a, b)|`, `for (a, b, c) in`, `let (a, b) = ..`); anywhere else they are pairs.
- **A lazy value is computed when it's created**, so a `LazyLock` / `Lazy` / `lazy_static!`
  static is initialized when the script loads, not on first use (only the order of side effects
  in the initializer differs). `OnceLock` / `OnceCell` have no `take` / `into_inner`.
- **`ensure!(a == b)` without a message** reports `Condition failed: `a == b``; anyhow 1.0.104
  also appends the two values (`(1 vs 0)`).
- **`num` covers the integer and float types up to 64 bits** (not `i128` / `u128`, and no
  `BigInt`, `Rational` or `Complex`).
- **`ordered-float`:** `{}` of an `OrderedFloat<f32>` / `NotNan<f32>` prints the `f64` digits
  (`0.10000000149011612`; `{:?}` is right): the generic `Display` can't tell an `f32` in JS.
  Float methods go through `Deref` (`x.sqrt()` is an `f64`, as without `use num_traits::Float`);
  `Sum` / `Product`, `NotNan` with plain floats (`n + 1.0`), comparisons with plain floats
  (`OrderedFloat(1.0) == 1.0`), `NotNan::try_from` and serde aren't there.
- **`indexmap`:** a number index is a position and a reference a key, by how it's written (the
  types don't keep references), so `map[k]` with `k: &u32` indexes by position here.
- **`log` writes `[LEVEL] message`** to stderr, without env_logger's timestamp and target, and
  there's no `RUST_LOG`: `env_logger::init()` logs errors only; set a level with
  `Builder::filter_level` or `Env::default().default_filter_or("info")`.
- **chrono:** `Local` is the JS host's time zone (Bitburner: the browser's). chrono-tz's zones
  (`chrono_tz::Europe::London`, `"America/New_York".parse::<Tz>()`) use the host's `Intl` data:
  offsets are the browser's tz database, and abbreviations come from `Intl`'s names (tried in a
  few English locales and Japanese, else the tz database's `+04` form), which can differ from
  chrono-tz's table for zones `Intl` names only by offset. `format(..)` gives a `String` (not a
  `DelayedFormat`); leap seconds, `ParseResult`, `Date<Tz>` (deprecated), `round_subsecs` /
  `DurationRound` and serde for its types aren't there; `parse_from_str` takes the common
  specifiers (`%Y %m %d %H %M %S %y %b %B %a %A %j %I %p %f %.f %z %:z %s %T %D %F %R`, ..).
- **clap:** `#[command(version)]` with no value is `0.1.0` and a command's name defaults to the
  program's: a script has no Cargo.toml to read them from (`version = "1.2"` sets one). Help isn't
  wrapped to the terminal and has no colors; `ArgGroup`, `conflicts_with` / `requires`, `env`,
  `global`, `num_args`, `arg!` / `command!`, `#[arg(value_parser = ..)]` ranges and nested
  subcommand variants aren't there. `get_one::<T>` parses the value it's asked for (clap checks
  the type it was given).
- **`petgraph`:** `Graph`'s index type is always `u32` (no `Graph<N, E, Ty, Ix>` / `NodeIndex<Ix>`),
  and there's no `StableGraph`, `MatrixGraph`, `Csr`, `Dot`, `Graph`'s `{:?}`, `map` /
  `filter_map` / `retain_*`, `Topo`, `Reversed`, `Walker::iter` or the other `algo` functions
  (`all_simple_paths`, `floyd_warshall`, `condensation`, ..). `neighbors(..)` and the other
  iterators are collected when called (no `detach()`). `node_weight_mut` / `edge_weight_mut` /
  `GraphMap::edge_weight_mut` write through for struct weights only; for numbers and strings use
  `g[n] = ..` / `g[n] += ..`. `from_edges` / `extend_with_edges` take pairs (`E::default()`
  weights) only when the list is written out (`&[(0, 1), ..]`, `vec![..]`); anything else is
  read as `(a, b, w)` triples. A `GraphMap` algorithm's cost closure gets the edge as a tuple
  struct (`e.weight()`, `*e.2`), so `|(a, b, w)|` doesn't destructure it. `DfsSpace` is accepted
  and unused. Walkers' `next(&g)` is recognized by its argument (an iterator's `next` has none),
  so your own `fn next(&mut self, x)` in a file that uses petgraph is rewritten too.

## Netscript without bindings

- **Without bindings** (`setNsBindings` not called), `NS` is entirely untyped and snake_case names
  map to camelCase.

## Macros and proc macros

- **`macro_rules!` hygiene covers local bindings.** Names a macro's body binds (`let`, `for`,
  closure parameters) are renamed per expansion, so they don't clash with the caller's; other
  names (functions, types, a `static` the body names) resolve at the call site, as in a textual
  expansion. Rule markers aren't renamed: `@closure` in `m!(@closure ..)` stays put even when
  the expansion also binds `let closure`. A struct literal's shorthand field naming such a local
  (`$name { ids }`) keeps the field's name: `ids: ids__h1`.
- **Fragments are parsed like rustc's**: `expr`, `ty`, `pat`, `pat_param`, `path`, `stmt`,
  `item` and `meta` captures go through rust-analyzer's parser (the prefix parse rustc's matcher
  does), so `$($n:expr)*` takes `0 1 2` as three and `2 * 4 -1` as one, and a qualified path's
  commas (`<R<(), ()> as Clone>::clone(&x)`) are the expression's. Where that parser rejects the
  tokens (it doesn't take `&0 as &dyn Tr<A = B> != &0`, which rustc does), the capture falls back
  to a token scan (an `expr` ends at a top-level `,`, `;` or `=>`). A `tt` takes joined
  punctuation as one token (`=>`, `::`, `..=`), as rustc's lexer does.
- **Captures stay one piece of syntax when written out**: an `expr` is parenthesized (a block
  isn't, so `$body $i += 1;` works), a `ty` before `<` is too (`x as $t << 4`), and tokens of
  separate repetitions are kept apart (`"x" as`, not the suffixed literal `"x"as`; `ident #y`,
  not the reserved prefix `ident#`). In expression position an expansion's trailing `;` is
  dropped, as rustc does under `#[allow(semicolon_in_expressions_from_macros)]` (a hard
  `deny` by default there).
- **Recursion limit**: expansions go 128 levels deep, as rustc's do, or as deep as a
  `#![recursion_limit = "N"]` in the file or its crate root says (the root is found through the
  `mod` items that declare the file, so a test file's `mod common;` sibling roots in `tests/`,
  which no `mod` path leads to, aren't seen). Past 1024 levels it's an error either way: each
  level re-reads the file. Deep expansions make deeply nested expressions; the compiler's wasm
  stack is 8 MB, which takes an accumulator like `(($acc) * 2 % 9 + 1)` some 600 levels deep
  before the JS engine's own stack runs out (a `RangeError`).
- **`#[cfg]`-gated alternative macros**: of several `macro_rules!` definitions of one name, an
  invocation uses the last one before it whose `#[cfg]` holds (features off, `test` off). A `cfg`
  the compiler can't evaluate (a custom `--cfg` name, a platform predicate) counts as off; when
  every definition is off, the last one before the invocation is used.
- **`log::debug!` is the `log` crate's**, also when the crate has its own `macro_rules! debug`
  (aho-corasick's `debug!` is `log!(log::debug!(..))` under `#[cfg(feature = "logging")]`, which
  used to expand itself until the limit). The bundled `log`'s macros are left out when the crate
  defines its own of that name, so such a call is reported as an unsupported macro. Features count
  as undecided (see `cfg`), so the `#[cfg(feature = ..)]` block isn't dropped.
- **A macro named `safe`** (bitvec's `safe! { .. }`) is spelled `Safe` before parsing, as
  rust-analyzer's parser takes `safe` before an item as the `safe fn` qualifier.
- **A `TokenStream` is text.** `to_string()` gives the tokens with the source's spacing (one space
  where there was whitespace, comments dropped) but without the line breaks rustc's printer adds
  to long items, so code that splits on `\n` can differ. `quote!` output keeps its template's
  spacing too, where `proc_macro2` prints a space between most tokens (`fn f () {}`); only
  macros that inspect that text can tell. Building a stream from token trees joins them with
  spaces (none after a `Joint` punct), not rustc's exact spacing.
- **`quote!`** splices values by their type as the `quote` crate's `ToTokens` does; types
  without it (a `Vec` outside a repetition, your own structs) are errors. Calls inside
  `quote!` / `parse_quote!` aren't expanded: they're tokens a proc macro emits.
- **`syn` is a subset.** The prelude covers your own `impl Parse` types on a `ParseStream`,
  `Token![..]`, `parse_quote!`, and parsing derive input, functions, structs, enums, literals,
  identifiers, paths, types and expressions. Types and expressions are token lists, not `syn`'s
  enums: `input.parse::<Expr>()` takes tokens up to the next `,` or `;` (or `=>`) outside `<..>`,
  and a `Type` also stops at `=`, `>` and `{`, which covers fields, arguments and `key = value`
  lists but not every grammar. `peek2` / `peek3` count token trees, so a `::` ahead counts as two.
  Parse errors have `syn`'s wording where it's simple, and point at the invocation rather than
  at the offending token (no spans). The prelude's names (`TkComma` for `Token![,]`,
  `IDENT` for `peek(Ident)`) show up in a macro crate's own error messages.
- **Proc macro output is spliced as text.** It's re-parsed in place, so names it introduces can
  clash with the caller's (rustc's `Span::call_site()` behaves the same; `Span::mixed_site()`
  hygiene isn't available). An expression macro's output is parenthesized, as rustc treats it as
  one expression.
- **Helper attributes** (`#[proc_macro_derive(Name, attributes(helper))]`) stay on the item and are
  ignored.

## Editions

- **Files are parsed as the 2021 edition.** `gen` is a name (`rng.gen()`, `let gen = ..`), as in
  rand 0.8 code; 2024-only syntax that depends on the edition (`gen` blocks, which are unstable)
  doesn't parse. 2015-edition `try!(x)` works (written `r#try!`; std's is `x?`, and a file's own
  `macro_rules! try` is used when it has one); other 2015-only syntax (`dyn` as a name, `async`
  as a name, anonymous trait method parameters) doesn't parse.

## Evaluation order

- **A block-like expression (`if`, `loop`, `{ ... }`) nested inside a larger expression is hoisted**
  into statements before it. If an earlier operand in the same expression has side effects, those
  now run after the hoisted block instead of before it.
