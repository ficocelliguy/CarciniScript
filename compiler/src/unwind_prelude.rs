// (unwind prelude)
// `std::panic::AssertUnwindSafe`, compiled in when a program names it. Nothing here checks unwind
// safety: the wrapper only has to give back what it holds (`catch_unwind` calls a wrapped closure).

/// `std::panic::AssertUnwindSafe<T>`: `T`, through `Deref` / `DerefMut` and `.0`.
#[derive(Default)]
pub struct AssertUnwindSafe<T>(pub T);

impl<T> std::ops::Deref for AssertUnwindSafe<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for AssertUnwindSafe<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for AssertUnwindSafe<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_tuple("AssertUnwindSafe").field(&self.0).finish()
    }
}
