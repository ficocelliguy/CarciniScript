// `Self::Target` in a `DerefMut` impl names its `Deref` impl's `type Target`, whichever comes
// first.
use std::ops::{Deref, DerefMut};

struct Stack(Vec<u8>);

impl DerefMut for Stack {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Deref for Stack {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn main() {
    let mut s = Stack(vec![1, 2]);
    s.push(3);
    s[0] = 9;
    println!("{} {:?}", s.len(), *s);
}
