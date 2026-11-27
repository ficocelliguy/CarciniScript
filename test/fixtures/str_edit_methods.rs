// `String` / `str` methods: editing in place (`reserve`, `split_off`, `drain`, `replace_range`,
// `retain`, `make_ascii_*`), searching and splitting (`match_indices`, `split_inclusive`,
// `split_ascii_whitespace`), `encode_utf16` / `String::from_utf16`, `escape_*` and
// `into_boxed_bytes`; also through a `&mut String` parameter.
fn edit(s: &mut String) -> String {
    s.reserve(8);
    s.reserve_exact(2);
    s.shrink_to_fit();
    let tail = s.split_off(3);
    s.make_ascii_uppercase();
    tail
}

fn main() {
    let mut s = String::from("héllo wörld");
    let tail = s.split_off(6);
    println!("[{s}] [{tail}]");
    let mut t = String::from("abcdef");
    let d: String = t.drain(1..3).collect();
    println!("{d} {t}");
    let d2: Vec<char> = t.drain(..).collect();
    println!("{d2:?} {:?} {}", t, t.is_empty());
    let mut u = String::from("αβγδ");
    let mid: String = u.drain(2..=3).collect();
    println!("{mid} {u}");
    let mut r = String::from("Hello, world");
    r.replace_range(0..5, "Howdy");
    r.replace_range(7.., "there!");
    println!("{r}");
    r.retain(|c| c != 'o' && c != 'e');
    println!("{r}");
    r.make_ascii_lowercase();
    println!("{r}");
    let mut p = String::from("abcdef");
    let tl = edit(&mut p);
    println!("{p} {tl}");

    let hay = "abXabXXab";
    let m: Vec<(usize, &str)> = hay.match_indices("ab").collect();
    let rm: Vec<(usize, &str)> = hay.rmatch_indices('X').collect();
    let um: Vec<(usize, &str)> = "äbäc".match_indices('ä').collect();
    println!("{m:?} {rm:?} {um:?}");
    let si: Vec<&str> = "a\nb\nc\n".split_inclusive('\n').collect();
    let si2: Vec<&str> = "one, two,three".split_inclusive(", ").collect();
    let si3: Vec<&str> = "a1b22c".split_inclusive(|c: char| c.is_ascii_digit()).collect();
    println!("{si:?} {si2:?} {si3:?}");
    println!("{:?}", "  a  b\tc\n ".split_ascii_whitespace().collect::<Vec<_>>());

    let units: Vec<u16> = "a😀é".encode_utf16().collect();
    println!("{units:?}");
    println!("{:?}", String::from_utf16(&units));
    let bad = [0x61u16, 0xD800, 0x62];
    println!("{:?} {}", String::from_utf16(&bad).is_err(), String::from_utf16_lossy(&bad));
    match String::from_utf16(&bad) {
        Ok(_) => {}
        Err(e) => println!("{e} / {e:?}"),
    }

    println!("{}", "tab\there \"q\" 'a' \\ é".escape_debug());
    println!("{}", "tab\there é\u{1}".escape_default());
    println!("{}", "aé".escape_unicode().to_string());
    let shown = format!("<{}>", "x\ny".escape_debug());
    println!("{shown}");
    let b: Box<[u8]> = String::from("hé").into_boxed_str().into_boxed_bytes();
    println!("{b:?}");
}
