use std::fmt;

pub struct Server {
    pub name: String,
    pub money: f64,
    pub max_money: f64,
}

impl Server {
    pub fn new(name: &str, money: f64, max_money: f64) -> Self {
        Server { name: name.to_string(), money, max_money }
    }

    pub fn fill(&self) -> f64 {
        if self.max_money == 0.0 { 0.0 } else { self.money / self.max_money }
    }
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, format_money(self.max_money))
    }
}

pub fn format_money(x: f64) -> String {
    const UNITS: [(f64, &str); 4] = [(1e12, "t"), (1e9, "b"), (1e6, "m"), (1e3, "k")];
    for (size, suffix) in UNITS {
        if x >= size {
            return format!("${:.2}{}", x / size, suffix);
        }
    }
    format!("${:.2}", x)
}

pub fn bar(fraction: f64, width: usize) -> String {
    let filled = (fraction * width as f64).round() as usize;
    format!("[{}{}]", "#".repeat(filled), "-".repeat(width - filled))
}
