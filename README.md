# CarciniScript

Compiles a practical subset of Rust to ES modules with v3 source maps, synchronously, in the
browser, at runtime. Built for Bitburner `.rs` scripts, following the shape of RapydScript-NS's
`.py` support.

```rust
pub async fn main(ns: NS) {
    let target = "n00dles";
    while ns.get_server_security_level(target) > 5.0 {
        ns.weaken(target).await;
    }
    let stolen = ns.hack(target).await;
    ns.tprint(&format!("stole ${:.2}", stolen));
}
```

becomes

```js
export async function main(ns) {
  const target = "n00dles";
  while (ns.getServerSecurityLevel(target) > 5.0) {
    await ns.weaken(target);
  }
  const stolen = await ns.hack(target);
  ns.tprint(`stole $${κσ.$fixed(stolen, 2)}`);
}
```

- [docs/PLAN.md](docs/PLAN.md): architecture, lowering decisions, open work
- [docs/FEATURES.md](docs/FEATURES.md): Rust feature support chart
- [docs/RUST_GAPS.md](docs/RUST_GAPS.md): where behavior differs from rustc

## Layout

| Path | What |
|---|---|
| `compiler/` | Rust crate: `ra_ap_syntax` parser → codegen → source map. Builds to wasm (~700 KB). |
| `runtime/rt.js` | Runtime helpers installed once on `globalThis.κσ`. |
| `index.js` | JS API: `initCarciniScript()`, then synchronous `compile(source, filename, resolve?)` (and `compileTests`, a test build). |
| `test/` | Differential fixtures (native rustc vs transpiled JS), NS mock scripts, source map checks. |
| `web-repl/` | Browser REPL and self-test. |
| `tools/build.mjs` | Builds the wasm and bindings into `pkg/`. |

## Building and testing

Needs Rust with the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` matching the crate's
`wasm-bindgen` version, and Node.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
npm test
```

## Web REPL

Try it online: **https://ficocelliguy.github.io/CarciniScript/**
(rebuilt and deployed by `.github/workflows/pages.yml` on every push to `main`).

To run it locally:

```sh
npm run build && npm run repl     # then open http://localhost:8765/web-repl/
```

Monaco editor with live diagnostics, generated JS, and click-to-highlight between Rust and JS via
the source map. Programs run in a Web Worker (Stop / 10 s watchdog for runaway loops) against a
mock `ns`; runtime-error stack traces are mapped back to Rust lines. **Run tests**
(Ctrl+Shift+Enter) compiles the editor's `#[test]` functions as a test build and shows libtest's
report.

**Self-test** (button in the REPL, or headless via `npm run test:browser`) verifies in the browser:
fixture output and panic messages match recorded native rustc output
(`test/fixtures/manifest.json`, refreshed with `npm run record-native`), the NS fixture, source
map positions, runtime-error mapping, and that arbitrary Rust never traps the wasm (the
compiler's own source, every prefix of every fixture, and random token soup).

`npm run sweep -- <dir>` runs the same crash check natively over any directory of `.rs` files
(`SWEEP_TRACE=1` prints each file first, to find one that takes the process down;
`SWEEP_SERVICE=1` also runs hover and completion at a few positions per file).

`node tools/diff-one.mjs file.rs [--js]` compiles one program with CarciniScript and with rustc,
runs both and shows where their output differs.

During development, `cargo run --example compile -- file.rs` (from `compiler/`) prints the
generated JS or diagnostics without building wasm.

## Usage

```js
import { initCarciniScript, compile, setNsBindings } from "carciniscript";
import "carciniscript/runtime"; // installs globalThis.κσ

await initCarciniScript(); // once, at startup
setNsBindings(nsBindingsText); // optional: typed NS (see below)
const { code, sourceMap, diagnostics, rendered } = compile(source, "hack.rs");
```

`rendered` is the diagnostics as rustc prints them (`error[E0308]: mismatched types`, the `-->`
line, the source with `^^^` under the span, `error: aborting due to 1 previous error`), for a
terminal; `diagnostics` has the same parts for an editor (`message`, `code`, `label`, `spans`,
`helps`, `notes`, `children`, and `help`, the rule in JavaScript terms for players new to Rust).
rustc's warnings come too (unused variables and `mut`s, dead code, unused imports, ..; `lint`
names each), with rustc's lint levels (`#[allow(dead_code)]`, `#![deny(warnings)]`); they never
stop a script from compiling unless denied. A panic that ends a script throws an error whose
`String(e)` and `e.stack` are rustc's panic report.

### Modules

`mod util;` loads `util.rs` from the same directory, and its `pub` items are typed and usable
(`use util::{helper, Target};` or `util::helper()`). The host supplies the files with a third
argument: `compile(source, "lib/main.rs", (path) => files.get(path))` (here it asks for
`lib/util.rs`). Each file compiles to its own ES module, importing its modules as
`import * as util from "./util.rs"`, so the host loads them the way it loads any import. Bitburner
passes the server's scripts. A module in another folder is loaded with rustc's `#[path]`: relative
to the file's directory (`#[path = "../lib/util.rs"] mod util;`) or from the root
(`#[path = "/lib/util.rs"]`, imported as `"/lib/util.rs"`, which Bitburner resolves from the
root), so scripts in `/hack/` and `/corp/` can share one `/lib/`.

### Testing your scripts

Tests are written as in Rust. `#[cfg(test)]` items and `#[test]` functions are test code: a
normal `compile` leaves them out, as `rustc` does without `--test`, so they cost no RAM and the
script runs as usual. Errors inside them are still reported, as warnings that say only a test
build compiles the code. In the editor, hover, completions, signature help and go to definition
inside test code answer from the test build, and references and rename include the uses in tests.

```rust
pub fn ways_to_sum(n: usize) -> u64 { /* .. */ }

pub async fn main(ns: &NS) {
    ns.tprint(&ways_to_sum(5).to_string());
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

`compileTests(source, filename, resolve?)` is `rustc --test`: `#[cfg(test)]` code is compiled, and
the module's `main(ns)` runs the file's `#[test]` functions and prints libtest's report (`test
tests::five ... ok`, then each failure's output and `test result:`) to the terminal with
`ns.tprintf`, or to stdout without an `ns`. Its `ns.args` are libtest's: name filters, `--exact`,
`--ignored`, `--include-ignored`. The script's own `main` isn't run.

A plain `compile` of a script under the top-level `tests/` folder is a test build too, as Cargo
builds `tests/*.rs`, so in Bitburner `run tests/solver.rs` prints the report. Such a script
reaches the code under test with `#[path = "/solver.rs"] mod solver;` (its `pub` items, as its
normal build has them), or runs that file's own unit tests with `include!("/solver.rs");`.

Without either, a script can run its own tests: list them with a macro and run each under
`catch_unwind`, which turns a failed assertion into an `Err` with the panic message
(`test/fixtures/assert_unwind_safe.rs` does this):

```rust
use std::panic::{self, AssertUnwindSafe};

macro_rules! tests {
    ($($name:ident $body:block)*) => {
        $(fn $name() $body)*
        const TESTS: &[(&str, fn())] = &[$((stringify!($name), $name)),*];
    };
}

tests! {
    five { assert_eq!(ways_to_sum(5), 6); }
    zero { assert_eq!(ways_to_sum(0), 1); }
}

pub async fn main(ns: &NS) {
    let mut failed = 0;
    for (name, test) in TESTS {
        match panic::catch_unwind(AssertUnwindSafe(test)) {
            Ok(()) => ns.tprint(&format!("test {name} ... ok")),
            Err(e) => {
                failed += 1;
                let msg = e.downcast_ref::<String>().cloned().unwrap_or_default();
                ns.tprint(&format!("test {name} ... FAILED: {msg}"));
            }
        }
    }
    ns.tprint(&format!("{} passed; {failed} failed", TESTS.len() - failed));
}
```

### Proc macros

A file with `#[proc_macro]`, `#[proc_macro_derive(Name)]` or `#[proc_macro_attribute]` functions
is a proc-macro crate, used as in Rust: `use mac::{show, Show};` then `show!(..)` or
`#[derive(Show)]` (or `mac::show!(..)`, `#[mac::traced]`), with `mac.rs` read through `resolve`
like a module file. Read the input with `input.to_string()`; return
`format!(..).parse().unwrap()`, build it with `quote!` (and `format_ident!`), walk its token
trees (`for tt in input`), or parse it with `syn` (`parse_macro_input!(input as DeriveInput)`, or
your own `impl Parse` on a `ParseStream` with `Token![..]`):
built-in stand-ins for `proc_macro2`, `quote` and the common parts of `syn`. The crate is compiled to JS and run during the compile by the
runtime (`carciniscript/runtime` must be loaded); a panic, or a loop running past 10,000,000
steps, is an error at the invocation. `compile` returns the crates a file used in
`dependencies`, so a host that caches compiled scripts can recompile when one changes (Bitburner
does).

### Bundled crates

A few popular crates are built in and need no `Cargo.toml`: `rand` (`rng.gen_range(1..=6)`,
`v.shuffle(&mut rng)`), `itertools` (`iter.sorted().dedup().join(", ")`, `combinations`,
`tuple_windows`, `counts`, ..), `anyhow` (`Result`, `bail!`, `.context(..)`), `thiserror`
(`#[derive(Error)]`), `bitflags!`, `once_cell` / `lazy_static!` (and std's `LazyLock` /
`OnceLock`), `either`, `cfg_if!`, `hashbrown`, `serde` + `serde_json`
(`serde_json::to_string(&state)?`, `from_str::<Config>(&text)?`, `json!`, `Value`), `regex`
(`Regex::new(r"(\d+)GB")`, `captures`, `replace_all`) and `rayon` (`par_iter()`, run on one
thread), `log` (`info!`, with `env_logger`), `num` (`Zero`, `gcd`, ..) and `tokio`
(`tokio::spawn`, `sleep`, `join!`, on the script's event loop), `rand_distr` (`Normal`,
`WeightedIndex`, ..), `chrono` (dates, times, time zones, `format`) and `clap` (`#[derive(Parser)]`
and the builder, with clap's help and errors). `extern crate x;` works for
these. See [FEATURES.md](docs/FEATURES.md) for what each
covers and [RUST_GAPS.md](docs/RUST_GAPS.md) for what differs (a seeded RNG's 64-bit values, for one).

### JS globals

The built-in `js` crate reaches JS globals with Rust types: `use js::{Date, JSON};`, then
`Date::now()`, `Date::from_millis(ms).to_iso_string()`, `JSON::stringify(&state)` and
`JSON::parse::<State>(&text)` (serde_json's format and error messages, going by the type;
`#[derive(Serialize, Deserialize)]` and `#[serde(rename_all = "camelCase")]` are understood).
`JsonError` and `Date` print with `{}` and `{:?}`.

The rest of the browser comes the same way (declared in `bindings/js.rs`): `js::document()`
(`query_selector`, `create_element`, `Element` fields and methods, events with Rust closures),
`js::window()`, `js::local_storage()`, `js::location()`, `js::navigator()` (clipboard),
`js::console::log(..)`, `js::Math::random()`, `js::performance::now()`, `js::crypto`,
`js::Url::parse(..)`, `js::fetch(url).await`, `js::btoa` / `atob`, timers
(`js::set_timeout(|| .., ms)`, `js::sleep(ms).await`), `js::Promise::all(futures).await` and
`js::global(name)` / `set_global`, `js::structured_clone(&x)`. Functions can be imported by name
(`use js::{btoa, sleep};`). `std::time::{Duration, Instant, SystemTime}` work as in Rust
(`let t = Instant::now(); .. println!("{:.2?}", t.elapsed());`), on `performance.now()`.
In Bitburner, `js::document()` and `js::window()` cost 25 GB like their JS names.

### Editor support

`hover(source, filename, line, column, resolve?)` returns markdown for the symbol under the cursor
(types, function signatures, NS docs with RAM costs), and `completions(...)` lists the members of
the value before a `.` (or the names in scope for a plain identifier), and `signatureHelp(...)`
gives parameter hints inside a call. `setOverflowChecks(true)` makes the compiles that follow panic on integer overflow, like a Rust debug
build (the default wraps, like a release build). Borrow conflicts rustc rejects are errors, as in rustc;
`setStrictBorrows(false)` makes them warnings (the program still runs). Errors in test code
(`#[test]`, `#[cfg(test)]`) come from a second, test build of the file, as warnings;
`setTestCodeChecks(false)` skips that build.

`definition(...)` finds where the name under the cursor is
defined (locals by scope; functions, types, methods, fields, variants and aliases; items of
`mod x;` files through `resolve`), `references(..., others)` lists its uses in the file and in
the `others` files, and `rename(..., newName, ..., others)` the ranges to replace. They work on code with syntax errors.
Bitburner registers them as Monaco hover, completion, signature help, definition, reference and
rename providers for `.rs` files; a definition in another script opens it in a tab.

### Typed NS bindings

`tools/dts2rs.mjs` turns Bitburner's `NetscriptDefinitions.d.ts` into Rust declarations
(`bindings/ns.rs`), which the compiler loads with `setNsBindings(text)`. With them, every `ns.*`
call, field and namespace is type-checked: wrong argument types, misspelled methods ("did you
mean"), and a forgotten `.await` are compile errors. The interfaces are Rust structs a script can
name and build (`RunOptions { threads: Some(4), ..Default::default() }`), a `Player` is accepted
where a `Person` is, the d.ts's unions are enums over the JS value itself (`ns.args` is a
`Vec<ScriptArg>`: `match a { ScriptArg::Str(s) => .., ScriptArg::Num(n) => .., ScriptArg::Bool(b) => .. }`,
or `a.as_string()` as before; `PlayerRequirement::Money(m)`, `NsResult::Success(..)`), and only
`any` is wasm-bindgen's `JsValue` (`serde_wasm_bindgen::from_value(data)`).
`ns.flags` takes typed pairs (`ns.flags(&[("target", "n00dles"), ("threads", 1), ("exclude", vec![])])`)
and returns `Flags` (`f.get_str("target")`, `get_f64`, `get_bool`, `get_list`, `f.rest`); for typed
fields, `--help` and error messages, use clap's `#[derive(Parser)]` and `Args::parse()`, which read
the script's arguments.
Regenerate after NS API changes:

```sh
npm run bindings   # reads ../bitoxidizer-src/src/ScriptEditor/NetscriptDefinitions.d.ts
```

`code` and `sourceMap` are `null` when there are errors. Diagnostics carry byte offsets plus
1-based line and UTF-16 column, ready for Monaco markers. Some carry a `help` line: the
rule behind the message in JavaScript terms and what to try, for players who don't know Rust;
show it under the message.
