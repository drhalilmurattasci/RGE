//! Phase 3.4 W04 sub-α — raw wasmtime Cranelift cross-engine bench
//! fixture.
//!
//! Distinct from [`crate::script_host`] which measures wasmtime
//! cranelift **through** the `rge-script-host` Counter orchestration
//! (capability checks, ECS marshaling, hot-reload state machine).
//! This module measures wasmtime cranelift directly — no script-host,
//! no host imports beyond the wasmtime defaults, no capability
//! plumbing. The four BASELINE.md `_pending W04_` rows
//! (`script_tick_1m_iters` / `per_frame_tick_10k_entities` /
//! `cold_start` / `memory_overhead`) flip to measured numbers via this
//! fixture.
//!
//! # Scope (sub-α)
//!
//! - **WasmtimeCranelift column only**. WasmtimeSinglepass (Winch /
//!   Strategy reconfig) is sub-β scope.
//! - **No MLua / Wasmer / BevyExtism deps** — those columns stay
//!   `_post-Phase-3_` for now.
//! - **Direct `wasmtime` API** — no `runtime-wasmtime-engine` /
//!   `runtime-wasmtime` involvement; raw `wasmtime::Engine` configured
//!   with `cranelift_opt_level(OptLevel::Speed)` mirroring
//!   `runtime-wasmtime-engine::Engine::new`'s setup.
//! - **Three WAT fixtures** inlined as `&'static str` constants
//!   (mirrors `rge-script-host`'s `COUNTER_V1_WAT` / `_V2` / `_BULK`
//!   precedent).
//! - **Module::serialize().len()** as the `memory_overhead_bytes`
//!   proxy — defensible measure of "bytes per loaded module" at the
//!   AOT-artifact level. NOT a runtime RSS measurement (out of scope;
//!   would require `/proc/self/status` or `GetProcessMemoryInfo`
//!   instrumentation per platform).

use std::time::{Duration, Instant};

use wasmtime::{Config, Engine, Instance, Module, OptLevel, Store};

// ---------------------------------------------------------------------------
// WAT fixtures (inlined per the script-host precedent)
// ---------------------------------------------------------------------------

/// `script_tick_1m_iters` workload — 1,000,000 iterations of
/// `pos = pos + dt * vel` in a tight WASM loop. Mirrors the native
/// baseline's tight-loop kernel
/// ([`crate::native_baseline::script_tick_1m_iters`]). Returns the
/// final `pos` value so the loop body isn't dead-code-eliminated.
pub(crate) const SCRIPT_TICK_1M_WAT: &str = r#"
(module
  (func (export "tick_1m") (param $dt f32) (param $vel f32) (result f32)
    (local $i i32)
    (local $pos f32)
    (loop $body
      (local.set $pos
        (f32.add (local.get $pos)
                 (f32.mul (local.get $dt) (local.get $vel))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $body (i32.lt_s (local.get $i) (i32.const 1000000)))
    )
    (local.get $pos)
  )
)
"#;

/// `per_frame_tick_10k_entities` workload — single integration frame
/// over 10,000 entities, each carrying a 6-f32 struct
/// `(pos.{x,y,z}, vel.{x,y,z})` (24 bytes per entity). 10,000 × 24 =
/// 240,000 bytes ≈ 3.66 pages; allocate 4 pages = 262,144 bytes.
/// Mirrors the native baseline's
/// `crate::native_baseline::per_frame_tick_10k_entities` kernel shape.
pub(crate) const PER_FRAME_TICK_10K_WAT: &str = r#"
(module
  (memory (export "mem") 4)
  (func (export "tick_10k") (param $dt f32)
    (local $i i32)
    (local $base i32)
    (loop $body
      (local.set $base (i32.mul (local.get $i) (i32.const 24)))
      ;; pos.x += dt * vel.x  (offset 0 from base; vel.x at offset 12)
      (f32.store offset=0 (local.get $base)
        (f32.add (f32.load offset=0 (local.get $base))
                 (f32.mul (local.get $dt) (f32.load offset=12 (local.get $base)))))
      ;; pos.y += dt * vel.y  (offset 4; vel.y at offset 16)
      (f32.store offset=4 (local.get $base)
        (f32.add (f32.load offset=4 (local.get $base))
                 (f32.mul (local.get $dt) (f32.load offset=16 (local.get $base)))))
      ;; pos.z += dt * vel.z  (offset 8; vel.z at offset 20)
      (f32.store offset=8 (local.get $base)
        (f32.add (f32.load offset=8 (local.get $base))
                 (f32.mul (local.get $dt) (f32.load offset=20 (local.get $base)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $body (i32.lt_s (local.get $i) (i32.const 10000)))
    )
  )
)
"#;

/// `cold_start` / `memory_overhead` workload — empty module exporting
/// a no-op function. Used for (a) cold-start measurement
/// (compile + instantiate + first call) and (b) memory-overhead
/// measurement (`Module::serialize().len()` as the AOT-artifact byte
/// count proxy).
pub(crate) const COLD_START_EMPTY_WAT: &str = r#"
(module
  (func (export "noop"))
)
"#;

// ---------------------------------------------------------------------------
// WasmtimeCraneliftBench
// ---------------------------------------------------------------------------

/// Raw wasmtime Cranelift fixture — three pre-compiled modules + a
/// shared `wasmtime::Engine` configured to mirror
/// [`rge_runtime_wasmtime_engine::Engine::new`]'s Cranelift +
/// `OptLevel::Speed` setup.
pub struct WasmtimeCraneliftBench {
    engine: Engine,
    script_tick_1m_module: Module,
    per_frame_tick_10k_module: Module,
    cold_start_empty_module: Module,
}

impl WasmtimeCraneliftBench {
    /// Construct the bench fixture: configure the `wasmtime::Engine`
    /// with Cranelift + `OptLevel::Speed`, then compile the three WAT
    /// fixtures.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if any of:
    /// - `Engine::new` rejects the Config (extremely unlikely on
    ///   default flags)
    /// - `wat::parse_str` fails on any of the inlined WAT fixtures
    ///   (would indicate a typo in the WAT constants — caught at this
    ///   call site, not at module load time)
    /// - `Module::new` fails to compile (would indicate a WASM
    ///   validation failure — caught at this call site)
    pub fn new() -> Result<Self, String> {
        let mut cfg = Config::new();
        cfg.cranelift_opt_level(OptLevel::Speed);
        let engine = Engine::new(&cfg).map_err(|e| format!("Engine::new: {e}"))?;

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
    /// `pos = pos + dt * vel` inside the cached wasm module. Returns
    /// the wall-clock duration of the single `tick_1m(dt, vel)` call
    /// (instance/store construction is INCLUDED in the duration
    /// because that's part of the per-run cost for the workload; the
    /// compile step is amortized via the cached module).
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
    /// integration frame over 10,000 entities. Memory is initialized
    /// to zero (newly-allocated linear memory zero-init per the WASM
    /// spec); first-call duration includes that init.
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

    /// Measure cold-start latency for an empty WASM module — fresh
    /// parse + compile + instantiate + first call. The cached
    /// `cold_start_empty_module` is NOT used (its compile time is
    /// already amortized at `WasmtimeCraneliftBench::new`); this fn
    /// re-parses and re-compiles to measure the cold path honestly.
    /// The engine itself IS shared (one `Engine` per bench instance;
    /// engine construction cost is amortized).
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

    /// Measure `memory_overhead` for a loaded module as the AOT-
    /// serialized byte count of the empty module (proxy for "bytes
    /// per loaded module" — captures compiled-code size + module
    /// metadata; does NOT capture runtime instance state like linear
    /// memory pages or store overhead). The native baseline measures
    /// `size_of::<fn(...)>()` = 8 bytes for the same workload class;
    /// the wasmtime proxy is structurally larger.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if `Module::serialize` fails (extremely
    /// unlikely for a successfully-compiled module).
    pub fn memory_overhead_bytes(&self) -> Result<usize, String> {
        self.cold_start_empty_module
            .serialize()
            .map(|v| v.len())
            .map_err(|e| format!("Module::serialize: {e}"))
    }
}

fn compile_wat_module(engine: &Engine, name: &str, wat_src: &str) -> Result<Module, String> {
    let bytes = wat::parse_str(wat_src).map_err(|e| format!("wat {name}: {e}"))?;
    Module::new(engine, &bytes).map_err(|e| format!("Module::new {name}: {e}"))
}

// ---------------------------------------------------------------------------
// W04 measurement tests — mirror phase_3_3 / phase_3_4 println style
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `script_tick_1m_iters` workload — raw wasmtime Cranelift row.
    /// Prints the per-op nanoseconds (`duration / 1_000_000`) for
    /// direct comparison against the native baseline's
    /// `script_tick_1m_iters` row in BASELINE.md.
    #[test]
    fn w04_script_tick_1m_iters_wasmtime_cranelift() {
        let bench = WasmtimeCraneliftBench::new().expect("compile fixtures");
        let duration = bench.script_tick_1m_iters(1e-6, 1.0).expect("tick_1m");
        #[allow(
            clippy::cast_precision_loss,
            reason = "duration nanos cast to f64 for per-op derivation; precision loss negligible at the bench scale"
        )]
        let per_op_ns = duration.as_nanos() as f64 / 1_000_000.0;
        println!(
            "w04_script_tick_1m_iters_wasmtime_cranelift: iters=1000000 duration_ns={} per_op_ns={:.3}",
            duration.as_nanos(),
            per_op_ns
        );
        assert!(duration > Duration::ZERO);
        assert!(
            duration < Duration::from_secs(10),
            "1M wasm iterations took > 10s — investigate (got {duration:?})"
        );
    }

    /// `per_frame_tick_10k_entities` workload — raw wasmtime Cranelift
    /// row. Prints per-entity ns derivation
    /// (`duration / 10_000`) for comparison against the native baseline.
    #[test]
    fn w04_per_frame_tick_10k_entities_wasmtime_cranelift() {
        let bench = WasmtimeCraneliftBench::new().expect("compile fixtures");
        let duration = bench.per_frame_tick_10k_entities(1e-6).expect("tick_10k");
        #[allow(
            clippy::cast_precision_loss,
            reason = "duration nanos cast to f64 for per-entity derivation; precision loss negligible at the bench scale"
        )]
        let per_entity_ns = duration.as_nanos() as f64 / 10_000.0;
        println!(
            "w04_per_frame_tick_10k_entities_wasmtime_cranelift: entities=10000 duration_ns={} per_entity_ns={:.3}",
            duration.as_nanos(),
            per_entity_ns
        );
        assert!(duration > Duration::ZERO);
        assert!(
            duration < Duration::from_secs(5),
            "10k entity frame took > 5s — investigate (got {duration:?})"
        );
    }

    /// `cold_start` workload — raw wasmtime Cranelift row. Prints the
    /// total cold-start microseconds (parse + compile + instantiate +
    /// first no-op call). Native baseline's `cold_start` is the
    /// closure-call timer floor (~48 ns); the wasmtime cranelift row
    /// is structurally larger because it includes JIT compile.
    #[test]
    fn w04_cold_start_wasmtime_cranelift() {
        let bench = WasmtimeCraneliftBench::new().expect("compile fixtures");
        let duration = bench.cold_start_once().expect("cold_start");
        #[allow(
            clippy::cast_precision_loss,
            reason = "duration nanos cast to f64 for ms derivation; precision loss negligible at the bench scale"
        )]
        let duration_ms = duration.as_nanos() as f64 / 1_000_000.0;
        println!(
            "w04_cold_start_wasmtime_cranelift: duration_ns={} duration_ms={:.3}",
            duration.as_nanos(),
            duration_ms
        );
        assert!(duration > Duration::ZERO);
        assert!(
            duration < Duration::from_secs(2),
            "cold-start took > 2s — investigate (got {duration:?})"
        );
    }

    /// `memory_overhead` workload — raw wasmtime Cranelift row. Uses
    /// `Module::serialize().len()` as the AOT-artifact byte count
    /// proxy for "bytes per loaded module". Native baseline is 8
    /// bytes (function pointer); the wasmtime proxy is structurally
    /// larger because it captures compiled-code size + module
    /// metadata.
    #[test]
    fn w04_memory_overhead_wasmtime_cranelift() {
        let bench = WasmtimeCraneliftBench::new().expect("compile fixtures");
        let bytes = bench.memory_overhead_bytes().expect("memory_overhead");
        println!("w04_memory_overhead_wasmtime_cranelift: bytes_per_module={bytes}");
        assert!(bytes > 0);
        assert!(
            bytes < 16 * 1024 * 1024,
            "module serialize > 16 MiB for empty module — investigate (got {bytes})"
        );
    }
}
