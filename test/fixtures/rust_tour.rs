// A tour of idiomatic Rust for JavaScript developers. Every section has a comment saying what the
// feature is, how the syntax reads, and roughly what the JS equivalent would be. Run it and
// compare the output with the code; click a line to see the JS it compiled to.

use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------------------------
// `const` is a compile-time constant. Unlike JS `const`, the type is always written out
// (`u32` = unsigned 32-bit integer). Rust has many number types: i32, u64, f64, usize, ...
// and never converts between them silently.
const MAX_STOCK: u32 = 100;

// ---------------------------------------------------------------------------------------------
// `struct` is like a class with only fields. `#[derive(...)]` asks the compiler to generate
// common behaviour: `Debug` lets you print it with `{:?}`, `Clone` gives a `.clone()` deep copy,
// `PartialEq` gives `==` that compares fields (JS `===` would compare object identity).
#[derive(Debug, Clone, PartialEq)]
struct Item {
    name: String, // `String` is an owned, growable string (JS strings are always this)
    price_cents: u32,
    category: Category,
}

// `enum` defines a type that is exactly one of a fixed set of variants, like a TS union of
// string literals ("Tool" | "Food" | ...), but checked by the compiler everywhere it's used.
// `Copy` means it is copied implicitly on assignment, like a JS number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Category {
    Tool,
    Food,
    Toy,
}

// Enum variants can also carry data, like a TS discriminated union
// ({ kind: "Sold", qty: number } | { kind: "Restocked", qty: number } | ...).
#[derive(Debug)]
enum Event {
    Sold { name: String, qty: u32 },
    Restocked(String, u32), // tuple-style variant: fields by position instead of by name
    Closed,                 // no data at all
}

// ---------------------------------------------------------------------------------------------
// `impl` blocks add methods to a type, separately from its definition.
impl Item {
    // An "associated function" (no `self`) is like a static method; call it as `Item::new(...)`.
    // `&str` is a borrowed string slice: a read-only view into someone else's string.
    // `Self` is shorthand for the type being implemented (here `Item`).
    fn new(name: &str, price_cents: u32, category: Category) -> Self {
        // Field init shorthand works like JS `{ price_cents, category }`.
        // The last expression without a `;` is the return value; no `return` needed.
        Self { name: name.to_string(), price_cents, category }
    }

    // `&self` is like `this`, borrowed read-only: the method can look but not modify.
    fn price_dollars(&self) -> f64 {
        // `as` is an explicit numeric conversion. Rust never mixes u32 and f64 implicitly.
        self.price_cents as f64 / 100.0
    }
}

// ---------------------------------------------------------------------------------------------
// Traits are like TS interfaces, but you can implement them for any type, even after the fact.
// `Display` is the standard trait for user-facing text: implementing it is like defining
// `toString()`, and makes the type usable with `{}` in `println!` and `format!`.
impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // `write!` is `format!` that writes into `f`. `{:.2}` means two decimal places.
        write!(f, "{} (${:.2})", self.name, self.price_dollars())
    }
}

// Our own trait. Any type that implements it promises to have a `describe` method.
trait Describe {
    fn describe(&self) -> String;

    // Traits can provide default method bodies, which implementors get for free.
    fn shout(&self) -> String {
        self.describe().to_uppercase()
    }
}

impl Describe for Category {
    fn describe(&self) -> String {
        // `match` is a `switch` that must cover every case (the compiler checks), and is an
        // expression that produces a value. `=>` separates a pattern from its result.
        match self {
            Category::Tool => "a tool".to_string(),
            Category::Food => "something to eat".to_string(),
            Category::Toy => "a toy".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Rust has no exceptions and no `null`. Failure is a value of type `Result<T, E>`, which is
// either `Ok(value)` or `Err(error)`. "Maybe missing" is `Option<T>`: `Some(value)` or `None`.
// Here's a custom error type, an enum so callers can `match` on exactly what went wrong.
#[derive(Debug, PartialEq)]
enum ShopError {
    UnknownItem(String),
    OutOfStock { name: String, wanted: u32, have: u32 },
}

impl fmt::Display for ShopError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // Patterns pull fields out of a variant, like JS destructuring.
            ShopError::UnknownItem(name) => write!(f, "no such item: {name}"),
            ShopError::OutOfStock { name, wanted, have } => {
                write!(f, "wanted {wanted} {name}, only {have} left")
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A struct that owns other data. `Vec<Item>` is a growable array (JS `Item[]`), and
// `HashMap<String, u32>` is a `Map<string, number>`. The `<...>` are generics, as in TS.
struct Shop {
    items: Vec<Item>,
    stock: HashMap<String, u32>,
    log: Vec<Event>,
}

impl Shop {
    fn new() -> Self {
        Shop { items: Vec::new(), stock: HashMap::new(), log: vec![] } // `vec![]` = `[]`
    }

    // `&mut self` borrows `this` mutably: the method may change the shop. Rust guarantees that
    // while something is borrowed mutably, nothing else can read or write it at the same time.
    fn add(&mut self, item: Item, qty: u32) {
        // `.entry(k).or_insert(0)` gets the value for `k`, inserting 0 first if it's missing.
        // It returns a mutable reference, and `*` "dereferences" it to update the value in place.
        *self.stock.entry(item.name.clone()).or_insert(0) += qty;
        // `item` is *moved* into the Vec here: after this line we can't use `item` anymore,
        // because the Vec owns it now. That's why we cloned the name above first.
        self.items.push(item);
    }

    // Returns `Option<&Item>`: a borrowed reference to an Item, or None. Iterator methods like
    // `.iter().find(...)` look just like JS array methods, but are lazy, like generator chains.
    // `|i| ...` is a closure (arrow function): JS `i => i.name === name`.
    fn find(&self, name: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.name == name)
    }

    // A function that can fail returns `Result`. `u32` on success, `ShopError` on failure.
    fn sell(&mut self, name: &str, qty: u32) -> Result<u32, ShopError> {
        // `.ok_or_else(...)` turns an Option into a Result (None becomes the given Err).
        // The `?` operator at the end means "if this is an Err, return it from `sell` right now;
        // otherwise unwrap the Ok value". It replaces try/catch for propagating errors.
        let item = self.find(name).ok_or_else(|| ShopError::UnknownItem(name.to_string()))?;
        let price = item.price_cents;

        // `if let` runs a block only when a pattern matches, binding the inner value.
        // `get_mut` returns `Option<&mut u32>`, so `have` is a mutable reference into the map.
        if let Some(have) = self.stock.get_mut(name) {
            if *have < qty {
                // An early `return` with an error. `{ name, wanted, have }` uses field shorthand.
                return Err(ShopError::OutOfStock { name: name.to_string(), wanted: qty, have: *have });
            }
            *have -= qty;
        }
        self.log.push(Event::Sold { name: name.to_string(), qty });
        Ok(price * qty)
    }

    fn restock(&mut self, name: &str, qty: u32) {
        // `let ... else` destructures or bails out: if the pattern doesn't match, the `else`
        // block runs and must leave the function (here with `return`).
        let Some(item) = self.find(name) else {
            return;
        };
        let key = item.name.clone();
        // `.min()` on numbers is a method, not `Math.min`. Methods work on primitives too.
        if let Some(have) = self.stock.get_mut(&key) {
            *have = (*have + qty).min(MAX_STOCK);
        }
        self.log.push(Event::Restocked(name.to_string(), qty));
    }
}

// ---------------------------------------------------------------------------------------------
// Generic function with a *trait bound*: `T: Describe` means "any type T, as long as it
// implements Describe". The compiler checks every call. `&[T]` is a slice: a borrowed view of
// any array or Vec of T.
fn describe_all<T: Describe>(things: &[T]) -> Vec<String> {
    things.iter().map(|t| t.describe()).collect()
}

// `impl Trait` in argument position is a shorter way to write the same kind of bound.
// `impl Fn(u32) -> u32` accepts any function or closure taking a u32 and returning a u32.
fn apply_twice(f: impl Fn(u32) -> u32, x: u32) -> u32 {
    f(f(x))
}

// Lifetimes: `'a` says "the returned &str lives as long as the inputs do". This lets the
// compiler prove the reference can't outlive the strings it points into (no dangling pointers).
// You only need to write them when the compiler can't figure it out on its own.
fn longer<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.len() >= b.len() { a } else { b }
}

// ---------------------------------------------------------------------------------------------
// `fn main` is the entry point. The program runs top to bottom from here.
fn main() {
    // `let` declares an immutable binding by default (like JS `const`). Add `mut` to allow
    // reassignment (like JS `let`). Types are usually inferred.
    let mut shop = Shop::new();
    shop.add(Item::new("hammer", 1250, Category::Tool), 3);
    shop.add(Item::new("apple", 99, Category::Food), 10);
    shop.add(Item::new("yo-yo", 450, Category::Toy), 1);
    shop.add(Item::new("wrench", 875, Category::Tool), 2);

    // `println!` is a *macro* (the `!` marks it): it checks the format string at compile time.
    // `{}` uses Display, `{:?}` uses Debug, and `{name}` reads a variable directly, like a
    // JS template literal `${name}`.
    let count = shop.items.len();
    println!("== {count} items, first is {} ==", shop.items[0]);
    println!("{:?}", shop.items[2]);

    // `for x in &collection` loops over references to each element, without taking ownership
    // (the loop just borrows the Vec). Like `for (const x of arr)`.
    for item in &shop.items {
        println!("  {:<8} {:>6.2} {:?}", item.name, item.price_dollars(), item.category);
    }

    // Matching on a Result. Each arm handles one case; forgetting one is a compile error.
    println!("== sales ==");
    for (name, qty) in [("hammer", 2), ("apple", 20), ("laser", 1), ("yo-yo", 1), ("yo-yo", 1)] {
        // Tuples `(a, b)` are fixed-size groups of values; the loop destructures each one.
        match shop.sell(name, qty) {
            Ok(cents) => println!("  sold {qty} {name} for {cents} cents"),
            // A match arm can test a nested pattern and a condition (`if` guard) at once.
            Err(ShopError::OutOfStock { have, .. }) if have == 0 => {
                println!("  {name}: sold out")
            }
            // `e` is bound to the error; `{e}` prints it with our Display impl.
            Err(e) => println!("  error: {e}"),
        }
    }

    // Option has helpful methods too. `.map` transforms the inner value if there is one,
    // `.unwrap_or` supplies a default. Compare JS `shop.find("x")?.price ?? 0`.
    let wrench_price = shop.find("wrench").map(|i| i.price_cents).unwrap_or(0);
    let laser_price = shop.find("laser").map(|i| i.price_cents).unwrap_or(0);
    println!("wrench {wrench_price}, laser {laser_price}");

    shop.restock("hammer", 200);
    shop.restock("laser", 5); // unknown: `let ... else` returns early, nothing happens

    // Iterator chains: lazy, and nothing runs until a "consumer" like `.collect()` or `.sum()`.
    // `.collect()` can build many collection types, so we say which one with a type annotation.
    // `|(_, &q)|` destructures each (key, value) pair, and `_` ignores the key.
    let total_stock: u32 = shop.stock.iter().map(|(_, &q)| q).sum();
    let mut low: Vec<&String> = shop.stock.iter().filter(|(_, q)| **q < 5).map(|(n, _)| n).collect();
    low.sort(); // HashMap order is unpredictable, so sort for stable output
    println!("total stock {total_stock}, low: {:?}", low);

    // Grouping into a HashMap with `entry`, then reading it back sorted.
    let mut by_category: HashMap<Category, Vec<&str>> = HashMap::new();
    for item in &shop.items {
        // `.as_str()` borrows a `&str` view of the String, without copying it.
        by_category.entry(item.category).or_default().push(item.name.as_str());
    }
    let mut cats: Vec<_> = by_category.into_iter().collect();
    // `sort_by_key` takes a closure that picks what to sort by; `{:?}` text works as a key here.
    cats.sort_by_key(|(c, _)| format!("{:?}", c));
    for (cat, names) in &cats {
        // `.join` on a slice of strings works like JS `arr.join(", ")`.
        println!("  {} -> {}", cat.shout(), names.join(", "));
    }

    // Generic functions and closures.
    println!("{:?}", describe_all(&[Category::Toy, Category::Food]));
    let markup = 10;
    // `move` makes the closure take its own copy of captured variables (here `markup`).
    let add_markup = move |c: u32| c + c * markup / 100;
    println!("1000 twice marked up: {}", apply_twice(add_markup, 1000));
    println!("longer: {}", longer("hammer", "yo-yo"));

    // Ownership in one picture: `clone` makes an independent copy; plain assignment of a
    // non-Copy value *moves* it, and the old name becomes unusable (the compiler enforces it).
    let original = shop.items[0].clone();
    let mut copy = original.clone();
    copy.price_cents += 1;
    // `==` compares by value thanks to `#[derive(PartialEq)]`.
    println!("same? {} / {}", original == copy, original == shop.items[0]);
    let moved = original; // `original` can't be used after this line
    println!("moved: {moved}");

    // Shadowing: `let` with the same name makes a new variable, even with a different type.
    // Handy for parse-then-convert steps. `.parse::<u32>()` uses "turbofish" syntax `::<T>`
    // to say which type to parse into; it returns a Result because parsing can fail.
    let qty = "7";
    let qty: u32 = qty.parse().unwrap_or(0);
    let bad = "seven".parse::<u32>();
    println!("parsed {qty}, bad is_err: {}", bad.is_err());

    // Everything is an expression: `if`, `match` and blocks `{ ... }` all produce values,
    // so there's no need for a ternary operator.
    let mood = if total_stock > 10 { "plenty" } else { "running low" };
    let size = match count {
        0 => "empty",
        1..=3 => "small", // inclusive range pattern
        _ => "big",       // `_` is the catch-all, like `default:`
    };
    println!("{mood}, {size} shop");

    // Matching on the event log, including tuple variants and `matches!` (a boolean match).
    // `.iter().rev()` walks it backwards without copying.
    shop.log.push(Event::Closed);
    for event in shop.log.iter().rev() {
        let line = match event {
            Event::Sold { name, qty } => format!("sold {qty} x {name}"),
            Event::Restocked(name, qty) => format!("restocked {name} +{qty}"),
            Event::Closed => String::from("closed for the day"),
        };
        println!("  log: {line}");
    }
    let sales = shop.log.iter().filter(|e| matches!(e, Event::Sold { .. })).count();
    println!("{sales} sales");

    // `while let` loops as long as the pattern matches. `Vec::pop` returns Option, so this
    // drains the stack until it's empty (pop returns None).
    let mut stack = vec![1, 2, 3];
    while let Some(top) = stack.pop() {
        print!("{top} ");
    }
    println!();

    // Loops can produce values: `break value` exits a `loop` and returns the value.
    let mut n = 1u64;
    let first_big_power = loop {
        n *= 3;
        if n > 1000 {
            break n;
        }
    };
    println!("first power of 3 over 1000: {first_big_power}");

    // `.unwrap()` takes the value out of an Option/Result, and *panics* (crashes the program
    // with a message) if there isn't one. Fine in examples and tests; real code prefers `?`
    // or `match`. `assert!`/`assert_eq!` also panic when the condition is false.
    let apple = shop.find("apple").unwrap();
    assert_eq!(apple.category, Category::Food);
    println!("done: {apple}");
}
