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
//! # Sub-δ.1.B / dispatch G / ISSUE-225 modes
//!
//! Three construction paths, mutually exclusive at startup:
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
//! - **`--scene <path>` (ISSUE-225)** — load a `.rge-project` (its
//!   first scene resolved relative to the manifest dir) or a
//!   `.rge-scene` into an ECS world via
//!   [`rge_scene_loader::load_scene_world_from_path`]. Hand the resulting
//!   world to [`EditorShell::with_world`]. This is a load-only path
//!   with no CAD projection or operator graph; the simple-scene golden
//!   fixture has no `BRepHandle`, so the editor reaches the event loop
//!   without a visible render (constructor-level / headless validation
//!   only, per the BASELINE PREFLIGHT for this integration). Mutually
//!   exclusive with `--glb`.
//!
//! Doctrinal note: imported glTF meshes are **render-only**. They are
//! NOT added to the CAD operator graph (no `OperatorNode::ImportedMesh`
//! variant — kittycad governs the canonical IR). Face-pick / save /
//! undo silently no-op on the imported mesh because the existing
//! `EditorShell` defensive guards return early when `projection` is
//! `None`.
//!
//! # v0 scope (dispatches G / I / J / K / M1 / M2 / M3)
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

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use glam::{Mat4, Quat, Vec3};
use rge_brep_render::RenderMesh;
use rge_cad_core::{BRepOwnerId, CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_editor_shell::{
    AssetReloadHook, EditorShell, GlbOpenDialog, SceneOpenHook, SceneSaveDialog, SceneSaveHook,
};
use rge_io_gltf::{import_glb, Cache, Entity, MemoryCache, Scene, Transform};
use rge_kernel_ecs::World;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

mod glb_watcher;
use glb_watcher::GlbWatcher;

/// Deterministic owner seed for the demo cuboid's `BRepHandle`. The
/// 16-byte choice is arbitrary; it just has to be stable so face-ID
/// resolution is reproducible across runs.
const ENTITY_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

// ---------------------------------------------------------------------------
// CLI parsing
// ---------------------------------------------------------------------------

/// Parsed command-line arguments.
///
/// Two optional, mutually-exclusive load flags (`--glb <path>` and
/// `--scene <path>`); future flags slot into this struct additively
/// without touching call sites.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Cli {
    /// Path supplied via `--glb <path>`, or `None` if the flag was
    /// absent. When `Some(path)`, the editor loads the glTF/GLB at
    /// that path as a render-only mesh; when `None` (and `scene_path`
    /// is also `None`), the editor runs the default cuboid demo.
    glb_path: Option<PathBuf>,
    /// Path supplied via `--scene <path>`, or `None` if the flag was
    /// absent. When `Some(path)`, the editor loads the file — a
    /// `.rge-project` (first scene resolved) or a `.rge-scene` — into an
    /// ECS world via [`rge_scene_loader::load_scene_world_from_path`] and
    /// constructs an [`EditorShell::with_world`] from it. Mutually
    /// exclusive with `glb_path` (validated by `parse_args`).
    scene_path: Option<PathBuf>,
}

/// Error returned by [`parse_args`] for malformed CLI inputs. The
/// binary surfaces these as a one-line stderr message and exits with
/// status 2 (matches the `tools/architecture-lints` precedent).
#[derive(Debug, Clone, PartialEq, Eq)]
enum CliError {
    /// `--glb` was supplied without a following path argument.
    MissingGlbPath,
    /// `--scene` was supplied without a following path argument.
    MissingScenePath,
    /// `--glb` and `--scene` were both supplied. The two load paths
    /// are mutually exclusive: `--glb` is render-only mesh ingestion
    /// without an ECS world, `--scene` is ECS-world scene-load
    /// without render-mesh ingestion.
    GlbAndSceneConflict,
    /// An argument was not recognised (e.g. typo, unsupported flag).
    UnknownArg(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::MissingGlbPath => write!(f, "--glb requires a path argument"),
            CliError::MissingScenePath => write!(f, "--scene requires a path argument"),
            CliError::GlbAndSceneConflict => {
                write!(f, "--glb and --scene are mutually exclusive")
            }
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
/// - No args → default cuboid demo (`Cli { glb_path: None, scene_path:
///   None }`).
/// - `--glb <path>` → render-only mesh from that path
///   (`Cli { glb_path: Some(path), scene_path: None }`).
/// - `--scene <path>` → load `.rge-project` or `.rge-scene` into an
///   ECS world (`Cli { glb_path: None, scene_path: Some(path) }`).
///
/// `--glb` and `--scene` are mutually exclusive and yield
/// [`CliError::GlbAndSceneConflict`] when both appear (in either
/// order). Any other input is a [`CliError`]. We don't accept
/// positional arguments at v0 to keep the future flag set (`--obj`,
/// `--stl`) unambiguous.
fn parse_args(args: &[String]) -> Result<Cli, CliError> {
    let mut cli = Cli::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--glb" => {
                let path = iter.next().ok_or(CliError::MissingGlbPath)?;
                if cli.scene_path.is_some() {
                    return Err(CliError::GlbAndSceneConflict);
                }
                cli.glb_path = Some(PathBuf::from(path));
            }
            "--scene" => {
                let path = iter.next().ok_or(CliError::MissingScenePath)?;
                if cli.glb_path.is_some() {
                    return Err(CliError::GlbAndSceneConflict);
                }
                cli.scene_path = Some(PathBuf::from(path));
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

/// Dispatch M3 — apply the inverse-transpose of `world` to every
/// model-space normal, re-normalizing the result. Returns a fresh
/// world-space `Vec` aligned 1:1 with the input slice.
///
/// The inverse-transpose is the standard normal-correction formula
/// — it preserves perpendicularity under non-uniform scale, flips
/// direction under negative-determinant (mirror) scale (which is
/// the geometrically-correct response for surface orientation),
/// and degrades gracefully under zero-determinant inputs (glam's
/// `inverse()` returns a non-finite matrix; the resulting normals
/// would be NaN — gates against that defensively below).
///
/// Transforms via `vec4(n, 0.0)` so translation is excluded
/// (`Mat4::transform_vector3` does this by zeroing the w
/// component). The result is re-normalized per vertex because:
///
/// 1. Non-uniform scale yields non-unit-length output even from
///    unit-length input.
/// 2. The fragment shader normalizes interpolated normals; passing
///    unit-length per-vertex normals minimizes interpolation drift.
///
/// Falls back to the dispatch-pre-M3 behaviour (caller-pass `None`
/// to [`rge_brep_render::RenderMesh::from_buffers_with_attributes`])
/// when:
///
/// - The input `normals` slice is empty (glTF primitive had no
///   `NORMAL` accessor).
/// - The world matrix is singular (`determinant().abs() < EPS`) —
///   inverse-transpose would produce NaN normals; brep-render
///   recomputes cross-product flat normals from the (also-baked)
///   positions, which stays finite.
fn bake_normals(normals: &[[f32; 3]], world: &Mat4) -> Vec<[f32; 3]> {
    // Inverse-transpose of `world`. The fragment shader normalizes
    // interpolated values, but we pre-normalize the per-vertex
    // result for accuracy under non-uniform scale.
    let normal_matrix = world.inverse().transpose();
    normals
        .iter()
        .map(|n| {
            let world_n = normal_matrix.transform_vector3(Vec3::from_array(*n));
            // glam's `normalize_or_zero` clamps NaN / zero-length
            // inputs to `[0, 0, 0]` rather than propagating NaN
            // into the GPU buffer. Fragment shader's `normalize(in.
            // world_normal)` is the second line of defense.
            world_n.normalize_or_zero().to_array()
        })
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
        // shows which meshes picked up real image bytes. Dispatch M3
        // adds `normal_count` (0 = recompute flat from positions,
        // otherwise = number of imported NORMAL accessor entries) so
        // smooth-vs-flat shading is visible at a glance.
        let world_translation = world.w_axis.truncate();
        let (texture_width, texture_height) = texture
            .as_ref()
            .map_or((0_u32, 0_u32), |t| (t.width, t.height));
        let normal_count = mesh_asset.normals.len();
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
            normal_count,
            "applied accumulated glTF TRS + base_color + base_color_texture + normals"
        );

        // Dispatch M1 — thread `MeshAsset.texcoords` through to
        // `RenderMesh::from_buffers_with_attributes`. UVs are 2D and
        // unaffected by the dispatch-J world matrix, so they pass
        // through untransformed. Empty texcoords (glTF primitive
        // without TEXCOORD_0) → `None` → output `RenderMesh.
        // texcoords` empty, and gfx's adapter falls back to `[0, 0]`
        // per vertex.
        let uvs: Option<&[[f32; 2]]> = if mesh_asset.texcoords.is_empty() {
            None
        } else {
            Some(&mesh_asset.texcoords)
        };

        // Dispatch M3 — CPU-bake input normals through the inverse-
        // transpose of the accumulated world matrix and thread them
        // into `RenderMesh::from_buffers_with_attributes` as
        // `Some(&baked_normals)`. When the glTF primitive lacks
        // NORMAL, pass `None` and let brep-render recompute flat
        // face normals from the (also-world-baked) positions.
        //
        // Defensive: if the world matrix is singular (degenerate
        // scale), `bake_normals` still returns finite values via
        // `normalize_or_zero`, but the resulting normals are
        // meaningless. Fall back to `None` so brep-render
        // recomputes from positions instead.
        let baked_normals: Option<Vec<[f32; 3]>> = if mesh_asset.normals.is_empty() {
            None
        } else if world.determinant().abs() < 1e-7 {
            // Degenerate world matrix — input normals can't be
            // meaningfully transformed. Let brep-render recompute
            // from the baked positions (whose flat normal stays
            // finite under degenerate scale).
            None
        } else {
            Some(bake_normals(&mesh_asset.normals, &world))
        };

        render_meshes.push(RenderMesh::from_buffers_with_attributes(
            &baked,
            &mesh_asset.indices,
            None,
            baked_normals.as_deref(),
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
        "loaded all glTF mesh primitives (render-only, no CAD, world-baked, per-mesh base_color + base_color_texture + imported normals)"
    );

    Ok((render_meshes, base_colors, base_color_textures))
}

// ---------------------------------------------------------------------------
// Asset hot-reload — R-key hook (loader bridge for editor-shell)
// ---------------------------------------------------------------------------

/// Stateless [`AssetReloadHook`] impl that re-runs [`load_all_glb_meshes`]
/// on the editor's `--glb` source path and adapts the resulting
/// `Vec<Option<TextureInfo>>` to the editor-shell's `Vec<Option<(u32,
/// u32, Vec<u8>)>>` shape.
///
/// Doctrinal note: this struct owns the loader edge so editor-shell
/// stays free of an `rge-io-gltf` dep. The hook is a unit struct
/// because v0's loader is stateless — every reload re-imports through
/// a fresh `MemoryCache` (the cache is owned inside `load_all_glb_meshes`
/// and dropped on return). A future per-path parse cache would
/// promote this to a stateful struct + `&mut self` on the trait
/// without churning the editor-shell side.
struct GlbLoaderHook;

impl AssetReloadHook for GlbLoaderHook {
    fn reload_glb(
        &self,
        path: &std::path::Path,
    ) -> Result<
        (
            Vec<RenderMesh>,
            Vec<[f32; 4]>,
            Vec<Option<(u32, u32, Vec<u8>)>>,
        ),
        String,
    > {
        let (render_meshes, base_colors, base_color_textures) = load_all_glb_meshes(path)?;
        let textures: Vec<Option<(u32, u32, Vec<u8>)>> = base_color_textures
            .into_iter()
            .map(|opt| opt.map(|t| (t.width, t.height, t.pixels)))
            .collect();
        Ok((render_meshes, base_colors, textures))
    }
}

// ---------------------------------------------------------------------------
// ISSUE-258 / SCENE-OPEN-WIRING — in-app "Open" dialog hook
// ---------------------------------------------------------------------------

/// Binary-owned [`GlbOpenDialog`] impl backed by `rfd`'s native file
/// dialog. Handed to [`EditorShell`] via
/// [`EditorShell::with_glb_open_dialog`] in every launch mode so
/// `Ctrl+O` works from the default cuboid demo as well as `--glb` /
/// `--scene`. Offers `.glb`, `.rge-scene`, and `.rge-project` (the
/// last via an "All Files" filter); the `Ctrl+O` handler dispatches on
/// the picked path's kind.
///
/// Doctrinal note: this struct owns the `rfd` edge so editor-shell
/// stays free of an `rfd` dependency — mirroring how [`GlbLoaderHook`]
/// owns the `rge-io-gltf` edge. Unit struct because the dialog is
/// stateless (no last-directory / recent-files memory in v0).
struct GlbOpenFileDialog;

impl GlbOpenDialog for GlbOpenFileDialog {
    fn pick_glb_path(&self) -> Option<PathBuf> {
        // Despite the `pick_glb_path` name (kept this dispatch; the
        // rename is a cosmetic follow-up), the dialog offers every
        // supported Open candidate. `rfd` filters by extension, and a
        // literal `.rge-project` is a leading-dot-only name with no
        // extension, so an "All Files" filter is included to make it
        // pickable in v0 (OQ2). The `Ctrl+O` handler dispatches on the
        // returned path's kind.
        rfd::FileDialog::new()
            .add_filter("RGE scene / project", &["rge-scene", "rge-project"])
            .add_filter("glTF Binary", &["glb"])
            .add_filter("All Files", &["*"])
            .set_title("Open")
            .pick_file()
    }
}

/// Binary-owned [`SceneOpenHook`] impl backed by
/// `rge_scene_loader::load_scene_world_from_path`. Handed to
/// [`EditorShell`] via [`EditorShell::with_scene_open_hook`] in every
/// launch mode so `Ctrl+O` can open a `.rge-scene` / `.rge-project`
/// from any start state. Owns the `rge-scene-loader` edge so
/// editor-shell never gains that dependency — mirroring how
/// [`GlbLoaderHook`] owns `rge-io-gltf` and [`GlbOpenFileDialog`] owns
/// `rfd`. Unit struct because the loader is stateless (it re-reads the
/// path on each call).
struct SceneOpenLoaderHook;

impl SceneOpenHook for SceneOpenLoaderHook {
    fn load_scene_world(&self, path: &std::path::Path) -> Result<World, String> {
        rge_scene_loader::load_scene_world_from_path(path).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// SCENE-SAVE-WIRING — in-app "Save" (Ctrl+S) dialog + writer hooks
// ---------------------------------------------------------------------------

/// Binary-owned [`SceneSaveDialog`] impl backed by `rfd`'s native save dialog.
/// Handed to [`EditorShell`] via [`EditorShell::with_scene_save_dialog`] in
/// every launch mode so `Ctrl+S` (Save-As) works from the default cuboid demo
/// as well as `--glb` / `--scene`. Offers the `.rge-scene` filter; the writer
/// ([`SceneSaveWriterHook`]) — via the substrate — rejects any non-`*.rge-scene`
/// name.
///
/// Doctrinal note: this struct owns the `rfd` edge so editor-shell stays free
/// of an `rfd` dependency — mirroring [`GlbOpenFileDialog`]. Unit struct because
/// the dialog is stateless (no last-directory memory in v0).
struct SceneSaveFileDialog;

impl SceneSaveDialog for SceneSaveFileDialog {
    fn pick_save_path(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("RGE scene", &["rge-scene"])
            .set_title("Save Scene As")
            .save_file()
    }
}

/// Binary-owned [`SceneSaveHook`] impl backed by
/// `rge_scene_loader::save_scene_world_to_path`. Handed to [`EditorShell`] via
/// [`EditorShell::with_scene_save_hook`] in every launch mode so `Ctrl+S` can
/// write a `.rge-scene` from any start state. Owns the `rge-scene-loader` edge
/// so editor-shell never gains that dependency — mirroring how
/// [`SceneOpenLoaderHook`] owns the load edge. Derives `Scene.name` from the
/// chosen file stem (SCENE-SAVE-WIRING v0). Unit struct because the writer is
/// stateless (it re-extracts the live world on each call).
struct SceneSaveWriterHook;

impl SceneSaveHook for SceneSaveWriterHook {
    fn save_scene_world(&self, world: &World, path: &Path) -> Result<(), String> {
        // v0: Scene.name = chosen file stem (e.g. "foo" for "foo.rge-scene").
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        rge_scene_loader::save_scene_world_to_path(world, path, name).map_err(|e| e.to_string())
    }
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
// ISSUE-258 follow-up — GLB watcher reconciliation decision (pure, unit-tested)
// ---------------------------------------------------------------------------

/// What [`EditorApp::sync_glb_watcher`] must do this window event to keep
/// the notify GLB watcher reconciled with the editor-shell's live
/// `glb_source_path`.
#[derive(Debug, PartialEq, Eq)]
enum GlbWatcherAction {
    /// Re-root the watcher onto this path — either the first root (an
    /// in-app Open from a demo / `--scene` start that had no watcher) or
    /// a move (an in-app Open swapped the source to a different file than
    /// the one launched via `--glb`).
    Reroot(PathBuf),
    /// Tear the watcher down: the live `glb_source_path` went away (an
    /// in-app scene Open swaps in a world with no GLB source and clears
    /// it), so the old-file watcher must stop rather than keep
    /// hot-reloading a file the editor no longer shows.
    Teardown,
    /// Leave the watcher as-is — already rooted on the live source, or
    /// nothing to watch and nothing currently watched.
    Unchanged,
}

/// Decide how the notify GLB watcher must be reconciled this event.
///
/// This is the **pure**, unit-tested core of the
/// [`EditorApp::sync_glb_watcher`] policy: a side-effect-free function
/// of the currently-watched path and the editor-shell's live
/// `glb_source_path`. Keeping the decision separate from the
/// `GlbWatcher::new` / `tracing` side effects lets the inline tests
/// pin the full truth table without a winit loop or a real `notify`
/// watcher.
///
/// Truth table:
///
/// - `source == Some(p)`, `watched != Some(p)` → [`GlbWatcherAction::Reroot`]
///   (first-root or moved source).
/// - `source == Some(p)`, `watched == Some(p)` → [`GlbWatcherAction::Unchanged`]
///   (steady state — the per-event common case; must NOT churn the
///   watcher).
/// - `source == None`, `watched == Some(_)` → [`GlbWatcherAction::Teardown`]
///   (an in-app scene Open cleared the source; stop watching the old
///   file).
/// - `source == None`, `watched == None` → [`GlbWatcherAction::Unchanged`]
///   (nothing to follow, e.g. the demo / `--scene` start before any
///   in-app Open).
fn glb_watcher_action(watched: Option<&Path>, source: Option<&Path>) -> GlbWatcherAction {
    match source {
        Some(source) => {
            if watched == Some(source) {
                GlbWatcherAction::Unchanged
            } else {
                GlbWatcherAction::Reroot(source.to_path_buf())
            }
        }
        None => {
            if watched.is_some() {
                GlbWatcherAction::Teardown
            } else {
                GlbWatcherAction::Unchanged
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ISSUE-85 — ApplicationHandler wrapper with GLB hot-reload watcher
// ---------------------------------------------------------------------------

/// Binary-owned wrapper around [`EditorShell`] that intercepts
/// `WindowEvent::RedrawRequested` to drain pending GLB-watcher reload
/// requests before the shell processes the redraw. Every other winit
/// callback delegates straight through to the inner shell — the
/// wrapper introduces no new lifecycle behavior of its own.
///
/// Why a wrapper rather than a hook in editor-shell: keeping the
/// `notify` dependency and the watcher's request-producer logic in
/// the binary leaves `editor-shell` free of any file-system surface
/// (consistent with how the existing R-key path keeps the glTF
/// loader edge inside `rge-editor`, exposing only the
/// [`AssetReloadHook`] trait to the shell). The drain is the
/// **only** place this wrapper deviates from a pure passthrough — it
/// calls `EditorShell::handle_asset_reload` for the actual reload,
/// preserving the shell's atomic-swap semantics, PIE gate, and
/// warn-on-failure posture.
struct EditorApp {
    shell: EditorShell,
    /// Active GLB watcher. `Some` when launched with `--glb <path>`
    /// AND notify successfully attached to the parent directory;
    /// `None` for the default cuboid demo path OR when notify
    /// construction failed at startup (warn-logged, manual R-key
    /// still works).
    watcher: Option<GlbWatcher>,
    /// Last path handed to [`GlbWatcher::new`] by
    /// [`Self::sync_glb_watcher`] — the path the watcher is (or last
    /// attempted to be) rooted on. Initialised to the `--glb` launch
    /// path (`Some` for `--glb`, `None` for the cuboid demo /
    /// `--scene`), then updated on EVERY re-root attempt, success OR
    /// failure. Recording the attempt even when `GlbWatcher::new`
    /// returns `Err` is what stops a persistently-failing re-root
    /// (e.g. an Open onto a file whose parent dir can't be watched)
    /// from rebuilding the watcher every single frame.
    watched_glb_path: Option<std::path::PathBuf>,
}

impl EditorApp {
    /// Reconcile the notify GLB watcher with the editor-shell's live
    /// `glb_source_path` every window event (ISSUE-258 follow-up;
    /// SCENE-OPEN-WIRING added teardown).
    ///
    /// An in-app `Ctrl+O` Open, handled inside [`EditorShell`], either
    /// loads + swaps a user-picked GLB and commits its path as the new
    /// `glb_source_path`, or loads a `.rge-scene` / `.rge-project` and
    /// swaps in a world that has NO GLB source (clearing
    /// `glb_source_path`). The binary-owned watcher must follow both: it
    /// re-roots onto a newly opened GLB (it would otherwise keep watching
    /// the original `--glb` directory) and tears down when the source is
    /// cleared (it would otherwise keep hot-reloading a superseded file).
    /// This method closes both gaps.
    ///
    /// Rationale for the shape:
    ///
    /// - The reconciliation *decision* is delegated to the pure,
    ///   unit-tested [`glb_watcher_action`]; this method only performs
    ///   the `notify` side effects (which can't be exercised headlessly).
    /// - `notify` stays entirely inside the `rge-editor` binary —
    ///   no new editor-shell surface — exactly as the launch-time
    ///   watcher and the R-key loader edge already do.
    /// - On `GlbWatcher::new` failure the *stale* watcher is dropped
    ///   (`self.watcher = None`) rather than retained, so it can never
    ///   fire another reload of the old file after the source moved on;
    ///   manual `R` still works through the unchanged reload hook.
    /// - On a `Reroot`, `self.watched_glb_path` records the attempted
    ///   target in BOTH arms (Ok and Err). Recording on failure is what
    ///   prevents a persistently-unwatchable source from rebuilding the
    ///   watcher every frame — the next call sees `watched == source` and
    ///   returns `Unchanged`.
    /// - On a `Teardown`, both `self.watcher` and `self.watched_glb_path`
    ///   are cleared, so the next call sees `None`/`None` → `Unchanged`
    ///   and never logs again until a fresh Open re-roots.
    fn sync_glb_watcher(&mut self) {
        match glb_watcher_action(
            self.watched_glb_path.as_deref(),
            self.shell.glb_source_path(),
        ) {
            GlbWatcherAction::Unchanged => {}
            GlbWatcherAction::Teardown => {
                // An in-app scene Open (or any path that cleared
                // `glb_source_path`) superseded the GLB source. Drop the
                // now-stale watcher AND the recorded target so it stops
                // hot-reloading a file the editor no longer shows; manual
                // R-key reload is already a no-op (the shell cleared its
                // source too).
                self.watcher = None;
                self.watched_glb_path = None;
                tracing::info!(
                    target: "rge::editor",
                    "tore down GLB watcher: glb_source_path was cleared (in-app scene Open superseded the GLB source)"
                );
            }
            GlbWatcherAction::Reroot(target) => {
                match GlbWatcher::new(target.clone()) {
                    Ok(w) => {
                        self.watcher = Some(w);
                        tracing::info!(
                            target: "rge::editor",
                            path = %target.display(),
                            "re-rooted GLB watcher to follow in-app-opened file"
                        );
                    }
                    Err(e) => {
                        // Drop the stale watcher so it can't keep
                        // reloading the old file; the source has already
                        // moved on.
                        self.watcher = None;
                        tracing::warn!(
                            target: "rge::editor",
                            path = %target.display(),
                            error = %e,
                            "failed to re-root GLB watcher onto in-app-opened file; automatic hot-reload disabled (manual R-key still works)"
                        );
                    }
                }
                // Record the attempt (Ok OR Err) so a persistently-failing
                // re-root does not retry every frame.
                self.watched_glb_path = Some(target);
            }
        }
    }
}

impl ApplicationHandler<()> for EditorApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.shell.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.shell.suspended(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Drain pending watcher requests BEFORE the shell processes
        // `RedrawRequested` so a successful reload's new
        // meshes/materials are visible in the very frame this redraw
        // produces. The drain itself never mutates render assets;
        // `handle_asset_reload` is the single place all reload work
        // happens (loader invocation, atomic swap, warn-on-failure).
        if matches!(event, WindowEvent::RedrawRequested) {
            if let Some(w) = self.watcher.as_mut() {
                if w.take_reload_request(std::time::Instant::now()) {
                    self.shell.handle_asset_reload();
                }
            }
        }
        self.shell.window_event(event_loop, window_id, event);
        // A Ctrl+O Open handled inside the shell during this event
        // re-points `glb_source_path`; sync re-roots the watcher the
        // same frame so auto-reload follows the newly-opened file.
        self.sync_glb_watcher();
    }
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
            eprintln!("usage: rge-editor [--glb <path> | --scene <path>]");
            return ExitCode::from(2);
        }
    };

    // ---- ISSUE-225 — `--scene <path>` load-only branch ---------------
    // Mutually exclusive with `--glb` (parse-time rejection); checked
    // before the `--glb` match so the scene path can short-circuit the
    // winit + render-mesh wiring entirely.
    if let Some(scene_path) = cli.scene_path.as_ref() {
        let world = match rge_scene_loader::load_scene_world_from_path(scene_path) {
            Ok(w) => w,
            Err(e) => {
                eprintln!(
                    "rge-editor: failed to load --scene {}: {e}",
                    scene_path.display()
                );
                return ExitCode::FAILURE;
            }
        };
        // `EditorShell::with_world` takes the editor-shell wrapper
        // `World` (`rge_editor_shell::world::World`), not the kernel
        // world the loader returns. The wrapper exposes `kernel_mut()`
        // as the only public ingress for an externally-built kernel
        // world; replacing its default-empty inner kernel with the
        // loader's populated one preserves the wrapper's TimeScale
        // resource contract (`with_world` inserts a default if the
        // caller has not pre-installed one — the loader never does).
        let mut shell_world = rge_editor_shell::world::World::new();
        *shell_world.kernel_mut() = world;
        // ISSUE-258 / SCENE-OPEN-WIRING — attach the in-app Open
        // machinery so Ctrl+O works from the `--scene` path too. The
        // dialog hook provides the native picker; the GLB loader hook
        // (no source path: `--scene` has no `--glb` file) lets Ctrl+O
        // import a user-picked GLB; the scene-open hook lets Ctrl+O open
        // a `.rge-scene` / `.rge-project`. R-key reload stays a no-op
        // until a successful GLB Open commits a path. `--scene` keeps
        // `watcher: None` (no auto-reload here).
        let mut shell = EditorShell::with_world(shell_world)
            .with_glb_open_dialog(Box::new(GlbOpenFileDialog))
            .with_scene_open_hook(Box::new(SceneOpenLoaderHook))
            .with_scene_save_dialog(Box::new(SceneSaveFileDialog))
            .with_scene_save_hook(Box::new(SceneSaveWriterHook));
        // SCENE-SAVE-SOURCE-PATH: seed the silent-save target only for a
        // `.rge-scene` launch; a `.rge-project` leaves it None (first Ctrl+S =
        // Save-As, which the writer can satisfy).
        if scene_path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".rge-scene"))
        {
            shell = shell.with_scene_source_path(scene_path.clone());
        }
        shell.attach_glb_loader_hook(GlbLoaderHook);
        let mut app = EditorApp {
            shell,
            watcher: None,
            // No `--glb` launch path; the first in-app Open re-roots the
            // watcher onto the picked file via `sync_glb_watcher`.
            watched_glb_path: None,
        };
        let event_loop = EventLoop::new().expect("event loop");
        if let Err(e) = event_loop.run_app(&mut app) {
            eprintln!("rge-editor: run_app: {e}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    // ---- Branch on --glb flag ----------------------------------------
    let (shell, watcher) = match cli.glb_path.as_ref() {
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
            let mut shell = EditorShell::with_render_meshes_and_base_colors_and_textures(
                render_meshes,
                base_colors,
                textures_for_shell,
            )
            // ISSUE-258 / SCENE-OPEN-WIRING — native Open dialog (Ctrl+O).
            // Attached in every launch mode; here it composes onto the
            // `--glb` constructor before the reload-source attach below.
            // The scene-open hook lets Ctrl+O also open a `.rge-scene` /
            // `.rge-project`.
            .with_glb_open_dialog(Box::new(GlbOpenFileDialog))
            .with_scene_open_hook(Box::new(SceneOpenLoaderHook))
            .with_scene_save_dialog(Box::new(SceneSaveFileDialog))
            .with_scene_save_hook(Box::new(SceneSaveWriterHook));
            // Asset hot-reload — both the manual R-key path AND the
            // ISSUE-85 automatic notify watcher route through the
            // same `EditorShell::handle_asset_reload` (and the
            // shared [`GlbLoaderHook`] above). The default cuboid
            // demo never reaches this branch → both reload sources
            // silently no-op there. This also sets the `reload_hook`
            // that Ctrl+O's import step uses.
            shell.attach_glb_reload_source(path.clone(), GlbLoaderHook);
            // ISSUE-85 — start a notify watcher rooted at the
            // active source's parent directory. Construction
            // failures (e.g. parent dir not readable) warn-log and
            // proceed without automatic reload; the user can still
            // press R to reload manually.
            let watcher = match GlbWatcher::new(path.clone()) {
                Ok(w) => Some(w),
                Err(e) => {
                    tracing::warn!(
                        target: "rge::editor",
                        path = %path.display(),
                        error = %e,
                        "failed to start GLB watcher; automatic hot-reload disabled (manual R-key still works)"
                    );
                    None
                }
            };
            (shell, watcher)
        }
        None => {
            // Default — cuboid demo. Render path is byte-identical to
            // pre-dispatch-G; ISSUE-258 additionally attaches the
            // in-app Open machinery so Ctrl+O works from the demo. The
            // loader hook carries no source path (the demo has no
            // `--glb` file), so R-key reload no-ops until a successful
            // Open commits a path; the dialog hook provides the native
            // picker. No watcher in the demo path.
            let mut shell = build_cuboid_demo_shell()
                .with_glb_open_dialog(Box::new(GlbOpenFileDialog))
                .with_scene_open_hook(Box::new(SceneOpenLoaderHook))
                .with_scene_save_dialog(Box::new(SceneSaveFileDialog))
                .with_scene_save_hook(Box::new(SceneSaveWriterHook));
            shell.attach_glb_loader_hook(GlbLoaderHook);
            (shell, None)
        }
    };

    // ---- Run winit event loop -----------------------------------------
    let mut app = EditorApp {
        shell,
        watcher,
        // The initial watch target: `Some(path)` for `--glb` (the
        // launch-time watcher above is already rooted here), `None` for
        // the cuboid demo. `sync_glb_watcher` re-roots from this baseline
        // when an in-app Open changes the shell's `glb_source_path`.
        watched_glb_path: cli.glb_path.clone(),
    };
    let event_loop = EventLoop::new().expect("event loop");
    if let Err(e) = event_loop.run_app(&mut app) {
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
    // ISSUE-225 — `--scene <path>` CLI + load-helper coverage
    // -----------------------------------------------------------------------

    #[test]
    fn scene_flag_with_path_captures_path() {
        let cli = parse_args(&args(&["--scene", "demo.rge-project"])).expect("parse");
        assert_eq!(cli.scene_path, Some(PathBuf::from("demo.rge-project")));
        assert!(cli.glb_path.is_none());
    }

    #[test]
    fn scene_flag_with_rge_scene_path_captures_path() {
        let cli = parse_args(&args(&["--scene", "scenes/main.rge-scene"])).expect("parse");
        assert_eq!(cli.scene_path, Some(PathBuf::from("scenes/main.rge-scene")));
    }

    #[test]
    fn scene_flag_without_path_returns_missing_scene_path() {
        let err = parse_args(&args(&["--scene"])).expect_err("must error");
        assert_eq!(err, CliError::MissingScenePath);
    }

    #[test]
    fn scene_then_glb_returns_conflict() {
        let err = parse_args(&args(&["--scene", "a.rge-project", "--glb", "b.glb"]))
            .expect_err("must error");
        assert_eq!(err, CliError::GlbAndSceneConflict);
    }

    #[test]
    fn glb_then_scene_returns_conflict() {
        let err = parse_args(&args(&["--glb", "b.glb", "--scene", "a.rge-project"]))
            .expect_err("must error");
        assert_eq!(err, CliError::GlbAndSceneConflict);
    }

    #[test]
    fn scene_cli_error_display_lines_match_glb_style() {
        // Pin the wording so future refactors stay aligned with the
        // existing one-line stderr posture used by `--glb` errors.
        assert_eq!(
            format!("{}", CliError::MissingScenePath),
            "--scene requires a path argument"
        );
        assert_eq!(
            format!("{}", CliError::GlbAndSceneConflict),
            "--glb and --scene are mutually exclusive"
        );
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
    // SCENE-SAVE-WIRING — binary SceneSaveWriterHook disk round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn scene_save_writer_hook_round_trips_via_disk() {
        // The binary SceneSaveWriterHook writes the live world to a `.rge-scene`
        // through the real substrate, deriving Scene.name from the file stem.
        // Load the tracked golden simple-scene, save it via the hook, reload,
        // and assert the entity set survives — proving the binary's wiring +
        // name derivation against `rge_scene_loader` end-to-end. (Component
        // value-fidelity is the substrate's own round-trip test's job.)
        let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("golden-projects")
            .join("simple-scene")
            .join(".rge-project");
        let world = rge_scene_loader::load_scene_world_from_path(&golden)
            .expect("load golden simple-scene");
        let before = world.entity_count();
        assert!(before > 0, "golden simple-scene must have entities");

        let out = std::env::temp_dir().join(format!(
            "rge_editor_save_writer_{}.rge-scene",
            std::process::id()
        ));
        SceneSaveWriterHook
            .save_scene_world(&world, &out)
            .expect("SceneSaveWriterHook writes a .rge-scene");

        let reloaded =
            rge_scene_loader::load_scene_world_from_path(&out).expect("reload saved .rge-scene");
        assert_eq!(
            reloaded.entity_count(),
            before,
            "save -> load via the binary hook must preserve the entity set"
        );

        std::fs::remove_file(&out).ok();
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
    // Dispatch N2 — end-to-end GLB visual acceptance
    //
    // Loads existing fixtures through the REAL `load_all_glb_meshes`
    // path → builds `EditorShell::with_render_meshes_and_base_colors_and_textures`
    // → renders one headless frame via the editor-shell visual-test
    // harness → asserts pixel signatures. Closes the M2/M3/K
    // acceptance gap that logs alone cannot verify.
    //
    // Tests skip gracefully when no GPU adapter is available
    // (`Err("init_render_state_headless: ...")` containing
    // `"NoAdapter"`) — matches the gfx-test `ctx_or_skip!` posture.
    // -----------------------------------------------------------------------

    /// Background-clear color components in linear `Rgba8Unorm` space.
    /// Mirrors `crate::editor_shell::render_path::DEFAULT_CLEAR` which
    /// is `(0.12, 0.12, 0.14)` in float, then `round * 255 = (30, 30,
    /// 36)`. Pinned here as constants so the N2 tests don't reach
    /// into editor-shell's render_path internals.
    const BG_R: i32 = 30;
    const BG_G: i32 = 30;
    const BG_B: i32 = 36;
    /// Per-channel delta from `DEFAULT_CLEAR` that classifies a pixel
    /// as "on the cube" rather than background. Loose enough for
    /// driver rounding + Phong ambient; tight enough to reject
    /// background pixels.
    const CUBE_THRESHOLD: i32 = 30;
    /// Visual-test target dimensions. Square aspect so the auto-frame
    /// (`compute_aabb_union` + `isometric_camera_for_bounds`) centers
    /// the cube symmetrically.
    const VISUAL_W: u32 = 256;
    const VISUAL_H: u32 = 256;

    /// Serializes the GPU-bearing tests in this binary. Multiple wgpu
    /// `GfxContext` instances created and torn down concurrently inside
    /// the same test process triggered Windows
    /// `STATUS_ACCESS_VIOLATION (0xc0000005)` under the canonical
    /// workspace verification gate (`cargo test --workspace
    /// --all-targets --no-fail-fast -j 1`), after all visible tests
    /// reported `ok`. The cargo test harness runs tests within one
    /// binary on a thread pool; each GPU test below builds an
    /// `EditorShell` that owns a `GfxContext`, and concurrent
    /// init / teardown of those contexts is the failure source.
    /// Each GPU test acquires this guard at its top so the lock
    /// outlives the local `EditorShell` (variables drop in reverse
    /// of declaration), meaning shell teardown is also serialized.
    static GPU_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Acquire the GPU-test serialization guard. Poisoned mutexes
    /// (a prior panicking test) are recovered so a single failure
    /// does not block the remaining GPU tests.
    fn gpu_test_lock() -> std::sync::MutexGuard<'static, ()> {
        GPU_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// ISSUE-258 — mock [`GlbOpenDialog`] returning a fixed,
    /// pre-configured result so the open-request end-to-end tests can
    /// drive `EditorShell::handle_open_request` without a native file
    /// dialog. `Some(path)` simulates the user picking a file;
    /// `None` simulates cancel.
    struct MockOpenDialog {
        result: Option<PathBuf>,
    }

    impl GlbOpenDialog for MockOpenDialog {
        fn pick_glb_path(&self) -> Option<PathBuf> {
            self.result.clone()
        }
    }

    /// Returns `true` if the supplied error string indicates that no
    /// compatible GPU adapter is available (i.e. the test should skip
    /// rather than panic). Centralised so both `render_fixture_end_to_end`
    /// and `render_shell_one_frame` recognise the same set of strings;
    /// `"no compatible GPU adapter"` is the canonical wgpu/gfx error on
    /// headless CI (ubuntu-latest without Mesa / GPU passthrough).
    fn is_missing_gpu_adapter_error(e: &str) -> bool {
        e.contains("NoAdapter") || e.contains("no GPU") || e.contains("no compatible GPU adapter")
    }

    /// Common end-to-end render pipeline: load a glTF fixture through
    /// the production loader, build an `EditorShell`, render one
    /// headless frame, return the readback buffer (or `None` to skip
    /// when no GPU adapter is available).
    ///
    /// Translates `Vec<Option<TextureInfo>>` → `Vec<Option<(u32, u32,
    /// Vec<u8>)>>` exactly the way `main()` does, so the test exercises
    /// the same boundary the production binary crosses.
    fn render_fixture_end_to_end(fixture_name: &str) -> Option<rge_gfx::ReadbackBuffer> {
        let path = fixtures_path().join(fixture_name);
        if skip_if_fixture_missing(&path) {
            return None;
        }
        let (meshes, base_colors, textures) = load_all_glb_meshes(&path)
            .unwrap_or_else(|e| panic!("load_all_glb_meshes({fixture_name}): {e}"));
        let textures_for_shell: Vec<Option<(u32, u32, Vec<u8>)>> = textures
            .into_iter()
            .map(|t| t.map(|t| (t.width, t.height, t.pixels)))
            .collect();
        let mut shell = EditorShell::with_render_meshes_and_base_colors_and_textures(
            meshes,
            base_colors,
            textures_for_shell,
        );
        match rge_editor_shell::visual_test_harness::render_one_frame_to_readback(
            &mut shell,
            wgpu::TextureFormat::Rgba8Unorm,
            VISUAL_W,
            VISUAL_H,
        ) {
            Ok(buf) => Some(buf),
            Err(e) if is_missing_gpu_adapter_error(&e) => {
                eprintln!("SKIP: no GPU adapter — {fixture_name} ({e})");
                None
            }
            Err(e) => panic!("render_one_frame_to_readback({fixture_name}): {e}"),
        }
    }

    /// Walk the central horizontal row, classify pixels as cube /
    /// background by channel delta from `DEFAULT_CLEAR`, and return
    /// `(cube_pixel_count, min_r, max_r, min_g, max_g, min_b, max_b,
    /// sum_r, sum_g, sum_b)` aggregates over the on-cube subset. The
    /// per-channel sums let callers compute averages for dominance
    /// assertions; min/max for spread.
    fn scan_central_row(buf: &rge_gfx::ReadbackBuffer) -> RowScanStats {
        let y = VISUAL_H / 2;
        let mut s = RowScanStats {
            cube_pixel_count: 0,
            min_r: u8::MAX,
            max_r: 0,
            min_g: u8::MAX,
            max_g: 0,
            min_b: u8::MAX,
            max_b: 0,
            sum_r: 0,
            sum_g: 0,
            sum_b: 0,
        };
        for x in 0..VISUAL_W {
            let p = buf.pixel(x, y).expect("row pixel exists");
            let dr = (i32::from(p.0) - BG_R).abs();
            let dg = (i32::from(p.1) - BG_G).abs();
            let db = (i32::from(p.2) - BG_B).abs();
            if dr > CUBE_THRESHOLD || dg > CUBE_THRESHOLD || db > CUBE_THRESHOLD {
                s.cube_pixel_count += 1;
                s.min_r = s.min_r.min(p.0);
                s.max_r = s.max_r.max(p.0);
                s.min_g = s.min_g.min(p.1);
                s.max_g = s.max_g.max(p.1);
                s.min_b = s.min_b.min(p.2);
                s.max_b = s.max_b.max(p.2);
                s.sum_r += u32::from(p.0);
                s.sum_g += u32::from(p.1);
                s.sum_b += u32::from(p.2);
            }
        }
        s
    }

    struct RowScanStats {
        cube_pixel_count: u32,
        min_r: u8,
        max_r: u8,
        min_g: u8,
        max_g: u8,
        min_b: u8,
        max_b: u8,
        sum_r: u32,
        sum_g: u32,
        sum_b: u32,
    }

    impl RowScanStats {
        fn avg_r(&self) -> u32 {
            if self.cube_pixel_count == 0 {
                0
            } else {
                self.sum_r / self.cube_pixel_count
            }
        }
        fn avg_g(&self) -> u32 {
            if self.cube_pixel_count == 0 {
                0
            } else {
                self.sum_g / self.cube_pixel_count
            }
        }
        fn avg_b(&self) -> u32 {
            if self.cube_pixel_count == 0 {
                0
            } else {
                self.sum_b / self.cube_pixel_count
            }
        }
    }

    /// **Canonical N2 acceptance gate.** Loads textured_uv_cube.glb
    /// through the production `load_all_glb_meshes` path, renders
    /// headlessly via the editor-shell harness, and asserts that the
    /// central row's on-cube pixels show red/blue channel spread
    /// `>50` — proving the 4×4 checkerboard's per-fragment UV
    /// sampling is active end-to-end (M1 UV propagation + M2 texture
    /// binding + M3 normal preservation all flowing through the real
    /// loader and the real shell construction path).
    #[test]
    fn textured_uv_cube_renders_with_visible_color_variance_end_to_end() {
        let _gpu_lock = gpu_test_lock();
        let Some(buf) = render_fixture_end_to_end("textured_uv_cube.glb") else {
            return;
        };
        let s = scan_central_row(&buf);
        assert!(
            s.cube_pixel_count > 8,
            "expected central row to hit the textured cube at multiple pixels; got {}",
            s.cube_pixel_count
        );
        let red_spread = i32::from(s.max_r) - i32::from(s.min_r);
        let blue_spread = i32::from(s.max_b) - i32::from(s.min_b);
        assert!(
            red_spread > 50 || blue_spread > 50,
            "textured_uv_cube.glb end-to-end must show per-fragment color variance \
             (UV sampling + texture binding active); \
             cube_pixels={} red_spread={red_spread} blue_spread={blue_spread} \
             red=[{}, {}] blue=[{}, {}]",
            s.cube_pixel_count,
            s.min_r,
            s.max_r,
            s.min_b,
            s.max_b
        );
    }

    /// Regression spot-check for the untextured base_color path:
    /// cube.glb (`base_color = [0.4, 0.6, 0.8, 1.0]`, no texture)
    /// renders with blue-dominant on-cube pixels. Verifies the
    /// dispatch-K base_color tint reaches the GPU when no texture is
    /// bound (1×1 placeholder path).
    #[test]
    fn cube_glb_renders_lit_blue_distinct_from_background_end_to_end() {
        let _gpu_lock = gpu_test_lock();
        let Some(buf) = render_fixture_end_to_end("cube.glb") else {
            return;
        };
        let s = scan_central_row(&buf);
        assert!(
            s.cube_pixel_count > 8,
            "expected central row to hit the cube at multiple pixels; got {}",
            s.cube_pixel_count
        );
        // base_color = [0.4, 0.6, 0.8, 1.0] (blue-dominant). After
        // Lambert+Phong shading the channel ordering survives:
        // avg_b > avg_g > avg_r by a comfortable margin.
        let avg_r = s.avg_r();
        let avg_g = s.avg_g();
        let avg_b = s.avg_b();
        assert!(
            avg_b > avg_g && avg_g > avg_r,
            "cube.glb end-to-end must show blue-dominant on-cube pixels matching \
             base_color = [0.4, 0.6, 0.8, 1.0]; got avg_rgb=({avg_r}, {avg_g}, {avg_b}) \
             cube_pixels={}",
            s.cube_pixel_count
        );
        // Additional sanity: the blue channel should clear the
        // background's blue baseline by a wide margin (no driver
        // ambiguity).
        assert!(
            i32::try_from(avg_b).unwrap_or(0) - BG_B > 30,
            "cube.glb on-cube blue must clear background by >30 bytes; \
             got avg_b={avg_b} background_b={BG_B}"
        );
    }

    /// Regression spot-check for the L+K combined path: pbr_material.
    /// glb has `base_color = [0.97, 0.86, 0.32, 1.0]` (gold) AND a
    /// 1×1 white placeholder texture (from dispatch L's real-PNG
    /// synthetic). Final tint = white_texture × gold_base_color =
    /// gold. Asserts on-cube pixels show gold dominance (red and
    /// green > blue, red ≈ green within tolerance).
    #[test]
    fn pbr_material_glb_renders_lit_gold_distinct_from_background_end_to_end() {
        let _gpu_lock = gpu_test_lock();
        let Some(buf) = render_fixture_end_to_end("pbr_material.glb") else {
            return;
        };
        let s = scan_central_row(&buf);
        assert!(
            s.cube_pixel_count > 8,
            "expected central row to hit the pbr-material cube at multiple pixels; got {}",
            s.cube_pixel_count
        );
        let avg_r = s.avg_r();
        let avg_g = s.avg_g();
        let avg_b = s.avg_b();
        // Gold = [0.97, 0.86, 0.32]: avg_r > avg_b AND avg_g > avg_b
        // by a comfortable margin.
        assert!(
            avg_r > avg_b && avg_g > avg_b,
            "pbr_material.glb end-to-end must show gold-dominant on-cube pixels \
             matching base_color = [0.97, 0.86, 0.32, 1.0]; \
             got avg_rgb=({avg_r}, {avg_g}, {avg_b}) cube_pixels={}",
            s.cube_pixel_count
        );
        // avg_r should clear avg_b by a substantial margin — gold
        // has 65 bytes more red than blue at full intensity, and
        // Phong shading preserves the ordering.
        let r_minus_b = i32::try_from(avg_r).unwrap_or(0) - i32::try_from(avg_b).unwrap_or(0);
        assert!(
            r_minus_b > 30,
            "pbr_material.glb avg_r should exceed avg_b by >30 bytes; \
             got avg_r={avg_r} avg_b={avg_b} (delta={r_minus_b})"
        );
    }

    // -----------------------------------------------------------------------
    // Asset hot-reload — R-key end-to-end tests
    //
    // Builds an `EditorShell` from one fixture's data, attaches a real
    // [`GlbLoaderHook`] pointing at a second fixture, calls
    // `shell.handle_asset_reload()` (simulating an R-key press), and
    // verifies the post-reload pixel signature differs in the expected
    // direction. Failure-path test asserts that a hook err leaves the
    // pre-reload field state intact (shell's prebuilt vecs reflect the
    // pre-reload fixture, so a re-render produces the same signature).
    // -----------------------------------------------------------------------

    /// Build an `EditorShell` from a glTF fixture's contents, NO render
    /// init yet. Companion to `render_fixture_end_to_end` for tests that
    /// need to inspect / mutate the shell between renders.
    fn build_shell_from_fixture(fixture_name: &str) -> Option<EditorShell> {
        let path = fixtures_path().join(fixture_name);
        if skip_if_fixture_missing(&path) {
            return None;
        }
        let (meshes, base_colors, textures) = load_all_glb_meshes(&path)
            .unwrap_or_else(|e| panic!("load_all_glb_meshes({fixture_name}): {e}"));
        let textures_for_shell: Vec<Option<(u32, u32, Vec<u8>)>> = textures
            .into_iter()
            .map(|t| t.map(|t| (t.width, t.height, t.pixels)))
            .collect();
        Some(
            EditorShell::with_render_meshes_and_base_colors_and_textures(
                meshes,
                base_colors,
                textures_for_shell,
            ),
        )
    }

    /// Render one headless frame from the given shell, returning the
    /// readback buffer (or `None` on no-GPU CI). Skip path matches
    /// `render_fixture_end_to_end`.
    fn render_shell_one_frame(shell: &mut EditorShell) -> Option<rge_gfx::ReadbackBuffer> {
        match rge_editor_shell::visual_test_harness::render_one_frame_to_readback(
            shell,
            wgpu::TextureFormat::Rgba8Unorm,
            VISUAL_W,
            VISUAL_H,
        ) {
            Ok(buf) => Some(buf),
            Err(e) if is_missing_gpu_adapter_error(&e) => {
                eprintln!("SKIP: no GPU adapter — {e}");
                None
            }
            Err(e) => panic!("render_one_frame_to_readback: {e}"),
        }
    }

    /// **Canonical R-key end-to-end test.** Loads cube.glb (blue-
    /// dominant, no texture), attaches the textured_uv_cube.glb path
    /// + [`GlbLoaderHook`], renders frame 1 (asserts blue dominance
    /// per the cube.glb reference assertion), simulates an R-key
    /// press via `shell.handle_asset_reload()`, renders frame 2 and
    /// asserts the on-cube pixels now show the red/blue channel
    /// spread that the textured_uv_cube checkerboard produces.
    ///
    /// This exercises the full path: keyboard handler → hook trait
    /// callback → real `load_all_glb_meshes` → editor-shell's
    /// `reload_render_assets` → atomic swap of materials + meshes →
    /// updated prebuilt vecs → next render reflects new content.
    #[test]
    fn r_key_reload_swaps_to_textured_uv_cube_end_to_end() {
        let _gpu_lock = gpu_test_lock();
        let Some(mut shell) = build_shell_from_fixture("cube.glb") else {
            return;
        };
        // Frame 1 — blue cube.glb.
        let Some(buf_pre) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let pre = scan_central_row(&buf_pre);
        assert!(
            pre.cube_pixel_count > 8,
            "pre-reload: expected central row hits; got {}",
            pre.cube_pixel_count
        );
        assert!(
            pre.avg_b() > pre.avg_g() && pre.avg_g() > pre.avg_r(),
            "pre-reload should be blue-dominant (cube.glb); got rgb=({}, {}, {})",
            pre.avg_r(),
            pre.avg_g(),
            pre.avg_b()
        );

        // Attach a hook pointing at the textured_uv_cube fixture so
        // `handle_asset_reload` swaps cube.glb's mesh+material set
        // for the checkerboard-textured version.
        let target_path = fixtures_path().join("textured_uv_cube.glb");
        if skip_if_fixture_missing(&target_path) {
            return;
        }
        shell.attach_glb_reload_source(target_path, GlbLoaderHook);

        // R-key press — drive the production keyboard handler path.
        shell.handle_asset_reload();

        // Frame 2 — textured_uv_cube.glb. Per
        // textured_uv_cube_renders_with_visible_color_variance_end_to_end:
        // central-row red OR blue spread > 50 proves the
        // checkerboard's per-fragment UV sampling is active.
        let Some(buf_post) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let post = scan_central_row(&buf_post);
        assert!(
            post.cube_pixel_count > 8,
            "post-reload: expected central row hits; got {}",
            post.cube_pixel_count
        );
        let red_spread = i32::from(post.max_r) - i32::from(post.min_r);
        let blue_spread = i32::from(post.max_b) - i32::from(post.min_b);
        assert!(
            red_spread > 50 || blue_spread > 50,
            "post-reload should show checkerboard color variance \
             (textured_uv_cube.glb); cube_pixels={} red_spread={red_spread} \
             blue_spread={blue_spread} red=[{}, {}] blue=[{}, {}]",
            post.cube_pixel_count,
            post.min_r,
            post.max_r,
            post.min_b,
            post.max_b
        );
    }

    /// **ISSUE-258 Open-GLB success end-to-end (Test B).** Mirrors the
    /// R-key success test, but drives [`EditorShell::handle_open_request`]
    /// through a mock [`GlbOpenDialog`] returning the
    /// `textured_uv_cube.glb` fixture path. Loads cube.glb (blue),
    /// renders frame 1 (asserts blue dominance), simulates Ctrl+O,
    /// renders frame 2 and asserts the checkerboard red/blue spread —
    /// AND asserts that `glb_source_path` was committed to the picked
    /// path ONLY after the swap succeeded (commit-after-success).
    ///
    /// Exercises the full Open path: dialog hook → candidate →
    /// `AssetReloadHook::reload_glb` (real `load_all_glb_meshes`) →
    /// `reload_render_assets` (atomic swap) → glb_source_path commit.
    #[test]
    fn open_request_success_swaps_assets_and_commits_path_end_to_end() {
        let _gpu_lock = gpu_test_lock();
        let Some(mut shell) = build_shell_from_fixture("cube.glb") else {
            return;
        };
        // Frame 1 — blue cube.glb.
        let Some(buf_pre) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let pre = scan_central_row(&buf_pre);
        assert!(
            pre.cube_pixel_count > 8,
            "pre-open: expected central row hits; got {}",
            pre.cube_pixel_count
        );
        assert!(
            pre.avg_b() > pre.avg_g() && pre.avg_g() > pre.avg_r(),
            "pre-open should be blue-dominant (cube.glb); got rgb=({}, {}, {})",
            pre.avg_r(),
            pre.avg_g(),
            pre.avg_b()
        );

        // The shell started with no Open machinery (build_shell_from_fixture
        // does not wire it); attach the real loader hook + a mock dialog
        // that "picks" textured_uv_cube.glb. glb_source_path is None
        // before the Open (the picked path is committed only on success).
        let target_path = fixtures_path().join("textured_uv_cube.glb");
        if skip_if_fixture_missing(&target_path) {
            return;
        }
        assert!(
            shell.glb_source_path().is_none(),
            "pre-open: build_shell_from_fixture leaves glb_source_path unset"
        );
        shell.attach_glb_loader_hook(GlbLoaderHook);
        shell = shell.with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(target_path.clone()),
        }));

        // Ctrl+O — drive the production open handler path.
        shell.handle_open_request();

        // Commit-after-success: the picked path is now the source path.
        assert_eq!(
            shell.glb_source_path(),
            Some(target_path.as_path()),
            "successful Open must commit the picked path to glb_source_path"
        );

        // Frame 2 — textured_uv_cube.glb checkerboard variance proves
        // the swap landed (same assertion as the R-key success test).
        let Some(buf_post) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let post = scan_central_row(&buf_post);
        assert!(
            post.cube_pixel_count > 8,
            "post-open: expected central row hits; got {}",
            post.cube_pixel_count
        );
        let red_spread = i32::from(post.max_r) - i32::from(post.min_r);
        let blue_spread = i32::from(post.max_b) - i32::from(post.min_b);
        assert!(
            red_spread > 50 || blue_spread > 50,
            "post-open should show checkerboard color variance \
             (textured_uv_cube.glb); cube_pixels={} red_spread={red_spread} \
             blue_spread={blue_spread} red=[{}, {}] blue=[{}, {}]",
            post.cube_pixel_count,
            post.min_r,
            post.max_r,
            post.min_b,
            post.max_b
        );
    }

    /// ISSUE-258 — a cancelled Open (mock dialog returns `None`)
    /// retains the prior rendered frame and commits no path. Runs
    /// through the real GPU path so the "previous frame retained"
    /// guarantee is observed at the pixel level, not just the field.
    #[test]
    fn open_request_cancel_retains_prior_frame_end_to_end() {
        let _gpu_lock = gpu_test_lock();
        let Some(mut shell) = build_shell_from_fixture("cube.glb") else {
            return;
        };
        let Some(buf_pre) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let pre = scan_central_row(&buf_pre);
        assert!(pre.cube_pixel_count > 8);
        let pre_avg_b = pre.avg_b();
        assert!(
            pre_avg_b > pre.avg_g() && pre.avg_g() > pre.avg_r(),
            "pre-open should be blue-dominant (cube.glb)"
        );

        // Attach the real loader + a cancelling dialog. Open no-ops.
        shell.attach_glb_loader_hook(GlbLoaderHook);
        shell = shell.with_glb_open_dialog(Box::new(MockOpenDialog { result: None }));
        shell.handle_open_request();

        assert!(
            shell.glb_source_path().is_none(),
            "cancelled Open must not commit a path"
        );

        // Frame 2 — still cube.glb (blue), no swap occurred.
        let Some(buf_post) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let post = scan_central_row(&buf_post);
        assert!(post.cube_pixel_count > 8);
        assert!(
            post.avg_b() > post.avg_g() && post.avg_g() > post.avg_r(),
            "post-cancel should still be blue-dominant; got rgb=({}, {}, {})",
            post.avg_r(),
            post.avg_g(),
            post.avg_b()
        );
        let db = (i32::try_from(post.avg_b()).unwrap_or(0) - i32::try_from(pre_avg_b).unwrap_or(0))
            .abs();
        assert!(
            db <= 5,
            "pre/post avg_b should match within 5 bytes (cancel = unchanged); \
             pre={pre_avg_b} post={} delta={db}",
            post.avg_b()
        );
    }

    /// R-key on a malformed / missing target preserves the prior
    /// frame's content. The hook returns Err; `handle_asset_reload`
    /// warn-logs and no-ops; the shell's prebuilt vecs still hold
    /// cube.glb's data; the next render reproduces the same blue
    /// signature.
    #[test]
    fn r_key_reload_on_missing_file_preserves_prior_frame() {
        let _gpu_lock = gpu_test_lock();
        let Some(mut shell) = build_shell_from_fixture("cube.glb") else {
            return;
        };
        let Some(buf_pre) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let pre = scan_central_row(&buf_pre);
        assert!(pre.cube_pixel_count > 8);
        let pre_avg_b = pre.avg_b();
        let pre_avg_r = pre.avg_r();
        assert!(
            pre_avg_b > pre.avg_g() && pre.avg_g() > pre_avg_r,
            "pre-reload should be blue-dominant (cube.glb)"
        );

        // Attach a hook pointing at a path that doesn't exist. The
        // hook's `load_all_glb_meshes` call will fail at `import_glb`.
        let bogus = fixtures_path().join("absolutely_does_not_exist__r_key_reload_test.glb");
        shell.attach_glb_reload_source(bogus, GlbLoaderHook);

        // R-key press — hook returns Err, handler warn-logs + no-ops.
        shell.handle_asset_reload();

        // Frame 2 — still cube.glb (prebuilt vecs unchanged). The
        // re-init reproduces the same blue signature.
        let Some(buf_post) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let post = scan_central_row(&buf_post);
        assert!(post.cube_pixel_count > 8);
        // Allow tiny driver rounding; assertion is "blue still dominant".
        assert!(
            post.avg_b() > post.avg_g() && post.avg_g() > post.avg_r(),
            "post-failed-reload should still be blue-dominant; got rgb=({}, {}, {})",
            post.avg_r(),
            post.avg_g(),
            post.avg_b()
        );
        // Stronger: pre/post avg_b within 5 bytes.
        let db = (i32::try_from(post.avg_b()).unwrap_or(0) - i32::try_from(pre_avg_b).unwrap_or(0))
            .abs();
        assert!(
            db <= 5,
            "pre/post avg_b should match within 5 bytes (failed reload = unchanged); \
             pre={pre_avg_b} post={} delta={db}",
            post.avg_b()
        );
    }

    /// R-key reload on a malformed (wrong-magic-bytes) target retains
    /// the prior rendered frame, then a valid overwrite of the same
    /// path recovers through the same attached hook. Parallel in shape
    /// to [`r_key_reload_on_missing_file_preserves_prior_frame`], but
    /// the file exists on disk with the wrong magic so the failure is
    /// a parser error (`Err` from `import_glb`) rather than I/O
    /// missing-file.
    ///
    /// Three legs:
    /// 1. Build a shell from `cube.glb`, render frame 1, assert the
    ///    central-row blue-dominant signature.
    /// 2. Stage a temp file containing `b"not a glb"` wrong-magic
    ///    bytes; directly assert `GlbLoaderHook.reload_glb(&path)`
    ///    returns `Err`; attach the same path to the shell; drive
    ///    `shell.handle_asset_reload()`; render frame 2 and assert
    ///    the central-row avg RGB matches frame 1 within
    ///    `CUBE_THRESHOLD` and stays blue-dominant.
    /// 3. Overwrite the same temp path with valid
    ///    `textured_uv_cube.glb` bytes; drive
    ///    `shell.handle_asset_reload()` again through the same
    ///    attached source; render frame 3 and assert the checkerboard
    ///    spread (`red_spread > 50 || blue_spread > 50`) and that at
    ///    least one channel of the avg RGB shifts by more than
    ///    `CUBE_THRESHOLD` relative to frames 1/2.
    #[test]
    fn r_key_reload_on_malformed_glb_retains_then_recovers() {
        let _gpu_lock = gpu_test_lock();
        // Stage a temp file with wrong-magic bytes. The direct hook
        // assertion below must run regardless of GPU availability, so
        // we build the malformed path before any GPU-dependent setup.
        let tmp_dir = std::env::temp_dir().join(format!(
            "rge-editor-issue89-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp_dir).expect("mkdir tmp_dir");
        let malformed_path = tmp_dir.join("malformed.glb");
        std::fs::write(&malformed_path, b"not a glb").expect("write malformed bytes");

        // Direct Err coverage — proves the parser-failure path without
        // inferring from pixels or logs. Runs even on no-GPU CI.
        assert!(
            GlbLoaderHook.reload_glb(&malformed_path).is_err(),
            "GlbLoaderHook.reload_glb must return Err for wrong-magic bytes \
             at {}",
            malformed_path.display()
        );

        // Frame 1 — blue cube.glb (skip on no GPU / missing fixture).
        let Some(mut shell) = build_shell_from_fixture("cube.glb") else {
            return;
        };
        let Some(buf_pre) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let pre = scan_central_row(&buf_pre);
        assert!(
            pre.cube_pixel_count > 8,
            "pre-reload: expected central row hits; got {}",
            pre.cube_pixel_count
        );
        assert!(
            pre.avg_b() > pre.avg_g() && pre.avg_g() > pre.avg_r(),
            "pre-reload should be blue-dominant (cube.glb); got rgb=({}, {}, {})",
            pre.avg_r(),
            pre.avg_g(),
            pre.avg_b()
        );
        let pre_avg_r = pre.avg_r();
        let pre_avg_g = pre.avg_g();
        let pre_avg_b = pre.avg_b();

        // Attach the malformed path through the production hook path,
        // then drive `handle_asset_reload` — the hook returns Err, the
        // handler warn-logs and no-ops, the shell's prebuilt vecs still
        // hold cube.glb's data.
        shell.attach_glb_reload_source(malformed_path.clone(), GlbLoaderHook);
        shell.handle_asset_reload();

        let Some(buf_fail) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let fail = scan_central_row(&buf_fail);
        assert!(
            fail.cube_pixel_count > 8,
            "post-malformed: expected central row hits; got {}",
            fail.cube_pixel_count
        );
        // Qualitative signature unchanged — cube.glb is still blue-dominant.
        assert!(
            fail.avg_b() > fail.avg_g() && fail.avg_g() > fail.avg_r(),
            "post-malformed should still be blue-dominant; got rgb=({}, {}, {})",
            fail.avg_r(),
            fail.avg_g(),
            fail.avg_b()
        );
        // Quantitative match — per-channel avg RGB deltas <= CUBE_THRESHOLD.
        let dr_fail = (i32::try_from(fail.avg_r()).unwrap_or(0)
            - i32::try_from(pre_avg_r).unwrap_or(0))
        .abs();
        let dg_fail = (i32::try_from(fail.avg_g()).unwrap_or(0)
            - i32::try_from(pre_avg_g).unwrap_or(0))
        .abs();
        let db_fail = (i32::try_from(fail.avg_b()).unwrap_or(0)
            - i32::try_from(pre_avg_b).unwrap_or(0))
        .abs();
        assert!(
            dr_fail <= CUBE_THRESHOLD && dg_fail <= CUBE_THRESHOLD && db_fail <= CUBE_THRESHOLD,
            "pre/post-malformed avg RGB should match within CUBE_THRESHOLD; \
             pre=({pre_avg_r}, {pre_avg_g}, {pre_avg_b}) post=({}, {}, {}) \
             deltas=({dr_fail}, {dg_fail}, {db_fail})",
            fail.avg_r(),
            fail.avg_g(),
            fail.avg_b()
        );

        // Recovery leg — overwrite the SAME temp path with valid GLB
        // bytes from textured_uv_cube.glb and prove the previously
        // attached hook still works.
        let textured_uv_cube = fixtures_path().join("textured_uv_cube.glb");
        if skip_if_fixture_missing(&textured_uv_cube) {
            return;
        }
        let valid_bytes = std::fs::read(&textured_uv_cube).expect("read textured_uv_cube.glb");
        std::fs::write(&malformed_path, &valid_bytes).expect("overwrite with valid bytes");

        // Reload through the SAME attached path — proves the failed
        // parse did not poison the hook/source path.
        shell.handle_asset_reload();

        let Some(buf_ok) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let ok = scan_central_row(&buf_ok);
        assert!(
            ok.cube_pixel_count > 8,
            "post-recovery: expected central row hits; got {}",
            ok.cube_pixel_count
        );
        // Checkerboard color variance from textured_uv_cube.glb.
        let red_spread = i32::from(ok.max_r) - i32::from(ok.min_r);
        let blue_spread = i32::from(ok.max_b) - i32::from(ok.min_b);
        assert!(
            red_spread > 50 || blue_spread > 50,
            "post-recovery should show checkerboard variance \
             (textured_uv_cube.glb); cube_pixels={} red_spread={red_spread} \
             blue_spread={blue_spread} red=[{}, {}] blue=[{}, {}]",
            ok.cube_pixel_count,
            ok.min_r,
            ok.max_r,
            ok.min_b,
            ok.max_b
        );
        // At least one channel of avg RGB must shift by more than
        // CUBE_THRESHOLD relative to frames 1/2 — proves the swap landed
        // and the recovery reload is not a no-op.
        let dr_ok =
            (i32::try_from(ok.avg_r()).unwrap_or(0) - i32::try_from(pre_avg_r).unwrap_or(0)).abs();
        let dg_ok =
            (i32::try_from(ok.avg_g()).unwrap_or(0) - i32::try_from(pre_avg_g).unwrap_or(0)).abs();
        let db_ok =
            (i32::try_from(ok.avg_b()).unwrap_or(0) - i32::try_from(pre_avg_b).unwrap_or(0)).abs();
        assert!(
            dr_ok > CUBE_THRESHOLD || dg_ok > CUBE_THRESHOLD || db_ok > CUBE_THRESHOLD,
            "post-recovery avg RGB should shift > CUBE_THRESHOLD on at least \
             one channel relative to frames 1/2; pre=({pre_avg_r}, {pre_avg_g}, \
             {pre_avg_b}) post=({}, {}, {}) deltas=({dr_ok}, {dg_ok}, {db_ok})",
            ok.avg_r(),
            ok.avg_g(),
            ok.avg_b()
        );

        // Best-effort cleanup — leaks across panic are tolerable.
        drop(std::fs::remove_file(&malformed_path));
        drop(std::fs::remove_dir(&tmp_dir));
    }

    // -----------------------------------------------------------------------
    // ISSUE-85 — notify-backed GLB watcher → handle_asset_reload integration
    // -----------------------------------------------------------------------
    //
    // The watcher's debounce/filter/coalesce logic is covered by
    // [`glb_watcher::tests`]. These integration tests prove the
    // composite path: a synthetic notify event drives a drained
    // reload request, the request is fed into the same
    // [`EditorShell::handle_asset_reload`] the R-key path uses, the
    // shell's failure-then-success flow honours the loader Err
    // (warn-log + prior state retained) and the subsequent loader
    // Ok (atomic swap), AND the same `GlbWatcher` instance keeps
    // producing fresh requests across both rounds.

    #[test]
    fn watcher_drain_drives_shell_reload_through_failure_and_recovery() {
        let _gpu_lock = gpu_test_lock();
        // Skip cleanly on no-GPU CI: the end-to-end success leg
        // needs `reload_render_assets`, which requires `gfx_ctx`.
        let Some(mut shell) = build_shell_from_fixture("cube.glb") else {
            return;
        };
        let textured_uv_cube = fixtures_path().join("textured_uv_cube.glb");
        if skip_if_fixture_missing(&textured_uv_cube) {
            return;
        }
        let Some(buf_pre) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let pre = scan_central_row(&buf_pre);
        assert!(
            pre.cube_pixel_count > 8,
            "pre-reload: expected central row hits; got {}",
            pre.cube_pixel_count
        );
        assert!(
            pre.avg_b() > pre.avg_g() && pre.avg_g() > pre.avg_r(),
            "pre-reload should be blue-dominant (cube.glb); got rgb=({}, {}, {})",
            pre.avg_r(),
            pre.avg_g(),
            pre.avg_b()
        );
        let pre_avg_b = pre.avg_b();

        // Stage a temp GLB at a path with a real parent directory.
        // The watcher targets this path; the loader hook reads from
        // the same path on every call, so the bytes on disk decide
        // whether the reload succeeds.
        let tmp_dir = std::env::temp_dir().join(format!(
            "rge-editor-issue85-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp_dir).expect("mkdir tmp_dir");
        let staged_path = tmp_dir.join("asset.glb");
        // Round 1: write malformed bytes — the loader's `import_glb`
        // will return `Err`.
        std::fs::write(&staged_path, b"NOT-A-VALID-GLB").expect("write malformed");

        // Wire up the watcher + hook against the staged path. The
        // hook is the real `GlbLoaderHook` (it does what production
        // does: `load_all_glb_meshes`).
        shell.attach_glb_reload_source(staged_path.clone(), GlbLoaderHook);
        let (mut watcher, tx) = glb_watcher::GlbWatcher::for_test(staged_path.clone());

        // Inject a modify-event burst, drain across the debounce
        // window, and route the resulting request into the shell.
        let t0 = std::time::Instant::now();
        tx.send(Ok(notify::Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![staged_path.clone()],
            attrs: Default::default(),
        }))
        .expect("send modify");
        assert!(!watcher.take_reload_request(t0), "ingest does not fire");
        assert!(
            watcher.take_reload_request(t0 + glb_watcher::DEBOUNCE),
            "request ready after debounce"
        );
        // The shell's `handle_asset_reload` calls back into the
        // hook → `load_all_glb_meshes` → `import_glb`, which returns
        // `Err` for malformed bytes. The handler must warn-log and
        // retain the previously-rendered cube.glb state.
        shell.handle_asset_reload();

        let Some(buf_after_fail) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let after_fail = scan_central_row(&buf_after_fail);
        assert!(after_fail.cube_pixel_count > 8);
        assert!(
            after_fail.avg_b() > after_fail.avg_g() && after_fail.avg_g() > after_fail.avg_r(),
            "post-failed-reload should still be blue-dominant (state retained); got rgb=({}, {}, {})",
            after_fail.avg_r(),
            after_fail.avg_g(),
            after_fail.avg_b()
        );
        let db = (i32::try_from(after_fail.avg_b()).unwrap_or(0)
            - i32::try_from(pre_avg_b).unwrap_or(0))
        .abs();
        assert!(
            db <= 5,
            "pre/after-fail avg_b should match within 5 bytes; pre={pre_avg_b} after={} delta={db}",
            after_fail.avg_b()
        );

        // Round 2: overwrite the staged file with valid GLB bytes.
        // The SAME watcher must produce another reload request, and
        // `handle_asset_reload` must succeed this time — proven by
        // the post-frame's checkerboard color variance from the
        // textured_uv_cube fixture.
        let valid_bytes = std::fs::read(&textured_uv_cube).expect("read fixture");
        std::fs::write(&staged_path, &valid_bytes).expect("write valid bytes");
        let t1 = t0 + glb_watcher::DEBOUNCE + std::time::Duration::from_millis(50);
        tx.send(Ok(notify::Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![staged_path.clone()],
            attrs: Default::default(),
        }))
        .expect("send modify (round 2)");
        assert!(!watcher.take_reload_request(t1), "ingest does not fire");
        assert!(
            watcher.take_reload_request(t1 + glb_watcher::DEBOUNCE),
            "second request ready — watcher still live"
        );
        shell.handle_asset_reload();

        let Some(buf_after_ok) = render_shell_one_frame(&mut shell) else {
            return;
        };
        let after_ok = scan_central_row(&buf_after_ok);
        assert!(after_ok.cube_pixel_count > 8);
        let red_spread = i32::from(after_ok.max_r) - i32::from(after_ok.min_r);
        let blue_spread = i32::from(after_ok.max_b) - i32::from(after_ok.min_b);
        assert!(
            red_spread > 50 || blue_spread > 50,
            "post-recovery should show checkerboard variance (textured_uv_cube.glb); \
             cube_pixels={} red_spread={red_spread} blue_spread={blue_spread}",
            after_ok.cube_pixel_count
        );

        // Best-effort cleanup. Leaks across panic are tolerable —
        // temp dirs are GC'd by the OS eventually.
        drop(std::fs::remove_file(&staged_path));
        drop(std::fs::remove_dir(&tmp_dir));
    }

    // -----------------------------------------------------------------------
    // ISSUE-258 follow-up / SCENE-OPEN-WIRING — `glb_watcher_action` decision
    // -----------------------------------------------------------------------
    //
    // Pure truth-table coverage of the watcher reconciliation decision
    // used by `EditorApp::sync_glb_watcher`. No winit, no real `notify`
    // watcher, no filesystem — plain `PathBuf` literals only.

    #[test]
    fn action_no_source_no_watch_is_unchanged() {
        // (a) Nothing to follow and nothing watched — the default cuboid
        // demo / `--scene` start before any in-app Open. No-op.
        assert_eq!(glb_watcher_action(None, None), GlbWatcherAction::Unchanged);
    }

    #[test]
    fn action_tears_down_when_source_cleared() {
        // (a') source None + watched Some → Teardown. An in-app scene
        // Open swapped in a world with no GLB source and cleared
        // `glb_source_path`; the old-file watcher must stop rather than
        // keep hot-reloading a superseded file. (Pre-SCENE-OPEN-WIRING
        // this case was a no-op `None`, leaving the stale watcher live.)
        assert_eq!(
            glb_watcher_action(Some(Path::new("A:/assets/cube.glb")), None),
            GlbWatcherAction::Teardown
        );
    }

    #[test]
    fn action_first_root_after_open_from_demo() {
        // (b) source Some + watched None → Reroot(source). First-root
        // case: an in-app Open from the demo (which launched with no
        // `--glb`, so nothing is watched yet) must root the watcher
        // onto the freshly-opened file.
        let source = Path::new("A:/assets/opened.glb");
        assert_eq!(
            glb_watcher_action(None, Some(source)),
            GlbWatcherAction::Reroot(PathBuf::from("A:/assets/opened.glb"))
        );
    }

    #[test]
    fn action_reroots_onto_different_path() {
        // (c) source Some + watched a DIFFERENT path → Reroot(source).
        // Re-root case: the editor launched with `--glb a.glb`, then
        // the user opened `b.glb` in-app. The watcher must follow to
        // `b.glb`.
        let watched = Path::new("A:/assets/a.glb");
        let source = Path::new("A:/assets/b.glb");
        assert_eq!(
            glb_watcher_action(Some(watched), Some(source)),
            GlbWatcherAction::Reroot(PathBuf::from("A:/assets/b.glb"))
        );
    }

    #[test]
    fn action_stable_when_source_equals_watched() {
        // (d) source Some == watched Some → Unchanged. Steady state: the
        // watcher is already rooted on the live source, so no rebuild
        // (this is the per-event common case — `sync_glb_watcher` runs
        // every `window_event` and must NOT churn the watcher when
        // nothing changed).
        let same = Path::new("A:/assets/cube.glb");
        assert_eq!(
            glb_watcher_action(Some(same), Some(same)),
            GlbWatcherAction::Unchanged
        );
    }

    // -----------------------------------------------------------------------
    // Dispatch M3 — normal-matrix baking + input-normal propagation tests
    // -----------------------------------------------------------------------

    #[test]
    fn bake_normals_identity_leaves_normals_unchanged() {
        let normals = vec![[1.0_f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let baked = bake_normals(&normals, &Mat4::IDENTITY);
        assert_eq!(baked.len(), 3);
        for (i, b) in baked.iter().enumerate() {
            assert!(vec3_approx_eq(
                Vec3::from_array(*b),
                Vec3::from_array(normals[i])
            ));
        }
    }

    #[test]
    fn bake_normals_rotation_transforms_normals_correctly() {
        // 90° about Y: [1, 0, 0] → [0, 0, -1] (RH coordinates).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let rot = Mat4::from_quat(Quat::from_xyzw(0.0, s, 0.0, s));
        let normals = vec![[1.0_f32, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let baked = bake_normals(&normals, &rot);
        // x-hat → -z-hat (Y is the rotation axis, unchanged).
        assert!(vec3_approx_eq(
            Vec3::from_array(baked[0]),
            Vec3::new(0.0, 0.0, -1.0)
        ));
        assert!(vec3_approx_eq(
            Vec3::from_array(baked[1]),
            Vec3::new(0.0, 1.0, 0.0)
        ));
    }

    #[test]
    fn bake_normals_non_uniform_scale_uses_inverse_transpose() {
        // Scale [2, 1, 1]. A normal at 45° in the XY plane —
        // input `(1, 1, 0)` normalized = `(0.707, 0.707, 0)` —
        // should NOT just get scaled in X by 2 (that would make
        // the rotated surface look misaligned). Inverse-transpose
        // of scale `(2, 1, 1)` is scale `(0.5, 1, 1)` (diagonal
        // matrix's inverse is the reciprocal-diagonal; transpose
        // of a diagonal matrix is itself). So input `(1, 1, 0)`
        // becomes `(0.5, 1, 0)`, then normalized = `(0.447,
        // 0.894, 0)`.
        let world = Mat4::from_scale(Vec3::new(2.0, 1.0, 1.0));
        let input_normal = Vec3::new(1.0, 1.0, 0.0).normalize().to_array();
        let baked = bake_normals(&[input_normal], &world);
        let expected = Vec3::new(0.5, 1.0, 0.0).normalize();
        assert!(
            vec3_approx_eq(Vec3::from_array(baked[0]), expected),
            "non-uniform scale should use inverse-transpose; got {:?}, expected {expected:?}",
            baked[0]
        );
    }

    #[test]
    fn bake_normals_re_normalizes_per_vertex() {
        // Input is non-unit-length; output must be unit-length.
        // (The world matrix is identity here so the only work is
        // the `normalize_or_zero` step.)
        let normals = vec![[3.0_f32, 0.0, 4.0]]; // length 5
        let baked = bake_normals(&normals, &Mat4::IDENTITY);
        let len = Vec3::from_array(baked[0]).length();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "baked normal must be unit-length; got {len}"
        );
    }

    #[test]
    fn load_all_glb_meshes_preserves_input_normals_when_present() {
        // cube.glb's `make_cube_glb` pushes per-face outward
        // normals (`[1, 0, 0]` for +X face, etc.). After M3 these
        // input normals reach the RenderMesh output instead of
        // being replaced by cross-product flat normals. With
        // cube.glb's per-face uniform normals, the input and the
        // cross-product flat normals happen to be equal — the
        // test below confirms the OUTPUT normals are non-empty
        // and finite, which is the substrate-correctness property.
        // A future smooth-normal fixture would distinguish input
        // vs recomputed values visibly.
        let path = fixtures_path().join("cube.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }
        let (meshes, _, _) = load_all_glb_meshes(&path).expect("load cube.glb");
        assert_eq!(meshes.len(), 1);
        // 24 input verts × 12 input tris × 3 = 36 output verts.
        // Cube.glb has 8 unique positions but per-face split = 24
        // input verts; brep-render vertex-tripling gives 36 output.
        // The exact output count depends on cube.glb's per-face
        // unwrap — assert non-empty + all finite + all unit-length.
        assert!(!meshes[0].normals.is_empty());
        assert_eq!(meshes[0].normals.len(), meshes[0].positions.len());
        for n in &meshes[0].normals {
            assert!(
                n.iter().all(|c| c.is_finite()),
                "normal must be finite: {n:?}"
            );
            let len = Vec3::from_array(*n).length();
            assert!(
                (len - 1.0).abs() < 1e-4,
                "baked normal must be unit-length: {n:?} (len={len})"
            );
        }
    }

    #[test]
    fn load_all_glb_meshes_falls_back_to_flat_normals_without_input() {
        // Build a synthetic GLB with positions but NO NORMAL accessor.
        // After load, the RenderMesh's normals must be the cross-
        // product flat normals from the (world-baked) positions.
        use rge_io_gltf::{export_glb, EntityComponents, MeshAsset, Scene};

        let mut cache = MemoryCache::new();
        let tri = MeshAsset {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![], // empty — no NORMAL accessor
            texcoords: vec![],
            indices: vec![0, 1, 2],
            material_index: None,
        };
        let mh = cache.insert_mesh(tri);
        let mut scene = Scene::new();
        scene.spawn(EntityComponents {
            name: "flat".into(),
            transform: rge_io_gltf::Transform::IDENTITY,
            parent: Entity::ROOT,
            mesh: Some(mh),
            material: None,
            skeleton: None,
        });

        let bytes = export_glb(&scene, &cache).expect("export");
        let path = std::env::temp_dir().join("rge_editor_m3_no_normals.glb");
        std::fs::write(&path, &bytes).expect("write");

        let (meshes, _, _) = load_all_glb_meshes(&path).expect("load");
        assert_eq!(meshes.len(), 1);
        // CCW XY plane → cross-product flat normal = +Z = [0, 0, 1].
        for n in &meshes[0].normals {
            assert!((n[0] - 0.0).abs() < 1e-5);
            assert!((n[1] - 0.0).abs() < 1e-5);
            assert!((n[2] - 1.0).abs() < 1e-5);
        }

        drop(std::fs::remove_file(&path));
    }

    // -----------------------------------------------------------------------
    // ISSUE-87 — smooth-normal sphere imported-vs-flat-recompute readback
    //
    // The cube fixture's NORMAL accessor happens to match flat
    // recompute byte-for-byte, so the existing
    // `load_all_glb_meshes_preserves_input_normals_when_present`
    // test only proves the normals reach the RenderMesh — it cannot
    // distinguish the imported-NORMAL path from the recompute path
    // visibly. The smooth-normal sphere fixture closes that
    // coverage gap: per-vertex smooth normals (= normalized radial
    // vector) differ meaningfully from per-triangle cross-product
    // flat normals across every triangle, so Lambert+Phong shading
    // produces a numerically distinct central-row pixel signature
    // between the two paths.
    // -----------------------------------------------------------------------

    /// **ISSUE-87 acceptance gate.** Loads `smooth_normal_sphere.glb`
    /// through the production `load_all_glb_meshes` path (imported
    /// NORMAL accessor → vertex-tripled smooth normals), then
    /// reconstructs the same geometry test-locally with `None`
    /// normals passed to `RenderMesh::from_buffers_with_attributes`
    /// (forces cross-product flat-recompute). Renders both shells
    /// through `render_one_frame_to_readback` and asserts the
    /// central horizontal row's accumulated absolute RGB delta
    /// clears a driver-noise-resistant numeric threshold. Failing
    /// this test means either (a) the M3 imported-NORMAL path
    /// regressed to recomputing flat normals, or (b) the
    /// recompute path picked up imported normals — both of which
    /// would silently break smooth shading on real glTF imports.
    #[test]
    fn smooth_normal_sphere_imported_vs_flat_recompute_visibly_differs_end_to_end() {
        let _gpu_lock = gpu_test_lock();
        use rge_io_gltf::import_glb;

        let path = fixtures_path().join("smooth_normal_sphere.glb");
        if skip_if_fixture_missing(&path) {
            return;
        }

        // Path A — imported smooth normals via the real loader.
        let (meshes_imp, base_colors, textures) =
            load_all_glb_meshes(&path).expect("load_all_glb_meshes(smooth_normal_sphere.glb)");
        let textures_for_shell_imp: Vec<Option<(u32, u32, Vec<u8>)>> = textures
            .into_iter()
            .map(|t| t.map(|t| (t.width, t.height, t.pixels)))
            .collect();
        let mut shell_imp = EditorShell::with_render_meshes_and_base_colors_and_textures(
            meshes_imp,
            base_colors.clone(),
            textures_for_shell_imp,
        );
        let Some(buf_imp) = render_shell_one_frame(&mut shell_imp) else {
            return;
        };

        // Path B — re-import the fixture directly, world-bake
        // positions exactly the way `load_all_glb_meshes` does, then
        // construct the RenderMesh with `normals = None` so
        // brep-render recomputes flat normals from the (same) baked
        // positions. Material / texture inputs match Path A so the
        // ONLY observable difference is the per-fragment normal.
        let mut cache_flat = MemoryCache::new();
        let scene_flat = import_glb(&path, &mut cache_flat)
            .expect("import_glb(smooth_normal_sphere.glb) for flat-recompute path");
        let (entity, mh) = scene_flat
            .iter()
            .find_map(|(e, comps)| comps.mesh.map(|m| (e, m)))
            .expect("scene carries one mesh-bearing entity");
        let world_flat = accumulate_world_transform(&scene_flat, entity);
        let mesh_asset_flat = cache_flat
            .get_mesh(&mh)
            .expect("flat-path mesh present in cache");
        let baked_flat = bake_positions(&mesh_asset_flat.positions, &world_flat);
        let mesh_flat = RenderMesh::from_buffers_with_attributes(
            &baked_flat,
            &mesh_asset_flat.indices,
            None,
            None, // forces brep-render flat recompute from baked positions
            None,
        );
        let mut shell_flat = EditorShell::with_render_meshes_and_base_colors_and_textures(
            vec![mesh_flat],
            base_colors,
            vec![None],
        );
        let Some(buf_flat) = render_shell_one_frame(&mut shell_flat) else {
            return;
        };

        // Central-row pixel comparison. A pixel counts as on-mesh
        // when EITHER render's per-channel delta from DEFAULT_CLEAR
        // exceeds CUBE_THRESHOLD on at least one channel — that way
        // we count the union of sphere footprints, then accumulate
        // the imported-vs-flat absolute delta over pixels that
        // landed on the sphere in both renders (the intersection).
        let y = VISUAL_H / 2;
        let mut common_on_mesh_pixels: u32 = 0;
        let mut sum_abs_rgb_delta: u64 = 0;
        let mut max_single_pixel_delta: u32 = 0;
        for x in 0..VISUAL_W {
            let pi = buf_imp.pixel(x, y).expect("imported-row pixel");
            let pf = buf_flat.pixel(x, y).expect("flat-row pixel");
            let on_mesh = |p: (u8, u8, u8, u8)| -> bool {
                (i32::from(p.0) - BG_R).abs() > CUBE_THRESHOLD
                    || (i32::from(p.1) - BG_G).abs() > CUBE_THRESHOLD
                    || (i32::from(p.2) - BG_B).abs() > CUBE_THRESHOLD
            };
            if on_mesh(pi) && on_mesh(pf) {
                common_on_mesh_pixels += 1;
                let dr = (i32::from(pi.0) - i32::from(pf.0)).unsigned_abs();
                let dg = (i32::from(pi.1) - i32::from(pf.1)).unsigned_abs();
                let db = (i32::from(pi.2) - i32::from(pf.2)).unsigned_abs();
                let pixel_delta = dr + dg + db;
                sum_abs_rgb_delta += u64::from(pixel_delta);
                max_single_pixel_delta = max_single_pixel_delta.max(pixel_delta);
            }
        }

        eprintln!(
            "ISSUE-87 smooth_normal_sphere central-row metrics: \
             common_on_mesh_pixels={common_on_mesh_pixels} \
             sum_abs_rgb_delta={sum_abs_rgb_delta} \
             max_single_pixel_delta={max_single_pixel_delta}"
        );

        // Gate 1 — enough commonly-on-mesh pixels to make the
        // distribution comparison meaningful. The sphere occupies
        // the central viewport region under the auto-frame camera,
        // so the central row crosses the sphere across dozens of
        // pixels; >8 is the protocol-required floor.
        assert!(
            common_on_mesh_pixels > 8,
            "smooth_normal_sphere central row must hit the sphere in both \
             imported-normal and flat-recompute renders; \
             got common_on_mesh_pixels={common_on_mesh_pixels}"
        );

        // Gate 2 — driver-noise-resistant numeric threshold. Per
        // ISSUE-87 task packet: `sum_abs_rgb_delta > 300` is the
        // documented absolute floor large enough to reject driver
        // rounding noise. Smooth-vs-flat on a 12×18 sphere lit by
        // a directional light easily clears 1000+ on a 256×256
        // viewport; 300 is the smallest literal threshold that
        // still rejects driver-noise-only deltas.
        assert!(
            sum_abs_rgb_delta > 300,
            "ISSUE-87: imported-normal vs flat-recompute central-row \
             sum_abs_rgb_delta must exceed 300 (driver-noise floor); \
             got sum_abs_rgb_delta={sum_abs_rgb_delta} \
             common_on_mesh_pixels={common_on_mesh_pixels} \
             max_single_pixel_delta={max_single_pixel_delta}"
        );
    }

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
