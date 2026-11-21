// (tokio prelude)
// The bundled `tokio` / `futures`, on the script's own event loop: `tokio::time::sleep(d).await`,
// `tokio::task::yield_now().await`, `tokio::spawn(fut)` (the task starts at once; awaiting its
// handle gives `Ok(value)`), `tokio::join!` and `futures::future::join_all`. `#[tokio::main]`
// is accepted on an `async fn main`.

/// `tokio::time::sleep(duration)`.
pub async fn __tokio_sleep(d: Duration) {
    js::sleep(d.as_secs_f64() * 1000.0).await;
}

/// `tokio::task::yield_now()`.
pub async fn __tokio_yield_now() {
    js::sleep(0.0).await;
}

/// `tokio::task::JoinError`: a spawned task can't fail here (a panic ends the script).
#[derive(Debug, Clone, PartialEq)]
pub struct TokioJoinError {}

impl std::fmt::Display for TokioJoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "task failed")
    }
}

impl std::error::Error for TokioJoinError {}

/// `tokio::sync::Mutex`: a `RefCell`, as `std::sync::Mutex` is (see the lock prelude). Locking
/// never waits; `m.lock().await` is the value in place (`codegen.rs` `lock_place`).
pub struct TokioMutex<T> {
    __c: std::cell::RefCell<T>,
}

impl<T> TokioMutex<T> {
    pub fn new(value: T) -> TokioMutex<T> {
        TokioMutex {
            __c: std::cell::RefCell::new(value),
        }
    }
    pub const fn const_new(value: T) -> TokioMutex<T> {
        TokioMutex {
            __c: std::cell::RefCell::new(value),
        }
    }
    pub async fn lock(&self) -> &mut T {
        self.__c.borrow_mut()
    }
    pub fn blocking_lock(&self) -> &mut T {
        self.__c.borrow_mut()
    }
    pub fn try_lock(&self) -> Result<&mut T, TokioTryLockError> {
        Ok(self.__c.borrow_mut())
    }
    pub fn get_mut(&mut self) -> &mut T {
        self.__c.borrow_mut()
    }
    pub fn into_inner(self) -> T {
        self.__c.into_inner()
    }
}

impl<T: Default> Default for TokioMutex<T> {
    fn default() -> TokioMutex<T> {
        TokioMutex::new(T::default())
    }
}

impl<T> From<T> for TokioMutex<T> {
    fn from(value: T) -> TokioMutex<T> {
        TokioMutex::new(value)
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for TokioMutex<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Mutex {{ data: {:?} }}", &*self.__c.borrow())
    }
}

/// `tokio::sync::RwLock`.
pub struct TokioRwLock<T> {
    __c: std::cell::RefCell<T>,
}

impl<T> TokioRwLock<T> {
    pub fn new(value: T) -> TokioRwLock<T> {
        TokioRwLock {
            __c: std::cell::RefCell::new(value),
        }
    }
    pub const fn const_new(value: T) -> TokioRwLock<T> {
        TokioRwLock {
            __c: std::cell::RefCell::new(value),
        }
    }
    pub async fn read(&self) -> &T {
        self.__c.borrow()
    }
    pub async fn write(&self) -> &mut T {
        self.__c.borrow_mut()
    }
    pub fn try_read(&self) -> Result<&T, TokioTryLockError> {
        Ok(self.__c.borrow())
    }
    pub fn try_write(&self) -> Result<&mut T, TokioTryLockError> {
        Ok(self.__c.borrow_mut())
    }
    pub fn get_mut(&mut self) -> &mut T {
        self.__c.borrow_mut()
    }
    pub fn into_inner(self) -> T {
        self.__c.into_inner()
    }
}

impl<T: Default> Default for TokioRwLock<T> {
    fn default() -> TokioRwLock<T> {
        TokioRwLock::new(T::default())
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for TokioRwLock<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "RwLock {{ data: {:?} }}", &*self.__c.borrow())
    }
}

/// `tokio::sync::TryLockError`: never made.
#[derive(Debug, Clone, PartialEq)]
pub struct TokioTryLockError {}

impl std::fmt::Display for TokioTryLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "operation would block")
    }
}

impl std::error::Error for TokioTryLockError {}

/// `tokio::time::error::Elapsed`.
#[derive(Debug, Clone, PartialEq)]
pub struct TokioElapsed {}

impl std::fmt::Display for TokioElapsed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "deadline has elapsed")
    }
}

impl std::error::Error for TokioElapsed {}

/// `tokio::time::timeout(d, fut)`: the future's value, or `Err(Elapsed)` once `d` has passed
/// first. (The future keeps running after a timeout: a JS promise can't be cancelled.)
pub async fn __tokio_timeout<T, F: std::future::Future<Output = T>>(
    d: Duration,
    fut: F,
) -> Result<T, TokioElapsed> {
    js::Promise::race(vec![
        async move { Ok(fut.await) },
        async move {
            js::sleep(d.as_secs_f64() * 1000.0).await;
            Err(TokioElapsed {})
        },
    ])
    .await
}

/// The queue a tokio channel's senders and receiver share, with the receiver's wakers. It closes
/// when the last sender is dropped (`drop(tx)`, or the end of the block that owns it) or the
/// receiver calls `close`.
pub struct TokioChan<T> {
    __q: std::collections::VecDeque<T>,
    __senders: usize,
    __closed: bool,
    __wakers: Vec<TaskWaker>,
}

impl<T> TokioChan<T> {
    fn __wake(&mut self) {
        for w in self.__wakers.drain(..) {
            w.wake();
        }
    }
}

/// `tokio::sync::mpsc::Sender` (a bounded channel's; the bound is ignored: sending never waits).
pub struct TokioSender<T> {
    __ch: std::rc::Rc<std::cell::RefCell<TokioChan<T>>>,
}

/// `tokio::sync::mpsc::UnboundedSender`.
pub struct TokioUnboundedSender<T> {
    __ch: std::rc::Rc<std::cell::RefCell<TokioChan<T>>>,
}

/// `tokio::sync::mpsc::Receiver` / `UnboundedReceiver`.
pub struct TokioReceiver<T> {
    __ch: std::rc::Rc<std::cell::RefCell<TokioChan<T>>>,
}

fn __tokio_new_chan<T>() -> std::rc::Rc<std::cell::RefCell<TokioChan<T>>> {
    std::rc::Rc::new(std::cell::RefCell::new(TokioChan {
        __q: std::collections::VecDeque::new(),
        __senders: 1,
        __closed: false,
        __wakers: Vec::new(),
    }))
}

/// `tokio::sync::mpsc::channel(buffer)`.
pub fn __tokio_channel<T>(_buffer: usize) -> (TokioSender<T>, TokioReceiver<T>) {
    let ch = __tokio_new_chan();
    (TokioSender { __ch: ch.clone() }, TokioReceiver { __ch: ch })
}

/// `tokio::sync::mpsc::unbounded_channel()`.
pub fn __tokio_unbounded_channel<T>() -> (TokioUnboundedSender<T>, TokioReceiver<T>) {
    let ch = __tokio_new_chan();
    (TokioUnboundedSender { __ch: ch.clone() }, TokioReceiver { __ch: ch })
}

fn __tokio_push<T>(ch: &std::rc::Rc<std::cell::RefCell<TokioChan<T>>>, value: T) -> Result<(), T> {
    let mut c = ch.borrow_mut();
    if c.__closed {
        return Err(value);
    }
    c.__q.push_back(value);
    c.__wake();
    Ok(())
}

fn __tokio_drop_sender<T>(ch: &std::rc::Rc<std::cell::RefCell<TokioChan<T>>>) {
    let mut c = ch.borrow_mut();
    c.__senders -= 1;
    if c.__senders == 0 {
        c.__wake();
    }
}

impl<T> TokioSender<T> {
    pub async fn send(&self, value: T) -> Result<(), TokioSendError<T>> {
        match __tokio_push(&self.__ch, value) {
            Ok(()) => Ok(()),
            Err(v) => Err(TokioSendError(v)),
        }
    }
    pub fn try_send(&self, value: T) -> Result<(), TokioTrySendError<T>> {
        match __tokio_push(&self.__ch, value) {
            Ok(()) => Ok(()),
            Err(v) => Err(TokioTrySendError::Closed(v)),
        }
    }
    pub fn blocking_send(&self, value: T) -> Result<(), TokioSendError<T>> {
        match __tokio_push(&self.__ch, value) {
            Ok(()) => Ok(()),
            Err(v) => Err(TokioSendError(v)),
        }
    }
    pub fn is_closed(&self) -> bool {
        self.__ch.borrow().__closed
    }
}

impl<T> TokioUnboundedSender<T> {
    pub fn send(&self, value: T) -> Result<(), TokioSendError<T>> {
        match __tokio_push(&self.__ch, value) {
            Ok(()) => Ok(()),
            Err(v) => Err(TokioSendError(v)),
        }
    }
    pub fn is_closed(&self) -> bool {
        self.__ch.borrow().__closed
    }
}

impl<T> Clone for TokioSender<T> {
    fn clone(&self) -> TokioSender<T> {
        self.__ch.borrow_mut().__senders += 1;
        TokioSender {
            __ch: self.__ch.clone(),
        }
    }
}

impl<T> Clone for TokioUnboundedSender<T> {
    fn clone(&self) -> TokioUnboundedSender<T> {
        self.__ch.borrow_mut().__senders += 1;
        TokioUnboundedSender {
            __ch: self.__ch.clone(),
        }
    }
}

impl<T> Drop for TokioSender<T> {
    fn drop(&mut self) {
        __tokio_drop_sender(&self.__ch);
    }
}

impl<T> Drop for TokioUnboundedSender<T> {
    fn drop(&mut self) {
        __tokio_drop_sender(&self.__ch);
    }
}

impl<T> std::fmt::Debug for TokioSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Sender {{ .. }}")
    }
}

impl<T> std::fmt::Debug for TokioUnboundedSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "UnboundedSender {{ .. }}")
    }
}

impl<T> TokioReceiver<T> {
    /// `rx.recv().await`: the next value, waiting for one; `None` once the channel is closed
    /// and empty. (A future of its own, so a `select!` that picks another branch loses nothing.)
    pub fn recv(&mut self) -> TokioRecv<T> {
        TokioRecv {
            __ch: self.__ch.clone(),
        }
    }
    pub fn try_recv(&mut self) -> Result<T, TokioTryRecvError> {
        let mut c = self.__ch.borrow_mut();
        match c.__q.pop_front() {
            Some(v) => Ok(v),
            None => {
                if c.__closed || c.__senders == 0 {
                    Err(TokioTryRecvError::Disconnected)
                } else {
                    Err(TokioTryRecvError::Empty)
                }
            }
        }
    }
    pub fn close(&mut self) {
        self.__ch.borrow_mut().__closed = true;
    }
    pub fn is_closed(&self) -> bool {
        let c = self.__ch.borrow();
        c.__closed || c.__senders == 0
    }
    pub fn is_empty(&self) -> bool {
        self.__ch.borrow().__q.is_empty()
    }
    pub fn len(&self) -> usize {
        self.__ch.borrow().__q.len()
    }
}

impl<T> std::fmt::Debug for TokioReceiver<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Receiver {{ .. }}")
    }
}

/// What `rx.recv()` returns.
pub struct TokioRecv<T> {
    __ch: std::rc::Rc<std::cell::RefCell<TokioChan<T>>>,
}

impl<T> std::future::Future for TokioRecv<T> {
    type Output = Option<T>;
    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext) -> TaskPoll<Option<T>> {
        let mut c = self.__ch.borrow_mut();
        if let Some(v) = c.__q.pop_front() {
            return TaskPoll::Ready(Some(v));
        }
        if c.__closed || c.__senders == 0 {
            return TaskPoll::Ready(None);
        }
        c.__wakers.push(cx.waker().clone());
        TaskPoll::Pending
    }
}

/// `tokio::sync::mpsc::error::SendError`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TokioSendError<T>(pub T);

impl<T> std::fmt::Debug for TokioSendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "SendError {{ .. }}")
    }
}

impl<T> std::fmt::Display for TokioSendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "channel closed")
    }
}

impl<T> std::error::Error for TokioSendError<T> {}

/// `tokio::sync::mpsc::error::TrySendError`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TokioTrySendError<T> {
    Full(T),
    Closed(T),
}

impl<T> std::fmt::Debug for TokioTrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TokioTrySendError::Full(_) => write!(f, "Full(..)"),
            TokioTrySendError::Closed(_) => write!(f, "Closed(..)"),
        }
    }
}

impl<T> std::fmt::Display for TokioTrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TokioTrySendError::Full(_) => write!(f, "no available capacity"),
            TokioTrySendError::Closed(_) => write!(f, "channel closed"),
        }
    }
}

/// `tokio::sync::mpsc::error::TryRecvError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokioTryRecvError {
    Empty,
    Disconnected,
}

impl std::fmt::Display for TokioTryRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TokioTryRecvError::Empty => write!(f, "receiving on an empty channel"),
            TokioTryRecvError::Disconnected => write!(f, "receiving on a closed channel"),
        }
    }
}

impl std::error::Error for TokioTryRecvError {}
