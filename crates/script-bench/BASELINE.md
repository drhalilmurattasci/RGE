# `rge-script-bench` baseline (native baseline + script-host hot-reload gate)

> **Re-recorded 2026-05-11 post toolchain bump (cargo 1.78 → 1.94, wasmtime 23 → 44).**
> All native-rust rows below are re-measured under the current workspace toolchain.
> The formal Phase 3.3/3.4 gate rows are likewise re-recorded on the current host.
> Gate verdicts (Phase 3.3 hot-reload p95 < 100 ms; Phase 3.4 ECS-via-WASM ratio ≤ 1.5×)
> remain **PASS**; see "Halt-on-regression delta" note in §3 below for the bench-refresh
> dispatch's flagged movement on the ECS ratio (0.97–1.06× → 1.21×, in-gate).

Status: **script-host gate wired**. Native baseline rows remain the denominator
for the "1.5x of native" claim. The real `rge-script-host` Counter fixture now
backs the formal Phase 3 hot-reload gate: 1,000 entities preserved across 100
consecutive swap cycles, with an opt-in one-hour memory soak.

This file records the reference numbers for every workload defined in
[`src/workloads.rs`](src/workloads.rs) when run on the
**native-Rust baseline** (`src/native_baseline.rs`). All later "engine X is
1.5x of native" claims are computed against the values here on the same host.

See [METHODOLOGY.md](METHODOLOGY.md) for what each row means and how to
reproduce it.

## Workload roster

| id  | name                          | native-Rust kernel                                                             |
| --- | ----------------------------- | ------------------------------------------------------------------------------ |
| W1  | `script_tick_1m_iters`        | tight loop: `Transform.translation += dt * Transform.velocity`, 1M iterations  |
| W2  | `per_frame_tick_10k_entities` | one frame over 10k entities, integration kernel applied once each              |
| W3  | `cold_start`                  | empty-closure timer floor (no native module-load step exists)                  |
| W4  | `hot_reload_swap`             | native closure swap plus real `script-host` 1000-entity / 100-cycle gate       |
| W5  | `memory_overhead`             | `size_of::<fn(&mut Transform)>()` (function-pointer cost)                      |

## Baseline results

The numbers below are the **current-run record** for the host where
`cargo bench -p rge-script-bench` was last executed. Re-runs on the same
host should land within ±5% of these values — that's the "no regressions"
exit criterion.

Re-recorded 2026-05-11 on a Windows 11 / x86_64 dev box, `cargo 1.94.1`,
`wasmtime 44.0.1`, `[profile.bench]` defaults (LTO=thin, opt-level=3,
codegen-units=1). Point estimates are the `mean.point_estimate` field from
each criterion `new/estimates.json`.

| workload                       | engine        | metric            | unit             | value     | samples | prior (cargo 1.78) |
| ------------------------------ | ------------- | ----------------- | ---------------- | --------- | ------- | ------------------ |
| `script_tick_1m_iters`         | `native_rust` | wall_time         | ns total / 1M op | 674 666   | 100     | 668 000            |
| `per_frame_tick_10k_entities`  | `native_rust` | wall_time         | ns total / 10k   | 7 594     | 100     | 8 102              |
| `cold_start`                   | `native_rust` | wall_time         | ns               | 48.74     | 50      | 50.8               |
| `hot_reload_swap`              | `native_rust` | wall_time_total   | ns / 100 cycles  | 107.25    | 50      | 110.6              |
| `memory_overhead`              | `native_rust` | wall_time_per_load | ns               | 0.911     | 50      | 1.28               |
| `memory_overhead`              | `native_rust` | bytes_per_module  | bytes            | 8         | n/a     | 8                  |

Per-op derivations (current):

- `script_tick_1m_iters` — 674 666 ns / 1 000 000 = **0.675 ns/op** (~1.48 Gelem/s).
- `per_frame_tick_10k_entities` — 7 594 ns / 10 000 = **0.76 ns/op** (~1.32 Gelem/s).

W1/W2/W3/W4/W5 native-rust deltas vs prior recording (cargo 1.78 / wasmtime 23):

- W1 +1.0% (within ±5% noise band)
- W2 −6.3% (improvement, marginally past the ±5% noise band — flag as net-faster)
- W3 −4.1% (improvement, within band)
- W4 −3.0% (improvement, within band)
- W5 −28.8% (improvement, well outside band — confidence interval point estimate
  per-load shrank from 1.28 ns to ~0.91 ns; criterion flagged "performance
  improved" with p<0.05 vs prior saved baseline)

None of the deltas trip the "no-regressions" ±5% band as regressions; all are
either within noise or improvements.

## Formal script-host hot-reload gate (Phase 3.3)

Re-recorded 2026-05-11 (release-profile test run) via:

```sh
cargo test -p rge-script-bench --release \
  script_host::tests::formal_100_cycle_preservation_gate_uses_1000_entities \
  -- --nocapture
```

| workload | engine | scene | cycles | metric | value |
| --- | --- | --- | --- | --- | --- |
| `hot_reload_swap` | `script_host_counter` | 1,000 `Counter` entities | 100 | p95 swap window | **0.796 ms** |
| `hot_reload_swap` | `script_host_counter` | 1,000 `Counter` entities | 100 | max swap window | **1.120 ms** |
| `hot_reload_swap` | `script_host_counter` | 1,000 `Counter` entities | 100 | avg swap window | **0.738 ms** |

**Re-validation 2026-05-12** (current main HEAD, same release-profile command + host as 2026-05-11; single-run point estimate):

| workload | engine | scene | cycles | metric | value | delta vs 2026-05-11 |
| --- | --- | --- | --- | --- | --- | --- |
| `hot_reload_swap` | `script_host_counter` | 1,000 `Counter` entities | 100 | p95 swap window | **0.818 ms** | +2.8% (within ±5% noise band) |
| `hot_reload_swap` | `script_host_counter` | 1,000 `Counter` entities | 100 | max swap window | **0.982 ms** | −12.3% (improvement) |
| `hot_reload_swap` | `script_host_counter` | 1,000 `Counter` entities | 100 | avg swap window | **0.793 ms** | +7.5% (slightly outside band, in-gate by a wide margin) |

Re-validation gate verdict: **PASS** against PLAN §5.6's <100 ms p95 budget — 0.818 / 100 ≈ 0.8% of budget (unchanged headroom). The p95 movement (+2.8%) is within the documented ±5% noise band and triggers no halt-on-regression action. The avg movement (+7.5%) is just outside the band but still ~125× under budget; flagged for transparency, not for action.

The p95 gate is **PASS** against PLAN §5.6's <100 ms budget — by a wide margin
(0.8 ms / 100 ms ≈ 0.8% of budget). Prior recording on cargo 1.78 / wasmtime 23
was p95=9.761 ms, max=10.868 ms, avg=7.992 ms; the wasmtime 23 → 44 toolchain
bump appears to be the dominant driver of the ~12× p95 reduction. The test
poisons all Counter components between capture and restore on every cycle, so
the preservation assertion exercises the restore path rather than unchanged
state. The one-hour memory-soak gate is compiled but ignored by default; run
`script_host::tests::phase_3_memory_soak_one_hour` with `--ignored` when a
release-readiness soak is desired. **As of 2026-05-12 the 1-hour soak remains harness-wired but UNRUN** — release-readiness/CI deferral preserved per HANDOFF.md (2026-05-11 dispatch flag "one-hour memory soak DEFERRED to release-readiness CI job"); today's docs-only re-validation explicitly does NOT certify Phase 3.4 exit criterion #3 (1-hour session without memory leak), only criteria #1 / #2 / #4 are re-validated here.

Additional criterion-captured row for the 1000-entity / 100-cycle swap window
(end-to-end, not just p95 — recorded by the `hot_reload_swap` bench group):

| workload | engine | scene | cycles | metric | value |
| --- | --- | --- | --- | --- | --- |
| `hot_reload_swap` | `script_host_counter_1000x100` | 1,000 `Counter` entities | 100 | criterion mean | 87.6 ms |
| `hot_reload_swap` | `script_host_counter_1000x100` | 1,000 `Counter` entities | 100 | criterion median | 86.7 ms |

This is the full 100-cycle window time (wall-clock for the 100 swaps including
setup overhead, not the per-cycle p95). The 0.796 ms p95 row above is the
load-bearing gate row per PLAN §5.6.

## Formal Phase 3.4 ECS-via-WASM ratio gate (bulk-path substrate)

Re-recorded 2026-05-11 (release-profile test run) via:

```sh
cargo test -p rge-script-bench --release \
  script_host::tests::phase_3_4_ecs_via_wasm_ratio_meets_gate \
  -- --nocapture
```

| workload | engine | scene | frames | metric | value |
| --- | --- | --- | --- | --- | --- |
| `ecs_iteration_ratio` | `script_host_counter_bulk` | 1,000 `Counter` entities | 10 | native per-frame avg | **~81 µs** |
| `ecs_iteration_ratio` | `script_host_counter_bulk` | 1,000 `Counter` entities | 10 | wasm per-frame avg | **~98 µs** |
| `ecs_iteration_ratio` | `script_host_counter_bulk` | 1,000 `Counter` entities | 10 | `wasm_total / native_total` | **~1.21× (≤ 1.5× gate ASSERTED)** |

**Re-validation 2026-05-12** (current main HEAD, same release-profile command + host as 2026-05-11; single-run point estimate):

| workload | engine | scene | frames | metric | value | delta vs 2026-05-11 |
| --- | --- | --- | --- | --- | --- | --- |
| `ecs_iteration_ratio` | `script_host_counter_bulk` | 1,000 `Counter` entities | 10 | native per-frame avg | **~67.93 µs** | −16.1% (native got faster) |
| `ecs_iteration_ratio` | `script_host_counter_bulk` | 1,000 `Counter` entities | 10 | wasm per-frame avg | **~90.82 µs** | −7.3% (wasm got faster but less than native) |
| `ecs_iteration_ratio` | `script_host_counter_bulk` | 1,000 `Counter` entities | 10 | `wasm_total / native_total` | **~1.34× (≤ 1.5× gate ASSERTED)** | +10.7% (1.21× → 1.34×; in-gate, drift flagged) |

Re-validation gate verdict: **PASS** against the ≤1.5× formal gate — 1.34 / 1.5 ≈ 89% of budget (vs prior 81%). The ratio movement (1.21× → 1.34×) is OUTSIDE the ±5% noise band but stays IN-GATE; per the halt-on-regression protocol below ("if numbers are WORSE than previous recording but still WITHIN gate, proceed but flag the delta") this re-validation flags the delta WITHOUT halting. The mechanical cause: native_per_frame improved ~16% while wasm_per_frame improved only ~7% — the host got faster at native more than at wasm, expanding the relative WASM penalty. Both prior and current measurements are single-run point estimates, so per-run noise contributes to the apparent movement. The bulk-path substrate is unchanged; no architecture regression.

**Bench-refresh delta flagged**: the prior recording oscillated in the 0.97×–1.06×
band (median 1.00×) under cargo 1.78 / wasmtime 23. The current recording lands
at 1.21×. This is **a measurement-time regression vs the prior baseline** but
**stays within the formal ≤1.5× gate** with comfortable headroom (1.21 / 1.5 ≈
81% of budget). Per the bench-refresh dispatch's halt-on-regression protocol —
"if numbers are WORSE than previous recording but still WITHIN gate, proceed
but flag the delta" — the dispatch flags this delta in `Status.md` for follow-up
attention without halting. The bulk-path substrate is unchanged; the most
plausible drivers are (a) wasmtime 23 → 44 internal-execution-path changes that
shifted the wasm/native ratio, and (b) per-run noise (single-run point estimate
without the prior 5-rerun re-recording band).

The bulk-path substrate is the gate's actual closure: each frame crosses the
wasm boundary exactly once (one `tick(dt)` call) and re-enters the host
exactly once (one `rge.ecs::add_to_all_counters(1)` host call), amortizing
the per-frame wasm-trampoline cost across all 1,000 entities. The per-entity
baseline of **2.17×** measured 2026-05-11 13:00 with `get_counter` /
`set_counter` host crossings once per entity per frame is preserved as the
historical record; it is no longer the live measurement.

The test asserts `report.ratio <= 1.5` directly. If a future substrate change
re-introduces per-entity boundary crossings, the assertion surfaces the
regression at the same gate.

> **Filling in the table.** After running `cargo bench -p rge-script-bench`,
> read `target/criterion/<group>/<name>/new/estimates.json` for each row and
> paste the `mean.point_estimate` (in nanoseconds) into the value column.
> This is intentionally manual at v0.0.1; the W04 follow-up wires automatic
> JSON aggregation through `src/output.rs`.

## Remaining engine rows (placeholder)

The table below is still the target comparison shape for future
Lua/mlua/Wasmer-singlepass/Bevy-extism comparisons. The `script_host_counter`
hot-reload gate above is the current real engine-backed row.

| workload                       | native_rust | wasmtime_cranelift | wasmtime_singlepass | mlua | wasmer_singlepass | bevy_extism |
| ------------------------------ | ----------- | ------------------ | ------------------- | ---- | ----------------- | ----------- |
| `script_tick_1m_iters`         | _baseline_  | _pending W04_      | _pending W04_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `per_frame_tick_10k_entities`  | _baseline_  | _pending W04_      | _pending W04_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `cold_start`                   | 0 ns *      | _pending W04_      | _pending W04_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `hot_reload_swap`              | _baseline_  | _script-host gate wired_ | _pending_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `memory_overhead`              | 8 B *       | _pending W04_      | _pending W04_       | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |

\* Native code has no module-load step and no per-module heap allocation;
the values shown are the formal lower bounds. See METHODOLOGY for why
this is fair.

## Targets to defend (per PLAN.md §5.6)

- `per_frame_tick_10k_entities` (engine) ≤ **1.5×** native row.
- `script_tick_1m_iters` (engine) ≤ **1.5×** native row.
- `cold_start` (engine) < **50 ms**.
- `hot_reload_swap` (engine, p95) < **100 ms**.
- `memory_overhead` (engine) < **1 MB** per module.

## Reproducing this file

```sh
# from RGE workspace root
cargo bench -p rge-script-bench
# Reads target/criterion/**/new/estimates.json for each group/function and
# updates the native rows manually.

cargo test -p rge-script-bench \
  script_host::tests::formal_100_cycle_preservation_gate_uses_1000_entities \
  -- --nocapture
# Updates the formal script-host hot-reload gate rows.
```

Methodology, including `--save-baseline`/`--baseline` flow and CI ratchet,
is in [METHODOLOGY.md](METHODOLOGY.md).
