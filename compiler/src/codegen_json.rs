//! The `js` crate's static functions: `Date::now()` and friends as `globalThis.Date` calls, and
//! `JSON::stringify` / `JSON::parse::<T>` through the runtime with a schema of the type (what
//! serde's derives know: field names, variants, `#[serde(..)]` renames). A child module of
//! `codegen`.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList},
};

use super::{Codegen, Frag, default_value, js_string};
use crate::adt::Shape;
use crate::ty::{Coll, IntKind, Ty};

/// `Vec<T>` with `T` replaced: a generic struct's field types for its arguments.
fn subst(t: &Ty, m: &HashMap<String, Ty>) -> Ty {
    let b = |x: &Ty| Box::new(subst(x, m));
    match t {
        Ty::Param(p) => m.get(p).cloned().unwrap_or(Ty::Unknown),
        Ty::Vec(e) => Ty::Vec(b(e)),
        Ty::Option(e) => Ty::Option(b(e)),
        Ty::Set(e) => Ty::Set(b(e)),
        Ty::RefCell(e) => Ty::RefCell(b(e)),
        Ty::Coll(k, v) => Ty::Coll(*k, b(v)),
        Ty::Map(k, v) => Ty::Map(b(k), b(v)),
        Ty::Result(a, e) => Ty::Result(b(a), b(e)),
        Ty::Tuple(es) => Ty::Tuple(es.iter().map(|x| subst(x, m)).collect()),
        Ty::Adt(n, args) => Ty::Adt(n.clone(), args.iter().map(|x| subst(x, m)).collect()),
        other => other.clone(),
    }
}

fn int_name(k: IntKind) -> &'static str {
    match k {
        IntKind::I8 => "i8",
        IntKind::I16 => "i16",
        IntKind::I32 => "i32",
        IntKind::I64 => "i64",
        IntKind::I128 => "i128",
        IntKind::Isize => "isize",
        IntKind::U8 => "u8",
        IntKind::U16 => "u16",
        IntKind::U32 => "u32",
        IntKind::U64 => "u64",
        IntKind::U128 => "u128",
        IntKind::Usize => "usize",
    }
}

impl Codegen<'_> {
    /// `Date::now()`, `JSON::stringify(&v)`, ..; `None` for other paths.
    pub(super) fn emit_js_call(&mut self, c: &ast::CallExpr, segs: &[&str]) -> Option<(Frag, Ty)> {
        let tail = match segs {
            ["js", rest @ ..] => rest,
            all => all,
        };
        // (`use serde_json::from_str;` then `from_str(..)`.)
        let imported;
        let tail = match tail {
            [f @ ("to_string" | "to_string_pretty" | "to_value" | "from_str" | "from_value")]
                if self
                    .crate_imports
                    .get_or_init(|| crate::typeck::crates::crate_imports(self.src))
                    .get(*f)
                    .is_some_and(|(k, i)| k == "serde_json" && i == f) =>
            {
                imported = ["serde_json", *f];
                &imported[..]
            }
            other => other,
        };
        let start = self.off(c.syntax());
        // (Keeping `BTreeMap` & co. as themselves: the schema builds the right collection.)
        let ty = self.raw_ty_of(c.syntax()).unwrap_or(Ty::Unknown);
        let args: Vec<ast::Expr> = c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let emitted = |cg: &mut Self| -> Vec<Frag> {
            args.iter()
                .map(|a| {
                    let (f, t) = cg.emit_expr(a, None);
                    cg.by_value(a, f, &t)
                })
                .collect()
        };
        let call = |head: &str, parts: Vec<Frag>| {
            let mut f = Frag::mapped(start, format!("{head}("));
            f.append(Frag::join(parts, ", ")).push(")");
            f
        };
        let f = match tail {
            ["Promise", "all"] => call("globalThis.Promise.all", emitted(self)),
            ["Promise", "race"] => call("globalThis.Promise.race", emitted(self)),
            ["JSON", f @ ("stringify" | "stringify_pretty")] => {
                let a = args.first()?;
                let t = self.raw_ty_of(a.syntax()).unwrap_or(Ty::Unknown);
                let (v, _) = self.emit_expr(a, None);
                let schema = self.json_schema_expr(&t);
                call(
                    "κσ.$jsonStringify",
                    vec![
                        v,
                        Frag::text(schema),
                        Frag::text((*f == "stringify_pretty").to_string()),
                    ],
                )
            }
            ["JSON", "parse"] => {
                let target = match &ty {
                    Ty::Result(t, _) => (**t).clone(),
                    _ => Ty::Unknown,
                };
                let schema = self.json_schema_expr(&target);
                let mut parts = emitted(self);
                parts.push(Frag::text(schema));
                call("κσ.$jsonParse", parts)
            }
            // The bundled serde_json: the same, in `Result`s of `serde_json::Error`.
            [
                "serde_json",
                f @ ("to_string" | "to_string_pretty" | "to_value"),
            ] => {
                let a = args.first()?;
                let t = self.raw_ty_of(a.syntax()).unwrap_or(Ty::Unknown);
                let (v, _) = self.emit_expr(a, None);
                let schema = self.json_schema_expr(&t);
                let text = call(
                    "κσ.$jsonStringify",
                    vec![
                        v,
                        Frag::text(schema),
                        Frag::text((*f == "to_string_pretty").to_string()),
                    ],
                );
                if *f == "to_value" {
                    let value = self.json_schema_expr(&Ty::Adt("Value".into(), vec![]));
                    let mut out = Frag::mapped(start, "κσ.$jsonParse(");
                    out.append(text).push(format!(", {value}, \"nopos\")"));
                    out
                } else {
                    text.wrap("κσ.Ok(", ")")
                }
            }
            ["serde_json", f @ ("from_str" | "from_value")] => {
                let target = match &ty {
                    Ty::Result(t, _) => (**t).clone(),
                    _ => Ty::Unknown,
                };
                let schema = self.json_schema_expr(&target);
                let mut parts = emitted(self);
                let mode = if *f == "from_str" {
                    "\"pos\""
                } else {
                    // (A `Value` as its text first.)
                    if let Some(p) = parts.pop() {
                        parts.push(p.wrap("__sj_write(", ", false, \"\")"));
                    }
                    "\"nopos\""
                };
                parts.push(Frag::text(schema));
                parts.push(Frag::text(mode));
                call("κσ.$jsonParse", parts)
            }
            _ => return None,
        };
        Some((f, ty))
    }

    /// A call of the `js` crate's bound functions (resolved by the type checker): the JS callee
    /// with the arguments, the getter's expression, or an array made of an array-like result.
    pub(super) fn emit_js_bound(&mut self, c: &ast::CallExpr) -> Option<(Frag, Ty)> {
        let (owner, fname) = self.types.js_calls.get(&self.range_of(c.syntax()))?.clone();
        let b = crate::bindings::current();
        let sig = match &owner {
            Some(t) => b.types.get(t)?.methods.get(&fname)?,
            None => b.functions.get(&fname)?,
        };
        let start = self.off(c.syntax());
        let ty = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
        let f = if sig.getter {
            Frag::mapped(start, sig.js.clone())
        } else {
            let tys: Vec<Ty> = sig.params.iter().map(|p| p.ty.clone()).collect();
            let args = self.emit_args(c.arg_list(), &tys);
            let mut f = Frag::mapped(start, format!("{}(", sig.js));
            f.append(Frag::join(args, ", ")).push(")");
            f
        };
        let f = if sig.array {
            f.wrap("κσ.$arrayOf(", ")")
        } else {
            f
        };
        Some((f, ty))
    }

    /// The schema of `t` as a JS expression: definitions of structs and enums are lazy and
    /// shared through `$d`, so recursive types (`struct Tree { kids: Vec<Tree> }`) work.
    pub(super) fn json_schema_expr(&mut self, t: &Ty) -> String {
        if self.serde.is_none() {
            self.serde = Some(std::rc::Rc::new(crate::json::collect(self.src)));
        }
        let body = self.json_schema(t, &mut Vec::new());
        format!("(($d) => ({body}))({{}})")
    }

    fn json_schema(&self, t: &Ty, stack: &mut Vec<String>) -> String {
        match t {
            Ty::Int(k) if k.is_big() => {
                let (min, max): (i128, u128) = match k {
                    IntKind::I128 => (i128::MIN, i128::MAX as u128),
                    _ => (0, u128::MAX),
                };
                format!(
                    "{{t:\"big\",name:\"{}\",min:\"{min}\",max:\"{max}\"}}",
                    int_name(*k)
                )
            }
            Ty::Int(k) => {
                // Exact bounds (as BigInt text): JSON numbers can be any 64-bit integer.
                let (min, max): (i128, i128) = match k {
                    IntKind::I8 => (i8::MIN.into(), i8::MAX.into()),
                    IntKind::I16 => (i16::MIN.into(), i16::MAX.into()),
                    IntKind::I32 => (i32::MIN.into(), i32::MAX.into()),
                    IntKind::I64 | IntKind::Isize => (i64::MIN.into(), i64::MAX.into()),
                    IntKind::U8 => (0, u8::MAX.into()),
                    IntKind::U16 => (0, u16::MAX.into()),
                    IntKind::U32 => (0, u32::MAX.into()),
                    _ => (0, u64::MAX.into()),
                };
                format!(
                    "{{t:\"int\",name:\"{}\",min:\"{min}\",max:\"{max}\"}}",
                    int_name(*k)
                )
            }
            Ty::IntLit => "{t:\"int\",name:\"i32\",min:\"-2147483648\",max:\"2147483647\"}".into(),
            Ty::Float { f32 } => format!("{{t:\"float\",f32:{f32}}}"),
            Ty::Bool => "{t:\"bool\"}".into(),
            Ty::Char => "{t:\"char\"}".into(),
            Ty::Str => "{t:\"str\"}".into(),
            Ty::Unit => "{t:\"unit\"}".into(),
            // `T: DeserializeOwned`: the schema of the class the caller passed for `T`.
            Ty::Param(_) => format!("κσ.$schemaOf({})", self.dict_of(t)),
            Ty::Option(e) => format!("{{t:\"opt\",s:{}}}", self.json_schema(e, stack)),
            Ty::Vec(e) => format!("{{t:\"vec\",s:{}}}", self.json_schema(e, stack)),
            Ty::Set(e) => format!(
                "{{t:\"set\",s:{},cls:\"HashSet\"}}",
                self.json_schema(e, stack)
            ),
            Ty::Map(k, v) => format!(
                "{{t:\"map\",k:{},v:{},cls:\"HashMap\"}}",
                self.json_schema(k, stack),
                self.json_schema(v, stack)
            ),
            Ty::Coll(kind, view) => match (kind, &**view) {
                (Coll::VecDeque, Ty::Vec(e)) => format!(
                    "{{t:\"vec\",s:{},cls:\"Deque\"}}",
                    self.json_schema(e, stack)
                ),
                (Coll::BTreeSet, Ty::Set(e)) => format!(
                    "{{t:\"set\",s:{},cls:\"BTreeSet\"}}",
                    self.json_schema(e, stack)
                ),
                (Coll::BTreeMap, Ty::Map(k, v)) => format!(
                    "{{t:\"map\",k:{},v:{},cls:\"BTreeMap\"}}",
                    self.json_schema(k, stack),
                    self.json_schema(v, stack)
                ),
                (_, other) => self.json_schema(other, stack),
            },
            Ty::Tuple(es) => {
                let parts: Vec<String> = es.iter().map(|e| self.json_schema(e, stack)).collect();
                format!("{{t:\"tuple\",s:[{}]}}", parts.join(","))
            }
            Ty::RefCell(e) => format!("{{t:\"cell\",s:{}}}", self.json_schema(e, stack)),
            Ty::Result(a, e) => format!(
                "{{t:\"result\",ok:{},err:{}}}",
                self.json_schema(a, stack),
                self.json_schema(e, stack)
            ),
            // The bundled serde_json's `Value`: made and written by its prelude's functions.
            Ty::Adt(n, _) if n == "Value" && self.types.adts.traits.contains_key("__SjIndex") => {
                "{t:\"jsonvalue\",mk:{nul:__sj_null,bool:__sj_bool,num:__sj_num,str:__sj_str,arr:__sj_arr,obj:__sj_obj},out:__sj_write}".into()
            }
            Ty::Adt(n, args) => {
                let key = format!("{n}{args:?}");
                if stack.contains(&key) {
                    return format!("$d[{}]", js_string(&key));
                }
                let Some(def) = self.types.adts.defs.get(n).cloned() else {
                    return "{t:\"any\"}".into();
                };
                stack.push(key.clone());
                let map: HashMap<String, Ty> = def
                    .type_params
                    .iter()
                    .cloned()
                    .zip(args.iter().cloned())
                    .collect();
                let info = self
                    .serde
                    .as_ref()
                    .and_then(|s| s.get(n).cloned())
                    .unwrap_or_default();
                let fields_js = |cg: &Self,
                                 variant: &str,
                                 v: &crate::adt::Variant,
                                 rule: Option<&str>,
                                 stack: &mut Vec<String>| {
                    let parts: Vec<String> = v
                        .fields
                        .iter()
                        .map(|f| {
                            let ft = subst(&f.ty, &map);
                            let attrs = info
                                .fields
                                .get(&(variant.to_string(), f.name.clone()))
                                .cloned()
                                .unwrap_or_default();
                            let key = attrs
                                .rename
                                .clone()
                                .unwrap_or_else(|| crate::json::rename_field(&f.name, rule));
                            let def = if attrs.default || attrs.skip_deserializing {
                                match &ft {
                                    Ty::Adt(dn, _) => format!("() => {}.default()", cg.class(dn)),
                                    other => format!(
                                        "() => {}",
                                        default_value(other).unwrap_or_else(|| "undefined".into())
                                    ),
                                }
                            } else {
                                "null".into()
                            };
                            // (The property the class has: `ram` is `ram$` where Bitburner
                            // charges for the name.)
                            let js = if v.shape == Shape::Tuple {
                                f.name.clone()
                            } else {
                                super::adt_gen::prop(&f.js)
                            };
                            format!(
                                "{{key:{},js:{},s:{},def:{def},skipOut:{},skipIn:{}{}}}",
                                js_string(&key),
                                js_string(&js),
                                cg.json_schema(&ft, stack),
                                attrs.skip_serializing,
                                attrs.skip_deserializing,
                                if attrs.flatten { ",flatten:true" } else { "" }
                            )
                        })
                        .collect();
                    format!("[{}]", parts.join(","))
                };
                let shape = |s: Shape| match s {
                    Shape::Named => "named",
                    Shape::Tuple => "tuple",
                    Shape::Unit => "unit",
                };
                let cls = self.class(n);
                // `#[serde(tag = "type")]` (and `content = "c"`): the variant's (or struct's)
                // name as a field of the object.
                let tagging = [("tag", &info.tag), ("content", &info.content)]
                    .into_iter()
                    .filter_map(|(k, v)| v.as_ref().map(|v| format!(",{k}:{}", js_string(v))))
                    .collect::<String>();
                let body = if def.is_enum {
                    let vs: Vec<String> = def
                        .variants
                        .iter()
                        .map(|v| {
                            let (rename, own_rule) =
                                info.variants.get(&v.name).cloned().unwrap_or_default();
                            let key = rename.unwrap_or_else(|| {
                                crate::json::rename_variant(&v.name, info.rename_all.as_deref())
                            });
                            format!(
                                "{{key:{},js:{},shape:\"{}\",fields:{}}}",
                                js_string(&key),
                                js_string(&v.name),
                                shape(v.shape),
                                fields_js(self, &v.name, v, own_rule.as_deref(), stack)
                            )
                        })
                        .collect();
                    format!(
                        "{{t:\"enum\",name:{},cls:{cls},variants:[{}]{}{tagging}}}",
                        js_string(n),
                        vs.join(","),
                        if info.untagged { ",untagged:true" } else { "" }
                    )
                } else {
                    let v = &def.variants[0];
                    format!(
                        "{{t:\"struct\",name:{},cls:{cls},shape:\"{}\",fields:{}{tagging}}}",
                        js_string(n),
                        shape(v.shape),
                        fields_js(self, "", v, info.rename_all.as_deref(), stack)
                    )
                };
                stack.pop();
                format!(
                    "($d[{k}] ??= {{t:\"lazy\",get:() => ({body})}})",
                    k = js_string(&key)
                )
            }
            _ => "{t:\"any\"}".into(),
        }
    }
}
