// A small scheduling simulation: an RNG struct, `iter_mut().zip(..cycle())` writing into structs,
// `retain`, `sort_by_key` with `std::cmp::Reverse` written out, `matches!`, `!=` on a Copy enum,
// and padded table formatting.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Job { Hack, Grow, Weaken }
#[derive(Debug, Clone)]
struct Worker { id: u32, job: Job, threads: u32, done: bool }
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = (self.0 * 75 + 74) % 65537;
        (self.0 >> 3) % 1000
    }
}
fn plan(security: f64, money: f64, max_money: f64) -> Job {
    if security > 5.0 { Job::Weaken } else if money < max_money * 0.75 { Job::Grow } else { Job::Hack }
}
fn main() {
    let mut rng = Rng(42);
    let rolls: Vec<u64> = (0..5).map(|_| rng.next()).collect();
    println!("{:?}", rolls);
    let mut workers: Vec<Worker> = (1..=6).map(|i| Worker { id: i, job: Job::Hack, threads: i * 10, done: i % 3 == 0 }).collect();
    let states = [(8.0, 50.0), (2.0, 10.0), (1.0, 99.0)];
    for (w, (sec, money)) in workers.iter_mut().zip(states.iter().cycle()) {
        w.job = plan(*sec, *money, 100.0);
    }
    workers.retain(|w| !w.done);
    workers.sort_by_key(|w| std::cmp::Reverse(w.threads));
    let hackers = workers.iter().filter(|w| matches!(w.job, Job::Hack)).count();
    println!("{} hackers; {:?}", hackers, workers.iter().map(|w| (w.id, w.job)).collect::<Vec<_>>());
    let total: u32 = workers.iter().filter(|w| w.job != Job::Grow).map(|w| w.threads).sum();
    println!("{total}");
    let table: Vec<String> = workers.iter().map(|w| format!("{:>3} {:<7} {:>4}", w.id, format!("{:?}", w.job), w.threads)).collect();
    println!("{}", table.join("\n"));
}
