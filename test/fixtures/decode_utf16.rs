// `char::decode_utf16`: `Ok` of each character, `Err` of each unpaired surrogate.
fn main() {
    let units: Vec<u16> = vec![0x48, 0xd83d, 0xde00, 0xdc00, 0x69, 0xd800, 0x41, 0xd801];
    for r in char::decode_utf16(units.iter().cloned()) {
        match r {
            Ok(c) => println!("ok {} {}", c, c.len_utf8()),
            Err(e) => println!("err {:x} {} {:?}", e.unpaired_surrogate(), e, e),
        }
    }
    let s: String = std::char::decode_utf16(units.clone())
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    println!("{} {}", s, s.chars().count());
    let all: Result<String, _> = char::decode_utf16([0x61u16, 0x62]).collect();
    println!("{:?}", all);
    let n = char::decode_utf16(units.iter().copied()).filter(|r| r.is_err()).count();
    println!("{}", n);
    let first = char::decode_utf16(Some(0xd800u16)).next();
    println!("{:?}", first.map(|r| r.is_ok()));
}
