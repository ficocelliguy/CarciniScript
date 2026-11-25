// Tally money per server; forgot `mut` twice.
struct Tally {
    total: f64,
    count: u32,
}

impl Tally {
    fn add(&mut self, amount: f64) {
        self.total += amount;
        self.count += 1;
    }
}

fn main() {
    let tally = Tally { total: 0.0, count: 0 };
    let best = 0.0;
    for amount in [1200.5, 99.0, 45000.25] {
        tally.add(amount);
        if amount > best {
            best = amount;
        }
    }
    println!("{} over {} servers, best {}", tally.total, tally.count, best);
}
