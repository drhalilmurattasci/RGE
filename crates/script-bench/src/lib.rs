//! `rge-script-bench` - scripting benchmark suite.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: benchmark-harness failures (workload setup error, JSON
//! output write failure, engine-stub initialisation failure) are transient
//! and recoverable in-place: the harness surfaces the error and re-runs or
//! skips the workload. The crate is a measurement scaffold: no PIE
//! participation, no runtime engine ownership, no cross-call state. Matches
//! gfx + ui-fonts (substrate / measurement crates with transient I/O risks).
//!
//! Provides the **harness**, the **native-Rust baseline**, the **script-host
//! formal gate harness**, and the **output format** for the "fastest script
//! engine" pillar verification per [PLAN.md §5.6](../../plans/PLAN.md). The
//! historical [`engine_stub`] remains available as a placeholder row, but
//! Phase 3.3/3.4 now exercises the shipped `rge-script-host` substrate.
//!
//! ## Why this crate exists
//!
//! The "1.5x of native" claim in §5.6 needs an unambiguous denominator. This
//! crate publishes the methodology (see `METHODOLOGY.md`), the workload
//! sources ([`workloads`]), and the native-Rust reference implementation
//! ([`native_baseline`]). All numbers downstream of that - engine cold-start,
//! per-tick throughput, hot-reload swap latency, memory overhead - are defined
//! as ratios over the values produced here.
//!
//! ## Scope
//!
//! - Workload definitions: [`workloads`].
//! - Native-Rust baseline: [`native_baseline`].
//! - Real script-host hot-reload gate: [`script_host`].
//! - Raw wasmtime Cranelift W04 fixture: [`wasmtime_cranelift`].
//! - Raw wasmtime Winch (singlepass) W04 fixture: [`wasmtime_singlepass`].
//! - JSON + Markdown output: [`output`].
//! - Historical placeholder engine row: [`engine_stub`].
//!
//! Comparison vs. Lua/mlua/Wasmer-singlepass/Bevy-extism is out of scope for
//! this dispatch (post-Phase-3 work per `tasks/W20/PLAN.md`).

pub mod engine_stub;
pub mod native_baseline;
pub mod output;
pub mod script_host;
pub mod wasmtime_cranelift;
pub mod wasmtime_singlepass;
pub mod workloads;

pub use output::{BenchReport, BenchResult, Engine, Workload};
pub use workloads::{Transform, Vec3, WorkloadId};
