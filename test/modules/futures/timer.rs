use std::future::Future;
use std::pin::{Pin, pin};
use std::task::{Context, Poll, Waker};

pub struct CountDown {
    n: u32,
    polls: u32,
}

impl CountDown {
    pub fn new(n: u32) -> CountDown {
        CountDown { n, polls: 0 }
    }
}

impl Future for CountDown {
    type Output = u32;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        self.polls += 1;
        if self.n == 0 {
            Poll::Ready(self.polls)
        } else {
            self.n -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}
