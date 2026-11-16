// `impl Drop`: values are dropped at the end of their scope in reverse order, unless moved;
// `drop(x)` drops early; reassigning drops the old value; temporaries and `let _ =` drop at
// once; fields and elements drop after their owner's `drop`, in order; early returns, `break`
// and `?` drop what's in scope; by-value parameters drop at the end of the function.
struct Guard {
    name: String,
}

impl Drop for Guard {
    fn drop(&mut self) {
        println!("drop {}", self.name);
    }
}

fn g(name: &str) -> Guard {
    Guard {
        name: name.to_string(),
    }
}

struct Pair {
    first: Guard,
    second: Guard,
}

impl Drop for Pair {
    fn drop(&mut self) {
        println!("drop pair ({} {})", self.first.name, self.second.name);
    }
}

fn consume(x: Guard) {
    println!("consume {}", x.name);
}

fn pass_through(x: Guard) -> Guard {
    println!("pass {}", x.name);
    x
}

fn early(flag: bool) -> u32 {
    let _a = g("early-a");
    if flag {
        return 1;
    }
    let _b = g("early-b");
    2
}

fn tryit(ok: bool) -> Result<u32, String> {
    let _t = g("try");
    let v: u32 = if ok { "5".parse().map_err(|_| "bad".to_string())? } else { "x".parse().map_err(|_| "bad".to_string())? };
    Ok(v)
}

fn main() {
    let _a = g("a");
    {
        let _b = g("b");
        println!("inner");
    }
    let c = g("c");
    drop(c);
    let d = g("d");
    let e = d;
    println!("moved d into e: {}", e.name);
    let mut r = g("r1");
    r = g("r2");
    println!("reassigned {}", r.name);
    let _ = g("temp");
    g("statement");
    let v = vec![g("v1"), g("v2")];
    let p = Pair {
        first: g("p1"),
        second: g("p2"),
    };
    consume(g("arg"));
    let kept = pass_through(g("through"));
    println!("kept {}", kept.name);
    println!("early {}", early(true));
    println!("early {}", early(false));
    println!("{:?}", tryit(true));
    println!("{:?}", tryit(false));
    for i in 0..2 {
        let _l = g(&format!("loop{}", i));
        if i == 1 {
            break;
        }
        println!("iter {}", i);
    }
    let maybe: Option<Guard> = Some(g("opt"));
    std::mem::drop(v);
    println!("end of main {} {}", p.first.name, maybe.is_some());
}
