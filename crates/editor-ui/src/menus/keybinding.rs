//! In-memory keybinding overrides for a single menu resolve.
//!
//! This module is intentionally data-only. It does not read settings,
//! persist profiles, talk to the host, or mutate the menu registry; callers pass
//! a collection to `MenuRegistry::resolve_with_keybinding_overrides` for the one
//! resolve that should see the remap or unbind.

use crate::menus::{EntryId, ExtensionPoint, Shortcut};

/// A menu entry targeted by a keybinding override.
///
/// Entry ids are unique only inside one extension point, so the target carries
/// both ids instead of using a command, shell route, or host-side action id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeybindingTarget {
    /// The extension point that owns the entry.
    pub extension_point: ExtensionPoint,
    /// The entry id inside [`Self::extension_point`].
    pub entry_id: EntryId,
}

impl KeybindingTarget {
    /// Construct a target from an extension point id and an entry id.
    #[must_use]
    pub fn new(extension_point: impl Into<ExtensionPoint>, entry_id: impl Into<EntryId>) -> Self {
        Self {
            extension_point: extension_point.into(),
            entry_id: entry_id.into(),
        }
    }
}

/// One resolve-time keybinding override.
///
/// `Some(shortcut)` remaps the target's executable shortcut for this resolve.
/// `None` unbinds the executable shortcut for this resolve. Neither form touches
/// display-only `shortcut_hint` data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindingOverride {
    /// The menu entry to override.
    pub target: KeybindingTarget,
    /// The replacement executable shortcut, or `None` to unbind it.
    pub shortcut: Option<Shortcut>,
}

impl KeybindingOverride {
    /// Construct an override directly from a target and optional shortcut.
    #[must_use]
    pub fn new(target: KeybindingTarget, shortcut: Option<Shortcut>) -> Self {
        Self { target, shortcut }
    }

    /// Remap `target` to `shortcut` for one resolve.
    #[must_use]
    pub fn remap(target: KeybindingTarget, shortcut: Shortcut) -> Self {
        Self::new(target, Some(shortcut))
    }

    /// Clear `target`'s executable shortcut for one resolve.
    #[must_use]
    pub fn unbind(target: KeybindingTarget) -> Self {
        Self::new(target, None)
    }
}

/// In-memory keybinding overrides supplied to one menu resolve.
///
/// The collection preserves insertion order so diagnostics are deterministic.
/// If a caller pushes multiple overrides for the same target, later overrides
/// win because they are applied in insertion order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeybindingOverrides {
    overrides: Vec<KeybindingOverride>,
}

impl KeybindingOverrides {
    /// Construct an empty override collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a collection from an iterator of overrides.
    #[must_use]
    pub fn from_overrides(overrides: impl IntoIterator<Item = KeybindingOverride>) -> Self {
        Self {
            overrides: overrides.into_iter().collect(),
        }
    }

    /// Append one override.
    pub fn push(&mut self, keybinding_override: KeybindingOverride) {
        self.overrides.push(keybinding_override);
    }

    /// Append a remap override and return the collection for chaining.
    pub fn remap(&mut self, target: KeybindingTarget, shortcut: Shortcut) -> &mut Self {
        self.push(KeybindingOverride::remap(target, shortcut));
        self
    }

    /// Append an unbind override and return the collection for chaining.
    pub fn unbind(&mut self, target: KeybindingTarget) -> &mut Self {
        self.push(KeybindingOverride::unbind(target));
        self
    }

    /// Iterate overrides in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &KeybindingOverride> {
        self.overrides.iter()
    }

    /// Number of overrides in the collection.
    #[must_use]
    pub fn len(&self) -> usize {
        self.overrides.len()
    }

    /// `true` when no overrides are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }
}

impl From<Vec<KeybindingOverride>> for KeybindingOverrides {
    fn from(overrides: Vec<KeybindingOverride>) -> Self {
        Self { overrides }
    }
}

/// Resolve-time diagnostics produced by keybinding overrides.
///
/// Diagnostics are data: an unknown target never panics and never becomes a
/// registry error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeybindingDiagnostic {
    /// An override targeted an extension point / entry id pair that is not
    /// registered in the source registry before visibility filtering.
    UnknownTarget {
        /// The unresolved target supplied by the override collection.
        target: KeybindingTarget,
    },
}
