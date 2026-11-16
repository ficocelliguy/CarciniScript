// (rand_distr prelude)
// The bundled `rand::distributions` and `rand_distr`: `Distribution` (for your own impls, and
// `impl Distribution<T> for Standard`), `Uniform`, `WeightedIndex`, `Bernoulli`, `Alphanumeric`,
// `Normal`, `LogNormal`, `Exp`, `Poisson`, `Binomial`, `Gamma`, `Beta`, `ChiSquared`,
// `StudentT`, `Triangular`, `Pareto`, `Cauchy`, `Weibull`, `Geometric`. `Standard`,
// `StandardNormal`, `Exp1`, `Open01` and `OpenClosed01` sample through the runtime (their
// output type is the caller's), as do `rng.sample(d)` and `rng.sample_iter(d)`.

/// `rand::distributions::Distribution<T>`.
pub trait RandDistribution<T> {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> T;
}

/// `rand::distributions::Standard` (`rand::distr::StandardUniform`).
#[derive(Clone, Copy, Debug)]
pub struct RandStandard;
/// `rand_distr::StandardNormal`.
#[derive(Clone, Copy, Debug)]
pub struct RandStandardNormal;
/// `rand_distr::Exp1`.
#[derive(Clone, Copy, Debug)]
pub struct RandExp1;
/// `rand::distributions::Open01`.
#[derive(Clone, Copy, Debug)]
pub struct RandOpen01;
/// `rand::distributions::OpenClosed01`.
#[derive(Clone, Copy, Debug)]
pub struct RandOpenClosed01;

/// The float types the distributions are generic over.
pub trait RandFloat: Copy + PartialOrd + std::fmt::Debug {
    fn __f(self) -> f64;
    fn __of(x: f64) -> Self;
}

impl RandFloat for f64 {
    fn __f(self) -> f64 {
        self
    }
    fn __of(x: f64) -> f64 {
        x
    }
}

impl RandFloat for f32 {
    fn __f(self) -> f64 {
        self as f64
    }
    fn __of(x: f64) -> f32 {
        x as f32
    }
}

/// A number in (0, 1].
fn __rand_open_closed<R: Rng + ?Sized>(rng: &mut R) -> f64 {
    1.0 - rng.r#gen::<f64>()
}

fn __rand_std_normal<R: Rng + ?Sized>(rng: &mut R) -> f64 {
    let u = __rand_open_closed(rng);
    let v: f64 = rng.r#gen();
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
}

fn __rand_gamma<R: Rng + ?Sized>(shape: f64, scale: f64, rng: &mut R) -> f64 {
    // Marsaglia and Tsang; a shape below 1 is boosted by `u^(1/shape)`.
    if shape < 1.0 {
        let u = __rand_open_closed(rng);
        return __rand_gamma(shape + 1.0, scale, rng) * u.powf(1.0 / shape);
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let x = __rand_std_normal(rng);
        let v = 1.0 + c * x;
        if v <= 0.0 {
            continue;
        }
        let v = v * v * v;
        let u = __rand_open_closed(rng);
        if u.ln() < 0.5 * x * x + d - d * v + d * v.ln() {
            return d * v * scale;
        }
    }
}

/// `ln(n!)` for the rejection samplers (Stirling's series past 10).
fn __rand_ln_fact(k: f64) -> f64 {
    if k < 10.0 {
        let mut r = 1.0;
        let mut i = 2.0;
        while i <= k {
            r *= i;
            i += 1.0;
        }
        return r.ln();
    }
    let k1 = k + 1.0;
    (k1 - 0.5) * k1.ln() - k1 + 0.5 * std::f64::consts::TAU.ln() + 1.0 / (12.0 * k1)
        - 1.0 / (360.0 * k1 * k1 * k1)
}

fn __rand_poisson<R: Rng + ?Sized>(lambda: f64, rng: &mut R) -> f64 {
    if lambda < 12.0 {
        // Knuth: multiply uniforms until the product drops below e^-lambda.
        let limit = (-lambda).exp();
        let mut k = 0.0;
        let mut p: f64 = rng.r#gen();
        while p > limit {
            k += 1.0;
            p *= rng.r#gen::<f64>();
        }
        return k;
    }
    // Hörmann's PTRS.
    let slam = lambda.sqrt();
    let loglam = lambda.ln();
    let b = 0.931 + 2.53 * slam;
    let a = -0.059 + 0.02483 * b;
    let inv_alpha = 1.1239 + 1.1328 / (b - 3.4);
    let vr = 0.9277 - 3.6224 / (b - 2.0);
    loop {
        let u = rng.r#gen::<f64>() - 0.5;
        let v: f64 = rng.r#gen();
        let us = 0.5 - u.abs();
        let k = ((2.0 * a / us + b) * u + lambda + 0.43).floor();
        if us >= 0.07 && v <= vr {
            return k;
        }
        if k < 0.0 || (us < 0.013 && v > us) {
            continue;
        }
        if v.ln() + inv_alpha.ln() - (a / (us * us) + b).ln()
            <= -lambda + k * loglam - __rand_ln_fact(k)
        {
            return k;
        }
    }
}

fn __rand_binomial<R: Rng + ?Sized>(n: u64, p: f64, rng: &mut R) -> u64 {
    if p > 0.5 {
        return n - __rand_binomial(n, 1.0 - p, rng);
    }
    let nf = n as f64;
    if p == 0.0 || n == 0 {
        return 0;
    }
    if nf * p < 10.0 {
        // Inversion: the number of geometric gaps that fit in n trials.
        let q = (1.0 - p).ln();
        let mut x = 0.0;
        let mut sum = 0.0;
        loop {
            let u = __rand_open_closed(rng);
            sum += (u.ln() / q).floor() + 1.0;
            if sum > nf {
                return x as u64;
            }
            x += 1.0;
        }
    }
    // Hörmann's BTRS.
    let q = 1.0 - p;
    let spq = (nf * p * q).sqrt();
    let b = 1.15 + 2.53 * spq;
    let a = -0.0873 + 0.0248 * b + 0.01 * p;
    let c = nf * p + 0.5;
    let vr = 0.92 - 4.2 / b;
    let alpha = (2.83 + 5.1 / b) * spq;
    let lpq = (p / q).ln();
    let m = ((nf + 1.0) * p).floor();
    let h = __rand_ln_fact(m) + __rand_ln_fact(nf - m);
    loop {
        let u = rng.r#gen::<f64>() - 0.5;
        let v: f64 = rng.r#gen();
        let us = 0.5 - u.abs();
        let k = ((2.0 * a / us + b) * u + c).floor();
        if k < 0.0 || k > nf {
            continue;
        }
        if us >= 0.07 && v <= vr {
            return k as u64;
        }
        let v = (v * alpha / (a / (us * us) + b)).ln();
        if v <= h - __rand_ln_fact(k) - __rand_ln_fact(nf - k) + (k - m) * lpq {
            return k as u64;
        }
    }
}

/// The number types `Uniform` samples.
pub trait RandSampleUniform: Copy + PartialOrd + std::fmt::Debug {
    fn __between<R: Rng + ?Sized>(lo: Self, hi: Self, incl: bool, rng: &mut R) -> Self;
}

impl RandSampleUniform for i8 {
    fn __between<R: Rng + ?Sized>(lo: i8, hi: i8, incl: bool, rng: &mut R) -> i8 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for i16 {
    fn __between<R: Rng + ?Sized>(lo: i16, hi: i16, incl: bool, rng: &mut R) -> i16 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for i32 {
    fn __between<R: Rng + ?Sized>(lo: i32, hi: i32, incl: bool, rng: &mut R) -> i32 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for i64 {
    fn __between<R: Rng + ?Sized>(lo: i64, hi: i64, incl: bool, rng: &mut R) -> i64 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for isize {
    fn __between<R: Rng + ?Sized>(lo: isize, hi: isize, incl: bool, rng: &mut R) -> isize {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for u8 {
    fn __between<R: Rng + ?Sized>(lo: u8, hi: u8, incl: bool, rng: &mut R) -> u8 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for u16 {
    fn __between<R: Rng + ?Sized>(lo: u16, hi: u16, incl: bool, rng: &mut R) -> u16 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for u32 {
    fn __between<R: Rng + ?Sized>(lo: u32, hi: u32, incl: bool, rng: &mut R) -> u32 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for u64 {
    fn __between<R: Rng + ?Sized>(lo: u64, hi: u64, incl: bool, rng: &mut R) -> u64 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for usize {
    fn __between<R: Rng + ?Sized>(lo: usize, hi: usize, incl: bool, rng: &mut R) -> usize {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for f32 {
    fn __between<R: Rng + ?Sized>(lo: f32, hi: f32, incl: bool, rng: &mut R) -> f32 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

impl RandSampleUniform for f64 {
    fn __between<R: Rng + ?Sized>(lo: f64, hi: f64, incl: bool, rng: &mut R) -> f64 {
        if incl { rng.gen_range(lo..=hi) } else { rng.gen_range(lo..hi) }
    }
}

/// `rand::distributions::Uniform<T>` (rand 0.8: `new` panics on an empty range).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandUniform<X> {
    low: X,
    high: X,
    inclusive: bool,
}

impl<X: RandSampleUniform + PartialOrd> RandUniform<X> {
    pub fn new(low: X, high: X) -> RandUniform<X> {
        if !(low < high) {
            panic!("Uniform::new called with `low >= high`");
        }
        RandUniform { low, high, inclusive: false }
    }
    pub fn new_inclusive(low: X, high: X) -> RandUniform<X> {
        if !(low <= high) {
            panic!("Uniform::new_inclusive called with `low > high`");
        }
        RandUniform { low, high, inclusive: true }
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> X {
        X::__between(self.low, self.high, self.inclusive, rng)
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = X> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

/// rand 0.9's `rand::distr::Uniform<T>`: `new` gives a `Result`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandUniform09<X> {
    low: X,
    high: X,
    inclusive: bool,
}

/// rand 0.9's `rand::distr::uniform::Error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandUniformError {
    EmptyRange,
    NonFinite,
}

impl std::fmt::Display for RandUniformError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandUniformError::EmptyRange => "low > high (or equal if exclusive) in uniform distribution",
            RandUniformError::NonFinite => "Non-finite range in uniform distribution",
        })
    }
}

impl std::error::Error for RandUniformError {}

impl<X: RandSampleUniform + PartialOrd> RandUniform09<X> {
    pub fn new(low: X, high: X) -> Result<RandUniform09<X>, RandUniformError> {
        if !(low < high) {
            return Err(RandUniformError::EmptyRange);
        }
        Ok(RandUniform09 { low, high, inclusive: false })
    }
    pub fn new_inclusive(low: X, high: X) -> Result<RandUniform09<X>, RandUniformError> {
        if !(low <= high) {
            return Err(RandUniformError::EmptyRange);
        }
        Ok(RandUniform09 { low, high, inclusive: true })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> X {
        X::__between(self.low, self.high, self.inclusive, rng)
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = X> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

/// `rand::distributions::WeightedError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandWeightedError {
    NoItem,
    InvalidWeight,
    AllWeightsZero,
    TooMany,
}

impl std::fmt::Display for RandWeightedError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandWeightedError::NoItem => "No weights provided in distribution",
            RandWeightedError::InvalidWeight => "A weight is invalid in distribution",
            RandWeightedError::AllWeightsZero => "All weights are zero in distribution",
            RandWeightedError::TooMany => "Too many weights (hit u32::MAX) in distribution",
        })
    }
}

impl std::error::Error for RandWeightedError {}

/// The weight types of a `WeightedIndex`.
pub trait RandWeight: Copy + PartialOrd + std::fmt::Debug {
    fn __w(self) -> f64;
}

impl RandWeight for i8 {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for i16 {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for i32 {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for i64 {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for isize {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for u8 {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for u16 {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for u32 {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for u64 {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for usize {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for f32 {
    fn __w(self) -> f64 {
        self as f64
    }
}

impl RandWeight for f64 {
    fn __w(self) -> f64 {
        self as f64
    }
}

/// `rand::distributions::WeightedIndex<X>`: an index picked with the weights' odds.
#[derive(Clone, Debug, PartialEq)]
pub struct RandWeightedIndex<X> {
    cumulative: Vec<f64>,
    total: f64,
    weights: Vec<X>,
}

impl<X: RandWeight + PartialOrd> RandWeightedIndex<X> {
    pub fn new<I: IntoIterator<Item = X>>(weights: I) -> Result<RandWeightedIndex<X>, RandWeightedError> {
        let weights: Vec<X> = weights.into_iter().collect();
        if weights.is_empty() {
            return Err(RandWeightedError::NoItem);
        }
        let mut cumulative = Vec::new();
        let mut total = 0.0;
        for w in &weights {
            let x = w.__w();
            if !(x >= 0.0) || x.is_infinite() {
                return Err(RandWeightedError::InvalidWeight);
            }
            total += x;
            cumulative.push(total);
        }
        if total == 0.0 {
            return Err(RandWeightedError::AllWeightsZero);
        }
        Ok(RandWeightedIndex { cumulative, total, weights })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> usize {
        let x = rng.r#gen::<f64>() * self.total;
        let last = self.cumulative.len() - 1;
        let (mut lo, mut hi) = (0, last);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.cumulative[mid] <= x {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = usize> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

/// `rand::distributions::BernoulliError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandBernoulliError {
    InvalidProbability,
}

impl std::fmt::Display for RandBernoulliError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("p is outside [0, 1] in Bernoulli distribution")
    }
}

impl std::error::Error for RandBernoulliError {}

/// `rand::distributions::Bernoulli`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandBernoulli {
    p: f64,
}

impl RandBernoulli {
    pub fn new(p: f64) -> Result<RandBernoulli, RandBernoulliError> {
        if !(0.0..=1.0).contains(&p) {
            return Err(RandBernoulliError::InvalidProbability);
        }
        Ok(RandBernoulli { p })
    }
    pub fn from_ratio(numerator: u32, denominator: u32) -> Result<RandBernoulli, RandBernoulliError> {
        if numerator > denominator || denominator == 0 {
            return Err(RandBernoulliError::InvalidProbability);
        }
        Ok(RandBernoulli { p: numerator as f64 / denominator as f64 })
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = bool> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

impl RandDistribution<bool> for RandBernoulli {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> bool {
        rng.gen_bool(self.p)
    }
}

/// `rand::distributions::Alphanumeric`: an ASCII letter or digit, as a `u8`.
#[derive(Clone, Copy, Debug)]
pub struct RandAlphanumeric;

impl RandAlphanumeric {
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = u8> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
    pub fn sample_string<R: Rng + ?Sized>(&self, rng: &mut R, len: usize) -> String {
        (0..len).map(|_| self.sample(rng) as char).collect()
    }
}

impl RandDistribution<u8> for RandAlphanumeric {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> u8 {
        let set = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        set[rng.gen_range(0..62)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandNormalError {
    MeanTooSmall,
    BadVariance,
}

impl std::fmt::Display for RandNormalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandNormalError::MeanTooSmall => "mean < 0 or NaN in log-normal distribution",
            RandNormalError::BadVariance => "variation parameter is non-finite in (log)normal distribution",
        })
    }
}

impl std::error::Error for RandNormalError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandExpError {
    LambdaTooSmall,
}

impl std::fmt::Display for RandExpError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandExpError::LambdaTooSmall => "lambda is negative or NaN in exponential distribution",
        })
    }
}

impl std::error::Error for RandExpError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandPoissonError {
    ShapeTooSmall,
}

impl std::fmt::Display for RandPoissonError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandPoissonError::ShapeTooSmall => "lambda is not positive in Poisson distribution",
        })
    }
}

impl std::error::Error for RandPoissonError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandBinomialError {
    ProbabilityTooSmall,
    ProbabilityTooLarge,
}

impl std::fmt::Display for RandBinomialError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandBinomialError::ProbabilityTooSmall => "p < 0 or is NaN in binomial distribution",
            RandBinomialError::ProbabilityTooLarge => "p > 1 in binomial distribution",
        })
    }
}

impl std::error::Error for RandBinomialError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandGammaError {
    ShapeTooSmall,
    ScaleTooSmall,
    ScaleTooLarge,
}

impl std::fmt::Display for RandGammaError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandGammaError::ShapeTooSmall => "shape is not positive in gamma distribution",
            RandGammaError::ScaleTooSmall => "scale is not positive in gamma distribution",
            RandGammaError::ScaleTooLarge => "scale is infinity in gamma distribution",
        })
    }
}

impl std::error::Error for RandGammaError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandBetaError {
    AlphaTooSmall,
    BetaTooSmall,
}

impl std::fmt::Display for RandBetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandBetaError::AlphaTooSmall => "alpha is not positive in beta distribution",
            RandBetaError::BetaTooSmall => "beta is not positive in beta distribution",
        })
    }
}

impl std::error::Error for RandBetaError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandChiSquaredError {
    DoFTooSmall,
}

impl std::fmt::Display for RandChiSquaredError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandChiSquaredError::DoFTooSmall => "degrees-of-freedom k is not positive in chi-squared distribution",
        })
    }
}

impl std::error::Error for RandChiSquaredError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandStudentTError {
    DoFTooSmall,
}

impl std::fmt::Display for RandStudentTError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandStudentTError::DoFTooSmall => "degrees-of-freedom n is not positive in Student t distribution",
        })
    }
}

impl std::error::Error for RandStudentTError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandTriangularError {
    RangeTooSmall,
    ModeRange,
}

impl std::fmt::Display for RandTriangularError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandTriangularError::RangeTooSmall => "requirement min <= max is not met in triangular distribution",
            RandTriangularError::ModeRange => "mode is outside [min, max] in triangular distribution",
        })
    }
}

impl std::error::Error for RandTriangularError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandParetoError {
    ScaleTooSmall,
    ShapeTooSmall,
}

impl std::fmt::Display for RandParetoError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandParetoError::ScaleTooSmall => "scale is not positive in Pareto distribution",
            RandParetoError::ShapeTooSmall => "shape is not positive in Pareto distribution",
        })
    }
}

impl std::error::Error for RandParetoError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandCauchyError {
    ScaleTooSmall,
}

impl std::fmt::Display for RandCauchyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandCauchyError::ScaleTooSmall => "scale is not positive in Cauchy distribution",
        })
    }
}

impl std::error::Error for RandCauchyError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandWeibullError {
    ScaleTooSmall,
    ShapeTooSmall,
}

impl std::fmt::Display for RandWeibullError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandWeibullError::ScaleTooSmall => "scale is not positive in Weibull distribution",
            RandWeibullError::ShapeTooSmall => "shape is not positive in Weibull distribution",
        })
    }
}

impl std::error::Error for RandWeibullError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandGeoError {
    InvalidProbability,
}

impl std::fmt::Display for RandGeoError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            RandGeoError::InvalidProbability => "p is NaN or outside the interval [0, 1] in geometric distribution",
        })
    }
}

impl std::error::Error for RandGeoError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandNormal<F> {
    mean: F,
    std_dev: F,
}

impl<F: RandFloat> RandNormal<F> {
    pub fn new(mean: F, std_dev: F) -> Result<RandNormal<F>, RandNormalError> {
        if !std_dev.__f().is_finite() { return Err(RandNormalError::BadVariance); }
        Ok(RandNormal { mean, std_dev })
    }
    pub fn from_mean_cv(mean: F, cv: F) -> Result<RandNormal<F>, RandNormalError> {
        if !cv.__f().is_finite() || cv.__f() < 0.0 {
            return Err(RandNormalError::BadVariance);
        }
        Ok(RandNormal { mean, std_dev: F::__of(cv.__f() * mean.__f()) })
    }
    pub fn from_zscore(&self, zscore: F) -> F {
        F::__of(self.mean.__f() + self.std_dev.__f() * zscore.__f())
    }
    pub fn mean(&self) -> F {
        self.mean
    }
    pub fn std_dev(&self) -> F {
        self.std_dev
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of(self.mean.__f() + self.std_dev.__f() * __rand_std_normal(rng))
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandLogNormal<F> {
    mu: F,
    sigma: F,
}

impl<F: RandFloat> RandLogNormal<F> {
    pub fn new(mu: F, sigma: F) -> Result<RandLogNormal<F>, RandNormalError> {
        if !sigma.__f().is_finite() { return Err(RandNormalError::BadVariance); }
        Ok(RandLogNormal { mu, sigma })
    }
    pub fn from_mean_cv(mean: F, cv: F) -> Result<RandLogNormal<F>, RandNormalError> {
        let (mean, cv) = (mean.__f(), cv.__f());
        if cv == 0.0 {
            return Ok(RandLogNormal { mu: F::__of(mean.ln()), sigma: F::__of(0.0) });
        }
        if !(mean > 0.0) {
            return Err(RandNormalError::MeanTooSmall);
        }
        if !(cv >= 0.0) {
            return Err(RandNormalError::BadVariance);
        }
        let a = 1.0 + cv * cv;
        let mu = 0.5 * (mean * mean / a).ln();
        let sigma = a.ln().sqrt();
        RandLogNormal::new(F::__of(mu), F::__of(sigma))
    }
    pub fn from_zscore(&self, zscore: F) -> F {
        F::__of((self.mu.__f() + self.sigma.__f() * zscore.__f()).exp())
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of((self.mu.__f() + self.sigma.__f() * __rand_std_normal(rng)).exp())
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandExp<F> {
    lambda: F,
}

impl<F: RandFloat> RandExp<F> {
    pub fn new(lambda: F) -> Result<RandExp<F>, RandExpError> {
        if !(lambda.__f() >= 0.0) { return Err(RandExpError::LambdaTooSmall); }
        Ok(RandExp { lambda })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of(-__rand_open_closed(rng).ln() / self.lambda.__f())
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandPoisson<F> {
    lambda: F,
}

impl<F: RandFloat> RandPoisson<F> {
    pub fn new(lambda: F) -> Result<RandPoisson<F>, RandPoissonError> {
        if !(lambda.__f() > 0.0) { return Err(RandPoissonError::ShapeTooSmall); }
        Ok(RandPoisson { lambda })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of(__rand_poisson(self.lambda.__f(), rng))
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandGamma<F> {
    shape: F,
    scale: F,
}

impl<F: RandFloat> RandGamma<F> {
    pub fn new(shape: F, scale: F) -> Result<RandGamma<F>, RandGammaError> {
        if !(shape.__f() > 0.0) { return Err(RandGammaError::ShapeTooSmall); }
        if !(scale.__f() > 0.0) { return Err(RandGammaError::ScaleTooSmall); }
        if scale.__f().is_infinite() { return Err(RandGammaError::ScaleTooLarge); }
        Ok(RandGamma { shape, scale })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of(__rand_gamma(self.shape.__f(), self.scale.__f(), rng))
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandBeta<F> {
    alpha: F,
    beta: F,
}

impl<F: RandFloat> RandBeta<F> {
    pub fn new(alpha: F, beta: F) -> Result<RandBeta<F>, RandBetaError> {
        if !(alpha.__f() > 0.0) { return Err(RandBetaError::AlphaTooSmall); }
        if !(beta.__f() > 0.0) { return Err(RandBetaError::BetaTooSmall); }
        Ok(RandBeta { alpha, beta })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of({ let x = __rand_gamma(self.alpha.__f(), 1.0, rng); let y = __rand_gamma(self.beta.__f(), 1.0, rng); x / (x + y) })
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandChiSquared<F> {
    k: F,
}

impl<F: RandFloat> RandChiSquared<F> {
    pub fn new(k: F) -> Result<RandChiSquared<F>, RandChiSquaredError> {
        if !(k.__f() > 0.0) { return Err(RandChiSquaredError::DoFTooSmall); }
        Ok(RandChiSquared { k })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of(__rand_gamma(self.k.__f() / 2.0, 2.0, rng))
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandStudentT<F> {
    n: F,
}

impl<F: RandFloat> RandStudentT<F> {
    pub fn new(n: F) -> Result<RandStudentT<F>, RandStudentTError> {
        if !(n.__f() > 0.0) { return Err(RandStudentTError::DoFTooSmall); }
        Ok(RandStudentT { n })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of({ let z = __rand_std_normal(rng); let n = self.n.__f(); z / (__rand_gamma(n / 2.0, 2.0, rng) / n).sqrt() })
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandTriangular<F> {
    min: F,
    max: F,
    mode: F,
}

impl<F: RandFloat> RandTriangular<F> {
    pub fn new(min: F, max: F, mode: F) -> Result<RandTriangular<F>, RandTriangularError> {
        if !(max.__f() >= min.__f()) { return Err(RandTriangularError::RangeTooSmall); }
        if !(mode.__f() >= min.__f() && max.__f() >= mode.__f()) { return Err(RandTriangularError::ModeRange); }
        Ok(RandTriangular { min, max, mode })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of({
            let (min, max, mode) = (self.min.__f(), self.max.__f(), self.mode.__f());
            let f: f64 = rng.r#gen();
            let (diff_mode, range) = (mode - min, max - min);
            let f_range = f * range;
            if f_range < diff_mode {
                min + (f_range * diff_mode).sqrt()
            } else {
                max - ((range - f_range) * (max - mode)).sqrt()
            }
        })
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandPareto<F> {
    scale: F,
    shape: F,
}

impl<F: RandFloat> RandPareto<F> {
    pub fn new(scale: F, shape: F) -> Result<RandPareto<F>, RandParetoError> {
        if !(scale.__f() > 0.0) { return Err(RandParetoError::ScaleTooSmall); }
        if !(shape.__f() > 0.0) { return Err(RandParetoError::ShapeTooSmall); }
        Ok(RandPareto { scale, shape })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of(self.scale.__f() * __rand_open_closed(rng).powf(-1.0 / self.shape.__f()))
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandCauchy<F> {
    median: F,
    scale: F,
}

impl<F: RandFloat> RandCauchy<F> {
    pub fn new(median: F, scale: F) -> Result<RandCauchy<F>, RandCauchyError> {
        if !(scale.__f() > 0.0) { return Err(RandCauchyError::ScaleTooSmall); }
        Ok(RandCauchy { median, scale })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of({ let x: f64 = rng.r#gen(); self.median.__f() + self.scale.__f() * (std::f64::consts::PI * x).tan() })
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandWeibull<F> {
    scale: F,
    shape: F,
}

impl<F: RandFloat> RandWeibull<F> {
    pub fn new(scale: F, shape: F) -> Result<RandWeibull<F>, RandWeibullError> {
        if !(scale.__f() > 0.0) { return Err(RandWeibullError::ScaleTooSmall); }
        if !(shape.__f() > 0.0) { return Err(RandWeibullError::ShapeTooSmall); }
        Ok(RandWeibull { scale, shape })
    }
    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> F {
        F::__of(self.scale.__f() * (-__rand_open_closed(rng).ln()).powf(1.0 / self.shape.__f()))
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = F> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

/// `rand_distr::Binomial`: successes in `n` trials.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandBinomial {
    n: u64,
    p: f64,
}

impl RandBinomial {
    pub fn new(n: u64, p: f64) -> Result<RandBinomial, RandBinomialError> {
        if !(p >= 0.0) {
            return Err(RandBinomialError::ProbabilityTooSmall);
        }
        if !(p <= 1.0) {
            return Err(RandBinomialError::ProbabilityTooLarge);
        }
        Ok(RandBinomial { n, p })
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = u64> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

impl RandDistribution<u64> for RandBinomial {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> u64 {
        __rand_binomial(self.n, self.p, rng)
    }
}

/// `rand_distr::Geometric`: failures before the first success.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandGeometric {
    p: f64,
}

impl RandGeometric {
    pub fn new(p: f64) -> Result<RandGeometric, RandGeoError> {
        if !(0.0..=1.0).contains(&p) {
            return Err(RandGeoError::InvalidProbability);
        }
        Ok(RandGeometric { p })
    }
    pub fn sample_iter<R: Rng>(self, mut rng: R) -> impl Iterator<Item = u64> {
        std::iter::repeat_with(move || self.sample(&mut rng))
    }
}

impl RandDistribution<u64> for RandGeometric {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> u64 {
        if self.p == 1.0 {
            return 0;
        }
        if self.p == 0.0 {
            return u64::MAX;
        }
        let u = __rand_open_closed(rng);
        let k = (u.ln() / (1.0 - self.p).ln()).floor();
        if k >= u64::MAX as f64 { u64::MAX } else { k as u64 }
    }
}
