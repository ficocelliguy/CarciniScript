// `#[cfg_attr(pred, attr, ..)]` is applied when `pred` holds with every feature off (a script
// has none): `not(feature = "x")` yes, `feature = "x"` and `docsrs` no. A `#[cfg(feature = "x")]`
// on a `macro_rules!` call next to a plain copy of it, or on a block's tail expression, is off.
#[cfg_attr(not(feature = "nope"), derive(Debug, Clone, PartialEq))]
struct St {
    a: u8,
}

#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(not(feature = "x"), derive(Hash, Debug))]
#[cfg_attr(feature = "x", derive(Default))]
enum Status {
    Ok = 0,
    End = 1,
}

#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[cfg_attr(not(feature = "std"), derive(Debug), derive(Default))]
struct Pair(u8, u8);

#[cfg_attr(all(not(feature = "a"), any(not(feature = "b"), debug_assertions)), derive(Clone))]
struct Both(i32);

struct A;
macro_rules! imp {
    ($t:ident) => {
        impl $t {
            fn tag(&self) -> u32 {
                1
            }
        }
    };
}
imp!(A);
#[cfg(feature = "stable_graph")]
imp!(A);

fn gated() -> () {
    let x = {
        #[cfg(feature = "nope")]
        {
            5u8
        }
    };
    x
}

fn main() {
    let s = St { a: 3 };
    println!("{:?} {}", s.clone(), s == s);
    let mut set = std::collections::HashSet::new();
    set.insert(Status::Ok);
    set.insert(Status::End);
    set.insert(Status::Ok);
    println!("{} {:?}", set.len(), Status::End);
    println!("{:?}", Pair::default());
    println!("{}", Both(4).clone().0);
    println!("{} {:?}", A.tag(), gated());
}
