//! In-app "Save" — Ctrl+S handler + save-dialog / scene-write traits.
//!
//! The save-axis companion to [`open_request`](super::open_request) (the
//! `Ctrl+O` Open axis). `Ctrl+S` reaches [`EditorShell::handle_save_request`]
//! via the [`MarkSaved`](super::EditorKeyCommand::MarkSaved) arm of
//! [`EditorShell::handle_key_command`]: the physical-key decode already maps
//! `Ctrl+S → MarkSaved`, and SCENE-SAVE-WIRING repoints that arm from a pure
//! `mark_saved` bookmark to a real Save-to-disk.
//!
//! # Design (mirrors the Open hook split)
//!
//! - **Save-As only (v0).** Every `Ctrl+S` prompts for a `*.rge-scene`
//!   destination via the binary-owned [`SceneSaveDialog`], writes the live
//!   `World` through the binary-owned [`SceneSaveHook`]
//!   (`rge_scene_loader::save_scene_world_to_path`), and — only on success —
//!   marks the Command-Bus saved point ([`EditorShell::mark_saved_command`]),
//!   clearing `is_dirty()`. There is no `scene_source_path` memory and no
//!   silent overwrite yet (a follow-up).
//!
//! - **editor-shell owns no file-system / loader edge.** The dialog impl owns
//!   the `rfd` dependency; the writer impl owns `rge-scene-loader`. editor-shell
//!   holds only the boxed `dyn` trait objects and calls through them — it never
//!   gains an `rfd`, `rge-scene-loader`, or `rge-data` dependency. Mirrors
//!   [`GlbOpenDialog`](super::GlbOpenDialog) /
//!   [`SceneOpenHook`](super::SceneOpenHook) exactly.
//!
//! - **Editing-gated.** Save only fires in [`PlayState::Editing`], mirroring the
//!   `Ctrl+O` / R-key reload PIE gate: a mid-Play Save would persist the
//!   transient play-state world, not the edit world.
//!
//! - **Mark-saved only on success.** Cancel, missing dialog, missing hook, and
//!   write-error paths all log and leave `command_bus().is_dirty()` untouched.

use std::path::PathBuf;

use crate::lifecycle::EditorShell;
use crate::play_state::PlayState;

/// Save-destination dialog for in-app Save (`Ctrl+S`) — the save-axis
/// companion to [`GlbOpenDialog`](super::GlbOpenDialog).
///
/// The editor binary (`rge-editor::main`) impls this with `rfd`
/// (`rfd::FileDialog::new().add_filter(..).save_file()`) and hands an instance
/// to [`EditorShell`] at construction via
/// [`EditorShell::with_scene_save_dialog`]. Keeping the impl in the binary
/// leaves editor-shell free of any `rfd` dependency — the shell holds only a
/// `Box<dyn SceneSaveDialog>` and calls [`Self::pick_save_path`] when `Ctrl+S`
/// is pressed.
///
/// `&self` (not `&mut self`) because the dialog is stateless — each invocation
/// spawns a fresh native dialog. A future stateful dialog (last-directory
/// memory) can promote this to `&mut self` without churning the call site.
pub trait SceneSaveDialog {
    /// Prompt the user for a `*.rge-scene` save destination. Returns
    /// `Some(path)` when the user chose a file, `None` when the dialog was
    /// cancelled (in which case the handler mutates no editor state).
    fn pick_save_path(&self) -> Option<PathBuf>;
}

/// Writer-callback for in-app Save — the save-axis companion to
/// [`SceneOpenHook`](super::SceneOpenHook).
///
/// The editor binary (`rge-editor::main`) impls this over
/// `rge_scene_loader::save_scene_world_to_path` and hands an instance to
/// [`EditorShell`] via [`EditorShell::with_scene_save_hook`]. Keeping the impl
/// in the binary leaves editor-shell free of any `rge-scene-loader` /
/// `rge-data` dependency — the shell holds only a `Box<dyn SceneSaveHook>` and
/// calls [`Self::save_scene_world`] when the user saves. The hook owns
/// `Scene.name` derivation (v0: the chosen file stem).
///
/// `&self` (not `&mut self`) because the writer is stateless — every save
/// re-extracts from the live world and writes afresh.
pub trait SceneSaveHook {
    /// Write `world` to `path` as a `.rge-scene`.
    ///
    /// On any extension / serialize / I/O failure, return `Err(message)`: the
    /// `Ctrl+S` handler warn-logs it and does NOT mark the bus saved. On `Ok`,
    /// the handler marks the Command-Bus saved point (clearing `is_dirty()`).
    fn save_scene_world(
        &self,
        world: &rge_kernel_ecs::World,
        path: &std::path::Path,
    ) -> Result<(), String>;
}

impl EditorShell {
    /// `Ctrl+S` handler — invoked from the
    /// [`MarkSaved`](super::EditorKeyCommand::MarkSaved) arm of
    /// [`Self::handle_key_command`]. Prompts via the [`SceneSaveDialog`] stashed
    /// by [`Self::with_scene_save_dialog`], writes the live `World` through the
    /// [`SceneSaveHook`] stashed by [`Self::with_scene_save_hook`], and marks
    /// the Command-Bus saved point **only** on a successful write.
    ///
    /// Save-As only (v0): every call prompts; there is no `scene_source_path`
    /// and no silent overwrite.
    ///
    /// All failure paths log and no-op (the bus saved point / `is_dirty()` is
    /// left untouched):
    /// - `play_state() != Editing` — Save is disallowed during PIE (warn-log;
    ///   consistent with the `Ctrl+O` / R-key gate).
    /// - `save_dialog` is `None` — no dialog attached (warn-log; defensive —
    ///   the binary attaches one in every launch mode).
    /// - `pick_save_path()` returned `None` — the user cancelled (info-log; NO
    ///   mutation).
    /// - `scene_save_hook` is `None` — no writer attached (warn-log; defensive).
    /// - Hook returned `Err` — the path was rejected / serialize / I/O failed;
    ///   the bus is NOT marked saved.
    ///
    /// Public so headless tests can drive Save without synthesizing a winit
    /// `KeyEvent`; production usage routes through the `Ctrl+S` →
    /// [`MarkSaved`](super::EditorKeyCommand::MarkSaved) → `handle_key_command`
    /// path.
    pub fn handle_save_request(&mut self) {
        // (a) PIE gate — Save only fires in Editing, mirroring the Ctrl+O gate.
        if self.play_state() != PlayState::Editing {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                play_state = %self.play_state(),
                "Ctrl+S ignored: PIE active, save only fires in Editing"
            );
            return;
        }

        // (b) Dialog presence — defensive; the binary attaches one in every
        //     launch mode.
        let Some(dialog) = self.save_dialog.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                "Ctrl+S ignored: no save_dialog attached (missing with_scene_save_dialog)"
            );
            return;
        };

        // (c) Prompt the user. `None` == cancelled → no mutation.
        let Some(path) = dialog.pick_save_path() else {
            tracing::info!(
                target: "rge::editor-shell::save_request",
                "save cancelled (dialog returned no path); editor state unchanged"
            );
            return;
        };

        // (d) Writer presence + write. Borrow `scene_save_hook` in a scoped
        //     block so the immutable borrow ends before the `&mut self`
        //     `mark_saved_command` call below. `self.world.kernel()` hands the
        //     inner `rge_kernel_ecs::World` to the binary-owned writer.
        let save_result = {
            let Some(hook) = self.scene_save_hook.as_ref() else {
                tracing::warn!(
                    target: "rge::editor-shell::save_request",
                    path = %path.display(),
                    "Ctrl+S ignored: no scene_save_hook attached (missing with_scene_save_hook)"
                );
                return;
            };
            hook.save_scene_world(self.world.kernel(), &path)
        };

        // (e) Commit the bus saved point ONLY on a successful write.
        match save_result {
            Ok(()) => {
                self.mark_saved_command();
                tracing::info!(
                    target: "rge::editor-shell::save_request",
                    path = %path.display(),
                    "save OK; .rge-scene written and bus marked saved (is_dirty cleared)"
                );
            }
            Err(e) => tracing::warn!(
                target: "rge::editor-shell::save_request",
                path = %path.display(),
                error = %e,
                "scene save failed; bus NOT marked saved, editor state unchanged"
            ),
        }
    }
}
