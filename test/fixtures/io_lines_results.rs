// `BufRead::lines()` yields `io::Result<String>`, and `io::Lines<B>` names that iterator.
use std::io::{self, BufRead, BufReader, Cursor};

fn read_lines(text: &str) -> io::Result<io::Lines<io::BufReader<Cursor<String>>>> {
    Ok(io::BufReader::new(Cursor::new(text.to_string())).lines())
}

fn main() {
    let text = "one\ntwo\nthree\n";
    if let Ok(lines) = read_lines(text) {
        for line in lines.flatten() {
            println!("{}", line);
        }
    }
    for line in BufReader::new(Cursor::new(text)).lines().map_while(Result::ok) {
        println!("> {}", line);
    }
    for line in BufReader::new(Cursor::new(text)).lines() {
        let line = line.unwrap();
        println!("{}", line.len());
    }
    let v: Vec<String> = BufReader::new(Cursor::new(text))
        .lines()
        .collect::<Result<_, _>>()
        .unwrap();
    println!("{:?}", v);
    let rs = vec![Ok(1), Err("bad"), Ok(3)];
    let oks: Vec<i32> = rs.into_iter().flatten().collect();
    println!("{:?}", oks);
}
