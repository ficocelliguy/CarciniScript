// A `&mut` rebound to a plain local, or bound by a pattern, is written through without `mut`:
// `let t = v;` with `v: &mut Vec<u8>`, `let t = if c { a } else { b };`, a `match` whose arms
// each give one, `if let Some(s) = o` and `let s = o?;` with `o: Option<&mut T>`, `let t =
// v.as_mut_slice();`, the `&mut` items of your own iterator (`type Item = &'a mut T`) and of
// `.zip(dst.chunks_mut(n))`.
fn push_through(v: &mut Vec<u8>) {
    let t = v;
    t.push(1);
}

fn pick(a: &mut Vec<u8>, b: &mut Vec<u8>, c: bool) {
    let t = if c { a } else { b };
    t.push(7);
}

fn pick3(a: &mut [u8], b: &mut [u8], c: &mut [u8], n: u8) {
    let t = if n == 0 {
        a
    } else if n == 1 {
        b
    } else {
        c
    };
    t[0] = n + 10;
}

fn arm(o: &mut Option<Vec<u8>>, fallback: &mut Vec<u8>) {
    let t = match o {
        Some(t) => t,
        None => fallback,
    };
    t.push(3);
}

fn if_let(o: Option<&mut Vec<u8>>) {
    if let Some(s) = o {
        s.push(9);
    }
}

fn via_q(o: Option<&mut Vec<u8>>) -> Option<usize> {
    let s = o?;
    s.push(8);
    Some(s.len())
}

fn via_s(o: Option<&mut String>) -> Option<usize> {
    let s = o?;
    s.push('!');
    Some(s.len())
}

fn typed_let(v: &mut Vec<String>) {
    let o: Option<&mut String> = v.get_mut(1);
    if let Some(s) = o {
        s.push_str("++");
    }
    let first = v.get_mut(0);
    if let Some(s) = first {
        s.insert(0, '>');
    }
}

fn slice_view(v: &mut Vec<u8>) {
    let t = v.as_mut_slice();
    t[0] = 42;
    t.swap(1, 2);
}

struct Words(Vec<String>);

struct WordsMut<'a>(std::slice::IterMut<'a, String>);

impl<'a> Iterator for WordsMut<'a> {
    type Item = &'a mut String;
    fn next(&mut self) -> Option<&'a mut String> {
        self.0.next()
    }
}

impl Words {
    fn words_mut(&mut self) -> WordsMut<'_> {
        WordsMut(self.0.iter_mut())
    }
    fn shout(&mut self) {
        for w in self.words_mut() {
            w.push('!');
        }
    }
}

struct Bytes(Vec<u8>);

struct BytesMut<'a>(std::slice::IterMut<'a, u8>);

impl<'a> Iterator for BytesMut<'a> {
    type Item = &'a mut u8;
    fn next(&mut self) -> Option<&'a mut u8> {
        self.0.next()
    }
}

impl Bytes {
    fn bytes_mut(&mut self) -> BytesMut<'_> {
        BytesMut(self.0.iter_mut())
    }
    fn upper(&mut self) {
        for b in self.bytes_mut() {
            *b = b.to_ascii_uppercase();
        }
    }
}

fn zip_into(src: &[u8], dst: &mut [u8]) {
    for (x, chunk) in src.iter().zip(dst.chunks_mut(2)) {
        chunk[0] = *x;
    }
    for (chunk, x) in dst.chunks_mut(2).zip(src.iter()) {
        chunk[1] = *x + 1;
    }
}

fn main() {
    let mut v = vec![0u8];
    push_through(&mut v);
    let mut w = vec![5u8];
    pick(&mut v, &mut w, false);
    pick(&mut v, &mut w, true);
    println!("{:?} {:?}", v, w);

    let (mut a, mut b, mut c) = ([0u8; 2], [0u8; 2], [0u8; 2]);
    pick3(&mut a, &mut b, &mut c, 0);
    pick3(&mut a, &mut b, &mut c, 1);
    pick3(&mut a, &mut b, &mut c, 2);
    println!("{:?} {:?} {:?}", a, b, c);

    let mut some = Some(vec![1u8]);
    let mut none = None;
    let mut fb = vec![];
    arm(&mut some, &mut fb);
    arm(&mut none, &mut fb);
    println!("{:?} {:?} {:?}", some, none, fb);

    if_let(Some(&mut v));
    if_let(None);
    let mut s = String::from("hi");
    println!("{:?} {:?} {:?} {:?} {}", via_q(Some(&mut v)), v, via_s(Some(&mut s)), via_s(None), s);

    let mut words = vec![String::from("a"), String::from("b")];
    typed_let(&mut words);
    println!("{:?}", words);

    let mut sl = vec![1u8, 2, 3];
    slice_view(&mut sl);
    println!("{:?}", sl);

    let mut ws = Words(vec![String::from("x"), String::from("y")]);
    ws.shout();
    let mut bs = Bytes(b"abc".to_vec());
    bs.upper();
    println!("{:?} {:?}", ws.0, bs.0);

    let mut dst = [0u8; 6];
    zip_into(&[1, 2, 3], &mut dst);
    println!("{:?}", dst);
}
