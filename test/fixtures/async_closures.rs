// Async closures (`async |x| ..`, `async move |x| ..`, with `-> T` and `?`), `AsyncFn` and
// `F: Fn() -> Fut, Fut: Future<Output = T>` bounds, and `Pin<Box<dyn Future<Output = T>>>` with
// `Box::pin(async move { .. })` for recursive async fns. Natively a no-op waker runs `run()`, as
// every future here is ready at once.
use std::future::Future;
use std::pin::Pin;

struct Node {
    name: String,
    kids: Vec<Node>,
}

fn walk<'a>(n: &'a Node, depth: usize, out: &'a mut Vec<String>) -> Pin<Box<dyn Future<Output = usize> + 'a>> {
    Box::pin(async move {
        out.push(format!("{}{}", "  ".repeat(depth), n.name));
        let mut total = 1;
        for k in &n.kids {
            total += walk(k, depth + 1, out).await;
        }
        total
    })
}

impl Node {
    fn size(&self) -> Pin<Box<dyn Future<Output = usize> + '_>> {
        Box::pin(async move {
            let mut s = 1;
            for k in &self.kids {
                s += k.size().await;
            }
            s
        })
    }
}

fn count(n: u32) -> Pin<Box<dyn Future<Output = u32>>> {
    Box::pin(async move {
        if n == 0 { 0 } else { n + count(n - 1).await }
    })
}

async fn double(x: u32) -> u32 {
    x * 2
}

async fn apply<F: AsyncFn(u32) -> u32>(f: F, x: u32) -> u32 {
    f(x).await + f(x + 1).await
}

async fn apply_fut<F, Fut>(f: F) -> String
where
    F: Fn(u8) -> Fut,
    Fut: Future<Output = String>,
{
    f(7).await
}

fn fib(n: u64) -> Pin<Box<dyn Future<Output = u64>>> {
    Box::pin(async move {
        if n < 2 {
            return n;
        }
        fib(n - 1).await + fib(n - 2).await
    })
}

async fn run() {
    let base = 10;
    let add = async |x: i32| x + base;
    println!("{}", add(5).await);
    let name = String::from("hi");
    let greet = async move |n: u32| format!("{name} x{n}");
    println!("{}", greet(3).await);
    println!("{}", greet(4).await);
    let tree = Node {
        name: "root".into(),
        kids: vec![
            Node { name: "a".into(), kids: vec![Node { name: "a1".into(), kids: vec![] }] },
            Node { name: "b".into(), kids: vec![] },
        ],
    };
    let mut lines = Vec::new();
    let n = walk(&tree, 0, &mut lines).await;
    for l in &lines {
        println!("{l}");
    }
    println!("{n} {}", tree.size().await);
    println!("{}", count(5).await);
    let mut acc = Vec::new();
    for i in 0..3 {
        let f = async |k: i32| -> i32 { k * 2 };
        acc.push(f(i).await);
    }
    println!("{acc:?}");
    let fut: Pin<Box<dyn Future<Output = String>>> = Box::pin(async { String::from("boxed") });
    println!("{}", fut.await);
    let k3 = 3;
    let r = apply(async |x| double(x).await + k3, 5).await;
    println!("{r}");
    let s = apply_fut(async |n| format!("n={n}")).await;
    println!("{s}");
    let parse = async |t: &str| -> Result<i32, std::num::ParseIntError> {
        let v: i32 = t.parse()?;
        Ok(v * 10)
    };
    println!("{:?}", parse("4").await);
    println!("{}", parse("x").await.is_err());
    let futs: Vec<Pin<Box<dyn Future<Output = u64>>>> = vec![fib(10), Box::pin(async { 99 })];
    for f in futs {
        println!("{}", f.await);
    }
    let mut total = 0;
    let mut add = async |x: u32| total += x;
    add(4).await;
    add(5).await;
    println!("{total}");
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
