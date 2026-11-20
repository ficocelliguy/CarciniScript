// (control flow prelude)
// `std::ops::ControlFlow`, compiled in when a program names it: `Break` out of or `Continue` a
// walk, as a value (`try_for_each` and `try_fold` here still take a `Result` or an `Option`).

/// `std::ops::ControlFlow<B, C>`: `Continue(c)` or `Break(b)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ControlFlow<B, C = ()> {
    Continue(C),
    Break(B),
}

impl<B, C> ControlFlow<B, C> {
    pub fn is_break(&self) -> bool {
        matches!(self, ControlFlow::Break(_))
    }

    pub fn is_continue(&self) -> bool {
        matches!(self, ControlFlow::Continue(_))
    }

    pub fn break_value(self) -> Option<B> {
        match self {
            ControlFlow::Break(b) => Some(b),
            ControlFlow::Continue(_) => None,
        }
    }

    pub fn continue_value(self) -> Option<C> {
        match self {
            ControlFlow::Continue(c) => Some(c),
            ControlFlow::Break(_) => None,
        }
    }

    pub fn map_break<T>(self, f: impl FnOnce(B) -> T) -> ControlFlow<T, C> {
        match self {
            ControlFlow::Break(b) => ControlFlow::Break(f(b)),
            ControlFlow::Continue(c) => ControlFlow::Continue(c),
        }
    }

    pub fn map_continue<T>(self, f: impl FnOnce(C) -> T) -> ControlFlow<B, T> {
        match self {
            ControlFlow::Break(b) => ControlFlow::Break(b),
            ControlFlow::Continue(c) => ControlFlow::Continue(f(c)),
        }
    }
}
