//! Phase 9 dispatch C — host-owned dock-tab bodies + the
//! `egui_dock::TabViewer` implementation that dispatches per-variant
//! rendering.
//!
//! # Why a fresh `TabBody` enum (not reusing `editor-ui::PlaceholderTabBody`)
//!
//! `editor-ui::dock::PlaceholderTabBody` is the v0.0.1 placeholder body
//! returned by the legacy `SpawnerRegistry` from W10. It carries a
//! `TabId` + `title: String` and nothing else — it cannot embed a
//! snapshot handle (no Arc), and the SpawnerRegistry is generic over
//! `TabBody`, expecting the consumer to define its own enum when it
//! needs to plug real per-tab data in. Dispatch C is exactly that
//! consumer-defined enum: variants carry the host-side data each tab
//! needs (`Arc<InspectorHandoff>` for `Inspector`; a `title: String`
//! for `Placeholder`). The two patterns coexist; this dispatch does
//! NOT touch `editor-ui::dock`.
//!
//! # No reflection
//!
//! Per the §1.1 reflection-scale preflight (`plans/BASELINE.md`): zero
//! production reflected types today. The TabViewer dispatch is a flat
//! `match` over a 2-variant enum; the inspector tab calls
//! `rge_editor_ui::widgets::inspector::ui(&snapshot, ui)` directly
//! without any registry / reflection / type-id lookup.

use std::sync::Arc;

use egui_dock::TabViewer;
use rge_editor_state::InspectorSnapshot;

use crate::handoff::InspectorHandoff;

// ---------------------------------------------------------------------------
// InspectorTabBody
// ---------------------------------------------------------------------------

/// Dock-tab body for the live editor inspector.
///
/// Holds an `Arc<InspectorHandoff>` shared with the editor-shell
/// publisher; on each [`crate::EguiHost::render`] the [`EditorTabViewer`]
/// acquires the most-recent snapshot and renders the
/// `rge_editor_ui::widgets::inspector::ui(&snap, ui)` widget against it.
///
/// # No interior state
///
/// The body itself is just an `Arc` clone — there is no per-tab egui
/// widget state (no scroll position, no filter text, no toggle flags
/// yet). Future enhancements (filter / pin / collapse) can grow this
/// struct without touching the handoff substrate.
///
/// # Empty-state behavior
///
/// If the handoff has not yet been published to (e.g. before the first
/// `tick_redraw` runs after EguiHost construction), [`Self::ui`]
/// renders the [`InspectorSnapshot::default()`] state — zero ticks,
/// `"Editing"` play state, `1.0x` time scale. The tab is therefore
/// visible from frame 1 of the host's lifetime without any flicker /
/// "no data" placeholder.
#[derive(Debug, Clone)]
pub struct InspectorTabBody {
    handoff: Arc<InspectorHandoff>,
}

impl InspectorTabBody {
    /// Construct an [`InspectorTabBody`] over the given handoff. The
    /// handoff is cloned into the body; the same handoff `Arc` is also
    /// retained by [`crate::EguiHost`] so the editor-shell publisher
    /// path can reach it via [`crate::EguiHost::inspector_handoff`].
    #[must_use]
    pub fn new(handoff: Arc<InspectorHandoff>) -> Self {
        Self { handoff }
    }

    /// Borrow the handoff this body reads from. Exposed primarily for
    /// tests that assert publish/acquire shape end-to-end across the
    /// body+handoff seam.
    #[must_use]
    pub fn handoff(&self) -> &Arc<InspectorHandoff> {
        &self.handoff
    }
}

// ---------------------------------------------------------------------------
// TabBody
// ---------------------------------------------------------------------------

/// Variant tag for the host's dock tabs.
///
/// v0 has two variants:
///
/// - `Inspector` — wraps an [`InspectorTabBody`] that reads from the
///   shared [`InspectorHandoff`].
/// - `Placeholder` — a static-label body for tabs that haven't grown
///   real content yet (none today, but ready for menu-driven tab
///   spawning in a later dispatch).
///
/// Future dispatches add variants (`Viewport`, `AssetBrowser`,
/// `NodeGraph`, `Console`) as those tabs grow real content. Each
/// variant carries the per-tab data the [`EditorTabViewer`] needs to
/// render it.
#[derive(Debug, Clone)]
pub enum TabBody {
    /// Live editor-session inspector. Holds the handoff `Arc` shared
    /// with the editor-shell publisher.
    Inspector(InspectorTabBody),

    /// Static-label placeholder. Renders a single `ui.label(title)` row;
    /// not yet wired by the default dock layout but reserved as the
    /// substrate-honest "blank tab" for menu-driven tab spawning in a
    /// future dispatch.
    Placeholder {
        /// Display title shown in both the tab bar and as the body's
        /// label row.
        title: String,
    },
}

impl TabBody {
    /// Title shown in the dock tab bar. Stable across renders for a
    /// given variant content — `Inspector` always returns `"Inspector"`;
    /// `Placeholder` returns the carried `title` string.
    #[must_use]
    pub fn title(&self) -> &str {
        match self {
            TabBody::Inspector(_) => "Inspector",
            TabBody::Placeholder { title } => title.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// EditorTabViewer
// ---------------------------------------------------------------------------

/// The unit `TabViewer` shipped by this crate. Pure dispatch over
/// [`TabBody`] variants — no widget state, no per-tab caches.
///
/// `egui_dock::DockArea::show_inside(ui, &mut viewer)` needs an instance
/// of a [`TabViewer`] each call; this struct is constructed inline at
/// the call site (`let mut viewer = EditorTabViewer;`) since it has no
/// fields.
#[derive(Debug, Default, Clone, Copy)]
pub struct EditorTabViewer;

impl TabViewer for EditorTabViewer {
    type Tab = TabBody;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            TabBody::Inspector(body) => {
                // Acquire the most-recently-published snapshot. If none
                // has been published yet, render the default state so
                // the tab is visible from frame 1 (no flicker, no
                // panic).
                if let Some(snapshot_arc) = body.handoff.acquire() {
                    rge_editor_ui::widgets::inspector::ui(snapshot_arc.as_ref(), ui);
                } else {
                    let empty = InspectorSnapshot::default();
                    rge_editor_ui::widgets::inspector::ui(&empty, ui);
                }
            }
            TabBody::Placeholder { title } => {
                ui.label(title.as_str());
            }
        }
    }

    fn is_closeable(&self, _tab: &Self::Tab) -> bool {
        // v0 the Inspector tab is always present and not closeable so
        // the user cannot end up in a "no inspector visible" state
        // without a way to spawn one back (we have no menu-driven
        // tab-spawn yet). Placeholder is symmetric — once the menu
        // dispatch lands a future revision can flip this per-variant.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_tab_body_title_is_inspector() {
        let handoff = Arc::new(InspectorHandoff::new());
        let body = TabBody::Inspector(InspectorTabBody::new(Arc::clone(&handoff)));
        assert_eq!(body.title(), "Inspector");
    }

    #[test]
    fn placeholder_tab_body_carries_title() {
        let body = TabBody::Placeholder {
            title: "Scratch".to_string(),
        };
        assert_eq!(body.title(), "Scratch");
    }

    #[test]
    fn inspector_body_handoff_accessor_returns_same_arc() {
        let handoff = Arc::new(InspectorHandoff::new());
        let body = InspectorTabBody::new(Arc::clone(&handoff));
        assert!(
            Arc::ptr_eq(body.handoff(), &handoff),
            "handoff() must return the same Arc that was passed to new()"
        );
    }

    #[test]
    fn inspector_body_observes_publish_through_handoff() {
        // End-to-end: construct body over a handoff, publish a snapshot
        // through that handoff's external Arc, then acquire through
        // the body's internal Arc — both must see the same snapshot.
        let handoff = Arc::new(InspectorHandoff::new());
        let body = InspectorTabBody::new(Arc::clone(&handoff));

        let mut snap = InspectorSnapshot::default();
        snap.tick_count = 123;
        handoff.publish(Arc::new(snap));

        let observed = body
            .handoff()
            .acquire()
            .expect("body must observe published snapshot via shared handoff");
        assert_eq!(observed.tick_count, 123);
    }

    #[test]
    fn editor_tab_viewer_title_dispatches_per_variant() {
        let handoff = Arc::new(InspectorHandoff::new());
        let mut inspector_tab = TabBody::Inspector(InspectorTabBody::new(handoff));
        let mut placeholder_tab = TabBody::Placeholder {
            title: "Foo".to_string(),
        };

        let mut viewer = EditorTabViewer;
        assert_eq!(viewer.title(&mut inspector_tab).text(), "Inspector");
        assert_eq!(viewer.title(&mut placeholder_tab).text(), "Foo");
    }

    #[test]
    fn editor_tab_viewer_is_not_closeable() {
        // v0 invariant — no menu-driven tab respawn yet.
        let handoff = Arc::new(InspectorHandoff::new());
        let inspector_tab = TabBody::Inspector(InspectorTabBody::new(handoff));
        let placeholder_tab = TabBody::Placeholder {
            title: "Foo".to_string(),
        };
        let viewer = EditorTabViewer;
        assert!(!viewer.is_closeable(&inspector_tab));
        assert!(!viewer.is_closeable(&placeholder_tab));
    }

    #[test]
    fn tab_body_is_send_static() {
        // Required so DockState<TabBody> can be stored in EguiHost
        // alongside the existing Send+'static requirements.
        fn assert_send_static<T: Send + 'static>() {}
        assert_send_static::<TabBody>();
        assert_send_static::<InspectorTabBody>();
    }
}
