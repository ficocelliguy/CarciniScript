use crate::clamp;

pub fn run() {
    println!("{}", clamp!(42, 0, 9));
    println!("{} {} {}", hp!(5), twice!(21), hp_value!(2));
    info!("ready {}", 3);
}
