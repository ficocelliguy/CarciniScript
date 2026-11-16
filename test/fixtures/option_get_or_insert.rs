// `Option::get_or_insert_with` / `get_or_insert` / `get_or_insert_default` / `insert`: each
// hands out a `&mut T` into the option, so methods, `*r += ..` and writes through a binding
// update it.
#[derive(Debug, Default)]
struct Cache {
    hits: Option<u32>,
    names: Option<Vec<String>>,
}

impl Cache {
    fn hit(&mut self) -> u32 {
        *self.hits.get_or_insert(0) += 1;
        *self.hits.get_or_insert(100)
    }

    fn add(&mut self, n: &str) {
        self.names.get_or_insert_with(Vec::new).push(n.to_string());
    }
}

fn first_word(slot: &mut Option<String>, text: &str) -> usize {
    slot.get_or_insert_with(|| text.split(' ').next().unwrap_or("").to_string()).len()
}

fn main() {
    let mut o: Option<Vec<i32>> = None;
    o.get_or_insert_with(Vec::new).push(1);
    o.get_or_insert_with(|| vec![99]).push(2);
    println!("{:?}", o);

    let mut c: Option<i32> = None;
    *c.get_or_insert(0) += 1;
    *c.get_or_insert(5) += 10;
    println!("{:?}", c);

    let mut n: Option<i64> = None;
    let r = n.get_or_insert(7);
    *r *= 3;
    println!("{:?}", n);

    let mut d: Option<i32> = Some(3);
    let x = d.insert(40);
    *x += 2;
    println!("{:?}", d);

    let mut s: Option<String> = None;
    s.get_or_insert_default().push_str("abc");
    s.get_or_insert_with(String::new).push('d');
    println!("{:?} {}", s, s.as_ref().map_or(0, |t| t.len()));

    let mut words = None;
    let l1 = first_word(&mut words, "hello world");
    let l2 = first_word(&mut words, "bye");
    println!("{} {} {:?}", l1, l2, words);

    let mut cache = Cache::default();
    cache.hit();
    let h = cache.hit();
    cache.add("a");
    cache.add("b");
    println!("{} {:?}", h, cache);

    let mut total: Option<f64> = None;
    for v in [1.5, 2.5, 3.0] {
        *total.get_or_insert(0.0) += v;
    }
    println!("{:?}", total);

    let mut flag: Option<bool> = None;
    let f = flag.get_or_insert_with(|| true);
    *f = !*f;
    println!("{:?}", flag);
}
