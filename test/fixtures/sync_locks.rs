// std::sync's locks and atomics and thread_local!, as a single-threaded script has them.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{self, AtomicBool, AtomicI64, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

static COUNTER: AtomicUsize = AtomicUsize::new(0);
static RUNNING: AtomicBool = AtomicBool::new(true);
static CACHE: Mutex<Vec<String>> = Mutex::new(Vec::new());
static CONFIG: RwLock<Option<String>> = RwLock::new(None);

thread_local! {
    static SEEN: RefCell<HashMap<String, u32>> = RefCell::new(HashMap::new());
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

#[derive(Debug, Default)]
struct Stats {
    hacks: u32,
    money: f64,
}

struct Server {
    name: String,
    stats: Mutex<Stats>,
    hits: AtomicU32,
}

impl Server {
    fn record(&self, money: f64) {
        let mut s = self.stats.lock().unwrap();
        s.hacks += 1;
        s.money += money;
        self.hits.fetch_add(1, Ordering::Relaxed);
    }
}

fn bump() -> usize {
    COUNTER.fetch_add(1, Ordering::SeqCst) + 1
}

fn first_entry(m: &Mutex<Vec<String>>) -> MutexGuard<'_, Vec<String>> {
    m.lock().unwrap()
}

fn visit(name: &str) -> u32 {
    SEEN.with_borrow_mut(|s| {
        let n = s.entry(name.to_string()).or_insert(0);
        *n += 1;
        *n
    })
}

fn main() {
    bump();
    println!("bump {}", bump());
    println!("counter {}", COUNTER.load(Ordering::Relaxed));
    RUNNING.store(false, Ordering::Release);
    println!("running {}", RUNNING.load(Ordering::Acquire));
    atomic::fence(Ordering::SeqCst);

    CACHE.lock().unwrap().push("a".to_string());
    {
        let mut c = CACHE.lock().expect("lock");
        c.push("b".to_string());
        c.retain(|s| s != "zz");
    }
    println!("cache {:?}", CACHE.lock().unwrap());
    println!("first {:?}", first_entry(&CACHE).first());
    println!("len {}", CACHE.lock().unwrap().len());

    *CONFIG.write().unwrap() = Some("n00dles".to_string());
    if let Some(t) = CONFIG.read().unwrap().as_ref() {
        println!("target {t}");
    }

    let shared = Arc::new(Mutex::new(5));
    let s2 = Arc::clone(&shared);
    *s2.lock().unwrap() += 10;
    {
        let mut g = shared.lock().unwrap();
        *g *= 2;
    }
    println!("shared {}", *shared.lock().unwrap());
    println!("{:?}", shared);

    let rw = RwLock::new(vec![1, 2, 3]);
    println!("read {:?}", rw.read().unwrap());
    rw.write().unwrap().push(4);
    println!("len {}", rw.read().unwrap().len());
    println!("{:?}", rw);

    let srv = Server {
        name: "n00dles".into(),
        stats: Mutex::new(Stats::default()),
        hits: AtomicU32::new(0),
    };
    srv.record(1.5);
    srv.record(2.0);
    println!(
        "{} {:?} {}",
        srv.name,
        srv.stats.lock().unwrap(),
        srv.hits.load(Ordering::SeqCst)
    );

    println!("visit {}", visit("x"));
    println!("visit {}", visit("x"));
    println!("visit {}", visit("y"));
    SEEN.with(|s| println!("seen {}", s.borrow().len()));
    let x = SEEN.with(|s| s.borrow()["x"]);
    println!("x {x}");
    DEPTH.set(DEPTH.get() + 3);
    println!("depth {}", DEPTH.get());

    let a = AtomicU32::new(7);
    a.fetch_sub(2, Ordering::SeqCst);
    println!("swap {}", a.swap(9, Ordering::SeqCst));
    println!("cas {:?}", a.compare_exchange(9, 11, Ordering::SeqCst, Ordering::SeqCst));
    println!("cas {:?}", a.compare_exchange(9, 12, Ordering::SeqCst, Ordering::Relaxed));
    println!(
        "upd {:?}",
        a.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| Some(v * 2))
    );
    println!("max {} {:?}", a.fetch_max(100, Ordering::SeqCst), a);
    println!("into {}", a.into_inner());
    let neg = AtomicI64::default();
    neg.fetch_sub(5, Ordering::SeqCst);
    println!("neg {}", neg.load(Ordering::SeqCst));
    let flag = AtomicBool::new(false);
    println!("not {} {:?}", flag.fetch_not(Ordering::SeqCst), flag);

    let m = Mutex::new(vec![1]);
    if let Ok(mut g) = m.try_lock() {
        g.push(2);
    }
    println!("poisoned {}", m.is_poisoned());
    println!("{:?}", m.into_inner().unwrap());
    let mut m2 = Mutex::new(1);
    *m2.get_mut().unwrap() = 2;
    println!("{}", *m2.lock().unwrap());
    let d: Mutex<Vec<i32>> = Mutex::default();
    d.lock().unwrap().push(3);
    println!("{:?}", d.lock().unwrap());
}
