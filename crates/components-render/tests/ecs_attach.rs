//! ECS round-trip: `Camera` and `Light` are attachable to `rge_kernel_ecs::World`.

use rge_components_render::{Camera, Light, LightKind, Projection};
use rge_kernel_ecs::World;

#[test]
fn camera_insert_and_retrieve() {
    let mut world = World::new();
    let entity = world.spawn();
    let value = Camera {
        projection: Projection::Orthographic {
            half_height: 4.0,
            near: -10.0,
            far: 10.0,
        },
        viewport: [0.0, 0.0, 1.0, 1.0],
        priority: 7,
        is_active: true,
    };
    world.insert(entity, value);

    let entity_ref = world.entity(entity).expect("entity exists");
    let got = entity_ref.get::<Camera>().expect("Camera was inserted");
    assert_eq!(*got, value);
}

#[test]
fn light_insert_and_retrieve() {
    let mut world = World::new();
    let entity = world.spawn();
    let value = Light {
        color: [0.25, 0.5, 0.75],
        kind: LightKind::Point {
            lumens: 250.0,
            range_m: 12.5,
        },
        affects_indirect: false,
    };
    world.insert(entity, value);

    let entity_ref = world.entity(entity).expect("entity exists");
    let got = entity_ref.get::<Light>().expect("Light was inserted");
    assert_eq!(*got, value);
}
