// Coding contracts: "Compression I: RLE Compression" and "Encryption I: Caesar Cipher".

fn rle(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        let mut run = 1;
        while chars.peek() == Some(&c) && run < 9 {
            chars.next();
            run += 1;
        }
        out.push_str(&format!("{}{}", run, c));
    }
    out
}

fn caesar(text: &str, shift: u8) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                (((c as u8 - b'A' + 26 - shift % 26) % 26) + b'A') as char
            } else {
                c
            }
        })
        .collect()
}

fn main() {
    let samples = ["aaaaabccc", "zzzzzzzzzzzzzzzzzzzyy", "", "abc", "WWWWWWWWWWWWWWWWWWWWWWWWWWWW55"];
    for s in samples.iter() {
        println!("{:<32} {}", format!("{:?}", s), rle(s));
    }
    let data = ("CACHE MACRO FLASH LINUX SHELL", 4);
    println!("{}", caesar(data.0, data.1));
    println!("{}", caesar("TRASH INBOX", 29));
}
