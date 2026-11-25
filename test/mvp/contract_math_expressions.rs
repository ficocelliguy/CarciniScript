// Coding contract: "Find All Valid Math Expressions".

struct Search<'a> {
    digits: &'a [u8],
    target: i64,
    found: Vec<String>,
}

impl<'a> Search<'a> {
    fn new(digits: &'a str, target: i64) -> Self {
        Search { digits: digits.as_bytes(), target, found: vec![] }
    }

    fn run(mut self) -> Vec<String> {
        self.go(0, String::new(), 0, 0);
        self.found.sort();
        self.found
    }

    fn go(&mut self, pos: usize, expr: String, value: i64, last: i64) {
        if pos == self.digits.len() {
            if value == self.target {
                self.found.push(expr);
            }
            return;
        }
        let mut operand: i64 = 0;
        for end in pos..self.digits.len() {
            if end > pos && self.digits[pos] == b'0' {
                break;
            }
            operand = operand * 10 + (self.digits[end] - b'0') as i64;
            let text = std::str::from_utf8(&self.digits[pos..=end]).unwrap();
            if pos == 0 {
                self.go(end + 1, text.to_string(), operand, operand);
            } else {
                self.go(end + 1, format!("{expr}+{text}"), value + operand, operand);
                self.go(end + 1, format!("{expr}-{text}"), value - operand, -operand);
                self.go(end + 1, format!("{expr}*{text}"), value - last + last * operand, last * operand);
            }
        }
    }
}

fn main() {
    let cases = [("123", 6), ("105", 5), ("00", 0), ("1234", 10), ("3456237490", 9191)];
    for (digits, target) in cases {
        let found = Search::new(digits, target).run();
        println!("{} = {}: {} {:?}", digits, target, found.len(), found);
    }
}
