//! `editor_ui::menus::default_menu` — the editor's canonical four-menu definition.
//!
//! [`default_editor_menu`] builds the [`MenuRegistry`] for the editor's four
//! main-menu surfaces — **File / Edit / Play / View** — as host-agnostic data:
//! the single source of truth for each menu's content, order, [`Command`], and
//! (for File/Edit) keyboard accelerator. It lives in `editor-ui` rather than the
//! egui host so BOTH consumers build from one definition without a reverse crate
//! edge:
//!
//! - **`editor-egui-host`** resolves it and projects each point to the
//!   `(label, accelerator display, `[`Command`]`)` triples its menu bar paints.
//! - **`editor-shell`** (the W08 accelerator-execution work) resolves it and
//!   routes a keystroke to its bound command through
//!   [`ResolveResult::command_for_shortcut`](crate::menus::ResolveResult::command_for_shortcut).
//!   The host cannot be the binding authority (`editor-egui-host` has no edge to
//!   `editor-shell`), and the shell cannot reach into the host's private builder,
//!   so the definition belongs here — in the crate both already depend on.
//!
//! The File/Edit accelerator VALUES are the CANONICAL source for the live
//! `editor-shell` keystroke routing (since the W08.3 cutover + the W08.4
//! retirement of the `EditorKeyCommand` mirror): `editor-shell`'s `window_event`
//! resolves a keystroke to its `Shortcut` and routes the bound `Command` via
//! `ResolveResult::command_for_shortcut` → `EditorShell::route_menu_command`, so
//! `Ctrl+O` / `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+Z` / `Ctrl+Y` live ONLY here.
//! Play/View carry no accelerator — Play's real keys are the plain `Space` /
//! `Escape` PIE binds, and `Reset Camera` has no binding. Every entry uses the
//! default section +
//! [`OrderHint::AtEnd`](crate::menus::OrderHint::AtEnd), so
//! [`MenuRegistry::resolve`] returns each point's entries in registration order.
//!
//! Resolving is the CONSUMER's call — each picks its own
//! [`PredicateContext`](crate::menus::PredicateContext); this module only builds
//! the definition.

use crate::menus::{
    Command, ExtensionPoint, Key, MenuEntry, MenuRegistry, Modifiers, Predicate, Shortcut,
};

/// Extension-point id for the editor's main-menu **File** surface. Plugins (a
/// future dispatch) register additional File entries against this same id.
const FILE_MENU_ID: &str = "editor.main_menu.file";

/// Extension-point id for the editor's main-menu **Edit** surface.
const EDIT_MENU_ID: &str = "editor.main_menu.edit";

/// Extension-point id for the editor's main-menu **Play** surface. The Play
/// menu's items route to the already-runtime-wired PIE driver
/// (`EditorShell::handle_button`) via the host→shell FIFO, not a new action.
const PLAY_MENU_ID: &str = "editor.main_menu.play";

/// Extension-point id for the editor's main-menu **View** surface. The View
/// menu's `Reset Camera` item routes to `EditorShell::reset_camera` (reframe the
/// live scene to its bounds) via the host→shell FIFO, not the PIE driver.
const VIEW_MENU_ID: &str = "editor.main_menu.view";

/// The **File** main-menu [`ExtensionPoint`] (`editor.main_menu.file`).
#[must_use]
pub fn file_menu_point() -> ExtensionPoint {
    ExtensionPoint::new(FILE_MENU_ID)
}

/// The **Edit** main-menu [`ExtensionPoint`] (`editor.main_menu.edit`).
#[must_use]
pub fn edit_menu_point() -> ExtensionPoint {
    ExtensionPoint::new(EDIT_MENU_ID)
}

/// The **Play** main-menu [`ExtensionPoint`] (`editor.main_menu.play`).
#[must_use]
pub fn play_menu_point() -> ExtensionPoint {
    ExtensionPoint::new(PLAY_MENU_ID)
}

/// The **View** main-menu [`ExtensionPoint`] (`editor.main_menu.view`).
#[must_use]
pub fn view_menu_point() -> ExtensionPoint {
    ExtensionPoint::new(VIEW_MENU_ID)
}

/// Build the editor's canonical [`MenuRegistry`] with ALL FOUR main-menu
/// extension points (File + Edit + Play + View) declared and each point's entries
/// registered, in order. The returned registry is UNRESOLVED — the consumer calls
/// [`MenuRegistry::resolve`] with its own
/// [`PredicateContext`](crate::menus::PredicateContext) (the host resolves once at
/// construction; `editor-shell` resolves to drive accelerator execution).
///
/// The registry is the single source of truth for all four menus' content + order:
/// - **File** = Open / Save / Save As New Project — each with its real keyboard
///   accelerator ([`Command::OpenFile`] `Ctrl+O`, [`Command::Save`] `Ctrl+S`,
///   [`Command::SaveAs`] `Ctrl+Shift+S`).
/// - **Edit** = Undo / Redo ([`Command::Undo`] `Ctrl+Z`, [`Command::Redo`]
///   `Ctrl+Y`).
/// - **Play** = Play / Pause / Stop / Step ([`Command::PlayStart`] /
///   [`Command::PlayPause`] / [`Command::PlayStop`] / [`Command::PlayStep`]) — no
///   accelerator (the live keys are the plain `Space` / `Escape` PIE binds).
/// - **View** = Reset Camera ([`Command::ResetCamera`]) — no accelerator.
///
/// ENABLEMENT predicates (greyed-but-present, accelerator intact — distinct from
/// visibility): File Save/Open/Save-As carry an `is_editing` predicate (they
/// no-op outside Editing), and each Play item a `can_play`/`can_pause`/`can_stop`/
/// `can_step` predicate keyed on the canonical `PlayState` transition the
/// consumer fills onto its [`PredicateContext`]. Edit / View are always enabled.
///
/// Every entry carries the default order hint
/// ([`OrderHint::AtEnd`](crate::menus::OrderHint::AtEnd)) in the default section,
/// so `resolve` returns each point in registration order. The `expect`s are
/// unreachable: a fresh registry with four distinct ids declares and registers
/// cleanly.
#[must_use]
pub fn default_editor_menu() -> MenuRegistry {
    let mut registry = MenuRegistry::new();
    let file_point = file_menu_point();
    let edit_point = edit_menu_point();
    let play_point = play_menu_point();
    let view_point = view_menu_point();
    registry
        .declare_extension_point(file_point.clone())
        .expect("static File extension point declares cleanly");
    registry
        .declare_extension_point(edit_point.clone())
        .expect("static Edit extension point declares cleanly");
    registry
        .declare_extension_point(play_point.clone())
        .expect("static Play extension point declares cleanly");
    registry
        .declare_extension_point(view_point.clone())
        .expect("static View extension point declares cleanly");
    for (id, label, command, shortcut) in [
        (
            "file.open",
            "Open…",
            Command::OpenFile,
            Shortcut::new(Modifiers::CTRL, Key::Char('O')),
        ),
        (
            "file.save",
            "Save",
            Command::Save,
            Shortcut::new(Modifiers::CTRL, Key::Char('S')),
        ),
        (
            "file.save_as",
            "Save As New Project…",
            Command::SaveAs,
            Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Char('S')),
        ),
    ] {
        registry
            .register_entry(
                &file_point,
                MenuEntry::new(id, label, command)
                    .with_shortcut(shortcut)
                    // Save / Open / Save-As no-op outside the Editing state, so
                    // they grey out there (ENABLEMENT, not visibility — the item
                    // stays present and keeps its accelerator).
                    .with_enabled(Predicate::from_fn(|c| c.is_editing)),
            )
            .expect("static File menu entries register cleanly");
    }
    for (id, label, command, shortcut) in [
        (
            "edit.undo",
            "Undo",
            Command::Undo,
            Shortcut::new(Modifiers::CTRL, Key::Char('Z')),
        ),
        (
            "edit.redo",
            "Redo",
            Command::Redo,
            Shortcut::new(Modifiers::CTRL, Key::Char('Y')),
        ),
    ] {
        registry
            .register_entry(
                &edit_point,
                MenuEntry::new(id, label, command).with_shortcut(shortcut),
            )
            .expect("static Edit menu entries register cleanly");
    }
    // Each Play item greys out when its PIE transition is a no-op for the current
    // state — ENABLEMENT predicates keyed on the canonical `PlayState::can_*`
    // booleans (filled shell-side onto the `PredicateContext`). The items stay
    // present (greyed), not hidden.
    for (id, label, command, enabled) in [
        (
            "play.start",
            "Play",
            Command::PlayStart,
            Predicate::from_fn(|c| c.can_play),
        ),
        (
            "play.pause",
            "Pause",
            Command::PlayPause,
            Predicate::from_fn(|c| c.can_pause),
        ),
        (
            "play.stop",
            "Stop",
            Command::PlayStop,
            Predicate::from_fn(|c| c.can_stop),
        ),
        (
            "play.step",
            "Step",
            Command::PlayStep,
            Predicate::from_fn(|c| c.can_step),
        ),
    ] {
        registry
            .register_entry(
                &play_point,
                MenuEntry::new(id, label, command).with_enabled(enabled),
            )
            .expect("static Play menu entries register cleanly");
    }
    registry
        .register_entry(
            &view_point,
            MenuEntry::new("view.reset_camera", "Reset Camera", Command::ResetCamera),
        )
        .expect("static View menu entries register cleanly");
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::PredicateContext;

    #[test]
    fn declares_file_edit_play_view_in_order() {
        let registry = default_editor_menu();
        let points: Vec<&str> = registry
            .extension_points()
            .map(ExtensionPoint::as_str)
            .collect();
        assert_eq!(
            points,
            vec![
                "editor.main_menu.file",
                "editor.main_menu.edit",
                "editor.main_menu.play",
                "editor.main_menu.view",
            ],
            "the canonical menu declares File / Edit / Play / View in that order"
        );
    }

    #[test]
    fn resolves_the_five_shared_accelerators_to_their_commands() {
        let resolved = default_editor_menu().resolve(&PredicateContext::default());
        let cmd = |m, k| resolved.command_for_shortcut(&Shortcut::new(m, k)).cloned();
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('O')),
            Some(Command::OpenFile),
            "Ctrl+O resolves to Open"
        );
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('S')),
            Some(Command::Save),
            "Ctrl+S resolves to Save"
        );
        assert_eq!(
            cmd(Modifiers::CTRL | Modifiers::SHIFT, Key::Char('S')),
            Some(Command::SaveAs),
            "Ctrl+Shift+S resolves to Save-As"
        );
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('Z')),
            Some(Command::Undo),
            "Ctrl+Z resolves to Undo"
        );
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('Y')),
            Some(Command::Redo),
            "Ctrl+Y resolves to Redo"
        );
    }

    #[test]
    fn has_no_shortcut_conflicts_and_binds_exactly_five() {
        let resolved = default_editor_menu().resolve(&PredicateContext::default());
        assert!(
            resolved.conflicts.is_empty(),
            "the canonical editor menu binds no shortcut twice"
        );
        assert_eq!(
            resolved.accelerator_table.len(),
            5,
            "exactly five distinct accelerators: Open / Save / Save-As / Undo / Redo"
        );
    }

    #[test]
    fn play_and_view_entries_carry_no_accelerator() {
        let resolved = default_editor_menu().resolve(&PredicateContext::default());
        for point in [play_menu_point(), view_menu_point()] {
            for r in resolved.entries_for(&point) {
                assert!(
                    r.entry.shortcut.is_none(),
                    "Play/View entries carry no accelerator: {}",
                    r.entry.id.as_str()
                );
            }
        }
    }

    #[test]
    fn enablement_predicates_track_context() {
        use crate::menus::ResolvedEntry;
        let enabled_of = |entries: &[ResolvedEntry], id: &str| -> bool {
            entries
                .iter()
                .find(|r| r.entry.id.as_str() == id)
                .map(|r| r.enabled)
                .expect("entry present (enablement never filters)")
        };

        // Editing: File items enabled; Play (start) enabled; pause/stop/step not.
        let editing = PredicateContext {
            is_editing: true,
            can_play: true,
            ..PredicateContext::default()
        };
        let res = default_editor_menu().resolve(&editing);
        assert!(enabled_of(res.entries_for(&file_menu_point()), "file.save"));
        assert!(enabled_of(res.entries_for(&file_menu_point()), "file.open"));
        // Edit Undo has no enablement predicate -> always on.
        assert!(enabled_of(res.entries_for(&edit_menu_point()), "edit.undo"));
        assert!(enabled_of(
            res.entries_for(&play_menu_point()),
            "play.start"
        ));
        assert!(!enabled_of(
            res.entries_for(&play_menu_point()),
            "play.pause"
        ));
        assert!(!enabled_of(
            res.entries_for(&play_menu_point()),
            "play.step"
        ));

        // Playing: File items PRESENT but greyed; Ctrl+S still BOUND (display) yet
        // the enabled-only resolver withholds it; pause/stop enabled, start not.
        let playing = PredicateContext {
            is_editing: false,
            can_pause: true,
            can_stop: true,
            ..PredicateContext::default()
        };
        let res = default_editor_menu().resolve(&playing);
        assert!(!enabled_of(
            res.entries_for(&file_menu_point()),
            "file.save"
        ));
        let ctrl_s = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
        assert_eq!(
            res.command_for_shortcut(&ctrl_s),
            Some(&Command::Save),
            "Ctrl+S stays bound for display while Save is greyed"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&ctrl_s),
            None,
            "Ctrl+S does not fire while Save is greyed"
        );
        assert!(enabled_of(
            res.entries_for(&play_menu_point()),
            "play.pause"
        ));
        assert!(enabled_of(res.entries_for(&play_menu_point()), "play.stop"));
        assert!(!enabled_of(
            res.entries_for(&play_menu_point()),
            "play.start"
        ));
    }
}
