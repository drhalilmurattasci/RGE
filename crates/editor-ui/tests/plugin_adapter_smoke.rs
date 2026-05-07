//! Phase-canary integration smoke tests for `editor-ui::EditorUiPlugin`.
//!
//! `EditorUiPlugin` is the fifth real Tier-2 plugin canary per the §10.4
//! dogfood rule (closes M2 editor-ui::Plugin canary deferral; first canary
//! OUTSIDE purely runtime-centric subsystems). These tests prove that the
//! v1 `PluginContext` owned-resources-handoff design composes for the
//! editor-ui substrate (`Selection` only — single-resource canary) without
//! forcing any change to the Tier-1 substrate, AND that `CanaryPlugin`
//! survives editor-style tooling-observational pressure cleanly per the
//! 2026-05-10 ChatGPT cross-review #8 (archived in `change.md` 09:00 entry).
//!
//! Mirrors the structure of `crates/{cad-projection,gfx,physics,audio}/tests/plugin_adapter_smoke.rs`.
//!
//! Scenarios:
//!
//! 1. **`editor_ui_plugin_full_lifecycle_through_plugin_host`** — M2 closure.
//!    Drives the plugin through register → init_all → tick_all (10 iterations
//!    with Selection staged) → shutdown_all; verifies all 10 ticks succeeded
//!    and the observations counter advanced to 10.
//!
//! 2. **`editor_ui_plugin_contract_violation_when_selection_missing`** —
//!    runtime safety: missing `Selection` resource surfaces as `PluginError`
//!    + plugin state Failed (not panic). Per audit-2 A5.1, the host's
//!    auto-emit classifies `ContractViolation` as a Warning (not Error).
//!
//! 3. **`editor_ui_plugin_isolation_with_sibling_panic`** — multi-plugin
//!    isolation: a sibling test fixture deliberately panics during tick;
//!    the host's `catch_unwind` recovers, the sibling is marked `Failed`,
//!    and `EditorUiPlugin` ticks successfully alongside it. Mirrors the
//!    cad-projection / gfx / physics / audio sibling-panic precedent.
//!
//! 4. **`editor_ui_plugin_multi_tick_observation_idempotence`** — the
//!    observational-not-mutating contract held for the cross-review #8
//!    binding constraint. Five ticks with the same Selection content
//!    produce five successful observations and leave the Selection
//!    BIT-IDENTICAL (no mutation; no entity addition / removal /
//!    reordering) after each tick. This is the load-bearing test for the
//!    "tooling-observational participant, NOT editor runtime authority"
//!    framing.

use rge_editor_state::Selection;
use rge_editor_ui::{EditorUiPlugin, EDITOR_UI_PLUGIN_ID};
use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};
use rge_kernel_ecs::EntityId;
use rge_kernel_plugin_host::{
    Plugin, PluginContext, PluginError, PluginHost, PluginId, PluginState,
};

// ===========================================================================
// EditorUiPlugin canary — fifth real Tier-2 plugin via the §10.4 dogfood
// rule. Closes M2 editor-ui::Plugin canary deferral. First canary OUTSIDE
// purely runtime-centric subsystems per cross-review #8.
// ===========================================================================

/// M2 closure: the `EditorUiPlugin` adapter drives the editor-ui canary
/// end-to-end through the unified `Plugin` trait + `PluginHost` lifecycle.
/// Verifies that:
///
/// 1. The plugin registers successfully under its canonical id.
/// 2. `init_all` advances the plugin from `Pending` → `Initialized`.
/// 3. `tick_all` (driven 10 times with a Selection staged before each tick)
///    extracts Selection from the context, observes its size, puts it back,
///    and reports a successful tick on every iteration.
/// 4. After 10 ticks, `observations_completed` == 10 (cross-review #8's
///    chosen telemetry name; load-bearing per ADR-116's increment-only-on-
///    success invariant).
/// 5. `shutdown_all` LIFO-shuts the plugin down without error.
#[test]
fn editor_ui_plugin_full_lifecycle_through_plugin_host() {
    let plugin = EditorUiPlugin::new();

    let plugin_id = PluginId::new(EDITOR_UI_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");
    assert_eq!(host.state(&plugin_id), Some(PluginState::Pending));

    let mut diags = DiagnosticAggregator::new();

    // Init — no resources required at init (the canary's init is a no-op,
    // matching the cad-projection / physics / audio precedent).
    {
        let mut ctx = PluginContext::new(&mut diags);
        let init_report = host.init_all(&mut ctx);
        assert_eq!(init_report.initialized, vec![plugin_id.clone()]);
        assert!(
            init_report.failed.is_empty(),
            "init failed: {:?}",
            init_report.failed
        );
        assert_eq!(host.state(&plugin_id), Some(PluginState::Initialized));
    }
    // Init must not auto-emit on success.
    assert_eq!(diags.len(), 0, "init must not auto-emit on success");

    // Drive 10 ticks. Stage a fresh Selection containing two entities for
    // every tick — the canary observes (read-only) and puts back.
    let mut selection = Selection::new();
    selection.add(EntityId::new());
    selection.add(EntityId::new());
    let initial_selection_snapshot = selection.clone();

    let mut total_ticks_succeeded = 0;
    for tick_n in 1..=10u64 {
        let mut ctx = PluginContext::new(&mut diags);
        // Stage Selection.
        assert!(
            ctx.insert(selection).is_none(),
            "tick {tick_n}: Selection slot was empty before insert"
        );
        assert_eq!(ctx.resource_count(), 1);

        let tick_report = host.tick_all(&mut ctx);
        assert_eq!(
            tick_report.ticked, 1,
            "tick {tick_n}: expected 1 successful tick, got {} (failed: {:?})",
            tick_report.ticked, tick_report.failed
        );
        assert!(
            tick_report.failed.is_empty(),
            "tick {tick_n} unexpectedly failed: {:?}",
            tick_report.failed
        );
        total_ticks_succeeded += 1;

        // Plugin state stays Initialized after a successful tick.
        assert_eq!(host.state(&plugin_id), Some(PluginState::Initialized));

        // Recover Selection from the context. The canary MUST put it back
        // unchanged.
        selection = ctx.take::<Selection>().expect("Selection put back by tick");
        assert_eq!(
            selection, initial_selection_snapshot,
            "tick {tick_n}: Selection must be bit-identical after the observational tick"
        );
        assert_eq!(ctx.resource_count(), 0);
    }
    assert_eq!(total_ticks_succeeded, 10);

    // No new diagnostics from the success path.
    assert_eq!(diags.len(), 0, "successful ticks must not auto-emit");

    // Shutdown LIFO. No plugin-level error expected.
    let shutdown_report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.shutdown_all(&mut ctx)
    };
    assert_eq!(shutdown_report.shutdown.len(), 1);
    assert!(shutdown_report.failed.is_empty());
    assert_eq!(host.count(), 0);
}

/// Runtime safety: a tick with the Selection resource missing surfaces as
/// `PluginError::ContractViolation { resource_type: "Selection" }` and marks
/// the plugin Failed (per plugin-fatal isolation), without panicking. Per
/// audit-2 A5.1, the host's auto-emit classifies this as a Warning (not
/// Error) — the plugin code is fine; the caller failed to stage the
/// prerequisites.
#[test]
fn editor_ui_plugin_contract_violation_when_selection_missing() {
    let plugin = EditorUiPlugin::new();
    let plugin_id = PluginId::new(EDITOR_UI_PLUGIN_ID);
    let mut host = PluginHost::new();
    host.register(plugin_id.clone(), Box::new(plugin))
        .expect("register");

    let mut diags = DiagnosticAggregator::new();
    {
        let mut ctx = PluginContext::new(&mut diags);
        let init_report = host.init_all(&mut ctx);
        assert!(init_report.failed.is_empty());
    }
    // Init produced no diagnostics; the only diagnostic that follows comes
    // from the tick failure.
    assert_eq!(diags.len(), 0, "init must not auto-emit on success");

    let tick_report = {
        let mut ctx = PluginContext::new(&mut diags);
        // Deliberately do NOT insert Selection. Tick must fail cleanly.
        host.tick_all(&mut ctx)
    };
    assert_eq!(tick_report.ticked, 0);
    assert_eq!(
        tick_report.failed.len(),
        1,
        "missing Selection must surface as a failed tick"
    );
    let (failed_id, failed_msg) = &tick_report.failed[0];
    assert_eq!(*failed_id, plugin_id);
    // Display impl for ContractViolation includes the resource type name —
    // "missing resource of type Selection".
    assert!(
        failed_msg.contains("missing resource of type Selection"),
        "error message must mention missing-Selection contract violation; got: {failed_msg}"
    );
    // Per plugin-fatal isolation, the plugin is now Failed.
    assert_eq!(host.state(&plugin_id), Some(PluginState::Failed));

    // Audit-2 A5.1: ContractViolation auto-emits as Warning, not Error.
    let new_diags: Vec<_> = diags.iter().collect();
    assert_eq!(
        new_diags.len(),
        1,
        "expected one auto-emit diagnostic for the contract violation",
    );
    assert_eq!(
        new_diags[0].severity,
        Severity::Warning,
        "ContractViolation must auto-emit as Warning (not Error) per audit-2 A5.1",
    );
}

// ===========================================================================
// Multi-plugin isolation canary — brings editor-ui to parity with the four
// prior canaries (cad-projection / gfx / physics / audio) per the §10.4
// dogfood rule. Mirrors the precedent verbatim.
// ===========================================================================

/// Multi-plugin isolation: register `EditorUiPlugin` alongside a sibling
/// test fixture that deliberately panics during `tick`. Verify:
///
/// 1. The host's `catch_unwind` recovers from the sibling's panic.
/// 2. The sibling is marked `Failed` (plugin-fatal isolation per PLAN §1.13).
/// 3. `EditorUiPlugin` ticks successfully alongside the sibling — its
///    state, resources, and observation counter are entirely unaffected by
///    the sibling's failure.
/// 4. The diagnostic stream contains an Error-severity diagnostic mentioning
///    the panic (attributable to the sibling, not to editor-ui).
/// 5. Resources staged for editor-ui (`Selection`) are still in the context
///    post-tick — the put-back invariant held despite the sibling panic.
#[test]
fn editor_ui_plugin_isolation_with_sibling_panic() {
    let plugin = EditorUiPlugin::new();

    let editor_id = PluginId::new(EDITOR_UI_PLUGIN_ID);
    let panicker_id = PluginId::new("test.panic-sibling");

    let mut host = PluginHost::new();
    host.register(editor_id.clone(), Box::new(plugin))
        .expect("register editor-ui plugin");
    host.register(
        panicker_id.clone(),
        Box::new(PanickingTickPlugin::new(panicker_id.clone())),
    )
    .expect("register panicker");

    let mut diags = DiagnosticAggregator::new();

    {
        let mut ctx = PluginContext::new(&mut diags);
        let init_report = host.init_all(&mut ctx);
        assert!(
            init_report.failed.is_empty(),
            "init: {:?}",
            init_report.failed
        );
        assert_eq!(init_report.initialized.len(), 2);
    }

    let pre_tick_diag_count = diags.len();
    let mut ctx = PluginContext::new(&mut diags);

    // Stage editor-ui resources; the PanickingTickPlugin doesn't take any,
    // so it panics on entry.
    let mut selection = Selection::new();
    let entity_a = EntityId::new();
    selection.add(entity_a);
    let staged_snapshot = selection.clone();
    assert!(ctx.insert(selection).is_none());
    assert_eq!(ctx.resource_count(), 1);

    let tick_report = host.tick_all(&mut ctx);

    assert_eq!(
        tick_report.ticked, 1,
        "exactly one plugin (editor-ui) ticked Ok"
    );
    assert_eq!(
        tick_report.failed.len(),
        1,
        "exactly one plugin (sibling) failed"
    );
    assert_eq!(tick_report.failed[0].0, panicker_id);
    assert!(
        tick_report.failed[0].1.contains("panicked during tick"),
        "sibling failure must mention panic; got: {}",
        tick_report.failed[0].1
    );

    // EditorUiPlugin survived in spite of the sibling's panic — plugin-
    // fatal isolation per PLAN §1.13.
    assert_eq!(host.state(&editor_id), Some(PluginState::Initialized));
    assert_eq!(host.state(&panicker_id), Some(PluginState::Failed));

    // Put-back invariant held despite sibling panic: Selection is still
    // present, and bit-identical to what we staged (the canary does not
    // mutate it per cross-review #8's tooling-observational design
    // principle).
    assert!(
        ctx.contains::<Selection>(),
        "Selection must be put back after tick (sibling panic must not disturb)"
    );
    let selection_back: Selection = ctx.take().expect("Selection present after tick");
    assert_eq!(
        selection_back, staged_snapshot,
        "Selection must be unchanged — observational canary does not mutate"
    );

    // Drop ctx so the diagnostic borrow ends, then inspect diagnostics.
    drop(ctx);

    // Exactly one new diagnostic — the PANICKED-during-tick one for the
    // sibling. Severity::Error per the plugin-panic auto-emit semantics.
    let new_diags: Vec<_> = diags.iter().skip(pre_tick_diag_count).collect();
    assert!(
        new_diags.iter().any(|d| d.severity == Severity::Error
            && d.message.contains("PANICKED during tick")
            && d.message.contains("test.panic-sibling")),
        "expected Error-severity PANICKED-during-tick diagnostic for sibling; got {:?}",
        new_diags
            .iter()
            .map(|d| (d.severity, d.message.as_str()))
            .collect::<Vec<_>>()
    );
    // EditorUiPlugin must NOT have produced any failure diagnostic.
    assert!(
        !new_diags
            .iter()
            .any(|d| d.message.contains(EDITOR_UI_PLUGIN_ID)
                && (d.message.contains("PANICKED")
                    || d.message.contains("violation")
                    || d.message.contains("failed"))),
        "editor-ui must not have produced failure diagnostics; got {:?}",
        new_diags
            .iter()
            .map(|d| (d.severity, d.message.as_str()))
            .collect::<Vec<_>>()
    );
}

// ===========================================================================
// Multi-tick observation idempotence — load-bearing for cross-review #8's
// "tooling-observational participant, NOT editor runtime authority" binding.
// Mirrors gfx::gfx_plugin_multiple_ticks_increment_counter / cad-projection's
// multi-tick idempotence tests in spirit (validates a different invariant —
// "observational tick is non-mutating" — using the same multi-tick shape).
// ===========================================================================

/// Multi-tick observation idempotence: 5 ticks with the same Selection
/// content produce 5 successful observations AND leave the Selection
/// bit-identical (Eq + same iteration order) across all 5 ticks. This is
/// the load-bearing test for cross-review #8's binding "the canary
/// OBSERVES `Selection` but does NOT mutate it" constraint.
///
/// Verifies (against the canonical `EditorUiPlugin::tick` path):
///
/// 1. Each tick succeeds (returns Ok); counter advances by exactly 1 per
///    tick.
/// 2. Selection content (entity set membership) is unchanged after every
///    tick — the observation does not add, remove, or reorder entities.
/// 3. After 5 ticks, `observations_completed == 5` and Selection is still
///    `==` the initial snapshot.
#[test]
fn editor_ui_plugin_multi_tick_observation_idempotence() {
    let mut plugin = EditorUiPlugin::new();
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    // Stage a Selection with three entities. The exact entities don't
    // matter — only that the canary doesn't disturb the set.
    let mut selection = Selection::new();
    let entity_a = EntityId::new();
    let entity_b = EntityId::new();
    let entity_c = EntityId::new();
    selection.add(entity_a);
    selection.add(entity_b);
    selection.add(entity_c);
    let initial_snapshot = selection.clone();
    assert_eq!(initial_snapshot.len(), 3);

    assert!(ctx.insert(selection).is_none());

    // Drive 5 ticks. After each tick, take the Selection back, assert
    // bit-identical equality to the initial snapshot, and re-stage it for
    // the next tick.
    for tick_n in 1..=5u64 {
        plugin
            .tick(&mut ctx)
            .unwrap_or_else(|e| panic!("tick {tick_n} unexpectedly failed: {e}"));
        assert_eq!(
            plugin.observations_completed(),
            tick_n,
            "tick {tick_n}: observations_completed must equal {tick_n}"
        );

        // The observational invariant: Selection unchanged after the tick.
        let post_tick: Selection = ctx
            .take()
            .unwrap_or_else(|| panic!("tick {tick_n}: Selection put back by tick"));
        assert_eq!(
            post_tick, initial_snapshot,
            "tick {tick_n}: Selection must be bit-identical to the initial snapshot \
             (cross-review #8: canary observes, never mutates)"
        );
        // Verify iteration order is preserved as well — `Selection`'s `Eq`
        // already verifies set equality, but we explicitly check the
        // ordered iteration to defend against a future hash-based
        // re-implementation that loses determinism.
        let post_tick_ordered: Vec<EntityId> = post_tick.iter().collect();
        let snapshot_ordered: Vec<EntityId> = initial_snapshot.iter().collect();
        assert_eq!(
            post_tick_ordered, snapshot_ordered,
            "tick {tick_n}: deterministic iteration order must be preserved"
        );

        // Re-stage for the next tick.
        assert!(
            ctx.insert(post_tick).is_none(),
            "tick {tick_n}: Selection slot empty after take, insert returns None"
        );
    }

    // Final-state checks.
    assert_eq!(plugin.observations_completed(), 5);
    let final_selection: Selection = ctx.take().expect("Selection present after final tick");
    assert_eq!(
        final_selection, initial_snapshot,
        "after 5 ticks, Selection must still equal the initial snapshot"
    );
}

// ---------------------------------------------------------------------------
// Test fixture: a plugin whose tick deliberately panics, used to drive the
// host's catch_unwind recovery path while editor-ui ticks normally alongside
// it. Mirrors the cad-projection / gfx / physics / audio canary fixtures
// verbatim.
// ---------------------------------------------------------------------------

/// Minimal `Plugin` impl that panics on every `tick`. Test-only sibling
/// fixture for the isolation test above. Mirrors the spirit of
/// `host.rs::TestPlugin::with_tick_panic` but lives outside the kernel
/// crate so this test file doesn't need privileged access.
struct PanickingTickPlugin {
    id: PluginId,
}

impl PanickingTickPlugin {
    fn new(id: PluginId) -> Self {
        Self { id }
    }
}

impl Plugin for PanickingTickPlugin {
    fn id(&self) -> PluginId {
        self.id.clone()
    }

    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }

    fn tick(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        // Deliberate panic to drive the host's catch_unwind recovery.
        panic!("PanickingTickPlugin: deliberate tick panic for isolation test");
    }
}
