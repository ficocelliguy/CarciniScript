// A loop, `if` or `while` body right after a fragment (`for i in 0..$len { arr[i] = .. }`) is a
// block, not a struct literal, so the hygiene renames inside it stay plain names.
macro_rules! table {
    ($name:ident, $len:expr) => {
        fn $name() -> [u8; $len] {
            let mut arr = [0u8; $len];
            for i in 0..$len { arr[i] = (i * 3) as u8; }
            arr
        }
    };
}

macro_rules! count_down {
    ($start:expr) => {{
        let mut n = $start;
        let mut steps = Vec::new();
        while n > $start / 2 { steps.push(n); n -= 1; }
        if n == $start / 2 { steps.push(0); }
        steps
    }};
}

struct P { x: i32, y: i32 }

macro_rules! point {
    ($x:expr) => {{
        let x = $x;
        let y = x * 2;
        P { x, y }
    }};
}

table!(small, 4);
table!(big, 6);

fn main() {
    println!("{:?}", small());
    println!("{:?}", big());
    println!("{:?}", count_down!(10));
    let p = point!(7);
    println!("{} {}", p.x, p.y);
}
