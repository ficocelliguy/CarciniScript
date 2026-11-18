#[derive(Debug)]
pub struct Timestamp {
    pub secs: u64,
}

impl Timestamp {
    pub fn now(_ctx: &context::NoContext) -> Timestamp {
        Timestamp { secs: 42 }
    }
}

pub mod context {
    pub struct NoContext;

    pub fn describe() -> String {
        "no context".to_string()
    }
}
