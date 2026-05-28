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
release-readiness soak is desired.

Additional criterion-captured row for the 1000-entity / 100-cycle swap window
(end-to-end, not just p95 — recorded by the `hot_reload_swap` bench group):

| workload | engine | scene | cycles | metric | value |
| --- | --- | --- | --- | --- | --- |
| `hot_reload_swap` | `script_host_counter_1000x100` | 1,000 `Counter` entities | 100 | criterion mean | 87.6 ms |
| `hot_reload_swap` | `script_host_counter_1000x100` | 1,000 `Counter` entities | 100 | criterion median | 86.7 ms |

This is the full 100-cycle window time (wall-clock for the 100 swaps including
setup overhead, not the per-cycle p95). The 0.796 ms p95 row above is the
load-bearing gate row per PLAN §5.6.

## Formal 1-hour memory soak (Phase 3.4 exit criterion #3) — RUN 2026-05-12

Recorded 2026-05-12 (release-profile background test run) via:

```sh
cargo test -p rge-script-bench --release --lib \
  script_host::tests::phase_3_memory_soak_one_hour \
  --manifest-path A:\RCAD\RGE\Cargo.toml \
  -- --ignored --nocapture
```

| workload | engine | scene | minimum_duration | metric | value |
| --- | --- | --- | --- | --- | --- |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `report.elapsed` (cargo wall-clock) | **3600.00 s** |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `report.cycles > 0` assertion | **HELD** (test result `ok`) |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `report.restored_components == cycles * entity_count` assertion | **HELD** (test result `ok`) |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | process OOM / hang / panic | **none** (exit code 0) |

**Phase 3.4 exit criterion #3 status**: **CLOSED 2026-05-12 on recorder host only** — `cargo test ... -- --ignored --nocapture` exits 0; test result `ok. 1 passed; 0 failed; 0 ignored; 0 measured; 18 filtered out; finished in 3600.00s`. The cargo wall-clock matches `FORMAL_MEMORY_SOAK_DURATION = Duration::from_secs(60 * 60)` exactly, confirming the soak loop ran for its full minimum duration. Estimated cycle count (not directly captured by the test's stdout — the test holds the `MemorySoakReport` in a local but does NOT print its fields): at the re-validated 2026-05-12 Phase 3.3 p95 of 0.818 ms/cycle, 1 hour ≈ **~4.4M cycles**; preservation invariant `restored_components == cycles * entity_count` held across all of them.

## Formal 1-hour memory soak — RUN 2026-05-17 (process-memory metrics enabled)

Recorded 2026-05-17 (release-profile background test run). This is the first
formal one-hour soak run against the 2026-05-16 process-memory harness revision
described in "Memory-soak process-memory metrics" below — i.e. the
"future release-readiness one-hour soak" that revision anticipated. Run via the
exact formal target:

```sh
cargo test -p rge-script-bench --release --lib \
  script_host::tests::phase_3_memory_soak_one_hour \
  -- --ignored --nocapture
```

Toolchain / host: Windows 11 / x86_64 recorder host, `cargo 1.92.0` /
`rustc 1.92.0`, `wasmtime 44.0.1`. Exit code 0; recorded cargo wall-clock
3600.46 s; test harness summary
`ok. 1 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out; finished in 3600.00s`.

Exact `--nocapture` stdout evidence:

```
phase3_memory_soak: entities=1000 cycles=3736934 elapsed_s=3600.00 restored_components=3736934000 expected_restored_components=3736934000 final_counter_sum=1000499500
phase3_memory_soak_memory: samples=3736936 start_rss_bytes=8859648 end_rss_bytes=10346496 peak_rss_bytes=10510336 start_vss_bytes=1327104 end_vss_bytes=2695168 vss_delta_bytes=1368064
```

| workload | engine | scene | minimum_duration | metric | value |
| --- | --- | --- | --- | --- | --- |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `elapsed_s` | **3600.00 s** |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `cycles` | **3736934** |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `restored_components` / `expected_restored_components` | **3736934000 / 3736934000** (equal — invariant HELD) |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `final_counter_sum` | **1000499500** |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | process-memory `samples` | **3736936** (= start + one per cycle + end) |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `start_rss_bytes` | **8859648** |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `end_rss_bytes` | **10346496** |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `peak_rss_bytes` | **10510336** |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `start_vss_bytes` | **1327104** |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `end_vss_bytes` | **2695168** |
| `memory_soak` | `script_host_counter` | 1,000 `Counter` entities | 1 hour (3600 s) | `vss_delta_bytes` | **1368064** |

All byte counts above are recorded exactly as printed to stdout; none are
normalized, rounded, or re-derived.

**Platform mapping (honest).** On the Windows recorder host the `memory-stats`
sampler reports *resident* memory as the process **working set**
(`GetProcessMemoryInfo` → `WorkingSetSize`) and *virtual* memory as the process
**commit charge** (`PagefileUsage`). So `peak_rss_bytes` is the peak working-set
sample observed across the soak, and `vss_delta_bytes` is an end-minus-start
**commit-charge** delta — not a virtual-address-space-span delta. The
`vss_delta_bytes` value here is positive (`1368064`, i.e. commit charge grew);
it is captured verbatim and would have been recorded as a negative number,
unchanged, had the commit footprint shrunk.

**Phase 3.4 exit criterion #3 — metrics-enabled re-run**: the one-hour soak
exits 0, the ignored test reports `ok`, and `elapsed_s` (3600.00) meets the
`FORMAL_MEMORY_SOAK_DURATION` 3600 s floor. Across 3736934 hot-reload cycles the
preservation invariant `restored_components == cycles * entity_count` held
exactly (3736934000 on both sides). The working set sampled from
`start_rss_bytes=8859648` to `end_rss_bytes=10346496` with
`peak_rss_bytes=10510336`, and commit charge from `start_vss_bytes=1327104` to
`end_vss_bytes=2695168`, with no panic / hang / OOM. This run complements the
2026-05-12 row above: that earlier run predates the metrics revision and its
"no memory leak" reading was implicit, whereas this run backs it with captured
process-memory numbers. The 2026-05-12 row stands unchanged as the pre-metrics
historical record.

**Scope limitation (LOAD-BEARING)**: CONSTRAINED-CERTIFIED on the recorder host
only (Windows 11 / x86_64, single one-hour run). It does NOT certify allocator
fragmentation, vendor / OS parity, sustained behavior beyond one hour, or the
per-cycle memory-growth curve — only start, per-cycle-folded, and end samples
are taken, summarized into peak plus endpoints. The values are single-run point
samples, not a multi-run distribution.

## Formal W04 raw WasmtimeCranelift gates — RUN 2026-05-12

Recorded 2026-05-12 (release-profile test run) via:

```sh
cargo test -p rge-script-bench --release --lib \
  wasmtime_cranelift::tests \
  --manifest-path A:\RCAD\RGE\Cargo.toml \
  -- --nocapture
```

Fixture at `crates/script-bench/src/wasmtime_cranelift.rs` — **direct wasmtime API**, no `runtime-wasmtime-engine` / `runtime-wasmtime` orchestration, no `rge-script-host` capability checks or ECS marshaling. `wasmtime::Engine` configured with `cranelift_opt_level(OptLevel::Speed)` mirroring `runtime-wasmtime-engine::Engine::new` exactly. Three inline WAT fixtures (`SCRIPT_TICK_1M_WAT` / `PER_FRAME_TICK_10K_WAT` / `COLD_START_EMPTY_WAT`).

**The four W04 cells flipped from `_pending W04_` to measured numbers**:

| workload | engine | metric | unit | value | native row | ratio (wasm / native) |
| --- | --- | --- | --- | --- | --- | --- |
| `script_tick_1m_iters` | `wasmtime_cranelift` (raw) | wall_time | ns total / 1M op | **713 200** | 674 666 | **1.057×** |
| `script_tick_1m_iters` | `wasmtime_cranelift` (raw) | per_op | ns/op | **0.713** | 0.675 | **1.057×** |
| `per_frame_tick_10k_entities` | `wasmtime_cranelift` (raw) | wall_time | ns total / 10k | **76 200** | 7 594 | **10.034×** |
| `per_frame_tick_10k_entities` | `wasmtime_cranelift` (raw) | per_entity | ns/entity | **7.620** | 0.76 | **10.026×** |
| `cold_start` | `wasmtime_cranelift` (raw) | wall_time | ns | **405 100** | 48.74 | N/A (different physics) |
| `cold_start` | `wasmtime_cranelift` (raw) | wall_time | ms | **0.405** | — | — |
| `memory_overhead` | `wasmtime_cranelift` (raw) | bytes_per_module | bytes | **13 680** | 8 | N/A (different physics) |

**`script_tick_1m_iters` verdict (PASS gate)**: 1.057× — well under the PLAN §5.6 1.5× target. The tight-loop f32-arithmetic workload sits comfortably within wasmtime cranelift's hot-path optimization — register-allocated `pos` accumulator, no memory access, no bounds checks. **Matches the "fastest script engine" pillar's design assumption** for compute-bound workloads.

**`per_frame_tick_10k_entities` verdict (FAILS the 1.5× target as a raw per-entity measurement)**: 10.034× — well over the PLAN §5.6 1.5× target. The workload is memory-bound (10,000 entities × 6 f32 memory operations per entity = 60,000 bounded loads/stores per frame). Wasmtime cranelift's linear-memory bounds checks aren't fully optimized away on this access pattern; native Rust's slice iteration is bounds-check-elision-friendly + vectorization-friendly. **This is NOT a regression — it's a structural characteristic of raw per-entity WASM execution against memory-bound workloads.** The previously-recorded **`script_host_counter_bulk` measurement of 1.34× (2026-05-12 re-validation; 1.21× on 2026-05-11) achieves the 1.5× target** by amortizing per-entity wasm overhead across a single host crossing per frame (one `tick(dt)` + one `rge.ecs::add_to_all_counters(1)` host call per frame; the host iterates the 1,000 entities natively rather than each entity crossing the wasm boundary). The two measurements describe different workload shapes:

- **`per_frame_tick_10k_entities` raw** measures wasmtime cranelift's per-entity ECS-iteration overhead when entities are visited from within wasm (10× over native — DOES NOT meet the 1.5× target);
- **`script_host_counter_bulk`** measures wasmtime cranelift's bulk-host-crossing overhead when the wasm boundary is amortized (1.34× over native — MEETS the 1.5× target).

The PLAN §5.6 1.5× target is achievable for the script-host workload pattern (bulk-path host crossings) but is structurally violated by the raw per-entity-wasm-loop pattern. **No engine reshape, no PLAN §5.6 retarget, and no native-baseline rewrite is proposed in this W04 sub-α dispatch** — the gap is recorded for transparency; downstream architectural decisions (e.g., enforce bulk-path discipline for production wasm code; or document that some workloads must go through bulk-path) are out of scope.

**`cold_start` verdict (different physics)**: 405 µs raw wasmtime cranelift cold-start (parse + Cranelift JIT compile + instantiate + first call) vs the native baseline's 48.74 ns (timer floor for an empty closure call). The two measurements describe different physics — native has no module-load step; the wasm 405 µs IS the module-load step. **Well under the PLAN §5.6 target of < 50 ms by ~125×** (0.405 ms / 50 ms ≈ 0.8% of budget) — comfortable headroom for a release build with JIT compile.

**`memory_overhead` verdict (different physics)**: 13,680 bytes / module raw wasmtime cranelift (`Module::serialize().len()` on the empty `(module (func (export "noop")))` — proxy for "bytes per loaded module" at the AOT-artifact level; captures compiled-code size + module metadata) vs the native baseline's 8 bytes (function-pointer cost). **Well under the PLAN §5.6 target of < 1 MB per module by ~75×** (13.68 KiB / 1024 KiB ≈ 1.3% of budget). Note: this is the SERIALIZED bytes count, NOT the runtime RSS — a true RSS measurement requires platform-specific instrumentation (`/proc/self/status` / `GetProcessMemoryInfo`) and is OUT OF SCOPE for this sub-α dispatch.

**Scope limitation (LOAD-BEARING)**: These W04 raw WasmtimeCranelift gates are **CONSTRAINED-CERTIFIED on the recorder host only** (Windows 11 / x86_64, cargo 1.94.1, wasmtime 44.0.1, single-run point estimates from the targeted `cargo test --release --lib wasmtime_cranelift::tests --nocapture` invocation). They certify:

- The four W04 cells (`script_tick_1m_iters` / `per_frame_tick_10k_entities` / `cold_start` / `memory_overhead`) for the `wasmtime_cranelift` column are NO LONGER `_pending W04_` — they hold measured numbers.
- The `wasmtime_cranelift` column measures **raw** wasmtime cranelift, **NOT** the `script_host_counter` orchestrated path (different fixtures, different overheads — both are real measurements of wasmtime cranelift JIT, but the raw path strips script-host's capability checks + ECS marshaling + hot-reload state machine).

They do NOT certify:

- Universal performance across hardware classes / vendor parity (single Windows 11 / x86_64 NVIDIA-host run).
- Cold-start frame cost (the 405 µs is single-run; criterion-style multi-sample distribution not captured here).
- Sustained-thermal behavior (single-shot test; not a long-run measurement).
- CI regression coverage (no ratchet baseline established; future re-runs against this 2026-05-12 measurement would be the natural ratchet target).
- Memory or VRAM beyond the AOT-artifact byte proxy.
- W04 cross-engine columns beyond `wasmtime_cranelift` — `wasmtime_singlepass` (Winch) is sub-β scope; `mlua` / `wasmer_singlepass` / `bevy_extism` are `_post-Phase-3_` per BASELINE.md's roster table.

**Harness**: `crates/script-bench/src/wasmtime_cranelift.rs::tests` (four `#[test]` fns, non-`#[ignore]`'d, run in default `cargo test` per the phase_3_3 / phase_3_4 convention). Invoke via:

```sh
cargo test -p rge-script-bench --release --lib \
  wasmtime_cranelift::tests \
  --manifest-path A:\RCAD\RGE\Cargo.toml \
  -- --nocapture
```

## W04 raw WasmtimeCranelift cold-start — RUN 2026-05-28 (ISSUE-243 rebaseline)

Recorded 2026-05-28 (release-profile focused-test run; ISSUE-243 docs+measurement
dispatch — see `ai_handoffs/ISSUE-243_TASK_2026-05-28_09-50-54+0300.md`). This
section is **append-only**; it does NOT rewrite the 2026-05-12 RUN row above and
does NOT touch the historical 904-microsecond wasmtime 23 record carried forward
in `crates/runtime-wasmtime-engine/BASELINE.md`. It addresses the carry-over
"WASM cold-start baseline (904µs) measured on wasmtime 23, not re-validated post
bump to 44" line that has been hanging in `HANDOFF.md` / `Status.md`: the
2026-05-12 RUN already recorded a wasmtime 44 number; this RUN adds a fresh
recorder-host wasmtime 44 sample so the rebaseline is on the current toolchain
rather than implied via the 2026-05-12 single-run point estimate.

Toolchain / host: Windows 11 Pro for Workstations / x86_64 recorder host
(NVIDIA RTX 4060 Ti present per Vulkan render-gate lineage); `cargo 1.92.0
(344c4567c 2025-10-21)`; `rustc 1.92.0 (ded5c06cf 2025-12-08)`;
`wasmtime 44.0.1` (per `Cargo.lock`). Exact command, repeated three times
back-to-back from the workspace root:

```powershell
cargo test -p rge-script-bench --release --lib `
  wasmtime_cranelift::tests::w04_cold_start_wasmtime_cranelift -- --nocapture
```

Raw `--nocapture` stdout for each of the three release runs (single test, single
sample per run; lines copied verbatim from the test runner):

```
w04_cold_start_wasmtime_cranelift: duration_ns=417400 duration_ms=0.417
w04_cold_start_wasmtime_cranelift: duration_ns=257300 duration_ms=0.257
w04_cold_start_wasmtime_cranelift: duration_ns=255600 duration_ms=0.256
```

| workload | engine | metric | unit | run 1 | run 2 | run 3 | min-of-3 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `cold_start` | `wasmtime_cranelift` (raw) | wall_time | ns | 417 400 | 257 300 | 255 600 | **255 600** |
| `cold_start` | `wasmtime_cranelift` (raw) | wall_time | ms | 0.417 | 0.257 | 0.256 | **0.256** |

**Selected min-of-3**: **255 600 ns / 0.256 ms**.

**Delta vs 2026-05-12 wasmtime 44 Cranelift anchor (405 100 ns / 0.405 ms)**:
`(255 600 - 405 100) / 405 100 = -36.9%` — the fresh min-of-3 is **faster**
than the 2026-05-12 single-run point estimate by ~37%. The greater-than-20%
slowdown observation threshold (per the dispatch's reporting rule) is therefore
not triggered; the movement is recorded as an observation only with no
pass/fail action and no regression investigation. The first sample's higher
value (417 400 ns) is consistent with a single-run warm-up effect on the cold
parse + Cranelift JIT compile path; the second and third samples agree to
within 0.7%.

**Scope limitation (LOAD-BEARING)**: This 2026-05-28 cold-start row is
**CONSTRAINED-CERTIFIED on the recorder host only** (Windows 11 Pro for
Workstations / x86_64, cargo 1.92.0, wasmtime 44.0.1, three back-to-back
single-sample release runs from the focused `w04_cold_start_wasmtime_cranelift`
test). It does **NOT** certify:

- Universal performance across hardware classes / vendor parity (single
  recorder-host run).
- Cold-start behavior on other machines (three samples on one host is not a
  multi-host distribution).
- Sustained thermal behavior (three back-to-back single-sample runs are not a
  long-run measurement).
- CI regression coverage (no ratchet baseline is established by this rebaseline;
  the 2026-05-12 row remains the comparable single-run anchor).
- Memory or VRAM beyond what the existing `memory_overhead` test already
  proxies (not re-run here).
- Any W04 cell other than `cold_start` for `wasmtime_cranelift` (the other
  three W04 sub-α cells — `script_tick_1m_iters`, `per_frame_tick_10k_entities`,
  `memory_overhead` — were not re-measured in this dispatch).

**History preserved (this section is additive)**:

- The 2026-05-12 RUN section above (wasmtime 44 single-run point estimates for
  all four W04 sub-α cells, including the 405 100 ns / 0.405 ms cold-start)
  stands unchanged as the dated historical record and remains the anchor used
  for the delta above.
- The 904-microsecond cold-start measurement recorded in
  `crates/runtime-wasmtime-engine/BASELINE.md` (median of 5 release runs taken
  2026-05-05 against wasmtime 23 in the prior wrapper-crate context) remains a
  historical record at a different engine version and through a different
  measurement path; it is **not** the comparison anchor for this rebaseline and
  was not re-measured here.

**Harness**: `crates/script-bench/src/wasmtime_cranelift.rs::tests::w04_cold_start_wasmtime_cranelift`
(unchanged; no Rust source / test / bench / fixture / Cargo edit was performed
by this dispatch).

## Formal W04 raw WasmtimeSinglepass (Winch) gates — RUN 2026-05-12

Recorded 2026-05-12 (release-profile test run; sub-β follow-on to sub-α) via:

```sh
cargo test -p rge-script-bench --release --lib \
  wasmtime_singlepass::tests \
  --manifest-path A:\RCAD\RGE\Cargo.toml \
  -- --nocapture
```

Fixture at `crates/script-bench/src/wasmtime_singlepass.rs` — mirror of `wasmtime_cranelift.rs` with one config-strategy swap: `Config::strategy(Strategy::Winch)` instead of `cranelift_opt_level(OptLevel::Speed)`. **Same four WAT fixtures** re-used as `pub(crate)` from `wasmtime_cranelift`; **same four workloads**; **same four measurement tests**. The `winch` feature flag is enabled in `crates/script-bench/Cargo.toml` (script-bench-local override; the runtime crates stay on default-Cranelift-only).

**The four W04 cells flipped from `_pending W04 sub-β_` to measured numbers**:

| workload | engine | metric | unit | value | native | Winch / native | Winch / Cranelift (sub-α) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `script_tick_1m_iters` | `wasmtime_singlepass` (raw Winch) | wall_time | ns total / 1M op | **2 546 600** | 674 666 | **3.774×** | **3.572×** |
| `script_tick_1m_iters` | `wasmtime_singlepass` (raw Winch) | per_op | ns/op | **2.547** | 0.675 | **3.774×** | **3.572×** |
| `per_frame_tick_10k_entities` | `wasmtime_singlepass` (raw Winch) | wall_time | ns total / 10k | **97 500** | 7 594 | **12.838×** | **1.280×** |
| `per_frame_tick_10k_entities` | `wasmtime_singlepass` (raw Winch) | per_entity | ns/entity | **9.750** | 0.76 | **12.829×** | **1.279×** |
| `cold_start` | `wasmtime_singlepass` (raw Winch) | wall_time | ns | **305 000** | 48.74 | N/A (different physics) | **0.753×** (Winch FASTER) |
| `cold_start` | `wasmtime_singlepass` (raw Winch) | wall_time | ms | **0.305** | — | — | — |
| `memory_overhead` | `wasmtime_singlepass` (raw Winch) | bytes_per_module | bytes | **13 680** | 8 | N/A (different physics) | **1.000×** (identical) |

## Central question's answer — Winch resolves the 10× per-entity penalty's origin

Per user direction at sub-β open: "is the raw per-entity overhead a Cranelift-specific shape or a broader direct-WASM execution cost?"

**Answer: the per-entity overhead is BROADLY characteristic of direct-WASM execution, NOT Cranelift-specific.** The `per_frame_tick_10k_entities` raw measurement shows:

- Cranelift: 7.620 ns/entity = **10.034× native**
- Winch: 9.750 ns/entity = **12.829× native**
- Winch / Cranelift ratio for this workload: **1.280×** — within 30% of each other; both compilers land in the same order-of-magnitude penalty band

If the 10× ratio were Cranelift-specific (e.g., a missed optimization opportunity in Cranelift's bounds-check elision), Winch — the simpler / less-optimizing compiler — would show a MUCH larger gap. Instead, Winch is only 1.28× slower than Cranelift on this workload. That tight cross-compiler ratio means the per-entity penalty is driven by the **WASM execution model itself** (bounds-checked linear-memory access on every f32 load/store, per-element instruction overhead, no auto-vectorization) — not by either compiler's codegen quality. **The PLAN §5.6 1.5× target is structurally unachievable for any direct-WASM per-entity-loop workload regardless of compiler choice**; the `script_host_counter_bulk` pattern (single host crossing per frame, host iterates entities natively) is the architectural answer, validated by the 1.34× ratio it achieves.

This is "uncomfortable data" by user framing — but it answers the question the chapter was opened to surface.

## Cranelift-vs-Winch cross-compiler analysis (sub-α + sub-β joint reading)

| workload | Cranelift (sub-α) | Winch (sub-β) | Winch / Cranelift | Interpretation |
| --- | --- | --- | --- | --- |
| `script_tick_1m_iters` | 0.713 ns/op | 2.547 ns/op | **3.572×** | Winch's non-optimizing codegen costs ~3.5× on tight compute loops. Cranelift's register allocation + loop hoisting + dead-code elimination are real for this workload. |
| `per_frame_tick_10k_entities` | 7.620 ns/entity | 9.750 ns/entity | **1.280×** | Within ~30%; bottleneck is WASM bounds-checked memory access, not compiler quality. **Central question answered.** |
| `cold_start` | 0.405 ms | 0.305 ms | **0.753×** (Winch faster) | Winch's design point realized: fast compile (saves ~100 µs / ~25%) at the cost of slower runtime. Validates Winch's intended use case (script hot-reload, cold-start-sensitive paths). |
| `memory_overhead` | 13 680 B | 13 680 B | **1.000×** (identical) | The empty `(module (func (export "noop")))` module's serialized artifact is the same size regardless of compiler — no function body to compile differently; just module metadata + a noop. |

**Engineering takeaway**: Cranelift is the better choice for compute-bound + memory-access-heavy hot paths; Winch is the better choice for hot-reload / fast-iteration paths. The script-host's hot-reload uses Cranelift (cranelift_opt_level Speed) — this is correct for runtime perf; Winch would save cold-start at the cost of swap-window p95 (currently 0.818 ms on Cranelift; would be slower on Winch). **No engine-default change proposed** — recording the cross-compiler trade-off in BASELINE.md for future architectural decisions.

## W04 sub-β scope limitation (LOAD-BEARING)

This sub-β closure is **CONSTRAINED-CERTIFIED on the recorder host only** (Windows 11 / x86_64, cargo 1.94.1, wasmtime 44.0.1 with `winch` feature enabled, single-run point estimates). Certifies:

- All four `wasmtime_singlepass` cells (previously `_pending W04 sub-β_`) hold measured numbers.
- The Winch compiler is functional on this build of wasmtime + this WAT fixture set.
- The central question (Cranelift-specific vs broader direct-WASM) is answered.

Does NOT certify:

- Universal performance across hardware classes (x86_64 only; Winch's aarch64 support is partial).
- Vendor parity (single recorder run; future ratchet baseline would establish per-host expectations).
- Sustained-thermal behavior (single-shot).
- CI regression coverage (no ratchet target established for the W04 columns yet).
- Memory or VRAM beyond the AOT-artifact byte proxy.
- W04 cross-engine columns beyond `wasmtime_singlepass` — MLua / WasmerSinglepass / BevyExtism stay `_post-Phase-3_` per the roster table.

**Harness**: `crates/script-bench/src/wasmtime_singlepass.rs::tests` (four `#[test]` fns, non-`#[ignore]`'d, run in default `cargo test` per the phase_3_3 / phase_3_4 / sub-α convention).

## W04 follow-on — raw Winch (singlepass) hot-reload measurement — RUN 2026-05-12

Recorded 2026-05-12 (release-profile test run; W04 follow-on to sub-α/β answering the empirical question "is the raw per-entity overhead a Cranelift-specific shape or a broader direct-WASM execution cost?" — sub-β answered that for the raw-WASM bench fixtures; this follow-on answers the parallel question "does Winch meaningfully improve swap-window / hot-reload p95 enough to matter for fast-iteration scenarios?") via:

```sh
cargo test -p rge-script-bench --release --lib \
  script_host::tests::w04_hot_reload_swap_wasmtime_singlepass \
  --manifest-path A:\RCAD\RGE\Cargo.toml \
  -- --nocapture
```

**The harness reuses `ScriptHostBench`'s engine-agnostic structure**: NEW `ScriptHostBench::new_with_strategy(strategy: wasmtime::Strategy)` constructor at `crates/script-bench/src/script_host.rs` (~12 LoC) builds the wasmtime engine with the supplied strategy and compiles the same 3 Counter WAT fixtures (`COUNTER_V1_WAT` / `COUNTER_V2_WAT` / `COUNTER_BULK_WAT`); the existing `ScriptHostBench::new()` stays byte-identical (`Engine::default()` → Cranelift, preserving all existing phase_3_3 / phase_3_4 / soak / hygiene tests). The Winch test uses the new constructor with `Strategy::Winch` then drives the SAME `hot_reload_preservation(HotReloadConfig::formal())` workflow as the Cranelift formal gate — same capability checks + ECS marshaling + hot-reload state machine + tick body; only the JIT backend swaps underneath.

| workload | engine | scene | cycles | metric | value | vs Cranelift formal gate (re-validated 2026-05-12) |
| --- | --- | --- | --- | --- | --- | --- |
| `hot_reload_swap` | `wasmtime_singlepass` (raw Winch) | 1,000 `Counter` entities | 100 | p95 swap window | **0.865 ms** | 1.057× (within ±6% noise of Cranelift's 0.818 ms) |
| `hot_reload_swap` | `wasmtime_singlepass` (raw Winch) | 1,000 `Counter` entities | 100 | max swap window | **1.219 ms** | 1.241× of Cranelift's 0.982 ms |
| `hot_reload_swap` | `wasmtime_singlepass` (raw Winch) | 1,000 `Counter` entities | 100 | avg swap window | **0.797 ms** | 1.005× of Cranelift's 0.793 ms (essentially identical) |

**Winch hot-reload verdict**: **PASS** against PLAN §5.6's <100 ms p95 budget — 0.865 / 100 ≈ 0.9% of budget (vs Cranelift's 0.8%; both have >100× headroom). Winch does **NOT meaningfully improve** swap-window p95 vs Cranelift for the `script_host_counter` orchestrated workload — the ~5.7% p95 difference is within typical single-run measurement noise. **The dispatch's central question is answered**: "is Winch meaningfully better at hot-reload?" → **No, not for this workload shape**.

**Mechanical explanation** (cross-referencing sub-α/β cross-compiler analysis):

- Winch compile path is **faster** (~25% faster cold_start measured at sub-β: 0.305 ms vs Cranelift 0.405 ms)
- Winch runtime path is **slower** (3.57× slower script_tick_1m: 2.547 ns/op vs Cranelift 0.713 ns/op)
- The hot-reload swap cycle includes BOTH compile work (Winch faster) AND tick execution (Winch slower)
- **Neither dominates** for the `script_host_counter` workload — they roughly balance, giving a near-identical p95
- Cranelift retains its production-default position because: (a) hot-reload p95 is similar; (b) runtime perf favors Cranelift; (c) no architectural reason to switch

**No engine-default change proposed.** `rge-runtime-wasmtime-engine::Engine::new` continues to use `cranelift_opt_level(OptLevel::Speed)`; production hot-reload continues to use Cranelift. The Winch measurement is a release-readiness data point for the BASELINE.md cross-engine row, not a target retarget. PLAN §5.6 1.5× / <100 ms targets stay unchanged.

**Scope limitation (LOAD-BEARING)**: This Winch hot-reload measurement is **CONSTRAINED-CERTIFIED on the recorder host only** (Windows 11 / x86_64, cargo 1.94.1, wasmtime 44.0.1 with `winch` feature enabled, single-run point estimate). Certifies:

- `Strategy::Winch` successfully compiles the `counter_v1.wat` / `counter_v2.wat` / `counter_bulk.wat` fixtures (no Winch coverage gap surfaced)
- Hot-reload preservation invariant (`restored_components == cycles * entity_count`) HOLDS under Winch — `script_host`'s capture/restore protocol is engine-agnostic, validated empirically
- Winch swap-window p95 is within ±6% of Cranelift's on this workload

Does NOT certify:

- Universal performance across hardware classes
- Vendor parity (single Windows 11 / x86_64 run)
- Long-run stability under Winch (no 1-hour soak under Winch run; sub-β's Phase 3.4 exit criterion #3 soak was Cranelift-only)
- Cross-cycle variance (single-run point estimate; criterion-style multi-sample distribution not captured here)
- W04 cross-engine columns beyond `wasmtime_singlepass` hot-reload — MLua / WasmerSinglepass / BevyExtism stay `_post-Phase-3_`

**Harness**: `crates/script-bench/src/script_host.rs::tests::w04_hot_reload_swap_wasmtime_singlepass` (one `#[test]` fn, non-`#[ignore]`'d, runs in default `cargo test` per the phase_3_3 / phase_3_4 / sub-α / sub-β convention). The test asserts `report.p95_duration < Duration::from_millis(500)` — loosened from the 100 ms PASS gate to PLAN §5.6's abort threshold (`IMPLEMENTATION.md:323`) per the dispatch's measurement-only framing.

**Scope limitation (LOAD-BEARING)**: This soak closure is **CONSTRAINED-CERTIFIED on the recorder host only** (Windows 11 / x86_64, cargo 1.94.1, wasmtime 44.0.1, single-run). It certifies:

- 1 hour of continuous hot-reload swap cycles completes without panic / OOM / hang
- 1000-entity preservation invariant (`restored_components == cycles * entity_count`) holds across millions of cycles
- The wasmtime engine + script-host substrate is stable under sustained swap load

It does NOT certify:

- Explicit memory-growth metrics **for this 2026-05-12 run**. That run predates the 2026-05-16 harness revision, so its "no memory leak" conclusion is implicit (the process would have OOM'd if it were leaking severely enough over 1 hour) rather than backed by captured process-memory numbers. The harness now does capture them — see "Memory-soak process-memory metrics" below — but the one-hour soak was **not** re-run for that revision, so no new formal one-hour `peak_rss` / `vss_delta` baseline row is published here.
- Allocator fragmentation (heap layout not inspected)
- VRAM (no GPU involved in script-host hot-reload)
- Sustained-thermal behavior beyond 1 hour
- Vendor / OS parity (single Windows 11 / x86_64 run)
- Per-cycle timing variance over the full hour (only the 100-cycle p95 gate above measures swap-window distribution; the soak measures stability not latency)

### Memory-soak process-memory metrics — harness revision 2026-05-16

`MemorySoakReport` now carries a `process_memory: Option<ProcessMemoryMetrics>`
field, populated by direct process-memory sampling inside
`ScriptHostBench::memory_soak`. The soak samples the host process at three
points — soak start, after each completed hot-reload cycle, and soak end — and
folds those observed samples into `ProcessMemoryMetrics`:

- `peak_rss_bytes` — largest resident / working-set sample observed across the soak.
- `start_rss_bytes` / `end_rss_bytes` — resident bytes at soak start / end.
- `start_vss_bytes` / `end_vss_bytes` — virtual-size bytes at soak start / end.
- `vss_delta_bytes` — end-minus-start virtual delta (signed).
- `samples` — sample count (start + one per cycle + end).

**Platform mapping.** On Windows (the recorder host) *resident* is the process
**working set** (`GetProcessMemoryInfo` → `WorkingSetSize`) and *virtual* is the
process **commit charge** (`PagefileUsage`) — not the true virtual
address-space span, so `vss_delta_bytes` is a commit-charge delta on Windows. On
Linux *resident* is `/proc/self` RSS and *virtual* is VSZ. On platforms with no
supported sampler the field is `None` — honest unavailability, never a
fabricated zero. The process-memory syscall is provided by the `memory-stats`
crate, kept local to `crates/script-bench`; a standard-library / minimal-FFI
sampler is blocked by the workspace `unsafe_code = "forbid"` lint, which a
crate-level `#[allow]` cannot lower.

The `phase_3_memory_soak_one_hour` test now prints these metrics under
`--nocapture` (a `phase3_memory_soak_memory: …` line alongside the existing
`phase3_memory_soak: …` line). A bounded, non-`#[ignore]`'d
`memory_soak_reports_process_memory_metrics` test exercises the same path with a
tiny scene and a sub-second duration floor, asserting numeric process-memory
values on Windows and honest `None` handling elsewhere; it completes in well
under a second.

**This revision adds the harness capability only.** It does **not** re-run the
formal one-hour soak and publishes no new one-hour memory baseline row — the
2026-05-12 RUN rows above stand unchanged. A future release-readiness one-hour
soak invocation would be the natural producer of a formal one-hour
`peak_rss` / `vss_delta` baseline.

> **Forward cross-reference (2026-05-23):** the "future release-readiness one-hour soak invocation" anticipated by this 2026-05-16 revision was performed on 2026-05-17 and is recorded above as "Formal 1-hour memory soak — RUN 2026-05-17 (process-memory metrics enabled)" (lines 140–211 of this file), with `peak_rss_bytes` and `vss_delta_bytes` captured via the harness revision described in this section. This section's body remains the dated 2026-05-16 capability description and is preserved as written.

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

## Remaining engine rows

The table below is the cross-engine comparison shape. As of 2026-05-12, the
**`wasmtime_cranelift` column is filled** with raw cranelift measurements
(W04 sub-α dispatch — see "Formal W04 raw WasmtimeCranelift gates" section
above). The `script_host_counter` orchestrated path (capability checks + ECS
marshaling + hot-reload state machine on top of wasmtime cranelift) measures
DIFFERENT numbers — kept distinct from the raw column. Lua/mlua /
Wasmer-singlepass / Bevy-extism remain `_post-Phase-3_`; `wasmtime_singlepass`
(Winch) is `_pending W04 sub-β_`.

| workload                       | native_rust | wasmtime_cranelift (raw) | wasmtime_singlepass (raw Winch) | mlua | wasmer_singlepass | bevy_extism |
| ------------------------------ | ----------- | ------------------------ | --------------------------------- | ---- | ----------------- | ----------- |
| `script_tick_1m_iters`         | _baseline_  | **713 200 ns / 0.713 ns/op (1.057× native; PASS 1.5×)** | **2 546 600 ns / 2.547 ns/op (3.774× native; FAILS 1.5×; 3.57× over Cranelift — Winch's non-optimizing codegen)** | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `per_frame_tick_10k_entities`  | _baseline_  | **76 200 ns / 7.620 ns/entity (10.034× native; FAILS 1.5× as raw per-entity; meet target via bulk-path)** | **97 500 ns / 9.750 ns/entity (12.829× native; FAILS 1.5×; 1.28× over Cranelift — per-entity penalty is BROADLY direct-WASM, not Cranelift-specific)** | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `cold_start`                   | 0 ns *      | **405 100 ns / 0.405 ms (PASS < 50 ms)** | **305 000 ns / 0.305 ms (PASS < 50 ms; 0.75× of Cranelift — Winch FASTER at compile)** | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `hot_reload_swap`              | _baseline_  | **`script_host_counter` orchestrated path: p95=0.818 ms (PASS < 100 ms); raw cranelift hot-reload not measured separately** | **`script_host_counter` orchestrated path with `Strategy::Winch`: p95=0.865 ms (PASS < 100 ms; 1.057× Cranelift — within ±6% noise; Winch does NOT meaningfully improve swap-window p95)** | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |
| `memory_overhead`              | 8 B *       | **13 680 B / module (`Module::serialize().len()` AOT-artifact proxy; PASS < 1 MB; runtime RSS not measured)** | **13 680 B / module (identical to Cranelift — empty-module artifact size is compiler-independent for this fixture)** | _post-Phase-3_ | _post-Phase-3_ | _post-Phase-3_ |

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
