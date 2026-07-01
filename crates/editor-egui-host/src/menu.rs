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

use std::path::Path;

use rge_editor_ui::menus::{
    edit_menu_point, file_menu_point, play_menu_point, plugins_menu_point, view_menu_point,
    Command, ExtensionPoint, MenuEntry, MenuRegistry, PredicateContext, RegistryError, Shortcut,
};

use crate::palette_pinned::toggle_command_palette_pinned_command;

/// Projected menu item: `(label, shortcut display, command, enabled)`.
pub(crate) type ProjectedMenuEntry = (String, Option<String>, Command, bool);

/// Maximum number of command-palette activations retained in host memory.
pub(crate) const COMMAND_PALETTE_RECENT_COMMAND_LIMIT: usize = 16;

/// Maximum number of command-palette pinned commands retained in host memory.
pub(crate) const COMMAND_PALETTE_PINNED_COMMAND_LIMIT: usize = 16;

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
    /// Ordered peer entry ids for the matching projected shortcut conflict.
    pub conflict_peer_entry_ids: Vec<String>,
}

/// Host-owned main-menu item with informational shortcut-conflict detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectedMainMenuItem {
    /// Display label for the menu item.
    pub label: String,
    /// Optional shortcut display carried from the projected menu entry.
    pub shortcut: Option<String>,
    /// Command enqueued if the item is clicked.
    pub command: Command,
    /// Whether the command is currently enabled in the live menu context.
    pub enabled: bool,
    /// Ordered peer entry ids for the matching projected shortcut conflict.
    pub conflict_peer_entry_ids: Vec<String>,
}

/// Direction for command-palette keyboard selection movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandPaletteSelectionDirection {
    /// Move to the previous enabled filtered row.
    Previous,
    /// Move to the next enabled filtered row.
    Next,
}

/// Host-owned shortcut-conflict diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectedShortcutConflict {
    /// Human-readable shortcut display, e.g. `Ctrl+S`.
    pub shortcut: String,
    /// Entry ids that claimed the same shortcut, in registration order.
    pub entries: Vec<String>,
}

/// Host-owned effective shortcut binding projected from `ResolveResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectedEffectiveBinding {
    /// Human-readable shortcut display, e.g. `Ctrl+S`.
    pub shortcut: String,
    /// Command the shortcut resolves to for display/introspection.
    pub command: Command,
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
    /// Effective executable shortcut bindings in `ResolveResult::bindings()` order.
    pub effective_bindings: Vec<ProjectedEffectiveBinding>,
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
    let effective_bindings = resolved
        .bindings()
        .map(|(shortcut, command)| ProjectedEffectiveBinding {
            shortcut: shortcut.display(),
            command: command.clone(),
        })
        .collect();
    ProjectedMainMenu {
        file: project(&file_menu_point()),
        edit: project(&edit_menu_point()),
        play: project(&play_menu_point()),
        view: project(&view_menu_point()),
        plugins: project(&plugins_menu_point()),
        conflicts,
        effective_bindings,
    }
}

/// Flatten the projected main-menu surface into the command palette's list.
///
/// The palette is a second view over the same resolved menu state: labels,
/// shortcuts, enablement, commands, and informational conflict annotations all
/// come from [`project_main_menu`].
pub(crate) fn command_palette_entries(
    main_menu: &ProjectedMainMenu,
) -> Vec<ProjectedCommandPaletteEntry> {
    fn append_menu(
        out: &mut Vec<ProjectedCommandPaletteEntry>,
        menu_label: &str,
        entries: &[ProjectedMenuEntry],
        conflicts: &[ProjectedShortcutConflict],
    ) {
        out.extend(entries.iter().map(|(label, shortcut, command, enabled)| {
            ProjectedCommandPaletteEntry {
                label: format!("{menu_label}: {label}"),
                shortcut: shortcut.clone(),
                command: command.clone(),
                enabled: *enabled,
                conflict_peer_entry_ids: projected_conflict_peer_entry_ids(
                    shortcut.as_deref(),
                    *enabled,
                    conflicts,
                ),
            }
        }));
    }

    let mut out = Vec::new();
    append_menu(&mut out, "File", &main_menu.file, &main_menu.conflicts);
    append_menu(&mut out, "Edit", &main_menu.edit, &main_menu.conflicts);
    append_menu(&mut out, "Play", &main_menu.play, &main_menu.conflicts);
    append_menu(&mut out, "View", &main_menu.view, &main_menu.conflicts);
    append_menu(
        &mut out,
        "Plugins",
        &main_menu.plugins,
        &main_menu.conflicts,
    );
    out
}

/// Annotate projected main-menu rows with already-projected shortcut conflicts.
pub(crate) fn annotated_main_menu_items(
    entries: &[ProjectedMenuEntry],
    conflicts: &[ProjectedShortcutConflict],
) -> Vec<ProjectedMainMenuItem> {
    entries
        .iter()
        .map(
            |(label, shortcut, command, enabled)| ProjectedMainMenuItem {
                label: label.clone(),
                shortcut: shortcut.clone(),
                command: command.clone(),
                enabled: *enabled,
                conflict_peer_entry_ids: projected_conflict_peer_entry_ids(
                    shortcut.as_deref(),
                    *enabled,
                    conflicts,
                ),
            },
        )
        .collect()
}

pub(crate) fn projected_conflict_peer_entry_ids(
    shortcut: Option<&str>,
    enabled: bool,
    conflicts: &[ProjectedShortcutConflict],
) -> Vec<String> {
    if !enabled {
        return Vec::new();
    }

    let Some(shortcut) = shortcut else {
        return Vec::new();
    };
    conflicts
        .iter()
        .find(|conflict| conflict.shortcut.as_str() == shortcut)
        .map(|conflict| conflict.entries.clone())
        .unwrap_or_default()
}

/// Record one successful command-palette activation by diagnostic id.
pub(crate) fn record_command_palette_recent_command(
    recent_command_ids: &mut Vec<String>,
    command_id: String,
) {
    if let Some(position) = recent_command_ids.iter().position(|id| id == &command_id) {
        recent_command_ids.remove(position);
    }
    recent_command_ids.insert(0, command_id);
    recent_command_ids.truncate(COMMAND_PALETTE_RECENT_COMMAND_LIMIT);
}

/// Return command-palette entries matching the user-entered filter.
///
/// Filtering is deliberately presentation-local: it does not persist history or
/// change command execution. Each whitespace-separated term must match the
/// menu-path label, shortcut display, or command diagnostic id. Matching entries
/// are ordered by deterministic, simple relevance: exact word/field match,
/// prefix match, substring match, fuzzy ordered-subsequence match, then original
/// menu order.
#[cfg(test)]
pub(crate) fn filter_command_palette_entries<'a>(
    entries: &'a [ProjectedCommandPaletteEntry],
    filter: &str,
) -> Vec<&'a ProjectedCommandPaletteEntry> {
    filter_command_palette_entries_with_pinned_and_recents(entries, filter, &[], &[])
}

/// Return command-palette entries, promoting recent enabled commands only for
/// blank filters.
#[cfg(test)]
pub(crate) fn filter_command_palette_entries_with_recents<'a>(
    entries: &'a [ProjectedCommandPaletteEntry],
    filter: &str,
    recent_command_ids: &[String],
) -> Vec<&'a ProjectedCommandPaletteEntry> {
    filter_command_palette_entries_with_pinned_and_recents(entries, filter, &[], recent_command_ids)
}

/// Return command-palette entries, promoting pinned commands before recents for
/// blank filters.
pub(crate) fn filter_command_palette_entries_with_pinned_and_recents<'a>(
    entries: &'a [ProjectedCommandPaletteEntry],
    filter: &str,
    pinned_command_ids: &[String],
    recent_command_ids: &[String],
) -> Vec<&'a ProjectedCommandPaletteEntry> {
    let terms: Vec<String> = filter
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    if terms.is_empty() {
        return blank_command_palette_entries_with_pinned_and_recents(
            entries,
            pinned_command_ids,
            recent_command_ids,
        );
    }

    let mut matches: Vec<(
        CommandPaletteMatchScore,
        usize,
        &ProjectedCommandPaletteEntry,
    )> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            command_palette_match_score(entry, &terms).map(|score| (score, index, entry))
        })
        .collect();
    matches.sort_by_key(|(score, index, _)| (*score, *index));
    matches.into_iter().map(|(_, _, entry)| entry).collect()
}

fn blank_command_palette_entries_with_pinned_and_recents<'a>(
    entries: &'a [ProjectedCommandPaletteEntry],
    pinned_command_ids: &[String],
    recent_command_ids: &[String],
) -> Vec<&'a ProjectedCommandPaletteEntry> {
    let mut ordered = Vec::with_capacity(entries.len());
    let mut emitted_indices = Vec::new();

    promote_command_ids(
        entries,
        pinned_command_ids,
        &mut ordered,
        &mut emitted_indices,
    );
    promote_command_ids(
        entries,
        recent_command_ids,
        &mut ordered,
        &mut emitted_indices,
    );

    for (index, entry) in entries.iter().enumerate() {
        if !emitted_indices.contains(&index) {
            ordered.push(entry);
        }
    }

    ordered
}

fn promote_command_ids<'a>(
    entries: &'a [ProjectedCommandPaletteEntry],
    command_ids: &[String],
    ordered: &mut Vec<&'a ProjectedCommandPaletteEntry>,
    emitted_indices: &mut Vec<usize>,
) {
    for command_id in command_ids {
        for (index, entry) in entries.iter().enumerate() {
            if entry.enabled
                && !emitted_indices.contains(&index)
                && entry.command.diagnostic_id() == *command_id
            {
                ordered.push(entry);
                emitted_indices.push(index);
                break;
            }
        }
    }
}

/// Return whether `entry` is currently pinned by diagnostic id.
pub(crate) fn command_palette_entry_is_pinned(
    entry: &ProjectedCommandPaletteEntry,
    pinned_command_ids: &[String],
) -> bool {
    let command_id = entry.command.diagnostic_id();
    pinned_command_ids.iter().any(|id| id == &command_id)
}

/// Return a valid filtered-row index for command-palette keyboard selection.
///
/// Disabled entries stay visible in the palette, but they are never a keyboard
/// target. If `current_index` already points at an enabled row in `entries`, it
/// is preserved; otherwise selection falls back to the first enabled row.
pub(crate) fn command_palette_selected_index(
    entries: &[&ProjectedCommandPaletteEntry],
    current_index: Option<usize>,
) -> Option<usize> {
    if let Some(index) = current_index {
        if entries.get(index).is_some_and(|entry| entry.enabled) {
            return Some(index);
        }
    }
    entries.iter().position(|entry| entry.enabled)
}

/// Return command-palette selection after a possible filter text edit.
///
/// A selected row index is meaningful only within one filtered result set. When
/// the filter changes, restart from the first enabled row in the new result set
/// instead of preserving the same numeric row position against different rows.
pub(crate) fn command_palette_selected_index_for_filter_change(
    entries: &[&ProjectedCommandPaletteEntry],
    current_index: Option<usize>,
    filter_changed: bool,
) -> Option<usize> {
    let current_index = if filter_changed { None } else { current_index };
    command_palette_selected_index(entries, current_index)
}

/// Move command-palette keyboard selection through enabled filtered rows.
///
/// Movement wraps at the ends. If `current_index` is absent or no longer points
/// at an enabled row, the first enabled row becomes selected instead of moving
/// past it.
pub(crate) fn move_command_palette_selected_index(
    entries: &[&ProjectedCommandPaletteEntry],
    current_index: Option<usize>,
    direction: CommandPaletteSelectionDirection,
) -> Option<usize> {
    let normalized = command_palette_selected_index(entries, current_index);
    if normalized != current_index {
        return normalized;
    }

    let current = normalized?;
    let enabled_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| entry.enabled.then_some(index))
        .collect();
    let position = enabled_indices
        .iter()
        .position(|index| *index == current)
        .expect("normalized selection must point at an enabled row");
    let next_position = match direction {
        CommandPaletteSelectionDirection::Next => (position + 1) % enabled_indices.len(),
        CommandPaletteSelectionDirection::Previous => {
            if position == 0 {
                enabled_indices.len() - 1
            } else {
                position - 1
            }
        }
    };
    Some(enabled_indices[next_position])
}

/// Return the command for the selected enabled filtered row.
///
/// If `selected_index` is absent, stale, or disabled, it is resolved the same
/// way as [`command_palette_selected_index`]: fall back to the first enabled
/// filtered row. Empty or disabled-only result sets return `None`.
pub(crate) fn selected_command_palette_entry(
    entries: &[&ProjectedCommandPaletteEntry],
    selected_index: Option<usize>,
) -> Option<Command> {
    let index = command_palette_selected_index(entries, selected_index)?;
    entries.get(index).map(|entry| entry.command.clone())
}

/// Consume a pending command-palette search-field focus request.
///
/// The host arms this when the palette opens. The window consumes it once after
/// creating the search field so later frames keep user-directed focus changes.
pub(crate) fn take_command_palette_search_focus_request(focus_requested: &mut bool) -> bool {
    let request_focus = *focus_requested;
    *focus_requested = false;
    request_focus
}

/// Return whether a filtered palette row should render as the selected row.
pub(crate) fn command_palette_row_is_selected(
    entry: &ProjectedCommandPaletteEntry,
    row_index: usize,
    selected_index: Option<usize>,
) -> bool {
    entry.enabled && selected_index == Some(row_index)
}

fn command_palette_match_score(
    entry: &ProjectedCommandPaletteEntry,
    terms: &[String],
) -> Option<CommandPaletteMatchScore> {
    let diagnostic_id = entry.command.diagnostic_id();
    let fields = [
        (0, entry.label.as_str()),
        (1, entry.shortcut.as_deref().unwrap_or_default()),
        (2, diagnostic_id.as_str()),
    ];
    let mut worst_class = 0;
    let mut class_sum = 0;
    let mut gap_sum = 0;
    let mut span_sum = 0;
    let mut field_sum = 0;
    for term in terms {
        let best = fields
            .iter()
            .filter_map(|(priority, field)| {
                command_palette_field_match_score(field, term, *priority)
            })
            .min()?;
        worst_class = worst_class.max(best.match_class);
        class_sum += best.match_class;
        gap_sum += best.gap_count;
        span_sum += best.span_len;
        field_sum += best.field_priority;
    }
    Some(CommandPaletteMatchScore {
        worst_class,
        class_sum,
        gap_sum,
        span_sum,
        field_sum,
        label_len: entry.label.len(),
    })
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CommandPaletteMatchScore {
    worst_class: usize,
    class_sum: usize,
    gap_sum: usize,
    span_sum: usize,
    field_sum: usize,
    label_len: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CommandPaletteTermScore {
    match_class: usize,
    gap_count: usize,
    span_len: usize,
    field_priority: usize,
}

fn command_palette_field_match_score(
    field: &str,
    term: &str,
    field_priority: usize,
) -> Option<CommandPaletteTermScore> {
    let field = field.to_ascii_lowercase();
    if field == term
        || field
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|word| word == term)
    {
        return Some(CommandPaletteTermScore {
            match_class: 0,
            gap_count: 0,
            span_len: 0,
            field_priority,
        });
    }
    if field.starts_with(term)
        || field
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|word| word.starts_with(term))
    {
        return Some(CommandPaletteTermScore {
            match_class: 1,
            gap_count: 0,
            span_len: 0,
            field_priority,
        });
    }
    if field.contains(term) {
        return Some(CommandPaletteTermScore {
            match_class: 2,
            gap_count: 0,
            span_len: 0,
            field_priority,
        });
    }
    let (gap_count, span_len) = command_palette_fuzzy_span(&field, term)?;
    Some(CommandPaletteTermScore {
        match_class: 3,
        gap_count,
        span_len,
        field_priority,
    })
}

fn command_palette_fuzzy_span(field: &str, term: &str) -> Option<(usize, usize)> {
    let mut first = None;
    let mut last = 0;
    let mut search_start = 0;
    for term_byte in term.bytes() {
        let field_tail = field.as_bytes().get(search_start..)?;
        let offset = field_tail
            .iter()
            .position(|field_byte| *field_byte == term_byte)?;
        let index = search_start + offset;
        first.get_or_insert(index);
        last = index;
        search_start = index + 1;
    }
    let first = first?;
    let span_len = last - first + 1;
    Some((span_len.saturating_sub(term.len()), span_len))
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

/// Add one main-menu item: its `label`, plus — when the entry carries an
/// accelerator — that hint rendered as egui's right-aligned `shortcut_text`.
/// `enabled` greys the item out — every caller passes the item's resolved
/// [`rge_editor_ui::menus::ResolvedEntry::enabled`] (from [`project_main_menu`]).
/// Returns the click [`egui::Response`]. Display-only: the accelerator is a
/// passive hint (the keystroke is routed by editor-shell); activation is the
/// click.
/// Render the command-palette window and return a command if the user activates
/// one this frame.
///
/// The host remains responsible for enqueueing the returned command through
/// [`crate::MenuCommandHandoff`]. This helper owns only palette presentation:
/// filter text, empty state, Enter/Escape handling, click handling, and clearing
/// transient filter state when the palette closes.
pub(crate) fn command_palette_window(
    ctx: &egui::Context,
    open: &mut bool,
    filter: &mut String,
    selected_index: &mut Option<usize>,
    search_focus_requested: &mut bool,
    entries: &[ProjectedCommandPaletteEntry],
    pinned_command_ids: &mut Vec<String>,
    pinned_path: &Path,
    recent_command_ids: &[String],
) -> Option<Command> {
    if !*open {
        *search_focus_requested = false;
        return None;
    }

    let mut selected_command = None;
    let mut close_command_palette = false;
    egui::Window::new("Command Palette")
        .id(egui::Id::new("rge_command_palette"))
        .collapsible(false)
        .resizable(true)
        .default_width(360.0)
        .open(open)
        .show(ctx, |ui| {
            let search_response = ui.add(
                egui::TextEdit::singleline(filter)
                    .id(egui::Id::new("rge_command_palette_search"))
                    .hint_text("Search commands"),
            );
            if take_command_palette_search_focus_request(search_focus_requested) {
                search_response.request_focus();
            }
            let filter_changed = search_response.changed();
            ui.separator();
            let filtered_entries = filter_command_palette_entries_with_pinned_and_recents(
                entries,
                filter.as_str(),
                pinned_command_ids.as_slice(),
                recent_command_ids,
            );
            if filtered_entries.is_empty() {
                ui.label("No commands match");
            }
            *selected_index = command_palette_selected_index_for_filter_change(
                &filtered_entries,
                *selected_index,
                filter_changed,
            );
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                close_command_palette = true;
            } else if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                *selected_index = move_command_palette_selected_index(
                    &filtered_entries,
                    *selected_index,
                    CommandPaletteSelectionDirection::Next,
                );
            } else if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                *selected_index = move_command_palette_selected_index(
                    &filtered_entries,
                    *selected_index,
                    CommandPaletteSelectionDirection::Previous,
                );
            } else if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                selected_command =
                    selected_command_palette_entry(&filtered_entries, *selected_index);
            }
            egui::ScrollArea::vertical()
                .id_salt("rge_command_palette_results")
                .max_height(360.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (index, entry) in filtered_entries.into_iter().enumerate() {
                        let row_selected =
                            command_palette_row_is_selected(entry, index, *selected_index);
                        let response = command_palette_menu_item(
                            ui,
                            entry,
                            row_selected,
                            pinned_command_ids,
                            pinned_path,
                        );
                        if row_selected {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }
                        if response.clicked() {
                            selected_command = Some(entry.command.clone());
                        }
                    }
                });
        });

    if close_command_palette {
        *open = false;
        filter.clear();
        *selected_index = None;
        *search_focus_requested = false;
        return None;
    }
    if let Some(command) = selected_command {
        *open = false;
        filter.clear();
        *selected_index = None;
        *search_focus_requested = false;
        return Some(command);
    }
    if !*open {
        filter.clear();
        *selected_index = None;
        *search_focus_requested = false;
    }
    None
}

pub(crate) fn menu_item(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    shortcut: Option<&str>,
    conflict_peer_entry_ids: &[String],
) -> egui::Response {
    let mut button = egui::Button::new(label);
    if let Some(text) = shortcut {
        button = button.shortcut_text(text);
    }
    if conflict_peer_entry_ids.is_empty() {
        ui.add_enabled(enabled, button)
    } else {
        ui.horizontal(|ui| {
            let response = ui.add_enabled(enabled, button);
            ui.label(egui::RichText::new("Conflict peers:").small());
            ui.monospace(conflict_peer_entry_ids.join(", "));
            response
        })
        .inner
    }
}

pub(crate) fn selected_main_menu_command(
    ui: &mut egui::Ui,
    items: &[ProjectedMainMenuItem],
) -> Option<Command> {
    for item in items {
        if menu_item(
            ui,
            item.enabled,
            item.label.as_str(),
            item.shortcut.as_deref(),
            item.conflict_peer_entry_ids.as_slice(),
        )
        .clicked()
        {
            ui.close();
            return Some(item.command.clone());
        }
    }
    None
}

fn command_palette_menu_item(
    ui: &mut egui::Ui,
    entry: &ProjectedCommandPaletteEntry,
    selected: bool,
    pinned_command_ids: &mut Vec<String>,
    pinned_path: &Path,
) -> egui::Response {
    ui.horizontal(|ui| {
        let pinned = command_palette_entry_is_pinned(entry, pinned_command_ids);
        let pin_label = if pinned { "Unpin" } else { "Pin" };
        if ui.small_button(pin_label).clicked() {
            toggle_command_palette_pinned_command(pinned_command_ids, pinned_path, &entry.command);
        }

        let mut button = egui::Button::new(entry.label.as_str()).selected(selected);
        if let Some(text) = entry.shortcut.as_deref() {
            button = button.shortcut_text(text);
        }
        let response = ui.add_enabled(entry.enabled, button);
        if !entry.conflict_peer_entry_ids.is_empty() {
            ui.label(egui::RichText::new("Conflict peers:").small());
            ui.monospace(entry.conflict_peer_entry_ids.join(", "));
        }
        response
    })
    .inner
}
