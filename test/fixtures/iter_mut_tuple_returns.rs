// A returned iterator whose items are tuples holding a `&mut` (`-> impl Iterator<Item = (usize,
// &mut u8)>`, boxed, or from a method): the items hold slots where the declared type says.
fn evens(v: &mut Vec<u8>) -> impl Iterator<Item = (usize, &mut u8)> {
    v.iter_mut().enumerate().filter(|(i, _)| i % 2 == 0)
}

fn boxed<'a>(v: &'a mut [String]) -> Box<dyn Iterator<Item = (usize, &'a mut String)> + 'a> {
    Box::new(v.iter_mut().enumerate().skip(1))
}

struct Grid {
    cells: Vec<i32>,
    names: Vec<String>,
}

impl Grid {
    fn numbered(&mut self) -> impl Iterator<Item = (usize, &mut i32)> + '_ {
        self.cells.iter_mut().enumerate()
    }
    fn pairs(&mut self) -> impl Iterator<Item = (&mut i32, &String)> + '_ {
        self.cells.iter_mut().zip(self.names.iter())
    }
}

fn main() {
    let mut v = vec![1u8, 2, 3, 4, 5];
    for (i, x) in evens(&mut v) {
        *x += i as u8 * 10;
    }
    {
        let mut it = evens(&mut v);
        if let Some((_, x)) = it.next() {
            *x = 99;
        }
        while let Some((i, x)) = it.next() {
            *x += i as u8;
        }
    }
    evens(&mut v).for_each(|(_, x)| *x += 1);
    println!("{v:?}");

    let mut s = vec![String::from("a"), String::from("b"), String::from("c")];
    for (i, t) in boxed(&mut s) {
        t.push_str(&i.to_string());
    }
    println!("{s:?}");

    let mut g = Grid { cells: vec![0, 0, 0], names: vec!["x".into(), "y".into(), "z".into()] };
    for (i, c) in g.numbered() {
        *c = i as i32 * 3;
    }
    for (c, n) in g.pairs() {
        if n == "y" {
            *c += 100;
        }
    }
    println!("{:?}", g.cells);
}
