// Inherent impls for `Either<&L, &R>` and `Either<&mut L, &mut R>`, each with a `cloned`.
#[derive(Debug)]
enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L: Clone, R: Clone> Either<&L, &R> {
    fn cloned(self) -> Either<L, R> {
        match self {
            Either::Left(l) => Either::Left(l.clone()),
            Either::Right(r) => Either::Right(r.clone()),
        }
    }
}

impl<L: Clone, R: Clone> Either<&mut L, &mut R> {
    fn cloned(self) -> Either<L, R> {
        match self {
            Either::Left(l) => Either::Left(l.clone()),
            Either::Right(r) => Either::Right(r.clone()),
        }
    }
}

fn main() {
    let a = 3;
    let s = String::from("x");
    let e: Either<&i32, &String> = Either::Left(&a);
    let f: Either<&i32, &String> = Either::Right(&s);
    println!("{:?} {:?}", e.cloned(), f.cloned());
    let mut v = vec![1, 2];
    let mut t = String::from("y");
    let g: Either<&mut Vec<i32>, &mut String> = Either::Left(&mut v);
    let mut owned = g.cloned();
    if let Either::Left(w) = &mut owned {
        w.push(3);
    }
    let h: Either<&mut Vec<i32>, &mut String> = Either::Right(&mut t);
    println!("{:?} {:?} {:?}", owned, h.cloned(), v);
}
