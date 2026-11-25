// Coding contract: "Merge Overlapping Intervals", input given as the game shows it.

fn parse(input: &str) -> Result<Vec<[u32; 2]>, String> {
    let inner = input.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']')).ok_or("not a list")?;
    let mut out = Vec::new();
    for part in inner.split("],") {
        let nums: Vec<&str> = part.trim_matches(|c| c == '[' || c == ']' || c == ' ').split(',').collect();
        if nums.len() != 2 {
            return Err(format!("bad interval {part:?}"));
        }
        let a = nums[0].trim().parse::<u32>().map_err(|e| e.to_string())?;
        let b = nums[1].trim().parse::<u32>().map_err(|e| e.to_string())?;
        out.push([a, b]);
    }
    Ok(out)
}

fn merge(mut intervals: Vec<[u32; 2]>) -> Vec<[u32; 2]> {
    intervals.sort_by_key(|iv| iv[0]);
    let mut merged: Vec<[u32; 2]> = Vec::with_capacity(intervals.len());
    for iv in intervals {
        if let Some(last) = merged.last_mut() {
            if iv[0] <= last[1] {
                last[1] = last[1].max(iv[1]);
                continue;
            }
        }
        merged.push(iv);
    }
    merged
}

fn main() {
    let inputs = [
        "[[1,3],[8,10],[2,6],[10,16]]",
        "[[15,17],[2,10],[23,26],[5,9],[1,4]]",
        "[[1, 2]]",
        "[[1,2],[x,4]]",
        "[1,2,3]",
        "1,2",
    ];
    for input in inputs {
        match parse(input) {
            Ok(iv) => println!("{:?}", merge(iv)),
            Err(e) => println!("error: {}", e),
        }
    }
}
