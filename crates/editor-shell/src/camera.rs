//! `editor-shell::camera` — CPU-side view-state and screen→world ray
//! contract for the editor's picking / interaction path.
//!
//! Independent of [`rge_gfx::Camera`] (which is a GPU UBO holder, not
//! CPU-readable). [`CameraView`] is a small struct over the combined
//! view*projection matrix and viewport pixel dimensions, with a
//! [`CameraView::screen_to_world_ray`] method that produces the existing
//! picker's [`rge_cad_projection::picking::Ray`] shape.
//!
//! # Render-backed face-selection sub-β substrate
//!
//! Sub-α (`brep-render`) shipped the CPU-side flat-shaded mesh-conversion
//! substrate. Sub-β (this module) ships the unproject primitive. Composes
//! into the eventual mouse-event handler (sub-δ) like:
//!
//! ```text
//! winit cursor → CameraView::screen_to_world_ray
//!              → cad_projection::CadProjection::pick_face
//!              → editor_state::FaceSelection
//!              → EditorCoord.face_selection
//! ```
//!
//! The chapter is **headless** through sub-β — caller composes
//! `view_proj`, sub-β unprojects, the picker resolves a face. Sub-γ
//! brings it onscreen.
//!
//! # NDC convention (LOAD-BEARING)
//!
//! `view_proj` is assumed to follow the **wgpu / Vulkan / D3D** NDC
//! convention: clip-space / NDC Z ∈ `[0.0, 1.0]` with `0.0` at the near
//! plane and `1.0` at the far plane. This matches `glam::Mat4::
//! perspective_rh()` and `glam::Mat4::orthographic_rh()` (NOT the `_gl`
//! variants, which use `[-1.0, +1.0]`).
//!
//! If the caller's view_proj uses GL convention (Z ∈ -1..+1), the
//! unprojected ray will be off — caller must use a wgpu-convention
//! projection or transform their matrix.
//!
//! # Y-flip convention
//!
//! `screen_pos` is in pixels with `(0, 0)` at the **top-left** (matching
//! winit's `WindowEvent::CursorMoved` event coordinates). NDC Y is
//! flipped on the way in (`ndc_y = 1.0 - 2.0 * (screen_y /
//! viewport_height)`).
//!
//! # Layering
//!
//! `editor-shell` is editor-tier (NOT renderer-tier). The `forbidden-dep`
//! lint's rule 6 (renderer crates may not depend on game-domain crates)
//! does NOT apply here — editor-shell is permitted to depend on
//! `rge-cad-projection` in production. The CameraView return type
//! `Option<Ray>` is the existing picker's input shape, so this module
//! sits at the "editor coordination ↔ projection query" seam.

use glam::{Mat4, Vec3, Vec4};
use rge_cad_projection::picking::Ray;

/// CPU-side view state for screen ↔ world conversion.
///
/// Holds the combined view * projection matrix and viewport pixel
/// dimensions. View / projection are caller-composed (`Mat4::look_at_rh`
/// + `Mat4::perspective_rh`, or whatever the editor uses) — this
/// substrate does NOT prescribe view math.
#[derive(Debug, Clone, Copy)]
pub struct CameraView {
    /// Combined view * projection matrix.
    pub view_proj: Mat4,
    /// Viewport size in pixels (`[width, height]`).
    pub viewport_size: [f32; 2],
}

impl CameraView {
    /// Convert a screen-space pixel position to a world-space [`Ray`].
    ///
    /// `screen_pos` is in pixels with `(0, 0)` at the top-left (winit
    /// cursor convention). The returned ray's `origin` is the near-plane
    /// intersection point in world space; `direction` points from near
    /// to far (NOT unit-length — matches the picker's `Ray` contract
    /// that direction need not be normalised).
    ///
    /// Returns `None` if `view_proj.inverse()` produces a non-finite
    /// matrix (degenerate camera — e.g., zero-determinant projection)
    /// or if a perspective divide hits a zero `w` component. Callers
    /// are expected to supply a sane camera at editor runtime; the
    /// `Option` shape is the substrate-honest contract for the
    /// degenerate case rather than a silent panic.
    #[must_use]
    pub fn screen_to_world_ray(&self, screen_pos: [f32; 2]) -> Option<Ray> {
        // Screen → NDC (Y-flipped to convert from winit top-left to
        // wgpu/Vulkan/D3D bottom-left NDC).
        let [vw, vh] = self.viewport_size;
        let ndc_x = 2.0 * (screen_pos[0] / vw) - 1.0;
        let ndc_y = 1.0 - 2.0 * (screen_pos[1] / vh);

        // Inverse view_proj with finiteness check. glam's `inverse()`
        // returns a Mat4 with NaN/inf entries for non-invertible inputs;
        // we treat those as the "degenerate camera" case and return None.
        let inv = self.view_proj.inverse();
        for col in inv.to_cols_array() {
            if !col.is_finite() {
                return None;
            }
        }

        // NDC near/far points (wgpu/Vulkan/D3D convention, Z ∈ 0..1).
        let near_clip = Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far_clip = Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

        // Unproject — multiply by inverse view_proj.
        let near_world = inv * near_clip;
        let far_world = inv * far_clip;

        // Perspective divide. A zero `w` indicates a degenerate
        // unprojection (caller's view_proj is pathological); return
        // None per the substrate-honest contract.
        if near_world.w == 0.0 || far_world.w == 0.0 {
            return None;
        }
        let near = near_world.truncate() / near_world.w;
        let far = far_world.truncate() / far_world.w;

        // The picker's `Ray::direction` need NOT be unit-length.
        let dir: Vec3 = far - near;

        Some(Ray {
            origin: [near.x, near.y, near.z],
            direction: [dir.x, dir.y, dir.z],
        })
    }
}

#[cfg(test)]
mod tests {
    use glam::Mat4;

    use super::*;

    /// Tolerance for the trivially-correct identity-matrix tests where
    /// the unprojection accumulates only the screen→NDC arithmetic.
    const EPS_IDENTITY: f32 = 1e-5;

    /// Tolerance for tests that go through `look_at_rh` + `perspective_rh`
    /// — perspective math accumulates float error on the order of 1e-4.
    const EPS_PERSPECTIVE: f32 = 1e-3;

    /// Test 1 — identity view_proj, screen center yields a ray through
    /// world origin pointing along +Z (NDC near→far is Z 0→1 under
    /// wgpu convention).
    #[test]
    fn identity_view_proj_screen_center_yields_ray_through_origin() {
        let cam = CameraView {
            view_proj: Mat4::IDENTITY,
            viewport_size: [1024.0, 768.0],
        };
        let ray = cam
            .screen_to_world_ray([512.0, 384.0])
            .expect("identity matrix is invertible");

        // Origin at NDC center (0, 0) and near plane (Z=0).
        assert!(
            ray.origin[0].abs() < EPS_IDENTITY,
            "origin.x ≈ 0, got {}",
            ray.origin[0]
        );
        assert!(
            ray.origin[1].abs() < EPS_IDENTITY,
            "origin.y ≈ 0, got {}",
            ray.origin[1]
        );
        assert!(
            ray.origin[2].abs() < EPS_IDENTITY,
            "origin.z ≈ 0 (near plane under wgpu Z=0..1 NDC), got {}",
            ray.origin[2]
        );

        // Direction along +Z (near Z=0 → far Z=1 → delta = +1.0 in Z).
        assert!(
            ray.direction[0].abs() < EPS_IDENTITY,
            "direction.x ≈ 0, got {}",
            ray.direction[0]
        );
        assert!(
            ray.direction[1].abs() < EPS_IDENTITY,
            "direction.y ≈ 0, got {}",
            ray.direction[1]
        );
        assert!(
            (ray.direction[2] - 1.0).abs() < EPS_IDENTITY,
            "direction.z ≈ +1 (NDC near→far under wgpu Z=0..1), got {}",
            ray.direction[2]
        );
    }

    /// Test 2 — top-left corner pixel (`[0, 0]`) maps to NDC `(-1, +1)`
    /// after the Y-flip; identity view_proj propagates through
    /// unchanged so the ray origin should match.
    #[test]
    fn identity_view_proj_top_left_corner_ray_origin_at_top_left_ndc() {
        let cam = CameraView {
            view_proj: Mat4::IDENTITY,
            viewport_size: [800.0, 600.0],
        };
        let ray = cam
            .screen_to_world_ray([0.0, 0.0])
            .expect("identity invertible");
        assert!(
            (ray.origin[0] - (-1.0)).abs() < EPS_IDENTITY,
            "origin.x ≈ -1 (NDC left), got {}",
            ray.origin[0]
        );
        assert!(
            (ray.origin[1] - 1.0).abs() < EPS_IDENTITY,
            "origin.y ≈ +1 (NDC top after Y-flip), got {}",
            ray.origin[1]
        );
        assert!(
            ray.origin[2].abs() < EPS_IDENTITY,
            "origin.z ≈ 0 (near plane, wgpu convention), got {}",
            ray.origin[2]
        );
    }

    /// Test 3 — bottom-center screen pixel maps to NDC y ≈ -1
    /// confirming the Y-flip from winit top-left to wgpu bottom-left.
    #[test]
    fn viewport_y_flip_correctness() {
        let viewport = [1024.0_f32, 768.0_f32];
        let cam = CameraView {
            view_proj: Mat4::IDENTITY,
            viewport_size: viewport,
        };
        // Bottom-center pixel: x = width/2, y = height (one past bottom
        // edge inclusive — we want screen_y == viewport_height which
        // maps to ndc_y = 1.0 - 2.0 * 1.0 = -1.0).
        let ray = cam
            .screen_to_world_ray([viewport[0] / 2.0, viewport[1]])
            .expect("identity invertible");
        assert!(
            ray.origin[0].abs() < EPS_IDENTITY,
            "bottom-CENTER → x ≈ 0, got {}",
            ray.origin[0]
        );
        assert!(
            (ray.origin[1] - (-1.0)).abs() < EPS_IDENTITY,
            "bottom of screen → ndc_y ≈ -1 (Y-flip honored), got {}",
            ray.origin[1]
        );
    }

    /// Test 4 — full view+perspective camera at `(0, 0, 5)` looking at
    /// origin: screen-center ray must pass through world origin within
    /// perspective-tolerance.
    #[test]
    fn perspective_camera_origin_at_5_screen_center_ray_through_origin() {
        let view = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        let proj = Mat4::perspective_rh(
            std::f32::consts::FRAC_PI_4, // 45°
            1.0,                         // square aspect
            0.1,
            100.0,
        );
        let view_proj = proj * view;
        let cam = CameraView {
            view_proj,
            viewport_size: [800.0, 800.0],
        };
        let ray = cam
            .screen_to_world_ray([400.0, 400.0])
            .expect("non-degenerate perspective is invertible");

        // The ray must pass through the world origin. Compute the
        // closest point on the ray to (0, 0, 0) and verify within
        // perspective tolerance.
        let o = Vec3::from(ray.origin);
        let d = Vec3::from(ray.direction);
        let len_sq = d.length_squared();
        assert!(
            len_sq > 0.0,
            "non-degenerate camera must yield non-zero direction"
        );
        // Parametric closest-point: t* = -dot(o, d) / dot(d, d)
        let t_star = -o.dot(d) / len_sq;
        let closest = o + d * t_star;
        let dist = closest.length();
        assert!(
            dist < EPS_PERSPECTIVE,
            "closest point on the screen-center ray to world origin should \
             be within {EPS_PERSPECTIVE} of origin; was {dist} (closest={closest:?})"
        );
    }

    /// Test 5 — under a non-identity perspective camera the picker's
    /// `Ray.direction` is NOT unit-length. Matches the existing
    /// `cad_projection::picking::Ray` contract that the direction need
    /// not be normalised.
    #[test]
    fn direction_is_not_normalized_per_picker_ray_contract() {
        let view = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.5, 0.1, 100.0);
        let cam = CameraView {
            view_proj: proj * view,
            viewport_size: [1200.0, 800.0],
        };
        let ray = cam.screen_to_world_ray([300.0, 200.0]).expect("invertible");
        let len = Vec3::from(ray.direction).length();
        assert!(
            (len - 1.0).abs() > 1e-3,
            "Ray.direction must NOT be unit-length per picker contract; \
             got length = {len}"
        );
    }

    /// Test 6 — degenerate `Mat4::ZERO` view_proj is non-invertible;
    /// the substrate-honest contract returns `None` rather than panic.
    #[test]
    fn degenerate_zero_view_proj_returns_none() {
        let cam = CameraView {
            view_proj: Mat4::ZERO,
            viewport_size: [800.0, 600.0],
        };
        let pick = cam.screen_to_world_ray([400.0, 300.0]);
        assert!(
            pick.is_none(),
            "non-invertible view_proj must yield None; got {pick:?}"
        );
    }

    /// Test 7 — identity view_proj, screen center: near plane Z must be
    /// 0 (wgpu/Vulkan/D3D NDC convention), NOT -1 (GL convention).
    /// Documents the convention assumption directly in test code.
    #[test]
    fn near_plane_z_is_zero_per_wgpu_convention() {
        let cam = CameraView {
            view_proj: Mat4::IDENTITY,
            viewport_size: [800.0, 600.0],
        };
        let ray = cam
            .screen_to_world_ray([400.0, 300.0])
            .expect("identity invertible");
        assert!(
            ray.origin[2].abs() < EPS_IDENTITY,
            "near plane must be at Z=0 (wgpu NDC convention), NOT Z=-1 \
             (GL NDC convention); got origin.z = {}",
            ray.origin[2]
        );
        // And the far plane delta must be +1 (Z=0 → Z=1, total delta
        // along ray = +1.0 in z) — NOT +2 which would be the GL case.
        assert!(
            (ray.direction[2] - 1.0).abs() < EPS_IDENTITY,
            "near→far delta in Z must be +1 (wgpu Z=0..1), NOT +2 (GL \
             Z=-1..+1); got direction.z = {}",
            ray.direction[2]
        );
    }
}
