# ADR-114: PluginContext owned-resources-handoff design

| Status | Accepted 2026-05-08 (substrate landed 2026-05-07 audit-1 CRITICAL #2; hardening 2026-05-08 audit-2 Phase 0) |
|---|---|
| Date | 2026-05-08 |
| Deciders | (RGE architecture review) |
| PLAN references | §10.4 (dogfood rule — Tier-2 plugins use the same `Plugin` trait as Tier-3), §1.13 (failure classes — plugin-fatal isolation), §1.5.4 (cad-core), workspace `unsafe_code = "forbid"` |
| ADR references | ADR-097 (cad-projection split — first canary), ADR-112 (cad-core Boolean — adjacent host integration), ADR-113-deferred (truck — future plugin user) |
| Implementation phase | Tier-1 kernel substrate (`kernel/plugin-host`); precondition for Phase-7 CAD-projection canary |

## Context

PLAN §10.4 commits the workspace to a "dogfood rule": Tier-2 subsystems (gfx, physics, audio, editor-ui, cad-projection, …) implement the SAME `Plugin` trait as Tier-3 sandboxed WASM plugins. The trait lives in `kernel/plugin-host`. Tier-1 kernel crates cannot import Tier-2 types per the `forbidden-dep` architecture lint, so the trait's `init` / `tick` / `shutdown` methods can't take `&mut World`, `&CadGraph`, etc. directly — those types live in Tier-2. Plugins still need access to those resources to do useful work. The bridge is a `PluginContext` handle passed into every lifecycle method.

The naive "context carries `&mut` references" design — `PluginContext { world: &mut World, cad_graph: &CadGraph, … }` — pulls all Tier-2 types into Tier-1 (violating the layering lint) and explodes generically across plugins (every plugin needs a different context shape). The 2026-05-07 audit-1 CRITICAL #2 closure shipped the v1 substrate that resolves this: a type-erased resource registry on the context, with plugins extracting owned resources and putting them back. The 2026-05-08 audit-2 Phase 0 hardening then wrapped every plugin call in `catch_unwind` and added a 5-variant `PluginError` taxonomy plus pre/post resource-snapshot leak detection.

Together the two waves represent a non-trivial design space. There were borrowed-reference variants, generic-context variants, and capability-based variants on the table; the chosen "owned-resources-handoff" path is non-obvious and the `unsafe_code = "forbid"` constraint is load-bearing for *why* this shape and not the alternatives. This ADR documents the design space and rationale so future maintainers don't have to reverse-engineer it from the host's source.

## Decision

**`PluginContext` holds a `BTreeMap<TypeId, Box<dyn Any + Send>>` resource registry. Plugins call `ctx.take::<T>()` at the start of their lifecycle method, do work with the owned resource, and `ctx.insert::<T>(value)` to put it back. The orchestrator inserts before each call and `take`s after the call. The `Plugin` host wraps every call in `std::panic::catch_unwind(AssertUnwindSafe(...))` and snapshots the registry's `BTreeSet<TypeId>` before/after to detect resource leaks regardless of the call's outcome (`Ok` / `Err` / `Panic`).**

Three sub-decisions follow.

1. **Owned-handoff, not borrowed-references.** Storing `&'a mut T` in a runtime-typed map without `unsafe` is genuinely impossible in safe Rust. The workspace policy is `unsafe_code = "forbid"` at the workspace root. Owned-handoff via `Box<dyn Any + Send>` is the only safe alternative that doesn't fork into per-plugin generics. The "no `unsafe`" property is **load-bearing** for both the workspace pledge and the kernel/userspace boundary framing per the audit cross-review.
2. **Type-erased registry, not generic context.** A `PluginContext<World, CadGraph, …>` shape would proliferate combinatorially across plugins (gfx + physics + audio + editor-ui + cad-projection each need a different resource set). The type-erased `BTreeMap<TypeId, Box<dyn Any + Send>>` keeps the trait monomorphic and the `Box<dyn Plugin>` storage in `PluginHost` viable.
3. **`catch_unwind` + pre/post-snapshot, not trust-the-plugin.** Plugins are **untrusted execution domains** (the kernel/userspace boundary equivalence per the audit framing). A panic inside a plugin must NOT corrupt orchestrator state. Every host → plugin call site is wrapped in `std::panic::catch_unwind(AssertUnwindSafe(...))`. The host snapshots the registry's `TypeId` set BEFORE the call and AGAIN AFTER (regardless of outcome), then diffs to detect resource leaks. A plugin that took `World` and didn't put it back is detected and surfaced as a structured diagnostic.

## Consequences

### Positive

- **Workspace `unsafe_code = "forbid"` holds.** No part of the host or context machinery uses `unsafe`. `Box<dyn Any + Send>` is owned + Send + 'static; `BTreeMap` insertion/removal is safe; `Any::downcast` is a safe API. The pledge is intact end-to-end.
- **Tier-1 ↔ Tier-2 layering holds.** `kernel/plugin-host` doesn't import any Tier-2 types. Plugins pull whichever Tier-2 types they need at their own crate level and downcast them out of the registry; the host is type-blind.
- **Single trait, single host, all plugins.** `Box<dyn Plugin>` storage in `PluginHost` works because the trait stays monomorphic. The `dogfood rule` is mechanically expressible.
- **Plugin failure is isolated.** A panic mid-`tick` after taking `World` no longer permanently loses `World`: the host detects the leak (registry snapshot diff), emits a structured diagnostic, marks the plugin `Failed`, and the orchestrator continues with other plugins running. PLAN §1.13 plugin-fatal isolation is enforced mechanically rather than aspirationally.
- **Error taxonomy distinguishes blame.** `ContractViolation` (caller-side warning) vs. `RuntimeFault` (plugin-side error) vs. `Panic` (host-classified, host-recovered). Auto-emit downgrades `ContractViolation` to a `Diagnostic::Warning` (the plugin code is fine; the caller failed to stage prerequisites) and elevates `Panic` to `Diagnostic::Error`. Future debugging UIs can route by variant.

### Negative / risks

- **Two allocator interactions per resource per call.** `take` is `BTreeMap::remove` + a downcast unbox (one allocator interaction); `insert` is `Box::new` + `BTreeMap::insert` (one allocator interaction). At plugin-tick rate (~60Hz × N plugins × M resources) the cost is well under 1µs/tick on commodity hardware. Documented in `host.rs` module-doc; if a future high-frequency plugin surfaces a real bottleneck, a per-resource pool or arena allocator can be layered on top.
- **`AssertUnwindSafe` is a manual assertion of unwind safety.** The host's surrounding scope IS the panic-recovery boundary, so the assertion is sound, but it's a soundness obligation that future maintainers must respect. Documented inline in `host.rs`.
- **Resources held by a panicking plugin are unrecoverable.** If a plugin panics after taking `World`, the host detects the leak but the `World` value itself was on the plugin's stack frame and is gone. The host signals the orchestrator that the resource is missing; the orchestrator must either re-stage a fresh resource or refuse further `tick` calls. Documented in the `PluginError::Panic` variant docstring.
- **Resource-handoff is not zero-cost.** Compared to a hypothetical "pass `&mut World` through generic context" design, owned-handoff allocates and de-allocates the `Box` once per call. The trade-off is documented; the plugin-tick rate makes it negligible.

### Mitigations

- **Snapshot diff fires regardless of outcome.** `Ok` / `Err(_)` / `Panic` paths all converge on the same post-call snapshot. A plugin that returns `Ok` but forgot to `insert` back is detected exactly the same way as a plugin that panicked.
- **`PluginError::ContractViolation { resource_type: &'static str }` carries the missing-resource type name.** Caller diagnostics include "missing resource of type World" — the orchestrator can fix the staging at the exact point of failure.
- **`PluginPhase` enum on `Panic`.** `Init` / `Tick` / `Shutdown`. The host knows which lifecycle method panicked; auto-emit messages read naturally ("plugin panicked during tick: <payload>").
- **First real canary validates end-to-end.** `cad-projection::CadProjectionPlugin` (`crates/cad-projection/src/plugin_adapter.rs`, ~190L) extracts `&mut World + &CadGraph + Tolerance` via `take<T>`, drives `CadProjection::tick`, puts back. 16 integration tests cover the missing-resource error path, the resources-put-back invariant, and the runtime-fault propagation. Demonstrates the design generalises beyond synthetic fixtures.

## Alternatives explicitly NOT chosen and why

**Borrowed `&mut` references with type-erasure.** A naive `BTreeMap<TypeId, *mut dyn Any>` (or any equivalent with `&mut` references stored as raw pointers) requires `unsafe` for the deref, the Send/Sync invariants, and the lifetime narrowing. The workspace policy is `unsafe_code = "forbid"` at the root; circumventing it for the plugin substrate would set a precedent that erodes the pledge across the entire kernel. The cost (one Box per resource per call) is provably negligible at plugin-tick rate.

**Generic context types per plugin (`PluginContext<World, CadGraph, …>`).** Every plugin in the dogfood-rule set (gfx + physics + audio + editor-ui + cad-projection) has a different resource shape. A generic context forces a different `PluginContext<…>` instantiation per plugin, which in turn breaks the `Box<dyn Plugin>` storage in `PluginHost` (the `Plugin` trait would be parameterized by context type, no longer object-safe). The host architecture would either (a) de-virtualize and inline every plugin (defeats the dogfood rule's point — Tier-3 WASM plugins MUST go through dynamic dispatch) or (b) introduce a separate trait per plugin (combinatorial proliferation). Type-erased registry sidesteps both.

**Capability-based context (per ChatGPT cross-review on audit-1).** "The user grants capability X; plugin can request via `ctx.cap::<X>()`". This shape was reviewed and tabled: it defers to runtime checks (capability is granted/denied at call time) rather than to the Tier-1 substrate. Capabilities CAN be layered on top of v1 (e.g. a future `ctx.take_with_cap::<World, ReadOnly>()` that gates by token), but they aren't load-bearing — they're a refinement of "the plugin asked for something that should be checked". v1's owned-handoff is the orthogonal Tier-1 substrate. Layer when the use case materialises.

**Trust the plugin (no `catch_unwind`, no snapshot diff).** This was the audit-1 v0 shape, before audit-2 Phase 0 hardening. A plugin that panicked mid-`tick` after taking `World` permanently lost World from the orchestrator's view. PLAN §1.13 plugin-fatal isolation requires that one plugin's panic NOT corrupt other plugins' state — the audit-2 hardening is what closes that gap. "Trust the plugin" is incompatible with the dogfood rule (Tier-3 WASM plugins are explicitly untrusted) and with §1.13. Hardening is mandatory.

## Implementation guidance

The snippets below show the two error-mapping shapes a plugin author writes by
hand: `contract_violation` (caller-supplied resource missing) and
`runtime_fault` (plugin's own logic returned `Err`). Three more variants —
[`InitFailed`](#pluginerror-variant-policy) (plugin's `init` returned `Err`),
[`ShutdownFailed`](#pluginerror-variant-policy) (plugin's `shutdown` returned
`Err`), and [`Panic`](#pluginerror-variant-policy) (host-classified after
`catch_unwind`) — exist on the [`PluginError`] enum and are documented in full
under §"`PluginError` variant policy" below. The 5-variant taxonomy landed in
the audit-2 Phase 0 hardening (2026-05-08); the constructors used here
(`PluginError::contract_violation` / `PluginError::runtime_fault`) wrap the
typed enum variants and ARE current as of the 5-variant expansion.

### Resource lifecycle (canonical pattern)

```rust
// orchestrator (before plugin call)
ctx.insert(world);          // moves World into ctx
ctx.insert(cad_graph);      // moves CadGraph into ctx
// ...

// plugin (inside tick)
fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
    let mut world = ctx.take::<World>()
        .ok_or_else(|| PluginError::contract_violation("World"))?;
    let cad_graph = ctx.take::<CadGraph>()
        .ok_or_else(|| {
            // restore what we already took before erroring (idempotent failure)
            ctx.insert(world);
            PluginError::contract_violation("CadGraph")
        })?;

    // do work; on RuntimeFault, restore everything before propagating
    let result = self.projection.tick(&mut world, &cad_graph, ...);

    ctx.insert(world);
    ctx.insert(cad_graph);
    result.map_err(|e| PluginError::runtime_fault(e.to_string()))
}

// orchestrator (after plugin call)
let world = ctx.take::<World>().expect("plugin returned World");
// ...
```

### Host-side wrap (canonical pattern)

```rust
fn dispatch_tick(record: &mut PluginRecord, ctx: &mut PluginContext<'_>) -> ... {
    let pre_snapshot = ctx.snapshot_resource_ids();
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
        record.plugin.tick(ctx)
    }));
    let post_snapshot = ctx.snapshot_resource_ids();

    let leaked: BTreeSet<_> = pre_snapshot.difference(&post_snapshot).copied().collect();
    if !leaked.is_empty() {
        // emit Diagnostic::warning("plugin leaked resources: ...");
    }

    match outcome {
        Ok(Ok(())) => /* plugin Ok */ ...,
        Ok(Err(e)) => /* plugin Err */ ...,
        Err(payload) => {
            let payload_str = extract_panic_payload(payload);
            // mark plugin Failed; emit Diagnostic::error
            // map to PluginError::Panic { phase: PluginPhase::Tick, payload: payload_str }
        }
    }
}
```

### `PluginError` variant policy

- **`InitFailed { reason }`** — plugin's `init` returned Err. Plugin marked `Failed`; no further `tick` / `shutdown` calls. Auto-emit `Diagnostic::error`.
- **`ShutdownFailed { reason }`** — plugin's `shutdown` returned Err. Plugin still treated as shut down; auto-emit `Diagnostic::error`.
- **`RuntimeFault { reason }`** — plugin's `tick` returned a generic error from inside its own logic. Auto-emit `Diagnostic::error`. Plugin may continue or be marked Failed depending on host policy.
- **`ContractViolation { resource_type: &'static str }`** — caller didn't stage a required resource. NOT a plugin bug. Auto-emit `Diagnostic::warning`. Static `&'static str` so the variant doesn't allocate and can match at zero cost.
- **`Panic { phase: PluginPhase, payload: String }`** — host caught a panic via `catch_unwind`. NEVER constructed by plugins (no public constructor). Auto-emit `Diagnostic::error`. Plugin marked `Failed`. Resources held by panicking plugin are unrecoverable; leak detection fires separately.

### Test recipes (validated by `cad-projection::CadProjectionPlugin`)

1. **Happy path.** Stage `World` + `CadGraph` + `Tolerance`; call `tick`; resources back in registry; result `Ok`.
2. **Missing required resource.** Stage `World` only (skip `CadGraph`); call `tick`; result `Err(PluginError::ContractViolation { resource_type: "CadGraph" })`; `World` (the one we did stage) preserved in registry.
3. **Runtime fault.** Stage all resources; cause projection's `tick` to fail; result `Err(PluginError::RuntimeFault { … })`; resources back in registry.
4. **Panic recovery.** Plugin panics during `tick`; host's `catch_unwind` catches; result `Err(PluginError::Panic { phase: Tick, payload: … })`; resource-leak diagnostic emitted; plugin marked `Failed`.
5. **Resource-leak detection on Ok.** Plugin returns `Ok(())` but forgot to `insert` back; pre/post snapshot diff detects the missing TypeId; warning diagnostic emitted.
6. **Lifecycle order invariant.** `init` → `tick` → ... → `shutdown`; `Failed` plugins skip `tick` and `shutdown`.

## Followups / open questions

- **gfx::Plugin canary.** `cad-projection` is the first canary; `gfx` is the planned second. Proves the design generalises beyond a single subsystem. Tracked in HANDOFF.md as the next plugin-host dispatch.
- **Tier-3 WASM plugin ABI.** Today's `Send + 'static` contract is in-process only. Tier-3 plugins live in WASM and can't pass `Box<dyn Any + Send>` over the wire. The Tier-3 substrate needs a serializable wire format (e.g. `Box<dyn ResourceProtocol>` where `ResourceProtocol: Serialize + DeserializeOwned`). Defer until `runtime-wasmtime × plugin-host` integration begins.
- **Per-plugin budget enforcement.** Today the host classifies failures but doesn't bound CPU time or memory consumption. A misbehaving plugin can monopolise the tick window. Future hardening: per-plugin tick budget (`Duration`) enforced via a watchdog thread; on overrun, host treats as panic-equivalent. Defer until a real misbehaving-plugin scenario surfaces.
- **Resource-leak recovery policy.** Today, leaks are diagnosed but not auto-recovered. A future host could optionally re-stage canonical resources after a panic (e.g. fresh empty `World` + cached `CadGraph` snapshot) to keep the orchestrator running. Policy decision deferred until the multi-plugin scenarios stabilize.
- **Auto-emit allocation cost.** Each Err / Panic / leak path allocates a `String` for the diagnostic message. At plugin-tick rate the cost is negligible and only fires on the failure path. If high-throughput plugin-failure scenarios surface (e.g. a continuously-misconfigured ctx hammering auto-emit at 60Hz), a future dispatch can add rate-limiting or a structured `Diagnostic::Code` enum to dedupe. Not a v1 problem.
- **`TickReport` / `InitReport` / `ShutdownReport` aggregation surface.** The host returns per-plugin lifecycle reports today; the orchestrator's consumption pattern (single struct vs. iterator vs. per-plugin diagnostic stream) will surface design pressure as more plugins land. Defer until the second canary lands.

## References

- PLAN.md §10.4 (dogfood rule), §1.13 (failure classes — plugin-fatal isolation)
- IMPLEMENTATION.md (Tier-1 kernel substrates)
- ADR-097 (cad-projection split — first canary user)
- ADR-112 (cad-core Boolean — adjacent host integration; cad-projection is a downstream consumer)
- 2026-05-07 audit-1 CRITICAL #2 (PluginContext v1 substrate)
- 2026-05-08 audit-2 Phase 0 (catch_unwind + 5-variant taxonomy + resource-leak detection)
- `kernel/plugin-host/src/context.rs` (`PluginContext` — owned-handoff registry)
- `kernel/plugin-host/src/host.rs` (`PluginHost` — catch_unwind + snapshot diff dispatch)
- `kernel/plugin-host/src/plugin.rs` (`Plugin` trait, `PluginError` taxonomy, `PluginPhase` enum)
- `crates/cad-projection/src/plugin_adapter.rs` (first real canary: `CadProjectionPlugin`)
- ChatGPT cross-review note (audit-1) on capability-based context — layered as a refinement, not a v1 substrate

## Amendment 2026-05-08 — Three-substrate validation

The original ADR (Decision §"sub-decision 1" + §"Mitigations") rested on a forward claim: that the owned-handoff design *generalises* beyond the cad-projection prototype. Three Tier-2 plugin canaries have now landed against the same `kernel/plugin-host` substrate with **zero kernel-side substrate change** between them, converting that forward claim into a closed proof point. This amendment captures the validation, the patterns surfaced across the canaries, and a refined followup list.

### Three-substrate proof

| Canary | Date | Resource family | File |
|---|---|---|---|
| `CadProjectionPlugin` | 2026-05-07 | CAD-graph (`World` + `CadGraph` + `Tolerance`) | `crates/cad-projection/src/plugin_adapter.rs` |
| `GfxPlugin` | 2026-05-08 | GPU device handles (`GfxContext` + `HeadlessTarget`) | `crates/gfx/src/plugin_adapter.rs` |
| `PhysicsPlugin` | 2026-05-08 | physics-world arenas (`World` + `PhysicsInputLedger`) | `crates/physics/src/plugin_adapter.rs` |

The three canaries cover three structurally distinct resource families: plain-Rust CAD-graph types, wgpu device + texture handles, and rapier3d's physics-world arenas (`RigidBodySet` / `ColliderSet` / `IslandManager` / `DefaultBroadPhase` / `NarrowPhase` / `ImpulseJointSet` / `MultibodyJointSet` / `CCDSolver` / `IntegrationParameters` / `PhysicsPipeline`). Each canary expresses its full lifecycle through the unified `Plugin` trait per the §10.4 dogfood rule, and not a single line of code in `kernel/plugin-host` changed across the three landings. The ADR's Decision sub-decision 2 ("type-erased registry, not generic context") and Mitigations entry "First real canary validates end-to-end" are now load-bearing for *three* canaries, not one.

### Resource-Send taxonomy observed across the canaries

The kernel substrate stores resources as `Box<dyn Any + Send>`, so every resource a plugin extracts must be `Send + 'static`. The canaries surface three distinct Send shapes — none of which required `Mutex`, `Arc`, or `unsafe` to satisfy the bound:

- **Plain-Rust types are Send by construction.** cad-projection's `World` + `CadGraph` + `Tolerance` are pure data and Send falls out of derives + the absence of `!Send` fields. No annotation work needed.
- **GPU device handles are Send + Sync.** wgpu 29's `wgpu::Device` / `wgpu::Queue` / `wgpu::Texture` / `wgpu::TextureView` are documented `Send + Sync`, so `GfxContext` and `HeadlessTarget` satisfy the bound transparently. Pre-canary inspection of the wgpu API was the only diligence needed; no wrapper crate or local newtype.
- **rapier3d physics arenas are Send under `enhanced-determinism`.** rapier3d 0.32's full physics-world arena set is `Send` (the `enhanced-determinism` feature does not introduce any `!Send` types), so the physics canary's `World` + `PhysicsInputLedger` satisfy the bound out of the box. (The ledger was renamed from `AuditLedger` 2026-05-09 — physics owns its own per-tick input ledger separate from `kernel/audit-ledger`'s generic event ledger; see `crates/physics/src/physics_input_ledger.rs` module-level docs for the divergence rationale.)

This validates the Decision sub-decision 1 claim that owned-handoff via `Box<dyn Any + Send>` is the only safe alternative *in safe Rust* — no canary needed to reach for `unsafe`, no canary needed to wrap a `!Send` type in a sync primitive, no canary needed to compromise the workspace `unsafe_code = "forbid"` pledge. The `Send` bound is the design's escape hatch, and three substrates have now confirmed it is wide enough.

### Pattern dichotomy: straight-line vs lazy-build-on-first-tick

Across the three canaries, two `tick`-body shapes emerged:

- **Straight-line tick** (cad-projection, physics). All resources required to do work are `take`n at the start, the inner work runs end-to-end, and resources are `insert`ed back at the end. No state on the plugin struct besides incidental counters. This is the canonical shape when the plugin's inner work needs only resources the orchestrator can reasonably stage before every tick.
- **Lazy-build-on-first-tick** (gfx). The plugin holds an `Option<T>` field (e.g. `pipeline: Option<TrianglePipeline>`) initialised to `None` at construction. The first `tick` checks `is_none()`, builds the resource using a borrow on a resource the orchestrator only stages at `tick` time (in gfx's case, the `&GfxContext` needed to construct `TrianglePipeline`), and assigns into the field. Subsequent ticks reuse the built value.

The pattern choice is mechanical: **when a plugin's internal resource requires `&Resource` from the context to construct, use lazy-build (resources aren't available until tick, so init can't build it). Otherwise prefer straight-line tick — its single-purpose body is easier to read and keeps the plugin's struct closer to zero state.**

The `Option<T>` cost is one branch on every tick after the first, which is negligible at plugin-tick rate. `init` in the lazy-build case becomes a no-op — exactly mirroring the straight-line case's no-op `init`. Both shapes leave the plugin's lifecycle phases visually identical from the orchestrator's perspective.

### No-RuntimeFault subcase

The `PluginError` taxonomy includes `RuntimeFault { reason }` for plugin-side errors raised from inside the inner work. The physics canary's inner work — `physics_step(&mut world, &mut ledger)` — wraps `rapier3d::PhysicsPipeline::step`, which is **infallible** (returns `()`, not `Result<(), _>`). There is no failure path inside the body to map onto `RuntimeFault`, and the variant is therefore statically unreachable in the physics canary's `tick`.

This is acceptable v0 design. The variant remains *reserved* in the canary: future fallible-step extensions (joint-build paths, rapier3d API upgrades that surface step errors, optional per-step validity gates) will route through `RuntimeFault` as the canonical map. The decision is identical to the §"PluginError variant policy" entry for `RuntimeFault`: when the inner work is infallible at the call boundary, the variant is reserved but unused — *not* aliased to `ContractViolation` (which would conflate plugin bugs with caller-misconfigured ctx) and *not* repurposed for missing-resource paths.

This is the canonical pattern for any future plugin whose inner work is infallible at the call boundary. Documented as "no-RuntimeFault straight-line subcase" in the new companion docs.

### Followups RESOLVED

- **gfx::Plugin canary** — RESOLVED 2026-05-08. The followup originally captured a single-canary validation gap; it is closed by the gfx canary at `crates/gfx/src/plugin_adapter.rs` (lazy-build-on-first-tick variant) and reinforced by the physics canary at `crates/physics/src/plugin_adapter.rs`.

### New followups (next-substrate-family proof points)

- **~~Audio canary (cpal-style RAII handles)~~ RESOLVED 2026-05-08.** Closed by the audio canary at `crates/audio/src/plugin_adapter.rs`. Outcome (a) — clean confirmation. See `## Amendment 2026-05-08 — Four-substrate validation` below.
- **Editor-UI canary (egui mutable-singleton resources).** A `crates/editor-ui` plugin would test a fifth shape: `egui::Context` and the editor's own `EditorState` / `Workspace` are typically singletons mutated through `&mut` borrows, and the orchestrator must stage them while preserving the borrow-pattern that egui's frame-loop expects. Likely a straight-line tick variant but with deeper invariants on the orchestrator side. Defer until the editor-ui Phase 5 dispatches stabilise the singleton shape.

## Amendment 2026-05-08 — Four-substrate validation

The `## Amendment 2026-05-08 — Three-substrate validation` section above anticipated either (a) clean fourth-substrate confirmation OR (b) a real boundary requiring `Pattern C: Arc<Mutex<T>>` for non-Send resources. The audio canary at `crates/audio/src/plugin_adapter.rs` (2026-05-08; +19 tests, audio 28 → 48; workspace 1649 → 1668) closes the proof under outcome (a). This second amendment captures the closure plus the new pattern intersection that surfaced.

### Four-substrate proof

| Canary | Date | Resource family | File |
|---|---|---|---|
| `CadProjectionPlugin` | 2026-05-07 | CAD-graph | `crates/cad-projection/src/plugin_adapter.rs` |
| `GfxPlugin` | 2026-05-08 | GPU device handles | `crates/gfx/src/plugin_adapter.rs` |
| `PhysicsPlugin` | 2026-05-08 | physics-world arenas | `crates/physics/src/plugin_adapter.rs` |
| `AudioPlugin` | 2026-05-08 | audio engine + frame buffer | `crates/audio/src/plugin_adapter.rs` |

The audio canary surfaces the fourth structurally-distinct resource family: a Kira-managed audio backend (`AudioManager<MockBackend>` for tests; `AudioManager<DefaultBackend>` for runtime) plus a per-tick `AudioFrame` mix-buffer. Send-confirmed empirically by a permanent `assert_send_static<T>()` lib test (`audio_manager_and_audio_frame_are_send_static`) — both `AudioManager<MockBackend>` and `AudioManager<DefaultBackend>` satisfy `Send + 'static`. As with the prior three substrates, **zero kernel-side substrate change** was needed.

### Where the cpal::Stream non-Send concern actually lives

The three-substrate amendment's audio followup anticipated that `cpal::Stream` might surface a non-Send resource needing `Mutex` wrapping. The actual resolution is more interesting: **Kira itself owns the cpal-stream-on-a-thread wrapper layer**, exposing only Send-safe handles to its public API. The audio canary observes only the wrapper layer, and the underlying `cpal::Stream`'s non-Send constraint on Windows (WASAPI ownership) is invisible to the plugin-host substrate.

This refines the "wrapper newtype" pattern anticipated for cpal-style handles: **the wrapper lives in the audio engine library (Kira), NOT in plugin-host machinery**. Future plugins built on libraries that expose non-Send handles directly to consumers (without a Kira-style internal wrapper) would still need a `Pattern C: Arc<Mutex<T>>` or similar — the design boundary anticipated by the three-substrate amendment is real but happens to not bite the audio family.

### Pattern A + fallible inner work — first cross-canary intersection

Across the four canaries, the pattern matrix now reads:

*[As of 2026-05-10: editor-ui added as fifth canary, adopting the same straight-line pattern; matrix below remains the canonical 4-substrate proof set per the 2026-05-08 amendment.]*

| Canary | Tick shape | Inner-work failure mode |
|---|---|---|
| cad-projection | Pattern A (straight-line) | Fallible — `CadProjection::tick` returns `Result<TickReport, ProjectionError>` → `RuntimeFault` |
| gfx | Pattern B (lazy-build-on-first-tick) | Fallible — pipeline-build error → `RuntimeFault` (statically reachable; runtime-unreached at csgrs-equivalent stability) |
| physics | Pattern A (straight-line) | **Infallible** — `PhysicsPipeline::step` returns `()`; `RuntimeFault` reserved-but-unused (no-RuntimeFault subcase per §"No-RuntimeFault subcase" above) |
| audio | Pattern A (straight-line) | Fallible — `audio_schedule_step` returns `Result<(), ManagerError>`; `ManagerError::UnknownClip` → `RuntimeFault` exercised end-to-end |

**Audio is the first canary that exercises Pattern A + fallible inner work end-to-end at the integration-test level.** cad-projection has the same intersection but its `RuntimeFault` mapping is exercised through unit tests on `tick_inner` rather than end-to-end smoke. gfx's lazy-build error path is statically reachable but never triggers in practice. The audio canary's `audio_schedule_step` with an unregistered clip ID is the first test fixture where a `RuntimeFault` round-trips from canary tick → host `tick_all` → diagnostic stream → `Severity::Error` auto-emit.

### Updated followup list

- **~~Three-substrate proof~~** — RESOLVED by the prior amendment.
- **~~Four-substrate proof~~** — RESOLVED by this amendment.
- **Editor-UI canary (egui mutable-singleton resources)** — open; documented above.
- **CI-tier `AudioManager<DefaultBackend>` smoke**. The audio canary's integration tests use `MockBackend` so they're hardware-free. A separate `#[cfg(not(headless))]` smoke test exercising the real `cpal::Stream`-backed `DefaultBackend` would close the runtime-tier validation gap. Defer until CI gains audio-device capability.
- **Pattern C if and when it surfaces.** Outcome (b) from the three-substrate amendment ("real boundary requiring `Arc<Mutex<T>>`") has not yet surfaced across four substrates. Reserved as future-amendment material if a fifth or later canary triggers it.

### Companion-doc updates

`docs/§18/PLUGIN_HOST_PATTERNS.md` Pattern A description now lists audio as canonical example alongside cad-projection and physics. The "Pattern A + fallible inner work" intersection is implicit in the existing §3 + §5 (audio is the first canary where it's load-bearing); a future expansion could add an explicit subsection if a fifth substrate needs it.
