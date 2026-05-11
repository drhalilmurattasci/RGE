//! Gfx-side adapter realising semantic [`MaterialDescriptor`]s into the
//! renderer's [`PsoKey`] + [`Material`] substrate.
//!
//! Failure class: recoverable
//!
//! # What this module IS
//!
//! The "intent → concrete" surface that closes the
//! [`plans/IMPLEMENTATION.md`](../../../plans/IMPLEMENTATION.md) §6.3 exit
//! gate ("100 material instances share one PSO"). The
//! [`rge-material-runtime`] crate (sibling Tier-2 utility) defines the
//! pure-intent five-axis [`MaterialDescriptor`]; this module maps each axis
//! to its gfx-side concrete value:
//!
//! | Intent axis ([`MaterialDescriptor`]) | Concrete gfx-side value |
//! |-------------------------------------|-------------------------|
//! | [`ShaderId`]        | [`ShaderHash`] via the matching pipeline-module helper |
//! | [`VertexLayoutId`]  | [`VertexLayoutDescriptor`] via the matching pipeline-module helper |
//! | [`ColorTargetId`]   | [`wgpu::TextureFormat`] |
//! | [`DepthIntent`]     | `Option<DepthStateKey>` |
//! | [`MaterialParams`]  | UBO payload — consumed by [`Material::from_descriptor`] |
//!
//! [`intent_to_pso_key`] is **total**: every legal [`MaterialDescriptor`]
//! produces a valid [`PsoKey`] (no `Result`, no `Option`).
//!
//! # Pipeline build dispatch
//!
//! [`build_pipeline_from_intent`] dispatches on the descriptor's
//! [`ShaderId`] and routes through the pipeline kind's `*::new_cached`
//! constructor against the [`GfxContext`]-owned [`PipelineCache`]. The
//! caller supplies the bind-group layouts the chosen pipeline kind
//! requires via [`PipelineLayouts`] — missing required layouts surface
//! as [`BuildIntentError::MissingLayout`] rather than panic.
//!
//! # Non-goals
//!
//! - **No texture binding.** v0 [`MaterialDescriptor`] has no texture axis;
//!   [`Material::from_descriptor`] uses a 1×1 white placeholder. A future
//!   texture-bound material variant lands when a real texturing consumer
//!   arrives.
//! - **No depth-attachment-format parameter.** [`DepthIntent::ReadWrite`]
//!   / `ReadOnly` map to `Depth32Float` + `CompareFunction::LessEqual`
//!   today — the canonical depth setup. The format axis becomes
//!   parameterisable when depth buffers actually land in `gfx`.
//! - **No new public types beyond the adapter surface.** [`PipelineLayouts`]
//!   is a thin POD borrow-bundle, not a new substrate type.
//!
//! [`MaterialDescriptor`]: rge_material_runtime::MaterialDescriptor
//! [`ShaderId`]: rge_material_runtime::ShaderId
//! [`VertexLayoutId`]: rge_material_runtime::VertexLayoutId
//! [`ColorTargetId`]: rge_material_runtime::ColorTargetId
//! [`DepthIntent`]: rge_material_runtime::DepthIntent
//! [`MaterialParams`]: rge_material_runtime::MaterialParams

use std::sync::Arc;

use rge_material_runtime::{
    ColorTargetId, DepthIntent, MaterialDescriptor, ShaderId, VertexLayoutId,
};

use crate::context::GfxContext;
use crate::lit_mesh_pipeline::{
    lit_mesh_shader_hash, vertex_layout_descriptor_for_lit_vertex, LitMeshPipeline,
    LitMeshPipelineError,
};
use crate::mesh_pipeline::{
    mesh_shader_hash, vertex_layout_descriptor_for_vertex, MeshPipeline, MeshPipelineError,
};
use crate::pipeline::{
    triangle_shader_hash, triangle_vertex_layout_descriptor, PipelineError, TrianglePipeline,
};
use crate::pso_cache::{DepthStateKey, PsoKey, ShaderHash, VertexLayoutDescriptor};

// ---------------------------------------------------------------------------
// Intent → concrete realisation (per-axis)
// ---------------------------------------------------------------------------

/// Canonical depth-attachment format for the [`DepthIntent::ReadWrite`] /
/// [`DepthIntent::ReadOnly`] variants.
///
/// v0 has no depth buffer in production; this is the planned default when
/// depth lands. Parameterised once a real consumer needs a different
/// depth format.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Canonical depth comparison for the populated [`DepthIntent`] variants.
const DEPTH_COMPARE: wgpu::CompareFunction = wgpu::CompareFunction::LessEqual;

/// Realise a [`ShaderId`] into its [`ShaderHash`].
#[must_use]
pub fn shader_id_to_hash(id: ShaderId) -> ShaderHash {
    match id {
        ShaderId::Triangle => triangle_shader_hash(),
        ShaderId::Mesh => mesh_shader_hash(),
        ShaderId::LitMesh => lit_mesh_shader_hash(),
    }
}

/// Realise a [`VertexLayoutId`] into its [`VertexLayoutDescriptor`].
#[must_use]
pub fn vertex_layout_id_to_descriptor(id: VertexLayoutId) -> VertexLayoutDescriptor {
    match id {
        VertexLayoutId::Empty => triangle_vertex_layout_descriptor(),
        VertexLayoutId::Vertex => vertex_layout_descriptor_for_vertex(),
        VertexLayoutId::LitVertex => vertex_layout_descriptor_for_lit_vertex(),
    }
}

/// Realise a [`ColorTargetId`] into its [`wgpu::TextureFormat`].
#[must_use]
pub fn color_target_id_to_format(id: ColorTargetId) -> wgpu::TextureFormat {
    match id {
        ColorTargetId::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        ColorTargetId::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
    }
}

/// Realise a [`DepthIntent`] into its [`Option<DepthStateKey>`].
///
/// - [`DepthIntent::None`] → `None` (no depth attachment)
/// - [`DepthIntent::ReadWrite`] → `Some(DepthStateKey { Depth32Float, write=true, LessEqual })`
/// - [`DepthIntent::ReadOnly`]  → `Some(DepthStateKey { Depth32Float, write=false, LessEqual })`
#[must_use]
pub fn depth_intent_to_key(intent: DepthIntent) -> Option<DepthStateKey> {
    match intent {
        DepthIntent::None => None,
        DepthIntent::ReadWrite => Some(DepthStateKey::new(DEPTH_FORMAT, true, DEPTH_COMPARE)),
        DepthIntent::ReadOnly => Some(DepthStateKey::new(DEPTH_FORMAT, false, DEPTH_COMPARE)),
    }
}

// ---------------------------------------------------------------------------
// intent_to_pso_key — the total mapping
// ---------------------------------------------------------------------------

/// Total mapping from a semantic [`MaterialDescriptor`] to a concrete
/// [`PsoKey`].
///
/// No `Result`, no `Option`. Every [`MaterialDescriptor`] produces a
/// valid [`PsoKey`]; identity is preserved (two descriptors comparing
/// equal produce keys comparing equal, per the load-bearing
/// `identical_descriptors_are_equal_and_hash_equal` contract that
/// `rge-material-runtime` pins in its own unit tests).
#[must_use]
pub fn intent_to_pso_key(desc: &MaterialDescriptor) -> PsoKey {
    PsoKey::new(
        shader_id_to_hash(desc.shader_id),
        vertex_layout_id_to_descriptor(desc.vertex_layout),
        color_target_id_to_format(desc.color_target),
        depth_intent_to_key(desc.depth),
    )
}

// ---------------------------------------------------------------------------
// build_pipeline_from_intent
// ---------------------------------------------------------------------------

/// Bind-group layouts the various pipeline kinds need at build time.
///
/// Borrow-bundle so a single caller can pass the right subset for whichever
/// [`ShaderId`] the descriptor selects:
///
/// | `ShaderId`  | Required fields                                |
/// |-------------|------------------------------------------------|
/// | `Triangle`  | none                                           |
/// | `Mesh`      | [`transform`](PipelineLayouts::transform)      |
/// | `LitMesh`   | [`camera`](PipelineLayouts::camera) + [`light`](PipelineLayouts::light) + [`material`](PipelineLayouts::material) |
///
/// Missing required layouts surface as [`BuildIntentError::MissingLayout`].
#[derive(Default, Clone, Copy)]
pub struct PipelineLayouts<'a> {
    /// Bind-group layout for the `MeshPipeline` transform UBO at `@group(0)`.
    pub transform: Option<&'a wgpu::BindGroupLayout>,
    /// Bind-group layout for the `LitMeshPipeline` camera UBO at `@group(0)`.
    pub camera: Option<&'a wgpu::BindGroupLayout>,
    /// Bind-group layout for the `LitMeshPipeline` directional-light UBO at `@group(1)`.
    pub light: Option<&'a wgpu::BindGroupLayout>,
    /// Bind-group layout for the `LitMeshPipeline` material UBO + texture +
    /// sampler at `@group(2)`.
    pub material: Option<&'a wgpu::BindGroupLayout>,
}

/// Errors that can occur in [`build_pipeline_from_intent`].
#[derive(Debug, thiserror::Error)]
pub enum BuildIntentError {
    /// A required bind-group layout was missing for the descriptor's
    /// [`ShaderId`]. The string names the missing layout
    /// (e.g. `"transform"`, `"camera"`, `"light"`, `"material"`).
    #[error("missing bind-group layout for intent: {0}")]
    MissingLayout(&'static str),

    /// Wraps an underlying pipeline-kind WGSL compile error.
    #[error("triangle pipeline build failed: {0}")]
    Triangle(#[from] PipelineError),

    /// Wraps an underlying [`MeshPipeline`] WGSL compile error.
    #[error("mesh pipeline build failed: {0}")]
    Mesh(#[from] MeshPipelineError),

    /// Wraps an underlying [`LitMeshPipeline`] WGSL compile error.
    #[error("lit-mesh pipeline build failed: {0}")]
    LitMesh(#[from] LitMeshPipelineError),
}

/// Build (or reuse from cache) the `wgpu::RenderPipeline` for the
/// [`MaterialDescriptor`]'s pipeline kind, returning the shared
/// `Arc<wgpu::RenderPipeline>`.
///
/// Routes through the [`GfxContext`]-owned [`PipelineCache`] (via each
/// pipeline kind's `*::new_cached` constructor), so identical
/// [`MaterialDescriptor`]s produce a single cache insert + hits on
/// subsequent calls — this is the substrate that closes the §6.3 gate.
///
/// # Layout requirements
///
/// - [`ShaderId::Triangle`] — no layouts required (vertex shader hard-codes positions).
/// - [`ShaderId::Mesh`] — requires [`PipelineLayouts::transform`].
/// - [`ShaderId::LitMesh`] — requires [`PipelineLayouts::camera`] +
///   [`PipelineLayouts::light`] + [`PipelineLayouts::material`].
///
/// # Errors
///
/// - [`BuildIntentError::MissingLayout`] when a required layout is `None`.
/// - [`BuildIntentError::Triangle`] / [`Mesh`](BuildIntentError::Mesh) /
///   [`LitMesh`](BuildIntentError::LitMesh) wrap the pipeline kind's WGSL
///   compile error (should not occur with the embedded shaders).
///
/// [`PipelineCache`]: crate::pso_cache::PipelineCache
pub fn build_pipeline_from_intent(
    ctx: &GfxContext,
    desc: &MaterialDescriptor,
    layouts: &PipelineLayouts<'_>,
) -> Result<Arc<wgpu::RenderPipeline>, BuildIntentError> {
    let color_format = color_target_id_to_format(desc.color_target);
    // Bind PSO cache lock for the whole call so a single concurrent caller
    // sees a consistent (miss, then hit) sequence.
    let mut cache = ctx.pso_cache().borrow_mut();
    match desc.shader_id {
        ShaderId::Triangle => {
            let pipeline = TrianglePipeline::new_cached(ctx, color_format, &mut cache)?;
            Ok(pipeline.pipeline_arc())
        }
        ShaderId::Mesh => {
            let transform_layout = layouts
                .transform
                .ok_or(BuildIntentError::MissingLayout("transform"))?;
            let pipeline =
                MeshPipeline::new_cached(ctx, transform_layout, color_format, &mut cache)?;
            Ok(pipeline.pipeline_arc())
        }
        ShaderId::LitMesh => {
            let camera_layout = layouts
                .camera
                .ok_or(BuildIntentError::MissingLayout("camera"))?;
            let light_layout = layouts
                .light
                .ok_or(BuildIntentError::MissingLayout("light"))?;
            let material_layout = layouts
                .material
                .ok_or(BuildIntentError::MissingLayout("material"))?;
            let pipeline = LitMeshPipeline::new_cached(
                ctx,
                camera_layout,
                light_layout,
                material_layout,
                color_format,
                &mut cache,
            )?;
            Ok(pipeline.pipeline_arc())
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests (pure-logic; no GPU)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_material_runtime::MaterialParams;

    use super::*;

    fn descriptor_lit_bgra_no_depth() -> MaterialDescriptor {
        MaterialDescriptor {
            shader_id: ShaderId::LitMesh,
            vertex_layout: VertexLayoutId::LitVertex,
            color_target: ColorTargetId::Bgra8UnormSrgb,
            depth: DepthIntent::None,
            params: MaterialParams::default(),
        }
    }

    // --- shader_id_to_hash --------------------------------------------------

    #[test]
    fn shader_id_to_hash_deterministic() {
        assert_eq!(
            shader_id_to_hash(ShaderId::Triangle),
            shader_id_to_hash(ShaderId::Triangle)
        );
        assert_eq!(
            shader_id_to_hash(ShaderId::Mesh),
            shader_id_to_hash(ShaderId::Mesh)
        );
        assert_eq!(
            shader_id_to_hash(ShaderId::LitMesh),
            shader_id_to_hash(ShaderId::LitMesh)
        );
    }

    #[test]
    fn shader_id_to_hash_distinct_per_variant() {
        let t = shader_id_to_hash(ShaderId::Triangle);
        let m = shader_id_to_hash(ShaderId::Mesh);
        let l = shader_id_to_hash(ShaderId::LitMesh);
        assert_ne!(t, m);
        assert_ne!(m, l);
        assert_ne!(t, l);
    }

    // --- color_target_id_to_format ------------------------------------------

    #[test]
    fn color_target_id_to_format_matches_real_callers() {
        assert_eq!(
            color_target_id_to_format(ColorTargetId::Rgba8Unorm),
            wgpu::TextureFormat::Rgba8Unorm
        );
        assert_eq!(
            color_target_id_to_format(ColorTargetId::Bgra8UnormSrgb),
            wgpu::TextureFormat::Bgra8UnormSrgb
        );
    }

    // --- depth_intent_to_key ------------------------------------------------

    #[test]
    fn depth_intent_none_maps_to_none() {
        assert_eq!(depth_intent_to_key(DepthIntent::None), None);
    }

    #[test]
    fn depth_intent_read_write_has_write_enabled() {
        let key = depth_intent_to_key(DepthIntent::ReadWrite).expect("Some");
        assert_eq!(key.format, DEPTH_FORMAT);
        assert!(key.depth_write_enabled);
        assert_eq!(key.depth_compare, DEPTH_COMPARE);
    }

    #[test]
    fn depth_intent_read_only_has_write_disabled() {
        let key = depth_intent_to_key(DepthIntent::ReadOnly).expect("Some");
        assert_eq!(key.format, DEPTH_FORMAT);
        assert!(!key.depth_write_enabled);
        assert_eq!(key.depth_compare, DEPTH_COMPARE);
    }

    #[test]
    fn depth_intent_read_write_and_read_only_keys_distinct() {
        let rw = depth_intent_to_key(DepthIntent::ReadWrite);
        let ro = depth_intent_to_key(DepthIntent::ReadOnly);
        assert_ne!(rw, ro);
    }

    // --- intent_to_pso_key --------------------------------------------------

    #[test]
    fn identical_descriptors_produce_equal_pso_keys() {
        let a = descriptor_lit_bgra_no_depth();
        let b = descriptor_lit_bgra_no_depth();
        assert_eq!(intent_to_pso_key(&a), intent_to_pso_key(&b));
    }

    #[test]
    fn differing_shader_id_produces_distinct_pso_key() {
        let a = descriptor_lit_bgra_no_depth();
        let mut b = descriptor_lit_bgra_no_depth();
        b.shader_id = ShaderId::Mesh;
        assert_ne!(intent_to_pso_key(&a), intent_to_pso_key(&b));
    }

    #[test]
    fn differing_color_target_produces_distinct_pso_key() {
        let a = descriptor_lit_bgra_no_depth();
        let mut b = descriptor_lit_bgra_no_depth();
        b.color_target = ColorTargetId::Rgba8Unorm;
        assert_ne!(intent_to_pso_key(&a), intent_to_pso_key(&b));
    }

    #[test]
    fn differing_depth_intent_produces_distinct_pso_key() {
        let a = descriptor_lit_bgra_no_depth();
        let mut b = descriptor_lit_bgra_no_depth();
        b.depth = DepthIntent::ReadWrite;
        assert_ne!(intent_to_pso_key(&a), intent_to_pso_key(&b));
    }

    #[test]
    fn differing_vertex_layout_produces_distinct_pso_key() {
        let a = descriptor_lit_bgra_no_depth();
        let mut b = descriptor_lit_bgra_no_depth();
        b.vertex_layout = VertexLayoutId::Vertex;
        assert_ne!(intent_to_pso_key(&a), intent_to_pso_key(&b));
    }

    #[test]
    fn params_do_not_affect_pso_key() {
        // PSO identity is governed by the four pipeline axes only; the UBO
        // payload (`params`) does NOT affect compiled pipeline state. Two
        // descriptors differing only in `params` must produce the same key.
        let a = descriptor_lit_bgra_no_depth();
        let mut b = descriptor_lit_bgra_no_depth();
        b.params.base_color = [0.5, 0.25, 0.125, 1.0];
        b.params.phong = [0.2, 0.8, 0.3, 16.0];
        assert_eq!(intent_to_pso_key(&a), intent_to_pso_key(&b));
    }
}
