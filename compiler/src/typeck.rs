//! Type inference and checking.
//!
//! Runs over the whole file before codegen, so a local's type can be decided by a later use:
//! `let mut v = Vec::new(); v.push(1u8);` makes `v` a `Vec<u8>`, and `let mut n = 0; n += x_u64;`
//! makes `n` a `u64`. Inference is per function body, using type variables and unification;
//! unsuffixed literals get integer/float variables that default to `i32`/`f64`, like rustc.
//!
//! The result is a table of fully resolved types keyed by absolute source range, covering every
//! expression and every binding name. Codegen reads types from it instead of guessing. This pass
//! reports name and type errors; codegen reports unsupported constructs.

use std::collections::HashMap;
use std::rc::Rc;

#[path = "typeck_adt.rs"]
mod adt_check;
#[path = "typeck_arrays.rs"]
mod arrays;
#[path = "typeck_coll.rs"]
mod coll;
#[path = "typeck_conv.rs"]
mod conv;
#[path = "typeck_crates.rs"]
pub(crate) mod crates;
#[path = "typeck_ext.rs"]
mod ext;
#[path = "typeck_gaps.rs"]
mod gaps;
#[path = "typeck_gen.rs"]
mod generic;
#[path = "typeck_iface.rs"]
mod iface;
#[path = "typeck_js.rs"]
mod js;
#[path = "typeck_jsval.rs"]
mod jsval;
#[path = "typeck_mod.rs"]
mod modules;
#[path = "typeck_ptr.rs"]
mod ptr;
#[path = "typeck_quote.rs"]
mod quote;
#[path = "typeck_res.rs"]
mod res;
#[path = "typeck_std.rs"]
mod std_lib;
#[path = "typeck_trait.rs"]
mod traits;
pub use arrays::array_try_from_len;
pub use crates::CrateCall;
pub use ext::{char_predicate, is_option_closure_method};
pub use gaps::delegated_fmt_is_debug;
pub use jsval::swb_imports;
pub use modules::{Imported, ModuleRef, Resolver, module_path, qualified_key, use_leaves};
pub use ptr::{Ptrs, cellable as ptr_cellable, is_ptr_type, transmute_name};
pub use quote::QuoteTok;
pub use res::{is_std_error, result_ctor_call, result_variant};

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{
        self, ArithOp, BinaryOp, CmpOp, ElseBranch, HasArgList, HasGenericArgs, HasGenericParams,
        HasLoopBody, HasName, LiteralKind, RangeItem, UnaryOp,
    },
};

use crate::bindings::{self, Bindings, bare};
use crate::fmt::{self, ArgRef, Piece};
use crate::macros::{self, MacroArg};
use crate::ty::{IntKind, Ty, is_float_method, std_const, std_tail};

pub struct RawDiag {
    pub start: usize,
    pub end: usize,
    pub message: String,
    pub severity: &'static str,
}

/// Resolved types keyed by absolute `(start, end)` byte range: every expression, and the name of
/// every binding (`let x`, parameters, `for` variables).
#[derive(Default)]
pub struct Types {
    map: HashMap<(usize, usize), Ty>,
    /// Structs, enums and their methods.
    pub adts: crate::adt::Adts,
    /// Pattern bindings that are `&mut` references into the matched place (by name range).
    pub mut_bindings: std::collections::HashSet<(usize, usize)>,
    /// Uses that move a binding's value (from the move checker), for `Drop`.
    pub move_sites: std::collections::HashSet<(usize, usize)>,
    /// `write!` / `writeln!` destinations (by their start) that are `io::Write`rs.
    pub io_writes: std::collections::HashSet<usize>,
    /// Bindings of numbers, strings, bools or Options that live in a one-element array (a cell),
    /// because a `&mut` to them is taken or they are `&mut` parameters (by name range).
    pub boxed: std::collections::HashSet<(usize, usize)>,
    /// Path expressions that pass a cell itself rather than its value (`&mut n`, or a `&mut`
    /// parameter handed on), by range.
    pub cell_args: std::collections::HashSet<(usize, usize)>,
    /// `&mut obj.field` / `&mut v[i]` arguments for `&mut i32`-style parameters: passed as a cell
    /// that reads and writes the slot (by the `&mut` expression's range).
    pub slot_args: std::collections::HashSet<(usize, usize)>,
    /// `&mut String::new()` for a `&mut String` parameter: a new cell holding the temporary.
    pub temp_cells: std::collections::HashSet<(usize, usize)>,
    /// Arguments for `impl Into<X>` parameters converted with `X::from` (`Crawl::from(ns, "home")`).
    pub into_args: std::collections::HashSet<(usize, usize)>,
    /// `get_mut` / `first_mut` / `last_mut` calls on numbers, strings or bools, where the result
    /// is matched or unwrapped: they yield the element's slot (value at `[1]`), so writes stick.
    pub slot_calls: std::collections::HashSet<(usize, usize)>,
    /// Bindings of such a slot (`Some(x)` in `if let`/`match`): `x` reads and writes `x[1]`.
    pub slot_bindings: std::collections::HashSet<(usize, usize)>,
    /// Reads of a fixed-size array of `Copy` values (a local or a struct field), which copy in
    /// by-value positions like rustc's `[T; N]: Copy`.
    pub array_reads: std::collections::HashSet<(usize, usize)>,
    /// `&mut place` results of `index_mut` bodies: a cell over the place (`κσ.$cell`).
    pub cell_returns: std::collections::HashSet<(usize, usize)>,
    /// `grid[i] = v` / `grid[i] += v` through `impl IndexMut`: written as `cell[0]`.
    pub index_places: std::collections::HashSet<(usize, usize)>,
    /// `&mut v[a..b]` of a Vec: a view whose writes reach `v` (`κσ.$sliceMut`), by the `&mut`.
    pub slice_views: std::collections::HashSet<(usize, usize)>,
    /// Auto-deref through `impl Deref`: these expressions are `x.deref()` (or `x.deref_mut()`,
    /// the `bool`), with the target type.
    pub derefs: HashMap<(usize, usize), (bool, Ty)>,
    /// `Trait::m(&x)` / `<T as Trait>::m(..)` calls, by the call's range.
    pub qualified_calls: HashMap<(usize, usize), QualifiedCall>,
    /// Generic functions that call `T::f()` / read `T::CONST` (directly or through another such
    /// function): the type parameters they take a class for, as trailing `$T` arguments.
    pub dict_params: HashMap<String, Vec<String>>,
    /// Calls of those functions: the types to pass a class for, in order, by the call's range.
    pub dict_args: HashMap<(usize, usize), Vec<Ty>>,
    /// The type parameters each function takes a `{:?}` formatter for (`$dbg$T`): those it
    /// prints with `{:?}` (or hands to a function that does), so `Some(..)` and whole floats
    /// print as the caller's type says.
    pub dbg_params: HashMap<String, Vec<String>>,
    /// Each generic call's types for those formatters.
    pub dbg_args: HashMap<(usize, usize), Vec<Ty>>,
    /// `mod name;` modules, in order.
    pub modules: Vec<ModuleRef>,
    /// Functions and constants from other modules.
    pub imported: Vec<Imported>,
    /// The `From` impl (or identity) each `Y::from(x)`, `x.into()` and converting `?` uses, by
    /// the expression's range.
    pub conversions: HashMap<(usize, usize), crate::adt::Conv>,
    /// std functions imported by name: name -> path after `std` (`max` -> `cmp::max`).
    pub std_fns: HashMap<String, Vec<String>>,
    /// Raw pointers.
    pub ptrs: Ptrs,
    /// How each `#var` of a `quote!` becomes tokens, by the name's range.
    pub quote_vars: HashMap<(usize, usize), QuoteTok>,
    /// The variables each `quote!` repetition iterates, by the offset of its `#`.
    pub quote_reps: HashMap<usize, Vec<String>>,
    /// Calls of the `js` crate's bound functions, by the call's range: (type for an associated
    /// function, function name).
    pub js_calls: HashMap<(usize, usize), (Option<String>, String)>,
    /// `T::from(x)` with `T: From<X>`: `X`'s name, by the call's range (the class's
    /// `$from$X`, or the number conversion).
    pub param_froms: HashMap<(usize, usize), String>,
    /// Structs whose values carry classes for some of their type parameters (`this.$T`), by
    /// position: what an `impl<T>`'s methods with `self` use for `T` (`a / b`, `s.parse::<T>()`).
    pub stored: HashMap<String, Vec<usize>>,
    /// Calls of the bundled crates' functions and methods (`rng.gen_range(..)`), by range.
    pub crate_calls: HashMap<(usize, usize), CrateCall>,
}

impl Types {
    pub fn get(&self, start: usize, end: usize) -> Option<&Ty> {
        self.map.get(&(start, end))
    }
}

#[derive(Clone)]
pub struct FnSig {
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub is_async: bool,
    /// The module it was imported from; None when declared in this file.
    pub module: Option<String>,
    /// Which parameters are `&mut` to a number, string, bool or Option (passed as cells).
    pub cell_params: Vec<bool>,
    /// Which parameters are `impl Into<X>` / `T: Into<X>` (an argument converts with `X::from`).
    pub into_params: Vec<bool>,
    /// Type parameters (not `Fn`-bounded ones) with their bounds.
    pub type_params: Vec<generic::TypeParam>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum VarKind {
    General,
    Int,
    Float,
}

enum VarState {
    Unbound(VarKind),
    Bound(Ty),
}

/// A type parameter's user-trait bounds with their arguments, by trait: (positional,
/// associated types given as `Unit = X`).
pub(super) type BoundArgs = HashMap<String, (Vec<Ty>, HashMap<String, Ty>)>;

#[derive(Clone)]
struct Local {
    ty: Ty,
    mutable: bool,
    /// `let x;`: assignable once per path even without `mut`.
    deferred: bool,
    /// `Some(true)` if the binding holds a `&mut` reference, `Some(false)` for `&`.
    reference: Option<bool>,
    /// The binding's name range (set by `declare`).
    decl: Option<(usize, usize)>,
}

/// What a mutation of a place is doing, for rustc-style error messages.
#[derive(Clone, Copy, PartialEq)]
enum Mutation {
    /// `x = value`
    Assign,
    /// `v[i] = value`, `*r = value`, `v.push(..)`, `&mut v`
    Borrow,
}

impl<'a> Checker<'a> {
    fn new(src: &'a str) -> Self {
        Checker {
            src,
            base: 0,
            vars: Vec::new(),
            raw: HashMap::new(),
            diags: Vec::new(),
            fns: HashMap::new(),
            scopes: Vec::new(),
            ret_ty: Ty::Unit,
            in_async: false,
            bindings: bindings::current(),
            pending_collects: Vec::new(),
            generics: HashMap::new(),
            type_aliases: HashMap::new(),
            adts: crate::adt::Adts::default(),
            self_ty: None,
            mut_bindings: Default::default(),
            pending_parses: Vec::new(),
            pending_defaults: Vec::new(),
            missing_steps: std::cell::Cell::new(0),
            consts: HashMap::new(),
            param_bounds: HashMap::new(),
            into_targets: HashMap::new(),
            nested_ok: Default::default(),
            pending_bounds: Vec::new(),
            loops: Vec::new(),
            boxed: Default::default(),
            cell_args: Default::default(),
            slot_args: Default::default(),
            temp_cells: Default::default(),
            into_args: Default::default(),
            slot_calls: Default::default(),
            slot_bindings: Default::default(),
            copy_arrays: Default::default(),
            move_sites: Default::default(),
            io_writes: Default::default(),
            array_fields: Default::default(),
            array_reads: Default::default(),
            cell_returns: Default::default(),
            index_places: Default::default(),
            slice_views: Default::default(),
            derefs: Default::default(),
            qualified_calls: Default::default(),
            pending_qualified: Vec::new(),
            cur_fn: None,
            dict_uses: Default::default(),
            dbg_uses: Default::default(),
            const_tys: HashMap::new(),
            from_args: HashMap::new(),
            const_needs: Default::default(),
            const_derivable: Default::default(),
            const_lits: Vec::new(),
            ctor_sites: Vec::new(),
            let_types: Vec::new(),
            crate_calls: HashMap::new(),
            crate_items: crates::crate_imports(src),
            stored_uses: Default::default(),
            param_froms: HashMap::new(),
            unbound_targs: Default::default(),
            generic_calls: Vec::new(),
            last_type_args: Vec::new(),
            borrow_events: Default::default(),
            ptrs: Default::default(),
            quote_vars: Default::default(),
            quote_reps: Default::default(),
            pending_json: Vec::new(),
            pending_tries: Vec::new(),
            js_calls: HashMap::new(),
            js_imports: HashMap::new(),
            opaque_index_mut: Default::default(),
            static_muts: Default::default(),
            statics: Default::default(),
            modules: Vec::new(),
            module_exports: HashMap::new(),
            imported: Vec::new(),
            pending_intos: Vec::new(),
            pending_try_intos: Vec::new(),
            std_fns: HashMap::new(),
            assoc_output: None,
            impl_assoc: HashMap::new(),
            param_trait_args: HashMap::new(),
            lenient_projs: Default::default(),
            pending_projections: Vec::new(),
            conversions: HashMap::new(),
        }
    }

    /// Everything other items can refer to: modules, types, traits, function signatures and
    /// constants.
    fn declare_items(
        &mut self,
        file: &ast::SourceFile,
        filename: &str,
        resolver: Resolver,
        stack: &mut Vec<String>,
    ) {
        self.load_modules(file, filename, resolver, stack);
        // (A struct's field may name a `const` as a const argument, `History<WINDOW>`, before
        // the consts are checked.)
        for item in crate::adt::all_items(file) {
            if let ast::Item::Const(k) = &item
                && let Some(n) = k.name()
            {
                self.consts
                    .entry(n.text().to_string())
                    .or_insert(Ty::Unknown);
            }
        }
        self.collect_adts(file);
        for item in crate::adt::all_items(file) {
            if let ast::Item::Fn(f) = &item
                && let Some(name) = f.name()
            {
                let type_params = self.enter_generics(f, true);
                let params: Vec<Ty> = f
                    .param_list()
                    .map(|pl| {
                        pl.params()
                            .map(|p| p.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown))
                            .collect()
                    })
                    .unwrap_or_default();
                let cell_params: Vec<bool> = f
                    .param_list()
                    .map(|pl| {
                        pl.params()
                            .zip(params.iter())
                            .map(|(p, t)| {
                                p.ty().as_ref().and_then(Self::ref_kind) == Some(true)
                                    && self.cellable(t)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let ret = f
                    .ret_type()
                    .and_then(|r| r.ty())
                    .map(|t| self.lower_type(&t))
                    .unwrap_or(Ty::Unit);
                if let Some(t) = f.ret_type().and_then(|r| r.ty())
                    && is_ptr_type(&t)
                    && self.cellable(&ret)
                {
                    self.error(
                        t.syntax(),
                        "returning a raw pointer to a number, string or bool isn't supported yet",
                    );
                }
                self.leave_generics();
                let sig = FnSig {
                    params,
                    ret,
                    is_async: f.async_token().is_some(),
                    type_params,
                    module: None,
                    cell_params,
                    into_params: into_param_flags(f),
                };
                if self
                    .fns
                    .get(name.text().to_string().as_str())
                    .is_some_and(|s| s.module.is_none())
                {
                    self.error(
                        name.syntax(),
                        format!(
                            "the name `{}` is defined multiple times (functions nested in other functions share the file's namespace)",
                            name.text()
                        ),
                    );
                }
                self.fns.insert(name.text().to_string(), sig);
            }
        }
        self.check_consts(file);
    }
}

/// Type-check `file`. `filename` is its path (for finding `mod` files) and `resolver` reads other
/// files.
pub fn check(
    file: &ast::SourceFile,
    src: &str,
    filename: &str,
    resolver: Resolver,
) -> (Types, Vec<RawDiag>) {
    let mut c = Checker::new(src);
    c.declare_items(file, filename, resolver, &mut vec![filename.to_string()]);
    c.scan_index_mut(file);
    for item in crate::adt::all_items(file) {
        if let ast::Item::Fn(f) = &item {
            c.check_fn(f);
        }
    }
    c.check_impls(file);
    c.check_trait_defaults(file);
    let map: HashMap<(usize, usize), Ty> = c.raw.iter().map(|(k, t)| (*k, c.zonk(t))).collect();
    let float_set = |t: &Ty| -> bool {
        fn walk(t: &Ty) -> bool {
            match t {
                Ty::Set(e) => e.contains_float() || walk(e),
                Ty::Map(k, v) => k.contains_float() || walk(k) || walk(v),
                Ty::Vec(e) | Ty::Option(e) | Ty::Future(e) => walk(e),
                Ty::Tuple(es) => es.iter().any(walk),
                _ => false,
            }
        }
        walk(t)
    };
    if let Some((&(start, end), ty)) = map
        .iter()
        .filter(|(_, t)| float_set(t))
        .min_by_key(|(k, _)| **k)
    {
        let message = format!(
            "the trait bound `f64: Hash` is not satisfied in `{}` (floats can't be hashed; store whole numbers or strings)",
            ty.describe()
        );
        c.diags.push(RawDiag {
            start,
            end,
            message,
            severity: "error",
        });
    }
    // (`Option<Option<T>>` and `Option<()>` are representable: `Some(None)` and `Some(())` are
    // markers.)
    if let Some((&(start, end), n)) = map
        .iter()
        .filter_map(|(k, t)| Some((k, c.unhashable_adt(t)?)))
        .min_by_key(|(k, _)| **k)
    {
        let message = format!(
            "the trait bound `{n}: Hash` is not satisfied (add `#[derive(PartialEq, Eq, Hash)]` to `{n}` to use it in a `HashSet` or as a `HashMap` key)"
        );
        c.diags.push(RawDiag {
            start,
            end,
            message,
            severity: "error",
        });
    }
    // Moves are only checked in programs that type-check: they rely on the types.
    if c.diags.iter().all(|d| d.severity != "error") {
        let (moves, sites) = crate::moves::check(file, src, &map, &c.adts, &c.copy_arrays);
        c.diags.extend(moves);
        c.move_sites = sites;
        let borrows = crate::borrows::check(file, src, 0, &c.borrow_events);
        c.diags.extend(borrows);
    }
    if let Some((&(start, end), ty)) = map
        .iter()
        .filter_map(|(k, t)| Some((k, c.unordered_coll(t)?)))
        .min_by_key(|(k, _)| **k)
    {
        let message = format!(
            "the trait bound `{ty}: Ord` is not satisfied (B-tree keys and heap items must be totally ordered: no floats; structs and enums need `#[derive(PartialEq, Eq, PartialOrd, Ord)]`)"
        );
        c.diags.push(RawDiag {
            start,
            end,
            message,
            severity: "error",
        });
    }
    // (Before the definitions move out: methods' type parameters are looked up there.)
    c.check_const_args();
    let dict_uses = std::mem::take(&mut c.dict_uses);
    let (dict_params, dict_args) = c.finish_needs(dict_uses, true);
    let dbg_uses = std::mem::take(&mut c.dbg_uses);
    let (dbg_params, dbg_args) = c.finish_needs(dbg_uses, false);
    let mut stored: HashMap<String, Vec<usize>> = HashMap::new();
    for (adt, i) in std::mem::take(&mut c.stored_uses) {
        stored.entry(adt).or_default().push(i);
    }
    for v in stored.values_mut() {
        v.sort();
    }
    let adts = std::mem::take(&mut c.adts);
    let mut_bindings = std::mem::take(&mut c.mut_bindings);
    let boxed = std::mem::take(&mut c.boxed);
    let cell_args = std::mem::take(&mut c.cell_args);
    let slot_args = std::mem::take(&mut c.slot_args);
    let temp_cells = std::mem::take(&mut c.temp_cells);
    let into_args = std::mem::take(&mut c.into_args);
    let slot_calls = std::mem::take(&mut c.slot_calls);
    let slot_bindings = std::mem::take(&mut c.slot_bindings);
    let array_reads = std::mem::take(&mut c.array_reads);
    let cell_returns = std::mem::take(&mut c.cell_returns);
    let index_places = std::mem::take(&mut c.index_places);
    let slice_views = std::mem::take(&mut c.slice_views);
    let derefs = std::mem::take(&mut c.derefs);
    let qualified_calls = std::mem::take(&mut c.qualified_calls);
    let modules = std::mem::take(&mut c.modules);
    let imported = std::mem::take(&mut c.imported);
    let conversions = std::mem::take(&mut c.conversions);
    let std_fns = std::mem::take(&mut c.std_fns);
    let ptrs = std::mem::take(&mut c.ptrs);
    let quote_vars = std::mem::take(&mut c.quote_vars);
    let quote_reps = std::mem::take(&mut c.quote_reps);
    let js_calls = std::mem::take(&mut c.js_calls);
    let param_froms = std::mem::take(&mut c.param_froms);
    let mut crate_calls = std::mem::take(&mut c.crate_calls);
    for cc in crate_calls.values_mut() {
        cc.tys = cc.tys.iter().map(|t| c.zonk(t)).collect();
    }
    (
        Types {
            map,
            adts,
            mut_bindings,
            move_sites: std::mem::take(&mut c.move_sites),
            io_writes: std::mem::take(&mut c.io_writes),
            boxed,
            cell_args,
            slot_args,
            temp_cells,
            into_args,
            slot_calls,
            slot_bindings,
            array_reads,
            cell_returns,
            index_places,
            slice_views,
            derefs,
            qualified_calls,
            dict_params,
            dict_args,
            dbg_params,
            dbg_args,
            modules,
            imported,
            conversions,
            std_fns,
            ptrs,
            quote_vars,
            quote_reps,
            js_calls,
            param_froms,
            stored,
            crate_calls,
        },
        c.diags,
    )
}

struct Checker<'a> {
    src: &'a str,
    /// Absolute offset of the text the current AST was parsed from (non-zero inside re-parsed
    /// macro arguments).
    base: usize,
    vars: Vec<VarState>,
    raw: HashMap<(usize, usize), Ty>,
    diags: Vec<RawDiag>,
    fns: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, Local>>,
    ret_ty: Ty,
    in_async: bool,
    bindings: Rc<Bindings>,
    /// `collect()` calls whose target type wasn't known yet: (target, item type, call node).
    pending_collects: Vec<(Ty, Ty, SyntaxNode, usize)>,
    /// The current signature's `F: Fn(..)` type parameters, as closure types.
    generics: HashMap<String, Ty>,
    /// `pub type Op = fn(i64) -> i64;` of the `mod` files loaded (this file's own aliases are
    /// expanded before checking).
    type_aliases: HashMap<String, Ty>,
    /// Structs, enums and their `impl` signatures.
    adts: crate::adt::Adts,
    /// `Self` inside an `impl` block.
    self_ty: Option<Ty>,
    mut_bindings: std::collections::HashSet<(usize, usize)>,
    /// `parse()` calls whose target type is checked at the end of the function.
    /// `parse()` calls: the target, the call, and its error type when it waits for the target.
    pending_parses: Vec<(Ty, SyntaxNode, Option<Ty>)>,
    /// `Default::default()` calls whose type was still unknown: checked at the end of the function.
    pending_defaults: Vec<(Ty, SyntaxNode)>,
    /// Work done by the current exhaustiveness check.
    missing_steps: std::cell::Cell<usize>,
    /// `const` and `static` items.
    consts: HashMap<String, Ty>,
    /// Bounds of the current function's type parameters.
    param_bounds: HashMap<String, Vec<String>>,
    /// `T: Into<f64>`: what `.into()` on a `T` makes (a number type; `T` stays generic).
    into_targets: HashMap<String, Ty>,
    /// Expressions of type `Option<Option<T>>` that only feed a `.flatten()`.
    nested_ok: std::collections::HashSet<(usize, usize)>,
    /// Bounds to check once a generic call's type arguments are known.
    pending_bounds: Vec<(Ty, String, SyntaxNode, usize)>,
    boxed: std::collections::HashSet<(usize, usize)>,
    cell_args: std::collections::HashSet<(usize, usize)>,
    slot_args: std::collections::HashSet<(usize, usize)>,
    temp_cells: std::collections::HashSet<(usize, usize)>,
    into_args: std::collections::HashSet<(usize, usize)>,
    slot_calls: std::collections::HashSet<(usize, usize)>,
    slot_bindings: std::collections::HashSet<(usize, usize)>,
    /// Locals and parameters that are fixed-size arrays of `Copy` values, by declaration.
    copy_arrays: std::collections::HashSet<(usize, usize)>,
    /// Uses that move a binding (filled by the move checker).
    move_sites: std::collections::HashSet<(usize, usize)>,
    /// `write!` destinations that are `io::Write`rs (by their start).
    io_writes: std::collections::HashSet<usize>,
    /// Struct fields declared as fixed-size arrays of `Copy` values: (struct, field).
    pub(super) array_fields: std::collections::HashSet<(String, String)>,
    array_reads: std::collections::HashSet<(usize, usize)>,
    cell_returns: std::collections::HashSet<(usize, usize)>,
    index_places: std::collections::HashSet<(usize, usize)>,
    slice_views: std::collections::HashSet<(usize, usize)>,
    derefs: HashMap<(usize, usize), (bool, Ty)>,
    qualified_calls: HashMap<(usize, usize), QualifiedCall>,
    ptrs: Ptrs,
    quote_vars: HashMap<(usize, usize), QuoteTok>,
    quote_reps: HashMap<usize, Vec<String>>,
    /// `JSON::stringify(&v)` / `JSON::parse::<T>(s)` types to check once settled: (type, node,
    /// whether parsing).
    pending_json: Vec<(Ty, SyntaxNode, bool)>,
    /// `?`s whose error type wasn't known yet: (the `?`, its error, the function's).
    pending_tries: Vec<(ast::TryExpr, Ty, Ty)>,
    js_calls: HashMap<(usize, usize), (Option<String>, String)>,
    /// `use js::set_timeout as later;`: local name → the `js` crate's function.
    js_imports: HashMap<String, String>,
    /// Qualified calls whose `Self` is settled at the end of the function: (call, `Self`,
    /// whether the trait is local, node).
    pending_qualified: Vec<((usize, usize), Ty, bool, SyntaxNode)>,
    /// The free function being checked.
    cur_fn: Option<String>,
    /// (function, type parameter) pairs where the body calls `T::f()` or reads `T::CONST`.
    dict_uses: std::collections::HashSet<(String, String)>,
    /// (function, type parameter) pairs whose values the function prints with `{:?}`.
    dbg_uses: std::collections::HashSet<(String, String)>,
    /// `T: From<X>` bounds in scope: `X` by parameter.
    pub(super) from_args: HashMap<String, Ty>,
    /// The const generic parameters in scope (`const N: usize`), with their types.
    pub(super) const_tys: HashMap<String, Ty>,
    /// (function, const parameter) pairs: each is passed like a class (`$N`)...
    const_needs: std::collections::HashSet<(String, String)>,
    /// ...unless the function can read it off an argument (`a: [T; N]` gives `a.length`).
    const_derivable: std::collections::HashSet<(String, String)>,
    /// `T::from(x)` calls: `X`'s name by range.
    param_froms: HashMap<(usize, usize), String>,
    /// Struct literals of structs with const parameters: (range, struct, type arguments).
    const_lits: Vec<((usize, usize), String, Vec<Ty>)>,
    /// Calls of the bundled crates (`rand`), by range.
    crate_calls: HashMap<(usize, usize), CrateCall>,
    /// Names the bundled crates' `use`s bring in (`use anyhow::Result;`): alias -> (crate, item).
    crate_items: HashMap<String, (String, String)>,
    /// `let` bindings without a written type: (the name's range, its type). One nothing settles
    /// (`let v = Vec::new();` never used) is rustc's "type annotations needed".
    let_types: Vec<((usize, usize), Ty)>,
    /// Every construction of a generic struct: (the function it's in, struct, type arguments).
    /// A struct that carries a class for its `T` gets it there (`$T`, or `this.$T`).
    ctor_sites: Vec<(Option<String>, String, Vec<Ty>)>,
    /// (struct, type parameter position): its values carry that parameter's class.
    stored_uses: std::collections::HashSet<(String, usize)>,
    /// Generic calls' type arguments nothing settled (by call range and position), as opposed
    /// to ones left unknown by an unchecked type.
    unbound_targs: std::collections::HashSet<((usize, usize), usize)>,
    /// Generic calls: (call, caller, callee, the callee's type arguments).
    generic_calls: Vec<GenericCall>,
    /// The type variables the last `instantiate` made, in order.
    last_type_args: Vec<Ty>,
    /// Borrows, uses and changes of locals, for the borrow-conflict warnings.
    borrow_events: crate::borrows::Events,
    /// Types whose `index_mut` doesn't end in `&mut <place>`: assigning through them can't work.
    opaque_index_mut: std::collections::HashSet<String>,
    /// `static mut` items, by name: using one takes an `unsafe` block or function.
    static_muts: std::collections::HashSet<String>,
    /// The other `static` items, which can't be assigned to.
    statics: std::collections::HashSet<String>,
    /// Enclosing loops, innermost last: (label, the value type for a `loop`; None for
    /// `while`/`for`, which can't break with a value), and whether some `break` targets it.
    loops: Vec<(Option<String>, Option<Ty>, bool, bool)>,
    /// Loaded modules, and the names each makes public.
    modules: Vec<ModuleRef>,
    module_exports: HashMap<String, std::collections::HashSet<String>>,
    imported: Vec<Imported>,
    /// `.into()` calls, settled at the end of the function: (source, target, call node).
    pending_intos: Vec<(Ty, Ty, SyntaxNode, usize)>,
    /// `x.try_into()` calls: (source, target, error type, call, base).
    pending_try_intos: Vec<(Ty, Ty, Ty, SyntaxNode, usize)>,
    /// std functions imported by name (`use std::cmp::max;`): name -> path after `std`.
    std_fns: HashMap<String, Vec<String>>,
    /// `type Output = ..;` of the operator impl being declared.
    assoc_output: Option<Ty>,
    /// The `impl`'s associated types (`type Err = String;`), for `Self::Err` in its signatures.
    pub(super) impl_assoc: HashMap<String, Ty>,
    /// Each type parameter's user-trait bounds with their arguments: (positional, `Unit = X`).
    pub(super) param_trait_args: HashMap<String, BoundArgs>,
    /// Projections a `where` clause constrains (`where I::Item: PartialEq<T>`): unchecked.
    pub(super) lenient_projs: std::collections::HashSet<String>,
    /// Generic calls' projections waiting for their type argument: (`S`'s variable, trait,
    /// associated type, the projection's variable).
    pub(super) pending_projections: Vec<(Ty, String, String, Ty)>,
    conversions: HashMap<(usize, usize), crate::adt::Conv>,
}

const USIZE: Ty = Ty::Int(IntKind::Usize);

impl<'a> Checker<'a> {
    // ------------------------------------------------------------ positions & diagnostics

    fn range(&self, n: &SyntaxNode) -> (usize, usize) {
        let r = n.text_range();
        (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        )
    }

    /// An endless loop that never yields: in Bitburner, the game freezes.
    fn warn_freeze(&mut self, loop_node: &SyntaxNode) {
        let (start, _) = self.range(loop_node);
        // Underline just the `loop` / `while` keyword.
        let end = start + loop_node.first_token().map(|t| t.text().len()).unwrap_or(4);
        self.diags.push(RawDiag {
            start,
            end,
            message: "this loop never ends and never awaits, so it would freeze Bitburner: add `ns.sleep(..).await` (or another awaited NS call) inside it".into(),
            severity: "warning",
        });
    }

    fn error(&mut self, n: &SyntaxNode, message: impl Into<String>) {
        let (start, end) = self.range(n);
        self.diags.push(RawDiag {
            start,
            end,
            message: message.into(),
            severity: "error",
        });
    }

    fn record(&mut self, n: &SyntaxNode, ty: Ty) -> Ty {
        let key = self.range(n);
        self.raw.insert(key, ty.clone());
        ty
    }

    // ------------------------------------------------------------ inference variables

    fn fresh(&mut self, kind: VarKind) -> Ty {
        self.vars.push(VarState::Unbound(kind));
        Ty::Var(self.vars.len() as u32 - 1)
    }

    /// Follow bound variables until reaching a concrete type or an unbound variable.
    fn shallow(&self, t: &Ty) -> Ty {
        let mut t = t.clone();
        while let Ty::Var(v) = t {
            match &self.vars[v as usize] {
                VarState::Bound(b) => t = b.clone(),
                VarState::Unbound(_) => break,
            }
        }
        t
    }

    /// Whether a variable nothing settled is left in `t` (number literals' take defaults).
    fn has_unbound(&self, t: &Ty) -> bool {
        match self.shallow(t) {
            Ty::Var(_) => self.var_kind(t) == Some(VarKind::General),
            // (Only the arguments its fields' types use: a const parameter or one lowered to
            // an iterator never settles here.)
            Ty::Adt(n, args) => {
                let Some(def) = self.adts.defs.get(&n) else {
                    return false;
                };
                args.iter().zip(&def.type_params).any(|(a, p)| {
                    let param = Ty::Param(p.clone());
                    let used = def.variants.iter().flat_map(|v| &v.fields).any(|f| {
                        let mut hit = false;
                        f.ty.walk(&mut |t| hit |= *t == param);
                        hit
                    });
                    used && self.has_unbound(a)
                })
            }
            s => s.inner_types().iter().any(|i| self.has_unbound(i)),
        }
    }

    fn var_kind(&self, t: &Ty) -> Option<VarKind> {
        match self.shallow(t) {
            Ty::Var(v) => match self.vars[v as usize] {
                VarState::Unbound(k) => Some(k),
                VarState::Bound(_) => None,
            },
            _ => None,
        }
    }

    /// Fully resolve a type. Leftover literal variables take rustc's defaults; anything else
    /// unconstrained becomes Unknown.
    fn zonk(&self, t: &Ty) -> Ty {
        match self.shallow(t) {
            Ty::Var(v) => match self.vars[v as usize] {
                VarState::Unbound(VarKind::Int) => Ty::Int(IntKind::I32),
                VarState::Unbound(VarKind::Float) => Ty::Float { f32: false },
                _ => Ty::Unknown,
            },
            Ty::Vec(e) => Ty::Vec(Box::new(self.zonk(&e))),
            Ty::Option(e) => Ty::Option(Box::new(self.zonk(&e))),
            Ty::Map(k, v) => Ty::Map(Box::new(self.zonk(&k)), Box::new(self.zonk(&v))),
            Ty::Entry(k, v) => Ty::Entry(Box::new(self.zonk(&k)), Box::new(self.zonk(&v))),
            Ty::RefCell(t) => Ty::RefCell(Box::new(self.zonk(&t))),
            Ty::Adt(n, args) => Ty::Adt(n, args.iter().map(|a| self.zonk(a)).collect()),
            Ty::Result(t, e) => Ty::Result(Box::new(self.zonk(&t)), Box::new(self.zonk(&e))),
            Ty::Closure { params, ret } => Ty::Closure {
                params: params.iter().map(|p| self.zonk(p)).collect(),
                ret: Box::new(self.zonk(&ret)),
            },
            Ty::Set(e) => Ty::Set(Box::new(self.zonk(&e))),
            Ty::Coll(k, v) => Ty::Coll(k, Box::new(self.zonk(&v))),
            Ty::Tuple(es) => Ty::Tuple(es.iter().map(|e| self.zonk(e)).collect()),
            Ty::Future(e) => Ty::Future(Box::new(self.zonk(&e))),
            Ty::Iter { elem, mutable } => Ty::Iter {
                elem: Box::new(self.zonk(&elem)),
                mutable,
            },
            Ty::IntoIter(e) => Ty::IntoIter(Box::new(self.zonk(&e))),
            Ty::Range(e, incl) => Ty::Range(Box::new(self.zonk(&e)), incl),
            other => other,
        }
    }

    fn describe(&self, t: &Ty) -> String {
        match self.shallow(t) {
            Ty::Var(_) => match self.var_kind(t) {
                Some(VarKind::Int) => "{integer}".into(),
                Some(VarKind::Float) => "{float}".into(),
                _ => "_".into(),
            },
            Ty::Vec(e) => format!("Vec<{}>", self.describe(&e)),
            Ty::Option(e) => format!("Option<{}>", self.describe(&e)),
            Ty::Set(e) => format!("HashSet<{}>", self.describe(&e)),
            Ty::Coll(k, v) => k.describe(&self.describe(&v)),
            Ty::Map(k, v) => format!("HashMap<{}, {}>", self.describe(&k), self.describe(&v)),
            Ty::Entry(k, v) => format!("Entry<'_, {}, {}>", self.describe(&k), self.describe(&v)),
            Ty::RefCell(t) => format!("RefCell<{}>", self.describe(&t)),
            Ty::Adt(n, args) if !args.is_empty() => format!(
                "{n}<{}>",
                args.iter()
                    .map(|a| self.describe(a))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Ty::Result(t, e) => format!("Result<{}, {}>", self.describe(&t), self.describe(&e)),
            Ty::Closure { params, ret } => crate::ty::describe_closure(
                params.iter().map(|p| self.describe(p)).collect(),
                self.describe(&ret),
            ),
            Ty::Tuple(es) => {
                crate::ty::describe_tuple(es.iter().map(|e| self.describe(e)).collect())
            }
            Ty::Future(e) => format!("impl Future<Output = {}>", self.describe(&e)),
            Ty::IntoIter(e) => format!("impl IntoIterator<Item = {}>", self.describe(&e)),
            Ty::Iter { elem, mutable } => {
                format!(
                    "std::slice::Iter{}<'_, {}>",
                    if mutable { "Mut" } else { "" },
                    self.describe(&elem)
                )
            }
            other => other.describe(),
        }
    }

    fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let (a, b) = (self.shallow(a), self.shallow(b));
        // `B::Item` where `B: Array<Item = A::Item>`: compare as `A::Item`.
        let (a, b) = (self.normalize_proj(a), self.normalize_proj(b));
        match (&a, &b) {
            (Ty::Never, _) | (_, Ty::Never) | (Ty::Unknown, _) | (_, Ty::Unknown) => true,
            // A trait's own associated type or type parameter (`Shape::Unit`, in its declared
            // signatures): whatever the impl says, so it matches any type.
            (Ty::Param(p), _) | (_, Ty::Param(p)) if self.is_template(p) => true,
            (Ty::Var(x), Ty::Var(y)) if x == y => true,
            (Ty::Var(x), Ty::Var(y)) => {
                let (kx, ky) = (self.var_kind(&a).unwrap(), self.var_kind(&b).unwrap());
                match (kx, ky) {
                    (VarKind::General, _) => self.vars[*x as usize] = VarState::Bound(b.clone()),
                    (_, VarKind::General) => self.vars[*y as usize] = VarState::Bound(a.clone()),
                    (k1, k2) if k1 == k2 => self.vars[*x as usize] = VarState::Bound(b.clone()),
                    _ => return false,
                }
                true
            }
            (Ty::Var(v), t) | (t, Ty::Var(v)) => {
                let ok = match self.var_kind(&Ty::Var(*v)).unwrap() {
                    VarKind::General => true,
                    VarKind::Int => matches!(t, Ty::Int(_)),
                    VarKind::Float => matches!(t, Ty::Float { .. }),
                };
                if ok {
                    self.vars[*v as usize] = VarState::Bound(t.clone());
                }
                ok
            }
            (Ty::Vec(x), Ty::Vec(y))
            | (Ty::Future(x), Ty::Future(y))
            | (Ty::Option(x), Ty::Option(y))
            | (Ty::RefCell(x), Ty::RefCell(y))
            | (Ty::Set(x), Ty::Set(y)) => self.unify(x, y),
            (Ty::Coll(a, x), Ty::Coll(b, y)) if a == b => self.unify(x, y),
            (Ty::IntoIter(x), Ty::IntoIter(y)) => {
                let (x, y) = (x.clone(), y.clone());
                self.unify(&x, &y)
            }
            // An `IntoIterator<Item = T>` parameter takes any iterable of `T` but an `Option`
            // (`Some(x)` is `x` at run time).
            (Ty::IntoIter(e), other) | (other, Ty::IntoIter(e))
                if !matches!(other, Ty::Var(_) | Ty::Option(_) | Ty::Param(_)) =>
            {
                let e = e.clone();
                match self.iter_elem(other) {
                    Some(item) => self.unify(&e, &item),
                    None => false,
                }
            }
            // (A `const` item's value isn't known here: it matches any value.)
            (Ty::ConstVal(x), Ty::ConstVal(y)) => x == y || !(const_literal(x) && const_literal(y)),
            (Ty::Adt(a, xs), Ty::Adt(b, ys)) if a == b && xs.len() == ys.len() => {
                let (xs, ys) = (xs.clone(), ys.clone());
                xs.iter().zip(ys.iter()).all(|(x, y)| self.unify(x, y))
            }
            // A struct or enum coerces to a `dyn Trait` it implements.
            (Ty::Dyn(tr), Ty::Adt(..)) | (Ty::Adt(..), Ty::Dyn(tr)) => {
                let adt = if matches!(a, Ty::Adt(..)) {
                    a.clone()
                } else {
                    b.clone()
                };
                let tr = tr.clone();
                self.implements_user_trait(&adt, &tr)
            }
            // A std error (`ParseIntError`) is a `dyn Error`.
            (Ty::Dyn(tr), Ty::Extern(n)) | (Ty::Extern(n), Ty::Dyn(tr))
                if tr == "Error" && is_std_error(n) =>
            {
                true
            }
            (Ty::Map(k1, v1), Ty::Map(k2, v2))
            | (Ty::Entry(k1, v1), Ty::Entry(k2, v2))
            | (Ty::Result(k1, v1), Ty::Result(k2, v2)) => {
                let (k1, v1, k2, v2) = (k1.clone(), v1.clone(), k2.clone(), v2.clone());
                self.unify(&k1, &k2) && self.unify(&v1, &v2)
            }
            (
                Ty::Closure {
                    params: p1,
                    ret: r1,
                },
                Ty::Closure {
                    params: p2,
                    ret: r2,
                },
            ) => {
                let (p1, p2, r1, r2) = (p1.clone(), p2.clone(), r1.clone(), r2.clone());
                p1.len() == p2.len()
                    && p1.iter().zip(p2.iter()).all(|(x, y)| self.unify(x, y))
                    && self.unify(&r1, &r2)
            }
            (Ty::Tuple(xs), Ty::Tuple(ys)) => {
                xs.len() == ys.len()
                    && xs
                        .clone()
                        .iter()
                        .zip(ys.clone().iter())
                        .all(|(x, y)| self.unify(x, y))
            }
            (
                Ty::Iter {
                    elem: x,
                    mutable: m1,
                },
                Ty::Iter {
                    elem: y,
                    mutable: m2,
                },
            ) => m1 == m2 && self.unify(x, y),
            (Ty::Range(x, i1), Ty::Range(y, i2)) => i1 == i2 && self.unify(x, y),
            // A range expression is an iterator until something asks for a `Range`.
            (
                Ty::Range(x, _),
                Ty::Iter {
                    elem: y,
                    mutable: false,
                },
            )
            | (
                Ty::Iter {
                    elem: y,
                    mutable: false,
                },
                Ty::Range(x, _),
            ) => {
                let (x, y) = (x.clone(), y.clone());
                self.unify(&x, &y)
            }
            _ => a == b,
        }
    }

    /// Unify `found` with `expected`, reporting rustc's "mismatched types" on failure.
    fn expect(&mut self, n: &SyntaxNode, found: &Ty, expected: &Ty) {
        if let (Ty::Dyn(tr), Ty::Adt(adt, _)) = (self.shallow(expected), self.shallow(found))
            && !self.unify(found, expected)
        {
            let msg = format!(
                "the trait bound `{adt}: {tr}` is not satisfied (add `impl {tr} for {adt}`)"
            );
            self.error(n, msg);
            return;
        }
        if !self.unify(found, expected) && !self.iface_coerces(found, expected) {
            let hint = match (self.shallow(expected), self.var_kind(found)) {
                (Ty::Float { .. }, Some(VarKind::Int)) => {
                    " (write `1.0`, or convert with `as f64`)"
                }
                (Ty::Float { .. }, _) if matches!(self.shallow(found), Ty::Int(_)) => {
                    " (convert with `as f64`)"
                }
                (Ty::Int(_), _) if matches!(self.shallow(found), Ty::Float { .. }) => {
                    " (NS numbers are `f64`; convert with `as`)"
                }
                (_, _) if matches!(self.shallow(found), Ty::Future(_)) => {
                    " (did you forget `.await`?)"
                }
                _ => "",
            };
            let msg = format!(
                "mismatched types: expected `{}`, found `{}`{hint}",
                self.describe(expected),
                self.describe(found)
            );
            self.error(n, msg);
        }
    }

    // ------------------------------------------------------------ scopes

    fn declare(&mut self, name: &ast::Name, mut local: Local) {
        local.decl = Some(self.range(name.syntax()));
        let ty = local.ty.clone();
        self.record(name.syntax(), ty);
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.text().to_string(), local);
    }

    fn lookup(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    // ------------------------------------------------------------ types

    fn lower_type(&mut self, t: &ast::Type) -> Ty {
        match t {
            ast::Type::RefType(r) => r.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            ast::Type::PtrType(r) => r.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            ast::Type::ParenType(p) => p.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            ast::Type::TupleType(tt) if tt.fields().next().is_none() => Ty::Unit,
            ast::Type::TupleType(tt) => {
                Ty::Tuple(tt.fields().map(|t| self.lower_type(&t)).collect())
            }
            ast::Type::NeverType(_) => Ty::Never,
            ast::Type::InferType(_) => self.fresh(VarKind::General),
            ast::Type::ImplTraitType(_) | ast::Type::DynTraitType(_) | ast::Type::FnPtrType(_)
                if self.lower_fn_type(t).is_some() =>
            {
                self.lower_fn_type(t).unwrap()
            }
            // `impl Into<String>` / `impl AsRef<str>` / `impl Borrow<T>`: the target type itself
            // (strings and references are erased, so `&str` and `String` both pass).
            ast::Type::ImplTraitType(i)
                if i.type_bound_list()
                    .and_then(|l| conversion_target(&l))
                    .is_some() =>
            {
                let target = i
                    .type_bound_list()
                    .and_then(|l| conversion_target(&l))
                    .unwrap();
                self.lower_type(&target)
            }
            // `state: &mut impl Hasher`
            ast::Type::ImplTraitType(i)
                if i.type_bound_list().is_some_and(|l| bounds_hasher(&l)) =>
            {
                Ty::Hasher
            }
            // `rng: &mut impl Rng`, `&mut dyn RngCore`
            ast::Type::ImplTraitType(i)
                if i.type_bound_list().is_some_and(|l| crates::bounds_rng(&l)) =>
            {
                Ty::Rng
            }
            ast::Type::DynTraitType(d)
                if d.type_bound_list().is_some_and(|l| crates::bounds_rng(&l)) =>
            {
                Ty::Rng
            }
            // `impl IntoIterator<Item = T>`: any iterable of `T`.
            ast::Type::ImplTraitType(i)
                if i.type_bound_list()
                    .and_then(|l| bounds_into_iter_item(&l))
                    .is_some() =>
            {
                let item = i
                    .type_bound_list()
                    .and_then(|l| bounds_into_iter_item(&l))
                    .unwrap();
                let elem = match item {
                    Some(t) => self.lower_type(&t),
                    None => self.fresh(VarKind::General),
                };
                Ty::IntoIter(Box::new(elem))
            }
            // `impl Iterator<Item = T>` / `Box<dyn Iterator<Item = T>>`: the runtime's iterator.
            ast::Type::ImplTraitType(_) | ast::Type::DynTraitType(_)
                if iterator_item(t).is_some() =>
            {
                let elem = match iterator_item(t).unwrap() {
                    Some(item) => self.lower_type(&item),
                    None => self.fresh(VarKind::General),
                };
                Ty::Iter {
                    elem: Box::new(elem),
                    mutable: false,
                }
            }
            // `impl Future<Output = T>` / `Pin<Box<dyn Future<Output = T> + '_>>`: a promise.
            ast::Type::ImplTraitType(_) | ast::Type::DynTraitType(_)
                if future_output(t).is_some() =>
            {
                let out = match future_output(t).unwrap() {
                    Some(o) => self.lower_type(&o),
                    None => Ty::Unit,
                };
                Ty::Future(Box::new(out))
            }
            ast::Type::ImplTraitType(_) | ast::Type::DynTraitType(_)
                if self.user_trait_type(t).is_some() =>
            {
                Ty::Dyn(self.user_trait_type(t).unwrap())
            }
            // `Box<dyn Error>`: any error value (they all display themselves).
            ast::Type::ImplTraitType(_) | ast::Type::DynTraitType(_) if names_std_error(t) => {
                Ty::Dyn("Error".into())
            }
            ast::Type::SliceType(s) => Ty::Vec(Box::new(
                s.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            )),
            ast::Type::ArrayType(a) => Ty::Vec(Box::new(
                a.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            )),
            ast::Type::PathType(p) => {
                let seg = p.path().and_then(|p| p.segment());
                let name = seg
                    .as_ref()
                    .and_then(|s| s.name_ref())
                    .map(|n| n.text().to_string())
                    .unwrap_or_default();
                if let Some(kind) = crate::ty::Coll::from_name(&name) {
                    let args: Vec<Ty> = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .map(|l| l.generic_args().collect::<Vec<_>>())
                        .unwrap_or_default()
                        .into_iter()
                        .map(|g| match g {
                            ast::GenericArg::TypeArg(ta) => {
                                ta.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown)
                            }
                            _ => self.fresh(VarKind::General),
                        })
                        .collect();
                    return self.coll_type(kind, args);
                }
                if name == "HashMap" {
                    let mut args = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .map(|l| l.generic_args().collect::<Vec<_>>())
                        .unwrap_or_default()
                        .into_iter();
                    let mut next = |c: &mut Self| match args.next() {
                        Some(ast::GenericArg::TypeArg(ta)) => {
                            ta.ty().map(|t| c.lower_type(&t)).unwrap_or(Ty::Unknown)
                        }
                        _ => c.fresh(VarKind::General),
                    };
                    let k = next(self);
                    let v = next(self);
                    return Ty::Map(Box::new(k), Box::new(v));
                }
                // `tokio::task::JoinHandle<T>`: a future of `Result<T, JoinError>`.
                if name == "TokioJoinHandle" {
                    let t = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next())
                        .and_then(|g| match g {
                            ast::GenericArg::TypeArg(ta) => ta.ty(),
                            _ => None,
                        })
                        .map(|t| self.lower_type(&t))
                        .unwrap_or_else(|| self.fresh(VarKind::General));
                    return Ty::Future(Box::new(Ty::Result(
                        Box::new(t),
                        Box::new(Ty::Adt("TokioJoinError".into(), vec![])),
                    )));
                }
                // `Range<usize>` / `RangeInclusive<i32>` (`std::ops::..`).
                if (name == "Range" || name == "RangeInclusive")
                    && !self.adts.defs.contains_key(&name)
                {
                    let elem = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next())
                        .and_then(|g| match g {
                            ast::GenericArg::TypeArg(ta) => ta.ty(),
                            _ => None,
                        })
                        .map(|t| self.lower_type(&t))
                        .unwrap_or_else(|| self.fresh(VarKind::General));
                    return Ty::Range(Box::new(elem), name == "RangeInclusive");
                }
                if name == "HashSet" {
                    let arg = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next());
                    let elem = match arg {
                        Some(ast::GenericArg::TypeArg(ta)) => ta.ty().map(|t| self.lower_type(&t)),
                        _ => None,
                    };
                    return Ty::Set(Box::new(
                        elem.unwrap_or_else(|| self.fresh(VarKind::General)),
                    ));
                }
                // `std::cmp::Ordering` (in `fn cmp(&self, o: &Self) -> Ordering`)
                if name == "Ordering" && !self.adts.defs.contains_key("Ordering") {
                    return Ty::Ordering;
                }
                // `anyhow::Error`, and `anyhow::Result<T>` (its error by default).
                if (name == "Error" || name == "Result")
                    && self.uses_crate("anyhow")
                    && p.path()
                        .is_some_and(|path| self.names_crate_item(&path, "anyhow", &name))
                {
                    if name == "Error" {
                        return crates::anyhow_error();
                    }
                    let mut it = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .map(|l| l.generic_args().collect::<Vec<_>>())
                        .unwrap_or_default()
                        .into_iter();
                    let mut next = |c: &mut Self| match it.next() {
                        Some(ast::GenericArg::TypeArg(ta)) => ta.ty().map(|t| c.lower_type(&t)),
                        _ => None,
                    };
                    let t = next(self).unwrap_or(Ty::Unit);
                    let e = next(self).unwrap_or_else(crates::anyhow_error);
                    return Ty::Result(Box::new(t), Box::new(e));
                }
                // `serde_json::Error`, and `serde_json::Result<T>`.
                if (name == "Error" || name == "Result")
                    && self.uses_crate("serde_json")
                    && p.path()
                        .is_some_and(|path| self.names_crate_item(&path, "serde_json", &name))
                {
                    if name == "Error" {
                        return crates::serde_json_error();
                    }
                    let t = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next())
                        .and_then(|g| match g {
                            ast::GenericArg::TypeArg(ta) => ta.ty(),
                            _ => None,
                        })
                        .map(|t| self.lower_type(&t))
                        .unwrap_or(Ty::Unit);
                    return Ty::Result(Box::new(t), Box::new(crates::serde_json_error()));
                }
                if name == "Result" {
                    let args: Vec<ast::GenericArg> = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .map(|l| l.generic_args().collect())
                        .unwrap_or_default();
                    if args.is_empty() {
                        // `fmt::Result`
                        let qualified = p
                            .path()
                            .is_some_and(|p| p.syntax().text().to_string().contains("fmt"));
                        if qualified {
                            return Ty::Result(
                                Box::new(Ty::Unit),
                                Box::new(Ty::Extern("fmt::Error".into())),
                            );
                        }
                    }
                    let mut it = args.into_iter();
                    let mut next = |c: &mut Self| match it.next() {
                        Some(ast::GenericArg::TypeArg(ta)) => {
                            ta.ty().map(|t| c.lower_type(&t)).unwrap_or(Ty::Unknown)
                        }
                        _ => c.fresh(VarKind::General),
                    };
                    let t = next(self);
                    let e = next(self);
                    return Ty::Result(Box::new(t), Box::new(e));
                }
                if name == "Formatter" {
                    return Ty::Formatter;
                }
                // (A struct or enum of the file's named `Cell` is that type, as in Rust.)
                if matches!(name.as_str(), "RefCell" | "Cell")
                    && !self.adts.defs.contains_key(&name)
                {
                    let arg = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next());
                    let inner = match arg {
                        Some(ast::GenericArg::TypeArg(ta)) => {
                            ta.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown)
                        }
                        _ => self.fresh(VarKind::General),
                    };
                    return Ty::RefCell(Box::new(inner));
                }
                // `Box<T>`, `Rc<T>` and `Arc<T>` are just `T` (shared immutable data can't tell
                // sharing from copying).
                // `Weak<T>`: the value or `null` (`Weak::new()`); `upgrade()` hands it back.
                if name == "Weak" {
                    let arg = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next());
                    let inner = match arg {
                        Some(ast::GenericArg::TypeArg(ta)) => {
                            ta.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown)
                        }
                        _ => self.fresh(VarKind::General),
                    };
                    return Ty::Option(Box::new(inner));
                }
                if matches!(name.as_str(), "Box" | "Rc" | "Arc")
                    || (name == "Pin" && !self.adts.defs.contains_key(&name))
                {
                    let arg = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next());
                    return match arg {
                        Some(ast::GenericArg::TypeArg(ta)) => {
                            ta.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown)
                        }
                        _ => self.fresh(VarKind::General),
                    };
                }
                if is_std_error(&name) {
                    return Ty::Extern(name);
                }
                if name == "Option" {
                    let arg = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next());
                    let inner = match arg {
                        Some(ast::GenericArg::TypeArg(ta)) => ta.ty().map(|t| self.lower_type(&t)),
                        _ => None,
                    };
                    return Ty::Option(Box::new(
                        inner.unwrap_or_else(|| self.fresh(VarKind::General)),
                    ));
                }
                if name == "Vec" {
                    let arg = seg
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next());
                    let elem = match arg {
                        Some(ast::GenericArg::TypeArg(ta)) => ta.ty().map(|t| self.lower_type(&t)),
                        _ => None,
                    };
                    return Ty::Vec(Box::new(
                        elem.unwrap_or_else(|| self.fresh(VarKind::General)),
                    ));
                }
                if let Some(g) = self.generics.get(&name) {
                    return g.clone();
                }
                if self.adts.defs.contains_key(&name) {
                    // `Stack<i32>`, `Buf<4>`; missing arguments are inferred (or a const
                    // parameter's default).
                    let given: Vec<Ty> = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .map(|l| l.generic_args().collect::<Vec<_>>())
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|g| self.generic_arg_ty(g))
                        .collect();
                    let n_given = given.len();
                    let mut args = self.fresh_args(&name);
                    for (a, g) in args.iter_mut().zip(given) {
                        *a = g;
                    }
                    let params = self.adts.defs[&name].type_params.clone();
                    if let Some(cps) = self.adts.const_params.get(&name) {
                        for (i, p) in params.iter().enumerate().skip(n_given) {
                            if let Some(d) = cps
                                .iter()
                                .find(|c| &c.name == p)
                                .and_then(|c| c.default.clone())
                            {
                                args[i] = Ty::ConstVal(d);
                            }
                        }
                    }
                    return Ty::Adt(name, args);
                }
                if name == "Self"
                    && let Some(t) = &self.self_ty
                {
                    return t.clone();
                }
                // `Self::Err` / `Self::Unit`: the impl's `type Err = ..;`.
                if p.path()
                    .and_then(|p| p.qualifier())
                    .is_some_and(|q| q.syntax().text() == "Self")
                    && let Some(t) = self.impl_assoc.get(&name)
                {
                    return t.clone();
                }
                // `Self::Item` in an `impl DoubleEndedIterator` (its `Iterator` impl's).
                if name == "Item"
                    && let Some(t) = self.gaps_self_item(p)
                {
                    return t;
                }
                // `T::Err` of a `T: FromStr`: unchecked.
                if name == "Err"
                    && let Some(q) = p.path().and_then(|p| p.qualifier())
                    && let Some(qn) = q.as_single_name_ref()
                    && self.param_has(qn.text().to_string().as_str(), "FromStr")
                {
                    return Ty::Unknown;
                }
                // `I::Item` of an `I: IntoIterator` (its own type parameter).
                if let Some(q) = p.path().and_then(|p| p.qualifier())
                    && let Some(qn) = q.as_single_name_ref()
                    && let Some(t) = self.generics.get(&format!("{}::{name}", qn.text()))
                {
                    return t.clone();
                }
                // `S::Unit` of a type parameter bounded by a trait with `type Unit;`.
                if let Some(q) = p.path().and_then(|p| p.qualifier())
                    && let Some(qn) = q.as_single_name_ref()
                    && self
                        .param_bounds
                        .contains_key(qn.text().to_string().as_str())
                    && let Some(t) = self.projection(qn.text().to_string().as_str(), &name)
                {
                    return t;
                }
                // `Self::Output` in an operator impl, `Self::Item` in an iterator.
                if (name == "Output" || name == "Item")
                    && let Some(t) = &self.assoc_output
                    && p.path()
                        .and_then(|p| p.qualifier())
                        .is_some_and(|q| q.syntax().text() == "Self")
                {
                    return t.clone();
                }
                // `Date`, `js::JsonError`: the `js` crate's types (JS objects).
                if bindings::is_js_type(&name) {
                    return Ty::Extern(name);
                }
                // `Server`, `RunOptions`: an interface of the NS bindings.
                if self.bindings.is_interface(&name) {
                    return Ty::Extern(name);
                }
                // `js::JsValue`: any JS value, unchecked (what `js::global(..)` returns).
                if name == "JsValue" {
                    return Ty::Unknown;
                }
                if name == "DefaultHasher" {
                    return Ty::Hasher;
                }
                if crates::is_rng_type(&name) && self.uses_crate("rand") {
                    return Ty::Rng;
                }
                // `Peekable<Chars<'_>>`, `std::slice::Iter<'_, T>`: the runtime's iterator.
                if let Some(t) = self.std_iter_type(&name, seg.as_ref()) {
                    return t;
                }
                // `Buf<SIZE>`: a `const` item as a const argument.
                if self.consts.contains_key(&name)
                    && p.path().is_some_and(|p| p.qualifier().is_none())
                {
                    return Ty::ConstVal(name);
                }
                // `net::Op`: a `mod` file's `pub type`.
                if let Some(t) = self.type_aliases.get(&name) {
                    return t.clone();
                }
                let ty = Ty::from_name(&name);
                if ty == Ty::Unknown {
                    self.error(
                        t.syntax(),
                        format!(
                            "type `{}` is not supported yet (planned for phase 1)",
                            t.syntax().text()
                        ),
                    );
                }
                ty
            }
            _ => {
                self.error(
                    t.syntax(),
                    format!(
                        "type `{}` is not supported yet (planned for phase 1)",
                        t.syntax().text()
                    ),
                );
                Ty::Unknown
            }
        }
    }

    /// `Some(is_mut)` when a declared type is a reference.
    fn ref_kind(t: &ast::Type) -> Option<bool> {
        match t {
            ast::Type::RefType(r) => Some(r.mut_token().is_some()),
            // A raw pointer to a number is a cell, `*const` too (writes elsewhere show through).
            ast::Type::PtrType(_) => Some(true),
            _ => None,
        }
    }

    // ------------------------------------------------------------ items & statements

    fn check_fn(&mut self, f: &ast::Fn) {
        let Some(name) = f.name() else { return };
        let Some(sig) = self.fns.get(name.text()) else {
            return;
        };
        let (params, ret, is_async) = (sig.params.clone(), sig.ret.clone(), sig.is_async);
        self.enter_generics(f, false);
        self.note_const_params(name.text(), f, Vec::new());
        self.cur_fn = Some(name.text().to_string());
        self.check_body(f, params, ret, is_async, None);
        self.cur_fn = None;
        self.leave_generics();
    }

    /// Check a function body against its signature. `self_local` is a method's `self`.
    fn check_body(
        &mut self,
        f: &ast::Fn,
        params: Vec<Ty>,
        ret: Ty,
        is_async: bool,
        self_local: Option<(ast::Name, Local)>,
    ) {
        self.ret_ty = ret.clone();
        self.in_async = is_async;
        self.scopes = vec![HashMap::new()];
        // Const generic parameters: values in the body.
        for (n, ty) in self.const_tys.clone() {
            let local = Local {
                ty,
                mutable: false,
                deferred: false,
                reference: None,
                decl: None,
            };
            self.scopes[0].insert(n, local);
        }
        if let Some((n, local)) = self_local {
            self.declare(&n, local);
        }
        if let Some(pl) = f.param_list() {
            for (p, ty) in pl.params().zip(params) {
                if let Some(ast::Pat::IdentPat(ip)) = p.pat()
                    && let Some(n) = ip.name()
                {
                    let reference = p.ty().as_ref().and_then(Self::ref_kind);
                    if reference == Some(true) && self.cellable(&ty) {
                        let r = self.range(n.syntax());
                        self.boxed.insert(r);
                    }
                    if p.ty().is_some_and(|t| is_ptr_type(&t)) {
                        let r = self.range(n.syntax());
                        self.ptrs.locals.insert(r);
                    }
                    if self.copy_array(p.ty().as_ref(), None, &ty) {
                        let r = self.range(n.syntax());
                        self.copy_arrays.insert(r);
                    }
                    let local = Local {
                        ty,
                        mutable: ip.mut_token().is_some(),
                        deferred: false,
                        reference,
                        decl: None,
                    };
                    self.declare(&n, local);
                } else if let Some(pat) = p.pat() {
                    // `(r, c): (usize, usize)`, `Point { x, y }: Point`, `&(a, b): &(i32, i32)`.
                    self.pat(&pat, &ty);
                }
            }
        }
        let Some(body) = f.body() else { return };
        let found = self.block(&body, Some(&ret));
        if !self.unify(&found, &ret) {
            let tail = body.stmt_list().and_then(|l| l.tail_expr());
            let node = tail
                .map(|t| t.syntax().clone())
                .unwrap_or_else(|| body.syntax().clone());
            let msg = format!(
                "mismatched types: expected `{}`, found `{}`",
                self.describe(&ret),
                self.describe(&found)
            );
            self.error(&node, msg);
        }
        self.resolve_intos();
        self.resolve_try_intos();
        self.resolve_collects();
        self.resolve_parses();
        self.resolve_defaults();
        self.resolve_tries();
        self.resolve_json();
        self.resolve_projections(true);
        self.resolve_bounds();
        self.resolve_qualified();
        let mut calls = std::mem::take(&mut self.generic_calls);
        for call in &mut calls {
            for (i, t) in call.3.iter().enumerate() {
                if matches!(self.shallow(t), Ty::Var(_)) {
                    self.unbound_targs.insert((call.0, i));
                }
            }
            call.3 = call.3.iter().map(|t| self.zonk(t)).collect();
        }
        self.generic_calls = calls;
        let mut sites = std::mem::take(&mut self.ctor_sites);
        for s in &mut sites {
            s.2 = s.2.iter().map(|t| self.zonk(t)).collect();
        }
        self.ctor_sites = sites;
        // (An error elsewhere can leave them unsettled.)
        let errors = self.diags.iter().any(|d| d.severity == "error");
        for (r, t) in std::mem::take(&mut self.let_types) {
            if errors {
                break;
            }
            if !self.has_unbound(&t) {
                continue;
            }
            let shown = self.describe(&t);
            let message = if shown == "_" {
                "type annotations needed".to_string()
            } else {
                format!("type annotations needed for `{shown}`")
            };
            self.diags.push(RawDiag {
                start: r.0,
                end: r.1,
                message,
                severity: "error",
            });
        }
        for (r, adt, targs) in std::mem::take(&mut self.const_lits) {
            if errors {
                break;
            }
            let params = self.adts.defs[&adt].type_params.clone();
            let cps = self.adts.const_params[&adt].clone();
            for (p, t) in params.iter().zip(&targs) {
                // (Only a variable nothing settled: an unchecked type leaves it unknown.)
                if let Some(cp) = cps.iter().find(|c| &c.name == p)
                    && cp.derive.is_none()
                    && matches!(self.shallow(t), Ty::Var(_))
                {
                    self.diags.push(RawDiag {
                        start: r.0,
                        end: r.1,
                        message: format!("type annotations needed: cannot infer the value of the const parameter `{p}` declared on the struct `{adt}`"),
                        severity: "error",
                    });
                }
            }
        }
    }

    /// Settle each qualified call's `Self`: a local struct or enum gets its `Trait$method`.
    fn resolve_qualified(&mut self) {
        for (r, self_ty, local_trait, node) in std::mem::take(&mut self.pending_qualified) {
            let t = self.zonk(&self_ty);
            let Some(q) = self.qualified_calls.get(&r).cloned() else {
                continue;
            };
            let adt = match &t {
                Ty::Adt(n, _) if self.adts.defs.get(n).is_some_and(|d| d.module.is_none()) => {
                    Some(n.clone())
                }
                _ => None,
            };
            let alias = adt.is_some() || (local_trait && matches!(t, Ty::Param(_) | Ty::Dyn(_)));
            let on_self = !q.receiver && t == Ty::Param("Self".into());
            let unconstrained = matches!(self.shallow(&self_ty), Ty::Var(_));
            if adt.is_none()
                && !q.receiver
                && !on_self
                && local_trait
                && (unconstrained || matches!(t, Ty::Param(_)))
            {
                if let Ty::Param(p) = &t {
                    self.error(
                        &node,
                        format!("`<{p} as {}>::{}(..)` on a type parameter isn't supported yet (planned for phase 4)", q.trait_name, q.method),
                    );
                    continue;
                }
                self.error(
                    &node,
                    format!(
                        "cannot call associated function on trait without specifying the corresponding `impl` type (write `<Type as {}>::{}(..)`)",
                        q.trait_name, q.method
                    ),
                );
            }
            self.qualified_calls.insert(
                r,
                QualifiedCall {
                    adt,
                    alias,
                    on_self,
                    ..q
                },
            );
        }
    }

    fn block(&mut self, b: &ast::BlockExpr, expected: Option<&Ty>) -> Ty {
        // `'a: { .. break 'a value; .. }`: the block's value is its tail or a `break` value.
        if let Some(ast::BlockModifier::Label(l)) = b.modifier() {
            let label = l.lifetime().map(|lt| lt.text().to_string());
            let value = self.fresh(VarKind::General);
            self.loops.push((label, Some(value.clone()), false, true));
            let t = self.block_body(b, expected);
            let (_, _, broke, _) = self.loops.pop().unwrap_or((None, None, false, false));
            if broke && self.shallow(&t) != Ty::Never && !self.unify(&t, &value) {
                let msg = format!(
                    "mismatched types: expected `{}`, found `{}`",
                    self.describe(&value),
                    self.describe(&t)
                );
                self.error(b.syntax(), msg);
            }
            let t = if broke { value } else { t };
            return self.record(b.syntax(), t);
        }
        if b.modifier().is_some() && !matches!(b.modifier(), Some(ast::BlockModifier::Unsafe(_))) {
            return self.record(b.syntax(), Ty::Unknown); // async blocks: codegen reports
        }
        self.block_body(b, expected)
    }

    fn block_body(&mut self, b: &ast::BlockExpr, expected: Option<&Ty>) -> Ty {
        let Some(list) = b.stmt_list() else {
            return self.record(b.syntax(), Ty::Unit);
        };
        self.scopes.push(HashMap::new());
        let mut diverged = false;
        for stmt in list.statements() {
            diverged |= self.stmt(&stmt);
        }
        let ty = match list.tail_expr() {
            Some(tail) => self.infer(&tail, expected),
            None if diverged => Ty::Never,
            None => Ty::Unit,
        };
        self.scopes.pop();
        self.record(b.syntax(), ty)
    }

    /// Returns true if the statement always diverges.
    fn stmt(&mut self, stmt: &ast::Stmt) -> bool {
        match stmt {
            ast::Stmt::LetStmt(l) => {
                self.let_stmt(l);
                false
            }
            ast::Stmt::ExprStmt(es) => match es.expr() {
                Some(e) => {
                    let t = self.infer(&e, None);
                    self.shallow(&t) == Ty::Never
                }
                None => false,
            },
            ast::Stmt::Item(_) => false,
        }
    }

    /// Whether a binding of type `ty`, declared as `ann` or made from `init`, is a fixed-size
    /// array of `Copy` values (`[i32; 3]`, `[[u8; 8]; 8]`, `["a", "b"]`), which copies like
    /// rustc's `[T; N]` where a `Vec` would move.
    pub(super) fn copy_array(
        &mut self,
        ann: Option<&ast::Type>,
        init: Option<&ast::Expr>,
        ty: &Ty,
    ) -> bool {
        let Ty::Vec(elem) = self.zonk(ty) else {
            return false;
        };
        match (ann, init.map(strip_parens)) {
            (Some(ast::Type::ArrayType(a)), _) => match a.ty() {
                Some(ast::Type::RefType(r)) => r.mut_token().is_none(),
                Some(inner @ ast::Type::ArrayType(_)) => self.copy_array(Some(&inner), None, &elem),
                _ => self.copy_elem(&elem),
            },
            (Some(_), _) => false,
            (None, Some(ast::Expr::ArrayExpr(arr))) => {
                let items: Vec<ast::Expr> = arr.exprs().collect();
                // `[x; n]`: the first expression is the value.
                let items = if arr.semicolon_token().is_some() {
                    items.into_iter().take(1).collect()
                } else {
                    items
                };
                if items.is_empty() {
                    return self.copy_elem(&elem);
                }
                items.iter().all(|e| match strip_parens(e) {
                    inner @ ast::Expr::ArrayExpr(_) => self.copy_array(None, Some(&inner), &elem),
                    ast::Expr::Literal(_) => true,
                    _ => self.copy_elem(&elem),
                })
            }
            _ => false,
        }
    }

    /// `Copy` for an array's elements (`String`s aren't; string literals are checked above).
    fn copy_elem(&self, t: &Ty) -> bool {
        let z = self.zonk(t);
        !matches!(z, Ty::Str | Ty::Unknown) && self.implements(&z, "Copy")
    }

    fn let_stmt(&mut self, l: &ast::LetStmt) {
        let declared = l.ty().map(|t| self.lower_type(&t));
        let declared_ref = l.ty().as_ref().and_then(Self::ref_kind);
        let init_ty = l.initializer().map(|init| {
            let t = self.infer(&init, declared.as_ref());
            if let Some(d) = &declared {
                self.expect(init.syntax(), &t, d);
            }
            t
        });
        if let Some(ast::Expr::RefExpr(r)) = l.initializer()
            && r.mut_token().is_some()
            && let Some(inner) = r.expr()
        {
            self.box_if_needed(&inner);
        }
        let pointer = l.ty().is_some_and(|t| is_ptr_type(&t))
            || l.initializer().is_some_and(|i| self.is_ptr(&i));
        let init_ref = match l.initializer().map(|i| ptr::unsafe_tail(&i)) {
            _ if pointer => Some(true),
            Some(ast::Expr::RefExpr(r)) => Some(r.mut_token().is_some()),
            // `let v = map.entry(k).or_insert(0);` is a `&mut V` into the map.
            Some(ast::Expr::MethodCallExpr(m))
                if m.name_ref().is_some_and(|n| {
                    matches!(
                        n.text().to_string().as_str(),
                        "or_insert"
                            | "or_default"
                            | "or_insert_with"
                            | "or_insert_with_key"
                            | "borrow_mut"
                    )
                }) =>
            {
                Some(true)
            }
            // `let s = v.iter_mut().min_by(..)?;`: a `&mut` element.
            Some(e) if mut_element(&e) => Some(true),
            _ => None,
        };
        let Some(pat) = l.pat() else { return };
        let simple_binding = match &pat {
            ast::Pat::IdentPat(ip) => ip.pat().is_none() && !is_none_pat(ip),
            _ => false,
        };
        if let (ast::Pat::IdentPat(ip), true, None) = (&pat, simple_binding, l.let_else()) {
            let Some(name) = ip.name() else { return };
            let deferred = l.initializer().is_none();
            let ty = match (declared, init_ty) {
                (Some(d), _) => d,
                (None, Some(t)) => t,
                (None, None) => self.fresh(VarKind::General),
            };
            let local = Local {
                ty,
                mutable: ip.mut_token().is_some(),
                deferred,
                reference: declared_ref.or(init_ref).or_else(|| {
                    // `let r = map.get_mut(k).unwrap();`: a `&mut` to the slot's value.
                    let r = self.range(name.syntax());
                    self.slot_bindings.contains(&r).then_some(true)
                }),
                decl: None,
            };
            let ty = local.ty.clone();
            if l.ty().is_none() {
                let r = self.range(name.syntax());
                self.let_types.push((r, ty.clone()));
            }
            self.declare(&name, local);
            if self.copy_array(l.ty().as_ref(), l.initializer().as_ref(), &ty) {
                let r = self.range(name.syntax());
                self.copy_arrays.insert(r);
            }
            // A raw pointer doesn't borrow.
            if self.ptr_let(l, &name, &ty) {
                return;
            }
            // `let r = &v;` / `let r = v.first();`: `r` borrows `v` while it's used.
            if let Some(init) = l.initializer()
                && let Some((of, mutable)) = self.borrowed_local(&init)
            {
                let holder = self.range(name.syntax());
                let from = self.range(l.syntax()).1;
                self.borrow_events.borrows.push((holder, of, mutable, from));
            }
            return;
        }
        // A pattern: `let Some(x) = opt else { ... };`, `let x @ 1..=5 = ...`, `let (a, b);`.
        let deferred = l.initializer().is_none();
        let ty = match declared.or(init_ty) {
            Some(t) => t,
            None if deferred => self.fresh_for_pat(&pat),
            None => Ty::Unknown,
        };
        // The `else` block runs without the pattern's bindings, so check it first.
        if let Some(le) = l.let_else() {
            if let Some(block) = le.block_expr() {
                let t = self.block(&block, None);
                if self.shallow(&t) != Ty::Never {
                    self.error(
                        block.syntax(),
                        "`else` clause of `let...else` does not diverge (it must `return`, `break`, `continue` or panic)",
                    );
                }
            }
        } else if !self.irrefutable_pat(&pat)
            && !self.or_pat_covers(&pat, &ty)
            && !self.array_pat_fits(l, &pat)
        {
            self.error(
                pat.syntax(),
                "refutable pattern in local binding (use `let ... else { ... }` or `if let`)",
            );
        }
        self.pat(&pat, &ty);
        // `let (left, right) = v.split_at_mut(mid);`: both are `&mut` views.
        if let Some(ast::Expr::MethodCallExpr(m)) = l.initializer().as_ref().map(strip_parens)
            && m.name_ref().is_some_and(|n| n.text() == "split_at_mut")
            && let Some(scope) = self.scopes.last_mut()
        {
            for ip in pat.syntax().descendants().filter_map(ast::IdentPat::cast) {
                if let Some(local) = ip.name().and_then(|n| scope.get_mut(n.text())) {
                    local.reference = Some(true);
                }
            }
        }
        // `let (a, b);`: each name is assigned later, once.
        if deferred && let Some(scope) = self.scopes.last_mut() {
            for ip in pat.syntax().descendants().filter_map(ast::IdentPat::cast) {
                if let Some(local) = ip.name().and_then(|n| scope.get_mut(n.text())) {
                    local.deferred = true;
                }
            }
        }
    }

    // ------------------------------------------------------------ patterns

    /// A fresh type shaped like `p`: tuple patterns get tuples of fresh variables.
    fn fresh_for_pat(&mut self, p: &ast::Pat) -> Ty {
        match p {
            ast::Pat::TuplePat(t) => {
                Ty::Tuple(t.fields().map(|f| self.fresh_for_pat(&f)).collect())
            }
            ast::Pat::ParenPat(pp) => match pp.pat() {
                Some(inner) => self.fresh_for_pat(&inner),
                None => self.fresh(VarKind::General),
            },
            _ => self.fresh(VarKind::General),
        }
    }

    /// Bindings from a pattern matched against `map.get_mut(..)`, `v.last_mut()`, `&mut x` and
    /// the like hold `&mut` references, so they can be mutated through without `mut`.
    fn mark_mut_ref_bindings(&mut self, scrutinee: Option<&ast::Expr>, pat: &ast::Pat) {
        // Matching on a place through `&mut` (`&mut x`, or `self` / a loop variable that is a
        // `&mut` reference): bindings alias the matched place, so `*r += 1` writes back.
        let through_place = match scrutinee.map(strip_parens) {
            Some(ast::Expr::RefExpr(r)) => r.mut_token().is_some(),
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(n.text().to_string().as_str()).cloned())
                .is_some_and(|l| l.reference == Some(true)),
            _ => false,
        };
        if through_place {
            for ip in pat
                .syntax()
                .descendants_with_tokens()
                .filter_map(|e| e.into_node())
                .filter_map(ast::IdentPat::cast)
            {
                if let Some(n) = ip.name() {
                    let r = self.range(n.syntax());
                    self.mut_bindings.insert(r);
                }
            }
        }
        let mut_source = through_place
            || match scrutinee.map(ptr::unsafe_tail) {
                Some(ast::Expr::RefExpr(r)) => r.mut_token().is_some(),
                Some(ast::Expr::MethodCallExpr(m)) => m.name_ref().is_some_and(|n| {
                    matches!(
                        n.text(),
                        "get_mut"
                            | "first_mut"
                            | "last_mut"
                            | "as_mut"
                            | "values_mut"
                            | "iter_mut"
                            | "split_at_mut"
                    )
                }),
                _ => false,
            };
        // `Some(x)` against a slot (`map.get_mut(..)` of numbers): `x` aliases the slot's value.
        if let Some(ast::Expr::MethodCallExpr(m)) = scrutinee.map(strip_parens)
            && self.slot_calls.contains(&self.range(m.syntax()))
            && let ast::Pat::TupleStructPat(tp) = pat
            && let Some(ast::Pat::IdentPat(ip)) = tp.fields().next()
            && let Some(n) = ip.name()
        {
            let r = self.range(n.syntax());
            self.slot_bindings.insert(r);
        }
        if mut_source && let Some(scope) = self.scopes.last_mut() {
            for local in scope.values_mut() {
                local.reference = Some(true);
            }
        }
    }

    fn let_condition(&mut self, le: &ast::LetExpr) {
        let st = le
            .expr()
            .map(|e| self.infer(&e, None))
            .unwrap_or(Ty::Unknown);
        if let Some(p) = le.pat() {
            self.pat(&p, &st);
            self.mark_mut_ref_bindings(le.expr().as_ref(), &p);
        }
        self.record(le.syntax(), Ty::Bool);
    }

    /// Check a pattern against the type of the value it matches, declaring its bindings in the
    /// current scope. Records the matched type on the pattern node for codegen.
    fn pat(&mut self, p: &ast::Pat, expected: &Ty) {
        self.record(p.syntax(), expected.clone());
        match p {
            ast::Pat::WildcardPat(_) => {}
            ast::Pat::ParenPat(pp) => {
                if let Some(inner) = pp.pat() {
                    self.pat(&inner, expected);
                }
            }
            ast::Pat::RefPat(rp) => {
                if let Some(inner) = rp.pat() {
                    self.pat(&inner, expected);
                }
            }
            ast::Pat::IdentPat(ip) if is_none_pat(ip) => self.expect_option(p, expected),
            ast::Pat::IdentPat(ip) if self.unit_struct_pat(ip).is_some() => {
                self.adt_pat(p, expected);
            }
            ast::Pat::IdentPat(ip) if self.const_pat(ip).is_some() => {
                // A `const` in a pattern compares against its value.
                let t = self.const_pat(ip).unwrap();
                if !self.unify(expected, &t) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `{}`",
                        self.describe(expected),
                        self.describe(&t)
                    );
                    self.error(p.syntax(), msg);
                }
            }
            ast::Pat::IdentPat(ip) => {
                if let Some(sub) = ip.pat() {
                    self.pat(&sub, expected);
                }
                if let Some(name) = ip.name() {
                    let reference = ip.ref_token().map(|_| ip.mut_token().is_some());
                    let local = Local {
                        ty: expected.clone(),
                        mutable: ip.mut_token().is_some(),
                        deferred: false,
                        reference,
                        decl: None,
                    };
                    self.declare(&name, local);
                }
            }
            ast::Pat::PathPat(pp) if path_is(pp.path(), "None") => self.expect_option(p, expected),
            ast::Pat::PathPat(pp) if ordering_variant(pp.path()).is_some() => {
                if !self.unify(expected, &Ty::Ordering) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `Ordering`",
                        self.describe(expected)
                    );
                    self.error(p.syntax(), msg);
                }
            }
            ast::Pat::TupleStructPat(tp) if result_variant(tp.path()).is_some() => {
                self.result_pat(p, tp, result_variant(tp.path()).unwrap(), expected);
            }
            ast::Pat::TupleStructPat(tp) if path_is(tp.path(), "Some") => {
                let inner = self.fresh(VarKind::General);
                if !self.unify(expected, &Ty::Option(Box::new(inner.clone()))) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `Option<_>`",
                        self.describe(expected)
                    );
                    self.error(p.syntax(), msg);
                }
                let fields: Vec<ast::Pat> = tp.fields().collect();
                if fields.len() != 1 {
                    self.error(
                        p.syntax(),
                        format!("`Some` has 1 field, but this pattern has {}", fields.len()),
                    );
                }
                if let Some(f) = fields.first() {
                    self.pat(f, &inner);
                }
            }
            ast::Pat::LiteralPat(lp) => {
                let t = match lp.literal() {
                    Some(l) => self.literal(&l),
                    None => Ty::Unknown,
                };
                if !self.unify(expected, &t) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `{}`",
                        self.describe(expected),
                        self.describe(&t)
                    );
                    self.error(p.syntax(), msg);
                }
            }
            ast::Pat::RangePat(rp) => {
                for bound in [rp.start(), rp.end()].into_iter().flatten() {
                    self.pat(&bound, expected);
                }
                let ok = matches!(self.shallow(expected), Ty::Int(_) | Ty::Char | Ty::Unknown)
                    || self.var_kind(expected) == Some(VarKind::Int);
                if !ok {
                    let msg = format!(
                        "range patterns need an integer or `char`, found `{}`",
                        self.describe(expected)
                    );
                    self.error(p.syntax(), msg);
                }
            }
            // `(Some(x), _) | (_, Some(x))`: every alternative binds the same names, with the
            // same types; the first alternative's bindings are the variables.
            ast::Pat::OrPat(op) => {
                let mut first: Option<Vec<(String, Local)>> = None;
                for alt in op.pats() {
                    self.pat(&alt, expected);
                    let bound: Vec<(String, Local)> = alt
                        .syntax()
                        .descendants()
                        .filter_map(ast::IdentPat::cast)
                        .filter_map(|ip| {
                            let n = ip.name()?;
                            // (`A | B` naming variants of another crate: not bindings.)
                            if n.text().starts_with(|c: char| c.is_ascii_uppercase()) {
                                return None;
                            }
                            let r = self.range(n.syntax());
                            let local = self.lookup(n.text().to_string().as_str())?.clone();
                            (local.decl == Some(r)).then(|| (n.text().to_string(), local))
                        })
                        .collect();
                    match &first {
                        None => first = Some(bound),
                        Some(f) => {
                            for (n, l) in f {
                                match bound.iter().find(|(m, _)| m == n) {
                                    Some((_, other)) => {
                                        if !self.unify(&l.ty, &other.ty) {
                                            let msg = format!(
                                                "mismatched types: `{n}` is `{}` in one alternative and `{}` in another",
                                                self.describe(&l.ty),
                                                self.describe(&other.ty)
                                            );
                                            self.error(alt.syntax(), msg);
                                        }
                                    }
                                    None => self.error(
                                        alt.syntax(),
                                        format!("variable `{n}` is not bound in all patterns"),
                                    ),
                                }
                            }
                            for (m, _) in &bound {
                                if !f.iter().any(|(n, _)| n == m) {
                                    self.error(
                                        op.pats().next().unwrap().syntax(),
                                        format!("variable `{m}` is not bound in all patterns"),
                                    );
                                }
                            }
                        }
                    }
                }
                for (n, l) in first.unwrap_or_default() {
                    self.scopes.last_mut().unwrap().insert(n, l);
                }
            }
            ast::Pat::TuplePat(tp) => self.tuple_pat(p, tp, expected),
            ast::Pat::SlicePat(sp) => self.slice_pat(p, sp, expected),
            // Structs and enum variants.
            _ => {
                self.adt_pat(p, expected);
            }
        }
    }

    fn tuple_pat(&mut self, p: &ast::Pat, tp: &ast::TuplePat, expected: &Ty) {
        let fields: Vec<ast::Pat> = tp.fields().collect();
        let rest = fields
            .iter()
            .position(|f| matches!(f, ast::Pat::RestPat(_)));
        // `()` (as in `Some(())`): matches the unit value.
        if fields.is_empty() && self.unify(expected, &Ty::Unit) {
            self.record(p.syntax(), Ty::Unit);
            return;
        }
        let elems = match (self.shallow(expected), rest) {
            (Ty::Tuple(es), _) => es,
            (Ty::Unknown, _) => vec![Ty::Unknown; fields.len()],
            (_, None) => {
                let es: Vec<Ty> = fields
                    .iter()
                    .map(|_| self.fresh(VarKind::General))
                    .collect();
                if !self.unify(expected, &Ty::Tuple(es.clone())) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found a tuple",
                        self.describe(expected)
                    );
                    self.error(p.syntax(), msg);
                }
                es
            }
            (_, Some(_)) => {
                self.error(
                    p.syntax(),
                    "type annotations needed for a tuple pattern with `..`",
                );
                return;
            }
        };
        let n = elems.len();
        let named = fields.len() - usize::from(rest.is_some());
        if (rest.is_none() && named != n) || named > n {
            let msg = format!(
                "mismatched types: expected a tuple with {n} element{}, found one with {named}",
                if n == 1 { "" } else { "s" }
            );
            self.error(p.syntax(), msg);
            return;
        }
        for (i, f) in fields.iter().enumerate() {
            let index = match rest {
                Some(r) if i == r => continue,
                Some(r) if i > r => n - (fields.len() - i),
                _ => i,
            };
            self.pat(f, &elems[index]);
        }
    }

    /// `[a, b]`, `[first, .., last]`, `[head, rest @ ..]` against a `Vec` / slice / array.
    fn slice_pat(&mut self, p: &ast::Pat, sp: &ast::SlicePat, expected: &Ty) {
        let elem = match self.shallow(expected) {
            Ty::Vec(e) => *e,
            Ty::Unknown => Ty::Unknown,
            _ => {
                let e = self.fresh(VarKind::General);
                if !self.unify(expected, &Ty::Vec(Box::new(e.clone()))) {
                    let msg = format!(
                        "expected an array or slice, found `{}`",
                        self.describe(expected)
                    );
                    self.error(p.syntax(), msg);
                }
                e
            }
        };
        let mut rests = 0;
        for f in sp.pats() {
            match &f {
                ast::Pat::RestPat(_) => rests += 1,
                // `rest @ ..` binds the middle as a slice.
                ast::Pat::IdentPat(ip) if matches!(ip.pat(), Some(ast::Pat::RestPat(_))) => {
                    rests += 1;
                    if let Some(name) = ip.name() {
                        let local = Local {
                            ty: Ty::Vec(Box::new(elem.clone())),
                            mutable: ip.mut_token().is_some(),
                            deferred: false,
                            reference: Some(false),
                            decl: None,
                        };
                        self.declare(&name, local);
                    }
                }
                other => self.pat(other, &elem),
            }
        }
        if rests > 1 {
            self.error(p.syntax(), "`..` can only be used once per slice pattern");
        }
    }

    fn expect_option(&mut self, p: &ast::Pat, expected: &Ty) {
        let fresh = self.fresh(VarKind::General);
        if !self.unify(expected, &Ty::Option(Box::new(fresh))) {
            let msg = format!(
                "mismatched types: expected `{}`, found `Option<_>`",
                self.describe(expected)
            );
            self.error(p.syntax(), msg);
        }
    }

    fn match_expr(&mut self, m: &ast::MatchExpr, expected: Option<&Ty>) -> Ty {
        let st = m
            .expr()
            .map(|e| self.infer(&e, None))
            .unwrap_or(Ty::Unknown);
        let arms: Vec<ast::MatchArm> = m
            .match_arm_list()
            .map(|l| l.arms().collect())
            .unwrap_or_default();
        let mut result: Option<Ty> = None;
        for arm in &arms {
            self.scopes.push(HashMap::new());
            if let Some(p) = arm.pat() {
                self.pat(&p, &st);
                self.mark_mut_ref_bindings(m.expr().as_ref(), &p);
            }
            if let Some(g) = arm.guard().and_then(|g| g.condition()) {
                self.check(&g, &Ty::Bool);
            }
            if let Some(body) = arm.expr() {
                let hint = result.clone().or_else(|| expected.cloned());
                let t = self.infer(&body, hint.as_ref());
                if self.shallow(&t) != Ty::Never {
                    match result.clone() {
                        None => result = Some(t),
                        Some(r) => {
                            if !self.unify(&r, &t) {
                                let msg = format!(
                                    "`match` arms have incompatible types: expected `{}`, found `{}`",
                                    self.describe(&r),
                                    self.describe(&t)
                                );
                                self.error(body.syntax(), msg);
                            }
                        }
                    }
                }
            }
            self.scopes.pop();
        }
        self.check_exhaustive(m, &arms, &st);
        result.unwrap_or(Ty::Never)
    }

    /// Exhaustiveness via the classic pattern-matrix "usefulness" algorithm over the constructors
    /// CarciniScript knows: `bool`, `Option` and tuples. Integers, chars and strings have
    /// effectively infinite domains, so covering them takes a catch-all arm.
    fn check_exhaustive(&mut self, m: &ast::MatchExpr, arms: &[ast::MatchArm], st: &Ty) {
        let mut rows = Vec::new();
        for arm in arms.iter().filter(|a| a.guard().is_none()) {
            if let Some(p) = arm.pat() {
                rows.extend(self.abstract_pat(&p).into_iter().map(|ap| vec![ap]));
            }
        }
        self.missing_steps.set(0);
        if let Some(witness) = self.missing(&rows, std::slice::from_ref(st)) {
            let node = m
                .expr()
                .map(|e| e.syntax().clone())
                .unwrap_or_else(|| m.syntax().clone());
            self.error(
                &node,
                format!("non-exhaustive patterns: `{}` not covered", witness[0]),
            );
        }
    }

    /// Whether a pattern with `|` alternatives covers every value (`let (Ok(n) | Err(n)) = r;`).
    fn or_pat_covers(&self, p: &ast::Pat, ty: &Ty) -> bool {
        if !p
            .syntax()
            .descendants()
            .any(|n| ast::OrPat::can_cast(n.kind()))
        {
            return false;
        }
        let rows: Vec<Vec<Ap>> = self
            .abstract_pat(p)
            .into_iter()
            .map(|ap| vec![ap])
            .collect();
        self.missing_steps.set(0);
        self.missing(&rows, &[self.zonk(ty)]).is_none()
    }

    /// A pattern as constructors, one entry per `|` alternative.
    fn abstract_pat(&self, p: &ast::Pat) -> Vec<Ap> {
        match strip_pat(p) {
            ast::Pat::WildcardPat(_) => vec![Ap::Wild],
            ast::Pat::IdentPat(ip) if is_none_pat(&ip) => vec![Ap::None],
            ast::Pat::IdentPat(ip) if self.unit_struct_pat(&ip).is_some() => self
                .adt_abstract(&ast::Pat::IdentPat(ip))
                .unwrap_or(vec![Ap::Opaque]),
            ast::Pat::IdentPat(ip) if self.const_pat(&ip).is_some() => vec![Ap::Other],
            ast::Pat::IdentPat(ip) => ip
                .pat()
                .map(|sub| self.abstract_pat(&sub))
                .unwrap_or_else(|| vec![Ap::Wild]),
            ast::Pat::PathPat(pp) if path_is(pp.path(), "None") => vec![Ap::None],
            ast::Pat::PathPat(pp) if ordering_variant(pp.path()).is_some() => {
                vec![Ap::Ord(ordering_variant(pp.path()).unwrap())]
            }
            ast::Pat::TupleStructPat(tp) if result_variant(tp.path()).is_some() => {
                let v = result_variant(tp.path()).unwrap();
                match tp.fields().next() {
                    Some(f) => self
                        .abstract_pat(&f)
                        .into_iter()
                        .map(|a| Ap::Ctor(v, vec![a]))
                        .collect(),
                    None => vec![Ap::Other],
                }
            }
            ast::Pat::TupleStructPat(tp) if path_is(tp.path(), "Some") => {
                match tp.fields().next() {
                    Some(f) => self
                        .abstract_pat(&f)
                        .into_iter()
                        .map(|a| Ap::Some(Box::new(a)))
                        .collect(),
                    None => vec![Ap::Other],
                }
            }
            ast::Pat::LiteralPat(lp) => match lp.literal().map(|l| l.kind()) {
                Some(LiteralKind::Bool(b)) => vec![Ap::Bool(b)],
                Some(LiteralKind::IntNumber(_)) => match int_lit_value(&lp) {
                    Some(v) => vec![Ap::Int(v, v)],
                    None => vec![Ap::Other],
                },
                Some(LiteralKind::Char(_)) => match char_lit_value(&lp) {
                    Some(v) => vec![Ap::Int(v, v)],
                    None => vec![Ap::Other],
                },
                _ => vec![Ap::Other],
            },
            // `1..=5`, `..10`, `200..` (bounds left out are the type's own, filled in later).
            ast::Pat::RangePat(rp) => {
                let bound = |p: Option<ast::Pat>| match p {
                    Some(ast::Pat::LiteralPat(lp)) => {
                        int_lit_value(&lp).or_else(|| char_lit_value(&lp)).map(Some)
                    }
                    // `u8::MAX`, `i32::MIN`
                    Some(ast::Pat::PathPat(pp)) => {
                        let path = pp.path()?;
                        let ty = path.qualifier()?.segment()?.name_ref()?.text().to_string();
                        let (min, max) = match IntKind::from_name(&ty) {
                            Some(k) => int_domain(k),
                            None if ty == "char" => (0, 0x10FFFF),
                            None => return None,
                        };
                        match path.segment()?.name_ref()?.text() {
                            "MIN" => Some(Some(min)),
                            "MAX" => Some(Some(max)),
                            _ => None,
                        }
                    }
                    None => Some(None),
                    _ => None,
                };
                let inclusive = rp.op_token().is_some_and(|t| t.text() == "..=");
                match (bound(rp.start()), bound(rp.end())) {
                    (Some(lo), Some(hi)) => {
                        let lo = lo.unwrap_or(i128::MIN);
                        let hi = match hi {
                            Some(h) if inclusive => h,
                            Some(h) => h - 1,
                            None => i128::MAX,
                        };
                        if lo <= hi {
                            vec![Ap::Int(lo, hi)]
                        } else {
                            vec![]
                        }
                    }
                    _ => vec![Ap::Other],
                }
            }
            ast::Pat::OrPat(op) => op.pats().flat_map(|alt| self.abstract_pat(&alt)).collect(),
            ast::Pat::TuplePat(tp) => {
                let fields: Vec<ast::Pat> = tp.fields().collect();
                let arity = match self.raw.get(&self.range(p.syntax())).map(|t| self.zonk(t)) {
                    Some(Ty::Tuple(es)) => es.len(),
                    _ => fields.len(),
                };
                // Expand `..` into wildcards, then take the product of each element's alternatives.
                let mut cols: Vec<Vec<Ap>> = Vec::new();
                for f in &fields {
                    if matches!(f, ast::Pat::RestPat(_)) {
                        let missing = arity.saturating_sub(fields.len() - 1);
                        cols.extend(std::iter::repeat_n(vec![Ap::Wild], missing));
                    } else {
                        cols.push(self.abstract_pat(f));
                    }
                }
                let mut product: Vec<Vec<Ap>> = vec![vec![]];
                for col in cols {
                    product = product
                        .into_iter()
                        .flat_map(|prefix| {
                            col.iter()
                                .map(move |c| [prefix.clone(), vec![c.clone()]].concat())
                        })
                        .collect();
                    if product.len() > 512 {
                        return vec![Ap::Wild]; // too many combinations to check; assume covered
                    }
                }
                product.into_iter().map(Ap::Tuple).collect()
            }
            ast::Pat::SlicePat(sp) => {
                // The product of each element's alternatives, like a tuple, split around `..`.
                let mut prefix_cols: Vec<Vec<Ap>> = Vec::new();
                let mut suffix_cols: Vec<Vec<Ap>> = Vec::new();
                let mut rest = false;
                for f in sp.pats() {
                    let is_rest = matches!(&f, ast::Pat::RestPat(_))
                        || matches!(&f, ast::Pat::IdentPat(ip) if matches!(ip.pat(), Some(ast::Pat::RestPat(_))));
                    if is_rest {
                        rest = true;
                    } else if rest {
                        suffix_cols.push(self.abstract_pat(&f));
                    } else {
                        prefix_cols.push(self.abstract_pat(&f));
                    }
                }
                let split = prefix_cols.len();
                let mut product: Vec<Vec<Ap>> = vec![vec![]];
                for col in prefix_cols.into_iter().chain(suffix_cols) {
                    product = product
                        .into_iter()
                        .flat_map(|prefix| {
                            col.iter()
                                .map(move |c| [prefix.clone(), vec![c.clone()]].concat())
                        })
                        .collect();
                    if product.len() > 512 {
                        return vec![Ap::Other];
                    }
                }
                product
                    .into_iter()
                    .map(|fields| {
                        let (prefix, suffix) = fields.split_at(split);
                        Ap::Slice(prefix.to_vec(), suffix.to_vec(), rest)
                    })
                    .collect()
            }
            other => self.adt_abstract(&other).unwrap_or(vec![Ap::Opaque]),
        }
    }

    /// A value not matched by any row (one rendered pattern per column), or None if every value
    /// is matched.
    fn missing(&self, rows: &[Vec<Ap>], tys: &[Ty]) -> Option<Vec<String>> {
        // The search is exponential in the worst case; past a budget, assume the match is covered.
        let steps = self.missing_steps.get() + 1;
        self.missing_steps.set(steps);
        if steps > 20_000 {
            return None;
        }
        let Some(ty) = tys.first() else {
            return if rows.is_empty() { Some(vec![]) } else { None };
        };
        let rest = &tys[1..];
        // The constructor family of this column: from its type, or from its patterns when the type
        // is unknown.
        let head_kind = rows
            .iter()
            .map(|r| &r[0])
            .find(|a| !matches!(a, Ap::Wild | Ap::Other));
        // An unsolved type is as good as unknown.
        let zt = match self.zonk(ty) {
            Ty::Var(_) => Ty::Unknown,
            t => t,
        };
        let family = match (zt, head_kind) {
            (Ty::Unit, None | Some(Ap::Tuple(_))) => Family::Tuple(vec![]),
            (Ty::Bool, _) | (Ty::Unknown, Some(Ap::Bool(_))) => Family::Bool,
            (Ty::Int(k), Some(Ap::Int(..))) => Family::Int(k),
            (Ty::Char, Some(Ap::Int(..))) => Family::Char,
            (Ty::Ordering, _) | (Ty::Unknown, Some(Ap::Ord(_))) => Family::Ordering,
            (Ty::Option(inner), _) => Family::Option(*inner),
            (Ty::Unknown, Some(Ap::None | Ap::Some(_))) => Family::Option(Ty::Unknown),
            (Ty::Tuple(es), _) => Family::Tuple(es),
            (Ty::Unknown, Some(Ap::Tuple(ps))) => Family::Tuple(vec![Ty::Unknown; ps.len()]),
            (Ty::Adt(n, targs), _) if self.adts.defs.contains_key(&n) => Family::Adt(n, targs),
            (Ty::Result(t, e), _) => Family::Result(*t, *e),
            (Ty::Vec(e), Some(Ap::Slice(..))) => Family::Slice(*e),
            // Values without end: a catch-all must cover what the patterns leave.
            (Ty::Int(_) | Ty::Float { .. } | Ty::Char | Ty::Str, _) => Family::Infinite,
            (Ty::Unknown, Some(Ap::Int(..))) => Family::Infinite,
            // A type this checker doesn't know the values of (another crate's enum, an unsolved
            // type): assume the patterns cover it, as rustc accepted them.
            _ => return None,
        };
        // Patterns of another kind than the type (it was mistyped, say a field declared twice
        // under `#[cfg]`s), or of a type unknown here: nothing to say.
        let fits = |a: &Ap| match (&family, a) {
            (_, Ap::Wild) => true,
            (_, Ap::Opaque) => false,
            (Family::Bool, Ap::Bool(_)) | (Family::Ordering, Ap::Ord(_)) => true,
            (Family::Option(_), Ap::None | Ap::Some(_)) => true,
            (Family::Tuple(_), Ap::Tuple(_)) | (Family::Slice(_), Ap::Slice(..)) => true,
            (Family::Adt(..) | Family::Result(..), Ap::Ctor(..)) => true,
            (Family::Int(_) | Family::Char | Family::Infinite, Ap::Int(..) | Ap::Other) => true,
            _ => false,
        };
        if !rows.iter().all(|r| fits(&r[0])) {
            return None;
        }
        let specialize = |ctor: &Ap, arity: usize| -> Vec<Vec<Ap>> {
            rows.iter()
                .filter_map(|r| {
                    let fields = match (&r[0], ctor) {
                        (Ap::Wild, _) => vec![Ap::Wild; arity],
                        (Ap::Bool(a), Ap::Bool(b)) if a == b => vec![],
                        (Ap::Ord(a), Ap::Ord(b)) if a == b => vec![],
                        (Ap::None, Ap::None) => vec![],
                        (Ap::Some(x), Ap::Some(_)) => vec![(**x).clone()],
                        (Ap::Tuple(xs), Ap::Tuple(_)) if xs.len() == arity => xs.clone(),
                        (Ap::Ctor(a, xs), Ap::Ctor(b, _)) if a == b && xs.len() == arity => {
                            xs.clone()
                        }
                        // A slice pattern against length `arity`.
                        (Ap::Slice(pre, suf, rest), Ap::Slice(..)) => {
                            let fixed = pre.len() + suf.len();
                            if (*rest && fixed <= arity) || (!*rest && fixed == arity) {
                                let mid = arity - fixed;
                                [pre.clone(), vec![Ap::Wild; mid], suf.clone()].concat()
                            } else {
                                return None;
                            }
                        }
                        _ => return None,
                    };
                    Some([fields, r[1..].to_vec()].concat())
                })
                .collect()
        };
        match family {
            Family::Tuple(es) => {
                let n = es.len();
                let w = self.missing(
                    &specialize(&Ap::Tuple(vec![]), n),
                    &[es, rest.to_vec()].concat(),
                )?;
                let head = crate::ty::describe_tuple(w[..n].to_vec());
                Some([vec![head], w[n..].to_vec()].concat())
            }
            Family::Adt(n, targs) => {
                let variants: Vec<crate::adt::Variant> = (0..self.adts.defs[&n].variants.len())
                    .map(|v| self.variant_inst(&n, v, &targs))
                    .collect();
                // No constructor in this column: use the default matrix rather than expanding
                // (which wouldn't terminate for recursive types).
                if rows.iter().all(|r| matches!(r[0], Ap::Wild)) {
                    let defaults: Vec<Vec<Ap>> = rows.iter().map(|r| r[1..].to_vec()).collect();
                    let w = self.missing(&defaults, rest)?;
                    let head = match variants.first() {
                        Some(v) if rows.is_empty() => {
                            self.render_ctor(&n, 0, &vec!["_".to_string(); v.fields.len()])
                        }
                        _ => "_".to_string(),
                    };
                    return Some([vec![head], w].concat());
                }
                variants.iter().enumerate().find_map(|(v, variant)| {
                    if self.adts.cfg_variants.contains(&(n.clone(), v)) {
                        return None;
                    }
                    let k = variant.fields.len();
                    let tys: Vec<Ty> = variant.fields.iter().map(|f| f.ty.clone()).collect();
                    let w = self.missing(
                        &specialize(&Ap::Ctor(v, vec![]), k),
                        &[tys, rest.to_vec()].concat(),
                    )?;
                    let head = self.render_ctor(&n, v, &w[..k]);
                    Some([vec![head], w[k..].to_vec()].concat())
                })
            }
            Family::Slice(elem) => {
                // Lengths up to the longest fixed part decide everything; longer behave alike.
                let longest = rows
                    .iter()
                    .map(|r| match &r[0] {
                        Ap::Slice(pre, suf, _) => pre.len() + suf.len(),
                        _ => 0,
                    })
                    .max()
                    .unwrap_or(0);
                (0..=longest + 1).find_map(|len| {
                    let w = self.missing(
                        &specialize(&Ap::Slice(vec![], vec![], false), len),
                        &[vec![elem.clone(); len], rest.to_vec()].concat(),
                    )?;
                    let items = w[..len].join(", ");
                    let head = if len > longest {
                        if len == 0 {
                            "[..]".to_string()
                        } else {
                            format!("[{items}, ..]")
                        }
                    } else {
                        format!("[{items}]")
                    };
                    Some([vec![head], w[len..].to_vec()].concat())
                })
            }
            Family::Result(t, e) => {
                [(0usize, t, "Ok"), (1, e, "Err")]
                    .into_iter()
                    .find_map(|(v, ty, name)| {
                        let w = self.missing(
                            &specialize(&Ap::Ctor(v, vec![]), 1),
                            &[vec![ty], rest.to_vec()].concat(),
                        )?;
                        Some([vec![format!("{name}({})", w[0])], w[1..].to_vec()].concat())
                    })
            }
            Family::Ordering => [-1i8, 0, 1].into_iter().find_map(|o| {
                let w = self.missing(&specialize(&Ap::Ord(o), 0), rest)?;
                let name =
                    ["Ordering::Less", "Ordering::Equal", "Ordering::Greater"][(o + 1) as usize];
                Some([vec![name.to_string()], w].concat())
            }),
            Family::Bool => [true, false].into_iter().find_map(|b| {
                let w = self.missing(&specialize(&Ap::Bool(b), 0), rest)?;
                Some([vec![b.to_string()], w].concat())
            }),
            Family::Option(inner) => {
                if let Some(w) = self.missing(&specialize(&Ap::None, 0), rest) {
                    return Some([vec!["None".to_string()], w].concat());
                }
                let w = self.missing(
                    &specialize(&Ap::Some(Box::new(Ap::Wild)), 1),
                    &[vec![inner], rest.to_vec()].concat(),
                )?;
                Some([vec![format!("Some({})", w[0])], w[1..].to_vec()].concat())
            }
            Family::Int(_) | Family::Char => {
                // Split the type's values where some pattern starts or ends; each piece is covered
                // by a row as a whole or not at all.
                let (min, max) = match family {
                    Family::Int(k) => int_domain(k),
                    _ => (0, 0x10FFFF),
                };
                let is_char = matches!(family, Family::Char);
                let mut cuts = vec![min];
                if is_char {
                    cuts.extend([0xD800, 0xE000]);
                }
                for r in rows {
                    if let Ap::Int(a, b) = r[0] {
                        cuts.push(a.clamp(min, max));
                        if b < max {
                            cuts.push((b + 1).clamp(min, max));
                        }
                    }
                }
                cuts.sort_unstable();
                cuts.dedup();
                for (i, &lo) in cuts.iter().enumerate() {
                    let hi = cuts.get(i + 1).map(|&c| c - 1).unwrap_or(max);
                    if is_char && (0xD800..0xE000).contains(&lo) {
                        continue;
                    }
                    let sub: Vec<Vec<Ap>> = rows
                        .iter()
                        .filter(|r| match r[0] {
                            Ap::Wild => true,
                            Ap::Int(a, b) => a <= lo && hi <= b,
                            _ => false,
                        })
                        .map(|r| r[1..].to_vec())
                        .collect();
                    if let Some(w) = self.missing(&sub, rest) {
                        let head = match family {
                            Family::Int(k) => describe_int_range(k, lo, hi),
                            _ => describe_char_range(lo, hi),
                        };
                        return Some([vec![head], w].concat());
                    }
                }
                None
            }
            Family::Infinite => {
                let defaults: Vec<Vec<Ap>> = rows
                    .iter()
                    .filter(|r| matches!(r[0], Ap::Wild))
                    .map(|r| r[1..].to_vec())
                    .collect();
                let w = self.missing(&defaults, rest)?;
                Some([vec!["_".to_string()], w].concat())
            }
        }
    }

    // ------------------------------------------------------------ expressions

    fn infer(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> Ty {
        let ty = self.infer_inner(e, expected);
        self.record(e.syntax(), ty)
    }

    /// Infer `e` and require it to have type `expected`.
    fn check(&mut self, e: &ast::Expr, expected: &Ty) -> Ty {
        let t = self.infer(e, Some(expected));
        // `&player` where a `&Person` is expected: an NS interface it extends.
        if self.iface_coerces(&t, expected) {
            return expected.clone();
        }
        // `&stack` where `&Vec<i32>` is expected, with `impl Deref<Target = Vec<i32>> for Stack`.
        if let Ty::Adt(n, _) = self.shallow(&t)
            && !matches!(
                self.shallow(expected),
                Ty::Adt(..) | Ty::Var(_) | Ty::Unknown
            )
            && let Some(target) = self.adts.deref_targets.get(&n).cloned()
            && self.could_be(&target, expected)
        {
            self.unify(&target, expected);
            self.auto_deref(e, &n, &target);
            return target;
        }
        self.expect(e.syntax(), &t, expected);
        t
    }

    /// Mark `e` (of type `adt`) as auto-dereferenced to `target`.
    fn auto_deref(&mut self, e: &ast::Expr, adt: &str, target: &Ty) {
        let r = self.range(e.syntax());
        let mutable = self.adts.deref_mut.contains(adt);
        self.derefs.insert(r, (mutable, target.clone()));
    }

    /// `x`'s type through `impl Deref`, when `x` is such a struct or enum.
    fn deref_target(&self, t: &Ty) -> Option<(String, Ty)> {
        match self.shallow(t) {
            // (A generic struct's target in its type arguments: `LazyLock<Vec<u32>>` derefs to
            // `Vec<u32>`.)
            Ty::Adt(n, targs) => self.adts.deref_targets.get(&n).map(|t| {
                let map = self.adt_subst(&n, &targs);
                (n.clone(), generic::subst(t, &map))
            }),
            _ => None,
        }
    }

    fn infer_inner(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> Ty {
        match e {
            ast::Expr::Literal(l) => self.literal(l),
            ast::Expr::PathExpr(p) => self.path(p),
            ast::Expr::ParenExpr(p) => p
                .expr()
                .map(|i| self.infer(&i, expected))
                .unwrap_or(Ty::Unit),
            ast::Expr::RefExpr(r) => {
                let Some(inner) = r.expr() else {
                    return Ty::Unknown;
                };
                if r.mut_token().is_some() {
                    self.check_mutable(&inner, Mutation::Borrow);
                }
                let t = self.infer(&inner, expected);
                // `&mut v[a..b]`: a view of that part of `v`.
                if r.mut_token().is_some()
                    && let ast::Expr::IndexExpr(ix) = strip_parens(&inner)
                    && matches!(ix.index(), Some(ast::Expr::RangeExpr(_)))
                {
                    let base = ix
                        .base()
                        .and_then(|b| self.raw.get(&self.range(b.syntax())).cloned());
                    match base.map(|b| self.zonk(&b)) {
                        Some(Ty::Str) => self.error(
                            r.syntax(),
                            "mutable string slices (`&mut s[a..b]`) are not supported (strings are values in JS); work on `s` itself",
                        ),
                        _ => {
                            let rr = self.range(r.syntax());
                            self.slice_views.insert(rr);
                        }
                    }
                }
                // `let r = &mut self.root;` with a number, string, bool or `Option` in that slot:
                // a JS variable can't point into the struct, so `*r = ..` would only change `r`.
                if r.mut_token().is_some()
                    && matches!(
                        strip_parens(&inner),
                        ast::Expr::FieldExpr(_) | ast::Expr::IndexExpr(_)
                    )
                    && let Some(p) = r.syntax().parent()
                    && (ast::LetStmt::can_cast(p.kind())
                        || ast::BinExpr::cast(p.clone()).is_some_and(|b| {
                            matches!(b.op_kind(), Some(BinaryOp::Assignment { op: None }))
                                && b.rhs().is_some_and(|x| x.syntax() == r.syntax())
                        }))
                {
                    let z = self.zonk(&t);
                    if z.is_js_primitive() || matches!(z, Ty::Option(_)) {
                        let msg = format!(
                            "keeping a `&mut` to a `{}` inside a struct, tuple or Vec in a variable isn't supported yet (JS can't reference that slot, so writes through it would be lost); assign through the place itself (`{} = ..`)",
                            self.describe(&t),
                            inner.syntax().text()
                        );
                        self.error(r.syntax(), msg);
                    }
                }
                t
            }
            ast::Expr::PrefixExpr(p) => self.prefix(p, expected),
            ast::Expr::BinExpr(b) => self.bin(b, expected),
            ast::Expr::CastExpr(c) if c.ty().is_some_and(|t| is_ptr_type(&t)) => {
                // `&mut x as *mut i32`, `p as *const T`: the same reference.
                let to = c.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                if let Some(inner) = c.expr() {
                    let t = self.infer(&inner, None);
                    if matches!(strip_parens(&inner), ast::Expr::Literal(_)) {
                        self.error(
                            c.syntax(),
                            "casting an integer to a raw pointer isn't supported: values have no memory addresses",
                        );
                    } else {
                        self.expect(inner.syntax(), &t, &to);
                    }
                }
                to
            }
            ast::Expr::CastExpr(c)
                if c.expr().is_some_and(|i| self.is_ptr(&i))
                    && c.ty().is_some_and(|t| {
                        matches!(Ty::from_name(&t.syntax().text().to_string()), Ty::Int(_))
                    }) =>
            {
                if let Some(inner) = c.expr() {
                    self.infer(&inner, None);
                }
                self.error(
                    c.syntax(),
                    "casting a raw pointer to an integer isn't supported: values have no memory addresses (compare pointers with `std::ptr::eq`)",
                );
                Ty::Unknown
            }
            ast::Expr::CastExpr(c) => {
                let to = c.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                // `x as Box<dyn Trait>`: a coercion, checked like one.
                let coerce = matches!(self.shallow(&to), Ty::Dyn(_) | Ty::Closure { .. });
                if let Some(inner) = c.expr() {
                    if coerce {
                        self.check(&inner, &to);
                    } else {
                        self.infer(&inner, None);
                    }
                }
                to
            }
            ast::Expr::CallExpr(c)
                if result_ctor_call(c).is_some()
                    && self.lookup("Ok").is_none()
                    && !self.fns.contains_key("Ok") =>
            {
                self.result_ctor(c, result_ctor_call(c).unwrap(), expected)
            }
            ast::Expr::CallExpr(c) if is_cell_new(c) && !self.user_cell(c) => {
                let args: Vec<ast::Expr> =
                    c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
                self.arg_count(c.syntax(), "function", 1, args.len());
                let hint = match expected.map(|t| self.shallow(t)) {
                    Some(Ty::RefCell(t)) => Some(*t),
                    _ => None,
                };
                let inner = match args.first() {
                    Some(a) => match &hint {
                        Some(h) => self.check(a, h),
                        None => self.infer(a, None),
                    },
                    None => Ty::Unknown,
                };
                Ty::RefCell(Box::new(inner))
            }
            ast::Expr::CallExpr(c) if is_box_new(c) || is_rc_call(c) => {
                // `Box::new(x)` is `x`.
                let args: Vec<ast::Expr> =
                    c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
                self.arg_count(c.syntax(), "function", 1, args.len());
                match args.first() {
                    Some(a) => match expected {
                        Some(e) => {
                            self.check(a, e);
                            e.clone()
                        }
                        None => self.infer(a, None),
                    },
                    None => Ty::Unknown,
                }
            }
            ast::Expr::CallExpr(c) if is_default_call(c) => {
                let args = c.arg_list().map(|a| a.args().count()).unwrap_or(0);
                self.arg_count(c.syntax(), "function", 0, args);
                match expected {
                    Some(t) if !matches!(self.shallow(t), Ty::Var(_) | Ty::Unknown) => {
                        let ok = match self.shallow(t) {
                            Ty::Adt(..) => self.implements(t, "Default"),
                            Ty::Extern(n) => self.iface_has_default(&n),
                            other => other.has_default(),
                        };
                        if !ok {
                            let msg = format!(
                                "the trait bound `{}: Default` is not satisfied",
                                self.describe(t)
                            );
                            self.error(c.syntax(), msg);
                        }
                        t.clone()
                    }
                    // Known later (a `break` value or arm before the one that fixes the type).
                    Some(t) if matches!(self.shallow(t), Ty::Var(_)) => {
                        self.pending_defaults.push((t.clone(), c.syntax().clone()));
                        t.clone()
                    }
                    None => {
                        let t = self.fresh(VarKind::General);
                        self.pending_defaults.push((t.clone(), c.syntax().clone()));
                        t
                    }
                    _ => {
                        self.error(
                            c.syntax(),
                            "type annotations needed for `Default::default()`",
                        );
                        Ty::Unknown
                    }
                }
            }
            ast::Expr::CallExpr(c) => self.call(c, expected),
            ast::Expr::MethodCallExpr(m) => self.method(m, expected),
            ast::Expr::FieldExpr(f) => {
                let Some(recv) = f.expr() else {
                    return Ty::Unknown;
                };
                let rt = self.infer(&recv, None);
                match self.shallow(&rt) {
                    Ty::Extern(name) => self.extern_field(f, &name),
                    Ty::Adt(name, targs) => {
                        let field = f.name_ref().map(|n| n.text().to_string());
                        if let Some(field) = field
                            && self.array_fields.contains(&(name.clone(), field))
                        {
                            let r = self.range(f.syntax());
                            self.array_reads.insert(r);
                        }
                        self.adt_field(f, &name, &targs)
                    }
                    Ty::Unknown | Ty::Var(_) => Ty::Unknown,
                    // `r.start` / `r.end` (an inclusive range's are methods).
                    Ty::Range(e, false)
                        if f.name_ref()
                            .is_some_and(|n| matches!(n.text(), "start" | "end")) =>
                    {
                        *e
                    }
                    Ty::Tuple(es) if f.name_ref().is_some_and(|n| n.as_tuple_field().is_some()) => {
                        let i = f.name_ref().and_then(|n| n.as_tuple_field()).unwrap_or(0);
                        match es.get(i) {
                            Some(t) => t.clone(),
                            None => {
                                let msg = format!(
                                    "no field `{i}` on type `{}`",
                                    self.describe(&Ty::Tuple(es.clone()))
                                );
                                self.error(f.syntax(), msg);
                                Ty::Unknown
                            }
                        }
                    }
                    other => {
                        let name = f
                            .name_ref()
                            .map(|n| n.text().to_string())
                            .unwrap_or_default();
                        let msg = format!("no field `{name}` on type `{}`", self.describe(&other));
                        self.error(f.syntax(), msg);
                        Ty::Unknown
                    }
                }
            }
            ast::Expr::AwaitExpr(a) => {
                if !self.in_async {
                    self.error(
                        a.syntax(),
                        "`await` is only allowed inside `async` functions and blocks",
                    );
                }
                let Some(inner) = a.expr() else {
                    return Ty::Unknown;
                };
                let t = self.infer(&inner, None);
                match self.shallow(&t) {
                    Ty::Future(t) => *t,
                    Ty::Unknown | Ty::Var(_) => Ty::Unknown,
                    other => {
                        let msg = format!("`{}` is not a future", self.describe(&other));
                        self.error(a.syntax(), msg);
                        Ty::Unknown
                    }
                }
            }
            ast::Expr::MacroExpr(m) => match m.macro_call() {
                Some(mc) => self.macro_call(&mc, expected),
                None => Ty::Unknown,
            },
            ast::Expr::IfExpr(i) => self.if_expr(i, expected),
            ast::Expr::MatchExpr(m) => self.match_expr(m, expected),
            ast::Expr::ClosureExpr(c) => self.closure(c, expected),
            ast::Expr::RecordExpr(r) => self.record_expr(r),
            ast::Expr::RangeExpr(r) => {
                use ra_ap_syntax::ast::RangeItem;
                let inclusive = r.op_kind() == Some(ast::RangeOp::Inclusive);
                let wanted = matches!(expected.map(|t| self.shallow(t)), Some(Ty::Range(..)));
                // (`let r = 0..n;`: a value, whose `start` / `contains` / `{:?}` work.)
                let bound = r.start().is_some()
                    && r.syntax()
                        .parent()
                        .and_then(ast::LetStmt::cast)
                        .is_some_and(|l| {
                            l.ty().is_none()
                                && l.initializer().is_some_and(|i| i.syntax() == r.syntax())
                                && matches!(l.pat(), Some(ast::Pat::IdentPat(_)))
                        });
                if wanted || bound {
                    // (Any ordered bounds: floats too, for `contains`.)
                    let elem = self.fresh(VarKind::General);
                    for b in [r.start(), r.end()].into_iter().flatten() {
                        self.check(&b, &elem);
                    }
                    return Ty::Range(Box::new(elem), inclusive);
                }
                let elem = self.range_elem(r);
                Ty::Iter {
                    elem: Box::new(elem),
                    mutable: false,
                }
            }
            ast::Expr::LetExpr(le) => {
                // A `let` that isn't the whole condition of an `if`/`while`: a let-chain.
                self.error(
                    le.syntax(),
                    "`let` chains (`if let ... && ...`) are not supported yet",
                );
                if let Some(e) = le.expr() {
                    self.infer(&e, None);
                }
                Ty::Bool
            }
            // `async { .. }` / `async move { .. }`: a future of the block's value (`.await` works
            // inside; `return` and `?` leave the block).
            ast::Expr::BlockExpr(b)
                if matches!(b.modifier(), Some(ast::BlockModifier::Async(_))) =>
            {
                let inner = match expected.map(|e| self.shallow(e)) {
                    Some(Ty::Future(t)) => Some(*t),
                    _ => None,
                };
                let ret = inner
                    .clone()
                    .unwrap_or_else(|| self.fresh(VarKind::General));
                let saved_ret = std::mem::replace(&mut self.ret_ty, ret.clone());
                let saved_async = std::mem::replace(&mut self.in_async, true);
                let t = self.block_body(b, inner.as_ref());
                if t != Ty::Never {
                    self.unify(&t, &ret);
                }
                self.ret_ty = saved_ret;
                self.in_async = saved_async;
                Ty::Future(Box::new(ret))
            }
            ast::Expr::BlockExpr(b) => self.block(b, expected),
            ast::Expr::LoopExpr(l) => {
                let label = l
                    .label()
                    .and_then(|lb| lb.lifetime())
                    .map(|lt| lt.text().to_string());
                let value = self.fresh(VarKind::General);
                self.loops.push((label, Some(value.clone()), false, false));
                if let Some(body) = l.loop_body() {
                    self.block(&body, None);
                    if never_awaits_or_exits(body.syntax()) {
                        self.warn_freeze(l.syntax());
                    }
                }
                let (_, _, broke, _) = self.loops.pop().unwrap_or((None, None, false, false));
                // A `loop` without a `break` never finishes.
                if broke { value } else { Ty::Never }
            }
            ast::Expr::WhileExpr(w) => {
                let label = w
                    .label()
                    .and_then(|lb| lb.lifetime())
                    .map(|lt| lt.text().to_string());
                self.loops.push((label, None, false, false));
                self.scopes.push(HashMap::new());
                if let Some(c) = w.condition() {
                    self.condition(&c);
                }
                if let Some(body) = w.loop_body() {
                    self.block(&body, None);
                    let forever = matches!(w.condition(), Some(ast::Expr::Literal(l)) if l.syntax().text() == "true");
                    if forever && never_awaits_or_exits(body.syntax()) {
                        self.warn_freeze(w.syntax());
                    }
                }
                self.scopes.pop();
                self.loops.pop();
                Ty::Unit
            }
            ast::Expr::ForExpr(f) => {
                let label = f
                    .label()
                    .and_then(|lb| lb.lifetime())
                    .map(|lt| lt.text().to_string());
                self.loops.push((label, None, false, false));
                self.for_expr(f);
                self.loops.pop();
                Ty::Unit
            }
            ast::Expr::ReturnExpr(r) => {
                let ret = self.ret_ty.clone();
                match r.expr() {
                    Some(v) => {
                        self.check(&v, &ret);
                    }
                    None if !self.unify(&Ty::Unit, &ret) => {
                        let msg = format!(
                            "mismatched types: expected `{}`, found `()`",
                            self.describe(&ret)
                        );
                        self.error(r.syntax(), msg);
                    }
                    None => {}
                }
                Ty::Never
            }
            ast::Expr::BreakExpr(b) => {
                self.break_expr(b);
                Ty::Never
            }
            ast::Expr::ContinueExpr(_) => Ty::Never,
            ast::Expr::TupleExpr(t) => {
                let fields: Vec<_> = t.fields().collect();
                if fields.is_empty() {
                    return Ty::Unit;
                }
                let hints = match expected.map(|e| self.shallow(e)) {
                    Some(Ty::Tuple(es)) if es.len() == fields.len() => es,
                    _ => vec![],
                };
                Ty::Tuple(
                    fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| self.infer(f, hints.get(i)))
                        .collect(),
                )
            }
            ast::Expr::IndexExpr(ix) => {
                let t = self.index(ix);
                // A row of a `[[T; N]; M]` of `Copy` values is a copy too.
                if let Some(base) = ix.base()
                    && self
                        .array_reads
                        .contains(&self.range(strip_parens(&base).syntax()))
                    && matches!(self.zonk(&t), Ty::Vec(_))
                {
                    let r = self.range(ix.syntax());
                    self.array_reads.insert(r);
                }
                t
            }
            ast::Expr::TryExpr(t) => self.try_expr(t),
            ast::Expr::ArrayExpr(a) => {
                let elem = match expected.map(|t| self.shallow(t)) {
                    Some(Ty::Vec(e)) => *e,
                    _ => self.fresh(VarKind::General),
                };
                match a.kind() {
                    ast::ArrayExprKind::ElementList(items) => {
                        for item in items {
                            self.check(&item, &elem);
                        }
                    }
                    ast::ArrayExprKind::Repeat {
                        initializer,
                        repeat,
                    } => {
                        if let Some(init) = initializer {
                            self.check(&init, &elem);
                        }
                        if let Some(count) = repeat {
                            self.check(&count, &USIZE);
                        }
                    }
                }
                Ty::Vec(Box::new(elem))
            }
            _ => {
                // Unsupported expressions (codegen reports them). Still visit sub-expressions so
                // their names resolve and their errors surface.
                for child in e.syntax().children().filter_map(ast::Expr::cast) {
                    self.infer(&child, None);
                }
                Ty::Unknown
            }
        }
    }

    fn literal(&mut self, l: &ast::Literal) -> Ty {
        match l.kind() {
            LiteralKind::IntNumber(n) => match n.suffix() {
                Some(s) => Ty::from_name(s),
                None => self.fresh(VarKind::Int),
            },
            LiteralKind::FloatNumber(n) => match n.suffix() {
                Some(s) => Ty::from_name(s),
                None => self.fresh(VarKind::Float),
            },
            LiteralKind::Bool(_) => Ty::Bool,
            LiteralKind::String(_) => Ty::Str,
            LiteralKind::Char(_) => Ty::Char,
            LiteralKind::Byte(_) => Ty::Int(IntKind::U8),
            // `b"raw"`: its bytes (a `&[u8; N]`, an array here).
            LiteralKind::ByteString(_) => Ty::Vec(Box::new(Ty::Int(IntKind::U8))),
            _ => Ty::Unknown,
        }
    }

    fn path(&mut self, p: &ast::PathExpr) -> Ty {
        let Some(path) = p.path() else {
            return Ty::Unknown;
        };
        if let Some(name) = path.as_single_name_ref() {
            if let Some(local) = self.lookup(name.text()).cloned() {
                if let Some(d) = local.decl {
                    let r = self.range(p.syntax());
                    self.borrow_events.uses.push((d, r));
                    if self.copy_arrays.contains(&d) {
                        self.array_reads.insert(r);
                    }
                }
                return local.ty;
            }
            if let Some(t) = self.consts.get(name.text().to_string().as_str()).cloned() {
                if self.static_muts.contains(name.text()) && !in_unsafe(p.syntax()) {
                    self.error(
                        p.syntax(),
                        "use of mutable static is unsafe and requires unsafe block",
                    );
                }
                return t;
            }
            if let Some(t) = self.adt_path(p, &[name.text().to_string()]) {
                return t;
            }
            if name.text() == "None" {
                return Ty::Option(Box::new(self.fresh(VarKind::General)));
            }
            // `Some`, `Ok` and `Err` as functions: `.map(Some)`, `.map(Ok)`.
            if matches!(name.text(), "Some" | "Ok" | "Err") {
                let (t, e) = (self.fresh(VarKind::General), self.fresh(VarKind::General));
                let (param, ret) = match name.text() {
                    "Some" => (t.clone(), Ty::Option(Box::new(t))),
                    "Ok" => (t.clone(), Ty::Result(Box::new(t), Box::new(e))),
                    _ => (e.clone(), Ty::Result(Box::new(t), Box::new(e))),
                };
                return Ty::Closure {
                    params: vec![param],
                    ret: Box::new(ret),
                };
            }
            if let Some(sig) = self.fns.get(name.text()) {
                let (is_async, tps) = (sig.is_async, sig.type_params.clone());
                let (params, ret) = (sig.params.clone(), sig.ret.clone());
                let (params, ret) = self.instantiate(p.syntax(), &tps, &params, &ret, &[]);
                let ret = if is_async {
                    Ty::Future(Box::new(ret))
                } else {
                    ret
                };
                return Ty::Closure {
                    params,
                    ret: Box::new(ret),
                };
            }
            self.error(
                p.syntax(),
                format!("cannot find value `{}` in this scope", name.text()),
            );
            return Ty::Unknown;
        }
        let segs: Vec<String> = path
            .segments()
            .filter_map(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .collect();
        if let [single] = self.strip_modules(&segs).as_slice() {
            if let Some(t) = self.consts.get(single).cloned() {
                if self.static_muts.contains(single) && !in_unsafe(p.syntax()) {
                    self.error(
                        p.syntax(),
                        "use of mutable static is unsafe and requires unsafe block",
                    );
                }
                return t;
            }
            if let Some(sig) = self.fns.get(single) {
                let (is_async, tps) = (sig.is_async, sig.type_params.clone());
                let (params, ret) = (sig.params.clone(), sig.ret.clone());
                let (params, ret) = self.instantiate(p.syntax(), &tps, &params, &ret, &[]);
                let ret = if is_async {
                    Ty::Future(Box::new(ret))
                } else {
                    ret
                };
                return Ty::Closure {
                    params,
                    ret: Box::new(ret),
                };
            }
        }
        if let Some(t) = self.adt_path(p, &segs) {
            return t;
        }
        // `String::from` / `ToString::to_string` as function values.
        if matches!(
            segs.iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            [.., "String", "from"] | [.., "ToString", "to_string"]
        ) {
            return Ty::Closure {
                params: vec![Ty::Str],
                ret: Box::new(Ty::Str),
            };
        }
        // `char::from` (from a byte) as a function value: `.map(char::from)`.
        if segs.len() >= 2 && segs[segs.len() - 2] == "char" && segs[segs.len() - 1] == "from" {
            return Ty::Closure {
                params: vec![Ty::Int(IntKind::U8)],
                ret: Box::new(Ty::Char),
            };
        }
        let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
        // `T::SIDES`: an associated const of a bound of `T`.
        if let [param, konst] = segs.as_slice()
            && let Some(t) = self.param_const(param, konst, p.syntax())
        {
            return t;
        }
        if let [.., "Ordering", "Less" | "Equal" | "Greater"] = segs.as_slice() {
            return Ty::Ordering;
        }
        if let Some((_, ty)) = std_const(std_tail(&segs)) {
            return ty;
        }
        if let Some(t) = self.jsvalue_path(&segs) {
            return t;
        }
        self.error(
            p.syntax(),
            format!(
                "cannot find value `{}` (paths are resolved in phase 2)",
                path.syntax().text()
            ),
        );
        Ty::Unknown
    }

    /// Whether `e` names a closure parameter of `filter`, `find`, `retain`, `sort_by` .. (which
    /// get `&Item`), so `*e` only drops that reference.
    fn derefs_reference(&self, e: &ast::Expr) -> bool {
        let ast::Expr::PathExpr(p) = strip_parens(e) else {
            return false;
        };
        let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
            return false;
        };
        let name = name.text().to_string();
        for anc in p.syntax().ancestors() {
            let Some(cl) = ast::ClosureExpr::cast(anc) else {
                continue;
            };
            let declares = cl.param_list().is_some_and(|pl| {
                pl.params().any(|prm| {
                    matches!(prm.pat(), Some(ast::Pat::IdentPat(ip))
                        if ip.name().is_some_and(|n| n.text() == name.as_str()))
                })
            });
            if !declares {
                continue;
            }
            return cl
                .syntax()
                .parent()
                .and_then(|a| a.parent())
                .and_then(ast::MethodCallExpr::cast)
                .and_then(|m| m.name_ref())
                .is_some_and(|n| {
                    matches!(
                        n.text().to_string().as_str(),
                        "filter"
                            | "find"
                            | "take_while"
                            | "skip_while"
                            | "retain"
                            | "sort_by"
                            | "sort_unstable_by"
                            | "max_by"
                            | "min_by"
                            | "dedup_by"
                            | "binary_search_by"
                            | "partition"
                            | "is_sorted_by"
                            | "inspect"
                    )
                });
        }
        false
    }

    fn prefix(&mut self, p: &ast::PrefixExpr, expected: Option<&Ty>) -> Ty {
        let Some(inner) = p.expr() else {
            return Ty::Unknown;
        };
        let t = self.infer(&inner, expected);
        let resolved = self.shallow(&t);
        if let (Ty::Adt(n, _), Some(op @ (UnaryOp::Neg | UnaryOp::Not))) = (&resolved, p.op_kind())
        {
            return self.unary_overload(p.syntax(), op == UnaryOp::Neg, n, &t);
        }
        // (`*h` of a reference binding strips the reference, not the value's `Deref`:
        // `(*h).clone()` in `.filter(|h| ..)` over `&Host`s is a `Host`.)
        if p.op_kind() == Some(UnaryOp::Deref)
            && !self.derefs_reference(&inner)
            && let Some((n, target)) = self.deref_target(&t)
        {
            self.auto_deref(&inner, &n, &target);
            return target;
        }
        match p.op_kind() {
            Some(UnaryOp::Deref) | None => t,
            Some(UnaryOp::Neg) if matches!(resolved, Ty::Param(_)) => {
                let Ty::Param(n) = resolved.clone() else {
                    unreachable!()
                };
                self.require_param_bound(p.syntax(), &t, "Neg", "`-`");
                self.param_class(&n, p.syntax(), None);
                t
            }
            // (`!` on an unsigned number depends on its width.)
            Some(UnaryOp::Not) if matches!(resolved, Ty::Param(_)) => {
                let Ty::Param(n) = resolved.clone() else {
                    unreachable!()
                };
                self.require_param_bound(p.syntax(), &t, "Not", "`!`");
                self.param_class(&n, p.syntax(), Some("`!`"));
                t
            }
            Some(UnaryOp::Neg) => {
                let ok = match &resolved {
                    Ty::Int(k) => k.is_signed(),
                    Ty::Float { .. } | Ty::Var(_) | Ty::Unknown | Ty::Never => true,
                    _ => false,
                };
                if !ok {
                    let msg = format!(
                        "cannot apply unary operator `-` to type `{}`",
                        self.describe(&t)
                    );
                    self.error(p.syntax(), msg);
                }
                t
            }
            Some(UnaryOp::Not) => {
                let ok = matches!(resolved, Ty::Bool | Ty::Int(_) | Ty::Unknown | Ty::Never)
                    || self.var_kind(&t).is_some_and(|k| k != VarKind::Float);
                if !ok {
                    let msg = format!(
                        "cannot apply unary operator `!` to type `{}`",
                        self.describe(&t)
                    );
                    self.error(p.syntax(), msg);
                }
                t
            }
        }
    }

    /// The `v[a..b]` a place goes through (`v[a..b]`, `v[a..b][i]`, `v[a..b][i].x`), if any.
    fn range_slice_in(e: &ast::Expr) -> Option<ast::IndexExpr> {
        let mut cur = strip_parens(e);
        loop {
            match cur {
                ast::Expr::IndexExpr(ix) => {
                    if matches!(ix.index(), Some(ast::Expr::RangeExpr(_))) {
                        return Some(ix);
                    }
                    cur = strip_parens(&ix.base()?);
                }
                ast::Expr::FieldExpr(f) => cur = strip_parens(&f.expr()?),
                _ => return None,
            }
        }
    }

    /// Root local of a place expression (`v`, `v[i]`, `v[i][j]`, `*r`), plus whether the place is
    /// reached through indexing or a deref (a mutation *through* the binding, not of it).
    fn place_root(e: &ast::Expr) -> Option<(ast::NameRef, bool)> {
        match e {
            ast::Expr::PathExpr(p) => Some((p.path()?.as_single_name_ref()?, false)),
            ast::Expr::ParenExpr(p) => Self::place_root(&p.expr()?),
            ast::Expr::IndexExpr(ix) => Some((Self::place_root(&ix.base()?)?.0, true)),
            ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Deref) => {
                Some((Self::place_root(&p.expr()?)?.0, true))
            }
            ast::Expr::RefExpr(r) => Self::place_root(&r.expr()?),
            ast::Expr::FieldExpr(f) => Some((Self::place_root(&f.expr()?)?.0, true)),
            _ => None,
        }
    }

    /// Whether `e` is `map[..]` on a `HashMap`, which can be read but not assigned through.
    fn indexes_a_map(&mut self, e: &ast::Expr) -> bool {
        let ast::Expr::IndexExpr(ix) = strip_parens(e) else {
            return false;
        };
        let Some(b) = ix.base() else { return false };
        let t = self.infer(&b, None);
        matches!(self.shallow(&t), Ty::Map(..))
    }

    /// rustc's mutability rules for assigning to or mutably borrowing a place.
    fn check_mutable(&mut self, e: &ast::Expr, how: Mutation) {
        // A write through `v[a..b]` would only change a copy, except for the in-place slice
        // methods, which codegen writes back.
        if let Some(range) = Self::range_slice_in(e) {
            let in_place = strip_parens(e).syntax() == range.syntax()
                && e.syntax()
                    .parent()
                    .and_then(ast::MethodCallExpr::cast)
                    .is_some_and(|m| {
                        m.receiver().is_some_and(|r| r.syntax() == e.syntax())
                            && m.name_ref()
                                .is_some_and(|n| crate::ty::slice_mut_method(n.text()))
                    });
            // `&mut v[a..b]` has its own error.
            let borrowed = e
                .syntax()
                .ancestors()
                .take(3)
                .filter_map(ast::RefExpr::cast)
                .any(|r| r.mut_token().is_some());
            if !in_place && !borrowed {
                // `v[1..].iter_mut()`, `v[0..2][1] = 9`: the range is a view of `v` instead.
                let base = range
                    .base()
                    .and_then(|b| self.raw.get(&self.range(b.syntax())).cloned());
                if base.is_some_and(|b| self.zonk(&b) == Ty::Str) {
                    self.error(
                        e.syntax(),
                        "writing through a range of a string isn't supported (strings are values in JS); work on the string itself",
                    );
                    return;
                }
                let r = self.range(range.syntax());
                self.slice_views.insert(r);
            }
        }
        let Some((name, through)) = Self::place_root(e) else {
            return;
        };
        if let Some(d) = self.lookup(name.text()).and_then(|l| l.decl) {
            let r = self.range(e.syntax());
            self.borrow_events
                .changes
                .push((d, r, how == Mutation::Assign && !through));
        }
        let Some(local) = self.lookup(name.text()).cloned() else {
            // A `static` or `const` item.
            let n = name.text().to_string();
            if how != Mutation::Assign {
                return;
            }
            if self.statics.contains(&n) {
                self.error(
                    e.syntax(),
                    format!("cannot assign to immutable static item `{n}`"),
                );
            } else if self.consts.contains_key(&n) && !self.static_muts.contains(&n) {
                self.error(
                    e.syntax(),
                    "invalid left-hand side of assignment (a `const` can't change)",
                );
            }
            return;
        };
        let n = name.text();
        if how == Mutation::Assign && !through {
            if !local.mutable && !local.deferred {
                self.error(e.syntax(), format!("cannot assign twice to immutable variable `{n}` (declare it with `let mut`)"));
            }
            return;
        }
        match local.reference {
            Some(true) => {}
            Some(false) => self.error(
                e.syntax(),
                format!("cannot borrow `*{n}` as mutable, as it is behind a `&` reference"),
            ),
            None if !local.mutable => self.error(
                e.syntax(),
                format!("cannot borrow `{n}` as mutable, as it is not declared as mutable"),
            ),
            None => {}
        }
    }

    fn bin(&mut self, b: &ast::BinExpr, expected: Option<&Ty>) -> Ty {
        let (Some(lhs), Some(rhs), Some(op)) = (b.lhs(), b.rhs(), b.op_kind()) else {
            return Ty::Unknown;
        };
        match op {
            BinaryOp::Assignment { op: None } if matches!(lhs, ast::Expr::TupleExpr(_)) => {
                let ast::Expr::TupleExpr(t) = &lhs else {
                    unreachable!()
                };
                let mut tys = Vec::new();
                for f in t.fields() {
                    let through = Self::place_root(&f).is_some_and(|(_, t)| t);
                    self.check_mutable(
                        &f,
                        if through {
                            Mutation::Borrow
                        } else {
                            Mutation::Assign
                        },
                    );
                    tys.push(self.infer(&f, None));
                }
                self.record(lhs.syntax(), Ty::Tuple(tys.clone()));
                self.check(&rhs, &Ty::Tuple(tys));
                Ty::Unit
            }
            BinaryOp::Assignment { .. } if self.indexes_a_map(&lhs) => {
                self.infer(&lhs, None);
                self.infer(&rhs, None);
                self.error(
                    lhs.syntax(),
                    "cannot assign to data in an index of a `HashMap` (use `map.insert(k, v)` or `*map.entry(k).or_insert(..) = ..`)",
                );
                Ty::Unit
            }
            BinaryOp::Assignment { op } => {
                if op.is_none() {
                    self.ptr_assign(b);
                }
                let through = Self::place_root(&lhs).is_some_and(|(_, t)| t);
                self.check_mutable(
                    &lhs,
                    if through {
                        Mutation::Borrow
                    } else {
                        Mutation::Assign
                    },
                );
                let lt = self.infer(&lhs, None);
                match op {
                    Some(op) if matches!(self.shallow(&lt), Ty::Param(_)) => {
                        let Ty::Param(n) = self.shallow(&lt) else {
                            unreachable!()
                        };
                        self.param_arith(b, op, &n, &lt, &rhs, true);
                    }
                    Some(ArithOp::Shl | ArithOp::Shr) => {
                        self.infer(&rhs, None);
                    }
                    Some(op) => {
                        self.arith_operands(b, op, &lt, &rhs);
                    }
                    None => {
                        self.check(&rhs, &lt);
                    }
                }
                Ty::Unit
            }
            BinaryOp::LogicOp(_) => {
                self.check(&lhs, &Ty::Bool);
                self.check(&rhs, &Ty::Bool);
                Ty::Bool
            }
            BinaryOp::CmpOp(cmp) => {
                let lt = self.infer(&lhs, None);
                let rt = self.infer(&rhs, Some(&lt));
                if !self.unify(&lt, &rt) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `{}`",
                        self.describe(&lt),
                        self.describe(&rt)
                    );
                    self.error(rhs.syntax(), msg);
                }
                let tr = if matches!(cmp, CmpOp::Ord { .. }) {
                    "PartialOrd"
                } else {
                    "PartialEq"
                };
                self.require_trait(b.syntax(), &lt, tr);
                if matches!(cmp, CmpOp::Ord { .. })
                    && matches!(self.shallow(&lt), Ty::Set(_) | Ty::Map(..))
                {
                    let msg = format!("`{}` can't be compared with `<`/`>`", self.describe(&lt));
                    self.error(b.syntax(), msg);
                }
                Ty::Bool
            }
            BinaryOp::ArithOp(op @ (ArithOp::Shl | ArithOp::Shr)) => {
                let lt = self.infer(&lhs, expected);
                if let Ty::Param(n) = self.shallow(&lt) {
                    return self.param_arith(b, op, &n, &lt, &rhs, false);
                }
                let rt = self.infer(&rhs, None);
                for (t, side) in [(&lt, &lhs), (&rt, &rhs)] {
                    if !self.is_integer(t) {
                        let msg = format!(
                            "no implementation for `{} {op} {}`",
                            self.describe(&lt),
                            self.describe(&rt)
                        );
                        self.error(side.syntax(), msg);
                        break;
                    }
                }
                lt
            }
            BinaryOp::ArithOp(op) => {
                let lt = self.infer(&lhs, expected);
                if let Ty::Param(n) = self.shallow(&lt) {
                    return self.param_arith(b, op, &n, &lt, &rhs, false);
                }
                self.arith_operands(b, op, &lt, &rhs)
            }
        }
    }

    /// The type arguments a path's segment `name` gives (`HashMap::<u8, u8>::new()`); `_` is a
    /// fresh variable.
    fn segment_targs(&mut self, segs: &[ast::PathSegment], name: &str) -> Vec<Ty> {
        let args: Vec<ast::GenericArg> = segs
            .iter()
            .find(|s| s.name_ref().is_some_and(|n| n.text() == name))
            .and_then(|s| s.generic_arg_list())
            .map(|l| l.generic_args().collect())
            .unwrap_or_default();
        args.into_iter()
            .map(|g| match g {
                ast::GenericArg::TypeArg(ta) => ta
                    .ty()
                    .map(|t| self.lower_type(&t))
                    .unwrap_or_else(|| self.fresh(VarKind::General)),
                _ => self.fresh(VarKind::General),
            })
            .collect()
    }

    fn is_integer(&self, t: &Ty) -> bool {
        matches!(self.shallow(t), Ty::Int(_) | Ty::Unknown | Ty::Never)
            || self.var_kind(t) == Some(VarKind::Int)
    }

    /// Check the right operand of `lhs op rhs` (or `lhs op= rhs`) and the operator's validity.
    fn arith_operands(&mut self, b: &ast::BinExpr, op: ArithOp, lt: &Ty, rhs: &ast::Expr) -> Ty {
        if let Ty::Adt(n, _) = self.shallow(lt) {
            let assign = matches!(b.op_kind(), Some(BinaryOp::Assignment { .. }));
            return self.op_overload(b.syntax(), op, assign, &n, lt, rhs);
        }
        if self.shallow(lt) == Ty::Str && op == ArithOp::Add {
            self.check(rhs, &Ty::Str);
            return Ty::Str;
        }
        let rt = self.infer(rhs, Some(lt));
        if !self.unify(lt, &rt) {
            let (l, r) = (self.describe(lt), self.describe(&rt));
            let msg = match op {
                ArithOp::Add => format!("cannot add `{r}` to `{l}`"),
                ArithOp::Sub => format!("cannot subtract `{r}` from `{l}`"),
                ArithOp::Mul => format!("cannot multiply `{l}` by `{r}`"),
                ArithOp::Div => format!("cannot divide `{l}` by `{r}`"),
                _ => format!("no implementation for `{l} {op} {r}`"),
            };
            self.error(b.syntax(), msg);
            return lt.clone();
        }
        let resolved = self.shallow(lt);
        let bitwise = matches!(op, ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor);
        let ok = match &resolved {
            Ty::Int(_) | Ty::Unknown | Ty::Never => true,
            Ty::Float { .. } => !bitwise,
            Ty::Bool => bitwise,
            Ty::Var(_) => !(bitwise && self.var_kind(lt) == Some(VarKind::Float)),
            _ => false,
        };
        if !ok {
            let msg = format!(
                "cannot apply binary operator `{op}` to type `{}`",
                self.describe(lt)
            );
            self.error(b.syntax(), msg);
        }
        lt.clone()
    }

    fn arg_count(&mut self, n: &SyntaxNode, what: &str, expected: usize, found: usize) {
        if expected != found {
            let s = if expected == 1 { "" } else { "s" };
            self.error(
                n,
                format!("this {what} takes {expected} argument{s} but {found} were supplied"),
            );
        }
    }

    fn call(&mut self, c: &ast::CallExpr, expected: Option<&Ty>) -> Ty {
        let args: Vec<ast::Expr> = c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let Some(ast::Expr::PathExpr(callee)) = c.expr() else {
            // `make_adder(1)(2)`: call whatever the expression evaluates to.
            let ft = c
                .expr()
                .map(|e| self.infer(&e, None))
                .unwrap_or(Ty::Unknown);
            if let Ty::Closure { params, ret } = self.shallow(&ft) {
                self.arg_count(c.syntax(), "closure", params.len(), args.len());
                for (a, p) in args.iter().zip(params.iter()) {
                    self.check(a, p);
                }
                return *ret;
            }
            for a in &args {
                self.infer(a, None);
            }
            return Ty::Unknown;
        };
        if let Some(t) = self.qualified_call(c, &callee, &args) {
            return t;
        }
        let Some(path) = callee.path() else {
            return Ty::Unknown;
        };
        // A std function imported by name (`use std::cmp::max;`) is resolved like its full path.
        let std_alias = path.as_single_name_ref().and_then(|n| {
            let n = n.text().to_string();
            (self.lookup(&n).is_none() && !self.fns.contains_key(&n))
                .then(|| self.std_fns.get(&n).cloned())
                .flatten()
        });
        if let Some(name) = path.as_single_name_ref()
            && std_alias.is_none()
        {
            if name.text() == "Some"
                && self.lookup("Some").is_none()
                && !self.fns.contains_key("Some")
            {
                return self.some_call(c, &args);
            }
            if let Some(local) = self.lookup(name.text())
                && let Ty::Closure { params, ret } = self.shallow(&local.ty.clone())
            {
                self.arg_count(c.syntax(), "closure", params.len(), args.len());
                for (a, p) in args.iter().zip(params.iter()) {
                    self.check(a, p);
                }
                return *ret;
            }
            if let Some(local) = self.lookup(name.text()) {
                let lt = self.shallow(&local.ty.clone());
                let numeric = matches!(self.var_kind(&lt), Some(VarKind::Int | VarKind::Float));
                if !matches!(lt, Ty::Var(_) | Ty::Unknown) || numeric {
                    let msg = format!("expected function, found `{}`", self.describe(&lt));
                    self.error(callee.syntax(), msg);
                }
                for a in &args {
                    self.infer(a, None);
                }
                return Ty::Unknown;
            }
            if !self.fns.contains_key(name.text().to_string().as_str())
                && let Some(t) = self.adt_call(c, &[name.text().to_string()], &args)
            {
                return t;
            }
            // `use js::set_timeout;` then `set_timeout(..)`.
            if !self.fns.contains_key(name.text().to_string().as_str())
                && let Some(t) = self.js_bound_call(c, &[&*name.text().to_string()], &args)
            {
                return t;
            }
            // `use serde_wasm_bindgen::from_value;` then `from_value(v)`.
            if !self.fns.contains_key(name.text().to_string().as_str())
                && let Some(t) = self.swb_call(c, &[&*name.text().to_string()], &args, expected)
            {
                return t;
            }
            return self.call_fn(c, name.text().to_string().as_str(), &path, &args);
        }

        let segs: Vec<ast::PathSegment> = path.segments().collect();
        // `<[i64; 2]>::try_from(v)`
        if let Some(t) = self.array_try_from(c, &path, &args) {
            return t;
        }
        let names: Vec<String> = std_alias.unwrap_or_else(|| {
            segs.iter()
                .filter_map(|s| s.name_ref())
                .map(|n| n.text().to_string())
                .collect()
        });
        let stripped = self.strip_modules(&names);
        if let [single] = stripped.as_slice()
            && self.fns.contains_key(single)
        {
            return self.call_fn(c, single, &path, &args);
        }
        if let ([module, ..], [single]) = (names.as_slice(), stripped.as_slice())
            && self.module_exports.contains_key(module)
            && self.adts.resolve(None, &stripped).is_none()
        {
            let msg = format!(
                "cannot find function `{single}` in module `{module}` (it doesn't exist or isn't `pub`)"
            );
            self.error(c.syntax(), msg);
            for a in &args {
                self.infer(a, None);
            }
            return Ty::Unknown;
        }
        // `TokenStream::from_iter(tokens)`
        if let [.., ty, f] = names.as_slice()
            && ty == "TokenStream"
            && f == "from_iter"
            && self.adts.method("TokenStream", "__push").is_some()
        {
            self.arg_count(c.syntax(), "function", 1, args.len());
            if let Some(a) = args.first() {
                let at = self.infer(a, None);
                self.require_tokens(a.syntax(), &at);
            }
            return Ty::Adt("TokenStream".into(), vec![]);
        }
        if let Some(t) = self.adt_call(c, &names, &args) {
            return t;
        }
        {
            let ns: Vec<&str> = names.iter().map(String::as_str).collect();
            if let Some(t) = self.coll_call(c, &ns, &args) {
                return t;
            }
            let tail = match ns.as_slice() {
                ["std" | "core", rest @ ..] => rest,
                all => all,
            };
            if let Some(t) = self.ptr_call(c, tail, &args, expected) {
                return t;
            }
            // `JSON::parse::<T>(s)` (the `js` crate), and its bound functions (`Date::now()`,
            // `js::set_timeout(..)`).
            if let Some(t) = self.js_call(c, tail, &args, expected) {
                return t;
            }
            if let Some(t) = self.swb_call(c, tail, &args, expected) {
                return t;
            }
            if let Some(t) = self.iface_call(c, tail, &args) {
                return t;
            }
            if let Some(t) = self.js_bound_call(c, &ns, &args) {
                return t;
            }
            if let Some(t) = self.crate_call(c, &ns, &args) {
                return t;
            }
            if let Some(t) = self.itertools_call(c, &ns, &args) {
                return t;
            }
            if let Some(t) = self.anyhow_call(c, &ns, &args) {
                return t;
            }
            if let Some(t) = self.std_call(c, &ns, &args) {
                return t;
            }
            if let Some(t) = self.gaps_call(c, &ns, &args) {
                return t;
            }
            // `i32::zero()`: an associated function of a trait you implemented for a built-in
            // type.
            if let [t, f] = ns.as_slice()
                && let Some(key) = crate::adt::ext_key(t)
                && self.adts.ext_types.contains(key)
                && let Some(sig) = self.adts.method(key, f).cloned()
                && sig.receiver.is_none()
            {
                let sig = self.method_inst(&sig, None);
                self.arg_count(c.syntax(), "function", sig.params.len(), args.len());
                for (a, p) in args.iter().zip(&sig.params) {
                    self.check(a, p);
                }
                self.record_ext(c.syntax(), key);
                return sig.ret;
            }
        }
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        match std_tail(&names) {
            ["String", "new"] => {
                self.arg_count(c.syntax(), "function", 0, args.len());
                Ty::Str
            }
            ["String", "from"] => {
                self.arg_count(c.syntax(), "function", 1, args.len());
                if let Some(a) = args.first() {
                    self.check(a, &Ty::Str);
                }
                Ty::Str
            }
            ["Vec" | "vec", ctor @ ("new" | "with_capacity")] => {
                self.arg_count(
                    c.syntax(),
                    "function",
                    usize::from(*ctor == "with_capacity"),
                    args.len(),
                );
                if let Some(a) = args.first() {
                    self.check(a, &USIZE);
                }
                // `Vec::<T>::new()` names the element type.
                let named = segs
                    .iter()
                    .find(|s| s.name_ref().is_some_and(|n| n.text() == "Vec"))
                    .and_then(|s| s.generic_arg_list())
                    .and_then(|l| l.generic_args().next());
                let elem = match named {
                    Some(ast::GenericArg::TypeArg(ta)) => ta.ty().map(|t| self.lower_type(&t)),
                    _ => None,
                };
                Ty::Vec(Box::new(
                    elem.unwrap_or_else(|| self.fresh(VarKind::General)),
                ))
            }
            [.., "HashMap", ctor @ ("new" | "with_capacity" | "from")] => {
                // (`HashMap::<K, V>::new()` names them.)
                let named = self.segment_targs(&segs, "HashMap");
                let mut named = named.into_iter();
                let k = named.next().unwrap_or_else(|| self.fresh(VarKind::General));
                let v = named.next().unwrap_or_else(|| self.fresh(VarKind::General));
                if *ctor == "from" {
                    self.arg_count(c.syntax(), "function", 1, args.len());
                    if let Some(a) = args.first() {
                        let pairs = Ty::Vec(Box::new(Ty::Tuple(vec![k.clone(), v.clone()])));
                        self.check(a, &pairs);
                    }
                } else {
                    self.arg_count(
                        c.syntax(),
                        "function",
                        usize::from(*ctor == "with_capacity"),
                        args.len(),
                    );
                    if let Some(a) = args.first() {
                        self.check(a, &USIZE);
                    }
                }
                Ty::Map(Box::new(k), Box::new(v))
            }
            [.., "HashSet", ctor @ ("new" | "with_capacity" | "from")] => {
                let ctor = *ctor;
                let elem = self
                    .segment_targs(&segs, "HashSet")
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| self.fresh(VarKind::General));
                match ctor {
                    "from" => {
                        self.arg_count(c.syntax(), "function", 1, args.len());
                        if let Some(a) = args.first() {
                            self.check(a, &Ty::Vec(Box::new(elem.clone())));
                        }
                    }
                    _ => {
                        self.arg_count(
                            c.syntax(),
                            "function",
                            usize::from(ctor == "with_capacity"),
                            args.len(),
                        );
                        if let Some(a) = args.first() {
                            self.check(a, &USIZE);
                        }
                    }
                }
                Ty::Set(Box::new(elem))
            }
            ["cmp", "max" | "min"] => {
                self.arg_count(c.syntax(), "function", 2, args.len());
                let t = args
                    .first()
                    .map(|a| self.infer(a, None))
                    .unwrap_or(Ty::Unknown);
                if let Some(b) = args.get(1) {
                    self.check(b, &t);
                }
                let what = names.last().copied().unwrap_or("max");
                if self.zonk(&t).contains_float() || self.var_kind(&t) == Some(VarKind::Float) {
                    let msg = format!(
                        "the trait bound `{}: Ord` is not satisfied (floats aren't totally ordered; use `a.{what}(b)` on `f64`)",
                        self.describe(&t)
                    );
                    self.error(c.syntax(), msg);
                }
                t
            }
            [ty_name, "from"] if Ty::from_name(ty_name).is_numeric() => {
                for a in &args {
                    self.infer(a, None);
                }
                Ty::from_name(ty_name)
            }
            // `T::make()` / `T::default()` on a type parameter: the function takes `T`'s class.
            [param, f]
                if self.param_bounds.contains_key(*param)
                    && let Some(t) = self.param_static_call(c, param, f, &args) =>
            {
                t
            }
            [param, f] if self.param_bounds.contains_key(*param) => {
                let msg = format!(
                    "no function or associated item named `{f}` found for type parameter `{param}` in the current scope (its bounds don't declare one)"
                );
                self.error(callee.syntax(), msg);
                for a in &args {
                    self.infer(a, None);
                }
                Ty::Unknown
            }
            _ => {
                self.error(
                    callee.syntax(),
                    format!(
                        "cannot find function `{}` (paths are resolved in phase 2)",
                        path.syntax().text()
                    ),
                );
                for a in &args {
                    self.infer(a, None);
                }
                Ty::Unknown
            }
        }
    }

    /// A field of a JS interface from the bindings.
    fn extern_field(&mut self, f: &ast::FieldExpr, name: &str) -> Ty {
        let field = f.name_ref().map(|n| bare(n.text())).unwrap_or_default();
        let b = self.bindings.clone();
        let Some(et) = b.types.get(name) else {
            return Ty::Extern(name.to_string()); // untyped NS: namespaces stay untyped too
        };
        if let Some(fd) = et.fields.get(&field) {
            return fd.ty.clone();
        }
        let msg = if et.methods.contains_key(&field) {
            format!("`{field}` is a method on `{name}`, not a field (call it: `.{field}()`)")
        } else {
            let hint = bindings::suggest(&field, et.fields.keys())
                .map(|s| format!("; did you mean `{s}`?"))
                .unwrap_or_default();
            format!("no field `{field}` on type `{name}`{hint}")
        };
        self.error(f.syntax(), msg);
        Ty::Unknown
    }

    /// A method call on a JS interface: arguments and return type from the bindings.
    fn extern_method(
        &mut self,
        m: &ast::MethodCallExpr,
        name: &str,
        method: &str,
        args: &[ast::Expr],
    ) -> Ty {
        let b = self.bindings.clone();
        let loose = |c: &mut Self| {
            for a in args {
                c.infer(a, None);
            }
        };
        let Some(et) = b.types.get(name) else {
            loose(self);
            return Ty::Unknown; // untyped NS
        };
        let Some(sig) = et.methods.get(&bare(method)) else {
            let msg = if et.fields.contains_key(&bare(method)) {
                format!("`{method}` is a field of `{name}`, not a method (drop the parentheses)")
            } else {
                let hint = bindings::suggest(method, et.methods.keys())
                    .map(|s| format!("; did you mean `{s}`?"))
                    .unwrap_or_default();
                format!("no method named `{method}` found for `{name}`{hint}")
            };
            self.error(m.syntax(), msg);
            loose(self);
            return Ty::Unknown;
        };
        if sig.is_static {
            let msg = format!(
                "`{method}` is an associated function of `{name}`, not a method: call `{name}::{method}(..)`"
            );
            self.error(m.syntax(), msg);
        }
        let label = format!("{name}::{method}");
        self.extern_args(m.syntax(), &label, sig, args);
        if sig.is_async {
            Ty::Future(Box::new(sig.ret.clone()))
        } else {
            sig.ret.clone()
        }
    }

    /// The arguments of a bound JS function or method (`label` names it in messages).
    pub(super) fn extern_args(
        &mut self,
        node: &SyntaxNode,
        label: &str,
        sig: &bindings::Method,
        args: &[ast::Expr],
    ) {
        let required = sig.params.iter().filter(|p| !p.optional).count();
        let max = if sig.variadic.is_some() {
            usize::MAX
        } else {
            sig.params.len()
        };
        if args.len() < required || args.len() > max {
            let plural = |n: usize| if n == 1 { "" } else { "s" };
            let range = match (required, max) {
                (r, m) if r == m => format!("{r} argument{}", plural(r)),
                (r, usize::MAX) => format!("at least {r} argument{}", plural(r)),
                (r, m) => format!("{r} to {m} arguments"),
            };
            self.error(
                node,
                format!(
                    "`{label}` takes {range} but {} {} supplied",
                    args.len(),
                    if args.len() == 1 { "was" } else { "were" }
                ),
            );
        }
        for (i, a) in args.iter().enumerate() {
            let ty = match sig.params.get(i) {
                Some(p) => p.ty.clone(),
                None => sig.variadic.clone().unwrap_or(Ty::Unknown),
            };
            if ty == Ty::Unknown {
                self.infer(a, None);
            } else if matches!(ty, Ty::Float { .. }) && is_int_literal(a) {
                // Deliberate extension: every NS number is `f64`, and `ns.sleep(100)` is how every
                // Bitburner script reads, so unsuffixed integer literals are accepted here.
                self.record(a.syntax(), ty);
            } else {
                self.check(a, &ty);
            }
        }
    }

    /// `expr?` on an `Option`: the value, or an early `return None`.
    /// `?`'s error `e` into the function's `re`: the same, or converted.
    fn check_try_error(&mut self, t: &ast::TryExpr, e: &Ty, re: &Ty) {
        if !self.try_convert(t, e, re) && !self.unify(e, re) {
            let msg = format!(
                "`?` couldn't convert the error to `{}` (add `impl From<{}> for {}`, or convert with `.map_err(..)?`)",
                self.describe(re),
                self.describe(e),
                self.describe(re)
            );
            self.error(t.syntax(), msg);
        }
    }

    /// The `?`s whose error wasn't known when they were checked.
    fn resolve_tries(&mut self) {
        for (t, e, re) in std::mem::take(&mut self.pending_tries) {
            if matches!(self.shallow(&e), Ty::Var(_)) {
                self.unify(&e, &re);
            } else {
                self.check_try_error(&t, &e, &re);
            }
        }
    }

    fn try_expr(&mut self, t: &ast::TryExpr) -> Ty {
        let Some(inner) = t.expr() else {
            return Ty::Unknown;
        };
        let it = self.infer(&inner, None);
        let ret = self.ret_ty.clone();
        // A closure without a return type (`|acc, c| Some(acc + c.to_digit(10)? as f64)`): `?`
        // makes it return what its operand is, as rustc infers from the body.
        if matches!(self.shallow(&ret), Ty::Var(_)) && self.var_kind(&ret) == Some(VarKind::General)
        {
            match self.shallow(&it) {
                Ty::Option(_) => {
                    let x = self.fresh(VarKind::General);
                    self.unify(&ret, &Ty::Option(Box::new(x)));
                }
                Ty::Result(_, e) => {
                    let x = self.fresh(VarKind::General);
                    self.unify(&ret, &Ty::Result(Box::new(x), e));
                }
                _ => {}
            }
        }
        let returns_result = matches!(self.shallow(&ret), Ty::Result(..));
        if let Ty::Var(_) = self.shallow(&it)
            && returns_result
            && self.var_kind(&it) == Some(VarKind::General)
        {
            let (x, e) = (self.fresh(VarKind::General), self.fresh(VarKind::General));
            self.unify(&it, &Ty::Result(Box::new(x), Box::new(e)));
        }
        if let Ty::Result(x, e) = self.shallow(&it) {
            match self.shallow(&ret) {
                // (An error not known yet, like a `parse()`'s whose target comes later: checked
                // once the types are settled.)
                Ty::Result(_, re)
                    if matches!(self.shallow(&e), Ty::Var(_))
                        && self.var_kind(&e) == Some(VarKind::General)
                        && !matches!(self.shallow(&re), Ty::Var(_)) =>
                {
                    self.pending_tries.push((t.clone(), *e, *re));
                }
                Ty::Result(_, re) => self.check_try_error(t, &e, &re),
                Ty::Unknown => {}
                other => {
                    let msg = format!(
                        "the `?` operator can only be used on `Result`s in a function that returns `Result` (this function returns `{}`)",
                        self.describe(&other)
                    );
                    self.error(t.syntax(), msg);
                }
            }
            return *x;
        }
        let value = match self.shallow(&it) {
            Ty::Option(x) => *x,
            Ty::Unknown => return Ty::Unknown,
            Ty::Var(_) if self.var_kind(&it) == Some(VarKind::General) => {
                let x = self.fresh(VarKind::General);
                self.unify(&it, &Ty::Option(Box::new(x.clone())));
                x
            }
            other => {
                let msg = format!(
                    "the `?` operator can only be applied to `Option` or `Result` (found `{}`)",
                    self.describe(&other)
                );
                self.error(t.syntax(), msg);
                return Ty::Unknown;
            }
        };
        if !matches!(self.shallow(&ret), Ty::Option(_) | Ty::Unknown) {
            let msg = format!(
                "the `?` operator can only be used in a function that returns `Option` (this function returns `{}`)",
                self.describe(&ret)
            );
            self.error(t.syntax(), msg);
        }
        value
    }

    /// A call of the function `name` (a local or imported `fn`).
    fn call_fn(
        &mut self,
        c: &ast::CallExpr,
        name: &str,
        path: &ast::Path,
        args: &[ast::Expr],
    ) -> Ty {
        let Some(sig) = self.fns.get(name) else {
            // `drop(x)` (the prelude's `std::mem::drop`).
            if name == "drop" {
                self.arg_count(c.syntax(), "function", 1, args.len());
                for a in args {
                    self.infer(a, None);
                }
                return Ty::Unit;
            }
            let callee = c
                .expr()
                .map(|e| e.syntax().clone())
                .unwrap_or_else(|| c.syntax().clone());
            self.error(
                &callee,
                format!("cannot find function `{name}` in this scope"),
            );
            for a in args {
                self.infer(a, None);
            }
            return Ty::Unknown;
        };
        let (params, ret, is_async) = (sig.params.clone(), sig.ret.clone(), sig.is_async);
        let type_params = sig.type_params.clone();
        let (params, ret) = if type_params.is_empty() {
            (params, ret)
        } else {
            let turbofish: Vec<Ty> = path
                .segment()
                .and_then(|s| s.generic_arg_list())
                .map(|l| l.generic_args().collect::<Vec<_>>())
                .unwrap_or_default()
                .iter()
                .filter_map(|g| self.generic_arg_ty(g))
                .collect();
            let inst = self.instantiate(c.syntax(), &type_params, &params, &ret, &turbofish);
            if let Some(caller) = self.cur_fn.clone() {
                let r = self.range(c.syntax());
                let targs = std::mem::take(&mut self.last_type_args);
                self.generic_calls
                    .push((r, caller, name.to_string(), targs));
            }
            inst
        };
        self.arg_count(c.syntax(), "function", params.len(), args.len());
        let cell_params = self
            .fns
            .get(name)
            .map(|s| s.cell_params.clone())
            .unwrap_or_default();
        let into_params = self
            .fns
            .get(name)
            .map(|s| s.into_params.clone())
            .unwrap_or_default();
        for (i, a) in args.iter().enumerate() {
            if cell_params.get(i) == Some(&true) {
                self.cell_arg(a);
            }
            match params.get(i) {
                Some(p) if into_params.get(i) == Some(&true) => {
                    self.check_into_arg(a, p);
                }
                Some(p) => {
                    self.check(a, p);
                }
                None => {
                    self.infer(a, None);
                }
            }
        }
        self.resolve_projections(false);
        if is_async {
            Ty::Future(Box::new(ret))
        } else {
            ret
        }
    }

    /// Values JS can't point at: they need a cell to be shared through `&mut`.
    pub(super) fn cellable(&self, t: &Ty) -> bool {
        match self.zonk(t) {
            Ty::Int(_)
            | Ty::IntLit
            | Ty::Float { .. }
            | Ty::Bool
            | Ty::Char
            | Ty::Str
            | Ty::Option(_) => true,
            Ty::Var(_) => matches!(self.var_kind(t), Some(VarKind::Int | VarKind::Float)),
            _ => false,
        }
    }

    /// `&mut x` with `x` a number/string/bool/Option local: `x` lives in a cell.
    fn box_if_needed(&mut self, inner: &ast::Expr) {
        let ast::Expr::PathExpr(p) = strip_parens(inner) else {
            return;
        };
        let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
            return;
        };
        let Some(local) = self.lookup(name.text().to_string().as_str()).cloned() else {
            return;
        };
        if local.reference.is_none()
            && self.cellable(&local.ty)
            && let Some(d) = local.decl
        {
            self.boxed.insert(d);
            let r = self.range(p.syntax());
            self.cell_args.insert(r);
        }
    }

    /// An argument for a `&mut i32`-style parameter: `&mut n`, or a `&mut` parameter passed on.
    pub(super) fn cell_arg(&mut self, a: &ast::Expr) {
        // A raw pointer argument (`&x as *const i32`, `ptr::null_mut()`, a pointer local).
        if self.is_ptr(a) {
            self.ptr_cell(a);
            return;
        }
        match strip_parens(a) {
            // (`&x` for a `*const i32` parameter too.)
            ast::Expr::RefExpr(r) => {
                let Some(inner) = r.expr() else { return };
                match strip_parens(&inner) {
                    ast::Expr::PathExpr(_) => self.box_if_needed(&inner),
                    // `&mut *x` reborrows the cell.
                    ast::Expr::PrefixExpr(pe) if pe.op_kind() == Some(UnaryOp::Deref) => {
                        if let Some(ast::Expr::PathExpr(p)) = pe.expr() {
                            let r = self.range(p.syntax());
                            self.cell_args.insert(r);
                        }
                    }
                    // `&mut node.left` / `&mut v[i]`: the slot itself, read and written through a
                    // getter/setter cell, when the place is simple enough to evaluate again.
                    other @ (ast::Expr::FieldExpr(_) | ast::Expr::IndexExpr(_)) if simple_place(&other) => {
                        let rr = self.range(r.syntax());
                        self.slot_args.insert(rr);
                    }
                    // A temporary (`&mut String::new()`, `&mut 0`): a cell of its own.
                    ast::Expr::CallExpr(_)
                    | ast::Expr::MethodCallExpr(_)
                    | ast::Expr::Literal(_)
                    | ast::Expr::MacroExpr(_)
                    | ast::Expr::BinExpr(_)
                    | ast::Expr::ArrayExpr(_)
                    | ast::Expr::TupleExpr(_) => {
                        let rr = self.range(r.syntax());
                        self.temp_cells.insert(rr);
                    }
                    other => self.error(
                        other.syntax(),
                        "passing a `&mut` to a number, string or bool inside a struct, tuple or Vec isn't supported yet (copy it into a local, pass `&mut local`, and write it back)",
                    ),
                }
            }
            ast::Expr::PathExpr(p) => {
                // A `&mut` parameter handed on: pass its cell.
                if let Some(name) = p.path().and_then(|p| p.as_single_name_ref())
                    && let Some(local) = self.lookup(name.text().to_string().as_str()).cloned()
                    && local.reference == Some(true)
                {
                    let r = self.range(p.syntax());
                    self.cell_args.insert(r);
                }
            }
            _ => {}
        }
    }

    /// `Default::default()` calls typed by a later `break` value or arm: now known, and `Default`.
    fn resolve_defaults(&mut self) {
        for (t, node) in std::mem::take(&mut self.pending_defaults) {
            match self.var_kind(&t) {
                Some(VarKind::Int) => {
                    self.unify(&t, &Ty::Int(IntKind::I32));
                }
                Some(VarKind::Float) => {
                    self.unify(&t, &Ty::Float { f32: false });
                }
                _ => {}
            }
            if matches!(self.shallow(&t), Ty::Var(_)) {
                self.error(&node, "type annotations needed for `Default::default()`");
                continue;
            }
            let ok = match self.zonk(&t) {
                Ty::Unknown => true,
                z @ Ty::Adt(..) => self.implements(&z, "Default"),
                Ty::Extern(n) => self.iface_has_default(&n),
                other => other.has_default(),
            };
            if !ok {
                let msg = format!(
                    "the trait bound `{}: Default` is not satisfied",
                    self.describe(&t)
                );
                self.error(&node, msg);
            }
        }
    }

    /// `break`, `break 'label`, `break value`: the value becomes the target `loop`'s value.
    fn break_expr(&mut self, b: &ast::BreakExpr) {
        let label = b.lifetime().map(|l| l.text().to_string());
        let target = match &label {
            Some(l) => self.loops.iter().rposition(|(x, ..)| x.as_ref() == Some(l)),
            // An unlabeled `break` leaves the innermost loop, not a labeled block.
            None => self.loops.iter().rposition(|e| !e.3),
        };
        let Some(i) = target else {
            if let Some(v) = b.expr() {
                self.infer(&v, None);
            }
            let msg = match &label {
                Some(l) => format!("use of undeclared label `{l}`"),
                None => "`break` outside of a loop".to_string(),
            };
            self.error(b.syntax(), msg);
            return;
        };
        self.loops[i].2 = true;
        match (self.loops[i].1.clone(), b.expr()) {
            (Some(t), Some(v)) => {
                self.check(&v, &t);
            }
            (Some(t), None) => {
                if !self.unify(&t, &Ty::Unit) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `()`",
                        self.describe(&t)
                    );
                    self.error(b.syntax(), msg);
                }
            }
            (None, Some(v)) => {
                self.infer(&v, None);
                self.error(b.syntax(), "`break` with value from a `while` or `for` loop (only `loop` can produce a value)");
            }
            (None, None) => {}
        }
    }

    fn some_call(&mut self, c: &ast::CallExpr, args: &[ast::Expr]) -> Ty {
        self.arg_count(c.syntax(), "enum variant", 1, args.len());
        let t = args
            .first()
            .map(|a| self.infer(a, None))
            .unwrap_or(Ty::Unknown);
        for a in args.iter().skip(1) {
            self.infer(a, None);
        }
        Ty::Option(Box::new(t))
    }

    fn method(&mut self, m: &ast::MethodCallExpr, expected: Option<&Ty>) -> Ty {
        let (Some(recv), Some(name)) = (m.receiver(), m.name_ref()) else {
            return Ty::Unknown;
        };
        let method = name.text().to_string();
        let args: Vec<ast::Expr> = m.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        if let Some(t) = self.ptr_method(m, &recv, &method, &args) {
            return t;
        }
        // `(lo..hi).contains(&x)`
        if method == "contains"
            && let ast::Expr::RangeExpr(r) = strip_parens(&recv)
        {
            // Any ordered bounds work here, floats too (only iterating needs integers).
            use ra_ap_syntax::ast::RangeItem;
            let elem = self.fresh(VarKind::General);
            for b in [r.start(), r.end()].into_iter().flatten() {
                self.check(&b, &elem);
            }
            self.record(recv.syntax(), Ty::Unknown);
            self.arg_count(m.syntax(), "method", 1, args.len());
            if let Some(a) = args.first() {
                self.check(a, &elem);
            }
            return Ty::Bool;
        }
        let mut rt = self.infer(&recv, None);
        // `stream.extend(tokens)` in a proc-macro crate: a token stream or any iterable of tokens.
        if method == "extend"
            && let Some(t) = self.token_extend(m, &recv, &rt, &args)
        {
            return t;
        }
        // `stack.push(1)` through `impl Deref`: a method the type itself doesn't have.
        if let Some((n, target)) = self.deref_target(&rt)
            && self.adts.method(&n, &method).is_none()
            && !matches!(
                method.as_str(),
                "clone"
                    | "to_owned"
                    | "eq"
                    | "ne"
                    | "cmp"
                    | "partial_cmp"
                    | "lt"
                    | "le"
                    | "gt"
                    | "ge"
                    | "deref"
                    | "deref_mut"
            )
            && !(method == "to_string" && self.adts.defs.get(&n).is_some_and(|d| d.display))
        {
            self.auto_deref(&recv, &n, &target);
            self.record(recv.syntax(), target.clone());
            rt = target;
        }
        // A user trait method on a built-in type (`x.money()` with `impl Money for f64`); std's own
        // methods come first, as inherent methods do in Rust.
        if let Some(key) = crate::adt::ext_key_of(&self.shallow(&rt))
            && self.adts.ext_types.contains(key)
            && let Some(sig) = self.adts.method(key, &method).cloned()
            && sig.receiver.is_some()
            && !crate::service::std_methods(&self.shallow(&rt)).contains(&method.as_str())
        {
            if sig.receiver == Some(crate::adt::Receiver::MutRef) {
                self.check_mutable(&recv, Mutation::Borrow);
            }
            let sig = self.method_inst(&sig, None);
            self.arg_count(m.syntax(), "method", sig.params.len(), args.len());
            for (a, p) in args.iter().zip(&sig.params) {
                self.check(a, p);
            }
            self.record_ext(m.syntax(), key);
            return if sig.is_async {
                Ty::Future(Box::new(sig.ret))
            } else {
                sig.ret
            };
        }
        // A blanket impl's method (`5.shout()` with `impl<T: Display> Shout for T`), when the
        // receiver has no method of that name itself and meets the impl's bounds.
        if let Some((key, sig)) = self.blanket_method(&rt, &method) {
            if sig.receiver == Some(crate::adt::Receiver::MutRef) {
                self.check_mutable(&recv, Mutation::Borrow);
            }
            let sig = self.method_inst(&sig, None);
            self.arg_count(m.syntax(), "method", sig.params.len(), args.len());
            for (a, p) in args.iter().zip(&sig.params) {
                self.check(a, p);
            }
            self.record_ext(m.syntax(), &key);
            return if sig.is_async {
                Ty::Future(Box::new(sig.ret))
            } else {
                sig.ret
            };
        }
        let resolved = match self.shallow(&rt) {
            // A custom iterator (`impl Iterator for Fib`): adapters and consumers work on it.
            Ty::Adt(n, targs)
                if self.adts.iter_items.contains_key(&n)
                    && self.adts.method(&n, &method).is_none()
                    && !crate::adt::own_method(&method) =>
            {
                let item = self.adts.iter_items[&n].clone();
                let map = self.adt_subst(&n, &targs);
                Ty::Iter {
                    elem: Box::new(crate::typeck::generic::subst(&item, &map)),
                    mutable: false,
                }
            }
            // A range value: `contains`, `start()` / `end()`, `is_empty`, `len`, `clone`, else
            // what an iterator does.
            Ty::Range(e, incl) => {
                match method.as_str() {
                    "contains" => {
                        self.arg_count(m.syntax(), "method", 1, args.len());
                        if let Some(a) = args.first() {
                            self.check(a, &e);
                        }
                        return Ty::Bool;
                    }
                    "start" | "end" if incl => {
                        self.arg_count(m.syntax(), "method", 0, args.len());
                        return *e;
                    }
                    "is_empty" => {
                        self.arg_count(m.syntax(), "method", 0, args.len());
                        return Ty::Bool;
                    }
                    "clone" => {
                        self.arg_count(m.syntax(), "method", 0, args.len());
                        return Ty::Range(e, incl);
                    }
                    _ => {}
                }
                Ty::Iter {
                    elem: e,
                    mutable: false,
                }
            }
            // A `VecDeque`, `BTreeMap`, ..: its own methods, else its view's.
            Ty::Coll(kind, view) => {
                if let Some(t) = self.coll_method(m, &recv, kind, &view, &method, &args) {
                    return t;
                }
                *view
            }
            other => other,
        };
        // The bundled crates' methods (`rng.gen_range(..)`, `v.shuffle(&mut rng)`).
        if let Some(t) = self.crate_method(m, &recv, &resolved, &method, &args) {
            return t;
        }

        // Infer remaining arguments without constraints, for paths that don't type them.
        let loose = |c: &mut Self| {
            for a in &args {
                c.infer(a, None);
            }
        };
        let no_method = |c: &mut Self, ty: &Ty| {
            let hint = if matches!(method.as_str(), "context" | "with_context")
                && matches!(ty, Ty::Result(..) | Ty::Option(_))
            {
                "if you meant `anyhow::Context`'s, add `use anyhow::Context;`"
            } else {
                "the std surface is still small; see docs/FEATURES.md"
            };
            let msg = format!(
                "no method named `{method}` found for `{}` ({hint})",
                c.describe(ty)
            );
            c.error(m.syntax(), msg);
            Ty::Unknown
        };

        if method == "into"
            && args.is_empty()
            && !matches!(&resolved, Ty::Adt(n, _) if self.adts.method(n, "into").is_some())
        {
            return self.conv_into_call(m, rt);
        }
        // `self.id.hash(state)` in an `impl Hash` (or with a `DefaultHasher`).
        if method == "hash"
            && args.len() == 1
            && !matches!(&resolved, Ty::Adt(n, _) if self.adts.method(n, "hash").is_some())
            && self.hasher_arg(&args[0])
        {
            self.infer(&args[0], None);
            let z = self.zonk(&rt);
            let lacks = match &z {
                Ty::Float { .. } => Some(z.describe()),
                _ => first_adt_of(&z)
                    .filter(|n| !self.implements(&Ty::Adt(n.clone(), vec![]), "Hash")),
            };
            if let Some(n) = lacks {
                self.error(
                    m.syntax(),
                    format!("the trait bound `{n}: Hash` is not satisfied"),
                );
            }
            return Ty::Unit;
        }
        if let Ty::Hasher = resolved {
            return match method.as_str() {
                "finish" => {
                    self.arg_count(m.syntax(), "method", 0, args.len());
                    Ty::Int(IntKind::U64)
                }
                "write" | "write_str" | "write_u8" | "write_u16" | "write_u32" | "write_u64"
                | "write_u128" | "write_usize" | "write_i8" | "write_i16" | "write_i32"
                | "write_i64" | "write_i128" | "write_isize" => {
                    self.arg_count(m.syntax(), "method", 1, args.len());
                    for a in &args {
                        self.infer(a, None);
                    }
                    Ty::Unit
                }
                _ => {
                    for a in &args {
                        self.infer(a, None);
                    }
                    no_method(self, &resolved)
                }
            };
        }
        if method == "try_into"
            && args.is_empty()
            && !matches!(&resolved, Ty::Adt(n, _) if self.adts.method(n, "try_into").is_some())
        {
            return self.conv_try_into_call(m, rt);
        }
        if matches!(method.as_str(), "cmp" | "partial_cmp")
            && matches!(resolved, Ty::Var(_))
            && let Some(t) = self.method_ext(m, &recv, &resolved, &method, &args, expected)
        {
            return t;
        }
        // `Ord` methods on an integer whose type isn't settled yet (`let mut b = 0; b.max(u)`):
        // the arguments decide it, as in rustc; only with none does the literal fallback apply.
        if self.var_kind(&rt) == Some(VarKind::Int)
            && matches!(method.as_str(), "max" | "min" | "clamp" | "cmp")
            && !args.is_empty()
        {
            let want = if method == "clamp" { 2 } else { 1 };
            self.arg_count(m.syntax(), "method", want, args.len());
            for a in &args {
                self.check(a, &rt);
            }
            return if method == "cmp" { Ty::Ordering } else { rt };
        }
        // Trait methods on an unsuffixed literal use the literal's fallback type, as in rustc.
        let trait_method = matches!(
            method.as_str(),
            "to_string" | "clone" | "eq" | "ne" | "partial_cmp" | "lt" | "le" | "gt" | "ge"
        ) || (self.var_kind(&rt) == Some(VarKind::Int)
            && matches!(method.as_str(), "max" | "min" | "clamp" | "cmp"));
        let resolved = match self.var_kind(&rt) {
            Some(kind @ (VarKind::Int | VarKind::Float)) if trait_method => {
                let d = if kind == VarKind::Int {
                    Ty::Int(IntKind::I32)
                } else {
                    Ty::Float { f32: false }
                };
                self.unify(&rt, &d);
                self.shallow(&rt)
            }
            _ => resolved,
        };
        if let Some(kind @ (VarKind::Int | VarKind::Float)) = self.var_kind(&rt) {
            let lit = if kind == VarKind::Int {
                "{integer}"
            } else {
                "{float}"
            };
            self.error(
                m.syntax(),
                format!("can't call method `{method}` on ambiguous numeric type `{lit}` (add a suffix like `2i32`)"),
            );
            loose(self);
            return Ty::Unknown;
        }
        if method == "clone" || method == "to_owned" {
            self.arg_count(m.syntax(), "method", 0, args.len());
            return rt;
        }

        // `sig(params, ret)` checks the arguments against `params` and returns `ret`.
        let sig = |c: &mut Self, params: &[Ty], ret: Ty| {
            c.arg_count(m.syntax(), "method", params.len(), args.len());
            for (a, p) in args.iter().zip(params) {
                c.check(a, p);
            }
            for a in args.iter().skip(params.len()) {
                c.infer(a, None);
            }
            ret
        };

        let receiver_ty = match resolved {
            Ty::Unknown | Ty::Var(_) if is_float_method(&method) => Ty::Float { f32: false },
            Ty::Extern(name) => return self.extern_method(m, &name, &method, &args),
            Ty::Adt(name, targs) => {
                return self.adt_method(m, &recv, &name, &targs, &method, &args);
            }
            Ty::Param(name) => return self.param_method(m, &recv, &name, &method, &args),
            Ty::Dyn(tr) => return self.dyn_method(m, &recv, &tr, &method, &args),
            Ty::RefCell(inner) => {
                let inner = *inner;
                return match method.as_str() {
                    "borrow" | "borrow_mut" | "get" | "into_inner" => {
                        self.arg_count(m.syntax(), "method", 0, args.len());
                        inner
                    }
                    "set" | "replace" => {
                        self.arg_count(m.syntax(), "method", 1, args.len());
                        if let Some(a) = args.first() {
                            self.check(a, &inner);
                        }
                        if method == "set" { Ty::Unit } else { inner }
                    }
                    "take" => {
                        self.arg_count(m.syntax(), "method", 0, args.len());
                        inner
                    }
                    "clone" => Ty::RefCell(Box::new(inner)),
                    _ => {
                        loose(self);
                        no_method(self, &Ty::RefCell(Box::new(inner)))
                    }
                };
            }
            ref other if !matches!(other, Ty::Unknown | Ty::Var(_)) => {
                if let Some(t) = self.method_ext(m, &recv, other, &method, &args, expected) {
                    return t;
                }
                other.clone()
            }
            Ty::Unknown | Ty::Var(_) => {
                loose(self);
                return Ty::Unknown;
            }
            other => other,
        };
        let t = receiver_ty.clone();
        match (&receiver_ty, method.as_str()) {
            (Ty::Float { .. }, m2) if is_float_method(m2) => match m2 {
                "powi" => sig(self, &[Ty::Int(IntKind::I32)], t),
                "powf" | "min" | "max" | "atan2" => sig(self, &[t.clone()], t),
                "clamp" => sig(self, &[t.clone(), t.clone()], t),
                "is_nan" | "is_finite" => sig(self, &[], Ty::Bool),
                "to_string" => sig(self, &[], Ty::Str),
                _ => sig(self, &[], t),
            },
            (Ty::Int(k), m2) => match m2 {
                "abs" | "signum" if k.is_signed() => sig(self, &[], t),
                "pow" => sig(self, &[Ty::Int(IntKind::U32)], t),
                "min" | "max" | "rem_euclid" => sig(self, &[t.clone()], t),
                "clamp" => sig(self, &[t.clone(), t.clone()], t),
                "to_string" => sig(self, &[], Ty::Str),
                _ => no_method(self, &t),
            },
            (Ty::Str, m2) => match m2 {
                "len" => sig(self, &[], USIZE),
                "is_empty" => sig(self, &[], Ty::Bool),
                "to_string" | "as_str" | "to_uppercase" | "to_lowercase" | "trim"
                | "trim_start" | "trim_end" => sig(self, &[], Ty::Str),
                "contains" | "starts_with" | "ends_with" => {
                    self.arg_count(m.syntax(), "method", 1, args.len());
                    if let Some(a) = args.first() {
                        let at = self.infer(a, None);
                        if !matches!(self.shallow(&at), Ty::Str | Ty::Char | Ty::Unknown) {
                            let msg = format!(
                                "expected a `&str` or `char` pattern, found `{}`",
                                self.describe(&at)
                            );
                            self.error(a.syntax(), msg);
                        }
                    }
                    Ty::Bool
                }
                "replace" => sig(self, &[Ty::Str, Ty::Str], Ty::Str),
                "repeat" => sig(self, &[USIZE], Ty::Str),
                "as_bytes" | "into_bytes" => {
                    sig(self, &[], Ty::Vec(Box::new(Ty::Int(IntKind::U8))))
                }
                "push_str" => {
                    self.check_mutable(&recv, Mutation::Borrow);
                    sig(self, &[Ty::Str], Ty::Unit)
                }
                "push" => {
                    self.check_mutable(&recv, Mutation::Borrow);
                    sig(self, &[Ty::Char], Ty::Unit)
                }
                // `s.extend(chars)` / `s.extend(strs)`
                "extend" => {
                    self.check_mutable(&recv, Mutation::Borrow);
                    self.arg_count(m.syntax(), "method", 1, args.len());
                    if let Some(a) = args.first() {
                        let at = self.infer(a, None);
                        let ok = self
                            .iter_elem(&at)
                            .map(|e| self.shallow(&e))
                            .is_some_and(|e| {
                                matches!(e, Ty::Char | Ty::Str | Ty::Unknown | Ty::Var(_))
                            });
                        if !ok {
                            let msg = format!(
                                "a `String` can be extended with chars or strings, not `{}`",
                                self.describe(&at)
                            );
                            self.error(a.syntax(), msg);
                        }
                    }
                    Ty::Unit
                }
                _ => no_method(self, &t),
            },
            (Ty::Bool | Ty::Char, "to_string") => sig(self, &[], Ty::Str),
            (Ty::IntoIter(elem), "into_iter") => {
                let elem = (**elem).clone();
                sig(
                    self,
                    &[],
                    Ty::Iter {
                        elem: Box::new(elem),
                        mutable: false,
                    },
                )
            }
            (Ty::Vec(elem), m2) => {
                let elem = (**elem).clone();
                let mutating = matches!(
                    m2,
                    "pop"
                        | "push"
                        | "insert"
                        | "remove"
                        | "swap"
                        | "reverse"
                        | "sort"
                        | "sort_unstable"
                        | "clear"
                        | "truncate"
                        | "extend_from_slice"
                        | "iter_mut"
                );
                if mutating {
                    self.check_mutable(&recv, Mutation::Borrow);
                }
                match m2 {
                    "len" => sig(self, &[], USIZE),
                    "is_empty" => sig(self, &[], Ty::Bool),
                    "push" => sig(self, &[elem], Ty::Unit),
                    "insert" => sig(self, &[USIZE, elem], Ty::Unit),
                    "remove" => sig(self, &[USIZE], elem),
                    "swap" => sig(self, &[USIZE, USIZE], Ty::Unit),
                    "truncate" => sig(self, &[USIZE], Ty::Unit),
                    "reverse" | "clear" => sig(self, &[], Ty::Unit),
                    "sort" | "sort_unstable" => {
                        if self.zonk(&elem).contains_float()
                            || self.var_kind(&elem) == Some(VarKind::Float)
                        {
                            self.error(
                                m.syntax(),
                                format!("the trait bound `{}: Ord` is not satisfied (floats can't be sorted with `sort`; `sort_by` is coming with closures)", self.describe(&elem)),
                            );
                        }
                        sig(self, &[], Ty::Unit)
                    }
                    "contains" => sig(self, &[elem], Ty::Bool),
                    "extend_from_slice" => sig(self, &[Ty::Vec(Box::new(elem))], Ty::Unit),
                    // `[[1, 2], [3]].concat()`: the inner Vecs one after another.
                    "concat" if matches!(self.shallow(&elem), Ty::Vec(_)) => sig(self, &[], elem),
                    "join" | "concat" => {
                        if !self.unify(&elem, &Ty::Str) {
                            let msg = format!(
                                "`{m2}` needs a `Vec<String>`, found `Vec<{}>`",
                                self.describe(&elem)
                            );
                            self.error(m.syntax(), msg);
                        }
                        let params = if m2 == "join" { vec![Ty::Str] } else { vec![] };
                        sig(self, &params, Ty::Str)
                    }
                    "to_vec" => sig(self, &[], t),
                    "iter" | "into_iter" => sig(
                        self,
                        &[],
                        Ty::Iter {
                            elem: Box::new(elem),
                            mutable: false,
                        },
                    ),
                    "iter_mut" => sig(
                        self,
                        &[],
                        Ty::Iter {
                            elem: Box::new(elem),
                            mutable: true,
                        },
                    ),
                    "pop" | "first" | "last" => sig(self, &[], Ty::Option(Box::new(elem))),
                    "get" => sig(self, &[USIZE], Ty::Option(Box::new(elem))),
                    // The element itself, which writes go through when it's a JS object (a struct,
                    // tuple, array or collection); numbers, strings and bools would be copies.
                    "get_mut" | "first_mut" | "last_mut" => {
                        self.check_mutable(&recv, Mutation::Borrow);
                        let resolved = self.zonk(&elem);
                        if (resolved.is_js_primitive() || resolved == Ty::Unknown)
                            && slot_position(m)
                        {
                            let r = self.range(m.syntax());
                            self.slot_calls.insert(r);
                            if let Some(n) = slot_let_binding(m) {
                                let r = self.range(n.syntax());
                                self.slot_bindings.insert(r);
                            }
                        } else if !only_tested(m) && resolved.is_js_primitive()
                            || resolved == Ty::Unknown
                        {
                            self.error(
                                m.syntax(),
                                format!(
                                    "`Vec::{m2}` on a Vec of `{}` isn't supported here (the element would be copied, so changes wouldn't stick); use it in `if let Some(x) = ..` / `match`, as `*v.{m2}(..).unwrap()`, or index with `v[i]`",
                                    self.describe(&elem)
                                ),
                            );
                        }
                        let params = if m2 == "get_mut" { vec![USIZE] } else { vec![] };
                        sig(self, &params, Ty::Option(Box::new(elem)))
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Option(inner), m2) => {
                let inner = (**inner).clone();
                match m2 {
                    "is_some" | "is_none" => sig(self, &[], Ty::Bool),
                    // `weak.upgrade()` (a `Weak<T>` is an `Option<T>` here)
                    "upgrade" => sig(self, &[], Ty::Option(Box::new(inner))),
                    "unwrap" => sig(self, &[], inner),
                    "expect" => sig(self, &[Ty::Str], inner),
                    "unwrap_or" => sig(self, &[inner.clone()], inner),
                    "unwrap_or_default" => {
                        let resolved = self.zonk(&inner);
                        if !resolved.has_default() {
                            let msg = format!(
                                "the trait bound `{}: Default` is not satisfied",
                                self.describe(&inner)
                            );
                            self.error(m.syntax(), msg);
                        }
                        sig(self, &[], inner)
                    }
                    "take" => {
                        self.check_mutable(&recv, Mutation::Borrow);
                        sig(self, &[], t)
                    }
                    "replace" => {
                        self.check_mutable(&recv, Mutation::Borrow);
                        sig(self, &[inner], t)
                    }
                    "as_ref" | "as_mut" | "as_deref" | "cloned" | "copied" => sig(self, &[], t),
                    // Zero or one item.
                    "iter" | "into_iter" | "iter_mut" => sig(
                        self,
                        &[],
                        Ty::Iter {
                            elem: Box::new(inner),
                            mutable: m2 == "iter_mut",
                        },
                    ),
                    // `Option<Option<T>>` -> `Option<T>`: the nested option before it is allowed
                    // (`None` and `Some(None)` are both `null`, which is what `flatten` gives).
                    "flatten" => {
                        let inner_opt = Ty::Option(Box::new(self.fresh(VarKind::General)));
                        self.unify(&inner, &inner_opt);
                        let mut r = Some(strip_parens(&recv));
                        while let Some(e) = r {
                            let range = self.range(e.syntax());
                            self.nested_ok.insert(range);
                            r = match e {
                                ast::Expr::MethodCallExpr(mc) => {
                                    mc.receiver().map(|x| strip_parens(&x))
                                }
                                _ => None,
                            };
                        }
                        sig(self, &[], inner_opt)
                    }
                    // `Option<Result<T, E>>` -> `Result<Option<T>, E>`.
                    "transpose" => {
                        let (x, e) = (self.fresh(VarKind::General), self.fresh(VarKind::General));
                        if !self.unify(
                            &inner,
                            &Ty::Result(Box::new(x.clone()), Box::new(e.clone())),
                        ) {
                            return no_method(self, &t);
                        }
                        sig(
                            self,
                            &[],
                            Ty::Result(Box::new(Ty::Option(Box::new(x))), Box::new(e)),
                        )
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Map(k, v), m2) => {
                let (k, v) = ((**k).clone(), (**v).clone());
                if matches!(
                    m2,
                    "insert" | "remove" | "clear" | "entry" | "get_mut" | "values_mut" | "iter_mut"
                ) {
                    self.check_mutable(&recv, Mutation::Borrow);
                }
                let opt = |t: Ty| Ty::Option(Box::new(t));
                let iter = |elem: Ty, mutable: bool| Ty::Iter {
                    elem: Box::new(elem),
                    mutable,
                };
                match m2 {
                    "insert" => sig(self, &[k, v.clone()], opt(v)),
                    "get" | "remove" => sig(self, &[k], opt(v)),
                    "contains_key" => sig(self, &[k], Ty::Bool),
                    "get_mut" => {
                        let resolved = self.zonk(&v);
                        if resolved.is_js_primitive() && slot_position(m) {
                            let r = self.range(m.syntax());
                            self.slot_calls.insert(r);
                            if let Some(n) = slot_let_binding(m) {
                                let r = self.range(n.syntax());
                                self.slot_bindings.insert(r);
                            }
                        } else if !only_tested(m) && resolved.is_js_primitive() {
                            let msg = format!(
                                "`HashMap::get_mut` on a map of `{}` values isn't supported here (the value would be copied, so changes wouldn't stick); use it in `if let Some(x) = ..` / `match`, as `*map.get_mut(..).unwrap()`, or `*map.entry(k).or_insert(..) += ..`",
                                self.describe(&v)
                            );
                            self.error(m.syntax(), msg);
                        }
                        sig(self, &[k], opt(v))
                    }
                    "len" => sig(self, &[], USIZE),
                    "is_empty" => sig(self, &[], Ty::Bool),
                    "clear" => sig(self, &[], Ty::Unit),
                    // A chained `.or_insert(..)` & co. use the entry in place; otherwise it's the
                    // prelude's `Entry` (`Entry::Occupied(e)`), when the program names it.
                    "entry"
                        if self.adts.defs.contains_key("StdMapEntry")
                            && !crate::typeck::entry_chained(m) =>
                    {
                        sig(
                            self,
                            &[k.clone()],
                            Ty::Adt("StdMapEntry".into(), vec![k, v]),
                        )
                    }
                    "entry" => sig(self, &[k.clone()], Ty::Entry(Box::new(k), Box::new(v))),
                    "keys" | "into_keys" => sig(self, &[], iter(k, false)),
                    "values" | "into_values" => sig(self, &[], iter(v, false)),
                    "values_mut" => sig(self, &[], iter(v, true)),
                    "iter" | "into_iter" => sig(self, &[], iter(Ty::Tuple(vec![k, v]), false)),
                    "iter_mut" => sig(self, &[], iter(Ty::Tuple(vec![k, v]), true)),
                    "retain" | "extend" | "drain" | "and_modify" => {
                        self.error(
                            m.syntax(),
                            format!("`HashMap::{m2}` needs iterators or closures, which are not supported yet; loop and `insert` instead"),
                        );
                        loose(self);
                        Ty::Unknown
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Entry(_, v), m2) => {
                let v = (**v).clone();
                match m2 {
                    "or_insert" => sig(self, &[v.clone()], v),
                    "or_default" => {
                        let resolved = self.zonk(&v);
                        // (A struct or enum with `#[derive(Default)]` or `impl Default`.)
                        let ok = match &resolved {
                            Ty::Adt(..) => self.implements(&resolved, "Default"),
                            Ty::Extern(n) => self.iface_has_default(n),
                            other => other.has_default(),
                        };
                        if !ok {
                            let msg = format!(
                                "the trait bound `{}: Default` is not satisfied",
                                self.describe(&v)
                            );
                            self.error(m.syntax(), msg);
                        }
                        sig(self, &[], v)
                    }
                    "or_insert_with" => {
                        // Closures don't exist yet; constructor paths like `Vec::new` work.
                        let made = match args.first() {
                            Some(ast::Expr::PathExpr(p)) => {
                                let text = p.syntax().text().to_string();
                                match std_tail(&text.split("::").collect::<Vec<_>>()) {
                                    [.., "Vec", "new"] => {
                                        Some(Ty::Vec(Box::new(self.fresh(VarKind::General))))
                                    }
                                    [.., "String", "new"] => Some(Ty::Str),
                                    [.., "HashSet", "new"] => {
                                        Some(Ty::Set(Box::new(self.fresh(VarKind::General))))
                                    }
                                    [.., "HashMap", "new"] => Some(Ty::Map(
                                        Box::new(self.fresh(VarKind::General)),
                                        Box::new(self.fresh(VarKind::General)),
                                    )),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };
                        match (made, args.first()) {
                            (Some(made), Some(a)) => {
                                self.record(a.syntax(), made.clone());
                                if !self.unify(&made, &v) {
                                    let msg = format!(
                                        "mismatched types: expected `{}`, found `{}`",
                                        self.describe(&v),
                                        self.describe(&made)
                                    );
                                    self.error(a.syntax(), msg);
                                }
                            }
                            _ => {
                                // A function value: `fn() -> V`.
                                if let Some(a) = args.first() {
                                    let want = Ty::Closure {
                                        params: vec![],
                                        ret: Box::new(v.clone()),
                                    };
                                    self.check(a, &want);
                                }
                            }
                        }
                        v
                    }
                    "and_modify" | "or_insert_with_key" => {
                        self.error(
                            m.syntax(),
                            format!(
                                "`Entry::{m2}` takes a closure, and closures are not supported yet"
                            ),
                        );
                        loose(self);
                        Ty::Unknown
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Set(elem), m2) => {
                let elem = (**elem).clone();
                if matches!(m2, "insert" | "remove" | "clear") {
                    self.check_mutable(&recv, Mutation::Borrow);
                }
                match m2 {
                    "insert" | "contains" | "remove" => sig(self, &[elem], Ty::Bool),
                    "len" => sig(self, &[], USIZE),
                    "is_empty" => sig(self, &[], Ty::Bool),
                    "clear" => sig(self, &[], Ty::Unit),
                    "is_subset" | "is_superset" | "is_disjoint" => {
                        sig(self, &[t.clone()], Ty::Bool)
                    }
                    "iter" | "into_iter" => sig(
                        self,
                        &[],
                        Ty::Iter {
                            elem: Box::new(elem),
                            mutable: false,
                        },
                    ),
                    "union"
                    | "intersection"
                    | "difference"
                    | "symmetric_difference"
                    | "retain"
                    | "extend" => {
                        self.error(
                            m.syntax(),
                            format!("`HashSet::{m2}` needs iterators or closures, which are not supported yet; loop and `insert` instead"),
                        );
                        loose(self);
                        Ty::Unknown
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Iter { elem, mutable }, "enumerate") => {
                let item = Ty::Tuple(vec![USIZE, (**elem).clone()]);
                sig(
                    self,
                    &[],
                    Ty::Iter {
                        elem: Box::new(item),
                        mutable: *mutable,
                    },
                )
            }
            (Ty::Iter { .. }, _) if crates::ITERTOOLS_METHODS.contains(&method.as_str()) => {
                self.error(
                    m.syntax(),
                    format!("no method named `{method}` found for iterator `{}` (it's an `Itertools` method: add `use itertools::Itertools;`)", self.describe(&t)),
                );
                loose(self);
                Ty::Unknown
            }
            (Ty::Iter { .. }, _) => {
                self.error(
                    m.syntax(),
                    format!("iterator adapters like `.{method}()` are not supported yet (planned for phase 1); use a `for` loop"),
                );
                loose(self);
                Ty::Unknown
            }
            _ => no_method(self, &t),
        }
    }

    fn index(&mut self, ix: &ast::IndexExpr) -> Ty {
        let (Some(base), Some(index)) = (ix.base(), ix.index()) else {
            return Ty::Unknown;
        };
        let mut bt = self.infer(&base, None);
        // `stack[i]` through `impl Deref` (when the type has no `Index` of its own).
        if let Some((n, target)) = self.deref_target(&bt)
            && !self
                .adts
                .op_impls
                .get(&n)
                .is_some_and(|v| v.iter().any(|i| i.trait_name == "Index"))
        {
            self.auto_deref(&base, &n, &target);
            self.record(base.syntax(), target.clone());
            bt = target;
        }
        // `grid[i]` with an `impl Index<I> for Grid`.
        if let Ty::Adt(n, _) = self.shallow(&bt) {
            return self.index_overload(ix, &n, &bt, &index);
        }
        // `deque[i]`, `btree_map[&k]`
        let bt = match self.shallow(&bt) {
            Ty::Coll(crate::ty::Coll::VecDeque | crate::ty::Coll::BTreeMap, view) => *view,
            other => other,
        };
        if let Ty::Map(k, v) = self.shallow(&bt) {
            self.check(&index, &k);
            return *v;
        }
        if let ast::Expr::RangeExpr(r) = &index {
            // `v[a..b]` / `s[a..b]`: a copy of that part (byte offsets for strings).
            for bound in [r.start(), r.end()].into_iter().flatten() {
                self.check(&bound, &USIZE);
            }
            self.record(index.syntax(), Ty::Unknown);
            return match self.shallow(&bt) {
                Ty::Vec(e) => Ty::Vec(e),
                Ty::Str => Ty::Str,
                Ty::Unknown | Ty::Var(_) => Ty::Unknown,
                other => {
                    let msg = format!("cannot slice a value of type `{}`", self.describe(&other));
                    self.error(ix.syntax(), msg);
                    Ty::Unknown
                }
            };
        }
        let it = self.infer(&index, Some(&USIZE));
        // `v[r]` / `&s[r]` with a `Range<usize>` value.
        if let Ty::Range(e, _) = self.shallow(&it) {
            self.unify(&e, &USIZE);
            return match self.shallow(&bt) {
                Ty::Vec(e) => Ty::Vec(e),
                Ty::Str => Ty::Str,
                Ty::Unknown | Ty::Var(_) => Ty::Unknown,
                other => {
                    let msg = format!("cannot slice a value of type `{}`", self.describe(&other));
                    self.error(ix.syntax(), msg);
                    Ty::Unknown
                }
            };
        }
        let resolved = self.shallow(&bt);
        if !self.unify(&it, &USIZE) {
            let container = match &resolved {
                Ty::Vec(e) => format!("[{}]", self.describe(e)),
                other => self.describe(other),
            };
            let msg = format!(
                "the type `{container}` cannot be indexed by `{}` (indices must be `usize`)",
                self.describe(&it)
            );
            self.error(index.syntax(), msg);
        }
        match resolved {
            Ty::Vec(e) => *e,
            Ty::Unknown | Ty::Var(_) => Ty::Unknown,
            Ty::Str => {
                self.error(
                    ix.syntax(),
                    "strings cannot be indexed by integers (Rust strings are UTF-8)",
                );
                Ty::Unknown
            }
            other => {
                let msg = format!(
                    "cannot index into a value of type `{}`",
                    self.describe(&other)
                );
                self.error(ix.syntax(), msg);
                Ty::Unknown
            }
        }
    }

    /// An `if` / `while` condition, including let-chains (`let Some(x) = a && x > 1`): each
    /// `let`'s bindings are visible to the parts after it and to the body.
    fn condition(&mut self, c: &ast::Expr) {
        match c {
            ast::Expr::LetExpr(le) => self.let_condition(le),
            _ if is_let_chain(c) => {
                for part in chain_parts(c) {
                    self.condition(&part);
                }
                self.record(c.syntax(), Ty::Bool);
            }
            _ => {
                self.check(c, &Ty::Bool);
            }
        }
    }

    fn if_expr(&mut self, i: &ast::IfExpr, expected: Option<&Ty>) -> Ty {
        // An `if let`'s bindings are visible in the `then` branch only.
        self.scopes.push(HashMap::new());
        if let Some(c) = i.condition() {
            self.condition(&c);
        }
        let then_ty = match i.then_branch() {
            Some(b) => self.block(&b, expected),
            None => Ty::Unit,
        };
        self.scopes.pop();
        let (else_ty, else_node) = match i.else_branch() {
            Some(ElseBranch::Block(b)) => (self.block(&b, Some(&then_ty)), b.syntax().clone()),
            Some(ElseBranch::IfExpr(ei)) => {
                let t = self.infer(&ast::Expr::IfExpr(ei.clone()), Some(&then_ty));
                (t, ei.syntax().clone())
            }
            None => return Ty::Unit,
        };
        if !self.unify(&then_ty, &else_ty) {
            let msg = format!(
                "`if` and `else` have incompatible types: expected `{}`, found `{}`",
                self.describe(&then_ty),
                self.describe(&else_ty)
            );
            self.error(&else_node, msg);
        }
        if self.shallow(&then_ty) == Ty::Never {
            else_ty
        } else {
            then_ty
        }
    }

    fn for_expr(&mut self, f: &ast::ForExpr) {
        let Some(iterable) = f.iterable() else { return };
        // `for x in &v` / `v.iter()`: the loop borrows `v` for its whole body.
        if let Some(body) = f.loop_body()
            && let Some((of, mutable)) = self.borrowed_local(&iterable)
        {
            let r = self.range(body.syntax());
            self.borrow_events.loops.push((of, mutable, r));
        }
        // (element type, reference kind of the loop variable)
        let (elem, reference) = match &iterable {
            ast::Expr::RangeExpr(r) => (self.range_elem(r), None),
            ast::Expr::MethodCallExpr(m)
                if m.name_ref().is_some_and(|n| n.text() == "rev")
                    && matches!(
                        m.receiver().map(|r| strip_parens(&r)),
                        Some(ast::Expr::RangeExpr(_))
                    ) =>
            {
                let Some(ast::Expr::RangeExpr(r)) = m.receiver().map(|r| strip_parens(&r)) else {
                    unreachable!()
                };
                let t = self.range_elem(&r);
                self.record(m.syntax(), Ty::Unknown);
                (t, None)
            }
            other => {
                let t = self.infer(other, None);
                let by_ref = match other {
                    ast::Expr::RefExpr(r) => Some(r.mut_token().is_some()),
                    _ => None,
                };
                let t = match self.shallow(&t) {
                    Ty::Coll(_, view) => *view,
                    // `for x in Fib { .. }`
                    Ty::Adt(n, targs) if self.adts.iter_items.contains_key(&n) => {
                        let map = self.adt_subst(&n, &targs);
                        Ty::Iter {
                            elem: Box::new(crate::typeck::generic::subst(
                                &self.adts.iter_items[&n],
                                &map,
                            )),
                            mutable: false,
                        }
                    }
                    other => other,
                };
                match self.shallow(&t) {
                    Ty::Vec(e) | Ty::Set(e) => (*e, by_ref),
                    Ty::Map(k, v) => (Ty::Tuple(vec![*k, *v]), by_ref),
                    Ty::Iter { elem, mutable } => (*elem, Some(mutable)),
                    Ty::Range(e, _) => (*e, Some(false)),
                    Ty::IntoIter(e) => (*e, None),
                    Ty::Unknown | Ty::Var(_) => (Ty::Unknown, None),
                    other_ty => {
                        let msg = format!("`{}` is not an iterator", self.describe(&other_ty));
                        self.error(iterable.syntax(), msg);
                        (Ty::Unknown, None)
                    }
                }
            }
        };
        self.scopes.push(HashMap::new());
        if let Some(pat) = f.pat() {
            if !self.irrefutable_pat(&pat) {
                self.error(pat.syntax(), "refutable pattern in `for` loop binding");
            }
            self.pat(&pat, &elem);
            // Loop variables over `&v` / `&mut v` are references.
            if let Some(scope) = self.scopes.last_mut() {
                for local in scope.values_mut() {
                    local.reference = reference;
                }
            }
        }
        if let Some(body) = f.loop_body() {
            self.block(&body, None);
        }
        self.scopes.pop();
    }

    fn range_elem(&mut self, r: &ast::RangeExpr) -> Ty {
        let lo = r.start().map(|e| self.infer(&e, None));
        let hi = match (r.end(), &lo) {
            (Some(e), Some(l)) => {
                let l = l.clone();
                Some(self.check(&e, &l))
            }
            (Some(e), None) => Some(self.infer(&e, None)),
            (None, _) => None,
        };
        self.record(r.syntax(), Ty::Unknown);
        let t = lo.or(hi).unwrap_or(Ty::Unknown);
        if !self.is_integer(&t) && self.shallow(&t) != Ty::Char {
            let msg = format!(
                "`{}` ranges can't be iterated (only integer ranges)",
                self.describe(&t)
            );
            self.error(r.syntax(), msg);
        }
        t
    }

    // ------------------------------------------------------------ macros

    fn macro_arg(&mut self, arg: &MacroArg, expected: Option<&Ty>) -> Ty {
        let (expr, errors) = macros::parse_arg(arg);
        for (start, end, message) in errors {
            self.diags.push(RawDiag {
                start,
                end,
                message,
                severity: "error",
            });
        }
        let Some(expr) = expr else { return Ty::Unknown };
        let saved = std::mem::replace(&mut self.base, arg.start);
        let t = match expected {
            Some(e) => self.check(&expr, e),
            None => self.infer(&expr, None),
        };
        self.base = saved;
        t
    }

    fn macro_call(&mut self, mc: &ast::MacroCall, expected: Option<&Ty>) -> Ty {
        let Some(name) = macros::macro_name(mc) else {
            return Ty::Unknown;
        };
        if name == "vec" {
            let elem = match expected.map(|t| self.shallow(t)) {
                Some(Ty::Vec(e)) => *e,
                _ => self.fresh(VarKind::General),
            };
            if let Some((value, count)) = macros::split_repeat(self.src, self.base, mc) {
                self.macro_arg(&value, Some(&elem));
                self.macro_arg(&count, Some(&USIZE));
            } else {
                for a in macros::split_args(self.src, self.base, mc) {
                    self.macro_arg(&a, Some(&elem));
                }
            }
            return Ty::Vec(Box::new(elem));
        }
        if matches!(name.as_str(), "quote" | "quote_spanned") {
            return self.quote_macro(mc);
        }
        // `compile_error!("..")` (a proc macro's `Error::to_compile_error()`): the message.
        if name == "compile_error" {
            // (Only when it's compiled: not under a `#[cfg(..)]` that's off.)
            if macros::under_false_cfg(mc.syntax()) {
                return Ty::Never;
            }
            let args = macros::split_args(self.src, self.base, mc);
            let msg = args
                .first()
                .and_then(macros::format_string)
                .unwrap_or_else(|| "compile_error! invoked".into());
            self.error(mc.syntax(), msg);
            return Ty::Never;
        }
        // `debug_assert!` / `debug_assert_eq!` / `debug_assert_ne!`: checked like `assert!`.
        let name = match name.strip_prefix("debug_") {
            Some(n @ ("assert" | "assert_eq" | "assert_ne")) => n.to_string(),
            _ => name,
        };
        if name == "stringify" {
            return Ty::Str;
        }
        if name == "cfg" {
            if let Err(e) = macros::eval_cfg(mc, false) {
                self.error(mc.syntax(), e);
            }
            return Ty::Bool;
        }
        let args = macros::split_args(self.src, self.base, mc);
        match name.as_str() {
            // `format_ident!("get_{}", name)`: an `Ident` (in a proc-macro crate).
            "format_ident" if self.adts.defs.contains_key("Ident") => {
                // (`span = ..` places the identifier; spans aren't tracked.)
                let args: Vec<_> = args
                    .into_iter()
                    .filter(|a| a.name.as_deref() != Some("span"))
                    .collect();
                if !args.is_empty() {
                    self.format_args(&args);
                }
                Ty::Adt("Ident".into(), vec![])
            }
            "println" | "print" | "eprintln" | "eprint" | "format" | "panic" | "unreachable"
            | "todo" | "unimplemented" => {
                if !args.is_empty() {
                    self.format_args(&args);
                }
                match name.as_str() {
                    "format" => Ty::Str,
                    "println" | "print" | "eprintln" | "eprint" => Ty::Unit,
                    _ => Ty::Never,
                }
            }
            "assert" => {
                if let Some(cond) = args.first() {
                    self.macro_arg(cond, Some(&Ty::Bool));
                }
                if args.len() > 1 {
                    self.format_args(&args[1..]);
                }
                Ty::Unit
            }
            "write" | "writeln" => {
                // `write!(f, ..)` into a `Formatter` (in a `Display` impl) or a `String`.
                let fmt_result = Ty::Result(
                    Box::new(Ty::Unit),
                    Box::new(Ty::Extern("fmt::Error".into())),
                );
                let Some(target) = args.first() else {
                    self.error(
                        mc.syntax(),
                        format!("`{name}!` needs a destination: `{name}!(f, \"..\")`"),
                    );
                    return fmt_result;
                };
                let tt = self.macro_arg(target, None);
                // Into an `io::Write` (`stdout()`, a `BufWriter`, your own writer): its
                // `write_all` of the text, an `io::Result<()>`.
                if self.adts.traits.contains_key("IoWrite")
                    && self.implements_user_trait(&tt, "IoWrite")
                {
                    if args.len() > 1 {
                        self.format_args(&args[1..]);
                    }
                    self.io_writes.insert(target.start);
                    return Ty::Result(
                        Box::new(Ty::Unit),
                        Box::new(Ty::Adt("IoError".into(), vec![])),
                    );
                }
                match self.shallow(&tt) {
                    Ty::Formatter | Ty::Unknown => {}
                    Ty::Str => {
                        if let (Some(e), _) = macros::parse_arg(target) {
                            let saved = std::mem::replace(&mut self.base, target.start);
                            self.check_mutable(&e, Mutation::Borrow);
                            self.base = saved;
                        }
                    }
                    other => {
                        let msg = format!(
                            "cannot write into a value of type `{}` (`write!` takes a `Formatter` or a `String`)",
                            self.describe(&other)
                        );
                        self.diags.push(RawDiag {
                            start: target.start,
                            end: target.end(),
                            message: msg,
                            severity: "error",
                        });
                    }
                }
                if args.len() > 1 {
                    self.format_args(&args[1..]);
                }
                fmt_result
            }
            "matches" => match macros::matches_as_match(&args) {
                Some(m) => self.macro_arg(&m, Some(&Ty::Bool)),
                None => {
                    self.error(
                        mc.syntax(),
                        "`matches!` takes a value and a pattern: `matches!(x, Some(_))`",
                    );
                    Ty::Bool
                }
            },
            "assert_eq" | "assert_ne" => {
                if args.len() >= 2 {
                    let l = self.macro_arg(&args[0], None);
                    let r = self.macro_arg(&args[1], Some(&l));
                    let _ = r;
                }
                if args.len() > 2 {
                    self.format_args(&args[2..]);
                }
                Ty::Unit
            }
            _ => Ty::Unknown,
        }
    }

    /// The type of the format argument `arg` refers to (`{}` takes the next positional one).
    fn format_arg_ty(
        &mut self,
        arg: &ArgRef,
        next: &mut usize,
        tys: &[Ty],
        args: &[MacroArg],
    ) -> Option<Ty> {
        match arg {
            ArgRef::Next => {
                *next += 1;
                tys.get(*next - 1).cloned()
            }
            ArgRef::Index(i) => tys.get(*i).cloned(),
            ArgRef::Name(n) => match args[1..]
                .iter()
                .position(|a| a.name.as_deref() == Some(n.as_str()))
            {
                Some(i) => tys.get(i).cloned(),
                // `{name}` with no `name = ...` argument captures a local (or a `const`).
                None => match self.lookup(n) {
                    Some(l) => Some(l.ty.clone()),
                    None if self.consts.contains_key(n.as_str()) => {
                        self.consts.get(n.as_str()).cloned()
                    }
                    None => {
                        let (start, end) = (args[0].start, args[0].end());
                        let message = format!("cannot find value `{n}` in this scope");
                        self.diags.push(RawDiag {
                            start,
                            end,
                            message,
                            severity: "error",
                        });
                        None
                    }
                },
            },
        }
    }

    /// Type the arguments of a format macro and check each placeholder can format its value.
    fn format_args(&mut self, args: &[MacroArg]) {
        let tys: Vec<Ty> = args[1..].iter().map(|a| self.macro_arg(a, None)).collect();
        let Some(fmt_str) = macros::format_string(&args[0]) else {
            return;
        };
        let Ok(pieces) = fmt::parse(&fmt_str) else {
            return;
        };
        let mut next = 0;
        for p in pieces {
            let Piece::Arg { arg, spec } = p else {
                continue;
            };
            // `{:.*}` takes the precision before the value; `width$` / `prec$` name a `usize`.
            let counts: Vec<ArgRef> = spec
                .precision_arg
                .iter()
                .chain(spec.width_arg.iter())
                .cloned()
                .collect();
            for count in counts {
                if let Some(t) = self.format_arg_ty(&count, &mut next, &tys, args)
                    && !self.unify(&t, &USIZE)
                {
                    let (start, end) = (args[0].start, args[0].end());
                    let message = format!(
                        "mismatched types: a width or precision must be `usize`, found `{}`",
                        self.describe(&t)
                    );
                    self.diags.push(RawDiag {
                        start,
                        end,
                        message,
                        severity: "error",
                    });
                }
            }
            let ty = self.format_arg_ty(&arg, &mut next, &tys, args);
            let Some(ty) = ty else { continue };
            let resolved = self.shallow(&ty);
            if spec.debug {
                let mut ps = Vec::new();
                Self::params_in(&self.zonk(&ty), &mut ps);
                for p in ps {
                    self.note_dbg(&p);
                }
            }
            // `{:x}` / `{:b}` / `{:o}` need an integer, `{:e}` a number.
            if let Some(k) = spec.kind {
                let ok = match k {
                    'e' | 'E' => {
                        matches!(resolved, Ty::Int(_) | Ty::Float { .. } | Ty::Unknown)
                            || self.var_kind(&ty).is_some_and(|k| k != VarKind::General)
                    }
                    _ => {
                        matches!(resolved, Ty::Int(_) | Ty::Unknown)
                            || self.var_kind(&ty) == Some(VarKind::Int)
                    }
                };
                if !ok {
                    let tr = match k {
                        'x' => "LowerHex",
                        'X' => "UpperHex",
                        'o' => "Octal",
                        'b' => "Binary",
                        'e' => "LowerExp",
                        _ => "UpperExp",
                    };
                    let (start, end) = (args[0].start, args[0].end());
                    let message = format!(
                        "`{}` doesn't implement `std::fmt::{tr}`",
                        self.describe(&ty)
                    );
                    self.diags.push(RawDiag {
                        start,
                        end,
                        message,
                        severity: "error",
                    });
                }
                continue;
            }
            let bad = self.lacks_fmt(&resolved, spec.debug);
            if bad {
                let trait_name = if spec.debug { "Debug" } else { "Display" };
                let hint = if matches!(resolved, Ty::Vec(_)) {
                    " (use `{:?}`)"
                } else {
                    ""
                };
                let message = format!(
                    "`{}` doesn't implement `std::fmt::{trait_name}`{hint}",
                    self.describe(&ty)
                );
                let (start, end) = (args[0].start, args[0].end());
                self.diags.push(RawDiag {
                    start,
                    end,
                    message,
                    severity: "error",
                });
            }
        }
    }
}

impl Checker<'_> {
    /// Whether a value of type `resolved` can't be printed with `{:?}` (`debug`) or `{}`.
    pub(super) fn lacks_fmt(&self, resolved: &Ty, debug: bool) -> bool {
        match &resolved {
            Ty::Vec(_)
            | Ty::Option(_)
            | Ty::Unit
            | Ty::Tuple(_)
            | Ty::Set(_)
            | Ty::Map(..)
            | Ty::Coll(..) => !debug,
            Ty::Entry(..) | Ty::Closure { .. } => true,
            Ty::Ordering | Ty::Result(..) => !debug,
            Ty::Formatter => true,
            Ty::RefCell(_) => !debug,
            Ty::Param(n) => !self.param_has(n, if debug { "Debug" } else { "Display" }),
            Ty::Dyn(tr) if tr == "Error" => false,
            Ty::Dyn(tr) => !self.trait_includes(tr, if debug { "Debug" } else { "Display" }),
            // (A JS type prints with `{}` when it declares `to_string`: `Date`, `Url`, ..)
            Ty::Extern(n)
                if is_std_error(n)
                    || n == "anyhow::Error"
                    // (`{}` of a `JsValue` is JS's `String(v)`: an extension, as it was untyped.)
                    || n == "JsValue"
                    || (bindings::is_js_type(n)
                        && self
                            .bindings
                            .types
                            .get(n)
                            .is_some_and(|t| t.methods.contains_key("to_string"))) =>
            {
                false
            }
            Ty::Adt(n, _) => match self.adts.defs.get(n) {
                Some(d) if debug => !d.implements("Debug"),
                Some(d) => !d.display,
                None => false,
            },
            // `{}` of `c.to_uppercase()`: its chars.
            Ty::Iter { elem, .. } if !debug && self.shallow(elem) == Ty::Char => false,
            Ty::Iter { .. } | Ty::Future(_) | Ty::Fn => true,
            Ty::Range(..) => !debug,
            Ty::Extern(_) => !debug,
            _ => false,
        }
    }

    /// `.field("x", &v)` and friends: `v` must be `Debug`; a type parameter's formatter is
    /// passed in.
    pub(super) fn check_debug_value(&mut self, node: &SyntaxNode, ty: &Ty) {
        let mut ps = Vec::new();
        Self::params_in(&self.zonk(ty), &mut ps);
        for p in ps {
            self.note_dbg(&p);
        }
        let resolved = self.shallow(ty);
        if self.lacks_fmt(&resolved, true) {
            let msg = format!(
                "`{}` doesn't implement `std::fmt::Debug`",
                self.describe(ty)
            );
            self.error(node, msg);
        }
    }
}

/// An unsuffixed integer literal, possibly negated: `100`, `-5`.
fn is_int_literal(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::Literal(l) => {
            matches!(l.kind(), LiteralKind::IntNumber(n) if n.suffix().is_none())
        }
        ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Neg) => {
            p.expr().is_some_and(|i| is_int_literal(&i))
        }
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|i| is_int_literal(&i)),
        _ => false,
    }
}

/// `Ordering::Less` → -1, `Equal` → 0, `Greater` → 1.
pub fn ordering_variant(path: Option<ast::Path>) -> Option<i8> {
    let text = path?.syntax().text().to_string();
    let last2: Vec<&str> = text.rsplit("::").take(2).collect();
    match last2.as_slice() {
        ["Less", "Ordering"] => Some(-1),
        ["Equal", "Ordering"] => Some(0),
        ["Greater", "Ordering"] => Some(1),
        _ => None,
    }
}

/// A pattern reduced to the constructors exhaustiveness checking understands.
#[derive(Clone, Debug)]
enum Ap {
    Wild,
    Bool(bool),
    Ord(i8),
    None,
    Some(Box<Ap>),
    Tuple(Vec<Ap>),
    /// A struct or enum variant (by index) with its fields in declaration order.
    Ctor(usize, Vec<Ap>),
    /// A slice pattern: elements before `..`, elements after it, and whether there is a `..`.
    Slice(Vec<Ap>, Vec<Ap>, bool),
    /// An integer literal or range pattern, as an inclusive range.
    Int(i128, i128),
    /// A refutable pattern over an effectively infinite domain (an integer, char or string).
    Other,
    /// A constructor pattern of a type this checker doesn't know (another crate's enum).
    Opaque,
}

enum Family {
    Adt(String, Vec<Ty>),
    Slice(Ty),
    Result(Ty, Ty),
    Bool,
    Ordering,
    Option(Ty),
    Tuple(Vec<Ty>),
    /// An integer type, by its range.
    Int(IntKind),
    /// `char`: code points, without the surrogates.
    Char,
    Infinite,
}

/// The values of an integer type. (`u128`'s top half doesn't fit; patterns don't reach it.)
fn int_domain(k: IntKind) -> (i128, i128) {
    match k {
        IntKind::I8 => (i8::MIN as i128, i8::MAX as i128),
        IntKind::I16 => (i16::MIN as i128, i16::MAX as i128),
        IntKind::I32 => (i32::MIN as i128, i32::MAX as i128),
        IntKind::I64 | IntKind::Isize => (i64::MIN as i128, i64::MAX as i128),
        IntKind::I128 => (i128::MIN, i128::MAX),
        IntKind::U8 => (0, u8::MAX as i128),
        IntKind::U16 => (0, u16::MAX as i128),
        IntKind::U32 => (0, u32::MAX as i128),
        IntKind::U64 | IntKind::Usize => (0, u64::MAX as i128),
        IntKind::U128 => (0, i128::MAX),
    }
}

/// A missing integer range the way rustc names it: `5_u8`, `i32::MIN..=-1_i32`, `128_u8..=u8::MAX`.
fn describe_int_range(k: IntKind, lo: i128, hi: i128) -> String {
    let (min, max) = int_domain(k);
    let ty = Ty::Int(k).describe();
    let end = |v: i128| {
        if v == min {
            format!("{ty}::MIN")
        } else if v == max {
            format!("{ty}::MAX")
        } else {
            format!("{v}_{ty}")
        }
    };
    if lo == hi {
        end(lo)
    } else {
        format!("{}..={}", end(lo), end(hi))
    }
}

/// A `char` literal pattern's code point.
fn char_lit_value(lp: &ast::LiteralPat) -> Option<i128> {
    let LiteralKind::Char(ch) = lp.literal()?.kind() else {
        return None;
    };
    ch.value().ok().map(|c| c as i128)
}

/// A missing `char` range the way rustc names it: `'a'`, `'\u{80}'..='\u{10ffff}'`.
fn describe_char_range(lo: i128, hi: i128) -> String {
    let one = |v: i128| match char::from_u32(v as u32) {
        Some(c) if !c.is_control() && (v as u32) < 0x80 => format!("{c:?}"),
        _ => format!("'\\u{{{v:x}}}'"),
    };
    if lo == hi {
        one(lo)
    } else {
        format!("{}..={}", one(lo), one(hi))
    }
}

/// An integer literal pattern's value (`5`, `-3`, `0xff`, `1_000u32`).
fn int_lit_value(lp: &ast::LiteralPat) -> Option<i128> {
    let lit = lp.literal()?;
    let LiteralKind::IntNumber(n) = lit.kind() else {
        return None;
    };
    let v = n.value().ok()? as i128;
    Some(if lp.minus_token().is_some() { -v } else { v })
}

/// `None` in a pattern parses as an identifier binding; it's really the variant.
fn is_none_pat(ip: &ast::IdentPat) -> bool {
    ip.pat().is_none()
        && ip.mut_token().is_none()
        && ip.ref_token().is_none()
        && ip.name().is_some_and(|n| n.text() == "None")
}

/// Whether a path is `name` or `Option::name`.
pub fn path_is(path: Option<ast::Path>, name: &str) -> bool {
    let Some(path) = path else { return false };
    let segs: Vec<String> = path
        .segments()
        .filter_map(|s| s.name_ref())
        .map(|n| n.text().to_string())
        .collect();
    match segs.as_slice() {
        [n] => n == name,
        [.., o, n] => o == "Option" && n == name,
        _ => false,
    }
}

pub fn strip_pat(p: &ast::Pat) -> ast::Pat {
    match p {
        ast::Pat::ParenPat(pp) => pp.pat().map(|i| strip_pat(&i)).unwrap_or_else(|| p.clone()),
        ast::Pat::RefPat(rp) => rp.pat().map(|i| strip_pat(&i)).unwrap_or_else(|| p.clone()),
        _ => p.clone(),
    }
}

/// Whether a loop body has no `.await` and no way out (`break`, `return`, `?`), not counting
/// closures inside it.
fn never_awaits_or_exits(body: &SyntaxNode) -> bool {
    use ra_ap_syntax::WalkEvent;
    let mut walk = body.preorder();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(n) = event else { continue };
        match n.kind() {
            ra_ap_syntax::SyntaxKind::CLOSURE_EXPR | ra_ap_syntax::SyntaxKind::FN => {
                walk.skip_subtree()
            }
            ra_ap_syntax::SyntaxKind::AWAIT_EXPR
            | ra_ap_syntax::SyntaxKind::BREAK_EXPR
            | ra_ap_syntax::SyntaxKind::RETURN_EXPR
            | ra_ap_syntax::SyntaxKind::TRY_EXPR => return false,
            // `panic!` and friends end the script.
            ra_ap_syntax::SyntaxKind::MACRO_CALL
                if ast::MacroCall::cast(n.clone())
                    .and_then(|m| macros::macro_name(&m))
                    .is_some_and(|m| {
                        matches!(
                            m.as_str(),
                            "panic" | "unreachable" | "todo" | "unimplemented"
                        )
                    }) =>
            {
                return false;
            }
            _ => {}
        }
    }
    true
}

/// The `&&`-separated parts of a condition.
pub fn chain_parts(e: &ast::Expr) -> Vec<ast::Expr> {
    match e {
        ast::Expr::BinExpr(b) if b.op_kind() == Some(BinaryOp::LogicOp(ast::LogicOp::And)) => {
            let mut out = b.lhs().map(|l| chain_parts(&l)).unwrap_or_default();
            out.extend(b.rhs().map(|r| chain_parts(&r)).unwrap_or_default());
            out
        }
        other => vec![other.clone()],
    }
}

/// `let PAT = x && ...`: a condition with a `let` among its `&&` parts.
pub fn is_let_chain(e: &ast::Expr) -> bool {
    let parts = chain_parts(e);
    parts.len() > 1 && parts.iter().any(|p| matches!(p, ast::Expr::LetExpr(_)))
}

/// `Box::new(x)`, or `Box::pin(x)` (a `Pin<Box<T>>` is `T` too).
pub fn is_box_new(c: &ast::CallExpr) -> bool {
    let Some(ast::Expr::PathExpr(p)) = c.expr() else {
        return false;
    };
    let names: Vec<String> = p
        .path()
        .map(|p| {
            p.segments()
                .filter_map(|s| s.name_ref())
                .map(|n| n.text().to_string())
                .collect()
        })
        .unwrap_or_default();
    matches!(
        names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice(),
        [.., "Box", "new" | "pin"]
    )
}

/// An element taken out of a `&mut` iterator: `v.iter_mut().find(..)`, `.max_by_key(..)?`,
/// `.next().unwrap()` (a `&mut` to it).
fn mut_element(e: &ast::Expr) -> bool {
    let mut e = strip_parens(e);
    loop {
        match &e {
            ast::Expr::TryExpr(t) => match t.expr() {
                Some(inner) => e = strip_parens(&inner),
                None => return false,
            },
            ast::Expr::MethodCallExpr(m) => {
                let name = m
                    .name_ref()
                    .map(|n| n.text().to_string())
                    .unwrap_or_default();
                if matches!(name.as_str(), "unwrap" | "expect") {
                    match m.receiver() {
                        Some(r) => e = strip_parens(&r),
                        None => return false,
                    }
                    continue;
                }
                if !matches!(
                    name.as_str(),
                    "next"
                        | "next_back"
                        | "last"
                        | "nth"
                        | "find"
                        | "min_by"
                        | "max_by"
                        | "min_by_key"
                        | "max_by_key"
                ) {
                    return false;
                }
                // Down the adapters to the iterator's source.
                let mut r = m.receiver();
                while let Some(ast::Expr::MethodCallExpr(inner)) = r.as_ref().map(strip_parens) {
                    let n = inner
                        .name_ref()
                        .map(|n| n.text().to_string())
                        .unwrap_or_default();
                    if matches!(n.as_str(), "iter_mut" | "values_mut") {
                        return true;
                    }
                    if !matches!(
                        n.as_str(),
                        "filter"
                            | "rev"
                            | "skip"
                            | "take"
                            | "skip_while"
                            | "take_while"
                            | "step_by"
                    ) {
                        return false;
                    }
                    r = inner.receiver();
                }
                return false;
            }
            _ => return false,
        }
    }
}

/// The file's own `Cell` / `RefCell` type, when `c` is `Cell::new(..)` of one.
pub fn cell_shadowed(c: &ast::CallExpr, adts: &crate::adt::Adts) -> bool {
    let names = path_names(c);
    names.len() == 2 && adts.defs.contains_key(&names[0])
}

impl Checker<'_> {
    /// `Cell::new(..)` of the file's own `Cell` type.
    fn user_cell(&self, c: &ast::CallExpr) -> bool {
        cell_shadowed(c, &self.adts)
    }
}

/// `RefCell::new(x)` / `Cell::new(x)`.
pub fn is_cell_new(c: &ast::CallExpr) -> bool {
    path_names(c).ends_with(&["RefCell".to_string(), "new".to_string()])
        || path_names(c).ends_with(&["Cell".to_string(), "new".to_string()])
}

/// `Rc::new(x)` / `Rc::clone(&x)` (and `Arc`): `x` itself.
pub fn is_rc_call(c: &ast::CallExpr) -> bool {
    let n = path_names(c);
    matches!(
        n.iter().map(String::as_str).collect::<Vec<_>>().as_slice(),
        [.., "Rc" | "Arc", "new" | "clone"]
    )
}

fn path_names(c: &ast::CallExpr) -> Vec<String> {
    let Some(ast::Expr::PathExpr(p)) = c.expr() else {
        return vec![];
    };
    p.path()
        .map(|p| {
            p.segments()
                .filter_map(|s| s.name_ref())
                .map(|n| n.text().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// `Default::default()` (also `std::default::Default::default()`).
pub fn is_default_call(c: &ast::CallExpr) -> bool {
    let Some(ast::Expr::PathExpr(p)) = c.expr() else {
        return false;
    };
    let text: String = p.syntax().text().to_string().split_whitespace().collect();
    matches!(
        text.as_str(),
        "Default::default" | "std::default::Default::default" | "core::default::Default::default"
    )
}

/// Patterns that match every value: `_`, `x`, `x @ <irrefutable>`.
pub fn irrefutable(p: &ast::Pat) -> bool {
    match strip_pat(p) {
        ast::Pat::WildcardPat(_) | ast::Pat::RestPat(_) => true,
        ast::Pat::IdentPat(ip) => !is_none_pat(&ip) && ip.pat().is_none_or(|sub| irrefutable(&sub)),
        ast::Pat::TuplePat(tp) => tp.fields().all(|f| irrefutable(&f)),
        _ => false,
    }
}

pub fn is_none_binding(ip: &ast::IdentPat) -> bool {
    is_none_pat(ip)
}

/// `dyn Error` / `dyn std::error::Error` (plus `Send + Sync` and the like).
fn names_std_error(t: &ast::Type) -> bool {
    let list = match t {
        ast::Type::ImplTraitType(i) => i.type_bound_list(),
        ast::Type::DynTraitType(d) => d.type_bound_list(),
        _ => None,
    };
    list.is_some_and(|l| {
        l.bounds().any(|b| match b.ty() {
            Some(ast::Type::PathType(p)) => p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .is_some_and(|n| n.text() == "Error"),
            _ => false,
        })
    })
}

/// For `impl Iterator<Item = T>` / `dyn Iterator<..>`: `Some(Some(T))`, or `Some(None)` without an
/// `Item`; `None` for other types.
fn iterator_item(t: &ast::Type) -> Option<Option<ast::Type>> {
    let list = match t {
        ast::Type::ImplTraitType(i) => i.type_bound_list()?,
        ast::Type::DynTraitType(d) => d.type_bound_list()?,
        _ => return None,
    };
    bounds_iterator_item(&list)
}

/// The `X` of an `Into<X>`, `AsRef<X>` or `Borrow<X>` bound (`impl Into<String>`,
/// `S: AsRef<str>`): values of those types are taken as `X` itself.
pub(super) fn conversion_target(list: &ast::TypeBoundList) -> Option<ast::Type> {
    use ra_ap_syntax::ast::HasGenericArgs;
    list.bounds().find_map(|b| match b.ty()? {
        ast::Type::PathType(p) => {
            let seg = p.path()?.segment()?;
            let n = seg.name_ref()?.text().to_string();
            if !matches!(n.as_str(), "Into" | "AsRef" | "Borrow") {
                return None;
            }
            seg.generic_arg_list()?
                .generic_args()
                .find_map(|g| match g {
                    ast::GenericArg::TypeArg(ta) => ta.ty(),
                    _ => None,
                })
        }
        _ => None,
    })
}

/// `iterator_item` of a bound list (`I: Iterator<Item = T>`).
/// `Hasher` among bounds (`H: Hasher` in an `impl Hash`).
pub(super) fn bounds_hasher(list: &ast::TypeBoundList) -> bool {
    list.bounds().any(|b| {
        matches!(b.ty(), Some(ast::Type::PathType(p))
            if p.path().and_then(|p| p.segment()).and_then(|s| s.name_ref()).is_some_and(|n| n.text() == "Hasher"))
    })
}

/// `IntoIterator<Item = T>` among bounds: Some(the `Item` type, if given).
pub(super) fn bounds_into_iter_item(list: &ast::TypeBoundList) -> Option<Option<ast::Type>> {
    use ra_ap_syntax::ast::HasGenericArgs;
    list.bounds().find_map(|b| match b.ty()? {
        ast::Type::PathType(p) => {
            let seg = p.path()?.segment()?;
            if seg.name_ref()?.text() != "IntoIterator" {
                return None;
            }
            Some(seg.generic_arg_list().and_then(|l| {
                l.generic_args().find_map(|g| match g {
                    ast::GenericArg::AssocTypeArg(a)
                        if a.name_ref().is_some_and(|n| n.text() == "Item") =>
                    {
                        a.ty()
                    }
                    _ => None,
                })
            }))
        }
        _ => None,
    })
}

pub(super) fn bounds_iterator_item(list: &ast::TypeBoundList) -> Option<Option<ast::Type>> {
    use ra_ap_syntax::ast::HasGenericArgs;
    list.bounds().find_map(|b| match b.ty()? {
        ast::Type::PathType(p) => {
            let seg = p.path()?.segment()?;
            let n = seg.name_ref()?.text().to_string();
            if !matches!(
                n.as_str(),
                "Iterator" | "DoubleEndedIterator" | "ExactSizeIterator"
            ) {
                return None;
            }
            let item = seg.generic_arg_list().and_then(|l| {
                l.generic_args().find_map(|g| match g {
                    ast::GenericArg::AssocTypeArg(a)
                        if a.name_ref().is_some_and(|n| n.text() == "Item") =>
                    {
                        a.ty()
                    }
                    _ => None,
                })
            });
            Some(item)
        }
        _ => None,
    })
}

/// A place made of names, fields and simple indexes (`n.left`, `v[i]`, `self.grid[r][c]`), which
/// can be evaluated again without side effects.
fn simple_place(e: &ast::Expr) -> bool {
    match strip_parens(e) {
        ast::Expr::PathExpr(_) => true,
        ast::Expr::FieldExpr(f) => f.expr().is_some_and(|x| simple_place(&x)),
        ast::Expr::IndexExpr(ix) => {
            ix.base().is_some_and(|b| simple_place(&b))
                && matches!(
                    ix.index().map(|i| strip_parens(&i)),
                    Some(ast::Expr::PathExpr(_) | ast::Expr::Literal(_) | ast::Expr::FieldExpr(_))
                )
        }
        ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Deref) => {
            p.expr().is_some_and(|x| simple_place(&x))
        }
        _ => false,
    }
}

pub(crate) fn strip_parens(e: &ast::Expr) -> ast::Expr {
    match e {
        ast::Expr::ParenExpr(p) => p
            .expr()
            .map(|i| strip_parens(&i))
            .unwrap_or_else(|| e.clone()),
        _ => e.clone(),
    }
}

/// `r` in `let r = v.get_mut(..).unwrap();`.
fn slot_let_binding(m: &ast::MethodCallExpr) -> Option<ast::Name> {
    let u = ast::MethodCallExpr::cast(m.syntax().parent()?)?;
    let l = ast::LetStmt::cast(u.syntax().parent()?)?;
    match l.pat()? {
        ast::Pat::IdentPat(ip) => ip.name(),
        _ => None,
    }
}

/// Whether a `get_mut`-style call's result is used where a slot can stand in for it: matched
/// (`if let` / `while let` / `match`), or unwrapped right into a deref or a `let`.
fn slot_position(m: &ast::MethodCallExpr) -> bool {
    let mut n = m.syntax().clone();
    while let Some(p) = n
        .parent()
        .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
    {
        n = p;
    }
    let Some(p) = n.parent() else { return false };
    if ast::LetExpr::can_cast(p.kind()) {
        return true;
    }
    if let Some(me) = ast::MatchExpr::cast(p.clone()) {
        return me.expr().is_some_and(|e| e.syntax() == &n);
    }
    if let Some(u) = ast::MethodCallExpr::cast(p)
        && u.receiver().is_some_and(|r| r.syntax() == &n)
        && u.name_ref()
            .is_some_and(|x| matches!(x.text(), "unwrap" | "expect"))
        && let Some(q) = u.syntax().parent()
    {
        return ast::LetStmt::can_cast(q.kind())
            || ast::PrefixExpr::cast(q)
                .is_some_and(|pe| pe.op_kind() == Some(ast::UnaryOp::Deref));
    }
    false
}

/// `v.get_mut(i).is_some()` and the like: the result is only looked at, so a copy is fine.
fn only_tested(m: &ast::MethodCallExpr) -> bool {
    m.syntax()
        .parent()
        .and_then(ast::MethodCallExpr::cast)
        .filter(|u| u.receiver().is_some_and(|r| r.syntax() == m.syntax()))
        .and_then(|u| u.name_ref())
        .is_some_and(|n| matches!(n.text(), "is_some" | "is_none"))
}

impl Checker<'_> {
    /// The `&mut place` results of each `impl IndexMut`'s `index_mut` become cells (`grid[i] = v`
    /// writes through them); a type whose `index_mut` ends some other way is noted, and
    /// assigning through it is an error.
    fn scan_index_mut(&mut self, file: &ast::SourceFile) {
        for imp in file.syntax().descendants().filter_map(ast::Impl::cast) {
            let (tr, ty) = crate::adt::impl_parts(&imp);
            if tr.as_deref() != Some("IndexMut") {
                continue;
            }
            let Some(f) = imp
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
                .find_map(|a| match a {
                    ast::AssocItem::Fn(f) if f.name().is_some_and(|n| n.text() == "index_mut") => {
                        Some(f)
                    }
                    _ => None,
                })
            else {
                continue;
            };
            let Some(body) = f.body() else { continue };
            let mut results: Vec<ast::Expr> = body
                .syntax()
                .descendants()
                .filter_map(ast::ReturnExpr::cast)
                .filter_map(|r| r.expr())
                .collect();
            if let Some(t) = body.stmt_list().and_then(|l| l.tail_expr()) {
                results.push(t);
            }
            for e in results {
                match strip_parens(&e) {
                    ast::Expr::RefExpr(r) if r.mut_token().is_some() => {
                        let rr = self.range(r.syntax());
                        self.cell_returns.insert(rr);
                    }
                    _ => {
                        if let Some(t) = &ty {
                            self.opaque_index_mut.insert(t.clone());
                        }
                    }
                }
            }
        }
    }
}

/// Whether `n` is inside an `unsafe { .. }` block or an `unsafe fn`.
fn in_unsafe(n: &SyntaxNode) -> bool {
    n.ancestors().any(|a| {
        ast::BlockExpr::cast(a.clone())
            .is_some_and(|b| matches!(b.modifier(), Some(ast::BlockModifier::Unsafe(_))))
            || ast::Fn::cast(a).is_some_and(|f| f.unsafe_token().is_some())
    })
}

/// Each function's `$T` parameters, and each call's classes for them.
type Dicts = (
    HashMap<String, Vec<String>>,
    HashMap<(usize, usize), Vec<Ty>>,
);

/// A generic call: (call, caller, callee, the callee's type arguments).
type GenericCall = ((usize, usize), String, String, Vec<Ty>);

/// A call naming its trait: `Pilot::fly(&h)`, `<Human as Wizard>::name()`.
#[derive(Clone, Debug)]
pub struct QualifiedCall {
    pub trait_name: String,
    pub method: String,
    /// The implementing struct or enum, when known (else the call goes by the plain name).
    pub adt: Option<String>,
    /// The first argument is `self`.
    pub receiver: bool,
    /// Call it as `Trait$method` (the type is local, or a generic / `dyn` of a local trait).
    pub alias: bool,
    /// `<Self as Trait>::f()` in a trait's default method: on `this` (or its class).
    pub on_self: bool,
}

impl Checker<'_> {
    /// `Trait::method(x, ..)` and `<T as Trait>::method(..)` for a trait of this file.
    fn qualified_call(
        &mut self,
        c: &ast::CallExpr,
        callee: &ast::PathExpr,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let path = callee.path()?;
        let method = path.segment()?.name_ref()?.text().to_string();
        let q = path.qualifier()?;
        if q.qualifier().is_some() {
            return None;
        }
        let seg = q.segment()?;
        // `<T as Trait>` or `Trait`.
        let (trait_name, anchor_ty) = match seg.type_anchor() {
            // (Both are `PATH_TYPE`s: the type, then the trait.)
            Some(a) if a.as_token().is_some() => {
                let mut tys = a.syntax().children().filter_map(ast::Type::cast);
                let ty = tys.next();
                let Some(ast::Type::PathType(tp)) = tys.next() else {
                    return None;
                };
                let tr = tp.path()?.segment()?.name_ref()?.text().to_string();
                (tr, ty)
            }
            Some(_) => return None,
            None => (seg.name_ref()?.text().to_string(), None),
        };
        let def = self.adts.traits.get(&trait_name).cloned()?;
        let tm = def.methods.iter().find(|m| m.name == method)?.clone();
        // (`self: Arc<Self>` and the like count too: the first parameter is about `Self`.)
        let receiver = tm.sig.receiver.is_some()
            || tm
                .sig
                .params
                .first()
                .is_some_and(|p| p.mentions_param("Self"));
        let self_ty = match (&anchor_ty, receiver, args.first()) {
            (Some(t), _, _) => self.lower_type(t),
            (None, true, Some(a)) => {
                let t = self.infer(a, None);
                self.shallow(&t)
            }
            _ => self.fresh(VarKind::General),
        };
        let sub: HashMap<String, Ty> = [("Self".to_string(), self_ty.clone())].into();
        let sig = traits::subst_method(&tm.sig, &sub);
        let rest = if receiver {
            args.get(1..).unwrap_or(&[])
        } else {
            args
        };
        if receiver
            && anchor_ty.is_some()
            && let Some(a) = args.first()
        {
            self.check(a, &self_ty);
        }
        self.arg_count(
            c.syntax(),
            "function",
            sig.params.len() + usize::from(receiver),
            args.len(),
        );
        for (a, p) in rest.iter().zip(sig.params.iter()) {
            self.check(a, p);
        }
        let local_trait = def.module.is_none();
        let r = self.range(c.syntax());
        self.qualified_calls.insert(
            r,
            QualifiedCall {
                trait_name,
                method,
                adt: None,
                receiver,
                alias: false,
                on_self: false,
            },
        );
        // `Self` may only be known later (`let x: Point = Default::default();`).
        self.pending_qualified
            .push((r, self_ty, local_trait, c.syntax().clone()));
        Some(sig.ret)
    }
}

impl Checker<'_> {
    /// `T`'s bounds and their supertraits, of this file.
    fn bound_traits(&self, param: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut todo: Vec<String> = self.param_bounds.get(param).cloned().unwrap_or_default();
        while let Some(t) = todo.pop() {
            if out.contains(&t) {
                continue;
            }
            if let Some(d) = self.adts.traits.get(&t) {
                todo.extend(d.supers.iter().cloned());
            }
            out.push(t);
        }
        out
    }

    /// Ask for `param`'s class where the function can take it (for an operator on a `T`). With
    /// `need` (the operator), an `impl<T>`'s parameter in a method with `self` is an error.
    pub(super) fn param_class(&mut self, param: &str, node: &SyntaxNode, need: Option<&str>) {
        if param == "Self" || param.contains("::") {
            return;
        }
        let own = self.cur_fn.clone().filter(|f| match f.split_once("::") {
            Some((ty, m)) => self
                .adts
                .method(ty, m)
                .is_some_and(|s| s.dict_names().iter().any(|p| p == param)),
            None => true,
        });
        match (own, need) {
            (Some(f), _) => {
                self.dict_uses.insert((f, param.to_string()));
            }
            // An `impl<T>`'s parameter in a method with `self`: the struct's values carry it.
            (None, _) if self.store_class(param) => {}
            (None, Some(op)) => self.error(
                node,
                format!("{op} on a type parameter of an `impl` isn't supported yet in methods with `self` of an enum or a trait (it works in a struct's `impl<T> S<T>`, on a function's or method's own type parameters, and in associated functions)"),
            ),
            (None, None) => {}
        }
    }

    /// In a method with `self` of `impl<T> S<T>`: `S`'s values carry `T`'s class (`this.$T`).
    /// False where they can't (an enum, `impl<T> S<Vec<T>>`, a trait's default method).
    fn store_class(&mut self, param: &str) -> bool {
        let Some(k) = self
            .cur_fn
            .as_deref()
            .and_then(|f| self.stored_pos(f, param))
        else {
            return false;
        };
        self.stored_uses.insert(k);
        true
    }

    /// (struct, position) of `param` when `f` is a method with `self` of `impl<.., param, ..>
    /// S<.., param, ..>` for a struct `S`.
    fn stored_pos(&self, f: &str, param: &str) -> Option<(String, usize)> {
        let (ty, m) = f.split_once("::")?;
        let def = self.adts.defs.get(ty)?;
        if def.is_enum {
            return None;
        }
        let sig = self.adts.method(ty, m)?;
        sig.receiver.as_ref()?;
        let i = sig
            .impl_args
            .iter()
            .position(|a| *a == Ty::Param(param.to_string()))?;
        // (Const parameters are carried as values already: `this.$N`.)
        let name = def.type_params.get(i)?;
        let is_const = self
            .adts
            .const_params
            .get(ty)
            .is_some_and(|cps| cps.iter().any(|c| &c.name == name));
        (!is_const).then(|| (ty.to_string(), i))
    }

    /// Note that the function being checked needs `param`'s class (not for `Self` in a trait's
    /// default method, which is `this`).
    fn note_dict(&mut self, param: &str, node: &SyntaxNode) {
        if param == "Self" {
            return;
        }
        // A free function's type parameter, or a method's own (`fn parse<T: Parse>(&self)`);
        // an `impl<T>`'s would need the class per value.
        let own = self.cur_fn.clone().filter(|f| match f.split_once("::") {
            Some((ty, m)) => self
                .adts
                .method(ty, m)
                .is_some_and(|s| s.dict_names().iter().any(|p| p == param)),
            None => true,
        });
        match own {
            Some(f) => {
                self.dict_uses.insert((f, param.to_string()));
            }
            None if self.store_class(param) => {}
            None => self.error(
                node,
                format!("`{param}::..` on a type parameter of an `impl` isn't supported yet in methods with `self` of an enum or a trait (it works in a struct's `impl<T> S<T>`, on a function's or method's own type parameters, and in associated functions)"),
            ),
        }
    }

    /// `T::make(..)` for a static function of a bound of `T`, or `T::default()`.
    fn param_static_call(
        &mut self,
        c: &ast::CallExpr,
        param: &str,
        f: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let pty = Ty::Param(param.to_string());
        let bounds = self.bound_traits(param);
        // `T::from(x)` with `T: From<X>`.
        if f == "from" && self.param_has(param, "From") {
            self.arg_count(c.syntax(), "function", 1, args.len());
            let x = self.from_args.get(param).cloned().unwrap_or(Ty::Unknown);
            if let Some(a) = args.first() {
                self.check(a, &x);
            }
            let r = self.range(c.syntax());
            let name = self.zonk(&x).describe();
            self.param_froms.insert(r, name);
            self.note_dict(param, c.syntax());
            return Some(pty);
        }
        if f == "default" && bounds.iter().any(|b| b == "Default") {
            self.arg_count(c.syntax(), "function", 0, args.len());
            self.note_dict(param, c.syntax());
            return Some(pty);
        }
        let sig = bounds.iter().find_map(|b| {
            let d = self.adts.traits.get(b)?;
            let tm = d
                .methods
                .iter()
                .find(|m| m.name == f && m.sig.receiver.is_none())?;
            Some(tm.sig.clone())
        })?;
        let sub: HashMap<String, Ty> = [("Self".to_string(), pty)].into();
        let sig = traits::subst_method(&sig, &sub);
        self.arg_count(c.syntax(), "function", sig.params.len(), args.len());
        for (a, p) in args.iter().zip(sig.params.iter()) {
            self.check(a, p);
        }
        self.note_dict(param, c.syntax());
        Some(sig.ret)
    }

    /// `T::SIDES`: an associated const declared by a bound of `T`.
    fn param_const(&mut self, param: &str, konst: &str, node: &SyntaxNode) -> Option<Ty> {
        if !self.param_bounds.contains_key(param) {
            return None;
        }
        let t = self
            .bound_traits(param)
            .iter()
            .find_map(|b| self.adts.traits.get(b)?.const_tys.get(konst).cloned())?;
        self.note_dict(param, node);
        Some(t)
    }

    /// Note that the function being checked prints values of `param` with `{:?}` (a function's
    /// or method's own type parameter; others print as their run-time value says).
    fn note_dbg(&mut self, param: &str) {
        // (`S::Unit` takes one too; a trait's own template doesn't.)
        if param == "Self" || param.starts_with("Self::") || self.is_template(param) {
            return;
        }
        let own = self.cur_fn.clone().filter(|f| match f.split_once("::") {
            Some((ty, m)) => self
                .adts
                .method(ty, m)
                .is_some_and(|s| s.dict_names().iter().any(|p| p == param)),
            None => true,
        });
        if let Some(f) = own {
            self.dbg_uses.insert((f, param.to_string()));
        }
    }

    /// The type parameters inside `t` (`Vec<Option<T>>` → `T`).
    fn params_in(t: &Ty, out: &mut Vec<String>) {
        match t {
            Ty::Param(p) => out.push(p.clone()),
            other => {
                for i in other.inner_types() {
                    Self::params_in(&i, out);
                }
            }
        }
    }

    /// The type parameters a function or method can take classes, formatters or const values
    /// for, in the order of its type arguments: a function's type parameters then their
    /// projections (`S::Unit`), in the order `instantiate` gives them variables; a method's
    /// `dict_names`.
    fn need_names(&self, f: &str) -> Vec<String> {
        match f.split_once("::") {
            // A method: `Type::name`.
            Some((ty, m)) => self
                .adts
                .method(ty, m)
                .map(|s| s.dict_names())
                .unwrap_or_default(),
            // A function: its type parameters, then their projections (`S::Unit`), in the
            // order `instantiate` gives them variables.
            None => self
                .fns
                .get(f)
                .map(|s| {
                    let mut names: Vec<String> =
                        s.type_params.iter().map(|(n, _)| n.clone()).collect();
                    names.extend(
                        self.proj_names(&s.type_params)
                            .into_iter()
                            .map(|(p, _, a)| format!("{p}::{a}")),
                    );
                    names
                })
                .unwrap_or_default(),
        }
    }

    /// A function's const parameters (and an associated function's `impl`'s, `extra`): passed
    /// as `$N` unless an argument holds them.
    fn note_const_params(&mut self, key: &str, f: &ast::Fn, extra: Vec<String>) {
        let mut names: Vec<String> = f
            .generic_param_list()
            .into_iter()
            .flat_map(|l| l.generic_params())
            .filter_map(|p| match p {
                ast::GenericParam::ConstParam(cp) => Some(cp.name()?.text().to_string()),
                _ => None,
            })
            .collect();
        names.extend(extra);
        for n in names {
            let k = (key.to_string(), n.clone());
            self.dict_uses.insert(k.clone());
            self.const_needs.insert(k.clone());
            if f.param_list()
                .into_iter()
                .flat_map(|pl| pl.params())
                .any(|p| {
                    p.ty()
                        .is_some_and(|t| crate::adt::const_path(&t, &n, &self.adts).is_some())
                })
            {
                self.const_derivable.insert(k);
            }
        }
    }

    /// Each generic call's const arguments must be known, or readable off an argument.
    fn check_const_args(&mut self) {
        // (An error elsewhere, like a bad const argument, leaves arguments unknown.)
        if self.diags.iter().any(|d| d.severity == "error") {
            return;
        }
        let mut errs = Vec::new();
        for (r, _, callee, targs) in &self.generic_calls {
            for (i, n) in self.need_names(callee).iter().enumerate() {
                let k = (callee.clone(), n.clone());
                if !self.const_needs.contains(&k) || self.const_derivable.contains(&k) {
                    continue;
                }
                if matches!(targs.get(i), Some(Ty::ConstVal(_) | Ty::Param(_)))
                    || !self.unbound_targs.contains(&(*r, i))
                {
                    continue;
                }
                let on = match callee.split_once("::") {
                    Some((ty, m))
                        if self
                            .adts
                            .method(ty, m)
                            .is_some_and(|s| s.type_params.contains(n)) =>
                    {
                        format!("the associated function `{m}`")
                    }
                    Some((ty, _)) => format!("the struct `{ty}`"),
                    None => format!("the function `{callee}`"),
                };
                errs.push(RawDiag {
                    start: r.0,
                    end: r.1,
                    message: format!("type annotations needed: cannot infer the value of the const parameter `{n}` declared on {on} (give it with `::<..>`: array lengths in types don't inform it here)"),
                    severity: "error",
                });
            }
        }
        self.diags.extend(errs);
    }

    /// A type or const generic argument (`i32`, `3`, `{ N }`); None for lifetimes and bindings.
    pub(super) fn generic_arg_ty(&mut self, g: &ast::GenericArg) -> Option<Ty> {
        match g {
            ast::GenericArg::TypeArg(ta) => {
                Some(ta.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown))
            }
            ast::GenericArg::ConstArg(c) => Some(
                c.expr()
                    .map(|e| self.lower_const_arg(&e))
                    .unwrap_or(Ty::Unknown),
            ),
            _ => None,
        }
    }

    /// A const argument: a literal (`3`, `true`, `'a'`, `-1`), a const parameter or a `const`
    /// item (`N`, `{ N }`).
    pub(super) fn lower_const_arg(&mut self, e: &ast::Expr) -> Ty {
        match e {
            ast::Expr::Literal(l) => match l.kind() {
                ast::LiteralKind::IntNumber(n) if n.value().is_ok() => {
                    Ty::ConstVal(n.value().unwrap().to_string())
                }
                ast::LiteralKind::Bool(b) => Ty::ConstVal(b.to_string()),
                // `b'A'`: a `u8`.
                ast::LiteralKind::Byte(b) if b.value().is_ok() => {
                    Ty::ConstVal(b.value().unwrap().to_string())
                }
                ast::LiteralKind::Char(_) => Ty::ConstVal(l.syntax().text().to_string()),
                _ => {
                    self.error(
                        e.syntax(),
                        "a const argument must be an integer, `bool` or `char`",
                    );
                    Ty::Unknown
                }
            },
            ast::Expr::BlockExpr(b)
                if b.statements().next().is_none() && b.tail_expr().is_some() =>
            {
                self.lower_const_arg(&b.tail_expr().unwrap())
            }
            ast::Expr::PathExpr(p) if p.path().and_then(|p| p.as_single_name_ref()).is_some() => {
                let n = p
                    .path()
                    .unwrap()
                    .as_single_name_ref()
                    .unwrap()
                    .text()
                    .to_string();
                if self.const_tys.contains_key(&n) {
                    Ty::Param(n)
                } else if self.consts.contains_key(&n) {
                    Ty::ConstVal(n)
                } else {
                    self.error(e.syntax(), format!("cannot find value `{n}` in this scope"));
                    Ty::Unknown
                }
            }
            // `{ N + 1 }`: rustc's error too (on stable).
            _ if e
                .syntax()
                .descendants()
                .filter_map(ast::PathExpr::cast)
                .any(|p| {
                    p.path()
                        .and_then(|p| p.as_single_name_ref())
                        .is_some_and(|n| self.const_tys.contains_key(n.text()))
                }) =>
            {
                self.error(
                    e.syntax(),
                    "generic parameters may not be used in const operations (a const argument is a literal, a const parameter or a `const` item)",
                );
                Ty::Unknown
            }
            // `{ 128 / 8 }`
            _ if const_int(e).is_some() => Ty::ConstVal(const_int(e).unwrap().to_string()),
            _ => {
                self.error(
                    e.syntax(),
                    "this const argument isn't supported yet (a literal, integer arithmetic on literals, a const parameter or a `const` item is)",
                );
                Ty::Unknown
            }
        }
    }

    /// A const parameter's type: an integer (up to 32 bits, or `usize`/`isize`), `bool` or
    /// `char`.
    pub(super) fn const_param_ty(&mut self, cp: &ast::ConstParam, report: bool) -> Ty {
        let ty = cp.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
        let ok = matches!(ty, Ty::Bool | Ty::Char | Ty::Unknown)
            || matches!(&ty, Ty::Int(k) if !k.is_big());
        if !ok && report {
            self.error(
                cp.syntax(),
                format!(
                    "const parameters of type `{}` aren't supported (integers up to 32 bits, `usize`, `isize`, `bool` and `char` are)",
                    ty.describe()
                ),
            );
        }
        ty
    }

    /// Which type parameters each function takes a class (or formatter) for: those its body
    /// uses (`needs`), and those it hands to a function that does (to a fixpoint). Then each
    /// call's types for them.
    /// With `stored` (classes, not `{:?}` formatters), an `impl<T>`'s method with `self` that
    /// passes `T` on gets it off its struct's values, which then carry it (`stored_uses`), as do
    /// the values each construction of such a struct makes.
    fn finish_needs(
        &mut self,
        needs: std::collections::HashSet<(String, String)>,
        stored: bool,
    ) -> Dicts {
        let mut needs = needs;
        let param_names = |c: &Self, f: &str| c.need_names(f);
        loop {
            let mut grew = false;
            let mut uses: Vec<(String, String)> = Vec::new();
            for (_, caller, callee, targs) in &self.generic_calls {
                let names = param_names(self, callee);
                for (i, n) in names.iter().enumerate() {
                    if needs.contains(&(callee.clone(), n.clone()))
                        && let Some(Ty::Param(p)) = targs.get(i)
                    {
                        uses.push((caller.clone(), p.clone()));
                    }
                }
            }
            if stored {
                for (f, adt, targs) in &self.ctor_sites {
                    let Some(f) = f else { continue };
                    for (i, t) in targs.iter().enumerate() {
                        if let Ty::Param(p) = t
                            && self.stored_uses.contains(&(adt.clone(), i))
                        {
                            uses.push((f.clone(), p.clone()));
                        }
                    }
                }
            }
            for (f, p) in uses {
                if param_names(self, &f).contains(&p) {
                    grew |= needs.insert((f, p));
                } else if stored && let Some(k) = self.stored_pos(&f, &p) {
                    grew |= self.stored_uses.insert(k);
                }
            }
            if !grew {
                break;
            }
        }
        let mut params: HashMap<String, Vec<String>> = HashMap::new();
        for f in needs
            .iter()
            .map(|(f, _)| f.clone())
            .collect::<std::collections::HashSet<_>>()
        {
            let list: Vec<String> = param_names(self, &f)
                .into_iter()
                .filter(|n| needs.contains(&(f.clone(), n.clone())))
                .collect();
            params.insert(f, list);
        }
        let mut args = HashMap::new();
        for (r, _, callee, targs) in &self.generic_calls {
            let Some(list) = params.get(callee) else {
                continue;
            };
            let names = param_names(self, callee);
            let tys: Vec<Ty> = list
                .iter()
                .map(|n| {
                    names
                        .iter()
                        .position(|x| x == n)
                        .and_then(|i| targs.get(i).cloned())
                        .unwrap_or(Ty::Unknown)
                })
                .collect();
            args.insert(*r, tys);
        }
        (params, args)
    }
}

impl Checker<'_> {
    /// The local `e` borrows (and whether mutably): `&v`, `&mut v.field`, `&v[i]`, and the
    /// methods that hand out references into their receiver (`v.iter()`, `v.first()`,
    /// `map.get(k)`, ..).
    fn borrowed_local(&self, e: &ast::Expr) -> Option<(crate::borrows::Decl, bool)> {
        let (place, mutable) = match strip_parens(e) {
            ast::Expr::RefExpr(r) => (r.expr()?, r.mut_token().is_some()),
            ast::Expr::MethodCallExpr(m) => {
                let mutable = match m.name_ref()?.text() {
                    "iter" | "first" | "last" | "get" | "keys" | "values" | "as_slice"
                    | "windows" | "chunks" => false,
                    "iter_mut" | "first_mut" | "last_mut" | "get_mut" | "values_mut"
                    | "as_mut_slice" => true,
                    _ => return None,
                };
                (m.receiver()?, mutable)
            }
            _ => return None,
        };
        let (name, through) = Self::place_root(&strip_parens(&place))?;
        let local = self.lookup(name.text())?;
        // A reference held in a local (`r` in `&r[0]`) isn't the owner being borrowed.
        if through && local.reference.is_some() {
            return None;
        }
        Some((local.decl?, mutable))
    }
}

/// Whether a const argument is a literal (not a `const` item's name).
fn const_literal(v: &str) -> bool {
    v.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '\'')
        || v == "true"
        || v == "false"
}

/// Integer arithmetic on literals in a const argument (`{ 128 / 8 }`); None for anything else.
fn const_int(e: &ast::Expr) -> Option<i128> {
    match e {
        ast::Expr::Literal(l) => match l.kind() {
            ast::LiteralKind::IntNumber(n) => i128::try_from(n.value().ok()?).ok(),
            _ => None,
        },
        ast::Expr::ParenExpr(p) => const_int(&p.expr()?),
        ast::Expr::BlockExpr(b) if b.statements().next().is_none() => const_int(&b.tail_expr()?),
        ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Neg) => {
            const_int(&p.expr()?)?.checked_neg()
        }
        ast::Expr::BinExpr(b) => {
            let (l, r) = (const_int(&b.lhs()?)?, const_int(&b.rhs()?)?);
            match b.op_kind()? {
                BinaryOp::ArithOp(ArithOp::Add) => l.checked_add(r),
                BinaryOp::ArithOp(ArithOp::Sub) => l.checked_sub(r),
                BinaryOp::ArithOp(ArithOp::Mul) => l.checked_mul(r),
                BinaryOp::ArithOp(ArithOp::Div) => l.checked_div(r),
                BinaryOp::ArithOp(ArithOp::Rem) => l.checked_rem(r),
                BinaryOp::ArithOp(ArithOp::Shl) => l.checked_shl(u32::try_from(r).ok()?),
                BinaryOp::ArithOp(ArithOp::Shr) => l.checked_shr(u32::try_from(r).ok()?),
                BinaryOp::ArithOp(ArithOp::BitAnd) => Some(l & r),
                BinaryOp::ArithOp(ArithOp::BitOr) => Some(l | r),
                BinaryOp::ArithOp(ArithOp::BitXor) => Some(l ^ r),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The first struct or enum in a type (`Vec<P>` → `P`).
fn first_adt_of(t: &Ty) -> Option<String> {
    let mut found = None;
    t.walk(&mut |x| {
        if let Ty::Adt(n, _) = x
            && found.is_none()
        {
            found = Some(n.clone());
        }
    });
    found
}

impl Checker<'_> {
    /// Whether `x.hash(arg)` is `Hash::hash`: `arg` is a `Hasher` local (`state`, `&mut h`).
    fn hasher_arg(&self, arg: &ast::Expr) -> bool {
        let arg = match arg {
            ast::Expr::RefExpr(r) => r.expr(),
            other => Some(other.clone()),
        };
        match arg {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(n.text().to_string().as_str()))
                .is_some_and(|l| matches!(l.ty, Ty::Hasher)),
            _ => false,
        }
    }
}

impl Checker<'_> {
    /// std's iterator types by name (for parameters like `chars: &mut Peekable<Chars>`).
    fn std_iter_type(&mut self, name: &str, seg: Option<&ast::PathSegment>) -> Option<Ty> {
        // (Names crates reuse only with their module: `slice::Iter`, `vec::IntoIter`,
        // `str::Bytes`.)
        let qualified = seg.and_then(|s| s.parent_path().qualifier()).is_some();
        let known = matches!(
            name,
            "Peekable"
                | "Fuse"
                | "Rev"
                | "Skip"
                | "Take"
                | "StepBy"
                | "Cycle"
                | "Enumerate"
                | "Chars"
                | "CharIndices"
                | "SplitWhitespace"
        ) || (qualified
            && matches!(
                name,
                "Iter" | "IntoIter" | "IterMut" | "Bytes" | "Lines" | "Split"
            ));
        if !known {
            return None;
        }
        let args: Vec<Ty> = seg
            .and_then(|s| s.generic_arg_list())
            .map(|l| {
                l.generic_args()
                    .filter_map(|g| match g {
                        ast::GenericArg::TypeArg(t) => t.ty(),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .iter()
            .map(|t| self.lower_type(t))
            .collect();
        let iter = |elem: Ty, mutable: bool| Ty::Iter {
            elem: Box::new(elem),
            mutable,
        };
        let inner = |args: &[Ty]| match args.first() {
            Some(t @ Ty::Iter { .. }) => t.clone(),
            _ => iter(Ty::Unknown, false),
        };
        Some(match name {
            "Peekable" | "Fuse" | "Rev" | "Skip" | "Take" | "StepBy" | "Cycle" => inner(&args),
            "Enumerate" => match inner(&args) {
                Ty::Iter { elem, mutable } => iter(Ty::Tuple(vec![USIZE, *elem]), mutable),
                other => other,
            },
            "Chars" => iter(Ty::Char, false),
            "CharIndices" => iter(Ty::Tuple(vec![USIZE, Ty::Char]), false),
            "Bytes" => iter(Ty::Int(IntKind::U8), false),
            "Lines" | "SplitWhitespace" | "Split" => iter(Ty::Str, false),
            "Iter" | "IntoIter" | "IterMut" => {
                let elem = match args.as_slice() {
                    [t] => t.clone(),
                    [k, v] => Ty::Tuple(vec![k.clone(), v.clone()]),
                    _ => return None,
                };
                iter(elem, name == "IterMut")
            }
            _ => return None,
        })
    }
}

/// Whether `m` (a `.entry(k)`) is the receiver of `.or_insert(..)` and friends.
pub fn entry_chained(m: &ast::MethodCallExpr) -> bool {
    m.syntax()
        .parent()
        .and_then(ast::MethodCallExpr::cast)
        .is_some_and(|p| {
            p.receiver().is_some_and(|r| r.syntax() == m.syntax())
                && p.name_ref().is_some_and(|n| {
                    matches!(
                        n.text().to_string().as_str(),
                        "or_insert"
                            | "or_insert_with"
                            | "or_insert_with_key"
                            | "or_default"
                            | "and_modify"
                            | "key"
                    )
                })
        })
}

/// Which of `f`'s parameters are `impl Into<X>` / `impl AsRef<X>` / `impl Borrow<X>`, or a type
/// parameter bounded by one of them.
pub(super) fn into_param_flags(f: &ast::Fn) -> Vec<bool> {
    use ra_ap_syntax::ast::HasTypeBounds;
    let into_bound = |l: &ast::TypeBoundList| {
        l.bounds().any(|b| {
            matches!(b.ty(), Some(ast::Type::PathType(p))
                if p.path().and_then(|p| p.segment()).and_then(|s| s.name_ref()).is_some_and(|n| matches!(n.text().to_string().as_str(), "Into" | "AsRef" | "Borrow")))
        })
    };
    let generic_into = |name: &str| {
        let in_list = f.generic_param_list().is_some_and(|gl| {
            gl.type_or_const_params().any(|p| match p {
                ast::TypeOrConstParam::Type(tp) => {
                    tp.name().is_some_and(|n| n.text() == name)
                        && tp.type_bound_list().is_some_and(|l| into_bound(&l))
                }
                _ => false,
            })
        });
        let in_where = f.where_clause().is_some_and(|w| {
            w.predicates().any(|p| {
                p.ty().is_some_and(|t| t.syntax().text() == name)
                    && p.type_bound_list().is_some_and(|l| into_bound(&l))
            })
        });
        in_list || in_where
    };
    f.param_list()
        .map(|pl| {
            pl.params()
                .map(|p| match p.ty() {
                    Some(ast::Type::ImplTraitType(i)) => {
                        i.type_bound_list().is_some_and(|l| into_bound(&l))
                    }
                    Some(ast::Type::PathType(pt)) => pt
                        .path()
                        .and_then(|p| p.as_single_name_ref())
                        .is_some_and(|n| generic_into(n.text())),
                    _ => false,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// For `impl Future<Output = T>` / `dyn Future<Output = T> + '_` (inside a `Pin<Box<..>>`):
/// `Some(Some(T))`, or `Some(None)` without an `Output`; `None` for other types.
fn future_output(t: &ast::Type) -> Option<Option<ast::Type>> {
    let list = match t {
        ast::Type::ImplTraitType(i) => i.type_bound_list()?,
        ast::Type::DynTraitType(d) => d.type_bound_list()?,
        _ => return None,
    };
    bounds_future_output(&list)
}

/// `future_output` of a bound list (`Fut: Future<Output = T>`).
pub(super) fn bounds_future_output(list: &ast::TypeBoundList) -> Option<Option<ast::Type>> {
    use ra_ap_syntax::ast::HasGenericArgs;
    list.bounds().find_map(|b| match b.ty()? {
        ast::Type::PathType(p) => {
            let seg = p.path()?.segment()?;
            if seg.name_ref()?.text() != "Future" {
                return None;
            }
            let out = seg.generic_arg_list().and_then(|l| {
                l.generic_args().find_map(|g| match g {
                    ast::GenericArg::AssocTypeArg(a)
                        if a.name_ref().is_some_and(|n| n.text() == "Output") =>
                    {
                        a.ty()
                    }
                    _ => None,
                })
            });
            Some(out)
        }
        _ => None,
    })
}
