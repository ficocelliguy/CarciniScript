// A method's `where` clause bounding its impl's parameter with a generic trait, and the
// trait's associated type as the method's result (`-> F::Container`).
trait KeyMethod<K, I> {
    type Container;
    fn make(&mut self, item: I) -> Self::Container;
}

struct Meta<Key, F> {
    key_method: F,
    calls: usize,
    last: Option<Key>,
}

impl<Key, F> Meta<Key, F> {
    fn filter<I>(&mut self, item: I) -> F::Container
    where
        F: KeyMethod<Key, I>,
    {
        self.calls += 1;
        self.key_method.make(item)
    }
}

struct Dup;
impl KeyMethod<u8, i32> for Dup {
    type Container = Vec<i32>;
    fn make(&mut self, item: i32) -> Vec<i32> {
        vec![item, item]
    }
}

struct Count(usize);
impl KeyMethod<char, &str> for Count {
    type Container = (usize, String);
    fn make(&mut self, item: &str) -> (usize, String) {
        self.0 += item.len();
        (self.0, item.to_uppercase())
    }
}

fn main() {
    let mut m: Meta<u8, Dup> = Meta { key_method: Dup, calls: 0, last: None };
    let v = m.filter(4);
    println!("{:?} {}", v, v.len());
    let mut c: Meta<char, Count> = Meta { key_method: Count(0), calls: 0, last: Some('x') };
    c.filter("ab");
    let (n, s) = c.filter("cde");
    println!("{} {:?} {} {} {:?} {:?}", n, s, m.calls, c.calls, m.last, c.last);
}
