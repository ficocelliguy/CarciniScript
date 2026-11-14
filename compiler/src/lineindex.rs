/// Maps byte offsets in UTF-8 source text to zero-based (line, UTF-16 column) pairs,
/// which is what source maps, Monaco and devtools all expect.
pub struct LineIndex<'a> {
    text: &'a str,
    line_starts: Vec<usize>,
    /// When the compiled text is a macro expansion of `text`: expanded offsets → `text`'s.
    map: Option<std::rc::Rc<crate::mbe::OffsetMap>>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex {
            text,
            line_starts,
            map: None,
        }
    }

    /// Positions of an expansion of `original`, reported in `original`.
    pub fn mapped(original: &'a str, map: std::rc::Rc<crate::mbe::OffsetMap>) -> Self {
        LineIndex {
            map: Some(map),
            ..LineIndex::new(original)
        }
    }

    pub fn line_col(&self, offset: usize) -> (u32, u32) {
        let offset = match &self.map {
            Some(m) => m.orig(offset, false),
            None => offset,
        };
        let offset = offset.min(self.text.len());
        let line = match self.line_starts.binary_search(&offset) {
            Ok(l) => l,
            Err(l) => l - 1,
        };
        let start = self.line_starts[line];
        let col: usize = self.text[start..offset].chars().map(char::len_utf16).sum();
        (line as u32, col as u32)
    }
}

/// Number of UTF-16 code units in `s`, used to track generated-code columns.
pub fn utf16_len(s: &str) -> u32 {
    s.chars().map(char::len_utf16).sum::<usize>() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_offsets_across_lines_and_wide_chars() {
        let idx = LineIndex::new("ab\n😀x\ny");
        assert_eq!(idx.line_col(0), (0, 0));
        assert_eq!(idx.line_col(3), (1, 0));
        // 😀 is 4 bytes in UTF-8 and 2 code units in UTF-16.
        assert_eq!(idx.line_col(7), (1, 2));
        assert_eq!(idx.line_col(9), (2, 0));
    }
}
