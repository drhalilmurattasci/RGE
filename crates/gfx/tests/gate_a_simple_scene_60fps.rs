//! Phase 6 §6.3 Gate A — 60fps simple-scene golden harness.
//!
//! Release-only `#[ignore]` headless wgpu render-loop benchmark.
//! Methodology locked by prior design inspect:
//!   - 1000 cubes (10x10x10 grid, 2-unit spacing)
//!   - 1 DirectionalLight, static camera @ Z=-40, 1280x720 viewport
//!   - shared PSO + 1 material across all 1000 cubes
//!   - 600 frames after 60-frame warmup, min-of-3 runs
//!   - P95 <= 16.67 ms AND variance across runs <= 30%
//!
//! **Scope limitation**: gate verifies 60fps on the recorder's box
//! under the recorded wgpu backend / adapter only. Does NOT certify
//! universal 60fps, vendor parity, cold-start, sustained thermal,
//! realistic geometry complexity, CI regression coverage, or memory.
//! See `BASELINE.md` §6.3 footnote (landed in D2).
//!
//! **Rendering strategy**: option (a) — all 1000 cubes' geometry
//! baked into a single VertexBuffer + IndexBuffer in world space,
//! 1 `draw_indexed` call per frame. Chosen because `LitMeshPipeline`
//! supports neither instance buffers (vertex layout uses
//! `VertexStepMode::Vertex` only) nor per-draw transforms (the WGSL
//! treats vertex `position` as world-space, transformed by the
//! camera UBO's `view_proj`). Option (a) requires zero non-test
//! `crates/gfx/src/` edits.

use std::time::Instant;

use rge_gfx::{
    record_lit_mesh_pass, Camera, DirectionalLight, GfxContext, HeadlessTarget, LitMesh,
    LitMeshPipeline, Material, VertexLit,
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
            eprintln!("SKIP (no GPU adapter): Gate A simple-scene 60fps test skipped");
            None
        }
    }
}

/// Append one 1×1×1 cube centred on `origin` to the running VB/IB. 24
/// vertices (4 per face, per-face normals split, CCW from outside matching
/// `LitMeshPipeline::front_face: Ccw`) + 36 indices (12 triangles).
fn push_cube(vertices: &mut Vec<VertexLit>, indices: &mut Vec<u32>, origin: [f32; 3]) {
    let [cx, cy, cz] = origin;
    let p = |dx: f32, dy: f32, dz: f32| [cx + dx, cy + dy, cz + dz];
    // 8 corners — naming `c[xyz]` where each axis letter is `n`/`p` for ±0.5.
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
    // Per face: (4 corner indices CCW from outside, outward normal).
    // Order matches `render_mesh_smoke.rs` canonical face emission:
    // NegZ → PosZ → NegY → PosY → NegX → PosX.
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

/// Pre-baked GPU resources for the simple scene — one allocation per kind.
struct SimpleScene {
    target: HeadlessTarget,
    pipeline: LitMeshPipeline,
    camera: Camera,
    light: DirectionalLight,
    material: Material,
    mesh: LitMesh,
}

fn build_simple_scene(ctx: &GfxContext) -> SimpleScene {
    let target = HeadlessTarget::new(ctx, VIEWPORT_W, VIEWPORT_H).expect("headless target");

    // Camera: perspective looking down +Z from CAMERA_Z toward grid centre.
    // Grid spans coords -9..+9 on each axis (10 cells × 2-unit spacing).
    let camera = Camera::new(ctx).expect("camera");
    let aspect = VIEWPORT_W as f32 / VIEWPORT_H as f32;
    let proj = glam::Mat4::perspective_rh(60.0_f32.to_radians(), aspect, 0.1, 200.0);
    let view = glam::Mat4::look_at_rh(
        glam::Vec3::new(0.0, 0.0, CAMERA_Z),
        glam::Vec3::ZERO,
        glam::Vec3::Y,
    );
    camera.update(ctx, proj * view, glam::Mat4::IDENTITY);

    // DirectionalLight pointing +Z so the camera-facing -Z face of each cube
    // is lit; lambert max applies.
    let light = DirectionalLight::new(ctx).expect("light");
    light.update(ctx, glam::Vec3::new(0.0, 0.0, 1.0), glam::Vec3::ONE);

    // Single shared material: 4×4 white texture (matches existing tests).
    let white_4x4: Vec<u8> = vec![255u8; 4 * 4 * 4];
    let material = Material::new(ctx, &white_4x4, 4, 4).expect("material");

    // Single PSO via context-owned cache.
    let pipeline = LitMeshPipeline::new(
        ctx,
        camera.bind_group_layout(),
        light.bind_group_layout(),
        material.bind_group_layout(),
        target.format(),
    )
    .expect("pipeline");

    // Bake all 1000 cubes into one VB/IB: 24 verts × 1000 + 36 idx × 1000.
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

    SimpleScene {
        target,
        pipeline,
        camera,
        light,
        material,
        mesh,
    }
}

/// Encode + submit one frame; block until GPU work completes so the wall-clock
/// timer captures the full frame, not just CPU encode time.
fn render_one_frame(ctx: &GfxContext, scene: &SimpleScene) {
    let mut encoder = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GateAFrame"),
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
        None,
    );
    ctx.queue().submit(std::iter::once(encoder.finish()));
    let _ = ctx.device().poll(wgpu::PollType::wait_indefinitely());
}

#[test]
#[ignore = "Phase 6 §6.3 Gate A — release-only, GPU-dependent perf gate; invoke with: cargo test -p rge-gfx --release --test gate_a_simple_scene_60fps -- --ignored --nocapture"]
fn gate_a_simple_scene_60fps() {
    let Some(ctx) = ctx_or_skip() else { return };

    let info = ctx.adapter_info();
    eprintln!(
        "Gate A adapter: name={:?} backend={:?} device_type={:?} driver={:?}",
        info.name, info.backend, info.device_type, info.driver
    );
    eprintln!(
        "Gate A scene: {} cubes ({}x{}x{} grid, spacing={}), viewport {}x{}, \
         camera Z={}, warmup={}, sample={}, runs={}",
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

    let scene = build_simple_scene(&ctx);

    let mut run_p50_ms: Vec<f64> = Vec::with_capacity(RUNS);
    let mut run_p95_ms: Vec<f64> = Vec::with_capacity(RUNS);
    let mut run_max_ms: Vec<f64> = Vec::with_capacity(RUNS);

    for run in 0..RUNS {
        for _ in 0..WARMUP_FRAMES {
            render_one_frame(&ctx, &scene);
        }
        let mut frame_ms: Vec<f64> = Vec::with_capacity(SAMPLE_FRAMES);
        for _ in 0..SAMPLE_FRAMES {
            let start = Instant::now();
            render_one_frame(&ctx, &scene);
            frame_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        frame_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = frame_ms[SAMPLE_FRAMES / 2];
        let p95 = frame_ms[(SAMPLE_FRAMES * 95) / 100];
        let max = *frame_ms.last().unwrap();
        eprintln!("Gate A run {run}: P50={p50:.3} ms, P95={p95:.3} ms, max={max:.3} ms");
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
        "Gate A (simple-scene 60fps): median P50 = {:.3} ms, min P95 = {min_p95:.3} ms, \
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
