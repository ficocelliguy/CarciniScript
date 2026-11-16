// Hand-written `Debug` impls through the Formatter's builders.
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write;

struct Point {
    x: f64,
    y: f64,
    label: Option<String>,
}

impl fmt::Debug for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Point")
            .field("x", &self.x)
            .field("y", &self.y)
            .field("label", &self.label)
            .finish()
    }
}

struct Meters(f64, char);

impl fmt::Debug for Meters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Meters")
            .field(&self.0)
            .field(&self.1)
            .finish()
    }
}

struct Secret {
    id: u32,
    key: String,
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let _ = &self.key;
        f.debug_struct("Secret")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

struct Bag {
    items: Vec<&'static str>,
}

impl fmt::Debug for Bag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list()
            .entries(self.items.iter())
            .entry(&"end")
            .finish()
    }
}

struct Scores {
    m: BTreeMap<String, i32>,
}

impl fmt::Debug for Scores {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_map()
            .entries(self.m.iter())
            .entry(&"total", &self.m.values().sum::<i32>())
            .key(&"k")
            .value(&Some(1.5))
            .finish()
    }
}

struct Tags(Vec<u8>);

impl fmt::Debug for Tags {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_set().entries(&self.0).finish()
    }
}

struct Wrapper<T> {
    inner: T,
}

impl<T: fmt::Debug> fmt::Debug for Wrapper<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Wrapper")
            .field("inner", &self.inner)
            .finish()
    }
}

struct Empty;

impl fmt::Debug for Empty {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_char('<')?;
        f.debug_tuple("Empty").finish()?;
        f.write_char('>')
    }
}

fn main() {
    let p = Point {
        x: 1.0,
        y: -2.5,
        label: Some("home".to_string()),
    };
    println!("{:?}", p);
    println!("{:#?}", p);
    println!("{:?}", Meters(3.0, 'm'));
    println!(
        "{:?}",
        Secret {
            id: 7,
            key: "k".into()
        }
    );
    println!(
        "{:?}",
        Bag {
            items: vec!["a", "b"]
        }
    );
    let mut m = BTreeMap::new();
    m.insert("b".to_string(), 2);
    m.insert("a".to_string(), 1);
    println!("{:?}", Scores { m });
    println!("{:?}", Tags(vec![3, 1]));
    println!("{:?}", Wrapper { inner: Some(2.0) });
    println!("{:?}", Wrapper { inner: vec![p] });
    println!("{:?} {:?}", Empty, vec![Meters(1.0, 'a')]);
}
