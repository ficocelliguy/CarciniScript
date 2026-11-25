// Coding contract: "Minimum Path Sum in a Triangle"
// Input comes in as the JSON-ish text the game shows, e.g. [[2],[3,4],[6,5,7],[4,1,8,3]]

#[derive(Debug)]
struct Triangle {
    rows: Vec<Vec<i64>>,
}

impl Triangle {
    fn parse(text: &str) -> Result<Triangle, String> {
        let inner = text
            .trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or_else(|| format!("not a list: {text}"))?;

        let mut rows = Vec::new();
        for chunk in inner.split(']') {
            let chunk = chunk.trim_start_matches(|c: char| c == ',' || c == '[' || c.is_whitespace());
            if chunk.is_empty() {
                continue;
            }
            let row = chunk
                .split(',')
                .map(|n| n.trim().parse::<i64>().map_err(|e| format!("bad number {n:?}: {e}")))
                .collect::<Result<Vec<_>, _>>()?;
            rows.push(row);
        }

        for (i, row) in rows.iter().enumerate() {
            if row.len() != i + 1 {
                return Err(format!("row {i} has {} entries, expected {}", row.len(), i + 1));
            }
        }
        Ok(Triangle { rows })
    }

    // bottom-up DP: best[j] is the cheapest path from row i, column j to the bottom
    fn min_path(&self) -> i64 {
        let Some(last) = self.rows.last() else {
            return 0;
        };
        let mut best = last.clone();
        for row in self.rows.iter().rev().skip(1) {
            best = row
                .iter()
                .enumerate()
                .map(|(j, v)| v + best[j].min(best[j + 1]))
                .collect();
        }
        best[0]
    }
}

fn main() {
    let inputs = [
        "[[2],[3,4],[6,5,7],[4,1,8,3]]",
        "[[1],[2,3],[3,6,7],[8,9,6,1]]",
        "[\n [7],\n [6,3],\n [5,8,1],\n [9,7,2,4],\n [4,9,9,6,8]\n]",
        "[[1],[2,3,4]]",
        "[[-1],[x,2]]",
    ];

    for (n, input) in inputs.iter().enumerate() {
        match Triangle::parse(input) {
            Ok(tri) => {
                println!(
                    "#{:<2} rows={:>2} answer={:>4}",
                    n + 1,
                    tri.rows.len(),
                    tri.min_path()
                );
            }
            Err(e) => println!("#{:<2} error: {}", n + 1, e),
        }
    }
}
