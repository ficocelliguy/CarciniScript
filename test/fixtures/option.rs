// Option<T>: construction, methods, if let / while let / let-else, and Vec methods that return it.

fn find_first_over(values: &[i32], limit: i32) -> Option<usize> {
    for i in 0..values.len() {
        if values[i] > limit {
            return Some(i);
        }
    }
    None
}

fn parse_digit(c: char) -> Option<u32> {
    if c >= '0' && c <= '9' { Some(c as u32 - '0' as u32) } else { None }
}

fn describe(slot: Option<i64>) -> String {
    let Some(v) = slot else {
        return String::from("empty");
    };
    format!("holds {v}")
}

fn sum_digits(text: &str) -> u32 {
    let mut total = 0;
    let mut chars: Vec<char> = Vec::new();
    for c in ['4', 'x', '2', '7'] {
        chars.push(c);
    }
    let _ = text;
    while let Some(c) = chars.pop() {
        if let Some(d) = parse_digit(c) {
            total += d;
        } else {
            total += 100;
        }
    }
    total
}

fn main() {
    let values = vec![3, 8, 1, 12, 5];
    println!("{:?} {:?}", find_first_over(&values, 7), find_first_over(&values, 50));

    let some: Option<i32> = Some(4);
    let none: Option<i32> = None;
    println!("{} {} {} {}", some.is_some(), some.is_none(), none.is_some(), none.is_none());
    println!("{} {} {}", some.unwrap(), none.unwrap_or(-1), none.unwrap_or_default());
    println!("{:?} {:?} {:?}", some, none, Some("text"));
    println!("{} {}", some == Some(4), none == None);

    let mut slot = Some(10);
    let taken = slot.take();
    println!("{:?} {:?}", taken, slot);
    let old = slot.replace(20);
    println!("{:?} {:?}", old, slot);

    println!("{} | {}", describe(Some(9)), describe(None));
    println!("digits: {}", sum_digits("ignored"));

    let mut stack = vec![1, 2, 3];
    println!("{:?} {:?} {:?} {:?}", stack.first(), stack.last(), stack.get(1), stack.get(9));
    let mut popped = Vec::new();
    while let Some(top) = stack.pop() {
        popped.push(top * 10);
    }
    println!("{:?} {:?} {:?}", popped, stack.pop(), stack.first());

    let words: Vec<String> = vec![String::from("a"), String::from("bb")];
    if let Some(w) = words.last() {
        println!("last word {w} has {} bytes", w.len());
    }
    let names: Vec<Option<String>> = vec![Some(String::from("x")), None];
    println!("{:?}", names);

    let maybe: Option<f64> = None;
    let value = maybe.expect("the reading should exist");
    println!("unreachable {value}");
}
