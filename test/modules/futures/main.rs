// A hand-written future from a `mod` file, awaited and polled by an executor there.
mod timer;

use timer::{CountDown, block_on};

async fn run() {
    let v = CountDown::new(3).await;
    println!("awaited {v}");
    println!("blocked on {}", block_on(CountDown::new(1)));
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    block_on(run());
}

#[cfg(target_arch = "wasm32")]
async fn main() {
    run().await;
}
