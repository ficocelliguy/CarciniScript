// `?` on Option: early return of None from the enclosing function.

fn digit(c: char) -> Option<u32> {
    if c >= '0' && c <= '9' { Some(c as u32 - '0' as u32) } else { None }
}

// Parses up to the first non-digit; None if there are no digits or a later char isn't one.
fn parse(chars: &[char]) -> Option<u32> {
    let first = *chars.first()?;
    let mut value = digit(first)?;
    for i in 1..chars.len() {
        value = value * 10 + digit(chars[i])?;
    }
    Some(value)
}

fn middle(values: &Vec<i32>) -> Option<i32> {
    let mid = values.len() / 2;
    let v = values.get(mid)?;
    Some(*v * 2)
}

fn first_even_sum(a: &Vec<i32>, b: &Vec<i32>) -> Option<i32> {
    let x = if a.is_empty() { None } else { Some(a[0]) }?;
    let y = b.last()?;
    if (x + *y) % 2 == 0 {
        return Some(x + *y);
    }
    let z = middle(b)?;
    Some(z)
}

fn main() {
    println!("{:?}", parse(&['4', '2', '7']));
    println!("{:?}", parse(&['4', 'x', '7']));
    println!("{:?}", parse(&[]));
    println!("{:?} {:?}", middle(&vec![1, 2, 3]), middle(&vec![]));
    println!("{:?}", first_even_sum(&vec![1], &vec![5, 3]));
    println!("{:?}", first_even_sum(&vec![1], &vec![4, 6, 8]));
    println!("{:?}", first_even_sum(&vec![], &vec![4]));
    println!("{:?}", first_even_sum(&vec![2], &vec![]));
}
