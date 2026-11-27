//! `std::any`: `TypeId::of::<T>()` / `x.type_id()` (a string naming the static type, so ids
//! compare and hash), `type_name::<T>()` (std's spelling: `alloc::string::String`,
//! `crate::Point`), and `dyn Any` (`&dyn Any`, `Box<dyn Any>`) with `is::<T>()`,
//! `downcast_ref` / `downcast_mut` / `downcast`, which test the value at run time: its class for a
//! struct or enum, its JS type (and an integer type's range) otherwise. A child module of
//! `typeck`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs},
};

use super::{Checker, VarKind};
use crate::ty::Ty;

/// std's path of a type's name, as `type_name` spells it.
fn std_path(name: &str) -> Option<&'static str> {
    Some(match name {
        "String" => "alloc::string::String",
        "Vec" => "alloc::vec::Vec",
        "Box" => "alloc::boxed::Box",
        "Rc" => "alloc::rc::Rc",
        "Arc" => "alloc::sync::Arc",
        "Option" => "core::option::Option",
        "Result" => "core::result::Result",
        "HashMap" => "std::collections::hash::map::HashMap",
        "HashSet" => "std::collections::hash::set::HashSet",
        "BTreeMap" => "alloc::collections::btree::map::BTreeMap",
        "BTreeSet" => "alloc::collections::btree::set::BTreeSet",
        "VecDeque" => "alloc::collections::vec_deque::VecDeque",
        "RefCell" => "core::cell::RefCell",
        "Cell" => "core::cell::Cell",
        _ => return None,
    })
}

/// Whether `t` names a type parameter (erased at run time).
fn has_param(t: &Ty) -> bool {
    match t {
        Ty::Param(_) => true,
        Ty::Vec(e)
        | Ty::Option(e)
        | Ty::Set(e)
        | Ty::Future(e)
        | Ty::RefCell(e)
        | Ty::Coll(_, e)
        | Ty::IntoIter(e)
        | Ty::Range(e, _)
        | Ty::Iter { elem: e, .. } => has_param(e),
        Ty::Map(a, b) | Ty::Result(a, b) | Ty::Entry(a, b) => has_param(a) || has_param(b),
        Ty::Tuple(es) | Ty::Adt(_, es) => es.iter().any(has_param),
        Ty::Closure { params, ret } => params.iter().any(has_param) || has_param(ret),
        _ => false,
    }
}

/// Whether `is::<T>()` can tell a `T` apart at run time.
pub(super) fn checkable(t: &Ty) -> bool {
    matches!(
        t,
        Ty::Adt(..)
            | Ty::Int(_)
            | Ty::Float { .. }
            | Ty::Bool
            | Ty::Char
            | Ty::Str
            | Ty::Unit
            | Ty::Vec(_)
            | Ty::Map(..)
            | Ty::Set(_)
            | Ty::Tuple(_)
    )
}

impl Checker<'_> {
    /// The type argument of a call or method (`TypeId::of::<T>()`, `x.is::<T>()`).
    fn any_type_arg(&mut self, list: Option<ast::GenericArgList>) -> Option<(ast::Type, Ty)> {
        let ty = list?.generic_args().find_map(|g| match g {
            ast::GenericArg::TypeArg(ta) => ta.ty(),
            _ => None,
        })?;
        let t = self.lower_type(&ty);
        Some((ty, t))
    }

    /// `TypeId::of::<T>()`, `type_name::<T>()`, `type_name_of_val(&x)`: their text, recorded for
    /// codegen. `None` for other paths.
    pub(super) fn any_call(
        &mut self,
        c: &ast::CallExpr,
        tail: &[&str],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let which = match tail {
            [.., "TypeId", "of"] if !self.adts.defs.contains_key("TypeId") => "of",
            ["any", w @ ("type_name" | "type_name_of_val")] => w,
            _ => return None,
        };
        let generics = match c.expr() {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.generic_arg_list()),
            _ => None,
        };
        let range = self.range(c.syntax());
        if which == "type_name_of_val" {
            self.arg_count(c.syntax(), "function", 1, args.len());
            let t = args.first().map(|a| self.infer(a, None));
            let t = t.map(|t| self.zonk(&t)).unwrap_or(Ty::Unknown);
            if let Some(name) = self.static_type_text(c.syntax(), &t, None) {
                self.any_strs.insert(range, name);
            }
            return Some(Ty::Str);
        }
        self.arg_count(c.syntax(), "function", 0, args.len());
        let Some((ast_ty, t)) = self.any_type_arg(generics) else {
            self.error(
                c.syntax(),
                format!("`{}` needs its type argument (`::<T>`)", tail.join("::")),
            );
            return Some(Ty::Str);
        };
        let text = if which == "of" {
            self.static_type_text(c.syntax(), &t, None)
                .map(|_| self.zonk(&t).describe())
        } else {
            self.static_type_text(c.syntax(), &t, Some(&ast_ty))
        };
        if let Some(text) = text {
            self.any_strs.insert(range, text);
        }
        Some(Ty::Str)
    }

    /// `type_name`'s text of `t` (from `written` when there is one, which tells `&str` from
    /// `String`), or an error for a type known only at run time (a type parameter).
    fn static_type_text(
        &mut self,
        node: &ra_ap_syntax::SyntaxNode,
        t: &Ty,
        written: Option<&ast::Type>,
    ) -> Option<String> {
        let z = self.zonk(t);
        if has_param(&z) || matches!(z, Ty::Unknown | Ty::Var(_)) {
            self.error(
                node,
                "the type here is a type parameter (or isn't known), and types are erased at run time: name a concrete type",
            );
            return None;
        }
        Some(match written {
            Some(w) => self.written_type_name(w),
            None => self.ty_type_name(&z),
        })
    }

    fn written_type_name(&self, t: &ast::Type) -> String {
        match t {
            ast::Type::RefType(r) => {
                let inner = r
                    .ty()
                    .map(|t| self.written_type_name(&t))
                    .unwrap_or_default();
                if r.mut_token().is_some() {
                    format!("&mut {inner}")
                } else {
                    format!("&{inner}")
                }
            }
            ast::Type::TupleType(tt) => {
                let parts: Vec<String> = tt.fields().map(|f| self.written_type_name(&f)).collect();
                if parts.len() == 1 {
                    format!("({},)", parts[0])
                } else {
                    format!("({})", parts.join(", "))
                }
            }
            ast::Type::SliceType(s) => format!(
                "[{}]",
                s.ty()
                    .map(|t| self.written_type_name(&t))
                    .unwrap_or_default()
            ),
            ast::Type::ArrayType(a) => format!(
                "[{}; {}]",
                a.ty()
                    .map(|t| self.written_type_name(&t))
                    .unwrap_or_default(),
                a.const_arg()
                    .map(|c| c.syntax().text().to_string())
                    .unwrap_or_default()
            ),
            ast::Type::ParenType(p) => p
                .ty()
                .map(|t| self.written_type_name(&t))
                .unwrap_or_default(),
            ast::Type::PathType(p) => {
                let Some(seg) = p.path().and_then(|p| p.segment()) else {
                    return t.syntax().text().to_string();
                };
                let name = seg
                    .name_ref()
                    .map(|n| n.text().to_string())
                    .unwrap_or_default();
                let args: Vec<String> = seg
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|l| l.generic_args())
                    .filter_map(|g| match g {
                        ast::GenericArg::TypeArg(ta) => ta.ty().map(|t| self.written_type_name(&t)),
                        ast::GenericArg::ConstArg(c) => Some(c.syntax().text().to_string()),
                        _ => None,
                    })
                    .collect();
                let base = if let Some(path) = std_path(&name) {
                    path.to_string()
                } else if self.adts.defs.contains_key(&name) {
                    format!("{}::{name}", self.crate_name)
                } else if let Some(t) = self.type_aliases.get(&name) {
                    return self.ty_type_name(t);
                } else {
                    name
                };
                if args.is_empty() {
                    base
                } else {
                    format!("{base}<{}>", args.join(", "))
                }
            }
            other => other.syntax().text().to_string(),
        }
    }

    /// `type_name`'s text of a type known only by its `Ty` (`type_name_of_val`).
    fn ty_type_name(&self, t: &Ty) -> String {
        let args = |ts: &[Ty]| {
            ts.iter()
                .map(|t| self.ty_type_name(t))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match t {
            Ty::Str => "alloc::string::String".into(),
            Ty::Vec(e) => format!("alloc::vec::Vec<{}>", self.ty_type_name(e)),
            Ty::Option(e) => format!("core::option::Option<{}>", self.ty_type_name(e)),
            Ty::Result(a, b) => format!(
                "core::result::Result<{}, {}>",
                self.ty_type_name(a),
                self.ty_type_name(b)
            ),
            Ty::Map(k, v) => format!(
                "std::collections::hash::map::HashMap<{}, {}>",
                self.ty_type_name(k),
                self.ty_type_name(v)
            ),
            Ty::Set(e) => format!(
                "std::collections::hash::set::HashSet<{}>",
                self.ty_type_name(e)
            ),
            Ty::Tuple(es) if es.len() == 1 => format!("({},)", self.ty_type_name(&es[0])),
            Ty::Tuple(es) => format!("({})", args(es)),
            Ty::Adt(n, ts) if ts.is_empty() => format!("{}::{n}", self.crate_name),
            Ty::Adt(n, ts) => format!("{}::{n}<{}>", self.crate_name, args(ts)),
            other => other.describe(),
        }
    }

    /// Methods of `dyn Any` (`is`, `downcast_ref`, `downcast_mut`, `downcast`) and `type_id()`
    /// of any value. `None` for other methods.
    pub(super) fn any_method(
        &mut self,
        m: &ast::MethodCallExpr,
        rt: &Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let is_any = matches!(self.shallow(rt), Ty::Dyn(t, _) if t == "Any");
        let range = self.range(m.syntax());
        if method == "type_id" && args.is_empty() {
            if is_any {
                self.error(
                    m.syntax(),
                    "the type of a `dyn Any` isn't known at run time: use `x.is::<T>()` (or `downcast_ref::<T>()`) instead of comparing `type_id()`s",
                );
            } else if let Some(text) = self.static_type_text(m.syntax(), rt, None) {
                let _ = text;
                self.any_strs.insert(range, self.zonk(rt).describe());
            }
            return Some(Ty::Str);
        }
        // (`dyn Error` has the same three, and `is`.)
        let dyn_ty = self.shallow(rt);
        let is_error = matches!(&dyn_ty, Ty::Dyn(t, _) if t.split('+').next() == Some("Error"));
        let is_any = is_any || is_error;
        if !is_any {
            return None;
        }
        let ret = |t: Ty| match method {
            "is" => Ty::Bool,
            "downcast_ref" | "downcast_mut" => Ty::Option(Box::new(t)),
            _ => Ty::Result(Box::new(t), Box::new(dyn_ty.clone())),
        };
        if !matches!(method, "is" | "downcast_ref" | "downcast_mut" | "downcast") {
            return None;
        }
        self.arg_count(m.syntax(), "method", 0, args.len());
        let Some((_, t)) = self.any_type_arg(m.generic_arg_list()) else {
            self.error(
                m.syntax(),
                format!("`{method}` needs its type argument here (`{method}::<T>()`)"),
            );
            let fresh = self.fresh(VarKind::General);
            return Some(ret(fresh));
        };
        let z = self.zonk(&t);
        if !checkable(&z) || has_param(&z) {
            let msg = format!(
                "`{method}::<{}>()` can't be told apart at run time (structs, enums, numbers, strings, `bool`, `char`, `()`, `Vec`s, maps, sets and tuples can)",
                self.describe(&z)
            );
            self.error(m.syntax(), msg);
        } else {
            self.any_checks.insert(range, z);
        }
        Some(ret(t))
    }
}
