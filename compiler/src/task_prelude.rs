// (task prelude)
// `std::task` and `std::future`, for hand-written futures (`impl Future for X { fn poll(..) }`),
// compiled in when a program names them. A rewrite (`bundled.rs`) gives these the `Task` /
// `Future` names below. `.await` on such a value (or on one passed as an `F: Future`) polls it
// from the runtime (`κσ.$drive`): each `Pending` waits until the context's waker is woken, so a
// future that never wakes it never finishes, as in Rust. `poll` on an async fn's or block's
// future (a JS promise) is `Pending` until the promise settles, then wakes the waker.

/// `std::task::Poll`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TaskPoll<T> {
    Ready(T),
    Pending,
}

impl<T> TaskPoll<T> {
    pub fn is_ready(&self) -> bool {
        match self {
            TaskPoll::Ready(_) => true,
            TaskPoll::Pending => false,
        }
    }
    pub fn is_pending(&self) -> bool {
        !self.is_ready()
    }
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> TaskPoll<U> {
        match self {
            TaskPoll::Ready(t) => TaskPoll::Ready(f(t)),
            TaskPoll::Pending => TaskPoll::Pending,
        }
    }
}

impl<T> From<T> for TaskPoll<T> {
    fn from(t: T) -> TaskPoll<T> {
        TaskPoll::Ready(t)
    }
}

/// `std::task::Waker`: wakes its task (the function the runtime gave it).
pub struct TaskWaker {
    __wake: std::rc::Rc<dyn Fn()>,
}

impl TaskWaker {
    pub fn wake(self) {
        (self.__wake)();
    }
    pub fn wake_by_ref(&self) {
        (self.__wake)();
    }
    pub fn will_wake(&self, _other: &TaskWaker) -> bool {
        false
    }
    /// `Waker::noop()`: wakes nothing.
    pub fn noop() -> TaskWaker {
        TaskWaker {
            __wake: std::rc::Rc::new(|| {}),
        }
    }
    pub fn __of(wake: std::rc::Rc<dyn Fn()>) -> TaskWaker {
        TaskWaker { __wake: wake }
    }
}

impl Clone for TaskWaker {
    // (The clone wakes the same task.)
    fn clone(&self) -> TaskWaker {
        TaskWaker::__of(self.__wake.clone())
    }
}

impl std::fmt::Debug for TaskWaker {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Waker {{ .. }}")
    }
}

/// `std::task::Context`: what `poll` gets, holding the task's waker.
pub struct TaskContext {
    __waker: TaskWaker,
}

impl TaskContext {
    pub fn from_waker(waker: &TaskWaker) -> TaskContext {
        TaskContext {
            __waker: waker.clone(),
        }
    }
    pub fn waker(&self) -> &TaskWaker {
        &self.__waker
    }
}

impl std::fmt::Debug for TaskContext {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Context {{ waker: Waker {{ .. }} }}")
    }
}

/// The context the runtime polls a hand-written future with, waking it by calling `wake`.
pub fn __task_context(wake: std::rc::Rc<dyn Fn()>) -> TaskContext {
    TaskContext {
        __waker: TaskWaker::__of(wake),
    }
}

/// `std::future::Ready` (`std::future::ready(v)`): ready at once with `v`.
#[derive(Debug, Clone)]
pub struct FutureReady<T> {
    __v: Option<T>,
}

impl<T> FutureReady<T> {
    pub fn into_inner(self) -> T {
        self.__v.expect("Called `into_inner()` on `Ready` after completion")
    }
}

impl<T> std::future::Future for FutureReady<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, _cx: &mut TaskContext) -> TaskPoll<T> {
        TaskPoll::Ready(self.__v.take().expect("`Ready` polled after completion"))
    }
}

pub fn __future_ready<T>(v: T) -> FutureReady<T> {
    FutureReady { __v: Some(v) }
}

/// `std::future::Pending` (`std::future::pending()`): never ready.
#[derive(Debug)]
pub struct FuturePending<T> {
    __t: Option<T>,
}

impl<T> std::future::Future for FuturePending<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, _cx: &mut TaskContext) -> TaskPoll<T> {
        TaskPoll::Pending
    }
}

pub fn __future_pending<T>() -> FuturePending<T> {
    FuturePending { __t: None }
}

/// `std::future::PollFn` (`std::future::poll_fn(f)`): polls by calling `f`.
pub struct FuturePollFn<T> {
    __f: Box<dyn FnMut(&mut TaskContext) -> TaskPoll<T>>,
}

impl<T> std::future::Future for FuturePollFn<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext) -> TaskPoll<T> {
        (self.__f)(cx)
    }
}

impl<T> std::fmt::Debug for FuturePollFn<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "PollFn")
    }
}

pub fn __future_poll_fn<T, F: FnMut(&mut TaskContext) -> TaskPoll<T> + 'static>(
    f: F,
) -> FuturePollFn<T> {
    FuturePollFn { __f: Box::new(f) }
}
