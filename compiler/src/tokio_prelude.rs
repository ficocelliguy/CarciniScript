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
