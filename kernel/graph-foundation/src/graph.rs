//! Generic graph container: nodes keyed by [`NodeId`], directed edges keyed
//! by [`EdgeId`]. Iteration is deterministic (BTreeMap-backed).
//!
//! Domain-specific traversal algorithms are explicitly out of scope — write
//! your own using the provided [`Graph::nodes`] / [`Graph::edges`] /
//! [`Graph::outgoing`] / [`Graph::incoming`] iterators.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::id::{EdgeId, NodeId};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by mutating operations on a [`Graph`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GraphError {
    /// A lookup by [`NodeId`] found no entry.
    #[error("node {0} not found")]
    NodeNotFound(NodeId),
    /// A lookup by [`EdgeId`] found no entry.
    #[error("edge {0} not found")]
    EdgeNotFound(EdgeId),
    /// An insert attempted to reuse an already-present [`NodeId`].
    #[error("duplicate node id {0}")]
    DuplicateNode(NodeId),
    /// An insert attempted to reuse an already-present [`EdgeId`].
    #[error("duplicate edge id {0}")]
    DuplicateEdge(EdgeId),
    /// Edge endpoints reference nodes not currently in the graph.
    #[error("edge endpoints not in graph: src={src} dst={dst}")]
    DanglingEndpoint {
        /// Source node id that was missing.
        src: NodeId,
        /// Destination node id that was missing.
        dst: NodeId,
    },
}

// ---------------------------------------------------------------------------
// EdgeRecord
// ---------------------------------------------------------------------------

/// An edge together with its source, destination, and payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeRecord<E> {
    /// Source (origin) node of the directed edge.
    pub src: NodeId,
    /// Destination (target) node of the directed edge.
    pub dst: NodeId,
    /// Domain-specific edge payload.
    pub data: E,
}

// ---------------------------------------------------------------------------
// Graph
// ---------------------------------------------------------------------------

/// Generic graph: nodes keyed by [`NodeId`], directed edges keyed by
/// [`EdgeId`]. Iteration is deterministic (BTreeMap-backed).
///
/// Domain-specific traversal algorithms are explicitly out of scope — write
/// your own using `nodes()`/`edges()`/`outgoing(...)`/`incoming(...)`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Graph<N, E> {
    nodes: BTreeMap<NodeId, N>,
    edges: BTreeMap<EdgeId, EdgeRecord<E>>,
    /// Forward adjacency: src → set of outgoing `EdgeId`s.
    outgoing: BTreeMap<NodeId, BTreeSet<EdgeId>>,
    /// Reverse adjacency: dst → set of incoming `EdgeId`s.
    incoming: BTreeMap<NodeId, BTreeSet<EdgeId>>,
}

impl<N: Clone, E: Clone> Graph<N, E> {
    /// Construct an empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            outgoing: BTreeMap::new(),
            incoming: BTreeMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Node operations
    // -----------------------------------------------------------------------

    /// Insert a new node.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::DuplicateNode`] when `id` is already present.
    pub fn insert_node(&mut self, id: NodeId, node: N) -> Result<(), GraphError> {
        if self.nodes.contains_key(&id) {
            return Err(GraphError::DuplicateNode(id));
        }
        self.nodes.insert(id, node);
        Ok(())
    }

    /// Replace the payload of an existing node. Returns the old value.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::NodeNotFound`] when `id` is not present.
    pub fn replace_node(&mut self, id: NodeId, node: N) -> Result<N, GraphError> {
        let slot = self
            .nodes
            .get_mut(&id)
            .ok_or(GraphError::NodeNotFound(id))?;
        Ok(std::mem::replace(slot, node))
    }

    /// Remove a node and all edges that touch it (incoming or outgoing).
    /// Returns the previous node payload.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::NodeNotFound`] when `id` is not present.
    pub fn remove_node(&mut self, id: NodeId) -> Result<N, GraphError> {
        let node = self.nodes.remove(&id).ok_or(GraphError::NodeNotFound(id))?;

        // Collect all edge ids to remove (outgoing + incoming).
        let mut to_remove: Vec<EdgeId> = Vec::new();
        if let Some(outs) = self.outgoing.get(&id) {
            to_remove.extend(outs);
        }
        if let Some(ins) = self.incoming.get(&id) {
            to_remove.extend(ins);
        }

        for eid in to_remove {
            self.remove_edge_unchecked(eid);
        }

        self.outgoing.remove(&id);
        self.incoming.remove(&id);

        Ok(node)
    }

    /// Look up a node by id.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&N> {
        self.nodes.get(&id)
    }

    /// Look up a node mutably by id.
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut N> {
        self.nodes.get_mut(&id)
    }

    /// Iterate over all (id, node) pairs in deterministic order.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &N)> {
        self.nodes.iter().map(|(&id, n)| (id, n))
    }

    // -----------------------------------------------------------------------
    // Edge operations
    // -----------------------------------------------------------------------

    /// Insert a directed edge from `src` to `dst`.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::DuplicateEdge`] when `id` is already present,
    /// or [`GraphError::DanglingEndpoint`] when either endpoint is absent.
    pub fn insert_edge(
        &mut self,
        id: EdgeId,
        src: NodeId,
        dst: NodeId,
        edge: E,
    ) -> Result<(), GraphError> {
        if self.edges.contains_key(&id) {
            return Err(GraphError::DuplicateEdge(id));
        }
        let src_ok = self.nodes.contains_key(&src);
        let dst_ok = self.nodes.contains_key(&dst);
        if !src_ok || !dst_ok {
            return Err(GraphError::DanglingEndpoint { src, dst });
        }
        self.edges.insert(
            id,
            EdgeRecord {
                src,
                dst,
                data: edge,
            },
        );
        self.outgoing.entry(src).or_default().insert(id);
        self.incoming.entry(dst).or_default().insert(id);
        Ok(())
    }

    /// Replace the payload of an existing edge. Returns the old payload.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::EdgeNotFound`] when `id` is not present.
    pub fn replace_edge(&mut self, id: EdgeId, edge: E) -> Result<E, GraphError> {
        let rec = self
            .edges
            .get_mut(&id)
            .ok_or(GraphError::EdgeNotFound(id))?;
        Ok(std::mem::replace(&mut rec.data, edge))
    }

    /// Remove an edge. Returns the full [`EdgeRecord`].
    ///
    /// # Errors
    ///
    /// Returns [`GraphError::EdgeNotFound`] when `id` is not present.
    ///
    /// # Panics
    ///
    /// Never panics in practice: existence is confirmed before calling the
    /// internal helper that asserts presence.
    pub fn remove_edge(&mut self, id: EdgeId) -> Result<EdgeRecord<E>, GraphError> {
        if !self.edges.contains_key(&id) {
            return Err(GraphError::EdgeNotFound(id));
        }
        Ok(self
            .remove_edge_unchecked(id)
            .expect("just confirmed present"))
    }

    /// Look up an edge by id.
    #[must_use]
    pub fn edge(&self, id: EdgeId) -> Option<&EdgeRecord<E>> {
        self.edges.get(&id)
    }

    /// Look up an edge mutably by id.
    pub fn edge_mut(&mut self, id: EdgeId) -> Option<&mut EdgeRecord<E>> {
        self.edges.get_mut(&id)
    }

    /// Iterate over all (id, record) pairs in deterministic order.
    pub fn edges(&self) -> impl Iterator<Item = (EdgeId, &EdgeRecord<E>)> {
        self.edges.iter().map(|(&id, e)| (id, e))
    }

    /// Iterate over the [`EdgeId`]s of all outgoing edges from `src`.
    pub fn outgoing(&self, src: NodeId) -> impl Iterator<Item = EdgeId> + '_ {
        self.outgoing
            .get(&src)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    /// Iterate over the [`EdgeId`]s of all incoming edges to `dst`.
    pub fn incoming(&self, dst: NodeId) -> impl Iterator<Item = EdgeId> + '_ {
        self.incoming
            .get(&dst)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    // -----------------------------------------------------------------------
    // Counts (Tier-A counters per ADR-115 phase-1)
    // -----------------------------------------------------------------------

    /// Returns the number of nodes currently in the graph.
    ///
    /// O(1). Tier-A counter per ADR-115 phase-1 (graph-metrics substrate
    /// design, sub-decisions 1+2). Every mutation that adds or removes a
    /// node is transactional through [`Graph::insert_node`] /
    /// [`Graph::remove_node`]; the BTreeMap-backed `nodes` storage's
    /// `.len()` is the canonical count and is itself O(1) per the
    /// `std::collections::BTreeMap::len` contract.
    ///
    /// # Companion metrics
    ///
    /// - [`Graph::edge_count`] — edge-side counterpart (this same Tier).
    /// - `cad-core::OperatorGraph::operator_count` — domain-specific
    ///   thin wrapper exposing this count under the operator-graph
    ///   semantic name (every node in `OperatorGraph` is an operator).
    /// - `constraint_count` — deferred per ADR-115; depends on a future
    ///   constraint-system substrate that does not yet exist.
    /// - `invalidation_count` — deferred per ADR-115; cross-substrate
    ///   concern (cad-projection head-advance + cad-core checkpoint
    ///   commits) that lands in phase-3+ via the event-sourced
    ///   `GraphEvent` stream (ADR-115 sub-decision 4).
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of edges currently in the graph.
    ///
    /// O(1). Tier-A counter per ADR-115 phase-1 (graph-metrics substrate
    /// design, sub-decisions 1+2). Every mutation that adds or removes
    /// an edge is transactional through [`Graph::insert_edge`] /
    /// [`Graph::remove_edge`] (and the cascade path inside
    /// [`Graph::remove_node`]); the BTreeMap-backed `edges` storage's
    /// `.len()` is the canonical count and is itself O(1) per the
    /// `std::collections::BTreeMap::len` contract.
    ///
    /// # Companion metrics
    ///
    /// See [`Graph::node_count`] for the node-side counterpart and the
    /// list of deferred companion counters (`operator_count` exposed by
    /// `cad-core::OperatorGraph`; `constraint_count` /
    /// `invalidation_count` deferred per ADR-115).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Remove an edge without checking for existence. Returns the record if
    /// present, updating both adjacency caches.
    fn remove_edge_unchecked(&mut self, id: EdgeId) -> Option<EdgeRecord<E>> {
        let rec = self.edges.remove(&id)?;
        if let Some(set) = self.outgoing.get_mut(&rec.src) {
            set.remove(&id);
        }
        if let Some(set) = self.incoming.get_mut(&rec.dst) {
            set.remove(&id);
        }
        Some(rec)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn n(v: u128) -> NodeId {
        NodeId::from_raw(v)
    }
    fn e(v: u128) -> EdgeId {
        EdgeId::from_raw(v)
    }

    #[test]
    fn insert_and_retrieve_node() {
        let mut g: Graph<&str, ()> = Graph::new();
        g.insert_node(n(1), "hello").unwrap();
        assert_eq!(g.node(n(1)), Some(&"hello"));
    }

    #[test]
    fn duplicate_node_fails() {
        let mut g: Graph<i32, ()> = Graph::new();
        g.insert_node(n(1), 10).unwrap();
        let err = g.insert_node(n(1), 20).unwrap_err();
        assert_eq!(err, GraphError::DuplicateNode(n(1)));
    }

    #[test]
    fn insert_edge_dangling_fails() {
        let mut g: Graph<i32, &str> = Graph::new();
        g.insert_node(n(1), 1).unwrap();
        // n(2) is absent
        let err = g.insert_edge(e(1), n(1), n(2), "x").unwrap_err();
        assert!(matches!(err, GraphError::DanglingEndpoint { .. }));
    }

    #[test]
    fn remove_node_cascades_edges() {
        let mut g: Graph<i32, i32> = Graph::new();
        g.insert_node(n(1), 1).unwrap();
        g.insert_node(n(2), 2).unwrap();
        g.insert_edge(e(10), n(1), n(2), 99).unwrap();
        assert_eq!(g.edge_count(), 1);
        g.remove_node(n(1)).unwrap();
        assert_eq!(
            g.edge_count(),
            0,
            "removing src node must cascade edge removal"
        );
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn outgoing_incoming_consistent() {
        let mut g: Graph<i32, i32> = Graph::new();
        g.insert_node(n(1), 1).unwrap();
        g.insert_node(n(2), 2).unwrap();
        g.insert_edge(e(10), n(1), n(2), 0).unwrap();

        let out: Vec<_> = g.outgoing(n(1)).collect();
        assert_eq!(out, vec![e(10)]);
        let inc: Vec<_> = g.incoming(n(2)).collect();
        assert_eq!(inc, vec![e(10)]);
    }

    #[test]
    fn outgoing_incoming_after_remove() {
        let mut g: Graph<i32, i32> = Graph::new();
        g.insert_node(n(1), 1).unwrap();
        g.insert_node(n(2), 2).unwrap();
        g.insert_edge(e(10), n(1), n(2), 0).unwrap();
        g.remove_edge(e(10)).unwrap();

        assert_eq!(g.outgoing(n(1)).count(), 0);
        assert_eq!(g.incoming(n(2)).count(), 0);
    }

    #[test]
    fn replace_node() {
        let mut g: Graph<i32, ()> = Graph::new();
        g.insert_node(n(1), 10).unwrap();
        let old = g.replace_node(n(1), 20).unwrap();
        assert_eq!(old, 10);
        assert_eq!(g.node(n(1)), Some(&20));
    }

    #[test]
    fn replace_edge() {
        let mut g: Graph<i32, i32> = Graph::new();
        g.insert_node(n(1), 1).unwrap();
        g.insert_node(n(2), 2).unwrap();
        g.insert_edge(e(10), n(1), n(2), 5).unwrap();
        let old = g.replace_edge(e(10), 99).unwrap();
        assert_eq!(old, 5);
        assert_eq!(g.edge(e(10)).map(|r| r.data), Some(99));
    }

    // ---------------------------------------------------------------------
    // Tier-A counter tests (ADR-115 phase-1)
    // ---------------------------------------------------------------------
    //
    // These tests pin the Tier-A counters' transactional-update contract:
    // every node/edge insert and remove is reflected in O(1) by the
    // counter accessors. Cascading-remove behaviour (remove_node drops
    // touching edges) is exercised so the counters stay consistent
    // across the most complex substrate-level mutation.

    #[test]
    fn empty_graph_has_zero_node_and_edge_count() {
        let g: Graph<i32, i32> = Graph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn node_count_reflects_add_node_calls() {
        let mut g: Graph<i32, ()> = Graph::new();
        assert_eq!(g.node_count(), 0);
        g.insert_node(n(1), 10).unwrap();
        assert_eq!(g.node_count(), 1);
        g.insert_node(n(2), 20).unwrap();
        assert_eq!(g.node_count(), 2);
        g.insert_node(n(3), 30).unwrap();
        assert_eq!(g.node_count(), 3);
        // edge_count untouched by node-only mutations.
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn edge_count_reflects_add_edge_calls() {
        let mut g: Graph<(), ()> = Graph::new();
        // Set up 4 nodes so we have somewhere to attach 3 edges.
        g.insert_node(n(1), ()).unwrap();
        g.insert_node(n(2), ()).unwrap();
        g.insert_node(n(3), ()).unwrap();
        g.insert_node(n(4), ()).unwrap();
        assert_eq!(g.edge_count(), 0);
        g.insert_edge(e(10), n(1), n(2), ()).unwrap();
        assert_eq!(g.edge_count(), 1);
        g.insert_edge(e(11), n(2), n(3), ()).unwrap();
        assert_eq!(g.edge_count(), 2);
        g.insert_edge(e(12), n(3), n(4), ()).unwrap();
        assert_eq!(g.edge_count(), 3);
        // node_count untouched by edge-only mutations.
        assert_eq!(g.node_count(), 4);
    }

    #[test]
    fn node_count_reflects_remove_node_cascading_edges() {
        // Build a 3-node fan: n(1) → n(2), n(1) → n(3); plus n(2) → n(3).
        // Removing n(1) must drop n(1)→n(2) and n(1)→n(3) (2 edges
        // cascaded), leaving n(2)→n(3) intact.
        let mut g: Graph<(), ()> = Graph::new();
        g.insert_node(n(1), ()).unwrap();
        g.insert_node(n(2), ()).unwrap();
        g.insert_node(n(3), ()).unwrap();
        g.insert_edge(e(10), n(1), n(2), ()).unwrap();
        g.insert_edge(e(11), n(1), n(3), ()).unwrap();
        g.insert_edge(e(12), n(2), n(3), ()).unwrap();
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 3);

        g.remove_node(n(1)).unwrap();

        assert_eq!(g.node_count(), 2, "removed n(1); count drops by 1");
        assert_eq!(
            g.edge_count(),
            1,
            "edges (1,2) + (1,3) cascade-removed; only (2,3) remains"
        );
    }

    /// Documents the O(1) property for `node_count`. No perf benchmark —
    /// the property is structural: `BTreeMap::len()` is O(1) per the
    /// `std::collections::BTreeMap::len` contract, and `node_count` is a
    /// single-line forwarder. Tier-A invariant per ADR-115 sub-decision 2:
    /// counter accessors MUST be queryable in constant time and MUST NOT
    /// allocate. Asserting only that successive calls return the same
    /// value (i.e. the accessor is stable) — the deeper guarantee is
    /// enforced by the source-level shape, not by a runtime test.
    #[test]
    fn node_count_o1_property() {
        let mut g: Graph<u32, ()> = Graph::new();
        for i in 0u32..16 {
            g.insert_node(n(u128::from(i)), i).unwrap();
        }
        // Successive calls return identical values without mutating state.
        let first = g.node_count();
        let second = g.node_count();
        let third = g.node_count();
        assert_eq!(first, 16);
        assert_eq!(first, second);
        assert_eq!(second, third);
    }
}
