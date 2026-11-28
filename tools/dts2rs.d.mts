// Types for `carciniscript/dts2rs` (tools/dts2rs.mjs). It needs TypeScript 5 or 6 installed (a peer
// dependency): TypeScript 7 has no JS API.

/**
 * Rust bindings, for `setNsBindings`, from the text of a TypeScript declaration file (Bitburner's
 * `NetscriptDefinitions.d.ts`, or one shaped like it).
 */
export function dtsToRust(text: string): string;

/**
 * Whether `owner`'s `member` (a field or a method's result), or its parameter `param`, is a whole
 * number (`i32`). `owner` is the TS interface, or the struct made for an inline type.
 */
export function isWholeNumber(owner: string, member: string, param?: string): boolean;

/** A TypeScript name in Rust's snake_case. */
export function snakeCase(name: string): string;
