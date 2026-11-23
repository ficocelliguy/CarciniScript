// `From<bool>` for every integer type and the floats: 1 or 0.
fn count(flags: &[bool]) -> u32 {
    flags.iter().map(|&f| u32::from(f)).sum()
}

fn main() {
    let a = i16::from(true);
    let b = u8::from(false);
    let c: i32 = true.into();
    let d = u64::from(true) + 5;
    let e = f64::from(true) * 2.5;
    let f = usize::from(true);
    let g: f32 = false.into();
    println!("{} {} {} {} {} {} {}", a, b, c, d, e, f, g);

    let n = i128::from(true);
    let m: u128 = true.into();
    println!("{} {}", n, m);

    println!("{}", count(&[true, false, true, true]));
    let v: Vec<u8> = [true, false].iter().map(|&x| x.into()).collect();
    println!("{:?}", v);
    let hits = 3;
    let open = true;
    println!("{}", hits + i32::from(open));
}
