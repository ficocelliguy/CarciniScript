<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/logo-dark.png">
    <source media="(prefers-color-scheme: light)" srcset="docs/logo-light.png">
    <img alt="CarciniScript: because everything becomes crab" src="docs/logo-light.png" width="600">
  </picture>
</p>

# CarciniScript

> *Carcinisation*: the tendency of evolution to keep producing crabs. Every language eventually becomes Rust. 🦀

CarciniScript compiles **Rust to JavaScript, in the browser, at runtime**, with source maps back
to the original Rust.

The compiler is a WebAssembly module. Hand it a `.rs` file and it
returns an ES module plus a v3 source map, synchronously, in milliseconds. There is no Rust
toolchain or build step involved, so you can add Rust as a language to any web app: a
playground, a scripting or plugin system, a game whose players write code, or a teaching tool.
Because every compiled module carries a source map, the browser's debugger, stack traces and
panic messages all point at Rust lines, not generated JavaScript.

**Try it online: https://ficocelliguy.github.io/CarciniScript/**

```sh
npm install carciniscript
```

See [CHANGELOG.md](CHANGELOG.md) for what each release adds.

## What it looks like

You write this:

```rust
struct Item {
    name: &'static str,
    price: f64,
}

fn main() {
    let cart = vec![
        Item { name: "apple", price: 0.5 },
        Item { name: "bread", price: 2.25 },
    ];
    let total: f64 = cart.iter().map(|i| i.price).sum();
    for item in &cart {
        println!("{:>8}: ${:.2}", item.name, item.price);
    }
    println!("total: ${:.2}", total);
}
```

and the browser runs this:

```js
export class Item {
  constructor($0, $1) {
    this.name = $0;
    this.price = $1;
  }
}

export function main() {
  const cart = [new Item("apple", 0.5), new Item("bread", 2.25)];
  const total = κσ.$iter(cart).$map((i) => i.price).$sum((a, b) => a + b, -0);
  for (const item of cart) {
    κσ.$println(`${κσ.$pad(item.name, 8, ">", " ", false, false)}: $${κσ.$fixed(item.price, 2)}`);
  }
  κσ.$println(`total: $${κσ.$fixed(total, 2)}`);
}
```

The output stays close to hand-written JavaScript: structs become classes, loops stay loops,
`async fn` becomes `async function`, and Rust's formatting and iterator methods go through a
small runtime (`κσ`).

## Features

- **In-browser compilation with source maps.** Compile at runtime, with no server and no
  toolchain. Every module comes with a v3 source map, so breakpoints, stepping and stack traces
  in the browser's devtools show your Rust code, and a runtime error or panic points at the Rust
  line that caused it.
- **Real Rust, checked like Rust.** Structs, enums, traits, generics, pattern matching,
  closures, iterators, `Result` and `?`, macros, async/await, _and the borrow checker_. Programs
  print the same output and panic with the same messages as when built with `rustc`.
- **Proc macros.** You can write your own `#[derive(..)]` and function-like macros, with
  stand-ins for `syn` and `quote`.
- **Error messages like rustc's.** Compile errors and warnings look like the ones `rustc` prints
  (`error[E0308]: mismatched types`, with the code underlined). Many also come with a short
  explanation in JavaScript terms, for people who are new to Rust.
- **Editor support.** A language service with hover for types and docs,
  autocomplete, parameter hints, go to definition, find references, and rename, all working on
  code with syntax errors, ready to plug into Monaco or a similar editor.
- **Typed host APIs.** Describe the JavaScript API your app exposes to user code as Rust
  declarations (or generate them from a TypeScript `.d.ts`), and calls into it are type-checked:
  a misspelled method ("did you mean ...?"), a wrong argument, or a forgotten `.await` is caught
  before the code runs.
- **Browser APIs.** The built-in `js` crate gives typed access to `document`, `localStorage`,
  `fetch`, timers, `JSON` and other JavaScript globals.
- **Multi-file programs.** `mod util;` loads `util.rs` through a resolver you supply, and each
  file compiles to its own ES module.
- **Unit tests.** `#[test]` functions work as in Rust and print the familiar `cargo test`
  report. They are left out of normal builds.
- **Popular crates built in.** `rand`, `itertools`, `serde` / `serde_json`, `regex`, `anyhow`,
  `thiserror`, `chrono`, `clap`, `tokio` and more work without a `Cargo.toml`.
- **Web REPL.** The [online playground](https://ficocelliguy.github.io/CarciniScript/) has a
  full code editor with live errors, the generated JavaScript side by side (click a line in one
  to highlight the matching line in the other), a Run button, and a way to run `#[test]`
  functions.

CarciniScript covers most of the Rust language and much of the standard library, enough to
compile a wide range of real-world Rust code. See [docs/FEATURES.md](docs/FEATURES.md) for the
full support chart and [docs/RUST_GAPS.md](docs/RUST_GAPS.md) for the remaining places where
behavior differs from real Rust.

## Writing programs

### Using more than one file

`mod util;` loads `util.rs` from the same folder, and its `pub` items can be used as in Rust
(`use util::{helper, Target};` or `util::helper()`). To load a file from another folder, use
rustc's `#[path]` attribute, either relative to the current file
(`#[path = "../lib/util.rs"] mod util;`) or from the root (`#[path = "/lib/util.rs"]`), so
programs in different folders can share one `/lib/`.

### Testing

Tests are written as in Rust:

```rust
pub fn ways_to_sum(n: usize) -> u64 { /* .. */ }

fn main() {
    println!("{}", ways_to_sum(5));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five() {
        assert_eq!(ways_to_sum(5), 6);
    }
}
```

A normal build leaves out `#[cfg(test)]` code, as `rustc` does. Errors inside test code are still
reported, as warnings.

A test build (`compileTests`, below) is `rustc --test`: its `main` runs the `#[test]` functions
and prints libtest's report. A test file can reach the code under test with
`#[path = "/solver.rs"] mod solver;`, or run that file's own unit tests with
`include!("/solver.rs");`. The usual libtest arguments work: name filters, `--exact`,
`--ignored`, `--include-ignored`.

### Bundled crates

These crates are built in and need no `Cargo.toml` (`extern crate x;` also works):

`rand`, `rand_distr`, `itertools`, `anyhow`, `thiserror`, `bitflags`, `once_cell`,
`lazy_static`, `either`, `cfg_if`, `hashbrown`, `serde`, `serde_json`, `regex`, `rayon` (runs
on one thread), `log` / `env_logger`, `num`, `tokio` (runs on the JavaScript event loop),
`chrono` and `clap`.

See [FEATURES.md](docs/FEATURES.md) for what each covers and [RUST_GAPS.md](docs/RUST_GAPS.md)
for what differs.

### JavaScript and browser APIs

The built-in `js` crate reaches JavaScript globals with Rust types:

```rust
use js::{Date, JSON};

let now = Date::now();
let text = JSON::stringify(&state);
let state: State = JSON::parse::<State>(&text)?;
```

Also available (declared in `bindings/js.rs`): `js::document()` (with `query_selector`,
`create_element`, element fields and methods, and events with Rust closures), `js::window()`,
`js::local_storage()`, `js::location()`, `js::navigator()`, `js::console::log(..)`,
`js::Math`, `js::performance::now()`, `js::crypto`, `js::Url::parse(..)`,
`js::fetch(url).await`, `js::btoa` / `atob`, timers (`js::set_timeout(|| .., ms)`,
`js::sleep(ms).await`), `js::Promise::all(futures).await`, `js::global(name)` / `set_global`
and `js::structured_clone(&x)`. `std::time::{Duration, Instant, SystemTime}` work as in Rust.

### Typed host APIs

A host app usually hands user code an API object, passed to `main`:

```rust
pub async fn main(api: &NS) {
    let n = api.fetch_count("users").await;
    api.print(&format!("{n} users"));
}
```

The host describes that object with Rust declarations: interfaces are `pub struct`s and `impl`
blocks with bodiless methods, each member carries its JavaScript name in `#[js = "..."]`, and
TypeScript unions become Rust enums whose value is the JS value itself. Rust's `snake_case`
calls compile to the JavaScript names (`api.fetchCount("users")`). Load the declarations with
`setNsBindings(text)` (below), and every call, field and namespace on the object is type-checked,
with its doc comments shown on hover. `tools/dts2rs.mjs` generates them from a TypeScript `.d.ts`;
`bindings/ns.rs` is a large generated example.

### Proc macros

A file with `#[proc_macro]`, `#[proc_macro_derive(Name)]` or `#[proc_macro_attribute]` functions
is a proc-macro crate, used as in Rust: `use mac::{show, Show};`, then `show!(..)` or
`#[derive(Show)]`. Build the output with `format!(..).parse().unwrap()`, `quote!` and
`format_ident!`, walk the token trees, or parse the input with `syn`
(`parse_macro_input!(input as DeriveInput)`, or your own `impl Parse`). These are built-in
stand-ins for `proc_macro2`, `quote` and the common parts of `syn`. The macro crate is compiled
to JavaScript and run during the compile. A panic, or a loop running past 10,000,000 steps, is an
error at the macro call.

---

## For developers

The rest of this file is for people embedding CarciniScript in an app or working on the
compiler itself.

### How it works

The compiler is written in Rust (using rust-analyzer's parser) and built to WebAssembly. It compiles
synchronously, at runtime, to ES modules with v3 source maps. The generated code calls a small set 
of runtime helpers that are installed once on `globalThis.κσ`.

| Path | What |
|---|---|
| `compiler/` | Rust crate: `ra_ap_syntax` parser → codegen → source map. Builds to wasm. |
| `runtime/rt.js` | Runtime helpers installed once on `globalThis.κσ`. |
| `index.js` | JS API (below). |
| `bindings/` | Rust declarations of browser globals (`js.rs`) and an example generated host API (`ns.rs`). |
| `test/` | Differential fixtures (native rustc vs transpiled JS), host API mock scripts, source map checks. |
| `web-repl/` | Browser REPL and self-test. |
| `tools/` | Build script, binding generator, and debugging tools. |

More detail: [docs/PLAN.md](docs/PLAN.md) (architecture, lowering decisions, open work),
[docs/FEATURES.md](docs/FEATURES.md), [docs/RUST_GAPS.md](docs/RUST_GAPS.md).

### JavaScript API

```js
import { initCarciniScript, compile, setNsBindings } from "carciniscript";
import "carciniscript/runtime"; // installs globalThis.κσ

await initCarciniScript(); // once, at startup
setNsBindings(hostApiBindingsText); // optional: typed host API
const { code, sourceMap, diagnostics, rendered, dependencies } =
  compile(source, "main.rs", (path) => files.get(path));
```

The same code works in Node, where `initCarciniScript()` reads the wasm from disk. For CommonJS
(or jest), `require("carciniscript/node")` gives the same API with the wasm already loaded (the
`init` functions do nothing). The package's entry points are `carciniscript` (browsers, bundlers
and Node), `carciniscript/node` (CommonJS), `carciniscript/runtime` and `carciniscript/dts2rs`,
all typed. `carciniscript/dts2rs` (`dtsToRust(text)`) needs TypeScript 5 or 6 installed;
TypeScript 7 has no JS API.

- `code` and `sourceMap` are `null` when there are errors. To get Rust in devtools and stack
  traces, append the map to the module (`//# sourceMappingURL=data:...`) before importing it
  from a `Blob` URL.
- `rendered` is the diagnostics as rustc prints them, for a terminal. `diagnostics` has the same
  parts for an editor (`message`, `code`, `label`, `spans`, `helps`, `notes`, `children`, and
  `help`, the rule in JavaScript terms). Spans carry byte offsets plus 1-based line and UTF-16
  column, ready for Monaco markers. Warnings carry their `lint` name and follow rustc's lint
  levels (`#[allow(dead_code)]`, `#![deny(warnings)]`).
- The third argument resolves `mod x;` files and proc-macro crates. Each file compiles to its
  own ES module, importing its modules as `import * as util from "./util.rs"`.
- `dependencies` lists the files a program used, so a host that caches compiled modules knows
  when to recompile.
- A panic that ends a program throws an error whose `String(e)` and `e.stack` are rustc's panic
  report.

`compileTests(source, filename, resolve?)` is `rustc --test`: the module's `main` runs the
file's `#[test]` functions and prints libtest's report to stdout, or with the host API's
`tprintf` when `main` is passed one.

Editor services, which all take `(source, filename, line, column, resolve?)` and work on code with
syntax errors:

- `hover` returns markdown for the symbol under the cursor.
- `completions` lists members after a `.`, or the names in scope.
- `signatureHelp` gives parameter hints inside a call.
- `definition` finds where a name is defined, including in other files through `resolve`.
- `references(..., others)` and `rename(..., newName, ..., others)` cover the file and the
  `others` files.

Settings:

- `setOverflowChecks(true)` panics on integer overflow, like a debug build (the default wraps,
  like a release build).
- `setStrictBorrows(false)` turns borrow-check errors into warnings.
- `setTestCodeChecks(false)` skips the second, test build that reports errors in test code.

### Generating host API bindings

`tools/dts2rs.mjs` turns a TypeScript declaration file into Rust bindings that the compiler loads
with `setNsBindings(text)`:

```sh
node tools/dts2rs.mjs path/to/definitions.d.ts bindings/ns.rs
```

Interfaces become structs and `impl` blocks, unions become enums, `Promise<T>` returns become
`async fn`, and optional and rest parameters become `#[optional]` and `#[variadic]`. The header
of `tools/dts2rs.mjs` lists the full mapping.

### Building and testing

Needs Rust with the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` matching the crate's
`wasm-bindgen` version, and Node.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
npm test
```

Run the web REPL locally (it is also rebuilt and deployed by `.github/workflows/pages.yml` on
every push to `main`):

```sh
npm run build && npm run repl     # then open http://localhost:8765/web-repl/
```

Other tools:

- **Self-test** (button in the REPL, or headless via `npm run test:browser`) checks, in the
  browser, that fixture output and panic messages match recorded native rustc output
  (`test/fixtures/manifest.json`, refreshed with `npm run record-native`), plus the host API
  fixture, source map positions, runtime-error mapping, and that arbitrary input never crashes
  the wasm.
- `npm run sweep -- <dir>` runs the same crash check natively over a directory of `.rs` files
  (`SWEEP_TRACE=1` prints each file first; `SWEEP_SERVICE=1` also runs hover and completion).
- `node tools/diff-one.mjs file.rs [--js]` compiles one program with CarciniScript and with
  rustc, runs both and shows where their output differs.
- `cargo run --example compile -- file.rs` (from `compiler/`) prints the generated JS or
  diagnostics without building wasm.

## License

MIT. See [LICENSE.md](LICENSE.md).
