//! ISSUE-63: integration smoke proving the one generic `NodeGraphWidget` path
//! can model all three Phase 8 graph domains — `rge_material_graph::MaterialGraph`,
//! `rge_anim_graph::AnimGraph`, and `rge_cad_core::OperatorGraph` — through the
//! existing `rge_kernel_graph_foundation::VizAdapter` bridge.
//!
//! This test deliberately lives outside `crates/editor-ui/src/**`: the
//! production editor-ui surface stays domain-agnostic (knows nothing about
//! materials, animation, CAD, scripts, operators) and must NOT name
//! `rge-material-graph`, `rge-anim-graph`, or `rge-cad-core`. Only this
//! integration test target consumes those crates, via the editor-ui
//! `[dev-dependencies]` edges added for ISSUE-60, ISSUE-61, and ISSUE-62.
//!
//! Consolidation smoke: the three single-domain smokes (`node_graph_material_smoke`,
//! `node_graph_anim_graph_smoke`, `node_graph_operator_smoke`) already prove each
//! adapter individually. This test proves the same `NodeGraphWidget` model path
//! handles all three domains within one test target — no domain-specific editor
//! code, no production API change.

use rge_anim_graph::{AnimGraph, AnimTransition};
use rge_cad_core::{BooleanOp, CuboidOp, OperatorGraph, OperatorNode};
use rge_editor_ui::widgets::node_graph::NodeGraphWidget;
use rge_material_graph::{MaterialEdge, MaterialGraph, PortType};

#[test]
fn node_graph_widget_consumes_all_three_phase8_domains_through_viz_adapter() {
    assert_material_domain();
    assert_anim_domain();
    assert_operator_domain();
}

fn assert_material_domain() {
    // Real material graph: three material nodes joined by two typed-port
    // connections (distinct port-type pairs).
    let mut graph = MaterialGraph::new();
    let albedo = graph.add_node("albedo").expect("add albedo node");
    let normal = graph.add_node("normal").expect("add normal node");
    let output = graph.add_node("output").expect("add output node");

    let albedo_edge = graph
        .connect(
            albedo,
            output,
            MaterialEdge {
                src_port: PortType::Color,
                dst_port: PortType::Color,
            },
        )
        .expect("connect albedo -> output");
    let normal_edge = graph
        .connect(
            normal,
            output,
            MaterialEdge {
                src_port: PortType::Vector,
                dst_port: PortType::Texture,
            },
        )
        .expect("connect normal -> output");

    let widget = NodeGraphWidget::new();
    let model = widget.model_from(&graph);

    assert_eq!(model.node_count(), 3, "three material nodes are exposed");
    assert_eq!(model.edge_count(), 2, "two material edges are exposed");

    let node_by_id = |id| {
        model
            .nodes()
            .iter()
            .copied()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("material node {id:?} missing from NodeGraphModel"))
    };
    let edge_by_id = |id| {
        model
            .edges()
            .iter()
            .copied()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("material edge {id:?} missing from NodeGraphModel"))
    };

    let albedo_node = node_by_id(albedo);
    let normal_node = node_by_id(normal);
    let output_node = node_by_id(output);

    assert_eq!(albedo_node.display_name, "albedo");
    assert_eq!(normal_node.display_name, "normal");
    assert_eq!(output_node.display_name, "output");

    assert_eq!(albedo_node.kind, "MaterialNode");
    assert_eq!(normal_node.kind, "MaterialNode");
    assert_eq!(output_node.kind, "MaterialNode");

    let albedo_record = edge_by_id(albedo_edge);
    assert_eq!(albedo_record.src, albedo);
    assert_eq!(albedo_record.dst, output);
    assert_eq!(albedo_record.label, "color->color");

    let normal_record = edge_by_id(normal_edge);
    assert_eq!(normal_record.src, normal);
    assert_eq!(normal_record.dst, output);
    assert_eq!(normal_record.label, "vector->texture");
}

fn assert_anim_domain() {
    // Real animation graph: two states joined by one transition.
    let mut graph = AnimGraph::new();
    let idle = graph.add_state("idle").expect("add idle state");
    let run = graph.add_state("run").expect("add run state");

    let start_run = graph
        .add_transition(idle, run, AnimTransition::new("start_run"))
        .expect("add idle -> run transition");

    let widget = NodeGraphWidget::new();
    let model = widget.model_from(&graph);

    assert_eq!(model.node_count(), 2, "two animation states are exposed");
    assert_eq!(model.edge_count(), 1, "one animation transition is exposed");

    let node_by_id = |id| {
        model
            .nodes()
            .iter()
            .copied()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("anim node {id:?} missing from NodeGraphModel"))
    };
    let edge_by_id = |id| {
        model
            .edges()
            .iter()
            .copied()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("anim edge {id:?} missing from NodeGraphModel"))
    };

    let idle_node = node_by_id(idle);
    let run_node = node_by_id(run);

    assert_eq!(idle_node.display_name, "idle");
    assert_eq!(run_node.display_name, "run");

    assert_eq!(idle_node.kind, "AnimState");
    assert_eq!(run_node.kind, "AnimState");

    let start_run_record = edge_by_id(start_run);
    assert_eq!(start_run_record.src, idle);
    assert_eq!(start_run_record.dst, run);
    assert_eq!(start_run_record.label, "start_run");
}

fn assert_operator_domain() {
    // Real operator graph: two distinct cuboid operators feeding one Boolean
    // union operator on ports 0 and 1. Distinct cuboid payloads ensure
    // content-derived `NodeId`s do not collide.
    let mut graph = OperatorGraph::new();
    let cuboid_left = graph
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid_left");
    let cuboid_right = graph
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 2.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid_right");
    let boolean = graph
        .add_operator(OperatorNode::Boolean(BooleanOp::union()))
        .expect("add boolean union");

    let edge_left = graph
        .connect(cuboid_left, boolean, 0)
        .expect("connect cuboid_left -> boolean port 0");
    let edge_right = graph
        .connect(cuboid_right, boolean, 1)
        .expect("connect cuboid_right -> boolean port 1");

    let widget = NodeGraphWidget::new();
    let model = widget.model_from(&graph);

    assert_eq!(model.node_count(), 3, "three operator nodes are exposed");
    assert_eq!(model.edge_count(), 2, "two operator edges are exposed");

    let node_by_id = |id| {
        model
            .nodes()
            .iter()
            .copied()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("operator node {id:?} missing from NodeGraphModel"))
    };
    let edge_by_id = |id| {
        model
            .edges()
            .iter()
            .copied()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("operator edge {id:?} missing from NodeGraphModel"))
    };

    let cuboid_left_node = node_by_id(cuboid_left);
    let cuboid_right_node = node_by_id(cuboid_right);
    let boolean_node = node_by_id(boolean);

    assert_eq!(cuboid_left_node.display_name, "Cuboid");
    assert_eq!(cuboid_left_node.kind, "Cuboid");
    assert_eq!(cuboid_right_node.display_name, "Cuboid");
    assert_eq!(cuboid_right_node.kind, "Cuboid");
    assert_eq!(boolean_node.display_name, "Boolean");
    assert_eq!(boolean_node.kind, "Boolean");

    let edge_left_record = edge_by_id(edge_left);
    assert_eq!(edge_left_record.src, cuboid_left);
    assert_eq!(edge_left_record.dst, boolean);
    assert_eq!(edge_left_record.label, "input[0]");

    let edge_right_record = edge_by_id(edge_right);
    assert_eq!(edge_right_record.src, cuboid_right);
    assert_eq!(edge_right_record.dst, boolean);
    assert_eq!(edge_right_record.label, "input[1]");
}
