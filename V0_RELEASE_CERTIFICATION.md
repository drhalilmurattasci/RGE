# RGE v0 Release Certification

| Field | Value |
| --- | --- |
| **Decision** | **CERTIFIED v0** |
| Certified commit | `6aaf7f12955db291b344eccd57bb3926f8c48ed4` (short `6aaf7f1`) |
| Certified at | 2026-05-14 18:13:03 +0300 |
| Certifier | Claude (Anthropic) via v0 release-certification dispatch |
| Origin sync | `git rev-list --left-right --count origin/main...HEAD` = `0 0` |
| Tracked tree | clean |

## 1. Cheap-gate re-run (this dispatch)

All three cheap gates re-run against the certified commit. Substrate has not changed since the prior cheap-gate green state — the five commits between the last full sweep and `6aaf7f1` are docs-only or test-broadening — so this re-run is a pure regression check.

| Gate | Command | Result |
| --- | --- | --- |
| nightly fmt --check | `cargo +nightly fmt --all -- --check` | **PASS** (exit 0, no diff) |
| 9-lint architecture | `cargo run -p rge-tool-architecture-lints -- all` | **PASS** (9 enforcement lints + 1 supplementary; all 0 violations) |
| workspace tests | `cargo test --workspace --no-fail-fast` | **PASS** (333 suites, 2549 tests passed, 0 failed, 20 ignored, exit 0) |

The 20 ignored tests are the documented expensive-gate suites (1-hour memory soak, recorder-host Gate A P95, etc.) — they are `#[ignore]` by convention and accounted for separately in §2.

### 9-lint architecture detail

`forbidden-dep`, `split-exemption`, `no-utils`, `graph-foundation`, `editor-state-ownership`, `command-bus`, `projection-modules`, `kernel-isolation`, `failure-class` — all PASS (0 violations each). Supplementary `snapshot-participate` lint: 3 stateful Tier-2 crates checked (`rge-cad-core`, `rge-cad-projection`, `rge-physics`), all 3 impl `SnapshotParticipate`, 0 still missing.

## 2. Expensive-gate decision (SKIPPED with justification)

Two ignored gates exist. Both have recent green runs against substrate that has not changed in any way that could affect them. Re-running each on this dispatch's host would either be wasteful (soak) or non-comparable (recorder-host-specific perf). Both are SKIPPED.

| Gate | Last green run | Substrate-change rationale | Decision |
| --- | --- | --- | --- |
| Phase 3 1-hour memory soak (`phase_3_memory_soak_one_hour`) | 2026-05-12, 3600.00 s wall-clock, ~4.4 M cycles, all 3 assertions HELD, no panic/OOM/hang | Substrate UNCHANGED since (Job 8 closeout: "Soak re-run deferred indefinitely (substrate UNCHANGED)"). Intervening commits are docs/protocol-only or test-broadening; none touch the runtime/hot-reload/script-bench paths exercised by the soak. | **SKIP** |
| Post-depth Gate A P95 (recorder-host gfx perf) | 2026-05-14, recorder-host min-of-3 P95 = 0.122 ms (commit `03d3f05`, broadened by `2b64241`) | Recorder-host gate is host-specific; re-running on any other host produces non-comparable numbers. No gfx production code has changed since `2b64241`. | **SKIP** |

Both rationales are recoverable: a future dispatch on the recorder host can re-fire either gate without any substrate change.

## 3. Closed-gate inventory (substrate state at `6aaf7f1`)

The substrate state at this commit reflects an accumulation of closed gates from the MAIN-ORDERED 10-job queue closeout (2026-05-14) and prior phase closures. Summary:

- **Phase 3 release-readiness**: 4/4 exit criteria CLOSED.
  - Hot-reload p95 < 100 ms — `phase3_hot_reload_1000_entities_100_cycles` (0.796 / 0.818 ms; ~125× under gate).
  - ECS-via-WASM ≤ 1.5× — `phase_3_4_ecs_via_wasm_ratio_meets_gate` (1.21× / 1.34×).
  - 1-hour memory soak — see §2.
  - Component preservation × 100 cycles — passed.
- **Phase 6 frame-graph + render substrate**: complete and production-consumed (`editor-shell::render_path:312`).
- **Phase 7.3 cad-projection**: Stable v0; gate CLOSED 2026-05-11 via seeded 1000-mutation umbrella test `phase_7_3_gate_closure_10_entities_100_edits_seed_0x7e5a_deae_3d49_c0e1`; 6-module split (3 Implemented + 3 Stub per PLAN §0.6 freeze policy).
- **D-Fillet (ADR-119 D1–D8)**: closed.
- **Tier-1 kernel cavity audit**: 15 crates, all 15 with `Failure class:` declaration. 10 IMPLEMENTED (`app` / `asset` / `audit-ledger` / `diagnostics` / `ecs` / `events` / `graph-foundation` / `plugin-host` / `schedule` / `types`); 4 doctrine cavity (`asset-streaming` / `asset-view` / `io-scheduler` / `job-system` — PLAN §1.6.5/§10.1 streaming-substrate cluster, explicit NON-GOALS per cavity); 1 admission-gated empty (`shared`); 0 empty stubs; 0 partial cavities.
- **MAIN-ORDERED 10-job audit queue**: CLOSED 2026-05-14 (`MAIN-ORDERED-QUEUE-CLOSEOUT_2026-05-14_15-52-45+0300.md`).
- **Protocol v2 Rule 7**: live across 9 substantive dispatches in the closing queue; zero duplicate Reviewer2 packets authored.

## 4. Known v0 deferrals (NOT blockers)

These are pressure-driven future work and explicitly NOT v0 blockers. They are listed here so a downstream reader does not mistake their absence for an oversight.

- **Editor-shell mock-event-loop perf harness** — the genuine outstanding measurement-gap deferral per `plans/BASELINE.md:248` / `IMPLEMENTATION.md:473`. End-to-end `EditorShell::render_frame` perf is unmeasured (frame-graph substrate IS measured).
- **`peak_rss` / `vss_delta` soak-harness improvement** — per `crates/script-bench/BASELINE.md`. Soak passes; the harness could report stronger evidence.
- **Four kernel doctrine cavities** — `job-system` work-stealing pool, `io-scheduler` actual I/O dispatch, `asset-view` WASM zero-copy mapping, `asset-streaming` residency manager. Each is an explicitly-scoped non-goal per PLAN §1.6.5 / §10.1; admission gate is live.
- **`compile.rs` legibility refactor** (29 KB, optional).

## 5. Reproducibility

The following commands, run from `A:\RCAD\RGE` (Windows) or the workspace root, reproduce this certification's cheap-gate results.

```powershell
$env:CARGO_HOME      = "A:\RustCache\cargo"
$env:CARGO_TARGET_DIR = "A:\RustCache\target"
$env:PATH            = "A:\RustCache\cargo\bin;$env:PATH"

cargo +nightly fmt --all -- --check
cargo run -p rge-tool-architecture-lints -- all
cargo test --workspace --no-fail-fast
```

Expected: `fmt` exits 0 with no diff; arch-lints prints `[<lint>] PASS (0 violations)` for all 9 enforcement lints plus the supplementary `snapshot-participate`; `cargo test` exits 0 with 333 suites OK, 2549 passed, 0 failed, 20 ignored.

## 6. Sign-off

v0 is certified at commit `6aaf7f1` against the gate set above. Future work is pressure-driven and listed in §4; none of it is a v0 blocker. Any regression in §1 or new pressure on a §4 item is a separate dispatch.
