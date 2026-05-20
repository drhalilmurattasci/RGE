//! `rge-script-graph` — script graph foundation wrapper.
//!
//! Failure class: snapshot-recoverable
//!
//! Phase 8 foundation slice. [`ScriptGraph`] is a thin wrapper over
//! `rge_kernel_graph_foundation::Graph` that stores data-only script nodes
//! keyed by content-derived [`NodeId`] and connects them with data-only
//! [`ScriptEdge`] payloads keyed by content-derived [`EdgeId`]. Like
//! `crates/anim-graph` and `crates/material-graph`, the script graph is
//! rebuildable structural state that participates in snapshot/restore —
//! a rejected mutation is recovered by restoring the last good snapshot
//! rather than terminating the session.
//!
//! This crate is the foundation layer only: it carries no script execution,
//! WASM generation, evaluator, interpreter, traversal runtime, editor UI,
//! ECS integration, runtime scheduling, or script-host integration.
//! [`ScriptNode`] and [`ScriptEdge`] are uninterpreted data-only payloads
//! used solely for deterministic ID derivation and duplicate detection.

#![forbid(unsafe_code)]

use rge_kernel_graph_foundation::{
    EdgeId, EdgeView, Graph, GraphError, NodeId, NodeView, VizAdapter,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error returned by mutating operations on a [`ScriptGraph`].
///
/// A thin newtype over the substrate [`GraphError`]; the wrapped value
/// preserves the exact graph-foundation failure (duplicate node, duplicate
/// edge, or dangling endpoint) for callers that need to inspect it.
#[derive(Debug, PartialEq, Eq)]
pub struct ScriptGraphError(pub GraphError);

impl std::fmt::Display for ScriptGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "script graph error: {}", self.0)
    }
}

impl std::error::Error for ScriptGraphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<GraphError> for ScriptGraphError {
    fn from(err: GraphError) -> Self {
        Self(err)
    }
}

// ---------------------------------------------------------------------------
// Node payload
// ---------------------------------------------------------------------------

/// Data-only script node payload.
///
/// The wrapper treats the node `key` as an uninterpreted string; the
/// substrate [`NodeId`] is derived deterministically from its bytes. The
/// payload carries no script execution, code generation, evaluator,
/// traversal, editor, runtime, or script-host behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptNode {
    /// Uninterpreted, deterministic key identifying the node.
    pub key: String,
}

impl ScriptNode {
    /// Construct a script node with the given uninterpreted `key`.
    #[must_use]
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Edge payload
// ---------------------------------------------------------------------------

/// Data-only script edge payload.
///
/// The wrapper treats the edge `key` as an uninterpreted string; it
/// participates in the edge's content-derived [`EdgeId`], so two edges
/// between the same nodes that use different keys are distinct edges. The
/// payload carries no conditions, ports, expressions, or runtime behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptEdge {
    /// Uninterpreted, deterministic key identifying the edge.
    pub key: String,
}

impl ScriptEdge {
    /// Construct a script edge with the given uninterpreted `key`.
    #[must_use]
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// ScriptGraph
// ---------------------------------------------------------------------------

/// Minimal script graph: data-only nodes connected by data-only edges,
/// backed by `rge_kernel_graph_foundation::Graph`.
#[derive(Debug, Clone)]
pub struct ScriptGraph {
    graph: Graph<ScriptNode, ScriptEdge>,
}

impl ScriptGraph {
    /// Construct an empty script graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
        }
    }

    /// Add a script node identified by `key`.
    ///
    /// The returned [`NodeId`] is derived deterministically from the key, so
    /// the same key yields the same id in any [`ScriptGraph`] instance.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptGraphError`] wrapping [`GraphError::DuplicateNode`]
    /// when a node with the same key (hence the same [`NodeId`]) is already
    /// present in this graph.
    pub fn add_node(&mut self, key: &str) -> Result<NodeId, ScriptGraphError> {
        let id = NodeId::from_bytes(key.as_bytes());
        self.graph.insert_node(id, ScriptNode::new(key))?;
        Ok(id)
    }

    /// Connect two existing nodes with the data-only payload `edge`.
    ///
    /// The returned [`EdgeId`] is derived deterministically from the endpoint
    /// ids together with the edge key.
    ///
    /// # Errors
    ///
    /// Returns [`ScriptGraphError`] wrapping:
    /// - [`GraphError::DuplicateEdge`] when an identical edge (same endpoints
    ///   and same key) already exists; or
    /// - [`GraphError::DanglingEndpoint`] when `src` or `dst` is not currently
    ///   a node in this graph.
    pub fn connect(
        &mut self,
        src: NodeId,
        dst: NodeId,
        edge: ScriptEdge,
    ) -> Result<EdgeId, ScriptGraphError> {
        let id = script_edge_id(src, dst, &edge);
        self.graph.insert_edge(id, src, dst, edge)?;
        Ok(id)
    }

    /// Returns the number of nodes currently in the graph.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Returns the number of edges currently in the graph.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

impl Default for ScriptGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Derive the content-stable [`EdgeId`] for a script edge from its endpoints
/// and key, so identical edges collide (duplicate detection) while edges
/// that differ only in key stay distinct.
fn script_edge_id(src: NodeId, dst: NodeId, edge: &ScriptEdge) -> EdgeId {
    let mut bytes = Vec::with_capacity(32 + edge.key.len());
    bytes.extend_from_slice(&src.0.to_le_bytes());
    bytes.extend_from_slice(&dst.0.to_le_bytes());
    bytes.extend_from_slice(edge.key.as_bytes());
    EdgeId::from_bytes(&bytes)
}

// ---------------------------------------------------------------------------
// Graph-viewer adapter
// ---------------------------------------------------------------------------

/// Exposes the script graph structure to editor graph-viewer widgets.
///
/// This is a read-only view surface only: it adds no script execution,
/// evaluator, interpreter, traversal runtime, editor behavior, ECS, runtime
/// scheduling, or script-host integration. Counts delegate straight to the
/// substrate counters, and node/edge views borrow the existing substrate
/// records — no duplicate structural state is introduced.
impl VizAdapter for ScriptGraph {
    fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = NodeView<'_>> + '_> {
        Box::new(self.graph.nodes().map(|(id, node)| NodeView {
            id,
            display_name: node.key.as_str(),
            kind: "ScriptNode",
        }))
    }

    fn edges(&self) -> Box<dyn Iterator<Item = EdgeView<'_>> + '_> {
        Box::new(self.graph.edges().map(|(id, record)| EdgeView {
            id,
            src: record.src,
            dst: record.dst,
            label: record.data.key.as_str(),
        }))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_kernel_graph_foundation::{EdgeRecord, GraphDiff, GraphError, GraphSnapshot};

    use super::*;

    #[test]
    fn node_ids_are_stable_across_graphs() {
        let mut a = ScriptGraph::new();
        let mut b = ScriptGraph::new();
        let id_a = a.add_node("entry").unwrap();
        let id_b = b.add_node("entry").unwrap();
        assert_eq!(
            id_a, id_b,
            "the same key in two fresh graphs must yield the same NodeId"
        );
    }

    #[test]
    fn duplicate_node_is_rejected() {
        let mut g = ScriptGraph::new();
        g.add_node("entry").unwrap();
        let err = g
            .add_node("entry")
            .expect_err("re-adding the same node key must fail");
        assert!(
            matches!(err.0, GraphError::DuplicateNode(_)),
            "expected DuplicateNode, got {err:?}"
        );
    }

    #[test]
    fn connect_succeeds_and_updates_counts() {
        let mut g = ScriptGraph::new();
        let a = g.add_node("entry").unwrap();
        let b = g.add_node("exit").unwrap();
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 0);

        g.connect(a, b, ScriptEdge::new("flow")).unwrap();

        assert_eq!(
            g.edge_count(),
            1,
            "a successful connect increments edge count"
        );
        assert_eq!(g.node_count(), 2, "connect must preserve node count");
    }

    #[test]
    fn duplicate_edge_is_rejected() {
        let mut g = ScriptGraph::new();
        let a = g.add_node("entry").unwrap();
        let b = g.add_node("exit").unwrap();
        g.connect(a, b, ScriptEdge::new("flow")).unwrap();

        let err = g
            .connect(a, b, ScriptEdge::new("flow"))
            .expect_err("re-adding an identical edge must fail");
        assert!(
            matches!(err.0, GraphError::DuplicateEdge(_)),
            "expected DuplicateEdge, got {err:?}"
        );
        assert_eq!(g.edge_count(), 1, "rejected connect must not add an edge");
    }

    #[test]
    fn differing_edge_keys_are_not_duplicates() {
        let mut g = ScriptGraph::new();
        let a = g.add_node("entry").unwrap();
        let b = g.add_node("exit").unwrap();
        g.connect(a, b, ScriptEdge::new("flow")).unwrap();
        g.connect(a, b, ScriptEdge::new("error")).unwrap();
        assert_eq!(
            g.edge_count(),
            2,
            "same endpoints with different edge keys are distinct edges"
        );
    }

    #[test]
    fn dangling_endpoint_is_rejected() {
        let mut g = ScriptGraph::new();
        let a = g.add_node("entry").unwrap();
        let ghost = NodeId::from_bytes(b"never-added");

        let err = g
            .connect(a, ghost, ScriptEdge::new("flow"))
            .expect_err("connecting to an absent node must fail");
        assert!(
            matches!(err.0, GraphError::DanglingEndpoint { .. }),
            "expected DanglingEndpoint, got {err:?}"
        );
        assert_eq!(g.edge_count(), 0, "rejected connect must not add an edge");
    }

    #[test]
    fn empty_graph_has_zero_counts() {
        let g = ScriptGraph::default();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn viz_adapter_counts_observed_through_trait() {
        let mut g = ScriptGraph::new();
        let entry = g.add_node("entry").unwrap();
        let body = g.add_node("body").unwrap();
        let exit = g.add_node("exit").unwrap();
        g.connect(entry, body, ScriptEdge::new("flow")).unwrap();
        g.connect(body, exit, ScriptEdge::new("done")).unwrap();

        let adapter: &dyn VizAdapter = &g;
        assert_eq!(
            adapter.node_count(),
            3,
            "VizAdapter node_count must delegate to the substrate count"
        );
        assert_eq!(
            adapter.edge_count(),
            2,
            "VizAdapter edge_count must delegate to the substrate count"
        );
        assert_eq!(
            adapter.nodes().count(),
            adapter.node_count(),
            "node view iteration yields exactly node_count items"
        );
        assert_eq!(
            adapter.edges().count(),
            adapter.edge_count(),
            "edge view iteration yields exactly edge_count items"
        );
    }

    #[test]
    fn viz_adapter_node_views_match_substrate_order() {
        let mut g = ScriptGraph::new();
        let entry = g.add_node("entry").unwrap();
        let body = g.add_node("body").unwrap();
        let exit = g.add_node("exit").unwrap();

        // Expected order is the deterministic substrate (BTreeMap) order,
        // i.e. sorted by NodeId — not the insertion order above.
        let mut expected: Vec<(NodeId, &str)> =
            vec![(entry, "entry"), (body, "body"), (exit, "exit")];
        expected.sort_by_key(|&(id, _)| id);

        let adapter: &dyn VizAdapter = &g;
        let views: Vec<(NodeId, String, String)> = adapter
            .nodes()
            .map(|n| (n.id, n.display_name.to_owned(), n.kind.to_owned()))
            .collect();

        assert_eq!(views.len(), 3, "one node view per script node");
        for (view, &(exp_id, exp_name)) in views.iter().zip(expected.iter()) {
            assert_eq!(view.0, exp_id, "node view id is the substrate NodeId");
            assert_eq!(
                view.1, exp_name,
                "node view display_name is the script node key"
            );
            assert_eq!(
                view.2, "ScriptNode",
                "every script node view has the static kind string"
            );
        }
    }

    #[test]
    fn viz_adapter_edge_views_match_substrate_order() {
        let mut g = ScriptGraph::new();
        let entry = g.add_node("entry").unwrap();
        let body = g.add_node("body").unwrap();
        let exit = g.add_node("exit").unwrap();

        let e1 = g.connect(entry, body, ScriptEdge::new("flow")).unwrap();
        let e2 = g.connect(body, exit, ScriptEdge::new("done")).unwrap();

        // Expected order is the deterministic substrate (BTreeMap) order,
        // i.e. sorted by EdgeId — not the insertion order above.
        let mut expected: Vec<(EdgeId, NodeId, NodeId, &str)> =
            vec![(e1, entry, body, "flow"), (e2, body, exit, "done")];
        expected.sort_by_key(|&(id, ..)| id);

        let adapter: &dyn VizAdapter = &g;
        let views: Vec<(EdgeId, NodeId, NodeId, String)> = adapter
            .edges()
            .map(|e| (e.id, e.src, e.dst, e.label.to_owned()))
            .collect();

        assert_eq!(views.len(), 2, "one edge view per script edge");
        for (view, &(exp_id, exp_src, exp_dst, exp_label)) in views.iter().zip(expected.iter()) {
            assert_eq!(view.0, exp_id, "edge view id is the substrate EdgeId");
            assert_eq!(view.1, exp_src, "edge view src is the record source");
            assert_eq!(view.2, exp_dst, "edge view dst is the record destination");
            assert_eq!(
                view.3, exp_label,
                "edge view label is the script edge key string"
            );
        }
    }

    #[test]
    fn snapshot_ron_round_trip_preserves_script_payloads() {
        // Build a populated script graph: three nodes joined by two edges
        // carrying distinct key payloads.
        let mut g = ScriptGraph::new();
        let entry = g.add_node("entry").unwrap();
        let body = g.add_node("body").unwrap();
        let exit = g.add_node("exit").unwrap();
        let e_flow = g.connect(entry, body, ScriptEdge::new("flow")).unwrap();
        let e_exit = g.connect(body, exit, ScriptEdge::new("done")).unwrap();

        // Capture the private substrate graph and round-trip it through the
        // full path: Graph -> GraphSnapshot -> RON text -> GraphSnapshot -> Graph.
        let snapshot = GraphSnapshot::from_graph(&g.graph);
        let ron = snapshot.to_ron().expect("snapshot serializes to RON");
        let restored_snapshot: GraphSnapshot<ScriptNode, ScriptEdge> =
            GraphSnapshot::from_ron(&ron).expect("snapshot deserializes from RON");
        let restored = restored_snapshot.to_graph();

        // Structure survives the round trip.
        assert_eq!(
            restored.node_count(),
            g.graph.node_count(),
            "node count survives the snapshot RON round trip"
        );
        assert_eq!(
            restored.edge_count(),
            g.graph.edge_count(),
            "edge count survives the snapshot RON round trip"
        );

        // Node identity: every NodeId paired with its stored ScriptNode
        // payload (the uninterpreted key) is preserved, in the same
        // deterministic order.
        let original_nodes: Vec<(NodeId, ScriptNode)> =
            g.graph.nodes().map(|(id, n)| (id, n.clone())).collect();
        let restored_nodes: Vec<(NodeId, ScriptNode)> =
            restored.nodes().map(|(id, n)| (id, n.clone())).collect();
        assert_eq!(
            restored_nodes, original_nodes,
            "every NodeId and ScriptNode payload is restored unchanged"
        );

        // Edge identity: every EdgeId paired with its full record — source
        // node, destination node, and ScriptEdge key payload — is preserved,
        // in the same deterministic order.
        let original_edges: Vec<(EdgeId, EdgeRecord<ScriptEdge>)> =
            g.graph.edges().map(|(id, r)| (id, r.clone())).collect();
        let restored_edges: Vec<(EdgeId, EdgeRecord<ScriptEdge>)> =
            restored.edges().map(|(id, r)| (id, r.clone())).collect();
        assert_eq!(
            restored_edges, original_edges,
            "every EdgeId, endpoint pair, and ScriptEdge payload is restored unchanged"
        );

        // Spot-check the concrete edges built above, addressed by the EdgeId
        // returned at construction time.
        let restored_flow = restored
            .edge(e_flow)
            .expect("the entry->body edge is present after restore");
        assert_eq!(restored_flow.src, entry, "flow edge source restored");
        assert_eq!(restored_flow.dst, body, "flow edge destination restored");
        assert_eq!(
            restored_flow.data.key, "flow",
            "flow edge key payload restored"
        );
        let restored_done = restored
            .edge(e_exit)
            .expect("the body->exit edge is present after restore");
        assert_eq!(restored_done.src, body, "done edge source restored");
        assert_eq!(restored_done.dst, exit, "done edge destination restored");
        assert_eq!(
            restored_done.data.key, "done",
            "done edge key payload restored"
        );
    }

    #[test]
    fn structural_diff_reports_added_script_node_and_edge() {
        // Build an initial script graph: two nodes joined by one script edge,
        // then snapshot it as the "old" state.
        let mut g = ScriptGraph::new();
        let entry = g.add_node("entry").unwrap();
        let body = g.add_node("body").unwrap();
        g.connect(entry, body, ScriptEdge::new("flow")).unwrap();
        let old_snapshot = GraphSnapshot::from_graph(&g.graph);

        // Mutate the same graph: add exactly one new script node and exactly
        // one new script edge connected to that node.
        let exit = g.add_node("exit").unwrap();
        let new_edge_id = g.connect(body, exit, ScriptEdge::new("done")).unwrap();
        let new_snapshot = GraphSnapshot::from_graph(&g.graph);

        // The graph-foundation structural diff over the old → new snapshots.
        let diff = GraphDiff::between(&old_snapshot, &new_snapshot);

        // Exactly the one new node is reported as added, with its payload.
        assert_eq!(
            diff.added_nodes.len(),
            1,
            "exactly one script node was added"
        );
        assert_eq!(
            diff.added_nodes.get(&exit),
            Some(&ScriptNode {
                key: "exit".to_owned(),
            }),
            "the added node is the new 'exit' script node, by id and payload"
        );

        // Exactly the one new edge is reported as added, with its full record.
        assert_eq!(
            diff.added_edges.len(),
            1,
            "exactly one script edge was added"
        );
        assert_eq!(
            diff.added_edges.get(&new_edge_id),
            Some(&EdgeRecord {
                src: body,
                dst: exit,
                data: ScriptEdge::new("done"),
            }),
            "the added edge record carries the source, destination, and edge key payload"
        );

        // Nothing was removed and no pre-existing node or edge record changed.
        assert!(diff.removed_nodes.is_empty(), "no script node was removed");
        assert!(diff.removed_edges.is_empty(), "no script edge was removed");
        assert!(
            diff.changed_nodes.is_empty(),
            "no existing script node record changed"
        );
        assert!(
            diff.changed_edges.is_empty(),
            "no existing script edge record changed"
        );
    }
}
