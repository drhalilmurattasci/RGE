//! `MenuRegistry` — declare extension points, register entries, resolve
//! ordering, build trees.
//!
//! adapted from rustforge::apps::editor-app::egui_overlay (menu bar) on 2026-05-05
//! — rebuilt as data-driven `MenuRegistry`.
//!
//! ## Lifecycle
//!
//! 1. Host calls [`MenuRegistry::declare_extension_point`] for every
//!    surface it wants plugins to extend (`"editor.main_menu.file"`,
//!    `"editor.toolbar.play_mode"`, ...).
//! 2. Host and plugins call [`MenuRegistry::register_entry`] against
//!    one of those points. The registry stores the entries verbatim
//!    in registration order.
//! 3. The frame rendering layer (see editor-shell, post-merge) calls
//!    [`MenuRegistry::resolve`] once per resolve tick. Resolve is pure
//!    — it walks the registered entries, applies sectioning + ordering
//!    hints + predicates, and returns a deterministic
//!    [`Vec<ResolvedEntry>`] per extension point plus a single
//!    [`AcceleratorTable`] across all surfaces. Conflicts surface as
//!    diagnostics, not errors.
//!
//! ## Resolve algorithm
//!
//! For each extension point:
//!
//! 1. Filter out entries with `visible == false` and entries whose
//!    predicate evaluates to `false`.
//! 2. Apply [`OrderHint::InSection`] to entries that opted into a
//!    section after construction (rewrite their `section` field).
//! 3. Bucket entries by section, preserving the order in which the
//!    section first appeared in registration (so the default section
//!    is first if anything registered without a section before any
//!    named section was used; otherwise the first named section's
//!    appearance index wins).
//! 4. Inside each bucket, place [`OrderHint::AtStart`] first (in
//!    registration order), [`OrderHint::AtEnd`] last (also in
//!    registration order), and resolve [`OrderHint::Before`] /
//!    [`OrderHint::After`] iteratively until no entry moves. An entry
//!    whose Before/After target is missing falls through to AtEnd.
//! 5. Concatenate buckets; the result is the [`Vec<ResolvedEntry>`]
//!    for the extension point.
//!
//! The algorithm is O(n²) per extension point in the worst case (the
//! Before/After fixed-point loop) which is fine for the menu surfaces
//! the editor exposes — even a heavily extended File menu has <50
//! entries. If profiling proves otherwise the registry can swap to a
//! topological sort without breaking the public surface.

use std::collections::{HashMap, HashSet};

use crate::menus::{
    AcceleratorTable, Command, EntryId, ExtensionPoint, MenuEntry, OrderHint, PredicateContext,
    Section, Shortcut, ShortcutConflict,
};

/// Errors emitted by the registry. Resolve-time diagnostics
/// (predicate-suppressed entries, missing Before/After targets,
/// shortcut conflicts) are returned as data, not errors — so this
/// enum stays small.
///
/// `Display` + `std::error::Error` impls are hand-rolled (further
/// down in this file). Adopting `thiserror` is deferred until the
/// editor-ui Cargo.toml is touched by another wave; this module is
/// scoped to `menus/` only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// Tried to register against an extension point that was never
    /// declared. The fix is for the host to call
    /// [`MenuRegistry::declare_extension_point`] first.
    UnknownExtensionPoint(String),
    /// Two entries inside the same extension point share an id. The
    /// later registration is rejected. Tuple is `(extension_point_id,
    /// entry_id)`.
    DuplicateEntryId(String, String),
    /// The same extension point id was declared twice.
    DuplicateExtensionPoint(String),
}

/// One entry in the resolved tree. Carries the original [`MenuEntry`]
/// plus its computed depth-1 position. The frontend walks `Vec<ResolvedEntry>`
/// paint-only — no further sorting.
#[derive(Debug, Clone)]
pub struct ResolvedEntry {
    /// The registered entry. Cloned so `resolve` does not borrow the
    /// registry past the call.
    pub entry: MenuEntry,
    /// The section bucket this entry resolved into (after applying
    /// [`OrderHint::InSection`] overrides).
    pub section: Section,
    /// Whether the entry's ENABLEMENT predicate ([`MenuEntry::enabled`])
    /// evaluated `true` against the resolve context. A disabled entry is still
    /// PRESENT (the host renders it greyed) and keeps its accelerator; only the
    /// VISIBILITY predicate / `visible` flag removes an entry. `true` for entries
    /// with no enablement predicate.
    pub enabled: bool,
}

#[derive(Debug, Default)]
struct ExtensionPointSlot {
    /// Entries in registration order. The resolver does not mutate
    /// this — it builds a fresh `Vec<ResolvedEntry>` each call so
    /// re-resolving with a different predicate context is safe.
    entries: Vec<MenuEntry>,
}

/// The top-level registry. Holds the declared extension points and
/// every registered entry. Single instance per editor session;
/// plugins receive a `&mut MenuRegistry` during their `register`
/// hook.
#[derive(Debug, Default)]
pub struct MenuRegistry {
    points: HashMap<ExtensionPoint, ExtensionPointSlot>,
    /// Insertion order of declared extension points. Stable iteration
    /// for tests and golden snapshots.
    point_order: Vec<ExtensionPoint>,
}

impl MenuRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a new extension point. Subsequent calls to
    /// [`Self::register_entry`] against the same point succeed; calls
    /// against any other point return [`RegistryError::UnknownExtensionPoint`].
    ///
    /// Re-declaring a point is a [`RegistryError::DuplicateExtensionPoint`].
    /// Hosts should declare every point at startup before any plugin
    /// registers.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::DuplicateExtensionPoint`] if the same
    /// id is declared twice.
    pub fn declare_extension_point(&mut self, point: ExtensionPoint) -> Result<(), RegistryError> {
        if self.points.contains_key(&point) {
            return Err(RegistryError::DuplicateExtensionPoint(point.into_inner()));
        }
        self.point_order.push(point.clone());
        self.points.insert(point, ExtensionPointSlot::default());
        Ok(())
    }

    /// Register an entry against an extension point. The registry
    /// stores the entry verbatim — ordering is computed lazily during
    /// [`Self::resolve`].
    ///
    /// # Errors
    ///
    /// - [`RegistryError::UnknownExtensionPoint`] if `point` was never
    ///   declared.
    /// - [`RegistryError::DuplicateEntryId`] if an entry with the same
    ///   id is already registered against `point`.
    pub fn register_entry(
        &mut self,
        point: &ExtensionPoint,
        entry: MenuEntry,
    ) -> Result<(), RegistryError> {
        let slot = self
            .points
            .get_mut(point)
            .ok_or_else(|| RegistryError::UnknownExtensionPoint(point.as_str().to_owned()))?;
        if slot.entries.iter().any(|e| e.id == entry.id) {
            return Err(RegistryError::DuplicateEntryId(
                point.as_str().to_owned(),
                entry.id.as_str().to_owned(),
            ));
        }
        slot.entries.push(entry);
        Ok(())
    }

    /// `true` when `point` has been declared.
    #[must_use]
    pub fn has_extension_point(&self, point: &ExtensionPoint) -> bool {
        self.points.contains_key(point)
    }

    /// Iterate over declared extension points in declaration order.
    pub fn extension_points(&self) -> impl Iterator<Item = &ExtensionPoint> {
        self.point_order.iter()
    }

    /// How many entries are registered against `point`. Returns `None`
    /// when the point is not declared.
    #[must_use]
    pub fn entry_count(&self, point: &ExtensionPoint) -> Option<usize> {
        self.points.get(point).map(|s| s.entries.len())
    }

    /// Resolve every extension point against the given predicate
    /// context. Returns a per-point ordered entry list plus a global
    /// [`AcceleratorTable`] and the list of detected
    /// [`ShortcutConflict`]s. Pure — does not mutate the registry.
    ///
    /// Entries with `visible == false` or with a failing predicate are
    /// dropped; their shortcuts are also excluded from the
    /// accelerator table (so a hidden entry never "wins" a keystroke
    /// that a visible entry could otherwise claim).
    #[must_use]
    pub fn resolve(&self, ctx: &PredicateContext) -> ResolveResult {
        let mut by_point: HashMap<ExtensionPoint, Vec<ResolvedEntry>> = HashMap::new();
        let mut accel = AcceleratorTable::new();
        let mut shortcut_commands: HashMap<Shortcut, (Command, bool)> = HashMap::new();

        for point in &self.point_order {
            let slot = self.points.get(point).expect(
                "point_order and points map must stay in sync — \
                 only declare_extension_point mutates either",
            );
            let resolved = resolve_slot(&slot.entries, ctx);
            for r in &resolved {
                if let Some(s) = &r.entry.shortcut {
                    accel.register(s.clone(), r.entry.id.clone());
                    // First registration wins a shortcut — iterated in the same
                    // order (point declaration × registration) the accelerator
                    // table uses, so `command_for_shortcut` agrees with the
                    // table's winner WITHOUT re-scanning entry ids (which are
                    // unique only within a point, never globally).
                    shortcut_commands
                        .entry(s.clone())
                        .or_insert_with(|| (r.entry.command.clone(), r.enabled));
                }
            }
            by_point.insert(point.clone(), resolved);
        }
        let conflicts = accel.detect_conflicts();
        let conflicted_shortcuts = conflicts
            .iter()
            .map(|conflict| conflict.shortcut.clone())
            .collect();
        ResolveResult {
            by_point,
            accelerator_table: accel,
            conflicts,
            shortcut_commands,
            conflicted_shortcuts,
        }
    }
}

/// Output of [`MenuRegistry::resolve`].
///
/// Carries per-extension-point resolved trees plus the global
/// [`AcceleratorTable`] and any detected shortcut conflicts. Hosts
/// surface conflicts as diagnostics; display/introspection lookups
/// preserve the first-registered winner, while the enabled-command
/// execution lookup suppresses conflicted shortcuts.
#[derive(Debug, Clone)]
pub struct ResolveResult {
    /// Per-extension-point ordered entries. Lookup by extension point
    /// id; missing points yield an empty list (the host can render an
    /// empty surface).
    pub by_point: HashMap<ExtensionPoint, Vec<ResolvedEntry>>,
    /// Global accelerator table. O(1) shortcut → entry id lookup.
    pub accelerator_table: AcceleratorTable,
    /// Every conflict detected during this resolve. Empty when no
    /// shortcut is bound twice.
    pub conflicts: Vec<ShortcutConflict>,
    /// O(1) shortcut → (winning [`Command`], enabled) index, built in resolve
    /// order (point declaration × registration; first registration wins). The
    /// `bool` is the winning entry's resolved [`ResolvedEntry::enabled`]. Backs
    /// [`Self::command_for_shortcut`] (binding, ignores the bool) and
    /// [`Self::enabled_command_for_shortcut`] (also checks enablement and
    /// conflict state) so neither re-scans entry ids — which are unique only
    /// WITHIN an extension point, not globally, so a shortcut → bare-id → entry
    /// scan could otherwise match a different point's same-id entry. Private:
    /// the public accessors are [`Self::command_for_shortcut`] /
    /// [`Self::enabled_command_for_shortcut`].
    shortcut_commands: HashMap<Shortcut, (Command, bool)>,
    /// O(1) shortcut membership for live conflicts. Kept private so the public
    /// diagnostic surface remains [`Self::conflicts`] in deterministic display
    /// order.
    conflicted_shortcuts: HashSet<Shortcut>,
}

impl ResolveResult {
    /// Borrow the resolved entry list for a single extension point.
    /// Returns an empty slice when the point was never registered.
    #[must_use]
    pub fn entries_for<'a>(&'a self, point: &ExtensionPoint) -> &'a [ResolvedEntry] {
        self.by_point.get(point).map_or(&[], Vec::as_slice)
    }

    /// Resolve a keystroke to the [`Command`] its bound entry dispatches, if any.
    ///
    /// O(1) lookup into the resolved shortcut → command index. The "winner" for a
    /// conflicted shortcut is the first-registered entry, matching
    /// [`AcceleratorTable::resolve`]. Returns `None` when no visible entry binds
    /// `shortcut` (a hidden / predicate-suppressed entry never claims a keystroke,
    /// since `resolve` excludes it). Keyed by the full [`Shortcut`], so it is
    /// correct even when entry ids repeat across extension points — ids are unique
    /// only within a point, so resolving via the bare-id accelerator table and
    /// scanning the resolved entries could otherwise return a different point's
    /// same-id command.
    ///
    /// This is a binding/display lookup. Keyboard execution uses
    /// [`Self::enabled_command_for_shortcut`] so disabled and conflicted
    /// shortcuts do not fire.
    #[must_use]
    pub fn command_for_shortcut(&self, shortcut: &Shortcut) -> Option<&Command> {
        self.shortcut_commands.get(shortcut).map(|(cmd, _)| cmd)
    }

    /// Resolve a keystroke to its bound [`Command`] ONLY IF the bound entry is
    /// currently ENABLED (its [`ResolvedEntry::enabled`] is `true` for this
    /// resolve context) AND the shortcut has no live conflict. Returns `None`
    /// for an unbound keystroke, a bound-but-disabled one, or any shortcut
    /// present in [`Self::conflicts`].
    ///
    /// This is the resolver the keyboard EXECUTION path uses, so a disabled
    /// accelerator (e.g. `Ctrl+S` while a play session is active and Save is
    /// greyed) or a conflicted accelerator does not fire.
    /// [`Self::command_for_shortcut`] is the binding/display lookup and ignores
    /// enablement and conflict state.
    #[must_use]
    pub fn enabled_command_for_shortcut(&self, shortcut: &Shortcut) -> Option<&Command> {
        if self.conflicted_shortcuts.contains(shortcut) {
            return None;
        }
        self.shortcut_commands
            .get(shortcut)
            .and_then(|(cmd, enabled)| (*enabled).then_some(cmd))
    }
}

// ---------------------------------------------------------------------------
// Resolve helpers (private). Spelled out as free functions so the
// algorithm reads top-down.
// ---------------------------------------------------------------------------

/// Apply visibility / predicate filter, then section + order
/// resolution, returning the ordered list for one extension point.
fn resolve_slot(entries: &[MenuEntry], ctx: &PredicateContext) -> Vec<ResolvedEntry> {
    // Step 1: filter visibility + predicate.
    let visible: Vec<MenuEntry> = entries
        .iter()
        .filter(|e| e.visible && e.predicate.evaluate(ctx))
        .cloned()
        .map(|mut e| {
            if let Some(label) = e
                .label_override
                .as_ref()
                .and_then(|label_override| label_override.evaluate(ctx))
            {
                e.label = label;
            }
            e
        })
        .collect();

    // Step 2: apply InSection overrides — a hint of OrderHint::InSection(name)
    // moves the entry into the named section and degrades to AtEnd inside it.
    let with_sections: Vec<MenuEntry> = visible
        .into_iter()
        .map(|mut e| {
            if let OrderHint::InSection(name) = &e.order_hint {
                e.section = Section::new(name.clone());
                e.order_hint = OrderHint::AtEnd;
            }
            e
        })
        .collect();

    // Step 3: bucket by section, preserving first-seen section order.
    let mut section_order: Vec<Section> = Vec::new();
    let mut buckets: HashMap<Section, Vec<MenuEntry>> = HashMap::new();
    for e in with_sections {
        let section = e.section.clone();
        if !buckets.contains_key(&section) {
            section_order.push(section.clone());
        }
        buckets.entry(section).or_default().push(e);
    }

    // Step 4: order within each bucket, then concatenate.
    let mut out: Vec<ResolvedEntry> = Vec::new();
    for section in &section_order {
        let bucket = buckets.remove(section).unwrap_or_default();
        let ordered = order_bucket(bucket);
        for e in ordered {
            // Enablement is evaluated HERE, not in Step 1's visibility filter: a
            // disabled entry stays present + keeps its accelerator (the host
            // greys it). Only the visibility predicate / `visible` flag (Step 1)
            // removes an entry.
            let enabled = e.enabled.evaluate(ctx);
            out.push(ResolvedEntry {
                entry: e,
                section: section.clone(),
                enabled,
            });
        }
    }
    out
}

/// Order a single section's entries.
///
/// `AtStart` and `AtEnd` form the spine; `Before(id)` / `After(id)`
/// resolve iteratively against the current order; missing targets
/// degrade to `AtEnd`.
fn order_bucket(bucket: Vec<MenuEntry>) -> Vec<MenuEntry> {
    // Partition by hint kind.
    let mut at_start: Vec<MenuEntry> = Vec::new();
    let mut at_end: Vec<MenuEntry> = Vec::new();
    let mut before: Vec<MenuEntry> = Vec::new();
    let mut after: Vec<MenuEntry> = Vec::new();
    for e in bucket {
        match &e.order_hint {
            OrderHint::AtStart => at_start.push(e),
            OrderHint::AtEnd => at_end.push(e),
            OrderHint::Before(_) => before.push(e),
            OrderHint::After(_) => after.push(e),
            // InSection was rewritten to AtEnd in resolve_slot already;
            // reaching this arm is a logic bug.
            OrderHint::InSection(_) => at_end.push(e),
        }
    }

    // Spine: AtStart ++ AtEnd, registration order preserved within
    // each.
    let mut order: Vec<MenuEntry> = at_start;
    order.extend(at_end);

    // Insert Before/After until no movement. Each pass: if the target
    // exists in `order`, splice the dependent entry next to it; else
    // keep it for next pass. After a fixed number of passes (entries
    // remaining after a no-progress pass) any residue degrades to
    // AtEnd — covers cycles and missing targets.
    let mut pending_before = before;
    let mut pending_after = after;
    loop {
        let mut progress = false;

        // Before: insert directly before the target.
        let (inserted_before, residue_before) =
            try_insert(pending_before, &mut order, |order, entry, target| {
                if let Some(idx) = order.iter().position(|e| {
                    if let OrderHint::Before(t) = &entry.order_hint {
                        e.id == *t
                    } else {
                        false
                    }
                }) {
                    let _ = target;
                    order.insert(idx, entry);
                    Some(())
                } else {
                    None
                }
            });
        progress |= inserted_before;
        pending_before = residue_before;

        // After: insert directly after the target.
        let (inserted_after, residue_after) =
            try_insert(pending_after, &mut order, |order, entry, target| {
                if let Some(idx) = order.iter().position(|e| {
                    if let OrderHint::After(t) = &entry.order_hint {
                        e.id == *t
                    } else {
                        false
                    }
                }) {
                    let _ = target;
                    order.insert(idx + 1, entry);
                    Some(())
                } else {
                    None
                }
            });
        progress |= inserted_after;
        pending_after = residue_after;

        if !progress {
            // No further progress — degrade remaining Before/After
            // entries to AtEnd (target missing or part of a cycle).
            for e in pending_before.into_iter().chain(pending_after.into_iter()) {
                order.push(e);
            }
            break;
        }
    }

    order
}

/// Try to splice each pending entry into `order`. Returns `(progressed,
/// residue)` — `progressed` is `true` when at least one entry landed.
fn try_insert<F>(
    pending: Vec<MenuEntry>,
    order: &mut Vec<MenuEntry>,
    mut splice: F,
) -> (bool, Vec<MenuEntry>)
where
    F: FnMut(&mut Vec<MenuEntry>, MenuEntry, &EntryId) -> Option<()>,
{
    let mut residue: Vec<MenuEntry> = Vec::new();
    let mut progress = false;
    for e in pending {
        let target = match &e.order_hint {
            OrderHint::Before(t) | OrderHint::After(t) => t.clone(),
            // Defensive: unreachable in practice; route to residue.
            _ => {
                residue.push(e);
                continue;
            }
        };
        if splice(order, e.clone(), &target).is_some() {
            progress = true;
        } else {
            residue.push(e);
        }
    }
    (progress, residue)
}

// ---------------------------------------------------------------------------
// Hand-rolled Display + std::error::Error impls for RegistryError.
// W08 is scoped to the menus submodule and avoids touching
// crates/editor-ui/Cargo.toml; once a later wave adds the workspace
// `thiserror` dep, the impls here can be replaced with `#[derive]`s.
// ---------------------------------------------------------------------------

impl core::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownExtensionPoint(id) => {
                write!(f, "extension point {id:?} not declared")
            }
            Self::DuplicateEntryId(point, id) => {
                write!(
                    f,
                    "entry id {id:?} already registered in extension point {point:?}",
                )
            }
            Self::DuplicateExtensionPoint(id) => {
                write!(f, "extension point {id:?} already declared")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::{Command, Key, Modifiers, Predicate, Shortcut};

    fn entry(id: &str, hint: OrderHint) -> MenuEntry {
        MenuEntry::new(id, id, Command::Custom(id.into())).with_order_hint(hint)
    }

    #[test]
    fn declare_then_register_works() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("editor.main_menu.file");
        r.declare_extension_point(p.clone()).unwrap();
        r.register_entry(&p, entry("file.open", OrderHint::AtEnd))
            .unwrap();
        assert_eq!(r.entry_count(&p), Some(1));
    }

    #[test]
    fn register_against_unknown_point_errors() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("editor.unknown");
        let err = r
            .register_entry(&p, entry("x", OrderHint::AtEnd))
            .unwrap_err();
        assert!(matches!(err, RegistryError::UnknownExtensionPoint(_)));
    }

    #[test]
    fn duplicate_extension_point_errors() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("a");
        r.declare_extension_point(p.clone()).unwrap();
        let err = r.declare_extension_point(p).unwrap_err();
        assert!(matches!(err, RegistryError::DuplicateExtensionPoint(_)));
    }

    #[test]
    fn duplicate_entry_id_errors() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("a");
        r.declare_extension_point(p.clone()).unwrap();
        r.register_entry(&p, entry("dup", OrderHint::AtEnd))
            .unwrap();
        let err = r
            .register_entry(&p, entry("dup", OrderHint::AtEnd))
            .unwrap_err();
        assert!(matches!(err, RegistryError::DuplicateEntryId(_, _)));
    }

    #[test]
    fn resolve_keeps_at_start_before_at_end() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("a");
        r.declare_extension_point(p.clone()).unwrap();
        r.register_entry(&p, entry("end1", OrderHint::AtEnd))
            .unwrap();
        r.register_entry(&p, entry("start1", OrderHint::AtStart))
            .unwrap();
        r.register_entry(&p, entry("end2", OrderHint::AtEnd))
            .unwrap();
        r.register_entry(&p, entry("start2", OrderHint::AtStart))
            .unwrap();
        let res = r.resolve(&PredicateContext::default());
        let ids: Vec<&str> = res
            .entries_for(&p)
            .iter()
            .map(|r| r.entry.id.as_str())
            .collect();
        assert_eq!(ids, vec!["start1", "start2", "end1", "end2"]);
    }

    #[test]
    fn predicate_filters_entries() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("a");
        r.declare_extension_point(p.clone()).unwrap();
        r.register_entry(
            &p,
            MenuEntry::new("only_when_selected", "X", Command::Save)
                .with_predicate(Predicate::from_fn(|c| c.has_selection)),
        )
        .unwrap();
        let mut ctx = PredicateContext::default();
        let res = r.resolve(&ctx);
        assert!(res.entries_for(&p).is_empty());
        ctx.has_selection = true;
        let res = r.resolve(&ctx);
        assert_eq!(res.entries_for(&p).len(), 1);
    }

    #[test]
    fn enabled_predicate_keeps_entry_visible_but_marks_disabled() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("a");
        r.declare_extension_point(p.clone()).unwrap();
        let s = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
        r.register_entry(
            &p,
            MenuEntry::new("file.save", "Save", Command::Save)
                .with_shortcut(s.clone())
                .with_enabled(Predicate::from_fn(|c| c.is_editing)),
        )
        .unwrap();

        // Disabled context: the entry STAYS present (unlike a visibility
        // predicate, which would filter it out) and resolves enabled == false.
        // The binding still resolves; the enabled-only resolver does not.
        let res = r.resolve(&PredicateContext::default());
        let entries = res.entries_for(&p);
        assert_eq!(entries.len(), 1, "a disabled entry is NOT filtered out");
        assert!(!entries[0].enabled, "is_editing=false -> disabled");
        assert_eq!(
            res.command_for_shortcut(&s),
            Some(&Command::Save),
            "binding lookup ignores enablement"
        );
        assert_eq!(
            res.enabled_command_for_shortcut(&s),
            None,
            "enabled-only resolver withholds a disabled command"
        );

        // Enabled context: both resolvers return the command.
        let mut ctx = PredicateContext::default();
        ctx.is_editing = true;
        let res = r.resolve(&ctx);
        assert!(res.entries_for(&p)[0].enabled);
        assert_eq!(res.command_for_shortcut(&s), Some(&Command::Save));
        assert_eq!(res.enabled_command_for_shortcut(&s), Some(&Command::Save));
    }

    #[test]
    fn entry_without_enabled_predicate_resolves_enabled() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("a");
        r.declare_extension_point(p.clone()).unwrap();
        r.register_entry(&p, MenuEntry::new("x", "X", Command::Save))
            .unwrap();
        let res = r.resolve(&PredicateContext::default());
        assert!(
            res.entries_for(&p)[0].enabled,
            "no enablement predicate -> always enabled"
        );
    }

    #[test]
    fn label_override_changes_resolved_label_only() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("a");
        r.declare_extension_point(p.clone()).unwrap();
        r.register_entry(
            &p,
            MenuEntry::new("play.start", "Play", Command::PlayStart).with_label_override(
                crate::menus::LabelOverride::from_fn(|ctx| {
                    (ctx.play_state == "paused").then(|| "Resume".to_owned())
                }),
            ),
        )
        .unwrap();

        let mut ctx = PredicateContext::default();
        let editing = r.resolve(&ctx);
        assert_eq!(editing.entries_for(&p)[0].entry.label, "Play");

        ctx.play_state = "paused".to_owned();
        let paused = r.resolve(&ctx);
        assert_eq!(paused.entries_for(&p)[0].entry.label, "Resume");

        let editing_again = r.resolve(&PredicateContext::default());
        assert_eq!(
            editing_again.entries_for(&p)[0].entry.label,
            "Play",
            "resolve clones entries; dynamic labels must not mutate the registry"
        );
    }

    #[test]
    fn shortcut_conflicts_surface_in_resolve() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("a");
        r.declare_extension_point(p.clone()).unwrap();
        let s = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
        r.register_entry(
            &p,
            MenuEntry::new("file.save", "Save", Command::Save).with_shortcut(s.clone()),
        )
        .unwrap();
        r.register_entry(
            &p,
            MenuEntry::new("plugin.foo", "Foo", Command::Custom("foo".into()))
                .with_shortcut(s.clone()),
        )
        .unwrap();
        let res = r.resolve(&PredicateContext::default());
        assert_eq!(res.conflicts.len(), 1);
        assert_eq!(res.conflicts[0].shortcut, s);
        assert!(res.accelerator_table.resolve(&s).is_some());
    }

    #[test]
    fn command_for_shortcut_resolves_winner_and_misses() {
        let mut r = MenuRegistry::new();
        let p = ExtensionPoint::new("a");
        r.declare_extension_point(p.clone()).unwrap();
        let s_save = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
        let s_unbound = Shortcut::new(Modifiers::CTRL, Key::Char('Q'));
        r.register_entry(
            &p,
            MenuEntry::new("file.save", "Save", Command::Save).with_shortcut(s_save.clone()),
        )
        .unwrap();
        // A conflicting second registration on the same keystroke — the winner is
        // the first-registered entry (matches `AcceleratorTable::resolve`).
        r.register_entry(
            &p,
            MenuEntry::new("plugin.alt", "Alt", Command::Custom("alt".into()))
                .with_shortcut(s_save.clone()),
        )
        .unwrap();
        let res = r.resolve(&PredicateContext::default());
        assert_eq!(
            res.command_for_shortcut(&s_save),
            Some(&Command::Save),
            "the first-registered entry wins a conflicted shortcut"
        );
        assert_eq!(
            res.command_for_shortcut(&s_unbound),
            None,
            "an unbound shortcut resolves to no command"
        );
    }

    #[test]
    fn command_for_shortcut_disambiguates_duplicate_ids_across_points() {
        // The same EntryId ("shared") in two different extension points, each
        // bound to a DIFFERENT shortcut + command. Entry ids are unique only
        // WITHIN a point, so resolving via the bare-id accelerator table and
        // scanning the resolved entries could match the wrong point's same-id
        // entry; `command_for_shortcut` must return the command of the entry that
        // actually owns each keystroke.
        let mut r = MenuRegistry::new();
        let p1 = ExtensionPoint::new("p1");
        let p2 = ExtensionPoint::new("p2");
        r.declare_extension_point(p1.clone()).unwrap();
        r.declare_extension_point(p2.clone()).unwrap();
        let s_save = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
        let s_delete = Shortcut::new(Modifiers::CTRL, Key::Char('D'));
        r.register_entry(
            &p1,
            MenuEntry::new("shared", "Save", Command::Save).with_shortcut(s_save.clone()),
        )
        .unwrap();
        r.register_entry(
            &p2,
            MenuEntry::new("shared", "Delete", Command::Delete).with_shortcut(s_delete.clone()),
        )
        .unwrap();
        let res = r.resolve(&PredicateContext::default());
        assert_eq!(
            res.command_for_shortcut(&s_save),
            Some(&Command::Save),
            "Ctrl+S resolves to the p1 'shared' entry's Save, not p2's Delete"
        );
        assert_eq!(
            res.command_for_shortcut(&s_delete),
            Some(&Command::Delete),
            "Ctrl+D resolves to the p2 'shared' entry's Delete, not p1's Save"
        );
    }
}
