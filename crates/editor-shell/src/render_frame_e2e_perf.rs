//! POSTV0-EDITOR-SHELL-PERF-HARNESS-001 — editor-shell encode/submit
//! perf harness, minus winit surface acquire/present.
//!
//! Crate-local `#[cfg(test)]` module gated by
//! `crates/editor-shell/src/lib.rs`. Kept inside `src/` rather than
//! under `tests/` so the helpers reached via [`EditorShell`] stay
//! `pub(crate)` — no public perf-only API surface.
//!
//! # Scope (what this DOES measure)
//!
//! Per-frame editor-shell substrate cost the production
//! [`crate::lifecycle::EditorShell::render_frame`] body performs AFTER
//! the winit-bound surface acquire and BEFORE present:
//! [`crate::lifecycle::EditorShell::acquire_depth_view`] (frame-graph
//! pool `begin_frame` + `build_resource_map`) plus
//! [`crate::lifecycle::EditorShell::render_frame_to_target`] (encoder
//! create + single render pass record with depth + bind-group binds +
//! `draw_indexed` + queue submit) against an offscreen
//! `wgpu::TextureFormat::Bgra8UnormSrgb` color target sized 1024×768
//! single-cuboid scene.
//!
//! # Scope (what this does NOT certify)
//!
//! - winit event-loop scheduling / `ActiveEventLoop` dispatch.
//! - surface acquire (`surface.get_current_texture()` latency).
//! - present / vsync / compositor handoff.
//! - universal hardware — single recorder host only like Gate A
//!   (re-measurement required for any new recorder host / adapter /
//!   backend / target size).
//! - cold-start timing.
//! - sustained thermal behavior.
//! - loaded-scene complexity beyond the current single-cuboid render
//!   path.
//! - CI regression coverage (`#[ignore]`-gated, release-only).
//!
//! # Measurement contract
//!
//! - 3 runs.
//! - Each run: **240 warmup frames** followed by **600 sample batches
//!   × 50 frames per batch**. Each batch is timed end-to-end with one
//!   `Instant::elapsed`; the stored sample is the per-frame batch mean
//!   `batch_elapsed_ms / FRAMES_PER_SAMPLE`. Per-batch timing puts the
//!   measured unit well above the Windows scheduler / `Instant`
//!   resolution noise floor; the 240-frame warmup absorbs the
//!   cold-binary first-run tail (page cache / code TLB / branch
//!   predictor / GPU command pool). The `(60, 10)` shape that landed
//!   in the first correction round straddled the noise floor — the
//!   Reviewer observed 54.6 % variance on a cold-binary invocation
//!   followed by 13.8 % on a hot rerun (correction packet 2026-05-14
//!   20:58:30); the `(240, 50)` shape lifts both axes off that floor.
//! - Per run: report P50, P95, min, max, worst-sample (worst batch
//!   mean, in ms-per-frame).
//! - Across the 3 run P95s: report median, min, max, variance%.
//! - Variance gate: `(max P95 − min P95) / median P95 ≤ 30%`. Asserted
//!   as a hard gate.
//! - Soft P95 target: 1.0 ms — REPORTED (under/over) only. NOT asserted
//!   as a hard threshold in this measurement-capture dispatch; the
//!   POSTV0-EDITOR-SHELL-PERF-HARNESS-001 EXEC packet recommends the
//!   future hard threshold from the observed values.
//!
//! Recorder-host invocation (canonical):
//!
//! ```text
//! cargo test -p rge-editor-shell --release render_frame_e2e_perf \
//!   -- --ignored --nocapture
//! ```

use std::time::Instant;

use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::World;

use crate::lifecycle::EditorShell;
use crate::render_path::DepthViewOutcome;

/// Offscreen color target format. Chosen to match the canonical
/// `SurfaceContext` color format on the recorder host so the
/// `LitMeshPipeline` compile matches the production PSO closely.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
/// Offscreen color target width in pixels. Matches the production
/// `WindowAttributes::default().with_inner_size(LogicalSize::new(1024, 768))`
/// width used by `init_render_state`'s Step 1.
const TARGET_WIDTH: u32 = 1024;
/// Offscreen color target height in pixels. Matches the production
/// `WindowAttributes::default().with_inner_size(LogicalSize::new(1024, 768))`
/// height used by `init_render_state`'s Step 1.
const TARGET_HEIGHT: u32 = 768;
/// Warmup frames per run. Bumped from 60 to 240 per the
/// 2026-05-14 20:58:30 correction packet — Reviewer's independent
/// cold-binary first run failed the 30 % variance gate at 54.6 %
/// with the prior `(60, 10)` shape, even though subsequent hot
/// reruns passed at 13.8 %. The extra warmup absorbs page-cache /
/// code-TLB / branch-predictor / GPU-command-pool cold tails that
/// affect the first run after a fresh binary load.
const WARMUP_FRAMES: usize = 240;
/// Number of timing samples per run. Each sample is a batch of
/// `FRAMES_PER_SAMPLE` consecutive frames timed under one
/// `Instant::elapsed` window; the stored value is the per-frame batch
/// mean. Total sampled frames per run = `SAMPLE_BATCHES * FRAMES_PER_SAMPLE`.
const SAMPLE_BATCHES: usize = 600;
/// Frames per timing batch. Bumped from 10 to 50 per the
/// 2026-05-14 20:58:30 correction packet — the prior `(60 warmup, 10
/// frames/batch)` shape clipped the Windows scheduler noise floor on
/// independent cold-binary runs (Reviewer observed 54.6 % variance on
/// the first invocation followed by 13.8 % on a hot rerun). At
/// ~0.02 ms per frame × 50 = ~1.0 ms per batch, comfortably above
/// the ~0.1 ms Windows timer noise floor and large enough that
/// scheduler preemption is amortised across the batch.
const FRAMES_PER_SAMPLE: usize = 50;
const RUN_COUNT: usize = 3;
/// Hard variance gate across the 3 run P95s. Same shape as Gate A
/// (`plans/BASELINE.md:240`) and Gate B
/// (`crates/editor-shell/tests/editor_frame_idle.rs:48-51`).
const VARIANCE_GATE_PCT: f64 = 30.0;
/// Soft P95 target for this measurement-capture run. Reported but
/// NOT asserted as a hard gate — the EXEC packet recommends a future
/// hard threshold once the observed value is captured.
const SOFT_P95_TARGET_MS: f64 = 1.0;

/// Build a single-cuboid `(CadGraph, CadProjection, World)` triple
/// matching the existing camera-picker smoke / face-picking smoke
/// idiom from `crates/cad-projection/tests/face_picking_smoke.rs`.
/// Uses a 1×1×1 origin-centered cuboid (the simplest valid scene the
/// production render path accepts).
fn build_unit_cuboid_world() -> (CadGraph, CadProjection, World) {
    let mut graph = CadGraph::new();
    graph
        .begin_operation()
        .expect("CadGraph::begin_operation: no in-progress op pre-seed");
    let cuboid_node = graph
        .graph_mut()
        .expect("CadGraph::graph_mut: in-progress op was just begun")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("OperatorGraph::add_operator: 1×1×1 cuboid is content-derived NodeId-unique");
    graph
        .graph_mut()
        .expect("CadGraph::graph_mut: in-progress op still active")
        .set_root(cuboid_node)
        .expect("OperatorGraph::set_root: cuboid_node is the only root candidate");
    graph
        .commit("perf-harness-seed-cuboid")
        .expect("CadGraph::commit: in-progress op has a root and a valid snapshot");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let _entity = projection
        .spawn_brep_entity(&mut world, cuboid_node)
        .expect("CadProjection::spawn_brep_entity: cuboid_node exists at the just-committed head");
    let tolerance = Tolerance::new(0.001).expect("Tolerance::new(0.001): finite positive");
    projection
        .tick(&mut world, &graph, tolerance)
        .expect("CadProjection::tick: graph head valid and entity registered");

    (graph, projection, world)
}

fn percentile(sorted_ms: &[f64], pct: usize) -> f64 {
    let idx = (sorted_ms.len() * pct) / 100;
    sorted_ms[idx.min(sorted_ms.len() - 1)]
}

/// Drive one `acquire_depth_view` + `render_frame_to_target` pair
/// against the offscreen color view. Panics on any uninitialised
/// state because `init_render_state_headless` populated everything
/// just before the measurement loop began; the panic is a measurement
/// integrity guard, not a recoverable runtime path.
fn tick_one_frame(shell: &mut EditorShell, color_view: &wgpu::TextureView) {
    let depth_view = match shell.acquire_depth_view() {
        DepthViewOutcome::Acquired(view) => view,
        DepthViewOutcome::RecoverableSkip => {
            // Substrate hiccup mid-measurement is a recoverable production
            // skip in production but a measurement integrity violation
            // here — surface to the test runner.
            panic!(
                "acquire_depth_view returned RecoverableSkip mid-measurement; \
                 build_resource_map failed inside an offscreen target loop"
            );
        }
        DepthViewOutcome::Uninitialized => {
            panic!(
                "acquire_depth_view returned Uninitialized after \
                 init_render_state_headless populated gfx_ctx / pools / compiled_frame_graph"
            );
        }
    };
    let rendered = shell.render_frame_to_target(color_view, &depth_view);
    assert!(
        rendered,
        "render_frame_to_target returned false after \
         init_render_state_headless populated pipeline / camera / light / material / mesh"
    );
}

#[test]
#[ignore = "release-only timing harness — invoke via `cargo test -p rge-editor-shell --release render_frame_e2e_perf -- --ignored --nocapture`; debug builds produce >30% variance and falsely trip the variance gate"]
fn render_frame_e2e_p95_minus_surface_acquire_present_recorder_host() {
    // Build single-cuboid scene and editor shell.
    let (graph, projection, world) = build_unit_cuboid_world();
    let mut shell = EditorShell::with_world_projection_graph(world, projection, graph);

    // Initialise render state headlessly: GfxContext::new_headless()
    // + the shared `init_render_state_post_surface` helper. No winit
    // window, no winit-bound SurfaceContext.
    shell
        .init_render_state_headless(TARGET_FORMAT, TARGET_WIDTH, TARGET_HEIGHT)
        .expect("init_render_state_headless on recorder host");

    // Allocate the offscreen color target once and reuse across all
    // frames + runs. The harness measures the per-frame encode/submit
    // cost; pool/begin_frame cycle is already inside `acquire_depth_view`
    // so the color target stays static.
    let color_target = {
        let gfx_ctx = shell
            .gfx_ctx
            .as_ref()
            .expect("init_render_state_headless populates gfx_ctx");
        gfx_ctx.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("rge-editor-shell.perf-harness.color-target"),
            size: wgpu::Extent3d {
                width: TARGET_WIDTH,
                height: TARGET_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    };
    let color_view = color_target.create_view(&wgpu::TextureViewDescriptor::default());

    // Per-run aggregates.
    let mut run_p50s_ms: Vec<f64> = Vec::with_capacity(RUN_COUNT);
    let mut run_p95s_ms: Vec<f64> = Vec::with_capacity(RUN_COUNT);
    let mut run_mins_ms: Vec<f64> = Vec::with_capacity(RUN_COUNT);
    let mut run_maxes_ms: Vec<f64> = Vec::with_capacity(RUN_COUNT);
    let mut run_worsts_ms: Vec<f64> = Vec::with_capacity(RUN_COUNT);

    eprintln!(
        "POSTV0-EDITOR-SHELL-PERF-HARNESS-001 — encode/submit minus surface acquire/present \
         (recorder-host-only, single-cuboid, {TARGET_WIDTH}x{TARGET_HEIGHT}, {TARGET_FORMAT:?}, \
         {WARMUP_FRAMES} warmup + {SAMPLE_BATCHES} sample batches x {FRAMES_PER_SAMPLE} frames x \
         {RUN_COUNT} runs; each sample = per-frame batch mean)"
    );

    for run_idx in 0..RUN_COUNT {
        // Warmup — clears caches/pools without contributing to stats.
        for _ in 0..WARMUP_FRAMES {
            tick_one_frame(&mut shell, &color_view);
        }

        // Sample. Each sample is the per-frame batch mean across
        // `FRAMES_PER_SAMPLE` consecutive frames timed under one
        // `Instant::elapsed` window. Batching the timer puts the
        // measured unit above the Windows scheduler / `Instant`
        // resolution noise floor that destabilised single-frame
        // timing on back-to-back reviewer invocations (correction
        // packet 2026-05-14 19:33:13).
        let mut sample_means_ms: Vec<f64> = Vec::with_capacity(SAMPLE_BATCHES);
        for _ in 0..SAMPLE_BATCHES {
            let start = Instant::now();
            for _ in 0..FRAMES_PER_SAMPLE {
                tick_one_frame(&mut shell, &color_view);
            }
            let batch_total_ms = start.elapsed().as_secs_f64() * 1000.0;
            sample_means_ms.push(batch_total_ms / FRAMES_PER_SAMPLE as f64);
        }

        // Per-run stats. Units are ms-per-frame (each sample is
        // already a per-frame batch mean), so percentile / min / max
        // arithmetic on `sample_means_ms` is comparable across runs.
        let mut sorted = sample_means_ms.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("sample-mean NaN"));
        let run_p50 = percentile(&sorted, 50);
        let run_p95 = percentile(&sorted, 95);
        let run_min = sorted[0];
        let run_max = *sorted.last().expect("SAMPLE_BATCHES > 0");
        let run_worst = run_max;

        for &v in &[run_p50, run_p95, run_min, run_max, run_worst] {
            assert!(
                v.is_finite() && v >= 0.0,
                "non-finite or negative measurement: {v}"
            );
        }

        eprintln!(
            "  run {run_idx}: P50 = {run_p50:.6} ms, P95 = {run_p95:.6} ms, \
             min = {run_min:.6} ms, max = {run_max:.6} ms, worst-sample = {run_worst:.6} ms"
        );

        run_p50s_ms.push(run_p50);
        run_p95s_ms.push(run_p95);
        run_mins_ms.push(run_min);
        run_maxes_ms.push(run_max);
        run_worsts_ms.push(run_worst);
    }

    // Cross-run aggregates.
    let mut sorted_p95s = run_p95s_ms.clone();
    sorted_p95s.sort_by(|a, b| a.partial_cmp(b).expect("run P95 NaN"));
    let median_p95 = sorted_p95s[RUN_COUNT / 2];
    let min_p95 = sorted_p95s[0];
    let max_p95 = *sorted_p95s.last().expect("RUN_COUNT > 0");
    let variance_pct = if median_p95 > 0.0 {
        (max_p95 - min_p95) / median_p95 * 100.0
    } else {
        0.0
    };

    let median_p50 = {
        let mut s = run_p50s_ms.clone();
        s.sort_by(|a, b| a.partial_cmp(b).expect("run P50 NaN"));
        s[RUN_COUNT / 2]
    };
    let agg_min = run_mins_ms.iter().copied().fold(f64::INFINITY, f64::min);
    let agg_max = run_maxes_ms
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let agg_worst = run_worsts_ms
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    eprintln!(
        "  cross-run: median P50 = {median_p50:.6} ms; median P95 = {median_p95:.6} ms; \
         min P95 = {min_p95:.6} ms; max P95 = {max_p95:.6} ms; \
         worst-sample = {agg_worst:.6} ms; min-sample = {agg_min:.6} ms; \
         max-sample = {agg_max:.6} ms; variance across run P95s = {variance_pct:.1}%"
    );
    eprintln!(
        "  soft P95 target = {SOFT_P95_TARGET_MS:.3} ms; observed median P95 is {} the soft target",
        if median_p95 <= SOFT_P95_TARGET_MS {
            "UNDER"
        } else {
            "OVER"
        }
    );

    for &v in &[
        median_p50, median_p95, min_p95, max_p95, agg_worst, agg_min, agg_max,
    ] {
        assert!(
            v.is_finite() && v >= 0.0,
            "non-finite or negative cross-run aggregate: {v}"
        );
    }
    assert!(
        variance_pct.is_finite() && variance_pct >= 0.0,
        "non-finite or negative variance: {variance_pct}"
    );

    // Hard variance gate. Soft P95 target is REPORTED above, not asserted.
    assert!(
        variance_pct <= VARIANCE_GATE_PCT,
        "variance across 3 run P95s = {variance_pct:.1}% exceeds {VARIANCE_GATE_PCT:.1}% gate; \
         measurement unstable on this host — re-run or escalate, do not tune"
    );
}
