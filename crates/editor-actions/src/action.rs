//! [`Action`] trait, context contract, and associated types.

use rge_kernel_ecs::{EntityId, World};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ActionId
// ---------------------------------------------------------------------------

/// Stable identifier for an [`Action`] — used for coalescing target identity.
///
/// For example, `"transform.translate(entity=0x1234)"` — the same id within
/// the 500 ms coalesce window will merge two consecutive actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionId(pub String);

impl ActionId {
    /// Construct an [`ActionId`] from any string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for ActionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// ActionResult
// ---------------------------------------------------------------------------

/// Errors returned by [`Action::apply`] and [`Action::revert`].
#[derive(Debug, thiserror::Error)]
pub enum ActionResult {
    /// The apply step failed with a human-readable message.
    #[error("apply failed: {0}")]
    ApplyFailed(String),
    /// The revert step failed with a human-readable message.
    #[error("revert failed: {0}")]
    RevertFailed(String),
    /// The target entity was not found in the world.
    #[error("entity {0:?} not found")]
    MissingEntity(EntityId),
}

// ---------------------------------------------------------------------------
// ActionContext
// ---------------------------------------------------------------------------

/// Minimal mutation context available to every [`Action`].
///
/// The default implementation is the kernel [`World`] itself, so existing
/// World-only actions can continue to implement `Action` without naming a
/// context type. Richer editor-owned contexts can implement this trait and add
/// their own extension traits outside this crate.
pub trait ActionContext {
    /// Borrow the kernel [`World`] owned by this context.
    fn world(&mut self) -> &mut World;
}

impl ActionContext for World {
    fn world(&mut self) -> &mut World {
        self
    }
}

/// Family of concrete action contexts accepted by a [`CommandBus`](crate::CommandBus).
///
/// The family type is stored on the bus; the borrowed context lifetime is not.
/// This lets an owning crate define a context such as `ShellContext<'a>` while
/// still storing object-safe `Box<dyn Action<MyFamily>>` entries.
pub trait ActionContextFamily {
    /// Concrete context borrowed for one submit/undo/redo call.
    type Context<'a>: ActionContext + 'a;
}

/// Default context family for World-only actions.
pub struct WorldActionContext;

impl ActionContextFamily for WorldActionContext {
    type Context<'a> = World;
}

// ---------------------------------------------------------------------------
// MergeOutcome
// ---------------------------------------------------------------------------

/// Outcome of attempting to merge two same-target [`Action`]s during coalescing.
#[derive(Debug, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Successfully merged — drop `next`, keep this [`Action`] with merged state.
    Merged,
    /// Cannot merge (different targets / different operations) — keep both.
    Distinct,
}

// ---------------------------------------------------------------------------
// ActionView
// ---------------------------------------------------------------------------

/// Context-independent view of an action used by coalescing.
///
/// Merging must be able to compare a pending action to the existing stack entry
/// even when the two live behind a richer context type. The view deliberately
/// exposes only stable metadata and payload bytes; it cannot apply or revert.
pub trait ActionView {
    /// Stable name for diagnostics + audit-ledger payload.
    fn name(&self) -> &str;

    /// Stable identifier for coalescing target.
    fn id(&self) -> ActionId;

    /// Serialize for audit-ledger payload and merge diagnostics.
    fn payload(&self) -> Vec<u8>;
}

/// Borrowed [`ActionView`] adapter for actions over non-default contexts.
pub struct ActionViewRef<'a, F: ActionContextFamily = WorldActionContext> {
    action: &'a dyn Action<F>,
}

impl<'a, F: ActionContextFamily> ActionViewRef<'a, F> {
    /// Construct a view over `action`.
    #[must_use]
    pub fn new(action: &'a dyn Action<F>) -> Self {
        Self { action }
    }
}

impl<F: ActionContextFamily + 'static> ActionView for ActionViewRef<'_, F> {
    fn name(&self) -> &str {
        self.action.name()
    }

    fn id(&self) -> ActionId {
        self.action.id()
    }

    fn payload(&self) -> Vec<u8> {
        self.action.payload()
    }
}

// ---------------------------------------------------------------------------
// Action trait
// ---------------------------------------------------------------------------

/// One reversible editor mutation.
///
/// Implementors:
/// - encapsulate the source entity/component/handle they mutate
/// - implement [`apply`](Action::apply) to perform the mutation against
///   the action context
/// - implement [`revert`](Action::revert) to undo the mutation byte-identically
/// - implement [`merge`](Action::merge) to coalesce with an adjacent
///   same-target [`Action`]
///
/// The context family defaults to [`WorldActionContext`]. That keeps `Box<dyn Action>`
/// object-safe and preserves the current World-only action shape, while
/// allowing an owning crate to use `Box<dyn Action<MyFamily>>` for actions
/// that need additional editor-owned state.
pub trait Action<F: ActionContextFamily = WorldActionContext>: Send + Sync + 'static {
    /// Stable name for diagnostics + audit-ledger payload (e.g. `"spawn-entity"`).
    fn name(&self) -> &str;

    /// Stable identifier for coalescing target. Same id within 500 ms coalesces.
    fn id(&self) -> ActionId;

    /// Apply the mutation.
    ///
    /// # Errors
    ///
    /// Returns [`ActionResult::MissingEntity`] when the target entity is absent,
    /// or [`ActionResult::ApplyFailed`] for any other apply-time failure.
    fn apply(&self, context: &mut F::Context<'_>) -> Result<(), ActionResult>;

    /// Revert the mutation. After successful `revert`, the world is byte-identical
    /// to its pre-[`apply`](Action::apply) state for the affected components.
    ///
    /// # Errors
    ///
    /// Returns [`ActionResult::RevertFailed`] when the revert cannot be completed,
    /// or [`ActionResult::MissingEntity`] when the target entity is absent.
    fn revert(&self, context: &mut F::Context<'_>) -> Result<(), ActionResult>;

    /// Try to merge `next` into self. Default: [`MergeOutcome::Distinct`] (no merging).
    ///
    /// Override to support coalescing. When [`MergeOutcome::Merged`] is returned,
    /// `self` holds the merged state and `next` is dropped.
    fn merge(&mut self, _next: &dyn ActionView) -> MergeOutcome {
        MergeOutcome::Distinct
    }

    /// Serialize for audit-ledger payload. Default: just the name as bytes.
    ///
    /// Override to capture parameters for richer replay diagnostics.
    fn payload(&self) -> Vec<u8> {
        self.name().as_bytes().to_vec()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unnecessary_literal_bound)]
mod tests {
    use rge_kernel_ecs::{Component, World};

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Marker(u32);
    impl Component for Marker {}

    /// A trivial Action that inserts/removes a `Marker` component.
    struct InsertMarker {
        entity: EntityId,
        value: u32,
    }

    impl Action for InsertMarker {
        fn name(&self) -> &str {
            "insert-marker"
        }

        fn id(&self) -> ActionId {
            ActionId::new(format!("insert-marker(entity={:?})", self.entity))
        }

        fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
            if world.entity(self.entity).is_none() {
                return Err(ActionResult::MissingEntity(self.entity));
            }
            world.insert(self.entity, Marker(self.value));
            Ok(())
        }

        fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
            world.remove::<Marker>(self.entity);
            Ok(())
        }
    }

    #[test]
    fn action_id_display() {
        let id = ActionId::new("test.action(entity=42)");
        assert_eq!(id.to_string(), "test.action(entity=42)");
    }

    #[test]
    fn default_merge_is_distinct() {
        let mut w = World::new();
        let e = w.spawn();
        let mut a = InsertMarker {
            entity: e,
            value: 1,
        };
        let b = InsertMarker {
            entity: e,
            value: 2,
        };
        assert_eq!(a.merge(&ActionViewRef::new(&b)), MergeOutcome::Distinct);
    }

    #[test]
    fn default_payload_is_name_bytes() {
        let mut w = World::new();
        let e = w.spawn();
        let a = InsertMarker {
            entity: e,
            value: 1,
        };
        assert_eq!(a.payload(), b"insert-marker");
    }

    #[test]
    fn apply_missing_entity_returns_error() {
        let mut w = World::new();
        // Spawn and immediately despawn to get a now-invalid EntityId.
        let e = w.spawn();
        w.despawn(e);
        let a = InsertMarker {
            entity: e,
            value: 0,
        };
        assert!(matches!(
            a.apply(&mut w),
            Err(ActionResult::MissingEntity(_))
        ));
    }
}
