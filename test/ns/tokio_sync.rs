// tokio's sync primitives on the script's event loop: a shared `Mutex` across spawned tasks,
// `timeout`, a channel that closes when the last spawned sender finishes, and `select!` (a
// losing `recv()` loses nothing, a refutable pattern disables its branch, `if` preconditions,
// `else`).
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{sleep, timeout, Duration};

async fn slow(ms: u64) -> u32 {
    sleep(Duration::from_millis(ms)).await;
    7
}

pub async fn main(ns: NS) {
    let total = Arc::new(Mutex::new(0u32));
    let mut handles = Vec::new();
    for i in 1..=3u32 {
        let t = Arc::clone(&total);
        handles.push(tokio::spawn(async move {
            sleep(Duration::from_millis(5 * i as u64)).await;
            *t.lock().await += i;
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    {
        let mut g = total.lock().await;
        *g *= 10;
    }
    ns.tprint(&format!("total {} {:?}", *total.lock().await, total));
    let names = RwLock::new(vec!["a".to_string()]);
    names.write().await.push("b".into());
    ns.tprint(&format!("{:?} {}", *names.read().await, names.read().await.len()));

    ns.tprint(&format!("{:?}", timeout(Duration::from_millis(100), slow(5)).await));
    match timeout(Duration::from_millis(5), slow(100)).await {
        Ok(v) => ns.tprint(&format!("ok {v}")),
        Err(e) => ns.tprint(&format!("err {e}")),
    }

    let (tx, mut rx) = mpsc::channel::<u32>(8);
    for i in 1..=3u32 {
        let tx = tx.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(10 * (4 - i) as u64)).await;
            tx.send(i).await.unwrap();
        });
    }
    drop(tx);
    let mut got = Vec::new();
    while let Some(v) = rx.recv().await {
        got.push(v);
    }
    ns.tprint(&format!("got {:?} {:?}", got, rx.try_recv()));

    let (utx, mut urx) = mpsc::unbounded_channel::<u32>();
    tokio::spawn(async move {
        for i in 0..3 {
            sleep(Duration::from_millis(15)).await;
            utx.send(i).unwrap();
        }
    });
    let mut ticks = 0;
    let mut seen = Vec::new();
    loop {
        tokio::select! {
            Some(v) = urx.recv() => {
                seen.push(v);
            }
            _ = sleep(Duration::from_millis(40)) => {
                ticks += 1;
                if ticks > 1 {
                    break;
                }
            }
        }
    }
    ns.tprint(&format!("seen {:?} ticks {}", seen, ticks));
    let which = tokio::select! {
        biased;
        v = slow(5) => format!("quick {v}"),
        _ = sleep(Duration::from_millis(50)) => "slow".to_string(),
    };
    ns.tprint(&which);
    let enabled = false;
    let r = tokio::select! {
        v = slow(1), if enabled => v,
        else => 99,
    };
    ns.tprint(&format!("{r}"));
}
