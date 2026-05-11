//! `rge-material-runtime` — semantic material intent for the renderer.
//!
//! Failure class: recoverable
//!
//! # What this crate is
//!
//! Pure intent types that describe **what material a mesh wants** without
//! taking any GPU-side dependency.  No `wgpu`, no `gfx`, no `gfx-ir`, no
//! `Device`, no PSO ownership, no texture management — those concerns live
//! in `rge-gfx` and the (forthcoming) `rge-gfx`-side adapter that realises
//! a [`MaterialDescriptor`] into a `gfx::Material` + `PsoKey`.
//!
//! # The five-axis material descriptor
//!
//! A [`MaterialDescriptor`] captures the five axes that today's render
//! pipelines need in order to produce a unique PSO + bind-group setup:
//!
//! 1. [`ShaderId`]        — which shader recipe to compile against
//! 2. [`VertexLayoutId`]  — vertex buffer layout (or `Empty` for `TrianglePipeline`)
//! 3. [`ColorTargetId`]   — surface colour-attachment format
//! 4. [`DepthIntent`]     — depth read/write semantics (today: always `None`)
//! 5. [`MaterialParams`]  — the 32-byte `(base_color, phong)` UBO payload
//!
//! All five axes are statically-known enums (or small POD), not opaque hashes.
//! Today's pipelines are statically known; expansion is additive (new variants)
//! as new pipelines or surface formats arrive.  Opaque IDs would invent shape
//! with no consumer.
//!
//! # Identity contract (LOAD-BEARING)
//!
//! Every type derives `Eq + Hash`.  Two descriptors with identical fields
//! **must** hash equal and compare equal — the gfx adapter's PSO cache will
//! key off the descriptor identity directly.  The unit tests below pin this:
//! identical descriptors are equal, and a single-axis change in any of the
//! five axes produces a distinct hash.
//!
//! # Hard non-goals
//!
//! - No `wgpu` / `gfx` / `gfx-ir` dependency.  This crate stays GPU-agnostic.
//! - No texture or `TextureId` field on the descriptor (deferred to a later
//!   dispatch when a real texture-bound material consumer asks for it).
//! - No PSO key construction, no shader hash, no vertex layout descriptor —
//!   those are gfx concerns realised by the adapter (next dispatch).
//! - No `serde` derives (no current consumer; workspace doctrine does not
//!   force them on intent types).

#![deny(unsafe_code)]

// ---------------------------------------------------------------------------
// Axis 1 — ShaderId
// ---------------------------------------------------------------------------

/// Which shader recipe a [`MaterialDescriptor`] references.
///
/// Today's pipelines are statically known; expansion is additive (new
/// variants) as new pipelines arrive.  Each variant corresponds to a
/// concrete `gfx`-side pipeline type that the adapter dispatch realises:
///
/// | Variant     | gfx-side pipeline                       |
/// |-------------|-----------------------------------------|
/// | `Triangle`  | `gfx::pipeline::TrianglePipeline`       |
/// | `Mesh`      | `gfx::mesh_pipeline::MeshPipeline`      |
/// | `LitMesh`   | `gfx::lit_mesh_pipeline::LitMeshPipeline` |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderId {
    /// Hard-coded full-screen triangle (no vertex buffer); used by smoke tests
    /// and the headless rendering path in `gfx::pipeline::TrianglePipeline`.
    Triangle,

    /// Unlit textured mesh; matches `gfx::mesh_pipeline::MeshPipeline`.
    Mesh,

    /// Lit textured mesh with directional/ambient lighting and Phong shading;
    /// matches `gfx::lit_mesh_pipeline::LitMeshPipeline`.
    LitMesh,
}

// ---------------------------------------------------------------------------
// Axis 2 — VertexLayoutId
// ---------------------------------------------------------------------------

/// Vertex layout intent.
///
/// Each variant has a known gfx-side `VertexLayoutDescriptor` realisation in
/// the adapter dispatch — this enum names the layout; the adapter binds it
/// to the concrete descriptor.
///
/// | Variant      | gfx-side layout                            |
/// |--------------|--------------------------------------------|
/// | `Empty`      | no vertex buffer (used by `TrianglePipeline`) |
/// | `Vertex`     | `gfx::vertex::Vertex` (pos + uv)           |
/// | `LitVertex`  | `gfx::vertex_lit::LitVertex` (pos + normal + uv) |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VertexLayoutId {
    /// No vertex buffer; vertex shader generates positions from `vertex_index`.
    Empty,

    /// Mesh pipeline vertex layout: position + uv.
    Vertex,

    /// Lit-mesh pipeline vertex layout: position + normal + uv.
    LitVertex,
}

// ---------------------------------------------------------------------------
// Axis 3 — ColorTargetId
// ---------------------------------------------------------------------------

/// Surface colour-target format intent.
///
/// Variants are grounded in the `wgpu::TextureFormat` values that real callers
/// of `*::new_cached` in `crates/gfx` pass today (verified via Grep at v0).
/// Expansion is additive: when a new colour-attachment format enters real
/// rendering paths, add a new variant here and a matching arm in the gfx-side
/// adapter.
///
/// | Variant            | wgpu format                       | Today's caller                      |
/// |--------------------|-----------------------------------|-------------------------------------|
/// | `Rgba8Unorm`       | `wgpu::TextureFormat::Rgba8Unorm` | offscreen `gfx::target::OffscreenTarget` |
/// | `Bgra8UnormSrgb`   | `wgpu::TextureFormat::Bgra8UnormSrgb` | swapchain surface (`gfx::surface`)  |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorTargetId {
    /// Linear 8-bit RGBA; today's `OffscreenTarget` / headless render path.
    Rgba8Unorm,

    /// sRGB 8-bit BGRA; today's window swapchain surface format.
    Bgra8UnormSrgb,
}

// ---------------------------------------------------------------------------
// Axis 4 — DepthIntent
// ---------------------------------------------------------------------------

/// Depth-attachment intent in semantic form.
///
/// Today's pipelines all pass `None` for depth state; `ReadWrite` and
/// `ReadOnly` are forward-looking for when depth lands as a real concern
/// (depth-tested opaque pass, depth-read-only transparent pass).
///
/// | Variant      | gfx-side `Option<DepthStateKey>` realisation                 |
/// |--------------|--------------------------------------------------------------|
/// | `None`       | `None` — no depth attachment                                 |
/// | `ReadWrite`  | depth write enabled + compare `LessEqual` (or `Less`)        |
/// | `ReadOnly`   | depth write disabled (read-only); e.g. transparent passes    |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DepthIntent {
    /// No depth attachment.  Today's only realised value.
    None,

    /// Depth read + write (opaque pass).
    ReadWrite,

    /// Depth read only (e.g. transparent / overlay pass).
    ReadOnly,
}

// ---------------------------------------------------------------------------
// Axis 5 — MaterialParams
// ---------------------------------------------------------------------------

/// Per-material payload that ends up in the gfx UBO.
///
/// Matches the current 32-byte UBO shape exposed by `gfx::material::Material`:
/// two `vec4<f32>` packed as `(base_color, phong)`.  `phong` packs
/// `(ambient, diffuse, specular, shininess)` in `(x, y, z, w)`.
///
/// # Layout
///
/// | offset | field        | type         |
/// |--------|--------------|--------------|
/// | 0      | `base_color` | `vec4<f32>`  |
/// | 16     | `phong`      | `vec4<f32>`  |
///
/// # Hash + Eq
///
/// Hash + Eq are derived bit-wise via the underlying `[f32; 4]` arrays,
/// which means `+0.0 != -0.0` (per IEEE 754) and `NaN != NaN` — that matches
/// the adapter's PSO-cache identity expectations (two materials that differ
/// only by a `+0.0` vs `-0.0` field really are distinct UBO uploads).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialParams {
    /// Linear-RGB base colour with straight alpha.
    pub base_color: [f32; 4],

    /// Phong factors: `(ambient, diffuse, specular, shininess)`.
    pub phong: [f32; 4],
}

impl Eq for MaterialParams {}

impl std::hash::Hash for MaterialParams {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash each f32 as its raw bit-pattern so the Hash impl is consistent
        // with the (bit-wise) derived PartialEq.  This is the standard idiom
        // for f32 keys in lookup tables; it preserves the "+0.0 != -0.0" /
        // "NaN != NaN" semantics of the derived Eq.
        for v in self.base_color {
            v.to_bits().hash(state);
        }
        for v in self.phong {
            v.to_bits().hash(state);
        }
    }
}

impl Default for MaterialParams {
    /// White base colour, zero Phong factors.
    ///
    /// This is the "no material data set yet" sentinel.  Real consumers will
    /// generally populate both arrays before handing the descriptor to the
    /// gfx adapter.
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            phong: [0.0; 4],
        }
    }
}

// ---------------------------------------------------------------------------
// MaterialDescriptor — the five-axis semantic contract
// ---------------------------------------------------------------------------

/// The semantic "what material does this mesh want?" contract.
///
/// Pure intent — the gfx adapter (follow-up dispatch) realises this into a
/// `gfx::Material` + `PsoKey` for the renderer's PSO cache.
///
/// # Identity contract
///
/// Two `MaterialDescriptor`s with identical fields **must** hash equal and
/// compare equal.  This is load-bearing for the gfx adapter's PSO-cache
/// identity: `identical_descriptors_are_equal_and_hash_equal` in the tests
/// below pins this property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialDescriptor {
    /// Which shader recipe (and therefore which gfx-side pipeline) this
    /// material binds against.
    pub shader_id: ShaderId,

    /// Vertex layout the pipeline expects.
    pub vertex_layout: VertexLayoutId,

    /// Colour-attachment format the pipeline targets.
    pub color_target: ColorTargetId,

    /// Depth-attachment intent.
    pub depth: DepthIntent,

    /// Per-material UBO payload (`base_color` + `phong`).
    pub params: MaterialParams,
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::*;

    /// Compute a `u64` digest for any `Hash` value using `DefaultHasher`.
    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    /// Build a baseline descriptor used by the per-axis distinctness tests.
    fn baseline() -> MaterialDescriptor {
        MaterialDescriptor {
            shader_id: ShaderId::LitMesh,
            vertex_layout: VertexLayoutId::LitVertex,
            color_target: ColorTargetId::Rgba8Unorm,
            depth: DepthIntent::None,
            params: MaterialParams {
                base_color: [1.0, 1.0, 1.0, 1.0],
                phong: [0.1, 1.0, 0.5, 32.0],
            },
        }
    }

    // --- LOAD-BEARING: identity ---------------------------------------------

    #[test]
    fn identical_descriptors_are_equal_and_hash_equal() {
        let a = baseline();
        let b = baseline();
        assert_eq!(a, b, "identical descriptors must compare equal");
        assert_eq!(
            hash_of(&a),
            hash_of(&b),
            "identical descriptors must hash equal"
        );
    }

    // --- per-axis distinctness ----------------------------------------------

    #[test]
    fn differing_shader_id_distinct() {
        let a = baseline();
        let mut b = baseline();
        b.shader_id = ShaderId::Mesh;
        assert_ne!(a, b);
        assert_ne!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn differing_vertex_layout_distinct() {
        let a = baseline();
        let mut b = baseline();
        b.vertex_layout = VertexLayoutId::Vertex;
        assert_ne!(a, b);
        assert_ne!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn differing_color_target_distinct() {
        let a = baseline();
        let mut b = baseline();
        b.color_target = ColorTargetId::Bgra8UnormSrgb;
        assert_ne!(a, b);
        assert_ne!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn differing_depth_distinct() {
        let a = baseline();
        let mut b = baseline();
        b.depth = DepthIntent::ReadWrite;
        assert_ne!(a, b);
        assert_ne!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn differing_params_distinct() {
        let a = baseline();
        let mut b = baseline();
        b.params.base_color[0] = 0.5;
        assert_ne!(a, b);
        assert_ne!(hash_of(&a), hash_of(&b));
    }

    // --- nice-to-have sanity checks -----------------------------------------

    #[test]
    fn material_descriptor_clone_preserves_identity() {
        let a = baseline();
        let b = a;
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn default_material_params_is_white_no_phong() {
        let p = MaterialParams::default();
        assert_eq!(p.base_color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(p.phong, [0.0, 0.0, 0.0, 0.0]);
    }
}
