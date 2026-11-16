// (sync prelude)
// What CarciniScript compiles in for `std::sync::{LazyLock, OnceLock}`, `std::cell::{LazyCell,
// OnceCell}` and the bundled `once_cell` (`Lazy`, `OnceCell`) when a program uses them. Scripts
// are single-threaded, so the sync and unsync kinds are the same. A lazy value is computed when
// it's created (a `static`'s when the script loads), not on first use.

/// `std::sync::LazyLock`.
pub struct LazyLock<T> {
    __v: T,
}

impl<T> LazyLock<T> {
    pub fn new<F: FnOnce() -> T>(f: F) -> LazyLock<T> {
        LazyLock { __v: f() }
    }
    pub fn force(this: &LazyLock<T>) -> &T {
        &this.__v
    }
}

impl<T> std::ops::Deref for LazyLock<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.__v
    }
}

/// `std::cell::LazyCell`.
pub struct LazyCell<T> {
    __v: T,
}

impl<T> LazyCell<T> {
    pub fn new<F: FnOnce() -> T>(f: F) -> LazyCell<T> {
        LazyCell { __v: f() }
    }
    pub fn force(this: &LazyCell<T>) -> &T {
        &this.__v
    }
}

impl<T> std::ops::Deref for LazyCell<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.__v
    }
}

/// `once_cell::sync::Lazy` / `once_cell::unsync::Lazy`.
pub struct Lazy<T> {
    __v: T,
}

impl<T> Lazy<T> {
    pub fn new<F: FnOnce() -> T>(f: F) -> Lazy<T> {
        Lazy { __v: f() }
    }
    pub fn force(this: &Lazy<T>) -> &T {
        &this.__v
    }
}

impl<T> std::ops::Deref for Lazy<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.__v
    }
}

/// `std::sync::OnceLock`.
pub struct OnceLock<T> {
    __v: std::cell::RefCell<Option<T>>,
}

impl<T> OnceLock<T> {
    pub fn new() -> OnceLock<T> {
        OnceLock {
            __v: std::cell::RefCell::new(None),
        }
    }
    pub fn get(&self) -> Option<&T> {
        self.__v.borrow().as_ref()
    }
    pub fn set(&self, value: T) -> Result<(), T> {
        if self.__v.borrow().is_some() {
            return Err(value);
        }
        *self.__v.borrow_mut() = Some(value);
        Ok(())
    }
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        if self.__v.borrow().is_none() {
            let v = f();
            *self.__v.borrow_mut() = Some(v);
        }
        self.get().unwrap()
    }
}

/// `std::cell::OnceCell` / `once_cell::sync::OnceCell` / `once_cell::unsync::OnceCell`.
pub struct OnceCell<T> {
    __v: std::cell::RefCell<Option<T>>,
}

impl<T> OnceCell<T> {
    pub fn new() -> OnceCell<T> {
        OnceCell {
            __v: std::cell::RefCell::new(None),
        }
    }
    pub fn get(&self) -> Option<&T> {
        self.__v.borrow().as_ref()
    }
    pub fn set(&self, value: T) -> Result<(), T> {
        if self.__v.borrow().is_some() {
            return Err(value);
        }
        *self.__v.borrow_mut() = Some(value);
        Ok(())
    }
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        if self.__v.borrow().is_none() {
            let v = f();
            *self.__v.borrow_mut() = Some(v);
        }
        self.get().unwrap()
    }
}

impl<T> Default for OnceLock<T> {
    fn default() -> OnceLock<T> {
        OnceLock::new()
    }
}

impl<T: Clone> Clone for OnceLock<T> {
    fn clone(&self) -> OnceLock<T> {
        OnceLock {
            __v: std::cell::RefCell::new(self.__v.borrow().clone()),
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for OnceLock<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.get() {
            Some(v) => write!(f, "OnceLock({:?})", v),
            None => write!(f, "OnceLock(<uninit>)"),
        }
    }
}

impl<T> Default for OnceCell<T> {
    fn default() -> OnceCell<T> {
        OnceCell::new()
    }
}

impl<T: Clone> Clone for OnceCell<T> {
    fn clone(&self) -> OnceCell<T> {
        OnceCell {
            __v: std::cell::RefCell::new(self.__v.borrow().clone()),
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for OnceCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.get() {
            Some(v) => write!(f, "OnceCell({:?})", v),
            None => write!(f, "OnceCell(<uninit>)"),
        }
    }
}
