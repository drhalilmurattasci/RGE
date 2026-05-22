//! Asset hot-reload — R-key handler + reload-hook trait.
//!
//! Companion to `commands.rs` (CommandBus-bound) and `playback.rs`
//! (state-machine-bound). This file holds the third keyboard axis:
//! plain `R` → re-import the editor's `--glb` source from disk and
//! swap the GPU-side mesh / material vecs via
//! [`crate::render_path::EditorShell::reload_render_assets`].
//!
//! # Design
//!
//! - The reload is **manual only** — pressing R is the user's signal
//!   that an external write to the file is complete. No `notify`-based
//!   watcher in v0 (deferred to a follow-up dispatch); the debounced
//!   watcher would have to handle write+rename, partial writes, and
//!   per-platform inotify quirks that the R-key path side-steps.
//!
//! - The shell stays decoupled from `rge-io-gltf` / `rge-brep-render`'s
//!   loader surface. The editor binary (`rge-editor::main`) impls
//!   [`AssetReloadHook`] over its own `load_all_glb_meshes` helper and
//!   hands an instance to [`crate::EditorShell`] at construction via
//!   [`crate::EditorShell::with_glb_reload_source`]. The shell calls
//!   the hook's [`AssetReloadHook::reload_glb`] when R is pressed, then
//!   passes the three returned vecs into `reload_render_assets`. The
//!   `RenderMesh` type IS already a dep of editor-shell (used at
//!   construction); the hook trait re-uses that existing edge rather
//!   than introducing a fresh one.
//!
//! - PIE-state gate: reload only fires when the shell is in
//!   [`crate::PlayState::Editing`]. Pressing R during Playing or
//!   Paused warn-logs and no-ops — mid-PIE asset swaps would conflict
//!   with the snapshot/restore round-trip.
//!
//! - Failure mode: any error path (no path, no hook, hook returned
//!   Err, swap returned Err) warn-logs and **retains the previous
//!   frame**. The GPU state is only mutated by
//!   `reload_render_assets`'s atomic-swap step, which runs after both
//!   the new materials and new lit-meshes have been built; partial
//!   uploads cannot corrupt the live render.

use std::path::Path;

use rge_brep_render::RenderMesh;

use crate::lifecycle::EditorShell;
use crate::play_state::PlayState;

/// Loader-callback trait for asset hot-reload.
///
/// The editor binary impls this over its own `load_all_glb_meshes`
/// helper and hands an instance to [`EditorShell::with_glb_reload_source`].
/// Keeps editor-shell free of an `rge-io-gltf` dep — the binary is
/// the only crate that knows how to parse a `.glb` file into the
/// three vecs the render path consumes.
///
/// `&self` (not `&mut self`) because v0's loader is stateless — every
/// reload re-imports from disk through a fresh
/// [`rge_io_gltf::MemoryCache`]. A future stateful loader (per-path
/// cache, parse-result memoization) can promote this to `&mut self`
/// without churning the trait's call sites.
pub trait AssetReloadHook {
    /// Re-import the glTF/GLB at `path` and return the three aligned
    /// vecs the render path consumes:
    ///
    /// - `Vec<RenderMesh>` — per-primitive flat-shaded meshes with
    ///   world-baked positions + UVs + normals
    ///   ([`rge_brep_render::RenderMesh`]).
    /// - `Vec<[f32; 4]>` — per-primitive linear-space `base_color`
    ///   factors.
    /// - `Vec<Option<(u32, u32, Vec<u8>)>>` — per-primitive embedded
    ///   `base_color_texture` payload as `(width, height, RGBA8
    ///   pixels)`, `None` when no texture.
    ///
    /// All three vecs must have matching length (one entry per
    /// drawable entity); the shell's `reload_render_assets` enforces
    /// this at runtime.
    ///
    /// On any parse / I/O / cache-lookup failure, return
    /// `Err(message)`. The shell's R-key handler warn-logs the
    /// message and retains the previous frame; no state is mutated.
    #[allow(clippy::type_complexity)]
    fn reload_glb(
        &self,
        path: &Path,
    ) -> Result<
        (
            Vec<RenderMesh>,
            Vec<[f32; 4]>,
            Vec<Option<(u32, u32, Vec<u8>)>>,
        ),
        String,
    >;
}

impl EditorShell {
    /// R-key handler — fires from the `WindowEvent::KeyboardInput`
    /// branch in [`Self::window_event`]. Reads the source path + hook
    /// stashed by [`Self::with_glb_reload_source`], invokes the hook,
    /// hands the result to
    /// [`crate::render_path::EditorShell::reload_render_assets`].
    ///
    /// All failure paths warn-log and no-op:
    /// - `play_state() != Editing` — reload is disallowed during PIE.
    /// - `glb_source_path` is `None` (default cuboid demo, or the
    ///   `--glb` mode without the source attached — should never
    ///   happen in production but is defensive).
    /// - `reload_hook` is `None` (same defensive shape as above).
    /// - Hook returned `Err` — typically I/O error, parse failure,
    ///   stale handle. File on disk is malformed; previous frame
    ///   stays.
    /// - `reload_render_assets` returned `Err` — typically a
    ///   length-invariant violation or GPU-upload failure. Previous
    ///   frame stays because the swap is atomic (only happens after
    ///   both new materials + new lit-meshes built successfully).
    ///
    /// Public so headless tests can drive reload without synthesizing
    /// a winit `KeyEvent`; production usage routes through the
    /// `WindowEvent::KeyboardInput` branch.
    pub fn handle_asset_reload(&mut self) {
        if self.play_state() != PlayState::Editing {
            tracing::warn!(
                target: "rge::editor-shell::asset_reload",
                play_state = %self.play_state(),
                "R-key ignored: PIE active, reload only fires in Editing"
            );
            return;
        }

        let (path, hook_result) = {
            let Some(path) = self.glb_source_path.as_ref() else {
                tracing::warn!(
                    target: "rge::editor-shell::asset_reload",
                    "R-key ignored: no glb_source_path attached (default cuboid demo or missing with_glb_reload_source)"
                );
                return;
            };
            let Some(hook) = self.reload_hook.as_ref() else {
                tracing::warn!(
                    target: "rge::editor-shell::asset_reload",
                    "R-key ignored: no reload_hook attached"
                );
                return;
            };
            (path.clone(), hook.reload_glb(path))
        };

        match hook_result {
            Ok((meshes, base_colors, textures)) => {
                let mesh_count = meshes.len();
                match self.reload_render_assets(meshes, base_colors, textures) {
                    Ok(()) => {
                        tracing::info!(
                            target: "rge::editor-shell::asset_reload",
                            path = %path.display(),
                            mesh_count,
                            "asset reload OK"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "rge::editor-shell::asset_reload",
                            path = %path.display(),
                            error = %e,
                            "reload_render_assets failed; retaining previous frame"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::asset_reload",
                    path = %path.display(),
                    error = %e,
                    "hook.reload_glb failed; retaining previous frame"
                );
            }
        }
    }
}
