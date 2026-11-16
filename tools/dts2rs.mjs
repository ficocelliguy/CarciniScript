// Generates CarciniScript's typed NS bindings from Bitburner's NetscriptDefinitions.d.ts.
//
//   node tools/dts2rs.mjs path/to/NetscriptDefinitions.d.ts [out.rs]
//
// The output is Rust declaration syntax that the compiler parses at init (`setNsBindings`):
// TS interfaces become `pub struct`s (fields) plus `impl` blocks (methods without bodies), and
// every member records its JS name in `#[js = "..."]`.
//
// Mapping (TS → Rust):
//   number → f64, string → String, boolean → bool, void → (), T[] / readonly T[] → Vec<T>
//   Promise<T> return → `async fn` returning T
//   T | undefined, `x?:` → Option<T> for fields; `#[optional]` for parameters (may be omitted)
//   ...rest: T[] → a trailing `#[variadic]` parameter of type T
//   string-literal unions and Bitburner's `_ValueOf<...EnumType>` enums → String
//   interfaces → their name, with `#[extends = "Base"]` for `interface X extends Base` (a value
//   of `X` is accepted where a `Base` is expected)
//   tuples → `[T; N]` when every element has the same type, else `(A, B, ..)`
//   Map<K, V>, Record<K, V> (also Partial) → HashMap<K, V>; number keys are i32, others String
//   inline object types in results and fields → a struct named after where they appear
//   (`GoMakeMoveResult`); `Named & { extra }` → `NamedWithExtra`, extending `Named`; a
//   `type X = A & { .. }` alias → struct `X`
//   anything else (other unions, generics, callbacks, inline types in parameters) → JsValue
//   (untyped; a `JsValue` parameter takes any value)
import fs from "node:fs";
import ts from "typescript";

const RUST_KEYWORDS = new Set(
  ("as break const continue crate else enum extern false fn for if impl in let loop match mod move mut pub ref " +
    "return self Self static struct super trait true type unsafe use where while async await dyn abstract become " +
    "box do final macro override priv typeof unsized virtual yield try gen")
    .split(" "),
);
const RESERVED_TYPE_NAMES = new Set(["Vec", "Option", "String", "JsValue", "Result", "Box", "Self"]);

export function snakeCase(name) {
  const s = name
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2")
    .toLowerCase()
    // Quoted TS names can contain anything ("find largest prime factor"); the JS name is kept
    // separately in `#[js = ...]`.
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/^(?=[0-9])/, "_");
  return RUST_KEYWORDS.has(s) ? `r#${s}` : s;
}

function docOf(node) {
  const docs = ts.getJSDocCommentsAndTags(node).filter(ts.isJSDoc);
  const doc = docs[docs.length - 1];
  if (!doc) return [];
  const summary = (ts.getTextOfJSDocComment(doc.comment) ?? "").split(/\n\s*\n/)[0].replace(/\s+/g, " ").trim();
  const lines = summary ? [summary.length > 200 ? `${summary.slice(0, 197)}...` : summary] : [];
  for (const tag of doc.tags ?? []) {
    const text = ts.getTextOfJSDocComment(tag.comment) ?? "";
    const ram = /RAM cost:[^\n]*/.exec(text);
    if (tag.tagName.text === "remarks" && ram) lines.push(ram[0].trim());
  }
  // `{@link Foo | bar}` → bar
  return lines.map((l) => l.replace(/\{@link\s+[^}|]*\|\s*([^}]*)\}/g, "$1").replace(/\{@link\s+([^}]*)\}/g, "$1"));
}

export function dtsToRust(text) {
  const sf = ts.createSourceFile("NetscriptDefinitions.d.ts", text, ts.ScriptTarget.Latest, true);
  const interfaces = new Map(); // name -> InterfaceDeclaration[] (declaration merging)
  const aliases = new Map(); // name -> TypeAliasDeclaration
  for (const st of sf.statements) {
    if (ts.isInterfaceDeclaration(st)) {
      if (!interfaces.has(st.name.text)) interfaces.set(st.name.text, []);
      interfaces.get(st.name.text).push(st);
    } else if (ts.isTypeAliasDeclaration(st)) {
      aliases.set(st.name.text, st);
    }
  }
  // Object-literal aliases (`type X = { ... }`) are emitted like interfaces.
  const objectAliases = new Map(
    [...aliases].filter(([, a]) => !a.typeParameters && ts.isTypeLiteralNode(a.type)).map(([n, a]) => [n, a]),
  );
  const isStruct = (name) =>
    !RESERVED_TYPE_NAMES.has(name) &&
    ((interfaces.has(name) && interfaces.get(name).every((d) => !d.typeParameters)) ||
      objectAliases.has(name) ||
      intersectionAliases.has(name));
  // `type X = A & { .. }` aliases of object types and interfaces: structs too, extending `A`.
  const intersectionAliases = new Map(
    [...aliases]
      .filter(
        ([, a]) =>
          !a.typeParameters &&
          ts.isIntersectionTypeNode(a.type) &&
          a.type.types.every(
            (t) =>
              ts.isTypeLiteralNode(t) ||
              (ts.isTypeReferenceNode(t) &&
                ts.isIdentifier(t.typeName) &&
                !t.typeArguments &&
                (interfaces.has(t.typeName.text) || objectAliases.has(t.typeName.text))),
          ),
      )
      .map(([n, a]) => [n, a]),
  );
  // Structs made for inline object types and intersections: name -> { members, extends, doc }.
  const synthesized = new Map();
  const synthByText = new Map();
  const pascal = (s) =>
    snakeCase(s)
      .replace(/^r#/, "")
      .split("_")
      .filter(Boolean)
      .map((w) => w[0].toUpperCase() + w.slice(1))
      .join("");
  /** A struct for an inline object type (or an intersection), named `hint` unless the same type
   * was already given a name. */
  function synthesize(node, members, bases, hint) {
    const key = node.getText().replace(/\s+/g, " ");
    if (synthByText.has(key)) return synthByText.get(key);
    let name = hint;
    for (let i = 2; interfaces.has(name) || aliases.has(name) || synthesized.has(name); i++) name = `${hint}${i}`;
    synthByText.set(key, name);
    synthesized.set(name, { members, bases });
    return name;
  }
  /** `[A, B]` element types. */
  const tupleElems = (node) => node.elements.map((e) => (ts.isNamedTupleMember(e) ? e.type : e));

  const resolving = new Set();
  /** Map a TS type to `{ ty, optional }` (`optional`: the type admits undefined/null). `hint`
   * names a struct made for an inline object type (none: such types are `JsValue`). */
  function map(node, hint) {
    if (!node) return { ty: "JsValue", optional: false };
    switch (node.kind) {
      case ts.SyntaxKind.NumberKeyword:
        return { ty: "f64", optional: false };
      case ts.SyntaxKind.StringKeyword:
        return { ty: "String", optional: false };
      case ts.SyntaxKind.BooleanKeyword:
        return { ty: "bool", optional: false };
      case ts.SyntaxKind.VoidKeyword:
        return { ty: "()", optional: false };
      case ts.SyntaxKind.UndefinedKeyword:
      case ts.SyntaxKind.NullKeyword:
        return { ty: "()", optional: true };
    }
    if (ts.isParenthesizedTypeNode(node)) return map(node.type, hint);
    if (ts.isTypeOperatorNode(node) && node.operator === ts.SyntaxKind.ReadonlyKeyword) return map(node.type, hint);
    if (ts.isArrayTypeNode(node)) return { ty: `Vec<${map(node.elementType, hint).ty}>`, optional: false };
    // `[number, number]` → `[f64; 2]`; `[number, string]` → `(f64, String)`.
    if (ts.isTupleTypeNode(node)) {
      const elems = tupleElems(node);
      if (elems.length === 0 || elems.some((e) => ts.isRestTypeNode(e) || ts.isOptionalTypeNode(e))) {
        return { ty: "JsValue", optional: false };
      }
      const tys = elems.map((e) => map(e, hint)).map((m) => (m.optional ? `Option<${m.ty}>` : m.ty));
      if (tys.includes("JsValue") || tys.includes("()")) return { ty: "JsValue", optional: false };
      const same = tys.every((t) => t === tys[0]);
      return { ty: same ? `[${tys[0]}; ${tys.length}]` : `(${tys.join(", ")})`, optional: false };
    }
    // An inline object type: a struct of its own.
    if (ts.isTypeLiteralNode(node) && hint) {
      if (!node.members.every((m) => ts.isPropertySignature(m) || ts.isMethodSignature(m))) {
        return { ty: "JsValue", optional: false };
      }
      return { ty: synthesize(node, [...node.members], [], hint), optional: false };
    }
    if (ts.isLiteralTypeNode(node)) {
      const l = node.literal;
      if (ts.isStringLiteral(l)) return { ty: "String", optional: false };
      if (ts.isNumericLiteral(l) || ts.isPrefixUnaryExpression(l)) return { ty: "f64", optional: false };
      if (l.kind === ts.SyntaxKind.TrueKeyword || l.kind === ts.SyntaxKind.FalseKeyword) return { ty: "bool", optional: false };
      if (l.kind === ts.SyntaxKind.NullKeyword) return { ty: "()", optional: true };
      return { ty: "JsValue", optional: false };
    }
    if (ts.isUnionTypeNode(node)) {
      const parts = node.types.map((t) => map(t, hint));
      const optional = parts.some((p) => p.optional);
      const tys = [...new Set(parts.filter((p) => !(p.optional && p.ty === "()")).map((p) => p.ty))];
      if (tys.length === 0) return { ty: "()", optional };
      if (tys.length === 1) return { ty: tys[0], optional };
      return { ty: "JsValue", optional };
    }
    // `Server & { isOnline?: boolean }`: `ServerWithIsOnline`, extending `Server` (without a
    // name for it, the named interface).
    if (ts.isIntersectionTypeNode(node)) {
      const named = node.types.map((t) => map(t)).find((m) => isStruct(m.ty));
      const literals = node.types.filter(ts.isTypeLiteralNode);
      const refs = node.types.filter((t) => ts.isTypeReferenceNode(t) && ts.isIdentifier(t.typeName));
      if (named && hint && literals.length && refs.length + literals.length === node.types.length) {
        const bases = refs.map((t) => t.typeName.text).filter(isStruct);
        if (bases.length === refs.length) {
          const extra = literals.flatMap((l) => l.members);
          const members = [...bases.flatMap((b) => membersOf(b)), ...extra];
          const extraNames = extra.map(memberName).filter(Boolean).map(pascal).join("");
          const name = synthesize(node, members, bases, `${bases[0]}With${extraNames || "Extra"}`);
          return { ty: name, optional: false };
        }
      }
      return named ?? { ty: "JsValue", optional: false };
    }
    if (ts.isTypeReferenceNode(node)) {
      if (!ts.isIdentifier(node.typeName)) return { ty: "JsValue", optional: false };
      const name = node.typeName.text;
      const args = node.typeArguments ?? [];
      if ((name === "Array" || name === "ReadonlyArray") && args.length === 1) {
        return { ty: `Vec<${map(args[0], hint).ty}>`, optional: false };
      }
      // `Map<number, number>`, `Record<string, T>`, `Partial<Record<K, T>>`: a `HashMap` (string
      // keys for every key type but `number`, whose keys are `i32`).
      if ((name === "Map" || name === "Record") && args.length === 2) {
        const k = map(args[0]);
        const v = map(args[1], hint);
        const key = k.ty === "f64" ? "i32" : k.ty === "String" ? "String" : null;
        if (!key || v.ty === "JsValue" || v.ty === "()") return { ty: "JsValue", optional: false };
        return { ty: `HashMap<${key}, ${v.optional ? `Option<${v.ty}>` : v.ty}>`, optional: false };
      }
      if (name === "Partial" && args.length === 1 && ts.isTypeReferenceNode(args[0]) && ts.isIdentifier(args[0].typeName) && args[0].typeName.text === "Record") {
        return map(args[0], hint);
      }
      if (name === "_ValueOf" && args.length === 1) return enumValue(args[0]);
      if (args.length === 0 && isStruct(name)) return { ty: name, optional: false };
      const alias = aliases.get(name);
      if (alias && !alias.typeParameters && args.length === 0 && !resolving.has(name)) {
        resolving.add(name);
        const r = map(alias.type, hint);
        resolving.delete(name);
        return r;
      }
      return { ty: "JsValue", optional: false };
    }
    return { ty: "JsValue", optional: false };
  }

  /** `_ValueOf<XEnumType>`: the values of an object of string literals. */
  function enumValue(arg) {
    const target =
      ts.isTypeReferenceNode(arg) && ts.isIdentifier(arg.typeName) ? aliases.get(arg.typeName.text)?.type : arg;
    if (target && ts.isTypeLiteralNode(target)) {
      const values = target.members.filter(ts.isPropertySignature).map((m) => map(m.type).ty);
      const unique = [...new Set(values)];
      if (unique.length === 1) return { ty: unique[0], optional: false };
    }
    return { ty: "JsValue", optional: false };
  }

  function membersOf(name, seen = new Set()) {
    if (seen.has(name)) return [];
    seen.add(name);
    const out = [];
    for (const decl of interfaces.get(name) ?? []) {
      for (const clause of decl.heritageClauses ?? []) {
        for (const t of clause.types) {
          if (ts.isIdentifier(t.expression)) out.push(...membersOf(t.expression.text, seen));
        }
      }
      out.push(...decl.members);
    }
    const alias = objectAliases.get(name);
    if (alias) out.push(...alias.type.members);
    const inter = intersectionAliases.get(name);
    if (inter) {
      for (const t of inter.type.types) {
        if (ts.isTypeLiteralNode(t)) out.push(...t.members);
        else out.push(...membersOf(t.typeName.text, seen));
      }
    }
    const synth = synthesized.get(name);
    if (synth) out.push(...synth.members);
    return out;
  }

  /** The interfaces `name` extends (`#[extends = ".."]`). */
  function basesOf(name) {
    const out = [];
    for (const decl of interfaces.get(name) ?? []) {
      for (const clause of decl.heritageClauses ?? []) {
        for (const t of clause.types) {
          if (ts.isIdentifier(t.expression) && isStruct(t.expression.text)) out.push(t.expression.text);
        }
      }
    }
    const inter = intersectionAliases.get(name);
    if (inter) {
      for (const t of inter.type.types) if (ts.isTypeReferenceNode(t)) out.push(t.typeName.text);
    }
    out.push(...(synthesized.get(name)?.bases ?? []));
    return [...new Set(out)];
  }

  const memberName = (m) =>
    m.name && (ts.isIdentifier(m.name) || ts.isStringLiteral(m.name)) ? m.name.text : undefined;

  function paramList(sig) {
    return sig.parameters.map((p, i) => {
      const name = ts.isIdentifier(p.name) ? snakeCase(p.name.text) : `arg${i}`;
      if (p.dotDotDotToken) {
        const t = p.type && ts.isArrayTypeNode(p.type) ? map(p.type.elementType).ty : "JsValue";
        return { name, ty: t, optional: true, variadic: true };
      }
      const m = map(p.type);
      return { name, ty: m.ty === "()" ? "JsValue" : m.ty, optional: !!p.questionToken || !!p.initializer || m.optional };
    });
  }

  function returnOf(sig, hint) {
    const t = sig.type;
    if (t && ts.isTypeReferenceNode(t) && ts.isIdentifier(t.typeName) && t.typeName.text === "Promise") {
      const inner = t.typeArguments?.[0] ? map(t.typeArguments[0], hint) : { ty: "()", optional: false };
      return { ...inner, isAsync: true };
    }
    return { ...map(t, hint), isAsync: false };
  }

  /** One Rust signature for a set of TS overloads. */
  function mergeOverloads(sigs, hint) {
    if (sigs.length === 1) return { params: paramList(sigs[0]), ret: returnOf(sigs[0], hint) };
    const lists = sigs.map(paramList);
    const longest = lists.reduce((a, b) => (b.length > a.length ? b : a));
    const minLen = Math.min(...lists.map((l) => l.filter((p) => !p.optional).length));
    const params = longest.map((p, i) => {
      const sameType = lists.every((l) => !l[i] || l[i].ty === p.ty);
      return { ...p, ty: sameType ? p.ty : "JsValue", optional: p.optional || i >= minLen };
    });
    const rets = sigs.map((sg) => returnOf(sg, hint));
    const sameRet = rets.every((r) => r.ty === rets[0].ty && r.isAsync === rets[0].isAsync);
    return { params, ret: sameRet ? rets[0] : { ty: "JsValue", optional: false, isAsync: rets.every((r) => r.isAsync) } };
  }

  const rustType = (m, asField) => (m.optional && asField ? `Option<${m.ty}>` : m.ty);
  const docLines = (node, indent) => docOf(node).map((l) => `${indent}/// ${l}\n`).join("");

  function emitType(name) {
    const members = membersOf(name);
    const fields = [];
    const methods = new Map();
    const used = new Set();
    for (const m of members) {
      const js = memberName(m);
      if (!js) continue;
      const isFnProp = ts.isPropertySignature(m) && m.type && ts.isFunctionTypeNode(m.type);
      if (ts.isMethodSignature(m) || isFnProp) {
        if (!methods.has(js)) methods.set(js, []);
        methods.get(js).push({ node: m, sig: isFnProp ? m.type : m });
      } else if (ts.isPropertySignature(m)) {
        const rust = snakeCase(js);
        if (used.has(rust)) continue;
        used.add(rust);
        const t = map(m.type, `${name}${pascal(js)}`);
        const ty = rustType({ ...t, optional: t.optional || !!m.questionToken }, true);
        fields.push(`${docLines(m, "    ")}    #[js = ${JSON.stringify(js)}]\n    pub ${rust}: ${ty},\n`);
      }
    }
    const declared = (interfaces.get(name) ?? [objectAliases.get(name) ?? intersectionAliases.get(name)])[0];
    const bases = basesOf(name);
    const extendsAttr = bases.length ? `#[extends = ${JSON.stringify(bases.join(", "))}]\n` : "";
    let out = `${declared ? docLines(declared, "") : ""}${extendsAttr}pub struct ${name} {\n${fields.join("")}}\n`;
    const fns = [];
    for (const [js, overloads] of methods) {
      const rust = snakeCase(js);
      if (used.has(rust)) continue;
      used.add(rust);
      const { params, ret } = mergeOverloads(
        overloads.map((o) => o.sig),
        `${name === "NS" ? "" : name}${pascal(js)}Result`,
      );
      const ps = params.map((p) => {
        const attr = p.variadic ? "#[variadic] " : p.optional ? "#[optional] " : "";
        return `${attr}${p.name}: ${p.ty}`;
      });
      const retTy = ret.ty === "()" ? "" : ` -> ${rustType(ret, true)}`;
      const asyncKw = ret.isAsync ? "async " : "";
      fns.push(
        `${docLines(overloads[0].node, "    ")}    #[js = ${JSON.stringify(js)}]\n    pub ${asyncKw}fn ${rust}(&self${ps.map((p) => `, ${p}`).join("")})${retTy};\n`,
      );
    }
    if (fns.length) out += `impl ${name} {\n${fns.join("")}}\n`;
    return out;
  }

  const names = [...new Set([...interfaces.keys(), ...objectAliases.keys(), ...intersectionAliases.keys()])]
    .filter(isStruct)
    .sort((a, b) => {
      if (a === "NS") return -1;
      if (b === "NS") return 1;
      return a.localeCompare(b);
    });
  const header =
    "// Generated by carciniscript/tools/dts2rs.mjs from Bitburner's NetscriptDefinitions.d.ts. Do not edit.\n" +
    "// `JsValue` is an untyped JS value. `#[optional]` parameters may be omitted; a `#[variadic]` last\n" +
    "// parameter takes any number of arguments.\n\n";
  const emitted = names.map(emitType);
  // The structs made along the way (their own inline types may add more).
  const done = new Set();
  for (let more = true; more; ) {
    more = false;
    for (const n of [...synthesized.keys()].sort()) {
      if (done.has(n)) continue;
      done.add(n);
      more = true;
      emitted.push(emitType(n));
    }
  }
  return header + emitted.join("\n");
}

if (import.meta.url === `file:///${process.argv[1].replace(/\\/g, "/").replace(/^\//, "")}`) {
  const [input, output] = process.argv.slice(2);
  if (!input) {
    console.error("usage: node tools/dts2rs.mjs <NetscriptDefinitions.d.ts> [out.rs]");
    process.exit(2);
  }
  const rust = dtsToRust(fs.readFileSync(input, "utf8"));
  if (output) fs.writeFileSync(output, rust);
  else process.stdout.write(rust);
}
