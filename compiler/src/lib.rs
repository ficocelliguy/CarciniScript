//! CarciniScript: compiles a subset of Rust to ES modules with source maps.

mod codegen;
mod fmt;
mod lineindex;
mod sourcemap;
mod ty;

pub use codegen::{CompileOutput, Diagnostic, compile};

use wasm_bindgen::prelude::*;

/// Compile Rust source to JS. Returns JSON: `{ code, sourceMap, diagnostics }`; `code` and
/// `sourceMap` are null when there are errors.
#[wasm_bindgen(js_name = compileJson)]
pub fn compile_json(source: &str, filename: &str) -> String {
    serde_json::to_string(&compile(source, filename)).unwrap()
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
