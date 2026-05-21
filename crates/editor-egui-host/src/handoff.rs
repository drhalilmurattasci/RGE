//! Phase 9 dispatch C — `InspectorHandoff`, the latest-only handoff
//! substrate that carries an [`rge_editor_state::InspectorSnapshot`] from
//! the editor-shell publisher to the host's [`crate::InspectorTabBody`]
//! consumer.
//!
//! The semantics mirror [`rge_editor_shell::render_input::RenderHandoff`]
//! (`crates/editor-shell/src/render_input.rs`), which is the canonical
//! ADR-117 latest-only handoff in this workspace. We copy the
//! `Mutex<Option<Arc<_>>>` + `AtomicU64` composition verbatim so the two
//! sites stay structurally aligned: future audits can grep for
//! `Mutex<Option<Arc<` and find the same exact shape every time.
//!
//! # Why a separate handoff (not a direct `Arc<RwLock<…>>` on the tab body)
//!
//! The editor-shell publisher and the host consumer are in different
//! crates. The host crate must NOT depend on editor-shell (would create
//! a cycle and foreclose the planned `editor-shell → editor-egui-host`
//! direction). A handoff defined HERE — in the host crate — that takes
//! an `Arc<InspectorSnapshot>` from anywhere keeps the dep direction
//! clean: editor-shell holds an `Arc<InspectorHandoff>` clone, publishes
//! through it; the host's tab body holds another clone, acquires from
//! it. The handoff itself depends only on `rge-editor-state` (where
//! `InspectorSnapshot` lives).
//!
//! # No `unsafe`, no broader deps
//!
//! `unsafe_code = "forbid"` per workspace policy. Composition uses only
//! `std::sync::{Arc, Mutex}` and `std::sync::atomic::AtomicU64`. The
//! handoff is `Send + Sync` by construction (all fields are `Send +
//! Sync`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rge_editor_state::InspectorSnapshot;

/// Latest-only immutable handoff slot for the editor inspector snapshot.
///
/// editor-shell publishes a fresh `Arc<InspectorSnapshot>` once per
/// frame (BEFORE the host's egui render pass — see
/// `rge_editor_shell::EditorShell::render_frame`); the host's
/// [`crate::InspectorTabBody::ui`] acquires the most-recently-published
/// snapshot when the inspector tab is rendered. Older un-acquired
/// snapshots drop on the next publish (latest-only / drop-old per
/// ADR-117 sub-decision 4).
///
/// # Semantics (mirrors `RenderHandoff`)
///
/// - **Latest-only.** [`Self::publish`] *replaces* rather than queues.
///   If editor-shell publishes K times between two render frames, the
///   host reads only the Kth snapshot.
/// - **Immutable from publish.** The handoff holds
///   `Arc<InspectorSnapshot>`, exposing only `&InspectorSnapshot`;
///   editor-shell has no path to mutate after publish.
/// - **Non-blocking on both sides.** Render NEVER blocks editor-shell;
///   editor-shell NEVER blocks render beyond the trivial mutex-protected
///   swap of a single `Arc` reference (uncontended on the steady-state
///   editor frame loop).
/// - **`generation()` is O(1).** A monotonically advancing `u64` is
///   bumped on each publish; render can poll it without taking the
///   slot mutex to decide whether the snapshot is stale.
///
/// # Empty-state behavior
///
/// Before any publish, [`Self::acquire`] returns `None`. The host's
/// inspector tab body renders the [`InspectorSnapshot::default()`]
/// state in that case (zero ticks, "Editing", `0` selection counts,
/// etc.) so the tab is visible from frame 1 — there is no flicker of
/// "no data" text and no panic on an empty handoff.
///
/// # Trait bounds
///
/// `InspectorHandoff` is `Send + Sync` (composition of `Send + Sync`
/// std primitives). It is intentionally NOT `Clone` — the canonical
/// usage is `Arc<InspectorHandoff>`, cloned via `Arc::clone` so all
/// shareholders observe the same slot.
pub struct InspectorHandoff {
    /// Most-recent published snapshot. `None` before first publish.
    ///
    /// Wrapped in `Arc` so that `acquire()` can return a cheap clone
    /// of the reference without copying the underlying snapshot; the
    /// `Mutex` exists so `publish` can replace the slot atomically
    /// (latest-only semantics).
    slot: Mutex<Option<Arc<InspectorSnapshot>>>,

    /// Monotonically advancing counter, incremented after each
    /// successful `publish`. Read via `Acquire` ordering paired with
    /// publish's `Release` so that an observer of an updated generation
    /// is guaranteed to see the new slot contents on the next
    /// `acquire()`.
    generation: AtomicU64,
}

impl InspectorHandoff {
    /// Construct an empty handoff. `acquire()` returns `None` and
    /// `generation()` returns `0` until [`Self::publish`] runs.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(None),
            generation: AtomicU64::new(0),
        }
    }

    /// Publish a new snapshot, replacing any prior un-acquired one
    /// (latest-only / drop-old per ADR-117 sub-decision 4). Increments
    /// the generation counter after the slot is updated so that any
    /// reader observing the new generation is guaranteed to see the
    /// new snapshot on its next `acquire()`.
    ///
    /// # Panics
    ///
    /// Panics if the slot mutex is poisoned (a prior holder panicked
    /// while holding the lock). Poisoning is treated as a hard-stop
    /// bug; single-publisher / single-consumer v0 means poisoning can
    /// only come from a deeper invariant break.
    pub fn publish(&self, snapshot: Arc<InspectorSnapshot>) {
        let mut guard = self
            .slot
            .lock()
            .expect("InspectorHandoff slot mutex poisoned");
        *guard = Some(snapshot);
        // Release the guard BEFORE bumping the generation so the
        // ordering pair (publish-Release / acquire-Acquire) covers a
        // valid mutex window for the next consumer.
        drop(guard);
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Acquire the most recently published snapshot, or `None` if
    /// nothing has been published yet. The slot retains its `Arc`
    /// reference, so subsequent acquires within the same generation
    /// are cheap (each clones the `Arc`).
    ///
    /// # Panics
    ///
    /// Panics if the slot mutex is poisoned (see [`Self::publish`]).
    #[must_use]
    pub fn acquire(&self) -> Option<Arc<InspectorSnapshot>> {
        let guard = self
            .slot
            .lock()
            .expect("InspectorHandoff slot mutex poisoned");
        guard.clone()
    }

    /// Current generation counter (monotonically advancing on each
    /// publish). Cheap "did the publisher publish since I last
    /// looked?" check without taking the slot mutex.
    ///
    /// Ordering: `Acquire`, paired with the `Release` increment in
    /// [`Self::publish`] so that on observing a new generation the
    /// slot is guaranteed to be up-to-date on the next acquire.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

impl Default for InspectorHandoff {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for InspectorHandoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Mirror RenderHandoff's Debug: avoid taking the slot lock so
        // the impl is panic-free under poisoned-lock conditions.
        // Report only the generation; the slot contents are
        // inspectable via `acquire()` at the call site.
        f.debug_struct("InspectorHandoff")
            .field("generation", &self.generation())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_handoff_returns_none_and_zero_generation() {
        let handoff = InspectorHandoff::new();
        assert!(handoff.acquire().is_none());
        assert_eq!(handoff.generation(), 0);
    }

    #[test]
    fn publish_then_acquire_returns_published_snapshot() {
        let handoff = InspectorHandoff::new();
        let mut snap = InspectorSnapshot::default();
        snap.tick_count = 42;
        snap.time_scale = 0.5;
        handoff.publish(Arc::new(snap));

        let got = handoff.acquire().expect("snapshot present after publish");
        assert_eq!(got.tick_count, 42);
        assert_eq!(got.time_scale, 0.5);
    }

    #[test]
    fn publish_advances_generation_monotonically() {
        let handoff = InspectorHandoff::new();
        assert_eq!(handoff.generation(), 0);

        handoff.publish(Arc::new(InspectorSnapshot::default()));
        assert_eq!(handoff.generation(), 1);

        handoff.publish(Arc::new(InspectorSnapshot::default()));
        assert_eq!(handoff.generation(), 2);

        handoff.publish(Arc::new(InspectorSnapshot::default()));
        assert_eq!(handoff.generation(), 3);
    }

    #[test]
    fn latest_only_replaces_previous_snapshot() {
        let handoff = InspectorHandoff::new();

        let mut older = InspectorSnapshot::default();
        older.tick_count = 1;
        handoff.publish(Arc::new(older));

        let mut newer = InspectorSnapshot::default();
        newer.tick_count = 99;
        handoff.publish(Arc::new(newer));

        let got = handoff.acquire().expect("snapshot present");
        assert_eq!(got.tick_count, 99, "latest publish must win over older one");
    }

    #[test]
    fn acquire_does_not_drain_slot() {
        // Subsequent acquires within the same generation should keep
        // returning the same snapshot (drop-old fires only on the
        // next publish, not on acquire).
        let handoff = InspectorHandoff::new();
        let mut snap = InspectorSnapshot::default();
        snap.tick_count = 7;
        handoff.publish(Arc::new(snap));

        let first = handoff.acquire().expect("first acquire");
        let second = handoff.acquire().expect("second acquire");
        assert_eq!(first.tick_count, 7);
        assert_eq!(second.tick_count, 7);
        // Same generation; no publish in between.
        assert_eq!(handoff.generation(), 1);
    }

    #[test]
    fn default_constructs_empty_handoff() {
        let handoff = InspectorHandoff::default();
        assert!(handoff.acquire().is_none());
        assert_eq!(handoff.generation(), 0);
    }

    #[test]
    fn handoff_is_send_and_sync() {
        // Compile-time assertion: InspectorHandoff must be Send+Sync
        // so an Arc<InspectorHandoff> can be shared across threads
        // (future render-thread direction per ADR-117 future work).
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<InspectorHandoff>();
    }

    #[test]
    fn debug_reports_generation_without_locking_slot() {
        let handoff = InspectorHandoff::new();
        handoff.publish(Arc::new(InspectorSnapshot::default()));
        handoff.publish(Arc::new(InspectorSnapshot::default()));
        let formatted = format!("{handoff:?}");
        assert!(
            formatted.contains("generation: 2"),
            "Debug impl must include generation; got {formatted:?}"
        );
    }
}
