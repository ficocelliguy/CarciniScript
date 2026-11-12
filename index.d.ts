export interface Diagnostic {
  message: string;
  severity: "error" | "warning";
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
}

export function initCarciniScript(moduleOrPath?: string | URL | Request | BufferSource | WebAssembly.Module): Promise<void>;
export function initCarciniScriptSync(module: BufferSource | WebAssembly.Module): void;
export function compile(source: string, filename?: string): CompileOutput;
export function version(): string;
