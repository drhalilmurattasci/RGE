# RUNTIME_ORCHESTRATOR

| Companion to | PLAN.md §6 (frame loop / runtime heartbeat) + PLAN.md §10.4 (Tier-2 dogfood rule) + PLAN.md §1.5.2 (sim/render thread separation) + PLAN.md §1.13 (failure-class taxonomy) + ADR-114 (PluginContext owned-resources-handoff design + 2026-05-08 four-substrate validation amendment) |
|---|---|
| Status | Pre-Phase-7 pattern doc; the four `runtime/runtime-{desktop,headless,mobile,web}` crates are stub binaries today (each is a `println!` with empty `[dependencies]`); this doc captures the orchestration **pattern** that the five Tier-2 plugin canaries (`cad-projection`, `gfx`, `physics`, `audio`, `editor-ui`) assume their orchestrator follows. The pattern is exercised today in each canary's `crates/<name>/tests/plugin_adapter_smoke.rs` integration suite as the test-side orchestration harness. |
| Audience | Phase-7 runtime authors building the actual orchestrator binary; reviewers verifying the canary-side contract; future Tier-2 canary authors needing to know what their orchestrator will do for them |
| Sibling doc | `KERNEL_APP_FRAME_LOOP.md` — the substrate the orchestrator runs on top of (`App::run_frame` is the per-frame entry); `KERNEL_PLUGIN_HOST_LIFECYCLE.md` — the host-side lifecycle the orchestrator drives via `init_all` / `tick_all` / `shutdown_all`; `PLUGIN_HOST_PATTERNS.md` — the plugin-author side of the contract; `PLUGIN_API.md` — the type-level surface the orchestrator stages resources against |
| Reference impls | `runtime/runtime-{desktop,headless,mobile,web}/src/main.rs` (current stubs) · `runtime/runtime-{desktop,headless,mobile,web}/Cargo.toml` (current empty manifests) · `crates/cad-projection/tests/plugin_adapter_smoke.rs` (orchestration-pattern reference, lines 87–171 — the `cad_projection_plugin_lifecycle_via_plugin_host` test) · `crates/gfx/tests/plugin_adapter_smoke.rs` (reference for lazy-build canary orchestration) · `crates/physics/tests/plugin_adapter_smoke.rs` · `crates/audio/tests/plugin_adapter_smoke.rs` · `crates/editor-ui/tests/plugin_adapter_smoke.rs` |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc captures the **runtime orchestrator pattern** as it stands pre-Phase-7. The actual orchestrator binary (one per platform tier) does not exist yet; each canary's plugin-adapter smoke test exercises the pattern as a test-side orchestrator. Phase-7 will lift this pattern into a real binary.

## 1. What "runtime orchestrator" means

> **Source-truth flag (load-bearing):** the dispatch spec asked this doc to document the orchestrator. Source-truth: `runtime/runtime-desktop`, `runtime/runtime-headless`, `runtime/runtime-mobile`, `runtime/runtime-web` are **stub binaries** today — each `src/main.rs` is a single `println!("rge-runtime-<x>: stub. Implementation pending per IMPLEMENTATION.md.")`, and each `Cargo.toml` declares an empty `[dependencies]` block. There is no live orchestrator binary in the workspace. This doc therefore **describes the pattern** (frozen by ADR-114 + the five canaries' test-side orchestration shape), not a concrete implementation. Phase-7 will translate the pattern into the real binaries.

The "runtime orchestrator" is the **binary-side composition layer** that:

1. Constructs a per-platform `App` via `kernel/app::AppBuilder` — the frame-loop driver substrate (`KERNEL_APP_FRAME_LOOP.md`).
2. Builds a `PluginHost` (from `kernel/plugin-host`) and registers the five Tier-2 plugin canaries in a deterministic order (§3).
3. Stages owned resources into a `PluginContext` per ADR-114's owned-handoff contract (§4).
4. Drives the frame loop, threading `PluginHost::tick_all(&mut ctx)` through `App::run_frame`'s per-phase runner closure (§5).
5. Shuts the host down LIFO when the loop terminates (§6).
6. Routes diagnostics through a workspace-wide `DiagnosticAggregator` (or platform-specific sink).

The orchestrator is **not** a kernel substrate — it lives in `runtime/*` (Tier-2-or-binary). It does **not** define new types; it composes the substrates. The kernel-side substrate it sits on top of is `kernel/app` (the frame-loop driver), which in turn does not know about plugins (the orchestrator is the layer that wires `PluginHost` into `App`'s per-phase runner).

## 2. Why a pattern doc pre-Phase-7

The four Tier-2 plugin canaries (`cad-projection` 2026-05-07, `gfx` + `physics` + `audio` 2026-05-08) landed before the orchestrator binary. ADR-114's amendment 2026-05-08 ("four-substrate validation") explicitly closed out the canary-side contract: the canaries assume an orchestrator that follows this pattern; if any of the four would diverge from a sibling, the design is wrong; conversely, if all four work uniformly with the pattern, the orchestrator can be a thin binary that mechanically applies it.

*[Editor-ui added 2026-05-10 as fifth canary post-orchestrator-design (per ADR-116 + cross-review #8's "tooling-observational participant" framing). The same pattern accommodated it without modification — confirming the four-substrate amendment's design hypothesis empirically.]*

The pattern is therefore **already exercised in production code** — not by a `runtime/*` binary, but by the test-side harness in each canary's `plugin_adapter_smoke.rs`. The `cad_projection_plugin_lifecycle_via_plugin_host` test (cad-projection lines 87–171) is the canonical reference; the remaining four canaries' lifecycle tests follow the same shape with different resource families.

This doc lifts the test-side harness into a documented pattern so Phase-7 has a frozen contract to translate.

## 3. The five plugin canaries — registration order

The five Tier-2 plugin canaries that constitute the substrate-validation set per ADR-114 amendment plus the 2026-05-10 editor-ui canary:

| Plugin | Crate | `PluginId` literal | Resource family staged for `tick` |
|---|---|---|---|
| `CadProjectionPlugin` | `crates/cad-projection` | `"rge-cad-projection.brep-handles-plugin"` | `World` + `CadGraph` + `Tolerance` |
| `GfxPlugin` | `crates/gfx` | `"rge-gfx.headless-triangle-plugin"` | `GfxContext` + `HeadlessTarget` (+ lazy `TrianglePipeline` on plugin struct) |
| `PhysicsPlugin` | `crates/physics` | `"rge-physics.fixed-step-plugin"` | `World` + `PhysicsInputLedger` |
| `AudioPlugin` | `crates/audio` | `"rge-audio.scheduling-plugin"` | `AudioManager` + `AudioFrame` |
| `EditorUiPlugin` | `crates/editor-ui` | `"rge-editor-ui.observational-canary"` | (tooling-observational; per ADR-116 cross-review #8 framing — no resources required) |

> **Source-truth flag:** the dispatch spec described the registration order as "cad-projection / gfx / physics / audio per ADR-114 amendments". Source-truth: this is the **landing chronology** (cad-projection landed first; gfx + physics + audio landed within the 2026-05-08 amendment; editor-ui added 2026-05-10) and is the natural registration order for tests. The orchestrator's actual registration order is **not load-bearing for correctness** — `PluginHost::register` validates uniqueness and stores in a `BTreeMap` keyed by id, so the lookup order is deterministic regardless of registration sequence. The **execution order** within `init_all` / `tick_all` follows the parallel `Vec<PluginId>` insertion-order side-table (forward direction); `shutdown_all` reverses it (LIFO). See `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §9 for the rationale.

The canonical Phase-7 registration block (mirroring the test pattern, generalised across all five canaries):

```rust
let mut host = PluginHost::new();
host.register(PluginId::new(CAD_PROJECTION_PLUGIN_ID),
              Box::new(CadProjectionPlugin::new()))?;
host.register(PluginId::new(GFX_PLUGIN_ID),
              Box::new(GfxPlugin::new()))?;
host.register(PluginId::new(PHYSICS_PLUGIN_ID),
              Box::new(PhysicsPlugin::new()))?;
host.register(PluginId::new(AUDIO_PLUGIN_ID),
              Box::new(AudioPlugin::new()))?;
host.register(PluginId::new(EDITOR_UI_PLUGIN_ID),
              Box::new(EditorUiPlugin::new()))?;
```

Each `<NAME>_PLUGIN_ID` constant is exported from the canary's `lib.rs` (e.g. `rge_cad_projection::CAD_PROJECTION_PLUGIN_ID`). Registration validates `Plugin::id() == registered_id` (rejected with `PluginHostError::IdMismatch` otherwise) + non-duplicate id (rejected with `DuplicateId`).

## 4. Resource staging into `PluginContext`

The orchestrator constructs a `PluginContext` and stages each canary's required resources before the lifecycle calls. The pattern from `cad_projection_plugin_lifecycle_via_plugin_host` (lines 110–130 of `crates/cad-projection/tests/plugin_adapter_smoke.rs`):

```rust
let mut diags = DiagnosticAggregator::new();
let mut ctx = PluginContext::new(&mut diags);

// init_all runs before resources are staged — init is a no-op for canaries
// using the canonical patterns. The lazy-build canaries (gfx) MUST NOT touch
// their resources until tick.
let init_report = host.init_all(&mut ctx);
assert!(init_report.failed.is_empty());

// Stage resources for the upcoming tick. Each insert<T>(value) returns
// Option<T> — None on first insert (per PLUGIN_API.md §2 insert semantics).
let _ = ctx.insert(world);
let _ = ctx.insert(cad_graph);
let _ = ctx.insert(tolerance);
let _ = ctx.insert(gfx_context);
let _ = ctx.insert(headless_target);
let _ = ctx.insert(physics_input_ledger);
let _ = ctx.insert(audio_manager);
let _ = ctx.insert(audio_frame);
```

The orchestrator owns the **construction** of each resource (per its platform's affordances — desktop has a real `wgpu::Device`; headless uses a mock; web ports through wasm-bindgen). It stages them via `PluginContext::insert::<T>`. The `BTreeMap<TypeId, Box<dyn Any + Send>>` registry is type-erased so the orchestrator's `kernel/plugin-host` dep does **not** need to import any Tier-2 type.

The `with_resource` builder variant (`PLUGIN_API.md` §2) supports the chained-construction shape:

```rust
let ctx = PluginContext::new(&mut diags)
    .with_resource(world)
    .with_resource(cad_graph)
    .with_resource(tolerance)
    /* ... */;
```

### Shared-resource convention

Two plugins (`cad-projection` and `physics`) both consume `World`. The orchestrator stages **one** `World` instance — both plugins `take<World>()` it, do work, and `insert<World>` it back. The `BTreeMap<TypeId, _>` keying ensures both see the same staged instance; the put-back invariant (`PLUGIN_HOST_PATTERNS.md` §6) ensures the resource survives across both plugins' tick bodies.

The host's `tick_all` walks plugins in registration order; each plugin's tick is sequenced (no parallelism in v1), so the cad-projection tick puts `World` back **before** the physics tick takes it. This is correct by construction — the single-threaded synchronous execution model preserves the per-resource hand-off invariant trivially.

## 5. Frame-loop integration

The orchestrator's frame body composes `App::run_frame` (`KERNEL_APP_FRAME_LOOP.md` §4) with `PluginHost::tick_all`. The canonical shape:

```rust
let mut app = AppBuilder::new()
    .fixed_dt(1.0 / 60.0)
    .max_fixed_steps(8)
    .frame_budget(1.0 / 60.0)
    .build();

let mut frame_dt = 1.0 / 60.0; // measured per-frame in real binary

loop {
    app.run_frame(frame_dt, &mut diags, |phase, ctx_frame, sink| {
        match phase {
            FramePhase::Input        => /* drain platform events */ (),
            FramePhase::FixedSim     => {
                // Run fixed-step systems N=ctx_frame.fixed_steps_this_frame times.
                // Plugins like physics whose work is per-fixed-step go here.
                for _ in 0..ctx_frame.fixed_steps_this_frame {
                    /* fixed-step plugin tick batch */
                }
            }
            FramePhase::Update       => {
                // The plugin-host tick. PluginContext is held by the
                // orchestrator's outer scope; tick_all walks the five canaries.
                let _tick_report = host.tick_all(&mut ctx);
            }
            FramePhase::LateUpdate   => /* late update systems */ (),
            FramePhase::StageRender  => /* render-snapshot capture (Phase 5) */ (),
            FramePhase::EndFrame     => /* diagnostics flush; per-frame stats */ (),
        }
    });

    if shutdown_requested() { break; }
}

// LIFO shutdown.
let _shutdown_report = host.shutdown_all(&mut ctx);
```

> **Source-truth flag:** the dispatch spec described the integration as "FramePhase × FixedStepAccumulator". Source-truth: the `FixedStepAccumulator` (per `KERNEL_APP_FRAME_LOOP.md` §3) is **inside** `App::run_frame` — the orchestrator does **not** invoke it directly. The accumulator's output is surfaced as `FrameContext::fixed_steps_this_frame` (a `u32`) and `FrameContext::fixed_alpha` (`f64` in `[0, 1)`), passed to the per-phase runner closure. The orchestrator reads `ctx_frame.fixed_steps_this_frame` to drive the inner loop in the `FramePhase::FixedSim` arm. The Fiedler-pattern accumulation, the death-spiral cap, and the alpha computation are all internal to `App`; the orchestrator just consumes the per-frame `FrameContext` value.

### Where each canary's tick fires

The mapping from canary work to `FramePhase`:

- **`PhysicsPlugin`** — fixed-step. The orchestrator can either (a) put physics into the `Update` phase and let `tick_all` drive it once per frame at variable rate, or (b) custom-route it into `FixedSim` for `ctx_frame.fixed_steps_this_frame` invocations per frame. The canary's `physics_step` is internally infallible (see `PLUGIN_HOST_PATTERNS.md` §5 no-`RuntimeFault` subcase) so either routing works; the v1 pattern uses (a) with `host.tick_all` once per frame and lets the canary track step-budget internally.
- **`CadProjectionPlugin`** — variable-rate. Belongs in `Update`; recomputes `BRepHandle::mesh_id` against the latest `CadGraph` checkpoint.
- **`GfxPlugin`** — variable-rate. Belongs in `Update` today (Phase 6.1 is single-threaded headless); future Phase-5 sim/render thread separation will move the GPU-touching record into `StageRender` while keeping the frame-data preparation in `Update` (per PLAN §1.5.2 + `GFX_RENDER_TIER.md`).
- **`AudioPlugin`** — variable-rate. Belongs in `Update`; appends one `AudioFrame` record per tick.
- **`EditorUiPlugin`** — variable-rate, observational-only (no resources required). Belongs in `Update`; increments observation counters per tick per ADR-116 / cross-review #8 "tooling-observational participant" framing.

The five canaries today all sit on the single `host.tick_all` call in `Update`; the per-canary phase routing is a Phase-5+ refinement.

## 6. Failure isolation per PLAN §1.13

The orchestrator inherits the host's failure-isolation guarantees. Per `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §10:

- **A plugin's `init` failure → that plugin marked `Failed`; orchestrator continues.** `init_all` aggregates failures into `InitReport::failed`; the orchestrator surfaces them through the diagnostic stream but does NOT abort.
- **A plugin's `tick` failure → that plugin marked `Failed`; subsequent `tick_all` skips it.** The orchestrator's frame loop continues; other plugins still tick.
- **A plugin's `shutdown` failure → reported in `ShutdownReport::failed`; orchestrator continues teardown.** LIFO walk completes regardless of individual plugin failures.
- **A plugin panic → caught by `catch_unwind`; that plugin marked `Failed`; orchestrator continues.** The host's `AssertUnwindSafe(...)` shield isolates the panic at the plugin-frame boundary.

The orchestrator's role in this contract: pass a **non-aborting** `DiagnosticSink` (e.g. `DiagnosticAggregator`, not a sink that panics on `emit`). The host auto-emit policy handles the per-failure severity classification (see `KERNEL_DIAGNOSTICS.md` §9).

### Kernel-fatal escalation

Some failures are **not** plugin-isolated:

- **`audit-ledger` checksum failure** (per `KERNEL_AUDIT_LEDGER.md` §9) — kernel-fatal. The orchestrator must terminate the loop and surface the failure; PIE state cannot be trusted.
- **Scheduler deadlock** (per `KERNEL_SCHEDULE.md` §10) — kernel-fatal. Phase 1.5 schedule is single-threaded synchronous, so this is defensive for a future async-execution path.

The orchestrator's failure-path triage:

```rust
// Pseudocode for the aggregate failure check at frame boundary.
if diags.has_errors() {
    if any_diagnostic_carries(FailureClass::KernelFatal) {
        // Snapshot-restore is the only recourse. Exit the loop.
        break;
    }
    // Plugin-fatal / recoverable / snapshot-recoverable: continue,
    // surface the diagnostics to the user / CI, retry next frame.
}
```

## 7. Diagnostic routing

The orchestrator owns the workspace's primary `DiagnosticSink`. The pattern:

```rust
let mut diags = DiagnosticAggregator::new();
// ... run frames, ticking plugins through PluginContext::new(&mut diags)
//     so every per-plugin emit lands in this aggregator.
// At frame boundary or shutdown:
for diag in diags.iter() {
    // Route to platform-specific surface: CLI stderr, editor UI overlay,
    // CI log file, structured-log adapter.
}
```

`DiagnosticAggregator` is the v1 default (`KERNEL_DIAGNOSTICS.md` §8). Future orchestrators may layer streaming sinks (CI log streamer, editor's live-warning surface) by wrapping `DiagnosticAggregator` in a tee — the trait is object-safe so composition is straightforward.

The plugin-host's auto-emit policy (`KERNEL_DIAGNOSTICS.md` §9) means the orchestrator does **not** need to manually translate `PluginError` into diagnostics — every `Err` / panic / leak path emits a structured `Diagnostic` automatically with the right severity. The orchestrator just iterates `diags` and surfaces.

## 8. Per-platform considerations (Phase-7 scope)

The four `runtime/runtime-{desktop,headless,mobile,web}` crates are stub binaries today. Phase-7 will fill each in:

- **`runtime-desktop`** — Win/macOS/Linux native binary. Drives the loop on `winit` events; `gfx` plugin gets a real `wgpu::Device` from the platform surface; `audio` plugin gets the cpal backend. Per-platform window sizing, fullscreen, input.
- **`runtime-headless`** — cook + dedicated server. No window; `gfx` plugin runs on the headless / mock backend (today's `GfxContext::new_headless()`); `audio` plugin on `kira::backend::mock::MockBackend`. Used for CI replay + golden tests + asset cooking.
- **`runtime-mobile`** — iOS/Android. Adds touch-event drainage in `FramePhase::Input`; platform-specific lifecycle (suspend / resume) maps onto `init_all` / `shutdown_all`.
- **`runtime-web`** — wasm32 via wasm-bindgen. Single-threaded; `requestAnimationFrame`-driven outer loop calling `app.run_frame`; `gfx` uses WebGPU backend; `audio` uses Web Audio.

The pattern is **identical** across the four — same plugin registration, same resource staging, same frame-loop integration, same failure-isolation contract. The platform differences live entirely in resource construction (which plugins receive their staged resources) and the outer-loop event source (`winit` vs `requestAnimationFrame` vs Android lifecycle callbacks). The pattern this doc captures is what makes that uniformity possible.

## 9. Test-side orchestration as the canonical reference

Until Phase-7 lands the binary, each canary's `plugin_adapter_smoke.rs` is the working orchestrator. The five canary lifecycle tests:

| Canary | Test name (line range) |
|---|---|
| cad-projection | `cad_projection_plugin_lifecycle_via_plugin_host` (lines 87–171) |
| gfx | `gfx_plugin_lifecycle_via_plugin_host` (lines 84+) |
| physics | `physics_plugin_lifecycle_via_plugin_host` |
| audio | `audio_plugin_lifecycle_via_plugin_host` |
| editor-ui | `editor_ui_plugin_full_lifecycle_through_plugin_host` |

Each one mirrors the same skeleton:

1. Construct the plugin (and the plugin's required resources outside the context).
2. `PluginHost::new()` + `host.register(PluginId, Box<plugin>)`.
3. `DiagnosticAggregator::new()` + `PluginContext::new(&mut diags)`.
4. `host.init_all(&mut ctx)` + assert no failures.
5. Stage resources via `ctx.insert(...)` (editor-ui requires no resources per its observational-only shape).
6. `host.tick_all(&mut ctx)` + assert ticked count + assert plugin-specific output (e.g. `BRepHandle::mesh_id` populated; `AudioFrame` record appended; `frames_recorded` incremented).
7. `ctx.take::<T>()` + verify resources still present (the put-back invariant).
8. `host.shutdown_all(&mut ctx)` + assert clean teardown (LIFO walks the single registered plugin).

Phase-7 generalises this to N=5 plugins at once. The pattern is mechanical; the only subtlety is the per-platform resource construction (covered in §8).

### Multi-plugin isolation pattern

Each canary additionally has a `*_isolation_with_sibling_*` test that registers a `PanickingTickPlugin` fixture alongside the canary, runs `tick_all`, and asserts:

1. The host's `catch_unwind` recovers from the sibling's panic.
2. The sibling is marked `Failed`.
3. The canary ticks successfully alongside it.
4. The diagnostic stream contains an `Error`-severity panic diagnostic attributable to the sibling.
5. Resources staged for the canary are still in the context post-tick.

This is the canonical multi-plugin isolation test (`PLUGIN_HOST_PATTERNS.md` §7). The Phase-7 orchestrator inherits this guarantee mechanically — the pattern is enforced by the host, not by the orchestrator.

## 10. Layering invariants

- **The orchestrator MAY depend on every Tier-2 canary crate.** It has to import each plugin's struct + `<NAME>_PLUGIN_ID` constant + each plugin's resource types (`World`, `CadGraph`, `GfxContext`, `HeadlessTarget`, etc.) to construct them.
- **The orchestrator MAY depend on `kernel/app` + `kernel/plugin-host` + `kernel/diagnostics`.** The frame loop driver, the host, the diagnostic substrate.
- **The orchestrator MUST NOT define new substrate types.** New types belong in their own Tier-1 or Tier-2 crate, not in the orchestrator binary. The orchestrator is composition-only.
- **The orchestrator MUST NOT bypass the owned-handoff contract.** Every resource flows through `PluginContext::insert` / `take` / `with_resource`; no plugin holds a long-lived `&mut World` across host calls. This is the invariant `KERNEL_PLUGIN_HOST_LIFECYCLE.md` §6 enforces.

## 11. Forward-compatibility — what changes in Phase-7+

When the orchestrator binary lands:

- **Cargo deps populated.** `runtime/runtime-*/Cargo.toml`'s empty `[dependencies]` block fills with the five canary crates + `kernel/app` + `kernel/plugin-host` + platform-specific bindings (`winit`, `wasm-bindgen`, etc.).
- **Concrete resource construction.** The platform-specific code that builds `World` / `CadGraph` / `GfxContext` / `HeadlessTarget` / `PhysicsInputLedger` / `AudioManager` / `AudioFrame` lands in `runtime/runtime-*/src/setup.rs` (or analogous). The canaries' construction patterns are already established in their `plugin_adapter_smoke.rs`.
- **Outer-loop driver.** Each platform's outer loop (winit `EventLoop::run`, wasm `requestAnimationFrame`, etc.) wraps `app.run_frame` per §5.
- **Diagnostic surface.** The aggregator drains into the platform-specific surface (CLI stderr; editor overlay; CI structured log).
- **Render-thread separation (Phase 5+).** The current single-threaded body splits into a sim thread and a render thread per PLAN §1.5.2; the orchestrator stages a `PieSnapshot` ring at `FramePhase::StageRender` (`PIE_SNAPSHOT.md`); the render thread reads from the ring.

The pattern this doc captures is the **stable contract**. Phase-7's translation is mechanical; the canaries shouldn't need any change.

## 12. References

- **PLAN.md §6** — frame loop / runtime heartbeat. The substrate the orchestrator drives.
- **PLAN.md §10.4** — Tier-2 dogfood rule. The five canaries use the unified `Plugin` trait per this rule.
- **PLAN.md §1.5.2** — sim/render thread separation. Phase-5+ work; the orchestrator's `StageRender` phase is the seam.
- **PLAN.md §1.13** — failure-class taxonomy; plugin-fatal isolation + kernel-fatal escalation.
- **ADR-114** — owned-handoff substrate design. The 2026-05-08 amendment ("four-substrate validation") is the load-bearing reference: cad-projection / gfx / physics / audio uniform validation.
- **`KERNEL_APP_FRAME_LOOP.md`** — sibling §18 doc; the substrate the orchestrator runs on top of. `App::run_frame` is the per-frame entry; `FramePhase` enumerates the six phases the per-phase runner closure dispatches to.
- **`KERNEL_PLUGIN_HOST_LIFECYCLE.md`** — sibling §18 doc; host-side machinery. The orchestrator drives `init_all` / `tick_all` / `shutdown_all`; the host owns the state machine + `catch_unwind` shield + leak-detection diff.
- **`PLUGIN_HOST_PATTERNS.md`** — sibling §18 doc; plugin-author side. The orchestrator's resource-staging contract is the dual of the canary's take/work/insert pattern.
- **`PLUGIN_API.md`** — sibling §18 doc; the `Plugin` trait + `PluginContext` API surface.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; per-frame diagnostic routing through `DiagnosticAggregator`.
- **`KERNEL_AUDIT_LEDGER.md`** — sibling §18 doc; kernel-fatal-class escalation path the orchestrator triages.
- **`KERNEL_SCHEDULE.md`** — sibling §18 doc; per-system scheduler that the orchestrator may layer between `host.tick_all` and individual systems.
- **`PIE_SNAPSHOT.md`** — sibling §18 doc; the future `StageRender` phase consumer (Phase 5+).
- **`runtime/runtime-desktop/{Cargo.toml,src/main.rs}`** — stub binary today.
- **`runtime/runtime-headless/{Cargo.toml,src/main.rs}`** — stub binary today.
- **`runtime/runtime-mobile/{Cargo.toml,src/main.rs}`** — stub binary today.
- **`runtime/runtime-web/{Cargo.toml,src/main.rs}`** — stub binary today.
- **`crates/cad-projection/tests/plugin_adapter_smoke.rs`** — canonical orchestration-pattern reference (lines 87–171: `cad_projection_plugin_lifecycle_via_plugin_host`).
- **`crates/gfx/tests/plugin_adapter_smoke.rs`** — lazy-build canary orchestration reference.
- **`crates/physics/tests/plugin_adapter_smoke.rs`** — fixed-step canary orchestration reference.
- **`crates/audio/tests/plugin_adapter_smoke.rs`** — audio canary orchestration reference (kira mock backend).
- **`crates/editor-ui/tests/plugin_adapter_smoke.rs`** — editor-ui (5th) canary orchestration reference (tooling-observational; no resources required).
