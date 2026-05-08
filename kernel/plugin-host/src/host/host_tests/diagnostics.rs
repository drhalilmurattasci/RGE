//! Diagnostic auto-emit policy tests.
//!
//! Sub-module of [`crate::host::host_tests`]; covers the Pairing-5 invariant
//! ("the diagnostic stream is the single source of truth for plugin
//! failures") and the audit-2 A5.1 severity discrimination rule
//! ([`crate::plugin::PluginError::ContractViolation`] = Warning;
//! everything else = Error). Companion of `panic_recovery.rs` (which
//! checks the PANICKED-prefixed diagnostic) and `resource_leak.rs`
//! (which checks the leaked-resource diagnostic).

use std::sync::{Arc, Mutex};

use rge_kernel_diagnostics::{DiagnosticAggregator, Severity};

use super::fixtures::TestPlugin;
use crate::context::PluginContext;
use crate::host::PluginHost;
use crate::plugin::PluginId;

#[test]
fn init_all_auto_emits_diagnostic_on_plugin_init_failure() {
    // Pairing-5 closure: a plugin that fails init produces a synthetic
    // Diagnostic::error in the sink, even if the plugin itself doesn't
    // call ctx.emit_diagnostic. The host is the single source of truth
    // for plugin-failure surfacing.
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("a"),
        Box::new(TestPlugin::new("a", log.clone()).with_init_failure()),
    )
    .expect("register");

    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx)
    };
    assert_eq!(report.failed.len(), 1);
    // Auto-emit produced exactly one error diagnostic.
    assert_eq!(diags.len(), 1);
    assert!(diags.has_errors());
    let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages[0].starts_with("plugin a init failed:"),
        "expected auto-emit prefix; got: {}",
        messages[0]
    );
}

#[test]
fn init_all_does_not_auto_emit_diagnostic_on_success() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("a"),
        Box::new(TestPlugin::new("a", log.clone())),
    )
    .expect("register");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    // Successful init produces no auto-emit (the plugin can still emit
    // its own diagnostics, but TestPlugin doesn't).
    assert_eq!(diags.len(), 0);
}

#[test]
fn tick_all_auto_emits_diagnostic_on_plugin_tick_failure() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("a"),
        Box::new(TestPlugin::new("a", log.clone()).with_tick_failure()),
    )
    .expect("register");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    // After successful init, sink is empty.
    assert_eq!(diags.len(), 0);

    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.tick_all(&mut ctx)
    };
    assert_eq!(report.failed.len(), 1);
    assert_eq!(diags.len(), 1);
    let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
    assert!(messages[0].starts_with("plugin a tick failed:"));
}

#[test]
fn shutdown_all_auto_emits_diagnostic_on_plugin_shutdown_failure() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("a"),
        Box::new(TestPlugin::new("a", log.clone()).with_shutdown_failure()),
    )
    .expect("register");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    assert_eq!(diags.len(), 0);

    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.shutdown_all(&mut ctx)
    };
    assert_eq!(report.failed.len(), 1);
    assert_eq!(diags.len(), 1);
    let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
    assert!(messages[0].starts_with("plugin a shutdown failed:"));
}

#[test]
fn init_all_auto_emits_one_diagnostic_per_failing_plugin() {
    // 3 plugins; b and c both fail init; expect exactly 2 auto-emits.
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("a"),
        Box::new(TestPlugin::new("a", log.clone())),
    )
    .expect("a");
    host.register(
        PluginId::new("b"),
        Box::new(TestPlugin::new("b", log.clone()).with_init_failure()),
    )
    .expect("b");
    host.register(
        PluginId::new("c"),
        Box::new(TestPlugin::new("c", log.clone()).with_init_failure()),
    )
    .expect("c");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    // Two auto-emits, one per failure.
    assert_eq!(diags.len(), 2);
    let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
    assert!(messages[0].starts_with("plugin b init failed:"));
    assert!(messages[1].starts_with("plugin c init failed:"));
}

/// Severity discrimination: a plugin returning `PluginError::ContractViolation`
/// produces a Warning auto-emit, NOT an Error. Other plugin errors continue
/// to produce Errors.
#[test]
fn tick_all_emits_warning_for_contract_violation() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("contract"),
        Box::new(TestPlugin::new("contract", log.clone()).with_contract_violation_in_tick()),
    )
    .expect("register");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    let pre_tick_diag_count = diags.len();
    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.tick_all(&mut ctx)
    };

    assert_eq!(report.ticked, 0);
    assert_eq!(report.failed.len(), 1);

    let new_diags: Vec<_> = diags.iter().skip(pre_tick_diag_count).collect();
    assert_eq!(
        new_diags.len(),
        1,
        "expected one warning diagnostic; got {} = {:?}",
        new_diags.len(),
        new_diags
            .iter()
            .map(|d| (d.severity, d.message.as_str()))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        new_diags[0].severity,
        Severity::Warning,
        "ContractViolation must auto-emit as Warning, not Error",
    );
    assert!(
        new_diags[0].message.contains("contract violation"),
        "warning should reference contract violation; got: {}",
        new_diags[0].message,
    );
}

/// Severity discrimination companion to `tick_all_emits_warning_for_contract_violation`:
/// a plugin returning `PluginError::RuntimeFault` from tick produces an
/// **Error** auto-emit (NOT Warning). This locks in the audit-2 Phase 0
/// auto-emit policy split — `ContractViolation` = caller misconfiguration
/// (Warning); `RuntimeFault` = plugin's own logic returned Err (Error). The
/// existing `tick_all_auto_emits_diagnostic_on_plugin_tick_failure` test
/// asserts the auto-emit fires + message prefix; this test asserts the
/// severity discrimination so future refactors can't silently downgrade
/// `RuntimeFault` to Warning.
#[test]
fn tick_all_emits_error_for_runtime_fault() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("rt"),
        Box::new(TestPlugin::new("rt", log.clone()).with_tick_failure()),
    )
    .expect("register");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    let pre_tick_diag_count = diags.len();
    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.tick_all(&mut ctx)
    };

    assert_eq!(report.ticked, 0);
    assert_eq!(report.failed.len(), 1);

    let new_diags: Vec<_> = diags.iter().skip(pre_tick_diag_count).collect();
    assert_eq!(new_diags.len(), 1, "expected exactly one auto-emit");
    assert_eq!(
        new_diags[0].severity,
        Severity::Error,
        "RuntimeFault must auto-emit as Error, not Warning (Warning is reserved for ContractViolation per audit-2 A5.1)",
    );
    assert!(
        new_diags[0].message.contains("tick failed"),
        "error should reference tick failure; got: {}",
        new_diags[0].message,
    );
    assert!(
        !new_diags[0].message.contains("contract violation"),
        "RuntimeFault message must NOT reference contract-violation framing; got: {}",
        new_diags[0].message,
    );
}

// ===== PluginError × PluginPhase auto-emit matrix =====
//
// The host's `emit_plugin_err_diagnostic` helper (host.rs) dispatches
// severity by VARIANT only, not phase: ContractViolation → Warning,
// everything else → Error. The four tests below pin that phase-agnostic
// dispatch for the Init and Shutdown phases (Tick is already covered by
// `tick_all_emits_warning_for_contract_violation` and
// `tick_all_emits_error_for_runtime_fault` above). Together they close
// the 4-cell coverage gap noted in HANDOFF.md backlog
// ("ContractViolation × Init/Shutdown + RuntimeFault × Init/Shutdown").

/// ContractViolation in init must auto-emit as Warning, not Error.
/// Phase-symmetry counterpart of `tick_all_emits_warning_for_contract_violation`.
#[test]
fn init_all_emits_warning_for_contract_violation() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("init-cv"),
        Box::new(TestPlugin::new("init-cv", log.clone()).with_contract_violation_in_init()),
    )
    .expect("register");

    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx)
    };

    assert_eq!(report.initialized.len(), 0);
    assert_eq!(report.failed.len(), 1);

    let new_diags: Vec<_> = diags.iter().collect();
    assert_eq!(
        new_diags.len(),
        1,
        "expected exactly one auto-emit for the failed init",
    );
    assert_eq!(
        new_diags[0].severity,
        Severity::Warning,
        "ContractViolation in init must auto-emit as Warning, not Error",
    );
    assert!(
        new_diags[0].message.contains("contract violation"),
        "warning should reference contract violation; got: {}",
        new_diags[0].message,
    );
}

/// ContractViolation in shutdown must auto-emit as Warning, not Error.
/// Phase-symmetry counterpart of `tick_all_emits_warning_for_contract_violation`.
#[test]
fn shutdown_all_emits_warning_for_contract_violation() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("sh-cv"),
        Box::new(TestPlugin::new("sh-cv", log.clone()).with_contract_violation_in_shutdown()),
    )
    .expect("register");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    let pre_shutdown_diag_count = diags.len();

    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.shutdown_all(&mut ctx)
    };

    assert_eq!(report.shutdown.len(), 0);
    assert_eq!(report.failed.len(), 1);

    let new_diags: Vec<_> = diags.iter().skip(pre_shutdown_diag_count).collect();
    assert_eq!(
        new_diags.len(),
        1,
        "expected exactly one auto-emit for the failed shutdown",
    );
    assert_eq!(
        new_diags[0].severity,
        Severity::Warning,
        "ContractViolation in shutdown must auto-emit as Warning, not Error",
    );
    assert!(
        new_diags[0].message.contains("contract violation"),
        "warning should reference contract violation; got: {}",
        new_diags[0].message,
    );
}

/// RuntimeFault in init must auto-emit as Error, not Warning.
/// Phase-symmetry counterpart of `tick_all_emits_error_for_runtime_fault`.
/// Distinguishes from `init_all_auto_emits_diagnostic_on_plugin_init_failure`
/// above (which uses `InitFailed`); this test pins the discrimination
/// against ContractViolation specifically.
#[test]
fn init_all_emits_error_for_runtime_fault() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("init-rf"),
        Box::new(TestPlugin::new("init-rf", log.clone()).with_runtime_fault_in_init()),
    )
    .expect("register");

    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx)
    };

    assert_eq!(report.initialized.len(), 0);
    assert_eq!(report.failed.len(), 1);

    let new_diags: Vec<_> = diags.iter().collect();
    assert_eq!(new_diags.len(), 1, "expected exactly one auto-emit");
    assert_eq!(
        new_diags[0].severity,
        Severity::Error,
        "RuntimeFault in init must auto-emit as Error, not Warning",
    );
    assert!(
        !new_diags[0].message.contains("contract violation"),
        "RuntimeFault message must NOT carry contract-violation framing; got: {}",
        new_diags[0].message,
    );
}

/// RuntimeFault in shutdown must auto-emit as Error, not Warning.
/// Phase-symmetry counterpart of `tick_all_emits_error_for_runtime_fault`.
/// Distinguishes from `shutdown_all_auto_emits_diagnostic_on_plugin_shutdown_failure`
/// above (which uses `ShutdownFailed`); this test pins the discrimination
/// against ContractViolation specifically.
#[test]
fn shutdown_all_emits_error_for_runtime_fault() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    host.register(
        PluginId::new("sh-rf"),
        Box::new(TestPlugin::new("sh-rf", log.clone()).with_runtime_fault_in_shutdown()),
    )
    .expect("register");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    let pre_shutdown_diag_count = diags.len();

    let report = {
        let mut ctx = PluginContext::new(&mut diags);
        host.shutdown_all(&mut ctx)
    };

    assert_eq!(report.shutdown.len(), 0);
    assert_eq!(report.failed.len(), 1);

    let new_diags: Vec<_> = diags.iter().skip(pre_shutdown_diag_count).collect();
    assert_eq!(new_diags.len(), 1, "expected exactly one auto-emit");
    assert_eq!(
        new_diags[0].severity,
        Severity::Error,
        "RuntimeFault in shutdown must auto-emit as Error, not Warning",
    );
    assert!(
        !new_diags[0].message.contains("contract violation"),
        "RuntimeFault message must NOT carry contract-violation framing; got: {}",
        new_diags[0].message,
    );
}

/// Per-LOW #5 invariant: an unregister-shutdown that errors emits a
/// Warning (NOT an Error) — host-initiated unregister is non-fatal by
/// design.
#[test]
fn unregister_emits_warning_on_shutdown_failure() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut diags = DiagnosticAggregator::new();
    let mut host = PluginHost::new();

    let id = PluginId::new("u");
    host.register(
        id.clone(),
        Box::new(TestPlugin::new("u", log.clone()).with_shutdown_failure()),
    )
    .expect("register");

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.init_all(&mut ctx);
    }
    let pre_unregister_diag_count = diags.len();

    {
        let mut ctx = PluginContext::new(&mut diags);
        host.unregister(&id, &mut ctx).expect("unregister");
    }

    let new_diags: Vec<_> = diags.iter().skip(pre_unregister_diag_count).collect();
    assert_eq!(
        new_diags.len(),
        1,
        "expected exactly one warning diagnostic from unregister-shutdown failure",
    );
    assert_eq!(
        new_diags[0].severity,
        Severity::Warning,
        "unregister-shutdown failure must auto-emit as Warning, not Error",
    );
    assert!(
        new_diags[0].message.contains("unregister-shutdown failed"),
        "warning should reference unregister-shutdown; got: {}",
        new_diags[0].message,
    );
}
