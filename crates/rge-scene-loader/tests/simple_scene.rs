//! Golden simple-scene loader tests.
//!
//! Loads the tracked `golden-projects/simple-scene/scenes/main.rge-scene`
//! fixture into a [`World`] via [`load_scene_into_world`] and asserts that
//! the two pinned entities (`Camera`, `KeyLight`) come back with the typed
//! components the scene file declares.

use std::path::PathBuf;
use std::str::FromStr;

use rge_components_render::{Camera, Light};
use rge_components_spatial::Transform;
use rge_components_visibility::Visibility;
use rge_data::{EntityId as SceneEntityId, Project, Scene};
use rge_kernel_ecs::EntityId;
use rge_scene_loader::load_scene_into_world;

/// Canonical ULID for the `Camera` entity in the golden simple-scene fixture.
const CAMERA_ULID: &str = "0000000000000G000000000000";
/// Canonical ULID for the `KeyLight` entity in the golden simple-scene fixture.
const KEYLIGHT_ULID: &str = "00000000000010000000000000";

/// Read the tracked golden project manifest, resolve its first scene reference,
/// and parse that scene from disk. Read-only — never modifies anything under
/// `golden-projects/`.
fn load_golden_scene() -> Scene {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_path = manifest_dir
        .parent()
        .and_then(|crates_dir| crates_dir.parent())
        .expect("workspace root")
        .join("golden-projects/simple-scene/.rge-project");

    let project_raw = std::fs::read_to_string(&project_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", project_path.display()));
    let project: Project = ron::from_str(&project_raw)
        .unwrap_or_else(|e| panic!("parse {}: {e}", project_path.display()));

    let project_dir = project_path
        .parent()
        .expect("golden project manifest path has a parent directory");
    let scene_rel = project
        .scenes
        .first()
        .expect("golden simple-scene project references a scene");
    let scene_path = project_dir.join(scene_rel.as_str());
    let raw = std::fs::read_to_string(&scene_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", scene_path.display()));
    ron::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", scene_path.display()))
}

fn scene_entity_to_ecs(id_str: &str) -> EntityId {
    let scene_id = SceneEntityId::from_str(id_str).expect("canonical golden ULID must parse");
    EntityId::from_ulid(*scene_id.as_ulid())
}

#[test]
fn golden_camera_has_transform_camera_and_visible() {
    let scene = load_golden_scene();
    let world = load_scene_into_world(&scene).expect("load golden scene");

    let camera_id = scene_entity_to_ecs(CAMERA_ULID);
    let camera_entity = world
        .entity(camera_id)
        .expect("camera entity preserved via spawn_with_id + from_ulid");

    assert!(
        camera_entity.get::<Transform>().is_some(),
        "camera must carry Transform"
    );
    assert!(
        camera_entity.get::<Camera>().is_some(),
        "camera must carry Camera"
    );
    let vis = camera_entity
        .get::<Visibility>()
        .expect("camera must carry Visibility");
    assert_eq!(*vis, Visibility::Visible);
}

#[test]
fn golden_keylight_has_transform_and_light() {
    let scene = load_golden_scene();
    let world = load_scene_into_world(&scene).expect("load golden scene");

    let light_id = scene_entity_to_ecs(KEYLIGHT_ULID);
    let light_entity = world
        .entity(light_id)
        .expect("key light entity preserved via spawn_with_id + from_ulid");

    assert!(
        light_entity.get::<Transform>().is_some(),
        "key light must carry Transform"
    );
    assert!(
        light_entity.get::<Light>().is_some(),
        "key light must carry Light"
    );
}

#[test]
fn golden_scene_loads_two_entities_total() {
    let scene = load_golden_scene();
    let world = load_scene_into_world(&scene).expect("load golden scene");
    assert_eq!(world.entity_count(), scene.entities.len());
    assert_eq!(scene.entities.len(), 2);
}

#[test]
fn golden_loaded_world_obeys_advance_tick_contract() {
    let scene = load_golden_scene();
    let mut world = load_scene_into_world(&scene).expect("load golden scene");

    let prior_current_tick = world.current_tick();
    world.advance_tick();

    assert_eq!(
        world.current_tick(),
        prior_current_tick + 1,
        "advance_tick must increment current_tick by exactly one"
    );
    assert_eq!(
        world.last_tick(),
        prior_current_tick,
        "advance_tick must save the prior current_tick as last_tick"
    );
}
