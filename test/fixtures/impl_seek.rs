// `impl Seek for ..`: your own `seek` gets std's `rewind` / `stream_position` / `seek_relative`;
// `S: Seek` bounds (also through a generic enum's impl), and `Cursor`'s `seek` (a negative
// position is `InvalidInput`).
use std::io::{self, Cursor, Read, Seek, SeekFrom};

struct Tape { data: Vec<u8>, pos: u64 }

impl Seek for Tape {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let p = match pos {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => self.data.len() as i64 + n,
            SeekFrom::Current(n) => self.pos as i64 + n,
        };
        if p < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "before start"));
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}

enum Either<L, R> { Left(L), Right(R) }
impl<L: Seek, R: Seek> Seek for Either<L, R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self { Either::Left(l) => l.seek(pos), Either::Right(r) => r.seek(pos) }
    }
}

fn to_end<S: Seek>(s: &mut S) -> u64 { s.seek(SeekFrom::End(0)).unwrap() }

fn main() {
    let mut t = Tape { data: vec![1, 2, 3, 4, 5], pos: 0 };
    println!("{:?}", t.seek(SeekFrom::Start(2)));
    println!("{:?}", t.seek(SeekFrom::Current(-1)));
    println!("{:?}", t.stream_position());
    println!("{}", to_end(&mut t));
    println!("{:?}", t.seek(SeekFrom::Current(-9)).is_err());
    t.rewind().unwrap();
    t.seek_relative(3).unwrap();
    println!("{}", t.pos);

    let mut c = Cursor::new(vec![10u8, 20, 30, 40]);
    c.seek(SeekFrom::End(-2)).unwrap();
    let mut b = [0u8; 1];
    c.read_exact(&mut b).unwrap();
    println!("{:?} {} {:?}", b, c.position(), c.seek(SeekFrom::Current(-10)).map_err(|e| e.kind()));
    println!("{}", to_end(&mut c));

    let mut e: Either<Tape, Cursor<Vec<u8>>> = Either::Right(Cursor::new(vec![1, 2]));
    println!("{}", to_end(&mut e));
    let mut e2: Either<Tape, Cursor<Vec<u8>>> = Either::Left(Tape { data: vec![0; 7], pos: 0 });
    println!("{}", to_end(&mut e2));
}
