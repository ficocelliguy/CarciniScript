// A helper grew a parameter; one call site wasn't updated.

fn threads_for(ram: f64, script_ram: f64, reserve: f64) -> u32 {
    ((ram - reserve) / script_ram).floor().max(0.0) as u32
}

fn main() {
    let hosts = [("home", 64.0), ("n00dles", 4.0), ("foodnstuff", 16.0)];
    for (host, ram) in hosts {
        let t = if host == "home" { threads_for(ram, 1.75, 8.0) } else { threads_for(ram, 1.75) };
        println!("{host}: {t}");
    }
}
