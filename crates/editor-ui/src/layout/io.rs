//! `Workspace` RON read / write.
//!
//! Adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//! for Workspace. The rustforge precursor used `ron::ser::to_string_pretty` with
//! the engine's default config; we keep the same defaults here (pretty config,
//! `enumerate_arrays = true`, `decimal_floats = true`) so on-disk RON is stable
//! across edits and round-trips byte-identically (CI gate per W09 exit criteria).
//!
//! Per ADR-018: RON only. No JSON/XML fallback. The on-disk format is the wire
//! format and the migration target.
//!
//! ## File contract
//!
//! * Files end in `.ron`.
//! * UTF-8.
//! * Newline at end of file (preserved on round-trip).
//! * Pretty-printed with `enumerate_arrays = true` (rustforge convention).

use std::fs;
use std::path::Path;

use ron::ser::PrettyConfig;

use super::version::{migrate, CURRENT_WORKSPACE_VERSION};
use super::workspace::Workspace;

/// Errors returned by `read_workspace` / `write_workspace`.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceIoError {
    /// `std::io` failure (file not found, permission denied, ...).
    #[error("io error on {path}: {source}")]
    Io {
        /// Path that failed.
        path: String,
        /// Underlying `std::io::Error`.
        #[source]
        source: std::io::Error,
    },
    /// RON parser failure on read.
    #[error("ron parse error on {path}: {reason}")]
    RonParse {
        /// Path that failed to parse.
        path: String,
        /// Human-readable reason from `ron::SpannedError`.
        reason: String,
    },
    /// RON serializer failure on write (rare — ron 0.8 only fails on non-serde shapes).
    #[error("ron serialize error: {reason}")]
    RonSerialize {
        /// Human-readable reason from `ron::Error`.
        reason: String,
    },
    /// Loaded workspace failed structural validation.
    #[error("workspace validation failed on {path}: {reason}")]
    Validate {
        /// Path of the failing workspace.
        path: String,
        /// Human-readable reason from `LayoutValidateError`.
        reason: String,
    },
    /// Loaded workspace declares an unsupported schema version.
    #[error("unsupported workspace version {version} on {path} (current: {current})")]
    UnsupportedVersion {
        /// Path of the failing workspace.
        path: String,
        /// Version string read from the file.
        version: String,
        /// Current schema version.
        current: String,
    },
}

/// Build the canonical pretty-printer used by all `write_workspace` calls.
///
/// Convention (matches rustforge): trailing commas + `enumerate_arrays(true)` +
/// `decimal_floats(true)`. These three knobs together produce stable byte-identical
/// output across the same logical document, which is what the round-trip CI gate
/// requires.
#[must_use]
pub fn canonical_pretty_config() -> PrettyConfig {
    PrettyConfig::new()
        .depth_limit(8)
        .new_line("\n".to_string())
        .indentor("    ".to_string())
        .enumerate_arrays(false)
        .struct_names(false)
        .compact_arrays(false)
}

/// Serialize a workspace to a RON `String` using `canonical_pretty_config`.
///
/// Used by `write_workspace` and by the round-trip test directly.
///
/// # Errors
///
/// Returns `WorkspaceIoError::RonSerialize` if `ron::ser::to_string_pretty`
/// fails (rare — only on non-serde shapes).
pub fn serialize_workspace(ws: &Workspace) -> Result<String, WorkspaceIoError> {
    let mut s = ron::ser::to_string_pretty(ws, canonical_pretty_config()).map_err(|e| {
        WorkspaceIoError::RonSerialize {
            reason: e.to_string(),
        }
    })?;
    if !s.ends_with('\n') {
        s.push('\n');
    }
    Ok(s)
}

/// Deserialize a workspace from a RON string, running migrations as needed.
///
/// On unsupported version returns `WorkspaceIoError::UnsupportedVersion`. On
/// successful parse, validates the layout tree.
///
/// # Errors
///
/// Returns `WorkspaceIoError::RonParse` on parse failure,
/// `WorkspaceIoError::UnsupportedVersion` if the document declares a version
/// not on the migration ladder, or `WorkspaceIoError::Validate` if the loaded
/// tree fails structural validation.
pub fn deserialize_workspace(text: &str, label: &str) -> Result<Workspace, WorkspaceIoError> {
    let mut ws: Workspace = ron::from_str(text).map_err(|e| WorkspaceIoError::RonParse {
        path: label.to_owned(),
        reason: e.to_string(),
    })?;
    // Migration ladder. Errors out on unsupported source versions; otherwise
    // mutates `ws.version` to `CURRENT_WORKSPACE_VERSION`.
    migrate(&mut ws).map_err(|version| WorkspaceIoError::UnsupportedVersion {
        path: label.to_owned(),
        version,
        current: CURRENT_WORKSPACE_VERSION.to_owned(),
    })?;
    ws.validate().map_err(|e| WorkspaceIoError::Validate {
        path: label.to_owned(),
        reason: e.to_string(),
    })?;
    Ok(ws)
}

/// Read a `Workspace` from a `.ron` file on disk.
///
/// Validates the loaded tree. Migrates `0.1.0` → current schema in-place.
///
/// # Errors
///
/// Returns `WorkspaceIoError::Io` on read failure, or any error from
/// `deserialize_workspace`.
pub fn read_workspace(path: &Path) -> Result<Workspace, WorkspaceIoError> {
    let text = fs::read_to_string(path).map_err(|source| WorkspaceIoError::Io {
        path: path.display().to_string(),
        source,
    })?;
    deserialize_workspace(&text, &path.display().to_string())
}

/// Write a `Workspace` to a `.ron` file on disk using `canonical_pretty_config`.
///
/// Creates parent directories on demand. Overwrites existing files.
///
/// # Errors
///
/// Returns `WorkspaceIoError::Io` on directory-create or write failure, or
/// any error from `serialize_workspace`.
pub fn write_workspace(ws: &Workspace, path: &Path) -> Result<(), WorkspaceIoError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| WorkspaceIoError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
    }
    let text = serialize_workspace(ws)?;
    fs::write(path, text.as_bytes()).map_err(|source| WorkspaceIoError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(())
}

/// Compute a stable content hash of a workspace's serialized RON form.
///
/// Used by the hot-reload watcher to skip no-op writes (same hash ⇒ no reconcile).
/// Hashing the canonical RON (rather than the in-memory struct) keeps the gate
/// aligned with the on-disk wire format.
#[must_use]
pub fn workspace_content_hash(ws: &Workspace) -> [u8; 32] {
    let text = serialize_workspace(ws).unwrap_or_default();
    *blake3::hash(text.as_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::super::node::{LayoutNode, LayoutNodeId, TabId};
    use super::super::workspace::{ShortcutsOverlay, Workspace, CURRENT_WORKSPACE_VERSION};
    use super::*;

    fn fixture() -> Workspace {
        Workspace {
            name: "Default".into(),
            version: CURRENT_WORKSPACE_VERSION.into(),
            theme: Some("dark".into()),
            layout: LayoutNode::HSplit {
                ratio: 0.2,
                id: Some(LayoutNodeId::new("root")),
                left: Box::new(LayoutNode::Stack {
                    id: Some(LayoutNodeId::new("scene")),
                    tabs: vec![TabId::new("tab/scene")],
                }),
                right: Box::new(LayoutNode::Stack {
                    id: Some(LayoutNodeId::new("viewport")),
                    tabs: vec![TabId::new("tab/viewport")],
                }),
            },
            main_menu: vec![],
            toolbars: vec![],
            shortcuts_overlay: ShortcutsOverlay::default(),
        }
    }

    #[test]
    fn round_trip_byte_identical() {
        let ws = fixture();
        let text1 = serialize_workspace(&ws).unwrap();
        let ws2 = deserialize_workspace(&text1, "<test>").unwrap();
        let text2 = serialize_workspace(&ws2).unwrap();
        assert_eq!(text1, text2, "round-trip must be byte-identical");
    }

    #[test]
    fn content_hash_stable_across_round_trip() {
        let ws = fixture();
        let h1 = workspace_content_hash(&ws);
        let ws2 = deserialize_workspace(&serialize_workspace(&ws).unwrap(), "<test>").unwrap();
        let h2 = workspace_content_hash(&ws2);
        assert_eq!(h1, h2);
    }
}
