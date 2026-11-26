// Impls for `&T` / `&mut T` / `Box<T>` that do more than `T`'s: a generic call whose `T` is a
// reference (`g(&x)` for `fn g<T: Tr>(t: T)`) reaches the reference's impl, which hands on to
// the referent's with its class; a direct call (`r.show()`) reaches the referent's, as method
// probing does.

trait Show {
    fn show(&self) -> String;
    fn twice(&self) -> String {
        format!("{}{}", self.show(), self.show())
    }
}
struct Cat(u8);
impl Show for Cat {
    fn show(&self) -> String {
        format!("cat{}", self.0)
    }
}
impl Show for i32 {
    fn show(&self) -> String {
        format!("i{}", self)
    }
}
impl<T: Show + ?Sized> Show for &T {
    fn show(&self) -> String {
        format!("&{}", (**self).show())
    }
}
impl<T: Show + ?Sized> Show for &mut T {
    fn show(&self) -> String {
        format!("&mut {}", (**self).show())
    }
}
impl<T: Show + ?Sized> Show for Box<T> {
    fn show(&self) -> String {
        format!("box {}", (**self).show())
    }
}

fn g<T: Show>(t: T) -> String {
    t.show()
}

// (Handing its `T` on.)
fn g2<T: Show>(t: T) -> String {
    g(t)
}

fn by_ref<T: Show + ?Sized>(t: &T) -> String {
    t.show()
}

fn by_ref_on<T: Show>(t: &T) -> String {
    g(t)
}

// Associated functions: `T::describe()` through a reference's class.
trait Describe {
    fn describe(out: &mut Vec<u8>);
}
struct A;
impl Describe for A {
    fn describe(out: &mut Vec<u8>) {
        out.push(1)
    }
}
impl Describe for u32 {
    fn describe(out: &mut Vec<u8>) {
        out.push(2)
    }
}
impl<T: Describe + ?Sized> Describe for &T {
    fn describe(out: &mut Vec<u8>) {
        out.push(9);
        T::describe(out)
    }
}

fn describe<T: Describe>(_t: T, out: &mut Vec<u8>) {
    T::describe(out)
}

fn main() {
    let c = Cat(1);
    let mut m = Cat(2);
    let x = 7;
    println!("{} | {} | {} | {}", g(&c), g(&mut m), g(Box::new(Cat(3))), g(&&x));
    println!("{} | {} | {}", g2(&c), by_ref(&c), by_ref_on(&c));
    let r = &c;
    let rr: &Cat = &c;
    println!("{} | {} | {} | {}", g(r), g(rr), r.show(), g(x));
    println!("{}", c.twice());

    let mut o = Vec::new();
    describe(A, &mut o);
    describe(&A, &mut o);
    describe(&&5u32, &mut o);
    <&A as Describe>::describe(&mut o);
    println!("{:?}", o);
}
