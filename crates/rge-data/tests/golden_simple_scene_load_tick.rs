//! Load + tick regression test for `golden-projects/simple-scene`.
//!
//! Parses the tracked golden `.rge-project` manifest as `rge_data::Project`,
//! resolves its scene path relative to the project manifest directory, parses
//! that file as `rge_data::Scene`, and bridges it into a fresh
//! `rge_kernel_ecs::World` through a private test-local identity-only helper.
//! Asserts entity-count parity, per-entity existence by ULID, and that
//! `advance_tick` advances `current_tick` by one while `last_tick` captures
//! the prior current tick.
//!
//! The bridge is intentionally identity-only: it spawns one ECS entity per
//! parsed scene entity preserving the parsed ULID, and does not inspect or
//! translate components, relations, assets, cameras, lights, transforms,
//! renderer resources, editor state, scripts, or typed components.

use std::fs;
use std::path::{Path, PathBuf};

use rge_data::{Project, Scene};
use rge_kernel_ecs::{EntityId as EcsEntityId, World};

fn simple_scene_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("golden-projects")
        .join("simple-scene")
        .join(".rge-project")
}

fn read_simple_scene_manifest() -> String {
    let path = simple_scene_manifest_path();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn read_scene_referenced_by(project: &Project, manifest_path: &Path) -> (PathBuf, String) {
    let project_dir = manifest_path
        .parent()
        .expect("project manifest path has a parent directory");
    let scene_rel = project
        .scenes
        .first()
        .expect("project must reference at least one scene");
    let scene_path = project_dir.join(scene_rel.as_str());
    let text = fs::read_to_string(&scene_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", scene_path.display()));
    (scene_path, text)
}

fn load_scene_into_world(scene: &Scene) -> World {
    let mut world = World::new();
    for entity in &scene.entities {
        let id = EcsEntityId::from_ulid(entity.id.0);
        world.spawn_with_id(id);
    }
    world
}

#[test]
fn simple_scene_loads_into_world_and_advances_tick() {
    let manifest_path = simple_scene_manifest_path();
    let manifest_text = read_simple_scene_manifest();
    let project: Project =
        ron::from_str(&manifest_text).expect("parse simple-scene manifest as Project");

    let (_scene_path, scene_text) = read_scene_referenced_by(&project, &manifest_path);
    let scene: Scene = ron::from_str(&scene_text).expect("parse referenced scene as Scene");

    let mut world = load_scene_into_world(&scene);

    assert_eq!(
        world.entity_count(),
        scene.entities.len(),
        "world entity count must match parsed scene entity count"
    );

    for entity in &scene.entities {
        let id = EcsEntityId::from_ulid(entity.id.0);
        assert!(
            world.entity(id).is_some(),
            "scene entity {id} must exist in world after load"
        );
    }

    let before_current = world.current_tick();
    let before_last = world.last_tick();

    world.advance_tick();

    assert_eq!(
        world.current_tick(),
        before_current + 1,
        "advance_tick must increment current_tick by one"
    );
    assert_eq!(
        world.last_tick(),
        before_current,
        "advance_tick must capture the prior current_tick into last_tick"
    );
    assert_ne!(
        world.last_tick(),
        before_last,
        "last_tick must change after advance_tick from a fresh world"
    );
}
