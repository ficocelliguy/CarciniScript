# Rust feature support chart

Status reflects phase 1 progress (type checker, `Vec`, `Option`, `match`, `?`, typed NS bindings, tuples, `HashSet`, `HashMap`, closures, iterators, structs, enums and `Result` done). Legend — **Phase**: `0` spike · `1` core · `2` multi-file/NS · `3` language service · `4` later ·
`—` out of scope. **Status**: ✅ done · 🚧 in progress · ⬜ planned · ❌ won't support ·
⚠️ supported with a documented gap (see `RUST_GAPS.md`).

## Items and modules

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `fn` items, params, return types | 0 | ✅ | `function`; all top-level fns exported (like RapydScript `export_all`). Parameters may be patterns (`(r, c): (usize, usize)`, `Point { x, y }: Point`, `&(a, _)`), also in methods |
| `async fn`, `.await` | 0 | ✅ | `async function`, `await` |
| `pub async fn main(ns: NS)` | 0 | ✅ | `export async function main(ns)` |
| `const`, `static` | 1 | ✅ | Module-level `export const`s, emitted after the classes in dependency order (any declaration order works); usable in patterns. `static mut` is a module-level `let`; using it outside `unsafe` is an error like rustc's, as is assigning to a plain `static` |
| `struct` (named, tuple, unit) | 1 | ✅ | A JS `class` whose constructor takes the fields in order; tuple fields are `this[0]`. Literals with shorthand and `..base`, field access and assignment, struct patterns. Generic structs: see below |
| `impl` blocks (inherent) | 1 | ✅ | Class methods (`self` is `this`); associated functions are `static`. `&self` / `&mut self` / `self`, `Self`, `Self { .. }`, UFCS `Point::len(&p)`, methods and constructors as function values |
| `enum` (field-less) | 1 | ✅ | Same class as data enums; `as i32` honours explicit discriminants |
| `enum` (with data) | 1 | ✅ | One class per enum with a `$tag`; `Shape.Circle(r)` factories, unit variants are fresh instances. Tuple, struct and unit variants, nested patterns, exhaustiveness naming the missing variant (`E::B(_)`) |
| `trait` + `impl Trait for Struct` | 1 | ✅ | User traits with required and default methods and supertraits; defaults are copied into each implementing class. `impl Display` / `impl Default` too, and hand-written `impl Ord` / `PartialOrd` / `PartialEq` / `Eq` / `Clone` / `fmt::Debug` (the runtime's comparisons, copies and `{:?}` call them; supertraits are checked; a `Debug` and a `Display` impl can both have `fmt`). Associated consts in impls (`Self::W`, `Grid::W`) and required `const X: T;` in traits ✅. Required trait functions without `self` (`fn make() -> Self;`), called on the type (`Wolf::make(..)`) ✅; calling them on a type parameter (`T::make()`), default ones, associated types, default trait consts and generic traits ⬜ |
| `dyn Trait`, `impl Trait`, `T: Trait` | 1 | ✅ | Erased: a trait method call is a JS method call, so dispatch is dynamic. Structs and enums coerce to `dyn Trait` they implement, also with `as Box<dyn Trait>` / `as Box<dyn Fn(..)>` |
| `impl From<X> for Y`, `Y::from(x)`, `x.into()` | 1 | ✅ | Each `From` impl of a type is its own static method (`from$0`, `from$1`, ..), chosen at compile time by the argument's type; `.into()` takes its target from the context (annotation, argument, field, return). Std `&str` → `String` and lossless numeric widenings are the identity. `impl From<X> for String`/`i32` (std targets) ⬜ |
| Operator overloading (`impl Add<R> for T`, `Sub`, `Mul`, `Div`, `Rem`, `BitAnd`/`BitOr`/`BitXor`, `Neg`, `Not`, `AddAssign` etc.) | 1 | ✅ | Each impl's method is its own JS method (`add$0`, `add$1`, ..), picked at compile time by the right operand's type, so `v * 2.0` and `v * w` can both exist; `a + b` is `a.add$0(b)`, `a += b` calls `add_assign` on `a`. `type Output` and `-> Self::Output` work. `impl Index<I>` reads (`grid[(r, c)]` is `grid.index$0([r, c])`), also by several index types. `impl IndexMut` writes (`grid[(r, c)] = v`, `+=`, a method on a `String` element) go through a cell its `index_mut` hands out, when it ends in `&mut <place>`. For your structs and enums (not `impl Add for i32`). `impl Deref`/`DerefMut`: a method the type doesn't have goes to the target (`stack.push(1)` is `stack.deref_mut().push(1)`), as do indexing, `*x` and `&x` where `&Target` is expected |
| `impl Trait for i32` etc. | 1 | ⚠️ | User traits on `i8`..`usize`, `f32`/`f64`, `bool`, `char`, `str`/`String` and `Vec<T>`: the methods live in a `$ext$f64` class and calls go through `$ext$f64.prototype.money.call(x, ..)`, chosen at compile time (std's own methods win, as inherent methods do). Default methods are compiled again per type. Through a generic `T: Trait` or `dyn Trait`, `κσ.$dcall` finds the impl by the value's JS kind (a whole number goes to an integer type's impl, other numbers to a float type's) |
| Generic fns | 1 | ✅ | Type parameters erased; each call instantiates them (turbofish too) and checks the bounds. Inside the body a `T` allows only what its bounds give: `Copy`, `Clone`, `Debug`, `Display`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, `Default`, `Add`, `Sub`, `Mul` |
| Generic structs, enums, impls | 1 | ✅ | `struct Stack<T>`, `enum Tree<T>`, `impl<T: Bound> Stack<T>`: arguments inferred from construction and use or written (`Stack<i32>`); fields, methods and patterns are typed per instantiation. Erased in JS; `{:?}` is told the argument types where they're known. Generic methods (`fn map<U, F: Fn(&T) -> U>` in an impl) instantiate per call |
| `T::assoc()` on generic `T` | 4 | ⬜ | Hidden trait-dictionary params |
| `mod foo;` | 2 | ✅ | Loads `foo.rs` (or `foo/mod.rs`) next to the file through the host's resolver (`compile(src, name, resolve)`); its `pub` fns, structs, enums, traits and consts are typed here. Emitted as `import * as foo from "./foo.rs"`; each file compiles to its own JS module. Missing files, private items and circular modules are errors |
| Inline `mod foo { ... }` | 2 | ✅ | Flattened before compiling: each item moves to the top level as `foo__item` (`a__b__item` when nested) and every path is resolved in its own module's scope: `foo::item`, `self::`, `super::` (also `super::super::`), `crate::foo::item`, `use` inside and outside modules (`as`, `*`, `{self as m}`), paths inside macro arguments (`format!("{}", geo::UNIT)`). `{:?}` prints the written names. Private items and modules used from outside are errors, as are missing names in a module. `#[cfg(test)] mod tests` is left out |
| `use foo::{a, b as c}`, `use foo::*`, `foo::item`, `crate::foo::item` | 2 | ✅ | Names from loaded modules; renaming works for functions and constants. std functions imported by name (`use std::cmp::max;`, `use std::mem::{swap, take as grab};`) are called like their paths. Nested module paths (`foo::bar::item`) through inline modules and through `pub mod` files (`geo.rs` → `geo/shapes.rs`, also in `use`) ✅ |
| `use Enum::*`, `use Enum::{A, B as Bee}` | 1 | ✅ | For enums of the file, at the top or inside a function (then for that function): each imported name in expressions, patterns and macro arguments is written out as `Enum::A` before compiling |
| `type` aliases (`type Grid<T> = Vec<Vec<T>>;`) | 1 | ✅ | File-level (also in inline modules) aliases are replaced by their type before compiling, generic arguments filled in; also in expressions and patterns (`Counts::new()`, `Bot { .. }`). Aliases inside functions and blocks are scoped to them (shadowing outer ones) |
| `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]` | 1 | ✅ | Generated `$dbg`, `$clone`, `$eq`, `$cmp`, `static default()`; `#[default]` variants. Checked like rustc: `Copy` needs `Clone`, no `Eq`/`Hash`/`Ord` with float fields, `HashSet`/`HashMap` keys need `Hash + Eq` |
| Other attributes | 1 | ⚠️ | `#[cfg(test)]` / `#[test]` items are left out; `#[allow(..)]` and others are ignored |
| Visibility (`pub`, `pub(crate)`) | 1 | ⚠️ | Enforced for module files and inline modules (a private item or module used from outside its module is an error); `pub(crate)`/`pub(super)` count as `pub` |
| Lifetimes (`'a`, `'static`, `for<'a>`) | 0 | ✅ | Parsed and erased |
| `unsafe { }` blocks, `unsafe fn`, `unsafe impl` | 1 | ✅ | Ordinary blocks and functions in JS; `unsafe impl Send`/`Sync` for a type is accepted and does nothing |
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
| `i64`/`u64`/`usize`/`isize` | 0 | ⚠️ | JS number; exact only to 2^53. Bitwise operators (`&`, `|`, `^`, `!`, `<<`, `>>`, compound forms) go through BigInt beyond 32 bits, wrapping to 64 bits; shift amounts are masked as in release builds. `from_str_radix` on every integer type; `str::as_bytes`/`into_bytes`, `str::from_utf8`, `String::from_utf8(_lossy)` |
| `i128`/`u128` | 1 | ✅ | JS BigInt, wrapping at 128 bits: literals, arithmetic (division by zero panics), shifts, bit operations, `!`, casts to and from the other numeric types, ranges, `pow`/`abs`/`min`/`max`, `sum`/`product`, `parse`, `{}`/`{:x}`/padding. Other integer methods on them ⬜ |
| Overflow panics (debug semantics) | 4 | ⬜ | Opt-in checked mode; default wraps |
| Comparison, logical, bitwise ops | 0 | ✅ | Direct; `>>` on unsigned → `>>>` |
| `as` casts | 0 | ✅ | Truncation/wrap helpers per target type |
| Compound assignment (`+=` ...) | 0 | ✅ | Same typed lowering as the binary op |
| `if` / `else` (statement and expression) | 0 | ✅ | `if` or IIFE/temp for expression position |
| Block expressions with tail value | 0 | ✅ | Temp variable |
| `loop`, `while`, `break`, `continue` | 0 | ✅ | `for(;;)`, `while` |
| Labeled loops | 0 | ✅ | JS labels (`$outer:`) |
| Labeled block expressions (`'a: { .. break 'a v; .. }`) | 1 | ✅ | A JS labeled block; the value goes where the block's value goes |
| Items inside function bodies (`fn`, `const`, `struct`, `enum`, `impl`) | 1 | ✅ | Lifted to the top level (they can't use the function's locals); two nested items with the same name are an error |
| `break value` from `loop` | 1 | ✅ | The value goes where the `loop`'s value goes (a temp, a `return`, ...); labeled breaks too. A `loop` without `break` has type `!` |
| `for i in a..b` / `a..=b` / `.rev()` | 0 | ✅ | Counting `for` loop, no iterator allocated |
| `for x in v` / `&v` / `.iter()` / `.into_iter()` over a `Vec` | 1 | ✅ | `for...of` |
| `for x in &mut v` / `v.iter_mut()` | 1 | ✅ | Index loop; `x` aliases `v[i]`, so `*x += 1` writes through |
| `for (i, x) in v.iter().enumerate()` (also `iter_mut`), `for x in &set`, destructuring loop patterns | 1 | ✅ | Index loop binding `i` and `v[i]` directly; `for...of` over sets |
| `for x in` other iterators (adapter chains, `s.chars()`, `'a'..='z'`, `0..`, `.enumerate()` of any iterator) | 1 | ✅ | `for...of` over a `κσ.Iter` |
| `if let` / `while let` / `let ... else` | 1 | ✅ | Pattern test + bind; `while let` re-evaluates its scrutinee each iteration. Let-chains (`if let Some(x) = a && x > 1 && let Ok(y) = b`, also in `while` and `else if`) ✅: nested `if`s, each part evaluated only after the previous ones held |
| `match` on literals (incl. negative), ranges (`a..=b`, `a..b`, `a..`, `..=b`; ints and chars), `|`, guards, `x @ p`, `Some`/`None`, as a statement or a value | 1 | ✅ | if/else chain; a labeled block when guards are present |
| Tuple patterns, including `..` (`(first, .., last)`) and nesting | 1 | ✅ | Elements are `subject[i]` |
| Struct, tuple-struct, enum-variant and unit patterns (with `..`) | 1 | ✅ | `$tag` test plus field bindings; bindings through `&mut` (`match self` in a `&mut self` method) alias the field, so `*r *= 2.0` writes back |
| Slice patterns | 1 | ✅ | `[]`, `[x]`, `[first, .., last]`, `[head, rest @ ..]`, nested and guarded; exhaustiveness reasons about lengths ("`[_, _, ..]` not covered") |
| Exhaustiveness checking | 1 | ✅ | Pattern-matrix usefulness over `bool`, `Option`, `Result`, tuples, enums, slices and integer ranges (`0..=127`, `128..=u8::MAX`, `i32::MIN..=-1`, `1..`), naming the missing case like rustc (`(false, Some(_))`, `101_u8`, `'b'`); `char` ranges (`'a'..='z'`, `'{'..=char::MAX`) too. Guarded arms don't count; `#[cfg]`'d variants are never asked for; matches on types the checker doesn't know (another crate's enums) aren't checked. String matches need `_` |
| Tuples: `(a, b)`, `t.0`, `(A, B)` types, `let (a, b)`, `(a, b) = (b, a)`, `==`, `<` (lexicographic), `{:?}` | 1 | ✅ | JS arrays; copied when read from a place into a by-value position, so they behave as values |
| Closures (`\|x\| ...`, `move`, block bodies, `return`, `?`, pattern parameters, captures read and written) | 1 | ✅ | Arrow functions. Parameter types come from annotations or the call site |
| `impl Fn(A) -> R` / `&dyn Fn` / `fn(A) -> R` / `<F: Fn(A) -> R>` (also `where`) parameters and returns; named functions as values | 1 | ✅ | A JS function. Each `F` is one concrete closure type; other generics ⬜ |
| `?` on `Option` | 1 | ✅ | `const $t = x; if ($t == null) return null;` |
| `std::error::Error`, `Box<dyn Error>`, `fn main() -> Result<(), E>` | 1 | ✅ | `impl Error for MyError {}` (needs `Debug` + `Display`); `?` into `Box<dyn Error>` passes std errors and your error types through unchanged, and `{}` / `{:?}` show them. A `main` returning `Err` fails with rustc's `Error: {e:?}` (a thrown `CarciniError`; exit code 1 under node) |
| `?` on `Result` | 1 | ✅ | `if ($t.$tag === 1) return $t;`. A different error type is converted through `impl From<E1> for E2`: `return κσ.Err(E2.from$0($t[0]))` |
| References `&x`, `&mut x`, `*x` | 0 | ✅ | Erased |
| `&mut` on a primitive local | 1 | ✅ | A number, string, bool or `Option` whose `&mut` is passed to a function or method, or bound with `let r = &mut x`, lives in a one-element array; `&mut i32` parameters receive it, `*x += 1` writes `x[0]`. `&mut s.field` of a primitive passed to a function ⬜ (error) `&mut obj.field` / `&mut v[i]` / `&mut grid[r][c]` passed to such a parameter is a getter/setter cell over that slot (so `fn insert(slot: &mut Option<Box<Node>>, ..)` can recurse with `&mut n.left`); keeping such a `&mut` in a local is an error |
| `Copy` structs by value | 1 | ✅ | Cloned when read from a place into a `let`, assignment, argument, return, struct field or `vec!` element; `&x` / `&mut x` pass the object itself |
| `return` | 0 | ✅ | `return` |
| Indexing `v[i]` (read, assign, compound assign) | 1 | ✅ | `v[κσ.$ix(v, i, loc)]`; rustc's out-of-bounds panic message and location |
| Array literals `[a, b]`, `[x; n]` | 1 | ⚠️ | Same as `vec!` (fixed-size arrays are growable JS arrays) |

## Macros

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `println!` / `print!` / `eprintln!` | 0 | ✅ | `console.log` / `console.error` |
| `format!` with `{}` / `{name}` / positional | 0 | ✅ | Template literal |
| `{:?}`, width, precision, fill, alignment, `+`, `0` | 0 | ✅ | Runtime helpers. Also `{:#?}` (pretty), `{:x}`/`{:X}`/`{:o}`/`{:b}` (two's complement for negatives; `#` adds `0x`/`0o`/`0b`, zero padding goes after it), `{:e}`/`{:E}`, and width/precision from arguments (`{:w$}`, `{:1$}`, `{:.*}`, `{:.p$}`). `{:.N}` rounds exact ties to even, like Rust |
| `panic!`, `assert!`, `assert_eq!`, `assert_ne!`, `unreachable!`, `todo!` | 0 | ✅ | `CarciniPanic` with `file:line:col` |
| `vec![a, b]`, `vec![x; n]` | 1 | ✅ | `[a, b]`; `κσ.$vecRepeat` clones non-primitive elements (`vec![vec![0; 3]; 3]` gets 3 rows) |
| `matches!`, `write!`, `writeln!` | 1 | ✅ | `matches!` (with guards) lowers to a `match` in an arrow function called in place, so it works anywhere a value does; `write!` targets a `Formatter` or a `String` |
| `macro_rules!` | 1 | ⚠️ | Expanded before compiling: rules matched in order; fragments `expr` (one parenthesized expression), `ident`, `ty`, `path`, `pat`, `literal`, `lifetime`, `tt`, `block`, `stmt`, `item`, `vis` (also empty), `meta`; `$( .. ) sep? *` / `+` / `?` repetitions, nested; recursive macros; macros producing expressions, statements or items; invocations inside other macros' arguments. Errors and panics point at the code you wrote (inside an expansion: its invocation), and hover, completions and signature help work on the expanded code. Macros from `mod` files: `#[macro_export]` ones, and all of a module's with `#[macro_use] mod x;` ✅. Hygiene isn't modeled |

## Standard library surface

| Feature | Phase | Status | JS lowering / notes |
|---|---|---|---|
| `Option<T>` | 1 | ✅ | `None` = `null` (`undefined` from JS also reads as `None`), `Some(x)` = `x`. `is_some`, `is_none`, `unwrap`, `expect`, `unwrap_or`, `unwrap_or_default`, `take`, `replace`, `as_ref`, `cloned`, `==`, `{:?}`. `Option<Option<T>>` / `Option<()>` rejected |
| `Option` methods taking closures | 1 | ✅ | `map`, `and_then`, `filter`, `unwrap_or_else`, `or_else`, `map_or`, `map_or_else`, `is_some_and`, `is_none_or`, `inspect`, plus `or`, `zip`, `transpose`. `Result` adds `inspect`/`inspect_err`/`transpose`; `bool::then` (lazy) / `then_some`. `Some`/`Ok`/`Err` work as functions (`.map(Some)`). `Option::flatten` ⬜ (`Option<Option<T>>` isn't representable) |
| `Result<T, E>` | 1 | ✅ | `κσ.Ok(v)` / `κσ.Err(e)`. `is_ok`, `is_err`, `unwrap`/`expect` (rustc's panic messages), `unwrap_err`, `unwrap_or`, `unwrap_or_else`, `unwrap_or_default`, `ok`, `err`, `map`, `map_err`, `and_then`, `or_else`, `is_ok_and`, `is_err_and`, `map_or`, `==`, `{:?}`. `Option::ok_or` / `ok_or_else` |
| `str::parse::<T>()` | 1 | ✅ | Integers (range-checked), floats (`inf`, `NaN`, exponents), `bool`, `char`; std's `ParseIntError` etc. with matching `Debug` and `Display` |
| `impl fmt::Display` | 1 | ✅ | `write!` / `writeln!` / `f.write_str` into a `Formatter`, `?` on `fmt::Result`; `{}` and `to_string()` use it. Width and precision are ignored, as with `write!`-based impls in Rust |
| `String`, `&str` common methods | 0–1 | ✅ | JS strings, UTF-8 semantics where they show (`len`, byte offsets, slicing). `trim*`, `to_uppercase`/`lowercase`, `contains`/`starts_with`/`ends_with`/`find`/`rfind`/`replace`/`replacen`/`split*`/`trim_matches`/`strip_prefix`/`split_once` with `&str`, `char` or closure patterns, `char_indices`, `bytes`, `lines`, `matches`, `splitn`/`rsplitn`/`rsplit`, `repeat`, `push`/`push_str`/`extend` (chars or strings)/`insert`/`insert_str`/`pop`/`remove`/`truncate`/`clear`, `eq_ignore_ascii_case`, `String::with_capacity`, `String::from` as a function |
| `char` | 1 | ✅ | One-code-point string. `is_alphabetic`/`is_numeric`/`is_whitespace`/`is_ascii_*` and friends (Unicode regexes), `to_ascii_uppercase`/`lowercase`, `to_digit`, `is_digit`, `to_string`; `to_uppercase()`/`to_lowercase()` are char iterators (`next`, `collect`, `flat_map`, `extend`, `to_string`, `{}`), so `'ß'` becomes "SS"; `Chars::as_str()` |
| `Vec<T>`, `&[T]`, `[T; N]` | 1 | ✅ | JS `Array`. `new`, `with_capacity`, `len`, `is_empty`, `push`, `insert`, `remove`, `swap`, `truncate`, `clear`, `reverse`, `sort`/`sort_unstable` (Rust `Ord`), `contains`, `extend_from_slice`, `join`, `concat`, `clone`/`to_vec`, `==`/`!=`, `{:?}`. Panic messages match rustc |
| `&v[a..b]`, `&s[a..b]` slicing | 1 | ✅ | A copy of that part (`a..b`, `a..`, `..b`, `..`, `a..=b`); strings by UTF-8 byte offset. Out-of-range and char-boundary panics match rustc. In-place methods on a range (`v[1..4].sort()`, `reverse`, `fill`, `swap`, `rotate_*`, `copy_from_slice`) write back into `v`. `&mut v[a..b]` is a view (a JS `Proxy`) whose writes, swaps and sorts reach `v`, also passed to functions and re-sliced; other writes through a range (`v[2..].iter_mut()`, `v[0..2][1] = 9`) and `split_at_mut` use views too. `&mut s[a..b]` on strings is an error |
| More `Vec` methods | 1 | ✅ | `chunks_exact`, `binary_search`/`_by`/`_by_key` (std's algorithm), `split_at`, `starts_with`/`ends_with`, `drain(range)`, `split_off`, `append`, `fill`, `rotate_left`/`right`, `swap_remove`, `repeat`, `is_sorted`, with rustc's panics |
| Integer and float helpers | 1 | ✅ | `checked_*`/`saturating_*`/`wrapping_*` (`add`, `sub`, `mul`, `div`, `rem`, `pow`, `neg`, `abs`), `abs_diff`, `count_ones`/`zeros`, `leading_zeros`/`trailing_zeros`, `is_power_of_two`, `f64::total_cmp`, `u64::from(x)`, `char::from_digit`/`from_u32`/`from` |
| `std::mem::{swap, replace, take}` | 1 | ✅ | Places are reassigned; `&mut` parameters have their contents swapped |
| `Vec` methods returning `Option` (`pop`, `first`, `last`, `get`) | 1 | ✅ | `get_mut`/`first_mut`/`last_mut` for elements that are JS objects (structs, tuples, arrays, collections), which writes go through; for numbers, strings and bools they're an error (the element would be a copy) |
| Iterator sources | 1 | ✅ | `iter`/`into_iter` (Vec, HashSet, HashMap), `keys`/`values`, ranges as values (`0..n`, `1..`, `'a'..='z'`), `chars`, `split`, `split_whitespace`, `lines`, `windows`, `chunks` |
| `impl Iterator<Item = T>` / `Box<dyn Iterator<Item = T>>` as parameter and return types | 1 | ✅ | The runtime's lazy iterator, so a function can return a chain (`fn evens(v: &[i32]) -> impl Iterator<Item = &i32> + '_`) |
| Custom iterators (`impl Iterator for T { type Item = X; fn next(&mut self) .. }`) | 1 | ✅ | The class gets `[Symbol.iterator]` driving its `next`, so values loop with `for` and take every adapter and consumer (`Fib { .. }.take(10).collect()`), also for generic iterator structs. `Self::Item` works in signatures |
| Iterator adapters | 1 | ✅ | Lazy `κσ.Iter`: `map`, `filter`, `filter_map`, `flat_map`, `flatten`, `enumerate`, `zip`, `chain`, `rev`, `take`, `skip`, `step_by`, `take_while`, `skip_while`, `inspect`, `cloned`, `copied`, `cycle`. `iter_mut()` with any adapter that passes items on (`zip`, `filter`, `skip`, ..) when the elements are JS objects (structs, tuples, ..), whose writes reach the Vec; over numbers and strings only with `for_each` / `enumerate` / a `for` loop |
| Iterator consumers | 1 | ✅ | `next` (on an iterator in a `let mut`), `collect` into `Vec`/`String`/`HashSet`/`HashMap`, or `Result`/`Option` of those (stopping at the first `Err`/`None`), from an annotation, turbofish or later use, `sum`/`product` (wrapping like the type), `count`, `last`, `nth`, `min`/`max` (+ `_by`, `_by_key`), `any`, `all`, `find`, `find_map`, `position`, `rposition`, `partition`, `fold`, `reduce`, `for_each` |
| `Vec` methods taking closures | 1 | ✅ | `sort_by`, `sort_by_key` (+ `_unstable`/`_cached`), `retain`, `dedup`, `dedup_by_key`, `extend` from any iterable |
| `HashSet<T>` | 1 | ✅ | `κσ.HashSet`, keyed structurally (tuples work as elements). `new`, `from`, `insert`, `contains`, `remove`, `len`, `is_empty`, `clear`, `clone`, `is_subset`/`is_superset`/`is_disjoint`, `==`, `{:?}`, iteration. Float elements rejected (`f64: Hash`) |
| `HashSet` set operations returning iterators (`union`, `intersection`, `difference`, `symmetric_difference`), `retain`, `extend` | 1 | ✅ | |
| `HashMap<K, V>` | 1 | ✅ | `κσ.HashMap` of `[key, value]` slots, keyed like `HashSet`. `new`, `from`, `insert`/`remove` (old value as `Option`), `get`, `contains_key`, `len`, `is_empty`, `clear`, `clone`, `map[&k]` (rustc's panic), `==`, `{:?}`. Float keys rejected |
| `entry(k).or_insert(v)` / `.or_default()` / `.or_insert_with(Vec::new)` | 1 | ✅ | The value is the slot's `[1]`, so `*map.entry(k).or_insert(0) += 1` writes back, as does `let n = map.entry(k).or_insert(0); *n += 1;` |
| `for (k, v) in &map`, `.keys()`, `.values()`, `.iter_mut()`, `.values_mut()` | 1 | ✅ | `for...of` over slots; `_mut` forms alias the slot |
| `get_mut` | 1 | ✅ | For collections and tuples, the element itself. For numbers, strings and bools (`HashMap::get_mut`, `Vec::get_mut`/`first_mut`/`last_mut`) where the result is matched (`if let`/`while let`/`match`) or unwrapped (`*m.get_mut(k).unwrap() += 1`, `let r = m.get_mut(k).unwrap()`): a slot whose writes go back into the collection. Anywhere else (an `Option<&mut i32>` passed on) is an error |
| `HashMap` methods taking closures or returning iterators: `retain`, `entry(k).and_modify(..)`, `or_insert_with(\|\| ..)`, `or_insert_with_key`, `extend`, `iter`/`keys`/`values` as iterators | 1 | ✅ | Closures that take `&mut V` alias the slot, so writes stick. `drain` ⬜ |
| `BTreeMap`, `BTreeSet`, `VecDeque`, `BinaryHeap` | 1 | ✅ | `VecDeque` and `BinaryHeap` are JS arrays (`push_back`/`push_front`/`pop_front`/`front`/`back`/`insert`/`remove`/`make_contiguous`/`append`; `push`/`pop`/`peek`/`into_sorted_vec`/`into_vec`/`drain`/`extend`/`append`, in std's internal heap order so `{:?}` matches); `BTreeMap`/`BTreeSet` are `κσ.BTreeMap`/`κσ.BTreeSet`, the hash classes kept in key order (`first`/`last`/`pop_first`/`pop_last`, `first_key_value`/`last_key_value`, sorted set operations). Everything shared with `Vec`/`HashMap`/`HashSet` works the same (`entry`, `retain`, `iter`, indexing, `collect`, `{:?}`, `==`). Keys and heap items must be `Ord`. `std::cmp::Reverse` (after `use std::cmp::Reverse`) makes min-heaps. `range(..)` ⬜ |
| `Box<T>` | 1 | ✅ | Erased: `Box::new(x)` is `x` (so `Box<dyn Trait>` and `Vec<Box<dyn Trait>>` work) |
| `Rc<T>`, `Arc<T>`, `RefCell<T>`, `Cell<T>` | 1 | ✅ | `Rc`/`Arc` erased (`Rc::new`, `Rc::clone`, `.clone()` share). `RefCell`/`Cell` are `κσ.RefCell` objects: `borrow`/`borrow_mut` reach `.v` (assignable, also through `let b = c.borrow_mut()`), `replace`, `take`, `get`, `set`, `into_inner`, `{:?}`. Borrow conflicts aren't checked |
| `Arc`, `Mutex`, threads | — | ❌ | No threads in Bitburner |
| `std::fs`, `std::net`, `std::process` | — | ❌ | Use the NS API |
| `f64` / integer math methods (`sqrt`, `pow`, `round`, `abs`, `min`, ...) | 0 | ✅ | `Math.*`; `round` is half-away-from-zero |
| `Ordering`, `cmp`, `partial_cmp` | 1 | ✅ | -1 / 0 / 1. `reverse`, `then`, `then_with`, `is_lt` etc., matching on `Ordering::Less`, `{:?}`. `std::cmp::{min, max}` (by `Ord`, so also strings and tuples) ✅ |

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
| Move checking (use after move) | 1 | ⚠️ | rustc's E0382 for locals and by-value parameters that own a non-`Copy` value: moves by `let`, assignment, by-value arguments, struct/tuple/array/`vec!` elements, `for`, `self` methods (user and std: `into_iter`, `unwrap`, iterator adapters, ..), `String +`, `move` closures, `return`/`break`/`?`. Follows branches, loops (a move in an earlier iteration), `break`/`continue`/labeled blocks, and reassignment. Conservative: partial moves (`x.field`), pattern bindings and closures that consume a capture without `move` aren't checked. Definite initialization too: a `let x;` used before every path assigns it ("isn't initialized" / "is possibly-uninitialized") or an immutable one assigned twice (also in a later loop iteration) is rustc's E0381 / E0384 |
| NLL-lite borrow warnings | 4 | ⬜ | Warnings; strict setting makes them errors |
| Full borrow checker (Polonius) | — | ❌ | Not planned |
| Monaco diagnostics / completion / hover / signature help / go to definition / references / rename | 2–3 | ✅ | Diagnostics, hover (types, signatures, NS docs and RAM costs) and completion after `.` (NS methods/fields, struct fields and methods, trait methods, std methods) ✅ via `hover()` / `completions()`; completion of names in scope (locals, functions, types, consts, modules) and signature help (`signatureHelp()`) ✅. Go to definition (`definition()`: locals by scope, items, methods and fields by the receiver's type, variants, associated functions, aliases, inline modules, and items of `mod x;` files, which open in their own tab), references in the file (`references()`) and rename within the file (`rename()`) ✅. Bitburner registers them all for `.rs`; references and rename across files ⬜ |
| Warn on `loop`/`while` with no `.await` | 2 | ✅ | A `loop` or `while true` with no `.await`, `break`, `return`, `?` or `panic!` is flagged: it would freeze Bitburner |

## Potential future features (from Bitburner Discord discussion)

Rust features that Bitburner Discord users have talked about using, liking, or missing in JS,
gathered from a server-wide "rust" search and from #beyond-javascript (November 2026 snapshot).
Most of them already appear in the tables above. This section records the details users cared about,
so the work can be prioritized against real interest. The **Status** column repeats the main chart.

| Feature | Status | What came up |
|---|---|---|
| `let ... else` | ✅ | Called a "favorite pattern" several times: `let Some(x) = opt else { return; };`, `let Ok(lock) = m.write() else { ... };` |
| `if let` / `while let` | ✅ | `while let Some(node) = tree.get(&seq) { match ... }`; `if let` in expression position |
| `match` as an expression, guards, unwrapping by pattern | ✅ | The most praised feature overall. Users want "unwrap the value if it matches" plus `if`/`match`/`loop` that yield values |
| Rest and slice patterns (`..`, `[first, .., last]`), destructuring | ✅ | Rest patterns compared to JS spread; "rust has destructuring" |
| `?` on `Result` / `Option` | ✅ | Named as the reason Rust doesn't need exceptions |
| Collecting into `Result`/`Option` (`collect::<Result<Vec<_>, _>>()`) | ✅ | Short-circuits on the first `Err`; needs the turbofish and `FromIterator` for `Result` |
| Turbofish `::<T>` | ✅ (`collect`, `sum`, `product`, `parse`, generic fns, `Ok::<T, E>`) | Needed for `collect`, `parse`, `throw_error::<...>` style calls |
| `Option`/`Result` combinators | ⚠️ (all but `Option::flatten` ✅) | `unwrap_or`, `unwrap_or_default`, `map_or`, `transpose`, `ok`, `flatten`, `inspect_err`, `bool::then` |
| Iterator chains | ✅ | `iter`/`into_iter`, `enumerate`, `zip`, `map`, `filter`, `sum`, `chars`, `cycle`, `collect::<String>()`. Long dot-chains are a common style |
| Range values and range indexing | ✅ | Ranges as values (iterators), `(a..=b).contains(&x)` (also open and float ranges); `&v[a..b]` and `&s[a..b]` (all range forms) with rustc's panics; `&mut v[a..b]` views ✅ |
| Enums with data (ADTs) | ✅ | "What Rust calls enums are algebraic data types". Used for union types instead of TS enums |
| `impl Default` + `unwrap_or_default` | ✅ | `enum CardKey { SIX([u8; 6]) }` with a hand-written `Default` |
| Method receivers `&self` / `&mut self` / `self` | ✅ | Consuming `self` for builder/finalize patterns; assoc fns without `self` as static methods |
| UFCS call form `Foo::method(foo)` | ✅ | Shown alongside `foo.method()` as equivalent |
| Traits with default methods, generic impls, bounds, `where` | ✅ | `impl<R: BufRead> Reader<R>`, `T: Float + From<f32>`, `where f64: From<T>` |
| Extension traits (methods visible only when the trait is in scope) | ⚠️ (traits on built-in types ✅, statically dispatched; scoping by `use` ⬜) | Contrasted with JS prototype patching. Needs `use` of a trait to gate method lookup |
| Fully qualified disambiguation `<T as Trait>::method` | ⬜ | Rust's answer to same-named methods from two traits |
| `From` / `Into` conversions | ✅ | `impl From<X> for Y` then `.into()`, very common in binding code |
| `std::ops` traits (`Deref`, `Add`, ...) | ⚠️ (arithmetic, bitwise, `Neg`/`Not`, the `*Assign` traits, `Index`, `IndexMut`, `Deref` and `DerefMut` on your structs and enums ✅) | `impl Deref for Wrapper`. Operator overloading "possible but rarely used" |
| Closures, including `move` | ✅ | "Closures are extra incredible in rust" |
| `macro_rules!` | ⚠️ (✅ but no hygiene) | Discussed at length: `$(...)*` repetition, fragment specifiers (`ident`, `ty`, `tt`, `expr`, `literal`), recursive macros, hygiene, macros used to stamp out impls per numeric type |
| `u128` / `i128` | ✅ | Widening multiply macros map `u64 → u128` |
| Fixed-size arrays and boxed slices (`[u8; 6]`, `Box<[u8]>`) | ⚠️ / ⬜ | Bitboard-style code with `&= !mask` on array elements |
| `Cell` / `RefCell` (interior mutability) | ✅ | Explained in chat as "mutates opaquely through a non-mut reference" |
| Overflow panics | ⬜ (4) | "rust panics on overflow if you dont specifically allow it". A user ran into it right away |
| Short-circuit block expressions | ✅ | `a && { b += 1; true }` works when the types match |
| Shadowing | ✅ | "I wish JS had the same capabilities of shadowing as rust has" |
| `#[cfg(test)] mod tests` + `#[test]` | ⚠️ (left out of the build like `rustc` without `--test`; an in-game test runner ⬜) | Rust testing is described as much nicer than JS testing. Possible in-game test runner |
| `///` doc comments (`# Arguments` sections) | ✅ (hover shows them, as markdown, for functions, methods and fields declared in the file) | Could feed Monaco hover/signature help |
| `#[allow(...)]` and naming-convention lints | ⬜ | `#[allow(dead_code)]`; rustc warns on non-snake-case names |
| Module paths (`module::function`, `foo.rs` vs `foo/mod.rs`) | ✅ (`pub use` re-exports ✅, also renamed and `*`; `super::` inside inline modules ✅; `super::` from a module *file* ⬜, since each file compiles on its own) | "I kinda miss rusts module::function" |
| `matches!`, `vec!`, `format!` family | ✅ | Used constantly in pasted snippets |
| Panic surfacing | ✅ | Wasm users had to install `panic::set_hook` to see panics in Bitburner. `CarciniPanic` with `file:line:col` already covers this |

Out of scope, even though they came up: threads and `mpsc` channels, pointer arithmetic and layout `transmute`,
crates.io proc macros (user-written ones are phase 4, above), external crates (`serde`, `rayon`,
`wasm_bindgen`), and generic code over numeric types that Rust itself can't express (SFINAE).
