// Const generics through type aliases (with lifetimes and defaults), statics with const items
// as arguments, and tuple-struct constructors as functions.
const KEY: usize = 16;

struct Suite<const K: usize> {
    name: &'static str,
}

impl<const K: usize> Suite<K> {
    fn key_len(&self) -> usize {
        K
    }
}

static AES: &Suite<KEY> = &Suite { name: "aes" };

struct Prod<T, const INCL: bool> {
    data: Vec<T>,
}

type InclProd<T> = Prod<T, true>;

impl<T> InclProd<T> {
    fn new_incl(data: Vec<T>) -> Self {
        Prod { data }
    }
    fn incl(&self) -> bool {
        true
    }
}

impl<T, const INCL: bool> Prod<T, INCL> {
    fn is_incl(&self) -> bool {
        INCL
    }
}

struct Chunk<const N: usize>(Vec<u8>);

fn chunks<const N: usize>(v: Vec<Vec<u8>>) -> Vec<Chunk<N>> {
    v.into_iter().map(Chunk).collect()
}

trait Fissile<P> {
    fn length(&self) -> usize;
}

impl Fissile<char> for String {
    fn length(&self) -> usize {
        self.len()
    }
}

struct SplitProducer<'p, P, V, const INCL: bool = false> {
    data: V,
    separator: &'p P,
    tail: usize,
}

type SplitInclusiveProducer<'p, P, V> = SplitProducer<'p, P, V, true>;

impl<'p, P, V> SplitInclusiveProducer<'p, P, V>
where
    V: Fissile<P>,
{
    fn new_incl(data: V, separator: &'p P) -> Self {
        SplitProducer {
            tail: data.length(),
            data,
            separator,
        }
    }
}

impl<'p, P, V, const INCL: bool> SplitProducer<'p, P, V, INCL> {
    fn incl(&self) -> bool {
        INCL
    }
}

fn main() {
    println!("{} {}", AES.key_len(), AES.name);
    let p = InclProd::new_incl(vec![1, 2]);
    println!("{} {} {}", p.incl(), p.is_incl(), p.data.len());
    let c = chunks::<4>(vec![vec![1], vec![2]]);
    println!("{}", c.len());
    let s = String::from("a,b");
    let p = SplitInclusiveProducer::new_incl(s, &',');
    println!("{} {} {}", p.incl(), p.tail, p.separator);
}
