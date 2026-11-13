// match: literals, negative literals, ranges, chars, `|`, guards, `@` bindings, Option, bool,
// match as an expression, and control flow out of arms.

fn classify(n: i32) -> &'static str {
    match n {
        i32::MIN..=-1 => "negative",
        0 => "zero",
        1 | 2 | 3 => "small",
        4..=9 => "digit",
        x if x % 2 == 0 => "big even",
        _ => "big odd",
    }
}

fn grade(score: u32) -> char {
    match score {
        90.. => 'A',
        80..90 => 'B',
        70..80 => 'C',
        _ => 'F',
    }
}

fn kind(c: char) -> &'static str {
    match c {
        'a' | 'e' | 'i' | 'o' | 'u' => "vowel",
        'a'..='z' => "consonant",
        '0'..='9' => "digit",
        '😀'..='😏' => "smiley",
        _ => "other",
    }
}

fn halve(n: Option<i32>) -> Option<i32> {
    match n {
        Some(v) if v % 2 == 0 => Some(v / 2),
        Some(_) => None,
        None => None,
    }
}

fn main() {
    for n in [-7, 0, 2, 7, 12, 13] {
        print!("{} ", classify(n));
    }
    println!();
    for s in [95, 85, 75, 10] {
        print!("{}", grade(s));
    }
    println!();
    for c in ['e', 'z', '7', '😃', '#'] {
        print!("{} ", kind(c));
    }
    println!();
    println!("{:?} {:?} {:?}", halve(Some(8)), halve(Some(7)), halve(None));

    let flag = true;
    let label = match flag {
        true => "on",
        false => "off",
    };
    println!("{label}");

    let message = match "pear" {
        "apple" => String::from("red"),
        "pear" => {
            let mut s = String::from("gr");
            s.push_str("een");
            s
        }
        other => format!("unknown {other}"),
    };
    println!("{message}");

    let mut total = 0;
    let mut i = 0;
    loop {
        i += 1;
        match i {
            n @ 1..=3 => total += n,
            4 => continue,
            n if n > 6 => break,
            _ => total += 100,
        }
    }
    println!("total {total} after {i}");

    let readings = vec![Some(3), None, Some(-2)];
    let mut sum = 0;
    for r in &readings {
        sum += match r {
            Some(v) if *v > 0 => *v,
            Some(v) => -*v * 10,
            None => 1000,
        };
    }
    println!("sum {sum}");

    let code = 404u16;
    let text = match code {
        200 => "ok",
        301 | 302 => "redirect",
        400..=499 => match code {
            404 => "not found",
            _ => "client error",
        },
        _ => "other",
    };
    println!("{text}");
}
