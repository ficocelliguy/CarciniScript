// (io prelude)
// `std::io`, compiled in when a program uses it: `io::Error` / `io::ErrorKind` / `io::Result`,
// the `Write` trait (your own writers too; `write!` / `writeln!` into one), `stdout()` /
// `stderr()` (and their locks, `BufWriter`), and `stdin()` (and `BufReader`), which is at its end:
// a script has no input stream. A rewrite (`bundled.rs`) gives these the `Io` names below.

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

/// `std::io::BufReader` over `stdin()`.
pub struct IoBufReader<R> {
    __inner: R,
}

impl<R> IoBufReader<R> {
    pub fn new(inner: R) -> IoBufReader<R> {
        IoBufReader { __inner: inner }
    }
    pub fn read_line(&mut self, _buf: &mut String) -> Result<usize, IoError> {
        Ok(0)
    }
    pub fn read_to_string(&mut self, _buf: &mut String) -> Result<usize, IoError> {
        Ok(0)
    }
    pub fn lines(self) -> std::vec::IntoIter<Result<String, IoError>> {
        Vec::new().into_iter()
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
