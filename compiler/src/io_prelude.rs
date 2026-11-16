// (io prelude)
// `std::io`, compiled in when a program uses it: `io::Error` / `io::ErrorKind` / `io::Result`,
// the `Write` trait (your own writers and `Vec<u8>` too; `write!` / `writeln!` into one), and
// `fmt::Write` (as `FmtWrite`), `stdout()` /
// `stderr()` (and their locks, `BufWriter`), and `stdin()` (and `BufReader`), which is at its end:
// a script has no input stream. A rewrite (`bundled.rs`) gives these the `Io` names below. The
// `Read` and `BufRead` traits (your own readers too) and `Cursor` (reading lines or bytes from a
// `String` / `Vec<u8>`) get theirs from `stdgaps.rs`.

/// `std::io::ErrorKind`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum IoErrorKind {
    NotFound,
    PermissionDenied,
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    NotConnected,
    AddrInUse,
    AddrNotAvailable,
    BrokenPipe,
    AlreadyExists,
    WouldBlock,
    InvalidInput,
    InvalidData,
    TimedOut,
    WriteZero,
    Interrupted,
    Unsupported,
    UnexpectedEof,
    OutOfMemory,
    Other,
}

impl std::fmt::Display for IoErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let text = match self {
            IoErrorKind::NotFound => "entity not found",
            IoErrorKind::PermissionDenied => "permission denied",
            IoErrorKind::ConnectionRefused => "connection refused",
            IoErrorKind::ConnectionReset => "connection reset",
            IoErrorKind::ConnectionAborted => "connection aborted",
            IoErrorKind::NotConnected => "not connected",
            IoErrorKind::AddrInUse => "address in use",
            IoErrorKind::AddrNotAvailable => "address not available",
            IoErrorKind::BrokenPipe => "broken pipe",
            IoErrorKind::AlreadyExists => "entity already exists",
            IoErrorKind::WouldBlock => "operation would block",
            IoErrorKind::InvalidInput => "invalid input parameter",
            IoErrorKind::InvalidData => "invalid data",
            IoErrorKind::TimedOut => "timed out",
            IoErrorKind::WriteZero => "write zero",
            IoErrorKind::Interrupted => "operation interrupted",
            IoErrorKind::Unsupported => "unsupported",
            IoErrorKind::UnexpectedEof => "unexpected end of file",
            IoErrorKind::OutOfMemory => "out of memory",
            IoErrorKind::Other => "other error",
        };
        write!(f, "{}", text)
    }
}

/// `std::io::Error`: a kind, and a message for one made with `new` / `other`.
#[derive(PartialEq)]
pub struct IoError {
    __kind: IoErrorKind,
    __msg: Option<String>,
}

impl IoError {
    pub fn new<E: std::fmt::Display>(kind: IoErrorKind, error: E) -> IoError {
        IoError {
            __kind: kind,
            __msg: Some(error.to_string()),
        }
    }
    pub fn other<E: std::fmt::Display>(error: E) -> IoError {
        IoError::new(IoErrorKind::Other, error)
    }
    pub fn kind(&self) -> IoErrorKind {
        self.__kind
    }
}

impl From<IoErrorKind> for IoError {
    fn from(kind: IoErrorKind) -> IoError {
        IoError {
            __kind: kind,
            __msg: None,
        }
    }
}

impl std::fmt::Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.__msg {
            Some(m) => write!(f, "{}", m),
            None => write!(f, "{}", self.__kind),
        }
    }
}

impl std::fmt::Debug for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.__msg {
            Some(m) => write!(f, "Custom {{ kind: {:?}, error: {:?} }}", self.__kind, m),
            None => write!(f, "Kind({:?})", self.__kind),
        }
    }
}

impl std::error::Error for IoError {}

/// `std::io::Write`.
pub trait IoWrite {
    fn write(&mut self, buf: &[u8]) -> Result<usize, IoError>;
    fn flush(&mut self) -> Result<(), IoError>;
    fn write_all(&mut self, buf: &[u8]) -> Result<(), IoError> {
        let mut at = 0;
        while at < buf.len() {
            let n = self.write(&buf[at..])?;
            if n == 0 {
                return Err(IoError::new(IoErrorKind::WriteZero, "failed to write whole buffer"));
            }
            at += n;
        }
        Ok(())
    }
    // What `write!` / `writeln!` call.
    fn __write_str(&mut self, s: &str) -> Result<(), IoError> {
        self.write_all(s.as_bytes())
    }
}

/// A `Vec<u8>` is a writer: the bytes are appended.
impl IoWrite for Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, IoError> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

/// `std::fmt::Write`, for your own writers (a `String` and a `Formatter` are writers without an
/// impl here: `write!` and these methods reach them through `κσ.$fmtWriteCall`).
pub trait FmtWrite {
    fn write_str(&mut self, s: &str) -> std::fmt::Result;
    fn write_char(&mut self, c: char) -> std::fmt::Result {
        self.write_str(&c.to_string())
    }
    // What `write!` / `writeln!` call.
    fn __write_str(&mut self, s: &str) -> std::fmt::Result {
        self.write_str(s)
    }
}

/// `std::io::Stdout` (and its lock): what's written is printed.
#[derive(Clone)]
pub struct IoStdout {}

impl IoStdout {
    pub fn lock(&self) -> IoStdout {
        IoStdout {}
    }
}

impl IoWrite for IoStdout {
    fn write(&mut self, buf: &[u8]) -> Result<usize, IoError> {
        print!("{}", String::from_utf8_lossy(buf));
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
    fn __write_str(&mut self, s: &str) -> Result<(), IoError> {
        print!("{}", s);
        Ok(())
    }
}

/// `std::io::Stderr` (and its lock).
#[derive(Clone)]
pub struct IoStderr {}

impl IoStderr {
    pub fn lock(&self) -> IoStderr {
        IoStderr {}
    }
}

impl IoWrite for IoStderr {
    fn write(&mut self, buf: &[u8]) -> Result<usize, IoError> {
        eprint!("{}", String::from_utf8_lossy(buf));
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
    fn __write_str(&mut self, s: &str) -> Result<(), IoError> {
        eprint!("{}", s);
        Ok(())
    }
}

/// `std::io::BufWriter`: writes go straight through.
pub struct IoBufWriter<W> {
    __inner: W,
}

impl<W: IoWrite> IoBufWriter<W> {
    pub fn new(inner: W) -> IoBufWriter<W> {
        IoBufWriter { __inner: inner }
    }
    pub fn with_capacity(_capacity: usize, inner: W) -> IoBufWriter<W> {
        IoBufWriter { __inner: inner }
    }
    pub fn get_ref(&self) -> &W {
        &self.__inner
    }
    pub fn into_inner(self) -> Result<W, IoError> {
        Ok(self.__inner)
    }
}

impl<W: IoWrite> IoWrite for IoBufWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, IoError> {
        self.__inner.write(buf)
    }
    fn flush(&mut self) -> Result<(), IoError> {
        self.__inner.flush()
    }
    fn __write_str(&mut self, s: &str) -> Result<(), IoError> {
        self.__inner.__write_str(s)
    }
}

/// `std::io::Stdin` (and its lock): at its end, as a script has no input.
#[derive(Clone)]
pub struct IoStdin {}

impl IoStdin {
    pub fn lock(&self) -> IoStdin {
        IoStdin {}
    }
    /// Reads nothing: `Ok(0)` is the end of the input.
    pub fn read_line(&self, _buf: &mut String) -> Result<usize, IoError> {
        Ok(0)
    }
    pub fn read_to_string(&mut self, _buf: &mut String) -> Result<usize, IoError> {
        Ok(0)
    }
    pub fn lines(self) -> std::vec::IntoIter<Result<String, IoError>> {
        Vec::new().into_iter()
    }
}

/// `std::io::BufReader`: over `stdin()` (at its end), a `Cursor`, or your own `Read`.
pub struct IoBufReader<R> {
    __inner: R,
    __buf: Vec<u8>,
    __pos: usize,
}

// (`R: IoRead`: over bytes, `inner` is a `Cursor` of them.)
impl<R: IoRead> IoBufReader<R> {
    pub fn new(inner: R) -> IoBufReader<R> {
        IoBufReader {
            __inner: inner,
            __buf: Vec::new(),
            __pos: 0,
        }
    }
    pub fn with_capacity(_capacity: usize, inner: R) -> IoBufReader<R> {
        IoBufReader::new(inner)
    }
}

impl<R> IoBufReader<R> {
    pub fn get_ref(&self) -> &R {
        &self.__inner
    }
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.__inner
    }
    pub fn into_inner(self) -> R {
        self.__inner
    }
    pub fn buffer(&self) -> Vec<u8> {
        self.__buf[self.__pos..].to_vec()
    }
}

impl<R: IoRead> IoRead for IoBufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        if self.__pos >= self.__buf.len() {
            return self.__inner.read(buf);
        }
        let n = std::cmp::min(buf.len(), self.__buf.len() - self.__pos);
        for i in 0..n {
            buf[i] = self.__buf[self.__pos + i];
        }
        self.__pos += n;
        Ok(n)
    }
}

impl<R: IoRead> IoBufRead for IoBufReader<R> {
    fn fill_buf(&mut self) -> Result<Vec<u8>, IoError> {
        if self.__pos >= self.__buf.len() {
            let mut b = vec![0u8; 8192];
            let n = self.__inner.read(&mut b)?;
            b.truncate(n);
            self.__buf = b;
            self.__pos = 0;
        }
        Ok(self.__buf[self.__pos..].to_vec())
    }
    fn consume(&mut self, amt: usize) {
        self.__pos = std::cmp::min(self.__pos + amt, self.__buf.len());
    }
}

impl IoRead for IoStdin {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, IoError> {
        Ok(0)
    }
}

pub fn __io_stdout() -> IoStdout {
    IoStdout {}
}

pub fn __io_stderr() -> IoStderr {
    IoStderr {}
}

pub fn __io_stdin() -> IoStdin {
    IoStdin {}
}

/// `std::io::Read`.
pub trait IoRead {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError>;
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize, IoError> {
        let mut chunk = vec![0u8; 512];
        let mut total = 0;
        loop {
            let n = self.read(&mut chunk)?;
            if n == 0 {
                return Ok(total);
            }
            buf.extend_from_slice(&chunk[..n]);
            total += n;
        }
    }
    fn read_to_string(&mut self, buf: &mut String) -> Result<usize, IoError> {
        let mut bytes = Vec::new();
        let n = self.read_to_end(&mut bytes)?;
        match String::from_utf8(bytes) {
            Ok(s) => {
                buf.push_str(&s);
                Ok(n)
            }
            Err(_) => Err(IoError::new(
                IoErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )),
        }
    }
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), IoError> {
        let mut at = 0;
        while at < buf.len() {
            let mut part = vec![0u8; buf.len() - at];
            let n = self.read(&mut part)?;
            if n == 0 {
                return Err(IoError::new(
                    IoErrorKind::UnexpectedEof,
                    "failed to fill whole buffer",
                ));
            }
            for i in 0..n {
                buf[at + i] = part[i];
            }
            at += n;
        }
        Ok(())
    }
    fn bytes(mut self) -> std::vec::IntoIter<Result<u8, IoError>>
    where
        Self: Sized,
    {
        let mut all = Vec::new();
        let mut out = Vec::new();
        match self.read_to_end(&mut all) {
            Ok(_) => {
                for b in all {
                    out.push(Ok(b));
                }
            }
            Err(e) => out.push(Err(e)),
        }
        out.into_iter()
    }
}

/// `std::io::BufRead`.
pub trait IoBufRead {
    fn fill_buf(&mut self) -> Result<Vec<u8>, IoError>;
    fn consume(&mut self, amt: usize);
    fn has_data_left(&mut self) -> Result<bool, IoError> {
        Ok(!self.fill_buf()?.is_empty())
    }
    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> Result<usize, IoError> {
        let mut total = 0;
        loop {
            let avail = self.fill_buf()?;
            if avail.is_empty() {
                return Ok(total);
            }
            match avail.iter().position(|b| *b == byte) {
                Some(i) => {
                    buf.extend_from_slice(&avail[..i + 1]);
                    self.consume(i + 1);
                    return Ok(total + i + 1);
                }
                None => {
                    let n = avail.len();
                    buf.extend_from_slice(&avail);
                    self.consume(n);
                    total += n;
                }
            }
        }
    }
    fn skip_until(&mut self, byte: u8) -> Result<usize, IoError> {
        let mut skipped = Vec::new();
        self.read_until(byte, &mut skipped)
    }
    fn read_line(&mut self, buf: &mut String) -> Result<usize, IoError> {
        let mut bytes = Vec::new();
        let n = self.read_until(10u8, &mut bytes)?;
        match String::from_utf8(bytes) {
            Ok(s) => {
                buf.push_str(&s);
                Ok(n)
            }
            Err(_) => Err(IoError::new(
                IoErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            )),
        }
    }
    fn lines(mut self) -> std::vec::IntoIter<Result<String, IoError>>
    where
        Self: Sized,
    {
        let mut out = Vec::new();
        loop {
            let mut line = String::new();
            match self.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if line.ends_with('\n') {
                        line.pop();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                    }
                    out.push(Ok(line));
                }
                Err(e) => {
                    out.push(Err(e));
                    break;
                }
            }
        }
        out.into_iter()
    }
    fn split(mut self, byte: u8) -> std::vec::IntoIter<Result<Vec<u8>, IoError>>
    where
        Self: Sized,
    {
        let mut out = Vec::new();
        loop {
            let mut part = Vec::new();
            match self.read_until(byte, &mut part) {
                Ok(0) => break,
                Ok(_) => {
                    if part.last() == Some(&byte) {
                        part.pop();
                    }
                    out.push(Ok(part));
                }
                Err(e) => {
                    out.push(Err(e));
                    break;
                }
            }
        }
        out.into_iter()
    }
}

/// `std::io::Cursor`: reads (and, over a `Vec<u8>`, writes) its data from a position.
#[derive(Clone, Default, PartialEq)]
pub struct IoCursor<T> {
    inner: T,
    pos: u64,
}

impl<T: std::fmt::Debug> std::fmt::Debug for IoCursor<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Cursor {{ inner: {:?}, pos: {} }}", self.inner, self.pos)
    }
}

impl<T> IoCursor<T> {
    pub fn new(inner: T) -> IoCursor<T> {
        IoCursor { inner, pos: 0 }
    }
    pub fn into_inner(self) -> T {
        self.inner
    }
    pub fn get_ref(&self) -> &T {
        &self.inner
    }
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
    pub fn position(&self) -> u64 {
        self.pos
    }
    pub fn set_position(&mut self, pos: u64) {
        self.pos = pos;
    }
    pub fn is_empty(&self) -> bool {
        self.pos as usize >= js::__io_bytes(&self.inner).len()
    }
    pub fn remaining_slice(&self) -> Vec<u8> {
        let data = js::__io_bytes(&self.inner);
        let at = std::cmp::min(self.pos as usize, data.len());
        data[at..].to_vec()
    }
}

impl<T> IoRead for IoCursor<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        let data = js::__io_bytes(&self.inner);
        let at = std::cmp::min(self.pos as usize, data.len());
        let n = std::cmp::min(buf.len(), data.len() - at);
        for i in 0..n {
            buf[i] = data[at + i];
        }
        self.pos += n as u64;
        Ok(n)
    }
}

impl<T> IoBufRead for IoCursor<T> {
    fn fill_buf(&mut self) -> Result<Vec<u8>, IoError> {
        Ok(self.remaining_slice())
    }
    fn consume(&mut self, amt: usize) {
        self.pos += amt as u64;
    }
}

impl<T> IoWrite for IoCursor<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, IoError> {
        let mut data = js::__io_bytes(&self.inner);
        let mut at = self.pos as usize;
        while data.len() < at {
            data.push(0);
        }
        for b in buf {
            if at < data.len() {
                data[at] = *b;
            } else {
                data.push(*b);
            }
            at += 1;
        }
        self.pos = at as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}
