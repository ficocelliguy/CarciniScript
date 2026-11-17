// `&mut self` methods of your traits implemented for a `String`, numbers, a `bool` and an
// `Option`, which JS can't change through `this`: called on locals, fields, elements and `&mut`
// parameters, from default methods, and through a generic `T: Tr` / `&mut W`.
trait Sink {
    fn put(&mut self, s: &str);
    fn put_line(&mut self, s: &str) {
        self.put(s);
        self.put("\n");
    }
}

impl Sink for String {
    fn put(&mut self, s: &str) {
        self.push_str(s);
    }
}

struct Count(usize);

impl Sink for Count {
    fn put(&mut self, s: &str) {
        self.0 += s.len();
    }
}

trait Bump: Sized {
    fn bump(&mut self) -> Self;
    fn bump_twice(&mut self) {
        self.bump();
        self.bump();
    }
}

impl Bump for u32 {
    fn bump(&mut self) -> u32 {
        *self += 1;
        *self
    }
}

impl Bump for f64 {
    fn bump(&mut self) -> f64 {
        *self *= 2.0;
        *self
    }
}

impl Bump for bool {
    fn bump(&mut self) -> bool {
        *self = !*self;
        *self
    }
}

trait Fill {
    fn fill_with(&mut self, v: i32) -> i32;
}

impl Fill for Option<i32> {
    fn fill_with(&mut self, v: i32) -> i32 {
        match self {
            Some(x) => {
                *x += v;
                *x
            }
            None => {
                *self = Some(v);
                v
            }
        }
    }
}

fn fill<W: Sink>(w: &mut W) {
    w.put("ab");
    w.put_line("c");
}

fn bump_all<T: Bump>(xs: &mut [T]) {
    for x in xs.iter_mut() {
        x.bump();
    }
}

fn via_param(s: &mut String, n: &mut u32) {
    s.put("<param>");
    n.bump_twice();
}

struct Log {
    text: String,
    hits: u32,
}

impl Log {
    // `&mut self.field` arguments and receivers inside a method.
    fn record(&mut self, what: &str) {
        self.text.put(what);
        self.hits.bump();
        via_param(&mut self.text, &mut self.hits);
    }
}

fn main() {
    let mut s = String::new();
    s.put("x");
    s.put_line("y");
    fill(&mut s);
    let mut c = Count(0);
    fill(&mut c);
    print!("{s}");
    println!("{}", c.0);

    let mut n: u32 = 5;
    let r = n.bump();
    n.bump_twice();
    let mut f: f64 = 1.5;
    f.bump();
    let mut b = false;
    b.bump();
    println!("{r} {n} {f} {b}");

    let mut log = Log { text: String::from(">"), hits: 0 };
    log.text.put("field");
    log.hits.bump();
    via_param(&mut log.text, &mut log.hits);
    log.record("/rec");
    println!("{} {}", log.text, log.hits);

    let mut words = vec![String::from("a"), String::from("b")];
    words[1].put("!");
    let mut nums = vec![1u32, 10, 100];
    nums[0].bump();
    bump_all(&mut nums);
    println!("{words:?} {nums:?}");

    let mut o: Option<i32> = None;
    let first = o.fill_with(3);
    let second = o.fill_with(4);
    println!("{first} {second} {o:?}");
}
