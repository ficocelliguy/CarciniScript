// `impl<T: Tr> Tr for Option<T>` through a generic `T: Tr`: `Some(Some(4))` reaches the `Option`
// impl twice, though at run time it's `4`.
trait Depth {
    fn depth(&self, tag: &str) -> String;
}

impl Depth for i32 {
    fn depth(&self, tag: &str) -> String {
        format!("{tag}i32 {self}")
    }
}

impl Depth for String {
    fn depth(&self, tag: &str) -> String {
        format!("{tag}{self:?}")
    }
}

#[derive(Debug)]
struct P(u8);

impl Depth for P {
    fn depth(&self, tag: &str) -> String {
        format!("{tag}{self:?}")
    }
}

impl<T: Depth> Depth for Option<T> {
    fn depth(&self, tag: &str) -> String {
        match self {
            Some(x) => format!("Some({})", x.depth(tag)),
            None => "None".to_string(),
        }
    }
}

fn show<T: Depth>(t: T) -> String {
    t.depth("")
}

fn show_ref<T: Depth>(t: &T, tag: &str) -> String {
    t.depth(tag)
}

// Passes its `T` on to `show`.
fn twice<T: Depth>(a: T, b: T) -> String {
    format!("{} / {}", show(a), show(b))
}

fn main() {
    println!("{}", show(Some(Some(4))));
    println!("{}", show(Some(None::<i32>)));
    println!("{}", show(None::<Option<i32>>));
    println!("{}", show(Some(4)));
    println!("{}", show(4));
    println!("{}", Some(Some(5)).depth("#"));
    println!("{}", show(Some(Some(Some(6)))));
    let x: Option<Option<i32>> = Some(Some(7));
    println!("{}", show_ref(&x, "~"));
    println!("{}", show(Some(Some("hi".to_string()))));
    println!("{}", show(Some(Some(P(3)))));
    println!("{}", twice(Some(Some(1)), Some(None)));
    let items = vec![Some(Some(8)), Some(None), None];
    for it in &items {
        println!("{}", show_ref(it, ""));
    }
}
