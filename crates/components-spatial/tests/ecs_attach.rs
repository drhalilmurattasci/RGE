//! ECS round-trip: `Transform` is attachable to `rge_kernel_ecs::World`.

use rge_components_spatial::Transform;
use rge_kernel_ecs::World;

#[test]
fn transform_insert_and_retrieve() {
    let mut world = World::new();
    let entity = world.spawn();
    let value = Transform::from_translation([1.0, 2.0, 3.0]);
    world.insert(entity, value);

    let entity_ref = world.entity(entity).expect("entity exists");
    let got = entity_ref
        .get::<Transform>()
        .expect("Transform was inserted");
    assert_eq!(*got, value);
}

#[test]
fn transform_spawn_with_query() {
    let mut world = World::new();
    let entity = world.spawn_with(Transform::IDENTITY);
    let entity_ref = world.entity(entity).expect("entity exists");
    let got = entity_ref
        .get::<Transform>()
        .expect("Transform present after spawn_with");
    assert_eq!(*got, Transform::IDENTITY);
}
