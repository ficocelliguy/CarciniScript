// Trait functions without `self`, required by the trait and given by each impl, called on the
// concrete type (`Wolf::species()`) and through `Self::` inside the impl's methods.
trait Animal {
    fn species() -> String;
    fn make(name: &str) -> Self;
    fn name(&self) -> String;
    fn describe(&self) -> String {
        format!("{} the animal", self.name())
    }
}

#[derive(Debug)]
struct Wolf {
    name: String,
}

#[derive(Debug)]
struct Owl(String);

impl Animal for Wolf {
    fn species() -> String {
        "canis lupus".to_string()
    }
    fn make(name: &str) -> Self {
        Wolf { name: name.to_uppercase() }
    }
    fn name(&self) -> String {
        format!("{} ({})", self.name, Self::species())
    }
}

impl Animal for Owl {
    fn species() -> String {
        "strigiformes".into()
    }
    fn make(name: &str) -> Owl {
        Owl(name.to_string())
    }
    fn name(&self) -> String {
        self.0.clone()
    }
    fn describe(&self) -> String {
        format!("{} hoots", self.0)
    }
}

fn main() {
    let w = Wolf::make("akela");
    let o = Owl::make("hedwig");
    println!("{} / {}", Wolf::species(), Owl::species());
    println!("{:?} {:?}", w, o);
    println!("{} | {}", w.describe(), o.describe());
}
