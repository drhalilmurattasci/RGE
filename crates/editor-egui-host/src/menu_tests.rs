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
    default_editor_menu, file_menu_point, Command, Key, MenuEntry, Modifiers, PredicateContext,
    RegistryError, Shortcut,
};

use super::MenuCommandHandoff;
use crate::menu::{
    command_palette_entries, filter_command_palette_entries, first_enabled_command_palette_entry,
    project_main_menu, register_menu_entry, register_plugin_menu_entry,
    ProjectedCommandPaletteEntry,
};

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
                "New".to_owned(),
                Some("Ctrl+N".to_owned()),
                Command::NewFile
            ),
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
            (
                "Close".to_owned(),
                Some("Ctrl+W".to_owned()),
                Command::Close,
            ),
            ("Quit".to_owned(), Some("Ctrl+Q".to_owned()), Command::Quit,),
        ],
        "the MenuRegistry resolves the File menu to exactly New / Open / Save / \
         Save-As-new-project / Close / Quit, in order — each with its real accelerator display"
    );
}

#[test]
fn edit_menu_registry_resolves_undo_redo_select_all_cut_copy_paste_delete_duplicate_in_order() {
    let (_file, edit, _play, _view) = menu_entries();
    assert_eq!(
        edit,
        vec![
            ("Undo".to_owned(), Some("Ctrl+Z".to_owned()), Command::Undo),
            ("Redo".to_owned(), Some("Ctrl+Y".to_owned()), Command::Redo),
            (
                "Select All".to_owned(),
                Some("Ctrl+A".to_owned()),
                Command::SelectAll,
            ),
            (
                "Cut".to_owned(),
                Some("Ctrl+X".to_owned()),
                Command::Cut,
            ),
            (
                "Copy".to_owned(),
                Some("Ctrl+C".to_owned()),
                Command::Copy,
            ),
            (
                "Paste".to_owned(),
                Some("Ctrl+V".to_owned()),
                Command::Paste,
            ),
            (
                "Delete".to_owned(),
                Some("Delete".to_owned()),
                Command::Delete,
            ),
            (
                "Duplicate".to_owned(),
                Some("Ctrl+D".to_owned()),
                Command::Duplicate,
            ),
        ],
        "the MenuRegistry resolves the Edit menu to exactly Undo / Redo / Select All / Cut / Copy / Paste / Delete / Duplicate, in order \
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
        vec![
            Command::NewFile,
            Command::OpenFile,
            Command::Save,
            Command::SaveAs,
            Command::Close,
            Command::Quit,
        ],
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
        vec![
            Command::Undo,
            Command::Redo,
            Command::SelectAll,
            Command::Cut,
            Command::Copy,
            Command::Paste,
            Command::Delete,
            Command::Duplicate,
        ],
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
fn view_menu_registry_resolves_command_palette_and_camera_commands() {
    let (_file, _edit, _play, view) = menu_entries();
    assert_eq!(
        view,
        vec![
            (
                "Command Palette".to_owned(),
                Some("Ctrl+Shift+P".to_owned()),
                Command::ToggleCommandPalette
            ),
            (
                "Reset Camera".to_owned(),
                Some("Home".to_owned()),
                Command::ResetCamera
            ),
            (
                "Zoom In".to_owned(),
                Some("PageUp".to_owned()),
                Command::ZoomIn
            ),
            (
                "Zoom Out".to_owned(),
                Some("PageDown".to_owned()),
                Command::ZoomOut
            ),
        ],
        "the MenuRegistry resolves the View menu to Command Palette / Reset Camera / Zoom In / \
         Zoom Out with accelerator display"
    );
}

#[test]
fn view_menu_projection_uses_frame_scene_label_when_frameable() {
    let mut ctx = PredicateContext::default();
    ctx.has_frameable_scene = true;
    let view = project_main_menu(&default_editor_menu(), &ctx).view;
    assert_eq!(
        view,
        vec![
            (
                "Command Palette".to_owned(),
                Some("Ctrl+Shift+P".to_owned()),
                Command::ToggleCommandPalette,
                true
            ),
            (
                "Frame Scene".to_owned(),
                Some("Home".to_owned()),
                Command::ResetCamera,
                true
            ),
            (
                "Zoom In".to_owned(),
                Some("PageUp".to_owned()),
                Command::ZoomIn,
                true
            ),
            (
                "Zoom Out".to_owned(),
                Some("PageDown".to_owned()),
                Command::ZoomOut,
                true
            ),
        ],
        "host projection receives the resolver-time scene-framing View label \
         while keeping the Zoom entries unchanged"
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
        vec![
            Command::ToggleCommandPalette,
            Command::ResetCamera,
            Command::ZoomIn,
            Command::ZoomOut,
        ],
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
fn extension_menu_registration_projects_declared_main_menu_points() {
    let mut registry = default_editor_menu();
    let command = Command::Custom("plugin.export.scene".to_owned());

    register_menu_entry(
        &mut registry,
        &file_menu_point(),
        MenuEntry::new("plugin.export.scene", "Export Scene", command.clone()),
    )
    .expect("extension entry registers in a declared File menu point");

    let file = project_main_menu(&registry, &PredicateContext::default()).file;

    assert!(
        file.iter().any(
            |(label, shortcut, projected_command, enabled)| label == "Export Scene"
                && shortcut.is_none()
                && projected_command == &command
                && *enabled
        ),
        "extension entries registered outside Plugins project through the host menu surface"
    );
}

#[test]
fn plugins_menu_projects_registered_entries_and_round_trips() {
    let mut registry = default_editor_menu();
    let command = Command::Plugin {
        plugin_id: "com.example.mesh-audit".to_owned(),
        action_id: "open-panel".to_owned(),
    };
    register_plugin_menu_entry(
        &mut registry,
        MenuEntry::new("plugin.mesh_audit.open", "Mesh Audit", command.clone()).with_shortcut(
            Shortcut::new(Modifiers::CTRL | Modifiers::ALT, Key::Char('M')),
        ),
    )
    .expect("synthetic plugin entry registers in the Plugins menu");

    let plugins = project_main_menu(&registry, &PredicateContext::default()).plugins;

    assert_eq!(
        plugins,
        vec![(
            "Mesh Audit".to_owned(),
            Some("Ctrl+Alt+M".to_owned()),
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
fn command_palette_entries_flatten_current_menu_projection() {
    let mut registry = default_editor_menu();
    let plugin_command = Command::Plugin {
        plugin_id: "com.example.mesh-audit".to_owned(),
        action_id: "open-panel".to_owned(),
    };
    register_plugin_menu_entry(
        &mut registry,
        MenuEntry::new(
            "plugin.mesh_audit.open",
            "Mesh Audit",
            plugin_command.clone(),
        )
        .with_shortcut(Shortcut::new(
            Modifiers::CTRL | Modifiers::ALT,
            Key::Char('M'),
        )),
    )
    .expect("synthetic plugin entry registers in the Plugins menu");

    let mut ctx = PredicateContext::default();
    ctx.is_editing = true;
    ctx.can_play = true;
    let main_menu = project_main_menu(&registry, &ctx);
    let palette = command_palette_entries(&main_menu);

    assert!(
        palette.iter().any(|entry| entry.label == "File: Save"
            && entry.shortcut.as_deref() == Some("Ctrl+S")
            && entry.command == Command::Save
            && entry.enabled),
        "palette carries enabled core menu commands with menu-path labels"
    );
    assert!(
        palette
            .iter()
            .any(|entry| entry.label == "Plugins: Mesh Audit"
                && entry.shortcut.as_deref() == Some("Ctrl+Alt+M")
                && entry.command == plugin_command
                && entry.enabled),
        "palette includes optional Plugins entries from the same projection"
    );
}

#[test]
fn command_palette_entries_preserve_disabled_state() {
    let mut ctx = PredicateContext::default();
    ctx.can_pause = true;
    ctx.can_stop = true;
    let main_menu = project_main_menu(&default_editor_menu(), &ctx);
    let palette = command_palette_entries(&main_menu);

    let save = palette
        .iter()
        .find(|entry| entry.command == Command::Save)
        .expect("Save is present even when disabled");
    assert_eq!(save.label, "File: Save");
    assert!(!save.enabled, "palette preserves menu enablement");
}

#[test]
fn command_palette_filter_matches_label_shortcut_and_command_id() {
    let main_menu = project_main_menu(&default_editor_menu(), &PredicateContext::default());
    let palette = command_palette_entries(&main_menu);
    let labels = |filter: &str| -> Vec<String> {
        filter_command_palette_entries(&palette, filter)
            .into_iter()
            .map(|entry| entry.label.clone())
            .collect()
    };

    assert_eq!(labels("  "), labels(""));
    assert_eq!(
        labels("ctrl+shift+p"),
        vec!["View: Command Palette".to_owned()]
    );
    assert_eq!(
        labels("toggle_command_palette"),
        vec!["View: Command Palette".to_owned()],
        "command diagnostic ids are searchable"
    );
    assert_eq!(
        labels("view palette"),
        vec!["View: Command Palette".to_owned()],
        "all whitespace-separated terms must match an entry"
    );
    assert!(
        labels("not-a-real-command").is_empty(),
        "unknown filters produce an empty palette list"
    );
}

#[test]
fn command_palette_filter_orders_exact_word_matches_before_longer_matches() {
    let entries = vec![
        ProjectedCommandPaletteEntry {
            label: "File: Save As New Project".to_owned(),
            shortcut: Some("Ctrl+Shift+S".to_owned()),
            command: Command::SaveAs,
            enabled: true,
        },
        ProjectedCommandPaletteEntry {
            label: "File: Save".to_owned(),
            shortcut: Some("Ctrl+S".to_owned()),
            command: Command::Save,
            enabled: true,
        },
        ProjectedCommandPaletteEntry {
            label: "File: Open".to_owned(),
            shortcut: Some("Ctrl+O".to_owned()),
            command: Command::OpenFile,
            enabled: true,
        },
    ];

    let labels: Vec<&str> = filter_command_palette_entries(&entries, "save")
        .into_iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec!["File: Save", "File: Save As New Project"],
        "shorter exact word matches sort ahead of longer exact word matches"
    );
}

#[test]
fn command_palette_enter_activation_uses_first_enabled_filtered_entry() {
    let entries = vec![
        ProjectedCommandPaletteEntry {
            label: "File: Save".to_owned(),
            shortcut: Some("Ctrl+S".to_owned()),
            command: Command::Save,
            enabled: false,
        },
        ProjectedCommandPaletteEntry {
            label: "View: Command Palette".to_owned(),
            shortcut: Some("Ctrl+Shift+P".to_owned()),
            command: Command::ToggleCommandPalette,
            enabled: true,
        },
    ];
    let filtered_entries: Vec<&ProjectedCommandPaletteEntry> = entries.iter().collect();

    assert_eq!(
        first_enabled_command_palette_entry(&filtered_entries),
        Some(Command::ToggleCommandPalette),
        "Enter skips disabled palette rows and activates the first enabled command"
    );
}

#[test]
fn command_palette_enter_activation_returns_none_when_every_match_is_disabled() {
    let entries = vec![ProjectedCommandPaletteEntry {
        label: "File: Save".to_owned(),
        shortcut: Some("Ctrl+S".to_owned()),
        command: Command::Save,
        enabled: false,
    }];
    let filtered_entries: Vec<&ProjectedCommandPaletteEntry> = entries.iter().collect();

    assert_eq!(
        first_enabled_command_palette_entry(&filtered_entries),
        None,
        "Enter does not dispatch disabled-only palette results"
    );
}

#[test]
fn plugins_menu_registration_rejects_duplicate_entry_ids() {
    let mut registry = default_editor_menu();
    let command = Command::Plugin {
        plugin_id: "com.example.mesh-audit".to_owned(),
        action_id: "open-panel".to_owned(),
    };
    let entry = MenuEntry::new("plugin.mesh_audit.open", "Mesh Audit", command);

    register_plugin_menu_entry(&mut registry, entry.clone())
        .expect("first plugin menu entry registers");
    let err = register_plugin_menu_entry(&mut registry, entry)
        .expect_err("duplicate plugin menu entry id is rejected");

    assert_eq!(
        err,
        RegistryError::DuplicateEntryId(
            "editor.main_menu.plugins".to_owned(),
            "plugin.mesh_audit.open".to_owned()
        ),
        "duplicate rejection reports the Plugins point and entry id"
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

    // Editing: File items + Select All + Delete + Duplicate + Play (start) enabled; pause/stop/step disabled.
    let mut editing = PredicateContext::default();
    editing.is_editing = true;
    editing.can_play = true;
    editing.has_selection = true;
    editing.has_selectable_entities = true;
    editing.has_clipboard_entities = true;
    let menu = project_main_menu(&default_editor_menu(), &editing);
    let file = menu.file;
    let edit = menu.edit;
    let play = menu.play;
    assert!(enabled_of(&file, &Command::NewFile));
    assert!(enabled_of(&file, &Command::Save));
    assert!(enabled_of(&file, &Command::OpenFile));
    assert!(enabled_of(&file, &Command::Close));
    assert!(enabled_of(&file, &Command::Quit));
    assert!(enabled_of(&edit, &Command::SelectAll));
    assert!(enabled_of(&edit, &Command::Cut));
    assert!(enabled_of(&edit, &Command::Copy));
    assert!(enabled_of(&edit, &Command::Paste));
    assert!(enabled_of(&edit, &Command::Delete));
    assert!(enabled_of(&edit, &Command::Duplicate));
    assert!(enabled_of(&play, &Command::PlayStart));
    assert!(!enabled_of(&play, &Command::PlayPause));
    assert!(!enabled_of(&play, &Command::PlayStep));

    // Playing: document-mutating File items are disabled, Quit stays enabled; pause/stop enabled.
    let mut playing = PredicateContext::default();
    playing.can_pause = true;
    playing.can_stop = true;
    playing.has_selection = true;
    playing.has_selectable_entities = true;
    playing.has_clipboard_entities = true;
    let menu = project_main_menu(&default_editor_menu(), &playing);
    let file = menu.file;
    let edit = menu.edit;
    let play = menu.play;
    assert!(
        !enabled_of(&file, &Command::Save),
        "Save greyed while playing"
    );
    assert!(
        !enabled_of(&file, &Command::NewFile),
        "New greyed while playing"
    );
    assert!(
        !enabled_of(&file, &Command::Close),
        "Close greyed while playing"
    );
    assert!(
        enabled_of(&file, &Command::Quit),
        "Quit remains enabled while playing"
    );
    assert_eq!(file.len(), 6, "File items stay present (6), not hidden");
    assert!(
        !enabled_of(&edit, &Command::SelectAll),
        "Select All greyed while playing"
    );
    assert!(
        !enabled_of(&edit, &Command::Cut),
        "Cut greyed while playing"
    );
    assert!(
        !enabled_of(&edit, &Command::Copy),
        "Copy greyed while playing"
    );
    assert!(
        !enabled_of(&edit, &Command::Paste),
        "Paste greyed while playing"
    );
    assert!(
        !enabled_of(&edit, &Command::Delete),
        "Delete greyed while playing"
    );
    assert!(
        !enabled_of(&edit, &Command::Duplicate),
        "Duplicate greyed while playing"
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
    // (Ctrl+N/O/S/Shift+S, Ctrl+Z/Y/A/X/C/V/D/Delete) — the SAME definition editor-shell's live
    // keystroke routing resolves through. Play carries display-only Space/Escape
    // hints for the separate playback route; View carries executable
    // Ctrl+Shift+P / Home / PageUp / PageDown.
    let (file, edit, play, view) = menu_entries();
    let accel = |entries: &[(String, Option<String>, Command)]| -> Vec<Option<String>> {
        entries.iter().map(|(_, s, _)| s.clone()).collect()
    };

    assert_eq!(
        accel(&file),
        vec![
            Some("Ctrl+N".to_owned()),
            Some("Ctrl+O".to_owned()),
            Some("Ctrl+S".to_owned()),
            Some("Ctrl+Shift+S".to_owned()),
            Some("Ctrl+W".to_owned()),
            Some("Ctrl+Q".to_owned()),
        ],
        "File items display New=Ctrl+N, Open=Ctrl+O, Save=Ctrl+S, Save-As=Ctrl+Shift+S, Close=Ctrl+W, Quit=Ctrl+Q"
    );
    assert_eq!(
        accel(&edit),
        vec![
            Some("Ctrl+Z".to_owned()),
            Some("Ctrl+Y".to_owned()),
            Some("Ctrl+A".to_owned()),
            Some("Ctrl+X".to_owned()),
            Some("Ctrl+C".to_owned()),
            Some("Ctrl+V".to_owned()),
            Some("Delete".to_owned()),
            Some("Ctrl+D".to_owned()),
        ],
        "Edit items display Undo=Ctrl+Z, Redo=Ctrl+Y, Select All=Ctrl+A, Cut=Ctrl+X, Copy=Ctrl+C, Paste=Ctrl+V, Delete=Delete, Duplicate=Ctrl+D"
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
        vec![
            Some("Ctrl+Shift+P".to_owned()),
            Some("Home".to_owned()),
            Some("PageUp".to_owned()),
            Some("PageDown".to_owned()),
        ],
        "View commands display Ctrl+Shift+P / Home / PageUp / PageDown accelerators"
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
