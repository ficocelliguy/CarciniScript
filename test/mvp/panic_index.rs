// A sliding-window helper with an off-by-one in its bounds.

fn window_sums(xs: &[i32], width: usize) -> Vec<i32> {
    let mut sums = Vec::new();
    for start in 0..=xs.len() - width + 1 {
        let mut s = 0;
        for i in start..start + width {
            s += xs[i];
        }
        sums.push(s);
    }
    sums
}

fn main() {
    let data = vec![3, 1, 4, 1, 5, 9, 2, 6];
    println!("data: {:?}", data);
    println!("first: {}", data[0]);
    let sums = window_sums(&data, 3);
    println!("sums: {:?}", sums);
}
