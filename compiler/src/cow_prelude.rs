// (cow prelude)
// `std::borrow::Cow`, compiled in when a program uses it. A `String` and a `str` (a `Vec<T>` and
// a `[T]`) are one type here, so `Owned` holds the same type `Borrowed` points to.

/// `std::borrow::Cow`: borrowed data, or an owned copy once it had to change.
pub enum Cow<'a, B: ?Sized + 'a> {
    Borrowed(&'a B),
    Owned(B),
}

impl<'a, B: ?Sized + Clone> Cow<'a, B> {
    /// The owned data (a copy if it was borrowed).
    pub fn into_owned(self) -> B {
        match self {
            Cow::Borrowed(b) => b.clone(),
            Cow::Owned(o) => o,
        }
    }
    /// The data to change, copied first if it was borrowed. (A `String` can't change in place
    /// here: this is for slices and other values.)
    pub fn to_mut(&mut self) -> &mut B {
        js::__cow_to_mut(self)
    }
}

impl<'a, B: ?Sized> std::ops::Deref for Cow<'a, B> {
    type Target = B;
    fn deref(&self) -> &B {
        match self {
            Cow::Borrowed(b) => b,
            Cow::Owned(o) => o,
        }
    }
}

impl<'a, B: ?Sized + Clone> Clone for Cow<'a, B> {
    fn clone(&self) -> Cow<'a, B> {
        match self {
            Cow::Borrowed(b) => Cow::Borrowed(*b),
            Cow::Owned(o) => Cow::Owned(o.clone()),
        }
    }
}

impl<'a, B: ?Sized + PartialEq> PartialEq for Cow<'a, B> {
    fn eq(&self, other: &Cow<'a, B>) -> bool {
        let a: &B = match self {
            Cow::Borrowed(b) => b,
            Cow::Owned(o) => o,
        };
        let b: &B = match other {
            Cow::Borrowed(b) => b,
            Cow::Owned(o) => o,
        };
        a == b
    }
}

impl<'a, B: ?Sized + Eq> Eq for Cow<'a, B> {}

impl<'a, B: ?Sized + std::fmt::Display> std::fmt::Display for Cow<'a, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Cow::Borrowed(b) => write!(f, "{}", b),
            Cow::Owned(o) => write!(f, "{}", o),
        }
    }
}

impl<'a, B: ?Sized + std::fmt::Debug> std::fmt::Debug for Cow<'a, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Cow::Borrowed(b) => write!(f, "{:?}", b),
            Cow::Owned(o) => write!(f, "{:?}", o),
        }
    }
}

impl<'a> From<&'a str> for Cow<'a, str> {
    fn from(s: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(s)
    }
}

// (`&[T]` and `Vec<T>` are one type here: this serves `Cow::from(&v[..])` too, as `Owned`.)
impl<'a, T: Clone> From<Vec<T>> for Cow<'a, [T]> {
    fn from(v: Vec<T>) -> Cow<'a, [T]> {
        Cow::Owned(v)
    }
}
