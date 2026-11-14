//! The `js` crate's static functions: `Date::now()`, `Date::new()`, `Date::from_millis(ms)`,
//! `Date::local(..)`, `Date::utc(..)`, `Date::parse(s)`, and `JSON::stringify(&v)`,
//! `JSON::stringify_pretty(&v)`, `JSON::parse::<T>(s)`. JSON goes by the value's static type,
//! like serde (codegen writes a schema of it). A child module of `typeck`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs},
};

use super::{Checker, VarKind};
use crate::ty::Ty;

/// The type `JSON::parse` fails with.
pub fn json_error() -> Ty {
    Ty::Extern("JsonError".into())
}

impl Checker<'_> {
    /// `Date::..` / `JSON::..` (also `js::Date::..`); `None` for other paths.
    pub(super) fn js_call(
        &mut self,
        c: &ast::CallExpr,
        names: &[&str],
        args: &[ast::Expr],
        expected: Option<&Ty>,
    ) -> Option<Ty> {
        let tail = match names {
            ["js", rest @ ..] => rest,
            all => all,
        };
        let count = |c2: &mut Self, lo: usize, hi: usize| {
            if args.len() < lo || args.len() > hi {
                let want = if lo == hi {
                    format!("{lo} argument{}", if lo == 1 { "" } else { "s" })
                } else {
                    format!("{lo} to {hi} arguments")
                };
                let msg = format!(
                    "this function takes {want} but {} {} supplied",
                    args.len(),
                    if args.len() == 1 { "was" } else { "were" }
                );
                c2.error(c.syntax(), msg);
            }
        };
        Some(match tail {
            // `Promise::all(futures)` / `Promise::race(futures)`: one future of every result (or
            // the first).
            ["Promise", f @ ("all" | "race")] => {
                count(self, 1, 1);
                let item = self.fresh(VarKind::General);
                if let Some(a) = args.first() {
                    let t = self.infer(a, None);
                    match self.iter_elem(&t).map(|e| self.zonk(&e)) {
                        Some(Ty::Future(inner)) => {
                            self.unify(&item, &inner);
                        }
                        Some(Ty::Var(_) | Ty::Unknown) => {}
                        _ => {
                            let msg = format!(
                                "`Promise::{f}` takes a `Vec` of futures (unawaited async calls), found `{}`",
                                self.describe(&t)
                            );
                            self.error(a.syntax(), msg);
                        }
                    }
                }
                if *f == "all" {
                    Ty::Future(Box::new(Ty::Vec(Box::new(item))))
                } else {
                    Ty::Future(Box::new(item))
                }
            }
            ["JSON", "stringify" | "stringify_pretty"] => {
                count(self, 1, 1);
                if let Some(a) = args.first() {
                    let t = self.infer(a, None);
                    self.pending_json.push((t, a.syntax().clone(), false));
                }
                Ty::Str
            }
            ["JSON", "parse"] => {
                count(self, 1, 1);
                if let Some(a) = args.first() {
                    self.check(a, &Ty::Str);
                }
                let turbofish = match c.expr() {
                    Some(ast::Expr::PathExpr(p)) => p
                        .path()
                        .and_then(|p| p.segment())
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next())
                        .and_then(|g| match g {
                            ast::GenericArg::TypeArg(ta) => ta.ty(),
                            _ => None,
                        }),
                    _ => None,
                };
                let t = match turbofish {
                    Some(t) => self.lower_type(&t),
                    None => match expected.map(|e| self.shallow(e)) {
                        Some(Ty::Result(t, _)) => *t,
                        _ => self.fresh(VarKind::General),
                    },
                };
                self.pending_json
                    .push((t.clone(), c.syntax().clone(), true));
                Ty::Result(Box::new(t), Box::new(json_error()))
            }
            _ => return None,
        })
    }

    /// A bound function of the `js` crate: `Date::now()` (an associated function of a JS type),
    /// `js::set_timeout(f, ms)` or an imported `set_timeout(f, ms)`.
    pub(super) fn js_bound_call(
        &mut self,
        c: &ast::CallExpr,
        names: &[&str],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let (explicit, tail) = match names {
            ["js", rest @ ..] => (true, rest),
            all => (false, all),
        };
        let b = self.bindings.clone();
        let (owner, fname, sig) = match tail {
            [t, f] if crate::bindings::is_js_type(t) => {
                let et = b.types.get(*t)?;
                let Some(sig) = et.methods.get(*f) else {
                    let hint = crate::bindings::suggest(
                        f,
                        et.methods.keys().filter(|k| et.methods[*k].is_static),
                    )
                    .map(|s| format!("; did you mean `{s}`?"))
                    .unwrap_or_default();
                    let msg =
                        format!("no function or associated item named `{f}` found for `{t}`{hint}");
                    self.error(c.syntax(), msg);
                    for a in args {
                        self.infer(a, None);
                    }
                    return Some(Ty::Unknown);
                };
                if !sig.is_static {
                    let msg = format!("`{t}::{f}` is a method: call it on a value (`x.{f}(..)`)");
                    self.error(c.syntax(), msg);
                }
                (Some(t.to_string()), f.to_string(), sig)
            }
            [f] => {
                let name = if explicit {
                    f.to_string()
                } else {
                    self.js_imports.get(*f)?.clone()
                };
                let sig = b.functions.get(&name)?;
                (None, name, sig)
            }
            _ => return None,
        };
        let label = match &owner {
            Some(t) => format!("{t}::{fname}"),
            None => format!("js::{fname}"),
        };
        // `structured_clone(&x)`: a copy of `x`, so its type; a Rust value needs `Clone`.
        if owner.is_none() && fname == "structured_clone" {
            self.arg_count(c.syntax(), "function", 1, args.len());
            let t = match args.first() {
                Some(a) => self.infer(a, None),
                None => Ty::Unknown,
            };
            let z = self.zonk(&t);
            if !matches!(z, Ty::Unknown | Ty::Extern(_) | Ty::Var(_))
                && !self.implements(&z, "Clone")
            {
                let msg = format!(
                    "the trait bound `{}: Clone` is not satisfied (`structured_clone` copies Rust values by their `Clone`)",
                    self.describe(&z)
                );
                self.error(args[0].syntax(), msg);
            }
            let r = self.range(c.syntax());
            self.js_calls.insert(r, (owner, fname));
            return Some(t);
        }
        self.extern_args(c.syntax(), &label, sig, args);
        let r = self.range(c.syntax());
        self.js_calls.insert(r, (owner, fname));
        Some(if sig.is_async {
            Ty::Future(Box::new(sig.ret.clone()))
        } else {
            sig.ret.clone()
        })
    }

    /// Check what `JSON` was given once types are settled: values serde could handle.
    pub(super) fn resolve_json(&mut self) {
        for (t, node, parse) in std::mem::take(&mut self.pending_json) {
            // (`zonk` turns an unsolved variable into `Unknown`; `shallow` keeps it.)
            if matches!(self.shallow(&t), Ty::Var(_)) {
                self.error(
                    &node,
                    "type annotations needed: say what to parse into, e.g. `JSON::parse::<Config>(..)`",
                );
                continue;
            }
            let z = self.zonk(&t);
            if let Some(bad) = self.json_bad(&z, &mut Vec::new()) {
                let tr = if parse { "Deserialize" } else { "Serialize" };
                let msg = format!(
                    "the trait bound `{}: {tr}` is not satisfied (JSON handles numbers, strings, bools, `Option`, `Vec`, tuples, maps, sets and your structs and enums)",
                    self.describe(&bad)
                );
                self.error(&node, msg);
            }
        }
    }

    /// The first part of `t` JSON can't represent.
    fn json_bad(&self, t: &Ty, seen: &mut Vec<String>) -> Option<Ty> {
        let t = self.zonk(t);
        let all = |c: &Self, ts: &[Ty], seen: &mut Vec<String>| {
            ts.iter().find_map(|x| c.json_bad(x, seen))
        };
        match &t {
            Ty::Int(_)
            | Ty::IntLit
            | Ty::Float { .. }
            | Ty::Bool
            | Ty::Char
            | Ty::Str
            | Ty::Unit => None,
            Ty::Unknown | Ty::Var(_) => None,
            Ty::Extern(n) if n == "JsValue" => None,
            Ty::Vec(e) | Ty::Option(e) | Ty::Set(e) | Ty::RefCell(e) => self.json_bad(e, seen),
            Ty::Coll(_, v) => self.json_bad(v, seen),
            Ty::Tuple(es) => all(self, es, seen),
            Ty::Map(k, v) => {
                // Keys are strings in JSON: strings, chars and integers work.
                if !matches!(
                    self.zonk(k),
                    Ty::Str | Ty::Char | Ty::Int(_) | Ty::IntLit | Ty::Var(_) | Ty::Unknown
                ) {
                    return Some((**k).clone());
                }
                self.json_bad(v, seen)
            }
            Ty::Result(a, b) => all(self, &[(**a).clone(), (**b).clone()], seen),
            Ty::Adt(n, args) => {
                let key = format!("{n}{args:?}");
                if seen.contains(&key) {
                    return None;
                }
                seen.push(key);
                let def = self.adts.defs.get(n)?;
                let map = self.adt_subst(n, args);
                for v in &def.variants {
                    for f in &v.fields {
                        let ft = super::generic::subst(&f.ty, &map);
                        if let Some(bad) = self.json_bad(&ft, seen) {
                            return Some(bad);
                        }
                    }
                }
                None
            }
            _ => Some(t),
        }
    }
}
