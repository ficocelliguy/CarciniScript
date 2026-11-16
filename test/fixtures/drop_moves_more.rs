// `Drop` through moves the first drop fixtures don't cover: a field moved out of its struct
// (the struct no longer drops it, its new owner does), a by-value `self` method that moves a
// field out and then drops the rest, and a closure handed to a std method (`it.map(f)`), whose
// captures drop at the end of that statement, with the adapter.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

struct Pair {
    left: Noisy,
    right: Noisy,
}

impl Pair {
    fn take_left(self) -> Noisy {
        println!("taking left");
        self.left
    }
    fn names(self) -> String {
        format!("{}+{}", self.left.0, self.right.0)
    }
}

fn keep(n: Noisy) -> Noisy {
    n
}

fn main() {
    {
        let p = Pair { left: Noisy("a"), right: Noisy("b") };
        let l = p.left;
        println!("moved {}", l.0);
        println!("end of block");
    }
    println!("--");
    {
        let p = Pair { left: Noisy("c"), right: Noisy("d") };
        let kept = keep(p.right);
        println!("kept {}", kept.0);
        drop(kept);
        println!("after drop");
    }
    println!("--");
    let left = Pair { left: Noisy("e"), right: Noisy("f") }.take_left();
    println!("got {}", left.0);
    println!("{}", Pair { left: Noisy("g"), right: Noisy("h") }.names());
    println!("--");
    {
        let tag = Noisy("closure");
        let f = move |x: i32| x + tag.0.len() as i32;
        let v: Vec<i32> = vec![1, 2].into_iter().map(f).collect();
        println!("{:?}", v);
        println!("still in block");
    }
    {
        // (Here the closure owns all of `whole`: it drops with the `map` adapter, at the end of
        // the `let`.)
        let whole = Noisy("whole");
        let g = move |x: i32| {
            let w = &whole;
            x * w.0.len() as i32
        };
        let v: Vec<i32> = vec![1, 2].into_iter().map(g).collect();
        println!("{:?}", v);
        println!("still in second block");
    }
    println!("end");
}
