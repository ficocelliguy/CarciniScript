// (map entry prelude)
// `std::collections::hash_map::Entry` (and `btree_map::Entry`) for a `map.entry(k)` kept as a
// value: matched as `Entry::Occupied(e)` / `Entry::Vacant(e)`, or used later. Each entry holds the
// map itself (the same JS object), so what it does reaches the map. A chained
// `map.entry(k).or_insert(v)` doesn't make one. The methods that hand out `&mut V` (`get_mut`,
// `into_mut`, `insert`, `or_insert`) are compiled as the map's slot for the key, a place
// (`*e.get_mut() += 1`).

pub enum StdMapEntry<K, V> {
    Occupied(StdOccupiedEntry<K, V>),
    Vacant(StdVacantEntry<K, V>),
}

pub struct StdOccupiedEntry<K, V> {
    map: std::collections::HashMap<K, V>,
    key: K,
}

pub struct StdVacantEntry<K, V> {
    map: std::collections::HashMap<K, V>,
    key: K,
}

impl<K: std::hash::Hash + Eq + Clone, V> StdOccupiedEntry<K, V> {
    fn __m(&self) -> &std::collections::HashMap<K, V> {
        &self.map
    }
    fn __k(&self) -> &K {
        &self.key
    }
    pub fn key(&self) -> &K {
        &self.key
    }
    pub fn get(&self) -> &V {
        self.map.get(&self.key).unwrap()
    }
    pub fn get_mut(&mut self) -> &mut V {
        self.map.get_mut(&self.key).unwrap()
    }
    pub fn into_mut(mut self) -> &mut V {
        self.map.get_mut(&self.key).unwrap()
    }
    pub fn insert(&mut self, value: V) -> V {
        self.map.insert(self.key.clone(), value).unwrap()
    }
    pub fn remove(mut self) -> V {
        self.map.remove(&self.key).unwrap()
    }
    pub fn remove_entry(mut self) -> (K, V) {
        let v = self.map.remove(&self.key).unwrap();
        (self.key, v)
    }
}

impl<K: std::hash::Hash + Eq + Clone, V> StdVacantEntry<K, V> {
    fn __m(&self) -> &std::collections::HashMap<K, V> {
        &self.map
    }
    fn __k(&self) -> &K {
        &self.key
    }
    pub fn key(&self) -> &K {
        &self.key
    }
    pub fn into_key(self) -> K {
        self.key
    }
    pub fn insert(mut self, value: V) -> &mut V {
        self.map.insert(self.key.clone(), value);
        self.map.get_mut(&self.key).unwrap()
    }
}

impl<K: std::hash::Hash + Eq + Clone, V> StdMapEntry<K, V> {
    fn __m(&self) -> &std::collections::HashMap<K, V> {
        match self {
            StdMapEntry::Occupied(e) => &e.map,
            StdMapEntry::Vacant(e) => &e.map,
        }
    }
    fn __k(&self) -> &K {
        match self {
            StdMapEntry::Occupied(e) => &e.key,
            StdMapEntry::Vacant(e) => &e.key,
        }
    }
    pub fn key(&self) -> &K {
        match self {
            StdMapEntry::Occupied(e) => &e.key,
            StdMapEntry::Vacant(e) => &e.key,
        }
    }
    pub fn or_insert(self, default: V) -> &mut V {
        match self {
            StdMapEntry::Occupied(e) => e.into_mut(),
            StdMapEntry::Vacant(e) => e.insert(default),
        }
    }
    pub fn or_insert_with<F: FnOnce() -> V>(self, f: F) -> &mut V {
        match self {
            StdMapEntry::Occupied(e) => e.into_mut(),
            StdMapEntry::Vacant(e) => e.insert(f()),
        }
    }
}

fn __std_map_entry<K: std::hash::Hash + Eq + Clone, V>(map: std::collections::HashMap<K, V>, key: K) -> StdMapEntry<K, V> {
    if map.contains_key(&key) {
        StdMapEntry::Occupied(StdOccupiedEntry { map, key })
    } else {
        StdMapEntry::Vacant(StdVacantEntry { map, key })
    }
}
