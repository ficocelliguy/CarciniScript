// Coding contract: "Generate IP Addresses".

fn valid_octet(s: &str) -> bool {
    if s.is_empty() || s.len() > 3 || (s.len() > 1 && s.starts_with('0')) {
        return false;
    }
    s.parse::<u16>().map_or(false, |n| n <= 255)
}

fn generate(digits: &str, parts: &mut Vec<String>, out: &mut Vec<String>) {
    if parts.len() == 4 {
        if digits.is_empty() {
            out.push(parts.join("."));
        }
        return;
    }
    for len in 1..=3.min(digits.len()) {
        let (head, rest) = digits.split_at(len);
        if valid_octet(head) {
            parts.push(head.to_string());
            generate(rest, parts, out);
            parts.pop();
        }
    }
}

fn main() {
    for input in ["25525511135", "1938718066", "0000", "101023", "12"] {
        let mut out = Vec::new();
        generate(input, &mut Vec::new(), &mut out);
        println!("{input}: {} -> {:?}", out.len(), out);
    }
}
