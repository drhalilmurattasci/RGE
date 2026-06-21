// SPLIT-EXEMPTION: cohesive editor-egui-host menu and command-palette test
// module. The command-palette filter, keyboard selection, projection, and
// menu-command FIFO tests share local palette-entry helpers and synthetic menu
// fixtures; splitting would duplicate setup or make the cross-behavior
// assertions harder to audit.
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

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rge_editor_ui::menus::{
    default_editor_menu, file_menu_point, plugins_menu_point, Command, Key, MenuEntry, Modifiers,
    PredicateContext, RegistryError, Shortcut,
};

use super::MenuCommandHandoff;
use crate::menu::{
    annotated_main_menu_items, command_palette_entries, command_palette_row_is_selected,
    command_palette_selected_index, command_palette_selected_index_for_filter_change,
    filter_command_palette_entries, filter_command_palette_entries_with_pinned_and_recents,
    filter_command_palette_entries_with_recents, move_command_palette_selected_index,
    project_main_menu, record_command_palette_recent_command, register_menu_entry,
    selected_command_palette_entry, take_command_palette_search_focus_request,
    CommandPaletteSelectionDirection, ProjectedCommandPaletteEntry, ProjectedMainMenu,
    ProjectedMainMenuItem, ProjectedShortcutConflict, COMMAND_PALETTE_PINNED_COMMAND_LIMIT,
    COMMAND_PALETTE_RECENT_COMMAND_LIMIT,
};
use crate::palette_pinned::{
    load_command_palette_pinned_command_ids, load_command_palette_pinned_command_ids_or_empty,
    save_command_palette_pinned_command_ids, toggle_command_palette_pinned_command,
    toggle_command_palette_pinned_command_id,
};
use crate::palette_recent::{
    enqueue_command_palette_activation, load_command_palette_recent_command_ids,
    load_command_palette_recent_command_ids_or_empty, save_command_palette_recent_command_ids,
};

type PaletteEntry = ProjectedCommandPaletteEntry;
type Dir = CommandPaletteSelectionDirection;

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

fn pe(label: &str, shortcut: Option<&str>, command: Command, enabled: bool) -> PaletteEntry {
    PaletteEntry {
        label: label.to_owned(),
        shortcut: shortcut.map(str::to_owned),
        command,
        enabled,
        conflict_peer_entry_ids: Vec::new(),
    }
}

fn save(enabled: bool) -> PaletteEntry {
    pe("File: Save", Some("Ctrl+S"), Command::Save, enabled)
}

fn open(enabled: bool) -> PaletteEntry {
    pe("File: Open", Some("Ctrl+O"), Command::OpenFile, enabled)
}

fn toggle(enabled: bool) -> PaletteEntry {
    pe(
        "View: Command Palette",
        Some("Ctrl+Shift+P"),
        Command::ToggleCommandPalette,
        enabled,
    )
}

fn refs(entries: &[PaletteEntry]) -> Vec<&PaletteEntry> {
    entries.iter().collect()
}

fn command_palette_recent_temp_root(test_name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rge_editor_egui_host_{test_name}_{}_{}",
        std::process::id(),
        stamp
    ))
}

fn assert_sel(e: &[&PaletteEntry], current: Option<usize>, expected: Option<usize>, msg: &str) {
    let actual = command_palette_selected_index(e, current);
    assert_eq!(actual, expected, "{msg}");
}

fn assert_move(
    entries: &[&PaletteEntry],
    current: Option<usize>,
    direction: Dir,
    expected: Option<usize>,
    msg: &str,
) {
    let actual = move_command_palette_selected_index(entries, current, direction);
    assert_eq!(actual, expected, "{msg}");
}

fn assert_cmd(e: &[&PaletteEntry], selected: Option<usize>, expected: Option<Command>, msg: &str) {
    let actual = selected_command_palette_entry(e, selected);
    assert_eq!(actual, expected, "{msg}");
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
fn edit_menu_registry_resolves_core_entries_in_order() {
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
            (
                "Delete Current CAD Cuboid".to_owned(),
                Some("Ctrl+Shift+Delete".to_owned()),
                Command::DeleteCurrentCadCuboid,
            ),
        ],
        "the MenuRegistry resolves the Edit menu to exactly Undo / Redo / Select All / Cut / Copy / Paste / Delete / Duplicate / Delete Current CAD Cuboid, in order \
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
            Command::DeleteCurrentCadCuboid,
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
    register_menu_entry(
        &mut registry,
        &plugins_menu_point(),
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
    register_menu_entry(
        &mut registry,
        &plugins_menu_point(),
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
            .any(|entry| entry.label == "Edit: Delete Current CAD Cuboid"
                && entry.shortcut.as_deref() == Some("Ctrl+Shift+Delete")
                && entry.command == Command::DeleteCurrentCadCuboid
                && !entry.enabled),
        "palette includes the dedicated CAD delete command shortcut through generic projection"
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
fn main_menu_items_annotate_enabled_shortcut_conflicts_from_projected_menu() {
    let plugin_command = Command::Plugin {
        plugin_id: "com.example.mesh-audit".to_owned(),
        action_id: "save".to_owned(),
    };
    let main_menu = ProjectedMainMenu {
        file: vec![
            (
                "Save".to_owned(),
                Some("Ctrl+S".to_owned()),
                Command::Save,
                true,
            ),
            (
                "Open".to_owned(),
                Some("Ctrl+O".to_owned()),
                Command::OpenFile,
                true,
            ),
            (
                "Close".to_owned(),
                Some("Ctrl+W".to_owned()),
                Command::Close,
                false,
            ),
        ],
        plugins: vec![(
            "Plugin Save".to_owned(),
            Some("Ctrl+S".to_owned()),
            plugin_command.clone(),
            true,
        )],
        conflicts: vec![
            ProjectedShortcutConflict {
                shortcut: "Ctrl+S".to_owned(),
                entries: vec!["file.save".to_owned(), "plugin.mesh_audit.save".to_owned()],
            },
            ProjectedShortcutConflict {
                shortcut: "Ctrl+W".to_owned(),
                entries: vec!["file.close".to_owned(), "plugin.close".to_owned()],
            },
        ],
        ..ProjectedMainMenu::default()
    };

    assert_eq!(
        annotated_main_menu_items(&main_menu.file, &main_menu.conflicts),
        vec![
            ProjectedMainMenuItem {
                label: "Save".to_owned(),
                shortcut: Some("Ctrl+S".to_owned()),
                command: Command::Save,
                enabled: true,
                conflict_peer_entry_ids: vec![
                    "file.save".to_owned(),
                    "plugin.mesh_audit.save".to_owned(),
                ],
            },
            ProjectedMainMenuItem {
                label: "Open".to_owned(),
                shortcut: Some("Ctrl+O".to_owned()),
                command: Command::OpenFile,
                enabled: true,
                conflict_peer_entry_ids: Vec::new(),
            },
            ProjectedMainMenuItem {
                label: "Close".to_owned(),
                shortcut: Some("Ctrl+W".to_owned()),
                command: Command::Close,
                enabled: false,
                conflict_peer_entry_ids: Vec::new(),
            },
        ],
        "main-menu annotation preserves row order, shortcut text, commands, and disabled state"
    );
    assert_eq!(
        annotated_main_menu_items(&main_menu.plugins, &main_menu.conflicts),
        vec![ProjectedMainMenuItem {
            label: "Plugin Save".to_owned(),
            shortcut: Some("Ctrl+S".to_owned()),
            command: plugin_command,
            enabled: true,
            conflict_peer_entry_ids: vec![
                "file.save".to_owned(),
                "plugin.mesh_audit.save".to_owned(),
            ],
        }],
        "plugin menu entries use the same projected shortcut conflict detail"
    );
}

#[test]
fn command_palette_entries_annotate_enabled_shortcut_conflicts_from_projected_menu() {
    let plugin_command = Command::Plugin {
        plugin_id: "com.example.mesh-audit".to_owned(),
        action_id: "save".to_owned(),
    };
    let main_menu = ProjectedMainMenu {
        file: vec![
            (
                "Save".to_owned(),
                Some("Ctrl+S".to_owned()),
                Command::Save,
                true,
            ),
            (
                "Open".to_owned(),
                Some("Ctrl+O".to_owned()),
                Command::OpenFile,
                true,
            ),
            (
                "Close".to_owned(),
                Some("Ctrl+W".to_owned()),
                Command::Close,
                false,
            ),
        ],
        plugins: vec![(
            "Plugin Save".to_owned(),
            Some("Ctrl+S".to_owned()),
            plugin_command.clone(),
            true,
        )],
        conflicts: vec![
            ProjectedShortcutConflict {
                shortcut: "Ctrl+S".to_owned(),
                entries: vec!["file.save".to_owned(), "plugin.mesh_audit.save".to_owned()],
            },
            ProjectedShortcutConflict {
                shortcut: "Ctrl+W".to_owned(),
                entries: vec!["file.close".to_owned(), "plugin.close".to_owned()],
            },
        ],
        ..ProjectedMainMenu::default()
    };

    let palette = command_palette_entries(&main_menu);
    let save = palette
        .iter()
        .find(|entry| entry.command == Command::Save)
        .expect("Save palette row exists");
    assert_eq!(
        save.conflict_peer_entry_ids,
        vec!["file.save".to_owned(), "plugin.mesh_audit.save".to_owned()],
        "enabled conflicted rows copy ordered peer ids from ProjectedMainMenu.conflicts"
    );
    let plugin_save = palette
        .iter()
        .find(|entry| entry.command == plugin_command)
        .expect("plugin palette row exists");
    assert_eq!(
        plugin_save.conflict_peer_entry_ids,
        vec!["file.save".to_owned(), "plugin.mesh_audit.save".to_owned()],
        "matching is by displayed shortcut string, not by command identity"
    );

    let open = palette
        .iter()
        .find(|entry| entry.command == Command::OpenFile)
        .expect("Open palette row exists");
    assert!(
        open.conflict_peer_entry_ids.is_empty(),
        "unconflicted enabled rows expose no conflict detail"
    );
    let close = palette
        .iter()
        .find(|entry| entry.command == Command::Close)
        .expect("Close palette row exists");
    assert!(!close.enabled);
    assert!(
        close.conflict_peer_entry_ids.is_empty(),
        "disabled rows do not gain conflict detail even when their shortcut is conflicted"
    );
}

#[test]
fn command_palette_conflict_annotation_preserves_order_filter_and_activation() {
    let main_menu = ProjectedMainMenu {
        file: vec![
            (
                "Save".to_owned(),
                Some("Ctrl+S".to_owned()),
                Command::Save,
                true,
            ),
            (
                "Open".to_owned(),
                Some("Ctrl+O".to_owned()),
                Command::OpenFile,
                true,
            ),
        ],
        conflicts: vec![ProjectedShortcutConflict {
            shortcut: "Ctrl+S".to_owned(),
            entries: vec!["peer_alpha".to_owned(), "peer_beta".to_owned()],
        }],
        ..ProjectedMainMenu::default()
    };

    let palette = command_palette_entries(&main_menu);
    assert_eq!(
        palette
            .iter()
            .map(|entry| entry.label.as_str())
            .collect::<Vec<_>>(),
        vec!["File: Save", "File: Open"],
        "conflict annotation does not reorder palette projection"
    );
    assert!(
        filter_command_palette_entries(&palette, "peer_alpha").is_empty(),
        "conflict peer ids are informational text, not palette search input"
    );
    assert_eq!(
        selected_command_palette_entry(&refs(&palette), Some(0)),
        Some(Command::Save),
        "conflict annotation does not change Enter activation"
    );
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
fn command_palette_recent_records_most_recent_ids_with_cap_and_deduplication() {
    let mut recent = Vec::new();
    let total = COMMAND_PALETTE_RECENT_COMMAND_LIMIT + 2;
    for index in 0..total {
        record_command_palette_recent_command(&mut recent, format!("cmd_{index:02}"));
    }

    let expected: Vec<String> = (2..total)
        .rev()
        .map(|index| format!("cmd_{index:02}"))
        .collect();
    assert_eq!(recent, expected, "only the capped most-recent ids remain");

    let duplicate = format!("cmd_{:02}", COMMAND_PALETTE_RECENT_COMMAND_LIMIT - 1);
    record_command_palette_recent_command(&mut recent, duplicate.clone());
    assert_eq!(recent.len(), COMMAND_PALETTE_RECENT_COMMAND_LIMIT);
    assert_eq!(recent.first().map(String::as_str), Some(duplicate.as_str()));
    assert_eq!(
        recent.iter().filter(|id| *id == &duplicate).count(),
        1,
        "re-recording an id moves it to the front without duplication"
    );

    record_command_palette_recent_command(&mut recent, "cmd_new".to_owned());
    assert_eq!(recent.len(), COMMAND_PALETTE_RECENT_COMMAND_LIMIT);
    assert_eq!(recent.first().map(String::as_str), Some("cmd_new"));
    assert!(
        !recent.contains(&"cmd_02".to_owned()),
        "recording a fresh id beyond the cap drops the oldest retained id"
    );
}

#[test]
fn command_palette_recent_persistence_round_trips_ids_only() {
    let root = command_palette_recent_temp_root("round_trips");
    let path = root.join("recent.txt");
    let recent = vec![
        Command::Save.diagnostic_id(),
        Command::OpenFile.diagnostic_id(),
        Command::ToggleCommandPalette.diagnostic_id(),
    ];

    save_command_palette_recent_command_ids(&path, &recent).expect("save recents");

    assert_eq!(
        std::fs::read_to_string(&path).expect("read persisted recents"),
        "save\nopen_file\ntoggle_command_palette\n",
        "the file stores only diagnostic id strings, one per line"
    );
    assert_eq!(
        load_command_palette_recent_command_ids(&path).expect("load recents"),
        recent
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_recent_persistence_caps_and_deduplicates_ids() {
    let root = command_palette_recent_temp_root("caps_and_deduplicates");
    let path = root.join("recent.txt");
    let mut recent: Vec<String> = (0..(COMMAND_PALETTE_RECENT_COMMAND_LIMIT + 4))
        .map(|index| format!("cmd_{index:02}"))
        .collect();
    recent.insert(2, "cmd_00".to_owned());

    save_command_palette_recent_command_ids(&path, &recent).expect("save recents");
    let loaded = load_command_palette_recent_command_ids(&path).expect("load recents");
    let expected: Vec<String> = (0..COMMAND_PALETTE_RECENT_COMMAND_LIMIT)
        .map(|index| format!("cmd_{index:02}"))
        .collect();

    assert_eq!(
        loaded, expected,
        "persisted recents keep first-seen most-recent order, dedupe, and stay capped"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_recent_missing_unreadable_or_corrupt_persistence_is_nonfatal() {
    let root = command_palette_recent_temp_root("nonfatal_load");
    let missing = root.join("missing.txt");
    let unreadable = root.join("as-directory");
    let corrupt = root.join("corrupt.txt");
    std::fs::create_dir_all(&unreadable).expect("create unreadable directory path");
    std::fs::write(&corrupt, "save\nbad id\n").expect("write corrupt recents");

    assert!(
        load_command_palette_recent_command_ids_or_empty(&missing).is_empty(),
        "missing recents fall back to empty"
    );
    assert!(
        load_command_palette_recent_command_ids_or_empty(&unreadable).is_empty(),
        "unreadable recents fall back to empty"
    );
    assert!(
        load_command_palette_recent_command_ids(&corrupt).is_err(),
        "corrupt recents are detected"
    );
    assert!(
        load_command_palette_recent_command_ids_or_empty(&corrupt).is_empty(),
        "corrupt recents fall back to empty at the host boundary"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_recent_persisted_stale_and_disabled_ids_are_not_promoted() {
    let root = command_palette_recent_temp_root("stale_disabled");
    let path = root.join("recent.txt");
    let persisted = vec![
        "missing_command".to_owned(),
        Command::Save.diagnostic_id(),
        Command::ToggleCommandPalette.diagnostic_id(),
    ];
    save_command_palette_recent_command_ids(&path, &persisted).expect("save recents");
    let recents = load_command_palette_recent_command_ids(&path).expect("load recents");
    let entries = vec![save(false), open(true), toggle(true)];

    let labels: Vec<&str> = filter_command_palette_entries_with_recents(&entries, "", &recents)
        .into_iter()
        .map(|entry| entry.label.as_str())
        .collect();

    assert_eq!(
        labels,
        vec!["View: Command Palette", "File: Save", "File: Open"],
        "only enabled persisted ids are promoted; stale ids are ignored and disabled ids remain in the original-order remainder"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_recent_palette_activation_records_saves_and_enqueues() {
    let root = command_palette_recent_temp_root("activation_saves");
    let path = root.join("recent.txt");
    let menu_commands = MenuCommandHandoff::new();
    let mut recent = Vec::new();

    enqueue_command_palette_activation(&menu_commands, &mut recent, &path, Command::Save);

    assert_eq!(recent, vec![Command::Save.diagnostic_id()]);
    assert_eq!(
        load_command_palette_recent_command_ids(&path).expect("load recents"),
        vec![Command::Save.diagnostic_id()],
        "successful palette activation saves the recorded diagnostic id"
    );
    assert_eq!(
        menu_commands.drain(),
        vec![Command::Save],
        "palette activation still enqueues through the menu-command handoff"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_recent_unwritable_save_does_not_block_palette_dispatch() {
    let root = command_palette_recent_temp_root("unwritable_save");
    let path = root.join("recent-directory");
    std::fs::create_dir_all(&path).expect("create directory at recents path");
    let menu_commands = MenuCommandHandoff::new();
    let mut recent = Vec::new();

    enqueue_command_palette_activation(&menu_commands, &mut recent, &path, Command::OpenFile);

    assert_eq!(recent, vec![Command::OpenFile.diagnostic_id()]);
    assert_eq!(
        menu_commands.drain(),
        vec![Command::OpenFile],
        "save failure is nonfatal and command dispatch still happens"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_recent_main_menu_activation_does_not_record_or_persist() {
    let root = command_palette_recent_temp_root("main_menu_non_recording");
    let path = root.join("recent.txt");
    let menu_commands = MenuCommandHandoff::new();
    let recent = vec![Command::ToggleCommandPalette.diagnostic_id()];

    menu_commands.push(Command::Save);

    assert_eq!(
        recent,
        vec![Command::ToggleCommandPalette.diagnostic_id()],
        "main-menu activation does not mutate command-palette recents"
    );
    assert!(
        !path.exists(),
        "main-menu activation does not create the command-palette recent file"
    );
    assert_eq!(
        menu_commands.drain(),
        vec![Command::Save],
        "main menu still uses the existing menu-command handoff path"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_pinned_toggle_caps_deduplicates_and_unpins_ids() {
    let mut pinned = Vec::new();
    let total = COMMAND_PALETTE_PINNED_COMMAND_LIMIT + 2;
    for index in 0..total {
        assert!(
            toggle_command_palette_pinned_command_id(&mut pinned, format!("cmd_{index:02}")),
            "fresh id is pinned"
        );
    }

    let expected: Vec<String> = (2..total)
        .rev()
        .map(|index| format!("cmd_{index:02}"))
        .collect();
    assert_eq!(
        pinned, expected,
        "only the capped most-recently pinned ids remain"
    );

    let existing = format!("cmd_{:02}", COMMAND_PALETTE_PINNED_COMMAND_LIMIT - 1);
    assert!(
        !toggle_command_palette_pinned_command_id(&mut pinned, existing.clone()),
        "toggling an existing pin unpins it"
    );
    assert!(!pinned.contains(&existing));
    assert_eq!(
        pinned.len(),
        COMMAND_PALETTE_PINNED_COMMAND_LIMIT - 1,
        "unpin removes the id without backfilling older dropped ids"
    );
}

#[test]
fn command_palette_pinned_persistence_round_trips_ids_only() {
    let root = command_palette_recent_temp_root("pinned_round_trips");
    let path = root.join("pinned.txt");
    let pinned = vec![
        Command::ToggleCommandPalette.diagnostic_id(),
        Command::Save.diagnostic_id(),
        Command::OpenFile.diagnostic_id(),
    ];

    save_command_palette_pinned_command_ids(&path, &pinned).expect("save pinned ids");

    assert_eq!(
        std::fs::read_to_string(&path).expect("read persisted pinned ids"),
        "toggle_command_palette\nsave\nopen_file\n",
        "the file stores only diagnostic id strings, one per line"
    );
    assert_eq!(
        load_command_palette_pinned_command_ids(&path).expect("load pinned ids"),
        pinned
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_pinned_persistence_caps_and_deduplicates_ids() {
    let root = command_palette_recent_temp_root("pinned_caps_and_deduplicates");
    let path = root.join("pinned.txt");
    let mut pinned: Vec<String> = (0..(COMMAND_PALETTE_PINNED_COMMAND_LIMIT + 4))
        .map(|index| format!("cmd_{index:02}"))
        .collect();
    pinned.insert(2, "cmd_00".to_owned());

    save_command_palette_pinned_command_ids(&path, &pinned).expect("save pinned ids");
    let loaded = load_command_palette_pinned_command_ids(&path).expect("load pinned ids");
    let expected: Vec<String> = (0..COMMAND_PALETTE_PINNED_COMMAND_LIMIT)
        .map(|index| format!("cmd_{index:02}"))
        .collect();

    assert_eq!(
        loaded, expected,
        "persisted pins keep first-seen order, dedupe, and stay capped"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_pinned_unpin_persists_across_load() {
    let root = command_palette_recent_temp_root("pinned_unpin_persists");
    let path = root.join("pinned.txt");
    let mut pinned = Vec::new();

    assert!(toggle_command_palette_pinned_command(
        &mut pinned,
        &path,
        &Command::Save
    ));
    assert!(!toggle_command_palette_pinned_command(
        &mut pinned,
        &path,
        &Command::Save
    ));

    assert!(
        load_command_palette_pinned_command_ids(&path)
            .expect("load pinned ids")
            .is_empty(),
        "unpinning persists an empty pinned list"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_pinned_missing_unreadable_or_corrupt_persistence_is_nonfatal() {
    let root = command_palette_recent_temp_root("pinned_nonfatal_load");
    let missing = root.join("missing.txt");
    let unreadable = root.join("as-directory");
    let corrupt = root.join("corrupt.txt");
    std::fs::create_dir_all(&unreadable).expect("create unreadable directory path");
    std::fs::write(&corrupt, "save\nbad id\n").expect("write corrupt pinned ids");

    assert!(
        load_command_palette_pinned_command_ids_or_empty(&missing).is_empty(),
        "missing pinned ids fall back to empty"
    );
    assert!(
        load_command_palette_pinned_command_ids_or_empty(&unreadable).is_empty(),
        "unreadable pinned ids fall back to empty"
    );
    assert!(
        load_command_palette_pinned_command_ids(&corrupt).is_err(),
        "corrupt pinned ids are detected"
    );
    assert!(
        load_command_palette_pinned_command_ids_or_empty(&corrupt).is_empty(),
        "corrupt pinned ids fall back to empty at the host boundary"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_pinned_blank_filter_promotes_pins_before_recents() {
    let entries = vec![save(true), open(true), toggle(true)];
    let pinned = vec![Command::OpenFile.diagnostic_id()];
    let recents = vec![
        Command::ToggleCommandPalette.diagnostic_id(),
        Command::OpenFile.diagnostic_id(),
        Command::Save.diagnostic_id(),
    ];

    let labels: Vec<&str> =
        filter_command_palette_entries_with_pinned_and_recents(&entries, " ", &pinned, &recents)
            .into_iter()
            .map(|entry| entry.label.as_str())
            .collect();

    assert_eq!(
        labels,
        vec!["File: Open", "View: Command Palette", "File: Save"],
        "pinned enabled commands rank before recents and do not duplicate rows"
    );
}

#[test]
fn command_palette_pinned_stale_and_disabled_ids_are_not_promoted() {
    let entries = vec![save(false), open(true), toggle(true)];
    let pinned = vec![
        "missing_command".to_owned(),
        Command::Save.diagnostic_id(),
        Command::ToggleCommandPalette.diagnostic_id(),
    ];
    let recents = vec![Command::OpenFile.diagnostic_id()];

    let labels: Vec<&str> =
        filter_command_palette_entries_with_pinned_and_recents(&entries, "", &pinned, &recents)
            .into_iter()
            .map(|entry| entry.label.as_str())
            .collect();

    assert_eq!(
        labels,
        vec!["View: Command Palette", "File: Open", "File: Save"],
        "stale pins add no rows and disabled pins stay in the original-order remainder"
    );
}

#[test]
fn command_palette_pinned_non_blank_filter_preserves_fuzzy_score_ordering() {
    let entries = vec![
        pe("Tools: Smart Vector", None, Command::OpenFile, true),
        pe("Tools: Autosave", None, Command::OpenFile, true),
        pe("Tools: Saver Options", None, Command::OpenFile, true),
        save(true),
    ];
    let pinned = vec![Command::OpenFile.diagnostic_id()];
    let recents = vec![Command::Save.diagnostic_id()];

    let labels: Vec<&str> =
        filter_command_palette_entries_with_pinned_and_recents(&entries, "save", &pinned, &recents)
            .into_iter()
            .map(|entry| entry.label.as_str())
            .collect();

    assert_eq!(
        labels,
        vec![
            "File: Save",
            "Tools: Saver Options",
            "Tools: Autosave",
            "Tools: Smart Vector",
        ],
        "pinned commands do not perturb non-blank fuzzy scoring"
    );
}

#[test]
fn command_palette_pin_toggle_does_not_dispatch_or_record_recent_history() {
    let root = command_palette_recent_temp_root("pin_non_dispatch");
    let path = root.join("pinned.txt");
    let menu_commands = MenuCommandHandoff::new();
    let mut pinned = Vec::new();
    let recent = vec![Command::ToggleCommandPalette.diagnostic_id()];

    assert!(toggle_command_palette_pinned_command(
        &mut pinned,
        &path,
        &Command::Save
    ));

    assert_eq!(pinned, vec![Command::Save.diagnostic_id()]);
    assert_eq!(
        load_command_palette_pinned_command_ids(&path).expect("load pinned ids"),
        vec![Command::Save.diagnostic_id()],
        "pinning persists the command id"
    );
    assert!(
        menu_commands.drain().is_empty(),
        "pinning is not command activation and does not enqueue"
    );
    assert_eq!(
        recent,
        vec![Command::ToggleCommandPalette.diagnostic_id()],
        "pinning does not update recent-history ids"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_pinned_unwritable_save_is_nonfatal() {
    let root = command_palette_recent_temp_root("pinned_unwritable_save");
    let path = root.join("pinned-directory");
    std::fs::create_dir_all(&path).expect("create directory at pinned path");
    let mut pinned = Vec::new();

    assert!(toggle_command_palette_pinned_command(
        &mut pinned,
        &path,
        &Command::OpenFile
    ));

    assert_eq!(
        pinned,
        vec![Command::OpenFile.diagnostic_id()],
        "save failure is nonfatal and the in-memory pin still changes"
    );

    drop(std::fs::remove_dir_all(root));
}

#[test]
fn command_palette_recent_blank_filter_promotes_enabled_recent_entries() {
    let entries = vec![save(true), open(true), toggle(true)];
    let recents = vec![
        Command::ToggleCommandPalette.diagnostic_id(),
        Command::Save.diagnostic_id(),
    ];

    let labels: Vec<&str> = filter_command_palette_entries_with_recents(&entries, "  ", &recents)
        .into_iter()
        .map(|entry| entry.label.as_str())
        .collect();

    assert_eq!(
        labels,
        vec!["View: Command Palette", "File: Save", "File: Open"],
        "blank filters put enabled recent commands first, then keep original order"
    );
}

#[test]
fn command_palette_recent_blank_filter_ignores_stale_ids_and_disabled_promotions() {
    let entries = vec![save(false), open(true), toggle(true)];
    let recents = vec![
        "missing_command".to_owned(),
        Command::Save.diagnostic_id(),
        Command::ToggleCommandPalette.diagnostic_id(),
    ];

    let labels: Vec<&str> = filter_command_palette_entries_with_recents(&entries, "", &recents)
        .into_iter()
        .map(|entry| entry.label.as_str())
        .collect();

    assert_eq!(
        labels,
        vec!["View: Command Palette", "File: Save", "File: Open"],
        "stale ids do not add rows and disabled recent rows stay in the remainder"
    );
}

#[test]
fn command_palette_recent_non_blank_filter_preserves_fuzzy_score_ordering() {
    let entries = vec![
        pe("Tools: Smart Vector", None, Command::OpenFile, true),
        pe("Tools: Autosave", None, Command::OpenFile, true),
        pe("Tools: Saver Options", None, Command::OpenFile, true),
        save(true),
    ];
    let recents = vec![Command::OpenFile.diagnostic_id()];

    let labels: Vec<&str> = filter_command_palette_entries_with_recents(&entries, "save", &recents)
        .into_iter()
        .map(|entry| entry.label.as_str())
        .collect();

    assert_eq!(
        labels,
        vec![
            "File: Save",
            "Tools: Saver Options",
            "Tools: Autosave",
            "Tools: Smart Vector",
        ],
        "recent history does not perturb non-blank fuzzy scoring"
    );
}

#[test]
fn command_palette_filter_orders_exact_word_matches_before_longer_matches() {
    let entries = vec![
        pe(
            "File: Save As New Project",
            Some("Ctrl+Shift+S"),
            Command::SaveAs,
            true,
        ),
        save(true),
        open(true),
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
fn command_palette_filter_fuzzy_matches_label_shortcut_and_command_id() {
    let entries = vec![
        pe(
            "File: Save As New Project",
            Some("Ctrl+Shift+S"),
            Command::SaveAs,
            true,
        ),
        toggle(true),
        open(true),
    ];
    let labels = |filter: &str| -> Vec<&str> {
        filter_command_palette_entries(&entries, filter)
            .into_iter()
            .map(|entry| entry.label.as_str())
            .collect()
    };

    assert_eq!(
        labels("snp"),
        vec!["File: Save As New Project"],
        "ordered-subsequence matching works over labels"
    );
    assert_eq!(
        labels("csp"),
        vec!["View: Command Palette"],
        "ordered-subsequence matching works over shortcut display"
    );
    assert_eq!(
        labels("tgcp"),
        vec!["View: Command Palette"],
        "ordered-subsequence matching works over command diagnostic ids"
    );
}

#[test]
fn command_palette_filter_keeps_exact_prefix_and_substring_before_fuzzy() {
    let entries = vec![
        pe("Tools: Smart Vector", None, Command::OpenFile, true),
        pe("Tools: Autosave", None, Command::OpenFile, true),
        pe("Tools: Saver Options", None, Command::OpenFile, true),
        save(true),
    ];

    let labels: Vec<&str> = filter_command_palette_entries(&entries, "save")
        .into_iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec![
            "File: Save",
            "Tools: Saver Options",
            "Tools: Autosave",
            "Tools: Smart Vector",
        ],
        "exact, prefix, and substring matches stay ahead of fuzzy-only matches"
    );
}

#[test]
fn command_palette_filter_orders_fuzzy_matches_by_compactness() {
    let entries = vec![
        pe("A: Sample Value", None, Command::OpenFile, true),
        pe("A: S Value", None, Command::OpenFile, true),
    ];

    let labels: Vec<&str> = filter_command_palette_entries(&entries, "sv")
        .into_iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec!["A: S Value", "A: Sample Value"],
        "more compact fuzzy matches sort first before original-order fallback"
    );
}

#[test]
fn command_palette_filter_keeps_unmatched_fuzzy_queries_empty() {
    let entries = vec![save(true), open(true), toggle(true)];

    assert!(
        filter_command_palette_entries(&entries, "zzzzzz").is_empty(),
        "a query still needs every term to match at least one searchable field"
    );
}

#[test]
fn command_palette_selection_uses_first_enabled_row_when_needed() {
    let entries = vec![save(false), toggle(true), open(true)];
    let filtered_entries = refs(&entries);

    assert_sel(
        &filtered_entries,
        None,
        Some(1),
        "missing selects first enabled",
    );
    assert_sel(
        &filtered_entries,
        Some(0),
        Some(1),
        "disabled row is skipped",
    );
    assert_sel(&filtered_entries, Some(99), Some(1), "stale row is clamped");
    assert_sel(
        &filtered_entries,
        Some(2),
        Some(2),
        "valid row is preserved",
    );
}

#[test]
fn command_palette_selection_handles_filter_change_boundaries() {
    let entries = vec![save(true), open(true), toggle(true)];
    let filtered_entries = refs(&entries);

    assert_eq!(
        command_palette_selected_index_for_filter_change(&filtered_entries, Some(2), true),
        Some(0)
    );
    assert_eq!(
        command_palette_selected_index_for_filter_change(&filtered_entries, Some(2), false),
        Some(2)
    );
    let disabled_first = vec![save(false), open(true)];
    let disabled_first_entries = refs(&disabled_first);
    assert_eq!(
        command_palette_selected_index_for_filter_change(&disabled_first_entries, Some(0), true),
        Some(1)
    );
}

#[test]
fn command_palette_selection_returns_none_without_enabled_rows() {
    let disabled = vec![save(false)];
    let disabled_entries = refs(&disabled);
    let empty_entries: Vec<&ProjectedCommandPaletteEntry> = Vec::new();

    assert_sel(&disabled_entries, None, None, "disabled-only selects none");
    assert_sel(&empty_entries, Some(0), None, "empty results select none");
}

#[test]
fn command_palette_navigation_moves_down_through_enabled_rows() {
    let entries = vec![save(false), toggle(true), open(true)];
    let filtered_entries = refs(&entries);
    let next = CommandPaletteSelectionDirection::Next;

    assert_move(&filtered_entries, None, next, Some(1), "down from none");
    assert_move(&filtered_entries, Some(1), next, Some(2), "down to next");
    assert_move(&filtered_entries, Some(2), next, Some(1), "down wraps");
}

#[test]
fn command_palette_navigation_moves_up_through_enabled_rows() {
    let entries = vec![save(false), toggle(true), open(true)];
    let filtered_entries = refs(&entries);
    let previous = CommandPaletteSelectionDirection::Previous;

    assert_move(&filtered_entries, None, previous, Some(1), "up from none");
    assert_move(
        &filtered_entries,
        Some(2),
        previous,
        Some(1),
        "up to previous",
    );
    assert_move(&filtered_entries, Some(1), previous, Some(2), "up wraps");
}

#[test]
fn command_palette_navigation_skips_disabled_rows_and_disabled_only_stays_none() {
    let entries = vec![save(true), toggle(false), open(true)];
    let filtered_entries = refs(&entries);
    let next = CommandPaletteSelectionDirection::Next;
    let previous = CommandPaletteSelectionDirection::Previous;

    assert_move(
        &filtered_entries,
        Some(0),
        next,
        Some(2),
        "down skips disabled",
    );
    assert_move(
        &filtered_entries,
        Some(2),
        previous,
        Some(0),
        "up skips disabled",
    );

    let disabled_only = vec![save(false)];
    let disabled_entries = refs(&disabled_only);
    assert_move(
        &disabled_entries,
        None,
        next,
        None,
        "disabled-only has no target",
    );
}

#[test]
fn command_palette_selected_enter_activates_selected_row() {
    let entries = vec![save(true), open(true)];
    let filtered_entries = refs(&entries);

    assert_cmd(
        &filtered_entries,
        Some(1),
        Some(Command::OpenFile),
        "enter uses selected row",
    );
}

#[test]
fn command_palette_selected_enter_clamps_stale_selection() {
    let entries = vec![save(false), toggle(true)];
    let filtered_entries = refs(&entries);

    assert_cmd(
        &filtered_entries,
        Some(99),
        Some(Command::ToggleCommandPalette),
        "stale selection is clamped",
    );
    assert_cmd(
        &filtered_entries,
        Some(0),
        Some(Command::ToggleCommandPalette),
        "disabled selection is skipped",
    );
}

#[test]
fn command_palette_selected_enter_dispatches_nothing_without_enabled_rows() {
    let disabled = vec![save(false)];
    let disabled_entries = refs(&disabled);
    let empty_entries: Vec<&ProjectedCommandPaletteEntry> = Vec::new();

    assert_cmd(
        &disabled_entries,
        Some(0),
        None,
        "disabled-only dispatches none",
    );
    assert_cmd(&empty_entries, None, None, "empty dispatches none");
}

#[test]
fn command_palette_search_focus_request_is_one_shot() {
    let mut focus_requested = true;

    assert!(
        take_command_palette_search_focus_request(&mut focus_requested),
        "the first palette frame consumes the pending search-focus request"
    );
    assert!(
        !focus_requested,
        "focus request state clears after the first consumption"
    );
    assert!(
        !take_command_palette_search_focus_request(&mut focus_requested),
        "later frames do not keep stealing focus"
    );
}

#[test]
fn command_palette_selected_row_affordance_tracks_enabled_selected_row() {
    let enabled_entry = open(true);
    let disabled_entry = save(false);

    assert!(
        command_palette_row_is_selected(&enabled_entry, 1, Some(1)),
        "enabled row at the selected filtered index renders selected"
    );
    assert!(
        !command_palette_row_is_selected(&enabled_entry, 0, Some(1)),
        "other enabled rows are not rendered selected"
    );
    assert!(
        !command_palette_row_is_selected(&disabled_entry, 1, Some(1)),
        "disabled rows stay visible but never receive the selected-row affordance"
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

    register_menu_entry(&mut registry, &plugins_menu_point(), entry.clone())
        .expect("first plugin menu entry registers");
    let err = register_menu_entry(&mut registry, &plugins_menu_point(), entry)
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

    // Editing: File items + Select All + Delete + Duplicate + current-CAD
    // delete + Play (start) enabled; pause/stop/step disabled.
    let mut editing = PredicateContext::default();
    editing.is_editing = true;
    editing.can_play = true;
    editing.has_selection = true;
    editing.has_selectable_entities = true;
    editing.has_clipboard_entities = true;
    editing.has_current_cad_cuboid_selection = true;
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
    assert!(enabled_of(&edit, &Command::DeleteCurrentCadCuboid));
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
    playing.has_current_cad_cuboid_selection = true;
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
    assert!(
        !enabled_of(&edit, &Command::DeleteCurrentCadCuboid),
        "Delete Current CAD Cuboid greyed while playing"
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
    // (Ctrl+N/O/S/Shift+S, Ctrl+Z/Y/A/X/C/V/D/Delete/Ctrl+Shift+Delete) — the SAME definition editor-shell's live
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
            Some("Ctrl+Shift+Delete".to_owned()),
        ],
        "Edit items display Undo=Ctrl+Z, Redo=Ctrl+Y, Select All=Ctrl+A, Cut=Ctrl+X, Copy=Ctrl+C, Paste=Ctrl+V, Delete=Delete, Duplicate=Ctrl+D, Delete Current CAD Cuboid=Ctrl+Shift+Delete"
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
