# CarciniScript: Rust compatibility gaps

Places where CarciniScript silently behaves differently from rustc, or accepts programs rustc
would reject. The counterpart to RapydScript-NS's `PYTHON_GAPS.md`. Unsupported features (which
produce a compile error) are tracked in [FEATURES.md](FEATURES.md) instead.

## Integers

- **Overflow wraps by default.** Arithmetic follows rustc's release-mode semantics (two's-complement
  wrapping). With overflow checks on (`setOverflowChecks(true)`, or Bitburner's "Rust: panic on
  integer overflow" option) it panics like a debug build.
- **`usize` and `isize` are JS numbers**, exact up to 2^53 (lengths and indexes never get near).
  They don't wrap at 2^64, and casting a negative value to `usize` keeps the sign. `i64` and `u64`
  are exact BigInts, which makes their arithmetic several times slower than `i32`'s or `f64`'s.
- **"Type annotations needed" is reported for `let` bindings only.** `let v = Vec::new();` that
  nothing types is rustc's error (also `HashMap<_, _>`, `Option<_>`, `Stack<_>`), but a value
  that's never bound (`Vec::new().len()`) or an unused closure parameter is treated as untyped.
  The check runs only when nothing else is wrong, and skips struct arguments no field's type
  uses (const parameters, iterator-bounded ones).

## Vec, arrays and slices

- **Fixed-size arrays are growable.** `[T; N]` and `&[T]` are the same JS array as `Vec<T>`, so
  the length isn't fixed by the type. They copy like rustc's (`Copy` elements) when read from a
  local, parameter, field or row into a new place; a copy made through a type parameter or a
  closure capture isn't tracked.
- **`for x in &mut v` needs a stable name for `v`.** The loop variable aliases `v[i]`. When `v` is
  a more complex expression, it's evaluated once into a temporary first.
- **Index panics on underflowed `usize`s show a negative index.** `usize` arithmetic doesn't wrap
  (see 64-bit integers above), so `v[i - 1]` with `i == 0` panics with "the index is -1" instead
  of rustc's 18446744073709551615.
- **A range of a Vec is a copy, unless borrowed `&mut`.** `v[1..4].sort()` and the other in-place
  slice methods copy the range, change it and write it back; `&mut v[a..b]` is a view (a JS
  `Proxy`, slower than the Vec itself) whose writes reach `v`, as are other writes through a range
  (`v[0..2][1] = 9`, `v[1..].iter_mut()`) and `split_at_mut`.
- **String sort order** compares UTF-16 code units; Rust compares UTF-8 bytes. The orders differ
  only between characters above U+FFFF and those in U+E000–U+FFFF.

## Tuples and HashSet

- **Tuples are JS arrays, copied at value boundaries.** Reading a tuple out of a local, field or
  element into a `let`, an assignment, an argument, a return value or a tuple copies it, so later
  mutation doesn't alias. A tuple stored inside a struct-like JS object from an NS call isn't
  tracked.
- **`HashMap::get_mut` needs a collection or tuple value.** For number, string or bool values the
  binding would be a copy, so writes wouldn't reach the map; that's a compile error pointing to
  `*map.entry(k).or_insert(..) += ..`. `iter_mut()` / `values_mut()` loops do write through.
- **`HashSet` and `HashMap` iterate in insertion order.** Rust's order is unspecified (and randomized), so code
  that depended on CarciniScript's order would already be wrong in Rust.
- **`BTreeMap` and `BTreeSet` re-sort when read after new keys arrive** (they are the hash classes
  kept in key order), so a loop that alternates inserts and ordered reads costs O(n log n) per
  read instead of O(log n). `VecDeque::pop_front` / `push_front` shift a JS array: O(n), fast enough
  for queues of a few thousand servers.
- **`VecDeque::insert` past the end appends instead of panicking**, and `BTreeMap::range` isn't
  supported yet.

## Closures and iterators

- **`rev()` collects first.** JS iterators only go forward, so `.rev()` drains what's before it.
  The output is the same unless the earlier adapters have side effects (`inspect`) or the
  iterator is endless. `next_back`, `nth_back`, `rfind`, `rfold` and `len` collect what's left
  the same way (so a `next()` / `next_back()` loop is quadratic), and `len()` works on any
  iterator, not only `ExactSizeIterator`s.
- **`peek` works on any iterator**, not only one from `peekable()`: the lookahead is added on the
  first `peek` / `next_if`. `size_hint()` and `peek_mut()` aren't supported. Cloning an iterator collects what's left (so the two go on independently, as in Rust).
- **Closure parameters are lenient about references.** `|x| ..`, `|&x| ..` and `|&&x| ..` all bind
  the element itself; numbers and strings are values in JS either way. Rust's own type check
  still applies to what you write, but CarciniScript accepts some forms rustc would reject.
- **A closure passed where `&mut T` is expected can only write back through collections.** Map
  slots (`retain`, `and_modify`) and `iter_mut().for_each` alias the real element; a named
  function passed in their place gets a copy.
- **`iter_mut()` over numbers, strings or bools outside a loop yields slots** (objects whose
  `[1]` is the element), followed through `next()`, non-closure adapters and `collect()` into a
  `Vec<&mut T>`. A slot passed on to a function that takes `&mut i32`, or printed with `{:?}`,
  is a copy of the element or the slot object; adapters with closures (`filter`, `map`) on such
  an iterator are rejected.
- **`sum()` / `product()` without a type use the element type** where rustc asks for an
  annotation.
- **Generic functions take only `Fn`-bounded type parameters**, and each one is a single concrete
  closure type, so a function can't be called with closures of different signatures.
- **Strings compare by UTF-16 code units**, not bytes, in `cmp`, `sort` and `min`/`max`. The
  order differs from Rust's only for characters outside the Basic Multilingual Plane.
- **`windows` / `chunks` yield copies.** Rust yields borrowed slices; reading them is identical.

## Structs, enums and Result

- **Values of a struct or enum are JS objects shared by reference.** Moves make that invisible
  (using a moved local is rejected as in rustc), and `Copy` types are copied wherever Rust would
  copy them. A non-`Copy` value reached through
  two paths at once (which Rust's borrow checker would reject) would show the aliasing.
- **A field named like one of its type's methods is stored as `name$f`** in JS (`fn name(&self)`
  next to a `name` field), so the instance property doesn't hide the method. This only shows when
  JS code reads the object directly.
- **A `&mut` to a number, string, bool or `Option` inside a struct, tuple or Vec** works as an
  argument (`insert(&mut node.left, v)`: the callee gets a cell over that slot), but can't be kept
  in a local (`let cur = &mut self.root;` walking a tree, `let n = &mut self.count;`): that's a
  compile error, since writes through it would be lost. Recursion, or assigning through the place
  itself, does the same job.
- **`*r = value` for a `&mut` struct replaces the object's contents in place** (`κσ.$replace`),
  since JS can't rebind the caller's variable. The result is the same as Rust's.
- **`From` impls must be for a struct or enum declared in your code.** `impl From<MyErr> for
  String` (a std target) isn't supported; `?` converts error types only into your own types.
- **`Into<X>` / `AsRef<X>` parameters and bounds take `X` itself.** `impl Into<String>` accepts
  `&str` and `String`; a type that converts through your own `From` impl (`fn f<T:
  Into<Meters>>(x: T)` given a `Feet`) doesn't, since a generic function can't pick the `from`
  method at run time.
- **`{:>10}` on a type with a `Display` impl ignores the width**, which matches Rust for impls
  that use `write!` (rather than `f.pad`).
- **Exhaustiveness checking gives up on very large matches** (it assumes they're covered).

## Generics and slices

- **Generic functions are erased, not monomorphized.** A `T` allows only operations from its
  bounds. Operators on a `T` (`+ - * / % & | ^ << >>`, `+=` etc., unary `-` and `!`) go by the
  class the caller passes for `T`, which knows its number kind (integer division truncates,
  results wrap, shift amounts are masked to the width) or calls a struct's own operator; so do
  `parse::<T>()`, `T::from(x)` and `sum::<T>()`. In a struct's `impl<T> S<T>`, methods with
  `self` read `T`'s class off the value (`this.$T`), which each construction stores when some
  method needs it. An enum's `impl<T>` (and `impl<T> S<Vec<T>>`) can't: there `/`, `%`, `<<`,
  `>>`, `!` and `parse::<T>()` are errors, and `+`, `-`, `*` are JS arithmetic (no wrapping).
  A struct built where its `T` isn't known (an unannotated empty `Vec` field nothing settles)
  carries no class, and its `/` divides like a float. Overflow on a `T` wraps even in a debug
  build (`--debug` comparisons), where Rust panics. `T::Err` of a `T: FromStr` is unchecked.
  `impl Shl` / `impl Shr` for your own types isn't supported.
- **`{:?}` of a generic value is guessed from the run-time value where no caller says its type.**
  A generic function or method that prints a `T` with `{:?}` takes a formatter for `T` from each
  call (so `Some(..)`, `1.0` and `'c'` print as in Rust), and so does a blanket impl's method
  called on a known type. Where nothing passes one (a type parameter of an `impl<T>` in a method
  with `self`, a generic function used as a value, a call through `dyn`), `Some(x)` prints as
  `x` and a whole float without `.0`.
- **A width on a derived `{:?}` pads the whole value.** Rust passes the width to each field
  (`{:>8?}` of `P { x: 1 }` is `P { x:        1 }`, and of `vec![1]` is `[       1]`); here the
  result is padded as one string. Hand-written `Debug` / `Display` impls get the width through
  the Formatter, as in Rust.
- **Every file is its own crate root for `mod`.** `mod x;` in `util.rs` loads `x.rs` next to
  `util.rs` when there is one, since each Bitburner script is compiled on its own (and may be run
  directly); otherwise `x/mod.rs`, then `util/x.rs` like rustc. For the same reason `super::` works only inside inline `mod x { .. }` blocks, not at the top of a
  module file, and `crate::` names this file. A module file's generic `pub type` aliases
  (`pub type Grid<T> = ..`) aren't usable from the file that loads it (plain ones are).
- **Generics are erased, so a few things need help at run time.** `T::new()`, `T::CONST` and
  `T::default()` work in free functions, on a method's own type parameters and on an `impl<T>`'s
  in its associated functions, which get `T`'s class as a hidden argument, but not on an
  `impl<T>`'s in methods with `self` (the class would have to travel with the value).
- **A hand-written `Hash` sets map keys only with a hand-written `PartialEq`.** Keys are
  structural (every field) for a derived `PartialEq`, and what `hash` writes for a hand-written
  one, so the two should use the same fields (as Rust requires anyway). `DefaultHasher::finish()`
  is FNV-1a, not SipHash: its values differ from rustc's (which aren't specified).
- **`v.try_into()` from a slice to an array is always `Ok`**, since array lengths aren't in
  the types; rustc returns `Err` (or panics at `unwrap`) for the wrong length. Written as
  `<[T; N]>::try_from(v)` the length is known and checked.
- **An array's length is known only where it's written.** `let [a, b] = arr;` is accepted when
  `arr`'s length shows in the `let` (an annotation, an array literal, `<[T; N]>::try_from(..)`);
  an array from a function call needs the annotation (`let [a, b]: [f64; 2] = f();`). The same
  goes for `Default` of `[T; N]` (a `#[derive(Default)]` field or a `let` annotation).
- **One `TryFrom`, `FromIterator` and `Extend` impl per type.** A second one is an error.
- **Array lengths aren't part of types, so const generics lean on arguments.** A const parameter
  is inferred from a written argument or a struct's type (`Matrix<2, 3>`), or read at run time off
  an argument (`[T; N]`'s length), but not from an array type alone: `let z: [f64; 4] = zeros();`
  needs `zeros::<4>()` (it's a "type annotations needed" error, not a wrong value). For the same
  reason a mismatched length (`sum::<2>([1, 2, 3])`) isn't an error. A const argument can be a
  literal, integer arithmetic on literals (`{ 128 / 8 }`), a const parameter or a `const` item,
  not a `const fn` call. Const parameters on enums, a trait's const parameter read in the
  trait's default methods, and 64/128-bit ones, aren't supported.
- **Associated types are checked through function type parameters, not `impl<T>`'s or `dyn`.**
  In `fn f<S: Shape>`, `S::Unit` is its own type with the bounds the trait declares, as in
  rustc; through an `impl<S: Shape>`'s parameter in a method, or `dyn Shape`, it matches any
  type. A generic trait can be implemented once per type. An associated type a `where` clause
  constrains (`where S::Unit: PartialEq<T>`), or one of a trait with a blanket impl, is unchecked
  rather than given those bounds.
- **`&mut` to a number, string or bool inside a struct, tuple or Vec can't be passed to a
  function** (the value would be copied). Pass the struct, or a `&mut` to a local and write it back.
- **`RefCell` doesn't track borrows**, so a `borrow_mut` while another borrow is alive doesn't
  panic. `Rc` is erased: `Rc::strong_count` and `Weak` aren't available, and `.clone()` on a
  struct without `#[derive(Clone)]` is taken to be an `Rc` clone (it shares the value).
- **Shared slices are copies.** `&v[1..3]` reads like Rust; `&mut v[a..b]` is a view instead (see
  above), and `&mut s[a..b]` of a string is rejected.

## Option and patterns

- **`None` is `null`, and `Some(x)` is `x`**, except where `x` is itself "nothing":
  `Some(None)` is a marker object (one per depth: `Some(Some(None))` is the next), and
  `Some(())` is `true`. So `Option<Option<T>>` and `Option<()>` work everywhere, but a value
  handed to JS code as an `Option<Option<T>>` shows the marker, and serde_json writes `Some(None)`
  as `null` (as serde does) and reads `null` as `None`. `undefined` from JS reads as `None`.
- **A kept `hash_map::Entry` holds the map**: `btree_map::Entry`'s `get_mut` / `insert` assume a
  `HashMap`'s storage, and `let v = e.get_mut(); *v += 1;` (a binding of the `&mut`) is
  rejected as a borrow of an immutable `v`; write `*e.get_mut() += 1`.
- **A generic `T: DeserializeOwned` gets its schema from `T`'s class**, so `T` can be a number,
  `String`, `bool`, `char` or a non-generic struct or enum, not a `Vec<..>` or a generic type.
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
- **Without bindings** (`setNsBindings` not called), `NS` is entirely untyped and snake_case names
  map to camelCase.

## Bundled crates

- **The bundled crates are `rand`, `itertools`, `anyhow`, `thiserror`, `bitflags`, `once_cell`,
  `lazy_static`, `either`, `cfg_if`, `hashbrown`, `serde`, `serde_json`, `regex`, `rayon`, `log`
  (with `env_logger` / `simple_logger`), `num` (`num-traits`, `num-integer`), `tokio`,
  `futures`, `rand_distr`, `chrono` and `clap`**; any other crate is an error.
- **`log` writes `[LEVEL] message`** to stderr, without env_logger's timestamp and target, and
  there's no `RUST_LOG`: `env_logger::init()` logs errors only; set a level with
  `Builder::filter_level` or `Env::default().default_filter_or("info")`.
- **`num` covers the integer and float types up to 64 bits** (not `i128` / `u128`, and no
  `BigInt`, `Rational` or `Complex`). `Signed::abs(&x)` written as a trait path doesn't work on
  a built-in type; `x.abs()` does.
- **tokio runs on the script's event loop.** `join!` awaits its futures in turn after starting
  them all (they run together, as in tokio); a spawned task starts when `spawn` is called; there's
  no `tokio::sync` (`Mutex`, channels), `select!`, `interval` or `timeout`, and a panicking task
  ends the script instead of giving `Err(JoinError)`.
- **rayon runs on one thread.** Results are what one thread gives, so they match rayon's except
  where rayon's own order isn't fixed (`par_bridge`, `for_each` side effects). There's no
  `ThreadPoolBuilder`, `rayon::scope` or `par_iter().update(..)`, and rayon's method names
  inside other macros' arguments are rewritten only for the plain ones (`par_iter`, ..), not
  `reduce(|| id, op)`.
- **serde_json reads the whole text before converting it**, so when a document has both a
  syntax error and, earlier, a value of the wrong type, the syntax error is reported where
  serde_json (which streams) reports the type error. A `Value` keeps object keys sorted (no
  `preserve_order`), and has no `IndexMut` (`v["k"] = ..`): use `as_object_mut()` /
  `get_mut`. `PartialEq` between a `Value` and a number or string (`v["n"] == 1`) is missing;
  compare `v["n"].as_i64() == Some(1)`. `to_writer` / `from_reader` / `to_vec` / `from_slice`
  aren't there.
- **regex runs on JS regular expressions.** Inline flags work only at the start of the pattern
  (`(?i)abc`, not `a(?i:b)c`), `x` (verbose) mode, class set operations (`[a&&b]`) and negated
  POSIX classes are errors, `\b` is an ASCII word boundary, and `regex::Error`'s text isn't
  regex's exact report (no caret line). `replace` and friends give a `String` where regex gives
  a `Cow<str>` (`into_owned()` and `to_string()` both work), `Match::range()` is missing, and
  there's no `RegexBuilder`, `RegexSet` or `regex::bytes`.
- **A lazy value is computed when it's created**, so a `LazyLock` / `Lazy` / `lazy_static!`
  static is initialized when the script loads, not on first use (only the order of side effects
  in the initializer differs). `OnceLock` / `OnceCell` have no `take` / `into_inner`.
- **`ensure!(a == b)` without a message** reports `Condition failed: `a == b``; anyhow 1.0.104
  also appends the two values (`(1 vs 0)`).
- **`cfg_if!` and `cfg` decide for a script**: features and every OS are off, so code that picks
  `#[cfg(windows)]` natively takes another branch here. The target is JS, described as rustc's
  `wasm32-unknown-unknown` but with `target_pointer_width = "64"` (`usize` is 64 bits here).
  `#[cfg(feature = "..")]` items are kept (a crate's code may have the feature on), and
  `#[cfg]`s inside macro arguments (`vec![#[cfg(x)] 1]`) aren't evaluated.
- **Only their common surface of `rand` and `itertools`** (see FEATURES.md): no
  `itertools::iproduct!` / `izip!` macros, `minmax`, `exactly_one`, `fold_while`, `kmerge`.
- **rand's distributions:** the built-in ones have `sample` / `sample_iter` methods but don't
  implement the `Distribution` trait, so a `D: Distribution<f64>` bound takes your own types only;
  your `impl Distribution<T> for Standard` can be for one `T`. `Uniform::from(lo..hi)`,
  `WeightedIndex::update_weights`, `Slice`, `Dirichlet`, `Zipf`, `Hypergeometric`, the unit
  shapes (`UnitCircle`, ..) and `DistIter` as a named type aren't there, and `rng.sample_iter(..)`
  doesn't move `rng` (rustc rejects using it afterwards).
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
- **`std::process::exit` ends a script by unwinding**: code in `Drop` impls still runs (Rust's
  doesn't). A Netscript `main(ns)` whose program can exit ends quietly, showing what clap's
  `Error::exit` prints (help, errors) with `ns.tprint`; other output of `print!` goes to the
  browser console, as always.
- **Random numbers differ from rand's.** The generator is xoshiro128**, not ChaCha12, so a
  seeded `StdRng` repeats its own sequence but not rand's, and `shuffle` / `choose` pick
  differently for the same seed.
- **`tuple_windows()` / `collect_tuple()` sizes are read from the code nearby** (a turbofish,
  `|(a, b)|`, `for (a, b, c) in`, `let (a, b) = ..`); anywhere else they are pairs.

## Drop, ranges, `Cow` and `std::io`

- **`Drop` follows moves the move checker sees**: by `let`, assignment, argument, method that
  takes `self`, struct / tuple / `vec!` literal, `return`, `break`, `?`, a closure capturing it
  by value, and a field moved out of a struct (`let x = s.inner;`: the struct drops the rest).
  A closure given to a std method (`it.map(f)`) drops what it captured at the end of that
  statement, with the adapter. A `move` closure that reads only a field with nothing to drop
  (`tag.0`) leaves the value with its scope, as Rust 2021 captures the field. A method taking
  `self` by value drops what's left of `self` at its end, unless it takes `self` apart with a
  pattern. `Rc` is erased, so an `Rc<T>` holding a `Drop` value drops it with the first `Rc`,
  not the last; `v.clear()`, `truncate` and assigning a new value to a field don't drop the old
  elements. On a panic the values in scope are dropped before the panic message is printed
  (Rust prints it first).
- **A range made by `a..b` is an iterator until something asks for a `Range`** (a `let r =
  a..b`, a `Range<T>` field, parameter or return type), so `(0..5).start` isn't there.
- **`Cow::to_mut()` can't change a string in place** (strings are immutable in JS): use it on
  slices, or `into_owned()` and change the `String`. `Cow::from(String)` is borrowed (a `String`
  and a `&str` are one type here), and `is_borrowed` / `is_owned` (unstable in Rust) aren't
  there.
- **`stdin()` is always at its end**, as a script has no input stream: read input from
  `ns.args`, a file or `ns.prompt` instead. Files aren't there.
- **`Read` / `BufRead` are the io prelude's traits.** `BufRead::fill_buf` gives a copy of the
  buffer (a `Vec<u8>`), and `lines()` / `split()` / `bytes()` read everything up front rather
  than lazily. `Seek` isn't there. `Cursor::new(Vec::new())` is a `Cursor<Vec<u8>>` (as its
  `Write` impl would say in Rust); another untyped `Vec` inside a `Cursor` needs an annotation. A
  `Cursor` over a `String` accepts writes (Rust has no such impl), which don't change the string.
- **`&[u8]` reads through a `Cursor` of its own.** Passed for an `R: Read` parameter, the bytes
  are wrapped on entry, so reading leaves the caller's slice where it was, as passing `&[u8]` by
  value does in Rust. Passing `&mut slice` (whose reads advance `slice` in Rust) is an error: use
  a `Cursor`. `get_ref()` of a `BufReader` over bytes gives that `Cursor`.

## Std types and traits filled in

- **`PhantomData`'s `{:?}` is `PhantomData<_>`**: Rust names the type (`PhantomData<i32>`), whose
  name isn't known at run time.
- **`Path` and `PathBuf` are one type**, a path's text, so a `&Path` and a `PathBuf` convert
  freely, and paths are Unix-style on every host (`join` uses `/`; native Windows uses `\`). A
  script has no file system: `exists()`, `is_file()` and `is_dir()` are always `false`.
  Comparing and hashing paths goes by their text, not their components (`a//b` != `a/b`).
- **`OsStr` / `OsString` are `String`**, so string methods work on them too and `into_string()`
  never fails.
- **`NonZeroU32` (and the other widths) is a `u32`**: arithmetic on it type-checks and nothing
  stops a 0 that isn't made through `new`. `NonZeroU32::MIN` is `u32::MIN`.
- **`fmt::Arguments` is a `String`**: `format_args!(..)` formats when it's made, and
  `Arguments::as_str()` isn't there.
- **`size_hint()` on std's iterators is exact** (what's left is counted, through a copy): Rust
  gives `(0, Some(n))` after a `filter`. A custom iterator's comes from its own `size_hint`, or
  `(0, None)`; its `len()` (with `impl ExactSizeIterator`) is that hint's lower bound, where Rust
  also asserts the bounds agree.
- **`x.hash(&mut your_hasher)` writes 8 bytes for every integer** (a `usize` / `u64`'s; Rust
  writes 4 for an `i32`), and a struct's derived `Hash` writes its runtime key (a string), so the
  `finish()` values match Rust's for `usize` / `u64` / `String` / `&str` keys and their `write_*`
  calls only. `HashMap::with_hasher(..)` ignores the hasher: keys compare by value.
- **`[T; N]::map` and slice `strip_prefix` also work on a `Vec`** (arrays and Vecs are one type
  here).
- **`self.0.fmt(f)` on a generic value** picks `Display` or `Debug` by the impl it's in when both
  could apply, and a whole-number float in it takes no precision (`{:.2}` of `Wrap(2.0)` prints
  `2`).

## Formatting

- **`{:#?}` of a value holding a hand-written `Debug`** spreads that impl's own parentheses and
  braces over lines too (the pretty form is made from the compact one), where Rust prints what
  the impl wrote: a serde_json `Value` in `{:#?}` shows `Bool(` / `true,` / `)` for
  `Bool(true)`.

## Proc macros

- **A `TokenStream` is text.** `to_string()` gives the tokens with the source's spacing (one space
  where there was whitespace, comments dropped) but without the line breaks rustc's printer adds
  to long items, so code that splits on `\n` can differ. `quote!` output keeps its template's
  spacing too, where `proc_macro2` prints a space between most tokens (`fn f () {}`); only
  macros that inspect that text can tell. Building a stream from token trees joins them with
  spaces (none after a `Joint` punct), not rustc's exact spacing.
- **`quote!`** splices values by their type as the `quote` crate's `ToTokens` does; types
  without it (a `Vec` outside a repetition, your own structs) are errors.
- **`syn` is a subset.** The prelude covers your own `impl Parse` types on a `ParseStream`,
  `Token![..]`, `parse_quote!`, and parsing derive input, functions, structs, enums, literals,
  identifiers, paths, types and expressions. Types and expressions are token lists, not `syn`'s
  enums: `input.parse::<Expr>()` takes tokens up to the next `,` or `;` (or `=>`) outside `<..>`,
  and a `Type` also stops at `=`, `>` and `{`, which covers fields, arguments and `key = value`
  lists but not every grammar. `peek2` / `peek3` count token trees, so a `::` ahead counts as two.
  Parse errors have `syn`'s wording where it's simple, and point at the invocation rather than
  at the offending token (no spans). The prelude's names (`TkComma` for `Token![,]`,
  `IDENT` for `peek(Ident)`) show up in a macro crate's own error messages.
- **Output is spliced as text.** It's re-parsed in place, so names it introduces can clash with the
  caller's (rustc's `Span::call_site()` behaves the same; `Span::mixed_site()` hygiene isn't
  available). An expression macro's output is parenthesized, as rustc treats it as one expression.
- **Helper attributes** (`#[proc_macro_derive(Name, attributes(helper))]`) stay on the item and are
  ignored.

## Rc and Weak

- **A `Weak` never expires.** Nothing is freed while reachable, so `upgrade()` returns `Some`
  even after every `Rc` is dropped, and there are no `strong_count` / `weak_count`.

## JS globals (the `js` crate)

- **`JSON` goes by the static type.** It writes and reads what serde_json would for the type in
  the source, so a value behind a type parameter in a generic function is written as JS sees it
  (`JSON::stringify(&x)` with `x: T` gives JS's `JSON.stringify`). The serde derives aren't
  needed, and only the `#[serde(..)]` attributes listed in FEATURES apply (no `flatten`,
  `tag`/`untagged` enums, `with`, or custom `Serialize` impls).
- **Integers past `u64` / `i64` are floats to serde_json**, so they're rejected with its "invalid
  type: floating point" error (`i128` / `u128` read them exactly).
- **`JsonError` has no position.** serde_json says `missing field `x` at line 1 column 8`; here
  it's `missing field `x``. Some syntax errors are worded differently.
- **`HashMap` keys come out in insertion order** (serde_json's follow its hash order, which
  varies anyway); `BTreeMap` is sorted in both.
- **`Date` is JS's.** Local time is the browser's time zone, and the string forms (`{}`,
  `to_locale_string`) are the browser's.
- **DOM values are handles, not owned data.** `Element`, `Document`, `Storage` and friends are
  the live JS objects: `clone()` copies the handle, not the node, and changes through any copy
  show everywhere (a `let mut` is still needed to assign a field, as in Rust). There's no
  `web_sys`-style `Result` for a JS exception: a bad selector or a failed `fetch` throws.
- **Timer ids are `f64`.** `set_timeout` returns what JS does (a number in the browser), only
  good for `clear_timeout`. Closures given to timers and event listeners stay alive as long as JS
  holds them; there's no `Closure::forget`.
- **`JsValue` is wasm-bindgen's, with two extensions.** `js::global(..)`, `Response::json()` and
  untyped NS results are `JsValue`s, read with `as_string()` / `as_f64()` / `as_bool()` or
  `serde_wasm_bindgen::from_value::<T>`; a `JsValue` parameter (`console::log`) takes any value
  without `JsValue::from`; and `{}` prints a `JsValue` as JS's `String(v)` (wasm-bindgen has only
  `{:?}`). `serde_wasm_bindgen::to_value` makes maps plain objects, not JS `Map`s.
- **`Promise::all` / `race` take a `Vec` of futures** (unawaited async calls). They start when
  called, as JS promises do, not when polled.
- **`JsValue` doesn't solve inference.** `let v: JsValue = JSON::parse(s).unwrap()` asks for
  a type; write `JSON::parse::<JsValue>(s)`.

## std::time

- **`Instant` has the browser's resolution**: `performance.now()` is microseconds or coarser
  (browsers round it), and it counts from page load, so `{:?}` of an `Instant` isn't rustc's.
  `SystemTime::now()` is `Date.now()`, whole milliseconds; its `{:?}` is Linux's form.
- **Panics inside these types are located in `<std>`.** rustc reports the caller's line for
  `Duration / 0` and `Instant ± Duration` overflow (`#[track_caller]`, which user code can't use
  here either); the messages match.
- **Only programs that name `time::` get the types**, and not when they declare their own
  `Duration`, `Instant` or `SystemTime`. `2 * duration` (`u32 * Duration`) isn't available:
  write `duration * 2`.

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
  function's return value is an error for now (a pointer to a struct can go anywhere).

## Checking

- **A partial borrow checker.** Lifetimes are erased. The common conflicts (changing a Vec while a
  reference into it is used later, or inside a `for` over it) are reported, as warnings unless
  strict borrows are on; borrows that flow through function calls, struct fields or closures
  aren't tracked, so other programs rustc rejects still run.
- **Use after move is checked only where the checker is sure a local owns its value** (see
  FEATURES). It misses moves out of fields (`let v = s.items;` then `s.items`), out of pattern
  bindings, and into closures without `move`; those run with the JS value still shared.
  Definite initialization is checked for `let x;` (use before assignment, assigning an immutable
  one twice); names declared by a `let (a, b);` pattern aren't checked yet.
- **Type checking covers the supported subset only.** Within it, mismatches and mutability
  errors are reported like rustc's. NS calls are unchecked until typed bindings land, and trait
  bounds don't exist yet (only `sort` on floats is special-cased).

## Traits on built-in types

- **A trait implemented for both an integer type and a float type**, called through a generic
  `T: Trait` or `dyn Trait`, picks the impl by the value: a whole number (`2.0f64` included) goes
  to the integer impl (`i64` / `u64` / `i128` / `u128` values are BigInts and always reach their
  own impl). Called on a value of known type (`x.money()` with `x: f64`), the right impl is
  chosen at compile time.
- **A trait of yours named like a std trait** (`trait Write`) is the one `impl fmt::Write for X`
  is checked against, since traits are matched by their last path segment.
- **`impl Tr for &T` / `&mut T` / `Box<T>` must forward to `T`'s impl.** References and boxes are
  erased, so `&x` is `x` and its methods are `T`'s: an impl whose methods each call `T`'s
  (`(**self).f(a)`, `T::f(self, a)`, `self.as_ref().f(a)`) is left out, and one that does
  anything else (or leaves to the default a method another impl overrides) is an error.
- **Arrays and slices share `Vec`'s impls**: `impl Tr for [u8; 4]` and `impl Tr for Vec<u8>` (or
  `[u16]`) would collide, and the element type isn't checked.

## Editions

- **Files are parsed as the 2021 edition.** `gen` is a name (`rng.gen()`, `let gen = ..`), as in
  rand 0.8 code; 2024-only syntax that depends on the edition (`gen` blocks, which are unstable)
  doesn't parse.

## Macros

- **`macro_rules!` hygiene covers local bindings.** Names a macro's body binds (`let`, `for`,
  closure parameters) are renamed per expansion, so they don't clash with the caller's; other
  names (functions, types, a `static` the body names) resolve at the call site, as in a textual
  expansion.
- **Fragments are matched by their tokens**, the way rustc's fragment parsers would split them:
  an `expr` ends at a top-level `,`, `;` or `=>` (a leading closure's `|a, b|` and a turbofish's
  `<A, B>` keep theirs), a `path` keeps commas inside `<..>`, a `ty` ends at a token that can't
  continue a type, an `item` after its `;` or `{ .. }` body. Rules that rely on finer parsing (an
  `expr` followed by an operator token) can match differently.
- **`#[cfg]`-gated alternative macros**: of several `macro_rules!` definitions of one name, an
  invocation uses the last one before it whose `#[cfg]` holds (features off, `test` off). A `cfg`
  the compiler can't evaluate (a custom `--cfg` name, a platform predicate) counts as off; when
  every definition is off, the last one before the invocation is used.
- **A macro's path import is dropped**: `pub(crate) use my_macro;` and `use crate::{my_macro, ..}`
  are removed with the definitions, unless an item of the file has that name too. Another file's
  `use crate::my_macro;` is still unresolved (macros come through `mod`, below).
- **`compile_error!` under a target `#[cfg]` fires as it would on wasm**: `#[cfg(not(any(unix,
  windows)))] compile_error!(..)` is an error here.
- **`env!` is an error and `option_env!` `None`**, except for Cargo's own variables, which get
  placeholders (`CARGO_PKG_NAME` `"script"`, `CARGO_PKG_VERSION` `"0.1.0"`, `CARGO_MANIFEST_DIR`
  `"."`, the rest empty): a script isn't built by cargo.
- **Macros from other files come through `mod`**: a file sees the `#[macro_export]` macros of the
  files it loads with `mod x;` (and all of them with `#[macro_use]`), not those of files further away.

## Evaluation order

- A block-like expression (`if`, `loop`, `{ ... }`) nested inside a larger expression is hoisted
  into statements before it. If an earlier operand in the same expression has side effects, those
  now run after the hoisted block instead of before it.
