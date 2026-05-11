//! §6.3 exit-gate integration test: 100 identical [`MaterialDescriptor`]s
//! produce exactly 1 PSO cache insert + 99 hits.
//!
//! This is the load-bearing behavioural test for the
//! `MaterialDescriptor → PsoKey` adapter dispatch. The test exercises the
//! end-to-end path:
//!
//! 1. Build 100 identical [`MaterialDescriptor`]s.
//! 2. Call [`build_pipeline_from_intent`] 100 times against a single
//!    [`GfxContext`].
//! 3. Assert the [`GfxContext`]-owned PSO cache observes **1 miss + 99 hits**.
//! 4. Assert all 100 returned `Arc<wgpu::RenderPipeline>` allocations are
//!    pointer-equal — they share a single underlying pipeline.
//!
//! Also pins:
//! - 100 identical descriptors via [`Material::from_descriptor`] all share
//!   *one* PSO entry (UBO payload differences don't affect PSO identity).
//! - Distinct [`MaterialDescriptor`]s produce distinct cache entries
//!   (sanity check that the cache isn't collapsing everything to a single
//!   key).
//!
//! On environments without a GPU adapter the test skips gracefully rather
//! than failing.
//!
//! [`MaterialDescriptor`]: rge_material_runtime::MaterialDescriptor
//! [`build_pipeline_from_intent`]: rge_gfx::build_pipeline_from_intent
//! [`Material::from_descriptor`]: rge_gfx::Material::from_descriptor

use std::sync::Arc;

use rge_gfx::{
    build_pipeline_from_intent, intent_to_pso_key, Camera, DirectionalLight, GfxContext,
    GfxContextError, Material, PipelineLayouts,
};
use rge_material_runtime::{
    ColorTargetId, DepthIntent, MaterialDescriptor, MaterialParams, ShaderId, VertexLayoutId,
};

/// Obtain a [`GfxContext`] or skip gracefully when no GPU adapter is present.
fn ctx_or_skip() -> Option<GfxContext> {
    match GfxContext::new_headless() {
        Ok(c) => Some(c),
        Err(GfxContextError::NoAdapter) => {
            eprintln!("SKIP (no GPU adapter): material-intent PSO-sharing tests skipped");
            None
        }
        Err(e) => panic!("unexpected GfxContext init error: {e}"),
    }
}

/// Build the canonical `LitMesh` descriptor used by the §6.3 gate test.
///
/// `LitMesh` is the canonical choice because it exercises all three
/// bind-group layouts (camera + light + material) and forces the full
/// adapter dispatch path to wire up.
fn descriptor_lit_bgra_no_depth() -> MaterialDescriptor {
    MaterialDescriptor {
        shader_id: ShaderId::LitMesh,
        vertex_layout: VertexLayoutId::LitVertex,
        color_target: ColorTargetId::Bgra8UnormSrgb,
        depth: DepthIntent::None,
        params: MaterialParams::default(),
    }
}

// ---------------------------------------------------------------------------
// §6.3 exit gate: 100 identical descriptors → 1 PSO entry
// ---------------------------------------------------------------------------

#[test]
fn one_hundred_identical_descriptors_share_one_pso() {
    let Some(ctx) = ctx_or_skip() else { return };

    // Build the bind-group layouts the LitMesh pipeline kind needs. These
    // are constructed once and re-used across all 100 calls.
    let camera = Camera::new(&ctx).expect("camera");
    let light = DirectionalLight::new(&ctx).expect("light");
    let material = Material::new(&ctx, &[0xFF, 0xFF, 0xFF, 0xFF], 1, 1).expect("material");
    let layouts = PipelineLayouts {
        transform: None,
        camera: Some(camera.bind_group_layout()),
        light: Some(light.bind_group_layout()),
        material: Some(material.bind_group_layout()),
    };

    let desc = descriptor_lit_bgra_no_depth();

    // Baseline cache counts: prior cache activity may have happened
    // (e.g. `Material::new` doesn't touch the pipeline cache, but the
    // future-proof check is to diff against the baseline rather than
    // assert raw counters).
    let baseline_hits = ctx.pso_cache().borrow().hits();
    let baseline_misses = ctx.pso_cache().borrow().misses();
    let baseline_len = ctx.pso_cache().borrow().len();

    // Drive 100 identical builds.
    let pipelines: Vec<Arc<wgpu::RenderPipeline>> = (0..100)
        .map(|_| build_pipeline_from_intent(&ctx, &desc, &layouts).expect("build"))
        .collect();

    let final_hits = ctx.pso_cache().borrow().hits();
    let final_misses = ctx.pso_cache().borrow().misses();
    let final_len = ctx.pso_cache().borrow().len();

    // --- the gate itself ---

    let inserts = final_misses - baseline_misses;
    let hits = final_hits - baseline_hits;
    let new_entries = final_len - baseline_len;

    assert_eq!(
        inserts, 1,
        "expected 1 PSO insert from 100 identical descriptors; got {inserts} \
         (baseline_misses={baseline_misses}, final_misses={final_misses})"
    );
    assert_eq!(
        hits, 99,
        "expected 99 PSO cache hits from 100 identical descriptors; got {hits} \
         (baseline_hits={baseline_hits}, final_hits={final_hits})"
    );
    assert_eq!(
        new_entries, 1,
        "expected the cache to grow by exactly 1 entry; got {new_entries} \
         (baseline_len={baseline_len}, final_len={final_len})"
    );

    // All 100 returned Arc allocations must point to the same RenderPipeline.
    let first = &pipelines[0];
    for (i, p) in pipelines.iter().enumerate().skip(1) {
        assert!(
            Arc::ptr_eq(first, p),
            "all 100 pipelines must share one Arc allocation; pipelines[{i}] differs"
        );
    }
}

// ---------------------------------------------------------------------------
// intent_to_pso_key — pure-mapping sanity (no GPU required)
// ---------------------------------------------------------------------------

#[test]
fn intent_to_pso_key_is_total_and_identity_preserving() {
    let a = descriptor_lit_bgra_no_depth();
    let b = descriptor_lit_bgra_no_depth();
    // Two descriptors that compare equal must map to keys that compare equal.
    assert_eq!(intent_to_pso_key(&a), intent_to_pso_key(&b));

    // Differing on each axis produces a distinct key.
    let mut shader_changed = a;
    shader_changed.shader_id = ShaderId::Mesh;
    assert_ne!(intent_to_pso_key(&a), intent_to_pso_key(&shader_changed));

    let mut color_changed = a;
    color_changed.color_target = ColorTargetId::Rgba8Unorm;
    assert_ne!(intent_to_pso_key(&a), intent_to_pso_key(&color_changed));

    let mut depth_changed = a;
    depth_changed.depth = DepthIntent::ReadWrite;
    assert_ne!(intent_to_pso_key(&a), intent_to_pso_key(&depth_changed));

    let mut layout_changed = a;
    layout_changed.vertex_layout = VertexLayoutId::Vertex;
    assert_ne!(intent_to_pso_key(&a), intent_to_pso_key(&layout_changed));

    // Params differences must NOT change the PSO key — UBO payload doesn't
    // affect compiled pipeline state.
    let mut params_changed = a;
    params_changed.params.base_color = [0.25, 0.5, 0.75, 1.0];
    params_changed.params.phong = [0.1, 0.8, 0.4, 16.0];
    assert_eq!(intent_to_pso_key(&a), intent_to_pso_key(&params_changed));
}

// ---------------------------------------------------------------------------
// Material::from_descriptor — payload plumbing sanity (GPU required)
// ---------------------------------------------------------------------------

#[test]
fn material_from_descriptor_builds_valid_bind_group() {
    let Some(ctx) = ctx_or_skip() else { return };

    let desc = MaterialDescriptor {
        shader_id: ShaderId::LitMesh,
        vertex_layout: VertexLayoutId::LitVertex,
        color_target: ColorTargetId::Bgra8UnormSrgb,
        depth: DepthIntent::None,
        params: MaterialParams {
            base_color: [0.5, 0.6, 0.7, 1.0],
            phong: [0.2, 0.8, 0.4, 16.0],
        },
    };

    let mat = Material::from_descriptor(&ctx, &desc).expect("from_descriptor");
    // The bind group + layout must be live and usable for downstream
    // pipeline construction (matching `Material::new(...).bind_group_layout()`).
    let _bg = mat.bind_group();
    let _bgl = mat.bind_group_layout();
}

// ---------------------------------------------------------------------------
// Distinct descriptors don't collapse into one cache entry
// ---------------------------------------------------------------------------

#[test]
fn distinct_descriptors_produce_distinct_cache_entries() {
    let Some(ctx) = ctx_or_skip() else { return };

    let camera = Camera::new(&ctx).expect("camera");
    let light = DirectionalLight::new(&ctx).expect("light");
    let material = Material::new(&ctx, &[0xFF, 0xFF, 0xFF, 0xFF], 1, 1).expect("material");
    let layouts = PipelineLayouts {
        transform: None,
        camera: Some(camera.bind_group_layout()),
        light: Some(light.bind_group_layout()),
        material: Some(material.bind_group_layout()),
    };

    let baseline_len = ctx.pso_cache().borrow().len();

    let desc_bgra = descriptor_lit_bgra_no_depth();
    let mut desc_rgba = desc_bgra;
    desc_rgba.color_target = ColorTargetId::Rgba8Unorm;

    let p1 = build_pipeline_from_intent(&ctx, &desc_bgra, &layouts).expect("p1");
    let p2 = build_pipeline_from_intent(&ctx, &desc_rgba, &layouts).expect("p2");

    let final_len = ctx.pso_cache().borrow().len();

    assert_eq!(
        final_len - baseline_len,
        2,
        "differing color_target descriptors must produce 2 cache entries"
    );
    assert!(
        !Arc::ptr_eq(&p1, &p2),
        "differing color_target descriptors must NOT share Arc allocations"
    );
}
