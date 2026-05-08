//! Workspace versioning + migration ladder.
//!
//! Adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//! for Workspace. The rustforge precursor versioned individual graphics-IR types via
//! a `validate()` call on each typed struct; here we add an explicit `version: String`
//! field and a migration ladder so old workspaces on disk can be brought forward
//! without losing user state.
//!
//! Per W09 exit criteria: `v0.1.0 → v0.2.0` migration is lossless on a fixture.
//! The ladder is structured as `migrate()` dispatching to per-step functions named
//! `migrate_0_1_0_to_0_2_0`. Each step mutates the `Workspace` in place and bumps
//! `ws.version`. Adding `0.3.0` later means appending one more step + raising
//! `CURRENT_WORKSPACE_VERSION` here.
//!
//! ## Wire-compatibility note
//!
//! `0.1.0` did not carry the `shortcuts_overlay` field. RON's `serde(default)`
//! handles that on the deserialize side; the migration here bumps the version
//! number so a subsequent save writes `0.2.0` to disk.

use super::workspace::Workspace;
pub use super::workspace::{CURRENT_WORKSPACE_VERSION, MIN_SUPPORTED_WORKSPACE_VERSION};

/// Migrate a `Workspace` to `CURRENT_WORKSPACE_VERSION` in place.
///
/// On success, `ws.version == CURRENT_WORKSPACE_VERSION`. On unsupported source
/// versions returns `Err(version_string)` so the caller (`io::deserialize_workspace`)
/// can surface a typed `WorkspaceIoError::UnsupportedVersion`.
///
/// The function is idempotent — calling it on an already-current workspace is a
/// no-op.
///
/// # Errors
///
/// Returns `Err(version)` carrying the offending version string if `ws.version`
/// is not on the migration ladder (i.e. unknown / future).
pub fn migrate(ws: &mut Workspace) -> Result<(), String> {
    loop {
        if ws.version == CURRENT_WORKSPACE_VERSION {
            return Ok(());
        }
        match ws.version.as_str() {
            "0.1.0" => migrate_0_1_0_to_0_2_0(ws),
            other => return Err(other.to_owned()),
        }
    }
}

/// `0.1.0 → 0.2.0`: introduce `shortcuts_overlay` field.
///
/// Lossless. `serde(default)` already injected the field on deserialize; the
/// migration here only bumps the version number so subsequent saves write
/// `0.2.0` to disk.
fn migrate_0_1_0_to_0_2_0(ws: &mut Workspace) {
    // No content rewrite needed — `Workspace` carries `shortcuts_overlay` with a
    // serde default. Preserve any explicitly-set value (the ladder step is the
    // version bump itself).
    "0.2.0".clone_into(&mut ws.version);
}

#[cfg(test)]
mod tests {
    use super::super::node::{LayoutNode, LayoutNodeId, TabId};
    use super::super::workspace::{ShortcutsOverlay, Workspace};
    use super::*;

    fn v0_1_0_fixture() -> Workspace {
        Workspace {
            name: "Default".into(),
            version: "0.1.0".into(),
            theme: None,
            layout: LayoutNode::Stack {
                id: Some(LayoutNodeId::new("scene")),
                tabs: vec![TabId::new("tab/scene")],
            },
            main_menu: vec![],
            toolbars: vec![],
            shortcuts_overlay: ShortcutsOverlay::default(),
        }
    }

    #[test]
    fn migrate_v0_1_0_to_current() {
        let mut ws = v0_1_0_fixture();
        migrate(&mut ws).expect("migration succeeds");
        assert_eq!(ws.version, CURRENT_WORKSPACE_VERSION);
    }

    #[test]
    fn migrate_idempotent_on_current() {
        let mut ws = v0_1_0_fixture();
        ws.version = CURRENT_WORKSPACE_VERSION.into();
        let before = ws.clone();
        migrate(&mut ws).unwrap();
        assert_eq!(ws, before);
    }

    #[test]
    fn migrate_rejects_future_version() {
        let mut ws = v0_1_0_fixture();
        ws.version = "99.0.0".into();
        let err = migrate(&mut ws).unwrap_err();
        assert_eq!(err, "99.0.0");
    }
}
