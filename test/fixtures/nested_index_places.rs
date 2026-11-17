// `grid[i][j]` as a place and as a value: compound assignment (the row is read once), plain
// assignment, reads in expressions, three levels, tuple and struct elements, strings in nested
// vectors (`push_str` needs the place), `&mut` borrows of elements, and the `for` / `match`
// subjects.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cell {
    v: i32,
    hits: u32,
}

fn main() {
    let n = 4;
    let mut grid = vec![vec![0i64; n]; n];
    for r in 0..3 {
        for i in 0..n {
            for j in 0..n {
                grid[i][j] += ((i * j + r) % 7) as i64;
                grid[j][i] -= 1;
            }
        }
    }
    grid[1][2] = grid[2][1] * 10 + grid[0][0];
    grid[3][3] *= grid[1][1] - 2;
    let total: i64 = grid.iter().map(|row| row.iter().sum::<i64>()).sum();
    println!("{:?} {}", grid, total);
    println!("{} {}", grid[1][2] + grid[2][1], grid[3][grid[0][1] as usize % n]);

    let mut cube = vec![vec![vec![1u32; 2]; 2]; 2];
    cube[1][0][1] += 5;
    cube[0][1][0] = cube[1][0][1] * 2;
    cube[1][1].push(9);
    cube[0][0][1] <<= 3;
    println!("{:?} {}", cube, cube[1][1].len() + cube[0][1][0] as usize);

    let mut pairs = vec![vec![(1, 'a'); 2]; 2];
    pairs[0][1].0 += 4;
    pairs[1][0].1 = 'z';
    println!("{:?} {}", pairs, pairs[0][1].0);

    let mut cells = vec![vec![Cell { v: 0, hits: 0 }; 2]; 3];
    for i in 0..3 {
        cells[i][i % 2].v += i as i32 * 3;
        cells[i][i % 2].hits += 1;
        let c = &mut cells[i][0];
        c.hits += 10;
    }
    cells[2][1] = Cell { v: -1, hits: 99 };
    println!("{:?}", cells);
    println!("{} {}", cells[0][0] == cells[0][1], cells[2][1].v);

    let mut names = vec![vec![String::from("a"); 2]; 2];
    names[0][1].push_str("bc");
    names[1][0] += "x";
    names[1][1] = names[0][1].clone() + &names[1][0];
    let len = names[0][1].len() + names[1][1].len();
    println!("{:?} {}", names, len);

    let mut nums = vec![vec![1, 2, 3], vec![4, 5]];
    {
        let r = &mut nums[1][0];
        *r += 100;
    }
    nums[0][2] = nums[1][1];
    let last = nums[1][nums[1].len() - 1];
    for x in &nums[0] {
        print!("{} ", x);
    }
    println!("{} {:?}", last, nums);
    match nums[1][0] {
        104 => println!("hundred-four"),
        _ => println!("other"),
    }
    let swapped = nums[0][0] > nums[1][0];
    println!("{} {}", swapped, nums[0][1] * nums[1][1] - nums[0][2]);
}
