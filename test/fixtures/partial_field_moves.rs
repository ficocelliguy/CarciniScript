// Moving one field out of a struct (`let a = p.a;`) leaves the others readable, and assigning
// the field again makes the struct whole.
struct P {
    a: String,
    b: String,
    v: Vec<u8>,
    k: u8,
}

struct Pair(String, u32);

fn main() {
    let mut p = P { a: "x".into(), b: "y".into(), v: vec![1, 2], k: 2 };
    let a = p.a;
    let v = p.v;
    println!("{} {} {:?} {}", a, p.b, v, p.k);
    p.a = "z".to_string();
    p.v = vec![3];
    let q = p;
    println!("{} {} {:?}", q.a, q.b, q.v);

    let t = Pair("left".into(), 4);
    let s = t.0;
    println!("{} {}", s, t.1);
}
