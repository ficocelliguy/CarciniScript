// Associated functions of a trait reached through a blanket impl for `Rc<T>` / `Box<T>` /
// `Arc<T>`: `<Rc<u8>>::f()`, `Rc::<u8>::f()` and `<Rc<u8> as Tr>::f()` call the wrapper's impl
// with `T` = `u8`, not `u8`'s own.
use std::rc::Rc;
use std::sync::Arc;

trait Schema {
    fn declaration() -> String;
    fn size() -> usize {
        1
    }
}
impl Schema for u8 {
    fn declaration() -> String {
        "u8".to_string()
    }
}
impl Schema for String {
    fn declaration() -> String {
        "string".to_string()
    }
    fn size() -> usize {
        3
    }
}
impl<T: Schema> Schema for Rc<T> {
    fn declaration() -> String {
        format!("Rc<{}>", T::declaration())
    }
    fn size() -> usize {
        T::size() + 8
    }
}

trait Tag {
    fn tag() -> u32;
}
impl Tag for u8 {
    fn tag() -> u32 {
        1
    }
}
impl<T: Tag> Tag for Box<T> {
    fn tag() -> u32 {
        T::tag() + 20
    }
}

trait Shared {
    fn shared() -> u32;
}
impl Shared for u16 {
    fn shared() -> u32 {
        5
    }
}
impl<T: Shared> Shared for Arc<T> {
    fn shared() -> u32 {
        T::shared() * 100
    }
}

fn dep<S: Schema>() -> String {
    format!("{}/{}", S::declaration(), S::size())
}

fn tagged<T: Tag>() -> u32 {
    T::tag()
}

fn main() {
    println!("{} {}", <Rc<u8>>::declaration(), Rc::<String>::declaration());
    println!("{} {}", <Rc<u8> as Schema>::declaration(), <Rc<String> as Schema>::size());
    println!("{} {}", <Rc<u8>>::size(), u8::declaration());
    println!("{} {} {}", <Box<u8>>::tag(), Box::<u8>::tag(), <Box<u8> as Tag>::tag());
    println!("{} {}", <Arc<u16>>::shared(), Arc::<u16>::shared());
    println!("{} {}", dep::<Rc<u8>>(), dep::<Rc<String>>());
    println!("{} {}", tagged::<Box<u8>>(), tagged::<u8>());
}
