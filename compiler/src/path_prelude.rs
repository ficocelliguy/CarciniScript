// (path prelude)
// `std::path`, compiled in when a program uses it. A path is its text: `Path` and `PathBuf` are
// both `PathBuf` here (a pre-pass writes `Path::new(..)` as `PathBuf::__new(..)`), and `OsStr` /
// `OsString` are `String`. Paths are Unix-style (`/`). A script has no file system, so `exists()`,
// `is_file()` and `is_dir()` are `false`.

/// What a path argument can be (`impl AsRef<Path>`): a string or a path.
pub trait __AsPath {
    fn __path_str(&self) -> String;
    fn as_ref(&self) -> PathBuf {
        PathBuf {
            __s: self.__path_str(),
        }
    }
}

impl __AsPath for String {
    fn __path_str(&self) -> String {
        self.clone()
    }
}

impl __AsPath for PathBuf {
    fn __path_str(&self) -> String {
        self.__s.clone()
    }
}

/// `std::path::Component`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Component {
    RootDir,
    CurDir,
    ParentDir,
    Normal(String),
}

impl Component {
    pub fn as_os_str(&self) -> String {
        match self {
            Component::RootDir => "/".to_string(),
            Component::CurDir => ".".to_string(),
            Component::ParentDir => "..".to_string(),
            Component::Normal(s) => s.clone(),
        }
    }
}

/// `std::path::StripPrefixError`.
#[derive(Clone, PartialEq, Eq)]
pub struct StripPrefixError {}

impl std::fmt::Debug for StripPrefixError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "StripPrefixError(())")
    }
}

impl std::fmt::Display for StripPrefixError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "prefix not found")
    }
}

impl std::error::Error for StripPrefixError {}

/// `std::path::PathBuf` (and `Path`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PathBuf {
    __s: String,
}

impl std::fmt::Debug for PathBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.__s)
    }
}

impl From<String> for PathBuf {
    fn from(s: String) -> PathBuf {
        PathBuf { __s: s }
    }
}

impl std::str::FromStr for PathBuf {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<PathBuf, Infallible> {
        Ok(PathBuf { __s: s.to_string() })
    }
}

impl PathBuf {
    pub fn new() -> PathBuf {
        PathBuf { __s: String::new() }
    }
    pub fn with_capacity(_capacity: usize) -> PathBuf {
        PathBuf::new()
    }
    /// `Path::new(..)`.
    pub fn __new<P: __AsPath>(p: P) -> PathBuf {
        PathBuf {
            __s: p.__path_str(),
        }
    }
    pub fn as_path(&self) -> PathBuf {
        self.clone()
    }
    pub fn to_path_buf(&self) -> PathBuf {
        self.clone()
    }
    pub fn into_boxed_path(self) -> PathBuf {
        self
    }
    pub fn into_path_buf(self) -> PathBuf {
        self
    }
    pub fn as_os_str(&self) -> String {
        self.__s.clone()
    }
    pub fn into_os_string(self) -> String {
        self.__s
    }
    pub fn to_str(&self) -> Option<String> {
        Some(self.__s.clone())
    }
    pub fn to_string_lossy(&self) -> String {
        self.__s.clone()
    }
    /// What `{}` shows of it.
    pub fn display(&self) -> String {
        self.__s.clone()
    }
    pub fn capacity(&self) -> usize {
        self.__s.len()
    }
    pub fn clear(&mut self) {
        self.__s = String::new();
    }
    pub fn reserve(&mut self, _additional: usize) {}
    pub fn shrink_to_fit(&mut self) {}
    pub fn has_root(&self) -> bool {
        self.__s.starts_with('/')
    }
    pub fn is_absolute(&self) -> bool {
        self.has_root()
    }
    pub fn is_relative(&self) -> bool {
        !self.has_root()
    }
    pub fn exists(&self) -> bool {
        false
    }
    pub fn try_exists(&self) -> Result<bool, String> {
        Ok(false)
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
    /// Its parts: a leading `/`, a leading `.`, `..` and names (empty parts and other `.`s are
    /// skipped).
    pub fn components(&self) -> std::vec::IntoIter<Component> {
        self.__components().into_iter()
    }
    fn __components(&self) -> Vec<Component> {
        let mut out = Vec::new();
        if self.has_root() {
            out.push(Component::RootDir);
        }
        let mut first = true;
        for part in self.__s.split('/') {
            if part.is_empty() {
                continue;
            }
            if part == "." {
                if first && !self.has_root() && self.__s.starts_with('.') {
                    out.push(Component::CurDir);
                }
                first = false;
                continue;
            }
            first = false;
            if part == ".." {
                out.push(Component::ParentDir);
            } else {
                out.push(Component::Normal(part.to_string()));
            }
        }
        out
    }
    /// The parts as strings (`/` for the root).
    pub fn iter(&self) -> std::vec::IntoIter<String> {
        let parts: Vec<String> = self.__components().iter().map(|c| c.as_os_str()).collect();
        parts.into_iter()
    }
    fn __from_components(parts: &[Component]) -> PathBuf {
        let mut s = String::new();
        for (i, c) in parts.iter().enumerate() {
            match c {
                Component::RootDir => s.push('/'),
                other => {
                    if i > 0 && !s.ends_with('/') {
                        s.push('/');
                    }
                    s.push_str(&other.as_os_str());
                }
            }
        }
        PathBuf { __s: s }
    }
    pub fn file_name(&self) -> Option<String> {
        match self.__components().last() {
            Some(Component::Normal(n)) => Some(n.clone()),
            _ => None,
        }
    }
    pub fn parent(&self) -> Option<PathBuf> {
        let mut parts = self.__components();
        match parts.pop() {
            Some(Component::Normal(_)) | Some(Component::CurDir) | Some(Component::ParentDir) => {
                Some(PathBuf::__from_components(&parts))
            }
            _ => None,
        }
    }
    /// The file name before its last `.` (all of `.bashrc`), and after it.
    fn __split_name(&self) -> Option<(String, Option<String>)> {
        let name = self.file_name()?;
        if name == ".." {
            return Some((name, None));
        }
        match name.rfind('.') {
            Some(0) | None => Some((name, None)),
            Some(i) => Some((name[..i].to_string(), Some(name[i + 1..].to_string()))),
        }
    }
    pub fn file_stem(&self) -> Option<String> {
        self.__split_name().map(|(stem, _)| stem)
    }
    pub fn extension(&self) -> Option<String> {
        self.__split_name().and_then(|(_, ext)| ext)
    }
    pub fn join<P: __AsPath>(&self, p: P) -> PathBuf {
        let mut out = self.clone();
        out.push(p);
        out
    }
    pub fn push<P: __AsPath>(&mut self, p: P) {
        let s = p.__path_str();
        if s.starts_with('/') {
            self.__s = s;
            return;
        }
        if !self.__s.is_empty() && !self.__s.ends_with('/') {
            self.__s.push('/');
        }
        self.__s.push_str(&s);
    }
    pub fn pop(&mut self) -> bool {
        match self.parent() {
            Some(p) => {
                self.__s = p.__s;
                true
            }
            None => false,
        }
    }
    pub fn set_file_name<P: __AsPath>(&mut self, name: P) {
        if self.file_name().is_some() {
            self.pop();
        }
        self.push(name);
    }
    pub fn with_file_name<P: __AsPath>(&self, name: P) -> PathBuf {
        let mut out = self.clone();
        out.set_file_name(name);
        out
    }
    pub fn set_extension<P: __AsPath>(&mut self, ext: P) -> bool {
        let Some((stem, _)) = self.__split_name() else {
            return false;
        };
        let name = self.file_name().unwrap_or_default();
        let mut s = self.__s.trim_end_matches('/').to_string();
        s.truncate(s.len() - name.len());
        s.push_str(&stem);
        let ext = ext.__path_str();
        if !ext.is_empty() {
            s.push('.');
            s.push_str(&ext);
        }
        self.__s = s;
        true
    }
    pub fn with_extension<P: __AsPath>(&self, ext: P) -> PathBuf {
        let mut out = self.clone();
        out.set_extension(ext);
        out
    }
    pub fn starts_with<P: __AsPath>(&self, base: P) -> bool {
        let base = PathBuf::__new(base).__components();
        let parts = self.__components();
        base.len() <= parts.len() && parts[..base.len()] == base[..]
    }
    pub fn ends_with<P: __AsPath>(&self, child: P) -> bool {
        let child = PathBuf::__new(child).__components();
        let parts = self.__components();
        child.len() <= parts.len() && parts[parts.len() - child.len()..] == child[..]
    }
    pub fn strip_prefix<P: __AsPath>(&self, base: P) -> Result<PathBuf, StripPrefixError> {
        let base = PathBuf::__new(base).__components();
        let parts = self.__components();
        if base.len() <= parts.len() && parts[..base.len()] == base[..] {
            Ok(PathBuf::__from_components(&parts[base.len()..]))
        } else {
            Err(StripPrefixError {})
        }
    }
}
