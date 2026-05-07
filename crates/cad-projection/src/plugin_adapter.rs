//! `cad-projection::CadProjectionPlugin` — first real Tier-2 plugin canary
//! per the §10.4 dogfood rule.
//!
//! Wraps a [`CadProjection`] and impls [`rge_kernel_plugin_host::Plugin`].
//! `tick` extracts `&mut World`, `&CadGraph`, and `Tolerance` from the
//! [`PluginContext`], drives `self.projection.tick(...)`, and puts the
//! resources back. Demonstrates the v1 resource registry pattern end-to-end.
//!
//! # Why this looks duplicated across the four canaries
//!
//! The take-resources / do-work / put-resources-back shape repeats verbatim
//! across [`cad-projection::CadProjectionPlugin`](self::CadProjectionPlugin)
//! / `gfx::GfxPlugin` / `physics::PhysicsPlugin` / `audio::AudioPlugin`.
//! That repetition is **intentional** per PLAN §10.4 dogfood rule: each
//! canary is a stand-alone teaching example demonstrating the kernel
//! substrate against a different resource family. Adding a kernel-level
//! helper that wraps the take/insert pattern would be *unfair* to Tier-3
//! plugin authors (who don't have access to the helper) and would erode
//! the dogfood-rule's value (Tier-2 canaries are supposed to use only
//! what Tier-3 has). The canonical pattern documentation lives at
//! [`docs/§18/PLUGIN_HOST_PATTERNS.md`](../../../docs/§18/PLUGIN_HOST_PATTERNS.md);
//! design rationale + rejected alternatives at
//! [`docs/adr/ADR-114-pluginctx-owned-handoff.md`](../../../docs/adr/ADR-114-pluginctx-owned-handoff.md).
//! Other canaries cross-reference this section.
//!
//! # Why this exists
//!
//! Closes Pairing-3 of the 2026-05-07 deep audit. Before this dispatch, the
//! dogfood-smoke had to use a fixture (`TestTier2Plugin`) that didn't
//! exercise any real subsystem. With the v1 [`PluginContext`] + this
//! adapter, a real subsystem (cad-projection) can express its full lifecycle
//! through the same trait Tier-3 plugins use.
//!
//! # Resource contract
//!
//! On `tick`, the plugin context MUST contain (caller-supplied):
//!
//! * [`World`] — owned `&mut` after `take`; mutated by the projection.
//! * [`CadGraph`] — read-only graph snapshot consulted during projection.
//! * [`Tolerance`] — tessellation tolerance (Copy; defaulted to 0.001 if
//!   the caller doesn't supply one).
//!
//! Missing `World` or `CadGraph` resources surface as
//! [`PluginError::ContractViolation`] (caller-supplied resource missing —
//! not a plugin-side bug; auto-emit downgrades to a warning). Genuine
//! tick-time errors from the projection itself surface as
//! [`PluginError::RuntimeFault`]. The projection state is preserved
//! (idempotent failure semantics — whatever resources WERE supplied are put
//! back into the context before the error propagates).

use rge_cad_core::{CadGraph, Tolerance};
use rge_kernel_ecs::World;
use rge_kernel_plugin_host::{Plugin, PluginContext, PluginError, PluginId};

use crate::CadProjection;

/// Stable [`PluginId`] reported by every [`CadProjectionPlugin`] instance.
pub const CAD_PROJECTION_PLUGIN_ID: &str = "cad-projection.brep-handles-plugin";

/// Tier-2 plugin adapter wrapping a [`CadProjection`].
///
/// Exposes the projection's tick lifecycle through the unified [`Plugin`]
/// trait per PLAN §10.4 dogfood rule. The adapter is a thin shim: all real
/// work happens inside `self.projection.tick(...)`. The adapter's job is to
/// (1) extract resources from the [`PluginContext`], (2) drive the
/// projection, and (3) put the resources back so the orchestrator can
/// retrieve them.
#[derive(Debug)]
pub struct CadProjectionPlugin {
    projection: CadProjection,
    /// Number of successful `tick()` calls. Incremented only when the
    /// projection's inner work succeeds (errors don't increment). Telemetry
    /// accessor for canary parity with gfx::GfxPlugin::frames_recorded /
    /// physics::PhysicsPlugin::steps_run / audio::AudioPlugin::steps_run
    /// (closes audit-6 round-6 H5 finding: canary accessor symmetry).
    ticks_run: u64,
}

impl CadProjectionPlugin {
    /// Build a plugin around a fresh empty projection.
    #[must_use]
    pub fn new() -> Self {
        Self {
            projection: CadProjection::new(),
            ticks_run: 0,
        }
    }

    /// Build a plugin wrapping an existing projection.
    #[must_use]
    pub fn from_projection(projection: CadProjection) -> Self {
        Self {
            projection,
            ticks_run: 0,
        }
    }

    /// Take ownership of the wrapped projection (e.g. for snapshotting or
    /// reuse after the plugin's lifecycle has finished).
    #[must_use]
    pub fn into_projection(self) -> CadProjection {
        self.projection
    }

    /// Borrow the wrapped projection.
    #[must_use]
    pub fn projection(&self) -> &CadProjection {
        &self.projection
    }

    /// Borrow the wrapped projection mutably (e.g. to spawn entities before
    /// registering with the host).
    pub fn projection_mut(&mut self) -> &mut CadProjection {
        &mut self.projection
    }

    /// Number of successful [`Plugin::tick`] calls observed by this plugin.
    ///
    /// Telemetry accessor for canary parity with the rest of the §10.4
    /// dogfood-rule canaries (gfx::GfxPlugin::frames_recorded /
    /// physics::PhysicsPlugin::steps_run / audio::AudioPlugin::steps_run).
    /// Incremented only on successful ticks; ContractViolation /
    /// RuntimeFault paths do NOT increment. Useful for tests asserting
    /// "tick was called N times".
    #[must_use]
    pub fn ticks_run(&self) -> u64 {
        self.ticks_run
    }
}

impl Default for CadProjectionPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for CadProjectionPlugin {
    fn id(&self) -> PluginId {
        PluginId::new(CAD_PROJECTION_PLUGIN_ID)
    }

    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // CadProjection::new() already produced the empty initial state;
        // nothing further to do at init.
        Ok(())
    }

    fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Sequential takes — each `take` releases the borrow on `ctx`
        // immediately so the next `take` / `insert` is unhindered. If a
        // required resource is missing, restore whatever we already took
        // before erroring (idempotent failure semantics).
        //
        // Missing-resource cases are CONTRACT violations (caller didn't
        // stage prerequisites) — distinct from RUNTIME faults coming out
        // of the projection itself. The host's auto-emit downgrades
        // ContractViolation to a warning per audit-2 A5.1.
        let mut world = ctx
            .take::<World>()
            .ok_or_else(|| PluginError::contract_violation("World"))?;
        let Some(cad) = ctx.take::<CadGraph>() else {
            // Put World back before erroring out so the orchestrator can
            // recover its handle.
            let replaced = ctx.insert(world);
            debug_assert!(replaced.is_none(), "World slot was empty after take");
            return Err(PluginError::contract_violation("CadGraph"));
        };
        // Tolerance is optional — fall back to a default 0.001m if the
        // caller didn't supply one. This matches the convention used in
        // cad-projection's own integration tests.
        let tolerance = ctx
            .take::<Tolerance>()
            .unwrap_or_else(|| Tolerance::new(0.001).expect("default 0.001 tolerance is valid"));

        let result = self.projection.tick(&mut world, &cad, tolerance);

        // Always put resources back, even on failure, so the orchestrator
        // can retrieve them. The plugin is responsible for not leaving the
        // ctx in a dirty state. The slots are empty (we just took from
        // them), so insert returns None — no resource is dropped on the
        // floor.
        debug_assert!(ctx.insert(world).is_none(), "World slot was empty");
        debug_assert!(ctx.insert(cad).is_none(), "CadGraph slot was empty");
        let _ = ctx.insert(tolerance);

        match result {
            Ok(_report) => {
                self.ticks_run += 1;
                Ok(())
            }
            Err(e) => Err(PluginError::runtime_fault(format!(
                "CadProjectionPlugin.tick: projection failed: {e}"
            ))),
        }
    }

    fn shutdown(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // No external resources held; projection is dropped with the plugin.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rge_kernel_diagnostics::DiagnosticAggregator;

    use super::*;

    #[test]
    fn cad_projection_plugin_id_matches_convention() {
        let plugin = CadProjectionPlugin::new();
        assert_eq!(
            plugin.id(),
            PluginId::new("cad-projection.brep-handles-plugin")
        );
        assert_eq!(plugin.id().as_str(), CAD_PROJECTION_PLUGIN_ID);
    }

    #[test]
    fn cad_projection_plugin_init_succeeds_without_resources() {
        let mut plugin = CadProjectionPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        let mut ctx = PluginContext::new(&mut diags);
        // No resources inserted; init should still succeed (it's a no-op).
        assert!(plugin.init(&mut ctx).is_ok());
        // Init must not have inserted anything either.
        assert_eq!(ctx.resource_count(), 0);
    }

    #[test]
    fn cad_projection_plugin_into_projection_returns_owned() {
        let projection = CadProjection::new();
        let plugin = CadProjectionPlugin::from_projection(projection);
        let recovered = plugin.into_projection();
        // Recovered projection is empty (just like the one we started with);
        // a fabricated raw NodeId yields no mapping.
        let fabricated_node = rge_kernel_graph_foundation::NodeId::from_raw(0);
        assert_eq!(recovered.entity_for(fabricated_node), None);
    }

    /// Audit-6 round-6 H5 closure — canary accessor symmetry.
    ///
    /// The 4 §10.4 dogfood-rule canaries (cad-projection / gfx /
    /// physics / audio) now each expose a telemetry accessor for
    /// "successful tick count":
    ///
    /// * cad-projection: `ticks_run()` — this method
    /// * gfx: `frames_recorded()`
    /// * physics: `steps_run()`
    /// * audio: `steps_run()`
    ///
    /// Pre-H5 the cad-projection canary lacked one (audit asymmetry
    /// finding). The accessor returns 0 on a fresh plugin and is
    /// incremented exclusively on successful ticks; ContractViolation
    /// + RuntimeFault paths leave it unchanged.
    #[test]
    fn cad_projection_plugin_ticks_run_starts_at_zero() {
        let plugin = CadProjectionPlugin::new();
        assert_eq!(plugin.ticks_run(), 0);
        let plugin_from = CadProjectionPlugin::from_projection(CadProjection::new());
        assert_eq!(plugin_from.ticks_run(), 0);
    }

    /// Audit-6 round-6 H5 closure — error-path invariant.
    ///
    /// `ticks_run` MUST stay at 0 when `tick()` returns an error
    /// (ContractViolation or RuntimeFault). Asserts the canonical
    /// "increment-only-on-success" semantics that mirrors gfx + physics
    /// + audio canaries.
    #[test]
    fn cad_projection_plugin_ticks_run_unchanged_on_contract_violation() {
        let mut plugin = CadProjectionPlugin::new();
        let mut diags = DiagnosticAggregator::new();
        // No resources staged → tick() returns ContractViolation for
        // missing World; ticks_run must stay at 0.
        let mut ctx = PluginContext::new(&mut diags);
        let result = plugin.tick(&mut ctx);
        assert!(result.is_err());
        assert_eq!(plugin.ticks_run(), 0);
    }
}
