//! `lifecycle::accelerator` — keyboard → menu-accelerator bridge (W08.2).
//!
//! [`keycode_to_shortcut`] translates a physical `rge_input::KeyCode` + the
//! Ctrl/Shift modifier flags into an `rge_editor_ui::menus::Shortcut` — the
//! accelerator vocabulary the canonical menu (`default_editor_menu`) is keyed by.
//! It is the shell-local half of accelerator execution: the translation MUST live
//! here because `editor-ui` cannot depend on `rge-input` (`forbidden-dep` rule 4),
//! so editor-shell — which depends on both — owns the bridge.
//!
//! W08.2 added the translation + a PARITY guard; W08.3 made it the live path
//! (`window_event` resolves each un-consumed keystroke to a `Shortcut` via
//! [`keycode_to_shortcut`] and dispatches the menu's bound `Command` through
//! `EditorShell::route_menu_command`); **W08.4 retired the
//! `EditorKeyCommand::{Undo, Redo, Save, SaveAsProject}` mirror**, so the File/Edit
//! keystroke→command literals (Ctrl+O/S/Shift+S/Z/Y) now live ONLY in the
//! canonical menu. `EditorKeyCommand::from_key_press` is left as the executor for
//! the execution-only time-scale binds (`Ctrl+2/0/4`), which have no menu home.
//!
//! The `#[cfg(test)]` guard pins both halves of the cutover: the menu binds the
//! canonical menu accelerators to their commands — the behaviour the live
//! keyboard path executes — AND `from_key_press` no longer claims any of them (so
//! no shadow can silently drift). `Ctrl+Shift+O` is a no-op (the menu binds
//! CTRL-only `Ctrl+O`); the time-scale binds stay execution-only
//! (`from_key_press` `Some`, menu `None`).

use rge_editor_ui::menus::{Key, Modifiers, Shortcut};
use rge_input::KeyCode;

/// Translate a physical [`KeyCode`] + Ctrl/Shift flags into the [`Shortcut`] the
/// canonical menu is keyed by.
///
/// Returns `None` when `key` is itself a modifier key (Ctrl/Shift/Alt/Super have
/// no standalone shortcut form). Letters map to [`Key::Char`] (uppercase), digits
/// to [`Key::Digit`], function keys to [`Key::Function`], and the edit / nav /
/// arrow keys to their named [`Key`] variants.
///
/// `Alt` / `Super` are not represented in the flags today — the accelerator
/// surface is Ctrl/Shift only (the canonical menu's modifier vocabulary; e.g.
/// `Ctrl+Shift+S` = Save-As); extend the signature additively if a bus-bound
/// Alt/Super accelerator lands.
///
/// W08.3 routes the live keyboard path through this translation and the
/// resolved menu's enabled-command lookup: `window_event` resolves a keystroke
/// here, looks up the menu's enabled `Command`, and dispatches it via
/// `EditorShell::route_menu_command` — the same sink the menu bar uses.
#[must_use]
pub fn keycode_to_shortcut(key: KeyCode, ctrl: bool, shift: bool) -> Option<Shortcut> {
    let mut modifiers = Modifiers::empty();
    if ctrl {
        modifiers |= Modifiers::CTRL;
    }
    if shift {
        modifiers |= Modifiers::SHIFT;
    }
    Some(Shortcut::new(modifiers, keycode_to_key(key)?))
}

/// Map a physical [`KeyCode`] to the menu's non-modifier [`Key`]. Returns `None`
/// for the eight modifier keys (they are never the `key` of a [`Shortcut`]).
///
/// Exhaustive over `KeyCode` on purpose: a new physical key added to the input
/// surface forces a deliberate decision here rather than silently mapping to
/// nothing.
fn keycode_to_key(key: KeyCode) -> Option<Key> {
    Some(match key {
        KeyCode::KeyA => Key::Char('A'),
        KeyCode::KeyB => Key::Char('B'),
        KeyCode::KeyC => Key::Char('C'),
        KeyCode::KeyD => Key::Char('D'),
        KeyCode::KeyE => Key::Char('E'),
        KeyCode::KeyF => Key::Char('F'),
        KeyCode::KeyG => Key::Char('G'),
        KeyCode::KeyH => Key::Char('H'),
        KeyCode::KeyI => Key::Char('I'),
        KeyCode::KeyJ => Key::Char('J'),
        KeyCode::KeyK => Key::Char('K'),
        KeyCode::KeyL => Key::Char('L'),
        KeyCode::KeyM => Key::Char('M'),
        KeyCode::KeyN => Key::Char('N'),
        KeyCode::KeyO => Key::Char('O'),
        KeyCode::KeyP => Key::Char('P'),
        KeyCode::KeyQ => Key::Char('Q'),
        KeyCode::KeyR => Key::Char('R'),
        KeyCode::KeyS => Key::Char('S'),
        KeyCode::KeyT => Key::Char('T'),
        KeyCode::KeyU => Key::Char('U'),
        KeyCode::KeyV => Key::Char('V'),
        KeyCode::KeyW => Key::Char('W'),
        KeyCode::KeyX => Key::Char('X'),
        KeyCode::KeyY => Key::Char('Y'),
        KeyCode::KeyZ => Key::Char('Z'),
        KeyCode::Digit0 => Key::Digit(0),
        KeyCode::Digit1 => Key::Digit(1),
        KeyCode::Digit2 => Key::Digit(2),
        KeyCode::Digit3 => Key::Digit(3),
        KeyCode::Digit4 => Key::Digit(4),
        KeyCode::Digit5 => Key::Digit(5),
        KeyCode::Digit6 => Key::Digit(6),
        KeyCode::Digit7 => Key::Digit(7),
        KeyCode::Digit8 => Key::Digit(8),
        KeyCode::Digit9 => Key::Digit(9),
        KeyCode::F1 => Key::Function(1),
        KeyCode::F2 => Key::Function(2),
        KeyCode::F3 => Key::Function(3),
        KeyCode::F4 => Key::Function(4),
        KeyCode::F5 => Key::Function(5),
        KeyCode::F6 => Key::Function(6),
        KeyCode::F7 => Key::Function(7),
        KeyCode::F8 => Key::Function(8),
        KeyCode::F9 => Key::Function(9),
        KeyCode::F10 => Key::Function(10),
        KeyCode::F11 => Key::Function(11),
        KeyCode::F12 => Key::Function(12),
        KeyCode::Space => Key::Space,
        KeyCode::Enter => Key::Enter,
        KeyCode::Escape => Key::Escape,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::ArrowUp => Key::Up,
        KeyCode::ArrowDown => Key::Down,
        KeyCode::ArrowLeft => Key::Left,
        KeyCode::ArrowRight => Key::Right,
        // The eight modifier keys are never a shortcut's `key`.
        KeyCode::ShiftLeft
        | KeyCode::ShiftRight
        | KeyCode::ControlLeft
        | KeyCode::ControlRight
        | KeyCode::AltLeft
        | KeyCode::AltRight
        | KeyCode::SuperLeft
        | KeyCode::SuperRight => return None,
    })
}

#[cfg(test)]
mod tests {
    use rge_editor_ui::menus::{
        default_editor_menu, Command, Key, Modifiers, PredicateContext, Shortcut,
    };
    use rge_input::KeyCode;

    use super::keycode_to_shortcut;
    use crate::EditorKeyCommand;

    #[test]
    fn keycode_to_shortcut_maps_letters_digits_and_no_modifiers() {
        assert_eq!(
            keycode_to_shortcut(KeyCode::KeyO, true, false),
            Some(Shortcut::new(Modifiers::CTRL, Key::Char('O')))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::KeyS, true, true),
            Some(Shortcut::new(
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::Char('S')
            ))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::Digit2, true, false),
            Some(Shortcut::new(Modifiers::CTRL, Key::Digit(2)))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::KeyR, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::Char('R'))),
            "no modifiers held -> a plain shortcut"
        );
    }

    #[test]
    fn keycode_to_shortcut_maps_function_and_nav_keys() {
        assert_eq!(
            keycode_to_shortcut(KeyCode::F5, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::Function(5)))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::ArrowUp, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::Up))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::Home, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::Home))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::PageUp, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::PageUp))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::PageDown, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::PageDown))
        );
        assert_eq!(
            keycode_to_shortcut(KeyCode::Delete, false, false),
            Some(Shortcut::new(Modifiers::empty(), Key::Delete))
        );
    }

    #[test]
    fn keycode_to_shortcut_rejects_modifier_keys() {
        // A modifier key has no standalone shortcut form.
        assert_eq!(keycode_to_shortcut(KeyCode::ControlLeft, true, false), None);
        assert_eq!(keycode_to_shortcut(KeyCode::ShiftRight, false, true), None);
        assert_eq!(keycode_to_shortcut(KeyCode::AltLeft, false, false), None);
        assert_eq!(keycode_to_shortcut(KeyCode::SuperLeft, false, false), None);
    }

    #[test]
    fn keyboard_map_and_menu_agree_on_shared_accelerators() {
        // Post-W08.3 the canonical menu (`default_editor_menu`) is the live
        // keyboard path for canonical menu accelerators. W08.4 retired the
        // File/Edit `EditorKeyCommand` mirror, and the View camera bindings follow
        // the same menu-routed path. This test pins both halves of the
        // cutover: (a) the menu binds each to the expected `Command` (the
        // behaviour the live keyboard path executes via `keycode_to_shortcut` ->
        // `command_for_shortcut`), and (b) `from_key_press` does not claim them
        // (no shadow table is left to drift). `from_key_press` is now reserved for
        // the execution-only time-scale binds.
        let menu = default_editor_menu().resolve(&PredicateContext::default());

        // Canonical menu accelerators -> their menu command, each
        // driven via `keycode_to_shortcut` (the live keyboard translation) ->
        // `command_for_shortcut`.
        let shared = [
            (KeyCode::KeyO, true, false, Command::OpenFile),
            (KeyCode::KeyS, true, false, Command::Save),
            (KeyCode::KeyS, true, true, Command::SaveAs),
            (KeyCode::KeyZ, true, false, Command::Undo),
            (KeyCode::KeyY, true, false, Command::Redo),
            (KeyCode::KeyA, true, false, Command::SelectAll),
            (KeyCode::Delete, false, false, Command::Delete),
            (KeyCode::Home, false, false, Command::ResetCamera),
            (KeyCode::PageUp, false, false, Command::ZoomIn),
            (KeyCode::PageDown, false, false, Command::ZoomOut),
        ];
        for (key, ctrl, shift, menu_command) in shared {
            let shortcut = keycode_to_shortcut(key, ctrl, shift)
                .expect("a shared accelerator translates to a Shortcut");
            assert_eq!(
                menu.command_for_shortcut(&shortcut),
                Some(&menu_command),
                "canonical menu binding for {key:?} ctrl={ctrl} shift={shift}"
            );
            assert_eq!(
                EditorKeyCommand::from_key_press(key, ctrl, shift),
                None,
                "{key:?} ctrl={ctrl} shift={shift} is menu-routed — \
                 EditorKeyCommand must not shadow it after the W08.4 retirement"
            );
        }

        // Time-scale binds (Ctrl+2/0/4) are execution-only: EditorKeyCommand
        // routes them, but they have NO menu entry, so the canonical menu does not
        // bind them. A future menu entry for any of these would (correctly) fail
        // this assertion, forcing the parity question to be answered.
        for (key, key_command) in [
            (KeyCode::Digit2, EditorKeyCommand::SetTimeScaleDoubleSpeed),
            (KeyCode::Digit0, EditorKeyCommand::ResetTimeScaleDefault),
            (
                KeyCode::Digit4,
                EditorKeyCommand::SetTimeScaleMaxFastForward,
            ),
        ] {
            assert_eq!(
                EditorKeyCommand::from_key_press(key, true, false),
                Some(key_command)
            );
            let shortcut = keycode_to_shortcut(key, true, false).unwrap();
            assert_eq!(
                menu.command_for_shortcut(&shortcut),
                None,
                "time-scale binds are execution-only — no menu home"
            );
        }
    }

    #[test]
    fn menu_path_makes_ctrl_o_precise_about_shift() {
        // W08.3 routes Ctrl+O through the canonical menu (which binds the precise
        // CTRL-only accelerator), replacing the old inline `window_event` arm that
        // fired on `key == KeyO && ctrl` — ignoring Shift. Pin the refinement:
        // Ctrl+O resolves to OpenFile, but Ctrl+Shift+O resolves to nothing, so the
        // cutover makes Ctrl+Shift+O a no-op rather than a phantom Open.
        let menu = default_editor_menu().resolve(&PredicateContext::default());

        let ctrl_o = keycode_to_shortcut(KeyCode::KeyO, true, false).unwrap();
        assert_eq!(
            menu.command_for_shortcut(&ctrl_o),
            Some(&Command::OpenFile),
            "Ctrl+O routes to Open via the menu path"
        );

        let ctrl_shift_o = keycode_to_shortcut(KeyCode::KeyO, true, true).unwrap();
        assert_eq!(
            menu.command_for_shortcut(&ctrl_shift_o),
            None,
            "Ctrl+Shift+O is a no-op post-W08.3 — the menu binds CTRL-only Ctrl+O"
        );
    }
}
