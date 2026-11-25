// Coding contract: "Compression II: LZ Decompression"
// Chunks alternate: type 1 = length L then L literal chars, type 2 = length L then offset X,
// copy L chars starting X back. L == 0 ends the chunk without output.

fn decompress(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    let mut literal = true;

    while i < bytes.len() {
        let len = (bytes[i] - b'0') as usize;
        i += 1;
        if literal {
            out.extend_from_slice(&bytes[i..i + len]);
            i += len;
        } else if len > 0 {
            let offset = (bytes[i] - b'0') as usize;
            i += 1;
            let start = out.len() - offset;
            for k in 0..len {
                let c = out[start + k];
                out.push(c);
            }
        }
        literal = !literal;
    }

    String::from_utf8(out).unwrap()
}

fn main() {
    let inputs = [
        "5aaabb450723abb",
        "2ab",
        "2ab21",
        "1a91031",
        "5abcde23",
        // pasted this one from the contract and something must've got mangled
        "3abc47",
    ];

    for input in inputs {
        let result = decompress(input);
        println!("{:<20} => {}", input, result);
    }
}
