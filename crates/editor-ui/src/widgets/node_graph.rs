//! Generic node-graph widget/model shell.
//!
//! ISSUE-58: the first editor-ui consumer of
//! [`rge_kernel_graph_foundation::VizAdapter`]. This module deliberately
//! contains **only** the substrate any future graph-viewer renderer will need:
//!
//! * a [`NodeGraphModel`] that snapshots node and edge views from a
//!   `&dyn VizAdapter`, copying stable ids/endpoints while keeping
//!   display labels as borrowed string slices tied to the adapter borrow;
//! * a [`NodeGraphWidget`] shell that builds such a model on demand.
//!
//! The module is intentionally domain-agnostic — it knows nothing about
//! materials, animation, CAD, scripts, operators, renderers, runtimes, or
//! layout. Adapter iteration order is preserved verbatim; no sorting,
//! traversal, selection, or graph-evaluation semantics are added.

use rge_kernel_graph_foundation::{EdgeId, EdgeView, NodeId, NodeView, VizAdapter};

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// Per-node entry collected from a [`VizAdapter`].
///
/// Stable id and node kind/display strings are taken verbatim from the
/// adapter's [`NodeView`]; the strings stay borrowed for the model's
/// lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRecord<'a> {
    /// Stable id assigned by the underlying graph domain.
    pub id: NodeId,
    /// Human-readable label borrowed from the adapter.
    pub display_name: &'a str,
    /// Domain-specific category string borrowed from the adapter.
    pub kind: &'a str,
}

/// Per-edge entry collected from a [`VizAdapter`].
///
/// Endpoints and stable id are copied; the label stays borrowed for the
/// model's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeRecord<'a> {
    /// Stable id assigned by the underlying graph domain.
    pub id: EdgeId,
    /// Source (origin) node id.
    pub src: NodeId,
    /// Destination (target) node id.
    pub dst: NodeId,
    /// Human-readable edge label borrowed from the adapter.
    pub label: &'a str,
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Snapshot of the node and edge views currently exposed by a
/// [`VizAdapter`] borrow.
///
/// The model preserves adapter iteration order, copies stable ids and
/// edge endpoints, and keeps display strings as borrowed slices tied to
/// the adapter borrow that produced it. It performs no traversal,
/// sorting, layout, selection, or graph-evaluation work.
#[derive(Debug, Clone)]
pub struct NodeGraphModel<'a> {
    nodes: Vec<NodeRecord<'a>>,
    edges: Vec<EdgeRecord<'a>>,
}

impl<'a> NodeGraphModel<'a> {
    /// Collect a model from `adapter`, preserving the adapter's iteration
    /// order for both nodes and edges.
    #[must_use]
    pub fn from_adapter(adapter: &'a dyn VizAdapter) -> Self {
        let nodes = adapter
            .nodes()
            .map(
                |NodeView {
                     id,
                     display_name,
                     kind,
                 }| NodeRecord {
                    id,
                    display_name,
                    kind,
                },
            )
            .collect();
        let edges = adapter
            .edges()
            .map(
                |EdgeView {
                     id,
                     src,
                     dst,
                     label,
                 }| EdgeRecord {
                    id,
                    src,
                    dst,
                    label,
                },
            )
            .collect();
        Self { nodes, edges }
    }

    /// Number of node records — must equal `adapter.node_count()`.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edge records — must equal `adapter.edge_count()`.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Node records in the adapter's original iteration order.
    #[must_use]
    pub fn nodes(&self) -> &[NodeRecord<'a>] {
        &self.nodes
    }

    /// Edge records in the adapter's original iteration order.
    #[must_use]
    pub fn edges(&self) -> &[EdgeRecord<'a>] {
        &self.edges
    }
}

// ---------------------------------------------------------------------------
// Widget shell
// ---------------------------------------------------------------------------

/// Lightweight node-graph widget shell.
///
/// The shell itself owns no state — a future renderer will hang pan/zoom,
/// selection, port routing, and style here. For now it only provides the
/// adapter-to-model bridge that every renderer will need.
#[derive(Debug, Default, Clone, Copy)]
pub struct NodeGraphWidget;

impl NodeGraphWidget {
    /// Construct a fresh widget shell.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Build a [`NodeGraphModel`] from the given adapter borrow.
    #[must_use]
    pub fn model_from<'a>(&self, adapter: &'a dyn VizAdapter) -> NodeGraphModel<'a> {
        NodeGraphModel::from_adapter(adapter)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic in-test fake — depends only on graph-foundation primitives
    /// (no material / animation / CAD / script coupling).
    struct FakeNode {
        id: NodeId,
        name: String,
        kind: String,
    }

    struct FakeEdge {
        id: EdgeId,
        src: NodeId,
        dst: NodeId,
        label: String,
    }

    struct FakeAdapter {
        nodes: Vec<FakeNode>,
        edges: Vec<FakeEdge>,
    }

    impl VizAdapter for FakeAdapter {
        fn node_count(&self) -> usize {
            self.nodes.len()
        }

        fn edge_count(&self) -> usize {
            self.edges.len()
        }

        fn nodes(&self) -> Box<dyn Iterator<Item = NodeView<'_>> + '_> {
            Box::new(self.nodes.iter().map(|n| NodeView {
                id: n.id,
                display_name: n.name.as_str(),
                kind: n.kind.as_str(),
            }))
        }

        fn edges(&self) -> Box<dyn Iterator<Item = EdgeView<'_>> + '_> {
            Box::new(self.edges.iter().map(|e| EdgeView {
                id: e.id,
                src: e.src,
                dst: e.dst,
                label: e.label.as_str(),
            }))
        }
    }

    fn sample_adapter() -> FakeAdapter {
        let n0 = NodeId::from_raw(0x10);
        let n1 = NodeId::from_raw(0x11);
        let n2 = NodeId::from_raw(0x12);
        FakeAdapter {
            nodes: vec![
                FakeNode {
                    id: n0,
                    name: "Alpha".to_string(),
                    kind: "Source".to_string(),
                },
                FakeNode {
                    id: n1,
                    name: "Beta".to_string(),
                    kind: "Op".to_string(),
                },
                FakeNode {
                    id: n2,
                    name: "Gamma".to_string(),
                    kind: "Sink".to_string(),
                },
            ],
            edges: vec![
                FakeEdge {
                    id: EdgeId::from_raw(0x20),
                    src: n0,
                    dst: n1,
                    label: "value".to_string(),
                },
                FakeEdge {
                    id: EdgeId::from_raw(0x21),
                    src: n1,
                    dst: n2,
                    label: "result".to_string(),
                },
            ],
        }
    }

    #[test]
    fn model_borrows_adapter_views_with_preserved_order() {
        let adapter = sample_adapter();
        let widget = NodeGraphWidget::new();
        let model = widget.model_from(&adapter);

        // Counts flow from the adapter unchanged.
        assert_eq!(model.node_count(), adapter.node_count());
        assert_eq!(model.edge_count(), adapter.edge_count());
        assert_eq!(model.node_count(), 3);
        assert_eq!(model.edge_count(), 2);

        // Node ids, display names, and kinds preserve adapter order verbatim
        // — they are not replaced by generic placeholders.
        let nodes = model.nodes();
        assert_eq!(nodes[0].id, NodeId::from_raw(0x10));
        assert_eq!(nodes[0].display_name, "Alpha");
        assert_eq!(nodes[0].kind, "Source");
        assert_eq!(nodes[1].id, NodeId::from_raw(0x11));
        assert_eq!(nodes[1].display_name, "Beta");
        assert_eq!(nodes[1].kind, "Op");
        assert_eq!(nodes[2].id, NodeId::from_raw(0x12));
        assert_eq!(nodes[2].display_name, "Gamma");
        assert_eq!(nodes[2].kind, "Sink");

        // Edge ids, endpoints, and labels also preserve adapter order verbatim.
        let edges = model.edges();
        assert_eq!(edges[0].id, EdgeId::from_raw(0x20));
        assert_eq!(edges[0].src, NodeId::from_raw(0x10));
        assert_eq!(edges[0].dst, NodeId::from_raw(0x11));
        assert_eq!(edges[0].label, "value");
        assert_eq!(edges[1].id, EdgeId::from_raw(0x21));
        assert_eq!(edges[1].src, NodeId::from_raw(0x11));
        assert_eq!(edges[1].dst, NodeId::from_raw(0x12));
        assert_eq!(edges[1].label, "result");

        // Borrowed-label proof: record string slices must point inside the
        // adapter's own string storage, not into copied buffers.
        let adapter_name_ptr = adapter.nodes[1].name.as_ptr();
        let model_name_ptr = nodes[1].display_name.as_ptr();
        assert!(
            std::ptr::eq(adapter_name_ptr, model_name_ptr),
            "display_name must remain a borrow from the adapter, not an owned copy",
        );
        let adapter_label_ptr = adapter.edges[0].label.as_ptr();
        let model_label_ptr = edges[0].label.as_ptr();
        assert!(
            std::ptr::eq(adapter_label_ptr, model_label_ptr),
            "edge label must remain a borrow from the adapter, not an owned copy",
        );
    }

    #[test]
    fn empty_adapter_yields_empty_model() {
        let adapter = FakeAdapter {
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        let model = NodeGraphModel::from_adapter(&adapter);
        assert_eq!(model.node_count(), 0);
        assert_eq!(model.edge_count(), 0);
        assert!(model.nodes().is_empty());
        assert!(model.edges().is_empty());
    }
}
