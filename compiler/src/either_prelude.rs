// (either prelude)
// The bundled `either` crate: `Either<L, R>` with its common methods.

/// `either::Either`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    pub fn is_left(&self) -> bool {
        match self {
            Either::Left(_) => true,
            Either::Right(_) => false,
        }
    }
    pub fn is_right(&self) -> bool {
        !self.is_left()
    }
    pub fn left(self) -> Option<L> {
        match self {
            Either::Left(l) => Some(l),
            Either::Right(_) => None,
        }
    }
    pub fn right(self) -> Option<R> {
        match self {
            Either::Left(_) => None,
            Either::Right(r) => Some(r),
        }
    }
    pub fn left_or(self, other: L) -> L {
        match self {
            Either::Left(l) => l,
            Either::Right(_) => other,
        }
    }
    pub fn right_or(self, other: R) -> R {
        match self {
            Either::Left(_) => other,
            Either::Right(r) => r,
        }
    }
    pub fn left_or_else<F: FnOnce(R) -> L>(self, f: F) -> L {
        match self {
            Either::Left(l) => l,
            Either::Right(r) => f(r),
        }
    }
    pub fn right_or_else<F: FnOnce(L) -> R>(self, f: F) -> R {
        match self {
            Either::Left(l) => f(l),
            Either::Right(r) => r,
        }
    }
    pub fn unwrap_left(self) -> L {
        match self {
            Either::Left(l) => l,
            Either::Right(_) => panic!("called `Either::unwrap_left()` on a `Right` value"),
        }
    }
    pub fn unwrap_right(self) -> R {
        match self {
            Either::Left(_) => panic!("called `Either::unwrap_right()` on a `Left` value"),
            Either::Right(r) => r,
        }
    }
    pub fn expect_left(self, msg: &str) -> L {
        match self {
            Either::Left(l) => l,
            Either::Right(_) => panic!("{}", msg),
        }
    }
    pub fn expect_right(self, msg: &str) -> R {
        match self {
            Either::Left(_) => panic!("{}", msg),
            Either::Right(r) => r,
        }
    }
    pub fn flip(self) -> Either<R, L> {
        match self {
            Either::Left(l) => Either::Right(l),
            Either::Right(r) => Either::Left(r),
        }
    }
    pub fn map_left<M, F: FnOnce(L) -> M>(self, f: F) -> Either<M, R> {
        match self {
            Either::Left(l) => Either::Left(f(l)),
            Either::Right(r) => Either::Right(r),
        }
    }
    pub fn map_right<S, F: FnOnce(R) -> S>(self, f: F) -> Either<L, S> {
        match self {
            Either::Left(l) => Either::Left(l),
            Either::Right(r) => Either::Right(f(r)),
        }
    }
    pub fn either<T, F: FnOnce(L) -> T, G: FnOnce(R) -> T>(self, f: F, g: G) -> T {
        match self {
            Either::Left(l) => f(l),
            Either::Right(r) => g(r),
        }
    }
}

impl<L: std::fmt::Display, R: std::fmt::Display> std::fmt::Display for Either<L, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Either::Left(l) => write!(f, "{}", l),
            Either::Right(r) => write!(f, "{}", r),
        }
    }
}
