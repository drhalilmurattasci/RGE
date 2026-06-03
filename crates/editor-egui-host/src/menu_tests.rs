//! Unit tests for the host's static main-menu wiring: that
//! [`super::build_main_menu_entries`] resolves each extension point
//! (File / Edit / Play / View) to the expected `(label, `[`Command`]`)`
//! list in order, and that each resolved [`Command`] round-trips through
//! the [`super::MenuCommandHandoff`] FIFO.
//!
//! Extracted verbatim from the inline `#[cfg(test)] mod menu_tests` in
//! `lib.rs` (EGUIHOST-TEST-EXTRACTION) so `lib.rs` drops back under the
//! §1.3 Rule 3 1000-line split cap and retires its prior line-cap split
//! annotation. Behaviour-identical — same module path (`super` is the crate
//! root either way), same tests.

use rge_editor_ui::menus::Command;

use super::{build_main_menu_entries, MenuCommandHandoff};

#[test]
fn file_menu_registry_resolves_the_authoring_loop_commands() {
    let (file, _edit, _play, _view) = build_main_menu_entries();
    assert_eq!(
        file,
        vec![
            ("Open…".to_owned(), Command::OpenFile),
            ("Save".to_owned(), Command::Save),
            ("Save As New Project…".to_owned(), Command::SaveAs),
        ],
        "the MenuRegistry resolves the File menu to exactly \
         Open / Save / Save-As-new-project, in order"
    );
}

#[test]
fn edit_menu_registry_resolves_undo_redo_in_order() {
    let (_file, edit, _play, _view) = build_main_menu_entries();
    assert_eq!(
        edit,
        vec![
            ("Undo".to_owned(), Command::Undo),
            ("Redo".to_owned(), Command::Redo),
        ],
        "the MenuRegistry resolves the Edit menu to exactly Undo / Redo, in order"
    );
}

#[test]
fn file_menu_entries_round_trip_through_the_handoff_in_order() {
    let (file, _edit, _play, _view) = build_main_menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, cmd) in file {
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
    let (_file, edit, _play, _view) = build_main_menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, cmd) in edit {
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
    let (_file, _edit, play, _view) = build_main_menu_entries();
    assert_eq!(
        play,
        vec![
            ("Play".to_owned(), Command::PlayStart),
            ("Pause".to_owned(), Command::PlayPause),
            ("Stop".to_owned(), Command::PlayStop),
            ("Step".to_owned(), Command::PlayStep),
        ],
        "the MenuRegistry resolves the Play menu to exactly \
         Play / Pause / Stop / Step, in order"
    );
}

#[test]
fn play_menu_entries_round_trip_through_the_handoff_in_order() {
    let (_file, _edit, play, _view) = build_main_menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, cmd) in play {
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
    let (_file, _edit, _play, view) = build_main_menu_entries();
    assert_eq!(
        view,
        vec![("Reset Camera".to_owned(), Command::ResetCamera)],
        "the MenuRegistry resolves the View menu to exactly Reset Camera"
    );
}

#[test]
fn view_menu_entries_round_trip_through_the_handoff() {
    let (_file, _edit, _play, view) = build_main_menu_entries();
    let handoff = MenuCommandHandoff::new();
    for (_, cmd) in view {
        handoff.push(cmd);
    }
    assert_eq!(
        handoff.drain(),
        vec![Command::ResetCamera],
        "each resolved View item enqueues its Command; they drain FIFO"
    );
}
