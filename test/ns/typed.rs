// Typed NS bindings: interface fields (snake_case → camelCase), optional fields as Option,
// namespaces, and f64 NS numbers converted with `as`.
fn describe(ns: &NS, host: &str) -> Option<String> {
    let server = ns.get_server(host);
    let max = server.money_max?;
    Some(format!("{} holds up to ${:.0}", server.hostname, max))
}

pub async fn main(ns: NS) {
    let home = ns.get_hostname();
    let server = ns.get_server(&home);
    let free = server.max_ram - server.ram_used;
    let threads = (free / 1.75).floor() as u32;
    ns.tprint(&format!("{} has {:.1} GB free on {} cores: {threads} threads", server.hostname, free, server.cpu_cores));
    ns.tprint(&format!("hacknet nodes: {}", ns.hacknet.num_nodes() as u32));
    for host in ["n00dles", "home"] {
        match describe(&ns, host) {
            Some(text) => ns.tprint(&text),
            None => ns.tprint(&format!("{host} has no money limit")),
        }
    }
}
