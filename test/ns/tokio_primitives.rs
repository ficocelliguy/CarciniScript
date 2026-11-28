// tokio's `oneshot`, `watch`, `Notify` and `time::interval` on the script's event loop.
use std::sync::Arc;
use tokio::sync::{oneshot, watch, Notify};
use tokio::time::{interval, sleep, Duration, Instant, MissedTickBehavior};

pub async fn main(ns: NS) {
    // oneshot: a value from a spawned task; a dropped sender; a closed receiver.
    let (tx, rx) = oneshot::channel::<u32>();
    tokio::spawn(async move {
        sleep(Duration::from_millis(10)).await;
        tx.send(42).unwrap();
    });
    ns.tprint(&format!("oneshot {:?}", rx.await));
    let (tx, rx) = oneshot::channel::<String>();
    tokio::spawn(async move {
        sleep(Duration::from_millis(5)).await;
        drop(tx);
    });
    match rx.await {
        Ok(v) => ns.tprint(&format!("got {v}")),
        Err(e) => ns.tprint(&format!("dropped: {e} {e:?}")),
    }
    let (tx, mut rx) = oneshot::channel::<u8>();
    ns.tprint(&format!("{:?}", rx.try_recv()));
    rx.close();
    ns.tprint(&format!("{} {:?}", tx.is_closed(), tx.send(9)));
    let (tx, mut rx) = oneshot::channel::<u8>();
    tx.send(3).unwrap();
    ns.tprint(&format!("{:?}", rx.try_recv()));

    // watch: the receiver sees the latest value; `changed` fails once the sender is gone.
    let (wtx, mut wrx) = watch::channel(0u32);
    let mut other = wtx.subscribe();
    tokio::spawn(async move {
        for i in 1..=3u32 {
            sleep(Duration::from_millis(10)).await;
            wtx.send(i * 10).unwrap();
        }
    });
    let mut seen = Vec::new();
    while wrx.changed().await.is_ok() {
        seen.push(*wrx.borrow_and_update());
    }
    ns.tprint(&format!("watch {:?} latest {} {:?}", seen, *other.borrow(), other.has_changed()));
    other.mark_unchanged();
    ns.tprint(&format!("{:?}", other.changed().await));
    let (wtx, wrx) = watch::channel("a".to_string());
    drop(wrx);
    ns.tprint(&format!("{} {}", wtx.is_closed(), wtx.send("b".into()).is_err()));

    // Notify: a stored permit, `notify_one` waking one waiter, `notify_waiters` all of them.
    let n = Notify::new();
    n.notify_one();
    n.notified().await;
    ns.tprint("permit used");
    let n = Arc::new(Notify::new());
    let hits = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let mut hs = Vec::new();
    for i in 0..3u32 {
        let (n, hits) = (n.clone(), hits.clone());
        hs.push(tokio::spawn(async move {
            n.notified().await;
            hits.lock().await.push(i);
        }));
    }
    sleep(Duration::from_millis(5)).await;
    n.notify_one();
    sleep(Duration::from_millis(5)).await;
    ns.tprint(&format!("after one {:?}", *hits.lock().await));
    n.notify_waiters();
    for h in hs {
        h.await.unwrap();
    }
    ns.tprint(&format!("after all {:?}", *hits.lock().await));

    // interval: the first tick is at once, then one each period.
    let start = Instant::now();
    let mut iv = interval(Duration::from_millis(50));
    iv.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut at = Vec::new();
    for _ in 0..4 {
        iv.tick().await;
        at.push((start.elapsed().as_millis() as f64 / 50.0).round() as u32);
    }
    ns.tprint(&format!("ticks {:?} {:?} {:?}", at, iv.period(), iv.missed_tick_behavior()));
}
