// Coding contract: "Spiralize Matrix".
fn spiral(matrix: &Vec<Vec<i32>>) -> Vec<i32> {
    let mut out = Vec::new();
    if matrix.is_empty() {
        return out;
    }
    let (mut top, mut bottom) = (0i32, matrix.len() as i32 - 1);
    let (mut left, mut right) = (0i32, matrix[0].len() as i32 - 1);
    while top <= bottom && left <= right {
        for c in left..=right {
            out.push(matrix[top as usize][c as usize]);
        }
        top += 1;
        for r in top..=bottom {
            out.push(matrix[r as usize][right as usize]);
        }
        right -= 1;
        if top <= bottom {
            for c in (left..=right).rev() {
                out.push(matrix[bottom as usize][c as usize]);
            }
            bottom -= 1;
        }
        if left <= right {
            for r in (top..=bottom).rev() {
                out.push(matrix[r as usize][left as usize]);
            }
            left += 1;
        }
    }
    out
}

fn main() {
    let a = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];
    let b = vec![vec![1, 2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11, 12]];
    let c = vec![vec![33, 8], vec![4, 27], vec![15, 49], vec![2, 1]];
    for m in [&a, &b, &c] {
        let s = spiral(m);
        println!("{}", s.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
    }
    println!("{:?}", spiral(&vec![]));
}
