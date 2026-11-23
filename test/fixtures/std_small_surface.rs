// `String::capacity`, `Duration::try_from_secs_f32`, `String::from_utf8_unchecked`,
// `mem::forget` (no `Drop`) and `panic::catch_unwind`.
use std::time::Duration;

struct Bomb(u8);

impl Drop for Bomb {
    fn drop(&mut self) {
        println!("boom {}", self.0);
    }
}

fn main() {
    let s = String::from("héllo");
    println!("{}", s.capacity() >= s.len());
    if let Ok(d) = Duration::try_from_secs_f32(1.5) {
        println!("{:?}", d);
    }
    println!("{:?}", Duration::try_from_secs_f64(0.25));
    match Duration::try_from_secs_f32(-1.0) {
        Ok(_) => println!("?"),
        Err(e) => println!("{e}"),
    }
    let t = unsafe { String::from_utf8_unchecked(vec![104, 105]) };
    println!("{t}");
    std::mem::forget(Bomb(1));
    let _b = Bomb(2);
    let r = std::panic::catch_unwind(|| 5);
    let bad = std::env::args().count() < 100;
    let e = std::panic::catch_unwind(|| if bad { panic!("no") } else { 1 });
    println!("{:?} {}", r.ok(), e.is_err());
}
