// Build a Rust program natively, for comparing its output with the JS. A program that uses the
// bundled crates (`rand`, `itertools`, `anyhow`, ..) builds with cargo against the real ones (from
// the cargo cache, offline) in a project kept between runs; anything else with plain rustc.
import { execFileSync, spawn } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
  rustc_hash: "2",
  smallvec: "1",
  indexmap: "2",
};
const CRATES = Object.keys(VERSIONS).concat(["lazy_static", "hashbrown"]).join("|");
const uses = (src, c) => new RegExp(`\\b${c}(::|!)|\\b(use|extern crate) ${c}\\b`).test(src);
export const usesCrates = (src) => new RegExp(`\\b(${CRATES})(::|!)|\\b(use|extern crate) (${CRATES})\\b`).test(src);

// How to build `file` into `exe`: the command, and for a program using the bundled crates the
// cargo project to write first and the binary to copy out after.
function plan(file, exe, { debug = false, test = false } = {}) {
  const src = fs.readFileSync(file, "utf8");
  // A `// edition: 2021` line builds the program as that edition (e.g. for `rng.gen()`).
  const edition = /^\/\/ edition: (\d{4})\r?$/m.exec(src)?.[1] ?? "2024";
  // (`#![feature(allocator_api)]`: unstable std APIs, on the stable toolchain.)
  const env = /^#!\[feature\(/m.test(src) ? { ...process.env, RUSTC_BOOTSTRAP: "1" } : process.env;
  if (test || !usesCrates(src)) {
    const flags = test ? ["--test"] : debug ? [] : ["-O"];
    return { cmd: "rustc", args: [...flags, "--edition", edition, "-A", "warnings", "-o", exe, file], opts: { env } };
  }
  const proj = path.join(os.tmpdir(), "crs-crates");
  const deps = Object.entries(VERSIONS)
    .filter(([c]) => uses(src, c))
    .map(([c, v]) =>
      c === "serde" || c === "clap"
        ? `${c} = { version = "${v}", features = ["derive"] }`
        : c === "rand"
        ? `rand = { version = "${v}", features = ["small_rng"] }`
        : `${c === "cfg_if" ? "cfg-if" : c === "chrono_tz" ? "chrono-tz" : c === "num_traits" ? "num-traits" : c === "rustc_hash" ? "rustc-hash" : c === "env_logger" ? "env_logger" : c} = "${v}"`,
    );
  const toml = ["[package]", 'name = "prog"', 'version = "0.1.0"', `edition = "${edition}"`, "", "[dependencies]", ...deps, ""];
  const built = path.join(proj, "target", debug ? "debug" : "release", process.platform === "win32" ? "prog.exe" : "prog");
  return {
    cargo: true,
    cmd: "cargo",
    args: ["build", "--offline", "-q", ...(debug ? [] : ["--release"])],
    opts: { cwd: proj, env: { ...process.env, RUSTFLAGS: "-A warnings" } },
    before() {
      fs.mkdirSync(path.join(proj, "src"), { recursive: true });
      fs.writeFileSync(path.join(proj, "Cargo.toml"), toml.join("\n"));
      fs.writeFileSync(path.join(proj, "src", "main.rs"), src);
    },
    after: () => fs.copyFileSync(built, exe),
  };
}

/** Compile `file` to the executable `exe` (`debug`: without optimizations). Throws on errors. */
export function buildNative(file, exe, { debug = false } = {}) {
  const p = plan(file, exe, { debug });
  p.before?.();
  execFileSync(p.cmd, p.args, { stdio: "pipe", ...p.opts });
  p.after?.();
}

// --- cached, parallel builds for the test suites. A native program's output only changes with
// its source, the flags and the toolchain, so each run is kept in test/out/native-cache under a
// hash of those and rebuilt only when one changes.
export const cacheDir = path.join(path.dirname(path.dirname(fileURLToPath(import.meta.url))), "test", "out", "native-cache");
let toolchain;
const hash = (parts) => {
  toolchain ??= execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  return crypto.createHash("sha256").update(JSON.stringify([toolchain, ...parts])).digest("hex").slice(0, 32);
};

/** Every file under `dir`, by its path from there, with its contents: for a key. */
export function dirContents(dir) {
  return fs
    .readdirSync(dir, { recursive: true })
    .map((f) => f.replaceAll("\\", "/"))
    .sort()
    .filter((f) => fs.statSync(path.join(dir, f)).isFile())
    .map((f) => [f, fs.readFileSync(path.join(dir, f), "utf8")]);
}

/** A child process's `{ stdout, stderr, status }`, as `spawnSync` gives them (stdin closed). */
export function run(cmd, args = [], opts = {}) {
  return new Promise((resolve) => {
    const c = spawn(cmd, args, opts);
    let stdout = "";
    let stderr = "";
    c.stdout.setEncoding("utf8").on("data", (d) => (stdout += d));
    c.stderr.setEncoding("utf8").on("data", (d) => (stderr += d));
    c.stdin.end();
    c.on("error", (e) => resolve({ stdout, stderr: stderr + e.message, status: null }));
    c.on("close", (status) => resolve({ stdout, stderr, status }));
  });
}

// How many builds run at once: `NATIVE_JOBS`, else half the cores (rustc and cargo are
// multi-threaded themselves, and the machine is shared).
const limit = Number(process.env.NATIVE_JOBS) || Math.max(2, Math.floor(os.availableParallelism() / 2));
let active = 0;
const waiting = [];
async function slot(fn) {
  if (active >= limit) await new Promise((r) => waiting.push(r));
  active++;
  try {
    return await fn();
  } finally {
    active--;
    waiting.shift()?.();
  }
}
// The bundled-crate builds share one cargo project, so they take turns.
let cargoTurn = Promise.resolve();
const inCargoTurn = (fn) => (cargoTurn = cargoTurn.then(fn, fn));

/**
 * `compute()`'s value, kept under a hash of `keyParts` and the toolchain; run in the build pool
 * when it isn't cached. A value with an `error` isn't kept.
 */
export function cachedNative(keyParts, compute) {
  const file = path.join(cacheDir, `${hash(keyParts)}.json`);
  try {
    return Promise.resolve(JSON.parse(fs.readFileSync(file, "utf8")));
  } catch {}
  return slot(compute).then((value) => {
    if (!value.error) {
      fs.mkdirSync(cacheDir, { recursive: true });
      fs.writeFileSync(file, JSON.stringify(value));
    }
    return value;
  });
}

/**
 * Builds and runs each job's program natively, at most `NATIVE_JOBS` at once, and gives one
 * promise per job of its raw `{ stdout, stderr, status }`, or `{ error }` with the build's
 * message (never a rejection: the promises are awaited one at a time, in order).
 * A job: `file`; `debug` (no `-O`); `test` (`rustc --test`, run with `--test-threads=1`);
 * `args` for the program; `dir`, a folder whose every file is part of the key (a program of
 * several files).
 */
export function nativeRuns(jobs) {
  return jobs.map(({ file, debug = false, test = false, args = [], dir }) => {
    const key = ["run", path.resolve(file), fs.readFileSync(file, "utf8"), debug, test, args, VERSIONS, dir ? dirContents(dir) : null];
    return cachedNative(key, async () => {
      // (A folder per build: rustc leaves a `.pdb` next to the executable on Windows.)
      const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "crs-native-"));
      const exe = path.join(tmp, `prog${process.platform === "win32" ? ".exe" : ""}`);
      try {
        const p = plan(file, exe, { debug, test });
        const build = async () => {
          p.before?.();
          const r = await run(p.cmd, p.args, p.opts);
          if (r.status === 0) p.after?.();
          return r;
        };
        const b = await (p.cargo ? inCargoTurn(build) : build());
        if (b.status !== 0) return { error: `${p.cmd} failed (exit ${b.status}):\n${b.stderr}` };
        return await run(exe, test ? ["--test-threads=1", ...args] : args);
      } catch (e) {
        return { error: e.stack };
      } finally {
        fs.rmSync(tmp, { recursive: true, force: true, maxRetries: 5 });
      }
    });
  });
}
