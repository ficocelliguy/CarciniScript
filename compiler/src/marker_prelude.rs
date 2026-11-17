// (marker prelude)
// `std::marker::PhantomData` (and `Infallible`, `alloc::Global`), compiled in when a program
// names it: a value with nothing in it, so `PhantomData`, `PhantomData::<T>` and
// `Default::default()` all make one.

/// `std::marker::PhantomData<T>`: zero-size, every one equal to every other.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PhantomData<T: ?Sized>;

impl<T: ?Sized> std::fmt::Debug for PhantomData<T> {
    // (Rust names `T` here; its name isn't known at run time.)
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "PhantomData<_>")
    }
}

/// `std::convert::Infallible`: no value has it (`Result<T, Infallible>` is always `Ok`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Infallible {}

impl std::fmt::Display for Infallible {
    // (Never called: there's no value to show.)
    fn fmt(&self, _f: &mut std::fmt::Formatter) -> std::fmt::Result {
        Ok(())
    }
}

impl std::error::Error for Infallible {}

/// `std::alloc::Global` / `System`: the allocator, which nothing here uses (an `A: Allocator`
/// parameter is ignored, `Vec::new_in(Global)` is `Vec::new()`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct AllocGlobal;

impl std::fmt::Debug for AllocGlobal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Global")
    }
}
