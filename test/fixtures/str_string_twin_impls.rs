// One trait implemented for both `str` and `String` (and `[u8]` and `Vec<u8>`) with different
// behavior: one type in JS, so each call picks its impl by how the receiver is written, as
// rustc's method probing does by its type.

trait Enc {
    fn enc(&self, out: &mut Vec<u8>);
}
impl Enc for str {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes())
    }
}
impl Enc for String {
    fn enc(&self, out: &mut Vec<u8>) {
        out.push(1);
        self.as_str().enc(out)
    }
}
impl Enc for [u8] {
    fn enc(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self)
    }
}
impl Enc for Vec<u8> {
    fn enc(&self, out: &mut Vec<u8>) {
        out.push(2);
        self.as_slice().enc(out)
    }
}

// Associated functions, picked by the path's type.
trait Decode: Sized {
    fn decode(data: &str) -> Self;
}
impl<'a> Decode for &'a str {
    fn decode(data: &str) -> &'a str {
        if data.is_empty() { "empty" } else { "borrowed" }
    }
}
impl Decode for String {
    fn decode(data: &str) -> String {
        format!("owned {}", data.len())
    }
}

struct Named {
    name: String,
    tag: &'static str,
    bytes: Vec<u8>,
}

fn owned_name() -> String {
    "fn".to_string()
}

fn encode_param(s: &str, owned: &String, out: &mut Vec<u8>) {
    s.enc(out);
    owned.enc(out);
}

fn main() {
    let s = String::from("ab");
    let v = vec![7u8, 8];
    let mut o = Vec::new();
    let t: &str = "t";
    let u = s.as_str();
    s.enc(&mut o);
    t.enc(&mut o);
    u.enc(&mut o);
    "lit".enc(&mut o);
    (&s).enc(&mut o);
    s.clone().enc(&mut o);
    format!("f{}", 1).enc(&mut o);
    owned_name().enc(&mut o);
    println!("{:?}", o);

    let mut o = Vec::new();
    let n = Named { name: "n".into(), tag: "g", bytes: vec![3] };
    n.name.enc(&mut o);
    n.tag.enc(&mut o);
    n.bytes.enc(&mut o);
    v.enc(&mut o);
    v.as_slice().enc(&mut o);
    [5u8, 6].enc(&mut o);
    v[..1].enc(&mut o);
    encode_param("p", &s, &mut o);
    println!("{:?}", o);

    let mut o = Vec::new();
    Enc::enc(&s, &mut o);
    Enc::enc("z", &mut o);
    <String as Enc>::enc(&s, &mut o);
    <str as Enc>::enc(&s, &mut o);
    for word in ["x", "y"] {
        let owned = word.to_string();
        owned.enc(&mut o);
        word.enc(&mut o);
    }
    println!("{:?}", o);

    println!("{} | {} | {}", String::decode("abc"), <&str as Decode>::decode(""), <&str as Decode>::decode("q"));
}
