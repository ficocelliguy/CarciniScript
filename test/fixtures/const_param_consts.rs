// Const generic parameters read by an impl's associated consts and by a trait's default methods,
// and a `const fn` method of a constant struct as a const argument.
struct U<const P: u128>;
impl<const P: u128> U<P> {
    const PER: u128 = P * 1000;
    fn per(&self) -> u128 {
        Self::PER
    }
}
struct Buf<const N: usize>;
impl<const N: usize> Buf<N> {
    const DOUBLE: usize = N * 2;
    const LAST: usize = N - 1;
    fn both(&self) -> (usize, usize) {
        (Self::DOUBLE, Self::LAST)
    }
}

trait Cap<const N: usize> {
    fn cap(&self) -> usize {
        N
    }
    fn twice(&self) -> usize {
        N * 2
    }
    fn label() -> String
    where
        Self: Sized,
    {
        format!("cap {N}")
    }
}
struct A;
struct B;
struct Arr<const M: usize>([u8; M]);
impl Cap<4> for A {}
impl Cap<8> for B {
    fn twice(&self) -> usize {
        100
    }
}
impl<const M: usize> Cap<M> for Arr<M> {}
impl Cap<3> for i32 {}
trait Named {
    const NAME: &'static str;
    fn greet(&self) -> String {
        format!("hi {}", Self::NAME)
    }
}
struct X;
impl Named for X {
    const NAME: &'static str = "x";
}

#[derive(Clone, Copy)]
struct Config {
    w: usize,
    h: usize,
}
impl Config {
    const DEFAULT: Config = Config { w: 3, h: 4 };
    const fn encode(&self) -> usize {
        self.w * 16 + self.h
    }
    const fn area(self) -> usize {
        self.w * self.h
    }
    const fn scaled(&self, k: usize) -> usize {
        self.w * k
    }
}
const SMALL: Config = Config { w: 1, h: 2 };
struct Grid<const N: usize>([u8; N]);
impl<const N: usize> Grid<N> {
    fn n(&self) -> usize {
        N
    }
}

fn main() {
    println!("{}", U::<7>.per());
    println!("{} {}", U::<3>::PER, Buf::<4>::DOUBLE);
    println!("{} {:?}", Buf::<9>::LAST, Buf::<5>.both());
    println!("{} {} {} {}", A.cap(), A.twice(), B.cap(), B.twice());
    println!("{} {}", A::label(), B::label());
    let arr = Arr([0u8; 6]);
    println!("{} {}", arr.cap(), arr.twice());
    println!("{} {}", 5i32.cap(), 5i32.twice());
    println!("{}", X.greet());
    let g: Grid<{ Config::DEFAULT.encode() }> = Grid([0; { Config::DEFAULT.encode() }]);
    let h: Grid<{ Config::DEFAULT.area() }> = Grid([1; Config::DEFAULT.area()]);
    let s: Grid<{ SMALL.scaled(5) }> = Grid([2; SMALL.scaled(5)]);
    println!("{} {} {} {} {}", g.n(), h.n(), h.0.len(), s.n(), SMALL.encode());
}
