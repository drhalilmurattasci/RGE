//! Magic-byte format sniffer.
//!
//! Per W18 spec: detection is **purely magic-bytes**, no extension reliance.
//! Each codec's signature is well-defined and short:
//!
//! - PNG: `\x89PNG\r\n\x1a\n` — 8 bytes (RFC 2083 §3.1).
//! - JPEG: `\xFF\xD8\xFF` — 3 bytes (SOI marker + first segment marker).
//! - OpenEXR: `\x76\x2F\x31\x01` — 4 bytes (little-endian 0x01312f76).
//! - Radiance HDR: ASCII `#?RADIANCE` or `#?RGBE` at byte 0.

/// Recognized image formats.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageFormat {
    /// PNG — Portable Network Graphics.
    Png,
    /// JPEG — Joint Photographic Experts Group baseline / progressive.
    Jpeg,
    /// OpenEXR — ILM/Industrial Light & Magic HDR floating-point.
    OpenExr,
    /// Radiance HDR — RGBE-encoded HDR.
    RadianceHdr,
}

/// Sniff the format of a byte buffer using only magic bytes.
///
/// Returns `None` if no known signature matches the prefix.
#[must_use]
pub fn detect_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(ImageFormat::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ImageFormat::Jpeg);
    }
    // OpenEXR magic: 0x76 0x2F 0x31 0x01 (little-endian 0x01312f76).
    if bytes.starts_with(&[0x76, 0x2F, 0x31, 0x01]) {
        return Some(ImageFormat::OpenExr);
    }
    // Radiance HDR begins with `#?RADIANCE` or the legacy `#?RGBE`.
    if bytes.starts_with(b"#?RADIANCE") || bytes.starts_with(b"#?RGBE") {
        return Some(ImageFormat::RadianceHdr);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_magic() {
        let bytes = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
        assert_eq!(detect_format(&bytes), Some(ImageFormat::Png));
    }

    #[test]
    fn jpeg_magic_jfif() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, b'J', b'F', b'I', b'F'];
        assert_eq!(detect_format(&bytes), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn jpeg_magic_exif() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x00];
        assert_eq!(detect_format(&bytes), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn exr_magic() {
        let bytes = [0x76, 0x2F, 0x31, 0x01, 0x02, 0x00];
        assert_eq!(detect_format(&bytes), Some(ImageFormat::OpenExr));
    }

    #[test]
    fn hdr_magic_radiance() {
        let bytes = b"#?RADIANCE\n";
        assert_eq!(detect_format(bytes), Some(ImageFormat::RadianceHdr));
    }

    #[test]
    fn hdr_magic_rgbe() {
        let bytes = b"#?RGBE\n";
        assert_eq!(detect_format(bytes), Some(ImageFormat::RadianceHdr));
    }

    #[test]
    fn unknown_format() {
        let bytes = [0x00, 0x01, 0x02, 0x03];
        assert_eq!(detect_format(&bytes), None);
    }

    #[test]
    fn empty_buffer() {
        assert_eq!(detect_format(&[]), None);
    }

    #[test]
    fn truncated_png_prefix_is_not_detected() {
        let bytes = [0x89, b'P', b'N'];
        assert_eq!(detect_format(&bytes), None);
    }
}
