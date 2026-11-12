// A typical early-game hacking loop, the target shape for Bitburner `.rs` scripts.
pub async fn main(ns: NS) {
    let target = "n00dles";
    let money_threshold = ns.get_server_max_money(target) * 0.75;
    let security_threshold = ns.get_server_min_security_level(target) + 5.0;
    let mut rounds = 0;
    while rounds < 3 {
        if ns.get_server_security_level(target) > security_threshold {
            ns.weaken(target).await;
        } else if ns.get_server_money_available(target) < money_threshold {
            ns.grow(target).await;
        } else {
            let stolen = ns.hack(&target).await;
            ns.tprint(&format!("stole ${:.2} from {target}", stolen));
        }
        rounds += 1;
    }
    ns.tprint(&format!("done after {rounds} rounds"));
}
