// The bundled `clap`: `#[derive(Parser)]` with options, flags, defaults, a `ValueEnum`, a
// positional `Vec`, subcommands; its errors and help as clap writes them; `parse()` on the
// program's own arguments, which lack `--name`, so it exits with code 2.
use clap::{Parser, Subcommand, ValueEnum};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long)]
    name: String,

    /// Number of times to greet
    #[arg(short, long, default_value_t = 1)]
    count: u8,

    /// Be loud
    #[arg(short, long)]
    verbose: bool,

    #[arg(long, value_enum, default_value_t = Mode::Fast)]
    mode: Mode,

    /// Input files
    files: Vec<String>,

    #[arg(long)]
    limit: Option<u32>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Mode {
    /// Go fast
    Fast,
    Slow,
    ReallySlow,
}

#[derive(Parser, Debug)]
#[command(name = "git2")]
struct Git {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Add a file
    Add { path: String },
    /// Commit things
    Commit {
        #[arg(short, long)]
        message: Option<String>,
        #[arg(short = 'a', long = "all")]
        all: bool,
    },
}

fn show<T: std::fmt::Debug>(r: Result<T, clap::Error>) {
    match r {
        Ok(a) => println!("OK {:?}", a),
        Err(e) => println!("ERR {:?}\n{}----", e.kind(), e),
    }
}

fn main() {
    show(Args::try_parse_from(["prog", "--name", "Bob", "-c", "3", "a.txt", "b.txt"]));
    show(Args::try_parse_from(["prog", "-n=Al", "-vc5", "--mode", "really-slow", "--limit=4"]));
    show(Args::try_parse_from(["prog"]));
    show(Args::try_parse_from(["prog", "--name", "x", "--count", "abc"]));
    show(Args::try_parse_from(["prog", "--name", "x", "--bogus"]));
    show(Args::try_parse_from(["prog", "--name", "x", "--mode", "medium"]));
    show(Args::try_parse_from(["prog", "--name"]));
    show(Args::try_parse_from(["prog", "--help"]));
    show(Args::try_parse_from(["prog", "--version"]));
    show(Args::try_parse_from(["prog", "-h"]));
    show(Git::try_parse_from(["git2", "add", "x.rs"]));
    show(Git::try_parse_from(["git2", "commit", "-m", "hi", "--all"]));
    show(Git::try_parse_from(["git2"]));
    show(Git::try_parse_from(["git2", "--help"]));
    show(Git::try_parse_from(["git2", "push"]));
    show(Git::try_parse_from(["git2", "commit", "--help"]));
    let args = Args::parse();
    println!("unreachable {:?}", args);
}
