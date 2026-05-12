//! Phase 3.4 W04 sub-β — raw wasmtime Winch (singlepass) cross-engine
//! bench fixture.
//!
//! Mirror of [`crate::wasmtime_cranelift`] with one config-strategy
//! swap: `wasmtime::Strategy::Winch` instead of
//! `cranelift_opt_level(OptLevel::Speed)`. **Same four WAT fixtures**
//! (re-used as `pub(crate)` from `wasmtime_cranelift` to avoid
//! duplication); **same four workloads**
//! (`script_tick_1m_iters` / `per_frame_tick_10k_entities` /
//! `cold_start` / `memory_overhead`); **same four measurement tests**
//! with the phase_3_3 / phase_3_4 sibling-site `println!` format,
//! re-named to `wasmtime_singlepass` for table-row clarity.
//!
//! # Scope (sub-β)
//!
//! - **WasmtimeSinglepass column only** (the W04 column adjacent to
//!   the sub-α-closed Cranelift column).
//! - **No MLua / Wasmer / BevyExtism deps** — those columns stay
//!   `_post-Phase-3_`.
//! - **`winch` feature flag added script-bench-locally** (in
//!   `crates/script-bench/Cargo.toml`); runtime crates
//!   (`rge-runtime-wasmtime` / `rge-runtime-wasmtime-engine`) stay on
//!   their default-Cranelift-only path. Cargo feature unification
//!   means wasmtime is compiled with the union
//!   `["cranelift", "runtime", "std", "winch"]`; the per-crate
//!   override expresses intent without committing the workspace.
//! - **Direct `wasmtime` API** — no `runtime-wasmtime-engine` /
//!   `runtime-wasmtime` involvement; raw `wasmtime::Engine` configured
//!   via `Config::strategy(Strategy::Winch)`.
//! - **`Module::serialize().len()`** as the `memory_overhead_bytes`
//!   proxy, mirroring sub-α exactly. (Winch's serialized artifact may
//!   differ in size from Cranelift's — recorded honestly.)
//!
//! # Central question (per user direction)
//!
//! "Is the raw per-entity overhead a Cranelift-specific shape or a
//! broader direct-WASM execution cost?" The 10× ratio on
//! `per_frame_tick_10k_entities` measured in sub-α (Cranelift) is the
//! signal under examination. Three possible Winch outcomes:
//!
//! - **Similar ~10× ratio**: per-entity overhead is broadly
//!   characteristic of direct-WASM execution (bounds checks +
//!   per-element instruction overhead) — NOT Cranelift-specific.
//! - **Much worse ratio** (e.g., 50×): Winch's simpler codegen
//!   amplifies the per-entity penalty; Cranelift IS the better choice
//!   for per-entity workloads.
//! - **Better ratio than Cranelift**: surprising; would warrant
//!   follow-up investigation.
//!
//! Whatever Winch produces is recorded in
//! `crates/script-bench/BASELINE.md` honestly, including the "ugly"
//! cases.

use std::time::{Duration, Instant};

use wasmtime::{Config, Engine, Instance, Module, Store, Strategy};

use crate::wasmtime_cranelift::{COLD_START_EMPTY_WAT, PER_FRAME_TICK_10K_WAT, SCRIPT_TICK_1M_WAT};

// ---------------------------------------------------------------------------
// WasmtimeSinglepassBench
// ---------------------------------------------------------------------------

/// Raw wasmtime Winch (singlepass) fixture — three pre-compiled
/// modules + a shared `wasmtime::Engine` configured via
/// `Config::strategy(Strategy::Winch)`. Mirrors
/// [`crate::wasmtime_cranelift::WasmtimeCraneliftBench`] shape
/// exactly; only the engine config differs.
pub struct WasmtimeSinglepassBench {
    engine: Engine,
    script_tick_1m_module: Module,
    per_frame_tick_10k_module: Module,
    cold_start_empty_module: Module,
}

impl WasmtimeSinglepassBench {
    /// Construct the bench fixture: configure the `wasmtime::Engine`
    /// with `Strategy::Winch`, then compile the three WAT fixtures
    /// re-used from the sub-α Cranelift module.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if any of:
    /// - `Engine::new` rejects the Config (e.g., Winch unsupported on
    ///   the current platform / wasmtime build — should be available
    ///   on x86_64 Windows/Linux/macOS with the `winch` feature flag
    ///   enabled at compile time per `script-bench`'s `Cargo.toml`)
    /// - `wat::parse_str` fails on any of the shared WAT fixtures
    ///   (unreachable — sub-α tests already exercise these and pass)
    /// - `Module::new` fails to compile under Winch (would indicate
    ///   the WAT uses a WASM feature Winch doesn't support — sub-α
    ///   fixtures use only basic features and SHOULD be Winch-safe,
    ///   but this error surface is preserved for honest reporting)
    pub fn new() -> Result<Self, String> {
        let mut cfg = Config::new();
        cfg.strategy(Strategy::Winch);
        let engine = Engine::new(&cfg).map_err(|e| format!("Engine::new (Winch): {e}"))?;

        let script_tick_1m_module =
            compile_wat_module(&engine, "script_tick_1m", SCRIPT_TICK_1M_WAT)?;
        let per_frame_tick_10k_module =
            compile_wat_module(&engine, "per_frame_tick_10k", PER_FRAME_TICK_10K_WAT)?;
        let cold_start_empty_module =
            compile_wat_module(&engine, "cold_start_empty", COLD_START_EMPTY_WAT)?;

        Ok(Self {
            engine,
            script_tick_1m_module,
            per_frame_tick_10k_module,
            cold_start_empty_module,
        })
    }

    /// Run the `script_tick_1m_iters` workload — 1M iterations of
    /// `pos = pos + dt * vel` inside the cached Winch-compiled module.
    /// Mirrors [`crate::wasmtime_cranelift::WasmtimeCraneliftBench::script_tick_1m_iters`]
    /// shape exactly.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on `Instance::new` / `get_typed_func` /
    /// `call` failures.
    pub fn script_tick_1m_iters(&self, dt: f32, vel: f32) -> Result<Duration, String> {
        let mut store = Store::new(&self.engine, ());
        let instance = Instance::new(&mut store, &self.script_tick_1m_module, &[])
            .map_err(|e| format!("Instance::new: {e}"))?;
        let func = instance
            .get_typed_func::<(f32, f32), f32>(&mut store, "tick_1m")
            .map_err(|e| format!("get_typed_func: {e}"))?;
        let start = Instant::now();
        let _final_pos = func
            .call(&mut store, (dt, vel))
            .map_err(|e| format!("call: {e}"))?;
        Ok(start.elapsed())
    }

    /// Run the `per_frame_tick_10k_entities` workload — single
    /// integration frame over 10,000 entities. Mirrors
    /// [`crate::wasmtime_cranelift::WasmtimeCraneliftBench::per_frame_tick_10k_entities`]
    /// shape exactly.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on `Instance::new` / `get_typed_func` /
    /// `call` failures.
    pub fn per_frame_tick_10k_entities(&self, dt: f32) -> Result<Duration, String> {
        let mut store = Store::new(&self.engine, ());
        let instance = Instance::new(&mut store, &self.per_frame_tick_10k_module, &[])
            .map_err(|e| format!("Instance::new: {e}"))?;
        let func = instance
            .get_typed_func::<f32, ()>(&mut store, "tick_10k")
            .map_err(|e| format!("get_typed_func: {e}"))?;
        let start = Instant::now();
        func.call(&mut store, dt)
            .map_err(|e| format!("call: {e}"))?;
        Ok(start.elapsed())
    }

    /// Measure cold-start latency for an empty WASM module under
    /// Winch — fresh parse + compile + instantiate + first call. The
    /// cached `cold_start_empty_module` is NOT used (its compile is
    /// amortized at `WasmtimeSinglepassBench::new`); this fn
    /// re-parses and re-compiles to measure the cold path honestly.
    /// Winch's compile path is expected to be MUCH faster than
    /// Cranelift's (that's Winch's design point — fast compile,
    /// slower runtime); the actual ratio is recorded in BASELINE.md.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on parse / compile / instantiate / call
    /// failures.
    pub fn cold_start_once(&self) -> Result<Duration, String> {
        let start = Instant::now();
        let wat_bytes =
            wat::parse_str(COLD_START_EMPTY_WAT).map_err(|e| format!("wat::parse_str: {e}"))?;
        let module =
            Module::new(&self.engine, &wat_bytes).map_err(|e| format!("Module::new: {e}"))?;
        let mut store = Store::new(&self.engine, ());
        let instance =
            Instance::new(&mut store, &module, &[]).map_err(|e| format!("Instance::new: {e}"))?;
        let func = instance
            .get_typed_func::<(), ()>(&mut store, "noop")
            .map_err(|e| format!("get_typed_func: {e}"))?;
        func.call(&mut store, ())
            .map_err(|e| format!("call: {e}"))?;
        Ok(start.elapsed())
    }

    /// Measure `memory_overhead` for a Winch-loaded module as the
    /// AOT-serialized byte count of the empty module (same proxy as
    /// sub-α — see
    /// [`crate::wasmtime_cranelift::WasmtimeCraneliftBench::memory_overhead_bytes`]
    /// for the rationale and limitations).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if `Module::serialize` fails.
    pub fn memory_overhead_bytes(&self) -> Result<usize, String> {
        self.cold_start_empty_module
            .serialize()
            .map(|v| v.len())
            .map_err(|e| format!("Module::serialize: {e}"))
    }
}

fn compile_wat_module(engine: &Engine, name: &str, wat_src: &str) -> Result<Module, String> {
    let bytes = wat::parse_str(wat_src).map_err(|e| format!("wat {name}: {e}"))?;
    Module::new(engine, &bytes).map_err(|e| format!("Module::new {name} (Winch): {e}"))
}

// ---------------------------------------------------------------------------
// W04 sub-β measurement tests — mirror sub-α's println style
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `script_tick_1m_iters` workload — raw wasmtime Winch row.
    /// Direct ratio comparison against the sub-α Cranelift number
    /// (`per_op_ns`) is the central question's first data point.
    #[test]
    fn w04_script_tick_1m_iters_wasmtime_singlepass() {
        let bench = WasmtimeSinglepassBench::new().expect("compile fixtures");
        let duration = bench.script_tick_1m_iters(1e-6, 1.0).expect("tick_1m");
        #[allow(
            clippy::cast_precision_loss,
            reason = "duration nanos cast to f64 for per-op derivation; precision loss negligible at the bench scale"
        )]
        let per_op_ns = duration.as_nanos() as f64 / 1_000_000.0;
        println!(
            "w04_script_tick_1m_iters_wasmtime_singlepass: iters=1000000 duration_ns={} per_op_ns={:.3}",
            duration.as_nanos(),
            per_op_ns
        );
        assert!(duration > Duration::ZERO);
        // Winch's runtime is slower than Cranelift's by design; allow
        // a wider upper bound. The sub-α Cranelift run completed in
        // ~0.713 ns/op for this workload; Winch may be 5-20× slower.
        // A 20-second upper bound is still 200× headroom over the
        // expected ~100ms case.
        assert!(
            duration < Duration::from_secs(20),
            "1M wasm iterations took > 20s under Winch — investigate (got {duration:?})"
        );
    }

    /// `per_frame_tick_10k_entities` workload — raw wasmtime Winch
    /// row. **This is the central question's load-bearing data
    /// point**: the sub-α Cranelift measurement showed 10× over
    /// native for raw per-entity execution. Winch's number tells us
    /// whether that 10× is Cranelift-specific or a broader
    /// direct-WASM characteristic.
    #[test]
    fn w04_per_frame_tick_10k_entities_wasmtime_singlepass() {
        let bench = WasmtimeSinglepassBench::new().expect("compile fixtures");
        let duration = bench.per_frame_tick_10k_entities(1e-6).expect("tick_10k");
        #[allow(
            clippy::cast_precision_loss,
            reason = "duration nanos cast to f64 for per-entity derivation; precision loss negligible at the bench scale"
        )]
        let per_entity_ns = duration.as_nanos() as f64 / 10_000.0;
        println!(
            "w04_per_frame_tick_10k_entities_wasmtime_singlepass: entities=10000 duration_ns={} per_entity_ns={:.3}",
            duration.as_nanos(),
            per_entity_ns
        );
        assert!(duration > Duration::ZERO);
        // Wide upper bound: Winch's per-entity execution may be
        // significantly slower than Cranelift's 7.620 ns/entity
        // (sub-α). 10 seconds = ~1000× the Cranelift case; still
        // generous headroom for honest reporting.
        assert!(
            duration < Duration::from_secs(10),
            "10k entity frame took > 10s under Winch — investigate (got {duration:?})"
        );
    }

    /// `cold_start` workload — raw wasmtime Winch row. Winch's
    /// design point is fast compile + slower runtime; expect this
    /// number to be MUCH lower than the sub-α Cranelift cold-start
    /// of 0.405 ms (Cranelift is an optimizing JIT; Winch is a
    /// single-pass non-optimizing JIT).
    #[test]
    fn w04_cold_start_wasmtime_singlepass() {
        let bench = WasmtimeSinglepassBench::new().expect("compile fixtures");
        let duration = bench.cold_start_once().expect("cold_start");
        #[allow(
            clippy::cast_precision_loss,
            reason = "duration nanos cast to f64 for ms derivation; precision loss negligible at the bench scale"
        )]
        let duration_ms = duration.as_nanos() as f64 / 1_000_000.0;
        println!(
            "w04_cold_start_wasmtime_singlepass: duration_ns={} duration_ms={:.3}",
            duration.as_nanos(),
            duration_ms
        );
        assert!(duration > Duration::ZERO);
        assert!(
            duration < Duration::from_secs(2),
            "cold-start took > 2s under Winch — investigate (got {duration:?})"
        );
    }

    /// `memory_overhead` workload — raw wasmtime Winch row.
    /// `Module::serialize().len()` proxy mirrors sub-α exactly;
    /// Winch's compiled-artifact size may differ from Cranelift's
    /// (single-pass code is typically less compact than optimized
    /// code) — recorded honestly.
    #[test]
    fn w04_memory_overhead_wasmtime_singlepass() {
        let bench = WasmtimeSinglepassBench::new().expect("compile fixtures");
        let bytes = bench.memory_overhead_bytes().expect("memory_overhead");
        println!("w04_memory_overhead_wasmtime_singlepass: bytes_per_module={bytes}");
        assert!(bytes > 0);
        assert!(
            bytes < 16 * 1024 * 1024,
            "module serialize > 16 MiB for empty Winch module — investigate (got {bytes})"
        );
    }
}
