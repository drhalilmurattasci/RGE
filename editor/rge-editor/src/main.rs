//! `rge-editor` — main editor binary.
//!
//! Sub-δ.1.B: real winit application that opens a window and displays a
//! single 1×1×1 Lambert+Phong-shaded cuboid against a neutral gray
//! background. Mouse interaction / picking / selection feedback are NOT
//! in scope (sub-δ.2 onwards).
//!
//! Construction sequence:
//!
//! 1. Build a `CadGraph` and commit a `CuboidOp(1.0, 1.0, 1.0)` as the
//!    root operator.
//! 2. Build a `CadProjection`, register `BRepHandle` as a snapshot
//!    component on a fresh `rge_kernel_ecs::World`, and spawn one
//!    entity bound to the cuboid node.
//! 3. Mutate the entity's `BRepHandle.brep_owner` to a deterministic
//!    16-byte seed so the projection's face IDs are stable.
//! 4. Tick the projection so the cuboid's `ProjectedMesh` lands in the
//!    cache.
//! 5. Hand the world / projection / graph triple to
//!    [`EditorShell::with_world_projection_graph`]; run the winit event
//!    loop. The shell's `resumed` callback constructs the wgpu
//!    instance + surface + pipeline + GPU mesh, and
//!    `RedrawRequested` renders the cuboid.

use std::process::ExitCode;

use rge_cad_core::{BRepOwnerId, CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_editor_shell::EditorShell;
use rge_kernel_ecs::World;
use winit::event_loop::EventLoop;

/// Deterministic owner seed for the demo cuboid's `BRepHandle`. The
/// 16-byte choice is arbitrary; it just has to be stable so face-ID
/// resolution is reproducible across runs.
const ENTITY_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

fn main() -> ExitCode {
    // Best-effort tracing init. If a global subscriber is already
    // installed (rare for a fresh binary), the call returns Err which
    // we drop deliberately. The clippy `let_underscore_drop` lint
    // forbids the bare `let _` pattern on values that have a
    // destructor; explicit `drop` is the recommended replacement.
    drop(tracing_subscriber::fmt::try_init());

    // ---- Step 1: build the CadGraph with one cuboid root --------------
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin_operation");
    let cuboid_node = graph
        .graph_mut()
        .expect("graph_mut after begin")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add_operator Cuboid");
    graph
        .graph_mut()
        .expect("graph_mut for set_root")
        .set_root(cuboid_node)
        .expect("set_root");
    graph.commit("rge-editor demo cuboid").expect("commit");

    // ---- Step 2: spawn an ECS entity for the cuboid -------------------
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, cuboid_node)
        .expect("spawn_brep_entity");

    // ---- Step 3: assign brep_owner so face IDs resolve ---------------
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }

    // ---- Step 4: tick projection so ProjectedMesh lands in cache -----
    let tolerance = Tolerance::new(0.001).expect("tolerance");
    projection
        .tick(&mut world, &graph, tolerance)
        .expect("projection tick");

    // ---- Step 5: hand off to EditorShell + run winit -----------------
    let mut shell = EditorShell::with_world_projection_graph(world, projection, graph);

    let event_loop = EventLoop::new().expect("event loop");
    if let Err(e) = event_loop.run_app(&mut shell) {
        eprintln!("rge-editor: run_app: {e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
