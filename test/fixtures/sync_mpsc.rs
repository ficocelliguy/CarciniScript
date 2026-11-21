// std::sync::mpsc on one thread: a shared queue; an empty channel's recv is an error, its
// iterators stop.
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

struct Worker {
    tx: Sender<String>,
}

impl Worker {
    fn report(&self, s: &str) {
        self.tx.send(s.to_string()).unwrap();
    }
}

fn drain(rx: &Receiver<String>) -> Vec<String> {
    rx.try_iter().collect()
}

fn main() {
    let (tx, rx) = mpsc::channel();
    let w = Worker { tx: tx.clone() };
    w.report("a");
    tx.send("b".to_string()).unwrap();
    println!("{:?}", rx.recv());
    println!("{:?}", drain(&rx));
    println!("{:?}", rx.try_recv());
    match rx.try_recv() {
        Err(TryRecvError::Empty) => println!("empty: {}", TryRecvError::Empty),
        other => println!("{:?}", other),
    }
    let (itx, irx) = mpsc::sync_channel::<i32>(4);
    for i in 0..3 {
        itx.send(i * 10).unwrap();
    }
    drop(itx);
    let total: i32 = irx.iter().sum();
    println!("total {total}");
    let (t2, r2) = mpsc::channel::<u8>();
    t2.send(1).unwrap();
    t2.send(2).unwrap();
    drop(t2);
    for v in r2 {
        println!("v {v}");
    }
}
