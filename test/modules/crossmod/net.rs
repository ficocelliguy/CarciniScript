use std::fmt;
use std::ops::{Deref, DerefMut};

/// A hostname newtype that reads as a `str`.
#[derive(Debug, Clone, PartialEq)]
pub struct Host(pub String);

impl Deref for Host {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.0)
    }
}

/// A stack that derefs to its `Vec`, mutably too.
pub struct Stack(pub Vec<i32>);

impl Deref for Stack {
    type Target = Vec<i32>;
    fn deref(&self) -> &Vec<i32> {
        &self.0
    }
}

impl DerefMut for Stack {
    fn deref_mut(&mut self) -> &mut Vec<i32> {
        &mut self.0
    }
}

/// Counts down from a number.
pub struct Countdown(pub u32);

impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.0 == 0 {
            return None;
        }
        self.0 -= 1;
        Some(self.0 + 1)
    }
}

pub type Op = fn(i64, i64) -> i64;
pub type Hosts = Vec<Host>;

pub fn add(a: i64, b: i64) -> i64 {
    a + b
}
