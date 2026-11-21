// (lock prelude)
// What CarciniScript compiles in for `std::sync::{Mutex, RwLock}`, the atomics and
// `thread_local!` when a program uses them. A script is single-threaded, so a lock is a
// `RefCell` (locking never blocks or fails, and nothing is ever poisoned) and an atomic is a
// `Cell` (every `Ordering` is the same). `m.lock().unwrap()` (and `read` / `write` / `get_mut`,
// with `unwrap` or `expect`) is the cell's value in place, so `*m.lock().unwrap() += 1` and
// `let mut g = m.lock().unwrap(); *g += 1;` write through; that's `codegen.rs` `lock_place`.

/// `std::sync::Mutex`.
pub struct Mutex<T> {
    __c: std::cell::RefCell<T>,
}

/// `std::sync::PoisonError`: never made (a panic ends the script).
#[derive(Debug)]
pub struct PoisonError<T> {
    __g: T,
}

impl<T> PoisonError<T> {
    pub fn new(guard: T) -> PoisonError<T> {
        PoisonError { __g: guard }
    }
    pub fn into_inner(self) -> T {
        self.__g
    }
    pub fn get_ref(&self) -> &T {
        &self.__g
    }
}

impl<T> std::fmt::Display for PoisonError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "poisoned lock: another task failed inside")
    }
}

/// `std::sync::TryLockError`: never made (a lock is never held across an `.await` by another
/// task in a way `try_lock` could see).
#[derive(Debug)]
pub enum TryLockError<T> {
    Poisoned(PoisonError<T>),
    WouldBlock,
}

impl<T> std::fmt::Display for TryLockError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TryLockError::Poisoned(_) => write!(f, "poisoned lock: another task failed inside"),
            TryLockError::WouldBlock => write!(f, "try_lock failed because the operation would block"),
        }
    }
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Mutex<T> {
        Mutex {
            __c: std::cell::RefCell::new(value),
        }
    }
    pub fn lock(&self) -> Result<&mut T, PoisonError<&mut T>> {
        Ok(self.__c.borrow_mut())
    }
    pub fn try_lock(
        &self,
    ) -> Result<&mut T, TryLockError<&mut T>> {
        Ok(self.__c.borrow_mut())
    }
    pub fn get_mut(&mut self) -> Result<&mut T, PoisonError<&mut T>> {
        Ok(self.__c.borrow_mut())
    }
    pub fn into_inner(self) -> Result<T, PoisonError<T>> {
        Ok(self.__c.into_inner())
    }
    pub fn is_poisoned(&self) -> bool {
        false
    }
    pub fn clear_poison(&self) {}
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Mutex<T> {
        Mutex::new(T::default())
    }
}

impl<T> From<T> for Mutex<T> {
    fn from(value: T) -> Mutex<T> {
        Mutex::new(value)
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Mutex {{ data: {:?}, poisoned: false, .. }}", &*self.__c.borrow())
    }
}

/// `std::sync::RwLock`.
pub struct RwLock<T> {
    __c: std::cell::RefCell<T>,
}

impl<T> RwLock<T> {
    pub const fn new(value: T) -> RwLock<T> {
        RwLock {
            __c: std::cell::RefCell::new(value),
        }
    }
    pub fn read(&self) -> Result<&T, PoisonError<&T>> {
        Ok(self.__c.borrow())
    }
    pub fn write(
        &self,
    ) -> Result<&mut T, PoisonError<&mut T>> {
        Ok(self.__c.borrow_mut())
    }
    pub fn try_read(&self) -> Result<&T, TryLockError<&T>> {
        Ok(self.__c.borrow())
    }
    pub fn try_write(
        &self,
    ) -> Result<&mut T, TryLockError<&mut T>> {
        Ok(self.__c.borrow_mut())
    }
    pub fn get_mut(&mut self) -> Result<&mut T, PoisonError<&mut T>> {
        Ok(self.__c.borrow_mut())
    }
    pub fn into_inner(self) -> Result<T, PoisonError<T>> {
        Ok(self.__c.into_inner())
    }
    pub fn is_poisoned(&self) -> bool {
        false
    }
    pub fn clear_poison(&self) {}
}

impl<T: Default> Default for RwLock<T> {
    fn default() -> RwLock<T> {
        RwLock::new(T::default())
    }
}

impl<T> From<T> for RwLock<T> {
    fn from(value: T) -> RwLock<T> {
        RwLock::new(value)
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "RwLock {{ data: {:?}, poisoned: false, .. }}", &*self.__c.borrow())
    }
}

/// `std::sync::atomic::Ordering` (after the rename; `std::cmp::Ordering` keeps its name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomicOrdering {
    Relaxed,
    Release,
    Acquire,
    AcqRel,
    SeqCst,
}

/// `std::sync::atomic::fence`.
pub fn __atomic_fence(_order: AtomicOrdering) {}

/// `std::sync::atomic::AtomicBool`.
pub struct AtomicBool {
    __c: std::cell::Cell<bool>,
}

impl AtomicBool {
    pub const fn new(value: bool) -> AtomicBool {
        AtomicBool {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> bool {
        self.__c.get()
    }
    pub fn store(&self, value: bool, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: bool, _order: AtomicOrdering) -> bool {
        self.__c.replace(value)
    }
    pub fn fetch_and(&self, value: bool, _order: AtomicOrdering) -> bool {
        let old = self.__c.get();
        self.__c.set(old && value);
        old
    }
    pub fn fetch_or(&self, value: bool, _order: AtomicOrdering) -> bool {
        let old = self.__c.get();
        self.__c.set(old || value);
        old
    }
    pub fn fetch_xor(&self, value: bool, _order: AtomicOrdering) -> bool {
        let old = self.__c.get();
        self.__c.set(old != value);
        old
    }
    pub fn fetch_nand(&self, value: bool, _order: AtomicOrdering) -> bool {
        let old = self.__c.get();
        self.__c.set(!(old && value));
        old
    }
    pub fn fetch_not(&self, _order: AtomicOrdering) -> bool {
        let old = self.__c.get();
        self.__c.set(!old);
        old
    }
    pub fn compare_exchange(
        &self,
        current: bool,
        new: bool,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<bool, bool> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: bool,
        new: bool,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<bool, bool> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(bool) -> Option<bool>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<bool, bool> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> bool {
        self.__c.get()
    }
}

impl Default for AtomicBool {
    fn default() -> AtomicBool {
        AtomicBool::new(false)
    }
}

impl From<bool> for AtomicBool {
    fn from(value: bool) -> AtomicBool {
        AtomicBool::new(value)
    }
}

impl std::fmt::Debug for AtomicBool {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicUsize`.
pub struct AtomicUsize {
    __c: std::cell::Cell<usize>,
}

impl AtomicUsize {
    pub const fn new(value: usize) -> AtomicUsize {
        AtomicUsize {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> usize {
        self.__c.get()
    }
    pub fn store(&self, value: usize, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: usize, _order: AtomicOrdering) -> usize {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: usize, _order: AtomicOrdering) -> usize {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: usize, _order: AtomicOrdering) -> usize {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: usize, _order: AtomicOrdering) -> usize {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: usize, _order: AtomicOrdering) -> usize {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: usize, _order: AtomicOrdering) -> usize {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: usize, _order: AtomicOrdering) -> usize {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: usize, _order: AtomicOrdering) -> usize {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: usize, _order: AtomicOrdering) -> usize {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: usize,
        new: usize,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<usize, usize> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: usize,
        new: usize,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<usize, usize> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(usize) -> Option<usize>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<usize, usize> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> usize {
        self.__c.get()
    }
}

impl Default for AtomicUsize {
    fn default() -> AtomicUsize {
        AtomicUsize::new(0)
    }
}

impl From<usize> for AtomicUsize {
    fn from(value: usize) -> AtomicUsize {
        AtomicUsize::new(value)
    }
}

impl std::fmt::Debug for AtomicUsize {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicIsize`.
pub struct AtomicIsize {
    __c: std::cell::Cell<isize>,
}

impl AtomicIsize {
    pub const fn new(value: isize) -> AtomicIsize {
        AtomicIsize {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> isize {
        self.__c.get()
    }
    pub fn store(&self, value: isize, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: isize, _order: AtomicOrdering) -> isize {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: isize, _order: AtomicOrdering) -> isize {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: isize, _order: AtomicOrdering) -> isize {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: isize, _order: AtomicOrdering) -> isize {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: isize, _order: AtomicOrdering) -> isize {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: isize, _order: AtomicOrdering) -> isize {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: isize, _order: AtomicOrdering) -> isize {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: isize, _order: AtomicOrdering) -> isize {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: isize, _order: AtomicOrdering) -> isize {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: isize,
        new: isize,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<isize, isize> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: isize,
        new: isize,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<isize, isize> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(isize) -> Option<isize>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<isize, isize> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> isize {
        self.__c.get()
    }
}

impl Default for AtomicIsize {
    fn default() -> AtomicIsize {
        AtomicIsize::new(0)
    }
}

impl From<isize> for AtomicIsize {
    fn from(value: isize) -> AtomicIsize {
        AtomicIsize::new(value)
    }
}

impl std::fmt::Debug for AtomicIsize {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicU8`.
pub struct AtomicU8 {
    __c: std::cell::Cell<u8>,
}

impl AtomicU8 {
    pub const fn new(value: u8) -> AtomicU8 {
        AtomicU8 {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> u8 {
        self.__c.get()
    }
    pub fn store(&self, value: u8, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: u8, _order: AtomicOrdering) -> u8 {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: u8, _order: AtomicOrdering) -> u8 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: u8, _order: AtomicOrdering) -> u8 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: u8, _order: AtomicOrdering) -> u8 {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: u8, _order: AtomicOrdering) -> u8 {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: u8, _order: AtomicOrdering) -> u8 {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: u8, _order: AtomicOrdering) -> u8 {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: u8, _order: AtomicOrdering) -> u8 {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: u8, _order: AtomicOrdering) -> u8 {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: u8,
        new: u8,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<u8, u8> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: u8,
        new: u8,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<u8, u8> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(u8) -> Option<u8>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<u8, u8> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> u8 {
        self.__c.get()
    }
}

impl Default for AtomicU8 {
    fn default() -> AtomicU8 {
        AtomicU8::new(0)
    }
}

impl From<u8> for AtomicU8 {
    fn from(value: u8) -> AtomicU8 {
        AtomicU8::new(value)
    }
}

impl std::fmt::Debug for AtomicU8 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicU16`.
pub struct AtomicU16 {
    __c: std::cell::Cell<u16>,
}

impl AtomicU16 {
    pub const fn new(value: u16) -> AtomicU16 {
        AtomicU16 {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> u16 {
        self.__c.get()
    }
    pub fn store(&self, value: u16, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: u16, _order: AtomicOrdering) -> u16 {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: u16, _order: AtomicOrdering) -> u16 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: u16, _order: AtomicOrdering) -> u16 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: u16, _order: AtomicOrdering) -> u16 {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: u16, _order: AtomicOrdering) -> u16 {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: u16, _order: AtomicOrdering) -> u16 {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: u16, _order: AtomicOrdering) -> u16 {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: u16, _order: AtomicOrdering) -> u16 {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: u16, _order: AtomicOrdering) -> u16 {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: u16,
        new: u16,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<u16, u16> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: u16,
        new: u16,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<u16, u16> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(u16) -> Option<u16>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<u16, u16> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> u16 {
        self.__c.get()
    }
}

impl Default for AtomicU16 {
    fn default() -> AtomicU16 {
        AtomicU16::new(0)
    }
}

impl From<u16> for AtomicU16 {
    fn from(value: u16) -> AtomicU16 {
        AtomicU16::new(value)
    }
}

impl std::fmt::Debug for AtomicU16 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicU32`.
pub struct AtomicU32 {
    __c: std::cell::Cell<u32>,
}

impl AtomicU32 {
    pub const fn new(value: u32) -> AtomicU32 {
        AtomicU32 {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> u32 {
        self.__c.get()
    }
    pub fn store(&self, value: u32, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: u32, _order: AtomicOrdering) -> u32 {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: u32, _order: AtomicOrdering) -> u32 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: u32, _order: AtomicOrdering) -> u32 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: u32, _order: AtomicOrdering) -> u32 {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: u32, _order: AtomicOrdering) -> u32 {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: u32, _order: AtomicOrdering) -> u32 {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: u32, _order: AtomicOrdering) -> u32 {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: u32, _order: AtomicOrdering) -> u32 {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: u32, _order: AtomicOrdering) -> u32 {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: u32,
        new: u32,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<u32, u32> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: u32,
        new: u32,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<u32, u32> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(u32) -> Option<u32>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<u32, u32> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> u32 {
        self.__c.get()
    }
}

impl Default for AtomicU32 {
    fn default() -> AtomicU32 {
        AtomicU32::new(0)
    }
}

impl From<u32> for AtomicU32 {
    fn from(value: u32) -> AtomicU32 {
        AtomicU32::new(value)
    }
}

impl std::fmt::Debug for AtomicU32 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicU64`.
pub struct AtomicU64 {
    __c: std::cell::Cell<u64>,
}

impl AtomicU64 {
    pub const fn new(value: u64) -> AtomicU64 {
        AtomicU64 {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> u64 {
        self.__c.get()
    }
    pub fn store(&self, value: u64, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: u64, _order: AtomicOrdering) -> u64 {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: u64, _order: AtomicOrdering) -> u64 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: u64, _order: AtomicOrdering) -> u64 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: u64, _order: AtomicOrdering) -> u64 {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: u64, _order: AtomicOrdering) -> u64 {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: u64, _order: AtomicOrdering) -> u64 {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: u64, _order: AtomicOrdering) -> u64 {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: u64, _order: AtomicOrdering) -> u64 {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: u64, _order: AtomicOrdering) -> u64 {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: u64,
        new: u64,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<u64, u64> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: u64,
        new: u64,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<u64, u64> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(u64) -> Option<u64>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<u64, u64> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> u64 {
        self.__c.get()
    }
}

impl Default for AtomicU64 {
    fn default() -> AtomicU64 {
        AtomicU64::new(0)
    }
}

impl From<u64> for AtomicU64 {
    fn from(value: u64) -> AtomicU64 {
        AtomicU64::new(value)
    }
}

impl std::fmt::Debug for AtomicU64 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicI8`.
pub struct AtomicI8 {
    __c: std::cell::Cell<i8>,
}

impl AtomicI8 {
    pub const fn new(value: i8) -> AtomicI8 {
        AtomicI8 {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> i8 {
        self.__c.get()
    }
    pub fn store(&self, value: i8, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: i8, _order: AtomicOrdering) -> i8 {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: i8, _order: AtomicOrdering) -> i8 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: i8, _order: AtomicOrdering) -> i8 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: i8, _order: AtomicOrdering) -> i8 {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: i8, _order: AtomicOrdering) -> i8 {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: i8, _order: AtomicOrdering) -> i8 {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: i8, _order: AtomicOrdering) -> i8 {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: i8, _order: AtomicOrdering) -> i8 {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: i8, _order: AtomicOrdering) -> i8 {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: i8,
        new: i8,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<i8, i8> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: i8,
        new: i8,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<i8, i8> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(i8) -> Option<i8>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<i8, i8> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> i8 {
        self.__c.get()
    }
}

impl Default for AtomicI8 {
    fn default() -> AtomicI8 {
        AtomicI8::new(0)
    }
}

impl From<i8> for AtomicI8 {
    fn from(value: i8) -> AtomicI8 {
        AtomicI8::new(value)
    }
}

impl std::fmt::Debug for AtomicI8 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicI16`.
pub struct AtomicI16 {
    __c: std::cell::Cell<i16>,
}

impl AtomicI16 {
    pub const fn new(value: i16) -> AtomicI16 {
        AtomicI16 {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> i16 {
        self.__c.get()
    }
    pub fn store(&self, value: i16, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: i16, _order: AtomicOrdering) -> i16 {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: i16, _order: AtomicOrdering) -> i16 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: i16, _order: AtomicOrdering) -> i16 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: i16, _order: AtomicOrdering) -> i16 {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: i16, _order: AtomicOrdering) -> i16 {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: i16, _order: AtomicOrdering) -> i16 {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: i16, _order: AtomicOrdering) -> i16 {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: i16, _order: AtomicOrdering) -> i16 {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: i16, _order: AtomicOrdering) -> i16 {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: i16,
        new: i16,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<i16, i16> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: i16,
        new: i16,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<i16, i16> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(i16) -> Option<i16>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<i16, i16> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> i16 {
        self.__c.get()
    }
}

impl Default for AtomicI16 {
    fn default() -> AtomicI16 {
        AtomicI16::new(0)
    }
}

impl From<i16> for AtomicI16 {
    fn from(value: i16) -> AtomicI16 {
        AtomicI16::new(value)
    }
}

impl std::fmt::Debug for AtomicI16 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicI32`.
pub struct AtomicI32 {
    __c: std::cell::Cell<i32>,
}

impl AtomicI32 {
    pub const fn new(value: i32) -> AtomicI32 {
        AtomicI32 {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> i32 {
        self.__c.get()
    }
    pub fn store(&self, value: i32, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: i32, _order: AtomicOrdering) -> i32 {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: i32, _order: AtomicOrdering) -> i32 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: i32, _order: AtomicOrdering) -> i32 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: i32, _order: AtomicOrdering) -> i32 {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: i32, _order: AtomicOrdering) -> i32 {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: i32, _order: AtomicOrdering) -> i32 {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: i32, _order: AtomicOrdering) -> i32 {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: i32, _order: AtomicOrdering) -> i32 {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: i32, _order: AtomicOrdering) -> i32 {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: i32,
        new: i32,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<i32, i32> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: i32,
        new: i32,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<i32, i32> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(i32) -> Option<i32>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<i32, i32> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> i32 {
        self.__c.get()
    }
}

impl Default for AtomicI32 {
    fn default() -> AtomicI32 {
        AtomicI32::new(0)
    }
}

impl From<i32> for AtomicI32 {
    fn from(value: i32) -> AtomicI32 {
        AtomicI32::new(value)
    }
}

impl std::fmt::Debug for AtomicI32 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}

/// `std::sync::atomic::AtomicI64`.
pub struct AtomicI64 {
    __c: std::cell::Cell<i64>,
}

impl AtomicI64 {
    pub const fn new(value: i64) -> AtomicI64 {
        AtomicI64 {
            __c: std::cell::Cell::new(value),
        }
    }
    pub fn load(&self, _order: AtomicOrdering) -> i64 {
        self.__c.get()
    }
    pub fn store(&self, value: i64, _order: AtomicOrdering) {
        self.__c.set(value)
    }
    pub fn swap(&self, value: i64, _order: AtomicOrdering) -> i64 {
        self.__c.replace(value)
    }
    pub fn fetch_add(&self, value: i64, _order: AtomicOrdering) -> i64 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_add(value));
        old
    }
    pub fn fetch_sub(&self, value: i64, _order: AtomicOrdering) -> i64 {
        let old = self.__c.get();
        self.__c.set(old.wrapping_sub(value));
        old
    }
    pub fn fetch_max(&self, value: i64, _order: AtomicOrdering) -> i64 {
        let old = self.__c.get();
        self.__c.set(old.max(value));
        old
    }
    pub fn fetch_min(&self, value: i64, _order: AtomicOrdering) -> i64 {
        let old = self.__c.get();
        self.__c.set(old.min(value));
        old
    }
    pub fn fetch_and(&self, value: i64, _order: AtomicOrdering) -> i64 {
        let old = self.__c.get();
        self.__c.set(old & value);
        old
    }
    pub fn fetch_or(&self, value: i64, _order: AtomicOrdering) -> i64 {
        let old = self.__c.get();
        self.__c.set(old | value);
        old
    }
    pub fn fetch_xor(&self, value: i64, _order: AtomicOrdering) -> i64 {
        let old = self.__c.get();
        self.__c.set(old ^ value);
        old
    }
    pub fn fetch_nand(&self, value: i64, _order: AtomicOrdering) -> i64 {
        let old = self.__c.get();
        self.__c.set(!(old & value));
        old
    }
    pub fn compare_exchange(
        &self,
        current: i64,
        new: i64,
        _success: AtomicOrdering,
        _failure: AtomicOrdering,
    ) -> Result<i64, i64> {
        let old = self.__c.get();
        if old == current {
            self.__c.set(new);
            Ok(old)
        } else {
            Err(old)
        }
    }
    pub fn compare_exchange_weak(
        &self,
        current: i64,
        new: i64,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    ) -> Result<i64, i64> {
        self.compare_exchange(current, new, success, failure)
    }
    pub fn fetch_update<F: FnMut(i64) -> Option<i64>>(
        &self,
        _set_order: AtomicOrdering,
        _fetch_order: AtomicOrdering,
        mut f: F,
    ) -> Result<i64, i64> {
        let old = self.__c.get();
        match f(old) {
            Some(new) => {
                self.__c.set(new);
                Ok(old)
            }
            None => Err(old),
        }
    }
    pub fn into_inner(self) -> i64 {
        self.__c.get()
    }
}

impl Default for AtomicI64 {
    fn default() -> AtomicI64 {
        AtomicI64::new(0)
    }
}

impl From<i64> for AtomicI64 {
    fn from(value: i64) -> AtomicI64 {
        AtomicI64::new(value)
    }
}

impl std::fmt::Debug for AtomicI64 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.__c.get(), f)
    }
}


/// `std::thread::LocalKey`: what `thread_local! { static K: T = init; }` declares (one thread,
/// so a `static`).
pub struct LocalKey<T> {
    __v: T,
}

impl<T> LocalKey<T> {
    pub const fn new(value: T) -> LocalKey<T> {
        LocalKey { __v: value }
    }
    pub fn with<R, F: FnOnce(&T) -> R>(&'static self, f: F) -> R {
        f(&self.__v)
    }
    pub fn try_with<R, F: FnOnce(&T) -> R>(&'static self, f: F) -> Result<R, AccessError> {
        Ok(f(&self.__v))
    }
}

impl<T> LocalKey<std::cell::RefCell<T>> {
    pub fn with_borrow<R, F: FnOnce(&T) -> R>(&'static self, f: F) -> R {
        f(&*self.__v.borrow())
    }
    pub fn with_borrow_mut<R, F: FnOnce(&mut T) -> R>(&'static self, f: F) -> R {
        let mut g = self.__v.borrow_mut();
        f(&mut *g)
    }
    pub fn set(&'static self, value: T) {
        *self.__v.borrow_mut() = value;
    }
    pub fn replace(&'static self, value: T) -> T {
        self.__v.replace(value)
    }
}

impl<T: Default> LocalKey<std::cell::RefCell<T>> {
    pub fn take(&'static self) -> T {
        self.__v.take()
    }
}

impl<T: Copy> LocalKey<std::cell::Cell<T>> {
    pub fn get(&'static self) -> T {
        self.__v.get()
    }
}

/// `std::thread::AccessError`: never made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessError {}

/// `std::sync::mpsc::Sender` / `SyncSender` (after the rename): one queue that every clone and
/// the receiver share. Sending never blocks (a `sync_channel`'s bound is ignored).
pub struct MpscSender<T> {
    __q: std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<T>>>,
}

/// `std::sync::mpsc::Receiver`. One thread, so a `recv` that would wait forever is an error: an
/// empty channel is `Err(RecvError)` (`try_recv`: `Err(TryRecvError::Empty)`), and its iterators
/// stop.
pub struct MpscReceiver<T> {
    __q: std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<T>>>,
}

/// `std::sync::mpsc::channel()`.
pub fn __mpsc_channel<T>() -> (MpscSender<T>, MpscReceiver<T>) {
    let q = std::rc::Rc::new(std::cell::RefCell::new(std::collections::VecDeque::new()));
    (MpscSender { __q: q.clone() }, MpscReceiver { __q: q })
}

/// `std::sync::mpsc::sync_channel(bound)`.
pub fn __mpsc_sync_channel<T>(_bound: usize) -> (MpscSender<T>, MpscReceiver<T>) {
    __mpsc_channel()
}

impl<T> MpscSender<T> {
    pub fn send(&self, value: T) -> Result<(), MpscSendError<T>> {
        self.__q.borrow_mut().push_back(value);
        Ok(())
    }
    pub fn try_send(&self, value: T) -> Result<(), MpscTrySendError<T>> {
        self.__q.borrow_mut().push_back(value);
        Ok(())
    }
}

impl<T> Clone for MpscSender<T> {
    fn clone(&self) -> MpscSender<T> {
        MpscSender {
            __q: self.__q.clone(),
        }
    }
}

impl<T> std::fmt::Debug for MpscSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Sender {{ .. }}")
    }
}

impl<T> MpscReceiver<T> {
    pub fn recv(&self) -> Result<T, MpscRecvError> {
        match self.__q.borrow_mut().pop_front() {
            Some(v) => Ok(v),
            None => Err(MpscRecvError {}),
        }
    }
    pub fn try_recv(&self) -> Result<T, MpscTryRecvError> {
        match self.__q.borrow_mut().pop_front() {
            Some(v) => Ok(v),
            None => Err(MpscTryRecvError::Empty),
        }
    }
    pub fn recv_timeout(&self, _timeout: Duration) -> Result<T, MpscRecvTimeoutError> {
        match self.__q.borrow_mut().pop_front() {
            Some(v) => Ok(v),
            None => Err(MpscRecvTimeoutError::Timeout),
        }
    }
    pub fn iter(&self) -> std::vec::IntoIter<T> {
        let v: Vec<T> = self.__q.borrow_mut().drain(..).collect();
        v.into_iter()
    }
    pub fn try_iter(&self) -> std::vec::IntoIter<T> {
        self.iter()
    }
}

impl<T> IntoIterator for MpscReceiver<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> std::vec::IntoIter<T> {
        self.iter()
    }
}

impl<T> std::fmt::Debug for MpscReceiver<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Receiver {{ .. }}")
    }
}

/// `std::sync::mpsc::SendError`: never made (the receiver is never gone).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MpscSendError<T>(pub T);

impl<T> std::fmt::Debug for MpscSendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "SendError {{ .. }}")
    }
}

impl<T> std::fmt::Display for MpscSendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "sending on a closed channel")
    }
}

impl<T> std::error::Error for MpscSendError<T> {}

/// `std::sync::mpsc::TrySendError`: never made.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MpscTrySendError<T> {
    Full(T),
    Disconnected(T),
}

impl<T> std::fmt::Debug for MpscTrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MpscTrySendError::Full(_) => write!(f, "Full(..)"),
            MpscTrySendError::Disconnected(_) => write!(f, "Disconnected(..)"),
        }
    }
}

impl<T> std::fmt::Display for MpscTrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MpscTrySendError::Full(_) => write!(f, "sending on a full channel"),
            MpscTrySendError::Disconnected(_) => write!(f, "sending on a closed channel"),
        }
    }
}

/// `std::sync::mpsc::RecvError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpscRecvError {}

impl std::fmt::Display for MpscRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "receiving on a closed channel")
    }
}

impl std::error::Error for MpscRecvError {}

/// `std::sync::mpsc::TryRecvError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpscTryRecvError {
    Empty,
    Disconnected,
}

impl std::fmt::Display for MpscTryRecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MpscTryRecvError::Empty => write!(f, "receiving on an empty channel"),
            MpscTryRecvError::Disconnected => write!(f, "receiving on an empty and disconnected channel"),
        }
    }
}

impl std::error::Error for MpscTryRecvError {}

/// `std::sync::mpsc::RecvTimeoutError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpscRecvTimeoutError {
    Timeout,
    Disconnected,
}

impl std::fmt::Display for MpscRecvTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MpscRecvTimeoutError::Timeout => write!(f, "timed out waiting on channel"),
            MpscRecvTimeoutError::Disconnected => write!(f, "channel is empty and sending half is closed"),
        }
    }
}

impl std::error::Error for MpscRecvTimeoutError {}
