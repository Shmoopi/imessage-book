//! `book.toml` configuration.
//!
//! Users configure the book's front matter and contact display names in a `book.toml`
//! file (auto-discovered in the current directory) instead of editing templates by hand.
//! Everything is optional; sensible defaults are used when the file is absent.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub title: Option<String>,
    pub author: Option<String>,
    pub dedication: Option<String>,
    /// Map from a raw handle identifier (phone/email) OR "me" to a friendly display
    /// name. Used for group-chat sender labels and the book title fallback.
    pub names: HashMap<String, String>,
    /// Font used for emoji glyphs in the PDF. When unset, "Noto Emoji" is used if
    /// installed. Note: the XeTeX/Tectonic engine renders emoji monochrome regardless
    /// of the font (it cannot rasterize color-emoji tables), so this mainly selects the
    /// mono glyph style. The HTML preview always shows the browser's native color emoji.
    pub emoji_font: Option<String>,
    /// Path to a cover image for the title page.
    pub cover_image: Option<String>,
    /// Include a gallery appendix with every embedded image.
    pub gallery: bool,
    /// Page geometry for the PDF.
    pub page: PageConfig,
    /// Bubble colors and fonts.
    pub theme: ThemeConfig,
    /// Layout/formatting knobs (gap threshold, chapter granularity).
    pub format: FormatConfig,
    /// Privacy controls (handle exclusion, keyword redaction, attachment hiding).
    pub privacy: PrivacyConfig,
}

/// Layout knobs that don't fit the theme or page geometry.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct FormatConfig {
    /// Minimum same-day silence (minutes) before a "… later" marker is shown.
    /// Defaults to 60. Values below 60 enable minute-granularity markers.
    pub gap_minutes: Option<i64>,
    /// How messages are grouped into chapters: "month" (default), "year", or "week".
    pub chapters: Option<String>,
}

/// Privacy controls applied while assembling the book.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {
    /// Raw handle identifiers (phone/email) whose messages are omitted entirely.
    /// Matched leniently (last-10-digits for phones, case-insensitive for emails).
    pub exclude_handles: Vec<String>,
    /// Case-insensitive words/phrases to mask (with █) wherever they appear in text.
    pub redact: Vec<String>,
    /// Drop every attachment (no images, no placeholders) — useful for text-only books.
    pub hide_attachments: bool,
}

/// Page geometry. `size` is a preset name; `margin_in` is the margin in inches.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PageConfig {
    /// One of: letter, a4, a5, 6x9, 5x8, 7x10, 8.5x11 (case-insensitive).
    pub size: Option<String>,
    pub margin_in: Option<f64>,
}

impl PageConfig {
    /// Resolve to (width, height, margin) as LaTeX lengths.
    pub fn dimensions(&self) -> (String, String, String) {
        let (w, h) = match self.size.as_deref().map(str::to_lowercase).as_deref() {
            Some("a4") => (8.27, 11.69),
            Some("a5") => (5.83, 8.27),
            Some("6x9") => (6.0, 9.0),
            Some("5x8") => (5.0, 8.0),
            Some("7x10") => (7.0, 10.0),
            Some("letter") | Some("8.5x11") | None => (8.5, 11.0),
            Some(_) => (8.5, 11.0),
        };
        let margin = self.margin_in.unwrap_or(0.85);
        (format!("{w}in"), format!("{h}in"), format!("{margin}in"))
    }
}

/// Bubble colors (hex, with or without a leading `#`) and an optional main font.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub me_color: Option<String>,
    pub them_color: Option<String>,
    pub sms_color: Option<String>,
    pub meta_color: Option<String>,
    pub font: Option<String>,
}

/// Normalize a user-provided hex color to six digits without a leading `#`.
pub fn normalize_hex(s: &str) -> String {
    s.trim().trim_start_matches('#').to_uppercase()
}

/// A fully-commented starter `book.toml`, written by `imessage-book init`. Kept here (not
/// in the binary) so a unit test can prove it always parses into a valid [`Config`].
pub const STARTER_TOML: &str = r#"# imessage-book configuration. Every field is optional.

title = "Our Conversation"
author = "Your Name"
dedication = "Dedicated to you."
# cover_image = "~/Pictures/cover.jpg"   # shown on the title page
gallery = false                          # append a grid of every photo

# emoji_font = "Noto Emoji"

[page]
size = "6x9"        # letter | a4 | a5 | 6x9 | 5x8 | 7x10 | 8.5x11
margin_in = 0.75

[theme]
me_color = "0B93F6"
them_color = "E5E5EA"
sms_color = "34C759"
# font = "Palatino"

[format]
gap_minutes = 60    # minimum same-day silence before a "… later" marker
chapters = "month"  # month | year | week

[privacy]
exclude_handles = []   # e.g. ["+15550000000"] — omit these people's messages
redact = []            # e.g. ["password"] — mask these words wherever they appear
hide_attachments = false

# Friendly names for group-chat participants, keyed by phone/email.
[names]
# "+15555555555" = "Naomi"
"#;

impl Config {
    /// Load configuration, preferring an explicit path when given.
    ///
    /// With `Some(path)` the file must exist and parse (a missing explicit config is an
    /// error, so typos surface). With `None` it falls back to auto-discovering
    /// `book.toml` in the current directory.
    pub fn load(explicit: Option<&Path>) -> Result<Config> {
        match explicit {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("reading config {}", path.display()))?;
                toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))
            }
            None => Self::load_from_dir(Path::new(".")),
        }
    }

    /// Load `book.toml` from `dir` if present, otherwise return defaults.
    pub fn load_from_dir(dir: &Path) -> Result<Config> {
        let path = dir.join("book.toml");
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let config: Config =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(config)
    }

    /// Friendly name for a raw handle id, falling back to the id itself.
    pub fn display_name(&self, handle_id: &str) -> String {
        self.names
            .get(handle_id)
            .cloned()
            .unwrap_or_else(|| handle_id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_hex_strips_and_uppercases() {
        assert_eq!(normalize_hex("#0b93f6"), "0B93F6");
        assert_eq!(normalize_hex("  e5e5ea "), "E5E5EA");
        assert_eq!(normalize_hex("34C759"), "34C759");
    }

    #[test]
    fn page_dimensions_presets_and_default() {
        assert_eq!(
            PageConfig {
                size: Some("6x9".into()),
                margin_in: Some(0.75)
            }
            .dimensions(),
            ("6in".to_string(), "9in".to_string(), "0.75in".to_string())
        );
        // Case-insensitive preset lookup.
        assert_eq!(
            PageConfig {
                size: Some("A4".into()),
                margin_in: None
            }
            .dimensions(),
            (
                "8.27in".to_string(),
                "11.69in".to_string(),
                "0.85in".to_string()
            )
        );
        // Unknown or absent size falls back to US letter with the default margin.
        assert_eq!(
            PageConfig::default().dimensions(),
            (
                "8.5in".to_string(),
                "11in".to_string(),
                "0.85in".to_string()
            )
        );
        assert_eq!(
            PageConfig {
                size: Some("nonsense".into()),
                margin_in: Some(1.0)
            }
            .dimensions(),
            ("8.5in".to_string(), "11in".to_string(), "1in".to_string())
        );
    }

    #[test]
    fn starter_toml_parses() {
        let config: Config = toml::from_str(STARTER_TOML).expect("starter book.toml must parse");
        assert_eq!(config.title.as_deref(), Some("Our Conversation"));
        assert_eq!(config.format.chapters.as_deref(), Some("month"));
        assert!(config.privacy.exclude_handles.is_empty());
    }

    #[test]
    fn parses_format_and_privacy_sections() {
        let toml = r#"
            title = "Book"
            [format]
            gap_minutes = 30
            chapters = "week"
            [privacy]
            exclude_handles = ["+15550000000"]
            redact = ["secret"]
            hide_attachments = true
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.format.gap_minutes, Some(30));
        assert_eq!(config.format.chapters.as_deref(), Some("week"));
        assert_eq!(
            config.privacy.exclude_handles,
            vec!["+15550000000".to_string()]
        );
        assert_eq!(config.privacy.redact, vec!["secret".to_string()]);
        assert!(config.privacy.hide_attachments);
    }
}
