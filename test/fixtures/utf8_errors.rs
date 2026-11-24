// `str::from_utf8` / `String::from_utf8` errors print as rustc's: `Utf8Error { valid_up_to, error_len }`.
fn main() {
    let cases: Vec<Vec<u8>> = vec![
        vec![0x82, 0xe6],
        b"ok\xffno".to_vec(),
        vec![b'a', 0xe2, 0x82],
        vec![b'a', 0xe2, 0x28, 0xa1],
        vec![0xf0, 0x90, 0x80, 0x41],
        vec![0xed, 0xa0, 0x80],
        vec![0xc0, 0x80],
        vec![b'h', b'i', 0xf4, 0x90, 0x80, 0x80],
    ];
    for c in &cases {
        match std::str::from_utf8(c) {
            Ok(s) => println!("ok {}", s),
            Err(e) => println!("{:?} / {}", e, e),
        }
        match String::from_utf8(c.clone()) {
            Ok(s) => println!("ok {}", s),
            Err(e) => println!("{:?} / {}", e, e),
        }
    }
}
