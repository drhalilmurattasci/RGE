//! `editor-egui-host::menu` — host projection of the editor's main menus.
//!
//! Resolves the canonical editor-menu definition EACH FRAME against the live
//! [`PredicateContext`] the editor-shell publishes, and projects each main-menu
//! surface (File / Edit / Play / View / Plugins) to the `(label, shortcut display,
//! `[`Command`]`, enabled)` tuples the host's menu bar paints — `enabled` greys
//! items whose enablement predicate is false for the current state. Also owns
//! [`menu_item`] (one button + optional `shortcut_text`).
//!
//! The menu DEFINITION — extension points, entries, and the File/Edit
//! accelerators — moved down to `editor-ui` (W08 canonical menu source) so
//! `editor-shell` can resolve the same bindings for accelerator EXECUTION without
//! a reverse crate edge; this module keeps only the host's display projection.
//! The `menu` submodule itself was split out of `lib.rs`
//! (EGUIHOST-MENU-EXTRACTION) to keep the host crate root under the §1.3 Rule-3
//! 1000-line cap; MENU-SHORTCUT-DISPLAY (#304) shipped the File/Edit accelerator
//! data the projection carries. Play's plain-key playback bindings are projected
//! only through display hints, not as executable menu accelerators.

use rge_editor_ui::menus::{
    edit_menu_point, file_menu_point, play_menu_point, plugins_menu_point, view_menu_point,
    Command, ExtensionPoint, MenuEntry, MenuRegistry, PredicateContext, RegistryError, Shortcut,
};

/// Projected menu item: `(label, shortcut display, command, enabled)`.
pub(crate) type ProjectedMenuEntry = (String, Option<String>, Command, bool);

/// Host-owned command-palette item projected from the current main-menu state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectedCommandPaletteEntry {
    /// Display label, prefixed with its menu path (for example `File: Save`).
    pub label: String,
    /// Optional shortcut display carried from the menu entry.
    pub shortcut: Option<String>,
    /// Command enqueued if the item is activated.
    pub command: Command,
    /// Whether the command is currently enabled in the live menu context.
    pub enabled: bool,
}

/// Host-owned shortcut-conflict diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectedShortcutConflict {
    /// Human-readable shortcut display, e.g. `Ctrl+S`.
    pub shortcut: String,
    /// Entry ids that claimed the same shortcut, in registration order.
    pub entries: Vec<String>,
}

/// Host-owned projection of the main menu surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProjectedMainMenu {
    /// File menu entries.
    pub file: Vec<ProjectedMenuEntry>,
    /// Edit menu entries.
    pub edit: Vec<ProjectedMenuEntry>,
    /// Play menu entries.
    pub play: Vec<ProjectedMenuEntry>,
    /// View menu entries.
    pub view: Vec<ProjectedMenuEntry>,
    /// Plugin menu entries. Empty until extension/plugin code registers entries.
    pub plugins: Vec<ProjectedMenuEntry>,
    /// Shortcut conflicts detected by the registry during this resolve.
    pub conflicts: Vec<ProjectedShortcutConflict>,
}

/// Resolve `registry` against the live `ctx` and project each main-menu point
/// (File / Edit / Play / View / Plugins) to the entries the menu bar paints. The shortcut
/// element is `Some(`[`Shortcut::display`]`)` for real executable shortcuts
/// (File/Edit/View) and also for passive display-only hints such as Play's
/// Space/Escape keys. Passive hints do not enter the accelerator table; the
/// keystroke itself is routed by editor-shell's playback path. `enabled` is the
/// resolved entry's [`rge_editor_ui::menus::ResolvedEntry::enabled`] for `ctx`
/// (greys the item when its enablement predicate is false). The projection also
/// carries registry shortcut conflicts so the host can render diagnostics instead
/// of silently dropping them.
///
/// Called PER FRAME with the live [`PredicateContext`] the editor-shell publishes,
/// so menu enablement tracks the live `PlayState` / editing state. The host caches
/// the `registry` (built once from `default_editor_menu` in `editor-ui`) and
/// re-resolves here each frame; the menus' content + order are owned by
/// `default_editor_menu` (the `menu_tests` pin every label + display string).
pub(crate) fn project_main_menu(
    registry: &MenuRegistry,
    ctx: &PredicateContext,
) -> ProjectedMainMenu {
    let resolved = registry.resolve(ctx);
    // Project each resolved entry to `(label, optional shortcut display,
    // command, enabled)`. The accelerator is sourced from the resolved
    // `MenuEntry.shortcut` via `Shortcut::display`, falling back to the passive
    // `shortcut_hint`; `enabled` is the resolved `ResolvedEntry.enabled` (the
    // host greys disabled items, which stay present).
    let project = |point: &ExtensionPoint| -> Vec<ProjectedMenuEntry> {
        resolved
            .entries_for(point)
            .iter()
            .map(|r| {
                (
                    r.entry.label.clone(),
                    r.entry
                        .shortcut
                        .as_ref()
                        .or(r.entry.shortcut_hint.as_ref())
                        .map(Shortcut::display),
                    r.entry.command.clone(),
                    r.enabled,
                )
            })
            .collect()
    };
    let conflicts = resolved
        .conflicts
        .iter()
        .map(|conflict| ProjectedShortcutConflict {
            shortcut: conflict.shortcut.display(),
            entries: conflict.entries.iter().map(ToString::to_string).collect(),
        })
        .collect();
    ProjectedMainMenu {
        file: project(&file_menu_point()),
        edit: project(&edit_menu_point()),
        play: project(&play_menu_point()),
        view: project(&view_menu_point()),
        plugins: project(&plugins_menu_point()),
        conflicts,
    }
}

/// Flatten the projected main-menu surface into the command palette's list.
///
/// The palette is a second view over the same resolved menu state: labels,
/// shortcuts, enablement, and commands all come from [`project_main_menu`].
/// Shortcut conflict diagnostics are intentionally omitted because they are not
/// activatable commands.
pub(crate) fn command_palette_entries(
    main_menu: &ProjectedMainMenu,
) -> Vec<ProjectedCommandPaletteEntry> {
    fn append_menu(
        out: &mut Vec<ProjectedCommandPaletteEntry>,
        menu_label: &str,
        entries: &[ProjectedMenuEntry],
    ) {
        out.extend(entries.iter().map(|(label, shortcut, command, enabled)| {
            ProjectedCommandPaletteEntry {
                label: format!("{menu_label}: {label}"),
                shortcut: shortcut.clone(),
                command: command.clone(),
                enabled: *enabled,
            }
        }));
    }

    let mut out = Vec::new();
    append_menu(&mut out, "File", &main_menu.file);
    append_menu(&mut out, "Edit", &main_menu.edit);
    append_menu(&mut out, "Play", &main_menu.play);
    append_menu(&mut out, "View", &main_menu.view);
    append_menu(&mut out, "Plugins", &main_menu.plugins);
    out
}

/// Register an extension-provided entry against any declared main-menu extension
/// point. The entry is stored in the same [`MenuRegistry`] that
/// [`project_main_menu`] resolves each frame; activation still only enqueues the
/// entry's [`Command`] into the host->shell menu handoff.
///
/// # Errors
///
/// Forwards [`RegistryError::UnknownExtensionPoint`] and
/// [`RegistryError::DuplicateEntryId`] from the registry.
pub(crate) fn register_menu_entry(
    registry: &mut MenuRegistry,
    point: &ExtensionPoint,
    entry: MenuEntry,
) -> Result<(), RegistryError> {
    registry.register_entry(point, entry)
}

/// Register a plugin-provided entry against the optional Plugins main-menu
/// extension point.
///
/// # Errors
///
/// Forwards [`RegistryError::DuplicateEntryId`] from the registry when another
/// entry with the same id already exists in Plugins. The default editor menu
/// declares the Plugins point, so [`RegistryError::UnknownExtensionPoint`] would
/// indicate a caller supplied a non-canonical registry.
pub(crate) fn register_plugin_menu_entry(
    registry: &mut MenuRegistry,
    entry: MenuEntry,
) -> Result<(), RegistryError> {
    register_menu_entry(registry, &plugins_menu_point(), entry)
}

/// Add one main-menu item: its `label`, plus — when the entry carries an
/// accelerator — that hint rendered as egui's right-aligned `shortcut_text`.
/// `enabled` greys the item out — every caller passes the item's resolved
/// [`rge_editor_ui::menus::ResolvedEntry::enabled`] (from [`project_main_menu`]).
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
