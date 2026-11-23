// Writes through a `&mut` field of a binding that isn't `mut`: a by-value `self`, and a binding
// from an enum pattern (named like std's `Entry`).
struct Table {
    items: Vec<u8>,
}

struct Slot<'a> {
    table: &'a mut Table,
}

impl<'a> Slot<'a> {
    fn insert(self, v: u8) -> &'a mut u8 {
        self.table.items.push(v);
        let n = self.table.items.len();
        &mut self.table.items[n - 1]
    }
}

struct Vals {
    values: Vec<i32>,
}

struct Occ<'a> {
    v: &'a mut Vals,
}

enum Entry<'a> {
    Occupied(Occ<'a>),
    Vacant,
}

fn get<'a>(e: Entry<'a>, i: usize) -> Option<&'a mut i32> {
    match e {
        Entry::Occupied(entry) => Some(&mut entry.v.values[i]),
        Entry::Vacant => None,
    }
}

fn main() {
    let mut t = Table { items: vec![] };
    {
        let r = Slot { table: &mut t }.insert(4);
        *r += 1;
    }
    Slot { table: &mut t }.insert(7);
    let mut vs = Vals { values: vec![1, 2] };
    if let Some(x) = get(Entry::Occupied(Occ { v: &mut vs }), 1) {
        *x = 9;
    }
    println!("{:?} {:?} {:?}", t.items, vs.values, get(Entry::Vacant, 0));
}
