// Associated consts (and std ones like `u8::MAX`) as patterns and range bounds: compared with
// their values, which also count for exhaustiveness.

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct S(u8);

impl S {
    const Z: S = S(0);
    const ONE: S = S(1);
    const LIMIT: u32 = 100;

    fn describe(self) -> &'static str {
        match self {
            Self::Z => "zero",
            S::ONE => "one",
            _ => "other",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Mode {
    Fast,
    Slow,
    Off,
}

impl Mode {
    const DEFAULT: Mode = Mode::Slow;
}

struct B;

impl B {
    const LO: u8 = 0;
    const MID: u8 = 127;
    const HI: u8 = 128;
    const BIG: i64 = 9_000_000_000;
    const NEG: i64 = -1;
    const A: char = 'a';
    const M: char = 'm';
    const ON: bool = true;
    const NAME: &'static str = "b";
}

trait Limits {
    const CAP: usize = 16;
    const FLOOR: usize;
}

struct L;

impl Limits for L {
    const FLOOR: usize = 2;
}

const SMALL: u32 = 10;

fn classify(n: u32) -> &'static str {
    match n {
        S::LIMIT => "limit",
        0..=SMALL => "small",
        0..=S::LIMIT => "under",
        _ => "over",
    }
}

fn bucket(n: u8) -> &'static str {
    // Exhaustive without `_`: the constants' values cover 0..=255.
    match n {
        B::LO..=B::MID => "low",
        B::HI..=u8::MAX => "high",
    }
}

fn big(n: i64) -> &'static str {
    match n {
        B::BIG => "big",
        B::NEG => "neg",
        i64::MIN..=-2 => "very neg",
        _ => "other",
    }
}

fn letter(c: char) -> &'static str {
    match c {
        B::A..=B::M => "first half",
        'n'..='z' => "second half",
        _ => "other",
    }
}

fn mode(m: Mode) -> &'static str {
    match m {
        Mode::DEFAULT => "default",
        Mode::Fast => "fast",
        _ => "off",
    }
}

fn cap(n: usize) -> &'static str {
    match n {
        <L as Limits>::CAP => "cap",
        <L as Limits>::FLOOR => "floor",
        _ => "?",
    }
}

fn main() {
    println!("{} {} {}", S(0).describe(), S(1).describe(), S(7).describe());
    for n in [100, 5, 10, 11, 500] {
        print!("{} ", classify(n));
    }
    println!();
    for n in [0u8, 50, 127, 128, 255] {
        print!("{} ", bucket(n));
    }
    println!();
    for n in [9_000_000_000i64, -1, -5, 7] {
        print!("{} ", big(n));
    }
    println!();
    for c in ['a', 'g', 'm', 'q', 'A'] {
        print!("{} ", letter(c));
    }
    println!();
    println!("{} {} {}", mode(Mode::Slow), mode(Mode::Fast), mode(Mode::Off));
    println!("{} {} {}", cap(16), cap(2), cap(3));
    match (B::ON, 3u8) {
        (B::ON, B::LO) => println!("on lo"),
        (B::ON, _) => println!("on"),
        _ => println!("off"),
    }
    for s in ["a", "b"] {
        match s {
            B::NAME => println!("B"),
            _ => println!("not B"),
        }
    }
    match u8::MAX {
        u8::MAX => println!("max"),
        _ => println!("no"),
    }
    let v = vec![Some(Mode::Slow), None, Some(Mode::Fast)];
    let n = v.iter().filter(|m| matches!(m, Some(Mode::DEFAULT))).count();
    println!("{}", n);
    if let Some(B::MID) = Some(127u8) {
        println!("mid");
    }
    let x = S(0);
    if let S::Z = x {
        println!("is zero");
    }
    println!("{}", matches!(x, S::Z | S::ONE));
}
