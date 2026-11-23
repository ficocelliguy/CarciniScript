// (bound prelude)
// `std::ops::Bound`, compiled in when a program names it: one end of a range, as a value.

/// `std::ops::Bound<T>`: `Included(x)`, `Excluded(x)` or `Unbounded`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Bound<T> {
    Included(T),
    Excluded(T),
    Unbounded,
}

impl<T> Bound<T> {
    pub fn as_ref(&self) -> Bound<&T> {
        match self {
            Bound::Included(x) => Bound::Included(x),
            Bound::Excluded(x) => Bound::Excluded(x),
            Bound::Unbounded => Bound::Unbounded,
        }
    }

    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Bound<U> {
        match self {
            Bound::Included(x) => Bound::Included(f(x)),
            Bound::Excluded(x) => Bound::Excluded(f(x)),
            Bound::Unbounded => Bound::Unbounded,
        }
    }
}

impl<T: Clone> Bound<&T> {
    pub fn cloned(self) -> Bound<T> {
        match self {
            Bound::Included(x) => Bound::Included(x.clone()),
            Bound::Excluded(x) => Bound::Excluded(x.clone()),
            Bound::Unbounded => Bound::Unbounded,
        }
    }
}
