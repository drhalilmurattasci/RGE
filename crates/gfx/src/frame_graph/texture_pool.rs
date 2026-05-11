//! Latest-only ring-buffered transient-texture pool (dispatch 120 per
//! ADR-118 D3 + D4 + D5).
//!
//! - [`AliasingGroupId`] — `u32`-newtype index into
//!   [`CompiledFrameGraph::aliasing_groups()`](super::CompiledFrameGraph::aliasing_groups).
//!   Position-based identity is stable across recompiles with identical
//!   graph shape (iteration order is `BTreeMap<ResourceId,_>`-deterministic
//!   in `compile.rs`); a content-hash variant is a future amendment.
//! - [`TexturePool`] — `N = 2` frames-in-flight texture pool per ADR-118
//!   D4. Slots rotate via [`begin_frame`](TexturePool::begin_frame); on
//!   slot re-entry, the slot's active acquisitions drain back into its
//!   free-list, which subsequent acquisitions consume before allocating
//!   fresh.
//!
//! # Scope (dispatch 120)
//!
//! Caller responsibilities: (a) pre-compute the max descriptor in each
//! aliasing group per ADR-118 D5 (the pool consumes the chosen descriptor);
//! (b) call [`begin_frame`](TexturePool::begin_frame) once per frame at
//! the start of frame-graph execution; (c) map `ResourceId`s in the same
//! aliasing group to the same returned `Arc`.
//!
//! # NON-GOALS
//!
//! No `BufferPool` (dispatch 121); no `wgpu::Queue` interaction (no
//! uploads / no manual barriers — D6 trusts wgpu auto-tracking); no
//! `FrameRecorder` integration (dispatch 121); no `GfxContext` mutation;
//! no async / fence ownership (renderer-thread spawn ADR future work per
//! ADR-117 non-decision #2); no serialized-graph support
//! ([`CompiledFrameGraph::descriptors`] is `#[serde(skip)]`; sibling
//! wire-format ADR concern).

use std::collections::HashMap;
use std::sync::Arc;

use super::descriptor::TextureDescriptor;

/// Index into [`CompiledFrameGraph::aliasing_groups()`](super::CompiledFrameGraph::aliasing_groups).
///
/// Per `compile.rs`, the iteration order of `aliasing_groups()` is
/// deterministic (greedy assignment over `BTreeMap<ResourceId, _>` order)
/// so this position-based identity is stable across recompiles with
/// identical graph shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AliasingGroupId(pub u32);

/// Number of frames-in-flight per ADR-118 D4 (initial N=2 — the minimum
/// that decouples CPU recording from GPU execution). A future amendment
/// may bump to N=3 if empirical fence-wait surfaces it as a hotspot.
const FRAMES_IN_FLIGHT: usize = 2;

/// Per-slot state in the [`TexturePool`] ring.
#[derive(Default)]
struct SlotState {
    /// Free-list of textures returned to the slot at slot re-entry,
    /// keyed per ADR-118 D3.
    free_lists: HashMap<(TextureDescriptor, AliasingGroupId), Vec<Arc<wgpu::Texture>>>,
    /// Currently-active acquisitions for THIS frame. The descriptor is
    /// carried alongside each `Arc` because wgpu 29's `Texture` does not
    /// expose the original `TextureDescriptor` for free-list routing
    /// at slot reset.
    active: Vec<(TextureDescriptor, AliasingGroupId, Arc<wgpu::Texture>)>,
}

/// Latest-only ring-buffered transient-texture pool per ADR-118 D3 + D4.
///
/// Owns `N = FRAMES_IN_FLIGHT` slot rings; rotates the active slot on each
/// [`begin_frame`](Self::begin_frame) call. Single-threaded by construction
/// (`&mut self` on every state-mutating method, mirroring
/// [`crate::pso_cache::PipelineCache`]); cross-thread wrapping is the
/// caller's responsibility per ADR-118 non-decision #1. Per ADR-118 D6,
/// the substrate trusts wgpu's auto-tracked hazard barriers — the only
/// correctness contract is that the caller stops recording against the
/// texture before the next slot rotation retires the GPU work.
pub struct TexturePool {
    slots: [SlotState; FRAMES_IN_FLIGHT],
    /// Index into `slots` for the active frame. Starts at 0; the first
    /// `begin_frame` call rotates to slot 1; the second back to slot 0.
    current_slot: usize,
}

impl Default for TexturePool {
    fn default() -> Self {
        Self::new()
    }
}

impl TexturePool {
    /// Construct an empty pool starting at slot 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: [SlotState::default(), SlotState::default()],
            current_slot: 0,
        }
    }

    /// Rotate to the next slot per ADR-118 D4 and drain the new slot's
    /// active acquisitions back to its free-list.
    ///
    /// Call once per frame at the start of frame-graph execution. The
    /// contract per ADR-118 D4 is "the slot is safe to reuse once the
    /// corresponding GPU submission has retired"; the pool does NOT
    /// enforce this — fence ownership is the caller's responsibility
    /// (sibling concern per the dispatch-120 scope). For the MVP,
    /// callers running synchronous `queue.submit` + readback workflows
    /// satisfy the contract trivially.
    pub fn begin_frame(&mut self) {
        self.current_slot = (self.current_slot + 1) % FRAMES_IN_FLIGHT;
        let slot = &mut self.slots[self.current_slot];
        for (desc, group, arc) in slot.active.drain(..) {
            slot.free_lists.entry((desc, group)).or_default().push(arc);
        }
    }

    /// Acquire a texture matching `descriptor` for `group`.
    ///
    /// Returns the same `Arc<wgpu::Texture>` as a previous acquisition if
    /// a slot free-list entry matches `(descriptor, group)`; otherwise
    /// allocates fresh via `device.create_texture(...)`. Per ADR-118 D5,
    /// the caller pre-computes which descriptor to use (the max descriptor
    /// in the group); this method consumes the chosen descriptor without
    /// further reasoning.
    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        descriptor: &TextureDescriptor,
        group: AliasingGroupId,
    ) -> Arc<wgpu::Texture> {
        let slot = &mut self.slots[self.current_slot];
        let key = (*descriptor, group);
        let arc = slot
            .free_lists
            .get_mut(&key)
            .and_then(Vec::pop)
            .unwrap_or_else(|| allocate_texture(device, descriptor));
        slot.active.push((*descriptor, group, Arc::clone(&arc)));
        arc
    }
}

/// Allocate a fresh `wgpu::Texture` matching `descriptor`. `label` is
/// `None` per ADR-118 D1 (pool-key-clean descriptors); `view_formats`
/// is empty because [`TextureDescriptor`] does not carry a view-format
/// list — view creation is the caller's lane.
fn allocate_texture(device: &wgpu::Device, descriptor: &TextureDescriptor) -> Arc<wgpu::Texture> {
    Arc::new(device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: descriptor.width,
            height: descriptor.height,
            depth_or_array_layers: descriptor.depth_or_array_layers,
        },
        mip_level_count: descriptor.mip_level_count,
        sample_count: descriptor.sample_count,
        dimension: descriptor.dimension,
        format: descriptor.format,
        usage: descriptor.usage,
        view_formats: &[],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! ctx_or_skip {
        () => {{
            match crate::context::GfxContext::new_headless() {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("SKIP: no GPU adapter");
                    return;
                }
            }
        }};
    }

    fn sample_descriptor() -> TextureDescriptor {
        TextureDescriptor {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
            mip_level_count: 1,
            sample_count: 1,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            dimension: wgpu::TextureDimension::D2,
            view_dimension: wgpu::TextureViewDimension::D2,
        }
    }

    // TP-1: structural — fresh pool starts empty at slot 0; begin_frame
    // rotates through 0 → 1 → 0 → 1.
    #[test]
    fn new_pool_starts_at_slot_zero_and_begin_frame_rotates() {
        let mut pool = TexturePool::new();
        assert_eq!(pool.current_slot, 0);
        for slot in &pool.slots {
            assert!(slot.active.is_empty());
            assert!(slot.free_lists.is_empty());
        }
        pool.begin_frame();
        assert_eq!(pool.current_slot, 1);
        pool.begin_frame();
        assert_eq!(pool.current_slot, 0, "N=2 ring wraps back to slot 0");
        pool.begin_frame();
        assert_eq!(pool.current_slot, 1);
    }

    // TP-2: GPU-gated — acquire returns a texture (strong_count proves
    // the pool retained one Arc in active; caller holds the other).
    #[test]
    fn acquire_with_real_device_returns_texture() {
        let ctx = ctx_or_skip!();
        let mut pool = TexturePool::new();
        let arc = pool.acquire(ctx.device(), &sample_descriptor(), AliasingGroupId(0));
        assert_eq!(Arc::strong_count(&arc), 2);
    }

    // TP-3: GPU-gated — full ring round-trip + acquire returns the SAME
    // Arc<wgpu::Texture> via the free-list (cache hit via pointer equality).
    //
    // Sequence: acquire X (slot 0 active). begin_frame → slot 1 (nothing
    // to drain). begin_frame → slot 0 (slot 0's active drains into its
    // free_list). acquire same key → pops X from slot 0's free_list.
    #[test]
    fn acquire_then_full_ring_returns_texture_to_free_list() {
        let ctx = ctx_or_skip!();
        let mut pool = TexturePool::new();
        let desc = sample_descriptor();
        let first = pool.acquire(ctx.device(), &desc, AliasingGroupId(0));
        let first_ptr = Arc::as_ptr(&first);
        drop(first);

        pool.begin_frame(); // slot 0 → slot 1
        pool.begin_frame(); // slot 1 → slot 0; drains slot 0's active to free_list

        let second = pool.acquire(ctx.device(), &desc, AliasingGroupId(0));
        assert_eq!(
            first_ptr,
            Arc::as_ptr(&second),
            "free-list reuse must return the same wgpu::Texture allocation"
        );
    }

    // TP-4: GPU-gated — distinct AliasingGroupIds get distinct textures
    // per ADR-118 D3 (key includes the group id).
    #[test]
    fn different_groups_with_same_descriptor_get_distinct_textures() {
        let ctx = ctx_or_skip!();
        let mut pool = TexturePool::new();
        let desc = sample_descriptor();
        let a = pool.acquire(ctx.device(), &desc, AliasingGroupId(0));
        let b = pool.acquire(ctx.device(), &desc, AliasingGroupId(1));
        assert!(
            !Arc::ptr_eq(&a, &b),
            "distinct AliasingGroupIds must produce distinct allocations even with \
             identical descriptors (ADR-118 D3 key)"
        );
    }

    // TP-5: GPU-gated — distinct descriptors in the same group get
    // distinct textures.
    #[test]
    fn different_descriptors_in_same_group_get_distinct_textures() {
        let ctx = ctx_or_skip!();
        let mut pool = TexturePool::new();
        let small = sample_descriptor();
        let large = TextureDescriptor {
            width: 128,
            ..small
        };
        let a = pool.acquire(ctx.device(), &small, AliasingGroupId(0));
        let b = pool.acquire(ctx.device(), &large, AliasingGroupId(0));
        assert!(!Arc::ptr_eq(&a, &b));
    }
}
