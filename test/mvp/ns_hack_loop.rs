// args: foodnstuff 12
// The classic early-game loop: weaken while security is high, grow while money is low, else hack.

const SECURITY_SLACK: f64 = 5.0;
const MONEY_FRACTION: f64 = 0.75;

pub async fn main(ns: NS) {
    let target = match ns.args.get(0).and_then(|a| a.as_string()) {
        Some(t) => t,
        None => {
            ns.tprint("usage: run hack.rs <target> [cycles]");
            return;
        }
    };
    let cycles = ns.args.get(1).and_then(|a| a.as_f64()).unwrap_or(10.0) as u32;

    if !ns.has_root_access(&target) {
        ns.nuke(&target);
    }
    let min_sec = ns.get_server_min_security_level(&target);
    let max_money = ns.get_server_max_money(&target);
    let mut earned = 0.0;

    for i in 0..cycles {
        let sec = ns.get_server_security_level(&target);
        let money = ns.get_server_money_available(&target);
        if sec > min_sec + SECURITY_SLACK {
            let w = ns.weaken(&target).await;
            ns.print(format!("#{i} weaken: -{w:.3} security"));
        } else if money < max_money * MONEY_FRACTION {
            let g = ns.grow(&target).await;
            ns.print(format!("#{i} grow: x{g:.4} (${:.2})", ns.get_server_money_available(&target)));
        } else {
            let stolen = ns.hack(&target).await;
            earned += stolen;
            ns.print(format!("#{i} hack: ${stolen:.2}"));
        }
        ns.sleep(100.0).await;
    }
    ns.tprint(format!("earned ${:.2} from {} in {} cycles", earned, target, cycles));
}
