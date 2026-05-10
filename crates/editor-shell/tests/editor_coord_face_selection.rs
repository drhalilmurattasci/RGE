//! Editor selection persistence sub-β — `EditorCoord.face_selection` field
//! integration smokes.
//!
//! Sub-α shipped `FaceSelection` + `FaceSelectionSet` + caller-driven
//! `partition` substrate inside `editor-state` (with most coverage living
//! in unit tests + a `cad-projection` integration smoke). Sub-β wires that
//! substrate into `editor-shell::EditorCoord` as a third public field
//! alongside `selection` and `active_tool` — the smallest honest
//! production-side proof that the substrate composes into actual editor
//! coordination state without any new wrapper API.
//!
//! These tests verify that:
//!
//! 1. `EditorCoord::new()` produces an empty `face_selection`.
//! 2. The full `FaceSelectionSet` API is reachable via direct public-field
//!    access (`coord.face_selection.add(...)`, `.clear()`, etc.) — exactly
//!    matching how `coord.selection.add(...)` already works for the
//!    entity-only selection field.
//! 3. The two selection fields are decoupled state — clearing one does not
//!    affect the other.
//! 4. `FaceSelectionSet::partition` is reachable through the field and
//!    returns the documented `(survivors, invalidated)` shape.
//!
//! Hard non-goals (NOT covered by this dispatch / file):
//!
//! - No `WorldSnapshot` integration. Per `lifecycle.rs`'s comment on
//!   `EditorCoord`, the coord container is *never* in `WorldSnapshot`;
//!   sub-β does not change that.
//! - No edge selections, automatic pruning, UI / GFX picking, or output-side
//!   FilletOp identity (`docs/architecture/FILLET_OUTPUT_IDENTITY.md`
//!   stays parked).
//! - No `EditorCoord` wrapper API on top of the field. The field is its
//!   own API, mirroring the existing `selection` precedent.

use rge_cad_core::{BRepFaceId, BRepOwnerId, CuboidFaceTag};
use rge_editor_shell::coord::{EditorCoord, FaceSelection, FaceSelectionSet};
use rge_kernel_ecs::EntityId;

const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0xab; 16]);

/// Build a `FaceSelection` referencing a fresh `EntityId` and the canonical
/// `TEST_OWNER`'s face_id for the supplied cuboid face tag.
fn make_face_selection(tag: CuboidFaceTag) -> FaceSelection {
    FaceSelection {
        entity: EntityId::new(),
        owner: TEST_OWNER,
        face_id: BRepFaceId::for_cuboid_face(TEST_OWNER, tag),
    }
}

#[test]
fn editor_coord_default_has_empty_face_selection() {
    let coord = EditorCoord::new();
    assert!(
        coord.face_selection.is_empty(),
        "fresh EditorCoord must have an empty face_selection"
    );
    assert_eq!(coord.face_selection.len(), 0);
}

#[test]
fn editor_coord_face_selection_can_be_added() {
    let mut coord = EditorCoord::new();
    let sel = make_face_selection(CuboidFaceTag::NegZ);

    let newly_added = coord.face_selection.add(sel);
    assert!(newly_added, "first add must report newly-added");
    assert_eq!(coord.face_selection.len(), 1);
    assert!(
        coord.face_selection.contains(&sel),
        "set must contain the just-added FaceSelection"
    );
}

#[test]
fn editor_coord_face_selection_clear_works() {
    let mut coord = EditorCoord::new();
    coord
        .face_selection
        .add(make_face_selection(CuboidFaceTag::NegZ));
    coord
        .face_selection
        .add(make_face_selection(CuboidFaceTag::PosZ));
    coord
        .face_selection
        .add(make_face_selection(CuboidFaceTag::NegY));
    assert_eq!(coord.face_selection.len(), 3);

    coord.face_selection.clear();
    assert!(
        coord.face_selection.is_empty(),
        "clear() must leave face_selection empty"
    );
    assert_eq!(coord.face_selection.len(), 0);
}

#[test]
fn editor_coord_face_selection_independent_from_entity_selection() {
    let mut coord = EditorCoord::new();
    let entity = EntityId::new();
    let face_sel = make_face_selection(CuboidFaceTag::NegZ);

    // Add to both fields.
    coord.selection.add(entity);
    coord.face_selection.add(face_sel);
    assert_eq!(coord.selection.len(), 1);
    assert_eq!(coord.face_selection.len(), 1);

    // Clearing the entity selection must not touch face_selection.
    coord.selection.clear();
    assert_eq!(coord.selection.len(), 0);
    assert_eq!(
        coord.face_selection.len(),
        1,
        "clearing entity selection must NOT affect face_selection"
    );

    // And the inverse: clearing face_selection must not touch entity selection.
    coord.selection.add(entity);
    coord.face_selection.clear();
    assert_eq!(coord.selection.len(), 1);
    assert_eq!(
        coord.face_selection.len(),
        0,
        "clearing face_selection must NOT affect entity selection"
    );
}

#[test]
fn editor_coord_face_selection_partition_via_field() {
    // Load-bearing: prove `FaceSelectionSet::partition` is reachable through
    // the public field with no wrapper, and returns the documented
    // `(survivors, invalidated)` shape.
    let mut coord = EditorCoord::new();
    let neg_z = make_face_selection(CuboidFaceTag::NegZ);
    let pos_z = make_face_selection(CuboidFaceTag::PosZ);
    let neg_y = make_face_selection(CuboidFaceTag::NegY);
    coord.face_selection.add(neg_z);
    coord.face_selection.add(pos_z);
    coord.face_selection.add(neg_y);
    assert_eq!(coord.face_selection.len(), 3);

    // Synthetic predicate: keep only `NegZ`'s face_id (everything else
    // lands in invalidated).
    let target_face_id = neg_z.face_id;
    let (survivors, invalidated) = coord
        .face_selection
        .partition(|fs| fs.face_id == target_face_id);

    assert_eq!(survivors.len(), 1);
    assert_eq!(invalidated.len(), 2);
    assert!(survivors.contains(&neg_z));
    assert!(invalidated.contains(&pos_z));
    assert!(invalidated.contains(&neg_y));

    // partition is a non-mutating read — the original set is unchanged.
    assert_eq!(
        coord.face_selection.len(),
        3,
        "partition must not mutate the source set"
    );
}

#[test]
fn editor_coord_face_selection_set_is_constructible_via_re_export() {
    // Sanity: the `FaceSelectionSet` re-export path works through
    // `rge_editor_shell::coord` — callers can pull both the type and the
    // field-access pattern from the same module.
    let set = FaceSelectionSet::new();
    assert!(set.is_empty());
}
