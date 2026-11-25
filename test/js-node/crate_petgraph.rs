// The bundled `petgraph`: a server network as a `Graph` (BFS / DFS from home, shortest paths by
// hops and by hack time, components), a dependency `DiGraph` (toposort, cycles, SCCs), a
// `GraphMap` keyed by hostnames, and `min_spanning_tree`. `neighbors` / `edges` come newest
// first as in petgraph, and `NodeIndex` prints as `NodeIndex(3)`.
// (Expected output: petgraph 0.8.3's, from a cargo build of this file with `sorted` taking
// 0.8's hashbrown maps and `use petgraph::data::FromElements`, which 0.8 needs for
// `from_elements`.)
use petgraph::algo::{
    astar, bellman_ford, connected_components, dijkstra, has_path_connecting, is_cyclic_directed,
    kosaraju_scc, min_spanning_tree, tarjan_scc, toposort,
};
use petgraph::data::Element;
use petgraph::graph::{DiGraph, EdgeIndex, NodeIndex, UnGraph};
use petgraph::graphmap::{DiGraphMap, UnGraphMap};
use petgraph::visit::{Bfs, Dfs, DfsPostOrder, EdgeRef};
use petgraph::{Direction, Graph, Incoming, Outgoing};
use std::collections::HashMap;

fn sorted<K: Ord + Copy, V: Copy>(m: &HashMap<K, V>) -> Vec<(K, V)> {
    let mut v: Vec<(K, V)> = m.iter().map(|(k, v)| (*k, *v)).collect();
    v.sort_by_key(|p| p.0);
    v
}

fn main() {
    // The network `scan()` finds: an undirected graph of hosts, weighted by hack time.
    let mut net: UnGraph<&str, f64> = UnGraph::new_undirected();
    let home = net.add_node("home");
    let n00dles = net.add_node("n00dles");
    let foodnstuff = net.add_node("foodnstuff");
    let sigma = net.add_node("sigma-cosmetics");
    let csec = net.add_node("CSEC");
    let neo = net.add_node("neo-net");
    let zer0 = net.add_node("zer0");
    net.add_edge(home, n00dles, 1.0);
    net.add_edge(home, foodnstuff, 2.5);
    net.add_edge(home, sigma, 4.0);
    net.add_edge(n00dles, csec, 6.0);
    net.add_edge(foodnstuff, csec, 2.0);
    net.add_edge(sigma, neo, 1.5);
    let e = net.add_edge(csec, neo, 3.0);
    println!("{} nodes, {} edges, directed: {}", net.node_count(), net.edge_count(), net.is_directed());
    println!("{:?} {:?} {} {:?}", home, e, net[csec], net.edge_endpoints(e));
    println!("{:?}", net.neighbors(home).map(|n| net[n]).collect::<Vec<_>>());
    println!("{:?}", net.neighbors(csec).collect::<Vec<_>>());
    println!("{:?} {:?}", net.find_edge(neo, csec), net.find_edge(home, zer0));
    println!("{} {:?}", net.contains_edge(csec, foodnstuff), net.edge_weight(EdgeIndex::new(1)));
    for er in net.edges(csec) {
        println!("  {} -> {} ({}) {:?}", net[er.source()], net[er.target()], er.weight(), er.id());
    }

    // Breadth-first from home, depth-first, and post-order.
    let mut bfs = Bfs::new(&net, home);
    let mut order = Vec::new();
    while let Some(nx) = bfs.next(&net) {
        order.push(net[nx]);
    }
    println!("bfs {:?}", order);
    let mut dfs = Dfs::new(&net, home);
    let mut order = Vec::new();
    while let Some(nx) = dfs.next(&net) {
        order.push(net[nx]);
    }
    println!("dfs {:?}", order);
    let mut post = DfsPostOrder::new(&net, home);
    let mut order = Vec::new();
    while let Some(nx) = post.next(&net) {
        order.push(nx.index());
    }
    println!("post {:?}", order);

    // Shortest paths: by hack time, by hops, and A* to CSEC.
    let times = dijkstra(&net, home, None, |e| *e.weight());
    for (n, t) in sorted(&times) {
        println!("  {:>16} {:.1}", net[n], t);
    }
    let hops = dijkstra(&net, home, Some(neo), |_| 1);
    println!("hops {:?} {:?}", hops.get(&neo), hops.len());
    let route = astar(&net, home, |n| n == csec, |e| *e.weight(), |_| 0.0);
    match route {
        Some((cost, path)) => println!("astar {} {:?}", cost, path.iter().map(|&n| net[n]).collect::<Vec<_>>()),
        None => println!("no route"),
    }
    println!("{:?}", astar(&net, home, |n| n == zer0, |e| *e.weight(), |_| 0.0));
    let paths = bellman_ford(&net, home).unwrap();
    println!("{:?}", paths.distances);
    println!("{:?}", paths.predecessors);
    println!("components {} {}", connected_components(&net), has_path_connecting(&net, home, zer0, None));
    println!("{:?}", net.externals(Outgoing).collect::<Vec<_>>());

    // A directed graph of script dependencies.
    let mut deps = DiGraph::<&str, ()>::new();
    let lib = deps.add_node("lib.js");
    let hack = deps.add_node("hack.js");
    let grow = deps.add_node("grow.js");
    let batch = deps.add_node("batcher.js");
    let ui = deps.add_node("ui.js");
    deps.add_edge(lib, hack, ());
    deps.add_edge(lib, grow, ());
    deps.add_edge(hack, batch, ());
    deps.add_edge(grow, batch, ());
    deps.add_edge(lib, ui, ());
    let topo = toposort(&deps, None).unwrap();
    println!("topo {:?}", topo.iter().map(|&n| deps[n]).collect::<Vec<_>>());
    println!("{} {:?}", is_cyclic_directed(&deps), deps.neighbors_directed(batch, Incoming).collect::<Vec<_>>());
    println!("{:?} {:?}", deps.externals(Incoming).collect::<Vec<_>>(), deps.externals(Direction::Outgoing).collect::<Vec<_>>());
    deps.add_edge(batch, lib, ());
    println!("{} {:?}", is_cyclic_directed(&deps), toposort(&deps, None).map_err(|c| c.node_id()));
    println!("kosaraju {:?}", kosaraju_scc(&deps));
    println!("tarjan {:?}", tarjan_scc(&deps));
    deps[ui] = "dashboard.js";
    if deps.node_weight(grow).is_some() {
        deps[grow] = "grow2.js";
    }
    println!("{:?}", deps.node_indices().map(|n| deps[n]).collect::<Vec<_>>());
    println!("{:?}", deps.edge_references().map(|e| (e.source().index(), e.target().index())).collect::<Vec<_>>());

    // Graphs from edge lists.
    let ring = Graph::<(), u32>::from_edges(&[(0, 1, 4), (1, 2, 1), (2, 0, 7), (2, 3, 2)]);
    println!("{} {} {:?}", ring.node_count(), ring.edge_count(), ring.neighbors(NodeIndex::new(2)).collect::<Vec<_>>());
    println!("{:?}", tarjan_scc(&ring));
    let mut u = UnGraph::<u8, ()>::from_edges(&[(0, 1), (1, 2), (3, 4)]);
    u.extend_with_edges(&[(4, 5)]);
    println!("{} {} {}", u.node_count(), connected_components(&u), u[NodeIndex::new(5)]);

    // GraphMap: hostnames are the nodes.
    let mut hosts: UnGraphMap<&str, u32> = UnGraphMap::new();
    hosts.add_edge("home", "n00dles", 1);
    hosts.add_edge("home", "foodnstuff", 1);
    hosts.add_edge("n00dles", "CSEC", 2);
    hosts.add_edge("foodnstuff", "CSEC", 5);
    println!("{:?} {:?}", hosts.add_edge("CSEC", "home", 9), hosts.add_edge("home", "CSEC", 3));
    hosts.add_node("darkweb");
    println!("{:?}", hosts.nodes().collect::<Vec<_>>());
    println!("{:?}", hosts.neighbors("home").collect::<Vec<_>>());
    println!("{:?}", hosts.all_edges().collect::<Vec<_>>());
    println!("{} {} {:?}", hosts.contains_edge("CSEC", "n00dles"), hosts.contains_node("zer0"), hosts.edge_weight("CSEC", "home"));
    let mut bfs = Bfs::new(&hosts, "home");
    let mut seen = Vec::new();
    while let Some(h) = bfs.next(&hosts) {
        seen.push(h);
    }
    println!("{:?}", seen);
    let d = dijkstra(&hosts, "home", None, |e| *e.weight());
    println!("{:?}", sorted(&d));
    println!("{:?}", astar(&hosts, "foodnstuff", |n| n == "CSEC", |e| *e.2, |_| 0));
    println!("{} {}", connected_components(&hosts), hosts[("n00dles", "CSEC")]);

    let mut flow: DiGraphMap<u32, f64> = DiGraphMap::new();
    for (a, b, w) in [(1, 2, 0.5), (2, 3, 1.5), (1, 3, 2.5), (3, 4, -1.0), (4, 2, 3.0)] {
        flow.add_edge(a, b, w);
    }
    println!("{:?} {:?}", flow.neighbors(2).collect::<Vec<_>>(), flow.neighbors_directed(2, Incoming).collect::<Vec<_>>());
    println!("{:?}", bellman_ford(&flow, 1).map(|p| p.distances));
    println!("{:?} {:?}", tarjan_scc(&flow), kosaraju_scc(&flow));
    flow.add_edge(4, 1, -5.0);
    println!("{:?}", bellman_ford(&flow, 1).map(|p| p.distances));
    println!("{:?}", toposort(&flow, None).is_err());

    // Kruskal: the cheapest wiring that still reaches every server.
    let mst = UnGraph::<&str, f64>::from_elements(min_spanning_tree(&net));
    println!("{} {}", mst.node_count(), mst.edge_count());
    for e in mst.edge_references() {
        println!("  {} - {} {}", mst[e.source()], mst[e.target()], e.weight());
    }
    let edges: Vec<_> = min_spanning_tree(&net)
        .filter_map(|el| match el {
            Element::Edge { source, target, weight } => Some((source, target, weight)),
            Element::Node { .. } => None,
        })
        .collect();
    println!("{:?}", edges);
}
