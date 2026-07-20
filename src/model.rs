//! Backend-agnostic view model.
//!
//! The DB layer produces raw `imessage_database` rows; [`assemble`](crate::assemble)
//! turns those into a [`BookView`], and the HTML, LaTeX, and EPUB renderers consume the
//! exact same [`BookView`] through minijinja templates. Text is kept raw here — each
//! backend escapes it via a template filter (HTML autoescape, or the `tex` filter).

use serde::Serialize;

/// The whole book: metadata plus month chapters.
#[derive(Debug, Default, Serialize)]
pub struct BookView {
    pub title: String,
    pub author: Option<String>,
    pub dedication: Option<String>,
    /// True when the conversation has more than two participants; controls whether
    /// sender names are shown.
    pub is_group: bool,
    pub chapters: Vec<ChapterView>,
    /// Total messages rendered (after any subset filtering), for status output.
    pub message_count: usize,
    /// Optional emoji font name for the LaTeX build (e.g. "Apple Color Emoji").
    pub emoji_font: Option<String>,
    /// Relative path to a cover image on the title page, if configured.
    pub cover: Option<String>,
    /// "By the numbers" summary, rendered as front matter.
    pub stats: Option<StatsView>,
    /// Relative paths of every embedded image, for the optional gallery appendix.
    pub gallery: Vec<String>,
    /// Bubble colors and fonts.
    pub theme: ThemeView,
    /// Page geometry for the PDF.
    pub page: PageView,
}

/// One calendar month, e.g. "November 2020".
#[derive(Debug, Default, Serialize)]
pub struct ChapterView {
    pub title: String,
    /// Stable slug used for LaTeX `\include` filenames and HTML anchors, e.g. `ch-2020-11`.
    pub id: String,
    pub days: Vec<DayView>,
}

/// One calendar day within a chapter.
#[derive(Debug, Default, Serialize)]
pub struct DayView {
    /// e.g. "Monday, November 2".
    pub heading: String,
    pub messages: Vec<MsgView>,
}

/// A single rendered message bubble (or a system line when `system` is set).
#[derive(Debug, Default, Serialize)]
pub struct MsgView {
    pub from_me: bool,
    /// Display name of the sender for group chats; `None` for 1:1 or from-me.
    pub sender: Option<String>,
    /// Time of day, e.g. "3:42 PM".
    pub time: String,
    /// A human "… later" marker shown before this bubble when there was a long
    /// silence, e.g. "3 hours later". `None` when messages are close together.
    pub gap_before: Option<String>,
    /// Raw (unescaped) message text; may contain newlines.
    pub text: Option<String>,
    pub attachments: Vec<AttView>,
    /// Rendered tapbacks such as "❤️ Loved" attached to this message.
    pub tapbacks: Vec<String>,
    /// Whether this message is a threaded reply.
    pub reply: bool,
    /// A short preview of the message this one replies to, when it can be resolved.
    pub reply_to: Option<String>,
    /// Whether the message was edited after sending.
    pub edited: bool,
    /// A label for a non-text app balloon (URL preview, Apple Pay, etc.), if any.
    pub app: Option<String>,
    /// An expressive-effect label such as "sent with Slam", if any.
    pub effect: Option<String>,
    /// When set, this entry is a centered system line (a group announcement) rather
    /// than a bubble, and the other fields are ignored.
    pub system: Option<String>,
    /// "imessage" | "sms" | "other" — drives bubble color.
    pub service: String,
}

/// The kind of an attachment, used for choosing how to render it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttKind {
    Image,
    Video,
    Audio,
    Other,
}

impl AttKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AttKind::Image => "image",
            AttKind::Video => "video",
            AttKind::Audio => "audio",
            AttKind::Other => "other",
        }
    }
}

/// A processed attachment ready for embedding.
#[derive(Debug, Default, Serialize)]
pub struct AttView {
    /// "image" | "video" | "audio" | "other".
    pub kind: String,
    /// Path relative to the output directory of an embeddable file (converted if
    /// needed). For video this is the poster frame; for a GIF this is a static JPEG.
    /// `None` means fall back to the labeled placeholder.
    pub src: Option<String>,
    /// Path to a browser-playable video file (the original, copied), when available.
    /// Only the HTML preview uses this — print backends embed `src` (the poster).
    pub video_src: Option<String>,
    /// Path to an animated / web-preferred image source (e.g. the original GIF), when it
    /// differs from `src`. The HTML preview shows this so GIFs animate; print backends
    /// keep the static `src`.
    pub web_src: Option<String>,
    /// Fallback label, e.g. "IMG_1234.HEIC · 2.3 MB".
    pub label: String,
    /// Extra caption, e.g. a video's duration or a "▶" hint.
    pub caption: Option<String>,
}

/// "By the numbers" statistics for the front matter.
#[derive(Debug, Default, Serialize)]
pub struct StatsView {
    pub total: usize,
    pub from_me: usize,
    pub from_others: usize,
    pub per_sender: Vec<SenderStat>,
    pub top_emoji: Vec<EmojiStat>,
    /// Words in your own messages.
    pub words: usize,
    /// Words in messages you received.
    pub words_received: usize,
    /// Average words per text message across everyone.
    pub avg_words: f64,
    /// Word count of the single longest text message.
    pub longest_message_words: usize,
    pub photos: usize,
    /// Video attachments sent/received.
    pub videos: usize,
    /// GIF attachments (counted separately from still photos).
    pub gifs: usize,
    /// Audio/voice attachments.
    pub audio: usize,
    /// Links (URLs) shared across all messages.
    pub links: usize,
    /// Total attachments of every kind.
    pub attachments_total: usize,
    /// Typical reply latency (median minutes to respond, within a day), if computable.
    pub median_response_minutes: Option<i64>,
    pub first_date: String,
    pub last_date: String,
    pub days: i64,
    /// Active days (days with at least one message).
    pub active_days: i64,
    pub busiest_day: Option<String>,
    /// Longest run of consecutive calendar days with at least one message.
    pub longest_streak: i64,
    /// The hour of day with the most messages, e.g. "9 PM".
    pub busiest_hour: Option<String>,
    /// Messages by hour of day (0–23) — the "when do you text" histogram.
    pub hourly: Vec<usize>,
    /// Messages by weekday (index 0 = Sunday … 6 = Saturday).
    pub weekday: Vec<usize>,
    /// Per-month totals across the whole span (gaps filled with zero) for the trend chart.
    pub monthly: Vec<MonthStat>,
    /// Per-year totals, for a coarse multi-year trend.
    pub yearly: Vec<YearStat>,
    /// Per active-day message counts, for the activity heatmap.
    pub daily: Vec<DayCount>,
}

/// One month in the message-volume trend.
#[derive(Debug, Default, Serialize)]
pub struct MonthStat {
    /// Human label, e.g. "Nov 2020".
    pub label: String,
    /// Sortable key, e.g. "2020-11".
    pub key: String,
    pub total: usize,
    pub from_me: usize,
    pub from_others: usize,
}

/// One year in the message-volume trend.
#[derive(Debug, Default, Serialize)]
pub struct YearStat {
    pub year: i32,
    pub total: usize,
}

/// A single day's message count, for the activity heatmap.
#[derive(Debug, Default, Serialize)]
pub struct DayCount {
    /// ISO date, e.g. "2020-11-02".
    pub date: String,
    pub count: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct SenderStat {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct EmojiStat {
    pub emoji: String,
    pub count: usize,
}

/// Bubble colors (hex, no leading `#`) and optional main font.
#[derive(Debug, Serialize)]
pub struct ThemeView {
    pub me_color: String,
    pub them_color: String,
    pub sms_color: String,
    pub meta_color: String,
    /// Main text font for the PDF (`\setmainfont`), if set.
    pub font: Option<String>,
}

impl Default for ThemeView {
    fn default() -> Self {
        ThemeView {
            me_color: "0B93F6".to_string(),
            them_color: "E5E5EA".to_string(),
            sms_color: "34C759".to_string(),
            meta_color: "8E8E93".to_string(),
            font: None,
        }
    }
}

/// PDF page geometry (all values are LaTeX lengths like "6in").
#[derive(Debug, Serialize)]
pub struct PageView {
    pub width: String,
    pub height: String,
    pub margin: String,
}

impl Default for PageView {
    fn default() -> Self {
        PageView {
            width: "8.5in".to_string(),
            height: "11in".to_string(),
            margin: "0.85in".to_string(),
        }
    }
}
