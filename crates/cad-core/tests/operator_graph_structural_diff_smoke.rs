//! Integration smoke test for GitHub issue #55: `OperatorGraph` snapshots
//! can be compared with graph-foundation's `GraphDiff`.
//!
//! Builds a small `Cuboid -> Transform` operator graph, captures
//! `graph.inner()` with `GraphSnapshot`, mutates the same graph by adding
//! exactly one new operator node and one new edge, snapshots again, and
//! asserts that `GraphDiff::between` reports only the newly added operator
//! node and newly added edge — by id and payload — with no removed or
//! changed nodes or edges.
//!
//! Scope is test coverage only: it exercises the existing
//! `GraphSnapshot::from_graph` and `GraphDiff::between` paths and adds no
//! domain diff API to cad-core.

use rge_cad_core::{CuboidOp, EdgeKind, OperatorGraph, OperatorNode, TransformOp};
use rge_kernel_graph_foundation::{EdgeRecord, GraphDiff, GraphSnapshot};

#[test]
fn operator_graph_structural_diff_reports_only_added_operator_and_edge() {
    // --- Build the initial operator graph: Cuboid -> Transform ----------
    let cuboid_node = OperatorNode::Cuboid(CuboidOp {
        width: 2.0,
        height: 1.0,
        depth: 1.5,
    });
    let first_transform_node = OperatorNode::Transform(TransformOp {
        translation: [3.0, -1.5, 2.0],
        ..TransformOp::default()
    });

    let mut graph = OperatorGraph::new();
    let cuboid_id = graph.add_operator(cuboid_node).expect("add cuboid");
    let first_transform_id = graph
        .add_operator(first_transform_node)
        .expect("add first transform");
    let _initial_edge_id = graph
        .connect(cuboid_id, first_transform_id, 0)
        .expect("connect cuboid -> first transform");

    assert_eq!(graph.node_count(), 2, "initial graph has two operators");
    assert_eq!(graph.edge_count(), 1, "initial graph has one edge");

    // --- Capture the old snapshot before mutation ------------------------
    let old_snapshot = GraphSnapshot::from_graph(graph.inner());
    assert_eq!(old_snapshot.node_count(), 2, "old snapshot: two nodes");
    assert_eq!(old_snapshot.edge_count(), 1, "old snapshot: one edge");

    // --- Mutate the same graph: add exactly one node and one edge -------
    // A distinct second Transform avoids the content-addressed dedup the
    // `derive_node_id` helper applies to identical payloads.
    let second_transform_payload = OperatorNode::Transform(TransformOp {
        translation: [7.0, 0.5, -2.25],
        ..TransformOp::default()
    });
    let expected_added_node = second_transform_payload.clone();
    let second_transform_id = graph
        .add_operator(second_transform_payload)
        .expect("add second transform");
    let new_edge_id = graph
        .connect(first_transform_id, second_transform_id, 0)
        .expect("connect first transform -> second transform");

    assert_eq!(
        graph.node_count(),
        3,
        "after mutation the graph has three operators"
    );
    assert_eq!(
        graph.edge_count(),
        2,
        "after mutation the graph has two edges"
    );

    // --- Capture the new snapshot after mutation -------------------------
    let new_snapshot = GraphSnapshot::from_graph(graph.inner());
    assert_eq!(new_snapshot.node_count(), 3, "new snapshot: three nodes");
    assert_eq!(new_snapshot.edge_count(), 2, "new snapshot: two edges");

    // --- Structural diff over old → new snapshots -----------------------
    let diff: GraphDiff<OperatorNode, EdgeKind> = GraphDiff::between(&old_snapshot, &new_snapshot);

    // Exactly one operator node was added — identity and payload pinned.
    assert_eq!(
        diff.added_nodes.len(),
        1,
        "exactly one operator node was added"
    );
    assert_eq!(
        diff.added_nodes.get(&second_transform_id),
        Some(&expected_added_node),
        "the added node is the new TransformOp, by NodeId and payload"
    );

    // Exactly one edge was added — full EdgeRecord triple pinned.
    assert_eq!(diff.added_edges.len(), 1, "exactly one edge was added");
    let expected_added_edge = EdgeRecord {
        src: first_transform_id,
        dst: second_transform_id,
        data: EdgeKind::Input(0),
    };
    assert_eq!(
        diff.added_edges.get(&new_edge_id),
        Some(&expected_added_edge),
        "the added edge record carries the expected src, dst, and EdgeKind::Input(0) payload"
    );

    // Nothing was removed and no pre-existing node or edge record changed.
    assert!(
        diff.removed_nodes.is_empty(),
        "no operator node was removed"
    );
    assert!(diff.removed_edges.is_empty(), "no edge was removed");
    assert!(
        diff.changed_nodes.is_empty(),
        "no existing operator node payload changed"
    );
    assert!(
        diff.changed_edges.is_empty(),
        "no existing edge record changed"
    );

    // Derived counts agree with the per-category assertions above.
    assert_eq!(
        diff.node_change_count(),
        1,
        "one node-level change in total (the single addition)"
    );
    assert_eq!(
        diff.edge_change_count(),
        1,
        "one edge-level change in total (the single addition)"
    );
    assert!(
        !diff.is_empty(),
        "the diff is non-empty: one node and one edge were added"
    );
}
