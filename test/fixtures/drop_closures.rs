// `Drop` and closures: a value a `move` closure captures (or a closure's body moves) is dropped
// with the closure, not at the end of its old scope: when the closure's binding ends, when the
// function it was passed to ends, with the struct, `Box` or `Vec` holding it, or (`FnOnce`) when
// the body moves it. A method taking `self` by value drops `self` when it ends, unless it moves
// it on (returns it, passes it to a function) or moves a field out.
struct Guard {
    name: String,
}

impl Drop for Guard {
    fn drop(&mut self) {
        println!("drop {}", self.name);
    }
}

fn g(name: &str) -> Guard {
    Guard { name: name.to_string() }
}

impl Guard {
    fn finish(self) -> usize {
        println!("finish {}", self.name);
        self.name.len()
    }
    fn rename(mut self, n: &str) -> Guard {
        self.name = n.to_string();
        self
    }
    fn into_name(self) -> String {
        let n = self.name.clone();
        println!("into_name {n}");
        n
    }
    fn hand_off(self) {
        consume(self);
        println!("handed off");
    }
}

fn consume(x: Guard) {
    println!("consume {}", x.name);
}

fn make_closure() -> impl Fn() -> usize {
    let a = g("returned-capture");
    move || a.name.len()
}

fn boxed() -> Box<dyn Fn()> {
    let b = g("boxed-capture");
    Box::new(move || println!("boxed sees {}", b.name))
}

struct Wrapper {
    inner: Guard,
    label: String,
}

impl Wrapper {
    fn into_inner(self) -> Guard {
        println!("unwrapping {}", self.label);
        self.inner
    }
    fn show(self) {
        println!("showing {}", self.inner.name);
    }
    fn label(&self) -> &str {
        &self.label
    }
}

struct Holder<F: Fn() -> usize> {
    f: F,
}

fn call_impl(f: impl Fn()) {
    f();
    println!("call_impl ends");
}

fn call_gen<F: Fn() -> usize>(f: F) -> usize {
    let n = f();
    println!("call_gen ends");
    n
}

fn call_ref(f: &dyn Fn()) {
    f();
}

fn make() -> impl Fn() -> usize {
    let k = g("let-bound");
    let f = move || k.name.len();
    f
}

fn main() {
    {
        let a = g("a");
        let f = move || println!("closure sees {}", a.name);
        println!("before call");
        f();
        f();
        println!("end of block");
    }
    println!("--");
    let outer;
    {
        let b = g("b");
        outer = move || println!("outer sees {}", b.name);
        println!("inner block ends");
    }
    outer();
    println!("--");
    {
        let c = g("c");
        let once = move || {
            let taken = c;
            println!("once took {}", taken.name);
        };
        println!("before once");
        once();
        println!("after once");
    }
    println!("--");
    {
        let d = g("d");
        let by_value = || drop(d);
        println!("before by_value");
        by_value();
        println!("after by_value");
    }
    println!("--");
    {
        let e = g("e");
        let unused = move || e.name.len();
        let _ = &unused;
        println!("unused closure ends");
    }
    println!("--");
    {
        let f = make_closure();
        println!("made {}", f());
    }
    println!("--");
    {
        let h = boxed();
        h();
        println!("boxed block ends");
    }
    println!("--");
    {
        let n = g("method").finish();
        println!("finished {n}");
        let r = g("old").rename("new");
        println!("renamed {}", r.name);
        let s = g("named").into_name();
        println!("got {s}");
        g("handoff").hand_off();
        println!("end");
    }
    println!("--");
    let closures: Vec<Box<dyn Fn()>> = vec![boxed(), boxed()];
    for c in &closures {
        c();
    }
    drop(closures);
    println!("==");
    {
        let w = Wrapper { inner: g("w1"), label: "one".into() };
        let inner = w.into_inner();
        println!("got {}", inner.name);
    }
    println!("--");
    {
        let w = Wrapper { inner: g("w2"), label: "two".into() };
        println!("{}", w.label());
        w.show();
        println!("after show");
    }
    println!("--");
    {
        let a = g("impl");
        call_impl(move || println!("impl sees {}", a.name));
        println!("after call_impl");
    }
    println!("--");
    {
        let b = g("gen");
        let f = move || b.name.len();
        println!("{}", call_gen(f));
        println!("after call_gen");
    }
    println!("--");
    {
        let c = g("ref");
        let f = move || println!("ref sees {}", c.name);
        call_ref(&f);
        call_ref(&f);
        println!("after call_ref");
    }
    println!("--");
    {
        let d = g("held");
        let h = Holder { f: move || d.name.len() };
        println!("held {}", (h.f)());
        println!("holder ends");
    }
    println!("--");
    {
        let e = g("moved");
        let f = move || e.name.len();
        let f2 = f;
        println!("f2 {}", f2());
        let boxed: Box<dyn Fn() -> usize> = Box::new(f2);
        println!("boxed {}", boxed());
        println!("block ends");
    }
    println!("--");
    {
        let m = make();
        println!("made {}", m());
    }
    println!("--");
    for i in 0..2 {
        let x = g(&format!("loop{i}"));
        let f = move || println!("loop sees {}", x.name);
        f();
    }
    println!("--");
    {
        let v = g("vec");
        let fs: Vec<Box<dyn Fn()>> = vec![Box::new(move || println!("in vec {}", v.name)), Box::new(|| println!("plain"))];
        for f in &fs {
            f();
        }
        println!("vec block ends");
    }
    println!("done");
}
