# PLUGIN_API

| Companion to | ADR-114 (PluginContext owned-resources-handoff design); PLAN.md §10.1 / §10.4 |
|---|---|
| Status | First §18 companion-doc landing 2026-05-08 (sibling to `PLUGIN_HOST_PATTERNS.md`); convention defined per `PLUGIN_HOST_PATTERNS.md` §header |
| Audience | Anyone reading or extending the `kernel/plugin-host` crate, or implementing the `Plugin` trait against it |
| Sibling doc | `PLUGIN_HOST_PATTERNS.md` — pattern-level guide; this doc is the API-surface reference |
| Reference impls | `kernel/plugin-host/src/plugin.rs` · `kernel/plugin-host/src/context.rs` · `kernel/plugin-host/src/host.rs` (host-side; included for orchestrator-author context) |

> This is the type-level reference. For pattern guidance ("when to use straight-line vs lazy-build", "how to map errors", "test recipe"), read `PLUGIN_HOST_PATTERNS.md` first; come back here for exact signatures + semantics.

## 1. The `Plugin` trait

Defined in `kernel/plugin-host/src/plugin.rs`. The contract every Tier-2 / Tier-3 plugin implements per PLAN.md §10.4 dogfood rule.

```rust
pub trait Plugin: Send + 'static {
    fn id(&self) -> PluginId;
    fn name(&self) -> &'static str { "" }
    fn init(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError>;
    fn tick(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> { Ok(()) }
    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> { Ok(()) }
}
```

### `Send + 'static` bound

The host stores plugins as `Box<dyn Plugin>` (ADR-114 §"Decision" sub-decision 2). `dyn Plugin` is object-safe; `Send + 'static` is required because a future hot-reload / cross-thread orchestrator must be able to move plugins between threads. Tier-2 plugins satisfy the bound trivially (their state is owned by the plugin struct, not borrowed). Tier-3 sandboxed WASM plugins satisfy it through the WASM ABI's owned-data semantics; Tier-3 lifetimes will be discussed in the future `runtime-wasmtime` × `plugin-host` integration ADR.

### Method-by-method semantics

#### `fn id(&self) -> PluginId`

Stable identifier for this plugin instance. Must match the `PluginId` under which the plugin was registered with `PluginHost::register`; the host validates this match and rejects mismatches with `PluginHostError::IdMismatch`.

Convention: `"<vendor>.<name>"` (Tier-3) or `"<crate-name>.<plugin-purpose>"` (Tier-2). Examples: `"rge-cad-projection.brep-handles-plugin"`, `"rge-gfx.headless-triangle-plugin"`, `"rge-physics.fixed-step-plugin"`. The convention mirrors `rge_kernel_ecs::participate::ParticipantId` (see `kernel/ecs/src/participate.rs` for the type itself).

No default impl — every plugin MUST declare its id.

#### `fn name(&self) -> &'static str`

Human-readable display name. Default `""`. Override when the canary wants a stable display string for debug overlays / inspectors / future hot-reload UIs.

The default is `""` and not (e.g.) the id's string because the trait can't return a borrow tied to a freshly-allocated `PluginId` without a lifetime. Implementations that want a non-empty name return a `&'static str` literal (the gfx canary uses `"rge-gfx headless triangle canary"`; physics uses `"rge-physics fixed-step canary"`).

#### `fn init(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError>`

One-shot init. Called exactly once after registration, **before** any `tick` or `shutdown`.

The plugin may interact with the supplied `PluginContext` (emit diagnostics, take/insert resources). For the lazy-build pattern (`PLUGIN_HOST_PATTERNS.md` §4), `init` is typically a no-op because the resources required to build the plugin's lazy state aren't staged until `tick` time.

**Errors:** Any `PluginError` returned here marks the plugin `Failed`; the host will not call `tick` or `shutdown` on a failed plugin. Use `PluginError::InitFailed { reason }` for canonical init failures (resource unavailable, dependency missing, validation failed). Use `PluginError::ContractViolation` if a required init-time resource was missing — though most current canaries have no init-time resource requirements.

No default impl — every plugin MUST implement init.

#### `fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError>`

Optional per-frame tick. Default no-op (`Ok(())`).

The plugin extracts owned resources from `ctx`, does work, and puts resources back per the patterns in `PLUGIN_HOST_PATTERNS.md` §3 / §4. Errors here mark the plugin `Failed` (per failure-class plugin-fatal isolation, PLAN.md §1.13) but the engine continues.

**Errors:** Map onto the `PluginError` taxonomy per `PLUGIN_HOST_PATTERNS.md` §5 (cheat-sheet). Most canaries return `Ok(())` on the success path and one of `ContractViolation` / `RuntimeFault` on the failure path.

#### `fn shutdown(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError>`

One-shot shutdown. Default no-op (`Ok(())`). Called exactly once when the host shuts down OR when the plugin is unregistered. After this returns the plugin is dropped.

**Errors:** Surfaced as diagnostics but do not block shutdown of other plugins. Host-initiated unregister classifies the error as `Warning` (the host explicitly asked for the unregister; teardown imperfection isn't an "engine is broken" signal); plugin-initiated shutdown errors classify as `Error`.

For canaries whose external resources are caller-staged (in the registry), `shutdown` is typically `Ok(())` because RAII handles internal resource cleanup at drop time.

## 2. The `PluginContext` API

Defined in `kernel/plugin-host/src/context.rs`. Carries:

- `&mut dyn DiagnosticSink` — direct accessor, preserved bit-identical from the v0 ctor (per the audit-1 CRITICAL #2 closure invariant).
- a type-erased resource registry: `BTreeMap<TypeId, Box<dyn Any + Send>>`. `BTreeMap` for deterministic iteration matching the workspace convention (per PLAN §1.6.8 determinism modes).

### Constructor

```rust
pub fn new(diagnostics: &'a mut dyn DiagnosticSink) -> Self
```

The v0 ctor signature, preserved bit-identical from pre-audit-1. Existing call sites that don't use the resource registry see no change in behaviour (the registry starts empty).

### Diagnostic API

```rust
pub fn emit_diagnostic(&mut self, diag: Diagnostic);
pub fn diagnostics(&mut self) -> &mut dyn DiagnosticSink;
```

`emit_diagnostic` is the high-level entry; `diagnostics()` returns the underlying sink for advanced use (e.g. when the plugin needs to pass it deeper into a helper that takes `&mut dyn DiagnosticSink` directly — passing the sink avoids re-borrowing the context). Both routes converge on the same `DiagnosticSink::emit` call.

### Resource registry API

```rust
pub fn insert<T: Any + Send>(&mut self, value: T) -> Option<T>;
pub fn get_mut<T: Any + Send>(&mut self) -> Option<&mut T>;
pub fn take<T: Any + Send>(&mut self) -> Option<T>;
pub fn contains<T: Any>(&self) -> bool;
pub fn resource_count(&self) -> usize;
pub fn with_resource<T: Any + Send>(self, value: T) -> Self;
```

Method-by-method semantics:

#### `insert<T>(value: T) -> Option<T>` — overwrite-semantics

Insert a resource. **Replaces** any previous value of the same type and returns the prior value if there was one. The first insert of a given `T` returns `None`; a subsequent insert of the same `T` returns `Some(previous)`.

Plugin authors typically rely on `insert` returning `None` after a `take` (the slot is empty) — the canary `debug_assert!`s this.

#### `get_mut<T>() -> Option<&mut T>` — peek-without-take

Borrow a resource mutably without removing it from the registry. Returns `None` if no resource of that type has been inserted.

Useful for plugins that want to mutate a resource without taking ownership (e.g. for in-place updates that don't need the owned-handoff guarantees). None of the three current canaries use `get_mut` — they all `take` + work + `insert`.

#### `take<T>() -> Option<T>` — owned removal; primary plugin entry

The plugin's primary entry into the registry. Removes the resource and returns owned `T`. The slot is left empty after this call. Returns `None` if no resource of that type has been inserted.

The owned-handoff design (ADR-114 §"Decision" sub-decision 1) makes this the canonical way to read the registry on the plugin side: take, do work with owned `T`, put back.

#### `contains<T>() -> bool` — predicate

Returns whether a resource of the given type is currently in the registry. Doesn't require `Send`; pure read-only inspection.

Useful for the host's pre-call validation and for tests that want to assert the put-back invariant (`PLUGIN_HOST_PATTERNS.md` §6).

#### `resource_count() -> usize`

Number of resources currently in the registry. Used by the host's leak-detection logic and by tests that want a coarse-grained registry-state assertion.

#### `with_resource<T>(self, value: T) -> Self` — builder pattern

Insert a resource and return `self` for chaining. Useful for orchestrator setup:

```rust
let ctx = PluginContext::new(&mut diags)
    .with_resource(world)
    .with_resource(cad_graph)
    .with_resource(tolerance);
```

The chain returns `self`, so it composes cleanly with multi-resource staging.

### Host-only inspection: `snapshot_resource_ids`

```rust
pub(crate) fn snapshot_resource_ids(&self) -> std::collections::BTreeSet<TypeId>;
```

`pub(crate)` — host-only. Used by `PluginHost` to verify resource preservation across plugin lifecycle calls (the leak-detection diff per ADR-114 §"Implementation guidance / Host-side wrap"). Plugin authors don't see this method; it's not part of the public plugin-side surface.

The host snapshots the registry's `BTreeSet<TypeId>` BEFORE invoking the plugin and again AFTER the call returns (or its panic is caught), then diffs to detect leaks. `BTreeSet` for deterministic iteration matching the workspace convention.

## 3. The `PluginError` taxonomy

Defined in `kernel/plugin-host/src/plugin.rs`. Five variants; the host's auto-emit logic maps each to the right `Diagnostic` severity per ADR-114 §"PluginError variant policy".

```rust
pub enum PluginError {
    InitFailed { reason: String },
    ShutdownFailed { reason: String },
    RuntimeFault { reason: String },
    ContractViolation { resource_type: &'static str },
    Panic { phase: PluginPhase, payload: String },
}
```

### Auto-emit policy

| Variant | Auto-emit Severity | Caller / plugin / host blame? |
|---|---|---|
| `InitFailed { reason }` | `Error` | Plugin |
| `ShutdownFailed { reason }` | `Warning` (host-initiated unregister) / `Error` (plugin-initiated shutdown) | Plugin |
| `RuntimeFault { reason }` | `Error` | Plugin |
| `ContractViolation { resource_type }` | `Warning` | Caller |
| `Panic { phase, payload }` | `Error` | Plugin (host-classified) |

Why these classifications:

- **`InitFailed` / `RuntimeFault` / `Panic` are plugin bugs.** Auto-emit at `Error` so the orchestrator's diagnostic stream surfaces them as failures.
- **`ShutdownFailed` is policy-dependent.** When the host unregisters a plugin (e.g. user-initiated), shutdown imperfection is non-fatal — the host explicitly asked for the unregister, so the engine isn't broken. When the plugin's own lifecycle drives shutdown, the error is a real failure. The host distinguishes via the call site and downgrades the unregister case to `Warning`.
- **`ContractViolation` is a caller bug.** The plugin code is fine; the orchestrator failed to stage prerequisites. `Warning`-level so the diagnostic stream doesn't elevate it to "engine is broken".

### Variant-by-variant constructor surface

Public constructors on `PluginError`:

```rust
impl PluginError {
    pub fn init(reason: impl Into<String>) -> Self;            // InitFailed
    pub fn shutdown(reason: impl Into<String>) -> Self;        // ShutdownFailed
    pub fn runtime_fault(reason: impl Into<String>) -> Self;   // RuntimeFault
    pub fn contract_violation(resource_type: &'static str) -> Self;
    // No public constructor for Panic — host-only (host-classified, host-recovered).
}
```

The lack of a public constructor for `Panic` is deliberate: plugins should NOT synthesize "I panicked" errors from inside their own code (a real panic produces this; a soft fault should use `RuntimeFault`). The host constructs `Panic` from `catch_unwind`'s panic payload after extracting via `Any::downcast_ref::<String>` / `&'static str`.

## 4. The `PluginPhase` enum

```rust
pub enum PluginPhase { Init, Tick, Shutdown }
```

Used inside `PluginError::Panic { phase, payload }` to identify which lifecycle method panicked. The host knows which call site caught the panic, and the `Display` impl renders as the lower-case method name (`"init"`, `"tick"`, `"shutdown"`) so auto-emit messages read naturally — for example, *"plugin panicked during tick: <payload>"*.

`Copy + PartialEq + Eq + Hash` so it's cheap to compare and use as a map key.

## 5. The `PluginHost` API summary

Defined in `kernel/plugin-host/src/host.rs`. The host owns plugins and manages their lifecycle. Plugins are registered with insertion-order tracking; `init_all` runs in registration order; `shutdown_all` drains in **LIFO** (reverse insertion order); `tick_all` walks `Initialized` plugins in registration order.

```rust
impl PluginHost {
    pub fn new() -> Self;
    pub fn register(&mut self, id: PluginId, plugin: Box<dyn Plugin>) -> Result<(), PluginHostError>;
    pub fn unregister(&mut self, id: &PluginId, ctx: &mut PluginContext<'_>) -> Result<(), PluginHostError>;
    pub fn init_all(&mut self, ctx: &mut PluginContext<'_>) -> InitReport;
    pub fn tick_all(&mut self, ctx: &mut PluginContext<'_>) -> TickReport;
    pub fn shutdown_all(&mut self, ctx: &mut PluginContext<'_>) -> ShutdownReport;
    pub fn get(&self, id: &PluginId) -> Option<&PluginRecord>;
    pub fn state(&self, id: &PluginId) -> Option<PluginState>;
    pub fn count(&self) -> usize;
    pub fn iter_ids(&self) -> impl Iterator<Item = &PluginId>;
}
```

### Lifecycle ordering

- **Registration:** plugins are stored in a `BTreeMap<PluginId, PluginRecord>` keyed by id, with a parallel `Vec<PluginId>` tracking insertion order. The map keeps lookups O(log n); the vec preserves the deterministic iteration order needed for init/tick/shutdown.
- **`init_all`:** walks `insertion_order` in forward order. Each `Pending` plugin gets one `init` call; failures (`Err` / panic / leak-on-`Ok`) mark the plugin `Failed` and are added to the report's `failed` list.
- **`tick_all`:** walks `insertion_order` in forward order. Each `Initialized` plugin gets one `tick` call; failures / leaks follow the same isolation pattern.
- **`shutdown_all`:** walks `insertion_order` in **reverse** (LIFO). Each `Initialized` plugin gets one `shutdown` call; `Failed` plugins are skipped (their `shutdown` is never called) so a broken plugin's shutdown is never called twice. Plugins that successfully shutdown transition to `Shutdown` state.

### Resource-leak detection wrap

Every direct call into a plugin's lifecycle method is wrapped in `std::panic::catch_unwind(AssertUnwindSafe(...))` plus a pre/post-snapshot diff of the resource registry. Implementation details are in ADR-114 §"Implementation guidance / Host-side wrap"; this doc summarises the contract:

1. Snapshot `BTreeSet<TypeId>` before the call.
2. Invoke the plugin via `AssertUnwindSafe` (safe — the surrounding scope is the panic-recovery boundary).
3. Snapshot again after the call (regardless of outcome).
4. Diff the two sets; emit a structured diagnostic for any `TypeId` that was present pre-call but absent post-call.
5. Map the outcome to `Ok` / `Err(PluginError)` / panic-payload; route to the right diagnostic severity.

`PluginRecord` is read-only from the public API (via `get(&id)`) — the host owns lifecycle transitions; external mutation is not part of the contract.

This doc does not specify the host's internal implementation beyond the contract above. For implementation rationale see ADR-114; for source see `kernel/plugin-host/src/host.rs`.

## 6. Layering invariants

These invariants are **load-bearing** for the `forbidden-dep` architecture lint and for the kernel-tier discipline that keeps `kernel/plugin-host` Tier-1.

### Plugin-side imports

- **Plugins MAY import any Tier-1 type.** `kernel/plugin-host`, `kernel/ecs`, `kernel/diagnostics`, `kernel/types`, `kernel/graph-foundation`, etc. are all valid imports for any Tier-2 plugin crate. The plugin uses the `Plugin` trait and `PluginContext` from `kernel/plugin-host`; uses `World` from `kernel/ecs` (if needed); routes diagnostics through `kernel/diagnostics`.
- **Plugins MAY define their own Tier-2 types and use them through `PluginContext` resource registry.** `cad-projection` defines `CadGraph` + `Tolerance`; `gfx` defines `GfxContext` + `HeadlessTarget`; `physics` defines `World` + `PhysicsInputLedger` (renamed from `AuditLedger` 2026-05-09 — see `crates/physics/src/physics_input_ledger.rs` module-doc for the divergence rationale from `kernel/audit-ledger`). These are Tier-2 types, owned by their crate, threaded through the type-erased registry.
- **Plugins MUST NOT import other Tier-2 plugins' types.** A `gfx` plugin cannot `use rge_physics::World`. The `forbidden-dep` lint enforces "no Tier-2 → Tier-2 dependency across non-shared boundaries". The shared substrate is the type-erased `PluginContext`; plugins coordinate via *types they each own*, not via shared imports.

### Host-side imports

- **`kernel/plugin-host` MUST NOT import any Tier-2 type.** It's Tier-1; the resource registry is type-erased (`BTreeMap<TypeId, Box<dyn Any + Send>>`) precisely so no upward imports are needed. `World`, `CadGraph`, `GfxContext`, etc. never appear in the host's source tree.
- **`kernel/plugin-host` MAY depend on other Tier-1 crates.** `kernel/diagnostics` (for `DiagnosticSink`), `serde` / `thiserror` for serialization + error derives. The crate's `Cargo.toml` declares this dep set; the architecture-lint enforces no upward-tier deps.

### Why this matters

If a plugin imports another plugin's types, the `forbidden-dep` lint fails the workspace build and the dispatch is blocked. This is mechanical enforcement of PLAN §10.4's dogfood rule: each plugin is independent; coordination happens through the type-erased registry, not through cross-imports. The architecture-lint is the gate; this doc is the rationale.

When you author a new plugin, the test for "am I in compliance?" is: **`cargo run -p rge-tool-architecture-lints -- all` exits 0 after your changes**. If it fails on a `forbidden-dep` rule, you've imported a Tier-2 type from another plugin's crate; refactor to use the type-erased registry instead.

## 7. References

- **ADR-114** — design rationale for the owned-handoff substrate; see §"Decision" + §"Implementation guidance" + §"Amendment 2026-05-08 — Three-substrate validation".
- **`PLUGIN_HOST_PATTERNS.md`** — sibling §18 doc; pattern-level guide for plugin authors. Use it for "when to use straight-line vs lazy-build", "how to map errors to `PluginError`", "test recipe template".
- **PLAN.md §10.1** — Tier-1 / Tier-2 / Tier-3 layering definition; the `forbidden-dep` lint enforces this.
- **PLAN.md §10.4** — dogfood rule; Tier-2 plugins use the same `Plugin` trait as Tier-3.
- **PLAN.md §1.13** — failure containment model; plugin-fatal isolation.
- **PLAN.md §1.6.8** — determinism modes; explains why the resource registry uses `BTreeMap` (deterministic iteration).
- **`kernel/ecs/src/participate.rs`** — `ParticipantId` type; the convention `PluginId` follows for cross-version identity stability.
- **`kernel/plugin-host/src/plugin.rs`** — `Plugin` trait, `PluginError` taxonomy, `PluginPhase` enum.
- **`kernel/plugin-host/src/context.rs`** — `PluginContext` with type-erased resource registry.
- **`kernel/plugin-host/src/host.rs`** — `PluginHost` lifecycle manager, `PluginRecord`, `PluginState`, `PluginHostError`, `InitReport` / `TickReport` / `ShutdownReport`.
- **`crates/cad-projection/src/plugin_adapter.rs`** · **`crates/gfx/src/plugin_adapter.rs`** · **`crates/physics/src/plugin_adapter.rs`** — three Tier-2 plugin canaries; canonical examples for the patterns in `PLUGIN_HOST_PATTERNS.md`.
