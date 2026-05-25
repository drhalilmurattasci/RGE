//! [`Camera`] — projection + viewport + render priority for camera entities.
//!
//! Per PLAN.md §1.5.1 the camera role pairs `Transform` + `Camera` + `Name`.

use serde::{Deserialize, Serialize};

/// Camera projection variant.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Projection {
    /// Standard perspective projection.
    Perspective {
        /// Vertical field of view, radians.
        fov_y_radians: f32,
        /// Near plane distance, meters.
        near: f32,
        /// Far plane distance, meters.
        far: f32,
    },
    /// Orthographic projection — typical for editor 2D views and ortho
    /// gameplay cameras.
    Orthographic {
        /// Half the camera's vertical extent in world units.
        half_height: f32,
        /// Near plane distance, meters.
        near: f32,
        /// Far plane distance, meters.
        far: f32,
    },
}

impl Default for Projection {
    fn default() -> Self {
        // 60 deg FOV, 0.05..1000 m — Bevy / Godot default range.
        Self::Perspective {
            fov_y_radians: std::f32::consts::FRAC_PI_3,
            near: 0.05,
            far: 1000.0,
        }
    }
}

/// Camera component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    /// Projection mode + parameters.
    pub projection: Projection,
    /// Viewport rectangle as fractions of the render target
    /// (`[x_min, y_min, x_max, y_max]`, all in `[0, 1]`).
    pub viewport: [f32; 4],
    /// Render-order priority. Higher renders later (so it overlays earlier
    /// cameras). Editor PIE uses 100; main game cam uses 0.
    pub priority: i32,
    /// `true` if this camera is currently driving a render target. The
    /// renderer skips inactive cameras even if they have a viewport set.
    pub is_active: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            projection: Projection::default(),
            viewport: [0.0, 0.0, 1.0, 1.0],
            priority: 0,
            is_active: true,
        }
    }
}

impl rge_kernel_ecs::Component for Camera {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_default_camera() {
        let c = Camera::default();
        let s = ron::to_string(&c).expect("serialize");
        let back: Camera = ron::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }

    #[test]
    fn round_trip_ron_orthographic() {
        let c = Camera {
            projection: Projection::Orthographic {
                half_height: 5.0,
                near: -10.0,
                far: 10.0,
            },
            viewport: [0.0, 0.0, 0.5, 1.0],
            priority: 50,
            is_active: false,
        };
        let s = ron::to_string(&c).expect("serialize");
        let back: Camera = ron::from_str(&s).expect("deserialize");
        assert_eq!(c, back);
    }

    #[test]
    fn default_priority_is_zero() {
        assert_eq!(Camera::default().priority, 0);
    }
}
