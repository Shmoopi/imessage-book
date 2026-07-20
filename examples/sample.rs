//! Render a synthetic conversation to HTML + LaTeX (and PDF if an engine is present).
//!
//! Run with: `cargo run --example sample -- <out_dir>`
//! Useful for eyeballing formatting without a real iMessage database. When `ffmpeg` is
//! available it also synthesizes a short video and an animated GIF so the HTML preview's
//! playable-video and animated-GIF handling can be exercised end to end.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{Duration, NaiveDate};
use imessage_book::build::{self, Engine};
use imessage_book::model::{
    AttView, BookView, ChapterView, DayCount, DayView, EmojiStat, MonthStat, MsgView, StatsView,
    YearStat,
};
use imessage_book::{preview, render};

fn msg(
    from_me: bool,
    sender: Option<&str>,
    time: &str,
    text: Option<&str>,
    service: &str,
) -> MsgView {
    MsgView {
        from_me,
        sender: sender.map(str::to_string),
        time: time.to_string(),
        gap_before: None,
        text: text.map(str::to_string),
        attachments: vec![],
        tapbacks: vec![],
        reply: false,
        reply_to: None,
        edited: false,
        app: None,
        service: service.to_string(),
        ..Default::default()
    }
}

const MON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A deterministic ~14-month daily-activity series for the heatmap and trend charts.
fn synth_daily() -> Vec<DayCount> {
    let start = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
    let mut out = Vec::new();
    for i in 0..440i64 {
        // Skip roughly every few weeks to leave realistic gaps in the heatmap.
        if i % 21 == 4 || i % 21 == 5 {
            continue;
        }
        let wave = ((i as f64) * 0.10).sin() * 7.0 + 9.0;
        let bump = if i % 30 < 4 { 14 } else { 0 };
        let count = (wave.max(1.0) as usize) + bump + (i % 5) as usize;
        let date = start + Duration::days(i);
        out.push(DayCount {
            date: date.format("%Y-%m-%d").to_string(),
            count,
        });
    }
    out
}

/// Roll the daily series up into a per-month trend (≈47% sent by "you").
fn synth_monthly(daily: &[DayCount]) -> Vec<MonthStat> {
    let mut months: BTreeMap<String, usize> = BTreeMap::new();
    for d in daily {
        *months.entry(d.date[..7].to_string()).or_default() += d.count;
    }
    months
        .into_iter()
        .map(|(key, total)| {
            let from_me = total * 47 / 100;
            let mon: usize = key[5..7].parse().unwrap_or(1);
            MonthStat {
                label: format!("{} {}", MON[(mon - 1).min(11)], &key[..4]),
                key,
                total,
                from_me,
                from_others: total - from_me,
            }
        })
        .collect()
}

fn synth_yearly(monthly: &[MonthStat]) -> Vec<YearStat> {
    let mut years: BTreeMap<i32, usize> = BTreeMap::new();
    for m in monthly {
        let y: i32 = m.key[..4].parse().unwrap_or(0);
        *years.entry(y).or_default() += m.total;
    }
    years
        .into_iter()
        .map(|(year, total)| YearStat { year, total })
        .collect()
}

/// Best-effort synthesis of a playable MP4 and an animated GIF via ffmpeg. Returns the
/// relative paths that were produced (or `None` when ffmpeg is unavailable/failed).
fn synth_media(out_dir: &Path) -> (Option<String>, Option<String>) {
    let have_ffmpeg = Command::new("which")
        .arg("ffmpeg")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !have_ffmpeg {
        return (None, None);
    }
    let att = out_dir.join("attachments");
    let mp4 = att.join("att-video.mp4");
    let gif = att.join("att-anim.gif");

    let vid_ok = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=3:size=480x360:rate=15",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
        ])
        .arg(&mp4)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let gif_ok = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=240x180:rate=8",
        ])
        .arg(&gif)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    (
        vid_ok.then(|| "attachments/att-video.mp4".to_string()),
        gif_ok.then(|| "attachments/att-anim.gif".to_string()),
    )
}

fn book(video_src: Option<String>, gif_src: Option<String>) -> BookView {
    let mut hi = msg(
        false,
        Some("Naomi"),
        "3:42 PM",
        Some("hey!! how'd the interview go? 🤞"),
        "imessage",
    );
    hi.tapbacks = vec!["❤️ Loved · You".to_string()];

    let mut pic = msg(
        true,
        None,
        "3:45 PM",
        Some("nailed it — celebrating with this"),
        "imessage",
    );
    pic.attachments = vec![AttView {
        kind: "image".to_string(),
        src: Some("attachments/sample.png".to_string()),
        label: "IMG_0001.png".to_string(),
        caption: None,
        ..Default::default()
    }];

    let mut later = msg(
        false,
        Some("Naomi"),
        "7:15 PM",
        Some("we HAVE to go out this weekend"),
        "imessage",
    );
    later.gap_before = Some("3 hours later".to_string());
    later.edited = true;
    later.reply = true;
    later.reply_to = Some("nailed it — celebrating with this".to_string());

    // A video attachment: poster still for print, playable file for the web.
    let mut clip = msg(
        false,
        Some("Naomi"),
        "7:20 PM",
        Some("clip from earlier 🎥"),
        "imessage",
    );
    clip.attachments = vec![AttView {
        kind: "video".to_string(),
        src: Some("attachments/sample.png".to_string()),
        video_src,
        label: "IMG_0009.mov · 4 MB".to_string(),
        caption: Some("▶ Video".to_string()),
        ..Default::default()
    }];

    // A GIF: static still for print, animated original for the web.
    let mut giphy = msg(true, None, "7:22 PM", Some("lololol"), "imessage");
    giphy.attachments = vec![AttView {
        kind: "image".to_string(),
        src: Some("attachments/sample.png".to_string()),
        web_src: gif_src,
        label: "reaction.gif · 900 KB".to_string(),
        caption: None,
        ..Default::default()
    }];

    // A message with a link and an email, so the client-side linkifier is exercised.
    let mut link = msg(
        true,
        None,
        "7:26 PM",
        Some("read this when you get a sec: https://example.com/great-article — or email me@example.com"),
        "imessage",
    );
    link.app = Some("🔗 Link".to_string());

    let announce = MsgView {
        system: Some("Naomi named the conversation \u{201C}Us\u{201D}".to_string()),
        time: "7:30 PM".to_string(),
        ..Default::default()
    };

    let daily = synth_daily();
    let monthly = synth_monthly(&daily);
    let yearly = synth_yearly(&monthly);
    let total: usize = daily.iter().map(|d| d.count).sum();
    let from_me = total * 47 / 100;

    BookView {
        title: "The Naomben Chronicles".to_string(),
        author: Some("Your Name".to_string()),
        dedication: Some("Dedicated to you.".to_string()),
        is_group: false,
        message_count: 7,
        emoji_font: std::env::var("MB_EMOJI_FONT").ok(),
        stats: Some(StatsView {
            total,
            from_me,
            from_others: total - from_me,
            words: 14_500,
            words_received: 16_200,
            avg_words: 8.4,
            longest_message_words: 214,
            photos: 342,
            videos: 47,
            gifs: 88,
            audio: 12,
            links: 63,
            attachments_total: 489,
            median_response_minutes: Some(7),
            first_date: "January 1, 2023".to_string(),
            last_date: "March 15, 2024".to_string(),
            days: 440,
            active_days: daily.len() as i64,
            busiest_day: Some("Saturday, June 17".to_string()),
            busiest_hour: Some("9 PM".to_string()),
            longest_streak: 23,
            hourly: vec![
                2, 1, 0, 0, 0, 1, 3, 8, 15, 22, 30, 28, 26, 24, 20, 25, 32, 40, 52, 60, 48, 30, 14,
                6,
            ],
            weekday: vec![120, 180, 175, 190, 210, 260, 240],
            monthly,
            yearly,
            daily,
            top_emoji: vec![
                EmojiStat {
                    emoji: "😂".into(),
                    count: 412,
                },
                EmojiStat {
                    emoji: "❤️".into(),
                    count: 305,
                },
                EmojiStat {
                    emoji: "🤞".into(),
                    count: 128,
                },
                EmojiStat {
                    emoji: "🎉".into(),
                    count: 96,
                },
                EmojiStat {
                    emoji: "😭".into(),
                    count: 74,
                },
                EmojiStat {
                    emoji: "🔥".into(),
                    count: 51,
                },
            ],
            ..Default::default()
        }),
        gallery: vec!["attachments/sample.png".to_string()],
        chapters: vec![ChapterView {
            title: "November 2020".to_string(),
            id: "ch-2020-11".to_string(),
            days: vec![DayView {
                heading: "Monday, November 2".to_string(),
                messages: vec![hi, pic, later, clip, giphy, link, announce],
            }],
        }],
        ..Default::default()
    }
}

/// A real (abstract) photo so the sample's embedded-image bubble looks like an actual
/// Messages photo. Shipped alongside this example.
const SAMPLE_PHOTO: &[u8] = include_bytes!("sample-photo.png");

fn main() -> anyhow::Result<()> {
    let out: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sample-out".into())
        .into();
    std::fs::create_dir_all(out.join("attachments"))?;
    std::fs::write(out.join("attachments/sample.png"), SAMPLE_PHOTO)?;

    let (video_src, gif_src) = synth_media(&out);

    let b = book(video_src, gif_src);
    let html = render::html::render(&b)?;
    preview::write_index(&out, &html)?;

    let latex = render::latex::render(&b)?;
    build::write_source(&out, &latex)?;
    build::build_pdf(&out, Engine::Auto, false)?;

    render::epub::build(&b, &out, false)?;

    println!(
        "Wrote {}/index.html, {}/book.pdf, {}/book.epub",
        out.display(),
        out.display(),
        out.display()
    );
    Ok(())
}
