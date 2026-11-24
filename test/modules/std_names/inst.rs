pub struct Instant(pub u64);
pub struct Duration { pub secs: u64 }
impl Duration { pub fn from_secs(secs: u64) -> Self { Duration { secs } } }
