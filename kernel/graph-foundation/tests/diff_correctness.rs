//! Integration test: [`GraphDiff`] correctness.
//!
//! snap1 = 3-node 2-edge graph
//! mutate to 4-node 3-edge (add 1 node + 1 edge, change 1 node payload)
//! snap2 = snapshot of mutated
//! `GraphDiff::between(&snap1, &snap2)` must report:
//!   `added_nodes` 1, `added_edges` 1, `changed_nodes` 1,
//!   `removed_nodes` 0, `removed_edges` 0, `changed_edges` 0

use rge_kernel_graph_foundation::{EdgeId, Graph, GraphDiff, GraphSnapshot, NodeId};

fn n(v: u128) -> NodeId {
    NodeId::from_raw(v)
}
fn e(v: u128) -> EdgeId {
    EdgeId::from_raw(v)
}

#[test]
fn diff_add_node_add_edge_change_node() {
    // --- snap1: 3 nodes, 2 edges ---
    let mut g: Graph<String, u32> = Graph::new();
    g.insert_node(n(1), "alpha".to_string()).unwrap();
    g.insert_node(n(2), "beta".to_string()).unwrap();
    g.insert_node(n(3), "gamma".to_string()).unwrap();
    g.insert_edge(e(10), n(1), n(2), 1).unwrap();
    g.insert_edge(e(11), n(2), n(3), 2).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g);

    // --- mutate ---
    g.replace_node(n(1), "alpha-modified".to_string()).unwrap(); // changed
    g.insert_node(n(4), "delta".to_string()).unwrap(); // added
    g.insert_edge(e(12), n(3), n(4), 3).unwrap(); // added edge

    let snap2 = GraphSnapshot::from_graph(&g);

    // --- diff ---
    let diff = GraphDiff::between(&snap1, &snap2);

    assert_eq!(diff.added_nodes.len(), 1, "one node added");
    assert!(diff.added_nodes.contains_key(&n(4)));

    assert_eq!(diff.removed_nodes.len(), 0, "no nodes removed");
    assert_eq!(diff.removed_edges.len(), 0, "no edges removed");

    assert_eq!(diff.changed_nodes.len(), 1, "one node changed");
    assert!(diff.changed_nodes.contains_key(&n(1)));
    let (old, new) = &diff.changed_nodes[&n(1)];
    assert_eq!(old, "alpha");
    assert_eq!(new, "alpha-modified");

    assert_eq!(diff.added_edges.len(), 1, "one edge added");
    assert!(diff.added_edges.contains_key(&e(12)));

    assert_eq!(diff.changed_edges.len(), 0, "no edges changed");

    // Derived counts
    assert_eq!(diff.node_change_count(), 2, "added(1) + changed(1) = 2");
    assert_eq!(diff.edge_change_count(), 1, "added(1) = 1");
}

#[test]
fn diff_empty_to_empty_is_empty() {
    let g1: Graph<String, u32> = Graph::new();
    let g2: Graph<String, u32> = Graph::new();
    let diff = GraphDiff::between(
        &GraphSnapshot::from_graph(&g1),
        &GraphSnapshot::from_graph(&g2),
    );
    assert!(diff.is_empty());
}

#[test]
fn diff_remove_node_cascades_edge_removal() {
    let mut g1: Graph<String, u32> = Graph::new();
    g1.insert_node(n(1), "a".to_string()).unwrap();
    g1.insert_node(n(2), "b".to_string()).unwrap();
    g1.insert_edge(e(10), n(1), n(2), 0).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g1);

    let mut g2 = g1.clone();
    g2.remove_node(n(1)).unwrap(); // also removes e(10)
    let snap2 = GraphSnapshot::from_graph(&g2);

    let diff = GraphDiff::between(&snap1, &snap2);
    assert_eq!(diff.removed_nodes.len(), 1);
    assert_eq!(diff.removed_edges.len(), 1);
}

#[test]
fn diff_remove_edge_only_reports_edge_removal() {
    let mut g: Graph<String, u32> = Graph::new();
    g.insert_node(n(1), "a".to_string()).unwrap();
    g.insert_node(n(2), "b".to_string()).unwrap();
    g.insert_edge(e(10), n(1), n(2), 7).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g);

    g.remove_edge(e(10)).unwrap();
    let snap2 = GraphSnapshot::from_graph(&g);

    let diff = GraphDiff::between(&snap1, &snap2);

    assert_eq!(diff.removed_edges.len(), 1);
    assert!(diff.removed_edges.contains_key(&e(10)));

    assert_eq!(diff.added_edges.len(), 0);
    assert_eq!(diff.changed_edges.len(), 0);
    assert_eq!(diff.added_nodes.len(), 0);
    assert_eq!(diff.removed_nodes.len(), 0);
    assert_eq!(diff.changed_nodes.len(), 0);

    assert_eq!(diff.node_change_count(), 0);
    assert_eq!(diff.edge_change_count(), 1);
}

#[test]
fn diff_node_mut_payload_reports_node_change_only() {
    let mut g: Graph<String, u32> = Graph::new();
    g.insert_node(n(1), "alpha".to_string()).unwrap();
    g.insert_node(n(2), "beta".to_string()).unwrap();
    g.insert_node(n(3), "gamma".to_string()).unwrap();
    g.insert_edge(e(10), n(1), n(2), 1).unwrap();
    g.insert_edge(e(11), n(2), n(3), 2).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g);

    *g.node_mut(n(2)).unwrap() = "beta-mutated".to_string();
    let snap2 = GraphSnapshot::from_graph(&g);

    let diff = GraphDiff::between(&snap1, &snap2);

    assert_eq!(diff.changed_nodes.len(), 1);
    assert!(diff.changed_nodes.contains_key(&n(2)));
    let (old, new) = &diff.changed_nodes[&n(2)];
    assert_eq!(old, "beta");
    assert_eq!(new, "beta-mutated");

    assert_eq!(diff.added_nodes.len(), 0);
    assert_eq!(diff.removed_nodes.len(), 0);
    assert_eq!(diff.added_edges.len(), 0);
    assert_eq!(diff.removed_edges.len(), 0);
    assert_eq!(diff.changed_edges.len(), 0);

    assert_eq!(diff.node_change_count(), 1);
    assert_eq!(diff.edge_change_count(), 0);
}

#[test]
fn diff_edge_mut_payload_reports_edge_change_only() {
    let mut g: Graph<String, u32> = Graph::new();
    g.insert_node(n(1), "a".to_string()).unwrap();
    g.insert_node(n(2), "b".to_string()).unwrap();
    g.insert_edge(e(10), n(1), n(2), 1).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g);

    g.edge_mut(e(10)).unwrap().data = 42;
    let snap2 = GraphSnapshot::from_graph(&g);

    let diff = GraphDiff::between(&snap1, &snap2);

    assert_eq!(diff.changed_edges.len(), 1);
    assert!(diff.changed_edges.contains_key(&e(10)));
    let (old, new) = &diff.changed_edges[&e(10)];
    assert_eq!(old.src, n(1));
    assert_eq!(old.dst, n(2));
    assert_eq!(old.data, 1);
    assert_eq!(new.src, n(1));
    assert_eq!(new.dst, n(2));
    assert_eq!(new.data, 42);

    assert_eq!(diff.removed_edges.len(), 0);
    assert_eq!(diff.added_edges.len(), 0);
    assert_eq!(diff.added_nodes.len(), 0);
    assert_eq!(diff.removed_nodes.len(), 0);
    assert_eq!(diff.changed_nodes.len(), 0);

    assert_eq!(diff.node_change_count(), 0);
    assert_eq!(diff.edge_change_count(), 1);
}

#[test]
fn diff_removed_edge_preserves_old_edge_record_payload() {
    // GitHub issue #80: a removed edge must surface in `removed_edges` as the
    // full old `EdgeRecord` (src, dst, data preserved), with no added/changed
    // edge or node noise.
    let mut g: Graph<String, u32> = Graph::new();
    g.insert_node(n(1), "a".to_string()).unwrap();
    g.insert_node(n(2), "b".to_string()).unwrap();
    g.insert_edge(e(10), n(1), n(2), 99).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g);

    g.remove_edge(e(10)).unwrap();
    let snap2 = GraphSnapshot::from_graph(&g);

    let diff = GraphDiff::between(&snap1, &snap2);

    assert_eq!(diff.removed_edges.len(), 1);
    assert!(diff.removed_edges.contains_key(&e(10)));
    let old = &diff.removed_edges[&e(10)];
    assert_eq!(old.src, n(1));
    assert_eq!(old.dst, n(2));
    assert_eq!(old.data, 99);

    assert_eq!(diff.added_edges.len(), 0);
    assert_eq!(diff.changed_edges.len(), 0);
    assert_eq!(diff.added_nodes.len(), 0);
    assert_eq!(diff.removed_nodes.len(), 0);
    assert_eq!(diff.changed_nodes.len(), 0);

    assert_eq!(diff.node_change_count(), 0);
    assert_eq!(diff.edge_change_count(), 1);
}

#[test]
fn diff_added_edge_preserves_new_edge_record_payload() {
    // GitHub issue #81: a newly added edge must surface in `added_edges` as
    // the full new `EdgeRecord` (src, dst, data preserved), with no node,
    // removed-edge, or changed-edge noise.
    let mut g: Graph<String, u32> = Graph::new();
    g.insert_node(n(1), "a".to_string()).unwrap();
    g.insert_node(n(2), "b".to_string()).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g);

    g.insert_edge(e(10), n(1), n(2), 99).unwrap();
    let snap2 = GraphSnapshot::from_graph(&g);

    let diff = GraphDiff::between(&snap1, &snap2);

    assert_eq!(diff.added_edges.len(), 1);
    assert!(diff.added_edges.contains_key(&e(10)));
    let new = &diff.added_edges[&e(10)];
    assert_eq!(new.src, n(1));
    assert_eq!(new.dst, n(2));
    assert_eq!(new.data, 99);

    assert_eq!(diff.removed_edges.len(), 0);
    assert_eq!(diff.changed_edges.len(), 0);
    assert_eq!(diff.added_nodes.len(), 0);
    assert_eq!(diff.removed_nodes.len(), 0);
    assert_eq!(diff.changed_nodes.len(), 0);

    assert_eq!(diff.node_change_count(), 0);
    assert_eq!(diff.edge_change_count(), 1);
}

#[test]
fn diff_edge_endpoint_change_same_id_reports_edge_change_only() {
    // GitHub issue #79: same `EdgeId` with a changed endpoint must surface
    // as exactly one `changed_edges` entry (old + new records preserved),
    // with no added/removed edge or node noise.
    let mut g: Graph<String, u32> = Graph::new();
    g.insert_node(n(1), "a".to_string()).unwrap();
    g.insert_node(n(2), "b".to_string()).unwrap();
    g.insert_node(n(3), "c".to_string()).unwrap();
    g.insert_edge(e(10), n(1), n(2), 7).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g);

    g.edge_mut(e(10)).unwrap().dst = n(3);
    let snap2 = GraphSnapshot::from_graph(&g);

    let diff = GraphDiff::between(&snap1, &snap2);

    assert_eq!(diff.changed_edges.len(), 1);
    let (old, new) = &diff.changed_edges[&e(10)];
    assert_eq!(old.src, n(1));
    assert_eq!(old.dst, n(2));
    assert_eq!(old.data, 7);
    assert_eq!(new.src, n(1));
    assert_eq!(new.dst, n(3));
    assert_eq!(new.data, 7);

    assert_eq!(diff.added_edges.len(), 0);
    assert_eq!(diff.removed_edges.len(), 0);
    assert_eq!(diff.added_nodes.len(), 0);
    assert_eq!(diff.removed_nodes.len(), 0);
    assert_eq!(diff.changed_nodes.len(), 0);

    assert_eq!(diff.edge_change_count(), 1);
    assert_eq!(diff.node_change_count(), 0);
}

#[test]
fn diff_removed_node_preserves_old_node_payload() {
    // GitHub issue #83: a removed isolated node must surface in `removed_nodes`
    // with its old node payload preserved, and no added-node, changed-node, or
    // edge-side diff noise.
    let mut g: Graph<String, u32> = Graph::new();
    g.insert_node(n(1), "alpha".to_string()).unwrap();
    g.insert_node(n(2), "beta".to_string()).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g);

    g.remove_node(n(2)).unwrap();
    let snap2 = GraphSnapshot::from_graph(&g);

    let diff = GraphDiff::between(&snap1, &snap2);

    assert_eq!(diff.removed_nodes.len(), 1);
    assert!(diff.removed_nodes.contains_key(&n(2)));
    let old = &diff.removed_nodes[&n(2)];
    assert_eq!(old, "beta");

    assert_eq!(diff.added_nodes.len(), 0);
    assert_eq!(diff.changed_nodes.len(), 0);
    assert_eq!(diff.added_edges.len(), 0);
    assert_eq!(diff.removed_edges.len(), 0);
    assert_eq!(diff.changed_edges.len(), 0);

    assert_eq!(diff.node_change_count(), 1);
    assert_eq!(diff.edge_change_count(), 0);
}

#[test]
fn diff_added_node_preserves_new_node_payload() {
    // GitHub issue #84: a newly added isolated node must surface in
    // `added_nodes` with its new node payload preserved, and no removed-node,
    // changed-node, or edge-side diff noise.
    let mut g: Graph<String, u32> = Graph::new();
    g.insert_node(n(1), "alpha".to_string()).unwrap();
    let snap1 = GraphSnapshot::from_graph(&g);

    g.insert_node(n(2), "beta".to_string()).unwrap();
    let snap2 = GraphSnapshot::from_graph(&g);

    let diff = GraphDiff::between(&snap1, &snap2);

    assert_eq!(diff.added_nodes.len(), 1);
    assert!(diff.added_nodes.contains_key(&n(2)));
    let new = &diff.added_nodes[&n(2)];
    assert_eq!(new, "beta");

    assert_eq!(diff.removed_nodes.len(), 0);
    assert_eq!(diff.changed_nodes.len(), 0);
    assert_eq!(diff.added_edges.len(), 0);
    assert_eq!(diff.removed_edges.len(), 0);
    assert_eq!(diff.changed_edges.len(), 0);

    assert_eq!(diff.node_change_count(), 1);
    assert_eq!(diff.edge_change_count(), 0);
}
