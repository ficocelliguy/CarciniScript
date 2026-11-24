pub struct IoRead { pub n: i32 }
impl IoRead { pub fn new(n: i32) -> Self { IoRead { n } } }
#[derive(Debug)]
pub enum Value { Num(i32) }
