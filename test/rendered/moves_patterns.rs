struct Person {
    name: String,
    age: u32,
}

enum Cmd {
    Say(String),
    Wait(u32),
}

fn if_let() {
    let o = Some(String::from("a"));
    if let Some(x) = o {
        println!("{}", x);
    }
    println!("{:?}", o);
}

fn match_arm() {
    let r: Result<String, i32> = Ok(String::from("a"));
    match r {
        Ok(s) => println!("{}", s),
        Err(_) => {}
    }
    println!("{:?}", r);
}

fn struct_field() {
    let p = Person { name: String::from("a"), age: 1 };
    let Person { name, age } = p;
    println!("{} {}", name, age);
    println!("{}", p.name);
}

fn variant_field() {
    let c = Cmd::Say(String::from("hi"));
    match c {
        Cmd::Say(s) => println!("{}", s),
        Cmd::Wait(n) => println!("{}", n),
    }
    let d = c;
}

fn main() {
    if_let();
    match_arm();
    struct_field();
    variant_field();
}
