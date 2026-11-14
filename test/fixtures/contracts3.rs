// Coding-contract solvers checked against rustc: valid math expressions (recursion over
// `as_bytes` and `str::from_utf8`), Hamming encoding (usize bit ops), Gray codes, bit counts and
// `from_str_radix`.
fn math_exprs(digits: &str, target: i64) -> Vec<String> {
    fn go(d: &[u8], pos: usize, expr: String, value: i64, last: i64, target: i64, out: &mut Vec<String>) {
        if pos == d.len() {
            if value == target { out.push(expr); }
            return;
        }
        for end in pos + 1..=d.len() {
            if end > pos + 1 && d[pos] == b'0' { break; }
            let s = std::str::from_utf8(&d[pos..end]).unwrap();
            let n: i64 = s.parse().unwrap();
            if pos == 0 {
                go(d, end, s.to_string(), n, n, target, out);
            } else {
                go(d, end, format!("{expr}+{s}"), value + n, n, target, out);
                go(d, end, format!("{expr}-{s}"), value - n, -n, target, out);
                go(d, end, format!("{expr}*{s}"), value - last + last * n, last * n, target, out);
            }
        }
    }
    let mut out = vec![];
    go(digits.as_bytes(), 0, String::new(), 0, 0, target, &mut out);
    out
}
fn hamming_encode(n: u32) -> String {
    let data: Vec<u8> = format!("{:b}", n).bytes().map(|b| b - b'0').collect();
    let mut p = 0;
    while (1usize << p) < data.len() + p + 1 { p += 1; }
    let total = data.len() + p + 1;
    let mut bits = vec![0u8; total];
    let mut di = 0;
    for i in 1..total { if !i.is_power_of_two() { bits[i] = data[di]; di += 1; } }
    for k in 0..p {
        let pos = 1 << k;
        let parity = (1..total).filter(|i| i & pos != 0).fold(0, |acc, i| acc ^ bits[i]);
        bits[pos] = parity;
    }
    bits[0] = bits.iter().fold(0, |a, b| a ^ b);
    bits.iter().map(|b| (b'0' + b) as char).collect()
}
fn gray(n: u32) -> Vec<String> { (0..1u32 << n).map(|i| format!("{:0width$b}", i ^ (i >> 1), width = n as usize)).collect() }
fn main() {
    println!("{:?}", math_exprs("123", 6));
    println!("{:?}", math_exprs("105", 5));
    println!("{} {}", hamming_encode(8), hamming_encode(21));
    println!("{:?} {} {}", gray(3), 12u32.count_ones(), 40u32.trailing_zeros());
    println!("{} {} {}", u32::from_str_radix("ff", 16).unwrap(), i64::from_str_radix("-101", 2).unwrap(), 255u8.leading_zeros());
}
