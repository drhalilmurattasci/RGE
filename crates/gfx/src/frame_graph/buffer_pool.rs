//! Latest-only ring-buffered transient-buffer pool (dispatch 121 per
//! ADR-118 D2 + D3 + D4 + D5).
//!
//! Clean mirror of [`super::texture_pool::TexturePool`] for the buffer
//! resource class per ADR-118 D2 (separate pool types — `TexturePool` and
//! `BufferPool` are distinct types). The wgpu seam differs only at
//! [`allocate_buffer`]: `device.create_buffer(...)` substitutes for
//! `create_texture`.
//!
//! [`super::texture_pool::AliasingGroupId`] is reused — graph-time identity
//! is shared across both pool kinds (one aliasing group can span both
//! texture and buffer resources at the compile-time analytical layer; the
//! discrimination happens at the pool boundary per ADR-118 D2).
//!
//! # Scope (dispatch 121)
//!
//! Caller responsibilities (identical to [`super::texture_pool::TexturePool`]
//! per ADR-118 D5): (a) pre-compute the max descriptor in each aliasing
//! group (the pool consumes the chosen descriptor); (b) call
//! [`begin_frame`](BufferPool::begin_frame) once per frame at the start of
//! frame-graph execution; (c) map `ResourceId`s in the same aliasing group
//! to the same returned `Arc` — this acquire-dedup contract is load-bearing
//! for dispatch 122 (`FrameRecorder` integration).
//!
//! Consumes in-process `CompiledFrameGraph` descriptor data only;
//! serialized-graph-driven allocation is OUT OF SCOPE for this MVP
//! (`CompiledFrameGraph::descriptors` is `#[serde(skip)]`; sibling
//! wire-format ADR concern).
//!
//! # NON-GOALS
//!
//! No `wgpu::Queue` interaction (no uploads / no `write_buffer` / no
//! `map_async` / no `unmap` — D6 trusts wgpu auto-tracking, and staging
//! buffer workflows live outside this allocator's lane per the descriptor
//! D1 "no `mapped_at_creation`" decision); no `FrameRecorder` integration
//! (dispatch 122); no `GfxContext` mutation; no async / fence ownership
//! (renderer-thread spawn ADR future work per ADR-117 non-decision #2);
//! no serialized-graph support; no behavioral change to
//! [`super::texture_pool`].

use std::collections::HashMap;
use std::sync::Arc;

use super::descriptor::BufferDescriptor;
use super::texture_pool::AliasingGroupId;

/// Number of frames-in-flight per ADR-118 D4 (initial N=2 — the minimum
/// that decouples CPU recording from GPU execution). Duplicated from
/// [`super::texture_pool`]'s private constant deliberately: the value is a
/// 1-LOC `const`, both pools amend it together if N changes, and hoisting
/// to a shared module would introduce churn disproportionate to the
/// payoff at this dispatch's scope.
const FRAMES_IN_FLIGHT: usize = 2;

/// Per-slot state in the [`BufferPool`] ring.
#[derive(Default)]
struct SlotState {
    /// Free-list of buffers returned to the slot at slot re-entry,
    /// keyed per ADR-118 D3.
    free_lists: HashMap<(BufferDescriptor, AliasingGroupId), Vec<Arc<wgpu::Buffer>>>,
    /// Currently-active acquisitions for THIS frame. The descriptor is
    /// carried alongside each `Arc` because wgpu 29's `Buffer` does not
    /// expose the original `BufferDescriptor` for free-list routing at
    /// slot reset.
    active: Vec<(BufferDescriptor, AliasingGroupId, Arc<wgpu::Buffer>)>,
}

/// Latest-only ring-buffered transient-buffer pool per ADR-118 D2 + D3 +
/// D4.
///
/// Owns `N = FRAMES_IN_FLIGHT` slot rings; rotates the active slot on each
/// [`begin_frame`](Self::begin_frame) call. Single-threaded by construction
/// (`&mut self` on every state-mutating method, mirroring
/// [`super::texture_pool::TexturePool`] and
/// [`crate::pso_cache::PipelineCache`]); cross-thread wrapping is the
/// caller's responsibility per ADR-118 non-decision #1. Per ADR-118 D6,
/// the substrate trusts wgpu's auto-tracked hazard barriers — the only
/// correctness contract is that the caller stops recording against the
/// buffer before the next slot rotation retires the GPU work.
pub struct BufferPool {
    slots: [SlotState; FRAMES_IN_FLIGHT],
    /// Index into `slots` for the active frame. Starts at 0; the first
    /// `begin_frame` call rotates to slot 1; the second back to slot 0.
    current_slot: usize,
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferPool {
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
    /// (sibling concern per the dispatch-121 scope). For the MVP,
    /// callers running synchronous `queue.submit` + readback workflows
    /// satisfy the contract trivially.
    pub fn begin_frame(&mut self) {
        self.current_slot = (self.current_slot + 1) % FRAMES_IN_FLIGHT;
        let slot = &mut self.slots[self.current_slot];
        for (desc, group, arc) in slot.active.drain(..) {
            slot.free_lists.entry((desc, group)).or_default().push(arc);
        }
    }

    /// Acquire a buffer matching `descriptor` for `group`.
    ///
    /// Returns the same `Arc<wgpu::Buffer>` as a previous acquisition if
    /// a slot free-list entry matches `(descriptor, group)`; otherwise
    /// allocates fresh via `device.create_buffer(...)`. Per ADR-118 D5,
    /// the caller pre-computes which descriptor to use (the max descriptor
    /// in the group); this method consumes the chosen descriptor without
    /// further reasoning.
    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        descriptor: &BufferDescriptor,
        group: AliasingGroupId,
    ) -> Arc<wgpu::Buffer> {
        let slot = &mut self.slots[self.current_slot];
        let key = (*descriptor, group);
        let arc = slot
            .free_lists
            .get_mut(&key)
            .and_then(Vec::pop)
            .unwrap_or_else(|| allocate_buffer(device, descriptor));
        slot.active.push((*descriptor, group, Arc::clone(&arc)));
        arc
    }
}

/// Allocate a fresh `wgpu::Buffer` matching `descriptor`. `label` is
/// `None` per ADR-118 D1 (pool-key-clean descriptors); `mapped_at_creation`
/// is `false` per ADR-118 D1 (transient resources never CPU-map at
/// creation; staging-buffer workflows live elsewhere).
fn allocate_buffer(device: &wgpu::Device, descriptor: &BufferDescriptor) -> Arc<wgpu::Buffer> {
    Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: descriptor.size_bytes,
        usage: descriptor.usage,
        mapped_at_creation: false,
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

    fn sample_descriptor() -> BufferDescriptor {
        BufferDescriptor {
            size_bytes: 4096,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        }
    }

    // BP-1: structural — fresh pool starts empty at slot 0; begin_frame
    // rotates through 0 → 1 → 0 → 1.
    #[test]
    fn new_pool_starts_at_slot_zero_and_begin_frame_rotates() {
        let mut pool = BufferPool::new();
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

    // BP-2: GPU-gated — acquire returns a buffer (strong_count proves
    // the pool retained one Arc in active; caller holds the other).
    #[test]
    fn acquire_with_real_device_returns_buffer() {
        let ctx = ctx_or_skip!();
        let mut pool = BufferPool::new();
        let arc = pool.acquire(ctx.device(), &sample_descriptor(), AliasingGroupId(0));
        assert_eq!(Arc::strong_count(&arc), 2);
    }

    // BP-3: GPU-gated — full ring round-trip + acquire returns the SAME
    // Arc<wgpu::Buffer> via the free-list (cache hit via pointer equality).
    //
    // Sequence: acquire X (slot 0 active). begin_frame → slot 1 (nothing
    // to drain). begin_frame → slot 0 (slot 0's active drains into its
    // free_list). acquire same key → pops X from slot 0's free_list.
    #[test]
    fn acquire_then_full_ring_returns_buffer_to_free_list() {
        let ctx = ctx_or_skip!();
        let mut pool = BufferPool::new();
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
            "free-list reuse must return the same wgpu::Buffer allocation"
        );
    }

    // BP-4: GPU-gated — distinct AliasingGroupIds get distinct buffers
    // per ADR-118 D3 (key includes the group id).
    #[test]
    fn different_groups_with_same_descriptor_get_distinct_buffers() {
        let ctx = ctx_or_skip!();
        let mut pool = BufferPool::new();
        let desc = sample_descriptor();
        let a = pool.acquire(ctx.device(), &desc, AliasingGroupId(0));
        let b = pool.acquire(ctx.device(), &desc, AliasingGroupId(1));
        assert!(
            !Arc::ptr_eq(&a, &b),
            "distinct AliasingGroupIds must produce distinct allocations even with \
             identical descriptors (ADR-118 D3 key)"
        );
    }

    // BP-5: GPU-gated — distinct descriptors in the same group get
    // distinct buffers.
    #[test]
    fn different_descriptors_in_same_group_get_distinct_buffers() {
        let ctx = ctx_or_skip!();
        let mut pool = BufferPool::new();
        let small = sample_descriptor();
        let large = BufferDescriptor {
            size_bytes: 8192,
            ..small
        };
        let a = pool.acquire(ctx.device(), &small, AliasingGroupId(0));
        let b = pool.acquire(ctx.device(), &large, AliasingGroupId(0));
        assert!(!Arc::ptr_eq(&a, &b));
    }
}
