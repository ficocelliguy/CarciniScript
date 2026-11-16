# Bitburner corpus

Bitburner scripts written as idiomatic Rust, not trimmed to what CarciniScript supports today. They
use the typed NS bindings (`bindings/ns.rs`) as a Rust programmer would expect them to work, and
no threads. Modeled on the scripts in `BitburnerScripts` and on bitburner-src's darknet code.
[GAPS.md](GAPS.md) lists what CarciniScript is missing to compile and run them.

| Script | What it does | Leans on |
|---|---|---|
| `net.rs` | Shared module: BFS crawler, rooting, RAM pool, top-k | `Deref` newtype, `impl Iterator` with a lifetime, `BinaryHeap` + `Reverse` + hand-written `Ord`, `#[macro_export]` |
| `batcher.rs` + `hwgw.rs` | HWGW batcher: target scoring, prep, formulas-based thread math, timed launches, port reports | `Index<Step>`, thiserror with `#[source]`, `TryFrom<&[JsValue]>`, `Duration` math, bisection over a closure, `Rc<RefCell>` + `at_exit`, labeled loops, serde |
| `darknet.rs` + `dn_models.rs` + `dn_tasks.rs` | Darknet crawler: cracks all 25 server models, shares a password vault, spreads itself, runs chores | `FromStr` model enum, generic `async fn`, nested `async fn` items, `let`-`else`, iterative maze DFS, recursive-descent parser, `i128` CRT, Gaussian peak inversion, `include_str!` word list (`common_passwords.txt`), itertools, `LazyLock<Regex>`, `#[cfg(test)]` tests |
| `contracts.rs` | Coding contract solver for 23 contract types | `fn` pointer table, `#[serde(untagged)]`, DP, BFS, `scan`/`try_fold`/`chunk_by` |
| `hacknet.rs` | Hacknet upgrades by payback time, hash spending | enum with data, `min_by`, `clamp` |
| `servers.rs` | Buys and upgrades cloud servers | anyhow, `successors`, slice patterns |
| `stocks.rs` | Stock trader with or without 4S data | const generics, `Into<f64>` bound, `Display` table |
| `go.rs` | IPvGO player | `Not`, `Index<Point>`, `TryFrom<char>`, flood fill |
| `deploy.rs` + `early_hack.rs` | Roots everything and fills it with the early-game loop | `Box<dyn Fn>` strategy map, ports |
| `cat.rs` | A cat that walks along the bottom of the screen in its tail window (port of `cat.jsx`) | `jsx!`, `use_state` / `use_effect` / `use_ref`, a closure component capturing `ns`, `Box<dyn Fn()>` in a ref, raw-string art |
| `monitor.rs` | Live tail-window table of rooted servers: money bars, security, hack chance, sort and filter buttons, refreshed every second | `jsx!` components with props (`<Meter fraction={..} />`, `<Line row={..} />`), `use_state` / `use_effect` with an interval calling `ns`, keyed list from an iterator |
| `dashboard.rs` | Tail-window stats from a port | `TryFrom<&'a str>` borrowed parse, `fmt::Write`, `Instant` |

Each top-level script is a Bitburner script (`pub async fn main(ns: NS)`). `mod x;` loads a
sibling as a shared module, so `batcher.rs` and `hwgw.rs` share the `Op` and `Report` types.
