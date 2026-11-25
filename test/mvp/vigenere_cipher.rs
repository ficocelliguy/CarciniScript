// Coding contract: "Encryption II: Vigenère Cipher"
// Plaintext is uppercase letters and spaces; the key repeats over the whole text.

struct Contract {
    plaintext: &'static str,
    keyword: &'static str,
}

fn shift(c: char, k: char) -> char {
    let p = c as u8 - b'A';
    let s = k as u8 - b'A';
    (b'A' + (p + s) % 26) as char
}

fn encrypt(plain: &str, key: &str) -> String {
    let key: Vec<char> = key.chars().collect();
    plain
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if c.is_ascii_uppercase() {
                shift(c, key[i % key.len()])
            } else {
                c
            }
        })
        .collect()
}

fn decrypt(cipher: &str, key: &str) -> String {
    let key: Vec<char> = key.chars().collect();
    cipher
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if c.is_ascii_uppercase() {
                let back = 26 - (key[i % key.len()] as u8 - b'A');
                shift(c, (b'A' + back % 26) as char)
            } else {
                c
            }
        })
        .collect()
}

fn main() {
    let contracts = [
        Contract { plaintext: "DASHBOARD", keyword: "LINUX" },
        Contract { plaintext: "SHIFT MOUSE ENTER", keyword: "CLOUD" },
        Contract { plaintext: "POPUPCACHEFRAMEPRINTLOGIN", keyword: "HACKER" },
    ];

    let mut answers: Vec<&str> = Vec::new();
    for c in &contracts {
        let cipher = encrypt(c.plaintext, c.keyword);
        assert_eq!(decrypt(&cipher, c.keyword), c.plaintext);
        answers.push(cipher);
    }

    for (c, answer) in contracts.iter().zip(&answers) {
        println!("{:<28} key={:<7} -> {}", c.plaintext, c.keyword, answer);
    }
}
