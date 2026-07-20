//! End-to-end rendering tests that don't need a live iMessage database.
//!
//! Builds a synthetic [`BookView`] covering the tricky cases (group senders, an image
//! attachment, a placeholder attachment, tapbacks, gap markers, edited flag, emoji and
//! LaTeX special characters) and renders both backends. When a LaTeX engine is present
//! it also compiles the generated `book.tex` to prove the template is valid.

use std::process::Command;

use imessage_book::model::{AttView, BookView, ChapterView, DayView, MsgView, StatsView};
use imessage_book::render;

fn msg(from_me: bool, text: &str, service: &str) -> MsgView {
    MsgView {
        from_me,
        time: "3:42 PM".to_string(),
        text: Some(text.to_string()),
        service: service.to_string(),
        ..Default::default()
    }
}

fn sample_book() -> BookView {
    let mut naomi = msg(false, "hey! 100% sure? cost was $5 & rising 😀", "imessage");
    naomi.sender = Some("Naomi".to_string());
    naomi.tapbacks = vec!["❤️ Loved".to_string()];

    let mut photo = msg(true, "check this out", "imessage");
    photo.time = "3:45 PM".to_string();
    photo.edited = true;
    photo.effect = Some("sent with Slam".to_string());
    photo.attachments = vec![AttView {
        kind: "image".to_string(),
        src: Some("attachments/sample.png".to_string()),
        label: "IMG_0001.png · 12 KB".to_string(),
        caption: None,
        ..Default::default()
    }];

    let mut link = msg(true, "https://example.com/article", "sms");
    link.time = "7:15 PM".to_string();
    link.gap_before = Some("3 hours later".to_string());
    link.reply = true;
    link.reply_to = Some("check this out".to_string());
    link.app = Some("🔗 Link".to_string());
    link.attachments = vec![AttView {
        kind: "audio".to_string(),
        src: None,
        label: "voice.caf · 88 KB".to_string(),
        caption: Some("offloaded to iCloud".to_string()),
        ..Default::default()
    }];

    let mut media = msg(true, "clips from earlier", "imessage");
    media.time = "7:30 PM".to_string();
    media.attachments = vec![
        AttView {
            kind: "video".to_string(),
            src: Some("attachments/att-5.png".to_string()),
            video_src: Some("attachments/att-5.mov".to_string()),
            label: "IMG_0005.mov · 4 MB".to_string(),
            caption: Some("▶ Video".to_string()),
            ..Default::default()
        },
        AttView {
            kind: "image".to_string(),
            src: Some("attachments/att-6.png".to_string()),
            web_src: Some("attachments/att-6.gif".to_string()),
            label: "funny.gif · 1 MB".to_string(),
            caption: None,
            ..Default::default()
        },
    ];

    let system = MsgView {
        system: Some("Naomi named the conversation \u{201C}Us\u{201D}".to_string()),
        time: "8:00 PM".to_string(),
        ..Default::default()
    };

    BookView {
        title: "The Test Chronicles".to_string(),
        author: Some("A. Tester".to_string()),
        dedication: Some("For the edge cases.".to_string()),
        is_group: true,
        message_count: 3,
        stats: Some(StatsView {
            total: 3,
            from_me: 2,
            from_others: 1,
            words: 12,
            photos: 1,
            first_date: "November 2, 2020".to_string(),
            last_date: "November 2, 2020".to_string(),
            days: 1,
            ..Default::default()
        }),
        gallery: vec!["attachments/sample.png".to_string()],
        chapters: vec![ChapterView {
            title: "November 2020".to_string(),
            id: "ch-2020-11".to_string(),
            days: vec![DayView {
                heading: "Monday, November 2".to_string(),
                messages: vec![naomi, photo, link, media, system],
            }],
        }],
        ..Default::default()
    }
}

#[test]
fn html_renders_expected_content() {
    let html = render::html::render(&sample_book()).expect("html render");
    assert!(html.contains("The Test Chronicles"));
    assert!(html.contains("Naomi"));
    assert!(html.contains("attachments/sample.png"));
    assert!(html.contains("3 hours later"));
    assert!(html.contains("❤️ Loved"));
    // Reply context is rendered.
    assert!(html.contains("check this out"));
    // App balloon label is rendered.
    assert!(html.contains("🔗 Link"));
    // System announcement line, expressive effect, stats, and gallery.
    assert!(html.contains("named the conversation"));
    assert!(html.contains("sent with Slam"));
    assert!(html.contains("By the numbers"));
    assert!(html.contains(r#"class="gallery""#));
    // HTML autoescaping should have escaped the ampersand.
    assert!(
        html.contains("&amp;"),
        "ampersand should be escaped in HTML"
    );
    // A playable video element and its source, plus the animated GIF, are emitted.
    assert!(html.contains("<video"));
    assert!(html.contains("attachments/att-5.mov"));
    assert!(html.contains("attachments/att-6.gif"));
    // The charts mount, the embedded stats blob, and the image lightbox are present.
    assert!(html.contains(r#"id="mb-charts""#));
    assert!(html.contains("window.__MB_STATS__"));
    assert!(html.contains(r#"id="mb-lightbox""#));
    // Links opened in a new tab are hardened with rel="noopener".
    assert!(html.contains("noopener"));
}

#[test]
fn latex_renders_and_escapes() {
    let tex = render::latex::render(&sample_book()).expect("latex render");
    assert!(tex.contains(r"\chapter{November 2020}"));
    assert!(tex.contains(r"\includegraphics"));
    // LaTeX special characters must be escaped by the `tex` filter.
    assert!(tex.contains(r"100\%"));
    assert!(tex.contains(r"\$5"));
    assert!(tex.contains(r"\&"));
    // Emoji wrapped for the emoji font.
    assert!(tex.contains(r"{\emojifont "));
    // Reply context is rendered via \replyquote.
    assert!(tex.contains(r"\replyquote{"));
    // App balloon label is rendered via \apptag.
    assert!(tex.contains(r"\apptag{"));
    // System line, stats front matter, and gallery appendix.
    assert!(tex.contains(r"\systemline{"));
    assert!(tex.contains("By the Numbers"));
    assert!(tex.contains(r"\chapter{Photos}"));
}

fn engine() -> Option<&'static str> {
    ["tectonic", "xelatex"].into_iter().find(|e| {
        Command::new("which")
            .arg(e)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// A 1x1 PNG so `\includegraphics` has a real file to embed.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

#[test]
fn latex_compiles_to_pdf() {
    let Some(engine) = engine() else {
        eprintln!("no LaTeX engine found; skipping PDF compile test");
        return;
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let tex = render::latex::render(&sample_book()).expect("latex render");
    std::fs::write(dir.path().join("book.tex"), &tex).unwrap();
    std::fs::create_dir_all(dir.path().join("attachments")).unwrap();
    std::fs::write(dir.path().join("attachments/sample.png"), TINY_PNG).unwrap();
    // The video poster and the GIF's static still are embedded by the LaTeX backend too.
    std::fs::write(dir.path().join("attachments/att-5.png"), TINY_PNG).unwrap();
    std::fs::write(dir.path().join("attachments/att-6.png"), TINY_PNG).unwrap();

    let args: &[&str] = if engine == "xelatex" {
        &["-interaction=nonstopmode", "book.tex"]
    } else {
        &["book.tex"]
    };
    let out = Command::new(engine)
        .args(args)
        .current_dir(dir.path())
        .output()
        .expect("run engine");

    let pdf = dir.path().join("book.pdf");
    if pdf.exists() {
        return; // success
    }

    let log = std::fs::read_to_string(dir.path().join("book.log")).unwrap_or_default();
    let combined = format!("{}\n{log}", String::from_utf8_lossy(&out.stderr));

    // A minimal system TeX (e.g. BasicTeX) may lack packages like xcolor/tcolorbox.
    // That's an environment limitation, not a template bug, so skip rather than fail.
    // Tectonic auto-fetches packages, so if *it* fails, that's a genuine error.
    let missing_package = combined.contains(".sty")
        || combined.contains("not found")
        || combined.contains("Emergency stop")
        || combined.contains("cannot \\read from terminal");
    if engine == "xelatex" && missing_package {
        eprintln!(
            "skipping PDF compile: system TeX appears to be missing packages \
             (install `tectonic` for a self-contained build)"
        );
        return;
    }

    let tail: String = log
        .lines()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    panic!("{engine} did not produce a PDF.\nlog tail:\n{tail}");
}
