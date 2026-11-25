// Look a server up by name in a table; the last lookup's name is a typo.

#[derive(Debug)]
struct Server {
    name: &'static str,
    ram: u32,
}

fn find<'a>(servers: &'a [Server], name: &str) -> Option<&'a Server> {
    servers.iter().find(|s| s.name == name)
}

fn main() {
    let servers = [
        Server { name: "home", ram: 64 },
        Server { name: "n00dles", ram: 4 },
        Server { name: "foodnstuff", ram: 16 },
    ];
    let total: u32 = servers.iter().map(|s| s.ram).sum();
    println!("total ram: {total}");
    for name in ["home", "foodnstuff", "noodles"] {
        let s = find(&servers, name).expect(&format!("no server named {name}"));
        println!("{:<12}{:>4}", s.name, s.ram);
    }
}
