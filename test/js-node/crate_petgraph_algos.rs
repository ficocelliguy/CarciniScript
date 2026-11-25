// The bundled `petgraph`'s other `algo` functions: `all_simple_paths`, `floyd_warshall`,
// `condensation`, `k_shortest_path`, `is_bipartite_undirected`, `find_negative_cycle`, `bridges`,
// `articulation_points`, `spfa` and `scc`, over a network of servers.
// (Expected output: petgraph 0.8.3's, from a cargo build of this file.)
use petgraph::algo::articulation_points::articulation_points;
use petgraph::algo::{
    all_simple_paths, bridges, condensation, find_negative_cycle,
    floyd_warshall, is_bipartite_undirected, k_shortest_path, spfa,
};
use petgraph::graph::{DiGraph, NodeIndex, UnGraph};
use petgraph::graphmap::{DiGraphMap, UnGraphMap};
use petgraph::visit::EdgeRef;
use petgraph::algo;
use std::collections::hash_map::RandomState;

fn sorted<'a, K: Ord + Copy + 'a, V: Copy + 'a>(m: impl Iterator<Item = (&'a K, &'a V)>) -> Vec<(K, V)> {
    let mut v: Vec<(K, V)> = m.map(|(k, v)| (*k, *v)).collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn main() {
    let mut g = DiGraph::<&str, u32>::new();
    let home = g.add_node("home");
    let n00 = g.add_node("n00dles");
    let food = g.add_node("foodnstuff");
    let csec = g.add_node("CSEC");
    let omega = g.add_node("omega-net");
    let hub = g.add_node("the-hub");
    g.extend_with_edges(&[
        (home, n00, 1),
        (home, food, 4),
        (n00, food, 2),
        (n00, csec, 7),
        (food, csec, 3),
        (csec, omega, 1),
        (food, omega, 8),
        (omega, hub, 2),
        (csec, hub, 6),
    ]);

    // Every route from home to the hub, and the ones with at most one hop between.
    let paths: Vec<Vec<NodeIndex>> = all_simple_paths::<Vec<_>, _, RandomState>(&g, home, hub, 0, None).collect();
    for p in &paths {
        println!("{:?}", p.iter().map(|&n| g[n]).collect::<Vec<_>>());
    }
    let short = all_simple_paths::<Vec<NodeIndex>, _, RandomState>(&g, home, hub, 0, Some(2)).count();
    let long = algo::all_simple_paths::<Vec<_>, _, RandomState>(&g, home, hub, 3, None).count();
    println!("{} {} {}", paths.len(), short, long);
    let first: Option<Vec<_>> = all_simple_paths::<Vec<_>, _, RandomState>(&g, food, hub, 0, None).next();
    println!("{:?}", first);

    // All pairs.
    let fw = floyd_warshall(&g, |e| *e.weight()).unwrap();
    println!("{:?} {:?} {:?}", fw[&(home, hub)], fw[&(n00, omega)], fw[&(hub, home)]);
    let mut fwv: Vec<_> = fw.iter().filter(|(_, &d)| d < u32::MAX).map(|(&(a, b), &d)| (a.index(), b.index(), d)).collect();
    fwv.sort();
    println!("{:?}", fwv);
    let neg = DiGraph::<(), i32>::from_edges(&[(0, 1, 1), (1, 2, -3), (2, 0, 1)]);
    println!("{:?}", floyd_warshall(&neg, |e| *e.weight()));
    let fu = floyd_warshall(&UnGraph::<(), f64>::from_edges(&[(0, 1, 1.5), (1, 2, 2.0)]), |e| *e.weight()).unwrap();
    println!("{:?}", fu[&(NodeIndex::new(2), NodeIndex::new(0))]);

    // The second and third shortest walks.
    let k2 = k_shortest_path(&g, home, None, 2, |e| *e.weight());
    println!("{:?}", sorted(k2.iter()));
    let k3 = k_shortest_path(&g, home, Some(hub), 3, |e| *e.weight());
    println!("{:?}", sorted(k3.iter()));

    // SPFA and negative cycles.
    let sp = spfa(&g, home, |e| *e.weight()).unwrap();
    println!("{:?}", sp.distances);
    println!("{:?}", sp.predecessors);
    println!("{:?}", spfa(&neg, NodeIndex::new(0), |e| *e.weight()).is_err());
    let mut fl = DiGraph::<(), f64>::new();
    let a = fl.add_node(());
    let b = fl.add_node(());
    let c = fl.add_node(());
    let d = fl.add_node(());
    fl.add_edge(a, b, 1.0);
    fl.add_edge(b, c, 1.0);
    fl.add_edge(c, d, -1.0);
    fl.add_edge(d, b, -1.5);
    println!("{:?}", find_negative_cycle(&fl, a));
    fl.remove_edge(fl.find_edge(d, b).unwrap());
    println!("{:?}", find_negative_cycle(&fl, a));
    let gm: DiGraphMap<&str, f64> = DiGraphMap::from_edges(&[("x", "y", 2.0), ("y", "z", -5.0), ("z", "y", 1.0)]);
    println!("{:?}", find_negative_cycle(&gm, "x"));

    // Condensation: one node per strongly connected component.
    let mut cyc = DiGraph::<&str, u8>::new();
    let v: Vec<_> = ["a", "b", "c", "d", "e"].iter().map(|s| cyc.add_node(*s)).collect();
    for (x, y) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 3), (1, 3)] {
        cyc.add_edge(v[x], v[y], (x * 10 + y) as u8);
    }
    println!("{:?}", algo::scc(&cyc));
    let cond = condensation(cyc.clone(), true);
    println!("{:?} {}", cond.node_weights().collect::<Vec<_>>(), cond.edge_count());
    println!("{:?}", cond.edge_references().map(|e| (e.source().index(), e.target().index(), *e.weight())).collect::<Vec<_>>());
    let cond2 = condensation(cyc, false);
    println!("{:?}", cond2.edge_references().map(|e| (e.source().index(), e.target().index(), *e.weight())).collect::<Vec<_>>());

    // Bipartite, bridges and articulation points on undirected graphs.
    let ring = UnGraph::<(), ()>::from_edges(&[(0, 1), (1, 2), (2, 3), (3, 0)]);
    let tri = UnGraph::<(), ()>::from_edges(&[(0, 1), (1, 2), (2, 0)]);
    println!("{} {}", is_bipartite_undirected(&ring, NodeIndex::new(0)), is_bipartite_undirected(&tri, NodeIndex::new(0)));
    let mut net = UnGraph::<&str, u32>::new_undirected();
    let s: Vec<_> = ["home", "a", "b", "c", "d", "e", "f"].iter().map(|n| net.add_node(*n)).collect();
    for (x, y) in [(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 5), (5, 3), (5, 6)] {
        net.add_edge(s[x], s[y], (x * 10 + y) as u32);
    }
    println!("{:?}", bridges(&net).map(|e| (e.source().index(), e.target().index(), *e.weight())).collect::<Vec<_>>());
    let mut ap: Vec<_> = articulation_points(&net).into_iter().map(|n| net[n]).collect();
    ap.sort();
    println!("{:?}", ap);
    let um = UnGraphMap::<u32, ()>::from_edges(&[(1, 2), (2, 3), (3, 4), (4, 2), (4, 5)]);
    println!("{:?}", bridges(&um).collect::<Vec<_>>());
    let mut apm: Vec<_> = articulation_points(&um).into_iter().collect();
    apm.sort();
    println!("{:?} {}", apm, is_bipartite_undirected(&um, 1));
    let paths_m: Vec<Vec<u32>> = all_simple_paths::<Vec<u32>, _, RandomState>(&um, 1, 5, 0, None).collect();
    println!("{:?}", paths_m);
    let fwm = floyd_warshall(&um, |_| 1u32).unwrap();
    println!("{:?} {:?}", fwm[&(1, 5)], fwm[&(5, 1)]);
}
