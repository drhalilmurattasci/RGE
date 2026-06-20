//! `editor_ui::menus::default_menu` — the editor's canonical main-menu definition.
//!
//! [`default_editor_menu`] builds the [`MenuRegistry`] for the editor's main-menu
//! surfaces — **File / Edit / Play / View / Plugins** — as host-agnostic data:
//! the single source of truth for each menu's content, order, [`Command`], and
//! canonical executable accelerators. Play carries display-only shortcut
//! hints for its separate plain-key playback path; Plugins is declared empty so
//! extension/plugin code can register entries without the host inventing another
//! surface. It lives in `editor-ui` rather than the egui host so BOTH consumers
//! build from one definition without a reverse crate edge:
//!
//! - **`editor-egui-host`** resolves it and projects each point to the
//!   `(label, shortcut display, `[`Command`]`)` triples its menu bar paints.
//! - **`editor-shell`** (the W08 accelerator-execution work) resolves it and
//!   routes a keystroke to its bound command through
//!   [`ResolveResult::enabled_command_for_shortcut`](crate::menus::ResolveResult::enabled_command_for_shortcut).
//!   The host cannot be the binding authority (`editor-egui-host` has no edge to
//!   `editor-shell`), and the shell cannot reach into the host's private builder,
//!   so the definition belongs here — in the crate both already depend on.
//!
//! The File/Edit/View accelerator VALUES are the CANONICAL source for the live
//! `editor-shell` keystroke routing (since the W08.3 cutover + the W08.4
//! retirement of the `EditorKeyCommand` mirror): `editor-shell`'s `window_event`
//! resolves a keystroke to its `Shortcut` and routes the enabled `Command` via
//! `ResolveResult::enabled_command_for_shortcut` →
//! `EditorShell::route_menu_command`, so `Ctrl+O` / `Ctrl+S` /
//! `Ctrl+Shift+S` / `Ctrl+Z` / `Ctrl+Y` / `Ctrl+A` / `Ctrl+Shift+P` /
//! `Home` / `PageUp` / `PageDown` live ONLY here.
//! Play carries no executable accelerator — its real keys are the separate plain
//! `Space` / `Escape` PIE binds, surfaced only as passive display hints. View
//! binds `Ctrl+Shift+P` for Command Palette, `Home` for Reset Camera / Frame
//! Scene, plus `PageUp` / `PageDown` for Zoom In / Zoom Out. Every core entry uses the default section +
//! [`OrderHint::AtEnd`](crate::menus::OrderHint::AtEnd), so
//! [`MenuRegistry::resolve`] returns each point's entries in registration order.
//!
//! Resolving is the CONSUMER's call — each picks its own
//! [`PredicateContext`](crate::menus::PredicateContext); this module only builds
//! the definition.

use crate::menus::{
    Command, ExtensionPoint, Key, LabelOverride, MenuEntry, MenuRegistry, Modifiers, Predicate,
    Shortcut,
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
/// menu routes camera commands through `EditorShell::route_menu_command` via the
/// host→shell FIFO, not the PIE driver.
const VIEW_MENU_ID: &str = "editor.main_menu.view";

/// Extension-point id for the editor's main-menu **Plugins** surface. The
/// default menu declares it with no core entries; the egui host renders the
/// top-level Plugins menu only when extension/plugin code registers entries.
const PLUGINS_MENU_ID: &str = "editor.main_menu.plugins";

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

/// The **Plugins** main-menu [`ExtensionPoint`] (`editor.main_menu.plugins`).
#[must_use]
pub fn plugins_menu_point() -> ExtensionPoint {
    ExtensionPoint::new(PLUGINS_MENU_ID)
}

/// Build the editor's canonical [`MenuRegistry`] with all core main-menu
/// extension points (File + Edit + Play + View) plus the optional Plugins
/// extension point declared. The returned registry is UNRESOLVED — the consumer
/// calls
/// [`MenuRegistry::resolve`] with its own
/// [`PredicateContext`](crate::menus::PredicateContext) (the host resolves at
/// render time; `editor-shell` resolves to drive accelerator execution).
///
/// The registry is the single source of truth for all main menus' content + order:
/// - **File** = New / Open / Save / Save As New Project / Close / Quit — each with its real
///   keyboard accelerator ([`Command::NewFile`] `Ctrl+N`,
///   [`Command::OpenFile`] `Ctrl+O`, [`Command::Save`] `Ctrl+S`,
///   [`Command::SaveAs`] `Ctrl+Shift+S`, [`Command::Close`] `Ctrl+W`,
///   [`Command::Quit`] `Ctrl+Q`).
/// - **Edit** = Undo / Redo ([`Command::Undo`] `Ctrl+Z`, [`Command::Redo`]
///   `Ctrl+Y`) plus Select All ([`Command::SelectAll`] `Ctrl+A`), Cut
///   ([`Command::Cut`] `Ctrl+X`) / Copy ([`Command::Copy`] `Ctrl+C`) / Paste
///   ([`Command::Paste`] `Ctrl+V`), Delete ([`Command::Delete`] `Delete`) /
///   Duplicate ([`Command::Duplicate`] `Ctrl+D`), and Delete Current CAD Cuboid
///   ([`Command::DeleteCurrentCadCuboid`], no shortcut).
/// - **Play** = Play / Pause / Stop / Step ([`Command::PlayStart`] /
///   [`Command::PlayPause`] / [`Command::PlayStop`] / [`Command::PlayStep`]) —
///   no executable accelerator; passive display hints show the already-live
///   plain `Space` / `Escape` PIE bindings.
/// - **View** = Command Palette ([`Command::ToggleCommandPalette`]
///   `Ctrl+Shift+P`), Reset Camera / Frame Scene ([`Command::ResetCamera`] `Home`),
///   Zoom In ([`Command::ZoomIn`] `PageUp`), Zoom Out ([`Command::ZoomOut`]
///   `PageDown`).
/// - **Plugins** = declared empty; plugin code may register
///   [`Command::Plugin`] entries against [`plugins_menu_point`].
///
/// ENABLEMENT predicates (greyed-but-present, accelerator intact — distinct from
/// visibility): File New/Open/Save/Save-As/Close carry an `is_editing` predicate (they
/// no-op outside Editing); File Quit has no enablement predicate and remains
/// available while PIE is active; and each Play item a `can_play`/`can_pause`/
/// `can_stop`/`can_step` predicate keyed on the canonical `PlayState` transition
/// the consumer fills onto its [`PredicateContext`]). Edit Select All carries an
/// Editing + non-empty-world predicate; Edit Cut/Copy/Delete/Duplicate carry an
/// Editing + non-empty-selection predicate; Edit Paste carries an Editing +
/// non-empty-clipboard predicate; Edit Delete Current CAD Cuboid carries an
/// Editing + exact tracked CAD cuboid selection predicate. View is always
/// enabled.
///
/// Every entry carries the default order hint
/// ([`OrderHint::AtEnd`](crate::menus::OrderHint::AtEnd)) in the default section,
/// so `resolve` returns each point in registration order. The `expect`s are
/// unreachable: a fresh registry with five distinct ids declares and registers
/// cleanly.
#[must_use]
pub fn default_editor_menu() -> MenuRegistry {
    let mut registry = MenuRegistry::new();
    let file_point = file_menu_point();
    let edit_point = edit_menu_point();
    let play_point = play_menu_point();
    let view_point = view_menu_point();
    let plugins_point = plugins_menu_point();
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
    registry
        .declare_extension_point(plugins_point)
        .expect("static Plugins extension point declares cleanly");
    for (id, label, command, shortcut) in [
        (
            "file.new",
            "New",
            Command::NewFile,
            Shortcut::new(Modifiers::CTRL, Key::Char('N')),
        ),
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
        (
            "file.close",
            "Close",
            Command::Close,
            Shortcut::new(Modifiers::CTRL, Key::Char('W')),
        ),
    ] {
        registry
            .register_entry(
                &file_point,
                MenuEntry::new(id, label, command)
                    .with_shortcut(shortcut)
                    // New / Open / Save / Save-As / Close no-op outside the Editing state, so
                    // they grey out there (ENABLEMENT, not visibility — the item
                    // stays present and keeps its accelerator).
                    .with_enabled(Predicate::from_fn(|c| c.is_editing)),
            )
            .expect("static File menu entries register cleanly");
    }
    registry
        .register_entry(
            &file_point,
            MenuEntry::new("file.quit", "Quit", Command::Quit)
                .with_shortcut(Shortcut::new(Modifiers::CTRL, Key::Char('Q'))),
        )
        .expect("static File Quit entry registers cleanly");
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
        (
            "edit.select_all",
            "Select All",
            Command::SelectAll,
            Shortcut::new(Modifiers::CTRL, Key::Char('A')),
        ),
        (
            "edit.cut",
            "Cut",
            Command::Cut,
            Shortcut::new(Modifiers::CTRL, Key::Char('X')),
        ),
        (
            "edit.copy",
            "Copy",
            Command::Copy,
            Shortcut::new(Modifiers::CTRL, Key::Char('C')),
        ),
        (
            "edit.paste",
            "Paste",
            Command::Paste,
            Shortcut::new(Modifiers::CTRL, Key::Char('V')),
        ),
        (
            "edit.delete",
            "Delete",
            Command::Delete,
            Shortcut::plain(Key::Delete),
        ),
        (
            "edit.duplicate",
            "Duplicate",
            Command::Duplicate,
            Shortcut::new(Modifiers::CTRL, Key::Char('D')),
        ),
    ] {
        let mut entry = MenuEntry::new(id, label, command).with_shortcut(shortcut);
        if id == "edit.select_all" {
            entry = entry.with_enabled(Predicate::from_fn(|c| {
                c.is_editing && c.has_selectable_entities
            }));
        }
        if matches!(
            id,
            "edit.cut" | "edit.copy" | "edit.delete" | "edit.duplicate"
        ) {
            entry = entry.with_enabled(Predicate::from_fn(|c| c.is_editing && c.has_selection));
        }
        if id == "edit.paste" {
            entry = entry.with_enabled(Predicate::from_fn(|c| {
                c.is_editing && c.has_clipboard_entities
            }));
        }
        registry
            .register_entry(&edit_point, entry)
            .expect("static Edit menu entries register cleanly");
    }
    registry
        .register_entry(
            &edit_point,
            MenuEntry::new(
                "edit.delete_current_cad_cuboid",
                "Delete Current CAD Cuboid",
                Command::DeleteCurrentCadCuboid,
            )
            .with_enabled(Predicate::from_fn(|c| {
                c.is_editing && c.has_current_cad_cuboid_selection
            })),
        )
        .expect("static Edit Delete Current CAD Cuboid entry registers cleanly");
    // Each Play item greys out when its PIE transition is a no-op for the current
    // state — ENABLEMENT predicates keyed on the canonical `PlayState::can_*`
    // booleans (filled shell-side onto the `PredicateContext`). The items stay
    // present (greyed), not hidden.
    for (id, label, command, enabled, shortcut_hint) in [
        (
            "play.start",
            "Play",
            Command::PlayStart,
            Predicate::from_fn(|c| c.can_play),
            Some(Shortcut::plain(Key::Space)),
        ),
        (
            "play.pause",
            "Pause",
            Command::PlayPause,
            Predicate::from_fn(|c| c.can_pause),
            Some(Shortcut::plain(Key::Space)),
        ),
        (
            "play.stop",
            "Stop",
            Command::PlayStop,
            Predicate::from_fn(|c| c.can_stop),
            Some(Shortcut::plain(Key::Escape)),
        ),
        (
            "play.step",
            "Step",
            Command::PlayStep,
            Predicate::from_fn(|c| c.can_step),
            None,
        ),
    ] {
        let mut entry = MenuEntry::new(id, label, command).with_enabled(enabled);
        if id == "play.start" {
            entry = entry.with_label_override(LabelOverride::from_fn(|ctx| {
                (ctx.play_state == "paused").then(|| "Resume".to_owned())
            }));
        }
        if let Some(shortcut_hint) = shortcut_hint {
            entry = entry.with_shortcut_hint(shortcut_hint);
        }
        registry
            .register_entry(&play_point, entry)
            .expect("static Play menu entries register cleanly");
    }
    for (id, label, command, shortcut) in [
        (
            "view.command_palette",
            "Command Palette",
            Command::ToggleCommandPalette,
            Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Char('P')),
        ),
        (
            "view.reset_camera",
            "Reset Camera",
            Command::ResetCamera,
            Shortcut::plain(Key::Home),
        ),
        (
            "view.zoom_in",
            "Zoom In",
            Command::ZoomIn,
            Shortcut::plain(Key::PageUp),
        ),
        (
            "view.zoom_out",
            "Zoom Out",
            Command::ZoomOut,
            Shortcut::plain(Key::PageDown),
        ),
    ] {
        let mut entry = MenuEntry::new(id, label, command).with_shortcut(shortcut);
        if id == "view.reset_camera" {
            entry = entry.with_label_override(LabelOverride::from_fn(|ctx| {
                ctx.has_frameable_scene.then(|| "Frame Scene".to_owned())
            }));
        }
        registry
            .register_entry(&view_point, entry)
            .expect("static View menu entries register cleanly");
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::PredicateContext;

    #[test]
    fn declares_file_edit_play_view_plugins_in_order() {
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
                "editor.main_menu.plugins",
            ],
            "the canonical menu declares File / Edit / Play / View / Plugins in that order"
        );
    }

    #[test]
    fn plugins_menu_declares_empty_extension_point() {
        let registry = default_editor_menu();
        assert_eq!(
            registry.entry_count(&plugins_menu_point()),
            Some(0),
            "the canonical Plugins menu point starts empty for extension/plugin entries"
        );
        assert!(
            registry
                .resolve(&PredicateContext::default())
                .entries_for(&plugins_menu_point())
                .is_empty(),
            "an empty plugin point resolves to no entries"
        );
    }

    #[test]
    fn resolves_shared_menu_accelerators_to_their_commands() {
        let resolved = default_editor_menu().resolve(&PredicateContext::default());
        let cmd = |m, k| resolved.command_for_shortcut(&Shortcut::new(m, k)).cloned();
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('N')),
            Some(Command::NewFile),
            "Ctrl+N resolves to New"
        );
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
            cmd(Modifiers::CTRL, Key::Char('W')),
            Some(Command::Close),
            "Ctrl+W resolves to Close"
        );
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('Q')),
            Some(Command::Quit),
            "Ctrl+Q resolves to Quit"
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
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('A')),
            Some(Command::SelectAll),
            "Ctrl+A resolves to Select All"
        );
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('X')),
            Some(Command::Cut),
            "Ctrl+X resolves to Cut"
        );
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('C')),
            Some(Command::Copy),
            "Ctrl+C resolves to Copy"
        );
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('V')),
            Some(Command::Paste),
            "Ctrl+V resolves to Paste"
        );
        assert_eq!(
            cmd(Modifiers::empty(), Key::Delete),
            Some(Command::Delete),
            "Delete resolves to Delete"
        );
        assert_eq!(
            cmd(Modifiers::CTRL, Key::Char('D')),
            Some(Command::Duplicate),
            "Ctrl+D resolves to Duplicate"
        );
        assert_eq!(
            cmd(Modifiers::CTRL | Modifiers::SHIFT, Key::Char('P')),
            Some(Command::ToggleCommandPalette),
            "Ctrl+Shift+P resolves to View / Command Palette"
        );
        assert_eq!(
            cmd(Modifiers::empty(), Key::Home),
            Some(Command::ResetCamera),
            "Home resolves to View / Reset Camera"
        );
        assert_eq!(
            cmd(Modifiers::empty(), Key::PageUp),
            Some(Command::ZoomIn),
            "PageUp resolves to View / Zoom In"
        );
        assert_eq!(
            cmd(Modifiers::empty(), Key::PageDown),
            Some(Command::ZoomOut),
            "PageDown resolves to View / Zoom Out"
        );
    }

    #[test]
    fn executable_accelerators_have_no_conflicts_and_bind_exactly_eighteen() {
        let resolved = default_editor_menu().resolve(&PredicateContext::default());
        assert!(
            resolved.conflicts.is_empty(),
            "the canonical editor menu binds no shortcut twice"
        );
        assert_eq!(
            resolved.accelerator_table.len(),
            18,
            "exactly eighteen distinct accelerators: New / Open / Save / Save-As / Close / Quit / Undo / Redo / Select All / Cut / Copy / Paste / Delete / Duplicate / Command Palette / Reset Camera / Zoom In / Zoom Out"
        );
    }

    #[test]
    fn edit_delete_current_cad_cuboid_entry_has_no_shortcut() {
        let resolved = default_editor_menu().resolve(&PredicateContext::default());
        let edit = resolved.entries_for(&edit_menu_point());
        let entry = edit
            .iter()
            .find(|r| r.entry.command == Command::DeleteCurrentCadCuboid)
            .expect("Edit menu contains Delete Current CAD Cuboid");

        assert_eq!(entry.entry.id.as_str(), "edit.delete_current_cad_cuboid");
        assert_eq!(entry.entry.label, "Delete Current CAD Cuboid");
        assert!(entry.entry.shortcut.is_none());
        assert!(entry.entry.shortcut_hint.is_none());
        assert_eq!(
            edit.last().map(|r| &r.entry.command),
            Some(&Command::DeleteCurrentCadCuboid),
            "the no-shortcut CAD delete entry follows the shortcut-backed Edit entries"
        );
    }

    #[test]
    fn play_entries_carry_no_executable_accelerator() {
        let resolved = default_editor_menu().resolve(&PredicateContext::default());
        for r in resolved.entries_for(&play_menu_point()) {
            assert!(
                r.entry.shortcut.is_none(),
                "Play entries carry no executable accelerator: {}",
                r.entry.id.as_str()
            );
        }
    }

    #[test]
    fn play_entries_carry_passive_shortcut_hints_only() {
        let resolved = default_editor_menu().resolve(&PredicateContext::default());
        let hints: Vec<Option<String>> = resolved
            .entries_for(&play_menu_point())
            .iter()
            .map(|r| r.entry.shortcut_hint.as_ref().map(Shortcut::display))
            .collect();
        assert_eq!(
            hints,
            vec![
                Some("Space".to_owned()),
                Some("Space".to_owned()),
                Some("Escape".to_owned()),
                None,
            ],
            "Play shows passive hints for the existing Space toggle / Escape stop path"
        );
        assert_eq!(
            resolved.accelerator_table.len(),
            18,
            "passive Play hints must not add executable accelerator bindings"
        );
    }

    #[test]
    fn play_start_label_changes_to_resume_when_paused() {
        let mut ctx = PredicateContext::default();
        ctx.play_state = "paused".to_owned();
        let resolved = default_editor_menu().resolve(&ctx);
        let labels: Vec<&str> = resolved
            .entries_for(&play_menu_point())
            .iter()
            .map(|r| r.entry.label.as_str())
            .collect();
        assert_eq!(
            labels,
            vec!["Resume", "Pause", "Stop", "Step"],
            "paused PIE context renames the Play command to Resume"
        );
        assert_eq!(
            resolved.accelerator_table.len(),
            18,
            "dynamic labels must not add executable accelerator bindings"
        );
    }

    #[test]
    fn view_reset_label_changes_to_frame_scene_when_frameable() {
        let mut ctx = PredicateContext::default();
        ctx.has_frameable_scene = true;
        let resolved = default_editor_menu().resolve(&ctx);
        let view = resolved.entries_for(&view_menu_point());
        let reset_camera = view
            .iter()
            .find(|r| r.entry.command == Command::ResetCamera)
            .expect("View menu keeps the Reset Camera command");
        assert_eq!(
            reset_camera.entry.label, "Frame Scene",
            "View camera action names the scene-framing behavior when bounds exist"
        );
        assert_eq!(
            view.iter()
                .map(|r| (
                    r.entry.label.as_str(),
                    r.entry.command.clone(),
                    r.entry.shortcut.as_ref().map(Shortcut::display)
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "Command Palette",
                    Command::ToggleCommandPalette,
                    Some("Ctrl+Shift+P".to_owned())
                ),
                ("Frame Scene", Command::ResetCamera, Some("Home".to_owned())),
                ("Zoom In", Command::ZoomIn, Some("PageUp".to_owned())),
                ("Zoom Out", Command::ZoomOut, Some("PageDown".to_owned())),
            ],
            "dynamic labels must not change View command identities or shortcuts"
        );
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

        // Editing: File items enabled; Select All enabled only when the scene
        // has selectable entities; Cut/Copy/Delete/Duplicate enabled only when
        // an entity is selected; Paste enabled only when clipboard content exists;
        // Play (start) enabled; pause/stop/step not.
        let editing = PredicateContext {
            is_editing: true,
            can_play: true,
            has_selection: true,
            has_selectable_entities: true,
            has_clipboard_entities: true,
            has_current_cad_cuboid_selection: true,
            ..PredicateContext::default()
        };
        let res = default_editor_menu().resolve(&editing);
        assert!(enabled_of(res.entries_for(&file_menu_point()), "file.save"));
        assert!(enabled_of(res.entries_for(&file_menu_point()), "file.new"));
        assert!(enabled_of(res.entries_for(&file_menu_point()), "file.open"));
        assert!(enabled_of(
            res.entries_for(&file_menu_point()),
            "file.close"
        ));
        assert!(enabled_of(res.entries_for(&file_menu_point()), "file.quit"));
        // Edit Undo has no enablement predicate -> always on.
        assert!(enabled_of(res.entries_for(&edit_menu_point()), "edit.undo"));
        assert!(enabled_of(
            res.entries_for(&edit_menu_point()),
            "edit.select_all"
        ));
        assert!(enabled_of(res.entries_for(&edit_menu_point()), "edit.cut"));
        assert!(enabled_of(res.entries_for(&edit_menu_point()), "edit.copy"));
        assert!(enabled_of(
            res.entries_for(&edit_menu_point()),
            "edit.paste"
        ));
        assert!(enabled_of(
            res.entries_for(&edit_menu_point()),
            "edit.delete"
        ));
        assert!(enabled_of(
            res.entries_for(&edit_menu_point()),
            "edit.duplicate"
        ));
        assert!(enabled_of(
            res.entries_for(&edit_menu_point()),
            "edit.delete_current_cad_cuboid"
        ));
        let mut empty_editing = editing.clone();
        empty_editing.has_selection = false;
        empty_editing.has_selectable_entities = false;
        empty_editing.has_clipboard_entities = false;
        empty_editing.has_current_cad_cuboid_selection = false;
        let empty_res = default_editor_menu().resolve(&empty_editing);
        assert!(!enabled_of(
            empty_res.entries_for(&edit_menu_point()),
            "edit.select_all"
        ));
        assert!(!enabled_of(
            empty_res.entries_for(&edit_menu_point()),
            "edit.cut"
        ));
        assert!(!enabled_of(
            empty_res.entries_for(&edit_menu_point()),
            "edit.copy"
        ));
        assert!(!enabled_of(
            empty_res.entries_for(&edit_menu_point()),
            "edit.paste"
        ));
        assert!(!enabled_of(
            empty_res.entries_for(&edit_menu_point()),
            "edit.delete"
        ));
        assert!(!enabled_of(
            empty_res.entries_for(&edit_menu_point()),
            "edit.duplicate"
        ));
        assert!(!enabled_of(
            empty_res.entries_for(&edit_menu_point()),
            "edit.delete_current_cad_cuboid"
        ));
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
            has_selection: true,
            has_selectable_entities: true,
            has_clipboard_entities: true,
            has_current_cad_cuboid_selection: true,
            ..PredicateContext::default()
        };
        let res = default_editor_menu().resolve(&playing);
        assert!(!enabled_of(
            res.entries_for(&file_menu_point()),
            "file.save"
        ));
        assert!(!enabled_of(res.entries_for(&file_menu_point()), "file.new"));
        assert!(!enabled_of(
            res.entries_for(&file_menu_point()),
            "file.close"
        ));
        assert!(enabled_of(res.entries_for(&file_menu_point()), "file.quit"));
        let ctrl_n = Shortcut::new(Modifiers::CTRL, Key::Char('N'));
        assert_eq!(
            res.command_for_shortcut(&ctrl_n),
            Some(&Command::NewFile),
            "Ctrl+N stays bound for display while New is greyed"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&ctrl_n),
            None,
            "Ctrl+N does not fire while New is greyed"
        );
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
        let ctrl_w = Shortcut::new(Modifiers::CTRL, Key::Char('W'));
        assert_eq!(
            res.command_for_shortcut(&ctrl_w),
            Some(&Command::Close),
            "Ctrl+W stays bound for display while Close is greyed"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&ctrl_w),
            None,
            "Ctrl+W does not fire while Close is greyed"
        );
        let ctrl_q = Shortcut::new(Modifiers::CTRL, Key::Char('Q'));
        assert_eq!(
            res.command_for_shortcut(&ctrl_q),
            Some(&Command::Quit),
            "Ctrl+Q stays bound for Quit while playing"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&ctrl_q),
            Some(&Command::Quit),
            "Ctrl+Q remains enabled while playing"
        );
        let ctrl_a = Shortcut::new(Modifiers::CTRL, Key::Char('A'));
        assert_eq!(
            res.command_for_shortcut(&ctrl_a),
            Some(&Command::SelectAll),
            "Ctrl+A stays bound for display while Select All is greyed"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&ctrl_a),
            None,
            "Ctrl+A does not fire while Select All is greyed"
        );
        let ctrl_x = Shortcut::new(Modifiers::CTRL, Key::Char('X'));
        assert_eq!(
            res.command_for_shortcut(&ctrl_x),
            Some(&Command::Cut),
            "Ctrl+X stays bound for display while Cut is greyed"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&ctrl_x),
            None,
            "Ctrl+X does not fire while Cut is greyed"
        );
        let ctrl_c = Shortcut::new(Modifiers::CTRL, Key::Char('C'));
        assert_eq!(
            res.command_for_shortcut(&ctrl_c),
            Some(&Command::Copy),
            "Ctrl+C stays bound for display while Copy is greyed"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&ctrl_c),
            None,
            "Ctrl+C does not fire while Copy is greyed"
        );
        let ctrl_v = Shortcut::new(Modifiers::CTRL, Key::Char('V'));
        assert_eq!(
            res.command_for_shortcut(&ctrl_v),
            Some(&Command::Paste),
            "Ctrl+V stays bound for display while Paste is greyed"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&ctrl_v),
            None,
            "Ctrl+V does not fire while Paste is greyed"
        );
        let delete = Shortcut::plain(Key::Delete);
        assert_eq!(
            res.command_for_shortcut(&delete),
            Some(&Command::Delete),
            "Delete stays bound for display while Delete is greyed"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&delete),
            None,
            "Delete does not fire while Delete is greyed"
        );
        let duplicate = Shortcut::new(Modifiers::CTRL, Key::Char('D'));
        assert_eq!(
            res.command_for_shortcut(&duplicate),
            Some(&Command::Duplicate),
            "Ctrl+D stays bound for display while Duplicate is greyed"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&duplicate),
            None,
            "Ctrl+D does not fire while Duplicate is greyed"
        );
        assert!(!enabled_of(
            res.entries_for(&edit_menu_point()),
            "edit.delete_current_cad_cuboid"
        ));
        let command_palette = Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Char('P'));
        assert_eq!(
            res.command_for_shortcut(&command_palette),
            Some(&Command::ToggleCommandPalette),
            "Ctrl+Shift+P stays bound for Command Palette while playing"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&command_palette),
            Some(&Command::ToggleCommandPalette),
            "Command Palette remains enabled while playing"
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
