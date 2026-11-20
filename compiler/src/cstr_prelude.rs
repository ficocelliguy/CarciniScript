// (cstr prelude)
// `std::ffi::CStr` (also `core::ffi::CStr`), compiled in when a program names it or writes a
// C string literal (`c"text"`, which `stdgaps` writes `CStr::__lit(&[..])`): the bytes, without
// the nul. There's no C here, so it's only a value: `to_bytes`, `to_str`, `{:?}`, comparisons.

/// `std::ffi::CStr`: bytes with no nul inside, the nul at the end implied.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CStr {
    bytes: Vec<u8>,
}

/// `CStr::from_bytes_with_nul`'s error.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FromBytesWithNulError {
    InteriorNul { position: usize },
    NotNulTerminated,
}

impl std::fmt::Display for FromBytesWithNulError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FromBytesWithNulError::InteriorNul { position } => write!(
                f,
                "data provided contains an interior nul byte at byte position {}",
                position
            ),
            FromBytesWithNulError::NotNulTerminated => {
                write!(f, "data provided is not nul terminated")
            }
        }
    }
}

impl std::error::Error for FromBytesWithNulError {}

/// `CStr::from_bytes_until_nul`'s error.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FromBytesUntilNulError;

impl std::fmt::Debug for FromBytesUntilNulError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "FromBytesUntilNulError(())")
    }
}

impl std::fmt::Display for FromBytesUntilNulError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "data provided does not contain a nul")
    }
}

impl std::error::Error for FromBytesUntilNulError {}

impl CStr {
    /// A `c"..."` literal's bytes (no nul).
    pub fn __lit(bytes: &[u8]) -> CStr {
        CStr {
            bytes: bytes.to_vec(),
        }
    }

    pub fn from_bytes_with_nul(bytes: &[u8]) -> Result<CStr, FromBytesWithNulError> {
        match bytes.iter().position(|b| *b == 0) {
            Some(i) if i + 1 == bytes.len() => Ok(CStr {
                bytes: bytes[..i].to_vec(),
            }),
            Some(i) => Err(FromBytesWithNulError::InteriorNul { position: i }),
            None => Err(FromBytesWithNulError::NotNulTerminated),
        }
    }

    pub fn from_bytes_until_nul(bytes: &[u8]) -> Result<CStr, FromBytesUntilNulError> {
        match bytes.iter().position(|b| *b == 0) {
            Some(i) => Ok(CStr {
                bytes: bytes[..i].to_vec(),
            }),
            None => Err(FromBytesUntilNulError),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    pub fn to_bytes_with_nul(&self) -> Vec<u8> {
        let mut v = self.bytes.clone();
        v.push(0);
        v
    }

    pub fn count_bytes(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn to_str(&self) -> Result<String, std::str::Utf8Error> {
        std::str::from_utf8(&self.bytes).map(|s| s.to_string())
    }

    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.bytes).to_string()
    }
}

/// One byte as `u8::escape_ascii` writes it.
fn cstr_escape_ascii(s: &mut String, b: u8) {
    match b {
        b'\t' => s.push_str("\\t"),
        b'\r' => s.push_str("\\r"),
        b'\n' => s.push_str("\\n"),
        b'\\' => s.push_str("\\\\"),
        b'\'' => s.push_str("\\'"),
        b'"' => s.push_str("\\\""),
        0x20..=0x7e => s.push(b as char),
        _ => s.push_str(&format!("\\x{:02x}", b)),
    }
}

// (std's: in quotes, ASCII as `escape_ascii` writes it, other UTF-8 characters as
// `escape_debug` does, and bytes that aren't UTF-8 as `\xNN`.)
impl std::fmt::Debug for CStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let b = &self.bytes;
        let mut s = String::from("\"");
        let mut i = 0;
        while i < b.len() {
            let len = match b[i] {
                0x00..=0x7f => 1,
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => 0,
            };
            if len == 1 {
                cstr_escape_ascii(&mut s, b[i]);
                i += 1;
                continue;
            }
            let ch = if len > 0 && i + len <= b.len() {
                std::str::from_utf8(&b[i..i + len])
                    .ok()
                    .and_then(|t| t.chars().next())
            } else {
                None
            };
            match ch {
                Some(c) => {
                    s.push_str(&c.escape_debug().to_string());
                    i += len;
                }
                None => {
                    cstr_escape_ascii(&mut s, b[i]);
                    i += 1;
                }
            }
        }
        s.push('"');
        write!(f, "{}", s)
    }
}

/// `std::ffi::CString`: an owned `CStr`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CString {
    c: CStr,
}

/// `CString::new`'s error: a nul byte inside the data (its position, and the data).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NulError(usize, Vec<u8>);

impl NulError {
    pub fn nul_position(&self) -> usize {
        self.0
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.1
    }
}

impl std::fmt::Display for NulError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "nul byte found in provided data at position: {}", self.0)
    }
}

impl std::error::Error for NulError {}

/// What `CString::new` takes: text or bytes.
pub trait __CStringBytes {
    fn __c_bytes(&self) -> Vec<u8>;
}

impl __CStringBytes for String {
    fn __c_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl __CStringBytes for Vec<u8> {
    fn __c_bytes(&self) -> Vec<u8> {
        self.clone()
    }
}

impl CString {
    /// `CString::new("text")` / `CString::new(bytes)`: `Err` when a nul byte is inside.
    pub fn new<T: __CStringBytes>(t: T) -> Result<CString, NulError> {
        let bytes = t.__c_bytes();
        match bytes.iter().position(|b| *b == 0) {
            Some(i) => Err(NulError(i, bytes)),
            None => Ok(CString {
                c: CStr::__lit(&bytes),
            }),
        }
    }

    pub fn as_c_str(&self) -> &CStr {
        &self.c
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        self.c.to_bytes()
    }

    pub fn as_bytes_with_nul(&self) -> Vec<u8> {
        self.c.to_bytes_with_nul()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.c.to_bytes()
    }

    pub fn into_bytes_with_nul(self) -> Vec<u8> {
        self.c.to_bytes_with_nul()
    }

    pub fn into_string(self) -> Result<String, std::str::Utf8Error> {
        self.c.to_str()
    }
}

impl std::ops::Deref for CString {
    type Target = CStr;
    fn deref(&self) -> &CStr {
        &self.c
    }
}

impl std::fmt::Debug for CString {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.c)
    }
}

impl From<&CStr> for CString {
    fn from(c: &CStr) -> CString {
        CString { c: c.clone() }
    }
}

impl From<CString> for Vec<u8> {
    fn from(s: CString) -> Vec<u8> {
        s.c.to_bytes()
    }
}
