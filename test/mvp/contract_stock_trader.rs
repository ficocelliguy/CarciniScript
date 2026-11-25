// Coding contracts: "Subarray with Maximum Sum" and "Algorithmic Stock Trader I-IV".

fn max_subarray(nums: &[i64]) -> i64 {
    let mut best = nums[0];
    let mut current = nums[0];
    for &x in &nums[1..] {
        current = x.max(current + x);
        best = best.max(current);
    }
    best
}

/// At most `k` transactions; `None` for unlimited.
fn max_profit(prices: &[i32], k: Option<usize>) -> i32 {
    if prices.len() < 2 {
        return 0;
    }
    let k = k.unwrap_or(prices.len() / 2).min(prices.len() / 2);
    if k == 0 {
        return 0;
    }
    let mut buy = vec![i32::MIN / 2; k + 1];
    let mut sell = vec![0; k + 1];
    for &p in prices {
        for j in 1..=k {
            buy[j] = buy[j].max(sell[j - 1] - p);
            sell[j] = sell[j].max(buy[j] + p);
        }
    }
    sell[k]
}

fn main() {
    let arrays: Vec<Vec<i64>> = vec![
        vec![-2, 1, -3, 4, -1, 2, 1, -5, 4],
        vec![5, -9, 6, -2, 3],
        vec![-7, -3, -10],
    ];
    for a in &arrays {
        println!("max subarray of {:?} = {}", a, max_subarray(a));
    }

    let prices = [107, 58, 9, 59, 186, 191, 42, 7, 140, 133, 41, 95, 156, 113];
    println!("I:   {}", max_profit(&prices, Some(1)));
    println!("II:  {}", max_profit(&prices, None));
    println!("III: {}", max_profit(&prices, Some(2)));
    for k in [0, 3, 10] {
        println!("IV k={k}: {}", max_profit(&prices, Some(k)));
    }
}
