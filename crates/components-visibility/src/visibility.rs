// adapted from rustforge::runtime-tags::predicates on 2026-05-05 — replaced the
//                                                       interned-tag tri-state with
//                                                       a simple enum since the
//                                                       components crate does not
//                                                       depend on the tag interner.
//
//! [`Visibility`] — tri-state visibility component.
//!
//! Per PLAN.md §1.5.1, every renderable entity carries a `Visibility`. The
//! `Inherited` variant means "use my parent's effective visibility" — the
//! transform-propagation system resolves the chain.

use serde::{Deserialize, Serialize};

/// Tri-state visibility.
///
/// `Inherited` is the default for non-root entities so that a single edit on
/// a parent flows down without per-child churn.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Visibility {
    /// Force-visible regardless of parent.
    Visible,
    /// Force-hidden regardless of parent.
    Hidden,
    /// Take the effective state from the parent (root = `Visible`).
    #[default]
    Inherited,
}

impl rge_kernel_ecs::Component for Visibility {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron_visible() {
        let v = Visibility::Visible;
        let s = ron::to_string(&v).expect("serialize");
        let back: Visibility = ron::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn round_trip_ron_hidden() {
        let v = Visibility::Hidden;
        let s = ron::to_string(&v).expect("serialize");
        let back: Visibility = ron::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn round_trip_ron_inherited() {
        let v = Visibility::Inherited;
        let s = ron::to_string(&v).expect("serialize");
        let back: Visibility = ron::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn default_is_inherited() {
        assert_eq!(Visibility::default(), Visibility::Inherited);
    }
}
