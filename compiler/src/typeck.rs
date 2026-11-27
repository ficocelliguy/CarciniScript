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
pub(crate) use arrays::{Decl, array_dims_in_file, array_type_dims, array_type_len, decl_at};
#[path = "typeck_coll.rs"]
mod coll;
pub(crate) use coll::index_method as coll_index_method;
pub(crate) use slots::keep_position as slot_keep_position;
#[path = "typeck_any.rs"]
mod any;
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
#[path = "typeck_react.rs"]
mod react;
#[path = "typeck_res.rs"]
mod res;
#[path = "typeck_scope.rs"]
mod scope;
#[path = "typeck_slots.rs"]
mod slots;
#[path = "typeck_std.rs"]
mod std_lib;
#[path = "typeck_trait.rs"]
mod traits;
#[path = "typeck_twins.rs"]
mod twins;
pub use arrays::array_try_from_len;
pub use crates::CrateCall;
pub use ext::{char_predicate, is_option_closure_method};
pub use gaps::{delegated_fmt_is_debug, pin_identity};
pub use jsval::swb_imports;
pub use modules::{
    Imported, ModuleRef, Resolver, capitalized, find_module, find_module_in, module_candidates,
    module_path, path_attr, path_attr_in, pub_use_renames, qualified_key, relative_path, use_leaves,
};
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

use crate::diag_help;
use crate::bindings::{self, Bindings, bare};
use crate::fmt::{self, ArgRef, Piece};
use crate::macros::{self, MacroArg};
use crate::ty::{IntKind, RangeKind, Ty, is_float_method, std_const, std_tail};

pub struct RawDiag {
    pub start: usize,
    pub end: usize,
    pub message: String,
    pub severity: &'static str,
    /// The rule behind the message for a player who doesn't know Rust (`diag_help.rs`);
    /// `None` leaves it to the message's shape.
    pub help: Option<String>,
    /// Other places the error involves: (start, end, rustc's label for it).
    pub spans: Vec<(usize, usize, String)>,
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
    /// `TypeId::of::<T>()`, `type_name::<T>()` and `x.type_id()` calls: their string, by range.
    /// `get_mut` / `first_mut` / `last_mut` of a `Vec<&mut i32>` of slots: the stored slot itself
    /// (`κσ.$vecItem`), not a slot over it, so `**r += 1` writes through to the referent.
    pub slot_item_calls: std::collections::HashSet<(usize, usize)>,
    pub any_strs: HashMap<(usize, usize), String>,
    /// `x.is::<T>()` / `downcast_ref::<T>()` .. on a `dyn Any`: the `T` to test for, by range.
    pub any_checks: HashMap<(usize, usize), Ty>,
    /// `write!` / `writeln!` destinations (by their start) that are `fmt::Write`rs of your own or
    /// a `W: fmt::Write` parameter.
    pub fmt_writes: std::collections::HashSet<usize>,
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
    /// `for x in v` iterables naming a `&mut` collection (`v: &mut [T]`): looped by index like
    /// `for x in &mut v`, so `*x = ..` writes the element.
    pub mut_ref_loops: std::collections::HashSet<(usize, usize)>,
    /// Reads of a fixed-size array of `Copy` values (a local or a struct field), which copy in
    /// by-value positions like rustc's `[T; N]: Copy`.
    pub array_reads: std::collections::HashSet<(usize, usize)>,
    /// Values of your own iterator type given where an `I: Iterator` / `impl Iterator` is
    /// expected (`total(Count { n: 0 })`): the runtime's iterator over it (`κσ.$iterOf`).
    pub iter_of_args: std::collections::HashSet<(usize, usize)>,
    /// Bindings (by name range) that a mutation needed declared `mut`: an assignment, a `&mut`
    /// borrow or a `&mut self` method call rooted at them (rustc's `unused_mut` reads it).
    pub mut_roots: std::collections::HashSet<(usize, usize)>,
    /// `&mut place` results of `index_mut` bodies: a cell over the place (`κσ.$cell`).
    pub cell_returns: std::collections::HashSet<(usize, usize)>,
    /// `grid[i] = v` / `grid[i] += v` through `impl IndexMut`: written as `cell[0]`.
    pub index_places: std::collections::HashSet<(usize, usize)>,
    /// `&mut v[a..b]` of a Vec: a view whose writes reach `v` (`κσ.$sliceMut`), by the `&mut`.
    pub slice_views: std::collections::HashSet<(usize, usize)>,
    /// `&mut &[T]` places, by range: `&mut s` of a `&[T]` binding, which hands over a copy of its
    /// own (`*r = &r[1..]` writes into the array, which may be another binding's), and the uses
    /// of a `&mut &[u8]` binding, which `r.read(..)` advances in place instead of copying.
    pub slice_ref_muts: std::collections::HashSet<(usize, usize)>,
    /// `let r = &mut v[i];` / `&mut s.count` of a number, string, bool or `Option`: a slot over
    /// that element or field (`κσ.$placeSlot`), by the `&mut`; `r` is a slot binding.
    pub place_slots: std::collections::HashSet<(usize, usize)>,
    /// Auto-derefs (`derefs`) whose `deref_mut` returns a slot over a number or string: read
    /// and written through its `[1]`.
    pub deref_slots: std::collections::HashSet<(usize, usize)>,
    /// Expressions whose JS value is a slot over a place (`[1]` reads and writes it): a call of
    /// one of the file's functions declared `-> &mut i32`, a `&mut i32` field, `o.unwrap()` of
    /// an `Option<&mut i32>` value. Codegen reads `[1]` unless the slot itself is kept.
    pub slot_values: std::collections::HashSet<(usize, usize)>,
    /// Whole values read where they hold slots: a `Vec<&mut i32>` or `Vec<(usize, &mut i32)>` of
    /// slots, one of its tuple items, given to `{:?}`; codegen copies them out with
    /// `κσ.$readSlots(v, shape)` (`shape`: the JS form of a `SlotShape`, under `{e: ..}` for a
    /// Vec).
    pub slot_reads: HashMap<(usize, usize), String>,
    /// The same for a captured local (`{refs:?}`), by the format string's start and the name.
    pub slot_read_captures: HashMap<(usize, String), String>,
    /// Calls of a generic `-> &mut T` (`false`) / `-> Option<&mut T>` (`true`) whose `T` is a
    /// struct or collection there: the function returns a slot, read as the value it holds.
    pub slot_unwraps: HashMap<(usize, usize), bool>,
    /// What a generic `-> &mut T` (`false`) / `-> Option<&mut T>` (`true`) returns where it isn't
    /// a slot (`m.get_mut(k)` of a map): wrapped in a slot holding the value.
    pub slot_wraps: HashMap<(usize, usize), bool>,
    /// Positions that keep a slot rather than read it: the initializer of a `&mut i32`-typed
    /// `let`, the right side of `r = &mut ..`, a `&mut i32` field's value, a return of a
    /// function declared `-> &mut i32`, `Some(..)`'s payload for an `Option<&mut i32>`. A
    /// local's slot is passed on there (not `r[1]`), a box as a slot over it.
    pub slot_keeps: std::collections::HashSet<(usize, usize)>,
    /// `r = &mut other;` with `r` holding a slot or cell: the assignment rebinds it.
    pub slot_assigns: std::collections::HashSet<(usize, usize)>,
    /// Slot-valued arguments to a `&mut i32` parameter (a cell): wrapped (`κσ.$slotCell`).
    pub slot_cell_args: std::collections::HashSet<(usize, usize)>,
    /// A function value given to `for_each` over `iter_mut()` whose parameter is a cell: each
    /// element is passed as a cell over its place.
    pub fn_cell_args: std::collections::HashSet<(usize, usize)>,
    /// Struct fields written `&mut T` with `T` a number, string, bool or `Option` (by type and
    /// field): they hold slots.
    pub mut_ref_fields: std::collections::HashSet<(String, String)>,
    /// Trait method calls on a type parameter's value whose trait has impls on both an integer
    /// and a float type (or two integer types): dispatched by `T`'s class (`κσ.$dcallT`), since
    /// the run-time value can't tell a whole `f64` from an `i32`.
    pub class_dispatch: std::collections::HashSet<(usize, usize)>,
    /// Associated types a trait's default method uses as a class (`Self::Error::custom(..)`): each
    /// impl's class has a `$at$Error` getter for them.
    pub self_assoc_dicts: std::collections::HashSet<String>,
    /// Methods of `impl<T> Tr for Option<T>` (`"Option::depth"`) that use `T`'s class: a caller
    /// that knows the `Option`'s type hands them its element's (`κσ.$callElem`, `$dcallT`).
    pub elem_methods: std::collections::HashSet<String>,
    /// Numbers coerced to a `dyn Trait` whose trait has impls on two number types: tagged with
    /// their type (`κσ.$dynNum(x, "f64")`), since the value can't tell a whole `f64` from an
    /// `i32`. By the expression's range, the type's impl key.
    pub dyn_nums: HashMap<(usize, usize), String>,
    /// Method calls that go to one of several impls of a generic trait: the method's key.
    pub method_keys: HashMap<(usize, usize), String>,
    /// `a == b` / `a < b` that a `T: PartialEq<R>` / `PartialOrd<R>` bound with `R` another type
    /// allowed (`κσ.$eqTo` / `$pcmpTo`: `a`'s own impl when `b` isn't of its class).
    pub bound_cmps: std::collections::HashSet<(usize, usize)>,
    /// Auto-deref through `impl Deref`: these expressions are `x.deref()` (or `x.deref_mut()`,
    /// the `bool`), with the target type.
    pub derefs: HashMap<(usize, usize), (bool, Ty)>,
    /// The `deref()` steps before a `derefs` one, where `impl Deref` targets chain (`w.sign` with
    /// `Outer: Deref<Target = Mid>`, `Mid: Deref<Target = Inner>`): whether each is `deref_mut`.
    pub deref_steps: HashMap<(usize, usize), Vec<bool>>,
    /// `Trait::m(&x)` / `<T as Trait>::m(..)` calls, by the call's range.
    pub qualified_calls: HashMap<(usize, usize), QualifiedCall>,
    /// Generic functions that call `T::f()` / read `T::CONST` (directly or through another such
    /// function): the type parameters they take a class for, as trailing `$T` arguments.
    pub dict_params: HashMap<String, Vec<String>>,
    /// Calls of those functions: the types to pass a class for, in order, by the call's range.
    pub dict_args: HashMap<(usize, usize), Vec<Ty>>,
    /// Wrappers of those classes, innermost first, by the call's range and position: `own` for an
    /// owned `String` / `Vec<T>` whose trait also has a `str` / `[T]` impl (`κσ.$ownT(..)`), `ref` /
    /// `refmut` / `box` for a reference whose trait has a `&T` impl of its own (`κσ.$wrapT(..)`).
    pub dict_owned: HashMap<(usize, usize), Vec<Vec<String>>>,
    /// `i32::ZERO` / `Tr::ZERO` (with `Self` from the context): an associated const of a trait
    /// impl, read off the class of `Self` (the type here), by the path's range.
    pub const_paths: HashMap<(usize, usize), (Ty, String)>,
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
    /// `<T as Tr>::make()` / `Tr::make()` where a `T` is expected, for a type parameter `T`: `T`,
    /// by the call's range (emitted as `T::make()`).
    pub param_paths: HashMap<(usize, usize), String>,
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
    /// User-trait bounds' type arguments (`S: Store<T>` → `("S", "Store", [T])`).
    pub bound_args: Vec<(String, String, Vec<Ty>)>,
    /// For each parameter declared as a reference (`&str`, `&[T]`, `&Point`), the type as
    /// written; `params` erases the `&`. An owned argument there is rustc's E0308.
    pub ref_params: Vec<Option<String>>,
    /// Whether the return type is written as an owned value (no `&`, `impl`, `dyn` or type
    /// parameter): a call's result then certainly owns its value.
    pub ret_owned: bool,
    /// Whether the return type is written `&mut T` or `Option<&mut T>`: a binding of the call's
    /// result can be written through without `mut`.
    pub ret_mut_ref: bool,
    /// For `-> &mut T` / `-> Option<&mut T>` with `T` a number, string, bool or `Option`: the
    /// call returns a slot (`Some(false)`) or `null` / a slot (`Some(true)`). See
    /// `Checker::ret_slot_kind`.
    pub ret_slot: Option<bool>,
    /// Which parameters are written `Option<&mut T>` with such a `T`: `null` or a slot.
    pub opt_slot_params: Vec<bool>,
    /// The slot shape of each parameter written as a tuple holding such a `&mut T` (`t: (usize,
    /// &mut i32)`, `t: &mut (usize, &mut i32)`): a tuple literal argument keeps its slots.
    tuple_slot_params: Vec<Option<slots::SlotShape>>,
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
    /// Whether the binding certainly owns its value, as rustc sees it: a parameter declared
    /// without `&`, or a `let` with an owned type or an initializer that makes a value
    /// (`String::from(..)`, `x.clone()`, an NS result). Passing it where a parameter is `&T`
    /// is rustc's E0308. False when unknown (a pattern binding, `v[i]`, a field).
    owned: bool,
    /// Whether the value came from the JS bindings (an NS result, `ns.args`), so nothing in it
    /// is a reference: its elements and fields are owned values too.
    js: bool,
    /// The binding's name range (set by `declare`).
    decl: Option<(usize, usize)>,
}

/// The std bounds a trait method's own type parameters are checked against where a call
/// through a type parameter names their types (the file's own traits are checked too).
const CHECKED_OWN_BOUNDS: &[&str] = &[
    "Default", "Clone", "Copy", "Debug", "Display", "PartialEq", "Eq", "PartialOrd", "Ord", "Hash",
];

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
            file_root: Default::default(),
            vars: Vec::new(),
            raw: HashMap::new(),
            diags: Vec::new(),
            fns: HashMap::new(),
            scopes: Vec::new(),
            ret_ty: Ty::Unit,
            ret_declared: true,
            fn_name: String::new(),
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
            std_array_dims: HashMap::new(),
            crate_uses: Default::default(),
            consts: HashMap::new(),
            const_vals: HashMap::new(),
            root_file: None,
            param_bounds: HashMap::new(),
            method_where: Vec::new(),
            where_sub: None,
            into_targets: HashMap::new(),
            nested_ok: Default::default(),
            slot_locals: HashMap::new(),
            closure_slots: Vec::new(),
            slot_options: HashMap::new(),
            slot_results: HashMap::new(),
            pending_bounds: Vec::new(),
            pending_arg_bounds: Vec::new(),
            adt_bounds: Vec::new(),
            loops: Vec::new(),
            boxed: Default::default(),
            cell_args: Default::default(),
            slot_args: Default::default(),
            temp_cells: Default::default(),
            into_args: Default::default(),
            slot_calls: Default::default(),
            slot_bindings: Default::default(),
            mut_ref_loops: Default::default(),
            copy_arrays: Default::default(),
            copy_array_consts: Default::default(),
            copy_array_assoc: Default::default(),
            twin_sides: HashMap::new(),
            twin_targs: HashMap::new(),
            dyn_twins: Default::default(),
            str_writers: Default::default(),
            const_paths: HashMap::new(),
            str_locals: Default::default(),
            ref_vecs: Default::default(),
            move_sites: Default::default(),
            trait_calls: Vec::new(),
            io_writes: Default::default(),
            slot_item_calls: Default::default(),
            lock_results: Default::default(),
            any_strs: Default::default(),
            any_checks: Default::default(),
            crate_name: String::new(),
            fmt_writes: Default::default(),
            array_fields: Default::default(),
            shared_ref_fields: Default::default(),
            rc_fields: Default::default(),
            array_ann_fields: Default::default(),
            fn_ptr_fields: Default::default(),
            array_reads: Default::default(),
            iter_of_args: Default::default(),
            mut_roots: Default::default(),
            cell_returns: Default::default(),
            index_places: Default::default(),
            slice_views: Default::default(),
            slice_ref_muts: Default::default(),
            place_slots: Default::default(),
            deref_slots: Default::default(),
            slot_values: Default::default(),
            slot_reads: Default::default(),
            slot_read_captures: Default::default(),
            slot_items: HashMap::new(),
            whole_slot_items: Default::default(),
            slot_unwraps: Default::default(),
            slot_wraps: Default::default(),
            ret_item_slots: Default::default(),
            ret_slot_param: false,
            slot_param_ok: false,
            slot_keeps: Default::default(),
            slot_assigns: Default::default(),
            slot_cell_args: Default::default(),
            fn_cell_args: Default::default(),
            mut_ref_fields: Default::default(),
            mut_borrow_fields: Default::default(),
            class_dispatch: Default::default(),
            self_assoc_dicts: Default::default(),
            elem_methods: Default::default(),
            dyn_num_coercions: Vec::new(),
            ret_slot: None,
            pending_cell_args: Vec::new(),
            method_keys: Default::default(),
            bound_cmps: Default::default(),
            derefs: Default::default(),
            deref_steps: Default::default(),
            qualified_calls: Default::default(),
            pending_qualified: Vec::new(),
            cur_fn: None,
            dict_uses: Default::default(),
            dbg_uses: Default::default(),
            const_tys: HashMap::new(),
            from_args: HashMap::new(),
            try_from_args: HashMap::new(),
            borrow_args: HashMap::new(),
            const_needs: Default::default(),
            const_derivable: Default::default(),
            const_lits: Vec::new(),
            ctor_sites: Vec::new(),
            let_types: Vec::new(),
            crate_calls: HashMap::new(),
            crate_items: crates::crate_imports(src),
            stored_uses: Default::default(),
            param_froms: HashMap::new(),
            param_paths: HashMap::new(),
            unbound_targs: Default::default(),
            generic_calls: Vec::new(),
            last_type_args: Vec::new(),
            ptrs: Default::default(),
            quote_vars: Default::default(),
            quote_reps: Default::default(),
            pending_json: Vec::new(),
            pending_tries: Vec::new(),
            pending_string_froms: Vec::new(),
            js_calls: HashMap::new(),
            js_imports: HashMap::new(),
            opaque_index_mut: Default::default(),
            static_muts: Default::default(),
            macro_in_unsafe: false,
            enum_payloads: false,
            statics: Default::default(),
            modules: Vec::new(),
            module_exports: HashMap::new(),
            imported: Vec::new(),
            pending_intos: Vec::new(),
            pending_try_intos: Vec::new(),
            std_fns: HashMap::new(),
            std_type_imports: Default::default(),
            imported_names: Default::default(),
            glob_imports: false,
            assoc_output: None,
            impl_assoc: HashMap::new(),
            param_trait_args: HashMap::new(),
            param_trait_alts: HashMap::new(),
            pending_bound_picks: Vec::new(),
            op_args: HashMap::new(),
            cmp_args: HashMap::new(),
            lenient_projs: Default::default(),
            pending_projections: Vec::new(),
            pending_trait_args: Vec::new(),
            file_trait_names: Default::default(),
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
        self.root_file = Some(file.clone());
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
                                    && self.cell_param(t)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let ret = f
                    .ret_type()
                    .and_then(|r| r.ty())
                    .map(|t| self.lower_ret_type(&t))
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
                let mut bound_args: Vec<(String, String, Vec<Ty>)> = self
                    .param_trait_args
                    .iter()
                    .flat_map(|(p, per)| {
                        per.iter()
                            .filter(|(_, (pos, _))| !pos.is_empty())
                            .map(|(tr, (pos, _))| (p.clone(), tr.clone(), pos.clone()))
                    })
                    .collect();
                bound_args.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
                self.leave_generics();
                let ref_params = f
                    .param_list()
                    .map(|pl| pl.params().map(|p| p.ty().as_ref().and_then(written_ref)).collect())
                    .unwrap_or_default();
                let ret_owned = f.ret_type().and_then(|r| r.ty()).is_some_and(|t| {
                    written_owned(&t) && !matches!(ret, Ty::Param(_) | Ty::Dyn(_, _) | Ty::Unknown)
                });
                let ret_mut_ref = f.ret_type().and_then(|r| r.ty()).is_some_and(|t| written_mut_ref(&t));
                let ret_slot = self.ret_slot_kind(f, &ret);
                let opt_slot_params = self.opt_slot_params(f, &params);
                let tuple_slot_params = f
                    .param_list()
                    .map(|pl| {
                        pl.params()
                            .map(|p| p.ty().and_then(|t| self.param_tuple_slot_shape(&t)))
                            .collect()
                    })
                    .unwrap_or_default();
                self.note_ret_item_slots(f, name.text().to_string());
                let sig = FnSig {
                    params,
                    ret,
                    is_async: f.async_token().is_some(),
                    type_params,
                    bound_args,
                    module: None,
                    cell_params,
                    into_params: into_param_flags(f),
                    ref_params,
                    ret_owned,
                    ret_mut_ref,
                    ret_slot,
                    opt_slot_params,
                    tuple_slot_params,
                };
                if self
                    .fns
                    .get(name.text().to_string().as_str())
                    .is_some_and(|s| s.module.is_none())
                {
                    self.error(
                        name.syntax(),
                        format!("the name `{}` is defined multiple times", name.text()),
                    );
                }
                self.fns.insert(name.text().to_string(), sig);
            }
        }
        self.check_consts(file);
    }
}

/// Where `src` came from: the preprocessor's map (if it was expanded) and the program's own
/// length, which together tell generated code (expansions, bundled preludes) from the user's.
#[derive(Clone, Default)]
pub struct Origin {
    pub map: Option<std::rc::Rc<crate::mbe::OffsetMap>>,
    pub len: usize,
    /// The program as written (what `map` maps back to).
    pub src: std::rc::Rc<str>,
}

impl Origin {
    /// Whether the offset is in code the compiler wrote.
    pub fn generated(&self, off: usize) -> bool {
        match &self.map {
            Some(m) => m.generated(off, self.len),
            None => off >= self.len,
        }
    }
}

/// Type-check `file`. `filename` is its path (for finding `mod` files) and `resolver` reads other
/// files.
pub fn check(
    file: &ast::SourceFile,
    src: &str,
    filename: &str,
    resolver: Resolver,
    origin: &Origin,
) -> (Types, Vec<RawDiag>) {
    let mut c = Checker::new(src);
    c.crate_name = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .trim_end_matches(".rs")
        .replace('-', "_");
    c.declare_items(file, filename, resolver, &mut vec![filename.to_string()]);
    c.scan_index_mut(file);
    for item in crate::adt::all_items(file) {
        if let ast::Item::Fn(f) = &item {
            c.check_fn(f);
        }
    }
    c.check_impls(file);
    c.check_trait_defaults(file);
    c.check_trait_scopes(file, origin);
    c.check_twin_paths();
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
            help: None,
            spans: Vec::new(),
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
            help: None,
            spans: Vec::new(),
        });
    }
    // Moves are only checked in programs that type-check: they rely on the types.
    if c.diags.iter().all(|d| d.severity != "error") {
        let (moves, sites) = crate::moves::check(file, src, &map, &c.adts, &c.copy_arrays);
        c.diags.extend(moves);
        c.move_sites = sites;
        let borrows = crate::nll::check(file, src, &map, &c.adts, origin);
        c.diags.extend(borrows);
    }
    if let Some((&(start, end), ty)) = map
        .iter()
        .filter_map(|(k, t)| Some((k, c.unordered_coll(t, file, k.0)?)))
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
            help: None,
            spans: Vec::new(),
        });
    }
    // (Before the definitions move out: methods' type parameters are looked up there.)
    c.check_const_args();
    let dict_uses = std::mem::take(&mut c.dict_uses);
    c.store_default_params();
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
    let dict_owned = c.finish_twin_dicts(&dict_params, &dict_args);
    let const_paths: HashMap<(usize, usize), (Ty, String)> = std::mem::take(&mut c.const_paths)
        .into_iter()
        .map(|(r, (t, k))| (r, (c.zonk(&t), k)))
        .collect();
    let dyn_nums = c.finish_dyn_nums();
    let adts = std::mem::take(&mut c.adts);
    let mut_bindings = std::mem::take(&mut c.mut_bindings);
    let boxed = std::mem::take(&mut c.boxed);
    let cell_args = std::mem::take(&mut c.cell_args);
    let slot_args = std::mem::take(&mut c.slot_args);
    let temp_cells = std::mem::take(&mut c.temp_cells);
    let into_args = std::mem::take(&mut c.into_args);
    let slot_calls = std::mem::take(&mut c.slot_calls);
    let slot_bindings = std::mem::take(&mut c.slot_bindings);
    let mut_ref_loops = std::mem::take(&mut c.mut_ref_loops);
    let array_reads = std::mem::take(&mut c.array_reads);
    let iter_of_args = std::mem::take(&mut c.iter_of_args);
    let cell_returns = std::mem::take(&mut c.cell_returns);
    let index_places = std::mem::take(&mut c.index_places);
    let slice_views = std::mem::take(&mut c.slice_views);
    let slice_ref_muts = std::mem::take(&mut c.slice_ref_muts);
    let place_slots = std::mem::take(&mut c.place_slots);
    let deref_slots = std::mem::take(&mut c.deref_slots);
    let slot_values = std::mem::take(&mut c.slot_values);
    let slot_reads = std::mem::take(&mut c.slot_reads);
    let slot_read_captures = std::mem::take(&mut c.slot_read_captures);
    let slot_unwraps = std::mem::take(&mut c.slot_unwraps);
    let slot_wraps = std::mem::take(&mut c.slot_wraps);
    let slot_keeps = std::mem::take(&mut c.slot_keeps);
    let slot_assigns = std::mem::take(&mut c.slot_assigns);
    let slot_cell_args = std::mem::take(&mut c.slot_cell_args);
    let fn_cell_args = std::mem::take(&mut c.fn_cell_args);
    let mut_ref_fields = std::mem::take(&mut c.mut_ref_fields);
    let class_dispatch = std::mem::take(&mut c.class_dispatch);
    let self_assoc_dicts = std::mem::take(&mut c.self_assoc_dicts);
    let elem_methods = std::mem::take(&mut c.elem_methods);
    let method_keys = std::mem::take(&mut c.method_keys);
    let bound_cmps = std::mem::take(&mut c.bound_cmps);
    let derefs = std::mem::take(&mut c.derefs);
    let deref_steps = std::mem::take(&mut c.deref_steps);
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
    let param_paths = std::mem::take(&mut c.param_paths);
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
            slot_item_calls: std::mem::take(&mut c.slot_item_calls),
            any_strs: std::mem::take(&mut c.any_strs),
            any_checks: std::mem::take(&mut c.any_checks),
            fmt_writes: std::mem::take(&mut c.fmt_writes),
            boxed,
            cell_args,
            slot_args,
            temp_cells,
            into_args,
            slot_calls,
            slot_bindings,
            mut_ref_loops,
            array_reads,
            iter_of_args,
            mut_roots: std::mem::take(&mut c.mut_roots),
            cell_returns,
            index_places,
            slice_views,
            slice_ref_muts,
            place_slots,
            deref_slots,
            slot_values,
            slot_reads,
            slot_read_captures,
            slot_unwraps,
            slot_wraps,
            slot_keeps,
            slot_assigns,
            slot_cell_args,
            fn_cell_args,
            mut_ref_fields,
            class_dispatch,
            self_assoc_dicts,
            elem_methods,
            dyn_nums,
            method_keys,
            bound_cmps,
            derefs,
            deref_steps,
            qualified_calls,
            dict_params,
            dict_args,
            dict_owned,
            const_paths,
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
            param_paths,
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
    /// The whole file's syntax, parsed again when a re-parsed macro argument names a local
    /// declared outside it (see `typeck_twins`).
    file_root: std::cell::OnceCell<SyntaxNode>,
    vars: Vec<VarState>,
    raw: HashMap<(usize, usize), Ty>,
    diags: Vec<RawDiag>,
    fns: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, Local>>,
    ret_ty: Ty,
    /// Whether the function being checked writes a return type (`-> T`); a `return e;` or a
    /// tail value in one that doesn't gets the "no return type" message.
    ret_declared: bool,
    /// The name of the function being checked, for that message.
    fn_name: String,
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
    /// Arrays of known length that std methods return (`x.to_le_bytes()`), by the call's range:
    /// their dimensions, for `let [a, b] = ..` refutability.
    std_array_dims: HashMap<ra_ap_syntax::TextRange, Vec<usize>>,
    /// `uses_crate`'s answers, by crate name (the source doesn't change during a check).
    crate_uses: std::cell::RefCell<HashMap<String, bool>>,
    /// `const` and `static` items.
    consts: HashMap<String, Ty>,
    /// Other files' `const` items' values (also as `module.NAME`), when they're constant
    /// integers or `bool`s: for const arguments (`Buf<{ util::SIZE * 2 }>`).
    pub(super) const_vals: HashMap<String, crate::consteval::CVal>,
    /// The file being checked: the items a const argument re-parsed out of a macro call
    /// (`println!("{}", f::<{ N * 2 }>())`) refers to.
    root_file: Option<ast::SourceFile>,
    /// Bounds of the current function's type parameters.
    param_bounds: HashMap<String, Vec<String>>,
    /// The impl's parameters a method's `where` clause bounds further (`fn log(&self) where T:
    /// Display`), with their bounds before it, put back after the method.
    method_where: Vec<(String, Vec<String>)>,
    /// Trait arguments for the next `call_trait_method`, from a `where Meters: Scale<R>` clause
    /// (see `where_self_method`).
    where_sub: Option<HashMap<String, Ty>>,
    /// `T: Into<f64>`: what `.into()` on a `T` makes (a number type; `T` stays generic).
    into_targets: HashMap<String, Ty>,
    /// Expressions of type `Option<Option<T>>` that only feed a `.flatten()`.
    nested_ok: std::collections::HashSet<(usize, usize)>,
    /// Locals holding an iterator of slots (`false`) or a Vec of them (`true`), by name, with
    /// their items' shape (see `typeck_slots.rs`).
    slot_locals: HashMap<String, (bool, slots::SlotShape)>,
    /// The parameters of the next closure checked that receive slot items (`it.filter(|x| ..)`
    /// over a slot iterator): (parameter index, shape).
    closure_slots: Vec<(usize, slots::SlotShape)>,
    /// Locals holding an `Option` of slot items (`let big = v.iter_mut().find(..)`).
    slot_options: HashMap<String, slots::SlotShape>,
    /// Locals holding a `Result` of a slot (`let r = m.get_mut(k).ok_or(e);`).
    slot_results: HashMap<String, slots::SlotShape>,
    /// Locals holding one tuple item with slots in it (`for t in &tagged`, `let t = &mut
    /// tagged[1];` of a `Vec<(usize, &mut i32)>`): `t.1` is a slot.
    slot_items: HashMap<String, slots::SlotShape>,
    /// Whole items of a `Vec<(_, &mut T)>` of slots used where that's supported (`let t = &rs[1]`,
    /// `{:?}` of `rs[1]`).
    whole_slot_items: std::collections::HashSet<(usize, usize)>,
    /// Bounds to check once a generic call's type arguments are known.
    pending_bounds: Vec<(Ty, String, SyntaxNode, usize)>,
    /// A turbofish's type with a bound that takes type arguments (`O::widen::<u8>` for
    /// `R: Into<i64>`), checked once it is known.
    pending_arg_bounds: Vec<(Ty, crate::adt::ArgBound, SyntaxNode, usize)>,
    /// New values' type arguments with the user-trait bounds their struct declares on them
    /// (`struct Auto<S: RawStream>`), to infer from (`Auto::new(Vec::new())`).
    adt_bounds: Vec<(Ty, String)>,
    boxed: std::collections::HashSet<(usize, usize)>,
    cell_args: std::collections::HashSet<(usize, usize)>,
    slot_args: std::collections::HashSet<(usize, usize)>,
    temp_cells: std::collections::HashSet<(usize, usize)>,
    into_args: std::collections::HashSet<(usize, usize)>,
    slot_calls: std::collections::HashSet<(usize, usize)>,
    slot_bindings: std::collections::HashSet<(usize, usize)>,
    mut_ref_loops: std::collections::HashSet<(usize, usize)>,
    /// Locals and parameters that are fixed-size arrays of `Copy` values, by declaration.
    copy_arrays: std::collections::HashSet<(usize, usize)>,
    /// `const` / `static` items that are fixed-size arrays of `Copy` values: a read copies.
    pub(super) copy_array_consts: std::collections::HashSet<String>,
    /// Associated `const`s that are fixed-size arrays of `Copy` values, by type and name.
    pub(super) copy_array_assoc: std::collections::HashSet<(String, String)>,
    /// Path calls that may reach a `str` / `String` (or `[T]` / `Vec<T>`) pair of impls (see
    /// `Adts::own_twins`): the method, and whether the call is for the owned type (None: unknown).
    twin_sides: HashMap<(usize, usize), (String, Option<bool>)>,
    /// Generic calls whose type parameter has a bound with `str` / `String` (or `[T]` / `Vec<T>`)
    /// twin impls: per parameter, whether its argument is the owned type (None: unknown), and the
    /// argument's range. The owned type's class goes as `κσ.$ownT(..)` (see `finish_twin_dicts`).
    twin_targs: HashMap<(usize, usize), Vec<(String, Option<bool>, (usize, usize), Vec<&'static str>)>>,
    /// Owned strings and vectors coerced to a `dyn Trait` with twin impls, by range.
    dyn_twins: std::collections::HashSet<(usize, usize)>,
    /// Strings and `Formatter`s coerced to a `dyn fmt::Write`, by range (see `dyn_nums`).
    str_writers: std::collections::HashSet<(usize, usize)>,
    /// See `Types::const_paths` (zonked when the file is done).
    const_paths: HashMap<(usize, usize), (Ty, String)>,
    /// `let` locals whose initializer is certainly a `&str` (`let s = "a";`, `let t = s;`,
    /// `let u = x.as_str();`), by declaration: `s + ".."` is rustc's E0369.
    str_locals: std::collections::HashSet<(usize, usize)>,
    /// Locals and parameters declared `Vec<&T>` / `VecDeque<&T>` (also behind `&mut`), by
    /// declaration, with `&T` as written: what they're given must be a reference.
    ref_vecs: HashMap<(usize, usize), String>,
    /// Uses that move a binding (filled by the move checker).
    move_sites: std::collections::HashSet<(usize, usize)>,
    /// Method calls a user trait's impl may answer (see `traitscope`).
    trait_calls: Vec<scope::TraitCall>,
    /// `write!` destinations that are `io::Write`rs (by their start).
    io_writes: std::collections::HashSet<usize>,
    /// See `Types::any_strs` / `any_checks`.
    slot_item_calls: std::collections::HashSet<(usize, usize)>,
    /// Locals holding an unwrapped `m.lock()` (`let r = m.lock();`): matched later, the `Ok`
    /// binding aliases the lock's value, as with the call itself.
    lock_results: std::collections::HashSet<String>,
    any_strs: HashMap<(usize, usize), String>,
    any_checks: HashMap<(usize, usize), Ty>,
    /// The crate's name (the file's stem), which `type_name` puts before a struct's name.
    crate_name: String,
    /// `write!` destinations that are `fmt::Write`rs (your own, or a type parameter's).
    fmt_writes: std::collections::HashSet<usize>,
    /// Struct fields declared as fixed-size arrays of `Copy` values: (struct, field).
    pub(super) array_fields: std::collections::HashSet<(String, String)>,
    /// Fields declared as shared references (`&'a [u8]`, `Option<&T>`): `Copy` and `Clone`
    /// whatever they point at. (struct or variant, field)
    pub(super) shared_ref_fields: std::collections::HashSet<(String, String)>,
    /// Fields declared as `Rc<T>` / `Arc<T>`: `Clone` whatever `T` is. (struct or variant, field)
    pub(super) rc_fields: std::collections::HashSet<(String, String)>,
    /// Fields declared as `[T; N]`: `Copy` when `T` is. (struct or variant, field)
    pub(super) array_ann_fields: std::collections::HashSet<(String, String)>,
    /// Fields declared as function pointers (`fn(u8) -> u8`, `Option<fn()>`): typed as closures,
    /// but `Copy`, `Clone`, `Debug` and `PartialEq`. (struct or variant, field)
    pub(super) fn_ptr_fields: std::collections::HashSet<(String, String)>,
    array_reads: std::collections::HashSet<(usize, usize)>,
    iter_of_args: std::collections::HashSet<(usize, usize)>,
    mut_roots: std::collections::HashSet<(usize, usize)>,
    cell_returns: std::collections::HashSet<(usize, usize)>,
    index_places: std::collections::HashSet<(usize, usize)>,
    slice_views: std::collections::HashSet<(usize, usize)>,
    slice_ref_muts: std::collections::HashSet<(usize, usize)>,
    place_slots: std::collections::HashSet<(usize, usize)>,
    deref_slots: std::collections::HashSet<(usize, usize)>,
    slot_values: std::collections::HashSet<(usize, usize)>,
    slot_reads: HashMap<(usize, usize), String>,
    slot_read_captures: HashMap<(usize, String), String>,
    slot_unwraps: HashMap<(usize, usize), bool>,
    slot_wraps: HashMap<(usize, usize), bool>,
    /// Functions (by name) and methods (`Adt::name`) declared `-> impl Iterator<Item = (usize,
    /// &mut T)>`: where their items hold slots.
    ret_item_slots: HashMap<String, slots::SlotShape>,
    /// The function being checked is declared `-> &mut T` / `-> Option<&mut T>` with `T` a type
    /// parameter (`ret_slot` is set): it returns a slot on every path (`keep_generic_ret`).
    ret_slot_param: bool,
    /// While marking what a function declared `-> &mut T` returns: a type parameter `T` counts
    /// as a slot referent (`slot_elem`).
    slot_param_ok: bool,
    slot_keeps: std::collections::HashSet<(usize, usize)>,
    slot_assigns: std::collections::HashSet<(usize, usize)>,
    slot_cell_args: std::collections::HashSet<(usize, usize)>,
    fn_cell_args: std::collections::HashSet<(usize, usize)>,
    pub(super) mut_ref_fields: std::collections::HashSet<(String, String)>,
    /// Every field written `&mut T` (`table: &'a mut Table`), by (struct, field): a write
    /// through one needs no `mut` on the binding that holds the struct.
    pub(super) mut_borrow_fields: std::collections::HashSet<(String, String)>,
    pub(super) class_dispatch: std::collections::HashSet<(usize, usize)>,
    pub(super) self_assoc_dicts: std::collections::HashSet<String>,
    pub(super) elem_methods: std::collections::HashSet<String>,
    /// Number-typed expressions checked against a `dyn Trait`, with that trait (see `dyn_nums`).
    dyn_num_coercions: Vec<((usize, usize), Ty, String)>,
    /// What the function being checked returns when declared `-> &mut T` / `-> Option<&mut T>`
    /// with `T` a number, string, bool or `Option`: a slot (`Some(false)`) or `None` / a slot
    /// (`Some(true)`); `None` otherwise.
    ret_slot: Option<bool>,
    /// Arguments to `&mut i32` parameters that are neither `&mut x` nor a `&mut` local, checked
    /// after the call's arguments are inferred (a slot-valued call, `o.unwrap()`).
    pending_cell_args: Vec<ast::Expr>,
    method_keys: HashMap<(usize, usize), String>,
    bound_cmps: std::collections::HashSet<(usize, usize)>,
    derefs: HashMap<(usize, usize), (bool, Ty)>,
    deref_steps: HashMap<(usize, usize), Vec<bool>>,
    qualified_calls: HashMap<(usize, usize), QualifiedCall>,
    ptrs: Ptrs,
    quote_vars: HashMap<(usize, usize), QuoteTok>,
    quote_reps: HashMap<usize, Vec<String>>,
    /// `JSON::stringify(&v)` / `JSON::parse::<T>(s)` types to check once settled: (type, node,
    /// whether parsing).
    pending_json: Vec<(Ty, SyntaxNode, bool)>,
    /// `?`s whose error type wasn't known yet: (the `?`, its error, the function's).
    pending_tries: Vec<(ast::TryExpr, Ty, Ty)>,
    /// `String::from` as a value with your `impl From<A> for String`: (the path, its argument).
    pending_string_froms: Vec<(SyntaxNode, Ty)>,
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
    /// `T: TryFrom<X>` bounds in scope: each `X` by parameter.
    pub(super) try_from_args: HashMap<String, Vec<Ty>>,
    /// `K: Borrow<Q>` bounds in scope: `Q` by parameter, the key a map of `K`s is looked up by.
    pub(super) borrow_args: HashMap<String, Ty>,
    /// The const generic parameters in scope (`const N: usize`), with their types.
    pub(super) const_tys: HashMap<String, Ty>,
    /// (function, const parameter) pairs: each is passed like a class (`$N`)...
    const_needs: std::collections::HashSet<(String, String)>,
    /// ...unless the function can read it off an argument (`a: [T; N]` gives `a.length`).
    const_derivable: std::collections::HashSet<(String, String)>,
    /// `T::from(x)` calls: `X`'s name by range.
    param_froms: HashMap<(usize, usize), String>,
    /// `<T as Tr>::make()` calls: `T` by range.
    param_paths: HashMap<(usize, usize), String>,
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
    /// Types whose `index_mut` doesn't end in `&mut <place>`: assigning through them can't work.
    opaque_index_mut: std::collections::HashSet<String>,
    /// `static mut` items, by name: using one takes an `unsafe` block or function.
    static_muts: std::collections::HashSet<String>,
    /// Checking the arguments of a macro called in an `unsafe` block (they are parsed on their
    /// own, so the block isn't among their ancestors).
    macro_in_unsafe: bool,
    /// Checking an NS call's argument: its array and tuple literals take a variant's payload where
    /// a bindings enum is expected (`ns.flags(&[("target", "home")])`), as the argument itself does.
    enum_payloads: bool,
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
    /// std types imported by name (`use std::cell::UnsafeCell;`), modeled or not: an unknown
    /// one is "not supported" rather than "cannot find".
    std_type_imports: std::collections::HashSet<String>,
    /// Every name a `use` brings in (`use std::collections::HashMap;`, `use hashbrown::HashMap as
    /// Map;`, a `mod` file's items), and whether some `use` is a glob (then nothing is known to be
    /// missing). For rustc's E0433 on a std type used without its import.
    imported_names: std::collections::HashSet<String>,
    glob_imports: bool,
    /// `type Output = ..;` of the operator impl being declared.
    assoc_output: Option<Ty>,
    /// The `impl`'s associated types (`type Err = String;`), for `Self::Err` in its signatures.
    pub(super) impl_assoc: HashMap<String, Ty>,
    /// Each type parameter's user-trait bounds with their arguments: (positional, `Unit = X`).
    pub(super) param_trait_args: HashMap<String, BoundArgs>,
    /// A type parameter bounded by one generic trait more than once (`C: Conv<u8> + Conv<u16>`):
    /// each bound's positional arguments, by (parameter, trait). `param_trait_args` keeps one.
    pub(super) param_trait_alts: HashMap<(String, String), Vec<Vec<Ty>>>,
    /// Calls through such a parameter: `C: Conv` (for the message), the trait arguments' variables, each bound's arguments
    /// and the call, settled once the body is checked (`resolve_bound_picks`).
    pub(super) pending_bound_picks: Vec<(String, Vec<Ty>, Vec<Vec<Ty>>, SyntaxNode)>,
    /// The right-hand sides of std operator bounds, by (parameter, trait): `L: Add<R>` is
    /// (`L`, `Add`) → `R`.
    pub(super) op_args: HashMap<(String, String), Ty>,
    /// `L: PartialEq<R>` / `PartialOrd<R>` bounds (and `where I::Item: PartialEq<J::Item>`), by
    /// (parameter or projection, trait): `R` as written, lowered where `a == b` compares.
    pub(super) cmp_args: HashMap<(String, String), ast::Type>,
    /// Projections a `where` clause constrains (`where I::Item: PartialEq<T>`): unchecked.
    pub(super) lenient_projs: std::collections::HashSet<String>,
    /// Generic calls' projections waiting for their type argument: (`S`'s variable, trait,
    /// associated type, the projection's variable).
    pub(super) pending_projections: Vec<(Ty, String, String, Ty)>,
    /// Generic calls' bound arguments waiting for their type argument (`S: Store<T>`): (`S`'s
    /// variable, trait, the arguments' types).
    pub(super) pending_trait_args: Vec<(Ty, String, Vec<Ty>)>,
    /// The file's trait names, known before its traits are collected: struct fields lowered
    /// first may name them (`struct Scene(Vec<Box<dyn Shape>>)`).
    pub(super) file_trait_names: std::collections::HashSet<String>,
    conversions: HashMap<(usize, usize), crate::adt::Conv>,
}

const USIZE: Ty = Ty::Int(IntKind::Usize);

/// Std and prelude type names the checker knows, for "a type with a similar name exists" on a
/// misspelled one (`Strng`, `Hashmap`).
const KNOWN_STD_TYPES: &[&str] = &[
    "String", "Vec", "Option", "Result", "Box", "Rc", "Arc", "RefCell", "Cell", "HashMap",
    "HashSet", "BTreeMap", "BTreeSet", "VecDeque", "BinaryHeap", "Ordering", "Duration",
    "Instant", "PathBuf", "Cow", "PhantomData", "usize", "isize", "u8", "u16", "u32", "u64",
    "u128", "i8", "i16", "i32", "i64", "i128", "f32", "f64", "bool", "char", "str",
];

/// Standard library types the checker has no model for, so a bare name (brought in by a `use`
/// the checker didn't keep) is "not supported" rather than "cannot find".
fn is_unmodeled_std_type(name: &str) -> bool {
    matches!(
        name,
        "UnsafeCell"
            | "ManuallyDrop"
            | "MaybeUninit"
            | "NonNull"
            | "PhantomPinned"
            | "Condvar"
            | "Barrier"
            | "Once"
            | "Weak"
            | "JoinHandle"
            | "Thread"
            | "TcpStream"
            | "TcpListener"
            | "UdpSocket"
            | "AtomicPtr"
    )
}

impl<'a> Checker<'a> {
    // ------------------------------------------------------------ positions & diagnostics

    fn range(&self, n: &SyntaxNode) -> (usize, usize) {
        let r = n.text_range();
        (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        )
    }

    /// The `fn` `n` is in, also for a node of a macro's re-parsed arguments (`format!("{}",
    /// m.scale(r))`), found in the file by its offset.
    pub(super) fn enclosing_fn(&self, n: &SyntaxNode) -> Option<ast::Fn> {
        if let Some(f) = n.ancestors().find_map(ast::Fn::cast) {
            return Some(f);
        }
        let at = ra_ap_syntax::TextSize::try_from(self.range(n).0).ok()?;
        self.root_file
            .as_ref()?
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|f| f.syntax().text_range().contains(at))
            .last()
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
            help: None,
            spans: Vec::new(),
        });
    }

    /// `ns.hack("x");` / `work();` with `async fn work()`: a call's future dropped as a
    /// statement never runs (rustc's `unused_must_use`). In Bitburner the script goes on, and
    /// its next NS call fails because this one is still pending.
    fn warn_dropped_future(&mut self, e: &ast::Expr, t: &Ty) {
        // (Also an `async { .. };` block.)
        let block = matches!(e, ast::Expr::BlockExpr(b) if b.async_token().is_some());
        if !(block || matches!(e, ast::Expr::CallExpr(_) | ast::Expr::MethodCallExpr(_)))
            || !matches!(self.shallow(t), Ty::Future(_))
        {
            return;
        }
        let (start, end) = self.range(e.syntax());
        self.diags.push(RawDiag {
            start,
            end,
            message: "unused implementer of `Future` that must be used".into(),
            severity: "warning",
            help: Some(format!("futures do nothing unless you `.await` them: write `.await` after the {} (an NS call left pending makes the script's next NS call fail)", if block { "block" } else { "call" })),
            spans: Vec::new(),
        });
    }

    fn error(&mut self, n: &SyntaxNode, message: impl Into<String>) {
        let (start, end) = self.range(n);
        self.diags.push(RawDiag {
            start,
            end,
            message: message.into(),
            severity: "error",
            help: None,
            spans: Vec::new(),
        });
    }

    /// rustc's "expected `()`, found `T`" for a value in a function with no `-> T`, saying where
    /// the `()` comes from.
    fn missing_ret_msg(&self, found: &str) -> String {
        let suggest = found.replace("{integer}", "i32").replace("{float}", "f64");
        format!(
            "mismatched types: expected `()`, found `{found}` (try adding a return type: `-> {suggest}`)"
        )
    }

    /// An error with its rule spelled out for a player who doesn't know Rust (`diag_help.rs`).
    fn error_help(&mut self, n: &SyntaxNode, message: impl Into<String>, help: &str) {
        let (start, end) = self.range(n);
        self.diags.push(RawDiag {
            start,
            end,
            message: message.into(),
            severity: "error",
            help: Some(help.to_string()),
            spans: Vec::new(),
        });
    }

    /// An error at `start..end` with rustc's secondary spans: (start, end, label).
    pub(super) fn error_spans(
        &mut self,
        (start, end): (usize, usize),
        message: impl Into<String>,
        help: Option<&str>,
        spans: Vec<(usize, usize, String)>,
    ) {
        self.diags.push(RawDiag {
            start,
            end,
            message: message.into(),
            severity: "error",
            help: help.map(str::to_string),
            spans,
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

    /// Of `impl PartialEq<R> for T` (or `PartialOrd`) impls, by `T`'s key: the method key and `R`
    /// of the one `rhs` fits; an untyped literal prefers `i32` / `f64`, as rustc's fallback does.
    fn pick_cmp_impl(&self, k: &str, m: &str, rhs: &Ty) -> Option<(String, Ty)> {
        let keys = self
            .adts
            .method_alts
            .get(&format!("{k}::{m}"))
            .cloned()
            .unwrap_or_else(|| vec![m.to_string()]);
        let fits: Vec<(String, Ty)> = keys
            .into_iter()
            .filter_map(|key| {
                let p = self.adts.method(k, &key)?.params.first()?.clone();
                // (`impl<T: PartialEq<U>, U> PartialEq<U> for Sp<T>`: any `U`.)
                let mut ps = Vec::new();
                p.collect_params(&mut ps);
                let any: HashMap<String, Ty> =
                    ps.into_iter().map(|n| (n, Ty::Unknown)).collect();
                let p = generic::subst(&p, &any);
                self.could_be(&p, rhs).then_some((key, p))
            })
            .collect();
        let rt = self.shallow(rhs);
        let preferred = match (&rt, self.var_kind(&rt)) {
            (Ty::IntLit, _) | (_, Some(VarKind::Int)) => Some(Ty::Int(IntKind::I32)),
            (_, Some(VarKind::Float)) => Some(Ty::Float { f32: false }),
            _ => None,
        };
        let exact = fits.iter().position(|(_, p)| {
            *p == rt || preferred.as_ref().is_some_and(|w| p == w)
        });
        let mut fits = fits;
        match exact {
            Some(i) => Some(fits.swap_remove(i)),
            None => fits.into_iter().next(),
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
            Ty::MutRef(t) => Ty::MutRef(Box::new(self.zonk(&t))),
            Ty::Adt(n, args) => Ty::Adt(n, args.iter().map(|a| self.zonk(a)).collect()),
            Ty::Dyn(n, args) => Ty::Dyn(n, args.iter().map(|a| self.zonk(a)).collect()),
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

    /// Whether the variable `v` appears in `t`.
    fn occurs(&self, v: u32, t: &Ty) -> bool {
        match self.shallow(t) {
            Ty::Var(x) => x == v,
            t => t.inner_types().iter().any(|i| self.occurs(v, i)),
        }
    }

    fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let (a, b) = (self.shallow(a), self.shallow(b));
        // (A closure's `&mut i32` parameter is an `i32`, passed in a cell.)
        if let Ty::MutRef(x) = &a {
            return self.unify(x, &b);
        }
        if let Ty::MutRef(y) = &b {
            return self.unify(&a, y);
        }
        // `B::Item` where `B: Array<Item = A::Item>`: compare as `A::Item`.
        let (a, b) = (self.normalize_proj(a), self.normalize_proj(b));
        match (&a, &b) {
            (Ty::Never, _) | (_, Ty::Never) | (Ty::Unknown, _) | (_, Ty::Unknown) => true,
            // In the trait's own default methods, its associated type is a type of its own, which
            // an inference variable takes (`let mut buf = vec![]; self.fill(&mut buf); buf` with
            // `fn fill(&self, buf: &mut Vec<Self::Leaf>)`).
            (Ty::Var(v), Ty::Param(p)) | (Ty::Param(p), Ty::Var(v))
                if self.is_template(p)
                    && matches!(self.var_kind(&Ty::Var(*v)), Some(VarKind::General))
                    && p.split_once("::").is_some_and(|(t, _)| {
                        self.param_bounds.get("Self").is_some_and(|b| b.iter().any(|x| x == t))
                    }) =>
            {
                self.vars[*v as usize] = VarState::Bound(Ty::Param(p.clone()));
                true
            }
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
            // (Never a cyclic type, `?0 = Regex<?0>`: a struct used without its type arguments
            // then overflowed the stack.)
            (Ty::Var(v), t) | (t, Ty::Var(v)) if self.occurs(*v, t) => true,
            // An unsuffixed literal into a `dyn Trait` (`Box::new(10.0)`, `&123` as `&dyn Any`):
            // rustc's fallback type, `f64` or `i32`.
            (Ty::Var(v), d @ Ty::Dyn(..)) | (d @ Ty::Dyn(..), Ty::Var(v)) => {
                let lit = match self.var_kind(&Ty::Var(*v)) {
                    Some(VarKind::Int) => Ty::Int(IntKind::I32),
                    Some(VarKind::Float) => Ty::Float { f32: false },
                    _ => {
                        self.vars[*v as usize] = VarState::Bound(d.clone());
                        return true;
                    }
                };
                let d = d.clone();
                self.vars[*v as usize] = VarState::Bound(lit.clone());
                self.unify(&lit, &d)
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
            // An `I: Iterator<Item = T>` / `impl Iterator<Item = T>` parameter takes your own
            // iterator of `T` (`total(Count { n: 0 })`), as it takes std's.
            (Ty::Iter { elem, .. }, other @ Ty::Adt(..)) | (other @ Ty::Adt(..), Ty::Iter { elem, .. }) =>
            {
                let (e, other) = (elem.clone(), other.clone());
                match self.iter_elem(&other) {
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
            // Anything is a `dyn Any`.
            (Ty::Dyn(tr, _), t) | (t, Ty::Dyn(tr, _)) if tr == "Any" && !matches!(t, Ty::Dyn(_, _)) => true,
            // Any type with std's `Display` (..) is a `dyn Display` / `impl Debug`.
            (Ty::Dyn(tr, _), t) | (t, Ty::Dyn(tr, _))
                if generic::is_std_dyn(tr) && !matches!(t, Ty::Dyn(_, _)) =>
            {
                let (tr, t) = (tr.clone(), t.clone());
                tr.split('+').all(|b| self.implements(&t, b))
            }
            // A struct or enum coerces to a `dyn Trait` it implements.
            (Ty::Dyn(tr, _), Ty::Adt(..)) | (Ty::Adt(..), Ty::Dyn(tr, _)) => {
                let adt = if matches!(a, Ty::Adt(..)) {
                    a.clone()
                } else {
                    b.clone()
                };
                let tr = tr.clone();
                self.implements_user_trait(&adt, &tr)
            }
            // A type parameter bounded by the trait (`Box::new(val)` as a `Box<dyn Tr>` with `T:
            // Tr`): its values carry their impls as a struct's do.
            (Ty::Dyn(tr, _), Ty::Param(p)) | (Ty::Param(p), Ty::Dyn(tr, _))
                if tr.split('+').all(|b| {
                    matches!(b.trim(), "Send" | "Sync" | "'static") || self.param_has(p, b.trim())
                }) =>
            {
                true
            }
            // A built-in type with your impl of the trait (`Box<dyn io::Write>` holding a
            // `Vec<u8>`, `Box<dyn Money>` an `f64`).
            (Ty::Dyn(tr, _), t) | (t, Ty::Dyn(tr, _))
                if crate::adt::ext_key_of(t).is_some_and(|k| self.adts.ext_types.contains(k)) =>
            {
                let (tr, t) = (tr.clone(), t.clone());
                if tr == "IoWrite"
                    && let Ty::Vec(e) = &t
                    && matches!(self.shallow(e), Ty::Var(_))
                {
                    self.unify(e, &Ty::Int(IntKind::U8));
                }
                self.implements_user_trait(&t, &tr)
            }
            // A hand-written future (`impl Future for X`) is a future of its `Output`.
            (Ty::Future(x), t @ Ty::Adt(..)) | (t @ Ty::Adt(..), Ty::Future(x))
                if self.adt_future_output(t).is_some() =>
            {
                let (x, out) = (x.clone(), self.adt_future_output(t).unwrap());
                self.unify(&x, &out)
            }
            // A std error (`ParseIntError`) is a `dyn Error`.
            (Ty::Dyn(tr, _), Ty::Extern(n)) | (Ty::Extern(n), Ty::Dyn(tr, _))
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

    /// Your own iterator (`found`) as an `I: Iterator` / `impl Iterator` (an argument, a return
    /// value): `n` is the runtime's iterator over it (`iter_of_args`).
    fn note_iter_of(&mut self, n: &SyntaxNode, found: &Ty, expected: &Ty) {
        if matches!(self.shallow(expected), Ty::Iter { .. })
            && matches!(self.shallow(found), Ty::Adt(a, _) if self.adts.iter_items.contains_key(&a))
        {
            let r = self.range(n);
            self.iter_of_args.insert(r);
        }
    }

    /// Unify `found` with `expected`, reporting rustc's "mismatched types" on failure.
    fn expect(&mut self, n: &SyntaxNode, found: &Ty, expected: &Ty) {
        if let Ty::Dyn(tr, _) = self.shallow(expected)
            && matches!(
                self.shallow(found),
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Var(_)
            )
        {
            let r = self.range(n);
            self.dyn_num_coercions.push((r, found.clone(), tr));
        }
        if let Ty::Dyn(tr, _) = self.shallow(expected) {
            self.note_dyn_twin(n, found, &tr);
        }
        // A `String` or a `Formatter` into a `&mut dyn fmt::Write` (a struct field, a closure's
        // parameter, a `Box`): a writer object over the place (`κσ.$fmtWriter`).
        if matches!(self.shallow(expected), Ty::Dyn(tr, _) if tr == "FmtWrite")
            && matches!(self.shallow(found), Ty::Str | Ty::Formatter)
        {
            let r = self.range(n);
            self.str_writers.insert(r);
            return;
        }
        if let (Ty::Dyn(tr, _), Ty::Adt(adt, _)) = (self.shallow(expected), self.shallow(found))
            && tr != "Any"
            && !self.unify(found, expected)
        {
            let msg = format!(
                "the trait bound `{adt}: {tr}` is not satisfied (add `impl {tr} for {adt}`)"
            );
            self.error(n, msg);
            return;
        }
        self.note_iter_of(n, found, expected);
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
            let help = match (self.shallow(expected), self.shallow(found)) {
                (e, f) if self.numeric(&e) && self.numeric(&f) => Some(diag_help::NUMBERS),
                (e, Ty::Option(_) | Ty::Result(..))
                    if !matches!(e, Ty::Option(_) | Ty::Result(..)) =>
                {
                    Some(diag_help::OPTION_RESULT)
                }
                (_, Ty::Future(_)) => Some(diag_help::FUTURES),
                _ => None,
            };
            let msg = format!(
                "mismatched types: expected `{}`, found `{}`{hint}",
                self.describe(expected),
                self.describe(found)
            );
            match help {
                Some(h) => self.error_help(n, msg, h),
                None => self.error(n, msg),
            }
        }
    }

    /// A number type, or a literal still deciding which (`{integer}` / `{float}`).
    fn numeric(&self, t: &Ty) -> bool {
        matches!(self.shallow(t), Ty::Int(_) | Ty::IntLit | Ty::Float { .. })
            || matches!(self.var_kind(t), Some(VarKind::Int | VarKind::Float))
    }

    // ------------------------------------------------------------ scopes

    fn declare(&mut self, name: &ast::Name, mut local: Local) {
        local.decl = Some(self.range(name.syntax()));
        self.slot_items.remove(name.text().to_string().as_str());
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
                let item = iterator_item(t).unwrap();
                // `impl Iterator<Item = &mut T>`: what `v.iter_mut()` returns (`Item = (usize, &mut
                // T)`: `iter_mut().enumerate()`).
                let mutable = item.as_ref().is_some_and(holds_mut_ref);
                let elem = match item {
                    Some(item) => self.lower_type(&item),
                    None => self.fresh(VarKind::General),
                };
                Ty::Iter {
                    elem: Box::new(elem),
                    mutable,
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
                let args = self.user_trait_args(t);
                Ty::Dyn(self.user_trait_type(t).unwrap(), args)
            }
            // `x: impl Display` (an anonymous type parameter: see `generic::impl_arg_params`).
            ast::Type::ImplTraitType(_)
                if self.generics.contains_key(&generic::impl_arg_key(t)) =>
            {
                self.generics[&generic::impl_arg_key(t)].clone()
            }
            // `&dyn Display`, `Box<dyn Debug>`, `-> impl Ord + Debug`: any value with those std
            // traits (`Dyn("Ord+Debug")`).
            ast::Type::ImplTraitType(_) | ast::Type::DynTraitType(_)
                if generic::std_impl_bounds(t).is_some() =>
            {
                Ty::Dyn(generic::std_impl_bounds(t).unwrap().join("+"), Vec::new())
            }
            // `dyn Any` (`&dyn Any`, `Box<dyn Any + Send>`): any value, tested by `is::<T>()`.
            ast::Type::DynTraitType(d)
                if d.type_bound_list().is_some_and(|l| {
                    l.bounds().any(|b| {
                        matches!(b.ty(), Some(ast::Type::PathType(p))
                            if p.path().and_then(|p| p.segment()).and_then(|s| s.name_ref())
                                .is_some_and(|n| n.text() == "Any"))
                    })
                }) =>
            {
                Ty::Dyn("Any".into(), Vec::new())
            }
            // `Box<dyn Error>`: any error value (they all display themselves).
            ast::Type::ImplTraitType(_) | ast::Type::DynTraitType(_) if names_std_error(t) => {
                Ty::Dyn("Error".into(), Vec::new())
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
                if p.path().is_some_and(|q| q.qualifier().is_none()) {
                    self.require_std_import(p.syntax(), &name, true);
                }
                if let Some(kind) = crate::ty::Coll::from_name(&name) {
                    // (A trailing hasher or allocator argument, `IndexSet<K, S>`, is ignored.)
                    let keep = match kind {
                        crate::ty::Coll::BTreeMap | crate::ty::Coll::IndexMap => 2,
                        _ => 1,
                    };
                    let args: Vec<Ty> = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .map(|l| l.generic_args().collect::<Vec<_>>())
                        .unwrap_or_default()
                        .into_iter()
                        .take(keep)
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
                // `Range<usize>`, `RangeInclusive<i32>`, `RangeFrom<u8>`, `RangeFull` (`std::ops`).
                if let Some(kind) = RangeKind::from_name(&name)
                    && !self.adts.defs.contains_key(&name)
                {
                    if kind == RangeKind::Full {
                        return Ty::Range(Box::new(Ty::Unit), kind);
                    }
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
                    return Ty::Range(Box::new(elem), kind);
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
                // `fmt::Error` (`-> Result<(), fmt::Error>`), what `fmt::Result` holds.
                if name == "Error"
                    && p.path().and_then(|p| p.qualifier()).is_some_and(|q| {
                        matches!(
                            q.syntax().text().to_string().as_str(),
                            "fmt" | "std::fmt" | "core::fmt" | "::std::fmt" | "::core::fmt"
                        )
                    })
                {
                    return Ty::Extern("fmt::Error".into());
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
                // (Unless the program defines its own: `pub struct ParseIntError { .. }`, which a
                // plain `ParseIntError` names; `num::ParseIntError` is still std's.)
                if is_std_error(&name)
                    && (!self.adts.defs.contains_key(&name)
                        || p.path().is_some_and(|q| q.qualifier().is_some()))
                {
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
                // `std::vec::IntoIter<T>` is std's even when the file has an `IntoIter` of its own
                // (the io prelude's `lines()` next to a collection's iterator struct), and so is
                // `btree_map::Keys<'a, K, V>` in a wrapper struct named `Keys`.
                if self.adts.defs.contains_key(&name)
                    && p.path().is_some_and(|q| self.std_qualified(&q))
                    && let Some(t) = self.std_iter_type(&name, seg.as_ref())
                {
                    return t;
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
                    // (`Map<K>` with `struct Map<K, S = RandomState>`: `S` is `RandomState`, not
                    // inferred. A default may name the parameters before it.)
                    if let Some(tds) = self.adts.type_defaults.get(&name).cloned() {
                        for (i, p) in params.iter().enumerate().skip(n_given) {
                            if let Some((_, d)) = tds.iter().find(|(n, _)| n == p) {
                                let map: HashMap<String, Ty> =
                                    params.iter().cloned().zip(args.iter().cloned()).take(i).collect();
                                args[i] = generic::subst(d, &map);
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
                // `Self::Target` in a `DerefMut` impl: its `Deref` impl's.
                if name == "Target"
                    && p.path()
                        .and_then(|p| p.qualifier())
                        .is_some_and(|q| q.syntax().text() == "Self")
                    && let Some(Ty::Adt(n, _)) = &self.self_ty
                    && let Some(t) = self.adts.deref_targets.get(n)
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
                    && let Some(qn) = p.path().and_then(|p| proj_qual(&p))
                    && self.param_has(qn.as_str(), "FromStr")
                {
                    return Ty::Unknown;
                }
                // `I::Item` of an `I: IntoIterator` (its own type parameter).
                if let Some(qn) = p.path().and_then(|p| proj_qual(&p))
                    && let Some(t) = self.generics.get(&format!("{qn}::{name}"))
                {
                    return t.clone();
                }
                // `S::Unit` of a type parameter bounded by a trait with `type Unit;`.
                if let Some(qn) = p.path().and_then(|p| proj_qual(&p))
                    && self
                        .param_bounds
                        .contains_key(qn.as_str())
                    && let Some(t) = self.projection(qn.as_str(), &name)
                {
                    return t;
                }
                // `A::Item` in a field of `struct S<A: Array>`: fields are lowered before the
                // file's traits are, so `A`'s projection, as the impls' `A::Item` will be.
                if let Some(qn) = p.path().and_then(|p| proj_qual(&p))
                    && self.generics.get(&qn) == Some(&Ty::Param(qn.clone()))
                    && self.param_bounds.get(&qn).is_some_and(|bs| {
                        bs.iter().any(|b| {
                            self.file_trait_names.contains(b) && !self.adts.traits.contains_key(b)
                        })
                    })
                {
                    return Ty::Param(format!("{qn}::{name}"));
                }
                // `T::Error` of a `T: TryFrom<X>` (unchecked).
                if let Some(qn) = p.path().and_then(|p| proj_qual(&p))
                    && self
                        .lenient_projs
                        .contains(&format!("{qn}::{name}"))
                {
                    return Ty::Param(format!("{qn}::{name}"));
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
                if self.bindings.is_interface(&name) || self.bindings.is_enum(&name) {
                    return Ty::Extern(name);
                }
                // `js::JsValue`: any JS value, unchecked (what `js::global(..)` returns).
                if name == "JsValue" {
                    return Ty::Unknown;
                }
                if name == "DefaultHasher" {
                    return Ty::Hasher;
                }
                // `std::any::TypeId`: the string naming the type.
                if name == "TypeId" && !self.adts.defs.contains_key("TypeId") {
                    return Ty::Str;
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
                    self.unknown_type_error(t, &name);
                }
                ty
            }
            // `impl Display` / `dyn Tr` of a trait the checker has no model for, `for<'a> ..`.
            _ => {
                self.error(
                    t.syntax(),
                    format!("`{}` types are not supported yet", t.syntax().text()),
                );
                Ty::Unknown
            }
        }
    }

    /// A std type used by its bare name without the `use` rustc needs (`HashMap::new()` with no
    /// `use std::collections::HashMap;`): E0433 (E0412 in a type position), naming the import.
    /// Only when the file brings nothing in by glob and declares or imports no such name.
    fn require_std_import(&mut self, node: &SyntaxNode, name: &str, in_type: bool) {
        let module = match name {
            "HashMap" | "HashSet" | "BTreeMap" | "BTreeSet" | "VecDeque" | "BinaryHeap"
            | "LinkedList" => "collections",
            "Rc" => "rc",
            "Arc" | "Mutex" | "RwLock" => "sync",
            "RefCell" | "Cell" => "cell",
            "Ordering" | "Reverse" => "cmp",
            "Duration" | "Instant" => "time",
            "PhantomData" => "marker",
            "Wrapping" => "num",
            "Cow" => "borrow",
            "Path" | "PathBuf" => "path",
            "ControlFlow" | "Bound" | "RangeBounds" => "ops",
            _ => return,
        };
        if self.glob_imports
            || self.imported_names.contains(name)
            // (`use rustc_hash::FxHashMap;`, rewritten to `HashMap` before the check.)
            || self
                .imported_names
                .iter()
                .any(|a| crate::bundled::renamed_target(a) == Some(name))
            || self.adts.defs.contains_key(name)
            || self.adts.traits.contains_key(name)
            || self.adts.modules.contains(name)
            || self.type_aliases.contains_key(name)
            || self.generics.contains_key(name)
        {
            return;
        }
        let kind = if matches!(name, "Ordering") { "enum" } else { "struct" };
        let msg = if in_type {
            format!("cannot find type `{name}` in this scope (consider importing this {kind}: `use std::{module}::{name};`)")
        } else {
            format!(
                "failed to resolve: use of undeclared type `{name}` (consider importing this {kind}: `use std::{module}::{name};`)"
            )
        };
        self.error(node, msg);
    }

    /// `let x: Foo = ..` with no `Foo` in scope: rustc's E0412, with a close name suggested
    /// (`Foo2`, or a std type's). A standard library type the checker has no model for
    /// (`std::cell::UnsafeCell<i32>`, or `UnsafeCell` imported by name) is unsupported, named.
    fn unknown_type_error(&mut self, t: &ast::Type, name: &str) {
        let text = t.syntax().text().to_string();
        let first = text
            .split(|c: char| c == ':' || c == '<' || c == '(')
            .next()
            .unwrap_or("");
        if matches!(first, "std" | "core" | "alloc")
            || self.std_type_imports.contains(name)
            || is_unmodeled_std_type(name)
        {
            let msg = format!("the standard library type `{text}` is not supported yet");
            self.error(t.syntax(), msg);
            return;
        }
        let hint = self
            .similar_type_name(name, true)
            .map(|(kind, s)| format!(" (a {kind} with a similar name exists: `{s}`)"))
            .unwrap_or_default();
        self.error(t.syntax(), format!("cannot find type `{name}` in this scope{hint}"));
    }

    /// The closest name to `name` among the file's structs, enums, type aliases and type
    /// parameters (and, with `std`, the std types the checker knows), with what it is.
    fn similar_type_name(&self, name: &str, std: bool) -> Option<(&'static str, String)> {
        // (`T`: anything is two edits away.)
        if name.len() < 3 {
            return None;
        }
        let mut candidates: Vec<String> = self
            .adts
            .defs
            .keys()
            .chain(self.type_aliases.keys())
            .chain(self.generics.keys())
            .cloned()
            .collect();
        if std {
            candidates.extend(KNOWN_STD_TYPES.iter().map(|s| s.to_string()));
        }
        let s = bindings::suggest(name, candidates.iter())?;
        let kind = match self.adts.defs.get(&s) {
            Some(d) if d.is_enum => "enum",
            Some(_) => "struct",
            None if self.type_aliases.contains_key(&s) => "type alias",
            None if self.generics.contains_key(&s) => "type parameter",
            None => "type",
        };
        Some((kind, s))
    }

    /// A value or function path the checker can't resolve (`what` is "value" or "function"): a
    /// std path is an item it has no model for; `Stak::new()` under a type that isn't declared
    /// but resembles one that is, rustc's E0433 with the name; `Stack::make()` under a declared
    /// one names the missing item; anything else is E0425.
    fn unresolved_path(&mut self, node: &SyntaxNode, what: &str, path: &str) {
        let segs: Vec<&str> = path.split("::").collect();
        let first = segs[0].split('<').next().unwrap_or("");
        let last = segs[segs.len() - 1].split('<').next().unwrap_or("");
        let under_type = segs.len() > 1 && first.starts_with(|c: char| c.is_ascii_uppercase());
        let msg = if under_type
            && !self.adts.defs.contains_key(first)
            && (KNOWN_STD_TYPES.contains(&first) || Ty::from_name(first) != Ty::Unknown)
        {
            // `HashMap::neww()` / `Ordering::Lesss`: a std type the checker models, without that item.
            let kind = match first {
                "Option" | "Result" | "Ordering" => "enum",
                _ => "struct",
            };
            let item = if what == "value" { "variant" } else { "function" };
            format!(
                "no {item} or associated item named `{last}` found for {kind} `{first}` in the current scope"
            )
        } else if matches!(first, "std" | "core" | "alloc")
            || under_type
                && (self.std_type_imports.contains(first) || is_unmodeled_std_type(first))
        {
            format!("the standard library {what} `{path}` is not supported yet")
        } else if under_type && let Some(d) = self.adts.defs.get(first) {
            let (kind, item) = match (d.is_enum, what) {
                (true, "value") => ("enum", "variant"),
                (true, _) => ("enum", "function"),
                (false, _) => ("struct", "function"),
            };
            format!(
                "no {item} or associated item named `{last}` found for {kind} `{first}` in the current scope"
            )
        } else if under_type
            && !self.type_aliases.contains_key(first)
            && !self.generics.contains_key(first)
            && let Some((kind, s)) = self.similar_type_name(first, false)
        {
            format!(
                "failed to resolve: use of undeclared type `{first}` (a {kind} with a similar name exists: `{s}`)"
            )
        } else {
            format!("cannot find {what} `{path}` in this scope")
        };
        self.error(node, msg);
    }

    /// Whether `e` certainly owns its value, as rustc sees it: a local declared so, a value
    /// made here (`x.clone()`, `format!(..)`, `vec![..]`, a struct literal, a literal), an NS
    /// result (JS values hold no references), or the result of a function declared to return
    /// an owned type. False when it may be a reference (`v[i]` of some `Vec<&str>`, a field, a
    /// pattern binding).
    pub(super) fn certainly_owned(&self, e: &ast::Expr) -> bool {
        if self.certainly_js(e) {
            return true;
        }
        let e = strip_parens(e);
        if conv::expr_ref_hint(&e) == Some(crate::adt::ArgRef::Owned) {
            return true;
        }
        match &e {
            ast::Expr::PathExpr(p) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(n.text().to_string().as_str()))
                .is_some_and(|l| l.owned),
            // `opt.unwrap()` of an owned option (an NS result) is its value.
            ast::Expr::MethodCallExpr(m) => {
                let Some(recv) = m.receiver() else {
                    return false;
                };
                let name = m.name_ref().map(|n| n.text().to_string()).unwrap_or_default();
                match name.as_str() {
                    "unwrap" | "expect" | "unwrap_or" | "unwrap_or_default" | "unwrap_or_else"
                    | "unwrap_unchecked" => {
                        matches!(
                            self.raw.get(&self.range(recv.syntax())).map(|t| self.shallow(t)),
                            Some(Ty::Option(_) | Ty::Result(..))
                        ) && self.certainly_owned(&recv)
                    }
                    _ => match self.raw.get(&self.range(recv.syntax())).map(|t| self.shallow(t)) {
                        // A method of your type declared to return an owned value.
                        Some(Ty::Adt(n, _)) => self
                            .adts
                            .method(&n, &name)
                            .is_some_and(|s| s.receiver.is_some() && s.ret_owned),
                        _ => false,
                    },
                }
            }
            ast::Expr::TryExpr(t) => t.expr().is_some_and(|i| self.certainly_owned(&i)),
            ast::Expr::AwaitExpr(a) => a.expr().is_some_and(|i| self.certainly_owned(&i)),
            // A function declared to return an owned value.
            ast::Expr::CallExpr(c) => {
                let Some(ast::Expr::PathExpr(p)) = c.expr() else {
                    return false;
                };
                let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
                    return false;
                };
                self.fns
                    .get(name.text().to_string().as_str())
                    .is_some_and(|s| s.ret_owned)
            }
            ast::Expr::RecordExpr(_) | ast::Expr::ArrayExpr(_) | ast::Expr::TupleExpr(_) => true,
            ast::Expr::MacroExpr(m) => m
                .macro_call()
                .and_then(|c| c.path()?.segment()?.name_ref())
                .is_some_and(|n| matches!(n.text().to_string().as_str(), "format" | "vec")),
            _ => false,
        }
    }

    /// Whether `e` is a value from the JS bindings (an NS method's result, `ns.args`, an
    /// element or field of one, a local holding one): owned, and holding no references.
    fn certainly_js(&self, e: &ast::Expr) -> bool {
        let e = strip_parens(e);
        let extern_recv = |me: &Self, r: &ast::Expr| {
            matches!(
                me.raw.get(&me.range(r.syntax())).map(|t| me.shallow(t)),
                Some(Ty::Extern(_))
            )
        };
        match &e {
            ast::Expr::PathExpr(p) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(n.text().to_string().as_str()))
                .is_some_and(|l| l.js),
            ast::Expr::MethodCallExpr(m) => {
                let Some(recv) = m.receiver() else {
                    return false;
                };
                let name = m.name_ref().map(|n| n.text().to_string()).unwrap_or_default();
                if extern_recv(self, &recv) {
                    // (Not `clone`, which is a std method of the handle, nor the React
                    // helpers, nor a bound `Fn` field being called.)
                    return !matches!(name.as_str(), "clone" | "to_owned" | "into" | "to_string");
                }
                matches!(
                    name.as_str(),
                    "unwrap" | "expect" | "unwrap_or" | "unwrap_or_default" | "unwrap_or_else"
                ) && self.certainly_js(&recv)
            }
            ast::Expr::FieldExpr(f) => f.expr().is_some_and(|r| extern_recv(self, &r) || self.certainly_js(&r)),
            ast::Expr::IndexExpr(ix) => ix.base().is_some_and(|b| self.certainly_js(&b)),
            ast::Expr::AwaitExpr(a) => a.expr().is_some_and(|i| self.certainly_js(&i)),
            ast::Expr::TryExpr(t) => t.expr().is_some_and(|i| self.certainly_js(&i)),
            _ => false,
        }
    }

    /// An owned argument where the parameter is declared `&T` (`shown`: `&str`, `&[i32]`):
    /// rustc's E0308, at the argument, with its borrow suggestion.
    pub(super) fn check_ref_arg(&mut self, a: &ast::Expr, shown: &str) {
        if !self.certainly_owned(a) {
            return;
        }
        let Some(found) = self.raw.get(&self.range(a.syntax())).cloned() else {
            return;
        };
        let found = self.zonk(&found);
        if matches!(
            found,
            Ty::Unknown | Ty::Var(_) | Ty::Param(_) | Ty::Dyn(_, _) | Ty::Closure { .. } | Ty::Fn | Ty::Never
        ) {
            return;
        }
        // (A `&dyn Trait` / `&impl Trait` parameter: the argument may be anything implementing it.)
        if shown.contains("dyn ") || shown.contains("impl ") {
            return;
        }
        let text = a.syntax().text().to_string();
        let hint = if text.len() <= 40 && !text.contains('\n') {
            format!(" (consider borrowing here: `&{text}`)")
        } else {
            " (consider borrowing here)".to_string()
        };
        let msg = format!(
            "mismatched types: expected `{shown}`, found `{}`{hint}",
            self.describe(&found)
        );
        self.error_help(a.syntax(), msg, diag_help::REFS);
    }

    /// `v.push(x)` into a local declared `Vec<&T>` (`ref_vecs`): an owned `x` of the element's
    /// type is rustc's E0308, as for a `&T` parameter.
    pub(super) fn check_ref_elem(&mut self, recv: &ast::Expr, a: &ast::Expr, elem: &Ty) {
        let ast::Expr::PathExpr(p) = strip_parens(recv) else { return };
        let Some(shown) = p
            .path()
            .and_then(|p| p.as_single_name_ref())
            .and_then(|n| self.lookup(n.text().to_string().as_str()))
            .and_then(|l| l.decl)
            .and_then(|d| self.ref_vecs.get(&d).cloned())
        else {
            return;
        };
        let found = self.raw.get(&self.range(a.syntax())).cloned().map(|t| self.zonk(&t));
        if found.is_some_and(|f| f == self.zonk(elem)) {
            self.check_ref_arg(a, &shown);
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
        self.slot_locals.clear();
        self.slot_options.clear();
        self.slot_results.clear();
        self.slot_items.clear();
        // `-> &mut i32` / `-> Option<&mut i32>`: what's returned is kept as a slot (`keep_slot`).
        self.ret_slot = self.ret_slot_kind(f, &ret);
        self.ret_slot_param = self.ret_slot.is_some_and(|opt| {
            let r = match self.zonk(&ret) {
                Ty::Option(i) if opt => *i,
                r => r,
            };
            matches!(r, Ty::Param(_))
        });
        let opt_slot = self.opt_slot_params(f, &params);
        // Const generic parameters: values in the body.
        for (n, ty) in self.const_tys.clone() {
            let local = Local {
                ty,
                mutable: false,
                deferred: false,
                reference: None,
                owned: false,
                js: false,
                decl: None,
            };
            self.scopes[0].insert(n, local);
        }
        if let Some((n, local)) = self_local {
            self.declare(&n, local);
        }
        if let Some(pl) = f.param_list() {
            for (i, (p, ty)) in pl.params().zip(params).enumerate() {
                if let Some(ast::Pat::IdentPat(ip)) = p.pat()
                    && let Some(n) = ip.name()
                {
                    let reference = p.ty().as_ref().and_then(Self::ref_kind);
                    // `o: Option<&mut i32>`: `null` or a slot; `if let Some(n) = o` binds the slot.
                    if opt_slot.get(i) == Some(&true) {
                        self.slot_options
                            .insert(n.text().to_string(), slots::SlotShape::Slot);
                    }
                    if reference == Some(true) && self.cell_param(&ty) {
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
                    // A by-value parameter owns its argument (unless its type may be a
                    // reference: a type parameter, `impl Trait`).
                    let owned = reference.is_none()
                        && p.ty().is_some_and(|t| written_owned(&t))
                        && !matches!(ty, Ty::Param(_) | Ty::Dyn(_, _) | Ty::Unknown | Ty::Closure { .. });
                    let local = Local {
                        ty,
                        mutable: ip.mut_token().is_some(),
                        deferred: false,
                        reference,
                        owned,
                        js: false,
                        decl: None,
                    };
                    self.declare(&n, local);
                    if let Some(shown) = p.ty().and_then(|t| ref_elem_written(&t)) {
                        let r = self.range(n.syntax());
                        self.ref_vecs.insert(r, shown);
                    }
                    // `t: &mut (usize, &mut i32)`: `t.1` is a slot.
                    if let Some(s) = p.ty().and_then(|t| self.param_tuple_slot_shape(&t)) {
                        self.slot_items.insert(n.text().to_string(), s);
                    }
                } else if let Some(pat) = p.pat() {
                    // `(r, c): (usize, usize)`, `Point { x, y }: Point`, `&(a, b): &(i32, i32)`.
                    self.pat(&pat, &ty);
                }
            }
        }
        let Some(body) = f.body() else { return };
        self.ret_declared = f.ret_type().is_some();
        self.fn_name = f.name().map(|n| n.text().to_string()).unwrap_or_default();
        let mut found = self.block(&body, Some(&ret));
        // `fn id(&self) -> &str { &self.key }` with a `Cow<str>` field: deref coercion of the
        // tail, as `check` does at an argument.
        if matches!(f.ret_type().and_then(|r| r.ty()), Some(ast::Type::RefType(_)))
            && let Some(tail) = body.stmt_list().and_then(|l| l.tail_expr())
            && let Some((n, target)) = self.deref_target(&found)
            && !matches!(self.shallow(&ret), Ty::Adt(ref m, _) if *m == n)
            && self.could_be(&target, &ret)
        {
            self.auto_deref(&tail, &n, &target);
            found = target;
        }
        // `-> impl Iterator<Item = u32> { Count { n } }`: the runtime's iterator over it.
        if let Some(tail) = body.stmt_list().and_then(|l| l.tail_expr()) {
            self.note_iter_of(tail.syntax(), &found, &ret);
        }
        if !self.unify(&found, &ret) {
            let tail = body.stmt_list().and_then(|l| l.tail_expr());
            let has_tail = tail.is_some();
            let node = tail
                .map(|t| t.syntax().clone())
                .unwrap_or_else(|| body.syntax().clone());
            let (expected_s, found_s) = (self.describe(&ret), self.describe(&found));
            // (rustc's secondary spans: the return type, the function's name.)
            let ret_ty = f.ret_type().and_then(|r| r.ty()).map(|t| self.range(t.syntax()));
            let fn_name = f.name().map(|n| self.range(n.syntax()));
            if !self.ret_declared && !matches!(self.shallow(&found), Ty::Unit) {
                // `fn name() { String::from("x") }`: the `()` comes from the missing `-> T`,
                // the mistake a JavaScript author makes most.
                let msg = self.missing_ret_msg(&found_s);
                self.error_help(&node, msg, diag_help::MISSING_RET);
            } else if self.ret_declared && !has_tail && matches!(self.shallow(&found), Ty::Unit) {
                // `fn name() -> String { String::from("x"); }`: the stray `;`. rustc points at
                // the return type, the name, and the `;`.
                let last = body
                    .stmt_list()
                    .and_then(|l| l.statements().last())
                    .map(|s| self.range(s.syntax()));
                let msg = format!("mismatched types: expected `{expected_s}`, found `()`");
                let mut spans = Vec::new();
                if let Some((s, e)) = fn_name {
                    let why = "implicitly returns `()` as its body has no tail or `return` expression";
                    spans.push((s, e, why.to_string()));
                }
                if let Some((_, e)) = last
                    && self.src.as_bytes().get(e.wrapping_sub(1)) == Some(&b';')
                {
                    spans.push((e - 1, e, "help: remove this semicolon to return this value".into()));
                }
                let at = ret_ty.or(last).unwrap_or_else(|| self.range(&node));
                self.error_spans(at, msg, Some(diag_help::STRAY_SEMI), spans);
            } else {
                let msg = format!("mismatched types: expected `{expected_s}`, found `{found_s}`");
                let spans = ret_ty
                    .map(|(s, e)| vec![(s, e, format!("expected `{expected_s}` because of return type"))])
                    .unwrap_or_default();
                self.error_spans(self.range(&node), msg, None, spans);
            }
        }
        self.resolve_intos();
        self.resolve_try_intos();
        self.resolve_collects();
        self.resolve_parses();
        self.resolve_bound_picks();
        self.resolve_defaults();
        self.resolve_tries();
        self.resolve_string_froms();
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
                help: None,
                spans: Vec::new(),
            });
        }
        for (r, adt, targs) in std::mem::take(&mut self.const_lits) {
            if errors {
                break;
            }
            let params = self.adts.defs[&adt].type_params.clone();
            let kind = if self.adts.defs[&adt].is_enum {
                "enum"
            } else {
                "struct"
            };
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
                        message: format!("type annotations needed: cannot infer the value of the const parameter `{p}` declared on the {kind} `{adt}`"),
                        severity: "error",
                        help: None,
                        spans: Vec::new(),
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
            // (`<&A as Tr>::f()` of a `&T` impl of its own: decided where it was checked.)
            if matches!(self.conversions.get(&r), Some(crate::adt::Conv::Ext { key }) if crate::adt::is_wrapper_key(key)) {
                continue;
            }
            let adt = match &t {
                Ty::Adt(n, _) if self.adts.defs.get(n).is_some_and(|d| d.module.is_none()) => {
                    Some(n.clone())
                }
                _ => None,
            };
            // (Not for impls the trait arguments pick: those share the method's dispatcher.)
            let alias = (adt.is_some() || (local_trait && matches!(t, Ty::Param(_) | Ty::Dyn(_, _))))
                && !self.adts.picked_by_trait_args(&q.trait_name, &q.method);
            let on_self = !q.receiver && t == Ty::Param("Self".into());
            // `Shout::shout(&5)` / `<f64 as Money>::money(&x)`: your impl's method on a
            // built-in type, or a blanket impl's (on any type without its own).
            if q.receiver && !matches!(t, Ty::Param(_) | Ty::Dyn(_, _) | Ty::Var(_) | Ty::Unknown) {
                let own = adt
                    .as_ref()
                    .is_some_and(|n| self.adts.method(n, &q.method).is_some());
                let ext = crate::adt::ext_key_of(&t)
                    .filter(|k| {
                        adt.is_none()
                            && self.adts.ext_types.contains(*k)
                            && self.adts.method(k, &q.method).is_some()
                    })
                    .map(str::to_string);
                let key = match ext {
                    Some(k) => Some(k),
                    None if !own => self.blanket_method(&t, &q.method).map(|(k, _)| k),
                    None => None,
                };
                if let Some(key) = key {
                    self.conversions.insert(r, crate::adt::Conv::Ext { key });
                    continue;
                }
            }
            // `<W as Make>::make()` for a struct only a blanket impl gives `make`.
            if !q.receiver
                && let Some(n) = &adt
                && self.adts.method(n, &q.method).is_none()
                && let Some((key, _)) = self.blanket_static(&t, &q.method)
            {
                self.conversions.insert(
                    r,
                    crate::adt::Conv::ExtDict {
                        key,
                        dict: t.clone(),
                    },
                );
                continue;
            }
            // `<i32 as Make>::make()` / `let x: String = Make::make();`: your impl for the
            // built-in type, or a blanket impl's.
            if adt.is_none() && !q.receiver && !matches!(t, Ty::Param(_) | Ty::Dyn(_, _) | Ty::Var(_))
            {
                let ext = crate::adt::ext_key_of(&t).filter(|k| {
                    self.adts.ext_types.contains(*k)
                        && self
                            .adts
                            .method(k, &q.method)
                            .is_some_and(|m| m.receiver.is_none())
                });
                let conv = match ext {
                    Some(k) => Some(crate::adt::Conv::Ext { key: k.to_string() }),
                    None => self.blanket_static(&t, &q.method).map(|(key, _)| {
                        crate::adt::Conv::ExtDict {
                            key,
                            dict: t.clone(),
                        }
                    }),
                };
                if let Some(conv) = conv {
                    self.conversions.insert(r, conv);
                    continue;
                }
            }
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
                        format!("`<{p} as {}>::{}(..)` on a type parameter isn't supported yet", q.trait_name, q.method),
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
        if b.modifier().is_some()
            && !matches!(
                b.modifier(),
                Some(ast::BlockModifier::Unsafe(_) | ast::BlockModifier::Const(_))
            )
        {
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
        // The tail of a function declared `-> &mut i32` / `-> Option<&mut i32>`: a slot (while
        // the body's locals are still in scope).
        if let Some(opt) = self.ret_slot
            && b.syntax().parent().is_some_and(|p| ast::Fn::can_cast(p.kind()))
            && let Some(tail) = list.tail_expr()
        {
            self.keep_ret_slot(&tail, opt);
        }
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
                    self.warn_dropped_future(&e, &t);
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
        // `let t = &mut tagged[1];` / `let (i, r) = &mut tagged[1];` of a `Vec<(usize, &mut
        // i32)>` of slots: `t.1` / `r` are the item's slots.
        let slot_item = l
            .initializer()
            .and_then(|i| self.slot_tuple_item(&i))
            .map(|(ix, s)| {
                let r = self.range(ix.syntax());
                self.whole_slot_items.insert(r);
                s
            });
        let init_ty = l.initializer().map(|init| {
            let t = self.infer(&init, declared.as_ref());
            if let Some(d) = &declared {
                self.expect(init.syntax(), &t, d);
            }
            t
        });
        // `let mut t = (1, &mut x);`: `t.1` is a slot over `x`; `let u = &t;` / `let (a, b) = t;`
        // take its shape on.
        let slot_item = slot_item
            .or_else(|| self.tuple_let_slot_shape(l.ty().as_ref(), l.initializer().as_ref()))
            .or_else(|| {
                l.initializer()
                    .filter(|i| matches!(strip_parens(i), ast::Expr::PathExpr(_) | ast::Expr::RefExpr(_)))
                    .and_then(|i| self.tuple_slot_shape(&i))
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
                            | "get_or_insert"
                            | "get_or_insert_with"
                            | "get_or_insert_default"
                    )
                }) =>
            {
                Some(true)
            }
            // `let r = opt.insert(v);`: a `&mut T` into the option.
            Some(ast::Expr::MethodCallExpr(m))
                if m.name_ref().is_some_and(|n| n.text() == "insert")
                    && m.receiver()
                        .and_then(|r| match strip_parens(&r) {
                            ast::Expr::PathExpr(p) => p.path()?.as_single_name_ref(),
                            _ => None,
                        })
                        .and_then(|n| self.lookup(n.text().to_string().as_str()))
                        .is_some_and(|l| matches!(self.zonk(&l.ty), Ty::Option(_))) =>
            {
                Some(true)
            }
            // `let s = v.iter_mut().min_by(..)?;`: a `&mut` element.
            Some(e) if mut_element(&e, &|e| self.typed_mut_iter(e)) => Some(true),
            // `let this = self.get_mut();` in a future's `poll` (`self: Pin<&mut Self>`).
            Some(ast::Expr::MethodCallExpr(m))
                if m.name_ref().is_some_and(|n| {
                    matches!(
                        n.text().to_string().as_str(),
                        "get_mut" | "get_unchecked_mut"
                    )
                }) && m.arg_list().is_some_and(|a| a.args().next().is_none())
                    && m.receiver().is_some_and(|r| r.syntax().text() == "self") =>
            {
                Some(true)
            }
            // `let links = v[i].as_mut().unwrap();`: the `&mut` it holds.
            Some(e) if self.unwrapped_mut_ref_call(&e) => Some(true),
            // `let g = net.get_mut(0);` / `let s = find(&mut v, "a").unwrap();` with your own
            // `fn get_mut(&mut self, ..) -> &mut Srv` / `-> Option<&mut Srv>`.
            Some(e) if self.user_mut_ref_call(&e) => Some(true),
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
            self.note_slot_let(&name, l.initializer().as_ref());
            if l.initializer().is_some_and(|i| self.lock_call(&i)) {
                self.lock_results.insert(name.text().to_string());
            } else {
                self.lock_results.remove(&*name.text());
            }
            // `let mut refs: Vec<&mut i32> = Vec::new();` / `let tagged: Vec<(usize, &mut i32)> =
            // vec![(0, &mut c)];`: a Vec of slots, filled by `push` or the literal.
            if let Some(t) = l.ty()
                && let Some(shape) = self.written_slot_shape(&t)
            {
                self.slot_locals
                    .insert(name.text().to_string(), (true, shape.clone()));
                if let Some(init) = l.initializer() {
                    self.note_slot_literal(&init, &shape);
                }
            }
            let ty = match (declared, init_ty) {
                (Some(d), _) => d,
                (None, Some(t)) => t,
                (None, None) => self.fresh(VarKind::General),
            };
            let reference = declared_ref.or(init_ref).or_else(|| {
                // `let r = map.get_mut(k).unwrap();`: a `&mut` to the slot's value.
                let r = self.range(name.syntax());
                self.slot_bindings.contains(&r).then_some(true)
            });
            // `let s: String = ..` / `let s = String::from(..)`: the binding owns its value.
            let js = l.initializer().is_some_and(|i| self.certainly_js(&i));
            let owned = reference.is_none()
                && !matches!(ty, Ty::Param(_) | Ty::Dyn(_, _) | Ty::Unknown | Ty::Closure { .. })
                && (l.ty().is_some_and(|t| written_owned(&t))
                    || l.initializer().is_some_and(|i| self.certainly_owned(&i)));
            let local = Local {
                ty,
                mutable: ip.mut_token().is_some(),
                deferred,
                reference,
                owned,
                js,
                decl: None,
            };
            let ty = local.ty.clone();
            if l.ty().is_none() {
                let r = self.range(name.syntax());
                self.let_types.push((r, ty.clone()));
            }
            self.declare(&name, local);
            if let Some(shown) = l.ty().and_then(|t| ref_elem_written(&t)) {
                let r = self.range(name.syntax());
                self.ref_vecs.insert(r, shown);
            }
            if let Some(s) = slot_item {
                self.slot_items.insert(name.text().to_string(), s);
            }
            if l.ty().is_none()
                && l.initializer().is_some_and(|i| self.str_init(&i))
                && self.zonk(&ty) == Ty::Str
            {
                let r = self.range(name.syntax());
                self.str_locals.insert(r);
            }
            if self.copy_array(l.ty().as_ref(), l.initializer().as_ref(), &ty) {
                let r = self.range(name.syntax());
                self.copy_arrays.insert(r);
            }
            // A raw pointer doesn't borrow.
            if self.ptr_let(l, &name, &ty) {
                return;
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
        } else if let Some(msg) = self.array_pat_len_error(l, &pat) {
            self.error(pat.syntax(), msg);
        } else if !self.irrefutable_pat(&pat)
            && !self.pat_covers(&pat, &ty)
            && !self.array_pat_fits(l, &pat)
            && !self.array_pat_covers(&pat, &ty, &self.let_dims(l))
            && !self.pat_fits(&pat, &self.let_shape(l))
        {
            self.error(
                pat.syntax(),
                "refutable pattern in local binding (use `let ... else { ... }` or `if let`)",
            );
        }
        self.pat(&pat, &ty);
        self.note_slot_tuple_let(&pat, l.initializer().as_ref());
        if let Some(s) = &slot_item {
            self.bind_slot_shape(&pat, s);
        }
        // `let Some(c) = s.maybe(true) else { return };` of your `-> Option<&mut i32>` (or an
        // `Option<&mut i32>` local or parameter): `c` binds the slot.
        // `let Some(ref mut n) = self.timer else { .. };` / `let (ref mut a, ref mut b) = pair;`
        // / `let Some(n) = self.timer.as_mut() else { .. };`: as in `if let`, the bindings alias
        // the place.
        let through_mut = l
            .initializer()
            .is_some_and(|i| self.alias_place_bindings(Some(&i), &pat));
        // (Else `let Some(c) = s.maybe(true) else { return };` of your `-> Option<&mut i32>`, or
        // an `Option<&mut i32>` local or parameter: `c` binds the slot.)
        if !through_mut && let Some(init) = l.initializer() {
            self.bind_slot_option(Some(&init), &pat);
        }
        // `let Some(s) = net.find_mut("a") else { return };` (or `v.get_mut(0)`, `&mut pair`,
        // `self.timer.as_mut()`): `s` is a `&mut` into the place, written through without `mut`.
        if (through_mut
            || l.let_else().is_some()
                && l.initializer().as_ref().is_some_and(|i| self.mut_ref_call(i)))
            && let Some(scope) = self.scopes.last_mut()
        {
            for ip in pat.syntax().descendants().filter_map(ast::IdentPat::cast) {
                if let Some(local) = ip.name().and_then(|n| scope.get_mut(n.text())) {
                    local.reference = Some(true);
                }
            }
        }
        self.mark_tuple_mut_refs(l.initializer().as_ref(), &pat);
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
        // `let (src, dst) = (a, &mut *b);`: a name bound to a `&` / `&mut` field is a reference.
        if let (ast::Pat::TuplePat(tp), Some(ast::Expr::TupleExpr(te))) =
            (&pat, l.initializer().as_ref().map(strip_parens))
        {
            let pairs: Vec<(ast::IdentPat, ast::RefExpr)> = tp
                .fields()
                .zip(te.fields())
                .filter_map(|(p, e)| match (p, strip_parens(&e)) {
                    (ast::Pat::IdentPat(ip), ast::Expr::RefExpr(r)) if ip.pat().is_none() => {
                        Some((ip, r))
                    }
                    _ => None,
                })
                .collect();
            for (ip, r) in pairs {
                let mutable = r.mut_token().is_some();
                if let Some(scope) = self.scopes.last_mut()
                    && let Some(local) = ip.name().and_then(|n| scope.get_mut(n.text()))
                {
                    local.reference = Some(mutable);
                }
                // `let (a, b) = (&mut t.0, &mut t.1);` of numbers: slots over the places.
                if mutable
                    && let Some(n) = ip.name()
                    && self.keep_slot(&ast::Expr::RefExpr(r))
                {
                    let nr = self.range(n.syntax());
                    self.slot_bindings.insert(nr);
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

    /// Whether a value matched or bound from `e` is a `&mut` reference: `&mut x`, a std call
    /// such as `map.get_mut(..)` / `v.last_mut()` / `it.next()` of an `iter_mut()`, or one of the
    /// file's own functions returning `&mut T` / `Option<&mut T>`.
    /// `m.lock()` (or `read` / `write` / `get_mut` / `try_*`) on the lock prelude's `Mutex` /
    /// `RwLock`, not unwrapped: matched, its `Ok` payload is the lock's value in place.
    pub(super) fn lock_call(&self, e: &ast::Expr) -> bool {
        let ast::Expr::MethodCallExpr(m) = strip_parens(e) else {
            return false;
        };
        if !m.name_ref().is_some_and(|n| {
            matches!(
                n.text().as_ref(),
                "lock" | "read" | "write" | "get_mut" | "try_lock" | "try_read" | "try_write"
            )
        }) {
            return false;
        }
        let Some(recv) = m.receiver() else { return false };
        let Some(Ty::Adt(n, _)) = self.raw.get(&self.range(recv.syntax())).map(|t| self.zonk(t))
        else {
            return false;
        };
        matches!(n.as_str(), "Mutex" | "RwLock")
            && self
                .adts
                .defs
                .get(&n)
                .and_then(|d| d.variants.first())
                .is_some_and(|v| v.field("__c").is_some())
    }

    fn mut_ref_call(&self, e: &ast::Expr) -> bool {
        (match ptr::unsafe_tail(e) {
            ast::Expr::RefExpr(r) => r.mut_token().is_some(),
            ast::Expr::MethodCallExpr(m) => {
                m.name_ref().is_some_and(|n| {
                    matches!(
                        n.text(),
                        "get_mut"
                            | "get_index_mut"
                            | "node_weight_mut"
                            | "edge_weight_mut"
                            | "first_mut"
                            | "last_mut"
                            | "as_mut"
                            | "values_mut"
                            | "iter_mut"
                            | "split_at_mut"
                            | "downcast_mut"
                            | "peek_mut"
                    )
                }) || mut_element(&ast::Expr::MethodCallExpr(m.clone()), &|e| {
                    self.typed_mut_iter(e)
                })
            }
            _ => false,
        })
            // `if let Some(s) = net.find_mut("a")` with your own `-> Option<&mut Srv>`.
            || self.user_mut_ref_call(e)
    }

    /// Whether `e` is a call of one of the file's own functions or methods whose declared return
    /// type is `&mut T` / `Option<&mut T>` (also through `?`, `.unwrap()` and `.expect(..)`): a
    /// binding of it holds a `&mut`, as `iter_mut()`'s items do, and is written through without
    /// `mut`.
    fn user_mut_ref_call(&self, e: &ast::Expr) -> bool {
        let e = strip_unwraps(e).0;
        match &e {
            ast::Expr::CallExpr(c) => {
                let Some(ast::Expr::PathExpr(p)) = c.expr() else {
                    return false;
                };
                let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
                    return false;
                };
                self.fns
                    .get(name.text().to_string().as_str())
                    .is_some_and(|s| s.ret_mut_ref)
            }
            ast::Expr::MethodCallExpr(m) => {
                let (Some(r), Some(n)) = (m.receiver(), m.name_ref()) else {
                    return false;
                };
                let Some(rt) = self.raw.get(&self.range(r.syntax())) else {
                    return false;
                };
                let Ty::Adt(adt, _) = self.zonk(rt) else {
                    return false;
                };
                self.adts
                    .method(&adt, n.text().to_string().as_str())
                    .is_some_and(|m| m.ret_mut_ref)
            }
            _ => false,
        }
    }

    /// `place.as_mut()` of an `Option` or `Result` in a place (`self.timer.as_mut()`, `t.as_mut()`,
    /// `v[i].as_mut()`): codegen emits the place itself for it, so a pattern against it can
    /// alias the place.
    pub(super) fn as_mut_of_place(&self, e: &ast::Expr) -> bool {
        let ast::Expr::MethodCallExpr(m) = e else {
            return false;
        };
        let Some(r) = m.receiver() else { return false };
        m.name_ref().is_some_and(|n| n.text() == "as_mut")
            && m.arg_list().is_none_or(|l| l.args().next().is_none())
            && is_place_expr(&strip_parens(&r))
            && self
                .raw
                .get(&self.range(r.syntax()))
                .is_some_and(|t| matches!(self.zonk(t), Ty::Option(_) | Ty::Result(..)))
    }

    /// `*r.as_mut().unwrap() += 1` / `let x = r.as_mut().expect("..");` of a `Result` in a place
    /// holding a number, string or bool: the `Ok` payload is a slot over the result's own
    /// (`κσ.$resSlot`), so the write reaches it. (Matched, the bindings alias it instead.)
    pub(super) fn mark_res_as_mut_slot(&mut self, m: &ast::MethodCallExpr, ok: &Ty) {
        let Some(recv) = m.receiver() else { return };
        let unwrapped = ast::MethodCallExpr::cast(m.syntax().parent().unwrap())
            .and_then(|u| u.name_ref())
            .is_some_and(|n| matches!(n.text(), "unwrap" | "expect"));
        if !unwrapped
            || !unwrapped_position(m)
            || !is_place_expr(&strip_parens(&recv))
            || !self.prim(&self.zonk(ok))
        {
            return;
        }
        let r = self.range(m.syntax());
        self.slot_calls.insert(r);
        if let Some(n) = slot_let_binding(m) {
            let r = self.range(n.syntax());
            self.slot_bindings.insert(r);
        }
    }

    /// `*self.timer.as_mut().unwrap() += 1` / `let r = v[i].as_mut().unwrap();` of a number,
    /// string or bool in a place: the call is a slot over the place (`κσ.$optSlot`, null for
    /// `None`), as `get_mut`'s are, so the write reaches it. (Matched with `if let` / `match`,
    /// the bindings alias the place instead: `mark_mut_ref_bindings`.) A local's slot needs the
    /// local in a cell.
    fn mark_as_mut_slot(&mut self, m: &ast::MethodCallExpr, inner: &Ty) {
        let Some(recv) = m.receiver() else { return };
        let recv = strip_parens(&recv);
        if !is_place_expr(&recv)
            || !slots::opt_slot_position(m)
            || !self.prim(&self.zonk(inner))
        {
            return;
        }
        if matches!(recv, ast::Expr::PathExpr(_)) {
            self.box_if_needed(&recv);
        }
        let r = self.range(m.syntax());
        self.slot_calls.insert(r);
        if let Some(n) = slot_let_binding(m) {
            let r = self.range(n.syntax());
            self.slot_bindings.insert(r);
        }
    }

    /// Whether `e` was checked as a `&mut` iterator (`std::slice::IterMut`, `impl Iterator<Item =
    /// &mut T>`, a local holding `v.iter_mut()`).
    fn typed_mut_iter(&self, e: &ast::Expr) -> bool {
        self.raw
            .get(&self.range(e.syntax()))
            .is_some_and(|t| matches!(self.zonk(t), Ty::Iter { mutable: true, .. }))
    }

    /// Bindings that alias the matched place rather than copy its value (codegen's
    /// `emit_binds`): all of them when matching through `&mut` (`&mut x`, `self` / a loop
    /// variable that is a `&mut` reference, `place.as_mut()`), so `*r += 1` writes back; the
    /// `ref mut` ones when matching a place by value. Returns whether the match goes through
    /// `&mut`. Also for `let` patterns (`let Some(ref mut n) = self.timer else { .. };`).
    fn alias_place_bindings(&mut self, scrutinee: Option<&ast::Expr>, pat: &ast::Pat) -> bool {
        let through_place = match scrutinee.map(strip_parens) {
            Some(ast::Expr::RefExpr(r)) => r.mut_token().is_some(),
            // (A local holding `m.lock()`'s result: as the call, below.)
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .is_some_and(|n| {
                    let n = n.text().to_string();
                    self.lock_results.contains(&n)
                        || self.lookup(&n).is_some_and(|l| l.reference == Some(true))
                }),
            // `if let Ok(mut g) = m.lock()`: codegen matches on a result whose payload is the
            // lock's value in place (`κσ.$lockRes`), so `g` aliases it.
            Some(e) if self.lock_call(&e) => true,
            // `if let Some(n) = self.timer.as_mut()`: the `Option` in that place itself (codegen
            // matches on the place), so `n` aliases it as it would through `&mut self.timer`.
            Some(e) => self.as_mut_of_place(&e),
            None => false,
        };
        if through_place {
            // `if let Some(n) = &mut o` / `o.as_mut()` / `match &mut x { n => .. }` of a local
            // number, string, bool or `Option`: the local lives in a cell, so `n` aliases `o[0]`
            // and `bump(n)` hands that cell on.
            let root = match scrutinee.map(strip_parens) {
                Some(ast::Expr::RefExpr(r)) => r.expr(),
                Some(ast::Expr::MethodCallExpr(m)) if self.as_mut_of_place(&ast::Expr::MethodCallExpr(m.clone())) => m.receiver(),
                _ => None,
            };
            if let Some(ast::Expr::PathExpr(p)) = root.map(|r| strip_parens(&r))
                && let Some(name) = p.path().and_then(|p| p.as_single_name_ref())
                && let Some(local) = self.lookup(name.text().to_string().as_str()).cloned()
                && local.reference != Some(true)
                && self.cellable(&local.ty)
                && let Some(d) = local.decl
            {
                self.boxed.insert(d);
            }
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
        } else if scrutinee.map(strip_parens).is_some_and(|e| is_place_expr(&e)) {
            // `match self.timer { Some(ref mut n) => *n -= 1, .. }`: a `ref mut` binding into a
            // place (a field, an element, a local) aliases it, so the write reaches the place
            // rather than a copy of the number.
            for ip in pat
                .syntax()
                .descendants_with_tokens()
                .filter_map(|e| e.into_node())
                .filter_map(ast::IdentPat::cast)
                .filter(|ip| ip.ref_token().is_some() && ip.mut_token().is_some())
            {
                if let Some(n) = ip.name() {
                    let r = self.range(n.syntax());
                    self.mut_bindings.insert(r);
                }
            }
        }
        through_place
    }

    /// `v[i].as_mut().unwrap()` / `map.get_mut(&k).expect("..")` / `v.last_mut()?`: a std call
    /// giving a `&mut`, unwrapped.
    fn unwrapped_mut_ref_call(&self, e: &ast::Expr) -> bool {
        let (e, unwrapped) = strip_unwraps(e);
        unwrapped && matches!(e, ast::Expr::MethodCallExpr(_)) && self.mut_ref_call(&e)
    }

    /// The bindings of a tuple pattern matched against a tuple literal whose element is a `&mut`
    /// (`(Some(dst), Some(src))` against `(v.get_mut(a..b), w.get(c..d))`, `(Some(a), b)` against
    /// `(Some(&mut x), 2)`): written through without `mut`.
    fn mark_tuple_mut_refs(&mut self, e: Option<&ast::Expr>, pat: &ast::Pat) {
        let (Some(ast::Expr::TupleExpr(t)), ast::Pat::TuplePat(tp)) = (e.map(strip_parens), pat)
        else {
            return;
        };
        let (es, ps): (Vec<_>, Vec<_>) = (t.fields().collect(), tp.fields().collect());
        if es.len() != ps.len() {
            return;
        }
        for (e, p) in es.iter().zip(&ps) {
            if matches!(p, ast::Pat::TuplePat(_)) {
                self.mark_tuple_mut_refs(Some(e), p);
                continue;
            }
            // (`Some(&mut x)`: what it holds.)
            let inner = match strip_parens(e) {
                ast::Expr::CallExpr(c)
                    if matches!(c.expr(), Some(ast::Expr::PathExpr(p)) if p.syntax().text() == "Some") =>
                {
                    c.arg_list().and_then(|l| l.args().next()).unwrap_or(e.clone())
                }
                other => other,
            };
            if !(self.mut_ref_call(&inner) || self.unwrapped_mut_ref_call(&inner)) {
                continue;
            }
            // (A number, string, bool or `Option` gets a slot: `(Some(a), b)` against
            // `(Some(&mut x), 2)`.)
            self.bind_slot_option(Some(e), p);
            for ip in p.syntax().descendants().filter_map(ast::IdentPat::cast) {
                let Some(n) = ip.name() else { continue };
                let Some(ty) = self.lookup(n.text()).map(|l| l.ty.clone()) else { continue };
                if self.cellable(&ty) {
                    continue;
                }
                if let Some(local) = self.scopes.last_mut().and_then(|s| s.get_mut(n.text())) {
                    local.reference = Some(true);
                }
            }
        }
    }

    /// Bindings from a pattern matched against `map.get_mut(..)`, `v.last_mut()`, `&mut x` and
    /// the like hold `&mut` references, so they can be mutated through without `mut`.
    fn mark_mut_ref_bindings(&mut self, scrutinee: Option<&ast::Expr>, pat: &ast::Pat) {
        let through_place = self.alias_place_bindings(scrutinee, pat);
        let mut_source = through_place || scrutinee.is_some_and(|e| self.mut_ref_call(e));
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
        // `Some((k, v))` against `index_map.get_index_mut(i)` of numbers: `v` is the slot.
        if let Some(ast::Expr::MethodCallExpr(m)) = scrutinee.map(strip_parens)
            && self.slot_calls.contains(&self.range(m.syntax()))
            && m.name_ref().is_some_and(|n| n.text() == "get_index_mut")
            && let ast::Pat::TupleStructPat(tp) = pat
            && let Some(ast::Pat::TuplePat(t)) = tp.fields().next()
            && let Some(ast::Pat::IdentPat(ip)) = t.fields().nth(1)
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
        // `Some(x)` against `it.next()` of an `iter_mut()` over numbers: `x` is a slot.
        self.bind_slot_option(scrutinee, pat);
        self.mark_tuple_mut_refs(scrutinee, pat);
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
                        owned: false,
                        js: false,
                        decl: None,
                    };
                    self.declare(&name, local);
                }
            }
            ast::Pat::PathPat(pp) if path_is(pp.path(), "None") => self.expect_option(p, expected),
            ast::Pat::PathPat(pp) if self.const_path_pat(pp).is_some() => {
                // An associated (or std) constant compares against its value.
                let (t, _) = self.const_path_pat(pp).unwrap();
                if !self.unify(expected, &t) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `{}`",
                        self.describe(expected),
                        self.describe(&t)
                    );
                    self.error(p.syntax(), msg);
                }
            }
            ast::Pat::PathPat(pp) if ordering_variant(pp.path(), &self.adts.defs).is_some() => {
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
                        format!("this pattern has {} field{}, but the corresponding tuple variant has 1 field", fields.len(), if fields.len() == 1 { "" } else { "s" }),
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
                // (A bindings enum's variant, `ScriptArg::Str(s)`, before the file's ADTs.)
                if !self.extern_enum_pat(p, expected) {
                    self.adt_pat(p, expected);
                }
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
                            owned: false,
                            js: false,
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
        // Arms coerced to an expected `dyn` (`Some(e)` of two error types as an `Option<&dyn
        // Error>`): each is checked against it, rather than against the first arm's type.
        let to_dyn = expected.filter(|t| {
            let mut dyn_ = false;
            self.zonk(t).walk(&mut |t| dyn_ |= matches!(t, Ty::Dyn(_, _)));
            dyn_
        });
        for arm in &arms {
            self.scopes.push(HashMap::new());
            if let Some(p) = arm.pat() {
                self.pat(&p, &st);
                self.mark_mut_ref_bindings(m.expr().as_ref(), &p);
            }
            if let Some(g) = arm.guard().and_then(|g| g.condition()) {
                self.check(&g, &Ty::Bool);
            }
            if let Some(to) = to_dyn
                && let Some(body) = arm.expr()
            {
                self.check(&body, to);
                result = Some(to.clone());
            } else if let Some(body) = arm.expr() {
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
        // (An array of known length: `[]` or longer arrays aren't values to cover. Looked up only
        // for an array: `array_dims_in_file` searches the file.)
        let dims = match self.zonk(st) {
            Ty::Vec(_) => m
                .expr()
                .map(|e| arrays::array_dims_in_file(&e, 0))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let st = self.with_dims(st, &dims);
        self.missing_steps.set(0);
        if let Some(witness) = self.missing(&rows, std::slice::from_ref(&st)) {
            let node = m
                .expr()
                .map(|e| e.syntax().clone())
                .unwrap_or_else(|| m.syntax().clone());
            // rustc names each missing constructor (`A`, `B` and `C`; past three, `and N
            // more`), through the reference the scrutinee is.
            let mut all = self.top_witnesses(&rows, &st);
            if all.is_empty() {
                all = vec![witness[0].clone()];
            }
            let amp = self.scrutinee_ref(m);
            let all: Vec<String> = all.iter().map(|w| format!("{amp}{w}")).collect();
            let ticked: Vec<String> = all.iter().map(|w| format!("`{w}`")).collect();
            let list = match ticked.as_slice() {
                [one] => one.clone(),
                [init @ .., last] if init.len() < 3 => format!("{} and {last}", init.join(", ")),
                _ => format!("{} and {} more", ticked[..3].join(", "), ticked.len() - 3),
            };
            let (start, end) = self.range(&node);
            let spans = self.non_exhaustive_spans(m, arms, &st, amp, &all);
            self.diags.push(RawDiag {
                start,
                end,
                message: format!("non-exhaustive patterns: {list} not covered"),
                severity: "error",
                help: None,
                spans,
            });
        }
    }

    /// Each constructor of the scrutinee's type the arms leave values of, as rustc lists them
    /// (empty for a type without a fixed set of constructors).
    fn top_witnesses(&self, rows: &[Vec<Ap>], st: &Ty) -> Vec<String> {
        let ctors: Vec<Ap> = match self.zonk(st) {
            Ty::Adt(n, targs) if self.adts.defs.contains_key(&n) => (0..self.adts.defs[&n].variants.len())
                .map(|v| Ap::Ctor(v, vec![Ap::Wild; self.variant_inst(&n, v, &targs).fields.len()]))
                .collect(),
            Ty::Option(_) => vec![Ap::None, Ap::Some(Box::new(Ap::Wild))],
            Ty::Result(..) => vec![Ap::Ctor(0, vec![Ap::Wild]), Ap::Ctor(1, vec![Ap::Wild])],
            Ty::Bool => vec![Ap::Bool(false), Ap::Bool(true)],
            Ty::Ordering => vec![Ap::Ord(-1), Ap::Ord(0), Ap::Ord(1)],
            _ => return Vec::new(),
        };
        let mut out = Vec::new();
        for i in 0..ctors.len() {
            // (The other constructors covered: a witness is one of this constructor's values.)
            let mut with_others = rows.to_vec();
            with_others.extend(ctors.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, c)| vec![c.clone()]));
            self.missing_steps.set(0);
            if let Some(w) = self.missing(&with_others, std::slice::from_ref(st)) {
                out.push(w[0].clone());
            }
        }
        out
    }

    /// `"&"` / `"&mut "` when the scrutinee is a reference (`&x`, a `&T` parameter, `&self`).
    fn scrutinee_ref(&self, m: &ast::MatchExpr) -> &'static str {
        let reference = match m.expr() {
            Some(ast::Expr::RefExpr(r)) => Some(r.mut_token().is_some()),
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(&n.text().to_string()))
                .and_then(|l| l.reference),
            _ => None,
        };
        match reference {
            Some(true) => "&mut ",
            Some(false) => "&",
            None => "",
        }
    }

    /// For a match over one of the file's own enums (not generic), what rustc adds to E0004, as
    /// spans for `rustc_diag` to lay out: the enum's name (label `""`) and each missing
    /// variant (`"not covered"`) for the "defined here" note, the matched type (`"=.."`), and the
    /// new arm after the last one (`"+.."`, inserted there).
    fn non_exhaustive_spans(
        &self,
        m: &ast::MatchExpr,
        arms: &[ast::MatchArm],
        st: &Ty,
        amp: &str,
        witnesses: &[String],
    ) -> Vec<(usize, usize, String)> {
        let Ty::Adt(name, targs) = self.zonk(st) else { return Vec::new() };
        let Some(root) = m.syntax().ancestors().last() else { return Vec::new() };
        let Some(e) = root
            .descendants()
            .filter_map(ast::Enum::cast)
            .find(|e| e.name().is_some_and(|n| n.text() == name.as_str()))
        else {
            return Vec::new();
        };
        if !targs.is_empty() || e.generic_param_list().is_some() {
            return Vec::new();
        }
        let Some(en) = e.name() else { return Vec::new() };
        let (s, t) = self.range(en.syntax());
        let mut spans = vec![(s, t, String::new())];
        for w in witnesses {
            let Some(v) = w.strip_prefix(amp).and_then(|w| w.strip_prefix(&format!("{name}::"))) else {
                return Vec::new();
            };
            let vname = v.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("");
            let Some(var) = e
                .variant_list()
                .into_iter()
                .flat_map(|l| l.variants())
                .find_map(|x| x.name().filter(|n| n.text() == vname))
            else {
                return Vec::new();
            };
            let (s, t) = self.range(var.syntax());
            spans.push((s, t, "not covered".into()));
        }
        spans.push((0, 0, format!("=the matched value is of type `{amp}{name}`")));
        // The new arm: after the last one (before its comma), on a line of its own.
        if let Some(last) = arms.last()
            && let Some(text) = root.text().to_string().get(..usize::from(last.syntax().text_range().start())).map(str::to_string)
        {
            let indent: String = text
                .rsplit('\n')
                .next()
                .unwrap_or("")
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            let pat = match witnesses {
                [one] => one.clone(),
                ws if ws.len() <= 3 => ws.join(" | "),
                _ => "_".into(),
            };
            let at = match last.comma_token() {
                Some(c) => {
                    self.range(last.syntax()).0
                        + usize::from(c.text_range().start() - last.syntax().text_range().start())
                }
                None => self.range(last.syntax()).1,
            };
            spans.push((at, at, format!("+,\n{indent}{pat} => todo!()")));
        }
        spans
    }

    /// Whether a pattern with `|` alternatives or enum variants covers every value (`let (Ok(n) |
    /// Err(n)) = r;`, `let Ok(n) = r;` with `r: Result<u8, Infallible>`).
    fn pat_covers(&self, p: &ast::Pat, ty: &Ty) -> bool {
        if !p.syntax().descendants().any(|n| {
            ast::OrPat::can_cast(n.kind())
                || ast::TupleStructPat::can_cast(n.kind())
                || ast::RecordPat::can_cast(n.kind())
        })
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
            ast::Pat::IdentPat(ip) if self.const_pat(&ip).is_some() => {
                let value = || {
                    let n = ip.name()?.text().to_string();
                    self.eval_const_segs(ip.syntax(), &[n])?.int()
                };
                match (self.const_pat(&ip), value()) {
                    (Some(Ty::Int(_)), Some(v)) => vec![Ap::Int(v, v)],
                    _ => vec![Ap::Other],
                }
            }
            ast::Pat::IdentPat(ip) => ip
                .pat()
                .map(|sub| self.abstract_pat(&sub))
                .unwrap_or_else(|| vec![Ap::Wild]),
            ast::Pat::PathPat(pp) if path_is(pp.path(), "None") => vec![Ap::None],
            ast::Pat::PathPat(pp) if self.const_path_pat(&pp).is_some() => {
                match self.const_path_pat(&pp) {
                    Some((Ty::Int(_), Some(v))) => vec![Ap::Int(v, v)],
                    _ => vec![Ap::Other],
                }
            }
            ast::Pat::PathPat(pp) if ordering_variant(pp.path(), &self.adts.defs).is_some() => {
                vec![Ap::Ord(
                    ordering_variant(pp.path(), &self.adts.defs).unwrap(),
                )]
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
                    // `LIMIT`
                    Some(ast::Pat::IdentPat(ip)) if self.const_pat(&ip).is_some() => {
                        let n = ip.name()?.text().to_string();
                        self.eval_const_segs(ip.syntax(), &[n])
                            .and_then(|v| v.int())
                            .map(Some)
                    }
                    // `S::LIMIT`
                    Some(ast::Pat::PathPat(pp))
                        if matches!(self.const_path_pat(&pp), Some((Ty::Int(_), Some(_)))) =>
                    {
                        Some(self.const_path_pat(&pp).and_then(|(_, v)| v))
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
            other => self
                .extern_enum_abstract(&other)
                .or_else(|| self.adt_abstract(&other))
                .unwrap_or(vec![Ap::Opaque]),
        }
    }

    /// Whether no value has type `t`: `!`, or an enum without variants (`Infallible`).
    fn uninhabited(&self, t: &Ty) -> bool {
        match self.zonk(t) {
            Ty::Never => true,
            Ty::Adt(n, _) => self
                .adts
                .defs
                .get(&n)
                .is_some_and(|d| d.is_enum && d.variants.is_empty()),
            _ => false,
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
            // A bindings enum (a TS union): its variants, each over one payload.
            (Ty::Extern(n), _)
                if self.bindings.is_enum(&n) && !self.adts.defs.contains_key(&n) =>
            {
                Family::Extern(n)
            }
            (Ty::Result(t, e), _) => Family::Result(*t, *e),
            (Ty::Vec(e), Some(Ap::Slice(..))) => Family::Slice(*e),
            (Ty::Coll(crate::ty::Coll::FixedArray(n), v), _) => match *v {
                Ty::Vec(e) => Family::Array(*e, n),
                _ => return None,
            },
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
            (Family::Tuple(_), Ap::Tuple(_))
            | (Family::Slice(_) | Family::Array(..), Ap::Slice(..)) => true,
            (Family::Adt(..) | Family::Result(..) | Family::Extern(_), Ap::Ctor(..)) => true,
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
            Family::Extern(n) => {
                let b = self.bindings.clone();
                let variants = &b.types[&n].variants;
                if rows.iter().all(|r| matches!(r[0], Ap::Wild)) {
                    let defaults: Vec<Vec<Ap>> = rows.iter().map(|r| r[1..].to_vec()).collect();
                    let w = self.missing(&defaults, rest)?;
                    let head = match variants.first() {
                        Some(v) if rows.is_empty() => format!("{n}::{}(_)", v.name),
                        _ => "_".to_string(),
                    };
                    return Some([vec![head], w].concat());
                }
                variants.iter().enumerate().find_map(|(v, variant)| {
                    let rows_v = specialize(&Ap::Ctor(v, vec![]), 1);
                    // (No row of this variant: every value of it is missing.)
                    if rows_v.is_empty() {
                        let head = format!("{n}::{}(_)", variant.name);
                        return Some([vec![head], vec!["_".to_string(); rest.len()]].concat());
                    }
                    let w = self.missing(
                        &rows_v,
                        &[vec![variant.payload.clone()], rest.to_vec()].concat(),
                    )?;
                    let head = format!("{n}::{}({})", variant.name, w[0]);
                    Some([vec![head], w[1..].to_vec()].concat())
                })
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
                    if self.adts.cfg_variants.contains(&(n.clone(), v))
                        || variant.fields.iter().any(|f| self.uninhabited(&f.ty))
                    {
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
            // An array: only its own length.
            Family::Array(elem, n) => {
                let w = self.missing(
                    &specialize(&Ap::Slice(vec![], vec![], false), n),
                    &[vec![elem; n], rest.to_vec()].concat(),
                )?;
                let head = format!("[{}]", w[..n].join(", "));
                Some([vec![head], w[n..].to_vec()].concat())
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
                    // (`Result<T, Infallible>` is always `Ok`.)
                    .filter(|(_, ty, _)| !self.uninhabited(ty))
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
        // (`enum_payloads` reaches through `&`, parentheses, arrays and tuples only.)
        let payloads = std::mem::take(&mut self.enum_payloads);
        self.enum_payloads = payloads
            && matches!(
                e,
                ast::Expr::RefExpr(_) | ast::Expr::ParenExpr(_) | ast::Expr::ArrayExpr(_) | ast::Expr::TupleExpr(_)
            );
        let ty = self.infer_inner(e, expected);
        self.enum_payloads = payloads;
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
        // (The target through the type arguments: `&cow` as a `&str`, `&LAZY` as a `&Context`.)
        if let Some((n, target)) = self.deref_target(&t)
            && match self.shallow(expected) {
                Ty::Var(_) | Ty::Unknown => false,
                Ty::Adt(m, _) => m != n && matches!(self.shallow(&target), Ty::Adt(ref k, _) if *k == m),
                _ => true,
            }
            && self.could_be(&target, expected)
        {
            self.unify(&target, expected);
            self.auto_deref(e, &n, &target);
            return target;
        }
        self.expect(e.syntax(), &t, expected);
        t
    }

    /// `dyn_nums`: the coercions whose trait has impls on two or more number types.
    fn finish_dyn_nums(&mut self) -> HashMap<(usize, usize), String> {
        let mut out = HashMap::new();
        for (r, t, tr) in std::mem::take(&mut self.dyn_num_coercions) {
            let numeric_impls = self
                .adts
                .trait_impls
                .iter()
                .filter(|(ty, trs)| {
                    trs.contains(&tr) && matches!(Ty::from_name(ty), Ty::Int(_) | Ty::Float { .. })
                })
                .count();
            if numeric_impls >= 2
                && let Some(k) = crate::adt::ext_key_of(&self.zonk(&t).default_lit())
            {
                out.insert(r, k.to_string());
            }
        }
        // (An owned string or vector into a `dyn` of a twin trait: see `note_dyn_twin`.)
        for r in std::mem::take(&mut self.dyn_twins) {
            out.insert(r, "$own".to_string());
        }
        for r in std::mem::take(&mut self.str_writers) {
            out.insert(r, "$fmtWriter".to_string());
        }
        out
    }

    /// A tuple field: inside an NS argument, a bindings enum takes a variant's payload.
    fn infer_payload(&mut self, f: &ast::Expr, hint: Option<&Ty>) -> Ty {
        if self.enum_payloads
            && let Some(Ty::Extern(n)) = hint.map(|h| self.shallow(h))
            && self.bindings.is_enum(&n)
        {
            self.check_enum_arg(f, &n);
            return Ty::Extern(n);
        }
        self.infer(f, hint)
    }

    /// An array element: as `check`, or as `infer_payload` inside an NS argument.
    fn check_payload_or(&mut self, item: &ast::Expr, elem: &Ty) {
        if self.enum_payloads
            && let Ty::Extern(n) = self.shallow(elem)
            && self.bindings.is_enum(&n)
        {
            self.check_enum_arg(item, &n);
        } else {
            self.check(item, elem);
        }
    }

    /// Mark `e` (of type `adt`) as auto-dereferenced to `target`.
    fn auto_deref(&mut self, e: &ast::Expr, adt: &str, target: &Ty) {
        let r = self.range(e.syntax());
        let mutable = self.adts.deref_mut.contains(adt);
        // (A `deref_mut` declared `-> &mut String` returns a slot over the field.)
        if mutable
            && self
                .adts
                .method(adt, "deref_mut")
                .is_some_and(|m| m.ret_slot == Some(false))
        {
            self.deref_slots.insert(r);
        }
        self.derefs.insert(r, (mutable, target.clone()));
    }

    /// The key a map lookup checks its argument against: `Q` for a `&Q` binding when the map's
    /// keys are a `K: Borrow<Q>` (`self.map.get(k)`), else `k`.
    fn lookup_key(&self, k: &Ty, arg: Option<&ast::Expr>) -> Ty {
        let inner = arg.map(strip_parens).map(|a| match &a {
            ast::Expr::RefExpr(r) => r.expr().map(|e| strip_parens(&e)).unwrap_or(a),
            _ => a,
        });
        if let Ty::Param(kn) = self.shallow(k)
            && let Some(q) = self.borrow_args.get(&kn)
            && let Some(ast::Expr::PathExpr(p)) = inner
            && let Some(n) = p.path().and_then(|p| p.as_single_name_ref())
            && let Some(local) = self.lookup(n.text())
            && self.shallow(&local.ty) == self.shallow(q)
        {
            return q.clone();
        }
        k.clone()
    }

    /// `x`'s type through `impl Deref`, when `x` is such a struct or enum.
    fn deref_target(&self, t: &Ty) -> Option<(String, Ty)> {
        match self.shallow(t) {
            // (A generic struct's target in its type arguments: `LazyLock<Vec<u32>>` derefs to
            // `Vec<u32>`.)
            Ty::Adt(n, targs) => self.adts.deref_targets.get(&n).map(|t| {
                let map = self.adt_subst(&n, &targs);
                // (Resolved: `OrderedFloat(x).sqrt()` derefs to the `f64` its variable holds.)
                (n.clone(), self.shallow(&generic::subst(t, &map)))
            }),
            _ => None,
        }
    }

    fn infer_inner(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> Ty {
        match e {
            ast::Expr::Literal(l) => self.literal(l),
            ast::Expr::PathExpr(p) => {
                self.check_slot_result_use(p);
                self.note_slot_tuple_ref(p);
                self.path(p)
            }
            ast::Expr::ParenExpr(p) => p
                .expr()
                .map(|i| self.infer(&i, expected))
                .unwrap_or(Ty::Unit),
            ast::Expr::RefExpr(r) => {
                let Some(inner) = r.expr() else {
                    return Ty::Unknown;
                };
                let t = self.infer(&inner, expected);
                // (After the place is typed: `through_mut_field` reads its fields' owners.)
                if r.mut_token().is_some() {
                    self.check_mutable(&inner, Mutation::Borrow);
                    if let ast::Expr::PathExpr(p) = strip_parens(&inner)
                        && let Some(n) = p.path().and_then(|p| p.as_single_name_ref())
                        && let Some(local) = self.lookup(n.text())
                        && local.reference == Some(false)
                        && matches!(self.shallow(&local.ty), Ty::Vec(_))
                    {
                        let rr = self.range(r.syntax());
                        self.slice_ref_muts.insert(rr);
                    }
                }
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
                                // (`r = &mut other;` with `r` a `&mut i32` local rebinds its
                                // slot: `slot_assigns`.)
                                && !b.lhs().is_some_and(|l| self.is_ref_local(&l))
                        }))
                {
                    let z = self.zonk(&t);
                    // `let r = &mut v[i];`: `r` is a slot over the element (or field), made
                    // once, so `*r = ..` writes it.
                    let let_name = ast::LetStmt::cast(p.clone()).and_then(|l| match l.pat() {
                        Some(ast::Pat::IdentPat(ip)) if ip.pat().is_none() => ip.name(),
                        _ => None,
                    });
                    let range_index = matches!(
                        strip_parens(&inner),
                        ast::Expr::IndexExpr(ix) if matches!(ix.index(), Some(ast::Expr::RangeExpr(_)))
                    );
                    // `let Some(file) = &mut self.file else { .. };`: the pattern's bindings
                    // alias the place (`alias_place_bindings`), as in `if let`.
                    let destructures = ast::LetStmt::cast(p.clone())
                        .and_then(|l| l.pat())
                        .is_some_and(|pat| !matches!(pat, ast::Pat::IdentPat(_)));
                    if destructures {
                    } else if (self.prim(&z) || matches!(z, Ty::Option(_)))
                        && !range_index
                        && let Some(n) = let_name
                    {
                        let rr = self.range(r.syntax());
                        self.place_slots.insert(rr);
                        let nr = self.range(n.syntax());
                        self.slot_bindings.insert(nr);
                    } else if self.prim(&z) || matches!(z, Ty::Option(_)) {
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
                    // (`n as *mut T` with `n` an integer; `&x as *const i32` is a reference.)
                    // (A local's own integer, or an integer going to a pointer to something else;
                    // a reference to an integer, `&x` or an `r: &i32`, is a pointer to it.)
                    let local_ref = match strip_parens(&inner) {
                        ast::Expr::PathExpr(p) => p
                            .path()
                            .and_then(|p| p.as_single_name_ref())
                            .and_then(|n| self.lookup(n.text().as_ref()))
                            .map(|l| l.reference.is_some()),
                        _ => None,
                    };
                    let int = matches!(self.shallow(&t), Ty::Int(_) | Ty::IntLit)
                        && !matches!(strip_parens(&inner), ast::Expr::RefExpr(_))
                        && !self.is_ptr(&inner)
                        && local_ref != Some(true)
                        && (local_ref == Some(false)
                            || !matches!(self.shallow(&to), Ty::Int(_) | Ty::IntLit | Ty::Var(_)));
                    if matches!(strip_parens(&inner), ast::Expr::Literal(_)) || int {
                        self.error(
                            c.syntax(),
                            "casting an integer to a raw pointer isn't supported: values have no memory addresses",
                        );
                    } else if (!self.could_be(&t, &to)
                        || matches!((self.shallow(&t), self.shallow(&to)), (Ty::Param(a), Ty::Param(b)) if a != b))
                        && !matches!(self.shallow(&to), Ty::Dyn(_, _))
                    {
                        // `s as *const str as *const Name`, `self as *const Self as *const u8`.
                        let msg = format!(
                            "casting a pointer to `{}` to a pointer to `{}` isn't supported: values have no memory layout to reinterpret",
                            self.describe(&t),
                            self.describe(&to)
                        );
                        self.error(c.syntax(), msg);
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
                let coerce = matches!(self.shallow(&to), Ty::Dyn(_, _) | Ty::Closure { .. });
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
            // `Box::from(sq)` as a `Box<dyn Shape>` through your `impl From<Sq> for Box<dyn
            // Shape>`; otherwise `Box::from(x)` is `x`, as `Box::new(x)`.
            ast::Expr::CallExpr(c)
                if let Some(named) = box_from_call(c)
                    && let Some(tr) =
                        named.or_else(|| match expected.map(|e| self.shallow(e)) {
                            Some(Ty::Dyn(tr, _)) => Some(tr),
                            _ => None,
                        })
                    && self.adts.from_impls.contains_key(&format!("dyn${tr}")) =>
            {
                let args: Vec<ast::Expr> =
                    c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
                self.conv_from_call(c, &format!("dyn${tr}"), &args);
                Ty::Dyn(tr, Vec::new())
            }
            ast::Expr::CallExpr(c)
                if is_box_new(c)
                    || box_from_call(c).is_some()
                    || is_rc_call(c)
                    || is_pin_new(c, self.adts.defs.contains_key("Pin")) =>
            {
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
                            // (`T: Default` on the function or the `impl`: as `T::default()`.)
                            Ty::Param(p) if self.param_has(&p, "Default") => {
                                self.note_dict(&p, c.syntax());
                                true
                            }
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
                        // `w.n` with `n: &mut i32`: the field holds a slot.
                        if let Some(field) = f.name_ref().map(|n| n.text().to_string())
                            && self.mut_ref_fields.contains(&(name.clone(), field))
                        {
                            let r = self.range(f.syntax());
                            self.slot_values.insert(r);
                        }
                        self.adt_field(f, &name, &targs)
                    }
                    Ty::Unknown | Ty::Var(_) => Ty::Unknown,
                    // `r.start` / `r.end` (an inclusive range's are methods).
                    Ty::Range(e, k)
                        if f.name_ref().is_some_and(|n| match &*n.text() {
                            "start" => k.has_start() && k != RangeKind::Inclusive,
                            "end" => k.has_end() && k != RangeKind::Inclusive,
                            _ => false,
                        }) =>
                    {
                        *e
                    }
                    Ty::Tuple(es) if f.name_ref().is_some_and(|n| n.as_tuple_field().is_some()) => {
                        let i = f.name_ref().and_then(|n| n.as_tuple_field()).unwrap_or(0);
                        // `tagged[1].1` of a `Vec<(usize, &mut i32)>`, or `t.1` of an item of
                        // one in a local: the slot in the item.
                        let item = self.tuple_slot_shape(&recv);
                        if let Some(slots::SlotShape::Tuple(fields)) = item
                            && fields.get(i) == Some(&Some(slots::SlotShape::Slot))
                        {
                            let r = self.range(f.syntax());
                            self.slot_values.insert(r);
                        }
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
                        // `s.length`: the JavaScript name.
                        match diag_help::js_alias(&name, true) {
                            Some(alias) => self.error_help(
                                f.syntax(),
                                format!("{msg} (you might have meant to use {alias})"),
                                diag_help::JS_NAMES,
                            ),
                            None => self.error(f.syntax(), msg),
                        }
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
                if let Some(out) = self.adt_future_output(&t) {
                    return out;
                }
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
                let kind = RangeKind::of(r.start().is_some(), r.end().is_some(), inclusive);
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
                if wanted
                    || bound
                    || self.range_ext_value(r, kind)
                    || range_conv_value(r)
                    || expected.is_some_and(|t| self.range_bound_arg(t, kind))
                {
                    if kind == RangeKind::Full {
                        return Ty::Range(Box::new(Ty::Unit), kind);
                    }
                    // (Any ordered bounds: floats too, for `contains`.)
                    let elem = self.fresh(VarKind::General);
                    for b in [r.start(), r.end()].into_iter().flatten() {
                        self.check(&b, &elem);
                    }
                    return Ty::Range(Box::new(elem), kind);
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
                let saved_ret_slot = self.ret_slot.take();
                let saved_declared = std::mem::replace(&mut self.ret_declared, true);
                let saved_async = std::mem::replace(&mut self.in_async, true);
                let t = self.block_body(b, inner.as_ref());
                if t != Ty::Never {
                    self.unify(&t, &ret);
                }
                self.ret_ty = saved_ret;
                self.ret_slot = saved_ret_slot;
                self.ret_declared = saved_declared;
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
                    // `return String::from("x");` in a function with no `-> T`.
                    Some(v) if !self.ret_declared => {
                        let t = self.infer(&v, None);
                        if !self.unify(&t, &ret)
                            && !matches!(self.shallow(&t), Ty::Unit | Ty::Never | Ty::Unknown)
                        {
                            let msg = self.missing_ret_msg(&self.describe(&t));
                            self.error_help(v.syntax(), msg, diag_help::MISSING_RET);
                        }
                    }
                    Some(v) => {
                        self.check(&v, &ret);
                        if let Some(opt) = self.ret_slot {
                            self.keep_ret_slot(&v, opt);
                        }
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
                        .map(|(i, f)| self.infer_payload(f, hints.get(i)))
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
                            self.check_payload_or(&item, &elem);
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
                    if self.copy_arrays.contains(&d) {
                        let r = self.range(p.syntax());
                        self.array_reads.insert(r);
                    }
                }
                if local.reference == Some(true) && matches!(self.shallow(&local.ty), Ty::Vec(_)) {
                    let r = self.range(p.syntax());
                    self.slice_ref_muts.insert(r);
                }
                return local.ty;
            }
            if let Some(t) = self.consts.get(name.text().to_string().as_str()).cloned() {
                // (`let mut w = ORDER;`: JS would share the item's array.)
                if self.copy_array_consts.contains(name.text()) {
                    let r = self.range(p.syntax());
                    self.array_reads.insert(r);
                }
                if self.static_muts.contains(name.text())
                    && !self.macro_in_unsafe
                    && !in_unsafe(p.syntax()) {
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
                // `None::<js::Element>`
                let arg = path
                    .segment()
                    .and_then(|s| s.generic_arg_list())
                    .and_then(|l| l.generic_args().next())
                    .and_then(|a| match a {
                        ast::GenericArg::TypeArg(t) => t.ty(),
                        _ => None,
                    });
                let t = match arg {
                    Some(t) => self.lower_type(&t),
                    None => self.fresh(VarKind::General),
                };
                return Ty::Option(Box::new(t));
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
                let (params, ret) = (fn_value_params(sig), sig.ret.clone());
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
            if let Some((_, ty)) = crate::ty::std_alias_const(&self.std_fns, &[name.text()]) {
                return ty;
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
                if self.static_muts.contains(single)
                    && !self.macro_in_unsafe
                    && !in_unsafe(p.syntax()) {
                    self.error(
                        p.syntax(),
                        "use of mutable static is unsafe and requires unsafe block",
                    );
                }
                return t;
            }
            if let Some(sig) = self.fns.get(single) {
                let (is_async, tps) = (sig.is_async, sig.type_params.clone());
                let (params, ret) = (fn_value_params(sig), sig.ret.clone());
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
        // `.map_err(String::from)` with your `impl From<A> for String`: which impl is known once
        // the argument's type is.
        if let [.., "String", "from"] = segs.iter().map(String::as_str).collect::<Vec<_>>().as_slice()
            && self.adts.from_impls.contains_key("String")
        {
            let a = self.fresh(VarKind::General);
            self.pending_string_froms.push((p.syntax().clone(), a.clone()));
            return Ty::Closure {
                params: vec![a],
                ret: Box::new(Ty::Str),
            };
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
        if segs.len() >= 2 {
            self.require_std_import(p.syntax(), &segs[0].clone(), false);
        }
        let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
        // `T::SIDES`: an associated const of a bound of `T`.
        if let [param, konst] = segs.as_slice()
            && let Some(t) = self.param_const(param, konst, p.syntax())
        {
            return t;
        }
        // `i32::ZERO`, `Tr::ZERO`: a trait impl's associated const.
        if let [owner, konst] = segs.as_slice()
            && let Some(t) = self.trait_const_path(p.syntax(), owner, konst)
        {
            return t;
        }
        if let [.., "Ordering", "Less" | "Equal" | "Greater"] = segs.as_slice() {
            return Ty::Ordering;
        }
        if let Some((_, ty)) = std_const(std_tail(&segs))
            .or_else(|| crate::ty::std_alias_const(&self.std_fns, &segs))
        {
            return ty;
        }
        if let Some(t) = self.jsvalue_path(&segs) {
            return t;
        }
        let text = path.syntax().text().to_string();
        self.unresolved_path(p.syntax(), "value", &text);
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

    /// Whether `e` is fields of a local (`p.x`, `s.a.b`), with no index or `*` on the way.
    fn field_place(e: &ast::Expr) -> bool {
        match strip_parens(e) {
            ast::Expr::FieldExpr(f) => f.expr().is_some_and(|b| match strip_parens(&b) {
                ast::Expr::PathExpr(_) => true,
                b => Self::field_place(&b),
            }),
            _ => false,
        }
    }

    /// Whether `e` is the left side of an assignment (`e = v`, `e += v`, `(e, f) = t`).
    fn assigned_place(e: &ast::Expr) -> bool {
        let mut node = e.syntax().clone();
        loop {
            let Some(parent) = node.parent() else { return false };
            if let Some(b) = ast::BinExpr::cast(parent.clone()) {
                return matches!(b.op_kind(), Some(BinaryOp::Assignment { .. }))
                    && b.lhs().is_some_and(|l| l.syntax() == &node);
            }
            if ast::TupleExpr::can_cast(parent.kind()) || ast::ParenExpr::can_cast(parent.kind()) {
                node = parent;
            } else {
                return false;
            }
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

    /// Whether the place `e` is reached through a field written `&mut T` (`self.table.items`
    /// with `table: &'a mut Table`): a write there goes through the reference, so the binding
    /// needn't be `mut`. (Assigning the field itself, `self.table = t`, still needs it.)
    fn through_mut_field(&self, e: &ast::Expr, how: Mutation) -> bool {
        // (`&mut s.table` borrows the field itself, which does need it; so does `s.table = t`,
        // which the caller passes as a `Borrow` since it is reached through a field.)
        let explicit = e
            .syntax()
            .parent()
            .and_then(ast::RefExpr::cast)
            .is_some_and(|r| r.mut_token().is_some())
            || Self::assigned_place(e);
        let mut cur = strip_parens(e);
        let mut outermost = true;
        loop {
            cur = match &cur {
                ast::Expr::FieldExpr(f) => {
                    let Some(base) = f.expr() else { return false };
                    let base = strip_parens(&base);
                    let field = f
                        .name_ref()
                        .map(|n| n.text().to_string())
                        .or_else(|| f.index_token().map(|t| t.text().to_string()));
                    if let (Some(field), Some(Ty::Adt(adt, _))) = (
                        field,
                        self.raw
                            .get(&self.range(base.syntax()))
                            .map(|t| self.zonk(t)),
                    ) && self.mut_borrow_fields.contains(&(adt, field))
                        && !(outermost && (how == Mutation::Assign || explicit))
                    {
                        return true;
                    }
                    base
                }
                ast::Expr::IndexExpr(ix) => match ix.base() {
                    Some(b) => strip_parens(&b),
                    None => return false,
                },
                ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Deref) => match p.expr() {
                    Some(b) => strip_parens(&b),
                    None => return false,
                },
                _ => return false,
            };
            outermost = false;
        }
    }

    /// `s.read(&mut buf)` with `s: &[u8]`: the io prelude's `impl IoRead for Vec<u8>` (std's
    /// `impl Read for &[u8]`), which advances the slice binding.
    pub(super) fn slice_read(&self, key: &str, method: &str, rt: &Ty) -> bool {
        key == "Vec"
            && self.is_bytes(rt)
            && self
                .adts
                .traits
                .get("IoRead")
                .is_some_and(|t| t.methods.iter().any(|m| m.name == method))
    }

    /// The receiver of a `slice_read`: a `&[u8]` binding is itself borrowed mutably (`let mut s:
    /// &[u8]`), not the bytes behind it.
    fn check_slice_reader(&mut self, recv: &ast::Expr) {
        if let ast::Expr::PathExpr(p) = strip_parens(recv)
            && let Some(n) = p.path().and_then(|p| p.as_single_name_ref())
            && let Some(local) = self.lookup(n.text()).cloned()
            && local.reference == Some(false)
        {
            if let Some(d) = local.decl {
                self.mut_roots.insert(d);
            }
            if !local.mutable {
                let n = n.text();
                self.error(
                    recv.syntax(),
                    format!("cannot borrow `{n}` as mutable, as it is not declared as mutable (declare it with `let mut {n}`)"),
                );
            }
            return;
        }
        self.check_mutable(recv, Mutation::Borrow);
    }

    /// rustc's mutability rules for assigning to or mutably borrowing a place.
    fn check_mutable(&mut self, e: &ast::Expr, how: Mutation) {
        // (Whatever the outcome, the root binding is one a mutation goes through.)
        if let Some((name, _)) = Self::place_root(e)
            && let Some(d) = self.lookup(name.text()).and_then(|l| l.decl)
        {
            self.mut_roots.insert(d);
        }
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
        // `*w.n += 1` with `n: &mut i32`, `*s.at(i) = 0`: a write through the reference.
        if self.through_slot(e) {
            return;
        }
        // `t.1.push('b')` with `t.1: &mut String`: borrowed through the reference.
        if how != Mutation::Assign && self.slot_values.contains(&self.range(strip_parens(e).syntax()))
        {
            return;
        }
        if self.through_mut_field(e, how) {
            return;
        }
        let Some((name, through)) = Self::place_root(e) else {
            return;
        };
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
        // `&mut s` with `s: &[u8]`: a `&mut &[u8]`, borrowing the binding, not what it points to.
        let reference = match local.reference {
            Some(false)
                if !through
                    && e.syntax()
                        .ancestors()
                        .skip(1)
                        .find(|a| !ast::ParenExpr::can_cast(a.kind()))
                        .and_then(ast::RefExpr::cast)
                        .is_some_and(|r| r.mut_token().is_some()) =>
            {
                None
            }
            r => r,
        };
        match reference {
            Some(true) => {}
            Some(false) => self.error(
                e.syntax(),
                format!("cannot borrow `*{n}` as mutable, as it is behind a `&` reference"),
            ),
            // `p.x = 5`, `s.table = t`: rustc's E0594 names the field.
            None if !local.mutable && Self::assigned_place(e) && Self::field_place(e) => {
                let place = strip_parens(e).syntax().text().to_string();
                self.error(
                    e.syntax(),
                    format!("cannot assign to `{place}`, as `{n}` is not declared as mutable (declare it with `let mut {n}`)"),
                )
            }
            None if !local.mutable => self.error(
                e.syntax(),
                format!("cannot borrow `{n}` as mutable, as it is not declared as mutable (declare it with `let mut {n}`)"),
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
                // (The place first: `*w.n += 1` is checked by what `w.n` is.)
                let lt = self.infer(&lhs, None);
                self.check_mutable(
                    &lhs,
                    if through {
                        Mutation::Borrow
                    } else {
                        Mutation::Assign
                    },
                );
                match op {
                    Some(op) if matches!(self.shallow(&lt), Ty::Param(_)) => {
                        let Ty::Param(n) = self.shallow(&lt) else {
                            unreachable!()
                        };
                        self.param_arith(b, op, &n, &lt, &rhs, true);
                    }
                    Some(op @ (ArithOp::Shl | ArithOp::Shr)) => {
                        // `b <<= 2` with your `impl ShlAssign<u32> for Bits`.
                        if let Ty::Adt(n, _) = self.shallow(&lt) {
                            self.op_overload(b.syntax(), op, true, &n, &lt, &rhs, None);
                        } else {
                            self.infer(&rhs, None);
                        }
                    }
                    Some(op) => {
                        self.arith_operands(b, op, &lt, &rhs);
                    }
                    None => {
                        self.check(&rhs, &lt);
                    }
                }
                // `r = &mut other;` / `r = s.at(1);` with `r` a `&mut i32` local: rebind its
                // slot rather than write through it.
                if op.is_none() && self.is_ref_local(&lhs) && self.keep_slot(&rhs) {
                    let r = self.range(b.syntax());
                    self.slot_assigns.insert(r);
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
                // `*"a" < b` / `b > *"a"` with `impl PartialOrd<Bytes> for str` / `impl PartialOrd<str>
                // for Bytes`: that impl's `partial_cmp` (codegen reads the operator off its result).
                if matches!(cmp, CmpOp::Ord { .. })
                    && !self.could_be(&lt, &rt)
                    && let Some(k) = match self.shallow(&lt) {
                        Ty::Adt(n, _) => Some(n),
                        t => crate::adt::ext_key_of(&self.zonk(&t))
                            .filter(|k| self.adts.ext_types.contains(*k))
                            .map(str::to_string),
                    }
                    && let Some((key, p)) = self.pick_cmp_impl(&k, "partial_cmp", &rt)
                {
                    self.unify(&p, &rt);
                    let r = self.range(b.syntax());
                    self.conversions.insert(r, crate::adt::Conv::Method { key });
                    return Ty::Bool;
                }
                // `"home" == host` / `id == 3` with `impl PartialEq<Host> for &str` /
                // `impl PartialEq<u32> for Id`: that impl's `eq`.
                if matches!(cmp, CmpOp::Eq { .. })
                    && !self.could_be(&lt, &rt)
                    && let Some(k) = match self.shallow(&lt) {
                        Ty::Adt(n, _) => Some(n),
                        t => crate::adt::ext_key_of(&self.zonk(&t))
                            .filter(|k| self.adts.ext_types.contains(*k))
                            .map(str::to_string),
                    }
                    && let Some((key, p)) = self.pick_cmp_impl(&k, "eq", &rt)
                {
                    self.unify(&p, &rt);
                    let r = self.range(b.syntax());
                    self.conversions.insert(r, crate::adt::Conv::Method { key });
                    return Ty::Bool;
                }
                let tr = if matches!(cmp, CmpOp::Ord { .. }) {
                    "PartialOrd"
                } else {
                    "PartialEq"
                };
                // `x != y` with `Elem1: PartialEq<Elem2>`.
                if self.cmp_bound_fits(&lt, &rt, tr) {
                    let r = self.range(b.syntax());
                    self.bound_cmps.insert(r);
                    return Ty::Bool;
                }
                if !self.unify(&lt, &rt) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `{}`",
                        self.describe(&lt),
                        self.describe(&rt)
                    );
                    self.error(rhs.syntax(), msg);
                }
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
                // `b << 2` with your `impl Shl<u32> for Bits`.
                if let Ty::Adt(n, _) = self.shallow(&lt) {
                    return self.op_overload(b.syntax(), op, false, &n, &lt, &rhs, None);
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
                // `&a | &b` (`&`, `-`, `^`) on two sets: a new set.
                if is_set(&self.shallow(&lt))
                    && matches!(
                        op,
                        ArithOp::BitOr | ArithOp::BitAnd | ArithOp::Sub | ArithOp::BitXor
                    )
                {
                    self.check(&rhs, &lt);
                    return lt;
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

    /// Whether `e` is certainly a `&str` (not a `String`): a string literal, `&x`, or a local
    /// declared `&str` or initialized with one (`str_locals`).
    fn borrowed_str(&self, e: &ast::Expr) -> bool {
        match e {
            ast::Expr::ParenExpr(p) => p.expr().is_some_and(|x| self.borrowed_str(&x)),
            ast::Expr::Literal(l) => matches!(l.kind(), ast::LiteralKind::String(_)),
            ast::Expr::RefExpr(_) => true,
            ast::Expr::PathExpr(p) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(n.text().as_ref()))
                .is_some_and(|l| l.reference == Some(false) || l.decl.is_some_and(|d| self.str_locals.contains(&d))),
            _ => false,
        }
    }

    /// A `let` initializer that is certainly a `&str`: what `borrowed_str` accepts, or a method
    /// giving one (`x.as_str()`, `x.trim()`).
    fn str_init(&self, e: &ast::Expr) -> bool {
        match e {
            ast::Expr::ParenExpr(p) => p.expr().is_some_and(|x| self.str_init(&x)),
            ast::Expr::MethodCallExpr(m) => {
                m.arg_list().is_some_and(|a| a.args().next().is_none())
                    && m.name_ref().is_some_and(|n| {
                        matches!(
                            n.text().to_string().as_str(),
                            "as_str" | "trim" | "trim_start" | "trim_end"
                        )
                    })
            }
            _ => self.borrowed_str(e),
        }
    }

    fn is_integer(&self, t: &Ty) -> bool {
        matches!(self.shallow(t), Ty::Int(_) | Ty::Unknown | Ty::Never)
            || self.var_kind(t) == Some(VarKind::Int)
    }

    /// Check the right operand of `lhs op rhs` (or `lhs op= rhs`) and the operator's validity.
    fn arith_operands(&mut self, b: &ast::BinExpr, op: ArithOp, lt: &Ty, rhs: &ast::Expr) -> Ty {
        if let Ty::Adt(n, _) = self.shallow(lt) {
            let assign = matches!(b.op_kind(), Some(BinaryOp::Assignment { .. }));
            return self.op_overload(b.syntax(), op, assign, &n, lt, rhs, None);
        }
        if self.shallow(lt) == Ty::Str && op == ArithOp::Add {
            // `"a" + "b"` / `&s + ".rs"` / `name + "!"` with `name: &str`: `+` takes a `String`
            // on the left (rustc's E0369).
            if let Some(l) = b.lhs()
                && !matches!(b.op_kind(), Some(BinaryOp::Assignment { .. }))
                && self.borrowed_str(&l)
            {
                // rustc's caret is on the `+`, with each side's type under it.
                let lhs_s = if matches!(l, ast::Expr::RefExpr(_)) { "&String" } else { "&str" };
                let rhs_s = if self.borrowed_str(rhs) {
                    "&str"
                } else if matches!(rhs, ast::Expr::RefExpr(_)) {
                    "&String"
                } else {
                    "String"
                };
                let msg = format!("cannot add `{rhs_s}` to `{lhs_s}`");
                let help = format!(
                    "create an owned `String` from the string reference: `{}.to_owned() + ..`, or use `format!`",
                    l.syntax().text()
                );
                let at = b
                    .op_token()
                    .map(|t| {
                        let r = t.text_range();
                        (self.base + usize::from(r.start()), self.base + usize::from(r.end()))
                    })
                    .unwrap_or_else(|| self.range(b.syntax()));
                let spans = vec![
                    (self.range(l.syntax()).0, self.range(l.syntax()).1, lhs_s.to_string()),
                    (self.range(rhs.syntax()).0, self.range(rhs.syntax()).1, rhs_s.to_string()),
                ];
                self.error_spans(at, msg, Some(&help), spans);
            }
            self.check(rhs, &Ty::Str);
            return Ty::Str;
        }
        let rt = self.infer(rhs, Some(lt));
        // `2.0 * v` with your `impl Mul<V> for f64`.
        let assign = matches!(b.op_kind(), Some(BinaryOp::Assignment { .. }));
        if let Some(k) = self.prim_op_key(op, assign, lt, &rt) {
            return self.op_overload(b.syntax(), op, assign, &k, lt, rhs, Some(rt));
        }
        if !self.unify(lt, &rt) {
            let (l, r) = (self.describe(lt), self.describe(&rt));
            let msg = match op {
                ArithOp::Add => format!("cannot add `{r}` to `{l}`"),
                ArithOp::Sub => format!("cannot subtract `{r}` from `{l}`"),
                ArithOp::Mul => format!("cannot multiply `{l}` by `{r}`"),
                ArithOp::Div => format!("cannot divide `{l}` by `{r}`"),
                _ => format!("no implementation for `{l} {op} {r}`"),
            };
            if self.numeric(lt) && self.numeric(&rt) {
                self.error_help(b.syntax(), msg, diag_help::NUMBERS);
            } else {
                self.error(b.syntax(), msg);
            }
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

    /// A closure call's arguments; a `&mut i32` / `&mut String` parameter takes a cell
    /// (`f(&mut n)` passes `n`'s).
    fn closure_call_args(&mut self, args: &[ast::Expr], params: &[Ty]) {
        for (a, p) in args.iter().zip(params) {
            if matches!(self.shallow(p), Ty::MutRef(_)) {
                self.cell_arg(a);
            }
            self.check(a, p);
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
                self.closure_call_args(&args, &params);
                return *ret;
            }
            for a in &args {
                self.infer(a, None);
            }
            return Ty::Unknown;
        };
        if let Some(t) = self.qualified_call(c, &callee, &args, expected) {
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
                self.closure_call_args(&args, &params);
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
        let from_alias = std_alias.is_some();
        let names: Vec<String> = std_alias.unwrap_or_else(|| {
            segs.iter()
                .filter_map(|s| s.name_ref())
                .map(|n| n.text().to_string())
                .collect()
        });
        // (`std::vec::Vec::new()`: as `Vec::new()`.)
        let names = crate::ty::strip_prelude_modules(names);
        if names.len() >= 2 && !from_alias {
            self.require_std_import(callee.syntax(), &names[0].clone(), false);
        }
        // `Self::make()` in a blanket impl (`T::make()`), or in an impl for a built-in type,
        // also in a default copied there (`i32::make()`).
        let names: Vec<String> = match (names.as_slice(), self.self_ty.clone()) {
            ([s, f], Some(Ty::Param(p))) if s == "Self" && p != "Self" => vec![p, f.clone()],
            ([s, f], Some(t)) if s == "Self" && !matches!(t, Ty::Param(_) | Ty::Adt(..)) => {
                match crate::adt::ext_key_of(&t) {
                    Some(k) if self.adts.ext_types.contains(k) => vec![k.to_string(), f.clone()],
                    _ => names,
                }
            }
            _ => names,
        };
        // `Self::Error::custom(..)` in a trait's default method, with `type Error: Error;`: the
        // implementing type's `Error` (its class's `$at$Error`).
        if let [s, a, f] = names.as_slice()
            && s == "Self"
            && self.self_projection(a)
        {
            let p = format!("Self::{a}");
            if let Some(t) = self.param_static_call(c, &p, f, &args) {
                let r = self.range(c.syntax());
                self.param_paths.insert(r, p);
                return t;
            }
        }
        // `T::E::mk(..)` with `T: Has` and `trait Has { type E: Mk; }`: `T`'s class's `$at$E`.
        if let [p, a, f] = names.as_slice()
            && p != "Self"
            && self.param_bounds.contains_key(p)
            && !self.bound_traits(&format!("{p}::{a}")).is_empty()
        {
            let p = format!("{p}::{a}");
            if let Some(t) = self.param_static_call(c, &p, f, &args) {
                let r = self.range(c.syntax());
                self.param_paths.insert(r, p);
                return t;
            }
        }
        // `i32::make()` / `Vec::<u8>::make()` / `Q::make()`: a blanket impl's associated function.
        if let [t, f] = names.as_slice()
            && let Some(qt) = self.blanket_qualifier(&path, t)
            && let Some(ty) = self.blanket_static_call(c, &qt, f, &args)
        {
            return ty;
        }
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
        // `ScriptArg::Str("x")`, `ScriptArg::try_from(v)`: a bindings enum's variant.
        if let Some(t) = self.extern_enum_call(c, &names, &args) {
            return t;
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
            if let Some(t) = self.any_call(c, tail, &args) {
                return t;
            }
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
                let impl_args = sig.impl_args.clone();
                let dict_names = sig.dict_names();
                let sig = self.method_inst(&sig, None);
                // (`<Vec<u16>>::name()` of `impl<T: Codec> Codec for Vec<T>`: `T`'s class, for
                // `T::name()` in its body.)
                if dict_names.len() > sig.type_params.len()
                    && let Some(caller) = self.cur_fn.clone()
                {
                    let qseg = path.qualifier().and_then(|q| q.segment());
                    let written = qseg
                        .as_ref()
                        .and_then(|s| s.type_anchor())
                        .and_then(|a| a.ty())
                        .map(|t| self.lower_type(&t));
                    let elems: Vec<Ty> = match written {
                        Some(Ty::Vec(e) | Ty::Option(e)) => vec![*e],
                        Some(Ty::Tuple(es)) => es,
                        // (`Vec::<u16>::name()`.)
                        _ => qseg
                            .and_then(|s| s.generic_arg_list())
                            .into_iter()
                            .flat_map(|l| l.generic_args())
                            .filter_map(|g| match g {
                                ast::GenericArg::TypeArg(t) => t.ty(),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .iter()
                            .map(|t| self.lower_type(t))
                            .collect(),
                    };
                    let mut targs = std::mem::take(&mut self.last_type_args);
                    // (`method_inst` made the impl's parameters fresh, after the method's own.)
                    targs.resize(dict_names.len(), Ty::Unknown);
                    for (j, n) in dict_names.iter().enumerate().skip(sig.type_params.len()) {
                        let i = impl_args.iter().position(|a| matches!(a, Ty::Param(p) if p == n));
                        if let Some(e) = i.and_then(|i| elems.get(i)) {
                            let t = targs[j].clone();
                            self.unify(&t, e);
                        }
                    }
                    let r = self.range(c.syntax());
                    self.generic_calls.push((r, caller, format!("{key}::{f}"), targs));
                }
                self.arg_count(c.syntax(), "function", sig.params.len(), args.len());
                for (a, p) in args.iter().zip(&sig.params) {
                    self.check(a, p);
                }
                self.record_ext(c.syntax(), key);
                // (`String::decode(..)` next to `impl Decode for &str`: by the path's type.)
                if self.twin_name(f) {
                    let side = path.qualifier().and_then(|q| self.qualifier_owned(&q, c.syntax()));
                    let r = self.range(c.syntax());
                    self.twin_sides.insert(r, (f.to_string(), side));
                }
                return sig.ret;
            }
        }
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        match std_tail(&names) {
            ["String", "new"] => {
                self.arg_count(c.syntax(), "function", 0, args.len());
                Ty::Str
            }
            ["String", "from"] if self.adts.from_impls.contains_key("String") => {
                // Your `impl From<A> for String`, or the string itself.
                let at = args.first().map(|a| self.infer(a, None));
                if let Some(at) = at
                    && !self.could_be(&at, &Ty::Str)
                {
                    self.conv_from_call(c, "String", &args);
                    return Ty::Str;
                }
                self.arg_count(c.syntax(), "function", 1, args.len());
                Ty::Str
            }
            ["String", "from"] => {
                self.arg_count(c.syntax(), "function", 1, args.len());
                if let Some(a) = args.first() {
                    self.check(a, &Ty::Str);
                }
                Ty::Str
            }
            ["Vec" | "vec", ctor @ ("new" | "with_capacity" | "default")] => {
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
            // `Vec::from(array)` / `Vec::from(&slice[..])` / `Vec::from(v)`, and `Vec::from("ab")`'s
            // bytes.
            ["Vec" | "vec", "from"] => {
                self.arg_count(c.syntax(), "function", 1, args.len());
                let elem = self
                    .segment_targs(&segs, "Vec")
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| self.fresh(VarKind::General));
                if let Some(a) = args.first() {
                    let at = self.infer(a, None);
                    if matches!(self.shallow(&at), Ty::Str) {
                        self.unify(&elem, &Ty::Int(IntKind::U8));
                    } else {
                        let want = Ty::Vec(Box::new(elem.clone()));
                        if !self.unify(&at, &want) {
                            let msg = format!(
                                "the trait bound `Vec<{}>: From<{}>` is not satisfied",
                                self.describe(&elem),
                                self.describe(&at)
                            );
                            self.error(a.syntax(), msg);
                        }
                    }
                }
                Ty::Vec(Box::new(elem))
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
                let text = path.syntax().text().to_string();
                self.unresolved_path(callee.syntax(), "function", &text);
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
            } else if matches!(&ty, Ty::Extern(n) if n == "ReactNode") {
                // `ns.print_raw(x)`: a node, text, a number, ..
                self.check_node_arg(a);
            } else if let Ty::Extern(n) = &ty
                && self.bindings.is_enum(n)
            {
                // `ns.run("x.rs", 4, "n00dles")`: the enum, or any variant's payload.
                let n = n.clone();
                self.check_enum_arg(a, &n);
            } else if matches!(ty, Ty::Float { .. }) && is_int_literal(a) {
                // Deliberate extension: every NS number is `f64`, and `ns.sleep(100)` is how every
                // Bitburner script reads, so unsuffixed integer literals are accepted here.
                self.record(a.syntax(), ty);
            } else {
                let before = self.diags.len();
                self.enum_payloads = true;
                self.check(a, &ty);
                self.enum_payloads = false;
                // `ns.hack(t)` with `t: String` where the parameter is `&str`: rustc's E0308.
                if self.diags.len() == before
                    && let Some(p) = sig.params.get(i)
                    && p.shown.starts_with('&')
                {
                    let shown = p.shown.clone();
                    self.check_ref_arg(a, &shown);
                }
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

    /// `String::from` values: your `From` impl for their argument, or the string itself.
    fn resolve_string_froms(&mut self) {
        for (n, a) in std::mem::take(&mut self.pending_string_froms) {
            if self.could_be(&a, &Ty::Str) {
                self.unify(&a, &Ty::Str);
                continue;
            }
            match self.pick_from("String", &a) {
                Some(key) => self.record_conv(
                    &n,
                    crate::adt::Conv::From {
                        adt: "String".into(),
                        key,
                    },
                ),
                None => {
                    let msg = format!(
                        "the trait bound `String: From<{}>` is not satisfied",
                        self.describe(&a)
                    );
                    self.error(&n, msg);
                }
            }
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
        // `*r? += 1` of a local holding `m.get_mut(k).ok_or(e)`: the slot.
        if self.result_slot_local(&inner).is_some() {
            self.mark_slot_yield(&ast::Expr::TryExpr(t.clone()));
        }
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
        let ref_params_raw = sig.ref_params.clone();
        let bound_args = sig.bound_args.clone();
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
            // `S: Store<T>`: `T` follows from `S`'s impl of `Store`, once `S` is known.
            if !bound_args.is_empty() {
                let m: HashMap<String, Ty> = type_params
                    .iter()
                    .map(|(n, _)| n.clone())
                    .zip(self.last_type_args.iter().cloned())
                    .collect();
                for (p, tr, args) in &bound_args {
                    if let Some(sv) = m.get(p) {
                        let args = args.iter().map(|a| generic::subst(a, &m)).collect();
                        self.pending_trait_args.push((sv.clone(), tr.clone(), args));
                    }
                }
            }
            if let Some(caller) = self.cur_fn.clone() {
                let r = self.range(c.syntax());
                let targs = std::mem::take(&mut self.last_type_args);
                self.note_twin_targs(r, &type_params, &params, &ref_params_raw, args);
                self.generic_calls
                    .push((r, caller, name.to_string(), targs));
            }
            inst
        };
        self.arg_count(c.syntax(), "function", params.len(), args.len());
        let opt_slot_params = self
            .fns
            .get(name)
            .map(|s| s.opt_slot_params.clone())
            .unwrap_or_default();
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
        let ref_params = self
            .fns
            .get(name)
            .map(|s| s.ref_params.clone())
            .unwrap_or_default();
        for (i, a) in args.iter().enumerate() {
            if cell_params.get(i) == Some(&true) {
                self.cell_arg(a);
            }
            match params.get(i) {
                Some(p) if into_params.get(i) == Some(&true) => {
                    self.check_into_arg(a, p);
                }
                // `impl Read` given bytes (`&[u8]`): read through a `Cursor` (`byte_reader`).
                Some(p @ Ty::Dyn(tr, _)) if tr == "IoRead" || tr == "IoBufRead" => {
                    let t = self.infer(a, None);
                    if !self.is_bytes(&t) {
                        self.expect(a.syntax(), &t, p);
                    }
                }
                Some(p) => {
                    let before = self.diags.len();
                    self.check(a, p);
                    // `f(s)` with `s: String` where `fn f(s: &str)`: rustc's E0308.
                    if self.diags.len() == before
                        && cell_params.get(i) != Some(&true)
                        && let Some(Some(shown)) = ref_params.get(i)
                    {
                        self.check_ref_arg(a, shown);
                    }
                }
                None => {
                    self.infer(a, None);
                }
            }
        }
        // `show((3, &mut x))` with `fn show(t: (usize, &mut i32))`: the tuple holds a slot.
        let tuple_slot_params = self
            .fns
            .get(name)
            .map(|s| s.tuple_slot_params.clone())
            .unwrap_or_default();
        for (a, s) in args.iter().zip(tuple_slot_params) {
            if let Some(s) = s {
                let a = match strip_parens(a) {
                    ast::Expr::RefExpr(r) => r.expr().unwrap_or_else(|| a.clone()),
                    a => a,
                };
                self.keep_slot_shape(&a, &s);
            }
        }
        // `*at(&mut v, 0) += 1` with `fn at(..) -> &mut i32`: the call is a slot.
        if let Some(s) = self.fns.get(name) {
            let (kind, declared) = (s.ret_slot, s.ret.clone());
            self.note_call_slot(&ast::Expr::CallExpr(c.clone()), kind, &declared, &ret);
        }
        self.finish_slot_args(args, &opt_slot_params);
        self.check_byte_readers(name, args, &params);
        self.resolve_projections(false);
        if is_async {
            Ty::Future(Box::new(ret))
        } else {
            ret
        }
    }

    /// Whether the range `r` is a `Range` value for your `impl Tr for Range<..>` rather than an
    /// iterator: the receiver of one of those impls' methods (`(0..10).width()`), under `&`
    /// (`w(&(1..2))`), or an array element (`[0..2, 3..9]`).
    fn range_ext_value(&self, r: &ast::RangeExpr, kind: RangeKind) -> bool {
        let key = kind.name();
        if !self.adts.ext_types.contains(key) {
            return false;
        }
        let mut node = r.syntax().clone();
        while let Some(p) = node
            .parent()
            .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
        {
            node = p;
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        if let Some(m) = ast::MethodCallExpr::cast(parent.clone()) {
            return m.receiver().is_some_and(|x| x.syntax() == &node)
                && m.name_ref().is_some_and(|n| {
                    self.adts
                        .method(key, n.text().to_string().as_str())
                        .is_some_and(|s| s.receiver.is_some())
                });
        }
        matches!(
            parent.kind(),
            ra_ap_syntax::SyntaxKind::REF_EXPR | ra_ap_syntax::SyntaxKind::ARRAY_EXPR
        )
    }

    /// Whether a range passed where `t` is expected is a value: `t` is a generic call's type
    /// argument bound by a trait the range type implements (`take(5..=6)` with `R:
    /// RangeBounds<i32>`), not an iterator.
    fn range_bound_arg(&self, t: &Ty, kind: RangeKind) -> bool {
        let key = kind.name();
        let implemented = |b: &str| {
            self.adts.traits.get(b).is_some_and(|tr| {
                tr.methods
                    .iter()
                    .any(|m| self.adts.method(key, &m.name).is_some())
            })
        };
        match self.shallow(t) {
            // (`r: impl RangeBounds<u8>`.)
            Ty::Dyn(b, _) => implemented(&b),
            t @ Ty::Var(_) => self
                .pending_bounds
                .iter()
                .any(|(v, b, ..)| self.shallow(v) == t && implemented(b)),
            _ => false,
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
            ref z => self.adts.atom_ty(z),
        }
    }

    /// A number, string, bool or char, or an immutable enum (`Adts::is_atom`): a value JS can't
    /// point into, so a `&mut` to one in a collection is a slot.
    pub(super) fn prim(&self, t: &Ty) -> bool {
        t.is_js_primitive() || self.adts.atom_ty(t)
    }

    /// A `&mut` parameter of this type takes a cell: a number, string, bool or `Option`, or a type
    /// parameter (which may be one: `fn bump<T: AddAssign>(x: &mut T, d: T)`).
    pub(super) fn cell_param(&self, t: &Ty) -> bool {
        self.cellable(t) || matches!(self.zonk(t), Ty::Param(_))
    }

    /// `&mut x` with `x` a number/string/bool/Option local (or a `&str` one, for a `&mut &str`):
    /// `x` lives in a cell.
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
        if local.reference != Some(true)
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
                    // Any other local, for a `&mut T` parameter: a getter/setter cell over it. (And
                    // a `&[u8]` one, for an `R: Read` parameter's `&mut R`.)
                    ast::Expr::PathExpr(p)
                        if p.path().and_then(|p| p.as_single_name_ref()).is_some_and(|n| {
                            self.lookup(n.text().to_string().as_str()).is_some_and(|l| {
                                (l.reference.is_none()
                                    || l.reference == Some(false) && matches!(self.shallow(&l.ty), Ty::Vec(_)))
                                    && !self.cellable(&l.ty)
                            })
                        }) =>
                    {
                        let rr = self.range(r.syntax());
                        self.slot_args.insert(rr);
                    }
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
                    if self.cell_param(&local.ty) {
                        self.cell_args.insert(r);
                    } else {
                        self.slot_args.insert(r);
                    }
                }
            }
            // `bump(s.at(0))` / `bump(o.unwrap())`: a slot-valued argument, wrapped as a cell
            // once the arguments are inferred (`finish_slot_args`).
            other => self.pending_cell_args.push(other.clone()),
        }
    }

    /// After a call's arguments are checked: slot-valued arguments to `&mut i32` parameters
    /// (`slot_cell_args`), and `Option<&mut i32>` arguments kept as `null` or a slot.
    pub(super) fn finish_slot_args(&mut self, args: &[ast::Expr], opt_slot_params: &[bool]) {
        for a in std::mem::take(&mut self.pending_cell_args) {
            if self.keep_slot(&a) {
                let r = self.range(a.syntax());
                self.slot_cell_args.insert(r);
            }
        }
        for (i, a) in args.iter().enumerate() {
            if opt_slot_params.get(i) == Some(&true) {
                self.keep_opt_slot(a);
            }
        }
    }

    /// Calls through `C: Conv<u8> + Conv<u16>`: the bound whose arguments fit what the call's
    /// trait arguments became (`let a: u8 = c.conv()`, an integer literal), else the first, as
    /// a later use can't change which impl runs.
    fn resolve_bound_picks(&mut self) {
        for (label, vars, alts, node) in std::mem::take(&mut self.pending_bound_picks) {
            let fits = |c: &Self, alt: &[Ty]| {
                vars.iter().zip(alt).all(|(v, a)| match c.var_kind(v) {
                    Some(VarKind::Int) => matches!(a, Ty::Int(_)),
                    Some(VarKind::Float) => matches!(a, Ty::Float { .. }),
                    Some(VarKind::General) => true,
                    None => c.zonk(v) == *a,
                })
            };
            let Some(alt) = alts.iter().find(|a| fits(self, a)).cloned() else {
                let got: Vec<String> = vars.iter().map(|v| self.zonk(v).describe()).collect();
                self.error(
                    &node,
                    format!("the trait bound `{label}<{}>` is not satisfied", got.join(", ")),
                );
                continue;
            };
            for (v, a) in vars.iter().zip(&alt) {
                self.unify(v, a);
            }
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
        // (`where Meters: Scale<R>`: the trait's method with the clause's arguments.)
        let shallow = self.shallow(&rt);
        if let Some(t) = self.where_self_method(m, &recv, &shallow, &method, &args) {
            return t;
        }
        // `b.as_ref()` with `B: AsRef<[u8]>`: what the bound says.
        if let Some(t) = self.conv_as_ref_call(m, &method) {
            return t;
        }
        // `self.key.as_ref()` with the impl's `T: AsRef<[u8]>`.
        if let Some(t) = self.conv_param_as_ref_call(m, &method, &rt) {
            return t;
        }
        // `s.as_ref()` on a string as a `&[u8]` or `&OsStr`.
        if let Some(t) = self.conv_str_as_ref_call(m, &method, &rt, expected) {
            return t;
        }
        // `5.seconds()` with your trait implemented for `i64` only: the literal is an `i64`, as in
        // rustc (several impls: the fallback `i32` / `f64` when it's one of them).
        if let Some(kind @ (VarKind::Int | VarKind::Float)) = self.var_kind(&rt) {
            let fallback = if kind == VarKind::Int { "i32" } else { "f64" };
            let mut fits: Vec<&str> = if kind == VarKind::Int {
                vec![
                    "i8", "i16", "i32", "i64", "isize", "i128", "u8", "u16", "u32", "u64", "usize",
                    "u128",
                ]
            } else {
                vec!["f32", "f64"]
            };
            let std = crate::service::std_methods(&Ty::Int(IntKind::I32));
            fits.retain(|k| {
                self.adts.ext_types.contains(*k)
                    && self.adts.method(k, &method).is_some_and(|s| s.receiver.is_some())
            });
            let pick = match fits.as_slice() {
                [one] => Some(*one),
                [_, ..] if fits.contains(&fallback) => Some(fallback),
                _ => None,
            };
            if let Some(k) = pick
                && !std.contains(&method.as_str())
            {
                self.unify(&rt, &Ty::from_name(k));
            }
        }
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
                    // (`Into` is every type's own: `host.into()` doesn't look through `Deref`.)
                    | "into"
            )
            && !(method == "to_string" && self.adts.defs.get(&n).is_some_and(|d| d.display))
        {
            self.auto_deref(&recv, &n, &target);
            self.record(recv.syntax(), target.clone());
            rt = target;
        }
        if let Some(t) = self.std_cmp_call(m, &rt, &method, &args) {
            return t;
        }
        // A user trait method on a built-in type (`x.money()` with `impl Money for f64`); std's own
        // methods come first, as inherent methods do in Rust.
        // (A generic trait implemented twice for the type: the impl whose signature fits.)
        // (Impls for two tuples of one length: by the receiver's elements.)
        let method = match crate::adt::ext_key_of(&self.shallow(&rt)) {
            // (One trait for both `str` and `String` (`[T]` and `Vec<T>`): by how the receiver is
            // written.)
            Some(key @ ("String" | "Vec"))
                if self.adts.own_twins.contains(&format!("{key}::{method}")) =>
            {
                self.pick_twin(m, &recv, key, &method)
            }
            Some(key) => {
                let targs = match self.shallow(&rt) {
                    t @ Ty::Tuple(_) => crate::adt::ext_args(&t),
                    // (A slice's element: `[u8]` next to `[T]`.)
                    t @ Ty::Vec(_) => crate::adt::ext_args(&t),
                    _ => Vec::new(),
                };
                self.pick_method_alt(m, key, &targs, &method, &args, expected)
                    .unwrap_or(method)
            }
            None => method,
        };
        // (`impl Inv for &f32` next to `impl Inv for f32`: by how the receiver is written.)
        let method = self.pick_ref_impl(m, &recv, &rt, method);
        if let Some(key) = crate::adt::ext_key_of(&self.shallow(&rt))
            && self.adts.ext_types.contains(key)
            && let Some(sig) = self.adts.method(key, &method).cloned()
            && sig.receiver.is_some()
            && !crate::service::std_methods(&self.shallow(&rt)).contains(&method.as_str())
        {
            if self.slice_read(key, &method, &rt) {
                self.check_slice_reader(&recv);
            } else if sig.receiver == Some(crate::adt::Receiver::MutRef) {
                self.check_mutable(&recv, Mutation::Borrow);
            }
            // The impl's type arguments: its parameters take the receiver's, and the receiver
            // takes its concrete ones (`HashMap::new()` is a `HashMap<String, i64>` once it calls
            // `impl Total for HashMap<String, i64>`'s method).
            // (`shallow`: an unsettled argument stays a variable, to settle here: `(0..10).width()`
            // with `impl Width for Range<usize>`.)
            let recv_args = crate::adt::ext_args(&self.shallow(&rt));
            for (ra, ia) in recv_args.iter().zip(&sig.impl_args) {
                if !matches!(ia, Ty::Param(_)) {
                    self.unify(ra, ia);
                }
            }
            let dicts = sig.dict_names().len();
            let sig = if !recv_args.is_empty() && recv_args.len() == sig.impl_args.len() {
                self.method_inst(&sig, Some(&recv_args))
            } else {
                self.method_inst(&sig, None)
            };
            // (The method's own type parameters' classes and those of its impl's that only the
            // trait's arguments name, for `E::default()` in its body.)
            if dicts > 0 {
                let own = std::mem::take(&mut self.last_type_args);
                if let Some(gal) = m.generic_arg_list() {
                    let given: Vec<Ty> = gal
                        .generic_args()
                        .collect::<Vec<_>>()
                        .iter()
                        .filter_map(|g| self.generic_arg_ty(g))
                        .collect();
                    for (v, t) in own.iter().zip(given) {
                        self.unify(v, &t);
                    }
                }
                let r = self.range(m.syntax());
                let caller = self.cur_fn.clone().unwrap_or_default();
                self.generic_calls
                    .push((r, caller, format!("{key}::{method}"), own));
            }
            self.arg_count(m.syntax(), "method", sig.params.len(), args.len());
            for (i, (a, p)) in args.iter().zip(&sig.params).enumerate() {
                // (`&mut String`: `bytes.read_to_string(&mut text)`.)
                if sig.cell_params.get(i) == Some(&true) {
                    self.cell_arg(a);
                }
                self.check(a, p);
            }
            self.record_ext(m.syntax(), key);
            self.note_trait_call(m, key, &method, &rt);
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
            // The impl's `T` (and `Self`) is the receiver's type: `fn into_source(self) -> T`.
            let recv_ty = self.zonk(&rt);
            let sig = self.blanket_inst(&key, &sig, &recv_ty);
            self.arg_count(m.syntax(), "method", sig.params.len(), args.len());
            for (a, p) in args.iter().zip(&sig.params) {
                self.check(a, p);
            }
            self.record_ext(m.syntax(), &key);
            self.note_trait_call(m, &key, &method, &rt);
            return if sig.is_async {
                Ty::Future(Box::new(sig.ret))
            } else {
                sig.ret
            };
        }
        // `x.type_id()`, `any.is::<T>()` / `downcast_ref::<T>()`.
        if !matches!(self.shallow(&rt), Ty::Adt(n, _) if self.adts.method(&n, &method).is_some())
            && let Some(t) = self.any_method(m, &rt, &method, &args)
        {
            return t;
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
                    "start" | "end" if incl == crate::ty::RangeKind::Inclusive => {
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
            // `v.length()`: the JavaScript name.
            if let Some(alias) = diag_help::js_alias(&method, false) {
                let msg = format!(
                    "no method named `{method}` found for `{}` (you might have meant to use {alias})",
                    c.describe(ty)
                );
                c.error_help(m.syntax(), msg, diag_help::JS_NAMES);
                return Ty::Unknown;
            }
            let hint = if matches!(method.as_str(), "context" | "with_context")
                && matches!(ty, Ty::Result(..) | Ty::Option(_))
            {
                " (if you meant `anyhow::Context`'s, add `use anyhow::Context;`)"
            } else {
                ""
            };
            let msg = format!(
                "no method named `{method}` found for `{}`{hint}",
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
            "to_string" | "clone" | "clone_from" | "eq" | "ne" | "partial_cmp" | "lt" | "le" | "gt"
                | "ge"
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
        // `a.clone_from(&b)`: `*a = b.clone()`, for a `Clone` type without a `clone_from` of
        // its own.
        if method == "clone_from"
            && !matches!(&resolved, Ty::Adt(n, _) if self.adts.method(n, "clone_from").is_some())
            && self.satisfies(&resolved, "Clone")
        {
            self.check_mutable(&recv, Mutation::Borrow);
            self.arg_count(m.syntax(), "method", 1, args.len());
            if let Some(a) = args.first() {
                self.check(a, &resolved);
            }
            return Ty::Unit;
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
                let method = self
                    .pick_method_alt(m, &name, &targs, &method, &args, expected)
                    .unwrap_or(method);
                return self.adt_method(m, &recv, &name, &targs, &method, &args);
            }
            // (A method that several impls of one generic trait give a type: told apart at run
            // time by its arguments when their types differ there, see `alt_dispatch`.)
            // (Through a type parameter the trait's arguments' classes come with the call and
            // pick it, see `alt_trait_args`; through `dyn Scale<f64>` its written arguments do.)
            Ty::Param(_) | Ty::Dyn(_, _)
                if self.adts.method_alts.keys().any(|k| {
                    k.strip_suffix(&format!("::{method}")).is_some_and(|ty| {
                        !self.adts.inherent_alts.contains(k)
                            && self.adts.alt_dispatch(ty, &method).is_none()
                            && (matches!(&resolved, Ty::Dyn(_, a) if a.is_empty())
                                || self.adts.alt_trait_args(ty, &method).is_none())
                    })
                }) =>
            {
                let msg = format!(
                    "`{method}` comes from a generic trait implemented more than once for one type; through `dyn` the impl is picked by the arguments at run time, and these impls take arguments that look alike in JS (two number types, or `str` and `char`): call it on the concrete type or through a type parameter"
                );
                self.error(m.syntax(), msg);
                for a in &args {
                    self.infer(a, None);
                }
                return Ty::Unknown;
            }
            Ty::Param(name) => return self.param_method(m, &recv, &name, &method, &args),
            Ty::Dyn(tr, targs) => return self.dyn_method(m, &recv, &tr, &targs, &method, &args),
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
                    "try_borrow" | "try_borrow_mut" => {
                        self.arg_count(m.syntax(), "method", 0, args.len());
                        let err = if method == "try_borrow" { "BorrowError" } else { "BorrowMutError" };
                        Ty::Result(Box::new(inner), Box::new(Ty::Extern(err.into())))
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
                    let t = sig(self, &[Ty::Str], Ty::Unit);
                    // `s.push_str(t)` with an owned `t`: rustc wants `&t`.
                    if let Some(a) = args.first()
                        && self.raw.get(&self.range(a.syntax())).map(|t| self.zonk(t)) == Some(Ty::Str)
                    {
                        self.check_ref_arg(a, "&str");
                    }
                    t
                }
                "push" => {
                    self.check_mutable(&recv, Mutation::Borrow);
                    sig(self, &[Ty::Char], Ty::Unit)
                }
                // `fmt::Write`'s methods: `push_str` / `push` that return `Ok(())`.
                "write_str" | "write_char" => {
                    self.check_mutable(&recv, Mutation::Borrow);
                    let arg = if method == "write_str" {
                        Ty::Str
                    } else {
                        Ty::Char
                    };
                    let fmt_result = Ty::Result(
                        Box::new(Ty::Unit),
                        Box::new(Ty::Extern("fmt::Error".into())),
                    );
                    sig(self, &[arg], fmt_result)
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
                    "push" => {
                        let t = sig(self, &[elem.clone()], Ty::Unit);
                        if let Some(a) = args.first() {
                            self.check_ref_elem(&recv, a, &elem);
                        }
                        // `refs.push(&mut a)` into a `Vec<&mut i32>`: the slot over `a`.
                        if let Some(a) = args.first()
                            && self.slot_vec_local(&recv)
                        {
                            self.keep_slot(a);
                        }
                        t
                    }
                    "insert" => {
                        let t = sig(self, &[USIZE, elem.clone()], Ty::Unit);
                        if let Some(a) = args.get(1) {
                            self.check_ref_elem(&recv, a, &elem);
                        }
                        t
                    }
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
                                format!("the trait bound `{}: Ord` is not satisfied (floats aren't totally ordered; sort them with `sort_by(|a, b| a.total_cmp(b))`)", self.describe(&elem)),
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
                    "iter_mut" => {
                        self.note_iter_mut(m, &elem);
                        sig(
                            self,
                            &[],
                            Ty::Iter {
                                elem: Box::new(elem),
                                mutable: true,
                            },
                        )
                    }
                    // `v.get(1..3)` / `dest.get_mut(n..)`: `Some` of the slice (a view for
                    // `get_mut`) or `None` past the end.
                    "get" | "get_mut"
                        if let Some(ast::Expr::RangeExpr(r)) =
                            args.first().map(|a| strip_parens(a).clone()) =>
                    {
                        use ra_ap_syntax::ast::RangeItem;
                        if m2 == "get_mut" {
                            self.check_mutable(&recv, Mutation::Borrow);
                        }
                        self.arg_count(m.syntax(), "method", 1, args.len());
                        for b in [r.start(), r.end()].into_iter().flatten() {
                            self.check(&b, &USIZE);
                        }
                        Ty::Option(Box::new(Ty::Vec(Box::new(elem))))
                    }
                    "pop" | "first" | "last" => sig(self, &[], Ty::Option(Box::new(elem))),
                    // (`v.get(r)` with a `Range<usize>` value: the slice, as `&v[r]`.)
                    "get" => {
                        self.arg_count(m.syntax(), "method", 1, args.len());
                        let t = args.first().map(|a| (a, self.infer(a, Some(&USIZE))));
                        match t.map(|(a, t)| (a, self.shallow(&t), t)) {
                            Some((_, Ty::Range(e, _), _)) => {
                                self.unify(&e, &USIZE);
                                Ty::Option(Box::new(Ty::Vec(Box::new(elem))))
                            }
                            Some((a, _, t)) => {
                                self.expect(a.syntax(), &t, &USIZE);
                                Ty::Option(Box::new(elem))
                            }
                            None => Ty::Option(Box::new(elem)),
                        }
                    }
                    // The element itself, which writes go through when it's a JS object (a struct,
                    // tuple, array or collection); numbers, strings and bools would be copies.
                    "get_mut" | "first_mut" | "last_mut" => {
                        self.check_mutable(&recv, Mutation::Borrow);
                        let resolved = self.zonk(&elem);
                        if (self.prim(&resolved) || resolved == Ty::Unknown)
                            && slot_position(m)
                        {
                            let r = self.range(m.syntax());
                            self.slot_calls.insert(r);
                            if self.vec_slot_shape(&recv) == Some(slots::SlotShape::Slot) {
                                self.slot_item_calls.insert(r);
                            }
                            if let Some(n) = slot_let_binding(m) {
                                let r = self.range(n.syntax());
                                self.slot_bindings.insert(r);
                            }
                        } else if !only_tested(m) && self.prim(&resolved)
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
                    "unwrap" | "expect" => {
                        // `o.unwrap()` of an `Option<&mut i32>` value (a parameter, a local, a
                        // call of your `-> Option<&mut i32>`; not a `get_mut`-style call, whose
                        // unwrap codegen reads itself): the slot.
                        if let Some(recv) = m.receiver()
                            && self.option_slot_shape(&recv) == Some(slots::SlotShape::Slot)
                            && !matches!(strip_parens(&recv), ast::Expr::MethodCallExpr(x)
                                if self.slot_calls.contains(&self.range(x.syntax())))
                        {
                            let r = self.range(m.syntax());
                            self.slot_values.insert(r);
                        }
                        let params: &[Ty] = if m2 == "unwrap" { &[] } else { &[Ty::Str] };
                        sig(self, params, inner)
                    }
                    "unwrap_or" => {
                        let t = sig(self, &[inner.clone()], inner);
                        // `*m.get_mut(k).unwrap_or(&mut 0) += 1`: the default is a slot too (over
                        // a local, or holding the value).
                        if let Some(ast::Expr::MethodCallExpr(x)) =
                            m.receiver().map(|r| strip_parens(&r))
                            && self.slot_calls.contains(&self.range(x.syntax()))
                            && let Some(a) = m.arg_list().and_then(|l| l.args().next())
                            && !self.keep_slot(&a)
                        {
                            let r = self.range(a.syntax());
                            self.slot_wraps.insert(r, false);
                        }
                        t
                    }
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
                    // `&mut T` into the option (references are erased).
                    "get_or_insert" | "insert" => {
                        self.check_mutable(&recv, Mutation::Borrow);
                        sig(self, &[inner.clone()], inner)
                    }
                    // `opt.get_or_insert_with(Vec::new)` (closures are checked elsewhere).
                    "get_or_insert_with" => {
                        self.check_mutable(&recv, Mutation::Borrow);
                        match (self.ctor_path_ty(args.first()), args.first()) {
                            (Some(made), Some(a)) => {
                                self.record(a.syntax(), made.clone());
                                if !self.unify(&made, &inner) {
                                    let msg = format!(
                                        "mismatched types: expected `{}`, found `{}`",
                                        self.describe(&inner),
                                        self.describe(&made)
                                    );
                                    self.error(a.syntax(), msg);
                                }
                            }
                            (_, a) => {
                                self.closure_arg(a, vec![], inner.clone());
                            }
                        }
                        inner
                    }
                    "get_or_insert_default" => {
                        self.check_mutable(&recv, Mutation::Borrow);
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
                    "as_mut" => {
                        self.mark_as_mut_slot(m, &inner);
                        sig(self, &[], t)
                    }
                    "as_ref" | "as_deref" | "cloned" | "copied" => sig(self, &[], t),
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
                        | "drain"
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
                    "get" | "remove" => {
                        let key = self.lookup_key(&k, args.first());
                        sig(self, &[key], opt(v))
                    }
                    "contains_key" => {
                        let key = self.lookup_key(&k, args.first());
                        sig(self, &[key], Ty::Bool)
                    }
                    "get_mut" => {
                        let resolved = self.zonk(&v);
                        if self.prim(&resolved) && slot_position(m) {
                            let r = self.range(m.syntax());
                            self.slot_calls.insert(r);
                            if let Some(n) = slot_let_binding(m) {
                                let r = self.range(n.syntax());
                                self.slot_bindings.insert(r);
                            }
                        } else if !only_tested(m) && self.prim(&resolved) {
                            let msg = format!(
                                "`HashMap::get_mut` on a map of `{}` values isn't supported here (the value would be copied, so changes wouldn't stick); use it in `if let Some(x) = ..` / `match`, as `*map.get_mut(..).unwrap()`, or `*map.entry(k).or_insert(..) += ..`",
                                self.describe(&v)
                            );
                            self.error(m.syntax(), msg);
                        }
                        let key = self.lookup_key(&k, args.first());
                        sig(self, &[key], opt(v))
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
                    "values_mut" => {
                        self.note_map_iter_mut(m, &v);
                        sig(self, &[], iter(v, true))
                    }
                    "iter" | "into_iter" | "drain" => {
                        sig(self, &[], iter(Ty::Tuple(vec![k, v]), false))
                    }
                    "iter_mut" => {
                        self.note_map_iter_mut(m, &v);
                        sig(self, &[], iter(Ty::Tuple(vec![k, v]), true))
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
                        let made = self.ctor_path_ty(args.first());
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
                if matches!(m2, "insert" | "remove" | "clear" | "drain") {
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
                    "iter" | "into_iter" | "drain" => sig(
                        self,
                        &[],
                        Ty::Iter {
                            elem: Box::new(elem),
                            mutable: false,
                        },
                    ),
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
                    format!("the iterator adapter `.{method}()` is not supported yet; use a `for` loop"),
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
        // `*refs[1] = 9` with `refs: Vec<&mut i32>`: its items are slots over the elements.
        match self.vec_slot_shape(&base) {
            Some(slots::SlotShape::Slot) => {
                let r = self.range(ix.syntax());
                self.slot_values.insert(r);
            }
            // `*tagged[1].1 = 9` with `tagged: Vec<(usize, &mut i32)>`: a field of the item is
            // read (the field access marks a slot); the whole item isn't.
            Some(slots::SlotShape::Tuple(fields)) => {
                let field = ix
                    .syntax()
                    .ancestors()
                    .skip(1)
                    .find(|p| p.kind() != ra_ap_syntax::SyntaxKind::PAREN_EXPR)
                    .and_then(ast::FieldExpr::cast)
                    .and_then(|f| f.name_ref()?.as_tuple_field());
                let r = self.range(ix.syntax());
                let mut ok = self.whole_slot_items.contains(&r)
                    || field.is_some_and(|i| {
                        matches!(fields.get(i), Some(None | Some(slots::SlotShape::Slot)))
                    });
                // `f(&tagged[1])` / `Some(&tagged[1])`: nothing can be written through a
                // `&(usize, &mut i32)`, so a copy with the slots read is the same.
                if !ok
                    && let Some(re) = ix
                        .syntax()
                        .ancestors()
                        .skip(1)
                        .find(|p| p.kind() != ra_ap_syntax::SyntaxKind::PAREN_EXPR)
                        .and_then(ast::RefExpr::cast)
                    && re.mut_token().is_none()
                {
                    self.note_slot_read(&ast::Expr::RefExpr(re));
                    ok = true;
                }
                if !ok {
                    self.error(
                        ix.syntax(),
                        "using a whole item of a `Vec<(_, &mut T)>` collected from `iter_mut()` over numbers, strings or bools by value or as `&mut` isn't supported yet; bind it (`let (i, r) = &mut v[1];`), write through its fields (`*v[1].1 = ..`) or pass `&v[1]` to read it",
                    );
                }
            }
            None => {}
        }
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
        // `map[0]` / `set[1]` by position, `map[&k] = v` (indexmap's `Index` / `IndexMut`).
        if let Ty::Coll(kind @ (crate::ty::Coll::IndexMap | crate::ty::Coll::IndexSet), view) =
            self.shallow(&bt)
            && let Some(t) = self.index_coll_index(ix, kind, &view, &index)
        {
            return t;
        }
        // `deque[i]`, `btree_map[&k]`
        let bt = match self.shallow(&bt) {
            Ty::Coll(
                crate::ty::Coll::VecDeque | crate::ty::Coll::BTreeMap | crate::ty::Coll::IndexMap,
                view,
            ) => *view,
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
                    // `for x in v` with `v: &mut [T]`: as `for x in &mut *v`.
                    ast::Expr::PathExpr(p)
                        if p.path().and_then(|p| p.as_single_name_ref()).is_some_and(|n| {
                            self.lookup(n.text()).is_some_and(|l| l.reference == Some(true))
                        }) =>
                    {
                        let r = self.range(other.syntax());
                        self.mut_ref_loops.insert(r);
                        Some(true)
                    }
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
            if !self.irrefutable_pat(&pat)
                && !self.array_pat_covers(
                    &pat,
                    &elem,
                    &f.iterable().map(|i| self.item_dims(&i)).unwrap_or_default(),
                )
                && !f.iterable().is_some_and(|i| self.pat_fits(&pat, &self.item_shape(&i)))
            {
                self.error(pat.syntax(), "refutable pattern in `for` loop binding");
            }
            self.pat(&pat, &elem);
            // Loop variables over `&v` / `&mut v` are references.
            if let Some(scope) = self.scopes.last_mut() {
                for local in scope.values_mut() {
                    local.reference = reference;
                }
            }
            // Over slots (`for r in refs`, a collected `Vec<&mut i32>`): `r` reads `r[1]`.
            if let Some(shape) = self.for_slot_shape(&iterable) {
                self.bind_slot_shape(&pat, &shape);
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
                help: None,
                spans: Vec::new(),
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
        let saved = self.macro_in_unsafe;
        self.macro_in_unsafe |= in_unsafe(mc.syntax());
        let t = self.macro_call_inner(mc, expected);
        self.macro_in_unsafe = saved;
        t
    }

    fn macro_call_inner(&mut self, mc: &ast::MacroCall, expected: Option<&Ty>) -> Ty {
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
            // `dbg!(x)` is `x` (printed to stderr with `{:#?}`); `dbg!(a, b)` the tuple.
            "dbg" => {
                let expected = match (args.len(), expected.map(|t| self.shallow(t))) {
                    (1, Some(t)) => vec![Some(t)],
                    (n, Some(Ty::Tuple(ts))) if ts.len() == n => ts.into_iter().map(Some).collect(),
                    (n, _) => vec![None; n],
                };
                let tys: Vec<Ty> = args
                    .iter()
                    .zip(expected)
                    .map(|(a, e)| {
                        let t = self.macro_arg(a, e.as_ref());
                        self.check_debug_value(mc.syntax(), &t);
                        t
                    })
                    .collect();
                match <[Ty; 1]>::try_from(tys) {
                    Ok([t]) => t,
                    Err(tys) if tys.is_empty() => Ty::Unit,
                    Err(tys) => Ty::Tuple(tys),
                }
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
                // `let mut buf = Vec::new(); write!(buf, ..)`: a `Vec<u8>`, the one `Vec` writer.
                if let Ty::Vec(e) = self.shallow(&tt)
                    && matches!(self.shallow(&e), Ty::Var(_))
                    && self.adts.traits.contains_key("IoWrite")
                {
                    self.unify(&e, &Ty::Int(IntKind::U8));
                }
                // Into a `fmt::Write` of your own, or a `W: fmt::Write` (a `String`, a `Formatter`
                // or your own writer): its `write_str` of the text.
                if self.adts.traits.contains_key("FmtWrite")
                    && (matches!(self.zonk(&tt), Ty::Dyn(t, _) if t == "FmtWrite")
                        || matches!(self.zonk(&tt), Ty::Adt(..) | Ty::Param(_))
                            && self.implements_user_trait(&tt, "FmtWrite"))
                {
                    if args.len() > 1 {
                        self.format_args(&args[1..]);
                    }
                    self.fmt_writes.insert(target.start);
                    return fmt_result;
                }
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
                            help: None,
                            spans: Vec::new(),
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
                } else if let [a] = args.as_slice() {
                    // (Codegen reports the missing argument; `assert_eq!(v.le` being typed still
                    // has types for the language service.)
                    self.macro_arg(a, None);
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
                    Some(l) => {
                        let t = l.ty.clone();
                        if let Some(js) = self.local_slot_read(n) {
                            self.slot_read_captures.insert((args[0].start, n.clone()), js);
                        }
                        Some(t)
                    }
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
                            help: None,
                            spans: Vec::new(),
                        });
                        None
                    }
                },
            },
        }
    }

    /// Type the arguments of a format macro and check each placeholder can format its value.
    fn format_args(&mut self, args: &[MacroArg]) {
        // `{:?}` of a Vec of slots or an item of one: copied out (`slot_reads`).
        for a in &args[1..] {
            if let (Some(e), _) = macros::parse_arg(a) {
                let saved = std::mem::replace(&mut self.base, a.start);
                self.note_slot_read(&e);
                self.base = saved;
            }
        }
        let tys: Vec<Ty> = args[1..].iter().map(|a| self.macro_arg(a, None)).collect();
        let Some(fmt_str) = macros::format_string(&args[0]) else {
            return;
        };
        let Ok(pieces) = fmt::parse(&fmt_str) else {
            return;
        };
        let mut next = 0;
        // (Which placeholder this is, for rustc's span on it.)
        let mut placeholder = 0;
        for p in pieces {
            let Piece::Arg { arg, spec } = p else {
                continue;
            };
            let this_placeholder = placeholder;
            placeholder += 1;
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
                        help: None,
                        spans: Vec::new(),
                    });
                }
            }
            // (The argument the placeholder takes, where rustc points; a captured name is in
            // the format string.)
            let taken = match &arg {
                ArgRef::Next => Some(next),
                ArgRef::Index(i) => Some(*i),
                ArgRef::Name(n) => args[1..].iter().position(|a| a.name.as_deref() == Some(n.as_str())),
            };
            let (arg_start, arg_end) = match taken.and_then(|i| args.get(i + 1)) {
                Some(a) => (a.start, a.end()),
                None => (args[0].start, args[0].end()),
            };
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
            // `{:p}`: any reference or pointer (they're all erased).
            if spec.kind == Some('p') {
                continue;
            }
            // `{:x?}`: `Debug` with each integer in hex, which a struct's or a generic value's
            // own `Debug` can't be told to do here.
            if spec.debug && spec.kind.is_some() {
                let z = self.zonk(&ty);
                if !hex_debuggable(&z) || spec.width_arg.is_some() {
                    let (start, end) = (args[0].start, args[0].end());
                    let message = format!(
                        "`{{:x?}}` is supported on numbers, strings and collections, options, results and tuples of them, with a literal width (found `{}`)",
                        self.describe(&ty)
                    );
                    self.diags.push(RawDiag {
                        start,
                        end,
                        message,
                        severity: "error",
                        help: None,
                        spans: Vec::new(),
                    });
                    continue;
                }
            }
            // `{:x}` / `{:b}` / `{:o}` need an integer, `{:e}` a number.
            if let Some(k) = spec.kind
                && !spec.debug
            {
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
                    let (start, end) = (arg_start, arg_end);
                    let message = format!(
                        "`{}` doesn't implement `std::fmt::{tr}`",
                        self.describe(&ty)
                    );
                    self.diags.push(RawDiag {
                        start,
                        end,
                        message,
                        severity: "error",
                        help: None,
                        spans: Vec::new(),
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
                let (start, end) = (arg_start, arg_end);
                self.diags.push(RawDiag {
                    start,
                    end,
                    message,
                    severity: "error",
                    help: None,
                    spans: placeholder_at(&args[0], this_placeholder)
                        .map(|(s, e)| vec![(s, e, "required by this formatting parameter".to_string())])
                        .unwrap_or_default(),
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
            Ty::Dyn(tr, _) if tr == "Error" => false,
            // (Or `impl Debug for Box<dyn Tr>`.)
            Ty::Dyn(_, _) => !self.implements(resolved, if debug { "Debug" } else { "Display" }),
            // (A JS type prints with `{}` when it declares `to_string`: `Date`, `Url`, ..)
            Ty::Extern(n)
                if is_std_error(n)
                    || n == "anyhow::Error"
                    // (`{}` of a `JsValue` is JS's `String(v)`: an extension, as it was untyped.)
                    || n == "JsValue"
                    // (A primitive union prints as JS's `String(v)`: `"n00dles"`, `5`, `true`.)
                    || self
                        .bindings
                        .enum_of(n)
                        .is_some_and(|t| t.repr == Some(bindings::Repr::Untagged))
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
            // `{:?}` of a slice iterator: what's left (`Iter([1, 2])`).
            Ty::Iter { mutable: false, elem } if debug => self.lacks_fmt(&self.shallow(elem), true),
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
pub(super) fn is_int_literal(e: &ast::Expr) -> bool {
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

/// `Ordering::Less` → -1, `Equal` → 0, `Greater` → 1. A user `enum Ordering` shadows std's
/// unless the path says `cmp::Ordering`.
pub fn ordering_variant(
    path: Option<ast::Path>,
    defs: &HashMap<String, crate::adt::AdtDef>,
) -> Option<i8> {
    let text = path?.syntax().text().to_string();
    let last3: Vec<&str> = text.rsplit("::").take(3).collect();
    if defs.contains_key("Ordering") && last3.get(2).is_none_or(|s| s.trim() != "cmp") {
        return None;
    }
    let last2 = &last3[..last3.len().min(2)];
    match last2 {
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
    /// A bindings enum (a TS union), by name.
    Extern(String),
    Slice(Ty),
    /// `[T; N]` with `N` known.
    Array(Ty, usize),
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

/// A function's parameters as a value's closure type: its `&mut i32`-style parameters take cells,
/// so they're `MutRef`s, and a call through the value (`let g = inc; g(&mut n)`) passes one.
fn fn_value_params(sig: &FnSig) -> Vec<Ty> {
    sig.params
        .iter()
        .enumerate()
        .map(|(i, t)| {
            if sig.cell_params.get(i) == Some(&true) {
                Ty::MutRef(Box::new(t.clone()))
            } else {
                t.clone()
            }
        })
        .collect()
}

/// `Box::from(x)` / `<Box<dyn Shape>>::from(x)`: `Some` of the `dyn` trait the path names (`None`
/// for a plain `Box::from`), or `None` when the call is something else.
pub fn box_from_call(c: &ast::CallExpr) -> Option<Option<String>> {
    let ast::Expr::PathExpr(p) = c.expr()? else {
        return None;
    };
    let path = p.path()?;
    if path.segment()?.name_ref()?.text() != "from" {
        return None;
    }
    let q = path.qualifier()?;
    let seg = q.segment()?;
    // (`<Box<dyn Shape>>::from`: a type anchor, which the UFCS pre-pass may have made
    // `Box::<dyn Shape>::from`; `Box::from` / `std::boxed::Box::from`: a name.)
    let s = match seg.type_anchor().and_then(|a| a.ty()) {
        Some(ast::Type::PathType(t)) => t.path()?.segment()?,
        Some(_) => return None,
        None => seg,
    };
    if s.name_ref()?.text() != "Box" {
        return None;
    }
    let dyn_trait = s
        .generic_arg_list()
        .and_then(|l| l.generic_args().next())
        .and_then(|g| match g {
            ast::GenericArg::TypeArg(a) => match a.ty()? {
                ast::Type::DynTraitType(d) => match d.type_bound_list()?.bounds().next()?.ty()? {
                    ast::Type::PathType(p) => {
                        Some(p.path()?.segment()?.name_ref()?.text().to_string())
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        });
    Some(dyn_trait)
}

/// `Pin::new(&mut x)` / `Pin::new_unchecked(..)` / `Pin::into_inner(..)`: `x` (`Pin` is erased),
/// unless the file has a `Pin` of its own (`has_own`).
pub fn is_pin_new(c: &ast::CallExpr, has_own: bool) -> bool {
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
    !has_own
        && matches!(
            names
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            [.., "Pin", "new" | "new_unchecked" | "into_inner"]
        )
}

/// Whether `e` is a `&mut` iterator: `v.iter_mut()` / `.values_mut()` under adapters that keep
/// its items (`filter`, `rev`, `enumerate`, `zip`, ..), or anything else `typed` says is one (a
/// local holding one, a call returning `impl Iterator<Item = &mut T>`).
fn mut_iter_chain(e: &ast::Expr, typed: &dyn Fn(&ast::Expr) -> bool) -> bool {
    let mut r = strip_parens(e);
    loop {
        match &r {
            ast::Expr::MethodCallExpr(inner) => {
                let n = inner
                    .name_ref()
                    .map(|n| n.text().to_string())
                    .unwrap_or_default();
                if matches!(n.as_str(), "iter_mut" | "values_mut") {
                    return true;
                }
                // `a.iter().zip(b.iter_mut())`
                if matches!(n.as_str(), "zip" | "chain")
                    && inner
                        .arg_list()
                        .and_then(|l| l.args().next())
                        .is_some_and(|a| mut_iter_chain(&a, typed))
                {
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
                        | "enumerate"
                        | "peekable"
                        | "by_ref"
                        | "fuse"
                        | "inspect"
                        | "zip"
                        | "chain"
                ) {
                    return typed(&r);
                }
                match inner.receiver() {
                    Some(x) => r = strip_parens(&x),
                    None => return false,
                }
            }
            other => return typed(other),
        }
    }
}

/// An element taken out of a `&mut` iterator: `v.iter_mut().find(..)`, `.max_by_key(..)?`,
/// `.next().unwrap()`, `it.next()` of a local `it = v.iter_mut()` (a `&mut` to it, or an
/// `Option` of one).
fn mut_element(e: &ast::Expr, typed: &dyn Fn(&ast::Expr) -> bool) -> bool {
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
                return m.receiver().is_some_and(|r| mut_iter_chain(&r, typed));
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

/// A struct pattern naming a std range type with public fields (`Range { start, end }`,
/// `RangeFrom`, `RangeTo`, `RangeToInclusive`; bare or under `ops::` / `std::ops::`): its kind.
/// Only where the name isn't one of the file's own types.
pub fn range_struct_pat(segs: &[String]) -> Option<RangeKind> {
    let (last, prefix) = segs.split_last()?;
    let std_path = match prefix {
        [] => true,
        [o] => o == "ops",
        [s, o] => (s == "std" || s == "core") && o == "ops",
        _ => false,
    };
    match RangeKind::from_name(last)? {
        RangeKind::Inclusive | RangeKind::Full => None,
        k => std_path.then_some(k),
    }
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

/// `&mut T`, or a tuple holding one (`(usize, &mut u8)`).
fn holds_mut_ref(t: &ast::Type) -> bool {
    match t {
        ast::Type::RefType(r) => r.mut_token().is_some(),
        ast::Type::TupleType(tt) => tt.fields().any(|f| holds_mut_ref(&f)),
        _ => false,
    }
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
/// Whether `{:x?}` can print `t`: its integers, inside collections, options, results and tuples,
/// are formatted where the format string is (a struct's derived `Debug` prints its own fields).
pub(super) fn hex_debuggable(t: &Ty) -> bool {
    match t {
        Ty::Int(_)
        | Ty::IntLit
        | Ty::Float { .. }
        | Ty::Bool
        | Ty::Char
        | Ty::Str
        | Ty::Unit
        | Ty::Ordering => true,
        Ty::Vec(e) | Ty::Option(e) | Ty::Set(e) | Ty::Coll(_, e) | Ty::RefCell(e) => {
            hex_debuggable(e)
        }
        Ty::Map(k, v) | Ty::Result(k, v) => hex_debuggable(k) && hex_debuggable(v),
        Ty::Tuple(es) => es.iter().all(hex_debuggable),
        _ => false,
    }
}

pub(super) fn conversion_target(list: &ast::TypeBoundList) -> Option<ast::Type> {
    use ra_ap_syntax::ast::HasGenericArgs;
    list.bounds().find_map(|b| match b.ty()? {
        ast::Type::PathType(p) => {
            let seg = p.path()?.segment()?;
            let n = seg.name_ref()?.text().to_string();
            if !matches!(
                n.as_str(),
                "Into" | "AsRef" | "AsMut" | "Borrow" | "BorrowMut"
            ) {
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

/// The type parameter a projection path is on: `T` of `T::Item` and of `<T as Tr>::Item`.
pub(super) fn proj_qual(path: &ast::Path) -> Option<String> {
    let q = path.qualifier()?;
    if let Some(n) = q.as_single_name_ref() {
        return Some(n.text().to_string());
    }
    if q.qualifier().is_some() {
        return None;
    }
    match q.segment()?.type_anchor()?.ty()? {
        ast::Type::PathType(pt) => Some(pt.path()?.as_single_name_ref()?.text().to_string()),
        _ => None,
    }
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

/// A place: a local, a field of one (`self.timer`, `a.b.c`) or an element (`v[i]`, `grid[r][c]`).
pub(super) fn is_place_expr(e: &ast::Expr) -> bool {
    match e {
        // (`*o` of a `&mut` loop variable: the element it aliases.)
        ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Deref) => {
            p.expr().is_some_and(|i| is_place_expr(&i))
        }
        ast::Expr::PathExpr(p) => p.path().is_some_and(|p| p.as_single_name_ref().is_some()),
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|i| is_place_expr(&i)),
        ast::Expr::FieldExpr(f) => f.expr().is_some_and(|i| is_place_expr(&i)),
        ast::Expr::IndexExpr(ix) => ix.base().is_some_and(|b| is_place_expr(&b)),
        _ => false,
    }
}

/// `r` in `let r = v.get_mut(..).unwrap();`.
fn slot_let_binding(m: &ast::MethodCallExpr) -> Option<ast::Name> {
    let u = ast::MethodCallExpr::cast(m.syntax().parent()?)?;
    if !u
        .name_ref()
        .is_some_and(|n| matches!(n.text(), "unwrap" | "expect" | "unwrap_or" | "unwrap_or_else"))
    {
        return None;
    }
    let l = ast::LetStmt::cast(u.syntax().parent()?)?;
    match l.pat()? {
        ast::Pat::IdentPat(ip) => ip.name(),
        _ => None,
    }
}

/// Whether a call's result is matched (`if let` / `while let` / `match`).
pub(super) fn matched_position(m: &ast::MethodCallExpr) -> bool {
    let mut n = m.syntax().clone();
    while let Some(p) = n
        .parent()
        .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
    {
        n = p;
    }
    let Some(p) = n.parent() else { return false };
    ast::LetExpr::can_cast(p.kind())
        || ast::MatchExpr::cast(p).is_some_and(|me| me.expr().is_some_and(|e| e.syntax() == &n))
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
    // `m.get_mut(k).map(|v| *v += 1)`: the closure's parameter binds the slot.
    if let Some(u) = ast::MethodCallExpr::cast(p.clone())
        && u.receiver().is_some_and(|r| r.syntax() == &n)
        && u.name_ref().is_some_and(|x| {
            matches!(
                x.text(),
                "map" | "and_then" | "is_some_and" | "is_none_or" | "inspect" | "map_or"
                    | "map_or_else"
            )
        })
    {
        return true;
    }
    unwrapped_position(m) || slots::keep_position(m.syntax()) || ok_or_slot(m).is_some()
        || ok_or_kept(m)
}

/// `let r = m.get_mut(k).ok_or(e);`: the `Result` kept in a local, which holds the slot.
fn ok_or_kept(m: &ast::MethodCallExpr) -> bool {
    let up = |n: SyntaxNode| {
        let mut n = n;
        while let Some(p) = n
            .parent()
            .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
        {
            n = p;
        }
        n
    };
    let n = up(m.syntax().clone());
    let Some(o) = n.parent().and_then(ast::MethodCallExpr::cast) else {
        return false;
    };
    if !o.receiver().is_some_and(|r| r.syntax() == &n)
        || !o
            .name_ref()
            .is_some_and(|x| matches!(x.text(), "ok_or" | "ok_or_else"))
    {
        return false;
    }
    let n = up(o.syntax().clone());
    n.parent()
        .and_then(ast::LetStmt::cast)
        .is_some_and(|l| {
            l.initializer().is_some_and(|i| i.syntax() == &n)
                && matches!(l.pat(), Some(ast::Pat::IdentPat(_)))
        })
}

/// `m.get_mut(k).ok_or(e)?` / `.ok_or_else(..).unwrap()`: the expression that yields the
/// `&mut`, where a slot stands in for it.
pub(super) fn ok_or_slot(m: &ast::MethodCallExpr) -> Option<ast::Expr> {
    let up = |n: SyntaxNode| {
        let mut n = n;
        while let Some(p) = n
            .parent()
            .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
        {
            n = p;
        }
        n
    };
    let n = up(m.syntax().clone());
    let o = ast::MethodCallExpr::cast(n.parent()?)?;
    if !o.receiver().is_some_and(|r| r.syntax() == &n)
        || !o
            .name_ref()
            .is_some_and(|x| matches!(x.text(), "ok_or" | "ok_or_else"))
    {
        return None;
    }
    let n = up(o.syntax().clone());
    let p = n.parent()?;
    if let Some(t) = ast::TryExpr::cast(p.clone()) {
        return Some(ast::Expr::TryExpr(t));
    }
    let u = ast::MethodCallExpr::cast(p)?;
    (u.receiver().is_some_and(|r| r.syntax() == &n)
        && u.name_ref().is_some_and(|x| matches!(x.text(), "unwrap" | "expect")))
    .then_some(ast::Expr::MethodCallExpr(u))
}

/// Whether a call's result is unwrapped (`.unwrap()` / `.expect(..)`) right into a deref
/// (`*v.get_mut(i).unwrap() += 1`) or a `let` (`let r = v.get_mut(i).unwrap();`).
pub(super) fn unwrapped_position(m: &ast::MethodCallExpr) -> bool {
    let mut n = m.syntax().clone();
    while let Some(p) = n
        .parent()
        .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
    {
        n = p;
    }
    let Some(p) = n.parent() else { return false };
    if let Some(u) = ast::MethodCallExpr::cast(p)
        && u.receiver().is_some_and(|r| r.syntax() == &n)
        && u.name_ref()
            .is_some_and(|x| matches!(x.text(), "unwrap" | "expect" | "unwrap_or" | "unwrap_or_else"))
        && let Some(q) = u.syntax().parent()
    {
        // (`m.get_mut(k).unwrap().push(..)`: a method on the value, which may change it.)
        return ast::PrefixExpr::cast(q.clone())
            .is_some_and(|pe| pe.op_kind() == Some(ast::UnaryOp::Deref))
            || ast::MethodCallExpr::cast(q)
                .is_some_and(|mc| mc.receiver().is_some_and(|r| r.syntax() == u.syntax()))
            || slots::keep_position(u.syntax());
    }
    false
}

/// `v.get_mut(i).is_some()` and the like: the result is only looked at, so a copy is fine.
fn only_tested(m: &ast::MethodCallExpr) -> bool {
    let mut n = m.syntax().clone();
    while let Some(p) = n
        .parent()
        .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
    {
        n = p;
    }
    // A macro argument has no parent: `println!("{:?}", m.get_mut(k))` reads a copy.
    let Some(p) = n.parent() else { return true };
    // `m.get_mut(k) == None`
    if let Some(b) = ast::BinExpr::cast(p.clone()) {
        return matches!(
            b.op_kind(),
            Some(ast::BinaryOp::CmpOp(ast::CmpOp::Eq { .. }))
        ) && [b.lhs(), b.rhs()]
            .into_iter()
            .flatten()
            .any(|s| s.syntax().text() == "None");
    }
    ast::MethodCallExpr::cast(p)
        .filter(|u| u.receiver().is_some_and(|r| r.syntax() == &n))
        .and_then(|u| u.name_ref())
        .is_some_and(|n| matches!(n.text(), "is_some" | "is_none" | "copied" | "cloned"))
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
            // (Through the arms of a `match` / `if` and the tails of blocks; a `panic!` hands
            // out nothing.)
            let mut i = 0;
            while i < results.len() {
                let e = strip_parens(&results[i]);
                let inner: Option<Vec<ast::Expr>> = match &e {
                    ast::Expr::MatchExpr(m) => Some(
                        m.match_arm_list()
                            .into_iter()
                            .flat_map(|l| l.arms())
                            .filter_map(|a| a.expr())
                            .collect(),
                    ),
                    ast::Expr::IfExpr(ie) => Some(
                        [ie.then_branch().map(ast::Expr::BlockExpr), ie.else_branch().map(|b| match b {
                            ast::ElseBranch::Block(b) => ast::Expr::BlockExpr(b),
                            ast::ElseBranch::IfExpr(x) => ast::Expr::IfExpr(x),
                        })]
                        .into_iter()
                        .flatten()
                        .collect(),
                    ),
                    ast::Expr::BlockExpr(b) => {
                        Some(b.stmt_list().and_then(|l| l.tail_expr()).into_iter().collect())
                    }
                    ast::Expr::MacroExpr(m)
                        if m.macro_call()
                            .and_then(|m| macros::macro_name(&m))
                            .is_some_and(|m| {
                                matches!(m.as_str(), "panic" | "unreachable" | "todo" | "unimplemented")
                            }) =>
                    {
                        Some(Vec::new())
                    }
                    // (Its value is among the `return`s already.)
                    ast::Expr::ReturnExpr(_) => Some(Vec::new()),
                    _ => None,
                };
                match inner {
                    Some(es) => {
                        results.remove(i);
                        results.extend(es);
                    }
                    None => i += 1,
                }
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
        expected: Option<&Ty>,
    ) -> Option<Ty> {
        let path = callee.path()?;
        let method = path.segment()?.name_ref()?.text().to_string();
        let q = path.qualifier()?;
        if q.qualifier().is_some() {
            return None;
        }
        let seg = q.segment()?;
        // `<T as Trait>` or `Trait`, and the trait's segment where its arguments are written.
        let (trait_name, anchor_ty, trait_seg) = match seg.type_anchor() {
            // (Both are `PATH_TYPE`s: the type, then the trait.)
            Some(a) if a.as_token().is_some() => {
                let mut tys = a.syntax().children().filter_map(ast::Type::cast);
                let ty = tys.next();
                let Some(ast::Type::PathType(tp)) = tys.next() else {
                    return None;
                };
                let ts = tp.path()?.segment()?;
                let tr = ts.name_ref()?.text().to_string();
                (tr, ty, Some(ts))
            }
            Some(_) => return None,
            None => {
                let n = seg.name_ref()?.text().to_string();
                // `T::len(x)` / `T::drive(self, v)`, with `T` a type parameter: its bound's
                // method, as `Trait::len(x)`. (Functions without `self` are `param_static_call`'s.)
                let bound = self
                    .param_bounds
                    .contains_key(&n)
                    .then(|| {
                        self.bound_traits(&n).into_iter().find(|b| {
                            self.adts.traits.get(b).is_some_and(|d| {
                                d.methods
                                    .iter()
                                    .any(|m| m.name == method && m.sig.receiver.is_some())
                            })
                        })
                    })
                    .flatten();
                match bound {
                    Some(b) => (b, None, None),
                    None => (n, None, Some(seg.clone())),
                }
            }
        };
        // `<T as Bounded>::min_value()`, or `Bounded::min_value()` where a `T` is expected, with
        // `T: Bounded`: as `T::min_value()` (by `T`'s class).
        let param = match &anchor_ty {
            Some(ast::Type::PathType(p)) => p.path().and_then(|p| match p.as_single_name_ref() {
                Some(n) => Some(n.text().to_string()),
                // (`<Self::Error as Error>::custom(..)`.)
                None => p
                    .qualifier()
                    .is_some_and(|q| q.syntax().text() == "Self")
                    .then(|| Some(format!("Self::{}", p.segment()?.name_ref()?.text())))
                    .flatten(),
            }),
            Some(_) => None,
            // (`Self` where the function's return type has it: `fn from(n: i64) -> Option<Self>`.)
            None => {
                let ret = self.adts.traits.get(&trait_name).and_then(|d| {
                    let m = d.methods.iter().find(|m| m.name == method)?;
                    Some(m.sig.ret.clone())
                });
                match (ret, expected) {
                    (Some(ret), Some(e)) => match self.self_in(&ret, e) {
                        Some(Ty::Param(p)) => Some(p),
                        _ => None,
                    },
                    _ => None,
                }
            }
        };
        // (A default method's signature names `Self::Error` by its trait, `Visitor::Error`.)
        let param = param.map(|p| match p.split_once("::") {
            Some((tr, a))
                if self.param_bounds.get("Self").is_some_and(|b| b.iter().any(|b| b == tr))
                    && self.self_projection(a) =>
            {
                format!("Self::{a}")
            }
            _ => p,
        });
        if let Some(p) = param.filter(|p| {
            p != "Self"
                && (self.param_bounds.contains_key(p)
                    || p.strip_prefix("Self::").is_some_and(|a| self.self_projection(a)))
        })
            && (self.bound_traits(&p).contains(&trait_name)
                || self.cur_blanket_trait(&p).is_some_and(|t| t == trait_name))
            && self.adts.traits.get(&trait_name).is_some_and(|d| {
                d.methods
                    .iter()
                    .any(|m| m.name == method && m.sig.receiver.is_none())
            })
            && let Some(t) = self.param_static_call(c, &p, &method, args)
        {
            let r = self.range(c.syntax());
            self.param_paths.insert(r, p);
            return Some(t);
        }
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
        let mut sub: HashMap<String, Ty> = [("Self".to_string(), self_ty.clone())].into();
        let trait_args = self.qualified_trait_args(c, &trait_name, &def, trait_seg.as_ref(), &self_ty);
        for (tp, a) in def.type_params.iter().zip(&trait_args) {
            sub.insert(format!("{trait_name}::{tp}"), a.clone());
        }
        // (The method's own type parameters, `fn custom<T: Display>(msg: T)`: fresh.)
        for p in &tm.sig.type_params {
            sub.insert(p.clone(), self.fresh(VarKind::General));
        }
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
        for (i, (a, p)) in rest.iter().zip(sig.params.iter()).enumerate() {
            // (A `&mut i32` / `&mut V` parameter takes a cell, as in a method call.)
            if sig.cell_params.get(i) == Some(&true) {
                self.cell_arg(a);
            }
            self.check(a, p);
        }
        if !receiver && anchor_ty.is_some() {
            self.note_ext_static_dicts(c, &self_ty, &method);
        }
        if !receiver
            && let Some(t) = &anchor_ty
        {
            self.note_wrapper_static(c, t, &trait_name, &method, &self_ty);
        }
        let local_trait = def.module.is_none();
        let r = self.range(c.syntax());
        // (Several impls of a generic trait for one type, `Conv<u8>` and `Conv<u16>`: the trait
        // arguments' classes pick one, as in a method call.)
        if !trait_args.is_empty() && !self.trait_arg_names(&trait_name, &method).is_empty() {
            let caller = self.cur_fn.clone().unwrap_or_default();
            self.generic_calls
                .push((r, caller, format!("{trait_name}::{method}"), trait_args.clone()));
        }
        // `T::money(&x)` on a `T` whose impls JS can't tell apart: by `T`'s class, as `x.money()`.
        if receiver
            && let Ty::Param(p) = self.shallow(&self_ty)
            && self.param_bounds.contains_key(&p)
            && self.by_class_dispatch(&p, &trait_name)
        {
            self.class_dispatch.insert(r);
        }
        // (`<String as Tr>::m(..)` by the written type, `Tr::m(&s)` by its argument: decided now,
        // while the locals are in scope.)
        if self.twin_name(&method) {
            let side = match (&anchor_ty, receiver, args.first()) {
                (Some(t), _, _) => twins::written_owned(t),
                (None, true, Some(a)) => self.twin_arg_side(a),
                _ => None,
            };
            self.twin_sides.insert(r, (method.clone(), side));
        }
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
    /// What `Self` is in `ret` (a trait function's return type) for a call expected to make
    /// `expected`: `Self` against `T`, `Option<Self>` against `Option<T>`, and so on.
    fn self_in(&self, ret: &Ty, expected: &Ty) -> Option<Ty> {
        let e = self.shallow(expected);
        match (ret, &e) {
            (Ty::Param(s), _) if s == "Self" => Some(e),
            (Ty::Option(r), Ty::Option(x)) | (Ty::Vec(r), Ty::Vec(x)) => self.self_in(r, x),
            (Ty::Result(r, re), Ty::Result(x, xe)) => {
                self.self_in(r, x).or_else(|| self.self_in(re, xe))
            }
            (Ty::Tuple(rs), Ty::Tuple(xs)) if rs.len() == xs.len() => {
                rs.iter().zip(xs).find_map(|(r, x)| self.self_in(r, x))
            }
            _ => None,
        }
    }

    /// `T`'s bounds and their supertraits, of this file.
    pub(super) fn bound_traits(&self, param: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut todo: Vec<String> = match (self.param_bounds.get(param), param.rsplit_once("::")) {
            (Some(b), _) => b.clone(),
            // `Vector::Mask`, a trait's own template for `Self::Mask` (a default method's
            // `self.movemask()`): what `Vector` or a supertrait declares.
            (None, Some((p, a))) if !self.param_bounds.contains_key(p) && self.adts.traits.contains_key(p) => {
                let mut seen: Vec<String> = Vec::new();
                let mut tr_todo = vec![p.to_string()];
                let mut found = Vec::new();
                while let Some(t) = tr_todo.pop() {
                    if seen.contains(&t) {
                        continue;
                    }
                    seen.push(t.clone());
                    if let Some(d) = self.adts.traits.get(&t) {
                        found.extend(d.assoc_bounds.get(a).cloned().unwrap_or_default());
                        tr_todo.extend(d.supers.iter().cloned());
                    }
                }
                found
            }
            // `T::P` with `T: Fac` and `trait Fac { type P: Parser; }`: what `Fac` declares.
            (None, Some((p, a))) => self
                .bound_traits(p)
                .iter()
                .filter_map(|t| self.adts.traits.get(t)?.assoc_bounds.get(a).cloned())
                .flatten()
                .collect(),
            (None, None) => Vec::new(),
        };
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
            Some((ty, m)) => self.own_dict_param(ty, m, param),
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

    /// In a method with `self` of `impl<T> S<T>` (a struct or an enum): `S`'s values carry
    /// `T`'s class (`this.$T`; in `impl<T> S<Vec<T>>`, on the `Vec`'s, `this.$T.elem`). False
    /// where they can't (a trait's default method).
    fn store_class(&mut self, param: &str) -> bool {
        // `impl<T> Tr for Option<T>`: a caller that knows the `Option`'s type hands its methods
        // `T`'s class.
        if let Some(f) = self.cur_fn.clone()
            && let Some(("Option", m)) = f.split_once("::")
            && self.adts.method("Option", m).is_some_and(|s| {
                s.receiver.is_some() && s.impl_args == [Ty::Param(param.to_string())]
            })
        {
            self.elem_methods.insert(f);
            return true;
        }
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

    /// In a method with `self` of `impl<T> Tr for Vec<T>` or `impl<A, B> Tr for (A, B)`: a caller
    /// that knows the value's type hands it `param`'s class, as for an `Option` (see
    /// `store_class`). Only for calls dispatched by class: anyone else leaves none.
    pub(super) fn elem_class(&mut self, param: &str) -> bool {
        let Some(f) = self.cur_fn.clone() else {
            return false;
        };
        let Some((ty, m)) = f.split_once("::") else {
            return false;
        };
        if ty != "Vec" && crate::adt::tuple_arity(ty).is_none() && !crate::adt::is_wrapper_key(ty) {
            return false;
        }
        let bare = self.adts.method(ty, m).is_some_and(|s| {
            s.receiver.is_some() && s.impl_args.iter().any(|a| matches!(a, Ty::Param(p) if p == param))
        });
        if bare {
            self.elem_methods.insert(f);
        }
        bare
    }

    /// (struct, position) of `param` when `f` is a method with `self` of `impl<.., param, ..>
    /// S<.., param, ..>` for a struct `S`.
    fn stored_pos(&self, f: &str, param: &str) -> Option<(String, usize)> {
        let (ty, m) = f.split_once("::")?;
        let def = self.adts.defs.get(ty)?;
        let sig = self.adts.method(ty, m)?;
        sig.receiver.as_ref()?;
        let i = sig
            .impl_args
            .iter()
            .position(|a| class_param(a) == Some(param))?;
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
        // (`Self::Error` in a trait's default method: the implementing class's getter.)
        if let Some(a) = param.strip_prefix("Self::") {
            self.self_assoc_dicts.insert(a.to_string());
            return;
        }
        // (`T::E` of a `T: Has`: `T`'s class, and its impls' getter for `E`.)
        if let Some((p, a)) = param.split_once("::") {
            self.self_assoc_dicts.insert(a.to_string());
            return self.note_dict(p, node);
        }
        // (In a method with `self` of `impl<T> Tr for Vec<T>` or a tuple's: handed over by a
        // caller that knows the value's type, `κσ.$callElem`.)
        // (In an associated const of `impl<T> Tr for S<T>`: read off the class view it's read
        // through, which carries `T`'s class, `this.$targs`.)
        if !self.try_note_dict(param) && !self.elem_class(param) && !impl_const_param(node, param)
        {
            self.error(
                node,
                format!("`{param}::..` on a type parameter of an `impl` isn't supported yet in methods with `self` of a trait or of `impl<T> S<Vec<T>>` (it works in a struct's or enum's `impl<T> S<T>`, on a function's or method's own type parameters, and in associated functions)"),
            );
        }
    }

    /// Note that the function being checked needs `param`'s class, when it can have one: a free
    /// function's type parameter, a method's own (`fn parse<T: Parse>(&self)`), or an `impl<T>
    /// S<T>`'s in a method with `self`, which the struct's or enum's values carry. False where
    /// it can't (a trait's default method, an impl for a tuple or `Vec<T>`).
    pub(super) fn try_note_dict(&mut self, param: &str) -> bool {
        let own = self.cur_fn.clone().filter(|f| match f.split_once("::") {
            Some((ty, m)) => self.own_dict_param(ty, m, param),
            None => true,
        });
        match own {
            Some(f) => {
                self.dict_uses.insert((f, param.to_string()));
                true
            }
            None => self.store_class(param),
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
        let mut bounds = self.bound_traits(param);
        // (In a blanket impl's associated function, `T` implements its trait too.)
        bounds.extend(self.cur_blanket_trait(param));
        // (And the traits blanket impls give it through its bounds.)
        for tr in self.param_blanket_traits(param) {
            if !bounds.contains(&tr) {
                bounds.push(tr);
            }
        }
        for tr in bounds.clone() {
            self.twin_generic_call(c.syntax(), &tr, f, &pty);
        }
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
        // `T::try_from(x)` with `T: TryFrom<X>`: `Result<T, T::Error>`, the bound picked by `x`.
        if f == "try_from" && self.param_has(param, "TryFrom") {
            self.arg_count(c.syntax(), "function", 1, args.len());
            let srcs = self.try_from_args.get(param).cloned().unwrap_or_default();
            let x = match args.first() {
                Some(a) if srcs.len() > 1 => {
                    let t = self.infer(a, None);
                    let x = srcs
                        .iter()
                        .find(|s| self.could_be(s, &t))
                        .unwrap_or(&srcs[0])
                        .clone();
                    self.expect(a.syntax(), &t, &x);
                    x
                }
                Some(a) => {
                    let x = srcs.first().cloned().unwrap_or(Ty::Unknown);
                    self.check(a, &x);
                    x
                }
                None => Ty::Unknown,
            };
            let r = self.range(c.syntax());
            let name = self.zonk(&x).describe();
            self.param_froms.insert(r, name);
            self.note_dict(param, c.syntax());
            return Some(Ty::Result(
                Box::new(pty),
                Box::new(Ty::Param(format!("{param}::Error"))),
            ));
        }
        // `C::from_iter(xs)` with `C: FromIterator<T>`: by `C`'s class.
        if f == "from_iter" && self.param_has(param, "FromIterator") {
            self.arg_count(c.syntax(), "function", 1, args.len());
            for a in args {
                self.infer(a, None);
            }
            self.note_dict(param, c.syntax());
            return Some(pty);
        }
        if f == "default" && bounds.iter().any(|b| b == "Default") {
            self.arg_count(c.syntax(), "function", 0, args.len());
            self.note_dict(param, c.syntax());
            return Some(pty);
        }
        let (tr, sig, own_bounds, arg_bounds) = bounds.iter().find_map(|b| {
            let d = self.adts.traits.get(b)?;
            let tm = d
                .methods
                .iter()
                .find(|m| m.name == f && m.sig.receiver.is_none())?;
            Some((
                b.clone(),
                tm.sig.clone(),
                tm.own_bounds.clone(),
                tm.arg_bounds.clone(),
            ))
        })?;
        // (`Fac::P` in `fn make() -> Self::P` is `T::P`, or the bound's `P = X`.)
        let mut sub = self.bound_subst(param);
        sub.insert("Self".to_string(), pty);
        // The method's own type parameters (`fn select<R: Copy>(r: R) -> R`): the turbofish's
        // (`O::select::<u8>(x)`), or fresh.
        let turbofish: Vec<(Ty, SyntaxNode)> = match c.expr() {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.generic_arg_list())
                .map(|l| l.generic_args().collect::<Vec<_>>())
                .unwrap_or_default()
                .iter()
                .filter_map(|g| Some((self.generic_arg_ty(g)?, g.syntax().clone())))
                .collect(),
            _ => Vec::new(),
        };
        // (Turbofish arguments go by the written order: a parameter lowered to its bound's target
        // (`S: Into<String>`, `F: Fn(u8)`) isn't among the signature's.)
        let written: Vec<&String> = own_bounds.iter().map(|(n, _)| n).collect();
        for (n, b) in &arg_bounds {
            if let Some((t, at)) = written
                .iter()
                .position(|w| *w == n)
                .and_then(|i| turbofish.get(i))
            {
                self.pending_arg_bounds
                    .push((t.clone(), b.clone(), at.clone(), self.base));
            }
        }
        for (i, p) in sig.type_params.iter().enumerate() {
            let i = written.iter().position(|w| *w == p).unwrap_or(i);
            let (t, at) = match turbofish.get(i) {
                Some((t, at)) => (t.clone(), at.clone()),
                None => (self.fresh(VarKind::General), c.syntax().clone()),
            };
            // Its bounds, checked once the type is known (`O::select::<NoDef>` with `R: Default`).
            for b in own_bounds.iter().filter(|(n, _)| n == p).flat_map(|(_, bs)| bs) {
                if CHECKED_OWN_BOUNDS.contains(&b.as_str()) || self.adts.traits.contains_key(b) {
                    self.pending_bounds
                        .push((t.clone(), b.clone(), at.clone(), self.base));
                }
            }
            sub.insert(p.clone(), t);
        }
        // (Its own parameters' classes, which an impl's body may need (`R::default()`), go with
        // the call as `Tr::f`'s: see `finish_needs`.)
        if !sig.type_params.is_empty()
            && let Some(caller) = self.cur_fn.clone()
        {
            let r = self.range(c.syntax());
            let targs = sig.type_params.iter().map(|p| sub[p].clone()).collect();
            self.generic_calls
                .push((r, caller, format!("{tr}::{f}"), targs));
        }
        let sig = traits::subst_method(&sig, &sub);
        self.arg_count(c.syntax(), "function", sig.params.len(), args.len());
        for (a, p) in args.iter().zip(sig.params.iter()) {
            self.check(a, p);
        }
        self.note_dict(param, c.syntax());
        Some(sig.ret)
    }

    /// Whether `Self::{assoc}` is a bounded associated type of the trait whose default method is
    /// being checked (`type Error: Error;`).
    fn self_projection(&self, assoc: &str) -> bool {
        self.self_ty == Some(Ty::Param("Self".into()))
            && self.param_bounds.contains_key("Self")
            && !self.bound_traits(&format!("Self::{assoc}")).is_empty()
    }

    /// `T::SIDES`: an associated const declared by a bound of `T`.
    fn param_const(&mut self, param: &str, konst: &str, node: &SyntaxNode) -> Option<Ty> {
        if !self.param_bounds.contains_key(param) {
            return None;
        }
        let (tr, t) = self.bound_traits(param).into_iter().find_map(|b| {
            let t = self.adts.traits.get(&b)?.const_tys.get(konst).cloned()?;
            Some((b, t))
        })?;
        // (`const ZERO: Self;` is `T` here.)
        let t = generic::subst(&t, &[("Self".to_string(), Ty::Param(param.to_string()))].into());
        // (`let mut r = Self::ROWS;`: JS would share the item's array.)
        if self.copy_array_assoc.contains(&(tr, konst.to_string())) {
            let r = self.range(node);
            self.array_reads.insert(r);
        }
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
            Some((ty, m)) => self.own_dict_param(ty, m, param),
            None => true,
        });
        if let Some(f) = own {
            self.dbg_uses.insert((f, param.to_string()));
        } else {
            // An `impl<T>`'s parameter in a method with `self`: the value carries `T`'s class,
            // whose formatter prints it (`κσ.$dbgOf(this.$T)`).
            self.store_class(param);
        }
    }

    /// A `#[derive(Default)]` struct with a bare `T` field (`last: T`): `default()` needs `T`'s
    /// class (`T::default()`), so its values carry it (`stored`) and `S::<f64>::default()` /
    /// `Default::default()` pass it.
    fn store_default_params(&mut self) {
        let mut uses = Vec::new();
        for def in self.adts.defs.values() {
            if def.is_enum || !def.derives("Default") {
                continue;
            }
            for field in def.variants.iter().flat_map(|v| v.fields.iter()) {
                if let Ty::Param(p) = &field.ty
                    && let Some(i) = def.type_params.iter().position(|x| x == p)
                {
                    uses.push((def.name.clone(), i));
                }
            }
        }
        self.stored_uses.extend(uses);
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
    /// Whether `param` is one `ty::m` takes a class for: its own type parameter, or its impl's in
    /// an associated function. (`ty` may be a trait whose default method `m` is.)
    fn own_dict_param(&self, ty: &str, m: &str, param: &str) -> bool {
        match self.adts.method(ty, m) {
            Some(s) => s.dict_names().iter().any(|p| p == param),
            None => self.adts.traits.get(ty).is_some_and(|d| {
                d.methods
                    .iter()
                    .any(|t| t.name == m && t.sig.type_params.iter().any(|p| p == param))
            }),
        }
    }

    /// A generic trait's method with `self` some impl of which takes a class for a parameter
    /// only the trait's arguments name (`impl<E: Default> Tr<E> for (Ix, Ix)`): a call through
    /// a bound hands the trait's arguments' classes too, after the method's own, by position.
    pub(super) fn trait_arg_names(&self, tr: &str, m: &str) -> Vec<String> {
        let Some(def) = self.adts.traits.get(tr) else {
            return Vec::new();
        };
        let takes = def.methods.iter().any(|t| t.name == m && t.sig.receiver.is_some())
            && self.adts.trait_impls.iter().any(|(ty, trs)| {
                trs.iter().any(|t| t == tr)
                    && self.adts.method(ty, m).is_some_and(|s| !s.trait_params.is_empty())
                    // (Or several impls of it for one type, told apart by those classes.)
                    || trs.iter().any(|t| t == tr) && self.adts.alt_trait_args(ty, m).is_some()
            });
        if !takes {
            return Vec::new();
        }
        def.type_params.iter().map(|tp| format!("{tr}::{tp}")).collect()
    }

    fn need_names(&self, f: &str) -> Vec<String> {
        match f.split_once("::") {
            // A method: `Type::name`.
            Some((ty, m)) => self
                .adts
                .method(ty, m)
                .map(|s| s.dict_names())
                // (Or a trait's, called through a type parameter: `T::select::<u8>(x)`.)
                .or_else(|| {
                    let tm = self
                        .adts
                        .traits
                        .get(ty)?
                        .methods
                        .iter()
                        .find(|t| t.name == m)?;
                    let mut names = tm.sig.dict_names();
                    names.extend(self.trait_arg_names(ty, m));
                    Some(names)
                })
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
        for (n, derivable) in self.const_param_needs(f, extra) {
            self.note_const_need(key, &n, derivable);
        }
    }

    /// `key` (a function, or `Type::name`) takes the const value `n` (also for another file's,
    /// which that file's checker found); `derivable` when an argument holds it.
    pub(super) fn note_const_need(&mut self, key: &str, n: &str, derivable: bool) {
        let k = (key.to_string(), n.to_string());
        self.dict_uses.insert(k.clone());
        self.const_needs.insert(k.clone());
        if derivable {
            self.const_derivable.insert(k);
        }
    }

    /// A function's const parameters, with `extra` (its `impl`'s), and whether an argument
    /// holds each (`a: [u8; N]`).
    pub(super) fn const_param_needs(&self, f: &ast::Fn, extra: Vec<String>) -> Vec<(String, bool)> {
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
        names
            .into_iter()
            .map(|n| {
                let derivable = f
                    .param_list()
                    .into_iter()
                    .flat_map(|pl| pl.params())
                    .any(|p| {
                        p.ty()
                            .is_some_and(|t| crate::adt::const_path(&t, &n, &self.adts).is_some())
                    });
                (n, derivable)
            })
            .collect()
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
                    help: None,
                    spans: Vec::new(),
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
    /// item (`N`, `{ N }`), or a constant expression `consteval` works out (`{ 2 * W + 1 }`).
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
            // `{ 2 * D_CODES + 1 }`, `{ CpuFeatures::NONE }`, `{ size_of::<u32>() }`, `{ elts(BITS) }`
            _ if self.eval_const(e).is_some() => Ty::ConstVal(self.eval_const(e).unwrap().text()),
            _ => {
                self.error(
                    e.syntax(),
                    "this const argument isn't supported yet (constant integer and `bool` expressions are: literals, `const` items, associated consts, arithmetic, `as` casts, and calls of this file's `const fn`s, also as methods of a `const` struct value)",
                );
                Ty::Unknown
            }
        }
    }

    /// The value of the constant the path `segs` names (at `node`, for its file and `Self`).
    pub(super) fn eval_const_segs(
        &self,
        node: &SyntaxNode,
        segs: &[String],
    ) -> Option<crate::consteval::CVal> {
        let known = crate::consteval::Known {
            consts: Some(&self.const_vals),
            assoc: Some(&self.adts.assoc_const_vals),
        };
        let self_ty = match &self.self_ty {
            Some(Ty::Adt(n, _)) => Some(n.as_str()),
            _ => None,
        };
        let own = node.ancestors().last().and_then(ast::SourceFile::cast);
        own.as_ref()
            .and_then(|f| crate::consteval::eval_segs(f, &known, segs, self_ty))
            .or_else(|| {
                let root = self.root_file.as_ref()?;
                if own.is_some_and(|f| f.syntax() == root.syntax()) {
                    return None;
                }
                crate::consteval::eval_segs(root, &known, segs, self_ty)
            })
    }

    /// The value of a constant expression, with what other files' consts are known to be.
    pub(super) fn eval_const(&self, e: &ast::Expr) -> Option<crate::consteval::CVal> {
        let known = crate::consteval::Known {
            consts: Some(&self.const_vals),
            assoc: Some(&self.adts.assoc_const_vals),
        };
        let own = e
            .syntax()
            .ancestors()
            .last()
            .and_then(ast::SourceFile::cast);
        own.as_ref()
            .and_then(|f| crate::consteval::eval(f, &known, e))
            .or_else(|| {
                let root = self.root_file.as_ref()?;
                if own.is_some_and(|f| f.syntax() == root.syntax()) {
                    return None;
                }
                crate::consteval::eval(root, &known, e)
            })
    }

    /// A const parameter's type: an integer (a 64- or 128-bit one is a BigInt in the body),
    /// `bool` or `char`.
    pub(super) fn const_param_ty(&mut self, cp: &ast::ConstParam, report: bool) -> Ty {
        let ty = cp.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
        let ok = matches!(ty, Ty::Bool | Ty::Char | Ty::Unknown | Ty::Int(_));
        if !ok && report {
            self.error(
                cp.syntax(),
                format!(
                    "const parameters of type `{}` aren't supported (integers, `bool` and `char` are)",
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
            // A trait's function called through a type parameter takes the classes any impl's
            // takes.
            for (_, _, callee, _) in &self.generic_calls {
                let Some((tr, m)) = callee.split_once("::") else {
                    continue;
                };
                if !self.adts.traits.contains_key(tr) || self.adts.method(tr, m).is_some() {
                    continue;
                }
                for n in param_names(self, callee) {
                    let any = self.adts.trait_impls.iter().any(|(ty, trs)| {
                        trs.iter().any(|t| t == tr)
                            && needs.contains(&(format!("{ty}::{m}"), n.clone()))
                    });
                    if any {
                        uses.push((callee.clone(), n));
                    }
                }
            }
            // A trait's method with type parameters of its own (`fn visit<E: Kind>`), its default
            // and each type's (own or inherited) take the same classes, by position: a call
            // through a bound or `dyn` (`Tr::m`) can't tell which one it reaches.
            for (tr, def) in &self.adts.traits {
                for tm in def.methods.iter().filter(|tm| !tm.sig.type_params.is_empty()) {
                    let mut all: Vec<(String, Vec<String>)> = self
                        .adts
                        .trait_impls
                        .iter()
                        .filter(|(_, trs)| trs.iter().any(|t| t == tr))
                        .filter_map(|(ty, _)| {
                            let s = self.adts.method(ty, &tm.name)?;
                            Some((format!("{ty}::{}", tm.name), s.type_params.clone()))
                        })
                        .collect();
                    all.push((format!("{tr}::{}", tm.name), tm.sig.type_params.clone()));
                    for i in 0..tm.sig.type_params.len() {
                        let needed = all.iter().any(|(k, ps)| {
                            ps.get(i).is_some_and(|n| needs.contains(&(k.clone(), n.clone())))
                        });
                        if needed {
                            for (k, ps) in &all {
                                if let Some(n) = ps.get(i) {
                                    uses.push((k.clone(), n.clone()));
                                }
                            }
                        }
                    }
                }
            }
            // And a generic trait's method's trait-argument classes (see `trait_arg_names`):
            // each impl takes them after its own, as its parameters the arguments name.
            for (tr, def) in &self.adts.traits {
                for tm in &def.methods {
                    let at = self.trait_arg_names(tr, &tm.name);
                    if at.is_empty() {
                        continue;
                    }
                    let own = tm.sig.type_params.len();
                    let mut all: Vec<(String, Vec<String>)> = self
                        .adts
                        .trait_impls
                        .iter()
                        .filter(|(_, trs)| trs.iter().any(|t| t == tr))
                        .filter_map(|(ty, _)| {
                            let s = self.adts.method(ty, &tm.name)?;
                            Some((format!("{ty}::{}", tm.name), s.dict_names()))
                        })
                        .collect();
                    all.push((format!("{tr}::{}", tm.name), param_names(self, &format!("{tr}::{}", tm.name))));
                    let alts = self.adts.trait_impls.iter().any(|(ty, trs)| {
                        trs.iter().any(|t| t == tr) && self.adts.alt_trait_args(ty, &tm.name).is_some()
                    });
                    for i in own..own + at.len() {
                        let needed = alts || all.iter().any(|(k, ps)| {
                            ps.get(i).is_some_and(|n| needs.contains(&(k.clone(), n.clone())))
                        });
                        if needed {
                            for (k, ps) in &all {
                                if let Some(n) = ps.get(i) {
                                    uses.push((k.clone(), n.clone()));
                                }
                            }
                        }
                    }
                }
            }
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
                        if let Some(p) = class_param(t)
                            && self.stored_uses.contains(&(adt.clone(), i))
                        {
                            uses.push((f.clone(), p.to_string()));
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
    /// Whether a type path's qualifier is std's: rooted at `std` / `core` / `alloc`, or a std
    /// module the file doesn't declare itself (`btree_map::Keys`, `vec::Drain`, `slice::Iter`).
    fn std_qualified(&self, path: &ast::Path) -> bool {
        let Some(q) = path.qualifier() else {
            return false;
        };
        let root = path.first_segment().map(|s| s.syntax().text().to_string());
        if matches!(root.as_deref(), Some("std" | "core" | "alloc")) {
            return true;
        }
        let Some(last) = q.segment().map(|s| s.syntax().text().to_string()) else {
            return false;
        };
        matches!(
            last.as_str(),
            "btree_map"
                | "btree_set"
                | "hash_map"
                | "hash_set"
                | "vec"
                | "vec_deque"
                | "slice"
                | "binary_heap"
                | "linked_list"
                | "str"
                | "string"
        ) && !self.src.lines().any(|l| {
            let l = l.trim_start().trim_start_matches("pub ");
            l.strip_prefix("mod ")
                .is_some_and(|r| r.trim_end_matches([';', ' ', '{']) == last)
        })
    }

    /// Whether a `use std::..` item imports `name` (`use std::vec::IntoIter;`), so an
    /// unqualified `IntoIter<T>` is std's.
    fn std_imported(&self, name: &str) -> bool {
        self.src.lines().any(|l| {
            let l = l.trim_start();
            (l.starts_with("use std::") || l.starts_with("use core::"))
                && l.split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|w| w == name)
        })
    }

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
                | "Chain"
                | "Zip"
                | "Enumerate"
                | "Chars"
                | "CharIndices"
                | "SplitWhitespace"
                | "Keys"
                | "Values"
                | "ValuesMut"
        ) || (matches!(
            name,
            "Iter" | "IntoIter" | "IterMut" | "Bytes" | "Lines" | "Split"
        ) && (qualified || self.std_imported(name)))
            || (qualified
                && matches!(
                    name,
                    "IntoKeys"
                        | "IntoValues"
                        | "Drain"
                        | "Difference"
                        | "Intersection"
                        | "Union"
                        | "SymmetricDifference"
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
            // (`Chain<A, B>`: `A`'s items; `Zip<A, B>`: pairs of both.)
            "Chain" => inner(&args),
            "Zip" => match args.as_slice() {
                [Ty::Iter { elem: a, .. }, Ty::Iter { elem: b, .. }] => {
                    iter(Ty::Tuple(vec![(**a).clone(), (**b).clone()]), false)
                }
                _ => iter(Ty::Unknown, false),
            },
            "Enumerate" => match inner(&args) {
                Ty::Iter { elem, mutable } => iter(Ty::Tuple(vec![USIZE, *elem]), mutable),
                other => other,
            },
            "Chars" => iter(Ty::Char, false),
            "CharIndices" => iter(Ty::Tuple(vec![USIZE, Ty::Char]), false),
            // (`hash_map::Keys<K, V>` and friends: the key or value type.)
            "Keys" | "Values" | "ValuesMut" | "IntoKeys" | "IntoValues" => match args.as_slice() {
                [k, v] => iter(
                    if matches!(name, "Keys" | "IntoKeys") { k.clone() } else { v.clone() },
                    name == "ValuesMut",
                ),
                _ => return None,
            },
            // (`btree_set::Difference<'a, T>` and friends: the set's items; `vec::Drain<'a, T>`.)
            "Difference" | "Intersection" | "Union" | "SymmetricDifference" | "Drain" => {
                match args.as_slice() {
                    [t] => iter(t.clone(), false),
                    [k, v] => iter(Ty::Tuple(vec![k.clone(), v.clone()]), false),
                    _ => return None,
                }
            }
            "Bytes" => iter(Ty::Int(IntKind::U8), false),
            // (`io::Lines<BufReader<File>>`, with a type argument: `io::Result<String>`s.)
            "Lines" if !args.is_empty() => iter(
                Ty::Result(
                    Box::new(Ty::Str),
                    Box::new(Ty::Adt("IoError".into(), vec![])),
                ),
                false,
            ),
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

impl Checker<'_> {
    /// The value a constructor path passed as a function makes (`Vec::new`, `String::new`,
    /// `HashSet::new`, `HashMap::new`): `or_insert_with(Vec::new)` builds the type's default.
    fn ctor_path_ty(&mut self, arg: Option<&ast::Expr>) -> Option<Ty> {
        let Some(ast::Expr::PathExpr(p)) = arg else {
            return None;
        };
        let text = p.syntax().text().to_string();
        match std_tail(&text.split("::").collect::<Vec<_>>()) {
            [.., "Vec", "new"] => Some(Ty::Vec(Box::new(self.fresh(VarKind::General)))),
            [.., "String", "new"] => Some(Ty::Str),
            [.., "HashSet", "new"] => Some(Ty::Set(Box::new(self.fresh(VarKind::General)))),
            [.., "HashMap", "new"] => Some(Ty::Map(
                Box::new(self.fresh(VarKind::General)),
                Box::new(self.fresh(VarKind::General)),
            )),
            _ => None,
        }
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
/// A parameter type written as a reference (`&str`, `&[T]`, `&mut Vec<T>`): its text.
pub(super) fn written_ref(t: &ast::Type) -> Option<String> {
    match t {
        ast::Type::RefType(_) => Some(t.syntax().text().to_string().split_whitespace().collect::<Vec<_>>().join(" ")),
        _ => None,
    }
}

/// Whether a type is written `&mut T` or `Option<&mut T>`.
pub(super) fn written_mut_ref(t: &ast::Type) -> bool {
    match t {
        ast::Type::RefType(r) => r.mut_token().is_some(),
        ast::Type::PathType(p) => p
            .path()
            .and_then(|p| p.segment())
            .filter(|s| s.name_ref().is_some_and(|n| n.text() == "Option"))
            .and_then(|s| s.generic_arg_list())
            .and_then(|g| g.generic_args().next())
            .is_some_and(|a| match a {
                ast::GenericArg::TypeArg(t) => t
                    .ty()
                    .is_some_and(|t| matches!(t, ast::Type::RefType(r) if r.mut_token().is_some())),
                _ => false,
            }),
        _ => false,
    }
}

/// `&mut T` as written (not `Option<&mut T>`).
pub(super) fn written_mut_ref_type(t: &ast::Type) -> bool {
    matches!(t, ast::Type::RefType(r) if r.mut_token().is_some())
}

/// `Vec<&T>` / `VecDeque<&T>`, also behind `&mut`: `&T` as written.
pub(super) fn ref_elem_written(t: &ast::Type) -> Option<String> {
    let t = match t {
        ast::Type::RefType(r) => r.ty()?,
        t => t.clone(),
    };
    let ast::Type::PathType(p) = &t else { return None };
    let seg = p.path()?.segment()?;
    if !matches!(seg.name_ref()?.text().to_string().as_str(), "Vec" | "VecDeque") {
        return None;
    }
    let Some(ast::GenericArg::TypeArg(ta)) = seg.generic_arg_list()?.generic_args().next() else {
        return None;
    };
    let ast::Type::RefType(r) = ta.ty()? else { return None };
    if r.mut_token().is_some() {
        return None;
    }
    Some(r.syntax().text().to_string().split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Whether a type is written as an owned value: not a reference, pointer, `impl Trait` or
/// `dyn Trait`, and holding no reference or lifetime inside (`Option<&str>`, `Parser<'a>`).
pub(super) fn written_owned(t: &ast::Type) -> bool {
    if matches!(
        t,
        ast::Type::RefType(_)
            | ast::Type::PtrType(_)
            | ast::Type::ImplTraitType(_)
            | ast::Type::DynTraitType(_)
            | ast::Type::FnPtrType(_)
            | ast::Type::InferType(_)
    ) {
        return false;
    }
    let text = t.syntax().text().to_string();
    !text.contains('&') && !text.contains('\'') && !text.contains("dyn ") && !text.contains("impl ")
}

pub(super) fn into_param_flags(f: &ast::Fn) -> Vec<bool> {
    use ra_ap_syntax::ast::HasTypeBounds;
    let into_bound = |l: &ast::TypeBoundList| {
        l.bounds().any(|b| {
            matches!(b.ty(), Some(ast::Type::PathType(p))
                if p.path().and_then(|p| p.segment()).and_then(|s| s.name_ref()).is_some_and(|n| matches!(n.text().to_string().as_str(), "Into" | "AsRef" | "AsMut" | "Borrow" | "BorrowMut")))
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
                // (`haystack: &'h H` with `H: ?Sized + AsRef<[u8]>`: the bound is on `H`.)
                .map(|p| match p.ty().map(|t| match &t {
                    ast::Type::RefType(r)
                        if matches!(r.ty(), Some(ast::Type::PathType(_))) =>
                    {
                        r.ty().unwrap()
                    }
                    _ => t,
                }) {
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

/// The type parameter whose class a struct's values can carry at an argument of this type: `T`,
/// or the element of a `Vec<T>` / `Option<T>`, whose classes hold their element's (`.elem`).
pub(crate) fn class_param(t: &Ty) -> Option<&str> {
    match t {
        Ty::Param(p) => Some(p),
        Ty::Vec(e) | Ty::Option(e) => match &**e {
            Ty::Param(p) => Some(p),
            _ => None,
        },
        _ => None,
    }
}

/// A range literal converted with `.into()` / `.try_into()` or passed to `X::from(..)` /
/// `X::try_from(..)` (`(1..=3).into()` with `impl From<RangeInclusive<usize>> for X`): a value,
/// not an iterator.
fn range_conv_value(r: &ast::RangeExpr) -> bool {
    let mut node = r.syntax().clone();
    while let Some(p) = node
        .parent()
        .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
    {
        node = p;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    if let Some(m) = ast::MethodCallExpr::cast(parent.clone()) {
        return m.receiver().is_some_and(|x| x.syntax() == &node)
            && m.name_ref().is_some_and(|n| matches!(n.text().to_string().as_str(), "into" | "try_into"));
    }
    let Some(c) = parent.parent().and_then(ast::CallExpr::cast) else {
        return false;
    };
    let Some(ast::Expr::PathExpr(p)) = c.expr() else {
        return false;
    };
    let last = p.path().and_then(|p| p.segment()).and_then(|s| s.name_ref());
    last.is_some_and(|n| matches!(n.text().to_string().as_str(), "from" | "try_from"))
        && p.path().and_then(|p| p.qualifier()).is_some()
        && c.arg_list().is_some_and(|l| l.args().count() == 1)
}

/// `e` without the `?`, `.unwrap()` and `.expect(..)` around it, and whether there were any.
fn strip_unwraps(e: &ast::Expr) -> (ast::Expr, bool) {
    let mut e = strip_parens(e);
    let mut stripped = false;
    loop {
        let inner = match &e {
            ast::Expr::TryExpr(t) => t.expr(),
            ast::Expr::MethodCallExpr(m)
                if m.name_ref().is_some_and(|n| matches!(n.text(), "unwrap" | "expect")) =>
            {
                m.receiver()
            }
            _ => None,
        };
        let Some(inner) = inner else { return (e, stripped) };
        e = strip_parens(&inner);
        stripped = true;
    }
}

/// A `HashSet` or `BTreeSet` (the operands of `&a | &b`).
pub fn is_set(t: &Ty) -> bool {
    matches!(t, Ty::Set(_) | Ty::Coll(crate::ty::Coll::BTreeSet, _))
}

/// The `n`th placeholder (`{}`, `{:?}`, `{name}`) of a format string literal: its absolute range.
fn placeholder_at(fmt: &MacroArg, n: usize) -> Option<(usize, usize)> {
    let t = fmt.text.as_str();
    if !(t.starts_with('"') || t.starts_with('r')) {
        return None;
    }
    let b = t.as_bytes();
    let (mut i, mut k) = (0, 0);
    while i < b.len() {
        match b[i] {
            b'{' if b.get(i + 1) == Some(&b'{') => i += 2,
            b'}' if b.get(i + 1) == Some(&b'}') => i += 2,
            b'{' => {
                let close = i + t[i..].find('}')?;
                if k == n {
                    return Some((fmt.start + i, fmt.start + close + 1));
                }
                k += 1;
                i = close + 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// Whether `node` is in an associated const of an `impl` whose self type takes the type
/// parameter `param` (`impl<T: Zero> Zero for Wrapping<T> { const ZERO: Self = Wrapping(T::ZERO); }`).
fn impl_const_param(node: &SyntaxNode, param: &str) -> bool {
    let Some(k) = node.ancestors().find_map(ast::Const::cast) else {
        return false;
    };
    let Some(imp) = k.syntax().ancestors().find_map(ast::Impl::cast) else {
        return false;
    };
    crate::adt::impl_type_param_pos(&imp, param).is_some()
}
