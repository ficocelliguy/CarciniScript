// (petgraph prelude)
// The bundled `petgraph` crate: `Graph` (petgraph's adjacency lists, so `neighbors` and `edges`
// come newest first, as there), `StableGraph` (the same, with vacant slots for what was
// removed), `GraphMap` (insertion-ordered, keyed by the nodes themselves), `Bfs` / `Dfs` /
// `DfsPostOrder` / `Topo` (and `Reversed`), `Dot`, and the `algo` functions, which the rewrite
// turns into methods of the graph (`dijkstra(&g, ..)` is `(&g).__pg_dijkstra(..)`) and walkers'
// `next(&g)` into `__pg_next(|n, d| (&g).neighbors_directed(n, d).collect())`. Each algorithm
// follows petgraph 0.8's code (visiting order, `BinaryHeap` ties), so results with ties come out
// the same. Indices are always `u32` (the rewrite drops an index type argument).

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

/// The weights `floyd_warshall` and `spfa` take (`BoundedMeasure`): a maximum for "no path", and
/// an addition that says when it overflowed (wrapping for integers; floats past their range).
pub trait PgBoundedMeasure: Copy + PartialOrd + Default + std::ops::Add<Output = Self> {
    fn __pg_max() -> Self;
    fn __pg_overflowing_add(self, rhs: Self) -> (Self, bool);
}
impl PgBoundedMeasure for i8 {
    fn __pg_max() -> i8 {
        i8::MAX
    }
    fn __pg_overflowing_add(self, rhs: i8) -> (i8, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for i16 {
    fn __pg_max() -> i16 {
        i16::MAX
    }
    fn __pg_overflowing_add(self, rhs: i16) -> (i16, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for i32 {
    fn __pg_max() -> i32 {
        i32::MAX
    }
    fn __pg_overflowing_add(self, rhs: i32) -> (i32, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for i64 {
    fn __pg_max() -> i64 {
        i64::MAX
    }
    fn __pg_overflowing_add(self, rhs: i64) -> (i64, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for isize {
    fn __pg_max() -> isize {
        isize::MAX
    }
    fn __pg_overflowing_add(self, rhs: isize) -> (isize, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for u8 {
    fn __pg_max() -> u8 {
        u8::MAX
    }
    fn __pg_overflowing_add(self, rhs: u8) -> (u8, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for u16 {
    fn __pg_max() -> u16 {
        u16::MAX
    }
    fn __pg_overflowing_add(self, rhs: u16) -> (u16, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for u32 {
    fn __pg_max() -> u32 {
        u32::MAX
    }
    fn __pg_overflowing_add(self, rhs: u32) -> (u32, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for u64 {
    fn __pg_max() -> u64 {
        u64::MAX
    }
    fn __pg_overflowing_add(self, rhs: u64) -> (u64, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for usize {
    fn __pg_max() -> usize {
        usize::MAX
    }
    fn __pg_overflowing_add(self, rhs: usize) -> (usize, bool) {
        match self.checked_add(rhs) {
            Some(x) => (x, false),
            None => (self.wrapping_add(rhs), true),
        }
    }
}
impl PgBoundedMeasure for f64 {
    fn __pg_max() -> f64 {
        f64::MAX
    }
    fn __pg_overflowing_add(self, rhs: f64) -> (f64, bool) {
        let overflow = self > 0.0 && rhs > 0.0 && self > f64::MAX - rhs;
        let underflow = !overflow && self < 0.0 && rhs < 0.0 && self < f64::MIN - rhs;
        (self + rhs, overflow || underflow)
    }
}
impl PgBoundedMeasure for f32 {
    fn __pg_max() -> f32 {
        f32::MAX
    }
    fn __pg_overflowing_add(self, rhs: f32) -> (f32, bool) {
        let overflow = self > 0.0 && rhs > 0.0 && self > f32::MAX - rhs;
        let underflow = !overflow && self < 0.0 && rhs < 0.0 && self < f32::MIN - rhs;
        (self + rhs, overflow || underflow)
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
        // (Undirected: both lists, whichever direction.)
        if !self.directed {
            return self.neighbors_undirected(a);
        }
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
            raw: Vec::new(),
            bound: n,
            directed: self.directed,
        };
        for i in 0..n {
            let a = PgNodeIndex::new(i);
            v.ids.push(a);
            v.raw.push(i);
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
    pub fn __pg_scc(&self) -> Vec<Vec<PgNodeIndex>> {
        __pg_kosaraju_scc_in(&self.__pg_view())
    }
    pub fn __pg_all_simple_paths(
        &self,
        from: PgNodeIndex,
        to: PgNodeIndex,
        min_intermediate_nodes: usize,
        max_intermediate_nodes: Option<usize>,
    ) -> PgSimplePaths<PgNodeIndex> {
        __pg_all_simple_paths_in(
            &self.__pg_view(),
            from.index(),
            to.index(),
            min_intermediate_nodes,
            max_intermediate_nodes,
        )
    }
    pub fn __pg_floyd_warshall<'a, K, F>(
        &'a self,
        edge_cost: F,
    ) -> Result<std::collections::HashMap<(PgNodeIndex, PgNodeIndex), K>, PgNegativeCycle>
    where
        K: PgBoundedMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
        F: FnMut(PgEdgeReference<'a, E>) -> K,
    {
        __pg_floyd_warshall_in(&self.__pg_view(), edge_cost)
    }
    pub fn __pg_k_shortest_path<'a, K, F>(
        &'a self,
        start: PgNodeIndex,
        goal: Option<PgNodeIndex>,
        k: usize,
        edge_cost: F,
    ) -> std::collections::HashMap<PgNodeIndex, K>
    where
        K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
        F: FnMut(PgEdgeReference<'a, E>) -> K,
    {
        __pg_k_shortest_path_in(
            &self.__pg_view(),
            start.index(),
            goal.map(|g| g.index()),
            k,
            edge_cost,
        )
    }
    pub fn __pg_is_bipartite_undirected(&self, start: PgNodeIndex) -> bool {
        __pg_is_bipartite_undirected_in(&self.__pg_view(), start.index())
    }
    pub fn __pg_bridges(&self) -> std::vec::IntoIter<PgEdgeReference<'_, E>> {
        __pg_bridges_in(&self.__pg_view()).into_iter()
    }
    pub fn __pg_articulation_points(&self) -> std::collections::HashSet<PgNodeIndex> {
        __pg_articulation_points_in(&self.__pg_view())
    }
    pub fn __pg_spfa<'a, K, F>(
        &'a self,
        source: PgNodeIndex,
        edge_cost: F,
    ) -> Result<PgPaths<PgNodeIndex, K>, PgNegativeCycle>
    where
        K: PgBoundedMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
        F: FnMut(PgEdgeReference<'a, E>) -> K,
    {
        __pg_spfa_in(&self.__pg_view(), source.index(), edge_cost)
    }
    /// `condensation(g, make_acyclic)`: a node per strongly connected component (its nodes'
    /// weights), and the edges between them (without self loops or repeats when acyclic).
    pub fn __pg_condensation(self, make_acyclic: bool) -> PgGraph<Vec<N>, E, Ty> {
        let sccs = self.__pg_kosaraju_scc();
        let mut condensed: PgGraph<Vec<N>, E, Ty> = PgGraph::with_capacity(0, 0);
        let mut node_map = vec![PgNodeIndex::end(); self.nodes.len()];
        for comp in sccs {
            let new_nix = condensed.add_node(Vec::new());
            for nix in comp {
                node_map[nix.index()] = new_nix;
            }
        }
        let PgGraph { nodes, edges, .. } = self;
        for (nix, node) in nodes.into_iter().enumerate() {
            condensed[node_map[nix]].push(node.weight);
        }
        for edge in edges {
            let source = node_map[edge.node[0].index()];
            let target = node_map[edge.node[1].index()];
            if make_acyclic {
                if source != target {
                    condensed.update_edge(source, target, edge.weight);
                }
            } else {
                condensed.add_edge(source, target, edge.weight);
            }
        }
        condensed
    }
    pub fn node_bound(&self) -> usize {
        self.nodes.len()
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
    pub fn __pg_find_negative_cycle(&self, source: PgNodeIndex) -> Option<Vec<PgNodeIndex>> {
        __pg_find_negative_cycle_in(&self.__pg_view(), source.index(), |r| *r.weight())
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
            raw: (0..self.nodes.len()).collect(),
            bound: self.nodes.len(),
            directed: self.directed,
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
    pub fn __pg_scc(&self) -> Vec<Vec<N>> {
        __pg_kosaraju_scc_in(&self.__pg_view())
    }
    pub fn __pg_all_simple_paths(
        &self,
        from: N,
        to: N,
        min_intermediate_nodes: usize,
        max_intermediate_nodes: Option<usize>,
    ) -> PgSimplePaths<N> {
        __pg_all_simple_paths_in(
            &self.__pg_view(),
            self.__pg_ix(from),
            self.__pg_ix(to),
            min_intermediate_nodes,
            max_intermediate_nodes,
        )
    }
    pub fn __pg_floyd_warshall<'a, K, F>(
        &'a self,
        edge_cost: F,
    ) -> Result<std::collections::HashMap<(N, N), K>, PgNegativeCycle>
    where
        K: PgBoundedMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
        F: FnMut(PgMapEdgeRef<'a, N, E>) -> K,
    {
        __pg_floyd_warshall_in(&self.__pg_view(), edge_cost)
    }
    pub fn __pg_k_shortest_path<'a, K, F>(
        &'a self,
        start: N,
        goal: Option<N>,
        k: usize,
        edge_cost: F,
    ) -> std::collections::HashMap<N, K>
    where
        K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
        F: FnMut(PgMapEdgeRef<'a, N, E>) -> K,
    {
        __pg_k_shortest_path_in(
            &self.__pg_view(),
            self.__pg_ix(start),
            goal.map(|g| self.__pg_ix(g)),
            k,
            edge_cost,
        )
    }
    pub fn __pg_is_bipartite_undirected(&self, start: N) -> bool {
        __pg_is_bipartite_undirected_in(&self.__pg_view(), self.__pg_ix(start))
    }
    pub fn __pg_bridges(&self) -> std::vec::IntoIter<PgMapEdgeRef<'_, N, E>> {
        __pg_bridges_in(&self.__pg_view()).into_iter()
    }
    pub fn __pg_articulation_points(&self) -> std::collections::HashSet<N> {
        __pg_articulation_points_in(&self.__pg_view())
    }
    pub fn __pg_spfa<'a, K, F>(
        &'a self,
        source: N,
        edge_cost: F,
    ) -> Result<PgPaths<N, K>, PgNegativeCycle>
    where
        K: PgBoundedMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
        F: FnMut(PgMapEdgeRef<'a, N, E>) -> K,
    {
        __pg_spfa_in(&self.__pg_view(), self.__pg_ix(source), edge_cost)
    }
    pub fn node_bound(&self) -> usize {
        self.nodes.len()
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
    pub fn __pg_find_negative_cycle(&self, source: N) -> Option<Vec<N>> {
        __pg_find_negative_cycle_in(&self.__pg_view(), self.__pg_ix(source), |r| *r.weight())
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

// ---------------------------------------------------------------- StableGraph

/// `petgraph::stable_graph::StableGraph`: a `Graph` whose removed nodes and edges leave vacant
/// slots (so the other indices stay put), reused by later additions, the latest freed first.
/// The vacant nodes form a doubly linked list through their `next` (forward, back), the vacant
/// edges a singly linked one through `next[0]`. A slot's weight is a `Vec` of one, empty when
/// vacant (`&mut w[0]` is a place `index_mut` can hand out, where `Option::as_mut` isn't).
#[derive(Clone)]
pub struct PgStableGraph<N, E, Ty = PgDirected> {
    g: PgGraph<Vec<N>, Vec<E>, Ty>,
    node_count: usize,
    edge_count: usize,
    free_node: PgNodeIndex,
    free_edge: PgEdgeIndex,
}

impl<N, E> PgStableGraph<N, E, PgDirected> {
    pub fn new() -> PgStableGraph<N, E, PgDirected> {
        PgStableGraph::with_capacity(0, 0)
    }
}
impl<N, E, Ty: PgEdgeType> Default for PgStableGraph<N, E, Ty> {
    fn default() -> Self {
        PgStableGraph::with_capacity(0, 0)
    }
}

impl<N, E, Ty: PgEdgeType> PgStableGraph<N, E, Ty> {
    pub fn with_capacity(_nodes: usize, _edges: usize) -> PgStableGraph<N, E, Ty> {
        PgStableGraph {
            g: PgGraph::with_capacity(0, 0),
            node_count: 0,
            edge_count: 0,
            free_node: PgNodeIndex::end(),
            free_edge: PgEdgeIndex::end(),
        }
    }
    pub fn node_count(&self) -> usize {
        self.node_count
    }
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }
    pub fn is_directed(&self) -> bool {
        self.g.directed
    }
    pub fn node_bound(&self) -> usize {
        let mut i = self.g.nodes.len();
        while i > 0 && self.g.nodes[i - 1].weight.is_empty() {
            i -= 1;
        }
        i
    }
    pub fn clear(&mut self) {
        self.node_count = 0;
        self.edge_count = 0;
        self.free_node = PgNodeIndex::end();
        self.free_edge = PgEdgeIndex::end();
        self.g.clear();
    }
    pub fn clear_edges(&mut self) {
        self.edge_count = 0;
        self.free_edge = PgEdgeIndex::end();
        self.g.edges.clear();
        for i in 0..self.g.nodes.len() {
            if !self.g.nodes[i].weight.is_empty() {
                self.g.nodes[i].next = [PgEdgeIndex::end(), PgEdgeIndex::end()];
            }
        }
    }
    pub fn contains_node(&self, a: PgNodeIndex) -> bool {
        a.index() < self.g.nodes.len() && !self.g.nodes[a.index()].weight.is_empty()
    }
    pub fn add_node(&mut self, weight: N) -> PgNodeIndex {
        if self.free_node != PgNodeIndex::end() {
            let ix = self.free_node;
            self.__pg_occupy_vacant_node(ix, weight);
            return ix;
        }
        self.node_count += 1;
        self.g.add_node(vec![weight])
    }
    /// Takes `ix` out of the vacant list (wherever it is in it) and puts `weight` there.
    fn __pg_occupy_vacant_node(&mut self, ix: PgNodeIndex, weight: N) {
        let i = ix.index();
        self.g.nodes[i].weight = vec![weight];
        let previous = self.g.nodes[i].next[1];
        let next = self.g.nodes[i].next[0];
        self.g.nodes[i].next = [PgEdgeIndex::end(), PgEdgeIndex::end()];
        if previous != PgEdgeIndex::end() {
            let p = self.g.nodes[previous.index()].next;
            self.g.nodes[previous.index()].next = [next, p[1]];
        }
        if next != PgEdgeIndex::end() {
            let q = self.g.nodes[next.index()].next;
            self.g.nodes[next.index()].next = [q[0], previous];
        }
        if self.free_node == ix {
            self.free_node = PgNodeIndex(next.0);
        }
        self.node_count += 1;
    }
    pub fn remove_node(&mut self, a: PgNodeIndex) -> Option<N> {
        if !self.contains_node(a) {
            return None;
        }
        // (A plain index, so the place is written.)
        let i = a.index();
        let weight = self.g.nodes[i].weight.pop();
        for k in 0..2 {
            loop {
                let next = self.g.nodes[a.index()].next[k];
                if next == PgEdgeIndex::end() {
                    break;
                }
                self.remove_edge(next);
            }
        }
        self.g.nodes[a.index()].next = [PgEdgeIndex(self.free_node.0), PgEdgeIndex::end()];
        if self.free_node != PgNodeIndex::end() {
            let f = self.g.nodes[self.free_node.index()].next;
            self.g.nodes[self.free_node.index()].next = [f[0], PgEdgeIndex(a.0)];
        }
        self.free_node = a;
        self.node_count -= 1;
        weight
    }
    pub fn add_edge(&mut self, a: PgNodeIndex, b: PgNodeIndex, weight: E) -> PgEdgeIndex {
        let n = self.g.nodes.len();
        let wrong = if a.index().max(b.index()) >= n {
            Some(a.index().max(b.index()))
        } else if self.g.nodes[a.index()].weight.is_empty() {
            Some(a.index())
        } else if self.g.nodes[b.index()].weight.is_empty() {
            Some(b.index())
        } else {
            None
        };
        if let Some(i) = wrong {
            panic!(
                "StableGraph::add_edge: node index {} is not a node in the graph",
                i
            );
        }
        let ix;
        if self.free_edge != PgEdgeIndex::end() {
            ix = self.free_edge;
            self.free_edge = self.g.edges[ix.index()].next[0];
            self.g.edges[ix.index()].weight = vec![weight];
            self.g.edges[ix.index()].node = [a, b];
        } else {
            ix = PgEdgeIndex::new(self.g.edges.len());
            self.g.edges.push(PgEdge {
                weight: vec![weight],
                next: [PgEdgeIndex::end(), PgEdgeIndex::end()],
                node: [a, b],
            });
        }
        if a == b {
            let an = self.g.nodes[a.index()].next;
            self.g.edges[ix.index()].next = an;
            self.g.nodes[a.index()].next = [ix, ix];
        } else {
            let an = self.g.nodes[a.index()].next;
            let bn = self.g.nodes[b.index()].next;
            self.g.edges[ix.index()].next = [an[0], bn[1]];
            self.g.nodes[a.index()].next = [ix, an[1]];
            self.g.nodes[b.index()].next = [bn[0], ix];
        }
        self.edge_count += 1;
        ix
    }
    pub fn update_edge(&mut self, a: PgNodeIndex, b: PgNodeIndex, weight: E) -> PgEdgeIndex {
        if let Some(ix) = self.find_edge(a, b) {
            self.g.edges[ix.index()].weight = vec![weight];
            return ix;
        }
        self.add_edge(a, b, weight)
    }
    pub fn remove_edge(&mut self, e: PgEdgeIndex) -> Option<E> {
        if e.index() >= self.g.edges.len() || self.g.edges[e.index()].weight.is_empty() {
            return None;
        }
        let edge_node = self.g.edges[e.index()].node;
        let edge_next = self.g.edges[e.index()].next;
        self.g.__pg_change_edge_links(edge_node, e, edge_next);
        self.g.edges[e.index()].next = [self.free_edge, PgEdgeIndex::end()];
        self.g.edges[e.index()].node = [PgNodeIndex::end(), PgNodeIndex::end()];
        self.free_edge = e;
        self.edge_count -= 1;
        let i = e.index();
        self.g.edges[i].weight.pop()
    }
    pub fn node_weight(&self, a: PgNodeIndex) -> Option<&N> {
        match self.g.nodes.get(a.index()) {
            Some(n) => n.weight.first(),
            None => None,
        }
    }
    pub fn node_weight_mut(&mut self, a: PgNodeIndex) -> Option<&mut N> {
        match self.g.nodes.get_mut(a.index()) {
            Some(n) => n.weight.first_mut(),
            None => None,
        }
    }
    pub fn edge_weight(&self, e: PgEdgeIndex) -> Option<&E> {
        match self.g.edges.get(e.index()) {
            Some(x) => x.weight.first(),
            None => None,
        }
    }
    pub fn edge_weight_mut(&mut self, e: PgEdgeIndex) -> Option<&mut E> {
        match self.g.edges.get_mut(e.index()) {
            Some(x) => x.weight.first_mut(),
            None => None,
        }
    }
    pub fn edge_endpoints(&self, e: PgEdgeIndex) -> Option<(PgNodeIndex, PgNodeIndex)> {
        match self.g.edges.get(e.index()) {
            Some(x) if !x.weight.is_empty() => Some((x.node[0], x.node[1])),
            _ => None,
        }
    }
    pub fn node_indices(&self) -> std::vec::IntoIter<PgNodeIndex> {
        let mut out = Vec::new();
        for i in 0..self.g.nodes.len() {
            if !self.g.nodes[i].weight.is_empty() {
                out.push(PgNodeIndex::new(i));
            }
        }
        out.into_iter()
    }
    pub fn edge_indices(&self) -> std::vec::IntoIter<PgEdgeIndex> {
        let mut out = Vec::new();
        for i in 0..self.g.edges.len() {
            if !self.g.edges[i].weight.is_empty() {
                out.push(PgEdgeIndex::new(i));
            }
        }
        out.into_iter()
    }
    pub fn node_weights(&self) -> std::vec::IntoIter<&N> {
        let mut out = Vec::new();
        for n in &self.g.nodes {
            if let Some(w) = n.weight.first() {
                out.push(w);
            }
        }
        out.into_iter()
    }
    pub fn edge_weights(&self) -> std::vec::IntoIter<&E> {
        let mut out = Vec::new();
        for e in &self.g.edges {
            if let Some(w) = e.weight.first() {
                out.push(w);
            }
        }
        out.into_iter()
    }
    pub fn neighbors(&self, a: PgNodeIndex) -> std::vec::IntoIter<PgNodeIndex> {
        self.neighbors_directed(a, PgDirection::Outgoing)
    }
    pub fn neighbors_directed(
        &self,
        a: PgNodeIndex,
        dir: PgDirection,
    ) -> std::vec::IntoIter<PgNodeIndex> {
        if !self.contains_node(a) {
            return Vec::new().into_iter();
        }
        self.g.neighbors_directed(a, dir)
    }
    pub fn neighbors_undirected(&self, a: PgNodeIndex) -> std::vec::IntoIter<PgNodeIndex> {
        if !self.contains_node(a) {
            return Vec::new().into_iter();
        }
        self.g.neighbors_undirected(a)
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
        if self.contains_node(a) {
            for (e, node) in self.g.__pg_edge_list(a, dir) {
                if let Some(weight) = self.g.edges[e.index()].weight.first() {
                    out.push(PgEdgeReference {
                        index: e,
                        node,
                        weight,
                    });
                }
            }
        }
        out.into_iter()
    }
    pub fn edges_connecting(
        &self,
        a: PgNodeIndex,
        b: PgNodeIndex,
    ) -> std::vec::IntoIter<PgEdgeReference<'_, E>> {
        let mut out = Vec::new();
        for r in self.edges_directed(a, PgDirection::Outgoing) {
            if r.target() == b {
                out.push(r);
            }
        }
        out.into_iter()
    }
    pub fn edge_references(&self) -> std::vec::IntoIter<PgEdgeReference<'_, E>> {
        let mut out = Vec::new();
        for i in 0..self.g.edges.len() {
            if let Some(weight) = self.g.edges[i].weight.first() {
                out.push(PgEdgeReference {
                    index: PgEdgeIndex::new(i),
                    node: self.g.edges[i].node,
                    weight,
                });
            }
        }
        out.into_iter()
    }
    pub fn find_edge(&self, a: PgNodeIndex, b: PgNodeIndex) -> Option<PgEdgeIndex> {
        if !self.contains_node(a) {
            return None;
        }
        self.g.find_edge(a, b)
    }
    pub fn find_edge_undirected(
        &self,
        a: PgNodeIndex,
        b: PgNodeIndex,
    ) -> Option<(PgEdgeIndex, PgDirection)> {
        if !self.contains_node(a) {
            return None;
        }
        self.g.find_edge_undirected(a, b)
    }
    pub fn contains_edge(&self, a: PgNodeIndex, b: PgNodeIndex) -> bool {
        self.find_edge(a, b).is_some()
    }
    pub fn externals(&self, dir: PgDirection) -> std::vec::IntoIter<PgNodeIndex> {
        let k = dir.index();
        let mut out = Vec::new();
        for i in 0..self.g.nodes.len() {
            let next = self.g.nodes[i].next;
            if !self.g.nodes[i].weight.is_empty()
                && next[k] == PgEdgeIndex::end()
                && (self.g.directed || next[1 - k] == PgEdgeIndex::end())
            {
                out.push(PgNodeIndex::new(i));
            }
        }
        out.into_iter()
    }
    pub fn reverse(&mut self) {
        for i in 0..self.g.edges.len() {
            if !self.g.edges[i].weight.is_empty() {
                let n = self.g.edges[i].node;
                let x = self.g.edges[i].next;
                self.g.edges[i].node = [n[1], n[0]];
                self.g.edges[i].next = [x[1], x[0]];
            }
        }
        for i in 0..self.g.nodes.len() {
            if !self.g.nodes[i].weight.is_empty() {
                let x = self.g.nodes[i].next;
                self.g.nodes[i].next = [x[1], x[0]];
            }
        }
    }
    /// The live nodes in index order; positions in the view, `raw` their indices.
    fn __pg_view(&self) -> PgView<PgNodeIndex, PgEdgeReference<'_, E>> {
        let live: Vec<PgNodeIndex> = self.node_indices().collect();
        let mut pos = vec![usize::MAX; self.g.nodes.len()];
        for (p, a) in live.iter().enumerate() {
            pos[a.index()] = p;
        }
        let mut v = PgView {
            ids: Vec::new(),
            out: Vec::new(),
            inc: Vec::new(),
            refs: Vec::new(),
            raw: Vec::new(),
            bound: self.node_bound(),
            directed: self.g.directed,
        };
        for &a in &live {
            v.ids.push(a);
            v.raw.push(a.index());
            let mut out = Vec::new();
            for r in self.edges(a) {
                out.push((pos[r.target().index()], r));
            }
            v.out.push(out);
            let mut inc = Vec::new();
            for b in self.neighbors_directed(a, PgDirection::Incoming) {
                inc.push(pos[b.index()]);
            }
            v.inc.push(inc);
        }
        for r in self.edge_references() {
            v.refs
                .push((pos[r.source().index()], pos[r.target().index()], r));
        }
        v
    }
    fn __pg_pos(&self, a: PgNodeIndex) -> usize {
        let mut p = 0;
        for i in 0..a.index().min(self.g.nodes.len()) {
            if !self.g.nodes[i].weight.is_empty() {
                p += 1;
            }
        }
        p
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
        __pg_dijkstra_in(
            &v,
            self.__pg_pos(start),
            goal.map(|g| self.__pg_pos(g)),
            edge_cost,
        )
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
        __pg_astar_in(&v, self.__pg_pos(start), is_goal, edge_cost, estimate_cost)
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
    pub fn __pg_kosaraju_scc(&self) -> Vec<Vec<PgNodeIndex>> {
        __pg_kosaraju_scc_in(&self.__pg_view())
    }
    pub fn __pg_scc(&self) -> Vec<Vec<PgNodeIndex>> {
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
        __pg_has_path_connecting_in(&self.__pg_view(), self.__pg_pos(from), self.__pg_pos(to))
    }
    pub fn __pg_all_simple_paths(
        &self,
        from: PgNodeIndex,
        to: PgNodeIndex,
        min_intermediate_nodes: usize,
        max_intermediate_nodes: Option<usize>,
    ) -> PgSimplePaths<PgNodeIndex> {
        __pg_all_simple_paths_in(
            &self.__pg_view(),
            self.__pg_pos(from),
            self.__pg_pos(to),
            min_intermediate_nodes,
            max_intermediate_nodes,
        )
    }
    pub fn __pg_k_shortest_path<'a, K, F>(
        &'a self,
        start: PgNodeIndex,
        goal: Option<PgNodeIndex>,
        k: usize,
        edge_cost: F,
    ) -> std::collections::HashMap<PgNodeIndex, K>
    where
        K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
        F: FnMut(PgEdgeReference<'a, E>) -> K,
    {
        __pg_k_shortest_path_in(
            &self.__pg_view(),
            self.__pg_pos(start),
            goal.map(|g| self.__pg_pos(g)),
            k,
            edge_cost,
        )
    }
    pub fn __pg_is_bipartite_undirected(&self, start: PgNodeIndex) -> bool {
        __pg_is_bipartite_undirected_in(&self.__pg_view(), self.__pg_pos(start))
    }
    pub fn __pg_bridges(&self) -> std::vec::IntoIter<PgEdgeReference<'_, E>> {
        __pg_bridges_in(&self.__pg_view()).into_iter()
    }
    pub fn __pg_articulation_points(&self) -> std::collections::HashSet<PgNodeIndex> {
        __pg_articulation_points_in(&self.__pg_view())
    }
    /// `spfa`: its vectors run to `node_bound()`, vacant slots unreached.
    pub fn __pg_spfa<'a, K, F>(
        &'a self,
        source: PgNodeIndex,
        edge_cost: F,
    ) -> Result<PgPaths<PgNodeIndex, K>, PgNegativeCycle>
    where
        K: PgBoundedMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
        F: FnMut(PgEdgeReference<'a, E>) -> K,
    {
        let v = self.__pg_view();
        let p = __pg_spfa_in(&v, self.__pg_pos(source), edge_cost)?;
        Ok(PgPaths {
            distances: __pg_spread(&v, p.distances, K::__pg_max()),
            predecessors: __pg_spread(&v, p.predecessors, None),
        })
    }
}

impl<N: Default, E, Ty: PgEdgeType> PgStableGraph<N, E, Ty> {
    /// A missing node: vacant slots up to it, then it (with `N::default()`).
    fn __pg_ensure_node_exists(&mut self, ix: PgNodeIndex) {
        if self.contains_node(ix) {
            return;
        }
        while ix.index() >= self.g.nodes.len() {
            let node_idx = self.g.add_node(Vec::new());
            self.g.nodes[node_idx.index()].next =
                [PgEdgeIndex(self.free_node.0), PgEdgeIndex::end()];
            if self.free_node != PgNodeIndex::end() {
                let f = self.g.nodes[self.free_node.index()].next;
                self.g.nodes[self.free_node.index()].next = [f[0], PgEdgeIndex(node_idx.0)];
            }
            self.free_node = node_idx;
        }
        self.__pg_occupy_vacant_node(ix, N::default());
    }
    pub fn extend_with_edges<X: PgIntoNodeIndex, I: IntoIterator<Item = (X, X, E)>>(
        &mut self,
        iterable: I,
    ) {
        for (a, b, w) in iterable {
            let a: PgNodeIndex = a.__pg_into_ix();
            let b: PgNodeIndex = b.__pg_into_ix();
            self.__pg_ensure_node_exists(a);
            self.__pg_ensure_node_exists(b);
            self.add_edge(a, b, w);
        }
    }
    pub fn from_edges<X: PgIntoNodeIndex, I: IntoIterator<Item = (X, X, E)>>(
        iterable: I,
    ) -> PgStableGraph<N, E, Ty> {
        let mut g = PgStableGraph::with_capacity(0, 0);
        g.extend_with_edges(iterable);
        g
    }
}

impl<N: Default, E: Default, Ty: PgEdgeType> PgStableGraph<N, E, Ty> {
    pub fn __pg_extend_with_edges2<X: PgIntoNodeIndex, I: IntoIterator<Item = (X, X)>>(
        &mut self,
        iterable: I,
    ) {
        for (a, b) in iterable {
            let a: PgNodeIndex = a.__pg_into_ix();
            let b: PgNodeIndex = b.__pg_into_ix();
            self.__pg_ensure_node_exists(a);
            self.__pg_ensure_node_exists(b);
            self.add_edge(a, b, E::default());
        }
    }
    pub fn __pg_from_edges2<X: PgIntoNodeIndex, I: IntoIterator<Item = (X, X)>>(
        iterable: I,
    ) -> PgStableGraph<N, E, Ty> {
        let mut g = PgStableGraph::with_capacity(0, 0);
        g.__pg_extend_with_edges2(iterable);
        g
    }
}

impl<N, E: PgFloatMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = E>, Ty: PgEdgeType>
    PgStableGraph<N, E, Ty>
{
    /// `bellman_ford`: its vectors run to `node_bound()`, vacant slots unreached.
    pub fn __pg_bellman_ford(
        &self,
        source: PgNodeIndex,
    ) -> Result<PgPaths<PgNodeIndex, E>, PgNegativeCycle> {
        let v = self.__pg_view();
        let p = __pg_bellman_ford_in(&v, self.__pg_pos(source), |r| *r.weight())?;
        Ok(PgPaths {
            distances: __pg_spread(&v, p.distances, E::__pg_infinite()),
            predecessors: __pg_spread(&v, p.predecessors, None),
        })
    }
    pub fn __pg_find_negative_cycle(&self, source: PgNodeIndex) -> Option<Vec<PgNodeIndex>> {
        __pg_find_negative_cycle_in(&self.__pg_view(), self.__pg_pos(source), |r| *r.weight())
    }
}

impl<N: Clone, E: Clone + PartialOrd, Ty: PgEdgeType> PgStableGraph<N, E, Ty> {
    /// `min_spanning_tree`: its edges' ends are the nodes' indices.
    pub fn __pg_min_spanning_tree(&self) -> std::vec::IntoIter<PgElement<N, E>> {
        let v = self.__pg_view();
        let mut nodes = Vec::new();
        for w in self.node_weights() {
            nodes.push(w.clone());
        }
        let mut edges = Vec::new();
        for &(a, b, r) in &v.refs {
            edges.push((a, b, r.weight().clone()));
        }
        let mut out = Vec::new();
        for el in __pg_min_spanning_tree_in(nodes, edges) {
            out.push(match el {
                PgElement::Edge {
                    source,
                    target,
                    weight,
                } => PgElement::Edge {
                    source: v.raw[source],
                    target: v.raw[target],
                    weight,
                },
                node => node,
            });
        }
        out.into_iter()
    }
}

impl<N, E, Ty: PgEdgeType> From<PgGraph<N, E, Ty>> for PgStableGraph<N, E, Ty> {
    fn from(g: PgGraph<N, E, Ty>) -> PgStableGraph<N, E, Ty> {
        // (Built field by field: `with_capacity` asks `Ty`, which a `From` call doesn't carry.)
        let mut out: PgStableGraph<N, E, Ty> = PgStableGraph {
            g: PgGraph {
                nodes: Vec::new(),
                edges: Vec::new(),
                directed: g.directed,
                ty: None,
            },
            node_count: g.nodes.len(),
            edge_count: g.edges.len(),
            free_node: PgNodeIndex::end(),
            free_edge: PgEdgeIndex::end(),
        };
        for n in g.nodes {
            out.g.nodes.push(PgNode {
                weight: vec![n.weight],
                next: n.next,
            });
        }
        for e in g.edges {
            out.g.edges.push(PgEdge {
                weight: vec![e.weight],
                next: e.next,
                node: e.node,
            });
        }
        out
    }
}

impl<N, E, Ty: PgEdgeType> From<PgStableGraph<N, E, Ty>> for PgGraph<N, E, Ty> {
    /// The live nodes and edges, in index order, renumbered.
    fn from(graph: PgStableGraph<N, E, Ty>) -> PgGraph<N, E, Ty> {
        let mut result: PgGraph<N, E, Ty> = PgGraph {
            nodes: Vec::new(),
            edges: Vec::new(),
            directed: graph.g.directed,
            ty: None,
        };
        let mut node_index_map = vec![PgNodeIndex::end(); graph.g.nodes.len()];
        for (i, node) in graph.g.nodes.into_iter().enumerate() {
            if let Some(w) = node.weight.into_iter().next() {
                node_index_map[i] = result.add_node(w);
            }
        }
        for edge in graph.g.edges {
            if let Some(w) = edge.weight.into_iter().next() {
                let source = node_index_map[edge.node[0].index()];
                let target = node_index_map[edge.node[1].index()];
                result.add_edge(source, target, w);
            }
        }
        result
    }
}

impl<N, E, Ty> std::ops::Index<PgNodeIndex> for PgStableGraph<N, E, Ty> {
    type Output = N;
    fn index(&self, i: PgNodeIndex) -> &N {
        let i = i.index();
        &self.g.nodes[i].weight[0]
    }
}
impl<N, E, Ty> std::ops::IndexMut<PgNodeIndex> for PgStableGraph<N, E, Ty> {
    fn index_mut(&mut self, i: PgNodeIndex) -> &mut N {
        let i = i.index();
        &mut self.g.nodes[i].weight[0]
    }
}
impl<N, E, Ty> std::ops::Index<PgEdgeIndex> for PgStableGraph<N, E, Ty> {
    type Output = E;
    fn index(&self, i: PgEdgeIndex) -> &E {
        let i = i.index();
        &self.g.edges[i].weight[0]
    }
}
impl<N, E, Ty> std::ops::IndexMut<PgEdgeIndex> for PgStableGraph<N, E, Ty> {
    fn index_mut(&mut self, i: PgEdgeIndex) -> &mut E {
        let i = i.index();
        &mut self.g.edges[i].weight[0]
    }
}

/// An algorithm's per-node vector over a view's positions, spread out to its raw indices (a
/// `StableGraph`'s vacant slots get `fill`).
fn __pg_spread<Id, R, T: Clone>(v: &PgView<Id, R>, compact: Vec<T>, fill: T) -> Vec<T> {
    if v.bound == compact.len() {
        return compact;
    }
    let mut out = vec![fill; v.bound];
    for (p, x) in compact.into_iter().enumerate() {
        out[v.raw[p]] = x;
    }
    out
}

// ---------------------------------------------------------------- walkers

/// What the walkers need of a graph (`&Graph`, `&StableGraph`, `&GraphMap`, `Reversed(..)`):
/// its nodes and their neighbors either way.
pub trait PgVisitGraph: Copy {
    type NodeId: Copy;
    fn __pg_node_ids(&self) -> Vec<Self::NodeId>;
    fn __pg_neighbors_directed(&self, n: Self::NodeId, dir: PgDirection) -> Vec<Self::NodeId>;
}
impl<'a, N, E, Ty: PgEdgeType> PgVisitGraph for &'a PgGraph<N, E, Ty> {
    type NodeId = PgNodeIndex;
    fn __pg_node_ids(&self) -> Vec<PgNodeIndex> {
        self.node_indices().collect()
    }
    fn __pg_neighbors_directed(&self, n: PgNodeIndex, dir: PgDirection) -> Vec<PgNodeIndex> {
        self.neighbors_directed(n, dir).collect()
    }
}
impl<'a, N, E, Ty: PgEdgeType> PgVisitGraph for &'a PgStableGraph<N, E, Ty> {
    type NodeId = PgNodeIndex;
    fn __pg_node_ids(&self) -> Vec<PgNodeIndex> {
        self.node_indices().collect()
    }
    fn __pg_neighbors_directed(&self, n: PgNodeIndex, dir: PgDirection) -> Vec<PgNodeIndex> {
        self.neighbors_directed(n, dir).collect()
    }
}
impl<'a, N: Copy + Ord + std::hash::Hash, E, Ty: PgEdgeType> PgVisitGraph
    for &'a PgGraphMap<N, E, Ty>
{
    type NodeId = N;
    fn __pg_node_ids(&self) -> Vec<N> {
        self.nodes.clone()
    }
    fn __pg_neighbors_directed(&self, n: N, dir: PgDirection) -> Vec<N> {
        self.neighbors_directed(n, dir).collect()
    }
}

/// `petgraph::visit::Reversed`: the graph with its edges turned around.
#[derive(Clone, Copy, Debug)]
pub struct PgReversed<G>(pub G);
impl<G: PgVisitGraph> PgVisitGraph for PgReversed<G> {
    type NodeId = G::NodeId;
    fn __pg_node_ids(&self) -> Vec<G::NodeId> {
        self.0.__pg_node_ids()
    }
    fn __pg_neighbors_directed(&self, n: G::NodeId, dir: PgDirection) -> Vec<G::NodeId> {
        self.0.__pg_neighbors_directed(n, dir.opposite())
    }
}
impl<G: PgVisitGraph> PgReversed<G> {
    pub fn neighbors(&self, n: G::NodeId) -> std::vec::IntoIter<G::NodeId> {
        self.0
            .__pg_neighbors_directed(n, PgDirection::Incoming)
            .into_iter()
    }
    pub fn neighbors_directed(
        &self,
        n: G::NodeId,
        dir: PgDirection,
    ) -> std::vec::IntoIter<G::NodeId> {
        self.0
            .__pg_neighbors_directed(n, dir.opposite())
            .into_iter()
    }
}

/// `petgraph::visit::Topo`: a node once all its incoming neighbors have been (nodes on a cycle
/// never are).
#[derive(Clone)]
pub struct PgTopo<N> {
    pub tovisit: Vec<N>,
    pub ordered: std::collections::HashSet<N>,
}
impl<N: Copy + Eq + std::hash::Hash> PgTopo<N> {
    pub fn new<G: PgVisitGraph<NodeId = N>>(graph: G) -> PgTopo<N> {
        let mut topo = PgTopo {
            tovisit: Vec::new(),
            ordered: std::collections::HashSet::new(),
        };
        topo.__pg_extend_with_initials(graph);
        topo
    }
    pub fn with_initials<G: PgVisitGraph<NodeId = N>, I: IntoIterator<Item = N>>(
        graph: G,
        initials: I,
    ) -> PgTopo<N> {
        let mut tovisit = Vec::new();
        for n in initials {
            if graph
                .__pg_neighbors_directed(n, PgDirection::Incoming)
                .is_empty()
            {
                tovisit.push(n);
            }
        }
        PgTopo {
            tovisit,
            ordered: std::collections::HashSet::new(),
        }
    }
    fn __pg_extend_with_initials<G: PgVisitGraph<NodeId = N>>(&mut self, graph: G) {
        for a in graph.__pg_node_ids() {
            if graph
                .__pg_neighbors_directed(a, PgDirection::Incoming)
                .is_empty()
            {
                self.tovisit.push(a);
            }
        }
    }
    pub fn reset<G: PgVisitGraph<NodeId = N>>(&mut self, graph: G) {
        self.ordered.clear();
        self.tovisit.clear();
        self.__pg_extend_with_initials(graph);
    }
    pub fn __pg_next<F: FnMut(N, PgDirection) -> Vec<N>>(&mut self, mut neighbors: F) -> Option<N> {
        while let Some(nix) = self.tovisit.pop() {
            if self.ordered.contains(&nix) {
                continue;
            }
            self.ordered.insert(nix);
            for neigh in neighbors(nix, PgDirection::Outgoing) {
                let mut ready = true;
                for b in neighbors(neigh, PgDirection::Incoming) {
                    if !self.ordered.contains(&b) {
                        ready = false;
                        break;
                    }
                }
                if ready {
                    self.tovisit.push(neigh);
                }
            }
            return Some(nix);
        }
        None
    }
}

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
    pub fn __pg_next<F: FnMut(N, PgDirection) -> Vec<N>>(&mut self, mut neighbors: F) -> Option<N> {
        if let Some(node) = self.stack.pop_front() {
            for succ in neighbors(node, PgDirection::Outgoing) {
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
    pub fn __pg_next<F: FnMut(N, PgDirection) -> Vec<N>>(&mut self, mut neighbors: F) -> Option<N> {
        while let Some(node) = self.stack.pop() {
            if self.discovered.insert(node) {
                for succ in neighbors(node, PgDirection::Outgoing) {
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
    pub fn __pg_next<F: FnMut(N, PgDirection) -> Vec<N>>(&mut self, mut neighbors: F) -> Option<N> {
        while let Some(&nx) = self.stack.last() {
            if self.discovered.insert(nx) {
                for succ in neighbors(nx, PgDirection::Outgoing) {
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

// ---------------------------------------------------------------- Dot

/// `petgraph::dot::RankDir`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgRankDir {
    TB,
    BT,
    LR,
    RL,
}

/// `petgraph::dot::Config`.
#[derive(Debug, PartialEq, Eq)]
pub enum PgDotConfig {
    NodeIndexLabel,
    EdgeIndexLabel,
    EdgeNoLabel,
    NodeNoLabel,
    GraphContentOnly,
    RankDir(PgRankDir),
}

/// What `Dot` prints of a graph: whether it's directed, its nodes (`to_index`, weight) and its
/// edges (`to_index` of each end, weight), in `node_references()` / `edge_references()` order.
pub trait PgDotGraph {
    type NodeWeight;
    type EdgeWeight;
    fn __pg_dot_directed(&self) -> bool;
    fn __pg_dot_nodes(&self) -> Vec<(usize, &Self::NodeWeight)>;
    fn __pg_dot_edges(&self) -> Vec<(usize, usize, &Self::EdgeWeight)>;
}
impl<N, E, Ty: PgEdgeType> PgDotGraph for PgGraph<N, E, Ty> {
    type NodeWeight = N;
    type EdgeWeight = E;
    fn __pg_dot_directed(&self) -> bool {
        self.directed
    }
    fn __pg_dot_nodes(&self) -> Vec<(usize, &N)> {
        let mut out = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            out.push((i, &n.weight));
        }
        out
    }
    fn __pg_dot_edges(&self) -> Vec<(usize, usize, &E)> {
        let mut out = Vec::new();
        for e in &self.edges {
            out.push((e.node[0].index(), e.node[1].index(), &e.weight));
        }
        out
    }
}
impl<N, E, Ty: PgEdgeType> PgDotGraph for PgStableGraph<N, E, Ty> {
    type NodeWeight = N;
    type EdgeWeight = E;
    fn __pg_dot_directed(&self) -> bool {
        self.g.directed
    }
    fn __pg_dot_nodes(&self) -> Vec<(usize, &N)> {
        let mut out = Vec::new();
        for (i, n) in self.g.nodes.iter().enumerate() {
            if let Some(w) = n.weight.first() {
                out.push((i, w));
            }
        }
        out
    }
    fn __pg_dot_edges(&self) -> Vec<(usize, usize, &E)> {
        let mut out = Vec::new();
        for e in &self.g.edges {
            if let Some(w) = e.weight.first() {
                out.push((e.node[0].index(), e.node[1].index(), w));
            }
        }
        out
    }
}
impl<N: Copy + Ord + std::hash::Hash, E, Ty: PgEdgeType> PgDotGraph for PgGraphMap<N, E, Ty> {
    type NodeWeight = N;
    type EdgeWeight = E;
    fn __pg_dot_directed(&self) -> bool {
        self.directed
    }
    fn __pg_dot_nodes(&self) -> Vec<(usize, &N)> {
        let mut out = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            out.push((i, n));
        }
        out
    }
    fn __pg_dot_edges(&self) -> Vec<(usize, usize, &E)> {
        let mut out = Vec::new();
        for e in &self.edges {
            out.push((self.node_ix[&e.0], self.node_ix[&e.1], &e.2));
        }
        out
    }
}
impl<'b, G: PgDotGraph> PgDotGraph for &'b G {
    type NodeWeight = G::NodeWeight;
    type EdgeWeight = G::EdgeWeight;
    fn __pg_dot_directed(&self) -> bool {
        (**self).__pg_dot_directed()
    }
    fn __pg_dot_nodes(&self) -> Vec<(usize, &G::NodeWeight)> {
        (**self).__pg_dot_nodes()
    }
    fn __pg_dot_edges(&self) -> Vec<(usize, usize, &G::EdgeWeight)> {
        (**self).__pg_dot_edges()
    }
}

/// `petgraph::dot::Dot`: the graph in Graphviz's format, its weights by `{}` or `{:?}` (as the
/// `Dot` itself is formatted).
pub struct PgDot<'a, G> {
    graph: G,
    config: &'a [PgDotConfig],
}
impl<'a, G> PgDot<'a, G> {
    pub fn new(graph: G) -> PgDot<'a, G> {
        PgDot { graph, config: &[] }
    }
    pub fn with_config(graph: G, config: &'a [PgDotConfig]) -> PgDot<'a, G> {
        PgDot { graph, config }
    }
}
impl<'a, G: PgDotGraph> std::fmt::Display for PgDot<'a, G>
where
    G::NodeWeight: std::fmt::Display,
    G::EdgeWeight: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let alt = f.alternate();
        let mut nodes = Vec::new();
        for (i, w) in self.graph.__pg_dot_nodes() {
            nodes.push((
                i,
                if alt {
                    format!("{:#}\n", w)
                } else {
                    format!("{}", w)
                },
            ));
        }
        let mut edges = Vec::new();
        for (a, b, w) in self.graph.__pg_dot_edges() {
            edges.push((
                a,
                b,
                if alt {
                    format!("{:#}\n", w)
                } else {
                    format!("{}", w)
                },
            ));
        }
        __pg_dot_write(f, self.config, self.graph.__pg_dot_directed(), nodes, edges)
    }
}
impl<'a, G: PgDotGraph> std::fmt::Debug for PgDot<'a, G>
where
    G::NodeWeight: std::fmt::Debug,
    G::EdgeWeight: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let alt = f.alternate();
        let mut nodes = Vec::new();
        for (i, w) in self.graph.__pg_dot_nodes() {
            nodes.push((
                i,
                if alt {
                    format!("{:#?}\n", w)
                } else {
                    format!("{:?}", w)
                },
            ));
        }
        let mut edges = Vec::new();
        for (a, b, w) in self.graph.__pg_dot_edges() {
            edges.push((
                a,
                b,
                if alt {
                    format!("{:#?}\n", w)
                } else {
                    format!("{:?}", w)
                },
            ));
        }
        __pg_dot_write(f, self.config, self.graph.__pg_dot_directed(), nodes, edges)
    }
}

/// A label between `"`s: `"` and `\` escaped, a newline Graphviz's left-justified `\l`.
fn __pg_dot_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\l"),
            _ => out.push(c),
        }
    }
    out
}

fn __pg_dot_write(
    f: &mut std::fmt::Formatter,
    config: &[PgDotConfig],
    directed: bool,
    nodes: Vec<(usize, String)>,
    edges: Vec<(usize, usize, String)>,
) -> std::fmt::Result {
    let has = |c: PgDotConfig| config.contains(&c);
    let content_only = has(PgDotConfig::GraphContentOnly);
    let mut rank_dir = None;
    for c in config {
        if let PgDotConfig::RankDir(d) = c {
            rank_dir = Some(*d);
        }
    }
    if !content_only {
        writeln!(f, "{} {{", if directed { "digraph" } else { "graph" })?;
    }
    if let Some(d) = rank_dir {
        let value = match d {
            PgRankDir::TB => "TB",
            PgRankDir::BT => "BT",
            PgRankDir::LR => "LR",
            PgRankDir::RL => "RL",
        };
        writeln!(f, "    rankdir=\"{}\"", value)?;
    }
    for (i, label) in nodes {
        write!(f, "    {} [ ", i)?;
        if !has(PgDotConfig::NodeNoLabel) {
            write!(f, "label = \"")?;
            if has(PgDotConfig::NodeIndexLabel) {
                write!(f, "{}", i)?;
            } else {
                write!(f, "{}", __pg_dot_escape(&label))?;
            }
            write!(f, "\" ")?;
        }
        writeln!(f, "]")?;
    }
    for (k, (a, b, label)) in edges.into_iter().enumerate() {
        write!(
            f,
            "    {} {} {} [ ",
            a,
            if directed { "->" } else { "--" },
            b
        )?;
        if !has(PgDotConfig::EdgeNoLabel) {
            write!(f, "label = \"")?;
            if has(PgDotConfig::EdgeIndexLabel) {
                write!(f, "{}", k)?;
            } else {
                write!(f, "{}", __pg_dot_escape(&label))?;
            }
            write!(f, "\" ")?;
        }
        writeln!(f, "]")?;
    }
    if !content_only {
        writeln!(f, "}}")?;
    }
    Ok(())
}

// ---------------------------------------------------------------- algorithms

/// A graph as the algorithms see it: node ids by position, each node's `edges(n)` (with the
/// target's position), its incoming neighbors (for `Reversed`), and `edge_references()`; each
/// position's index (`to_index`: a `StableGraph`'s skip its vacant slots) and `node_bound()`.
pub struct PgView<Id, R> {
    ids: Vec<Id>,
    out: Vec<Vec<(usize, R)>>,
    inc: Vec<Vec<usize>>,
    refs: Vec<(usize, usize, R)>,
    raw: Vec<usize>,
    bound: usize,
    directed: bool,
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
    let (distances, predecessors) = __pg_bellman_ford_relax(v, source, &mut weight);
    for i in 0..n {
        for &(j, r) in &v.out[i] {
            let w = weight(r);
            if distances[i] + w < distances[j] {
                return Err(PgNegativeCycle(()));
            }
        }
    }
    let mut preds = Vec::new();
    for p in predecessors {
        preds.push(p.map(|p| v.ids[p]));
    }
    Ok(PgPaths {
        distances,
        predecessors: preds,
    })
}

/// Bellman-Ford's first two steps: the distances from `source` after relaxing every edge up to
/// `n - 1` times, and each node's predecessor (by position).
fn __pg_bellman_ford_relax<Id, R, K, W>(
    v: &PgView<Id, R>,
    source: usize,
    weight: &mut W,
) -> (Vec<K>, Vec<Option<usize>>)
where
    R: Copy,
    K: PgFloatMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
    W: FnMut(R) -> K,
{
    let n = v.ids.len();
    let mut predecessors: Vec<Option<usize>> = vec![None; n];
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
                    predecessors[j] = Some(i);
                    did_update = true;
                }
            }
        }
        if !did_update {
            break;
        }
    }
    (distances, predecessors)
}

/// `find_negative_cycle`: from the first edge that still relaxes, back along the predecessors
/// until the walk comes round.
fn __pg_find_negative_cycle_in<Id, R, K, W>(
    v: &PgView<Id, R>,
    source: usize,
    mut weight: W,
) -> Option<Vec<Id>>
where
    Id: Copy,
    R: Copy,
    K: PgFloatMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
    W: FnMut(R) -> K,
{
    let n = v.ids.len();
    let (distance, predecessor) = __pg_bellman_ford_relax(v, source, &mut weight);
    let mut path: Vec<usize> = Vec::new();
    'outer: for i in 0..n {
        for &(j, r) in &v.out[i] {
            let w = weight(r);
            if distance[i] + w < distance[j] {
                let start = j;
                let mut node = start;
                let mut visited = vec![false; n];
                loop {
                    let ancestor = match predecessor[node] {
                        Some(p) => p,
                        None => node,
                    };
                    if ancestor == start {
                        path.push(ancestor);
                        break;
                    } else if visited[ancestor] {
                        let pos = path.iter().position(|&p| p == ancestor).unwrap();
                        path = path[pos..].to_vec();
                        break;
                    }
                    path.push(ancestor);
                    visited[ancestor] = true;
                    node = ancestor;
                }
                break 'outer;
            }
        }
    }
    if path.is_empty() {
        return None;
    }
    path.reverse();
    let mut out = Vec::new();
    for p in path {
        out.push(v.ids[p]);
    }
    Some(out)
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
    for i in 0..n {
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

/// `all_simple_paths`' iterator: a depth-first walk keeping the path so far, yielding each
/// time it can step onto `to` (petgraph's order, lazily). The paths are `Vec`s, whatever
/// collection the turbofish names.
pub struct PgSimplePaths<Id> {
    ids: Vec<Id>,
    out: Vec<Vec<usize>>,
    to: usize,
    min_length: usize,
    max_length: usize,
    visited: Vec<usize>,
    on_path: Vec<bool>,
    // (A node on the path, and the next of its neighbors to try.)
    stack: Vec<(usize, usize)>,
}
impl<Id: Copy> PgSimplePaths<Id> {
    fn __pg_pop(&mut self) {
        self.stack.pop();
        if let Some(x) = self.visited.pop() {
            self.on_path[x] = false;
        }
    }
}
impl<Id: Copy> Iterator for PgSimplePaths<Id> {
    type Item = Vec<Id>;
    fn next(&mut self) -> Option<Vec<Id>> {
        while let Some(&(node, k)) = self.stack.last() {
            let last = self.stack.len() - 1;
            if k >= self.out[node].len() {
                self.__pg_pop();
                continue;
            }
            let child = self.out[node][k];
            self.stack[last] = (node, k + 1);
            if self.on_path[child] {
                continue;
            }
            if self.visited.len() < self.max_length {
                if child == self.to {
                    if self.visited.len() >= self.min_length {
                        return Some(self.__pg_path());
                    }
                } else {
                    self.visited.push(child);
                    self.on_path[child] = true;
                    self.stack.push((child, 0));
                }
            } else {
                // (At the longest: is `to` among this node's remaining neighbors?)
                let mut found = child == self.to;
                if !found {
                    let mut j = k + 1;
                    while j < self.out[node].len() {
                        let x = self.out[node][j];
                        j += 1;
                        if x == self.to && !self.on_path[x] {
                            found = true;
                            break;
                        }
                    }
                    self.stack[last] = (node, j);
                }
                if found && self.visited.len() >= self.min_length {
                    return Some(self.__pg_path());
                }
                self.__pg_pop();
            }
        }
        None
    }
}
impl<Id: Copy> PgSimplePaths<Id> {
    fn __pg_path(&self) -> Vec<Id> {
        let mut path = Vec::new();
        for &p in &self.visited {
            path.push(self.ids[p]);
        }
        path.push(self.ids[self.to]);
        path
    }
}

fn __pg_all_simple_paths_in<Id: Copy, R>(
    v: &PgView<Id, R>,
    from: usize,
    to: usize,
    min_intermediate_nodes: usize,
    max_intermediate_nodes: Option<usize>,
) -> PgSimplePaths<Id> {
    let n = v.ids.len();
    let max_length = match max_intermediate_nodes {
        Some(l) => l + 1,
        None => n.saturating_sub(1),
    };
    let mut out = Vec::new();
    for edges in &v.out {
        let mut targets = Vec::new();
        for &(t, _) in edges {
            targets.push(t);
        }
        out.push(targets);
    }
    let mut on_path = vec![false; n];
    on_path[from] = true;
    PgSimplePaths {
        ids: v.ids.clone(),
        out,
        to,
        min_length: min_intermediate_nodes + 1,
        max_length,
        visited: vec![from],
        on_path,
        stack: vec![(from, 0)],
    }
}

fn __pg_floyd_warshall_in<Id, R, K, F>(
    v: &PgView<Id, R>,
    mut edge_cost: F,
) -> Result<std::collections::HashMap<(Id, Id), K>, PgNegativeCycle>
where
    Id: Copy + Eq + std::hash::Hash,
    R: Copy,
    K: PgBoundedMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
    F: FnMut(R) -> K,
{
    let n = v.ids.len();
    let mut dist = vec![vec![K::__pg_max(); n]; n];
    for &(s, t, r) in &v.refs {
        let cost = edge_cost(r);
        if dist[s][t] > cost {
            dist[s][t] = cost;
            if !v.directed {
                dist[t][s] = cost;
            }
        }
    }
    for i in 0..n {
        dist[i][i] = K::default();
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                let (result, overflow) = dist[i][k].__pg_overflowing_add(dist[k][j]);
                if !overflow && dist[i][j] > result {
                    dist[i][j] = result;
                }
            }
        }
    }
    for i in 0..n {
        if dist[i][i] < K::default() {
            return Err(PgNegativeCycle(()));
        }
    }
    let mut out = std::collections::HashMap::new();
    for i in 0..n {
        for j in 0..n {
            out.insert((v.ids[i], v.ids[j]), dist[i][j]);
        }
    }
    Ok(out)
}

/// `k_shortest_path`: each node's cost the `k`-th time the search pops it.
fn __pg_k_shortest_path_in<Id, R, K, F>(
    v: &PgView<Id, R>,
    start: usize,
    goal: Option<usize>,
    k: usize,
    mut edge_cost: F,
) -> std::collections::HashMap<Id, K>
where
    Id: Copy + Eq + std::hash::Hash,
    R: Copy,
    K: Copy + Default + PartialOrd + std::ops::Add<Output = K>,
    F: FnMut(R) -> K,
{
    let mut counter = vec![0usize; v.ids.len()];
    let mut scores = std::collections::HashMap::new();
    let mut visit_next = std::collections::BinaryHeap::new();
    visit_next.push(PgMinScored(K::default(), start));
    while let Some(PgMinScored(node_score, node)) = visit_next.pop() {
        counter[node] += 1;
        let current_counter = counter[node];
        if current_counter > k {
            continue;
        }
        if current_counter == k {
            scores.insert(v.ids[node], node_score);
        }
        if goal == Some(node) && current_counter == k {
            break;
        }
        for &(next, r) in &v.out[node] {
            visit_next.push(PgMinScored(node_score + edge_cost(r), next));
        }
    }
    scores
}

/// `is_bipartite_undirected`: a breadth-first two-coloring from `start`.
fn __pg_is_bipartite_undirected_in<Id, R>(v: &PgView<Id, R>, start: usize) -> bool {
    let n = v.ids.len();
    let mut red = vec![false; n];
    let mut blue = vec![false; n];
    red[start] = true;
    let mut stack = std::collections::VecDeque::new();
    stack.push_front(start);
    while let Some(node) = stack.pop_front() {
        let is_red = red[node];
        let is_blue = blue[node];
        assert!(is_red ^ is_blue);
        for &(neighbour, _) in &v.out[node] {
            let is_neighbour_red = red[neighbour];
            let is_neighbour_blue = blue[neighbour];
            if (is_red && is_neighbour_red) || (is_blue && is_neighbour_blue) {
                return false;
            }
            if !is_neighbour_red && !is_neighbour_blue {
                if is_red {
                    blue[neighbour] = true;
                } else {
                    red[neighbour] = true;
                }
                stack.push_back(neighbour);
            }
        }
    }
    true
}

/// `bridges`: the edges (in `edge_references()` order) whose removal splits their component,
/// by the earliest node each depth-first subtree reaches back to.
fn __pg_bridges_in<Id, R: Copy>(v: &PgView<Id, R>) -> Vec<R> {
    let n = v.ids.len();
    let mut clock = 0usize;
    let mut visit_time: Vec<Option<usize>> = vec![None; n];
    let mut earliest_backedge = vec![usize::MAX; n];
    for start in 0..n {
        if visit_time[start].is_some() {
            continue;
        }
        visit_time[start] = Some(clock);
        clock += 1;
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(&(node, k)) = stack.last() {
            let last = stack.len() - 1;
            let parent = if last > 0 {
                Some(stack[last - 1].0)
            } else {
                None
            };
            if k < v.out[node].len() {
                stack[last] = (node, k + 1);
                let child = v.out[node][k].0;
                if parent != Some(child) {
                    if let Some(time) = visit_time[child] {
                        earliest_backedge[node] = earliest_backedge[node].min(time);
                    } else {
                        visit_time[child] = Some(clock);
                        clock += 1;
                        stack.push((child, 0));
                    }
                }
            } else {
                if let Some(p) = parent {
                    earliest_backedge[p] = earliest_backedge[p].min(earliest_backedge[node]);
                }
                stack.pop();
            }
        }
    }
    let mut out = Vec::new();
    for &(s, t, r) in &v.refs {
        let (parent, node) = if visit_time[s].unwrap() < visit_time[t].unwrap() {
            (s, t)
        } else {
            (t, s)
        };
        if earliest_backedge[node] > visit_time[parent].unwrap() {
            out.push(r);
        }
    }
    out
}

/// `articulation_points`: petgraph's iterative depth-first search, its recursion as steps on a
/// stack (0: visit, 1: a child, 2: back from a child, 3: the root's child count).
fn __pg_articulation_points_in<Id: Copy + Eq + std::hash::Hash, R>(
    v: &PgView<Id, R>,
) -> std::collections::HashSet<Id> {
    let n = v.ids.len();
    let mut visited = vec![false; n];
    let mut low = vec![usize::MAX; n];
    let mut disc = vec![usize::MAX; n];
    let mut parent = vec![usize::MAX; n];
    let mut time = 0usize;
    let mut points = std::collections::HashSet::new();
    for root in 0..n {
        if visited[root] {
            continue;
        }
        let mut stack: Vec<(u8, usize, usize)> = vec![(0, root, 0)];
        let mut children_count = vec![0usize; n];
        while let Some((step, current, child)) = stack.pop() {
            match step {
                0 => {
                    visited[current] = true;
                    disc[current] = time;
                    low[current] = time;
                    time += 1;
                    stack.push((3, current, 0));
                    for &(c, _) in &v.out[current] {
                        stack.push((1, current, c));
                    }
                }
                1 => {
                    if !visited[child] {
                        parent[child] = current;
                        children_count[current] += 1;
                        stack.push((2, current, child));
                        stack.push((0, child, 0));
                    } else if child != parent[current] {
                        low[current] = low[current].min(disc[child]);
                    }
                }
                2 => {
                    low[current] = low[current].min(low[child]);
                    if parent[current] != usize::MAX && low[child] >= disc[current] {
                        points.insert(v.ids[current]);
                    }
                }
                _ => {
                    if parent[current] == usize::MAX && children_count[current] > 1 {
                        points.insert(v.ids[current]);
                    }
                }
            }
        }
    }
    points
}

/// `spfa`: Bellman-Ford with a queue of the nodes whose distance just went down; a node queued
/// `node_bound()` times means a negative cycle.
fn __pg_spfa_in<Id, R, K, F>(
    v: &PgView<Id, R>,
    source: usize,
    mut edge_cost: F,
) -> Result<PgPaths<Id, K>, PgNegativeCycle>
where
    Id: Copy,
    R: Copy,
    K: PgBoundedMeasure + Copy + PartialOrd + Default + std::ops::Add<Output = K>,
    F: FnMut(R) -> K,
{
    let n = v.ids.len();
    let mut distances = vec![K::__pg_max(); n];
    let mut predecessors: Vec<Option<usize>> = vec![None; n];
    distances[source] = K::default();
    let mut queue = std::collections::VecDeque::new();
    let mut in_queue = vec![false; n];
    queue.push_back(source);
    in_queue[source] = true;
    let mut visits = vec![0usize; n];
    while let Some(i) = queue.pop_front() {
        in_queue[i] = false;
        if visits[i] >= v.bound {
            return Err(PgNegativeCycle(()));
        }
        visits[i] += 1;
        for &(j, r) in &v.out[i] {
            let w = edge_cost(r);
            let (dist, overflow) = distances[i].__pg_overflowing_add(w);
            if !overflow && dist < distances[j] {
                distances[j] = dist;
                predecessors[j] = Some(i);
                if !in_queue[j] {
                    in_queue[j] = true;
                    queue.push_back(j);
                }
            }
        }
    }
    let mut preds = Vec::new();
    for p in predecessors {
        preds.push(p.map(|p| v.ids[p]));
    }
    Ok(PgPaths {
        distances,
        predecessors: preds,
    })
}
