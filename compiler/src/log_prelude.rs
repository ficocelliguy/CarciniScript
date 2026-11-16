// (log prelude)
// The bundled `log` facade and its `env_logger` / `simple_logger` setup: `error!` .. `trace!`
// (bundled macros) print `[LEVEL] message` to stderr once a logger is set up, filtered by its
// level (env_logger's default is `error`); with no logger they print nothing, as with `log` alone.

/// `log::Level`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn __n(&self) -> usize {
        match self {
            LogLevel::Error => 1,
            LogLevel::Warn => 2,
            LogLevel::Info => 3,
            LogLevel::Debug => 4,
            LogLevel::Trace => 5,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }
    pub fn to_level_filter(&self) -> LogLevelFilter {
        match self {
            LogLevel::Error => LogLevelFilter::Error,
            LogLevel::Warn => LogLevelFilter::Warn,
            LogLevel::Info => LogLevelFilter::Info,
            LogLevel::Debug => LogLevelFilter::Debug,
            LogLevel::Trace => LogLevelFilter::Trace,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.pad(self.as_str())
    }
}

/// `log::LevelFilter`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum LogLevelFilter {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevelFilter {
    fn __n(&self) -> usize {
        match self {
            LogLevelFilter::Off => 0,
            LogLevelFilter::Error => 1,
            LogLevelFilter::Warn => 2,
            LogLevelFilter::Info => 3,
            LogLevelFilter::Debug => 4,
            LogLevelFilter::Trace => 5,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevelFilter::Off => "OFF",
            LogLevelFilter::Error => "ERROR",
            LogLevelFilter::Warn => "WARN",
            LogLevelFilter::Info => "INFO",
            LogLevelFilter::Debug => "DEBUG",
            LogLevelFilter::Trace => "TRACE",
        }
    }
}

impl std::fmt::Display for LogLevelFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.pad(self.as_str())
    }
}

// The max level `log` passes on (`set_max_level`), and whether a logger is set up.
static mut __LOG_MAX: usize = 0;
static mut __LOG_ON: bool = false;

pub fn __log_set_max_level(level: LogLevelFilter) {
    unsafe {
        __LOG_MAX = level.__n();
    }
}

pub fn __log_max_level() -> LogLevelFilter {
    match unsafe { __LOG_MAX } {
        0 => LogLevelFilter::Off,
        1 => LogLevelFilter::Error,
        2 => LogLevelFilter::Warn,
        3 => LogLevelFilter::Info,
        4 => LogLevelFilter::Debug,
        _ => LogLevelFilter::Trace,
    }
}

/// What the logging macros call.
pub fn __log_emit(level: LogLevel, msg: &str) {
    if __log_enabled(level) {
        eprintln!("[{}] {}", level.as_str(), msg);
    }
}

pub fn __log_enabled(level: LogLevel) -> bool {
    unsafe { __LOG_ON && level.__n() <= __LOG_MAX }
}

fn __log_start(level: LogLevelFilter) {
    unsafe {
        __LOG_ON = true;
        __LOG_MAX = level.__n();
    }
}

fn __log_parse(spec: &str) -> LogLevelFilter {
    match spec.trim().to_lowercase().as_str() {
        "off" => LogLevelFilter::Off,
        "error" => LogLevelFilter::Error,
        "warn" => LogLevelFilter::Warn,
        "info" => LogLevelFilter::Info,
        "debug" => LogLevelFilter::Debug,
        "trace" => LogLevelFilter::Trace,
        _ => LogLevelFilter::Error,
    }
}

/// `env_logger::init()`: errors only (there's no `RUST_LOG` here).
pub fn __log_env_init() {
    __log_start(LogLevelFilter::Error);
}

pub fn __log_env_try_init() -> Result<(), String> {
    __log_start(LogLevelFilter::Error);
    Ok(())
}

/// `simple_logger::init_with_level(level)`.
pub fn __log_init_with_level(level: LogLevel) -> Result<(), String> {
    __log_start(level.to_level_filter());
    Ok(())
}

/// `env_logger::Env`: only its default filter matters here.
pub struct LogEnv {
    __filter: String,
}

impl LogEnv {
    pub fn new() -> LogEnv {
        LogEnv {
            __filter: "error".to_string(),
        }
    }
    pub fn default() -> LogEnv {
        LogEnv::new()
    }
    pub fn default_filter_or(self, filter: &str) -> LogEnv {
        LogEnv {
            __filter: filter.to_string(),
        }
    }
    pub fn filter(self, _name: &str) -> LogEnv {
        self
    }
    pub fn filter_or(self, _name: &str, filter: &str) -> LogEnv {
        LogEnv {
            __filter: filter.to_string(),
        }
    }
}

/// `env_logger::Builder` (and `simple_logger::SimpleLogger`).
pub struct LogBuilder {
    __level: LogLevelFilter,
}

pub fn __log_builder() -> LogBuilder {
    LogBuilder::new()
}

impl LogBuilder {
    pub fn new() -> LogBuilder {
        LogBuilder {
            __level: LogLevelFilter::Error,
        }
    }
    pub fn from_default_env() -> LogBuilder {
        LogBuilder::new()
    }
    pub fn from_env(env: LogEnv) -> LogBuilder {
        LogBuilder {
            __level: __log_parse(&env.__filter),
        }
    }
    pub fn filter_level(&mut self, level: LogLevelFilter) -> &mut LogBuilder {
        self.__level = level;
        self
    }
    pub fn filter(&mut self, _module: Option<&str>, level: LogLevelFilter) -> &mut LogBuilder {
        self.__level = level;
        self
    }
    pub fn filter_module(&mut self, _module: &str, level: LogLevelFilter) -> &mut LogBuilder {
        self.__level = level;
        self
    }
    pub fn parse_filters(&mut self, filters: &str) -> &mut LogBuilder {
        self.__level = __log_parse(filters);
        self
    }
    pub fn with_level(self, level: LogLevelFilter) -> LogBuilder {
        LogBuilder { __level: level }
    }
    pub fn format_timestamp(&mut self, _ts: Option<i32>) -> &mut LogBuilder {
        self
    }
    pub fn format_target(&mut self, _yes: bool) -> &mut LogBuilder {
        self
    }
    pub fn init(&mut self) {
        __log_start(self.__level);
    }
    pub fn try_init(&mut self) -> Result<(), String> {
        __log_start(self.__level);
        Ok(())
    }
}
