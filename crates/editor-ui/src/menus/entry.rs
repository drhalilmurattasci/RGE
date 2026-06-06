//! `MenuEntry` — the data shape that hosts and plugins push through the
//! registry.
//!
//! adapted from rustforge::apps::editor-app::egui_overlay (menu bar) on 2026-05-05
//! — rebuilt as data-driven `MenuRegistry`.
//!
//! The rustforge `MenuDefinition`/`ActionDefinition` pair was rendered
//! directly. Here we capture richer metadata up-front so the resolver
//! can position entries deterministically, then compile a tree the
//! frontend can walk paint-only (mirroring the rustforge contract).

use core::fmt;
use std::sync::Arc;

use crate::menus::{Command, IconHandle, OrderHint, Predicate, PredicateContext, Shortcut, Style};

/// Stable, unique id for a single menu / toolbar entry.
///
/// Ids are dotted strings (`"file.open"`, `"view.zoom_in"`,
/// `"plugin.foo.bar"`); the resolver uses the id verbatim when matching
/// `OrderHint::Before` / `OrderHint::After` references. Two entries
/// sharing an id within the same extension point is a registration
/// error — see `MenuRegistry::register_entry`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntryId(String);

impl EntryId {
    /// Construct from a stable id. Empty ids are a programming error.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        debug_assert!(!id.is_empty(), "EntryId must be non-empty");
        Self(id)
    }

    /// Borrow the inner id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and yield the inner [`String`].
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for EntryId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for EntryId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Optional grouping label inside an extension point. Sections are
/// separated by a divider in the rendered output and `OrderHint::InSection`
/// scopes resolution to a single section. The default ("") section is
/// always present and absorbs any entry that does not opt into a named
/// section.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Section(pub String);

impl Section {
    /// Build a named section. The empty string is the implicit "default"
    /// section — prefer [`Section::default`] for that case.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Borrow the section name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` when this is the implicit default section (empty name).
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&str> for Section {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for Section {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Resolver-time display-label override.
///
/// The static [`MenuEntry::label`] remains the default and the stable fallback.
/// A label override can specialize text for live UI state without changing the
/// entry id or command, for example showing "Resume" for the Play item while PIE
/// is paused.
#[derive(Clone)]
pub enum LabelOverride {
    /// A Rust callback. Returning `None` keeps the static label.
    Closure(Arc<dyn Fn(&PredicateContext) -> Option<String> + Send + Sync>),
}

impl LabelOverride {
    /// Wrap a Rust closure as a resolver-time label override.
    #[must_use]
    pub fn from_fn<F>(f: F) -> Self
    where
        F: Fn(&PredicateContext) -> Option<String> + Send + Sync + 'static,
    {
        Self::Closure(Arc::new(f))
    }

    /// Evaluate the override. `None` means "keep the static label".
    #[must_use]
    pub fn evaluate(&self, ctx: &PredicateContext) -> Option<String> {
        match self {
            Self::Closure(f) => (f)(ctx),
        }
    }
}

impl fmt::Debug for LabelOverride {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closure(_) => f.write_str("LabelOverride::Closure(<fn>)"),
        }
    }
}

/// One menu / toolbar entry.
///
/// Carries all metadata needed by both the resolver (id, section,
/// `order_hint`, `predicate`, `visible`) and the renderer (label, icon,
/// shortcut, passive shortcut hint, command, optional style override). The shape
/// is closed by design — extension is via [`Command::Plugin`] inside the command,
/// not by adding fields.
#[derive(Debug, Clone)]
pub struct MenuEntry {
    /// Stable id. Required.
    pub id: EntryId,
    /// Display label (already localised by the host).
    pub label: String,
    /// Optional resolver-time label override. `None` or an override returning
    /// `None` keeps [`Self::label`].
    pub label_override: Option<LabelOverride>,
    /// Icon handle. [`IconHandle::none`] when label-only.
    pub icon: IconHandle,
    /// Optional keyboard shortcut. The registry tracks an
    /// [`AcceleratorTable`](crate::menus::AcceleratorTable) across all
    /// entries and reports conflicts (see `MenuRegistry::resolve`).
    pub shortcut: Option<Shortcut>,
    /// Optional display-only shortcut hint. Unlike [`Self::shortcut`], this is
    /// not registered in the accelerator table and never participates in
    /// keyboard execution. Use it only for bindings owned by a separate input
    /// path, such as Play's plain-key Space/Escape playback controls.
    pub shortcut_hint: Option<Shortcut>,
    /// What clicking this entry dispatches.
    pub command: Command,
    /// Section bucket inside the parent extension point.
    pub section: Section,
    /// How this entry positions itself relative to its section / siblings.
    pub order_hint: OrderHint,
    /// VISIBILITY predicate. When it evaluates `false` the entry is REMOVED
    /// from the resolved tree (and its accelerator dropped) — like `visible`,
    /// but state-dependent. `Predicate::always_visible()` when not gated. For
    /// "present but greyed", use [`Self::enabled`] instead.
    pub predicate: Predicate,
    /// ENABLEMENT predicate. When it evaluates `false` the entry STAYS in the
    /// resolved tree (and keeps its accelerator) but resolves
    /// [`ResolvedEntry::enabled`](crate::menus::ResolvedEntry) to `false`, so the
    /// host renders it DISABLED (greyed). `Predicate::always_visible()` (= always
    /// enabled) when not gated.
    pub enabled: Predicate,
    /// Static visibility flag (host-controlled, plugin-overridable).
    /// `false` removes the entry from the resolved tree without
    /// evaluating the predicate.
    pub visible: bool,
    /// Optional style token override; `None` falls through to the
    /// theme's default for the slot.
    pub style: Option<Style>,
}

impl MenuEntry {
    /// Minimal constructor — sets sensible defaults for everything but
    /// the id, label, and command. Mirrors the rustforge `MenuBuilder`
    /// "common case" path (action with no shortcut / tooltip).
    #[must_use]
    pub fn new(id: impl Into<EntryId>, label: impl Into<String>, command: Command) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            label_override: None,
            icon: IconHandle::none(),
            shortcut: None,
            shortcut_hint: None,
            command,
            section: Section::default(),
            order_hint: OrderHint::AtEnd,
            predicate: Predicate::always_visible(),
            enabled: Predicate::always_visible(),
            visible: true,
            style: None,
        }
    }

    /// Builder-style: set the icon.
    #[must_use]
    pub fn with_icon(mut self, icon: IconHandle) -> Self {
        self.icon = icon;
        self
    }

    /// Builder-style: set a resolver-time label override.
    #[must_use]
    pub fn with_label_override(mut self, label_override: LabelOverride) -> Self {
        self.label_override = Some(label_override);
        self
    }

    /// Builder-style: set the keyboard shortcut.
    #[must_use]
    pub fn with_shortcut(mut self, shortcut: Shortcut) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    /// Builder-style: set a display-only shortcut hint.
    ///
    /// This does not bind a keyboard accelerator; [`MenuRegistry::resolve`] only
    /// registers [`Self::shortcut`] in the accelerator table.
    #[must_use]
    pub fn with_shortcut_hint(mut self, shortcut_hint: Shortcut) -> Self {
        self.shortcut_hint = Some(shortcut_hint);
        self
    }

    /// Builder-style: place into a named section.
    #[must_use]
    pub fn with_section(mut self, section: impl Into<Section>) -> Self {
        self.section = section.into();
        self
    }

    /// Builder-style: override the order hint.
    #[must_use]
    pub fn with_order_hint(mut self, hint: OrderHint) -> Self {
        self.order_hint = hint;
        self
    }

    /// Builder-style: install a VISIBILITY predicate (false REMOVES the entry).
    #[must_use]
    pub fn with_predicate(mut self, predicate: Predicate) -> Self {
        self.predicate = predicate;
        self
    }

    /// Builder-style: install an ENABLEMENT predicate. Unlike
    /// [`Self::with_predicate`] (visibility — a false predicate removes the
    /// entry), a false `enabled` predicate keeps the entry visible but renders
    /// it disabled (greyed); its accelerator stays bound.
    #[must_use]
    pub fn with_enabled(mut self, enabled: Predicate) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builder-style: toggle the static `visible` flag.
    #[must_use]
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Builder-style: override the style token.
    #[must_use]
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::Command;

    #[test]
    fn entry_id_round_trips() {
        let id = EntryId::new("file.open");
        assert_eq!(id.as_str(), "file.open");
        assert_eq!(id.to_string(), "file.open");
    }

    #[test]
    fn section_default_is_empty() {
        let s = Section::default();
        assert!(s.is_default());
        assert_eq!(s.as_str(), "");
    }

    #[test]
    fn new_sets_sensible_defaults() {
        let e = MenuEntry::new("file.open", "Open...", Command::OpenFile);
        assert_eq!(e.id.as_str(), "file.open");
        assert_eq!(e.label, "Open...");
        assert!(e.label_override.is_none());
        assert_eq!(e.icon, IconHandle::none());
        assert!(e.shortcut.is_none());
        assert!(e.shortcut_hint.is_none());
        assert!(e.section.is_default());
        assert_eq!(e.order_hint, OrderHint::AtEnd);
        assert!(e.visible);
        assert!(e.style.is_none());
    }

    #[test]
    fn builder_chain_threads_each_field() {
        let e = MenuEntry::new("x", "X", Command::Custom("x".into()))
            .with_icon(IconHandle::named("x.icon"))
            .with_label_override(LabelOverride::from_fn(|_| Some("Y".to_owned())))
            .with_section("primary")
            .with_order_hint(OrderHint::AtStart)
            .with_visible(false)
            .with_style(Style::new("custom.token"));
        assert_eq!(e.icon, IconHandle::named("x.icon"));
        assert!(e.label_override.is_some());
        assert_eq!(e.section.as_str(), "primary");
        assert_eq!(e.order_hint, OrderHint::AtStart);
        assert!(!e.visible);
        assert_eq!(e.style, Some(Style::new("custom.token")));
    }
}
