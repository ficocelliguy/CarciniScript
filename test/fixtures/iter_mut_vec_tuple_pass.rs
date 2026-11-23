// A whole item of a `Vec<(usize, &mut T)>` collected from `iter_mut()` over numbers, strings or
// bools, passed on by shared reference (`show(&rs[1])`, `Some(&rs[1])`, `vec![&rs[0]]`): a copy
// with the slots read, which is all a `&(usize, &mut T)` can see. Writes through the item's
// fields still reach the Vec it came from.
fn show(t: &(usize, &mut i32)) -> String {
    format!("#{} = {}", t.0, *t.1)
}

fn total(items: &[&(usize, &mut i32)]) -> i32 {
    items.iter().map(|t| *t.1).sum()
}

fn label(t: &(usize, &mut String)) -> usize {
    t.0 + t.1.len()
}

fn main() {
    let mut w = vec![10, 20, 30];
    let mut rs: Vec<(usize, &mut i32)> = w.iter_mut().enumerate().collect();
    *rs[1].1 += 5;
    println!("{}", show(&rs[1]));
    let first = Some(&rs[0]).map(|t| t.0 * 100 + *t.1 as usize);
    println!("{:?}", first);
    println!("{}", total(&[&rs[0], &rs[2]]));
    *rs[2].1 = 7;
    println!("{}", show(&rs[2]));
    let picked = vec![&rs[0], &rs[1]];
    println!("{}", picked.len());
    println!("{:?}", w);

    let mut names = vec![String::from("home"), String::from("n00dles")];
    let mut ns: Vec<(usize, &mut String)> = names.iter_mut().enumerate().collect();
    ns[0].1.push('!');
    println!("{}", label(&ns[1]));
    println!("{}", label(&ns[0]));
    println!("{:?}", names);
}
