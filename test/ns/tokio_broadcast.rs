// tokio's `broadcast`, `watch::Sender::send_modify` / `send_if_modified` / `send_replace` and
// `time::interval_at`, checked against tokio 1.54.
use tokio::sync::broadcast::{self, error::RecvError, error::TryRecvError};
use tokio::sync::watch;
use tokio::time::{interval_at, sleep, Duration, Instant};

pub async fn main(ns: NS) {
    // broadcast: every receiver sees every value; a late subscriber only later ones.
    let (tx, mut rx1) = broadcast::channel::<u32>(4);
    let rx2 = tx.subscribe();
    ns.tprint(&format!("sent to {:?}", tx.send(1)));
    let mut rx3 = tx.subscribe();
    ns.tprint(&format!("sent to {:?} receivers {}", tx.send(2), tx.receiver_count()));
    ns.tprint(&format!("len {} {} {}", tx.len(), rx1.len(), rx3.len()));
    ns.tprint(&format!("{:?} {:?} {:?}", rx1.recv().await, rx1.recv().await, rx1.try_recv()));
    ns.tprint(&format!("{:?} {:?}", rx3.recv().await, rx3.try_recv()));
    ns.tprint(&format!("tx len {}", tx.len()));
    drop(rx2);
    ns.tprint(&format!("tx len after drop {}", tx.len()));
    // Waiting receivers in spawned tasks.
    let mut hs = Vec::new();
    for id in 0..2u32 {
        let mut rx = tx.subscribe();
        hs.push(tokio::spawn(async move {
            let mut got = Vec::new();
            loop {
                match rx.recv().await {
                    Ok(v) => got.push(v * 10 + id),
                    Err(e) => {
                        return format!("{id}: {got:?} then {e} {e:?}");
                    }
                }
            }
        }));
    }
    for i in 1..=3u32 {
        sleep(Duration::from_millis(5)).await;
        tx.send(i).unwrap();
    }
    let tx2 = tx.clone();
    drop(tx);
    tx2.send(9).unwrap();
    drop(tx2);
    for h in hs {
        ns.tprint(&format!("{}", h.await.unwrap()));
    }
    ns.tprint(&format!("{:?} {}", rx1.try_recv(), rx1.is_closed()));
    // Lagging: capacity 3 rounds up to 4.
    let (tx, mut rx) = broadcast::channel::<String>(3);
    for i in 0..7 {
        tx.send(format!("m{i}")).unwrap();
    }
    let r = rx.recv().await;
    ns.tprint(&format!("{:?}", r));
    if let Err(RecvError::Lagged(n)) = r {
        ns.tprint(&format!("lagged {n}"));
    }
    ns.tprint(&format!("{:?} {:?}", rx.recv().await, rx.try_recv()));
    for i in 7..20 {
        tx.send(format!("m{i}")).unwrap();
    }
    match rx.try_recv() {
        Err(TryRecvError::Lagged(n)) => ns.tprint(&format!("try lagged {n}: {}", TryRecvError::Lagged(n))),
        other => ns.tprint(&format!("{other:?}")),
    }
    let mut again = rx.resubscribe();
    ns.tprint(&format!("{:?} {:?}", rx.try_recv(), again.try_recv()));
    drop(rx);
    drop(again);
    match tx.send("x".into()) {
        Err(e) => ns.tprint(&format!("send err {} {:?}", e, e)),
        Ok(n) => ns.tprint(&format!("{n}")),
    }
    let tx = broadcast::Sender::<u8>::new(2);
    let mut r = tx.subscribe();
    tx.send(5).unwrap();
    ns.tprint(&format!("{:?} {}", r.recv().await, tx.is_empty()));

    // watch: send_modify, send_if_modified, send_replace.
    let (wtx, mut wrx) = watch::channel(vec![1u32]);
    wtx.send_modify(|v| v.push(2));
    ns.tprint(&format!("{:?} {:?}", wrx.has_changed(), *wrx.borrow_and_update()));
    let changed = wtx.send_if_modified(|v| {
        if v.len() > 5 {
            v.clear();
            true
        } else {
            false
        }
    });
    ns.tprint(&format!("{} {:?}", changed, wrx.has_changed()));
    let old = wtx.send_replace(vec![7]);
    ns.tprint(&format!("{:?} {:?} {:?}", old, wrx.has_changed(), *wrx.borrow()));
    let (ctx, mut crx) = watch::channel(0u32);
    tokio::spawn(async move {
        for _ in 0..3 {
            sleep(Duration::from_millis(5)).await;
            ctx.send_modify(|n| *n += 1);
        }
    });
    let mut counts = Vec::new();
    while crx.changed().await.is_ok() {
        counts.push(*crx.borrow_and_update());
    }
    ns.tprint(&format!("counts {:?}", counts));
    drop(wrx);
    ns.tprint(&format!("{:?} {:?}", wtx.send_replace(vec![]), wtx.is_closed()));

    // interval_at: the first tick at `start`, then one each period.
    let start = Instant::now();
    let mut iv = interval_at(start + Duration::from_millis(60), Duration::from_millis(30));
    let mut at = Vec::new();
    for _ in 0..3 {
        iv.tick().await;
        at.push((start.elapsed().as_millis() as f64 / 30.0).round() as u32);
    }
    ns.tprint(&format!("ticks {:?}", at));
}
