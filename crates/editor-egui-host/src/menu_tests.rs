//! Unit tests for the host's main-menu wiring: that
//! [`crate::menu::project_main_menu`] resolves each extension point
//! (File / Edit / Play / View / optional Plugins) to the expected
//! `(label, shortcut display, `[`Command`]`)` list in order, that File/Edit
//! items carry their real accelerator hint while Play carries passive
//! Space/Escape hints, that shortcut conflicts project as host diagnostics, and
//! that each resolved [`Command`] round-trips through the
//! [`super::MenuCommandHandoff`] FIFO.
//!
//! Originally extracted verbatim from the inline `#[cfg(test)] mod menu_tests`
//! in `lib.rs` (EGUIHOST-TEST-EXTRACTION) — at the time a behaviour-identical
//! move that dropped `lib.rs` back under the §1.3 Rule 3 1000-line split cap.
//! MENU-SHORTCUT-DISPLAY (#304) later widened these tests to pin the File/Edit
//! accelerator display + the then-current Play/View deferral; the passive Play
//! hint slice now pins Space/Escape display without changing keyboard execution.
//! EGUIHOST-MENU-EXTRACTION moved the menu-construction code these tests target
//! into the `menu` submodule (hence the `crate::menu::` paths below), keeping
//! `lib.rs` under the cap.

use rge_editor_ui::menus::{
    default_editor_menu, file_menu_point, plugins_menu_point, Command, Key, MenuEntry, Modifiers,
    PredicateContext, Shortcut,
};

use super::MenuCommandHandoff;
use crate::menu::project_main_menu;

/// Project the canonical menu's four points to `(label, accel, command)` triples,
/// dropping the resolved `enabled` flag — these tests pin labels / commands /
/// shortcut display / order, which are context-independent. Resolved against an
/// empty context; enablement is covered by `enablement_tracks_context`.
#[allow(clippy::type_complexity)]
fn menu_entries() -> (
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
) {
    let strip = |v: Vec<(String, Option<String>, Command, bool)>| {
        v.into_iter()
            .map(|(l, a, c, _)| (l, a, c))
            .collect::<Vec<_>>()
    };
    let menu = project_main_menu(&default_editor_menu(), &PredicateContext::default());
    (
        strip(menu.file),
        strip(menu.edit),
        strip(menu.play),
        strip(menu.view),
    )
}

#[test]
fn file_menu_registry_resolves_the_authoring_loop_commands() {
    let (file, _edit, _play, _view) = menu_entries();
    assert_eq!(
        file,
        vec![
            (
                "Open…".to_owned(),
                Some("Ctrl+O".to_owned()),
                Command::OpenFile,
            ),
            ("Save".to_owned(), Some("Ctrl+S".to_owned()), Command::Save),
            (
                "Save As New Project…".to_owned(),
                Some("Ctrl+Shift+S".to_owned()),
                Command::SaveAs,
            ),
        ],
        "the MenuRegistry resolves the File menu to exactly Open / Save / \
         Save-As-new-project, in order — each with its real accelerator display"
    );
}

#[test]
fn edit_menu_registry_resolves_undo_redo_in_order() {
    let (_file, edit, _play, _view) = menu_entries();
    assert_eq!(
        edit,
        vec![
            ("Undo".to_owned(), Some("Ctrl+Z".to_owned()), Command::Undo),
            ("Redo".to_owned(), Some("Ctrl+Y".to_owned()), Command::Redo),
        ],
        "the MenuRegistry resolves the Edit menu to exactly Undo / Redo, in order \
         — each with its real accelerator display"
    );
}

#[test]
fn file_menu_entries_round_trip_through_the_handoff_in_order() {
    let (file, _edit, _play, _view) = menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, _, cmd) in file {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![Command::OpenFile, Command::Save, Command::SaveAs],
        "each resolved File item enqueues its Command; they drain FIFO"
    );
}

#[test]
fn edit_menu_entries_round_trip_through_the_handoff_in_order() {
    let (_file, edit, _play, _view) = menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, _, cmd) in edit {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![Command::Undo, Command::Redo],
        "each resolved Edit item enqueues its Command; they drain FIFO"
    );
}

#[test]
fn play_menu_registry_resolves_play_pause_stop_step_in_order() {
    let (_file, _edit, play, _view) = menu_entries();
    assert_eq!(
        play,
        vec![
            (
                "Play".to_owned(),
                Some("Space".to_owned()),
                Command::PlayStart,
            ),
            (
                "Pause".to_owned(),
                Some("Space".to_owned()),
                Command::PlayPause,
            ),
            (
                "Stop".to_owned(),
                Some("Escape".to_owned()),
                Command::PlayStop,
            ),
            ("Step".to_owned(), None, Command::PlayStep),
        ],
        "the MenuRegistry resolves the Play menu to exactly Play / Pause / Stop / \
         Step, in order — Space/Escape are passive display hints for the existing \
         playback key path, not executable menu accelerators"
    );
}

#[test]
fn play_menu_projection_uses_resume_label_when_paused() {
    let mut ctx = PredicateContext::default();
    ctx.play_state = "paused".to_owned();
    ctx.can_play = true;
    ctx.can_pause = true;
    ctx.can_stop = true;
    ctx.can_step = true;
    let play = project_main_menu(&default_editor_menu(), &ctx).play;
    let labels: Vec<&str> = play.iter().map(|(label, _, _, _)| label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["Resume", "Pause", "Stop", "Step"],
        "host projection receives the resolver-time Resume label"
    );
    assert_eq!(
        play[0].1.as_deref(),
        Some("Space"),
        "Resume keeps the same passive Space hint as Play"
    );
}

#[test]
fn play_menu_entries_round_trip_through_the_handoff_in_order() {
    let (_file, _edit, play, _view) = menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, _, cmd) in play {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![
            Command::PlayStart,
            Command::PlayPause,
            Command::PlayStop,
            Command::PlayStep,
        ],
        "each resolved Play item enqueues its Command; they drain FIFO"
    );
}

#[test]
fn view_menu_registry_resolves_reset_camera() {
    let (_file, _edit, _play, view) = menu_entries();
    assert_eq!(
        view,
        vec![(
            "Reset Camera".to_owned(),
            Some("Home".to_owned()),
            Command::ResetCamera
        )],
        "the MenuRegistry resolves the View menu to exactly Reset Camera \
         with its Home accelerator display"
    );
}

#[test]
fn view_menu_projection_uses_frame_scene_label_when_frameable() {
    let mut ctx = PredicateContext::default();
    ctx.has_frameable_scene = true;
    let view = project_main_menu(&default_editor_menu(), &ctx).view;
    assert_eq!(
        view,
        vec![(
            "Frame Scene".to_owned(),
            Some("Home".to_owned()),
            Command::ResetCamera,
            true
        )],
        "host projection receives the resolver-time scene-framing View label"
    );
}

#[test]
fn view_menu_entries_round_trip_through_the_handoff() {
    let (_file, _edit, _play, view) = menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, _, cmd) in view {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![Command::ResetCamera],
        "each resolved View item enqueues its Command; they drain FIFO"
    );
}

#[test]
fn plugins_menu_defaults_empty() {
    let menu = project_main_menu(&default_editor_menu(), &PredicateContext::default());
    assert!(
        menu.plugins.is_empty(),
        "Plugins is an optional top-level menu and starts hidden/empty"
    );
}

#[test]
fn plugins_menu_projects_registered_entries_and_round_trips() {
    let mut registry = default_editor_menu();
    let command = Command::Plugin {
        plugin_id: "com.example.mesh-audit".to_owned(),
        action_id: "open-panel".to_owned(),
    };
    registry
        .register_entry(
            &plugins_menu_point(),
            MenuEntry::new("plugin.mesh_audit.open", "Mesh Audit", command.clone()).with_shortcut(
                Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Char('P')),
            ),
        )
        .expect("synthetic plugin entry registers in the Plugins menu");

    let plugins = project_main_menu(&registry, &PredicateContext::default()).plugins;

    assert_eq!(
        plugins,
        vec![(
            "Mesh Audit".to_owned(),
            Some("Ctrl+Shift+P".to_owned()),
            command.clone(),
            true
        )],
        "registered plugin entries project into the optional Plugins menu"
    );
    let handoff = MenuCommandHandoff::new();
    for (_, _, cmd, _) in plugins {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![command],
        "a plugin menu entry enqueues its Command::Plugin unchanged"
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn enablement_tracks_context() {
    // Greying flows from the resolved `ResolvedEntry.enabled` (the 4th projected
    // element) — the canonical registry path that replaced the bespoke
    // MenuStateSnapshot / play_item_enabled channel. Each context yields a
    // distinct enablement pattern. (PredicateContext is #[non_exhaustive], so it
    // is built via default() + field assignment, not a struct literal.)
    let enabled_of = |entries: &[(String, Option<String>, Command, bool)], cmd: &Command| -> bool {
        entries
            .iter()
            .find(|(_, _, c, _)| c == cmd)
            .map(|(_, _, _, e)| *e)
            .expect("command present (enablement never filters)")
    };

    // Editing: File items + Play (start) enabled; pause/stop/step disabled.
    let mut editing = PredicateContext::default();
    editing.is_editing = true;
    editing.can_play = true;
    let menu = project_main_menu(&default_editor_menu(), &editing);
    let file = menu.file;
    let play = menu.play;
    assert!(enabled_of(&file, &Command::Save));
    assert!(enabled_of(&file, &Command::OpenFile));
    assert!(enabled_of(&play, &Command::PlayStart));
    assert!(!enabled_of(&play, &Command::PlayPause));
    assert!(!enabled_of(&play, &Command::PlayStep));

    // Playing: File items DISABLED (greyed, still present); pause/stop enabled.
    let mut playing = PredicateContext::default();
    playing.can_pause = true;
    playing.can_stop = true;
    let menu = project_main_menu(&default_editor_menu(), &playing);
    let file = menu.file;
    let play = menu.play;
    assert!(
        !enabled_of(&file, &Command::Save),
        "Save greyed while playing"
    );
    assert_eq!(
        file.len(),
        3,
        "disabled File items stay present (3), not hidden"
    );
    assert!(enabled_of(&play, &Command::PlayPause));
    assert!(enabled_of(&play, &Command::PlayStop));
    assert!(!enabled_of(&play, &Command::PlayStart));
}

#[test]
fn file_and_edit_items_carry_accelerators_play_carries_passive_hints() {
    // The shortcut-display column (middle tuple element) is sourced from each
    // resolved executable `MenuEntry.shortcut`, falling back to passive
    // `shortcut_hint`. File + Edit carry the canonical executable accelerators
    // (Ctrl+O/S/Shift+S, Ctrl+Z/Y) — the SAME definition editor-shell's live
    // keystroke routing resolves through. Play carries display-only Space/Escape
    // hints for the separate playback route; View carries executable Home.
    let (file, edit, play, view) = menu_entries();
    let accel = |entries: &[(String, Option<String>, Command)]| -> Vec<Option<String>> {
        entries.iter().map(|(_, s, _)| s.clone()).collect()
    };

    assert_eq!(
        accel(&file),
        vec![
            Some("Ctrl+O".to_owned()),
            Some("Ctrl+S".to_owned()),
            Some("Ctrl+Shift+S".to_owned()),
        ],
        "File items display Open=Ctrl+O, Save=Ctrl+S, Save-As=Ctrl+Shift+S"
    );
    assert_eq!(
        accel(&edit),
        vec![Some("Ctrl+Z".to_owned()), Some("Ctrl+Y".to_owned())],
        "Edit items display Undo=Ctrl+Z, Redo=Ctrl+Y"
    );
    assert_eq!(
        accel(&play),
        vec![
            Some("Space".to_owned()),
            Some("Space".to_owned()),
            Some("Escape".to_owned()),
            None,
        ],
        "Play items display the existing Space toggle / Escape stop keys as passive hints"
    );
    assert_eq!(
        accel(&view),
        vec![Some("Home".to_owned())],
        "View Reset Camera displays its Home accelerator"
    );
}

#[test]
fn shortcut_conflicts_project_as_host_diagnostics() {
    let mut registry = default_editor_menu();
    registry
        .register_entry(
            &file_menu_point(),
            MenuEntry::new(
                "plugin.conflict.save",
                "Plugin Save",
                Command::Custom("plugin.save".to_owned()),
            )
            .with_shortcut(Shortcut::new(Modifiers::CTRL, Key::Char('S'))),
        )
        .expect("synthetic plugin entry registers in the File menu");

    let menu = project_main_menu(&registry, &PredicateContext::default());

    assert_eq!(
        menu.conflicts.len(),
        1,
        "the host projection carries registry shortcut conflicts"
    );
    assert_eq!(menu.conflicts[0].shortcut, "Ctrl+S");
    assert_eq!(
        menu.conflicts[0].entries,
        vec!["file.save".to_owned(), "plugin.conflict.save".to_owned()],
        "conflict diagnostics preserve registration order"
    );
}
