// Float-heavy nested loops. JS doubles must match native f64 bit for bit.
fn escape_time(cx: f64, cy: f64, limit: u32) -> u32 {
    let mut x = 0.0;
    let mut y = 0.0;
    let mut i = 0;
    while i < limit && x * x + y * y <= 4.0 {
        let xt = x * x - y * y + cx;
        y = 2.0 * x * y + cy;
        x = xt;
        i += 1;
    }
    i
}

fn main() {
    let width = 64;
    let height = 24;
    let mut total: u64 = 0;
    for row in 0..height {
        let mut line = String::new();
        for col in 0..width {
            let cx = -2.2 + 3.2 * (col as f64) / (width as f64);
            let cy = -1.2 + 2.4 * (row as f64) / (height as f64);
            let t = escape_time(cx, cy, 60);
            total += t as u64;
            let ch = if t >= 60 {
                '#'
            } else if t > 12 {
                '+'
            } else if t > 5 {
                '.'
            } else {
                ' '
            };
            line.push(ch);
        }
        println!("{}", line.trim_end());
    }
    println!("total iterations: {total}");
}
