//! Host-local keyboard-shortcut help derived from the current projected menu.

use std::collections::BTreeSet;

use crate::menu::{ProjectedMainMenu, ProjectedMenuEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutHelpGroup {
    File,
    Edit,
    Play,
    View,
    Plugins,
}

impl ShortcutHelpGroup {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::Play => "Play",
            Self::View => "View",
            Self::Plugins => "Plugins",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShortcutHelpRow {
    pub group: ShortcutHelpGroup,
    pub label: String,
    pub shortcut: Option<String>,
    pub command_id: String,
    pub enabled: bool,
    pub conflicted: bool,
}

impl ShortcutHelpRow {
    pub(crate) fn state(&self) -> ShortcutHelpRowState {
        if !self.enabled {
            ShortcutHelpRowState::Disabled
        } else if self.conflicted {
            ShortcutHelpRowState::Conflicted
        } else {
            ShortcutHelpRowState::Enabled
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShortcutHelpRowState {
    Enabled,
    Disabled,
    Conflicted,
}

impl ShortcutHelpRowState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Enabled => "Enabled",
            Self::Disabled => "Disabled",
            Self::Conflicted => "Conflicted",
        }
    }
}

pub(crate) fn shortcut_help_rows(main_menu: &ProjectedMainMenu) -> Vec<ShortcutHelpRow> {
    let mut rows = Vec::new();
    let conflicted_shortcuts: BTreeSet<&str> = main_menu
        .conflicts
        .iter()
        .map(|conflict| conflict.shortcut.as_str())
        .collect();
    append_rows(
        &mut rows,
        ShortcutHelpGroup::File,
        &main_menu.file,
        &conflicted_shortcuts,
    );
    append_rows(
        &mut rows,
        ShortcutHelpGroup::Edit,
        &main_menu.edit,
        &conflicted_shortcuts,
    );
    append_rows(
        &mut rows,
        ShortcutHelpGroup::Play,
        &main_menu.play,
        &conflicted_shortcuts,
    );
    append_rows(
        &mut rows,
        ShortcutHelpGroup::View,
        &main_menu.view,
        &conflicted_shortcuts,
    );
    append_rows(
        &mut rows,
        ShortcutHelpGroup::Plugins,
        &main_menu.plugins,
        &conflicted_shortcuts,
    );
    rows
}

pub(crate) fn view_menu_affordance(ui: &mut egui::Ui, open: &mut bool) {
    if ui.button("Keyboard Shortcuts").clicked() {
        toggle_shortcut_help(open);
        ui.close();
    }
}

pub(crate) fn shortcut_help_window(ctx: &egui::Context, open: &mut bool, rows: &[ShortcutHelpRow]) {
    if !*open {
        return;
    }

    egui::Window::new("Keyboard Shortcuts")
        .id(egui::Id::new("rge_shortcut_help"))
        .collapsible(false)
        .resizable(true)
        .default_width(640.0)
        .open(open)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("rge_shortcut_help_rows")
                .max_height(420.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("rge_shortcut_help_grid")
                        .num_columns(5)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Menu");
                            ui.strong("Item");
                            ui.strong("Shortcut");
                            ui.strong("Command ID");
                            ui.strong("State");
                            ui.end_row();

                            for row in rows {
                                ui.label(row.group.label());
                                ui.label(row.label.as_str());
                                ui.monospace(row.shortcut.as_deref().unwrap_or_default());
                                ui.monospace(row.command_id.as_str());
                                ui.label(row.state().label());
                                ui.end_row();
                            }
                        });
                });
        });
}

pub(crate) fn toggle_shortcut_help(open: &mut bool) {
    *open = !*open;
}

fn append_rows(
    rows: &mut Vec<ShortcutHelpRow>,
    group: ShortcutHelpGroup,
    entries: &[ProjectedMenuEntry],
    conflicted_shortcuts: &BTreeSet<&str>,
) {
    rows.extend(entries.iter().map(|(label, shortcut, command, enabled)| {
        ShortcutHelpRow {
            group,
            label: label.clone(),
            shortcut: shortcut.clone(),
            command_id: command.diagnostic_id(),
            enabled: *enabled,
            conflicted: *enabled
                && shortcut
                    .as_deref()
                    .is_some_and(|shortcut| conflicted_shortcuts.contains(shortcut)),
        }
    }));
}

#[cfg(test)]
mod tests {
    use rge_editor_ui::menus::{
        default_editor_menu, plugins_menu_point, Command, Key, MenuEntry, Modifiers,
        PredicateContext, Shortcut,
    };

    use super::*;
    use crate::menu::{
        command_palette_entries, project_main_menu, register_menu_entry, ProjectedMainMenu,
        ProjectedShortcutConflict,
    };
    use crate::MenuCommandHandoff;

    fn row_for<'a>(rows: &'a [ShortcutHelpRow], command: &Command) -> &'a ShortcutHelpRow {
        let id = command.diagnostic_id();
        rows.iter()
            .find(|row| row.command_id == id)
            .expect("shortcut-help row exists for command")
    }

    #[test]
    fn shortcut_help_rows_preserve_group_and_entry_order_from_projected_menu() {
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

        let menu = project_main_menu(&registry, &PredicateContext::default());
        let rows = shortcut_help_rows(&menu);
        let order: Vec<(ShortcutHelpGroup, String)> = rows
            .iter()
            .map(|row| (row.group, row.command_id.clone()))
            .collect();

        assert_eq!(
            order,
            vec![
                (ShortcutHelpGroup::File, Command::NewFile.diagnostic_id()),
                (ShortcutHelpGroup::File, Command::OpenFile.diagnostic_id()),
                (ShortcutHelpGroup::File, Command::Save.diagnostic_id()),
                (ShortcutHelpGroup::File, Command::SaveAs.diagnostic_id()),
                (ShortcutHelpGroup::File, Command::Close.diagnostic_id()),
                (ShortcutHelpGroup::File, Command::Quit.diagnostic_id()),
                (ShortcutHelpGroup::Edit, Command::Undo.diagnostic_id()),
                (ShortcutHelpGroup::Edit, Command::Redo.diagnostic_id()),
                (ShortcutHelpGroup::Edit, Command::SelectAll.diagnostic_id()),
                (ShortcutHelpGroup::Edit, Command::Cut.diagnostic_id()),
                (ShortcutHelpGroup::Edit, Command::Copy.diagnostic_id()),
                (ShortcutHelpGroup::Edit, Command::Paste.diagnostic_id()),
                (ShortcutHelpGroup::Edit, Command::Delete.diagnostic_id()),
                (ShortcutHelpGroup::Edit, Command::Duplicate.diagnostic_id()),
                (ShortcutHelpGroup::Play, Command::PlayStart.diagnostic_id()),
                (ShortcutHelpGroup::Play, Command::PlayPause.diagnostic_id()),
                (ShortcutHelpGroup::Play, Command::PlayStop.diagnostic_id()),
                (ShortcutHelpGroup::Play, Command::PlayStep.diagnostic_id()),
                (
                    ShortcutHelpGroup::View,
                    Command::ToggleCommandPalette.diagnostic_id(),
                ),
                (
                    ShortcutHelpGroup::View,
                    Command::ResetCamera.diagnostic_id()
                ),
                (ShortcutHelpGroup::View, Command::ZoomIn.diagnostic_id()),
                (ShortcutHelpGroup::View, Command::ZoomOut.diagnostic_id()),
                (ShortcutHelpGroup::Plugins, plugin_command.diagnostic_id()),
            ],
            "shortcut help preserves the projected File/Edit/Play/View/Plugins order"
        );
        assert_eq!(row_for(&rows, &Command::Save).label, "Save");
        assert_eq!(row_for(&rows, &plugin_command).label, "Mesh Audit");
    }

    #[test]
    fn shortcut_help_rows_expose_executable_shortcuts_and_command_ids() {
        let menu = project_main_menu(&default_editor_menu(), &PredicateContext::default());
        let rows = shortcut_help_rows(&menu);

        let save = row_for(&rows, &Command::Save);
        assert_eq!(save.group, ShortcutHelpGroup::File);
        assert_eq!(save.shortcut.as_deref(), Some("Ctrl+S"));
        assert_eq!(save.command_id, Command::Save.diagnostic_id());

        let palette = row_for(&rows, &Command::ToggleCommandPalette);
        assert_eq!(palette.group, ShortcutHelpGroup::View);
        assert_eq!(palette.shortcut.as_deref(), Some("Ctrl+Shift+P"));
        assert_eq!(
            palette.command_id,
            Command::ToggleCommandPalette.diagnostic_id()
        );
    }

    #[test]
    fn shortcut_help_rows_include_passive_hints_and_empty_shortcuts() {
        let menu = project_main_menu(&default_editor_menu(), &PredicateContext::default());
        let rows = shortcut_help_rows(&menu);

        assert_eq!(
            row_for(&rows, &Command::PlayStart).shortcut.as_deref(),
            Some("Space"),
            "Play's Space display is a passive projected hint"
        );
        assert_eq!(
            row_for(&rows, &Command::PlayStop).shortcut.as_deref(),
            Some("Escape"),
            "Stop's Escape display is a passive projected hint"
        );
        assert_eq!(
            row_for(&rows, &Command::PlayStep).shortcut,
            None,
            "entries without shortcut display remain visible"
        );
    }

    #[test]
    fn shortcut_help_rows_preserve_disabled_state() {
        let mut ctx = PredicateContext::default();
        ctx.can_pause = true;
        ctx.can_stop = true;
        let menu = project_main_menu(&default_editor_menu(), &ctx);
        let rows = shortcut_help_rows(&menu);

        assert!(
            !row_for(&rows, &Command::Save).enabled,
            "disabled File rows stay present and disabled"
        );
        assert!(
            row_for(&rows, &Command::PlayPause).enabled,
            "enabled Play rows stay enabled"
        );
    }

    #[test]
    fn shortcut_help_rows_mark_enabled_conflicts_from_projected_menu() {
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
            conflicts: vec![ProjectedShortcutConflict {
                shortcut: "Ctrl+S".to_owned(),
                entries: vec!["file.save".to_owned(), "plugin.conflict.save".to_owned()],
            }],
            ..ProjectedMainMenu::default()
        };

        let rows = shortcut_help_rows(&main_menu);

        let save = row_for(&rows, &Command::Save);
        assert!(save.enabled);
        assert!(save.conflicted);
        assert_eq!(save.state(), ShortcutHelpRowState::Conflicted);
        assert_eq!(save.state().label(), "Conflicted");

        let open = row_for(&rows, &Command::OpenFile);
        assert!(open.enabled);
        assert!(!open.conflicted);
        assert_eq!(open.state(), ShortcutHelpRowState::Enabled);
        assert_eq!(open.state().label(), "Enabled");

        let close = row_for(&rows, &Command::Close);
        assert!(!close.enabled);
        assert!(!close.conflicted);
        assert_eq!(close.state(), ShortcutHelpRowState::Disabled);
        assert_eq!(close.state().label(), "Disabled");
    }

    #[test]
    fn shortcut_help_rows_include_projected_plugin_rows_without_registry_resolution() {
        let plugin_command = Command::Plugin {
            plugin_id: "com.example.mesh-audit".to_owned(),
            action_id: "open-panel".to_owned(),
        };
        let main_menu = ProjectedMainMenu {
            plugins: vec![(
                "Mesh Audit".to_owned(),
                Some("Ctrl+Alt+M".to_owned()),
                plugin_command.clone(),
                false,
            )],
            ..ProjectedMainMenu::default()
        };

        assert_eq!(
            shortcut_help_rows(&main_menu),
            vec![ShortcutHelpRow {
                group: ShortcutHelpGroup::Plugins,
                label: "Mesh Audit".to_owned(),
                shortcut: Some("Ctrl+Alt+M".to_owned()),
                command_id: plugin_command.diagnostic_id(),
                enabled: false,
                conflicted: false,
            }],
            "shortcut help consumes only rows already present in ProjectedMainMenu.plugins"
        );
    }

    #[test]
    fn shortcut_help_projection_leaves_command_palette_entries_and_menu_handoff_unchanged() {
        let mut menu = project_main_menu(&default_editor_menu(), &PredicateContext::default());
        menu.conflicts.push(ProjectedShortcutConflict {
            shortcut: "Ctrl+Shift+P".to_owned(),
            entries: vec![
                "view.command_palette".to_owned(),
                "plugin.conflict.command_palette".to_owned(),
            ],
        });
        let palette_before = command_palette_entries(&menu);
        let handoff = MenuCommandHandoff::new();

        let rows = shortcut_help_rows(&menu);
        let palette_after = command_palette_entries(&menu);

        assert!(
            !rows.is_empty(),
            "shortcut help has rows without activating commands"
        );
        assert!(
            row_for(&rows, &Command::ToggleCommandPalette).conflicted,
            "shortcut help consumes projected conflict diagnostics without routing commands"
        );
        assert_eq!(
            palette_after, palette_before,
            "projecting shortcut help does not mutate command-palette projection"
        );
        assert!(
            handoff.drain().is_empty(),
            "projecting shortcut help does not enqueue menu commands"
        );
    }

    #[test]
    fn shortcut_help_toggle_does_not_touch_menu_handoff_or_palette_state() {
        let handoff = MenuCommandHandoff::new();
        let command_palette_open = true;
        let recent_ids = vec![Command::Save.diagnostic_id()];
        let pinned_ids = vec![Command::ToggleCommandPalette.diagnostic_id()];
        let mut shortcut_help_open = false;

        toggle_shortcut_help(&mut shortcut_help_open);
        assert!(shortcut_help_open);
        assert!(command_palette_open, "command palette stays open");
        assert_eq!(recent_ids, vec![Command::Save.diagnostic_id()]);
        assert_eq!(
            pinned_ids,
            vec![Command::ToggleCommandPalette.diagnostic_id()]
        );
        assert!(
            handoff.drain().is_empty(),
            "opening shortcut help does not enqueue menu commands"
        );

        toggle_shortcut_help(&mut shortcut_help_open);
        assert!(!shortcut_help_open);
        assert!(command_palette_open, "command palette still stays open");
        assert!(
            handoff.drain().is_empty(),
            "closing shortcut help does not enqueue menu commands"
        );
    }
}
