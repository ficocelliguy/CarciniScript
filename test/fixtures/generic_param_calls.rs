// Calls by path on a type parameter: `T::len(x)` / `T::name(x)` / `T::drive(t, v)` (a bound's
// method with `self`, also in an `impl<T: Tr>` and with a `&mut` cell argument), `A::size()`,
// `M::INFO.label(..)` (an associated const through a parameter), `S::default()` passed to a
// constructor, and `Trait::method(x, &mut n)` / `<T as Trait>::method(..)` with a cell argument.
trait Sized2 {
    fn size() -> usize;
    fn len(&self) -> usize;
    fn name(&self) -> String {
        format!("len {}", self.len())
    }
}

struct Pair;
struct Word(String);

impl Sized2 for Pair {
    fn size() -> usize {
        2
    }
    fn len(&self) -> usize {
        2
    }
}

impl Sized2 for Word {
    fn size() -> usize {
        1
    }
    fn len(&self) -> usize {
        self.0.len()
    }
}

fn total<A: Sized2>(xs: &[A]) -> usize {
    xs.iter().map(|x| A::len(x)).sum::<usize>() + A::size()
}

fn describe<T: Sized2>(x: &T) -> String {
    format!("{} / {}", T::name(x), <T as Sized2>::len(x))
}

trait Visitor {
    fn visit(&mut self, n: i32);
}

trait Drive {
    fn drive<V: Visitor>(&self, v: &mut V);
}

struct Sum(i32);
impl Visitor for Sum {
    fn visit(&mut self, n: i32) {
        self.0 += n;
    }
}

struct Nums(Vec<i32>);
impl Drive for Nums {
    fn drive<V: Visitor>(&self, v: &mut V) {
        for n in &self.0 {
            v.visit(*n);
        }
    }
}

fn run<T: Drive, V: Visitor>(t: &T, v: &mut V) {
    T::drive(t, v);
}

trait Info {
    const INFO: Meta;
    const SCALE: i32;
}

#[derive(Debug, Clone, Copy)]
struct Meta {
    id: u32,
}

impl Meta {
    fn label(&self, lang: &str) -> String {
        format!("{}-{}", lang, self.id)
    }
}

struct En;
impl Info for En {
    const INFO: Meta = Meta { id: 7 };
    const SCALE: i32 = 3;
}

fn localize<M: Info>(lang: &str) -> String {
    format!("{} x{}", M::INFO.label(lang), M::SCALE)
}

#[derive(Debug, Default)]
struct Store<S> {
    state: S,
    n: usize,
}

impl<S: Default> Store<S> {
    fn new_in(state: S) -> Self {
        Store { state, n: 1 }
    }
}

fn fresh<S: Default + std::fmt::Debug>() -> Store<S> {
    Store::new_in(S::default())
}

fn main() {
    println!("{}", total(&[Pair, Pair]));
    println!("{}", total(&[Word("abc".into()), Word("de".into())]));
    println!("{}", describe(&Word("hello".into())));
    let mut s = Sum(0);
    run(&Nums(vec![1, 2, 3]), &mut s);
    println!("{}", s.0);
    println!("{}", localize::<En>("en"));
    let st: Store<Vec<i32>> = fresh();
    println!("{:?}", st);
    let st2 = fresh::<String>();
    println!("{:?}", st2);
    main2();
}

trait Measure {
    fn len(&self) -> usize;
    fn add_to(&self, total: &mut usize) {
        *total += self.len();
    }
}

struct Token(String);
impl Measure for Token {
    fn len(&self) -> usize {
        self.0.len()
    }
}

struct Wrapper<T> {
    inner: T,
    extra: usize,
}

impl<T: Measure> Wrapper<T> {
    fn total(&self) -> usize {
        T::len(&self.inner) + self.extra
    }
    fn accumulate(&self, acc: &mut usize) {
        T::add_to(&self.inner, acc);
        Measure::add_to(&self.inner, acc);
    }
}

fn main2() {
    let w = Wrapper { inner: Token("abcd".into()), extra: 3 };
    println!("{}", w.total());
    let mut acc = 0;
    w.accumulate(&mut acc);
    println!("{acc}");
    let mut n = 10;
    Measure::add_to(&Token("xy".into()), &mut n);
    <Token as Measure>::add_to(&Token("xyz".into()), &mut n);
    println!("{n}");
}
