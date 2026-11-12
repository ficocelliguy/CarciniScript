/**
 * rustc's panic report from stderr, normalized for comparison with CarciniScript's:
 * drops the thread id (`thread 'main' (1234)`), the source directory, and the trailing
 * RUST_BACKTRACE note.
 */
export function nativePanic(stderr) {
  return stderr
    .replace(/\r\n/g, "\n")
    .split("\nnote: ")[0]
    .trim()
    .replace(/^thread 'main' \(\d+\)/, "thread 'main'")
    .replace(/panicked at [^\n]*[\\/]([^\\/\n]+:\d+:\d+:)/, "panicked at $1");
}
