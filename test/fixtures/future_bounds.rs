// `F: Future<Output = T>` bounds on a function's parameters (`T` another type parameter, inline
// or in `where`), a bare `F: Future` with `F::Output`, and `Fut: Future` named by an `Fn` bound.
// Natively a no-op waker runs `run()`, as every future here is ready at once.
use std::future::Future;
async fn two() -> i32 { 2 }
async fn name() -> String { "n".to_string() }
async fn run1<F: Future<Output = i32>>(f: F) -> i32 { f.await + 1 }
async fn run2<F>(f: F) -> i32 where F: Future<Output = i32> { f.await * 10 }
async fn run3<T, F: Future<Output = T>>(f: F) -> T { f.await }
async fn run4<F: Future>(f: F) -> F::Output { f.await }
async fn run5<F, T>(f: F) -> Vec<T> where F: Future<Output = T> { vec![f.await] }
async fn twice<G, Fut>(g: G) -> i32 where G: Fn(i32) -> Fut, Fut: Future<Output = i32> { g(1).await + g(2).await }
async fn run() {
    println!("{}", run1(two()).await);
    println!("{}", run2(two()).await);
    println!("{}", run3(two()).await);
    println!("{}", run3(name()).await);
    println!("{}", run1(async { 5 }).await);
    println!("{}", run4(name()).await);
    println!("{:?}", run5(two()).await);
    println!("{}", twice(|x| async move { x * 3 }).await);
}

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        #[tokio::main]
        async fn main() {
            run().await;
        }
    } else {
        fn main() {
            let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
            let mut f = Box::pin(run());
            while f.as_mut().poll(&mut cx).is_pending() {}
        }
    }
}
