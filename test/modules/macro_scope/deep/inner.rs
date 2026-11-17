pub fn go() {
    let x = 15;
    println!("{} {} {}", crate::clamp!(x, 0, 10), clamp!(-3, 0, 10), hp!(x));
}
