// `for x in v` where `v` is already a `&mut` collection (a parameter or a local): the loop
// variable is a `&mut` to each element, so writes through it stick.
#[derive(Debug)]
struct Unit {
    hp: i32,
    name: String,
}

fn normalize(vector: &mut [f64; 3]) {
    let mag = 2.0;
    for item in vector {
        *item /= mag;
    }
}

fn bump(v: &mut [i32]) {
    for x in v {
        *x += 1;
    }
}

fn shout(v: &mut Vec<String>) {
    for s in v {
        s.push('!');
    }
}

fn heal(units: &mut [Unit]) {
    for u in units {
        u.hp += 10;
        u.name.push('+');
    }
}

fn zero_first(grid: &mut Vec<Vec<u8>>) {
    for row in grid {
        row[0] = 0;
        for cell in row {
            *cell += 1;
        }
    }
}

fn total(v: &[i32]) -> i32 {
    let mut sum = 0;
    for x in v {
        sum += *x;
    }
    sum
}

fn main() {
    let mut a = [2.0, 4.0, 6.0];
    normalize(&mut a);
    println!("{:?}", a);

    let mut b = vec![1, 2, 3];
    bump(&mut b);
    bump(&mut b[1..]);
    println!("{:?} {}", b, total(&b));

    let mut c = vec!["a".to_string(), "b".to_string()];
    shout(&mut c);
    println!("{:?}", c);

    let mut units = vec![
        Unit { hp: 1, name: "x".into() },
        Unit { hp: 2, name: "y".into() },
    ];
    heal(&mut units);
    println!("{:?}", units);

    let mut grid = vec![vec![5, 5], vec![7, 7, 7]];
    zero_first(&mut grid);
    println!("{:?}", grid);

    let mut d = [10, 20];
    let r = &mut d;
    for x in r {
        *x *= 3;
    }
    println!("{:?}", d);
}
