# ADR-116: Canary protocol — `CanaryPlugin` trait

| Status | Accepted 2026-05-10 (substrate landed alongside this ADR; pure-additive on the public API; backwards-compatible) |
|---|---|
| Date | 2026-05-10 |
| Deciders | (RGE architecture review; informed by 2026-05-10 ChatGPT cross-review #4 archived in `change.md`) |
| PLAN references | §10.4 (dogfood rule — Tier-2 plugins use the same `Plugin` trait as Tier-3), §1.13 (failure containment — plugin-fatal isolation) |
| ADR references | ADR-097 (cad-projection split — first canary user), ADR-114 (PluginContext owned-handoff — substrate this trait extends), ADR-115 (graph-metrics substrate design — peer ADR; same cross-review chain provenance; with 2026-05-10 amendment) |
| Implementation phase | Tier-1 kernel substrate (`kernel/plugin-host/src/canary.rs`); retroactive impls on the four §10.4 canaries (`crates/cad-projection`, `crates/gfx`, `crates/physics`, `crates/audio`) |

## Context

PLAN §10.4's dogfood rule has driven four Tier-2 plugin canaries against the same `kernel/plugin-host` substrate: `CadProjectionPlugin` (2026-05-07), `GfxPlugin` (2026-05-08), `PhysicsPlugin` (2026-05-08), `AudioPlugin` (2026-05-08). ADR-114's amendments document this convergence as the substrate proof point: four structurally-distinct resource families exercise the unified `Plugin` trait with zero kernel-side substrate change.

The 2026-05-10 ultra-deep audit's H5 finding closed a *semantic* parity gap across the four canaries: each now exposes a "successful tick count" telemetry accessor with the increment-only-on-success invariant (counter increments only when `tick()` returns `Ok(_)`; ContractViolation / RuntimeFault / Panic paths leave it unchanged). The H5 closure is documented in `change.md` 2026-05-10 05:35 + 06:00 entries.

*[As of 2026-05-10 09:30: editor-ui added as fifth canary — adopting `CanaryPlugin` from authoring per the protocol this ADR establishes — confirming the rule scales beyond runtime-centric subsystems. The four-canary framing in this Context section reflects the parity gap's authoring-time scope.]*

Post-H5 the four canaries are *semantically* parallel but *syntactically* divergent: each accessor carries a different inherent method name reflecting its subsystem's natural vocabulary:

| Canary | Inherent method | Source location |
|---|---|---|
| `CadProjectionPlugin` | `ticks_run() -> u64` | `crates/cad-projection/src/plugin_adapter.rs:126` |
| `GfxPlugin` | `frames_recorded() -> u64` | `crates/gfx/src/plugin_adapter.rs:147` |
| `PhysicsPlugin` | `steps_run() -> u64` | `crates/physics/src/plugin_adapter.rs:119` |
| `AudioPlugin` | `frames_advanced() -> u64` | `crates/audio/src/plugin_adapter.rs:213` |

The 2026-05-10 ChatGPT cross-review #4 (archived in `change.md` 06:00 entry) elevates this parity gap from "minor inconsistency" to a governance question: "you are transitioning from individual subsystem implementations to codified engine governance patterns. That is how serious engines evolve." Without a uniform protocol surface, future canaries (editor-ui per audit-7's M2 deferral; future material/anim/script canaries) drift further; downstream tooling (replay diagnostics, observability dashboards, hot-reload telemetry, editor sandbox) cannot consume the four telemetry surfaces uniformly; subsystem-contract lineage forks.

The cross-review carries a binding caution paired with the recommendation: "Do not over-generalize too early. The canary abstraction should stay minimal, telemetry-oriented, deterministic, governance-focused. Avoid building a giant plugin meta-framework. You do not yet have enough subsystem diversity for that." This ADR captures the minimal codification consistent with both the recommendation and the caution. ADR-115's "minimal-not-meta" framing is the structural precedent (phase-1 ships `core/` only).

## Decision

**`kernel/plugin-host` introduces a new `CanaryPlugin` trait that extends `Plugin` (super-trait), exposing exactly one method — `successful_ticks(&self) -> u64` — that returns the canonical telemetry counter shared across all §10.4 dogfood-rule canaries. The four existing canaries gain retroactive `impl CanaryPlugin` blocks that delegate to their existing inherent accessors; no inherent method is renamed and no public API is broken. The trait is object-safe (`&dyn CanaryPlugin` and `Box<dyn CanaryPlugin>` both work). No structured `CanaryTelemetry` type is introduced; no auto-registration / reflection machinery is introduced; the codification is bounded to the single `successful_ticks` method that already exists in semantic-but-not-syntactic form.**

Four sub-decisions follow.

### Sub-decision 1 — minimal trait shape: one method, no structured telemetry

The trait carries a single method:

```rust
pub trait CanaryPlugin: Plugin {
    fn successful_ticks(&self) -> u64;
}
```

No `CanaryTelemetry` struct. No lifecycle markers. No replay markers. No health states. The cross-review's enumeration of "potentially with: deterministic counters / lifecycle hooks / validation semantics / replay markers / health states" is **deferred** in full. Reasoning: a structured telemetry type with N fields is justified only when canary diversity surfaces N distinct shapes that need shared accessors. With four canaries today, there is exactly **one** uniform shape: the success-counter. Codifying the success-counter into a trait method is the minimal codification consistent with the data; codifying anything beyond it would invent shape that no canary today exhibits.

This is the **same discipline as ADR-115's phase-rollout**: phase-1 ships only Tier-A counters because that is what the workspace's graph consumers need today; structural / topology / analytical tiers ship in later phases as their consumer surfaces emerge. ADR-116 phase-1 (this ADR) ships only `successful_ticks` for the same reason. A future canary that exposes a richer telemetry shape (e.g. lifecycle markers as part of the editor-ui plugin per audit-7 M2; or replay-marker hooks once the replay-stable substrate per PLAN §1.6.8 has its first canary integration) will trigger an ADR-116 amendment that introduces the structured type. That amendment is out of scope here.

The cross-review's seven-property enumeration of canary-shared properties — (1) static plugin ID, (2) runtime ID accessor, (3) constructor pattern, (4) telemetry accessor, (5) deterministic counter semantics, (6) tests for initialization, (7) tests for failure invariants — is partly already codified by the existing `Plugin` trait (properties 2-3 via `Plugin::id()` + the `new()` convention; property 6 implicitly by `Plugin::init()`'s contract; property 7 by the H5 audit closure's per-canary error-path tests). Property 1 (static plugin ID const) is conventional rather than trait-enforced; codifying it as `const PLUGIN_ID: PluginId` on `CanaryPlugin` is **rejected** in this ADR because (a) it conflicts with `dyn`-safety (associated consts on object-safe traits work for direct types but cannot be queried through `&dyn CanaryPlugin`), and (b) the existing convention (each canary exposes a `pub const X_PLUGIN_ID: &str`) is sufficient and more idiomatic. Property 5 (deterministic counter semantics) is captured by the trait's increment-only-on-success contract per Sub-decision 4. Property 4 (telemetry accessor) is the trait method itself. So the codification covers properties 4-5 explicitly and inherits properties 2-3 + 6-7 from the existing substrate; only property 1 is left intentionally conventional.

### Sub-decision 2 — backwards-compatibility: existing inherent methods stay

Each canary's existing inherent method (`ticks_run` / `frames_recorded` / `steps_run` / `frames_advanced`) **stays** as part of its public API. The `impl CanaryPlugin for X` block delegates verbatim:

```rust
impl CanaryPlugin for CadProjectionPlugin {
    fn successful_ticks(&self) -> u64 {
        self.ticks_run()
    }
}
```

No renames. No deprecations. No `#[deprecated]` annotations. Reasoning: each inherent name is **contextually meaningful** — frames are not steps are not ticks. A user reading `gfx_plugin.frames_recorded()` learns "this counts frame submissions"; a user reading `physics_plugin.steps_run()` learns "this counts solver steps". Renaming all four to a single uniform name (`successful_ticks`) would erase that domain-specific signal at every callsite. Carrying both names — the domain-specific inherent for the canary's own consumers, the uniform trait method for cross-canary tooling — costs nothing and preserves both readings.

Pure-additive on the public API: 0 break, 0 deprecation, 4 retroactive trait impls.

### Sub-decision 3 — object-safety: `&dyn CanaryPlugin` and `Box<dyn CanaryPlugin>` work

The trait's only method takes `&self` and returns `u64`. No generics, no associated types, no `Self` in argument or return positions. The `Plugin` super-trait is itself object-safe (verified by existing `Box<dyn Plugin>` storage in `PluginHost`). Therefore `CanaryPlugin` is object-safe by composition.

Object-safety is **load-bearing for** future tooling that registers a heterogeneous canary set: an observability dashboard that polls `Vec<Box<dyn CanaryPlugin>>` to render "successful ticks per canary" needs dynamic dispatch; a replay-diagnostic that walks the host's plugin list and projects out the canary subset via downcast needs dynamic dispatch. A non-object-safe variant (e.g. one that took an associated `type Counter`) would force every consumer to either monomorphize per canary (defeats the cross-canary tooling point) or wrap each canary in a `Box<dyn CanaryPlugin<Counter = u64>>`-style projection (ergonomically painful). Keeping the trait object-safe lets `Box<dyn CanaryPlugin>` storage just work.

The trait inherits the `Send + 'static` bound from `Plugin`; `Box<dyn CanaryPlugin>` is `Send + 'static` by construction.

### Sub-decision 4 — increment-only-on-success invariant codified

The trait's docstring states the binding contract for `successful_ticks`:

> The returned counter MUST increment exactly when `Plugin::tick()` returns `Ok(_)`. `ContractViolation` / `RuntimeFault` / `Panic` paths MUST NOT increment the counter. The "increment-only-on-success" semantics is canon per the 2026-05-10 H5 audit closure and is a binding contract for any future canary impl.

The four existing canaries already satisfy this invariant (verified by the H5 closure's per-canary "counter unchanged on contract violation" tests). Future canary authors reading the trait's docstring see the invariant inline at the point they would implement the trait; no separate doctrine document is needed.

The invariant is asserted at the unit-test level per canary (each canary already has a "counter starts at zero" test; the H5 closure added "counter unchanged on error" tests in `cad-projection`). A later dispatch may add a workspace-level integration test that runs all canaries through a synthetic error-path harness to assert the invariant uniformly; that test is out of scope here (would require staging the four resource families, which is a non-trivial cross-crate orchestration).

The cross-review #4's "enterprise-grade correction" framing of H5 (the increment-only-on-success canonicalization) is what makes this contract load-bearing rather than cosmetic: a counter that quietly increments on every tick (success and failure alike) silently corrupts replay determinism (two replays diverge in their telemetry trajectory even when they semantically converge), distributed-orchestration synchronization (cross-process canary-counter comparisons would diverge when a node's local error path differs from a peer's error path on the same input), and editor-runtime separation (the editor's overlay would display a tick count that includes failed ticks, contradicting the user's intuition of "successful work done"). Codifying the contract at the trait level — not just per-canary — pre-emptively closes these failure modes for any future canary impl, including hypothetical Tier-3 sandboxed canaries that may surface in the future per ADR-114's followups list.

## Consequences

### Positive

- **Codified governance pattern.** Future canaries land against a trait, not against an ad-hoc inherent method. The four existing canaries' parallel-but-divergent state converges syntactically without losing the domain-specific inherent names.
- **Cross-canary tooling unblocked.** Observability dashboards / replay diagnostics / hot-reload telemetry / editor sandbox can consume `Vec<Box<dyn CanaryPlugin>>` uniformly instead of pattern-matching against four concrete types.
- **Backwards-compatible.** Pure-additive: no renames, no deprecations, no public API breakage. Every existing test, every existing call site, every existing doc reference to the four inherent methods continues to compile and behave identically.
- **Minimal-by-construction.** One method, no structured telemetry, no auto-registration, no reflection. The cross-review's "do not over-generalize too early" caution is structurally honored, not aspirationally honored.
- **Increment-only-on-success contract documented at the trait level.** Future canary authors see the invariant at the point they implement the trait; the H5 audit-finding's semantic correction becomes the trait's binding contract.

### Negative / risks

- **Two parallel surfaces per canary.** Each canary now exposes both an inherent method (`gfx::frames_recorded()`) and a trait method (`<gfx as CanaryPlugin>::successful_ticks()`). Consumers must choose which surface to use. Mitigation: the inherent method is the natural choice for users coding against a known concrete canary; the trait method is the natural choice for tooling working against `dyn CanaryPlugin`. The two surfaces don't compete; they serve different consumers.
- **Trait scope drift risk.** A future contributor may be tempted to add a second method to `CanaryPlugin` (e.g. `total_ticks()` / `failed_ticks()` / `last_tick_duration()`). Each addition compounds maintenance cost across all impls. Mitigation: the ADR's Sub-decision 1 binds the trait to a single method until canary diversity justifies expansion; expansion requires an ADR-116 amendment, not just a PR.
- **Object-safety constraint forecloses future shapes.** Adding methods that return `Self` or use generic associated types becomes a breaking change. Mitigation: the constraint is the design (Sub-decision 3); object-safety is what makes the trait useful for cross-canary tooling. If a future need genuinely requires non-object-safety, a sibling `CanaryPluginExt` trait can be added without breaking this one.

### Mitigations

- **Trait expansion gated by ADR amendment.** Sub-decision 1 binds the trait to one method; expansion requires an ADR-116 amendment with explicit canary-diversity justification (mirrors ADR-114's amendment pattern: "three-substrate validation" + "four-substrate validation").
- **Inherent methods carry a docstring back-reference.** Each canary's inherent method docstring already mentions the cross-canary parity (closes audit-6 round-6 H5). A future inherent-method rename would have to update the docstring, which forces the conversation.
- **Cross-link to ADR-114.** ADR-114 documented the substrate; ADR-116 documents the protocol on top of it. The bidirectional cross-reference makes the substrate ↔ protocol relationship discoverable from either direction.
- **Per-canary acceptance test pinned to `&dyn CanaryPlugin`.** The 4 retroactive impls each carry a `<canary>_plugin_impls_canary_protocol` test that uses the trait through dynamic dispatch (not just direct call). If a future refactor accidentally breaks dyn-safety (e.g. by adding a generic method to `CanaryPlugin`), every per-canary test fails to compile — load-bearing for keeping the trait usable as a trait object. Mirrors the "tautological-test regression" defense pattern that audit-2 established for plugin error-mapping (see `crates/gfx/src/plugin_adapter.rs::map_pipeline_err`).
- **Increment-only-on-success contract pre-existed in source.** Before this ADR, the contract was implicit in the four canaries' code (each `tick` body increments only on the `Ok` arm of the match against the inner work's outcome). Codifying the contract at the trait level documents it without requiring code changes — the existing semantics ARE the trait's contract. If a future canary impl violates the contract (counter increments on error path), it ships a bug that the trait's docstring + the per-canary `counter_unchanged_on_error` test (existing per H5) catches.

## Alternatives explicitly NOT chosen and why

**Don't formalize at all — leave the four inherent methods divergent.** REJECTED. The cross-review's "future canaries subtly diverge / editor-runtime drift begins / tooling assumptions fracture / telemetry schemas fork" is a real risk: with four canaries already and editor-ui/material/anim/script canaries on the horizon, the duplication compounds. Codification is justified by the existing four canaries' parallel-but-divergent state; deferring formalization until 6+ canaries doesn't eliminate the cost, only postpones it while increasing the size of the eventual refactor.

**Structured `CanaryTelemetry` type with multiple fields (tick count + lifecycle markers + replay markers + health states).** REJECTED per the cross-review's "do not over-generalize too early" caution. With four canaries and one shared shape (success counter), inventing a struct with N fields invents shape no canary exhibits. The ADR-115 phase-rollout precedent applies: codify what the workspace has today; expand when canary diversity surfaces additional shapes that justify additional fields. Deferred to a later ADR-116 amendment when concrete canary need surfaces.

**Rename the inherent methods to a uniform name (`successful_ticks`).** REJECTED. Each inherent name carries domain-specific signal at the call site (`frames_recorded` reads naturally for the gfx canary; `steps_run` for physics; `ticks_run` for cad-projection; `frames_advanced` for audio). A blanket rename to `successful_ticks` flattens four meaningful names into one generic name and breaks every existing call site. Carrying both — the domain-specific inherent for in-canary use, the uniform trait method for cross-canary tooling — preserves both readings without breakage.

**Promote `successful_ticks` into the existing `Plugin` trait.** REJECTED. Not all `Plugin` impls are canaries: the `Plugin` trait is the substrate-level contract for both Tier-2 in-process plugins (canaries) and future Tier-3 sandboxed WASM plugins. Tier-3 plugins per future `runtime-wasmtime × plugin-host` integration won't necessarily expose a "successful ticks" telemetry counter — their telemetry shape is a wire-protocol concern (per ADR-114's followups list), structurally different from the in-process canary shape. Adding a method to `Plugin` would force every Tier-3 plugin to implement a counter that may not match its execution model. Splitting `CanaryPlugin` off as a refinement of `Plugin` keeps the substrate trait minimal and lets the canary-specific semantics live where they belong.

**Auto-registration via `inventory` crate or a build-time reflection pass.** REJECTED for v0. The inventory crate would let canaries register themselves at link time, so a hypothetical observability tool could discover them without a manual list. But auto-registration adds a build-time dependency, complicates the workspace's link-order discipline, and is overkill for four canaries (a manual list is trivially maintainable). The cross-review's "reflection / tooling architecture" target is flagged as a future maturity target needing its own design session; auto-registration of canaries would be a downstream deliverable of that work, not a v0 of `CanaryPlugin`. Out of scope.

**Architecture-lint enforcing every canary impl `CanaryPlugin`.** REJECTED for v0. A lint that walks the workspace and verifies every plugin canary (identified by the `plugin_adapter.rs` filename convention or by a marker attribute) carries `impl CanaryPlugin for X` would close a real drift risk: a future contributor who lands a fifth canary without the trait impl would silently bypass the protocol. But the lint's "canary identification" rule is genuinely subtle (the four current canaries are §10.4 dogfood-rule canaries, but `plugin_adapter.rs` is conventional rather than mandatory; future plugins might land in differently-named modules). Codifying the heuristic is harder than it sounds and creates an opportunity for false-positive lint failures. Deferred until a fifth canary actually lands without the trait impl (signal that the lint is needed) or until the cross-review's "reflection / tooling architecture" matures (signal that the heuristic-detection problem has a clean answer). Doc-comment-canonical per ADR-104 in the meantime: the convention lives in this ADR + each canary's docstring.

**Generic `CanaryPlugin<Counter>` parameterized by counter type.** REJECTED. A future canary might want a higher-precision counter (`u128` for very-high-frequency canaries) or a typed counter (`NonZeroU64` after first tick). Parameterizing the trait by counter type would accommodate the variation, but at the cost of object-safety (Sub-decision 3): `Box<dyn CanaryPlugin<Counter = u64>>` is a different type from `Box<dyn CanaryPlugin<Counter = u128>>`, and cross-canary tooling that wants to walk all canaries cannot use a single `Vec` of boxed trait objects. The four current canaries use `u64` uniformly; codifying that as the trait's return type is consistent with their actual data. If a future canary's counter genuinely cannot fit in `u64` (~5,800 years at 100MHz tick rate), an ADR-116 amendment can revisit; until then, the simpler design wins.

## Implementation guidance

The trait + retroactive impls + tests land alongside this ADR.

### Trait definition (`kernel/plugin-host/src/canary.rs`)

```rust
use crate::plugin::Plugin;

/// The §10.4 dogfood-rule canary protocol.
///
/// Every Tier-2 plugin canary impl'ing the dogfood rule SHOULD impl
/// this trait to expose a uniform telemetry-accessor surface that
/// future tooling (replay diagnostics / observability dashboards /
/// editor sandbox / hot-reload telemetry) can consume through a
/// single `&dyn CanaryPlugin` reference.
///
/// # Increment-only-on-success invariant
///
/// `successful_ticks` MUST return a counter that increments exactly
/// when `Plugin::tick()` returns `Ok(_)`. `ContractViolation` /
/// `RuntimeFault` / `Panic` paths MUST NOT increment. Codified per
/// the 2026-05-10 H5 audit closure; binding for any future canary.
pub trait CanaryPlugin: Plugin {
    /// Number of successful `Plugin::tick()` calls.
    fn successful_ticks(&self) -> u64;
}
```

### Retroactive impl pattern (per canary)

```rust
// crates/cad-projection/src/plugin_adapter.rs
use rge_kernel_plugin_host::CanaryPlugin;

impl CanaryPlugin for CadProjectionPlugin {
    fn successful_ticks(&self) -> u64 {
        self.ticks_run()
    }
}
```

Mirrored verbatim across the four canaries — each delegates to its existing inherent accessor:

*[As of 2026-05-10: editor-ui added as fifth canary, adopting CanaryPlugin from authoring rather than retroactively. Its delegation: `successful_ticks() → self.observations_completed()`.]*

| Canary | Trait method | Delegates to inherent |
|---|---|---|
| `CadProjectionPlugin` | `successful_ticks()` | `self.ticks_run()` |
| `GfxPlugin` | `successful_ticks()` | `self.frames_recorded()` |
| `PhysicsPlugin` | `successful_ticks()` | `self.steps_run()` |
| `AudioPlugin` | `successful_ticks()` | `self.frames_advanced()` |

The delegation is a single-expression body; each impl block is ~5 lines including the `use` statement. Total retroactive impl footprint across the four canaries: ~20 lines of code excluding the new acceptance tests.

*[Editor-ui's `CanaryPlugin` impl was authored from the start (~5L), so editor-ui's adoption was not retroactive. Total workspace footprint for the original 4-canary retroactive impl remains ~20L.]*

### Dyn-safety / object-safety test pattern

The `kernel/plugin-host/src/canary.rs` foot-of-file tests use a minimal in-module `MockCanary` to prove the trait is object-safe without pulling in any of the four real canary types (which would create a Tier-2 → Tier-1 cycle and trip the `kernel-isolation` architecture lint):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::PluginContext;
    use crate::plugin::{PluginError, PluginId};

    struct MockCanary {
        ticks: u64,
    }

    impl Plugin for MockCanary {
        fn id(&self) -> PluginId { PluginId::new("rge.canary.mock") }
        fn init(&mut self, _: &mut PluginContext<'_>) -> Result<(), PluginError> {
            Ok(())
        }
    }

    impl CanaryPlugin for MockCanary {
        fn successful_ticks(&self) -> u64 { self.ticks }
    }

    #[test]
    fn canary_plugin_is_dyn_safe() {
        // If this compiles, the trait is object-safe.
        let mock = MockCanary { ticks: 7 };
        let dyn_ref: &dyn CanaryPlugin = &mock;
        assert_eq!(dyn_ref.successful_ticks(), 7);

        let boxed: Box<dyn CanaryPlugin> = Box::new(MockCanary { ticks: 42 });
        assert_eq!(boxed.successful_ticks(), 42);
    }

    #[test]
    fn canary_plugin_extends_plugin() {
        let mock = MockCanary { ticks: 0 };
        let canary_ref: &dyn CanaryPlugin = &mock;
        // Super-trait coercion: &dyn CanaryPlugin → &dyn Plugin.
        let plugin_ref: &dyn Plugin = canary_ref;
        assert_eq!(plugin_ref.id().as_str(), "rge.canary.mock");
    }
}
```

### Test recipes

The trait + retroactive impls land with **6 new tests** (1737 → 1743 expected). The recipes split into two foot-of-file groups:

**`kernel/plugin-host/src/canary.rs` (2 new tests).**

1. **Object-safety / dyn-safety.** A `&dyn CanaryPlugin` reference compiles; a `Box<dyn CanaryPlugin>` storage compiles; the boxed value's `successful_ticks` is callable through dynamic dispatch. The test uses a minimal in-module `MockCanary` impl (zero-state struct that returns 0 from `successful_ticks` and `Ok(())` from `tick`); the mock proves the trait is implementable without pulling in any of the four real canaries (which would create a Tier-2 → Tier-1 dependency violating the kernel-isolation lint).
2. **Super-trait composition (`&dyn CanaryPlugin` → `&dyn Plugin`).** The trait's `Plugin` super-trait is exercised by assigning a `&dyn CanaryPlugin` to a `&dyn Plugin` binding. If the assignment compiles, super-trait coercion works; the test then calls `Plugin::id()` through the `&dyn Plugin` view and asserts the round-trip.

**Per-canary tests (4 new tests, one in each canary's `plugin_adapter.rs` foot).**

3. **`cad_projection_plugin_impls_canary_protocol`** — instantiate `CadProjectionPlugin::new()`, take `&dyn CanaryPlugin`, assert `successful_ticks() == 0`. Same shape repeated as:
4. **`gfx_plugin_impls_canary_protocol`** for `GfxPlugin`.
5. **`physics_plugin_impls_canary_protocol`** for `PhysicsPlugin`.
6. **`audio_plugin_impls_canary_protocol`** for `AudioPlugin`.

Each per-canary test is a **3-line acceptance gate**: it confirms (a) the canary impls `CanaryPlugin`, (b) the trait method is callable through `&dyn CanaryPlugin`, and (c) the initial value matches the inherent accessor's initial value. If a future refactor accidentally drops the `impl CanaryPlugin` block from any canary, the test fails to compile — load-bearing for keeping the four impls in sync.

The tests are foot-of-file unit tests (the existing canary-test convention; mirrors `cad_projection_plugin_id_matches_convention` and similar shape).

## Followups / open questions

- **Structured `CanaryTelemetry` type (tick count + lifecycle markers + replay markers + health states).** Deferred per Sub-decision 1's minimal-trait shape until canary diversity surfaces 2+ additional shared shapes. Triggered by a future canary (e.g. editor-ui's M2 per audit-7) that exposes telemetry beyond the success counter.
- **Auto-registration via `inventory` crate or reflection.** Deferred until kernel/types reflection stabilizes per the cross-review's "reflection/tooling architecture maturity" target. Auto-registration would let cross-canary tooling discover canaries without a manual list; today's four-canary workspace doesn't need it.
- **Editor-UI canary (audit-7 M2 deferral).** When `editor-ui::Plugin` lands per audit-7's M2 / phase 5 stabilisation, it adopts `CanaryPlugin` via this ADR. The editor-ui canary will surface egui-singleton resource shapes that may motivate the structured `CanaryTelemetry` amendment above.
- **Tier-3 sandboxed WASM plugins.** `CanaryPlugin` is Tier-2 in-process only. Tier-3 plugins per future `runtime-wasmtime × plugin-host` integration have different telemetry shapes (wire-protocol-mediated; cross-process); the substrate's wire format is ADR-114's open followup. A separate ADR will codify Tier-3 telemetry once the WASM ABI lands.
- **Workspace-wide error-path invariant test.** A synthetic harness that exercises every `CanaryPlugin` impl through the four error paths (ContractViolation / RuntimeFault / Panic / Ok) and asserts the increment-only-on-success invariant uniformly. Today each canary asserts the invariant in its own test; a uniform test would strengthen the contract but requires staging four resource families across crate boundaries. Deferred until cross-crate-test infrastructure stabilizes.

## References

- **PLAN.md §10.4** — dogfood rule (Tier-2 plugins use the same `Plugin` trait as Tier-3); the rule that drove the four canaries this ADR formalizes. *[Editor-ui as fifth canary subsequently confirmed the rule scales beyond runtime-centric subsystems.]*
- **PLAN.md §1.13** — failure containment; the increment-only-on-success invariant aligns with the plugin-fatal isolation classification (errors must not silently corrupt downstream telemetry).
- **ADR-097** — cad-projection split; the first canary user, baseline for the parallel-impl pattern.
- **ADR-114** — PluginContext owned-handoff; the substrate this trait extends. The four-substrate validation amendments document the canaries that this trait codifies; bidirectional cross-link.
- **ADR-115** — graph-metrics substrate design; peer ADR with the same cross-review-chain provenance (cross-review #2 ↔ ADR-115; cross-review #4 ↔ this ADR). The minimal-not-meta framing precedent.
- **`kernel/plugin-host/src/canary.rs`** — the `CanaryPlugin` trait definition + foot-of-file dyn-safety tests.
- **`kernel/plugin-host/src/plugin.rs`** — the `Plugin` super-trait this trait extends.
- **`crates/cad-projection/src/plugin_adapter.rs`** — `CadProjectionPlugin` + `impl CanaryPlugin` (delegates to `ticks_run`).
- **`crates/gfx/src/plugin_adapter.rs`** — `GfxPlugin` + `impl CanaryPlugin` (delegates to `frames_recorded`).
- **`crates/physics/src/plugin_adapter.rs`** — `PhysicsPlugin` + `impl CanaryPlugin` (delegates to `steps_run`).
- **`crates/audio/src/plugin_adapter.rs`** — `AudioPlugin` + `impl CanaryPlugin` (delegates to `frames_advanced`).
- **`change.md` 2026-05-10 06:00 entry** — ChatGPT cross-review #4 archived in full; the source-of-truth for the canary-protocol-formalization recommendation and the minimal-not-meta caution.
- **`change.md` 2026-05-10 05:35 entry** — H5 closure that established the increment-only-on-success semantic invariant codified here as the trait's binding contract.
