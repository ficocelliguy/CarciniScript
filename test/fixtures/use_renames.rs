// `use .. as ..` renames of std types, traits and modules (`HashMap as Map`, `fmt::Result as
// FmtResult`, `collections as coll`), of the file's own types, and of a crate path inside a list.
use std::collections::BTreeMap as Sorted;
use std::collections::HashMap as Map;
use std::collections::{self as coll, HashSet as Set, VecDeque as Queue};
use std::fmt;
use std::fmt::Result as FmtResult;
use std::fmt::Write as FmtWrite;

mod shapes {
    #[derive(Debug, Clone, Copy)]
    pub struct Square(pub i32);
    #[derive(Debug)]
    pub enum Kind {
        Small,
        Big,
    }
}

use shapes::Kind as K;
use shapes::Square as Sq;

struct Report {
    counts: Map<String, usize>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter) -> FmtResult {
        let sorted: Sorted<&String, &usize> = self.counts.iter().collect();
        for (k, v) in sorted {
            write!(f, "{k}={v};")?;
        }
        Ok(())
    }
}

fn kinds(s: &[Sq]) -> Vec<K> {
    s.iter().map(|q| if q.0 > 2 { K::Big } else { K::Small }).collect()
}

fn main() {
    let mut m: Map<String, usize> = Map::new();
    for w in "a b a c b a".split(' ') {
        *m.entry(w.to_string()).or_insert(0) += 1;
    }
    println!("{}", Report { counts: m });

    let mut set = Set::new();
    set.insert(3);
    set.insert(3);
    let mut q: Queue<i32> = Queue::new();
    q.push_back(1);
    q.push_front(0);
    let mut b: coll::BTreeSet<i32> = coll::BTreeSet::new();
    b.insert(9);
    b.insert(4);
    println!("{} {:?} {:?}", set.len(), q, b);

    let mut out = String::new();
    write!(out, "{}-{}", 1, 2).unwrap();
    println!("{out}");

    let squares = vec![Sq(1), Sq(5)];
    println!("{:?} {:?}", squares, kinds(&squares));
    let sorted = Sorted::from([(2, "two"), (1, "one")]);
    println!("{:?}", sorted);
}
