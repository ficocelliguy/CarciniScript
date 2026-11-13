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
//   interfaces → their name; anything else (other unions, generics, tuples, callbacks,
//   intersections, inline object types) → JsValue (untyped)
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
    ((interfaces.has(name) && interfaces.get(name).every((d) => !d.typeParameters)) || objectAliases.has(name));

  const resolving = new Set();
  /** Map a TS type to `{ ty, optional }` (`optional`: the type admits undefined/null). */
  function map(node) {
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
    if (ts.isParenthesizedTypeNode(node)) return map(node.type);
    if (ts.isTypeOperatorNode(node) && node.operator === ts.SyntaxKind.ReadonlyKeyword) return map(node.type);
    if (ts.isArrayTypeNode(node)) return { ty: `Vec<${map(node.elementType).ty}>`, optional: false };
    if (ts.isLiteralTypeNode(node)) {
      const l = node.literal;
      if (ts.isStringLiteral(l)) return { ty: "String", optional: false };
      if (ts.isNumericLiteral(l) || ts.isPrefixUnaryExpression(l)) return { ty: "f64", optional: false };
      if (l.kind === ts.SyntaxKind.TrueKeyword || l.kind === ts.SyntaxKind.FalseKeyword) return { ty: "bool", optional: false };
      if (l.kind === ts.SyntaxKind.NullKeyword) return { ty: "()", optional: true };
      return { ty: "JsValue", optional: false };
    }
    if (ts.isUnionTypeNode(node)) {
      const parts = node.types.map(map);
      const optional = parts.some((p) => p.optional);
      const tys = [...new Set(parts.filter((p) => !(p.optional && p.ty === "()")).map((p) => p.ty))];
      if (tys.length === 0) return { ty: "()", optional };
      if (tys.length === 1) return { ty: tys[0], optional };
      return { ty: "JsValue", optional };
    }
    // `Server & { isOnline?: boolean }`: the named interface (the extra inline fields are dropped).
    if (ts.isIntersectionTypeNode(node)) {
      const named = node.types.map(map).find((m) => isStruct(m.ty));
      return named ?? { ty: "JsValue", optional: false };
    }
    if (ts.isTypeReferenceNode(node)) {
      if (!ts.isIdentifier(node.typeName)) return { ty: "JsValue", optional: false };
      const name = node.typeName.text;
      const args = node.typeArguments ?? [];
      if ((name === "Array" || name === "ReadonlyArray") && args.length === 1) {
        return { ty: `Vec<${map(args[0]).ty}>`, optional: false };
      }
      if (name === "_ValueOf" && args.length === 1) return enumValue(args[0]);
      if (args.length === 0 && isStruct(name)) return { ty: name, optional: false };
      const alias = aliases.get(name);
      if (alias && !alias.typeParameters && args.length === 0 && !resolving.has(name)) {
        resolving.add(name);
        const r = map(alias.type);
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
    return out;
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

  function returnOf(sig) {
    const t = sig.type;
    if (t && ts.isTypeReferenceNode(t) && ts.isIdentifier(t.typeName) && t.typeName.text === "Promise") {
      const inner = t.typeArguments?.[0] ? map(t.typeArguments[0]) : { ty: "()", optional: false };
      return { ...inner, isAsync: true };
    }
    return { ...map(t), isAsync: false };
  }

  /** One Rust signature for a set of TS overloads. */
  function mergeOverloads(sigs) {
    if (sigs.length === 1) return { params: paramList(sigs[0]), ret: returnOf(sigs[0]) };
    const lists = sigs.map(paramList);
    const longest = lists.reduce((a, b) => (b.length > a.length ? b : a));
    const minLen = Math.min(...lists.map((l) => l.filter((p) => !p.optional).length));
    const params = longest.map((p, i) => {
      const sameType = lists.every((l) => !l[i] || l[i].ty === p.ty);
      return { ...p, ty: sameType ? p.ty : "JsValue", optional: p.optional || i >= minLen };
    });
    const rets = sigs.map(returnOf);
    const sameRet = rets.every((r) => r.ty === rets[0].ty && r.isAsync === rets[0].isAsync);
    return { params, ret: sameRet ? rets[0] : { ty: "JsValue", optional: false, isAsync: rets.every((r) => r.isAsync) } };
  }

  const rustType = (m, asField) => (m.optional && asField && m.ty !== "JsValue" ? `Option<${m.ty}>` : m.ty);
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
        const t = map(m.type);
        const ty = rustType({ ...t, optional: t.optional || !!m.questionToken }, true);
        fields.push(`${docLines(m, "    ")}    #[js = ${JSON.stringify(js)}]\n    pub ${rust}: ${ty},\n`);
      }
    }
    let out = `${docLines((interfaces.get(name) ?? [objectAliases.get(name)])[0], "")}pub struct ${name} {\n${fields.join("")}}\n`;
    const fns = [];
    for (const [js, overloads] of methods) {
      const rust = snakeCase(js);
      if (used.has(rust)) continue;
      used.add(rust);
      const { params, ret } = mergeOverloads(overloads.map((o) => o.sig));
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

  const names = [...new Set([...interfaces.keys(), ...objectAliases.keys()])].filter(isStruct).sort((a, b) => {
    if (a === "NS") return -1;
    if (b === "NS") return 1;
    return a.localeCompare(b);
  });
  const header =
    "// Generated by carciniscript/tools/dts2rs.mjs from Bitburner's NetscriptDefinitions.d.ts. Do not edit.\n" +
    "// `JsValue` is an untyped JS value. `#[optional]` parameters may be omitted; a `#[variadic]` last\n" +
    "// parameter takes any number of arguments.\n\n";
  return header + names.map(emitType).join("\n");
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
