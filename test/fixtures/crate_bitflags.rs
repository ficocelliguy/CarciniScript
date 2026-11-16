// The bundled `bitflags!`: consts, set operations and operators, `from_bits*`, `iter_names`,
// and bitflags 2's `{:?}` (`Perms(READ | WRITE)`, unknown bits in hex).
use bitflags::bitflags;

bitflags! {
    /// Access rights.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Perms: u32 {
        const READ = 0b0001;
        const WRITE = 0b0010;
        const EXEC = 1 << 2;
        const RW = Self::READ.bits() | Self::WRITE.bits();
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Small: u8 {
        const A = 1;
        const B = 2;
    }
}

fn describe(p: Perms) -> String {
    let mut parts = Vec::new();
    if p.contains(Perms::READ) {
        parts.push("r");
    }
    if p.contains(Perms::WRITE) {
        parts.push("w");
    }
    if p.intersects(Perms::EXEC) {
        parts.push("x");
    }
    parts.join("")
}

fn main() {
    let rw = Perms::READ | Perms::WRITE;
    println!("{:?} {:?} {:?}", rw, Perms::empty(), Perms::all());
    println!("{:?} {:?}", Perms::EXEC, Perms::from_bits_retain(0b1_0001));
    println!("{} {} {}", rw.bits(), describe(rw), describe(Perms::all()));
    println!("{} {} {}", rw == Perms::RW, rw.is_empty(), Perms::all().is_all());
    let mut p = Perms::empty();
    p.insert(Perms::EXEC);
    p |= Perms::READ;
    p.toggle(Perms::WRITE);
    p.remove(Perms::READ);
    p.set(Perms::READ, true);
    println!("{:?} {}", p, p.bits());
    println!("{:?} {:?}", p - Perms::EXEC, !Perms::READ);
    println!("{:?} {:?}", Perms::from_bits(3), Perms::from_bits(64));
    println!("{:?}", Perms::from_bits_truncate(0xff));
    println!("{:?} {:?}", Perms::from_name("EXEC"), Perms::from_name("nope"));
    println!("{:?}", rw.iter_names().map(|(n, _)| n).collect::<Vec<_>>());
    println!("{:?}", (rw & Perms::WRITE, rw ^ Perms::all(), rw.complement()));
    println!("{:?} {:?}", Small::A | Small::B, Small::all().union(Small::A));
    let mut s = Small::A;
    s &= Small::B;
    println!("{:?} {}", s, s.is_empty());
}
