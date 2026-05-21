//! `rge-editor` — main editor binary.
//!
//! # Sub-δ.1.B / dispatch G modes
//!
//! Two construction paths, mutually exclusive at startup:
//!
//! - **Default (no CLI flag)** — build a single `CuboidOp(1.0, 1.0, 1.0)`
//!   through the CAD pipeline (`CadGraph` + `CadProjection` + ECS world),
//!   hand to [`EditorShell::with_world_projection_graph`]. Behaviour is
//!   byte-identical to the pre-dispatch-G binary.
//! - **`--glb <path>` (dispatch G)** — read a `.glb` file, import via
//!   [`rge_io_gltf::import_glb`], extract the FIRST [`rge_io_gltf::MeshAsset`]
//!   from the resulting scene's first mesh-bearing entity, convert to
//!   [`rge_brep_render::RenderMesh`] via `RenderMesh::from_buffers`, and
//!   hand to [`EditorShell::with_render_mesh`]. The CAD pipeline is
//!   skipped entirely (no graph, no projection, no operator history).
//!
//! Doctrinal note: imported glTF meshes are **render-only**. They are
//! NOT added to the CAD operator graph (no `OperatorNode::ImportedMesh`
//! variant — kittycad governs the canonical IR). Face-pick / save /
//! undo silently no-op on the imported mesh because the existing
//! `EditorShell` defensive guards return early when `projection` is
//! `None`.
//!
//! # v0 scope (dispatches G / I / J)
//!
//! - **All mesh primitives** in the scene are loaded (dispatch I).
//! - **Accumulated transform hierarchy.** Every mesh-bearing entity's
//!   positions are CPU-baked through the product of its parent-chain
//!   TRS before [`rge_brep_render::RenderMesh::from_buffers`] (dispatch
//!   J). Flat normals come out correct because `from_buffers`
//!   recomputes them from the baked positions via cross-product of
//!   CCW winding — including under non-uniform / negative scale.
//! - **No material / texture support.** The render path uses the
//!   editor's hardcoded white-1×1 Lambert+Phong material.
//! - **No animation / skeleton.** Skipped during import; skinning
//!   would require per-frame vertex re-bake.
//! - **No asset-store integration.** Uses io-gltf's in-memory
//!   `MemoryCache` stub for the import lifetime.
//! - **No face-pick on the imported mesh.** `face_labels = None`; no
//!   B-Rep topology; the editor's pick path silently no-ops.
//! - **No save / undo for the imported mesh.** It isn't part of any
//!   operator history.
//!
//! These limits are deliberate — they keep the dispatch bounded while
//! still giving the editor a real external-asset surface. Each can be
//! lifted by a future dispatch without changing this file's structure.

use std::path::PathBuf;
use std::process::ExitCode;

use glam::{Mat4, Quat, Vec3};
use rge_brep_render::RenderMesh;
use rge_cad_core::{BRepOwnerId, CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_editor_shell::EditorShell;
use rge_io_gltf::{import_glb, Cache, Entity, MemoryCache, Scene, Transform};
use rge_kernel_ecs::World;
use winit::event_loop::EventLoop;

/// Deterministic owner seed for the demo cuboid's `BRepHandle`. The
/// 16-byte choice is arbitrary; it just has to be stable so face-ID
/// resolution is reproducible across runs.
const ENTITY_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

// ---------------------------------------------------------------------------
// CLI parsing
// ---------------------------------------------------------------------------

/// Parsed command-line arguments.
///
/// v0 has a single optional flag (`--glb <path>`); future flags slot
/// into this struct additively without touching call sites.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Cli {
    /// Path supplied via `--glb <path>`, or `None` if the flag was
    /// absent. When `Some(path)`, the editor loads the glTF/GLB at
    /// that path as a render-only mesh; when `None`, the editor runs
    /// the default cuboid demo.
    glb_path: Option<PathBuf>,
}

/// Error returned by [`parse_args`] for malformed CLI inputs. The
/// binary surfaces these as a one-line stderr message and exits with
/// status 2 (matches the `tools/architecture-lints` precedent).
#[derive(Debug, Clone, PartialEq, Eq)]
enum CliError {
    /// `--glb` was supplied without a following path argument.
    MissingGlbPath,
    /// An argument was not recognised (e.g. typo, unsupported flag).
    UnknownArg(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::MissingGlbPath => write!(f, "--glb requires a path argument"),
            CliError::UnknownArg(a) => write!(f, "unknown argument: {a}"),
        }
    }
}

/// Pure CLI parser. `args` is `std::env::args().skip(1).collect()` —
/// i.e. argv WITHOUT the binary name at index 0. Public-in-crate so
/// the inline tests at the bottom can exercise the parser without
/// spinning up a winit loop.
///
/// Supported syntax:
///
/// - No args → default cuboid demo (`Cli { glb_path: None }`).
/// - `--glb <path>` → render-only mesh from that path
///   (`Cli { glb_path: Some(path) }`).
///
/// Any other input is a [`CliError`]. We don't accept positional
/// arguments at v0 to keep the future flag set (`--obj`, `--stl`,
/// `--scene`) unambiguous.
fn parse_args(args: &[String]) -> Result<Cli, CliError> {
    let mut cli = Cli::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--glb" => {
                let path = iter.next().ok_or(CliError::MissingGlbPath)?;
                cli.glb_path = Some(PathBuf::from(path));
            }
            other => return Err(CliError::UnknownArg(other.to_string())),
        }
    }
    Ok(cli)
}

// ---------------------------------------------------------------------------
// Dispatch J — glTF TRS hierarchy → CPU-baked world-space positions
// ---------------------------------------------------------------------------

/// Build a column-major [`Mat4`] from a glTF/W17 [`Transform`].
///
/// glTF stores `rotation` as `(x, y, z, w)` (quaternion vector first,
/// scalar last) — `glam::Quat::from_xyzw` uses the same convention, so
/// no element shuffle is needed. Construction order is canonical
/// `T * R * S` (column-major), matching
/// [`glam::Mat4::from_scale_rotation_translation`].
fn trs_to_mat4(t: &Transform) -> Mat4 {
    let translation = Vec3::from_array(t.translation);
    let rotation = Quat::from_xyzw(t.rotation[0], t.rotation[1], t.rotation[2], t.rotation[3]);
    let scale = Vec3::from_array(t.scale);
    Mat4::from_scale_rotation_translation(scale, rotation, translation)
}

/// Resolve an entity's world transform by walking its parent chain up
/// to [`Entity::ROOT`] and folding the local TRS matrices in
/// `topmost * ... * leaf` order.
///
/// Returns [`Mat4::IDENTITY`] for an entity whose parent chain
/// terminates immediately at `ROOT` with an identity local transform.
/// Bounded by `scene.len()` iterations to defend against malformed
/// inputs (the io-gltf importer guarantees a DAG, but we still cap
/// the walk to avoid spinning on a hypothetical cycle).
fn accumulate_world_transform(scene: &Scene, entity: Entity) -> Mat4 {
    let max_depth = scene.len();
    let mut chain: Vec<Mat4> = Vec::with_capacity(8);
    let mut current = entity;
    for _ in 0..=max_depth {
        if current == Entity::ROOT {
            break;
        }
        let Some(comps) = scene.get(current) else {
            break;
        };
        chain.push(trs_to_mat4(&comps.transform));
        if comps.parent == current {
            // Self-loop defense — should never happen but cheap to guard.
            break;
        }
        current = comps.parent;
    }
    // chain[0] is the leaf's local, chain.last() is the topmost ancestor.
    // World = topmost * ... * parent * leaf, applied right-to-left to a
    // model-space vertex. Build by folding from topmost down to leaf with
    // right multiplication.
    let mut world = Mat4::IDENTITY;
    for m in chain.iter().rev() {
        world *= *m;
    }
    world
}

/// Apply `world` to every model-space position, returning a fresh
/// world-space `Vec`.
///
/// Uses [`Mat4::transform_point3`], which extends the input to a
/// 4-vector with `w = 1` so translation is included. Caller-owned
/// allocation — the input slice is left untouched.
fn bake_positions(positions: &[[f32; 3]], world: &Mat4) -> Vec<[f32; 3]> {
    positions
        .iter()
        .map(|p| world.transform_point3(Vec3::from_array(*p)).to_array())
        .collect()
}

// ---------------------------------------------------------------------------
// glTF → Vec<RenderMesh>
// ---------------------------------------------------------------------------

/// Load a glTF/GLB file and convert **every** mesh primitive in its
/// scene to a flat-shaded [`RenderMesh`], returning them in
/// scene-entity order with accumulated TRS hierarchy CPU-baked
/// into vertex positions.
///
/// # Dispatch I — multi-mesh contract
///
/// Imports the file via [`import_glb`] and iterates the resulting
/// [`Scene`]'s entities in id order. Every entity carrying
/// `mesh: Some(MeshHandle)` contributes one [`RenderMesh`] to the
/// returned `Vec`. Pure-transform entities (`mesh: None` — e.g.
/// armature roots, bones) are skipped.
///
/// Multi-primitive meshes are expanded into per-primitive entities
/// at import time by `rge_io_gltf::scene_builder::visit_node` (the
/// parent node entity carries the TRS with `mesh: None`; each
/// primitive child has `Transform::IDENTITY` and `parent =
/// node_entity`). Iterating `Scene.iter()` already yields
/// per-primitive granularity — `accumulate_world_transform` walks
/// up to the parent node to pick up the shared TRS.
///
/// # Dispatch J — world-space baking
///
/// For each mesh-bearing entity, the parent chain is walked up to
/// [`Entity::ROOT`] and the local TRS matrices are folded into a
/// single world matrix via [`accumulate_world_transform`]. The mesh
/// asset's model-space positions are then CPU-baked through that
/// matrix before being handed to [`RenderMesh::from_buffers`], which
/// recomputes flat normals from the baked positions — so non-uniform
/// or negative scale produces correct outward-facing normals
/// automatically.
///
/// `face_labels = None` for every mesh — glTF data has no B-Rep
/// topology, so the editor's face-pick path silently no-ops in
/// render-only mode (the existing `handle_left_click`
/// projection-None guard fires).
///
/// # Order
///
/// Output Vec order matches `Scene.iter()` order, which matches the
/// glTF document's node order (per the `scene_builder` contract).
/// This is the same order the editor's render pass draws meshes in,
/// so what you load is what you render in document order.
///
/// # Errors
///
/// - File-system / parse errors propagate from `import_glb` as a
///   string message.
/// - "no meshes in scene" if no entity carries a mesh handle. Empty
///   `Vec` is NOT returned silently — the binary surfaces this as a
///   hard error so the user knows the file is empty.
/// - "mesh cache lookup failed" if a mesh handle isn't in the cache
///   (would indicate an io-gltf bug; never expected in practice).
fn load_all_glb_meshes(path: &std::path::Path) -> Result<Vec<RenderMesh>, String> {
    let mut cache = MemoryCache::new();
    let scene = import_glb(path, &mut cache).map_err(|e| format!("glTF import: {e}"))?;

    let drawable: Vec<(Entity, rge_io_gltf::MeshHandle)> = scene
        .iter()
        .filter_map(|(e, comps)| comps.mesh.map(|m| (e, m)))
        .collect();
    if drawable.is_empty() {
        return Err(format!(
            "no meshes in glTF scene at {} (file has {} entities, none with a mesh)",
            path.display(),
            scene.len()
        ));
    }

    let mut render_meshes: Vec<RenderMesh> = Vec::with_capacity(drawable.len());
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    for (mesh_index, (entity, handle)) in drawable.iter().enumerate() {
        let mesh_asset = cache
            .get_mesh(handle)
            .ok_or_else(|| "io-gltf returned a mesh handle that isn't in its cache".to_string())?;
        let world = accumulate_world_transform(&scene, *entity);
        let baked = bake_positions(&mesh_asset.positions, &world);
        total_vertices += mesh_asset.vertex_count();
        total_triangles += mesh_asset.triangle_count();

        // The 4th column of a TRS matrix is `(tx, ty, tz, 1)` — useful
        // for runtime smoke verification ("is the cube actually at
        // (1,2,3)?"). Logged per-mesh so a multi-mesh scene shows the
        // distribution at a glance.
        let world_translation = world.w_axis.truncate();
        tracing::info!(
            target: "rge::editor",
            mesh_index,
            entity_id = entity.0,
            world_x = world_translation.x,
            world_y = world_translation.y,
            world_z = world_translation.z,
            vertices = mesh_asset.vertex_count(),
            triangles = mesh_asset.triangle_count(),
            "applied accumulated glTF TRS"
        );

        render_meshes.push(RenderMesh::from_buffers(&baked, &mesh_asset.indices, None));
    }

    tracing::info!(
        target: "rge::editor",
        path = %path.display(),
        scene_entities = scene.len(),
        mesh_count = drawable.len(),
        total_vertices,
        total_triangles,
        "loaded all glTF mesh primitives (render-only, no CAD, world-baked)"
    );

    Ok(render_meshes)
}

// ---------------------------------------------------------------------------
// Cuboid demo (existing — unchanged behaviour)
// ---------------------------------------------------------------------------

/// Build the default-demo `EditorShell` (1×1×1 cuboid through the CAD
/// pipeline). Extracted into a helper so `main` can branch cleanly
/// between this and the `--glb` path.
///
/// Construction sequence (unchanged from the pre-dispatch-G binary):
///
/// 1. Build a `CadGraph` and commit a `CuboidOp(1.0, 1.0, 1.0)` as
///    the root operator.
/// 2. Build a `CadProjection`, register `BRepHandle` as a snapshot
///    component on a fresh `rge_kernel_ecs::World`, and spawn one
///    entity bound to the cuboid node.
/// 3. Mutate the entity's `BRepHandle.brep_owner` to a deterministic
///    16-byte seed so the projection's face IDs are stable.
/// 4. Tick the projection so the cuboid's `ProjectedMesh` lands in
///    the cache.
/// 5. Hand the world / projection / graph triple to
///    [`EditorShell::with_world_projection_graph`].
fn build_cuboid_demo_shell() -> EditorShell {
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

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, cuboid_node)
        .expect("spawn_brep_entity");

    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }

    let tolerance = Tolerance::new(0.001).expect("tolerance");
    projection
        .tick(&mut world, &graph, tolerance)
        .expect("projection tick");

    EditorShell::with_world_projection_graph(world, projection, graph)
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    // Best-effort tracing init. If a global subscriber is already
    // installed (rare for a fresh binary), the call returns Err which
    // we drop deliberately. The clippy `let_underscore_drop` lint
    // forbids the bare `let _` pattern on values that have a
    // destructor; explicit `drop` is the recommended replacement.
    drop(tracing_subscriber::fmt::try_init());

    // ---- Parse CLI ----------------------------------------------------
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let cli = match parse_args(&argv) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("rge-editor: {e}");
            eprintln!("usage: rge-editor [--glb <path>]");
            return ExitCode::from(2);
        }
    };

    // ---- Branch on --glb flag ----------------------------------------
    let mut shell = match cli.glb_path.as_ref() {
        Some(path) => {
            // Dispatch G + I — render-only mesh(es) from glTF.
            let render_meshes = match load_all_glb_meshes(path) {
                Ok(meshes) => meshes,
                Err(e) => {
                    eprintln!("rge-editor: failed to load --glb {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            };
            EditorShell::with_render_meshes(render_meshes)
        }
        None => {
            // Default — cuboid demo (byte-identical to pre-dispatch-G).
            build_cuboid_demo_shell()
        }
    };

    // ---- Run winit event loop -----------------------------------------
    let event_loop = EventLoop::new().expect("event loop");
    if let Err(e) = event_loop.run_app(&mut shell) {
        eprintln!("rge-editor: run_app: {e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// Inline tests — pure CLI parser
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn no_args_returns_default_cli() {
        let cli = parse_args(&[]).expect("parse");
        assert_eq!(cli, Cli::default());
        assert!(cli.glb_path.is_none());
    }

    #[test]
    fn glb_flag_with_path_captures_path() {
        let cli = parse_args(&args(&["--glb", "scene.glb"])).expect("parse");
        assert_eq!(cli.glb_path, Some(PathBuf::from("scene.glb")));
    }

    #[test]
    fn glb_flag_with_absolute_path_captures_path() {
        let cli = parse_args(&args(&["--glb", "A:/assets/cube.glb"])).expect("parse");
        assert_eq!(cli.glb_path, Some(PathBuf::from("A:/assets/cube.glb")));
    }

    #[test]
    fn glb_flag_without_path_returns_missing_glb_path() {
        let err = parse_args(&args(&["--glb"])).expect_err("must error");
        assert_eq!(err, CliError::MissingGlbPath);
    }

    #[test]
    fn unknown_flag_returns_unknown_arg() {
        let err = parse_args(&args(&["--asset", "x.glb"])).expect_err("must error");
        match err {
            CliError::UnknownArg(s) => assert_eq!(s, "--asset"),
            other => panic!("expected UnknownArg(\"--asset\"), got {other:?}"),
        }
    }

    #[test]
    fn positional_arg_is_unknown() {
        // We intentionally don't support a bare positional `<path>`
        // at v0 so future format flags (`--obj`, `--stl`) stay
        // unambiguous. A scene file passed positionally must be
        // wrapped in `--glb <path>` explicitly.
        let err = parse_args(&args(&["scene.glb"])).expect_err("must error");
        match err {
            CliError::UnknownArg(s) => assert_eq!(s, "scene.glb"),
            other => panic!("expected UnknownArg(\"scene.glb\"), got {other:?}"),
        }
    }

    #[test]
    fn cli_error_display_formats_concisely() {
        // The binary prints these to stderr; pin the wording so a
        // future refactor doesn't accidentally change user-visible
        // error output.
        assert_eq!(
            format!("{}", CliError::MissingGlbPath),
            "--glb requires a path argument"
        );
        assert_eq!(
            format!("{}", CliError::UnknownArg("--foo".to_string())),
            "unknown argument: --foo"
        );
    }

    #[test]
    fn parser_consumes_path_argument_after_flag() {
        // Sanity check: the path is consumed as a value, not parsed
        // as a separate arg. A subsequent `--unknown` after a valid
        // `--glb <path>` should be the first thing flagged as
        // unknown, not the path.
        let err = parse_args(&args(&["--glb", "scene.glb", "--unknown"])).expect_err("must error");
        match err {
            CliError::UnknownArg(s) => assert_eq!(s, "--unknown"),
            other => panic!("expected UnknownArg(\"--unknown\"), got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Dispatch J — TRS hierarchy baking
    // -----------------------------------------------------------------------

    use rge_io_gltf::EntityComponents;

    /// Tolerance used by the matrix / position comparisons below. `1e-5` is
    /// the same tolerance the cube round-trip test in io-gltf uses for TRS
    /// preservation; `RenderMesh::from_buffers`'s `DEGENERATE_AREA_EPS` is
    /// `1e-6`, so we stay well above the numerical noise floor.
    const EPS: f32 = 1e-5;

    fn mat_approx_eq(a: Mat4, b: Mat4) -> bool {
        a.to_cols_array()
            .iter()
            .zip(b.to_cols_array().iter())
            .all(|(x, y)| (x - y).abs() < EPS)
    }

    fn vec3_approx_eq(a: Vec3, b: Vec3) -> bool {
        (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS && (a.z - b.z).abs() < EPS
    }

    /// Build a single-entity scene with the given local TRS and parent.
    fn scene_with_one_entity(transform: Transform, parent: Entity) -> Scene {
        let mut scene = Scene::new();
        scene.spawn(EntityComponents {
            name: "e".into(),
            transform,
            parent,
            mesh: None,
            material: None,
            skeleton: None,
        });
        scene
    }

    #[test]
    fn trs_to_mat4_identity_yields_mat4_identity() {
        assert_eq!(trs_to_mat4(&Transform::IDENTITY), Mat4::IDENTITY);
    }

    #[test]
    fn trs_to_mat4_translation_only_matches_mat4_from_translation() {
        let t = Transform {
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        };
        let expected = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        assert!(mat_approx_eq(trs_to_mat4(&t), expected));
    }

    #[test]
    fn trs_to_mat4_scale_only_matches_mat4_from_scale() {
        let t = Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [2.0, 3.0, 4.0],
        };
        let expected = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
        assert!(mat_approx_eq(trs_to_mat4(&t), expected));
    }

    #[test]
    fn trs_to_mat4_rotation_only_matches_mat4_from_quat() {
        // 90° about Y: quat = (0, sin(45°), 0, cos(45°)).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let t = Transform {
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, s, 0.0, s],
            scale: [1.0, 1.0, 1.0],
        };
        let q = Quat::from_xyzw(0.0, s, 0.0, s);
        let expected = Mat4::from_quat(q);
        assert!(mat_approx_eq(trs_to_mat4(&t), expected));
    }

    #[test]
    fn world_transform_single_root_entity_equals_local_trs() {
        let local = Transform {
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        };
        let scene = scene_with_one_entity(local, Entity::ROOT);
        let world = accumulate_world_transform(&scene, Entity(0));
        assert!(mat_approx_eq(world, trs_to_mat4(&local)));
    }

    #[test]
    fn world_transform_two_level_chain_is_parent_times_child() {
        // Parent translates (10, 0, 0); child translates (1, 2, 3).
        // World for child = parent * child, applied to origin = (11, 2, 3).
        let mut scene = Scene::new();
        let parent = scene.spawn(EntityComponents {
            name: "parent".into(),
            transform: Transform {
                translation: [10.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            parent: Entity::ROOT,
            mesh: None,
            material: None,
            skeleton: None,
        });
        let _child = scene.spawn(EntityComponents {
            name: "child".into(),
            transform: Transform {
                translation: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            parent,
            mesh: None,
            material: None,
            skeleton: None,
        });
        let world = accumulate_world_transform(&scene, Entity(1));
        let origin_in_world = world.transform_point3(Vec3::ZERO);
        assert!(vec3_approx_eq(origin_in_world, Vec3::new(11.0, 2.0, 3.0)));
    }

    #[test]
    fn world_transform_three_level_chain_composes_grandparent_parent_child() {
        // grandparent: scale 2 uniformly.
        // parent:     translate (1, 0, 0).
        // child:      rotate 90° about Y.
        //
        // Apply world to vertex (1, 0, 0):
        //   child rotate   (1, 0, 0) -> (0, 0, -1)
        //   parent transl  (0, 0, -1) -> (1, 0, -1)
        //   grandp scale-2 (1, 0, -1) -> (2, 0, -2)
        // Expected final world * (1, 0, 0) = (2, 0, -2).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let mut scene = Scene::new();
        let gp = scene.spawn(EntityComponents {
            name: "gp".into(),
            transform: Transform {
                translation: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [2.0, 2.0, 2.0],
            },
            parent: Entity::ROOT,
            mesh: None,
            material: None,
            skeleton: None,
        });
        let p = scene.spawn(EntityComponents {
            name: "p".into(),
            transform: Transform {
                translation: [1.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            parent: gp,
            mesh: None,
            material: None,
            skeleton: None,
        });
        let _c = scene.spawn(EntityComponents {
            name: "c".into(),
            transform: Transform {
                translation: [0.0, 0.0, 0.0],
                rotation: [0.0, s, 0.0, s],
                scale: [1.0, 1.0, 1.0],
            },
            parent: p,
            mesh: None,
            material: None,
            skeleton: None,
        });
        let world = accumulate_world_transform(&scene, Entity(2));
        let baked = world.transform_point3(Vec3::new(1.0, 0.0, 0.0));
        assert!(
            vec3_approx_eq(baked, Vec3::new(2.0, 0.0, -2.0)),
            "got {baked:?}"
        );
    }

    #[test]
    fn world_transform_multi_primitive_split_inherits_parent_trs() {
        // Mirrors scene_builder's multi-primitive shape: parent node
        // entity carries the TRS (mesh = None); primitive child entity
        // has IDENTITY transform and parent = node_entity. The child's
        // world transform must equal the parent's TRS.
        let mut scene = Scene::new();
        let node = scene.spawn(EntityComponents {
            name: "node".into(),
            transform: Transform {
                translation: [5.0, 6.0, 7.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
            parent: Entity::ROOT,
            mesh: None,
            material: None,
            skeleton: None,
        });
        let _prim = scene.spawn(EntityComponents {
            name: "node#prim0".into(),
            transform: Transform::IDENTITY,
            parent: node,
            mesh: None,
            material: None,
            skeleton: None,
        });
        let world = accumulate_world_transform(&scene, Entity(1));
        let baked = world.transform_point3(Vec3::ZERO);
        assert!(vec3_approx_eq(baked, Vec3::new(5.0, 6.0, 7.0)));
    }

    #[test]
    fn bake_positions_applies_translation_pointwise() {
        let positions = vec![[1.0_f32, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let m = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let baked = bake_positions(&positions, &m);
        assert_eq!(baked.len(), 2);
        assert!(vec3_approx_eq(
            Vec3::from_array(baked[0]),
            Vec3::new(11.0, 22.0, 33.0)
        ));
        assert!(vec3_approx_eq(
            Vec3::from_array(baked[1]),
            Vec3::new(14.0, 25.0, 36.0)
        ));
    }

    #[test]
    fn bake_positions_applies_scale_pointwise() {
        let positions = vec![[1.0_f32, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let m = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
        let baked = bake_positions(&positions, &m);
        assert!(vec3_approx_eq(
            Vec3::from_array(baked[0]),
            Vec3::new(2.0, 6.0, 12.0)
        ));
        assert!(vec3_approx_eq(
            Vec3::from_array(baked[1]),
            Vec3::new(8.0, 15.0, 24.0)
        ));
    }

    #[test]
    fn bake_positions_applies_rotation_pointwise() {
        // 90° about Y: (1, 0, 0) -> (0, 0, -1).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let positions = vec![[1.0_f32, 0.0, 0.0]];
        let m = Mat4::from_quat(Quat::from_xyzw(0.0, s, 0.0, s));
        let baked = bake_positions(&positions, &m);
        assert!(vec3_approx_eq(
            Vec3::from_array(baked[0]),
            Vec3::new(0.0, 0.0, -1.0)
        ));
    }

    #[test]
    fn cube_glb_end_to_end_baked_aabb_translated_by_node_trs() {
        // The cube.glb fixture has translation (1, 2, 3) and a [-0.5, +0.5]^3
        // cube. Post-bake AABB must be [0.5, 1.5] × [1.5, 2.5] × [2.5, 3.5].
        //
        // The fixture is generated lazily by io-gltf's cube_round_trip test
        // on first run. If the file isn't on disk yet, skip — this test
        // exercises file-format compatibility, not the bake math (covered by
        // the per-axis tests above).
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("crates")
            .join("io-gltf")
            .join("tests")
            .join("fixtures")
            .join("cube.glb");
        if !path.exists() {
            eprintln!(
                "SKIP: cube.glb fixture not present at {} (io-gltf tests materialize it on first run)",
                path.display()
            );
            return;
        }

        let meshes = load_all_glb_meshes(&path).expect("load cube.glb");
        assert_eq!(meshes.len(), 1, "cube.glb has exactly one mesh entity");

        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in &meshes[0].positions {
            for i in 0..3 {
                if v[i] < min[i] {
                    min[i] = v[i];
                }
                if v[i] > max[i] {
                    max[i] = v[i];
                }
            }
        }
        // Tolerance 1e-4: round-trip through glTF f32 buffers + matrix
        // multiplication.
        let tol = 1e-4_f32;
        assert!((min[0] - 0.5).abs() < tol, "min.x = {} (want 0.5)", min[0]);
        assert!((max[0] - 1.5).abs() < tol, "max.x = {} (want 1.5)", max[0]);
        assert!((min[1] - 1.5).abs() < tol, "min.y = {} (want 1.5)", min[1]);
        assert!((max[1] - 2.5).abs() < tol, "max.y = {} (want 2.5)", max[1]);
        assert!((min[2] - 2.5).abs() < tol, "min.z = {} (want 2.5)", min[2]);
        assert!((max[2] - 3.5).abs() < tol, "max.z = {} (want 3.5)", max[2]);
    }
}
