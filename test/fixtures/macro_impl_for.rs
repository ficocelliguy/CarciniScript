trait Total {
    fn total(&self) -> i64;
}
macro_rules! impl_total {
    ($($t:ty),*) => {
        $(
            impl Total for Vec<$t> {
                fn total(&self) -> i64 {
                    let mut sum = 0i64;
                    for x in self.iter() {
                        sum += *x as i64;
                    }
                    sum
                }
            }
        )*
    };
}
impl_total!(u8);
fn main() {
    let x = 5;
    let v: Vec<u8> = vec![1, 2, 3];
    println!("{} {}", v.total(), x);
}
