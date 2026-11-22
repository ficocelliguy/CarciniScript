// Two traits (and an inherent impl) with a method of the same name on one type: picked by
// `<T as Trait>::m`, `Trait::m`, a type parameter's bound, `dyn`, and a default method's `self`.
trait UsernameWidget {
    fn get(&self) -> String;
}

trait AgeWidget {
    fn get(&self) -> u8;
    fn older(&self) -> u8 {
        self.get() + 1
    }
}

struct Form {
    username: String,
    age: u8,
}

impl Form {
    fn get(&self) -> i32 {
        -1
    }
}

impl UsernameWidget for Form {
    fn get(&self) -> String {
        self.username.clone()
    }
}

impl AgeWidget for Form {
    fn get(&self) -> u8 {
        self.age
    }
}

// Only one `get`: still reached as `AgeWidget`'s through a bound.
struct Baby;

impl AgeWidget for Baby {
    fn get(&self) -> u8 {
        0
    }
}

fn age_of<T: AgeWidget>(t: &T) -> u8 {
    t.get()
}

fn name_of(t: &dyn UsernameWidget) -> String {
    t.get()
}

fn main() {
    let form = Form {
        username: "rustacean".to_owned(),
        age: 28,
    };
    let username = <Form as UsernameWidget>::get(&form);
    let age = <Form as AgeWidget>::get(&form);
    println!("{username} {age}");
    println!("{} {}", AgeWidget::get(&form), UsernameWidget::get(&form));
    println!("inherent {}", form.get());
    println!("{} {}", age_of(&form), age_of(&Baby));
    println!("{}", name_of(&form));
    println!("{} {}", form.older(), Baby.older());
    let widgets: Vec<Box<dyn AgeWidget>> = vec![Box::new(Baby), Box::new(form)];
    for w in &widgets {
        println!("{}", w.get());
    }
}
