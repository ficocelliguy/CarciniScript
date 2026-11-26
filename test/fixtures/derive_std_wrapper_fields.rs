// Derives over fields whose impl comes from std's wrapper, not the field's own type:
// `Rc<T>` / `Arc<T>` are `Clone` for any `T`, `Cow<str>` is `Eq`, `[T; N]` is `Copy` when `T` is.
use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug)]
struct Inner {
    n: u32,
}

#[derive(Debug, Clone)]
struct Handle {
    inner: Rc<Inner>,
}

#[derive(Debug, Clone)]
struct SharedHandle(Arc<Inner>);

#[derive(Debug, Clone)]
enum Node {
    Leaf(Rc<Inner>),
    Pair { left: Rc<Inner>, right: Arc<Inner> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Fragment<'a> {
    Literal(Cow<'a, str>),
    Arg(usize),
}

#[derive(Debug, Copy, Clone)]
struct ConstArrayBuilder<const N: usize, T> {
    full_array: [T; N],
    start: usize,
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct Pair<T>([T; 2]);

fn main() {
    let h = Handle { inner: Rc::new(Inner { n: 1 }) };
    let h2 = h.clone();
    println!("{} {}", h2.inner.n, Rc::ptr_eq(&h.inner, &h2.inner));

    let s = SharedHandle(Arc::new(Inner { n: 2 }));
    let s2 = s.clone();
    println!("{:?} {}", s2, Arc::ptr_eq(&s.0, &s2.0));

    let nodes = vec![
        Node::Leaf(Rc::new(Inner { n: 3 })),
        Node::Pair { left: Rc::new(Inner { n: 4 }), right: Arc::new(Inner { n: 5 }) },
    ];
    let copies = nodes.clone();
    println!("{:?}", copies);

    let a = Fragment::Literal(Cow::Borrowed("x"));
    let b = Fragment::Literal(Cow::Owned("x".to_string()));
    let c = Fragment::Arg(1);
    println!("{} {} {:?}", a == b, a == c, b.clone());

    let mut builder = ConstArrayBuilder { full_array: [1u8, 2, 3], start: 0 };
    let snapshot = builder;
    builder.full_array[0] = 9;
    builder.start = 1;
    println!("{:?} {:?}", builder, snapshot);

    let p = Pair([1.5, 2.5]);
    let mut q = p;
    q.0[1] = 0.0;
    println!("{:?} {:?} {}", p, q, p == q);
}
