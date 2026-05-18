//! Distance-attenuation curves for [`AudioSource`](crate::AudioSource).
//!
//! Per the W12 dispatch package: four curve kinds — `Linear`, `Logarithmic`,
//! `InverseSquare`, `Custom`. These are evaluated host-side so the renderer
//! and any non-Kira spatializer (e.g. an offline mix-down test) can compute
//! amplitude identically. The mapping into Kira's own `Easing`-driven
//! attenuation is a one-shot conversion in [`AudioFalloff::to_kira_easing`].
//!
//! ## Definitions
//!
//! Let `d_min`, `d_max` be the source's near / far distance, `d` the actual
//! source-to-listener distance, and `t = clamp((d - d_min) / (d_max - d_min), 0, 1)`.
//! All four curves return amplitude in `[0, 1]`:
//!
//! | Curve            | Amplitude                                     | Notes                                  |
//! |------------------|-----------------------------------------------|----------------------------------------|
//! | `Linear`         | `1 - t`                                       | Reference is a straight line.          |
//! | `Logarithmic`    | `1 / (1 + 9·t)` — base-10 shaped, fast roll-off| Approximates -20 dB at `d_max`.        |
//! | `InverseSquare`  | `(d_min / max(d, d_min))²` clamped to `0` at `d_max` | Physical free-field point source. |
//! | `Custom(a)`      | `(1 - t)^a`                                   | `a > 1` = sharper; `a < 1` = gentler.  |
//!
//! ## `InverseSquare` and the W12 exit criterion
//!
//! The W12 spec requires: "`AudioSource` at 10m `InverseSquare` falloff =
//! 1/100 amplitude vs at 1m." With `d_min = 1`, `d_max = 100` that's
//! `(1/10)² = 1/100`, which is exactly what [`AudioFalloff::amplitude`]
//! returns. See [`distance_falloff_test`](../../tests/distance_falloff_test.rs).

use serde::{Deserialize, Serialize};

/// Distance-attenuation curve attached to an [`AudioSource`](crate::AudioSource).
///
/// [`Default`] is [`Self::Linear`] — gentlest, most predictable, matches Kira's
/// default `Easing::Linear`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AudioFalloff {
    /// `1 - t` where `t` is normalised distance.
    #[default]
    Linear,
    /// Logarithmic-shaped roll-off. Approximates real-world perceived loudness
    /// drop better than [`Self::Linear`] at the cost of a steeper near-field.
    Logarithmic,
    /// Physical free-field inverse-square. Required for the W12 exit
    /// criterion: amplitude drops to `1/100` when distance increases tenfold.
    InverseSquare,
    /// `(1 - t)^exponent` — user-tunable. `exponent > 1` sharpens, `< 1`
    /// flattens. Tests use `2.0` to produce a quadratic curve.
    Custom(f32),
}

impl AudioFalloff {
    /// Compute amplitude in `[0, 1]` for a given distance and distance bounds.
    ///
    /// `distance` is metres, `min_distance` and `max_distance` are the
    /// source's [`AudioSource::distances`](crate::AudioSource::distances)
    /// pair.
    ///
    /// Behaviour at the edges:
    /// - `distance <= min_distance` → `1.0` (full volume)
    /// - `distance >= max_distance` → `0.0` (silence)
    ///
    /// Both edges hold for every curve kind so callers can mix-and-match
    /// curves without special-casing near / far field.
    #[must_use]
    pub fn amplitude(self, distance: f32, min_distance: f32, max_distance: f32) -> f32 {
        // Normalise the distance window. Guard against degenerate input
        // (max <= min) which would otherwise NaN the divisor.
        let min_distance = min_distance.max(0.0);
        let max_distance = max_distance.max(min_distance + f32::EPSILON);

        if distance <= min_distance {
            return 1.0;
        }
        if distance >= max_distance {
            return 0.0;
        }

        let t = ((distance - min_distance) / (max_distance - min_distance)).clamp(0.0, 1.0);

        match self {
            Self::Linear => 1.0 - t,
            Self::Logarithmic => 1.0 / (1.0 + 9.0 * t),
            Self::InverseSquare => {
                // (min / d)^2 — physical falloff. d is guaranteed > min by
                // the early-return above.
                let ratio = min_distance / distance;
                ratio * ratio
            }
            Self::Custom(exp) => (1.0 - t).powf(exp.max(0.0)),
        }
    }

    /// Convert to Kira's `Easing` so that the host-computed curve and the
    /// in-engine spatializer agree (within Kira's curve-family limits).
    ///
    /// Kira's spatial mixer applies the easing to a `(1 - t)` argument
    /// internally — see `kira::track` spatial source. We round-trip our
    /// shape into the closest Kira primitive:
    ///
    /// | Curve              | Kira easing            |
    /// |--------------------|------------------------|
    /// | `Linear`           | `Easing::Linear`       |
    /// | `Logarithmic`      | `Easing::OutPowf(2.5)` |
    /// | `InverseSquare`    | `Easing::OutPowi(2)`   |
    /// | `Custom(a)`        | `Easing::OutPowf(a as f64)` |
    ///
    /// Note: `InverseSquare` is only an approximation in the Kira mapping
    /// (Kira normalises distance to `min..=max` first); the host-side
    /// [`Self::amplitude`] is the source of truth for tests.
    #[must_use]
    pub fn to_kira_easing(self) -> kira::Easing {
        match self {
            Self::Linear => kira::Easing::Linear,
            Self::Logarithmic => kira::Easing::OutPowf(2.5),
            Self::InverseSquare => kira::Easing::OutPowi(2),
            Self::Custom(exp) => kira::Easing::OutPowf(f64::from(exp.max(0.0))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Boundary behaviour — every curve must return 1.0 at min and 0.0 at max.
    #[test]
    fn boundaries_are_unity_and_silence() {
        for falloff in [
            AudioFalloff::Linear,
            AudioFalloff::Logarithmic,
            AudioFalloff::InverseSquare,
            AudioFalloff::Custom(2.0),
        ] {
            assert!(
                (falloff.amplitude(1.0, 1.0, 100.0) - 1.0).abs() < 1e-6,
                "{falloff:?} not unity at d=min"
            );
            assert!(
                falloff.amplitude(100.0, 1.0, 100.0).abs() < 1e-6,
                "{falloff:?} not silent at d=max"
            );
        }
    }

    /// Linear curve halfway between min and max should be 0.5.
    #[test]
    fn linear_midpoint_is_half() {
        let amp = AudioFalloff::Linear.amplitude(50.5, 1.0, 100.0);
        assert!((amp - 0.5).abs() < 1e-3, "amp = {amp}");
    }

    /// W12 exit criterion: `InverseSquare` at 10x min distance is 1/100.
    #[test]
    fn inverse_square_decade_is_hundredth() {
        let amp = AudioFalloff::InverseSquare.amplitude(10.0, 1.0, 100.0);
        assert!(
            (amp - 0.01).abs() < 1e-6,
            "amp = {amp}, expected 0.01 ± 1e-6"
        );
    }

    /// Custom(2.0) should equal the square of (1-t).
    #[test]
    fn custom_quadratic_matches_formula() {
        let amp = AudioFalloff::Custom(2.0).amplitude(50.5, 1.0, 100.0);
        let expected = (1.0_f32 - 0.5).powi(2);
        assert!(
            (amp - expected).abs() < 1e-3,
            "amp = {amp}, expected {expected}"
        );
    }

    /// Distance < `min_distance` clamps to 1.0; distance > max clamps to 0.0.
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "asserting exact bit-equality of the early-return literals 1.0 / 0.0 emitted by the boundary branches in `amplitude`; no arithmetic is performed at these inputs"
    )]
    fn out_of_band_clamps() {
        let f = AudioFalloff::Linear;
        assert_eq!(f.amplitude(0.0, 1.0, 100.0), 1.0);
        assert_eq!(f.amplitude(-50.0, 1.0, 100.0), 1.0);
        assert_eq!(f.amplitude(500.0, 1.0, 100.0), 0.0);
    }

    /// Default falloff is Linear (matches Kira default & doc'd contract).
    #[test]
    fn default_is_linear() {
        assert_eq!(AudioFalloff::default(), AudioFalloff::Linear);
    }

    /// `to_kira_easing` maps every variant deterministically — all four
    /// curve kinds, including the exact `OutPowf` payloads for `Logarithmic`
    /// and `Custom`.
    #[test]
    fn easing_map_is_total() {
        use kira::Easing;
        assert!(matches!(
            AudioFalloff::Linear.to_kira_easing(),
            Easing::Linear
        ));
        assert!(
            matches!(
                AudioFalloff::Logarithmic.to_kira_easing(),
                Easing::OutPowf(p) if (p - 2.5).abs() < 1e-9
            ),
            "Logarithmic must map to Easing::OutPowf(2.5), got {:?}",
            AudioFalloff::Logarithmic.to_kira_easing()
        );
        assert!(matches!(
            AudioFalloff::InverseSquare.to_kira_easing(),
            Easing::OutPowi(2)
        ));
        let custom_exp = 3.0_f32;
        assert!(
            matches!(
                AudioFalloff::Custom(custom_exp).to_kira_easing(),
                Easing::OutPowf(p) if (p - f64::from(custom_exp)).abs() < 1e-9
            ),
            "Custom(exp) must map to Easing::OutPowf(exp as f64), got {:?}",
            AudioFalloff::Custom(custom_exp).to_kira_easing()
        );
    }

    /// `Custom` with a negative exponent at an in-range distance: the
    /// host-side `amplitude` clamps the exponent to `0.0` (`exp.max(0.0)`),
    /// so the result stays a finite, non-negative amplitude rather than
    /// diverging. Documents existing behaviour — does not change it.
    #[test]
    fn custom_negative_exponent_amplitude_is_finite() {
        let amp = AudioFalloff::Custom(-2.0).amplitude(50.5, 1.0, 100.0);
        assert!(amp.is_finite(), "amp = {amp} is not finite");
        assert!(amp >= 0.0, "amp = {amp} is negative");
    }

    /// Logarithmic monotonically decreases.
    #[test]
    fn logarithmic_is_monotone() {
        let f = AudioFalloff::Logarithmic;
        let a = f.amplitude(2.0, 1.0, 100.0);
        let b = f.amplitude(20.0, 1.0, 100.0);
        let c = f.amplitude(80.0, 1.0, 100.0);
        assert!(a > b && b > c, "non-monotone: {a} {b} {c}");
    }
}
