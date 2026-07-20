//! Turn raw `imessage-database` messages into a [`BookView`].

use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike};
use imessage_database::message_types::expressives::{BubbleEffect, Expressive, ScreenEffect};
use imessage_database::message_types::variants::{Announcement, CustomBalloon, Variant};
use imessage_database::tables::messages::Service;
use imessage_database::tables::messages::{BubbleType, Message};
use imessage_database::tables::table::ME;
use rayon::prelude::*;
use regex::Regex;
use rusqlite::Connection;
use unicode_segmentation::UnicodeSegmentation;

use crate::attachments::{AttPlan, Processor};
use crate::config::{normalize_hex, Config};
use crate::db::contacts::{normalize, ContactResolver};
use crate::model::{
    BookView, ChapterView, DayCount, DayView, EmojiStat, MonthStat, MsgView, PageView, SenderStat,
    StatsView, ThemeView, YearStat,
};

/// Single emoji/pictographic characters, for the stats "top emoji" tally.
static EMOJI_CHAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\p{Extended_Pictographic}").expect("valid emoji regex"));

/// Matches URLs for the "links shared" tally (kept in sync with the preview's
/// client-side linkifier).
static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:https?://|www\.)[^\s<>()\[\]]+").expect("valid url regex")
});

/// Message-count / date-range narrowing for fast iteration.
#[derive(Debug, Default, Clone)]
pub struct Subset {
    pub limit: Option<usize>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Evenly sample this many messages across the whole range.
    pub sample: Option<usize>,
}

/// Default minimum same-day gap before a "… later" marker is shown.
const GAP_THRESHOLD_MINUTES: i64 = 60;

/// How messages are grouped into chapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Granularity {
    Month,
    Year,
    Week,
}

impl Granularity {
    fn parse(s: Option<&str>) -> Granularity {
        match s.map(str::to_lowercase).as_deref() {
            Some("year") => Granularity::Year,
            Some("week") => Granularity::Week,
            _ => Granularity::Month,
        }
    }

    /// The (stable id, human title) of the chapter a timestamp belongs to.
    fn chapter_key(self, ts: &DateTime<Local>) -> (String, String) {
        match self {
            Granularity::Month => (
                ts.format("ch-%Y-%m").to_string(),
                format!("{} {}", ts.format("%B"), ts.year()),
            ),
            Granularity::Year => (ts.format("ch-%Y").to_string(), ts.year().to_string()),
            Granularity::Week => {
                let date = ts.date_naive();
                let monday = date - Duration::days(date.weekday().num_days_from_monday() as i64);
                (
                    format!("ch-{}", monday.format("%Y-%m-%d")),
                    format!(
                        "Week of {} {}, {}",
                        monday.format("%B"),
                        monday.day(),
                        monday.year()
                    ),
                )
            }
        }
    }
}

/// Build a case-insensitive matcher over the configured redaction words, if any.
fn build_redactor(words: &[String]) -> Option<Regex> {
    let alts: Vec<String> = words
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .map(regex::escape)
        .collect();
    if alts.is_empty() {
        return None;
    }
    Regex::new(&format!("(?i)({})", alts.join("|"))).ok()
}

/// Replace any redacted words in `text` with a block-character mask of equal length.
fn redact(text: &str, redactor: &Option<Regex>) -> String {
    match redactor {
        Some(re) => re
            .replace_all(text, |c: &regex::Captures| "█".repeat(c[0].chars().count()))
            .into_owned(),
        None => text.to_string(),
    }
}

/// Reaction `associated_message_type` → human label. Only "add" types (2000–2005).
fn tapback_label(t: i32) -> Option<&'static str> {
    match t {
        2000 => Some("❤️ Loved"),
        2001 => Some("👍 Liked"),
        2002 => Some("👎 Disliked"),
        2003 => Some("😂 Laughed"),
        2004 => Some("‼️ Emphasized"),
        2005 => Some("❓ Questioned"),
        _ => None,
    }
}

/// The first name (or short label) of whoever placed a reaction, for tapback
/// attribution. `None` when the reactor can't be resolved, so the label degrades to
/// the bare reaction.
fn reactor_name(
    msg: &Message,
    handle_names: &HashMap<i32, String>,
    resolver: &ContactResolver,
) -> Option<String> {
    let full = if msg.is_from_me {
        "You".to_string()
    } else {
        let raw = handle_names.get(&msg.handle_id?)?;
        if raw == ME {
            "Me".to_string()
        } else {
            resolver.display_name(raw)
        }
    };
    // Keep the pill compact: just the first name (Messages does the same).
    Some(full.split_whitespace().next().unwrap_or(&full).to_string())
}

/// Extract the target message GUID from an `associated_message_guid`, which may be
/// prefixed like `p:0/<GUID>` or `bp:<GUID>`.
fn target_guid(assoc: &str) -> &str {
    if let Some(idx) = assoc.rfind('/') {
        &assoc[idx + 1..]
    } else if let Some(idx) = assoc.rfind(':') {
        &assoc[idx + 1..]
    } else {
        assoc
    }
}

/// The system-line text for a group announcement (rename / photo change).
fn announcement_text(msg: &Message, who: &str) -> String {
    match msg.get_announcement() {
        Some(Announcement::NameChange(name)) => {
            format!("{who} named the conversation \u{201C}{name}\u{201D}")
        }
        Some(Announcement::PhotoChange) => format!("{who} changed the group photo"),
        _ => format!("{who} updated the conversation"),
    }
}

/// A label for an expressive send effect ("sent with Slam"), if any.
fn effect_label(msg: &Message) -> Option<String> {
    if !msg.is_expressive() {
        return None;
    }
    let name = match msg.get_expressive() {
        Expressive::Bubble(b) => match b {
            BubbleEffect::Slam => "Slam",
            BubbleEffect::Loud => "Loud",
            BubbleEffect::Gentle => "Gentle",
            BubbleEffect::InvisibleInk => "Invisible Ink",
        },
        Expressive::Screen(s) => match s {
            ScreenEffect::Confetti => "Confetti",
            ScreenEffect::Echo => "Echo",
            ScreenEffect::Fireworks => "Fireworks",
            ScreenEffect::Balloons => "Balloons",
            ScreenEffect::Heart => "Heart",
            ScreenEffect::Lasers => "Lasers",
            ScreenEffect::ShootingStar => "Shooting Star",
            ScreenEffect::Sparkles => "Sparkles",
            ScreenEffect::Spotlight => "Spotlight",
        },
        Expressive::Unknown(_) | Expressive::None => return None,
    };
    Some(format!("sent with {name}"))
}

/// A short label for a non-text app balloon, so these messages aren't rendered blank.
fn app_label(msg: &Message) -> Option<String> {
    match msg.variant() {
        Variant::App(balloon) => Some(match balloon {
            CustomBalloon::URL => "🔗 Link".to_string(),
            CustomBalloon::Handwriting => "✍️ Handwritten".to_string(),
            CustomBalloon::ApplePay => "💸 Apple Pay".to_string(),
            CustomBalloon::Fitness => "🏃 Fitness".to_string(),
            CustomBalloon::Slideshow => "🖼️ Slideshow".to_string(),
            CustomBalloon::CheckIn => "📍 Check In".to_string(),
            CustomBalloon::FindMy => "📍 Find My".to_string(),
            CustomBalloon::Application(bundle) => format!("App: {bundle}"),
        }),
        _ => None,
    }
}

fn service_str(service: Service) -> &'static str {
    match service {
        Service::iMessage => "imessage",
        Service::SMS => "sms",
        _ => "other",
    }
}

fn time_of_day(dt: &DateTime<Local>) -> String {
    let h24 = dt.hour();
    let (h12, ampm) = match h24 {
        0 => (12, "AM"),
        1..=11 => (h24, "AM"),
        12 => (12, "PM"),
        _ => (h24 - 12, "PM"),
    };
    format!("{}:{:02} {}", h12, dt.minute(), ampm)
}

fn day_heading(dt: &DateTime<Local>) -> String {
    format!("{}, {} {}", dt.format("%A"), dt.format("%B"), dt.day())
}

fn gap_marker(prev: &DateTime<Local>, cur: &DateTime<Local>, threshold: i64) -> Option<String> {
    if prev.date_naive() != cur.date_naive() {
        return None; // a new day heading already separates these
    }
    let mins = (*cur - *prev).num_minutes();
    if mins < threshold {
        return None;
    }
    if mins < 60 {
        return Some(format!(
            "{mins} minute{} later",
            if mins == 1 { "" } else { "s" }
        ));
    }
    let hours = mins / 60;
    Some(format!(
        "{hours} hour{} later",
        if hours == 1 { "" } else { "s" }
    ))
}

/// Format an hour-of-day (0–23) as a 12-hour label like "9 PM".
fn hour_label(h: u32) -> String {
    let (h12, ampm) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12} {ampm}")
}

/// A short single-line preview of a message's text, for reply quoting.
fn truncate_preview(text: &str) -> String {
    let one_line = text.replace('\n', " ");
    let trimmed = one_line.trim();
    let mut out: String = trimmed.chars().take(60).collect();
    if trimmed.chars().count() > 60 {
        out.push('…');
    }
    out
}

/// Combine all text bubbles of a message into a single string.
fn message_text(msg: &Message) -> Option<String> {
    let mut parts: Vec<String> = vec![];
    for bubble in msg.body() {
        if let BubbleType::Text(t) = bubble {
            let t = t.trim();
            if !t.is_empty() {
                parts.push(t.to_string());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// A message that will be rendered (already filtered), paired with its timestamp.
struct Dated {
    msg: Message,
    ts: DateTime<Local>,
}

/// A partially-built message: everything computed from the DB, with attachment
/// conversion still pending (`plans`) and the gap marker set later during grouping.
struct Build {
    ts: DateTime<Local>,
    view: MsgView,
    plans: Vec<AttPlan>,
}

fn long_date(dt: &DateTime<Local>) -> String {
    format!("{} {}, {}", dt.format("%B"), dt.day(), dt.year())
}

/// Three-letter month abbreviation for a 1-based month number.
fn month_abbr(m: u32) -> &'static str {
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    MON[(m.clamp(1, 12) - 1) as usize]
}

/// The step after `(year, month)` in calendar order.
fn next_month(y: i32, m: u32) -> (i32, u32) {
    if m == 12 {
        (y + 1, 1)
    } else {
        (y, m + 1)
    }
}

/// True when this attachment is an animated GIF (it carries a separate web source, or
/// its label names a `.gif`).
fn is_gif(a: &crate::model::AttView) -> bool {
    a.web_src.is_some() || a.label.to_lowercase().contains(".gif")
}

/// The middle value of a sorted copy of `v`, or `None` when empty.
fn median(mut v: Vec<i64>) -> Option<i64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

/// Compute the "by the numbers" summary from the built (non-system) messages.
fn compute_stats(builds: &[Build], default_title: &str) -> Option<StatsView> {
    // `builds` is chronological, and filtering preserves order — reply-latency tracking
    // below relies on that.
    let msgs: Vec<&Build> = builds.iter().filter(|b| b.view.system.is_none()).collect();
    let first = msgs.first()?;
    let (mut lo, mut hi) = (first.ts, first.ts);

    let mut sender_counts: HashMap<String, usize> = HashMap::new();
    let mut emoji_counts: HashMap<String, usize> = HashMap::new();
    let mut day_counts: HashMap<NaiveDate, usize> = HashMap::new();
    let mut month_counts: HashMap<(i32, u32), (usize, usize, usize)> = HashMap::new();
    let mut year_counts: HashMap<i32, usize> = HashMap::new();
    let mut hourly = vec![0usize; 24];
    let mut weekday = vec![0usize; 7];
    let mut words = 0usize;
    let mut words_received = 0usize;
    let mut total_words = 0usize;
    let mut text_msgs = 0usize;
    let mut longest_message_words = 0usize;
    let mut photos = 0usize;
    let mut videos = 0usize;
    let mut gifs = 0usize;
    let mut audio = 0usize;
    let mut attachments_total = 0usize;
    let mut links = 0usize;
    let mut from_me = 0usize;

    // Reply latency: the gap whenever the speaker flips, capped at a day so overnight
    // silences and "conversation restarts" don't masquerade as replies.
    let mut responses: Vec<i64> = Vec::new();
    let mut prev: Option<(DateTime<Local>, bool)> = None;

    for b in &msgs {
        if b.view.from_me {
            from_me += 1;
        }
        let label = if b.view.from_me {
            "You".to_string()
        } else {
            b.view
                .sender
                .clone()
                .unwrap_or_else(|| default_title.to_string())
        };
        *sender_counts.entry(label).or_default() += 1;

        if let Some(text) = &b.view.text {
            let w = text.split_whitespace().count();
            total_words += w;
            text_msgs += 1;
            longest_message_words = longest_message_words.max(w);
            if b.view.from_me {
                // "Words sent" only counts your own messages.
                words += w;
            } else {
                words_received += w;
            }
            links += URL_RE.find_iter(text).count();
            // Count emoji by grapheme cluster so multi-codepoint sequences (ZWJ
            // families, flags, skin-tone modifiers) tally as one emoji, not several.
            for g in text.graphemes(true) {
                if EMOJI_CHAR.is_match(g) {
                    *emoji_counts.entry(g.to_string()).or_default() += 1;
                }
            }
        }

        for a in &b.view.attachments {
            attachments_total += 1;
            match a.kind.as_str() {
                "image" if is_gif(a) => gifs += 1,
                "image" => photos += 1,
                "video" => videos += 1,
                "audio" => audio += 1,
                _ => {}
            }
        }

        let date = b.ts.date_naive();
        *day_counts.entry(date).or_default() += 1;
        let entry = month_counts.entry((b.ts.year(), b.ts.month())).or_default();
        entry.0 += 1;
        if b.view.from_me {
            entry.1 += 1;
        } else {
            entry.2 += 1;
        }
        *year_counts.entry(b.ts.year()).or_default() += 1;
        hourly[b.ts.hour() as usize] += 1;
        weekday[b.ts.weekday().num_days_from_sunday() as usize] += 1;

        if let Some((pt, pfrom)) = prev {
            if pfrom != b.view.from_me {
                let mins = (b.ts - pt).num_minutes();
                if (0..=1440).contains(&mins) {
                    responses.push(mins);
                }
            }
        }
        prev = Some((b.ts, b.view.from_me));

        lo = lo.min(b.ts);
        hi = hi.max(b.ts);
    }

    let mut per_sender: Vec<SenderStat> = sender_counts
        .into_iter()
        .map(|(name, count)| SenderStat { name, count })
        .collect();
    per_sender.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));

    let mut top_emoji: Vec<EmojiStat> = emoji_counts
        .into_iter()
        .map(|(emoji, count)| EmojiStat { emoji, count })
        .collect();
    top_emoji.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.emoji.cmp(&b.emoji)));
    top_emoji.truncate(10);

    // Longest run of consecutive calendar days that each had at least one message.
    let mut active_day_list: Vec<NaiveDate> = day_counts.keys().copied().collect();
    active_day_list.sort_unstable();
    let active_days = active_day_list.len() as i64;
    let mut longest_streak = if active_day_list.is_empty() { 0 } else { 1 };
    let mut current = longest_streak;
    for pair in active_day_list.windows(2) {
        if pair[0].succ_opt() == Some(pair[1]) {
            current += 1;
            longest_streak = longest_streak.max(current);
        } else {
            current = 1;
        }
    }

    // Per-active-day counts for the heatmap, in date order.
    let daily: Vec<DayCount> = active_day_list
        .iter()
        .map(|d| DayCount {
            date: d.format("%Y-%m-%d").to_string(),
            count: day_counts[d],
        })
        .collect();

    // Continuous month series from the first to the last month (gaps filled with zero).
    let mut monthly: Vec<MonthStat> = Vec::new();
    let (mut cy, mut cm) = (lo.year(), lo.month());
    let (ey, em) = (hi.year(), hi.month());
    loop {
        let (total, me, others) = month_counts.get(&(cy, cm)).copied().unwrap_or((0, 0, 0));
        monthly.push(MonthStat {
            label: format!("{} {}", month_abbr(cm), cy),
            key: format!("{cy:04}-{cm:02}"),
            total,
            from_me: me,
            from_others: others,
        });
        if (cy, cm) == (ey, em) {
            break;
        }
        // Guard against an inverted range (shouldn't happen for chronological input).
        if monthly.len() > 10_000 {
            break;
        }
        (cy, cm) = next_month(cy, cm);
    }

    let mut yearly: Vec<YearStat> = year_counts
        .into_iter()
        .map(|(year, total)| YearStat { year, total })
        .collect();
    yearly.sort_by_key(|y| y.year);

    let busiest_day = active_day_list
        .iter()
        .max_by_key(|d| day_counts[*d])
        .map(|d| format!("{}, {} {}", d.format("%A"), d.format("%B"), d.day()));
    // Break ties toward the earlier hour so the label is stable across runs.
    let busiest_hour = hourly
        .iter()
        .enumerate()
        .filter(|(_, c)| **c > 0)
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(h, _)| hour_label(h as u32));

    let avg_words = if text_msgs > 0 {
        (total_words as f64 / text_msgs as f64 * 10.0).round() / 10.0
    } else {
        0.0
    };

    Some(StatsView {
        total: msgs.len(),
        from_me,
        from_others: msgs.len() - from_me,
        per_sender,
        top_emoji,
        words,
        words_received,
        avg_words,
        longest_message_words,
        photos,
        videos,
        gifs,
        audio,
        links,
        attachments_total,
        median_response_minutes: median(responses),
        first_date: long_date(&lo),
        last_date: long_date(&hi),
        days: (hi.date_naive() - lo.date_naive()).num_days() + 1,
        active_days,
        busiest_day,
        longest_streak,
        busiest_hour,
        hourly,
        weekday,
        monthly,
        yearly,
        daily,
    })
}

/// The indices of a `len`-length, date-ascending list to keep when evenly sampling
/// down to `n` items. `n == 0` or `len <= n` keeps everything.
fn sample_indices(len: usize, n: usize) -> Vec<usize> {
    if n == 0 || len <= n {
        return (0..len).collect();
    }
    let step = len as f64 / n as f64;
    (0..n).map(|i| (i as f64 * step) as usize).collect()
}

/// Apply date range, then even sampling, then a plain limit (all by index, so no
/// clone of the non-`Clone` `Message` is required). Sampling and limit compose:
/// `--sample` thins the list first, then `--limit` caps whatever remains.
fn apply_subset(mut msgs: Vec<Dated>, subset: &Subset) -> Vec<Dated> {
    if let Some(from) = subset.from {
        msgs.retain(|d| d.ts.date_naive() >= from);
    }
    if let Some(to) = subset.to {
        msgs.retain(|d| d.ts.date_naive() <= to);
    }
    if let Some(n) = subset.sample {
        let keep: std::collections::HashSet<usize> =
            sample_indices(msgs.len(), n).into_iter().collect();
        if keep.len() < msgs.len() {
            msgs = msgs
                .into_iter()
                .enumerate()
                .filter(|(i, _)| keep.contains(i))
                .map(|(_, d)| d)
                .collect();
        }
    }
    if let Some(limit) = subset.limit {
        msgs.truncate(limit);
    }
    msgs
}

/// Build the full book view.
///
/// `messages` must be in chronological order. `handle_names` maps `handle_id` to a
/// raw contact identifier (from [`crate::db::contacts::handle_map`]).
#[allow(clippy::too_many_arguments)]
pub fn build_book(
    db: &Connection,
    mut messages: Vec<Message>,
    offset: i64,
    is_group: bool,
    handle_names: &HashMap<i32, String>,
    resolver: &ContactResolver,
    config: &Config,
    default_title: &str,
    processor: &Processor,
    subset: &Subset,
) -> Result<BookView> {
    // Populate `text` from attributedBody up front. Modern messages store their text
    // only in attributedBody, so this must run before pass 1 reads `msg.text` to build
    // the reply-preview map — otherwise replies to such messages lose their quote.
    for msg in &mut messages {
        let _ = msg.gen_text(db);
    }

    let redactor = build_redactor(&config.privacy.redact);
    // Handle rowids whose messages should be omitted entirely (privacy).
    let excluded: std::collections::HashSet<i32> = {
        let wanted: std::collections::HashSet<String> = config
            .privacy
            .exclude_handles
            .iter()
            .map(|h| normalize(h))
            .collect();
        handle_names
            .iter()
            .filter(|(_, raw)| wanted.contains(&normalize(raw)))
            .map(|(id, _)| *id)
            .collect()
    };
    let is_excluded = |msg: &Message| -> bool {
        !msg.is_from_me && msg.handle_id.is_some_and(|h| excluded.contains(&h))
    };

    // Pass 1: collect tapbacks/stickers keyed by their target GUID, and build a map of
    // every message's GUID to a short text preview (for rendering reply context).
    let mut tapbacks: HashMap<String, Vec<String>> = HashMap::new();
    let mut previews: HashMap<String, String> = HashMap::new();
    for msg in &messages {
        if is_excluded(msg) {
            continue;
        }
        if msg.is_reaction() {
            if let Some(assoc) = msg.associated_message_guid.as_deref() {
                let base = msg
                    .associated_message_type
                    .and_then(tapback_label)
                    .map(str::to_string)
                    // A sticker placed on a message is a reaction with no tapback label;
                    // surface it rather than dropping it entirely.
                    .or_else(|| msg.is_sticker().then(|| "🩷 Sticker".to_string()));
                if let Some(base) = base {
                    // Attribute the reaction to whoever placed it, so a reader can tell
                    // who reacted (Messages shows this on the bubble in group chats).
                    let who = reactor_name(msg, handle_names, resolver);
                    let label = match who {
                        Some(name) if !name.is_empty() => {
                            format!("{base} \u{00B7} {name}")
                        }
                        _ => base,
                    };
                    tapbacks
                        .entry(target_guid(assoc).to_string())
                        .or_default()
                        .push(label);
                }
            }
            continue;
        }
        if let Some(t) = msg.text.as_deref() {
            let t = t.trim();
            if !t.is_empty() {
                previews.insert(msg.guid.clone(), truncate_preview(&redact(t, &redactor)));
            }
        }
    }

    // Pass 2: keep displayable messages (reactions/shareplay are folded in elsewhere or
    // dropped); group announcements are kept and rendered as system lines.
    let mut dated: Vec<Dated> = Vec::new();
    for msg in messages {
        if msg.is_reaction() || msg.is_shareplay() || is_excluded(&msg) {
            continue;
        }
        match msg.date(&offset) {
            Ok(ts) => dated.push(Dated { msg, ts }),
            Err(e) => eprintln!("  (skipping message with unreadable date: {e})"),
        }
    }

    let working = apply_subset(dated, subset);

    // Phase 3a (sequential, DB-bound): compute each message's view fields and resolve
    // its attachments into conversion plans.
    let mut builds: Vec<Build> = Vec::with_capacity(working.len());
    for d in working {
        let ts = d.ts;

        // Group announcements become centered system lines rather than bubbles.
        if d.msg.is_announcement() {
            let who = if d.msg.is_from_me {
                "You".to_string()
            } else {
                d.msg
                    .handle_id
                    .and_then(|h| handle_names.get(&h))
                    .map(|raw| {
                        if raw == ME {
                            "Me".to_string()
                        } else {
                            resolver.display_name(raw)
                        }
                    })
                    .unwrap_or_else(|| "Someone".to_string())
            };
            let view = MsgView {
                time: time_of_day(&ts),
                system: Some(announcement_text(&d.msg, &who)),
                ..Default::default()
            };
            builds.push(Build {
                ts,
                view,
                plans: Vec::new(),
            });
            continue;
        }

        let sender = if is_group && !d.msg.is_from_me {
            d.msg.handle_id.and_then(|h| {
                handle_names.get(&h).map(|raw| {
                    if raw == ME {
                        "Me".to_string()
                    } else {
                        resolver.display_name(raw)
                    }
                })
            })
        } else {
            None
        };

        let reply = d.msg.is_reply();
        let reply_to = if reply {
            d.msg
                .thread_originator_guid
                .as_deref()
                .and_then(|g| previews.get(target_guid(g)).cloned())
        } else {
            None
        };

        let view = MsgView {
            from_me: d.msg.is_from_me,
            sender,
            time: time_of_day(&ts),
            gap_before: None, // set during grouping below
            text: message_text(&d.msg).map(|t| redact(&t, &redactor)),
            attachments: Vec::new(), // filled in phase 3b
            tapbacks: tapbacks.get(&d.msg.guid).cloned().unwrap_or_default(),
            reply,
            reply_to,
            edited: d.msg.is_edited(),
            app: app_label(&d.msg),
            effect: effect_label(&d.msg),
            system: None,
            service: service_str(d.msg.service()).to_string(),
        };
        let plans = if config.privacy.hide_attachments {
            Vec::new()
        } else {
            processor.plan_message(db, &d.msg)
        };
        builds.push(Build { ts, view, plans });
    }

    // Phase 3b (parallel, no DB): run the sips/ffmpeg conversions across messages.
    builds.par_iter_mut().for_each(|b| {
        let plans = std::mem::take(&mut b.plans);
        b.view.attachments = plans.into_iter().map(AttPlan::finalize).collect();
    });

    // Derived aggregates computed before the grouping loop consumes `builds`.
    let stats = compute_stats(&builds, default_title);
    let gallery: Vec<String> = if config.gallery {
        builds
            .iter()
            .flat_map(|b| b.view.attachments.iter())
            .filter(|a| a.kind == "image")
            .filter_map(|a| a.src.clone())
            .collect()
    } else {
        Vec::new()
    };

    // Phase 3c (sequential): group into chapters + day sections and set gaps.
    let granularity = Granularity::parse(config.format.chapters.as_deref());
    let gap_threshold = config.format.gap_minutes.unwrap_or(GAP_THRESHOLD_MINUTES);
    let mut chapters: Vec<ChapterView> = Vec::new();
    let mut message_count = 0usize;
    let mut prev_ts: Option<DateTime<Local>> = None;

    for b in builds {
        let ts = b.ts;
        let (chapter_id, chapter_title) = granularity.chapter_key(&ts);
        let heading = day_heading(&ts);

        if chapters.last().map(|c| c.id.as_str()) != Some(chapter_id.as_str()) {
            chapters.push(ChapterView {
                title: chapter_title,
                id: chapter_id,
                days: Vec::new(),
            });
            prev_ts = None; // reset gap tracking across chapters
        }
        let chapter = chapters.last_mut().unwrap();

        if chapter.days.last().map(|day| day.heading.as_str()) != Some(heading.as_str()) {
            chapter.days.push(DayView {
                heading,
                messages: Vec::new(),
            });
        }
        let day = chapter.days.last_mut().unwrap();

        let mut view = b.view;
        if view.system.is_none() {
            view.gap_before = prev_ts
                .as_ref()
                .and_then(|p| gap_marker(p, &ts, gap_threshold));
            message_count += 1;
        }
        day.messages.push(view);
        prev_ts = Some(ts);
    }

    let title = config
        .title
        .clone()
        .unwrap_or_else(|| default_title.to_string());

    let default_theme = ThemeView::default();
    let theme = ThemeView {
        me_color: config
            .theme
            .me_color
            .as_deref()
            .map(normalize_hex)
            .unwrap_or(default_theme.me_color),
        them_color: config
            .theme
            .them_color
            .as_deref()
            .map(normalize_hex)
            .unwrap_or(default_theme.them_color),
        sms_color: config
            .theme
            .sms_color
            .as_deref()
            .map(normalize_hex)
            .unwrap_or(default_theme.sms_color),
        meta_color: config
            .theme
            .meta_color
            .as_deref()
            .map(normalize_hex)
            .unwrap_or(default_theme.meta_color),
        font: config.theme.font.clone(),
    };
    let (width, height, margin) = config.page.dimensions();

    Ok(BookView {
        title,
        author: config.author.clone(),
        dedication: config.dedication.clone(),
        is_group,
        chapters,
        message_count,
        emoji_font: config.emoji_font.clone(),
        cover: None, // set by the caller once the cover image is processed
        stats,
        gallery,
        theme,
        page: PageView {
            width,
            height,
            margin,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2020, 11, 2, h, m, 0).unwrap()
    }

    #[test]
    fn twelve_hour_time() {
        assert_eq!(time_of_day(&at(0, 5)), "12:05 AM");
        assert_eq!(time_of_day(&at(9, 0)), "9:00 AM");
        assert_eq!(time_of_day(&at(12, 30)), "12:30 PM");
        assert_eq!(time_of_day(&at(15, 42)), "3:42 PM");
    }

    #[test]
    fn gap_only_for_long_same_day_silences() {
        let t = GAP_THRESHOLD_MINUTES;
        assert_eq!(gap_marker(&at(9, 0), &at(9, 30), t), None); // short
        assert_eq!(
            gap_marker(&at(9, 0), &at(12, 30), t).as_deref(),
            Some("3 hours later")
        );
        assert_eq!(
            gap_marker(&at(9, 0), &at(10, 0), t).as_deref(),
            Some("1 hour later")
        );
        // Different day => no gap marker (a day heading separates them).
        let next_day = Local.with_ymd_and_hms(2020, 11, 3, 8, 0, 0).unwrap();
        assert_eq!(gap_marker(&at(23, 0), &next_day, t), None);
    }

    #[test]
    fn gap_minutes_tier_with_low_threshold() {
        // A sub-hour threshold enables minute-granularity markers.
        assert_eq!(
            gap_marker(&at(9, 0), &at(9, 20), 15).as_deref(),
            Some("20 minutes later")
        );
        assert_eq!(gap_marker(&at(9, 0), &at(9, 10), 15), None); // below threshold
        assert_eq!(
            gap_marker(&at(9, 0), &at(11, 0), 15).as_deref(),
            Some("2 hours later")
        );
    }

    #[test]
    fn chapter_keys_by_granularity() {
        let ts = at(15, 0); // Monday, November 2, 2020
        assert_eq!(Granularity::Month.chapter_key(&ts).0, "ch-2020-11");
        assert_eq!(Granularity::Month.chapter_key(&ts).1, "November 2020");
        assert_eq!(
            Granularity::Year.chapter_key(&ts),
            ("ch-2020".to_string(), "2020".to_string())
        );
        // The week chapter is keyed by its Monday.
        assert_eq!(Granularity::Week.chapter_key(&ts).0, "ch-2020-11-02");
        assert_eq!(
            Granularity::Week.chapter_key(&ts).1,
            "Week of November 2, 2020"
        );
        assert_eq!(Granularity::parse(Some("YEAR")), Granularity::Year);
        assert_eq!(Granularity::parse(None), Granularity::Month);
    }

    #[test]
    fn redactor_masks_case_insensitively() {
        let re = build_redactor(&["Secret".to_string(), "".to_string()]);
        assert!(re.is_some());
        assert_eq!(redact("my SECRET plan", &re), "my ██████ plan");
        assert_eq!(redact("nothing here", &re), "nothing here");
        // Empty/whitespace-only word lists produce no matcher.
        assert!(build_redactor(&["   ".to_string()]).is_none());
        assert_eq!(redact("untouched", &None), "untouched");
    }

    #[test]
    fn sample_indices_even_and_bounded() {
        assert_eq!(sample_indices(10, 3), vec![0, 3, 6]);
        // Sampling more than we have keeps everything, in order.
        assert_eq!(sample_indices(5, 10), vec![0, 1, 2, 3, 4]);
        // n == 0 is a no-op (keep all).
        assert_eq!(sample_indices(5, 0), vec![0, 1, 2, 3, 4]);
        assert_eq!(sample_indices(4, 4), vec![0, 1, 2, 3]);
    }

    #[test]
    fn day_heading_format() {
        assert_eq!(day_heading(&at(15, 0)), "Monday, November 2");
    }

    #[test]
    fn tapback_labels_only_for_adds() {
        assert_eq!(tapback_label(2000), Some("❤️ Loved"));
        assert_eq!(tapback_label(2003), Some("😂 Laughed"));
        assert_eq!(tapback_label(3000), None); // removal
        assert_eq!(tapback_label(1000), None); // sticker
    }

    #[test]
    fn preview_truncates_and_single_lines() {
        assert_eq!(truncate_preview("hi\nthere"), "hi there");
        let long = "a".repeat(80);
        let p = truncate_preview(&long);
        assert!(p.ends_with('…'));
        assert_eq!(p.chars().count(), 61); // 60 chars + ellipsis
    }

    #[test]
    fn extracts_target_guid_from_prefixes() {
        assert_eq!(target_guid("p:0/ABC-123"), "ABC-123");
        assert_eq!(target_guid("bp:ABC-123"), "ABC-123");
        assert_eq!(target_guid("ABC-123"), "ABC-123");
    }
}
