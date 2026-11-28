/** A secondary place a diagnostic involves, with rustc's label for it (`value moved here`). */
export interface DiagnosticSpan {
  start: number;
  end: number;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  label: string;
  /** Another primary span (`^^^`), as rustc marks each unused import of a `use`. */
  primary?: boolean;
}

/**
 * A rustc sub-diagnostic shown after the `= help:` / `= note:` lines. With `spans` it has its own
 * `-->` line and snippet (`note: the lint level is defined here`); with `edits` it is a
 * suggestion (each edit's `label` is the text replacing its range); with neither, a plain line.
 */
export interface DiagnosticChild {
  kind: "help" | "note";
  message: string;
  spans?: DiagnosticSpan[];
  edits?: DiagnosticSpan[];
  /**
   * A note into another file (std's own source, as rustc shows it with rust-src installed):
   * rustc's path for it, and the text of line `line`, which `spans` are on.
   */
  source?: { path: string; line: number; text: string };
}

export interface Diagnostic {
  /** rustc's headline: `mismatched types`. */
  message: string;
  severity: "error" | "warning";
  /** rustc's error code, for the errors rustc gives one: `E0308`. */
  code?: string;
  /** The text rustc shows under the span: `expected `i32`, found `&str``. */
  label?: string;
  /** Other places the error involves. */
  spans?: DiagnosticSpan[];
  /** rustc's `help:` lines. */
  helps?: string[];
  /** rustc's `note:` lines. */
  notes?: string[];
  /** rustc's sub-diagnostics after those, in order. */
  children?: DiagnosticChild[];
  /**
   * The lint a warning is, by rustc's name (`unused_variables`, `dead_code`, ..; `inexact_64bit`
   * is this compiler's own). `#[allow(..)]` takes it; `#[deny(..)]` makes the warning an error.
   */
  lint?: string;
  /**
   * The rule behind the message in JavaScript or Python terms, and what to try, for a player who
   * doesn't know Rust. One or two sentences; not every message has one. Show it under `message`
   * in an editor; `rendered` (rustc's format) leaves it out.
   */
  help?: string;
  /** Byte offsets into the source. */
  start: number;
  end: number;
  /** 1-based line and 1-based UTF-16 column, matching Monaco markers. */
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
}

export interface CompileOutput {
  /** ES module source, or null if there were errors. */
  code: string | null;
  /** Source map v3 JSON, or null if there were errors. */
  sourceMap: string | null;
  diagnostics: Diagnostic[];
  /**
   * The diagnostics as rustc prints them (`formatDiagnostics`), summary included; empty when
   * there are none. Show it as is, in a monospace font.
   */
  rendered: string;
  /**
   * Files the output depends on besides its `mod` files (which it imports): the proc-macro crates
   * it used. A host that caches compiled scripts should recompile when one of them changes.
   */
  dependencies: string[];
}

/**
 * Load the compiler. Call once at startup; `compile` is synchronous afterwards. With no argument it
 * loads the package's own wasm (fetched next to the module in browsers, read from disk in Node).
 */
export function initCarciniScript(moduleOrPath?: string | URL | Request | BufferSource | WebAssembly.Module): Promise<void>;
/**
 * Load the compiler synchronously from wasm bytes or a compiled module. In Node the argument may be
 * left out to read the package's own wasm; elsewhere it is required.
 */
export function initCarciniScriptSync(module?: BufferSource | WebAssembly.Module): void;
/**
 * Compile Rust to an ES module. `resolve(path)` returns the source of another file, for `mod name;`
 * (which reads `name.rs` next to `filename`, or the file a `#[path]` names, from the directory of
 * `filename` or from the root as `lib/util.rs`); undefined when there is no such file.
 */
export function compile(
  source: string,
  filename?: string,
  resolve?: (path: string) => string | undefined,
): CompileOutput;
/**
 * Compile Rust's tests, like `rustc --test`: `cfg(test)` is on, and the module's `main(ns)` runs the
 * `#[test]` functions and prints libtest's report (to the terminal with `ns.tprintf`, or to stdout
 * without an `ns`) instead of running the program's `main`. `ns.args` are libtest's: name filters,
 * `--exact`, `--ignored`, `--include-ignored`.
 */
export function compileTests(
  source: string,
  filename?: string,
  resolve?: (path: string) => string | undefined,
): CompileOutput;
export interface Hover {
  /** Markdown. */
  contents: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
}

export interface Completion {
  label: string;
  kind: "method" | "field" | "function" | "variable" | "struct" | "enum" | "trait" | "constant" | "module";
  /** A signature or type. */
  detail: string;
  documentation: string;
}

/** Hover at a 1-based line and UTF-16 column; null when there's nothing to show. */
export function hover(
  source: string,
  filename: string,
  line: number,
  column: number,
  resolve?: (path: string) => string | undefined,
): Hover | null;

/** Completions after a `.` at a 1-based line and UTF-16 column. */
export function completions(
  source: string,
  filename: string,
  line: number,
  column: number,
  resolve?: (path: string) => string | undefined,
): Completion[];

export interface SignatureHelp {
  /** The whole signature. */
  label: string;
  /** Each parameter as it appears in `label`. */
  params: string[];
  /** Index of the parameter the cursor is in. */
  active: number;
  documentation: string;
}

/** Parameter hints for the call around a 1-based line and UTF-16 column; null outside a call. */
export function signatureHelp(
  source: string,
  filename: string,
  line: number,
  column: number,
  resolve?: (path: string) => string | undefined,
): SignatureHelp | null;

/** Install typed `NS` bindings (generated by `tools/dts2rs.mjs`). Returns problems found. */
export function setNsBindings(text: string): string[];
export function version(): string;
/**
 * Diagnostics in rustc's human format (`error[E0308]: mismatched types`, the `-->` line, the source
 * with `^^^` under each span and its label, `help:` and `note:` lines), then rustc's summary
 * (`error: aborting due to 2 previous errors`). `compile()`'s `rendered` is this for its diagnostics.
 */
export function formatDiagnostics(source: string, filename: string, diagnostics: Diagnostic[]): string;

/** A range in a file: 1-based lines, UTF-16 columns. */
export interface Location {
  /** The file asked about, or another file's path as passed to `resolve`. */
  file: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
}

/** Where the name at a 1-based line and UTF-16 column is defined; null when unknown. */
export function definition(
  source: string,
  filename: string,
  line: number,
  column: number,
  resolve?: (path: string) => string | undefined,
): Location | null;

/**
 * Every use of the name at a 1-based line and UTF-16 column, its declaration included, in this file
 * and in `others` (paths `resolve` reads).
 */
export function references(
  source: string,
  filename: string,
  line: number,
  column: number,
  resolve?: (path: string) => string | undefined,
  others?: string[],
): Location[];

/** The ranges to replace with `newName` to rename the name there, also in `others`; empty when it can't be renamed. */
export function rename(
  source: string,
  filename: string,
  line: number,
  column: number,
  newName: string,
  resolve?: (path: string) => string | undefined,
  others?: string[],
): Location[];

/**
 * Debug-build overflow checks for the compiles that follow: integer arithmetic that overflows
 * panics (`attempt to add with overflow`) instead of wrapping, as `rustc` without `-O` does.
 */
export function setOverflowChecks(on: boolean): void;

/**
 * Borrow conflicts rustc rejects are errors by default, as in rustc; `false` makes them warnings
 * for the compiles that follow (the program still runs).
 */
export function setStrictBorrows(on: boolean): void;

/**
 * Errors in test code (`#[test]` functions, `#[cfg(test)]` modules) come from a second, test build
 * of the file and are reported as warnings; `false` skips that build for the compiles that follow.
 */
export function setTestCodeChecks(on: boolean): void;
