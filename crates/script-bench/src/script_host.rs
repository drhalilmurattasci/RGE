//! Real `script-host` benchmark harness for the formal Phase 3 gates.
//!
//! This module keeps the scope deliberately narrow: it measures the shipped
//! `rge-script-host` Counter hot-reload substrate against the formal
//! 1000-entity / 100-cycle preservation gate. It does not add a new WASM ABI,
//! a generic component bridge, reflection migration, or plugin-host integration.

use std::time::{Duration, Instant};

use rge_kernel_diagnostics::DiagnosticAggregator;
use rge_kernel_ecs::{EntityId, World};
use rge_kernel_events::EventBus;
use rge_script_host::ecs_bridge::{entity_id_to_i64, Counter};
use rge_script_host::{capture_state, restore_state, ScriptInstance, ScriptModule};
use wasmtime::Engine;

use crate::workloads::{FIXED_DT, HOT_RELOAD_CYCLES};

const COUNTER_V1_WAT: &str = include_str!("../../script-host/tests/fixtures/counter_v1.wat");
const COUNTER_V2_WAT: &str = include_str!("../../script-host/tests/fixtures/counter_v2.wat");

/// Formal Phase 3.3 / 3.4 hot-reload scene size.
pub const FORMAL_HOT_RELOAD_ENTITY_COUNT: usize = 1_000;

/// Formal Phase 3.4 memory-soak wall-clock duration.
pub const FORMAL_MEMORY_SOAK_DURATION: Duration = Duration::from_secs(60 * 60);

/// Hot-reload preservation workload configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotReloadConfig {
    /// Number of Counter-bearing entities in the scene.
    pub entity_count: usize,
    /// Number of consecutive module-swap cycles to run.
    pub cycles: u32,
}

impl HotReloadConfig {
    /// Formal 1000-entity / 100-cycle Phase 3 gate.
    #[must_use]
    pub const fn formal() -> Self {
        Self {
            entity_count: FORMAL_HOT_RELOAD_ENTITY_COUNT,
            cycles: HOT_RELOAD_CYCLES,
        }
    }
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self::formal()
    }
}

/// Memory-soak workload configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySoakConfig {
    /// Number of Counter-bearing entities in the scene.
    pub entity_count: usize,
    /// Minimum wall-clock time to keep swapping modules.
    pub minimum_duration: Duration,
}

impl MemorySoakConfig {
    /// Formal 1-hour Phase 3 memory-soak gate.
    #[must_use]
    pub const fn formal() -> Self {
        Self {
            entity_count: FORMAL_HOT_RELOAD_ENTITY_COUNT,
            minimum_duration: FORMAL_MEMORY_SOAK_DURATION,
        }
    }
}

impl Default for MemorySoakConfig {
    fn default() -> Self {
        Self::formal()
    }
}

/// Summary of a hot-reload preservation run.
#[derive(Debug, Clone, PartialEq)]
pub struct HotReloadReport {
    /// Number of Counter-bearing entities in the scene.
    pub entity_count: usize,
    /// Number of consecutive module-swap cycles completed.
    pub cycles: u32,
    /// Total number of Counter components restored across all cycles.
    pub restored_components: usize,
    /// Total wall-clock time across all measured swap windows.
    pub total_duration: Duration,
    /// Per-cycle p95 swap-window latency.
    pub p95_duration: Duration,
    /// Slowest single swap window.
    pub max_duration: Duration,
    /// Final sum across all Counter components after the last restore.
    pub final_counter_sum: i128,
}

impl HotReloadReport {
    /// Average swap-window duration.
    #[must_use]
    pub fn average_duration(&self) -> Duration {
        if self.cycles == 0 {
            return Duration::ZERO;
        }
        duration_div_u32(self.total_duration, self.cycles)
    }

    /// Average swap-window duration in milliseconds.
    #[must_use]
    pub fn average_ms(&self) -> f64 {
        duration_ms(self.average_duration())
    }

    /// p95 swap-window duration in milliseconds.
    #[must_use]
    pub fn p95_ms(&self) -> f64 {
        duration_ms(self.p95_duration)
    }
}

/// Phase 3.4 ECS-via-WASM ratio measurement configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcsIterationConfig {
    /// Number of Counter-bearing entities in the scene.
    pub entity_count: usize,
    /// Number of frames to drive over the scene per measured run.
    pub frames: u32,
}

impl EcsIterationConfig {
    /// Formal Phase 3.4 measurement scene size: 1000 entities × 10 frames.
    ///
    /// 10k host↔WASM transitions per measured run is enough signal to
    /// distinguish per-call overhead from setup noise without the test
    /// running for more than a second on commodity hardware.
    #[must_use]
    pub const fn formal() -> Self {
        Self {
            entity_count: FORMAL_HOT_RELOAD_ENTITY_COUNT,
            frames: 10,
        }
    }
}

impl Default for EcsIterationConfig {
    fn default() -> Self {
        Self::formal()
    }
}

/// Summary of a Phase 3.4 ECS-via-WASM ratio measurement.
///
/// Both `native_*` and `wasm_*` paths perform identical algorithmic work
/// per increment: a query-scan to find the entity by handle, then a
/// `World::insert` to write the new value. The native path drives this
/// directly via `rge_kernel_ecs::World`; the wasm path drives it via
/// `ScriptInstance::tick`, which transitions into wasm and back through
/// the `rge.ecs::{get_counter, set_counter}` host bridge once per
/// entity per frame. The recorded `ratio` therefore captures host-bridge
/// transition overhead almost in isolation (algorithmic work cancels).
#[derive(Debug, Clone, PartialEq)]
pub struct EcsIterationReport {
    /// Number of Counter-bearing entities iterated per frame.
    pub entity_count: usize,
    /// Number of frames driven during the measurement.
    pub frames: u32,
    /// Total wall-clock time spent in the native baseline.
    pub native_total: Duration,
    /// Total wall-clock time spent in the wasm-via-host path.
    pub wasm_total: Duration,
    /// Native time per frame (averaged across `frames`).
    pub native_per_frame_avg: Duration,
    /// Wasm time per frame (averaged across `frames`).
    pub wasm_per_frame_avg: Duration,
    /// `wasm_total / native_total`. The Phase 3.4 gate target is ≤ 1.5;
    /// substrate-pending until bulk-iteration host functions land
    /// (current `rge.ecs::get_counter` / `set_counter` cross the wasm
    /// boundary once per entity per frame).
    pub ratio: f64,
}

impl EcsIterationReport {
    /// Render the ratio as a 2-decimal multiple (e.g. `"11.43×"`).
    #[must_use]
    pub fn ratio_pretty(&self) -> String {
        format!("{:.2}×", self.ratio)
    }
}

/// Summary of a memory-soak run.
#[derive(Debug, Clone, PartialEq)]
pub struct MemorySoakReport {
    /// Number of Counter-bearing entities in the scene.
    pub entity_count: usize,
    /// Number of hot-reload cycles completed before the duration elapsed.
    pub cycles: u32,
    /// Total wall-clock runtime.
    pub elapsed: Duration,
    /// Total Counter components restored across all cycles.
    pub restored_components: usize,
    /// Final sum across all Counter components.
    pub final_counter_sum: i128,
}

/// Compiled script-host modules and shared wasmtime engine for benchmark runs.
pub struct ScriptHostBench {
    engine: Engine,
    module_v1: ScriptModule,
    module_v2: ScriptModule,
}

impl ScriptHostBench {
    /// Compile the canonical Counter v1/v2 fixtures once for repeated runs.
    ///
    /// # Errors
    ///
    /// Returns a string error when WAT parsing or wasmtime compilation fails.
    pub fn new() -> Result<Self, String> {
        let engine = Engine::default();
        let module_v1 = compile_counter_module(&engine, "counter_v1", COUNTER_V1_WAT)?;
        let module_v2 = compile_counter_module(&engine, "counter_v2", COUNTER_V2_WAT)?;
        Ok(Self {
            engine,
            module_v1,
            module_v2,
        })
    }

    /// Measure compile + instantiate + first tick for the Counter fixture.
    ///
    /// # Errors
    ///
    /// Returns a string error when compilation, instantiation, or tick fails.
    pub fn cold_start_once() -> Result<Duration, String> {
        let t0 = Instant::now();
        let engine = Engine::default();
        let module = compile_counter_module(&engine, "counter_v1", COUNTER_V1_WAT)?;
        let mut instance = ScriptInstance::instantiate(&engine, &module)
            .map_err(|e| format!("instantiate: {e}"))?;
        let mut world = World::new();
        let entity = world.spawn_with(Counter { value: 0 });
        let mut events = EventBus::new();
        let mut diagnostics = DiagnosticAggregator::new();
        instance
            .call_init_entity(
                entity_id_to_i64(entity),
                &mut world,
                &mut events,
                &mut diagnostics,
            )
            .map_err(|e| format!("init_entity: {e}"))?;
        instance
            .tick(FIXED_DT, &mut world, &mut events, &mut diagnostics)
            .map_err(|e| format!("tick: {e}"))?;
        Ok(t0.elapsed())
    }

    /// Run the formal hot-reload preservation workload.
    ///
    /// Each cycle captures the full Counter state, poisons all counters, swaps
    /// to the alternate module version, restores the snapshot, and verifies the
    /// expected sum. This proves restore is doing the work rather than relying
    /// on the world still carrying the captured values.
    ///
    /// # Errors
    ///
    /// Returns a string error when configuration is invalid, a module cannot be
    /// instantiated, state capture/restore fails, or preservation drifts.
    pub fn hot_reload_preservation(
        &self,
        config: HotReloadConfig,
    ) -> Result<HotReloadReport, String> {
        if config.entity_count == 0 {
            return Err("entity_count must be > 0".to_owned());
        }
        if config.cycles == 0 {
            return Err("cycles must be > 0".to_owned());
        }

        let (mut world, entities) = seed_counter_world(config.entity_count);
        let mut instance = ScriptInstance::instantiate(&self.engine, &self.module_v1)
            .map_err(|e| format!("instantiate v1: {e}"))?;

        let mut durations = Vec::with_capacity(config.cycles as usize);
        let mut restored_components = 0usize;
        let mut final_counter_sum = 0i128;

        for cycle in 0..config.cycles {
            seed_cycle_values(&mut world, &entities, cycle);
            let expected_sum = counter_sum(&world);
            world.advance_tick();

            let t0 = Instant::now();
            let plan = capture_state(&world).map_err(|e| format!("capture: {e}"))?;
            poison_counter_world(&mut world, &entities, cycle);

            drop(instance);
            let next_module = if cycle % 2 == 0 {
                &self.module_v2
            } else {
                &self.module_v1
            };
            instance = ScriptInstance::instantiate(&self.engine, next_module)
                .map_err(|e| format!("instantiate swap target: {e}"))?;

            let restored = restore_state(&mut world, &plan).map_err(|e| format!("restore: {e}"))?;
            let elapsed = t0.elapsed();

            restored_components += restored;
            durations.push(elapsed);

            if restored != config.entity_count {
                return Err(format!(
                    "restored {restored} components, expected {}",
                    config.entity_count
                ));
            }

            let observed_sum = counter_sum(&world);
            if observed_sum != expected_sum {
                return Err(format!(
                    "counter drift after cycle {cycle}: expected {expected_sum}, got {observed_sum}"
                ));
            }
            final_counter_sum = observed_sum;
        }

        Ok(HotReloadReport {
            entity_count: config.entity_count,
            cycles: config.cycles,
            restored_components,
            total_duration: durations.iter().copied().sum(),
            p95_duration: percentile_duration(&durations, 95),
            max_duration: durations.iter().copied().max().unwrap_or_default(),
            final_counter_sum,
        })
    }

    /// Run the Phase 3.4 ECS-via-WASM ratio measurement.
    ///
    /// Drives the same logical workload — increment every Counter-bearing
    /// entity by 1 per frame, for `frames` frames — through two paths in
    /// sequence against independent worlds:
    ///
    /// 1. **Native baseline** — for every entity, scan `query::<Counter>()`
    ///    to find the matching `EntityId` (mirroring the host bridge's
    ///    `get_counter` linear search), then `world.insert` the new value
    ///    (mirroring `set_counter`'s scan + insert pair). Same algorithmic
    ///    work per increment as the wasm path; no shortcut.
    /// 2. **Wasm-via-host** — instantiate one `ScriptInstance` against the
    ///    Counter v1 module, call `init_entity(handle)` then `tick(dt)`
    ///    once per entity per frame. Each tick crosses the wasm boundary
    ///    once and re-enters the host bridge twice (`get_counter` +
    ///    `set_counter`) for the actual increment.
    ///
    /// Final assertion: both worlds end with each Counter incremented by
    /// exactly `frames`, validating both paths perform identical work.
    ///
    /// # Errors
    ///
    /// Returns a string error when configuration is invalid, the wasm
    /// instance fails to instantiate, or the post-run integrity check
    /// detects a per-Counter increment mismatch.
    pub fn ecs_iteration_ratio(
        &self,
        config: EcsIterationConfig,
    ) -> Result<EcsIterationReport, String> {
        if config.entity_count == 0 {
            return Err("entity_count must be > 0".to_owned());
        }
        if config.frames == 0 {
            return Err("frames must be > 0".to_owned());
        }

        // Independent worlds so the wasm side cannot pollute the native
        // measurement and vice versa.
        let (mut native_world, native_entities) = seed_counter_world(config.entity_count);
        let (mut wasm_world, wasm_entities) = seed_counter_world(config.entity_count);
        let native_handles: Vec<i64> = native_entities
            .iter()
            .copied()
            .map(entity_id_to_i64)
            .collect();
        let wasm_handles: Vec<i64> = wasm_entities
            .iter()
            .copied()
            .map(entity_id_to_i64)
            .collect();

        // Capture starting sums so we can verify both paths advance by
        // exactly `frames * entity_count` increments after the measurement.
        let native_start_sum = counter_sum(&native_world);
        let wasm_start_sum = counter_sum(&wasm_world);

        // ---- Native path ----
        let t_native = Instant::now();
        for _frame in 0..config.frames {
            for handle in &native_handles {
                native_increment_via_handle(&mut native_world, *handle);
            }
        }
        let native_total = t_native.elapsed();

        // ---- Wasm-via-host path ----
        let mut wasm_instance = ScriptInstance::instantiate(&self.engine, &self.module_v1)
            .map_err(|e| format!("instantiate v1: {e}"))?;
        let mut events = EventBus::new();
        let mut diagnostics = DiagnosticAggregator::new();

        let t_wasm = Instant::now();
        for _frame in 0..config.frames {
            for handle in &wasm_handles {
                wasm_instance
                    .call_init_entity(*handle, &mut wasm_world, &mut events, &mut diagnostics)
                    .map_err(|e| format!("init_entity: {e}"))?;
                wasm_instance
                    .tick(FIXED_DT, &mut wasm_world, &mut events, &mut diagnostics)
                    .map_err(|e| format!("tick: {e}"))?;
            }
        }
        let wasm_total = t_wasm.elapsed();

        // ---- Integrity check ----
        let expected_delta =
            i128::from(config.frames) * i128::try_from(config.entity_count).unwrap_or(i128::MAX);
        let native_delta = counter_sum(&native_world) - native_start_sum;
        let wasm_delta = counter_sum(&wasm_world) - wasm_start_sum;
        if native_delta != expected_delta {
            return Err(format!(
                "native path delta {native_delta} != expected {expected_delta}"
            ));
        }
        if wasm_delta != expected_delta {
            return Err(format!(
                "wasm path delta {wasm_delta} != expected {expected_delta}"
            ));
        }

        let frames_u32 = config.frames;
        let native_per_frame_avg = duration_div_u32(native_total, frames_u32);
        let wasm_per_frame_avg = duration_div_u32(wasm_total, frames_u32);
        let native_nanos = native_total.as_nanos();
        let ratio = if native_nanos == 0 {
            f64::INFINITY
        } else {
            // Casts: u128 → f64 has bounded precision loss but at the
            // microsecond / nanosecond scale the ratio is well within f64
            // mantissa (53 bits = ~9 quadrillion-int precision; both totals
            // fit comfortably).
            #[allow(
                clippy::cast_precision_loss,
                reason = "duration totals fit comfortably in f64 mantissa at the microsecond/nanosecond scale; ratio precision is dominated by measurement noise, not f64 conversion"
            )]
            let r = wasm_total.as_nanos() as f64 / native_nanos as f64;
            r
        };

        Ok(EcsIterationReport {
            entity_count: config.entity_count,
            frames: config.frames,
            native_total,
            wasm_total,
            native_per_frame_avg,
            wasm_per_frame_avg,
            ratio,
        })
    }

    /// Run the memory-soak hot-reload workload for at least `minimum_duration`.
    ///
    /// # Errors
    ///
    /// Returns a string error when configuration is invalid or preservation
    /// fails during any swap cycle.
    pub fn memory_soak(&self, config: MemorySoakConfig) -> Result<MemorySoakReport, String> {
        if config.entity_count == 0 {
            return Err("entity_count must be > 0".to_owned());
        }

        let started = Instant::now();
        let mut cycles = 0u32;
        let mut restored_components = 0usize;
        let mut final_counter_sum = 0i128;

        while started.elapsed() < config.minimum_duration || cycles == 0 {
            let report = self.hot_reload_preservation(HotReloadConfig {
                entity_count: config.entity_count,
                cycles: 1,
            })?;
            cycles = cycles.saturating_add(1);
            restored_components += report.restored_components;
            final_counter_sum = report.final_counter_sum;
        }

        Ok(MemorySoakReport {
            entity_count: config.entity_count,
            cycles,
            elapsed: started.elapsed(),
            restored_components,
            final_counter_sum,
        })
    }
}

impl std::fmt::Debug for ScriptHostBench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptHostBench")
            .field("module_v1", &self.module_v1.name())
            .field("module_v2", &self.module_v2.name())
            .finish_non_exhaustive()
    }
}

fn compile_counter_module(
    engine: &Engine,
    name: &'static str,
    wat_src: &str,
) -> Result<ScriptModule, String> {
    let bytes = wat::parse_str(wat_src).map_err(|e| format!("wat parse {name}: {e}"))?;
    ScriptModule::from_bytes(engine, name, &bytes).map_err(|e| format!("compile {name}: {e}"))
}

fn seed_counter_world(entity_count: usize) -> (World, Vec<EntityId>) {
    let mut world = World::new();
    let mut entities = Vec::with_capacity(entity_count);
    for index in 0..entity_count {
        entities.push(world.spawn_with(Counter {
            value: index_to_counter_value(0, index),
        }));
    }
    (world, entities)
}

fn seed_cycle_values(world: &mut World, entities: &[EntityId], cycle: u32) {
    for (index, entity) in entities.iter().copied().enumerate() {
        world.insert(
            entity,
            Counter {
                value: index_to_counter_value(cycle, index),
            },
        );
    }
}

fn poison_counter_world(world: &mut World, entities: &[EntityId], cycle: u32) {
    for entity in entities.iter().copied() {
        world.insert(
            entity,
            Counter {
                value: i64::MIN + i64::from(cycle),
            },
        );
    }
}

/// Native baseline mirror of `rge.ecs::get_counter` + `set_counter` —
/// scan the world to find the entity by handle, then write the
/// incremented Counter via `World::insert`. Two scans per call match
/// the host bridge's two scans (one per host function), so both paths
/// perform identical algorithmic work per increment.
fn native_increment_via_handle(world: &mut World, handle: i64) {
    let mut found_value: Option<i64> = None;
    let mut found_id: Option<EntityId> = None;
    for (id, counter) in world.query::<Counter>() {
        if entity_id_to_i64(id) == handle {
            found_value = Some(counter.value);
            found_id = Some(id);
            break;
        }
    }
    if let (Some(value), Some(id)) = (found_value, found_id) {
        world.insert(id, Counter { value: value + 1 });
    }
}

fn counter_sum(world: &World) -> i128 {
    world
        .query::<Counter>()
        .map(|(_, c)| i128::from(c.value))
        .sum()
}

fn index_to_counter_value(cycle: u32, index: usize) -> i64 {
    let cycle_base = i64::from(cycle) + 1;
    let index = i64::try_from(index).expect("formal entity counts fit in i64");
    cycle_base * 1_000_000 + index
}

fn percentile_duration(values: &[Duration], percentile: u32) -> Duration {
    debug_assert!(!values.is_empty(), "caller validates cycles > 0");
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let len = sorted.len();
    let rank = ((len * usize::try_from(percentile).expect("percentile fits usize")) + 99) / 100;
    sorted[rank.saturating_sub(1).min(len - 1)]
}

fn duration_div_u32(duration: Duration, divisor: u32) -> Duration {
    let nanos = duration.as_nanos() / u128::from(divisor);
    let nanos = u64::try_from(nanos).unwrap_or(u64::MAX);
    Duration::from_nanos(nanos)
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formal_config_matches_phase_3_exit_gate() {
        let config = HotReloadConfig::formal();
        assert_eq!(config.entity_count, 1_000);
        assert_eq!(config.cycles, 100);
        assert_eq!(
            MemorySoakConfig::formal().minimum_duration,
            Duration::from_secs(60 * 60)
        );
    }

    #[test]
    fn seeded_counter_world_has_expected_shape() {
        let (world, entities) = seed_counter_world(FORMAL_HOT_RELOAD_ENTITY_COUNT);
        assert_eq!(world.entity_count(), FORMAL_HOT_RELOAD_ENTITY_COUNT);
        assert_eq!(entities.len(), FORMAL_HOT_RELOAD_ENTITY_COUNT);
        assert_eq!(
            world.query::<Counter>().count(),
            FORMAL_HOT_RELOAD_ENTITY_COUNT
        );
    }

    #[test]
    fn percentile_duration_uses_nearest_rank() {
        let values: Vec<Duration> = (1_u64..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile_duration(&values, 95), Duration::from_millis(95));
    }

    #[test]
    fn cold_start_counter_module_completes() {
        let elapsed = ScriptHostBench::cold_start_once().expect("cold-start");
        assert!(
            elapsed < Duration::from_secs(50),
            "cold-start smoke should complete on any developer machine: {elapsed:?}"
        );
    }

    #[test]
    fn formal_100_cycle_preservation_gate_uses_1000_entities() {
        let bench = ScriptHostBench::new().expect("compile fixtures");
        let report = bench
            .hot_reload_preservation(HotReloadConfig::formal())
            .expect("formal preservation");

        println!(
            "phase3_hot_reload: entities={} cycles={} p95_ms={:.3} max_ms={:.3} avg_ms={:.3}",
            report.entity_count,
            report.cycles,
            report.p95_ms(),
            duration_ms(report.max_duration),
            report.average_ms()
        );

        assert_eq!(report.entity_count, FORMAL_HOT_RELOAD_ENTITY_COUNT);
        assert_eq!(report.cycles, HOT_RELOAD_CYCLES);
        assert_eq!(
            report.restored_components,
            FORMAL_HOT_RELOAD_ENTITY_COUNT * usize::try_from(HOT_RELOAD_CYCLES).unwrap()
        );
        assert!(
            report.p95_duration < Duration::from_millis(100),
            "formal hot-reload p95 budget is <100ms; got {:.3}ms",
            report.p95_ms()
        );
    }

    #[test]
    fn phase_3_4_ecs_via_wasm_ratio_records_baseline() {
        let bench = ScriptHostBench::new().expect("compile fixtures");
        let report = bench
            .ecs_iteration_ratio(EcsIterationConfig::formal())
            .expect("ECS-via-WASM ratio measurement");

        println!(
            "phase3_4_ecs_via_wasm: entities={} frames={} native_per_frame_us={:.2} wasm_per_frame_us={:.2} ratio={}",
            report.entity_count,
            report.frames,
            report.native_per_frame_avg.as_secs_f64() * 1_000_000.0,
            report.wasm_per_frame_avg.as_secs_f64() * 1_000_000.0,
            report.ratio_pretty(),
        );

        // Sanity gates only — we record the actual ratio but do NOT
        // assert ≤ 1.5× because the current `rge.ecs::{get_counter,
        // set_counter}` host bridge crosses the wasm boundary once per
        // entity per frame; achieving ≤ 1.5× requires bulk-iteration
        // host functions that are out of scope for this measurement
        // dispatch (Phase 3.4 substrate work proper).
        assert_eq!(report.entity_count, FORMAL_HOT_RELOAD_ENTITY_COUNT);
        assert_eq!(report.frames, 10);
        assert!(
            report.native_total > Duration::ZERO,
            "native path should take measurable time"
        );
        assert!(
            report.wasm_total > Duration::ZERO,
            "wasm path should take measurable time"
        );
        assert!(
            report.ratio.is_finite() && report.ratio > 0.0,
            "ratio must be finite and positive: {}",
            report.ratio
        );
    }

    #[test]
    fn ecs_iteration_config_validation_rejects_zero() {
        let bench = ScriptHostBench::new().expect("compile fixtures");
        let zero_entities = bench.ecs_iteration_ratio(EcsIterationConfig {
            entity_count: 0,
            frames: 1,
        });
        assert!(zero_entities.is_err());
        let zero_frames = bench.ecs_iteration_ratio(EcsIterationConfig {
            entity_count: 1,
            frames: 0,
        });
        assert!(zero_frames.is_err());
    }

    #[test]
    #[ignore = "Phase 3.4 memory-soak gate runs for one hour; run explicitly when validating release readiness"]
    fn phase_3_memory_soak_one_hour() {
        let bench = ScriptHostBench::new().expect("compile fixtures");
        let report = bench
            .memory_soak(MemorySoakConfig::formal())
            .expect("one-hour memory soak");
        assert!(report.elapsed >= FORMAL_MEMORY_SOAK_DURATION);
        assert!(report.cycles > 0);
        assert_eq!(
            report.restored_components,
            report.cycles as usize * report.entity_count
        );
    }
}
