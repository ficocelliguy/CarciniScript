// Generic structs and enums: type parameters on fields, `impl<T>` blocks (with bounds, `Self`,
// associated functions), inference from construction and from later use, explicit `Stack<i32>`
// annotations, generic enums with exhaustive matches, derives, and generics over user types.
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq)]
struct Stack<T> {
    items: Vec<T>,
}

impl<T: Clone> Stack<T> {
    fn new() -> Self {
        Stack { items: Vec::new() }
    }

    fn push(&mut self, x: T) {
        self.items.push(x);
    }

    fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    fn peek(&self) -> Option<T> {
        self.items.last().cloned()
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn map<U, F: Fn(&T) -> U>(&self, f: F) -> Stack<U> {
        Stack { items: self.items.iter().map(f).collect() }
    }

    fn fold<A>(&self, init: A, f: impl Fn(A, &T) -> A) -> A {
        let mut acc = init;
        for x in &self.items {
            acc = f(acc, x);
        }
        acc
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pair<A, B> {
    first: A,
    second: B,
}

impl<A: Display, B: Display> Pair<A, B> {
    fn show(&self) -> String {
        format!("({}, {})", self.first, self.second)
    }

    fn swap(self) -> Pair<B, A> {
        Pair { first: self.second, second: self.first }
    }
}

#[derive(Debug)]
enum Tree<T> {
    Leaf(T),
    Node(Vec<Tree<T>>),
}

fn count<T>(t: &Tree<T>) -> usize {
    match t {
        Tree::Leaf(_) => 1,
        Tree::Node(children) => children.iter().map(count).sum(),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Reply<T> {
    Yes(T),
    No,
}

fn ask(n: i32) -> Reply<String> {
    if n > 0 { Reply::Yes(format!("got {}", n)) } else { Reply::No }
}

fn main() {
    let mut s = Stack::new();
    s.push(3);
    s.push(9);
    let top = s.pop();
    println!("{:?} {:?} {} {:?}", top, s.peek(), s.len(), s);
    let mut words: Stack<String> = Stack::new();
    words.push("a".to_string());
    let copy = words.clone();
    words.push("b".to_string());
    println!("{:?} {:?} {}", words, copy, words == copy);
    let lens = words.map(|w| w.len() as f64 * 1.5);
    let joined = words.fold(String::new(), |acc, w| acc + w);
    println!("{:?} {} {}", lens, joined, s.fold(0, |a, x| a + x));

    let p = Pair { first: 1, second: "two" };
    let q = p.swap();
    println!("{} {} {:?}", p.show(), q.show(), q);
    let pts: Vec<Pair<i32, f64>> = vec![Pair { first: 1, second: 0.5 }, Pair { first: 2, second: 1.5 }];
    let total: f64 = pts.iter().map(|p| p.first as f64 * p.second).sum();
    println!("{}", total);

    let tree = Tree::Node(vec![Tree::Leaf('a'), Tree::Node(vec![Tree::Leaf('b'), Tree::Leaf('c')])]);
    println!("{} {:?}", count(&tree), tree);
    for n in [3, -1] {
        match ask(n) {
            Reply::Yes(s) => println!("yes: {}", s),
            Reply::No => println!("no"),
        }
    }
    let r: Reply<i32> = Reply::No;
    println!("{:?} {}", r, r == Reply::No);
}
