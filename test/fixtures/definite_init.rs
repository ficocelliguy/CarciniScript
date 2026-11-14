// Definite initialization must accept valid code: `let x;` assigned in every branch of an `if`
// or `match`, in a `loop` before `break`, before an early `return`, and `let mut x;` assigned
// more than once (also in loops), and `let (a, b);` with each name assigned once per path.
fn pick(n: i32) -> &'static str {
    let label;
    match n {
        0 => label = "zero",
        1 | 2 => label = "small",
        _ => label = "big",
    }
    label
}

fn first_even(v: &[i32]) -> Option<i32> {
    let found;
    let mut i = 0;
    loop {
        if i == v.len() {
            return None;
        }
        if v[i] % 2 == 0 {
            found = v[i];
            break;
        }
        i += 1;
    }
    Some(found)
}

fn main() {
    let x;
    if pick(1) == "small" {
        x = 10;
    } else {
        x = 20;
    }
    println!("{x} {} {} {}", pick(0), pick(2), pick(9));
    println!("{:?} {:?}", first_even(&[3, 5, 8, 9]), first_even(&[1]));

    let mut total;
    total = 0;
    for i in 1..=4 {
        total += i;
    }
    let names: Vec<String>;
    names = vec!["a".to_string(), "b".to_string()];
    println!("{total} {:?}", names);

    let (lo, hi);
    if total > 5 {
        lo = 1;
        hi = total;
    } else {
        lo = total;
        hi = 100;
    }
    println!("{lo}..{hi}");
}
