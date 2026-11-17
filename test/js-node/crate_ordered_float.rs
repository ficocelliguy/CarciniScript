// The bundled `ordered-float`: `OrderedFloat<f64>` makes money/sec scores keys of a `BinaryHeap`,
// `BTreeMap`, `BTreeSet` and `HashMap`, and sortable (NaN sorts last, every NaN equal), with
// arithmetic between them and with plain floats; `NotNan::new` refuses NaN. `{}` is the float's,
// `{:?}` the crate's (`OrderedFloat(1.5)`).
// (The crate isn't in the cargo cache: the expected output is rustc's on this program with the
// prelude (`compiler/src/ordered_float_prelude.rs`) in place of the crate.)
use ordered_float::{NotNan, OrderedFloat};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};

struct Server {
    name: &'static str,
    money: f64,
    hack_time: f64,
}

fn score(s: &Server) -> OrderedFloat<f64> {
    OrderedFloat(s.money / s.hack_time)
}

fn main() {
    let servers = [
        Server { name: "n00dles", money: 1.75e6, hack_time: 2.5 },
        Server { name: "joesguns", money: 2.5e7, hack_time: 12.0 },
        Server { name: "foodnstuff", money: 5.0e7, hack_time: 20.0 },
        Server { name: "sigma-cosmetics", money: 2.3e7, hack_time: 9.5 },
        Server { name: "broken", money: 0.0, hack_time: 0.0 },
    ];

    // Best target first.
    let mut heap = BinaryHeap::new();
    for s in &servers {
        heap.push((score(s), s.name));
    }
    while let Some((OrderedFloat(rate), name)) = heap.pop() {
        println!("{name:>16} {rate:.1}");
    }

    // Cheapest first, with `Reverse`.
    let mut cheap: BinaryHeap<Reverse<OrderedFloat<f64>>> = BinaryHeap::new();
    for x in [3.5, -1.0, 10.25, 2.0] {
        cheap.push(Reverse(OrderedFloat(x)));
    }
    println!("{:?}", cheap.peek());
    let order: Vec<f64> = std::iter::from_fn(|| cheap.pop().map(|Reverse(x)| x.0)).collect();
    println!("{order:?}");

    // Sorting, max/min, NaN last.
    let mut v: Vec<OrderedFloat<f64>> = [2.5, f64::NAN, -0.5, 7.0, 0.0].iter().map(|&x| OrderedFloat(x)).collect();
    v.sort();
    println!("{v:?}");
    let best = servers.iter().max_by_key(|s| score(s)).unwrap();
    let worst = servers.iter().map(score).min().unwrap();
    println!("{} {} {:?}", best.name, worst, v.iter().max());
    println!("{} {}", OrderedFloat(f64::NAN) == OrderedFloat(f64::NAN), OrderedFloat(-0.0) == OrderedFloat(0.0));
    println!("{:?}", OrderedFloat(f64::NAN).cmp(&OrderedFloat(f64::INFINITY)));

    // Map and set keys.
    let mut by_rate: BTreeMap<OrderedFloat<f64>, &str> = BTreeMap::new();
    for s in &servers {
        by_rate.insert(score(s), s.name);
    }
    println!("{:?}", by_rate.keys().next_back());
    println!("{:?}", by_rate.range(OrderedFloat(1e6)..).map(|(_, n)| *n).collect::<Vec<_>>());
    let mut counts: HashMap<OrderedFloat<f64>, u32> = HashMap::new();
    for x in [1.5, 2.0, 1.5, f64::NAN, f64::NAN, f64::INFINITY, -0.0, 0.0] {
        *counts.entry(OrderedFloat(x)).or_insert(0) += 1;
    }
    let mut cv: Vec<_> = counts.into_iter().collect();
    cv.sort();
    println!("{cv:?}");
    let set: BTreeSet<OrderedFloat<f64>> = [3.0, 1.0, 3.0, 2.0].into_iter().map(OrderedFloat).collect();
    println!("{set:?}");

    // Arithmetic, deref and conversions.
    let a = OrderedFloat(1.5);
    let b: OrderedFloat<f64> = 2.25.into();
    let mut c = a + b * 2.0 - OrderedFloat(0.5);
    c += 1.0;
    c *= 2.0;
    println!("{} {:?} {} {} {:.3}", c, -c, c.into_inner(), *c / 4.0, c.sqrt());
    println!("{} {}", (a / b).0, OrderedFloat::from(3.0) % OrderedFloat(2.0));
    let f: OrderedFloat<f32> = OrderedFloat(0.1);
    println!("{:?} {:?}", f, f + OrderedFloat(0.2));
    println!("{:?}", OrderedFloat::<f64>::default());

    // NotNan.
    let n = NotNan::new(4.5).unwrap();
    let m = NotNan::new(0.5).unwrap();
    println!("{} {:?} {} {}", n, m, *n + 1.0, (n + m).into_inner());
    println!("{:?} {:?}", NotNan::new(f64::NAN), NotNan::new(1.0).map(|x| x * NotNan::new(3.0).unwrap()));
    match NotNan::new(0.0 / 0.0) {
        Ok(x) => println!("ok {x}"),
        Err(e) => println!("error: {e}"),
    }
    let mut nn: Vec<NotNan<f64>> = [3.0, -1.0, 2.0].iter().map(|&x| NotNan::new(x).unwrap()).collect();
    nn.sort();
    println!("{nn:?} {:?}", nn.iter().max());
}
