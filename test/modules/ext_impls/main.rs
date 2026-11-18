// A `mod` file's traits implemented here for built-in types (numbers, `bool`, `str` / `String`,
// `Vec<T>`, `Option<T>`, also from a `macro_rules!`): direct calls, the trait's default methods,
// and the module's own generic functions (`fmt::describe`), which find these impls through the
// module's registry.
mod fmt;

use fmt::{Format, Width};

macro_rules! prim {
    ($($t:ty),*) => {
        $(impl Format for $t {
            fn format(&self) -> String {
                format!("<{}>", self)
            }
        })*
    };
}

prim!(u8, i64, bool);

impl Format for str {
    fn format(&self) -> String {
        format!("str:{}", self)
    }
}

impl Format for String {
    fn format(&self) -> String {
        self.as_str().format()
    }
}

impl<T: Format> Format for Vec<T> {
    fn format(&self) -> String {
        let parts: Vec<String> = self.iter().map(|x| x.format()).collect();
        format!("[{}]", parts.join(","))
    }
}

impl<T: Format> Format for Option<T> {
    fn format(&self) -> String {
        match self {
            Some(x) => format!("Some({})", x.format()),
            None => "None".to_string(),
        }
    }
}

impl fmt::Width for u8 {
    fn width(&self) -> usize {
        self.to_string().len()
    }
}

impl Width for String {
    fn width(&self) -> usize {
        self.chars().count()
    }
}

fn show<T: Format>(x: &T) -> String {
    x.format()
}

fn main() {
    println!("{}", 5u8.format());
    println!("{}", (-3i64).format());
    println!("{}", true.format());
    println!("{}", "hi".format());
    println!("{}", String::from("yo").format());
    println!("{}", vec![1u8, 2].format());
    println!("{}", Some(4u8).format());
    println!("{}", None::<u8>.format());
    println!("{}", show(&9u8));
    println!("{} {}", 5u8.twice(), "ab".twice());
    println!("{}", fmt::describe(&String::from("s")));
    println!("{}", fmt::describe(&vec![1u8, 2]));
    println!("{}", fmt::describe(&None::<bool>));
    println!("{}", fmt::total_width(&[1u8, 22, 233]));
    println!("{} {}", 123u8.width(), String::from("héllo").width());
    println!("{}", fmt::describe(&7u8));
}
