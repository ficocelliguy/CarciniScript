// (os prelude)
// `std::fs` and `std::process::Command`, compiled in when a program names them. A script has no
// filesystem (Bitburner has `ns.read` / `ns.write`) and can't start processes, so these compile
// and every operation fails with an `io::Error`: `NotFound` for reading (`File::open`,
// `fs::read_to_string`, `fs::metadata`, ..), `Unsupported` for anything else (creating, writing,
// removing, running a command). A rewrite (`bundled.rs`) gives these the `Fs` / `Proc` names.

fn __fs_not_found(what: &str) -> IoError {
    IoError::new(
        IoErrorKind::NotFound,
        format!("{what}: a script has no filesystem (Bitburner has `ns.read`)"),
    )
}

fn __fs_unsupported(what: &str) -> IoError {
    IoError::new(
        IoErrorKind::Unsupported,
        format!("{what}: a script has no filesystem (Bitburner has `ns.write`)"),
    )
}

/// `std::fs::File`: never opened (`open` fails with `NotFound`, `create` with `Unsupported`).
#[derive(Debug)]
pub struct FsFile {
    __path: String,
}

impl FsFile {
    pub fn open<P>(_path: P) -> Result<FsFile, IoError> {
        Err(__fs_not_found("File::open"))
    }
    pub fn create<P>(_path: P) -> Result<FsFile, IoError> {
        Err(__fs_unsupported("File::create"))
    }
    pub fn create_new<P>(_path: P) -> Result<FsFile, IoError> {
        Err(__fs_unsupported("File::create_new"))
    }
    pub fn options() -> FsOpenOptions {
        FsOpenOptions::new()
    }
    pub fn metadata(&self) -> Result<FsMetadata, IoError> {
        Err(__fs_unsupported("File::metadata"))
    }
    pub fn sync_all(&self) -> Result<(), IoError> {
        Err(__fs_unsupported("File::sync_all"))
    }
    pub fn sync_data(&self) -> Result<(), IoError> {
        Err(__fs_unsupported("File::sync_data"))
    }
    pub fn set_len(&self, _size: u64) -> Result<(), IoError> {
        Err(__fs_unsupported("File::set_len"))
    }
    pub fn try_clone(&self) -> Result<FsFile, IoError> {
        Err(__fs_unsupported("File::try_clone"))
    }
}

impl IoRead for FsFile {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, IoError> {
        Err(__fs_unsupported("File::read"))
    }
}

impl IoWrite for FsFile {
    fn write(&mut self, _buf: &[u8]) -> Result<usize, IoError> {
        Err(__fs_unsupported("File::write"))
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Err(__fs_unsupported("File::flush"))
    }
}

/// `std::fs::OpenOptions`: `open` fails like `File::open` / `File::create`.
#[derive(Clone, Debug)]
pub struct FsOpenOptions {
    __write: bool,
}

impl FsOpenOptions {
    pub fn new() -> FsOpenOptions {
        FsOpenOptions { __write: false }
    }
    pub fn read(&mut self, _read: bool) -> &mut FsOpenOptions {
        self
    }
    pub fn write(&mut self, write: bool) -> &mut FsOpenOptions {
        self.__write = self.__write || write;
        self
    }
    pub fn append(&mut self, append: bool) -> &mut FsOpenOptions {
        self.__write = self.__write || append;
        self
    }
    pub fn truncate(&mut self, truncate: bool) -> &mut FsOpenOptions {
        self.__write = self.__write || truncate;
        self
    }
    pub fn create(&mut self, create: bool) -> &mut FsOpenOptions {
        self.__write = self.__write || create;
        self
    }
    pub fn create_new(&mut self, create_new: bool) -> &mut FsOpenOptions {
        self.__write = self.__write || create_new;
        self
    }
    pub fn open<P>(&self, _path: P) -> Result<FsFile, IoError> {
        if self.__write {
            Err(__fs_unsupported("OpenOptions::open"))
        } else {
            Err(__fs_not_found("OpenOptions::open"))
        }
    }
}

/// `std::fs::Metadata`: never made (`fs::metadata` fails).
#[derive(Clone, Debug)]
pub struct FsMetadata {
    __len: u64,
}

impl FsMetadata {
    pub fn len(&self) -> u64 {
        self.__len
    }
    pub fn is_file(&self) -> bool {
        false
    }
    pub fn is_dir(&self) -> bool {
        false
    }
    pub fn is_symlink(&self) -> bool {
        false
    }
    pub fn file_type(&self) -> FsFileType {
        FsFileType { __kind: 0 }
    }
}

/// `std::fs::FileType`: never made (`Metadata` and `DirEntry` aren't).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct FsFileType {
    __kind: u8,
}

impl FsFileType {
    pub fn is_file(&self) -> bool {
        self.__kind == 1
    }
    pub fn is_dir(&self) -> bool {
        self.__kind == 2
    }
    pub fn is_symlink(&self) -> bool {
        self.__kind == 3
    }
}

/// `std::fs::DirEntry`: never made (`fs::read_dir` fails).
#[derive(Debug)]
pub struct FsDirEntry {
    __name: String,
}

impl FsDirEntry {
    pub fn file_name(&self) -> String {
        self.__name.clone()
    }
    pub fn path(&self) -> PathBuf {
        PathBuf::from(self.__name.clone())
    }
    pub fn metadata(&self) -> Result<FsMetadata, IoError> {
        Err(__fs_unsupported("DirEntry::metadata"))
    }
    pub fn file_type(&self) -> Result<FsFileType, IoError> {
        Err(__fs_unsupported("DirEntry::file_type"))
    }
}

pub fn __fs_read_to_string<P>(_path: P) -> Result<String, IoError> {
    Err(__fs_not_found("fs::read_to_string"))
}

pub fn __fs_read<P>(_path: P) -> Result<Vec<u8>, IoError> {
    Err(__fs_not_found("fs::read"))
}

pub fn __fs_metadata<P>(_path: P) -> Result<FsMetadata, IoError> {
    Err(__fs_not_found("fs::metadata"))
}

pub fn __fs_read_dir<P>(_path: P) -> Result<std::vec::IntoIter<Result<FsDirEntry, IoError>>, IoError> {
    Err(__fs_not_found("fs::read_dir"))
}

pub fn __fs_symlink_metadata<P>(_path: P) -> Result<FsMetadata, IoError> {
    Err(__fs_not_found("fs::symlink_metadata"))
}

pub fn __fs_read_link<P>(_path: P) -> Result<PathBuf, IoError> {
    Err(__fs_not_found("fs::read_link"))
}

pub fn __fs_canonicalize<P>(_path: P) -> Result<PathBuf, IoError> {
    Err(__fs_not_found("fs::canonicalize"))
}

pub fn __fs_exists<P>(_path: P) -> Result<bool, IoError> {
    Ok(false)
}

pub fn __fs_write<P, C>(_path: P, _contents: C) -> Result<(), IoError> {
    Err(__fs_unsupported("fs::write"))
}

pub fn __fs_create_dir<P>(_path: P) -> Result<(), IoError> {
    Err(__fs_unsupported("fs::create_dir"))
}

pub fn __fs_create_dir_all<P>(_path: P) -> Result<(), IoError> {
    Err(__fs_unsupported("fs::create_dir_all"))
}

pub fn __fs_remove_file<P>(_path: P) -> Result<(), IoError> {
    Err(__fs_unsupported("fs::remove_file"))
}

pub fn __fs_remove_dir<P>(_path: P) -> Result<(), IoError> {
    Err(__fs_unsupported("fs::remove_dir"))
}

pub fn __fs_remove_dir_all<P>(_path: P) -> Result<(), IoError> {
    Err(__fs_unsupported("fs::remove_dir_all"))
}

pub fn __fs_copy<P, Q>(_from: P, _to: Q) -> Result<u64, IoError> {
    Err(__fs_unsupported("fs::copy"))
}

pub fn __fs_rename<P, Q>(_from: P, _to: Q) -> Result<(), IoError> {
    Err(__fs_unsupported("fs::rename"))
}

/// `Path`'s methods that read the filesystem: they fail like `fs::metadata` and the rest.
impl PathBuf {
    pub fn metadata(&self) -> Result<FsMetadata, IoError> {
        Err(__fs_not_found("Path::metadata"))
    }
    pub fn symlink_metadata(&self) -> Result<FsMetadata, IoError> {
        Err(__fs_not_found("Path::symlink_metadata"))
    }
    pub fn read_dir(&self) -> Result<std::vec::IntoIter<Result<FsDirEntry, IoError>>, IoError> {
        Err(__fs_not_found("Path::read_dir"))
    }
    pub fn read_link(&self) -> Result<PathBuf, IoError> {
        Err(__fs_not_found("Path::read_link"))
    }
    pub fn canonicalize(&self) -> Result<PathBuf, IoError> {
        Err(__fs_not_found("Path::canonicalize"))
    }
}

fn __proc_unsupported(what: &str) -> IoError {
    IoError::new(
        IoErrorKind::Unsupported,
        format!("{what}: a script can't run processes"),
    )
}

/// `std::process::Command`: built as usual (its program and arguments aren't kept); running it
/// fails with `Unsupported`.
#[derive(Debug)]
pub struct ProcCommand {}

impl ProcCommand {
    pub fn new<S>(_program: S) -> ProcCommand {
        ProcCommand {}
    }
    pub fn arg<S>(&mut self, _arg: S) -> &mut ProcCommand {
        self
    }
    pub fn args<I>(&mut self, _args: I) -> &mut ProcCommand {
        self
    }
    pub fn env<K, V>(&mut self, _key: K, _val: V) -> &mut ProcCommand {
        self
    }
    pub fn env_remove<K>(&mut self, _key: K) -> &mut ProcCommand {
        self
    }
    pub fn env_clear(&mut self) -> &mut ProcCommand {
        self
    }
    pub fn current_dir<P>(&mut self, _dir: P) -> &mut ProcCommand {
        self
    }
    pub fn stdin(&mut self, _cfg: ProcStdio) -> &mut ProcCommand {
        self
    }
    pub fn stdout(&mut self, _cfg: ProcStdio) -> &mut ProcCommand {
        self
    }
    pub fn stderr(&mut self, _cfg: ProcStdio) -> &mut ProcCommand {
        self
    }
    pub fn output(&mut self) -> Result<ProcOutput, IoError> {
        Err(__proc_unsupported("Command::output"))
    }
    pub fn status(&mut self) -> Result<ProcExitStatus, IoError> {
        Err(__proc_unsupported("Command::status"))
    }
    pub fn spawn(&mut self) -> Result<ProcChild, IoError> {
        Err(__proc_unsupported("Command::spawn"))
    }
}

/// `std::process::Stdio`.
#[derive(Debug)]
pub struct ProcStdio {}

impl ProcStdio {
    pub fn piped() -> ProcStdio {
        ProcStdio {}
    }
    pub fn inherit() -> ProcStdio {
        ProcStdio {}
    }
    pub fn null() -> ProcStdio {
        ProcStdio {}
    }
}

/// `std::process::ExitStatus`: never made (commands don't run).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ProcExitStatus {
    __code: i32,
}

impl ProcExitStatus {
    pub fn success(&self) -> bool {
        self.__code == 0
    }
    pub fn code(&self) -> Option<i32> {
        Some(self.__code)
    }
}

impl std::fmt::Display for ProcExitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "exit code: {}", self.__code)
    }
}

/// `std::process::Output`.
#[derive(Clone, Debug)]
pub struct ProcOutput {
    pub status: ProcExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// `std::process::Child`: never made (`spawn` fails).
#[derive(Debug)]
pub struct ProcChild {
    __id: u32,
    pub stdin: Option<ProcChildStdin>,
    pub stdout: Option<ProcChildStdout>,
    pub stderr: Option<ProcChildStderr>,
}

/// `std::process::ChildStdin`: never made.
#[derive(Debug)]
pub struct ProcChildStdin {}

impl IoWrite for ProcChildStdin {
    fn write(&mut self, _buf: &[u8]) -> Result<usize, IoError> {
        Err(__proc_unsupported("ChildStdin::write"))
    }
    fn flush(&mut self) -> Result<(), IoError> {
        Err(__proc_unsupported("ChildStdin::flush"))
    }
}

/// `std::process::ChildStdout`: never made.
#[derive(Debug)]
pub struct ProcChildStdout {}

impl IoRead for ProcChildStdout {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, IoError> {
        Err(__proc_unsupported("ChildStdout::read"))
    }
}

/// `std::process::ChildStderr`: never made.
#[derive(Debug)]
pub struct ProcChildStderr {}

impl IoRead for ProcChildStderr {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, IoError> {
        Err(__proc_unsupported("ChildStderr::read"))
    }
}

impl ProcChild {
    pub fn id(&self) -> u32 {
        self.__id
    }
    pub fn wait(&mut self) -> Result<ProcExitStatus, IoError> {
        Err(__proc_unsupported("Child::wait"))
    }
    pub fn wait_with_output(self) -> Result<ProcOutput, IoError> {
        Err(__proc_unsupported("Child::wait_with_output"))
    }
    pub fn kill(&mut self) -> Result<(), IoError> {
        Err(__proc_unsupported("Child::kill"))
    }
    pub fn try_wait(&mut self) -> Result<Option<ProcExitStatus>, IoError> {
        Err(__proc_unsupported("Child::try_wait"))
    }
}
