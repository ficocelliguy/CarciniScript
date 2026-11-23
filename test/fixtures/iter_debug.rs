// `{:?}` of a slice iterator shows what's left, as `slice::Iter`'s does; also as a derived
// `Debug` field.
#[derive(Debug)]
struct W<'a> {
    it: std::slice::Iter<'a, u8>,
    n: u8,
}

fn main() {
    let v = vec![1u8, 2, 3];
    let mut it = v.iter();
    println!("{:?}", it);
    it.next();
    println!("{:?}", it);
    println!("{:?}", it.next());
    let w = W { it: v.iter(), n: 3 };
    println!("{:?}", w);
    println!("{:#?}", w);
    let words = ["a", "b"];
    println!("{:?}", words.iter());
}
