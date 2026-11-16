pub fn count() -> usize {
    include_str!("words.txt").split_whitespace().count()
}
