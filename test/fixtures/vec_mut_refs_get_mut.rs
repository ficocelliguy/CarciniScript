// `get_mut` / `first_mut` / `last_mut` of a `Vec<&mut i32>` (or `&mut String`): the slot
// stored there, so `**r += 1` writes through to the referent.

fn part1() {
    let mut a = 1;
    let mut b = 2;
    let mut rs: Vec<&mut i32> = vec![&mut a, &mut b];
    **rs.get_mut(1).unwrap() += 10;
    if let Some(r) = rs.get_mut(0) { **r += 5; }
    println!("{} {}", rs[0], rs[1]);
    println!("{} {}", a, b);
    let mut s1 = String::from("x");
    let mut ss: Vec<&mut String> = vec![&mut s1];
    ss.get_mut(0).unwrap().push('y');
    println!("{}", s1);
}

fn part2() {
    let mut a = 1;
    let mut b = 2;
    let mut c = 3;
    let mut rs: Vec<&mut i32> = vec![&mut a, &mut b, &mut c];
    **rs.first_mut().unwrap() += 100;
    **rs.last_mut().unwrap() *= 7;
    let r = rs.get_mut(1).unwrap();
    **r = 50;
    match rs.get_mut(2) { Some(x) => **x -= 1, None => {} }
    for x in rs.iter_mut() { **x += 1; }
    let o = rs.get_mut(9);
    println!("{}", o.is_none());
    println!("{:?}", rs);
    println!("{} {} {}", a, b, c);
}

fn main() {
    part1();
    part2();
}
