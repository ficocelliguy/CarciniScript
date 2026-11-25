// args: n00dles 3 --dry-run
// Parse a script's arguments by hand: a target, a thread count and flags.

#[derive(Debug, Default)]
struct Options {
    target: Option<String>,
    threads: u32,
    dry_run: bool,
    unknown: Vec<String>,
}

fn parse(args: &[ScriptArg]) -> Options {
    let mut opts = Options { threads: 1, ..Default::default() };
    for arg in args {
        match arg {
            ScriptArg::Str(s) if s.starts_with("--") => match &s[2..] {
                "dry-run" => opts.dry_run = true,
                other => opts.unknown.push(other.to_string()),
            },
            ScriptArg::Str(s) => opts.target = Some(s.clone()),
            ScriptArg::Num(n) => opts.threads = *n as u32,
            ScriptArg::Bool(b) => opts.dry_run = *b,
        }
    }
    opts
}

pub async fn main(ns: NS) {
    let opts = parse(&ns.args);
    ns.tprint(format!("{:?}", opts));
    let Some(target) = opts.target else {
        ns.tprint("no target given");
        return;
    };
    let ram = ns.get_server_max_ram(&target) - ns.get_server_used_ram(&target);
    let threads = opts.threads.min((ram / 1.75) as u32);
    if opts.dry_run {
        ns.tprint(format!("would run {threads} thread(s) on {target} ({ram} GB free)"));
    } else {
        ns.tprint(format!("running {threads} thread(s) on {target}"));
    }
}
