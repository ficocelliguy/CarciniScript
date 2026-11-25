struct Job {
    name: String,
    id: u32,
}

fn whole() {
    let s = String::from("a");
    let c = || drop(s);
    c();
    println!("{}", s);
}

fn field() {
    let j = Job { name: String::from("a"), id: 1 };
    let c = || drop(j.name);
    c();
    println!("{}", j.id);
    let k = j;
}

fn main() {
    whole();
    field();
}
