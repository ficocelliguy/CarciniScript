// Restricted visibility in inline modules: `pub(super)`, `pub(in super)`, `pub(crate)` and
// `pub(self)`, each used from where it's visible.
mod outer {
    pub mod inner {
        pub(super) fn helper() -> i32 {
            1
        }
        pub(in super) fn deep() -> i32 {
            2
        }
        pub(crate) fn wide() -> i32 {
            3
        }
        pub(self) fn mine() -> i32 {
            4
        }
        pub fn total() -> i32 {
            mine() + helper()
        }
    }

    pub fn sum() -> i32 {
        inner::helper() + inner::deep()
    }
}

fn main() {
    println!("{} {} {}", outer::sum(), outer::inner::wide(), outer::inner::total());
}
