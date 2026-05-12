//! Frame-graph chapter umbrella analytical smoke (post-dispatch-122).
//!
//! Composes the chapter's substrate end-to-end without touching
//! `FrameRecorder` or real pass-record sites:
//!
//! ```text
//!   FrameGraph::add_pass × 3 → compile()
//!     → TexturePool::new + BufferPool::new
//!       → build_resource_map (frame 0)
//!         → both pools' begin_frame()
//!           → build_resource_map (frame 1)
//! ```
//!
//! Asserts the cross-frame freshness invariant per ADR-118 D4 — the one
//! compositional invariant not covered by isolated pool / map tests today.
//! Within-frame aliasing-group Arc dedup + structural-hash stability across
//! recompile are smoked alongside as regression guards.
//!
//! Phase 6 chapter scope honesty: this dispatch closes the chapter for
//! "substrate complete + cross-frame composable"; runtime-perf re-validation
//! against Gate A's recorder-host CLOSED marker (`IMPLEMENTATION.md:468`,
//! commit `35e5078`) is deferred until real pass-record sites grow
//! transient-resource consumers (`FrameRecorder` is currently triangle-only
//! and bypassed by `editor-shell::render_frame`). This is NOT a 60 fps re-run.

use std::sync::Arc;

use rge_gfx::frame_graph::{
    build_resource_map, BufferPool, FrameGraph, ResourceClassDescriptor, ResourceId,
    TextureDescriptor, TexturePool,
};
use rge_gfx::GfxContext;

fn ctx_or_skip() -> Option<GfxContext> {
    match GfxContext::new_headless() {
        Ok(c) => Some(c),
        Err(_) => {
            eprintln!("SKIP: no GPU adapter — skipping frame_graph_umbrella_smoke");
            None
        }
    }
}

fn tex_desc(format: wgpu::TextureFormat, side: u32) -> ResourceClassDescriptor {
    ResourceClassDescriptor::Texture(TextureDescriptor {
        width: side,
        height: side,
        depth_or_array_layers: 1,
        mip_level_count: 1,
        sample_count: 1,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        dimension: wgpu::TextureDimension::D2,
        view_dimension: wgpu::TextureViewDimension::D2,
    })
}

#[test]
fn umbrella_smoke_full_composition_cross_frame_freshness() {
    let Some(ctx) = ctx_or_skip() else { return };
    let device = ctx.device();

    // 3-pass shadow / scene / lighting graph — matches `frame_graph_smoke.rs`
    // vocabulary at smaller dimensions (the umbrella exercises composition;
    // descriptor sizing is not load-bearing).
    let mut fg = FrameGraph::new();
    let depth = ResourceId::from_bytes([0x01; 16]);
    let scene_color = ResourceId::from_bytes([0x02; 16]);
    let shadow_map = ResourceId::from_bytes([0x03; 16]);
    let lit = ResourceId::from_bytes([0x04; 16]);

    let depth_d = tex_desc(wgpu::TextureFormat::Depth32Float, 256);
    let color_d = tex_desc(wgpu::TextureFormat::Rgba8Unorm, 256);
    let shadow_d = tex_desc(wgpu::TextureFormat::R32Float, 512);

    fg.add_pass(
        "geometry",
        vec![],
        vec![(depth, depth_d), (scene_color, color_d)],
    )
    .expect("geometry add_pass");
    fg.add_pass("shadow", vec![depth], vec![(shadow_map, shadow_d)])
        .expect("shadow add_pass");
    fg.add_pass(
        "lighting",
        vec![scene_color, shadow_map],
        vec![(lit, color_d)],
    )
    .expect("lighting add_pass");

    let compiled = fg.compile().expect("compile succeeds");

    let mut tex_pool = TexturePool::new();
    let mut buf_pool = BufferPool::new();

    // Frame 0.
    let map_frame_0 = build_resource_map(&compiled, device, &mut tex_pool, &mut buf_pool)
        .expect("frame 0 build_resource_map");

    // Assertion 1: every declared ResourceId appears in the map (all-texture
    // graph → texture_map covers every id; buffer_map empty).
    for resource_id in [depth, scene_color, shadow_map, lit] {
        assert!(
            map_frame_0.texture_map.contains_key(&resource_id),
            "frame 0 must cover {resource_id} in texture_map"
        );
    }
    assert!(
        map_frame_0.buffer_map.is_empty(),
        "all-texture graph must leave buffer_map empty"
    );

    // Assertion 2: within-frame aliasing-group Arc dedup. Every multi-member
    // group's members map to the same Arc — the structural enforcement of
    // ADR-118 L213's load-bearing dedup contract at the builder layer.
    let groups = compiled.aliasing_groups();
    let mut multi_member_groups = 0usize;
    for group in groups {
        if group.0.len() <= 1 {
            continue;
        }
        multi_member_groups += 1;
        let first = map_frame_0
            .texture_map
            .get(&group.0[0])
            .expect("group member must be in texture_map for this all-texture graph")
            .clone();
        for resource_id in &group.0[1..] {
            let other = map_frame_0
                .texture_map
                .get(resource_id)
                .expect("group member in map");
            assert!(
                Arc::ptr_eq(&first, other),
                "within-frame: aliasing-group members must share one Arc \
                 (ADR-118 D5 dedup; ResourceId {resource_id})"
            );
        }
    }
    // The 3-pass shadow / scene / lighting shape produces at least one
    // multi-member group (depth + lit have non-overlapping lifetimes per the
    // analytical assertions in `frame_graph_smoke.rs`).
    assert!(
        multi_member_groups >= 1,
        "expected at least one multi-member aliasing group; groups={groups:?}"
    );

    // Capture frame-0 Arcs per-ResourceId for cross-frame comparison.
    let frame_0_arcs: Vec<(ResourceId, Arc<wgpu::Texture>)> = map_frame_0
        .texture_map
        .iter()
        .map(|(rid, arc)| (*rid, Arc::clone(arc)))
        .collect();
    drop(map_frame_0);

    // Frame 1: rotate both pools to slot 1, rebuild. Per ADR-118 D4 with
    // N=2, slot 1 starts empty on its first visit → `build_resource_map`
    // allocates fresh.
    tex_pool.begin_frame();
    buf_pool.begin_frame();

    let map_frame_1 = build_resource_map(&compiled, device, &mut tex_pool, &mut buf_pool)
        .expect("frame 1 build_resource_map");

    // Assertion 3 (LOAD-BEARING): cross-frame freshness — same ResourceId
    // across frames yields DISTINCT Arc allocations. Slot 0 holds frame-0
    // Arcs (in `tex_pool.slots[0].active`); slot 1 holds frame-1 Arcs.
    // ResourceId is stable; physical allocation is per-slot per ADR-118 D4.
    for (resource_id, frame_0_arc) in &frame_0_arcs {
        let frame_1_arc = map_frame_1
            .texture_map
            .get(resource_id)
            .expect("frame 1 must cover the same ResourceId");
        assert!(
            !Arc::ptr_eq(frame_0_arc, frame_1_arc),
            "cross-frame freshness: ResourceId {resource_id} must get a distinct Arc \
             allocation in frame 1 (slot 1) per ADR-118 D4 N=2 ring-buffer policy"
        );
    }

    // Assertion 4: structural_hash stable across recompile (regression guard
    // for the deterministic-substrate contract under umbrella composition).
    let compiled_again = fg.compile().expect("recompile succeeds");
    assert_eq!(
        compiled.structural_hash(),
        compiled_again.structural_hash(),
        "structural_hash must be stable across recompile (deterministic chapter contract)"
    );
}
