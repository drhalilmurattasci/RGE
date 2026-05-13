//! Phase 6 §6.3 Gate A — 60fps simple-scene golden harness, **post-depth variant**.
//!
//! Release-only `#[ignore]` headless wgpu render-loop benchmark.
//! Mirrors the pre-depth `gate_a_simple_scene_60fps.rs` harness in
//! every measurable dimension — same 1000-cube scene, same camera,
//! same warmup / sample / run counts, same P95 + variance gates —
//! but constructs the pipeline via
//! `LitMeshPipeline::new_with_depth(.., Some(DepthStateKey { Depth24Plus,
//! depth_write_enabled: false, LessEqual }))` and passes
//! `Some(&depth_view)` to `record_lit_mesh_pass(...)` so the recorded
//! frame timings include the depth-attached pass cost — the API path
//! that editor-shell production actually consumes post-sub-β.
//!
//! **Scope limitation**: same recorder-host-only scope as the pre-depth
//! Gate A test. Does NOT certify universal 60fps, vendor parity,
//! cold-start, sustained thermal, realistic geometry complexity, CI
//! regression coverage, or editor-shell `render_frame` end-to-end. See
//! `plans/BASELINE.md` §6.3 and the post-depth measurement note.
//!
//! **Rendering strategy**: identical to the pre-depth harness — option
//! (a) baked-into-a-single-VB-IB world-space geometry, one `draw_indexed`
//! per frame. The only delta from the pre-depth harness is the
//! pipeline construction (depth state) and the depth-view argument
//! to `record_lit_mesh_pass`. Zero non-test `crates/gfx/src/` edits.
//!
//! **Why duplicate the pre-depth helpers** (`push_cube`, `ctx_or_skip`,
//! the scene-build flow): production abstraction across integration
//! tests is intentionally avoided per the TASK packet's `Constraints /
//! Non-Goals` ("Do not change LitMeshPipeline, record_lit_mesh_pass,
//! TexturePool, FrameGraph, ResourceMap, or PSO cache behavior"). Test
//! helpers are local to each integration-test binary; sharing them
//! would require a production abstraction this dispatch is forbidden
//! to introduce.

use std::sync::Arc;
use std::time::Instant;

use rge_gfx::{
    record_lit_mesh_pass, Camera, DepthStateKey, DirectionalLight, GfxContext, HeadlessTarget,
    LitMesh, LitMeshPipeline, Material, VertexLit,
};

const GRID_DIM: usize = 10;
const CUBE_COUNT: usize = GRID_DIM * GRID_DIM * GRID_DIM; // 1000
const SPACING: f32 = 2.0;
const CAMERA_Z: f32 = -40.0;
const VIEWPORT_W: u32 = 1280;
const VIEWPORT_H: u32 = 720;
const WARMUP_FRAMES: usize = 60;
const SAMPLE_FRAMES: usize = 600;
const RUNS: usize = 3;
const GATE_P95_MS: f64 = 16.67;
const VARIANCE_GATE_PCT: f64 = 30.0;

fn ctx_or_skip() -> Option<GfxContext> {
    match GfxContext::new_headless() {
        Ok(c) => Some(c),
        Err(_) => {
            eprintln!("SKIP (no GPU adapter): Gate A simple-scene post-depth 60fps test skipped");
            None
        }
    }
}

/// Append one 1×1×1 cube centred on `origin` to the running VB/IB. 24
/// vertices (4 per face, per-face normals split, CCW from outside matching
/// `LitMeshPipeline::front_face: Ccw`) + 36 indices (12 triangles).
///
/// Byte-for-byte mirror of `gate_a_simple_scene_60fps.rs::push_cube`.
fn push_cube(vertices: &mut Vec<VertexLit>, indices: &mut Vec<u32>, origin: [f32; 3]) {
    let [cx, cy, cz] = origin;
    let p = |dx: f32, dy: f32, dz: f32| [cx + dx, cy + dy, cz + dz];
    let c = [
        p(-0.5, -0.5, -0.5), // 0 nnn
        p(0.5, -0.5, -0.5),  // 1 pnn
        p(0.5, 0.5, -0.5),   // 2 ppn
        p(-0.5, 0.5, -0.5),  // 3 npn
        p(-0.5, -0.5, 0.5),  // 4 nnp
        p(0.5, -0.5, 0.5),   // 5 pnp
        p(0.5, 0.5, 0.5),    // 6 ppp
        p(-0.5, 0.5, 0.5),   // 7 npp
    ];
    let faces: [([usize; 4], [f32; 3]); 6] = [
        ([0, 3, 2, 1], [0.0, 0.0, -1.0]), // NegZ
        ([4, 5, 6, 7], [0.0, 0.0, 1.0]),  // PosZ
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]), // NegY
        ([3, 7, 6, 2], [0.0, 1.0, 0.0]),  // PosY
        ([0, 4, 7, 3], [-1.0, 0.0, 0.0]), // NegX
        ([1, 2, 6, 5], [1.0, 0.0, 0.0]),  // PosX
    ];
    let uv = [0.0, 0.0];
    for (corners, normal) in faces {
        let base = u32::try_from(vertices.len()).unwrap();
        for &ci in &corners {
            vertices.push(VertexLit::new(c[ci], normal, uv));
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Pre-baked GPU resources for the post-depth simple scene — one
/// allocation per kind plus the depth texture + view.
struct SimpleSceneWithDepth {
    target: HeadlessTarget,
    pipeline: LitMeshPipeline,
    camera: Camera,
    light: DirectionalLight,
    material: Material,
    mesh: LitMesh,
    /// Owns the depth texture so its `wgpu::TextureView` (held inside
    /// `depth_view`) outlives every frame. The wgpu API takes views by
    /// reference; the underlying texture must remain live across the
    /// full measurement loop.
    _depth_texture: Arc<wgpu::Texture>,
    depth_view: wgpu::TextureView,
}

fn build_simple_scene_with_depth(ctx: &GfxContext) -> SimpleSceneWithDepth {
    let target = HeadlessTarget::new(ctx, VIEWPORT_W, VIEWPORT_H).expect("headless target");

    // Camera (mirrors pre-depth harness).
    let camera = Camera::new(ctx).expect("camera");
    let aspect = VIEWPORT_W as f32 / VIEWPORT_H as f32;
    let proj = glam::Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.1, 200.0);
    let view = glam::Mat4::look_at_rh(
        glam::Vec3::new(0.0, 0.0, CAMERA_Z),
        glam::Vec3::ZERO,
        glam::Vec3::Y,
    );
    camera.update(ctx, proj * view, glam::Mat4::IDENTITY);

    // DirectionalLight pointing +Z (mirrors pre-depth harness).
    let light = DirectionalLight::new(ctx).expect("light");
    light.update(ctx, glam::Vec3::new(0.0, 0.0, 1.0), glam::Vec3::ONE);

    // Single shared material (mirrors pre-depth harness).
    let white_4x4: Vec<u8> = vec![255u8; 4 * 4 * 4];
    let material = Material::new(ctx, &white_4x4, 4, 4).expect("material");

    // **Post-depth pipeline** — the only structural difference from the
    // pre-depth harness. `DepthStateKey` matches editor-shell production
    // post-sub-β EXACTLY: `Depth24Plus` + `depth_write_enabled: false`
    // + `LessEqual` (the same configuration verified pixel-correct by
    // `lit_mesh_depth_overlay_smoke.rs`).
    let depth_state = DepthStateKey::new(
        wgpu::TextureFormat::Depth24Plus,
        false,
        wgpu::CompareFunction::LessEqual,
    );
    let pipeline = LitMeshPipeline::new_with_depth(
        ctx,
        camera.bind_group_layout(),
        light.bind_group_layout(),
        material.bind_group_layout(),
        target.format(),
        Some(depth_state),
    )
    .expect("pipeline with depth");

    // Per-frame depth texture. Allocated once and reused across all
    // frames — same texture, same view, `record_lit_mesh_pass` clears
    // it to 1.0 at every frame's `LoadOp::Clear(1.0)`. No `TexturePool`
    // / `FrameGraph` substrate involvement (those are tested separately
    // by the frame_graph_* tests; this harness is the gfx-level
    // primitives-only measurement).
    let depth_texture = ctx.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("GateASimpleSceneDepth60fpsDepth"),
        size: wgpu::Extent3d {
            width: VIEWPORT_W,
            height: VIEWPORT_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24Plus,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_texture = Arc::new(depth_texture);

    // Bake all 1000 cubes into one VB/IB (mirrors pre-depth harness).
    let mut vertices: Vec<VertexLit> = Vec::with_capacity(24 * CUBE_COUNT);
    let mut indices: Vec<u32> = Vec::with_capacity(36 * CUBE_COUNT);
    let half = (GRID_DIM as f32 - 1.0) * 0.5 * SPACING;
    for ix in 0..GRID_DIM {
        for iy in 0..GRID_DIM {
            for iz in 0..GRID_DIM {
                push_cube(
                    &mut vertices,
                    &mut indices,
                    [
                        ix as f32 * SPACING - half,
                        iy as f32 * SPACING - half,
                        iz as f32 * SPACING - half,
                    ],
                );
            }
        }
    }
    let mesh = LitMesh::from_indexed(ctx, &vertices, &indices).expect("scene mesh");

    SimpleSceneWithDepth {
        target,
        pipeline,
        camera,
        light,
        material,
        mesh,
        _depth_texture: depth_texture,
        depth_view,
    }
}

/// Encode + submit one frame with depth attachment; block until GPU
/// work completes so the wall-clock timer captures the full frame.
/// The only delta from the pre-depth `render_one_frame` is the final
/// argument to `record_lit_mesh_pass`: `Some(&scene.depth_view)`
/// instead of `None`.
fn render_one_frame_with_depth(ctx: &GfxContext, scene: &SimpleSceneWithDepth) {
    let mut encoder = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GateADepthFrame"),
        });
    record_lit_mesh_pass(
        &mut encoder,
        &scene.target,
        &scene.pipeline,
        &scene.camera,
        &scene.light,
        &scene.material,
        &scene.mesh,
        wgpu::Color::BLACK,
        Some(&scene.depth_view),
    );
    ctx.queue().submit(std::iter::once(encoder.finish()));
    let _ = ctx.device().poll(wgpu::PollType::wait_indefinitely());
}

#[test]
#[ignore = "Phase 6 §6.3 Gate A — post-depth variant; release-only, GPU-dependent perf gate; invoke with: cargo test -p rge-gfx --release --test gate_a_simple_scene_depth_60fps -- --ignored --nocapture"]
fn gate_a_simple_scene_depth_60fps() {
    let Some(ctx) = ctx_or_skip() else { return };

    let info = ctx.adapter_info();
    eprintln!(
        "Gate A (post-depth) adapter: name={:?} backend={:?} device_type={:?} driver={:?}",
        info.name, info.backend, info.device_type, info.driver
    );
    eprintln!(
        "Gate A (post-depth) scene: {} cubes ({}x{}x{} grid, spacing={}), viewport {}x{}, \
         camera Z={}, warmup={}, sample={}, runs={}; depth state = Depth24Plus / \
         depth_write_enabled=false / LessEqual",
        CUBE_COUNT,
        GRID_DIM,
        GRID_DIM,
        GRID_DIM,
        SPACING,
        VIEWPORT_W,
        VIEWPORT_H,
        CAMERA_Z,
        WARMUP_FRAMES,
        SAMPLE_FRAMES,
        RUNS
    );

    let scene = build_simple_scene_with_depth(&ctx);

    let mut run_p50_ms: Vec<f64> = Vec::with_capacity(RUNS);
    let mut run_p95_ms: Vec<f64> = Vec::with_capacity(RUNS);
    let mut run_max_ms: Vec<f64> = Vec::with_capacity(RUNS);

    for run in 0..RUNS {
        for _ in 0..WARMUP_FRAMES {
            render_one_frame_with_depth(&ctx, &scene);
        }
        let mut frame_ms: Vec<f64> = Vec::with_capacity(SAMPLE_FRAMES);
        for _ in 0..SAMPLE_FRAMES {
            let start = Instant::now();
            render_one_frame_with_depth(&ctx, &scene);
            frame_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        frame_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = frame_ms[SAMPLE_FRAMES / 2];
        let p95 = frame_ms[(SAMPLE_FRAMES * 95) / 100];
        let max = *frame_ms.last().unwrap();
        eprintln!(
            "Gate A (post-depth) run {run}: P50={p50:.3} ms, P95={p95:.3} ms, max={max:.3} ms"
        );
        run_p50_ms.push(p50);
        run_p95_ms.push(p95);
        run_max_ms.push(max);
    }

    let sort = |v: &mut Vec<f64>| v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sort(&mut run_p50_ms);
    sort(&mut run_p95_ms);
    let (min_p95, median_p95, max_p95) =
        (run_p95_ms[0], run_p95_ms[RUNS / 2], run_p95_ms[RUNS - 1]);
    let variance_pct = (max_p95 - min_p95) / median_p95 * 100.0;
    let max_max = run_max_ms.iter().cloned().fold(0.0_f64, f64::max);
    eprintln!(
        "Gate A (post-depth, simple-scene 60fps): median P50 = {:.3} ms, min P95 = {min_p95:.3} ms, \
         median P95 = {median_p95:.3} ms, max P95 = {max_p95:.3} ms, \
         worst frame = {max_max:.3} ms, variance across runs = {variance_pct:.1}%",
        run_p50_ms[RUNS / 2]
    );

    assert!(
        variance_pct <= VARIANCE_GATE_PCT,
        "variance {variance_pct:.1}% exceeds {VARIANCE_GATE_PCT}% — \
         measurement unstable; record then escalate"
    );
    assert!(
        min_p95 <= GATE_P95_MS,
        "min-of-{RUNS} P95 = {min_p95:.3} ms exceeds Gate A threshold {GATE_P95_MS} ms — \
         record then escalate, do not tune"
    );
}
