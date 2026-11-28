# Changelog

All notable changes to CarciniScript are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the package follows
[Semantic Versioning](https://semver.org/).

## 0.1.0

The first public release on npm. CarciniScript compiles Rust to JavaScript ES modules, in the
browser or in Node, at runtime, with source maps back to the Rust. Nothing to install beyond the
package: there's no Rust toolchain and no build step.

### Compiler

- **Rust to ES modules, synchronously.** `compile(source, filename, resolve)` returns an ES module
  and a v3 source map in milliseconds. The compiler is Rust (built on rust-analyzer's parser)
  compiled to WebAssembly. Generated code calls a small runtime installed once on `globalThis.κσ`.
- **Source maps everywhere.** Breakpoints, stepping, stack traces and panic messages in devtools
  point at Rust lines, not generated JavaScript.
- **Most of the Rust language.** Structs, enums, traits (default methods, supertraits,
  associated types and consts, blanket impls, impls on built-in types), generics and const
  generics, pattern matching with exhaustiveness checking, closures, iterators, `Result` and `?`,
  operator overloading, `Deref`, `Drop`, `impl Trait` / `dyn Trait`, async/await (including
  hand-written futures), `macro_rules!` and inline modules.
- **Rust's numbers.** Integer types wrap like a release build and divide like Rust. `i64` / `u64`
  are plain JS numbers, exact to 2^53, with a warning when results may be inexact.
  `i128` / `u128` are BigInts.
  `setOverflowChecks(true)` gives debug-build overflow panics.
- **Rust's output.** Programs print the same output and panic with the same messages as when
  built with `rustc`, checked by differential tests against native builds.
- **A large slice of std.** `String` and `str`, `Vec` and slices, `HashMap` / `HashSet` /
  `BTreeMap` / `BTreeSet` / `VecDeque` / `BinaryHeap` / `LinkedList`, `Option` / `Result`
  combinators, the full iterator adapter and consumer set, `Rc` / `Arc` / `RefCell` / `Cell` /
  `Weak`, `Cow`, `std::fmt` (width, precision, fill, alignment, `{:?}` and `{:#?}`),
  `std::io` readers and writers, `std::time`, `std::mem`, `std::any`, paths and more.
  `Mutex`, `RwLock`, atomics, `mpsc` and `thread_local!` work on the one JS thread.

### Checking and diagnostics

- **Type checking and inference** with rustc's rules, including trait bounds on generic code.
- **Borrow checking (NLL)**, move checking and mutability checking. `setStrictBorrows(false)`
  turns borrow errors into warnings.
- **rustc's diagnostic format.** `error[E0308]: mismatched types`, the `-->` line, the code
  underlined with labels, `help:` and `note:` lines and suggestions. Results come both as
  structured data for an editor (`diagnostics`) and as rustc's text for a terminal (`rendered`).
- **Explanations for newcomers.** Many diagnostics carry a `help` field that explains the rule in
  JavaScript terms.
- **rustc's lints**, such as `unused_variables` and `dead_code`, honoring `#[allow]`, `#[deny]`
  and `#![deny(warnings)]`, plus warnings for un-awaited futures and for `loop`s with no `.await`
  that would hang a single-threaded page.

### Editor services

- `hover`, `completions`, `signatureHelp`, `definition`, `references` and `rename`, all working
  on code with syntax errors and across files, with 1-based lines and UTF-16 columns ready for
  Monaco.

### Programs and tooling

- **Multi-file programs.** `mod util;` and `#[path = ".."]` load files through a resolver you
  supply. Each file compiles to its own ES module. `dependencies` lists what a compile read, for
  caching.
- **Unit tests.** `#[test]` and `#[cfg(test)]` work as in Rust. `compileTests` is `rustc --test`
  and prints libtest's report, with name filters, `--exact`, `--ignored` and
  `--include-ignored`. Errors in test code are reported from normal builds as warnings
  (`setTestCodeChecks(false)` skips that check).
- **Proc macros.** `#[proc_macro]`, `#[proc_macro_derive]` and `#[proc_macro_attribute]` crates,
  with built-in stand-ins for `proc_macro2`, `quote` and the common parts of `syn`. They're
  compiled to JavaScript and run during the compile.
- **Bundled crates**, no `Cargo.toml` needed: `rand`, `rand_distr`, `itertools`, `anyhow`,
  `thiserror`, `bitflags`, `once_cell`, `lazy_static`, `either`, `cfg_if`, `hashbrown`, `serde`,
  `serde_json`, `regex`, `rayon` (single-threaded), `log` / `env_logger`, `num`, `tokio` (on the
  JS event loop), `chrono` and `clap`.

### JavaScript interop

- **The `js` crate.** Typed access to `document` and DOM elements (events take Rust closures),
  `window`, `localStorage`, `fetch`, timers and `sleep`, `Promise::all`, `JSON`, `Math`,
  `crypto`, `URL`, `console` and arbitrary globals.
- **Typed host APIs.** Describe the API object your app hands to user code as Rust declarations
  and load them with `setNsBindings(text)`. Calls into it are type-checked ("did you mean ...?",
  wrong arguments, a missing `.await`) and show docs on hover. `snake_case` calls compile to the
  JavaScript names.
- **`carciniscript/dts2rs`** generates those declarations from a TypeScript `.d.ts`.
- **`jsx!` and React.** Markup inside a `jsx!` macro compiles to React element calls, with the
  embedded expressions type-checked and borrow-checked as Rust.
- `JsValue`, `serde_wasm_bindgen::{to_value, from_value}`, and `Option` mapped to
  `undefined` / `null` at the boundary.

### Package

- Entry points: `carciniscript` (ES module for browsers, bundlers and Node, with async
  `initCarciniScript()`), `carciniscript/node` (CommonJS for Node and jest, ready on `require`),
  `carciniscript/runtime` (the runtime helpers) and `carciniscript/dts2rs` (needs TypeScript 5 or 6,
  an optional peer dependency).
- TypeScript declarations for every entry point, for both ES module and CommonJS consumers.

### Known limitations

- Types are erased at run time, so some trait dispatch on built-in types picks the impl by the
  value's JavaScript kind.
- `i64` / `u64` arithmetic past 2^53 is inexact (a warning flags it).
- No files, sockets or processes at run time. Raw pointer tricks, `transmute`, `union` and FFI
  are not supported.
- Threads run on the one JavaScript thread.

See [docs/FEATURES.md](docs/FEATURES.md) for the full support chart and
[docs/RUST_GAPS.md](docs/RUST_GAPS.md) for every known difference from rustc.
