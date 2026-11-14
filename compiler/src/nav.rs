//! Navigation for editors: go to definition, find references and rename. A name is resolved
//! the way the checker sees it: locals by scope, items by name (also in inline modules and in
//! `mod x;` files through the resolver), methods and fields by the receiver's type, variants and
//! associated functions by their type's path.

use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind, SyntaxNode, SyntaxToken, TextRange, TextSize,
    ast::{self, HasLoopBody, HasModuleItem, HasName},
};
use serde::Serialize;

use crate::lineindex::LineIndex;
use crate::macros::EDITION;
use crate::service::{Analysis, analyze, offset_of, type_at};
use crate::ty::Ty;
use crate::typeck::Resolver;

/// A range in a file (1-based lines, UTF-16 columns); `file` is the one asked about, or a
/// module's path as the resolver knows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// Where a name is defined: a range of the analyzed text, or a name in another file.
#[derive(Clone, Debug, PartialEq)]
enum Def {
    Here(TextRange),
    File(String, TextRange),
}

fn location(text: &str, file: &str, a: usize, b: usize) -> Location {
    let idx = LineIndex::new(text);
    let (l0, c0) = idx.line_col(a);
    let (l1, c1) = idx.line_col(b);
    Location {
        file: file.to_string(),
        line: l0 + 1,
        column: c0 + 1,
        end_line: l1 + 1,
        end_column: c1 + 1,
    }
}

/// A range of the analyzed text in `src`, narrowed to the last path segment where a flattened
/// module path (`geo::Point` → `geo__Point`) maps back to several.
fn src_range(an: &Analysis, src: &str, r: TextRange) -> (usize, usize) {
    let (a, b) = (usize::from(r.start()), usize::from(r.end()));
    let (mut a, b) = match &an.map {
        Some(m) => (m.orig(a, false), m.orig(b, true)),
        None => (a, b),
    };
    if let Some(i) = src.get(a..b).and_then(|t| t.rfind("::")) {
        a += i + 2;
    }
    (a, b)
}

fn name_range(n: &impl HasName) -> Option<TextRange> {
    n.name().map(|n| n.syntax().text_range())
}

/// A named item of `file` (outside function bodies' own items too): fn, struct, enum, trait,
/// const, static, type alias.
fn item_named(root: &SyntaxNode, name: &str) -> Option<TextRange> {
    root.descendants()
        .filter(|n| {
            !n.ancestors()
                .skip(1)
                .any(|a| ast::Impl::can_cast(a.kind()) || ast::Trait::can_cast(a.kind()))
        })
        .find_map(|n| {
            let n = match ast::Item::cast(n)? {
                ast::Item::Fn(x) => x.name(),
                ast::Item::Struct(x) => x.name(),
                ast::Item::Enum(x) => x.name(),
                ast::Item::Trait(x) => x.name(),
                ast::Item::Const(x) => x.name(),
                ast::Item::Static(x) => x.name(),
                ast::Item::TypeAlias(x) => x.name(),
                ast::Item::Union(x) => x.name(),
                _ => None,
            }?;
            (n.text() == name).then(|| n.syntax().text_range())
        })
}

/// The type an `impl` is for, by its last path segment's name.
fn impl_self(i: &ast::Impl) -> Option<String> {
    match i.self_ty()? {
        ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
        _ => None,
    }
}

/// Method or associated item `name` of type `ty`: in its impls, then in traits' declarations.
fn assoc_named(root: &SyntaxNode, ty: Option<&str>, name: &str) -> Option<TextRange> {
    let in_list = |l: Option<ast::AssocItemList>| {
        l?.assoc_items().find_map(|a| match a {
            ast::AssocItem::Fn(f) if f.name()?.text() == name => name_range(&f),
            ast::AssocItem::Const(c) if c.name()?.text() == name => name_range(&c),
            ast::AssocItem::TypeAlias(t) if t.name()?.text() == name => name_range(&t),
            _ => None,
        })
    };
    let impls: Vec<ast::Impl> = root.descendants().filter_map(ast::Impl::cast).collect();
    if let Some(ty) = ty {
        // Inherent impls first, then trait impls.
        let mut ours: Vec<&ast::Impl> = impls
            .iter()
            .filter(|i| impl_self(i).as_deref() == Some(ty))
            .collect();
        ours.sort_by_key(|i| i.trait_().is_some());
        if let Some(r) = ours.iter().find_map(|i| in_list(i.assoc_item_list())) {
            return Some(r);
        }
        // A default method of a trait it implements.
        for i in &ours {
            let Some(ast::Type::PathType(t)) = i.trait_() else {
                continue;
            };
            let Some(tn) = t
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
            else {
                continue;
            };
            if let Some(r) = root
                .descendants()
                .filter_map(ast::Trait::cast)
                .filter(|t| t.name().is_some_and(|n| n.text() == tn.text()))
                .find_map(|t| in_list(t.assoc_item_list()))
            {
                return Some(r);
            }
        }
        // The type is a trait (`dyn Shape`, `Shape::area`).
        if let Some(r) = root
            .descendants()
            .filter_map(ast::Trait::cast)
            .filter(|t| t.name().is_some_and(|n| n.text() == ty))
            .find_map(|t| in_list(t.assoc_item_list()))
        {
            return Some(r);
        }
        return None;
    }
    // Unknown receiver (a generic `T`): any trait declaring it, else any impl.
    root.descendants()
        .filter_map(ast::Trait::cast)
        .find_map(|t| in_list(t.assoc_item_list()))
        .or_else(|| impls.iter().find_map(|i| in_list(i.assoc_item_list())))
}

fn variant_named(root: &SyntaxNode, en: &str, name: &str) -> Option<TextRange> {
    root.descendants()
        .filter_map(ast::Enum::cast)
        .filter(|e| e.name().is_some_and(|n| n.text() == en))
        .find_map(|e| {
            e.variant_list()?
                .variants()
                .find(|v| v.name().is_some_and(|n| n.text() == name))
        })
        .and_then(|v| name_range(&v))
}

fn field_named(root: &SyntaxNode, owner: &str, name: &str) -> Option<TextRange> {
    let fields = |l: Option<ast::RecordFieldList>| {
        l?.fields()
            .find(|f| f.name().is_some_and(|n| n.text() == name))
            .and_then(|f| name_range(&f))
    };
    root.descendants()
        .filter_map(ast::Struct::cast)
        .filter(|s| s.name().is_some_and(|n| n.text() == owner))
        .find_map(|s| match s.field_list()? {
            ast::FieldList::RecordFieldList(l) => fields(Some(l)),
            _ => None,
        })
        .or_else(|| {
            // A struct-like variant (`Shape::Circle { r }`).
            root.descendants()
                .filter_map(ast::Variant::cast)
                .filter(|v| v.name().is_some_and(|n| n.text() == owner))
                .find_map(|v| match v.field_list()? {
                    ast::FieldList::RecordFieldList(l) => fields(Some(l)),
                    _ => None,
                })
        })
}

/// The binding a local name at `at` refers to: the latest one before it whose scope covers it.
fn local_named(at: &SyntaxNode, name: &str) -> Option<TextRange> {
    local_at(at, at.text_range(), name)
}

/// `local_named` for a use at `use_range` inside `at` (a token of a macro's arguments).
fn local_at(at: &SyntaxNode, use_range: TextRange, name: &str) -> Option<TextRange> {
    let body = at
        .ancestors()
        .filter(|a| {
            ast::Fn::can_cast(a.kind())
                || ast::Const::can_cast(a.kind())
                || ast::Static::can_cast(a.kind())
        })
        .last()?;
    body.descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|ip| ip.name().is_some_and(|n| n.text() == name))
        .filter(|ip| {
            let r = ip.syntax().text_range();
            if r.start() > use_range.start() {
                return false;
            }
            let scope = ip.syntax().ancestors().find_map(|a| match a.kind() {
                SyntaxKind::LET_STMT => {
                    // From the end of the `let` to the end of its block.
                    let block = a.parent()?.text_range();
                    Some(TextRange::new(a.text_range().end(), block.end()))
                }
                SyntaxKind::PARAM | SyntaxKind::SELF_PARAM => a
                    .ancestors()
                    .find(|x| ast::Fn::can_cast(x.kind()) || ast::ClosureExpr::can_cast(x.kind()))
                    .map(|x| x.text_range()),
                SyntaxKind::MATCH_ARM => Some(a.text_range()),
                SyntaxKind::FOR_EXPR => ast::ForExpr::cast(a.clone())
                    .and_then(|f| f.loop_body())
                    .map(|b| b.syntax().text_range()),
                SyntaxKind::LET_EXPR => a
                    .ancestors()
                    .find(|x| matches!(x.kind(), SyntaxKind::IF_EXPR | SyntaxKind::WHILE_EXPR))
                    .map(|x| x.text_range()),
                _ => None,
            });
            scope.is_some_and(|s| s.contains_range(use_range))
        })
        .max_by_key(|ip| ip.syntax().text_range().start())
        .and_then(|ip| name_range(&ip))
}

/// The `mod x;` files of `file`: module name → (resolver path, text).
fn file_module(
    file: &SourceFile,
    filename: &str,
    name: &str,
    resolver: Resolver,
) -> Option<(String, String)> {
    file.items().find_map(|i| match i {
        ast::Item::Module(m) if m.item_list().is_none() && m.name()?.text() == name => Some(()),
        _ => None,
    })?;
    let flat = crate::typeck::module_path(filename, name);
    if let Some(t) = resolver(&flat) {
        return Some((flat, t));
    }
    let nested = crate::typeck::module_path(filename, &format!("{name}/mod"));
    resolver(&nested).map(|t| (nested, t))
}

/// `segs` (after a `mod x;` name) in that file: an item, or a type's associated item.
fn in_file(path: String, text: &str, segs: &[String]) -> Option<Def> {
    let file = SourceFile::parse(text, EDITION).tree();
    let root = file.syntax();
    let r = match segs {
        [name] => item_named(root, name),
        [ty, name] => variant_named(root, ty, name).or_else(|| assoc_named(root, Some(ty), name)),
        _ => None,
    }?;
    Some(Def::File(path, r))
}

/// What `use` brings `name` in as, if from a `mod x;` file: the path's segments.
fn imported(file: &SourceFile, name: &str) -> Option<Vec<String>> {
    file.syntax()
        .descendants()
        .filter_map(ast::Use::cast)
        .find_map(|u| {
            crate::typeck::use_leaves(&u)
                .into_iter()
                .find_map(|(segs, rename, star)| {
                    (!star && rename.as_deref().or(segs.last().map(String::as_str)) == Some(name))
                        .then_some(segs)
                })
        })
}

struct Ctx<'a> {
    an: &'a Analysis,
    filename: &'a str,
    resolver: Resolver<'a>,
}

impl Ctx<'_> {
    fn root(&self) -> &SyntaxNode {
        self.an.file.syntax()
    }

    /// The type name of a value's type, for methods and fields.
    fn owner(&self, e: &SyntaxNode) -> Option<String> {
        match type_at(&self.an.types, e)? {
            Ty::Adt(n, _) => Some(n),
            Ty::Dyn(n) => Some(n),
            _ => None,
        }
    }

    /// The type a path names (`Point`, `Self`, an alias already expanded).
    fn path_type(&self, p: &ast::Path) -> Option<String> {
        let name = p.segment()?.name_ref()?.text().to_string();
        if name == "Self" {
            let i = p.syntax().ancestors().find_map(ast::Impl::cast)?;
            return impl_self(&i);
        }
        Some(name)
    }

    /// Resolve the path `p` (up to its last segment).
    fn path(&self, p: &ast::Path) -> Option<Def> {
        let seg = p.segment()?;
        let name = {
            let n = seg.name_ref()?;
            n.text().to_string()
        };
        let root = self.root();
        let Some(q) = p.qualifier() else {
            // A lone name: a local, an item, or one a `use` brings from a module file.
            let in_type = p
                .syntax()
                .ancestors()
                .any(|a| ast::PathType::can_cast(a.kind()));
            if !in_type && let Some(r) = local_named(p.syntax(), &name) {
                return Some(Def::Here(r));
            }
            if name == "Self" {
                let i = p.syntax().ancestors().find_map(ast::Impl::cast)?;
                let t = impl_self(&i)?;
                return item_named(root, &t).map(Def::Here);
            }
            if let Some(r) = item_named(root, &name) {
                return Some(Def::Here(r));
            }
            let segs = imported(&self.an.file, &name)?;
            let (path, text) =
                file_module(&self.an.file, self.filename, segs.first()?, self.resolver)?;
            return in_file(path, &text, &segs[1..]);
        };
        // `module::item` / `module::Type::item` through a `mod x;` file.
        let mut segs: Vec<String> = std::iter::successors(Some(p.clone()), |p| p.qualifier())
            .filter_map(|p| p.segment()?.name_ref().map(|n| n.text().to_string()))
            .collect();
        segs.reverse();
        if segs.len() >= 2
            && let Some((path, text)) =
                file_module(&self.an.file, self.filename, &segs[0], self.resolver)
        {
            return in_file(path, &text, &segs[1..]);
        }
        // `Type::Variant`, `Type::method`, `Trait::method`.
        let ty = self.path_type(&q)?;
        variant_named(root, &ty, &name)
            .or_else(|| assoc_named(root, Some(&ty), &name))
            .map(Def::Here)
    }

    /// The definition the identifier `token` refers to.
    fn def(&self, token: &SyntaxToken) -> Option<Def> {
        let node = token.parent()?;
        if node.kind() == SyntaxKind::NAME {
            return Some(Def::Here(node.text_range()));
        }
        if node.kind() == SyntaxKind::TOKEN_TREE {
            // A name among a macro's arguments (`println!("{}", total)`): a local or an item,
            // unless it's a field or method (after `.`) or a path's later segment.
            let prev = std::iter::successors(token.prev_token(), |t| t.prev_token())
                .find(|t| !t.kind().is_trivia());
            if prev.as_ref().is_some_and(|t| t.text() == ".") {
                // `p.len()` / `p.x` (the receiver's type isn't known here): a method or field of
                // that name.
                let call = std::iter::successors(token.next_token(), |t| t.next_token())
                    .find(|t| !t.kind().is_trivia())
                    .is_some_and(|t| t.text() == "(");
                let root = self.root();
                return if call {
                    assoc_named(root, None, token.text())
                } else {
                    root.descendants()
                        .filter_map(ast::RecordField::cast)
                        .find(|f| f.name().is_some_and(|n| n.text() == token.text()))
                        .and_then(|f| name_range(&f))
                }
                .map(Def::Here);
            }
            if prev.is_some_and(|t| matches!(t.text(), "::" | ":")) {
                return None;
            }
            if let Some(r) = local_at(&node, token.text_range(), token.text())
                .or_else(|| item_named(self.root(), token.text()))
            {
                return Some(Def::Here(r));
            }
            // A name a `use` brings from a module file.
            let segs = imported(&self.an.file, token.text())?;
            let (path, text) =
                file_module(&self.an.file, self.filename, segs.first()?, self.resolver)?;
            return in_file(path, &text, &segs[1..]);
        }
        if node.kind() != SyntaxKind::NAME_REF {
            return None;
        }
        let parent = node.parent()?;
        let name = token.text();
        let root = self.root();
        if let Some(m) = ast::MethodCallExpr::cast(parent.clone()) {
            let owner = m.receiver().and_then(|r| self.owner(r.syntax()));
            return assoc_named(root, owner.as_deref(), name).map(Def::Here);
        }
        if let Some(fe) = ast::FieldExpr::cast(parent.clone()) {
            let owner = self.owner(fe.expr()?.syntax())?;
            return field_named(root, &owner, name).map(Def::Here);
        }
        if let Some(f) = ast::RecordExprField::cast(parent.clone()) {
            // `Point { x: 1 }` (the field), or shorthand `Point { x }` (the local, as rustc's
            // go-to-definition does for the value).
            if f.expr().is_none() {
                return local_named(f.syntax(), name).map(Def::Here);
            }
            let rec = f.syntax().ancestors().find_map(ast::RecordExpr::cast)?;
            let owner = rec.path()?.segment()?.name_ref()?.text().to_string();
            return field_named(root, &owner, name).map(Def::Here);
        }
        if let Some(f) = ast::RecordPatField::cast(parent.clone()) {
            let rec = f.syntax().ancestors().find_map(ast::RecordPat::cast)?;
            let owner = rec.path()?.segment()?.name_ref()?.text().to_string();
            return field_named(root, &owner, name).map(Def::Here);
        }
        let seg = ast::PathSegment::cast(parent)?;
        let path = seg.parent_path();
        self.path(&path)
    }
}

/// The identifier at (line, column) and what's needed to resolve it.
fn token_at(an: &Analysis, src: &str, line: u32, column: u32) -> Option<SyntaxToken> {
    let offset = offset_of(src, line, column);
    let offset = match &an.map {
        Some(m) => m.cur_near(offset)?,
        None => offset,
    };
    let at = an
        .file
        .syntax()
        .token_at_offset(TextSize::from(offset as u32));
    at.clone()
        .find(|t| t.kind() == SyntaxKind::IDENT)
        .or_else(|| at.right_biased().filter(|t| t.kind() == SyntaxKind::IDENT))
}

/// A range of the analyzed text as a location in `src`.
fn here(an: &Analysis, src: &str, filename: &str, r: TextRange) -> Location {
    let (a, b) = src_range(an, src, r);
    location(src, filename, a, b)
}

/// Where the name at (line, column) is defined.
pub fn definition(
    src: &str,
    filename: &str,
    line: u32,
    column: u32,
    resolver: Resolver,
) -> Option<Location> {
    let an = analyze(src, filename, resolver);
    let token = token_at(&an, src, line, column)?;
    let cx = Ctx {
        an: &an,
        filename,
        resolver,
    };
    let Some(d) = cx.def(&token) else {
        return raw_item(src, filename, line, column);
    };
    match d {
        Def::File(path, r) => {
            let text = resolver(&path)?;
            Some(location(&text, &path, r.start().into(), r.end().into()))
        }
        Def::Here(r) => Some(here(&an, src, filename, r)),
    }
}

/// An item of `src` itself named by the identifier at (line, column), for names the analysis
/// rewrote away (`type` aliases).
fn raw_item(src: &str, filename: &str, line: u32, column: u32) -> Option<Location> {
    let file = SourceFile::parse(src, EDITION).tree();
    let offset = offset_of(src, line, column);
    let token = file
        .syntax()
        .token_at_offset(TextSize::from(offset as u32))
        .find(|t| t.kind() == SyntaxKind::IDENT)?;
    let r = item_named(file.syntax(), token.text())?;
    Some(location(src, filename, r.start().into(), r.end().into()))
}

/// A definition as (file, start, end) in that file's own text, so files can be compared.
fn norm(an: &Analysis, src: &str, filename: &str, d: &Def) -> (String, usize, usize) {
    match d {
        Def::Here(r) => {
            let (a, b) = src_range(an, src, *r);
            (filename.to_string(), a, b)
        }
        Def::File(path, r) => (path.clone(), r.start().into(), r.end().into()),
    }
}

/// The identifiers named `name` in `src` (the file `filename`) whose definition is `target`.
fn uses_in(
    src: &str,
    filename: &str,
    name: &str,
    target: &(String, usize, usize),
    resolver: Resolver,
) -> Vec<Location> {
    let an = analyze(src, filename, resolver);
    let cx = Ctx {
        an: &an,
        filename,
        resolver,
    };
    let mut out: Vec<Location> = an
        .file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() == SyntaxKind::IDENT && t.text() == name)
        .filter(|t| {
            cx.def(t)
                .is_some_and(|d| &norm(&an, src, filename, &d) == target)
        })
        .map(|t| here(&an, src, filename, t.text_range()))
        .collect();
    out.dedup();
    out
}

/// Every use of the name at (line, column), its definition included: in this file, and in
/// `others` (paths the resolver reads, like the other scripts of a project).
pub fn references(
    src: &str,
    filename: &str,
    line: u32,
    column: u32,
    resolver: Resolver,
    others: &[String],
) -> Vec<Location> {
    let an = analyze(src, filename, resolver);
    let Some(token) = token_at(&an, src, line, column) else {
        return Vec::new();
    };
    let cx = Ctx {
        an: &an,
        filename,
        resolver,
    };
    let Some(def) = cx.def(&token) else {
        return Vec::new();
    };
    let target = norm(&an, src, filename, &def);
    let name = token.text().to_string();
    let mut out = uses_in(src, filename, &name, &target, resolver);
    for path in others.iter().filter(|p| p.as_str() != filename) {
        if let Some(text) = resolver(path) {
            out.extend(uses_in(&text, path, &name, &target, resolver));
        }
    }
    out
}

/// The ranges to replace to rename the name at (line, column), across this file and `others`;
/// empty when it's not renamable (not an identifier, or `new_name` isn't one).
pub fn rename(
    src: &str,
    filename: &str,
    line: u32,
    column: u32,
    new_name: &str,
    resolver: Resolver,
    others: &[String],
) -> Vec<Location> {
    let valid = new_name
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && new_name.chars().all(|c| c.is_alphanumeric() || c == '_');
    if !valid {
        return Vec::new();
    }
    references(src, filename, line, column, resolver, others)
}
