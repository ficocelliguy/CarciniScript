// Enum discriminants that are constant expressions: `const`s, arithmetic, shifts, other enums'
// variants cast with `as`, `u8::MAX`, chars and bytes, with implicit increments after them.
const BASE: isize = 4;
const STEP: isize = BASE * 2 - 1;

#[derive(Debug, Clone, Copy)]
enum Other {
    X = 10,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Code {
    A = BASE + 1,
    B,
    C = 1 << 3,
    D = Other::Y as isize,
    E,
    F = STEP * -2,
    G,
    H = (u8::MAX as isize) + 1,
    I = b'a' as isize,
    J = 'Z' as isize,
    K = !0 & 0xF0,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum Small {
    Lo = 0x10 | 1,
    Mid = 200u8 as i8 as u8,
    Hi = (Small::Lo as u8) + 100,
}

fn main() {
    let all = [
        Code::A,
        Code::B,
        Code::C,
        Code::D,
        Code::E,
        Code::F,
        Code::G,
        Code::H,
        Code::I,
        Code::J,
        Code::K,
    ];
    for c in all {
        println!("{:?} = {}", c, c as isize);
    }
    println!("{} {}", Other::X as i32, Other::Y as i32);
    println!("{} {} {}", Small::Lo as u8, Small::Mid as u8, Small::Hi as u8);
    println!("{:?}", Code::G == Code::G);
}
