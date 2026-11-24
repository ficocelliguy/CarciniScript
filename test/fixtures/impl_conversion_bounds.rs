// `T: AsRef<str>` / `T: Into<String>` on an impl's type parameter (heck's `AsKebabCase<T>`): `T` is
// a string in the impl's methods, inline or in `where`, on a struct with or without the bound.
use std::fmt;
struct Kebab<T>(T);
impl<T: AsRef<str>> fmt::Display for Kebab<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.as_ref().to_lowercase().replace(' ', "-"))
    }
}
struct Len<T: AsRef<str>>(T);
impl<T: AsRef<str>> Len<T> {
    fn n(&self) -> usize { self.0.as_ref().len() }
}
struct Shout<T> { s: T }
impl<T> Shout<T> where T: Into<String> + Clone {
    fn get(&self) -> String { let s: String = self.s.clone().into(); s.to_uppercase() + "!" }
}
pub trait ToKebab { fn to_kebab(&self) -> String; }
impl ToKebab for str { fn to_kebab(&self) -> String { Kebab(self).to_string() } }
fn main() {
    println!("{} {}", Kebab("Hello World"), Kebab(String::from("A B C")));
    println!("{} {}", Len("abc").n(), Len(String::from("abcd")).n());
    println!("{} {}", Shout { s: "hi" }.get(), Shout { s: String::from("yo") }.get());
    println!("{}", "Going To Inherit".to_kebab());
}
