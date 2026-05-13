# RGE — Performance Baselines

> **Purpose:** Per-wave perf baselines for the metrics that gate `IMPLEMENTATION.md`'s
> "abort condition" thresholds. Each section is appended by the wave that owns the
> measurement; trend tracking is part of the §1.10.4 metrics review at every minor
> version bump.

---

## W03 — PIE snapshot/restore (Phase 5 abort gate)

**Threshold (per `IMPLEMENTATION.md` Phase 5):** if PIE snapshot+restore exceeds
**500ms on a 10k-entity scene**, ECS storage layout needs redesign.

**Harness:** `crates/editor-shell/tests/timing_baseline.rs` — runs
`measure_round_trip` 4× (1 warmup + 3 timed) and reports `min(total)`.

**Run mode:** `cargo test -p rge-editor-shell --release --test timing_baseline -- --nocapture`

**Workload:** entities each carry one `TickCounter` (8 bytes) + one `Position`
(12 bytes); deterministic `BTreeMap`-backed stub `World` (per `world.rs`).

### 2026-05-05 — initial baseline (W03 stub ECS)

| Entities | Serialized bytes | Capture | Restore | Total | Threshold breached |
|---:|---:|---:|---:|---:|---:|
|     100 |     6,048 |  14.1µs |  33.7µs |  47.8µs | no |
|   1,000 |    60,048 |  77.7µs |  92.5µs | 170.2µs | no |
|  10,000 |   600,048 | 1.897ms | 1.955ms | 3.852ms | no |

**Status:** PASS. 10k-entity round-trip is **3.85ms vs 500ms threshold** —
~130× headroom. Phase 5 abort condition not engaged.

### Notes / caveats

- `world.rs` is a v0 stub; real `kernel/ecs::World` (W02) is archetype-based
  and may have different scaling. Re-run after W02 lands to update the table
  in place (do **not** delete this row — keeps the trend visible).
- Capture/restore approximately equal because both go through a single
  `World::clone` (clone-on-capture, clone-on-restore). Real ECS may diverge
  if structural sharing is added.
- Hardware: per `change.log`'s W03 run on Windows 11 / x86_64; release profile
  uses workspace defaults (opt-level 3, lto thin, codegen-units 1).

---

## Phase 5.3 — kernel/ecs PIE round-trip (re-baseline post-migration)

**Threshold (per `IMPLEMENTATION.md` Phase 5):** if PIE snapshot+restore exceeds
**500ms on a 10k-entity scene**, ECS storage layout needs redesign.

**Harness:** `crates/editor-shell/tests/timing_baseline.rs` — same harness as
W03, now driven by `rge_kernel_ecs::World` + 2 `SnapshotComponent`s (Position + `TickCounter`).

**Run mode:** `cargo test -p rge-editor-shell --release --test timing_baseline -- --nocapture`

### 2026-05-06 — re-baseline post Phase 5.3 (real kernel/ecs::World, snapshot v1 = RON payloads)

| Entities | Serialized bytes | Capture | Restore | Total | Threshold breached |
|---:|---:|---:|---:|---:|---:|
|     100 |     11,370 |  50.7µs |  78.9µs | 129.6µs | no |
|   1,000 |    116,570 | 514.3µs | 798.4µs |   1.3ms | no |
|  10,000 |  1,195,570 |   5.3ms |   8.3ms |  13.6ms | no |

**Status:** PASS — 10k-entity round-trip is **13.6ms vs 500ms threshold** —
~36× headroom. Phase 5 abort condition not engaged.

### 2026-05-05 — snapshot v2 (postcard payloads, format VERSION bump 1 → 2)

| Entities | Serialized bytes | Capture | Restore | Total | Threshold breached |
|---:|---:|---:|---:|---:|---:|
|     100 |     10,210 |  22.9µs |  22.0µs |  44.9µs | no |
|   1,000 |    102,882 | 257.0µs | 215.4µs | 472.4µs | no |
|  10,000 |  1,029,882 |   2.8ms |   2.6ms |   5.3ms | no |

**Status:** PASS — 10k-entity round-trip is **5.3ms vs 500ms threshold** —
~94× headroom. Phase 5 abort condition not engaged.

### Comparison: v1 (RON) vs v2 (postcard)

| Entities | v1 bytes | v2 bytes | size delta | v1 total | v2 total | speedup |
|---:|---:|---:|---:|---:|---:|---:|
|   100 |    11,370 |    10,210 | -10.2% | 129.6µs | 44.9µs  | 2.89× |
|   1k  |   116,570 |   102,882 | -11.7% |   1.3ms | 472.4µs | 2.75× |
|  10k  | 1,195,570 | 1,029,882 | -13.9% |  13.6ms |   5.3ms | 2.55× |

Size reduction is modest (~10–14%) because the snapshot framing — entity ULIDs, component
type names (`snapshot_round_trip::Position` etc.), and length prefixes — dominates the
per-component payload bytes. The wall-time speedup (~2.5–2.9×) reflects postcard's faster
encode/decode path vs RON's text parsing on the small payloads we have here. The original
hesitation to adopt postcard ("non-deterministic without explicit key ordering") was
unfounded for our case: postcard serializes structs in declaration order, and the snapshot
framing already sorts entities by ULID and component types by `snapshot_name()`, so v2
output is byte-identical across runs. (Verified by `serialize_restore_serialize_byte_identical`
test in `kernel/ecs/tests/snapshot_round_trip.rs`.)

### Comparison vs W03 stub baseline (v2 numbers)

| Entities | W03 stub (BTreeMap blob) | Phase 5.3 v2 (kernel/ecs + postcard) | delta |
|---:|---:|---:|---:|
|   100 |  47.8µs  |  44.9µs | -6%   |
|  1k   | 170.2µs  | 472.4µs | +2.8× |
|  10k  |  3.852ms |   5.3ms | +1.4× |

The stub used a flat `BTreeMap<EntityId, Vec<u8>>` with raw byte blobs (zero serde cost);
real kernel/ecs adds archetype iteration + postcard encoding. With v2, 10k overhead vs
the stub floor shrinks to 1.4× (was 3.5× under v1). Abort gate is informational here —
correctness matters, not the absolute comparison.

### Notes / caveats

- v2 wire format: postcard per-component payloads, custom binary framing (RGES magic +
  LE integers + `VERSION = 2`). Entity iteration sorted by ULID `u128`; component type
  iteration sorted by `snapshot_name()` string. v1 (RON) snapshots are not readable by v2
  — bump-only migration; no on-disk persistence existed at the time of the bump.
- The kernel/ecs snapshot test (`kernel/ecs/tests/snapshot_round_trip.rs` test 6) reports
  6.85ms for 10k entities under v2 (was 14.5ms under v1). Single-shot measurement, not
  the min-of-3 used by the editor-shell harness above.
- Archetype iteration determinism: the single catch-all archetype means entity row order
  depends on spawn/despawn history; snapshot sorts by EntityId before iterating, ensuring
  byte-identical output regardless of insertion order.
- Hardware: Windows 11 / x86_64 / release profile (opt-level 3, lto thin, codegen-units 1).

---

## Phase 3.2 — script-host module swap (Phase 3 hot-reload abort gate)

**Threshold (per `IMPLEMENTATION.md` Phase 3 + §5.6):**
- Hot-reload swap p95 **< 100ms** (gate)
- Cold-start (Module compile + first instantiate) **< 50ms** (PLAN §5.6 budget)
- Hard abort: hot-reload p95 **> 500ms** triggers ADR-077 review

**Harness:** `crates/script-host/tests/swap_smoke.rs` — measures the swap
window (capture state → drop old instance → instantiate v2 module → restore
state) on a 1-entity Counter scene with two WAT fixtures (`counter_v1.wat`
increments by 1; `counter_v2.wat` increments by 2).

`crates/script-host/tests/cold_start_smoke.rs` — measures Module compile +
fresh instantiate latency on a hello-world module.

**Run mode:** `cargo test -p rge-script-host` (debug build).

### 2026-05-05 — initial baseline (single-iteration, debug, 1-entity scene)

| Measurement | Value | Threshold | Result |
|---|---|---|---|
| Module swap window (capture → drop → compile → instantiate → restore) | **0.31 ms** | <100 ms p95 | ~320× headroom |
| Cold-start (Module compile + Instance new on hello-world) | **9.1 ms** | <50 ms | ~5× headroom |

**Status:** Constitutional hot-reload bet **validated** at the substrate level.
The swap mechanism (state capture via RON over Counter + wasmtime instance
re-instantiation + state restore) clears the abort gate by two orders of
magnitude.

### Deferred to formal Phase 3.3/3.4 dispatch

The numbers above are single-iteration debug-mode smoke tests on a 1-entity
scene. The full Phase-3 exit criteria (per `IMPLEMENTATION.md`) require:

| Gate | Status |
|---|---|
| Hot-reload p95 < 100ms on a **1000-entity scene** | not yet measured |
| ECS iteration via WASM ≤ **1.5×** native Rust | not yet measured |
| **1-hour** session without memory leak | not yet measured |
| Component data preserved across **100 hot-reload cycles** | only 1 cycle smoke-tested |

The criterion benchmarks in `crates/script-bench/benches/{cold_start,hot_reload_swap,memory_overhead,script_tick_1m}.rs`
are scaffolded but currently driven by a stub engine; they need re-wiring
against `rge-script-host` + a 1000-entity Counter fixture before the formal
p95 gate can be measured. Tracked as Phase 3.3+3.4 follow-up dispatch.

### Notes / caveats

- ECS bridge is hard-coded for `Counter(i64)` — generic component bridge
  (WIT-typed, reflection-driven over `kernel/types`) is Phase 4-Foundation.
- Swap state capture uses direct `ron::to_string` on a hand-shaped
  `CounterSnapshot`, not the generalized `kernel/types` reflect-roundtrip
  pathway. Real-scene swap latency depends on the reflection cost; pending
  the generic bridge, the 0.31ms above is a lower bound.
- Wasmtime version: 44 (per workspace.dependencies). `unsafe_code = "deny"`
  override at the script-host crate root (3 sites with `// SAFETY:` proofs)
  for the wasmtime call-scope pointer pattern; mirror of the pak-format
  precedent for `mmap`.

---

## §13.2 Editor frame idle (Phase 6 §6.3 Gate B)

| Date | Hardware | Methodology | Scope | P50 | P95 | Variance | Gate (≤ 8 ms) |
|---|---|---|---|---|---|---|---|
| 2026-05-11 | dev box (Windows / cargo 1.94 / wasmtime 44) | batch N=1000 × K=10 | **empty-shell CPU-idle baseline** | 0.000044 ms | 0.000047 ms | 9.7% | PASS |

**Methodology**: batch timing around `EditorShell::tick_redraw()` calls
to clear Windows `Instant` resolution floor (~100 ns per call). K=10
batches × N=1000 frames each. P50/P95 computed across the 10
per-frame batch means. Variance gate applies across batch means.

**Scope limitation (LOAD-BEARING)**: This is the CURRENT empty-shell
CPU-idle baseline — `EditorShell::new()` with no `cad_world`, no
projection, no scene, no GPU, no winit event loop. It is NOT a
loaded-editor idle measurement. **Future re-measure required** once
non-trivial editor systems / idle scene are wired (driven by future
Phase 6 dispatches), at which point the same harness shape can be
re-run against the loaded shell.

**Gate B status**: CLOSED for current CPU-idle interpretation
(P95 = 0.000047 ms, ~170 000× under 8 ms gate). Re-measure required
for loaded-editor interpretation.

**Harness**: `crates/editor-shell/tests/editor_frame_idle.rs` (annotated
`#[ignore]` — release-only timing test; debug build trips variance gate).
Invoke via:

```
cargo test -p rge-editor-shell --release --test editor_frame_idle -- --ignored --nocapture
```

---

## §6.3 Gate A — 60fps simple-scene golden (1k cubes, 1 directional light)

| Date | Adapter | Backend | Methodology | Scope | P50 | min-P95 | median P95 | max P95 | Worst frame | Variance | Gate (≤ 16.67 ms) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 2026-05-11 | NVIDIA GeForce RTX 4060 Ti (DiscreteGpu, NVIDIA driver) | Vulkan | 600 frames after 60-frame warmup; 3 runs, min-of-3 reported | 1280×720, static camera, release mode | 0.085 ms | **0.112 ms** | 0.116 ms | 0.117 ms | 1.803 ms | 4.9% | **PASS** |

**Methodology**: release-mode headless wgpu render-loop. 1000 axis-aligned cubes baked into a single `VertexBuffer` + `IndexBuffer` (option-(a) single-draw-call strategy — `LitMeshPipeline` has no instance-attribute or per-draw-transform support and the D1 dispatch forbade non-test `crates/gfx/src/**` edits). Single `DirectionalLight`; static camera at Z=-40; 1280×720 viewport; shared PSO + 1 material across all 1000 cubes; one `draw_indexed` call per frame. 600 sampled frames after a 60-frame warmup. 3 runs; min-of-3 P95 reported. Variance gate applies across the 3 runs' P95 values (threshold ≤ 30%).

**Scope limitation (LOAD-BEARING)**: This Gate A closure is **CONSTRAINED-CERTIFIED on the recorder host only**. It does NOT certify:

- universal 60fps across hardware classes
- vendor parity (NVIDIA vs AMD vs Intel; Vulkan vs DX12 vs Metal vs WebGPU)
- cold-start frame cost (the 60-frame warmup explicitly discards it)
- sustained thermal behavior (3 runs × 600 frames is too short)
- realistic geometry complexity (1000 axis-aligned cubes sharing 1 PSO is fragment-light, vertex-light, draw-call-medium)
- CI regression coverage (release-only `#[ignore]` test — PR-time regressions surface only on the next manual recorder invocation)
- memory or VRAM footprint (orthogonal PLAN §13.2 350 MB simple-scene gate, not measured here)

**Gate A status**: **CLOSED** on recorder host only (min-of-3 P95 = 0.112 ms, ~150× under the 16.67 ms gate). Re-measure required for any new recorder host / adapter / backend / viewport / camera path.

**Harness**: `crates/gfx/tests/gate_a_simple_scene_60fps.rs` (annotated `#[ignore]` — release-only timing test). Invoke via:

```
cargo test -p rge-gfx --release --test gate_a_simple_scene_60fps -- --ignored --nocapture
```

**Sequencing note**: Gate B (CPU-idle empty-shell baseline) closed earlier 2026-05-11; Gate A (this entry) closes for current recorder constraints; **Gate C (render-thread sees stable snapshot; sim-thread mutations don't race) remains DEFERRED** — blocked on the sim/render thread split landing per PLAN §1.5.2 (today's substrate is single-threaded, so the property is vacuously true and the gate is structurally unmeasurable until the split exists).

**Post-depth Gate A — CLOSED 2026-05-14 (MAIN-RENDER-POSTDEPTH-GATEA-001 dispatch, gfx-level synthetic harness)**: The "depth-attached gfx-level harness" option (a) listed in the prior `Post-sub-β measurement gap` note landed as `crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs` — an additive, release-only, `#[ignore]` integration test that mirrors the pre-depth Gate A methodology byte-for-byte (1000 cubes / 10×10×10 / 1280×720 / 60 warmup + 600 sample / 3 runs / P95 ≤ 16.67 ms / variance ≤ 30%) but constructs the pipeline via `LitMeshPipeline::new_with_depth(.., Some(DepthStateKey { Depth24Plus, depth_write_enabled: false, LessEqual }))` (sub-α API) and passes `Some(&depth_view)` to `record_lit_mesh_pass(...)` (per-frame `Depth24Plus` depth texture allocated once and reused). Zero non-test `crates/gfx/src/` edits; the existing `record_lit_mesh_pass` already supports the `Option<&wgpu::TextureView>` arg. Recorder-host run on **NVIDIA GeForce RTX 4060 Ti / Vulkan / DiscreteGpu**: run 0 P95 = 0.125 ms, run 1 P95 = 0.122 ms, run 2 P95 = 0.122 ms → **min-of-3 P95 = 0.122 ms** (median P95 = 0.122 ms, max P95 = 0.125 ms, worst frame = 1.996 ms, **variance across runs = 2.6%**). About 9% slower than pre-depth (0.122 ms vs 0.112 ms) — the measured cost of the depth attachment — and still ~137× under the 16.67 ms gate. **The 0.112 ms pre-depth claim above remains valid for the pre-depth gfx path; this post-depth claim is the additional valid measurement for the depth-attached gfx path.** **Scope (recorder-host-only)**: NOT universal, NOT vendor parity, NOT cold-start, NOT sustained thermal, NOT realistic geometry complexity, NOT CI regression coverage, NOT editor-shell `render_frame` end-to-end (the harness exercises the gfx-level primitives that editor-shell production consumes post-sub-β; it does not exercise editor-shell's winit + `SurfaceContext` + `FrameGraph` + `build_resource_map` substrate ceremony — that remains a separate non-winit-perf-harness scope, blocked on `EditorShell::render_frame` accepting a mock event loop, not pursued by this dispatch). **What's still deferred**: option (b) non-winit editor-shell perf harness (unchanged scope; pressure-driven future dispatch); option (c) manual user report (unchanged; orthogonal to harness-level proof). **No new architecture, no production-source edits, no PLAN target retargeting in this dispatch.**
