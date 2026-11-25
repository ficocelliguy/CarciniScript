// args: 20
// Pick the best hackable target we have root on, then run hack/grow/weaken cycles against it.
// First arg is how many cycles to run (default 10).

const CANDIDATES: [&str; 6] = [
    "n00dles",
    "foodnstuff",
    "sigma-cosmetics",
    "nectar-net",
    "neo-net",
    "max-hardware",
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Weaken,
    Grow,
    Hack,
}

fn pick_target(ns: &NS) -> Option<String> {
    let level = ns.get_hacking_level();
    CANDIDATES
        .iter()
        .filter(|h| ns.has_root_access(h))
        // only bother with targets well under our level, like the docs suggest
        .filter(|h| ns.get_server_required_hacking_level(h) <= level / 2 || level == 1)
        .max_by(|a, b| {
            let ma = ns.get_server_max_money(a) / ns.get_server_min_security_level(a);
            let mb = ns.get_server_max_money(b) / ns.get_server_min_security_level(b);
            ma.partial_cmp(&mb).unwrap()
        })
        .map(|h| h.to_string())
}

fn next_action(ns: &NS, target: &str) -> Action {
    let sec = ns.get_server_security_level(target);
    let min_sec = ns.get_server_min_security_level(target);
    let money = ns.get_server_money_available(target);
    let max_money = ns.get_server_max_money(target);

    if sec > min_sec + 5.0 {
        Action::Weaken
    } else if money < max_money * 0.75 {
        Action::Grow
    } else {
        Action::Hack
    }
}

pub async fn main(ns: NS) {
    let cycles = ns
        .args
        .first()
        .and_then(|a| a.as_f64())
        .map(|n| n as u32)
        .unwrap_or(10);

    // make sure the usual targets are rooted first
    for host in CANDIDATES {
        if !ns.has_root_access(host) && ns.get_server_num_ports_required(host) == 0 {
            ns.nuke(host);
        }
    }

    let Some(target) = pick_target(&ns) else {
        ns.tprint("no target available, exiting");
        return;
    };
    ns.tprint(format!("targeting {} for {} cycles", target, cycles));

    let mut earned = 0.0;
    let mut counts = [0u32; 3];
    for i in 0..cycles {
        let action = next_action(&ns, &target);
        match action {
            Action::Weaken => {
                let took = ns.weaken(&target).await;
                counts[0] += 1;
                ns.print(format!("{i:>3}: weaken -{took:.2} -> {:.2}", ns.get_server_security_level(&target)));
            }
            Action::Grow => {
                let factor = ns.grow(&target).await;
                counts[1] += 1;
                ns.print(format!("{i:>3}: grow x{factor:.3} -> ${:.0}", ns.get_server_money_available(&target)));
            }
            Action::Hack => {
                let stolen = ns.hack(&target).await;
                counts[2] += 1;
                earned += stolen;
                ns.print(format!("{i:>3}: hack +${stolen:.0}"));
            }
        }
        ns.sleep(200.0).await;
    }

    ns.tprint(format!(
        "done: {} weaken, {} grow, {} hack, earned ${:.0}",
        counts[0], counts[1], counts[2], earned
    ));
}
