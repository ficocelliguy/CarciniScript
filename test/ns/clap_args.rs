// The bundled clap in an NS script: `Args::parse()` reads the script's name and `ns.args`
// (numbers as their text); an error exits the script, shown in the terminal.
use clap::Parser;

/// Deploy a script.
#[derive(Parser, Debug)]
struct Args {
    /// The server to deploy to
    #[arg(long)]
    target: String,
    /// Threads per server
    #[arg(short, long, default_value_t = 1)]
    threads: u32,
    #[arg(long)]
    dry_run: bool,
}

pub async fn main(ns: &NS) {
    let args = Args::parse();
    ns.tprint(&format!("{:?}", args));
    match Args::try_parse_from(["deploy.js", "--threads", "x"]) {
        Ok(_) => ns.tprint("parsed?"),
        Err(e) => ns.tprint(&e.to_string()),
    }
    ns.tprint(&Args::command().render_help());
    // An error ends the script quietly, its message in the terminal.
    let _ = Args::parse_from(["deploy.js", "--target", "x", "--bogus"]);
    ns.tprint("not reached");
}
