//! `rge-material-graph` — material graph foundation wrapper.
//!
//! Failure class: snapshot-recoverable
//!
//! Phase 8 foundation slice. [`MaterialGraph`] is a thin wrapper over
//! `rge_kernel_graph_foundation::Graph` that stores opaque material nodes
//! keyed by content-derived [`NodeId`] and connects them with typed-port
//! [`MaterialEdge`] payloads. Like `cad-core`'s operator-graph wrapper, the
//! material graph is rebuildable structural state that participates in
//! snapshot/restore — a rejected mutation is recovered by restoring the last
//! good snapshot rather than terminating the session.
//!
//! This crate is the foundation layer only: it carries no WGSL generation,
//! runtime evaluation, editor behavior, traversal, cycle detection, or gfx
//! integration. The [`PortType`] surface is a data-only tag with no shader,
//! evaluator, or renderer semantics.

use rge_kernel_graph_foundation::{
    EdgeId, EdgeView, Graph, GraphError, NodeId, NodeView, VizAdapter,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error returned by mutating operations on a [`MaterialGraph`].
///
/// A thin newtype over the substrate [`GraphError`]; the wrapped value
/// preserves the exact graph-foundation failure (duplicate node, duplicate
/// edge, or dangling endpoint) for callers that need to inspect it.
#[derive(Debug, PartialEq, Eq)]
pub struct MaterialGraphError(pub GraphError);

impl std::fmt::Display for MaterialGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "material graph error: {}", self.0)
    }
}

impl std::error::Error for MaterialGraphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<GraphError> for MaterialGraphError {
    fn from(err: GraphError) -> Self {
        Self(err)
    }
}

// ---------------------------------------------------------------------------
// Typed ports
// ---------------------------------------------------------------------------

/// Data-only tag identifying the type carried by a material connection port.
///
/// This is a minimal classification used only to record what a connection
/// transports; it carries no shader, evaluator, editor, or renderer
/// semantics, and the wrapper performs no type-compatibility validation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortType {
    /// A single scalar channel.
    Scalar = 0,
    /// A multi-component vector channel.
    Vector = 1,
    /// A color channel.
    Color = 2,
    /// A texture-sample channel.
    Texture = 3,
}

/// Payload stored on a material connection: the typed source and destination
/// ports it joins.
///
/// Data-only. The pair `(src_port, dst_port)` participates in the connection's
/// content-derived [`EdgeId`], so two connections between the same nodes that
/// use different port types are distinct edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MaterialEdge {
    /// Typed port on the source node that the connection leaves from.
    pub src_port: PortType,
    /// Typed port on the destination node that the connection arrives at.
    pub dst_port: PortType,
}

// ---------------------------------------------------------------------------
// Node payload
// ---------------------------------------------------------------------------

/// Opaque material node payload.
///
/// The wrapper treats the node `key` as an uninterpreted string; the substrate
/// [`NodeId`] is derived deterministically from its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MaterialNode {
    key: String,
}

// ---------------------------------------------------------------------------
// MaterialGraph
// ---------------------------------------------------------------------------

/// Minimal material graph: opaque nodes connected by typed-port edges,
/// backed by `rge_kernel_graph_foundation::Graph`.
#[derive(Debug, Clone)]
pub struct MaterialGraph {
    graph: Graph<MaterialNode, MaterialEdge>,
}

impl MaterialGraph {
    /// Construct an empty material graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
        }
    }

    /// Add an opaque material node identified by `key`.
    ///
    /// The returned [`NodeId`] is derived deterministically from the key, so
    /// the same key yields the same id in any [`MaterialGraph`] instance.
    ///
    /// # Errors
    ///
    /// Returns [`MaterialGraphError`] wrapping [`GraphError::DuplicateNode`]
    /// when a node with the same key (hence the same [`NodeId`]) is already
    /// present in this graph.
    pub fn add_node(&mut self, key: &str) -> Result<NodeId, MaterialGraphError> {
        let id = NodeId::from_bytes(key.as_bytes());
        self.graph.insert_node(
            id,
            MaterialNode {
                key: key.to_owned(),
            },
        )?;
        Ok(id)
    }

    /// Connect two existing nodes with the typed-port payload `edge`.
    ///
    /// The returned [`EdgeId`] is derived deterministically from the endpoint
    /// ids together with both port types.
    ///
    /// # Errors
    ///
    /// Returns [`MaterialGraphError`] wrapping:
    /// - [`GraphError::DuplicateEdge`] when an identical connection (same
    ///   endpoints and same port types) already exists; or
    /// - [`GraphError::DanglingEndpoint`] when `src` or `dst` is not currently
    ///   a node in this graph.
    pub fn connect(
        &mut self,
        src: NodeId,
        dst: NodeId,
        edge: MaterialEdge,
    ) -> Result<EdgeId, MaterialGraphError> {
        let id = material_edge_id(src, dst, edge);
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

impl Default for MaterialGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Derive the content-stable [`EdgeId`] for a connection from its endpoints
/// and typed ports, so identical connections collide (duplicate detection)
/// while connections that differ only in port type stay distinct.
fn material_edge_id(src: NodeId, dst: NodeId, edge: MaterialEdge) -> EdgeId {
    let mut bytes = [0u8; 34];
    bytes[..16].copy_from_slice(&src.0.to_le_bytes());
    bytes[16..32].copy_from_slice(&dst.0.to_le_bytes());
    bytes[32] = edge.src_port as u8;
    bytes[33] = edge.dst_port as u8;
    EdgeId::from_bytes(&bytes)
}

// ---------------------------------------------------------------------------
// Graph-viewer adapter
// ---------------------------------------------------------------------------

/// Deterministic, non-allocating label for a material connection, formed from
/// its typed `(src_port, dst_port)` pair as a lower-case `src->dst` string.
///
/// Every result is a string literal, so the returned `&'static str` satisfies
/// the lifetime that [`EdgeView::label`] borrows without any allocation. The
/// label is derived only from the existing [`MaterialEdge`] port payload — it
/// adds no new edge state and is exhaustive over every [`PortType`] pair.
fn material_edge_label(edge: MaterialEdge) -> &'static str {
    use PortType::{Color, Scalar, Texture, Vector};
    match (edge.src_port, edge.dst_port) {
        (Scalar, Scalar) => "scalar->scalar",
        (Scalar, Vector) => "scalar->vector",
        (Scalar, Color) => "scalar->color",
        (Scalar, Texture) => "scalar->texture",
        (Vector, Scalar) => "vector->scalar",
        (Vector, Vector) => "vector->vector",
        (Vector, Color) => "vector->color",
        (Vector, Texture) => "vector->texture",
        (Color, Scalar) => "color->scalar",
        (Color, Vector) => "color->vector",
        (Color, Color) => "color->color",
        (Color, Texture) => "color->texture",
        (Texture, Scalar) => "texture->scalar",
        (Texture, Vector) => "texture->vector",
        (Texture, Color) => "texture->color",
        (Texture, Texture) => "texture->texture",
    }
}

/// Exposes the material graph structure to editor graph-viewer widgets.
///
/// This is a read-only view surface only: it adds no WGSL generation, runtime
/// evaluation, editor behavior, traversal, or gfx integration. Counts delegate
/// straight to the substrate counters, and node/edge views borrow the existing
/// substrate records — no duplicate structural state is introduced.
impl VizAdapter for MaterialGraph {
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
            kind: "MaterialNode",
        }))
    }

    fn edges(&self) -> Box<dyn Iterator<Item = EdgeView<'_>> + '_> {
        Box::new(self.graph.edges().map(|(id, record)| EdgeView {
            id,
            src: record.src,
            dst: record.dst,
            label: material_edge_label(record.data),
        }))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_kernel_graph_foundation::{EdgeRecord, GraphError, GraphSnapshot};

    use super::*;

    fn edge(src_port: PortType, dst_port: PortType) -> MaterialEdge {
        MaterialEdge { src_port, dst_port }
    }

    #[test]
    fn node_ids_are_stable_across_graphs() {
        let mut a = MaterialGraph::new();
        let mut b = MaterialGraph::new();
        let id_a = a.add_node("albedo").unwrap();
        let id_b = b.add_node("albedo").unwrap();
        assert_eq!(
            id_a, id_b,
            "the same key in two fresh graphs must yield the same NodeId"
        );
    }

    #[test]
    fn distinct_keys_get_distinct_ids() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        assert_ne!(a, b, "distinct keys must yield distinct NodeIds");
    }

    #[test]
    fn connect_succeeds_and_updates_counts() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 0);

        g.connect(a, b, edge(PortType::Color, PortType::Color))
            .unwrap();

        assert_eq!(
            g.edge_count(),
            1,
            "a successful connect increments edge count"
        );
        assert_eq!(g.node_count(), 2, "connect must preserve node count");
    }

    #[test]
    fn duplicate_node_is_rejected() {
        let mut g = MaterialGraph::new();
        g.add_node("a").unwrap();
        let err = g
            .add_node("a")
            .expect_err("re-adding the same node key must fail");
        assert!(
            matches!(err.0, GraphError::DuplicateNode(_)),
            "expected DuplicateNode, got {err:?}"
        );
    }

    #[test]
    fn duplicate_edge_is_rejected() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        let e = edge(PortType::Scalar, PortType::Scalar);
        g.connect(a, b, e).unwrap();

        let err = g
            .connect(a, b, e)
            .expect_err("re-adding an identical connection must fail");
        assert!(
            matches!(err.0, GraphError::DuplicateEdge(_)),
            "expected DuplicateEdge, got {err:?}"
        );
        assert_eq!(g.edge_count(), 1, "rejected connect must not add an edge");
    }

    #[test]
    fn differing_ports_are_not_duplicates() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        g.connect(a, b, edge(PortType::Scalar, PortType::Scalar))
            .unwrap();
        g.connect(a, b, edge(PortType::Color, PortType::Texture))
            .unwrap();
        assert_eq!(
            g.edge_count(),
            2,
            "same endpoints with different port types are distinct edges"
        );
    }

    #[test]
    fn dangling_endpoint_is_rejected() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let ghost = NodeId::from_bytes(b"never-added");

        let err = g
            .connect(a, ghost, edge(PortType::Color, PortType::Color))
            .expect_err("connecting to an absent node must fail");
        assert!(
            matches!(err.0, GraphError::DanglingEndpoint { .. }),
            "expected DanglingEndpoint, got {err:?}"
        );
        assert_eq!(g.edge_count(), 0, "rejected connect must not add an edge");
    }

    #[test]
    fn empty_graph_has_zero_counts() {
        let g = MaterialGraph::default();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn viz_adapter_counts_observed_through_trait() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        let c = g.add_node("c").unwrap();
        g.connect(a, b, edge(PortType::Scalar, PortType::Color))
            .unwrap();
        g.connect(b, c, edge(PortType::Texture, PortType::Vector))
            .unwrap();

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
        let mut g = MaterialGraph::new();
        let albedo = g.add_node("albedo").unwrap();
        let normal = g.add_node("normal").unwrap();
        let roughness = g.add_node("roughness").unwrap();

        // Expected order is the deterministic substrate (BTreeMap) order,
        // i.e. sorted by NodeId — not the insertion order above.
        let mut expected: Vec<(NodeId, &str)> = vec![
            (albedo, "albedo"),
            (normal, "normal"),
            (roughness, "roughness"),
        ];
        expected.sort_by_key(|&(id, _)| id);

        let adapter: &dyn VizAdapter = &g;
        let views: Vec<(NodeId, String, String)> = adapter
            .nodes()
            .map(|n| (n.id, n.display_name.to_owned(), n.kind.to_owned()))
            .collect();

        assert_eq!(views.len(), 3, "one node view per material node");
        for (view, &(exp_id, exp_name)) in views.iter().zip(expected.iter()) {
            assert_eq!(view.0, exp_id, "node view id is the substrate NodeId");
            assert_eq!(
                view.1, exp_name,
                "node view display_name is the material node key"
            );
            assert_eq!(
                view.2, "MaterialNode",
                "every material node view has the static kind string"
            );
        }
    }

    #[test]
    fn snapshot_ron_round_trip_preserves_material_payloads() {
        // Build a populated material graph: three material nodes joined by
        // two typed-port connections (distinct port-type pairs).
        let mut g = MaterialGraph::new();
        let albedo = g.add_node("albedo").unwrap();
        let normal = g.add_node("normal").unwrap();
        let output = g.add_node("output").unwrap();
        let e_albedo = g
            .connect(albedo, output, edge(PortType::Color, PortType::Color))
            .unwrap();
        let e_normal = g
            .connect(normal, output, edge(PortType::Vector, PortType::Texture))
            .unwrap();

        // Capture the private substrate graph and round-trip it through the
        // full path: Graph -> GraphSnapshot -> RON text -> GraphSnapshot -> Graph.
        let snapshot = GraphSnapshot::from_graph(&g.graph);
        let ron = snapshot.to_ron().expect("snapshot serializes to RON");
        let restored_snapshot: GraphSnapshot<MaterialNode, MaterialEdge> =
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

        // Node identity: every NodeId paired with its stored material node
        // payload (the opaque key) is preserved, in the same deterministic
        // order.
        let original_nodes: Vec<(NodeId, MaterialNode)> =
            g.graph.nodes().map(|(id, n)| (id, n.clone())).collect();
        let restored_nodes: Vec<(NodeId, MaterialNode)> =
            restored.nodes().map(|(id, n)| (id, n.clone())).collect();
        assert_eq!(
            restored_nodes, original_nodes,
            "every NodeId and material node payload is restored unchanged"
        );

        // Edge identity: every EdgeId paired with its full record — source
        // node, destination node, and MaterialEdge port payload — is
        // preserved, in the same deterministic order.
        let original_edges: Vec<(EdgeId, EdgeRecord<MaterialEdge>)> =
            g.graph.edges().map(|(id, r)| (id, r.clone())).collect();
        let restored_edges: Vec<(EdgeId, EdgeRecord<MaterialEdge>)> =
            restored.edges().map(|(id, r)| (id, r.clone())).collect();
        assert_eq!(
            restored_edges, original_edges,
            "every EdgeId, endpoint pair, and MaterialEdge payload is restored unchanged"
        );

        // Spot-check the concrete connections built above, addressed by the
        // EdgeId returned at construction time.
        let restored_albedo = restored
            .edge(e_albedo)
            .expect("the albedo->output edge is present after restore");
        assert_eq!(restored_albedo.src, albedo, "albedo edge source restored");
        assert_eq!(
            restored_albedo.dst, output,
            "albedo edge destination restored"
        );
        assert_eq!(
            restored_albedo.data,
            edge(PortType::Color, PortType::Color),
            "albedo edge typed-port payload restored"
        );
        let restored_normal = restored
            .edge(e_normal)
            .expect("the normal->output edge is present after restore");
        assert_eq!(restored_normal.src, normal, "normal edge source restored");
        assert_eq!(
            restored_normal.dst, output,
            "normal edge destination restored"
        );
        assert_eq!(
            restored_normal.data,
            edge(PortType::Vector, PortType::Texture),
            "normal edge typed-port payload restored"
        );
    }

    #[test]
    fn viz_adapter_edge_views_match_substrate_order() {
        let mut g = MaterialGraph::new();
        let a = g.add_node("a").unwrap();
        let b = g.add_node("b").unwrap();
        let c = g.add_node("c").unwrap();

        let e1 = g
            .connect(a, b, edge(PortType::Scalar, PortType::Color))
            .unwrap();
        let e2 = g
            .connect(b, c, edge(PortType::Texture, PortType::Vector))
            .unwrap();

        // Expected order is the deterministic substrate (BTreeMap) order,
        // i.e. sorted by EdgeId — not the insertion order above.
        let mut expected: Vec<(EdgeId, NodeId, NodeId, &str)> =
            vec![(e1, a, b, "scalar->color"), (e2, b, c, "texture->vector")];
        expected.sort_by_key(|&(id, ..)| id);

        let adapter: &dyn VizAdapter = &g;
        let views: Vec<(EdgeId, NodeId, NodeId, String)> = adapter
            .edges()
            .map(|e| (e.id, e.src, e.dst, e.label.to_owned()))
            .collect();

        assert_eq!(views.len(), 2, "one edge view per connection");
        for (view, &(exp_id, exp_src, exp_dst, exp_label)) in views.iter().zip(expected.iter()) {
            assert_eq!(view.0, exp_id, "edge view id is the substrate EdgeId");
            assert_eq!(view.1, exp_src, "edge view src is the record source");
            assert_eq!(view.2, exp_dst, "edge view dst is the record destination");
            assert_eq!(
                view.3, exp_label,
                "edge view label is the deterministic port-pair string"
            );
        }
    }
}
