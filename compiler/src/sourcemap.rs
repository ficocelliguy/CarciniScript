//! Minimal source map v3 encoder. One source file, no names.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub gen_line: u32,
    pub gen_col: u32,
    pub src_line: u32,
    pub src_col: u32,
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn vlq(out: &mut String, value: i64) {
    let mut v = if value < 0 {
        ((-value) << 1) | 1
    } else {
        value << 1
    } as u64;
    loop {
        let mut digit = (v & 31) as u8;
        v >>= 5;
        if v > 0 {
            digit |= 32;
        }
        out.push(B64[digit as usize] as char);
        if v == 0 {
            break;
        }
    }
}

pub fn encode_mappings(mappings: &mut Vec<Mapping>) -> String {
    mappings.sort_by_key(|m| (m.gen_line, m.gen_col));
    mappings.dedup_by_key(|m| (m.gen_line, m.gen_col));
    let mut out = String::new();
    let (mut line, mut prev_gen_col, mut prev_src_line, mut prev_src_col) =
        (0u32, 0i64, 0i64, 0i64);
    let mut first_in_line = true;
    for m in mappings.iter() {
        while line < m.gen_line {
            out.push(';');
            line += 1;
            prev_gen_col = 0;
            first_in_line = true;
        }
        if !first_in_line {
            out.push(',');
        }
        first_in_line = false;
        vlq(&mut out, m.gen_col as i64 - prev_gen_col);
        vlq(&mut out, 0); // source index: always the single source
        vlq(&mut out, m.src_line as i64 - prev_src_line);
        vlq(&mut out, m.src_col as i64 - prev_src_col);
        prev_gen_col = m.gen_col as i64;
        prev_src_line = m.src_line as i64;
        prev_src_col = m.src_col as i64;
    }
    out
}

pub fn to_json(source_name: &str, source_content: &str, mappings: &mut Vec<Mapping>) -> String {
    serde_json::json!({
        "version": 3,
        "sources": [source_name],
        "sourcesContent": [source_content],
        "names": [],
        "mappings": encode_mappings(mappings),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_vlq() {
        let mut s = String::new();
        for v in [0, 1, -1, 15, 16, -16, 1000] {
            vlq(&mut s, v);
            s.push(' ');
        }
        assert_eq!(s, "A C D e gB hB w+B ");
    }

    #[test]
    fn encodes_relative_segments() {
        let mut maps = vec![
            Mapping {
                gen_line: 0,
                gen_col: 0,
                src_line: 0,
                src_col: 0,
            },
            Mapping {
                gen_line: 0,
                gen_col: 4,
                src_line: 0,
                src_col: 3,
            },
            Mapping {
                gen_line: 2,
                gen_col: 2,
                src_line: 1,
                src_col: 4,
            },
        ];
        assert_eq!(encode_mappings(&mut maps), "AAAA,IAAG;;EACC");
    }
}
