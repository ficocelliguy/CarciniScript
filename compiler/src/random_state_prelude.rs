// (random state prelude)
// `std::collections::hash_map::RandomState`, compiled in when a program names it: a
// `BuildHasher` whose hashers are `DefaultHasher`s (`build_hasher`, `hash_one`), and the default
// of a `struct Map<K, S = RandomState>`. A `HashMap` still ignores its hasher.

/// `std::collections::hash_map::RandomState`. (Every one hashes alike: std's differ per value.)
#[derive(Clone, Default)]
pub struct RandomState;

impl RandomState {
    pub fn new() -> RandomState {
        RandomState
    }
}

impl std::hash::BuildHasher for RandomState {
    type Hasher = std::collections::hash_map::DefaultHasher;
    fn build_hasher(&self) -> std::collections::hash_map::DefaultHasher {
        DefaultHasher::new()
    }
}

impl std::fmt::Debug for RandomState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("RandomState { .. }")
    }
}
