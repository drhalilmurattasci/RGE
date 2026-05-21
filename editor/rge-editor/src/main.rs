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
//! # v0 scope (dispatch G)
//!
//! - **First mesh primitive ONLY.** Multi-mesh / multi-primitive glTF
//!   files are loaded as their FIRST `MeshAsset` only. Multi-mesh
//!   support is a separate future dispatch.
//! - **No material / texture support.** The render path uses the
//!   editor's hardcoded white-1×1 Lambert+Phong material.
//! - **No transform tree.** The mesh renders at the local origin
//!   regardless of the glTF node's TRS.
//! - **No animation / skeleton.** Skipped during import.
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

use rge_brep_render::RenderMesh;
use rge_cad_core::{BRepOwnerId, CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_editor_shell::EditorShell;
use rge_io_gltf::{import_glb, Cache, MemoryCache};
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
// glTF → RenderMesh
// ---------------------------------------------------------------------------

/// Load a glTF/GLB file and convert its FIRST mesh primitive into a
/// flat-shaded [`RenderMesh`].
///
/// # v0 single-mesh contract
///
/// The function imports the file via [`import_glb`], then iterates
/// the resulting [`rge_io_gltf::Scene`]'s entities in order and picks
/// the FIRST entity whose `mesh: Option<MeshHandle>` is `Some`. The
/// corresponding [`rge_io_gltf::MeshAsset`] in the cache is converted
/// via [`RenderMesh::from_buffers`] using its `positions` + `indices`
/// (face_labels = None — glTF meshes have no B-Rep topology, so the
/// editor's face-pick path will silently no-op on this mesh).
///
/// Multi-primitive / multi-entity scenes are partially ignored at v0:
/// only the first one renders. A future "load all meshes" dispatch
/// will iterate the full scene.
///
/// # Errors
///
/// - File-system / parse errors propagate from `import_glb` as a
///   string message (the binary prints them to stderr and exits 1).
/// - "no meshes in scene" if no entity in the imported scene carries
///   a mesh handle.
/// - "mesh cache lookup failed" if the import populated a handle that
///   doesn't resolve in the cache (would indicate an io-gltf bug;
///   never expected in practice).
fn load_first_glb_mesh(path: &std::path::Path) -> Result<RenderMesh, String> {
    let mut cache = MemoryCache::new();
    let scene = import_glb(path, &mut cache).map_err(|e| format!("glTF import: {e}"))?;

    // Find the first mesh-bearing entity in the scene. `scene.iter`
    // yields `(Entity, &EntityComponents)` in entity-id order which
    // matches the glTF document's node order per the io-gltf
    // scene_builder contract.
    let mesh_handle = scene
        .iter()
        .find_map(|(_, comps)| comps.mesh)
        .ok_or_else(|| {
            format!(
                "no meshes in glTF scene at {} (file has {} entities, none with a mesh)",
                path.display(),
                scene.len()
            )
        })?;

    let mesh_asset = cache
        .get_mesh(&mesh_handle)
        .ok_or_else(|| "io-gltf returned a mesh handle that isn't in its cache".to_string())?;

    tracing::info!(
        target: "rge::editor",
        path = %path.display(),
        scene_entities = scene.len(),
        mesh_vertex_count = mesh_asset.vertex_count(),
        mesh_triangle_count = mesh_asset.triangle_count(),
        "loaded first glTF mesh primitive (render-only, no CAD)"
    );

    // `face_labels = None` — glTF meshes have no B-Rep topology, so
    // we don't lie to the editor by inventing labels. The pick path
    // silently no-ops in render-only mode (see `with_render_mesh`
    // doc + the existing `handle_left_click` projection-None guard).
    Ok(RenderMesh::from_buffers(
        &mesh_asset.positions,
        &mesh_asset.indices,
        None,
    ))
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
            // Dispatch G — render-only mesh from glTF.
            let render_mesh = match load_first_glb_mesh(path) {
                Ok(mesh) => mesh,
                Err(e) => {
                    eprintln!("rge-editor: failed to load --glb {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            };
            EditorShell::with_render_mesh(render_mesh)
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
}
