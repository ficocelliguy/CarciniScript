pub trait Format {
    fn format(&self) -> String;
    fn twice(&self) -> String {
        format!("{}{}", self.format(), self.format())
    }
}

pub trait Width {
    fn width(&self) -> usize;
}

pub fn describe<T: Format>(x: &T) -> String {
    format!("described {}", x.twice())
}

pub fn total_width<T: Width>(xs: &[T]) -> usize {
    xs.iter().map(|x| x.width()).sum()
}
