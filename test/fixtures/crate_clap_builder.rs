// The bundled `clap`'s builder: `Command` / `Arg` / `ArgAction`, `value_parser!`, `ArgMatches`.
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};

fn cli() -> Command {
    Command::new("tool")
        .about("A builder-style tool")
        .version("1.2.3")
        .arg(Arg::new("input").help("File to read").required(true))
        .arg(
            Arg::new("count")
                .short('c')
                .long("count")
                .help("How many")
                .value_parser(value_parser!(u32))
                .default_value("3"),
        )
        .arg(Arg::new("verbose").short('v').long("verbose").action(ArgAction::Count))
        .arg(Arg::new("dry").long("dry-run").action(ArgAction::SetTrue).help("Do nothing"))
        .arg(Arg::new("tag").short('t').long("tag").action(ArgAction::Append))
        .subcommand(Command::new("list").about("List things").arg(Arg::new("all").short('a').action(ArgAction::SetTrue)))
}

fn show(m: &ArgMatches) {
    let input = m.get_one::<String>("input").unwrap();
    let count = *m.get_one::<u32>("count").unwrap();
    let tags: Vec<String> = m.get_many::<String>("tag").unwrap_or_default().cloned().collect();
    println!("input={} count={} v={} dry={} tags={:?}", input, count, m.get_count("verbose"), m.get_flag("dry"), tags);
    match m.subcommand() {
        Some(("list", sub)) => println!("list all={}", sub.get_flag("all")),
        Some((other, _)) => println!("other {}", other),
        None => println!("no subcommand"),
    }
}

fn main() {
    for args in [
        vec!["tool", "in.txt"],
        vec!["tool", "in.txt", "-vv", "--count", "7", "-t", "x", "--tag=y", "--dry-run", "list", "-a"],
        vec!["tool", "in.txt", "--count", "-5"],
        vec!["tool", "in.txt", "--count", "many"],
        vec!["tool", "--help"],
        vec!["tool", "-V"],
    ] {
        match cli().try_get_matches_from(args) {
            Ok(m) => show(&m),
            Err(e) => print!("ERR {:?}\n{}----\n", e.kind(), e),
        }
    }
    let m = cli().get_matches_from(["tool", "a"]);
    println!("{}", m.get_one::<String>("input").map(|s| s.as_str()).unwrap_or("-"));
    println!("{}", cli().render_usage());
}
