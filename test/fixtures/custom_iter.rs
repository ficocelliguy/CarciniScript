// Custom iterators: `impl Iterator for T` with `type Item`, used in `for` loops, with adapters
// (`take`, `take_while`, `filter`, `zip`, `map`, `max`, `collect`), through `next()` directly, and
// a generic iterator struct.
struct Fib { a: u64, b: u64 }
impl Iterator for Fib {
    type Item = u64;
    fn next(&mut self) -> Option<u64> {
        let r = self.a;
        self.a = self.b;
        self.b += r;
        Some(r)
    }
}
struct Countdown(u32);
impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 { return None; }
        self.0 -= 1;
        Some(self.0 + 1)
    }
}
struct Repeat<T: Clone> {
    value: T,
    left: usize,
}
impl<T: Clone> Iterator for Repeat<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.left == 0 {
            None
        } else {
            self.left -= 1;
            Some(self.value.clone())
        }
    }
}
fn main() {
    let first: Vec<u64> = Fib { a: 0, b: 1 }.take(10).collect();
    println!("{:?}", first);
    let even_sum: u64 = Fib { a: 0, b: 1 }.take_while(|&x| x < 100).filter(|x| x % 2 == 0).sum();
    println!("{}", even_sum);
    for n in Countdown(3) { print!("{n} "); }
    println!();
    let mut c = Countdown(2);
    println!("{:?} {:?} {:?}", c.next(), c.next(), c.next());
    let zipped: Vec<(u32, u64)> = Countdown(3).zip(Fib { a: 5, b: 8 }).collect();
    println!("{:?} {}", zipped, Countdown(4).map(|x| x * 10).max().unwrap());
    let words: Vec<String> = Repeat { value: "ab".to_string(), left: 3 }.collect();
    let total: usize = Repeat { value: 7usize, left: 4 }.sum();
    println!("{:?} {}", words, total);
}
