// SPLIT-EXEMPTION: cohesive editor-binary entry point. This file holds (a)
// CLI parsing (`Cli`, `CliError`, `parse_args`), (b) the glTF -> editor-shell
// load pipeline (`trs_to_mat4`, `accumulate_world_transform`, `bake_positions`,
// `resolve_base_color`, `load_all_glb_meshes`), (c) the default CAD-cuboid
// demo (`build_cuboid_demo_shell`), (d) `main` dispatching on the CLI, and
// (e) inline `#[cfg(test)]` coverage of every private helper. Splitting would
// either expose the helpers as `pub(crate)` (no other module needs them) or
// fragment a single-binary surface across sibling files for no cognitive
// gain — the dispatch-J / dispatch-K helpers only matter at the
// `load_all_glb_meshes` boundary, which lives here.

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
//! # v0 scope (dispatches G / I / J / K)
//!
//! - **All mesh primitives** in the scene are loaded (dispatch I).
//! - **Accumulated transform hierarchy.** Every mesh-bearing entity's
//!   positions are CPU-baked through the product of its parent-chain
//!   TRS before [`rge_brep_render::RenderMesh::from_buffers`] (dispatch
//!   J). Flat normals come out correct because `from_buffers`
//!   recomputes them from the baked positions via cross-product of
//!   CCW winding — including under non-uniform / negative scale.
//! - **Per-mesh `base_color` from glTF `MaterialAsset`** (dispatch K).
//!   Resolves `EntityComponents.material` through `MemoryCache::
//!   get_material`; entities without a material slot fall back to
//!   `[1.0, 1.0, 1.0, 1.0]`. The render path then constructs one
//!   `rge_gfx::Material` per mesh, all sharing the existing 1×1 white
//!   placeholder texture but each carrying a distinct UBO base_color.
//!   The Lambert+Phong shader is unchanged — only `base_color` flows
//!   through; metallic / roughness / emissive / normal / textures /
//!   alpha modes are all deferred.
//! - **No texture / image / sampler support.** glTF texture indices
//!   are dropped on the floor; io-gltf doesn't extract image bytes
//!   today.
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

/// Default `base_color` for entities whose `comps.material` is
/// `None` — opaque white. Matches `MaterialAsset::default().
/// base_color`, and matches the editor's pre-dispatch-K hardcoded
/// single-Material colour, so a glTF scene with no materials at all
/// renders byte-identically to how it did before dispatch K.
const DEFAULT_BASE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Dispatch M2 — owned RGBA8 texture payload handed from the editor
/// binary into editor-shell. Decoupled from
/// `rge_io_gltf::ImageAsset` so editor-shell stays glTF-agnostic;
/// only `(width, height, pixels)` cross the boundary.
///
/// `pixels.len() == width * height * 4` for the dispatch-M2 v0
/// `Rgba8`-only contract (`Material::new` expects that layout).
#[derive(Debug, Clone, PartialEq, Eq)]
struct TextureInfo {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Resolve the per-entity `base_color` for one [`EntityComponents`].
///
/// If `comps.material` is `Some(handle)` and the cache holds a
/// matching [`rge_io_gltf::MaterialAsset`], the asset's `base_color`
/// is returned. Otherwise the [`DEFAULT_BASE_COLOR`] white is
/// returned — covers both entities that opt out of materials (`mesh:
/// Some, material: None`) and the should-never-happen case of a
/// dangling handle.
fn resolve_base_color(cache: &MemoryCache, comps: &rge_io_gltf::EntityComponents) -> [f32; 4] {
    comps
        .material
        .as_ref()
        .and_then(|h| cache.get_material(h))
        .map_or(DEFAULT_BASE_COLOR, |m| m.base_color)
}

/// Dispatch M2 — resolve the per-entity base-colour texture, if any.
///
/// Walks `comps.material → MaterialAsset.base_color_image_handle →
/// MemoryCache::get_image` and copies the decoded image's
/// `(width, height, pixels)` into an owned [`TextureInfo`]. Returns
/// `None` when:
///
/// - the entity has no `material` slot,
/// - the material has no `base_color_image_handle` (e.g. cube.glb /
///   pbr_material.glb: no extractable image),
/// - the image is in a non-`Rgba8` pixel format (e.g. 16-bit PNG,
///   EXR) — `Material::new` expects RGBA8 bytes only at v0; future
///   dispatches can add format conversion. Logged at WARN so a real
///   asset hitting this path is visible.
///
/// Pixel bytes are cloned into the returned `Vec<u8>` so the cache
/// can be dropped after `load_all_glb_meshes` returns.
fn resolve_base_color_texture(
    cache: &MemoryCache,
    comps: &rge_io_gltf::EntityComponents,
) -> Option<TextureInfo> {
    let mat_handle = comps.material.as_ref()?;
    let mat = cache.get_material(mat_handle)?;
    let img_handle = mat.base_color_image_handle.as_ref()?;
    let img_asset = cache.get_image(img_handle)?;
    if img_asset.pixel_format() != rge_io_gltf::PixelFormat::Rgba8 {
        tracing::warn!(
            target: "rge::editor",
            width = img_asset.width(),
            height = img_asset.height(),
            pixel_format = ?img_asset.pixel_format(),
            "base_color image is not Rgba8; texture skipped (Material::new requires RGBA8 at v0)"
        );
        return None;
    }
    Some(TextureInfo {
        width: img_asset.width(),
        height: img_asset.height(),
        pixels: img_asset.pixels().to_vec(),
    })
}

/// Load a glTF/GLB file and convert **every** mesh primitive in its
/// scene into a (flat-shaded [`RenderMesh`], `base_color`) pair,
/// returned as two parallel `Vec`s in scene-entity order with
/// accumulated TRS hierarchy CPU-baked into vertex positions and
/// per-entity material colours resolved through the import cache.
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
/// # Dispatch K — per-mesh `base_color`
///
/// For each mesh-bearing entity, `comps.material` is resolved
/// through the import [`MemoryCache`] into a [`f32; 4]` `base_color`.
/// Entities without a material handle (or with a dangling handle)
/// fall back to [`DEFAULT_BASE_COLOR`] white. The returned
/// `Vec<[f32; 4]>` has the same length and order as the returned
/// `Vec<RenderMesh>`; the caller (editor-shell) builds one
/// `rge_gfx::Material` per mesh with these colours.
///
/// `face_labels = None` for every mesh — glTF data has no B-Rep
/// topology, so the editor's face-pick path silently no-ops in
/// render-only mode (the existing `handle_left_click`
/// projection-None guard fires).
///
/// # Order
///
/// Output `Vec` order matches `Scene.iter()` order, which matches
/// the glTF document's node order (per the `scene_builder`
/// contract). Both returned vecs are aligned 1:1 — `meshes[i]` and
/// `base_colors[i]` describe the same entity. This is the same
/// order the editor's render pass draws meshes in, so what you load
/// is what you render in document order.
///
/// # Errors
///
/// - File-system / parse errors propagate from `import_glb` as a
///   string message.
/// - "no meshes in scene" if no entity carries a mesh handle. Empty
///   `Vec`s are NOT returned silently — the binary surfaces this as
///   a hard error so the user knows the file is empty.
/// - "mesh cache lookup failed" if a mesh handle isn't in the cache
///   (would indicate an io-gltf bug; never expected in practice).
fn load_all_glb_meshes(
    path: &std::path::Path,
) -> Result<(Vec<RenderMesh>, Vec<[f32; 4]>, Vec<Option<TextureInfo>>), String> {
    let mut cache = MemoryCache::new();
    let scene = import_glb(path, &mut cache).map_err(|e| format!("glTF import: {e}"))?;

    let drawable: Vec<(
        Entity,
        rge_io_gltf::MeshHandle,
        [f32; 4],
        Option<TextureInfo>,
    )> = scene
        .iter()
        .filter_map(|(e, comps)| {
            comps.mesh.map(|m| {
                let bc = resolve_base_color(&cache, comps);
                let tex = resolve_base_color_texture(&cache, comps);
                (e, m, bc, tex)
            })
        })
        .collect();
    if drawable.is_empty() {
        return Err(format!(
            "no meshes in glTF scene at {} (file has {} entities, none with a mesh)",
            path.display(),
            scene.len()
        ));
    }

    let mut render_meshes: Vec<RenderMesh> = Vec::with_capacity(drawable.len());
    let mut base_colors: Vec<[f32; 4]> = Vec::with_capacity(drawable.len());
    let mut base_color_textures: Vec<Option<TextureInfo>> = Vec::with_capacity(drawable.len());
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    for (mesh_index, (entity, handle, base_color, texture)) in drawable.into_iter().enumerate() {
        let mesh_asset = cache
            .get_mesh(&handle)
            .ok_or_else(|| "io-gltf returned a mesh handle that isn't in its cache".to_string())?;
        let world = accumulate_world_transform(&scene, entity);
        let baked = bake_positions(&mesh_asset.positions, &world);
        total_vertices += mesh_asset.vertex_count();
        total_triangles += mesh_asset.triangle_count();

        // The 4th column of a TRS matrix is `(tx, ty, tz, 1)` — useful
        // for runtime smoke verification ("is the cube actually at
        // (1,2,3)?"). Logged per-mesh so a multi-mesh scene shows the
        // distribution at a glance. Dispatch K adds `base_color_r/g/b/a`
        // so the per-mesh tint is visible alongside the per-mesh pose.
        // Dispatch M2 adds `texture_width / texture_height` (0/0 when
        // no texture, real dimensions otherwise) so the runtime smoke
        // shows which meshes picked up real image bytes.
        let world_translation = world.w_axis.truncate();
        let (texture_width, texture_height) = texture
            .as_ref()
            .map_or((0_u32, 0_u32), |t| (t.width, t.height));
        tracing::info!(
            target: "rge::editor",
            mesh_index,
            entity_id = entity.0,
            world_x = world_translation.x,
            world_y = world_translation.y,
            world_z = world_translation.z,
            base_color_r = base_color[0],
            base_color_g = base_color[1],
            base_color_b = base_color[2],
            base_color_a = base_color[3],
            texture_width,
            texture_height,
            vertices = mesh_asset.vertex_count(),
            triangles = mesh_asset.triangle_count(),
            "applied accumulated glTF TRS + base_color + base_color_texture"
        );

        // Dispatch M1 — thread `MeshAsset.texcoords` through to
        // `RenderMesh::from_buffers_with_uvs`. UVs are 2D and unaffected
        // by the dispatch-J world matrix, so they pass through
        // untransformed. Empty texcoords (glTF primitive without
        // TEXCOORD_0) → `None` → output `RenderMesh.texcoords` empty,
        // and gfx's adapter falls back to `[0, 0]` per vertex.
        let uvs: Option<&[[f32; 2]]> = if mesh_asset.texcoords.is_empty() {
            None
        } else {
            Some(&mesh_asset.texcoords)
        };
        render_meshes.push(RenderMesh::from_buffers_with_uvs(
            &baked,
            &mesh_asset.indices,
            None,
            uvs,
        ));
        base_colors.push(base_color);
        base_color_textures.push(texture);
    }

    tracing::info!(
        target: "rge::editor",
        path = %path.display(),
        scene_entities = scene.len(),
        mesh_count = render_meshes.len(),
        total_vertices,
        total_triangles,
        "loaded all glTF mesh primitives (render-only, no CAD, world-baked, per-mesh base_color + base_color_texture)"
    );

    Ok((render_meshes, base_colors, base_color_textures))
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
            // Dispatches G + I + J + K + M1 + M2 — render-only mesh(es)
            // from glTF with TRS hierarchy CPU-baked, per-mesh
            // `base_color` resolved through the import cache, UVs
            // threaded through `RenderMesh.texcoords`, and embedded
            // `base_color_texture` pixels passed in as
            // `Option<(width, height, Vec<u8>)>` per mesh.
            let (render_meshes, base_colors, base_color_textures) = match load_all_glb_meshes(path)
            {
                Ok(triple) => triple,
                Err(e) => {
                    eprintln!("rge-editor: failed to load --glb {}: {e}", path.display());
                    return ExitCode::FAILURE;
                }
            };
            let textures_for_shell: Vec<Option<(u32, u32, Vec<u8>)>> = base_color_textures
                .into_iter()
                .map(|opt| opt.map(|t| (t.width, t.height, t.pixels)))
                .collect();
            EditorShell::with_render_meshes_and_base_colors_and_textures(
                render_meshes,
                base_colors,
                textures_for_shell,
            )
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

        let (meshes, base_colors, _textures) = load_all_glb_meshes(&path).expect("load cube.glb");
        assert_eq!(meshes.len(), 1, "cube.glb has exactly one mesh entity");
        assert_eq!(
            base_colors.len(),
            meshes.len(),
            "base_colors must be aligned 1:1 with meshes"
        );

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

    // -----------------------------------------------------------------------
    // Dispatch K — per-mesh `base_color` resolution
    // -----------------------------------------------------------------------

    fn fixtures_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("crates")
            .join("io-gltf")
            .join("tests")
            .join("fixtures")
    }

    fn skip_if_fixture_missing(path: &std::path::Path) -> bool {
        if !path.exists() {
            eprintln!(
                "SKIP: fixture not present at {} (io-gltf tests materialize it on first run)",
                path.display()
            );
            return true;
        }
        false
    }

    fn base_color_approx_eq(a: [f32; 4], b: [f32; 4]) -> bool {
        let tol = 1e-5_f32;
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < tol)
    }

    #[test]
    fn cube_glb_returns_expected_base_color() {
        // cube.glb fixture sets MaterialAsset.base_color = [0.4, 0.6, 0.8, 1.0]
        // (the make_cube_glb helper in io-gltf/tests/common/mod.rs).
        let path = fixtures_path().join("cube.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }
        let (meshes, base_colors, _textures) = load_all_glb_meshes(&path).expect("load cube.glb");
        assert_eq!(meshes.len(), 1);
        assert_eq!(base_colors.len(), 1);
        assert!(
            base_color_approx_eq(base_colors[0], [0.4, 0.6, 0.8, 1.0]),
            "cube.glb base_color = {:?} (expected [0.4, 0.6, 0.8, 1.0])",
            base_colors[0]
        );
    }

    #[test]
    fn pbr_material_glb_returns_expected_base_color() {
        // pbr_material.glb fixture sets base_color = [0.97, 0.86, 0.32, 1.0]
        // (the make_pbr_material_glb helper).
        let path = fixtures_path().join("pbr_material.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }
        let (_meshes, base_colors, _textures) =
            load_all_glb_meshes(&path).expect("load pbr_material.glb");
        assert_eq!(base_colors.len(), 1);
        assert!(
            base_color_approx_eq(base_colors[0], [0.97, 0.86, 0.32, 1.0]),
            "pbr_material.glb base_color = {:?} (expected [0.97, 0.86, 0.32, 1.0])",
            base_colors[0]
        );
    }

    #[test]
    fn animated_character_glb_returns_skin_tone_base_color() {
        // animated_character.glb fixture sets base_color = [1.0, 0.85, 0.7, 1.0].
        // The fixture has one mesh-bearing entity (the armature) plus two
        // bone-only entities (no mesh); load_all_glb_meshes returns 1 mesh.
        let path = fixtures_path().join("animated_character.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }
        let (meshes, base_colors, _textures) =
            load_all_glb_meshes(&path).expect("load animated_character.glb");
        assert_eq!(
            meshes.len(),
            1,
            "animated_character has 1 mesh-bearing entity (bones are mesh: None)"
        );
        assert!(
            base_color_approx_eq(base_colors[0], [1.0, 0.85, 0.7, 1.0]),
            "animated_character base_color = {:?} (expected [1.0, 0.85, 0.7, 1.0])",
            base_colors[0]
        );
    }

    #[test]
    fn missing_material_falls_back_to_white() {
        // Synthetic scene: one entity with `mesh: Some(_)` but `material:
        // None`. resolve_base_color must return DEFAULT_BASE_COLOR white.
        use rge_io_gltf::{EntityComponents, MeshAsset};

        let mut cache = MemoryCache::new();
        let mh = cache.insert_mesh(MeshAsset {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![],
            texcoords: vec![],
            indices: vec![0, 1, 2],
            material_index: None,
        });
        let comps = EntityComponents {
            name: "no-mat".into(),
            transform: rge_io_gltf::Transform::IDENTITY,
            parent: Entity::ROOT,
            mesh: Some(mh),
            material: None,
            skeleton: None,
        };
        let bc = resolve_base_color(&cache, &comps);
        assert_eq!(bc, DEFAULT_BASE_COLOR);
        assert_eq!(bc, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn material_present_returns_cached_base_color() {
        // Synthetic scene: one entity with `mesh: Some(_)` AND `material:
        // Some(handle)`. resolve_base_color must return the cached
        // MaterialAsset's `base_color`.
        use rge_io_gltf::{EntityComponents, MaterialAsset, MeshAsset};

        let mut cache = MemoryCache::new();
        let mh = cache.insert_mesh(MeshAsset {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![],
            texcoords: vec![],
            indices: vec![0, 1, 2],
            material_index: Some(0),
        });
        let mat_h = cache.insert_material(MaterialAsset {
            name: "red".into(),
            base_color: [0.9, 0.1, 0.1, 1.0],
            ..Default::default()
        });
        let comps = EntityComponents {
            name: "red-tri".into(),
            transform: rge_io_gltf::Transform::IDENTITY,
            parent: Entity::ROOT,
            mesh: Some(mh),
            material: Some(mat_h),
            skeleton: None,
        };
        let bc = resolve_base_color(&cache, &comps);
        assert!(base_color_approx_eq(bc, [0.9, 0.1, 0.1, 1.0]));
    }

    #[test]
    fn multi_material_scene_preserves_entity_order_colors() {
        // Build an in-memory GLB with two mesh-bearing entities carrying
        // distinct materials (red and blue). Verify the returned
        // base_colors Vec preserves the scene-iteration order.
        //
        // glTF nuance: material is per-primitive (i.e. per-glTF-mesh),
        // not per-node. If two entities share the same MeshHandle,
        // io-gltf's exporter writes ONE glTF mesh whose first-seen
        // entity's material is baked in — both entities then re-import
        // with the same material. To test per-entity tinting we must
        // give each entity a DISTINCT mesh asset (different content
        // hash). We perturb a single vertex by 1e-3 — visually
        // identical, structurally distinct.
        use rge_io_gltf::{export_glb, EntityComponents, MaterialAsset, MeshAsset, Scene};

        let mut cache = MemoryCache::new();
        let tri_a = MeshAsset {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![],
            texcoords: vec![],
            indices: vec![0, 1, 2],
            material_index: None,
        };
        let tri_b = MeshAsset {
            positions: vec![[0.0, 0.0, 0.0], [1.001, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![],
            texcoords: vec![],
            indices: vec![0, 1, 2],
            material_index: None,
        };
        let mh_a = cache.insert_mesh(tri_a);
        let mh_b = cache.insert_mesh(tri_b);
        let red = cache.insert_material(MaterialAsset {
            name: "red".into(),
            base_color: [0.9, 0.1, 0.1, 1.0],
            ..Default::default()
        });
        let blue = cache.insert_material(MaterialAsset {
            name: "blue".into(),
            base_color: [0.1, 0.2, 0.9, 1.0],
            ..Default::default()
        });

        let mut scene = Scene::new();
        scene.spawn(EntityComponents {
            name: "red_tri".into(),
            transform: rge_io_gltf::Transform::IDENTITY,
            parent: Entity::ROOT,
            mesh: Some(mh_a),
            material: Some(red),
            skeleton: None,
        });
        scene.spawn(EntityComponents {
            name: "blue_tri".into(),
            transform: rge_io_gltf::Transform::IDENTITY,
            parent: Entity::ROOT,
            mesh: Some(mh_b),
            material: Some(blue),
            skeleton: None,
        });

        let bytes = export_glb(&scene, &cache).expect("export");
        let path = std::env::temp_dir().join("rge_editor_test_multi_mat.glb");
        std::fs::write(&path, &bytes).expect("write");

        let (meshes, base_colors, _textures) = load_all_glb_meshes(&path).expect("load multi-mat");
        assert_eq!(meshes.len(), 2);
        assert_eq!(base_colors.len(), 2);
        assert!(
            base_color_approx_eq(base_colors[0], [0.9, 0.1, 0.1, 1.0]),
            "base_colors[0] = {:?}",
            base_colors[0]
        );
        assert!(
            base_color_approx_eq(base_colors[1], [0.1, 0.2, 0.9, 1.0]),
            "base_colors[1] = {:?}",
            base_colors[1]
        );

        drop(std::fs::remove_file(&path));
    }

    // -----------------------------------------------------------------------
    // Dispatch M1 — UV propagation tests
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Dispatch M2 — per-mesh `base_color_texture` propagation tests
    // -----------------------------------------------------------------------

    #[test]
    fn load_all_glb_meshes_propagates_texture_when_present() {
        // textured_uv_cube.glb has an embedded 4×4 PNG referenced by
        // material.base_color_texture. After load, the texture
        // payload must be `Some` with width = height = 4 and 64
        // bytes of RGBA8 pixels.
        let path = fixtures_path().join("textured_uv_cube.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }
        let (meshes, base_colors, textures) =
            load_all_glb_meshes(&path).expect("load textured_uv_cube.glb");
        assert_eq!(meshes.len(), 1);
        assert_eq!(base_colors.len(), 1);
        assert_eq!(textures.len(), 1);
        let tex = textures[0]
            .as_ref()
            .expect("textured_uv_cube carries a base_color_texture");
        assert_eq!(tex.width, 4);
        assert_eq!(tex.height, 4);
        assert_eq!(tex.pixels.len(), 4 * 4 * 4, "RGBA8: w*h*4 bytes");
    }

    #[test]
    fn load_all_glb_meshes_returns_none_texture_when_absent() {
        // cube.glb has NO base_color_texture; the resulting texture
        // slot must be `None`.
        let path = fixtures_path().join("cube.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }
        let (_meshes, _base_colors, textures) = load_all_glb_meshes(&path).expect("load cube.glb");
        assert_eq!(textures.len(), 1);
        assert!(
            textures[0].is_none(),
            "cube.glb has no base_color_texture; got {:?}",
            textures[0].as_ref().map(|t| (t.width, t.height))
        );
    }

    #[test]
    fn load_all_glb_meshes_textured_uv_cube_keeps_non_empty_texcoords() {
        // Sanity: textured_uv_cube combines UV propagation (M1) +
        // texture pixels (M2). Both must survive.
        let path = fixtures_path().join("textured_uv_cube.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }
        let (meshes, _, _) = load_all_glb_meshes(&path).expect("load");
        assert_eq!(meshes.len(), 1);
        assert_eq!(
            meshes[0].texcoords.len(),
            meshes[0].positions.len(),
            "M1 UV propagation still in effect"
        );
        assert_eq!(meshes[0].texcoords.len(), 36, "12 input tris × 3");
    }

    #[test]
    fn load_all_glb_meshes_returns_three_aligned_vecs() {
        // Defensive: regardless of which fixture loads, the three
        // parallel vecs returned by load_all_glb_meshes must have
        // equal lengths. Walks every available fixture.
        for name in [
            "cube.glb",
            "pbr_material.glb",
            "animated_character.glb",
            "uv_cube.glb",
            "textured_uv_cube.glb",
        ] {
            let path = fixtures_path().join(name);
            if skip_if_fixture_missing(&path) {
                continue;
            }
            let (meshes, base_colors, textures) = load_all_glb_meshes(&path).expect("load fixture");
            assert_eq!(
                meshes.len(),
                base_colors.len(),
                "{name}: meshes.len()={} != base_colors.len()={}",
                meshes.len(),
                base_colors.len()
            );
            assert_eq!(
                meshes.len(),
                textures.len(),
                "{name}: meshes.len()={} != textures.len()={}",
                meshes.len(),
                textures.len()
            );
        }
    }

    #[test]
    fn load_all_glb_meshes_propagates_uvs_when_present() {
        // The uv_cube.glb fixture (dispatch M1) carries
        // TEXCOORD_0; `load_all_glb_meshes` must thread those UVs
        // through to `RenderMesh::texcoords`. Vertex-tripled output:
        // 12 input tris × 3 = 36 UVs.
        let path = fixtures_path().join("uv_cube.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }
        let (meshes, _base_colors, _textures) =
            load_all_glb_meshes(&path).expect("load uv_cube.glb");
        assert_eq!(meshes.len(), 1);
        assert_eq!(
            meshes[0].texcoords.len(),
            meshes[0].positions.len(),
            "texcoords aligned 1:1 with positions after vertex tripling"
        );
        assert_eq!(
            meshes[0].texcoords.len(),
            36,
            "12 input tris × 3 output verts = 36 UVs"
        );
        // Spot-check: the first triangle's first vertex must carry
        // the first face's (0, 0) corner UV. Cube_mesh in io-gltf's
        // `tests/common/mod.rs` lays out per-face UVs as
        // `(0,0) → (1,0) → (1,1) → (0,1)`, and the first input tri
        // covers (vert0, vert1, vert2).
        assert_eq!(meshes[0].texcoords[0], [0.0, 0.0]);
        assert_eq!(meshes[0].texcoords[1], [1.0, 0.0]);
        assert_eq!(meshes[0].texcoords[2], [1.0, 1.0]);
    }

    #[test]
    fn load_all_glb_meshes_leaves_texcoords_empty_when_absent() {
        // cube.glb has NO TEXCOORD_0; the resulting RenderMesh's
        // `texcoords` field must be empty. The gfx adapter then
        // falls back to `[0, 0]` per vertex.
        let path = fixtures_path().join("cube.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }
        let (meshes, _, _) = load_all_glb_meshes(&path).expect("load cube.glb");
        assert_eq!(meshes.len(), 1);
        assert!(
            meshes[0].texcoords.is_empty(),
            "cube.glb has no UVs; got {} texcoords",
            meshes[0].texcoords.len()
        );
    }

    #[test]
    fn meshes_and_base_colors_have_matching_lengths() {
        // Defensive smoke: regardless of which fixture loads, the parallel
        // Vec invariant must hold. Runs against every available fixture.
        for name in ["cube.glb", "pbr_material.glb", "animated_character.glb"] {
            let path = fixtures_path().join(name);
            if skip_if_fixture_missing(&path) {
                continue;
            }
            let (meshes, base_colors, _textures) =
                load_all_glb_meshes(&path).expect("load fixture");
            assert_eq!(
                meshes.len(),
                base_colors.len(),
                "{name}: meshes.len()={} != base_colors.len()={}",
                meshes.len(),
                base_colors.len()
            );
        }
    }
}
