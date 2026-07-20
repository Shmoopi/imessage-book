//! Attachment orchestration: resolve → materialize → convert → cache.

pub mod convert;
pub mod icloud;

use std::path::{Path, PathBuf};
use std::time::Duration;

use imessage_database::tables::attachment::{Attachment, MediaType};
use imessage_database::util::platform::Platform;

use crate::model::{AttKind, AttView};

/// How much attachment processing to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachMode {
    /// Never embed; always render a labeled placeholder.
    None,
    /// Embed still images and video poster frames.
    Media,
}

/// User-facing attachment options.
#[derive(Debug, Clone)]
pub struct AttachOptions {
    pub mode: AttachMode,
    /// Opt-in: download offloaded attachments from iCloud.
    pub download_icloud: bool,
    /// Skip files larger than this many bytes (source size), if set.
    pub max_bytes: Option<u64>,
    /// Directory (relative to output root) where processed files are written.
    pub subdir: String,
}

impl Default for AttachOptions {
    fn default() -> Self {
        AttachOptions {
            mode: AttachMode::Media,
            download_icloud: false,
            max_bytes: None,
            subdir: "attachments".to_string(),
        }
    }
}

/// Everything needed to resolve attachment paths for one run.
pub struct Processor<'a> {
    pub platform: Platform,
    /// For iOS this must be the backup *root*; for macOS it is unused by the library.
    pub attachment_db_root: PathBuf,
    /// Absolute output root directory (processed files go under `out_root/subdir`).
    pub out_root: &'a Path,
    pub opts: &'a AttachOptions,
}

/// Process a cover image into the output directory, transcoding if needed. Returns the
/// output filename (relative to `out_root`) on success.
pub fn process_cover(src: &Path, out_root: &Path) -> Option<String> {
    if !src.exists() {
        eprintln!("(cover image not found: {})", src.display());
        return None;
    }
    if std::fs::create_dir_all(out_root).is_err() {
        return None;
    }
    let ext: String = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    // Trust the extension only when the bytes agree — a `.jpeg` holding HEIC must still
    // be transcoded (see `content_is_jpeg_or_png`).
    let passthrough = convert::is_passthrough_image(&ext) && convert::content_is_jpeg_or_png(src);
    let (name, transcode) = if passthrough {
        (format!("cover.{ext}"), false)
    } else {
        ("cover.jpg".to_string(), true)
    };
    let out = out_root.join(&name);
    let result = if transcode {
        convert::transcode_image(src, &out)
    } else {
        std::fs::copy(src, &out)
            .map(|_| ())
            .map_err(anyhow::Error::from)
    };
    match result {
        Ok(()) => {
            convert::bake_orientation(&out);
            Some(name)
        }
        Err(e) => {
            eprintln!("(could not process cover image: {e})");
            None
        }
    }
}

/// The conversion applied to an attachment's source file.
#[derive(Debug, Clone, Copy)]
enum ConvertOp {
    Copy,
    Transcode,
    VideoPoster,
}

/// Where a web-only secondary output is recorded on the resulting [`AttView`].
#[derive(Debug, Clone, Copy)]
enum WebField {
    /// A browser-playable video → [`AttView::video_src`].
    Video,
    /// An animated / web-preferred image (a GIF) → [`AttView::web_src`].
    Web,
}

/// A secondary, web-only output — the copied original video or GIF — produced alongside
/// the print-safe `src` (a poster frame or static JPEG). Only the HTML preview uses it.
#[derive(Debug)]
struct WebAsset {
    op: ConvertOp,
    out: PathBuf,
    rel: String,
    field: WebField,
}

/// A self-contained unit of conversion work with no database dependency, so it can run
/// on a worker thread. Produced by [`Processor::plan_message`] and completed by
/// [`AttPlan::finalize`].
#[derive(Debug)]
pub struct ConvertJob {
    kind: AttKind,
    label: String,
    caption: Option<String>,
    rel: String,
    src: PathBuf,
    out: PathBuf,
    op: ConvertOp,
    /// Optional playable-video / animated-GIF copy for the web.
    web: Option<WebAsset>,
    download_icloud: bool,
    is_macos: bool,
    max_bytes: Option<u64>,
}

/// Either an already-decided placeholder or a conversion job to run in parallel.
#[derive(Debug)]
pub enum AttPlan {
    Placeholder(AttView),
    Job(ConvertJob),
}

impl AttPlan {
    /// Complete the plan into a renderable view. Safe to call from any thread.
    pub fn finalize(self) -> AttView {
        match self {
            AttPlan::Placeholder(view) => view,
            AttPlan::Job(job) => job.run(),
        }
    }
}

fn placeholder(kind: AttKind, label: String, caption: Option<String>) -> AttView {
    AttView {
        kind: kind.as_str().to_string(),
        src: None,
        video_src: None,
        web_src: None,
        label,
        caption,
    }
}

/// Run one conversion into `out`, returning whether `out` now holds a usable file.
/// A prior run's output is reused as a cache hit.
fn produce(src: &Path, out: &Path, op: ConvertOp) -> bool {
    if icloud::has_data(out) {
        return true; // cache hit from an earlier run
    }
    let result = match op {
        ConvertOp::Copy => std::fs::copy(src, out)
            .map(|_| ())
            .map_err(anyhow::Error::from),
        ConvertOp::Transcode => convert::transcode_image(src, out),
        ConvertOp::VideoPoster => convert::video_poster(src, out),
    };
    match result {
        Ok(()) => true,
        Err(e) => {
            eprintln!("  (skipping attachment output {}: {e})", out.display());
            false
        }
    }
}

/// True for a filename that names one of iMessage's internal, non-user-facing blobs —
/// chiefly the `pluginPayloadAttachment` that backs rich-link and app-balloon messages.
/// They carry no viewable content of their own (the balloon's app label or the message
/// text already represents the message), so we drop them instead of rendering a
/// meaningless "pluginPayloadAttachment · 2 KB" placeholder next to the real content.
///
/// The match is deliberately lax: these blobs turn up with inconsistent casing, are
/// sometimes stored as dotfiles (`.pluginPayloadAttachment`), and are sometimes named
/// with a UUID and a `.pluginPayloadAttachment` *extension*
/// (`1A15AF9B-…-F225DF200500.pluginPayloadAttachment`) — so match the marker as both the
/// leading name and the trailing extension.
fn is_internal_artifact_name(name: &str) -> bool {
    const MARKER: &str = "pluginpayloadattachment";
    let file = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(name)
        .trim_start_matches('.')
        .to_ascii_lowercase();
    file.starts_with(MARKER) || file.ends_with(&format!(".{MARKER}"))
}

/// Whether an attachment is an internal artifact, checked against both the display name
/// (`filename()`, which prefers `transfer_name`) and the raw on-disk `filename` path.
fn is_internal_artifact(att: &Attachment) -> bool {
    is_internal_artifact_name(att.filename())
        || att
            .filename
            .as_deref()
            .is_some_and(is_internal_artifact_name)
}

impl ConvertJob {
    /// Resolve the primary output's op and path, correcting a plan-time `Copy` when the
    /// source's real bytes aren't the JPEG/PNG its extension promised (e.g. HEIC stored
    /// under a `.jpeg` name). It re-targets to a transcoded `.jpg` so the output
    /// extension matches the content the PDF/HTML backend actually loads — and, being a
    /// new name, sidesteps any stale verbatim copy an earlier buggy run left cached.
    fn primary_output(&self) -> (ConvertOp, PathBuf, String) {
        if matches!(self.op, ConvertOp::Copy) && !convert::content_is_jpeg_or_png(&self.src) {
            let out = self.out.with_extension("jpg");
            let rel = Path::new(&self.rel)
                .with_extension("jpg")
                .to_string_lossy()
                .into_owned();
            return (ConvertOp::Transcode, out, rel);
        }
        (self.op, self.out.clone(), self.rel.clone())
    }

    fn run(self) -> AttView {
        // Materialize from iCloud if needed and allowed.
        if !icloud::has_data(&self.src) {
            let dataless = icloud::is_dataless(&self.src);
            let materialized = self.is_macos
                && self.download_icloud
                && dataless
                && icloud::materialize(&self.src, Duration::from_secs(60));
            if !materialized {
                let caption = if dataless {
                    Some("offloaded to iCloud".to_string())
                } else {
                    Some("file not found".to_string())
                };
                return placeholder(self.kind, self.label, caption);
            }
        }

        if let Some(max) = self.max_bytes {
            if std::fs::metadata(&self.src).map(|m| m.len()).unwrap_or(0) > max {
                return placeholder(
                    self.kind,
                    self.label,
                    Some("too large to embed".to_string()),
                );
            }
        }

        // Print-safe primary output (a poster frame for video, a JPEG/copy for images).
        // The Copy op is chosen at plan time from the filename extension; verify the
        // source's real bytes are the JPEG/PNG it promised before copying. Apple stores
        // some HEIC images under a `.jpeg`/`.jpg` transfer name, which xelatex/tectonic
        // can't embed — transcode those to a matching `.jpg` output instead.
        let (op, out, rel) = self.primary_output();
        let src = if produce(&self.src, &out, op) {
            // Phone photos store sideways pixels plus an EXIF orientation tag the PDF
            // backend ignores; bake the rotation into the pixels so they aren't sideways.
            // No-op for already-upright images and for the web/video copies handled below.
            if matches!(self.kind, AttKind::Image) {
                convert::bake_orientation(&out);
            }
            Some(rel.clone())
        } else {
            None
        };

        // Optional web-only copy: a playable video, or an animated GIF.
        let (mut video_src, mut web_src) = (None, None);
        if let Some(web) = &self.web {
            if produce(&self.src, &web.out, web.op) {
                match web.field {
                    WebField::Video => video_src = Some(web.rel.clone()),
                    WebField::Web => web_src = Some(web.rel.clone()),
                }
            }
        }

        // Nothing embeddable came out — fall back to the labeled placeholder.
        if src.is_none() && video_src.is_none() && web_src.is_none() {
            return placeholder(self.kind, self.label, None);
        }

        AttView {
            kind: self.kind.as_str().to_string(),
            src,
            video_src,
            web_src,
            label: self.label,
            caption: self.caption,
        }
    }
}

impl<'a> Processor<'a> {
    fn out_subdir(&self) -> PathBuf {
        self.out_root.join(&self.opts.subdir)
    }

    /// Classify by MIME type.
    fn classify(att: &Attachment) -> AttKind {
        match att.mime_type() {
            MediaType::Image(_) => AttKind::Image,
            MediaType::Video(_) => AttKind::Video,
            MediaType::Audio(_) => AttKind::Audio,
            _ => AttKind::Other,
        }
    }

    fn label(att: &Attachment) -> String {
        let name = Path::new(att.filename())
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| att.filename());
        format!("{name} · {}", att.file_size())
    }

    /// Resolve a message's attachments into plans. This is the DB-bound step; the
    /// returned [`AttPlan`]s carry everything needed to finish conversion off-thread.
    pub fn plan_message(
        &self,
        db: &rusqlite::Connection,
        msg: &imessage_database::tables::messages::Message,
    ) -> Vec<AttPlan> {
        let attachments = match Attachment::from_message(db, msg) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("  (could not load attachments for a message: {e})");
                return vec![];
            }
        };
        attachments
            .iter()
            .filter(|att| !is_internal_artifact(att))
            .map(|att| self.plan_one(att))
            .collect()
    }

    fn plan_one(&self, att: &Attachment) -> AttPlan {
        let kind = Self::classify(att);
        let label = Self::label(att);

        if self.opts.mode == AttachMode::None || matches!(kind, AttKind::Audio | AttKind::Other) {
            return AttPlan::Placeholder(placeholder(kind, label, None));
        }

        let Some(resolved) =
            att.resolved_attachment_path(&self.platform, &self.attachment_db_root, None)
        else {
            return AttPlan::Placeholder(placeholder(kind, label, None));
        };
        let src = PathBuf::from(resolved);

        let out_dir = self.out_subdir();
        if std::fs::create_dir_all(&out_dir).is_err() {
            return AttPlan::Placeholder(placeholder(kind, label, None));
        }

        // Sanitize the extension to ASCII alphanumerics so the generated output path
        // (att-<rowid>.<ext>) is always safe to embed unescaped in HTML/LaTeX.
        let ext: String = att
            .extension()
            .unwrap_or("")
            .to_lowercase()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect();

        // Build a web-only secondary copy (playable video / animated GIF) named
        // att-<rowid>-web.<ext>. The `-web` suffix keeps it distinct from the primary
        // poster/still, which is always att-<rowid>.jpg — otherwise a video whose on-disk
        // name ends in `.jpg` would collide with (and be shadowed by) its own poster.
        let subdir = &self.opts.subdir;
        let web_asset = |ext: &str, field: WebField| -> WebAsset {
            let name = format!("att-{}-web.{}", att.rowid, ext);
            WebAsset {
                op: ConvertOp::Copy,
                out: out_dir.join(&name),
                rel: format!("{subdir}/{name}"),
                field,
            }
        };

        let (out_name, op, caption, web): (String, ConvertOp, Option<String>, Option<WebAsset>) =
            match kind {
                // Animated GIF: a static JPEG for print backends, plus the original GIF
                // for the web so it keeps animating.
                AttKind::Image if ext == "gif" => (
                    format!("att-{}.jpg", att.rowid),
                    ConvertOp::Transcode,
                    None,
                    Some(web_asset("gif", WebField::Web)),
                ),
                AttKind::Image if convert::is_passthrough_image(&ext) => (
                    format!("att-{}.{}", att.rowid, ext),
                    ConvertOp::Copy,
                    None,
                    None,
                ),
                AttKind::Image => (
                    format!("att-{}.jpg", att.rowid),
                    ConvertOp::Transcode,
                    None,
                    None,
                ),
                // Video: a poster frame for print/thumbnails, plus the original video
                // (copied) so the preview can actually play it back.
                AttKind::Video => {
                    let web = (!ext.is_empty()).then(|| web_asset(&ext, WebField::Video));
                    (
                        format!("att-{}.jpg", att.rowid),
                        ConvertOp::VideoPoster,
                        Some("▶ Video".to_string()),
                        web,
                    )
                }
                _ => return AttPlan::Placeholder(placeholder(kind, label, None)),
            };

        AttPlan::Job(ConvertJob {
            kind,
            label,
            caption,
            rel: format!("{}/{}", self.opts.subdir, out_name),
            out: out_dir.join(&out_name),
            src,
            op,
            web,
            download_icloud: self.opts.download_icloud,
            is_macos: self.platform == Platform::macOS,
            max_bytes: self.opts.max_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::is_internal_artifact_name;

    #[test]
    fn hides_plugin_payload_artifacts() {
        assert!(is_internal_artifact_name("pluginPayloadAttachment"));
        assert!(is_internal_artifact_name(
            "/var/mobile/Library/SMS/Attachments/ab/00/pluginPayloadAttachment"
        ));
        assert!(is_internal_artifact_name("pluginPayloadAttachment-1"));
        // Casing and dotfile variants also seen in the wild.
        assert!(is_internal_artifact_name(".pluginPayloadAttachment"));
        assert!(is_internal_artifact_name("PluginPayloadAttachment"));
        assert!(is_internal_artifact_name("/x/y/.pluginPayloadAttachment"));
        // UUID-named blobs carry the marker as the file *extension* instead.
        assert!(is_internal_artifact_name(
            "1A15AF9B-D79A-4810-B02E-F225DF200500.pluginPayloadAttachment"
        ));
        assert!(is_internal_artifact_name(
            "/var/mobile/Library/SMS/Attachments/ab/00/1A15AF9B-D79A-4810-B02E-F225DF200500.pluginPayloadAttachment"
        ));
        assert!(is_internal_artifact_name(
            "1A15AF9B-D79A-4810-B02E-F225DF200500.PLUGINPAYLOADATTACHMENT"
        ));
        // Real, user-facing attachments are kept.
        assert!(!is_internal_artifact_name("IMG_1234.HEIC"));
        assert!(!is_internal_artifact_name("voicememo.caf"));
        assert!(!is_internal_artifact_name("/tmp/Photo.jpg"));
    }
}
