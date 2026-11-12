fn main() {
    let name = "crab";
    let legs = 10;
    let pi = 3.14159265;
    println!("{name} has {legs} legs");
    println!("{0} {1} {0}", "a", "b");
    println!("{n}!", n = name);
    println!("[{:>8}] [{:<8}] [{:^8}] [{:*^9}]", name, name, name, name);
    println!("[{:5}] [{:<5}] [{:05}] [{:+}]", legs, legs, -legs, legs);
    println!("{:.2} {:.0} {:8.3}|", pi, pi, pi);
    println!("{:?} {:?} {:?}", name, 'c', "tab\there \"quoted\"");
    println!("{} {:?} {} {:?}", 1.0, 1.0, 0.1 + 0.2, 1e21);
    println!("{} {}", f64::INFINITY, -f64::INFINITY);
    println!("{} {:?}", 1e-7, 1e-7);
    println!("{{literal}} braces");
    let s = format!("{}-{}", legs, name);
    println!("{} has len {}", s, s.len());
    println!("{}", "héllo".len());
    println!("{:.3}", "truncate me");
    println!("{}", name.to_uppercase());
    println!("{}", "  padded  ".trim());
    println!("{}", true);
}
