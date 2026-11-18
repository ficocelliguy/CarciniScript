//! The TS unions as enums: `ns.args` is `Vec<ScriptArg>`, requirements and tasks are tagged by
//! their `type`, `Result` by `success`, and a coding contract by its name. The values are the
//! game's own (nothing converts): matching reads the JS type or the tag field.

fn describe(arg: &ScriptArg) -> String {
    match arg {
        ScriptArg::Str(s) => format!("text {s}"),
        ScriptArg::Num(n) => format!("number {n}"),
        ScriptArg::Bool(b) => format!("flag {b}"),
    }
}

fn requirement(r: &PlayerRequirement) -> String {
    match r {
        PlayerRequirement::Money(m) => format!("${}", m.money),
        PlayerRequirement::Skill(s) => format!("hacking {:?}", s.skills.hacking),
        PlayerRequirement::Not(n) => format!("not ({})", requirement(&n.condition)),
        PlayerRequirement::EmployedBy(e) => format!("employed by {}", e.company),
        other => format!("{other:?}"),
    }
}

pub async fn main(ns: NS) {
    // `ns.args` reads as it did (`as_string`, `as_f64`), and as an enum.
    let target = ns.args.first().and_then(|a| a.as_string()).unwrap_or_default();
    let threads = ns.args.get(1).and_then(ScriptArg::as_f64).unwrap_or(1.0);
    let shown: Vec<String> = ns.args.iter().map(describe).collect();
    ns.tprint(&format!("{target} {threads} {shown:?}"));
    ns.tprint(&format!("{:?} {} {}", ns.args, ns.args[0], ns.args[1].to_string()));
    let flags = ns.args.iter().filter(|a| matches!(a, ScriptArg::Str(s) if s.starts_with("--"))).count();
    ns.tprint(&format!("{flags} flag(s)"));

    // A union parameter takes any payload, or the enum.
    ns.run("hack.rs", 4, "n00dles", true);
    ns.run("hack.rs", RunOptions { threads: Some(2.0), ..Default::default() }, ScriptArg::Num(7.0));
    let args: Vec<ScriptArg> = vec![ScriptArg::Str("a".into()), ScriptArg::Num(1.0)];
    ns.exec("hack.rs", "home", 1, args[0].clone(), args[1].clone());
    ns.kill(7, "home");
    ns.kill("hack.rs", "home", "n00dles");

    // A JS value in and out of the enum.
    let raw: JsValue = ScriptArg::Bool(false).into();
    let back = ScriptArg::try_from(raw);
    let not_an_arg = ScriptArg::try_from(JsValue::NULL);
    ns.tprint(&format!("{back:?} {}", not_an_arg.is_err()));

    // Tagged unions: requirements (recursive), a task, a sleeve's task.
    for r in ns.singularity.get_faction_invite_requirements("CyberSec") {
        ns.tprint(&requirement(&r));
    }
    match ns.singularity.get_current_work() {
        Some(Task::Study(s)) => ns.tprint(&format!("studying {} at {}", s.class_type, s.location)),
        Some(other) => ns.tprint(&format!("working: {other:?}")),
        None => ns.tprint("idle"),
    }
    if let Some(SleeveTask::Crime(c)) = ns.sleeve.get_task(0) {
        ns.tprint(&format!("sleeve 0 commits {} ({}/{})", c.crime_type, c.cycles_worked, c.cycles_needed));
    }
    ns.tprint(&format!("{:?}", ns.sleeve.get_task(1)));

    // Bitburner's `Result`: matched, or as a Rust `Result`.
    match ns.sleeve.purchase_sleeve() {
        NsResult::Success(_) => ns.tprint("bought"),
        NsResult::Failure(f) => ns.tprint(&format!("not bought: {}", f.message)),
    }
    let upgraded = ns.sleeve.upgrade_memory(0, 1).into_result().map(|s| s.success);
    ns.tprint(&format!("{upgraded:?}"));

    // A coding contract: its `data` and `submit` typed by the contract.
    match ns.codingcontract.get_contract("a.cct", "home") {
        CodingContractObject::FindLargestPrimeFactor(c) => {
            let n = c.data as i64;
            let mut best = 1;
            let mut m = n;
            let mut p = 2;
            while p * p <= m {
                while m % p == 0 {
                    best = p;
                    m /= p;
                }
                p += 1;
            }
            if m > 1 {
                best = m;
            }
            ns.tprint(&format!("{}: {} -> {}", c.r#type, c.data, c.submit(best as f64)));
        }
        other => ns.tprint(&format!("skipped {other:?}")),
    }
    if let CodingContractObject::MergeOverlappingIntervals(c) = ns.codingcontract.get_contract("b.cct", "home") {
        let mut v = c.data.clone();
        v.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        ns.tprint(&format!("{} tries left; {}", c.num_tries_remaining(), c.submit(&v)));
    }

    // A prompt's answer is a bool or text.
    match ns.prompt("Continue?", PromptOptions { r#type: Some("text".into()), ..Default::default() }).await {
        PromptResult::Str(s) => ns.tprint(&format!("typed {s}")),
        PromptResult::Bool(b) => ns.tprint(&format!("clicked {b}")),
    }
}
