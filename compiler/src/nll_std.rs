//! The borrow table for the std surface: how each std method takes its receiver, and what its
//! result borrows (`v.iter()` borrows `v`; `it.next()` hands out what the iterator borrows, not
//! the iterator; `opt.as_ref()` borrows the option; `s.len()` borrows nothing past the call).
//! Anything not listed takes `&self` and returns an owned value with no references in it, which
//! can never produce a false error.

use crate::nll_bir::{Body, RTy, Region, Span};
use crate::nll_rty::{Env, Recv};
use crate::ty::Ty;

/// How a std method takes its receiver, given the receiver's type with references stripped.
pub fn recv_mode(base: &RTy, name: &str) -> Recv {
    match base {
        RTy::Iter { .. } => match name {
            "next" | "next_back" | "nth" | "nth_back" | "peek" | "peek_mut" | "by_ref" | "any"
            | "all" | "find" | "find_map" | "position" | "rposition" | "try_fold"
            | "try_for_each" | "advance_by" | "next_if" | "next_if_eq" | "size_hint" | "len"
            | "is_empty" | "key" => {
                if matches!(name, "size_hint" | "len" | "is_empty" | "key") {
                    Recv::Ref
                } else {
                    Recv::MutRef
                }
            }
            "clone" | "as_str" | "as_slice" | "get" | "remove_entry" => Recv::Ref,
            // Adapters and consumers take the iterator by value; so do an entry's methods.
            _ => Recv::Value,
        },
        RTy::Option(_) | RTy::Result(..) => match name {
            "as_ref" | "is_some" | "is_none" | "is_ok" | "is_err" | "iter" | "as_deref"
            | "is_some_and" | "is_none_or" | "is_ok_and" | "is_err_and" | "as_slice" | "clone"
            | "cloned" | "copied" | "unwrap_or_default" | "eq" | "ne" | "cmp" | "partial_cmp"
            | "to_owned" | "hash" | "fmt" | "to_string" => {
                if matches!(name, "cloned" | "copied" | "unwrap_or_default") {
                    Recv::Value
                } else {
                    Recv::Ref
                }
            }
            "as_mut" | "take" | "replace" | "insert" | "get_or_insert" | "get_or_insert_with"
            | "iter_mut" | "as_deref_mut" | "as_mut_slice" => Recv::MutRef,
            _ => Recv::Value,
        },
        RTy::Vec(_) | RTy::Set(_) | RTy::Map(..) | RTy::String | RTy::Str => match name {
            "push" | "push_str" | "push_back" | "push_front" | "pop" | "pop_back" | "pop_front"
            | "insert" | "insert_str" | "remove" | "swap_remove" | "clear" | "truncate"
            | "retain" | "retain_mut" | "dedup" | "dedup_by" | "dedup_by_key" | "sort"
            | "sort_by" | "sort_by_key" | "sort_unstable" | "sort_unstable_by"
            | "sort_unstable_by_key" | "sort_by_cached_key" | "reverse" | "extend"
            | "extend_from_slice" | "extend_from_within" | "append" | "drain" | "split_off"
            | "resize" | "resize_with" | "fill" | "fill_with" | "swap" | "rotate_left"
            | "rotate_right" | "iter_mut" | "get_mut" | "first_mut" | "last_mut"
            | "values_mut" | "entry" | "as_mut" | "as_mut_slice" | "as_mut_str"
            | "chunks_mut" | "chunks_exact_mut" | "split_at_mut" | "split_first_mut"
            | "split_last_mut" | "make_ascii_uppercase" | "make_ascii_lowercase" | "reserve"
            | "reserve_exact" | "shrink_to_fit" | "shrink_to" | "copy_from_slice"
            | "clone_from_slice" | "clone_from" | "copy_within" | "splice" | "pop_first" | "pop_last"
            | "remove_entry" | "take" | "replace_range" | "get_or_insert_with"
            | "get_mut_or_insert_with" | "extract_if" | "swap_indices" | "shift_remove"
            | "swap_remove_index" | "shift_insert" | "move_index" | "sort_keys"
            | "first_entry" | "last_entry" | "iter_mut_range" | "range_mut"
            | "get_index_mut" | "last_mut_entry" | "or_insert" => {
                if name == "take" && matches!(base, RTy::Str | RTy::String) {
                    Recv::Ref
                } else {
                    Recv::MutRef
                }
            }
            "into_iter" | "into_keys" | "into_values" | "into_bytes" | "into_boxed_slice"
            | "into_boxed_str" | "into_sorted_vec" | "into_vec" | "into_string" => Recv::Value,
            _ => Recv::Ref,
        },
        RTy::Cell(_) => match name {
            "get_mut" | "into_inner" => {
                if name == "into_inner" {
                    Recv::Value
                } else {
                    Recv::MutRef
                }
            }
            _ => Recv::Ref,
        },
        RTy::Closure { .. } => Recv::Ref,
        RTy::Tuple(_) | RTy::Adt(..) | RTy::Ref(..) | RTy::Opaque(_) => match name {
            "into_iter" | "into" | "into_inner" | "unwrap" | "expect" => Recv::Value,
            "next" | "next_back" | "nth" | "by_ref" => Recv::MutRef,
            _ => Recv::Ref,
        },
    }
}

/// What a std method returns, given the receiver's type with references stripped, the region
/// (and mutability) of the receiver borrow, and the arguments' types. `ty` is the type checker's
/// view of the result, which settles `collect`. Also the type each argument is expected to have
/// (where it matters for borrowing), by position.
#[allow(clippy::too_many_arguments)]
pub fn result(
    env: &Env,
    body: &mut Body,
    base: &RTy,
    r: Region,
    name: &str,
    args: &[RTy],
    ty: Option<&Ty>,
    span: Span,
) -> (RTy, Vec<Option<RTy>>) {
    let rf = |t: RTy| RTy::Ref(r, false, Box::new(t));
    let rm = |t: RTy| RTy::Ref(r, true, Box::new(t));
    let opt = |t: RTy| RTy::Option(Box::new(t));
    let iter = |region: Option<Region>, mutable: bool, elem: RTy| RTy::Iter {
        region,
        mutable,
        elem: Box::new(elem),
    };
    let closure_ret = |i: usize| -> RTy {
        match args.get(i) {
            Some(RTy::Closure { ret, .. }) => (**ret).clone(),
            _ => RTy::opaque(),
        }
    };
    let closure_region = |i: usize| -> Option<Region> {
        match args.get(i) {
            Some(RTy::Closure { region, .. }) => *region,
            Some(t) => {
                let mut rs = Vec::new();
                t.regions(&mut rs);
                rs.first().copied()
            }
            None => None,
        }
    };
    let none = Vec::new();
    match base {
        RTy::Vec(e) => {
            let e = (**e).clone();
            let t = match name {
                "iter" => iter(Some(r), false, rf(e)),
                "iter_mut" => iter(Some(r), true, rm(e)),
                "drain" | "extract_if" => iter(Some(r), true, e),
                "get" | "first" | "last" => match ty {
                    Some(Ty::Option(inner)) if matches!(**inner, Ty::Vec(_)) && !matches!(e, RTy::Vec(_)) => {
                        opt(rf(RTy::Vec(Box::new(e))))
                    }
                    _ => opt(rf(e)),
                },
                "get_mut" | "first_mut" | "last_mut" => match ty {
                    Some(Ty::Option(inner)) if matches!(**inner, Ty::Vec(_)) && !matches!(e, RTy::Vec(_)) => {
                        opt(rm(RTy::Vec(Box::new(e))))
                    }
                    _ => opt(rm(e)),
                },
                "as_slice" | "as_ref" | "borrow" | "deref" => rf(RTy::Vec(Box::new(e))),
                "as_mut_slice" | "as_mut" | "borrow_mut" | "deref_mut" => rm(RTy::Vec(Box::new(e))),
                "windows" | "chunks" | "chunks_exact" | "rchunks" | "split" | "splitn"
                | "rsplit" | "group_by" | "chunk_by" => iter(Some(r), false, rf(RTy::Vec(Box::new(e)))),
                "chunks_mut" | "chunks_exact_mut" | "split_mut" => {
                    iter(Some(r), true, rm(RTy::Vec(Box::new(e))))
                }
                "split_at" => RTy::Tuple(vec![rf(RTy::Vec(Box::new(e.clone()))), rf(RTy::Vec(Box::new(e)))]),
                "split_at_mut" => RTy::Tuple(vec![rm(RTy::Vec(Box::new(e.clone()))), rm(RTy::Vec(Box::new(e)))]),
                "split_first" | "split_last" => opt(RTy::Tuple(vec![rf(e.clone()), rf(RTy::Vec(Box::new(e)))])),
                "split_first_mut" | "split_last_mut" => opt(RTy::Tuple(vec![rm(e.clone()), rm(RTy::Vec(Box::new(e)))])),
                "pop" | "pop_back" | "pop_front" | "swap_remove" | "pop_first" | "pop_last" => opt(e),
                "remove" => match ty {
                    Some(Ty::Option(_)) => opt(e),
                    _ => e,
                },
                "split_off" | "clone" | "to_vec" | "to_owned" | "into_sorted_vec" | "into_vec" => RTy::Vec(Box::new(e)),
                "into_iter" => iter(None, false, e),
                "concat" => match e {
                    RTy::Vec(inner) => RTy::Vec(inner),
                    _ => RTy::String,
                },
                "join" | "connect" => RTy::String,
                "push" | "push_back" | "push_front" => {
                    return (RTy::opaque(), vec![Some(e)]);
                }
                "insert" => return (RTy::opaque(), vec![None, Some(e)]),
                "contains" | "binary_search" | "starts_with" | "ends_with" => RTy::opaque(),
                "repeat" => RTy::Vec(Box::new(e)),
                "peek" | "peek_mut" => {
                    if name == "peek" { opt(rf(e)) } else { opt(rm(e)) }
                }
                "front" | "back" => opt(rf(e)),
                "front_mut" | "back_mut" => opt(rm(e)),
                "as_slices" => RTy::Tuple(vec![rf(RTy::Vec(Box::new(e.clone()))), rf(RTy::Vec(Box::new(e)))]),
                "range" => iter(Some(r), false, rf(e)),
                "range_mut" => iter(Some(r), true, rm(e)),
                "last_mut_entry" => opt(rm(e)),
                _ => RTy::opaque(),
            };
            (t, none)
        }
        RTy::String | RTy::Str => {
            let t = match name {
                "as_str" | "as_ref" | "trim" | "trim_start" | "trim_end" | "trim_matches"
                | "trim_start_matches" | "trim_end_matches" | "trim_left" | "trim_right"
                | "borrow" | "deref" | "as_mut_str" | "trim_ascii" | "trim_ascii_start"
                | "trim_ascii_end" => {
                    if name == "as_mut_str" { rm(RTy::Str) } else { rf(RTy::Str) }
                }
                "strip_prefix" | "strip_suffix" | "get" => opt(rf(RTy::Str)),
                "as_bytes" => rf(RTy::Vec(Box::new(RTy::opaque()))),
                "as_bytes_mut" => rm(RTy::Vec(Box::new(RTy::opaque()))),
                "split_at" => RTy::Tuple(vec![rf(RTy::Str), rf(RTy::Str)]),
                "split_once" | "rsplit_once" => opt(RTy::Tuple(vec![rf(RTy::Str), rf(RTy::Str)])),
                "split_whitespace" | "split" | "rsplit" | "splitn" | "rsplitn" | "lines"
                | "split_terminator" | "rsplit_terminator" | "matches" | "split_ascii_whitespace"
                | "split_inclusive" => iter(Some(r), false, rf(RTy::Str)),
                "chars" | "bytes" | "char_indices" | "match_indices" | "encode_utf16"
                | "escape_debug" | "escape_default" => {
                    if name == "match_indices" {
                        iter(Some(r), false, RTy::Tuple(vec![RTy::opaque(), rf(RTy::Str)]))
                    } else {
                        iter(Some(r), false, RTy::opaque())
                    }
                }
                "drain" => iter(Some(r), true, RTy::opaque()),
                "to_string" | "to_owned" | "to_uppercase" | "to_lowercase" | "replace"
                | "replacen" | "repeat" | "to_ascii_uppercase" | "to_ascii_lowercase"
                | "into_string" | "into_boxed_str" | "trim_end_matches_owned" => RTy::String,
                "clone" => base.clone(),
                "into_bytes" => RTy::Vec(Box::new(RTy::opaque())),
                "push_str" => return (RTy::opaque(), vec![Some(rf(RTy::Str))]),
                _ => RTy::opaque(),
            };
            (t, none)
        }
        RTy::Map(k, v) => {
            let (k, v) = ((**k).clone(), (**v).clone());
            let t = match name {
                "get" => opt(rf(v)),
                "get_mut" => opt(rm(v)),
                "get_key_value" => opt(RTy::Tuple(vec![rf(k), rf(v)])),
                "iter" | "range" => iter(Some(r), false, RTy::Tuple(vec![rf(k), rf(v)])),
                "iter_mut" | "range_mut" => iter(Some(r), true, RTy::Tuple(vec![rf(k), rm(v)])),
                "keys" => iter(Some(r), false, rf(k)),
                "values" => iter(Some(r), false, rf(v)),
                "values_mut" => iter(Some(r), true, rm(v)),
                "into_iter" => iter(None, false, RTy::Tuple(vec![k, v])),
                "into_keys" => iter(None, false, k),
                "into_values" => iter(None, false, v),
                "drain" | "extract_if" => iter(Some(r), true, RTy::Tuple(vec![k, v])),
                "entry" => iter(Some(r), true, v),
                "insert" => return (opt(v.clone()), vec![Some(k), Some(v)]),
                "remove" | "shift_remove" | "swap_remove" => opt(v),
                "remove_entry" | "pop_first" | "pop_last" | "shift_remove_entry" => {
                    opt(RTy::Tuple(vec![k, v]))
                }
                "first_key_value" | "last_key_value" => opt(RTy::Tuple(vec![rf(k), rf(v)])),
                "first_entry" | "last_entry" => opt(iter(Some(r), true, v)),
                "get_index" => opt(RTy::Tuple(vec![rf(k), rf(v)])),
                "get_index_mut" => opt(RTy::Tuple(vec![rf(k), rm(v)])),
                "clone" => base.clone(),
                _ => RTy::opaque(),
            };
            (t, none)
        }
        RTy::Set(e) => {
            let e = (**e).clone();
            let t = match name {
                "get" | "first" | "last" => opt(rf(e)),
                "iter" | "range" => iter(Some(r), false, rf(e)),
                "into_iter" => iter(None, false, e),
                "drain" | "extract_if" => iter(Some(r), true, e),
                "take" | "pop_first" | "pop_last" | "replace" => opt(e),
                "insert" => return (RTy::opaque(), vec![Some(e)]),
                "union" | "intersection" | "difference" | "symmetric_difference" => {
                    // (The other set is borrowed as long as the iterator lives.)
                    return (
                        iter(Some(r), false, rf(e.clone())),
                        vec![Some(rf(RTy::Set(Box::new(e))))],
                    );
                }
                "clone" => base.clone(),
                _ => RTy::opaque(),
            };
            (t, none)
        }
        RTy::Option(inner) => {
            let t = (**inner).clone();
            let out = match name {
                "as_ref" => opt(rf(t)),
                "as_mut" => opt(rm(t)),
                "as_deref" => opt(rf(deref_target(t))),
                "as_deref_mut" => opt(rm(deref_target(t))),
                "as_slice" => rf(RTy::Vec(Box::new(t))),
                "unwrap" | "expect" | "unwrap_unchecked" | "unwrap_or_default" => t,
                "unwrap_or" => return (t.clone(), vec![Some(t)]),
                "unwrap_or_else" => {
                    return (t.clone(), vec![Some(RTy::Closure { region: None, ret: Box::new(t) })]);
                }
                "map" => opt(closure_ret(0)),
                "map_or" => return (args.first().cloned().unwrap_or(RTy::opaque()), vec![None, Some(RTy::Closure { region: None, ret: Box::new(args.first().cloned().unwrap_or(RTy::opaque())) })]),
                "map_or_else" => closure_ret(1),
                "and_then" => closure_ret(0),
                "or" | "xor" => return (base.clone(), vec![Some(base.clone())]),
                "or_else" => base.clone(),
                "and" => args.first().cloned().unwrap_or(RTy::opaque()),
                "ok_or" => RTy::Result(Box::new(t), Box::new(args.first().cloned().unwrap_or(RTy::opaque()))),
                "ok_or_else" => RTy::Result(Box::new(t), Box::new(closure_ret(0))),
                "take" | "replace" | "filter" | "clone" | "inspect" => {
                    if name == "replace" {
                        return (base.clone(), vec![Some(t)]);
                    }
                    base.clone()
                }
                "insert" | "get_or_insert" => return (rm(t.clone()), vec![Some(t)]),
                "get_or_insert_with" | "get_or_insert_default" => rm(t),
                "iter" => iter(Some(r), false, rf(t)),
                "iter_mut" => iter(Some(r), true, rm(t)),
                "into_iter" => iter(None, false, t),
                "cloned" | "copied" => opt(deref_target(t)),
                "flatten" => t,
                "zip" => opt(RTy::Tuple(vec![
                    t,
                    match args.first() {
                        Some(RTy::Option(o)) => (**o).clone(),
                        _ => RTy::opaque(),
                    },
                ])),
                "unzip" => match t {
                    RTy::Tuple(es) if es.len() == 2 => {
                        RTy::Tuple(vec![opt(es[0].clone()), opt(es[1].clone())])
                    }
                    _ => RTy::opaque(),
                },
                "transpose" => match t {
                    RTy::Result(a, b) => RTy::Result(Box::new(opt(*a)), b),
                    _ => RTy::opaque(),
                },
                _ => RTy::opaque(),
            };
            (out, none)
        }
        RTy::Result(a, b) => {
            let (t, e) = ((**a).clone(), (**b).clone());
            let out = match name {
                "ok" => opt(t),
                "err" => opt(e),
                "unwrap" | "expect" | "unwrap_or_default" | "unwrap_unchecked" => t,
                "unwrap_err" | "expect_err" => e,
                "unwrap_or" => return (t.clone(), vec![Some(t)]),
                "unwrap_or_else" => {
                    return (t.clone(), vec![Some(RTy::Closure { region: None, ret: Box::new(t) })]);
                }
                "map" => RTy::Result(Box::new(closure_ret(0)), Box::new(e)),
                "map_err" => RTy::Result(Box::new(t), Box::new(closure_ret(0))),
                "and_then" | "or_else" => closure_ret(0),
                "map_or" => args.first().cloned().unwrap_or(RTy::opaque()),
                "map_or_else" => closure_ret(1),
                "as_ref" => RTy::Result(Box::new(rf(t)), Box::new(rf(e))),
                "as_mut" => RTy::Result(Box::new(rm(t)), Box::new(rm(e))),
                "as_deref" => RTy::Result(Box::new(rf(deref_target(t))), Box::new(rf(e))),
                "iter" => iter(Some(r), false, rf(t)),
                "iter_mut" => iter(Some(r), true, rm(t)),
                "into_iter" => iter(None, false, t),
                "cloned" | "copied" => RTy::Result(Box::new(deref_target(t)), Box::new(e)),
                "clone" | "inspect" | "inspect_err" => base.clone(),
                "or" | "and" => args.first().cloned().unwrap_or(RTy::opaque()),
                "transpose" => match t {
                    RTy::Option(inner) => opt(RTy::Result(inner, Box::new(e))),
                    _ => RTy::opaque(),
                },
                _ => RTy::opaque(),
            };
            (out, none)
        }
        RTy::Iter {
            region: ir,
            mutable,
            elem,
        } => {
            let elem = (**elem).clone();
            let ir = *ir;
            // A method taking the iterator (or entry) by value hands out what it borrowed, not a
            // borrow of itself.
            let r = match (recv_mode(base, name), ir) {
                (Recv::Value, Some(inner)) => inner,
                _ => r,
            };
            let rf = |t: RTy| RTy::Ref(r, false, Box::new(t));
            let rm = |t: RTy| RTy::Ref(r, true, Box::new(t));
            let same = |elem: RTy| iter(ir, *mutable, elem);
            let with = |body: &mut Body, other: Option<Region>, elem: RTy| {
                let region = union(body, ir, other, span);
                iter(region, *mutable, elem)
            };
            let out = match name {
                "next" | "next_back" | "last" | "max" | "min" | "nth" | "nth_back" | "find"
                | "max_by_key" | "min_by_key" | "max_by" | "min_by" | "reduce" | "next_if"
                | "next_if_eq" | "find_last" => opt(elem),
                "find_map" => closure_ret(0),
                "peek" => opt(rf(elem)),
                "peek_mut" => opt(rm(elem)),
                "by_ref" => rm(base.clone()),
                "count" | "position" | "rposition" | "any" | "all" | "sum" | "product"
                | "eq" | "ne" | "lt" | "le" | "gt" | "ge" | "cmp" | "partial_cmp" | "for_each"
                | "is_sorted" | "len" | "size_hint" | "is_empty" | "try_for_each"
                | "advance_by" => RTy::opaque(),
                "fold" | "try_fold" => {
                    let init = args.first().cloned().unwrap_or(RTy::opaque());
                    return (
                        init.clone(),
                        vec![None, Some(RTy::Closure { region: None, ret: Box::new(init) })],
                    );
                }
                "collect" | "collect_vec" => collect_shape(ty, &elem),
                "partition" => RTy::Tuple(vec![RTy::Vec(Box::new(elem.clone())), RTy::Vec(Box::new(elem))]),
                "unzip" => match elem {
                    RTy::Tuple(es) if es.len() == 2 => RTy::Tuple(vec![
                        collect_shape(unzip_ty(ty, 0).as_ref(), &es[0]),
                        collect_shape(unzip_ty(ty, 1).as_ref(), &es[1]),
                    ]),
                    _ => RTy::opaque(),
                },
                "map" => with(body, closure_region(0), closure_ret(0)),
                "filter" | "take_while" | "skip_while" | "inspect" | "map_while" => {
                    let out = with(body, closure_region(0), elem);
                    if name == "map_while" {
                        return (
                            match closure_ret(0) {
                                RTy::Option(inner) => with(body, closure_region(0), *inner),
                                _ => iter(ir, *mutable, RTy::opaque()),
                            },
                            none,
                        );
                    }
                    out
                }
                "filter_map" => match closure_ret(0) {
                    RTy::Option(inner) => with(body, closure_region(0), *inner),
                    _ => with(body, closure_region(0), RTy::opaque()),
                },
                "flat_map" => with(body, closure_region(0), elem_of(closure_ret(0))),
                "flatten" => same(elem_of(elem)),
                "scan" => {
                    let region = union(body, ir, closure_region(1), span);
                    let region = union(body, region, closure_region(0), span);
                    match closure_ret(1) {
                        RTy::Option(inner) => iter(region, *mutable, *inner),
                        _ => iter(region, *mutable, RTy::opaque()),
                    }
                }
                "enumerate" => same(RTy::Tuple(vec![RTy::opaque(), elem])),
                "zip" => {
                    let other = args.first().cloned().unwrap_or(RTy::opaque());
                    let (oreg, oelem) = iter_parts(body, &other);
                    with(body, oreg, RTy::Tuple(vec![elem, oelem]))
                }
                "chain" => {
                    let other = args.first().cloned().unwrap_or(RTy::opaque());
                    let (oreg, _) = iter_parts(body, &other);
                    with(body, oreg, elem)
                }
                "rev" | "skip" | "take" | "step_by" | "peekable" | "fuse" | "cycle" | "cloned"
                | "copied" | "into_iter" | "sorted" | "dedup" | "unique" | "skip_last"
                | "chunks" | "windows" | "tuple_windows" | "sorted_by" | "sorted_by_key"
                | "rev_iter" => {
                    if matches!(name, "cloned" | "copied") {
                        same(deref_target(elem))
                    } else {
                        same(elem)
                    }
                }
                "or_insert" | "or_insert_with" | "or_default" | "or_insert_with_key"
                | "get_mut" | "into_mut" => {
                    // An entry: hands out the slot.
                    if name == "or_insert" {
                        return (rm(elem.clone()), vec![Some(elem)]);
                    }
                    rm(elem)
                }
                "and_modify" => base.clone(),
                "key" => rf(RTy::opaque()),
                "get" => rf(elem),
                "insert" => return (rm(elem.clone()), vec![Some(elem)]),
                "remove" | "remove_entry" => RTy::opaque(),
                "as_str" | "as_slice" | "remainder" => match ir {
                    Some(_) => rf(RTy::Str),
                    None => RTy::opaque(),
                },
                "clone" | "into_owned" | "to_mut" | "to_owned" => {
                    // (A `Cow`: `to_mut` hands out its contents mutably; `into_owned` owns.)
                    match name {
                        "to_mut" => rm(elem),
                        "into_owned" | "to_owned" => elem,
                        _ => base.clone(),
                    }
                }
                "sum_by" | "max_by_cached_key" => RTy::opaque(),
                _ => RTy::opaque(),
            };
            (out, none)
        }
        RTy::Cell(inner) => {
            let t = (**inner).clone();
            let out = match name {
                "borrow" | "read" => rf(t),
                "borrow_mut" | "lock" | "write" | "get_mut" => match name {
                    "lock" | "write" => RTy::Result(Box::new(rm(t)), Box::new(RTy::opaque())),
                    _ => rm(t),
                },
                "try_borrow" | "try_read" => RTy::Result(Box::new(rf(t)), Box::new(RTy::opaque())),
                "try_borrow_mut" | "try_write" | "try_lock" => {
                    RTy::Result(Box::new(rm(t)), Box::new(RTy::opaque()))
                }
                "get" | "take" | "into_inner" | "replace" => t,
                "set" => return (RTy::opaque(), vec![Some(t)]),
                "clone" => base.clone(),
                _ => RTy::opaque(),
            };
            (out, none)
        }
        RTy::Tuple(_) => (
            if name == "clone" { base.clone() } else { RTy::opaque() },
            none,
        ),
        RTy::Adt(n, ..) => {
            let out = match name {
                "clone" | "to_owned" if env.is_clone_adt(n) => base.clone(),
                "to_string" => RTy::String,
                "iter" | "iter_mut" | "into_iter" | "next" | "peekable" | "map" | "filter"
                | "rev" | "take" | "skip" | "enumerate" | "zip" | "chain" | "collect"
                | "count" | "last" | "max" | "min" | "sum" | "fold" | "any" | "all" | "find"
                | "position" | "for_each" | "nth" | "step_by" | "cloned" | "copied"
                | "flat_map" | "filter_map" | "take_while" | "skip_while" | "max_by_key"
                | "min_by_key" | "max_by" | "min_by" | "inspect" | "by_ref" | "fuse" | "cycle"
                | "partition" | "unzip" | "reduce" | "scan" | "flatten" | "product" => {
                    // A user iterator: its items (erased) through the iterator table.
                    match env.adts.iter_items.get(n) {
                        Some(item) => {
                            let elem = env.rty_of_ty(body, item);
                            let it = iter(None, false, elem);
                            return result(env, body, &it, r, name, args, ty, span);
                        }
                        None => RTy::opaque(),
                    }
                }
                _ => RTy::opaque(),
            };
            (out, none)
        }
        RTy::Closure { ret, .. } => (
            if name == "clone" { base.clone() } else { (**ret).clone() },
            none,
        ),
        RTy::Ref(..) | RTy::Opaque(_) => (RTy::opaque(), none),
    }
}

/// The one region two borrowing things give what holds both of them.
fn union(body: &mut Body, a: Option<Region>, b: Option<Region>, span: Span) -> Option<Region> {
    match (a, b) {
        (None, None) => None,
        (Some(x), None) | (None, Some(x)) => Some(x),
        (Some(x), Some(y)) if x == y => Some(x),
        (Some(x), Some(y)) => {
            let r = body.new_region();
            body.outlives(x, r, span);
            body.outlives(y, r, span);
            Some(r)
        }
    }
}

/// What an iterable borrows and yields (`zip(&v)`, `chain(w.iter())`).
fn iter_parts(body: &mut Body, t: &RTy) -> (Option<Region>, RTy) {
    match t {
        RTy::Iter { region, elem, .. } => (*region, (**elem).clone()),
        RTy::Ref(r, m, inner) => match &**inner {
            RTy::Vec(e) | RTy::Set(e) => (Some(*r), RTy::Ref(*r, *m, e.clone())),
            RTy::Map(k, v) => (
                Some(*r),
                RTy::Tuple(vec![RTy::Ref(*r, false, k.clone()), RTy::Ref(*r, *m, v.clone())]),
            ),
            RTy::Str => (Some(*r), RTy::opaque()),
            _ => (Some(*r), RTy::opaque()),
        },
        RTy::Vec(e) | RTy::Set(e) => (None, (**e).clone()),
        RTy::Map(k, v) => (None, RTy::Tuple(vec![(**k).clone(), (**v).clone()])),
        RTy::Option(e) => (None, (**e).clone()),
        _ => {
            let mut rs = Vec::new();
            t.regions(&mut rs);
            let region = rs.first().copied();
            let _ = body;
            (region, RTy::opaque())
        }
    }
}

/// The items of an iterable value (for `flat_map` / `flatten`).
fn elem_of(t: RTy) -> RTy {
    match t {
        RTy::Iter { elem, .. } => *elem,
        RTy::Vec(e) | RTy::Set(e) | RTy::Option(e) => *e,
        RTy::Map(k, v) => RTy::Tuple(vec![*k, *v]),
        RTy::Ref(r, m, inner) => match *inner {
            RTy::Vec(e) | RTy::Set(e) | RTy::Option(e) => RTy::Ref(r, m, e),
            RTy::Map(k, v) => RTy::Tuple(vec![RTy::Ref(r, false, k), RTy::Ref(r, m, v)]),
            _ => RTy::opaque(),
        },
        _ => RTy::opaque(),
    }
}

/// `*x` for `cloned` / `as_deref`: the type behind one reference (or the `str` of a `String`).
fn deref_target(t: RTy) -> RTy {
    match t {
        RTy::Ref(_, _, inner) => *inner,
        RTy::String => RTy::Str,
        other => other,
    }
}

/// What `collect()` builds, by the type checker's result type: the same shape over the items.
fn collect_shape(ty: Option<&Ty>, elem: &RTy) -> RTy {
    match ty {
        Some(Ty::Vec(_)) => RTy::Vec(Box::new(elem.clone())),
        Some(Ty::Set(_)) => RTy::Set(Box::new(elem.clone())),
        Some(Ty::Str) => RTy::String,
        Some(Ty::Map(..)) => match elem {
            RTy::Tuple(es) if es.len() == 2 => {
                RTy::Map(Box::new(es[0].clone()), Box::new(es[1].clone()))
            }
            _ => RTy::Map(Box::new(RTy::opaque()), Box::new(RTy::opaque())),
        },
        Some(Ty::Coll(_, view)) => collect_shape(Some(view), elem),
        Some(Ty::Result(ok, _)) => {
            let inner = match elem {
                RTy::Result(a, _) => (**a).clone(),
                _ => RTy::opaque(),
            };
            RTy::Result(
                Box::new(collect_shape(Some(ok), &inner)),
                Box::new(RTy::opaque()),
            )
        }
        Some(Ty::Option(o)) => {
            let inner = match elem {
                RTy::Option(a) => (**a).clone(),
                _ => RTy::opaque(),
            };
            RTy::Option(Box::new(collect_shape(Some(o), &inner)))
        }
        Some(Ty::Adt(..)) | Some(Ty::Var(_)) | Some(Ty::Unknown) | None => RTy::opaque(),
        _ => RTy::opaque(),
    }
}

fn unzip_ty(ty: Option<&Ty>, i: usize) -> Option<Ty> {
    match ty {
        Some(Ty::Tuple(es)) => es.get(i).cloned(),
        _ => None,
    }
}

/// Whether a free std function moves its arguments: `drop(x)`, `std::mem::take(&mut x)`.
pub fn free_fn_result(
    env: &Env,
    body: &mut Body,
    path: &[String],
    args: &[RTy],
) -> Option<(RTy, Vec<Option<RTy>>)> {
    let last = path.last()?.as_str();
    let ty = path.get(path.len().wrapping_sub(2)).map(|s| s.as_str());
    let _ = env;
    Some(match (ty, last) {
        (_, "Some") => (
            RTy::Option(Box::new(args.first().cloned().unwrap_or(RTy::opaque()))),
            Vec::new(),
        ),
        (_, "Ok") => (
            RTy::Result(
                Box::new(args.first().cloned().unwrap_or(RTy::opaque())),
                Box::new(RTy::opaque()),
            ),
            Vec::new(),
        ),
        (_, "Err") => (
            RTy::Result(
                Box::new(RTy::opaque()),
                Box::new(args.first().cloned().unwrap_or(RTy::opaque())),
            ),
            Vec::new(),
        ),
        (Some("Box") | Some("Rc") | Some("Arc"), "new") | (_, "identity") | (_, "black_box") => {
            (args.first().cloned().unwrap_or(RTy::opaque()), Vec::new())
        }
        (Some("RefCell") | Some("Cell") | Some("Mutex") | Some("RwLock"), "new") => (
            RTy::Cell(Box::new(args.first().cloned().unwrap_or(RTy::opaque()))),
            Vec::new(),
        ),
        (Some("mem"), "take") | (Some("mem"), "replace") => match args.first() {
            Some(RTy::Ref(_, _, inner)) => ((**inner).clone(), Vec::new()),
            _ => (RTy::opaque(), Vec::new()),
        },
        (Some("mem"), "swap") => {
            // Both arguments point at the same type (each through its own borrow).
            let expect = match args.first() {
                Some(RTy::Ref(_, _, inner)) => {
                    let r = body.new_region();
                    Some(RTy::Ref(r, true, inner.clone()))
                }
                _ => None,
            };
            (RTy::opaque(), vec![None, expect])
        }
        (Some("String"), "from") | (Some("String"), "new") | (Some("String"), "with_capacity")
        | (Some("String"), "from_utf8_lossy") => (RTy::String, Vec::new()),
        (Some("Vec"), "new") | (Some("Vec"), "with_capacity") | (Some("Vec"), "from") => (
            match args.first() {
                Some(RTy::Vec(e)) => RTy::Vec(e.clone()),
                Some(RTy::Ref(_, _, inner)) => match &**inner {
                    RTy::Vec(e) => RTy::Vec(e.clone()),
                    _ => RTy::Vec(Box::new(RTy::opaque())),
                },
                _ => RTy::Vec(Box::new(RTy::opaque())),
            },
            Vec::new(),
        ),
        (Some("HashMap") | Some("BTreeMap") | Some("IndexMap"), "new")
        | (Some("HashMap") | Some("BTreeMap") | Some("IndexMap"), "with_capacity") => (
            RTy::Map(Box::new(RTy::opaque()), Box::new(RTy::opaque())),
            Vec::new(),
        ),
        (Some("HashSet") | Some("BTreeSet") | Some("IndexSet"), "new")
        | (Some("HashSet") | Some("BTreeSet") | Some("IndexSet"), "with_capacity") => {
            (RTy::Set(Box::new(RTy::opaque())), Vec::new())
        }
        (Some("iter"), "once") | (Some("iter"), "repeat") => (
            RTy::Iter {
                region: None,
                mutable: false,
                elem: Box::new(args.first().cloned().unwrap_or(RTy::opaque())),
            },
            Vec::new(),
        ),
        (Some("Cow"), "Borrowed") => match args.first() {
            Some(RTy::Ref(r, _, inner)) => (
                RTy::Iter {
                    region: Some(*r),
                    mutable: false,
                    elem: inner.clone(),
                },
                Vec::new(),
            ),
            _ => (RTy::opaque(), Vec::new()),
        },
        (Some("Cow"), "Owned") => (
            RTy::Iter {
                region: None,
                mutable: false,
                elem: Box::new(args.first().cloned().unwrap_or(RTy::opaque())),
            },
            Vec::new(),
        ),
        _ => return None,
    })
}
