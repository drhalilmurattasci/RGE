//! Dispatch N1 — editor-shell headless visual smoke.
//!
//! Crate-local `#[cfg(test)]` module gated by
//! `crates/editor-shell/src/lib.rs`. Kept inside `src/` rather than
//! under `tests/` so the helpers reached via [`EditorShell`]
//! (`init_render_state_headless`, `render_frame_to_target`,
//! `acquire_depth_view`) stay `pub(crate)` — no public visual-test API
//! surface.
//!
//! # Scope (what this DOES verify)
//!
//! - One frame can be rendered headlessly (`HeadlessTarget` +
//!   `init_render_state_headless` + `acquire_depth_view` +
//!   `render_frame_to_target` succeed without a winit window).
//! - The rendered pixels can be read back through
//!   `rge_gfx::ReadbackBuffer::from_target`.
//! - **A textured cube with a 4×4 red/blue checkerboard produces
//!   per-fragment color variance** — closes the M2/M3 visual-
//!   acceptance gap where logs proved the wiring but no test verified
//!   pixels.
//! - Background corner pixels stay close to `DEFAULT_CLEAR` (the
//!   render pass actually painted something; the cube isn't a
//!   uniform-color full-screen blit).
//! - Untextured cube center is distinct from the background (the
//!   mesh-draw path renders even when no texture is bound — the
//!   pre-dispatch-M2 placeholder white path still works).
//!
//! # Scope (what this does NOT certify)
//!
//! - End-to-end glTF acceptance — uses hand-rolled mesh + texture
//!   data, NOT `rge_io_gltf::import_glb`. Dispatch N2 will wire the
//!   loader-side acceptance gate.
//! - Pixel-perfect golden-image regression — assertions are
//!   tolerance-based, not exact byte equality. wgpu's color sampling
//!   has ±1-byte cross-driver rounding; the variance assertion uses
//!   `>50` channel delta which is far above any driver noise.
//! - Pipeline / shader / vertex-layout correctness in isolation —
//!   those are covered by `crates/gfx/tests/*_smoke.rs`. This module
//!   is the editor-shell-level integration verifier.

use rge_brep_render::RenderMesh;
use rge_gfx::ReadbackBuffer;

use crate::lifecycle::EditorShell;
use crate::visual_test_harness;

/// Offscreen target format. `Rgba8Unorm` (linear) matches
/// `HeadlessTarget`'s hardcoded format; the lit shader writes linear
/// values so pixel assertions compare against linear-space expected
/// colors. NOT sRGB — sRGB-encoding would non-linearly remap red/blue
/// and complicate the variance assertions for no benefit.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Offscreen target width in pixels. Small enough to keep readback
/// fast (~256 KB), large enough that sampling at fractional positions
/// (W/4, W/2, 3W/4) lands on distinct cube faces.
const TARGET_WIDTH: u32 = 256;
/// Offscreen target height — matches width for a square aspect ratio
/// (predictable `compute_aabb_union` framing). The auto-frame for a
/// unit cube at origin centers the cube in the viewport at this size.
const TARGET_HEIGHT: u32 = 256;

/// Returns a [`rge_gfx::GfxContext`] or skips the test if no GPU
/// adapter is available. Matches the gfx-test boilerplate so the
/// editor-shell visual smoke degrades gracefully on no-GPU CI.
macro_rules! ctx_or_skip {
    () => {{
        match rge_gfx::GfxContext::new_headless() {
            Ok(c) => c,
            Err(rge_gfx::GfxContextError::NoAdapter) => {
                eprintln!("SKIP: no GPU adapter — skipping editor-shell visual smoke");
                return;
            }
            Err(e) => panic!("unexpected GfxContext error: {e}"),
        }
    }};
}

// ---------------------------------------------------------------------------
// Hand-rolled cube geometry (no glTF loader involvement)
// ---------------------------------------------------------------------------

/// Build the 24-vertex / 12-triangle / 6-face cube geometry that
/// matches the io-gltf `uv_cube_mesh` fixture shape: per-face
/// outward normals, per-face UV unwrap of the unit square, indices
/// `(base, base+1, base+2, base, base+2, base+3)` per quad.
///
/// Returns `(positions, normals, texcoords, indices)` ready to hand
/// to [`RenderMesh::from_buffers_with_attributes`].
///
/// Cube is centered at origin, `[-0.5, 0.5]^3`. Identity TRS (no
/// dispatch-J world bake required — the visual smoke is substrate-
/// level only).
fn unit_cube_attributes() -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) {
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [0.5, -0.5, 0.5],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
                [-0.5, -0.5, -0.5],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, -0.5],
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                [0.5, -0.5, -0.5],
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
            ],
        ),
    ];
    // Canonical per-face UV unwrap matching `make_uv_cube_glb`.
    let face_uvs: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut texcoords: Vec<[f32; 2]> = Vec::with_capacity(24);
    let mut indices: Vec<u32> = Vec::with_capacity(36);

    for (normal, verts) in &faces {
        let base = positions.len() as u32;
        for (v, uv) in verts.iter().zip(face_uvs.iter()) {
            positions.push(*v);
            normals.push(*normal);
            texcoords.push(*uv);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    (positions, normals, texcoords, indices)
}

/// Build a 4×4 red/blue checkerboard RGBA8 texture. 64 bytes,
/// row-major, no padding. Pattern: red where `(x + y) % 2 == 0`,
/// blue otherwise. Mirrors the io-gltf `make_checker_4x4_png`
/// fixture data so a future glTF-loader-driven N2 acceptance test
/// compares against the same canonical layout.
fn checkerboard_4x4_rgba8() -> Vec<u8> {
    let red: [u8; 4] = [255, 0, 0, 255];
    let blue: [u8; 4] = [0, 0, 255, 255];
    let mut rgba = Vec::with_capacity(64);
    for y in 0..4 {
        for x in 0..4 {
            let red_cell = (x + y) % 2 == 0;
            rgba.extend_from_slice(if red_cell { &red } else { &blue });
        }
    }
    rgba
}

// ---------------------------------------------------------------------------
// Shell + headless-render helpers
// ---------------------------------------------------------------------------

/// Construct a textured-cube `EditorShell` ready for headless render.
///
/// The shell carries:
/// - One `RenderMesh` — the 24-vert unit cube via
///   `RenderMesh::from_buffers_with_attributes(positions, indices,
///   None, Some(&normals), Some(&texcoords))`.
/// - `base_color = [1, 1, 1, 1]` (white tint so the checkerboard
///   renders unmodulated; matches the io-gltf `make_textured_uv_cube_
///   glb` fixture's material).
/// - `Some((4, 4, checkerboard_pixels))` per-mesh texture.
fn build_textured_cube_shell() -> EditorShell {
    let (positions, normals, texcoords, indices) = unit_cube_attributes();
    let mesh = RenderMesh::from_buffers_with_attributes(
        &positions,
        &indices,
        None,
        Some(&normals),
        Some(&texcoords),
    );
    EditorShell::with_render_meshes_and_base_colors_and_textures(
        vec![mesh],
        vec![[1.0, 1.0, 1.0, 1.0]],
        vec![Some((4, 4, checkerboard_4x4_rgba8()))],
    )
}

/// Construct the untextured variant — same geometry, no texture
/// payload, magenta `base_color` so the rendered cube is visibly
/// distinct from the dark-gray `DEFAULT_CLEAR` background.
fn build_untextured_cube_shell() -> EditorShell {
    let (positions, normals, texcoords, indices) = unit_cube_attributes();
    let mesh = RenderMesh::from_buffers_with_attributes(
        &positions,
        &indices,
        None,
        Some(&normals),
        Some(&texcoords),
    );
    EditorShell::with_render_meshes_and_base_colors_and_textures(
        vec![mesh],
        vec![[0.9, 0.2, 0.9, 1.0]],
        vec![None],
    )
}

/// Drive one full headless render frame via
/// [`visual_test_harness::render_one_frame_to_readback`] and return
/// the pixel buffer.
///
/// Thin panic-on-error wrapper around the harness so the N1 tests
/// keep the "measurement integrity guard" posture they had before
/// the N2 refactor. Test failures stay short and informative —
/// `render_one_frame_to_readback` returns a `Result<_, String>` with
/// failure-prefix discrimination that the wrapper unwraps.
fn render_one_frame_to_readback(
    shell: &mut EditorShell,
    width: u32,
    height: u32,
) -> ReadbackBuffer {
    visual_test_harness::render_one_frame_to_readback(shell, TARGET_FORMAT, width, height)
        .expect("render_one_frame_to_readback")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Mechanical guard: the headless render path produces a buffer of
/// the requested dimensions, with content (non-zero, finite-ish
/// pixels at any sampled position). No semantic assertion on color
/// — just that the pipeline ran end-to-end.
#[test]
fn headless_render_smoke_for_textured_path() {
    let _ctx = ctx_or_skip!();
    let mut shell = build_textured_cube_shell();
    let buf = render_one_frame_to_readback(&mut shell, TARGET_WIDTH, TARGET_HEIGHT);
    assert_eq!(buf.width, TARGET_WIDTH);
    assert_eq!(buf.height, TARGET_HEIGHT);
    assert_eq!(
        buf.pixels.len(),
        (TARGET_WIDTH * TARGET_HEIGHT * 4) as usize,
        "RGBA8 = w * h * 4 bytes"
    );
    // Smoke: the center pixel exists. (Color asserted in subsequent tests.)
    let center = buf
        .pixel(TARGET_WIDTH / 2, TARGET_HEIGHT / 2)
        .expect("center pixel exists");
    assert_eq!(center.3, 255, "alpha is opaque");
}

/// Scan the central horizontal row, identify pixels that landed on
/// the textured cube (i.e. distinct from `DEFAULT_CLEAR`), and
/// assert their red/blue spread is large enough to prove per-
/// fragment UV sampling is varying.
///
/// Row-scan rather than fixed sample points because the auto-framed
/// unit cube spans only ~25% of the viewport at the default
/// `eye/diag = 3.0` distance — fixed-position sampling would risk
/// hitting the background. Picking the two extreme cube pixels by
/// red-channel delta along the central row gives a stable variance
/// signal regardless of camera-framing tweaks (assuming the cube
/// still projects somewhere on `y = H/2`).
///
/// Tolerance: `>50` byte delta. The 4×4 checkerboard's red and blue
/// cells differ by 255 in those channels; even with Lambert+Phong
/// attenuation at oblique angles, the delta lands well above 50.
#[test]
fn textured_cube_shows_color_variance() {
    let _ctx = ctx_or_skip!();
    let mut shell = build_textured_cube_shell();
    let buf = render_one_frame_to_readback(&mut shell, TARGET_WIDTH, TARGET_HEIGHT);

    // DEFAULT_CLEAR is dark gray (~30, ~30, ~36). Any pixel whose
    // channels stray more than 30 from that anchor on at least one
    // channel is considered "on the cube". 30 is loose enough to
    // tolerate driver rounding + Phong ambient, tight enough to
    // reject background pixels.
    let bg_r: i32 = (0.12_f32 * 255.0).round() as i32;
    let bg_g: i32 = (0.12_f32 * 255.0).round() as i32;
    let bg_b: i32 = (0.14_f32 * 255.0).round() as i32;
    let cube_threshold: i32 = 30;

    let mut min_red: u8 = u8::MAX;
    let mut max_red: u8 = 0;
    let mut min_blue: u8 = u8::MAX;
    let mut max_blue: u8 = 0;
    let mut cube_pixel_count: u32 = 0;

    let y = TARGET_HEIGHT / 2;
    for x in 0..TARGET_WIDTH {
        let p = buf.pixel(x, y).expect("row pixel exists");
        let dr = (i32::from(p.0) - bg_r).abs();
        let dg = (i32::from(p.1) - bg_g).abs();
        let db = (i32::from(p.2) - bg_b).abs();
        if dr > cube_threshold || dg > cube_threshold || db > cube_threshold {
            cube_pixel_count += 1;
            min_red = min_red.min(p.0);
            max_red = max_red.max(p.0);
            min_blue = min_blue.min(p.2);
            max_blue = max_blue.max(p.2);
        }
    }

    assert!(
        cube_pixel_count > 8,
        "expected the central row to hit the cube at multiple pixels; got {cube_pixel_count}"
    );

    let red_spread = i32::from(max_red) - i32::from(min_red);
    let blue_spread = i32::from(max_blue) - i32::from(min_blue);
    assert!(
        red_spread > 50 || blue_spread > 50,
        "textured cube should show per-fragment color variance across the central row \
         (UV sampling active); cube_pixels={cube_pixel_count} \
         red_spread={red_spread} blue_spread={blue_spread} \
         red=[{min_red}, {max_red}] blue=[{min_blue}, {max_blue}]"
    );
}

/// Corner pixels (where the cube does not project) should be close
/// to `DEFAULT_CLEAR` — proves the render pass actually painted
/// something rather than blitting a uniform color across the whole
/// frame.
///
/// `DEFAULT_CLEAR` is `(0.12, 0.12, 0.14, 1.0)` in linear-space
/// floats → `(30, 30, 36, 255)` in the `Rgba8Unorm` readback (the
/// `HeadlessTarget` format is linear, not sRGB). Tolerance ±5 per
/// channel covers wgpu's cross-driver rounding.
#[test]
fn textured_cube_background_corner_is_default_clear() {
    let _ctx = ctx_or_skip!();
    let mut shell = build_textured_cube_shell();
    let buf = render_one_frame_to_readback(&mut shell, TARGET_WIDTH, TARGET_HEIGHT);

    // Top-left corner. The unit cube at origin under isometric
    // framing doesn't reach the very corners of a square viewport,
    // so this pixel is guaranteed to be the cleared background.
    let corner = buf.pixel(2, 2).expect("corner pixel exists");
    let expected_r: u8 = (0.12_f32 * 255.0).round() as u8;
    let expected_g: u8 = (0.12_f32 * 255.0).round() as u8;
    let expected_b: u8 = (0.14_f32 * 255.0).round() as u8;
    let tol = 5_i32;
    let dr = (i32::from(corner.0) - i32::from(expected_r)).abs();
    let dg = (i32::from(corner.1) - i32::from(expected_g)).abs();
    let db = (i32::from(corner.2) - i32::from(expected_b)).abs();
    assert!(
        dr <= tol && dg <= tol && db <= tol,
        "corner pixel should be close to DEFAULT_CLEAR ({expected_r}, {expected_g}, {expected_b}, 255); got {corner:?}"
    );
    assert_eq!(corner.3, 255, "alpha opaque");
}

/// Asset hot-reload (R-key) — full GPU path. Init a textured cube,
/// render once, swap to three untextured meshes via
/// `reload_render_assets`, re-render. Asserts:
///
/// - Post-reload `meshes.len() == 3` and `materials.len() == 3`.
/// - `prebuilt_*` Vecs updated to length 3.
/// - `pipeline`, `gfx_camera`, `light` remain `Some(...)` (preserved).
/// - `highlight_index_buffer` cleared.
/// - The second render succeeds (proves the swap left the encode path
///   intact).
///
/// The "1 textured cube → 3 colored cubes" shape exercises both the
/// mesh-count change AND the texture→placeholder transition in a
/// single test.
#[test]
fn reload_render_assets_swaps_meshes_keeps_pipeline() {
    let _ctx = ctx_or_skip!();
    let mut shell = build_textured_cube_shell();

    // First frame primes the GPU side.
    let _buf = render_one_frame_to_readback(&mut shell, TARGET_WIDTH, TARGET_HEIGHT);
    assert_eq!(shell.meshes.len(), 1, "initial mesh count");
    assert_eq!(shell.materials.len(), 1, "initial material count");
    assert!(shell.pipeline.is_some(), "pipeline initialized");
    assert!(shell.gfx_camera.is_some(), "camera initialized");
    assert!(shell.light.is_some(), "light initialized");

    // Build 3 distinct meshes for the reload — geometry is the same
    // unit cube; colors differ so per-mesh material UBO swap is
    // distinguishable downstream.
    let (positions, normals, texcoords, indices) = unit_cube_attributes();
    let make = || {
        RenderMesh::from_buffers_with_attributes(
            &positions,
            &indices,
            None,
            Some(&normals),
            Some(&texcoords),
        )
    };
    shell
        .reload_render_assets(
            vec![make(), make(), make()],
            vec![
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
            ],
            vec![None, None, None],
        )
        .expect("reload_render_assets succeeds");

    assert_eq!(shell.meshes.len(), 3, "post-reload mesh count");
    assert_eq!(shell.materials.len(), 3, "post-reload material count");
    assert_eq!(shell.prebuilt_render_meshes.len(), 3);
    assert_eq!(shell.prebuilt_render_base_colors.len(), 3);
    assert_eq!(shell.prebuilt_render_base_textures.len(), 3);
    assert!(
        shell.highlight_index_buffer.is_none(),
        "stale highlight cleared"
    );
    assert!(shell.pipeline.is_some(), "pipeline preserved");
    assert!(shell.gfx_camera.is_some(), "camera preserved");
    assert!(shell.light.is_some(), "light preserved");

    // Render again — must succeed (proves the swap didn't corrupt
    // the encode path).
    let buf2 = render_one_frame_to_readback(&mut shell, TARGET_WIDTH, TARGET_HEIGHT);
    assert_eq!(buf2.width, TARGET_WIDTH);
    assert_eq!(buf2.height, TARGET_HEIGHT);
    let center = buf2
        .pixel(TARGET_WIDTH / 2, TARGET_HEIGHT / 2)
        .expect("center pixel exists post-reload");
    assert_eq!(center.3, 255, "alpha opaque post-reload");
}

/// The untextured cube path (texture = `None`, magenta `base_color`)
/// still draws a mesh — center pixel is distinct from the background
/// clear color. Regression guard: the per-mesh `WHITE_1X1_RGBA`
/// placeholder + `update_color(base_color, DEFAULT_PHONG)` path
/// continues to render correctly even when no texture is bound.
#[test]
fn untextured_cube_lit_center_is_distinct_from_background() {
    let _ctx = ctx_or_skip!();
    let mut shell = build_untextured_cube_shell();
    let buf = render_one_frame_to_readback(&mut shell, TARGET_WIDTH, TARGET_HEIGHT);

    let center = buf
        .pixel(TARGET_WIDTH / 2, TARGET_HEIGHT / 2)
        .expect("center pixel exists");

    // DEFAULT_CLEAR is dark gray (30, 30, 36). The untextured cube
    // is magenta tinted by Lambert+Phong — the rendered center
    // pixel should differ from the background by more than the
    // ±5 corner-clear tolerance, on at least one channel.
    let bg_r: i32 = (0.12_f32 * 255.0).round() as i32;
    let bg_g: i32 = (0.12_f32 * 255.0).round() as i32;
    let bg_b: i32 = (0.14_f32 * 255.0).round() as i32;
    let dr = (i32::from(center.0) - bg_r).abs();
    let dg = (i32::from(center.1) - bg_g).abs();
    let db = (i32::from(center.2) - bg_b).abs();
    assert!(
        dr > 20 || dg > 20 || db > 20,
        "untextured cube center should differ from DEFAULT_CLEAR; got center={center:?}, \
         deltas (r,g,b) = ({dr}, {dg}, {db})"
    );
    assert_eq!(center.3, 255, "alpha opaque");
}
