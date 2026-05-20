//! ISSUE-64 + ISSUE-65 + ISSUE-72: integration smokes proving the shared
//! `rge_kernel_graph_foundation::GraphDiff::between` path reports the
//! expected additions for the Phase 8 graph domains —
//! `rge_material_graph::MaterialGraph`, `rge_anim_graph::AnimGraph`,
//! `rge_cad_core::OperatorGraph`, and `rge_script_graph::ScriptGraph` —
//! over snapshots projected through their existing public `VizAdapter`
//! surfaces.
//!
//! ISSUE-64 proved a single added node and edge per-domain independently
//! (one diff per domain). ISSUE-65 extended that with PLAN 13.14's
//! combined-checkpoint wording: build the domains together, capture one
//! old combined checkpoint, mutate each domain by exactly one node and one
//! edge, capture one new combined checkpoint, and prove a single
//! `GraphDiff::between` call reports exactly one add per node-side and one
//! add per edge-side per domain across the union. ISSUE-72 adds the
//! `rge_script_graph::ScriptGraph` domain to both the per-domain and the
//! combined-checkpoint paths under its own deterministic namespace.
//!
//! These tests deliberately live outside `crates/editor-ui/src/**`: the
//! production editor-ui surface stays domain-agnostic and the test target
//! is the only place that names `rge-material-graph`, `rge-anim-graph`,
//! `rge-cad-core`, and `rge-script-graph`. The wrapper domains do not
//! expose their inner `Graph<N, E>` publicly to editor-ui, so the snapshot
//! helpers bridge through the public `VizAdapter` projection into a
//! test-local `Graph<DiffNode, String>` — keeping the smokes outside
//! domain-private internals while still exercising the shared
//! `GraphSnapshot` / `GraphDiff` path over the stable ids and edge records
//! each domain already emits.

use rge_anim_graph::{AnimGraph, AnimTransition};
use rge_cad_core::{CuboidOp, OperatorGraph, OperatorNode, TransformOp};
use rge_kernel_graph_foundation::{
    EdgeId, EdgeRecord, Graph, GraphDiff, GraphSnapshot, NodeId, VizAdapter,
};
use rge_material_graph::{MaterialEdge, MaterialGraph, PortType};
use rge_script_graph::{ScriptEdge, ScriptGraph};

/// Test-local node payload that captures only the projected `VizAdapter`
/// fields the diff needs to inspect: the display name and kind strings.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DiffNode {
    display_name: String,
    kind: String,
}

/// Project a domain graph's public `VizAdapter` surface into a graph-foundation
/// `GraphSnapshot<DiffNode, String>`. The snapshot preserves every adapter
/// `NodeId`, `EdgeId`, edge `src`, edge `dst`, node display name, node kind,
/// and edge label exactly as the adapter exposes them.
fn snapshot_via_adapter(adapter: &dyn VizAdapter) -> GraphSnapshot<DiffNode, String> {
    let mut graph: Graph<DiffNode, String> = Graph::new();
    for view in adapter.nodes() {
        graph
            .insert_node(
                view.id,
                DiffNode {
                    display_name: view.display_name.to_owned(),
                    kind: view.kind.to_owned(),
                },
            )
            .expect("adapter node ids are unique");
    }
    for view in adapter.edges() {
        graph
            .insert_edge(view.id, view.src, view.dst, view.label.to_owned())
            .expect("adapter edge endpoints refer to inserted nodes");
    }
    GraphSnapshot::from_graph(&graph)
}

#[test]
fn graph_diff_between_reports_one_added_node_and_edge_for_all_phase8_domains() {
    assert_material_domain();
    assert_anim_domain();
    assert_operator_domain();
    assert_script_domain();
}

fn assert_material_domain() {
    let mut graph = MaterialGraph::new();
    let albedo = graph.add_node("albedo").expect("add albedo node");
    let output = graph.add_node("output").expect("add output node");
    graph
        .connect(
            albedo,
            output,
            MaterialEdge {
                src_port: PortType::Color,
                dst_port: PortType::Color,
            },
        )
        .expect("connect albedo -> output");
    let old_snapshot = snapshot_via_adapter(&graph);

    let normal = graph.add_node("normal").expect("add normal node");
    let new_edge_id = graph
        .connect(
            normal,
            output,
            MaterialEdge {
                src_port: PortType::Vector,
                dst_port: PortType::Texture,
            },
        )
        .expect("connect normal -> output");
    let new_snapshot = snapshot_via_adapter(&graph);

    let diff = GraphDiff::between(&old_snapshot, &new_snapshot);

    assert_eq!(
        diff.added_nodes.len(),
        1,
        "exactly one material node was added"
    );
    assert_eq!(
        diff.added_nodes.get(&normal),
        Some(&DiffNode {
            display_name: "normal".to_owned(),
            kind: "MaterialNode".to_owned(),
        }),
        "the added node is the new 'normal' material node, projected through VizAdapter"
    );

    assert_eq!(
        diff.added_edges.len(),
        1,
        "exactly one material edge was added"
    );
    assert_eq!(
        diff.added_edges.get(&new_edge_id),
        Some(&EdgeRecord {
            src: normal,
            dst: output,
            data: "vector->texture".to_owned(),
        }),
        "the added edge record carries the new node, destination, and projected port-pair label"
    );

    assert!(
        diff.removed_nodes.is_empty(),
        "no material node was removed"
    );
    assert!(
        diff.removed_edges.is_empty(),
        "no material edge was removed"
    );
    assert!(
        diff.changed_nodes.is_empty(),
        "no existing material node projection changed"
    );
    assert!(
        diff.changed_edges.is_empty(),
        "no existing material edge projection changed"
    );
    assert_eq!(
        diff.node_change_count(),
        1,
        "the material diff is exactly one node-level change"
    );
    assert_eq!(
        diff.edge_change_count(),
        1,
        "the material diff is exactly one edge-level change"
    );
    assert!(
        !diff.is_empty(),
        "the material diff is non-empty (one add each on node and edge sides)"
    );
}

fn assert_anim_domain() {
    let mut graph = AnimGraph::new();
    let idle = graph.add_state("idle").expect("add idle state");
    let run = graph.add_state("run").expect("add run state");
    graph
        .add_transition(idle, run, AnimTransition::new("start_run"))
        .expect("add idle -> run transition");
    let old_snapshot = snapshot_via_adapter(&graph);

    let jump = graph.add_state("jump").expect("add jump state");
    let new_edge_id = graph
        .add_transition(run, jump, AnimTransition::new("leap"))
        .expect("add run -> jump transition");
    let new_snapshot = snapshot_via_adapter(&graph);

    let diff = GraphDiff::between(&old_snapshot, &new_snapshot);

    assert_eq!(
        diff.added_nodes.len(),
        1,
        "exactly one animation state was added"
    );
    assert_eq!(
        diff.added_nodes.get(&jump),
        Some(&DiffNode {
            display_name: "jump".to_owned(),
            kind: "AnimState".to_owned(),
        }),
        "the added node is the new 'jump' animation state, projected through VizAdapter"
    );

    assert_eq!(
        diff.added_edges.len(),
        1,
        "exactly one animation transition was added"
    );
    assert_eq!(
        diff.added_edges.get(&new_edge_id),
        Some(&EdgeRecord {
            src: run,
            dst: jump,
            data: "leap".to_owned(),
        }),
        "the added edge record carries the existing source, the new state, and the trigger label"
    );

    assert!(
        diff.removed_nodes.is_empty(),
        "no animation state was removed"
    );
    assert!(
        diff.removed_edges.is_empty(),
        "no animation transition was removed"
    );
    assert!(
        diff.changed_nodes.is_empty(),
        "no existing animation state projection changed"
    );
    assert!(
        diff.changed_edges.is_empty(),
        "no existing animation transition projection changed"
    );
    assert_eq!(
        diff.node_change_count(),
        1,
        "the animation diff is exactly one node-level change"
    );
    assert_eq!(
        diff.edge_change_count(),
        1,
        "the animation diff is exactly one edge-level change"
    );
    assert!(
        !diff.is_empty(),
        "the animation diff is non-empty (one add each on node and edge sides)"
    );
}

fn assert_operator_domain() {
    // Distinct transform payloads keep the two Transform NodeIds apart: the
    // CAD operator graph derives `NodeId` from the serialized operator
    // content, so two identical Transform payloads would collide on insert.
    let first_transform_op = OperatorNode::Transform(TransformOp {
        translation: [1.0, 0.0, 0.0],
        ..TransformOp::default()
    });
    let second_transform_op = OperatorNode::Transform(TransformOp {
        translation: [2.0, 0.0, 0.0],
        ..TransformOp::default()
    });

    let mut graph = OperatorGraph::new();
    let cuboid = graph
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid");
    let first_transform = graph
        .add_operator(first_transform_op)
        .expect("add first transform");
    graph
        .connect(cuboid, first_transform, 0)
        .expect("connect cuboid -> first_transform port 0");
    let old_snapshot = snapshot_via_adapter(&graph);

    let second_transform = graph
        .add_operator(second_transform_op)
        .expect("add second transform with a distinct payload");
    let new_edge_id = graph
        .connect(first_transform, second_transform, 0)
        .expect("connect first_transform -> second_transform port 0");
    let new_snapshot = snapshot_via_adapter(&graph);

    let diff = GraphDiff::between(&old_snapshot, &new_snapshot);

    assert_eq!(
        diff.added_nodes.len(),
        1,
        "exactly one operator node was added"
    );
    assert_eq!(
        diff.added_nodes.get(&second_transform),
        Some(&DiffNode {
            display_name: "Transform".to_owned(),
            kind: "Transform".to_owned(),
        }),
        "the added node is the second Transform operator, projected through VizAdapter"
    );

    assert_eq!(
        diff.added_edges.len(),
        1,
        "exactly one operator edge was added"
    );
    assert_eq!(
        diff.added_edges.get(&new_edge_id),
        Some(&EdgeRecord {
            src: first_transform,
            dst: second_transform,
            data: "input[0]".to_owned(),
        }),
        "the added edge record carries the first Transform, the new Transform, and the input-port-0 label"
    );

    assert!(
        diff.removed_nodes.is_empty(),
        "no operator node was removed"
    );
    assert!(
        diff.removed_edges.is_empty(),
        "no operator edge was removed"
    );
    assert!(
        diff.changed_nodes.is_empty(),
        "no existing operator node projection changed"
    );
    assert!(
        diff.changed_edges.is_empty(),
        "no existing operator edge projection changed"
    );
    assert_eq!(
        diff.node_change_count(),
        1,
        "the operator diff is exactly one node-level change"
    );
    assert_eq!(
        diff.edge_change_count(),
        1,
        "the operator diff is exactly one edge-level change"
    );
    assert!(
        !diff.is_empty(),
        "the operator diff is non-empty (one add each on node and edge sides)"
    );
}

fn assert_script_domain() {
    let mut graph = ScriptGraph::new();
    let entry = graph.add_node("entry").expect("add entry script node");
    let body = graph.add_node("body").expect("add body script node");
    graph
        .connect(entry, body, ScriptEdge::new("flow"))
        .expect("connect entry -> body");
    let old_snapshot = snapshot_via_adapter(&graph);

    let exit = graph.add_node("exit").expect("add exit script node");
    let new_edge_id = graph
        .connect(body, exit, ScriptEdge::new("done"))
        .expect("connect body -> exit");
    let new_snapshot = snapshot_via_adapter(&graph);

    let diff = GraphDiff::between(&old_snapshot, &new_snapshot);

    assert_eq!(
        diff.added_nodes.len(),
        1,
        "exactly one script node was added"
    );
    assert_eq!(
        diff.added_nodes.get(&exit),
        Some(&DiffNode {
            display_name: "exit".to_owned(),
            kind: "ScriptNode".to_owned(),
        }),
        "the added node is the new 'exit' script node, projected through VizAdapter"
    );

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
            data: "done".to_owned(),
        }),
        "the added edge record carries the existing body source, the new exit node, and the script edge key"
    );

    assert!(diff.removed_nodes.is_empty(), "no script node was removed");
    assert!(diff.removed_edges.is_empty(), "no script edge was removed");
    assert!(
        diff.changed_nodes.is_empty(),
        "no existing script node projection changed"
    );
    assert!(
        diff.changed_edges.is_empty(),
        "no existing script edge projection changed"
    );
    assert_eq!(
        diff.node_change_count(),
        1,
        "the script diff is exactly one node-level change"
    );
    assert_eq!(
        diff.edge_change_count(),
        1,
        "the script diff is exactly one edge-level change"
    );
    assert!(
        !diff.is_empty(),
        "the script diff is non-empty (one add each on node and edge sides)"
    );
}

// ---------------------------------------------------------------------------
// ISSUE-65 + ISSUE-72: combined Phase 8 checkpoint diff across all domains
// ---------------------------------------------------------------------------

/// Derive a test-local namespaced [`NodeId`] from the adapter-projected id and
/// an explicit domain label. Mixing the label into a BLAKE3 input keeps the
/// per-domain projections collision-free when inserted into one shared
/// graph-foundation `Graph<DiffNode, String>` without touching any production
/// id-derivation code. Deterministic and depends only on the adapter id plus
/// the domain label.
fn namespaced_node_id(domain: &str, id: NodeId) -> NodeId {
    let mut bytes = Vec::with_capacity(domain.len() + 1 + 16);
    bytes.extend_from_slice(domain.as_bytes());
    bytes.push(b':');
    bytes.extend_from_slice(&id.0.to_le_bytes());
    NodeId::from_bytes(&bytes)
}

/// Derive a test-local namespaced [`EdgeId`] from the adapter-projected id and
/// an explicit domain label. Symmetric companion to [`namespaced_node_id`];
/// see its doc for the namespacing contract.
fn namespaced_edge_id(domain: &str, id: EdgeId) -> EdgeId {
    let mut bytes = Vec::with_capacity(domain.len() + 1 + 16);
    bytes.extend_from_slice(domain.as_bytes());
    bytes.push(b':');
    bytes.extend_from_slice(&id.0.to_le_bytes());
    EdgeId::from_bytes(&bytes)
}

/// Fold one domain's public `VizAdapter` projection into `graph`, namespacing
/// each adapter `NodeId` / `EdgeId` (and each edge's `src` / `dst`) with the
/// `domain` label so multiple domain projections coexist deterministically in
/// the same `Graph<DiffNode, String>` without colliding. Display name, kind,
/// and edge label come straight from the adapter views — the namespacing
/// touches only the ids, preserving every other adapter-projected field.
fn extend_combined_with_adapter(
    graph: &mut Graph<DiffNode, String>,
    adapter: &dyn VizAdapter,
    domain: &str,
) {
    for view in adapter.nodes() {
        graph
            .insert_node(
                namespaced_node_id(domain, view.id),
                DiffNode {
                    display_name: view.display_name.to_owned(),
                    kind: view.kind.to_owned(),
                },
            )
            .expect("namespaced adapter node ids are unique within the combined graph");
    }
    for view in adapter.edges() {
        graph
            .insert_edge(
                namespaced_edge_id(domain, view.id),
                namespaced_node_id(domain, view.src),
                namespaced_node_id(domain, view.dst),
                view.label.to_owned(),
            )
            .expect("namespaced adapter edge endpoints refer to inserted nodes");
    }
}

/// Build one combined `GraphSnapshot<DiffNode, String>` over the Phase 8
/// domain graphs. Each domain's public `VizAdapter` projection is folded
/// into a single `Graph<DiffNode, String>` via [`extend_combined_with_adapter`]
/// before snapshotting; the resulting snapshot carries every domain's nodes
/// and edges side-by-side under deterministic per-domain namespaces.
fn combined_snapshot(
    material: &MaterialGraph,
    anim: &AnimGraph,
    operator: &OperatorGraph,
    script: &ScriptGraph,
) -> GraphSnapshot<DiffNode, String> {
    let mut graph: Graph<DiffNode, String> = Graph::new();
    extend_combined_with_adapter(&mut graph, material, "material");
    extend_combined_with_adapter(&mut graph, anim, "anim");
    extend_combined_with_adapter(&mut graph, operator, "operator");
    extend_combined_with_adapter(&mut graph, script, "script");
    GraphSnapshot::from_graph(&graph)
}

#[test]
fn graph_diff_between_reports_one_added_node_and_edge_per_domain_for_combined_phase8_checkpoint() {
    // Old combined state: one node + one edge baseline in each of the Phase
    // 8 graph domains.
    let mut material = MaterialGraph::new();
    let mat_albedo = material
        .add_node("albedo")
        .expect("add albedo material node");
    let mat_output = material
        .add_node("output")
        .expect("add output material node");
    material
        .connect(
            mat_albedo,
            mat_output,
            MaterialEdge {
                src_port: PortType::Color,
                dst_port: PortType::Color,
            },
        )
        .expect("connect albedo -> output");

    let mut anim = AnimGraph::new();
    let anim_idle = anim.add_state("idle").expect("add idle anim state");
    let anim_run = anim.add_state("run").expect("add run anim state");
    anim.add_transition(anim_idle, anim_run, AnimTransition::new("start_run"))
        .expect("add idle -> run transition");

    // Distinct Transform payloads keep the two operator Transform NodeIds
    // apart: `OperatorGraph` derives `NodeId` from the serialized operator
    // content, so two identical Transform payloads would collide on insert.
    let first_transform_op = OperatorNode::Transform(TransformOp {
        translation: [1.0, 0.0, 0.0],
        ..TransformOp::default()
    });
    let second_transform_op = OperatorNode::Transform(TransformOp {
        translation: [2.0, 0.0, 0.0],
        ..TransformOp::default()
    });

    let mut operator = OperatorGraph::new();
    let op_cuboid = operator
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid operator");
    let op_first_transform = operator
        .add_operator(first_transform_op)
        .expect("add first transform operator");
    operator
        .connect(op_cuboid, op_first_transform, 0)
        .expect("connect cuboid -> first_transform port 0");

    let mut script = ScriptGraph::new();
    let script_entry = script.add_node("entry").expect("add entry script node");
    let script_body = script.add_node("body").expect("add body script node");
    script
        .connect(script_entry, script_body, ScriptEdge::new("flow"))
        .expect("connect entry -> body");

    let old_combined_checkpoint = combined_snapshot(&material, &anim, &operator, &script);

    // Mutate each domain by exactly one node and one edge.
    let mat_normal = material
        .add_node("normal")
        .expect("add normal material node");
    let new_material_edge_id = material
        .connect(
            mat_normal,
            mat_output,
            MaterialEdge {
                src_port: PortType::Vector,
                dst_port: PortType::Texture,
            },
        )
        .expect("connect normal -> output");

    let anim_jump = anim.add_state("jump").expect("add jump anim state");
    let new_anim_edge_id = anim
        .add_transition(anim_run, anim_jump, AnimTransition::new("leap"))
        .expect("add run -> jump transition");

    let op_second_transform = operator
        .add_operator(second_transform_op)
        .expect("add second transform operator with a distinct payload");
    let new_operator_edge_id = operator
        .connect(op_first_transform, op_second_transform, 0)
        .expect("connect first_transform -> second_transform port 0");

    let script_exit = script.add_node("exit").expect("add exit script node");
    let new_script_edge_id = script
        .connect(script_body, script_exit, ScriptEdge::new("done"))
        .expect("connect body -> exit");

    let new_combined_checkpoint = combined_snapshot(&material, &anim, &operator, &script);

    // Exactly one combined diff over the combined-checkpoint pair.
    let diff = GraphDiff::between(&old_combined_checkpoint, &new_combined_checkpoint);

    // Four added nodes — one per domain — and four added edges, with
    // nothing removed or mutated on the pre-existing projections.
    assert_eq!(
        diff.added_nodes.len(),
        4,
        "exactly one node added per domain across the combined checkpoint diff"
    );
    assert_eq!(
        diff.added_edges.len(),
        4,
        "exactly one edge added per domain across the combined checkpoint diff"
    );
    assert!(
        diff.removed_nodes.is_empty(),
        "no node was removed in any domain"
    );
    assert!(
        diff.removed_edges.is_empty(),
        "no edge was removed in any domain"
    );
    assert!(
        diff.changed_nodes.is_empty(),
        "no existing combined-projection node record changed"
    );
    assert!(
        diff.changed_edges.is_empty(),
        "no existing combined-projection edge record changed"
    );
    assert_eq!(
        diff.node_change_count(),
        4,
        "node-level changes total exactly the per-domain adds"
    );
    assert_eq!(
        diff.edge_change_count(),
        4,
        "edge-level changes total exactly the per-domain adds"
    );
    assert!(
        !diff.is_empty(),
        "the combined diff is non-empty (one add per domain on each of node and edge sides)"
    );

    // Identity: the added node entries are exactly the namespaced new
    // nodes, each carrying the adapter's display name and kind unchanged.
    // This pins that the combined helper preserves the adapter projection's
    // node-side structure through the namespacing.
    let new_material_node = namespaced_node_id("material", mat_normal);
    let new_anim_node = namespaced_node_id("anim", anim_jump);
    let new_operator_node = namespaced_node_id("operator", op_second_transform);
    let new_script_node = namespaced_node_id("script", script_exit);
    assert_eq!(
        diff.added_nodes.get(&new_material_node),
        Some(&DiffNode {
            display_name: "normal".to_owned(),
            kind: "MaterialNode".to_owned(),
        }),
        "the new material node is in added_nodes with its VizAdapter display name and kind"
    );
    assert_eq!(
        diff.added_nodes.get(&new_anim_node),
        Some(&DiffNode {
            display_name: "jump".to_owned(),
            kind: "AnimState".to_owned(),
        }),
        "the new animation state is in added_nodes with its VizAdapter display name and kind"
    );
    assert_eq!(
        diff.added_nodes.get(&new_operator_node),
        Some(&DiffNode {
            display_name: "Transform".to_owned(),
            kind: "Transform".to_owned(),
        }),
        "the new operator node is in added_nodes with its VizAdapter display name and kind"
    );
    assert_eq!(
        diff.added_nodes.get(&new_script_node),
        Some(&DiffNode {
            display_name: "exit".to_owned(),
            kind: "ScriptNode".to_owned(),
        }),
        "the new script node is in added_nodes with its VizAdapter display name and kind"
    );

    // Identity: the added edge entries are exactly the namespaced new
    // edges, each carrying the adapter's `src`, `dst`, and label unchanged
    // (with endpoints likewise namespaced under the same domain label as
    // the edge). This pins that the combined helper preserves edge
    // identity, edge endpoints, and edge labels across all domains.
    assert_eq!(
        diff.added_edges
            .get(&namespaced_edge_id("material", new_material_edge_id)),
        Some(&EdgeRecord {
            src: new_material_node,
            dst: namespaced_node_id("material", mat_output),
            data: "vector->texture".to_owned(),
        }),
        "the new material edge is in added_edges with its namespaced endpoints and projected label"
    );
    assert_eq!(
        diff.added_edges.get(&namespaced_edge_id("anim", new_anim_edge_id)),
        Some(&EdgeRecord {
            src: namespaced_node_id("anim", anim_run),
            dst: new_anim_node,
            data: "leap".to_owned(),
        }),
        "the new animation edge is in added_edges with its namespaced endpoints and projected trigger label"
    );
    assert_eq!(
        diff.added_edges.get(&namespaced_edge_id("operator", new_operator_edge_id)),
        Some(&EdgeRecord {
            src: namespaced_node_id("operator", op_first_transform),
            dst: new_operator_node,
            data: "input[0]".to_owned(),
        }),
        "the new operator edge is in added_edges with its namespaced endpoints and projected input-port label"
    );
    assert_eq!(
        diff.added_edges
            .get(&namespaced_edge_id("script", new_script_edge_id)),
        Some(&EdgeRecord {
            src: namespaced_node_id("script", script_body),
            dst: new_script_node,
            data: "done".to_owned(),
        }),
        "the new script edge is in added_edges with its namespaced endpoints and projected script-edge-key label"
    );
}
