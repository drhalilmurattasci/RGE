//! Headless face-picking sub-α end-to-end smoke for
//! [`CadProjection::pick_face`].
//!
//! These tests exercise the **closest-resolvable face hit** selection rule
//! that distinguishes the picker from a naive closest-triangle picker:
//!
//! * Hits whose triangle does NOT resolve a [`BRepFaceId`] (unlabeled
//!   tessellation OR topology-changing source operator OR no owner) are
//!   transparent to the picker — they do NOT mask resolvable faces behind
//!   them.
//! * Hits whose triangle DOES resolve are sorted by ray-`t`, and the first
//!   one in that sorted order wins.
//!
//! The 6 integration tests cover, in order:
//!
//! 1. `pick_hits_cuboid_top_face` — baseline. Synthetic ray pointing -Z
//!    from above a 1×1×1 cuboid centered at origin → returns the cuboid's
//!    +Z face.
//! 2. `pick_returns_nearest_when_two_cuboids_along_ray` — two cuboids
//!    along the ray (a larger 2×2×2 wrapping a smaller 1×1×1, both at
//!    origin so the larger's top is in front of the smaller's top along
//!    the ray). The picker MUST return the entity whose top face is hit
//!    FIRST in `t` — the larger one.
//! 3. `pick_returns_none_when_owner_missing` — entity has a `BRepHandle`
//!    with `brep_owner == None` (mutated post-spawn). Even though the
//!    geometry intersects, the picker filters such entities out at iteration
//!    time and returns `None`.
//! 4. `pick_resolves_filleted_geometry_top_face` — `Cuboid -> Fillet`
//!    chain bound to a single entity. Inherited Cuboid labels resolve
//!    through the Fillet root, so the picker returns the inherited +Z face.
//! 5. `pick_punches_through_unresolvable_to_resolvable_behind` ← LOAD-BEARING
//!    for the closest-resolvable rule. Two entities: a front entity with no
//!    `brep_owner` (unresolvable) and a plain cuboid entity deeper in
//!    `t`-order (resolvable). Ray from above. The picker MUST return the
//!    cuboid hit; unresolvable front hits are walked past transparently.
//! 6. `pick_returns_none_when_ray_misses_all_geometry` — ray pointing into
//!    empty space → `None`.
//!
//! Bonus test 7 (`pick_composes_into_face_selection`) demonstrates downstream
//! composition: cad-projection does NOT depend on `editor-state` in
//! production, but a caller can build `editor_state::FaceSelection { entity,
//! owner, face_id }` from a [`FacePick`] using the dev-dep.
//!
//! Test 8 (`pick_resolves_round_filleted_geometry_top_face`) is the
//! RoundFillet sibling of test 4: it exercises the same inherited-Cuboid
//! face-identity resolution rule, but through `OperatorNode::RoundFillet`
//! (the topology-preserving rounded-edge operator) instead of
//! `OperatorNode::Fillet`.
//!
//! Test 9 (`pick_resolves_extrude_top_cap_face`) is the variable-N topology
//! sibling of tests 1-8: those all pick fixed-topology Cuboid (or
//! Cuboid-rooted) geometry, while this one projects a directly rooted
//! square-profile `OperatorNode::Extrude` and proves `pick_face` flows that
//! variable-N consumer through the same picker path, returning the stable
//! `ExtrudeFaceTag::Top` cap identity.
//!
//! Test 10 (`pick_resolves_revolve_start_cap_face`) is the mode-driven
//! topology sibling of test 9: it projects a directly rooted partial-mode
//! square-ring `OperatorNode::Revolve` and proves `pick_face` flows the
//! Revolve consumer through the same picker path, returning the stable
//! `RevolveFaceTag::StartCap { profile_count: 4 }` cap identity.
//!
//! Test 11 (`pick_resolves_loft_top_cap_face`) is the two-profile topology
//! sibling of test 9: it projects a directly rooted square-to-larger-square
//! `OperatorNode::Loft` and proves `pick_face` flows the Loft consumer
//! through the same picker path, returning the stable `LoftFaceTag::Top`
//! cap identity. The projection crate already has Loft face-ID *lookup*
//! coverage (`loft_brep_face_id_lookup_smoke.rs`); this test extends that to
//! the ray-driven `pick_face` integration surface.

use std::f32::consts::PI;

use rge_cad_core::{
    BRepEdgeProvider, BRepFaceId, BRepOwnerId, CadGraph, CuboidFaceTag, CuboidOp, ExtrudeFaceTag,
    ExtrudeOp, FilletOp, LoftFaceTag, LoftOp, OperatorNode, Polygon2D, RevolveFaceTag, RevolveOp,
    RoundFilletOp, Tolerance,
};
use rge_cad_projection::{BRepHandle, CadProjection, FacePick, Ray};
use rge_kernel_ecs::World;

const ENTITY_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);
const SECOND_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x77; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// Build a `(graph, projection, world, entity)` tuple with a single Cuboid
/// committed and projected. The `BRepHandle.brep_owner` is set to `owner`
/// post-spawn.
fn build_cuboid(
    width: f32,
    height: f32,
    depth: f32,
    owner: BRepOwnerId,
) -> (CadGraph, CadProjection, World, rge_kernel_ecs::EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width,
            height,
            depth,
        }))
        .expect("add cuboid");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(cuboid_node)
        .expect("set root");
    graph.commit("cuboid").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, cuboid_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(owner);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity)
}

/// Add a fresh Cuboid entity (with `brep_owner = Some(owner)`) into an
/// existing `(graph, projection, world)` triple. Returns the new
/// `EntityId`. The graph is committed if `commit_msg` is `Some`; otherwise
/// the caller is expected to commit later.
fn add_cuboid_entity(
    graph: &mut CadGraph,
    projection: &mut CadProjection,
    world: &mut World,
    width: f32,
    height: f32,
    depth: f32,
    owner: BRepOwnerId,
    commit_msg: Option<&str>,
) -> rge_kernel_ecs::EntityId {
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width,
            height,
            depth,
        }))
        .expect("add cuboid");
    if let Some(msg) = commit_msg {
        graph.commit(msg).expect("commit");
    }
    let entity = projection
        .spawn_brep_entity(world, cuboid_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(owner);
        }
    }
    entity
}

/// **Test 1** — baseline. Single 1×1×1 cuboid centered at origin; ray from
/// `z=+5` along `-Z` hits the +Z (top) face; picker returns the
/// corresponding `BRepFaceId::for_cuboid_face(owner, PosZ)`.
#[test]
fn pick_hits_cuboid_top_face() {
    let (graph, projection, world, entity) = build_cuboid(1.0, 1.0, 1.0, ENTITY_OWNER);
    let ray = Ray {
        origin: [0.0, 0.0, 5.0],
        direction: [0.0, 0.0, -1.0],
    };
    let pick: FacePick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("ray must hit cuboid top face");

    assert_eq!(pick.entity, entity, "the cuboid entity must be picked");
    assert_eq!(pick.owner, ENTITY_OWNER);
    let expected = BRepFaceId::for_cuboid_face(ENTITY_OWNER, CuboidFaceTag::PosZ);
    assert_eq!(
        pick.face_id, expected,
        "the picked face_id must be the +Z (top) face's stable identity"
    );
    assert!(pick.t > 0.0, "ray-t must be strictly positive");
    // The cuboid's top face is at z=+0.5; ray origin at z=+5 along -Z hits
    // at t = 4.5. Allow a generous epsilon to accommodate floating-point
    // accumulation.
    assert!(
        (pick.t - 4.5).abs() < 1e-4,
        "ray-t at the +Z face is expected at 4.5, got {}",
        pick.t
    );
}

/// **Test 2** — closest-along-ray wins between two resolvable cuboids.
///
/// Setup: a 2×2×2 cuboid (owned by `ENTITY_OWNER`, top at `z=+1.0`) AND a
/// 1×1×1 cuboid (owned by `SECOND_OWNER`, top at `z=+0.5`), both centered
/// at origin so the larger envelops the smaller. Ray from `z=+5` along
/// `-Z` hits the larger cuboid's top face FIRST (`t=4.0`), then the
/// smaller one's top face at `t=4.5`.
///
/// The picker MUST return the larger cuboid's `entity`, not the smaller.
#[test]
fn pick_returns_nearest_when_two_cuboids_along_ray() {
    let (mut graph, mut projection, mut world, larger_entity) =
        build_cuboid(2.0, 2.0, 2.0, ENTITY_OWNER);
    let smaller_entity = add_cuboid_entity(
        &mut graph,
        &mut projection,
        &mut world,
        1.0,
        1.0,
        1.0,
        SECOND_OWNER,
        Some("smaller cuboid"),
    );
    projection
        .tick(&mut world, &graph, tol())
        .expect("re-tick after second add");

    let ray = Ray {
        origin: [0.0, 0.0, 5.0],
        direction: [0.0, 0.0, -1.0],
    };
    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("ray must hit at least one cuboid top face");

    assert_eq!(
        pick.entity, larger_entity,
        "the closer (larger) cuboid must be picked, not {smaller_entity:?}"
    );
    assert_eq!(pick.owner, ENTITY_OWNER);
    assert_eq!(
        pick.face_id,
        BRepFaceId::for_cuboid_face(ENTITY_OWNER, CuboidFaceTag::PosZ),
        "the picked face_id must be the larger cuboid's +Z face under \
         ENTITY_OWNER (different from SECOND_OWNER's tag)"
    );
    // Larger cuboid (2×2×2) top at z=+1.0, ray-t = 4.0.
    assert!(
        (pick.t - 4.0).abs() < 1e-4,
        "ray-t expected ≈4.0, got {}",
        pick.t
    );
}

/// **Test 3** — entity has a `BRepHandle` with `brep_owner == None`. Even
/// though the cuboid geometry intersects the ray, the picker filters
/// no-owner entities out before considering their triangles → `None`.
#[test]
fn pick_returns_none_when_owner_missing() {
    let (graph, projection, mut world, entity) = build_cuboid(1.0, 1.0, 1.0, ENTITY_OWNER);
    // Mutate brep_owner to None post-spawn.
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = None;
        }
    }

    let ray = Ray {
        origin: [0.0, 0.0, 5.0],
        direction: [0.0, 0.0, -1.0],
    };
    let pick = projection.pick_face(&ray, &world, graph.graph());
    assert!(
        pick.is_none(),
        "no-owner entities must be transparent to the picker; got {pick:?}"
    );
}

/// **Test 4** — `Cuboid -> Fillet` output: inherited Cuboid labels resolve
/// through the Fillet root, so a ray from +Z picks the inherited +Z face.
#[test]
fn pick_resolves_filleted_geometry_top_face() {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid.clone()))
        .expect("cuboid");
    let edge_id = cuboid.brep_edge_ids(ENTITY_OWNER)[0];
    let fillet =
        FilletOp::new(&cuboid, ENTITY_OWNER, vec![edge_id], 0.1).expect("fillet construction");
    let fillet_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(fillet))
        .expect("fillet node");
    graph
        .graph_mut()
        .expect("mut")
        .connect(cuboid_node, fillet_node, 0)
        .expect("connect");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(fillet_node)
        .expect("set root");
    graph.commit("cuboid -> fillet").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, fillet_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let ray = Ray {
        origin: [0.0, 0.0, 5.0],
        direction: [0.0, 0.0, -1.0],
    };
    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("filleted Cuboid should resolve inherited face identity");
    assert_eq!(pick.entity, entity);
    assert_eq!(pick.owner, ENTITY_OWNER);
    assert_eq!(
        pick.face_id,
        BRepFaceId::for_cuboid_face(ENTITY_OWNER, CuboidFaceTag::PosZ),
        "ray from +Z should resolve the inherited Cuboid +Z face through FilletOp"
    );
}

/// **Test 5 — LOAD-BEARING for the closest-resolvable rule.**
///
/// Two entities in the same world:
///
/// * **Front (in `t`-order, unresolvable)**: a `Cuboid → Fillet` chain
///   producing an unlabeled tessellation. The input cuboid is 4×4×4, so
///   the fillet output's bounding box's PosZ-ish triangles are around
///   `z ≈ +2.0` (give or take chamfer geometry); the ray hits these
///   FIRST in `t` (around `t ≈ 3.0`).
/// * **Behind (in `t`-order, resolvable)**: a plain 1×1×1 cuboid centered
///   at origin (top at `z=+0.5`). Its top face triangles are hit at
///   `t = 4.5`, AFTER the fillet's hits.
///
/// The picker MUST return the plain cuboid's hit — the fillet's
/// unresolvable hits are transparent and walked past in the resolve loop.
/// This is the bug-prevention measure that distinguishes "closest resolvable"
/// from "closest geometric".
#[test]
fn pick_punches_through_unresolvable_to_resolvable_behind() {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    // Front entity — Cuboid 4x4x4 → Fillet (unresolvable identity).
    let front_cuboid = CuboidOp {
        width: 4.0,
        height: 4.0,
        depth: 4.0,
    };
    let front_cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(front_cuboid.clone()))
        .expect("front cuboid");
    let front_edge = front_cuboid.brep_edge_ids(ENTITY_OWNER)[0];
    let front_fillet =
        FilletOp::new(&front_cuboid, ENTITY_OWNER, vec![front_edge], 0.1).expect("front fillet");
    let front_fillet_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(front_fillet))
        .expect("front fillet node");
    graph
        .graph_mut()
        .expect("mut")
        .connect(front_cuboid_node, front_fillet_node, 0)
        .expect("connect front");
    graph
        .commit("front cuboid -> fillet")
        .expect("commit front");

    // Behind entity — plain Cuboid 1x1x1 (resolvable identity).
    graph.begin_operation().expect("begin behind");
    let behind_cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let behind_cuboid_node = graph
        .graph_mut()
        .expect("mut behind")
        .add_operator(OperatorNode::Cuboid(behind_cuboid))
        .expect("behind cuboid");
    graph.commit("behind plain cuboid").expect("commit behind");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();

    // Spawn front (fillet) entity bound to the fillet root node, then
    // leave its owner empty so its geometric hits stay unresolvable.
    let front_entity = projection
        .spawn_brep_entity(&mut world, front_fillet_node)
        .expect("spawn front");
    if let Some(mut em) = world.entity_mut(front_entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = None;
        }
    }
    // Spawn behind (plain cuboid) entity.
    let behind_entity = projection
        .spawn_brep_entity(&mut world, behind_cuboid_node)
        .expect("spawn behind");
    if let Some(mut em) = world.entity_mut(behind_entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(SECOND_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let ray = Ray {
        origin: [0.0, 0.0, 5.0],
        direction: [0.0, 0.0, -1.0],
    };
    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("punch-through must yield the cuboid behind, NOT None");
    assert_eq!(
        pick.entity, behind_entity,
        "the resolvable cuboid (behind) must be picked; \
         unresolvable fillet hits in front must be walked past"
    );
    assert_eq!(pick.owner, SECOND_OWNER);
    assert_eq!(
        pick.face_id,
        BRepFaceId::for_cuboid_face(SECOND_OWNER, CuboidFaceTag::PosZ),
        "the picked face_id must be the plain cuboid's +Z face under \
         SECOND_OWNER (the resolvable identity space)"
    );
    // The plain cuboid's top is at z=+0.5; t = 4.5.
    assert!(
        (pick.t - 4.5).abs() < 1e-4,
        "the picker must report the cuboid's t (4.5), not the fillet's earlier t; got {}",
        pick.t
    );
}

/// **Test 6** — ray pointing into empty space far above the cuboid; no
/// triangle is hit, picker returns `None`.
#[test]
fn pick_returns_none_when_ray_misses_all_geometry() {
    let (graph, projection, world, _entity) = build_cuboid(1.0, 1.0, 1.0, ENTITY_OWNER);
    // Origin off to the side, direction along +Y — ray goes from (10, 0, 0)
    // along +Y, never crossing the cuboid which lives in [-0.5, 0.5]^3.
    let ray = Ray {
        origin: [10.0, 0.0, 0.0],
        direction: [0.0, 1.0, 0.0],
    };
    let pick = projection.pick_face(&ray, &world, graph.graph());
    assert!(
        pick.is_none(),
        "ray pointing into empty space must return None; got {pick:?}"
    );
}

/// **Bonus Test 7** — picker output composes into `editor-state::FaceSelection`.
///
/// `cad-projection` does NOT depend on `editor-state` in production. The
/// returned [`FacePick`]'s three identifying fields (`entity`, `owner`,
/// `face_id`) are exactly the three fields of `FaceSelection` — so a
/// caller can construct one from the other in one line. This test
/// demonstrates the composition path through the dev-dep.
#[test]
fn pick_composes_into_face_selection() {
    use rge_editor_state::FaceSelection;

    let (graph, projection, world, entity) = build_cuboid(1.0, 1.0, 1.0, ENTITY_OWNER);
    let ray = Ray {
        origin: [0.0, 0.0, 5.0],
        direction: [0.0, 0.0, -1.0],
    };
    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("must hit");

    let selection = FaceSelection {
        entity: pick.entity,
        owner: pick.owner,
        face_id: pick.face_id,
    };
    assert_eq!(selection.entity, entity);
    assert_eq!(selection.owner, ENTITY_OWNER);
    assert_eq!(
        selection.face_id,
        BRepFaceId::for_cuboid_face(ENTITY_OWNER, CuboidFaceTag::PosZ)
    );
}

/// **Test 8** — `Cuboid -> RoundFillet` output: inherited Cuboid labels
/// resolve through the RoundFillet root, so a ray from +Z picks the
/// inherited +Z face.
///
/// This is the RoundFillet sibling of test 4
/// (`pick_resolves_filleted_geometry_top_face`): that test exercises
/// `OperatorNode::Fillet`; this one specifically exercises
/// `OperatorNode::RoundFillet`, the topology-preserving rounded-edge
/// operator. Per ADR-119 D4, `RoundFilletOp` clones the upstream Cuboid's
/// faces verbatim, so the inherited +Z face still resolves to the exact
/// upstream Cuboid `BRepFaceId`. The entity under test is bound to the
/// RoundFillet root node (not the upstream Cuboid node), so the picker
/// assertion proves resolution through the projected RoundFillet output.
#[test]
fn pick_resolves_round_filleted_geometry_top_face() {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid.clone()))
        .expect("cuboid");
    // Every edge passed into `RoundFilletOp::new` comes from the upstream
    // Cuboid's `BRepEdgeProvider` surface — no `BRepEdgeId` is synthesized.
    let edges = cuboid.brep_edge_ids(ENTITY_OWNER);
    let round = RoundFilletOp::new(&cuboid, ENTITY_OWNER, vec![edges[0]], 0.1)
        .expect("round fillet construction");
    let round_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::RoundFillet(round))
        .expect("round fillet node");
    graph
        .graph_mut()
        .expect("mut")
        .connect(cuboid_node, round_node, 0)
        .expect("connect");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(round_node)
        .expect("set root");
    graph.commit("cuboid -> round fillet").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    // Bind the projected entity to the RoundFillet root node, not the
    // upstream Cuboid node.
    let entity = projection
        .spawn_brep_entity(&mut world, round_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let ray = Ray {
        origin: [0.0, 0.0, 5.0],
        direction: [0.0, 0.0, -1.0],
    };
    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("round-filleted Cuboid should resolve inherited face identity");
    assert_eq!(
        pick.entity, entity,
        "the RoundFillet-bound entity must be picked"
    );
    assert_eq!(pick.owner, ENTITY_OWNER);
    assert_eq!(
        pick.face_id,
        BRepFaceId::for_cuboid_face(ENTITY_OWNER, CuboidFaceTag::PosZ),
        "ray from +Z should resolve the inherited Cuboid +Z face through RoundFilletOp"
    );
}

/// **Test 9** — direct square-profile `ExtrudeOp` root: a ray from above the
/// center of the top cap picks the Extrude's stable Top-cap `BRepFaceId`.
///
/// Tests 1-8 all exercise fixed-topology Cuboid (or Cuboid-rooted) geometry.
/// This smoke proves `CadProjection::pick_face` flows the variable-N
/// `ExtrudeOp` topology consumer through the same picker path: a directly
/// projected `OperatorNode::Extrude` root, a `BRepHandle` owner, and the
/// closest-resolvable selection rule. The ray targets `[0.5, 0.5]` in XY,
/// which is interior to both top-cap fan triangles of the CCW unit-square
/// profile — no cap edge, cap vertex, side wall, or triangle-boundary
/// ambiguity.
#[test]
fn pick_resolves_extrude_top_cap_face() {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    // CCW unit-square profile, matching the existing Extrude projection
    // smokes' convention.
    let profile = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .expect("square profile");
    let extrude = ExtrudeOp::new(profile, 1.0).expect("extrude");
    let extrude_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(extrude))
        .expect("add extrude");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(extrude_node)
        .expect("set root");
    graph.commit("square extrude").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, extrude_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    // Ray straight down through the center of the top cap. The top ring of
    // a length-1.0 extrude sits at z=+1.0; [0.5, 0.5] in XY is interior to
    // both top-cap fan triangles of the CCW unit square.
    let ray = Ray {
        origin: [0.5, 0.5, 5.0],
        direction: [0.0, 0.0, -1.0],
    };
    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("ray must hit the Extrude top cap");

    assert_eq!(
        pick.entity, entity,
        "the directly projected Extrude-root entity must be picked"
    );
    assert_eq!(pick.owner, ENTITY_OWNER);
    assert_eq!(
        pick.face_id,
        BRepFaceId::for_extrude_face(ENTITY_OWNER, ExtrudeFaceTag::Top),
        "the picked face_id must be the Extrude Top-cap's stable identity"
    );
    assert!(pick.t > 0.0, "ray-t must be strictly positive");
    // Top cap at z=+1.0; ray origin at z=+5 along -Z hits at t = 4.0.
    assert!(
        (pick.t - 4.0).abs() < 1e-4,
        "ray-t at the Top cap (z=1.0) is expected at 4.0, got {}",
        pick.t
    );
}

/// **Test 10** — direct partial-mode square-ring `RevolveOp` root: a ray
/// through the interior of the start cap picks the Revolve's stable
/// `StartCap` `BRepFaceId`.
///
/// Test 9 proves `pick_face` flows the variable-N `ExtrudeOp` consumer
/// through the picker. This smoke is the mode-driven-topology sibling:
/// `RevolveOp` is the only operator whose face count and label repetition
/// both change with a Full/Partial mode flip. The fixture is the labeled
/// partial-mode square-ring Revolve used by the Revolve projection smokes —
/// `Polygon2D::new([[1,0],[2,0],[2,1],[1,1]])`, `segments = 8`, `angle = π`
/// — projecting to 68 triangles (`2*n*segments + 2*(n-2) = 64 + 4`).
///
/// In Partial mode the start cap is a fan-triangulated copy of the profile
/// lying in the `z = 0` plane. The ray origin `[1.25, 0.5, -5.0]` along
/// `+Z` targets `[1.25, 0.5]` in XY: interior to the `[1,2]×[0,1]` square
/// profile, off the cap boundary, off every cap vertex, and above the fan
/// diagonal from `[1,0]` to `[2,1]` (`y = x - 1` ⇒ `0.5 > 0.25`). The
/// picker may see farther intersections deeper in the swept solid; the
/// assertion is that the closest resolvable face is the start cap, hit at
/// `z = 0` ⇒ `t = 5.0`.
#[test]
fn pick_resolves_revolve_start_cap_face() {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    // Square-ring profile + segments=8 + angle=π — the labeled partial-mode
    // square-ring fixture shared with the Revolve projection smokes.
    let profile = Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]])
        .expect("square-ring profile");
    let revolve = RevolveOp::partial(profile, 8, PI).expect("revolve partial");
    let revolve_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Revolve(revolve))
        .expect("add revolve");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(revolve_node)
        .expect("set root");
    graph.commit("partial square-ring revolve").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, revolve_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    // Sanity-check the fixture is the labeled partial-mode square-ring case:
    // 2*n*segments + 2*(n-2) = 2*4*8 + 2*2 = 68 triangles, all labeled.
    let mesh = projection.projected_mesh(entity).expect("mesh");
    assert_eq!(
        mesh.triangle_count(),
        68,
        "partial-mode square-ring Revolve (n=4, segments=8) projects to 68 triangles"
    );
    assert!(
        mesh.face_labels.is_some(),
        "the partial-mode Revolve projection must carry face_labels"
    );

    // Ray along +Z through the start-cap interior at z=0. [1.25, 0.5] is
    // interior to the [1,2]×[0,1] square profile, off the cap boundary,
    // cap vertices, and the fan diagonal y = x - 1.
    let ray = Ray {
        origin: [1.25, 0.5, -5.0],
        direction: [0.0, 0.0, 1.0],
    };
    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("ray must hit the Revolve start cap");

    assert_eq!(
        pick.entity, entity,
        "the directly projected Revolve-root entity must be picked"
    );
    assert_eq!(pick.owner, ENTITY_OWNER);
    assert_eq!(
        pick.face_id,
        BRepFaceId::for_revolve_face(ENTITY_OWNER, RevolveFaceTag::StartCap { profile_count: 4 }),
        "the picked face_id must be the Revolve StartCap's stable identity"
    );
    assert!(pick.t > 0.0, "ray-t must be strictly positive");
    // Start cap lies in the z=0 plane; ray origin at z=-5 along +Z hits it
    // at t = 5.0.
    assert!(
        (pick.t - 5.0).abs() < 1e-4,
        "ray-t at the start cap (z=0) is expected at 5.0, got {}",
        pick.t
    );
}

/// **Test 11** — direct square-to-larger-square `LoftOp` root: a ray from
/// above the top cap interior picks the Loft's stable `Top`-cap `BRepFaceId`.
///
/// Test 9 proves `pick_face` flows the variable-N `ExtrudeOp` consumer
/// through the picker; this smoke is the two-profile sibling. `LoftOp`
/// sweeps `profile_a` at `z = 0` to `profile_b` at `z = length`, fan-
/// triangulating each cap from vertex 0. The fixture mirrors the direct Loft
/// setup in `loft_brep_face_id_lookup_smoke.rs`: a CCW unit-square bottom
/// profile, a CCW larger-square top profile, and a positive length.
///
/// The top cap is `profile_b` (`[0,3]×[0,3]`) lifted to `z = +1.0`. The ray
/// targets `[2.0, 1.0]` in XY: interior to the larger square, and — since
/// the top cap fans from vertex `[0,0]` across the diagonal `y = x` — strictly
/// inside the lower-right fan triangle (`[0,0], [3,0], [3,3]`) because
/// `1.0 < 2.0`. That point is outside the `[0,1]×[0,1]` bottom cap, so the
/// only resolvable cap hit along the ray is the Top cap at `z = +1.0`.
#[test]
fn pick_resolves_loft_top_cap_face() {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    // CCW unit-square bottom profile and CCW larger-square top profile,
    // matching the direct Loft fixture in loft_brep_face_id_lookup_smoke.rs.
    let profile_a = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        .expect("square bottom profile");
    let profile_b = Polygon2D::new(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]])
        .expect("larger square top profile");
    let loft = LoftOp::new(profile_a, profile_b, 1.0).expect("loft");
    let loft_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Loft(loft))
        .expect("add loft");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(loft_node)
        .expect("set root");
    graph
        .commit("square-to-larger-square loft")
        .expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, loft_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    // Ray straight down through the top-cap interior. The top ring of a
    // length-1.0 Loft sits at z=+1.0; [2.0, 1.0] in XY is interior to the
    // larger square and strictly inside the lower-right top-cap fan triangle.
    let ray = Ray {
        origin: [2.0, 1.0, 5.0],
        direction: [0.0, 0.0, -1.0],
    };
    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("ray must hit the Loft top cap");

    assert_eq!(
        pick.entity, entity,
        "the directly projected Loft-root entity must be picked"
    );
    assert_eq!(pick.owner, ENTITY_OWNER);
    assert_eq!(
        pick.face_id,
        BRepFaceId::for_loft_face(ENTITY_OWNER, LoftFaceTag::Top),
        "the picked face_id must be the Loft Top-cap's stable identity"
    );
    assert!(pick.t > 0.0, "ray-t must be strictly positive");
    // Top cap at z=+1.0; ray origin at z=+5 along -Z hits at t = 4.0.
    assert!(
        (pick.t - 4.0).abs() < 1e-4,
        "ray-t at the Top cap (z=1.0) is expected at 4.0, got {}",
        pick.t
    );
}
