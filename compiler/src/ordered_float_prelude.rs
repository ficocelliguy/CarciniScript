// (ordered_float prelude)
// The bundled `ordered-float` crate: `OrderedFloat<T>` (a total order with NaN greatest and all
// NaNs equal, so `f64` scores can be `BinaryHeap` / `BTreeMap` / `HashMap` keys and `sort()` /
// `max()` work) and `NotNan<T>` (a float that isn't NaN).

/// What `OrderedFloat` / `NotNan` need of `f32` and `f64` (the crate's `FloatCore`).
pub trait OrderedFloatCore: Copy + PartialEq + PartialOrd {
    fn __of_is_nan(self) -> bool;
    /// The bits `Hash` writes: every NaN alike, and `-0.0` as `0.0` (they're equal).
    fn __of_bits(self) -> u64;
}
impl OrderedFloatCore for f64 {
    fn __of_is_nan(self) -> bool {
        self.is_nan()
    }
    fn __of_bits(self) -> u64 {
        if self.is_nan() {
            0x7ff8000000000000
        } else if self == 0.0 {
            0
        } else {
            self.to_bits()
        }
    }
}
impl OrderedFloatCore for f32 {
    fn __of_is_nan(self) -> bool {
        self.is_nan()
    }
    fn __of_bits(self) -> u64 {
        if self.is_nan() {
            0x7fc00000
        } else if self == 0.0 {
            0
        } else {
            self.to_bits() as u64
        }
    }
}

/// `ordered_float::OrderedFloat`.
#[derive(Debug, Clone, Copy)]
pub struct OrderedFloat<T>(pub T);

impl<T: Default> Default for OrderedFloat<T> {
    fn default() -> Self {
        OrderedFloat(T::default())
    }
}

impl<T: OrderedFloatCore> OrderedFloat<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}
impl<T: OrderedFloatCore + PartialEq + PartialOrd> PartialEq for OrderedFloat<T> {
    fn eq(&self, other: &Self) -> bool {
        if self.0.__of_is_nan() {
            other.0.__of_is_nan()
        } else {
            self.0 == other.0
        }
    }
}
impl<T: OrderedFloatCore> Eq for OrderedFloat<T> {}
impl<T: OrderedFloatCore> PartialOrd for OrderedFloat<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T: OrderedFloatCore + PartialEq + PartialOrd> Ord for OrderedFloat<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let (a, b) = (self.0, other.0);
        match (a.__of_is_nan(), b.__of_is_nan()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => {
                if a < b {
                    std::cmp::Ordering::Less
                } else if a > b {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            }
        }
    }
}
impl<T: OrderedFloatCore> std::hash::Hash for OrderedFloat<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.__of_bits().hash(state);
    }
}
impl<T: std::fmt::Display> std::fmt::Display for OrderedFloat<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl<T> std::ops::Deref for OrderedFloat<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T> std::ops::DerefMut for OrderedFloat<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}
impl<T: OrderedFloatCore> From<T> for OrderedFloat<T> {
    fn from(x: T) -> Self {
        OrderedFloat(x)
    }
}
impl<T: std::ops::Add<Output = T>> std::ops::Add for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn add(self, o: OrderedFloat<T>) -> OrderedFloat<T> {
        OrderedFloat(self.0 + o.0)
    }
}
impl<T: std::ops::Add<Output = T>> std::ops::Add<T> for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn add(self, o: T) -> OrderedFloat<T> {
        OrderedFloat(self.0 + o)
    }
}
impl<T: std::ops::Sub<Output = T>> std::ops::Sub for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn sub(self, o: OrderedFloat<T>) -> OrderedFloat<T> {
        OrderedFloat(self.0 - o.0)
    }
}
impl<T: std::ops::Sub<Output = T>> std::ops::Sub<T> for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn sub(self, o: T) -> OrderedFloat<T> {
        OrderedFloat(self.0 - o)
    }
}
impl<T: std::ops::Mul<Output = T>> std::ops::Mul for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn mul(self, o: OrderedFloat<T>) -> OrderedFloat<T> {
        OrderedFloat(self.0 * o.0)
    }
}
impl<T: std::ops::Mul<Output = T>> std::ops::Mul<T> for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn mul(self, o: T) -> OrderedFloat<T> {
        OrderedFloat(self.0 * o)
    }
}
impl<T: std::ops::Div<Output = T>> std::ops::Div for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn div(self, o: OrderedFloat<T>) -> OrderedFloat<T> {
        OrderedFloat(self.0 / o.0)
    }
}
impl<T: std::ops::Div<Output = T>> std::ops::Div<T> for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn div(self, o: T) -> OrderedFloat<T> {
        OrderedFloat(self.0 / o)
    }
}
impl<T: std::ops::Rem<Output = T>> std::ops::Rem for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn rem(self, o: OrderedFloat<T>) -> OrderedFloat<T> {
        OrderedFloat(self.0 % o.0)
    }
}
impl<T: std::ops::Neg<Output = T>> std::ops::Neg for OrderedFloat<T> {
    type Output = OrderedFloat<T>;
    fn neg(self) -> OrderedFloat<T> {
        OrderedFloat(-self.0)
    }
}
impl<T: std::ops::AddAssign> std::ops::AddAssign for OrderedFloat<T> {
    fn add_assign(&mut self, o: OrderedFloat<T>) {
        self.0 += o.0;
    }
}
impl<T: std::ops::AddAssign> std::ops::AddAssign<T> for OrderedFloat<T> {
    fn add_assign(&mut self, o: T) {
        self.0 += o;
    }
}
impl<T: std::ops::SubAssign> std::ops::SubAssign for OrderedFloat<T> {
    fn sub_assign(&mut self, o: OrderedFloat<T>) {
        self.0 -= o.0;
    }
}
impl<T: std::ops::SubAssign> std::ops::SubAssign<T> for OrderedFloat<T> {
    fn sub_assign(&mut self, o: T) {
        self.0 -= o;
    }
}
impl<T: std::ops::MulAssign> std::ops::MulAssign<T> for OrderedFloat<T> {
    fn mul_assign(&mut self, o: T) {
        self.0 *= o;
    }
}
impl<T: std::ops::DivAssign> std::ops::DivAssign<T> for OrderedFloat<T> {
    fn div_assign(&mut self, o: T) {
        self.0 /= o;
    }
}

/// `ordered_float::FloatIsNan`: the error of `NotNan::new(f64::NAN)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FloatIsNan;
impl std::fmt::Display for FloatIsNan {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "NotNan constructed with NaN")
    }
}
impl std::error::Error for FloatIsNan {}

/// `ordered_float::NotNan`: a float that's never NaN (`new` checks), with a total order.
#[derive(Debug, Clone, Copy)]
pub struct NotNan<T>(T);

impl<T: Default> Default for NotNan<T> {
    fn default() -> Self {
        NotNan(T::default())
    }
}

impl<T: OrderedFloatCore> NotNan<T> {
    pub fn new(x: T) -> Result<NotNan<T>, FloatIsNan> {
        if x.__of_is_nan() {
            Err(FloatIsNan)
        } else {
            Ok(NotNan(x))
        }
    }
    pub fn into_inner(self) -> T {
        self.0
    }
}
impl<T: OrderedFloatCore + PartialEq + PartialOrd> PartialEq for NotNan<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T: OrderedFloatCore> Eq for NotNan<T> {}
impl<T: OrderedFloatCore> PartialOrd for NotNan<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T: OrderedFloatCore + PartialEq + PartialOrd> Ord for NotNan<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.0 < other.0 {
            std::cmp::Ordering::Less
        } else if self.0 > other.0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }
}
impl<T: OrderedFloatCore> std::hash::Hash for NotNan<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.__of_bits().hash(state);
    }
}
impl<T: std::fmt::Display> std::fmt::Display for NotNan<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl<T> std::ops::Deref for NotNan<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: OrderedFloatCore + std::ops::Add<Output = T>> std::ops::Add for NotNan<T> {
    type Output = NotNan<T>;
    fn add(self, o: NotNan<T>) -> NotNan<T> {
        NotNan::new(self.0 + o.0).expect("Addition resulted in NaN")
    }
}
impl<T: OrderedFloatCore + std::ops::Sub<Output = T>> std::ops::Sub for NotNan<T> {
    type Output = NotNan<T>;
    fn sub(self, o: NotNan<T>) -> NotNan<T> {
        NotNan::new(self.0 - o.0).expect("Subtraction resulted in NaN")
    }
}
impl<T: OrderedFloatCore + std::ops::Mul<Output = T>> std::ops::Mul for NotNan<T> {
    type Output = NotNan<T>;
    fn mul(self, o: NotNan<T>) -> NotNan<T> {
        NotNan::new(self.0 * o.0).expect("Multiplication resulted in NaN")
    }
}
impl<T: OrderedFloatCore + std::ops::Div<Output = T>> std::ops::Div for NotNan<T> {
    type Output = NotNan<T>;
    fn div(self, o: NotNan<T>) -> NotNan<T> {
        NotNan::new(self.0 / o.0).expect("Division resulted in NaN")
    }
}
impl<T: OrderedFloatCore + std::ops::Neg<Output = T>> std::ops::Neg for NotNan<T> {
    type Output = NotNan<T>;
    fn neg(self) -> NotNan<T> {
        NotNan(-self.0)
    }
}
