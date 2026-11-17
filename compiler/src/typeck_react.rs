//! The bundled `react` crate's built-in bounds: what a `jsx!` child (and `ns.print_raw`'s
//! argument) may be, and what each kind of HTML attribute takes. The react prelude declares them
//! as marker traits (`ReactIntoNode`, `ReactText`, ..) on its `__rx_*` helpers; which types meet
//! them is decided here, by shape (`Option<Vec<&str>>` is a node), as the runtime converts them.
//! A child module of `typeck`.

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList},
};

use super::Checker;
use crate::ty::Ty;

/// The react prelude's built-in bounds.
const BOUNDS: &[&str] = &[
    "ReactIntoNode",
    "ReactText",
    "ReactTextOrNum",
    "ReactBool",
    "ReactStyle",
    "ReactProps",
    "ReactKey",
];

fn is_js(t: &Ty, names: &[&str]) -> bool {
    matches!(t, Ty::Extern(n) if names.contains(&n.as_str()))
}

impl Checker<'_> {
    /// Whether `t` meets `b`, when `b` is one of the react prelude's bounds.
    pub(super) fn react_bound(&self, t: &Ty, b: &str) -> Option<bool> {
        if !BOUNDS.contains(&b) {
            return None;
        }
        Some(self.react_meets(&self.zonk(t), b))
    }

    fn react_meets(&self, t: &Ty, b: &str) -> bool {
        let num = matches!(t, Ty::Int(_) | Ty::IntLit | Ty::Float { .. });
        let text = matches!(t, Ty::Str | Ty::Char);
        match t {
            Ty::Var(_) | Ty::Unknown => return true,
            Ty::Param(n) => return self.param_has(n, b),
            _ if is_js(t, &["JsValue"]) => return true,
            // (`None` leaves an attribute out, and shows nothing as a child.)
            Ty::Option(e) => return self.react_meets(&self.zonk(e), b),
            _ => {}
        }
        // A `(name, value)` pair list or a map, for `style` and `{..spread}`.
        let pairs = |key: &Ty, value: &Ty, b: &str| {
            matches!(self.zonk(key), Ty::Str | Ty::Var(_))
                && (b == "ReactProps"
                    || matches!(
                        self.zonk(value),
                        Ty::Str
                            | Ty::Char
                            | Ty::Int(_)
                            | Ty::IntLit
                            | Ty::Float { .. }
                            | Ty::Var(_)
                            | Ty::Option(_)
                    ))
        };
        match b {
            "ReactIntoNode" => match t {
                Ty::Vec(e) => self.react_meets(&self.zonk(e), b),
                _ => text || num || is_js(t, &["ReactNode"]),
            },
            "ReactText" => text,
            "ReactTextOrNum" | "ReactKey" => text || num,
            "ReactBool" => matches!(t, Ty::Bool),
            "ReactStyle" | "ReactProps" => match t {
                Ty::Str => b == "ReactStyle",
                Ty::Vec(e) => match self.zonk(e) {
                    Ty::Tuple(kv) if kv.len() == 2 => pairs(&kv[0], &kv[1], b),
                    Ty::Var(_) => true,
                    _ => false,
                },
                Ty::Map(k, v) => pairs(k, v, b),
                Ty::Coll(crate::ty::Coll::BTreeMap, m) => match self.zonk(m) {
                    Ty::Map(k, v) => pairs(&k, &v, b),
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        }
    }

    /// Where and how to report `t` not meeting react bound `b` at the call `node`: at its
    /// argument (the expression written in `jsx!`).
    pub(super) fn react_bound_error(
        &self,
        t: &Ty,
        b: &str,
        node: &SyntaxNode,
    ) -> Option<(SyntaxNode, String)> {
        if !BOUNDS.contains(&b) {
            return None;
        }
        let ty = self.describe(t);
        let msg = match b {
            "ReactIntoNode" => format!(
                "React can't show a `{ty}`: a `jsx!` child is a `react::Node`, text, a number, or an `Option` / `Vec` of them"
            ),
            "ReactText" => format!("this attribute takes text (`&str` or `String`), not `{ty}`"),
            "ReactTextOrNum" => format!("this attribute takes a number or text, not `{ty}`"),
            "ReactBool" => format!("this attribute takes a `bool`, not `{ty}`"),
            "ReactStyle" => format!(
                "`style` takes a CSS string (`\"color: red\"`), `(name, value)` pairs or a `HashMap`, not `{ty}`"
            ),
            "ReactProps" => format!(
                "`{{..spread}}` on an HTML element takes `(name, value)` pairs, a `HashMap` or a `JsValue` object, not `{ty}`"
            ),
            _ => format!("`key` takes text or a number, not `{ty}`"),
        };
        // (Inside the parentheses `jsx!` puts around an `{expr}`.)
        let mut arg = ast::CallExpr::cast(node.clone())
            .and_then(|c| c.arg_list())
            .and_then(|l| l.args().next());
        while let Some(ast::Expr::ParenExpr(p)) = &arg {
            arg = p.expr();
        }
        let at = arg.map_or_else(|| node.clone(), |a| a.syntax().clone());
        Some((at, msg))
    }

    /// An argument of an NS function taking a React node (`ns.print_raw(x)`): anything a `jsx!`
    /// child may be.
    pub(super) fn check_node_arg(&mut self, a: &ast::Expr) {
        let t = self.infer(a, None);
        let t = self.zonk(&t);
        if !self.react_meets(&t, "ReactIntoNode") {
            let msg = format!(
                "React can't show a `{}`: this takes a `react::Node` (`jsx! {{ .. }}`), text, a number, or an `Option` / `Vec` of them",
                self.describe(&t)
            );
            self.error(a.syntax(), msg);
        }
    }
}
