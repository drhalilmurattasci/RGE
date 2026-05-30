//! In-app "Open" — Ctrl+O handler + open-dialog / scene-load traits.
//!
//! Companion to `asset_reload.rs` (the R-key reload axis). This file
//! holds the **fourth keyboard axis**: `Ctrl+O` → prompt the user via a
//! native file dialog, then dispatch on the picked path's kind:
//! - a `.glb` is imported and swapped into the GPU-side mesh / material
//!   vecs via [`crate::render_path::EditorShell::reload_render_assets`]
//!   — the same atomic-swap machinery the R-key path uses;
//! - a `.rge-scene` / `.rge-project` is loaded into a fresh kernel
//!   `World` (through the binary-owned [`SceneOpenHook`]) and swapped in
//!   live via [`EditorShell::replace_world`];
//! - anything else warn-logs and no-ops.
//!
//! # Design
//!
//! - **Hook split — editor-shell owns no file-system / loader edge.**
//!   The "pick a path" step ([`GlbOpenDialog::pick_glb_path`]), the
//!   "load a `.glb` into render vecs" step
//!   ([`super::AssetReloadHook::reload_glb`]), and the "load a scene
//!   into a kernel `World`" step ([`SceneOpenHook::load_scene_world`])
//!   are distinct traits with distinct binary-owned impls. The dialog
//!   impl owns the `rfd` dependency; the GLB loader impl owns
//!   `rge-io-gltf`; the scene loader impl owns `rge-scene-loader`.
//!   editor-shell gains NONE of them — it holds only the boxed `dyn`
//!   trait objects and calls through them. This mirrors how the R-key
//!   path keeps the glTF loader edge inside `rge-editor`
//!   (`AssetReloadHook`), and is the standing rule from
//!   `.ai/dispatch.tasks.md` ("Loader stays in `rge-editor`; no
//!   `editor-shell → io-gltf` edge"). The dialog + scene hooks get the
//!   same treatment so editor-shell never depends on `rfd`,
//!   `rge-scene-loader`, or `rge-data` either.
//!
//! - **Scene Open (`.rge-scene` / `.rge-project`).** A scene path loads
//!   into a fresh kernel `World` via the binary-owned [`SceneOpenHook`]
//!   (which calls `rge_scene_loader::load_scene_world_from_path`), then
//!   swaps that world in live via [`EditorShell::replace_world`] — the
//!   runtime `World`-swap surface this path used to lack. The load runs
//!   in the hook BEFORE `replace_world`, so a malformed scene fails with
//!   the live world untouched (the scene analogue of the GLB
//!   commit-after-success property below). v0 renders the swapped-in
//!   scene blank (matching the `--scene` / `replace_world` semantics);
//!   rendering scene entities is future work.
//!
//! - **PIE-state gate.** Open only fires when the shell is in
//!   [`crate::PlayState::Editing`] — consistent with the R-key reload
//!   PIE gate. Pressing Ctrl+O during Playing or Paused warn-logs and
//!   no-ops; a mid-PIE asset swap would conflict with the
//!   snapshot/restore round-trip.
//!
//! - **Commit-after-success ordering.** Unlike the R-key path (which
//!   reloads a path already committed to [`EditorShell::glb_source_path`]),
//!   the Open handler must NOT commit the freshly-picked path until the
//!   load AND the GPU swap have both succeeded. The sequence is:
//!   pick → guard hook + PIE → `reload_glb` → `reload_render_assets` →
//!   only on `Ok(())` assign `glb_source_path`. A malformed picked GLB
//!   fails the swap, the previous frame is correctly retained, AND
//!   `glb_source_path` is left untouched — so a subsequent R-key reload
//!   still targets the last *good* file, never the rejected one. This
//!   is the load-bearing safety property the dispatch correction
//!   pinned (see the failing-candidate test).
//!
//! - **Failure mode.** Every error path (not Editing, no dialog, dialog
//!   cancelled, loader returned `Err`, swap returned `Err`) logs and
//!   returns without mutating render state. The GPU is only mutated by
//!   `reload_render_assets`'s atomic-swap step, which runs after both
//!   the new materials and new lit-meshes have been built; partial
//!   uploads cannot corrupt the live render.
//!
//! - **Watcher coupling (binary-side).** The hot-reload watcher is
//!   binary-owned (`rge-editor::EditorApp.watcher`, a `notify`-backed
//!   `GlbWatcher`); editor-shell has no watcher and no `notify`
//!   dependency. The binary reconciles the watcher against
//!   [`EditorShell::glb_source_path`] every window event
//!   (`sync_glb_watcher`): committing a new GLB source here re-roots the
//!   watcher onto the opened file, and a scene Open (which clears
//!   `glb_source_path` via `replace_world`) tears the watcher down. This
//!   handler never touches the watcher directly — it only moves
//!   `glb_source_path`, and the binary follows. See the comment at the
//!   GLB commit site.

use std::path::PathBuf;

use crate::lifecycle::EditorShell;
use crate::play_state::PlayState;

/// Loader-callback trait for the in-app "Open" file dialog.
///
/// Despite the historical `Glb`-prefixed name (kept to bound the
/// scene-open wiring dispatch — a rename is a cosmetic follow-up), this
/// dialog picks ANY supported Open candidate: a `.glb`, a `.rge-scene`,
/// or a `.rge-project`. The `Ctrl+O` handler
/// ([`EditorShell::handle_open_request`]) dispatches on the *returned
/// path's* kind, so the dialog only has to offer the right filters.
///
/// The editor binary (`rge-editor::main`) impls this with `rfd`
/// (`rfd::FileDialog::new().add_filter(..).pick_file()`) and hands an
/// instance to [`EditorShell`] at construction via
/// [`EditorShell::with_glb_open_dialog`]. Keeping the impl in the
/// binary leaves editor-shell free of any `rfd` (or `rge-io-gltf`)
/// dependency — the shell holds only a `Box<dyn GlbOpenDialog>` and
/// calls [`Self::pick_glb_path`] when `Ctrl+O` is pressed. Mirrors the
/// [`super::AssetReloadHook`] split exactly.
///
/// `&self` (not `&mut self`) because the dialog is stateless — each
/// invocation spawns a fresh native dialog. A future stateful dialog
/// (last-directory memory, recent-files) can promote this to
/// `&mut self` without churning the single call site.
pub trait GlbOpenDialog {
    /// Prompt the user for an Open candidate (a `.glb`, `.rge-scene`, or
    /// `.rge-project`). Returns `Some(path)` when the user picked a file,
    /// `None` when the dialog was cancelled.
    ///
    /// The returned path is a *candidate* —
    /// [`EditorShell::handle_open_request`] dispatches on its kind (GLB
    /// import vs. scene world-swap) and mutates editor state only on
    /// success. A cancelled dialog (`None`) mutates no editor state.
    fn pick_glb_path(&self) -> Option<PathBuf>;
}

/// Loader-callback trait for the in-app "Open scene" path (`.rge-scene`
/// / `.rge-project`) — the scene-axis companion to [`GlbOpenDialog`].
///
/// The editor binary (`rge-editor::main`) impls this over
/// `rge_scene_loader::load_scene_world_from_path` and hands an instance
/// to [`EditorShell`] at construction via
/// [`EditorShell::with_scene_open_hook`]. Keeping the impl in the binary
/// leaves editor-shell free of any `rge-scene-loader` / `rge-data`
/// dependency — the shell holds only a `Box<dyn SceneOpenHook>` and
/// calls [`Self::load_scene_world`] when the user opens a scene path.
/// Mirrors the [`super::AssetReloadHook`] / [`GlbOpenDialog`] split
/// exactly.
///
/// `&self` (not `&mut self`) because the loader is stateless — every
/// open re-reads from disk. A future stateful loader (project cache) can
/// promote this to `&mut self` without churning the single call site.
pub trait SceneOpenHook {
    /// Load a `.rge-project` / `.rge-scene` at `path` into a fresh
    /// [`rge_kernel_ecs::World`].
    ///
    /// On any I/O / parse / load failure, return `Err(message)`: the
    /// `Ctrl+O` handler warn-logs it and leaves the live world untouched
    /// ([`EditorShell::replace_world`] is never reached). On `Ok`, the
    /// returned world is swapped in live via `replace_world`, which
    /// blanks the viewport (v0 scene render is blank, matching the
    /// `--scene` semantics).
    fn load_scene_world(&self, path: &std::path::Path) -> Result<rge_kernel_ecs::World, String>;
}

/// Classify an Open candidate as a scene path (`.rge-scene` /
/// `.rge-project`). Matches the file name the way the loader
/// (`rge_scene_loader::load_scene_world_from_path`) does: a literal
/// `.rge-project` (a leading-dot-only name with no `Path::extension()`)
/// or any `*.rge-scene` suffix.
fn candidate_is_scene(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == ".rge-project" || name.ends_with(".rge-scene"))
}

/// Classify an Open candidate as a GLB (`*.glb`, case-insensitive on the
/// extension so a Windows-picked `CUBE.GLB` still routes to the GLB
/// import path).
fn candidate_is_glb(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("glb"))
}

impl EditorShell {
    /// `Ctrl+O` handler — fires from the `WindowEvent::KeyboardInput`
    /// branch in [`Self::window_event`]. Prompts via the
    /// [`GlbOpenDialog`] stashed by [`Self::with_glb_open_dialog`], then
    /// dispatches on the picked path's kind:
    ///
    /// - `.rge-scene` / `.rge-project` → load via the [`SceneOpenHook`]
    ///   stashed by [`Self::with_scene_open_hook`] and swap the live
    ///   `World` via [`EditorShell::replace_world`] (the scene load runs
    ///   in the hook BEFORE the swap, so a malformed scene leaves the
    ///   live world untouched);
    /// - `.glb` → import via the [`super::AssetReloadHook`] stashed by
    ///   [`Self::attach_glb_reload_source`] and hand the result to
    ///   [`crate::render_path::EditorShell::reload_render_assets`] (the
    ///   commit-after-success path below);
    /// - anything else → warn-log and no-op.
    ///
    /// # Commit-after-success ordering (the `.glb` branch)
    ///
    /// [`Self::glb_source_path`] is assigned **only** after the load
    /// and the swap have both returned `Ok`. This deliberately does NOT
    /// delegate to [`Self::handle_asset_reload`] (which would require
    /// pre-setting `glb_source_path` before the swap, leaving it
    /// pointing at a rejected file on failure). See the module-level
    /// "Commit-after-success ordering" note.
    ///
    /// All failure paths log and no-op (render state + `glb_source_path`
    /// untouched, previous frame retained):
    /// - `play_state() != Editing` — Open is disallowed during PIE
    ///   (warn-log; consistent with the R-key gate).
    /// - `open_dialog` is `None` — no dialog was attached (warn-log;
    ///   defensive — the binary attaches one in every launch mode).
    /// - `pick_glb_path()` returned `None` — the user cancelled the
    ///   dialog (info-log; NO mutation).
    /// - `reload_hook` is `None` — no loader was attached (warn-log;
    ///   defensive).
    /// - Hook returned `Err` — the picked file is malformed / missing;
    ///   the previous frame stays and `glb_source_path` is UNCHANGED.
    /// - `reload_render_assets` returned `Err` — a length-invariant
    ///   violation or GPU-upload failure; previous frame stays
    ///   (atomic swap) and `glb_source_path` is UNCHANGED.
    ///
    /// Public so headless tests can drive Open without synthesizing a
    /// winit `KeyEvent`; production usage routes through the
    /// `WindowEvent::KeyboardInput` branch.
    pub fn handle_open_request(&mut self) {
        // (a) PIE gate — Open only fires in Editing, mirroring the
        //     R-key reload gate.
        if self.play_state() != PlayState::Editing {
            tracing::warn!(
                target: "rge::editor-shell::open_request",
                play_state = %self.play_state(),
                "Ctrl+O ignored: PIE active, open only fires in Editing"
            );
            return;
        }

        // (b) Dialog presence — defensive; the binary attaches a
        //     dialog in every launch mode.
        let Some(dialog) = self.open_dialog.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::open_request",
                "Ctrl+O ignored: no open_dialog attached (missing with_glb_open_dialog)"
            );
            return;
        };

        // (c) Prompt the user. `None` == cancelled → no mutation.
        let Some(candidate) = dialog.pick_glb_path() else {
            tracing::info!(
                target: "rge::editor-shell::open_request",
                "open cancelled (dialog returned no path); editor state unchanged"
            );
            return;
        };

        // (d) Scene dispatch. A `.rge-scene` / `.rge-project` takes the
        //     runtime World-swap route and returns; a `.glb` falls
        //     through to the render-asset import below; anything else
        //     warns. The scene load runs inside the binary-owned
        //     `SceneOpenHook` BEFORE `replace_world`, so a malformed
        //     scene fails with the live world untouched — the scene
        //     analogue of the GLB commit-after-success property.
        if candidate_is_scene(&candidate) {
            let load_result = {
                let Some(hook) = self.scene_open_hook.as_ref() else {
                    tracing::warn!(
                        target: "rge::editor-shell::open_request",
                        path = %candidate.display(),
                        "Ctrl+O scene open ignored: no scene_open_hook attached (missing with_scene_open_hook)"
                    );
                    return;
                };
                hook.load_scene_world(&candidate)
            };
            let world = match load_result {
                Ok(world) => world,
                Err(e) => {
                    tracing::warn!(
                        target: "rge::editor-shell::open_request",
                        path = %candidate.display(),
                        error = %e,
                        "scene load failed; retaining the live world, no swap"
                    );
                    return;
                }
            };
            // `replace_world` is Editing-gated (already checked above) and
            // clears `glb_source_path` + `scene_source_path`, so the binary
            // tears down the GLB watcher on its next `sync_glb_watcher`.
            match self.replace_world(world) {
                Ok(()) => {
                    // Commit the silent-save target — but ONLY for a
                    // `*.rge-scene` source. A `.rge-project` cannot be
                    // overwritten by the writer (`save_scene_world_to_path`
                    // rejects it), so it stays `None` and `Ctrl+S` is Save-As.
                    if candidate
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".rge-scene"))
                    {
                        self.scene_source_path = Some(candidate.clone());
                    }
                    tracing::info!(
                        target: "rge::editor-shell::open_request",
                        path = %candidate.display(),
                        scene_source_tracked = self.scene_source_path.is_some(),
                        "scene open OK; world swapped, viewport blanked, glb_source_path cleared"
                    );
                }
                Err(e) => tracing::warn!(
                    target: "rge::editor-shell::open_request",
                    path = %candidate.display(),
                    error = %e,
                    "replace_world rejected the scene swap; live world unchanged"
                ),
            }
            return;
        }
        if !candidate_is_glb(&candidate) {
            tracing::warn!(
                target: "rge::editor-shell::open_request",
                path = %candidate.display(),
                "Ctrl+O ignored: unsupported Open candidate (expected .glb, .rge-scene, or .rge-project)"
            );
            return;
        }

        // (e) GLB loader presence + import. Borrow `reload_hook` and run
        //     the import inside a scoped block so the immutable borrow
        //     ends before the `&mut self` calls below — mirroring how
        //     `handle_asset_reload` scopes its borrows. `candidate` is
        //     already owned (the dialog returned it by value), so it
        //     survives the borrow boundary without an extra clone.
        let hook_result = {
            let Some(hook) = self.reload_hook.as_ref() else {
                tracing::warn!(
                    target: "rge::editor-shell::open_request",
                    path = %candidate.display(),
                    "Ctrl+O ignored: no reload_hook attached"
                );
                return;
            };
            hook.reload_glb(&candidate)
        };

        let (meshes, base_colors, textures) = match hook_result {
            Ok(triple) => triple,
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::open_request",
                    path = %candidate.display(),
                    error = %e,
                    "hook.reload_glb failed; retaining previous frame, glb_source_path unchanged"
                );
                return;
            }
        };

        // (f) Swap render assets, then commit ONLY on success. The
        //     immutable `reload_hook` borrow above has ended, so the
        //     `&mut self` swap call is unambiguous.
        let mesh_count = meshes.len();
        match self.reload_render_assets(meshes, base_colors, textures) {
            Ok(()) => {
                // Commit the new source path — and ONLY now. R-key
                // reloads henceforth target the newly opened file.
                //
                // The binary-owned `notify` watcher re-roots onto this
                // committed `glb_source_path` on its next
                // `sync_glb_watcher`, so automatic hot-reload follows the
                // newly opened file too (not just the manual R-key).
                // editor-shell itself holds no watcher — it only moves
                // `glb_source_path`; the binary reconciles.
                self.glb_source_path = Some(candidate.clone());
                tracing::info!(
                    target: "rge::editor-shell::open_request",
                    path = %candidate.display(),
                    mesh_count,
                    "open OK; render assets swapped and glb_source_path committed"
                );
            }
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::open_request",
                    path = %candidate.display(),
                    error = %e,
                    "reload_render_assets failed; retaining previous frame, glb_source_path unchanged"
                );
            }
        }
    }
}
