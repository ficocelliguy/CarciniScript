// (linked list prelude)
// `std::collections::LinkedList`, compiled in when a program names it: a `VecDeque` underneath
// (JS has no cheaper list), with the list's names.

/// `std::collections::LinkedList<T>`: a deque with a list's API.
#[derive(Clone, PartialEq, Eq)]
pub struct LinkedList<T> {
    items: std::collections::VecDeque<T>,
}

impl<T> LinkedList<T> {
    pub fn new() -> LinkedList<T> {
        LinkedList {
            items: std::collections::VecDeque::new(),
        }
    }

    pub fn push_back(&mut self, v: T) {
        self.items.push_back(v);
    }

    pub fn push_front(&mut self, v: T) {
        self.items.push_front(v);
    }

    pub fn pop_back(&mut self) -> Option<T> {
        self.items.pop_back()
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.items.pop_front()
    }

    pub fn front(&self) -> Option<&T> {
        self.items.front()
    }

    pub fn back(&self) -> Option<&T> {
        self.items.back()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn contains(&self, x: &T) -> bool
    where
        T: PartialEq,
    {
        self.items.contains(x)
    }

    /// Moves `other`'s elements to the end, leaving it empty.
    pub fn append(&mut self, other: &mut LinkedList<T>) {
        self.items.append(&mut other.items);
    }

    /// The elements from `at` on, as a new list.
    pub fn split_off(&mut self, at: usize) -> LinkedList<T> {
        LinkedList {
            items: self.items.split_off(at),
        }
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, T> {
        self.items.iter()
    }
}

impl<T> Default for LinkedList<T> {
    fn default() -> LinkedList<T> {
        LinkedList::new()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for LinkedList<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_list().entries(self.items.iter()).finish()
    }
}

impl<T> FromIterator<T> for LinkedList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> LinkedList<T> {
        LinkedList {
            items: it.into_iter().collect(),
        }
    }
}

impl<T> Extend<T> for LinkedList<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, it: I) {
        self.items.extend(it);
    }
}

impl<T> IntoIterator for LinkedList<T> {
    type Item = T;
    type IntoIter = std::collections::vec_deque::IntoIter<T>;
    fn into_iter(self) -> std::collections::vec_deque::IntoIter<T> {
        self.items.into_iter()
    }
}
