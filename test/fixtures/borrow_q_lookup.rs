// A map looked up by a `&Q` under a `Borrow<Q>` bound on its key type, both a parameter
// (`K: Borrow<Q>`) and a concrete type (`String: Borrow<Q>`, through an alias).
use std::borrow::Borrow;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

type Declaration = String;

struct Container {
    definitions: BTreeMap<Declaration, u32>,
}

impl Container {
    fn get_definition<Q>(&self, declaration: &Q) -> Option<&u32>
    where
        Declaration: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.definitions.get(declaration)
    }

    fn has<Q: Ord + ?Sized>(&self, q: &Q) -> bool
    where
        String: Borrow<Q>,
    {
        self.definitions.contains_key(q)
    }
}

struct Gen<K> {
    m: HashMap<K, u32>,
}

impl<K: Hash + Eq> Gen<K> {
    fn get<Q: Hash + Eq + ?Sized>(&self, q: &Q) -> u32
    where
        K: Borrow<Q>,
    {
        self.m.get(q).copied().unwrap_or(0)
    }
}

fn count<Q>(m: &HashMap<String, u32>, q: &Q) -> u32
where
    String: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
{
    m.get(q).copied().unwrap_or(0)
}

fn main() {
    let mut definitions = BTreeMap::new();
    definitions.insert("a".to_string(), 1);
    definitions.insert("b".to_string(), 2);
    let c = Container { definitions };
    println!("{:?} {:?}", c.get_definition("a"), c.get_definition("z"));
    println!("{} {}", c.has("b"), c.has("q"));
    let mut m = HashMap::new();
    m.insert("x".to_string(), 5);
    let g = Gen { m: m.clone() };
    println!("{} {}", g.get("x"), g.get("y"));
    println!("{} {}", count(&m, "x"), count(&m, "nope"));
}
