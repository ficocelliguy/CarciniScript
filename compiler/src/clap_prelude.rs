// (clap prelude)
// The bundled `clap`: the derives (`Parser`, `Args`, `Subcommand`, `ValueEnum`, written out by
// the pre-pass as calls into this) and the builder (`Command`, `Arg`, `ArgAction`,
// `ArgMatches`), with clap's parsing rules, help, usage and error messages.

/// `clap::error::ErrorKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClapErrorKind {
    InvalidValue,
    UnknownArgument,
    InvalidSubcommand,
    NoEquals,
    ValueValidation,
    TooManyValues,
    TooFewValues,
    WrongNumberOfValues,
    ArgumentConflict,
    MissingRequiredArgument,
    MissingSubcommand,
    InvalidUtf8,
    DisplayHelp,
    DisplayHelpOnMissingArgumentOrSubcommand,
    DisplayVersion,
    Io,
    Format,
}

/// `clap::Error`: its kind and the whole message clap would print.
#[derive(Clone, Debug)]
pub struct ClapError {
    kind: ClapErrorKind,
    message: String,
}

impl ClapError {
    pub fn kind(&self) -> ClapErrorKind {
        self.kind
    }
    fn __to_stdout(&self) -> bool {
        matches!(self.kind, ClapErrorKind::DisplayHelp | ClapErrorKind::DisplayVersion)
    }
    pub fn exit_code(&self) -> i32 {
        if self.__to_stdout() {
            0
        } else {
            2
        }
    }
    pub fn use_stderr(&self) -> bool {
        !self.__to_stdout()
    }
    pub fn render(&self) -> String {
        self.message.clone()
    }
    pub fn print(&self) -> Result<(), String> {
        if self.__to_stdout() {
            print!("{}", self.message);
        } else {
            eprint!("{}", self.message);
        }
        Ok(())
    }
    pub fn exit(&self) -> ! {
        js::__exit_with(self.exit_code(), &self.message, self.__to_stdout())
    }
    pub fn raw(kind: ClapErrorKind, message: &str) -> ClapError {
        ClapError { kind, message: format!("error: {}", message) }
    }
    fn __plain(kind: ClapErrorKind, body: String) -> ClapError {
        ClapError {
            kind,
            message: format!("error: {}\n\nFor more information, try '--help'.\n", body),
        }
    }
    fn __with_usage(kind: ClapErrorKind, body: String, usage: String) -> ClapError {
        ClapError {
            kind,
            message: format!(
                "error: {}\n\nUsage: {}\n\nFor more information, try '--help'.\n",
                body, usage
            ),
        }
    }
}

impl std::fmt::Display for ClapError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ClapError {}

/// `clap::ArgAction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClapAction {
    Set,
    Append,
    SetTrue,
    SetFalse,
    Count,
    Help,
    HelpShort,
    HelpLong,
    Version,
}

/// `clap::value_parser!(T)`: the type values are checked as.
#[derive(Clone, Debug, PartialEq)]
pub struct ClapValueParser {
    kind: String,
}

impl ClapValueParser {
    pub fn __of(kind: &str) -> ClapValueParser {
        ClapValueParser { kind: kind.to_string() }
    }
    pub fn range<R>(self, _r: R) -> ClapValueParser {
        self
    }
}

/// `clap::builder::PossibleValue`.
#[derive(Clone, Debug, PartialEq)]
pub struct ClapPossibleValue {
    name: String,
    help: Option<String>,
}

impl ClapPossibleValue {
    pub fn new(name: &str) -> ClapPossibleValue {
        ClapPossibleValue { name: name.to_string(), help: None }
    }
    pub fn help(mut self, h: &str) -> ClapPossibleValue {
        self.help = Some(h.to_string());
        self
    }
    pub fn get_name(&self) -> &str {
        &self.name
    }
}

/// `clap::Arg`.
#[derive(Clone, Debug)]
pub struct ClapArg {
    id: String,
    short: Option<char>,
    long: Option<String>,
    help: Option<String>,
    long_help: Option<String>,
    value_name: Option<String>,
    required: bool,
    action: Option<ClapAction>,
    default: Option<String>,
    possible: Vec<ClapPossibleValue>,
    kind: String,
    aliases: Vec<String>,
    hide: bool,
    index: Option<usize>,
}

impl ClapArg {
    pub fn new(id: &str) -> ClapArg {
        ClapArg {
            id: id.to_string(),
            short: None,
            long: None,
            help: None,
            long_help: None,
            value_name: None,
            required: false,
            action: None,
            default: None,
            possible: Vec::new(),
            kind: String::new(),
            aliases: Vec::new(),
            hide: false,
            index: None,
        }
    }
    pub fn short(mut self, c: char) -> ClapArg {
        self.short = Some(c);
        self
    }
    pub fn long(mut self, l: &str) -> ClapArg {
        self.long = Some(l.to_string());
        self
    }
    pub fn help(mut self, h: &str) -> ClapArg {
        self.help = Some(h.to_string());
        self
    }
    pub fn long_help(mut self, h: &str) -> ClapArg {
        self.long_help = Some(h.to_string());
        self
    }
    pub fn value_name(mut self, v: &str) -> ClapArg {
        self.value_name = Some(v.to_string());
        self
    }
    pub fn required(mut self, r: bool) -> ClapArg {
        self.required = r;
        self
    }
    pub fn action(mut self, a: ClapAction) -> ClapArg {
        self.action = Some(a);
        self
    }
    pub fn default_value(mut self, v: &str) -> ClapArg {
        self.default = Some(v.to_string());
        self
    }
    pub fn value_parser(mut self, p: ClapValueParser) -> ClapArg {
        self.kind = p.kind;
        self
    }
    pub fn value_parser_values(mut self, values: Vec<ClapPossibleValue>) -> ClapArg {
        self.possible = values;
        self
    }
    pub fn alias(mut self, a: &str) -> ClapArg {
        self.aliases.push(a.to_string());
        self
    }
    pub fn visible_alias(self, a: &str) -> ClapArg {
        self.alias(a)
    }
    pub fn hide(mut self, h: bool) -> ClapArg {
        self.hide = h;
        self
    }
    pub fn index(mut self, i: usize) -> ClapArg {
        self.index = Some(i);
        self
    }
    pub fn num_args<R>(self, _r: R) -> ClapArg {
        self
    }
    pub fn global(self, _g: bool) -> ClapArg {
        self
    }
    pub fn get_id(&self) -> &str {
        &self.id
    }
    fn __action(&self) -> ClapAction {
        match self.action {
            Some(a) => a,
            None => ClapAction::Set,
        }
    }
    fn __takes_value(&self) -> bool {
        matches!(self.__action(), ClapAction::Set | ClapAction::Append)
    }
    fn __positional(&self) -> bool {
        self.short.is_none() && self.long.is_none()
    }
    fn __value_name(&self) -> String {
        match &self.value_name {
            Some(v) => v.clone(),
            None => self.id.clone(),
        }
    }
    fn __multiple(&self) -> bool {
        self.__action() == ClapAction::Append
    }
    /// How errors and usage name it: `--name <NAME>`, `-n <NAME>`, `--verbose`, `<FILE>`,
    /// `[FILES]...`.
    fn __display(&self) -> String {
        if self.__positional() {
            let v = self.__value_name();
            let s = if self.required { format!("<{}>", v) } else { format!("[{}]", v) };
            return if self.__multiple() { format!("{}...", s) } else { s };
        }
        let flag = match (&self.long, self.short) {
            (Some(l), _) => format!("--{}", l),
            (None, Some(c)) => format!("-{}", c),
            _ => String::new(),
        };
        if self.__takes_value() {
            format!("{} <{}>", flag, self.__value_name())
        } else {
            flag
        }
    }
    /// The left column of help: `-n, --name <NAME>`, `    --mode <MODE>`, `-v, --verbose...`.
    fn __help_left(&self) -> String {
        if self.__positional() {
            return self.__display();
        }
        let mut s = match (self.short, &self.long) {
            (Some(c), Some(l)) => format!("-{}, --{}", c, l),
            (Some(c), None) => format!("-{}", c),
            (None, Some(l)) => format!("    --{}", l),
            _ => String::new(),
        };
        if self.__takes_value() {
            s.push_str(&format!(" <{}>", self.__value_name()));
        } else if self.__action() == ClapAction::Count {
            s.push_str("...");
        }
        s
    }
    /// `[default: 1] [possible values: a, b]` after the help, as the short help shows them.
    fn __help_short(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(h) = &self.help {
            parts.push(h.clone());
        }
        if self.__takes_value()
            && let Some(d) = &self.default
        {
            parts.push(format!("[default: {}]", d));
        }
        if !self.possible.is_empty() {
            let names: Vec<String> = self.possible.iter().map(|p| p.name.clone()).collect();
            parts.push(format!("[possible values: {}]", names.join(", ")));
        }
        parts.join(" ")
    }
    fn __has_long(&self) -> bool {
        self.long_help.is_some() || self.possible.iter().any(|p| p.help.is_some())
    }
    /// The lines under it in the long help (without their indent).
    fn __help_long(&self) -> Vec<String> {
        let mut blocks: Vec<Vec<String>> = Vec::new();
        let text = match (&self.long_help, &self.help) {
            (Some(l), _) => Some(l.clone()),
            (None, Some(h)) => Some(h.clone()),
            _ => None,
        };
        if let Some(t) = text {
            blocks.push(t.split('\n').map(|l| l.to_string()).collect());
        }
        if !self.possible.is_empty() {
            if self.possible.iter().any(|p| p.help.is_some()) {
                let w = self.possible.iter().map(|p| p.name.len()).max().unwrap_or(0) + 2;
                let mut lines = vec!["Possible values:".to_string()];
                for p in &self.possible {
                    match &p.help {
                        Some(h) => lines.push(format!("- {:<w$}{}", format!("{}:", p.name), h, w = w)),
                        None => lines.push(format!("- {}", p.name)),
                    }
                }
                blocks.push(lines);
            } else {
                let names: Vec<String> = self.possible.iter().map(|p| p.name.clone()).collect();
                blocks.push(vec![format!("[possible values: {}]", names.join(", "))]);
            }
        }
        if self.__takes_value()
            && let Some(d) = &self.default
        {
            blocks.push(vec![format!("[default: {}]", d)]);
        }
        let mut out: Vec<String> = Vec::new();
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                out.push(String::new());
            }
            for l in b {
                out.push(l.clone());
            }
        }
        if out.is_empty() {
            out.push(String::new());
        }
        out
    }
}

/// `clap::Command`.
#[derive(Clone, Debug)]
pub struct ClapCommand {
    name: String,
    bin: Option<String>,
    about: Option<String>,
    long_about: Option<String>,
    version: Option<String>,
    args: Vec<ClapArg>,
    subs: Vec<ClapCommand>,
    sub_required: bool,
    else_help: bool,
}

/// Jaro similarity, as clap's suggestions use it.
fn __clap_jaro(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let window = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut b_used = vec![false; b.len()];
    let mut a_matched: Vec<char> = Vec::new();
    for (i, ca) in a.iter().enumerate() {
        let lo = i.saturating_sub(window);
        let hi = (i + window + 1).min(b.len());
        for j in lo..hi {
            if !b_used[j] && b[j] == *ca {
                b_used[j] = true;
                a_matched.push(*ca);
                break;
            }
        }
    }
    let m = a_matched.len();
    if m == 0 {
        return 0.0;
    }
    let b_matched: Vec<char> = b.iter().zip(b_used.iter()).filter(|(_, u)| **u).map(|(c, _)| *c).collect();
    let mut t = 0;
    for k in 0..m {
        if a_matched[k] != b_matched[k] {
            t += 1;
        }
    }
    let m = m as f64;
    (m / a.len() as f64 + m / b.len() as f64 + (m - (t / 2) as f64) / m) / 3.0
}

/// The most similar candidate above clap's 0.7, if any.
fn __clap_similar(v: &str, candidates: &[String]) -> Option<String> {
    let mut best: Option<(f64, String)> = None;
    for c in candidates {
        let s = __clap_jaro(v, c);
        if s > 0.7 && best.as_ref().is_none_or(|(b, _)| s > *b) {
            best = Some((s, c.clone()));
        }
    }
    best.map(|(_, c)| c)
}

/// `dir/prog.exe` → `prog.exe` (clap shows the file name).
fn __clap_base_name(path: &str) -> String {
    let base = path.rsplit(|c: char| c == '/' || c == '\\').next().unwrap_or(path);
    base.to_string()
}

/// Whether `v` parses as the type a value parser names.
fn __clap_check(kind: &str, v: &str) -> Result<(), String> {
    let r = match kind {
        "u8" => v.parse::<u8>().map(|_| ()).map_err(|e| e.to_string()),
        "u16" => v.parse::<u16>().map(|_| ()).map_err(|e| e.to_string()),
        "u32" => v.parse::<u32>().map(|_| ()).map_err(|e| e.to_string()),
        "u64" => v.parse::<u64>().map(|_| ()).map_err(|e| e.to_string()),
        "usize" => v.parse::<usize>().map(|_| ()).map_err(|e| e.to_string()),
        "i8" => v.parse::<i8>().map(|_| ()).map_err(|e| e.to_string()),
        "i16" => v.parse::<i16>().map(|_| ()).map_err(|e| e.to_string()),
        "i32" => v.parse::<i32>().map(|_| ()).map_err(|e| e.to_string()),
        "i64" => v.parse::<i64>().map(|_| ()).map_err(|e| e.to_string()),
        "isize" => v.parse::<isize>().map(|_| ()).map_err(|e| e.to_string()),
        "f32" => v.parse::<f32>().map(|_| ()).map_err(|e| e.to_string()),
        "f64" => v.parse::<f64>().map(|_| ()).map_err(|e| e.to_string()),
        "char" => v.parse::<char>().map(|_| ()).map_err(|e| e.to_string()),
        "bool" => v.parse::<bool>().map(|_| ()).map_err(|e| e.to_string()),
        _ => Ok(()),
    };
    r
}

impl ClapCommand {
    pub fn new(name: &str) -> ClapCommand {
        ClapCommand {
            name: name.to_string(),
            bin: None,
            about: None,
            long_about: None,
            version: None,
            args: Vec::new(),
            subs: Vec::new(),
            sub_required: false,
            else_help: false,
        }
    }
    pub fn name(mut self, n: &str) -> ClapCommand {
        self.name = n.to_string();
        self
    }
    pub fn bin_name(mut self, n: &str) -> ClapCommand {
        self.bin = Some(n.to_string());
        self
    }
    pub fn about(mut self, a: &str) -> ClapCommand {
        self.about = Some(a.to_string());
        self
    }
    pub fn long_about(mut self, a: &str) -> ClapCommand {
        self.long_about = Some(a.to_string());
        self
    }
    pub fn version(mut self, v: &str) -> ClapCommand {
        self.version = Some(v.to_string());
        self
    }
    pub fn author(self, _a: &str) -> ClapCommand {
        self
    }
    pub fn arg(mut self, a: ClapArg) -> ClapCommand {
        self.args.push(a);
        self
    }
    pub fn args(mut self, args: Vec<ClapArg>) -> ClapCommand {
        for a in args {
            self.args.push(a);
        }
        self
    }
    pub fn subcommand(mut self, c: ClapCommand) -> ClapCommand {
        self.subs.push(c);
        self
    }
    pub fn subcommands(mut self, cs: Vec<ClapCommand>) -> ClapCommand {
        for c in cs {
            self.subs.push(c);
        }
        self
    }
    pub fn subcommand_required(mut self, r: bool) -> ClapCommand {
        self.sub_required = r;
        self
    }
    pub fn arg_required_else_help(mut self, r: bool) -> ClapCommand {
        self.else_help = r;
        self
    }
    pub fn propagate_version(self, _p: bool) -> ClapCommand {
        self
    }
    pub fn get_name(&self) -> &str {
        &self.name
    }
    pub fn get_about(&self) -> Option<String> {
        self.about.clone()
    }
    pub fn get_version(&self) -> Option<String> {
        self.version.clone()
    }
    pub fn debug_assert(&self) {}

    fn __bin(&self) -> String {
        match &self.bin {
            Some(b) => b.clone(),
            None => self.__name(),
        }
    }
    /// Its name, else the program's (the first of `std::env::args()`, as a file name).
    fn __name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        let first = std::env::args().next().unwrap_or_default();
        __clap_base_name(&first)
    }
    fn __options(&self) -> Vec<ClapArg> {
        self.args.iter().filter(|a| !a.__positional() && !a.hide).cloned().collect()
    }
    fn __positionals(&self) -> Vec<ClapArg> {
        self.args.iter().filter(|a| a.__positional()).cloned().collect()
    }
    /// The usage line: `prog [OPTIONS] --name <NAME> [FILES]... <COMMAND>` (`full`), or the
    /// short one some errors show, without `[OPTIONS]` and the subcommand.
    fn __usage(&self, full: bool) -> String {
        let mut s = self.__bin();
        if full && self.__options().iter().any(|a| !a.required) {
            s.push_str(" [OPTIONS]");
        }
        for a in self.__options().iter().filter(|a| a.required) {
            s.push(' ');
            s.push_str(&a.__display());
        }
        for a in self.__positionals() {
            s.push(' ');
            s.push_str(&a.__display());
        }
        if full && !self.subs.is_empty() {
            s.push_str(if self.sub_required { " <COMMAND>" } else { " [COMMAND]" });
        }
        s
    }
    /// clap's "smart" usage for some errors: the required options, the given ones, the
    /// positionals (and a required subcommand).
    fn __usage_used(&self, used: &[String]) -> String {
        let mut s = self.__bin();
        for a in self.__options().iter().filter(|a| a.required || used.contains(&a.id)) {
            s.push(' ');
            s.push_str(&a.__display());
        }
        for a in self.__positionals() {
            s.push(' ');
            s.push_str(&a.__display());
        }
        if self.sub_required && !self.subs.is_empty() {
            s.push_str(" <COMMAND>");
        }
        s
    }
    fn __help_arg(&self) -> ClapArg {
        ClapArg::new("help").short('h').long("help").action(ClapAction::Help)
    }
    fn __version_arg(&self) -> ClapArg {
        ClapArg::new("version").short('V').long("version").action(ClapAction::Version)
    }
    fn __needs_long(&self) -> bool {
        self.long_about.is_some() || self.args.iter().any(|a| a.__has_long())
    }
    fn __help(&self, long: bool) -> String {
        let long = long && self.__needs_long();
        let differs = self.__needs_long();
        let mut out = String::new();
        let about = if long {
            match &self.long_about {
                Some(l) => Some(l.clone()),
                None => self.about.clone(),
            }
        } else {
            self.about.clone()
        };
        if let Some(a) = about {
            out.push_str(&a);
            out.push_str("\n\n");
        }
        out.push_str(&format!("Usage: {}\n", self.__usage(true)));
        // (section header, [(left, short help, long lines)])
        let mut sections: Vec<(String, Vec<(String, String, Vec<String>)>)> = Vec::new();
        if !self.subs.is_empty() {
            let mut items = Vec::new();
            for c in &self.subs {
                let about = match &c.about {
                    Some(a) => a.clone(),
                    None => String::new(),
                };
                items.push((c.name.clone(), about.clone(), vec![about]));
            }
            let h = "Print this message or the help of the given subcommand(s)".to_string();
            items.push(("help".to_string(), h.clone(), vec![h]));
            sections.push(("Commands:".to_string(), items));
        }
        let pos = self.__positionals();
        if !pos.is_empty() {
            let items = pos.iter().map(|a| (a.__help_left(), a.__help_short(), a.__help_long())).collect();
            sections.push(("Arguments:".to_string(), items));
        }
        let mut opts: Vec<(String, String, Vec<String>)> = self
            .__options()
            .iter()
            .map(|a| (a.__help_left(), a.__help_short(), a.__help_long()))
            .collect();
        let help_text = if !differs {
            "Print help"
        } else if long {
            "Print help (see a summary with '-h')"
        } else {
            "Print help (see more with '--help')"
        };
        opts.push((self.__help_arg().__help_left(), help_text.to_string(), vec![help_text.to_string()]));
        if self.version.is_some() {
            let v = "Print version".to_string();
            opts.push((self.__version_arg().__help_left(), v.clone(), vec![v]));
        }
        sections.push(("Options:".to_string(), opts));
        if long {
            let total: usize = sections.iter().map(|(_, items)| items.len()).sum();
            let mut n = 0;
            for (title, items) in &sections {
                // (Between sections, the blank line after the last item is the separator.)
                if n == 0 {
                    out.push('\n');
                }
                out.push_str(title);
                out.push('\n');
                for (left, _, lines) in items {
                    n += 1;
                    out.push_str(&format!("  {}\n", left));
                    for l in lines {
                        out.push_str(&format!("          {}\n", l));
                    }
                    if n < total {
                        out.push('\n');
                    }
                }
                let _ = n;
            }
        } else {
            for (title, items) in &sections {
                out.push('\n');
                out.push_str(title);
                out.push('\n');
                let w = items.iter().map(|(l, _, _)| l.chars().count()).max().unwrap_or(0);
                for (left, help, _) in items {
                    out.push_str(&format!("  {:<w$}  {}\n", left, help, w = w));
                }
            }
        }
        out
    }
    pub fn render_help(&self) -> String {
        self.__help(false)
    }
    pub fn render_long_help(&self) -> String {
        self.__help(true)
    }
    pub fn render_usage(&self) -> String {
        format!("Usage: {}", self.__usage(true))
    }
    pub fn render_version(&self) -> String {
        format!("{} {}\n", self.__name(), self.version.clone().unwrap_or_default())
    }
    pub fn print_help(&self) -> Result<(), String> {
        print!("{}", self.__help(false));
        Ok(())
    }
    pub fn print_long_help(&self) -> Result<(), String> {
        print!("{}", self.__help(true));
        Ok(())
    }
    pub fn get_matches(self) -> ClapMatches {
        let args: Vec<String> = std::env::args().collect();
        self.get_matches_from(args)
    }
    pub fn try_get_matches(self) -> Result<ClapMatches, ClapError> {
        let args: Vec<String> = std::env::args().collect();
        self.try_get_matches_from(args)
    }
    pub fn get_matches_from<I>(self, args: I) -> ClapMatches
    where
        I: IntoIterator,
        I::Item: ToString,
    {
        match self.try_get_matches_from(args) {
            Ok(m) => m,
            Err(e) => e.exit(),
        }
    }
    pub fn try_get_matches_from<I>(self, args: I) -> Result<ClapMatches, ClapError>
    where
        I: IntoIterator,
        I::Item: ToString,
    {
        let args: Vec<String> = args.into_iter().map(|a| a.to_string()).collect();
        let mut c = self;
        if c.bin.is_none()
            && let Some(first) = args.first()
        {
            let base = __clap_base_name(first);
            c.bin = Some(base.clone());
            if c.name.is_empty() {
                c.name = base;
            }
        }
        let rest: Vec<String> = args.into_iter().skip(1).collect();
        c.__parse(&rest)
    }

    fn __find_long(&self, l: &str) -> Option<ClapArg> {
        self.args
            .iter()
            .find(|a| a.long.as_deref() == Some(l) || a.aliases.iter().any(|x| x == l))
            .cloned()
    }
    fn __find_short(&self, c: char) -> Option<ClapArg> {
        self.args.iter().find(|a| a.short == Some(c)).cloned()
    }
    /// An unknown flag. For a `--long` one, a similar argument is suggested and the usage is the
    /// smart one when any were given (the suggested one counts); otherwise the full usage.
    fn __unknown(&self, arg: &str, m: &ClapMatches) -> ClapError {
        let mut used: Vec<String> = m.vals.iter().map(|(k, _)| k.clone()).collect();
        let similar = match arg.strip_prefix("--") {
            Some(l) => {
                let mut all: Vec<String> = self.args.iter().filter_map(|a| a.long.clone()).collect();
                all.push("help".to_string());
                if self.version.is_some() {
                    all.push("version".to_string());
                }
                __clap_similar(l, &all)
            }
            None => None,
        };
        if let Some(s) = &similar
            && let Some(a) = self.__find_long(s)
            && !used.contains(&a.id)
        {
            used.push(a.id.clone());
        }
        let mut body = format!("unexpected argument '{}' found", arg);
        if let Some(s) = &similar {
            body.push_str(&format!("\n\n  tip: a similar argument exists: '--{}'", s));
        } else if arg.starts_with('-') && !self.__positionals().is_empty() {
            body.push_str(&format!("\n\n  tip: to pass '{}' as a value, use '-- {}'", arg, arg));
        }
        let usage = if arg.starts_with("--") && !used.is_empty() {
            self.__usage_used(&used)
        } else {
            self.__usage(true)
        };
        ClapError::__with_usage(ClapErrorKind::UnknownArgument, body, usage)
    }
    fn __set(&self, m: &mut ClapMatches, a: &ClapArg, v: String) -> Result<(), ClapError> {
        if !a.possible.is_empty() && !a.possible.iter().any(|p| p.name == v) {
            let names: Vec<String> = a.possible.iter().map(|p| p.name.clone()).collect();
            let mut body = format!(
                "invalid value '{}' for '{}'\n  [possible values: {}]",
                v,
                a.__display(),
                names.join(", ")
            );
            if let Some(s) = __clap_similar(&v, &names) {
                body.push_str(&format!("\n\n  tip: a similar value exists: '{}'", s));
            }
            return Err(ClapError::__plain(ClapErrorKind::InvalidValue, body));
        }
        if let Err(e) = __clap_check(&a.kind, &v) {
            return Err(ClapError::__plain(
                ClapErrorKind::ValueValidation,
                format!("invalid value '{}' for '{}': {}", v, a.__display(), e),
            ));
        }
        if a.__action() == ClapAction::Set && m.__has(&a.id) {
            return Err(ClapError::__with_usage(
                ClapErrorKind::ArgumentConflict,
                format!("the argument '{}' cannot be used multiple times", a.__display()),
                self.__usage(true),
            ));
        }
        m.__push(&a.id, v);
        Ok(())
    }
    fn __flag(&self, m: &mut ClapMatches, a: &ClapArg) -> Result<(), ClapError> {
        match a.__action() {
            ClapAction::Count => {
                let n = m.get_count(&a.id);
                m.__set_one(&a.id, (n + 1).to_string());
            }
            ClapAction::SetFalse => m.__set_one(&a.id, "false".to_string()),
            _ => {
                if m.__has(&a.id) {
                    return Err(ClapError::__with_usage(
                        ClapErrorKind::ArgumentConflict,
                        format!("the argument '{}' cannot be used multiple times", a.__display()),
                        self.__usage(true),
                    ));
                }
                m.__set_one(&a.id, "true".to_string());
            }
        }
        Ok(())
    }
    /// An option that wants a value, followed by `next` that looks like a flag: clap's error
    /// for an unknown flag, else that the value is missing.
    fn __flag_after(&self, a: &ClapArg, next: &str, m: &ClapMatches) -> ClapError {
        let known = match next.strip_prefix("--") {
            Some(l) => {
                let l = l.split('=').next().unwrap_or(l);
                self.__find_long(l).is_some() || l == "help" || (l == "version" && self.version.is_some())
            }
            None => match next[1..].chars().next() {
                Some(c) => self.__find_short(c).is_some() || c == 'h' || (c == 'V' && self.version.is_some()),
                None => true,
            },
        };
        if known {
            self.__missing_value(a)
        } else {
            self.__unknown(next, m)
        }
    }
    fn __missing_value(&self, a: &ClapArg) -> ClapError {
        ClapError::__plain(
            ClapErrorKind::InvalidValue,
            format!("a value is required for '{}' but none was supplied", a.__display()),
        )
    }
    fn __help_error(&self, long: bool) -> ClapError {
        ClapError { kind: ClapErrorKind::DisplayHelp, message: self.__help(long) }
    }
    fn __parse(&self, args: &[String]) -> Result<ClapMatches, ClapError> {
        let mut m = ClapMatches::default();
        let positionals = self.__positionals();
        let mut pos_i = 0;
        let mut only_values = false;
        let mut i = 0;
        if args.is_empty() && (self.else_help || (self.sub_required && self.args.iter().all(|a| !a.required))) && !self.subs.is_empty() {
            return Err(ClapError {
                kind: ClapErrorKind::DisplayHelpOnMissingArgumentOrSubcommand,
                message: self.__help(false),
            });
        }
        while i < args.len() {
            let arg = args[i].clone();
            i += 1;
            if !only_values && arg == "--" {
                only_values = true;
                continue;
            }
            if !only_values && arg.starts_with("--") {
                let (name, inline) = match arg[2..].split_once('=') {
                    Some((n, v)) => (n.to_string(), Some(v.to_string())),
                    None => (arg[2..].to_string(), None),
                };
                let Some(a) = self.__find_long(&name) else {
                    if name == "help" {
                        return Err(self.__help_error(true));
                    }
                    if name == "version" && self.version.is_some() {
                        return Err(ClapError { kind: ClapErrorKind::DisplayVersion, message: self.render_version() });
                    }
                    return Err(self.__unknown(&arg, &m));
                };
                if a.__takes_value() {
                    let v = match inline {
                        Some(v) => v,
                        None => {
                            if i < args.len() && !(args[i].starts_with('-') && args[i].len() > 1) {
                                i += 1;
                                args[i - 1].clone()
                            } else if i < args.len() {
                                return Err(self.__flag_after(&a, &args[i], &m));
                            } else {
                                return Err(self.__missing_value(&a));
                            }
                        }
                    };
                    self.__set(&mut m, &a, v)?;
                } else {
                    self.__flag(&mut m, &a)?;
                }
                continue;
            }
            if !only_values && arg.starts_with('-') && arg.len() > 1 {
                let chars: Vec<char> = arg[1..].chars().collect();
                let mut k = 0;
                while k < chars.len() {
                    let c = chars[k];
                    k += 1;
                    let Some(a) = self.__find_short(c) else {
                        if c == 'h' {
                            return Err(self.__help_error(false));
                        }
                        if c == 'V' && self.version.is_some() {
                            return Err(ClapError { kind: ClapErrorKind::DisplayVersion, message: self.render_version() });
                        }
                        return Err(self.__unknown(&format!("-{}", c), &m));
                    };
                    if a.__takes_value() {
                        let rest: String = chars[k..].iter().collect();
                        let rest = rest.strip_prefix('=').unwrap_or(&rest).to_string();
                        let v = if !rest.is_empty() {
                            rest
                        } else if i < args.len() && !(args[i].starts_with('-') && args[i].len() > 1) {
                            i += 1;
                            args[i - 1].clone()
                        } else if i < args.len() {
                            return Err(self.__flag_after(&a, &args[i], &m));
                        } else {
                            return Err(self.__missing_value(&a));
                        };
                        self.__set(&mut m, &a, v)?;
                        break;
                    }
                    self.__flag(&mut m, &a)?;
                }
                continue;
            }
            // A subcommand, once the positionals before it are in.
            if !only_values && !self.subs.is_empty() && (pos_i >= positionals.len() || positionals[pos_i..].iter().all(|p| !p.required)) {
                if arg == "help" {
                    if i < args.len()
                        && let Some(sc) = self.subs.iter().find(|s| s.name == args[i])
                    {
                        let mut sc = sc.clone();
                        sc.bin = Some(format!("{} {}", self.__bin(), sc.name));
                        return Err(sc.__help_error(true));
                    }
                    return Err(self.__help_error(true));
                }
                if let Some(sc) = self.subs.iter().find(|s| s.name == arg) {
                    let mut sc = sc.clone();
                    sc.bin = Some(format!("{} {}", self.__bin(), sc.name));
                    let sm = sc.__parse(&args[i..])?;
                    m.sub = Some((sc.name.clone(), Box::new(sm)));
                    break;
                }
                if positionals.is_empty() {
                    return Err(ClapError::__with_usage(
                        ClapErrorKind::InvalidSubcommand,
                        format!("unrecognized subcommand '{}'", arg),
                        self.__usage(true),
                    ));
                }
            }
            if pos_i >= positionals.len() {
                return Err(ClapError::__with_usage(
                    ClapErrorKind::UnknownArgument,
                    format!("unexpected argument '{}' found", arg),
                    self.__usage(true),
                ));
            }
            let p = positionals[pos_i].clone();
            if !p.__multiple() {
                pos_i += 1;
            }
            self.__set(&mut m, &p, arg)?;
        }
        let missing: Vec<ClapArg> = self
            .args
            .iter()
            .filter(|a| a.required && !m.__has(&a.id) && a.default.is_none())
            .cloned()
            .collect();
        if !missing.is_empty() {
            let mut body = "the following required arguments were not provided:".to_string();
            let (opts, pos): (Vec<ClapArg>, Vec<ClapArg>) = missing.iter().cloned().partition(|a| !a.__positional());
            for a in opts.iter().chain(pos.iter()) {
                body.push_str(&format!("\n  {}", a.__display()));
            }
            let used: Vec<String> = m.vals.iter().map(|(k, _)| k.clone()).collect();
            let mut usage = self.__usage_used(&used);
            if self.sub_required && !self.subs.is_empty() {
                usage = usage.trim_end_matches(" <COMMAND>").to_string();
            }
            return Err(ClapError::__with_usage(ClapErrorKind::MissingRequiredArgument, body, usage));
        }
        if self.sub_required && m.sub.is_none() {
            let mut names: Vec<String> = self.subs.iter().map(|s| s.name.clone()).collect();
            names.push("help".to_string());
            return Err(ClapError::__with_usage(
                ClapErrorKind::MissingSubcommand,
                format!(
                    "'{}' requires a subcommand but one was not provided\n  [subcommands: {}]",
                    self.__bin(),
                    names.join(", ")
                ),
                self.__usage(true),
            ));
        }
        for a in &self.args {
            if !m.__has(&a.id) {
                if let Some(d) = &a.default {
                    m.__push(&a.id, d.clone());
                } else if matches!(a.__action(), ClapAction::SetTrue) {
                    m.__set_one(&a.id, "false".to_string());
                } else if matches!(a.__action(), ClapAction::SetFalse) {
                    m.__set_one(&a.id, "true".to_string());
                } else if matches!(a.__action(), ClapAction::Count) {
                    m.__set_one(&a.id, "0".to_string());
                }
            }
        }
        Ok(m)
    }
}

/// `clap::ArgMatches`: each argument's values, as given.
#[derive(Clone, Debug, Default)]
pub struct ClapMatches {
    vals: Vec<(String, Vec<String>)>,
    sub: Option<(String, Box<ClapMatches>)>,
}

impl ClapMatches {
    fn __has(&self, id: &str) -> bool {
        self.vals.iter().any(|(k, _)| k == id)
    }
    fn __push(&mut self, id: &str, v: String) {
        for (k, vs) in self.vals.iter_mut() {
            if k == id {
                vs.push(v);
                return;
            }
        }
        self.vals.push((id.to_string(), vec![v]));
    }
    fn __set_one(&mut self, id: &str, v: String) {
        for (k, vs) in self.vals.iter_mut() {
            if k == id {
                *vs = vec![v];
                return;
            }
        }
        self.vals.push((id.to_string(), vec![v]));
    }
    fn __raw(&self, id: &str) -> Option<Vec<String>> {
        self.vals.iter().find(|(k, _)| k == id).map(|(_, v)| v.clone())
    }
    pub fn get_one<T: std::str::FromStr>(&self, id: &str) -> Option<T> {
        let vs = self.__raw(id)?;
        vs.last()?.parse::<T>().ok()
    }
    pub fn get_many<T: std::str::FromStr>(&self, id: &str) -> Option<std::vec::IntoIter<T>> {
        let vs = self.__raw(id)?;
        let out: Vec<T> = vs.iter().filter_map(|v| v.parse::<T>().ok()).collect();
        Some(out.into_iter())
    }
    pub fn get_flag(&self, id: &str) -> bool {
        match self.__raw(id) {
            Some(v) => v.last().is_some_and(|x| x == "true"),
            None => false,
        }
    }
    pub fn get_count(&self, id: &str) -> u8 {
        match self.__raw(id) {
            Some(v) => v.last().and_then(|x| x.parse::<u8>().ok()).unwrap_or(0),
            None => 0,
        }
    }
    pub fn contains_id(&self, id: &str) -> bool {
        self.__has(id)
    }
    pub fn subcommand(&self) -> Option<(&str, &ClapMatches)> {
        match &self.sub {
            Some((n, m)) => Some((n.as_str(), &**m)),
            None => None,
        }
    }
    pub fn subcommand_name(&self) -> Option<&str> {
        match &self.sub {
            Some((n, _)) => Some(n.as_str()),
            None => None,
        }
    }
    pub fn subcommand_matches(&self, name: &str) -> Option<&ClapMatches> {
        match &self.sub {
            Some((n, m)) if n == name => Some(&**m),
            _ => None,
        }
    }
}

/// A derived field's value: the argument's (last) value as `T`.
fn __clap_one<T: std::str::FromStr>(m: &ClapMatches, id: &str, shown: &str) -> Result<Option<T>, ClapError>
where
    T::Err: std::fmt::Display,
{
    let Some(vs) = m.__raw(id) else {
        return Ok(None);
    };
    let Some(v) = vs.last() else {
        return Ok(None);
    };
    match v.parse::<T>() {
        Ok(x) => Ok(Some(x)),
        Err(e) => Err(ClapError::__plain(
            ClapErrorKind::ValueValidation,
            format!("invalid value '{}' for '{}': {}", v, shown, e),
        )),
    }
}

/// A derived `Vec<T>` field's values.
fn __clap_many<T: std::str::FromStr>(m: &ClapMatches, id: &str, shown: &str) -> Result<Vec<T>, ClapError>
where
    T::Err: std::fmt::Display,
{
    let mut out = Vec::new();
    if let Some(vs) = m.__raw(id) {
        for v in vs {
            match v.parse::<T>() {
                Ok(x) => out.push(x),
                Err(e) => {
                    return Err(ClapError::__plain(
                        ClapErrorKind::ValueValidation,
                        format!("invalid value '{}' for '{}': {}", v, shown, e),
                    ));
                }
            }
        }
    }
    Ok(out)
}

/// `Args::parse()` and friends: the arguments, else the error printed and the process ended.
fn __clap_or_exit<T>(r: Result<T, ClapError>) -> T {
    match r {
        Ok(x) => x,
        Err(e) => e.exit(),
    }
}
