// A method's own `I: Iterator` / `I: IntoIterator` without `Item = ..`: `I::Item` names its items
// (a fresh type per call), in the signature and in `where I::Item: ..`.
use std::fmt::Display;

struct Picker {
    skip: usize,
}

impl Picker {
    fn first<I: Iterator>(&self, mut it: I) -> Option<I::Item> {
        for _ in 0..self.skip {
            it.next();
        }
        it.next()
    }

    fn last<I>(&self, it: I) -> I::Item
    where
        I: Iterator,
        I::Item: Default,
    {
        it.fold(Default::default(), |_, x| x)
    }

    fn all<I: IntoIterator>(&self, xs: I) -> Vec<I::Item> {
        xs.into_iter().skip(self.skip).collect()
    }

    fn biggest<I>(&self, xs: I) -> Option<I::Item>
    where
        I: IntoIterator,
        I::Item: Ord + Display,
    {
        let m = xs.into_iter().max();
        if let Some(x) = &m {
            println!("max is {}", x);
        }
        m
    }

    fn tagged<T: Display, I: Iterator>(&self, tag: T, it: I) -> Vec<(String, I::Item)> {
        it.map(|x| (tag.to_string(), x)).collect()
    }
}

trait Source {
    fn pick<I: Iterator>(&self, it: I) -> Option<I::Item>;
    fn pick_twice<I: Iterator>(&self, mut it: I) -> (Option<I::Item>, Option<I::Item>) {
        let a = it.next();
        (a, self.pick(it))
    }
}

struct Second;

impl Source for Second {
    fn pick<I: Iterator>(&self, mut it: I) -> Option<I::Item> {
        it.next();
        it.next()
    }
}

fn main() {
    let p = Picker { skip: 1 };
    println!("{:?}", p.first(vec![1, 2, 3].into_iter()));
    println!("{:?}", p.first("abc".chars()));
    println!("{}", p.last(vec![4, 5, 6].into_iter()));
    println!("{:?}", p.last(vec![String::from("x"), String::from("y")].into_iter()));
    println!("{:?}", p.all(vec![7, 8, 9]));
    println!("{:?}", p.all(["a", "b"]));
    println!("{:?}", p.biggest(vec![3, 9, 2]));
    println!("{:?}", p.biggest(Vec::<u8>::new()));
    println!("{:?}", p.tagged("t", 1..3));
    println!("{:?}", p.tagged(5, "hi".chars()));
    let s = Second;
    println!("{:?}", s.pick(10..20));
    println!("{:?}", s.pick_twice("wxyz".chars()));
    let n: Option<u64> = s.pick(vec![1u64, 2].into_iter());
    println!("{:?}", n.map(|x| x * 2));
}
