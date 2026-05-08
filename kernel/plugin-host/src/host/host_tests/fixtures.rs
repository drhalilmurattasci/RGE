//! Shared test fixtures for the [`crate::host::PluginHost`] test suite.
//!
//! Sub-module of [`crate::host::host_tests`]; siblings (`registration`,
//! `lifecycle`, `diagnostics`, `panic_recovery`, `resource_leak`) import these
//! via `use super::fixtures::*;`.
//!
//! Only [`TestPlugin`] / [`LyingPlugin`] (and their inherent helpers) are
//! `pub(super)`-visible; everything else is private to this file.

use std::sync::{Arc, Mutex};

use crate::context::PluginContext;
use crate::plugin::{Plugin, PluginError, PluginId};

/// Test helper: a plugin that records its lifecycle events into a shared
/// log so tests can assert ordering.
///
/// Allow `clippy::struct_excessive_bools` because this is a test fixture
/// driving an N-way behavior matrix (8 independent failure / panic /
/// resource-misuse modes); each flag reflects an orthogonal test
/// dimension that doesn't compose with the others, so a state-machine
/// rewrite would obscure the per-test setup. A "real" plugin never
/// looks anything like this.
#[allow(clippy::struct_excessive_bools)]
pub(super) struct TestPlugin {
    id: PluginId,
    log: Arc<Mutex<Vec<String>>>,
    fail_init: bool,
    fail_tick: bool,
    fail_shutdown: bool,
    panic_init: bool,
    panic_tick: bool,
    panic_shutdown: bool,
    /// On tick: take a `u32` from ctx but never put it back. Simulates a
    /// plugin that fails to honor the resource-handoff invariant. Returns
    /// `Ok(())` so the leak detection path is exercised independently of
    /// any `Err` return.
    leak_u32_in_tick: bool,
    /// On init: take a `u32` from ctx but never put it back. Used to
    /// drive the init-phase leak-detection path (audit-2 closure: tick
    /// has `tick_all_detects_resource_leak` but init lacked a
    /// counterpart until this dispatch).
    leak_u32_in_init: bool,
    /// On shutdown: take a `u32` from ctx but never put it back. Used to
    /// drive the shutdown-phase leak-detection path (audit-2 closure
    /// alongside `leak_u32_in_init`).
    leak_u32_in_shutdown: bool,
    /// On tick: return [`PluginError::ContractViolation`] for a missing
    /// resource. Used to verify warning-vs-error severity discrimination.
    emit_contract_violation_in_tick: bool,
    /// On init: return [`PluginError::ContractViolation`]. Used to pin the
    /// init-phase cell of the PluginError × PluginPhase auto-emit matrix
    /// (closes the 4-cell coverage gap noted in HANDOFF.md backlog).
    emit_contract_violation_in_init: bool,
    /// On shutdown: return [`PluginError::ContractViolation`]. Used to pin
    /// the shutdown-phase cell of the PluginError × PluginPhase auto-emit
    /// matrix.
    emit_contract_violation_in_shutdown: bool,
    /// On init: return [`PluginError::RuntimeFault`] (NOT
    /// [`PluginError::InitFailed`]). Used to pin the init-phase cell of
    /// RuntimeFault auto-emit; verifies the host's by-variant severity
    /// dispatch is phase-agnostic (RuntimeFault = Error in any phase).
    emit_runtime_fault_in_init: bool,
    /// On shutdown: return [`PluginError::RuntimeFault`] (NOT
    /// [`PluginError::ShutdownFailed`]). Used to pin the shutdown-phase
    /// cell of RuntimeFault auto-emit.
    emit_runtime_fault_in_shutdown: bool,
    /// On tick: take a `u32` from ctx, then panic MID-PUT-BACK (without
    /// completing the put-back). Exercises the audit-6 round-6 M4 path:
    /// proves catch_unwind catches a panic that occurs DURING the plugin's
    /// own ctx.insert call, AND that resource-leak detection correctly
    /// flags the unrecoverable resource (the value was on the panicking
    /// stack frame and is gone). The panic site is BEFORE ctx.insert
    /// completes, so the registry post-snapshot is missing the u32 slot.
    panic_after_resource_take_in_tick: bool,
}

impl TestPlugin {
    pub(super) fn new(id: &str, log: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id: PluginId::new(id),
            log,
            fail_init: false,
            fail_tick: false,
            fail_shutdown: false,
            panic_init: false,
            panic_tick: false,
            panic_shutdown: false,
            leak_u32_in_tick: false,
            leak_u32_in_init: false,
            leak_u32_in_shutdown: false,
            emit_contract_violation_in_tick: false,
            emit_contract_violation_in_init: false,
            emit_contract_violation_in_shutdown: false,
            emit_runtime_fault_in_init: false,
            emit_runtime_fault_in_shutdown: false,
            panic_after_resource_take_in_tick: false,
        }
    }

    pub(super) fn with_init_failure(mut self) -> Self {
        self.fail_init = true;
        self
    }

    pub(super) fn with_tick_failure(mut self) -> Self {
        self.fail_tick = true;
        self
    }

    pub(super) fn with_shutdown_failure(mut self) -> Self {
        self.fail_shutdown = true;
        self
    }

    pub(super) fn with_init_panic(mut self) -> Self {
        self.panic_init = true;
        self
    }

    pub(super) fn with_tick_panic(mut self) -> Self {
        self.panic_tick = true;
        self
    }

    pub(super) fn with_shutdown_panic(mut self) -> Self {
        self.panic_shutdown = true;
        self
    }

    /// Plugin variant that takes a `u32` from ctx in `tick` and never
    /// puts it back. Used to drive the leak-detection path.
    pub(super) fn with_resource_take_no_putback(mut self) -> Self {
        self.leak_u32_in_tick = true;
        self
    }

    /// Plugin variant that takes a `u32` from ctx in `init` and never
    /// puts it back. Drives the init-phase leak-detection path.
    pub(super) fn with_init_resource_take_no_putback(mut self) -> Self {
        self.leak_u32_in_init = true;
        self
    }

    /// Plugin variant that takes a `u32` from ctx in `shutdown` and
    /// never puts it back. Drives the shutdown-phase leak-detection
    /// path.
    pub(super) fn with_shutdown_resource_take_no_putback(mut self) -> Self {
        self.leak_u32_in_shutdown = true;
        self
    }

    /// Plugin variant whose `tick` returns
    /// [`PluginError::ContractViolation`] (not `RuntimeFault`). Used to
    /// verify warning-vs-error severity discrimination.
    pub(super) fn with_contract_violation_in_tick(mut self) -> Self {
        self.emit_contract_violation_in_tick = true;
        self
    }

    /// Plugin variant whose `init` returns
    /// [`PluginError::ContractViolation`]. Drives the init-phase cell of
    /// the PluginError × PluginPhase auto-emit matrix.
    pub(super) fn with_contract_violation_in_init(mut self) -> Self {
        self.emit_contract_violation_in_init = true;
        self
    }

    /// Plugin variant whose `shutdown` returns
    /// [`PluginError::ContractViolation`]. Drives the shutdown-phase cell
    /// of the PluginError × PluginPhase auto-emit matrix.
    pub(super) fn with_contract_violation_in_shutdown(mut self) -> Self {
        self.emit_contract_violation_in_shutdown = true;
        self
    }

    /// Plugin variant whose `init` returns [`PluginError::RuntimeFault`]
    /// (NOT [`PluginError::InitFailed`]). Drives the init-phase cell of
    /// RuntimeFault auto-emit, verifying the host's by-variant severity
    /// dispatch is phase-agnostic.
    pub(super) fn with_runtime_fault_in_init(mut self) -> Self {
        self.emit_runtime_fault_in_init = true;
        self
    }

    /// Plugin variant whose `shutdown` returns
    /// [`PluginError::RuntimeFault`] (NOT [`PluginError::ShutdownFailed`]).
    /// Drives the shutdown-phase cell of RuntimeFault auto-emit.
    pub(super) fn with_runtime_fault_in_shutdown(mut self) -> Self {
        self.emit_runtime_fault_in_shutdown = true;
        self
    }

    /// Plugin variant whose `tick` takes a `u32` from ctx, then panics
    /// before completing the put-back. Drives the audit-6 round-6 M4
    /// path: validates that catch_unwind catches a panic that occurs
    /// AFTER ctx.take but BEFORE ctx.insert, and that the resource-leak
    /// detection still fires (the resource was on the panicking stack
    /// frame and is unrecoverable).
    pub(super) fn with_panic_after_resource_take_in_tick(mut self) -> Self {
        self.panic_after_resource_take_in_tick = true;
        self
    }
}

// Allow `clippy::manual_assert` for the panic! calls below: these are
// INTENTIONAL panics meant to drive the host's catch_unwind recovery
// path. `assert!(!flag, "msg")` would have identical runtime behaviour
// but reads as a precondition check rather than a deliberate panic
// injection, which obscures the test intent.
#[allow(clippy::manual_assert)]
impl Plugin for TestPlugin {
    fn id(&self) -> PluginId {
        self.id.clone()
    }

    fn init(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        self.log.lock().unwrap().push(format!("init:{}", self.id));
        if self.panic_init {
            panic!("test plugin {} init panic", self.id);
        }
        if self.leak_u32_in_init {
            // Take but don't put back — the init-phase leak path.
            let _ = ctx.take::<u32>();
            return Ok(());
        }
        if self.emit_contract_violation_in_init {
            return Err(PluginError::contract_violation("World"));
        }
        if self.emit_runtime_fault_in_init {
            return Err(PluginError::runtime_fault(format!(
                "{} runtime fault in init",
                self.id
            )));
        }
        if self.fail_init {
            Err(PluginError::init(format!("{} failed init", self.id)))
        } else {
            Ok(())
        }
    }

    fn tick(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        self.log.lock().unwrap().push(format!("tick:{}", self.id));
        if self.panic_tick {
            panic!("test plugin {} tick panic", self.id);
        }
        if self.panic_after_resource_take_in_tick {
            // Take a u32, then panic BEFORE completing the put-back. The
            // value lives on this stack frame; the panic unwinds and the
            // value is dropped. catch_unwind catches the panic; the
            // post-call registry snapshot will show the u32 slot missing,
            // and resource-leak detection fires for it. This is the
            // audit-6 round-6 M4 path.
            let _value = ctx.take::<u32>();
            panic!("test plugin {} panic after resource take in tick", self.id);
        }
        if self.leak_u32_in_tick {
            // Take but don't put back — the leak path.
            let _ = ctx.take::<u32>();
            return Ok(());
        }
        if self.emit_contract_violation_in_tick {
            return Err(PluginError::contract_violation("World"));
        }
        if self.fail_tick {
            Err(PluginError::runtime_fault(format!(
                "{} failed tick",
                self.id
            )))
        } else {
            Ok(())
        }
    }

    fn shutdown(&mut self, ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        self.log
            .lock()
            .unwrap()
            .push(format!("shutdown:{}", self.id));
        if self.panic_shutdown {
            panic!("test plugin {} shutdown panic", self.id);
        }
        if self.leak_u32_in_shutdown {
            // Take but don't put back — the shutdown-phase leak path.
            let _ = ctx.take::<u32>();
            return Ok(());
        }
        if self.emit_contract_violation_in_shutdown {
            return Err(PluginError::contract_violation("World"));
        }
        if self.emit_runtime_fault_in_shutdown {
            return Err(PluginError::runtime_fault(format!(
                "{} runtime fault in shutdown",
                self.id
            )));
        }
        if self.fail_shutdown {
            Err(PluginError::shutdown(format!(
                "{} failed shutdown",
                self.id
            )))
        } else {
            Ok(())
        }
    }
}

/// Plugin whose `id()` returns a different value than registration —
/// for `IdMismatch` test.
pub(super) struct LyingPlugin {
    pub(super) actual_id: PluginId,
}

impl Plugin for LyingPlugin {
    fn id(&self) -> PluginId {
        self.actual_id.clone()
    }
    fn init(&mut self, _ctx: &mut PluginContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }
}
