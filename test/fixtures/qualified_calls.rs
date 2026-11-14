// Fully qualified calls: `Pilot::fly(&h)` and `<Human as Wizard>::name()` pick between two traits' methods
// of the same name; a trait's default method called as `Greet::hello(x)`, also on a generic `T: Greet`.
trait Pilot {
    fn fly(&self) -> String;
    fn name() -> String;
}

trait Wizard {
    fn fly(&self) -> String;
    fn name() -> String;
}

struct Human {
    id: u32,
}

impl Pilot for Human {
    fn fly(&self) -> String {
        format!("captain {}", self.id)
    }
    fn name() -> String {
        "pilot".to_string()
    }
}

impl Wizard for Human {
    fn fly(&self) -> String {
        format!("up {}", self.id)
    }
    fn name() -> String {
        "wizard".to_string()
    }
}

fn main() {
    let h = Human { id: 7 };
    println!("{} {}", Pilot::fly(&h), Wizard::fly(&h));
    println!("{} {}", <Human as Pilot>::name(), <Human as Wizard>::name());
    println!("{}", <Human as Wizard>::fly(&h));
    main2();
}
trait Greet {
    fn name(&self) -> String;
    fn hello(&self) -> String {
        format!("hello {}", self.name())
    }
}

trait Shout {
    fn hello(&self) -> String;
}

struct Cat;

impl Greet for Cat {
    fn name(&self) -> String {
        "cat".into()
    }
}

impl Shout for Cat {
    fn hello(&self) -> String {
        "HELLO".into()
    }
}

fn greet_all<T: Greet>(xs: &[T]) -> Vec<String> {
    xs.iter().map(|x| Greet::hello(x)).collect()
}

fn main2() {
    let c = Cat;
    println!("{} {}", Greet::hello(&c), Shout::hello(&c));
    println!("{} {:?}", <Cat as Greet>::name(&c), greet_all(&[Cat, Cat]));
}
