// Coding contract: "Proper 2-Coloring of a Graph"
// Answer is a list of 0/1 per vertex, or [] if the graph isn't bipartite.
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Color {
    Uncolored,
    Red,
    Blue,
}

impl Color {
    fn other(self) -> Color {
        match self {
            Color::Red => Color::Blue,
            Color::Blue => Color::Red,
        }
    }
}

struct Graph {
    adj: Vec<Vec<usize>>,
}

impl Graph {
    fn new(n: usize, edges: &[(usize, usize)]) -> Graph {
        let mut adj = vec![Vec::new(); n];
        for &(a, b) in edges {
            adj[a].push(b);
            adj[b].push(a);
        }
        Graph { adj }
    }

    fn two_color(&self) -> Option<Vec<u8>> {
        let n = self.adj.len();
        let mut colors = vec![Color::Uncolored; n];
        for start in 0..n {
            if colors[start] != Color::Uncolored {
                continue;
            }
            colors[start] = Color::Red;
            let mut queue = VecDeque::from([start]);
            while let Some(v) = queue.pop_front() {
                for &w in &self.adj[v] {
                    if colors[w] == Color::Uncolored {
                        colors[w] = colors[v].other();
                        queue.push_back(w);
                    } else if colors[w] == colors[v] {
                        return None;
                    }
                }
            }
        }
        Some(colors.iter().map(|&c| if c == Color::Red { 0 } else { 1 }).collect())
    }
}

fn main() {
    let cases: Vec<(usize, Vec<(usize, usize)>)> = vec![
        (4, vec![(0, 2), (0, 3), (1, 2), (1, 3)]),
        (3, vec![(0, 1), (1, 2), (2, 0)]),
        (6, vec![(0, 1), (2, 3), (3, 4), (4, 5)]),
        (8, vec![(1, 6), (0, 6), (2, 7), (5, 7), (3, 4), (4, 6), (1, 5)]),
    ];

    for (n, edges) in &cases {
        let g = Graph::new(*n, edges);
        match g.two_color() {
            Some(colors) => println!("{} vertices: {:?}", n, colors),
            None => println!("{} vertices: []", n),
        }
    }
}
