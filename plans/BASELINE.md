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

**2026-05-23 supersession of the "option (b) non-winit editor-shell perf harness" deferral (ISSUE-118; docs-only history reconciliation)**: The clause directly above stating that "option (b) non-winit editor-shell perf harness (unchanged scope; pressure-driven future dispatch)" is "still deferred" is **HISTORICAL ONLY as of the 2026-05-14 paragraph that records it**, and is **SUPERSEDED** by the post-v0 landing recorded here. The non-winit editor-shell `render_frame` perf harness landed **post-v0** at commit `f8b8ed4` as `crates/editor-shell/src/render_frame_e2e_perf.rs` — a release-only `#[ignore]` recorder-host integration test that drives `EditorShell::render_frame` end-to-end without a winit event loop and measures the encode/submit window excluding surface acquire/present. Provenance for existence, path, and commit: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_21-51-40+0300.md`. **v0 release certification at commit `6aaf7f1` is unchanged** — that certification predates `f8b8ed4` and remains the v0 reference; the editor-shell perf harness landed *after* v0 certification and resolves the option (b) deferral without retroactively altering the cert. **No new BASELINE.md measurement row is added** for the editor-shell harness, and **no recorder-host P95 / P50 / worst-sample numbers from the POSTV0 EXEC packet are copied into this doc**. **Hard P95 / worst-sample / variance threshold pinning** for this harness remains a **future, explicitly-authorized certification scope** and is **not chosen here**; the POSTV0 EXEC packet itself documents that threshold pinning was left deferred. ISSUE-118 is documentation reconciliation only — no source / test / bench / Cargo / schema / lint / automation / protocol-doc / `plans/IMPLEMENTATION.md` edits, and no Cargo or perf-harness execution in this dispatch.

---

## §13.3 Compile-time baseline (Phase 9 preflight)

**Budget anchors (per `plans/PLAN.md` §1.10 + `plans/IMPLEMENTATION.md` §6 table at line 689–690):**

- Clean-build budget: **≤ 120 s** (`cargo build --release` from a wiped `target/`)
- Incremental p95 budget: **≤ 10 s** (`cargo build` after a 1-line source change)
- Reflection compile-time gate (Phase 1.1): **> 30 s on 5 pilot types ⇒ STOP**
- Incremental invalidation radius (v0.7, NEW): **> 30 % of workspace rebuilt after touching one core type ⇒ lint warn**

**This entry is a Phase 9 PREFLIGHT — a warm-cache `cargo check` baseline ONLY.** It is explicitly **NOT** a proof that the clean-build or incremental p95 budgets are satisfied, and it does NOT close any §13.3 gate. It establishes the first recorded compile-time reference number for the workspace so future regressions can be detected; the formal clean-build and 1-line-edit incremental measurements are deferred to a future dispatch that owns the target-dir rewarm cost and a dedicated harness script.

**Harness (manual):** PowerShell `[System.Diagnostics.Stopwatch]` around `cargo check` invocations (no `--timings` flag, no on-disk artifacts written outside `target/`). Reproducer:

```
$env:CARGO_HOME='A:\RustCache\cargo'; $env:RUSTUP_HOME='A:\RustCache\rustup'
$env:Path='A:\RustCache\cargo\bin;' + $env:Path
cd A:\RCAD\RGE
$sw = [System.Diagnostics.Stopwatch]::StartNew()
cargo check --workspace --message-format=short
$sw.Stop(); $sw.Elapsed.TotalSeconds
```

For the `--all-targets` variants, append `--all-targets` to the `cargo check` line.

### 2026-05-21 — initial warm-cache `cargo check` baseline (Phase 9 preflight; recorder host)

| Measurement | Command | Elapsed (wall) | Cargo "Finished" | Notes |
|---|---|---:|---:|---|
| Warm, fingerprint-stale full-workspace check | `cargo check --workspace` | **17.65 s** | 17.42 s | Many workspace crates re-checked despite warm cache → fingerprint drift since last build (recent dispatch-publish commits touched source). Worst-of-pair for this preflight. |
| Warm no-op rerun (full workspace, no `--all-targets`) | `cargo check --workspace` (immediate rerun) | **0.93 s** | 0.76 s | Sentinel scan only — cargo overhead floor for this workspace under the warm cache. |
| Warm `--all-targets` first run (adds tests + benches) | `cargo check --workspace --all-targets` | **13.69 s** | 13.40 s | Tests/benches for two crates (`rge-io-3mf`, `rge-kernel-shared`) checked for the first time this session; rest were already up-to-date. |
| Warm `--all-targets` no-op rerun | `cargo check --workspace --all-targets` (immediate rerun) | **1.18 s** | 0.91 s | Sentinel scan only with tests + benches included. |

**Recorder context (for trend tracking):**

| Field | Value |
|---|---|
| Workspace members (Cargo.toml count) | **94 crates** (kernel 15 / crates 65 / tools 8 / runtime 4 / editor 1 + 1 proc-macro at `crates/macros-reflect`) |
| Source files (non-vendor `.rs`, excludes `target/` / `.claude/` / `OLD/` / `third_party/`) | **673** |
| Source LoC (non-vendor `.rs`, same exclusions) | **144,754** (kernel 21,324 / crates 116,806 / runtime 20 / editor 96 / tools 6,508) |
| Largest single crate by `src/` LoC | **`cad-core` = 24,842 LoC** (next: `gfx` 8,950, `editor-ui` 5,779, `editor-shell` 5,256) |
| Rust toolchain | **1.92.0** (pinned via `rust-toolchain.toml`; floor driven by `egui_dock 0.19` MSRV) |
| `CARGO_TARGET_DIR` | **`A:\RustCache\target`** (shared across dispatches; not the workspace-local `target/`) |
| Shared target dir on-disk size | **≈ 385 GB** (~395 GB measured at sample time; warm with all transitive deps from prior dispatches) |
| Host OS | Windows 11 / x86_64 |

**Status:** **PHASE 9 PREFLIGHT — warm-cache only.**

- The four numbers above establish the first recorded compile/check reference for the workspace. They do NOT satisfy or close any §13.3 budget gate.
- **NOT a clean-build measurement**: `target/` was deliberately not wiped (would cost hours of recompile time across the ~385 GB shared cache and would have broken every subsequent dispatch). The 17.65 s number is best read as "warm cache after fingerprint drift from the most recent source touches", not as the §13.3 ≤ 120 s clean-build budget.
- **NOT a 1-line-edit incremental p95 measurement**: this preflight was docs-only by directive — no source touch, no Cargo touch, no lint/ADR/automation touch. The "no-op rerun" floors (0.93 s / 1.18 s) are a lower bound on cargo overhead, not the p95 metric the §13.3 budget targets.
- **`cargo check` not `cargo build`**: §13.3's ≤ 120 s clean / ≤ 10 s incremental budgets are written against `cargo build`. `cargo check` is a strict subset (no codegen / no linking), so a passing `cargo check` time is necessary but not sufficient evidence for the build budget.

**Top 3 compile-time pressure risks identified by this preflight (qualitative; no measurement yet):**

1. **No formal compile-time baseline existed prior to this entry.** Every other Phase 9 compile-time axis is downstream of this row.
2. **Incremental invalidation radius likely already grazing the 30 % lint-warn threshold.** `kernel/graph-foundation::NodeId` is a transitive dep of `cad-core`, `material-graph`, `anim-graph`, `script-graph`, `editor-ui`, `cad-projection`, `gfx`, `kernel/asset`, `kernel/asset-store`, plus all four Tier-2 plugin canaries and all 5 `node_graph_*_smoke.rs` integration tests — roughly 30+ of 94 crates (~32 %). `kernel/types::EntityId` is similar or worse. **Not yet measured empirically; deferred to a follow-up Phase 9 dispatch.**
3. **`cad-core` at 24,842 LoC is the dominant single-crate compile cost.** Already internally split (`topology/` / `operators/` / `topo_lineage/` / `tessellation/` / `checkpoints/` / `graph/`), but fingerprinted as one unit, so any cad-core source edit recompiles the full 25 k LoC plus the csgrs / nalgebra / blake3 link tail. Severity is low–medium today; would matter only when iteration on cad-core becomes the bottleneck (constraint solver, Fillet G2 patches, a second CAD-kernel adapter under ADR-113-deferred).

**Explicit deferrals (next dispatches, in order; NOT executed in this preflight):**

1. **True clean-build measurement** (§13.3 ≤ 120 s gate) — owns the `target/` rewarm cost; should land its own tiny harness (e.g. `tools/compile-timing.ps1`) before wiping the cache.
2. **Incremental invalidation radius measurement** for the highest-fan-out kernel types (`kernel/types::EntityId`, `kernel/graph-foundation::NodeId`, `kernel/graph-foundation::EdgeId`) — pure measurement, no lint added; maps directly to PLAN §1.10.4's 30 % lint-warn threshold.
3. **1-line-edit incremental p95 sample** (§13.3 ≤ 10 s gate) — minimal source touch (e.g. a comment append on a leaf crate) with explicit revert in the same dispatch.

**Notes / caveats:**

- Cargo's "Checking …" lines do not imply work was done; only the "Finished … in N.NNs" line counts. The "wall" column above is the PowerShell-stopwatch wall-clock around the whole `cargo` invocation (includes process startup + stdout drain); the "Cargo `Finished`" column is what cargo itself reports.
- Two warnings were emitted during the runs (`rge-ui-theme` missing-docs, `rge-cad-core revolve_fillet_smoke.rs` unused variable). They are pre-existing and unrelated to this preflight; they did not affect timing meaningfully.
- The shared `CARGO_TARGET_DIR=A:\RustCache\target` setup means individual dispatch sessions inherit a fully warm cache; a fresh-checkout developer on a different machine will see materially different numbers on first build. That asymmetry is exactly why a future clean-build dispatch is non-trivial to schedule.
- Hardware identity is deliberately not pinned in this row beyond "recorder host / Windows / x86_64". A future dispatch that owns the cleaner harness should record the CPU model, NVMe vs SATA on `A:\`, and antivirus posture (NTFS realtime scan is a known cargo-throughput drag on Windows).

---

## §1.10.4 Incremental-invalidation-radius preflight (Phase 9)

**Budget anchor (per `plans/PLAN.md` §1.10.4 / risk-table line 1218):**

> **Incremental invalidation radius** (crates rebuilt after touching one core type) **> 30 % of workspace ⇒ lint warn**.

For the current workspace (95 members per `cargo metadata`), the lint-warn threshold is **28.5 crates** (i.e. a crate whose transitive reverse-dep closure includes ≥ 29 workspace members would trip the warning).

**This entry is a Phase 9 PREFLIGHT — pure read-only `cargo metadata` measurement.** It is NOT a lint, it is NOT a harness wired into CI, and it does NOT touch source / Cargo / lint code. It establishes the first recorded radius reference for the workspace so future regressions are visible, and it codifies the revisit triggers under which a real lint / harness becomes warranted.

**Methodology (read-only):**

1. `cargo metadata --format-version 1 > meta.json` — produces the full resolved dep tree including `dep_kinds` per edge.
2. Parse the JSON: collect `workspace_members` (set of package IDs), then walk `resolve.nodes[].deps[].dep_kinds[]` to build the workspace-internal forward graph in two flavours:
   - **NORMAL** = edges with `dep_kinds.kind = null` (normal lib) ∪ `"build"` (build-deps invalidate too).
   - **NORMAL+DEV** = above ∪ edges with `dep_kinds.kind = "dev"` (counts test/bench rebuilds).
3. Invert each forward graph to a reverse graph, then compute the transitive reverse-dependency closure for every workspace crate (DFS through the reverse adjacency map).
4. The percentage **closure / 95** is the invalidation-radius measurement.

No source files were read; only `Cargo.toml` (via cargo's own resolver) and the resolved metadata JSON. The Python parser is throw-away (lives outside the repo at `C:/Users/halil/AppData/Local/Temp/rge_radius2.py`); reproducer is below.

### 2026-05-21 — initial workspace radius snapshot (recorder host, Rust 1.92.0)

**Workspace context:**

| Field | Value |
|---|---|
| `cargo metadata` workspace members | **95** crates |
| Older `Status.md` / `HANDOFF.md` / `README.md` wording | "94 crates" (one-off doc drift; one extra crate has landed since those rows were last refreshed; not material to threshold analysis — the discrepancy is < 2 %) |
| Workspace-internal edges (normal + build) | **64** |
| Workspace-internal edges (dev) | **13** |
| Distinct workspace-internal edges | **75** |
| **Isolated crates** (zero workspace-internal edges in either direction) | **57 of 95 (60 %)** |
| Examples of isolated crates | `anim-clip`, `anim-ik`, `anim-retarget`, `cad-native`, `cad-occt`, `components-{editor, interaction, lifecycle, networking, physics, spatial}`, `runtime-{web, mobile, headless}`, 7 of 8 `tools/*` (only `architecture-lints` is connected) |

**Top 10 workspace crates by reverse-dep closure (descending):**

| Rank | Crate | Normal closure | % of 95 | Direct (normal) | +Dev closure | +Dev % |
|---:|---|---:|---:|---:|---:|---:|
| 1 | `rge-kernel-graph-foundation` | **18** | **18.9 %** | 9 | 18 | 18.9 % |
| 2 | `rge-kernel-diagnostics` | 15 | 15.8 % | 12 | 15 | 15.8 % |
| 3 | `rge-kernel-ecs` | 10 | 10.5 % | 9 | 10 | 10.5 % |
| 4 | `rge-kernel-asset` | 7 | 7.4 % | 7 | 7 | 7.4 % |
| 5 | `rge-kernel-plugin-host` | 7 | 7.4 % | 5 | 7 | 7.4 % |
| 6 | `rge-cad-core` | 5 | 5.3 % | 4 | 5 | 5.3 % |
| 7 | `rge-brep-render` | 4 | 4.2 % | 3 | 4 | 4.2 % |
| 8 | `rge-editor-state` | 3 | 3.2 % | 2 | 4 | 4.2 % |
| 9 | `rge-material-runtime` | 3 | 3.2 % | 1 | 4 | 4.2 % |
| 10 | `rge-runtime-wasmtime` | 3 | 3.2 % | 2 | 3 | 3.2 % |

**Requested candidates (explicit) — side-by-side normal vs +dev:**

| Crate | Normal closure / % | +Dev closure / % | Direct normal revdeps |
|---|---:|---:|---|
| `rge-kernel-types` | 2 / 2.1 % | 3 / 3.2 % | `rge-macros-reflect`, `rge-script-host` |
| `rge-kernel-graph-foundation` | **18 / 18.9 %** | 18 / 18.9 % | `rge-anim-graph`, `rge-asset-store`, `rge-cad-core`, `rge-cad-projection`, `rge-editor-ui`, `rge-gfx`, `rge-kernel-asset`, `rge-material-graph`, `rge-script-graph` |
| `rge-cad-core` | 5 / 5.3 % | 5 / 5.3 % | `rge-cad-projection`, `rge-editor`, `rge-editor-shell`, `rge-editor-state`, `rge-editor-ui` |
| `rge-macros-reflect` | **0 / 0.0 %** | 0 / 0.0 % | — (only its own internal tests/fixtures use it) |
| `rge-kernel-app` | 0 / 0.0 % | 0 / 0.0 % | — (declared in workspace; no consumer) |
| `rge-kernel-schedule` | 0 / 0.0 % | 0 / 0.0 % | — (declared in workspace; no consumer) |

**Status:** **PHASE 9 PREFLIGHT — no breach. Defer lint and tool implementation.**

- **No crate is anywhere near the 30 % threshold today.** Highest fanout is `kernel/graph-foundation` at **18.9 %** (18 of 95 crates) — **11.1 pp under** the lint-warn ceiling, with **~10.5 crates of headroom** before the warn level fires.
- The earlier rough qualitative estimate (`graph-foundation NodeId ~32 %`, recorded in the §13.3 Compile-time baseline section's "Top 3 risks" #2 entry) was **wrong** in direction-of-error: it conflated *VizAdapter trait usage via `&dyn`* (which doesn't add a crate-level Cargo edge) with *transitive Cargo deps*. The current radius is materially safer than that section implied. The §13.3 entry's qualitative claim should be read in that corrected light.

**Top 3 invalidation-radius risks (qualitative; baseline-state findings):**

1. **No present breach, but 60 % of the workspace is structurally isolated.** 57 of 95 crates have zero workspace-internal Cargo edges in either direction. The current 18.9 % top is a **temporary low-water mark, not a stable equilibrium** — radius will increase materially as stubs land and start consuming kernel substrate. Implication: this baseline must be **revisited periodically**, not treated as evergreen.
2. **`kernel/diagnostics` is the second-place fanout at 15.8 %** with **12 direct normal revdeps** — the densest direct edge count of any crate. Any signature-breaking change to `Diagnostic` / `Severity` / `DiagnosticSink` / `FailureClass` would cascade across 12 crates immediately and 15 transitively. Today this is well under threshold; if `kernel/diagnostics` ever absorbs additional concerns (e.g. structured telemetry, metrics, plugin telemetry), it is the most likely first crate to pierce 25 %.
3. **Three "architectural-root" crates are effectively orphaned by Cargo:** `kernel/types` (2 normal revdeps), `kernel/app` (0), `kernel/schedule` (0); plus `macros-reflect` itself (0). `kernel/types` is documented in PLAN §1.1 as *the* reflection root, but no production crate currently goes through `macros-reflect`-derived reflection — only the macro crate's own `tests/compile_budget_5_pilots.rs` exercises 5 pilot types. This is a **honesty gap between the §1.1 framing and the dep graph**, not a compile-time risk today, but it explains why the §13.3 reflection compile-time gate (`> 30 s on 5 pilot types ⇒ STOP`) has never fired: there *are* no production-reflected types in the workspace yet.

**Revisit triggers** — re-run this `cargo metadata`-based preflight when **either** of the following becomes true:

1. **Any single crate's normal-closure percentage crosses 25 %** (≈ 24 of 95 crates today; ≈ a 5 pp jump from the current top of 18.9 %). At that point the warn-level breach at 30 % is one substrate-merger or one kernel-substrate-consumer landing away, and a real lint becomes warranted.
2. **The isolated-stubs population drops below 30 of 95** (i.e. **more than ~ 65 of 95 workspace crates have wired up to workspace-internal deps**). At that connectivity level, the closure percentages of the existing top crates will have grown enough that radius regression is no longer dominated by stub-state.

Until **at least one** of those fires, treat the current radius as observed-safe and **defer both the lint and any tool wiring**. `tools/invalidation-profiler/` is currently a 5-line `main.rs` stub; that is the correct state for now — building it before either revisit trigger fires would be premature mechanism per PLAN §1.10's "pressure-driven" doctrine.

**Reproducer (read-only, no harness in-tree):**

```
$env:CARGO_HOME='A:\RustCache\cargo'; $env:RUSTUP_HOME='A:\RustCache\rustup'
$env:Path='A:\RustCache\cargo\bin;' + $env:Path
cd A:\RCAD\RGE
cargo metadata --format-version 1 > meta.json
# Parse meta.json with any JSON-aware tool:
#   - workspace IDs: .workspace_members[]
#   - graph: .resolve.nodes[] (each node has .id, .deps[].pkg, .deps[].dep_kinds[].kind)
#   - filter dep_kinds where kind is null (normal) or "build" for the normal-closure;
#     include "dev" for the +dev variant.
#   - transitive reverse closure = DFS through reverse adjacency map.
# The throw-away parser used for this entry lives at
#   C:/Users/halil/AppData/Local/Temp/rge_radius2.py
# but is not committed and not required (any JSON path tool reproduces the same numbers
# from meta.json — Python with `json` stdlib, jq, or PowerShell ConvertFrom-Json).
```

**Notes / caveats:**

- All numbers above are workspace-internal only. External crates.io deps are NOT counted in the percentages — they don't trigger workspace-crate recompilation when their version is unchanged.
- The `+Dev` column matters for `cargo test --workspace` invalidation but NOT for `cargo build --workspace`; the §1.10.4 budget targets the latter, so the **normal-closure column is the primary signal**. The `+Dev` column is included for completeness and to highlight cases (e.g. `kernel/types` 2 → 3, `kernel/diagnostics` 15 → 15, `cad-core` 5 → 5) where test/bench-only deps don't materially shift the picture today.
- The "94 vs 95" workspace-member count discrepancy is harmless. `cargo metadata` is the authoritative count and reports 95; the older "94" wording in `Status.md` / `HANDOFF.md` / `README.md` predates the latest workspace-Cargo.toml addition. A future docs-only reconciliation can refresh those numbers when a meatier Status/HANDOFF sweep is warranted; not in scope for this baseline-record dispatch.
- Two crates show **direct revdep count > normal closure** in the top 10 (`kernel/diagnostics`: direct 12, closure 15; `kernel/ecs`: direct 9, closure 10). That happens when most direct consumers are leaf crates (no further fanout); good news structurally — diagnostics has wide *direct* reach but doesn't compound transitively.
- This preflight does **NOT** measure: compile-time wall-clock impact of a 1-line edit to a core type (that's a §13.3 incremental p95 measurement, separately deferred); reflection schema explosion (separately gated by PLAN §1.1's "> 30 s on 5 pilot types" reflection gate, never fired); or generic-monomorphization count per crate (PLAN §1.10's "5,000 warn / 15,000 hard" threshold, not measured here). It strictly measures *which* crates would be invalidated, not *how long* that invalidation would take to resolve.
- The `cad-projection` closure (2 normal revdeps: `rge-editor`, `rge-editor-shell`) is much smaller than expected given its central architectural role — this is because `cad-projection` is consumed at the *application* layer (editor binary + editor-shell orchestrator), not by downstream Tier-2 crates. The cad-projection moat is wide-but-shallow in graph-shape terms.

---

## §1.1 Reflection-scale honesty preflight (Phase 9)

**Budget anchors and gate references:**

- IMPLEMENTATION.md Phase 1 §1.1 (line 117): "`kernel/types` — FIRST REAL CRATE. The architectural root. Everything depends on this."
- IMPLEMENTATION.md Phase 1 abort (line 190): "> 30 s on 5 pilot types ⇒ STOP and replan reflection strategy."
- IMPLEMENTATION.md Phase 9 §9 (line 597): "Reflection scale — compile time + binary size at 100+ reflected types."
- PLAN.md §13.2 (line 1124): "reflection cache 1000 components ≤ 2 MB."
- PLAN.md §13.3 (line 1128): "gen instantiations per crate ≤ 5,000 warn / ≤ 15,000 hard · trait expansion depth ≤ 8/16."
- PLAN.md §13.10 / §1.10.4 (line 526): "Reflection schema size (typed components × fields) > 10 K = warn."
- **Phase 1.1 compile-budget source of truth: [`kernel/types/BUDGET.md`](../kernel/types/BUDGET.md)** (baseline taken 2026-05-05; not duplicated here).

**This entry is a Phase 9 PREFLIGHT — pure read-only audit of current reflection adoption.** It does NOT change the substrate, add pilot types, or touch any reflection consumer. It establishes the first recorded *adoption* baseline (distinct from the *compile-budget* baseline already in `BUDGET.md`) so future production-reflection landings have an honest before-and-after reference.

**Methodology (read-only):**

1. Inspect crate state via `wc -l` on `kernel/types/src/` and `crates/macros-reflect/{src,tests}/`.
2. Grep-based inventory of `#[derive(Reflect)]`, `rge_macros_reflect::`, and `rge_kernel_types::*` reflection-API imports across all workspace `*.rs` files (excluding `target/`, `.claude/`, `OLD/`, `worktrees/`).
3. Cross-check against the existing Cargo dep declarations (using yesterday's `cargo metadata` parse for `kernel/types` reverse-dependency closure: 2 normal revdeps = `macros-reflect` dev-dep, `script-host`).
4. Distinguish production `src/` usage from `tests/` usage and from doc-comment-only mentions.

### 2026-05-21 — initial reflection adoption snapshot (recorder host, Rust 1.92.0)

**Substrate is real, not a stub:**

| Crate | Source LoC | Test LoC | Cargo shape | Purpose |
|---|---:|---:|---|---|
| `kernel/types` | **1,151** across 7 files (`field_descriptor.rs` 178 / `lib.rs` 63 / `reflect.rs` 283 / `schema_version.rs` 95 / `serde_bridge.rs` 165 / `type_id.rs` 202 / `ui_hint.rs` 165) | (its own `tests/reflect_round_trip.rs` is 1 file) | normal deps `serde` / `ron` / `thiserror` (workspace floor only — explicitly no `blake3` / no `inventory` / no `linkme`) | Hand-rolled FNV-1a-128 `TypeId`, closed-set `UiHint`, `Reflect` trait, `FieldDescriptor`, `SchemaVersion`, RON serde bridge via reflection walk |
| `crates/macros-reflect` | **819** (`attrs.rs` 314 / `codegen.rs` 360 / `derive.rs` 60 / `lib.rs` 85) | **301** (5-pilot probe 99 / `derive_test.rs` 82 / `ui_hints_test.rs` 68 / `validate_attr_test.rs` 52) + `fixtures/render_pass.rs` 90 | `proc-macro = true`; normal deps `proc-macro2` / `quote` / `syn`; dev-dep on `rge-kernel-types` | proc-macro emits `impl rge_kernel_types::Reflect` from `#[derive(Reflect)]`; no `darling`, no `proc-macro-crate`, no generic helpers in emitted code |
| `kernel/types/BUDGET.md` | 84 lines | — | — | Phase 1.1 compile-budget baseline document; recorded 2026-05-05 |

The substrate is **complete and well-engineered**. It is NOT empty, NOT a stub, NOT a placeholder. The Phase 1.1 abort gate has been formally measured; see **[`kernel/types/BUDGET.md`](../kernel/types/BUDGET.md)** for the canonical 5-pilot wall-clock (**7.5 s**, ~4× under the 30 s abort), per-field LLVM-line cost (**~23 lines/field**), and the 100-type extrapolation (**~9,000 LLVM lines**, well under the 15,000 warn threshold). Those numbers are not duplicated here; the BUDGET doc remains the source of truth.

**Production-vs-test adoption inventory:**

| Symbol / pattern | Production `src/` uses (workspace, non-test) | Test uses | Doc-comment-only mentions |
|---|---:|---:|---|
| `#[derive(Reflect)]` | **0** | 7 (all in `crates/macros-reflect/tests/`) | n/a |
| `use rge_macros_reflect::*` / `rge_macros_reflect::Reflect` | **0** (no consumer outside macros-reflect itself) | 3 files (`compile_budget_5_pilots.rs`, `fixtures/render_pass.rs`, `macros-reflect/src/lib.rs` doc example) | n/a |
| `use rge_kernel_types::{Reflect,TypeId,FieldDescriptor,SchemaVersion,UiHint,ReflectValue,from_ron,to_ron}` | **0** in production `src/` | 4 test files (`kernel/types/tests/reflect_round_trip.rs` + 3 in `crates/macros-reflect/tests/`) | 2 doc-only mentions: `crates/components-spatial/src/lib.rs:20` (comment saying "callers should `use rge_kernel_types::Entity;`"); `crates/rge-data/src/lib.rs:39,75` (comment promising `pub use rge_kernel_types::Reflect;` is "a one-line change") — **neither actually imports** |
| Cargo declared dep on `rge-kernel-types` (normal lib) | **2 crates** — `rge-macros-reflect` (dev-dep only, used solely by tests), `rge-script-host` (declared but **0 actual `use rge_kernel_types::...` lines in `script-host/src/` or `script-host/tests/`**) | — | — |

**Reflected-type inventory:**

| Type | File | Production / Test | Real semantic identity? |
|---|---|---|---|
| `Pilot1` | `crates/macros-reflect/tests/compile_budget_5_pilots.rs:18` | Test | No — anonymous compile-cost calibration probe (4 fields) |
| `Pilot2` | same file:29 | Test | No — calibration probe (4 fields) |
| `Pilot3` | same file:40 | Test | No — calibration probe (5 fields) |
| `Pilot4` | same file:55 | Test | No — calibration probe (4 fields) |
| `Pilot5` | same file:67 | Test | No — calibration probe (7 fields; exercises all UI-hint variants) |
| `RenderPass` | `crates/macros-reflect/tests/fixtures/render_pass.rs:16` | Test fixture | Mirrors the rustforge `editor-app/RenderPass` shape from W02; **not** wired into any production renderer in the workspace |
| `WithValidate` | same file:59 | Test fixture | No — exercises `validate` / `custom_drawer` attribute plumbing |

**Total reflected types in workspace: 7. Production: 0. Test-only: 7.**

**Phase 9 + §13.x reflection-gate signal status:**

| Gate | Threshold | Today | Signal status |
|---|---|---|---|
| Phase 1.1 abort (IMPLEMENTATION.md:190) | > 30 s on 5 pilot types | 7.5 s (5 pilots; recorded in BUDGET.md) | **PASS, recorded** |
| §13.3 reflection compile-time projection | ≤ 15,000 LLVM lines for 100-type estimate | ~9,000 (extrapolated in BUDGET.md) | **PASS, recorded (extrapolated)** |
| §13.3 generic instantiations / crate | 5,000 warn / 15,000 hard | 0 (macro emits no generic helpers by design) | **PASS** |
| §13.2 reflection cache 1000 components ≤ 2 MB | 2 MB | n/a — no reflection cache deployed; "no global registry" is a hard architectural constraint per `BUDGET.md` constraint #1 | **VACUOUSLY SATISFIED** |
| §13.10 / §1.10.4 reflection schema size metric | > 10 K typed-components × fields = warn | 0 production fields (24 fields total across 5 test-only pilots) | **VACUOUSLY SATISFIED** |
| Phase 9 §9 evaluation axis: 100+ reflected types | qualitative | 0 production types | **STRUCTURALLY UNMEASURABLE** until production adoption begins |

**Status:** **PHASE 9 PREFLIGHT — substrate complete, production adoption zero. Defer.**

- **`kernel/types` is real substrate but not load-bearing in production yet.** The crate is fully implemented (1,151 LoC across 7 source files with proper trait/serde plumbing), the Phase 1.1 compile-budget is recorded and PASS, and the `#[derive(Reflect)]` proc-macro works end-to-end — but no production code path currently consumes any of it.
- **7 reflected types in the workspace, all 7 test-only. 0 production reflected types. 0 production consumers of `rge-macros-reflect`.** The `RenderPass` fixture mirrors the spec's named pilot type (rustforge `editor-app/RenderPass`) but is in `crates/macros-reflect/tests/fixtures/`, not in `crates/gfx/` or `crates/editor-ui/`.
- **The Phase 9 §9 reflection-scale evaluation is structurally unmeasurable until production adoption begins.** With zero production reflected types, neither compile-time-at-100-types nor binary-size-at-100-types can be sampled against any real workload. The §13.2 reflection-cache budget and the §13.10 schema-size metric are vacuously satisfied for the same reason.

**Top 3 honesty gaps (qualitative; baseline-state findings):**

1. **`kernel/types` is documented as "the architectural root" but has zero production consumers today.** IMPLEMENTATION.md Phase 1 §1.1 line 117 says verbatim: "Everything depends on this." Reality: 0 production `.rs` files import any reflection API. The two Cargo revdeps (`macros-reflect`, `script-host`) are either dev-only or declared-but-unused. This is **aspirational framing**, not load-bearing today. (Same crate showed up in yesterday's `## §1.10.4` invalidation-radius preflight at 2.1 % normal-closure — both preflights triangulate the same gap from different angles.)
2. **Phase 9 §9 reflection-scale evaluation has nothing to evaluate.** The §13.3 compile-time scaling table in `BUDGET.md` extrapolates linearly from 5 → 100 types and predicts ~9,000 LLVM lines, well under the warn threshold. But that prediction is **unverified against any production workload**: no production type has ever been reflected, so the per-type LLVM cost in a real consumer crate (which would also link `serde` / `ron` infrastructure separately) is unknown. The Phase 9 gate cannot fire and cannot regress; it can only be unblocked by a real consumer landing first.
3. **`script-host`'s declared `kernel/types` Cargo dep is dead substrate.** `crates/script-host/Cargo.toml` carries `rge-kernel-types = { path = "../../kernel/types" }`, but `crates/script-host/src/**/*.rs` contains zero `use rge_kernel_types::...` lines and `crates/script-host/tests/**/*.rs` is the same. The dep is either a forward-looking declaration awaiting the generic reflect-based hot-reload migration referenced in this BASELINE.md at `Phase 3.2` Notes/caveats ("real-scene swap latency depends on the reflection cost; pending the generic bridge, the 0.31 ms above is a lower bound") or accumulated cruft. Either way it's the **only** workspace-Cargo-graph signal that something outside `macros-reflect` "intends" to use reflection — and that intent is currently un-acted-upon.

**Revisit triggers** — re-run this preflight when **either** of the following becomes true:

1. **Any production crate (non-test) adds its first `#[derive(Reflect)]` derive or its first `use rge_kernel_types::Reflect` (or other reflection API) import.** This signals real adoption pressure has begun and the Phase 9 §9 evaluation axis becomes meaningfully measurable.
2. **`script-host` actually wires its declared `kernel/types` Cargo dep into the generic hot-reload migration path** (i.e. replaces the hand-rolled `CounterSnapshot` per this BASELINE.md's Phase 3.2 Notes/caveats with a `Reflect`-driven value-walk). This signals the canonical "generic bridge" consumer referenced in the existing baseline is materializing.

Until **at least one** of those fires, treat the reflection substrate as observed-deployed-but-unused, **defer any reflection adoption work**, and **do not add new pilot types** — adding more synthetic pilots would conflate compile-budget calibration with adoption signal. The substrate's correctness is already proven by the existing 7 test-only types + the `kernel/types/tests/reflect_round_trip.rs` round-trip test; further calibration is only warranted once a real consumer dictates the value-walk shape (inspector vs hot-reload-migration vs asset-metadata vs component-RON have different optimal trait surfaces).

**Notes / caveats:**

- The "tiny adoption task" path was explicitly considered and rejected per the user directive after the preflight ("document and defer"). The closest candidates were: (a) **editor inspector widget** consuming `Reflect` for `Slider` / `ColorRgb` / `FilePath` UI hints — but no `inspector.rs` exists in `crates/editor-ui/src/widgets/` today; (b) **`script-host` generic hot-reload migration** — substantive substrate dispatch, not "tiny"; (c) **`rge-data` `pub use rge_kernel_types::Reflect;`** per the doc-comment promise at `crates/rge-data/src/lib.rs:39,75` — would be a one-line edit but landing it without a simultaneous consumer would be premature mechanism per PLAN §1.10's pressure-driven doctrine.
- This preflight does NOT propose shrinking or simplifying `kernel/types`. The substrate is healthy and well-bounded (the BUDGET.md constraints — no global registry / no generic helpers in derive output / no heavy hash crate / `UiHint` serialize-only — are load-bearing). Shrinking it before a consumer materializes would risk later having to re-add what was removed, at higher cost.
- The 95-vs-94 workspace-crate count discrepancy noted in the §1.10.4 preflight is also visible here in the form of `kernel/types`-related crates: the workspace has 1 macro crate (`macros-reflect`) and 1 reflection-substrate crate (`kernel/types`); no reflection-consuming production crate exists. Both counts agree with `cargo metadata`.
- Reproducer for the consumer inventory (read-only grep, no harness in-tree):
  ```
  # production-reflect-derives (expect zero outside crates/macros-reflect/tests/):
  rg "#\[derive\([^)]*\bReflect\b" --type rust
  # kernel/types reflection-API imports outside its own tests + macros-reflect tests:
  rg "use rge_kernel_types::(Reflect|TypeId|FieldDescriptor|SchemaVersion|UiHint|ReflectValue|from_ron|to_ron)" --type rust
  # macros-reflect imports outside macros-reflect itself:
  rg "use rge_macros_reflect::" --type rust
  ```
- This preflight is read-only and complementary to (not a replacement for) `kernel/types/BUDGET.md`. The BUDGET doc owns Phase 1.1 compile-budget numbers and their re-running instructions; this entry owns the **adoption** baseline and the two-arm revisit trigger. They should be re-read together when either trigger fires.

---

## Editor-usability preflight (Phase 9)

**Budget anchors and gate references:**

- IMPLEMENTATION.md Phase 9 §9 (line 600): "Editor usability — friction points from real authoring."
- IMPLEMENTATION.md Phase 5 §5.1–§5.2 (lines 374, 384): `editor-shell` + `editor-state` (narrow per §1.15).
- IMPLEMENTATION.md Phase 2 §2.2 (line 217): `editor-actions` (Command Bus) — **VERY EARLY**.
- PLAN.md §1.15: editor-state coordination-not-authority rule (selection / hover / active-tool only; no authoritative content types).
- PLAN.md §6.16.7: Command Bus 500 ms coalesce semantics.

**This entry is a Phase 9 PREFLIGHT — pure read-only audit of the editor's actual user-facing surface.** It does NOT add commands, wire keyboard handlers, build scene serialization, or change any substrate. It is the first recorded *editor-usability* baseline (distinct from the Phase 5 / Phase 6 substrate-closure records already in this doc) so future user-loop landings have an honest before-and-after reference.

**Methodology (read-only):**

1. Inventory of `[[bin]]` entry points and `src/main.rs` across `crates/editor-*/`, `editor/`, `apps/`, `tools/`.
2. Read `lib.rs` + key modules + `Cargo.toml` of `editor-shell`, `editor-ui`, `editor-actions`, `editor-state`, `components-editor`, `anim-graph-editor`, `material-graph-editor`.
3. Grep across `editor-*` for: `open_file`, `load_project`, `save_project`, `.rge` / `.rgeproj` / `.scene` / `.project` file-extension string literals, `unimplemented!()` / `todo!()` / stub markers, `KeyboardInput` event branches, call sites of `io-gltf` / `io-image` / `io-3mf` public APIs.
4. Cross-check the editor's call graph against the `CommandBus::submit` / `Action::apply` / `Action::revert` signatures to determine whether user-visible CAD mutations can flow through the existing bus.
5. Test inventory across `editor-*` (`#[test]` count + integration vs unit breakdown + workflow coverage).

### 2026-06-01 — Save-As to a NEW `.rge-project` tree landed (#283 substrate + #284 wiring)

**Forward-only follow-up (SAVEAS-STATUS-SNAPSHOT).** The 2026-06-01 subsection below ("Editor Open/Save surface landed (#264–#281)") and its **Still-open** list recorded "Save-As to a *new* `.rge-project` tree (creating a fresh project directory) remains a carried/deferred item." That shipped; this prepend supersedes it. The subsection below is preserved **byte-identical** (no in-place rewrite). Grounded at main commit `a74e479`.

**Now CLOSED — Save-As to a new project tree.** End-to-end:

- **Substrate (#283 NEWPROJECT-SAVE-SUBSTRATE).** `rge_scene_loader::save_world_as_new_project(world, project_dir) -> Result<PathBuf, NewProjectWorldSaveError>` creates `<dir>/.rge-project` (manifest: folder-derived `name`, `V0_1_0`, `target_tiers: [Desktop]`, no plugins, `scenes: ["scenes/main.rge-scene"]`) + `<dir>/scenes/main.rge-scene` from the live world, returning the created `.rge-project`; **no-clobber** — errs if either path already exists. Round-trips through `load_scene_world_from_path`.
- **Wiring (#284 NEWPROJECT-SAVE-WIRING).** **`Ctrl+Shift+S`** → `EditorKeyCommand::SaveAsProject` → `EditorShell::handle_save_as_new_project_request` over the binary-owned `NewProjectSaveDialog` (rfd `pick_folder`) + `NewProjectSaveHook` (over the substrate fn). On success **adopts** `SaveSource::Project { path: <created>, name: <folder-derived; None if non-UTF-8> }` and marks saved — so the next plain `Ctrl+S` overwrites it silently. PIE-gated; cancel / no-dialog / no-hook / hook-error all log + no-op. editor-shell stayed loader-free / rfd-free (`forbidden-dep` rule 7).

The editor now supports the full authoring loop: **Open** (`Ctrl+O`), **Save** (`Ctrl+S` — `.rge-scene` or `.rge-project`, silent overwrite by `SaveSource`), and **Save-As to a new `.rge-project` tree** (`Ctrl+Shift+S`).

**Still open — explicitly NOT closed here:**

- **Menu-entry wiring for Save-As** — there is still no functional `MenuRegistry::resolve` dispatch (`Command::OpenFile` carries only a diagnostic id); Save-As is keyboard-only (`Ctrl+Shift+S`).
- A last-directory-memory dialog; an in-app confirmation when the picked folder is non-empty.
- The non-Open/Save audit gaps (drag-drop ingestion, `io-image` consumption, the World-only Command-Bus `Action` context) are **unchanged** — as the 2026-05-28 ISSUE-256 entry records.

**Scope:** docs-only, forward-only. No source / test / `Cargo.toml` change; the 2026-06-01 (#264–#281) subsection and all earlier dated entries below are byte-identical.

### 2026-06-01 — Editor Open/Save surface landed (#264–#281); SAVE-direction + in-app-open gaps CLOSED

This subsection forward-reconciles the dated 2026-05-28 reconciliation below (grounded at `6e24706`, pre-#264) and the 2026-05-21 snapshot beneath it, both of which recorded the editor's **SAVE direction** as having "no path at all" and **non-CLI open/load UX** as "absent." Both are now stale: the in-app file **Open/Save** authoring loop shipped across the contiguous PR run **#264–#281**. Grounded at main commit `f76e001`. This is a pure prepend — the 2026-05-28 and 2026-05-21 dated content below is preserved byte-identical; reconciliation is never by in-place edit.

**Gap 1 (`:588`, scene/project persistence — the SAVE direction) — now CLOSED.** A runtime serializer path exists end-to-end, directly superseding the 2026-05-28 "(4) SAVE direction has no path at all" / 2026-05-21 "the editor never calls it … cannot save user work":

- `.rge-scene` writer `rge_scene_loader::save_scene_world_to_path` (`crates/rge-scene-loader/src/lib.rs:534`; World→rge-scene extraction + save, #267 SCENE-SAVE-SUBSTRATE), wired to in-app **Ctrl+S** Save / Save-As (#268 SCENE-SAVE-WIRING).
- **True Save** = silent overwrite of the opened source (#269 SCENE-SAVE-SOURCE-PATH).
- `.rge-project` writer `rge_scene_loader::save_project_world_to_path` (`:635`, #273 PROJECT-SAVE-SUBSTRATE).
- **Ctrl+S routed by `SaveSource`** `{ Scene(PathBuf), Project { path, name } }` (`crates/editor-shell/src/lifecycle/save_source.rs:25`), replacing the earlier `scene_source_path` (#274 PROJECT-SAVE-WIRING).

**"Non-CLI open/load UX is absent … no in-app file picker or 'File → Open' gesture" — now CLOSED.** In-app **Ctrl+O** scene Open landed (`crates/editor-shell/src/lifecycle/open_request.rs::handle_open_request` at `:228`, #266 SCENE-OPEN-WIRING) over the `EditorShell::replace_world` runtime world-swap substrate (`crates/editor-shell/src/lifecycle/mod.rs:753`, #265 EDITOR-WORLD-SWAP) and the scene-path resolver promoted into `rge-scene-loader` (#264 SCENE-WORLD-BRIDGE), with GLB-watcher teardown on Open. The Open dialog is mediated by a binary-owned `SceneOpenHook` seam so `editor-shell` stays loader-free.

**Surfacing of the open source (new since 2026-05-28).** Window title reflects the open source + dirty state (#270 EDITOR-WINDOW-TITLE); an in-app bottom status bar shows source name + dirty (#271 EDITOR-SAVE-STATUS-INDICATOR); `SaveSource::display_name()` (`save_source.rs:76`) shows a `.rge-project`'s manifest name (folder name as fallback), not the literal `.rge-project` (#275 SAVE-SOURCE-DISPLAY-NAME, #279 PROJECT-NAME-DISPLAY, tests+prose #281 PROJECT-NAME-DISPLAY-FOLLOWUP); status wording is source-neutral (`scene_file_name`→`source_name`, "No scene"→"No file"; #277 SAVE-STATUS-SOURCE-NEUTRAL); the key-command renamed `MarkSaved`→`Save` (#278 KEYCOMMAND-SAVE-RENAME). Boundary hardening: `editor-shell` is loader-free, machine-enforced by `forbidden-dep` rule 7, and `editor-state-ownership` Part B was revived (#280 ARCH-LINT-EDITOR-BOUNDARIES).

**Still open — explicitly NOT closed by this reconciliation (anti-over-claim, per the §253 / §256 grounding discipline):**

- **Save-As to a *new* `.rge-project` tree** (creating a fresh project directory) remains a carried/deferred item — only saving to an already-known source and the `.rge-project` *writer* shipped.
- The **other** 2026-05-28 still-open gaps are **unchanged** and out of this Open/Save scope: menu-command execution (no functional `MenuRegistry::resolve` dispatch; `Command::OpenFile` still carries only a diagnostic id at `crates/editor-ui/src/menus/command.rs:103`), drag-drop ingestion, `io-image` consumption, and the World-only Command-Bus `Action` context (`crates/editor-actions/src/action.rs:87` — cannot reach `CadGraph` / `CadProjection`). This subsection makes **no** claim about those.

**Scope:** docs-only, forward-only. No source / test / bench / fixture / `Cargo.toml` change; no other `plans/BASELINE.md` subsection (W03 / W04 / W08 / 6.3 / 13.2 / Live-inspector wiring preflight) or the Editor-usability `:622-639` Notes/caveats block touched; the 2026-05-28 and 2026-05-21 dated content below is byte-identical.

### 2026-05-28  Editor-usability preflight reconciliation (post-ISSUE-225 / dispatch-G / ISSUE-249 + Phase-9 keyboard wiring)

This subsection forward-reconciles the dated 2026-05-21 "initial editor-usability adoption snapshot" below, which has aged substantially. It is grounded in the correction-loop-verified audit `ai_handoffs/ISSUE-254_EXEC_2026-05-28_23-49-37+0300.md` (passed Codex control after two CORRECT rounds), and every closure-evidence citation below was re-confirmed against current source at main commit `6e24706` for this reconciliation — the audit is the map; current source is the territory. The 2026-05-21 dated content below (the entry-point table, the "Workflows that work end-to-end TODAY" table, the test-coverage paragraph, the Top 3 gaps, the rejected `F → SpawnCuboidAt` analysis, the Status, the Revisit triggers, and the Notes / caveats block) is preserved byte-identical; reconciliation is by this prepend, never by in-place edit.

This dispatch supersedes the abandoned ISSUE-253 (control-blocked, closed not-planned), which asserted "Gap 2 UNCHANGED" and "neither revisit trigger fired" by extrapolating from the dated 2026-05-21 text instead of reading current source. The #254 audit was filed expressly to ground-truth current state before this reconciliation.

**Headline current reality (grounded at main commit `6e24706`):** the editor now has launch-time load paths — `--glb <path>` (dispatch G) and `--scene <path>` (ISSUE-225) — and `--scene` renders a visible egui-dock window (ISSUE-249). Six Ctrl-bound keyboard commands (Ctrl+Z / Ctrl+Y / Ctrl+S / Ctrl+0 / Ctrl+2 / Ctrl+4) route through the `editor-actions` CommandBus, and two plain-key playback commands (Space / Escape) route through the PIE PlayState. **Still absent:** the SAVE direction (no runtime serializer call site at all), any non-launch-time open/load UX, menu-command execution, drag-drop ingestion, and `io-image` consumption. The bus's `Action` context remains World-only, so the wiring that closed is keyboard-shaped, not CAD-shaped.

**Gap re-classification (each verdict re-grounded against current source):**

- **Gap 1 (`:588`, scene/project persistence) — PARTIALLY STALE.** Load direction is wired: `--scene` parses `.rge-project` / `.rge-scene` RON and lands a populated `World` (`editor/rge-editor/src/main.rs:912-968`; deps `editor/rge-editor/Cargo.toml:36-38`), and `--glb` imports a GLB (`main.rs:971-1026` → `import_glb` at `main.rs:612`; dep `Cargo.toml:29`). The SAVE direction has **no path at all** — there is no `ron::ser` / `ron::to_string` / `save_project` / `save_scene` / `save_to` / `write_to_file` runtime symbol in `editor/rge-editor/src/` or `crates/editor-shell/src/` (the only `fs::write` hits are six test-fixture writes under `#[cfg(test)]` in `main.rs`). Ctrl+S marks the bus saved-cursor (`crates/editor-shell/src/lifecycle/commands.rs:309`), not a filesystem write. So the "the editor never calls" the RON serializer claim is stale for load, still current for save.
- **Gap 2 (`:590`, Command Bus unreachable from editor UI) — SUBSTANTIALLY CLOSED.** The keyboard-to-bus wire is real: `crates/editor-shell/src/lifecycle/commands.rs:280` (`command_bus.submit`, plus undo `:291` / redo `:302` / mark_saved `:309`) is reached from the `WindowEvent::KeyboardInput` arm at `crates/editor-shell/src/lifecycle/mod.rs:1676` (gated on `!egui_consumed`). The 2026-05-21 claim that the keyboard catch-all "swallows every `KeyboardInput`" is stale — the catch-all `_ => {}` moved to `mod.rs:1725`, downstream of the real keyboard arm at `:1676`. The residual narrower gap: the bus `Action` context is World-only (`crates/editor-actions/src/action.rs:87`, unchanged), so CAD-graph mutations still cannot flow through the bus.
- **Gap 3 (`:592`, MenuRegistry + io-* loaders) — PARTIALLY STALE.** `io-gltf::import_glb` is now called (`main.rs:612`, via `--glb`). MenuRegistry is **not** reached functionally from the editor surface — the only literal `MenuRegistry` token in editor-shell / rge-editor / editor-egui-host is a doc-comment cross-reference at `crates/editor-shell/src/play_toolbar.rs:12`; the functional searches `MenuRegistry::`, `ResolvedEntry`, `menus::Command`, and `.resolve(` are individually zero in that surface. `io-image` is **not** consumed — zero `io_image::` / `load_path` / `load_bytes` matches and no `rge-io-image` dep in `editor/rge-editor/Cargo.toml` (image bytes arrive only co-bundled via `rge_io_gltf::MaterialAsset`). Drag-drop ingestion is absent (zero `DroppedFile` / `HoveredFile` / `DragAndDrop` matches).

**Revisit-trigger reality (`:615-620`):**

- **Trigger 2 (a non-CAD user-input path lands first) — FIRED.** PIE Play/Stop is bound to the keyboard: `EditorPlaybackCommand::{TogglePlay, Stop}` maps Space / Escape (`crates/editor-shell/src/lifecycle/playback.rs:110-123`) and drives the PlayState toolbar buttons (`:156-188`), reached from `mod.rs:1706-1709`. This is precisely the 2026-05-21 example "PIE Play/Stop bound to a keyboard shortcut."
- **Trigger 1 (a CommandBus integration design decision) — AMBIGUOUS.** Implementation landed (the keyboard-to-bus wire plus the single production `SetTimeScale` Action), but **no formal design artifact exists** — no ADR, no `EditorCommandCtx` aggregate, no `(&mut CadGraph, &mut CadProjection)` extension. The World-only CommandBus posture is **IMPLICIT-VIA-SHIPPED-CODE** (`crates/editor-actions/src/action.rs:87`, byte-identical to the 2026-05-21 record), not a documented prior decision. This dispatch does **not** declare Trigger 1 fired; whether the de-facto wiring suffices or a docs-only ADR is still required is a human-arbitration call.
- Because Trigger 2 has fired, the 2026-05-21 closing guidance at `:620` — "defer all user-facing editor wire-up dispatches" — is **no longer in force**.

**Phantom-reference callout (governance-surface drift).** `plans/BASELINE.md`'s own narrative and ISSUE-251's 2026-05-28 live-inspector reconciliation both refer in passing to a "CommandBus integration design preflight (decided the bus stays World-only)" as if a standalone section recorded that decision. **No such section exists.** The only BASELINE.md prose that decides "World-only" is this very Editor-usability preflight — the reference is self-referential drift. This reconciliation corrects the drift by acknowledgment: the World-only state is IMPLICIT-VIA-SHIPPED-CODE (`crates/editor-actions/src/action.rs:87`), not a prior formal decision. It deliberately does **not** author an ADR or "design preflight" artifact to make the phantom reference resolve cleanly; manufacturing the referenced artifact is the anti-pattern this dispatch exists to avoid. A forward-looking CommandBus decision record, if later desired, is a separate present-dated chip.

**How the 2026-05-21 text aged (recorded in passing, not edited in place).** The dated `:625` rejected-micro-dispatch (b) — "`--load <gltf-path>` CLI arg invokes `io-gltf::import_glb`" — was subsequently shipped as `--glb` (dispatch G; `main.rs:971-1026` + `:612`). The 2026-05-21 wording is preserved verbatim below as dated history.

**Still-open usability gaps the #254 audit ground-truthed (audit §4, Gaps 4-10), each source-grounded at `6e24706`:**

- Save direction has no path at all (no runtime serializer symbol; the only `fs::write` hits are six test fixtures under `#[cfg(test)]` in `main.rs`).
- Menu-command execution is absent (no functional `MenuRegistry::resolve`; `Command::OpenFile` carries a diagnostic id at `crates/editor-ui/src/menus/command.rs:103` but no editor-surface code dispatches the variant).
- Drag-drop ingestion is absent (zero `DroppedFile` / `HoveredFile` / `DragAndDrop` across the editor surface).
- `io-image` is unused on the editor surface (zero `io_image::` / `load_path` / `load_bytes`; no `rge-io-image` dep in `editor/rge-editor/Cargo.toml`).
- Non-CLI open/load UX is absent (files arrive only via boot-time `--glb` / `--scene` or the ISSUE-85 notify watcher on an already-`--glb`-bound path; there is no in-app file picker or "File → Open" gesture).
- CommandBus coverage is keyboard-shaped, not CAD-shaped — `SetTimeScale` is the only production `Action` impl; the World-only `Action::apply` signature (`crates/editor-actions/src/action.rs:87`) cannot reach `CadGraph` / `CadProjection`.
- The spawner registry still registers `PlaceholderTabBody` for `"tab/inspector"` (`crates/editor-ui/src/dock/spawner_registry.rs:165-168`, unchanged); the inspector renders via the host-internal `TabBody::Inspector` (`crates/editor-egui-host/src/tabs.rs`), a separate path that does not flow through the spawner registry.

**Explicitly preserved as dated methodology history (this reconciliation is bounded and does not edit them):** the `:573-582` "Workflows that work end-to-end TODAY" table, the `:584` test-coverage paragraph (the 312-`#[test]` inventory), and the entire `:622-639` Notes / caveats block (reproducer grep recipes, complementary-baselines text). These remain load-bearing dated records; this subsection references their aging items in prose without editing them.

**Closure-evidence citations (re-confirmed against current source at main `6e24706`):**

- `crates/editor-shell/src/lifecycle/commands.rs` — `EditorKeyCommand` surface + `command_bus.submit` at `:280` (undo `:291` / redo `:302` / mark_saved `:309`).
- `crates/editor-shell/src/lifecycle/playback.rs:110-123` (Space `TogglePlay` / Escape `Stop` mapping) + `:156-188` (`handle_playback_command`).
- `crates/editor-shell/src/lifecycle/mod.rs:1676` (the `WindowEvent::KeyboardInput` arm; catch-all moved to `:1725`).
- `crates/editor-shell/src/render_path.rs:279-285` (post-ISSUE-249 init split: `has_cad_scene || has_prebuilt_mesh` guards Phase 2 only), `:313-328` (EguiHost construction + InspectorHandoff stash), `:510-610` (the egui-only `render_frame_egui_only` branch that makes the `--scene` window visible).
- `editor/rge-editor/src/main.rs:912-968` (`--scene`), `:612` (`import_glb`), `:971-1026` (`--glb`).
- `editor/rge-editor/Cargo.toml:36-38` (`--scene` `rge-data` / `rge-scene-loader` / `ron` deps), `:29` (`rge-io-gltf` dep).
- `crates/editor-actions/src/action.rs:87` (World-only `Action` signature, unchanged).
- `crates/editor-ui/src/dock/spawner_registry.rs:165-168` (`PlaceholderTabBody` registration, unchanged).
- Audit basis: `ai_handoffs/ISSUE-254_EXEC_2026-05-28_23-49-37+0300.md`.

Forward-only snapshot pattern matches the ISSUE-243 / ISSUE-245 / ISSUE-251 precedent (and directly mirrors ISSUE-251's live-inspector prepend in this same file). ISSUE-249 (`--scene` window) and dispatch G (`--glb`) are closure evidence, not snapshot precedent.

### 2026-05-21 — initial editor-usability adoption snapshot (recorder host, Rust 1.92.0)

**Editor binary entry point (confirmed):**

| Binary | Path | Entry | What it does at launch today |
|---|---|---|---|
| `rge-editor` | `editor/rge-editor/Cargo.toml:12-14` → `editor/rge-editor/src/main.rs:38-96` | `fn main()` | Constructs `CadGraph` with one hardcoded `CuboidOp(1.0, 1.0, 1.0)` (line 47); spawns one ECS entity with `BRepHandle` (line 68); ticks `CadProjection` once (line 83); hands world / projection / graph to `EditorShell::with_world_projection_graph()` (line 87); runs winit event loop; renders the cuboid with Lambert+Phong + directional light |

Only one editor binary exists today; `crates/editor-shell/src/bin/` does not contain a binary entry point.

**Workflows that work end-to-end TODAY:**

| Workflow | Status | Citation |
|---|---|---|
| Launch `rge-editor`, render the hardcoded 1×1×1 cuboid | ✅ WORKING | `editor/rge-editor/src/main.rs:38-96`, `crates/editor-shell/src/render_path.rs:471-578` (clear color `0.12, 0.12, 0.14` at `:509`; one `draw_indexed` for the cuboid + optional second `draw_indexed` for the highlight overlay) |
| Mouse cursor tracking | ✅ WORKING | `crates/editor-shell/src/lifecycle.rs:760-765` updates `self.cursor_pos` on `CursorMoved` |
| Left-click face picking + orange highlight overlay (sub-ε) | ✅ WORKING | `lifecycle.rs:767-774` (`MouseInput` left-press → `handle_left_click`); `crates/editor-shell/src/camera.rs::pick_face_at()`; `crates/editor-shell/src/pick_path.rs` (`rebuild_highlight_overlay`); `crates/cad-projection/src/picking.rs:194` (`CadProjection::pick_face()`); highlight color constant at `render_path.rs:69` |
| Play / Stop / Pause / Step PIE — byte-identical world snapshot across 100 ticks | ✅ WORKING (Phase 5.3 CLOSED) | `lifecycle.rs:479` (`handle_button`); `crates/editor-shell/src/snapshot.rs` (`WorldSnapshot::capture_and_audit` / `restore_and_audit`); 8 tests in `lifecycle.rs` |
| Editor-coord state (`Selection` / `Hover` / `ActiveTool` / `FaceSelection`) persists across Play/Stop | ✅ WORKING | `crates/editor-shell/src/coord.rs` (`EditorCoord`); `crates/editor-shell/tests/snapshot_correctness.rs:24,45,84` |
| Workspace layout persistence (egui_dock RON files) | ✅ WORKING | `crates/editor-ui/src/dock/layout_service.rs` (`LayoutService::{load,save}`); `crates/editor-ui/tests/workspace_round_trip.rs:41,57` |
| Render handoff (latest-only single-threaded proxy) | ✅ WORKING (Phase 6.2) | `crates/editor-shell/src/render_input.rs` (`RenderHandoff::{acquire,publish}`); `crates/editor-shell/tests/render_input_boundary.rs:27,72,160,396` |
| Time-scale dilation (0.5× halves game progress; editor unaffected) | ✅ WORKING | `crates/editor-shell/tests/time_scale_test.rs:32` |

**Test coverage:** **312 `#[test]` annotations across `editor-*` crates** — 74 integration (`editor-shell/tests/` 42 + `editor-ui/tests/` 32) + 238 inline / `#[cfg(test)]` unit (`editor-shell/src/` 68 + `editor-ui/src/` 81 + `editor-actions/src/` 23 + `editor-state/src/` 33 + dock/layout subsystem 33). Strong coverage of: PIE snapshot semantics, face picking via camera ray, render-input handoff, editor/game-state boundary discipline, time-scale game-systems dilation, workspace-layout disk round-trip. **No coverage** of: full UI event loop (mouse clicks / keyboard input through `window_event`), WASM script reload via editor, multi-entity scene complexity, fillet/loft operator-graph UX flow.

**Top 3 usability gaps (qualitative; baseline-state findings):**

1. **No scene/project persistence — zero file-I/O paths for actual scene state.** No `open_file` / `load_project` / `save_project` symbols anywhere in `editor-shell` or `rge-editor`. Zero string-literal matches for `.rge` / `.rgeproj` / `.scene` / `.project` extensions. `crates/rge-data` declares `serde` + `ron` dependencies (e.g. `crates/cad-projection/Cargo.toml:19-20`) and a RON serialization API exists, but **the editor never calls it**. The editor cannot save user work and cannot reload anything; the hardcoded `CuboidOp(1.0, 1.0, 1.0)` is the only shape that ever exists. The authoring loop is therefore **structurally unmeasurable** — there is no loop to friction-test.

2. **Command Bus is fully implemented in `editor-actions` but structurally unreachable from the editor UI.** `crates/editor-actions/src/bus.rs:86` defines `CommandBus` with `submit` (line 143), `UndoStack`, `SaveMark`, 500 ms coalesce window, audit-ledger projection, and full unit coverage. **Zero call sites from `editor-shell` / `rge-editor` dispatch any user-triggered command through the bus.** `crates/editor-shell/src/lifecycle.rs::window_event()` ends with a catch-all `_ => {}` (line 776) that silently swallows every `KeyboardInput` event the OS delivers. The user cannot press Ctrl+Z, cannot trigger any command from the keyboard, cannot escape a tool. The Phase 2 doctrine ("Command Bus VERY EARLY", IMPLEMENTATION.md:217) has materialized as substrate but has **no production user yet** at the editor surface.

3. **`MenuRegistry` + `io-*` asset loaders are both ready internally but never called from the editor.** `crates/editor-ui/src/menus/registry.rs` (`MenuRegistry::declare_extension_point` / `register_entry` / `resolve`) is closed per W08 and tested; the `menus::Command` enum is defined. **Zero menu handlers are wired** — `ResolvedEntry` produced by the registry is never acted upon by editor-shell. `crates/io-gltf/src/lib.rs:20-24` (`import_glb` / `export_glb`) and `crates/io-image/src/lib.rs:80,87` (`load_path` / `load_bytes`) are public APIs but **have zero call sites** from `editor-shell` or `rge-editor`; no drag-drop path; no CLI argument parsing for a file path. Users cannot "File → Open" anything. Phase 5 W08 menu substrate and the Phase 4 `io-*` loaders are **paper-only at the editor surface**.

**Cross-cutting pattern.** Substrate-first architecture has worked: PIE / Command Bus / MenuRegistry / io-gltf / io-image / CadProjection / LitMeshPipeline are all closed and tested in isolation, each with their own gate-recorded baselines in this doc. **But no user-input path connects any of them to a visible scene change.** The editor today is a rendering + picking testbed with battle-tested internals nobody can drive from the UI.

**Rejected/boundedness note — F → `SpawnCuboidAt` proposal:**

A direct preflight follow-up was considered and **rejected as not bounded**: "wire `F` keypress → dispatch `Command::SpawnCuboidAt(Vec3)` through `CommandBus::submit()` → spawn a second visible cuboid; map `Ctrl+Z` to `CommandBus::undo()`; one headless integration test asserting entity-count round-trip." On surface inspection this looked like ≤ 200 LoC source + ≤ 100 LoC test. The actual scope is larger:

- **`CommandBus` is World-only today.** `crates/editor-actions/src/bus.rs:143` signs `submit(action: Box<dyn Action>, world: &mut World)`; `crates/editor-actions/src/action.rs:74-96` signs `Action::apply(&self, world: &mut World)` and `revert(&self, world: &mut World)`. The `Action` trait has **no access** to `CadGraph`, `CadProjection`, or any editor-shell render projection state. `BusEntry::apply/revert` at `bus.rs:33,44` mirror the World-only signature.
- **A visible CAD-cuboid spawn requires mutating `CadGraph` and producing a fresh `CadProjection` snapshot** — neither of which lives in `World`. The current cuboid is constructed in `editor/rge-editor/src/main.rs:47-68` against the standalone `CadGraph` + `CadProjection` instances that are handed to `EditorShell` once at construction time (`with_world_projection_graph(world, projection, graph)` at `main.rs:87`).
- **`EditorShell::with_world_projection_graph` is explicitly single-cuboid / sub-δ scope.** The render path assumes a single `BRepHandle` and a single mesh in the projection cache (matching the sub-δ.1.B closure noted in `render_path.rs`). A second visible cuboid would require:
  - Extending the projection-side rendering path to iterate multiple `BRepHandle` meshes (currently single-cuboid by construction).
  - Either (a) a `CommandBus` context redesign that exposes `&mut CadGraph` + `&mut CadProjection` to `Action::apply` (changing the World-only invariant on which the existing 23 `editor-actions` tests rely), or (b) a parallel "editor command" channel that mutates editor-shell state outside the `Action` trait (which forks the architecture and the audit-ledger story).
  - Snapshot/restore semantics for entity-count changes mid-undo-stack, which the current `WorldSnapshot` was not exercised against — `pie_round_trip.rs:156` covers entity-count-preserved-across-Play/Stop but not undo-on-Play/Stop boundary.

The minimum bounded next dispatch is therefore **NOT** a visible-CAD-spawn task. It is a smaller **CommandBus integration design / adapter** dispatch that explicitly decides whether the bus stays World-only or grows an editor command context — and what the cad-graph mutation path looks like either way. That design dispatch is not started here.

**Status:** **PHASE 9 PREFLIGHT — substrate complete in isolation, no user-input path connects to it, defer.**

- The editor binary renders + picks but cannot save / load / spawn / undo through user input.
- All three usability gaps (persistence / Command-Bus-from-UI / menu+asset wiring) share the same shape: the underlying substrate is closed and tested, only the input-to-substrate path is missing.
- The natural "small" follow-up (F → SpawnCuboidAt) is bigger than it looks because of the World-only `CommandBus` / `Action` trait surface.

**Revisit triggers** — re-run this preflight when **either** of the following becomes true:

1. **A CommandBus integration design dispatch lands a decision** on whether `CommandBus::submit` stays `(&mut World)`-only or grows a richer context (e.g. `(&mut World, &mut CadGraph, &mut CadProjection)` or a typed `&mut EditorCommandCtx` aggregate), and what the corresponding `Action::apply`/`revert` signature looks like. The decision itself can be a docs-only ADR / design note plus a stub adapter; it does not have to land the full multi-context bus.
2. **A non-CAD user-input path lands first** — e.g. workspace-layout RON load/save bound to `Ctrl+S` / `Ctrl+O` (already has working substrate per `workspace_round_trip.rs`), or PIE Play/Stop bound to a keyboard shortcut. Either would surface the `KeyboardInput` catch-all at `lifecycle.rs:776` and force a real `window_event` keyboard branch without first redesigning the bus.

Until **at least one** of those fires, **defer all user-facing editor wire-up dispatches**. The substrate quality is high and not at risk; the architectural cost of wiring the wrong abstraction first (e.g. fork the bus, or bypass the audit ledger to "ship something") is high.

**Notes / caveats:**

- This preflight does NOT propose shrinking, refactoring, or rewriting any existing editor substrate. PIE / Command Bus / MenuRegistry / `editor-coord` / `LitMeshPipeline` / `RenderHandoff` are all healthy and well-tested. The gap is purely the absence of input → substrate plumbing.
- **F → `SpawnCuboidAt` is not the only rejected micro-dispatch** — also considered and deferred for the same architectural-cost reason: (a) `Ctrl+S` writes workspace RON to a fixed path (would land easily but ships a half-loop with no `Ctrl+O` partner); (b) `--load <gltf-path>` CLI arg invokes `io-gltf::import_glb` (would land easily but ECS/projection ingestion of an external mesh is unexercised and the projection cache today assumes the `CadGraph`-owned mesh, not an imported one); (c) wire `MenuRegistry::resolve` output to a no-op handler (provides nothing user-visible).
- The asset-ingestion path is itself a Phase 4 / Phase 8 concern (cad-projection invalidation behavior on a foreign mesh isn't covered by the existing `cad-projection` tests). A foreign-mesh-into-editor dispatch would need its own preflight independent of the CommandBus-context question.
- The 312-test surface on `editor-*` provides good regression coverage for the *internals*; the gaps above are pure *external surface* gaps. Adding more `editor-*` unit tests would not move this preflight's headline numbers.
- Reproducer for the consumer inventory (read-only grep; no harness in-tree):
  ```
  # editor-shell call sites of io-* loaders (expect zero today):
  rg "io_gltf::|io_image::|io_3mf::" crates/editor-shell crates/editor-ui editor/rge-editor --type rust
  # KeyboardInput branches in editor-shell window_event (expect zero non-catch-all):
  rg "KeyboardInput|key_code|virtual_keycode" crates/editor-shell/src/lifecycle.rs
  # CommandBus::submit call sites from editor-shell / editor-ui (expect zero):
  rg "CommandBus|\.submit\(|editor_actions::" crates/editor-shell crates/editor-ui editor/rge-editor --type rust
  # File-extension string literals (expect zero in editor surface today):
  rg "\.rge\"|\.rgeproj\"|\.scene\"|\.project\"" crates/editor-shell crates/editor-ui editor/rge-editor --type rust
  ```
- This preflight is read-only and complementary to (not a replacement for) the Phase 5.3 PIE-round-trip baseline, the W10 workspace-round-trip baseline, and the §6.3 Gate A render-performance baseline already in this doc. Those entries own the *substrate-closure* baselines; this entry owns the **user-loop adoption** baseline and the two-arm revisit trigger. They should be re-read together when either trigger fires.

---

## Live-inspector wiring preflight (Phase 9)

**Budget anchors and gate references:**

- IMPLEMENTATION.md Phase 9 §9 (line 600): "Editor usability — friction points from real authoring."
- IMPLEMENTATION.md Phase 5 §5.1 (line 374): `editor-shell` — winit + lifecycle + PIE.
- This entry is a follow-up to two earlier Phase 9 preflights also in this doc: the **Editor-usability preflight** (cataloged the editor's user-facing gaps) and the **CommandBus integration design preflight** (decided the bus stays World-only). Both feed into the question: "now that an inspector widget + headless snapshot exist, how does it get rendered?"
- Companion commits in this dispatch chain (all on `origin/main`): `e3f6d27` (added headless `InspectorSnapshot` model + `EditorShell::inspector_snapshot()` accessor), `1d4ddbc` (added `editor-ui::widgets::inspector::{inspector_lines, ui}` over `&rge_editor_state::InspectorSnapshot`, moved the snapshot struct to `editor-state` so both crates share it without forcing either to depend on the other).

**This entry is a Phase 9 PREFLIGHT — pure read-only audit of the editor's egui host integration status.** It does NOT change source / tests / Cargo / lints. It establishes the negative finding that **no egui host exists in the workspace today**, names the blocker explicitly so future agents do not attempt fake live-wiring, and recommends the next read-only dispatch (egui host integration preflight) rather than a code dispatch.

**Methodology (read-only):**

1. Grep across the workspace (excluding `target/`, `OLD/`, `worktrees/`, `.claude/`, `.ai/`, `ai_handoffs/`) for `egui::Context::new`, `egui::Context::default`, `Context::run`, `egui_dock::DockArea::new`, `DockArea::show`, `egui_wgpu::Renderer`, `egui_winit::State`.
2. Read `editor/rge-editor/src/main.rs` + `Cargo.toml` end-to-end.
3. Read `crates/editor-shell/src/lifecycle/mod.rs::window_event` + `render_path.rs::render_frame_to_target` end-to-end.
4. Read `crates/editor-ui/src/dock/{mod.rs, spawner_registry.rs, tab_manager.rs}` and `widgets/{inspector.rs, node_graph.rs}` to confirm the egui consumer surface.
5. Cross-check workspace `Cargo.toml` for declared-but-unused `egui-*` workspace deps.

### 2026-05-28 - Live-inspector wiring preflight reconciliation (post-Dispatch-F + #249)

**Forward-only snapshot — the 2026-05-21 subsection below is preserved byte-identical as dated history.** This 2026-05-28 entry records that the named blocker the 2026-05-21 entry called "no egui host" no longer exists, and that the residual `--scene` no-window gap surfaced by the ISSUE-247 audit was closed by ISSUE-249 at main commit `007635d`. The pattern (prepend a dated forward snapshot above the prior dated snapshot, do not rewrite history) follows the precedent set by ISSUE-243 and ISSUE-245.

**Headline current reality (post-Dispatch-F + #249):**

- **The egui host exists.** `crates/editor-egui-host` is a workspace member at Dispatch F, with `EguiHost`, `InspectorHandoff`, `EditorTabViewer`, `egui_dock::DockState` carrying `TabBody`, and `ViewportRectSink` shipped through Dispatches A, B, C, D, and F (see closure evidence below).
- **`InspectorHandoff` is the chosen and shipped Option C delivery substrate.** `rge_editor_egui_host::InspectorHandoff` mirrors the canonical `RenderHandoff` latest-only pattern (`Mutex<Option<Arc<T>>>` + generation counter), exactly the "handoff substrate" shape captured as Option C in the 2026-05-21 A/B/C/D table below. A/B/D were not pursued.
- **`--scene` now produces a visible window with egui dock and Inspector chrome painted.** ISSUE-249, landed on main at `007635d`, constructs the window, surface, `EguiHost`, and `InspectorHandoff` unconditionally in `init_render_state`; `render_frame` has an egui-only branch for world-only launches that publishes the inspector snapshot, paints the dock, submits, and presents.

**Stale 2026-05-21 findings (recorded by line number for traceability; not edited in place):**

- `plans/BASELINE.md:652` — "no egui host exists in the workspace today". **STALE.** Superseded: `crates/editor-egui-host` exists as a workspace member at Dispatch F (see closure evidence).
- `plans/BASELINE.md:672` — "egui host — DOES NOT EXIST". **STALE.** Superseded: `EguiHost` is shipped in `crates/editor-egui-host/src/lib.rs` and constructed by `editor-shell::render_path` per frame.
- `plans/BASELINE.md:673` — "Snapshot-delivery substrate — DOES NOT EXIST". **STALE.** Superseded: `InspectorHandoff` ships in `rge_editor_egui_host::handoff` and is the chosen Option C substrate.
- `plans/BASELINE.md:676` — "Headline finding: NOT READY ... no egui host". **STALE.** The named blocker is removed; the inspector ecosystem is now wired end-to-end through editor-shell → editor-egui-host → editor-ui.
- `plans/BASELINE.md:684` — "`crates/editor-shell/Cargo.toml` declares no egui dep". **STALE.** Superseded: `crates/editor-shell/Cargo.toml:26-32` declares the `editor-shell → rge-editor-egui-host` dependency edge directly, which transitively pulls the egui pins. The host crate's own `[dependencies]` consume the workspace `egui`, `egui-winit`, `egui-wgpu`, and `egui_dock` pins.
- `plans/BASELINE.md:686` — "`egui-winit` and `egui-wgpu` ... referenced by no crate". **STALE.** Superseded: both are consumed by `crates/editor-egui-host/Cargo.toml:28-29`.
- `plans/BASELINE.md:687` — "no post-cuboid UI pass". **STALE.** Superseded: `editor-shell::render_path` now drives an egui pass on every frame (the cuboid path and the egui-only path both call `EguiHost::render` between geometry/clear and `queue.submit()`).
- `plans/BASELINE.md:688` — "No UI pass between or after". **STALE.** Same supersession as `:687`.
- `plans/BASELINE.md:716+` — "Recommended next dispatch: egui host integration preflight" recommendation block (lines 716-734). **STALE as a forward recommendation.** The egui host integration preflight already ran, Dispatches A/F shipped the host, the ISSUE-247 audit closed Q3/Q4 with explicit verdicts, and ISSUE-249 closed the residual `--scene` no-window gap. The recommendation block is preserved verbatim as a dated artifact of how the call was framed on 2026-05-21.

**Explicit preserve list — the following 2026-05-21 material remains useful as history and is NOT marked stale:**

- `plans/BASELINE.md:689` — the W03 egui-stripping markers. The historical record that W03 consciously deferred host integration to a later wave remains accurate and load-bearing for anyone reading the `editor-shell::lifecycle::mod.rs:18` / `:810` comments today.
- `plans/BASELINE.md:741` — the workspace egui dependency-pin observation (`egui` 0.34 / `egui-winit` 0.34 / `egui-wgpu` 0.34 / `egui_dock` 0.19). The pins are still the production pins; they are now consumed by `editor-egui-host` and `editor-ui` instead of being workspace-only forward-looking declarations.
- `plans/BASELINE.md:692-701` — the A/B/C/D delivery-options table. Recorded as a historical design-space record: **Option C (handoff substrate) is the chosen and shipped path.** The table itself is preserved verbatim so a future reader can see what alternatives were considered and why C was selected.
- `plans/BASELINE.md:738` — the 11 plus 14 headless test-count inventory for `inspector_snapshot_smoke.rs` + `inspector_widget_smoke.rs`. Those tests still exist and still pin the producer + formatter + render-fn contracts; this dispatch does not re-measure them but does not invalidate them either.

**Closure evidence (grounded at main commit `007635d`):**

- `crates/editor-egui-host/Cargo.toml:1-39` — the crate exists, is published as `rge-editor-egui-host`, and consumes the workspace `egui` / `egui-winit` / `egui-wgpu` / `egui_dock` pins plus `wgpu` / `winit`. This directly closes the stale `:686` claim.
- `crates/editor-egui-host/src/lib.rs:1-100` — the Dispatch A/F arc is documented in the crate doc-comment: Dispatch A scaffold (`EguiHost` struct + constructor + input adapter + resize hook), Dispatch B render pass, Dispatch C `InspectorHandoff` + `TabBody` / `EditorTabViewer` + `DockState<TabBody>` + `EguiHost::inspector_handoff`, Dispatch D split dock layout (Viewport + Inspector panes), Dispatch F `ViewportRectSink` for face-pick routing. This closes the stale `:672` / `:673` claims.
- `crates/editor-shell/Cargo.toml:26-32` — the `editor-shell → rge-editor-egui-host` dependency edge is declared, with an inline comment recording that the reverse edge would create a cycle and is forbidden. This directly supersedes the stale `:684` "no egui dep" claim.
- `crates/editor-shell/src/render_path.rs:279-285` — post-ISSUE-249, the `has_cad_scene || has_prebuilt_mesh` guard gates only Phase 2 render-state setup (`init_render_state_post_surface`); the empty-world branch stashes `gfx_ctx` ourselves so the EguiHost construction below (and the egui-only `render_frame` path) can read `self.gfx_ctx`. This closes the `--scene` no-window gap the ISSUE-247 audit identified.
- `crates/editor-shell/src/render_path.rs:313-328` — `EguiHost::new(device, surface_format, depth_format=None, msaa_samples=1, window, ViewportId::ROOT)` construction and `self.inspector_handoff = Some(Arc::clone(host.inspector_handoff()))` stash, performed unconditionally when `gfx_ctx + surface_ctx + window` are all present. The host and the editor-shell-side handoff clone point at the same underlying slot — the publish/acquire pair is the live Dispatch C wire.
- `crates/editor-shell/src/render_path.rs:510-610` — the egui-only `render_frame_egui_only` branch added by ISSUE-249: acquire surface, clear pass with `DEFAULT_CLEAR`, publish a fresh `InspectorSnapshot` via the handoff, call `EguiHost::render` (egui-winit `take_egui_input` + `Context::run` + `egui-wgpu` paint into the same encoder), submit, present, request next redraw. This is the painted egui frame the `--scene` no-window path now produces. It closes the stale `:687` / `:688` "no post-cuboid UI pass" claims for the world-only launch shape.
- Main commit reference: `007635d` (ISSUE-249 merge to main). All file/line refs above are stable at that commit; the orchestrator dispatch worktree was branched from it.

**Out-of-scope (scope-bounding mention, not reconciled here):** the Editor-usability preflight at `plans/BASELINE.md:588-592` concerning `open_file`, `load_project`, and `save_project` is partially stale post-ISSUE-225 and more stale post-ISSUE-249, but reconciling that section is out of scope for ISSUE-251 and belongs in a separate hygiene dispatch. This 2026-05-28 entry only reconciles the live-inspector wiring preflight (Phase 9).

---

### 2026-05-21 — initial egui-host status snapshot (recorder host, Rust 1.92.0)

**Inspector ecosystem state — what exists today:**

| Component | Status | Citation |
|---|---|---|
| **Producer** — `EditorShell::inspector_snapshot()` | ✅ exists | `crates/editor-shell/src/lifecycle/mod.rs::inspector_snapshot()` (per `e3f6d27`); 11 headless tests in `tests/inspector_snapshot_smoke.rs` |
| **Shared data type** — `rge_editor_state::InspectorSnapshot` | ✅ exists | `crates/editor-state/src/inspector_snapshot.rs` (per `1d4ddbc`); flat `Copy` struct with 10 fields; re-exported as `editor_shell::InspectorSnapshot` |
| **Pure formatter** — `inspector_lines(&InspectorSnapshot) -> Vec<(String, String)>` | ✅ exists | `crates/editor-ui/src/widgets/inspector.rs` (per `1d4ddbc`); 14 headless tests in `tests/inspector_widget_smoke.rs` |
| **egui render fn** — `ui(&InspectorSnapshot, &mut egui::Ui)` | ✅ exists | `crates/editor-ui/src/widgets/inspector.rs::ui` (per `1d4ddbc`) |
| **egui host** — `egui::Context` + `egui_winit::State` + `egui_wgpu::Renderer` driving frames | ❌ DOES NOT EXIST | Zero matches across the workspace |
| **Snapshot-delivery substrate** — mechanism to thread `InspectorSnapshot` to a rendering tab body per frame | ❌ DOES NOT EXIST | Depends on host; no design decision today |
| **Spawner wire-up** — `"tab/inspector"` → real Inspector tab body | ❌ DOES NOT EXIST | `crates/editor-ui/src/dock/spawner_registry.rs:165-169` continues to register `PlaceholderTabBody` for every default tab id including `"tab/inspector"` |

**Headline finding: NOT READY for live inspector-tab wiring. Named blocker: no egui host.**

**Evidence — every component a "live wiring" dispatch would need is absent:**

- **Zero `egui::Context` construction anywhere in production code.** Grep of `egui::Context::new`, `egui::Context::default`, `Context::run` returned zero matches outside `target/` / `OLD/` / `worktrees/`.
- **Zero `egui_dock::DockArea::show` (or any DockArea constructor) call sites.** The only `DockState` construction is `crates/editor-ui/src/dock/tab_manager.rs:219` — a state container builder inside `LayoutBlueprint::into_dock_state_with`, NOT a renderer host.
- **Zero `egui_winit::State` adapter usage.** No code routes winit `WindowEvent` to egui input.
- **Zero `egui_wgpu::Renderer` integration.** No code performs the egui GPU render pass.
- **`crates/editor-shell/Cargo.toml`** declares no egui dep of any kind (verified by reading lines 19-63): the production deps are `rge-editor-state`, `rge-editor-actions`, `rge-kernel-ecs`, `rge-input`, `rge-cad-projection`, `rge-gfx`, `rge-brep-render`, `rge-cad-core`, plus the external `winit`, `tracing`, `glam`, `wgpu`, optional `serde`/`ron` (`fixture-ron` feature).
- **`editor/rge-editor/Cargo.toml`** declares no egui dep of any kind: the production deps are `rge-editor-shell`, `rge-cad-core`, `rge-cad-projection`, `rge-kernel-ecs`, plus `winit`, `tracing-subscriber`.
- **Workspace `Cargo.toml`** does pin `egui = "0.34"`, `egui-winit = "0.34"`, `egui-wgpu = "0.34"`, `egui_dock = "0.19"` — but only `editor-ui` consumes `egui` + `egui_dock`, and only as widget-substrate (`&mut egui::Ui` consumer pattern). `egui-winit` and `egui-wgpu` are declared in the workspace `[workspace.dependencies]` table but **referenced by no crate**.
- **`editor-shell::render_path::render_frame_to_target`** (`crates/editor-shell/src/render_path.rs:471-582`) clears the surface, sets the lit-mesh pipeline + camera/light/material bind groups, encodes one cuboid `draw_indexed`, optionally encodes a second `draw_indexed` for the sub-ε highlight overlay, then closes the pass and calls `gfx_ctx.queue().submit()`. There is **no post-cuboid UI pass**.
- **`editor-shell::lifecycle::window_event::WindowEvent::RedrawRequested`** (the per-frame entry point) ticks game systems via `tick_redraw`, acquires the render-input snapshot via `RenderHandoff::acquire`, and calls `render_frame()`. No UI pass between or after.
- **The egui-stripping was deliberate, not a stub.** `crates/editor-shell/src/lifecycle/mod.rs:18` documents verbatim: *"The original rustforge file pulls in wgpu device/queue/pipeline state and **an egui overlay**; W03 strips those out (gfx wave W21+ owns wgpu) and keeps only the lifecycle skeleton + PIE plumbing."* And `:810`: *"egui-overlay routing + IR-rebuild + close-persist stripped."* These are historical markers indicating the W03 refactor consciously deferred the host integration to a later wave.
- **Inspector widget render fn is callable from nothing today.** `editor-ui::widgets::inspector::ui(&InspectorSnapshot, &mut egui::Ui)` requires a `&mut egui::Ui` scope. No production code obtains one. The widget is structurally unreachable until the host materializes.

**Snapshot-delivery options (academic until the host exists):**

| Option | Shape | When right |
|---|---|---|
| **A — captured closure** | `Arc<dyn Fn() -> InspectorSnapshot + Send + Sync>` registered with spawner; widget pulls per frame | Conceptually clean but blocked by `EditorShell`'s non-`Sync` ownership of winit `Window` + wgpu state |
| **B — shared slot** | `Arc<RwLock<InspectorSnapshot>>` — sim writes per tick, widget reads per frame | Pragmatic if single-threaded host suffices; matches a simple publish/subscribe-per-frame pattern |
| **C — handoff substrate** | Mirror `editor-shell::render_input::RenderHandoff` per ADR-117: `Mutex<Option<Arc<T>>>` + `AtomicU64` generation counter; latest-only | Right answer if the editor grows toward dedicated render thread; precedent + tests already exist |
| **D — static snapshot in tab body** | `pub struct InspectorTabBody { snapshot: InspectorSnapshot }`; host rebuilds tab body each frame or mutates field | Toy demo only; stale-by-construction; collapses to A/B/C the moment refresh is required |

**Recommendation (when the host materializes):** Option C (handoff substrate) — matches the existing `RenderHandoff` pattern in editor-shell, future-proofs for multi-threaded render. Option B is acceptable if simpler suffices and multi-threading is deferred. Option A is blocked today by ownership; Option D is not a real option.

**This comparison is recorded for the future host-design dispatch — picking a delivery mechanism without a host to consume it would be premature.**

**Status:** **PHASE 9 PREFLIGHT — inspector ecosystem 4 of 7 components ready (producer + type + formatter + renderer); 3 missing (host + delivery substrate + spawner wire-up). Defer all live-wiring dispatches until the egui host integration preflight settles the host design.**

**Explicit rejections — what NOT to dispatch next:**

1. **NOT** an `InspectorTabBody { snapshot: InspectorSnapshot }` wrapper added to editor-ui's spawner registry. That is the Option D "static tab body" — scaffolding for a non-existent host. The widget already takes `&InspectorSnapshot` directly; wrapping the snapshot in a tab body adds a layer with no real consumer and lies about progress toward live wiring.
2. **NOT** a snapshot-delivery substrate (RwLock slot, handoff) added to editor-shell before the host exists. Premature; the host's input-routing and render-pass design dictate which substrate fits.
3. **NOT** an `egui-*` dep added to editor-shell or rge-editor today. Both are doctrine-significant decisions (where does the host live? does editor-shell grow a UI subsystem? or is a new `editor-egui-host` crate the right home?) that belong in the host integration preflight, not in an incremental code dispatch.
4. **NOT** replacing `"tab/inspector"`'s `PlaceholderTabBody` registration with a stub `InspectorTab` returning `Default::default()` snapshots. Same reason — there is no host to spawn it, and producing a stub spawner without a host is sham progress.
5. **NOT** a `ShowInspector` menu Command variant added to `editor-ui::menus::Command` enum. The 23 existing Command variants have no menu handlers wired; adding a 24th without a host that resolves any of them is theater.
6. **NOT** an egui-rendering test added (e.g. via `egui::Context::run` constructing a headless context) — the goal of such a test would be to exercise the widget end-to-end, but the value depends on the host design (which Context configuration the production host uses), so a headless test pre-host would either be too generic to be useful or would lock in design choices not yet made.

**Recommended next dispatch:** **read-only `egui host integration preflight`.**

Scope of that future read-only dispatch (NOT this preflight's responsibility to land):

1. **Where the host lives** — `editor-shell` extension vs new `crates/editor-egui-host` between editor-shell and editor-ui vs `editor/rge-editor` binary-only host. Each has distinct implications for the editor-shell ↔ editor-ui dep direction.
2. **Input-routing semantics** — how `egui_winit::State::on_window_event` interacts with the existing Phase 9 `EditorKeyCommand` keyboard branch (Ctrl+Z/Y/S) in `lifecycle::window_event`. Decide ordering (egui-first vs game-first) and whether egui consumes events the bus would otherwise receive. Cite ADR if doctrine settled here.
3. **Render-pass composition** — same encoder vs separate submit; depth-buffer interaction with the existing depth attachment; queue-ordering with the cuboid + highlight overlay pass. The egui pass would slot **after** `render_path.rs:577` (end of highlight overlay encode) and **before** `:580` (`gfx_ctx.queue().submit()`).
4. **DockState ownership** — which crate holds `DockState<TabBody>`, what the `TabBody` enum looks like across `PlaceholderTabBody` / `NodeGraphTabBody` / `InspectorTabBody` / future widgets, how the spawner registry produces `TabBody` values that include InspectorSnapshot-aware widgets.
5. **Snapshot delivery mechanism** — pick from the A/B/C/D table above with explicit rationale tied to the chosen host architecture.
6. **Dep-edge implications** — confirm `forbidden-dep` doesn't fire on the new edges (verified academically: `forbidden-dep` Rule 6 forbids only RENDERER_CRATES → game-domain, none of the proposed edges trigger it); confirm `editor-state-ownership` Part B (forbidden-imports list at `tools/architecture-lints/src/editor_state_ownership.rs:71-102`) isn't triggered; confirm no cycle.
7. **Test strategy** — can the egui host be tested headlessly? `egui` has a render-to-pixels test pattern; `egui-wgpu` does not. What's a smallest end-to-end "render the inspector tab to an off-screen target and assert pixel content" test? Or is the integration only testable interactively?
8. **`resumed`-callback timing** — `egui_winit::State` needs the winit window; `egui_wgpu::Renderer` needs the wgpu device/queue/surface. Both are constructed in `editor-shell::lifecycle::resumed`. Confirm the egui host setup belongs in or immediately after that callback.

**Revisit triggers** — re-run THIS preflight (live-inspector wiring) when **either** of the following becomes true:

1. **The `egui host integration preflight` lands a decision** on where the host lives, how it threads input/render, and which snapshot-delivery option (A/B/C/D) is chosen. At that point the live-wiring dispatch becomes a bounded code dispatch consuming the host design.
2. **A non-inspector consumer of the egui host materializes first** — e.g. a menu-bar implementation, a viewport gizmo overlay, a status-bar dirty indicator. If any of those land before the inspector, the host substrate they require will already exist, and the inspector wiring becomes incremental rather than substrate-defining.

Until **at least one** of those fires, **defer all inspector live-wiring dispatches**. The 4-of-7 ready components (producer / type / formatter / renderer) sit ready for the moment the remaining 3 (host / delivery / spawner) become buildable, and adding more model/widget surface without a host would be carrying scaffolding for a structure that doesn't exist.

**Notes / caveats:**

- The 11 headless tests in `editor-shell/tests/inspector_snapshot_smoke.rs` + the 14 in `editor-ui/tests/inspector_widget_smoke.rs` continue to pin the producer + formatter + render-fn contracts even though no live rendering happens. They are not theater — they pin the API surface so the eventual host can consume them with confidence.
- This preflight does NOT propose shrinking, removing, or simplifying any existing inspector substrate. `InspectorSnapshot` / `inspector_snapshot()` / `inspector_lines()` / `widgets::inspector::ui` are all healthy and well-tested; the gap is purely the absence of an egui host to drive them.
- The historical comments at `crates/editor-shell/src/lifecycle/mod.rs:18` and `:810` should be retained — they are the most concise statement that the W03 refactor *intentionally* stripped egui from editor-shell as a separation-of-concerns move. Future agents should read those before considering whether to re-add egui to editor-shell directly.
- The four `egui-*` workspace dependency pins (`egui` 0.34 / `egui-winit` 0.34 / `egui-wgpu` 0.34 / `egui_dock` 0.19) in the root `Cargo.toml` are correct as-is; the host integration preflight will decide which crates consume them. Pinning them in the workspace is forward-looking, not stale.
- Reproducer for the empty-host finding (read-only grep; no harness in-tree):
  ```
  # production egui::Context construction sites (expect zero):
  rg "egui::Context::new\(|egui::Context::default\(|Context::run\(" --type rust
  # production DockArea::show call sites (expect zero):
  rg "DockArea::(new|show|style)" --type rust
  # egui-winit + egui-wgpu integration (expect zero):
  rg "egui_winit::State|egui_wgpu::Renderer" --type rust
  # editor-shell + rge-editor egui deps (expect zero matches):
  rg "egui" crates/editor-shell/Cargo.toml editor/rge-editor/Cargo.toml
  ```
- This preflight is read-only and complementary to the Editor-usability preflight + the CommandBus integration design preflight already in this doc. Those entries own the substrate-readiness inventory; this entry owns the **host-readiness** baseline for the inspector ecosystem specifically and the named-blocker recommendation. They should be re-read together when the egui host integration preflight is dispatched.
