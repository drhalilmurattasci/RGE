// adapted from rustforge::runtime-color::space on 2026-05-05 — kept the linear
//                                                  RGB intensity convention; light
//                                                  color is scene-linear sRGB,
//                                                  intensity in nits (lumens/sr/m^2)
//                                                  for directional, lumens for point/
//                                                  spot. ColorSpace enum re-introduced
//                                                  by W18 (io-image).
//
//! [`Light`] — directional / point / spot light component.
//!
//! Per PLAN.md §1.5.1 every light entity carries `Transform` + `Light` +
//! `Name`. Shadow-map binding lives in the optional `ShadowMap` component
//! (introduced by the gfx wave).

use serde::{Deserialize, Serialize};

/// Light type discriminant + per-variant parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LightKind {
    /// Distant light (sun / moon). Position ignored; only orientation matters.
    Directional {
        /// Illuminance at the receiving plane, lux.
        illuminance_lux: f32,
    },
    /// Omnidirectional point light.
    Point {
        /// Luminous intensity, lumens.
        lumens: f32,
        /// Maximum effective range, meters. Beyond this, attenuation is
        /// clamped to zero so the renderer can spatially cull.
        range_m: f32,
    },
    /// Cone-shaped spot light.
    Spot {
        /// Luminous intensity, lumens.
        lumens: f32,
        /// Maximum effective range, meters.
        range_m: f32,
        /// Inner cone half-angle (full intensity), radians.
        inner_angle_radians: f32,
        /// Outer cone half-angle (zero intensity), radians.
        outer_angle_radians: f32,
    },
}

impl Default for LightKind {
    fn default() -> Self {
        // Mid-day sunlight ~100 klux.
        Self::Directional {
            illuminance_lux: 100_000.0,
        }
    }
}

/// Light component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Light {
    /// Linear sRGB color tint (R, G, B).
    pub color: [f32; 3],
    /// Type-specific intensity / falloff parameters.
    pub kind: LightKind,
    /// Whether this light contributes to indirect / GI passes.
    pub affects_indirect: bool,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0],
            kind: LightKind::default(),
            affects_indirect: true,
        }
    }
}

impl rge_kernel_ecs::Component for Light {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_default_directional() {
        let l = Light::default();
        let s = ron::to_string(&l).expect("serialize");
        let back: Light = ron::from_str(&s).expect("deserialize");
        assert_eq!(l, back);
    }

    #[test]
    fn round_trip_ron_point_light() {
        let l = Light {
            color: [1.0, 0.85, 0.7],
            kind: LightKind::Point {
                lumens: 800.0,
                range_m: 25.0,
            },
            affects_indirect: false,
        };
        let s = ron::to_string(&l).expect("serialize");
        let back: Light = ron::from_str(&s).expect("deserialize");
        assert_eq!(l, back);
    }

    #[test]
    fn round_trip_ron_spot_light() {
        let l = Light {
            color: [0.5, 0.5, 1.0],
            kind: LightKind::Spot {
                lumens: 1500.0,
                range_m: 40.0,
                inner_angle_radians: 0.3,
                outer_angle_radians: 0.6,
            },
            affects_indirect: true,
        };
        let s = ron::to_string(&l).expect("serialize");
        let back: Light = ron::from_str(&s).expect("deserialize");
        assert_eq!(l, back);
    }
}
