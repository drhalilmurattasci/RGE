//! Integration tests for the menu registry's ordering / shortcut /
//! predicate surface — exit-criteria coverage for W08.
//!
//! See `tasks/W08/PLAN.md` exit criteria:
//! 1. Declare extension point + register 5 entries with mixed
//!    Before / After / InSection — resolved order matches expected.
//! 2. Shortcut conflict detection.
//! 3. Predicate Closure variant works.

use rge_editor_ui::menus::{
    default_editor_menu, edit_menu_point, file_menu_point, play_menu_point, plugins_menu_point,
    view_menu_point, Command, EntryId, ExtensionPoint, Key, KeybindingDiagnostic,
    KeybindingOverride, KeybindingOverrides, KeybindingTarget, MenuEntry, MenuRegistry, Modifiers,
    OrderHint, Predicate, PredicateContext, Shortcut,
};

fn entry(id: &str, hint: OrderHint, section: &str) -> MenuEntry {
    let mut e = MenuEntry::new(id, id, Command::Custom(id.into())).with_order_hint(hint);
    if !section.is_empty() {
        e = e.with_section(section);
    }
    e
}

fn target(point: ExtensionPoint, entry_id: &str) -> KeybindingTarget {
    KeybindingTarget::new(point, entry_id)
}

fn menu_signature(
    resolved: &rge_editor_ui::menus::registry::ResolveResult,
    point: &ExtensionPoint,
) -> Vec<(String, String, String, Option<String>, Option<String>, bool)> {
    resolved
        .entries_for(point)
        .iter()
        .map(|r| {
            (
                r.entry.id.as_str().to_owned(),
                r.entry.label.clone(),
                r.entry.command.diagnostic_id(),
                r.entry.shortcut.as_ref().map(Shortcut::display),
                r.entry.shortcut_hint.as_ref().map(Shortcut::display),
                r.enabled,
            )
        })
        .collect()
}

/// Exit criterion: register 5 entries with mixed `Before` / `After` /
/// `InSection` hints into a single extension point; resolve and assert
/// the resulting order is the one the algorithm specifies.
///
/// Layout of the input (registration order):
/// 1. `file.open`   — `AtStart`,   default section
/// 2. `file.exit`   — `AtEnd`,     default section
/// 3. `file.save`   — `After(file.open)`, default section
/// 4. `file.recent` — `Before(file.exit)`, default section
/// 5. `file.export` — `InSection("primary")`
///
/// Expected resolve:
/// - default-section bucket (first seen): `file.open` (AtStart),
///   `file.save` (after open), `file.recent` (before exit),
///   `file.exit` (AtEnd).
/// - `primary` section bucket (seen second): `file.export`.
///
/// So the full order is:
/// `file.open, file.save, file.recent, file.exit, file.export`.
#[test]
fn exit_criterion_five_entries_mixed_order_hints() {
    let mut r = MenuRegistry::new();
    let p = ExtensionPoint::new("editor.main_menu.file");
    r.declare_extension_point(p.clone()).unwrap();

    r.register_entry(&p, entry("file.open", OrderHint::AtStart, ""))
        .unwrap();
    r.register_entry(&p, entry("file.exit", OrderHint::AtEnd, ""))
        .unwrap();
    r.register_entry(
        &p,
        entry("file.save", OrderHint::After(EntryId::new("file.open")), ""),
    )
    .unwrap();
    r.register_entry(
        &p,
        entry(
            "file.recent",
            OrderHint::Before(EntryId::new("file.exit")),
            "",
        ),
    )
    .unwrap();
    r.register_entry(
        &p,
        entry("file.export", OrderHint::InSection("primary".into()), ""),
    )
    .unwrap();

    let res = r.resolve(&PredicateContext::default());
    let ids: Vec<&str> = res
        .entries_for(&p)
        .iter()
        .map(|r| r.entry.id.as_str())
        .collect();

    assert_eq!(
        ids,
        vec![
            "file.open",
            "file.save",
            "file.recent",
            "file.exit",
            "file.export",
        ],
        "five-entry mixed-hint resolve must match the order in the doc \
         comment exactly (default section first; primary section second)",
    );
}

/// Exit criterion: shortcut conflict detection.
///
/// Two entries register the same `Ctrl+S`. Resolve must surface
/// exactly one [`ShortcutConflict`] containing both entry ids in
/// registration order, and the accelerator table must still resolve
/// the keystroke to *something* (the first registration wins).
#[test]
fn exit_criterion_shortcut_conflict_detection() {
    let mut r = MenuRegistry::new();
    let p = ExtensionPoint::new("editor.main_menu.file");
    r.declare_extension_point(p.clone()).unwrap();

    let s = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
    r.register_entry(
        &p,
        MenuEntry::new("file.save", "Save", Command::Save).with_shortcut(s.clone()),
    )
    .unwrap();
    r.register_entry(
        &p,
        MenuEntry::new(
            "plugin.foo.alt_save",
            "Foo Save",
            Command::Custom("foo.save".into()),
        )
        .with_shortcut(s.clone()),
    )
    .unwrap();

    let res = r.resolve(&PredicateContext::default());
    assert_eq!(res.conflicts.len(), 1, "exactly one conflict expected");
    assert_eq!(res.conflicts[0].shortcut, s);
    let entry_ids: Vec<&str> = res.conflicts[0]
        .entries
        .iter()
        .map(|e| e.as_str())
        .collect();
    assert_eq!(
        entry_ids,
        vec!["file.save", "plugin.foo.alt_save"],
        "conflict entries must list registrations in registration order",
    );

    // The accelerator table still routes the keystroke to the first
    // registration so the editor remains operable in the presence of
    // a conflict (the host surfaces the conflict diagnostic
    // separately).
    let bound = res
        .accelerator_table
        .resolve(&s)
        .expect("conflict-bound shortcut still resolves to first entry")
        .as_str();
    assert_eq!(bound, "file.save");
    assert_eq!(
        res.command_for_shortcut(&s),
        Some(&Command::Save),
        "display/introspection lookup keeps the first registered winner",
    );
    assert_eq!(
        res.enabled_command_for_shortcut(&s),
        None,
        "keyboard execution suppresses live conflicted shortcuts",
    );
}

#[test]
fn unconflicted_enabled_shortcut_executes() {
    let mut r = MenuRegistry::new();
    let p = ExtensionPoint::new("editor.main_menu.file");
    r.declare_extension_point(p.clone()).unwrap();

    let s = Shortcut::new(Modifiers::CTRL, Key::Char('O'));
    r.register_entry(
        &p,
        MenuEntry::new("file.open", "Open", Command::OpenFile).with_shortcut(s.clone()),
    )
    .unwrap();

    let res = r.resolve(&PredicateContext::default());
    assert!(res.conflicts.is_empty());
    assert_eq!(res.command_for_shortcut(&s), Some(&Command::OpenFile));
    assert_eq!(
        res.enabled_command_for_shortcut(&s),
        Some(&Command::OpenFile),
        "unconflicted enabled shortcuts remain executable",
    );
}

#[test]
fn disabled_visible_shortcut_stays_bound_but_does_not_execute() {
    let mut r = MenuRegistry::new();
    let p = ExtensionPoint::new("editor.main_menu.file");
    r.declare_extension_point(p.clone()).unwrap();

    let s = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
    r.register_entry(
        &p,
        MenuEntry::new("file.save", "Save", Command::Save)
            .with_shortcut(s.clone())
            .with_enabled(Predicate::from_fn(|c| c.is_editing)),
    )
    .unwrap();

    let res = r.resolve(&PredicateContext::default());
    let entries = res.entries_for(&p);
    assert_eq!(entries.len(), 1, "disabled entries stay visible");
    assert!(!entries[0].enabled);
    assert_eq!(
        res.command_for_shortcut(&s),
        Some(&Command::Save),
        "display/introspection lookup keeps disabled-visible bindings",
    );
    assert_eq!(
        res.enabled_command_for_shortcut(&s),
        None,
        "disabled-visible bindings do not execute",
    );

    let mut ctx = PredicateContext::default();
    ctx.is_editing = true;
    let res = r.resolve(&ctx);
    assert_eq!(res.enabled_command_for_shortcut(&s), Some(&Command::Save));
}

#[test]
fn hidden_entries_release_their_shortcut_for_visible_entries() {
    let mut r = MenuRegistry::new();
    let p = ExtensionPoint::new("editor.main_menu.edit");
    r.declare_extension_point(p.clone()).unwrap();

    let s = Shortcut::new(Modifiers::CTRL, Key::Char('D'));
    r.register_entry(
        &p,
        MenuEntry::new("edit.hidden_delete", "Hidden Delete", Command::Delete)
            .with_shortcut(s.clone())
            .with_visible(false),
    )
    .unwrap();
    r.register_entry(
        &p,
        MenuEntry::new("edit.duplicate", "Duplicate", Command::Duplicate).with_shortcut(s.clone()),
    )
    .unwrap();

    let res = r.resolve(&PredicateContext::default());
    assert!(res.conflicts.is_empty());
    let ids: Vec<&str> = res
        .entries_for(&p)
        .iter()
        .map(|r| r.entry.id.as_str())
        .collect();
    assert_eq!(ids, vec!["edit.duplicate"]);
    assert_eq!(
        res.accelerator_table.resolve(&s).map(|id| id.as_str()),
        Some("edit.duplicate"),
    );
    assert_eq!(res.command_for_shortcut(&s), Some(&Command::Duplicate));
    assert_eq!(
        res.enabled_command_for_shortcut(&s),
        Some(&Command::Duplicate),
    );
}

/// Exit criterion: predicate `Closure` variant works.
///
/// Register one entry whose visibility predicate keys off
/// `PredicateContext::has_selection`. With the default context the
/// entry must be filtered out; flipping the bit must surface it.
#[test]
fn exit_criterion_predicate_closure_works() {
    let mut r = MenuRegistry::new();
    let p = ExtensionPoint::new("editor.main_menu.edit");
    r.declare_extension_point(p.clone()).unwrap();

    r.register_entry(
        &p,
        MenuEntry::new("edit.delete", "Delete", Command::Delete)
            .with_predicate(Predicate::from_fn(|c| c.has_selection)),
    )
    .unwrap();

    let mut ctx = PredicateContext::default();

    // Default context: predicate fails → entry filtered out.
    let res = r.resolve(&ctx);
    assert!(
        res.entries_for(&p).is_empty(),
        "predicate Closure must remove entry when callback returns false",
    );

    // Activate selection: predicate passes → entry surfaces.
    ctx.has_selection = true;
    let res = r.resolve(&ctx);
    let ids: Vec<&str> = res
        .entries_for(&p)
        .iter()
        .map(|r| r.entry.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["edit.delete"],
        "predicate Closure must surface entry when callback returns true",
    );
}

/// Bonus: shortcuts registered against entries that the predicate
/// filters out must NOT enter the accelerator table — a hidden entry
/// can never own a keystroke.
#[test]
fn predicate_filtered_entries_release_their_shortcut() {
    let mut r = MenuRegistry::new();
    let p = ExtensionPoint::new("editor.main_menu.edit");
    r.declare_extension_point(p.clone()).unwrap();

    let s = Shortcut::new(Modifiers::CTRL, Key::Char('D'));
    r.register_entry(
        &p,
        MenuEntry::new("edit.delete", "Delete", Command::Delete)
            .with_shortcut(s.clone())
            .with_predicate(Predicate::from_fn(|c| c.has_selection)),
    )
    .unwrap();
    r.register_entry(
        &p,
        MenuEntry::new("edit.duplicate", "Duplicate", Command::Duplicate).with_shortcut(s.clone()),
    )
    .unwrap();

    let res = r.resolve(&PredicateContext::default());
    assert!(res.conflicts.is_empty());
    assert_eq!(
        res.accelerator_table.resolve(&s).map(|id| id.as_str()),
        Some("edit.duplicate"),
        "hidden-by-predicate entry must not occupy the shortcut slot \
         a visible entry can claim",
    );
    assert_eq!(
        res.enabled_command_for_shortcut(&s),
        Some(&Command::Duplicate),
    );
}

#[test]
fn shortcut_conflicts_are_reported_in_deterministic_display_order() {
    let mut r = MenuRegistry::new();
    let p = ExtensionPoint::new("editor.main_menu.tools");
    r.declare_extension_point(p.clone()).unwrap();

    let ctrl_b = Shortcut::new(Modifiers::CTRL, Key::Char('B'));
    let ctrl_a = Shortcut::new(Modifiers::CTRL, Key::Char('A'));
    for (id, shortcut) in [
        ("tool.b.first", ctrl_b.clone()),
        ("tool.b.second", ctrl_b.clone()),
        ("tool.a.first", ctrl_a.clone()),
        ("tool.a.second", ctrl_a.clone()),
    ] {
        r.register_entry(
            &p,
            MenuEntry::new(id, id, Command::Custom(id.into())).with_shortcut(shortcut),
        )
        .unwrap();
    }

    let res = r.resolve(&PredicateContext::default());
    let conflicts: Vec<(String, Vec<&str>)> = res
        .conflicts
        .iter()
        .map(|conflict| {
            (
                conflict.shortcut.display(),
                conflict
                    .entries
                    .iter()
                    .map(|entry| entry.as_str())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        conflicts,
        vec![
            ("Ctrl+A".to_owned(), vec!["tool.a.first", "tool.a.second"],),
            ("Ctrl+B".to_owned(), vec!["tool.b.first", "tool.b.second"],),
        ],
        "conflicts sort by shortcut display while entries keep registration order",
    );
}

/// Edge case: missing `Before` target degrades to `AtEnd` — a plugin
/// targeting an entry that the host hasn't shipped should still place
/// its entry safely (at the end of the section) rather than vanish or
/// panic.
#[test]
fn missing_before_target_degrades_to_at_end() {
    let mut r = MenuRegistry::new();
    let p = ExtensionPoint::new("a");
    r.declare_extension_point(p.clone()).unwrap();
    r.register_entry(&p, entry("first", OrderHint::AtStart, ""))
        .unwrap();
    r.register_entry(
        &p,
        entry(
            "orphan",
            OrderHint::Before(EntryId::new("does.not.exist")),
            "",
        ),
    )
    .unwrap();
    let res = r.resolve(&PredicateContext::default());
    let ids: Vec<&str> = res
        .entries_for(&p)
        .iter()
        .map(|r| r.entry.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["first", "orphan"],
        "missing Before target must degrade to AtEnd in the same section",
    );
}

#[test]
fn default_edit_menu_contains_ctrl_shift_delete_current_cad_cuboid_delete() {
    let mut ctx = PredicateContext::default();
    ctx.is_editing = true;
    ctx.has_current_cad_cuboid_selection = true;

    let resolved = default_editor_menu().resolve(&ctx);
    let edit = resolved.entries_for(&edit_menu_point());
    let entry = edit
        .iter()
        .find(|r| r.entry.command == Command::DeleteCurrentCadCuboid)
        .expect("default Edit menu has a dedicated CAD delete command");

    assert_eq!(entry.entry.id.as_str(), "edit.delete_current_cad_cuboid");
    assert_eq!(entry.entry.label, "Delete Current CAD Cuboid");
    assert!(entry.enabled);
    let cad_delete = Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Delete);
    assert_eq!(entry.entry.shortcut.as_ref(), Some(&cad_delete));
    assert!(entry.entry.shortcut_hint.is_none());
    assert_eq!(
        resolved.command_for_shortcut(&cad_delete),
        Some(&Command::DeleteCurrentCadCuboid),
        "Ctrl+Shift+Delete resolves only to the dedicated CAD delete command"
    );
    assert_eq!(
        resolved.enabled_command_for_shortcut(&cad_delete),
        Some(&Command::DeleteCurrentCadCuboid),
        "Ctrl+Shift+Delete executes while the exact current CAD cuboid predicate is true"
    );
    assert_eq!(
        resolved.command_for_shortcut(&Shortcut::plain(Key::Delete)),
        Some(&Command::Delete),
        "bare Delete keeps the generic Delete accelerator"
    );
    assert!(
        resolved
            .accelerator_table
            .resolve(&Shortcut::plain(Key::Delete))
            .is_some(),
        "generic Delete keeps the Delete accelerator"
    );

    ctx.has_current_cad_cuboid_selection = false;
    let disabled = default_editor_menu().resolve(&ctx);
    let disabled_entry = disabled
        .entries_for(&edit_menu_point())
        .iter()
        .find(|r| r.entry.command == Command::DeleteCurrentCadCuboid)
        .expect("disabled entries stay visible");
    assert!(!disabled_entry.enabled);
    assert_eq!(
        disabled.enabled_command_for_shortcut(&cad_delete),
        None,
        "Ctrl+Shift+Delete is withheld when the exact current CAD cuboid predicate is false"
    );
}

#[test]
fn empty_keybinding_overrides_preserve_default_resolve_behavior() {
    let registry = default_editor_menu();
    let mut ctx = PredicateContext::default();
    ctx.is_editing = true;
    ctx.has_selection = true;
    ctx.has_selectable_entities = true;
    ctx.has_clipboard_entities = true;
    ctx.has_current_cad_cuboid_selection = true;

    let normal = registry.resolve(&ctx);
    let overridden =
        registry.resolve_with_keybinding_overrides(&ctx, &KeybindingOverrides::default());

    assert_eq!(normal.accelerator_table.len(), 19);
    assert!(normal.conflicts.is_empty());
    assert!(overridden.keybinding_diagnostics.is_empty());
    assert_eq!(
        overridden.accelerator_table.len(),
        normal.accelerator_table.len()
    );
    assert_eq!(overridden.conflicts, normal.conflicts);

    for point in [
        file_menu_point(),
        edit_menu_point(),
        play_menu_point(),
        view_menu_point(),
        plugins_menu_point(),
    ] {
        assert_eq!(
            menu_signature(&overridden, &point),
            menu_signature(&normal, &point),
            "empty overrides must preserve the resolved tree for {point}"
        );
    }

    for shortcut in [
        Shortcut::new(Modifiers::CTRL, Key::Char('O')),
        Shortcut::new(Modifiers::CTRL, Key::Char('S')),
        Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Delete),
        Shortcut::plain(Key::PageDown),
    ] {
        assert_eq!(
            overridden.command_for_shortcut(&shortcut),
            normal.command_for_shortcut(&shortcut),
            "empty overrides preserve display lookup for {}",
            shortcut.display()
        );
        assert_eq!(
            overridden.enabled_command_for_shortcut(&shortcut),
            normal.enabled_command_for_shortcut(&shortcut),
            "empty overrides preserve execution lookup for {}",
            shortcut.display()
        );
    }
}

#[test]
fn keybinding_noop_remap_reports_diagnostic_without_changing_resolve() {
    let registry = default_editor_menu();
    let mut ctx = PredicateContext::default();
    ctx.is_editing = true;
    ctx.has_selection = true;
    ctx.has_selectable_entities = true;
    ctx.has_clipboard_entities = true;
    ctx.has_current_cad_cuboid_selection = true;

    let default_shortcut = Shortcut::new(Modifiers::CTRL, Key::Char('O'));
    let remap_target = target(file_menu_point(), "file.open");
    let overrides = KeybindingOverrides::from_overrides([KeybindingOverride::remap(
        remap_target.clone(),
        default_shortcut.clone(),
    )]);

    let baseline = registry.resolve(&ctx);
    let resolved = registry.resolve_with_keybinding_overrides(&ctx, &overrides);

    assert_eq!(
        resolved.keybinding_diagnostics,
        vec![KeybindingDiagnostic::NoOpRemap {
            target: remap_target,
            shortcut: default_shortcut.clone(),
        }]
    );
    assert_eq!(
        resolved.accelerator_table.len(),
        baseline.accelerator_table.len()
    );
    assert_eq!(resolved.conflicts, baseline.conflicts);
    assert_eq!(
        resolved.command_for_shortcut(&default_shortcut),
        Some(&Command::OpenFile)
    );
    assert_eq!(
        resolved.enabled_command_for_shortcut(&default_shortcut),
        Some(&Command::OpenFile)
    );
    for point in [
        file_menu_point(),
        edit_menu_point(),
        play_menu_point(),
        view_menu_point(),
        plugins_menu_point(),
    ] {
        assert_eq!(
            menu_signature(&resolved, &point),
            menu_signature(&baseline, &point),
            "no-op remaps must not change the resolved menu tree for {point}"
        );
    }
}

#[test]
fn keybinding_remap_applies_to_one_resolve_without_mutating_registry() {
    let registry = default_editor_menu();
    let mut ctx = PredicateContext::default();
    ctx.is_editing = true;
    let default_shortcut = Shortcut::new(Modifiers::CTRL, Key::Char('O'));
    let remapped_shortcut = Shortcut::new(Modifiers::CTRL | Modifiers::ALT, Key::Char('O'));
    let overrides = KeybindingOverrides::from_overrides([KeybindingOverride::remap(
        target(file_menu_point(), "file.open"),
        remapped_shortcut.clone(),
    )]);

    let remapped = registry.resolve_with_keybinding_overrides(&ctx, &overrides);
    let file_open = remapped
        .entries_for(&file_menu_point())
        .iter()
        .find(|r| r.entry.id.as_str() == "file.open")
        .expect("File/Open remains visible");

    assert_eq!(file_open.entry.shortcut.as_ref(), Some(&remapped_shortcut));
    assert_eq!(
        remapped
            .accelerator_table
            .resolve(&remapped_shortcut)
            .map(|id| id.as_str()),
        Some("file.open")
    );
    assert_eq!(
        remapped.command_for_shortcut(&remapped_shortcut),
        Some(&Command::OpenFile)
    );
    assert_eq!(
        remapped.enabled_command_for_shortcut(&remapped_shortcut),
        Some(&Command::OpenFile)
    );
    assert_eq!(remapped.command_for_shortcut(&default_shortcut), None);
    assert!(remapped.keybinding_diagnostics.is_empty());

    let normal_after = registry.resolve(&ctx);
    let normal_file_open = normal_after
        .entries_for(&file_menu_point())
        .iter()
        .find(|r| r.entry.id.as_str() == "file.open")
        .expect("File/Open remains visible");
    assert_eq!(
        normal_file_open.entry.shortcut.as_ref(),
        Some(&default_shortcut),
        "override must not mutate the registry or later normal resolves"
    );
    assert_eq!(
        normal_after.command_for_shortcut(&default_shortcut),
        Some(&Command::OpenFile)
    );
}

#[test]
fn keybinding_unbind_removes_executable_shortcut_but_keeps_entry_visible() {
    let registry = default_editor_menu();
    let mut ctx = PredicateContext::default();
    ctx.is_editing = true;
    ctx.has_current_cad_cuboid_selection = true;
    let cad_delete = Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Delete);
    let overrides = KeybindingOverrides::from_overrides([KeybindingOverride::unbind(target(
        edit_menu_point(),
        "edit.delete_current_cad_cuboid",
    ))]);

    let unbound = registry.resolve_with_keybinding_overrides(&ctx, &overrides);
    let entry = unbound
        .entries_for(&edit_menu_point())
        .iter()
        .find(|r| r.entry.id.as_str() == "edit.delete_current_cad_cuboid")
        .expect("unbinding keeps the menu entry visible");

    assert_eq!(entry.entry.label, "Delete Current CAD Cuboid");
    assert_eq!(entry.entry.command, Command::DeleteCurrentCadCuboid);
    assert!(entry.enabled);
    assert!(entry.entry.shortcut.is_none());
    assert!(entry.entry.shortcut_hint.is_none());
    assert_eq!(unbound.accelerator_table.resolve(&cad_delete), None);
    assert_eq!(unbound.command_for_shortcut(&cad_delete), None);
    assert_eq!(unbound.enabled_command_for_shortcut(&cad_delete), None);
    assert_eq!(
        unbound.accelerator_table.len(),
        18,
        "unbinding only removes the executable accelerator"
    );
    assert!(unbound.keybinding_diagnostics.is_empty());
}

#[test]
fn keybinding_redundant_unbind_reports_diagnostic_without_changing_visible_entry() {
    let mut registry = MenuRegistry::new();
    let point = ExtensionPoint::new("editor.main_menu.view");
    registry.declare_extension_point(point.clone()).unwrap();
    registry
        .register_entry(
            &point,
            MenuEntry::new(
                "view.zoom_to_fit",
                "Zoom to Fit",
                Command::Custom("view.zoom_to_fit".into()),
            ),
        )
        .unwrap();
    let unbind_target = target(point.clone(), "view.zoom_to_fit");
    let overrides =
        KeybindingOverrides::from_overrides([KeybindingOverride::unbind(unbind_target.clone())]);

    let baseline = registry.resolve(&PredicateContext::default());
    let resolved =
        registry.resolve_with_keybinding_overrides(&PredicateContext::default(), &overrides);

    assert_eq!(
        resolved.keybinding_diagnostics,
        vec![KeybindingDiagnostic::RedundantUnbind {
            target: unbind_target,
        }]
    );
    assert_eq!(
        menu_signature(&resolved, &point),
        menu_signature(&baseline, &point)
    );
    assert_eq!(resolved.entries_for(&point).len(), 1);
    assert!(resolved.entries_for(&point)[0].entry.shortcut.is_none());
    assert!(resolved.accelerator_table.is_empty());
    assert!(resolved.conflicts.is_empty());
}

#[test]
fn keybinding_remap_conflict_is_nonfatal_and_suppresses_execution() {
    let registry = default_editor_menu();
    let mut ctx = PredicateContext::default();
    ctx.is_editing = true;
    let ctrl_s = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
    let overrides = KeybindingOverrides::from_overrides([KeybindingOverride::remap(
        target(file_menu_point(), "file.open"),
        ctrl_s.clone(),
    )]);

    let resolved = registry.resolve_with_keybinding_overrides(&ctx, &overrides);

    assert_eq!(resolved.conflicts.len(), 1);
    assert_eq!(resolved.conflicts[0].shortcut, ctrl_s);
    let conflict_ids: Vec<&str> = resolved.conflicts[0]
        .entries
        .iter()
        .map(|entry| entry.as_str())
        .collect();
    assert_eq!(
        conflict_ids,
        vec!["file.open", "file.save"],
        "override-induced conflicts keep deterministic resolve order"
    );
    assert_eq!(
        resolved.command_for_shortcut(&ctrl_s),
        Some(&Command::OpenFile),
        "display lookup keeps the first resolved winner"
    );
    assert_eq!(
        resolved.enabled_command_for_shortcut(&ctrl_s),
        None,
        "execution lookup suppresses conflicted shortcuts"
    );
    assert!(resolved.keybinding_diagnostics.is_empty());
}

#[test]
fn unknown_keybinding_target_reports_diagnostic_without_changing_resolve() {
    let registry = default_editor_menu();
    let mut ctx = PredicateContext::default();
    ctx.is_editing = true;
    ctx.has_selection = true;
    ctx.has_selectable_entities = true;
    ctx.has_clipboard_entities = true;
    let unknown_target = target(edit_menu_point(), "edit.missing");
    let overrides = KeybindingOverrides::from_overrides([KeybindingOverride::remap(
        unknown_target.clone(),
        Shortcut::new(Modifiers::CTRL | Modifiers::ALT, Key::Char('M')),
    )]);

    let baseline = registry.resolve(&ctx);
    let resolved = registry.resolve_with_keybinding_overrides(&ctx, &overrides);

    assert_eq!(
        resolved.keybinding_diagnostics,
        vec![KeybindingDiagnostic::UnknownTarget {
            target: unknown_target
        }]
    );
    assert_eq!(
        resolved.accelerator_table.len(),
        baseline.accelerator_table.len()
    );
    assert_eq!(resolved.conflicts, baseline.conflicts);
    for point in [
        file_menu_point(),
        edit_menu_point(),
        play_menu_point(),
        view_menu_point(),
        plugins_menu_point(),
    ] {
        assert_eq!(
            menu_signature(&resolved, &point),
            menu_signature(&baseline, &point),
            "unknown targets must not change the resolved menu tree for {point}"
        );
    }
}

#[test]
fn known_hidden_keybinding_target_does_not_emit_unknown_diagnostic() {
    let mut registry = MenuRegistry::new();
    let point = ExtensionPoint::new("editor.main_menu.hidden_test");
    registry.declare_extension_point(point.clone()).unwrap();
    registry
        .register_entry(
            &point,
            MenuEntry::new("hidden.entry", "Hidden", Command::Custom("hidden".into()))
                .with_shortcut(Shortcut::new(Modifiers::CTRL, Key::Char('H')))
                .with_visible(false),
        )
        .unwrap();
    let overrides = KeybindingOverrides::from_overrides([KeybindingOverride::remap(
        target(point.clone(), "hidden.entry"),
        Shortcut::new(Modifiers::CTRL, Key::Char('J')),
    )]);

    let resolved =
        registry.resolve_with_keybinding_overrides(&PredicateContext::default(), &overrides);

    assert!(resolved.entries_for(&point).is_empty());
    assert!(resolved.accelerator_table.is_empty());
    assert!(resolved.conflicts.is_empty());
    assert!(
        resolved.keybinding_diagnostics.is_empty(),
        "registered targets are known even when visibility filtering hides them"
    );
}
