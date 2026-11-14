use mac::{Fields, logged, make_consts, tables};

#[derive(Debug, Fields)]
struct Server {
    pub host: String,
    ram: f64,
    cores: u32,
}

tables!(alpha: 1 2 3; beta: 40 50; gamma: 7);

make_consts!(LIMIT = 40, RATIO = 0.75, NAME = "home");

#[logged("sum")]
fn sum(a: i32, b: i32) -> i32 {
    a + b
}

#[logged("ping")]
fn ping() {
    println!("pong");
}

fn main() {
    let s = Server { host: "n00dles".to_string(), ram: 4.0, cores: 1 };
    println!("{} {:?}", Server::FIELD_COUNT, Server::field_names());
    println!("{} {} {}", s.get_host(), s.get_ram(), s.get_cores());
    println!("{} {} {}", LIMIT * 2, RATIO, NAME);
    println!("{}", sum(2, 3));
    ping();
    println!("{:?} {:?} {:?}", alpha(), beta(), gamma());
}
