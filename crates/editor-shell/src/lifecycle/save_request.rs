//! In-app "Save" — Ctrl+S handler + save-dialog / scene-write traits.
//!
//! The save-axis companion to [`open_request`](super::open_request) (the
//! `Ctrl+O` Open path). `Ctrl+S` reaches [`EditorShell::handle_save_request`]
//! via the canonical menu: the keystroke resolves to `Command::Save` and routes
//! through [`EditorShell::route_menu_command`] (the W08.3 cutover + the W08.4
//! retirement of the old `EditorKeyCommand::Save` arm). SCENE-SAVE-WIRING made
//! that a real Save-to-disk rather than a pure `mark_saved` bookmark.
//!
//! # Design (mirrors the Open hook split)
//!
//! - **True Save with Save-As fallback, routed by [`SaveSource`].** `Ctrl+S`
//!   dispatches on [`EditorShell::save_source`]:
//!   - [`SaveSource::Scene`] (a `.rge-scene` was opened / launched, or a prior
//!     Save-As committed one) → write the live `World` straight back to it via
//!     the binary-owned [`SceneSaveHook`]
//!     (`rge_scene_loader::save_scene_world_to_path`) with **no dialog** (silent
//!     overwrite).
//!   - [`SaveSource::Project`] (a literal `.rge-project` was opened / launched)
//!     → write the world back to it via the binary-owned [`ProjectSaveHook`]
//!     (`rge_scene_loader::save_project_world_to_path` — overwrite the first
//!     scene + re-write the manifest) with **no dialog**.
//!   - `None` (blank / demo / `.glb`) → the binary-owned [`SceneSaveDialog`]
//!     prompts (Save-As) and the picked path is committed as a new
//!     [`SaveSource::Scene`] on success.
//!
//!   Either way the Command-Bus saved point is marked
//!   ([`EditorShell::mark_saved_command`]) only on a successful write, clearing
//!   `is_dirty()`. (This `Ctrl+S` no-source Save-As produces a `.rge-scene`
//!   [`SaveSource::Scene`]. Save-As to a *new* `.rge-project` tree is a separate
//!   gesture — `Ctrl+Shift+S`, [`EditorShell::handle_save_as_new_project_request`]
//!   — which creates the tree and adopts a [`SaveSource::Project`]; see
//!   [`NewProjectSaveDialog`] / [`NewProjectSaveHook`].)
//!
//! - **editor-shell owns no file-system / loader edge.** The dialog impl owns
//!   the `rfd` dependency; the scene + project writer impls own
//!   `rge-scene-loader`. editor-shell holds only the boxed `dyn` trait objects
//!   and calls through them — it never gains an `rfd`, `rge-scene-loader`, or
//!   `rge-data` dependency. Mirrors [`GlbOpenDialog`](super::GlbOpenDialog) /
//!   [`SceneOpenHook`](super::SceneOpenHook) exactly.
//!
//! - **Editing-gated.** Save only fires in [`PlayState::Editing`], mirroring the
//!   `Ctrl+O` / R-key reload PIE gate: a mid-Play Save would persist the
//!   transient play-state world, not the edit world.
//!
//! - **Mark-saved only on success.** Cancel, missing dialog, missing hook, and
//!   write-error paths all log and leave `command_bus().is_dirty()` untouched.

use std::path::PathBuf;

use super::SaveSource;
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

/// Writer-callback for in-app Save of a `.rge-project` — the project-axis
/// companion to [`SceneSaveHook`].
///
/// The editor binary (`rge-editor::main`) impls this over
/// `rge_scene_loader::save_project_world_to_path` and hands an instance to
/// [`EditorShell`] via [`EditorShell::with_project_save_hook`]. Keeping the impl
/// in the binary leaves editor-shell free of any `rge-scene-loader` /
/// `rge-data` dependency — the shell holds only a `Box<dyn ProjectSaveHook>` and
/// calls [`Self::save_project_world`] when the user saves an open
/// `.rge-project`.
///
/// `&self` (not `&mut self`) because the writer is stateless — every save
/// re-extracts from the live world and writes afresh.
pub trait ProjectSaveHook {
    /// Write `world` back to the project at `project_path` (overwrite its first
    /// scene + re-write the manifest).
    ///
    /// On any failure, return `Err(message)`: the `Ctrl+S` handler warn-logs it
    /// and does NOT mark the bus saved. On `Ok`, the handler marks the
    /// Command-Bus saved point (clearing `is_dirty()`).
    fn save_project_world(
        &self,
        world: &rge_kernel_ecs::World,
        project_path: &std::path::Path,
    ) -> Result<(), String>;
}

/// Directory-picker for Save-As to a **new** `.rge-project` tree (`Ctrl+Shift+S`)
/// — the new-project companion to [`SceneSaveDialog`].
///
/// The editor binary (`rge-editor::main`) impls this with `rfd`
/// (`rfd::FileDialog::new().pick_folder()`) and hands an instance to
/// [`EditorShell`] via [`EditorShell::with_new_project_save_dialog`]. Keeping the
/// impl in the binary leaves editor-shell free of any `rfd` dependency — the
/// shell holds only a `Box<dyn NewProjectSaveDialog>`.
///
/// `&self` (stateless) — each invocation spawns a fresh native dialog.
pub trait NewProjectSaveDialog {
    /// Prompt the user for a target directory to host a new project. Returns
    /// `Some(dir)` when the user chose a folder, `None` when cancelled (in which
    /// case the handler mutates no editor state).
    fn pick_new_project_dir(&self) -> Option<std::path::PathBuf>;
}

/// Writer-callback for Save-As to a **new** `.rge-project` tree — the new-project
/// companion to [`ProjectSaveHook`].
///
/// The editor binary (`rge-editor::main`) impls this over
/// `rge_scene_loader::save_world_as_new_project` and hands an instance to
/// [`EditorShell`] via [`EditorShell::with_new_project_save_hook`]. Keeping the
/// impl in the binary leaves editor-shell free of any `rge-scene-loader` /
/// `rge-data` dependency — the shell holds only a `Box<dyn NewProjectSaveHook>`.
///
/// `&self` (stateless) — every save re-extracts from the live world.
pub trait NewProjectSaveHook {
    /// Create a new `.rge-project` tree at `project_dir` from `world`, returning
    /// the created `.rge-project` path on success.
    ///
    /// On any failure, return `Err(message)`: the `Ctrl+Shift+S` handler
    /// warn-logs it and does NOT mark the bus saved or adopt a source. On `Ok`,
    /// the handler adopts the returned path as a [`SaveSource::Project`] and
    /// marks the Command-Bus saved point (clearing `is_dirty()`).
    fn save_world_as_new_project(
        &self,
        world: &rge_kernel_ecs::World,
        project_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf, String>;
}

impl EditorShell {
    /// `Ctrl+S` handler — invoked from the `Command::Save` arm of
    /// [`Self::route_menu_command`] (the keystroke resolves to `Command::Save`
    /// through the canonical menu). Routes the live `World` to disk by the open
    /// [`SaveSource`] and marks the Command-Bus saved point **only** on a
    /// successful write.
    ///
    /// Routing on [`Self::save_source`]:
    /// - [`SaveSource::Scene`] → silent overwrite via the [`SceneSaveHook`]
    ///   stashed by [`Self::with_scene_save_hook`] (**no dialog**).
    /// - [`SaveSource::Project`] → silent write via the [`ProjectSaveHook`]
    ///   stashed by [`Self::with_project_save_hook`] (overwrite the first scene +
    ///   re-write the manifest; **no dialog**).
    /// - `None` → Save-As: the [`SceneSaveDialog`] stashed by
    ///   [`Self::with_scene_save_dialog`] prompts for a `.rge-scene`
    ///   destination; on a successful write the picked path is committed as a
    ///   new [`SaveSource::Scene`] so the next `Ctrl+S` overwrites silently.
    ///
    /// All failure paths log and no-op (the bus saved point / `is_dirty()` and
    /// `save_source` are left untouched):
    /// - `play_state() != Editing` — Save is disallowed during PIE (warn-log;
    ///   consistent with the `Ctrl+O` / R-key gate).
    /// - Save-As with no `save_dialog` attached — warn-log (defensive — the
    ///   binary attaches one in every launch mode).
    /// - `pick_save_path()` returned `None` — the user cancelled (info-log; NO
    ///   mutation).
    /// - The matching writer hook is `None` — none attached (warn-log;
    ///   defensive).
    /// - Hook returned `Err` — the path was rejected / serialize / I/O failed;
    ///   the bus is NOT marked saved and no source is committed.
    ///
    /// Public so headless tests can drive Save without synthesizing a winit
    /// `KeyEvent`; production usage routes through the `Ctrl+S` → `Command::Save`
    /// → [`Self::route_menu_command`] path (or the File ▸ Save menu item).
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

        // (b) Route by the open SaveSource. Clone it so the `&self` read ends
        //     before the `&mut self` mark / commit below. Scene + Project are
        //     silent writes; None is Save-As (commits a new Scene on success).
        //     The `write_*` helpers warn-log a missing hook / a write error and
        //     return `false`; this router only logs the success + marks saved.
        match self.save_source.clone() {
            Some(SaveSource::Scene(path)) => {
                if self.write_scene_world(&path) {
                    tracing::info!(
                        target: "rge::editor-shell::save_request",
                        path = %path.display(),
                        "save OK; .rge-scene overwritten and bus marked saved (is_dirty cleared)"
                    );
                    self.mark_saved_command();
                }
            }
            Some(SaveSource::Project { path, .. }) => {
                if self.write_project_world(&path) {
                    tracing::info!(
                        target: "rge::editor-shell::save_request",
                        path = %path.display(),
                        "save OK; .rge-project written (first scene + manifest) and bus marked saved (is_dirty cleared)"
                    );
                    self.mark_saved_command();
                }
            }
            None => {
                // Save-As — no tracked source. Prompt for a `.rge-scene`
                // destination; on a successful write commit it as a new
                // `SaveSource::Scene` so the next `Ctrl+S` overwrites silently.
                let Some(dialog) = self.save_dialog.as_ref() else {
                    tracing::warn!(
                        target: "rge::editor-shell::save_request",
                        "Ctrl+S ignored: no save source and no save_dialog attached (missing with_scene_save_dialog)"
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
                if self.write_scene_world(&picked) {
                    tracing::info!(
                        target: "rge::editor-shell::save_request",
                        path = %picked.display(),
                        "Save-As OK; .rge-scene written, source committed, bus marked saved (is_dirty cleared)"
                    );
                    self.save_source = Some(SaveSource::Scene(picked));
                    self.mark_saved_command();
                }
            }
        }
    }

    /// Write the live world to a `.rge-scene` at `path` via the
    /// [`SceneSaveHook`]. Scopes the `&self` borrows (hook + world) so they end
    /// before any `&mut self` commit in the caller. Returns `true` on a
    /// successful write; `false` (with a warn-log) when no `scene_save_hook` is
    /// attached or the hook returned `Err`. Does NOT mark the bus saved — the
    /// caller owns the success-side mark + source commit.
    fn write_scene_world(&self, path: &std::path::Path) -> bool {
        let Some(hook) = self.scene_save_hook.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                path = %path.display(),
                "Ctrl+S ignored: no scene_save_hook attached (missing with_scene_save_hook)"
            );
            return false;
        };
        match hook.save_scene_world(self.world.kernel(), path) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::save_request",
                    path = %path.display(),
                    error = %e,
                    "scene save failed; bus NOT marked saved, editor state unchanged"
                );
                false
            }
        }
    }

    /// Write the live world back to the `.rge-project` at `path` via the
    /// [`ProjectSaveHook`] (overwrite first scene + manifest). Mirrors
    /// [`Self::write_scene_world`]: returns `true` on success; `false` (with a
    /// warn-log) when no `project_save_hook` is attached or the hook returned
    /// `Err`. Does NOT mark the bus saved.
    fn write_project_world(&self, path: &std::path::Path) -> bool {
        let Some(hook) = self.project_save_hook.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                path = %path.display(),
                "Ctrl+S ignored: no project_save_hook attached (missing with_project_save_hook)"
            );
            return false;
        };
        match hook.save_project_world(self.world.kernel(), path) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::save_request",
                    path = %path.display(),
                    error = %e,
                    "project save failed; bus NOT marked saved, editor state unchanged"
                );
                false
            }
        }
    }

    /// `Ctrl+Shift+S` handler — Save-As to a **new** `.rge-project` tree.
    /// Invoked from the `Command::SaveAs` arm of [`Self::route_menu_command`] (the
    /// keystroke resolves to `Command::SaveAs` through the canonical menu). Prompts
    /// (via the binary-owned
    /// [`NewProjectSaveDialog`]) for a target directory, creates a fresh project
    /// tree there (via the [`NewProjectSaveHook`] over
    /// `rge_scene_loader::save_world_as_new_project`), and on success **adopts**
    /// the created `.rge-project` as the live [`SaveSource::Project`] — so the
    /// next plain `Ctrl+S` overwrites it silently — and marks the Command-Bus
    /// saved point.
    ///
    /// All failure paths log and no-op (no source adopted, bus untouched),
    /// mirroring [`Self::handle_save_request`]:
    /// - `play_state() != Editing` — disallowed during PIE (warn-log).
    /// - no `new_project_dialog` attached — warn-log (defensive).
    /// - `pick_new_project_dir()` returned `None` — user cancelled (info-log).
    /// - no `new_project_hook` attached, or the hook returned `Err` — warn-log.
    ///
    /// The adopted `name` is folder-derived from the picked directory; an
    /// unnameable (non-UTF-8) directory yields `name: None` and **the save still
    /// succeeds** — the `.rge-project` path is the source of truth and
    /// [`SaveSource::display_name`] falls back to it.
    ///
    /// Public so headless tests can drive Save-As without synthesizing a winit
    /// `KeyEvent`.
    pub fn handle_save_as_new_project_request(&mut self) {
        if self.play_state() != PlayState::Editing {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                play_state = %self.play_state(),
                "Ctrl+Shift+S ignored: PIE active, Save-As only fires in Editing"
            );
            return;
        }

        let Some(dialog) = self.new_project_dialog.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                "Ctrl+Shift+S ignored: no new_project_dialog attached (missing with_new_project_save_dialog)"
            );
            return;
        };
        let Some(dir) = dialog.pick_new_project_dir() else {
            tracing::info!(
                target: "rge::editor-shell::save_request",
                "Save-As (new project) cancelled (dialog returned no directory); editor state unchanged"
            );
            return;
        };

        let Some(project_path) = self.create_new_project_world(&dir) else {
            return;
        };

        // Folder-derived display name; `None` when the dir has no UTF-8 name —
        // this MUST NOT block the save (the path is the source of truth and
        // `SaveSource::display_name` falls back to it).
        let name = dir.file_name().and_then(|s| s.to_str()).map(String::from);
        tracing::info!(
            target: "rge::editor-shell::save_request",
            path = %project_path.display(),
            "Save-As (new project) OK; tree created, source adopted, bus marked saved (is_dirty cleared)"
        );
        self.save_source = Some(SaveSource::Project {
            path: project_path,
            name,
        });
        self.mark_saved_command();
    }

    /// Create a new `.rge-project` tree at `dir` via the [`NewProjectSaveHook`],
    /// returning the created `.rge-project` path on success. Scopes the `&self`
    /// hook borrow so it ends before the caller's `&mut self` adopt/commit;
    /// returns `None` (with a warn-log) when no `new_project_hook` is attached or
    /// the hook returned `Err`. Mirrors [`Self::write_scene_world`].
    fn create_new_project_world(&self, dir: &std::path::Path) -> Option<std::path::PathBuf> {
        let Some(hook) = self.new_project_hook.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::save_request",
                dir = %dir.display(),
                "Ctrl+Shift+S ignored: no new_project_hook attached (missing with_new_project_save_hook)"
            );
            return None;
        };
        match hook.save_world_as_new_project(self.world.kernel(), dir) {
            Ok(project_path) => Some(project_path),
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::save_request",
                    dir = %dir.display(),
                    error = %e,
                    "new-project save failed; bus NOT marked saved, editor state unchanged"
                );
                None
            }
        }
    }
}
