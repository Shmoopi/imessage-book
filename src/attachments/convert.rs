//! Converting attachments into embeddable formats.
//!
//! Neither xelatex nor web browsers render Apple's HEIC/HEIF, so still images are
//! transcoded to JPEG, and a single poster frame is extracted from videos. We prefer
//! macOS's built-in `sips` for images (zero install) and require `ffmpeg` for video
//! frames (falling back to a placeholder when it is absent).

use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Context, Result};

/// Image extensions that both xelatex/tectonic (xdvipdfmx) and browsers can embed
/// directly. Anything else — HEIC/HEIF/TIFF and notably GIF, which xelatex cannot
/// embed — is transcoded to JPEG.
const PASSTHROUGH_IMAGE: &[&str] = &["jpg", "jpeg", "png"];

fn tool_available(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn have_ffmpeg() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| tool_available("ffmpeg"))
}

fn have_sips() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| tool_available("sips"))
}

/// Can an image with this extension be embedded without conversion?
pub fn is_passthrough_image(ext: &str) -> bool {
    PASSTHROUGH_IMAGE.contains(&ext.to_lowercase().as_str())
}

/// Whether a file's leading bytes are a real JPEG or PNG — the still-image formats
/// xelatex/tectonic (xdvipdfmx) and browsers embed directly.
///
/// The passthrough decision keys off the *filename* extension, but Apple sometimes
/// stores HEIC bytes under a `.jpeg`/`.jpg` transfer name. Copying such a file verbatim
/// yields a `.jpeg` the PDF/HTML backend can't load ("Unable to load picture"), so
/// callers sniff the actual content and transcode when it doesn't match the extension.
pub fn content_is_jpeg_or_png(path: &Path) -> bool {
    let mut buf = [0u8; 8];
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(n) = f.read(&mut buf) else {
        return false;
    };
    let head = &buf[..n];
    // JPEG SOI + marker: FF D8 FF. PNG signature: 89 50 4E 47 0D 0A 1A 0A.
    head.starts_with(&[0xFF, 0xD8, 0xFF])
        || head.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
}

/// The EXIF orientation tag (1–8) describes how a viewer should rotate/flip a photo for
/// display; phones store the sensor's raw pixels plus this tag rather than rotating the
/// pixels. Browsers honor it, but xelatex/xdvipdfmx ignores it, so portrait phone photos
/// come out sideways in the PDF. We bake the rotation into the pixels and drop the tag so
/// every backend agrees. See [`bake_orientation`].
///
/// Reads the tag from a JPEG's APP1/Exif segment; returns 1 (upright) when absent or
/// unparseable. PNG and other formats don't carry it, so they always read as 1.
fn exif_orientation(data: &[u8]) -> u8 {
    // JPEG SOI.
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return 1;
    }
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            break;
        }
        let marker = data[i + 1];
        // Start of scan: pixel data follows, no more headers to read.
        if marker == 0xDA {
            break;
        }
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if seg_len < 2 {
            break;
        }
        let (seg_start, seg_end) = (i + 4, i + 2 + seg_len);
        if seg_end > data.len() {
            break;
        }
        // APP1 carrying an "Exif\0\0" identifier holds the TIFF block with the tag.
        if marker == 0xE1 {
            let seg = &data[seg_start..seg_end];
            if seg.len() > 6 && &seg[0..6] == b"Exif\0\0" {
                if let Some(o) = tiff_orientation(&seg[6..]) {
                    return o;
                }
            }
        }
        i = seg_end;
    }
    1
}

/// Find the Orientation tag (0x0112) in a TIFF/Exif block's first IFD. `tiff` starts at
/// the byte-order marker (`II`/`MM`).
fn tiff_orientation(tiff: &[u8]) -> Option<u8> {
    if tiff.len() < 8 {
        return None;
    }
    let be = match &tiff[0..2] {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let rd16 = |o: usize| -> Option<u16> {
        let b = tiff.get(o..o + 2)?;
        Some(if be {
            u16::from_be_bytes([b[0], b[1]])
        } else {
            u16::from_le_bytes([b[0], b[1]])
        })
    };
    let rd32 = |o: usize| -> Option<u32> {
        let b = tiff.get(o..o + 4)?;
        Some(if be {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        })
    };
    let ifd = rd32(4)? as usize;
    let count = rd16(ifd)? as usize;
    for e in 0..count {
        let entry = ifd + 2 + e * 12;
        if rd16(entry)? == 0x0112 {
            // Type SHORT: the value sits in the first two bytes of the value field.
            let val = rd16(entry + 8)?;
            return (1..=8).contains(&val).then_some(val as u8);
        }
    }
    None
}

/// Read up to `max` bytes from the start of a file (EXIF lives near the top).
fn read_head(path: &Path, max: usize) -> Vec<u8> {
    let Ok(f) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut buf = Vec::new();
    let _ = f.take(max as u64).read_to_end(&mut buf);
    buf
}

/// Bake a JPEG's EXIF orientation into its pixels and clear the tag, in place, so the PDF
/// (which ignores the tag) and the HTML preview (which honors it) both render it upright.
///
/// No-op when the image is already upright, isn't a JPEG, or when `ffmpeg` is absent —
/// `sips` can rotate pixels but can't drop the tag, and a rotated-pixels-but-tag-kept file
/// would double-rotate in the browser, so we'd rather leave it untouched than corrupt it.
pub fn bake_orientation(path: &Path) {
    // The tag lives in APP1, which is capped at ~64 KiB and sits right after the SOI.
    if exif_orientation(&read_head(path, 128 * 1024)) == 1 {
        return;
    }
    if !have_ffmpeg() {
        return;
    }
    // ffmpeg's default autorotate applies the orientation and writes no orientation tag.
    // Encode to a sibling temp (the `.jpg` name forces JPEG) then swap it in.
    let tmp = path.with_extension("orient-tmp.jpg");
    let done = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(path)
        .args(["-q:v", "2"])
        .arg(&tmp)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if done && tmp.exists() {
        let _ = std::fs::rename(&tmp, path);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// A converter may leave a truncated/zero-byte file behind after a failed run. Remove
/// it so a later run doesn't mistake the partial file for a valid cached conversion.
fn cleanup_partial(dst: &Path) {
    let _ = std::fs::remove_file(dst);
}

/// Transcode a still image to JPEG at `dst`. Uses `sips`, then `ffmpeg`.
pub fn transcode_image(src: &Path, dst: &Path) -> Result<()> {
    if have_sips() {
        let out = Command::new("sips")
            .args(["-s", "format", "jpeg"])
            .arg(src)
            .arg("--out")
            .arg(dst)
            .output()
            .context("running sips")?;
        if out.status.success() && dst.exists() {
            return Ok(());
        }
    }
    if have_ffmpeg() {
        let out = Command::new("ffmpeg")
            .arg("-y")
            .arg("-i")
            .arg(src)
            .arg(dst)
            .output()
            .context("running ffmpeg for image")?;
        if out.status.success() && dst.exists() {
            return Ok(());
        }
    }
    cleanup_partial(dst);
    anyhow::bail!(
        "could not transcode image {} (need sips or ffmpeg)",
        src.display()
    );
}

/// Extract a single poster frame from a video to `dst` (JPEG). Requires `ffmpeg`.
pub fn video_poster(src: &Path, dst: &Path) -> Result<()> {
    if !have_ffmpeg() {
        anyhow::bail!("ffmpeg not available for video poster frame");
    }
    let out = Command::new("ffmpeg")
        .arg("-y")
        .args(["-i"])
        .arg(src)
        .args(["-frames:v", "1", "-q:v", "3"])
        .arg(dst)
        .output()
        .context("running ffmpeg for video frame")?;
    if out.status.success() && dst.exists() {
        return Ok(());
    }
    cleanup_partial(dst);
    anyhow::bail!("ffmpeg failed to extract a frame from {}", src.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_classification() {
        assert!(is_passthrough_image("jpg"));
        assert!(is_passthrough_image("PNG")); // case-insensitive
        assert!(!is_passthrough_image("heic"));
        assert!(!is_passthrough_image("tiff"));
        assert!(!is_passthrough_image("gif")); // GIF must transcode; xelatex can't embed it
    }

    #[test]
    fn content_sniff_sees_past_a_lying_extension() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, bytes: &[u8]| {
            let p = dir.path().join(name);
            std::fs::File::create(&p).unwrap().write_all(bytes).unwrap();
            p
        };
        let jpeg = write("a.jpeg", &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F']);
        let png = write("a.png", &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        // HEIC 'ftyp heic' box — as Apple stores it under a `.jpeg` transfer name.
        let heic = write("fake.jpeg", &[0, 0, 0, 0x18, b'f', b't', b'y', b'p', b'h', b'e', b'i', b'c']);
        assert!(content_is_jpeg_or_png(&jpeg));
        assert!(content_is_jpeg_or_png(&png));
        assert!(!content_is_jpeg_or_png(&heic)); // extension lies; bytes don't
        assert!(!content_is_jpeg_or_png(&dir.path().join("missing.jpeg")));
    }

    /// A minimal JPEG carrying a single little-endian Exif IFD entry: Orientation = `val`.
    fn jpeg_with_orientation(val: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II"); // little-endian
        tiff.extend_from_slice(&[0x2A, 0x00]); // TIFF magic (42)
        tiff.extend_from_slice(&[0x08, 0x00, 0x00, 0x00]); // IFD0 offset = 8
        tiff.extend_from_slice(&[0x01, 0x00]); // one entry
        tiff.extend_from_slice(&[0x12, 0x01]); // tag 0x0112 (Orientation)
        tiff.extend_from_slice(&[0x03, 0x00]); // type SHORT
        tiff.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // count 1
        tiff.extend_from_slice(&val.to_le_bytes()); // value ...
        tiff.extend_from_slice(&[0x00, 0x00]); // ... padded to 4 bytes
        tiff.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // next IFD = none

        let mut app1 = Vec::from(*b"Exif\0\0");
        app1.extend_from_slice(&tiff);
        let seg_len = (app1.len() + 2) as u16;

        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        jpeg.extend_from_slice(&seg_len.to_be_bytes());
        jpeg.extend_from_slice(&app1);
        jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI
        jpeg
    }

    #[test]
    fn reads_exif_orientation() {
        for val in 1..=8u16 {
            assert_eq!(exif_orientation(&jpeg_with_orientation(val)), val as u8);
        }
        // No Exif / not a JPEG → treated as upright.
        assert_eq!(exif_orientation(&[0xFF, 0xD8, 0xFF, 0xD9]), 1);
        assert_eq!(exif_orientation(&[0x89, b'P', b'N', b'G']), 1);
        assert_eq!(exif_orientation(&[]), 1);
        // Out-of-range tag value is ignored.
        assert_eq!(exif_orientation(&jpeg_with_orientation(99)), 1);
    }
}
