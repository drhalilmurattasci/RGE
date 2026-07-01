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

    /// Return the last override for `target`, if present.
    pub fn last_for_target(&self, target: &KeybindingTarget) -> Option<&KeybindingOverride> {
        self.overrides
            .iter()
            .rev()
            .find(|keybinding_override| keybinding_override.target.eq(target))
    }

    /// Iterate the effective last-wins overrides in winning insertion order.
    pub fn effective_overrides(&self) -> impl Iterator<Item = &KeybindingOverride> {
        self.overrides
            .iter()
            .enumerate()
            .filter_map(move |(index, keybinding_override)| {
                let has_later_override = self.overrides[index + 1..]
                    .iter()
                    .any(|later| later.target == keybinding_override.target);
                (!has_later_override).then_some(keybinding_override)
            })
    }

    /// Iterate distinct effective targets in the same order as effective overrides.
    pub fn targets(&self) -> impl Iterator<Item = &KeybindingTarget> {
        self.effective_overrides()
            .map(|keybinding_override| &keybinding_override.target)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::{Key, Modifiers};

    fn target(entry_id: &str) -> KeybindingTarget {
        KeybindingTarget::new("test.menu", entry_id)
    }

    fn shortcut(key: char) -> Shortcut {
        Shortcut::new(Modifiers::CTRL, Key::Char(key))
    }

    #[test]
    fn keybinding_overrides_effective_helpers_are_last_wins_in_winning_order() {
        let target_a = target("a");
        let target_b = target("b");
        let absent = target("absent");
        let a1 = KeybindingOverride::remap(target_a.clone(), shortcut('A'));
        let b1 = KeybindingOverride::remap(target_b.clone(), shortcut('B'));
        let a2 = KeybindingOverride::remap(target_a.clone(), shortcut('C'));
        let overrides = KeybindingOverrides::from_overrides([a1.clone(), b1.clone(), a2.clone()]);

        assert_eq!(
            overrides.iter().cloned().collect::<Vec<_>>(),
            vec![a1, b1.clone(), a2.clone()]
        );
        assert_eq!(
            overrides.effective_overrides().cloned().collect::<Vec<_>>(),
            vec![b1.clone(), a2.clone()]
        );
        assert_eq!(
            overrides.targets().cloned().collect::<Vec<_>>(),
            vec![target_b, target_a]
        );
        assert!(overrides.last_for_target(&absent).is_none());

        let raw = overrides.iter().collect::<Vec<_>>();
        let last_a = overrides
            .last_for_target(&a2.target)
            .expect("target a has a final override");
        assert!(std::ptr::eq(last_a, raw[2]));
    }

    #[test]
    fn keybinding_overrides_unbind_and_remap_last_wins() {
        let target_a = target("a");
        let remap = KeybindingOverride::remap(target_a.clone(), shortcut('R'));
        let unbind = KeybindingOverride::unbind(target_a.clone());

        let remap_then_unbind =
            KeybindingOverrides::from_overrides([remap.clone(), unbind.clone()]);
        assert_eq!(
            remap_then_unbind.effective_overrides().collect::<Vec<_>>(),
            vec![&unbind]
        );
        assert_eq!(
            remap_then_unbind
                .last_for_target(&target_a)
                .expect("target a remains effective")
                .shortcut
                .as_ref(),
            None
        );

        let unbind_then_remap = KeybindingOverrides::from_overrides([unbind, remap.clone()]);
        assert_eq!(
            unbind_then_remap.effective_overrides().collect::<Vec<_>>(),
            vec![&remap]
        );
        assert_eq!(
            unbind_then_remap
                .last_for_target(&target_a)
                .expect("target a remains effective")
                .shortcut
                .as_ref(),
            Some(&shortcut('R'))
        );
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
    /// A remap override matched a resolved visible entry whose executable
    /// shortcut was already the requested shortcut.
    NoOpRemap {
        /// The resolved target supplied by the override collection.
        target: KeybindingTarget,
        /// The shortcut that was already bound to the target.
        shortcut: Shortcut,
    },
    /// An unbind override matched a resolved visible entry that already had no
    /// executable shortcut.
    RedundantUnbind {
        /// The resolved target supplied by the override collection.
        target: KeybindingTarget,
    },
}
