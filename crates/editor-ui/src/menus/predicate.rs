//! Visibility / enablement predicates for menu entries.
//!
//! adapted from rustforge::apps::editor-app::egui_overlay (menu bar) on 2026-05-05
//! — rebuilt as data-driven `MenuRegistry`.
//!
//! The rustforge prior art let the host gate menu items by branching
//! before construction (e.g. only emit "Stop" when `play_state ==
//! Playing`). That works for a host that owns its menu but does not
//! survive plugin authorship: a plugin cannot shadow a host's
//! conditional branch.
//!
//! The registry installs the predicate as data on the [`MenuEntry`]
//! and evaluates it during resolve. Two backends:
//!
//! - [`Predicate::Closure`] — a Rust callback, the always-available path.
//! - [`Predicate::Expr`] — an `expr-wasm` handle (W19); falls through
//!   to a stub that the host wires up when the upstream crate lands.
//!
//! [`MenuEntry`]: crate::menus::MenuEntry

use std::sync::Arc;

use crate::menus::ExprHandle;

/// Opaque context passed to predicate closures and expressions.
///
/// Today the registry hands an empty context (the registry itself is
/// data-only); the editor-shell composes its own context with the
/// active selection / play-state / focused tab when it dispatches the
/// resolve call. Plugins cannot smuggle host state into the predicate
/// — they receive the same opaque context. The container is left
/// non-exhaustive so editor-shell can extend without a breaking
/// change here.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct PredicateContext {
    /// Current play state name (`"editing"`, `"playing"`, `"paused"`,
    /// `"stepping"`). Empty string when the host is not in a play
    /// session. Stored as a string so it survives the
    /// editor-shell ↔ editor-ui boundary without a cyclic dep.
    pub play_state: String,
    /// `true` when at least one entity is selected. Mirrors the
    /// rustforge `selection.is_some()` predicate idiom.
    pub has_selection: bool,
    /// Focused tab id (`""` when no tab is focused). Lets predicates
    /// gate "Save" on whether the focused tab is dirty without baking
    /// that policy into editor-ui.
    pub focused_tab: String,
    /// `true` when starting Play is available (the `PlayState::can_play`
    /// authority, filled shell-side). Gates the Play menu item's ENABLEMENT
    /// (not visibility). Default `false`.
    pub can_play: bool,
    /// `true` when Pause is available (`PlayState::can_pause`). Gates the Pause
    /// item's enablement. Default `false`.
    pub can_pause: bool,
    /// `true` when Stop is available (`PlayState::can_stop`). Gates the Stop
    /// item's enablement. Default `false`.
    pub can_stop: bool,
    /// `true` when Step is available (`PlayState::can_step`). Gates the Step
    /// item's enablement. Default `false`.
    pub can_step: bool,
    /// `true` only in the Editing state (PIE not active). Gates the File
    /// Save / Open / Save-As items, which no-op outside Editing. Default `false`.
    pub is_editing: bool,
    /// `true` when View -> Reset Camera would frame a live scene's renderable
    /// bounds instead of falling back to the default camera pose. Default `false`.
    pub has_frameable_scene: bool,
}

/// Type alias for the closure form. `Arc` so [`Predicate`] is `Clone`;
/// `Send + Sync` so plugins can ship predicates from background
/// threads.
pub type PredicateFn = Arc<dyn Fn(&PredicateContext) -> bool + Send + Sync>;

/// Visibility / enablement test for a menu entry. Evaluated during
/// resolve. The "always visible" path is [`Predicate::AlwaysVisible`];
/// callers that always allow can skip the field entirely (entry
/// builders default to it).
#[derive(Clone)]
pub enum Predicate {
    /// Constant `true` — the cheapest path, used as the default. No
    /// closure storage, no expr-wasm round-trip.
    AlwaysVisible,
    /// A Rust closure. Stored behind `Arc<dyn Fn>` so [`Predicate`]
    /// stays `Clone` without forcing the closure type to be `Copy`.
    /// The closure is `Send + Sync` so background-thread plugins can
    /// hand off predicates safely.
    Closure(PredicateFn),
    /// An `expr-wasm` expression handle. The W19 crate compiles a
    /// single expression to WASM; the editor-shell evaluates it via
    /// the script-host. While W19 is unmerged the [`ExprHandle`]
    /// stub round-trips the raw source — the registry stores the
    /// handle and defers evaluation to the host.
    Expr(ExprHandle),
}

impl Predicate {
    /// The default "always allowed" predicate.
    #[must_use]
    pub const fn always_visible() -> Self {
        Self::AlwaysVisible
    }

    /// Wrap a Rust closure as a predicate.
    #[must_use]
    pub fn from_fn<F>(f: F) -> Self
    where
        F: Fn(&PredicateContext) -> bool + Send + Sync + 'static,
    {
        Self::Closure(Arc::new(f))
    }

    /// Wrap an expr-wasm source string as a predicate. The string is
    /// the expression body; validation is deferred to the W19
    /// compiler.
    #[must_use]
    pub fn from_expr(source: impl Into<String>) -> Self {
        Self::Expr(ExprHandle::new(source))
    }

    /// Evaluate the predicate against a context.
    ///
    /// - [`Self::AlwaysVisible`] returns `true`.
    /// - [`Self::Closure`] calls the closure.
    /// - [`Self::Expr`] returns `true` while W19 is unmerged — the
    ///   editor-shell is expected to short-circuit `Expr` predicates
    ///   through the real expr-wasm runtime once available; today
    ///   "always allow" is the safest fallback (entries stay visible
    ///   instead of vanishing).
    #[must_use]
    pub fn evaluate(&self, ctx: &PredicateContext) -> bool {
        match self {
            Self::AlwaysVisible => true,
            Self::Closure(f) => (f)(ctx),
            // W19 stub: defer to host.
            Self::Expr(_) => true,
        }
    }
}

impl Default for Predicate {
    fn default() -> Self {
        Self::AlwaysVisible
    }
}

impl core::fmt::Debug for Predicate {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AlwaysVisible => f.write_str("Predicate::AlwaysVisible"),
            Self::Closure(_) => f.write_str("Predicate::Closure(<fn>)"),
            Self::Expr(h) => write!(f, "Predicate::Expr({:?})", h),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_visible_returns_true() {
        let p = Predicate::always_visible();
        let ctx = PredicateContext::default();
        assert!(p.evaluate(&ctx));
    }

    #[test]
    fn closure_predicate_observes_context() {
        let p = Predicate::from_fn(|ctx| ctx.has_selection);
        let mut ctx = PredicateContext::default();
        assert!(!p.evaluate(&ctx));
        ctx.has_selection = true;
        assert!(p.evaluate(&ctx));
    }

    #[test]
    fn closure_can_match_play_state() {
        let p = Predicate::from_fn(|ctx| ctx.play_state == "playing");
        let mut ctx = PredicateContext::default();
        assert!(!p.evaluate(&ctx));
        ctx.play_state = "playing".into();
        assert!(p.evaluate(&ctx));
    }

    #[test]
    fn expr_predicate_short_circuits_to_true_pre_w19() {
        let p = Predicate::from_expr("selection.is_some()");
        assert!(p.evaluate(&PredicateContext::default()));
        // The handle round-trips the source.
        if let Predicate::Expr(h) = p {
            assert_eq!(h.source, "selection.is_some()");
        } else {
            panic!("Expr variant expected");
        }
    }

    #[test]
    fn predicate_clone_does_not_double_invoke() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = Arc::new(AtomicUsize::new(0));
        let counter2 = counter.clone();
        let p = Predicate::from_fn(move |_| {
            counter2.fetch_add(1, Ordering::SeqCst);
            true
        });
        let p2 = p.clone();
        let _ = p.evaluate(&PredicateContext::default());
        let _ = p2.evaluate(&PredicateContext::default());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn enablement_booleans_default_false_and_drive_closures() {
        let ctx = PredicateContext::default();
        assert!(!ctx.can_play && !ctx.can_pause && !ctx.can_stop && !ctx.can_step);
        assert!(!ctx.is_editing);
        assert!(!ctx.has_frameable_scene);

        let p = Predicate::from_fn(|c| c.can_play);
        assert!(!p.evaluate(&ctx), "can_play defaults false");
        let mut editing = PredicateContext::default();
        editing.can_play = true;
        editing.is_editing = true;
        assert!(p.evaluate(&editing), "closure observes the can_play flag");
    }
}
