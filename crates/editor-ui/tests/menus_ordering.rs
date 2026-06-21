//! Integration tests for the menu registry's ordering / shortcut /
//! predicate surface — exit-criteria coverage for W08.
//!
//! See `tasks/W08/PLAN.md` exit criteria:
//! 1. Declare extension point + register 5 entries with mixed
//!    Before / After / InSection — resolved order matches expected.
//! 2. Shortcut conflict detection.
//! 3. Predicate Closure variant works.

use rge_editor_ui::menus::{
    default_editor_menu, edit_menu_point, Command, EntryId, ExtensionPoint, Key, MenuEntry,
    MenuRegistry, Modifiers, OrderHint, Predicate, PredicateContext, Shortcut,
};

fn entry(id: &str, hint: OrderHint, section: &str) -> MenuEntry {
    let mut e = MenuEntry::new(id, id, Command::Custom(id.into())).with_order_hint(hint);
    if !section.is_empty() {
        e = e.with_section(section);
    }
    e
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
