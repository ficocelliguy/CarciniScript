// In a trait's default method, `vec![]` / `Vec::new()` typed only by the `&mut Vec<Self::Leaf>`
// it is passed to.
trait Tree {
    type Leaf;
    fn collect_buf(&self, buf: &mut Vec<Self::Leaf>);
    fn leaves(&self) -> Vec<Self::Leaf> {
        let mut buf = vec![];
        self.collect_buf(&mut buf);
        buf
    }
    fn leaves_new(&self) -> Vec<Self::Leaf> {
        let mut buf = Vec::new();
        self.collect_buf(&mut buf);
        buf
    }
}

struct T(Vec<u8>);

impl Tree for T {
    type Leaf = u8;
    fn collect_buf(&self, buf: &mut Vec<u8>) {
        buf.extend(&self.0)
    }
}

fn main() {
    let t = T(vec![1, 2]);
    println!("{:?} {:?}", t.leaves(), t.leaves_new());
}
