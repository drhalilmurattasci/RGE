//! Pipeline State Object (PSO) memoization cache (Phase 6.3 fill-in;
//! renderer-local substrate).
//!
//! Failure class: recoverable
//!
//! # What this module IS
//!
//! Vocabulary + ownership boundaries for caching `wgpu::RenderPipeline`
//! instances by `(shader source hash, vertex layout)`:
//!
//! - [`ShaderHash`] — opaque 32-byte BLAKE3 of shader source bytes (mirrors
//!   `crate::frame_graph::ResourceId` / `kernel/io-scheduler::IoRequestId`
//!   in shape — `const from_bytes` / `const as_bytes` so callers can build
//!   sentinels at compile time, plus a `from_source` BLAKE3 helper).
//! - [`VertexLayoutDescriptor`] — `(stride, step_mode, attributes)` triple
//!   keyed against `wgpu::VertexAttribute` / `wgpu::VertexStepMode`. The
//!   wgpu types are `Hash + Eq` by derive so the descriptor inherits both
//!   without further work.
//! - [`PsoKey`] — `(ShaderHash, VertexLayoutDescriptor)` composite key.
//! - [`PipelineCache<T>`] — generic-over-T cache. Production code uses
//!   `PipelineCache<wgpu::RenderPipeline>` (or a wrapper struct that owns
//!   one); tests can use any `T` to verify memoization semantics without a
//!   wgpu `Device`.
//!
//! # NON-GOALS (load-bearing for v0 scope)
//!
//! - **No shader graph.** This cache is keyed on already-hashed shader
//!   source; it does NOT compose, link, or specialize shader source.
//! - **No Naga integration.** Shader source is opaque bytes from the
//!   cache's perspective; consumers may pre-process via Naga but the cache
//!   does not import or depend on Naga.
//! - **No runtime frame loop.** The cache is a memoization substrate; it
//!   does NOT drive frame-loop ordering, sub-frame scheduling, or the
//!   sim/render-thread split (deferred per `SCENE_EXTRACTION_CONTRACT.md`
//!   §6.2).
//! - **No render-snapshot integration.** The cache holds long-lived
//!   pipelines; it does NOT participate in `SnapshotParticipate`. GPU
//!   resource state is `recoverable` per `SCENE_EXTRACTION_CONTRACT.md`
//!   §5.4 — pipelines are rebuilt-on-demand; not PIE-participating.
//! - **No benchmark.** Phase 6 exit gate; substrate-only here.
//! - **No `MeshPipeline` / `LitMeshPipeline` / `TrianglePipeline`
//!   integration.** Those wrappers may consume the cache in a follow-up
//!   dispatch, but the cache stands alone in this dispatch. Their existing
//!   shape is preserved.
//! - **No eviction policy.** Cached pipelines stay live until [`PipelineCache::clear`]
//!   drops them or the cache itself is dropped. LRU / weak-ref / cold-pipeline
//!   eviction are out of v0 scope.
//! - **No new architecture lint, no new ADR, no new doctrine doc, no new
//!   §18 companion.** Routine substrate cavity-shaping per PLAN §0.6
//!   freeze policy.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wgpu::{VertexAttribute, VertexStepMode};

// ---------------------------------------------------------------------------
// ShaderHash
// ---------------------------------------------------------------------------

/// Opaque 32-byte BLAKE3 hash of shader source bytes.
///
/// The cache treats shader source as opaque — the only requirement is that
/// hash equality implies semantic equality (i.e. consumers must hash the
/// canonical source representation, not whitespace-variant strings).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ShaderHash([u8; 32]);

impl ShaderHash {
    /// Construct from raw bytes. `const` so callers can build sentinel
    /// hashes at compile time.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying bytes. `const` for parity with [`from_bytes`].
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hash shader source bytes via BLAKE3 (32-byte digest).
    ///
    /// Convenience helper for the common case where consumers have raw
    /// shader source. Equivalent to `ShaderHash::from_bytes(*blake3::hash(source).as_bytes())`.
    #[must_use]
    pub fn from_source(source: &[u8]) -> Self {
        let hash = blake3::hash(source);
        Self(*hash.as_bytes())
    }
}

// ---------------------------------------------------------------------------
// VertexLayoutDescriptor
// ---------------------------------------------------------------------------

/// Hashable, owned mirror of `wgpu::VertexBufferLayout` fields.
///
/// `wgpu::VertexBufferLayout` carries a `&'static [VertexAttribute]` which
/// is awkward to use as a `HashMap` key. This descriptor owns its
/// attribute vector and uses wgpu's derive-`Hash` + derive-`Eq` impls on
/// [`VertexAttribute`] / [`VertexStepMode`] to inherit both traits.
///
/// Two descriptors with structurally-identical content compare equal and
/// hash to the same bucket — the central memoization invariant.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VertexLayoutDescriptor {
    /// Bytes per vertex; matches `wgpu::VertexBufferLayout::array_stride`.
    pub stride: u64,
    /// Per-vertex vs per-instance step rate.
    pub step_mode: VertexStepMode,
    /// Per-attribute (location, offset, format).
    pub attributes: Vec<VertexAttribute>,
}

impl VertexLayoutDescriptor {
    /// Construct a layout descriptor from owned data.
    #[must_use]
    pub fn new(stride: u64, step_mode: VertexStepMode, attributes: Vec<VertexAttribute>) -> Self {
        Self {
            stride,
            step_mode,
            attributes,
        }
    }
}

// ---------------------------------------------------------------------------
// PsoKey
// ---------------------------------------------------------------------------

/// Composite cache key — `(shader hash, vertex layout)`.
///
/// Two pipelines with the same shader source and the same vertex layout
/// produce structurally-identical compiled state and may share a single
/// `wgpu::RenderPipeline` allocation. Per-material uniforms (camera,
/// material, light) live in bind groups outside the pipeline object and
/// are cheap to switch — that is what makes pipeline sharing valuable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PsoKey {
    /// BLAKE3 hash of the shader source.
    pub shader: ShaderHash,
    /// Vertex buffer layout the pipeline binds against.
    pub layout: VertexLayoutDescriptor,
}

impl PsoKey {
    /// Construct from owned components.
    #[must_use]
    pub fn new(shader: ShaderHash, layout: VertexLayoutDescriptor) -> Self {
        Self { shader, layout }
    }
}

// ---------------------------------------------------------------------------
// PipelineCache
// ---------------------------------------------------------------------------

/// Memoization cache mapping [`PsoKey`] to `Arc<T>`.
///
/// Generic over `T` so production code can cache `wgpu::RenderPipeline` (or
/// a wrapper) while tests verify the memoization semantics with a trivial
/// `T` (no GPU `Device` required).
///
/// The cache is **single-threaded** by construction (`&mut self` on
/// [`get_or_insert`]); cross-thread sharing is via `Arc<T>` clones returned
/// from lookups. A future Phase 6+ multi-threaded variant would add a
/// `Mutex` or `RwLock` shell; v0 keeps the surface bounded.
#[derive(Debug)]
pub struct PipelineCache<T> {
    pipelines: HashMap<PsoKey, Arc<T>>,
    hits: u64,
    misses: u64,
}

impl<T> PipelineCache<T> {
    /// Construct an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Look up an existing pipeline by `key`, or build + insert a new one
    /// via `builder` if absent.
    ///
    /// On hit: increments [`hits`](Self::hits); returns a clone of the
    /// cached `Arc<T>` without invoking `builder`.
    ///
    /// On miss: increments [`misses`](Self::misses); calls `builder` to
    /// produce the `T`, wraps it in `Arc`, inserts into the cache, and
    /// returns a clone of the same `Arc`.
    pub fn get_or_insert<F>(&mut self, key: PsoKey, builder: F) -> Arc<T>
    where
        F: FnOnce() -> T,
    {
        match self.pipelines.entry(key) {
            Entry::Occupied(e) => {
                self.hits += 1;
                Arc::clone(e.get())
            }
            Entry::Vacant(e) => {
                self.misses += 1;
                let pipeline = Arc::new(builder());
                e.insert(Arc::clone(&pipeline));
                pipeline
            }
        }
    }

    /// Number of distinct pipelines currently cached.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pipelines.len()
    }

    /// `true` if the cache holds no pipelines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    /// Drop every cached pipeline. Hit/miss stats are preserved across
    /// `clear` so the workload's lifetime cache-effectiveness ratio stays
    /// observable across resource churn.
    pub fn clear(&mut self) {
        self.pipelines.clear();
    }

    /// Cumulative cache-hit count since construction.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Cumulative cache-miss count since construction.
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.misses
    }
}

impl<T> Default for PipelineCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use wgpu::{VertexAttribute, VertexFormat, VertexStepMode};

    use super::*;

    fn shader_a() -> ShaderHash {
        ShaderHash::from_bytes([0xa1; 32])
    }

    fn shader_b() -> ShaderHash {
        ShaderHash::from_bytes([0xb2; 32])
    }

    fn layout_position_only() -> VertexLayoutDescriptor {
        VertexLayoutDescriptor::new(
            12,
            VertexStepMode::Vertex,
            vec![VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            }],
        )
    }

    fn layout_position_color() -> VertexLayoutDescriptor {
        VertexLayoutDescriptor::new(
            24,
            VertexStepMode::Vertex,
            vec![
                VertexAttribute {
                    format: VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                VertexAttribute {
                    format: VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
            ],
        )
    }

    // ----- ShaderHash -----

    #[test]
    fn shader_hash_round_trip_via_bytes() {
        let bytes = [7u8; 32];
        let h = ShaderHash::from_bytes(bytes);
        assert_eq!(h.as_bytes(), &bytes);
    }

    #[test]
    fn shader_hash_zero_and_max_distinct() {
        let z = ShaderHash::from_bytes([0u8; 32]);
        let m = ShaderHash::from_bytes([0xffu8; 32]);
        assert_ne!(z, m);
    }

    #[test]
    fn shader_hash_from_source_deterministic() {
        let src = b"fn main() { /* shader */ }";
        let h1 = ShaderHash::from_source(src);
        let h2 = ShaderHash::from_source(src);
        assert_eq!(h1, h2);
    }

    #[test]
    fn shader_hash_from_source_distinct_for_different_source() {
        let h1 = ShaderHash::from_source(b"fn a() {}");
        let h2 = ShaderHash::from_source(b"fn b() {}");
        assert_ne!(h1, h2);
    }

    // ----- VertexLayoutDescriptor -----

    #[test]
    fn vertex_layout_descriptor_eq_for_same_content() {
        let a = layout_position_only();
        let b = layout_position_only();
        assert_eq!(a, b);
    }

    #[test]
    fn vertex_layout_descriptor_distinct_for_different_attributes() {
        assert_ne!(layout_position_only(), layout_position_color());
    }

    #[test]
    fn vertex_layout_descriptor_distinct_for_different_step_mode() {
        let a = VertexLayoutDescriptor::new(
            12,
            VertexStepMode::Vertex,
            vec![VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            }],
        );
        let b = VertexLayoutDescriptor::new(
            12,
            VertexStepMode::Instance,
            vec![VertexAttribute {
                format: VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            }],
        );
        assert_ne!(a, b);
    }

    // ----- PsoKey -----

    #[test]
    fn pso_key_eq_for_same_content() {
        let a = PsoKey::new(shader_a(), layout_position_only());
        let b = PsoKey::new(shader_a(), layout_position_only());
        assert_eq!(a, b);
    }

    #[test]
    fn pso_key_distinct_for_different_shader() {
        let a = PsoKey::new(shader_a(), layout_position_only());
        let b = PsoKey::new(shader_b(), layout_position_only());
        assert_ne!(a, b);
    }

    #[test]
    fn pso_key_distinct_for_different_layout() {
        let a = PsoKey::new(shader_a(), layout_position_only());
        let b = PsoKey::new(shader_a(), layout_position_color());
        assert_ne!(a, b);
    }

    // ----- PipelineCache -----

    #[test]
    fn pipeline_cache_new_is_empty() {
        let cache = PipelineCache::<u32>::new();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 0);
    }

    #[test]
    fn pipeline_cache_default_matches_new() {
        let a = PipelineCache::<u32>::new();
        let b = PipelineCache::<u32>::default();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.hits(), b.hits());
        assert_eq!(a.misses(), b.misses());
    }

    #[test]
    fn pipeline_cache_first_insert_is_miss() {
        let mut cache = PipelineCache::<u32>::new();
        let key = PsoKey::new(shader_a(), layout_position_only());
        let mut build_count = 0;
        let pipeline = cache.get_or_insert(key, || {
            build_count += 1;
            42
        });
        assert_eq!(*pipeline, 42);
        assert_eq!(build_count, 1);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn pipeline_cache_second_lookup_is_hit_and_shares_arc() {
        let mut cache = PipelineCache::<u32>::new();
        let key = PsoKey::new(shader_a(), layout_position_only());
        let p1 = cache.get_or_insert(key.clone(), || 42);
        let p2 = cache.get_or_insert(key, || panic!("builder must NOT be called on cache hit"));
        assert!(Arc::ptr_eq(&p1, &p2));
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn pipeline_cache_different_shader_is_miss() {
        let mut cache = PipelineCache::<u32>::new();
        let key_a = PsoKey::new(shader_a(), layout_position_only());
        let key_b = PsoKey::new(shader_b(), layout_position_only());
        cache.get_or_insert(key_a, || 1);
        cache.get_or_insert(key_b, || 2);
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn pipeline_cache_different_layout_is_miss() {
        let mut cache = PipelineCache::<u32>::new();
        let key_a = PsoKey::new(shader_a(), layout_position_only());
        let key_b = PsoKey::new(shader_a(), layout_position_color());
        cache.get_or_insert(key_a, || 1);
        cache.get_or_insert(key_b, || 2);
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn many_material_instances_share_one_pipeline() {
        // The principal use case: N material instances binding the same
        // shader + vertex layout share a single cached pipeline. Builder
        // fires once; all subsequent lookups hit; every Arc points to the
        // same allocation.
        let mut cache = PipelineCache::<u32>::new();
        let key = PsoKey::new(shader_a(), layout_position_only());
        let mut build_count = 0;

        let pipelines: Vec<Arc<u32>> = (0..100)
            .map(|_| {
                cache.get_or_insert(key.clone(), || {
                    build_count += 1;
                    7
                })
            })
            .collect();

        assert_eq!(build_count, 1, "builder fires exactly once");
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 99);
        // Every returned Arc points to the same allocation.
        let first = &pipelines[0];
        for p in &pipelines[1..] {
            assert!(Arc::ptr_eq(first, p));
            assert_eq!(**p, 7);
        }
    }

    #[test]
    fn pipeline_cache_clear_drops_pipelines_and_preserves_stats() {
        let mut cache = PipelineCache::<u32>::new();
        let key_a = PsoKey::new(shader_a(), layout_position_only());
        let key_b = PsoKey::new(shader_b(), layout_position_only());
        cache.get_or_insert(key_a.clone(), || 1);
        cache.get_or_insert(key_b, || 2);
        cache.get_or_insert(key_a, || panic!("hit, builder NOT called"));
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 2);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        // Hit/miss counters are preserved — observable lifetime stats
        // survive resource-churn cycles.
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 2);
    }

    #[test]
    fn pipeline_cache_clear_then_reinsert_misses_again() {
        // After clear, the same key that previously hit must now miss
        // because the underlying allocation was dropped.
        let mut cache = PipelineCache::<u32>::new();
        let key = PsoKey::new(shader_a(), layout_position_only());
        cache.get_or_insert(key.clone(), || 1);
        cache.clear();
        let mut build_count = 0;
        cache.get_or_insert(key, || {
            build_count += 1;
            2
        });
        assert_eq!(build_count, 1, "post-clear lookup must rebuild");
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.len(), 1);
    }
}
