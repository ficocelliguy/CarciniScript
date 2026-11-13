// Mutation through `&mut` with structs and enums: `&mut` parameters, `*p = ..`, matching on
// `&mut self` and `&mut` loop variables with writes through the bindings, Copy structs passed by
// value and stored in collections, nested field assignment.
#[derive(Debug, Clone, Copy, PartialEq)]
struct P {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone)]
struct Outer {
    inner: P,
    tag: String,
    list: Vec<P>,
}

#[derive(Debug)]
enum Shape {
    Circle(f64),
    Rect { w: f64, h: f64 },
}

impl Shape {
    fn grow(&mut self) {
        match self {
            Shape::Circle(r) => *r *= 2.0,
            Shape::Rect { w, h } => {
                *w += 1.0;
                *h += 1.0;
            }
        }
    }
}

fn bump(mut p: P) -> P {
    p.x += 100;
    p
}

fn reset(p: &mut P) {
    *p = P { x: 0, y: 0 };
}

fn nudge(p: &mut P) {
    p.y -= 1;
}

fn main() {
    let a = P { x: 1, y: 2 };
    let b = bump(a);
    println!("{:?} {:?}", a, b);
    let mut c = a;
    reset(&mut c);
    nudge(&mut c);
    println!("{:?} {:?}", a, c);
    let mut o = Outer { inner: a, tag: "t".to_string(), list: vec![a, b] };
    o.inner.x = 50;
    o.list[1].y = 7;
    let copied = o.list[0];
    o.list[0].x = -5;
    println!("{:?} {:?} {:?}", o, copied, a);
    let o2 = o.clone();
    o.tag.push('!');
    o.list.push(c);
    println!("{} {} {}", o2.tag, o.tag, o2.list.len());
    let mut s = Shape::Circle(1.5);
    s.grow();
    let mut r = Shape::Rect { w: 1.0, h: 2.0 };
    r.grow();
    println!("{:?} {:?}", s, r);
    let mut shapes = vec![Shape::Circle(1.0), Shape::Rect { w: 0.5, h: 0.5 }];
    for sh in shapes.iter_mut() {
        sh.grow();
    }
    for sh in &mut shapes {
        if let Shape::Circle(rad) = sh {
            *rad += 0.25;
        }
    }
    println!("{:?}", shapes);
    let mut ps = vec![a, b];
    for p in ps.iter_mut() {
        p.x *= 3;
    }
    let total: i32 = ps.iter().map(|p| p.x + p.y).sum();
    println!("{:?} {}", ps, total);
    let mut grid = [[P { x: 0, y: 0 }; 2]; 2];
    grid[1][0].x = 9;
    println!("{:?}", grid);
}
