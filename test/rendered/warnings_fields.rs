#[derive(Debug)]
struct One(i32);
#[derive(Debug)]
struct Two(i32, String);
struct Mid(i32, u8, u8);
#[derive(Debug)]
enum E { A(i32), B(u8, u8), C { x: i32 } }
fn main() {
    println!("{:?}", One(1));
    let t = Two(1, String::new());
    println!("{:?} {}", t, t.1);
    let m = Mid(1, 2, 3);
    let _ = m.0;
    println!("{:?} {:?} {:?}", E::A(1), E::B(1, 2), E::C { x: 1 });
}
