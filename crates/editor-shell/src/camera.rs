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

use glam::{Mat4, Quat, Vec3, Vec4};
use rge_cad_core::OperatorGraph;
use rge_cad_projection::picking::Ray;
use rge_cad_projection::CadProjection;
use rge_editor_state::FaceSelection;
use rge_kernel_ecs::World;

/// CPU-side **editor-runtime** camera intent — eye / target / up plus
/// perspective parameters.
///
/// This is the editor-side camera state that owns the *intent* (eye /
/// target / up / FOV / clip planes), not the derived `view_proj` matrix.
/// The matrix is recomputed each frame (or on viewport resize) by
/// [`EditorCameraState::view_proj`] from the current aspect ratio so the
/// projection matches the surface dimensions exactly.
///
/// Sub-δ.1.B fixed-camera contract: the default value places the camera
/// at `(3, 3, 3)`, looking at the world origin with `+Y` up,
/// `fov_y = π/4`, near plane `0.1`, far plane `100.0`. Three faces of a
/// 1×1×1 cuboid at the origin (the +X / +Y / +Z faces) are visible from
/// this vantage point with a directional light from `-1, -1, -1`,
/// producing the Lambert+Phong shading variation that the visual
/// verification looks for. Initial viewport navigation shipped later on top of
/// this same camera state rather than replacing it with a separate controller.
///
/// [`EditorCameraState::to_camera_view`] composes this struct into a
/// [`CameraView`] suitable for the existing
/// [`CameraView::screen_to_world_ray`] picker plumbing — sub-δ.2's
/// mouse-pick flow consumes that path. Sub-δ.1.B does NOT exercise it
/// (no mouse / picking yet); the method is shipped together because
/// editor-runtime camera intent and the screen-ray bridge are siblings
/// in the same module.
#[derive(Debug, Clone, Copy)]
pub struct EditorCameraState {
    /// Camera position in world space.
    pub eye: Vec3,
    /// World-space point the camera is looking at.
    pub target: Vec3,
    /// World-space up direction (normalised conceptually; left to caller).
    pub up: Vec3,
    /// Vertical field-of-view, radians.
    pub fov_y_radians: f32,
    /// Near clip plane (world-space distance).
    pub near: f32,
    /// Far clip plane (world-space distance).
    pub far: f32,
}

impl Default for EditorCameraState {
    fn default() -> Self {
        Self {
            eye: Vec3::new(3.0, 3.0, 3.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_radians: std::f32::consts::FRAC_PI_4,
            near: 0.1,
            far: 100.0,
        }
    }
}

impl EditorCameraState {
    /// Compute the combined view*projection matrix for the given pixel
    /// aspect ratio.
    ///
    /// Uses [`Mat4::look_at_rh`] + [`Mat4::perspective_rh`] (wgpu /
    /// Vulkan / D3D NDC convention, Z ∈ `[0.0, 1.0]`) — matches sub-β's
    /// LOAD-BEARING NDC contract on [`CameraView`]. NOT the `_gl`
    /// variants (Z ∈ `[-1.0, +1.0]`), which would make the unprojected
    /// ray and rendered geometry diverge.
    #[must_use]
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let proj = Mat4::perspective_rh(self.fov_y_radians, aspect, self.near, self.far);
        proj * view
    }

    /// Build a [`CameraView`] suitable for sub-β's
    /// [`CameraView::screen_to_world_ray`].
    ///
    /// `viewport_size` is `[width, height]` in pixels; the aspect ratio
    /// for the projection matrix is computed as `width / height`. The
    /// returned [`CameraView`]'s `viewport_size` is set to the same
    /// pair so screen→world unprojection sees consistent dimensions.
    #[must_use]
    pub fn to_camera_view(&self, viewport_size: [f32; 2]) -> CameraView {
        let aspect = viewport_size[0] / viewport_size[1];
        CameraView {
            view_proj: self.view_proj(aspect),
            viewport_size,
        }
    }

    /// Rotate [`Self::eye`] around [`Self::target`] while preserving the
    /// target, distance, up vector, FOV, and clip planes.
    pub(crate) fn orbit_around_target(&mut self, yaw_radians: f32, pitch_radians: f32) {
        if !yaw_radians.is_finite() || !pitch_radians.is_finite() {
            return;
        }
        if yaw_radians == 0.0 && pitch_radians == 0.0 {
            return;
        }

        let offset = self.eye - self.target;
        let distance = offset.length();
        if !offset.is_finite() || !distance.is_finite() || distance <= 1e-6 {
            return;
        }

        let up_len = self.up.length();
        if !self.up.is_finite() || !up_len.is_finite() || up_len <= 1e-6 {
            return;
        }
        let up = self.up / up_len;

        let yawed = Quat::from_axis_angle(up, yaw_radians) * offset;
        if !yawed.is_finite() {
            return;
        }

        let pitch_axis = yawed.cross(up);
        let pitch_axis_len = pitch_axis.length();
        let pitched = if pitch_radians != 0.0
            && pitch_axis.is_finite()
            && pitch_axis_len.is_finite()
            && pitch_axis_len > 1e-6
        {
            Quat::from_axis_angle(pitch_axis / pitch_axis_len, pitch_radians) * yawed
        } else {
            yawed
        };
        let pitched_len = pitched.length();
        if !pitched.is_finite() || !pitched_len.is_finite() || pitched_len <= 1e-6 {
            return;
        }

        self.eye = self.target + pitched * (distance / pitched_len);
    }

    /// Translate [`Self::eye`] and [`Self::target`] together in the current
    /// camera view plane.
    pub(crate) fn pan_in_view_plane(
        &mut self,
        cursor_delta: [f32; 2],
        viewport_size: [f32; 2],
    ) -> bool {
        if !cursor_delta[0].is_finite() || !cursor_delta[1].is_finite() {
            return false;
        }
        if cursor_delta[0] == 0.0 && cursor_delta[1] == 0.0 {
            return false;
        }
        if !viewport_size[0].is_finite()
            || !viewport_size[1].is_finite()
            || viewport_size[0] <= 0.0
            || viewport_size[1] <= 0.0
        {
            return false;
        }

        let forward = self.target - self.eye;
        let distance = forward.length();
        if !forward.is_finite() || !distance.is_finite() || distance <= 1e-6 {
            return false;
        }
        let forward = forward / distance;

        let up_len = self.up.length();
        if !self.up.is_finite() || !up_len.is_finite() || up_len <= 1e-6 {
            return false;
        }
        let up = self.up / up_len;

        let right = forward.cross(up);
        let right_len = right.length();
        if !right.is_finite() || !right_len.is_finite() || right_len <= 1e-6 {
            return false;
        }
        let right = right / right_len;

        let view_up = right.cross(forward);
        if !view_up.is_finite() {
            return false;
        }

        if !self.fov_y_radians.is_finite() || self.fov_y_radians <= 0.0 {
            return false;
        }
        let world_units_per_pixel =
            2.0 * distance * (self.fov_y_radians * 0.5).tan() / viewport_size[1];
        if !world_units_per_pixel.is_finite() || world_units_per_pixel <= 0.0 {
            return false;
        }

        let translation =
            (-right * cursor_delta[0] + view_up * cursor_delta[1]) * world_units_per_pixel;
        if !translation.is_finite() {
            return false;
        }

        let eye = self.eye + translation;
        let target = self.target + translation;
        if !eye.is_finite() || !target.is_finite() {
            return false;
        }

        self.eye = eye;
        self.target = target;
        true
    }
}

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

/// Compose [`CameraView::screen_to_world_ray`] +
/// [`CadProjection::pick_face`] + [`FaceSelection`] construction in one
/// call. Returns `None` when:
///
/// * `screen_to_world_ray` returns `None` (degenerate camera — non-invertible
///   `view_proj` or zero perspective-divide `w`), OR
/// * `pick_face` returns `None` (no resolvable face hit; either the ray
///   misses every entity, all hits filter out for missing owners, or every
///   hit triangle's `brep_face_id_for_triangle` returns `None`).
///
/// The returned [`FaceSelection`] carries `entity` / `owner` / `face_id`
/// from the picker's [`rge_cad_projection::FacePick`], ready to be added
/// to [`crate::coord::EditorCoord::face_selection`]. Extracted as a free
/// function (rather than an inherent method) so it's directly testable
/// without constructing a full [`crate::EditorShell`] — the click handler
/// in [`crate::lifecycle`] is a thin wrapper that supplies the cursor
/// position + viewport size + camera state from the running editor.
///
/// This is the central helper for sub-δ.2's click→select chain:
///
/// ```text
/// winit MouseInput → cursor_pos + viewport_size
///                  → editor_camera.to_camera_view(viewport)
///                  → pick_face_at(camera_view, cursor, projection, world, graph)
///                  → coord.face_selection.add(...)
/// ```
#[must_use]
pub fn pick_face_at(
    camera_view: &CameraView,
    screen_pos: [f32; 2],
    projection: &CadProjection,
    world: &World,
    graph: &OperatorGraph,
) -> Option<FaceSelection> {
    let ray = camera_view.screen_to_world_ray(screen_pos)?;
    let pick = projection.pick_face(&ray, world, graph)?;
    Some(FaceSelection {
        entity: pick.entity,
        owner: pick.owner,
        face_id: pick.face_id,
    })
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

    // -----------------------------------------------------------------------
    // Sub-δ.1.B EditorCameraState math
    // -----------------------------------------------------------------------

    /// EC1 — `EditorCameraState::default()` matches the documented
    /// editor-runtime fixed-camera contract: eye `(3, 3, 3)`, target at
    /// origin, +Y up, FOV π/4, near 0.1, far 100.0.
    #[test]
    fn editor_camera_state_default_eye_at_3_3_3() {
        let cam = EditorCameraState::default();
        assert_eq!(cam.eye, Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(cam.target, Vec3::ZERO);
        assert_eq!(cam.up, Vec3::Y);
        assert!((cam.fov_y_radians - std::f32::consts::FRAC_PI_4).abs() < f32::EPSILON);
        assert!((cam.near - 0.1).abs() < 1e-6);
        assert!((cam.far - 100.0).abs() < 1e-3);
    }

    /// EC2 — `view_proj(1.0)` (square aspect) is finite across the whole
    /// matrix. No NaN / inf entries — proves the default camera is
    /// non-degenerate under `look_at_rh + perspective_rh`.
    #[test]
    fn editor_camera_view_proj_identity_aspect_produces_finite_matrix() {
        let cam = EditorCameraState::default();
        let m = cam.view_proj(1.0);
        for v in m.to_cols_array() {
            assert!(v.is_finite(), "view_proj entry must be finite, got {v}");
        }
    }

    /// EC3 — Changing the aspect ratio changes the projection matrix
    /// only along the X axis (perspective_rh's only aspect-dependent
    /// component). The Y, Z, and W columns of the *projection* part
    /// stay identical; the view part is unchanged across aspect ratios.
    /// This proves the math composes correctly when the surface is
    /// resized.
    #[test]
    fn editor_camera_view_proj_aspect_change_affects_x_axis_only() {
        let cam = EditorCameraState::default();
        let m1 = cam.view_proj(1.0);
        let m2 = cam.view_proj(16.0 / 9.0);

        // The two matrices must differ — aspect changes the projection.
        assert!(
            !(m1 - m2).abs_diff_eq(Mat4::ZERO, 1e-6),
            "different aspects must produce different view*proj matrices"
        );
        // Helper: extract the four column vectors.
        let c1 = m1.to_cols_array_2d();
        let c2 = m2.to_cols_array_2d();

        // Column 0 of the *projection* matrix carries `1 / (aspect * tan(fov_y/2))` —
        // changes with aspect. The result `proj * view` propagates that into
        // the world's X-aligned components only. Specifically: each column of
        // the result is `proj * view_col_k`; only proj's row 0 differs across
        // aspects, so only the .x component of every result column varies.
        for k in 0..4 {
            // .x can differ.
            // .y / .z / .w must match (perspective_rh has 0 in entries 1/2/3 of
            // row 0, and identical row-1..3 entries, so y/z/w of every
            // proj * view_col are aspect-invariant).
            assert!(
                (c1[k][1] - c2[k][1]).abs() < 1e-5,
                "col {k}.y differs across aspects: {} vs {}",
                c1[k][1],
                c2[k][1]
            );
            assert!(
                (c1[k][2] - c2[k][2]).abs() < 1e-5,
                "col {k}.z differs across aspects: {} vs {}",
                c1[k][2],
                c2[k][2]
            );
            assert!(
                (c1[k][3] - c2[k][3]).abs() < 1e-5,
                "col {k}.w differs across aspects: {} vs {}",
                c1[k][3],
                c2[k][3]
            );
        }

        // And at least one .x must actually differ — proves the assertion is
        // doing real work.
        let x_diff = (0..4).any(|k| (c1[k][0] - c2[k][0]).abs() > 1e-5);
        assert!(
            x_diff,
            "at least one .x component must differ across aspects"
        );
    }

    /// EC4 — `to_camera_view([w, h])` returns a [`CameraView`] whose
    /// `viewport_size` is `[w, h]` and whose `view_proj` matches
    /// `view_proj(w/h)`. Proves the bridge from editor-runtime intent to
    /// the picker's input shape composes correctly.
    #[test]
    fn editor_camera_to_camera_view_threads_viewport_size() {
        let cam = EditorCameraState::default();
        let viewport = [1024.0_f32, 768.0_f32];
        let view = cam.to_camera_view(viewport);
        assert_eq!(view.viewport_size, viewport);
        let expected = cam.view_proj(viewport[0] / viewport[1]);
        let actual = view.view_proj;
        // Element-wise tolerance (perspective_rh / look_at_rh accumulate ~1e-6).
        for k in 0..16 {
            let a = actual.to_cols_array()[k];
            let e = expected.to_cols_array()[k];
            assert!(
                (a - e).abs() < 1e-5,
                "view_proj entry {k} mismatch: actual={a}, expected={e}"
            );
        }
    }

    /// EC5 — `to_camera_view` composes with `screen_to_world_ray` such
    /// that the viewport-center ray under the default camera (`(3, 3, 3)`
    /// looking at origin) emerges from near the eye and points roughly
    /// toward the target. Reuses sub-β's `CameraView` API and the
    /// existing picker `Ray` shape.
    #[test]
    fn editor_camera_view_proj_consistent_with_camera_view_screen_center() {
        let cam = EditorCameraState::default();
        let viewport = [1024.0_f32, 768.0_f32];
        let view = cam.to_camera_view(viewport);
        let ray = view
            .screen_to_world_ray([viewport[0] / 2.0, viewport[1] / 2.0])
            .expect("default camera is non-degenerate");

        // Origin should lie on the line from eye toward target — the
        // screen-center ray under a perspective camera at `eye` looking
        // at `target` puts the near-plane intersection along that line.
        // Specifically, the *direction* must point from eye toward target
        // (parametrised in world space); the closest point on the ray to
        // `target` is within perspective tolerance of `target`.
        let o = Vec3::from(ray.origin);
        let d = Vec3::from(ray.direction);
        let len_sq = d.length_squared();
        assert!(
            len_sq > 0.0,
            "non-degenerate camera must have non-zero ray direction"
        );
        let to_target = cam.target - o;
        let t_star = to_target.dot(d) / len_sq;
        let closest = o + d * t_star;
        let dist = (closest - cam.target).length();
        assert!(
            dist < 1e-2,
            "screen-center ray under default camera must pass near `target`; \
             closest point distance = {dist} (closest = {closest:?}, target = {:?})",
            cam.target
        );

        // Direction must point *away from* eye toward target (positive dot
        // with `(target - eye)`). Substantively: a screen-center ray
        // shouldn't be flipped.
        let toward = (cam.target - cam.eye).normalize();
        let d_norm = d.normalize();
        assert!(
            toward.dot(d_norm) > 0.5,
            "screen-center ray direction must align with eye→target; \
             got dot = {}",
            toward.dot(d_norm)
        );
    }

    // -----------------------------------------------------------------------
    // Sub-δ.2 `pick_face_at` free function — composes
    //   `screen_to_world_ray` + `pick_face` + `FaceSelection`.
    //
    // The world+projection+entity construction pattern mirrors
    // `crates/cad-projection/tests/face_picking_smoke.rs` and
    // `crates/editor-shell/tests/camera_picker_smoke.rs` — same
    // `CadGraph` + `Cuboid` + `BRepHandle.brep_owner = ENTITY_OWNER`
    // setup so behaviour is bit-for-bit comparable.
    // -----------------------------------------------------------------------

    use rge_cad_core::{
        BRepFaceId, BRepOwnerId, CadGraph, CuboidFaceTag, CuboidOp, OperatorNode, Tolerance,
    };
    use rge_cad_projection::{BRepHandle, CadProjection};
    use rge_editor_state::{FaceSelection, FaceSelectionSet};
    use rge_kernel_ecs::World;

    const ENTITY_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

    fn tol() -> Tolerance {
        Tolerance::new(0.001).expect("tolerance")
    }

    /// Build a `(graph, projection, world, entity)` tuple with a single
    /// 1×1×1 cuboid committed and projected at origin under
    /// [`ENTITY_OWNER`]. Mirrors the pattern in
    /// `tests/camera_picker_smoke.rs::build_unit_cuboid`.
    fn build_unit_cuboid() -> (CadGraph, CadProjection, World, rge_kernel_ecs::EntityId) {
        let mut graph = CadGraph::new();
        graph.begin_operation().expect("begin");
        let cuboid_node = graph
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width: 1.0,
                height: 1.0,
                depth: 1.0,
            }))
            .expect("add cuboid");
        graph
            .graph_mut()
            .expect("mut2")
            .set_root(cuboid_node)
            .expect("set root");
        graph.commit("cuboid").expect("commit");

        let mut projection = CadProjection::new();
        let mut world = World::new();
        world.register_snapshot_component::<BRepHandle>();
        let entity = projection
            .spawn_brep_entity(&mut world, cuboid_node)
            .expect("spawn");
        if let Some(mut em) = world.entity_mut(entity) {
            if let Some(mut handle) = em.get_mut::<BRepHandle>() {
                handle.brep_owner = Some(ENTITY_OWNER);
            }
        }
        projection.tick(&mut world, &graph, tol()).expect("tick");

        (graph, projection, world, entity)
    }

    /// `Mat4::look_at_rh + Mat4::perspective_rh` from `(0, 0, 5)` toward
    /// origin — same camera the existing `camera_picker_smoke.rs` uses
    /// so the screen-center ray hits the cuboid's +Z face.
    fn editor_camera_view_proj(viewport: [f32; 2]) -> Mat4 {
        let view = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );
        let aspect = viewport[0] / viewport[1];
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
        proj * view
    }

    /// **PFA-1** — viewport-center pixel under a camera at `(0, 0, 5)`
    /// looking down -Z; `pick_face_at` returns
    /// `Some(FaceSelection { entity, owner: ENTITY_OWNER,
    /// face_id: PosZ })`. End-to-end demonstration of "click center →
    /// FaceSelection" through the helper.
    #[test]
    fn pick_face_at_returns_face_selection_for_screen_center_on_cuboid() {
        let (graph, projection, world, entity) = build_unit_cuboid();
        let viewport = [800.0_f32, 600.0_f32];
        let camera_view = CameraView {
            view_proj: editor_camera_view_proj(viewport),
            viewport_size: viewport,
        };

        let selection = pick_face_at(
            &camera_view,
            [viewport[0] / 2.0, viewport[1] / 2.0],
            &projection,
            &world,
            graph.graph(),
        )
        .expect("center-screen ray under camera at z=+5 must yield a FaceSelection");

        assert_eq!(selection.entity, entity, "the unit cuboid entity");
        assert_eq!(selection.owner, ENTITY_OWNER);
        assert_eq!(
            selection.face_id,
            BRepFaceId::for_cuboid_face(ENTITY_OWNER, CuboidFaceTag::PosZ),
            "the +Z (top) face under ENTITY_OWNER"
        );
    }

    /// **PFA-2** — far-off-axis screen pixel; ray exits the cuboid's
    /// silhouette → picker returns `None` → `pick_face_at` returns
    /// `None`. The `?` early-returns at the second `?` (pick_face).
    #[test]
    fn pick_face_at_returns_none_for_off_axis_screen_pos_far_from_cuboid() {
        let (graph, projection, world, _entity) = build_unit_cuboid();
        let viewport = [800.0_f32, 600.0_f32];
        let camera_view = CameraView {
            view_proj: editor_camera_view_proj(viewport),
            viewport_size: viewport,
        };

        // Top-left-ish pixel — far off the centered cuboid's silhouette.
        let pick = pick_face_at(
            &camera_view,
            [50.0, 50.0],
            &projection,
            &world,
            graph.graph(),
        );
        assert!(
            pick.is_none(),
            "off-axis ray must miss centered cuboid; got {pick:?}"
        );
    }

    /// **PFA-3** — degenerate `Mat4::ZERO` view_proj is non-invertible;
    /// `screen_to_world_ray` returns `None`; the `?` early-returns at
    /// the first `?` so `pick_face_at` returns `None` without ever
    /// invoking the picker. Documents the substrate-honest contract.
    #[test]
    fn pick_face_at_returns_none_when_camera_view_unprojection_fails() {
        let (graph, projection, world, _entity) = build_unit_cuboid();
        let camera_view = CameraView {
            view_proj: Mat4::ZERO,
            viewport_size: [1024.0, 768.0],
        };
        let pick = pick_face_at(
            &camera_view,
            [512.0, 384.0],
            &projection,
            &world,
            graph.graph(),
        );
        assert!(
            pick.is_none(),
            "degenerate camera_view must yield None even if geometry is present; got {pick:?}"
        );
    }

    /// **PFA-4** — composes into [`FaceSelectionSet`] (the editor's
    /// face-selection container). Smallest end-to-end demonstration
    /// of the full "click → coord.face_selection.add" routing; mirrors
    /// the LOAD-BEARING pattern from sub-β's
    /// `camera_view_composes_into_face_selection_set` integration test.
    #[test]
    fn pick_face_at_composes_into_face_selection_set() {
        let (graph, projection, world, entity) = build_unit_cuboid();
        let viewport = [800.0_f32, 600.0_f32];
        let camera_view = CameraView {
            view_proj: editor_camera_view_proj(viewport),
            viewport_size: viewport,
        };

        let selection: FaceSelection = pick_face_at(
            &camera_view,
            [viewport[0] / 2.0, viewport[1] / 2.0],
            &projection,
            &world,
            graph.graph(),
        )
        .expect("hit");

        let mut set = FaceSelectionSet::default();
        let added = set.add(selection);
        assert!(added, "fresh add must report newly-added");
        assert!(
            set.contains(&selection),
            "FaceSelectionSet must contain the just-added FaceSelection"
        );
        assert_eq!(set.len(), 1);
        assert_eq!(selection.entity, entity);
        assert_eq!(selection.owner, ENTITY_OWNER);
        assert_eq!(
            selection.face_id,
            BRepFaceId::for_cuboid_face(ENTITY_OWNER, CuboidFaceTag::PosZ),
        );
    }
}
