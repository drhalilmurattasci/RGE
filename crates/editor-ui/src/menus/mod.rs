//! `editor_ui::menus` — UE5 `UToolMenus`-inspired data-driven menu/toolbar registry.
//!
//! adapted from rustforge::apps::editor-app::egui_overlay (menu bar) on 2026-05-05
//! — rebuilt as data-driven `MenuRegistry`.
//!
//! The rustforge editor wired its menu bar imperatively, calling
//! [`render_menu_bar`](https://example.invalid) inside `egui_overlay`
//! against a `Vec<MenuDefinition>` it constructed each frame. That works
//! for a single host but cannot be extended by plugins without forking
//! the host. This module rebuilds the surface as a registry: callers
//! [`declare_extension_point`](MenuRegistry::declare_extension_point) a
//! named slot, then [`register_entry`](MenuRegistry::register_entry)
//! against it. Order across registrations is resolved deterministically
//! via [`OrderHint::Before`] / [`OrderHint::After`] / [`OrderHint::InSection`]
//! / [`OrderHint::AtStart`] / [`OrderHint::AtEnd`].
//!
//! The shape follows UE5 `UToolMenus` (extension hooks → entries
//! anchored by id) with the conflict-detection and predicate model from
//! the v0.8 plan §6.3. Predicates may be a Rust closure (always
//! available) or an `expr-wasm` expression handle (W19; falls through
//! to a local stub when the crate is not yet merged).
//!
//! ## Layout
//!
//! - [`extension_point`] — typed slot ids and the slot registry.
//! - [`entry`] — `MenuEntry` shape + stable [`EntryId`].
//! - [`order_hint`] — relative-position hints used during resolve.
//! - [`shortcut`] — keyboard accelerators + global conflict table.
//! - [`command`] — the [`Command`] enum (core variants + plugin escape hatch).
//! - [`predicate`] — visibility / enablement tests.
//! - [`registry`] — top-level [`MenuRegistry`] facade.
//! - [`default_menu`] — the editor's canonical File/Edit/Play/View definition.
//!
//! ## Local stubs (see plan: stub if W05/W06/W19 not merged)
//!
//! - [`Style`] — placeholder for `ui-theme::Style`.
//! - [`IconHandle`] — placeholder for `ui-icons::IconHandle`.
//! - [`ExprHandle`] — placeholder for `expr-wasm::ExprHandle`.
//!
//! Each is a thin newtype that round-trips a string identifier so the
//! registry can store entries today and swap in real types when the
//! upstream crates land — replacing the stub in one place lights up
//! every entry that already references the symbol.

pub mod command;
pub mod default_menu;
pub mod entry;
pub mod extension_point;
pub mod order_hint;
pub mod predicate;
pub mod registry;
pub mod shortcut;

pub use command::Command;
pub use default_menu::{
    default_editor_menu, edit_menu_point, file_menu_point, play_menu_point, view_menu_point,
};
pub use entry::{EntryId, LabelOverride, MenuEntry, Section};
pub use extension_point::ExtensionPoint;
pub use order_hint::OrderHint;
pub use predicate::{Predicate, PredicateContext};
pub use registry::{MenuRegistry, RegistryError, ResolvedEntry};
pub use shortcut::{AcceleratorTable, Key, Modifiers, Shortcut, ShortcutConflict};

// ---------------------------------------------------------------------------
// Local stubs for upstream crates that have not yet merged. Each is a
// data-only newtype: enough to construct a [`MenuEntry`] today and a
// drop-in replacement target once W05/W06/W19 land.
// ---------------------------------------------------------------------------

/// Local stub of `ui-theme::Style` (W05 not yet merged).
///
/// Carries a stable token id (e.g. `"editor.menubar.fg"`); the real
/// crate adds resolution against a token registry. The stub stores the
/// id verbatim so call sites that already reference [`Style`] become
/// the real type by `pub use rge_ui_theme::Style;` at the top of this
/// module once the upstream crate lands.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Style {
    /// Token identifier such as `"editor.menubar.entry.label"`.
    pub token: String,
}

impl Style {
    /// Construct from a token id. `Style::new("foo")` is the canonical
    /// constructor — the field is also `pub` so consumers can pattern-
    /// match without going through the helper.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

/// Local stub of `ui-icons::IconHandle` (W06 not yet merged).
///
/// A handle into the icon catalogue; the real crate resolves to an
/// atlas slot. Today this stores a string id (e.g. `"file.open"`)
/// matching the `UiIcon` enum used in the rustforge prior art. Public
/// fields are intentional — the stub is a passive carrier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct IconHandle {
    /// Stable icon id; `None` means "no icon, fall back to label only".
    pub id: Option<String>,
}

impl IconHandle {
    /// Build a handle that points at a named icon.
    #[must_use]
    pub fn named(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
        }
    }

    /// The "no icon" handle — use when an entry should render label-only.
    #[must_use]
    pub const fn none() -> Self {
        Self { id: None }
    }
}

/// Local stub of `expr-wasm::ExprHandle` (W19 not yet merged).
///
/// A precompiled inline expression; the real crate compiles a single
/// expression to WASM and yields a handle that resolves against a
/// reflection schema. Today the stub stores the source expression
/// verbatim so [`Predicate::Expr`] can round-trip through tests
/// without taking a dependency on the W19 compiler.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprHandle {
    /// Source expression (`"selection.is_some()"`, etc.).
    pub source: String,
}

impl ExprHandle {
    /// Construct from a raw expression string. Validation is deferred
    /// until the real expr-wasm compiler is wired in via W19; the stub
    /// performs no parsing.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }
}
