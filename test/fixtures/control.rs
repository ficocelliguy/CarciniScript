fn classify(n: i32) -> &'static str {
    if n < 0 {
        "negative"
    } else if n == 0 {
        "zero"
    } else {
        "positive"
    }
}

fn fib(n: u32) -> u64 {
    if n < 2 {
        return n as u64;
    }
    fib(n - 1) + fib(n - 2)
}

fn first_square_over(limit: i32) -> i32 {
    let mut i = 0;
    loop {
        if i * i > limit {
            break;
        }
        i += 1;
    }
    i
}

fn collatz_steps(mut n: u64) -> u32 {
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
    }
    steps
}

fn main() {
    println!("{} {} {}", classify(-5), classify(0), classify(9));
    println!("{}", fib(30));
    println!("{}", first_square_over(50));
    println!("{}", collatz_steps(27));

    let x = 5;
    let x = x * 2;
    {
        let x = "shadowed";
        println!("{x}");
    }
    println!("{x}");

    let label = {
        let a = 3;
        let b = 4;
        if a * a + b * b == 25 {
            let msg = format!("{a}-{b}-5");
            msg
        } else {
            String::from("nope")
        }
    };
    println!("{label}");

    for i in 0..3 {
        print!("{i} ");
    }
    println!();
    for i in (0..3).rev() {
        print!("{i} ");
    }
    println!();
    for i in 1..=3 {
        print!("{i} ");
    }
    println!();
    let n = 2 + 2;
    for i in (1..=n).rev() {
        print!("{i} ");
    }
    println!();

    let mut fx = 0;
    let mut fy = 0;
    'outer: for a in 1..20 {
        for b in a..20 {
            if a * b == 42 && a + b == 13 {
                fx = a;
                fy = b;
                break 'outer;
            }
            if b > 15 {
                continue 'outer;
            }
        }
    }
    println!("{fx} {fy}");

    let mut count = 0;
    while {
        count += 1;
        count < 3
    } {}
    println!("{count}");

    let late;
    if fx > 1 {
        late = "big";
    } else {
        late = "small";
    }
    println!("{late}");
}
