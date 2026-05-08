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

use rge_gfx::frame_graph::{FrameGraph, ResourceId};

#[test]
fn frame_graph_three_pass_pipeline_smoke() {
    let mut fg = FrameGraph::new();

    let depth = ResourceId::from_bytes([0x01; 16]);
    let scene_color = ResourceId::from_bytes([0x02; 16]);
    let shadow_map = ResourceId::from_bytes([0x03; 16]);
    let lit = ResourceId::from_bytes([0x04; 16]);

    let geom = fg
        .add_pass("geometry", vec![], vec![depth, scene_color])
        .expect("geometry");
    let shadow = fg
        .add_pass("shadow", vec![depth], vec![shadow_map])
        .expect("shadow");
    let lighting = fg
        .add_pass("lighting", vec![scene_color, shadow_map], vec![lit])
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

    // Recompile is byte-identical (deterministic substrate).
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
}
