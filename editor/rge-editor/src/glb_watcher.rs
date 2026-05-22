//! Notify-backed GLB hot-reload watcher (ISSUE-85).
//!
//! When `rge-editor` is launched with `--glb <path>`, this module owns
//! a `notify` watcher rooted at the active source file's parent
//! directory and turns coalesced modify events into reload **requests**.
//! It deliberately performs no reload work itself — the binary's
//! `ApplicationHandler` wrapper drains pending requests at
//! `WindowEvent::RedrawRequested` time and calls
//! [`rge_editor_shell::EditorShell::handle_asset_reload`] for the
//! actual reload semantics (loader invocation, failure handling, warn
//! logs, render-asset swap).
//!
//! # Design
//!
//! - **Parent-directory watch (`RecursiveMode::NonRecursive`).** Many
//!   editors save via "write to sibling temp, rename over original"
//!   which causes a file-level watcher to lose its inode. Watching the
//!   parent dir non-recursively catches both straight-overwrite and
//!   rename-replace sequences without picking up subtree noise.
//!
//! - **Event filtering.** Only `notify::EventKind::Modify(_)` events
//!   whose `event.paths` contain the active `glb_source_path` are
//!   converted into pending reload intent. Create / Remove / Access /
//!   Other events, and Modify events for siblings, are ignored.
//!
//! - **Debounce.** A single user save burst can fan out into several
//!   modify notifications (truncate, write, flush, metadata). The
//!   debounce window collapses a burst into one reload by deferring
//!   the "ready" signal until ~200 ms have elapsed since the most
//!   recent matching modify.
//!
//! - **Failure posture.** Errors from the notify callback are dropped
//!   (logged-once would be reasonable; v0 stays silent because every
//!   notify error path is "watcher still live, future event will
//!   still arrive"). Construction failures (e.g. parent directory
//!   missing) bubble up to the binary, which warn-logs and proceeds
//!   without automatic reload (manual R-key still works).
//!
//! - **Producer-only.** Nothing in this module touches `EditorShell`'s
//!   render assets, mesh vectors, materials, GPU state, or the loader
//!   surface. The drain returns `bool` — the caller decides what to do
//!   with it.
//!
//! - **Path resolution for the watched parent.** `Path::parent()`
//!   returns `Some("")` (not `None`) for a bare relative filename
//!   like `asset.glb`, and `watcher.watch("")` fails on every
//!   platform. [`watch_parent_for`] normalizes that empty case to
//!   `.` so `--glb asset.glb` watches the current working
//!   directory; a genuinely empty input path still bubbles up as
//!   an error. See the function's tests for the matrix.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Debounce window for coalescing burst-write notifications into one
/// reload request. ~200 ms catches editor save sequences (truncate +
/// write + flush + metadata) without making the user-visible reload
/// feel laggy.
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(200);

/// Notify-backed parent-directory watcher for the active `--glb`
/// source. Produces reload requests only — see module docs.
pub(crate) struct GlbWatcher {
    /// Active `--glb` path. Modify events whose `event.paths` do not
    /// include this exact value are filtered out (sibling-file noise).
    glb_source_path: PathBuf,
    /// Channel side that the notify callback (or test code) sends
    /// `notify::Result<Event>` into. Drained non-blockingly by
    /// [`Self::take_reload_request`].
    rx: Receiver<notify::Result<Event>>,
    /// Debounce duration. Fixed at [`DEBOUNCE`] today; tests drive
    /// the same default through synthetic-time arithmetic on the
    /// `now` parameter of [`Self::take_reload_request`].
    debounce: Duration,
    /// Wall-clock instant of the most-recent matching modify event.
    /// `take_reload_request` only fires once `now.duration_since(this)
    /// >= debounce`. Cleared each time the request fires.
    pending_at: Option<Instant>,
    /// Real notify watcher object, kept alive for the editor lifetime
    /// because dropping it stops file-system event delivery. `None`
    /// for the test-only constructor [`Self::for_test`], where the
    /// test owns the sender side directly and no platform watcher is
    /// needed.
    _watcher: Option<RecommendedWatcher>,
}

impl GlbWatcher {
    /// Build a production watcher rooted at the given GLB path's
    /// parent directory.
    ///
    /// Bare relative names (e.g. `--glb asset.glb`) resolve to the
    /// current working directory — see [`watch_parent_for`] for the
    /// full resolution matrix.
    ///
    /// # Errors
    ///
    /// - `notify::Error` if the platform watcher cannot be constructed
    ///   or the resolved parent directory cannot be watched.
    /// - `notify::Error::generic` if the GLB path is itself empty
    ///   (no parent to resolve at all). Bare relative names like
    ///   `asset.glb` succeed by resolving to `.`.
    pub(crate) fn new(glb_source_path: PathBuf) -> notify::Result<Self> {
        let parent = watch_parent_for(&glb_source_path)?;
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            // Best-effort send; if the receiver was dropped (editor
            // shutting down), the error is intentionally discarded —
            // the watcher is a request producer, not a transport-
            // reliability primitive.
            drop(tx.send(res));
        })?;
        watcher.watch(&parent, RecursiveMode::NonRecursive)?;
        Ok(Self {
            glb_source_path,
            rx,
            debounce: DEBOUNCE,
            pending_at: None,
            _watcher: Some(watcher),
        })
    }

    /// Test-only constructor. Skips the platform watcher; the test
    /// owns the sender side and drives events directly through the
    /// returned channel.
    ///
    /// Returns the watcher and a `Sender` that tests use to inject
    /// `notify::Result<Event>` exactly as the platform callback
    /// would.
    #[cfg(test)]
    pub(crate) fn for_test(
        glb_source_path: PathBuf,
    ) -> (Self, std::sync::mpsc::Sender<notify::Result<Event>>) {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        (
            Self {
                glb_source_path,
                rx,
                debounce: DEBOUNCE,
                pending_at: None,
                _watcher: None,
            },
            tx,
        )
    }

    /// Drain pending notify events and report whether a reload
    /// request is ready to fire at `now`.
    ///
    /// "Ready" means a `Modify(_)` event targeting
    /// [`Self::glb_source_path`] was observed at some point, AND the
    /// configured debounce window has elapsed since the most recent
    /// such event. When ready, the function clears the pending mark
    /// and returns `true` — the caller is responsible for calling
    /// `EditorShell::handle_asset_reload` (or equivalent).
    ///
    /// Non-blocking: drains everything already delivered through the
    /// channel and returns immediately.
    ///
    /// Notify-level errors (channel `Err` payloads) are dropped: they
    /// do not advance debounce state and do not stop the watcher
    /// from continuing to deliver future events. The producer-only
    /// contract is preserved — this function never touches the
    /// editor's render assets.
    pub(crate) fn take_reload_request(&mut self, now: Instant) -> bool {
        while let Ok(res) = self.rx.try_recv() {
            if let Ok(event) = res {
                if event_targets_path(&event, &self.glb_source_path) {
                    self.pending_at = Some(now);
                }
            }
        }
        if let Some(at) = self.pending_at {
            if now.saturating_duration_since(at) >= self.debounce {
                self.pending_at = None;
                return true;
            }
        }
        false
    }
}

/// Pure predicate: does this notify event represent a `Modify` to the
/// active `--glb` path?
///
/// Split out so tests can exercise the filter directly without
/// constructing a `GlbWatcher` or running a debounce window.
fn event_targets_path(event: &Event, target: &Path) -> bool {
    matches!(event.kind, EventKind::Modify(_)) && event.paths.iter().any(|p| p == target)
}

/// Resolve the directory to watch for a given GLB source path.
///
/// `Path::parent()` distinguishes three cases that matter here:
///
/// | input            | `parent()`     | resolved watch dir |
/// |------------------|----------------|--------------------|
/// | `"asset.glb"`    | `Some("")`     | `.`                |
/// | `"./asset.glb"`  | `Some(".")`    | `.`                |
/// | `"a/b.glb"`      | `Some("a")`    | `a`                |
/// | `"/tmp/x.glb"`   | `Some("/tmp")` | `/tmp`             |
/// | `""`             | `None`         | error              |
///
/// The historical bug was treating the bare-filename row as "no
/// parent" — `Some("")` is *not* `None`, so the old code passed an
/// empty path to `watcher.watch(...)` which then failed on the OS
/// side and silently disabled auto-reload (the binary only
/// warn-logs construction errors). Substituting `.` for the empty
/// case makes `--glb asset.glb` watch the cwd, which is what a
/// developer running the editor from the asset directory expects.
///
/// Split out as a pure helper so tests cover the resolution matrix
/// without constructing a real platform watcher.
fn watch_parent_for(source: &Path) -> notify::Result<PathBuf> {
    let parent = source
        .parent()
        .ok_or_else(|| notify::Error::generic("--glb path has no parent directory to watch"))?;
    if parent.as_os_str().is_empty() {
        Ok(PathBuf::from("."))
    } else {
        Ok(parent.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use notify::event::{DataChange, ModifyKind, RemoveKind};

    use super::*;

    fn modify_event_for(path: &Path) -> Event {
        Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            paths: vec![path.to_path_buf()],
            attrs: Default::default(),
        }
    }

    fn remove_event_for(path: &Path) -> Event {
        Event {
            kind: EventKind::Remove(RemoveKind::File),
            paths: vec![path.to_path_buf()],
            attrs: Default::default(),
        }
    }

    #[test]
    fn event_filter_accepts_modify_on_target() {
        let p = PathBuf::from("/tmp/x.glb");
        assert!(event_targets_path(&modify_event_for(&p), &p));
    }

    #[test]
    fn event_filter_rejects_modify_on_sibling() {
        let target = PathBuf::from("/tmp/x.glb");
        let sibling = PathBuf::from("/tmp/y.glb");
        assert!(!event_targets_path(&modify_event_for(&sibling), &target));
    }

    #[test]
    fn event_filter_rejects_non_modify_on_target() {
        let p = PathBuf::from("/tmp/x.glb");
        assert!(!event_targets_path(&remove_event_for(&p), &p));
    }

    #[test]
    fn drain_returns_false_when_no_events() {
        let p = PathBuf::from("/tmp/x.glb");
        let (mut w, _tx) = GlbWatcher::for_test(p);
        let now = Instant::now();
        assert!(!w.take_reload_request(now));
    }

    #[test]
    fn drain_holds_request_during_debounce_window() {
        // The watcher anchors `pending_at` at the drain that first
        // observed the event. Production uses ~60 Hz `RedrawRequested`
        // drains, so the first drain ingests the event and sets the
        // anchor; the next drain after the debounce window fires.
        let p = PathBuf::from("/tmp/x.glb");
        let (mut w, tx) = GlbWatcher::for_test(p.clone());
        let t0 = Instant::now();
        tx.send(Ok(modify_event_for(&p))).unwrap();
        // First drain: ingests the event, anchors `pending_at = t0`.
        assert!(!w.take_reload_request(t0));
        // Mid-window drain: no new events; window not yet elapsed.
        assert!(!w.take_reload_request(t0 + DEBOUNCE / 2));
        // After the debounce window: ready.
        assert!(w.take_reload_request(t0 + DEBOUNCE));
        // Once consumed, the request does not re-fire on its own.
        assert!(!w.take_reload_request(t0 + DEBOUNCE + Duration::from_millis(50)));
    }

    #[test]
    fn drain_coalesces_burst_into_one_request() {
        let p = PathBuf::from("/tmp/x.glb");
        let (mut w, tx) = GlbWatcher::for_test(p.clone());
        let t0 = Instant::now();
        for _ in 0..8 {
            tx.send(Ok(modify_event_for(&p))).unwrap();
        }
        // First drain ingests every event in the burst at once and
        // anchors `pending_at = t0`. Even with 8 notifications, only
        // a single request fires once the debounce window settles.
        assert!(!w.take_reload_request(t0));
        assert!(w.take_reload_request(t0 + DEBOUNCE));
        // No subsequent re-fire from the same burst.
        assert!(!w.take_reload_request(t0 + DEBOUNCE + Duration::from_millis(10)));
    }

    #[test]
    fn drain_ignores_sibling_file_modify_events() {
        let target = PathBuf::from("/tmp/x.glb");
        let sibling = PathBuf::from("/tmp/y.glb");
        let (mut w, tx) = GlbWatcher::for_test(target);
        let t0 = Instant::now();
        for _ in 0..5 {
            tx.send(Ok(modify_event_for(&sibling))).unwrap();
        }
        // Ingest pass + post-window check: sibling-only burst never
        // produces a reload request.
        assert!(!w.take_reload_request(t0));
        assert!(!w.take_reload_request(t0 + DEBOUNCE));
        assert!(!w.take_reload_request(t0 + DEBOUNCE + Duration::from_secs(1)));
    }

    #[test]
    fn drain_ignores_non_modify_events_on_target() {
        let p = PathBuf::from("/tmp/x.glb");
        let (mut w, tx) = GlbWatcher::for_test(p.clone());
        let t0 = Instant::now();
        tx.send(Ok(remove_event_for(&p))).unwrap();
        assert!(!w.take_reload_request(t0));
        assert!(!w.take_reload_request(t0 + DEBOUNCE));
    }

    #[test]
    fn drain_ignores_notify_callback_errors_but_keeps_running() {
        let p = PathBuf::from("/tmp/x.glb");
        let (mut w, tx) = GlbWatcher::for_test(p.clone());
        let t0 = Instant::now();
        // Inject a synthetic notify-level error; this should NOT
        // produce a reload request and MUST NOT poison the watcher
        // (a subsequent valid modify still fires).
        tx.send(Err(notify::Error::generic("synthetic test error")))
            .unwrap();
        assert!(!w.take_reload_request(t0));
        assert!(!w.take_reload_request(t0 + DEBOUNCE));
        // A real modify arrives after the error noise — the watcher
        // still anchors and fires.
        let t1 = t0 + DEBOUNCE + Duration::from_millis(50);
        tx.send(Ok(modify_event_for(&p))).unwrap();
        assert!(!w.take_reload_request(t1));
        assert!(w.take_reload_request(t1 + DEBOUNCE));
    }

    #[test]
    fn drain_subsequent_request_after_consumed_one() {
        let p = PathBuf::from("/tmp/x.glb");
        let (mut w, tx) = GlbWatcher::for_test(p.clone());
        let t0 = Instant::now();
        // Burst 1.
        tx.send(Ok(modify_event_for(&p))).unwrap();
        assert!(!w.take_reload_request(t0));
        assert!(w.take_reload_request(t0 + DEBOUNCE));
        // Burst 2 — same watcher; must produce a fresh request.
        let t1 = t0 + DEBOUNCE + Duration::from_millis(50);
        tx.send(Ok(modify_event_for(&p))).unwrap();
        assert!(!w.take_reload_request(t1));
        assert!(w.take_reload_request(t1 + DEBOUNCE));
    }

    // -----------------------------------------------------------------
    // watch_parent_for resolution matrix — see the helper's doc-comment.
    // Pure tests; no platform watcher is constructed.

    #[test]
    fn watch_parent_for_bare_relative_resolves_to_dot() {
        // `--glb asset.glb` — the historical bug. Path::parent() yields
        // Some(""), which we normalize to "." so the cwd is watched.
        let resolved = watch_parent_for(Path::new("asset.glb"))
            .expect("bare relative path should resolve, not error");
        assert_eq!(resolved, PathBuf::from("."));
    }

    #[test]
    fn watch_parent_for_dot_slash_relative_resolves_to_dot() {
        // `./asset.glb` — Path::parent() already yields Some("."); the
        // normalization is a no-op for this case.
        let resolved =
            watch_parent_for(Path::new("./asset.glb")).expect("./relative path should resolve");
        assert_eq!(resolved, PathBuf::from("."));
    }

    #[test]
    fn watch_parent_for_nested_relative_resolves_to_parent_dir() {
        let resolved = watch_parent_for(Path::new("a/b.glb")).expect("nested path should resolve");
        assert_eq!(resolved, PathBuf::from("a"));
    }

    #[test]
    fn watch_parent_for_empty_path_errors() {
        // Path::parent() returns None for the empty path; we still
        // surface that as a hard error.
        let err = watch_parent_for(Path::new(""))
            .expect_err("empty path must not resolve to a watchable parent");
        let msg = format!("{err}");
        assert!(
            msg.contains("no parent directory"),
            "expected 'no parent directory' in error, got: {msg}"
        );
    }

    #[test]
    fn new_accepts_bare_relative_path_by_watching_cwd() {
        // End-to-end smoke: constructing `GlbWatcher::new` on a bare
        // relative name MUST succeed -- it watches the cwd (always
        // exists during a test run). Historically this returned the
        // notify error from `watcher.watch("")` and the binary
        // warn-logged + disabled automatic reload. The leaf file
        // itself does not need to exist (notify watches the parent).
        let p = PathBuf::from("rge_glb_watcher_bare_relative_smoke_target.glb");
        let result = GlbWatcher::new(p);
        assert!(
            result.is_ok(),
            "GlbWatcher::new should accept a bare relative path by watching the cwd, got Err: {:?}",
            result.err()
        );
    }
}
