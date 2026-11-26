// Your own iterator where a generic iterator is expected: `I: Iterator<Item = u32>` and
// `impl Iterator<Item = u32>` take it, as they take std's.
#[derive(Clone, Copy, Debug)]
struct Count {
    n: u32,
}

impl Iterator for Count {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.n >= 5 {
            None
        } else {
            self.n += 1;
            Some(self.n)
        }
    }
}

fn total<I: Iterator<Item = u32>>(it: I) -> u32 {
    it.sum()
}

fn evens(it: impl Iterator<Item = u32>) -> Vec<u32> {
    it.filter(|x| x % 2 == 0).collect()
}

fn longest<I>(it: I) -> usize
where
    I: Iterator<Item = u32>,
{
    let mut best = 0;
    for x in it {
        best = best.max(x.to_string().len() + x as usize);
    }
    best
}

struct Acc;

impl Acc {
    fn add<I: Iterator<Item = u32>>(&self, it: I) -> u32 {
        it.fold(100, |a, x| a + x)
    }
}

fn counter(n: u32) -> impl Iterator<Item = u32> {
    if n > 9 {
        return Count { n: 4 };
    }
    Count { n }
}

fn main() {
    let c = Count { n: 3 };
    println!("{} {:?}", total(c), c);
    println!("{} {}", Acc.add(Count { n: 2 }), total(counter(1)) + total(counter(10)));
    println!("{}", total(Count { n: 0 }));
    println!("{:?}", evens(Count { n: 1 }));
    println!("{}", longest(Count { n: 2 }));
    println!("{}", total(vec![1u32, 2, 3].into_iter()));
    println!("{}", total(Count { n: 0 }.map(|x| x * 10)));
    println!("{} {:?}", total(1..4u32), evens(0..7));
}
