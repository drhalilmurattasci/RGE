# `rge-script-bench` methodology

> Companion to [BASELINE.md](BASELINE.md). Defines what each workload measures,
> how it is constructed, and how to reproduce the published numbers. Per
> [PLAN.md §14](../../plans/PLAN.md), the published methodology is the
> defence against cherry-picked counter-benchmarks against the "fastest
> script engine" pillar (PLAN.md §5.6).

## Versioning

This document is versioned implicitly by [`src/workloads.rs`](src/workloads.rs)
constants and [`src/output.rs::SCHEMA_VERSION`](src/output.rs). Bumping a
workload constant (iteration count, entity count, seed, `FIXED_DT`) requires
a methodology bump and a `BASELINE.md` reset on every supported host.

| key                              | value                  | source                                      |
| -------------------------------- | ---------------------- | ------------------------------------------- |
| Schema version                   | 1                      | `output::SCHEMA_VERSION`                    |
| Iteration count (W1)             | 1,000,000              | `workloads::SCRIPT_TICK_ITERATIONS`         |
| Entity count (W2)                | 10,000                 | `workloads::PER_FRAME_ENTITY_COUNT`         |
| Formal hot-reload entities (W4)  | 1,000                  | `script_host::FORMAL_HOT_RELOAD_ENTITY_COUNT` |
| Hot-reload cycles (W4)           | 100                    | `workloads::HOT_RELOAD_CYCLES`              |
| Formal memory soak duration (W5) | 1 hour                 | `script_host::FORMAL_MEMORY_SOAK_DURATION`  |
| Fixed timestep                   | 1 / 60 s               | `workloads::FIXED_DT`                       |
| Entity-buffer seed               | 0x5247_452D_5732_3031  | `workloads::ENTITY_SEED`                    |

## Workload construction

### W1 — `script_tick_1m_iters`

**Kernel.** `Transform.translation += dt * Transform.velocity`, written as
`Transform::integrate(&mut self, dt: f32)`.

**Inputs.** A single `Transform` with `translation = (0, 0, 0)` and
`velocity = (1, 2, 3)`. Constants chosen so the loop body cannot be
constant-folded (compiler cannot prove the result without iterating).

**Loop.** Executed `SCRIPT_TICK_ITERATIONS = 1_000_000` times.

**What is measured.** Wall-clock nanoseconds for the full 1M-iteration loop
returned through `criterion::black_box` so the optimiser cannot eliminate
work.

**What it tells us.** Per-iteration arithmetic throughput in tightest-possible
form. An engine candidate with a per-tick guard, type-check, or sandbox
boundary will lose here proportionally to its overhead.

### W2 — `per_frame_tick_10k_entities`

**Kernel.** Same as W1.

**Inputs.** A `Vec<Transform>` of length `PER_FRAME_ENTITY_COUNT = 10_000`,
populated deterministically by `workloads::generate_entities(count, seed)`
using SplitMix64 with `seed = ENTITY_SEED`. Identical bytes on every host.

**Loop.** One pass over the buffer applying the integration kernel once per
entry. The buffer is owned by the bench harness and `clone`d into each
sample via `criterion::iter_batched(BatchSize::SmallInput)` so the work is
"one frame" each time.

**What is measured.** Wall-clock nanoseconds for one frame's traversal
(per-op throughput is then `time / 10_000`).

**What it tells us.** Realistic-shaped per-frame work for a typical scripted
component update. Engine FFI call cost shows up here.

### W3 — `cold_start`

**Native baseline.** Native Rust has no module-load step; the kernel is
`Instant::now() / black_box(()) / .elapsed()`. The published number is the
floor of measurement noise — the unavoidable overhead an engine cannot drop
below.

**Script-host version.** Construct a fresh `wasmtime::Engine`, compile the
canonical Counter fixture through `rge-script-host`, instantiate it, register
one Counter entity, run one tick, stop the timer.

**What is measured.** Wall-clock nanoseconds from "no engine yet" to "first
tick has returned". Single-shot per sample (`sample_size = 50`).

**Target.** < 50 ms (PLAN.md §5.6).

### W4 — `hot_reload_swap`

**Native baseline.** `Box<dyn Fn(&mut Transform)>` replaced with another
boxed closure, repeated `HOT_RELOAD_CYCLES = 100` times. The two closures
differ only in `dt` to defeat compiler dead-code elimination.

**Script-host version.** Build Counter module v1 and module v2 ahead of the
timer. The formal gate seeds 1,000 Counter-bearing ECS entities. In each of
100 cycles: capture the full Counter snapshot, poison all live counters,
drop the old instance, instantiate the alternate module version, restore the
snapshot, and verify the restored Counter sum. Poisoning ensures the restore
path is doing real work rather than observing unchanged world state.

**What is measured.** Per-cycle swap-window latency across 100 cycles. The
formal unit gate reports p95 directly; the Criterion row wraps the same
1000-entity / 100-cycle workload for repeatable benchmark comparisons.

**Target.** p95 < 100 ms (PLAN.md §5.6).

### W4b — `ecs_iteration_ratio` (Phase 3.4 ECS-via-WASM bulk-path gate)

**Native baseline.** `native_bulk_add_to_all_counters(world: &mut World, delta: i64) -> usize`
in `script_host.rs`: ONE `world.query::<Counter>()` scan collecting `(EntityId, new_value)` pairs
plus ONE `world.insert` pass writing the new values. Returns the count of components updated.
This mirrors the new `rge.ecs::add_to_all_counters` host function's algorithmic shape EXACTLY,
so the recorded ratio captures wasm-trampoline + host-call overhead in near-isolation rather
than algorithmic-cost asymmetry.

**Script-host version.** A new WAT fixture `crates/script-host/tests/fixtures/counter_bulk.wat`
exports a single `tick(dt: f32)` whose body issues exactly one `rge.ecs::add_to_all_counters(1)`
host call per frame and discards the result. The bench drives `module_bulk` ONCE (`instantiate`)
then `tick(FIXED_DT)` ONCE per frame — no per-entity `init_entity` loop, no per-entity tick. The
host function performs the same scan + insert work the native helper performs.

**Bulk vs per-entity boundary cost.** The earlier `phase_3_4_ecs_via_wasm_ratio_records_baseline`
measurement (recorded 2026-05-11 13:00 in the historical row) drove the wasm path through
`rge.ecs::{init_entity, get_counter, set_counter}` once per entity per frame, paying ~3,000 host
crossings per frame at 1,000 entities. Under the bulk substrate, each frame crosses the wasm
boundary exactly twice (one `tick`, one host fn). The amortization brings the recorded ratio
within measurement noise of 1.0×.

**Inputs.** Independent worlds (one per path); each seeded with 1,000 Counter-bearing ECS
entities via the same `seed_counter_world` helper used by the hot-reload gate. Starting Counter
values are deterministic per `index_to_counter_value(0, index)`.

**Loop.** `EcsIterationConfig::formal()` pins 1,000 entities × 10 frames. 10,000
host↔WASM transitions per per-entity measurement collapses to 20 transitions under the bulk
substrate; the per-frame trampoline cost is amortized across the 1,000-entity scan.

**Integrity check.** Final assertion: both worlds end with each Counter incremented by exactly
`frames` (= 10) and the recorded delta matches `frames * entity_count`. Validates both paths
performed the workload.

**What is measured.** `wasm_total / native_total`. Reported by `EcsIterationReport::ratio_pretty`
as e.g. `"0.99×"`.

**Target.** ≤ **1.5×** native. Asserted in `phase_3_4_ecs_via_wasm_ratio_meets_gate`.

### W5 — `memory_overhead`

**Native baseline.** `size_of::<fn(&mut Transform)>()` — the smallest
"loaded module" Rust can have. Reported in bytes via
`Criterion::iter_custom`, encoded as a `Duration::from_nanos(bytes)` to
ride the criterion timer infrastructure.

**Script-host soak gate.** `script_host::ScriptHostBench::memory_soak`
repeats the same 1000-entity preservation workload until a configured
wall-clock duration elapses. The formal one-hour gate is exposed as an
ignored test so ordinary workspace runs compile it without spending one hour.
As of the 2026-05-16 harness revision the soak also samples the host
process's memory footprint — at soak start, after each completed
hot-reload cycle, and at soak end — and folds those samples into a
`MemorySoakReport::process_memory` field (`Option<ProcessMemoryMetrics>`)
carrying `peak_rss_bytes` (largest observed resident sample), start/end
resident bytes, start/end virtual bytes, and `vss_delta_bytes` (the
end-minus-start virtual delta). On Windows the sampler reports the
process working set (resident) and commit charge (virtual); on Linux it
reports RSS and virtual size (VSZ); on platforms with no supported
sampler the field is `None` rather than a fabricated zero. The
underlying process-memory syscall is provided by the `memory-stats`
crate, kept local to `script-bench`.

**What is measured.** Default Criterion still records the native allocation
floor. The formal soak gate records successful cycles, restored components,
wall-clock duration, and — on supported platforms — observed process
peak-RSS and start-to-end memory deltas for leak observation.

**Target.** < 1 MB per module (PLAN.md §5.6).

## Anti-cheat rules

These rules apply to every engine candidate row added in later waves:

1. **Identical input bytes.** The entity buffer for W2 is generated by the
   *exact* SplitMix64 in `workloads::generate_entities`. Engines may not
   substitute their own RNG.
2. **Identical kernel.** `Transform::integrate` is the only permitted
   arithmetic. No SIMD swap, no FMA reorder, no precomputed table.
3. **No `#[inline(always)]` on the kernel.** Native baseline is an honest
   denominator: only the optimiser's own choices apply.
4. **No `unsafe` in either side.** Workspace lints forbid it; the bench
   crate inherits them via `[lints] workspace = true`.
5. **Same machine.** Engine numbers must be quoted alongside native numbers
   from the same `cargo bench` invocation. Never compare an engine number
   on host A to a native number on host B.
6. **Same compiler & profile.** `[profile.bench]` in the workspace root
   pins LTO=thin, opt-level=3, codegen-units=1.

## Reproducer instructions

### One-shot run

```sh
# From the RGE workspace root.
cargo bench -p rge-script-bench
```

This runs every `[[bench]]` declared in `crates/script-bench/Cargo.toml`:

- `script_tick_1m`        — W1 + W2 (one binary, two groups)
- `cold_start`            — W3
- `memory_overhead`       — W5
- `hot_reload_swap`       — W4

Criterion writes per-bench JSON estimates to
`target/criterion/<group>/<name>/new/estimates.json`.

### Formal Phase 3 hot-reload gate

```sh
cargo test -p rge-script-bench \
  script_host::tests::formal_100_cycle_preservation_gate_uses_1000_entities \
  -- --nocapture
```

This runs the real `rge-script-host` Counter swap protocol across 1,000
entities and 100 consecutive hot-reload cycles, then prints p95 / max / avg
swap-window latency.

### Formal Phase 3.4 ECS-via-WASM ratio gate

```sh
cargo test -p rge-script-bench \
  script_host::tests::phase_3_4_ecs_via_wasm_ratio_meets_gate \
  -- --nocapture
```

This drives the bulk-path substrate (`rge.ecs::add_to_all_counters` host fn +
`counter_bulk.wat` fixture) over 1,000 Counter entities × 10 frames, prints
`native_per_frame_us` / `wasm_per_frame_us` / `ratio`, and asserts `ratio <= 1.5`.
A regression that reintroduces per-entity boundary crossings will surface at
this gate directly.

### One-hour memory soak

```sh
cargo test -p rge-script-bench \
  script_host::tests::phase_3_memory_soak_one_hour \
  -- --ignored --nocapture
```

The soak is ignored by default because it intentionally runs for one hour.

### Stable-comparison flow (for "no regressions ±5%")

```sh
# Record a baseline named "main" once on a clean machine.
cargo bench -p rge-script-bench -- --save-baseline main

# Later, on the same machine:
cargo bench -p rge-script-bench -- --baseline main
# Criterion prints "x.x% faster" / "x.x% slower" deltas per row.
```

The exit criterion in `tasks/W20/PLAN.md` is **±5%** between adjacent runs
on the same host. Any row outside that band is a regression and the cause
must be tracked down before the run is published to `BASELINE.md`.

### CI ingestion (post-W20)

Future work (a `cargo run -p rge-script-bench --bin bench-collect`-style
front-end) will:

1. Run `cargo bench -p rge-script-bench --no-run` to compile.
2. Execute each bench binary with `--measurement-time` and `--noplot`
   tuned for CI runners.
3. Read every `target/criterion/**/estimates.json`.
4. Build a [`BenchReport`](src/output.rs) and emit JSON to
   `target/script-bench-report.json` plus the Markdown table for
   `BASELINE.md`.
5. Fail the build if any row drifts > 5% versus a stored baseline file
   (per PLAN.md §13.3 ratchet rule).

The `BenchReport` schema is already stable at `schema_version = 1` so the
CI front-end is a pure plumbing job.

## What is *not* measured (and why)

- **Cross-language FFI raw cost.** Subsumed by W2; isolating it would
  publish a number with no policy meaning.
- **JIT warmup separately from cold-start.** The engine spec for W3 fixes
  "cold start" to mean "first tick returned"; warmup is part of cold
  start by definition. A separate "steady-state warmed" metric can be
  added in a future schema bump if `BenchReport.metric` grows a new
  variant.
- **Engine compilation cost.** Counted within W3 if it occurs before the
  first tick. AOT-cached engines should report a separate row using
  `metric: "warm_start"` once the schema accommodates it.

## Change log

- **Memory-soak process-memory metrics** (2026-05-16) - `ScriptHostBench::memory_soak`
  now samples process resident / virtual memory (at start, after each completed
  cycle, and at end) into `MemorySoakReport::process_memory`
  (`Option<ProcessMemoryMetrics>` with `peak_rss_bytes` / `vss_delta_bytes`);
  the W5 methodology above is updated to describe it. No workload constant
  changed and the formal one-hour soak duration is unchanged, so this is not a
  methodology version bump.
- **v0.0.3 Phase 3.4 ratio gate closed** (2026-05-11) - bulk-path substrate
  added (`rge.ecs::add_to_all_counters` host fn + `counter_bulk.wat` fixture);
  `EcsIterationConfig` / `EcsIterationReport` / `ScriptHostBench::ecs_iteration_ratio`
  now drive the bulk shape; `phase_3_4_ecs_via_wasm_ratio_meets_gate` asserts
  `ratio <= 1.5` (recorded ~1.00× under the bulk substrate; per-entity baseline
  2.17× preserved as historical reference).
- **v0.0.2 gate wiring** - `script_host` harness added for the real
  `rge-script-host` Counter fixture: W3 cold-start row, W4 1000-entity /
  100-cycle preservation p95 gate, and W5 opt-in one-hour memory soak.
- **v0.0.1** (this wave) — initial scaffold; native-Rust baseline only;
  engine rows pending W04. JSON schema v1 frozen.
