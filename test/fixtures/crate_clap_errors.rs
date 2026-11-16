// More of the bundled `clap`: suggestions, repeated arguments, counted flags, positionals
// with an optional subcommand, a required subcommand, `help` as a subcommand.
use clap::{Parser, Subcommand, ValueEnum, ArgAction};

#[derive(Parser, Debug)]
struct A {
    #[arg(short, long)]
    name: String,
    #[arg(long, value_enum)]
    mode: Option<Mode>,
    #[arg(short, long, action = ArgAction::Count)]
    verbose: u8,
    #[arg(short = 't', long = "tag")]
    tags: Vec<String>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Mode {
    Fast,
    Slow,
}

#[derive(Parser, Debug)]
#[command(name = "tool", version = "2.3", about = "Does things")]
struct B {
    /// The input
    input: String,
    /// The output
    output: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    Run,
    Stop { #[arg(long)] force: bool },
}

#[derive(Parser, Debug)]
struct G {
    #[command(subcommand)]
    cmd: Cmd,
    #[arg(long)]
    dry: bool,
}

fn show<T: std::fmt::Debug>(r: Result<T, clap::Error>) {
    match r {
        Ok(a) => println!("OK {:?}", a),
        Err(e) => println!("ERR {:?}\n{}----", e.kind(), e),
    }
}

fn main() {
    show(A::try_parse_from(["a", "--nmae", "x"]));
    show(A::try_parse_from(["a", "--zzz"]));
    show(A::try_parse_from(["a", "-q"]));
    show(A::try_parse_from(["a", "-n", "x", "-n", "y"]));
    show(A::try_parse_from(["a", "-n", "x", "--mode", "slw"]));
    show(A::try_parse_from(["a", "-n", "x", "-vvv", "-t", "a", "--tag", "b", "-tc"]));
    show(A::try_parse_from(["a", "-n", "x", "extra"]));
    show(A::try_parse_from(["a", "-h"]));
    show(B::try_parse_from(["tool", "in", "out", "run"]));
    show(B::try_parse_from(["tool", "in"]));
    show(B::try_parse_from(["tool", "in", "out", "stop", "--force"]));
    show(B::try_parse_from(["tool", "-h"]));
    show(B::try_parse_from(["tool", "-V"]));
    show(B::try_parse_from(["tool"]));
    show(B::try_parse_from(["tool", "a", "b", "c"]));
    show(G::try_parse_from(["g", "--dry"]));
    show(G::try_parse_from(["g", "rn"]));
    show(G::try_parse_from(["g", "help"]));
    show(G::try_parse_from(["g", "help", "stop"]));
    show(G::try_parse_from(["g", "stop", "-h"]));
    show(G::try_parse_from(["g", "-h"]));
}
