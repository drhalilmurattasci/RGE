//! Frame-graph resource descriptors.
//!
//! Per ADR-118 D1 + D7, this module introduces the two concrete descriptor
//! types ([`TextureDescriptor`] / [`BufferDescriptor`]) that the
//! transient-resource allocator will use to size GPU allocations, plus the
//! graph-time routing discriminator [`ResourceClassDescriptor`] used to
//! thread descriptors through [`FrameGraph::add_pass`] uniformly across the
//! texture / buffer split.
//!
//! # Scope (dispatch 119)
//!
//! This dispatch lands ONLY descriptor types and their analytical flow
//! through compile. NO GPU resource allocation. The downstream
//! `TexturePool` / `BufferPool` (dispatch 120) keys on `(Descriptor,
//! AliasingGroupId)` per ADR-118 D3; this module ships the descriptor side
//! of that key.
//!
//! # Non-goals (per ADR-118 D1)
//!
//! - **No `label` field.** Excluding it from the descriptor identity
//!   prevents pool over-fragmentation; the allocator may synthesize labels
//!   per allocation for diagnostic purposes.
//! - **No `mapped_at_creation`.** Transient resources never CPU-map at
//!   creation; CPU-mapping belongs to staging-buffer workflows, outside
//!   this allocator's lane.
//! - **No serde derives.** wgpu types only derive `Serialize` /
//!   `Deserialize` behind the `serde` feature, which the workspace does
//!   not enable for wgpu. Descriptors are runtime-only.
//!
//! [`FrameGraph::add_pass`]: super::FrameGraph::add_pass
//! [`AliasingGroupId`]: super::AliasingGroup

/// Texture descriptor — mirrors wgpu 29's `TextureDescriptor` for the
/// fields that affect transient-resource pool identity per ADR-118 D1.
///
/// `label` and `mapped_at_creation` are deliberately omitted (per ADR-118
/// D1): they are not pool-identity-relevant. The allocator may synthesize
/// labels per physical allocation for diagnostics; the descriptor stays
/// pool-key-clean.
///
/// Field correspondence to `wgpu::TextureDescriptor` (wgpu 29):
/// - `width` / `height` / `depth_or_array_layers` ← `size.{width, height,
///   depth_or_array_layers}` (flattened for `Hash + Eq` derive).
/// - `mip_level_count` / `sample_count` / `dimension` / `format` / `usage`
///   ← same-named wgpu fields.
/// - `view_dimension` ← added in this descriptor for view-shape pool
///   keying; the wgpu `TextureDescriptor` itself does not carry it (it
///   lives on `TextureViewDescriptor`), but two transient textures with
///   identical storage that surface as `View2D` vs `ViewCube` are not
///   interchangeable from a pool-reuse perspective.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureDescriptor {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Depth (3D textures) or array layer count.
    pub depth_or_array_layers: u32,
    /// Mip level count (1 = no mipmaps).
    pub mip_level_count: u32,
    /// MSAA sample count (1 = no MSAA).
    pub sample_count: u32,
    /// Pixel format.
    pub format: wgpu::TextureFormat,
    /// Allowed usage flags (binding / copy / render-attachment / etc.).
    pub usage: wgpu::TextureUsages,
    /// Storage dimension (D1 / D2 / D3).
    pub dimension: wgpu::TextureDimension,
    /// View dimension (D1 / D2 / D2Array / Cube / CubeArray / D3).
    pub view_dimension: wgpu::TextureViewDimension,
}

impl TextureDescriptor {
    /// Approximate byte size of one allocation of this descriptor.
    ///
    /// Per ADR-118 D5, the allocator picks the largest descriptor in each
    /// `AliasingGroup` to govern the physical slot. This helper exposes a
    /// best-effort size estimate (`width * height * depth_or_array_layers
    /// * sample_count * bytes_per_pixel`); mip chain pyramid is included
    /// via `1 + 1/4 + 1/16 + ...` ≈ `4/3` factor when `mip_level_count >
    /// 1`. Compressed / planar / depth-stencil formats return the
    /// `block_copy_size` for color aspect only — the allocator uses this
    /// for relative comparison within a group, not as a wgpu allocation
    /// directive.
    ///
    /// Returns `0` if the format does not report a single-aspect
    /// `block_copy_size` (e.g. multi-planar) or if any field is zero.
    #[must_use]
    pub fn byte_size_estimate(&self) -> u64 {
        let Some(bpp) = self.format.block_copy_size(None) else {
            return 0;
        };
        let base = u64::from(self.width)
            .saturating_mul(u64::from(self.height))
            .saturating_mul(u64::from(self.depth_or_array_layers))
            .saturating_mul(u64::from(self.sample_count))
            .saturating_mul(u64::from(bpp));
        if self.mip_level_count > 1 {
            // Geometric series 1 + 1/4 + 1/16 + ... ≤ 4/3 for unbounded
            // mip chain. Truncated mip chains converge below 4/3; use
            // 4/3 as an upper bound for pool sizing purposes.
            base.saturating_mul(4) / 3
        } else {
            base
        }
    }
}

/// Buffer descriptor — mirrors wgpu 29's `BufferDescriptor` for the fields
/// that affect transient-resource pool identity per ADR-118 D1.
///
/// `label` is omitted (allocator may synthesize per allocation);
/// `mapped_at_creation` is omitted (transient resources are GPU-resident
/// only; staging-buffer workflows live elsewhere).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferDescriptor {
    /// Size in bytes.
    pub size_bytes: u64,
    /// Allowed usage flags (vertex / index / uniform / storage / copy /
    /// indirect / map / etc.).
    pub usage: wgpu::BufferUsages,
}

impl BufferDescriptor {
    /// Byte size of one allocation of this descriptor — trivially
    /// `size_bytes` for buffers. Provided for parity with
    /// [`TextureDescriptor::byte_size_estimate`].
    #[must_use]
    pub const fn byte_size_estimate(&self) -> u64 {
        self.size_bytes
    }
}

/// Graph-time routing discriminator threading both descriptor classes
/// through [`FrameGraph::add_pass`] uniformly.
///
/// Per ADR-118 D2, the allocator-level pools (`TexturePool` /
/// `BufferPool`) remain **separate types** — this enum exists purely for
/// graph-time API uniformity so callers declare descriptors against
/// `ResourceId`s without the substrate needing two parallel `add_pass`
/// surfaces. The pools (dispatch 120) discriminate on the variant once
/// per resource.
///
/// [`FrameGraph::add_pass`]: super::FrameGraph::add_pass
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceClassDescriptor {
    /// Texture-class resource — see [`TextureDescriptor`].
    Texture(TextureDescriptor),
    /// Buffer-class resource — see [`BufferDescriptor`].
    Buffer(BufferDescriptor),
}

impl ResourceClassDescriptor {
    /// Approximate byte size of one allocation of this descriptor.
    ///
    /// Delegates to the variant's `byte_size_estimate`. Per ADR-118 D5,
    /// the allocator uses this for "largest descriptor in group"
    /// comparisons; the value is best-effort and not authoritative for
    /// wgpu's physical allocation.
    #[must_use]
    pub fn byte_size_estimate(&self) -> u64 {
        match self {
            Self::Texture(t) => t.byte_size_estimate(),
            Self::Buffer(b) => b.byte_size_estimate(),
        }
    }

    /// True iff this descriptor describes a texture-class resource.
    #[must_use]
    pub const fn is_texture(&self) -> bool {
        matches!(self, Self::Texture(_))
    }

    /// True iff this descriptor describes a buffer-class resource.
    #[must_use]
    pub const fn is_buffer(&self) -> bool {
        matches!(self, Self::Buffer(_))
    }
}

impl From<TextureDescriptor> for ResourceClassDescriptor {
    fn from(d: TextureDescriptor) -> Self {
        Self::Texture(d)
    }
}

impl From<BufferDescriptor> for ResourceClassDescriptor {
    fn from(d: BufferDescriptor) -> Self {
        Self::Buffer(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_texture_descriptor() -> TextureDescriptor {
        TextureDescriptor {
            width: 1024,
            height: 1024,
            depth_or_array_layers: 1,
            mip_level_count: 1,
            sample_count: 1,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            dimension: wgpu::TextureDimension::D2,
            view_dimension: wgpu::TextureViewDimension::D2,
        }
    }

    fn sample_buffer_descriptor() -> BufferDescriptor {
        BufferDescriptor {
            size_bytes: 4096,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        }
    }

    // TD-1: identical descriptors are equal + hash-equal; differing
    // descriptors are not equal.
    #[test]
    fn texture_descriptor_equal_when_all_fields_match() {
        let a = sample_texture_descriptor();
        let b = sample_texture_descriptor();
        assert_eq!(a, b);
        // Hash equality follows from Eq for derived `Hash` impls; assert
        // via a hash-set round-trip.
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn texture_descriptor_distinct_when_width_differs() {
        let a = sample_texture_descriptor();
        let mut b = sample_texture_descriptor();
        b.width = 2048;
        assert_ne!(a, b);
    }

    #[test]
    fn texture_descriptor_distinct_when_format_differs() {
        let a = sample_texture_descriptor();
        let mut b = sample_texture_descriptor();
        b.format = wgpu::TextureFormat::Bgra8UnormSrgb;
        assert_ne!(a, b);
    }

    #[test]
    fn texture_descriptor_distinct_when_view_dimension_differs() {
        let a = sample_texture_descriptor();
        let mut b = sample_texture_descriptor();
        b.view_dimension = wgpu::TextureViewDimension::Cube;
        b.depth_or_array_layers = 6;
        assert_ne!(a, b);
    }

    #[test]
    fn buffer_descriptor_equal_when_all_fields_match() {
        let a = sample_buffer_descriptor();
        let b = sample_buffer_descriptor();
        assert_eq!(a, b);
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn buffer_descriptor_distinct_when_size_differs() {
        let a = sample_buffer_descriptor();
        let mut b = sample_buffer_descriptor();
        b.size_bytes = 8192;
        assert_ne!(a, b);
    }

    #[test]
    fn buffer_descriptor_distinct_when_usage_differs() {
        let a = sample_buffer_descriptor();
        let mut b = sample_buffer_descriptor();
        b.usage = wgpu::BufferUsages::STORAGE;
        assert_ne!(a, b);
    }

    // TD-2: ResourceClassDescriptor round-trip — Texture(d).clone() ==
    // Texture(d), and Texture(d) != Buffer(b) even when sizes match.
    #[test]
    fn resource_class_descriptor_texture_clone_equal() {
        let d = sample_texture_descriptor();
        let a = ResourceClassDescriptor::Texture(d);
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn resource_class_descriptor_buffer_clone_equal() {
        let d = sample_buffer_descriptor();
        let a = ResourceClassDescriptor::Buffer(d);
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn resource_class_descriptor_texture_and_buffer_distinct() {
        let t = ResourceClassDescriptor::Texture(sample_texture_descriptor());
        let b = ResourceClassDescriptor::Buffer(sample_buffer_descriptor());
        assert_ne!(t, b);
    }

    #[test]
    fn resource_class_descriptor_from_impls_route_to_correct_variant() {
        let t: ResourceClassDescriptor = sample_texture_descriptor().into();
        let b: ResourceClassDescriptor = sample_buffer_descriptor().into();
        assert!(t.is_texture());
        assert!(!t.is_buffer());
        assert!(b.is_buffer());
        assert!(!b.is_texture());
    }

    #[test]
    fn texture_descriptor_byte_size_estimate_baseline() {
        let d = sample_texture_descriptor();
        // 1024 * 1024 * 1 * 1 * 4 (Rgba8Unorm) = 4 MiB exactly; no mips
        // → no 4/3 factor.
        assert_eq!(d.byte_size_estimate(), 1024 * 1024 * 4);
    }

    #[test]
    fn texture_descriptor_byte_size_estimate_with_mips_applies_4_3() {
        let mut d = sample_texture_descriptor();
        d.mip_level_count = 11;
        // 1024² * 4 = 4 MiB base; * 4/3 (upper bound) = 5.33 MiB
        let base = 1024u64 * 1024 * 4;
        assert_eq!(d.byte_size_estimate(), base * 4 / 3);
    }

    #[test]
    fn buffer_descriptor_byte_size_estimate_is_size_bytes() {
        let d = sample_buffer_descriptor();
        assert_eq!(d.byte_size_estimate(), 4096);
    }

    #[test]
    fn resource_class_descriptor_byte_size_estimate_dispatches() {
        let t: ResourceClassDescriptor = sample_texture_descriptor().into();
        let b: ResourceClassDescriptor = sample_buffer_descriptor().into();
        assert_eq!(t.byte_size_estimate(), 1024 * 1024 * 4);
        assert_eq!(b.byte_size_estimate(), 4096);
    }
}
