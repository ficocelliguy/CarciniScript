// Hand-written futures: `impl Future for X` with `Pin<&mut Self>` / `Context` / `Poll`, wakers
// (`wake_by_ref`, `clone`, `Waker::noop`), `Pin::new`, `Box::pin`, `pin!`, `as_mut`, `get_mut`,
// a future polling another (hand-written, boxed or an async block's), `std::future::{ready,
// pending, poll_fn}`, `F: Future` bounds, and `.await` on them. A tiny executor drives them
// both natively and here; `run()` (an async fn) is awaited by the script's own `async fn main`.
use std::cell::RefCell;
use std::future::{self, Future};
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

// Ready after `n` polls, waking itself each time.
struct CountDown {
    n: u32,
    log: Rc<RefCell<Vec<String>>>,
}

impl Future for CountDown {
    type Output = &'static str;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
        if self.n == 0 {
            Poll::Ready("liftoff")
        } else {
            self.log.borrow_mut().push(format!("tick {}", self.n));
            self.n -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

// Ready once a shared flag is set; keeps a clone of the waker to wake later.
struct Gate {
    open: Rc<RefCell<bool>>,
    waker: Rc<RefCell<Option<Waker>>>,
}

impl Future for Gate {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<i32> {
        if *self.open.borrow() {
            Poll::Ready(42)
        } else {
            *self.waker.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

// Counts the polls of the future it wraps.
struct Counted<F> {
    inner: F,
    polls: u32,
}

impl<F: Future + Unpin> Future for Counted<F> {
    type Output = (F::Output, u32);
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        this.polls += 1;
        match Pin::new(&mut this.inner).poll(cx) {
            Poll::Ready(v) => Poll::Ready((v, this.polls)),
            Poll::Pending => Poll::Pending,
        }
    }
}

// Polls boxed futures together; ready with every output once all are.
struct JoinAll {
    futs: Vec<Pin<Box<dyn Future<Output = i32>>>>,
    outs: Vec<Option<i32>>,
}

impl Future for JoinAll {
    type Output = Vec<i32>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Vec<i32>> {
        let mut all = true;
        for i in 0..self.futs.len() {
            if self.outs[i].is_some() {
                continue;
            }
            match self.futs[i].as_mut().poll(cx) {
                Poll::Ready(v) => self.outs[i] = Some(v),
                Poll::Pending => all = false,
            }
        }
        if all {
            Poll::Ready(self.outs.iter().map(|o| o.unwrap()).collect())
        } else {
            Poll::Pending
        }
    }
}

fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

fn poll_once<F: Future + Unpin>(f: &mut F) -> Poll<F::Output> {
    let mut cx = Context::from_waker(Waker::noop());
    Pin::new(f).poll(&mut cx)
}

async fn add(a: i32, b: i32) -> i32 {
    a + b
}

async fn twice<F: Future<Output = i32>>(f: F) -> i32 {
    f.await * 2
}

async fn run() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let s = CountDown { n: 2, log: log.clone() }.await;
    println!("{s} after {:?}", log.borrow());
    println!("ready: {}", future::ready(7).await);
    let mut left = 3;
    let v = future::poll_fn(move |cx: &mut Context<'_>| {
        if left == 0 {
            Poll::Ready("polled out")
        } else {
            left -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
    println!("{v}");
    let (s, polls) = Counted { inner: CountDown { n: 3, log: log.clone() }, polls: 0 }.await;
    println!("{s} in {polls} polls");
    println!("twice: {}", twice(future::ready(21)).await);
    let all = JoinAll {
        futs: vec![
            Box::pin(add(1, 2)),
            Box::pin(async { 10 }),
            Box::pin(future::ready(5)),
        ],
        outs: vec![None, None, None],
    };
    println!("joined: {:?}", all.await);
    let boxed: Pin<Box<dyn Future<Output = &'static str>>> =
        Box::pin(CountDown { n: 1, log: log.clone() });
    println!("boxed: {}", boxed.await);
    println!("log: {}", log.borrow().len());
}

fn main_sync() {
    let log = Rc::new(RefCell::new(Vec::new()));
    println!("{}", block_on(CountDown { n: 3, log: log.clone() }));
    println!("{:?}", log.borrow());
    println!("{:?}", block_on(Counted { inner: CountDown { n: 2, log: log.clone() }, polls: 0 }));
    println!("{}", block_on(future::ready("now")));

    // A gate polled by hand, opened, then woken through the waker it kept.
    let open = Rc::new(RefCell::new(false));
    let kept = Rc::new(RefCell::new(None));
    let mut gate = Gate { open: open.clone(), waker: kept.clone() };
    println!("{:?}", poll_once(&mut gate));
    println!("waker kept: {}", kept.borrow().is_some());
    *open.borrow_mut() = true;
    let w = kept.borrow().clone();
    if let Some(w) = w {
        w.wake();
    }
    let p = poll_once(&mut gate);
    println!("{:?} {} {}", p, p.is_ready(), p.is_pending());
    println!("{:?}", p.map(|x| x + 1));
    let pend: Poll<i32> = Poll::Pending;
    println!("{:?} {:?}", pend, pend.map(|x| x * 2));

    let mut never = future::pending::<i32>();
    println!("{:?}", poll_once(&mut never));
    let mut cd = CountDown { n: 1, log: log.clone() };
    let mut pinned = Pin::new(&mut cd);
    let mut cx = Context::from_waker(Waker::noop());
    println!("{:?}", pinned.as_mut().poll(&mut cx));
    println!("{:?}", pinned.as_mut().poll(&mut cx));
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    main_sync();
    block_on(run());
}

#[cfg(target_arch = "wasm32")]
async fn main() {
    main_sync();
    run().await;
}
