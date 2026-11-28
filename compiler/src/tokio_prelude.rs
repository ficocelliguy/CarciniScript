// (tokio prelude)
// The bundled `tokio` / `futures`, on the script's own event loop: `tokio::time::sleep(d).await`,
// `tokio::task::yield_now().await`, `tokio::spawn(fut)` (the task starts at once; awaiting its
// handle gives `Ok(value)`), `tokio::join!` and `futures::future::join_all`. `#[tokio::main]`
// is accepted on an `async fn main`, and `#[tokio::test]` is a `#[test]` the test build
// awaits. Also `sync::oneshot`, `sync::watch`, `sync::broadcast`, `sync::Notify` and
// `time::interval` / `interval_at`.

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

/// `tokio::sync::oneshot::error::RecvError` / `tokio::sync::watch::error::RecvError`: the
/// sender went away.
#[derive(Clone, PartialEq, Eq)]
pub struct TokioRecvError {}

impl std::fmt::Debug for TokioRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "RecvError(())")
    }
}

impl std::fmt::Display for TokioRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "channel closed")
    }
}

impl std::error::Error for TokioRecvError {}

/// `tokio::sync::oneshot::error::TryRecvError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokioOneshotTryRecvError {
    Empty,
    Closed,
}

impl std::fmt::Display for TokioOneshotTryRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TokioOneshotTryRecvError::Empty => write!(f, "channel empty"),
            TokioOneshotTryRecvError::Closed => write!(f, "channel closed"),
        }
    }
}

impl std::error::Error for TokioOneshotTryRecvError {}

/// What a oneshot channel's two ends share: the value (at most one), whether the sender is gone
/// and whether the receiver closed, and the receiver's wakers.
pub struct TokioOneshotSlot<T> {
    __v: Vec<T>,
    __sender_gone: bool,
    __closed: bool,
    __wakers: Vec<TaskWaker>,
}

/// `tokio::sync::oneshot::Sender`.
pub struct TokioOneshotSender<T> {
    __s: std::rc::Rc<std::cell::RefCell<TokioOneshotSlot<T>>>,
}

/// `tokio::sync::oneshot::Receiver`: a future of `Result<T, RecvError>` (`rx.await`).
pub struct TokioOneshotReceiver<T> {
    __s: std::rc::Rc<std::cell::RefCell<TokioOneshotSlot<T>>>,
}

/// `tokio::sync::oneshot::channel()`.
pub fn __tokio_oneshot_channel<T>() -> (TokioOneshotSender<T>, TokioOneshotReceiver<T>) {
    let s = std::rc::Rc::new(std::cell::RefCell::new(TokioOneshotSlot {
        __v: Vec::new(),
        __sender_gone: false,
        __closed: false,
        __wakers: Vec::new(),
    }));
    (TokioOneshotSender { __s: s.clone() }, TokioOneshotReceiver { __s: s })
}

impl<T> TokioOneshotSender<T> {
    /// The value to the receiver, or back as `Err` once the receiver has closed.
    pub fn send(self, value: T) -> Result<(), T> {
        let mut s = self.__s.borrow_mut();
        if s.__closed {
            return Err(value);
        }
        s.__v.push(value);
        for w in s.__wakers.drain(..) {
            w.wake();
        }
        Ok(())
    }
    pub fn is_closed(&self) -> bool {
        self.__s.borrow().__closed
    }
}

impl<T> Drop for TokioOneshotSender<T> {
    fn drop(&mut self) {
        let mut s = self.__s.borrow_mut();
        s.__sender_gone = true;
        for w in s.__wakers.drain(..) {
            w.wake();
        }
    }
}

impl<T> std::fmt::Debug for TokioOneshotSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Sender {{ .. }}")
    }
}

impl<T> TokioOneshotReceiver<T> {
    pub fn try_recv(&mut self) -> Result<T, TokioOneshotTryRecvError> {
        let mut s = self.__s.borrow_mut();
        match s.__v.pop() {
            Some(v) => Ok(v),
            None => {
                if s.__sender_gone || s.__closed {
                    Err(TokioOneshotTryRecvError::Closed)
                } else {
                    Err(TokioOneshotTryRecvError::Empty)
                }
            }
        }
    }
    /// Stops the sender from sending; a value sent before stays receivable.
    pub fn close(&mut self) {
        self.__s.borrow_mut().__closed = true;
    }
}

impl<T> std::future::Future for TokioOneshotReceiver<T> {
    type Output = Result<T, TokioRecvError>;
    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext) -> TaskPoll<Result<T, TokioRecvError>> {
        let mut s = self.__s.borrow_mut();
        if let Some(v) = s.__v.pop() {
            return TaskPoll::Ready(Ok(v));
        }
        if s.__sender_gone || s.__closed {
            return TaskPoll::Ready(Err(TokioRecvError {}));
        }
        s.__wakers.push(cx.waker().clone());
        TaskPoll::Pending
    }
}

impl<T> std::fmt::Debug for TokioOneshotReceiver<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Receiver {{ .. }}")
    }
}

/// What a watch channel's ends share: the latest value and its version, how many senders and
/// receivers are left, and the waiting receivers' wakers.
pub struct TokioWatchShared<T> {
    __v: T,
    __version: usize,
    __senders: usize,
    __receivers: usize,
    __wakers: Vec<TaskWaker>,
}

/// `tokio::sync::watch::Sender`.
pub struct TokioWatchSender<T> {
    __s: std::rc::Rc<std::cell::RefCell<TokioWatchShared<T>>>,
}

/// `tokio::sync::watch::Receiver`: sees the latest value, and waits for the next with
/// `changed().await`.
pub struct TokioWatchReceiver<T> {
    __s: std::rc::Rc<std::cell::RefCell<TokioWatchShared<T>>>,
    __seen: usize,
}

/// `tokio::sync::watch::channel(init)`.
pub fn __tokio_watch_channel<T>(init: T) -> (TokioWatchSender<T>, TokioWatchReceiver<T>) {
    let s = std::rc::Rc::new(std::cell::RefCell::new(TokioWatchShared {
        __v: init,
        __version: 0,
        __senders: 1,
        __receivers: 1,
        __wakers: Vec::new(),
    }));
    (
        TokioWatchSender { __s: s.clone() },
        TokioWatchReceiver { __s: s, __seen: 0 },
    )
}

impl<T> TokioWatchSender<T> {
    /// A new value for the receivers, or back as `Err` when none is left.
    pub fn send(&self, value: T) -> Result<(), TokioSendError<T>> {
        let mut s = self.__s.borrow_mut();
        if s.__receivers == 0 {
            return Err(TokioSendError(value));
        }
        s.__v = value;
        s.__version += 1;
        for w in s.__wakers.drain(..) {
            w.wake();
        }
        Ok(())
    }
    pub fn borrow(&self) -> &T {
        &self.__s.borrow().__v
    }
    pub fn subscribe(&self) -> TokioWatchReceiver<T> {
        let mut s = self.__s.borrow_mut();
        s.__receivers += 1;
        TokioWatchReceiver {
            __s: self.__s.clone(),
            __seen: s.__version,
        }
    }
    pub fn receiver_count(&self) -> usize {
        self.__s.borrow().__receivers
    }
    pub fn is_closed(&self) -> bool {
        self.__s.borrow().__receivers == 0
    }
    /// Changes the value in place and tells the receivers.
    pub fn send_modify<F: FnOnce(&mut T)>(&self, modify: F) {
        self.send_if_modified(|v| {
            modify(v);
            true
        });
    }
    /// Changes the value in place, telling the receivers only when `modify` says it changed.
    pub fn send_if_modified<F: FnOnce(&mut T) -> bool>(&self, modify: F) -> bool {
        let mut s = self.__s.borrow_mut();
        if !modify(&mut s.__v) {
            return false;
        }
        s.__version += 1;
        for w in s.__wakers.drain(..) {
            w.wake();
        }
        true
    }
    /// A new value for the receivers even when none is left; gives back the old one.
    pub fn send_replace(&self, value: T) -> T {
        let mut s = self.__s.borrow_mut();
        let old = std::mem::replace(&mut s.__v, value);
        s.__version += 1;
        for w in s.__wakers.drain(..) {
            w.wake();
        }
        old
    }
}

impl<T> Drop for TokioWatchSender<T> {
    fn drop(&mut self) {
        let mut s = self.__s.borrow_mut();
        s.__senders -= 1;
        for w in s.__wakers.drain(..) {
            w.wake();
        }
    }
}

impl<T> std::fmt::Debug for TokioWatchSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Sender {{ .. }}")
    }
}

impl<T> TokioWatchReceiver<T> {
    pub fn borrow(&self) -> &T {
        &self.__s.borrow().__v
    }
    /// The value, marked seen.
    pub fn borrow_and_update(&mut self) -> &T {
        self.__seen = self.__s.borrow().__version;
        &self.__s.borrow().__v
    }
    pub fn has_changed(&self) -> Result<bool, TokioRecvError> {
        let s = self.__s.borrow();
        if s.__senders == 0 {
            return Err(TokioRecvError {});
        }
        Ok(s.__version != self.__seen)
    }
    pub fn mark_changed(&mut self) {
        self.__seen = usize::MAX;
    }
    pub fn mark_unchanged(&mut self) {
        self.__seen = self.__s.borrow().__version;
    }
    /// Waits for a value this receiver has not seen (marking it seen); `Err` once the senders
    /// are gone with nothing new.
    pub async fn changed(&mut self) -> Result<(), TokioRecvError> {
        loop {
            let version = self.__s.borrow().__version;
            if version != self.__seen {
                self.__seen = version;
                return Ok(());
            }
            if self.__s.borrow().__senders == 0 {
                return Err(TokioRecvError {});
            }
            TokioWatchWait {
                __s: self.__s.clone(),
                __version: version,
            }
            .await;
        }
    }
}

impl<T> Clone for TokioWatchReceiver<T> {
    fn clone(&self) -> TokioWatchReceiver<T> {
        self.__s.borrow_mut().__receivers += 1;
        TokioWatchReceiver {
            __s: self.__s.clone(),
            __seen: self.__seen,
        }
    }
}

impl<T> Drop for TokioWatchReceiver<T> {
    fn drop(&mut self) {
        self.__s.borrow_mut().__receivers -= 1;
    }
}

impl<T> std::fmt::Debug for TokioWatchReceiver<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Receiver {{ .. }}")
    }
}

/// Ready once a watch channel's version moves past `__version` or its senders are gone.
pub struct TokioWatchWait<T> {
    __s: std::rc::Rc<std::cell::RefCell<TokioWatchShared<T>>>,
    __version: usize,
}

impl<T> std::future::Future for TokioWatchWait<T> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext) -> TaskPoll<()> {
        let mut s = self.__s.borrow_mut();
        if s.__version != self.__version || s.__senders == 0 {
            return TaskPoll::Ready(());
        }
        s.__wakers.push(cx.waker().clone());
        TaskPoll::Pending
    }
}

/// What a `Notify` and its `Notified` futures share: a stored permit, the `notify_waiters`
/// generation, and the futures waiting (in order) or picked by `notify_one`.
pub struct TokioNotifyState {
    __permit: bool,
    __gen: usize,
    __next_id: usize,
    __waiting: Vec<(usize, TaskWaker)>,
    __woken: Vec<usize>,
}

/// `tokio::sync::Notify`.
pub struct TokioNotify {
    __s: std::rc::Rc<std::cell::RefCell<TokioNotifyState>>,
}

impl TokioNotify {
    pub fn new() -> TokioNotify {
        TokioNotify {
            __s: std::rc::Rc::new(std::cell::RefCell::new(TokioNotifyState {
                __permit: false,
                __gen: 0,
                __next_id: 0,
                __waiting: Vec::new(),
                __woken: Vec::new(),
            })),
        }
    }
    /// Wakes the longest-waiting `notified()`, or stores a permit for the next one.
    pub fn notify_one(&self) {
        let mut s = self.__s.borrow_mut();
        if s.__waiting.is_empty() {
            s.__permit = true;
            return;
        }
        let (id, w) = s.__waiting.remove(0);
        s.__woken.push(id);
        w.wake();
    }
    /// Wakes every `notified()` future made so far; stores no permit.
    pub fn notify_waiters(&self) {
        let mut s = self.__s.borrow_mut();
        s.__gen += 1;
        for (_, w) in s.__waiting.drain(..) {
            w.wake();
        }
    }
    pub fn notified(&self) -> TokioNotified {
        let mut s = self.__s.borrow_mut();
        s.__next_id += 1;
        TokioNotified {
            __s: self.__s.clone(),
            __id: s.__next_id,
            __gen: s.__gen,
        }
    }
}

impl Default for TokioNotify {
    fn default() -> TokioNotify {
        TokioNotify::new()
    }
}

impl std::fmt::Debug for TokioNotify {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Notify {{ .. }}")
    }
}

/// What `notify.notified()` returns.
pub struct TokioNotified {
    __s: std::rc::Rc<std::cell::RefCell<TokioNotifyState>>,
    __id: usize,
    __gen: usize,
}

impl std::future::Future for TokioNotified {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext) -> TaskPoll<()> {
        let id = self.__id;
        let gen = self.__gen;
        let mut s = self.__s.borrow_mut();
        if let Some(i) = s.__woken.iter().position(|w| *w == id) {
            s.__woken.remove(i);
            return TaskPoll::Ready(());
        }
        if s.__gen != gen {
            s.__waiting.retain(|(w, _)| *w != id);
            return TaskPoll::Ready(());
        }
        if s.__permit {
            s.__permit = false;
            return TaskPoll::Ready(());
        }
        s.__waiting.retain(|(w, _)| *w != id);
        s.__waiting.push((id, cx.waker().clone()));
        TaskPoll::Pending
    }
}

/// `tokio::time::MissedTickBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokioMissedTickBehavior {
    #[default]
    Burst,
    Delay,
    Skip,
}

/// `tokio::time::Interval`: ticks at `start + n * period`; the first tick is at once.
pub struct TokioInterval {
    __start: Instant,
    __period: Duration,
    __ticks: u32,
    __missed: TokioMissedTickBehavior,
}

/// `tokio::time::interval(period)`.
pub fn __tokio_interval(period: Duration) -> TokioInterval {
    if period.is_zero() {
        panic!("`period` must be non-zero.");
    }
    TokioInterval {
        __start: Instant::now(),
        __period: period,
        __ticks: 0,
        __missed: TokioMissedTickBehavior::Burst,
    }
}

impl TokioInterval {
    /// Waits for the next tick. Behind by a period or more, `Burst` ticks at once until caught
    /// up, `Delay` starts counting again from now, `Skip` drops the missed ticks.
    pub async fn tick(&mut self) -> Instant {
        let p = self.__period.as_secs_f64();
        let target = p * self.__ticks as f64;
        // (Before the start of an `interval_at`, the time to it, negative.)
        let now = Instant::now();
        let el = if now >= self.__start {
            now.duration_since(self.__start).as_secs_f64()
        } else {
            -self.__start.duration_since(now).as_secs_f64()
        };
        if target > el {
            js::sleep((target - el) * 1000.0).await;
        } else if el - target >= p {
            if self.__missed == TokioMissedTickBehavior::Delay {
                self.__start = Instant::now();
                self.__ticks = 0;
            } else if self.__missed == TokioMissedTickBehavior::Skip {
                self.__ticks = (el / p) as u32;
            }
        }
        self.__ticks += 1;
        Instant::now()
    }
    /// The next tick a full period from now.
    pub fn reset(&mut self) {
        self.__start = Instant::now();
        self.__ticks = 1;
    }
    /// The next tick at once.
    pub fn reset_immediately(&mut self) {
        self.__start = Instant::now();
        self.__ticks = 0;
    }
    pub fn period(&self) -> Duration {
        self.__period
    }
    pub fn missed_tick_behavior(&self) -> TokioMissedTickBehavior {
        self.__missed
    }
    pub fn set_missed_tick_behavior(&mut self, behavior: TokioMissedTickBehavior) {
        self.__missed = behavior;
    }
}

impl std::fmt::Debug for TokioInterval {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Interval {{ period: {:?} }}", self.__period)
    }
}

/// `tokio::time::interval_at(start, period)`: the first tick at `start`.
pub fn __tokio_interval_at(start: Instant, period: Duration) -> TokioInterval {
    if period.is_zero() {
        panic!("`period` must be non-zero.");
    }
    TokioInterval {
        __start: start,
        __period: period,
        __ticks: 0,
        __missed: TokioMissedTickBehavior::Burst,
    }
}

/// What a broadcast channel's ends share: the last `__cap` values (each with how many of the
/// receivers alive when it was sent have yet to see it), the position of the first, how many
/// senders and receivers are left, and the waiting receivers' wakers.
pub struct TokioBroadcastShared<T> {
    __buf: std::collections::VecDeque<(T, usize)>,
    __head: u64,
    __cap: usize,
    __senders: usize,
    __receivers: usize,
    __wakers: Vec<TaskWaker>,
}

impl<T> TokioBroadcastShared<T> {
    fn __tail(&self) -> u64 {
        self.__head + self.__buf.len() as u64
    }
    /// A receiver at `next` is done with the values from there on (dropped).
    fn __release(&mut self, next: u64) {
        let head = self.__head;
        for (i, slot) in self.__buf.iter_mut().enumerate() {
            if head + i as u64 >= next && slot.1 > 0 {
                slot.1 -= 1;
            }
        }
    }
}

/// `tokio::sync::broadcast::Sender`.
pub struct TokioBroadcastSender<T> {
    __s: std::rc::Rc<std::cell::RefCell<TokioBroadcastShared<T>>>,
}

/// `tokio::sync::broadcast::Receiver`: sees every value sent after it subscribed, unless it
/// falls more than the capacity behind (`Lagged`).
pub struct TokioBroadcastReceiver<T> {
    __s: std::rc::Rc<std::cell::RefCell<TokioBroadcastShared<T>>>,
    __next: u64,
}

/// `tokio::sync::broadcast::channel(capacity)` (rounded up to a power of two, as tokio does).
pub fn __tokio_broadcast_channel<T: Clone>(
    capacity: usize,
) -> (TokioBroadcastSender<T>, TokioBroadcastReceiver<T>) {
    let tx = TokioBroadcastSender::new(capacity);
    let rx = tx.subscribe();
    (tx, rx)
}

impl<T: Clone> TokioBroadcastSender<T> {
    /// A sender with no receivers yet (`subscribe` makes them).
    pub fn new(capacity: usize) -> TokioBroadcastSender<T> {
        if capacity == 0 {
            panic!("broadcast channel capacity cannot be zero");
        }
        TokioBroadcastSender {
            __s: std::rc::Rc::new(std::cell::RefCell::new(TokioBroadcastShared {
                __buf: std::collections::VecDeque::new(),
                __head: 0,
                __cap: capacity.next_power_of_two(),
                __senders: 1,
                __receivers: 0,
                __wakers: Vec::new(),
            })),
        }
    }
    /// Sends to every receiver: how many there are, or the value back when there are none.
    pub fn send(&self, value: T) -> Result<usize, TokioBroadcastSendError<T>> {
        let mut s = self.__s.borrow_mut();
        let n = s.__receivers;
        if n == 0 {
            return Err(TokioBroadcastSendError(value));
        }
        s.__buf.push_back((value, n));
        if s.__buf.len() > s.__cap {
            s.__buf.pop_front();
            s.__head += 1;
        }
        for w in s.__wakers.drain(..) {
            w.wake();
        }
        Ok(n)
    }
    pub fn subscribe(&self) -> TokioBroadcastReceiver<T> {
        let mut s = self.__s.borrow_mut();
        s.__receivers += 1;
        TokioBroadcastReceiver {
            __s: self.__s.clone(),
            __next: s.__tail(),
        }
    }
    /// How many values some receiver has yet to see.
    pub fn len(&self) -> usize {
        self.__s.borrow().__buf.iter().filter(|v| v.1 > 0).count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn receiver_count(&self) -> usize {
        self.__s.borrow().__receivers
    }
    pub fn same_channel(&self, other: &TokioBroadcastSender<T>) -> bool {
        std::rc::Rc::ptr_eq(&self.__s, &other.__s)
    }
}

impl<T> Clone for TokioBroadcastSender<T> {
    fn clone(&self) -> TokioBroadcastSender<T> {
        self.__s.borrow_mut().__senders += 1;
        TokioBroadcastSender {
            __s: self.__s.clone(),
        }
    }
}

impl<T> Drop for TokioBroadcastSender<T> {
    fn drop(&mut self) {
        let mut s = self.__s.borrow_mut();
        s.__senders -= 1;
        if s.__senders == 0 {
            for w in s.__wakers.drain(..) {
                w.wake();
            }
        }
    }
}

impl<T> std::fmt::Debug for TokioBroadcastSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Sender {{ .. }}")
    }
}

impl<T: Clone> TokioBroadcastReceiver<T> {
    /// The next value now: `Empty` when there is none yet, `Closed` when there never will be,
    /// `Lagged(n)` when `n` values were overwritten before this receiver got to them.
    pub fn try_recv(&mut self) -> Result<T, TokioBroadcastTryRecvError> {
        let mut s = self.__s.borrow_mut();
        let head = s.__head;
        if self.__next < head {
            let missed = head - self.__next;
            self.__next = head;
            return Err(TokioBroadcastTryRecvError::Lagged(missed));
        }
        if self.__next < s.__tail() {
            let i = (self.__next - head) as usize;
            s.__buf[i].1 -= 1;
            let v = s.__buf[i].0.clone();
            self.__next += 1;
            return Ok(v);
        }
        if s.__senders == 0 {
            Err(TokioBroadcastTryRecvError::Closed)
        } else {
            Err(TokioBroadcastTryRecvError::Empty)
        }
    }
    /// Waits for the next value (`Err(Closed)` once the senders are gone and it has seen all).
    pub async fn recv(&mut self) -> Result<T, TokioBroadcastRecvError> {
        loop {
            match self.try_recv() {
                Ok(v) => return Ok(v),
                Err(TokioBroadcastTryRecvError::Lagged(n)) => {
                    return Err(TokioBroadcastRecvError::Lagged(n));
                }
                Err(TokioBroadcastTryRecvError::Closed) => {
                    return Err(TokioBroadcastRecvError::Closed);
                }
                Err(TokioBroadcastTryRecvError::Empty) => {}
            }
            let tail = self.__s.borrow().__tail();
            TokioBroadcastWait {
                __s: self.__s.clone(),
                __tail: tail,
            }
            .await;
        }
    }
    /// A new receiver that sees what is sent from now on.
    pub fn resubscribe(&self) -> TokioBroadcastReceiver<T> {
        let mut s = self.__s.borrow_mut();
        s.__receivers += 1;
        TokioBroadcastReceiver {
            __s: self.__s.clone(),
            __next: s.__tail(),
        }
    }
    /// How many values this receiver has yet to see.
    pub fn len(&self) -> usize {
        let s = self.__s.borrow();
        let from = if self.__next < s.__head { s.__head } else { self.__next };
        (s.__tail() - from) as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn is_closed(&self) -> bool {
        self.__s.borrow().__senders == 0
    }
    pub fn same_channel(&self, other: &TokioBroadcastReceiver<T>) -> bool {
        std::rc::Rc::ptr_eq(&self.__s, &other.__s)
    }
}

impl<T> Drop for TokioBroadcastReceiver<T> {
    fn drop(&mut self) {
        let mut s = self.__s.borrow_mut();
        s.__receivers -= 1;
        s.__release(self.__next);
    }
}

impl<T> std::fmt::Debug for TokioBroadcastReceiver<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Receiver {{ .. }}")
    }
}

/// Ready once a broadcast channel gets a value past `__tail` or its senders are gone.
pub struct TokioBroadcastWait<T> {
    __s: std::rc::Rc<std::cell::RefCell<TokioBroadcastShared<T>>>,
    __tail: u64,
}

impl<T> std::future::Future for TokioBroadcastWait<T> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext) -> TaskPoll<()> {
        let mut s = self.__s.borrow_mut();
        if s.__tail() != self.__tail || s.__senders == 0 {
            return TaskPoll::Ready(());
        }
        s.__wakers.push(cx.waker().clone());
        TaskPoll::Pending
    }
}

/// `tokio::sync::broadcast::error::SendError`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TokioBroadcastSendError<T>(pub T);

impl<T: std::fmt::Debug> std::fmt::Debug for TokioBroadcastSendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_tuple("SendError").field(&self.0).finish()
    }
}

impl<T> std::fmt::Display for TokioBroadcastSendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "channel closed")
    }
}

impl<T: std::fmt::Debug> std::error::Error for TokioBroadcastSendError<T> {}

/// `tokio::sync::broadcast::error::RecvError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokioBroadcastRecvError {
    Closed,
    Lagged(u64),
}

impl std::fmt::Display for TokioBroadcastRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TokioBroadcastRecvError::Closed => write!(f, "channel closed"),
            TokioBroadcastRecvError::Lagged(n) => write!(f, "channel lagged by {n}"),
        }
    }
}

impl std::error::Error for TokioBroadcastRecvError {}

/// `tokio::sync::broadcast::error::TryRecvError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokioBroadcastTryRecvError {
    Empty,
    Closed,
    Lagged(u64),
}

impl std::fmt::Display for TokioBroadcastTryRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TokioBroadcastTryRecvError::Empty => write!(f, "channel empty"),
            TokioBroadcastTryRecvError::Closed => write!(f, "channel closed"),
            TokioBroadcastTryRecvError::Lagged(n) => write!(f, "channel lagged by {n}"),
        }
    }
}

impl std::error::Error for TokioBroadcastTryRecvError {}
