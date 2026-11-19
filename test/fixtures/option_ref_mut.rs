// `ref mut` bindings and `as_mut()` on an `Option` of a number, string or bool kept in a place
// (a struct field, a Vec element, a local): `if let`, `match` (with guards), `while let`,
// `let .. else`, plain `let` patterns, `.unwrap()` / `.expect()` into a deref or a `let`, and
// the same from inside `iter_mut()` loops. Each write must reach the place, not a copy.
use std::collections::VecDeque;

#[derive(Debug)]
struct Job {
    pid: Option<u32>,
    timer: Option<i32>,
    name: Option<String>,
    inner: Inner,
}

#[derive(Debug)]
struct Inner {
    retries: Option<u8>,
}

impl Job {
    fn tick_if_let(&mut self) {
        if let Some(ref mut n) = self.timer {
            *n -= 1;
        }
    }

    fn tick_match(&mut self) {
        match self.timer {
            Some(ref mut n) if *n > 100 => *n = 100,
            Some(ref mut n) => *n -= 1,
            None => {}
        }
    }

    fn tick_as_mut(&mut self) {
        if let Some(n) = self.timer.as_mut() {
            *n -= 1;
        }
    }

    fn tick_as_mut_match(&mut self) {
        match self.timer.as_mut() {
            Some(n) => *n -= 1,
            None => self.timer = Some(0),
        }
    }

    fn tick_let_else(&mut self) -> i32 {
        let Some(ref mut n) = self.timer else {
            return -1;
        };
        *n += 10;
        *n
    }

    fn tick_as_mut_let_else(&mut self) -> i32 {
        let Some(n) = self.timer.as_mut() else {
            return -1;
        };
        *n += 10;
        *n
    }

    fn tick_unwrap(&mut self) {
        *self.timer.as_mut().unwrap() += 100;
        *self.timer.as_mut().expect("a timer") -= 1;
    }

    fn tick_unwrap_let(&mut self) {
        let r = self.timer.as_mut().unwrap();
        *r *= 2;
        let q = self.inner.retries.as_mut().unwrap();
        *q += 1;
    }

    fn drain(&mut self) -> u32 {
        let mut steps = 0;
        while let Some(n) = self.timer.as_mut() {
            if *n <= 0 {
                break;
            }
            *n -= 7;
            steps += 1;
        }
        steps
    }

    fn rename(&mut self) {
        if let Some(ref mut s) = self.name {
            s.push('!');
        }
        if let Some(s) = self.name.as_mut() {
            s.push_str("?");
        }
        match self.name {
            Some(ref mut s) => s.insert(0, '>'),
            None => {}
        }
    }

    fn nested(&mut self) {
        if let Some(ref mut r) = self.inner.retries {
            *r += 1;
        }
        if let Some(r) = self.inner.retries.as_mut() {
            *r *= 3;
        }
    }

    fn pid_or_assign(&mut self) {
        if let Some(ref mut p) = self.pid {
            *p += 1;
        } else {
            self.pid = Some(7);
        }
    }
}

fn main() {
    let mut j = Job { pid: None, timer: Some(10), name: Some("n".to_string()), inner: Inner { retries: Some(1) } };
    j.tick_if_let();
    println!("{:?}", j.timer);
    j.tick_match();
    println!("{:?}", j.timer);
    j.tick_as_mut();
    println!("{:?}", j.timer);
    j.tick_as_mut_match();
    println!("{:?}", j.timer);
    println!("{} {:?}", j.tick_let_else(), j.timer);
    println!("{} {:?}", j.tick_as_mut_let_else(), j.timer);
    j.tick_unwrap();
    println!("{:?}", j.timer);
    j.tick_match();
    println!("{:?}", j.timer);
    j.tick_unwrap_let();
    println!("{:?} {:?}", j.timer, j.inner.retries);
    println!("{} {:?}", j.drain(), j.timer);
    j.rename();
    println!("{:?}", j.name);
    j.nested();
    println!("{:?}", j.inner.retries);
    j.pid_or_assign();
    j.pid_or_assign();
    println!("{:?}", j.pid);
    // `None` stays `None`.
    j.timer = None;
    j.tick_if_let();
    j.tick_as_mut();
    println!("{} {:?}", j.tick_let_else(), j.timer);
    j.tick_as_mut_match();
    println!("{:?}", j.timer);

    // A local.
    let mut t: Option<i32> = Some(1);
    if let Some(ref mut n) = t {
        *n += 5;
    }
    match t {
        Some(ref mut n) => *n *= 2,
        None => {}
    }
    if let Some(n) = t.as_mut() {
        *n += 1;
    }
    *t.as_mut().unwrap() += 100;
    let r = t.as_mut().unwrap();
    *r -= 13;
    println!("{:?}", t);
    let mut pair = (Some(1), 2);
    let (ref mut a, ref mut b) = pair;
    *a = Some(5);
    *b += 1;
    println!("{:?}", pair);
    let mut flag: Option<bool> = Some(false);
    if let Some(ref mut f) = flag {
        *f = !*f;
    }
    println!("{:?}", flag);

    // Elements.
    let mut v = vec![Some(1), None, Some(3)];
    for o in v.iter_mut() {
        match *o {
            Some(ref mut n) => *n += 1,
            None => {}
        }
    }
    for o in v.iter_mut() {
        if let Some(n) = o.as_mut() {
            *n *= 10;
        }
    }
    if let Some(ref mut n) = v[0] {
        *n += 1;
    }
    match v[2] {
        Some(ref mut n) => *n += 2,
        None => {}
    }
    *v[0].as_mut().unwrap() += 1000;
    println!("{:?}", v);
    let mut grid = vec![vec![Some(0u8); 2]; 2];
    if let Some(c) = grid[1][0].as_mut() {
        *c = 9;
    }
    *grid[0][1].as_mut().unwrap() += 4;
    println!("{:?}", grid);
    let mut dq: VecDeque<Option<i32>> = VecDeque::from(vec![Some(1), Some(2)]);
    dq.pop_front();
    dq.push_back(Some(3));
    if let Some(ref mut n) = dq[0] {
        *n += 40;
    }
    *dq[1].as_mut().unwrap() += 20;
    println!("{:?}", dq);
    // Structs in a Vec, their Option fields from an `iter_mut()` loop.
    let mut jobs = vec![
        Job { pid: Some(1), timer: Some(5), name: None, inner: Inner { retries: None } },
        Job { pid: None, timer: None, name: Some("x".to_string()), inner: Inner { retries: Some(2) } },
    ];
    for j in jobs.iter_mut() {
        if let Some(p) = j.pid.as_mut() {
            *p = 0;
        }
        if let Some(ref mut t) = j.timer {
            *t += 1;
        }
        if let Some(r) = j.inner.retries.as_mut() {
            *r += 1;
        }
        if let Some(ref mut s) = j.name {
            s.push('y');
        }
    }
    println!("{:?}", jobs);
}
