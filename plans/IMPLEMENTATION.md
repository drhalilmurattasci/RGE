# RGE — Recommended Implementation Order (v0.8)

> **Status:** Implementation plan. Companion to [`PLAN.md`](./PLAN.md) (architecture, frozen at v0.8) and [`WAVES.md`](./WAVES.md) (21-wave parallel dispatch).
>
> **Drafted:** 2026-05-05.
>
> **Relationship to other docs:**
> - **PLAN.md** = *what* the architecture is. Frozen at v0.8.
> - **WAVES.md** = *parallel* dispatch view (21 crates, 3 calendar days at full parallelism). Optimized for throughput once architecture is proven.
> - **IMPLEMENTATION.md (this doc)** = *sequential de-risking* view. Optimized for *killing the highest-risk assumptions first*. The two views overlap — WAVES.md is what you do *after* the critical path validates, but most of the early waves are blocked behind de-risking gates documented here.

---

## 0. Why this order

**The primary goal of early implementation is NOT features. It is de-risking architectural assumptions.**

Specifically:
- **WASM hot-reload** — constitutional load-bearing claim; if it doesn't work, ADR-077 escape clause activates and the architecture rewrites
- **Reflection schema** — every later subsystem (editor inspector, hot-reload migration, scripting bridge, asset metadata) depends on `kernel/types`
- **cad-projection** — the CAD/ECS bridge from v0.6's biggest catch; if invalidation economics are bad, the whole CAD pillar is in trouble
- **PIE snapshot/restore semantics** — without this, editor/runtime unification claim collapses
- **Invalidation economics** — projection invalidation density (§1.10.4 entropy metric); could detonate cad-projection
- **Editor/runtime integration** — Command Bus + editor-state coordination-not-authority rule (§1.15)

If any of these fail, *we want to know in week 12, not week 48.*

Order strategy:
1. Establish irreversible foundations (architecture enforcement before code)
2. Validate highest-risk assumptions early (WASM hot-reload by week 12)
3. Delay expensive sophistication (renderer, marketplace, XR, advanced graphs)
4. Aggressively collapse unknowns (each phase has an "abort condition" — what triggers a replan)

---

## 1. The Critical Path

If you compress everything into the irreducible chain:

```
kernel/types (reflection)
  → kernel/ecs (entities, relations, Changed<T>)
    → editor-actions (Command Bus + undo)
      → runtime-wasmtime + script-host (WASM hot-reload — THE constitutional bet)
        → editor-shell + PIE (snapshot/restore validates editor/runtime split)
          → render-snapshot staging (sim/render thread separation)
            → cad-core MVP (operator graph + checkpoints)
              → cad-projection (CAD/ECS bridge)
                → topology lineage (persistent IDs survive rebuilds)
                  → graph-foundation (last; pressure-tested by 3+ graph types)
```

**Everything else is secondary.** Renderer features, marketplace governance, advanced editor tooling, multiplayer, XR, console targets — all of these depend on the critical path validating first.

---

## 2. Phase-by-phase order

### Phase 0 — Bootstrap & Constraints (Week 1)

**Goal:** make future chaos impossible.

#### 0.1 Workspace skeleton

Cargo workspace with all 70+ crate directories created (per [WAVES.md §0](./WAVES.md)). Stub `Cargo.toml` + `src/lib.rs` per crate. Workspace root manifest. `.gitignore`.

Tools wired:
- `clippy` (allow-on-PR, enforce on main)
- `rustfmt`
- `cargo-deny` (license + audit + duplicate detection)
- `cargo-udeps` (unused-dep)
- `cargo-llvm-lines` (monomorphization tracking)

#### 0.2 Architecture enforcement FIRST

**Before any engine code.**

CI enforces from day one:
- Forbidden-dependency DAG validator (`scripts/dep-dag.rs`)
- `// SPLIT-EXEMPTION` lint (no `.rs` file >1000 lines without justification)
- No `utils.rs` / `helpers.rs` lint
- graph-foundation usage lint (no graph system reinvents NodeId / EdgeId / hashing primitives)
- editor-state ownership lint (no subsystem owns `Selection`/`Hover`/`ActiveTool` outside `editor-state`)
- Command Bus mutation lint (editor mutations route through `editor-actions`)
- coordination-not-authority lint (`editor-state` doesn't import authoritative content types)

**Rationale:** architecture without enforcement decays immediately. Putting CI in place *before* the code prevents accumulation of violations that are expensive to retrofit.

#### 0.3 Golden test projects

`tests/golden-projects/` with stub fixtures (per [WAVES.md W21](./WAVES.md)):
- `simple-scene` (basic load, transform, camera + light)
- `material-zoo` (PBR, unlit, skinned, blend-shape, B-Rep tessellated)
- `skinned-character` (glTF, skeleton, anim, skinning)
- `physics-puzzle` (rigid bodies, joints, deterministic replay)

These become perf baselines, regression harnesses, serialization tests, snapshot tests, hot-reload tests. **Do not delay this** — every later phase tests against these.

#### Exit criteria

- `cargo build --workspace` produces all 70+ stub crates (with empty `lib.rs`)
- All CI lints active and passing on stubs
- Golden fixtures exist (empty but valid project schemas)

#### Abort condition

If CI architecture enforcement can't be made to work in week 1: the architecture is too elaborate to enforce. Cut subsystems before continuing.

---

### Phase 1 — Kernel Spine (Weeks 2–5)

**Goal:** create the minimum irreversible substrate.

#### 1.1 `kernel/types` — FIRST REAL CRATE

The architectural root. Everything depends on this.

Implement:
- `TypeId` system (interned, stable across builds)
- Reflection registry (`#[rge::reflect]` derive — initially without UI hints, just round-trip serde)
- `UiHint` enum (closed-set per §6.15)
- Schema versioning (every reflected type carries a schema version)
- Serialization bridge (RON serde via reflection walk)

Pilot type: `RenderPass` (existing in `rustforge/apps/editor-app/`). Round-trip test: serialize → deserialize → byte-identical.

**High risk:** reflection explosion (compile time, monomorphization, binary size).

**Validate early:**
- `cargo-llvm-lines` baseline on a single reflected type
- Extrapolate to ~100 reflected types (rough estimate)
- If projected compile-time impact > §13.3 budget, redesign before adding more reflected types

PLAN.md cross-ref: §1.2.4 (zero-copy asset views consumes), §6.15 (UI hints), ADR-022.

#### 1.2 `kernel/diagnostics`

Implement immediately after `kernel/types`. Every later subsystem emits diagnostics from day one.

- `Span` types (source location, graph node ID, script line, asset path)
- Structured diagnostics (`miette`/`ariadne`-style)
- Aggregation (don't fail-fast)
- Severity (error/warning/info/suggestion)
- Failure-class declarations (per §1.13 — recoverable / snapshot-recoverable / plugin-fatal / session-fatal / kernel-fatal)

PLAN.md cross-ref: §1.7, §1.13.

#### 1.3 `kernel/events`

Minimal typed event bus. **Do not overbuild.**

- Typed event channels
- Subscriptions
- Frame-queued delivery
- Diagnostics integration

No distributed/event-sourcing fantasies. No async-everywhere.

#### 1.4 `kernel/app`

Main loop skeleton:
- Frame phases
- Sim/update separation (skeleton — full render-snapshot staging comes in Phase 5)
- Fixed timestep
- Diagnostics hooks

The runtime heartbeat.

#### 1.5 `kernel/schedule`

**Do not build a fancy scheduler first.** Boring, observable, deterministic.

- Ordered stages (early-update, update, late-update, fixed-update)
- Deterministic ordering (sorted, no HashMap iteration)
- Async boundaries (declared, not opaque)
- Dependency edges between systems

Fancy parallelism comes later — only if benchmarks demand it.

#### Exit criteria

- `kernel/types` round-trips a non-trivial reflected type via RON
- Diagnostic span info renders correctly in console output
- `kernel/app` runs a 60Hz fixed-step loop with no allocations after warmup
- `kernel/schedule` runs 10 systems in deterministic order

#### Abort condition

If `kernel/types` reflection compile-time is already >30s on 5 pilot types, **STOP and replan reflection strategy** before proceeding. Reflection is the architectural root; it cannot be slow.

---

### Phase 2 — ECS & Command Bus (Weeks 5–8)

**Goal:** prove runtime mutation semantics.

#### 2.1 `kernel/ecs`

Minimal first. **Do NOT implement** at this stage:
- Replication
- Advanced query optimizer
- Fancy parallel iteration
- Deferred magical mutation systems

**DO implement:**
- Entities (ULID-based EntityId)
- Archetypes (basic, dense column-store)
- Component storage (specialized: tree / dense-linear / sparse — graph-foundation NodeId-only)
- Relations (`parent_of`, `bone_of` are sufficient; others come later)
- `Changed<T>` (per-archetype mutation generation counters)

Smoke test: spawn 100k entities, iterate `Transform`, mutate, verify `Changed<T>`.

PLAN.md cross-ref: §1.2 (substrate), §1.2.2 (relations), §1.2.3 (change detection).

#### 2.2 `editor-actions` (Command Bus) — VERY EARLY

Core architecture. Implement *before* editor tooling, inspectors, gizmos, graph editors.

- `Action` trait (apply / revert / merge / name)
- `UndoStack` (entries, cursor, save mark)
- `CompoundAction` (atomic cross-subsystem)
- 500ms-window same-target coalescing
- Audit-ledger projection

Smoke test: spawn entity → modify component → undo → verify byte-identical state.

PLAN.md cross-ref: §6.16 (Command Bus + UndoStack).

#### 2.3 `kernel/audit-ledger`

Minimal append-only first.

- Event recording (Action events + future CadCheckpoint events)
- Undo stream projection
- Replay
- Deterministic event IDs (BLAKE3 over event payload)

Validates replay assumptions, snapshot assumptions, hot-reload restore semantics.

#### Exit criteria

- 100k-entity scene mutates and undoes byte-identically
- Audit-ledger replay produces identical world state from event log
- Command Bus is the only mutation path (CI lint enforces)

#### Abort condition

If undo round-trip fails byte-identical correctness on a non-trivial component shape: there's a hole in the reflection schema or the Command Bus model. Fix before proceeding to Phase 3.

---

### Phase 3 — WASM Validation (Weeks 8–12) — CRITICAL PHASE

**This is the single highest-risk area in the entire architecture.** WASM hot-reload is the constitutional load-bearing claim. If it doesn't work, the architecture rewrites.

#### 3.1 `runtime-wasmtime` + `runtime-wasmtime-engine`

- Module loading (wasmtime + Cranelift JIT)
- Capability gates (effect specifiers — already designed in rustforge)
- Function invocation
- Memory limits (per-plugin caps)
- Panic containment (a crashed module doesn't kill the editor)

Activate the deferred `engine_wasmtime` feature flag (currently off in rustforge).

#### 3.2 `script-host`

Very small initially.

- ECS bridge (WIT-typed query, `Changed<T>` observer)
- Asset view (zero-copy WASM linear memory mapping for read-only buffers)
- Event hooks (subscribe to `kernel/events`)

PLAN.md cross-ref: §5.4 (ECS bridge), §1.2.4 (asset views).

#### 3.3 Hot-reload prototype — CRITICAL

**Implement the FULL LOOP:**

```
edit Rust source
  → cargo build (background tokio task, ~1–3s incremental)
    → wasmtime::Module::new (~50ms)
      → reflect-roundtrip migrate component data
        → instance swap
          → continue sim from next tick
```

Smoke test: edit a gameplay system that mutates `Transform`, save, verify the new behavior takes effect within p95 <100ms without losing component data.

**If this fails:** major architecture changes may still be needed. Possible failure modes:
- Reflect-roundtrip migration is too slow (replan reflection schema)
- wasmtime swap latency exceeds budget (replan to AOT-only model)
- Component shape changes break migration (replan schema versioning)
- Hot-reload during physics tick corrupts state (replan stall semantics)

This phase is where the §0.6 freeze policy gets its first real test: if hot-reload fundamentally doesn't meet budget, the constitution ADR-077 escape clause may activate.

#### 3.4 `script-bench`

Immediately benchmark:
- Hot-reload swap latency (p95 budget <100ms)
- ECS iteration via WASM (target: within 1.5× native Rust)
- WASM call overhead (per-frame tick)
- Memory growth across hot-reload cycles
- Reload jitter over 1-hour session

**Do not wait until later.** These numbers determine whether the "fastest script engine" pillar is viable.

PLAN.md cross-ref: §5.6 (benchmarks), §5.7 (script tooling).

#### Exit criteria

- Hot-reload p95 < 100ms on a 1000-entity scene
- ECS iteration via WASM ≤ 1.5× native Rust
- 1-hour session without memory leak
- Component data preserved across 100 hot-reload cycles

#### Abort condition

If hot-reload p95 > 500ms after optimization: the constitutional bet may be unrealistic. **Trigger ADR-077 review.** Options: AOT-only with stop-and-relaunch (no hot reload), or runtime escape clause activated.

---

### Phase 4 — Asset & Serialization Spine (Weeks 12–15)

**Goal:** stabilize identity and persistence.

#### 4.1 `kernel/asset`

- `AssetId` (`blake3:<hash>` content-addressed)
- Handles (typed, ref-counted)
- Registry (in-memory + disk-backed)
- Dependency tracking (asset A depends on assets B, C)

#### 4.2 `pak-format`

- Header (magic `RGEP`, version, flags, compression algo)
- Sorted asset index (BLAKE3-keyed)
- zstd compression
- Deterministic byte-identical output (CI gate per §13.4)
- Signatures deferred to Phase 8+ (Ed25519 placeholder)

#### 4.3 RON serialization (`rge-data`)

- Project (`.rge-project`), Scene (`.rge-scene`), Prefab (`.rge-prefab`) schemas
- Schema migration (`version:` field, registered migrations)
- Stable formatting (deterministic field order, fixed indentation)
- Round-trip tests on golden fixtures

**Validate:** human-editable source remains viable. If RON files become too verbose to edit by hand at realistic scale (>5MB scene file), reconsider.

PLAN.md cross-ref: §1.6 (file format discipline).

#### Exit criteria

- Cook same source twice → byte-identical `.rge-pak` output
- Scene RON load → save round-trips byte-identically
- Asset content-addressing consistent across machines (blake3 same input → same ID)
- Schema migration v0.0 → v0.1 lossless on 5 fixture projects

#### Abort condition

If cook output is non-deterministic despite sorted iteration / pinned compression: there's a hidden source of non-determinism (HashMap iteration, parallel cook ordering, system clock leakage). Find and fix before declaring Phase 4 done.

---

### Phase 5 — Editor Skeleton (Weeks 15–20)

**Goal:** prove editor/runtime unification.

#### 5.1 `editor-shell`

Minimal:
- Viewport (one, no multi-viewport yet)
- Hierarchy panel (scene tree)
- Inspector panel (auto-generated from `#[rge::reflect]`)
- Diagnostics panel (consumes `kernel/diagnostics`)

Nothing fancy. No theme editor, no menu registry yet.

#### 5.2 `editor-state` (narrow per §1.15)

Implement ONLY:
- Selection (entity sets)
- Hover (per-panel)
- Active tool

**Delay** drag/drop and modal-state until needed by an actual feature. v0.8 may still slightly overcommit here — promote categories only on demonstrated 2-subsystem pressure (per §0.6 freeze policy).

#### 5.3 PIE — CRITICAL

Implement:
```
[Play] → ECS world snapshot (clone storage)
       → PlayState: Editing → Playing
       → editor systems pause; game systems unpause
[Stop] → restore snapshot → world byte-identical to pre-play
```

Validates:
- ECS snapshot/restore correctness
- Audit-ledger assumptions (audit log records play-mode events)
- Runtime/editor separation discipline (constitutional principle #8)
- editor-state persists across Play/Stop

PLAN.md cross-ref: §6.13 (PIE).

#### Exit criteria

- 100-entity scene loads, renders (placeholder), inspector shows component fields
- Play → 60 ticks → Stop → world byte-identical to pre-play
- Selection persists across Play/Stop cycle
- Hot-reload during Play doesn't crash editor

#### Abort condition

If PIE snapshot/restore exceeds 500ms on a 10k-entity scene: ECS storage layout needs redesign. Possible: switch to copy-on-write columns, or commit to diff-mode by default (per §6.13.2).

---

### Phase 6 — Rendering Baseline (Weeks 20–28)

**Goal:** make the editor visually usable.

#### 6.1 `gfx` minimal

- wgpu init (Vulkan on Win/Linux, Metal on macOS)
- Frame-graph (minimal — transient resource lifetimes computed at frame begin)
- Mesh rendering (vertex/index buffers, draw calls)
- Transforms (uploaded as uniforms or instance buffers)
- PBR-lite (single-light Lambert + Phong; full PBR comes later)

**NOT this phase:**
- Bindless
- TSR
- Nanite-like
- Virtualized geometry
- Giant shader permutation systems
- Lumen
- VSM

#### 6.2 Render-snapshot separation — IMPORTANT

Validate sim-thread / render-thread separation early (per §1.5.2).

- Render thread reads frozen `WorldSnapshot{N}`
- Sim thread mutates state for `N+1`
- Atomic snapshot swap on tick boundary

Without this validated, B-Rep + cluster systems detonate later.

PLAN.md cross-ref: §1.5.2, ADR-080.

#### 6.3 `material-runtime`

Minimal:
- Material parameters (uniform buffers)
- Shader compile (WGSL + naga)
- Pipeline cache (PSO keyed on shader+vertex layout)

**Delay material graph editor.** That's Phase 7+ and depends on graph-foundation (Phase 8).

#### Exit criteria

- 60fps on `simple-scene` golden project (1k cubes, 1 directional light) **[CLOSED 2026-05-11 on recorder host only: NVIDIA GeForce RTX 4060 Ti / Vulkan / DiscreteGpu / 1280×720 / static camera / shared PSO + 1 material / option-(a) single `draw_indexed`; min-of-3 P95 = 0.112 ms (~150× under 16.67 ms gate); NOT universal, NOT vendor parity, NOT cold-start, NOT thermal, NOT CI; see BASELINE.md §6.3]**
- Editor frame time idle ≤ 8ms (matches §13.2 gate) **[CLOSED 2026-05-11 for CPU-idle interpretation: empty-shell P95 = 0.000047 ms; loaded re-measure deferred; see BASELINE.md §13.2]**
- Render-thread sees stable snapshot; sim-thread mutations don't race **[CLOSED 2026-05-11 for ADR-117 `RenderHandoff` boundary invariant: held `Arc<RenderInputOwned>` stable across subsequent publishes; latest-only / drop-old; `(ecs_tick, checkpoint_id)` anchor preserved; single-threaded proxy today (PLAN §13.6); future dedicated renderer thread must keep the same invariant; does not certify a full render-thread architecture yet; see `crates/editor-shell/tests/render_input_boundary.rs::gate_c_held_snapshot_stable_across_subsequent_publishes` + ADR-117]**
- 100 material instances share one PSO (variant cache hit)

#### Abort condition

If render-snapshot overhead exceeds 5% of frame budget: the staging architecture is wrong. Possible: smaller snapshot scope, or move some state out of snapshot path.

---

### Phase 7 — CAD Spike (Weeks 28–40) — HIGHEST SECONDARY RISK

**Many architectures die here.** This is where v0.6's CAD/ECS impedance fix gets tested by reality.

#### 7.1 `cad-core` MVP

Implement ONLY:
- Operator graph (DAG, built on graph-foundation primitives — but graph-foundation is still pending; use direct types and migrate later)
- Three operators: Extrude, Revolve, Boolean
- Checkpointing (begin_operation / commit / rollback / restore_to)
- Tessellation cache (keyed on cad_node_id + tolerance)

**NOT this phase:**
- Full constraint solver
- Advanced healing strategies
- Collaborative editing
- Full operator library (Fillet, Loft, Sweep, Shell come later)
- Full topology tooling

#### 7.2 Persistent topology IDs

Validate:
- Rebuild stability (same operator chain produces same IDs)
- Save/load stability (IDs survive round-trip through `.rge-pak`)
- Projection stability (cad-projection sees consistent IDs frame-to-frame)

Smoke test: build 100 operator chains; rebuild each 10 times with random small parameter changes; verify face/edge IDs preserved per `TopologyEvolution` enum.

#### 7.3 `cad-projection` minimal

- `BRepHandle(CadRef)` component
- Tessellation projection (cad-core tessellation → ECS-side mesh handle)
- ECS entity ↔ cad-core node mapping
- Invalidation on cad-core commit

This validates the CAD/ECS bridge for real.

PLAN.md cross-ref: §1.5.4 (cad-core architecture), §1.5.4.5 (cad-projection internal split).

#### 7.4 Topology lineage prototype

Per §1.5.4.3. One of the most novel systems in the architecture — prototype early to surface unknowns.

```rust
enum TopologyEvolution {
    Preserved, Split(...), Merged(...), Deleted, Reinterpreted,
}
```

Test on Boolean operations (where topology splits/merges most aggressively).

#### Exit criteria

- 100 random parametric edits on 10 B-Rep entities preserve face/edge IDs correctly
- cad-projection invalidation triggers ECS update within one tick
- Cook of B-Rep scene → load → cad-core graph identical
- Triangle fallback always available (per §4.2)

#### Abort condition

If persistent topology IDs are unstable across rebuilds despite lineage tracking: the entire CAD pillar is at risk. Options: scale back B-Rep to "tessellate-once-no-rebuild" (loses parametric editing), or accept advisory IDs only (loses constraint persistence).

---

### Phase 8 — Graph Foundation (Weeks 40–48)

**Goal:** validate substrate reuse without god-abstraction.

#### 8.1 `kernel/graph-foundation`

Implement primitives only (per §1.14):
- NodeId / EdgeId types
- Stable hashing (BLAKE3-keyed structural)
- Diff primitives (3-way merge, structural diff)
- Snapshot serialization (immutable + structural sharing)
- Invalidation propagation API
- Visualization-adapter trait

**Do NOT implement:**
- Graph traversal algorithms (each domain has its own)
- Graph evaluation (each domain's evaluator)
- Universal graph runtime

#### 8.2 Migrate existing graphs onto substrate

**One by one, not simultaneously:**

1. **cad-core operator graph** (already exists from Phase 7) — refactor to use graph-foundation NodeId/EdgeId/hashing
2. **Material graph** — implement on graph-foundation from day 1
3. **Anim graph** — implement on graph-foundation from day 1

If at any point graph-foundation primitives don't fit a graph type, that's signal to either (a) fix the primitives, or (b) accept the graph as outside the substrate. Don't force-fit.

PLAN.md cross-ref: §1.14, ADR-101.

#### Exit criteria

- 3 graph types share NodeId / EdgeId / hash / diff / snapshot primitives
- Cross-graph diff works (e.g., diff between material graph + cad-core graph at two checkpoints)
- Editor `node_graph.rs` widget renders all 3 graph types via shared viz adapter
- graph-foundation ADR additions require review (per §1.14 discipline)

#### Abort condition

If two of three graph types can't share primitives without leaking domain semantics into graph-foundation: the substrate is too narrow or too broad. Reconsider scope before adding more graph types.

---

### Phase 9 — Production Pressure (Weeks 48+)

**Now the architecture starts becoming real.** Implementation pressure drives evolution from here.

This is where you evaluate:
- **§0.6 freeze policy validity** — did any subsystem need a 6th editor-state category? Was the freeze too tight?
- **Abstraction pain** — which architectures are paying off? Which are paper-only?
- **Invalidation economics** — projection-density metric (§1.10.4) reality-checked
- **Reflection scale** — compile time + binary size at 100+ reflected types
- **Async orchestration** — job-system pressure under real load
- **Compile times** — incremental p95 and clean-build budgets validated
- **Editor usability** — friction points from real authoring
- **GPU pressure** — VRAM residency under real scenes

Each entropy metric (§1.10.4) gets reality-checked. Some will be over-budget; that's signal to fix or relax. Some will be under-budget; no action needed.

**§0.6 freeze policy may relax here** if implementation evidence justifies new subsystems (the four-condition gate exists for this case). Equally likely: some architectures we thought we needed turn out to be unnecessary (and we delete them per §1.10.3 crate fusion criteria).

---

## 3. The most important rule

**DO NOT BUILD:**

- Multiplayer
- Advanced renderer features (Lumen, Nanite, MegaLights, VSM, TSR)
- Marketplace
- XR / OpenXR
- Distributed cooking
- Sophisticated CAD constraints
- Advanced graph tooling (sub-graphs, multi-monitor, cross-graph composition)
- Plugin ecosystem features
- Multi-monitor workspace systems

**Before:**

- WASM hot-reload (Phase 3) — proven
- cad-projection (Phase 7) — proven
- PIE snapshot/restore (Phase 5) — proven
- Reflection schema (Phase 1) — proven
- Command Bus (Phase 2) — proven

**These are the architectural load-bearing walls.** Adding sophistication on top of unproven walls is how engines fail mid-life.

---

## 4. Phase → WAVES.md mapping

[`WAVES.md`](./WAVES.md) describes 21 parallel waves. Most of those waves are blocked behind de-risking gates here. Mapping:

| Wave | IMPLEMENTATION.md phase | Blocked behind |
|---|---|---|
| W1 (components) | Phase 0 + Phase 2.1 | Phase 1.1 (kernel/types) |
| W2 (macros-reflect + kernel/types) | Phase 1.1 | — (this is the start) |
| W3 (editor-shell PIE) | Phase 5 | Phases 1, 2, 3 |
| W4 (wasmtime-engine) | Phase 3.1 | Phases 1, 2 |
| W5–W7 (ui-theme, ui-icons, ui-fonts) | Phase 5 | Phase 1 |
| W8–W10 (editor-ui menus/layout/dock) | Phase 5 | Phases 1, 2 |
| W11 (physics) | Phase 5+ | Phases 1, 2 |
| W12 (audio) | Phase 5+ | Phases 1, 2 |
| W13 (input) | Phase 5 | Phase 1 |
| W14–W16 (rge-data, pak-format, asset-store) | Phase 4 | Phases 1, 2 |
| W17–W18 (io-gltf, io-image) | Phase 4 | Phase 4 itself |
| W19 (expr-wasm) | Phase 3+ | Phases 1, 3 |
| W20 (script-bench) | Phase 3.4 | Phase 3 |
| W21 (golden test projects) | Phase 0.3 | — (start of project) |

**Reconciling the two views:**
- WAVES.md = what to do in parallel *once architecture is proven*.
- IMPLEMENTATION.md = what to prove first, sequentially, before parallelism is safe.

The effective execution plan is: do Phase 0 + 1 + 2 + 3 in tight sequence (weeks 1–12) with limited parallelism inside each phase. Once Phase 3 (WASM hot-reload) validates, the remaining waves can fan out per WAVES.md's parallelism model.

---

## 5. Risk-driven phase ordering rationale

| Phase | Validates | If it fails |
|---|---|---|
| 0 | Architecture enforcement is implementable | Architecture is too elaborate; cut subsystems |
| 1 | Reflection schema is fast enough | Replan reflection strategy |
| 2 | ECS + Command Bus mutation semantics | Replan undo model |
| **3** | **WASM hot-reload viable** | **Trigger ADR-077 escape clause; constitutional rewrite** |
| 4 | Cook determinism + RON viable | Find non-determinism source; possibly switch to alternate format |
| 5 | PIE snapshot/restore correctness | Redesign ECS storage for snapshot efficiency |
| 6 | Render-snapshot staging works | Reconsider sim/render thread separation |
| **7** | **cad-projection invalidation economics** | **Scale back B-Rep ambition or accept advisory IDs** |
| 8 | graph-foundation primitives reusable | Domain-specific graphs; no shared substrate |
| 9 | Production pressure surfaces real bottlenecks | Adapt; freeze policy relaxes if needed |

The two phases bolded (3 and 7) are the highest-risk validation points. If either fails fundamentally, the architecture changes shape.

---

## 6. What to track during implementation

Beyond the architecture entropy metrics from §1.10.4 (which are tracked at minor version bumps), implementation phases need real-time metrics:

| Metric | Source | Target |
|---|---|---|
| Compile time clean | `cargo build --release` | ≤120s (§13.3) |
| Compile time incremental p95 | `cargo build` after 1-line change | ≤10s |
| Hot-reload swap p95 | `script-bench` | <100ms |
| ECS mutation throughput | benchmarks | ≥1M ops/sec |
| Editor frame time idle | profiler | ≤8ms |
| Editor resident memory on simple-scene | runtime | ≤350MB |
| cad-core checkpoint storage per op | telemetry | <1MB typical |
| cad-projection invalidation density | counter | <30% per minor bump |
| Generic instantiations per crate | `cargo-llvm-lines` | <5000 warn / <15000 hard |
| Trait expansion depth | analyzer | <8 warn / <16 hard |
| Incremental invalidation radius | dep-DAG analysis | <30% of workspace |

Track from Phase 1 onward. Regression triggers freeze on the offending subsystem until resolved.

---

## 7. Pitfalls to avoid

Based on patterns from other engines + this plan's own review history:

1. **"We'll add architecture enforcement later."** Architecture without enforcement decays in weeks. Phase 0.2 is non-negotiable.

2. **"Hot-reload is a polish feature."** It's the constitutional bet. Validate it in Phase 3 or accept that the architecture rewrites.

3. **"Let's parallelize all 21 waves immediately."** Most waves are blocked behind de-risking gates. The 1-day bootstrap from WAVES.md §0 is necessary but doesn't unlock all parallelism.

4. **"Reflection is a syntactic concern; we'll optimize it later."** Reflection is the architectural root. If Phase 1.1 is slow, every later subsystem inherits the slowness.

5. **"cad-core is 'just' the CAD pillar."** It's load-bearing for the moat. Phase 7 failure changes RGE from "CAD-native engine" to "engine with CAD import" — strategically very different products.

6. **"We'll figure out determinism mode boundaries later."** Determinism is hard to retrofit. Validate gameplay-only Replay-Stable in Phase 5 (PIE replay).

7. **"PIE is a UX feature."** It's the test of editor/runtime unification. Phase 5 PIE failure means the unification claim is fictional.

8. **"Premature optimization is bad — ignore compile times for now."** Compile times that hit 5min in Phase 3 will hit 30min in Phase 9. Track from Phase 1.

9. **"More architecture is always better."** No. v0.8 was the freeze. Adding more before implementation evidence is exactly what §0.6 forbids.

10. **"We need to plan everything before coding."** This is the failure mode the v0.7 → v0.8 cycle nearly committed. Plan the load-bearing walls; let implementation pressure shape the rest.

---

## 8. Companion docs that should be written during implementation (not before)

These are listed in [PLAN.md §18](./PLAN.md) but should be written *during* the phase that produces the subsystem, not in advance:

- `RGE/CONVENTIONS.md` — Phase 0 (with the CI lint specs)
- `RGE/SCENE_MODEL.md` — Phase 2 (after `kernel/ecs` shape stabilizes)
- `RGE/PIE_MODEL.md` — Phase 5 (after PIE works)
- `RGE/UNDO_REDO_MODEL.md` — Phase 2 (after Command Bus works)
- `RGE/SCRIPT_BENCH_METHODOLOGY.md` — Phase 3.4 (with first benchmark numbers)
- `RGE/RENDERER_MODEL.md` — Phase 6 (after render-snapshot staging)
- `RGE/CAD_CORE_MODEL.md` — Phase 7 (with implementation experience)
- `RGE/CAD_TOPOLOGY_LINEAGE.md` — Phase 7
- `RGE/GRAPH_FOUNDATION.md` — Phase 8
- `RGE/EDITOR_STATE_MODEL.md` — Phase 5

Writing these in advance produces speculative documentation that drifts from implementation. Writing during implementation produces docs that match the code.

---

## 9. Final words

The architecture is frozen at v0.8. **The plan is done; the engine is not.**

This implementation order optimizes for *killing the highest-risk assumptions first*. Weeks 1–12 validate or invalidate the constitutional bet (WASM hot-reload). Weeks 28–40 validate or invalidate the moat (CAD-native). If both validate, the rest is execution. If either fails, replan happens at a known point with known scope.

The plan can survive partial failure of either critical-path validation. What it cannot survive is *delayed discovery of the failure*. Phase 3 and Phase 7 are positioned early specifically so that failure, if it comes, comes early — when the architecture can still adapt.

Ship.
