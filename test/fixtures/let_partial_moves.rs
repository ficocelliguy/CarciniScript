// Destructuring with `let` moves only the fields it binds by value: a `Copy` field leaves the
// value whole, and after a partial move the other fields can still be read.

#[derive(Debug)]
struct P {
    x: i32,
    y: i32,
    name: String,
    tags: Vec<String>,
}

struct Q(u8, Vec<u8>, f64);

fn main() {
    let p = P { x: 1, y: 2, name: "a".to_string(), tags: vec!["t".to_string()] };
    let P { x, .. } = p;
    println!("{} {:?}", x, p);
    let P { x: a, y: b, .. } = p;
    println!("{} {} {}", a, b, p.name);
    if let P { x: 1, y, .. } = p {
        println!("y={} {}", y, p.name);
    }
    match p {
        P { x, y, .. } => println!("{} {}", x, y),
    }
    println!("{:?}", p);
    let r = &p;
    let P { name, .. } = r;
    println!("{} {}", name, p.x);

    // Partial moves: the moved field goes, the rest stays readable.
    let P { tags, .. } = p;
    println!("{:?} {} {} {}", tags, p.x, p.y, p.name);

    let q = Q(3, vec![1, 2], 1.5);
    let Q(n, _, f) = q;
    println!("{} {} {:?}", n, f, q.1);
    let Q(_, bytes, _) = q;
    println!("{:?} {} {}", bytes, q.0, q.2);

    let t = (1, vec![2, 3], "s");
    let (_, v, _) = t;
    println!("{:?} {} {}", v, t.0, t.2);

    // A partial move on one branch only.
    let k = P { x: 7, y: 8, name: "k".to_string(), tags: vec![] };
    if k.x > 5 {
        let P { tags: _taken, .. } = k;
    }
    println!("{} {}", k.x, k.name);

    // `ref` bindings and `&` patterns don't move.
    let m = P { x: 9, y: 10, name: "m".to_string(), tags: vec!["u".to_string()] };
    let P { ref tags, .. } = m;
    println!("{:?} {:?}", tags, m);
    let pair = (vec![1], vec![2]);
    let (ref first, _) = pair;
    println!("{:?} {:?}", first, pair);
}
