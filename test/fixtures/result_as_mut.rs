// `Result::as_mut()` on a place: matched, its bindings write the payload in place; unwrapped, a
// number's slot does.
#[derive(Debug)]
struct Holder {
    r: Result<i32, String>,
    s: Result<String, i32>,
}

impl Holder {
    fn bump(&mut self) {
        if let Ok(n) = self.r.as_mut() {
            *n += 1;
        }
        *self.r.as_mut().unwrap() *= 10;
        if let Ok(s) = self.s.as_mut() {
            s.push('!');
        }
    }
}

fn main() {
    let mut r: Result<i32, String> = Ok(1);
    if let Ok(x) = r.as_mut() {
        *x += 1;
    }
    println!("{:?}", r);

    let mut e: Result<i32, i32> = Err(3);
    if let Err(x) = e.as_mut() {
        *x *= 5;
    }
    match e.as_mut() {
        Ok(v) => *v = 0,
        Err(v) => *v += 1,
    }
    println!("{:?}", e);

    let mut s: Result<String, ()> = Ok("a".to_string());
    if let Ok(x) = s.as_mut() {
        x.push('b');
    }
    println!("{:?}", s);

    let y = r.as_mut().unwrap();
    *y = 40;
    *r.as_mut().expect("ok") += 2;
    println!("{:?}", r);

    let mut h = Holder {
        r: Ok(5),
        s: Ok("hi".to_string()),
    };
    h.bump();
    println!("{:?}", h);

    let mut v: Vec<Result<u8, bool>> = vec![Ok(1), Err(false), Ok(3)];
    for i in 0..v.len() {
        match v[i].as_mut() {
            Ok(n) => *n += 100,
            Err(b) => *b = !*b,
        }
    }
    *v[2].as_mut().unwrap() -= 1;
    println!("{:?}", v);

    let mut f: Result<f64, ()> = Ok(1.5);
    let g = f;
    if let Ok(x) = f.as_mut() {
        *x *= 2.0;
    }
    println!("{:?} {:?}", f, g);
}
