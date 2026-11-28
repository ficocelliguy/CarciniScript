//! Operator traits' methods called by name, as a pre-pass over the syntax tree (after macro
//! expansion): `a.add(b)` is written as `(a + (b))`, `n.not()` as `(!n)`, `x.add_assign(y)` as
//! `x += (y)`, `s.deref()` as `(&*s)` and `v.index(i)` as `(&v[i])` (`(v[i])` before a `.`), so they take the operators' checking and code. The
//! receiver and arguments keep their text (and positions); only the `.add(` and the closing `)`
//! are rewritten. A name the file declares as a function of its own (an inherent `fn add`, a
//! trait's) outside an impl of the operator's trait is left alone, and so is anything in `unsafe`
//! code (a raw pointer's `p.add(1)`).

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasGenericArgs, HasName},
};

use crate::mbe::OffsetMap;

/// How each method is written: (method, trait, arguments, prefix, middle, suffix), as in
/// `{prefix}recv{middle}arg{suffix}`.
const METHODS: &[(&str, &str, usize, &str, &str, &str)] = &[
    ("add", "Add", 1, "(", " + (", "))"),
    ("sub", "Sub", 1, "(", " - (", "))"),
    ("mul", "Mul", 1, "(", " * (", "))"),
    ("div", "Div", 1, "(", " / (", "))"),
    ("rem", "Rem", 1, "(", " % (", "))"),
    ("bitand", "BitAnd", 1, "(", " & (", "))"),
    ("bitor", "BitOr", 1, "(", " | (", "))"),
    ("bitxor", "BitXor", 1, "(", " ^ (", "))"),
    ("shl", "Shl", 1, "(", " << (", "))"),
    ("shr", "Shr", 1, "(", " >> (", "))"),
    ("add_assign", "AddAssign", 1, "", " += (", ")"),
    ("sub_assign", "SubAssign", 1, "", " -= (", ")"),
    ("mul_assign", "MulAssign", 1, "", " *= (", ")"),
    ("div_assign", "DivAssign", 1, "", " /= (", ")"),
    ("rem_assign", "RemAssign", 1, "", " %= (", ")"),
    ("bitand_assign", "BitAndAssign", 1, "", " &= (", ")"),
    ("bitor_assign", "BitOrAssign", 1, "", " |= (", ")"),
    ("bitxor_assign", "BitXorAssign", 1, "", " ^= (", ")"),
    ("shl_assign", "ShlAssign", 1, "", " <<= (", ")"),
    ("shr_assign", "ShrAssign", 1, "", " >>= (", ")"),
    ("neg", "Neg", 0, "(-", "", ")"),
    ("not", "Not", 0, "(!", "", ")"),
    ("deref", "Deref", 0, "(&*", "", ")"),
    ("deref_mut", "DerefMut", 0, "(&mut *", "", ")"),
    ("index", "Index", 1, "(&", "[", "])"),
    ("index_mut", "IndexMut", 1, "(&mut ", "[", "])"),
];

/// Whether `text` may call one.
pub fn has_op_methods(text: &str) -> bool {
    METHODS
        .iter()
        .any(|(m, ..)| text.contains(&format!(".{m}(")))
        || text.contains("Index::index")
}

/// Rewrite the calls; whether anything changed.
pub fn expand(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    // The names the file declares as functions outside an impl of the operator's trait.
    let mut own: Vec<String> = Vec::new();
    for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
        let Some(n) = f.name() else { continue };
        let n = n.text().to_string();
        let Some(&(_, tr, ..)) = METHODS.iter().find(|(m, ..)| *m == n) else {
            continue;
        };
        let in_op_impl = f
            .syntax()
            .ancestors()
            .find_map(ast::Impl::cast)
            .and_then(|i| crate::adt::impl_parts(&i).0)
            .is_some_and(|t| t == tr);
        if !in_op_impl && !own.contains(&n) {
            own.push(n);
        }
    }
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    calls(file.syntax(), 0, false, &own, &mut edits);
    let changed = !edits.is_empty();
    // (Last first, so earlier offsets stay put. Of the insertions at one place, which come outer
    // call first, the inner goes in first, so the outer ends up leftmost: `a.add(b).neg()` is
    // `(-(a + (b)))`.)
    let mut edits: Vec<(usize, (usize, usize, String))> = edits.into_iter().enumerate().collect();
    edits.sort_by_key(|(i, e)| std::cmp::Reverse((e.0, e.1, *i)));
    for (_, (a, b, new)) in edits {
        text.replace_range(a..b, &new);
        map.replace(a, b, new.len());
    }
    changed
}

/// The calls under `node`, whose text starts at `base` in the file; `in_unsafe`: `node` is a
/// macro's arguments, in `unsafe` code. A builtin macro's arguments (`println!("{}", a.add(b))`)
/// are parsed again, as an array expression (`[a, b]` and `[x; n]` both parse).
fn calls(
    node: &SyntaxNode,
    base: isize,
    in_unsafe: bool,
    own: &[String],
    edits: &mut Vec<(usize, usize, String)>,
) {
    for call in node.descendants().filter_map(ast::MethodCallExpr::cast) {
        let Some(name) = call.name_ref() else { continue };
        let n = name.text().to_string();
        let Some(&(_, _, arity, prefix, middle, suffix)) = METHODS.iter().find(|(m, ..)| *m == n)
        else {
            continue;
        };
        if own.contains(&n) || call.generic_arg_list().is_some() {
            continue;
        }
        let (Some(recv), Some(args)) = (call.receiver(), call.arg_list()) else {
            continue;
        };
        if args.args().count() != arity {
            continue;
        }
        if in_unsafe || is_unsafe(call.syntax()) {
            continue;
        }
        let (Some(dot), Some(open), Some(close)) = (
            call.dot_token(),
            args.l_paren_token(),
            args.r_paren_token(),
        ) else {
            continue;
        };
        let at = |t: &ra_ap_syntax::SyntaxToken| {
            (
                (base + usize::from(t.text_range().start()) as isize) as usize,
                (base + usize::from(t.text_range().end()) as isize) as usize,
            )
        };
        let recv_start = (base + usize::from(recv.syntax().text_range().start()) as isize) as usize;
        let (dot_start, _) = at(&dot);
        let (_, open_end) = at(&open);
        let (close_start, close_end) = at(&close);
        // (`v.index_mut(i).name.push(..)` / `v.index(1..3).len()`: `(v[i])`, as auto-deref has it.)
        let place = matches!(n.as_str(), "index" | "index_mut")
            && call.syntax().parent().is_some_and(|p| {
                ast::FieldExpr::cast(p.clone()).is_some()
                    || ast::MethodCallExpr::cast(p)
                        .and_then(|m| m.receiver())
                        .is_some_and(|r| r.syntax() == call.syntax())
            });
        let star = if matches!(n.as_str(), "index" | "index_mut") { deref_star(call.syntax()) } else { None };
        if let Some(s) = star {
            let a = (base + usize::from(s.start()) as isize) as usize;
            edits.push((a, a + 1, String::new()));
        }
        let prefix = if place || star.is_some() { "(" } else { prefix };
        if !prefix.is_empty() {
            edits.push((recv_start, recv_start, prefix.to_string()));
        }
        if arity == 0 {
            edits.push((dot_start, close_end, suffix.to_string()));
        } else {
            edits.push((dot_start, open_end, middle.to_string()));
            edits.push((close_start, close_end, suffix.to_string()));
        }
    }
    // `Index::index(&v, i)` / `IndexMut::index_mut(&mut v, i)` (also through `ops::` /
    // `std::ops::`) as `(&(v)[i])` / `(&mut (v)[i])`, when the first argument is written
    // `&v` / `&mut v`. (`Index::index(&**self, i)` keeps its own handling: a slice's index.)
    for call in node.descendants().filter_map(ast::CallExpr::cast) {
        let Some(ast::Expr::PathExpr(pe)) = call.expr() else { continue };
        let Some(path) = pe.path() else { continue };
        let segs: Vec<String> = path
            .syntax()
            .text()
            .to_string()
            .split("::")
            .map(|s| s.trim().to_string())
            .collect();
        let (tr, m) = match segs.as_slice() {
            [.., a, b] => (a.as_str(), b.as_str()),
            _ => continue,
        };
        let prefix = match (tr, m) {
            ("Index", "index") => "(&(",
            ("IndexMut", "index_mut") => "(&mut (",
            _ => continue,
        };
        if !matches!(&segs[..segs.len() - 2], [] | [_] | [_, _])
            || segs.len() > 2 && segs[segs.len() - 3] != "ops"
            || own.contains(&m.to_string())
            || in_unsafe
            || is_unsafe(call.syntax())
        {
            continue;
        }
        let Some(list) = call.arg_list() else { continue };
        let args: Vec<ast::Expr> = list.args().collect();
        let [ast::Expr::RefExpr(r), _] = args.as_slice() else { continue };
        if (r.mut_token().is_some()) != (m == "index_mut")
            || r.expr().is_none_or(|e| matches!(e, ast::Expr::PrefixExpr(_)))
        {
            continue;
        }
        let comma = list
            .syntax()
            .children_with_tokens()
            .find(|t| t.kind() == ra_ap_syntax::SyntaxKind::COMMA);
        let (Some(inner), Some(comma), Some(close)) =
            (r.expr(), comma, list.r_paren_token())
        else {
            continue;
        };
        let off = |x: ra_ap_syntax::TextSize| (base + usize::from(x) as isize) as usize;
        let start = off(call.syntax().text_range().start());
        let prefix = match deref_star(call.syntax()) {
            Some(s) => {
                let a = off(s.start());
                edits.push((a, a + 1, String::new()));
                "(("
            }
            None => prefix,
        };
        edits.push((start, off(inner.syntax().text_range().start()), prefix.to_string()));
        edits.push((
            off(inner.syntax().text_range().end()),
            off(comma.text_range().end()),
            ")[".to_string(),
        ));
        edits.push((off(close.text_range().start()), off(close.text_range().end()), "])".to_string()));
    }
    for mc in node.descendants().filter_map(ast::MacroCall::cast) {
        let name = mc
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .unwrap_or_default();
        if !ARG_MACROS.contains(&name.as_str()) {
            continue;
        }
        let Some(tt) = mc.token_tree() else { continue };
        let (Some(open), Some(close)) = (tt.left_delimiter_token(), tt.right_delimiter_token())
        else {
            continue;
        };
        let text = tt.syntax().text().to_string();
        let (o, c) = (open.text().len(), close.text().len());
        if text.len() < o + c {
            continue;
        }
        let wrapped = format!("{PREFIX}{}]; }}", &text[o..text.len() - c]);
        let parsed = crate::macros::parse_tree(&wrapped);
        // (The arguments start right after the delimiter.)
        let inner = base + usize::from(open.text_range().end()) as isize;
        calls(
            parsed.syntax(),
            inner - PREFIX.len() as isize,
            in_unsafe || is_unsafe(mc.syntax()),
            own,
            edits,
        );
    }
}

/// The `*` of a `*call` (`*v.index_mut(i) = x`): its range, so the call can be written as the place
/// itself (`(v[i]) = x`).
fn deref_star(call: &SyntaxNode) -> Option<ra_ap_syntax::TextRange> {
    let p = ast::PrefixExpr::cast(call.parent()?)?;
    (p.op_kind() == Some(ast::UnaryOp::Deref)).then_some(p.op_token()?.text_range())
}

/// What a macro's arguments are wrapped in to parse them.
const PREFIX: &str = "fn __f() { [";

/// The builtin macros whose arguments are expressions (and a format string).
const ARG_MACROS: &[&str] = &[
    "println", "print", "eprintln", "eprint", "format", "write", "writeln", "panic", "assert",
    "assert_eq", "assert_ne", "debug_assert", "debug_assert_eq", "debug_assert_ne", "vec", "dbg",
    "format_args", "todo", "unimplemented", "unreachable", "info", "warn", "error", "debug",
    "trace",
];

/// Whether `node` is in an `unsafe` block or function.
fn is_unsafe(node: &SyntaxNode) -> bool {
    node.ancestors().any(|a| {
        ast::BlockExpr::cast(a.clone()).is_some_and(|b| b.unsafe_token().is_some())
            || ast::Fn::cast(a).is_some_and(|f| f.unsafe_token().is_some())
    })
}
