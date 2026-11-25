mod money {
    pub trait Money {
        fn money(&self) -> String;
    }

    impl Money for f64 {
        fn money(&self) -> String {
            format!("${:.2}", self)
        }
    }
}

fn main() {
    let cash = 12.5f64;
    println!("{}", cash.money());
}
