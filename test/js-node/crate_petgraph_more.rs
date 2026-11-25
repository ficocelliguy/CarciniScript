// More of the bundled `petgraph`: `use petgraph::prelude::*`, `algo::` and fully qualified calls
// (also inside `println!`), removing nodes and edges (petgraph's swap-remove renumbering),
// `g[n] += ..` on weights, `reverse`, `edges_directed`, struct weights, and `Dfs` /
// `has_path_connecting` / `toposort` over a `DiGraphMap` keyed by hostnames.
// (Expected output: petgraph 0.8.3's, from a cargo build of this file.)
use petgraph::algo;
use petgraph::prelude::*;
use petgraph::Direction::Incoming;

#[derive(Debug, Clone, Default)]
struct Host {
    name: String,
    ram: u32,
}

fn reachable(g: &DiGraphMap<&str, u32>, from: &str) -> Vec<String> {
    let mut dfs = Dfs::new(g, from);
    let mut out = Vec::new();
    while let Some(h) = dfs.next(g) {
        out.push(h.to_string());
    }
    out
}

fn main() {
    let mut g: Graph<Host, u32> = Graph::new();
    let names = ["home", "n00dles", "CSEC", "avmnite-02h", "I.I.I.I"];
    let ix: Vec<NodeIndex> = names
        .iter()
        .enumerate()
        .map(|(i, n)| g.add_node(Host { name: n.to_string(), ram: 8 << i }))
        .collect();
    for (a, b, w) in [(0, 1, 1), (0, 2, 4), (1, 2, 2), (2, 3, 3), (3, 4, 5), (1, 4, 9)] {
        g.add_edge(ix[a], ix[b], w);
    }
    g[ix[2]].ram *= 2;
    if let Some(h) = g.node_weight_mut(ix[3]) {
        h.ram += 1;
    }
    let e = g.find_edge(ix[1], ix[4]).unwrap();
    g[e] += 100;
    println!("{} {:?} {:?}", g[ix[2]].name, g[ix[2]], g.edge_weight(e));
    println!("{:?}", g.node_weights().map(|h| h.ram).collect::<Vec<_>>());
    println!("{:?}", g.edges_directed(ix[2], Incoming).map(|r| (r.source().index(), *r.weight())).collect::<Vec<_>>());
    println!("{:?}", algo::dijkstra(&g, ix[0], None, |r| *r.weight()).get(&ix[4]));
    println!("{}", petgraph::algo::has_path_connecting(&g, ix[4], ix[0], None));

    // Removing an edge moves the last edge into its index; removing a node moves the last node.
    println!("{:?}", g.remove_edge(EdgeIndex::new(1)));
    println!("{:?}", g.edge_references().map(|r| (r.id().index(), r.source().index(), r.target().index(), *r.weight())).collect::<Vec<_>>());
    println!("{:?}", g.remove_node(ix[1]).map(|h| h.name));
    println!("{} {}", g.node_count(), g.edge_count());
    println!("{:?}", g.node_indices().map(|n| g[n].name.clone()).collect::<Vec<_>>());
    println!("{:?}", g.edge_references().map(|r| (r.source().index(), r.target().index(), *r.weight())).collect::<Vec<_>>());
    println!("{:?}", g.neighbors(NodeIndex::new(0)).collect::<Vec<_>>());
    println!("{:?} {:?}", g.remove_node(NodeIndex::new(9)).map(|h| h.ram), g.remove_edge(EdgeIndex::new(42)));
    g.reverse();
    println!("{:?}", g.neighbors(NodeIndex::new(1)).collect::<Vec<_>>());
    println!("topo {:?}", algo::toposort(&g, None).map(|v| v.iter().map(|n| n.index()).collect::<Vec<_>>()));
    let u = g.update_edge(NodeIndex::new(0), NodeIndex::new(2), 7);
    println!("{:?} {:?} {}", u, g.edge_endpoints(u), g.edge_count());
    g.clear_edges();
    println!("{} {} {:?}", g.node_count(), g.edge_count(), g.externals(Outgoing).collect::<Vec<_>>());

    // A directed map of who can reach whom.
    let mut net: DiGraphMap<&str, u32> = DiGraphMap::new();
    net.add_edge("home", "n00dles", 1);
    net.add_edge("home", "foodnstuff", 1);
    net.add_edge("n00dles", "CSEC", 1);
    net.add_edge("CSEC", "run4theh111z", 1);
    net.add_edge("foodnstuff", "CSEC", 1);
    println!("{:?}", reachable(&net, "home"));
    println!("{:?}", reachable(&net, "CSEC"));
    println!("{} {}", algo::has_path_connecting(&net, "home", "run4theh111z", None), algo::has_path_connecting(&net, "CSEC", "home", None));
    println!("{:?}", algo::toposort(&net, None));
    println!("{:?}", net.edges("home").collect::<Vec<_>>());
    println!("{:?}", net.edges_directed("CSEC", Incoming).collect::<Vec<_>>());
    net[("home", "n00dles")] = 5;
    println!("{:?}", net.edge_weight("home", "n00dles"));
    println!("{:?}", net.remove_edge("home", "foodnstuff"));
    println!("{} {:?}", net.remove_node("CSEC"), net.nodes().collect::<Vec<_>>());
    println!("{:?} {}", net.all_edges().collect::<Vec<_>>(), net.edge_count());
    println!("{:?}", net.neighbors_directed("run4theh111z", Incoming).collect::<Vec<_>>());

    // Undirected map: edges are stored smaller end first.
    let mut ring = UnGraphMap::<u32, ()>::from_edges(&[(3, 1), (1, 2), (2, 3)]);
    ring.add_edge(4, 4, ());
    println!("{:?}", ring.all_edges().map(|(a, b, _)| (a, b)).collect::<Vec<_>>());
    println!("{:?} {:?}", ring.neighbors(1).collect::<Vec<_>>(), ring.neighbors(4).collect::<Vec<_>>());
    println!("{} {}", algo::is_cyclic_undirected(&ring), algo::connected_components(&ring));
    let mut post = DfsPostOrder::new(&ring, 1);
    let mut order = Vec::new();
    while let Some(n) = post.next(&ring) {
        order.push(n);
    }
    println!("{:?}", order);
    let tree = UnGraph::<i32, ()>::from_edges(&[(0, 1), (1, 2), (1, 3)]);
    println!("{} {}", algo::is_cyclic_undirected(&tree), algo::is_cyclic_directed(&Graph::<(), ()>::from_edges(&[(0, 1), (1, 1)])));
}
