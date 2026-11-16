// An element taken out of `iter_mut()` (`min_by`, `find`, `next`, through `?` or `unwrap`) is a
// `&mut`: assigning through it needs no `let mut`, and changes the collection.
#[derive(Debug)]
struct Slot {
    host: String,
    free: f64,
}

fn reserve(slots: &mut Vec<Slot>, need: f64) -> Option<String> {
    let slot = slots
        .iter_mut()
        .filter(|s| s.free >= need)
        .min_by(|a, b| a.free.total_cmp(&b.free))?;
    slot.free -= need;
    Some(slot.host.clone())
}

fn main() {
    let mut slots = vec![
        Slot { host: "a".into(), free: 8.0 },
        Slot { host: "b".into(), free: 4.0 },
        Slot { host: "c".into(), free: 16.0 },
    ];
    println!("{:?} {:?} {:?}", reserve(&mut slots, 3.0), reserve(&mut slots, 3.0), reserve(&mut slots, 20.0));
    let first = slots.iter_mut().next().unwrap();
    first.free += 0.5;
    let c = slots.iter_mut().find(|s| s.host == "c").expect("c");
    c.host.push('!');
    println!("{slots:?}");
}
