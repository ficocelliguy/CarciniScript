// (petgraph prelude)
// The bundled `petgraph` crate: `Graph` (petgraph's adjacency lists, so `neighbors` and `edges`
// come newest first, as there), `GraphMap` (insertion-ordered, keyed by the nodes themselves),
// `Bfs` / `Dfs` / `DfsPostOrder`, and the `algo` functions, which the rewrite turns into methods
// of the graph (`dijkstra(&g, ..)` is `(&g).__pg_dijkstra(..)`) and walkers' `next(&g)` into
// `__pg_next(|n| (&g).neighbors(n).collect())`. Each algorithm follows petgraph's code (visiting
// order, `BinaryHeap` ties), so results with ties come out the same.

/// `petgraph::graph::NodeIndex` (with the default `u32` index).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PgNodeIndex(u32);
impl PgNodeIndex {
    pub fn new(x: usize) -> PgNodeIndex {
        PgNodeIndex(x as u32)
    }
    pub fn index(self) -> usize {
        self.0 as usize
    }
    pub fn end() -> PgNodeIndex {
        PgNodeIndex(u32::MAX)
    }
}
impl From<u32> for PgNodeIndex {
    fn from(x: u32) -> PgNodeIndex {
        PgNodeIndex(x)
    }
}
impl std::fmt::Debug for PgNodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "NodeIndex({:?})", self.0)
    }
}
/// `petgraph::graph::node_index`.
pub fn __pg_node_index(x: usize) -> PgNodeIndex {
    PgNodeIndex(x as u32)
}

/// `petgraph::graph::EdgeIndex`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PgEdgeIndex(u32);
impl PgEdgeIndex {
    pub fn new(x: usize) -> PgEdgeIndex {
        PgEdgeIndex(x as u32)
    }
    pub fn index(self) -> usize {
        self.0 as usize
    }
    pub fn end() -> PgEdgeIndex {
        PgEdgeIndex(u32::MAX)
    }
}
impl From<u32> for PgEdgeIndex {
    fn from(x: u32) -> PgEdgeIndex {
        PgEdgeIndex(x)
    }
}
impl std::fmt::Debug for PgEdgeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "EdgeIndex({:?})", self.0)
    }
}
/// `petgraph::graph::edge_index`.
pub fn __pg_edge_index(x: usize) -> PgEdgeIndex {
    PgEdgeIndex(x as u32)
}

/// What `from_edges` / `extend_with_edges` take as an edge's ends: a `NodeIndex` or a number.
pub trait PgIntoNodeIndex: Copy {
    fn __pg_into_ix(self) -> PgNodeIndex;
}
impl PgIntoNodeIndex for PgNodeIndex {
    fn __pg_into_ix(self) -> PgNodeIndex {
        self
    }
}
impl PgIntoNodeIndex for u32 {
    fn __pg_into_ix(self) -> PgNodeIndex {
        PgNodeIndex(self)
    }
}
impl PgIntoNodeIndex for i32 {
    fn __pg_into_ix(self) -> PgNodeIndex {
        PgNodeIndex(self as u32)
    }
}
impl PgIntoNodeIndex for usize {
    fn __pg_into_ix(self) -> PgNodeIndex {
        PgNodeIndex(self as u32)
    }
}

/// `petgraph::Direction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PgDirection {
    Outgoing,
    Incoming,
}
impl PgDirection {
    pub fn opposite(self) -> PgDirection {
        match self {
            PgDirection::Outgoing => PgDirection::Incoming,
            PgDirection::Incoming => PgDirection::Outgoing,
        }
    }
    pub fn index(self) -> usize {
        match self {
            PgDirection::Outgoing => 0,
            PgDirection::Incoming => 1,
        }
    }
}

/// `petgraph::EdgeType`: `Directed` or `Undirected`.
pub trait PgEdgeType {
    fn is_directed() -> bool;
}
/// `petgraph::Directed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PgDirected;
/// `petgraph::Undirected`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PgUndirected;
impl PgEdgeType for PgDirected {
    fn is_directed() -> bool {
        true
    }
}
impl PgEdgeType for PgUndirected {
    fn is_directed() -> bool {
        false
    }
}

/// `petgraph::graph::EdgeReference`: what `edges(n)` / `edge_references()` give.
pub struct PgEdgeReference<'a, E> {
    index: PgEdgeIndex,
    node: [PgNodeIndex; 2],
    weight: &'a E,
}
impl<'a, E> Clone for PgEdgeReference<'a, E> {
    fn clone(&self) -> Self {
        PgEdgeReference {
            index: self.index,
            node: self.node,
            weight: self.weight,
        }
    }
}
impl<'a, E> Copy for PgEdgeReference<'a, E> {}
impl<'a, E> PgEdgeReference<'a, E> {
    pub fn weight(&self) -> &'a E {
        self.weight
    }
    pub fn source(&self) -> PgNodeIndex {
        self.node[0]
    }
    pub fn target(&self) -> PgNodeIndex {
        self.node[1]
    }
    pub fn id(&self) -> PgEdgeIndex {
        self.index
    }
}
impl<'a, E: std::fmt::Debug> std::fmt::Debug for PgEdgeReference<'a, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("EdgeReference")
            .field("index", &self.index)
            .field("node", &self.node)
            .field("weight", &self.weight)
            .finish()
    }
}

/// A `GraphMap` edge as the algorithms' cost functions see it: `(a, b, &weight)`, with `.0` /
/// `.1` / `.2` and `EdgeRef`'s `source()` / `target()` / `weight()`.
pub struct PgMapEdgeRef<'a, N, E>(pub N, pub N, pub &'a E);
impl<'a, N: Copy, E> Clone for PgMapEdgeRef<'a, N, E> {
    fn clone(&self) -> Self {
        PgMapEdgeRef(self.0, self.1, self.2)
    }
}
impl<'a, N: Copy, E> Copy for PgMapEdgeRef<'a, N, E> {}
impl<'a, N: Copy, E> PgMapEdgeRef<'a, N, E> {
    pub fn weight(&self) -> &'a E {
        self.2
    }
    pub fn source(&self) -> N {
        self.0
    }
    pub fn target(&self) -> N {
        self.1
    }
}
impl<'a, N: std::fmt::Debug, E: std::fmt::Debug> std::fmt::Debug for PgMapEdgeRef<'a, N, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_tuple("")
            .field(&self.0)
            .field(&self.1)
            .field(&self.2)
            .finish()
    }
}

/// `petgraph::algo::Cycle`: `toposort`'s error, a node on a cycle.
#[derive(Clone, PartialEq)]
pub struct PgCycle<N>(N);
impl<N: Copy> PgCycle<N> {
    pub fn node_id(&self) -> N {
        self.0
    }
}
impl<N: std::fmt::Debug> std::fmt::Debug for PgCycle<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_tuple("Cycle").field(&self.0).finish()
    }
}

/// `petgraph::algo::NegativeCycle`: `bellman_ford`'s error.
#[derive(Clone, PartialEq)]
pub struct PgNegativeCycle(pub ());
impl std::fmt::Debug for PgNegativeCycle {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "NegativeCycle(())")
    }
}

/// `petgraph::algo::bellman_ford::Paths`.
#[derive(Clone)]
pub struct PgPaths<NodeId, EdgeWeight> {
    pub distances: Vec<EdgeWeight>,
    pub predecessors: Vec<Option<NodeId>>,
}
impl<NodeId: std::fmt::Debug, EdgeWeight: std::fmt::Debug> std::fmt::Debug
    for PgPaths<NodeId, EdgeWeight>
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Paths")
            .field("distances", &self.distances)
            .field("predecessors", &self.predecessors)
            .finish()
    }
}

/// `petgraph::algo::DfsSpace` (accepted; the algorithms make their own).
pub struct PgDfsSpace;
impl PgDfsSpace {
    pub fn new<G>(_g: G) -> PgDfsSpace {
        PgDfsSpace
    }
}

/// `petgraph::data::Element`: what `min_spanning_tree` yields.
#[derive(Debug, Clone, PartialEq)]
pub enum PgElement<N, E> {
    Node {
        weight: N,
    },
    Edge {
        source: usize,
        target: usize,
        weight: E,
    },
}

/// The float weights `bellman_ford` takes (`FloatMeasure`).
pub trait PgFloatMeasure: Copy + PartialOrd + Default + std::ops::Add<Output = Self> {
    fn __pg_infinite() -> Self;
}
impl PgFloatMeasure for f64 {
    fn __pg_infinite() -> f64 {
        f64::INFINITY
    }
}
impl PgFloatMeasure for f32 {
    fn __pg_infinite() -> f32 {
        f32::INFINITY
    }
}

// ---------------------------------------------------------------- Graph

#[derive(Clone)]
struct PgNode<N> {
    weight: N,
    next: [PgEdgeIndex; 2],
}
#[derive(Clone)]
struct PgEdge<E> {
    weight: E,
    next: [PgEdgeIndex; 2],
    node: [PgNodeIndex; 2],
}

/// `petgraph::Graph`: nodes and edges in `Vec`s, each node with the heads of its outgoing and
/// incoming edge lists (so the newest edge comes first).
#[derive(Clone)]
pub struct PgGraph<N, E, Ty = PgDirected> {
    nodes: Vec<PgNode<N>>,
    edges: Vec<PgEdge<E>>,
    directed: bool,
    ty: Option<Ty>,
}

impl<N, E> PgGraph<N, E, PgDirected> {
    pub fn new() -> PgGraph<N, E, PgDirected> {
        PgGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            directed: true,
            ty: None,
        }
    }
}
impl<N, E> PgGraph<N, E, PgUndirected> {
    pub fn new_undirected() -> PgGraph<N, E, PgUndirected> {
        PgGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            directed: false,
            ty: None,
        }
    }
}
impl<N, E, Ty: PgEdgeType> Default for PgGraph<N, E, Ty> {
    fn default() -> Self {
        PgGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            directed: Ty::is_directed(),
            ty: None,
        }
    }
}

impl<N, E, Ty: PgEdgeType> PgGraph<N, E, Ty> {
    pub fn with_capacity(_nodes: usize, _edges: usize) -> PgGraph<N, E, Ty> {
        PgGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            directed: Ty::is_directed(),
            ty: None,
        }
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
    pub fn is_directed(&self) -> bool {
        self.directed
    }
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
    }
    pub fn clear_edges(&mut self) {
        self.edges.clear();
        for i in 0..self.nodes.len() {
            self.nodes[i].next = [PgEdgeIndex::end(), PgEdgeIndex::end()];
        }
    }
    pub fn add_node(&mut self, weight: N) -> PgNodeIndex {
        let ix = PgNodeIndex::new(self.nodes.len());
        self.nodes.push(PgNode {
            weight,
            next: [PgEdgeIndex::end(), PgEdgeIndex::end()],
        });
        ix
    }
    pub fn node_weight(&self, a: PgNodeIndex) -> Option<&N> {
        self.nodes.get(a.index()).map(|n| &n.weight)
    }
    pub fn node_weight_mut(&mut self, a: PgNodeIndex) -> Option<&mut N> {
        self.nodes.get_mut(a.index()).map(|n| &mut n.weight)
    }
    pub fn add_edge(&mut self, a: PgNodeIndex, b: PgNodeIndex, weight: E) -> PgEdgeIndex {
        let ix = PgEdgeIndex::new(self.edges.len());
        if a.index().max(b.index()) >= self.nodes.len() {
            panic!("Graph::add_edge: node indices out of bounds");
        }
        let next;
        if a == b {
            let an = self.nodes[a.index()].next;
            next = an;
            self.nodes[a.index()].next = [ix, ix];
        } else {
            let an = self.nodes[a.index()].next;
            let bn = self.nodes[b.index()].next;
            next = [an[0], bn[1]];
            self.nodes[a.index()].next = [ix, an[1]];
            self.nodes[b.index()].next = [bn[0], ix];
        }
        self.edges.push(PgEdge {
            weight,
            next,
            node: [a, b],
        });
        ix
    }
    pub fn update_edge(&mut self, a: PgNodeIndex, b: PgNodeIndex, weight: E) -> PgEdgeIndex {
        if let Some(ix) = self.find_edge(a, b) {
            self.edges[ix.index()].weight = weight;
            return ix;
        }
        self.add_edge(a, b, weight)
    }
    pub fn edge_weight(&self, e: PgEdgeIndex) -> Option<&E> {
        self.edges.get(e.index()).map(|x| &x.weight)
    }
    pub fn edge_weight_mut(&mut self, e: PgEdgeIndex) -> Option<&mut E> {
        self.edges.get_mut(e.index()).map(|x| &mut x.weight)
    }
    pub fn edge_endpoints(&self, e: PgEdgeIndex) -> Option<(PgNodeIndex, PgNodeIndex)> {
        self.edges.get(e.index()).map(|x| (x.node[0], x.node[1]))
    }
    /// The edges after `e` in its lists: where the link to `e` is, replace it with `next`.
    fn __pg_change_edge_links(
        &mut self,
        edge_node: [PgNodeIndex; 2],
        e: PgEdgeIndex,
        edge_next: [PgEdgeIndex; 2],
    ) {
        for k in 0..2 {
            let n = edge_node[k].index();
            if n >= self.nodes.len() {
                return;
            }
            let fst = self.nodes[n].next[k];
            if fst == e {
                let mut nn = self.nodes[n].next;
                nn[k] = edge_next[k];
                self.nodes[n].next = nn;
            } else {
                let mut cur = fst;
                while cur.index() < self.edges.len() {
                    let mut cn = self.edges[cur.index()].next;
                    if cn[k] == e {
                        cn[k] = edge_next[k];
                        self.edges[cur.index()].next = cn;
                        break;
                    }
                    cur = cn[k];
                }
            }
        }
    }
    pub fn remove_edge(&mut self, e: PgEdgeIndex) -> Option<E> {
        if e.index() >= self.edges.len() {
            return None;
        }
        let edge_node = self.edges[e.index()].node;
        let edge_next = self.edges[e.index()].next;
        self.__pg_change_edge_links(edge_node, e, edge_next);
        let edge = self.edges.swap_remove(e.index());
        if e.index() >= self.edges.len() {
            return Some(edge.weight);
        }
        let swap = self.edges[e.index()].node;
        let swapped_e = PgEdgeIndex::new(self.edges.len());
        self.__pg_change_edge_links(swap, swapped_e, [e, e]);
        Some(edge.weight)
    }
    pub fn remove_node(&mut self, a: PgNodeIndex) -> Option<N> {
        if a.index() >= self.nodes.len() {
            return None;
        }
        for k in 0..2 {
            loop {
                let next = self.nodes[a.index()].next[k];
                if next == PgEdgeIndex::end() {
                    break;
                }
                self.remove_edge(next);
            }
        }
        let node = self.nodes.swap_remove(a.index());
        if a.index() >= self.nodes.len() {
            return Some(node.weight);
        }
        let swap_edges = self.nodes[a.index()].next;
        for k in 0..2 {
            let mut cur = swap_edges[k];
            while cur.index() < self.edges.len() {
                let mut nd = self.edges[cur.index()].node;
                nd[k] = a;
                self.edges[cur.index()].node = nd;
                cur = self.edges[cur.index()].next[k];
            }
        }
        Some(node.weight)
    }
    /// The edges at `a` in `dir` as petgraph's `Edges` iterates them: for an undirected graph
    /// both lists (self loops once), each with `a` as its source.
    fn __pg_edge_list(
        &self,
        a: PgNodeIndex,
        dir: PgDirection,
    ) -> Vec<(PgEdgeIndex, [PgNodeIndex; 2])> {
        let mut out = Vec::new();
        if a.index() >= self.nodes.len() {
            return out;
        }
        let next = self.nodes[a.index()].next;
        let both = !self.directed;
        let reverse_out = both && dir == PgDirection::Incoming;
        let reverse_in = both && dir == PgDirection::Outgoing;
        if both || dir == PgDirection::Outgoing {
            let mut e = next[0];
            while e.index() < self.edges.len() {
                let edge = &self.edges[e.index()];
                let node = if reverse_out {
                    [edge.node[1], edge.node[0]]
                } else {
                    edge.node
                };
                out.push((e, node));
                e = edge.next[0];
            }
        }
        if both || dir == PgDirection::Incoming {
            let mut e = next[1];
            while e.index() < self.edges.len() {
                let edge = &self.edges[e.index()];
                let skip = both && edge.node[0] == a;
                if !skip {
                    let node = if reverse_in {
                        [edge.node[1], edge.node[0]]
                    } else {
                        edge.node
                    };
                    out.push((e, node));
                }
                e = edge.next[1];
            }
        }
        out
    }
    pub fn neighbors(&self, a: PgNodeIndex) -> std::vec::IntoIter<PgNodeIndex> {
        self.neighbors_directed(a, PgDirection::Outgoing)
    }
    pub fn neighbors_directed(
        &self,
        a: PgNodeIndex,
        dir: PgDirection,
    ) -> std::vec::IntoIter<PgNodeIndex> {
        let mut out = Vec::new();
        for (_, node) in self.__pg_edge_list(a, dir) {
            // (The other end: for an incoming edge of a directed graph, the source.)
            if self.directed && dir == PgDirection::Incoming {
                out.push(node[0]);
            } else {
                out.push(node[1]);
            }
        }
        out.into_iter()
    }
    pub fn neighbors_undirected(&self, a: PgNodeIndex) -> std::vec::IntoIter<PgNodeIndex> {
        let mut out = Vec::new();
        if a.index() < self.nodes.len() {
            let next = self.nodes[a.index()].next;
            let mut e = next[0];
            while e.index() < self.edges.len() {
                out.push(self.edges[e.index()].node[1]);
                e = self.edges[e.index()].next[0];
            }
            let mut e = next[1];
            while e.index() < self.edges.len() {
                if self.edges[e.index()].node[0] != a {
                    out.push(self.edges[e.index()].node[0]);
                }
                e = self.edges[e.index()].next[1];
            }
        }
        out.into_iter()
    }
    pub fn edges(&self, a: PgNodeIndex) -> std::vec::IntoIter<PgEdgeReference<'_, E>> {
        self.edges_directed(a, PgDirection::Outgoing)
    }
    pub fn edges_directed(
        &self,
        a: PgNodeIndex,
        dir: PgDirection,
    ) -> std::vec::IntoIter<PgEdgeReference<'_, E>> {
        let mut out = Vec::new();
        for (e, node) in self.__pg_edge_list(a, dir) {
            out.push(PgEdgeReference {
                index: e,
                node,
                weight: &self.edges[e.index()].weight,
            });
        }
        out.into_iter()
    }
    pub fn edges_connecting(
        &self,
        a: PgNodeIndex,
        b: PgNodeIndex,
    ) -> std::vec::IntoIter<PgEdgeReference<'_, E>> {
        let mut out = Vec::new();
        for (e, node) in self.__pg_edge_list(a, PgDirection::Outgoing) {
            if node[1] == b {
                out.push(PgEdgeReference {
                    index: e,
                    node,
                    weight: &self.edges[e.index()].weight,
                });
            }
        }
        out.into_iter()
    }
    pub fn edge_references(&self) -> std::vec::IntoIter<PgEdgeReference<'_, E>> {
        let mut out = Vec::new();
        for i in 0..self.edges.len() {
            out.push(PgEdgeReference {
                index: PgEdgeIndex::new(i),
                node: self.edges[i].node,
                weight: &self.edges[i].weight,
            });
        }
        out.into_iter()
    }
    pub fn find_edge(&self, a: PgNodeIndex, b: PgNodeIndex) -> Option<PgEdgeIndex> {
        if !self.directed {
            return self.find_edge_undirected(a, b).map(|(e, _)| e);
        }
        if a.index() >= self.nodes.len() {
            return None;
        }
        let mut e = self.nodes[a.index()].next[0];
        while e.index() < self.edges.len() {
            if self.edges[e.index()].node[1] == b {
                return Some(e);
            }
            e = self.edges[e.index()].next[0];
        }
        None
    }
    pub fn find_edge_undirected(
        &self,
        a: PgNodeIndex,
        b: PgNodeIndex,
    ) -> Option<(PgEdgeIndex, PgDirection)> {
        if a.index() >= self.nodes.len() {
            return None;
        }
        for d in [PgDirection::Outgoing, PgDirection::Incoming] {
            let k = d.index();
            let mut e = self.nodes[a.index()].next[k];
            while e.index() < self.edges.len() {
                if self.edges[e.index()].node[1 - k] == b {
                    return Some((e, d));
                }
                e = self.edges[e.index()].next[k];
            }
        }
        None
    }
    pub fn contains_edge(&self, a: PgNodeIndex, b: PgNodeIndex) -> bool {
        self.find_edge(a, b).is_some()
    }
    pub fn externals(&self, dir: PgDirection) -> std::vec::IntoIter<PgNodeIndex> {
        let k = dir.index();
        let mut out = Vec::new();
        for i in 0..self.nodes.len() {
            let next = self.nodes[i].next;
            if next[k] == PgEdgeIndex::end() && (self.directed || next[1 - k] == PgEdgeIndex::end())
            {
                out.push(PgNodeIndex::new(i));
            }
        }
        out.into_iter()
    }
    pub fn node_indices(&self) -> std::vec::IntoIter<PgNodeIndex> {
        let mut out = Vec::new();
        for i in 0..self.nodes.len() {
            out.push(PgNodeIndex::new(i));
        }
        out.into_iter()
    }
    pub fn edge_indices(&self) -> std::vec::IntoIter<PgEdgeIndex> {
        let mut out = Vec::new();
        for i in 0..self.edges.len() {
            out.push(PgEdgeIndex::new(i));
        }
        out.into_iter()
    }
    pub fn node_weights(&self) -> std::vec::IntoIter<&N> {
        let mut out = Vec::new();
        for n in &self.nodes {
            out.push(&n.weight);
        }
        out.into_iter()
    }
    pub fn edge_weights(&self) -> std::vec::IntoIter<&E> {
        let mut out = Vec::new();
        for e in &self.edges {
            out.push(&e.weight);
        }
        out.into_iter()
    }
    pub fn reverse(&mut self) {
        for i in 0..self.edges.len() {
            let n = self.edges[i].node;
            let x = self.edges[i].next;
            self.edges[i].node = [n[1], n[0]];
            self.edges[i].next = [x[1], x[0]];
        }
        for i in 0..self.nodes.len() {
            let x = self.nodes[i].next;
            self.nodes[i].next = [x[1], x[0]];
        }
    }
    pub fn from_elements<I: IntoIterator<Item = PgElement<N, E>>>(
        iterable: I,
    ) -> PgGraph<N, E, Ty> {
        let mut g = PgGraph::with_capacity(0, 0);
        for elt in iterable {
            match elt {
                PgElement::Node { weight } => {
                    g.add_node(weight);
                }
                PgElement::Edge {
                    source,
                    target,
                    weight,
                } => {
                    g.add_edge(PgNodeIndex::new(source), PgNodeIndex::new(target), weight);
                }
            }
        }
        g
    }
    fn __pg_view(&self) -> PgView<PgNodeIndex, PgEdgeReference<'_, E>> {
        let n = self.nodes.len();
        let mut v = PgView {
            ids: Vec::new(),
            out: Vec::new(),
            inc: Vec::new(),
            refs: Vec::new(),
        };
        for i in 0..n {
            let a = PgNodeIndex::new(i);
            v.ids.push(a);
            let mut out = Vec::new();
            for r in self.edges(a) {
                out.push((r.target().index(), r));
            }
            v.out.push(out);
            let mut inc = Vec::new();
            for b in self.neighbors_directed(a, PgDirection::Incoming) {
                inc.push(b.index());
            }
            v.inc.push(inc);
        }
        for r in self.edge_references() {
            v.refs.push((r.source().index(), r.target().index(), r));
        }
        v
    }
    pub fn __pg_dijkstra<'a, K, F>(
        &'a self,
        start: PgNodeIndex,
        goal: Option<PgNodeIndex>,
        edge_cost: F,
    ) -> std::collections::HashMap<PgNodeIndex, K>
    where
        K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
        F: FnMut(PgEdgeReference<'a, E>) -> K,
    {
        let v = self.__pg_view();
        __pg_dijkstra_in(&v, start.index(), goal.map(|g| g.index()), edge_cost)
    }
    pub fn __pg_astar<'a, K, IsGoal, F, H>(
        &'a self,
        start: PgNodeIndex,
        is_goal: IsGoal,
        edge_cost: F,
        estimate_cost: H,
    ) -> Option<(K, Vec<PgNodeIndex>)>
    where
        K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
        IsGoal: FnMut(PgNodeIndex) -> bool,
        F: FnMut(PgEdgeReference<'a, E>) -> K,
        H: FnMut(PgNodeIndex) -> K,
    {
        let v = self.__pg_view();
        __pg_astar_in(&v, start.index(), is_goal, edge_cost, estimate_cost)
    }
    pub fn __pg_toposort(
        &self,
        _space: Option<&mut PgDfsSpace>,
    ) -> Result<Vec<PgNodeIndex>, PgCycle<PgNodeIndex>> {
        __pg_toposort_in(&self.__pg_view())
    }
    pub fn __pg_is_cyclic_directed(&self) -> bool {
        __pg_is_cyclic_directed_in(&self.__pg_view())
    }
    pub fn __pg_is_cyclic_undirected(&self) -> bool {
        __pg_is_cyclic_undirected_in(&self.__pg_view())
    }
    pub fn __pg_connected_components(&self) -> usize {
        __pg_connected_components_in(&self.__pg_view())
    }
    pub fn __pg_kosaraju_scc(&self) -> Vec<Vec<PgNodeIndex>> {
        __pg_kosaraju_scc_in(&self.__pg_view())
    }
    pub fn __pg_tarjan_scc(&self) -> Vec<Vec<PgNodeIndex>> {
        __pg_tarjan_scc_in(&self.__pg_view())
    }
    pub fn __pg_has_path_connecting(
        &self,
        from: PgNodeIndex,
        to: PgNodeIndex,
        _space: Option<&mut PgDfsSpace>,
    ) -> bool {
        __pg_has_path_connecting_in(&self.__pg_view(), from.index(), to.index())
    }
}

impl<N: Default, E, Ty: PgEdgeType> PgGraph<N, E, Ty> {
    /// `extend_with_edges(&[(0, 1, w), ..])` (a missing node is added with `N::default()`).
    pub fn extend_with_edges<X: PgIntoNodeIndex, I: IntoIterator<Item = (X, X, E)>>(
        &mut self,
        iterable: I,
    ) {
        for (a, b, w) in iterable {
            let a: PgNodeIndex = a.__pg_into_ix();
            let b: PgNodeIndex = b.__pg_into_ix();
            while a.index().max(b.index()) >= self.nodes.len() {
                self.add_node(N::default());
            }
            self.add_edge(a, b, w);
        }
    }
    pub fn from_edges<X: PgIntoNodeIndex, I: IntoIterator<Item = (X, X, E)>>(
        iterable: I,
    ) -> PgGraph<N, E, Ty> {
        let mut g = PgGraph::with_capacity(0, 0);
        g.extend_with_edges(iterable);
        g
    }
}

impl<N: Default, E: Default, Ty: PgEdgeType> PgGraph<N, E, Ty> {
    /// `extend_with_edges(&[(0, 1), ..])`: edges with `E::default()` weights.
    pub fn __pg_extend_with_edges2<X: PgIntoNodeIndex, I: IntoIterator<Item = (X, X)>>(
        &mut self,
        iterable: I,
    ) {
        for (a, b) in iterable {
            let a: PgNodeIndex = a.__pg_into_ix();
            let b: PgNodeIndex = b.__pg_into_ix();
            while a.index().max(b.index()) >= self.nodes.len() {
                self.add_node(N::default());
            }
            self.add_edge(a, b, E::default());
        }
    }
    pub fn __pg_from_edges2<X: PgIntoNodeIndex, I: IntoIterator<Item = (X, X)>>(
        iterable: I,
    ) -> PgGraph<N, E, Ty> {
        let mut g = PgGraph::with_capacity(0, 0);
        g.__pg_extend_with_edges2(iterable);
        g
    }
}

impl<N, E: PgFloatMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = E>, Ty: PgEdgeType>
    PgGraph<N, E, Ty>
{
    pub fn __pg_bellman_ford(
        &self,
        source: PgNodeIndex,
    ) -> Result<PgPaths<PgNodeIndex, E>, PgNegativeCycle> {
        __pg_bellman_ford_in(&self.__pg_view(), source.index(), |r| *r.weight())
    }
}

impl<N: Clone, E: Clone + PartialOrd, Ty: PgEdgeType> PgGraph<N, E, Ty> {
    pub fn __pg_min_spanning_tree(&self) -> std::vec::IntoIter<PgElement<N, E>> {
        let mut nodes = Vec::new();
        for n in &self.nodes {
            nodes.push(n.weight.clone());
        }
        let mut edges = Vec::new();
        for e in &self.edges {
            edges.push((e.node[0].index(), e.node[1].index(), e.weight.clone()));
        }
        __pg_min_spanning_tree_in(nodes, edges).into_iter()
    }
}

impl<N, E, Ty> std::ops::Index<PgNodeIndex> for PgGraph<N, E, Ty> {
    type Output = N;
    fn index(&self, i: PgNodeIndex) -> &N {
        &self.nodes[i.index()].weight
    }
}
impl<N, E, Ty> std::ops::IndexMut<PgNodeIndex> for PgGraph<N, E, Ty> {
    fn index_mut(&mut self, i: PgNodeIndex) -> &mut N {
        &mut self.nodes[i.index()].weight
    }
}
impl<N, E, Ty> std::ops::Index<PgEdgeIndex> for PgGraph<N, E, Ty> {
    type Output = E;
    fn index(&self, i: PgEdgeIndex) -> &E {
        &self.edges[i.index()].weight
    }
}
impl<N, E, Ty> std::ops::IndexMut<PgEdgeIndex> for PgGraph<N, E, Ty> {
    fn index_mut(&mut self, i: PgEdgeIndex) -> &mut E {
        &mut self.edges[i.index()].weight
    }
}

// ---------------------------------------------------------------- GraphMap

/// `petgraph::graphmap::GraphMap`: the nodes are the keys (`Copy + Ord + Hash`), in insertion
/// order, each with its neighbors in insertion order; the edges are keyed by their ends (the
/// smaller one first when undirected).
#[derive(Clone)]
pub struct PgGraphMap<N, E, Ty = PgDirected> {
    nodes: Vec<N>,
    adj: Vec<Vec<(N, PgDirection)>>,
    node_ix: std::collections::HashMap<N, usize>,
    edges: Vec<(N, N, E)>,
    edge_ix: std::collections::HashMap<(N, N), usize>,
    directed: bool,
    ty: Option<Ty>,
}

impl<N: Copy + Ord + std::hash::Hash, E, Ty: PgEdgeType> Default for PgGraphMap<N, E, Ty> {
    fn default() -> Self {
        PgGraphMap::new()
    }
}

impl<N: Copy + Ord + std::hash::Hash, E, Ty: PgEdgeType> PgGraphMap<N, E, Ty> {
    pub fn new() -> PgGraphMap<N, E, Ty> {
        PgGraphMap {
            nodes: Vec::new(),
            adj: Vec::new(),
            node_ix: std::collections::HashMap::new(),
            edges: Vec::new(),
            edge_ix: std::collections::HashMap::new(),
            directed: Ty::is_directed(),
            ty: None,
        }
    }
    pub fn with_capacity(_nodes: usize, _edges: usize) -> PgGraphMap<N, E, Ty> {
        PgGraphMap::new()
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
    pub fn is_directed(&self) -> bool {
        self.directed
    }
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.adj.clear();
        self.node_ix.clear();
        self.edges.clear();
        self.edge_ix.clear();
    }
    fn __pg_key(&self, a: N, b: N) -> (N, N) {
        if self.directed || a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }
    pub fn add_node(&mut self, n: N) -> N {
        if !self.node_ix.contains_key(&n) {
            self.node_ix.insert(n, self.nodes.len());
            self.nodes.push(n);
            self.adj.push(Vec::new());
        }
        n
    }
    pub fn contains_node(&self, n: N) -> bool {
        self.node_ix.contains_key(&n)
    }
    pub fn add_edge(&mut self, a: N, b: N, weight: E) -> Option<E> {
        let key = self.__pg_key(a, b);
        if let Some(&i) = self.edge_ix.get(&key) {
            let old = std::mem::replace(&mut self.edges[i].2, weight);
            return Some(old);
        }
        self.edge_ix.insert(key, self.edges.len());
        self.edges.push((key.0, key.1, weight));
        let ia = self.add_node(a);
        let ia = self.node_ix[&ia];
        self.adj[ia].push((b, PgDirection::Outgoing));
        if a != b {
            let ib = self.add_node(b);
            let ib = self.node_ix[&ib];
            self.adj[ib].push((a, PgDirection::Incoming));
        }
        None
    }
    pub fn contains_edge(&self, a: N, b: N) -> bool {
        self.edge_ix.contains_key(&self.__pg_key(a, b))
    }
    pub fn edge_weight(&self, a: N, b: N) -> Option<&E> {
        match self.edge_ix.get(&self.__pg_key(a, b)) {
            Some(&i) => Some(&self.edges[i].2),
            None => None,
        }
    }
    pub fn edge_weight_mut(&mut self, a: N, b: N) -> Option<&mut E> {
        match self.edge_ix.get(&self.__pg_key(a, b)) {
            Some(&i) => Some(&mut self.edges[i].2),
            None => None,
        }
    }
    /// One adjacency entry of `a` (`IndexMap::swap_remove`'s order: the last moves into the hole).
    fn __pg_unlink(&mut self, a: N, b: N, dir: PgDirection) -> bool {
        let Some(&ia) = self.node_ix.get(&a) else {
            return false;
        };
        let directed = self.directed;
        let pos = self.adj[ia]
            .iter()
            .position(|&(n, d)| n == b && (!directed || d == dir));
        match pos {
            Some(p) => {
                self.adj[ia].swap_remove(p);
                true
            }
            None => false,
        }
    }
    fn __pg_take_edge(&mut self, key: (N, N)) -> Option<E> {
        let i = self.edge_ix.remove(&key)?;
        let removed = self.edges.swap_remove(i);
        if i < self.edges.len() {
            let moved = (self.edges[i].0, self.edges[i].1);
            self.edge_ix.insert(moved, i);
        }
        Some(removed.2)
    }
    pub fn remove_edge(&mut self, a: N, b: N) -> Option<E> {
        self.__pg_unlink(a, b, PgDirection::Outgoing);
        if a != b {
            self.__pg_unlink(b, a, PgDirection::Incoming);
        }
        let key = self.__pg_key(a, b);
        self.__pg_take_edge(key)
    }
    pub fn remove_node(&mut self, n: N) -> bool {
        let Some(i) = self.node_ix.remove(&n) else {
            return false;
        };
        self.nodes.swap_remove(i);
        let links = self.adj.swap_remove(i);
        if i < self.nodes.len() {
            let moved = self.nodes[i];
            self.node_ix.insert(moved, i);
        }
        for (succ, dir) in links {
            let key = if dir == PgDirection::Outgoing {
                self.__pg_key(n, succ)
            } else {
                self.__pg_key(succ, n)
            };
            self.__pg_unlink(succ, n, dir.opposite());
            self.__pg_take_edge(key);
        }
        true
    }
    pub fn nodes(&self) -> std::vec::IntoIter<N> {
        self.nodes.clone().into_iter()
    }
    pub fn neighbors(&self, a: N) -> std::vec::IntoIter<N> {
        let mut out = Vec::new();
        if let Some(&i) = self.node_ix.get(&a) {
            for &(n, d) in &self.adj[i] {
                if !self.directed || d == PgDirection::Outgoing {
                    out.push(n);
                }
            }
        }
        out.into_iter()
    }
    pub fn neighbors_directed(&self, a: N, dir: PgDirection) -> std::vec::IntoIter<N> {
        let mut out = Vec::new();
        if let Some(&i) = self.node_ix.get(&a) {
            for &(n, d) in &self.adj[i] {
                if !self.directed || d == dir || n == a {
                    out.push(n);
                }
            }
        }
        out.into_iter()
    }
    pub fn edges(&self, a: N) -> std::vec::IntoIter<(N, N, &E)> {
        let mut out = Vec::new();
        for b in self.neighbors(a) {
            let i = self.edge_ix[&self.__pg_key(a, b)];
            out.push((a, b, &self.edges[i].2));
        }
        out.into_iter()
    }
    pub fn edges_directed(&self, a: N, dir: PgDirection) -> std::vec::IntoIter<(N, N, &E)> {
        let mut out = Vec::new();
        for b in self.neighbors_directed(a, dir) {
            let (s, t) = if dir == PgDirection::Incoming {
                (b, a)
            } else {
                (a, b)
            };
            let i = self.edge_ix[&self.__pg_key(s, t)];
            out.push((s, t, &self.edges[i].2));
        }
        out.into_iter()
    }
    pub fn all_edges(&self) -> std::vec::IntoIter<(N, N, &E)> {
        let mut out = Vec::new();
        for e in &self.edges {
            out.push((e.0, e.1, &e.2));
        }
        out.into_iter()
    }
    pub fn from_edges<I: IntoIterator<Item = (N, N, E)>>(iterable: I) -> PgGraphMap<N, E, Ty> {
        let mut g = PgGraphMap::new();
        g.extend(iterable);
        g
    }
    pub fn extend<I: IntoIterator<Item = (N, N, E)>>(&mut self, iterable: I) {
        for (a, b, w) in iterable {
            self.add_edge(a, b, w);
        }
    }
    fn __pg_view(&self) -> PgView<N, PgMapEdgeRef<'_, N, E>> {
        let mut v = PgView {
            ids: self.nodes.clone(),
            out: Vec::new(),
            inc: Vec::new(),
            refs: Vec::new(),
        };
        for &a in &self.nodes {
            let mut out = Vec::new();
            for (s, t, w) in self.edges(a) {
                out.push((self.node_ix[&t], PgMapEdgeRef(s, t, w)));
            }
            v.out.push(out);
            let mut inc = Vec::new();
            for b in self.neighbors_directed(a, PgDirection::Incoming) {
                inc.push(self.node_ix[&b]);
            }
            v.inc.push(inc);
        }
        for e in &self.edges {
            v.refs.push((
                self.node_ix[&e.0],
                self.node_ix[&e.1],
                PgMapEdgeRef(e.0, e.1, &e.2),
            ));
        }
        v
    }
    fn __pg_ix(&self, n: N) -> usize {
        match self.node_ix.get(&n) {
            Some(&i) => i,
            None => usize::MAX,
        }
    }
    pub fn __pg_dijkstra<'a, K, F>(
        &'a self,
        start: N,
        goal: Option<N>,
        edge_cost: F,
    ) -> std::collections::HashMap<N, K>
    where
        K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
        F: FnMut(PgMapEdgeRef<'a, N, E>) -> K,
    {
        if !self.contains_node(start) {
            let mut m = std::collections::HashMap::new();
            m.insert(start, K::default());
            return m;
        }
        let v = self.__pg_view();
        __pg_dijkstra_in(
            &v,
            self.__pg_ix(start),
            goal.map(|g| self.__pg_ix(g)),
            edge_cost,
        )
    }
    pub fn __pg_astar<'a, K, IsGoal, F, H>(
        &'a self,
        start: N,
        is_goal: IsGoal,
        edge_cost: F,
        estimate_cost: H,
    ) -> Option<(K, Vec<N>)>
    where
        K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
        IsGoal: FnMut(N) -> bool,
        F: FnMut(PgMapEdgeRef<'a, N, E>) -> K,
        H: FnMut(N) -> K,
    {
        if !self.contains_node(start) {
            return None;
        }
        let v = self.__pg_view();
        __pg_astar_in(&v, self.__pg_ix(start), is_goal, edge_cost, estimate_cost)
    }
    pub fn __pg_toposort(&self, _space: Option<&mut PgDfsSpace>) -> Result<Vec<N>, PgCycle<N>> {
        __pg_toposort_in(&self.__pg_view())
    }
    pub fn __pg_is_cyclic_directed(&self) -> bool {
        __pg_is_cyclic_directed_in(&self.__pg_view())
    }
    pub fn __pg_is_cyclic_undirected(&self) -> bool {
        __pg_is_cyclic_undirected_in(&self.__pg_view())
    }
    pub fn __pg_connected_components(&self) -> usize {
        __pg_connected_components_in(&self.__pg_view())
    }
    pub fn __pg_kosaraju_scc(&self) -> Vec<Vec<N>> {
        __pg_kosaraju_scc_in(&self.__pg_view())
    }
    pub fn __pg_tarjan_scc(&self) -> Vec<Vec<N>> {
        __pg_tarjan_scc_in(&self.__pg_view())
    }
    pub fn __pg_has_path_connecting(
        &self,
        from: N,
        to: N,
        _space: Option<&mut PgDfsSpace>,
    ) -> bool {
        if !self.contains_node(from) || !self.contains_node(to) {
            return from == to;
        }
        __pg_has_path_connecting_in(&self.__pg_view(), self.__pg_ix(from), self.__pg_ix(to))
    }
}

impl<N: Copy + Ord + std::hash::Hash, E: Default, Ty: PgEdgeType> PgGraphMap<N, E, Ty> {
    /// `from_edges(&[(a, b), ..])`: edges with `E::default()` weights.
    pub fn __pg_from_edges2<I: IntoIterator<Item = (N, N)>>(iterable: I) -> PgGraphMap<N, E, Ty> {
        let mut g = PgGraphMap::new();
        for (a, b) in iterable {
            g.add_edge(a, b, E::default());
        }
        g
    }
}

impl<
    N: Copy + Ord + std::hash::Hash,
    E: PgFloatMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = E>,
    Ty: PgEdgeType,
> PgGraphMap<N, E, Ty>
{
    pub fn __pg_bellman_ford(&self, source: N) -> Result<PgPaths<N, E>, PgNegativeCycle> {
        __pg_bellman_ford_in(&self.__pg_view(), self.__pg_ix(source), |r| *r.weight())
    }
}

impl<N: Copy + Ord + std::hash::Hash, E: Clone + PartialOrd, Ty: PgEdgeType> PgGraphMap<N, E, Ty> {
    pub fn __pg_min_spanning_tree(&self) -> std::vec::IntoIter<PgElement<N, E>> {
        let mut edges = Vec::new();
        for e in &self.edges {
            edges.push((self.node_ix[&e.0], self.node_ix[&e.1], e.2.clone()));
        }
        __pg_min_spanning_tree_in(self.nodes.clone(), edges).into_iter()
    }
}

impl<N: Copy + Ord + std::hash::Hash, E, Ty: PgEdgeType> std::ops::Index<(N, N)>
    for PgGraphMap<N, E, Ty>
{
    type Output = E;
    fn index(&self, ix: (N, N)) -> &E {
        match self.edge_ix.get(&self.__pg_key(ix.0, ix.1)) {
            Some(&i) => &self.edges[i].2,
            None => panic!("no such edge"),
        }
    }
}
impl<N: Copy + Ord + std::hash::Hash, E, Ty: PgEdgeType> std::ops::IndexMut<(N, N)>
    for PgGraphMap<N, E, Ty>
{
    fn index_mut(&mut self, ix: (N, N)) -> &mut E {
        let key = self.__pg_key(ix.0, ix.1);
        let i = match self.edge_ix.get(&key) {
            Some(&i) => i,
            None => panic!("no such edge"),
        };
        &mut self.edges[i].2
    }
}

// ---------------------------------------------------------------- walkers

/// `petgraph::visit::Bfs`: `while let Some(n) = bfs.next(&g)`.
#[derive(Clone)]
pub struct PgBfs<N> {
    pub stack: std::collections::VecDeque<N>,
    pub discovered: std::collections::HashSet<N>,
}
impl<N: Copy + Eq + std::hash::Hash> PgBfs<N> {
    pub fn new<G>(_graph: G, start: N) -> PgBfs<N> {
        let mut discovered = std::collections::HashSet::new();
        discovered.insert(start);
        let mut stack = std::collections::VecDeque::new();
        stack.push_front(start);
        PgBfs { stack, discovered }
    }
    pub fn __pg_next<F: FnMut(N) -> Vec<N>>(&mut self, mut neighbors: F) -> Option<N> {
        if let Some(node) = self.stack.pop_front() {
            for succ in neighbors(node) {
                if self.discovered.insert(succ) {
                    self.stack.push_back(succ);
                }
            }
            return Some(node);
        }
        None
    }
}

/// `petgraph::visit::Dfs`.
#[derive(Clone)]
pub struct PgDfs<N> {
    pub stack: Vec<N>,
    pub discovered: std::collections::HashSet<N>,
}
impl<N: Copy + Eq + std::hash::Hash> PgDfs<N> {
    pub fn new<G>(_graph: G, start: N) -> PgDfs<N> {
        let mut dfs = PgDfs {
            stack: Vec::new(),
            discovered: std::collections::HashSet::new(),
        };
        dfs.move_to(start);
        dfs
    }
    pub fn empty<G>(_graph: G) -> PgDfs<N> {
        PgDfs {
            stack: Vec::new(),
            discovered: std::collections::HashSet::new(),
        }
    }
    pub fn reset<G>(&mut self, _graph: G) {
        self.discovered.clear();
        self.stack.clear();
    }
    pub fn move_to(&mut self, start: N) {
        self.stack.clear();
        self.stack.push(start);
    }
    pub fn __pg_next<F: FnMut(N) -> Vec<N>>(&mut self, mut neighbors: F) -> Option<N> {
        while let Some(node) = self.stack.pop() {
            if self.discovered.insert(node) {
                for succ in neighbors(node) {
                    if !self.discovered.contains(&succ) {
                        self.stack.push(succ);
                    }
                }
                return Some(node);
            }
        }
        None
    }
}

/// `petgraph::visit::DfsPostOrder`.
#[derive(Clone)]
pub struct PgDfsPostOrder<N> {
    pub stack: Vec<N>,
    pub discovered: std::collections::HashSet<N>,
    pub finished: std::collections::HashSet<N>,
}
impl<N: Copy + Eq + std::hash::Hash> PgDfsPostOrder<N> {
    pub fn new<G>(_graph: G, start: N) -> PgDfsPostOrder<N> {
        let mut dfs = PgDfsPostOrder {
            stack: Vec::new(),
            discovered: std::collections::HashSet::new(),
            finished: std::collections::HashSet::new(),
        };
        dfs.move_to(start);
        dfs
    }
    pub fn empty<G>(_graph: G) -> PgDfsPostOrder<N> {
        PgDfsPostOrder {
            stack: Vec::new(),
            discovered: std::collections::HashSet::new(),
            finished: std::collections::HashSet::new(),
        }
    }
    pub fn reset<G>(&mut self, _graph: G) {
        self.discovered.clear();
        self.finished.clear();
        self.stack.clear();
    }
    pub fn move_to(&mut self, start: N) {
        self.stack.clear();
        self.stack.push(start);
    }
    pub fn __pg_next<F: FnMut(N) -> Vec<N>>(&mut self, mut neighbors: F) -> Option<N> {
        while let Some(&nx) = self.stack.last() {
            if self.discovered.insert(nx) {
                for succ in neighbors(nx) {
                    if !self.discovered.contains(&succ) {
                        self.stack.push(succ);
                    }
                }
            } else {
                self.stack.pop();
                if self.finished.insert(nx) {
                    return Some(nx);
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------- algorithms

/// A graph as the algorithms see it: node ids by position, each node's `edges(n)` (with the
/// target's position), its incoming neighbors (for `Reversed`), and `edge_references()`.
pub struct PgView<Id, R> {
    ids: Vec<Id>,
    out: Vec<Vec<(usize, R)>>,
    inc: Vec<Vec<usize>>,
    refs: Vec<(usize, usize, R)>,
}

/// petgraph's `MinScored`: a min-heap entry by score (NaN last), ties equal.
struct PgMinScored<K>(K, usize);
impl<K: PartialOrd> PartialEq for PgMinScored<K> {
    fn eq(&self, other: &PgMinScored<K>) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
impl<K: PartialOrd> Eq for PgMinScored<K> {}
impl<K: PartialOrd> PartialOrd for PgMinScored<K> {
    fn partial_cmp(&self, other: &PgMinScored<K>) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<K: PartialOrd> Ord for PgMinScored<K> {
    #[allow(clippy::eq_op)]
    fn cmp(&self, other: &PgMinScored<K>) -> std::cmp::Ordering {
        let a = &self.0;
        let b = &other.0;
        if a == b {
            std::cmp::Ordering::Equal
        } else if a < b {
            std::cmp::Ordering::Greater
        } else if a > b {
            std::cmp::Ordering::Less
        } else if a != a && b != b {
            std::cmp::Ordering::Equal
        } else if a != a {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    }
}

fn __pg_dijkstra_in<Id, R, K, F>(
    v: &PgView<Id, R>,
    start: usize,
    goal: Option<usize>,
    mut edge_cost: F,
) -> std::collections::HashMap<Id, K>
where
    Id: Copy + Eq + std::hash::Hash,
    R: Copy,
    K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
    F: FnMut(R) -> K,
{
    let mut visited = vec![false; v.ids.len()];
    let mut scores: std::collections::HashMap<usize, K> = std::collections::HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    let mut visit_next = std::collections::BinaryHeap::new();
    let zero = K::default();
    scores.insert(start, zero);
    order.push(start);
    visit_next.push(PgMinScored(zero, start));
    while let Some(PgMinScored(node_score, node)) = visit_next.pop() {
        if visited[node] {
            continue;
        }
        if goal == Some(node) {
            break;
        }
        for &(next, r) in &v.out[node] {
            if visited[next] {
                continue;
            }
            let next_score = node_score + edge_cost(r);
            match scores.get(&next) {
                Some(&old) => {
                    if next_score < old {
                        scores.insert(next, next_score);
                        visit_next.push(PgMinScored(next_score, next));
                    }
                }
                None => {
                    scores.insert(next, next_score);
                    order.push(next);
                    visit_next.push(PgMinScored(next_score, next));
                }
            }
        }
        visited[node] = true;
    }
    let mut out = std::collections::HashMap::new();
    for i in order {
        out.insert(v.ids[i], scores[&i]);
    }
    out
}

fn __pg_astar_in<Id, R, K, IsGoal, F, H>(
    v: &PgView<Id, R>,
    start: usize,
    mut is_goal: IsGoal,
    mut edge_cost: F,
    mut estimate_cost: H,
) -> Option<(K, Vec<Id>)>
where
    Id: Copy,
    R: Copy,
    K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
    IsGoal: FnMut(Id) -> bool,
    F: FnMut(R) -> K,
    H: FnMut(Id) -> K,
{
    let mut visit_next = std::collections::BinaryHeap::new();
    let mut scores: std::collections::HashMap<usize, K> = std::collections::HashMap::new();
    let mut estimate_scores: std::collections::HashMap<usize, K> = std::collections::HashMap::new();
    let mut came_from: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    scores.insert(start, K::default());
    visit_next.push(PgMinScored(estimate_cost(v.ids[start]), start));
    while let Some(PgMinScored(estimate_score, node)) = visit_next.pop() {
        if is_goal(v.ids[node]) {
            let mut path = vec![v.ids[node]];
            let mut current = node;
            while let Some(&previous) = came_from.get(&current) {
                path.push(v.ids[previous]);
                current = previous;
            }
            path.reverse();
            return Some((scores[&node], path));
        }
        let node_score = scores[&node];
        if let Some(&old) = estimate_scores.get(&node)
            && old <= estimate_score
        {
            continue;
        }
        estimate_scores.insert(node, estimate_score);
        for &(next, r) in &v.out[node] {
            let next_score = node_score + edge_cost(r);
            if let Some(&old) = scores.get(&next)
                && old <= next_score
            {
                continue;
            }
            scores.insert(next, next_score);
            came_from.insert(next, node);
            let next_estimate_score = next_score + estimate_cost(v.ids[next]);
            visit_next.push(PgMinScored(next_estimate_score, next));
        }
    }
    None
}

fn __pg_bellman_ford_in<Id, R, K, W>(
    v: &PgView<Id, R>,
    source: usize,
    mut weight: W,
) -> Result<PgPaths<Id, K>, PgNegativeCycle>
where
    Id: Copy,
    R: Copy,
    K: PgFloatMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
    W: FnMut(R) -> K,
{
    let n = v.ids.len();
    let mut predecessors: Vec<Option<Id>> = vec![None; n];
    let mut distances: Vec<K> = vec![K::__pg_infinite(); n];
    if source < n {
        distances[source] = K::default();
    }
    for _ in 1..n {
        let mut did_update = false;
        for i in 0..n {
            for &(j, r) in &v.out[i] {
                let w = weight(r);
                if distances[i] + w < distances[j] {
                    distances[j] = distances[i] + w;
                    predecessors[j] = Some(v.ids[i]);
                    did_update = true;
                }
            }
        }
        if !did_update {
            break;
        }
    }
    for i in 0..n {
        for &(j, r) in &v.out[i] {
            let w = weight(r);
            if distances[i] + w < distances[j] {
                return Err(PgNegativeCycle(()));
            }
        }
    }
    Ok(PgPaths {
        distances,
        predecessors,
    })
}

/// `Dfs::next` over positions, with `reversed` following incoming edges (`Reversed(g)`).
fn __pg_dfs_next<Id, R>(
    v: &PgView<Id, R>,
    stack: &mut Vec<usize>,
    discovered: &mut [bool],
    reversed: bool,
) -> Option<usize> {
    while let Some(node) = stack.pop() {
        if !discovered[node] {
            discovered[node] = true;
            if reversed {
                for &succ in &v.inc[node] {
                    if !discovered[succ] {
                        stack.push(succ);
                    }
                }
            } else {
                for &(succ, _) in &v.out[node] {
                    if !discovered[succ] {
                        stack.push(succ);
                    }
                }
            }
            return Some(node);
        }
    }
    None
}

fn __pg_toposort_in<Id: Copy, R>(v: &PgView<Id, R>) -> Result<Vec<Id>, PgCycle<Id>> {
    let n = v.ids.len();
    let mut discovered = vec![false; n];
    let mut finished = vec![false; n];
    let mut finish_stack: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for i in (0..n).rev() {
        if discovered[i] {
            continue;
        }
        stack.push(i);
        while let Some(&nx) = stack.last() {
            if !discovered[nx] {
                discovered[nx] = true;
                for &(succ, _) in &v.out[nx] {
                    if succ == nx {
                        return Err(PgCycle(v.ids[nx]));
                    }
                    if !discovered[succ] {
                        stack.push(succ);
                    }
                }
            } else {
                stack.pop();
                if !finished[nx] {
                    finished[nx] = true;
                    finish_stack.push(nx);
                }
            }
        }
    }
    finish_stack.reverse();
    let mut discovered = vec![false; n];
    for &i in &finish_stack {
        stack.clear();
        stack.push(i);
        let mut cycle = false;
        while let Some(j) = __pg_dfs_next(v, &mut stack, &mut discovered, true) {
            if cycle {
                return Err(PgCycle(v.ids[j]));
            }
            cycle = true;
        }
    }
    let mut out = Vec::new();
    for i in finish_stack {
        out.push(v.ids[i]);
    }
    Ok(out)
}

fn __pg_is_cyclic_directed_in<Id, R>(v: &PgView<Id, R>) -> bool {
    // (A depth-first search that meets a node still on its path: a back edge.)
    let n = v.ids.len();
    let mut state = vec![0u8; n];
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        state[root] = 1;
        while let Some(&(node, k)) = stack.last() {
            if k < v.out[node].len() {
                let last = stack.len() - 1;
                stack[last] = (node, k + 1);
                let succ = v.out[node][k].0;
                if state[succ] == 1 {
                    return true;
                }
                if state[succ] == 0 {
                    state[succ] = 1;
                    stack.push((succ, 0));
                }
            } else {
                state[node] = 2;
                stack.pop();
            }
        }
    }
    false
}

fn __pg_find(parent: &mut [usize], x: usize) -> usize {
    let mut x = x;
    while parent[x] != x {
        let p = parent[x];
        parent[x] = parent[p];
        x = p;
    }
    x
}
/// Union-find's `union`: whether `a` and `b` were apart.
fn __pg_union(parent: &mut [usize], a: usize, b: usize) -> bool {
    let ra = __pg_find(parent, a);
    let rb = __pg_find(parent, b);
    if ra == rb {
        return false;
    }
    parent[rb] = ra;
    true
}

fn __pg_is_cyclic_undirected_in<Id, R>(v: &PgView<Id, R>) -> bool {
    let mut parent: Vec<usize> = (0..v.ids.len()).collect();
    for &(a, b, _) in &v.refs {
        if !__pg_union(&mut parent, a, b) {
            return true;
        }
    }
    false
}

fn __pg_connected_components_in<Id, R>(v: &PgView<Id, R>) -> usize {
    let n = v.ids.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for &(a, b, _) in &v.refs {
        __pg_union(&mut parent, a, b);
    }
    let mut roots = std::collections::HashSet::new();
    for i in 0..n {
        roots.insert(__pg_find(&mut parent, i));
    }
    roots.len()
}

fn __pg_kosaraju_scc_in<Id: Copy, R>(v: &PgView<Id, R>) -> Vec<Vec<Id>> {
    let n = v.ids.len();
    let mut discovered = vec![false; n];
    let mut finished = vec![false; n];
    let mut finish_order: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    // (Post-order over the reversed graph.)
    for i in 0..n {
        if discovered[i] {
            continue;
        }
        stack.clear();
        stack.push(i);
        while let Some(&nx) = stack.last() {
            if !discovered[nx] {
                discovered[nx] = true;
                for &succ in &v.inc[nx] {
                    if !discovered[succ] {
                        stack.push(succ);
                    }
                }
            } else {
                stack.pop();
                if !finished[nx] {
                    finished[nx] = true;
                    finish_order.push(nx);
                }
            }
        }
    }
    let mut discovered = vec![false; n];
    stack.clear();
    let mut sccs = Vec::new();
    for &i in finish_order.iter().rev() {
        if discovered[i] {
            continue;
        }
        stack.clear();
        stack.push(i);
        let mut scc = Vec::new();
        while let Some(nx) = __pg_dfs_next(v, &mut stack, &mut discovered, false) {
            scc.push(v.ids[nx]);
        }
        sccs.push(scc);
    }
    sccs
}

/// Pearce's version of Tarjan's algorithm, as petgraph runs it (a component's nodes in the
/// order they left the stack, components in reverse topological order).
fn __pg_tarjan_scc_in<Id: Copy, R>(v: &PgView<Id, R>) -> Vec<Vec<Id>> {
    let n = v.ids.len();
    // (0 is "not visited"; finished components count down from `usize::MAX`.)
    let mut rootindex = vec![0usize; n];
    let mut index = 1usize;
    let mut componentcount = usize::MAX;
    let mut stack: Vec<usize> = Vec::new();
    let mut sccs = Vec::new();
    // (The recursion, as a stack of (node, next edge, is local root).)
    let mut frames: Vec<(usize, usize, bool)> = Vec::new();
    for root in 0..n {
        if rootindex[root] != 0 {
            continue;
        }
        rootindex[root] = index;
        index += 1;
        frames.push((root, 0, true));
        while let Some(&(node, k, local_root)) = frames.last() {
            let last = frames.len() - 1;
            if k < v.out[node].len() {
                let w = v.out[node][k].0;
                if rootindex[w] == 0 {
                    // (Visit `w` first; this edge's check runs when it returns.)
                    rootindex[w] = index;
                    index += 1;
                    frames.push((w, 0, true));
                    continue;
                }
                let mut lr = local_root;
                if rootindex[w] < rootindex[node] {
                    rootindex[node] = rootindex[w];
                    lr = false;
                }
                frames[last] = (node, k + 1, lr);
                continue;
            }
            frames.pop();
            if local_root {
                let mut indexadjustment = 1;
                let c = componentcount;
                let mut start = 0;
                let mut j = stack.len();
                while j > 0 {
                    let w = stack[j - 1];
                    if rootindex[node] > rootindex[w] {
                        start = j;
                        break;
                    }
                    rootindex[w] = c;
                    indexadjustment += 1;
                    j -= 1;
                }
                rootindex[node] = c;
                stack.push(node);
                let mut scc = Vec::new();
                for &w in &stack[start..] {
                    scc.push(v.ids[w]);
                }
                sccs.push(scc);
                stack.truncate(start);
                index -= indexadjustment;
                componentcount -= 1;
            } else {
                stack.push(node);
            }
            // (Back in the caller: the edge to `node` is checked now.)
            if let Some(&(parent, pk, plr)) = frames.last() {
                let pl = frames.len() - 1;
                let mut lr = plr;
                if rootindex[node] < rootindex[parent] {
                    rootindex[parent] = rootindex[node];
                    lr = false;
                }
                frames[pl] = (parent, pk + 1, lr);
            }
        }
    }
    sccs
}

fn __pg_has_path_connecting_in<Id, R>(v: &PgView<Id, R>, from: usize, to: usize) -> bool {
    let mut discovered = vec![false; v.ids.len()];
    let mut stack = vec![from];
    while let Some(x) = __pg_dfs_next(v, &mut stack, &mut discovered, false) {
        if x == to {
            return true;
        }
    }
    false
}

fn __pg_min_spanning_tree_in<NW, EW: PartialOrd>(
    nodes: Vec<NW>,
    edges: Vec<(usize, usize, EW)>,
) -> Vec<PgElement<NW, EW>> {
    let n = nodes.len();
    let mut out = Vec::new();
    for w in nodes {
        out.push(PgElement::Node { weight: w });
    }
    // (Kruskal's: the lightest edge first, joining two trees.)
    let mut ends = Vec::new();
    let mut heap = std::collections::BinaryHeap::new();
    for (k, (a, b, w)) in edges.into_iter().enumerate() {
        ends.push((a, b));
        heap.push(PgMinScored(w, k));
    }
    let mut parent: Vec<usize> = (0..n).collect();
    while let Some(PgMinScored(score, k)) = heap.pop() {
        let (a, b) = ends[k];
        if __pg_union(&mut parent, a, b) {
            out.push(PgElement::Edge {
                source: a,
                target: b,
                weight: score,
            });
        }
    }
    out
}
