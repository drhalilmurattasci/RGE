//! Frame-graph resource identity + usage classification.
//!
//! [`ResourceId`] mirrors `kernel/io-scheduler::IoRequestId` in shape — opaque
//! 16-byte caller-supplied identifier with `const` accessors so callers can
//! build sentinel ids at compile time. The frame-graph does not interpret
//! the bytes; callers may use any encoding (BLAKE3 prefix / atomic counter /
//! hand-rolled).

use serde::{Deserialize, Serialize};

/// Opaque 16-byte caller-supplied identifier for a frame-graph resource.
///
/// Resources are conceptually transient GPU allocations (textures, buffers,
/// uniform blocks) that pass authors declare reads/writes against. The
/// frame-graph itself owns no GPU memory — it produces ordering and lifetime
/// metadata that an eventual allocator (out of scope for v0) consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceId([u8; 16]);

impl ResourceId {
    /// Construct from raw bytes. `const` so callers can build sentinel ids
    /// at compile time.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying bytes. `const` for parity with [`from_bytes`].
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Render as a short hex prefix for legibility in test failure
        // messages and diagnostic output.
        for byte in &self.0[..4] {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// How a pass uses a resource.
///
/// Frame-graph v0 derives execution dependencies from RAW (Read-After-Write)
/// only; WAR / WAW dependencies are NON-GOAL (see `frame_graph` module-doc
/// `# NON-GOALS`). The variant is `#[non_exhaustive]` so future extensions
/// (e.g. `Indirect` for indirect-draw resources, `Persistent` for
/// frame-spanning resources) are non-breaking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ResourceUsage {
    /// Pass reads this resource.
    Read,
    /// Pass writes this resource.
    Write,
    /// Pass both reads and writes (counts as a read-site for lifetime
    /// analysis; v0 does not yet distinguish read-then-write semantics
    /// for dependency tracking).
    ReadWrite,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_id_round_trip_via_bytes() {
        let bytes = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let id = ResourceId::from_bytes(bytes);
        assert_eq!(id.as_bytes(), &bytes);
    }

    #[test]
    fn resource_id_zero_and_max_distinct() {
        let z = ResourceId::from_bytes([0u8; 16]);
        let m = ResourceId::from_bytes([0xffu8; 16]);
        assert_ne!(z, m);
    }

    #[test]
    fn resource_id_display_shows_hex_prefix() {
        let id =
            ResourceId::from_bytes([0xab, 0xcd, 0xef, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(format!("{id}"), "abcdef01");
    }

    #[test]
    fn resource_id_ord_matches_byte_order() {
        let a = ResourceId::from_bytes([0u8; 16]);
        let b = ResourceId::from_bytes([1u8; 16]);
        assert!(a < b);
    }

    #[test]
    #[allow(
        unreachable_patterns,
        reason = "intentional: simulates cross-crate consumer pattern; \
                  same-crate compilation sees the enum as exhaustive so the \
                  wildcard arm is unreachable from inside the crate, but the \
                  `#[non_exhaustive]` SemVer barrier requires it for external \
                  consumers"
    )]
    fn resource_usage_non_exhaustive_pattern_compiles() {
        let usage = ResourceUsage::Read;
        let _name = match usage {
            ResourceUsage::Read => "read",
            ResourceUsage::Write => "write",
            ResourceUsage::ReadWrite => "read_write",
            _ => "future-variant",
        };
    }
}
