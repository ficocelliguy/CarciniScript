// Build a Rust program natively, for comparing its output with the JS. A program that uses the
// bundled crates (`rand`, `itertools`, `anyhow`, ..) builds with cargo against the real ones (from
// the cargo cache, offline) in a project kept between runs; anything else with plain rustc.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

// The bundled crates, with the versions the native builds use. (`hashbrown`'s dependencies aren't
// in the cache and `lazy_static` isn't either: programs using them can't be compared.)
const VERSIONS = {
  rand: "0.8",
  itertools: "0.15",
  anyhow: "1",
  thiserror: "2",
  bitflags: "2",
  once_cell: "1",
  either: "1",
  cfg_if: "1",
  rayon: "1",
  serde: "1",
  serde_json: "1",
  regex: "1",
  log: "0.4",
  env_logger: "0.11",
  num_traits: "0.2",
  rand_distr: "0.4",
  chrono: "0.4",
  chrono_tz: "0.10",
  clap: "4",
};
const CRATES = Object.keys(VERSIONS).concat(["lazy_static", "hashbrown"]).join("|");
const uses = (src, c) => new RegExp(`\\b${c}(::|!)|\\buse ${c}\\b`).test(src);
export const usesCrates = (src) => new RegExp(`\\b(${CRATES})(::|!)|\\buse (${CRATES})\\b`).test(src);

/** Compile `file` to the executable `exe` (`debug`: without optimizations). Throws on errors. */
export function buildNative(file, exe, { debug = false } = {}) {
  const src = fs.readFileSync(file, "utf8");
  if (!usesCrates(src)) {
    const args = [...(debug ? [] : ["-O"]), "--edition", "2024", "-A", "warnings", "-o", exe, file];
    execFileSync("rustc", args, { stdio: "pipe" });
    return;
  }
  const proj = path.join(os.tmpdir(), "crs-crates");
  fs.mkdirSync(path.join(proj, "src"), { recursive: true });
  const deps = Object.entries(VERSIONS)
    .filter(([c]) => uses(src, c))
    .map(([c, v]) =>
      c === "serde" || c === "clap"
        ? `${c} = { version = "${v}", features = ["derive"] }`
        : `${c === "cfg_if" ? "cfg-if" : c === "chrono_tz" ? "chrono-tz" : c === "num_traits" ? "num-traits" : c === "env_logger" ? "env_logger" : c} = "${v}"`,
    );
  const toml = ["[package]", 'name = "prog"', 'version = "0.1.0"', 'edition = "2024"', "", "[dependencies]", ...deps, ""];
  fs.writeFileSync(path.join(proj, "Cargo.toml"), toml.join("\n"));
  fs.writeFileSync(path.join(proj, "src", "main.rs"), src);
  execFileSync("cargo", ["build", "--offline", "-q", ...(debug ? [] : ["--release"])], {
    cwd: proj,
    stdio: "pipe",
    env: { ...process.env, RUSTFLAGS: "-A warnings" },
  });
  const built = path.join(proj, "target", debug ? "debug" : "release", process.platform === "win32" ? "prog.exe" : "prog");
  fs.copyFileSync(built, exe);
}
