// Coding contract: "Sanitize Parentheses in Expression"
// Remove the minimum number of parens so the expression is valid, give every distinct answer.
use std::collections::{BTreeSet, HashSet, VecDeque};

fn is_valid(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

fn sanitize(input: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    let mut found = BTreeSet::new();

    seen.insert(input.to_string());
    queue.push_back(input.to_string());

    while let Some(cur) = queue.pop_front() {
        if is_valid(&cur) {
            found.insert(cur.clone());
        }
        // once we have something at this level, don't go deeper
        if !found.is_empty() {
            continue;
        }
        for (i, c) in cur.char_indices() {
            if c != '(' && c != ')' {
                continue;
            }
            let next = format!("{}{}", &cur[..i], &cur[i + 1..]);
            if seen.insert(next.clone()) {
                queue.push_back(next);
            }
        }
    }

    found.into_iter().collect()
}

fn main() {
    let cases = ["()())()", "(a)())()", ")(", "((a))", "(()a(((", "())(()(a)"];
    for case in cases {
        let answers = sanitize(case);
        println!("{:>12} -> {:?}", case, answers);
    }

    let total: usize = cases.iter().map(|c| sanitize(c).len()).sum();
    println!("total answers: {total}");
}
