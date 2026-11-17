// Const generics on enums: `enum Buf<const N: usize>`, with impls reading `N` (each value keeps
// its `N`, as struct values do).
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
enum Buf<const N: usize> {
    Full([u8; N]),
    Partial { data: Vec<u8> },
    Empty,
}

impl<const N: usize> Buf<N> {
    fn capacity(&self) -> usize {
        N
    }

    fn new() -> Self {
        Buf::Empty
    }

    fn filled(b: u8) -> Self {
        Buf::Full([b; N])
    }

    fn push(self, b: u8) -> Self {
        match self {
            Buf::Empty => Buf::Partial { data: vec![b] },
            Buf::Partial { mut data } => {
                data.push(b);
                if data.len() == N {
                    let mut a = [0u8; N];
                    a.copy_from_slice(&data);
                    Buf::Full(a)
                } else {
                    Buf::Partial { data }
                }
            }
            Buf::Full(a) => Buf::Full(a),
        }
    }

    fn len(&self) -> usize {
        match self {
            Buf::Full(_) => N,
            Buf::Partial { data } => data.len(),
            Buf::Empty => 0,
        }
    }
}

impl<const N: usize> fmt::Display for Buf<N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Buf<{}>({}/{})", N, self.len(), N)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Level<const MAX: u32> {
    Low,
    High(u32),
}

impl<const MAX: u32> Level<MAX> {
    fn of(x: u32) -> Self {
        if x > MAX / 2 {
            Level::High(x.min(MAX))
        } else {
            Level::Low
        }
    }

    fn describe(&self) -> String {
        match self {
            Level::Low => format!("low (max {})", MAX),
            Level::High(v) => format!("high {} of {}", v, MAX),
        }
    }
}

const SLOTS: usize = 3;

#[derive(Debug, Clone, Default)]
enum Slot<T, const N: usize> {
    Some(Vec<T>),
    #[default]
    Vacant,
}

impl<T: Clone + std::fmt::Debug, const N: usize> Slot<T, N> {
    fn fill(x: T) -> Self {
        Self::Some(vec![x; N])
    }

    fn take(&mut self) -> Option<Vec<T>> {
        match std::mem::replace(self, Self::Vacant) {
            Slot::Some(v) => Some(v),
            Slot::Vacant => None,
        }
    }

    fn size(&self) -> usize {
        N
    }

    fn resized<const M: usize>(&self) -> Slot<T, M> {
        match self {
            Slot::Some(v) => Slot::Some(v.iter().cycle().take(M).cloned().collect()),
            Slot::Vacant => Slot::Vacant,
        }
    }
}

struct Holder {
    slot: Slot<char, SLOTS>,
}

fn total<const N: usize>(bufs: &[Buf<N>]) -> usize {
    bufs.iter().map(|b| b.len()).sum()
}

fn main() {
    let b: Buf<4> = Buf::new();
    println!("{} {}", b.capacity(), b);
    let b = b.push(1).push(2);
    println!("{:?} {}", b, b);
    let b = b.push(3).push(4);
    println!("{:?} {} {}", b, b, b.len());
    let c = b.clone();
    println!("{} {}", c == b, c.capacity());
    let f = Buf::<3>::filled(7);
    println!("{:?} {}", f, f.capacity());
    let e = Buf::<2>::Empty;
    println!("{} {}", e, e.capacity());
    let p: Buf<5> = Buf::Partial { data: vec![1] };
    println!("{} {}", p, p.capacity());
    let v: Vec<Buf<2>> = vec![Buf::new().push(1), Buf::new().push(1).push(2), Buf::Empty];
    println!("{} {:?}", total(&v), v.iter().map(|b| b.capacity()).collect::<Vec<_>>());
    let mut h = Holder { slot: Slot::fill('x') };
    println!("{:?} {}", h.slot, h.slot.size());
    println!("{:?} {:?} {}", h.slot.take(), h.slot, h.slot.size());
    let d: Slot<u8, 2> = Slot::default();
    println!("{:?} {}", d, d.size());
    let s: Slot<i32, 2> = Slot::fill(5);
    let r = s.resized::<5>();
    println!("{:?} {} {:?} {}", r, r.size(), s.clone(), s.clone().size());
    let fulls: Vec<Buf<2>> = vec![[1, 2], [3, 4]].into_iter().map(Buf::Full).collect();
    println!("{:?} {:?}", fulls, fulls.iter().map(|b| b.capacity()).collect::<Vec<_>>());
    for x in [1, 6, 30] {
        let l: Level<10> = Level::of(x);
        let m = l;
        println!("{} {:?} {}", l.describe(), m, m == l);
    }
}
