// The bundled tokio (tasks on the script's event loop) and num-integer in an NS script: spawned
// tasks run together, `join!` starts every future first, `join_all` keeps the order given.
use num::integer::{gcd, lcm};
use num::Integer;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

async fn work(ns: &NS, id: u32, ms: u64) -> u32 {
    sleep(Duration::from_millis(ms)).await;
    ns.tprint(&format!("done {}", id));
    id * 10
}

pub async fn main(ns: NS) {
    let a = tokio::spawn(work(&ns, 1, 30));
    let b = tokio::spawn(async { 7 });
    ns.tprint(&format!("{} {}", a.await.unwrap(), b.await.unwrap()));
    let (x, y) = tokio::join!(work(&ns, 3, 20), work(&ns, 4, 1));
    ns.tprint(&format!("{} {}", x, y));
    let handles: Vec<JoinHandle<u32>> = (5..7).map(|i| tokio::spawn(work(&ns, i, 1))).collect();
    let mut total = 0;
    for h in handles {
        total += h.await.unwrap();
    }
    ns.tprint(&format!("total {}", total));
    let all = futures::future::join_all(vec![work(&ns, 9, 15), work(&ns, 10, 1)]).await;
    ns.tprint(&format!("{:?}", all));
    ns.tprint(&format!("{} {} {}", gcd(12, 18), lcm(4u64, 6), gcd(-8i32, 12)));
    ns.tprint(&format!(
        "{} {} {} {:?}",
        7i32.div_floor(&-2),
        (-7i32).mod_floor(&3),
        9u8.is_odd(),
        17i32.div_rem(&5)
    ));
}
