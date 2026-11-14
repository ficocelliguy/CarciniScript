// `&mut` to a slot inside a struct, tuple or Vec, passed to a `&mut` parameter: the callee reads
// and writes the slot itself (`&mut n.left` building a tree recursively, `&mut v[i]`,
// `&mut grid[r][c]`, `&mut p.x`, `&mut pair.1`, `std::mem::swap`-free swaps through helpers).
#[derive(Debug)]
struct Node {
    val: i32,
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}

fn insert(slot: &mut Option<Box<Node>>, val: i32) {
    match slot {
        None => *slot = Some(Box::new(Node { val, left: None, right: None })),
        Some(n) => {
            if val < n.val {
                insert(&mut n.left, val)
            } else {
                insert(&mut n.right, val)
            }
        }
    }
}

fn inorder(n: &Option<Box<Node>>, out: &mut Vec<i32>) {
    if let Some(b) = n {
        inorder(&b.left, out);
        out.push(b.val);
        inorder(&b.right, out);
    }
}

fn bump(x: &mut i32, by: i32) {
    *x += by;
}

fn shout(s: &mut String) {
    *s = s.to_uppercase() + "!";
}

#[derive(Debug)]
struct P {
    x: i32,
    name: String,
}

fn main() {
    let mut root = None;
    for v in [5, 3, 8, 1, 4, 9] {
        insert(&mut root, v);
    }
    let mut out = vec![];
    inorder(&root, &mut out);
    println!("{:?}", out);

    let mut v = vec![1, 2, 3];
    let i = 1;
    bump(&mut v[i], 10);
    bump(&mut v[0], 5);
    let mut grid = vec![vec![0; 3]; 2];
    let (r, c) = (1, 2);
    bump(&mut grid[r][c], 7);
    let mut p = P { x: 1, name: "n00dles".into() };
    bump(&mut p.x, 41);
    shout(&mut p.name);
    let mut pair = (1, 2);
    bump(&mut pair.1, 100);
    println!("{:?} {:?} {:?} {:?}", v, grid, p, pair);
}
