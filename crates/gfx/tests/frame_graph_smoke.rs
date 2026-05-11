//! Phase 6 frame-graph minimal substrate — integration smoke.
//!
//! End-to-end exercise of the public surface (no `wgpu` calls, no GPU
//! allocation). Mirrors the substrate-cavity smoke pattern from
//! `kernel/io-scheduler::lib::smoke` — confirms a small DAG of passes
//! compiles into a deterministic execution order with sensible lifetimes
//! and aliasing groups.
//!
//! Concretely models a three-pass shadow-pass / scene-pass / lighting-pass
//! sketch:
//!
//! 1. `geometry` writes `depth` + `scene_color`
//! 2. `shadow` reads `depth`, writes `shadow_map`
//! 3. `lighting` reads `scene_color` + `shadow_map`, writes `lit`
//!
//! `depth` and `shadow_map` should be aliasable (their lifetimes do not
//! overlap once `lighting` consumes `shadow_map` after `geometry` no
//! longer needs `depth`). The smoke asserts the structural properties
//! without prescribing which group each lands in (the greedy assignment
//! is order-dependent on `ResourceId` byte order).
//!
//! # Dispatch 119 — descriptor flow
//!
//! Each write declaration carries a `ResourceClassDescriptor` per ADR-118
//! D7. The smoke uses realistic descriptors (a `Depth32Float` depth
//! target, an `Rgba8Unorm` scene-color target, an `R32Float` shadow map,
//! and an `Rgba8Unorm` final lit target) and asserts the compiled output
//! preserves the per-resource descriptors verbatim.

use rge_gfx::frame_graph::{
    BufferDescriptor, FrameGraph, ResourceClassDescriptor, ResourceId, TextureDescriptor,
};

fn depth_descriptor() -> ResourceClassDescriptor {
    ResourceClassDescriptor::Texture(TextureDescriptor {
        width: 1920,
        height: 1080,
        depth_or_array_layers: 1,
        mip_level_count: 1,
        sample_count: 1,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        dimension: wgpu::TextureDimension::D2,
        view_dimension: wgpu::TextureViewDimension::D2,
    })
}

fn color_descriptor() -> ResourceClassDescriptor {
    ResourceClassDescriptor::Texture(TextureDescriptor {
        width: 1920,
        height: 1080,
        depth_or_array_layers: 1,
        mip_level_count: 1,
        sample_count: 1,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        dimension: wgpu::TextureDimension::D2,
        view_dimension: wgpu::TextureViewDimension::D2,
    })
}

fn shadow_descriptor() -> ResourceClassDescriptor {
    ResourceClassDescriptor::Texture(TextureDescriptor {
        width: 2048,
        height: 2048,
        depth_or_array_layers: 1,
        mip_level_count: 1,
        sample_count: 1,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        dimension: wgpu::TextureDimension::D2,
        view_dimension: wgpu::TextureViewDimension::D2,
    })
}

#[test]
fn frame_graph_three_pass_pipeline_smoke() {
    let mut fg = FrameGraph::new();

    let depth = ResourceId::from_bytes([0x01; 16]);
    let scene_color = ResourceId::from_bytes([0x02; 16]);
    let shadow_map = ResourceId::from_bytes([0x03; 16]);
    let lit = ResourceId::from_bytes([0x04; 16]);

    let geom = fg
        .add_pass(
            "geometry",
            vec![],
            vec![
                (depth, depth_descriptor()),
                (scene_color, color_descriptor()),
            ],
        )
        .expect("geometry");
    let shadow = fg
        .add_pass(
            "shadow",
            vec![depth],
            vec![(shadow_map, shadow_descriptor())],
        )
        .expect("shadow");
    let lighting = fg
        .add_pass(
            "lighting",
            vec![scene_color, shadow_map],
            vec![(lit, color_descriptor())],
        )
        .expect("lighting");

    let compiled = fg.compile().expect("compile");

    assert_eq!(compiled.pass_count(), 3);

    // Geometry must precede every consumer.
    let order = compiled.execution_order();
    let pos_geom = order.iter().position(|n| *n == geom).unwrap();
    let pos_shadow = order.iter().position(|n| *n == shadow).unwrap();
    let pos_lighting = order.iter().position(|n| *n == lighting).unwrap();
    assert!(pos_geom < pos_shadow);
    assert!(pos_geom < pos_lighting);

    // Shadow must precede lighting (lighting reads shadow_map which shadow
    // writes).
    assert!(pos_shadow < pos_lighting);

    // depth and scene_color first appear at pos_geom.
    let depth_lt = compiled.resource_lifetime(depth).expect("depth lifetime");
    let scene_lt = compiled
        .resource_lifetime(scene_color)
        .expect("scene_color lifetime");
    assert_eq!(depth_lt.first_use, pos_geom);
    assert_eq!(scene_lt.first_use, pos_geom);

    // depth's last_use is shadow (depth not read after shadow).
    assert_eq!(depth_lt.last_use, pos_shadow);

    // scene_color's last_use is lighting.
    assert_eq!(scene_lt.last_use, pos_lighting);

    // lit appears only at lighting.
    let lit_lt = compiled.resource_lifetime(lit).expect("lit lifetime");
    assert_eq!(lit_lt.first_use, lit_lt.last_use);
    assert_eq!(lit_lt.first_use, pos_lighting);

    // Recompile is byte-identical (deterministic substrate). Descriptors
    // do NOT enter the structural hash, so the hash matches even with the
    // descriptor sidecar populated.
    let recompiled = fg.compile().expect("recompile");
    assert_eq!(compiled.structural_hash(), recompiled.structural_hash());
    assert_eq!(compiled.execution_order(), recompiled.execution_order());

    // depth and lit have non-overlapping lifetimes ([pos_geom, pos_shadow]
    // vs [pos_lighting, pos_lighting]) — they should be aliasable.
    assert!(!depth_lt.overlaps(&lit_lt));

    // Aliasing groups are non-empty and cover every declared resource.
    let groups = compiled.aliasing_groups();
    assert!(!groups.is_empty());
    let mut all_resources: Vec<ResourceId> =
        groups.iter().flat_map(|g| g.0.iter().copied()).collect();
    all_resources.sort();
    let mut expected = vec![depth, scene_color, shadow_map, lit];
    expected.sort();
    assert_eq!(all_resources, expected);

    // Dispatch 119 — descriptor flow assertions.
    assert_eq!(compiled.descriptors().len(), 4);
    assert_eq!(compiled.descriptor(depth), Some(&depth_descriptor()));
    assert_eq!(compiled.descriptor(scene_color), Some(&color_descriptor()));
    assert_eq!(compiled.descriptor(shadow_map), Some(&shadow_descriptor()));
    assert_eq!(compiled.descriptor(lit), Some(&color_descriptor()));
    // Re-compile carries the same descriptors.
    assert_eq!(recompiled.descriptors().len(), 4);
    assert_eq!(recompiled.descriptor(depth), Some(&depth_descriptor()));
}

#[test]
fn frame_graph_smoke_buffer_descriptor_round_trip() {
    // Substrate honesty: the descriptor flow is buffer-class as well as
    // texture-class. Smoke a small two-pass graph that uses a uniform
    // buffer for a transform feedback / staging shape.
    let mut fg = FrameGraph::new();

    let transforms = ResourceId::from_bytes([0xa1; 16]);
    let lit = ResourceId::from_bytes([0xa2; 16]);

    let transforms_desc = ResourceClassDescriptor::Buffer(BufferDescriptor {
        size_bytes: 64 * 256, // 256 entries of mat4
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let lit_desc = color_descriptor();

    fg.add_pass("upload", vec![], vec![(transforms, transforms_desc)])
        .expect("upload");
    fg.add_pass("render", vec![transforms], vec![(lit, lit_desc)])
        .expect("render");

    let compiled = fg.compile().expect("compile");
    assert_eq!(compiled.descriptors().len(), 2);
    assert!(matches!(
        compiled.descriptor(transforms),
        Some(ResourceClassDescriptor::Buffer(_))
    ));
    assert!(matches!(
        compiled.descriptor(lit),
        Some(ResourceClassDescriptor::Texture(_))
    ));
}
