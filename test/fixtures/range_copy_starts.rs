// In-place slice methods on a range whose start is an expression with a call
// (`tmp[8 - buf.len()..].copy_from_slice(buf)`, gimli) or on a range of a range
// (`dst[1..][..n].copy_from_slice(..)`, ring, regex-automata): the start is evaluated once.
fn next(c: &mut usize) -> usize { *c += 1; *c }
fn main() {
    let buf = [1u8, 2, 3];
    let mut tmp = [0u8; 8];
    tmp[8 - buf.len()..].copy_from_slice(&buf);
    let mut dst = vec![0u8; 8];
    dst[1..][..buf.len()].copy_from_slice(&buf);
    dst[4..][1..3].clone_from_slice(&[9, 9]);
    let mut c = 0;
    let mut out = vec![5, 4, 3, 2, 1, 0];
    out[next(&mut c)..].sort();
    out[..next(&mut c)].reverse();
    println!("{:?} {:?} {:?} {}", tmp, dst, out, c);
    let mut words = vec!["d", "c", "b", "a"];
    let n = words.len();
    words[n - 3..][..2].sort();
    println!("{:?}", words);
}
