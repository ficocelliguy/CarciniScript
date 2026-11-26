// One trait implemented for both `str` and `String` (and `[u8]` and `Vec<u8>`) with different
// behavior, called through a type parameter or `dyn`: the caller passes the owned type's class
// for a `T`, and an owned value coerced to a `dyn` is tagged, so the call reaches the right impl.

trait Enc {
    fn enc(&self, out: &mut Vec<u8>);
}
trait Tag {
    fn tag() -> u8;
}
impl Enc for str {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes())
    }
}
impl Tag for str {
    fn tag() -> u8 {
        10
    }
}
impl Enc for String {
    fn enc(&self, out: &mut Vec<u8>) {
        out.push(1);
        self.as_str().enc(out)
    }
}
impl Tag for String {
    fn tag() -> u8 {
        20
    }
}
impl Enc for [u8] {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self)
    }
}
impl Tag for [u8] {
    fn tag() -> u8 {
        30
    }
}
impl Enc for Vec<u8> {
    fn enc(&self, out: &mut Vec<u8>) {
        out.push(2);
        self.as_slice().enc(out)
    }
}
impl Tag for Vec<u8> {
    fn tag() -> u8 {
        40
    }
}

fn put<T: Enc + Tag + ?Sized>(t: &T, out: &mut Vec<u8>) {
    out.push(T::tag());
    t.enc(out);
}

// (Handing its own `T` on.)
fn put2<T: Enc + Tag + ?Sized>(t: &T, out: &mut Vec<u8>) {
    put(t, out);
}

fn put_owned<T: Enc>(t: T, out: &mut Vec<u8>) {
    t.enc(out);
}

fn put_qualified<T: Enc + ?Sized>(t: &T, out: &mut Vec<u8>) {
    Enc::enc(t, out);
}

fn put_impl(t: &impl Enc, out: &mut Vec<u8>) {
    t.enc(out)
}

fn put_dyn(t: &dyn Enc, out: &mut Vec<u8>) {
    t.enc(out);
}

trait Value {
    fn record(&self, out: &mut Vec<String>);
}
impl Value for str {
    fn record(&self, out: &mut Vec<String>) {
        out.push(format!("str:{}", self))
    }
}
impl Value for String {
    fn record(&self, out: &mut Vec<String>) {
        out.push(format!("String:{}", self))
    }
}
impl Value for i32 {
    fn record(&self, out: &mut Vec<String>) {
        out.push(format!("i32:{}", self))
    }
}

// A method's own type parameter.
struct Recorder {
    out: Vec<String>,
}
impl Recorder {
    fn add<V: Value + ?Sized>(&mut self, v: &V) {
        v.record(&mut self.out);
    }
}

fn main() {
    let s = String::from("ab");
    let v = vec![7u8];
    let mut o = Vec::new();
    put(&s, &mut o);
    put("cd", &mut o);
    put(s.as_str(), &mut o);
    put(&v, &mut o);
    put(&v[..], &mut o);
    put2(&s, &mut o);
    put2("e", &mut o);
    println!("{:?}", o);

    let mut o = Vec::new();
    put_owned(s.clone(), &mut o);
    put_owned(v.clone(), &mut o);
    put_qualified(&s, &mut o);
    put_qualified("f", &mut o);
    put_impl(&s, &mut o);
    put_impl(&v, &mut o);
    println!("{:?}", o);

    let mut o = Vec::new();
    put_dyn(&s, &mut o);
    put_dyn(&v, &mut o);
    let boxes: Vec<Box<dyn Enc>> = vec![Box::new(String::from("q")), Box::new(vec![3u8])];
    for b in &boxes {
        b.enc(&mut o);
    }
    println!("{:?}", o);

    let mut r = Recorder { out: Vec::new() };
    r.add(&s);
    r.add("g");
    r.add(&5);
    println!("{:?}", r.out);
}
