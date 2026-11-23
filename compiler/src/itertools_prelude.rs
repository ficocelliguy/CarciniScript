// (itertools prelude)
// The types the bundled `itertools` methods return (`κσ.$itertools` makes their values):
// `minmax`'s `MinMaxResult`, `fold_while`'s `FoldWhile` and `exactly_one`'s `ExactlyOneError`.

/// `itertools::MinMaxResult`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MinMaxResult<T> {
    NoElements,
    OneElement(T),
    MinMax(T, T),
}

impl<T: Clone> MinMaxResult<T> {
    pub fn into_option(self) -> Option<(T, T)> {
        match self {
            MinMaxResult::NoElements => None,
            MinMaxResult::OneElement(x) => Some((x.clone(), x)),
            MinMaxResult::MinMax(x, y) => Some((x, y)),
        }
    }
}

/// `itertools::FoldWhile`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FoldWhile<T> {
    Continue(T),
    Done(T),
}

impl<T> FoldWhile<T> {
    pub fn into_inner(self) -> T {
        match self {
            FoldWhile::Continue(x) => x,
            FoldWhile::Done(x) => x,
        }
    }
    pub fn is_done(&self) -> bool {
        match self {
            FoldWhile::Continue(_) => false,
            FoldWhile::Done(_) => true,
        }
    }
}

/// `itertools::ExactlyOneError`: an iterator over the items `exactly_one` took (none, or all of
/// them when there were two or more). itertools' is generic over the iterator, this one over
/// the items.
#[derive(Clone)]
pub struct ExactlyOneError<T> {
    items: Vec<T>,
    two: bool,
}

impl<T> Iterator for ExactlyOneError<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.remove(0))
        }
    }
}

impl<T> std::fmt::Display for ExactlyOneError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.two {
            write!(f, "got at least 2 elements when exactly one was expected")
        } else {
            write!(f, "got zero elements when exactly one was expected")
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for ExactlyOneError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.items.len() >= 2 {
            write!(
                f,
                "ExactlyOneError {{ first: {:?}, second: {:?}, inner: {:?} }}",
                self.items[0],
                self.items[1],
                &self.items[2..]
            )
        } else {
            write!(f, "ExactlyOneError {{ inner: {:?} }}", self.items)
        }
    }
}

impl<T: std::fmt::Debug> std::error::Error for ExactlyOneError<T> {}
