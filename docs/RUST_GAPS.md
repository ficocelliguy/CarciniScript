# CarciniScript: Rust compatibility gaps

Places where CarciniScript silently behaves differently from rustc, or accepts programs rustc
would reject. The counterpart to RapydScript-NS's `PYTHON_GAPS.md`. Unsupported features (which
produce a compile error) are tracked in [FEATURES.md](FEATURES.md) instead.

## Integers

- **Overflow wraps by default.** Arithmetic follows rustc's release-mode semantics (two's-complement
  wrapping). With overflow checks on (`setOverflowChecks(true)`, or Bitburner's "Rust: panic on
  integer overflow" option) it panics like a debug build; `i64`/`u64` overflow past 2^53 is
  detected, though the values themselves are inexact there (see below).
- **64-bit integers are JS numbers.** `i64`, `u64`, `isize` and `usize` are exact only up to 2^53.
  They don't wrap at 2^64, and casting a negative value to an unsigned 64-bit type keeps the sign.
  Literals above 2^53 that a double can't hold exactly produce a warning. Their bitwise operators are exact (through BigInt) for
  values that fit, but a result above 2^53, such as `!0u64`, is rounded like any other.
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
  iterator is endless.
- **Closure parameters are lenient about references.** `|x| ..`, `|&x| ..` and `|&&x| ..` all bind
  the element itself; numbers and strings are values in JS either way. Rust's own type check
  still applies to what you write, but CarciniScript accepts some forms rustc would reject.
- **A closure passed where `&mut T` is expected can only write back through collections.** Map
  slots (`retain`, `and_modify`) and `iter_mut().for_each` alias the real element; a named
  function passed in their place gets a copy.
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
  `Into` bounds on generic parameters (`fn f<T: Into<Meters>>(x: T)`) aren't supported either:
  a generic function can't pick the `from` method at run time (types are erased).
- **`{:>10}` on a type with a `Display` impl ignores the width**, which matches Rust for impls
  that use `write!` (rather than `f.pad`).
- **Exhaustiveness checking gives up on very large matches** (it assumes they're covered).
- **64-bit integers parse only within JS's safe integer range**; larger values report overflow.

## Generics and slices

- **Generic functions are erased, not monomorphized.** A `T` allows only operations from its
  bounds; `/` and `%` on a `T` are rejected, because erased code can't tell integer division
  from float division. `+`, `-` and `*` on a `T` don't wrap integer overflow.
- **`{:?}` of a generic value prints whole-number floats without `.0`**, and one-character
  strings and chars the same way, since the type is gone at run time.
- **A width on a derived `{:?}` pads the whole value.** Rust passes the width to each field
  (`{:>8?}` of `P { x: 1 }` is `P { x:        1 }`, and of `vec![1]` is `[       1]`); here the
  result is padded as one string. Hand-written `Debug` / `Display` impls get the width through
  the Formatter, as in Rust.
- **Every file is its own crate root for `mod`.** `mod x;` in `util.rs` loads `x.rs` next to
  `util.rs` when there is one, since each Bitburner script is compiled on its own (and may be run
  directly); otherwise `x/mod.rs`, then `util/x.rs` like rustc. For the same reason `super::` works only inside inline `mod x { .. }` blocks, not at the top of a
  module file, and `crate::` names this file.
- **Generics are erased, so a few things need help at run time.** `T::new()`, `T::CONST` and
  `T::default()` work in free functions, on a method's own type parameters and on an `impl<T>`'s
  in its associated functions, which get `T`'s class as a hidden argument, but not on an
  `impl<T>`'s in methods with `self` (the class would have to travel with the value). Generic
  traits (`trait Convert<T>`) and associated types
  declared in your traits aren't supported.
- **`&mut` to a number, string or bool inside a struct, tuple or Vec can't be passed to a
  function** (the value would be copied). Pass the struct, or a `&mut` to a local and write it back.
- **`RefCell` doesn't track borrows**, so a `borrow_mut` while another borrow is alive doesn't
  panic. `Rc` is erased: `Rc::strong_count` and `Weak` aren't available, and `.clone()` on a
  struct without `#[derive(Clone)]` is taken to be an `Rc` clone (it shares the value).
- **A generic type's arguments can stay unknown** (`let s = Stack::new();` with no use), which
  rustc rejects; nothing depends on them at run time.
- **Shared slices are copies.** `&v[1..3]` reads like Rust; `&mut v[a..b]` is a view instead (see
  above), and `&mut s[a..b]` of a string is rejected.

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
- **64-bit integers past 2^53** are JS numbers here, so `u64::MAX` reads in and writes out
  inexactly (`i128`/`u128` are exact).
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
- **`JsValue` is untyped.** `js::global(..)`, `Response::json()` and `console::log`'s arguments
  aren't checked; read typed data with `JSON::parse::<T>`.
- **`Promise::all` / `race` take a `Vec` of futures** (unawaited async calls). They start when
  called, as JS promises do, not when polled.
- **`JsValue` doesn't solve inference.** `let v: JsValue = JSON::parse(s).unwrap()` asks for
  a type; write `JSON::parse::<JsValue>(s)`.

## std::time

- **`Instant` has the browser's resolution**: `performance.now()` is microseconds or coarser
  (browsers round it), and it counts from page load, so `{:?}` of an `Instant` isn't rustc's.
  `SystemTime::now()` is `Date.now()`, whole milliseconds; its `{:?}` is Linux's form.
- **`u64` seconds are JS numbers**, exact up to 2^53 seconds; `Duration::MAX` is inexact.
  `as_millis` / `as_micros` / `as_nanos` are `u128`, exact.
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
  to the integer impl. Called on a value of known type (`x.money()` with `x: f64`), the right
  impl is chosen at compile time.

## Macros

- **`macro_rules!` hygiene covers local bindings.** Names a macro's body binds (`let`, `for`,
  closure parameters) are renamed per expansion, so they don't clash with the caller's; other
  names (functions, types, a `static` the body names) resolve at the call site, as in a textual
  expansion.
- **Fragments are matched by their tokens**, the way rustc's fragment parsers would split them:
  an `expr` ends at a top-level `,`, `;` or `=>`, a `ty` or `path` keeps commas inside `<..>`.
  Rules that rely on finer parsing (an `expr` followed by an operator token) can match differently.
- **Macros from other files come through `mod`**: a file sees the `#[macro_export]` macros of the
  files it loads with `mod x;` (and all of them with `#[macro_use]`), not those of files further away.

## Evaluation order

- A block-like expression (`if`, `loop`, `{ ... }`) nested inside a larger expression is hoisted
  into statements before it. If an earlier operand in the same expression has side effects, those
  now run after the hoisted block instead of before it.
