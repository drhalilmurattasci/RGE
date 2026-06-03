//! `editor-egui-host::menu` — production main-menu construction for the host.
//!
//! Builds the data-driven [`MenuRegistry`] for the four main-menu surfaces
//! (File / Edit / Play / View), resolves it once, and projects each point to the
//! `(label, accelerator display, Command)` triples the host's menu bar paints.
//! Also owns the two render-time helpers the menu bar calls:
//! [`play_item_enabled`] (per-item PIE enablement routing) and [`menu_item`]
//! (one button + optional `shortcut_text`).
//!
//! Split out of `lib.rs` (EGUIHOST-MENU-EXTRACTION) so the host crate root stays
//! under the §1.3 Rule-3 1000-line cap — the same remedy EGUIHOST-TEST-EXTRACTION
//! (#301) applied to the inline tests. Behaviour-identical: the host `use`s these
//! items and calls them exactly as before; MENU-SHORTCUT-DISPLAY (#304) shipped
//! the File/Edit accelerator data the projection carries.

use rge_editor_ui::menus::{
    Command, ExtensionPoint, Key, MenuEntry, MenuRegistry, Modifiers, PredicateContext, Shortcut,
};

/// Extension-point id for the editor's main-menu **File** surface. Plugins (a
/// future dispatch) register additional File entries against this same id.
const FILE_MENU_EXTENSION_POINT: &str = "editor.main_menu.file";

/// Extension-point id for the editor's main-menu **Edit** surface (A2).
const EDIT_MENU_EXTENSION_POINT: &str = "editor.main_menu.edit";

/// Extension-point id for the editor's main-menu **Play** surface (A3). The Play
/// menu's items route to the already-runtime-wired `EditorShell::handle_button`
/// (PIE) via the host→shell FIFO, not a new action.
const PLAY_MENU_EXTENSION_POINT: &str = "editor.main_menu.play";

/// Extension-point id for the editor's main-menu **View** surface (A4). The View
/// menu's `Reset Camera` item routes to the new `EditorShell::reset_camera`
/// (reframe the live scene to its bounds) via the host→shell FIFO, not the PIE
/// driver.
const VIEW_MENU_EXTENSION_POINT: &str = "editor.main_menu.view";

/// Build the production [`MenuRegistry`] with ALL FOUR main-menu extension
/// points (File + Edit + Play + View), register each point's entries, resolve
/// ONCE against an empty [`PredicateContext`], and project each point's resolved
/// entries to the `(label, accelerator display, `[`Command`]`)` triples the menu
/// bar paints. The accelerator element is `Some(`[`Shortcut::display`]`)` for the
/// File/Edit entries — their real keyboard accelerators, rendered as egui
/// `shortcut_text` — and `None` for every Play/View entry (display-only; the
/// keystroke itself is routed by editor-shell). Returns `(file, edit, play, view)`.
///
/// The registry is the single source of truth for all four menus' content + order.
/// Every entry carries the default order hint (`OrderHint::AtEnd`) in the
/// default section, so `resolve` returns each point's entries in registration
/// order:
/// - **File** = Open / Save / Save As New Project (byte-identical labels to A1,
///   behaviour-identical). "Save As New Project…" enqueues [`Command::SaveAs`];
///   the editor-shell consumer routes it to
///   `EditorShell::handle_save_as_new_project_request`.
/// - **Edit** = Undo / Redo, enqueuing [`Command::Undo`] / [`Command::Redo`],
///   which the editor-shell drain routes to `EditorShell::undo_command` /
///   `redo_command` — behaviour-identical to the existing `Ctrl+Z` / `Ctrl+Y`
///   keystroke path.
/// - **Play** = Play / Pause / Stop / Step, enqueuing [`Command::PlayStart`] /
///   [`Command::PlayPause`] / [`Command::PlayStop`] / [`Command::PlayStep`],
///   which the editor-shell drain routes to `EditorShell::handle_button`
///   (`ToolbarButtonId::{Play, Pause, Stop, Step}`) — the same PIE driver the
///   Space / Escape keyboard playback path uses. Static items; an invalid-state
///   click (e.g. Stop while Editing) is a benign swallowed no-op.
/// - **View** = Reset Camera, enqueuing [`Command::ResetCamera`], which the
///   editor-shell drain routes to the new infallible `EditorShell::reset_camera`
///   — reframe the editor camera to the live scene's AABB union (default pose
///   when the scene is empty / non-finite).
///
/// All four menus are static (no predicates / dynamic visibility), so resolving
/// once at construction is sufficient and the results are cached on the host;
/// per-frame re-resolve is deferred to a future dispatch. Construction errors
/// are unreachable here (fresh registry, distinct ids), hence the `expect`s.
pub(crate) fn build_main_menu_entries() -> (
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
    Vec<(String, Option<String>, Command)>,
) {
    let mut registry = MenuRegistry::new();
    let file_point = ExtensionPoint::new(FILE_MENU_EXTENSION_POINT);
    let edit_point = ExtensionPoint::new(EDIT_MENU_EXTENSION_POINT);
    let play_point = ExtensionPoint::new(PLAY_MENU_EXTENSION_POINT);
    let view_point = ExtensionPoint::new(VIEW_MENU_EXTENSION_POINT);
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
    // File + Edit entries carry their real, executing keyboard accelerators
    // (display-only here). The VALUES mirror the live editor-shell keystroke
    // routing — `EditorKeyCommand::from_key_press` (Ctrl+Z/Y/S, Ctrl+Shift+S)
    // and the Ctrl+O `handle_open_request` arm — which the host cannot import
    // (reverse crate edge). `MenuEntry.shortcut` is the substrate's designated
    // home for an entry's accelerator; the deferred W08 accelerator-EXECUTION
    // work unifies the two by routing keystrokes through the resolved
    // `AcceleratorTable`. The `menu_tests` pin every display string.
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
                MenuEntry::new(id, label, command).with_shortcut(shortcut),
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
    for (id, label, command) in [
        ("play.start", "Play", Command::PlayStart),
        ("play.pause", "Pause", Command::PlayPause),
        ("play.stop", "Stop", Command::PlayStop),
        ("play.step", "Step", Command::PlayStep),
    ] {
        registry
            .register_entry(&play_point, MenuEntry::new(id, label, command))
            .expect("static Play menu entries register cleanly");
    }
    registry
        .register_entry(
            &view_point,
            MenuEntry::new("view.reset_camera", "Reset Camera", Command::ResetCamera),
        )
        .expect("static View menu entries register cleanly");
    let resolved = registry.resolve(&PredicateContext::default());
    // Project each resolved entry to `(label, optional accelerator display,
    // command)`. The middle element is sourced straight from the resolved
    // `MenuEntry.shortcut` via `Shortcut::display` — `Some("Ctrl+S")` for the
    // File/Edit entries above, `None` for every Play/View entry (their
    // accelerator display is deferred with the W08 execution work).
    let project = |point: &ExtensionPoint| -> Vec<(String, Option<String>, Command)> {
        resolved
            .entries_for(point)
            .iter()
            .map(|r| {
                (
                    r.entry.label.clone(),
                    r.entry.shortcut.as_ref().map(Shortcut::display),
                    r.entry.command.clone(),
                )
            })
            .collect()
    };
    (
        project(&file_point),
        project(&edit_point),
        project(&play_point),
        project(&view_point),
    )
}

/// Map a Play-menu [`Command`] to its enabled flag from the per-frame
/// [`rge_editor_state::MenuStateSnapshot`] (published by editor-shell from the
/// canonical `PlayState`). The host re-encodes NO `PlayState` validity — it only
/// routes the already-computed booleans. Non-Play commands never appear in the
/// Play menu; they default to enabled (the editor-shell router benign-ignores any
/// stray command anyway).
pub(crate) fn play_item_enabled(
    cmd: &Command,
    menu_state: &rge_editor_state::MenuStateSnapshot,
) -> bool {
    match cmd {
        Command::PlayStart => menu_state.play_can_start,
        Command::PlayPause => menu_state.play_can_pause,
        Command::PlayStop => menu_state.play_can_stop,
        Command::PlayStep => menu_state.play_can_step,
        _ => true,
    }
}

/// Add one main-menu item: its `label`, plus — when the entry carries an
/// accelerator — that hint rendered as egui's right-aligned `shortcut_text`.
/// `enabled` greys the item out (`true` for every File / Edit / View item; the
/// Play menu passes its per-item PIE enablement from [`play_item_enabled`]).
/// Returns the click [`egui::Response`]. Display-only: the accelerator is a
/// passive hint (the keystroke is routed by editor-shell); activation is the
/// click.
pub(crate) fn menu_item(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    shortcut: Option<&str>,
) -> egui::Response {
    let mut button = egui::Button::new(label);
    if let Some(text) = shortcut {
        button = button.shortcut_text(text);
    }
    ui.add_enabled(enabled, button)
}
