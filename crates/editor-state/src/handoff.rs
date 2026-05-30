//! `Handoff<T>` — the workspace's shared latest-only snapshot handoff.
//!
//! A single-publisher / single-consumer slot that carries the
//! most-recently-published `Arc<T>` from a producer to a consumer:
//! `publish` *replaces* (latest-only / drop-old), `acquire` reads the latest,
//! `generation` is an O(1) "did the producer publish since I last looked?"
//! counter. The composition is `Mutex<Option<Arc<T>>>` + `AtomicU64` — std-only
//! safe Rust, no `unsafe`.
//!
//! # Why it lives here, and what it is NOT
//!
//! `Handoff<T>` is **shared editor-tier infrastructure**, NOT a sixth
//! coordination category (the §0.6 freeze gates those at 5 — Selection, Hover,
//! ActiveTool, ModalState, DragDrop — and this is not one of them). It lives in
//! `editor-state` for the **same dep-neutrality reason** as the observation
//! aggregators [`crate::InspectorSnapshot`] / [`crate::SaveStatusSnapshot`]:
//! both `editor-shell` and `editor-egui-host` already depend on `editor-state`,
//! and neither may depend on the other (that would cycle), so a type both need
//! to share is housed here. The `editor-state-ownership` lint does not list
//! `Handoff`, so Part A does not fire; the module imports only `std::sync`, so
//! Part B does not fire.
//!
//! # Relationship to ADR-117
//!
//! This generic implements the latest-only handoff **semantics** pinned by
//! `docs/adr/ADR-117-render-handoff-mechanism.md`:
//!
//! - **Latest-only / drop-old** (sub-decision 1 & 4): [`Handoff::publish`]
//!   replaces rather than queues; if the producer publishes K times between two
//!   consumer reads, the first K−1 snapshots drop (their `Arc` strong count
//!   reaches zero) and the consumer reads only the Kth.
//! - **Immutable from publish** (sub-decision 1): the slot holds `Arc<T>` and
//!   hands out only `Arc<T>` clones — the producer has no path to mutate a
//!   published snapshot.
//! - **Non-blocking on both sides** (sub-decision 1): neither side blocks the
//!   other beyond the trivial mutex-guarded swap of a single `Arc` reference,
//!   uncontended on the steady-state hot path.
//! - **Monotonic generation** (sub-decision 3): [`Handoff::generation`] is an
//!   opaque, monotonically advancing `u64` (NOT a domain tick) for cheap
//!   "should I re-acquire?" polling without taking the slot lock.
//!
//! ADR-117 sub-decision 5 (*"pin SEMANTICS, defer crate/std choice to dispatch
//! 4"*) recommends exactly this std-only `Mutex<Option<Arc<_>>>` composition,
//! and non-decision 6 explicitly leaves the concrete primitive/crate choice
//! open — so housing the mechanism here as a payload-agnostic generic is within
//! the ADR's latitude. Any per-payload identity that ADR-117 anchors on (e.g.
//! `RenderInputOwned`'s `(ecs_tick, checkpoint_id)`) stays a **field of the
//! payload `T`**, not of the handoff.
//!
//! # One definition, not three copies
//!
//! `RenderHandoff`, `InspectorHandoff`, and `SaveStatusHandoff` were three
//! byte-identical hand-written copies of this slot; they are now thin type
//! aliases over `Handoff<T>` at their original sites
//! (`crates/editor-shell/src/render_input.rs`,
//! `crates/editor-egui-host/src/handoff.rs`). The earlier doctrine kept the
//! copies verbatim "so audits grep the same `Mutex<Option<Arc<` shape"; that
//! intent is now served *better* — auditors read this single definition instead
//! of reconciling three.
//!
//! # v0 contract
//!
//! Single-publisher / single-consumer (ADR-117 mitigation 1). A poisoned slot
//! mutex (a holder panicked mid-swap) is treated as a hard-stop invariant break
//! and surfaces as a panic; multi-publisher / multi-consumer is a future
//! amendment if it ever surfaces.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Latest-only immutable snapshot slot. See the module doc for semantics and
/// the ADR-117 relationship.
///
/// # Trait bounds
///
/// `Handoff<T>` is `Send + Sync` whenever `T: Send + Sync` (the slot is
/// `Mutex<Option<Arc<T>>>`); the bound is auto-derived per payload — there is
/// no explicit `T: …` bound on the type, `new`, `publish`, `acquire`,
/// `generation`, `Default`, or `Debug` (the `Debug` impl reports only the
/// generation, so it needs no `T: Debug`). Intentionally **not** `Clone` — the
/// canonical usage is `Arc<Handoff<T>>`, cloned via `Arc::clone` so all
/// shareholders observe the same slot.
pub struct Handoff<T> {
    /// Most-recent published snapshot; `None` before the first publish.
    slot: Mutex<Option<Arc<T>>>,
    /// Monotonically advancing counter, bumped (Release) after each publish.
    generation: AtomicU64,
}

impl<T> Handoff<T> {
    /// Construct an empty handoff. [`Self::acquire`] returns `None` and
    /// [`Self::generation`] returns `0` until the first [`Self::publish`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(None),
            generation: AtomicU64::new(0),
        }
    }

    /// Publish a new snapshot, replacing any prior un-acquired one (latest-only
    /// / drop-old). Increments the generation AFTER the slot swap (Release),
    /// paired with [`Self::generation`]'s Acquire load, so a reader that
    /// observes the new generation is guaranteed to see the new slot contents
    /// on its next [`Self::acquire`].
    ///
    /// # Panics
    ///
    /// Panics if the slot mutex is poisoned (a prior holder panicked while
    /// holding the lock). Single-publisher / single-consumer v0 means poisoning
    /// can only come from a deeper invariant break, so it is treated as a
    /// hard-stop bug.
    pub fn publish(&self, snapshot: Arc<T>) {
        let mut guard = self.slot.lock().expect("Handoff slot mutex poisoned");
        *guard = Some(snapshot);
        // Release the guard BEFORE bumping the generation so the ordering pair
        // (publish-Release / acquire-Acquire) covers a valid mutex window for
        // the next consumer.
        drop(guard);
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Acquire the most recently published snapshot, or `None` if nothing has
    /// been published yet. The slot retains its `Arc`, so subsequent acquires
    /// within the same generation are cheap (each clones the `Arc`);
    /// drop-old fires only on the next [`Self::publish`].
    ///
    /// # Panics
    ///
    /// Panics if the slot mutex is poisoned (see [`Self::publish`]).
    #[must_use]
    pub fn acquire(&self) -> Option<Arc<T>> {
        let guard = self.slot.lock().expect("Handoff slot mutex poisoned");
        guard.clone()
    }

    /// Current generation counter (monotonically advancing on each publish).
    /// Cheap "did the producer publish since I last looked?" check without
    /// taking the slot mutex.
    ///
    /// Ordering: `Acquire`, paired with the `Release` increment in
    /// [`Self::publish`].
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

impl<T> Default for Handoff<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Debug for Handoff<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Avoid taking the slot lock so Debug is panic-free under a poisoned
        // lock. Report only the generation (no `T: Debug` bound needed); the
        // slot contents are inspectable via `acquire()` at the call site.
        f.debug_struct("Handoff")
            .field("generation", &self.generation())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A small, representative non-Copy payload (mirrors the real payloads,
    // which all carry owned data) for exercising the generic once.
    #[derive(Debug, PartialEq)]
    struct Probe {
        id: u64,
        label: String,
    }

    fn probe(id: u64, label: &str) -> Arc<Probe> {
        Arc::new(Probe {
            id,
            label: label.to_string(),
        })
    }

    #[test]
    fn fresh_handoff_returns_none_and_zero_generation() {
        let h: Handoff<Probe> = Handoff::new();
        assert!(h.acquire().is_none());
        assert_eq!(h.generation(), 0);
    }

    #[test]
    fn publish_then_acquire_returns_published_snapshot() {
        let h = Handoff::new();
        h.publish(probe(7, "seven"));
        let got = h.acquire().expect("snapshot present after publish");
        assert_eq!(got.id, 7);
        assert_eq!(got.label, "seven");
    }

    #[test]
    fn publish_advances_generation_monotonically() {
        let h: Handoff<Probe> = Handoff::new();
        assert_eq!(h.generation(), 0);
        h.publish(probe(1, "a"));
        assert_eq!(h.generation(), 1);
        h.publish(probe(2, "b"));
        assert_eq!(h.generation(), 2);
        h.publish(probe(3, "c"));
        assert_eq!(h.generation(), 3);
    }

    #[test]
    fn latest_only_replaces_previous_snapshot() {
        let h = Handoff::new();
        h.publish(probe(1, "old"));
        h.publish(probe(99, "new"));
        let got = h.acquire().expect("snapshot present");
        assert_eq!(got.id, 99, "latest publish must win over the older one");
        assert_eq!(got.label, "new");
    }

    #[test]
    fn acquire_does_not_drain_slot() {
        // Subsequent acquires within the same generation keep returning the
        // same snapshot; drop-old fires only on the next publish.
        let h = Handoff::new();
        h.publish(probe(7, "seven"));
        let first = h.acquire().expect("first acquire");
        let second = h.acquire().expect("second acquire");
        assert_eq!(first.id, 7);
        assert_eq!(second.id, 7);
        assert_eq!(h.generation(), 1);
    }

    #[test]
    fn dropped_snapshot_releases_when_replaced() {
        // Latest-only / drop-old: once replaced AND no reader holds it, the
        // old snapshot's strong count reaches zero.
        let h = Handoff::new();
        let first = probe(1, "first");
        let weak = Arc::downgrade(&first);
        h.publish(first);
        assert!(weak.upgrade().is_some(), "held by the slot");
        h.publish(probe(2, "second"));
        assert!(
            weak.upgrade().is_none(),
            "replaced snapshot must drop once no reader holds it"
        );
    }

    #[test]
    fn default_constructs_empty_handoff() {
        let h: Handoff<Probe> = Handoff::default();
        assert!(h.acquire().is_none());
        assert_eq!(h.generation(), 0);
    }

    #[test]
    fn handoff_is_send_and_sync_for_send_sync_payload() {
        fn assert_send_sync<H: Send + Sync + 'static>() {}
        // Probe is Send + Sync, so Handoff<Probe> must be too.
        assert_send_sync::<Handoff<Probe>>();
    }

    #[test]
    fn debug_reports_generation_without_locking_slot() {
        let h: Handoff<Probe> = Handoff::new();
        h.publish(probe(1, "a"));
        h.publish(probe(2, "b"));
        let formatted = format!("{h:?}");
        assert!(
            formatted.contains("generation: 2"),
            "Debug impl must include generation; got {formatted:?}"
        );
        assert!(
            formatted.contains("Handoff"),
            "Debug impl reports the generic name; got {formatted:?}"
        );
    }
}
