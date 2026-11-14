// `impl Index<I>` on your own types: a grid indexed by `(row, col)` and by row, a lookup table
// indexed by `&str`, method calls and fields on the result, and indexing in loops and closures.
use std::ops::Index;

struct Grid {
    w: usize,
    cells: Vec<char>,
}

impl Index<(usize, usize)> for Grid {
    type Output = char;
    fn index(&self, at: (usize, usize)) -> &char {
        &self.cells[at.0 * self.w + at.1]
    }
}

impl Index<usize> for Grid {
    type Output = [char];
    fn index(&self, row: usize) -> &[char] {
        &self.cells[row * self.w..(row + 1) * self.w]
    }
}

#[derive(Debug)]
struct Server {
    ram: u32,
}

struct Network {
    names: Vec<String>,
    servers: Vec<Server>,
}

impl Index<&str> for Network {
    type Output = Server;
    fn index(&self, name: &str) -> &Server {
        let i = self.names.iter().position(|n| n == name).expect("no such server");
        &self.servers[i]
    }
}

fn main() {
    let g = Grid { w: 3, cells: "abcdefghi".chars().collect() };
    println!("{} {} {}", g[(0, 0)], g[(1, 2)], g[(2, 1)]);
    println!("{:?} {}", &g[1], g[2].len());
    let diag: String = (0..3).map(|i| g[(i, i)]).collect();
    println!("{diag}");

    let net = Network {
        names: vec!["home".to_string(), "n00dles".to_string()],
        servers: vec![Server { ram: 8 }, Server { ram: 4 }],
    };
    println!("{:?} {}", net["n00dles"], net["home"].ram * 2);
}
