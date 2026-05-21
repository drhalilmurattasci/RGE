//! Phase 9 dispatch C — public-API smoke tests for the
//! [`rge_editor_egui_host::InspectorHandoff`] latest-only handoff
//! substrate.
//!
//! These tests are the integration-layer companion to the inline
//! unit tests in `src/handoff.rs::tests`. The inline tests pin the
//! per-method behavior with full access to private struct internals;
//! these tests pin the **public-API** shape — `InspectorHandoff` is
//! reachable through the top-level crate re-export, the constructor
//! produces an empty handoff, publish/acquire/generation behave the
//! same way an out-of-crate consumer would observe.
//!
//! Why both: the inline tests would still pass if `pub use
//! handoff::InspectorHandoff` got dropped from the crate root by a
//! future refactor (they'd just test the `crate::handoff::*` path).
//! These integration tests fail to compile if the public re-export
//! disappears, catching API-surface regressions loudly.

use std::sync::Arc;

use rge_editor_egui_host::InspectorHandoff;
use rge_editor_state::InspectorSnapshot;

// ---------------------------------------------------------------------------
// Trait bounds
// ---------------------------------------------------------------------------

/// Compile-time assertion: the handoff is `Send + Sync + 'static`.
/// Required so an `Arc<InspectorHandoff>` can be shared across the
/// editor-shell publisher and the in-host inspector tab body without
/// lifetime gymnastics, and prepares for a future render-thread
/// direction per ADR-117 future work.
#[test]
fn handoff_is_send_sync_static() {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<InspectorHandoff>();
}

// ---------------------------------------------------------------------------
// Constructor / empty state
// ---------------------------------------------------------------------------

#[test]
fn new_returns_empty_handoff() {
    let handoff = InspectorHandoff::new();
    assert!(
        handoff.acquire().is_none(),
        "empty handoff must return None on acquire"
    );
    assert_eq!(
        handoff.generation(),
        0,
        "empty handoff must report generation 0"
    );
}

#[test]
fn default_matches_new() {
    let by_new = InspectorHandoff::new();
    let by_default = InspectorHandoff::default();
    assert!(by_new.acquire().is_none());
    assert!(by_default.acquire().is_none());
    assert_eq!(by_new.generation(), 0);
    assert_eq!(by_default.generation(), 0);
}

// ---------------------------------------------------------------------------
// Publish / acquire / generation
// ---------------------------------------------------------------------------

#[test]
fn publish_then_acquire_returns_published_data() {
    let handoff = InspectorHandoff::new();
    let mut snap = InspectorSnapshot::default();
    snap.tick_count = 1234;
    snap.time_scale = 0.25;
    snap.is_dirty = true;
    snap.selection_len = 7;

    handoff.publish(Arc::new(snap));

    let observed = handoff.acquire().expect("snapshot present after publish");
    assert_eq!(observed.tick_count, 1234);
    assert_eq!(observed.time_scale, 0.25);
    assert!(observed.is_dirty);
    assert_eq!(observed.selection_len, 7);
}

#[test]
fn publish_advances_generation_by_one_each_call() {
    let handoff = InspectorHandoff::new();
    assert_eq!(handoff.generation(), 0);

    for expected in 1..=10u64 {
        handoff.publish(Arc::new(InspectorSnapshot::default()));
        assert_eq!(
            handoff.generation(),
            expected,
            "generation must monotonically advance"
        );
    }
}

#[test]
fn latest_publish_wins() {
    let handoff = InspectorHandoff::new();

    for n in 1..=20u64 {
        let mut snap = InspectorSnapshot::default();
        snap.tick_count = n;
        handoff.publish(Arc::new(snap));
    }

    // After 20 publishes, acquire must observe the 20th.
    let observed = handoff.acquire().expect("snapshot present");
    assert_eq!(
        observed.tick_count, 20,
        "latest-only must keep only the most recent publish"
    );
    assert_eq!(
        handoff.generation(),
        20,
        "generation must reflect 20 publishes"
    );
}

#[test]
fn acquire_within_same_generation_returns_same_data() {
    let handoff = InspectorHandoff::new();
    let mut snap = InspectorSnapshot::default();
    snap.tick_count = 555;
    handoff.publish(Arc::new(snap));

    for _ in 0..5 {
        let observed = handoff.acquire().expect("acquire within same generation");
        assert_eq!(observed.tick_count, 555);
    }
    assert_eq!(handoff.generation(), 1, "acquire must NOT bump generation");
}

#[test]
fn acquire_returns_independent_arc_clones() {
    // Both acquires return a clone of the *same* Arc — different
    // ref-counted handles to the same allocation. Mutating one
    // clone (impossible since InspectorSnapshot is Copy) wouldn't
    // affect the other; pointer-equality across acquires proves the
    // handoff isn't copying the snapshot per acquire.
    let handoff = InspectorHandoff::new();
    handoff.publish(Arc::new(InspectorSnapshot::default()));

    let first = handoff.acquire().expect("first acquire");
    let second = handoff.acquire().expect("second acquire");
    assert!(
        Arc::ptr_eq(&first, &second),
        "successive acquires within same generation must return Arc clones \
         of the SAME snapshot (no allocation per acquire)"
    );
}

#[test]
fn publish_drops_previous_arc_when_uniquely_held() {
    // Latest-only semantics: when an old Arc has no other holder,
    // publish should release it entirely so memory pressure is
    // bounded by 1 snapshot in steady state.
    let handoff = InspectorHandoff::new();

    let first = Arc::new(InspectorSnapshot::default());
    let first_weak = Arc::downgrade(&first);
    handoff.publish(first);
    // No external holder of `first` now (we moved it into publish);
    // the handoff is the sole strong holder.
    assert_eq!(first_weak.strong_count(), 1);

    let second = Arc::new(InspectorSnapshot::default());
    handoff.publish(second);

    // The first Arc should now be dropped (strong count 0). The
    // handoff replaced it; no other strong reference exists.
    assert_eq!(
        first_weak.strong_count(),
        0,
        "latest-only must drop the previous Arc on publish"
    );
}

// ---------------------------------------------------------------------------
// Cross-thread shareability
// ---------------------------------------------------------------------------

#[test]
fn handoff_is_shareable_across_threads_via_arc() {
    // Spawn one publisher thread and one consumer thread; they share
    // a single `Arc<InspectorHandoff>`. The consumer polls the
    // generation counter to detect publishes. This exercises the
    // Send+Sync bound at runtime + the lock-free generation read.
    let handoff = Arc::new(InspectorHandoff::new());

    let publisher_handoff = Arc::clone(&handoff);
    let publisher = std::thread::spawn(move || {
        for n in 1..=50u64 {
            let mut snap = InspectorSnapshot::default();
            snap.tick_count = n;
            publisher_handoff.publish(Arc::new(snap));
            // Yield so the consumer gets a chance to observe.
            std::thread::yield_now();
        }
    });

    let consumer_handoff = Arc::clone(&handoff);
    let consumer = std::thread::spawn(move || {
        let mut last_seen = 0u64;
        let mut observations = 0;
        // Spin until we've observed at least one publish OR the
        // publisher has clearly finished (generation >= 50 implies
        // we missed them all; that's still a valid observation).
        while consumer_handoff.generation() < 50 || observations == 0 {
            if let Some(snap) = consumer_handoff.acquire() {
                if snap.tick_count > last_seen {
                    last_seen = snap.tick_count;
                    observations += 1;
                }
            }
            std::thread::yield_now();
            // Bail out if the publisher finished cleanly.
            if consumer_handoff.generation() == 50 && observations > 0 {
                break;
            }
        }
        (observations, last_seen)
    });

    publisher.join().expect("publisher join");
    let (observations, last_seen) = consumer.join().expect("consumer join");
    assert_eq!(
        handoff.generation(),
        50,
        "publisher must complete 50 publishes"
    );
    assert!(
        observations >= 1,
        "consumer must observe at least one publish"
    );
    assert!(
        last_seen >= 1 && last_seen <= 50,
        "consumer's last observation must be within 1..=50; got {last_seen}"
    );
}
