// A field and a method with the same name (the getter pattern): the field gets another JS
// property so it doesn't hide the method. Literals, update syntax, destructuring, derives.
#[derive(Debug, Clone, PartialEq, Default)]
struct Server {
    name: String,
    ram: u32,
}

impl Server {
    fn name(&self) -> &str {
        &self.name
    }
    fn ram(&self) -> u32 {
        self.ram
    }
    fn set_ram(&mut self, r: u32) {
        self.ram = r;
    }
}

fn main() {
    let mut s = Server { name: "n00dles".to_string(), ram: 4 };
    s.set_ram(s.ram() * 2);
    let Server { name, ram } = s.clone();
    let t = Server { ram: 1, ..s.clone() };
    println!("{} {} {} {} {:?} {}", s.name(), s.ram, name, ram, t, s == s.clone());
    println!("{:?}", Server::default());
}
