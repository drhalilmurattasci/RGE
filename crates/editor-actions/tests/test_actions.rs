//! Shared helpers for integration tests.
#![allow(dead_code, unreachable_pub, missing_docs, clippy::pedantic)]

use rge_editor_actions::action::{Action, ActionId, ActionResult, ActionView, MergeOutcome};
use rge_kernel_ecs::{Component, EntityId, World};

// ---------------------------------------------------------------------------
// TestComponent
// ---------------------------------------------------------------------------

/// A simple test component carrying a single `i32` value.
#[derive(Debug, Clone, PartialEq)]
pub struct TestVal(pub i32);
impl Component for TestVal {}

// ---------------------------------------------------------------------------
// InsertAction
// ---------------------------------------------------------------------------

/// Insert `TestVal(value)` on apply; remove it on revert.
pub struct InsertAction {
    pub entity: EntityId,
    pub value: i32,
}

impl Action for InsertAction {
    fn name(&self) -> &str {
        "insert-test-val"
    }

    fn id(&self) -> ActionId {
        ActionId::new(format!("insert-test-val(entity={:?})", self.entity))
    }

    fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
        if world.entity(self.entity).is_none() {
            return Err(ActionResult::MissingEntity(self.entity));
        }
        world.insert(self.entity, TestVal(self.value));
        Ok(())
    }

    fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
        world.remove::<TestVal>(self.entity);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ModifyAction
// ---------------------------------------------------------------------------

/// Replace `TestVal` with a new value; restore the old value on revert.
pub struct ModifyAction {
    pub entity: EntityId,
    pub new_value: i32,
    pub old_value: i32,
}

impl Action for ModifyAction {
    fn name(&self) -> &str {
        "modify-test-val"
    }

    fn id(&self) -> ActionId {
        ActionId::new(format!("modify-test-val(entity={:?})", self.entity))
    }

    fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
        if world.entity(self.entity).is_none() {
            return Err(ActionResult::MissingEntity(self.entity));
        }
        world.insert(self.entity, TestVal(self.new_value));
        Ok(())
    }

    fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
        if world.entity(self.entity).is_none() {
            return Err(ActionResult::MissingEntity(self.entity));
        }
        world.insert(self.entity, TestVal(self.old_value));
        Ok(())
    }

    fn merge(&mut self, next: &dyn ActionView) -> MergeOutcome {
        // Only merge if the next action targets the same entity and is also a
        // ModifyAction (identified by name).
        if next.name() == "modify-test-val" && next.id() == self.id() {
            // The next action's `new_value` becomes our `new_value`.
            // We cannot downcast `dyn Action`, so encode new_value in payload
            // bytes: 4-byte LE i32.
            let p = next.payload();
            if p.len() >= 4 {
                let new_val = i32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                self.new_value = new_val;
                return MergeOutcome::Merged;
            }
        }
        MergeOutcome::Distinct
    }

    fn payload(&self) -> Vec<u8> {
        self.new_value.to_le_bytes().to_vec()
    }
}

// ---------------------------------------------------------------------------
// SpawnAction
// ---------------------------------------------------------------------------

/// Spawn an entity on apply; despawn it on revert.
pub struct SpawnAction {
    /// The entity id is set after the first apply.
    pub entity_out: std::sync::Mutex<Option<EntityId>>,
    pub initial_value: i32,
}

impl SpawnAction {
    pub fn new(initial_value: i32) -> Self {
        Self {
            entity_out: std::sync::Mutex::new(None),
            initial_value,
        }
    }
}

impl Action for SpawnAction {
    fn name(&self) -> &str {
        "spawn-entity"
    }

    fn id(&self) -> ActionId {
        ActionId::new("spawn-entity")
    }

    fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
        let e = world.spawn_with(TestVal(self.initial_value));
        *self.entity_out.lock().unwrap() = Some(e);
        Ok(())
    }

    fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
        if let Some(e) = *self.entity_out.lock().unwrap() {
            world.despawn(e);
        }
        Ok(())
    }
}
