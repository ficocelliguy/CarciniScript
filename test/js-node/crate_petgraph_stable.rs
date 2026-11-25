// The bundled `petgraph`'s `StableGraph` (indices that survive removals, vacant slots reused
// newest first), `Topo` and `Reversed` walkers, `Dot` output, and graphs with an index type
// other than `u32` (`Graph<N, E, Directed, u16>`, `NodeIndex<usize>`).
// (Expected output: petgraph 0.8.3's, from a cargo build of this file.)
use petgraph::dot::{Config, Dot, RankDir};
use petgraph::graph::{DiGraph, EdgeIndex, Graph, NodeIndex, UnGraph};
use petgraph::stable_graph::{StableDiGraph, StableGraph, StableUnGraph};
use petgraph::visit::{Bfs, Dfs, DfsPostOrder, EdgeRef, IntoEdgeReferences, IntoNeighbors, Reversed, Topo};
use petgraph::{algo, Directed, Direction, Incoming, Outgoing};

#[derive(Debug, Clone, Default)]
struct Server {
    name: String,
    ram: u32,
}

fn names(g: &StableGraph<Server, u32>, ns: &[NodeIndex]) -> Vec<String> {
    ns.iter().map(|&n| g[n].name.clone()).collect()
}

fn main() {
    // StableGraph: removing keeps the other indices.
    let mut g: StableGraph<Server, u32> = StableGraph::new();
    let hosts = ["home", "n00dles", "foodnstuff", "CSEC", "omega-net", "the-hub"];
    let ix: Vec<NodeIndex> = hosts
        .iter()
        .enumerate()
        .map(|(i, h)| g.add_node(Server { name: h.to_string(), ram: 4 << i }))
        .collect();
    let edges = [(0, 1, 1), (0, 2, 2), (1, 3, 3), (2, 3, 1), (3, 4, 5), (4, 5, 2), (2, 5, 9)];
    let eix: Vec<EdgeIndex> = edges.iter().map(|&(a, b, w)| g.add_edge(ix[a], ix[b], w)).collect();
    println!("{} {} {:?}", g.node_count(), g.edge_count(), g.node_indices().collect::<Vec<_>>());
    println!("{:?}", g.remove_node(ix[2]).map(|s| s.name));
    println!("{} {} {:?}", g.node_count(), g.edge_count(), g.node_indices().map(|n| n.index()).collect::<Vec<_>>());
    println!("{:?}", g.edge_references().map(|e| (e.id().index(), e.source().index(), e.target().index(), *e.weight())).collect::<Vec<_>>());
    println!("{} {} {:?}", g.contains_node(ix[2]), g.contains_node(ix[3]), g.node_weight(ix[2]).map(|s| s.ram));
    println!("{:?} {:?}", g.edge_weight(eix[1]), g.edge_endpoints(eix[4]));
    println!("{:?}", g.remove_edge(eix[0]));
    println!("{:?}", g.remove_edge(eix[0]));
    // Vacant slots are reused, the most recently freed first.
    let x = g.add_node(Server { name: "max-hardware".into(), ram: 32 });
    let y = g.add_node(Server { name: "zer0".into(), ram: 32 });
    let z = g.add_node(Server { name: "phantasy".into(), ram: 32 });
    println!("{:?} {:?} {:?}", x, y, z);
    let e1 = g.add_edge(x, ix[5], 4);
    let e2 = g.add_edge(y, x, 6);
    let e3 = g.add_edge(z, y, 7);
    println!("{:?} {:?} {:?}", e1, e2, e3);
    println!("{:?}", g.neighbors(ix[1]).collect::<Vec<_>>());
    println!("{:?}", names(&g, &g.neighbors_directed(ix[5], Incoming).collect::<Vec<_>>()));
    println!("{:?}", g.edges_directed(ix[3], Outgoing).map(|e| (e.target().index(), *e.weight())).collect::<Vec<_>>());
    println!("{:?}", g.externals(Incoming).map(|n| g[n].name.as_str()).collect::<Vec<_>>());
    g[ix[4]].ram += 1;
    g[e2] *= 10;
    println!("{} {:?} {:?}", g[ix[4]].ram, g.edge_weight(e2), g.find_edge(y, x));
    println!("{:?}", g.node_weights().map(|s| s.ram).collect::<Vec<_>>());
    println!("{:?}", g.edge_weights().collect::<Vec<_>>());
    println!("{:?}", g.edge_indices().collect::<Vec<_>>());
    let d = algo::dijkstra(&g, z, None, |e| *e.weight());
    let mut d: Vec<_> = d.into_iter().map(|(n, c)| (n.index(), c)).collect();
    d.sort();
    println!("{:?}", d);
    println!("{:?}", algo::toposort(&g, None).map(|v| v.iter().map(|n| n.index()).collect::<Vec<_>>()));
    println!("{} {}", algo::has_path_connecting(&g, z, ix[5], None), algo::is_cyclic_directed(&g));
    println!("{:?}", algo::kosaraju_scc(&g));
    println!("{:?}", algo::tarjan_scc(&g).len());
    let mut dfs = Dfs::new(&g, z);
    let mut seen = Vec::new();
    while let Some(n) = dfs.next(&g) {
        seen.push(n.index());
    }
    println!("{:?}", seen);
    let mut gg = g.clone();
    gg.reverse();
    println!("{:?}", gg.neighbors(ix[5]).collect::<Vec<_>>());
    gg.clear_edges();
    println!("{} {}", gg.node_count(), gg.edge_count());

    // Into a `Graph` (compacted) and back.
    let plain: Graph<Server, u32> = Graph::from(g.clone());
    println!("{} {:?}", plain.node_count(), plain.edge_references().map(|e| (e.source().index(), e.target().index())).collect::<Vec<_>>());
    let back = StableGraph::from(plain);
    println!("{:?}", back.node_indices().collect::<Vec<_>>());

    // Padding: `from_edges` leaves the indices it skipped vacant.
    let mut s = StableDiGraph::<u8, ()>::from_edges(&[(0, 3), (3, 5)]);
    println!("{} {:?}", s.node_count(), s.node_indices().collect::<Vec<_>>());
    println!("{:?}", s.add_node(7));
    s.extend_with_edges(&[(1, 0)]);
    println!("{:?} {}", s.node_indices().collect::<Vec<_>>(), s.edge_count());
    let mut u = StableUnGraph::<i32, i32>::default();
    let a = u.add_node(1);
    let b = u.add_node(2);
    let c = u.add_node(3);
    u.add_edge(a, b, 10);
    u.add_edge(b, c, 20);
    u.add_edge(c, c, 30);
    println!("{:?} {:?}", u.neighbors(b).collect::<Vec<_>>(), u.neighbors(c).collect::<Vec<_>>());
    println!("{:?}", u.edges(c).map(|e| (e.source().index(), e.target().index(), *e.weight())).collect::<Vec<_>>());
    println!("{:?}", u.update_edge(c, b, 21));
    println!("{:?} {}", u.edge_weights().collect::<Vec<_>>(), u.is_directed());
    u.remove_node(b);
    println!("{} {}", u.edge_count(), algo::is_cyclic_undirected(&u));

    // Topo: dependency order (scripts waiting on the ones they import).
    let mut deps = DiGraph::<&str, ()>::new();
    let lib = deps.add_node("lib.js");
    let hack = deps.add_node("hack.js");
    let grow = deps.add_node("grow.js");
    let batch = deps.add_node("batcher.js");
    let main = deps.add_node("main.js");
    deps.extend_with_edges(&[(lib, hack), (lib, grow), (hack, batch), (grow, batch), (batch, main), (lib, main)]);
    let mut topo = Topo::new(&deps);
    let mut order = Vec::new();
    while let Some(n) = topo.next(&deps) {
        order.push(deps[n]);
    }
    println!("{:?}", order);
    // What `main.js` needs, by walking the edges backwards.
    let mut back = Dfs::new(Reversed(&deps), main);
    let mut needs = Vec::new();
    while let Some(n) = back.next(Reversed(&deps)) {
        needs.push(deps[n]);
    }
    println!("{:?}", needs);
    let mut bfs = Bfs::new(Reversed(&deps), main);
    let mut layer = Vec::new();
    while let Some(n) = bfs.next(Reversed(&deps)) {
        layer.push(n.index());
    }
    println!("{:?}", layer);
    let mut post = DfsPostOrder::new(Reversed(&deps), main);
    let mut po = Vec::new();
    while let Some(n) = post.next(Reversed(&deps)) {
        po.push(n.index());
    }
    println!("{:?}", po);
    println!("{:?}", Reversed(&deps).neighbors(batch).collect::<Vec<_>>());
    // (Starting points with incoming edges are dropped.)
    let mut from = Topo::with_initials(&deps, [grow, lib]);
    let mut rest = Vec::new();
    while let Some(n) = from.next(&deps) {
        rest.push(n.index());
    }
    println!("{:?}", rest);
    // A cycle stops it short.
    deps.add_edge(main, grow, ());
    let mut topo = Topo::new(&deps);
    let mut partial = Vec::new();
    while let Some(n) = topo.next(&deps) {
        partial.push(n.index());
    }
    println!("{:?}", partial);
    let mut topo = Topo::new(&g);
    let mut sorder = Vec::new();
    while let Some(n) = topo.next(&g) {
        sorder.push(n.index());
    }
    println!("{:?}", sorder);

    // Dot.
    let mut net = Graph::<&str, u32>::new();
    let h = net.add_node("home");
    let n = net.add_node("n00dles \"quoted\"");
    let c = net.add_node("CSEC\\x");
    net.add_edge(h, n, 1);
    net.add_edge(n, c, 2);
    net.add_edge(h, c, 3);
    println!("{:?}", Dot::new(&net));
    println!("{}", Dot::new(&net));
    println!("{:?}", Dot::with_config(&net, &[Config::EdgeNoLabel]));
    println!("{}", Dot::with_config(&net, &[Config::NodeIndexLabel, Config::EdgeIndexLabel]));
    print!("{:?}", Dot::with_config(&net, &[Config::GraphContentOnly, Config::NodeNoLabel]));
    let un = UnGraph::<i32, ()>::from_edges(&[(0, 1), (1, 2)]);
    print!("{:?}", Dot::new(&un));
    print!("{:?}", Dot::with_config(&un, &[Config::RankDir(RankDir::LR), Config::EdgeNoLabel]));
    let text = format!("{:?}", Dot::with_config(&g, &[Config::EdgeNoLabel]));
    println!("{}", text.lines().count());
    let mut sd = StableGraph::<u32, f64>::new();
    let p = sd.add_node(1);
    let q = sd.add_node(2);
    let r = sd.add_node(3);
    sd.add_edge(p, r, 0.5);
    sd.add_edge(r, q, 1.25);
    sd.remove_node(q);
    print!("{}", Dot::new(&sd));
    let gm = petgraph::graphmap::DiGraphMap::<u32, ()>::from_edges(&[(5, 3), (3, 9)]);
    print!("{:?}", Dot::with_config(&gm, &[Config::EdgeNoLabel]));
    println!();

    // Other index types.
    let mut small: Graph<&str, u8, Directed, u16> = Graph::default();
    let s0 = small.add_node("a");
    let s1: NodeIndex<u16> = small.add_node("b");
    small.add_edge(s0, s1, 3);
    println!("{:?} {:?} {}", s1, small.find_edge(s0, s1), small[s1]);
    let big = DiGraph::<u8, (), usize>::from_edges(&[(0usize, 1usize), (1, 2)]);
    let ni: NodeIndex<usize> = NodeIndex::new(2);
    println!("{:?} {:?}", big.neighbors(NodeIndex::new(1)).collect::<Vec<_>>(), ni);
    println!("{:?}", algo::dijkstra(&big, NodeIndex::<usize>::new(0), Some(ni), |_| 1).get(&ni));
    let mut st: StableGraph<i32, i32, Directed, u8> = StableGraph::with_capacity(4, 4);
    let k = st.add_node(5);
    st.add_edge(k, k, 1);
    println!("{:?} {:?}", k, st.edges(k).next().map(|e| e.id()));
    let dir: Direction = Outgoing;
    println!("{:?}", st.neighbors_directed(k, dir.opposite()).collect::<Vec<_>>());
}
