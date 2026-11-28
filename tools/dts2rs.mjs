// Generates CarciniScript's typed NS bindings from Bitburner's NetscriptDefinitions.d.ts.
//
//   node tools/dts2rs.mjs path/to/NetscriptDefinitions.d.ts [out.rs]
//
// The output is Rust declaration syntax that the compiler parses at init (`setNsBindings`):
// TS interfaces become `pub struct`s (fields) plus `impl` blocks (methods without bodies), TS
// unions become `pub enum`s, and every member records its JS name in `#[js = "..."]`.
//
// Mapping (TS → Rust):
//   number → f64, string → String, boolean → bool, bigint → i128, void → (), never → !
//   number → i32 for whole-number values picked out by name (`isWholeNumber`): threads, pids,
//   ports, cores, levels, counts, indexes, Go / Stanek coordinates (`RunningScript.threads`,
//   `getServerNumPortsRequired()`, `hacknet.numNodes()`, `RunThreadOrOptions::Num(i32)`,
//   `FilenameOrPID::Num(i32)`); money, RAM, time, rates, multipliers, reputation, hashes and
//   shares stay f64
//   T[] / readonly T[] → Vec<T> (parameters borrow them: string → &str, T[] → &[T], [T, T] → &[T; 2])
//   Promise<T> return → `async fn` returning T; a `Promise<T>` property → `impl Future<Output = T>`
//   T | undefined, `x?:` → Option<T> for fields; `#[optional]` for parameters (may be omitted)
//   ...rest: T[] → a trailing `#[variadic]` parameter of type T
//   string-literal unions and Bitburner's `_ValueOf<...EnumType>` enums → String
//   other unions → an enum named after the TS alias (`ScriptArg`) or after where it appears
//   (`RunThreadOrOptions`, `MaterialDesiredSellPrice`): `#[untagged]` when the members are told
//   apart by their JS type (`Str(String)`, `Num(f64)`, `Bool(bool)`, `List(Vec<T>)`, a struct by its
//   name), `#[tag = "type"]` when every member is an interface with a distinct literal in one
//   field (`#[js = "money"] Money(MoneyRequirement)`). The value is the JS value itself; a
//   parameter of an enum type also takes any variant's payload directly.
//   `flags(schema)` (`[string, any][]` in the d.ts) → `schema: &[(&str, FlagValue)]` returning
//   `Flags` (`get_str(name)`, `rest`, ..)
//   Bitburner's `Result<T>` → `NsResult { Success(SuccessResult), Failure(FailureResult) }` by
//   `success`, with `into_result()`.
//   `CodingContractObject` (a mapped type over the contract names) → one struct per contract
//   (`FindLargestPrimeFactorContract`, with `data` and `submit(answer)` typed for it) and the
//   enum over them by `type`
//   interfaces → their name, with `#[extends = "Base"]` for `interface X extends Base` (a value
//   of `X` is accepted where a `Base` is expected)
//   tuples → `[T; N]` when every element has the same type, else `(A, B, ..)`
//   Map<K, V>, Record<K, V> (also Partial) → HashMap<K, V>; number keys are i32, others String
//   Partial<Interface> → a struct with every field optional (`PartialSkills`)
//   X["Key"] indexed access and `Exclude<Union, ..>` over string literals → String
//   inline object types in results and fields → a struct named after where they appear
//   (`GoMakeMoveResult`); `Named & { extra }` → `NamedWithExtra`, extending `Named`; a
//   `type X = A & { .. }` alias → struct `X`
//   `() => void` parameters → `impl Fn()`; `Date` → the `js` crate's `Date`;
//   `Uint8Array` → `Vec<u8>` (`#[array]`)
//   ReactNode / ReactElement (Bitburner's stand-ins for React's) → `ReactNode` (`react::Node`, from
//   the `js` crate); a parameter of it takes what a `jsx!` child may be (text, numbers, ..)
//   anything else (`any`, generics, inline types in parameters) → JsValue (untyped; a `JsValue`
//   parameter takes any value)
import fs from "node:fs";
import { pathToFileURL } from "node:url";
import ts from "typescript";

// TypeScript 7 (the native compiler) has no JS API; this needs TypeScript 5 or 6.
const needTs5 = () => {
  if (typeof ts?.createSourceFile !== "function")
    throw new Error(`dts2rs needs the JS API of TypeScript 5 or 6 (npm install -D typescript@6); found typescript ${ts?.version ?? "without one"}`);
};

const RUST_KEYWORDS = new Set(
  ("as break const continue crate else enum extern false fn for if impl in let loop match mod move mut pub ref " +
    "return self Self static struct super trait true type unsafe use where while async await dyn abstract become " +
    "box do final macro override priv typeof unsized virtual yield try gen")
    .split(" "),
);
const RESERVED_TYPE_NAMES = new Set(["Vec", "Option", "String", "JsValue", "Result", "Box", "Self"]);

// Whole-number NS values are `i32`, not `f64` (TS has one `number` type), so `pid == 0`,
// `for i in 0..ns.hacknet.num_nodes()` and `v[n as usize]` need no cast. Names that are whole
// numbers anywhere in the API, by exact name or pattern; the rest by `Owner.member` or
// `Owner.method(param)`. Everything else stays `f64`: money, RAM, time, rates, multipliers,
// reputation, favor, hashes (fractional), shares (past 2^31) and `getActionCountRemaining`
// (`Infinity` for general actions).
const INT_NAMES = new Set(
  (
    "threads threadOrOptions pid parent cores cpuCores port portNumber sleeveNumber index level lvl nextBN " +
    "fractionalDigits rootX rootY rotation fragmentId giftWidth giftHeight count cheatCount extraCores " +
    "extraCache startingCore startingCache skillPoints correctCharactersInPassword logsToCapture " +
    "passwordLength depth tasksCompleted kills entropy wins losses winStreak highestWinStreak round " +
    "bonusCycles maxProducts bitNodeN sourceFile hacknetCores versionNumber cache memory"
  ).split(" "),
);
const INT_MEMBERS = new Set([
  "NS.run",
  "NS.exec",
  "NS.getBitNodeMultipliers(n)",
  "Hacknet.purchaseNode",
  "Hacknet.upgradeLevel(n)",
  "Hacknet.upgradeRam(n)",
  "Hacknet.upgradeCore(n)",
  "Hacknet.upgradeCache(n)",
  "Hacknet.getLevelUpgradeCost(n)",
  "Hacknet.getRamUpgradeCost(n)",
  "Hacknet.getCoreUpgradeCost(n)",
  "Hacknet.getCacheUpgradeCost(n)",
  "HacknetNodesFormulas.hacknetNodeCost(n)",
  "HacknetServersFormulas.hacknetServerCost(n)",
  "SourceFileLvl.n",
  "Bladeburner.getSkillPoints",
  "Bladeburner.getCityCommunities",
  "Bladeburner.getActionSuccesses",
  "Bladeburner.getBlackOpRank",
  "Bladeburner.setTeamSize(size)",
  "Gang.getRecruitsAvailable",
  "Singularity.getHackingLevelRequirementOfProgram",
  "SkillsFormulas.calculateSkill",
  "SkillsFormulas.calculateExp(skill)",
  "Office.employeeJobs",
  "ResetInfo.currentNode",
  "ResetInfo.ownedSF",
  "ResetInfo.ownedAugs",
  "BitNodeOptions.sourceFileOverrides",
  "Sleeve.upgradeMemory(amount)",
  "Sleeve.getMemoryUpgradeCost(amount)",
  "GoAnalysis.getChains",
  "GoAnalysis.getLiberties",
]);
/** Whether `owner`'s `member` (a field or a method's result), or its parameter `param`, is a
 * whole number (`i32`). `owner` is the TS interface, or the struct made for an inline type. */
export function isWholeNumber(owner, member, param) {
  const name = param ?? member;
  if (INT_MEMBERS.has(param === undefined ? `${owner}.${member}` : `${owner}.${member}(${param})`)) return true;
  if (/Multipliers$|Constants$|^Stock|Signatures$/.test(owner) && !/^Max(Level|Cores|Servers|Cache)$/.test(name)) return false;
  if (/hashes$|shares|Ram(?=[A-Z]|$)|^ram|RAM|security|wanted|intelligence|population|rank$|productionLimit/i.test(name)) return false;
  if (/Skills$/.test(owner) && param === undefined) return true;
  if (INT_NAMES.has(name)) return true;
  if (/(^|[a-z])[nN]um[A-Z]/.test(name)) return true;
  if (/Count$|Levels?$|Limit$|TeamSize$|^required\w+Skill$|^Max(Level|Cores|Servers|Cache)$/.test(name)) return true;
  if (/^Go|^ActiveFragment$|^Fragment$/.test(owner) && /^(x|y|x1|y1|x2|y2|id|limit|previousMove)$/.test(name)) return true;
  if (/^(Office|OfficeAPI)$/.test(owner) && /^(size|amount)$/.test(name)) return true;
  return false;
}
// While set, `number` maps to `i32` (a whole-number member's type, unions and tuples included).
let wholeNumber = false;
function asWholeNumber(on, f) {
  const prev = wholeNumber;
  wholeNumber = prev || on;
  try {
    return f();
  } finally {
    wholeNumber = prev;
  }
}

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
  let docs;
  try {
    docs = ts.getJSDocCommentsAndTags(node).filter(ts.isJSDoc);
  } catch {
    return [];
  }
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

/** The source text of a type node (one line), or a rendering for a node made here. */
function textOf(node) {
  try {
    if (node.pos >= 0) return node.getText().replace(/\s+/g, " ");
  } catch {
    // (a synthesized node)
  }
  if (ts.isLiteralTypeNode(node) && ts.isStringLiteral(node.literal)) return JSON.stringify(node.literal.text);
  return "?";
}

export function dtsToRust(text) {
  needTs5();
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
  // (React's types are `react::Node`, not structs of their own.)
  interfaces.delete("ReactElement");
  aliases.delete("ReactNode");
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
  // Structs made for inline object types and intersections: name -> { members, extends, env }.
  const synthesized = new Map();
  const synthByText = new Map();
  // Enums made for unions: name -> { repr: "untagged" | { tag }, variants, doc, methods }.
  const enums = new Map();
  const enumByText = new Map();
  const pascal = (s) =>
    snakeCase(s)
      .replace(/^r#/, "")
      .split("_")
      .filter(Boolean)
      .map((w) => w[0].toUpperCase() + w.slice(1))
      .join("");
  // (A generic alias never becomes a type here, so its name is free: `SuccessResult`.)
  const taken = (name) =>
    interfaces.has(name) ||
    (aliases.has(name) && !aliases.get(name).typeParameters) ||
    synthesized.has(name) ||
    enums.has(name) ||
    RESERVED_TYPE_NAMES.has(name);
  /** A Rust name from a TS key (`"Total Ways to Sum II"` → `TotalWaysToSumII`): words
   * capitalized, short all-caps words (roman numerals, `RLE`) kept, accents dropped. */
  const titleCase = (s) =>
    s
      .normalize("NFD")
      .replace(/[̀-ͯ]/g, "")
      .split(/[^A-Za-z0-9]+/)
      .filter(Boolean)
      .map((w) => (/^[A-Z0-9]{1,3}$/.test(w) ? w : w[0].toUpperCase() + w.slice(1)))
      .join("");
  // (Names a bare pattern would take for the prelude's.)
  const PRELUDE_NAMES = new Set(["Some", "None", "Ok", "Err", "Self", "Option", "Result", "Vec", "String", "Box"]);
  const envKey = (env) => (env ? JSON.stringify(Object.fromEntries(Object.entries(env).map(([k, v]) => [k, textOf(v)]))) : "");
  /** A struct for an inline object type (or an intersection), named `hint` unless the same type
   * was already given a name. `env` resolves a mapped type's parameter in its members. */
  function synthesize(node, members, bases, hint, env) {
    const key = textOf(node) + envKey(env);
    if (synthByText.has(key)) return synthByText.get(key);
    let name = hint;
    for (let i = 2; taken(name); i++) name = `${hint}${i}`;
    synthByText.set(key, name);
    synthesized.set(name, { members, bases, env });
    return name;
  }
  const isStructName = (ty) => isStruct(ty) || synthesized.has(ty);
  const envOf = (name) => synthesized.get(name)?.env;
  /** `[A, B]` element types. */
  const tupleElems = (node) => node.elements.map((e) => (ts.isNamedTupleMember(e) ? e.type : e));
  const isNullish = (t) =>
    t.kind === ts.SyntaxKind.UndefinedKeyword ||
    t.kind === ts.SyntaxKind.NullKeyword ||
    (ts.isLiteralTypeNode(t) && t.literal.kind === ts.SyntaxKind.NullKeyword);

  /** A type node with a mapped type's parameter, an alias and an indexed access (`X["k"]`,
   * `[A, B][0]`) resolved, one level. Null when the access names nothing. */
  function resolveTypeNode(node, env) {
    if (ts.isParenthesizedTypeNode(node)) return resolveTypeNode(node.type, env);
    if (ts.isTypeReferenceNode(node) && ts.isIdentifier(node.typeName) && !node.typeArguments) {
      const n = node.typeName.text;
      if (env && env[n]) return env[n];
      const a = aliases.get(n);
      if (a && !a.typeParameters) return a.type;
      return node;
    }
    if (ts.isIndexedAccessTypeNode(node)) {
      const obj = resolveTypeNode(node.objectType, env);
      const idx = resolveTypeNode(node.indexType, env);
      if (!obj || !idx || !ts.isLiteralTypeNode(idx)) return null;
      if (ts.isStringLiteral(idx.literal)) {
        const key = idx.literal.text;
        const members = ts.isTypeLiteralNode(obj)
          ? obj.members
          : ts.isTypeReferenceNode(obj) && ts.isIdentifier(obj.typeName)
            ? membersOf(obj.typeName.text)
            : [];
        const m = members.find((m) => ts.isPropertySignature(m) && memberName(m) === key);
        return m?.type ?? null;
      }
      if (ts.isNumericLiteral(idx.literal) && ts.isTupleTypeNode(obj)) {
        return tupleElems(obj)[Number(idx.literal.text)] ?? null;
      }
      return null;
    }
    return node;
  }

  /** The literal a member's type is (`"money"`, `true`), resolving a mapped type's parameter. */
  function literalOf(typeNode, env) {
    const t = typeNode && resolveTypeNode(typeNode, env);
    if (!t || !ts.isLiteralTypeNode(t)) return undefined;
    if (ts.isStringLiteral(t.literal)) return t.literal.text;
    if (t.literal.kind === ts.SyntaxKind.TrueKeyword) return true;
    if (t.literal.kind === ts.SyntaxKind.FalseKeyword) return false;
    return undefined;
  }

  /** What JS calls values of a Rust type from `map`: the `typeof`, or `array`; null when it
   * can't be told apart (`JsValue`). */
  function jsKind(ty) {
    if (ty === "String") return "string";
    if (ty === "f64" || ty === "i32") return "number";
    if (ty === "bool") return "boolean";
    if (ty === "i128") return "bigint";
    if (/^Vec<|^\[.*; \d+\]$|^\(.*\)$/.test(ty)) return "array";
    if (/^impl Fn/.test(ty)) return "function";
    if (ty === "JsValue" || ty === "()") return null;
    return "object";
  }
  const VARIANT_BY_KIND = { string: "Str", number: "Num", boolean: "Bool", bigint: "BigInt", array: "List", function: "Fn" };
  function variantName(ty, kind) {
    if (kind !== "object") return VARIANT_BY_KIND[kind];
    if (ty === "ReactNode") return "Node";
    if (/^HashMap</.test(ty)) return "Map";
    return ty;
  }

  /** The field every struct in `names` declares with a distinct literal type: `{ key, values }`. */
  function commonTag(names) {
    const first = membersOf(names[0]).filter(ts.isPropertySignature);
    for (const m of first) {
      const key = memberName(m);
      if (!key) continue;
      const values = new Map();
      let ok = true;
      for (const n of names) {
        const member = membersOf(n).find((x) => ts.isPropertySignature(x) && memberName(x) === key);
        const lit = member && literalOf(member.type, envOf(n));
        if (lit === undefined || [...values.values()].includes(lit)) {
          ok = false;
          break;
        }
        values.set(n, lit);
      }
      if (ok) return { key, values };
    }
    return null;
  }

  const commonPrefix = (names) => {
    let p = names[0];
    for (const n of names) while (!n.startsWith(p)) p = p.slice(0, -1);
    return p;
  };
  const commonSuffix = (names) => {
    let s = names[0];
    for (const n of names) while (!n.endsWith(s)) s = s.slice(1);
    return s;
  };

  /** Registers an enum for a union whose members map to the Rust types `tys` (text: the TS
   * union, for its doc). Returns its name, or null when the members can't be told apart. */
  function enumFor(tys, hint, key, tsText) {
    if (enumByText.has(key)) return enumByText.get(key);
    let def = null;
    if (tys.length > 1 && tys.every(isStructName)) {
      const tag = commonTag(tys);
      if (tag) {
        const prefix = commonPrefix(tys);
        const suffix = commonSuffix(tys);
        const variants = tys.map((n) => {
          let v = n.slice(prefix.length, n.length - suffix.length);
          if (!v || tys.filter((o) => o.slice(prefix.length, o.length - suffix.length) === v).length > 1) v = n;
          if (PRELUDE_NAMES.has(v)) v = n;
          return { name: pascal(v), ty: n, js: tag.values.get(n) };
        });
        def = { repr: { tag: tag.key }, variants };
      }
    }
    if (!def) {
      const kinds = tys.map(jsKind);
      if (kinds.includes(null) || new Set(kinds).size !== kinds.length) return null;
      def = { repr: "untagged", variants: tys.map((t, i) => ({ name: variantName(t, kinds[i]), ty: t })) };
    }
    let name = hint;
    if (!name) return null;
    for (let i = 2; taken(name) && !(aliases.has(name) && name === hint && !enums.has(name)); i++) name = `${hint}${i}`;
    enumByText.set(key, name);
    def.doc = [`TypeScript: \`${tsText}\``];
    enums.set(name, def);
    return name;
  }

  /** Bitburner's `Result<T>` (`SuccessResult<T> | FailureResult` by `success`). */
  function nsResult() {
    if (enums.has("NsResult")) return "NsResult";
    const success = aliases.get("SuccessResult");
    const lit = success && ts.isIntersectionTypeNode(success.type) ? success.type.types.find(ts.isTypeLiteralNode) : null;
    if (!lit || !isStruct("FailureResult")) return "JsValue";
    const ok = synthesize(lit, [...lit.members], [], "SuccessResult");
    enums.set("NsResult", {
      repr: { tag: "success" },
      variants: [
        { name: "Success", ty: ok, js: true },
        { name: "Failure", ty: "FailureResult", js: false },
      ],
      doc: ["TypeScript: `SuccessResult<T> | FailureResult` (Bitburner's `Result`)"],
      methods: [
        {
          rust: "into_result",
          js: "nsResult",
          sig: `(self) -> Result<${ok}, String>`,
          doc: "The result as a Rust `Result`: `Ok(success)` or `Err(message)`, so `?` works.",
        },
      ],
    });
    return "NsResult";
  }

  /** `{ [T in keyof Sig]: {..} }[keyof Sig]` (`CodingContractObject`): one struct per key, with
   * the key for `T`, and the enum over them by the field that names the key. */
  function expandMapped(alias, name) {
    if (enums.has(name)) return name;
    const access = alias.type;
    if (!ts.isIndexedAccessTypeNode(access) || !ts.isMappedTypeNode(access.objectType)) return null;
    const mapped = access.objectType;
    const constraint = mapped.typeParameter.constraint;
    if (!constraint || !ts.isTypeOperatorNode(constraint) || !ts.isTypeReferenceNode(constraint.type)) return null;
    const sig = resolveTypeNode(constraint.type, undefined);
    if (!sig || !ts.isTypeLiteralNode(sig) || !mapped.type || !ts.isTypeLiteralNode(mapped.type)) return null;
    const param = mapped.typeParameter.name.text;
    const tys = [];
    const keys = [];
    for (const m of sig.members) {
      const key = memberName(m);
      if (!key) continue;
      const env = { [param]: ts.factory.createLiteralTypeNode(ts.factory.createStringLiteral(key)) };
      tys.push(synthesize(mapped.type, [...mapped.type.members], [], `${titleCase(key)}Contract`, env));
      keys.push(key);
    }
    const tag = commonTag(tys);
    if (!tag) return null;
    enums.set(name, {
      repr: { tag: tag.key },
      variants: tys.map((t, i) => ({ name: titleCase(keys[i]), ty: t, js: tag.values.get(t) })),
      doc: [`TypeScript: \`${textOf(alias.type).slice(0, 120)}..\` (one variant per contract)`],
    });
    return name;
  }

  const resolving = new Set();
  /** Map a TS type to `{ ty, optional, array? }` (`optional`: the type admits undefined/null).
   * `hint` names a struct or enum made for an inline type (none: such types are `JsValue`);
   * `env` resolves a mapped type's parameter. */
  function map(node, hint, env) {
    if (!node) return { ty: "JsValue", optional: false };
    switch (node.kind) {
      case ts.SyntaxKind.NumberKeyword:
        return { ty: wholeNumber ? "i32" : "f64", optional: false };
      case ts.SyntaxKind.StringKeyword:
        return { ty: "String", optional: false };
      case ts.SyntaxKind.BooleanKeyword:
        return { ty: "bool", optional: false };
      case ts.SyntaxKind.BigIntKeyword:
        return { ty: "i128", optional: false };
      case ts.SyntaxKind.VoidKeyword:
        return { ty: "()", optional: false };
      case ts.SyntaxKind.NeverKeyword:
        return { ty: "!", optional: false };
      case ts.SyntaxKind.UndefinedKeyword:
      case ts.SyntaxKind.NullKeyword:
        return { ty: "()", optional: true };
    }
    if (ts.isParenthesizedTypeNode(node)) return map(node.type, hint, env);
    if (ts.isTypeOperatorNode(node) && node.operator === ts.SyntaxKind.ReadonlyKeyword) return map(node.type, hint, env);
    if (ts.isArrayTypeNode(node)) return { ty: `Vec<${map(node.elementType, hint, env).ty}>`, optional: false };
    // `[number, number]` → `[f64; 2]`; `[number, string]` → `(f64, String)`.
    if (ts.isTupleTypeNode(node)) {
      const elems = tupleElems(node);
      if (elems.length === 0 || elems.some((e) => ts.isRestTypeNode(e) || ts.isOptionalTypeNode(e))) {
        return { ty: "JsValue", optional: false };
      }
      const tys = elems.map((e) => map(e, hint, env)).map((m) => (m.optional ? `Option<${m.ty}>` : m.ty));
      if (tys.includes("JsValue") || tys.includes("()")) return { ty: "JsValue", optional: false };
      const same = tys.every((t) => t === tys[0]);
      return { ty: same ? `[${tys[0]}; ${tys.length}]` : `(${tys.join(", ")})`, optional: false };
    }
    // An inline object type: a struct of its own.
    if (ts.isTypeLiteralNode(node) && hint) {
      if (!node.members.every((m) => ts.isPropertySignature(m) || ts.isMethodSignature(m))) {
        return { ty: "JsValue", optional: false };
      }
      return { ty: synthesize(node, [...node.members], [], hint, env), optional: false };
    }
    if (ts.isLiteralTypeNode(node)) {
      const l = node.literal;
      if (ts.isStringLiteral(l)) return { ty: "String", optional: false };
      if (ts.isNumericLiteral(l) || ts.isPrefixUnaryExpression(l)) return { ty: wholeNumber ? "i32" : "f64", optional: false };
      if (l.kind === ts.SyntaxKind.TrueKeyword || l.kind === ts.SyntaxKind.FalseKeyword) return { ty: "bool", optional: false };
      if (l.kind === ts.SyntaxKind.NullKeyword) return { ty: "()", optional: true };
      return { ty: "JsValue", optional: false };
    }
    // `(x: number) => void`: a callback parameter.
    if (ts.isFunctionTypeNode(node)) {
      const params = node.parameters.map((p) => map(p.type, undefined, env).ty);
      const ret = map(node.type, undefined, env).ty;
      return { ty: `impl Fn(${params.join(", ")})${ret === "()" ? "" : ` -> ${ret}`}`, optional: false };
    }
    if (ts.isIndexedAccessTypeNode(node)) {
      const r = resolveTypeNode(node, env);
      return r ? map(r, hint, env) : { ty: "JsValue", optional: false };
    }
    if (ts.isUnionTypeNode(node)) {
      const members = node.types.filter((t) => !isNullish(t));
      const optional = members.length !== node.types.length;
      // `Sig[T][1] | string` (a contract's `submit(answer)`): the answer type itself; its text
      // form is a convenience the typed binding doesn't need.
      if (env && members.length === 2) {
        const indexed = members.find(ts.isIndexedAccessTypeNode);
        if (indexed && members.some((t) => t.kind === ts.SyntaxKind.StringKeyword)) {
          return { ...map(indexed, hint, env), optional };
        }
      }
      const parts = members.map((t) => map(t, hint, env));
      const tys = [...new Set(parts.filter((p) => !(p.optional && p.ty === "()")).map((p) => p.ty))];
      if (tys.length === 0) return { ty: "()", optional };
      if (tys.length === 1) return { ty: tys[0], optional };
      // `string | ReactElement`: a node (text is one).
      if (tys.includes("ReactNode") && tys.every((t) => ["ReactNode", "String", "f64", "bool", "Vec<ReactNode>"].includes(t))) {
        return { ty: "ReactNode", optional };
      }
      // (Keyed by where it appears too: an anonymous union is a type of its own per owner.)
      const name = enumFor(tys, hint, `${hint}:${textOf(node)}${envKey(env)}`, textOf(node));
      return { ty: name ?? "JsValue", optional };
    }
    // `Server & { isOnline?: boolean }`: `ServerWithIsOnline`, extending `Server` (without a
    // name for it, the named interface).
    if (ts.isIntersectionTypeNode(node)) {
      const named = node.types.map((t) => map(t, undefined, env)).find((m) => isStruct(m.ty));
      const literals = node.types.filter(ts.isTypeLiteralNode);
      const refs = node.types.filter((t) => ts.isTypeReferenceNode(t) && ts.isIdentifier(t.typeName));
      if (named && hint && literals.length && refs.length + literals.length === node.types.length) {
        const bases = refs.map((t) => t.typeName.text).filter(isStruct);
        if (bases.length === refs.length) {
          const extra = literals.flatMap((l) => l.members);
          const members = [...bases.flatMap((b) => membersOf(b)), ...extra];
          const extraNames = extra.map(memberName).filter(Boolean).map(pascal).join("");
          const name = synthesize(node, members, bases, `${bases[0]}With${extraNames || "Extra"}`, env);
          return { ty: name, optional: false };
        }
      }
      return named ?? { ty: "JsValue", optional: false };
    }
    if (ts.isTypeReferenceNode(node)) {
      if (!ts.isIdentifier(node.typeName)) return { ty: "JsValue", optional: false };
      const name = node.typeName.text;
      const args = node.typeArguments ?? [];
      if (env && args.length === 0 && env[name]) return map(env[name], hint, env);
      if (name === "ReactNode" || name === "ReactElement") return { ty: "ReactNode", optional: name === "ReactNode" };
      if ((name === "Array" || name === "ReadonlyArray") && args.length === 1) {
        return { ty: `Vec<${map(args[0], hint, env).ty}>`, optional: false };
      }
      // `Map<number, number>`, `Record<string, T>`, `Partial<Record<K, T>>`: a `HashMap` (string
      // keys for every key type but `number`, whose keys are `i32`).
      if ((name === "Map" || name === "Record") && args.length === 2) {
        const k = map(args[0], undefined, env);
        const v = map(args[1], hint, env);
        const key = k.ty === "f64" || k.ty === "i32" ? "i32" : k.ty === "String" ? "String" : null;
        if (!key || v.ty === "JsValue" || v.ty === "()") return { ty: "JsValue", optional: false };
        return { ty: `HashMap<${key}, ${v.optional ? `Option<${v.ty}>` : v.ty}>`, optional: false };
      }
      if (name === "Partial" && args.length === 1 && ts.isTypeReferenceNode(args[0]) && ts.isIdentifier(args[0].typeName)) {
        const inner = args[0].typeName.text;
        if (inner === "Record") return map(args[0], hint, env);
        // `Partial<Skills>`: the struct with every field optional.
        if (isStruct(inner)) {
          const members = membersOf(inner).map((m) => {
            if (!ts.isPropertySignature(m) || m.questionToken) return m;
            const o = Object.create(m);
            o.questionToken = ts.factory.createToken(ts.SyntaxKind.QuestionToken);
            return o;
          });
          return { ty: synthesize(node, members, [], `Partial${inner}`), optional: false };
        }
      }
      // `Exclude<Union, "Lit">` over string literals: the strings.
      if (name === "Exclude" && args.length >= 1) return map(args[0], hint, env);
      // A `Promise<void>` property (a task's `nextCompletion`): awaitable.
      if (name === "Promise") {
        const inner = args[0] ? map(args[0], hint, env) : { ty: "()" };
        return { ty: `impl Future<Output = ${inner.optional ? `Option<${inner.ty}>` : inner.ty}>`, optional: false };
      }
      if (name === "Date") return { ty: "Date", optional: false };
      if (name === "Uint8Array") return { ty: "Vec<u8>", optional: false, array: true };
      if (name === "Result") return { ty: nsResult(), optional: false };
      if (name === "_ValueOf" && args.length === 1) return enumValue(args[0]);
      if (args.length === 0 && isStruct(name)) return { ty: name, optional: false };
      if (args.length === 0 && enums.has(name)) return { ty: name, optional: false };
      const alias = aliases.get(name);
      if (alias && !alias.typeParameters && args.length === 0 && !resolving.has(name)) {
        resolving.add(name);
        let r;
        if (ts.isUnionTypeNode(alias.type)) {
          // A union alias: an enum named after it (`ScriptArg`), or `String` for literals. (A
          // `FilenameOrPID`'s number is a pid.)
          const parts = asWholeNumber(/PID$/.test(name), () =>
            alias.type.types.filter((t) => !isNullish(t)).map((t) => map(t, name, env)),
          );
          const tys = [...new Set(parts.map((p) => p.ty))];
          const optional = parts.length !== alias.type.types.length;
          if (tys.length === 1) r = { ty: tys[0], optional };
          else r = { ty: enumFor(tys, name, `alias:${name}`, textOf(alias.type)) ?? "JsValue", optional };
        } else if (ts.isIndexedAccessTypeNode(alias.type) && ts.isMappedTypeNode(alias.type.objectType)) {
          r = { ty: expandMapped(alias, name) ?? "JsValue", optional: false };
        } else {
          r = map(alias.type, hint, env);
        }
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

  /** `hint`: names an enum or struct made for a parameter's inline type (`RunThreadOrOptions`). */
  function paramList(sig, hint, env, owner, member) {
    return sig.parameters.map((p, i) => {
      const name = ts.isIdentifier(p.name) ? snakeCase(p.name.text) : `arg${i}`;
      const ph = hint ? `${hint}${pascal(name)}` : undefined;
      if (p.dotDotDotToken) {
        const t = p.type && ts.isArrayTypeNode(p.type) ? map(p.type.elementType, ph, env).ty : "JsValue";
        return { name, ty: t, optional: true, variadic: true };
      }
      const whole = ts.isIdentifier(p.name) && isWholeNumber(owner, member, p.name.text);
      const m = asWholeNumber(whole, () => map(p.type, ph, env));
      return { name, ty: m.ty === "()" ? "JsValue" : m.ty, optional: !!p.questionToken || !!p.initializer || m.optional };
    });
  }

  function returnOf(sig, hint, env, owner, member) {
    const whole = isWholeNumber(owner, member);
    const t = sig.type;
    if (t && ts.isTypeReferenceNode(t) && ts.isIdentifier(t.typeName) && t.typeName.text === "Promise") {
      const inner = t.typeArguments?.[0]
        ? asWholeNumber(whole, () => map(t.typeArguments[0], hint, env))
        : { ty: "()", optional: false };
      return { ...inner, isAsync: true };
    }
    return { ...asWholeNumber(whole, () => map(t, hint, env)), isAsync: false };
  }

  /** One Rust signature for a set of TS overloads. `owner` and `member` name the method, for
   * `isWholeNumber`. */
  function mergeOverloads(sigs, hint, env, owner, member) {
    if (sigs.length === 1) {
      return { params: paramList(sigs[0], hint, env, owner, member), ret: returnOf(sigs[0], `${hint}Result`, env, owner, member) };
    }
    const lists = sigs.map((s) => paramList(s, hint, env, owner, member));
    const longest = lists.reduce((a, b) => (b.length > a.length ? b : a));
    const minLen = Math.min(...lists.map((l) => l.filter((p) => !p.optional).length));
    const params = longest.map((p, i) => {
      const tys = [...new Set(lists.filter((l) => l[i]).map((l) => l[i].ty))];
      // (Overloads that disagree on a parameter's type: an enum of them, `KillFilename`.)
      const ty =
        tys.length === 1 ? p.ty : (enumFor(tys, `${hint}${pascal(p.name)}`, `overloads:${hint}:${p.name}:${tys.join("|")}`, tys.join(" | ")) ?? "JsValue");
      return { ...p, ty, optional: p.optional || i >= minLen };
    });
    const rets = sigs.map((sg) => returnOf(sg, `${hint}Result`, env, owner, member));
    const sameRet = rets.every((r) => r.ty === rets[0].ty && r.isAsync === rets[0].isAsync);
    return { params, ret: sameRet ? rets[0] : { ty: "JsValue", optional: false, isAsync: rets.every((r) => r.isAsync) } };
  }

  const rustType = (m, asField) => (m.optional && asField ? `Option<${m.ty}>` : m.ty);
  /** A parameter's type: strings and arrays are borrowed (no NS function changes or keeps the
   * ones it's given), so `&str` for `String` and `&[T]` for `Vec<T>`. */
  const paramType = (ty) => {
    if (ty === "String") return "&str";
    const vec = ty.match(/^Vec<(.*)>$/);
    if (vec) return `&[${vec[1]}]`;
    if (/^\[.*; \d+\]$/.test(ty)) return `&${ty}`;
    return ty;
  };
  const docLines = (node, indent) => docOf(node).map((l) => `${indent}/// ${l}\n`).join("");

  function emitType(name) {
    const env = envOf(name);
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
        const t = asWholeNumber(isWholeNumber(name, js), () => map(m.type, `${name}${pascal(js)}`, env));
        // (A field that may hold a node, like a script's `title`, is read as any JS value.)
        if (t.ty === "ReactNode") t.ty = "JsValue";
        const ty = rustType({ ...t, optional: t.optional || !!m.questionToken }, true);
        const array = t.array ? "    #[array]\n" : "";
        fields.push(`${docLines(m, "    ")}    #[js = ${JSON.stringify(js)}]\n${array}    pub ${rust}: ${ty},\n`);
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
      if (js === "flags" && (name === "NS" || name === "AutocompleteData")) {
        fns.push(`${docLines(overloads[0].node, "    ")}${name === "NS" ? FLAGS_DOC : FLAGS_DOC.replace("ns.flags", "data.flags")}    #[js = "flags"]\n    pub fn flags(&self, schema: &[(&str, ${flagValue()})]) -> Flags;\n`);
        continue;
      }
      const { params, ret } = mergeOverloads(
        overloads.map((o) => o.sig),
        `${name === "NS" ? "" : name}${pascal(js)}`,
        env,
        name,
        js,
      );
      const ps = params.map((p) => {
        const attr = p.variadic ? "#[variadic] " : p.optional ? "#[optional] " : "";
        return `${attr}${p.name}: ${p.variadic ? p.ty : paramType(p.ty)}`;
      });
      const retTy = ret.ty === "()" ? "" : ` -> ${rustType(ret, true)}`;
      const asyncKw = ret.isAsync ? "async " : "";
      const array = ret.array ? "    #[array]\n" : "";
      fns.push(
        `${docLines(overloads[0].node, "    ")}    #[js = ${JSON.stringify(js)}]\n${array}    pub ${asyncKw}fn ${rust}(&self${ps.map((p) => `, ${p}`).join("")})${retTy};\n`,
      );
    }
    if (fns.length) out += `impl ${name} {\n${fns.join("")}}\n`;
    return out;
  }

  // `ns.flags(schema)`: the d.ts says `[string, any][]` and an index signature, so the schema's
  // defaults are a `FlagValue` (what the game parses by: a string, number, boolean or string
  // list) and the result is `Flags`, read by name through the runtime (`κσ.$jsv.$flagStr`).
  const FLAGS_DOC = [
    "",
    "In Rust: `let f = ns.flags(&[(\"target\", \"n00dles\"), (\"threads\", 1), (\"loop\", false), (\"exclude\", vec![])]);`",
    "then `f.get_str(\"target\")`, `f.get_f64(\"threads\")`, `f.get_bool(\"loop\")`, `f.get_list(\"exclude\")` and",
    "`f.rest` (the arguments that aren't flags). For typed fields, `--help` and error messages, clap's",
    "`#[derive(Parser)]` reads the script's arguments too (`Args::parse()`).",
  ]
    .map((l) => `    ///${l && " "}${l}\n`)
    .join("");
  function flagValue() {
    return enumFor(["String", "f64", "bool", "Vec<String>"], "FlagValue", "flags", "string | number | boolean | string[]");
  }
  const FLAGS_STRUCT = [
    "/// The result of `ns.flags(schema)`: each flag's value by name, and the other arguments.",
    "#[debug_keys]",
    "pub struct Flags {",
    "    /// The arguments that aren't flags (`run x.rs --threads 4 n00dles` has `[\"n00dles\"]`).",
    '    #[js = "_"]',
    "    pub rest: Vec<ScriptArg>,",
    "}",
    "impl Flags {",
    ...[
      ["get", "flag", "Option<FlagValue>", "The flag's value, if the schema names it."],
      ["get_str", "flagStr", "Option<String>", "The flag's value, if it's a string."],
      ["get_f64", "flagF64", "Option<f64>", "The flag's value, if it's a number."],
      ["get_bool", "flagBool", "Option<bool>", "The flag's value, if it's a boolean."],
      ["get_list", "flagList", "Option<Vec<String>>", "The flag's values, if it's a list (a `vec![]` default; `--x a --x b`)."],
    ].map(([rust, js, ret, doc]) => `    /// ${doc}\n    #[js = "${js}"]\n    #[runtime]\n    pub fn ${rust}(&self, name: &str) -> ${ret};`),
    "}",
    "",
  ].join("\n");

  /** The readers an untagged enum gets, by the JS kinds of its payloads (what `JsValue` has). */
  const READERS = {
    string: [
      ["as_string", "asString", "Option<String>", "The string, if the value is one."],
      ["is_string", "isString", "bool", "Whether the value is a string."],
    ],
    number: [["as_f64", "asF64", "Option<f64>", "The number, if the value is one."]],
    boolean: [["as_bool", "asBool", "Option<bool>", "The boolean, if the value is one."]],
    array: [["is_array", "isArray", "bool", "Whether the value is an array."]],
    object: [["is_object", "isObject", "bool", "Whether the value is an object (not `null`)."]],
  };

  function emitEnum(name) {
    const def = enums.get(name);
    const attr = def.repr === "untagged" ? "#[untagged]" : `#[tag = ${JSON.stringify(def.repr.tag)}]`;
    const variants = def.variants.map((v) => {
      const js = v.js === undefined ? "" : `    #[js = ${typeof v.js === "string" ? JSON.stringify(v.js) : v.js}]\n`;
      return `${js}    ${v.name}(${v.ty}),\n`;
    });
    let out = `${def.doc.map((l) => `/// ${l}\n`).join("")}${attr}\npub enum ${name} {\n${variants.join("")}}\n`;
    const fns = [];
    if (def.repr === "untagged") {
      const seen = new Set();
      for (const v of def.variants) {
        for (const [rust, js, ret, doc] of READERS[jsKind(v.ty)] ?? []) {
          if (seen.has(rust)) continue;
          seen.add(rust);
          fns.push(`    /// ${doc}\n    #[js = ${JSON.stringify(js)}]\n    pub fn ${rust}(&self) -> ${ret};\n`);
        }
      }
      fns.push(`    /// The value as JS text (\`"n00dles"\`, \`"5"\`, \`"true"\`), as \`{}\` prints it.\n    #[js = "asText"]\n    pub fn to_string(&self) -> String;\n`);
    }
    for (const m of def.methods ?? []) {
      fns.push(`    /// ${m.doc}\n    #[js = ${JSON.stringify(m.js)}]\n    pub fn ${m.rust}${m.sig};\n`);
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
    "// parameter takes any number of arguments. An `enum` is a TS union: its value is the JS value\n" +
    "// itself (`#[untagged]`: by its JS type; `#[tag = \"f\"]`: by the field `f`), and a parameter of it\n" +
    "// also takes any variant's payload directly.\n\n";
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
  if (enums.has("FlagValue")) emitted.push(FLAGS_STRUCT);
  for (const n of [...enums.keys()].sort()) emitted.push(emitEnum(n));
  return header + emitted.join("\n");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [input, output] = process.argv.slice(2);
  if (!input) {
    console.error("usage: node tools/dts2rs.mjs <NetscriptDefinitions.d.ts> [out.rs]");
    process.exit(2);
  }
  const rust = dtsToRust(fs.readFileSync(input, "utf8"));
  if (output) fs.writeFileSync(output, rust);
  else process.stdout.write(rust);
}
