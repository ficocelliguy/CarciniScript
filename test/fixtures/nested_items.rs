// Items declared inside function bodies (fn, const, struct, enum, impl), and labeled block
// expressions: `break 'a value`, falling off the end, unlabeled `break` inside one leaving the
// enclosing loop.
fn main() {
    fn helper(x: i32) -> i32 {
        x * 2
    }
    const K: i32 = 3;

    #[derive(Debug)]
    struct Pair {
        a: i32,
        b: i32,
    }
    impl Pair {
        fn sum(&self) -> i32 {
            self.a + self.b
        }
    }

    let v = 'found: {
        for i in 0..10 {
            if helper(i) > 7 {
                break 'found i;
            }
        }
        -1
    };
    let w: i32 = 'none: {
        if K > 10 {
            break 'none 100;
        }
        K * 2
    };
    let mut seen = Vec::new();
    for i in 0..5 {
        'check: {
            if i % 2 == 0 {
                break 'check;
            }
            seen.push(i);
        }
        if i == 3 {
            break;
        }
    }
    let p = Pair { a: v, b: w };
    println!("{} {} {} {} {:?} {:?} {}", helper(4), K, v, w, seen, p, p.sum());
}
