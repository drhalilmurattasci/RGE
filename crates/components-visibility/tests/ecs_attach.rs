//! ECS round-trip: `Visibility` is attachable to `rge_kernel_ecs::World`.

use rge_components_visibility::Visibility;
use rge_kernel_ecs::World;

#[test]
fn visibility_insert_and_retrieve() {
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, Visibility::Hidden);

    let entity_ref = world.entity(entity).expect("entity exists");
    let got = entity_ref
        .get::<Visibility>()
        .expect("Visibility was inserted");
    assert_eq!(*got, Visibility::Hidden);
}

#[test]
fn visibility_spawn_with_default() {
    let mut world = World::new();
    let entity = world.spawn_with(Visibility::default());
    let entity_ref = world.entity(entity).expect("entity exists");
    let got = entity_ref
        .get::<Visibility>()
        .expect("Visibility present after spawn_with");
    assert_eq!(*got, Visibility::Inherited);
}
