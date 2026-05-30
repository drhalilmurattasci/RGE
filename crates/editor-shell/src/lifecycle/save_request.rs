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
//! - **True Save with Save-As fallback.** When
//!   [`EditorShell::scene_source_path`] is `Some` (a `.rge-scene` was opened /
//!   launched, or a prior Save-As committed one), `Ctrl+S` writes the live
//!   `World` straight back to it via the binary-owned [`SceneSaveHook`]
//!   (`rge_scene_loader::save_scene_world_to_path`) with **no dialog** (silent
//!   overwrite). When it is `None`, the binary-owned [`SceneSaveDialog`] prompts
//!   (Save-As) and the picked path is committed as the new `scene_source_path`
//!   on success. Either way the Command-Bus saved point is marked
//!   ([`EditorShell::mark_saved_command`]) only on a successful write, clearing
//!   `is_dirty()`. (`.rge-project` sources are not tracked — the writer cannot
//!   overwrite them — so they stay Save-As.)
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
    /// [`Self::handle_key_command`]. Writes the live `World` through the
    /// [`SceneSaveHook`] stashed by [`Self::with_scene_save_hook`] and marks the
    /// Command-Bus saved point **only** on a successful write.
    ///
    /// Target selection:
    /// - **Silent overwrite** — when [`Self::scene_source_path`] is `Some` (a
    ///   `.rge-scene` was opened / launched, or a prior Save-As committed one),
    ///   the world is written straight back to it with **no dialog**.
    /// - **Save-As** — when it is `None`, the [`SceneSaveDialog`] stashed by
    ///   [`Self::with_scene_save_dialog`] prompts for a destination; on a
    ///   successful write the picked path is committed as the new
    ///   `scene_source_path` so the next `Ctrl+S` overwrites silently.
    ///
    /// All failure paths log and no-op (the bus saved point / `is_dirty()` and
    /// `scene_source_path` are left untouched):
    /// - `play_state() != Editing` — Save is disallowed during PIE (warn-log;
    ///   consistent with the `Ctrl+O` / R-key gate).
    /// - Save-As with no `save_dialog` attached — warn-log (defensive — the
    ///   binary attaches one in every launch mode).
    /// - `pick_save_path()` returned `None` — the user cancelled (info-log; NO
    ///   mutation).
    /// - `scene_save_hook` is `None` — no writer attached (warn-log; defensive).
    /// - Hook returned `Err` — the path was rejected / serialize / I/O failed;
    ///   the bus is NOT marked saved and no source path is committed.
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

        // (b) Target selection. A known `scene_source_path` is a silent
        //     overwrite (no dialog); otherwise prompt Save-As. `from_dialog`
        //     records whether the path must be committed as the new source on
        //     success.
        let (path, from_dialog) = match self.scene_source_path.clone() {
            Some(src) => (src, false),
            None => {
                let Some(dialog) = self.save_dialog.as_ref() else {
                    tracing::warn!(
                        target: "rge::editor-shell::save_request",
                        "Ctrl+S ignored: no scene_source_path and no save_dialog attached (missing with_scene_save_dialog)"
                    );
                    return;
                };
                let Some(picked) = dialog.pick_save_path() else {
                    tracing::info!(
                        target: "rge::editor-shell::save_request",
                        "save cancelled (dialog returned no path); editor state unchanged"
                    );
                    return;
                };
                (picked, true)
            }
        };

        // (c) Writer presence + write. Scope the `&self` borrows (hook + world)
        //     so they end before the `&mut self` commit below. `self.world.
        //     kernel()` hands the inner `rge_kernel_ecs::World` to the writer.
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

        // (d) On success: mark saved, and (Save-As path only) commit the picked path
        //     as the new silent-save source so the next `Ctrl+S` overwrites it.
        //     On failure: NOT marked, `scene_source_path` untouched.
        match save_result {
            Ok(()) => {
                tracing::info!(
                    target: "rge::editor-shell::save_request",
                    path = %path.display(),
                    silent = !from_dialog,
                    "save OK; .rge-scene written and bus marked saved (is_dirty cleared)"
                );
                if from_dialog {
                    self.scene_source_path = Some(path);
                }
                self.mark_saved_command();
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
