//! Signatures and one-line summaries of the std methods the compiler supports, for hover,
//! signature help and completions (`service.rs`). The summaries are the first sentence of each
//! method's std doc comment.
//!
//! A signature is written over the owner's own type parameters (`T`, `K`, `V`, `E`; `Self` for a
//! number or `char`), which [`show`] replaces with the receiver's arguments, so `v.first()` on a
//! `Vec<i32>` shows `fn first(&self) -> Option<&i32>` on `[i32]`, as rust-analyzer does.

use crate::ty::{Coll, Ty};

/// One std method: its owner (the type or trait it's declared on, as a hover names it), its
/// name, its signature, and a summary.
pub struct StdDoc {
    pub owner: &'static str,
    pub name: &'static str,
    pub sig: &'static str,
    pub doc: &'static str,
}

/// The owners whose methods a value of type `t` has, in lookup order (`Vec<T>` then `[T]`), and
/// the receiver's type parameters (`T`, or `K` and `V`, or `T` and `E`) as described types.
pub fn owners(t: &Ty) -> Option<(Vec<&'static str>, Vec<(&'static str, String)>)> {
    Some(match t {
        Ty::Vec(e) => (vec!["Vec", "slice"], vec![("T", e.describe())]),
        Ty::Coll(Coll::FixedArray(_), v) => return owners(v),
        Ty::Coll(Coll::VecDeque, v) => (vec!["VecDeque", "slice"], vec![("T", elem(v))]),
        Ty::Coll(Coll::BinaryHeap, v) => (vec!["BinaryHeap"], vec![("T", elem(v))]),
        Ty::Coll(Coll::BTreeMap, v) => (vec!["BTreeMap", "HashMap"], kv(v)),
        Ty::Coll(Coll::IndexMap, v) => (vec!["HashMap"], kv(v)),
        Ty::Coll(Coll::BTreeSet, v) => (vec!["BTreeSet", "HashSet"], vec![("T", elem(v))]),
        Ty::Coll(Coll::IndexSet, v) => (vec!["HashSet"], vec![("T", elem(v))]),
        Ty::Str => (vec!["String", "str"], vec![]),
        Ty::Option(e) => (vec!["Option"], vec![("T", e.describe())]),
        Ty::Result(t, e) => (vec!["Result"], vec![("T", t.describe()), ("E", e.describe())]),
        Ty::Set(e) => (vec!["HashSet"], vec![("T", e.describe())]),
        Ty::Map(k, v) => (vec!["HashMap"], vec![("K", k.describe()), ("V", v.describe())]),
        Ty::Iter { elem, .. } => (vec!["Iterator"], vec![("T", elem.describe())]),
        Ty::Range(e, _) => (vec!["Range", "Iterator"], vec![("T", e.describe())]),
        Ty::Int(_) => (vec!["int"], vec![("Self", t.describe())]),
        Ty::IntLit => (vec!["int"], vec![("Self", "i32".into())]),
        Ty::Float { .. } => (vec!["float"], vec![("Self", t.describe())]),
        Ty::Char => (vec!["char"], vec![("Self", "char".into())]),
        Ty::Bool => (vec!["bool"], vec![("Self", "bool".into())]),
        _ => return None,
    })
}

fn elem(view: &Ty) -> String {
    match view {
        Ty::Vec(e) | Ty::Set(e) => e.describe(),
        other => other.describe(),
    }
}

fn kv(view: &Ty) -> Vec<(&'static str, String)> {
    match view {
        Ty::Map(k, v) => vec![("K", k.describe()), ("V", v.describe())],
        _ => vec![],
    }
}

/// How the receiver is named under a hover of a method of `owner` (`[i32]`, `Option<i32>`).
pub fn receiver(owner: &str, args: &[(&str, String)]) -> String {
    let shown = match owner {
        "slice" => "[T]",
        "Vec" => "Vec<T>",
        "VecDeque" => "VecDeque<T>",
        "BinaryHeap" => "BinaryHeap<T>",
        "BTreeMap" => "BTreeMap<K, V>",
        "HashMap" => "HashMap<K, V>",
        "BTreeSet" => "BTreeSet<T>",
        "HashSet" => "HashSet<T>",
        "Option" => "Option<T>",
        "Result" => "Result<T, E>",
        "Iterator" | "Itertools" => "impl Iterator<Item = T>",
        "Range" => "Range<T>",
        "int" | "float" | "char" | "bool" => "Self",
        other => other,
    };
    substitute(shown, args)
}

/// `text` with each type parameter in `args` replaced by its argument (whole identifiers only).
pub fn substitute(text: &str, args: &[(&str, String)]) -> String {
    if args.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len() && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &text[start..i];
            match args.iter().find(|(p, _)| *p == word) {
                Some((_, a)) => out.push_str(a),
                None => out.push_str(word),
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// The method `name` of a value of type `t`: `(owner, signature, summary)`, with the owner's
/// type parameters filled in from `t`.
pub fn show(t: &Ty, name: &str) -> Option<(String, String, &'static str)> {
    let (owners, args) = owners(t)?;
    for owner in owners {
        if let Some(d) = lookup(owner, name) {
            return Some((
                receiver(owner, &args),
                substitute(d.sig, &args),
                d.doc,
            ));
        }
    }
    None
}

/// An associated function or constant of a std type, named by path (`String::from`,
/// `Vec::with_capacity`, `i32::MAX`): `(owner, signature, summary)`.
pub fn show_assoc(type_name: &str, name: &str) -> Option<(String, String, &'static str)> {
    let (owner, args): (&str, Vec<(&str, String)>) = match type_name {
        "String" | "str" => ("String", vec![]),
        "Vec" => ("Vec", vec![]),
        "VecDeque" => ("VecDeque", vec![]),
        "BinaryHeap" => ("BinaryHeap", vec![]),
        "HashMap" => ("HashMap", vec![]),
        "HashSet" => ("HashSet", vec![]),
        "BTreeMap" => ("BTreeMap", vec![]),
        "BTreeSet" => ("BTreeSet", vec![]),
        "Option" => ("Option", vec![]),
        "char" => ("char", vec![("Self", "char".into())]),
        "f32" | "f64" => ("float", vec![("Self", type_name.into())]),
        n if crate::ty::IntKind::from_name(n).is_some() => ("int", vec![("Self", n.into())]),
        _ => return None,
    };
    let d = lookup(owner, name)?;
    Some((receiver(owner, &args), substitute(d.sig, &args), d.doc))
}

/// The parameters of a signature from [`show`], without `self`, as signature help lists them.
pub fn params(sig: &str) -> Vec<String> {
    let Some(open) = sig.find('(') else {
        return vec![];
    };
    // (`->` is an arrow, not a closing `>`.)
    let arrow = |i: usize| i > 0 && sig.as_bytes()[i - 1] == b'-';
    let mut depth = 0usize;
    let mut close = sig.len();
    for (i, c) in sig[open..].char_indices() {
        match c {
            '(' | '<' | '[' => depth += 1,
            '>' if arrow(open + i) => {}
            ')' | '>' | ']' => {
                depth -= 1;
                if depth == 0 {
                    close = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let inner = &sig[open + 1..close];
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '(' | '<' | '[' => depth += 1,
            '>' if arrow(open + 1 + i) => {}
            ')' | '>' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        out.push(last.to_string());
    }
    out.retain(|p| !matches!(p.as_str(), "self" | "&self" | "&mut self" | "mut self"));
    out
}

pub fn lookup(owner: &str, name: &str) -> Option<&'static StdDoc> {
    DOCS.iter().find(|d| d.owner == owner && d.name == name)
}


macro_rules! docs {
    ($($owner:literal { $($name:literal => $sig:literal, $doc:literal;)* })*) => {
        &[$($(StdDoc { owner: $owner, name: $name, sig: $sig, doc: $doc },)*)*]
    };
}

pub static DOCS: &[StdDoc] = docs! {
    "Vec" {
        "new" => "fn new() -> Vec<T>", "Constructs a new, empty `Vec<T>`.";
        "with_capacity" => "fn with_capacity(capacity: usize) -> Vec<T>", "Constructs a new, empty `Vec<T>` with at least the specified capacity.";
        "from" => "fn from(value: impl Into<Vec<T>>) -> Vec<T>", "Converts an array, slice or string into a `Vec`.";
        "push" => "fn push(&mut self, value: T)", "Appends an element to the back of a collection.";
        "pop" => "fn pop(&mut self) -> Option<T>", "Removes the last element from a vector and returns it, or `None` if it is empty.";
        "insert" => "fn insert(&mut self, index: usize, element: T)", "Inserts an element at position `index` within the vector, shifting all elements after it to the right.";
        "remove" => "fn remove(&mut self, index: usize) -> T", "Removes and returns the element at position `index` within the vector, shifting all elements after it to the left.";
        "swap_remove" => "fn swap_remove(&mut self, index: usize) -> T", "Removes an element from the vector and returns it, replacing it with the last element.";
        "clear" => "fn clear(&mut self)", "Clears the vector, removing all values.";
        "truncate" => "fn truncate(&mut self, len: usize)", "Shortens the vector, keeping the first `len` elements and dropping the rest.";
        "resize" => "fn resize(&mut self, new_len: usize, value: T)", "Resizes the `Vec` in-place so that `len` is equal to `new_len`.";
        "append" => "fn append(&mut self, other: &mut Vec<T>)", "Moves all the elements of `other` into `self`, leaving `other` empty.";
        "extend" => "fn extend(&mut self, iter: impl IntoIterator<Item = T>)", "Extends a collection with the contents of an iterator.";
        "extend_from_slice" => "fn extend_from_slice(&mut self, other: &[T])", "Clones and appends all elements in a slice to the `Vec`.";
        "drain" => "fn drain(&mut self, range: impl RangeBounds<usize>) -> impl Iterator<Item = T>", "Removes the specified range from the vector in bulk, returning all removed elements as an iterator.";
        "retain" => "fn retain(&mut self, f: impl FnMut(&T) -> bool)", "Retains only the elements specified by the predicate.";
        "retain_mut" => "fn retain_mut(&mut self, f: impl FnMut(&mut T) -> bool)", "Retains only the elements specified by the predicate, passing a mutable reference to it.";
        "dedup" => "fn dedup(&mut self)", "Removes consecutive repeated elements in the vector according to the `PartialEq` trait implementation.";
        "dedup_by_key" => "fn dedup_by_key<K: PartialEq>(&mut self, key: impl FnMut(&mut T) -> K)", "Removes all but the first of consecutive elements in the vector that resolve to the same key.";
        "dedup_by" => "fn dedup_by(&mut self, same_bucket: impl FnMut(&mut T, &mut T) -> bool)", "Removes all but the first of consecutive elements in the vector satisfying a given equality relation.";
        "split_off" => "fn split_off(&mut self, at: usize) -> Vec<T>", "Splits the collection into two at the given index.";
        "splice" => "fn splice(&mut self, range: impl RangeBounds<usize>, replace_with: impl IntoIterator<Item = T>) -> impl Iterator<Item = T>", "Creates a splicing iterator that replaces the specified range in the vector with the given `replace_with` iterator and yields the removed items.";
        "as_slice" => "fn as_slice(&self) -> &[T]", "Extracts a slice containing the entire vector.";
        "as_mut_slice" => "fn as_mut_slice(&mut self) -> &mut [T]", "Extracts a mutable slice of the entire vector.";
        "into_iter" => "fn into_iter(self) -> impl Iterator<Item = T>", "Creates a consuming iterator, that is, one that moves each value out of the vector (from start to end).";
        "into_boxed_slice" => "fn into_boxed_slice(self) -> Box<[T]>", "Converts the vector into `Box<[T]>`.";
        "capacity" => "fn capacity(&self) -> usize", "Returns the total number of elements the vector can hold without reallocating.";
        "reserve" => "fn reserve(&mut self, additional: usize)", "Reserves capacity for at least `additional` more elements to be inserted in the given `Vec<T>`.";
        "reserve_exact" => "fn reserve_exact(&mut self, additional: usize)", "Reserves the minimum capacity for at least `additional` more elements to be inserted in the given `Vec<T>`.";
        "shrink_to_fit" => "fn shrink_to_fit(&mut self)", "Shrinks the capacity of the vector as much as possible.";
        "clone" => "fn clone(&self) -> Vec<T>", "Returns a copy of the value.";
    }
    "slice" {
        "len" => "fn len(&self) -> usize", "Returns the number of elements in the slice.";
        "is_empty" => "fn is_empty(&self) -> bool", "Returns `true` if the slice has a length of 0.";
        "first" => "fn first(&self) -> Option<&T>", "Returns the first element of the slice, or `None` if it is empty.";
        "first_mut" => "fn first_mut(&mut self) -> Option<&mut T>", "Returns a mutable reference to the first element of the slice, or `None` if it is empty.";
        "last" => "fn last(&self) -> Option<&T>", "Returns the last element of the slice, or `None` if it is empty.";
        "last_mut" => "fn last_mut(&mut self) -> Option<&mut T>", "Returns a mutable reference to the last item in the slice, or `None` if it is empty.";
        "split_first" => "fn split_first(&self) -> Option<(&T, &[T])>", "Returns the first and all the rest of the elements of the slice, or `None` if it is empty.";
        "split_last" => "fn split_last(&self) -> Option<(&T, &[T])>", "Returns the last and all the rest of the elements of the slice, or `None` if it is empty.";
        "get" => "fn get(&self, index: usize) -> Option<&T>", "Returns a reference to an element or subslice depending on the type of index.";
        "get_mut" => "fn get_mut(&mut self, index: usize) -> Option<&mut T>", "Returns a mutable reference to an element or subslice depending on the type of index.";
        "iter" => "fn iter(&self) -> impl Iterator<Item = &T>", "Returns an iterator over the slice.";
        "iter_mut" => "fn iter_mut(&mut self) -> impl Iterator<Item = &mut T>", "Returns an iterator that allows modifying each value.";
        "contains" => "fn contains(&self, x: &T) -> bool", "Returns `true` if the slice contains an element with the given value.";
        "starts_with" => "fn starts_with(&self, needle: &[T]) -> bool", "Returns `true` if `needle` is a prefix of the slice or equal to the slice.";
        "ends_with" => "fn ends_with(&self, needle: &[T]) -> bool", "Returns `true` if `needle` is a suffix of the slice or equal to the slice.";
        "swap" => "fn swap(&mut self, a: usize, b: usize)", "Swaps two elements in the slice.";
        "reverse" => "fn reverse(&mut self)", "Reverses the order of elements in the slice, in place.";
        "fill" => "fn fill(&mut self, value: T)", "Fills `self` with elements by cloning `value`.";
        "sort" => "fn sort(&mut self)", "Sorts the slice, preserving initial order of equal elements.";
        "sort_by" => "fn sort_by(&mut self, compare: impl FnMut(&T, &T) -> Ordering)", "Sorts the slice with a comparison function, preserving initial order of equal elements.";
        "sort_by_key" => "fn sort_by_key<K: Ord>(&mut self, f: impl FnMut(&T) -> K)", "Sorts the slice with a key extraction function, preserving initial order of equal elements.";
        "sort_unstable" => "fn sort_unstable(&mut self)", "Sorts the slice without preserving the initial order of equal elements.";
        "sort_unstable_by" => "fn sort_unstable_by(&mut self, compare: impl FnMut(&T, &T) -> Ordering)", "Sorts the slice with a comparison function, without preserving the initial order of equal elements.";
        "sort_unstable_by_key" => "fn sort_unstable_by_key<K: Ord>(&mut self, f: impl FnMut(&T) -> K)", "Sorts the slice with a key extraction function, without preserving the initial order of equal elements.";
        "is_sorted" => "fn is_sorted(&self) -> bool", "Checks if the elements of this slice are sorted.";
        "is_sorted_by" => "fn is_sorted_by(&self, compare: impl FnMut(&T, &T) -> bool) -> bool", "Checks if the elements of this slice are sorted using the given comparator function.";
        "is_sorted_by_key" => "fn is_sorted_by_key<K: PartialOrd>(&self, f: impl FnMut(&T) -> K) -> bool", "Checks if the elements of this slice are sorted using the given key extraction function.";
        "binary_search" => "fn binary_search(&self, x: &T) -> Result<usize, usize>", "Binary searches this slice for a given element.";
        "binary_search_by" => "fn binary_search_by(&self, f: impl FnMut(&T) -> Ordering) -> Result<usize, usize>", "Binary searches this slice with a comparator function.";
        "binary_search_by_key" => "fn binary_search_by_key<B: Ord>(&self, b: &B, f: impl FnMut(&T) -> B) -> Result<usize, usize>", "Binary searches this slice with a key extraction function.";
        "rotate_left" => "fn rotate_left(&mut self, mid: usize)", "Rotates the slice in-place such that the first `mid` elements of the slice move to the end while the last `self.len() - mid` elements move to the front.";
        "rotate_right" => "fn rotate_right(&mut self, k: usize)", "Rotates the slice in-place such that the first `self.len() - k` elements of the slice move to the end while the last `k` elements move to the front.";
        "split_at" => "fn split_at(&self, mid: usize) -> (&[T], &[T])", "Divides one slice into two at an index.";
        "split_at_mut" => "fn split_at_mut(&mut self, mid: usize) -> (&mut [T], &mut [T])", "Divides one mutable slice into two at an index.";
        "windows" => "fn windows(&self, size: usize) -> impl Iterator<Item = &[T]>", "Returns an iterator over all contiguous windows of length `size`.";
        "chunks" => "fn chunks(&self, chunk_size: usize) -> impl Iterator<Item = &[T]>", "Returns an iterator over `chunk_size` elements of the slice at a time, starting at the beginning of the slice.";
        "chunks_mut" => "fn chunks_mut(&mut self, chunk_size: usize) -> impl Iterator<Item = &mut [T]>", "Returns an iterator over `chunk_size` elements of the slice at a time, starting at the beginning of the slice.";
        "chunks_exact" => "fn chunks_exact(&self, chunk_size: usize) -> impl Iterator<Item = &[T]>", "Returns an iterator over `chunk_size` elements of the slice at a time, starting at the beginning of the slice, skipping a remainder.";
        "chunks_exact_mut" => "fn chunks_exact_mut(&mut self, chunk_size: usize) -> impl Iterator<Item = &mut [T]>", "Returns an iterator over `chunk_size` elements of the slice at a time, starting at the beginning of the slice, skipping a remainder.";
        "concat" => "fn concat(&self) -> Vec<T::Item>", "Flattens a slice of `T` into a single value `Vec<T::Item>`.";
        "join" => "fn join(&self, sep: &str) -> String", "Flattens a slice of `T` into a single value, placing a given separator between each.";
        "repeat" => "fn repeat(&self, n: usize) -> Vec<T>", "Creates a vector by copying a slice `n` times.";
        "to_vec" => "fn to_vec(&self) -> Vec<T>", "Copies `self` into a new `Vec`.";
        "copy_within" => "fn copy_within(&mut self, src: impl RangeBounds<usize>, dest: usize)", "Copies elements from one part of the slice to another part of itself.";
        "copy_from_slice" => "fn copy_from_slice(&mut self, src: &[T])", "Copies all elements from `src` into `self`, using a memcpy.";
        "clone_from_slice" => "fn clone_from_slice(&mut self, src: &[T])", "Copies the elements from `src` into `self`.";
        "map" => "fn map<U>(self, f: impl FnMut(T) -> U) -> [U; N]", "Returns an array of the same size as `self`, with function `f` applied to each element in order.";
        "eq_ignore_ascii_case" => "fn eq_ignore_ascii_case(&self, other: &[u8]) -> bool", "Checks that two slices are an ASCII case-insensitive match.";
        "strip_prefix" => "fn strip_prefix(&self, prefix: &[T]) -> Option<&[T]>", "Returns a subslice with the prefix removed.";
        "strip_suffix" => "fn strip_suffix(&self, suffix: &[T]) -> Option<&[T]>", "Returns a subslice with the suffix removed.";
        "max" => "fn max(self, other: Self) -> Self", "Compares and returns the maximum of two values.";
        "min" => "fn min(self, other: Self) -> Self", "Compares and returns the minimum of two values.";
    }
    "VecDeque" {
        "new" => "fn new() -> VecDeque<T>", "Creates an empty deque.";
        "with_capacity" => "fn with_capacity(capacity: usize) -> VecDeque<T>", "Creates an empty deque with space for at least `capacity` elements.";
        "from" => "fn from(value: Vec<T>) -> VecDeque<T>", "Turns a `Vec<T>` into a `VecDeque<T>`.";
        "push_back" => "fn push_back(&mut self, value: T)", "Appends an element to the back of the deque.";
        "push_front" => "fn push_front(&mut self, value: T)", "Prepends an element to the deque.";
        "pop_back" => "fn pop_back(&mut self) -> Option<T>", "Removes the last element from the deque and returns it, or `None` if it is empty.";
        "pop_front" => "fn pop_front(&mut self) -> Option<T>", "Removes the first element and returns it, or `None` if the deque is empty.";
        "front" => "fn front(&self) -> Option<&T>", "Provides a reference to the front element, or `None` if the deque is empty.";
        "front_mut" => "fn front_mut(&mut self) -> Option<&mut T>", "Provides a mutable reference to the front element, or `None` if the deque is empty.";
        "back" => "fn back(&self) -> Option<&T>", "Provides a reference to the back element, or `None` if the deque is empty.";
        "back_mut" => "fn back_mut(&mut self) -> Option<&mut T>", "Provides a mutable reference to the back element, or `None` if the deque is empty.";
        "get" => "fn get(&self, index: usize) -> Option<&T>", "Provides a reference to the element at the given index.";
        "get_mut" => "fn get_mut(&mut self, index: usize) -> Option<&mut T>", "Provides a mutable reference to the element at the given index.";
        "insert" => "fn insert(&mut self, index: usize, value: T)", "Inserts an element at `index` within the deque, shifting all elements with indices greater than or equal to `index` towards the back.";
        "remove" => "fn remove(&mut self, index: usize) -> Option<T>", "Removes and returns the element at `index` from the deque.";
        "clear" => "fn clear(&mut self)", "Clears the deque, removing all values.";
        "truncate" => "fn truncate(&mut self, len: usize)", "Shortens the deque, keeping the first `len` elements and dropping the rest.";
        "append" => "fn append(&mut self, other: &mut VecDeque<T>)", "Moves all the elements of `other` into `self`, leaving `other` empty.";
        "extend" => "fn extend(&mut self, iter: impl IntoIterator<Item = T>)", "Extends a collection with the contents of an iterator.";
        "drain" => "fn drain(&mut self, range: impl RangeBounds<usize>) -> impl Iterator<Item = T>", "Removes the specified range from the deque in bulk, returning all removed elements as an iterator.";
        "retain" => "fn retain(&mut self, f: impl FnMut(&T) -> bool)", "Retains only the elements specified by the predicate.";
        "make_contiguous" => "fn make_contiguous(&mut self) -> &mut [T]", "Rearranges the internal storage of this deque so it is one contiguous slice, which is then returned.";
        "into_iter" => "fn into_iter(self) -> impl Iterator<Item = T>", "Consumes the deque into a front-to-back iterator yielding elements by value.";
        "clone" => "fn clone(&self) -> VecDeque<T>", "Returns a copy of the value.";
    }
    "BinaryHeap" {
        "new" => "fn new() -> BinaryHeap<T>", "Creates an empty `BinaryHeap` as a max-heap.";
        "with_capacity" => "fn with_capacity(capacity: usize) -> BinaryHeap<T>", "Creates an empty `BinaryHeap` with at least the specified capacity.";
        "from" => "fn from(value: Vec<T>) -> BinaryHeap<T>", "Converts a `Vec<T>` into a `BinaryHeap<T>`.";
        "push" => "fn push(&mut self, item: T)", "Pushes an item onto the binary heap.";
        "pop" => "fn pop(&mut self) -> Option<T>", "Removes the greatest item from the binary heap and returns it, or `None` if it is empty.";
        "peek" => "fn peek(&self) -> Option<&T>", "Returns the greatest item in the binary heap, or `None` if it is empty.";
        "len" => "fn len(&self) -> usize", "Returns the length of the binary heap.";
        "is_empty" => "fn is_empty(&self) -> bool", "Checks if the binary heap is empty.";
        "clear" => "fn clear(&mut self)", "Drops all items from the binary heap.";
        "append" => "fn append(&mut self, other: &mut BinaryHeap<T>)", "Moves all the elements of `other` into `self`, leaving `other` empty.";
        "extend" => "fn extend(&mut self, iter: impl IntoIterator<Item = T>)", "Extends a collection with the contents of an iterator.";
        "drain" => "fn drain(&mut self) -> impl Iterator<Item = T>", "Clears the binary heap, returning an iterator over the removed elements in arbitrary order.";
        "iter" => "fn iter(&self) -> impl Iterator<Item = &T>", "Returns an iterator visiting all values in the underlying vector, in arbitrary order.";
        "into_iter" => "fn into_iter(self) -> impl Iterator<Item = T>", "Creates a consuming iterator, that is, one that moves each value out of the binary heap in arbitrary order.";
        "into_sorted_vec" => "fn into_sorted_vec(self) -> Vec<T>", "Consumes the `BinaryHeap` and returns a vector in sorted (ascending) order.";
        "into_vec" => "fn into_vec(self) -> Vec<T>", "Consumes the `BinaryHeap` and returns the underlying vector in arbitrary order.";
        "clone" => "fn clone(&self) -> BinaryHeap<T>", "Returns a copy of the value.";
    }
    "HashMap" {
        "new" => "fn new() -> HashMap<K, V>", "Creates an empty `HashMap`.";
        "with_capacity" => "fn with_capacity(capacity: usize) -> HashMap<K, V>", "Creates an empty `HashMap` with at least the specified capacity.";
        "from" => "fn from(arr: [(K, V); N]) -> HashMap<K, V>", "Converts an array of key-value pairs into a `HashMap`.";
        "insert" => "fn insert(&mut self, k: K, v: V) -> Option<V>", "Inserts a key-value pair into the map.";
        "get" => "fn get(&self, k: &K) -> Option<&V>", "Returns a reference to the value corresponding to the key.";
        "get_mut" => "fn get_mut(&mut self, k: &K) -> Option<&mut V>", "Returns a mutable reference to the value corresponding to the key.";
        "get_key_value" => "fn get_key_value(&self, k: &K) -> Option<(&K, &V)>", "Returns the key-value pair corresponding to the supplied key.";
        "contains_key" => "fn contains_key(&self, k: &K) -> bool", "Returns `true` if the map contains a value for the specified key.";
        "remove" => "fn remove(&mut self, k: &K) -> Option<V>", "Removes a key from the map, returning the value at the key if the key was previously in the map.";
        "remove_entry" => "fn remove_entry(&mut self, k: &K) -> Option<(K, V)>", "Removes a key from the map, returning the stored key and value if the key was previously in the map.";
        "entry" => "fn entry(&mut self, key: K) -> Entry<'_, K, V>", "Gets the given key's corresponding entry in the map for in-place manipulation.";
        "len" => "fn len(&self) -> usize", "Returns the number of elements in the map.";
        "is_empty" => "fn is_empty(&self) -> bool", "Returns `true` if the map contains no elements.";
        "clear" => "fn clear(&mut self)", "Clears the map, removing all key-value pairs.";
        "keys" => "fn keys(&self) -> impl Iterator<Item = &K>", "An iterator visiting all keys in arbitrary order.";
        "values" => "fn values(&self) -> impl Iterator<Item = &V>", "An iterator visiting all values in arbitrary order.";
        "values_mut" => "fn values_mut(&mut self) -> impl Iterator<Item = &mut V>", "An iterator visiting all values mutably in arbitrary order.";
        "into_keys" => "fn into_keys(self) -> impl Iterator<Item = K>", "Creates a consuming iterator visiting all the keys in arbitrary order.";
        "into_values" => "fn into_values(self) -> impl Iterator<Item = V>", "Creates a consuming iterator visiting all the values in arbitrary order.";
        "iter" => "fn iter(&self) -> impl Iterator<Item = (&K, &V)>", "An iterator visiting all key-value pairs in arbitrary order.";
        "iter_mut" => "fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)>", "An iterator visiting all key-value pairs in arbitrary order, with mutable references to the values.";
        "into_iter" => "fn into_iter(self) -> impl Iterator<Item = (K, V)>", "Creates a consuming iterator, that is, one that moves each key-value pair out of the map in arbitrary order.";
        "drain" => "fn drain(&mut self) -> impl Iterator<Item = (K, V)>", "Clears the map, returning all key-value pairs as an iterator.";
        "extend" => "fn extend(&mut self, iter: impl IntoIterator<Item = (K, V)>)", "Extends a collection with the contents of an iterator.";
        "retain" => "fn retain(&mut self, f: impl FnMut(&K, &mut V) -> bool)", "Retains only the elements specified by the predicate.";
        "clone" => "fn clone(&self) -> HashMap<K, V>", "Returns a copy of the value.";
    }
    "BTreeMap" {
        "new" => "fn new() -> BTreeMap<K, V>", "Makes a new, empty `BTreeMap`.";
        "from" => "fn from(arr: [(K, V); N]) -> BTreeMap<K, V>", "Converts a `[(K, V); N]` into a `BTreeMap<K, V>`.";
        "first_key_value" => "fn first_key_value(&self) -> Option<(&K, &V)>", "Returns the first key-value pair in the map.";
        "last_key_value" => "fn last_key_value(&self) -> Option<(&K, &V)>", "Returns the last key-value pair in the map.";
        "pop_first" => "fn pop_first(&mut self) -> Option<(K, V)>", "Removes and returns the first element in the map.";
        "pop_last" => "fn pop_last(&mut self) -> Option<(K, V)>", "Removes and returns the last element in the map.";
        "range" => "fn range(&self, range: impl RangeBounds<K>) -> impl Iterator<Item = (&K, &V)>", "Constructs a double-ended iterator over a sub-range of elements in the map.";
        "append" => "fn append(&mut self, other: &mut BTreeMap<K, V>)", "Moves all elements from `other` into `self`, leaving `other` empty.";
        "keys" => "fn keys(&self) -> impl Iterator<Item = &K>", "Gets an iterator over the keys of the map, in sorted order.";
        "values" => "fn values(&self) -> impl Iterator<Item = &V>", "Gets an iterator over the values of the map, in order by key.";
        "iter" => "fn iter(&self) -> impl Iterator<Item = (&K, &V)>", "Gets an iterator over the entries of the map, sorted by key.";
        "clone" => "fn clone(&self) -> BTreeMap<K, V>", "Returns a copy of the value.";
    }
    "HashSet" {
        "new" => "fn new() -> HashSet<T>", "Creates an empty `HashSet`.";
        "with_capacity" => "fn with_capacity(capacity: usize) -> HashSet<T>", "Creates an empty `HashSet` with at least the specified capacity.";
        "from" => "fn from(arr: [T; N]) -> HashSet<T>", "Converts an array into a `HashSet`.";
        "insert" => "fn insert(&mut self, value: T) -> bool", "Adds a value to the set.";
        "contains" => "fn contains(&self, value: &T) -> bool", "Returns `true` if the set contains a value.";
        "remove" => "fn remove(&mut self, value: &T) -> bool", "Removes a value from the set.";
        "take" => "fn take(&mut self, value: &T) -> Option<T>", "Removes and returns the value in the set, if any, that is equal to the given one.";
        "get" => "fn get(&self, value: &T) -> Option<&T>", "Returns a reference to the value in the set, if any, that is equal to the given value.";
        "len" => "fn len(&self) -> usize", "Returns the number of elements in the set.";
        "is_empty" => "fn is_empty(&self) -> bool", "Returns `true` if the set contains no elements.";
        "clear" => "fn clear(&mut self)", "Clears the set, removing all values.";
        "iter" => "fn iter(&self) -> impl Iterator<Item = &T>", "An iterator visiting all elements in arbitrary order.";
        "into_iter" => "fn into_iter(self) -> impl Iterator<Item = T>", "Creates a consuming iterator, that is, one that moves each value out of the set in arbitrary order.";
        "drain" => "fn drain(&mut self) -> impl Iterator<Item = T>", "Clears the set, returning all elements as an iterator.";
        "extend" => "fn extend(&mut self, iter: impl IntoIterator<Item = T>)", "Extends a collection with the contents of an iterator.";
        "retain" => "fn retain(&mut self, f: impl FnMut(&T) -> bool)", "Retains only the elements specified by the predicate.";
        "union" => "fn union(&self, other: &HashSet<T>) -> impl Iterator<Item = &T>", "Visits the values representing the union, i.e., all the values in `self` or `other`, without duplicates.";
        "intersection" => "fn intersection(&self, other: &HashSet<T>) -> impl Iterator<Item = &T>", "Visits the values representing the intersection, i.e., the values that are both in `self` and `other`.";
        "difference" => "fn difference(&self, other: &HashSet<T>) -> impl Iterator<Item = &T>", "Visits the values representing the difference, i.e., the values that are in `self` but not in `other`.";
        "symmetric_difference" => "fn symmetric_difference(&self, other: &HashSet<T>) -> impl Iterator<Item = &T>", "Visits the values representing the symmetric difference, i.e., the values that are in `self` or in `other` but not in both.";
        "is_subset" => "fn is_subset(&self, other: &HashSet<T>) -> bool", "Returns `true` if the set is a subset of another, i.e., `other` contains at least all the values in `self`.";
        "is_superset" => "fn is_superset(&self, other: &HashSet<T>) -> bool", "Returns `true` if the set is a superset of another, i.e., `self` contains at least all the values in `other`.";
        "is_disjoint" => "fn is_disjoint(&self, other: &HashSet<T>) -> bool", "Returns `true` if `self` has no elements in common with `other`.";
        "clone" => "fn clone(&self) -> HashSet<T>", "Returns a copy of the value.";
    }
    "BTreeSet" {
        "new" => "fn new() -> BTreeSet<T>", "Makes a new, empty `BTreeSet`.";
        "from" => "fn from(arr: [T; N]) -> BTreeSet<T>", "Converts a `[T; N]` into a `BTreeSet<T>`.";
        "first" => "fn first(&self) -> Option<&T>", "Returns a reference to the first element in the set, if any.";
        "last" => "fn last(&self) -> Option<&T>", "Returns a reference to the last element in the set, if any.";
        "pop_first" => "fn pop_first(&mut self) -> Option<T>", "Removes the first element from the set and returns it, if any.";
        "pop_last" => "fn pop_last(&mut self) -> Option<T>", "Removes the last element from the set and returns it, if any.";
        "range" => "fn range(&self, range: impl RangeBounds<T>) -> impl Iterator<Item = &T>", "Constructs a double-ended iterator over a sub-range of elements in the set.";
        "append" => "fn append(&mut self, other: &mut BTreeSet<T>)", "Moves all elements from `other` into `self`, leaving `other` empty.";
        "iter" => "fn iter(&self) -> impl Iterator<Item = &T>", "Gets an iterator that visits the elements in the `BTreeSet` in ascending order.";
        "union" => "fn union(&self, other: &BTreeSet<T>) -> impl Iterator<Item = &T>", "Visits the elements representing the union, i.e., all the elements in `self` or `other`, without duplicates, in ascending order.";
        "intersection" => "fn intersection(&self, other: &BTreeSet<T>) -> impl Iterator<Item = &T>", "Visits the elements representing the intersection, i.e., the elements that are both in `self` and `other`, in ascending order.";
        "difference" => "fn difference(&self, other: &BTreeSet<T>) -> impl Iterator<Item = &T>", "Visits the elements representing the difference, i.e., the elements that are in `self` but not in `other`, in ascending order.";
        "symmetric_difference" => "fn symmetric_difference(&self, other: &BTreeSet<T>) -> impl Iterator<Item = &T>", "Visits the elements representing the symmetric difference, i.e., the elements that are in `self` or in `other` but not in both, in ascending order.";
        "clone" => "fn clone(&self) -> BTreeSet<T>", "Returns a copy of the value.";
    }
    "String" {
        "new" => "fn new() -> String", "Creates a new empty `String`.";
        "from" => "fn from(s: &str) -> String", "Converts a `&str` into a `String`.";
        "with_capacity" => "fn with_capacity(capacity: usize) -> String", "Creates a new empty `String` with at least the specified capacity.";
        "from_utf8" => "fn from_utf8(vec: Vec<u8>) -> Result<String, FromUtf8Error>", "Converts a vector of bytes to a `String`.";
        "from_utf8_lossy" => "fn from_utf8_lossy(v: &[u8]) -> Cow<'_, str>", "Converts a slice of bytes to a string, including invalid characters.";
        "push" => "fn push(&mut self, ch: char)", "Appends the given `char` to the end of this `String`.";
        "push_str" => "fn push_str(&mut self, string: &str)", "Appends a given string slice onto the end of this `String`.";
        "pop" => "fn pop(&mut self) -> Option<char>", "Removes the last character from the string buffer and returns it.";
        "insert" => "fn insert(&mut self, idx: usize, ch: char)", "Inserts a character into this `String` at a byte position.";
        "insert_str" => "fn insert_str(&mut self, idx: usize, string: &str)", "Inserts a string slice into this `String` at a byte position.";
        "remove" => "fn remove(&mut self, idx: usize) -> char", "Removes a `char` from this `String` at a byte position and returns it.";
        "clear" => "fn clear(&mut self)", "Truncates this `String`, removing all contents.";
        "truncate" => "fn truncate(&mut self, new_len: usize)", "Shortens this `String` to the specified length.";
        "retain" => "fn retain(&mut self, f: impl FnMut(char) -> bool)", "Retains only the characters specified by the predicate.";
        "drain" => "fn drain(&mut self, range: impl RangeBounds<usize>) -> impl Iterator<Item = char>", "Removes the specified range from the string in bulk, returning all removed characters as an iterator.";
        "as_str" => "fn as_str(&self) -> &str", "Extracts a string slice containing the entire `String`.";
        "as_mut_str" => "fn as_mut_str(&mut self) -> &mut str", "Converts a `String` into a mutable string slice.";
        "into_bytes" => "fn into_bytes(self) -> Vec<u8>", "Converts a `String` into a byte vector.";
        "into_boxed_str" => "fn into_boxed_str(self) -> Box<str>", "Converts this `String` into a `Box<str>`.";
        "capacity" => "fn capacity(&self) -> usize", "Returns this `String`'s capacity, in bytes.";
        "reserve" => "fn reserve(&mut self, additional: usize)", "Reserves capacity for at least `additional` bytes more than the current length.";
        "shrink_to_fit" => "fn shrink_to_fit(&mut self)", "Shrinks the capacity of this `String` to match its length.";
        "write_str" => "fn write_str(&mut self, s: &str) -> fmt::Result", "Writes a string slice into this writer, returning whether the write succeeded.";
        "write_char" => "fn write_char(&mut self, c: char) -> fmt::Result", "Writes a `char` into this writer, returning whether the write succeeded.";
        "clone" => "fn clone(&self) -> String", "Returns a copy of the value.";
    }
    "str" {
        "len" => "fn len(&self) -> usize", "Returns the length of `self`, in bytes, not `char`s or graphemes.";
        "is_empty" => "fn is_empty(&self) -> bool", "Returns `true` if `self` has a length of zero bytes.";
        "chars" => "fn chars(&self) -> impl Iterator<Item = char>", "Returns an iterator over the `char`s of a string slice.";
        "char_indices" => "fn char_indices(&self) -> impl Iterator<Item = (usize, char)>", "Returns an iterator over the `char`s of a string slice, and their positions.";
        "bytes" => "fn bytes(&self) -> impl Iterator<Item = u8>", "Returns an iterator over the bytes of a string slice.";
        "as_bytes" => "fn as_bytes(&self) -> &[u8]", "Converts a string slice to a byte slice.";
        "lines" => "fn lines(&self) -> impl Iterator<Item = &str>", "Returns an iterator over the lines of a string, as string slices.";
        "split" => "fn split(&self, pat: impl Pattern) -> impl Iterator<Item = &str>", "Returns an iterator over substrings of this string slice, separated by characters matched by a pattern.";
        "rsplit" => "fn rsplit(&self, pat: impl Pattern) -> impl Iterator<Item = &str>", "Returns an iterator over substrings of the given string slice, separated by characters matched by a pattern and yielded in reverse order.";
        "splitn" => "fn splitn(&self, n: usize, pat: impl Pattern) -> impl Iterator<Item = &str>", "Returns an iterator over substrings of the given string slice, separated by a pattern, restricted to returning at most `n` items.";
        "rsplitn" => "fn rsplitn(&self, n: usize, pat: impl Pattern) -> impl Iterator<Item = &str>", "Returns an iterator over substrings of this string slice, separated by a pattern, starting from the end of the string, restricted to returning at most `n` items.";
        "split_once" => "fn split_once(&self, delimiter: impl Pattern) -> Option<(&str, &str)>", "Splits the string on the first occurrence of the specified delimiter and returns prefix before delimiter and suffix after delimiter.";
        "rsplit_once" => "fn rsplit_once(&self, delimiter: impl Pattern) -> Option<(&str, &str)>", "Splits the string on the last occurrence of the specified delimiter and returns prefix before delimiter and suffix after delimiter.";
        "split_whitespace" => "fn split_whitespace(&self) -> impl Iterator<Item = &str>", "Splits a string slice by whitespace.";
        "split_ascii_whitespace" => "fn split_ascii_whitespace(&self) -> impl Iterator<Item = &str>", "Splits a string slice by ASCII whitespace.";
        "split_terminator" => "fn split_terminator(&self, pat: impl Pattern) -> impl Iterator<Item = &str>", "Returns an iterator over substrings of the given string slice, separated by characters matched by a pattern, with a trailing empty substring skipped.";
        "split_at" => "fn split_at(&self, mid: usize) -> (&str, &str)", "Divides one string slice into two at an index.";
        "split_inclusive" => "fn split_inclusive(&self, pat: impl Pattern) -> impl Iterator<Item = &str>", "Returns an iterator over substrings of this string slice, separated by characters matched by a pattern, with the matched part kept at the end of each substring.";
        "matches" => "fn matches(&self, pat: impl Pattern) -> impl Iterator<Item = &str>", "Returns an iterator over the disjoint matches of a pattern within the given string slice.";
        "match_indices" => "fn match_indices(&self, pat: impl Pattern) -> impl Iterator<Item = (usize, &str)>", "Returns an iterator over the disjoint matches of a pattern within this string slice as well as the index that the match starts at.";
        "contains" => "fn contains(&self, pat: impl Pattern) -> bool", "Returns `true` if the given pattern matches a sub-slice of this string slice.";
        "starts_with" => "fn starts_with(&self, pat: impl Pattern) -> bool", "Returns `true` if the given pattern matches a prefix of this string slice.";
        "ends_with" => "fn ends_with(&self, pat: impl Pattern) -> bool", "Returns `true` if the given pattern matches a suffix of this string slice.";
        "find" => "fn find(&self, pat: impl Pattern) -> Option<usize>", "Returns the byte index of the first character of this string slice that matches the pattern.";
        "rfind" => "fn rfind(&self, pat: impl Pattern) -> Option<usize>", "Returns the byte index for the first character of the last match of the pattern in this string slice.";
        "strip_prefix" => "fn strip_prefix(&self, prefix: impl Pattern) -> Option<&str>", "Returns a string slice with the prefix removed.";
        "strip_suffix" => "fn strip_suffix(&self, suffix: impl Pattern) -> Option<&str>", "Returns a string slice with the suffix removed.";
        "trim" => "fn trim(&self) -> &str", "Returns a string slice with leading and trailing whitespace removed.";
        "trim_start" => "fn trim_start(&self) -> &str", "Returns a string slice with leading whitespace removed.";
        "trim_end" => "fn trim_end(&self) -> &str", "Returns a string slice with trailing whitespace removed.";
        "trim_matches" => "fn trim_matches(&self, pat: impl Pattern) -> &str", "Returns a string slice with all prefixes and suffixes that match a pattern repeatedly removed.";
        "trim_start_matches" => "fn trim_start_matches(&self, pat: impl Pattern) -> &str", "Returns a string slice with all prefixes that match a pattern repeatedly removed.";
        "trim_end_matches" => "fn trim_end_matches(&self, pat: impl Pattern) -> &str", "Returns a string slice with all suffixes that match a pattern repeatedly removed.";
        "to_lowercase" => "fn to_lowercase(&self) -> String", "Returns the lowercase equivalent of this string slice, as a new `String`.";
        "to_uppercase" => "fn to_uppercase(&self) -> String", "Returns the uppercase equivalent of this string slice, as a new `String`.";
        "to_ascii_lowercase" => "fn to_ascii_lowercase(&self) -> String", "Returns a copy of this string where each character is mapped to its ASCII lower case equivalent.";
        "to_ascii_uppercase" => "fn to_ascii_uppercase(&self) -> String", "Returns a copy of this string where each character is mapped to its ASCII upper case equivalent.";
        "make_ascii_lowercase" => "fn make_ascii_lowercase(&mut self)", "Converts this string to its ASCII lower case equivalent in-place.";
        "make_ascii_uppercase" => "fn make_ascii_uppercase(&mut self)", "Converts this string to its ASCII upper case equivalent in-place.";
        "eq_ignore_ascii_case" => "fn eq_ignore_ascii_case(&self, other: &str) -> bool", "Checks that two strings are an ASCII case-insensitive match.";
        "is_ascii" => "fn is_ascii(&self) -> bool", "Checks if all characters in this string are within the ASCII range.";
        "to_owned" => "fn to_owned(&self) -> String", "Creates owned data from borrowed data, usually by cloning.";
        "to_string" => "fn to_string(&self) -> String", "Converts the given value to a `String`.";
        "repeat" => "fn repeat(&self, n: usize) -> String", "Creates a new `String` by repeating a string `n` times.";
        "replace" => "fn replace(&self, from: impl Pattern, to: &str) -> String", "Replaces all matches of a pattern with another string.";
        "replacen" => "fn replacen(&self, pat: impl Pattern, to: &str, count: usize) -> String", "Replaces first N matches of a pattern with another string.";
        "parse" => "fn parse<F: FromStr>(&self) -> Result<F, F::Err>", "Parses this string slice into another type.";
        "get" => "fn get(&self, range: impl RangeBounds<usize>) -> Option<&str>", "Returns a subslice of `str`.";
        "is_char_boundary" => "fn is_char_boundary(&self, index: usize) -> bool", "Checks that `index`-th byte is the first byte in a UTF-8 code point sequence or the end of the string.";
        "char_at" => "fn char_at(&self, i: usize) -> char", "The character at a position.";
        "cmp" => "fn cmp(&self, other: &str) -> Ordering", "This method returns an `Ordering` between `self` and `other`.";
        "eq" => "fn eq(&self, other: &str) -> bool", "Tests for `self` and `other` values to be equal.";
    }
    "Option" {
        "is_some" => "fn is_some(&self) -> bool", "Returns `true` if the option is a `Some` value.";
        "is_some_and" => "fn is_some_and(self, f: impl FnOnce(T) -> bool) -> bool", "Returns `true` if the option is a `Some` and the value inside of it matches a predicate.";
        "is_none" => "fn is_none(&self) -> bool", "Returns `true` if the option is a `None` value.";
        "is_none_or" => "fn is_none_or(self, f: impl FnOnce(T) -> bool) -> bool", "Returns `true` if the option is a `None` or the value inside of it matches a predicate.";
        "as_ref" => "fn as_ref(&self) -> Option<&T>", "Converts from `&Option<T>` to `Option<&T>`.";
        "as_mut" => "fn as_mut(&mut self) -> Option<&mut T>", "Converts from `&mut Option<T>` to `Option<&mut T>`.";
        "as_deref" => "fn as_deref(&self) -> Option<&T::Target>", "Converts from `Option<T>` (or `&Option<T>`) to `Option<&T::Target>`.";
        "expect" => "fn expect(self, msg: &str) -> T", "Returns the contained `Some` value, consuming the `self` value.";
        "unwrap" => "fn unwrap(self) -> T", "Returns the contained `Some` value, consuming the `self` value.";
        "unwrap_or" => "fn unwrap_or(self, default: T) -> T", "Returns the contained `Some` value or a provided default.";
        "unwrap_or_else" => "fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T", "Returns the contained `Some` value or computes it from a closure.";
        "unwrap_or_default" => "fn unwrap_or_default(self) -> T", "Returns the contained `Some` value or a default.";
        "map" => "fn map<U>(self, f: impl FnOnce(T) -> U) -> Option<U>", "Maps an `Option<T>` to `Option<U>` by applying a function to a contained value (if `Some`) or returns `None` (if `None`).";
        "inspect" => "fn inspect(self, f: impl FnOnce(&T)) -> Option<T>", "Calls a function with a reference to the contained value if `Some`.";
        "map_or" => "fn map_or<U>(self, default: U, f: impl FnOnce(T) -> U) -> U", "Returns the provided default result (if none), or applies a function to the contained value (if any).";
        "map_or_else" => "fn map_or_else<U>(self, default: impl FnOnce() -> U, f: impl FnOnce(T) -> U) -> U", "Computes a default function result (if none), or applies a different function to the contained value (if any).";
        "ok_or" => "fn ok_or<E>(self, err: E) -> Result<T, E>", "Transforms the `Option<T>` into a `Result<T, E>`, mapping `Some(v)` to `Ok(v)` and `None` to `Err(err)`.";
        "ok_or_else" => "fn ok_or_else<E>(self, err: impl FnOnce() -> E) -> Result<T, E>", "Transforms the `Option<T>` into a `Result<T, E>`, mapping `Some(v)` to `Ok(v)` and `None` to `Err(err())`.";
        "and" => "fn and<U>(self, optb: Option<U>) -> Option<U>", "Returns `None` if the option is `None`, otherwise returns `optb`.";
        "and_then" => "fn and_then<U>(self, f: impl FnOnce(T) -> Option<U>) -> Option<U>", "Returns `None` if the option is `None`, otherwise calls `f` with the wrapped value and returns the result.";
        "filter" => "fn filter(self, predicate: impl FnOnce(&T) -> bool) -> Option<T>", "Returns `None` if the option is `None`, otherwise calls `predicate` with the wrapped value and returns `Some(t)` if it returns `true`.";
        "or" => "fn or(self, optb: Option<T>) -> Option<T>", "Returns the option if it contains a value, otherwise returns `optb`.";
        "or_else" => "fn or_else(self, f: impl FnOnce() -> Option<T>) -> Option<T>", "Returns the option if it contains a value, otherwise calls `f` and returns the result.";
        "xor" => "fn xor(self, optb: Option<T>) -> Option<T>", "Returns `Some` if exactly one of `self`, `optb` is `Some`, otherwise returns `None`.";
        "insert" => "fn insert(&mut self, value: T) -> &mut T", "Inserts `value` into the option, then returns a mutable reference to it.";
        "get_or_insert" => "fn get_or_insert(&mut self, value: T) -> &mut T", "Inserts `value` into the option if it is `None`, then returns a mutable reference to the contained value.";
        "get_or_insert_with" => "fn get_or_insert_with(&mut self, f: impl FnOnce() -> T) -> &mut T", "Inserts a value computed from `f` into the option if it is `None`, then returns a mutable reference to the contained value.";
        "get_or_insert_default" => "fn get_or_insert_default(&mut self) -> &mut T", "Inserts the default value into the option if it is `None`, then returns a mutable reference to the contained value.";
        "take" => "fn take(&mut self) -> Option<T>", "Takes the value out of the option, leaving a `None` in its place.";
        "take_if" => "fn take_if(&mut self, predicate: impl FnOnce(&mut T) -> bool) -> Option<T>", "Takes the value out of the option, but only if the predicate evaluates to `true` on a mutable reference to the value.";
        "replace" => "fn replace(&mut self, value: T) -> Option<T>", "Replaces the actual value in the option by the value given in parameter, returning the old value if present, leaving a `Some` in its place.";
        "zip" => "fn zip<U>(self, other: Option<U>) -> Option<(T, U)>", "Zips `self` with another `Option`.";
        "unzip" => "fn unzip(self) -> (Option<A>, Option<B>)", "Unzips an option containing a tuple of two options.";
        "copied" => "fn copied(self) -> Option<T>", "Maps an `Option<&T>` to an `Option<T>` by copying the contents of the option.";
        "cloned" => "fn cloned(self) -> Option<T>", "Maps an `Option<&T>` to an `Option<T>` by cloning the contents of the option.";
        "flatten" => "fn flatten(self) -> Option<T>", "Converts from `Option<Option<T>>` to `Option<T>`.";
        "iter" => "fn iter(&self) -> impl Iterator<Item = &T>", "Returns an iterator over the possibly contained value.";
        "into_iter" => "fn into_iter(self) -> impl Iterator<Item = T>", "Returns a consuming iterator over the possibly contained value.";
        "clone" => "fn clone(&self) -> Option<T>", "Returns a copy of the value.";
    }
    "Result" {
        "is_ok" => "fn is_ok(&self) -> bool", "Returns `true` if the result is `Ok`.";
        "is_ok_and" => "fn is_ok_and(self, f: impl FnOnce(T) -> bool) -> bool", "Returns `true` if the result is `Ok` and the value inside of it matches a predicate.";
        "is_err" => "fn is_err(&self) -> bool", "Returns `true` if the result is `Err`.";
        "is_err_and" => "fn is_err_and(self, f: impl FnOnce(E) -> bool) -> bool", "Returns `true` if the result is `Err` and the value inside of it matches a predicate.";
        "ok" => "fn ok(self) -> Option<T>", "Converts from `Result<T, E>` to `Option<T>`.";
        "err" => "fn err(self) -> Option<E>", "Converts from `Result<T, E>` to `Option<E>`.";
        "as_ref" => "fn as_ref(&self) -> Result<&T, &E>", "Converts from `&Result<T, E>` to `Result<&T, &E>`.";
        "as_mut" => "fn as_mut(&mut self) -> Result<&mut T, &mut E>", "Converts from `&mut Result<T, E>` to `Result<&mut T, &mut E>`.";
        "map" => "fn map<U>(self, op: impl FnOnce(T) -> U) -> Result<U, E>", "Maps a `Result<T, E>` to `Result<U, E>` by applying a function to a contained `Ok` value, leaving an `Err` value untouched.";
        "map_or" => "fn map_or<U>(self, default: U, f: impl FnOnce(T) -> U) -> U", "Returns the provided default (if `Err`), or applies a function to the contained value (if `Ok`).";
        "map_or_else" => "fn map_or_else<U>(self, default: impl FnOnce(E) -> U, f: impl FnOnce(T) -> U) -> U", "Maps a `Result<T, E>` to `U` by applying fallback function `default` to a contained `Err` value, or function `f` to a contained `Ok` value.";
        "map_err" => "fn map_err<F>(self, op: impl FnOnce(E) -> F) -> Result<T, F>", "Maps a `Result<T, E>` to `Result<T, F>` by applying a function to a contained `Err` value, leaving an `Ok` value untouched.";
        "inspect" => "fn inspect(self, f: impl FnOnce(&T)) -> Result<T, E>", "Calls a function with a reference to the contained value if `Ok`.";
        "inspect_err" => "fn inspect_err(self, f: impl FnOnce(&E)) -> Result<T, E>", "Calls a function with a reference to the contained value if `Err`.";
        "iter" => "fn iter(&self) -> impl Iterator<Item = &T>", "Returns an iterator over the possibly contained value.";
        "expect" => "fn expect(self, msg: &str) -> T", "Returns the contained `Ok` value, consuming the `self` value.";
        "unwrap" => "fn unwrap(self) -> T", "Returns the contained `Ok` value, consuming the `self` value.";
        "unwrap_or_default" => "fn unwrap_or_default(self) -> T", "Returns the contained `Ok` value or a default.";
        "expect_err" => "fn expect_err(self, msg: &str) -> E", "Returns the contained `Err` value, consuming the `self` value.";
        "unwrap_err" => "fn unwrap_err(self) -> E", "Returns the contained `Err` value, consuming the `self` value.";
        "and" => "fn and<U>(self, res: Result<U, E>) -> Result<U, E>", "Returns `res` if the result is `Ok`, otherwise returns the `Err` value of `self`.";
        "and_then" => "fn and_then<U>(self, op: impl FnOnce(T) -> Result<U, E>) -> Result<U, E>", "Calls `op` if the result is `Ok`, otherwise returns the `Err` value of `self`.";
        "or" => "fn or<F>(self, res: Result<T, F>) -> Result<T, F>", "Returns `res` if the result is `Err`, otherwise returns the `Ok` value of `self`.";
        "or_else" => "fn or_else<F>(self, op: impl FnOnce(E) -> Result<T, F>) -> Result<T, F>", "Calls `op` if the result is `Err`, otherwise returns the `Ok` value of `self`.";
        "unwrap_or" => "fn unwrap_or(self, default: T) -> T", "Returns the contained `Ok` value or a provided default.";
        "unwrap_or_else" => "fn unwrap_or_else(self, op: impl FnOnce(E) -> T) -> T", "Returns the contained `Ok` value or computes it from a closure.";
        "copied" => "fn copied(self) -> Result<T, E>", "Maps a `Result<&T, E>` to a `Result<T, E>` by copying the contents of the `Ok` part.";
        "cloned" => "fn cloned(self) -> Result<T, E>", "Maps a `Result<&T, E>` to a `Result<T, E>` by cloning the contents of the `Ok` part.";
        "clone" => "fn clone(&self) -> Result<T, E>", "Returns a copy of the value.";
    }
    "Iterator" {
        "next" => "fn next(&mut self) -> Option<T>", "Advances the iterator and returns the next value.";
        "size_hint" => "fn size_hint(&self) -> (usize, Option<usize>)", "Returns the bounds on the remaining length of the iterator.";
        "count" => "fn count(self) -> usize", "Consumes the iterator, counting the number of iterations and returning it.";
        "last" => "fn last(self) -> Option<T>", "Consumes the iterator, returning the last element.";
        "nth" => "fn nth(&mut self, n: usize) -> Option<T>", "Returns the `n`th element of the iterator.";
        "step_by" => "fn step_by(self, step: usize) -> impl Iterator<Item = T>", "Creates an iterator starting at the same point, but stepping by the given amount at each iteration.";
        "chain" => "fn chain(self, other: impl IntoIterator<Item = T>) -> impl Iterator<Item = T>", "Takes two iterators and creates a new iterator over both in sequence.";
        "zip" => "fn zip<U>(self, other: impl IntoIterator<Item = U>) -> impl Iterator<Item = (T, U)>", "'Zips up' two iterators into a single iterator of pairs.";
        "unzip" => "fn unzip<A, B>(self) -> (Vec<A>, Vec<B>)", "Converts an iterator of pairs into a pair of containers.";
        "map" => "fn map<B>(self, f: impl FnMut(T) -> B) -> impl Iterator<Item = B>", "Takes a closure and creates an iterator which calls that closure on each element.";
        "for_each" => "fn for_each(self, f: impl FnMut(T))", "Calls a closure on each element of an iterator.";
        "filter" => "fn filter(self, predicate: impl FnMut(&T) -> bool) -> impl Iterator<Item = T>", "Creates an iterator which uses a closure to determine if an element should be yielded.";
        "filter_map" => "fn filter_map<B>(self, f: impl FnMut(T) -> Option<B>) -> impl Iterator<Item = B>", "Creates an iterator that both filters and maps.";
        "enumerate" => "fn enumerate(self) -> impl Iterator<Item = (usize, T)>", "Creates an iterator which gives the current iteration count as well as the next value.";
        "peekable" => "fn peekable(self) -> Peekable<Self>", "Creates an iterator which can use the `peek` and `peek_mut` methods to look at the next element of the iterator without consuming it.";
        "skip_while" => "fn skip_while(self, predicate: impl FnMut(&T) -> bool) -> impl Iterator<Item = T>", "Creates an iterator that skips elements based on a predicate.";
        "take_while" => "fn take_while(self, predicate: impl FnMut(&T) -> bool) -> impl Iterator<Item = T>", "Creates an iterator that yields elements based on a predicate.";
        "map_while" => "fn map_while<B>(self, predicate: impl FnMut(T) -> Option<B>) -> impl Iterator<Item = B>", "Creates an iterator that both yields elements based on a predicate and maps.";
        "skip" => "fn skip(self, n: usize) -> impl Iterator<Item = T>", "Creates an iterator that skips the first `n` elements.";
        "take" => "fn take(self, n: usize) -> impl Iterator<Item = T>", "Creates an iterator that yields the first `n` elements, or fewer if the underlying iterator ends sooner.";
        "scan" => "fn scan<St, B>(self, initial_state: St, f: impl FnMut(&mut St, T) -> Option<B>) -> impl Iterator<Item = B>", "An iterator adapter which, like `fold`, holds internal state, but unlike `fold`, produces a new iterator.";
        "flat_map" => "fn flat_map<U: IntoIterator>(self, f: impl FnMut(T) -> U) -> impl Iterator<Item = U::Item>", "Creates an iterator that works like map, but flattens nested structure.";
        "flatten" => "fn flatten(self) -> impl Iterator<Item = T::Item>", "Creates an iterator that flattens nested structure.";
        "fuse" => "fn fuse(self) -> impl Iterator<Item = T>", "Creates an iterator which ends after the first `None`.";
        "inspect" => "fn inspect(self, f: impl FnMut(&T)) -> impl Iterator<Item = T>", "Does something with each element of an iterator, passing the value on.";
        "by_ref" => "fn by_ref(&mut self) -> &mut Self", "Creates a 'by reference' adapter for this instance of `Iterator`.";
        "collect" => "fn collect<B: FromIterator<T>>(self) -> B", "Transforms an iterator into a collection.";
        "partition" => "fn partition<B>(self, f: impl FnMut(&T) -> bool) -> (B, B)", "Consumes an iterator, creating two collections from it.";
        "fold" => "fn fold<B>(self, init: B, f: impl FnMut(B, T) -> B) -> B", "Folds every element into an accumulator by applying an operation, returning the final result.";
        "try_fold" => "fn try_fold<B, R>(&mut self, init: B, f: impl FnMut(B, T) -> R) -> R", "An iterator method that applies a function as long as it returns successfully, producing a single, final value.";
        "try_for_each" => "fn try_for_each<R>(&mut self, f: impl FnMut(T) -> R) -> R", "An iterator method that applies a fallible function to each item in the iterator, stopping at the first error and returning that error.";
        "reduce" => "fn reduce(self, f: impl FnMut(T, T) -> T) -> Option<T>", "Reduces the elements to a single one, by repeatedly applying a reducing operation.";
        "all" => "fn all(&mut self, f: impl FnMut(T) -> bool) -> bool", "Tests if every element of the iterator matches a predicate.";
        "any" => "fn any(&mut self, f: impl FnMut(T) -> bool) -> bool", "Tests if any element of the iterator matches a predicate.";
        "find" => "fn find(&mut self, predicate: impl FnMut(&T) -> bool) -> Option<T>", "Searches for an element of an iterator that satisfies a predicate.";
        "find_map" => "fn find_map<B>(&mut self, f: impl FnMut(T) -> Option<B>) -> Option<B>", "Applies function to the elements of iterator and returns the first non-none result.";
        "position" => "fn position(&mut self, predicate: impl FnMut(T) -> bool) -> Option<usize>", "Searches for an element in an iterator, returning its index.";
        "rposition" => "fn rposition(&mut self, predicate: impl FnMut(T) -> bool) -> Option<usize>", "Searches for an element in an iterator from the right, returning its index.";
        "max" => "fn max(self) -> Option<T>", "Returns the maximum element of an iterator.";
        "min" => "fn min(self) -> Option<T>", "Returns the minimum element of an iterator.";
        "max_by_key" => "fn max_by_key<B: Ord>(self, f: impl FnMut(&T) -> B) -> Option<T>", "Returns the element that gives the maximum value from the specified function.";
        "max_by" => "fn max_by(self, compare: impl FnMut(&T, &T) -> Ordering) -> Option<T>", "Returns the element that gives the maximum value with respect to the specified comparison function.";
        "min_by_key" => "fn min_by_key<B: Ord>(self, f: impl FnMut(&T) -> B) -> Option<T>", "Returns the element that gives the minimum value from the specified function.";
        "min_by" => "fn min_by(self, compare: impl FnMut(&T, &T) -> Ordering) -> Option<T>", "Returns the element that gives the minimum value with respect to the specified comparison function.";
        "rev" => "fn rev(self) -> impl Iterator<Item = T>", "Reverses an iterator's direction.";
        "copied" => "fn copied(self) -> impl Iterator<Item = T>", "Creates an iterator which copies all of its elements.";
        "cloned" => "fn cloned(self) -> impl Iterator<Item = T>", "Creates an iterator which clones all of its elements.";
        "cycle" => "fn cycle(self) -> impl Iterator<Item = T>", "Repeats an iterator endlessly.";
        "sum" => "fn sum<S: Sum<T>>(self) -> S", "Sums the elements of an iterator.";
        "product" => "fn product<P: Product<T>>(self) -> P", "Iterates over the entire iterator, multiplying all the elements.";
        "cmp" => "fn cmp(self, other: impl IntoIterator<Item = T>) -> Ordering", "Lexicographically compares the elements of this `Iterator` with those of another.";
        "eq" => "fn eq(self, other: impl IntoIterator<Item = T>) -> bool", "Determines if the elements of this `Iterator` are equal to those of another.";
        "lt" => "fn lt(self, other: impl IntoIterator<Item = T>) -> bool", "Determines if the elements of this `Iterator` are lexicographically less than those of another.";
        "is_sorted" => "fn is_sorted(self) -> bool", "Checks if the elements of this iterator are sorted.";
        "next_back" => "fn next_back(&mut self) -> Option<T>", "Removes and returns an element from the end of the iterator.";
        "nth_back" => "fn nth_back(&mut self, n: usize) -> Option<T>", "Returns the `n`th element from the end of the iterator.";
        "rfind" => "fn rfind(&mut self, predicate: impl FnMut(&T) -> bool) -> Option<T>", "Searches for an element of an iterator from the back that satisfies a predicate.";
        "rfold" => "fn rfold<B>(self, init: B, f: impl FnMut(B, T) -> B) -> B", "An iterator method that reduces the iterator's elements to a single, final value, starting from the back.";
        "len" => "fn len(&self) -> usize", "Returns the exact remaining length of the iterator.";
        "peek" => "fn peek(&mut self) -> Option<&T>", "Returns a reference to the `next()` value without advancing the iterator.";
        "next_if" => "fn next_if(&mut self, func: impl FnOnce(&T) -> bool) -> Option<T>", "Consume and return the next value of this iterator if a condition is true.";
        "next_if_eq" => "fn next_if_eq(&mut self, expected: &T) -> Option<T>", "Consume and return the next item if it is equal to `expected`.";
    }
    "Itertools" {
        "join" => "fn join(&mut self, sep: &str) -> String", "Combine all iterator elements into one `String`, separated by `sep`.";
        "format" => "fn format(self, sep: &str) -> Format<'_, Self>", "Format all iterator elements, separated by `sep`.";
        "collect_vec" => "fn collect_vec(self) -> Vec<T>", "`.collect_vec()` is simply a type specialization of `Iterator::collect`, for convenience.";
        "sorted" => "fn sorted(self) -> impl Iterator<Item = T>", "Sort all iterator elements into a new iterator in ascending order.";
        "sorted_unstable" => "fn sorted_unstable(self) -> impl Iterator<Item = T>", "Sort all iterator elements into a new iterator in ascending order, without preserving the order of equal elements.";
        "sorted_by" => "fn sorted_by(self, cmp: impl FnMut(&T, &T) -> Ordering) -> impl Iterator<Item = T>", "Sort all iterator elements into a new iterator in ascending order, by a comparison function.";
        "sorted_unstable_by" => "fn sorted_unstable_by(self, cmp: impl FnMut(&T, &T) -> Ordering) -> impl Iterator<Item = T>", "Sort all iterator elements into a new iterator in ascending order, by a comparison function, without preserving the order of equal elements.";
        "sorted_by_key" => "fn sorted_by_key<K: Ord>(self, f: impl FnMut(&T) -> K) -> impl Iterator<Item = T>", "Sort all iterator elements into a new iterator in ascending order, by a key.";
        "sorted_unstable_by_key" => "fn sorted_unstable_by_key<K: Ord>(self, f: impl FnMut(&T) -> K) -> impl Iterator<Item = T>", "Sort all iterator elements into a new iterator in ascending order, by a key, without preserving the order of equal elements.";
        "sorted_by_cached_key" => "fn sorted_by_cached_key<K: Ord>(self, f: impl FnMut(&T) -> K) -> impl Iterator<Item = T>", "Sort all iterator elements into a new iterator in ascending order, by a key computed once per element.";
        "unique" => "fn unique(self) -> impl Iterator<Item = T>", "Return an iterator adaptor that filters out elements that have already been produced once during the iteration.";
        "unique_by" => "fn unique_by<K: Eq + Hash>(self, f: impl FnMut(&T) -> K) -> impl Iterator<Item = T>", "Return an iterator adaptor that filters out elements that have already been produced once during the iteration, by a key.";
        "dedup" => "fn dedup(self) -> impl Iterator<Item = T>", "Remove duplicates from sections of consecutive identical elements.";
        "duplicates" => "fn duplicates(self) -> impl Iterator<Item = T>", "Return an iterator adaptor that produces elements that appear more than once during the iteration.";
        "tuple_windows" => "fn tuple_windows<U>(self) -> impl Iterator<Item = U>", "Return an iterator over all contiguous windows producing tuples of a specific size (up to 12).";
        "circular_tuple_windows" => "fn circular_tuple_windows<U>(self) -> impl Iterator<Item = U>", "Return an iterator over all windows, wrapping back to the first elements when the window would otherwise exceed the length of the iterator, producing tuples of a specific size.";
        "tuples" => "fn tuples<U>(self) -> impl Iterator<Item = U>", "Return an iterator that groups the items in tuples of a specific size (up to 12).";
        "collect_tuple" => "fn collect_tuple<U>(self) -> Option<U>", "Collects all items from the iterator into a tuple of a specific size (up to 12).";
        "next_tuple" => "fn next_tuple<U>(&mut self) -> Option<U>", "Advances the iterator and returns the next items grouped in a tuple of a specific size (up to 12).";
        "combinations" => "fn combinations(self, k: usize) -> impl Iterator<Item = Vec<T>>", "Return an iterator adaptor that iterates over the `k`-length combinations of the elements from an iterator.";
        "combinations_with_replacement" => "fn combinations_with_replacement(self, k: usize) -> impl Iterator<Item = Vec<T>>", "Return an iterator that iterates over the `k`-length combinations of the elements from an iterator, with replacement.";
        "permutations" => "fn permutations(self, k: usize) -> impl Iterator<Item = Vec<T>>", "Return an iterator adaptor that iterates over all `k`-permutations of the elements from an iterator.";
        "powerset" => "fn powerset(self) -> impl Iterator<Item = Vec<T>>", "Return an iterator that iterates through the powerset of the elements from an iterator.";
        "cartesian_product" => "fn cartesian_product<U>(self, other: impl IntoIterator<Item = U>) -> impl Iterator<Item = (T, U)>", "Return an iterator adaptor that iterates over the cartesian product of the element sets of two iterators.";
        "multi_cartesian_product" => "fn multi_cartesian_product(self) -> impl Iterator<Item = Vec<T::Item>>", "Return an iterator adaptor that iterates over the cartesian product of all subiterators returned by meta-iterator `self`.";
        "interleave" => "fn interleave(self, other: impl IntoIterator<Item = T>) -> impl Iterator<Item = T>", "Alternate elements from two iterators until both have run out.";
        "merge" => "fn merge(self, other: impl IntoIterator<Item = T>) -> impl Iterator<Item = T>", "Return an iterator adaptor that merges the two base iterators in ascending order.";
        "intersperse" => "fn intersperse(self, element: T) -> impl Iterator<Item = T>", "An iterator adaptor to insert a particular value between each element of the adapted iterator.";
        "zip_eq" => "fn zip_eq<U>(self, other: impl IntoIterator<Item = U>) -> impl Iterator<Item = (T, U)>", "Create an iterator which iterates over both this and the specified iterator simultaneously, yielding pairs, and panics if they have different lengths.";
        "set_from" => "fn set_from(&mut self, from: impl IntoIterator<Item = T>) -> usize", "Assign to each reference in `self` from the `from` iterator, stopping at the shortest of the two iterators.";
        "take_while_inclusive" => "fn take_while_inclusive(self, accept: impl FnMut(&T) -> bool) -> impl Iterator<Item = T>", "Returns an iterator adaptor that consumes elements while the given predicate is `true`, including the element for which the predicate first returned `false`.";
        "positions" => "fn positions(self, predicate: impl FnMut(T) -> bool) -> impl Iterator<Item = usize>", "Return an iterator adaptor that yields the indices of all elements satisfying a predicate, counted from the start of the iterator.";
        "dropping" => "fn dropping(self, n: usize) -> Self", "Consume the first `n` elements from the iterator eagerly, and return the same iterator again.";
        "k_smallest" => "fn k_smallest(self, k: usize) -> impl Iterator<Item = T>", "Sort the k smallest elements into a new iterator, in ascending order.";
        "k_largest" => "fn k_largest(self, k: usize) -> impl Iterator<Item = T>", "Sort the k largest elements into a new iterator, in descending order.";
        "counts" => "fn counts(self) -> HashMap<T, usize>", "Collect the items in this iterator and return a `HashMap` which contains each item that appears in the iterator and the number of times it appears.";
        "counts_by" => "fn counts_by<K: Eq + Hash>(self, f: impl FnMut(T) -> K) -> HashMap<K, usize>", "Collect the items in this iterator and return a `HashMap` which contains each distinct key from `f` and the number of times it appears.";
        "all_equal" => "fn all_equal(&mut self) -> bool", "Check whether all elements compare equal.";
        "all_unique" => "fn all_unique(&mut self) -> bool", "Check whether all elements are unique (non equal).";
        "contains" => "fn contains(&mut self, query: &T) -> bool", "Returns `true` if the given item is present in this iterator.";
        "find_position" => "fn find_position(&mut self, pred: impl FnMut(&T) -> bool) -> Option<(usize, T)>", "Find the position and value of the first element satisfying a predicate.";
        "chunk_by" => "fn chunk_by<K: PartialEq>(self, key: impl FnMut(&T) -> K) -> impl Iterator<Item = (K, Vec<T>)>", "Return an iterable that can chunk the iterator: consecutive elements that map to the same key are grouped.";
        "group_by" => "fn group_by<K: PartialEq>(self, key: impl FnMut(&T) -> K) -> impl Iterator<Item = (K, Vec<T>)>", "Return an iterable that can group iterator elements: consecutive elements that map to the same key are assigned to the same group.";
        "into_group_map" => "fn into_group_map<K: Eq + Hash, V>(self) -> HashMap<K, Vec<V>>", "Return a `HashMap` of keys mapped to `Vec`s of values, from an iterator of `(key, value)` pairs.";
        "into_group_map_by" => "fn into_group_map_by<K: Eq + Hash>(self, f: impl FnMut(&T) -> K) -> HashMap<K, Vec<T>>", "Return a `HashMap` keyed by the result of `f` for each element and holding a `Vec` of them.";
    }
    "Range" {
        "contains" => "fn contains(&self, item: &T) -> bool", "Returns `true` if `item` is contained in the range.";
        "is_empty" => "fn is_empty(&self) -> bool", "Returns `true` if the range contains no items.";
        "start" => "fn start(&self) -> &T", "Returns the lower bound of the range (inclusive).";
        "end" => "fn end(&self) -> &T", "Returns the upper bound of the range.";
    }
    "int" {
        "MAX" => "const MAX: Self", "The largest value that can be represented by this integer type.";
        "MIN" => "const MIN: Self", "The smallest value that can be represented by this integer type.";
        "BITS" => "const BITS: u32", "The size of this integer type in bits.";
        "from_str_radix" => "fn from_str_radix(src: &str, radix: u32) -> Result<Self, ParseIntError>", "Parses an integer from a string slice with digits in a given base.";
        "abs" => "fn abs(self) -> Self", "Computes the absolute value of `self`.";
        "abs_diff" => "fn abs_diff(self, other: Self) -> Self", "Computes the absolute difference between `self` and `other`.";
        "unsigned_abs" => "fn unsigned_abs(self) -> Self", "Computes the absolute value of `self` without any wrapping or panicking.";
        "signum" => "fn signum(self) -> Self", "Returns a number representing sign of `self`.";
        "is_positive" => "fn is_positive(self) -> bool", "Returns `true` if `self` is positive and `false` if the number is zero or negative.";
        "is_negative" => "fn is_negative(self) -> bool", "Returns `true` if `self` is negative and `false` if the number is zero or positive.";
        "pow" => "fn pow(self, exp: u32) -> Self", "Raises self to the power of `exp`, using exponentiation by squaring.";
        "checked_pow" => "fn checked_pow(self, exp: u32) -> Option<Self>", "Checked exponentiation. Computes `self.pow(exp)`, returning `None` if overflow occurred.";
        "isqrt" => "fn isqrt(self) -> Self", "Returns the square root of the number, rounded down.";
        "ilog" => "fn ilog(self, base: Self) -> u32", "Returns the logarithm of the number with respect to an arbitrary base, rounded down.";
        "ilog2" => "fn ilog2(self) -> u32", "Returns the base 2 logarithm of the number, rounded down.";
        "ilog10" => "fn ilog10(self) -> u32", "Returns the base 10 logarithm of the number, rounded down.";
        "checked_add" => "fn checked_add(self, rhs: Self) -> Option<Self>", "Checked integer addition. Computes `self + rhs`, returning `None` if overflow occurred.";
        "checked_sub" => "fn checked_sub(self, rhs: Self) -> Option<Self>", "Checked integer subtraction. Computes `self - rhs`, returning `None` if overflow occurred.";
        "checked_mul" => "fn checked_mul(self, rhs: Self) -> Option<Self>", "Checked integer multiplication. Computes `self * rhs`, returning `None` if overflow occurred.";
        "checked_div" => "fn checked_div(self, rhs: Self) -> Option<Self>", "Checked integer division. Computes `self / rhs`, returning `None` if `rhs == 0` or the division results in overflow.";
        "checked_rem" => "fn checked_rem(self, rhs: Self) -> Option<Self>", "Checked integer remainder. Computes `self % rhs`, returning `None` if `rhs == 0` or the division results in overflow.";
        "checked_neg" => "fn checked_neg(self) -> Option<Self>", "Checked negation. Computes `-self`, returning `None` if `self == MIN`.";
        "saturating_add" => "fn saturating_add(self, rhs: Self) -> Self", "Saturating integer addition. Computes `self + rhs`, saturating at the numeric bounds instead of overflowing.";
        "saturating_sub" => "fn saturating_sub(self, rhs: Self) -> Self", "Saturating integer subtraction. Computes `self - rhs`, saturating at the numeric bounds instead of overflowing.";
        "saturating_mul" => "fn saturating_mul(self, rhs: Self) -> Self", "Saturating integer multiplication. Computes `self * rhs`, saturating at the numeric bounds instead of overflowing.";
        "saturating_pow" => "fn saturating_pow(self, exp: u32) -> Self", "Saturating integer exponentiation. Computes `self.pow(exp)`, saturating at the numeric bounds instead of overflowing.";
        "wrapping_add" => "fn wrapping_add(self, rhs: Self) -> Self", "Wrapping (modular) addition. Computes `self + rhs`, wrapping around at the boundary of the type.";
        "wrapping_sub" => "fn wrapping_sub(self, rhs: Self) -> Self", "Wrapping (modular) subtraction. Computes `self - rhs`, wrapping around at the boundary of the type.";
        "wrapping_mul" => "fn wrapping_mul(self, rhs: Self) -> Self", "Wrapping (modular) multiplication. Computes `self * rhs`, wrapping around at the boundary of the type.";
        "wrapping_neg" => "fn wrapping_neg(self) -> Self", "Wrapping (modular) negation. Computes `-self`, wrapping around at the boundary of the type.";
        "wrapping_pow" => "fn wrapping_pow(self, exp: u32) -> Self", "Wrapping (modular) exponentiation. Computes `self.pow(exp)`, wrapping around at the boundary of the type.";
        "overflowing_add" => "fn overflowing_add(self, rhs: Self) -> (Self, bool)", "Calculates `self + rhs`, returning a tuple of the addition along with a boolean indicating whether an arithmetic overflow would occur.";
        "overflowing_sub" => "fn overflowing_sub(self, rhs: Self) -> (Self, bool)", "Calculates `self - rhs`, returning a tuple of the subtraction along with a boolean indicating whether an arithmetic overflow would occur.";
        "overflowing_mul" => "fn overflowing_mul(self, rhs: Self) -> (Self, bool)", "Calculates the multiplication of `self` and `rhs`, returning a tuple of the multiplication along with a boolean indicating whether an arithmetic overflow would occur.";
        "div_euclid" => "fn div_euclid(self, rhs: Self) -> Self", "Calculates the quotient of Euclidean division of `self` by `rhs`.";
        "rem_euclid" => "fn rem_euclid(self, rhs: Self) -> Self", "Calculates the least nonnegative remainder of `self (mod rhs)`.";
        "div_ceil" => "fn div_ceil(self, rhs: Self) -> Self", "Calculates the quotient of `self` and `rhs`, rounding the result towards positive infinity.";
        "next_multiple_of" => "fn next_multiple_of(self, rhs: Self) -> Self", "Calculates the smallest value greater than or equal to `self` that is a multiple of `rhs`.";
        "is_power_of_two" => "fn is_power_of_two(self) -> bool", "Returns `true` if and only if `self == 2^k` for some unsigned integer `k`.";
        "next_power_of_two" => "fn next_power_of_two(self) -> Self", "Returns the smallest power of two greater than or equal to `self`.";
        "count_ones" => "fn count_ones(self) -> u32", "Returns the number of ones in the binary representation of `self`.";
        "count_zeros" => "fn count_zeros(self) -> u32", "Returns the number of zeros in the binary representation of `self`.";
        "leading_zeros" => "fn leading_zeros(self) -> u32", "Returns the number of leading zeros in the binary representation of `self`.";
        "trailing_zeros" => "fn trailing_zeros(self) -> u32", "Returns the number of trailing zeros in the binary representation of `self`.";
        "leading_ones" => "fn leading_ones(self) -> u32", "Returns the number of leading ones in the binary representation of `self`.";
        "trailing_ones" => "fn trailing_ones(self) -> u32", "Returns the number of trailing ones in the binary representation of `self`.";
        "rotate_left" => "fn rotate_left(self, n: u32) -> Self", "Shifts the bits to the left by a specified amount, `n`, wrapping the truncated bits to the end of the resulting integer.";
        "rotate_right" => "fn rotate_right(self, n: u32) -> Self", "Shifts the bits to the right by a specified amount, `n`, wrapping the truncated bits to the beginning of the resulting integer.";
        "swap_bytes" => "fn swap_bytes(self) -> Self", "Reverses the byte order of the integer.";
        "reverse_bits" => "fn reverse_bits(self) -> Self", "Reverses the order of bits in the integer.";
        "to_le_bytes" => "fn to_le_bytes(self) -> [u8; N]", "Returns the memory representation of this integer as a byte array in little-endian byte order.";
        "to_be_bytes" => "fn to_be_bytes(self) -> [u8; N]", "Returns the memory representation of this integer as a byte array in big-endian (network) byte order.";
        "to_ne_bytes" => "fn to_ne_bytes(self) -> [u8; N]", "Returns the memory representation of this integer as a byte array in native byte order.";
        "from_le_bytes" => "fn from_le_bytes(bytes: [u8; N]) -> Self", "Creates an integer value from its representation as a byte array in little endian.";
        "from_be_bytes" => "fn from_be_bytes(bytes: [u8; N]) -> Self", "Creates an integer value from its representation as a byte array in big endian.";
        "min" => "fn min(self, other: Self) -> Self", "Compares and returns the minimum of two values.";
        "max" => "fn max(self, other: Self) -> Self", "Compares and returns the maximum of two values.";
        "clamp" => "fn clamp(self, min: Self, max: Self) -> Self", "Restrict a value to a certain interval.";
        "cmp" => "fn cmp(&self, other: &Self) -> Ordering", "This method returns an `Ordering` between `self` and `other`.";
        "partial_cmp" => "fn partial_cmp(&self, other: &Self) -> Option<Ordering>", "This method returns an ordering between `self` and `other` values if one exists.";
        "to_string" => "fn to_string(&self) -> String", "Converts the given value to a `String`.";
        "try_from" => "fn try_from(value: impl TryInto<Self>) -> Result<Self, TryFromIntError>", "Performs the conversion.";
        "try_into" => "fn try_into<U>(self) -> Result<U, TryFromIntError>", "Performs the conversion.";
        "from" => "fn from(value: impl Into<Self>) -> Self", "Converts to this type from the input type.";
        "checked_ilog2" => "fn checked_ilog2(self) -> Option<u32>", "Returns the base 2 logarithm of the number, rounded down, or `None` if the number is zero or negative.";
        "checked_ilog10" => "fn checked_ilog10(self) -> Option<u32>", "Returns the base 10 logarithm of the number, rounded down, or `None` if the number is zero or negative.";
    }
    "float" {
        "MAX" => "const MAX: Self", "Largest finite value of this floating-point type.";
        "MIN" => "const MIN: Self", "Smallest finite value of this floating-point type.";
        "MIN_POSITIVE" => "const MIN_POSITIVE: Self", "Smallest positive normal value of this floating-point type.";
        "EPSILON" => "const EPSILON: Self", "Machine epsilon value for this floating-point type.";
        "INFINITY" => "const INFINITY: Self", "Infinity (∞).";
        "NEG_INFINITY" => "const NEG_INFINITY: Self", "Negative infinity (−∞).";
        "NAN" => "const NAN: Self", "Not a Number (NaN).";
        "abs" => "fn abs(self) -> Self", "Computes the absolute value of `self`.";
        "signum" => "fn signum(self) -> Self", "Returns a number that represents the sign of `self`.";
        "copysign" => "fn copysign(self, sign: Self) -> Self", "Returns a number composed of the magnitude of `self` and the sign of `sign`.";
        "floor" => "fn floor(self) -> Self", "Returns the largest integer less than or equal to `self`.";
        "ceil" => "fn ceil(self) -> Self", "Returns the smallest integer greater than or equal to `self`.";
        "round" => "fn round(self) -> Self", "Returns the nearest integer to `self`. If a value is half-way between two integers, round away from `0.0`.";
        "trunc" => "fn trunc(self) -> Self", "Returns the integer part of `self`. This means that non-integer numbers are always truncated towards zero.";
        "fract" => "fn fract(self) -> Self", "Returns the fractional part of `self`.";
        "sqrt" => "fn sqrt(self) -> Self", "Returns the square root of a number.";
        "cbrt" => "fn cbrt(self) -> Self", "Returns the cube root of a number.";
        "powi" => "fn powi(self, n: i32) -> Self", "Raises a number to an integer power.";
        "powf" => "fn powf(self, n: Self) -> Self", "Raises a number to a floating point power.";
        "exp" => "fn exp(self) -> Self", "Returns `e^(self)`, (the exponential function).";
        "exp2" => "fn exp2(self) -> Self", "Returns `2^(self)`.";
        "ln" => "fn ln(self) -> Self", "Returns the natural logarithm of the number.";
        "log" => "fn log(self, base: Self) -> Self", "Returns the logarithm of the number with respect to an arbitrary base.";
        "log2" => "fn log2(self) -> Self", "Returns the base 2 logarithm of the number.";
        "log10" => "fn log10(self) -> Self", "Returns the base 10 logarithm of the number.";
        "hypot" => "fn hypot(self, other: Self) -> Self", "Compute the distance between the origin and a point (`x`, `y`) on the Euclidean plane.";
        "sin" => "fn sin(self) -> Self", "Computes the sine of a number (in radians).";
        "cos" => "fn cos(self) -> Self", "Computes the cosine of a number (in radians).";
        "tan" => "fn tan(self) -> Self", "Computes the tangent of a number (in radians).";
        "asin" => "fn asin(self) -> Self", "Computes the arcsine of a number.";
        "acos" => "fn acos(self) -> Self", "Computes the arccosine of a number.";
        "atan" => "fn atan(self) -> Self", "Computes the arctangent of a number.";
        "atan2" => "fn atan2(self, other: Self) -> Self", "Computes the four quadrant arctangent of `self` (`y`) and `other` (`x`) in radians.";
        "sinh" => "fn sinh(self) -> Self", "Hyperbolic sine function.";
        "cosh" => "fn cosh(self) -> Self", "Hyperbolic cosine function.";
        "tanh" => "fn tanh(self) -> Self", "Hyperbolic tangent function.";
        "sin_cos" => "fn sin_cos(self) -> (Self, Self)", "Simultaneously computes the sine and cosine of the number, `x`.";
        "to_degrees" => "fn to_degrees(self) -> Self", "Converts radians to degrees.";
        "to_radians" => "fn to_radians(self) -> Self", "Converts degrees to radians.";
        "mul_add" => "fn mul_add(self, a: Self, b: Self) -> Self", "Fused multiply-add. Computes `(self * a) + b` with only one rounding error.";
        "recip" => "fn recip(self) -> Self", "Takes the reciprocal (inverse) of a number, `1/x`.";
        "div_euclid" => "fn div_euclid(self, rhs: Self) -> Self", "Calculates Euclidean division, the matching method for `rem_euclid`.";
        "rem_euclid" => "fn rem_euclid(self, rhs: Self) -> Self", "Calculates the least nonnegative remainder of `self (mod rhs)`.";
        "is_nan" => "fn is_nan(self) -> bool", "Returns `true` if this value is NaN.";
        "is_finite" => "fn is_finite(self) -> bool", "Returns `true` if this number is neither infinite nor NaN.";
        "is_infinite" => "fn is_infinite(self) -> bool", "Returns `true` if this value is positive infinity or negative infinity, and `false` otherwise.";
        "is_normal" => "fn is_normal(self) -> bool", "Returns `true` if the number is neither zero, infinite, subnormal, or NaN.";
        "is_sign_positive" => "fn is_sign_positive(self) -> bool", "Returns `true` if `self` has a positive sign, including `+0.0`, NaNs with positive sign bit and positive infinity.";
        "is_sign_negative" => "fn is_sign_negative(self) -> bool", "Returns `true` if `self` has a negative sign, including `-0.0`, NaNs with negative sign bit and negative infinity.";
        "min" => "fn min(self, other: Self) -> Self", "Returns the minimum of the two numbers, ignoring NaN.";
        "max" => "fn max(self, other: Self) -> Self", "Returns the maximum of the two numbers, ignoring NaN.";
        "clamp" => "fn clamp(self, min: Self, max: Self) -> Self", "Restrict a value to a certain interval unless it is NaN.";
        "partial_cmp" => "fn partial_cmp(&self, other: &Self) -> Option<Ordering>", "This method returns an ordering between `self` and `other` values if one exists.";
        "total_cmp" => "fn total_cmp(&self, other: &Self) -> Ordering", "Returns the ordering between `self` and `other`.";
        "to_bits" => "fn to_bits(self) -> u64", "Raw transmutation to an integer of the same size.";
        "from_bits" => "fn from_bits(v: u64) -> Self", "Raw transmutation from an integer of the same size.";
        "to_string" => "fn to_string(&self) -> String", "Converts the given value to a `String`.";
        "from" => "fn from(value: impl Into<Self>) -> Self", "Converts to this type from the input type.";
    }
    "char" {
        "MAX" => "const MAX: char", "The highest valid code point a `char` can have, `'\\u{10FFFF}'`.";
        "from_u32" => "fn from_u32(i: u32) -> Option<char>", "Converts a `u32` to a `char`.";
        "from_digit" => "fn from_digit(num: u32, radix: u32) -> Option<char>", "Converts a digit in the given radix to a `char`.";
        "from" => "fn from(value: u8) -> char", "Converts a `u8` into a `char`.";
        "is_alphabetic" => "fn is_alphabetic(self) -> bool", "Returns `true` if this `char` has the `Alphabetic` property.";
        "is_alphanumeric" => "fn is_alphanumeric(self) -> bool", "Returns `true` if this `char` satisfies either `is_alphabetic()` or `is_numeric()`.";
        "is_numeric" => "fn is_numeric(self) -> bool", "Returns `true` if this `char` has one of the general categories for numbers.";
        "is_digit" => "fn is_digit(self, radix: u32) -> bool", "Checks if a `char` is a digit in the given radix.";
        "is_ascii_digit" => "fn is_ascii_digit(&self) -> bool", "Checks if the value is an ASCII decimal digit: U+0030 '0' ..= U+0039 '9'.";
        "is_ascii_hexdigit" => "fn is_ascii_hexdigit(&self) -> bool", "Checks if the value is an ASCII hexadecimal digit.";
        "is_ascii_alphabetic" => "fn is_ascii_alphabetic(&self) -> bool", "Checks if the value is an ASCII alphabetic character.";
        "is_ascii_alphanumeric" => "fn is_ascii_alphanumeric(&self) -> bool", "Checks if the value is an ASCII alphanumeric character.";
        "is_ascii_punctuation" => "fn is_ascii_punctuation(&self) -> bool", "Checks if the value is an ASCII punctuation character.";
        "is_ascii_whitespace" => "fn is_ascii_whitespace(&self) -> bool", "Checks if the value is an ASCII whitespace character.";
        "is_ascii_uppercase" => "fn is_ascii_uppercase(&self) -> bool", "Checks if the value is an ASCII uppercase character: U+0041 'A' ..= U+005A 'Z'.";
        "is_ascii_lowercase" => "fn is_ascii_lowercase(&self) -> bool", "Checks if the value is an ASCII lowercase character: U+0061 'a' ..= U+007A 'z'.";
        "is_ascii" => "fn is_ascii(&self) -> bool", "Checks if the value is within the ASCII range.";
        "is_whitespace" => "fn is_whitespace(self) -> bool", "Returns `true` if this `char` has the `White_Space` property.";
        "is_uppercase" => "fn is_uppercase(self) -> bool", "Returns `true` if this `char` has the `Uppercase` property.";
        "is_lowercase" => "fn is_lowercase(self) -> bool", "Returns `true` if this `char` has the `Lowercase` property.";
        "is_control" => "fn is_control(self) -> bool", "Returns `true` if this `char` has the general category for control codes.";
        "to_digit" => "fn to_digit(self, radix: u32) -> Option<u32>", "Converts a `char` to a digit in the given radix.";
        "to_ascii_uppercase" => "fn to_ascii_uppercase(&self) -> char", "Makes a copy of the value in its ASCII upper case equivalent.";
        "to_ascii_lowercase" => "fn to_ascii_lowercase(&self) -> char", "Makes a copy of the value in its ASCII lower case equivalent.";
        "to_uppercase" => "fn to_uppercase(self) -> impl Iterator<Item = char>", "Returns an iterator that yields the uppercase mapping of this `char` as one or more `char`s.";
        "to_lowercase" => "fn to_lowercase(self) -> impl Iterator<Item = char>", "Returns an iterator that yields the lowercase mapping of this `char` as one or more `char`s.";
        "eq_ignore_ascii_case" => "fn eq_ignore_ascii_case(&self, other: &char) -> bool", "Checks that two values are an ASCII case-insensitive match.";
        "len_utf8" => "fn len_utf8(self) -> usize", "Returns the number of bytes this `char` would need if encoded in UTF-8.";
        "encode_utf8" => "fn encode_utf8(self, dst: &mut [u8]) -> &mut str", "Encodes this character as UTF-8 into the provided byte buffer, and then returns the subslice of the buffer that contains the encoded character.";
        "escape_debug" => "fn escape_debug(self) -> impl Iterator<Item = char>", "Returns an iterator that yields the literal escape code of a character as `char`s.";
        "to_string" => "fn to_string(&self) -> String", "Converts the given value to a `String`.";
        "cmp" => "fn cmp(&self, other: &char) -> Ordering", "This method returns an `Ordering` between `self` and `other`.";
    }
    "bool" {
        "then" => "fn then<T>(self, f: impl FnOnce() -> T) -> Option<T>", "Returns `Some(f())` if the `bool` is `true`, or `None` otherwise.";
        "then_some" => "fn then_some<T>(self, t: T) -> Option<T>", "Returns `Some(t)` if the `bool` is `true`, or `None` otherwise.";
        "to_string" => "fn to_string(&self) -> String", "Converts the given value to a `String`.";
    }
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_whole_words() {
        let args = [("T", "i32".to_string())];
        assert_eq!(
            substitute("fn first(&self) -> Option<&T>", &args),
            "fn first(&self) -> Option<&i32>"
        );
        // `Target` and `FromIterator<T>` keep their own names.
        assert_eq!(
            substitute("fn collect<B: FromIterator<T>>(self) -> B", &args),
            "fn collect<B: FromIterator<i32>>(self) -> B"
        );
    }

    #[test]
    fn shows_a_vec_method_on_the_slice() {
        let (owner, sig, doc) = show(&Ty::Vec(Box::new(Ty::Int(crate::ty::IntKind::I32))), "first").unwrap();
        assert_eq!(owner, "[i32]");
        assert_eq!(sig, "fn first(&self) -> Option<&i32>");
        assert!(doc.starts_with("Returns the first element"));
        let (owner, sig, _) = show(&Ty::Vec(Box::new(Ty::Str)), "push").unwrap();
        assert_eq!(owner, "Vec<String>");
        assert_eq!(sig, "fn push(&mut self, value: String)");
    }

    #[test]
    fn lists_parameters_without_self() {
        assert_eq!(
            params("fn sort_by_key<K: Ord>(&mut self, f: impl FnMut(&T) -> K)"),
            vec!["f: impl FnMut(&T) -> K"]
        );
        assert_eq!(params("fn len(&self) -> usize"), Vec::<String>::new());
        assert_eq!(
            params("fn insert(&mut self, k: K, v: V) -> Option<V>"),
            vec!["k: K", "v: V"]
        );
    }

    #[test]
    fn no_duplicate_entries() {
        for (i, a) in DOCS.iter().enumerate() {
            for b in &DOCS[i + 1..] {
                assert!(
                    !(a.owner == b.owner && a.name == b.name),
                    "{}::{} twice",
                    a.owner,
                    a.name
                );
            }
        }
    }
}

#[cfg(test)]
mod coverage {
    use super::*;
    use crate::ty::{Coll, IntKind};

    /// Every std method the completions list has a signature and a summary here.
    #[test]
    fn every_listed_method_is_documented() {
        let i32_ = Box::new(Ty::Int(IntKind::I32));
        let types = vec![
            Ty::Coll(Coll::VecDeque, Box::new(Ty::Vec(i32_.clone()))),
            Ty::Coll(Coll::BinaryHeap, Box::new(Ty::Vec(i32_.clone()))),
            Ty::Coll(Coll::BTreeMap, Box::new(Ty::Map(Box::new(Ty::Str), i32_.clone()))),
            Ty::Coll(Coll::BTreeSet, Box::new(Ty::Set(i32_.clone()))),
            Ty::Str,
            Ty::Vec(i32_.clone()),
            Ty::Iter { elem: i32_.clone(), mutable: false },
            Ty::Option(i32_.clone()),
            Ty::Result(i32_.clone(), Box::new(Ty::Str)),
            Ty::Set(i32_.clone()),
            Ty::Map(Box::new(Ty::Str), i32_.clone()),
            Ty::Int(IntKind::I32),
            Ty::Float { f32: false },
            Ty::Char,
        ];
        let mut missing = Vec::new();
        for t in &types {
            for name in crate::service::std_methods(t) {
                if show(t, name).is_none() {
                    missing.push(format!("{}::{name}", t.describe()));
                }
            }
        }
        assert!(missing.is_empty(), "undocumented: {}", missing.join(", "));
    }
}
