struct Srv { name: String, ram: f64 }
struct Net { v: Vec<Srv> }
impl Net {
    fn find_mut(&mut self, n: &str) -> Option<&mut Srv> { self.v.iter_mut().find(|s| s.name == n) }
    fn get_mut(&mut self, i: usize) -> &mut Srv { &mut self.v[i] }
}
fn find<'a>(v: &'a mut Vec<Srv>, n: &str) -> Option<&'a mut Srv> { v.iter_mut().find(|s| s.name == n) }
fn main() {
    let mut net = Net { v: vec![Srv { name: "a".into(), ram: 8.0 }] };
    if let Some(s) = net.find_mut("a") { s.ram -= 4.0; }
    let g = net.get_mut(0);
    g.ram -= 1.0;
    match net.find_mut("a") { Some(s) => s.ram += 1.0, None => {} }
    let Some(s2) = net.find_mut("a") else { return };
    s2.ram += 2.0;
    if let Some(s) = find(&mut net.v, "a") { s.ram *= 2.0; }
    let h = find(&mut net.v, "a").unwrap();
    h.ram += 0.5;
    println!("{}", net.v[0].ram);
}
