//! Gate C prerequisite dispatch 1 — `RenderInput<'a>` boundary test.
//! Gate C prerequisite dispatch 2 — boundary-discipline regression.
//! Gate C prerequisite dispatch 4 — `RenderInputOwned` + `RenderHandoff`
//! handoff primitive.
//!
//! Pins the structural shape of the snapshot-handoff boundary
//! between sim/editor state and the render path. Does NOT exercise
//! GPU init, threading, or wire format.
//!
//! See `crates/editor-shell/src/render_input.rs` for the boundary
//! rationale; PLAN.md §13.6 (Gate C measurability) and §1.5.2
//! (`(ECS_tick_N, CadCheckpointId_N)` immutability) for the upstream
//! contract this boundary will eventually enforce; `docs/adr/ADR-117-
//! render-handoff-mechanism.md` for the binding handoff semantics.

use std::sync::Arc;

use rge_editor_shell::{
    EditorCameraState, EditorShell, RenderHandoff, RenderInput, RenderInputOwned,
};

/// Structural — confirms `RenderInput::from_editor_shell`
/// constructs cleanly from a default-built [`EditorShell`] and that
/// the public type is reachable from outside the crate via the
/// `pub use` re-export in `lib.rs`.
#[test]
fn from_editor_shell_constructs_cleanly() {
    let shell = EditorShell::default();
    let _input = RenderInput::from_editor_shell(&shell);
}

/// Field-presence — confirms `editor_camera` is reachable through
/// [`RenderInput`] as a borrowed reference. The default
/// `EditorCameraState` places the eye at `(3, 3, 3)`; we check that
/// invariant through the view-type to prove the field traversal
/// works (and to guard against accidental rewiring of the field).
#[test]
fn editor_camera_field_reachable_via_render_input() {
    let shell = EditorShell::default();
    let input = RenderInput::from_editor_shell(&shell);
    // Default eye is (3, 3, 3) per `EditorCameraState::default()`.
    assert!((input.editor_camera.eye.x - 3.0).abs() < f32::EPSILON);
    assert!((input.editor_camera.eye.y - 3.0).abs() < f32::EPSILON);
    assert!((input.editor_camera.eye.z - 3.0).abs() < f32::EPSILON);
}

// Gate C prerequisite dispatch 2 — boundary discipline regression
// =============================================================
// The two tests below pin the structural rule that render-side
// per-frame / per-resize functions must NOT read `self.editor_camera`
// directly; they must consume it via a `&RenderInput<'_>` parameter
// instead. They use source-text inspection (`include_str!`) rather
// than a new architecture lint to keep enforcement editor-shell local
// and dependency-free.
//
// **Scope clarification**: `init_render_state` is one-shot setup and
// is intentionally OUT of the per-frame / per-resize handoff
// boundary, so its `self.editor_camera` reads are not flagged.
//
// Brittleness budget: matches on stable signature prefixes
// (`fn render_frame(`, `fn resize_render_path(`). Robust to
// whitespace inside function bodies; would fail if someone renames
// the functions — that's a deliberate trip-wire, not brittleness.

/// Discipline — `render_frame` body must not read
/// `self.editor_camera` directly. Today's `render_frame` reads zero
/// sim-side state per frame (camera updates land via the GPU UBO
/// from `resize_render_path`); a future regression that reaches
/// into mutable sim state through `self` would defeat the Gate C
/// boundary. PLAN.md §13.6.
#[test]
fn render_frame_body_does_not_read_self_editor_camera() {
    let source = include_str!("../src/render_path.rs");
    let body = function_body(source, "fn render_frame(");
    assert!(
        !body.contains("self.editor_camera"),
        "render_frame body reads `self.editor_camera` directly — route through `RenderInput` instead.\n\nBody:\n{body}"
    );
}

/// Discipline — `resize_render_path` body must not read
/// `self.editor_camera` directly. Per Gate C dispatch 1, this
/// function takes `&RenderInput<'_>` and reads
/// `render_input.editor_camera` for the view*proj update. A
/// regression that bypasses the parameter and reaches into
/// `self.editor_camera` would re-couple the render path to
/// mutable sim state. PLAN.md §13.6 / §1.5.2.
#[test]
fn resize_render_path_body_does_not_read_self_editor_camera() {
    let source = include_str!("../src/render_path.rs");
    let body = function_body(source, "fn resize_render_path(");
    assert!(
        !body.contains("self.editor_camera"),
        "resize_render_path body reads `self.editor_camera` directly — route through `RenderInput` instead.\n\nBody:\n{body}"
    );
}

/// Extracts a function body (`{ ... }`) from `source` by locating
/// the first `{` after `signature_prefix` and walking matched
/// braces. Sufficient for `render_path.rs` (no string literals
/// containing unmatched braces; doc-comments live above the body).
fn function_body<'a>(source: &'a str, signature_prefix: &str) -> &'a str {
    let sig_idx = source
        .find(signature_prefix)
        .unwrap_or_else(|| panic!("signature `{signature_prefix}` not found in render_path.rs"));
    let body_start = source[sig_idx..]
        .find('{')
        .map(|i| sig_idx + i)
        .unwrap_or_else(|| panic!("no opening brace after `{signature_prefix}`"));
    let bytes = source.as_bytes();
    let mut depth: i32 = 0;
    for i in body_start..bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[body_start..=i];
                }
            }
            _ => {}
        }
    }
    panic!("function body for `{signature_prefix}` not closed")
}

// Gate C prerequisite dispatch 4 — `RenderInputOwned` + `RenderHandoff`
// =====================================================================
// The tests below pin the load-bearing invariants of the latest-only
// immutable render-input handoff per ADR-117. All assertions are
// dependency-free (std-only) and exercise the public re-exports
// from `rge_editor_shell`.
//
// Coverage:
//   1. compile-time Send + 'static (RenderInputOwned)
//   2. compile-time Send + Sync (RenderHandoff)
//   3. publish → acquire round-trip (happy path)
//   4. latest-only / drop-old (Arc strong-count check)
//   5. generation counter monotonicity (0 → 1 → 2)
//   6. empty-slot None return
//   7. as_render_input borrow round-trip

/// Build a small `RenderInputOwned` snapshot anchored at the given
/// `(ecs_tick, checkpoint_id)`. Camera state is the documented
/// editor-runtime default (eye `(3, 3, 3)`, looking at origin).
fn owned_snapshot(ecs_tick: u64, checkpoint_id: u64) -> RenderInputOwned {
    RenderInputOwned {
        ecs_tick,
        checkpoint_id,
        editor_camera: EditorCameraState::default(),
    }
}

/// **RIO-1** — compile-time assertion that `RenderInputOwned: Send +
/// 'static`. Per ADR-117 sub-decision 2 the bound is pinned now so
/// the cross-thread substrate is ready when the future render-thread
/// dispatch lands. The function does no runtime work; the failure
/// mode is a compile error inside the test's helper bound.
#[test]
fn render_input_owned_is_send_and_static() {
    fn assert_send_static<T: Send + 'static>() {}
    assert_send_static::<RenderInputOwned>();
}

/// **RH-1** — compile-time assertion that `RenderHandoff: Send +
/// Sync`. The `Mutex<Option<Arc<_>>>` + `AtomicU64` combo gives this
/// automatically; the assertion documents the bound and catches any
/// future field addition that would silently regress it.
#[test]
fn render_handoff_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RenderHandoff>();
}

/// **RH-2** — publish → acquire happy path. After one publish, the
/// acquired snapshot carries the same anchor values that were
/// published.
#[test]
fn render_handoff_publish_then_acquire_returns_same_snapshot() {
    let handoff = RenderHandoff::new();
    let snap = Arc::new(owned_snapshot(42, 7));
    handoff.publish(Arc::clone(&snap));
    let got = handoff
        .acquire()
        .expect("post-publish acquire must yield a snapshot");
    assert_eq!(got.ecs_tick, 42);
    assert_eq!(got.checkpoint_id, 7);
    // The acquired Arc is the *same* allocation we published (latest-only
    // semantics keep the snapshot in the slot until the next publish).
    assert!(
        Arc::ptr_eq(&snap, &got),
        "acquire must return the same Arc allocation that was published"
    );
}

/// **RH-3** — latest-only / drop-old. Publish A, publish B without
/// acquiring A; acquire returns B; A's strong-count is 1 (only the
/// test holder; the handoff dropped its reference). Per ADR-117
/// sub-decision 4 — the first K-1 of K snapshots between two render
/// frames must drop.
#[test]
fn render_handoff_latest_only_drops_older() {
    let handoff = RenderHandoff::new();
    let a = Arc::new(owned_snapshot(1, 100));
    let b = Arc::new(owned_snapshot(2, 200));

    handoff.publish(Arc::clone(&a));
    // Sanity — sim's local + handoff's slot = 2 strong refs.
    assert_eq!(Arc::strong_count(&a), 2);

    handoff.publish(Arc::clone(&b));

    // The handoff has replaced its A reference with B; the only
    // remaining A is the test holder.
    assert_eq!(
        Arc::strong_count(&a),
        1,
        "older snapshot must drop to strong-count 1 (test holder only) after newer publish"
    );

    // Acquire returns B's anchor values.
    let got = handoff.acquire().expect("post-publish acquire must yield");
    assert_eq!(got.ecs_tick, 2);
    assert_eq!(got.checkpoint_id, 200);
    assert!(Arc::ptr_eq(&b, &got));
}

/// **RH-4** — generation counter advances monotonically on each
/// publish (0 → 1 → 2). Per ADR-117 sub-decision 3 the counter is
/// the O(1) "did sim publish?" signal for the render-side poll.
#[test]
fn render_handoff_generation_advances_monotonically() {
    let handoff = RenderHandoff::new();
    assert_eq!(
        handoff.generation(),
        0,
        "fresh handoff starts at generation 0"
    );

    handoff.publish(Arc::new(owned_snapshot(1, 10)));
    assert_eq!(
        handoff.generation(),
        1,
        "first publish bumps generation to 1"
    );

    handoff.publish(Arc::new(owned_snapshot(2, 20)));
    assert_eq!(
        handoff.generation(),
        2,
        "second publish bumps generation to 2"
    );

    handoff.publish(Arc::new(owned_snapshot(3, 30)));
    assert_eq!(
        handoff.generation(),
        3,
        "third publish bumps generation to 3"
    );
}

/// **RH-5** — acquire before any publish returns `None`. Per ADR-117
/// sub-decision 1, render's "if no snapshot has ever been published,
/// render either skips the frame or uses a sentinel" is the caller's
/// choice; this test pins the substrate-honest `Option<_>` contract.
#[test]
fn render_handoff_acquire_before_publish_returns_none() {
    let handoff = RenderHandoff::new();
    assert!(
        handoff.acquire().is_none(),
        "empty handoff acquire must return None"
    );
    assert_eq!(handoff.generation(), 0, "no publish, no generation advance");
}

/// **RIO-2** — `as_render_input()` borrows the owned snapshot's
/// payload back into the dispatch-1 [`RenderInput<'_>`] shape. The
/// returned view's `editor_camera.eye` matches the owned value
/// element-wise.
#[test]
fn render_input_owned_as_render_input_borrows_correctly() {
    let owned = owned_snapshot(99, 999);
    let borrowed: RenderInput<'_> = owned.as_render_input();
    // The default camera places the eye at (3, 3, 3) (see
    // EditorCameraState::default()).
    assert!((borrowed.editor_camera.eye.x - 3.0).abs() < f32::EPSILON);
    assert!((borrowed.editor_camera.eye.y - 3.0).abs() < f32::EPSILON);
    assert!((borrowed.editor_camera.eye.z - 3.0).abs() < f32::EPSILON);
    // And the borrowed view points at the same address as the owned
    // field — proves the call is borrow-only (no clone).
    assert!(std::ptr::eq(
        borrowed.editor_camera as *const _,
        &owned.editor_camera as *const _
    ));
}

// Gate C prerequisite dispatch 5 — empirical handoff invariant
// =============================================================
// The test below validates the PLAN §13.6 invariant at the
// `RenderHandoff` boundary per ADR-117: a render-side acquired
// snapshot remains stable while sim-side publishes/mutates newer
// snapshots.
//
// **Scope honesty (LOAD-BEARING)**: today's renderer runs inline on
// `WindowEvent::RedrawRequested` — there is NO dedicated render
// thread yet. This is a single-threaded **empirical proxy** for the
// cross-thread invariant; it proves the handoff's `Arc` semantics
// preserve a held snapshot through subsequent publishes. When the
// dedicated renderer thread lands in a future dispatch, the same
// invariant must continue to hold under real concurrency. **This
// test does NOT certify a full render-thread architecture.**

/// **GC-1** — Gate C empirical handoff invariant per PLAN §13.6
/// + ADR-117. A render-side `acquire()` clones the published
/// `Arc<RenderInputOwned>`; the resulting handle MUST remain stable
/// across subsequent `publish()` calls. The newest publish becomes
/// visible only on the next `acquire()`. Single-threaded proxy
/// today; the same invariant must hold under future real
/// concurrency without changing this test's shape.
#[test]
fn gate_c_held_snapshot_stable_across_subsequent_publishes() {
    use glam::Vec3;

    let with_eye = |tick: u64, ckpt: u64, eye_x: f32| RenderInputOwned {
        ecs_tick: tick,
        checkpoint_id: ckpt,
        editor_camera: EditorCameraState {
            eye: Vec3::new(eye_x, eye_x, eye_x),
            ..EditorCameraState::default()
        },
    };

    let handoff = RenderHandoff::new();

    // Sim publishes snapshot N. Render acquires and HOLDS the Arc.
    handoff.publish(Arc::new(with_eye(100, 1000, 1.0)));
    let render_held = handoff.acquire().expect("snapshot N was published");
    assert_eq!(handoff.generation(), 1);

    // Sim publishes N+1 then N+2 with different anchors and camera.
    // N+1 will be dropped by the latest-only semantics; render-held
    // remains anchored to N.
    handoff.publish(Arc::new(with_eye(101, 1001, 2.0)));
    handoff.publish(Arc::new(with_eye(102, 1002, 3.0)));

    // LOAD-BEARING: the held snapshot is UNCHANGED despite 2
    // subsequent publishes. This is the §13.6 invariant the
    // handoff guarantees: a render-side acquired snapshot is
    // immutable for the lifetime of the held `Arc`.
    assert_eq!(
        render_held.ecs_tick, 100,
        "held snapshot's ecs_tick must not change"
    );
    assert_eq!(
        render_held.checkpoint_id, 1000,
        "held snapshot's checkpoint_id must not change"
    );
    assert!(
        (render_held.editor_camera.eye.x - 1.0).abs() < f32::EPSILON,
        "held snapshot's camera must not change"
    );

    // Fresh acquire returns the LATEST published snapshot (N+2);
    // N+1 was dropped by latest-only / drop-old semantics.
    let render_fresh = handoff.acquire().expect("snapshot N+2 was published");
    assert_eq!(
        render_fresh.ecs_tick, 102,
        "fresh acquire returns latest publish"
    );
    assert_eq!(render_fresh.checkpoint_id, 1002);
    assert!((render_fresh.editor_camera.eye.x - 3.0).abs() < f32::EPSILON);

    // Generation advanced monotonically across all 3 publishes.
    assert_eq!(handoff.generation(), 3, "3 publishes → generation = 3");
}
