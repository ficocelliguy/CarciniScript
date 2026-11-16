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

- [docs/PLAN.md](docs/PLAN.md): architecture, lowering decisions, phases
- [docs/FEATURES.md](docs/FEATURES.md): Rust feature support chart
- [docs/RUST_GAPS.md](docs/RUST_GAPS.md): where behavior differs from rustc

## Layout

| Path | What |
|---|---|
| `compiler/` | Rust crate: `ra_ap_syntax` parser → codegen → source map. Builds to wasm (~700 KB). |
| `runtime/rt.js` | Runtime helpers installed once on `globalThis.κσ`. |
| `index.js` | JS API: `initCarciniScript()`, then synchronous `compile(source, filename, resolve?)`. |
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
mock `ns`; runtime-error stack traces are mapped back to Rust lines.

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
const { code, sourceMap, diagnostics } = compile(source, "hack.rs");
```

### Modules

`mod util;` loads `util.rs` from the same directory, and its `pub` items are typed and usable
(`use util::{helper, Target};` or `util::helper()`). The host supplies the files with a third
argument: `compile(source, "lib/main.rs", (path) => files.get(path))` (here it asks for
`lib/util.rs`). Each file compiles to its own ES module, importing its modules as
`import * as util from "./util.rs"`, so the host loads them the way it loads any import. Bitburner
passes the server's scripts.

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
(`tokio::spawn`, `sleep`, `join!`, on the script's event loop). `extern crate x;` works for
these. See [FEATURES.md](docs/FEATURES.md) for what each
covers and [RUST_GAPS.md](docs/RUST_GAPS.md) for what differs (rand's seeded sequences, for one).

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
build (the default wraps, like a release build). Borrow conflicts rustc rejects are warnings;
`setStrictBorrows(true)` makes them errors.

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
mean"), and a forgotten `.await` are compile errors. Regenerate after NS API changes:

```sh
npm run bindings   # reads ../bitoxidizer-src/src/ScriptEditor/NetscriptDefinitions.d.ts
```

`code` and `sourceMap` are `null` when there are errors. Diagnostics carry byte offsets plus
1-based line and UTF-16 column, ready for Monaco markers.
