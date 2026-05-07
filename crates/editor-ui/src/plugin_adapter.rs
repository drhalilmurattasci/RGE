//! `editor_ui::EditorUiPlugin` — fifth real Tier-2 plugin canary per the
//! §10.4 dogfood rule. First canary OUTSIDE purely runtime-centric subsystems
//! — validates that ADR-116's `CanaryPlugin` protocol remains coherent under
//! tooling-observational pressure.
//!
//! # Why this exists
//!
//! Closes M2 editor-ui::Plugin canary deferral (audit-7 Phase 5 stabilisation
//! gate met). Adopts ADR-116's `CanaryPlugin` trait as the FIRST new canary
//! under the formalised contract (the four prior canaries adopted retroactively
//! in commit 1b14287). Per the 2026-05-10 ChatGPT cross-review #8 (archived in
//! `change.md` 09:00 entry): "the editor canary is the first canary likely to
//! naturally pressure observational semantics / tooling contracts / snapshot
//! interpretation / runtime-tooling separation."
//!
//! The take/insert pattern this module repeats verbatim across all five canaries
//! (cad-projection / gfx / physics / audio / editor-ui) is intentional per
//! PLAN §10.4 dogfood rule — see [`rge_cad_projection::plugin_adapter`]'s
//! `# Why this looks duplicated across the four canaries` section for the
//! canonical rationale (which now applies to five).
//!
//! # Tooling-observational design principle (cross-review #8 binding)
//!
//! This canary behaves as a **tooling-observational participant**, NOT an
//! **editor runtime authority**. Specifically:
//!
//! * The plugin OBSERVES [`Selection`] from the [`PluginContext`] but does NOT
//!   mutate it.
//! * The plugin does NOT drive runtime behaviour based on observations —
//!   counters are advisory telemetry, not control flow.
//! * The plugin's tick is pure-read-and-count: take the resource, observe
//!   its state (count selected entities), put the resource back, increment
//!   the counter on success.
//!
//! This separation is load-bearing per PLAN §1.15 (editor-state =
//! coordination, not authoritative content) + ADR-115 phase-2.5 amendment's
//! "Canonical Runtime Truth vs Analytical Interpretation Layers" doctrine.
//! Editor canaries belong on the observational side.
//!
//! # Resource contract
//!
//! On `tick`, the plugin context MUST contain (caller-supplied):
//!
//! * [`Selection`] — owned `&mut` after `take`; observed (size queried) but
//!   NEVER mutated. Put back unchanged.
//!
//! Missing [`Selection`] surfaces as
//! [`PluginError::ContractViolation`] (caller-supplied resource missing —
//! NOT a plugin-side bug; auto-emit downgrades to a warning per audit-2
//! A5.1). Tick is infallible at the plugin-adapter level — observation is
//! pure read; there is no [`PluginError::RuntimeFault`] surface here. The
//! variant remains reserved for future fallible observation paths (e.g. a
//! validation pass that returns Err on detected invariant violations). This
//! is the second canary to inhabit the canonical "no-RuntimeFault straight-
//! line subcase" formalised in ADR-114 §"Amendment 2026-05-08 — Three-
//! substrate validation" (physics was the first; editor-ui is the second).
//!
//! # Failure class: recoverable
//!
//! Inherited from `editor-ui` lib root. Editor-ui canary observation
//! failures are transient (caller misconfigured ctx; next tick may succeed)
//! and do NOT corrupt PIE state. No [`SnapshotParticipate`] impl needed
//! (editor-ui correctly absent from `STATEFUL_TIER2_CRATES` per audit-3 H3
//! audit-and-discriminate; UI state is session-scoped per PLAN §1.15).
//!
//! [`SnapshotParticipate`]: https://docs.rs/rge-kernel-ecs/latest/rge_kernel_ecs/participate/trait.SnapshotParticipate.html

use rge_editor_state::Selection;
use rge_kernel_plugin_host::{CanaryPlugin, Plugin, PluginContext, PluginError, PluginId};

/// Stable [`PluginId`] reported by every [`EditorUiPlugin`] instance.
pub const EDITOR_UI_PLUGIN_ID: &str = "rge-editor-ui.observational-canary";

/// Tier-2 plugin adapter that observes editor coordination state per tick.
///
/// Mirrors the cad-projection / gfx / physics / audio canary pattern but
/// inverts the polarity: this canary OBSERVES rather than ADVANCES. The
/// per-tick "advancement" is the act of completing one observation cycle
/// (read selection size, put back, increment). The counter
/// `observations_completed` is the canary's success-counter per ADR-116 +
/// cross-review #8's telemetry-naming guidance.
#[derive(Debug)]
pub struct EditorUiPlugin {
    /// Number of successful observation cycles. Incremented only when the
    /// plugin's tick completes successfully (`Ok(_)` path).
    /// `ContractViolation` / `RuntimeFault` paths leave the counter
    /// unchanged. Naming follows cross-review #8's observational/editor/
    /// tooling-scope guidance — avoids runtime-loaded terms like
    /// `frames_advanced`, `steps_run`, `ticks_run`, and the mid-rejection
    /// `ui_ticks` (still scheduler-loaded).
    observations_completed: u64,
}

impl EditorUiPlugin {
    /// Build a fresh plugin with zero observations recorded.
    #[must_use]
    pub fn new() -> Self {
        Self {
            observations_completed: 0,
        }
    }

    /// Number of successful observations across all completed ticks.
    /// Increments only on the success path; failed ticks (contract
    /// violation) leave the counter unchanged. This is the canary's
    /// telemetry accessor per ADR-116 + observational-scope-naming
    /// per cross-review #8.
    #[must_use]
    pub fn observations_completed(&self) -> u64 {
        self.observations_completed
    }
}

impl Default for EditorUiPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for EditorUiPlugin {
    fn id(&self) -> PluginId {
        PluginId::new(EDITOR_UI_PLUGIN_ID)
    }

    fn name(&self) -> &'static str {
        "rge-editor-ui observational canary"
    }

    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Construction already produced the zero-state counter; observation
        // requires a Selection resource staged by the orchestrator at tick
        // time. Init is a no-op — mirrors the cad-projection / physics /
        // audio precedent of an init that does no real work (the resource
        // is caller-staged, not plugin-built).
        Ok(())
    }

    fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Take Selection: missing → ContractViolation (caller didn't stage
        // the prerequisites). The host's auto-emit downgrades
        // ContractViolation to a warning per audit-2 A5.1.
        //
        // Single-resource canary: no idempotent-failure put-back chain
        // needed (we error before holding any other resource). This is the
        // simplest take/insert shape across the five canaries.
        let selection = ctx
            .take::<Selection>()
            .ok_or_else(|| PluginError::contract_violation("Selection"))?;

        // Pure observation: query selection size. NO mutation.
        // The reading itself is the canary's observational unit-of-work;
        // the size is not preserved beyond this tick (advisory only per
        // ADR-115 phase-2.5 amendment's "Advisory" terminology entry).
        // Bound to `_observed_count` so the read is unambiguously observed
        // and not optimised away by future inliner regressions.
        let _observed_count = selection.iter().count();

        // Always put back. Slot was empty after take, so insert returns None.
        debug_assert!(
            ctx.insert(selection).is_none(),
            "Selection slot was empty after observation take"
        );

        // Pure-observation canary: no RuntimeFault surface today (the
        // observation cannot fail — `BTreeSet::iter().count()` is
        // infallible). Counter increments only on the success path,
        // honouring ADR-116's increment-only-on-success invariant.
        self.observations_completed += 1;
        Ok(())
    }

    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // No external resources held; the Selection is caller-staged
        // per-tick. Mirrors the cad-projection / gfx / physics / audio
        // precedent of a default Ok(()) shutdown.
        Ok(())
    }
}

/// ADR-116 §10.4 dogfood-rule canary protocol impl. Delegates to the
/// inherent `observations_completed` accessor; backwards-compat per
/// ADR-116 Sub-decision 2 (the inherent name preserves the observational-
/// scope vocabulary per cross-review #8; the trait method exposes the
/// uniform name for cross-canary tooling).
impl CanaryPlugin for EditorUiPlugin {
    fn successful_ticks(&self) -> u64 {
        self.observations_completed()
    }
}

#[cfg(test)]
mod tests {
    use rge_kernel_diagnostics::DiagnosticAggregator;

    use super::*;

    #[test]
    fn editor_ui_plugin_id_matches_convention() {
        let plugin = EditorUiPlugin::new();
        assert_eq!(
            plugin.id(),
            PluginId::new("rge-editor-ui.observational-canary")
        );
        assert_eq!(plugin.id().as_str(), EDITOR_UI_PLUGIN_ID);
    }

    #[test]
    fn editor_ui_plugin_name_is_stable_human_readable_string() {
        let plugin = EditorUiPlugin::new();
        assert_eq!(plugin.name(), "rge-editor-ui observational canary");
    }

    #[test]
    fn editor_ui_plugin_observations_completed_starts_at_zero() {
        let plugin = EditorUiPlugin::new();
        assert_eq!(plugin.observations_completed(), 0);
    }

    #[test]
    fn editor_ui_plugin_default_impl_matches_new() {
        let from_default: EditorUiPlugin = EditorUiPlugin::default();
        let from_new = EditorUiPlugin::new();
        assert_eq!(
            from_default.observations_completed(),
            from_new.observations_completed()
        );
    }

    #[test]
    fn editor_ui_plugin_init_succeeds_without_resources() {
        let mut plugin = EditorUiPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        // No resources inserted; init should still succeed (it's a no-op —
        // the cad-projection / physics / audio precedent).
        assert!(plugin.init(&mut ctx).is_ok());
        // Init must not have inserted anything either.
        assert_eq!(ctx.resource_count(), 0);
        // And it must NOT have advanced any state — the counter is unchanged.
        assert_eq!(plugin.observations_completed(), 0);
    }

    /// Audit-6 round-6 H5 closure (extended to the fifth canary) — error-path
    /// invariant. `observations_completed` MUST stay at 0 when `tick()`
    /// returns an error (ContractViolation here; RuntimeFault is N/A for the
    /// observational canary). Asserts the canonical
    /// "increment-only-on-success" semantics that mirrors the cad-projection
    /// / gfx / physics / audio precedents — and that ADR-116 codifies as the
    /// trait's binding contract.
    #[test]
    fn editor_ui_plugin_observations_completed_unchanged_on_contract_violation() {
        let mut plugin = EditorUiPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        // No Selection staged → tick() returns ContractViolation;
        // observations_completed must stay at 0.
        let mut ctx = PluginContext::new(&mut diags);
        let result = plugin.tick(&mut ctx);
        assert!(result.is_err(), "tick must fail without a Selection");
        match result.unwrap_err() {
            PluginError::ContractViolation { resource_type } => {
                assert_eq!(resource_type, "Selection");
            }
            other => panic!("expected ContractViolation for Selection; got {other:?}"),
        }
        assert_eq!(plugin.observations_completed(), 0);
        // No resources were left behind in the registry (single-resource
        // canary: take fails before holding anything).
        assert_eq!(ctx.resource_count(), 0);
    }

    /// Success path: tick increments the counter and leaves the Selection
    /// in the context for the orchestrator to retrieve. Verifies the
    /// pure-read-and-count contract per cross-review #8.
    #[test]
    fn editor_ui_plugin_observation_path_increments_on_success() {
        let mut plugin = EditorUiPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);

        // Stage an empty Selection — the observational canary still records
        // a successful tick because the observation succeeded (counted
        // zero entities, but the act of observing is itself the unit of
        // work).
        assert!(ctx.insert(Selection::new()).is_none());
        assert_eq!(ctx.resource_count(), 1);

        plugin.tick(&mut ctx).expect("tick must succeed");
        assert_eq!(plugin.observations_completed(), 1);

        // Selection must be put back so the orchestrator can retrieve it
        // (put-back invariant per the take/insert pattern).
        assert!(
            ctx.contains::<Selection>(),
            "Selection must be put back after tick"
        );
        assert_eq!(ctx.resource_count(), 1);
    }

    /// ADR-116 acceptance: `EditorUiPlugin` impls the `CanaryPlugin`
    /// protocol. Trait method delegates to the existing inherent
    /// `observations_completed` accessor; calling through `&dyn CanaryPlugin`
    /// exercises the dynamic-dispatch path future cross-canary tooling will
    /// use. This is the FIRST canary to land WITH the trait impl from day
    /// one (the four prior canaries adopted retroactively).
    #[test]
    fn editor_ui_plugin_impls_canary_protocol() {
        let plugin = EditorUiPlugin::new();
        let canary: &dyn CanaryPlugin = &plugin;
        assert_eq!(canary.successful_ticks(), 0);
        assert_eq!(canary.successful_ticks(), plugin.observations_completed());
    }
}
