//! Sniff image dimensions from the leading bytes of PNG, JPEG, GIF, and
//! WebP files. Used by `fetch` to verify a page's `og:image` actually
//! renders at preview quality — without pulling in an image crate for
//! what is a four-header problem.
//!
//! All formats keep dimensions near the front, so 64 KB of body is
//! plenty. Progressive JPEGs occasionally push the SOF marker deeper;
//! those return `None` and the audit simply reports nothing rather
//! than guessing.

/// Returns `(width, height)` if the byte prefix is a recognisable image.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    png(bytes).or_else(|| gif(bytes)).or_else(|| webp(bytes)).or_else(|| jpeg(bytes))
}

/// True when the prefix carries any known image signature — used to tell
/// "wrong dimensions" apart from "not an image at all" (a bot-challenge
/// HTML page served where the image should be).
pub fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G'])
        || bytes.starts_with(b"GIF8")
        || (bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(b"<svg")
        || bytes.starts_with(b"<?xml")
}

fn png(b: &[u8]) -> Option<(u32, u32)> {
    // Signature (8) + IHDR length/type (8) + width/height at 16..24.
    if b.len() < 24 || !b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return None;
    }
    let w = u32::from_be_bytes(b[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(b[20..24].try_into().ok()?);
    Some((w, h))
}

fn gif(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 10 || !b.starts_with(b"GIF8") {
        return None;
    }
    let w = u16::from_le_bytes([b[6], b[7]]) as u32;
    let h = u16::from_le_bytes([b[8], b[9]]) as u32;
    Some((w, h))
}

fn webp(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 30 || &b[..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return None;
    }
    match &b[12..16] {
        // Extended format: 24-bit little-endian canvas size minus one.
        b"VP8X" => {
            let w = 1 + u32::from_le_bytes([b[24], b[25], b[26], 0]);
            let h = 1 + u32::from_le_bytes([b[27], b[28], b[29], 0]);
            Some((w, h))
        }
        // Lossy: frame header behind a 3-byte frame tag + start code.
        b"VP8 " => {
            if b[23..26] != [0x9D, 0x01, 0x2A] {
                return None;
            }
            let w = (u16::from_le_bytes([b[26], b[27]]) & 0x3FFF) as u32;
            let h = (u16::from_le_bytes([b[28], b[29]]) & 0x3FFF) as u32;
            Some((w, h))
        }
        // Lossless: 14-bit width-1 / height-1 packed after the 0x2F byte.
        b"VP8L" => {
            if b[20] != 0x2F {
                return None;
            }
            let bits = u32::from_le_bytes([b[21], b[22], b[23], b[24]]);
            let w = 1 + (bits & 0x3FFF);
            let h = 1 + ((bits >> 14) & 0x3FFF);
            Some((w, h))
        }
        _ => None,
    }
}

fn jpeg(b: &[u8]) -> Option<(u32, u32)> {
    if !b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return None;
    }
    // Walk the segment chain until a start-of-frame marker.
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        // SOF0..SOF15 minus DHT (C4), JPG (C8), DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        // Skip this segment by its declared length.
        if matches!(marker, 0xD8 | 0x01 | 0xD0..=0xD9) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_dimensions() {
        let mut b = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        b.extend_from_slice(&[0, 0, 0, 13]); // IHDR length
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&1200u32.to_be_bytes());
        b.extend_from_slice(&630u32.to_be_bytes());
        assert_eq!(dimensions(&b), Some((1200, 630)));
    }

    #[test]
    fn gif_dimensions() {
        let mut b = b"GIF89a".to_vec();
        b.extend_from_slice(&800u16.to_le_bytes());
        b.extend_from_slice(&418u16.to_le_bytes());
        assert_eq!(dimensions(&b), Some((800, 418)));
    }

    #[test]
    fn webp_vp8x_dimensions() {
        let mut b = b"RIFF".to_vec();
        b.extend_from_slice(&[0; 4]); // file size (ignored)
        b.extend_from_slice(b"WEBP");
        b.extend_from_slice(b"VP8X");
        b.extend_from_slice(&[10, 0, 0, 0]); // chunk size
        b.extend_from_slice(&[0; 4]); // flags + reserved
        let w = 1200u32 - 1;
        let h = 630u32 - 1;
        b.extend_from_slice(&[(w & 0xFF) as u8, ((w >> 8) & 0xFF) as u8, ((w >> 16) & 0xFF) as u8]);
        b.extend_from_slice(&[(h & 0xFF) as u8, ((h >> 8) & 0xFF) as u8, ((h >> 16) & 0xFF) as u8]);
        assert_eq!(dimensions(&b), Some((1200, 630)));
    }

    #[test]
    fn jpeg_sof0_dimensions() {
        // SOI, APP0 (minimal), SOF0 with 630x1200.
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
        b.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        b.extend_from_slice(&630u16.to_be_bytes());
        b.extend_from_slice(&1200u16.to_be_bytes());
        b.extend_from_slice(&[0; 10]);
        assert_eq!(dimensions(&b), Some((1200, 630)));
    }

    #[test]
    fn html_is_not_an_image() {
        assert_eq!(dimensions(b"<!DOCTYPE html><html>..."), None);
        assert!(!looks_like_image(b"<!DOCTYPE html>"));
        assert!(looks_like_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
    }
}
